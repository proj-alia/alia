//! Application-inbox delivery is a single local transaction, not an external send.
use super::store::{Store, StoreError, advance, edge, insert_event, outbox, state};
use crate::{
    domain::Lifecycle,
    runtime::contact::{ContactPolicy, ContactReceipt, ContactWait},
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Deserialize)]
struct Draft {
    run_id: Uuid,
    mode: String,
    status: String,
    privacy_epoch: i64,
    delivered_event_id: Option<Uuid>,
    cancellation_reason: Option<String>,
}
type ContactWindow = (Uuid, DateTime<Utc>, DateTime<Utc>, i64);
fn policy(config: &Value) -> Result<ContactPolicy, StoreError> {
    let c = &config["contact"];
    let s = |k: &str| {
        c[k].as_str()
            .ok_or(StoreError::Invalid("contact configuration"))
    };
    let n = |k: &str| {
        c[k].as_i64()
            .ok_or(StoreError::Invalid("contact configuration"))
    };
    Ok(ContactPolicy {
        timezone: s("timezone")?
            .parse()
            .map_err(|_| StoreError::Invalid("contact timezone"))?,
        quiet_start: s("quiet_start")?
            .parse()
            .map_err(|_| StoreError::Invalid("quiet start"))?,
        quiet_end: s("quiet_end")?
            .parse()
            .map_err(|_| StoreError::Invalid("quiet end"))?,
        daily_limit: n("max_proactive_per_day")?,
        minimum_interval: Duration::try_hours(n("minimum_interval_hours")?)
            .ok_or(StoreError::Invalid("contact interval"))?,
    })
}

impl Store {
    /// Safe to retry after an unknown commit outcome or a process restart. No
    /// caller-controlled time, model call, or network I/O is accepted here.
    pub async fn deliver_proactive(&self, draft_id: Uuid) -> Result<ContactReceipt, StoreError> {
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let value: Value = sqlx::query_scalar("SELECT to_jsonb(m) FROM outgoing_messages m WHERE individual_id=$1 AND draft_event_id=$2")
            .bind(current.id).bind(draft_id).fetch_optional(&mut *tx).await?
            .ok_or(StoreError::Invalid("message absent"))?;
        let draft: Draft = serde_json::from_value(value)?;
        if draft.mode != "background" {
            return Err(StoreError::Invalid("not a proactive draft"));
        }
        if draft.status == "cancelled" {
            return Ok(ContactReceipt::Cancelled {
                reason: draft
                    .cancellation_reason
                    .unwrap_or_else(|| "cancelled".into()),
            });
        }
        // Recursively inspect ancestors, even if a deletion barrier has not yet
        // materialized the entire descendant closure. UNION tolerates graph cycles.
        let source_blocked: bool = sqlx::query_scalar(r#"WITH RECURSIVE ancestors(id) AS (
 SELECT $2::uuid UNION SELECT d.source_id FROM derivation_edges d JOIN ancestors a ON d.target_id=a.id WHERE d.individual_id=$1
) SELECT EXISTS(SELECT 1 FROM objects o WHERE o.individual_id=$1 AND (o.id=$1 OR o.id IN (SELECT id FROM ancestors)) AND o.blocked)"#)
            .bind(current.id).bind(draft_id).fetch_one(&mut *tx).await?;
        if draft.status == "delivered" {
            let delivered_blocked: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM objects WHERE individual_id=$1 AND id=$2 AND NOT blocked)")
                .bind(current.id).bind(draft.delivered_event_id).fetch_one(&mut *tx).await?;
            if source_blocked || delivered_blocked || draft.privacy_epoch != current.privacy_epoch {
                return Err(StoreError::StaleState);
            }
            return Ok(ContactReceipt::Delivered {
                message_id: draft
                    .delivered_event_id
                    .ok_or(StoreError::Invalid("delivery receipt absent"))?,
            });
        }
        let new_input: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events newer JOIN events draft ON draft.individual_id=newer.individual_id WHERE draft.id=$2 AND newer.individual_id=$1 AND newer.kind='message.received' AND newer.sequence>draft.sequence)")
            .bind(current.id).bind(draft_id).fetch_one(&mut *tx).await?;
        let run_valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs r JOIN objects o ON (o.individual_id,o.id)=(r.individual_id,r.id) WHERE r.individual_id=$1 AND r.id=$2 AND r.mode='background' AND r.status='committed' AND NOT o.blocked)")
            .bind(current.id).bind(draft.run_id).fetch_one(&mut *tx).await?;
        let reason = if source_blocked || draft.privacy_epoch != current.privacy_epoch {
            Some("source_unavailable")
        } else if !run_valid {
            Some("run_unavailable")
        } else if new_input {
            Some("new_input")
        } else if matches!(
            current.lifecycle,
            Lifecycle::Archived | Lifecycle::Terminated
        ) {
            Some("inactive")
        } else {
            None
        };
        if let Some(reason) = reason {
            sqlx::query("UPDATE outgoing_messages SET status='cancelled',cancellation_reason=$3 WHERE individual_id=$1 AND draft_event_id=$2")
                .bind(current.id).bind(draft_id).bind(reason).execute(&mut *tx).await?;
            // No copied draft text or new content event when privacy has changed.
            outbox(
                &mut tx,
                current.id,
                "operation.updated",
                draft.run_id,
                current.state_version,
            )
            .await?;
            tx.commit().await?;
            return Ok(ContactReceipt::Cancelled {
                reason: reason.into(),
            });
        }
        let paused: bool =
            sqlx::query_scalar("SELECT autonomy_paused FROM individuals WHERE id=$1")
                .bind(current.id)
                .fetch_one(&mut *tx)
                .await?;
        if current.lifecycle != Lifecycle::Active
            || paused
            || current.runtime_config["autonomy"]["enabled"] != true
        {
            return Ok(ContactReceipt::Deferred {
                reason: ContactWait::Paused,
            });
        }
        let busy: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE individual_id=$1 AND (status='running' OR (mode='foreground' AND status='queued')))")
            .bind(current.id).fetch_one(&mut *tx).await?;
        if busy {
            return Ok(ContactReceipt::Deferred {
                reason: ContactWait::Busy,
            });
        }
        let policy = policy(&current.runtime_config)?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let latest: Option<ContactWindow> = sqlx::query_as("SELECT id,starts_at,ends_at,delivered_count FROM contact_windows WHERE individual_id=$1 ORDER BY starts_at DESC LIMIT 1")
            .bind(current.id).fetch_optional(&mut *tx).await?;
        let (window, used) = match latest {
            Some((_, start, _, _)) if now < start => {
                return Ok(ContactReceipt::Deferred {
                    reason: ContactWait::ClockRollback,
                });
            }
            Some((id, _, end, used)) if now < end => (id, used),
            previous => {
                let timezone = policy.timezone.name();
                let (start,end):(DateTime<Utc>,DateTime<Utc>)=sqlx::query_as("SELECT date_trunc('day',$1::timestamptz AT TIME ZONE $2) AT TIME ZONE $2,(date_trunc('day',$1::timestamptz AT TIME ZONE $2)+interval '1 day') AT TIME ZONE $2")
                    .bind(now).bind(timezone).fetch_one(&mut *tx).await?;
                let start = previous.map_or(start, |(_, _, end, _)| start.max(end));
                if start > now || end <= now {
                    return Err(StoreError::Invalid("contact window"));
                }
                let id = Uuid::new_v4();
                sqlx::query("INSERT INTO contact_windows(id,individual_id,timezone,starts_at,ends_at) VALUES($1,$2,$3,$4,$5)")
                    .bind(id).bind(current.id).bind(timezone).bind(start).bind(end).execute(&mut *tx).await?;
                (id, 0)
            }
        };
        let last: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT max(last_delivered_at) FROM contact_windows WHERE individual_id=$1",
        )
        .bind(current.id)
        .fetch_one(&mut *tx)
        .await?;
        if let Some(reason) = policy.wait(now, used, last) {
            // Keep the existing day boundary fixed across timezone changes.
            tx.commit().await?;
            return Ok(ContactReceipt::Deferred { reason });
        }
        let payload: Value=sqlx::query_scalar("SELECT payload FROM events WHERE individual_id=$1 AND id=$2 AND kind='message.drafted'")
            .bind(current.id).bind(draft_id).fetch_one(&mut *tx).await?;
        if payload["text"].as_str().is_none_or(|t| t.trim().is_empty()) {
            return Err(StoreError::Invalid("draft text absent"));
        }
        let message = Uuid::new_v4();
        let version = current.state_version + 1;
        insert_event(&mut tx,current.id,message,"message.sent","generated","alia",json!({"text":payload["text"],"evidence_ids":payload["evidence_ids"],"conversation_id":payload["conversation_id"],"draft_id":draft_id,"proactive":true}),draft.run_id,None).await?;
        edge(&mut tx, current.id, draft_id, message).await?;
        let subject:Uuid=sqlx::query_scalar("INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT individual_id,subject_id,$2 FROM relationships WHERE individual_id=$1 AND subject_key='owner' RETURNING subject_id")
            .bind(current.id).bind(message).fetch_one(&mut *tx).await?;
        edge(&mut tx, current.id, message, subject).await?;
        sqlx::query("UPDATE relationships SET version=version+1 WHERE individual_id=$1 AND subject_key='owner'").bind(current.id).execute(&mut *tx).await?;
        sqlx::query("UPDATE outgoing_messages SET status='delivered',delivered_event_id=$3,delivered_at=$4,contact_window_id=$5 WHERE individual_id=$1 AND draft_event_id=$2")
            .bind(current.id).bind(draft_id).bind(message).bind(now).bind(window).execute(&mut *tx).await?;
        sqlx::query("UPDATE contact_windows SET delivered_count=delivered_count+1,last_delivered_at=$2 WHERE id=$1").bind(window).bind(now).execute(&mut *tx).await?;
        advance(&mut tx, current.id, version).await?;
        outbox(&mut tx, current.id, "message.created", message, version).await?;
        tx.commit().await?;
        Ok(ContactReceipt::Delivered {
            message_id: message,
        })
    }
}
