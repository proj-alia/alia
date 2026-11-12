//! Durable notifications are hints, never commands. Bodies remain in guarded projections.
use super::{
    api::{VISIBLE, existing},
    store::{Store, StoreError},
};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error(transparent)]
    Storage(#[from] StoreError),
    #[error("event cursor expired; reload state snapshot")]
    Expired,
    #[error("event cursor is ahead of committed history")]
    Future,
}
impl From<sqlx::Error> for NotificationError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(StoreError::Database(e))
    }
}
#[derive(Debug)]
pub struct Notification {
    pub sequence: i64,
    // None advances the cursor without revealing an unavailable resource.
    pub resource: Option<(String, Uuid, i64)>,
}
#[derive(Debug)]
pub struct NotificationBatch {
    pub cursor: i64,
    pub items: Vec<Notification>,
}
const TYPES: &[&str] = &[
    "operation.updated",
    "message.created",
    "state.changed",
    "memory.changed",
    "goal.changed",
    "development.changed",
    "privacy.changed",
    "budget.changed",
];

pub(super) async fn head(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar("SELECT GREATEST(COALESCE((SELECT max(sequence) FROM outbox WHERE individual_id=$1),0),COALESCE((SELECT discarded_through FROM event_stream_watermarks WHERE individual_id=$1),0))")
        .bind(id).fetch_one(&mut **tx).await?)
}
// Retain a contiguous suffix, even if the system clock moved backwards. This
// prevents removing a newer row behind a still-live earlier cursor.
const EXPIRED_PREFIX: &str = "SELECT COALESCE(max(sequence),0) FROM outbox WHERE individual_id=$1 AND created_at < now()-interval '7 days' AND sequence < COALESCE((SELECT min(sequence) FROM outbox WHERE individual_id=$1 AND created_at >= now()-interval '7 days'),9223372036854775807)";
impl Store {
    /// Bounded housekeeping. Never creates an individual or advances its state.
    pub async fn prune_notifications(&self) -> Result<(), StoreError> {
        let mut tx = self.write().await?;
        let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM individuals WHERE singleton")
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(());
        };
        let expired: i64 = sqlx::query_scalar(EXPIRED_PREFIX)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let removed: Option<i64> = sqlx::query_scalar("WITH selected AS (SELECT sequence FROM outbox WHERE individual_id=$1 AND sequence<=$2 ORDER BY sequence LIMIT 1000), gone AS (DELETE FROM outbox WHERE individual_id=$1 AND sequence IN (SELECT sequence FROM selected) RETURNING sequence) SELECT max(sequence) FROM gone")
            .bind(id).bind(expired).fetch_one(&mut *tx).await?;
        if let Some(floor) = removed {
            sqlx::query("INSERT INTO event_stream_watermarks(individual_id,discarded_through) VALUES($1,$2) ON CONFLICT(individual_id) DO UPDATE SET discarded_through=GREATEST(event_stream_watermarks.discarded_through,EXCLUDED.discarded_through)")
                .bind(id).bind(floor).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
    /// One short repeatable-read snapshot per batch. No connection survives idle waits.
    pub async fn notifications(
        &self,
        after: Option<i64>,
    ) -> Result<NotificationBatch, NotificationError> {
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let id = existing(&mut tx).await?;
        let high = head(&mut tx, id).await?;
        let pruned: i64 = sqlx::query_scalar("SELECT COALESCE((SELECT discarded_through FROM event_stream_watermarks WHERE individual_id=$1),0)").bind(id).fetch_one(&mut *tx).await?;
        let expired: i64 = sqlx::query_scalar(EXPIRED_PREFIX)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let cursor = after.unwrap_or(high);
        if cursor < pruned.max(expired) {
            return Err(NotificationError::Expired);
        }
        if cursor > high {
            return Err(NotificationError::Future);
        }
        let rows = sqlx::query(&format!("{VISIBLE} SELECT sequence,event_type,resource_id,version,NOT EXISTS(SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(o.individual_id,o.resource_id)) AS visible FROM outbox o WHERE individual_id=$1 AND sequence>$2 ORDER BY sequence LIMIT 100"))
            .bind(id).bind(cursor).fetch_all(&mut *tx).await?;
        let items = rows
            .into_iter()
            .map(|row| {
                let kind: String = row.get("event_type");
                let resource = (row.get::<bool, _>("visible") && TYPES.contains(&kind.as_str()))
                    .then(|| (kind, row.get("resource_id"), row.get("version")));
                Notification {
                    sequence: row.get("sequence"),
                    resource,
                }
            })
            .collect();
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(NotificationBatch { cursor, items })
    }
}
