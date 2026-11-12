//! Shared scoped retrieval. Local encoding occurs before opening the read snapshot.
use super::{goals::unavailable, store::*};
use crate::domain::{cognition::query_terms, history::Memory, retrieval::*};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

// Both candidate pools and coverage use exactly the same eligibility predicates.
const SCOPE: &str = r#"WITH RECURSIVE denied(individual_id,id) AS (
 SELECT individual_id,id FROM objects WHERE blocked UNION
 SELECT d.individual_id,d.target_id FROM derivation_edges d JOIN denied b ON (b.individual_id,b.id)=(d.individual_id,d.source_id)
), eligible AS (
 SELECT m.*,i.privacy_epoch AS index_epoch FROM memories m JOIN individuals i ON i.id=m.individual_id
 WHERE i.singleton AND (m.validity='active' OR ($2 AND m.validity='superseded'))
 AND m.valid_from<=$3 AND (m.valid_until IS NULL OR m.valid_until>$3)
 AND (m.scope='general' OR m.scope=$4)
 AND NOT EXISTS(SELECT 1 FROM denied d WHERE d.individual_id=m.individual_id AND d.id IN(m.id,i.id))
 AND ($5::uuid IS NULL OR EXISTS(SELECT 1 FROM memory_sources ms JOIN relationship_events re ON (re.individual_id,re.event_id)=(ms.individual_id,ms.event_id) WHERE ms.individual_id=m.individual_id AND ms.memory_id=m.id AND re.subject_id=$5))
 AND ($6::uuid IS NULL OR EXISTS(SELECT 1 FROM memory_person_links link WHERE link.individual_id=m.individual_id AND link.memory_id=m.id AND link.memory_version=m.version AND link.content_digest=encode(sha256(convert_to(m.content,'UTF8')),'hex') AND link.entity_id=$6 AND link.role='subject' AND NOT EXISTS(SELECT 1 FROM denied d WHERE d.individual_id=link.individual_id AND d.id IN(link.entity_id,link.source_event_id,link.assertion_event_id))))
), indexed AS (
 SELECT e.id FROM eligible e JOIN memory_vector_sets s ON(s.individual_id,s.memory_id)=(e.individual_id,e.id)
 WHERE s.profile=$1 AND s.memory_version=e.version AND s.privacy_epoch=e.index_epoch AND s.content_digest=encode(sha256(convert_to(e.content,'UTF8')),'hex')
 AND s.chunk_count=(SELECT count(*) FROM memory_vectors v WHERE(v.individual_id,v.memory_id,v.profile)=(s.individual_id,s.memory_id,s.profile))
) "#;
const MEMORY: &str = "to_jsonb(e)-'index_epoch'||jsonb_build_object('source_event_ids',ARRAY(SELECT event_id FROM memory_sources ms WHERE(ms.individual_id,ms.memory_id)=(e.individual_id,e.id) ORDER BY event_id))";
macro_rules! scope {
    ($query:expr,$q:expr) => {
        $query
            .bind(INDEX_PROFILE)
            .bind($q.include_history)
            .bind($q.at)
            .bind(&$q.scope)
            .bind($q.subject_id)
            .bind($q.person_id)
    };
}
// Read the complete exact-alias match, independent of the bounded model catalogue.
// Names are query parameters, not instructions and not a new identity assertion.
async fn person_target(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    individual: Uuid,
    q: &RetrievalRequest,
) -> Result<(Option<Uuid>, Vec<Uuid>), StoreError> {
    if let Some(name) = &q.person_name {
        let rows: Vec<(Uuid,Uuid,Uuid)> = sqlx::query_as(&format!(
            "{} SELECT a.entity_id,a.source_event_id,a.assertion_event_id FROM person_aliases a JOIN person_entities p ON (p.individual_id,p.id)=(a.individual_id,a.entity_id) WHERE a.individual_id=$1 AND a.surface=$2 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=a.individual_id AND h.id IN(a.entity_id,a.source_event_id,a.assertion_event_id)) ORDER BY a.entity_id LIMIT 2",
            super::api::VISIBLE
        )).bind(individual).bind(name).fetch_all(&mut **tx).await?;
        return match rows.as_slice() {
            [] => Err(StoreError::NotFound),
            [(person, source, assertion)] => {
                let mut evidence = vec![*person, *source, *assertion];
                evidence.sort();
                evidence.dedup();
                Ok((Some(*person), evidence))
            }
            _ => Err(StoreError::PersonAmbiguous),
        };
    }
    if let Some(person) = q.person_id {
        if unavailable(tx, individual, &[person]).await?
            || !sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM person_entities WHERE individual_id=$1 AND id=$2)",
            )
            .bind(individual)
            .bind(person)
            .fetch_one(&mut **tx)
            .await?
        {
            return Err(StoreError::NotFound);
        }
        return Ok((Some(person), vec![person]));
    }
    Ok((None, vec![]))
}
pub(super) async fn annotate(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    individual: Uuid,
    memories: Vec<Memory>,
) -> Result<Vec<Memory>, StoreError> {
    let mut values = memories
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?;
    Store::annotate_memories(tx, individual, &mut values).await?;
    values
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(StoreError::from))
        .collect()
}
impl Store {
    pub async fn public_retrieval(&self, q: &RetrievalRequest) -> Result<Value, StoreError> {
        let page = self.retrieve_memories(q).await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let current = read_state(&mut tx)
            .await?
            .ok_or(StoreError::NotInitialized)?;
        if unavailable(&mut tx, current.id, &[current.id]).await? {
            return Err(StoreError::NotFound);
        }
        let memories = Self::validate_retrieval_page(
            &mut tx,
            &page,
            current.id,
            current.state_version,
            current.privacy_epoch,
        )
        .await?;
        let items = memories
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(
            serde_json::json!({"items":items,"retrieval":page.summary,"state_version":page.state_version,"limit":q.limit,"exhaustive":false}),
        )
    }
    pub async fn retrieve_memories(
        &self,
        q: &RetrievalRequest,
    ) -> Result<RetrievalPage, StoreError> {
        if q.text.trim().is_empty()
            || q.text.contains('\0')
            || q.text.chars().count() > 1000
            || !(1..=20).contains(&q.limit)
            || (q.person_id.is_some() && q.person_name.is_some())
            || q.person_name.as_ref().is_some_and(|name| {
                name.trim().is_empty()
                    || name != name.trim()
                    || name.chars().count() > 80
                    || name.chars().any(char::is_control)
            })
            || q.scope
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.chars().count() > 200 || s.contains('\0'))
        {
            return Err(StoreError::Invalid("retrieval query"));
        }
        let current = self.snapshot().await?.ok_or(StoreError::NotInitialized)?;
        let vector = if let Some(encoder) = &self.encoder {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                encoder.encode_query(q.text.clone()),
            )
            .await;
            let v = match result {
                Ok(Ok(v)) => v,
                Ok(Err(crate::runtime::indexing::IndexEncodingError::Invalid)) => {
                    return Err(StoreError::Invalid("retrieval input encoding"));
                }
                _ => return Err(StoreError::RetrievalUnavailable),
            };
            if !valid_vector(&v) {
                return Err(StoreError::RetrievalUnavailable);
            }
            Some(v)
        } else {
            None
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let latest = read_state(&mut tx)
            .await?
            .ok_or(StoreError::NotInitialized)?;
        if current.id != latest.id
            || current.state_version != latest.state_version
            || current.privacy_epoch != latest.privacy_epoch
        {
            return Err(StoreError::StaleState);
        }
        if unavailable(&mut tx, current.id, &[current.id]).await? {
            return Err(StoreError::NotFound);
        }
        let (person_id, person_evidence) = person_target(&mut tx, current.id, q).await?;
        let request = q.clone();
        let mut effective = q.clone();
        effective.person_id = person_id;
        let q = &effective;
        let (eligible, indexed): (i64, i64) = scope!(
            sqlx::query_as(&format!(
                "{SCOPE} SELECT (SELECT count(*) FROM eligible),(SELECT count(*) FROM indexed)"
            )),
            q
        )
        .fetch_one(&mut *tx)
        .await?;
        // Semantic scores and keyword counts are not calibrated to one another.
        // Preserve semantic order; use keyword matches for records without a
        // current vector, interleaving one after every three semantic results.
        let lexical:Vec<Value>=scope!(sqlx::query_scalar(&format!("{SCOPE} SELECT {MEMORY} FROM eligible e WHERE $8::bool OR (NOT EXISTS(SELECT 1 FROM indexed ix WHERE ix.id=e.id) AND EXISTS(SELECT 1 FROM unnest($7::text[]) term WHERE strpos(lower(e.content),term)>0)) ORDER BY (e.scope=$4) DESC NULLS LAST,(SELECT count(*) FROM unnest($7::text[]) term WHERE strpos(lower(e.content),term)>0) DESC,e.created_at DESC,e.id LIMIT 20")),q).bind(query_terms(&q.text)).bind(vector.is_none()).fetch_all(&mut *tx).await?;
        let mut lexical: VecDeque<Memory> = lexical
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()?;
        let mut semantic: VecDeque<Memory> = if let Some(vector) = &vector {
            let rows:Vec<Value>=scope!(sqlx::query_scalar(&format!("{SCOPE}, ranked AS(SELECT e.id,min(v.embedding <=> $7::text::vector) AS distance FROM eligible e JOIN indexed ix ON ix.id=e.id JOIN memory_vectors v ON(v.individual_id,v.memory_id)=(e.individual_id,e.id) WHERE v.profile=$1 GROUP BY e.id) SELECT {MEMORY} FROM ranked r JOIN eligible e ON e.id=r.id ORDER BY r.distance,e.id LIMIT 20")),q).bind(serde_json::to_string(vector)?).fetch_all(&mut *tx).await?;
            rows.into_iter()
                .map(serde_json::from_value)
                .collect::<Result<_, _>>()?
        } else {
            VecDeque::new()
        };
        let mut ranked = Vec::new();
        while ranked.len() < q.limit as usize {
            let next = if ranked.len() % 4 == 3 {
                lexical.pop_front().or_else(|| semantic.pop_front())
            } else {
                semantic.pop_front().or_else(|| lexical.pop_front())
            };
            match next {
                Some(memory) => ranked.push(memory),
                None => break,
            }
        }
        let ranked = annotate(&mut tx, current.id, ranked).await?;
        tx.commit().await?;
        Ok(RetrievalPage {
            request,
            person_evidence,
            individual: current.id,
            state_version: current.state_version,
            privacy_epoch: current.privacy_epoch,
            memories: ranked,
            summary: RetrievalSummary {
                mode: if vector.is_some() {
                    "hybrid"
                } else {
                    "lexical"
                }
                .into(),
                profile: vector.as_ref().map(|_| INDEX_PROFILE.into()),
                ranking: if vector.is_some() {
                    "semantic-primary-unindexed-interleave3/1"
                } else {
                    "lexical/1"
                }
                .into(),
                eligible,
                indexed,
                query_characters: q.text.chars().count(),
                query_limited: false,
                scope: q.scope.clone(),
                person_id: q.person_id,
                person_filter: if q.person_name.is_some() {
                    "exact_alias"
                } else if q.person_id.is_some() {
                    "explicit_id"
                } else {
                    "none"
                }
                .into(),
                include_history: q.include_history,
                at: q.at,
            },
        })
    }
    /// Re-read selected content in the model/action snapshot; never inject an old private copy.
    pub(super) async fn validate_retrieval_page(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        page: &RetrievalPage,
        individual: Uuid,
        version: i64,
        epoch: i64,
    ) -> Result<Vec<Memory>, StoreError> {
        if (page.individual, page.state_version, page.privacy_epoch) != (individual, version, epoch)
        {
            return Err(StoreError::StaleState);
        }
        let (person_id, evidence) = match person_target(tx, individual, &page.request).await {
            Ok(target) => target,
            Err(StoreError::NotFound | StoreError::PersonAmbiguous) => {
                return Err(StoreError::StaleState);
            }
            Err(e) => return Err(e),
        };
        if person_id != page.summary.person_id || evidence != page.person_evidence {
            return Err(StoreError::StaleState);
        }
        let mut effective = page.request.clone();
        effective.person_id = person_id;
        let ids: Vec<Uuid> = page.memories.iter().map(|m| m.id).collect();
        let values: Vec<Value> = scope!(
            sqlx::query_scalar(&format!(
                "{SCOPE} SELECT {MEMORY} FROM eligible e WHERE e.id=ANY($7::uuid[])"
            )),
            effective
        )
        .bind(ids)
        .fetch_all(&mut **tx)
        .await?;
        let memories = values
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<Memory>, _>>()?;
        let mut current = HashMap::new();
        for m in annotate(tx, individual, memories).await? {
            current.insert(m.id, m);
        }
        for m in &page.memories {
            let fresh = current.get(&m.id).ok_or(StoreError::StaleState)?;
            if serde_json::to_value(fresh)? != serde_json::to_value(m)? {
                return Err(StoreError::StaleState);
            }
        }
        Ok(page.memories.clone())
    }
}
