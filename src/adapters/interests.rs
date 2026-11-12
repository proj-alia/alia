//! Stable interests: trusted writes, source-bound revisions and read-only selection.
use super::{
    api::existing,
    store::{
        Store, StoreError, advance, digest, edge, insert_event, object, outbox, replay,
        save_receipt, state,
    },
};
use crate::domain::{
    Lifecycle,
    cognition::{InterestRef, ModelContext},
    config::PersonaSeed,
    history::ExpectedState,
    interests::{
        CurrentInterest, CurrentInterestView, InterestContext, InterestDraft, InterestStatus,
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::collections::HashSet;
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
const ROUTE: &str = "/internal/interests";
const LINKS: &str = r#"WITH RECURSIVE links(target,source) AS (
 SELECT target_id,source_id FROM derivation_edges WHERE individual_id=$1
 UNION SELECT i.object_id,s.event_id FROM interests i JOIN interest_sources s ON s.individual_id=i.individual_id AND s.interest_id=i.id WHERE i.individual_id=$1
 UNION SELECT r.id,i.object_id FROM interest_revisions r JOIN interests i ON i.individual_id=r.individual_id AND i.id=r.interest_id WHERE r.individual_id=$1
 UNION SELECT id,event_id FROM interest_revisions WHERE individual_id=$1
 UNION SELECT id,previous_revision_id FROM interest_revisions WHERE individual_id=$1 AND previous_revision_id IS NOT NULL
 UNION SELECT object_id,revision_id FROM (SELECT DISTINCT ON(i.object_id) i.object_id,r.id AS revision_id FROM interests i JOIN interest_revisions r ON r.individual_id=i.individual_id AND r.interest_id=i.id WHERE i.individual_id=$1 ORDER BY i.object_id,r.after_version DESC,r.id DESC) latest
 UNION SELECT subject_id,event_id FROM relationship_events WHERE individual_id=$1
 UNION SELECT revision_id,event_id FROM relationship_revision_events WHERE individual_id=$1
 UNION SELECT id,previous_revision_id FROM relationship_revisions WHERE individual_id=$1 AND previous_revision_id IS NOT NULL
 UNION SELECT object_id,event_id FROM affective_sources WHERE individual_id=$1
 UNION SELECT id,supersedes FROM affective_attitudes WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT memory_id,event_id FROM memory_sources WHERE individual_id=$1
 UNION SELECT belief_id,event_id FROM belief_evidence WHERE individual_id=$1
 UNION SELECT id,supersedes FROM beliefs WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT autobiography_id,event_id FROM autobiography_events WHERE individual_id=$1
 UNION SELECT autobiography_id,belief_id FROM autobiography_beliefs WHERE individual_id=$1
 UNION SELECT id,supersedes FROM autobiographies WHERE individual_id=$1 AND supersedes IS NOT NULL
) "#;
const BAD: &str = r#",bad(root) AS (
 SELECT c.root FROM closure c LEFT JOIN objects o ON o.individual_id=$1 AND o.id=c.id WHERE o.id IS NULL OR o.blocked
 UNION SELECT c.root FROM closure c JOIN interests i ON i.individual_id=$1 AND i.object_id=c.id JOIN interest_sources s ON s.individual_id=i.individual_id AND s.interest_id=i.id JOIN events e ON e.individual_id=s.individual_id AND e.id=s.event_id WHERE e.source_kind='fictional_background' OR e.kind='decision.reviewed'
 UNION SELECT c.root FROM closure c JOIN interests i ON i.individual_id=$1 AND i.object_id=c.id WHERE NOT EXISTS(SELECT 1 FROM interest_sources s WHERE s.individual_id=i.individual_id AND s.interest_id=i.id)
) "#;
async fn available(tx: &mut Tx<'_>, individual: Uuid, ids: &[Uuid]) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(&format!("{LINKS},closure(root,id) AS (SELECT id,id FROM unnest($2::uuid[]) t(id) UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD} SELECT NOT EXISTS(SELECT 1 FROM bad)")).bind(individual).bind(ids).fetch_one(&mut **tx).await?)
}
const PROJECTION: &str = r#"jsonb_build_object('id',i.id,'object_id',i.object_id,'version',i.version,'description',i.description,'status',i.status,'origin_kind',i.origin_kind,'created_at',i.created_at,'updated_at',i.updated_at,'event_ids',ARRAY(SELECT event_id FROM interest_sources s WHERE s.individual_id=$1 AND s.interest_id=i.id ORDER BY event_id))"#;

/// Called exactly in the birth transaction, never as a read-side initializer.
pub(super) async fn seed_interests(
    tx: &mut Tx<'_>,
    individual: Uuid,
    seed: &Value,
    birth_event: Uuid,
) -> Result<(), StoreError> {
    PersonaSeed::parse(&serde_json::to_vec(seed)?)
        .map_err(|_| StoreError::Invalid("interest birth seed"))?;
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM individuals i JOIN events e ON e.individual_id=i.id AND e.id=$2 WHERE i.id=$1 AND i.seed=$3 AND i.seed_digest=$4 AND e.kind='individual.created' AND e.source_kind='direct_observation' AND e.actor_id='system' AND e.payload->>'seed_digest'=i.seed_digest AND e.sequence=(SELECT min(sequence) FROM events WHERE individual_id=i.id AND kind='individual.created'))").bind(individual).bind(birth_event).bind(seed).bind(digest(seed)?).fetch_one(&mut **tx).await?;
    if !valid || !available(tx, individual, &[individual, birth_event]).await? {
        return Err(StoreError::NotFound);
    }
    let values = seed["personality"]["initial_interests"]
        .as_array()
        .ok_or(StoreError::Invalid("seed interests"))?;
    for (index, value) in values.iter().enumerate() {
        let description = value
            .as_str()
            .filter(|v| !v.trim().is_empty() && !v.contains('\0') && v.chars().count() <= 2000)
            .ok_or(StoreError::Invalid("seed interest description"))?;
        let id = format!("interest:{individual}:{index}");
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM interests WHERE individual_id=$1 AND id=$2)",
        )
        .bind(individual)
        .bind(&id)
        .fetch_one(&mut **tx)
        .await?;
        if exists {
            continue;
        }
        let object_id = Uuid::new_v4();
        object(tx, individual, object_id, "interest").await?;
        sqlx::query("INSERT INTO interests(individual_id,id,object_id,description,origin_kind) VALUES($1,$2,$3,$4,'initial_seed')").bind(individual).bind(&id).bind(object_id).bind(description).execute(&mut **tx).await?;
        sqlx::query(
            "INSERT INTO interest_sources(individual_id,interest_id,event_id) VALUES($1,$2,$3)",
        )
        .bind(individual)
        .bind(&id)
        .bind(birth_event)
        .execute(&mut **tx)
        .await?;
        edge(tx, individual, birth_event, object_id).await?;
    }
    Ok(())
}
/// At most33 complete active entries; the caller owns its32-item context budget.
pub(super) async fn for_context(
    tx: &mut Tx<'_>,
    individual: Uuid,
) -> Result<Vec<InterestRef>, StoreError> {
    if !available(tx, individual, &[individual]).await? {
        return Err(StoreError::NotFound);
    }
    let rows:Vec<(String,String)>=sqlx::query_as(&format!("{LINKS},closure(root,id) AS (SELECT object_id,object_id FROM interests WHERE individual_id=$1 UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD} SELECT i.id,i.description FROM interests i WHERE i.individual_id=$1 AND i.status='active' AND NOT EXISTS(SELECT 1 FROM bad WHERE root=i.object_id) ORDER BY i.created_at,i.id LIMIT 33")).bind(individual).fetch_all(&mut **tx).await?;
    Ok(rows
        .into_iter()
        .map(|(id, description)| InterestRef { id, description })
        .collect())
}

/// Logical IDs are stable across relabels. Their complete object provenance is
/// reserved with the context even when no source prose is needed as motivation.
pub async fn context_sources(
    tx: &mut Tx<'_>,
    individual: Uuid,
    ids: &[String],
) -> Result<Vec<Uuid>, StoreError> {
    let roots: Vec<Uuid> = sqlx::query_scalar(
        "SELECT object_id FROM interests WHERE individual_id=$1 AND id=ANY($2) AND status='active'",
    )
    .bind(individual)
    .bind(ids)
    .fetch_all(&mut **tx)
    .await?;
    if roots.len() != ids.len() || !available(tx, individual, &roots).await? {
        return Err(StoreError::StaleState);
    }
    let sources:Vec<Uuid>=sqlx::query_scalar(&format!("{LINKS},closure(id) AS (SELECT id FROM unnest($2::uuid[]) root(id) UNION SELECT l.source FROM closure c JOIN links l ON l.target=c.id) SELECT id FROM closure ORDER BY id LIMIT 1025")).bind(individual).bind(&roots).fetch_all(&mut **tx).await?;
    if sources.len() > 1024 {
        return Err(StoreError::Invalid(
            "interest context provenance exceeds limit",
        ));
    }
    Ok(sources)
}

pub async fn active_object(
    tx: &mut Tx<'_>,
    individual: Uuid,
    id: &str,
) -> Result<Uuid, StoreError> {
    let object: Option<Uuid> = sqlx::query_scalar(
        "SELECT object_id FROM interests WHERE individual_id=$1 AND id=$2 AND status='active'",
    )
    .bind(individual)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    let object = object.ok_or(StoreError::NotFound)?;
    if !available(tx, individual, &[object]).await? {
        return Err(StoreError::NotFound);
    }
    Ok(object)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterestReceipt {
    pub individual_id: Uuid,
    pub interest_id: String,
    pub revision_id: Uuid,
    pub event_id: Uuid,
    pub version: i64,
    pub interest_version: i64,
}
async fn validate_interest(
    tx: &mut Tx<'_>,
    individual: Uuid,
    input_manifest: &[Uuid],
    draft: &InterestDraft,
    strict_manifest: bool,
) -> Result<(crate::domain::interests::ValidatedInterest, Option<Uuid>), StoreError> {
    draft
        .validate_shape()
        .map_err(|_| StoreError::Invalid("interest fields"))?;
    let inputs: HashSet<_> = input_manifest.iter().copied().collect();
    if inputs.is_empty() || inputs.len() != input_manifest.len() || inputs.contains(&Uuid::nil()) {
        return Err(StoreError::Invalid("interest manifest"));
    }
    if draft.event_ids.iter().any(|id| !inputs.contains(id)) {
        return Err(StoreError::Invalid("interest source outside manifest"));
    }
    let mut refs = input_manifest.to_vec();
    refs.push(individual);
    if !available(tx, individual, &refs).await? {
        return Err(StoreError::NotFound);
    }
    let inspected = if strict_manifest {
        input_manifest
    } else {
        draft.event_ids.as_slice()
    };
    let forbidden:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE individual_id=$1 AND id=ANY($2) AND (source_kind='fictional_background' OR kind='decision.reviewed'))").bind(individual).bind(inspected).fetch_one(&mut **tx).await?;
    if forbidden {
        return Err(StoreError::Invalid("interest needs actual events"));
    }
    let events: HashSet<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM events WHERE individual_id=$1 AND id=ANY($2)",
    )
    .bind(individual)
    .bind(input_manifest)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect();
    let current = if let Some(id) = &draft.id {
        let (object_id,version,description,status):(Uuid,i64,String,String)=sqlx::query_as("SELECT object_id,version,description,status FROM interests WHERE individual_id=$1 AND id=$2 FOR UPDATE").bind(individual).bind(id).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
        if draft.expected_version != Some(version) {
            return Err(StoreError::StaleState);
        }
        if !inputs.contains(&object_id) {
            return Err(StoreError::Invalid("interest object outside manifest"));
        }
        Some(CurrentInterest {
            individual_id: individual,
            id: id.clone(),
            object_id,
            version,
            description,
            status: serde_json::from_value(json!(status))?,
        })
    } else {
        None
    };
    let previous: Option<Uuid> = if let Some(old) = &current {
        sqlx::query_scalar("SELECT id FROM interest_revisions WHERE individual_id=$1 AND interest_id=$2 ORDER BY after_version DESC,id DESC LIMIT 1").bind(individual).bind(&old.id).fetch_optional(&mut **tx).await?
    } else {
        None
    };
    if previous.is_some_and(|id| !inputs.contains(&id)) {
        return Err(StoreError::Invalid("interest predecessor outside manifest"));
    }
    let validated = draft
        .clone()
        .validate(individual, &events, current.as_ref())
        .map_err(|_| StoreError::Invalid("interest source, current state or no change"))?;

    Ok((validated, previous))
}

async fn insert_interest(
    tx: &mut Tx<'_>,
    individual: Uuid,
    input_manifest: &[Uuid],
    validated: &crate::domain::interests::ValidatedInterest,
    previous: Option<Uuid>,
    version: i64,
    metadata: (Option<Uuid>, Option<&str>),
) -> Result<InterestReceipt, StoreError> {
    let draft = validated.draft();
    let current = validated.previous();
    let (correlation, key) = metadata;
    let id = current
        .as_ref()
        .map(|v| v.id.clone())
        .unwrap_or_else(|| format!("interest:{}", Uuid::new_v4()));
    let object_id = current
        .as_ref()
        .map(|v| v.object_id)
        .unwrap_or_else(Uuid::new_v4);
    let before = current.as_ref().map_or(0, |v| v.version);
    let interest_version = before
        .checked_add(1)
        .ok_or(StoreError::Invalid("interest version exhausted"))?;
    let revision_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    if let Some(old) = &current {
        let changed=sqlx::query("UPDATE interests SET description=$3,status=$4,version=$5,updated_at=clock_timestamp() WHERE individual_id=$1 AND id=$2 AND version=$6").bind(individual).bind(&id).bind(&draft.description).bind(draft.status.as_str()).bind(interest_version).bind(old.version).execute(&mut **tx).await?;
        if changed.rows_affected() != 1 {
            return Err(StoreError::StaleState);
        }
    } else {
        object(tx, individual, object_id, "interest").await?;
        sqlx::query("INSERT INTO interests(individual_id,id,object_id,description,status,origin_kind) VALUES($1,$2,$3,$4,'active','experience')").bind(individual).bind(&id).bind(object_id).bind(&draft.description).execute(&mut **tx).await?;
    }
    insert_event(tx,individual,event_id,if current.is_none(){"interest.formed"}else{"interest.revised"},"inference","individual",json!({"interest_id":id,"object_id":object_id,"revision_id":revision_id,"event_ids":draft.event_ids}),correlation.unwrap_or(event_id),key).await?;
    object(tx, individual, revision_id, "interest_revision").await?;
    sqlx::query("INSERT INTO interest_revisions(individual_id,id,interest_id,before_version,after_version,before_description,before_status,after_description,after_status,brief_basis,event_id,previous_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(individual).bind(revision_id).bind(&id).bind(before).bind(interest_version).bind(validated.previous().map(|v|v.description.as_str())).bind(validated.previous().map(|v|v.status.as_str())).bind(&draft.description).bind(draft.status.as_str()).bind(&draft.brief_basis).bind(event_id).bind(previous).execute(&mut **tx).await?;
    for source in &draft.event_ids {
        sqlx::query("INSERT INTO interest_sources(individual_id,interest_id,event_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(individual).bind(&id).bind(source).execute(&mut **tx).await?;
    }
    for source in input_manifest {
        edge(tx, individual, *source, event_id).await?;
        edge(tx, individual, *source, revision_id).await?;
        if *source != object_id {
            edge(tx, individual, *source, object_id).await?;
        }
    }
    edge(tx, individual, event_id, revision_id).await?;
    edge(tx, individual, revision_id, object_id).await?;
    outbox(tx, individual, "interest.changed", object_id, version).await?;
    let r = InterestReceipt {
        individual_id: individual,
        interest_id: id,
        revision_id,
        event_id,
        version,
        interest_version,
    };

    Ok(r)
}

impl Store {
    pub async fn record_interest(
        &self,
        key: &str,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        draft: &InterestDraft,
    ) -> Result<InterestReceipt, StoreError> {
        draft
            .validate_shape()
            .map_err(|_| StoreError::Invalid("interest fields"))?;
        let inputs: HashSet<_> = input_manifest.iter().copied().collect();
        if inputs.is_empty()
            || inputs.len() != input_manifest.len()
            || inputs.contains(&Uuid::nil())
        {
            return Err(StoreError::Invalid("interest manifest"));
        }
        let hash =
            digest(&json!({"expected":expected,"input_manifest":input_manifest,"draft":draft}))?;
        let mut tx = self.write().await?;
        if let Some(r) = replay::<InterestReceipt>(&mut tx, "individual", ROUTE, key, &hash).await?
        {
            let individual = existing(&mut tx).await?;
            let object_id:Option<Uuid>=sqlx::query_scalar("SELECT i.object_id FROM interests i JOIN interest_revisions r ON r.individual_id=i.individual_id AND r.interest_id=i.id AND r.id=$3 JOIN events e ON e.individual_id=r.individual_id AND e.id=r.event_id WHERE i.individual_id=$1 AND i.id=$2 AND e.id=$4 AND e.kind IN ('interest.formed','interest.revised') AND e.source_kind='inference'").bind(individual).bind(&r.interest_id).bind(r.revision_id).bind(r.event_id).fetch_optional(&mut *tx).await?;
            let object_id = object_id.ok_or(StoreError::NotFound)?;
            let mut refs = input_manifest.to_vec();
            refs.extend([individual, object_id, r.revision_id, r.event_id]);
            if individual != r.individual_id || !available(&mut tx, individual, &refs).await? {
                return Err(StoreError::NotFound);
            }
            return Ok(r);
        }
        let state = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if state.state_version != expected.version || state.privacy_epoch != expected.privacy_epoch
        {
            return Err(StoreError::StaleState);
        }
        if !matches!(state.lifecycle, Lifecycle::Active | Lifecycle::Paused) {
            return Err(StoreError::Inactive);
        }
        let (validated, previous) =
            validate_interest(&mut tx, state.id, input_manifest, draft, true).await?;
        let version = state
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?;
        let r = insert_interest(
            &mut tx,
            state.id,
            input_manifest,
            &validated,
            previous,
            version,
            (None, Some(key)),
        )
        .await?;
        advance(&mut tx, state.id, version).await?;
        save_receipt(&mut tx, "individual", ROUTE, key, &hash, &r).await?;
        tx.commit().await?;
        Ok(r)
    }
    pub async fn interests(&self, include_retired: bool, limit: u32) -> Result<Value, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("interest limit"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        if !available(&mut tx, individual, &[individual]).await? {
            return Err(StoreError::NotFound);
        }
        let mut items:Vec<Value>=sqlx::query_scalar(&format!("{LINKS},closure(root,id) AS (SELECT object_id,object_id FROM interests WHERE individual_id=$1 UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD} SELECT {PROJECTION} FROM interests i WHERE i.individual_id=$1 AND ($2 OR i.status='active') AND NOT EXISTS(SELECT 1 FROM bad WHERE root=i.object_id) ORDER BY i.created_at,i.id LIMIT $3")).bind(individual).bind(include_retired).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let (state_version, privacy_epoch, observed_at): (i64, i64, DateTime<Utc>) =
            sqlx::query_as(
                "SELECT state_version,privacy_epoch,clock_timestamp() FROM individuals WHERE id=$1",
            )
            .bind(individual)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,"privacy_epoch":privacy_epoch,"observed_at":observed_at,"items":items,"has_more":has_more}),
        )
    }
    pub async fn interest_history(&self, id: &str, limit: u32) -> Result<Value, StoreError> {
        if id.is_empty()
            || id.len() > 160
            || id.chars().any(char::is_control)
            || !(1..=100).contains(&limit)
        {
            return Err(StoreError::Invalid("interest history query"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        let object_id: Uuid =
            sqlx::query_scalar("SELECT object_id FROM interests WHERE individual_id=$1 AND id=$2")
                .bind(individual)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        if !available(&mut tx, individual, &[individual, object_id]).await? {
            return Err(StoreError::NotFound);
        }
        let mut items:Vec<Value>=sqlx::query_scalar(&format!("{LINKS},closure(root,id) AS (SELECT id,id FROM interest_revisions WHERE individual_id=$1 AND interest_id=$2 UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD} SELECT to_jsonb(r)-'individual_id' FROM interest_revisions r WHERE r.individual_id=$1 AND r.interest_id=$2 AND NOT EXISTS(SELECT 1 FROM bad WHERE root=r.id) ORDER BY r.created_at,r.id LIMIT $3")).bind(individual).bind(id).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let (state_version, privacy_epoch, observed_at): (i64, i64, DateTime<Utc>) =
            sqlx::query_as(
                "SELECT state_version,privacy_epoch,clock_timestamp() FROM individuals WHERE id=$1",
            )
            .bind(individual)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,"privacy_epoch":privacy_epoch,"observed_at":observed_at,"items":items,"has_more":has_more}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::interests::InterestStatus;
    use crate::{domain::config::RuntimeRules, runtime::capability::IMPLEMENTED_CAPABILITIES};
    #[sqlx::test]
    #[ignore = "P04 seed/context helpers: isolated PostgreSQL and total lock required"]
    async fn birth_helper_is_idempotent_after_retirement_and_context_returns_only_33_active(
        pool: sqlx::PgPool,
    ) {
        let s = Store::new(pool.clone());
        let seed = PersonaSeed::parse(include_bytes!("../../config/persona.example.json")).unwrap();
        let rules = RuntimeRules::parse(
            include_bytes!("../../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .unwrap();
        let born = s.initialize("helper-init", &seed, &rules).await.unwrap();
        let row: (String, Uuid, i64) =
            sqlx::query_as("SELECT id,object_id,version FROM interests ORDER BY id LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        let e = s
            .submit_message("helper-source", "合成实际来源", None)
            .await
            .unwrap()
            .event_id;
        let d = InterestDraft {
            id: Some(row.0.clone()),
            expected_version: Some(row.2),
            description: "经过修订的种子兴趣".into(),
            status: InterestStatus::Retired,
            event_ids: vec![e],
            brief_basis: "显式退休，不因缺席自动扣减".into(),
        };
        s.record_interest(
            "helper-retire",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[row.1, e],
            &d,
        )
        .await
        .unwrap();
        let before: Value = sqlx::query_scalar("SELECT to_jsonb(i) FROM interests i WHERE id=$1")
            .bind(&row.0)
            .fetch_one(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        seed_interests(
            &mut tx,
            born.individual_id,
            seed.as_value(),
            born.genesis_event_id,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let after: Value = sqlx::query_scalar("SELECT to_jsonb(i) FROM interests i WHERE id=$1")
            .bind(&row.0)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, after);
        for n in 0..34 {
            let d = InterestDraft {
                id: None,
                expected_version: None,
                description: format!("上下文兴趣{n}"),
                status: InterestStatus::Active,
                event_ids: vec![e],
                brief_basis: "实际来源".into(),
            };
            s.record_interest(
                &format!("helper-{n}"),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[e],
                &d,
            )
            .await
            .unwrap();
        }
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await
            .unwrap();
        let refs = for_context(&mut tx, born.individual_id).await.unwrap();
        assert_eq!(refs.len(), 33);
        assert!(!refs.iter().any(|v| v.id == row.0));
        tx.commit().await.unwrap();
        let mut changed = seed.as_value().clone();
        changed["identity"]["display_name"] = json!("不同的出生模板");
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(
            seed_interests(&mut tx, born.individual_id, &changed, born.genesis_event_id).await,
            Err(StoreError::NotFound)
        ));
        tx.rollback().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(
            seed_interests(&mut tx, born.individual_id, seed.as_value(), e).await,
            Err(StoreError::NotFound)
        ));
        tx.rollback().await.unwrap();
    }
}

async fn full_graph(tx: &mut Tx<'_>) -> Result<String, StoreError> {
    let graph = super::deletion::reference_graph_sql(tx).await?;
    Ok(format!(
        "WITH RECURSIVE links(target,source) AS (SELECT target,source FROM ({graph}) actual(source,target) UNION SELECT r.id,CASE WHEN x.id ~* '^[0-9a-f]{{8}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{12}}$' THEN x.id::uuid END FROM interest_revisions r JOIN events e ON e.individual_id=r.individual_id AND e.id=r.event_id CROSS JOIN LATERAL jsonb_array_elements_text(CASE WHEN jsonb_typeof(e.payload->'event_ids')='array' THEN e.payload->'event_ids' ELSE '[]'::jsonb END) x(id) WHERE r.individual_id=$1 AND x.id ~* '^[0-9a-f]{{8}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{12}}$') "
    ))
}
async fn complete_sources(
    tx: &mut Tx<'_>,
    individual: Uuid,
    object: Uuid,
) -> Result<Option<Vec<Uuid>>, StoreError> {
    let graph = full_graph(tx).await?;
    let rows:Vec<(Uuid,bool)>=sqlx::query_as(&format!("{graph},closure(id) AS (SELECT unnest($2::uuid[]) UNION SELECT l.source FROM closure c JOIN links l ON l.target=c.id) SELECT c.id,coalesce(o.blocked,true) OR EXISTS(SELECT 1 FROM development_targets d WHERE d.individual_id=$1 AND d.id=c.id AND d.provenance_removed) OR EXISTS(SELECT 1 FROM developments d WHERE d.individual_id=$1 AND d.id=c.id AND d.provenance_removed) OR EXISTS(SELECT 1 FROM development_revisions d WHERE d.individual_id=$1 AND d.id=c.id AND d.provenance_removed) FROM closure c LEFT JOIN objects o ON o.individual_id=$1 AND o.id=c.id ORDER BY c.id LIMIT 1025")).bind(individual).bind(vec![individual,object]).fetch_all(&mut **tx).await?;
    if rows.len() > 1024 || rows.iter().any(|(_, bad)| *bad) {
        return Ok(None);
    }
    Ok(Some(rows.into_iter().map(|(id, _)| id).collect()))
}
async fn interest_bundle(
    tx: &mut Tx<'_>,
    individual: Uuid,
    id: &str,
) -> Result<Option<(CurrentInterestView, Vec<crate::domain::history::Event>)>, StoreError> {
    let value:Option<Value>=sqlx::query_scalar(&format!("SELECT ({PROJECTION})-'created_at'-'updated_at' FROM interests i WHERE i.individual_id=$1 AND i.id=$2")).bind(individual).bind(id).fetch_optional(&mut **tx).await?;
    let Some(mut value) = value else {
        return Err(StoreError::NotFound);
    };
    let object: Uuid = serde_json::from_value(value["object_id"].clone())?;
    if !available(tx, individual, &[individual, object]).await? {
        return Ok(None);
    }
    let Some(provenance) = complete_sources(tx, individual, object).await? else {
        return Ok(None);
    };
    let revision_rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(r)-'individual_id'-'interest_id' FROM interest_revisions r WHERE individual_id=$1 AND interest_id=$2 ORDER BY after_version,id LIMIT 1025").bind(individual).bind(id).fetch_all(&mut **tx).await?;
    if revision_rows.len() > 1024 {
        return Ok(None);
    }
    let event_rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(e) FROM events e WHERE individual_id=$1 AND id=ANY($2) ORDER BY sequence",
    )
    .bind(individual)
    .bind(&provenance)
    .fetch_all(&mut **tx)
    .await?;
    let events: Vec<crate::domain::history::Event> = event_rows
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()?;
    value["provenance_ids"] = json!(provenance);
    value["basis_event_ids"] = json!(events.iter().map(|e| e.id).collect::<Vec<_>>());
    value["revisions"] = json!(revision_rows);
    let view: CurrentInterestView = serde_json::from_value(value)?;
    if !view.validate_shape() {
        return Err(StoreError::Invalid(
            "interest history state is inconsistent",
        ));
    }
    let mut expected_sources = HashSet::new();
    if view.origin_kind == "initial_seed" {
        let (seed, seed_digest): (Value, String) =
            sqlx::query_as("SELECT seed,seed_digest FROM individuals WHERE id=$1")
                .bind(individual)
                .fetch_one(&mut **tx)
                .await?;
        PersonaSeed::validate(seed.clone())
            .map_err(|_| StoreError::Invalid("interest immutable seed"))?;
        if digest(&seed)? != seed_digest {
            return Err(StoreError::Invalid("interest immutable seed digest"));
        }
        let prefix = format!("interest:{individual}:");
        let index: usize = view
            .id
            .strip_prefix(&prefix)
            .and_then(|v| v.parse().ok())
            .ok_or(StoreError::Invalid("interest stable seed identity"))?;
        let description = seed["personality"]["initial_interests"]
            .get(index)
            .and_then(Value::as_str)
            .ok_or(StoreError::Invalid("interest seed index"))?;
        let birth:Option<Uuid>=sqlx::query_scalar("SELECT id FROM events WHERE individual_id=$1 AND kind='individual.created' ORDER BY sequence LIMIT 1").bind(individual).fetch_optional(&mut **tx).await?;
        let birth = birth.ok_or(StoreError::NotFound)?;
        let actual = events
            .iter()
            .find(|e| e.id == birth)
            .ok_or(StoreError::NotFound)?;
        if actual.source_kind != crate::domain::history::SourceKind::DirectObservation
            || actual.actor_id != "system"
            || actual.payload["seed_digest"] != json!(seed_digest)
        {
            return Err(StoreError::Invalid("interest seed birth binding"));
        }
        if let Some(first) = view.revisions.first() {
            if first.before_description.as_deref() != Some(description)
                || first.before_status != Some(InterestStatus::Active)
            {
                return Err(StoreError::Invalid("interest initial revision baseline"));
            }
        } else if view.description != description || view.status != InterestStatus::Active {
            return Err(StoreError::Invalid("interest initial state baseline"));
        }
        expected_sources.insert(birth);
    }
    for revision in &view.revisions {
        let event = events
            .iter()
            .find(|e| e.id == revision.event_id)
            .ok_or(StoreError::NotFound)?;
        if event.source_kind != crate::domain::history::SourceKind::Inference
            || event.actor_id != "individual"
            || event.kind
                != if revision.before_version == 0 {
                    "interest.formed"
                } else {
                    "interest.revised"
                }
            || event.payload["interest_id"] != json!(view.id)
            || event.payload["object_id"] != json!(object)
            || event.payload["revision_id"] != json!(revision.id)
        {
            return Err(StoreError::Invalid("interest revision event binding"));
        }
        let ids: Vec<Uuid> = serde_json::from_value(event.payload["event_ids"].clone())?;
        if ids.is_empty()
            || ids.len() > 32
            || ids.contains(&Uuid::nil())
            || ids.iter().copied().collect::<HashSet<_>>().len() != ids.len()
            || ids
                .iter()
                .any(|id| !view.event_ids.contains(id) || !view.provenance_ids.contains(id))
        {
            return Err(StoreError::Invalid("interest revision source associations"));
        }
        expected_sources.extend(ids);
    }
    if expected_sources != view.event_ids.iter().copied().collect() {
        return Err(StoreError::Invalid("interest complete source associations"));
    }
    Ok(Some((view, events)))
}

/// Bounded read-only enrichment. It never repairs or initializes storage.
pub async fn enrich_context(tx: &mut Tx<'_>, context: &mut ModelContext) -> Result<(), StoreError> {
    let graph = full_graph(tx).await?;
    if !available(tx, context.individual_id, &[context.individual_id]).await? {
        return Err(StoreError::NotFound);
    }
    let ids:Vec<String>=sqlx::query_scalar(&format!("{graph},closure(root,id) AS (SELECT object_id,object_id FROM interests WHERE individual_id=$1 UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD} SELECT i.id FROM interests i WHERE i.individual_id=$1 AND NOT EXISTS(SELECT 1 FROM bad WHERE root=i.object_id) ORDER BY i.updated_at DESC,i.id LIMIT 33")).bind(context.individual_id).fetch_all(&mut **tx).await?;
    let mut next = context.clone();
    next.interest_context = Some(InterestContext {
        items: vec![],
        has_more: ids.len() > 32,
    });
    next.interests.clear();
    next.goal_origin_catalog
        .items
        .retain(|o| o.origin_type != "interest");
    if serde_json::to_vec(&next)?.len() > 96 * 1024 {
        return Err(StoreError::Invalid(
            "interest selection metadata exceeds context limit",
        ));
    }
    for id in ids.into_iter().take(32) {
        let Some((view, events)) = interest_bundle(tx, context.individual_id, &id).await? else {
            next.interest_context.as_mut().unwrap().has_more = true;
            continue;
        };
        let mut candidate = next.clone();
        for event in events {
            if let Some(existing) = candidate.events.iter().find(|e| e.id == event.id) {
                if serde_json::to_value(existing)? != serde_json::to_value(&event)? {
                    return Err(StoreError::StaleState);
                }
            } else {
                candidate.events.push(event)
            }
        }
        candidate.input_manifest.extend(&view.provenance_ids);
        candidate.input_manifest.sort();
        candidate.input_manifest.dedup();
        candidate.events.sort_by_key(|e| e.sequence);
        if view.status == InterestStatus::Active {
            candidate.interests.push(InterestRef {
                id: view.id.clone(),
                description: view.description.clone(),
            });
            candidate
                .goal_origin_catalog
                .items
                .push(crate::domain::cognition::GoalOriginOption {
                    origin_type: "interest".into(),
                    origin_id: view.id.clone(),
                    label: view.description.clone(),
                });
        }
        candidate
            .interest_context
            .as_mut()
            .unwrap()
            .items
            .push(view);
        if !candidate
            .interest_context
            .as_ref()
            .unwrap()
            .valid_for(&candidate)
        {
            return Err(StoreError::Invalid("invalid interest context"));
        }
        if serde_json::to_vec(&candidate)?.len() <= 96 * 1024 {
            next = candidate
        } else {
            next.interest_context.as_mut().unwrap().has_more = true
        }
    }
    next.goal_origin_catalog.has_more |= next.interest_context.as_ref().unwrap().has_more;
    *context = next;
    Ok(())
}

/// Shared caller owns commit/state advance/idempotency. No nested transactions.
pub async fn apply_model_changes(
    tx: &mut Tx<'_>,
    context: &ModelContext,
    changes: &[InterestDraft],
    decision: Uuid,
    version: i64,
) -> Result<Vec<Uuid>, StoreError> {
    if changes.is_empty() {
        return Ok(vec![]);
    }
    if changes.len() > 2 || decision.is_nil() {
        return Err(StoreError::Invalid("interest model changes limit"));
    }
    let current = state(tx).await?.ok_or(StoreError::NotInitialized)?;
    if current.id != context.individual_id
        || current.state_version != context.state_version
        || current.privacy_epoch != context.privacy_epoch
        || current.config_version != context.config_version
        || current.state_version.checked_add(1) != Some(version)
    {
        return Err(StoreError::StaleState);
    }
    if current.lifecycle != Lifecycle::Active {
        return Err(StoreError::Inactive);
    }
    let actual_decision:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events e JOIN runs r ON r.individual_id=e.individual_id AND r.id=e.correlation_id WHERE e.individual_id=$1 AND e.id=$2 AND e.kind='decision.accepted' AND e.source_kind='generated' AND e.actor_id='alia' AND r.id=$3)").bind(context.individual_id).bind(decision).bind(context.run_id).fetch_one(&mut **tx).await?;
    if !actual_decision {
        return Err(StoreError::Invalid("interest decision binding"));
    }
    let mut seen = HashSet::new();
    let mut validated = vec![];
    for draft in changes {
        let observed = draft
            .validate_context(context)
            .map_err(|_| StoreError::Invalid("interest model context"))?;
        if let Some(observed) = observed {
            if !seen.insert(&observed.id) {
                return Err(StoreError::Invalid("duplicate interest model revision"));
            }
            let actual = interest_bundle(tx, context.individual_id, &observed.id)
                .await?
                .ok_or(StoreError::StaleState)?
                .0;
            if &actual != observed {
                return Err(StoreError::StaleState);
            }
        }
        let (change, previous) = validate_interest(
            tx,
            context.individual_id,
            &context.input_manifest,
            draft,
            false,
        )
        .await?;
        validated.push((change, previous));
    }
    let mut ids = vec![];
    for (change, previous) in validated {
        let receipt = insert_interest(
            tx,
            context.individual_id,
            &context.input_manifest,
            &change,
            previous,
            version,
            (Some(context.run_id), None),
        )
        .await?;
        edge(tx, context.individual_id, decision, receipt.event_id).await?;
        edge(tx, context.individual_id, decision, receipt.revision_id).await?;
        let object_id: Uuid =
            sqlx::query_scalar("SELECT object_id FROM interests WHERE individual_id=$1 AND id=$2")
                .bind(context.individual_id)
                .bind(receipt.interest_id)
                .fetch_one(&mut **tx)
                .await?;
        edge(tx, context.individual_id, decision, object_id).await?;
        ids.push(object_id);
    }
    Ok(ids)
}
