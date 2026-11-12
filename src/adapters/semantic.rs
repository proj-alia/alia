//! Exact vector storage/search. Inference happens outside these transactions.
use super::{goals::unavailable, store::*};
use crate::domain::{history::Memory, retrieval::*};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;
// Private fencing fields cannot be supplied by the model or an HTTP caller.
pub struct EmbeddingSnapshot {
    pub(super) individual: Uuid,
    pub(super) id: Uuid,
    pub(super) version: i64,
    pub(super) epoch: i64,
    pub(super) content: String,
    pub(super) digest: String,
}
impl EmbeddingSnapshot {
    pub fn content(&self) -> &str {
        &self.content
    }
}
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn vector_text(v: &[f32]) -> Result<String, StoreError> {
    if !valid_vector(v) {
        return Err(StoreError::Invalid("embedding vector"));
    }
    Ok(serde_json::to_string(v)?)
}
impl Store {
    pub async fn prepare_memory_index(&self, id: Uuid) -> Result<EmbeddingSnapshot, StoreError> {
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if unavailable(&mut tx, current.id, &[current.id, id]).await? {
            return Err(StoreError::NotFound);
        }
        let row: Option<(String, i64)> =
            sqlx::query_as("SELECT content,version FROM memories WHERE individual_id=$1 AND id=$2")
                .bind(current.id)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let (content, version) = row.ok_or(StoreError::NotFound)?;
        let digest = hash(&content);
        tx.commit().await?;
        Ok(EmbeddingSnapshot {
            individual: current.id,
            id,
            version,
            epoch: current.privacy_epoch,
            content,
            digest,
        })
    }
    pub async fn commit_memory_index(
        &self,
        snapshot: &EmbeddingSnapshot,
        chunks: &[IndexedChunk],
    ) -> Result<(), StoreError> {
        let encoded = encode_chunks(snapshot, chunks)?;
        let mut tx = self.write().await?;
        commit_index(&mut tx, snapshot, chunks, encoded).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn search_semantic(&self, q: &SemanticQuery) -> Result<Vec<Memory>, StoreError> {
        if q.profile != INDEX_PROFILE || !(1..=20).contains(&q.limit) {
            return Err(StoreError::Invalid("embedding query profile or limit"));
        }
        let vector = vector_text(&q.vector)?;
        let rows:Vec<Value>=sqlx::query_scalar(r#"WITH RECURSIVE denied(individual_id,id) AS (
    SELECT individual_id,id FROM objects WHERE blocked UNION
    SELECT d.individual_id,d.target_id FROM derivation_edges d JOIN denied b ON (b.individual_id,b.id)=(d.individual_id,d.source_id)
   ), ranked AS (
    SELECT m.individual_id,m.id,min(v.embedding <=> $1::text::vector) AS distance
    FROM memories m JOIN individuals i ON i.id=m.individual_id
    JOIN memory_vector_sets s ON (s.individual_id,s.memory_id)=(m.individual_id,m.id)
    JOIN memory_vectors v ON (v.individual_id,v.memory_id,v.profile)=(s.individual_id,s.memory_id,s.profile)
    WHERE i.singleton AND s.profile=$2 AND s.memory_version=m.version AND s.privacy_epoch=i.privacy_epoch
    AND s.content_digest=encode(sha256(convert_to(m.content,'UTF8')),'hex')
    AND (SELECT count(*) FROM memory_vectors complete WHERE (complete.individual_id,complete.memory_id,complete.profile)=(s.individual_id,s.memory_id,s.profile))=s.chunk_count
    AND NOT EXISTS(SELECT 1 FROM denied d WHERE d.individual_id=m.individual_id AND d.id IN (m.id,i.id))
    AND (m.validity='active' OR ($3 AND m.validity='superseded'))
    AND m.valid_from<=$4 AND (m.valid_until IS NULL OR m.valid_until>$4)
    AND (m.scope='general' OR m.scope=$5)
    AND ($6::uuid IS NULL OR EXISTS(SELECT 1 FROM memory_sources ms JOIN relationship_events re ON (re.individual_id,re.event_id)=(ms.individual_id,ms.event_id) WHERE ms.individual_id=m.individual_id AND ms.memory_id=m.id AND re.subject_id=$6))
    AND ($8::uuid IS NULL OR EXISTS(SELECT 1 FROM memory_person_links link
      JOIN person_entities person ON (person.individual_id,person.id)=(link.individual_id,link.entity_id)
      WHERE link.individual_id=m.individual_id AND link.memory_id=m.id AND link.memory_version=m.version
      AND link.entity_id=$8 AND link.role='subject' AND link.content_digest=encode(sha256(convert_to(m.content,'UTF8')),'hex')
      AND NOT EXISTS(SELECT 1 FROM denied d WHERE d.individual_id=link.individual_id AND d.id IN (link.entity_id,link.source_event_id,link.assertion_event_id))))
    GROUP BY m.individual_id,m.id
   ) SELECT to_jsonb(m) || jsonb_build_object('source_event_ids',ARRAY(SELECT event_id FROM memory_sources ms WHERE (ms.individual_id,ms.memory_id)=(m.individual_id,m.id) ORDER BY event_id))
   FROM ranked r JOIN memories m ON (m.individual_id,m.id)=(r.individual_id,r.id)
   ORDER BY r.distance,m.id LIMIT $7"#).bind(vector).bind(&q.profile).bind(q.include_history).bind(q.at).bind(&q.scope).bind(q.subject_id).bind(i64::from(q.limit)).bind(q.person_id).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|v| serde_json::from_value(v).map_err(StoreError::from))
            .collect()
    }
}

pub(super) fn encode_chunks(
    snapshot: &EmbeddingSnapshot,
    chunks: &[IndexedChunk],
) -> Result<Vec<String>, StoreError> {
    if chunks.is_empty() || chunks.len() > 128 {
        return Err(StoreError::Invalid("embedding chunks"));
    }
    let chars: Vec<char> = snapshot.content.chars().collect();
    let mut covered = vec![false; chars.len()];
    let mut encoded = vec![];
    for c in chunks {
        if c.start_char >= c.end_char || c.end_char > chars.len() {
            return Err(StoreError::Invalid("embedding range"));
        }
        covered[c.start_char..c.end_char].fill(true);
        encoded.push(vector_text(&c.vector)?);
    }
    if chars
        .iter()
        .zip(covered)
        .any(|(c, yes)| !yes && !c.is_whitespace())
    {
        return Err(StoreError::Invalid("incomplete embedding coverage"));
    }
    Ok(encoded)
}
pub(super) async fn commit_index(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    snapshot: &EmbeddingSnapshot,
    chunks: &[IndexedChunk],
    encoded: Vec<String>,
) -> Result<(), StoreError> {
    let current = state(tx).await?.ok_or(StoreError::NotInitialized)?;
    if current.id != snapshot.individual
        || current.privacy_epoch != snapshot.epoch
        || unavailable(tx, current.id, &[current.id, snapshot.id]).await?
    {
        return Err(StoreError::StaleState);
    }
    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT content,version FROM memories WHERE individual_id=$1 AND id=$2")
            .bind(current.id)
            .bind(snapshot.id)
            .fetch_optional(&mut **tx)
            .await?;
    if row.as_ref().is_none_or(|(content, version)| {
        *version != snapshot.version || hash(content) != snapshot.digest
    }) {
        return Err(StoreError::StaleState);
    }
    sqlx::query(
        "DELETE FROM memory_vector_sets WHERE individual_id=$1 AND memory_id=$2 AND profile=$3",
    )
    .bind(current.id)
    .bind(snapshot.id)
    .bind(INDEX_PROFILE)
    .execute(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO memory_vector_sets(individual_id,memory_id,profile,memory_version,privacy_epoch,content_digest,chunk_count) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(current.id).bind(snapshot.id).bind(INDEX_PROFILE).bind(snapshot.version).bind(snapshot.epoch).bind(&snapshot.digest).bind(chunks.len() as i32).execute(&mut **tx).await?;
    for (i, (c, vector)) in chunks.iter().zip(encoded).enumerate() {
        sqlx::query("INSERT INTO memory_vectors(individual_id,memory_id,profile,ordinal,start_char,end_char,embedding) VALUES($1,$2,$3,$4,$5,$6,$7::text::vector)").bind(current.id).bind(snapshot.id).bind(INDEX_PROFILE).bind(i as i32).bind(c.start_char as i32).bind(c.end_char as i32).bind(vector).execute(&mut **tx).await?;
    }
    Ok(())
}
