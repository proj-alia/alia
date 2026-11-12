//! Trusted source-bound relationship changes. No HTTP/model entry point.
use super::{
    api::existing,
    store::{
        Store, StoreError, advance, digest, edge, insert_event, object, outbox, replay,
        save_receipt, state,
    },
};
use crate::domain::{
    Lifecycle,
    history::ExpectedState,
    relationships::{
        CurrentRelationship, PreviousRelationshipRevision, RelationshipRevisionDraft,
        RelationshipState,
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::collections::HashSet;
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
const ROUTE: &str = "/internal/relationship-revisions";
const LINKS: &str = r#"WITH RECURSIVE links(target,source) AS (
 SELECT target_id,source_id FROM derivation_edges WHERE individual_id=$1
 UNION SELECT subject_id,event_id FROM relationship_events WHERE individual_id=$1
 UNION SELECT revision_id,event_id FROM relationship_revision_events WHERE individual_id=$1
 UNION SELECT id,subject_id FROM relationship_revisions WHERE individual_id=$1
 UNION SELECT id,previous_revision_id FROM relationship_revisions WHERE individual_id=$1 AND previous_revision_id IS NOT NULL
 UNION SELECT subject_id,id FROM (SELECT DISTINCT ON(subject_id) subject_id,id FROM relationship_revisions WHERE individual_id=$1 ORDER BY subject_id,after_version DESC,id DESC) latest
 UNION SELECT object_id,event_id FROM affective_sources WHERE individual_id=$1
 UNION SELECT id,goal_id FROM affective_appraisals WHERE individual_id=$1 AND goal_id IS NOT NULL
 UNION SELECT id,subject_id FROM affective_appraisals WHERE individual_id=$1 AND subject_id IS NOT NULL
 UNION SELECT id,subject_id FROM affective_attitudes WHERE individual_id=$1 AND subject_id IS NOT NULL
 UNION SELECT id,supersedes FROM affective_attitudes WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT memory_id,event_id FROM memory_sources WHERE individual_id=$1
 UNION SELECT id,supersedes FROM memories WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT belief_id,event_id FROM belief_evidence WHERE individual_id=$1
 UNION SELECT id,supersedes FROM beliefs WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT autobiography_id,event_id FROM autobiography_events WHERE individual_id=$1
 UNION SELECT autobiography_id,belief_id FROM autobiography_beliefs WHERE individual_id=$1
 UNION SELECT id,supersedes FROM autobiographies WHERE individual_id=$1 AND supersedes IS NOT NULL
) "#;
const BAD: &str = r#",bad(root) AS (
 SELECT c.root FROM closure c LEFT JOIN objects o ON o.individual_id=$1 AND o.id=c.id WHERE o.id IS NULL OR o.blocked
 UNION SELECT c.root FROM closure c JOIN relationship_revision_events r ON r.individual_id=$1 AND r.revision_id=c.id JOIN events e ON e.individual_id=$1 AND e.id=r.event_id WHERE e.source_kind='fictional_background' OR e.kind='decision.reviewed'
 UNION SELECT c.root FROM closure c JOIN objects o ON o.individual_id=$1 AND o.id=c.id WHERE o.kind='relationship_revision' AND NOT EXISTS(SELECT 1 FROM relationship_revision_events r WHERE r.individual_id=$1 AND r.revision_id=c.id)
) "#;
async fn available(tx: &mut Tx<'_>, individual: Uuid, ids: &[Uuid]) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(&format!("{LINKS},closure(root,id) AS (SELECT id,id FROM unnest($2::uuid[]) t(id) UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD} SELECT NOT EXISTS(SELECT 1 FROM bad)")).bind(individual).bind(ids).fetch_one(&mut **tx).await?)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipRevisionReceipt {
    pub individual_id: Uuid,
    pub subject_id: Uuid,
    pub revision_id: Uuid,
    pub event_id: Uuid,
    pub version: i64,
    pub relationship_version: i64,
}
async fn validate_revision(
    tx: &mut Tx<'_>,
    individual: Uuid,
    input_manifest: &[Uuid],
    draft: &RelationshipRevisionDraft,
    strict_manifest: bool,
) -> Result<crate::domain::relationships::ValidatedRelationshipRevision, StoreError> {
    let inputs: HashSet<_> = input_manifest.iter().copied().collect();
    if !inputs.contains(&draft.subject_id) || draft.event_ids.iter().any(|id| !inputs.contains(id))
    {
        return Err(StoreError::Invalid(
            "relationship reference outside manifest",
        ));
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
        return Err(StoreError::Invalid("relationship needs actual events"));
    }
    let (relationship_version,value):(i64,Value)=sqlx::query_as("SELECT version,jsonb_build_object('stage',stage,'expectations',expectations,'boundaries',boundaries,'commitments',commitments,'unresolved',unresolved) FROM relationships WHERE individual_id=$1 AND subject_id=$2 FOR UPDATE").bind(individual).bind(draft.subject_id).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    if relationship_version != draft.expected_relationship_version {
        return Err(StoreError::StaleState);
    }
    let latest:Option<(Uuid,i64)>=sqlx::query_as("SELECT id,after_version FROM relationship_revisions WHERE individual_id=$1 AND subject_id=$2 ORDER BY after_version DESC,id DESC LIMIT 1").bind(individual).bind(draft.subject_id).fetch_optional(&mut **tx).await?;
    if latest.is_some_and(|(id, _)| !inputs.contains(&id)) {
        return Err(StoreError::Invalid(
            "latest relationship revision outside manifest",
        ));
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
    let relation = CurrentRelationship {
        individual_id: individual,
        subject_id: draft.subject_id,
        version: relationship_version,
        state: serde_json::from_value::<RelationshipState>(value)?,
    };
    let previous = latest.map(|(id, after_version)| PreviousRelationshipRevision {
        individual_id: individual,
        subject_id: draft.subject_id,
        id,
        after_version,
    });
    let validated = draft
        .clone()
        .validate(individual, &relation, &events, previous.as_ref())
        .map_err(|_| {
            StoreError::Invalid("relationship revision sources, predecessor or no change")
        })?;
    Ok(validated)
}
async fn insert_revision(
    tx: &mut Tx<'_>,
    validated: &crate::domain::relationships::ValidatedRelationshipRevision,
    input_manifest: &[Uuid],
    version: i64,
    correlation: Option<Uuid>,
    key: Option<&str>,
) -> Result<RelationshipRevisionReceipt, StoreError> {
    let individual = validated.individual_id();
    let draft = validated.draft();
    let relationship_version = draft.expected_relationship_version;
    let next_relationship_version = relationship_version
        .checked_add(1)
        .ok_or(StoreError::Invalid("relationship version exhausted"))?;
    let revision_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let after = validated.after();
    let changed=sqlx::query("UPDATE relationships SET stage=$3,expectations=$4,boundaries=$5,commitments=$6,unresolved=$7,version=$8 WHERE individual_id=$1 AND subject_id=$2 AND version=$9").bind(individual).bind(draft.subject_id).bind(&after.stage).bind(json!(after.expectations)).bind(json!(after.boundaries)).bind(json!(after.commitments)).bind(json!(after.unresolved)).bind(next_relationship_version).bind(relationship_version).execute(&mut **tx).await?;
    if changed.rows_affected() != 1 {
        return Err(StoreError::StaleState);
    }
    object(tx, individual, revision_id, "relationship_revision").await?;
    sqlx::query("INSERT INTO relationship_revisions(individual_id,id,subject_id,before_version,after_version,before_state,after_state,brief_basis,previous_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(individual).bind(revision_id).bind(draft.subject_id).bind(relationship_version).bind(next_relationship_version).bind(serde_json::to_value(validated.before())?).bind(serde_json::to_value(after)?).bind(&draft.brief_basis).bind(validated.previous_revision_id()).execute(&mut **tx).await?;
    for source in &draft.event_ids {
        sqlx::query("INSERT INTO relationship_revision_events(individual_id,revision_id,event_id) VALUES($1,$2,$3)").bind(individual).bind(revision_id).bind(source).execute(&mut **tx).await?;
    }
    insert_event(tx,individual,event_id,"relationship.revised","inference","individual",json!({"subject_id":draft.subject_id,"revision_id":revision_id,"before_version":relationship_version,"after_version":next_relationship_version}),correlation.unwrap_or(event_id),key).await?;
    for source in input_manifest {
        edge(tx, individual, *source, revision_id).await?;
        edge(tx, individual, *source, event_id).await?;
    }
    edge(tx, individual, event_id, revision_id).await?;
    edge(tx, individual, revision_id, draft.subject_id).await?;
    outbox(
        tx,
        individual,
        "relationship.changed",
        draft.subject_id,
        version,
    )
    .await?;
    Ok(RelationshipRevisionReceipt {
        individual_id: individual,
        subject_id: draft.subject_id,
        revision_id,
        event_id,
        version,
        relationship_version: next_relationship_version,
    })
}

pub(super) async fn apply_model_changes(
    tx: &mut Tx<'_>,
    c: &crate::domain::cognition::ModelContext,
    changes: &[RelationshipRevisionDraft],
    decision: Uuid,
    version: i64,
) -> Result<Vec<Uuid>, StoreError> {
    let mut ids = Vec::new();
    for draft in changes {
        let expected = draft
            .validate_context(c)
            .map_err(|_| StoreError::Invalid("relationship context revision"))?;
        let validated =
            validate_revision(tx, c.individual_id, &c.input_manifest, draft, false).await?;
        if validated.before() != expected.before()
            || validated.previous_revision_id() != expected.previous_revision_id()
        {
            return Err(StoreError::StaleState);
        }
        let r = insert_revision(
            tx,
            &validated,
            &c.input_manifest,
            version,
            Some(c.run_id),
            None,
        )
        .await?;
        edge(tx, c.individual_id, decision, r.event_id).await?;
        edge(tx, c.individual_id, decision, r.revision_id).await?;
        ids.push(r.revision_id);
    }
    Ok(ids)
}

impl Store {
    pub async fn revise_relationship(
        &self,
        key: &str,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        draft: &RelationshipRevisionDraft,
    ) -> Result<RelationshipRevisionReceipt, StoreError> {
        draft
            .validate_shape()
            .map_err(|_| StoreError::Invalid("relationship revision fields"))?;
        let inputs: HashSet<_> = input_manifest.iter().copied().collect();
        if inputs.is_empty()
            || inputs.len() != input_manifest.len()
            || inputs.contains(&Uuid::nil())
        {
            return Err(StoreError::Invalid("relationship manifest"));
        }
        let hash =
            digest(&json!({"expected":expected,"input_manifest":input_manifest,"draft":draft}))?;
        let mut tx = self.write().await?;
        if let Some(r) =
            replay::<RelationshipRevisionReceipt>(&mut tx, "individual", ROUTE, key, &hash).await?
        {
            let individual = existing(&mut tx).await?;
            let stored:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM relationship_revisions r JOIN events e ON e.individual_id=r.individual_id AND e.id=$4 WHERE r.individual_id=$1 AND r.id=$2 AND r.subject_id=$3 AND e.kind='relationship.revised' AND e.source_kind='inference' AND e.payload->>'revision_id'=r.id::text)").bind(individual).bind(r.revision_id).bind(r.subject_id).bind(r.event_id).fetch_one(&mut *tx).await?;
            let mut refs = input_manifest.to_vec();
            refs.extend([individual, r.subject_id, r.revision_id, r.event_id]);
            if individual != r.individual_id
                || !stored
                || !available(&mut tx, individual, &refs).await?
            {
                return Err(StoreError::NotFound);
            }
            return Ok(r);
        }
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if current.state_version != expected.version
            || current.privacy_epoch != expected.privacy_epoch
        {
            return Err(StoreError::StaleState);
        }
        if !matches!(current.lifecycle, Lifecycle::Active | Lifecycle::Paused) {
            return Err(StoreError::Inactive);
        }
        let validated = validate_revision(&mut tx, current.id, input_manifest, draft, true).await?;
        let version = current
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?;
        let receipt = insert_revision(
            &mut tx,
            &validated,
            input_manifest,
            version,
            None,
            Some(key),
        )
        .await?;
        advance(&mut tx, current.id, version).await?;
        save_receipt(&mut tx, "individual", ROUTE, key, &hash, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }
    pub async fn relationship_history(
        &self,
        subject: Uuid,
        limit: u32,
    ) -> Result<Value, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("relationship history limit"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM relationships WHERE individual_id=$1 AND subject_id=$2)",
        )
        .bind(individual)
        .bind(subject)
        .fetch_one(&mut *tx)
        .await?;
        if !exists || !available(&mut tx, individual, &[individual, subject]).await? {
            return Err(StoreError::NotFound);
        }
        let mut items:Vec<Value>=sqlx::query_scalar(&format!("{LINKS},closure(root,id) AS (SELECT id,id FROM relationship_revisions WHERE individual_id=$1 AND subject_id=$2 UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD} SELECT (to_jsonb(r)-'individual_id')||jsonb_build_object('event_ids',ARRAY(SELECT event_id FROM relationship_revision_events e WHERE e.individual_id=$1 AND e.revision_id=r.id ORDER BY event_id)) FROM relationship_revisions r WHERE r.individual_id=$1 AND r.subject_id=$2 AND NOT EXISTS(SELECT 1 FROM bad WHERE root=r.id) ORDER BY created_at DESC,id DESC LIMIT $3")).bind(individual).bind(subject).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
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
