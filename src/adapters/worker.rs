use super::{
    scheduling::{close_pending, require_lease},
    store::{Store, StoreError, advance, edge, insert_event, outbox, state},
};
use crate::{
    domain::cognition::RunMode,
    runtime::{
        scheduling::RunLease,
        worker::{CycleProgress, RetryCause, StopReason},
    },
};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

impl Store {
    pub async fn cycle_progress(&self, lease: &RunLease) -> Result<CycleProgress, StoreError> {
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        let (calls,limit,delay):(i32,i32,f64)=sqlx::query_as("SELECT r.model_calls,(i.runtime_config->'autonomy'->>'max_model_calls_per_cycle')::integer,GREATEST(COALESCE(extract(epoch FROM r.retry_after-clock_timestamp()),0),0)::float8 FROM runs r JOIN individuals i ON i.id=r.individual_id WHERE r.id=$1").bind(lease.run_id).fetch_one(&mut *tx).await?;
        let actions:Vec<(Uuid,String)>=sqlx::query_as("SELECT id,capability_id FROM actions WHERE run_id=$1 AND status='planned' ORDER BY sequence").bind(lease.run_id).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(CycleProgress {
            model_calls: calls,
            model_limit: limit,
            actions,
            retry_delay: Duration::from_secs_f64(delay),
        })
    }
    /// Retry metadata and delay survive a worker crash; no provider output is stored.
    pub async fn schedule_model_retry(
        &self,
        lease: &RunLease,
        cause: RetryCause,
    ) -> Result<Option<i64>, StoreError> {
        self.schedule_retry(lease, cause, None).await
    }
    pub(crate) async fn schedule_review_retry(
        &self,
        lease: &RunLease,
        cause: RetryCause,
        author: Uuid,
    ) -> Result<Option<i64>, StoreError> {
        self.schedule_retry(lease, cause, Some(author)).await
    }
    async fn schedule_retry(
        &self,
        lease: &RunLease,
        cause: RetryCause,
        author: Option<Uuid>,
    ) -> Result<Option<i64>, StoreError> {
        let repair = cause.repair();
        let (validation_code, instruction) = cause.feedback();
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        super::scheduling::require_regular_run(&mut tx, lease.individual_id, lease.run_id).await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if let Some(author) = author {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM usage_reservations WHERE id=$1 AND individual_id=$2 AND run_id=$3 AND lease_generation=$4 AND status='settled')").bind(author).bind(lease.individual_id).bind(lease.run_id).bind(lease.generation).fetch_one(&mut *tx).await?;
            if !valid {
                return Err(StoreError::Invalid("review author reservation"));
            }
        }
        let retried:Option<i32>=sqlx::query_scalar("UPDATE runs SET model_retries=model_retries+1,repair_attempts=repair_attempts+CASE WHEN $2 THEN 1 ELSE 0 END,retry_after=clock_timestamp()+make_interval(secs=>power(2,model_retries)::float8) WHERE id=$1 AND model_retries<2 AND (NOT $2 OR repair_attempts<1) AND model_calls<(SELECT (runtime_config->'autonomy'->>'max_model_calls_per_cycle')::integer FROM individuals WHERE id=$3) RETURNING model_retries")
            .bind(lease.run_id).bind(repair).bind(current.id).fetch_optional(&mut *tx).await?;
        let Some(attempt) = retried else {
            return Ok(None);
        };
        let event = Uuid::new_v4();
        let version = current.state_version + i64::from(author.is_none());
        insert_event(&mut tx,current.id,event,if author.is_some(){"decision.review_retry_scheduled"}else{"run.retry_scheduled"},"direct_observation","runtime",json!({"attempt":attempt,"code":if repair{"invalid_proposal"}else{"model_transient_failure"},"validation_code":validation_code,"instruction":instruction,"role":if author.is_some(){"content_reviewer"}else{"author"},"author_reservation_id":author}),lease.run_id,None).await?;
        let sources: Vec<Uuid> = sqlx::query_scalar("SELECT input_manifest FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&mut *tx)
            .await?;
        for source in sources {
            edge(&mut tx, current.id, source, event).await?;
        }
        require_lease(&mut tx, lease).await?;
        if author.is_none() {
            sqlx::query("UPDATE runs SET state_version=$2,input_manifest=array_append(input_manifest,$3) WHERE id=$1").bind(lease.run_id).bind(version).bind(event).execute(&mut *tx).await?;
            advance(&mut tx, current.id, version).await?;
        } else {
            outbox(
                &mut tx,
                current.id,
                "operation.updated",
                lease.run_id,
                version,
            )
            .await?;
        }
        tx.commit().await?;
        Ok(Some(version))
    }
    /// Terminal runtime errors are observations, never a fabricated model reply.
    /// Resource waits do not count as personality regression or goal achievement.
    pub async fn stop_cycle(&self, lease: &RunLease, reason: StopReason) -> Result<(), StoreError> {
        self.stop_cycle_with_feedback(lease, reason, None).await
    }
    /// Preserve the final fixed diagnostic even when no repair call is left.
    pub(crate) async fn stop_cycle_with_feedback(
        &self,
        lease: &RunLease,
        reason: StopReason,
        feedback: Option<RetryCause>,
    ) -> Result<(), StoreError> {
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let evaluation = super::development_measurements::is_evaluation_run_in(
            &mut tx,
            lease.individual_id,
            lease.run_id,
        )
        .await?;
        close_pending(&mut tx, lease.run_id).await?;
        let version = current.state_version + i64::from(!evaluation);
        let event = Uuid::new_v4();
        let actual:Vec<Uuid>=sqlx::query_scalar("SELECT result_event_id FROM actions WHERE run_id=$1 AND result_event_id IS NOT NULL ORDER BY sequence").bind(lease.run_id).fetch_all(&mut *tx).await?;
        let mut payload = json!({"code":reason.code(),"message":reason.message(),"result_event_ids":actual,"mode":lease.mode});
        if reason == StopReason::ModelRejected
            && let Some(feedback) = feedback
        {
            let (code, instruction) = feedback.feedback();
            payload["validation_code"] = json!(code);
            payload["instruction"] = json!(instruction);
        }
        insert_event(
            &mut tx,
            current.id,
            event,
            if evaluation {
                "development.trial_run_stopped"
            } else {
                "run.stopped"
            },
            "direct_observation",
            "runtime",
            payload,
            lease.run_id,
            None,
        )
        .await?;
        let sources: Vec<Uuid> = sqlx::query_scalar("SELECT input_manifest FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&mut *tx)
            .await?;
        for source in sources {
            edge(&mut tx, current.id, source, event).await?;
        }
        sqlx::query("UPDATE actions SET status=CASE WHEN status='executing' THEN 'unknown' ELSE 'cancelled' END WHERE run_id=$1 AND status IN ('planned','executing')").bind(lease.run_id).execute(&mut *tx).await?;
        if !evaluation && lease.mode == RunMode::Background && reason.waits_for_resource() {
            let goal:Option<Uuid>=sqlx::query_scalar("UPDATE goals SET status='waiting',waiting_reason=$2,due_at=NULL,resume_at=CASE WHEN $2='budget_unavailable' THEN CASE WHEN (SELECT budget_frozen FROM individuals WHERE id=$1) THEN NULL ELSE COALESCE((SELECT max(ends_at) FROM budget_windows WHERE individual_id=$1 AND ends_at>clock_timestamp()),(SELECT (date_trunc('day',clock_timestamp() AT TIME ZONE (runtime_config->'contact'->>'timezone'))+interval '1 day') AT TIME ZONE (runtime_config->'contact'->>'timezone') FROM individuals WHERE id=$1)) END ELSE NULL END,version=version+1,review_revision=review_revision+1 WHERE individual_id=$1 AND id=(SELECT goal_id FROM runs WHERE id=$3) AND status IN ('proposed','active') RETURNING id").bind(current.id).bind(reason.code()).bind(lease.run_id).fetch_optional(&mut *tx).await?;
            if let Some(goal) = goal {
                edge(&mut tx, current.id, event, goal).await?;
                outbox(&mut tx, current.id, "goal.changed", goal, version).await?;
            }
        }
        require_lease(&mut tx, lease).await?;
        sqlx::query("UPDATE runs SET status='failed',error_code=$2,finished_at=clock_timestamp(),state_version=$3,lease_owner=NULL,lease_expires_at=NULL WHERE id=$1").bind(lease.run_id).bind(reason.code()).bind(version).execute(&mut *tx).await?;
        if !evaluation {
            advance(&mut tx, current.id, version).await?;
        }
        outbox(
            &mut tx,
            current.id,
            "operation.updated",
            lease.run_id,
            version,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn pending_contacts(&self) -> Result<Vec<Uuid>, StoreError> {
        Ok(sqlx::query_scalar("SELECT m.draft_event_id FROM outgoing_messages m JOIN events e ON (e.individual_id,e.id)=(m.individual_id,m.draft_event_id) WHERE m.mode='background' AND m.status='pending' ORDER BY e.sequence LIMIT 8").fetch_all(&self.pool).await?)
    }
    pub async fn idle_delay(&self) -> Result<Duration, StoreError> {
        // Idle exploration may be hourly; lease recovery must not wait an hour.
        // Only future goal dates participate: inaccessible overdue goals must
        // not turn a low-frequency idle check into a busy loop.
        let seconds:Option<f64>=sqlx::query_scalar(r#"SELECT LEAST(
 (i.runtime_config->'autonomy'->>'idle_check_minutes')::float8*60,
 COALESCE((SELECT GREATEST(COALESCE(extract(epoch FROM min(LEAST(r.lease_expires_at,r.deadline))-clock_timestamp()),86400),1)::float8 FROM runs r WHERE r.individual_id=i.id AND r.status='running'),86400),
 CASE WHEN i.autonomy_paused OR NOT (i.runtime_config->'autonomy'->>'enabled')::boolean THEN 86400 ELSE
 COALESCE((SELECT extract(epoch FROM min(CASE WHEN g.status='waiting' THEN g.resume_at ELSE g.due_at END)-clock_timestamp())::float8 FROM goals g WHERE g.individual_id=i.id AND ((g.status IN ('proposed','active') AND g.due_at>clock_timestamp()) OR (g.status='waiting' AND g.waiting_reason='budget_unavailable' AND NOT i.budget_frozen AND g.resume_at>clock_timestamp()))),86400) END,
 CASE WHEN EXISTS(SELECT 1 FROM outgoing_messages m WHERE m.individual_id=i.id AND m.mode='background' AND m.status='pending') THEN 60 ELSE 86400 END
) FROM individuals i WHERE singleton"#).fetch_optional(&self.pool).await?;
        Ok(Duration::from_secs_f64(
            seconds.unwrap_or(60.0).clamp(1.0, 86400.0),
        ))
    }
}
