use super::store::{Store, StoreError, digest, edge, outbox, state};
use crate::{
    domain::cognition::{ModelContext, RunMode},
    runtime::{
        model::{ProviderMode, TokenUsage},
        scheduling::*,
    },
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::collections::HashSet;
use uuid::Uuid;

type Tx<'a> = Transaction<'a, Postgres>;
fn integer(value: &Value, key: &str) -> Result<i64, StoreError> {
    value[key]
        .as_i64()
        .ok_or(StoreError::Invalid("stored runtime limit"))
}
/// Trial identity belongs to the durable run, not a removable model-context
/// marker. Only the dedicated trial path may reserve or dispatch its requests.
pub(super) async fn require_regular_run(
    tx: &mut Tx<'_>,
    individual: Uuid,
    run: Uuid,
) -> Result<(), StoreError> {
    if super::development_measurements::is_evaluation_run_in(tx, individual, run).await? {
        return Err(StoreError::Invalid("evaluation run requires trial runtime"));
    }
    Ok(())
}
pub(super) async fn require_lease(tx: &mut Tx<'_>, lease: &RunLease) -> Result<(), StoreError> {
    let valid:bool=sqlx::query_scalar(r#"SELECT EXISTS(
SELECT 1 FROM runs r JOIN individuals i ON i.id=r.individual_id
JOIN objects o ON (o.individual_id,o.id)=(r.individual_id,r.id)
JOIN objects trigger ON (trigger.individual_id,trigger.id)=(r.individual_id,r.trigger_event_id)
WHERE r.id=$1 AND r.individual_id=$2 AND r.lease_owner=$3 AND r.lease_generation=$4
AND r.status='running' AND r.lease_expires_at>clock_timestamp() AND r.deadline>clock_timestamp()
AND i.lifecycle='active' AND NOT o.blocked AND NOT trigger.blocked
AND NOT EXISTS(SELECT 1 FROM objects source WHERE source.individual_id=i.id AND (source.id=i.id OR source.id=ANY(r.input_manifest)) AND source.blocked)
AND r.state_version=i.state_version AND r.privacy_epoch=i.privacy_epoch AND r.config_version=i.config_version
AND r.state_version=$5 AND r.privacy_epoch=$6 AND r.config_version=$7 AND r.mode=$8
AND (r.mode='foreground' OR (NOT i.autonomy_paused AND (i.runtime_config->'autonomy'->>'enabled')::boolean)))"#)
        .bind(lease.run_id).bind(lease.individual_id).bind(lease.owner_id).bind(lease.generation)
        .bind(lease.state_version).bind(lease.privacy_epoch).bind(lease.config_version).bind(if lease.mode==RunMode::Foreground{"foreground"}else{"background"}).fetch_one(&mut **tx).await?;
    if !valid {
        return Err(StoreError::LeaseLost);
    }
    let inputs: Vec<Uuid> = sqlx::query_scalar(
        "SELECT input_manifest || ARRAY[individual_id,trigger_event_id] FROM runs WHERE id=$1",
    )
    .bind(lease.run_id)
    .fetch_one(&mut **tx)
    .await?;
    if super::goals::unavailable(tx, lease.individual_id, &inputs).await? {
        return Err(StoreError::LeaseLost);
    }
    super::development_context::require_sources(tx, lease.individual_id, &inputs).await?;
    super::development_context::require_public_persona(tx, lease.individual_id).await?;
    Ok(())
}
async fn receipt(tx: &mut Tx<'_>, id: Uuid) -> Result<Reservation, StoreError> {
    let row: Value = sqlx::query_scalar("SELECT to_jsonb(u) FROM usage_reservations u WHERE id=$1")
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(serde_json::from_value(row)?)
}

// Accounting is independent of lease validity: a stale request still incurred cost.
// Once an unknown call has been charged in full, a late receipt never refunds it.
async fn settle(
    tx: &mut Tx<'_>,
    id: Uuid,
    actual: Option<TokenUsage>,
    release_unsent: bool,
) -> Result<Reservation, StoreError> {
    let row: Value =
        sqlx::query_scalar("SELECT to_jsonb(u) FROM usage_reservations u WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::Invalid("reservation absent"))?;
    let status = row["status"]
        .as_str()
        .ok_or(StoreError::Invalid("reservation status"))?;
    let dispatched = !row["dispatched_at"].is_null();
    if !dispatched && !release_unsent {
        return Err(StoreError::Invalid("request was not dispatched"));
    }
    let actual = actual
        .filter(|u| u.input.checked_add(u.output) == Some(u.total) && u.total <= i64::MAX as u64);
    if status == "settled" && actual.is_none() {
        return receipt(tx, id).await;
    }
    if status == "settled" && !row["actual_total"].is_null() {
        if let Some(u) = actual
            && (row["actual_input"].as_u64() != Some(u.input)
                || row["actual_output"].as_u64() != Some(u.output)
                || row["actual_total"].as_u64() != Some(u.total))
        {
            return Err(StoreError::IdempotencyConflict);
        }
        return receipt(tx, id).await;
    }
    let reserved = integer(&row, "reserved_tokens")?;
    let previous = integer(&row, "charged_tokens")?;
    let unknown = dispatched && (actual.is_none() || row["usage_unknown"] == true);
    let charge = if !dispatched {
        0
    } else if unknown {
        reserved.max(actual.map_or(0, |u| u.total as i64))
    } else {
        actual.map_or(reserved, |u| u.total as i64)
    }
    .max(previous);
    let delta = charge - previous;
    let individual: Uuid = serde_json::from_value(row["individual_id"].clone())?;
    let window: Uuid = serde_json::from_value(row["window_id"].clone())?;
    sqlx::query("UPDATE budget_windows SET charged_total=charged_total+$2,charged_background=charged_background+$3 WHERE id=$1")
        .bind(window).bind(delta).bind(if row["mode"]=="background"{delta}else{0}).execute(&mut **tx).await?;
    sqlx::query("UPDATE usage_reservations SET status='settled',charged_tokens=$2,usage_unknown=$3,actual_input=COALESCE($4,actual_input),actual_output=COALESCE($5,actual_output),actual_total=COALESCE($6,actual_total),settled_at=COALESCE(settled_at,clock_timestamp()) WHERE id=$1")
        .bind(id).bind(charge).bind(unknown).bind(actual.map(|u|u.input as i64)).bind(actual.map(|u|u.output as i64)).bind(actual.map(|u|u.total as i64)).execute(&mut **tx).await?;
    if let Some(u) = actual
        && (u.input > integer(&row, "input_bound")? as u64
            || u.output > integer(&row, "output_cap")? as u64)
    {
        sqlx::query("UPDATE individuals SET budget_frozen=true WHERE id=$1")
            .bind(individual)
            .execute(&mut **tx)
            .await?;
    }
    let version: i64 = sqlx::query_scalar("SELECT state_version FROM individuals WHERE id=$1")
        .bind(individual)
        .fetch_one(&mut **tx)
        .await?;
    outbox(tx, individual, "budget.changed", window, version).await?;
    receipt(tx, id).await
}
pub(super) async fn close_pending(tx: &mut Tx<'_>, run: Uuid) -> Result<(), StoreError> {
    let pending: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM usage_reservations WHERE run_id=$1 AND status IN ('reserved','dispatched')",
    )
    .bind(run)
    .fetch_all(&mut **tx)
    .await?;
    for id in pending {
        settle(tx, id, None, true).await?;
    }
    Ok(())
}

impl Store {
    /// Reconcile orphaned/stale work, then claim one run. All clocks are database
    /// clocks; no model or network operation occurs under the transaction gate.
    pub async fn claim_run(&self, owner: Uuid) -> Result<Option<RunLease>, StoreError> {
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let invalid:Vec<(Uuid,String)>=sqlx::query_as(r#"SELECT r.id,CASE
 WHEN i.lifecycle<>'active' OR (r.mode='background' AND (i.autonomy_paused OR NOT (i.runtime_config->'autonomy'->>'enabled')::boolean)) THEN 'cancelled'
 WHEN r.privacy_epoch<>i.privacy_epoch OR r.config_version<>i.config_version OR r.state_version<>i.state_version OR o.blocked OR trigger.blocked OR EXISTS(SELECT 1 FROM objects source WHERE source.individual_id=i.id AND (source.id=i.id OR source.id=ANY(r.input_manifest)) AND source.blocked) THEN 'stale'
 WHEN r.deadline<=clock_timestamp() THEN 'failed'
 ELSE 'queued' END
FROM runs r JOIN individuals i ON i.id=r.individual_id
JOIN objects o ON (o.individual_id,o.id)=(r.individual_id,r.id)
JOIN objects trigger ON (trigger.individual_id,trigger.id)=(r.individual_id,r.trigger_event_id)
WHERE r.status='running' AND (r.lease_expires_at<=clock_timestamp() OR r.deadline<=clock_timestamp()
 OR r.state_version<>i.state_version OR r.privacy_epoch<>i.privacy_epoch OR r.config_version<>i.config_version
 OR i.lifecycle<>'active' OR o.blocked OR trigger.blocked
 OR EXISTS(SELECT 1 FROM objects source WHERE source.individual_id=i.id AND (source.id=i.id OR source.id=ANY(r.input_manifest)) AND source.blocked)
 OR (r.mode='background' AND (i.autonomy_paused OR NOT (i.runtime_config->'autonomy'->>'enabled')::boolean)))"#).fetch_all(&mut *tx).await?;
        for (id, mut status) in invalid {
            sqlx::query(
                "UPDATE actions SET status='unknown' WHERE run_id=$1 AND status='executing'",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            let unknown: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM actions WHERE run_id=$1 AND status='unknown')",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            if unknown && status == "queued" {
                status = "failed".into();
            }
            if status != "queued" {
                sqlx::query(
                    "UPDATE actions SET status='cancelled' WHERE run_id=$1 AND status='planned'",
                )
                .bind(id)
                .execute(&mut *tx)
                .await?;
            }

            close_pending(&mut tx, id).await?;
            sqlx::query("UPDATE runs SET status=$2,lease_owner=NULL,lease_expires_at=NULL,error_code=CASE WHEN $2='queued' THEN 'lease_expired' ELSE $2 END,finished_at=CASE WHEN $2='queued' THEN NULL ELSE clock_timestamp() END WHERE id=$1")
                .bind(id).bind(&status).execute(&mut *tx).await?;
            outbox(
                &mut tx,
                current.id,
                "operation.updated",
                id,
                current.state_version,
            )
            .await?;
        }
        // Queued, never-started inputs may be rebased onto current knowledge. An
        // already-started cycle must not be silently rebased or reset its counters.
        let invalid_queued:Vec<Uuid>=sqlx::query_scalar(r#"UPDATE runs r SET status='stale',error_code='queued_context_expired',finished_at=clock_timestamp()
FROM individuals i WHERE r.individual_id=i.id AND r.status='queued' AND
(r.privacy_epoch<>i.privacy_epoch OR r.deadline<=clock_timestamp() OR
(r.goal_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM goals g JOIN objects go ON (go.individual_id,go.id)=(g.individual_id,g.id) WHERE g.individual_id=r.individual_id AND g.id=r.goal_id AND g.status IN ('proposed','active') AND NOT go.blocked)) OR
(r.model_calls>0 AND (r.state_version<>i.state_version OR r.config_version<>i.config_version))
 OR EXISTS(SELECT 1 FROM objects source WHERE source.individual_id=i.id AND (source.id=i.id OR source.id=r.id OR source.id=r.trigger_event_id OR source.id=ANY(r.input_manifest)) AND source.blocked)) RETURNING r.id"#).fetch_all(&mut *tx).await?;
        for id in invalid_queued {
            sqlx::query(
                "UPDATE actions SET status='cancelled' WHERE run_id=$1 AND status='planned'",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            close_pending(&mut tx, id).await?;
            outbox(
                &mut tx,
                current.id,
                "operation.updated",
                id,
                current.state_version,
            )
            .await?;
        }
        let running: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE status='running')")
                .fetch_one(&mut *tx)
                .await?;
        if running || current.lifecycle != crate::domain::Lifecycle::Active {
            tx.commit().await?;
            return Ok(None);
        }
        // Expired/closed trial queues belong to the trial maintenance path.
        // Claiming them first would create a fresh lease that prevents their
        // atomic deadline cleanup, even though no provider call is permitted.
        let trial_filter = if super::development_measurements::reference_edges_in(&mut tx)
            .await?
            .is_some()
        {
            "AND NOT EXISTS(SELECT 1 FROM development_trial_cases c JOIN development_trial_plans p ON (p.individual_id,p.id)=(c.individual_id,c.plan_id) WHERE c.individual_id=r.individual_id AND c.evaluation_run_id=r.id AND (p.status<>'active' OR p.deadline<=clock_timestamp()))"
        } else {
            ""
        };
        let selection = format!(
            r#"SELECT r.id,r.mode FROM runs r
JOIN individuals i ON i.id=r.individual_id
JOIN objects o ON (o.individual_id,o.id)=(r.individual_id,r.id)
JOIN objects trigger ON (trigger.individual_id,trigger.id)=(r.individual_id,r.trigger_event_id)
LEFT JOIN goals g ON (g.individual_id,g.id)=(r.individual_id,r.goal_id)
WHERE r.status='queued' AND NOT o.blocked AND NOT trigger.blocked
AND (r.mode='foreground' OR (NOT i.autonomy_paused AND (i.runtime_config->'autonomy'->>'enabled')::boolean))
{trial_filter}
ORDER BY (r.mode='foreground') DESC,COALESCE(g.priority,0) DESC,g.due_at ASC NULLS LAST,r.created_at,r.id
FOR UPDATE OF r SKIP LOCKED LIMIT 1"#
        );
        let selected: Option<(Uuid, String)> =
            sqlx::query_as(&selection).fetch_optional(&mut *tx).await?;
        let Some((id, mode)) = selected else {
            tx.commit().await?;
            return Ok(None);
        };
        let generation:i64=sqlx::query_scalar("UPDATE individuals SET lease_generation=lease_generation+1 WHERE id=$1 RETURNING lease_generation").bind(current.id).fetch_one(&mut *tx).await?;
        let timeout = integer(&current.runtime_config["autonomy"], "cycle_timeout_seconds")?;
        let (expires_at,deadline):(DateTime<Utc>,DateTime<Utc>)=sqlx::query_as("UPDATE runs SET status='running',lease_owner=$2,lease_generation=$3,deadline=COALESCE(deadline,clock_timestamp()+make_interval(secs=>$4)),lease_expires_at=LEAST(COALESCE(deadline,clock_timestamp()+make_interval(secs=>$4)),clock_timestamp()+interval '30 seconds'),state_version=$5,privacy_epoch=$6,config_version=$7,error_code=NULL WHERE id=$1 RETURNING lease_expires_at,deadline")
            .bind(id).bind(owner).bind(generation).bind(timeout as f64).bind(current.state_version).bind(current.privacy_epoch).bind(current.config_version).fetch_one(&mut *tx).await?;
        outbox(
            &mut tx,
            current.id,
            "operation.updated",
            id,
            current.state_version,
        )
        .await?;
        tx.commit().await?;
        Ok(Some(RunLease {
            individual_id: current.id,
            run_id: id,
            owner_id: owner,
            generation,
            expires_at,
            deadline,
            state_version: current.state_version,
            privacy_epoch: current.privacy_epoch,
            config_version: current.config_version,
            mode: if mode == "foreground" {
                RunMode::Foreground
            } else {
                RunMode::Background
            },
        }))
    }

    pub async fn remaining_cycle_time(
        &self,
        lease: &RunLease,
    ) -> Result<std::time::Duration, StoreError> {
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        let seconds:f64=sqlx::query_scalar("SELECT GREATEST(EXTRACT(epoch FROM deadline-clock_timestamp()),0)::double precision FROM runs WHERE id=$1")
            .bind(lease.run_id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(std::time::Duration::from_secs_f64(seconds))
    }

    pub async fn renew_lease(&self, lease: &RunLease) -> Result<DateTime<Utc>, StoreError> {
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        let until=sqlx::query_scalar("UPDATE runs SET lease_expires_at=LEAST(deadline,clock_timestamp()+interval '30 seconds') WHERE id=$1 RETURNING lease_expires_at")
            .bind(lease.run_id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(until)
    }

    pub async fn finish_run(
        &self,
        lease: &RunLease,
        outcome: RunOutcome,
    ) -> Result<(), StoreError> {
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        close_pending(&mut tx, lease.run_id).await?;
        sqlx::query("UPDATE actions SET status=CASE WHEN status='executing' THEN 'unknown' ELSE 'cancelled' END WHERE run_id=$1 AND status IN ('planned','executing')").bind(lease.run_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE runs SET status=$2,finished_at=clock_timestamp(),lease_owner=NULL,lease_expires_at=NULL WHERE id=$1")
            .bind(lease.run_id).bind(outcome.as_str()).execute(&mut *tx).await?;
        outbox(
            &mut tx,
            lease.individual_id,
            "operation.updated",
            lease.run_id,
            lease.state_version,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Reserve a validated adapter bound and snapshot provenance before any HTTP
    /// request. The supplied context must have been assembled by this runtime.
    pub async fn reserve_model_call(
        &self,
        lease: &RunLease,
        key: Uuid,
        context: &ModelContext,
        allowance: TokenAllowance,
    ) -> Result<Reservation, StoreError> {
        let total = allowance
            .input_upper_bound
            .checked_add(u64::from(allowance.output_cap))
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or(StoreError::Invalid("token allowance"))? as i64;
        if allowance.input_upper_bound == 0 || !(1..=8192).contains(&allowance.output_cap) {
            return Err(StoreError::Invalid("token allowance"));
        }
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        let trial = context.trial_binding.is_some();
        if trial {
            super::development_trial_context::require_context_in(
                &mut tx,
                context,
                super::development_trial_context::TrialContextPhase::Reservation,
            )
            .await?;
        } else {
            require_regular_run(&mut tx, lease.individual_id, lease.run_id).await?;
        }
        if context.individual_id != lease.individual_id
            || context.run_id != lease.run_id
            || context.state_version != lease.state_version
            || context.privacy_epoch != lease.privacy_epoch
            || context.config_version != lease.config_version
            || context.mode != lease.mode
        {
            return Err(StoreError::StaleState);
        }
        if context
            .social_context
            .as_ref()
            .is_some_and(|s| !s.current_at(chrono::Utc::now()))
        {
            return Err(StoreError::StaleState);
        }
        if context
            .selected_beliefs
            .as_ref()
            .is_some_and(|b| !b.valid_for(context))
            || context
                .autobiographical_context
                .as_ref()
                .is_some_and(|a| !a.valid_for(context))
        {
            return Err(StoreError::Invalid("cognitive context evidence"));
        }
        if super::goals::unavailable(&mut tx, lease.individual_id, &context.input_manifest).await? {
            return Err(StoreError::StaleState);
        }
        let hash = digest(&json!({"context":context,"allowance":allowance}))?;
        let previous:Option<(Uuid,String,i64)>=sqlx::query_as("SELECT id,request_digest,lease_generation FROM usage_reservations WHERE individual_id=$1 AND run_id=$2 AND request_key=$3")
            .bind(lease.individual_id).bind(lease.run_id).bind(key).fetch_optional(&mut *tx).await?;
        if let Some((_, previous_hash, generation)) = &previous
            && (previous_hash != &hash || *generation != lease.generation)
        {
            return Err(StoreError::IdempotencyConflict);
        }
        if trial {
            // The dedicated adapter has validated the exact prebound baseline
            // or sole permitted overlay. It does not authorize normal writes.
        } else if let Some(review) = &context.decision_review {
            let mut author = context.clone();
            author.decision_review = None;
            super::grounding::accounted(&mut tx, lease, review.author_reservation_id, &author)
                .await?;
            super::development_context::require_context(&mut tx, context).await?;
        } else if previous.is_some() {
            // The actual persisted request binds this exact context and allowance,
            // even before dispatch. Its own reservation may have expanded run
            // ancestry; revalidate the original snapshot and real provenance graph.
            super::development_context::require_context(&mut tx, context).await?;
        } else {
            super::development_context::require_author_context(&mut tx, context).await?;
        }
        if let Some((id, _, _)) = previous {
            if trial {
                super::development_trial_context::bind_reservation_in(
                    &mut tx, lease, context, key, id, allowance,
                )
                .await?;
                let result = receipt(&mut tx, id).await?;
                tx.commit().await?;
                return Ok(result);
            }
            return receipt(&mut tx, id).await;
        }
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals WHERE id=$1")
            .bind(lease.individual_id)
            .fetch_one(&mut *tx)
            .await?;
        if frozen {
            return Err(StoreError::BudgetUnavailable);
        }
        let calls: i32 = sqlx::query_scalar("SELECT model_calls FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&mut *tx)
            .await?;
        let pending_actions:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM actions WHERE run_id=$1 AND status IN ('planned','executing','unknown'))").bind(lease.run_id).fetch_one(&mut *tx).await?;
        if pending_actions {
            return Err(StoreError::Invalid("action results pending"));
        }
        let outstanding:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM usage_reservations WHERE run_id=$1 AND status IN ('reserved','dispatched'))").bind(lease.run_id).fetch_one(&mut *tx).await?;
        if outstanding
            || i64::from(calls)
                >= integer(
                    &current.runtime_config["autonomy"],
                    "max_model_calls_per_cycle",
                )?
        {
            return Err(StoreError::CallLimit);
        }
        let inputs: HashSet<_> = context.input_manifest.iter().copied().collect();
        if inputs.len() != context.input_manifest.len()
            || !inputs.contains(&context.current_event_id)
            || !inputs.contains(&lease.individual_id)
            || !inputs.contains(&lease.run_id)
        {
            return Err(StoreError::Invalid("context manifest"));
        }
        let referenced = context
            .events
            .iter()
            .map(|e| e.id)
            .chain(context.memories.iter().map(|m| m.id))
            .chain(
                context
                    .selected_beliefs
                    .iter()
                    .flat_map(|b| b.items.iter().map(|b| b.id)),
            )
            .chain(
                context
                    .autobiographical_context
                    .iter()
                    .flat_map(|s| s.entries.iter().map(|a| a.id)),
            )
            .chain(
                context
                    .memories
                    .iter()
                    .flat_map(|m| m.source_event_ids.iter().copied()),
            )
            .chain(context.goals.iter().map(|g| g.id));
        if referenced.into_iter().any(|id| !inputs.contains(&id)) {
            return Err(StoreError::Invalid("incomplete context manifest"));
        }
        if let Some(subject) = context.relationship["subject_id"].as_str()
            && !Uuid::parse_str(subject)
                .ok()
                .is_some_and(|id| inputs.contains(&id))
        {
            return Err(StoreError::Invalid("relationship manifest"));
        }
        if context
            .social_context
            .as_ref()
            .is_some_and(|s| !s.valid_for(context))
        {
            return Err(StoreError::Invalid("social context manifest"));
        }
        let available: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM objects WHERE individual_id=$1 AND id=ANY($2) AND NOT blocked",
        )
        .bind(lease.individual_id)
        .bind(&context.input_manifest)
        .fetch_one(&mut *tx)
        .await?;
        if available != inputs.len() as i64 {
            return Err(StoreError::StaleState);
        }
        super::reviews::validate_context(&mut tx, context).await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let latest:Option<(Uuid,DateTime<Utc>,DateTime<Utc>)>=sqlx::query_as("SELECT id,starts_at,ends_at FROM budget_windows WHERE individual_id=$1 ORDER BY starts_at DESC LIMIT 1")
            .bind(lease.individual_id).fetch_optional(&mut *tx).await?;
        let window = match latest {
            Some((_, start, _)) if now < start => {
                return Err(StoreError::Invalid("billing clock moved backwards"));
            }
            Some((id, _, end)) if now < end => id,
            previous => {
                let timezone = current.runtime_config["contact"]["timezone"]
                    .as_str()
                    .ok_or(StoreError::Invalid("timezone"))?;
                let (start,end):(DateTime<Utc>,DateTime<Utc>)=sqlx::query_as("SELECT date_trunc('day',$1::timestamptz AT TIME ZONE $2) AT TIME ZONE $2,(date_trunc('day',$1::timestamptz AT TIME ZONE $2)+interval '1 day') AT TIME ZONE $2")
                    .bind(now).bind(timezone).fetch_one(&mut *tx).await?;
                let start = previous.map_or(start, |(_, _, last_end)| start.max(last_end));
                if end <= now || start > now {
                    return Err(StoreError::Invalid("billing window"));
                }
                let id = Uuid::new_v4();
                sqlx::query("INSERT INTO budget_windows(id,individual_id,timezone,starts_at,ends_at) VALUES($1,$2,$3,$4,$5)")
                    .bind(id).bind(lease.individual_id).bind(timezone).bind(start).bind(end).execute(&mut *tx).await?;
                id
            }
        };
        let (used_total, used_bg): (i64, i64) = sqlx::query_as(
            "SELECT charged_total,charged_background FROM budget_windows WHERE id=$1",
        )
        .bind(window)
        .fetch_one(&mut *tx)
        .await?;
        let (pending_total,pending_bg):(i64,i64)=sqlx::query_as("SELECT COALESCE(sum(reserved_tokens),0)::bigint,COALESCE(sum(reserved_tokens) FILTER (WHERE mode='background'),0)::bigint FROM usage_reservations WHERE window_id=$1 AND status IN ('reserved','dispatched')")
            .bind(window).fetch_one(&mut *tx).await?;
        let limit = integer(&current.runtime_config["budgets"], "total_tokens_per_day")?;
        let background_limit = integer(
            &current.runtime_config["budgets"],
            "background_tokens_per_day",
        )?;
        if total
            > limit
                .saturating_sub(used_total)
                .saturating_sub(pending_total)
            || (lease.mode == RunMode::Background
                && total
                    > background_limit
                        .saturating_sub(used_bg)
                        .saturating_sub(pending_bg))
        {
            return Err(StoreError::BudgetUnavailable);
        }
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO usage_reservations(id,individual_id,run_id,lease_generation,request_key,request_digest,window_id,mode,input_bound,output_cap,reserved_tokens,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'reserved')")
            .bind(id).bind(lease.individual_id).bind(lease.run_id).bind(lease.generation).bind(key).bind(hash).bind(window)
            .bind(if lease.mode==RunMode::Background{"background"}else{"foreground"}).bind(allowance.input_upper_bound as i64).bind(i64::from(allowance.output_cap)).bind(total).execute(&mut *tx).await?;
        sqlx::query("UPDATE runs SET input_manifest=ARRAY(SELECT DISTINCT unnest(input_manifest || $2::uuid[])) WHERE id=$1")
            .bind(lease.run_id).bind(&context.input_manifest).execute(&mut *tx).await?;
        for source in &inputs {
            if *source != lease.run_id {
                edge(&mut tx, lease.individual_id, *source, lease.run_id).await?;
            }
        }
        if trial {
            super::development_trial_context::bind_reservation_in(
                &mut tx, lease, context, key, id, allowance,
            )
            .await?;
        }
        outbox(
            &mut tx,
            lease.individual_id,
            "budget.changed",
            window,
            lease.state_version,
        )
        .await?;
        let result = receipt(&mut tx, id).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Returns true exactly once. A false result must never issue another request.
    /// A crash after this marker is conservatively treated as a possibly sent call.
    pub async fn dispatch_model_call(
        &self,
        lease: &RunLease,
        reservation: Uuid,
    ) -> Result<bool, StoreError> {
        self.dispatch_model_call_inner(lease, reservation, None)
            .await
    }

    /// Internal runtime entry: the actual adapter mode and prebound request are
    /// attested in the *same* transaction as the original dispatch marker.
    pub(crate) async fn dispatch_model_call_with_context(
        &self,
        lease: &RunLease,
        reservation: Uuid,
        context: &ModelContext,
        mode: ProviderMode,
    ) -> Result<bool, StoreError> {
        self.dispatch_model_call_inner(lease, reservation, Some((context, mode)))
            .await
    }

    async fn dispatch_model_call_inner(
        &self,
        lease: &RunLease,
        reservation: Uuid,
        request: Option<(&ModelContext, ProviderMode)>,
    ) -> Result<bool, StoreError> {
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        let trial = request.is_some_and(|(context, _)| context.trial_binding.is_some());
        if let Some((context, _)) = request.filter(|_| trial) {
            super::development_trial_context::require_context_in(
                &mut tx,
                context,
                super::development_trial_context::TrialContextPhase::Dispatch,
            )
            .await?;
        } else {
            require_regular_run(&mut tx, lease.individual_id, lease.run_id).await?;
        }
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT status,lease_generation FROM usage_reservations WHERE id=$1 AND run_id=$2",
        )
        .bind(reservation)
        .bind(lease.run_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((status, generation)) = row else {
            return Err(StoreError::Invalid("reservation absent"));
        };
        if generation != lease.generation {
            return Err(StoreError::LeaseLost);
        }
        if status != "reserved" {
            return Ok(false);
        }
        let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals WHERE id=$1")
            .bind(lease.individual_id)
            .fetch_one(&mut *tx)
            .await?;
        if frozen {
            return Err(StoreError::BudgetUnavailable);
        }
        if let Some((_, mode)) = request.filter(|_| trial) {
            super::development_measurements::attest_development_trial_dispatch_in(
                &mut tx,
                lease,
                reservation,
                mode,
            )
            .await?;
        }
        sqlx::query("UPDATE usage_reservations SET status='dispatched',dispatched_at=clock_timestamp() WHERE id=$1").bind(reservation).execute(&mut *tx).await?;
        sqlx::query("UPDATE runs SET model_calls=model_calls+1 WHERE id=$1")
            .bind(lease.run_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn settle_model_call(
        &self,
        id: Uuid,
        usage: Option<TokenUsage>,
    ) -> Result<Reservation, StoreError> {
        let mut tx = self.write().await?;
        let result = settle(&mut tx, id, usage, false).await?;
        tx.commit().await?;
        Ok(result)
    }
}
