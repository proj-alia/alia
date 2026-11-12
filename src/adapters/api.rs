//! Owner-facing projections and commands. No provider errors or deployment secrets.
use super::{
    goals::unavailable,
    store::{
        Store, StoreError, advance, digest, insert_event, outbox, replay, save_receipt, state,
    },
};
use crate::domain::{Lifecycle, LifecycleCommand, config::RuntimeRules};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

// UNION (not UNION ALL) terminates even when the provenance graph contains cycles.
pub(super) const VISIBLE: &str = "WITH RECURSIVE hidden(individual_id,id) AS (SELECT individual_id,id FROM objects WHERE blocked UNION SELECT d.individual_id,d.target_id FROM derivation_edges d JOIN hidden h ON (d.individual_id,d.source_id)=(h.individual_id,h.id)) ";
pub(super) async fn existing(tx: &mut Transaction<'_, Postgres>) -> Result<Uuid, StoreError> {
    let id: Uuid = sqlx::query_scalar("SELECT id FROM individuals WHERE singleton")
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotInitialized)?;
    if unavailable(tx, id, &[id]).await? {
        return Err(StoreError::NotFound);
    }
    Ok(id)
}
#[derive(Clone, Copy)]
pub enum Listing {
    Messages,
    RecentMessages,
    Memories,
    Goals,
    Runs,
    Audit,
}
/// Only public metadata participates in these read-only filters.
#[derive(Default, Serialize)]
pub struct ConsoleFilter {
    pub q: String,
    pub status: Option<String>,
    pub mode: Option<String>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}
impl ConsoleFilter {
    pub fn fingerprint(&self) -> Result<String, StoreError> {
        digest(self)
    }
    pub fn is_empty(&self) -> bool {
        self.q.is_empty()
            && self.status.is_none()
            && self.mode.is_none()
            && self.from.is_none()
            && self.to.is_none()
    }
}
#[derive(Default)]
pub struct ConsolePosition {
    pub id: Uuid,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub sequence: i64,
}
#[derive(Serialize, Deserialize)]
pub struct StateChange {
    pub individual_id: Uuid,
    pub version: i64,
    pub config_version: i64,
}
#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum OwnerChange {
    AutonomyPaused(bool),
    Lifecycle(LifecycleCommand),
    Settings(Value),
}
impl Store {
    pub async fn public_state(&self) -> Result<Value, StoreError> {
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let id = existing(&mut tx).await?;
        super::development_context::require_public_persona(&mut tx, id).await?;
        let mut view:Value=sqlx::query_scalar("SELECT jsonb_build_object('individual_id',id,'created_at',created_at,'origin',origin,'persona',persona,'lifecycle',lifecycle,'version',state_version,'config_version',config_version,'autonomy_paused',autonomy_paused,'runtime',runtime_config,'budget_frozen',budget_frozen) FROM individuals WHERE id=$1").bind(id).fetch_one(&mut *tx).await?;
        let window:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('starts_at',w.starts_at,'ends_at',w.ends_at,'charged_total',w.charged_total,'charged_background',w.charged_background,'reserved_total',COALESCE((SELECT sum(reserved_tokens) FROM usage_reservations r WHERE r.window_id=w.id AND status IN ('reserved','dispatched')),0),'reserved_background',COALESCE((SELECT sum(reserved_tokens) FROM usage_reservations r WHERE r.window_id=w.id AND mode='background' AND status IN ('reserved','dispatched')),0)) FROM budget_windows w WHERE individual_id=$1 AND ends_at>clock_timestamp() ORDER BY starts_at DESC LIMIT 1").bind(id).fetch_optional(&mut *tx).await?;
        view["event_cursor"] = json!(super::notifications::head(&mut tx, id).await?.to_string());
        view["budget_window"] = window.unwrap_or(Value::Null);
        let goals:Value=sqlx::query_scalar(&format!("{VISIBLE} SELECT COALESCE(jsonb_object_agg(status,n),'{{}}') FROM (SELECT status,count(*) n FROM goals g WHERE g.individual_id=$1 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(g.individual_id,g.id)) GROUP BY status) q")).bind(id).fetch_one(&mut *tx).await?;
        view["goals"] = goals;
        // Construct capabilities from the same config snapshot, not a second read.
        use crate::runtime::capability::{Capability, LocalCapability};
        let mut caps = vec![
            LocalCapability::ReadDocument.info(),
            LocalCapability::WriteNote.info(),
            LocalCapability::SearchMemory.info(),
            crate::runtime::web::web_info(),
        ];
        for cap in &mut caps {
            cap.authorized = view["runtime"]["capabilities"]["enabled"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(&cap.id)));
            if cap.id == "web.read_allowed"
                && view["runtime"]["capabilities"]["allowed_web_hosts"]
                    .as_array()
                    .is_none_or(|hosts| hosts.is_empty())
            {
                cap.authorized = false;
            }
        }
        view["capabilities"] = serde_json::to_value(caps)?;
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(view)
    }
    pub(super) async fn annotate_memories(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: Uuid,
        rows: &mut [Value],
    ) -> Result<(), StoreError> {
        // Same read snapshot and privacy closure as the memory list. No raw
        // assertion payload, external_key, or unrelated person catalogue.
        let ids: Vec<Uuid> = rows
            .iter()
            .filter_map(|m| m["id"].as_str()?.parse().ok())
            .collect();
        let annotations: Vec<(Uuid, Value)> = sqlx::query_as(&format!(r#"{VISIBLE}, candidates AS (
              SELECT l.memory_id,l.entity_id,
                jsonb_build_object('entity_id',l.entity_id,'label',p.label,'role',l.role,
                  'assertion_event_id',e.id,'source_event_id',l.source_event_id,
                  'actor_id',e.actor_id,'source_kind',e.source_kind,'occurred_at',e.occurred_at) AS item,
                row_number() OVER(PARTITION BY l.memory_id ORDER BY l.entity_id) AS rn
              FROM memory_person_links l JOIN memories m ON (m.individual_id,m.id)=(l.individual_id,l.memory_id)
              JOIN person_entities p ON (p.individual_id,p.id)=(l.individual_id,l.entity_id)
              JOIN events e ON (e.individual_id,e.id)=(l.individual_id,l.assertion_event_id)
              WHERE l.individual_id=$1 AND l.memory_id=ANY($2::uuid[])
                AND l.memory_version=m.version AND l.content_digest=encode(sha256(convert_to(m.content,'UTF8')),'hex')
                AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=l.individual_id
                  AND h.id IN (l.entity_id,l.assertion_event_id,l.source_event_id))
            ) SELECT memory_id,jsonb_agg(item ORDER BY entity_id) FROM candidates WHERE rn<=21 GROUP BY memory_id"#))
                .bind(id).bind(ids).fetch_all(&mut **tx).await?;
        let mut annotations: std::collections::HashMap<Uuid, Value> =
            annotations.into_iter().collect();
        for row in rows.iter_mut() {
            let id: Uuid = serde_json::from_value(row["id"].clone())?;
            let mut items = annotations.remove(&id).unwrap_or_else(|| json!([]));
            let items = items
                .as_array_mut()
                .ok_or(StoreError::Invalid("person projection"))?;
            row["person_assertions_more"] = json!(items.len() > 20);
            items.truncate(20);
            row["person_assertions"] = json!(items);
        }
        Ok(())
    }
    pub async fn public_list(
        &self,
        kind: Listing,
        sequence: i64,
        after: Uuid,
        limit: u32,
        q: &str,
        validity: Option<&str>,
    ) -> Result<Vec<Value>, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let id = existing(&mut tx).await?;
        let mut rows: Vec<Value>=match kind {
            Listing::Messages=>sqlx::query_scalar(&format!("{VISIBLE} SELECT jsonb_build_object('id',e.id,'sequence',e.sequence,'role',CASE WHEN e.kind='message.received' THEN 'user' ELSE 'assistant' END,'text',e.payload->>'text','occurred_at',e.occurred_at,'source_kind',e.source_kind,'operation_id',e.correlation_id) FROM events e WHERE e.individual_id=$1 AND e.kind IN ('message.received','message.sent') AND e.sequence>$2 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(e.individual_id,e.id)) ORDER BY e.sequence LIMIT $3")).bind(id).bind(sequence).bind(i64::from(limit)).fetch_all(&mut *tx).await?,
            Listing::RecentMessages=>sqlx::query_scalar(&format!("{VISIBLE} SELECT jsonb_build_object('id',e.id,'sequence',e.sequence,'role',CASE WHEN e.kind='message.received' THEN 'user' ELSE 'assistant' END,'text',e.payload->>'text','occurred_at',e.occurred_at,'source_kind',e.source_kind,'operation_id',e.correlation_id) FROM events e WHERE e.individual_id=$1 AND e.kind IN ('message.received','message.sent') AND e.sequence<$2 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(e.individual_id,e.id)) ORDER BY e.sequence DESC LIMIT $3")).bind(id).bind(if sequence==0{i64::MAX}else{sequence}).bind(i64::from(limit)).fetch_all(&mut *tx).await?,
            Listing::Goals=>sqlx::query_scalar(&format!("{VISIBLE} SELECT to_jsonb(g) FROM goals g WHERE g.individual_id=$1 AND g.id>$2 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(g.individual_id,g.id)) ORDER BY g.id LIMIT $3")).bind(id).bind(after).bind(i64::from(limit)).fetch_all(&mut *tx).await?,
            Listing::Runs=>sqlx::query_scalar(&format!("{VISIBLE} SELECT jsonb_build_object('id',r.id,'status',r.status,'mode',r.mode,'model_calls',r.model_calls,'tool_calls',r.tool_calls,'created_at',r.created_at,'finished_at',r.finished_at,'error_code',r.error_code,'model_retries',r.model_retries,'repair_attempts',r.repair_attempts) FROM runs r WHERE r.individual_id=$1 AND ($2::uuid='00000000-0000-0000-0000-000000000000'::uuid OR (r.created_at,r.id)<(SELECT anchor.created_at,anchor.id FROM runs anchor WHERE anchor.individual_id=$1 AND anchor.id=$2 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=anchor.individual_id AND h.id IN (anchor.id,anchor.trigger_event_id)))) AND ($4::text IS NULL OR r.status=$4) AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=r.individual_id AND h.id IN (r.individual_id,r.id,r.trigger_event_id)) ORDER BY r.created_at DESC,r.id DESC LIMIT $3")).bind(id).bind(after).bind(i64::from(limit)).bind(validity).fetch_all(&mut *tx).await?,
            Listing::Audit=>sqlx::query_scalar(&format!("{VISIBLE} SELECT jsonb_build_object('id',e.id,'sequence',e.sequence,'kind',e.kind,'actor_id',e.actor_id,'source_kind',e.source_kind,'occurred_at',e.occurred_at,'correlation_id',CASE WHEN NOT EXISTS(SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(e.individual_id,e.correlation_id)) THEN e.correlation_id ELSE NULL END) FROM events e WHERE e.individual_id=$1 AND e.sequence>$2 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=e.individual_id AND h.id IN (e.individual_id,e.id)) ORDER BY e.sequence LIMIT $3")).bind(id).bind(sequence).bind(i64::from(limit)).fetch_all(&mut *tx).await?,
            Listing::Memories=>sqlx::query_scalar(&format!("{VISIBLE} SELECT to_jsonb(m)||jsonb_build_object('source_event_ids',ARRAY(SELECT event_id FROM memory_sources s WHERE (s.individual_id,s.memory_id)=(m.individual_id,m.id))) FROM memories m WHERE m.individual_id=$1 AND m.id>$2 AND ($4::text IS NULL OR m.validity=$4) AND strpos(lower(m.content),lower($5))>0 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(m.individual_id,m.id)) ORDER BY m.id LIMIT $3")).bind(id).bind(after).bind(i64::from(limit)).bind(validity).bind(q).fetch_all(&mut *tx).await?,
        };
        if matches!(kind, Listing::Runs) {
            for row in &mut rows {
                row["error_code"] = json!(super::console::safe_failure(row["error_code"].as_str()));
            }
        }
        if matches!(kind, Listing::Memories) {
            Self::annotate_memories(&mut tx, id, &mut rows).await?;
        }
        tx.commit().await?;
        Ok(rows)
    }
    pub async fn normalize_console_filter(
        &self,
        mut filter: ConsoleFilter,
    ) -> Result<ConsoleFilter, StoreError> {
        // Use the database's Unicode/collation rules for BOTH the cursor digest
        // and substring comparison. Rust lowercasing can produce a different
        // character sequence (e.g. capital dotted I) and silently miss metadata.
        filter.q = sqlx::query_scalar("SELECT lower($1::text)")
            .bind(&filter.q)
            .fetch_one(&self.pool)
            .await?;
        Ok(filter)
    }
    pub async fn console_list(
        &self,
        kind: Listing,
        filter: &ConsoleFilter,
        position: &ConsolePosition,
        limit: u32,
    ) -> Result<Vec<Value>, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let id = existing(&mut tx).await?;
        let rows = match kind {
            Listing::Runs => {
                // Project the error allowlist BEFORE matching. An arbitrary raw
                // provider/storage error must not become a search oracle.
                let public_codes = [
                    "model_unavailable",
                    "unverified_token_bound",
                    "budget_unavailable",
                    "model_rejected",
                    "model_failure",
                    "capability_unavailable",
                    "call_limit",
                    "deadline",
                    "lease_expired",
                    "stale",
                    "cancelled",
                    "queued_context_expired",
                    "goal_owner_changed",
                    "source_unavailable",
                    "invalid_proposal",
                    "model_transient_failure",
                ];
                sqlx::query_scalar(&format!(r#"{VISIBLE}, public_runs AS (
                    SELECT r.*, CASE WHEN r.error_code IS NULL THEN NULL
                        WHEN r.error_code=ANY($10::text[]) THEN r.error_code ELSE 'unclassified_failure' END AS public_error
                    FROM runs r WHERE r.individual_id=$1
                    AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=r.individual_id
                        AND h.id IN (r.individual_id,r.id,r.trigger_event_id))
                ) SELECT jsonb_build_object('id',r.id,'status',r.status,'mode',r.mode,
                    'model_calls',r.model_calls,'tool_calls',r.tool_calls,'created_at',r.created_at,
                    'finished_at',r.finished_at,'error_code',r.public_error,
                    'model_retries',r.model_retries,'repair_attempts',r.repair_attempts)
                FROM public_runs r WHERE
                    ($2::uuid='00000000-0000-0000-0000-000000000000'::uuid OR
                    (r.created_at,r.id)<(COALESCE($3::timestamptz,
                        (SELECT a.created_at FROM public_runs a WHERE a.id=$2)), $2))
                    AND ($4::text IS NULL OR r.status=$4) AND ($5::text IS NULL OR r.mode=$5)
                    AND ($6::timestamptz IS NULL OR r.created_at >= $6)
                    AND ($7::timestamptz IS NULL OR r.created_at < $7)
                    AND (strpos(lower(r.id::text),lower($8))>0 OR strpos(lower(r.public_error),lower($8))>0)
                ORDER BY r.created_at DESC,r.id DESC LIMIT $9"#))
                    .bind(id).bind(position.id).bind(position.created_at).bind(&filter.status)
                    .bind(&filter.mode).bind(filter.from).bind(filter.to).bind(&filter.q)
                    .bind(i64::from(limit)).bind(public_codes.as_slice()).fetch_all(&mut *tx).await?
            }
            Listing::Audit => {
                sqlx::query_scalar(&format!(
                    r#"{VISIBLE}, public_events AS (
                SELECT e.*, CASE WHEN NOT EXISTS(SELECT 1 FROM hidden h
                    WHERE (h.individual_id,h.id)=(e.individual_id,e.correlation_id))
                    THEN e.correlation_id ELSE NULL END AS public_correlation
                FROM events e WHERE e.individual_id=$1 AND e.sequence>$2
                    AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=e.individual_id
                        AND h.id IN (e.individual_id,e.id))
                ) SELECT jsonb_build_object('id',e.id,'sequence',e.sequence,'kind',e.kind,
                    'actor_id',e.actor_id,'source_kind',e.source_kind,'occurred_at',e.occurred_at,
                    'correlation_id',e.public_correlation)
                FROM public_events e WHERE ($3::timestamptz IS NULL OR e.occurred_at >= $3)
                    AND ($4::timestamptz IS NULL OR e.occurred_at < $4)
                    AND (strpos(lower(e.kind),lower($5))>0 OR strpos(lower(e.actor_id),lower($5))>0
                        OR strpos(lower(e.id::text),lower($5))>0
                        OR strpos(lower(e.public_correlation::text),lower($5))>0)
                ORDER BY e.sequence LIMIT $6"#
                ))
                .bind(id)
                .bind(position.sequence)
                .bind(filter.from)
                .bind(filter.to)
                .bind(&filter.q)
                .bind(i64::from(limit))
                .fetch_all(&mut *tx)
                .await?
            }
            _ => return Err(StoreError::Invalid("console listing")),
        };
        tx.commit().await?;
        Ok(rows)
    }
    pub async fn public_operation(&self, operation: Uuid) -> Result<Value, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let id = existing(&mut tx).await?;
        if unavailable(&mut tx, id, &[operation]).await? {
            return Err(StoreError::NotFound);
        }
        let mut view:Value=sqlx::query_scalar("SELECT jsonb_build_object('operation_id',id,'status',status,'mode',mode,'version',state_version,'model_calls',model_calls,'tool_calls',tool_calls,'error_code',error_code,'created_at',created_at,'finished_at',finished_at) FROM runs WHERE individual_id=$1 AND id=$2").bind(id).bind(operation).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let messages:Vec<Uuid>=sqlx::query_scalar(&format!("{VISIBLE} SELECT m.delivered_event_id FROM outgoing_messages m WHERE m.individual_id=$1 AND m.run_id=$2 AND m.status='delivered' AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=m.individual_id AND h.id=m.delivered_event_id)")).bind(id).bind(operation).fetch_all(&mut *tx).await?;
        view["message_ids"] = json!(messages);
        tx.commit().await?;
        Ok(view)
    }
    /// Idempotency is checked before If-Match, so a lost response is replayable.
    pub async fn owner_change(
        &self,
        key: &str,
        expected: i64,
        change: OwnerChange,
    ) -> Result<StateChange, StoreError> {
        let route = match &change {
            OwnerChange::AutonomyPaused(true) => "/autonomy/pause",
            OwnerChange::AutonomyPaused(false) => "/autonomy/resume",
            OwnerChange::Lifecycle(_) => "/lifecycle/transitions",
            OwnerChange::Settings(_) => "/settings",
        };
        let hash = digest(&json!({"expected":expected,"change":change}))?;
        let mut tx = self.write().await?;
        if let Some(r) = replay(&mut tx, "owner", route, key, &hash).await? {
            return Ok(r);
        }
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if unavailable(&mut tx, current.id, &[current.id]).await? {
            return Err(StoreError::NotFound);
        }
        if current.state_version != expected {
            return Err(StoreError::StaleState);
        }
        if current.lifecycle == Lifecycle::Terminated {
            return Err(StoreError::Inactive);
        }
        let event = Uuid::new_v4();
        let version = current.state_version + 1;
        let mut config_version = current.config_version;
        let (kind, payload) = match change {
            OwnerChange::AutonomyPaused(paused) => {
                sqlx::query("UPDATE individuals SET autonomy_paused=$2 WHERE id=$1")
                    .bind(current.id)
                    .bind(paused)
                    .execute(&mut *tx)
                    .await?;
                ("autonomy.changed", json!({"paused":paused}))
            }
            OwnerChange::Lifecycle(command) => {
                let next = current
                    .lifecycle
                    .transition(command)
                    .map_err(|_| StoreError::Invalid("lifecycle transition"))?;
                sqlx::query("UPDATE individuals SET lifecycle=$2 WHERE id=$1")
                    .bind(current.id)
                    .bind(
                        serde_json::to_value(next)?
                            .as_str()
                            .ok_or(StoreError::Invalid("lifecycle"))?,
                    )
                    .execute(&mut *tx)
                    .await?;
                (
                    "lifecycle.changed",
                    json!({"from":current.lifecycle,"to":next}),
                )
            }
            OwnerChange::Settings(value) => {
                let rules = RuntimeRules::validate(
                    value,
                    crate::runtime::capability::IMPLEMENTED_CAPABILITIES,
                )
                .map_err(|_| StoreError::Invalid("runtime configuration"))?;
                config_version += 1;
                sqlx::query(
                    "UPDATE individuals SET runtime_config=$2,config_version=$3 WHERE id=$1",
                )
                .bind(current.id)
                .bind(rules.as_value())
                .bind(config_version)
                .execute(&mut *tx)
                .await?;
                (
                    "settings.changed",
                    json!({"config_version":config_version,"config_digest":digest(rules.as_value())?}),
                )
            }
        };
        insert_event(
            &mut tx,
            current.id,
            event,
            kind,
            "direct_observation",
            "owner",
            payload,
            event,
            Some(key),
        )
        .await?;
        advance(&mut tx, current.id, version).await?;
        outbox(&mut tx, current.id, "state.changed", current.id, version).await?;
        let receipt = StateChange {
            individual_id: current.id,
            version,
            config_version,
        };
        save_receipt(&mut tx, "owner", route, key, &hash, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }
}
