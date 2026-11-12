use super::store::{Store, StoreError};
use crate::domain::{
    cognition::*,
    history::{Event, IndividualState, Memory},
};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

type ContextRunRow = (Uuid, Uuid, String, i64, i64, i64, Option<Uuid>);

pub(super) async fn search(
    connection: &mut PgConnection,
    query: &MemoryQuery,
) -> Result<Vec<Memory>, StoreError> {
    if query.text.chars().count() > 1000 {
        return Err(StoreError::Invalid("query size"));
    }
    let terms = query_terms(&query.text);
    let rows:Vec<Value>=sqlx::query_scalar(r#"
WITH RECURSIVE denied(individual_id,id) AS (
 SELECT individual_id,id FROM objects WHERE blocked UNION
 SELECT d.individual_id,d.target_id FROM derivation_edges d JOIN denied b ON (b.individual_id,b.id)=(d.individual_id,d.source_id)
)
SELECT to_jsonb(m) || jsonb_build_object('source_event_ids',
 ARRAY(SELECT event_id FROM memory_sources s WHERE (s.individual_id,s.memory_id)=(m.individual_id,m.id) ORDER BY event_id))
FROM memories m JOIN objects o ON (o.individual_id,o.id)=(m.individual_id,m.id)
WHERE NOT o.blocked AND m.validity='active' AND m.valid_from<=$1 AND (m.valid_until IS NULL OR m.valid_until>$1)
 AND (m.scope='general' OR m.scope=$2)
 AND NOT EXISTS(SELECT 1 FROM denied d WHERE d.individual_id=m.individual_id AND d.id IN (m.id,m.individual_id))
 AND ($3::uuid IS NULL OR EXISTS(SELECT 1 FROM memory_sources ms JOIN relationship_events re
     ON (ms.individual_id,ms.event_id)=(re.individual_id,re.event_id)
     WHERE (ms.individual_id,ms.memory_id)=(m.individual_id,m.id) AND re.subject_id=$3))
ORDER BY (m.scope=$2) DESC NULLS LAST,
 (SELECT count(*) FROM unnest($4::text[]) term WHERE strpos(lower(m.content),term)>0) DESC,
 m.created_at DESC,m.id
LIMIT $5
"#).bind(query.at).bind(&query.scope).bind(query.subject_id).bind(terms).bind(i64::from(query.limit.clamp(1,20)))
        .fetch_all(connection).await?;
    rows.into_iter()
        .map(|v| serde_json::from_value(v).map_err(StoreError::from))
        .collect()
}

impl Store {
    pub async fn search_memories(&self, query: &MemoryQuery) -> Result<Vec<Memory>, StoreError> {
        search(&mut *self.pool.acquire().await?, query).await
    }

    /// Capture a consistent snapshot. The leased runner must persist the returned manifest
    /// before a model request, then recheck versions/epoch on commit (M1-05).
    pub async fn model_context(
        &self,
        run_id: Uuid,
        capabilities: Vec<CapabilityInfo>,
    ) -> Result<ModelContext, StoreError> {
        let base = self
            .model_context_snapshot(run_id, capabilities.clone(), None)
            .await?;
        let trigger = base.events.iter().find(|e| e.id == base.current_event_id);
        let text = trigger
            .and_then(|e| e.payload["text"].as_str())
            .or_else(|| {
                base.goals
                    .iter()
                    .find(|g| Some(g.id) == base.current_goal_id)
                    .map(|g| g.description.as_str())
            })
            .or_else(|| trigger.and_then(|e| e.payload["title"].as_str()))
            .unwrap_or("");
        if text.trim().is_empty() {
            return Ok(base);
        }
        let mut selection = self
            .retrieve_memories(&crate::domain::retrieval::RetrievalRequest {
                text: text.chars().take(1000).collect(),
                scope: Some(base.current_scope.clone()),
                subject_id: None,
                person_id: None,
                person_name: None,
                at: base.now,
                include_history: false,
                limit: 10,
            })
            .await?;
        selection.summary.query_limited = text.chars().count() > 1000;
        if (
            selection.individual,
            selection.state_version,
            selection.privacy_epoch,
        ) != (base.individual_id, base.state_version, base.privacy_epoch)
        {
            return Err(StoreError::StaleState);
        }
        self.model_context_snapshot(run_id, capabilities, Some(&selection))
            .await
    }
    async fn model_context_snapshot(
        &self,
        run_id: Uuid,
        capabilities: Vec<CapabilityInfo>,
        selected: Option<&crate::domain::retrieval::RetrievalPage>,
    ) -> Result<ModelContext, StoreError> {
        let privacy_generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let mut connection = self.pool.acquire().await?;
        let mut tx = connection.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let current: Value =
            sqlx::query_scalar("SELECT to_jsonb(i) FROM individuals i JOIN objects o ON (o.individual_id,o.id)=(i.id,i.id) WHERE singleton AND NOT o.blocked")
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotInitialized)?;
        let current: IndividualState = serde_json::from_value(current)?;
        let run:Option<ContextRunRow>=sqlx::query_as("SELECT r.individual_id,r.trigger_event_id,r.mode,r.state_version,r.privacy_epoch,r.config_version,r.goal_id FROM runs r JOIN objects o ON (o.individual_id,o.id)=(r.individual_id,r.id) WHERE r.id=$1 AND NOT o.blocked AND r.status IN ('queued','running')")
            .bind(run_id).fetch_optional(&mut *tx).await?;
        let (individual, trigger, mode, version, epoch, config_version, current_goal_id) =
            run.ok_or(StoreError::Invalid("run unavailable"))?;
        if individual != current.id
            || version != current.state_version
            || epoch != current.privacy_epoch
            || config_version != current.config_version
        {
            return Err(StoreError::StaleState);
        }
        if current.lifecycle != crate::domain::Lifecycle::Active {
            return Err(StoreError::Inactive);
        }
        // A directly unavailable trigger retains its established error contract.
        // The recursive check still rejects revoked ancestors before any payload
        // is admitted into a model context or used to reserve a request.
        let trigger_event:Value=sqlx::query_scalar("SELECT to_jsonb(e) FROM events e JOIN objects o ON (o.individual_id,o.id)=(e.individual_id,e.id) WHERE e.id=$1 AND e.individual_id=$2 AND NOT o.blocked")
            .bind(trigger).bind(current.id).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("trigger unavailable"))?;
        if super::goals::unavailable(&mut tx, current.id, &[current.id, run_id, trigger]).await? {
            return Err(StoreError::StaleState);
        }
        let trigger: Event = serde_json::from_value(trigger_event)?;
        let scope = format!("event:{}", trigger.id);
        let relationship: Value = sqlx::query_scalar(&format!(
            "{} SELECT to_jsonb(r) FROM relationships r WHERE r.individual_id=$1 AND r.subject_key='owner'
             AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=$1 AND h.id IN ($1,r.subject_id))",
            super::api::VISIBLE
        ))
        .bind(current.id).fetch_optional(&mut *tx).await?.unwrap_or(json!({}));
        let relationship_id = relationship["subject_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok());
        let text = trigger.payload["text"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(1000)
            .collect();
        let memories = if let Some(page) = selected {
            Store::validate_retrieval_page(&mut tx, page, current.id, version, epoch).await?
        } else {
            let recalled = search(
                &mut tx,
                &MemoryQuery {
                    text,
                    scope: Some(scope.clone()),
                    subject_id: None,
                    at: Utc::now(),
                    limit: 10,
                },
            )
            .await?;
            super::retrieval::annotate(&mut tx, current.id, recalled).await?
        };
        let mut interests = super::interests::for_context(&mut tx, current.id).await?;
        let interests_has_more = interests.len() > 32;
        interests.truncate(32);
        let interest_ids: Vec<_> = interests.iter().map(|i| i.id.clone()).collect();
        let interest_sources =
            super::interests::context_sources(&mut tx, current.id, &interest_ids).await?;
        let raw_goals: Vec<Value> = sqlx::query_scalar(&format!(
            "{} SELECT to_jsonb(g) FROM goals g WHERE g.individual_id=$2
             AND g.status IN ('proposed','active','waiting')
             AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=$2 AND h.id IN ($2,g.id))
             ORDER BY (g.id=(SELECT goal_id FROM runs WHERE id=$1)) DESC NULLS LAST,g.priority DESC,g.created_at LIMIT 10",
            super::api::VISIBLE
        ))
        .bind(run_id).bind(current.id).fetch_all(&mut *tx).await?;
        let goals: Vec<GoalView> = raw_goals
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()?;
        let (model_used, tool_used): (i32, i32) =
            sqlx::query_as("SELECT model_calls,tool_calls FROM runs WHERE id=$1")
                .bind(run_id)
                .fetch_one(&mut *tx)
                .await?;
        let limit = |field: &str| {
            current.runtime_config["autonomy"][field]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or(StoreError::Invalid("runtime call budget"))
        };
        let model_limit = limit("max_model_calls_per_cycle")?;
        let tool_limit = limit("max_tool_calls_per_cycle")?;
        let mut context = ModelContext {
            goal_origin_catalog: Default::default(),
            execution_budget: Some(ExecutionBudgetView {
                model_calls_used: model_used as u32,
                model_calls_remaining: model_limit.saturating_sub(model_used as u32),
                model_call_limit: model_limit,
                tool_calls_used: tool_used as u32,
                tool_calls_remaining: tool_limit.saturating_sub(tool_used as u32),
                tool_call_limit: tool_limit,
            }),
            decision_review: None,
            retrieval: selected.map(|s| s.summary.clone()),
            selected_beliefs: None,
            autobiographical_context: None,
            social_context: None,
            interest_context: None,
            development_context: None,
            development_targets: vec![],
            trial_binding: None,
            individual_id: current.id,
            run_id,
            state_version: version,
            privacy_epoch: epoch,
            config_version,
            mode: if mode == "foreground" {
                RunMode::Foreground
            } else {
                RunMode::Background
            },
            now: Utc::now(),
            persona: current.persona,
            relationship,
            current_event_id: trigger.id,
            current_scope: scope,
            current_goal_id,
            completion_evidence: vec![],
            events: vec![trigger],
            memories: vec![],
            goals,
            interests,
            capabilities,
            document_catalog: DocumentCatalog::default(),
            person_catalog: Default::default(),
            input_manifest: vec![current.id, run_id],
            verified_goal_completions: vec![],
            goal_review: None,
        };
        context.input_manifest.extend(interest_sources);
        if let Some(id) = relationship_id {
            context.input_manifest.push(id);
        }
        context.input_manifest.push(context.current_event_id);
        context
            .input_manifest
            .extend(context.goals.iter().map(|g| g.id));
        // A goal's original observation must survive the recent-event window.
        // It is data, not an instruction that overrides current runtime rules.
        let goal: Option<(String,Value)>=sqlx::query_as("SELECT g.origin_type,g.origin_ids FROM goals g JOIN runs r ON (r.individual_id,r.goal_id)=(g.individual_id,g.id) WHERE r.id=$1 AND g.status IN ('proposed','active')")
            .bind(run_id).fetch_optional(&mut *tx).await?;
        if let Some((origin_type, ids)) = goal {
            let ids: Vec<String> = serde_json::from_value(ids)?;
            let origins = GoalOriginRef::from_storage(&origin_type, &ids)
                .map_err(|_| StoreError::Invalid("goal origin"))?;
            for origin in origins {
                let GoalOriginRef::Event(id) = origin else {
                    continue;
                };
                if super::goals::unavailable(&mut tx, current.id, &[id]).await? {
                    return Err(StoreError::StaleState);
                }
                if context.events.iter().any(|e| e.id == id) {
                    continue;
                }
                let row: Value = sqlx::query_scalar(
                    "SELECT to_jsonb(e) FROM events e WHERE individual_id=$1 AND id=$2",
                )
                .bind(current.id)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
                context.events.push(serde_json::from_value(row)?);
                context.input_manifest.push(id);
            }
        }
        // Recompute verifiable results from actual storage, never from model claims.
        // The caller receives only authorized evidence; commit recomputes it again.
        let mut required = Vec::new();
        for goal in &context.goals {
            if let Some(id) = super::reviews::feedback(&mut tx, current.id, goal.id).await? {
                required.push(id);
            }
            if let Some(contract) = &goal.completion {
                required.push(contract.citation().document_id);
            }
            if let Some(proof) = super::completion::verify(&mut tx, current.id, goal.id).await? {
                context.verified_goal_completions.push(goal.id);
                context.completion_evidence.push(CompletionEvidenceRef {
                    goal_id: goal.id,
                    result_event_id: proof.result_event_id,
                });
                required.push(proof.result_event_id);
            }
        }
        // Persisted validation feedback is bounded to two retries per run. It
        // contains runtime-authored guidance, never raw provider error bodies.
        let feedback:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM events WHERE individual_id=$1 AND correlation_id=$2 AND (kind='run.retry_scheduled' OR (kind='decision.reviewed' AND payload->>'supported'='false')) ORDER BY sequence")
            .bind(current.id).bind(run_id).fetch_all(&mut *tx).await?;
        required.extend(feedback);
        required.sort();
        required.dedup();
        for id in required {
            if super::goals::unavailable(&mut tx, current.id, &[id]).await? {
                return Err(StoreError::StaleState);
            }
            if context.events.iter().any(|e| e.id == id) {
                continue;
            }
            let row: Value = sqlx::query_scalar(
                "SELECT to_jsonb(e) FROM events e WHERE individual_id=$1 AND id=$2",
            )
            .bind(current.id)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            context.events.push(serde_json::from_value(row)?);
            context.input_manifest.push(id);
        }
        if serde_json::to_vec(&context)?.len() > 96 * 1024 {
            return Err(StoreError::Invalid("base context exceeds budget"));
        }
        // Results of this cycle take priority over general recall. Omitting them
        // would let the next model step report an action without its receipt.
        let results:Vec<(Value,bool)>=sqlx::query_as("SELECT to_jsonb(e),o.blocked OR EXISTS(SELECT 1 FROM derivation_edges d JOIN objects s ON (s.individual_id,s.id)=(d.individual_id,d.source_id) WHERE d.individual_id=e.individual_id AND d.target_id=e.id AND s.blocked) FROM actions a JOIN events e ON (e.individual_id,e.id)=(a.individual_id,a.result_event_id) JOIN objects o ON (o.individual_id,o.id)=(e.individual_id,e.id) WHERE a.run_id=$1 ORDER BY a.sequence")
            .bind(run_id).fetch_all(&mut *tx).await?;
        for (row, blocked) in results {
            if blocked {
                return Err(StoreError::StaleState);
            }
            let event: Event = serde_json::from_value(row)?;
            context.input_manifest.push(event.id);
            if !context.events.iter().any(|e| e.id == event.id) {
                context.events.push(event);
            }
        }
        if serde_json::to_vec(&context)?.len() > 96 * 1024 {
            return Err(StoreError::Invalid("action results exceed context budget"));
        }
        // Resource discovery must survive the eight-event recency window.
        // Metadata does not establish a read receipt or citation coverage.
        let documents: Vec<Value> = sqlx::query_scalar(
            "WITH RECURSIVE denied(id) AS (SELECT id FROM objects WHERE individual_id=$1 AND blocked UNION SELECT edge.target_id FROM derivation_edges edge JOIN denied ON denied.id=edge.source_id WHERE edge.individual_id=$1) SELECT jsonb_build_object('document_id',d.event_id,'title',d.title,'source_label',d.source_label,'characters',char_length(d.body)) FROM documents d JOIN events e ON (e.individual_id,e.id)=(d.individual_id,d.event_id) JOIN objects o ON (o.individual_id,o.id)=(d.individual_id,d.event_id) WHERE d.individual_id=$1 AND NOT o.blocked AND NOT EXISTS(SELECT 1 FROM denied WHERE denied.id=d.event_id) ORDER BY e.sequence DESC LIMIT 21",
        )
        .bind(current.id)
        .fetch_all(&mut *tx)
        .await?;
        context.document_catalog.has_more = documents.len() > 20;
        for row in documents.into_iter().take(20) {
            let document: DocumentRef = serde_json::from_value(row)?;
            let mut candidate = context.clone();
            candidate.input_manifest.push(document.document_id);
            candidate.document_catalog.items.push(document);
            if serde_json::to_vec(&candidate)?.len() <= 96 * 1024 {
                context = candidate;
            } else {
                context.document_catalog.has_more = true;
            }
        }
        let catalog = super::entities::catalog(&mut tx, current.id).await?;
        context.person_catalog.has_more = catalog.len() > 20;
        for (person, sources, more) in catalog.into_iter().take(20) {
            let mut candidate = context.clone();
            candidate.person_catalog.items.push(person);
            candidate.input_manifest.extend(sources);
            candidate.person_catalog.has_more |= more;
            if serde_json::to_vec(&candidate)?.len() <= 96 * 1024 {
                context = candidate;
            } else {
                context.person_catalog.has_more = true;
            }
        }
        // Each included memory brings its actual evidence. Never invent IDs for omitted evidence.
        for memory in memories {
            let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(e) FROM events e JOIN objects o ON (o.individual_id,o.id)=(e.individual_id,e.id) WHERE e.id=ANY($1) AND NOT o.blocked ORDER BY e.sequence")
                .bind(&memory.source_event_ids).fetch_all(&mut *tx).await?;
            if rows.len() != memory.source_event_ids.len() {
                continue;
            }
            let mut candidate = context.clone();
            for row in rows {
                let event: Event = serde_json::from_value(row)?;
                if !candidate.events.iter().any(|e| e.id == event.id) {
                    candidate.input_manifest.push(event.id);
                    candidate.events.push(event);
                }
            }
            candidate.input_manifest.push(memory.id);
            candidate
                .input_manifest
                .extend(memory.person_evidence_ids());
            candidate.memories.push(memory);
            if serde_json::to_vec(&candidate)?.len() <= 96 * 1024 {
                context = candidate;
            }
        }
        let recent: Vec<Value> = sqlx::query_scalar(&format!(
            "{} SELECT to_jsonb(e) FROM events e WHERE e.individual_id=$1
             AND e.kind IN ('message.received','message.sent','tool.result','document.imported')
             AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=$1 AND h.id IN ($1,e.id))
             ORDER BY e.sequence DESC LIMIT 8",
            super::api::VISIBLE
        ))
        .bind(current.id)
        .fetch_all(&mut *tx)
        .await?;
        for row in recent {
            let event: Event = serde_json::from_value(row)?;
            if context.events.iter().any(|e| e.id == event.id) {
                continue;
            }
            let mut candidate = context.clone();
            candidate.input_manifest.push(event.id);
            candidate.events.push(event);
            if serde_json::to_vec(&candidate)?.len() <= 96 * 1024 {
                context = candidate;
            }
        }
        context.events.sort_by_key(|e| e.sequence);
        context.input_manifest.sort();
        context.input_manifest.dedup();
        let mut origin_events: Vec<_> = context
            .events
            .iter()
            .filter(|e| is_goal_origin_event(e))
            .collect();
        origin_events.sort_by_key(|e| {
            (
                e.id != context.current_event_id,
                std::cmp::Reverse(e.sequence),
            )
        });
        let origins: Vec<_> = origin_events
            .into_iter()
            .map(|e| GoalOriginOption {
                origin_type: "event".into(),
                origin_id: e.id.to_string(),
                label: e.kind.clone(),
            })
            .chain(context.interests.iter().map(|i| GoalOriginOption {
                origin_type: "interest".into(),
                origin_id: i.id.clone(),
                label: i.description.clone(),
            }))
            .collect();
        context.goal_origin_catalog.has_more = interests_has_more || origins.len() > 32;
        for origin in origins.into_iter().take(32) {
            let mut candidate = context.clone();
            candidate.goal_origin_catalog.items.push(origin);
            if serde_json::to_vec(&candidate)?.len() <= 96 * 1024 {
                context = candidate;
            } else {
                context.goal_origin_catalog.has_more = true;
            }
        }
        // Recall is a selection, not a claim to complete belief/history coverage.
        // A stance brings *all* support and counter events, or none of the bundle.
        let beliefs =
            super::beliefs::for_context(&mut tx, current.id, &context.current_scope).await?;
        if !beliefs.is_empty() {
            context.selected_beliefs = Some(crate::domain::beliefs::SelectedBeliefs {
                items: vec![],
                has_more: beliefs.len() > 6,
            });
            if serde_json::to_vec(&context)?.len() > 96 * 1024 {
                return Err(StoreError::Invalid(
                    "belief selection metadata exceeds budget",
                ));
            }
            for belief in beliefs.into_iter().take(6) {
                let mut ids: Vec<_> = belief.evidence_ids().collect();
                ids.sort();
                ids.dedup();
                if super::goals::unavailable(&mut tx, current.id, &ids).await? {
                    return Err(StoreError::StaleState);
                }
                let rows: Vec<Value> = sqlx::query_scalar(
                    "SELECT to_jsonb(e) FROM events e WHERE individual_id=$1 AND id=ANY($2) ORDER BY sequence",
                )
                .bind(current.id)
                .bind(&ids)
                .fetch_all(&mut *tx)
                .await?;
                if rows.len() != ids.len() {
                    return Err(StoreError::Invalid("incomplete belief evidence"));
                }
                let mut candidate = context.clone();
                candidate.input_manifest.push(belief.id);
                for row in rows {
                    let event: Event = serde_json::from_value(row)?;
                    candidate.input_manifest.push(event.id);
                    if !candidate.events.iter().any(|e| e.id == event.id) {
                        candidate.events.push(event);
                    }
                }
                candidate
                    .selected_beliefs
                    .as_mut()
                    .unwrap()
                    .items
                    .push(belief);
                candidate.input_manifest.sort();
                candidate.input_manifest.dedup();
                if serde_json::to_vec(&candidate)?.len() <= 96 * 1024 {
                    context = candidate;
                } else {
                    context.selected_beliefs.as_mut().unwrap().has_more = true;
                }
            }
            if !context
                .selected_beliefs
                .as_ref()
                .unwrap()
                .valid_for(&context)
            {
                return Err(StoreError::Invalid("invalid belief selection"));
            }
            context.events.sort_by_key(|e| e.sequence);
        }
        // Historical interpretations arrive with the whole actual source bundle.
        // Preserve existing context first; never crop a contrary source to fit.
        super::autobiography::enrich_context(&mut tx, &mut context).await?;
        super::social::enrich_context(&mut tx, &mut context).await?;
        super::development_context::enrich_context(&mut tx, &mut context).await?;
        tx.commit().await?;
        self.require_privacy_snapshot(privacy_generation)?;
        Ok(context)
    }
}
