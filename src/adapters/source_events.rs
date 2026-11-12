//! Explicit owner source inspection; never a raw event/provider payload API.
use super::{
    api::existing,
    goals::unavailable,
    store::{Store, StoreError},
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

impl Store {
    pub async fn public_source_event(
        &self,
        event: Uuid,
        offset: u32,
        max_chars: u32,
    ) -> Result<Value, StoreError> {
        if !(1..=4096).contains(&max_chars) {
            return Err(StoreError::Invalid("invalid source event range"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        if unavailable(&mut tx, individual, &[individual, event]).await? {
            return Err(StoreError::NotFound);
        }
        // Select an explicit public projection and bounded text in PostgreSQL.
        // Even unknown kinds with a payload.text field never expose that value.
        let (metadata, length, fragment): (Value, Option<i32>, Option<String>) = sqlx::query_as(
            r#"WITH source AS (
              SELECT e.*,CASE WHEN kind IN ('message.received','message.sent','memory.corrected')
                AND jsonb_typeof(payload->'text')='string' THEN payload->>'text' END AS exposed
              FROM events e WHERE individual_id=$1 AND id=$2
            ) SELECT jsonb_build_object('id',id,'sequence',sequence,'kind',kind,
                'source_kind',source_kind,'actor_id',actor_id,'occurred_at',occurred_at,
                'recorded_at',recorded_at),char_length(exposed),
              substring(exposed FROM
                (LEAST($3::bigint,COALESCE(char_length(exposed),0)::bigint)+1)::integer
                FOR $4::integer)
              FROM source"#,
        )
        .bind(individual)
        .bind(event)
        .bind(i64::from(offset))
        .bind(max_chars as i32)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let content = if let Some(length) = length {
            let total = length as u32;
            if offset > total {
                return Err(StoreError::Invalid("source event offset out of range"));
            }
            let end = (u64::from(offset) + u64::from(max_chars)).min(u64::from(total)) as u32;
            Some(
                json!({"offset":offset,"max_chars":max_chars,"total_chars":total,
                "text":fragment.ok_or(StoreError::Invalid("source event text unavailable"))?,
                "next_offset":if end<total {Some(end)}else{None}}),
            )
        } else {
            if offset != 0 {
                return Err(StoreError::Invalid("source event has no exposed text"));
            }
            None
        };
        // A matching correlation or payload UUID proves neither of these links.
        let document_id: Option<Uuid> = if metadata["kind"] == "document.imported" {
            sqlx::query_scalar(
                "SELECT event_id FROM documents WHERE individual_id=$1 AND event_id=$2",
            )
            .bind(individual)
            .bind(event)
            .fetch_optional(&mut *tx)
            .await?
        } else {
            None
        };
        let links: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as(
            "SELECT id,NULL::uuid AS action FROM runs WHERE individual_id=$1 AND trigger_event_id=$2 \
             UNION SELECT run_id,id AS action FROM actions WHERE individual_id=$1 AND result_event_id=$2 \
             ORDER BY 1,2 NULLS FIRST",
        )
        .bind(individual)
        .bind(event)
        .fetch_all(&mut *tx)
        .await?;
        let mut run_id = None;
        for (run, action) in links {
            let mut objects = vec![individual, run];
            objects.extend(action);
            if !unavailable(&mut tx, individual, &objects).await? {
                run_id = Some(run);
                break;
            }
        }
        let (state_version, privacy_epoch, observed_at): (i64, i64, DateTime<Utc>) =
            sqlx::query_as(
                "SELECT state_version,privacy_epoch,clock_timestamp() FROM individuals WHERE id=$1",
            )
            .bind(individual)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,
            "privacy_epoch":privacy_epoch,"observed_at":observed_at,"event":metadata,
            "content_status":if content.is_some(){"available"}else{"not_exposed"},
            "content":content,"document_id":document_id,"run_id":run_id}),
        )
    }
}
