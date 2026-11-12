//! Explicit owner-facing metadata projections. Never serialize raw provider/tool payloads.
use super::{
    api::{VISIBLE, existing},
    goals::unavailable,
    store::*,
};
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) fn safe_failure(code: Option<&str>) -> Option<&'static str> {
    code.map(|code| match code {
        "model_unavailable" => "model_unavailable",
        "unverified_token_bound" => "unverified_token_bound",
        "budget_unavailable" => "budget_unavailable",
        "model_rejected" => "model_rejected",
        "model_failure" => "model_failure",
        "capability_unavailable" => "capability_unavailable",
        "call_limit" => "call_limit",
        "deadline" => "deadline",
        "lease_expired" => "lease_expired",
        "stale" => "stale",
        "cancelled" => "cancelled",
        "queued_context_expired" => "queued_context_expired",
        "goal_owner_changed" => "goal_owner_changed",
        "source_unavailable" => "source_unavailable",
        "invalid_proposal" => "invalid_proposal",
        "model_transient_failure" => "model_transient_failure",
        _ => "unclassified_failure",
    })
}
impl Store {
    pub async fn console_persona(&self) -> Result<Value, StoreError> {
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let id = existing(&mut tx).await?;
        super::development_context::require_public_persona(&mut tx, id).await?;
        let value:Value=sqlx::query_scalar("SELECT jsonb_build_object('individual_id',id,'initial',seed,'current',persona,'state_version',state_version,'history_supported',false,'rollback_supported',false) FROM individuals WHERE id=$1").bind(id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(value)
    }

    pub async fn console_run(&self, run: Uuid) -> Result<Value, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let id = existing(&mut tx).await?;
        if unavailable(&mut tx, id, &[id, run]).await? {
            return Err(StoreError::NotFound);
        }
        let mut view:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'status',status,'mode',mode,'created_at',created_at,'finished_at',finished_at,'model_calls',model_calls,'tool_calls',tool_calls,'error_code',error_code,'model_retries',model_retries,'repair_attempts',repair_attempts,'version',state_version) FROM runs WHERE individual_id=$1 AND id=$2").bind(id).bind(run).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        view["error_code"] = json!(safe_failure(view["error_code"].as_str()));
        let model:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'kind','model','created_at',created_at,'parent_id',run_id,'status',status,'started_at',dispatched_at,'finished_at',settled_at,'duration_ms',CASE WHEN dispatched_at IS NOT NULL AND settled_at IS NOT NULL THEN greatest(0,extract(epoch FROM settled_at-dispatched_at)*1000) ELSE NULL END,'input_tokens',actual_input,'output_tokens',actual_output,'charged_tokens',charged_tokens,'usage_unknown',usage_unknown) FROM usage_reservations WHERE individual_id=$1 AND run_id=$2 ORDER BY created_at,id LIMIT 51").bind(id).bind(run).fetch_all(&mut *tx).await?;
        let tools:Vec<Value>=sqlx::query_scalar(&format!("{VISIBLE} SELECT jsonb_build_object('id',a.id,'kind','tool','created_at',a.created_at,'parent_id',a.run_id,'status',a.status,'sequence',a.sequence,'capability_id',a.capability_id,'capability_version',a.capability_version,'execution_mode',a.execution_mode,'started_at',a.started_at,'finished_at',a.finished_at,'duration_ms',CASE WHEN a.status IN ('succeeded','failed') AND a.started_at IS NOT NULL AND a.finished_at IS NOT NULL THEN greatest(0,extract(epoch FROM a.finished_at-a.started_at)*1000) ELSE NULL END,'result_event_id',a.result_event_id) FROM actions a WHERE a.individual_id=$1 AND a.run_id=$2 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=a.individual_id AND (h.id=a.id OR h.id=a.result_event_id)) ORDER BY a.sequence LIMIT 51")).bind(id).bind(run).fetch_all(&mut *tx).await?;
        let mut spans = model;
        spans.extend(tools);
        spans.sort_by_key(|span| {
            let time = span["started_at"]
                .as_str()
                .or_else(|| span["created_at"].as_str())
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.timestamp_micros());
            (time, span["id"].as_str().unwrap_or_default().to_owned())
        });
        let more = spans.len() > 50;
        spans.truncate(50);
        tx.commit().await?;
        Ok(
            json!({"individual_id":id,"run":view,"spans":spans,"spans_more":more,"payload_policy":"metadata_only","trace_format":"alia-local-execution/1"}),
        )
    }
    pub async fn console_diagnostics(&self) -> Result<Value, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let id = existing(&mut tx).await?;
        let runs:Value=sqlx::query_scalar(&format!("{VISIBLE} SELECT COALESCE(jsonb_object_agg(status,n),'{{}}') FROM (SELECT r.status,count(*) n FROM runs r WHERE r.individual_id=$1 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=r.individual_id AND h.id IN (r.id,r.trigger_event_id)) GROUP BY r.status) q")).bind(id).fetch_one(&mut *tx).await?;
        let migration: i64 = sqlx::query_scalar(
            "SELECT COALESCE(max(version),0) FROM _sqlx_migrations WHERE success",
        )
        .fetch_one(&mut *tx)
        .await?;
        let index_jobs:Value=sqlx::query_scalar(&format!("{VISIBLE} SELECT COALESCE(jsonb_object_agg(status,n),'{{}}') FROM (SELECT j.status,count(*) n FROM memory_index_jobs j JOIN memories m ON (m.individual_id,m.id)=(j.individual_id,j.memory_id) JOIN individuals i ON i.id=j.individual_id WHERE j.individual_id=$1 AND j.profile=$2 AND j.privacy_epoch=i.privacy_epoch AND j.memory_version=m.version AND j.content_digest=encode(sha256(convert_to(m.content,'UTF8')),'hex') AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=j.individual_id AND h.id IN (j.memory_id,j.individual_id)) GROUP BY j.status) q")).bind(id).bind(crate::domain::retrieval::INDEX_PROFILE).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(
            json!({"individual_id":id,"observed_at":chrono::Utc::now(),"database":"reachable","schema_version":migration,"runs":runs,"index_jobs":index_jobs,"raw_logs_available":false}),
        )
    }
}
