//! Generated notes: a successful save is provenance, never independent factual verification.
use super::{
    api::{VISIBLE, existing},
    store::{Store, StoreError, digest},
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

fn query_text(q: &str) -> Result<&str, StoreError> {
    if q.chars().count() > 200 || q.chars().any(char::is_control) {
        return Err(StoreError::Invalid("invalid note query"));
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
    let invalid = || StoreError::Invalid("invalid note cursor");
    if cursor.len() > 256 {
        return Err(invalid());
    }
    let fields: Vec<_> = cursor.split(':').collect();
    if fields.len() != 5 || fields[0] != "v1" || fields[1] != "notes" || fields[2] != fingerprint {
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
    pub async fn public_notes(
        &self,
        q: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Value, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("invalid note limit"));
        }
        let q = query_text(q)?;
        let fingerprint = digest(&json!({"q":q}))?;
        let after = position(cursor, &fingerprint)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        let mut rows: Vec<(DateTime<Utc>, Uuid, Value)> = sqlx::query_as(&format!(
            r#"{VISIBLE}
            SELECT n.created_at,n.action_id,jsonb_build_object(
                'id',n.action_id,'title',n.title,'created_at',n.created_at,
                'run_id',a.run_id,'result_event_id',a.result_event_id,
                'capability_id',a.capability_id,'capability_version',a.capability_version)
            FROM notes n
            JOIN actions a ON (a.individual_id,a.id)=(n.individual_id,n.action_id)
            JOIN runs r ON (r.individual_id,r.id)=(a.individual_id,a.run_id)
            JOIN events e ON (e.individual_id,e.id)=(a.individual_id,a.result_event_id)
            WHERE n.individual_id=$1 AND a.capability_id='notes.write' AND a.status='succeeded'
              AND e.kind='tool.result' AND e.payload->>'status'='succeeded'
              AND e.payload->>'action_id'=a.id::text AND e.payload->>'capability_id'=a.capability_id
              AND e.payload->>'capability_version'=a.capability_version
              AND e.payload->'output'->>'note_id'=n.action_id::text
              AND (strpos(lower(n.title),lower($2))>0 OR strpos(a.id::text,lower($2))>0)
              AND ($3::timestamptz IS NULL OR (n.created_at,n.action_id)<($3,$4::uuid))
              AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=n.individual_id
                  AND h.id IN (n.individual_id,n.action_id,a.run_id,a.result_event_id))
            ORDER BY n.created_at DESC,n.action_id DESC LIMIT $5"#
        ))
        .bind(individual)
        .bind(q)
        .bind(after.as_ref().map(|p| p.0))
        .bind(after.as_ref().map(|p| p.1))
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *tx)
        .await?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next = if more {
            rows.last()
                .map(|r| format!("v1:notes:{fingerprint}:{}:{}", r.0.timestamp_micros(), r.1))
        } else {
            None
        };
        let items: Vec<_> = rows.into_iter().map(|(_, _, item)| item).collect();
        tx.commit().await?;
        Ok(json!({"items":items,"next_cursor":next}))
    }

    pub async fn public_note(
        &self,
        note: Uuid,
        offset: u32,
        max_chars: u32,
    ) -> Result<Value, StoreError> {
        if offset > 1_048_576 || !(1..=4096).contains(&max_chars) {
            return Err(StoreError::Invalid("invalid note range"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        let (length, mut item): (i32,Value) = sqlx::query_as(&format!(r#"{VISIBLE}
            SELECT char_length(n.body),jsonb_build_object(
                'individual_id',n.individual_id,'note_id',n.action_id,'action_id',n.action_id,
                'run_id',a.run_id,'result_event_id',a.result_event_id,
                'title',n.title,'created_at',n.created_at,
                'capability_id',a.capability_id,'capability_version',a.capability_version,
                'offset',$3::integer,'max_chars',$4::integer,'total_chars',char_length(n.body),
                'text',substring(n.body FROM ($3::integer+1) FOR $4::integer),'observed_at',transaction_timestamp())
            FROM notes n
            JOIN actions a ON (a.individual_id,a.id)=(n.individual_id,n.action_id)
            JOIN runs r ON (r.individual_id,r.id)=(a.individual_id,a.run_id)
            JOIN events e ON (e.individual_id,e.id)=(a.individual_id,a.result_event_id)
            WHERE n.individual_id=$1 AND n.action_id=$2
              AND a.capability_id='notes.write' AND a.status='succeeded'
              AND e.kind='tool.result' AND e.payload->>'status'='succeeded'
              AND e.payload->>'action_id'=a.id::text AND e.payload->>'capability_id'=a.capability_id
              AND e.payload->>'capability_version'=a.capability_version
              AND e.payload->'output'->>'note_id'=n.action_id::text
              AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=n.individual_id
                  AND h.id IN (n.individual_id,n.action_id,a.run_id,a.result_event_id))"#))
            .bind(individual).bind(note).bind(offset as i32).bind(max_chars as i32)
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        if offset > length as u32 {
            return Err(StoreError::Invalid("note offset out of range"));
        }
        let end = (offset + max_chars).min(length as u32);
        item["next_offset"] = json!(if end < length as u32 { Some(end) } else { None });
        tx.commit().await?;
        Ok(item)
    }
}
