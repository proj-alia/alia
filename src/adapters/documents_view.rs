//! Owner-only document projections. Reading here never records model/tool activity.
use super::{
    api::{VISIBLE, existing},
    store::{Store, StoreError, digest},
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

fn query_text(q: &str) -> Result<&str, StoreError> {
    if q.chars().count() > 200 || q.chars().any(char::is_control) {
        return Err(StoreError::Invalid("invalid document query"));
    }
    Ok(if q.trim().is_empty() { "" } else { q })
}

fn position(
    cursor: Option<&str>,
    fingerprint: &str,
) -> Result<Option<(DateTime<Utc>, Uuid)>, StoreError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let invalid = || StoreError::Invalid("invalid document cursor");
    if cursor.len() > 256 {
        return Err(invalid());
    }
    let fields: Vec<_> = cursor.split(':').collect();
    if fields.len() != 5
        || fields[0] != "v1"
        || fields[1] != "documents"
        || fields[2] != fingerprint
    {
        return Err(invalid());
    }
    let time = fields[3]
        .parse::<i64>()
        .ok()
        .and_then(DateTime::from_timestamp_micros)
        .ok_or_else(invalid)?;
    let id = fields[4].parse::<Uuid>().map_err(|_| invalid())?;
    Ok(Some((time, id)))
}

impl Store {
    pub async fn public_documents(
        &self,
        q: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Value, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("invalid document limit"));
        }
        let q = query_text(q)?;
        let fingerprint = digest(&json!({"q":q}))?;
        let after = position(cursor, &fingerprint)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        let mut rows: Vec<(DateTime<Utc>,Uuid,Value)> = sqlx::query_as(&format!(r#"{VISIBLE}
            SELECT e.recorded_at,d.event_id,jsonb_build_object(
                'id',d.event_id,'title',d.title,'source_label',d.source_label,
                'source_kind',e.source_kind,'created_at',e.recorded_at)
            FROM documents d JOIN events e ON (e.individual_id,e.id)=(d.individual_id,d.event_id)
            WHERE d.individual_id=$1
              AND (strpos(lower(d.title),lower($2))>0 OR strpos(lower(d.source_label),lower($2))>0)
              AND ($3::timestamptz IS NULL OR (e.recorded_at,d.event_id)<($3,$4::uuid))
              AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=d.individual_id AND h.id IN (d.individual_id,d.event_id))
            ORDER BY e.recorded_at DESC,d.event_id DESC LIMIT $5"#))
            .bind(individual).bind(q).bind(after.as_ref().map(|p| p.0))
            .bind(after.as_ref().map(|p| p.1)).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next = if more {
            rows.last().map(|r| {
                format!(
                    "v1:documents:{fingerprint}:{}:{}",
                    r.0.timestamp_micros(),
                    r.1
                )
            })
        } else {
            None
        };
        let items: Vec<_> = rows.into_iter().map(|(_, _, item)| item).collect();
        tx.commit().await?;
        Ok(json!({"items":items,"next_cursor":next}))
    }

    pub async fn public_document(
        &self,
        document: Uuid,
        offset: u32,
        max_chars: u32,
    ) -> Result<Value, StoreError> {
        if offset > 1_048_576 || !(1..=4096).contains(&max_chars) {
            return Err(StoreError::Invalid("invalid document range"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        let (length, mut item): (i32,Value) = sqlx::query_as(&format!(r#"{VISIBLE}
            SELECT char_length(d.body),jsonb_build_object(
                'individual_id',d.individual_id,'document_id',d.event_id,'event_id',d.event_id,
                'title',d.title,'source_label',d.source_label,'source_kind',e.source_kind,
                'offset',$3::integer,'max_chars',$4::integer,'total_chars',char_length(d.body),
                'text',substring(d.body FROM ($3::integer+1) FOR $4::integer),'observed_at',transaction_timestamp())
            FROM documents d JOIN events e ON (e.individual_id,e.id)=(d.individual_id,d.event_id)
            WHERE d.individual_id=$1 AND d.event_id=$2
              AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=d.individual_id AND h.id IN (d.individual_id,d.event_id))"#))
            .bind(individual).bind(document).bind(offset as i32).bind(max_chars as i32)
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        if offset > length as u32 {
            return Err(StoreError::Invalid("document offset out of range"));
        }
        let end = (offset + max_chars).min(length as u32);
        item["next_offset"] = json!(if end < length as u32 { Some(end) } else { None });
        tx.commit().await?;
        Ok(item)
    }
}
