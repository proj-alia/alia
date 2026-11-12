//! Stored relationship records and explicitly linked communication, owner-only and read-only.
use super::{
    api::{VISIBLE, existing},
    store::{Store, StoreError},
};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

fn limit(value: u32) -> Result<i64, StoreError> {
    if !(1..=100).contains(&value) {
        return Err(StoreError::Invalid("invalid relationship limit"));
    }
    Ok(i64::from(value))
}
fn directory_position(cursor: Option<&str>) -> Result<Option<Uuid>, StoreError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let invalid = || StoreError::Invalid("invalid relationship cursor");
    let parts: Vec<_> = cursor.split(':').collect();
    if cursor.len() > 128
        || parts.len() != 4
        || parts[..3] != ["v1", "relationships", "subject_asc"]
    {
        return Err(invalid());
    }
    Ok(Some(parts[3].parse().map_err(|_| invalid())?))
}
fn event_position(cursor: Option<&str>, subject: Uuid) -> Result<Option<(i64, Uuid)>, StoreError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let invalid = || StoreError::Invalid("invalid relationship event cursor");
    let parts: Vec<_> = cursor.split(':').collect();
    if cursor.len() > 256
        || parts.len() != 6
        || parts[..3] != ["v1", "relationship-events", "sequence_desc"]
        || parts[3] != subject.to_string()
    {
        return Err(invalid());
    }
    let seq = parts[4].parse::<i64>().map_err(|_| invalid())?;
    if seq <= 0 {
        return Err(invalid());
    }
    Ok(Some((seq, parts[5].parse().map_err(|_| invalid())?)))
}
async fn snapshot(tx: &mut Transaction<'_, Postgres>) -> Result<(Uuid, Value), StoreError> {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut **tx)
        .await?;
    let individual = existing(tx).await?;
    let envelope:Value=sqlx::query_scalar("SELECT jsonb_build_object('individual_id',id,'state_version',state_version,'privacy_epoch',privacy_epoch,'observed_at',transaction_timestamp()) FROM individuals WHERE id=$1").bind(individual).fetch_one(&mut **tx).await?;
    Ok((individual, envelope))
}
async fn relationship(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    subject: Uuid,
) -> Result<Value, StoreError> {
    sqlx::query_scalar(&format!(r#"{VISIBLE}
        SELECT jsonb_build_object('subject_id',r.subject_id,'subject_key',r.subject_key,'stage',r.stage,'version',r.version,
            'expectations',r.expectations,'boundaries',r.boundaries,'commitments',r.commitments,'unresolved',r.unresolved)
        FROM relationships r WHERE r.individual_id=$1 AND r.subject_id=$2
        AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=r.individual_id AND h.id IN (r.individual_id,r.subject_id))"#))
        .bind(individual).bind(subject).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)
}
impl Store {
    pub async fn public_relationships(
        &self,
        cursor: Option<&str>,
        page_limit: u32,
    ) -> Result<Value, StoreError> {
        let page_limit = limit(page_limit)?;
        let after = directory_position(cursor)?;
        let mut tx = self.pool.begin().await?;
        let (individual, mut response) = snapshot(&mut tx).await?;
        if let Some(anchor) = after {
            relationship(&mut tx, individual, anchor)
                .await
                .map_err(|e| match e {
                    StoreError::NotFound => StoreError::Invalid("unavailable relationship cursor"),
                    other => other,
                })?;
        }
        let mut rows:Vec<(Uuid,Value)>=sqlx::query_as(&format!(r#"{VISIBLE}
            SELECT r.subject_id,jsonb_build_object('subject_id',r.subject_id,'subject_key',r.subject_key,'stage',r.stage,'version',r.version)
            FROM relationships r WHERE r.individual_id=$1 AND ($2::uuid IS NULL OR r.subject_id>$2)
            AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=r.individual_id AND h.id IN (r.individual_id,r.subject_id))
            ORDER BY r.subject_id ASC LIMIT $3"#)).bind(individual).bind(after).bind(page_limit+1).fetch_all(&mut *tx).await?;
        let more = rows.len() > page_limit as usize;
        rows.truncate(page_limit as usize);
        response["next_cursor"] = json!(if more {
            rows.last()
                .map(|r| format!("v1:relationships:subject_asc:{}", r.0))
        } else {
            None
        });
        response["items"] = json!(rows.into_iter().map(|(_, value)| value).collect::<Vec<_>>());
        tx.commit().await?;
        Ok(response)
    }
    pub async fn public_relationship(&self, subject: Uuid) -> Result<Value, StoreError> {
        let mut tx = self.pool.begin().await?;
        let (individual, mut response) = snapshot(&mut tx).await?;
        response["relationship"] = relationship(&mut tx, individual, subject).await?;
        tx.commit().await?;
        Ok(response)
    }
    pub async fn public_relationship_events(
        &self,
        subject: Uuid,
        cursor: Option<&str>,
        page_limit: u32,
    ) -> Result<Value, StoreError> {
        let page_limit = limit(page_limit)?;
        let mut tx = self.pool.begin().await?;
        let (individual, mut response) = snapshot(&mut tx).await?;
        relationship(&mut tx, individual, subject).await?;
        let after = event_position(cursor, subject)?;
        if let Some((seq, id)) = after {
            let available:bool=sqlx::query_scalar(&format!(r#"{VISIBLE}
                SELECT EXISTS(SELECT 1 FROM relationship_events re JOIN events e ON (e.individual_id,e.id)=(re.individual_id,re.event_id)
                WHERE re.individual_id=$1 AND re.subject_id=$2 AND e.sequence=$3 AND e.id=$4
                AND e.kind IN ('message.received','message.sent') AND jsonb_typeof(e.payload->'text')='string'
                AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=re.individual_id AND h.id IN (re.individual_id,re.subject_id,e.id)))"#))
                .bind(individual).bind(subject).bind(seq).bind(id).fetch_one(&mut *tx).await?;
            if !available {
                return Err(StoreError::Invalid("unavailable relationship event cursor"));
            }
        }
        let mut rows:Vec<(i64,Uuid,Value)>=sqlx::query_as(&format!(r#"{VISIBLE}
            SELECT e.sequence,e.id,jsonb_build_object('id',e.id,'sequence',e.sequence,'kind',e.kind,'source_kind',e.source_kind,'actor_id',e.actor_id,'occurred_at',e.occurred_at,'text',e.payload->>'text')
            FROM relationship_events re JOIN events e ON (e.individual_id,e.id)=(re.individual_id,re.event_id)
            WHERE re.individual_id=$1 AND re.subject_id=$2 AND ($3::bigint IS NULL OR e.sequence<$3)
            AND e.kind IN ('message.received','message.sent') AND jsonb_typeof(e.payload->'text')='string'
            AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=re.individual_id AND h.id IN (re.individual_id,re.subject_id,e.id))
            ORDER BY e.sequence DESC LIMIT $4"#))
            .bind(individual).bind(subject).bind(after.map(|p|p.0)).bind(page_limit+1).fetch_all(&mut *tx).await?;
        let more = rows.len() > page_limit as usize;
        rows.truncate(page_limit as usize);
        response["subject_id"] = json!(subject);
        response["next_cursor"] = json!(if more {
            rows.last().map(|r| {
                format!(
                    "v1:relationship-events:sequence_desc:{subject}:{}:{}",
                    r.0, r.1
                )
            })
        } else {
            None
        });
        response["items"] = json!(
            rows.into_iter()
                .map(|(_, _, value)| value)
                .collect::<Vec<_>>()
        );
        tx.commit().await?;
        Ok(response)
    }
}
