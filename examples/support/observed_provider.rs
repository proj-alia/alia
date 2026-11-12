//! Synthetic-development telemetry only. Not used by the service or deploy config.
use alia::{
    adapters::model::ChatProvider,
    domain::{cognition::ModelContext, review::GoalReviewInput},
    runtime::{model::*, scheduling::TokenAllowance},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

pub struct ObservedProvider {
    inner: ChatProvider,
    candidates: Mutex<Vec<Value>>,
    reviews: Mutex<Vec<Value>>,
    belief_capture: bool,
    belief_observations: Mutex<Vec<Value>>,
}
impl ObservedProvider {
    pub fn new(inner: ChatProvider) -> Self {
        Self {
            inner,
            candidates: Mutex::new(vec![]),
            reviews: Mutex::new(vec![]),
            belief_capture: false,
            belief_observations: Mutex::new(vec![]),
        }
    }
    /// Opt-in only: old example output and provider behavior stay unchanged.
    #[allow(dead_code)] // Shared by older examples which do not opt into v5 capture.
    pub fn with_belief_observations(inner: ChatProvider) -> Self {
        Self {
            belief_capture: true,
            ..Self::new(inner)
        }
    }
    #[allow(dead_code)] // Shared by older examples which preserve their old output.
    pub fn belief_observations(&self) -> Vec<Value> {
        self.belief_observations.lock().unwrap().clone()
    }
    #[allow(dead_code)] // Some observers consume only the opt-in v5 projection.
    pub fn candidates(&self) -> Vec<Value> {
        self.candidates.lock().unwrap().clone()
    }
    #[allow(dead_code)] // Some observers consume only the opt-in v5 projection.
    pub fn reviews(&self) -> Vec<Value> {
        self.reviews.lock().unwrap().clone()
    }
}
#[async_trait::async_trait]
impl ModelProvider for ObservedProvider {
    fn mode(&self) -> ProviderMode {
        self.inner.mode()
    }
    fn status(&self) -> ModelStatus {
        self.inner.status()
    }
    fn token_allowance(
        &self,
        c: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        self.inner.token_allowance(c, l)
    }
    fn supports_goal_review(&self) -> bool {
        self.inner.supports_goal_review()
    }
    fn supports_decision_review(&self) -> bool {
        self.inner.supports_decision_review()
    }
    fn review_allowance(
        &self,
        c: &GoalReviewInput,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        self.inner.review_allowance(c, l)
    }
    fn decision_review_allowance(
        &self,
        c: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        self.inner.decision_review_allowance(c, l)
    }
    async fn review(
        &self,
        i: &GoalReviewInput,
        l: CallLimits,
        m: ModelCallMetadata,
        c: &ModelContext,
    ) -> Result<GoalReviewResult, ModelFailure> {
        self.inner.review(i, l, m, c).await
    }
    async fn review_decision(
        &self,
        c: &ModelContext,
        l: CallLimits,
        m: ModelCallMetadata,
    ) -> Result<DecisionReviewResult, ModelFailure> {
        let packet = self.belief_capture.then(|| selected_review_packet(c));
        let result = self.inner.review_decision(c, l, m).await;
        let value = match &result {
            Ok(r) => {
                json!({"status":"returned_assessment_not_commit_proof","assessment":r.assessment,"usage":r.usage,"provider_model":r.provider_model,"provider_mode":r.provider_mode})
            }
            Err(f) => {
                json!({"status":"assessment_failed","error":f.error.to_string(),"validation_code":match f.error{ModelError::Assessment(error)=>Some(error.code()),_=>None},"usage":f.usage,"request_sent":f.request_sent})
            }
        };
        self.reviews.lock().unwrap().push(value.clone());
        if self.belief_capture {
            self.belief_observations.lock().unwrap().push(json!({
                "role":"decision_review","run_id":c.run_id,
                "context":belief_context(c, None),"packet":packet,"outcome":value
            }));
        }
        result
    }
    async fn decide(
        &self,
        c: &ModelContext,
        l: CallLimits,
        m: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        let result = self.inner.decide(c, l, m).await;
        if self.belief_capture {
            let candidate = result.as_ref().ok().map(|r| &r.proposal);
            let outcome = match &result {
                Ok(r) => {
                    json!({"status":"parsed_candidate_not_commit_proof", "proposal":r.proposal,
                    "proposal_sha256":canonical_sha256(&r.proposal),"usage":r.usage,
                    "provider_mode":r.provider_mode,"provider_model":r.provider_model})
                }
                Err(f) => json!({"status":"author_failed","error":f.error.to_string(),
                    "usage":f.usage,"request_sent":f.request_sent}),
            };
            self.belief_observations.lock().unwrap().push(json!({
                "role":"author","run_id":c.run_id,
                "context":belief_context(c,candidate),"outcome":outcome
            }));
        }
        if let Ok(r) = &result {
            self.candidates.lock().unwrap().push(json!({"status":"parsed_candidate_commit_outcome_in_events","run_id":c.run_id,"execution_budget":c.execution_budget,"goal_origin_catalog":c.goal_origin_catalog,"evidence_view":evidence_view(c,&r.proposal),"proposal":r.proposal,"provider_model":r.provider_model}));
        }
        result
    }
}

fn canonical_sha256(value: &impl serde::Serialize) -> String {
    let value = serde_json::to_value(value).expect("domain observation serialization");
    format!("{:x}", Sha256::digest(serde_json::to_vec(&value).unwrap()))
}

/// Retain actual selected stances and source material needed by these targets,
/// not the full prompt/persona, provider wire, hidden reasoning or unrelated history.
fn belief_context(
    c: &ModelContext,
    p: Option<&alia::domain::cognition::DecisionProposal>,
) -> Value {
    let mut ids = std::collections::HashSet::from([c.current_event_id]);
    for b in c.selected_beliefs.iter().flat_map(|s| &s.items) {
        ids.extend(b.evidence_ids());
    }
    if let Some(p) = p {
        ids.extend(p.reply_evidence_ids.iter().copied());
        for b in &p.belief_changes {
            ids.extend(
                b.support_event_ids
                    .iter()
                    .chain(&b.counter_event_ids)
                    .copied(),
            );
        }
        for m in &p.memory_changes {
            ids.extend(m.source_event_ids.iter().copied());
        }
        for person in &p.entity_changes {
            ids.insert(person.source.event_id);
        }
    }
    json!({"individual_id":c.individual_id,"run_id":c.run_id,
        "state_version":c.state_version,"privacy_epoch":c.privacy_epoch,
        "config_version":c.config_version,"current_scope":c.current_scope,
        "current_event_id":c.current_event_id,"input_manifest":c.input_manifest,
        "execution_budget":c.execution_budget,"context_sha256":canonical_sha256(c),
        "selected_beliefs":c.selected_beliefs,
        "visible_event_ids":c.events.iter().map(|e|e.id).collect::<Vec<_>>(),
        "visible_memory_ids":c.memories.iter().map(|m|m.id).collect::<Vec<_>>(),
        "events":c.events.iter().filter(|e|ids.contains(&e.id)).collect::<Vec<_>>(),
        "memories":c.memories.iter().filter(|m|ids.contains(&m.id)).collect::<Vec<_>>(),
        "coverage":"actual_selected_beliefs_and_target_material_only",
        "global_history_complete":false})
}

fn selected_review_packet(c: &ModelContext) -> Value {
    match alia::domain::grounding::evidence_packet(c) {
        Ok(packet) => {
            let mut selected = serde_json::Map::new();
            for key in [
                "packet_version",
                "individual_id",
                "run_id",
                "current_scope",
                "decision_review",
                "evidence_by_target",
                "runtime_atomic_commit_contract",
                "request_not_evidence",
            ] {
                selected.insert(key.into(), packet[key].clone());
            }
            Value::Object(selected)
        }
        Err(error) => json!({"capture_error":error.code()}),
    }
}

// Only domain source projections of explicitly declared target evidence are
// retained. This is not the complete prompt, provider wire or hidden reasoning.
fn declared_material(c: &ModelContext, ids: &[uuid::Uuid]) -> Vec<Value> {
    ids.iter()
        .map(|id| {
            if let Some(event) = c.events.iter().find(|event| event.id == *id) {
                json!({
                    "id":id,"object":"event","kind":event.kind,
                    "source_kind":event.source_kind,"actor_id":event.actor_id,
                    "text":event.payload.get("text"),
                    "result_text":event.payload.get("output").and_then(|output| output.get("text")),
                    "capability_id":event.payload.get("capability_id"),
                    "result_status":event.payload.get("status"),
                    "projection":"source text and receipt metadata only; import metadata is not read content"
                })
            } else if let Some(memory) = c.memories.iter().find(|memory| memory.id == *id) {
                json!({"id":id,"object":"memory","content":memory.content,
                    "scope":memory.scope,"version":memory.version,"validity":memory.validity,
                    "source_event_ids":memory.source_event_ids})
            } else {
                json!({"id":id,"object":"not_in_visible_events_or_memories","content":null})
            }
        })
        .collect()
}

fn evidence_view(c: &ModelContext, proposal: &alia::domain::cognition::DecisionProposal) -> Value {
    let memories: Vec<Value> = proposal.memory_changes.iter().enumerate().map(|(index,memory)| {
        json!({"target":{"kind":"memory","index":index},
            "declared_ids":memory.source_event_ids,"material":declared_material(c,&memory.source_event_ids)})
    }).collect();
    let people: Vec<Value> = proposal.entity_changes.iter().enumerate().map(|(index,person)| {
        json!({"target":{"kind":"entity","index":index},
            "declared_ids":[person.source.event_id],"material":declared_material(c,&[person.source.event_id])})
    }).collect();
    json!({
        "version":"synthetic-target-evidence/1",
        "scope":"selected author context only; not a complete database or history scan",
        "global_history_complete":false,
        "run_id":c.run_id,"state_version":c.state_version,"privacy_epoch":c.privacy_epoch,
        "config_version":c.config_version,"current_event_id":c.current_event_id,
        "visible_event_ids":c.events.iter().map(|event|event.id).collect::<Vec<_>>(),
        "visible_memory_ids":c.memories.iter().map(|memory|memory.id).collect::<Vec<_>>(),
        "input_manifest":c.input_manifest,
        "reply":{"declared_ids":proposal.reply_evidence_ids,
            "material":declared_material(c,&proposal.reply_evidence_ids)},
        "memory_targets":memories,"entity_targets":people
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ModelContext {
        serde_json::from_value(json!({
            "individual_id":"00000000-0000-4000-8000-000000000001",
            "run_id":"00000000-0000-4000-8000-000000000002",
            "state_version":1,"privacy_epoch":0,"config_version":1,
            "mode":"foreground","now":"2026-11-12T00:00:00Z",
            "persona":{},"relationship":{},
            "current_event_id":"00000000-0000-4000-8000-000000000003",
            "current_scope":"general",
            "events":[{
                "individual_id":"00000000-0000-4000-8000-000000000001",
                "id":"00000000-0000-4000-8000-000000000003","sequence":1,
                "kind":"document.imported","source_kind":"external_document","actor_id":"owner",
                "occurred_at":"2026-11-12T00:00:00Z","recorded_at":"2026-11-12T00:00:00Z",
                "payload":{"title":"metadata only","characters":99},
                "correlation_id":"00000000-0000-4000-8000-000000000002","causation_id":null
            }],
            "memories":[{
                "individual_id":"00000000-0000-4000-8000-000000000001",
                "id":"00000000-0000-4000-8000-000000000004","content":"synthetic known fact",
                "kind":"semantic","scope":"general","validity":"active",
                "valid_from":"2026-11-12T00:00:00Z","valid_until":null,"version":2,
                "source_event_ids":["00000000-0000-4000-8000-000000000003"],"supersedes":null
            }],
            "goals":[],"interests":[],"capabilities":[],
            "input_manifest":["00000000-0000-4000-8000-000000000003","00000000-0000-4000-8000-000000000004"],
            "verified_goal_completions":[]
        })).expect("synthetic context")
    }

    #[test]
    fn import_metadata_and_unavailable_id_never_become_read_content() {
        let c = context();
        let absent = uuid::Uuid::from_u128(10);
        let material = declared_material(&c, &[c.current_event_id, absent]);
        assert_eq!(material[0]["kind"], "document.imported");
        assert!(material[0]["text"].is_null());
        assert!(material[0]["result_text"].is_null());
        assert_eq!(material[1]["object"], "not_in_visible_events_or_memories");
        assert!(material[1]["content"].is_null());
    }

    #[test]
    fn each_target_keeps_its_own_declared_material_and_visible_scope() {
        let c = context();
        let proposal = alia::domain::cognition::DecisionProposal::parse(
            json!({"schema_version":1,"reply":"Synthetic source comparison",
                "reply_evidence_ids":[c.memories[0].id],
                "memory_changes":[{"content":"synthetic proposed fact","kind":"semantic","scope":"general",
                    "source_event_ids":[c.current_event_id],"supersedes":null}],
                "goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"synthetic test"})
                .to_string().as_bytes()
        ).expect("synthetic candidate");
        let view = evidence_view(&c, &proposal);
        assert_eq!(view["global_history_complete"], false);
        assert_eq!(
            view["reply"]["material"][0]["id"],
            c.memories[0].id.to_string()
        );
        assert_eq!(
            view["reply"]["material"][0]["content"],
            "synthetic known fact"
        );
        assert_eq!(
            view["memory_targets"][0]["material"][0]["id"],
            c.current_event_id.to_string()
        );
        assert!(view["memory_targets"][0]["material"][0]["text"].is_null());
        assert_eq!(view["visible_memory_ids"], json!([c.memories[0].id]));
        assert_eq!(view["input_manifest"], json!(c.input_manifest));
    }
    #[test]
    fn optional_belief_capture_is_off_for_legacy_constructors() {
        use alia::adapters::model::ChatDialect;
        let provider = || {
            ChatProvider::new(
                "https://example.invalid",
                "glm-5.3-flash",
                "synthetic-never-sent",
                ChatDialect::OpenCodeGo,
            )
            .unwrap()
        };
        let legacy = ObservedProvider::new(provider());
        assert!(!legacy.belief_capture);
        assert!(legacy.belief_observations().is_empty());
        assert!(legacy.candidates().is_empty() && legacy.reviews().is_empty());
        assert!(ObservedProvider::with_belief_observations(provider()).belief_capture);
    }

    #[test]
    fn v5_capture_keeps_actual_previous_basis_roles_without_full_prompt() {
        use alia::domain::{
            beliefs::{BeliefPosition, CurrentBelief, SelectedBeliefs},
            grounding,
        };
        let mut c = context();
        c.persona = json!({"synthetic_private_prompt_marker":"must-not-copy"});
        let mut counter = c.events[0].clone();
        counter.id = uuid::Uuid::from_u128(5);
        counter.payload = json!({"text":"Synthetic counterexample; not a verified fact"});
        c.events.push(counter.clone());
        let old = CurrentBelief {
            id: uuid::Uuid::from_u128(6),
            version: 2,
            statement: "Earlier bounded understanding".into(),
            scope: "general".into(),
            position: BeliefPosition::Questioned,
            brief_basis: "Two source roles".into(),
            support_event_ids: vec![c.current_event_id],
            counter_event_ids: vec![counter.id, c.current_event_id],
        };
        c.input_manifest.extend([counter.id, old.id]);
        c.selected_beliefs = Some(SelectedBeliefs {
            items: vec![old.clone()],
            has_more: false,
        });
        let p = alia::domain::cognition::DecisionProposal::parse(
            json!({"schema_version":1,
            "reply":"A new interpretation remains bounded","reply_evidence_ids":[old.id],
            "memory_changes":[],"belief_changes":[{"statement":"Changed interpretation",
            "scope":"general","position":"questioned","brief_basis":"Counterevidence retained",
            "support_event_ids":[c.current_event_id],"counter_event_ids":[counter.id],
            "supersedes":old.id}],"goal_changes":[],"tool_intents":[],"development":null,
            "brief_reason":"Synthetic spy only"})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let captured = belief_context(&c, Some(&p));
        assert_eq!(
            captured["selected_beliefs"]["items"][0]["id"],
            json!(old.id)
        );
        assert_eq!(captured["events"].as_array().unwrap().len(), 2);
        assert!(captured.get("persona").is_none());
        assert!(!captured.to_string().contains("must-not-copy"));
        let review = grounding::review_context(&c, uuid::Uuid::from_u128(7), &p).unwrap();
        let packet = selected_review_packet(&review);
        assert_eq!(packet["packet_version"], "target-citations/5");
        assert!(packet.get("persona_not_experience").is_none());
        let reply = &packet["evidence_by_target"][0];
        assert_eq!(reply["allowed_evidence_ids"], json!([old.id]));
        assert_eq!(
            reply["evidence"][0]["basis_events"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["record"]["id"] == json!(c.current_event_id))
                .unwrap()["roles"],
            json!(["support", "counter"])
        );
        let belief = &packet["evidence_by_target"][1];
        assert_eq!(belief["target"], json!({"kind":"belief","index":0}));
        let previous =
            &belief["belief_under_review"]["previous_comparison_not_additional_citations"];
        assert_eq!(previous["record"], json!(old));
        assert_eq!(previous["basis_events"].as_array().unwrap().len(), 2);
        assert!(
            !belief["allowed_evidence_ids"]
                .as_array()
                .unwrap()
                .contains(&json!(old.id))
        );
    }
}
