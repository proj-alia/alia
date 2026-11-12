//! Owner browsing of actual stances and both evidence roles, never a truth verdict.
use super::{
    api::{VISIBLE, existing},
    goals::unavailable,
    store::{Store, StoreError, digest},
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

const VISIBLE_BELIEFS: &str = r#", lineage(root,id) AS (
 SELECT id,id FROM beliefs WHERE individual_id=$1
 UNION SELECT l.root,b.supersedes FROM lineage l JOIN beliefs b ON b.individual_id=$1 AND b.id=l.id WHERE b.supersedes IS NOT NULL
), eligible AS (
 SELECT b.* FROM beliefs b WHERE b.individual_id=$1
 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=$1 AND h.id=$1)
 AND NOT EXISTS(SELECT 1 FROM lineage l LEFT JOIN objects o ON o.individual_id=$1 AND o.id=l.id
   WHERE l.root=b.id AND (o.id IS NULL OR EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=$1 AND h.id=l.id)))
 AND NOT EXISTS(SELECT 1 FROM lineage l JOIN belief_evidence e ON e.individual_id=$1 AND e.belief_id=l.id
   WHERE l.root=b.id AND EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=$1 AND h.id=e.event_id))
) "#;
const PROJECTION: &str = r#"jsonb_build_object('id',b.id,'version',b.version,'statement',b.statement,
 'scope',b.scope,'position',b.position,'status',b.status,'brief_basis',b.brief_basis,
 'created_at',b.created_at,'superseded_at',b.superseded_at,'supersedes',b.supersedes,
 'support_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE e.individual_id=$1 AND e.belief_id=b.id AND role='support' ORDER BY event_id),
 'counter_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE e.individual_id=$1 AND e.belief_id=b.id AND role='counter' ORDER BY event_id))"#;
const FILTER: &str = "($2='all' OR b.status=$2) AND ($3::text IS NULL OR b.scope=$3) AND (strpos(lower(b.statement),lower($4))>0 OR strpos(lower(b.brief_basis),lower($4))>0)";

fn text(value: &str, max: usize, empty: bool) -> Result<(), StoreError> {
    if value.chars().count() > max
        || value.chars().any(char::is_control)
        || (!value.is_empty() && value.trim().is_empty())
        || (!empty && value.is_empty())
    {
        Err(StoreError::Invalid("invalid belief filter"))
    } else {
        Ok(())
    }
}
fn position(
    cursor: Option<&str>,
    fingerprint: &str,
) -> Result<Option<(DateTime<Utc>, Uuid)>, StoreError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let bad = || StoreError::Invalid("invalid belief cursor");
    if cursor.len() > 256 {
        return Err(bad());
    }
    let parts: Vec<_> = cursor.split(':').collect();
    if parts.len() != 5 || parts[0] != "v1" || parts[1] != "beliefs-desc" || parts[2] != fingerprint
    {
        return Err(bad());
    }
    let time = parts[3]
        .parse::<i64>()
        .ok()
        .and_then(DateTime::from_timestamp_micros)
        .ok_or_else(bad)?;
    Ok(Some((time, parts[4].parse().map_err(|_| bad())?)))
}
impl Store {
    pub async fn public_beliefs(
        &self,
        q: &str,
        scope: Option<&str>,
        status: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Value, StoreError> {
        text(q, 200, true)?;
        if let Some(scope) = scope {
            text(scope, 200, false)?;
        }
        if !(1..=100).contains(&limit) || !matches!(status, "current" | "superseded" | "all") {
            return Err(StoreError::Invalid("invalid belief query"));
        }
        let fingerprint = digest(&json!({"q":q,"scope":scope,"status":status}))?;
        let after = position(cursor, &fingerprint)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        if unavailable(&mut tx, individual, &[individual]).await? {
            return Err(StoreError::NotFound);
        }
        if let Some((time, id)) = after {
            let valid:bool=sqlx::query_scalar(&format!("{VISIBLE}{VISIBLE_BELIEFS} SELECT EXISTS(SELECT 1 FROM eligible b WHERE {FILTER} AND b.created_at=$5 AND b.id=$6)"))
    .bind(individual).bind(status).bind(scope).bind(q).bind(time).bind(id).fetch_one(&mut *tx).await?;
            if !valid {
                return Err(StoreError::Invalid("unavailable belief cursor"));
            }
        }
        let mut rows:Vec<(DateTime<Utc>,Uuid,Value)>=sqlx::query_as(&format!("{VISIBLE}{VISIBLE_BELIEFS} SELECT b.created_at,b.id,{PROJECTION} FROM eligible b WHERE {FILTER} AND ($5::timestamptz IS NULL OR (b.created_at,b.id)<($5,$6::uuid)) ORDER BY b.created_at DESC,b.id DESC LIMIT $7"))
   .bind(individual).bind(status).bind(scope).bind(q).bind(after.map(|p|p.0)).bind(after.map(|p|p.1)).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next = if more {
            rows.last().map(|(time, id, _)| {
                format!(
                    "v1:beliefs-desc:{fingerprint}:{}:{id}",
                    time.timestamp_micros()
                )
            })
        } else {
            None
        };
        let items: Vec<_> = rows.into_iter().map(|(_, _, v)| v).collect();
        let (state_version, privacy_epoch, observed_at): (i64, i64, DateTime<Utc>) =
            sqlx::query_as(
                "SELECT state_version,privacy_epoch,clock_timestamp() FROM individuals WHERE id=$1",
            )
            .bind(individual)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,"privacy_epoch":privacy_epoch,"observed_at":observed_at,"items":items,"next_cursor":next}),
        )
    }
    pub async fn public_belief(&self, id: Uuid) -> Result<Value, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        if unavailable(&mut tx, individual, &[individual, id]).await? {
            return Err(StoreError::NotFound);
        }
        let belief: Value = sqlx::query_scalar(&format!(
            "{VISIBLE}{VISIBLE_BELIEFS} SELECT {PROJECTION} FROM eligible b WHERE b.id=$2"
        ))
        .bind(individual)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let (state_version, privacy_epoch, observed_at): (i64, i64, DateTime<Utc>) =
            sqlx::query_as(
                "SELECT state_version,privacy_epoch,clock_timestamp() FROM individuals WHERE id=$1",
            )
            .bind(individual)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,"privacy_epoch":privacy_epoch,"observed_at":observed_at,"belief":belief}),
        )
    }
}
