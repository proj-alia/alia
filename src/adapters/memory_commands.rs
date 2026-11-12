//! Explicit owner corrections preserve the original events and their provenance.
use super::{
    api::{VISIBLE, existing},
    goals::unavailable,
    store::{
        Store, StoreError, advance, apply_memories, digest, edge, insert_event, outbox, replay,
        save_receipt, state,
    },
};
use crate::domain::{
    Lifecycle,
    history::{MemoryKind, NewMemory},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRevision {
    pub content: String,
    pub scope: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryRevisionReceipt {
    pub individual_id: Uuid,
    pub memory_id: Uuid,
    pub previous_memory_id: Uuid,
    pub event_id: Uuid,
    /// Global state version, distinct from either memory object's version.
    pub version: i64,
    pub memory_version: i64,
    pub previous_memory_version: i64,
}

fn valid_text(text: &str, maximum: usize) -> bool {
    !text.trim().is_empty() && !text.contains('\0') && text.chars().count() <= maximum
}

// Both provenance and supersession traversals use UNION to terminate on cycles.
// The traversal never crosses an unavailable row or an incompatible scope.
const HISTORY_CHAIN: &str = r#", eligible AS (
  SELECT m.* FROM memories m
  WHERE m.individual_id=$1
    AND m.scope=(SELECT scope FROM memories WHERE individual_id=$1 AND id=$2)
    AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=m.individual_id
      AND h.id IN (m.individual_id,m.id))
    AND NOT EXISTS(SELECT 1 FROM memory_sources s LEFT JOIN events e
      ON (e.individual_id,e.id)=(s.individual_id,s.event_id)
      WHERE (s.individual_id,s.memory_id)=(m.individual_id,m.id)
      AND (e.id IS NULL OR EXISTS(SELECT 1 FROM hidden h
        WHERE h.individual_id=s.individual_id AND h.id=s.event_id)))
), links(id,neighbor) AS (
  SELECT m.id,p.id FROM eligible m JOIN eligible p ON m.supersedes=p.id
  UNION SELECT p.id,m.id FROM eligible m JOIN eligible p ON m.supersedes=p.id
), connected(id) AS (
  SELECT id FROM eligible WHERE id=$2
  UNION SELECT l.neighbor FROM links l JOIN connected c ON l.id=c.id
) "#;

fn history_position(
    cursor: Option<&str>,
    target: Uuid,
) -> Result<Option<(DateTime<Utc>, Uuid)>, StoreError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let invalid = || StoreError::Invalid("invalid memory history cursor");
    if cursor.len() > 200 {
        return Err(invalid());
    }
    let fields: Vec<_> = cursor.split(':').collect();
    if fields.len() != 5
        || fields[0] != "v1"
        || fields[1] != "memory-history-asc"
        || fields[2].parse::<Uuid>().ok() != Some(target)
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
    /// The current connected correction chain, with an exact visible anchor.
    pub async fn owner_memory_history(
        &self,
        target: Uuid,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Value, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("invalid memory history limit"));
        }
        let after = history_position(cursor, target)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        if unavailable(&mut tx, individual, &[individual, target]).await? {
            return Err(StoreError::NotFound);
        }
        let target_visible: bool = sqlx::query_scalar(&format!(
            "{VISIBLE}{HISTORY_CHAIN} SELECT EXISTS(SELECT 1 FROM connected WHERE id=$2)"
        ))
        .bind(individual)
        .bind(target)
        .fetch_one(&mut *tx)
        .await?;
        if !target_visible {
            return Err(StoreError::NotFound);
        }
        if let Some((time, anchor)) = after {
            let valid: bool = sqlx::query_scalar(&format!(
                "{VISIBLE}{HISTORY_CHAIN} SELECT EXISTS(SELECT 1 FROM connected c \
                 JOIN eligible m ON m.id=c.id WHERE m.id=$3 AND m.created_at=$4)"
            ))
            .bind(individual)
            .bind(target)
            .bind(anchor)
            .bind(time)
            .fetch_one(&mut *tx)
            .await?;
            if !valid {
                return Err(StoreError::Invalid("unavailable memory history cursor"));
            }
        }
        let (state_version, privacy_epoch, observed_at): (i64, i64, DateTime<Utc>) =
            sqlx::query_as(
                "SELECT state_version,privacy_epoch,clock_timestamp() FROM individuals WHERE id=$1",
            )
            .bind(individual)
            .fetch_one(&mut *tx)
            .await?;
        let mut rows: Vec<(DateTime<Utc>, Uuid, Value, Option<Value>)> =
            sqlx::query_as(&format!(r#"{VISIBLE}{HISTORY_CHAIN}
            SELECT m.created_at,m.id,to_jsonb(m)||jsonb_build_object(
              'supersedes',CASE WHEN EXISTS(SELECT 1 FROM eligible p WHERE p.id=m.supersedes)
                THEN m.supersedes ELSE NULL END,
              'source_event_ids',ARRAY(SELECT s.event_id FROM memory_sources s WHERE
                (s.individual_id,s.memory_id)=(m.individual_id,m.id) ORDER BY s.event_id)),
              (SELECT jsonb_build_object('event_id',e.id,'reason',e.payload->>'reason',
                'occurred_at',e.occurred_at)
               FROM memory_sources s JOIN events e
                 ON (e.individual_id,e.id)=(s.individual_id,s.event_id)
               WHERE (s.individual_id,s.memory_id)=(m.individual_id,m.id)
                 AND e.kind='memory.corrected' AND e.actor_id='owner'
                 AND e.source_kind='reported_claim'
                 AND e.payload->>'previous_memory_id'=m.supersedes::text
                 AND e.payload->>'text'=m.content AND e.payload->>'scope'=m.scope
                 AND jsonb_typeof(e.payload->'reason')='string'
                 AND char_length(e.payload->>'reason') BETWEEN 1 AND 1000
                 AND (e.payload->>'reason') ~ '[^[:space:]]'
                 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=e.individual_id AND h.id=e.id)
               ORDER BY e.sequence DESC,e.id DESC LIMIT 1)
            FROM eligible m JOIN connected c ON c.id=m.id
            WHERE ($3::timestamptz IS NULL OR (m.created_at,m.id)>($3,$4::uuid))
            ORDER BY m.created_at,m.id LIMIT $5"#))
            .bind(individual)
            .bind(target)
            .bind(after.map(|p| p.0))
            .bind(after.map(|p| p.1))
            .bind(i64::from(limit) + 1)
            .fetch_all(&mut *tx)
            .await?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor = if more {
            rows.last().map(|(time, anchor, _, _)| {
                format!(
                    "v1:memory-history-asc:{target}:{}:{anchor}",
                    time.timestamp_micros()
                )
            })
        } else {
            None
        };
        let mut memories: Vec<_> = rows.iter().map(|row| row.2.clone()).collect();
        Self::annotate_memories(&mut tx, individual, &mut memories).await?;
        let items: Vec<_> = rows
            .into_iter()
            .zip(memories)
            .map(|((_, _, _, correction), memory)| {
                let correction = correction.filter(|value| {
                    value["reason"]
                        .as_str()
                        .is_some_and(|reason| valid_text(reason, 1000))
                });
                json!({"memory":memory,"correction":correction})
            })
            .collect();
        tx.commit().await?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,
            "privacy_epoch":privacy_epoch,"observed_at":observed_at,"target_memory_id":target,
            "items":items,"next_cursor":next_cursor}),
        )
    }

    /// Return the same source/person projection as the public memory archive.
    pub async fn owner_memory(&self, memory: Uuid) -> Result<Value, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        if unavailable(&mut tx, individual, &[individual, memory]).await? {
            return Err(StoreError::NotFound);
        }
        let (version,): (i64,) =
            sqlx::query_as("SELECT state_version FROM individuals WHERE id=$1")
                .bind(individual)
                .fetch_one(&mut *tx)
                .await?;
        let value: Value = sqlx::query_scalar(
            r#"SELECT to_jsonb(m)||jsonb_build_object('source_event_ids',ARRAY(
             SELECT s.event_id FROM memory_sources s WHERE
             (s.individual_id,s.memory_id)=(m.individual_id,m.id) ORDER BY s.event_id))
             FROM memories m WHERE m.individual_id=$1 AND m.id=$2"#,
        )
        .bind(individual)
        .bind(memory)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let mut rows = vec![value];
        Self::annotate_memories(&mut tx, individual, &mut rows).await?;
        tx.commit().await?;
        Ok(json!({"individual_id":individual,"state_version":version,"memory":rows[0]}))
    }

    /// Owner statements do not pass through the model or become objective truth.
    pub async fn revise_owner_memory(
        &self,
        key: &str,
        memory: Uuid,
        expected_memory_version: i64,
        revision: &MemoryRevision,
    ) -> Result<MemoryRevisionReceipt, StoreError> {
        if expected_memory_version <= 0
            || !valid_text(&revision.content, 2000)
            || !valid_text(&revision.scope, 200)
            || !valid_text(&revision.reason, 1000)
        {
            return Err(StoreError::Invalid("memory revision fields"));
        }
        let route = format!("/memories/{memory}/revisions");
        let hash = digest(&json!({"expected":expected_memory_version,"revision":revision}))?;
        let mut tx = self.write().await?;
        if let Some(receipt) =
            replay::<MemoryRevisionReceipt>(&mut tx, "owner", &route, key, &hash).await?
        {
            let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
            if current.id != receipt.individual_id
                || unavailable(
                    &mut tx,
                    current.id,
                    &[
                        current.id,
                        receipt.previous_memory_id,
                        receipt.memory_id,
                        receipt.event_id,
                    ],
                )
                .await?
            {
                return Err(StoreError::NotFound);
            }
            return Ok(receipt);
        }
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if unavailable(&mut tx, current.id, &[current.id, memory]).await? {
            return Err(StoreError::NotFound);
        }
        if !matches!(current.lifecycle, Lifecycle::Active | Lifecycle::Paused) {
            return Err(StoreError::Inactive);
        }
        let (previous_version, validity, kind, scope): (i64, String, String, String) =
            sqlx::query_as(
                "SELECT version,validity,kind,scope FROM memories WHERE individual_id=$1 AND id=$2",
            )
            .bind(current.id)
            .bind(memory)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        if previous_version != expected_memory_version || validity == "superseded" {
            return Err(StoreError::StaleState);
        }
        if scope != revision.scope {
            return Err(StoreError::Invalid("memory correction must preserve scope"));
        }
        let kind: MemoryKind = serde_json::from_value(json!(kind))?;
        let version = current
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?;
        let previous_memory_version = previous_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("memory version exhausted"))?;
        let event = Uuid::new_v4();
        insert_event(
            &mut tx,
            current.id,
            event,
            "memory.corrected",
            "reported_claim",
            "owner",
            json!({"previous_memory_id":memory,"previous_memory_version":previous_version,
                "text":revision.content,"scope":revision.scope,"reason":revision.reason}),
            event,
            Some(key),
        )
        .await?;
        // The old memory stays in the provenance graph, not in the new claim's
        // factual source list. Revoking its ancestors still revokes this chain.
        edge(&mut tx, current.id, memory, event).await?;
        let ids = apply_memories(
            &mut tx,
            current.id,
            &[memory, event],
            &[NewMemory {
                content: revision.content.clone(),
                scope: revision.scope.clone(),
                kind,
                source_event_ids: vec![event],
                supersedes: Some(memory),
            }],
        )
        .await?;
        let corrected = ids[0];
        // Fresh ID/version cannot inherit old vectors or person assertions.
        // The old version bump also fences pending index results and links.
        outbox(&mut tx, current.id, "memory.changed", memory, version).await?;
        outbox(&mut tx, current.id, "memory.changed", corrected, version).await?;
        advance(&mut tx, current.id, version).await?;
        let receipt = MemoryRevisionReceipt {
            individual_id: current.id,
            memory_id: corrected,
            previous_memory_id: memory,
            event_id: event,
            version,
            memory_version: 1,
            previous_memory_version,
        };
        save_receipt(&mut tx, "owner", &route, key, &hash, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }
}
