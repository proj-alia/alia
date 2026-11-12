//! Owner goals share the scheduler's state, provenance and optimistic write gate.
use super::{
    goals::unavailable,
    store::{
        Store, StoreError, advance, digest, edge, insert_event, object, outbox, replay,
        save_receipt, state,
    },
};
use crate::domain::Lifecycle;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalDraft {
    pub description: String,
    pub success_criterion: String,
    pub next_step: String,
    pub stop_condition: String,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalAction {
    Activate,
    Wait,
    Abandon,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalCommand {
    pub action: GoalAction,
    pub reason: String,
    #[serde(default)]
    pub evidence_ids: Vec<Uuid>,
}
#[derive(Serialize, Deserialize)]
pub struct GoalReceipt {
    pub individual_id: Uuid,
    pub goal_id: Uuid,
    pub event_id: Uuid,
    pub version: i64,
    pub goal_version: i64,
    pub status: String,
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.chars().count() <= 2000 && !s.contains('\0')
}
impl Store {
    pub async fn create_owner_goal(
        &self,
        key: &str,
        draft: &GoalDraft,
    ) -> Result<GoalReceipt, StoreError> {
        if ![
            &draft.description,
            &draft.success_criterion,
            &draft.next_step,
            &draft.stop_condition,
        ]
        .into_iter()
        .all(|s| text(s))
        {
            return Err(StoreError::Invalid(
                "goal text must contain 1..2000 characters",
            ));
        }
        let hash = digest(draft)?;
        let mut tx = self.write().await?;
        if let Some(r) = replay::<GoalReceipt>(&mut tx, "owner", "/goals", key, &hash).await? {
            if unavailable(
                &mut tx,
                r.individual_id,
                &[r.individual_id, r.goal_id, r.event_id],
            )
            .await?
            {
                return Err(StoreError::NotFound);
            }
            return Ok(r);
        }
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if !matches!(current.lifecycle, Lifecycle::Active | Lifecycle::Paused) {
            return Err(StoreError::Inactive);
        }
        if unavailable(&mut tx, current.id, &[current.id]).await? {
            return Err(StoreError::NotFound);
        }
        let goal = Uuid::new_v4();
        let event = Uuid::new_v4();
        let version = current.state_version + 1;
        insert_event(
            &mut tx,
            current.id,
            event,
            "user.goal_requested",
            "reported_claim",
            "owner",
            json!({"goal_id":goal,"request":draft}),
            event,
            Some(key),
        )
        .await?;
        object(&mut tx, current.id, goal, "goal").await?;
        sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition,due_at) VALUES($1,$2,'event',$3,$4,$5,$6,$7,clock_timestamp())")
            .bind(current.id).bind(goal).bind(json!([event])).bind(&draft.description).bind(&draft.success_criterion).bind(&draft.next_step).bind(&draft.stop_condition).execute(&mut *tx).await?;
        edge(&mut tx, current.id, event, goal).await?;
        advance(&mut tx, current.id, version).await?;
        outbox(&mut tx, current.id, "goal.changed", goal, version).await?;
        let r = GoalReceipt {
            individual_id: current.id,
            goal_id: goal,
            event_id: event,
            version,
            goal_version: 1,
            status: "proposed".into(),
        };
        save_receipt(&mut tx, "owner", "/goals", key, &hash, &r).await?;
        tx.commit().await?;
        Ok(r)
    }
    pub async fn change_owner_goal(
        &self,
        key: &str,
        goal: Uuid,
        expected: i64,
        command: &GoalCommand,
    ) -> Result<GoalReceipt, StoreError> {
        if !text(&command.reason)
            || command.evidence_ids.len() > 16
            || command
                .evidence_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != command.evidence_ids.len()
        {
            return Err(StoreError::Invalid(
                "goal transition reason/evidence invalid",
            ));
        }
        let route = format!("/goals/{goal}/transitions");
        let hash = digest(&json!({"expected":expected,"command":command}))?;
        let mut tx = self.write().await?;
        if let Some(r) = replay::<GoalReceipt>(&mut tx, "owner", &route, key, &hash).await? {
            if unavailable(
                &mut tx,
                r.individual_id,
                &[r.individual_id, r.goal_id, r.event_id],
            )
            .await?
            {
                return Err(StoreError::NotFound);
            }
            return Ok(r);
        }
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if !matches!(current.lifecycle, Lifecycle::Active | Lifecycle::Paused) {
            return Err(StoreError::Inactive);
        }
        if unavailable(&mut tx, current.id, &[current.id, goal]).await? {
            return Err(StoreError::NotFound);
        }
        let (previous,goal_version,count,waiting):(String,i64,i32,Option<i64>)=sqlx::query_as("SELECT status,version,no_progress_cycles,waiting_since_sequence FROM goals WHERE individual_id=$1 AND id=$2").bind(current.id).bind(goal).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        if goal_version != expected {
            return Err(StoreError::StaleState);
        }
        let (next, valid) = match command.action {
            GoalAction::Activate => (
                "active",
                matches!(previous.as_str(), "proposed" | "waiting"),
            ),
            GoalAction::Wait => (
                "waiting",
                matches!(previous.as_str(), "proposed" | "active"),
            ),
            GoalAction::Abandon => (
                "abandoned",
                matches!(previous.as_str(), "proposed" | "active" | "waiting"),
            ),
        };
        if !valid {
            return Err(StoreError::Invalid("goal transition not allowed"));
        }
        if unavailable(&mut tx, current.id, &command.evidence_ids).await? {
            return Err(StoreError::NotFound);
        }
        // Evidence must be an existing external observation, not the transition
        // itself, a previous plan, model self-report or fabricated object ID.
        let evidence_count:i64=sqlx::query_scalar("SELECT count(*) FROM events WHERE individual_id=$1 AND id=ANY($2) AND (kind IN ('message.received','document.imported') OR (kind='tool.result' AND source_kind='external_document'))")
            .bind(current.id).bind(&command.evidence_ids).fetch_one(&mut *tx).await?;
        if evidence_count != command.evidence_ids.len() as i64 {
            return Err(StoreError::Invalid("goal evidence must be external events"));
        }
        if next == "active" && count >= 3 {
            let fresh:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE individual_id=$1 AND id=ANY($2) AND sequence>$3)").bind(current.id).bind(&command.evidence_ids).bind(waiting).fetch_one(&mut *tx).await?;
            if !fresh {
                return Err(StoreError::Invalid("goal needs new external evidence"));
            }
        }
        let event = Uuid::new_v4();
        let version = current.state_version + 1;
        insert_event(&mut tx,current.id,event,"goal.owner_changed","reported_claim","owner",json!({"goal_id":goal,"from":previous,"to":next,"reason":command.reason,"evidence_ids":command.evidence_ids}),event,Some(key)).await?;
        edge(&mut tx, current.id, goal, event).await?;
        for id in &command.evidence_ids {
            edge(&mut tx, current.id, *id, event).await?;
        }
        edge(&mut tx, current.id, event, goal).await?;
        // Retain the no-progress barrier across manual wait/resume. A reason by
        // itself cannot erase it; only newly observed evidence can reset it.
        sqlx::query("UPDATE goals SET status=$3,version=version+1,review_revision=review_revision+1,waiting_reason=CASE WHEN $3='waiting' THEN 'owner_wait' ELSE NULL END,resume_at=NULL,due_at=CASE WHEN $3='active' THEN clock_timestamp() ELSE NULL END,no_progress_cycles=CASE WHEN $3='active' AND no_progress_cycles>=3 THEN 0 ELSE no_progress_cycles END,waiting_since_sequence=CASE WHEN $3='active' THEN NULL ELSE waiting_since_sequence END WHERE individual_id=$1 AND id=$2")
            .bind(current.id).bind(goal).bind(next).execute(&mut *tx).await?;
        // Do not rebase an older queued occurrence after a quick wait/reactivate.
        // Running work is fenced by the global version and accounted by recovery.
        let queued:Vec<Uuid>=sqlx::query_scalar("UPDATE runs SET status='stale',error_code='goal_owner_changed',finished_at=clock_timestamp() WHERE individual_id=$1 AND goal_id=$2 AND status='queued' RETURNING id")
            .bind(current.id).bind(goal).fetch_all(&mut *tx).await?;
        for run in queued {
            super::scheduling::close_pending(&mut tx, run).await?;
            sqlx::query("UPDATE actions SET status='cancelled' WHERE individual_id=$1 AND run_id=$2 AND status='planned'").bind(current.id).bind(run).execute(&mut *tx).await?;
            outbox(&mut tx, current.id, "operation.updated", run, version).await?;
        }
        // The owner explicitly changed the next step's lifecycle. Close older
        // goal-cycle accounting so a delayed evaluator cannot postpone the new
        // activation or increment its no-progress counter. Runs/results remain.
        sqlx::query("INSERT INTO goal_cycles(individual_id,run_id,goal_id,outcome,novel_results) SELECT individual_id,id,goal_id,'superseded_by_owner',0 FROM runs WHERE individual_id=$1 AND goal_id=$2 AND goal_due_at IS NOT NULL ON CONFLICT DO NOTHING")
            .bind(current.id).bind(goal).execute(&mut *tx).await?;
        advance(&mut tx, current.id, version).await?;
        outbox(&mut tx, current.id, "goal.changed", goal, version).await?;
        let r = GoalReceipt {
            individual_id: current.id,
            goal_id: goal,
            event_id: event,
            version,
            goal_version: goal_version + 1,
            status: next.into(),
        };
        save_receipt(&mut tx, "owner", &route, key, &hash, &r).await?;
        tx.commit().await?;
        Ok(r)
    }
}
