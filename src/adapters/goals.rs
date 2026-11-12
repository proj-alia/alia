//! Goal due occurrences are durable work. Novelty is not a completion evaluator.
use super::store::{Store, StoreError, advance, digest, edge, insert_event, object, outbox, state};
use crate::domain::Lifecycle;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

type Tx<'a> = Transaction<'a, Postgres>;
type ProgressResult = (Uuid, String, Value, Option<String>, Option<String>);

pub(super) async fn unavailable(
    tx: &mut Tx<'_>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(r#"WITH RECURSIVE ancestors(id) AS (
 SELECT unnest($2::uuid[]) UNION SELECT d.source_id FROM derivation_edges d JOIN ancestors a ON d.target_id=a.id WHERE d.individual_id=$1
) SELECT EXISTS(SELECT 1 FROM ancestors a LEFT JOIN objects o ON o.individual_id=$1 AND o.id=a.id WHERE o.id IS NULL OR o.blocked)"#)
        .bind(individual).bind(ids).fetch_one(&mut **tx).await?)
}

impl Store {
    /// Reconcile at most 20 finished goal cycles, then queue at most one due goal.
    /// No model call and no caller-controlled clock. Idle checks do not make events.
    pub async fn tick_goals(&self) -> Result<Option<Uuid>, StoreError> {
        let mut tx = self.write().await?;
        let mut current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let paused: bool =
            sqlx::query_scalar("SELECT autonomy_paused FROM individuals WHERE id=$1")
                .bind(current.id)
                .fetch_one(&mut *tx)
                .await?;
        let running: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE individual_id=$1 AND status='running')",
        )
        .bind(current.id)
        .fetch_one(&mut *tx)
        .await?;
        if current.lifecycle != Lifecycle::Active
            || paused
            || current.runtime_config["autonomy"]["enabled"] != true
            || running
            || unavailable(&mut tx, current.id, &[current.id]).await?
        {
            return Ok(None);
        }
        let exclude_trials = if super::development_measurements::reference_edges_in(&mut tx)
            .await?
            .is_some()
        {
            " AND NOT EXISTS(SELECT 1 FROM development_trial_cases trial WHERE trial.individual_id=r.individual_id AND trial.evaluation_run_id=r.id)"
        } else {
            ""
        };
        let finished:Vec<(Uuid,Uuid,String,i64)>=sqlx::query_as(&format!("SELECT r.id,r.goal_id,r.status,r.privacy_epoch FROM runs r WHERE r.individual_id=$1 AND r.goal_due_at IS NOT NULL AND r.status IN ('committed','failed','cancelled','stale') AND NOT EXISTS(SELECT 1 FROM goal_cycles c WHERE c.individual_id=r.individual_id AND c.run_id=r.id){exclude_trials} ORDER BY r.finished_at,r.created_at,r.id LIMIT 20"))
            .bind(current.id).fetch_all(&mut *tx).await?;
        for (run, goal, status, epoch) in finished {
            let invalid = epoch != current.privacy_epoch
                || unavailable(&mut tx, current.id, &[run, goal]).await?;
            let mut novel = 0i32;
            if !invalid {
                // Only actual successful action receipts. IDs, timestamps and note
                // titles cannot make repeated content count as new experience.
                let results:Vec<ProgressResult>=sqlx::query_as("SELECT e.id,a.capability_id,e.payload,n.body,d.body FROM actions a JOIN events e ON (e.individual_id,e.id)=(a.individual_id,a.result_event_id) LEFT JOIN notes n ON (n.individual_id,n.action_id)=(a.individual_id,a.id) LEFT JOIN documents d ON d.individual_id=a.individual_id AND d.event_id::text=e.payload->'output'->>'document_id' WHERE a.individual_id=$1 AND a.run_id=$2 AND a.status='succeeded' ORDER BY a.sequence")
                    .bind(current.id).bind(run).fetch_all(&mut *tx).await?;
                for (event, cap, payload, note, web_body) in results {
                    if unavailable(&mut tx, current.id, &[event]).await? {
                        continue;
                    }
                    let text = match cap.as_str() {
                        "notes.write" => note,
                        "documents.read" => payload["output"]["text"].as_str().map(str::to_owned),
                        "web.read_allowed" => web_body
                            .or_else(|| payload["output"]["text"].as_str().map(str::to_owned)),
                        _ => None,
                    };
                    let Some(text) = text.filter(|s| !s.trim().is_empty()) else {
                        continue;
                    };
                    let hash = digest(
                        &json!({"kind":if cap=="notes.write"{"artifact"}else{"observation"},"text":text.split_whitespace().collect::<Vec<_>>().join(" ")}),
                    )?;
                    novel+=sqlx::query("INSERT INTO goal_progress(individual_id,goal_id,content_digest,event_id) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(current.id).bind(goal).bind(hash).bind(event).execute(&mut *tx).await?.rows_affected() as i32;
                }
            }
            sqlx::query("INSERT INTO goal_cycles(individual_id,run_id,goal_id,outcome,novel_results) VALUES($1,$2,$3,$4,$5)").bind(current.id).bind(run).bind(goal).bind(if invalid{"source_unavailable"}else{&status}).bind(novel).execute(&mut *tx).await?;
            let (goal_status, count): (String, i32) = sqlx::query_as(
                "SELECT status,no_progress_cycles FROM goals WHERE individual_id=$1 AND id=$2",
            )
            .bind(current.id)
            .bind(goal)
            .fetch_one(&mut *tx)
            .await?;
            if !matches!(goal_status.as_str(), "proposed" | "active") {
                continue;
            }
            let no_progress = if novel > 0 {
                0
            } else if status == "committed" || status == "failed" {
                count + 1
            } else {
                count
            };
            let reason = if invalid {
                Some("source_unavailable")
            } else if no_progress >= 3 {
                Some("no_progress")
            } else {
                None
            };
            let next = if reason.is_some() {
                "waiting"
            } else {
                goal_status.as_str()
            };
            let delay = current.runtime_config["autonomy"]["idle_check_minutes"]
                .as_i64()
                .ok_or(StoreError::Invalid("idle interval"))?;
            sqlx::query("UPDATE goals SET status=$3,no_progress_cycles=$4,waiting_reason=$5,resume_at=NULL,due_at=CASE WHEN $5::text IS NULL THEN clock_timestamp()+make_interval(secs=>$6) ELSE NULL END,version=version+1 WHERE individual_id=$1 AND id=$2")
                .bind(current.id).bind(goal).bind(next).bind(no_progress).bind(reason).bind((delay*60)as f64).execute(&mut *tx).await?;
            if !invalid {
                let event = Uuid::new_v4();
                insert_event(&mut tx,current.id,event,"goal.cycle_evaluated","direct_observation","runtime",json!({"goal_id":goal,"run_id":run,"outcome":status,"novel_results":novel,"no_progress_cycles":no_progress,"to":next,"waiting_reason":reason}),run,None).await?;
                edge(&mut tx, current.id, goal, event).await?;
                edge(&mut tx, current.id, run, event).await?;
                if reason.is_some() {
                    sqlx::query("UPDATE goals SET waiting_since_sequence=(SELECT sequence FROM events WHERE id=$3) WHERE individual_id=$1 AND id=$2").bind(current.id).bind(goal).bind(event).execute(&mut *tx).await?;
                }
            }
            current.state_version += 1;
            advance(&mut tx, current.id, current.state_version).await?;
            outbox(
                &mut tx,
                current.id,
                "goal.changed",
                goal,
                current.state_version,
            )
            .await?;
        }
        // Resource failure is not lack of progress. Resume only elapsed budget
        // waits; unavailable adapters require an explicit capability/config fix.
        let resumable:Vec<Uuid>=sqlx::query_scalar("SELECT g.id FROM goals g JOIN individuals i ON i.id=g.individual_id WHERE g.individual_id=$1 AND g.status='waiting' AND g.waiting_reason='budget_unavailable' AND g.resume_at<=clock_timestamp() AND NOT i.budget_frozen ORDER BY g.resume_at,g.id LIMIT 20")
            .bind(current.id).fetch_all(&mut *tx).await?;
        for goal in resumable {
            if unavailable(&mut tx, current.id, &[goal]).await? {
                continue;
            }
            sqlx::query("UPDATE goals SET status='active',waiting_reason=NULL,resume_at=NULL,due_at=clock_timestamp(),version=version+1 WHERE individual_id=$1 AND id=$2")
                .bind(current.id).bind(goal).execute(&mut *tx).await?;
            let event = Uuid::new_v4();
            insert_event(
                &mut tx,
                current.id,
                event,
                "goal.resource_resumed",
                "direct_observation",
                "runtime",
                json!({"goal_id":goal,"reason":"budget_window_elapsed"}),
                event,
                None,
            )
            .await?;
            edge(&mut tx, current.id, goal, event).await?;
            current.state_version += 1;
            advance(&mut tx, current.id, current.state_version).await?;
            outbox(
                &mut tx,
                current.id,
                "goal.changed",
                goal,
                current.state_version,
            )
            .await?;
        }
        // Claim foreground work first. Never invalidate its in-flight cognition.
        let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE individual_id=$1 AND status IN ('queued','running'))").bind(current.id).fetch_one(&mut *tx).await?;
        if pending {
            tx.commit().await?;
            return Ok(None);
        }
        let candidates:Vec<(Uuid,DateTime<Utc>,String,String)>=sqlx::query_as("SELECT g.id,g.due_at,g.description,g.next_step FROM goals g WHERE g.individual_id=$1 AND g.status IN ('proposed','active') AND g.due_at<=clock_timestamp() AND NOT EXISTS(SELECT 1 FROM runs r WHERE r.individual_id=g.individual_id AND r.goal_id=g.id AND (r.status IN ('queued','running') OR r.goal_due_at=g.due_at)) ORDER BY g.priority DESC,g.due_at,g.created_at,g.id LIMIT 20")
            .bind(current.id).fetch_all(&mut *tx).await?;
        for (goal, due, description, next_step) in candidates {
            if unavailable(&mut tx, current.id, &[goal]).await? {
                continue;
            }
            let run = Uuid::new_v4();
            let trigger = Uuid::new_v4();
            insert_event(
                &mut tx,
                current.id,
                trigger,
                "goal.due",
                "direct_observation",
                "runtime",
                json!({"goal_id":goal,"due_at":due,"text":description,"next_step":next_step}),
                run,
                None,
            )
            .await?;
            object(&mut tx, current.id, run, "run").await?;
            edge(&mut tx, current.id, goal, trigger).await?;
            edge(&mut tx, current.id, trigger, run).await?;
            current.state_version += 1;
            sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,goal_id,goal_due_at,mode,state_version,privacy_epoch,config_version,input_manifest) VALUES($1,$2,$3,$4,$5,'background',$6,$7,$8,$9)")
                .bind(current.id).bind(run).bind(trigger).bind(goal).bind(due).bind(current.state_version).bind(current.privacy_epoch).bind(current.config_version).bind(vec![current.id,trigger,goal]).execute(&mut *tx).await?;
            advance(&mut tx, current.id, current.state_version).await?;
            outbox(
                &mut tx,
                current.id,
                "operation.updated",
                run,
                current.state_version,
            )
            .await?;
            tx.commit().await?;
            return Ok(Some(run));
        }
        tx.commit().await?;
        Ok(None)
    }
}
