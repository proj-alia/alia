//! Trusted appraisal/attitude storage. These engineering states grant no rights.
use super::{
    api::existing,
    store::{
        Store, StoreError, advance, digest, edge, insert_event, object, outbox, replay,
        save_receipt, state,
    },
};
use crate::domain::{
    Lifecycle,
    affect::{
        APPRAISAL_LIFETIME_SECONDS, AppraisalDraft, AttitudeDraft, PreviousAttitude,
        attention_weight,
    },
    history::ExpectedState,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::collections::HashSet;
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
const ROUTE: &str = "/internal/affect";
const LINKS: &str = r#"WITH RECURSIVE links(target,source) AS (
 SELECT target_id,source_id FROM derivation_edges WHERE individual_id=$1
 UNION SELECT object_id,event_id FROM affective_sources WHERE individual_id=$1
 UNION SELECT id,goal_id FROM affective_appraisals WHERE individual_id=$1 AND goal_id IS NOT NULL
 UNION SELECT id,subject_id FROM affective_appraisals WHERE individual_id=$1 AND subject_id IS NOT NULL
 UNION SELECT id,subject_id FROM affective_attitudes WHERE individual_id=$1 AND subject_id IS NOT NULL
 UNION SELECT id,supersedes FROM affective_attitudes WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT subject_id,event_id FROM relationship_events WHERE individual_id=$1
 UNION SELECT g.id,CASE WHEN x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN x.value::uuid END
 FROM goals g CROSS JOIN LATERAL jsonb_array_elements_text(CASE WHEN jsonb_typeof(g.origin_ids)='array' THEN g.origin_ids ELSE '[]' END) x(value)
 WHERE g.individual_id=$1 AND x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
 UNION SELECT memory_id,event_id FROM memory_sources WHERE individual_id=$1
 UNION SELECT id,supersedes FROM memories WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT belief_id,event_id FROM belief_evidence WHERE individual_id=$1
 UNION SELECT id,supersedes FROM beliefs WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT autobiography_id,event_id FROM autobiography_events WHERE individual_id=$1
 UNION SELECT autobiography_id,belief_id FROM autobiography_beliefs WHERE individual_id=$1
 UNION SELECT id,supersedes FROM autobiographies WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT id,trigger_event_id FROM runs WHERE individual_id=$1
 UNION SELECT id,goal_id FROM runs WHERE individual_id=$1 AND goal_id IS NOT NULL
 UNION SELECT result_event_id,id FROM actions WHERE individual_id=$1 AND result_event_id IS NOT NULL
 UNION SELECT id,run_id FROM actions WHERE individual_id=$1
 UNION SELECT id,decision_event_id FROM actions WHERE individual_id=$1
) "#;
const BAD: &str = r#", bad(root) AS (
 SELECT c.root FROM closure c LEFT JOIN objects o ON o.individual_id=$1 AND o.id=c.id WHERE o.id IS NULL OR o.blocked
 UNION SELECT c.root FROM closure c JOIN affective_sources s ON s.individual_id=$1 AND s.object_id=c.id JOIN events e ON e.individual_id=$1 AND e.id=s.event_id WHERE e.source_kind='fictional_background' OR e.kind='decision.reviewed'
 UNION SELECT c.root FROM closure c JOIN objects o ON o.individual_id=$1 AND o.id=c.id WHERE o.kind IN ('affective_state','affective_attitude') AND NOT EXISTS(SELECT 1 FROM affective_sources s WHERE s.individual_id=$1 AND s.object_id=c.id)
) "#;
async fn available(tx: &mut Tx<'_>, individual: Uuid, ids: &[Uuid]) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(&format!("{LINKS},closure(root,id) AS (SELECT id,id FROM unnest($2::uuid[]) t(id) UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD} SELECT NOT EXISTS(SELECT 1 FROM bad)"))
  .bind(individual).bind(ids).fetch_one(&mut **tx).await?)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AffectReceipt {
    pub individual_id: Uuid,
    pub appraisal_id: Option<Uuid>,
    pub attitude_id: Option<Uuid>,
    pub previous_attitude_id: Option<Uuid>,
    pub event_id: Uuid,
    pub version: i64,
}
async fn validate_update(
    tx: &mut Tx<'_>,
    individual: Uuid,
    input_manifest: &[Uuid],
    appraisal: Option<&AppraisalDraft>,
    attitude: Option<&AttitudeDraft>,
    strict_manifest: bool,
) -> Result<
    (
        Option<crate::domain::affect::ValidatedAppraisal>,
        Option<crate::domain::affect::ValidatedAttitude>,
    ),
    StoreError,
> {
    let inputs: HashSet<_> = input_manifest.iter().copied().collect();
    let mut refs = input_manifest.to_vec();
    refs.push(individual);
    if !available(tx, individual, &refs).await? {
        return Err(StoreError::NotFound);
    }
    let required: Vec<_> = appraisal
        .into_iter()
        .flat_map(|a| {
            a.event_ids
                .iter()
                .copied()
                .chain(a.goal_id)
                .chain(a.subject_id)
        })
        .chain(attitude.into_iter().flat_map(|a| {
            a.event_ids
                .iter()
                .copied()
                .chain(a.subject_id)
                .chain(a.supersedes)
        }))
        .collect();
    if required.iter().any(|id| !inputs.contains(id)) {
        return Err(StoreError::Invalid("affect reference outside manifest"));
    }
    let explicit: Vec<_> = appraisal
        .into_iter()
        .flat_map(|a| a.event_ids.iter().copied())
        .chain(
            attitude
                .into_iter()
                .flat_map(|a| a.event_ids.iter().copied()),
        )
        .collect();
    let inspected = if strict_manifest {
        input_manifest
    } else {
        explicit.as_slice()
    };
    let forbidden:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE individual_id=$1 AND id=ANY($2) AND (source_kind='fictional_background' OR kind='decision.reviewed'))")
   .bind(individual).bind(inspected).fetch_one(&mut **tx).await?;
    if forbidden {
        return Err(StoreError::Invalid("affect needs actual events"));
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
    let goals: HashSet<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM goals WHERE individual_id=$1 AND id=ANY($2)")
            .bind(individual)
            .bind(input_manifest)
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .collect();
    let subjects: HashSet<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT subject_id FROM relationships WHERE individual_id=$1 AND subject_id=ANY($2)",
    )
    .bind(individual)
    .bind(input_manifest)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect();
    let previous = if let Some(id) = attitude.and_then(|a| a.supersedes) {
        let (topic,subject_id,version,status):(String,Option<Uuid>,i64,String)=sqlx::query_as("SELECT topic,subject_id,version,status FROM affective_attitudes WHERE individual_id=$1 AND id=$2").bind(individual).bind(id).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
        Some(PreviousAttitude {
            individual_id: individual,
            id,
            topic,
            subject_id,
            version,
            superseded: status != "current",
        })
    } else {
        None
    };
    let app = appraisal
        .cloned()
        .map(|a| a.validate(individual, &events, &goals, &subjects))
        .transpose()
        .map_err(|_| StoreError::Invalid("appraisal sources or targets"))?;
    let long = attitude
        .cloned()
        .map(|a| a.validate(individual, &events, &subjects, previous.as_ref()))
        .transpose()
        .map_err(|_| StoreError::Invalid("attitude sources, subject or predecessor"))?;
    if let Some(a) = &long
        && previous.is_none()
    {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM affective_attitudes WHERE individual_id=$1 AND topic=$2 AND status='current')").bind(individual).bind(&a.draft().topic).fetch_one(&mut **tx).await?;
        if exists {
            return Err(StoreError::Invalid("attitude needs current predecessor"));
        }
    }
    Ok((app, long))
}
async fn insert_update(
    tx: &mut Tx<'_>,
    individual: Uuid,
    input_manifest: &[Uuid],
    app: Option<&crate::domain::affect::ValidatedAppraisal>,
    long: Option<&crate::domain::affect::ValidatedAttitude>,
    version: i64,
    metadata: (Option<Uuid>, Option<&str>),
) -> Result<AffectReceipt, StoreError> {
    let (correlation, key) = metadata;
    let appraisal = app.map(|a| a.draft());
    let attitude = long.map(|a| a.draft());
    let appraisal_id = app.as_ref().map(|_| Uuid::new_v4());
    let attitude_id = long.as_ref().map(|_| Uuid::new_v4());
    let event_id = Uuid::new_v4();
    if let (Some(id), Some(a)) = (appraisal_id, app.as_ref()) {
        let old:Vec<Uuid>=sqlx::query_scalar("UPDATE affective_appraisals SET status='superseded' WHERE individual_id=$1 AND status='current' RETURNING id").bind(individual).fetch_all(&mut **tx).await?;
        for old in old {
            outbox(tx, individual, "affect.changed", old, version).await?;
        }
        let a = a.draft();
        object(tx, individual, id, "affective_state").await?;
        sqlx::query("WITH clock AS (SELECT clock_timestamp() AS now) INSERT INTO affective_appraisals(individual_id,id,goal_id,subject_id,valence,arousal,goal_relevance,expectation_gap,controllability,uncertainty,brief_basis,created_at,expires_at) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,now,now+($12::integer*interval '1 second') FROM clock")
    .bind(individual).bind(id).bind(a.goal_id).bind(a.subject_id).bind(a.valence).bind(a.arousal).bind(a.goal_relevance).bind(a.expectation_gap).bind(a.controllability).bind(a.uncertainty).bind(&a.brief_basis).bind(APPRAISAL_LIFETIME_SECONDS).execute(&mut **tx).await?;
    }
    if let (Some(id), Some(a)) = (attitude_id, long.as_ref()) {
        if let Some((old, before)) = a.expected_previous() {
            let change=sqlx::query("UPDATE affective_attitudes SET status='superseded',version=version+1,superseded_at=clock_timestamp() WHERE individual_id=$1 AND id=$2 AND version=$3 AND status='current'").bind(individual).bind(old).bind(before).execute(&mut **tx).await?;
            if change.rows_affected() != 1 {
                return Err(StoreError::StaleState);
            }
            outbox(tx, individual, "affect.changed", old, version).await?;
        }
        let a = a.draft();
        object(tx, individual, id, "affective_attitude").await?;
        sqlx::query("INSERT INTO affective_attitudes(individual_id,id,topic,subject_id,stance,brief_basis,supersedes) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(individual).bind(id).bind(&a.topic).bind(a.subject_id).bind(&a.stance).bind(&a.brief_basis).bind(a.supersedes).execute(&mut **tx).await?;
    }
    for (id, events) in appraisal_id
        .into_iter()
        .zip(appraisal.into_iter().map(|a| &a.event_ids))
        .chain(
            attitude_id
                .into_iter()
                .zip(attitude.into_iter().map(|a| &a.event_ids)),
        )
    {
        for source in events {
            sqlx::query(
                "INSERT INTO affective_sources(individual_id,object_id,event_id) VALUES($1,$2,$3)",
            )
            .bind(individual)
            .bind(id)
            .bind(source)
            .execute(&mut **tx)
            .await?;
        }
    }
    insert_event(tx,individual,event_id,"affect.updated","inference","individual",json!({"appraisal_id":appraisal_id,"attitude_id":attitude_id,"previous_attitude_id":attitude.and_then(|a|a.supersedes)}),correlation.unwrap_or(event_id),key).await?;
    for source in input_manifest {
        edge(tx, individual, *source, event_id).await?;
        for id in appraisal_id.into_iter().chain(attitude_id) {
            edge(tx, individual, *source, id).await?;
        }
    }
    for id in appraisal_id.into_iter().chain(attitude_id) {
        edge(tx, individual, event_id, id).await?;
        outbox(tx, individual, "affect.changed", id, version).await?;
    }
    Ok(AffectReceipt {
        individual_id: individual,
        appraisal_id,
        attitude_id,
        previous_attitude_id: attitude.and_then(|a| a.supersedes),
        event_id,
        version,
    })
}

pub(super) struct ModelAffectIds {
    pub appraisal_ids: Vec<Uuid>,
    pub attitude_ids: Vec<Uuid>,
}
pub(super) async fn apply_model_changes(
    tx: &mut Tx<'_>,
    c: &crate::domain::cognition::ModelContext,
    appraisals: &[AppraisalDraft],
    attitudes: &[AttitudeDraft],
    decision: Uuid,
    version: i64,
) -> Result<ModelAffectIds, StoreError> {
    let mut result = ModelAffectIds {
        appraisal_ids: vec![],
        attitude_ids: vec![],
    };
    for draft in appraisals {
        draft
            .validate_context(c)
            .map_err(|_| StoreError::Invalid("appraisal context change"))?;
        let (app, long) = validate_update(
            tx,
            c.individual_id,
            &c.input_manifest,
            Some(draft),
            None,
            false,
        )
        .await?;
        let r = insert_update(
            tx,
            c.individual_id,
            &c.input_manifest,
            app.as_ref(),
            long.as_ref(),
            version,
            (Some(c.run_id), None),
        )
        .await?;
        edge(tx, c.individual_id, decision, r.event_id).await?;
        if let Some(id) = r.appraisal_id {
            edge(tx, c.individual_id, decision, id).await?;
            result.appraisal_ids.push(id);
        }
    }
    for draft in attitudes {
        let expected = draft
            .validate_context(c)
            .map_err(|_| StoreError::Invalid("attitude context change"))?;
        let (app, long) = validate_update(
            tx,
            c.individual_id,
            &c.input_manifest,
            None,
            Some(draft),
            false,
        )
        .await?;
        if long.as_ref().map(|a| a.expected_previous()) != Some(expected.expected_previous()) {
            return Err(StoreError::StaleState);
        }
        let r = insert_update(
            tx,
            c.individual_id,
            &c.input_manifest,
            app.as_ref(),
            long.as_ref(),
            version,
            (Some(c.run_id), None),
        )
        .await?;
        edge(tx, c.individual_id, decision, r.event_id).await?;
        if let Some(id) = r.attitude_id {
            edge(tx, c.individual_id, decision, id).await?;
            result.attitude_ids.push(id);
        }
    }
    Ok(result)
}

impl Store {
    pub async fn record_affect(
        &self,
        key: &str,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        appraisal: Option<&AppraisalDraft>,
        attitude: Option<&AttitudeDraft>,
    ) -> Result<AffectReceipt, StoreError> {
        if appraisal.is_none() && attitude.is_none() {
            return Err(StoreError::Invalid("empty affect update"));
        }
        if let Some(a) = appraisal {
            a.validate_shape()
                .map_err(|_| StoreError::Invalid("appraisal fields"))?;
        }
        if let Some(a) = attitude {
            a.validate_shape()
                .map_err(|_| StoreError::Invalid("attitude fields"))?;
        }
        let inputs: HashSet<_> = input_manifest.iter().copied().collect();
        if inputs.is_empty()
            || inputs.len() != input_manifest.len()
            || inputs.contains(&Uuid::nil())
        {
            return Err(StoreError::Invalid("affect manifest"));
        }
        let hash = digest(
            &json!({"expected":expected,"input_manifest":input_manifest,"appraisal":appraisal,"attitude":attitude}),
        )?;
        let mut tx = self.write().await?;
        if let Some(r) = replay::<AffectReceipt>(&mut tx, "individual", ROUTE, key, &hash).await? {
            let individual = existing(&mut tx).await?;
            let stored:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE individual_id=$1 AND id=$2 AND kind='affect.updated' AND source_kind='inference') AND ($3::uuid IS NULL OR EXISTS(SELECT 1 FROM affective_appraisals WHERE individual_id=$1 AND id=$3)) AND ($4::uuid IS NULL OR EXISTS(SELECT 1 FROM affective_attitudes WHERE individual_id=$1 AND id=$4))")
    .bind(individual).bind(r.event_id).bind(r.appraisal_id).bind(r.attitude_id).fetch_one(&mut *tx).await?;
            let mut refs = input_manifest.to_vec();
            refs.extend([individual, r.event_id]);
            refs.extend(r.appraisal_id);
            refs.extend(r.attitude_id);
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
        if current.lifecycle != Lifecycle::Active {
            return Err(StoreError::Inactive);
        }
        let (app, long) = validate_update(
            &mut tx,
            current.id,
            input_manifest,
            appraisal,
            attitude,
            true,
        )
        .await?;
        let version = current
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?;
        let r = insert_update(
            &mut tx,
            current.id,
            input_manifest,
            app.as_ref(),
            long.as_ref(),
            version,
            (None, Some(key)),
        )
        .await?;
        advance(&mut tx, current.id, version).await?;
        save_receipt(&mut tx, "individual", ROUTE, key, &hash, &r).await?;
        tx.commit().await?;
        Ok(r)
    }
    pub async fn affect_snapshot(
        &self,
        at: DateTime<Utc>,
        limit: u32,
    ) -> Result<Value, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("affect limit"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        if !available(&mut tx, individual, &[individual]).await? {
            return Err(StoreError::NotFound);
        }
        let common = format!(
            "{LINKS},closure(root,id) AS (SELECT id,id FROM affective_appraisals WHERE individual_id=$1 UNION SELECT id,id FROM affective_attitudes WHERE individual_id=$1 UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD}"
        );
        let app:Option<(Value,f64,f64,f64)>=sqlx::query_as(&format!("{common} SELECT (to_jsonb(a)-'individual_id')||jsonb_build_object('event_ids',ARRAY(SELECT event_id FROM affective_sources s WHERE s.individual_id=$1 AND s.object_id=a.id ORDER BY event_id)),a.goal_relevance,a.expectation_gap,a.uncertainty FROM affective_appraisals a WHERE a.individual_id=$1 AND a.status='current' AND a.created_at<=$2 AND a.expires_at>$2 AND NOT EXISTS(SELECT 1 FROM bad WHERE root=a.id) ORDER BY a.created_at DESC,a.id DESC LIMIT 1"))
   .bind(individual).bind(at).fetch_optional(&mut *tx).await?;
        let appraisal = app
            .map(|(mut v, g, e, u)| {
                v["attention_weight"] = json!(
                    attention_weight(g, e, u)
                        .map_err(|_| StoreError::Invalid("stored appraisal values"))?
                );
                Ok::<_, StoreError>(v)
            })
            .transpose()?;
        let mut attitudes:Vec<Value>=sqlx::query_scalar(&format!("{common} SELECT (to_jsonb(a)-'individual_id')||jsonb_build_object('event_ids',ARRAY(SELECT event_id FROM affective_sources s WHERE s.individual_id=$1 AND s.object_id=a.id ORDER BY event_id)) FROM affective_attitudes a WHERE a.individual_id=$1 AND a.status='current' AND NOT EXISTS(SELECT 1 FROM bad WHERE root=a.id) ORDER BY a.created_at DESC,a.id DESC LIMIT $2"))
   .bind(individual).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
        let has_more = attitudes.len() > limit as usize;
        attitudes.truncate(limit as usize);
        let (state_version, privacy_epoch, observed_at): (i64, i64, DateTime<Utc>) =
            sqlx::query_as(
                "SELECT state_version,privacy_epoch,clock_timestamp() FROM individuals WHERE id=$1",
            )
            .bind(individual)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,"privacy_epoch":privacy_epoch,"observed_at":observed_at,"appraisal":appraisal,"attitudes":attitudes,"has_more":has_more}),
        )
    }
}
