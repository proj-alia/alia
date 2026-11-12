//! Trusted internal M3 persistence; no model/HTTP command deserialization.
use crate::domain::development::{IndependentAssessment, ValidationRoute};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Serialize)]
pub enum DevelopmentCommand {
    StartTrial {
        route: ValidationRoute,
        source_event_ids: Vec<Uuid>,
    },
    RecordValidation {
        review_event_id: Uuid,
        assessment: IndependentAssessment,
        source_event_ids: Vec<Uuid>,
    },
    Accept,
    Reject {
        reason: String,
    },
    Expire,
    Revert {
        reason: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentReceipt {
    pub individual_id: Uuid,
    pub development_id: Uuid,
    pub event_id: Uuid,
    pub state_version: i64,
    pub development_version: i64,
    pub status: String,
    pub revision_id: Option<Uuid>,
}

use super::{
    api::existing,
    store::{
        Store, StoreError, advance, digest, edge, insert_event, object, outbox, replay,
        save_receipt, state,
    },
};
use crate::domain::{Lifecycle, development::*, history::ExpectedState};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::collections::HashSet;
type Tx<'a> = Transaction<'a, Postgres>;
const ROUTE: &str = "/internal/developments";
fn invalid(_: DevelopmentError) -> StoreError {
    StoreError::Invalid("development policy")
}
fn status(d: &Development) -> &'static str {
    match d.status() {
        DevelopmentStatus::Proposed => "proposed",
        DevelopmentStatus::Trial => "trial",
        DevelopmentStatus::Accepted => "accepted",
        DevelopmentStatus::Rejected => "rejected",
        DevelopmentStatus::Expired => "expired",
        DevelopmentStatus::Reverted => "reverted",
    }
}
fn target_name(t: DevelopmentTarget) -> Result<String, StoreError> {
    Ok(serde_json::from_value(serde_json::to_value(t)?)?)
}
fn scope_parts(s: &Option<StrategyScope>) -> (Option<&'static str>, Option<Uuid>) {
    match s {
        None => (None, None),
        Some(StrategyScope::Goal(id)) => (Some("goal"), Some(*id)),
        Some(StrategyScope::Task(id)) => (Some("task"), Some(*id)),
    }
}
fn refs_valid(v: &[Uuid]) -> bool {
    !v.is_empty()
        && !v.contains(&Uuid::nil())
        && v.iter().copied().collect::<HashSet<_>>().len() == v.len()
}
fn contains_all(
    manifest: &[Uuid],
    required: impl IntoIterator<Item = Uuid>,
) -> Result<(), StoreError> {
    let inputs: HashSet<_> = manifest.iter().copied().collect();
    if required.into_iter().any(|id| !inputs.contains(&id)) {
        Err(StoreError::Invalid(
            "development reference outside manifest",
        ))
    } else {
        Ok(())
    }
}
pub(super) const LINKS: &str = r#"WITH RECURSIVE links(target,source) AS (
 SELECT target_id,source_id FROM derivation_edges WHERE individual_id=$1
 UNION SELECT object_id,source_id FROM development_dependencies WHERE individual_id=$1
 UNION SELECT development_id,event_id FROM development_evidence WHERE individual_id=$1
 UNION SELECT id,created_event_id FROM developments WHERE individual_id=$1
 UNION SELECT id,target_id FROM developments WHERE individual_id=$1
 UNION SELECT id,proposer_id FROM developments WHERE individual_id=$1
 UNION SELECT id,before_head FROM developments WHERE individual_id=$1 AND before_head IS NOT NULL
 UNION SELECT development_id,event_id FROM development_steps WHERE individual_id=$1
 UNION SELECT id,current_revision_id FROM development_targets WHERE individual_id=$1 AND current_revision_id IS NOT NULL
 UNION SELECT id,scope_id FROM development_targets WHERE individual_id=$1 AND scope_id IS NOT NULL
 UNION SELECT id,event_id FROM development_revisions WHERE individual_id=$1
 UNION SELECT id,development_id FROM development_revisions WHERE individual_id=$1
 UNION SELECT id,previous_revision_id FROM development_revisions WHERE individual_id=$1 AND previous_revision_id IS NOT NULL
 UNION SELECT id,registration_event_id FROM development_verifiers WHERE individual_id=$1
 UNION SELECT event_id,verifier_id FROM development_observations WHERE individual_id=$1
 UNION SELECT event_id,trial_event_id FROM development_observations WHERE individual_id=$1 AND trial_event_id IS NOT NULL
 UNION SELECT event_id,verifier_id FROM development_review_records WHERE individual_id=$1
 UNION SELECT event_id,trial_event_id FROM development_review_records WHERE individual_id=$1
 UNION SELECT review_event_id,event_id FROM development_review_sources WHERE individual_id=$1
 UNION SELECT i.object_id,s.event_id FROM interests i JOIN interest_sources s ON s.individual_id=i.individual_id AND s.interest_id=i.id WHERE i.individual_id=$1
 UNION SELECT r.id,i.object_id FROM interest_revisions r JOIN interests i ON i.individual_id=r.individual_id AND i.id=r.interest_id WHERE r.individual_id=$1
 UNION SELECT id,event_id FROM interest_revisions WHERE individual_id=$1
 UNION SELECT id,previous_revision_id FROM interest_revisions WHERE individual_id=$1 AND previous_revision_id IS NOT NULL
 UNION SELECT subject_id,event_id FROM relationship_events WHERE individual_id=$1
 UNION SELECT revision_id,event_id FROM relationship_revision_events WHERE individual_id=$1
 UNION SELECT id,previous_revision_id FROM relationship_revisions WHERE individual_id=$1 AND previous_revision_id IS NOT NULL
 UNION SELECT object_id,event_id FROM affective_sources WHERE individual_id=$1
 UNION SELECT id,supersedes FROM affective_attitudes WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT memory_id,event_id FROM memory_sources WHERE individual_id=$1
 UNION SELECT id,supersedes FROM memories WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT belief_id,event_id FROM belief_evidence WHERE individual_id=$1
 UNION SELECT id,supersedes FROM beliefs WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT autobiography_id,event_id FROM autobiography_events WHERE individual_id=$1
 UNION SELECT autobiography_id,belief_id FROM autobiography_beliefs WHERE individual_id=$1
 UNION SELECT id,supersedes FROM autobiographies WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT id,run_id FROM actions WHERE individual_id=$1
 UNION SELECT id,result_event_id FROM actions WHERE individual_id=$1 AND result_event_id IS NOT NULL
 UNION SELECT result_event_id,id FROM actions WHERE individual_id=$1 AND result_event_id IS NOT NULL
 UNION SELECT id,trigger_event_id FROM runs WHERE individual_id=$1
 UNION SELECT id,unnest(input_manifest) FROM runs WHERE individual_id=$1
 UNION SELECT id,goal_id FROM runs WHERE individual_id=$1 AND goal_id IS NOT NULL
 UNION SELECT id,decision_event_id FROM actions WHERE individual_id=$1
 UNION SELECT id,causation_id FROM events WHERE individual_id=$1 AND causation_id IS NOT NULL
 UNION SELECT id,formed_in_run FROM goals WHERE individual_id=$1 AND formed_in_run IS NOT NULL
 UNION SELECT g.id,CASE WHEN x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN x.value::uuid END FROM goals g CROSS JOIN LATERAL jsonb_array_elements_text(g.origin_ids) x(value) WHERE g.individual_id=$1 AND x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
 UNION SELECT g.id,i.object_id FROM goals g JOIN interests i ON i.individual_id=g.individual_id AND g.origin_ids @> jsonb_build_array(i.id) WHERE g.individual_id=$1
 UNION SELECT goal_id,event_id FROM goal_progress WHERE individual_id=$1
 UNION SELECT goal_id,event_id FROM goal_reviews WHERE individual_id=$1
 UNION SELECT goal_id,run_id FROM goal_cycles WHERE individual_id=$1
 UNION SELECT id,subject_id FROM relationship_revisions WHERE individual_id=$1
 UNION SELECT subject_id,id FROM (SELECT DISTINCT ON(subject_id) subject_id,id FROM relationship_revisions WHERE individual_id=$1 ORDER BY subject_id,after_version DESC,id DESC) latest_relationship
 UNION SELECT object_id,revision_id FROM (SELECT DISTINCT ON(i.object_id) i.object_id,r.id AS revision_id FROM interests i JOIN interest_revisions r ON r.individual_id=i.individual_id AND r.interest_id=i.id WHERE i.individual_id=$1 ORDER BY i.object_id,r.after_version DESC,r.id DESC) latest_interest
 UNION SELECT id,goal_id FROM affective_appraisals WHERE individual_id=$1 AND goal_id IS NOT NULL
 UNION SELECT id,subject_id FROM affective_appraisals WHERE individual_id=$1 AND subject_id IS NOT NULL
 UNION SELECT id,subject_id FROM affective_attitudes WHERE individual_id=$1 AND subject_id IS NOT NULL
 UNION SELECT entity_id,source_event_id FROM person_aliases WHERE individual_id=$1
 UNION SELECT entity_id,assertion_event_id FROM person_aliases WHERE individual_id=$1
 UNION SELECT memory_id,entity_id FROM memory_person_links WHERE individual_id=$1
 UNION SELECT memory_id,source_event_id FROM memory_person_links WHERE individual_id=$1
 UNION SELECT memory_id,assertion_event_id FROM memory_person_links WHERE individual_id=$1
) "#;
pub(super) async fn available(
    tx: &mut Tx<'_>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<bool, StoreError> {
    let links = if let Some(trial) = super::development_measurements::reference_edges_in(tx).await?
    {
        let end = LINKS.rfind(')').expect("static links CTE");
        format!(
            "{} UNION SELECT target,source FROM ({trial}) actual_trial(source,target)) ",
            &LINKS[..end]
        )
    } else {
        LINKS.into()
    };
    Ok(sqlx::query_scalar(&format!("{links},closure(id) AS (SELECT id FROM unnest($2::uuid[]) t(id) UNION SELECT l.source FROM closure c JOIN links l ON l.target=c.id) SELECT NOT EXISTS(SELECT 1 FROM closure c LEFT JOIN objects o ON o.individual_id=$1 AND o.id=c.id WHERE o.id IS NULL OR o.blocked OR EXISTS(SELECT 1 FROM development_targets z WHERE z.individual_id=$1 AND z.id=c.id AND z.provenance_removed) OR EXISTS(SELECT 1 FROM developments z WHERE z.individual_id=$1 AND z.id=c.id AND z.provenance_removed) OR EXISTS(SELECT 1 FROM development_revisions z WHERE z.individual_id=$1 AND z.id=c.id AND z.provenance_removed))")).bind(individual).bind(ids).fetch_one(&mut **tx).await?)
}
async fn dependency(
    tx: &mut Tx<'_>,
    individual: Uuid,
    target: Uuid,
    refs: &[Uuid],
) -> Result<(), StoreError> {
    for source in refs {
        if *source != target {
            sqlx::query("INSERT INTO development_dependencies(individual_id,object_id,source_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(individual).bind(target).bind(source).execute(&mut **tx).await?;
            edge(tx, individual, *source, target).await?;
        }
    }
    Ok(())
}
async fn proposer(tx: &mut Tx<'_>, individual: Uuid, id: Uuid) -> Result<(), StoreError> {
    let ok:bool=sqlx::query_scalar("SELECT $1=$2 OR EXISTS(SELECT 1 FROM relationships WHERE individual_id=$1 AND subject_id=$2 AND subject_key='owner')").bind(individual).bind(id).fetch_one(&mut **tx).await?;
    if id.is_nil() || !ok || !available(tx, individual, &[id]).await? {
        return Err(StoreError::NotFound);
    }
    Ok(())
}
pub(super) async fn verifier(
    tx: &mut Tx<'_>,
    individual: Uuid,
    id: Uuid,
    at: DateTime<Utc>,
) -> Result<String, StoreError> {
    let row:Option<(String,String,String,String,Value,DateTime<Utc>)>=sqlx::query_as("SELECT v.kind,o.kind,e.kind,e.actor_id,e.payload,e.recorded_at FROM development_verifiers v JOIN objects o ON o.individual_id=v.individual_id AND o.id=v.id JOIN events e ON e.individual_id=v.individual_id AND e.id=v.registration_event_id WHERE v.individual_id=$1 AND v.id=$2 AND e.source_kind='direct_observation'").bind(individual).bind(id).fetch_optional(&mut **tx).await?;
    let Some((kind, object_kind, event_kind, actor, payload, recorded)) = row else {
        return Err(StoreError::Invalid("unregistered development verifier"));
    };
    if event_kind != "development.verifier.registered"
        || actor != "system"
        || recorded > at
        || payload != json!({"verifier_id":id,"kind":kind})
        || !available(tx, individual, &[id]).await?
    {
        return Err(StoreError::Invalid("development verifier provenance"));
    }
    if kind == "human" {
        let owner:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM relationships WHERE individual_id=$1 AND subject_id=$2 AND subject_key='owner')").bind(individual).bind(id).fetch_one(&mut **tx).await?;
        if object_kind != "relationship" || !owner {
            return Err(StoreError::Invalid("development human verifier"));
        }
    } else if kind != "measurement" || object_kind != "development_verifier" {
        return Err(StoreError::Invalid("development measurement verifier"));
    }
    Ok(kind)
}
#[derive(Debug, sqlx::FromRow)]
struct Observation {
    verifier_id: Uuid,
    trial_event_id: Option<Uuid>,
    role: String,
    case_id: Option<Uuid>,
    context: Option<String>,
    outcome: Option<String>,
}
async fn observation(
    tx: &mut Tx<'_>,
    individual: Uuid,
    event: Uuid,
    at: DateTime<Utc>,
) -> Result<Option<Observation>, StoreError> {
    let row:Option<Observation>=sqlx::query_as("SELECT verifier_id,trial_event_id,role,case_id,context,outcome FROM development_observations WHERE individual_id=$1 AND event_id=$2").bind(individual).bind(event).fetch_optional(&mut **tx).await?;
    if let Some(o) = &row {
        verifier(tx, individual, o.verifier_id, at).await?;
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE individual_id=$1 AND id=$2 AND source_kind='direct_observation' AND actor_id=$3 AND kind='development.observed' AND recorded_at<=$4 AND payload=$5)").bind(individual).bind(event).bind(format!("development-verifier:{}",o.verifier_id)).bind(at).bind(json!({"verifier_id":o.verifier_id,"trial_event_id":o.trial_event_id,"role":o.role,"case_id":o.case_id,"context":o.context,"outcome":o.outcome})).fetch_one(&mut **tx).await?;
        if !valid {
            return Err(StoreError::Invalid("development observation receipt"));
        }
    }
    Ok(row)
}
async fn sources(
    tx: &mut Tx<'_>,
    individual: Uuid,
    ids: &[Uuid],
    at: DateTime<Utc>,
) -> Result<Vec<ServerSource>, StoreError> {
    if !refs_valid(ids) || !available(tx, individual, ids).await? {
        return Err(StoreError::NotFound);
    }
    let rows:Vec<(Uuid,String,String,DateTime<Utc>)>=sqlx::query_as("SELECT id,kind,source_kind,recorded_at FROM events WHERE individual_id=$1 AND id=ANY($2) ORDER BY id").bind(individual).bind(ids).fetch_all(&mut **tx).await?;
    if rows.len() != ids.len() {
        return Err(StoreError::Invalid("development needs actual events"));
    }
    let mut result = Vec::new();
    for (id, kind, source, recorded_at) in rows {
        if recorded_at > at || source == "fictional_background" || kind == "decision.reviewed" {
            return Err(StoreError::Invalid("development evidence provenance"));
        }
        let class = if let Some(o) = observation(tx, individual, id, at).await? {
            if o.role == "significant_event" {
                SourceClass::SignificantEvent
            } else {
                SourceClass::ObservedOutcome
            }
        } else if kind == "individual.created" {
            SourceClass::InitialSetting
        } else if kind == "affect.updated" {
            SourceClass::TransientAffect
        } else if source == "generated" || source == "inference" {
            SourceClass::ModelSelfReport
        } else {
            let actual:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM actions a JOIN events e ON e.individual_id=a.individual_id AND e.id=a.result_event_id WHERE a.individual_id=$1 AND a.result_event_id=$2 AND a.status IN ('succeeded','failed','unknown') AND a.receipt IS NOT NULL AND a.execution_mode IS DISTINCT FROM 'fixture' AND a.receipt->>'executed'='true' AND e.payload->>'executed'='true' AND e.kind='tool.result' AND e.actor_id='capability' AND e.payload->>'action_id'=a.id::text AND a.receipt->>'result_event_id'=e.id::text)").bind(individual).bind(id).fetch_one(&mut **tx).await?;
            if actual {
                SourceClass::ObservedOutcome
            } else if kind != "tool.result"
                && (source == "reported_claim" || source == "external_document")
            {
                SourceClass::ExternalReport
            } else {
                return Err(StoreError::Invalid("unverified development observation"));
            }
        };
        result.push(ServerSource {
            id,
            individual_id: individual,
            class,
            recorded_at,
        });
    }
    Ok(result)
}
// Private DB DTOs are never evidence tokens and never public request input.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentRow {
    review_id: Uuid,
    reviewer_id: Uuid,
    origin: String,
    reviewed_evidence_ids: Vec<Uuid>,
    reviewed_counterevidence_ids: Vec<Uuid>,
    counter_search_event_ids: Vec<Uuid>,
    counter_review: String,
    observations: Vec<ScenarioRow>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioRow {
    case_id: Uuid,
    context: String,
    source_event_ids: Vec<Uuid>,
    result_event_id: Uuid,
    outcome: String,
}
fn outcome(s: &str) -> Result<Outcome, StoreError> {
    match s {
        "Passed" | "passed" => Ok(Outcome::Passed),
        "Failed" | "failed" => Ok(Outcome::Failed),
        "Unverified" | "unverified" => Ok(Outcome::Unverified),
        _ => Err(StoreError::Invalid("development outcome")),
    }
}
impl AssessmentRow {
    fn checked(self) -> Result<IndependentAssessment, StoreError> {
        Ok(IndependentAssessment {
            review_id: self.review_id,
            reviewer_id: self.reviewer_id,
            origin: match self.origin.as_str() {
                "IndependentHuman" => ReviewOrigin::IndependentHuman,
                "IndependentMeasurement" => ReviewOrigin::IndependentMeasurement,
                _ => return Err(StoreError::Invalid("development review origin")),
            },
            reviewed_evidence_ids: self.reviewed_evidence_ids,
            reviewed_counterevidence_ids: self.reviewed_counterevidence_ids,
            counter_search_event_ids: self.counter_search_event_ids,
            counter_review: outcome(&self.counter_review)?,
            observations: self
                .observations
                .into_iter()
                .map(|o| {
                    Ok(ScenarioObservation {
                        case_id: o.case_id,
                        context: o.context,
                        source_event_ids: o.source_event_ids,
                        result_event_id: o.result_event_id,
                        outcome: outcome(&o.outcome)?,
                    })
                })
                .collect::<Result<_, StoreError>>()?,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum RouteRow {
    CrossScenarioHabit,
    SignificantEvent { event_id: Uuid },
}
impl RouteRow {
    fn checked(self) -> ValidationRoute {
        match self {
            Self::CrossScenarioHabit => ValidationRoute::CrossScenarioHabit,
            Self::SignificantEvent { event_id } => ValidationRoute::SignificantEvent { event_id },
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum CommandRow {
    StartTrial {
        route: RouteRow,
        source_event_ids: Vec<Uuid>,
    },
    RecordValidation {
        review_event_id: Uuid,
        assessment: AssessmentRow,
        source_event_ids: Vec<Uuid>,
    },
    Accept,
    Reject {
        reason: String,
    },
    Expire,
    Revert {
        reason: String,
    },
}
impl CommandRow {
    fn checked(self) -> Result<DevelopmentCommand, StoreError> {
        Ok(match self {
            Self::StartTrial {
                route,
                source_event_ids,
            } => DevelopmentCommand::StartTrial {
                route: route.checked(),
                source_event_ids,
            },
            Self::RecordValidation {
                review_event_id,
                assessment,
                source_event_ids,
            } => DevelopmentCommand::RecordValidation {
                review_event_id,
                assessment: assessment.checked()?,
                source_event_ids,
            },
            Self::Accept => DevelopmentCommand::Accept,
            Self::Reject { reason } => DevelopmentCommand::Reject { reason },
            Self::Expire => DevelopmentCommand::Expire,
            Self::Revert { reason } => DevelopmentCommand::Revert { reason },
        })
    }
}
fn command_kind(c: &DevelopmentCommand) -> &'static str {
    match c {
        DevelopmentCommand::StartTrial { .. } => "development.trial_started",
        DevelopmentCommand::RecordValidation { .. } => "development.validated",
        DevelopmentCommand::Accept => "development.accepted",
        DevelopmentCommand::Reject { .. } => "development.rejected",
        DevelopmentCommand::Expire => "development.expired",
        DevelopmentCommand::Revert { .. } => "development.reverted",
    }
}
#[derive(Debug, sqlx::FromRow)]
struct Head {
    id: Uuid,
    target: String,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    version: i64,
    value: Value,
    current_revision_id: Option<Uuid>,
}
impl Head {
    fn snapshot(&self) -> Result<TargetSnapshot, StoreError> {
        let scope = match (self.scope_kind.as_deref(), self.scope_id) {
            (None, None) => None,
            (Some("goal"), Some(id)) => Some(StrategyScope::Goal(id)),
            (Some("task"), Some(id)) => Some(StrategyScope::Task(id)),
            _ => return Err(StoreError::Invalid("stored development scope")),
        };
        Ok(TargetSnapshot {
            target: serde_json::from_value(json!(self.target))?,
            version: self.version,
            head_revision_id: self.current_revision_id,
            value: self.value.clone(),
            scope,
        })
    }
}
async fn head(tx: &mut Tx<'_>, individual: Uuid, id: Uuid) -> Result<Head, StoreError> {
    sqlx::query_as("SELECT id,target,scope_kind,scope_id,version,value,current_revision_id FROM development_targets WHERE individual_id=$1 AND id=$2 AND NOT provenance_removed").bind(individual).bind(id).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)
}
fn persona_with(
    persona: &Value,
    target: DevelopmentTarget,
    value: &Value,
) -> Result<Value, StoreError> {
    let mut p = persona.clone();
    if let Some(pointer) = target.persona_pointer() {
        *p.pointer_mut(pointer)
            .ok_or(StoreError::Invalid("development persona target"))? = value.clone();
    }
    Ok(p)
}
async fn scope_available(
    tx: &mut Tx<'_>,
    individual: Uuid,
    scope: &Option<StrategyScope>,
) -> Result<(), StoreError> {
    let (kind, id) = scope_parts(scope);
    if let Some(id) = id {
        let ok:bool=sqlx::query_scalar("SELECT CASE WHEN $3='goal' THEN EXISTS(SELECT 1 FROM goals WHERE individual_id=$1 AND id=$2) WHEN $3='task' THEN EXISTS(SELECT 1 FROM runs WHERE individual_id=$1 AND id=$2) ELSE false END").bind(individual).bind(id).bind(kind).fetch_one(&mut **tx).await?;
        if !ok || !available(tx, individual, &[id]).await? {
            return Err(StoreError::NotFound);
        }
    }
    Ok(())
}
async fn trial_event(tx: &mut Tx<'_>, individual: Uuid, id: Uuid) -> Result<Uuid, StoreError> {
    sqlx::query_scalar("SELECT event_id FROM development_steps WHERE individual_id=$1 AND development_id=$2 AND command ? 'StartTrial'").bind(individual).bind(id).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("development trial receipt"))
}
async fn review_proof(
    tx: &mut Tx<'_>,
    individual: Uuid,
    d: &Development,
    review_event: Uuid,
    a: &IndependentAssessment,
    source_ids: &[Uuid],
    at: DateTime<Utc>,
) -> Result<VerifiedReview, StoreError> {
    let trial = trial_event(tx, individual, d.candidate().id()).await?;
    let stored:Option<(Uuid,Uuid,Value)>=sqlx::query_as("SELECT trial_event_id,verifier_id,assessment FROM development_review_records WHERE individual_id=$1 AND event_id=$2 AND development_id=$3").bind(individual).bind(review_event).bind(d.candidate().id()).fetch_optional(&mut **tx).await?;
    let Some((stored_trial, reviewer, assessment)) = stored else {
        return Err(StoreError::Invalid("missing trusted development review"));
    };
    let kind = verifier(tx, individual, reviewer, at).await?;
    if stored_trial != trial
        || reviewer != a.reviewer_id
        || assessment != serde_json::to_value(a)?
        || (kind == "human" && a.origin != ReviewOrigin::IndependentHuman)
        || (kind == "measurement" && a.origin != ReviewOrigin::IndependentMeasurement)
        || !available(tx, individual, &[review_event]).await?
    {
        return Err(StoreError::Invalid("development review binding"));
    }
    let actual:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE individual_id=$1 AND id=$2 AND kind='development.review.recorded' AND source_kind='direct_observation' AND actor_id=$3 AND recorded_at<=$4 AND payload=$5)").bind(individual).bind(review_event).bind(format!("development-verifier:{reviewer}")).bind(at).bind(json!({"development_id":d.candidate().id(),"trial_event_id":trial,"review_id":a.review_id,"verifier_id":reviewer})).fetch_one(&mut **tx).await?;
    let stored_sources:Vec<Uuid>=sqlx::query_scalar("SELECT event_id FROM development_review_sources WHERE individual_id=$1 AND review_event_id=$2 ORDER BY event_id").bind(individual).bind(review_event).fetch_all(&mut **tx).await?;
    if !actual
        || !refs_valid(source_ids)
        || stored_sources.iter().copied().collect::<HashSet<_>>()
            != source_ids.iter().copied().collect()
    {
        return Err(StoreError::Invalid("development review source receipts"));
    }
    for o in &a.observations {
        let actual = observation(tx, individual, o.result_event_id, at)
            .await?
            .ok_or(StoreError::Invalid("missing actual development scenario"))?;
        if actual.trial_event_id != Some(trial)
            || actual.role != "scenario_result"
            || actual.case_id != Some(o.case_id)
            || actual.context.as_deref() != Some(o.context.as_str())
            || outcome(actual.outcome.as_deref().unwrap_or(""))? != o.outcome
        {
            return Err(StoreError::Invalid("development scenario receipt binding"));
        }
    }
    let mut counter = Outcome::Passed;
    for id in &a.counter_search_event_ids {
        let o = observation(tx, individual, *id, at)
            .await?
            .ok_or(StoreError::Invalid("missing actual counter search"))?;
        if o.trial_event_id != Some(trial) || o.role != "counter_search" {
            return Err(StoreError::Invalid("development counter search binding"));
        }
        let result = outcome(o.outcome.as_deref().unwrap_or(""))?;
        if result == Outcome::Failed {
            counter = Outcome::Failed
        } else if result == Outcome::Unverified && counter != Outcome::Failed {
            counter = Outcome::Unverified
        }
    }
    if counter != a.counter_review {
        return Err(StoreError::Invalid("development counter review receipt"));
    }
    TrustedValidator::review(
        d,
        a.clone(),
        sources(tx, individual, source_ids, at).await?,
        at,
    )
    .map_err(invalid)
}

#[derive(sqlx::FromRow)]
struct CandidateRow {
    id: Uuid,
    target_id: Uuid,
    proposer_id: Uuid,
    draft: Value,
    before_value: Value,
    before_head: Option<Uuid>,
    prepared_at: DateTime<Utc>,
    version: i64,
    status: String,
    created_event_id: Uuid,
}
#[derive(sqlx::FromRow)]
struct Step {
    version: i64,
    event_id: Uuid,
    occurred_at: DateTime<Utc>,
    command: Value,
    status: String,
    transition_record: Option<Value>,
    validation_record: Option<Value>,
    revision_id: Option<Uuid>,
}
struct Loaded {
    domain: Development,
    row: CandidateRow,
    head: Head,
}
fn initial_payload(
    id: Uuid,
    target: Uuid,
    proposer: Uuid,
    draft: &DevelopmentDraft,
    before: &TargetSnapshot,
    at: DateTime<Utc>,
) -> Result<Value, StoreError> {
    Ok(
        json!({"development_id":id,"target_id":target,"proposer_id":proposer,"candidate_digest":digest(&json!({"draft":draft,"before_value":before.value,"before_head":before.head_revision_id,"scope":before.scope,"prepared_at":at}))?}),
    )
}
fn step_payload(
    id: Uuid,
    version: i64,
    command: &DevelopmentCommand,
    revision: Option<Uuid>,
) -> Result<Value, StoreError> {
    Ok(
        json!({"development_id":id,"development_version":version,"command_digest":digest(command)?,"revision_id":revision}),
    )
}
async fn check_event(
    tx: &mut Tx<'_>,
    individual: Uuid,
    id: Uuid,
    kind: &str,
    payload: &Value,
) -> Result<(), StoreError> {
    let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE individual_id=$1 AND id=$2 AND kind=$3 AND source_kind='inference' AND actor_id='individual' AND payload=$4)").bind(individual).bind(id).bind(kind).bind(payload).fetch_one(&mut **tx).await?;
    if !ok {
        return Err(StoreError::Invalid("stored development event binding"));
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn apply(
    tx: &mut Tx<'_>,
    individual: Uuid,
    d: &Development,
    cmd: &DevelopmentCommand,
    snapshot: &TargetSnapshot,
    persona: &Value,
    revision: Option<Uuid>,
    at: DateTime<Utc>,
) -> Result<(Development, Option<ApplicationIntent>), StoreError> {
    match cmd {
        DevelopmentCommand::StartTrial {
            route,
            source_event_ids,
        } => {
            let auth = TrustedValidator::authorize_trial(
                d.candidate(),
                sources(tx, individual, source_event_ids, at).await?,
                route.clone(),
                at,
            )
            .map_err(invalid)?;
            Ok((d.start_trial(&auth, at).map_err(invalid)?, None))
        }
        DevelopmentCommand::RecordValidation {
            review_event_id,
            assessment,
            source_event_ids,
        } => {
            let proof = review_proof(
                tx,
                individual,
                d,
                *review_event_id,
                assessment,
                source_event_ids,
                at,
            )
            .await?;
            Ok((d.record_validation(&proof, at).map_err(invalid)?, None))
        }
        DevelopmentCommand::Accept => {
            let (next, intent) = d
                .accept(
                    snapshot,
                    persona,
                    revision.ok_or(StoreError::Invalid("development revision"))?,
                    at,
                )
                .map_err(invalid)?;
            Ok((next, Some(intent)))
        }
        DevelopmentCommand::Reject { reason } => Ok((d.reject(reason, at).map_err(invalid)?, None)),
        DevelopmentCommand::Expire => Ok((d.expire(at).map_err(invalid)?, None)),
        DevelopmentCommand::Revert { reason } => {
            let (next, intent) = d
                .revert(
                    snapshot,
                    persona,
                    revision.ok_or(StoreError::Invalid("development revision"))?,
                    reason,
                    at,
                )
                .map_err(invalid)?;
            Ok((next, Some(intent)))
        }
    }
}
fn records(
    previous: Option<&Development>,
    next: &Development,
) -> Result<(Option<Value>, Option<Value>), StoreError> {
    let history = previous.map_or(0, |d| d.history().len());
    let validations = previous.map_or(0, |d| d.validations().len());
    Ok((
        if next.history().len() > history {
            Some(serde_json::to_value(next.history().last())?)
        } else {
            None
        },
        if next.validations().len() > validations {
            Some(serde_json::to_value(next.validations().last())?)
        } else {
            None
        },
    ))
}
async fn check_revision(
    tx: &mut Tx<'_>,
    individual: Uuid,
    development: Uuid,
    target: Uuid,
    event: Uuid,
    action: &str,
    intent: &ApplicationIntent,
) -> Result<(), StoreError> {
    let row:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'target_id',target_id,'development_id',development_id,'event_id',event_id,'action',action,'before_version',before_version,'after_version',after_version,'before_value',before_value,'after_value',after_value,'previous_revision_id',previous_revision_id) FROM development_revisions WHERE individual_id=$1 AND id=$2 AND NOT provenance_removed").bind(individual).bind(intent.revision_id).fetch_optional(&mut **tx).await?;
    if row
        != Some(
            json!({"id":intent.revision_id,"target_id":target,"development_id":development,"event_id":event,"action":action,"before_version":intent.expected_version,"after_version":intent.next_version,"before_value":intent.before_value,"after_value":intent.after_value,"previous_revision_id":intent.expected_head_revision_id}),
        )
    {
        return Err(StoreError::Invalid("stored development revision binding"));
    }
    Ok(())
}
async fn load(
    tx: &mut Tx<'_>,
    individual: Uuid,
    id: Uuid,
    persona: &Value,
) -> Result<Loaded, StoreError> {
    if !available(tx, individual, &[individual, id]).await? {
        return Err(StoreError::NotFound);
    }
    let row:CandidateRow=sqlx::query_as("SELECT id,target_id,proposer_id,draft,before_value,before_head,prepared_at,version,status,created_event_id FROM developments WHERE individual_id=$1 AND id=$2 AND NOT provenance_removed").bind(individual).bind(id).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    proposer(tx, individual, row.proposer_id).await?;
    let head = head(tx, individual, row.target_id).await?;
    verify_head(tx, individual, &head, persona).await?;
    let snapshot = head.snapshot()?;
    scope_available(tx, individual, &snapshot.scope).await?;
    let draft: DevelopmentDraft = serde_json::from_value(row.draft.clone())?;
    let before = TargetSnapshot {
        version: draft.before_version,
        head_revision_id: row.before_head,
        value: row.before_value.clone(),
        ..snapshot
    };
    let before_persona = persona_with(persona, before.target, &before.value)?;
    let candidate = DevelopmentCandidate::prepare(
        row.id,
        individual,
        row.proposer_id,
        draft.clone(),
        &before,
        &before_persona,
        row.prepared_at,
    )
    .map_err(invalid)?;
    let evidence:Vec<(Uuid,String)>=sqlx::query_as("SELECT event_id,role FROM development_evidence WHERE individual_id=$1 AND development_id=$2 ORDER BY event_id").bind(individual).bind(id).fetch_all(&mut **tx).await?;
    let mut expected_evidence: Vec<_> = draft
        .evidence_ids
        .iter()
        .map(|id| (*id, "evidence".to_string()))
        .chain(
            draft
                .counterevidence_ids
                .iter()
                .map(|id| (*id, "counterevidence".to_string())),
        )
        .collect();
    expected_evidence.sort();
    if evidence != expected_evidence {
        return Err(StoreError::Invalid("stored development evidence binding"));
    }
    let mut initial_sources = draft.evidence_ids.clone();
    initial_sources.extend(&draft.counterevidence_ids);
    TrustedValidator::authorize_trial(
        &candidate,
        sources(tx, individual, &initial_sources, row.prepared_at).await?,
        ValidationRoute::CrossScenarioHabit,
        row.prepared_at,
    )
    .map_err(invalid)?;
    check_event(
        tx,
        individual,
        row.created_event_id,
        "development.proposed",
        &initial_payload(
            id,
            row.target_id,
            row.proposer_id,
            &draft,
            &before,
            row.prepared_at,
        )?,
    )
    .await?;
    let steps:Vec<Step>=sqlx::query_as("SELECT version,event_id,occurred_at,command,status,transition_record,validation_record,revision_id FROM development_steps WHERE individual_id=$1 AND development_id=$2 ORDER BY version").bind(individual).bind(id).fetch_all(&mut **tx).await?;
    if i64::try_from(steps.len()).ok() != Some(row.version) || steps.is_empty() {
        return Err(StoreError::Invalid("stored development step sequence"));
    }
    let mut domain = Development::propose(candidate);
    let mut replay_snapshot = before;
    let mut replay_persona = before_persona;
    for (i, step) in steps.iter().enumerate() {
        if step.version != i as i64 + 1 {
            return Err(StoreError::Invalid("stored development step version"));
        }
        let next = if i == 0 {
            if step.command != json!("Propose")
                || step.event_id != row.created_event_id
                || step.occurred_at != row.prepared_at
                || step.revision_id.is_some()
            {
                return Err(StoreError::Invalid("stored development proposal step"));
            }
            domain.clone()
        } else {
            let cmd = serde_json::from_value::<CommandRow>(step.command.clone())?.checked()?;
            check_event(
                tx,
                individual,
                step.event_id,
                command_kind(&cmd),
                &step_payload(id, step.version, &cmd, step.revision_id)?,
            )
            .await?;
            let (next, intent) = apply(
                tx,
                individual,
                &domain,
                &cmd,
                &replay_snapshot,
                &replay_persona,
                step.revision_id,
                step.occurred_at,
            )
            .await?;
            if let Some(intent) = intent {
                check_revision(
                    tx,
                    individual,
                    id,
                    row.target_id,
                    step.event_id,
                    if matches!(cmd, DevelopmentCommand::Accept) {
                        "accept"
                    } else {
                        "revert"
                    },
                    &intent,
                )
                .await?;
                replay_snapshot = TargetSnapshot {
                    version: intent.next_version,
                    head_revision_id: Some(intent.revision_id),
                    value: intent.after_value.clone(),
                    ..replay_snapshot
                };
                if let ApplicationValue::Persona(value) = intent.application {
                    replay_persona = value
                }
            } else if step.revision_id.is_some() {
                return Err(StoreError::Invalid("unexpected development revision"));
            }
            next
        };
        let (transition, validation) = records(if i == 0 { None } else { Some(&domain) }, &next)?;
        if step.status != status(&next)
            || step.transition_record != transition
            || step.validation_record != validation
        {
            return Err(StoreError::Invalid("stored development history binding"));
        }
        domain = next;
    }
    if status(&domain) != row.status {
        return Err(StoreError::Invalid("stored development status binding"));
    }
    Ok(Loaded { domain, row, head })
}
async fn manifest(tx: &mut Tx<'_>, individual: Uuid, inputs: &[Uuid]) -> Result<(), StoreError> {
    if !refs_valid(inputs) {
        return Err(StoreError::Invalid("development manifest"));
    }
    let mut refs = inputs.to_vec();
    refs.push(individual);
    if !available(tx, individual, &refs).await? {
        return Err(StoreError::NotFound);
    }
    Ok(())
}
/// Reconstruct the real candidate and complete trial history in the caller's
/// transaction. Returned DTOs do not replace later lease/head/source checks.
pub(super) async fn trial_candidate_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    development_id: Uuid,
) -> Result<(Development, i64, TargetSnapshot), StoreError> {
    let persona: Value = sqlx::query_scalar("SELECT persona FROM individuals WHERE id=$1")
        .bind(individual)
        .fetch_one(&mut **tx)
        .await?;
    let loaded = load(tx, individual, development_id, &persona).await?;
    let snapshot = loaded.head.snapshot()?;
    if !available(tx, individual, &[loaded.head.id]).await? {
        return Err(StoreError::NotFound);
    }
    Ok((loaded.domain, loaded.row.version, snapshot))
}
async fn required_command_refs(
    tx: &mut Tx<'_>,
    individual: Uuid,
    cmd: &DevelopmentCommand,
) -> Result<Vec<Uuid>, StoreError> {
    let mut refs = vec![];
    match cmd {
        DevelopmentCommand::StartTrial {
            source_event_ids, ..
        } => refs.extend(source_event_ids),
        DevelopmentCommand::RecordValidation {
            review_event_id,
            assessment,
            source_event_ids,
        } => {
            refs.extend(source_event_ids);
            refs.extend([*review_event_id, assessment.reviewer_id]);
            let registration:Option<Uuid>=sqlx::query_scalar("SELECT registration_event_id FROM development_verifiers WHERE individual_id=$1 AND id=$2").bind(individual).bind(assessment.reviewer_id).fetch_optional(&mut **tx).await?;
            refs.extend(registration);
            refs.extend(&assessment.reviewed_evidence_ids);
            refs.extend(&assessment.reviewed_counterevidence_ids);
            refs.extend(&assessment.counter_search_event_ids);
            for o in &assessment.observations {
                refs.push(o.result_event_id);
                refs.extend(&o.source_event_ids)
            }
        }
        _ => {}
    }
    Ok(refs)
}
#[allow(clippy::too_many_arguments)]
async fn save_step(
    tx: &mut Tx<'_>,
    individual: Uuid,
    d: &Development,
    version: i64,
    event: Uuid,
    cmd: Value,
    at: DateTime<Utc>,
    previous: Option<&Development>,
    revision: Option<Uuid>,
) -> Result<(), StoreError> {
    let (transition, validation) = records(previous, d)?;
    sqlx::query("INSERT INTO development_steps(individual_id,development_id,version,event_id,occurred_at,command,status,transition_record,validation_record,revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(individual).bind(d.candidate().id()).bind(version).bind(event).bind(at).bind(cmd).bind(status(d)).bind(transition).bind(validation).bind(revision).execute(&mut **tx).await?;
    Ok(())
}
pub(super) async fn replay_receipt(
    tx: &mut Tx<'_>,
    inputs: &[Uuid],
    r: &DevelopmentReceipt,
) -> Result<(), StoreError> {
    let individual = existing(tx).await?;
    let mut refs = inputs.to_vec();
    refs.extend([individual, r.development_id, r.event_id]);
    refs.extend(r.revision_id);
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_steps WHERE individual_id=$1 AND development_id=$2 AND version=$3 AND event_id=$4 AND status=$5 AND revision_id IS NOT DISTINCT FROM $6)").bind(individual).bind(r.development_id).bind(r.development_version).bind(r.event_id).bind(&r.status).bind(r.revision_id).fetch_one(&mut **tx).await?;
    if individual != r.individual_id || !valid || !available(tx, individual, &refs).await? {
        return Err(StoreError::NotFound);
    }
    let persona: Value = sqlx::query_scalar("SELECT persona FROM individuals WHERE id=$1")
        .bind(individual)
        .fetch_one(&mut **tx)
        .await?;
    load(tx, individual, r.development_id, &persona).await?;
    Ok(())
}
/// A proposed record written inside the caller's transaction; not an acceptance proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedDevelopment {
    pub development_id: Uuid,
    pub event_id: Uuid,
    pub target_id: Uuid,
    pub development_version: i64,
}

/// Read proposal metadata in the caller's snapshot. Missing heads remain absent;
/// the next baseline version includes removed history without returning its IDs.
pub async fn target_catalog_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    run_id: Uuid,
    goal_id: Option<Uuid>,
) -> Result<Vec<DevelopmentTargetView>, StoreError> {
    let actual: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT goal_id FROM runs WHERE individual_id=$1 AND id=$2")
            .bind(individual)
            .bind(run_id)
            .fetch_optional(&mut **tx)
            .await?;
    if actual != Some(goal_id) {
        return Err(StoreError::StaleState);
    }
    super::privacy_development::verify_development_projection_in(tx, individual).await?;
    immutable_seed(tx, individual).await?;
    let persona: Value = sqlx::query_scalar("SELECT persona FROM individuals WHERE id=$1")
        .bind(individual)
        .fetch_one(&mut **tx)
        .await?;
    let mut targets = vec![
        (DevelopmentTarget::Traits, None),
        (DevelopmentTarget::Values, None),
        (DevelopmentTarget::Expression, None),
        (DevelopmentTarget::CognitivePreferences, None),
        (
            DevelopmentTarget::TaskStrategy,
            Some(StrategyScope::Task(run_id)),
        ),
    ];
    if let Some(id) = goal_id {
        targets.push((
            DevelopmentTarget::TaskStrategy,
            Some(StrategyScope::Goal(id)),
        ));
    }
    let mut views = Vec::with_capacity(targets.len());
    for (target, scope) in targets {
        scope_available(tx, individual, &scope).await?;
        let (kind, id) = scope_parts(&scope);
        let name = target_name(target)?;
        let actual:Option<Head>=sqlx::query_as("SELECT id,target,scope_kind,scope_id,version,value,current_revision_id FROM development_targets WHERE individual_id=$1 AND target=$2 AND scope_kind IS NOT DISTINCT FROM $3 AND scope_id IS NOT DISTINCT FROM $4 AND NOT provenance_removed")
            .bind(individual).bind(&name).bind(kind).bind(id).fetch_optional(&mut **tx).await?;
        let (before_version, target_id, head_revision_id) = if let Some(head) = actual {
            verify_head(tx, individual, &head, &persona).await?;
            (head.version, Some(head.id), head.current_revision_id)
        } else {
            (
                generation_baseline(tx, individual, &name, kind, id, None).await?,
                None,
                None,
            )
        };
        views.push(DevelopmentTargetView {
            target,
            scope,
            before_version,
            target_id,
            head_revision_id,
        });
    }
    Ok(views)
}

/// Trusted in-transaction proposal writer. The caller owns lease/CAS, privacy
/// fences, transaction completion, global version/outbox and top-level receipt.
/// This function acquires the actual individual row through the same transaction.
pub async fn propose_in_tx(
    tx: &mut Tx<'_>,
    individual: Uuid,
    input_manifest: &[Uuid],
    proposer_id: Uuid,
    draft: &DevelopmentDraft,
    event_key: Option<&str>,
) -> Result<ProposedDevelopment, StoreError> {
    let current = state(tx).await?.ok_or(StoreError::NotInitialized)?;
    if current.id != individual {
        return Err(StoreError::NotFound);
    }
    if !matches!(current.lifecycle, Lifecycle::Active | Lifecycle::Paused) {
        return Err(StoreError::Inactive);
    }
    manifest(tx, current.id, input_manifest).await?;
    proposer(tx, current.id, proposer_id).await?;
    let scope = if draft.target == DevelopmentTarget::TaskStrategy {
        Some(serde_json::from_value::<TaskStrategy>(draft.proposed_value.clone())?.scope)
    } else {
        None
    };
    scope_available(tx, current.id, &scope).await?;
    let (kind, scope_id) = scope_parts(&scope);
    let target = target_name(draft.target)?;
    let target_row:Option<Head>=sqlx::query_as("SELECT id,target,scope_kind,scope_id,version,value,current_revision_id FROM development_targets WHERE individual_id=$1 AND target=$2 AND scope_kind IS NOT DISTINCT FROM $3 AND scope_id IS NOT DISTINCT FROM $4 AND NOT provenance_removed").bind(current.id).bind(&target).bind(kind).bind(scope_id).fetch_optional(&mut **tx).await?;
    let is_new = target_row.is_none();
    let head = if let Some(head) = target_row {
        head
    } else {
        let (seed, _) = immutable_seed(tx, current.id).await?;
        Head {
            id: Uuid::new_v4(),
            target: target.clone(),
            scope_kind: kind.map(str::to_string),
            scope_id,
            version: generation_baseline(tx, current.id, &target, kind, scope_id, None).await?,
            value: draft
                .target
                .persona_pointer()
                .and_then(|p| seed.pointer(p))
                .cloned()
                .unwrap_or(Value::Null),
            current_revision_id: None,
        }
    };
    if !is_new {
        verify_head(tx, current.id, &head, &current.persona).await?;
    }
    let snapshot = head.snapshot()?;
    let mut required = draft.evidence_ids.clone();
    required.extend(&draft.counterevidence_ids);
    required.extend(scope_id);
    if proposer_id != current.id {
        required.push(proposer_id)
    }
    if !is_new {
        required.push(head.id)
    }
    required.extend(head.current_revision_id);
    contains_all(input_manifest, required)?;
    if !is_new && !available(tx, current.id, &[head.id]).await? {
        return Err(StoreError::NotFound);
    }
    let at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let id = Uuid::new_v4();
    let candidate = DevelopmentCandidate::prepare(
        id,
        current.id,
        proposer_id,
        draft.clone(),
        &snapshot,
        &current.persona,
        at,
    )
    .map_err(invalid)?;
    let mut events = draft.evidence_ids.clone();
    events.extend(&draft.counterevidence_ids);
    TrustedValidator::authorize_trial(
        &candidate,
        sources(tx, current.id, &events, at).await?,
        ValidationRoute::CrossScenarioHabit,
        at,
    )
    .map_err(invalid)?;
    if is_new {
        object(tx, current.id, head.id, "development_target").await?;
        sqlx::query("INSERT INTO development_targets(individual_id,id,target,scope_kind,scope_id,value,version) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(current.id).bind(head.id).bind(&target).bind(kind).bind(scope_id).bind(&head.value).bind(head.version).execute(&mut **tx).await?;
        let (_, birth) = immutable_seed(tx, current.id).await?;
        let mut origin = vec![birth];
        origin.extend(scope_id);
        dependency(tx, current.id, head.id, &origin).await?;
    }
    let event = Uuid::new_v4();
    insert_event(
        tx,
        current.id,
        event,
        "development.proposed",
        "inference",
        "individual",
        initial_payload(id, head.id, proposer_id, draft, &snapshot, at)?,
        id,
        event_key,
    )
    .await?;
    object(tx, current.id, id, "development").await?;
    sqlx::query("INSERT INTO developments(individual_id,id,target_id,proposer_id,draft,before_value,before_head,prepared_at,status,created_event_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'proposed',$9)").bind(current.id).bind(id).bind(head.id).bind(proposer_id).bind(serde_json::to_value(draft)?).bind(&snapshot.value).bind(snapshot.head_revision_id).bind(at).bind(event).execute(&mut **tx).await?;
    for (evidence, role) in [
        (&draft.evidence_ids, "evidence"),
        (&draft.counterevidence_ids, "counterevidence"),
    ] {
        for source in evidence {
            sqlx::query("INSERT INTO development_evidence(individual_id,development_id,event_id,role) VALUES($1,$2,$3,$4)").bind(current.id).bind(id).bind(source).bind(role).execute(&mut **tx).await?;
        }
    }
    let domain = Development::propose(candidate);
    save_step(
        tx,
        current.id,
        &domain,
        1,
        event,
        json!("Propose"),
        at,
        None,
        None,
    )
    .await?;
    dependency(tx, current.id, id, input_manifest).await?;
    dependency(tx, current.id, event, input_manifest).await?;
    Ok(ProposedDevelopment {
        development_id: id,
        event_id: event,
        target_id: head.id,
        development_version: 1,
    })
}

/// A transition persisted inside the caller's transaction, without global commit metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionedDevelopment {
    pub development_id: Uuid,
    pub event_id: Uuid,
    pub development_version: i64,
    pub status: String,
    pub revision_id: Option<Uuid>,
}

/// Trusted transition kernel shared with the Store wrapper. The caller owns
/// the top-level state CAS, privacy fences, version/outbox, receipt and commit.
pub async fn transition_in_tx(
    tx: &mut Tx<'_>,
    individual: Uuid,
    input_manifest: &[Uuid],
    development_id: Uuid,
    expected_development_version: i64,
    command: &DevelopmentCommand,
    event_key: Option<&str>,
) -> Result<TransitionedDevelopment, StoreError> {
    let current = state(tx).await?.ok_or(StoreError::NotInitialized)?;
    if current.id != individual {
        return Err(StoreError::NotFound);
    }
    if !matches!(current.lifecycle, Lifecycle::Active | Lifecycle::Paused) {
        return Err(StoreError::Inactive);
    }
    manifest(tx, current.id, input_manifest).await?;
    let loaded = load(tx, current.id, development_id, &current.persona).await?;
    if loaded.row.version != expected_development_version {
        return Err(StoreError::StaleState);
    }
    let snapshot = loaded.head.snapshot()?;
    let mut required = vec![development_id, loaded.head.id];
    required.extend(loaded.domain.candidate().draft().evidence_ids.iter());
    required.extend(loaded.domain.candidate().draft().counterevidence_ids.iter());
    required.extend(loaded.row.before_head);
    required.extend(snapshot.head_revision_id);
    required.extend(scope_parts(&snapshot.scope).1);
    if loaded.row.proposer_id != current.id {
        required.push(loaded.row.proposer_id)
    }
    required.extend(required_command_refs(tx, current.id, command).await?);
    contains_all(input_manifest, required)?;
    if !available(tx, current.id, &[loaded.head.id]).await? {
        return Err(StoreError::NotFound);
    }
    let at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let revision = matches!(
        command,
        DevelopmentCommand::Accept | DevelopmentCommand::Revert { .. }
    )
    .then(Uuid::new_v4);
    let (next, intent) = apply(
        tx,
        current.id,
        &loaded.domain,
        command,
        &snapshot,
        &current.persona,
        revision,
        at,
    )
    .await?;
    let event = Uuid::new_v4();
    let development_version = loaded
        .row
        .version
        .checked_add(1)
        .ok_or(StoreError::Invalid("development version exhausted"))?;
    insert_event(
        tx,
        current.id,
        event,
        command_kind(command),
        "inference",
        "individual",
        step_payload(development_id, development_version, command, revision)?,
        development_id,
        event_key,
    )
    .await?;
    if let Some(intent) = &intent {
        object(tx, current.id, intent.revision_id, "development_revision").await?;
        sqlx::query("INSERT INTO development_revisions(individual_id,id,target_id,development_id,event_id,action,before_version,after_version,before_value,after_value,previous_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(current.id).bind(intent.revision_id).bind(loaded.head.id).bind(development_id).bind(event).bind(if matches!(command,DevelopmentCommand::Accept){"accept"}else{"revert"}).bind(intent.expected_version).bind(intent.next_version).bind(&intent.before_value).bind(&intent.after_value).bind(intent.expected_head_revision_id).execute(&mut **tx).await?;
        let updated=sqlx::query("UPDATE development_targets SET version=$3,value=$4,current_revision_id=$5 WHERE individual_id=$1 AND id=$2 AND NOT provenance_removed AND version=$6 AND current_revision_id IS NOT DISTINCT FROM $7 AND value=$8").bind(current.id).bind(loaded.head.id).bind(intent.next_version).bind(&intent.after_value).bind(intent.revision_id).bind(intent.expected_version).bind(intent.expected_head_revision_id).bind(&intent.before_value).execute(&mut **tx).await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::StaleState);
        }
        if let ApplicationValue::Persona(value) = &intent.application {
            sqlx::query("UPDATE individuals SET persona=$2 WHERE id=$1")
                .bind(current.id)
                .bind(value)
                .execute(&mut **tx)
                .await?;
        }
        dependency(tx, current.id, intent.revision_id, input_manifest).await?;
        dependency(tx, current.id, intent.revision_id, &[event]).await?;
        dependency(tx, current.id, loaded.head.id, &[intent.revision_id]).await?;
    }
    let updated=sqlx::query("UPDATE developments SET version=$3,status=$4 WHERE individual_id=$1 AND id=$2 AND NOT provenance_removed AND version=$5").bind(current.id).bind(development_id).bind(development_version).bind(status(&next)).bind(loaded.row.version).execute(&mut **tx).await?;
    if updated.rows_affected() != 1 {
        return Err(StoreError::StaleState);
    }
    save_step(
        tx,
        current.id,
        &next,
        development_version,
        event,
        serde_json::to_value(command)?,
        at,
        Some(&loaded.domain),
        revision,
    )
    .await?;
    dependency(tx, current.id, event, input_manifest).await?;
    Ok(TransitionedDevelopment {
        development_id,
        event_id: event,
        development_version,
        status: status(&next).into(),
        revision_id: revision,
    })
}

impl Store {
    pub async fn propose_development(
        &self,
        key: &str,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        proposer_id: Uuid,
        draft: &DevelopmentDraft,
    ) -> Result<DevelopmentReceipt, StoreError> {
        let hash = digest(
            &json!({"expected":expected,"input_manifest":input_manifest,"proposer_id":proposer_id,"draft":draft}),
        )?;
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        if let Some(r) =
            replay::<DevelopmentReceipt>(&mut tx, "individual", ROUTE, key, &hash).await?
        {
            replay_receipt(&mut tx, input_manifest, &r).await?;
            self.require_privacy_snapshot(generation)?;
            return Ok(r);
        }
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if current.expected().version != expected.version
            || current.privacy_epoch != expected.privacy_epoch
        {
            return Err(StoreError::StaleState);
        }
        if !matches!(current.lifecycle, Lifecycle::Active | Lifecycle::Paused) {
            return Err(StoreError::Inactive);
        }
        let proposed = propose_in_tx(
            &mut tx,
            current.id,
            input_manifest,
            proposer_id,
            draft,
            Some(key),
        )
        .await?;
        let id = proposed.development_id;
        let event = proposed.event_id;
        let version = current
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?;
        outbox(&mut tx, current.id, "development.changed", id, version).await?;
        advance(&mut tx, current.id, version).await?;
        let r = DevelopmentReceipt {
            individual_id: current.id,
            development_id: id,
            event_id: event,
            state_version: version,
            development_version: 1,
            status: "proposed".into(),
            revision_id: None,
        };
        save_receipt(&mut tx, "individual", ROUTE, key, &hash, &r).await?;
        self.require_privacy_snapshot(generation)?;
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(r)
    }
    pub async fn transition_development(
        &self,
        key: &str,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        development_id: Uuid,
        expected_development_version: i64,
        command: &DevelopmentCommand,
    ) -> Result<DevelopmentReceipt, StoreError> {
        let hash = digest(
            &json!({"expected":expected,"input_manifest":input_manifest,"development_id":development_id,"expected_development_version":expected_development_version,"command":command}),
        )?;
        self.transition_development_with_intent(
            expected,
            input_manifest,
            development_id,
            expected_development_version,
            command,
            TransitionIntent {
                key,
                actor: "individual",
                route: ROUTE,
                hash,
            },
        )
        .await
    }
    async fn transition_development_with_intent(
        &self,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        development_id: Uuid,
        expected_development_version: i64,
        command: &DevelopmentCommand,
        intent: TransitionIntent<'_>,
    ) -> Result<DevelopmentReceipt, StoreError> {
        let TransitionIntent {
            key,
            actor,
            route,
            hash,
        } = intent;
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        if let Some(r) = replay::<DevelopmentReceipt>(&mut tx, actor, route, key, &hash).await? {
            replay_receipt(&mut tx, input_manifest, &r).await?;
            self.require_privacy_snapshot(generation)?;
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
        let transitioned = transition_in_tx(
            &mut tx,
            current.id,
            input_manifest,
            development_id,
            expected_development_version,
            command,
            Some(key),
        )
        .await?;
        let event = transitioned.event_id;
        let development_version = transitioned.development_version;
        let revision = transitioned.revision_id;
        let version = current
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?;
        outbox(
            &mut tx,
            current.id,
            "development.changed",
            development_id,
            version,
        )
        .await?;
        advance(&mut tx, current.id, version).await?;
        let r = DevelopmentReceipt {
            individual_id: current.id,
            development_id,
            event_id: event,
            state_version: version,
            development_version,
            status: transitioned.status,
            revision_id: revision,
        };
        save_receipt(&mut tx, actor, route, key, &hash, &r).await?;
        self.require_privacy_snapshot(generation)?;
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(r)
    }
}

fn projection(loaded: &Loaded, include_history: bool) -> Result<Value, StoreError> {
    let c = loaded.domain.candidate();
    let d = c.draft();
    let snapshot = loaded.head.snapshot()?;
    let mut value = json!({"id":c.id(),"version":loaded.row.version,"status":status(&loaded.domain),"target_id":loaded.head.id,"target":d.target,"scope":snapshot.scope,"proposer_id":loaded.row.proposer_id,"prepared_at":loaded.row.prepared_at,"before_version":d.before_version,"before_head":loaded.row.before_head,"before_value":loaded.row.before_value,"proposed_value":d.proposed_value,"proposed_description":d.proposed_description,"evidence_ids":d.evidence_ids,"counterevidence_ids":d.counterevidence_ids,"validation_plan":d.validation_plan,"deadline":d.deadline,"current_target_version":snapshot.version,"current_target_head":snapshot.head_revision_id});
    if include_history {
        value["transitions"] = serde_json::to_value(loaded.domain.history())?;
        value["validations"] = serde_json::to_value(loaded.domain.validations())?;
    }
    Ok(value)
}
impl Store {
    pub async fn developments(
        &self,
        include_history: bool,
        limit: u32,
    ) -> Result<Value, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("development limit"));
        }
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        if !available(&mut tx, individual, &[individual]).await? {
            return Err(StoreError::NotFound);
        }
        let (state_version,privacy_epoch,observed_at,persona):(i64,i64,DateTime<Utc>,Value)=sqlx::query_as("SELECT state_version,privacy_epoch,clock_timestamp(),persona FROM individuals WHERE id=$1").bind(individual).fetch_one(&mut *tx).await?;
        let ids:Vec<Uuid>=sqlx::query_scalar(&format!("{LINKS},closure(root,id) AS (SELECT id,id FROM developments WHERE individual_id=$1 UNION SELECT id,target_id FROM developments WHERE individual_id=$1 UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id),bad(root) AS (SELECT c.root FROM closure c LEFT JOIN objects o ON o.individual_id=$1 AND o.id=c.id WHERE o.id IS NULL OR o.blocked OR EXISTS(SELECT 1 FROM development_targets z WHERE z.individual_id=$1 AND z.id=c.id AND z.provenance_removed) OR EXISTS(SELECT 1 FROM developments z WHERE z.individual_id=$1 AND z.id=c.id AND z.provenance_removed) OR EXISTS(SELECT 1 FROM development_revisions z WHERE z.individual_id=$1 AND z.id=c.id AND z.provenance_removed)) SELECT d.id FROM developments d JOIN development_targets t ON t.individual_id=d.individual_id AND t.id=d.target_id WHERE d.individual_id=$1 AND NOT d.provenance_removed AND NOT t.provenance_removed AND ($2 OR d.status IN ('proposed','trial') OR (d.status='accepted' AND EXISTS(SELECT 1 FROM development_revisions r WHERE r.individual_id=$1 AND r.development_id=d.id AND r.id=t.current_revision_id AND r.action='accept' AND NOT r.provenance_removed))) AND NOT EXISTS(SELECT 1 FROM bad WHERE root=d.id) ORDER BY d.prepared_at DESC,d.id DESC LIMIT $3")).bind(individual).bind(include_history).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
        let has_more = ids.len() > limit as usize;
        let mut items = Vec::new();
        for id in ids.into_iter().take(limit as usize) {
            let loaded = load(&mut tx, individual, id, &persona).await?;
            items.push(projection(&loaded, include_history)?)
        }
        self.require_privacy_snapshot(generation)?;
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,"privacy_epoch":privacy_epoch,"observed_at":observed_at,"include_history":include_history,"items":items,"has_more":has_more}),
        )
    }
    pub async fn development(&self, id: Uuid) -> Result<Value, StoreError> {
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        let (state_version,privacy_epoch,observed_at,persona):(i64,i64,DateTime<Utc>,Value)=sqlx::query_as("SELECT state_version,privacy_epoch,clock_timestamp(),persona FROM individuals WHERE id=$1").bind(individual).fetch_one(&mut *tx).await?;
        let loaded = load(&mut tx, individual, id, &persona).await?;
        if !available(&mut tx, individual, &[loaded.head.id]).await? {
            return Err(StoreError::NotFound);
        }
        let item = projection(&loaded, true)?;
        self.require_privacy_snapshot(generation)?;
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,"privacy_epoch":privacy_epoch,"observed_at":observed_at,"development":item}),
        )
    }
}

async fn verify_head(
    tx: &mut Tx<'_>,
    individual: Uuid,
    head: &Head,
    persona: &Value,
) -> Result<(), StoreError> {
    let snapshot = head.snapshot()?;
    if !available(tx, individual, &[head.id]).await? {
        return Err(StoreError::NotFound);
    }
    if let Some(pointer) = snapshot.target.persona_pointer()
        && persona.pointer(pointer) != Some(&snapshot.value)
    {
        return Err(StoreError::Invalid(
            "development persona projection mismatch",
        ));
    }
    if let Some(revision) = snapshot.head_revision_id {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_revisions WHERE individual_id=$1 AND id=$2 AND target_id=$3 AND after_version=$4 AND after_value=$5 AND NOT provenance_removed)").bind(individual).bind(revision).bind(head.id).bind(snapshot.version).bind(&snapshot.value).fetch_one(&mut **tx).await?;
        if !valid {
            return Err(StoreError::Invalid("development head revision mismatch"));
        }
    } else {
        let (seed, _) = immutable_seed(tx, individual).await?;
        let baseline = snapshot
            .target
            .persona_pointer()
            .and_then(|p| seed.pointer(p))
            .cloned()
            .unwrap_or(Value::Null);
        let (kind, scope) = scope_parts(&snapshot.scope);
        let expected =
            generation_baseline(tx, individual, &head.target, kind, scope, Some(head.id)).await?;
        if snapshot.version != expected || snapshot.value != baseline {
            return Err(StoreError::Invalid("development initial head provenance"));
        }
    }
    scope_available(tx, individual, &snapshot.scope).await?;
    Ok(())
}

const OWNER_REVERT_ROUTE: &str = "/owner/developments/revert";
const OWNER_REVERT_MAX_PROVENANCE: usize = 4096;
struct TransitionIntent<'a> {
    key: &'a str,
    actor: &'static str,
    route: &'static str,
    hash: String,
}

impl Store {
    /// Owner supplies only the immutable request intent, never a trusted manifest.
    /// The RR planning transaction is closed before the ordinary CAS writer starts.
    pub async fn owner_revert_development(
        &self,
        key: &str,
        id: Uuid,
        expected_development_version: i64,
        reason: &str,
    ) -> Result<DevelopmentReceipt, StoreError> {
        let hash = digest(
            &json!({"id":id,"expected_development_version":expected_development_version,"reason":reason}),
        )?;
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        if let Some(r) =
            replay::<DevelopmentReceipt>(&mut tx, "owner", OWNER_REVERT_ROUTE, key, &hash).await?
        {
            if r.development_id != id
                || r.status != "reverted"
                || r.revision_id.is_none()
                || expected_development_version.checked_add(1) != Some(r.development_version)
            {
                return Err(StoreError::Invalid("owner development receipt binding"));
            }
            replay_receipt(&mut tx, &[], &r).await?;
            self.require_privacy_snapshot(generation)?;
            tx.commit().await?;
            self.require_privacy_snapshot(generation)?;
            return Ok(r);
        }
        if id.is_nil()
            || expected_development_version <= 0
            || reason.trim().is_empty()
            || reason.contains('\0')
            || reason.chars().count() > 2000
        {
            return Err(StoreError::Invalid("owner development revert fields"));
        }
        let individual = existing(&mut tx).await?;
        let (state_version, privacy_epoch, persona, lifecycle): (i64, i64, Value, String) =
            sqlx::query_as(
                "SELECT state_version,privacy_epoch,persona,lifecycle FROM individuals WHERE id=$1",
            )
            .bind(individual)
            .fetch_one(&mut *tx)
            .await?;
        if !matches!(lifecycle.as_str(), "active" | "paused") {
            return Err(StoreError::Inactive);
        }
        let (version, target): (i64, Uuid) = sqlx::query_as(
            "SELECT version,target_id FROM developments WHERE individual_id=$1 AND id=$2 AND NOT provenance_removed",
        )
        .bind(individual)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        if version != expected_development_version {
            return Err(StoreError::StaleState);
        }
        // Never truncate provenance into a smaller, apparently valid manifest.
        let inputs: Vec<Uuid> = sqlx::query_scalar(&format!("{LINKS},closure(id) AS (SELECT id FROM unnest($2::uuid[]) t(id) UNION SELECT l.source FROM closure c JOIN links l ON l.target=c.id) SELECT id FROM closure ORDER BY id LIMIT $3"))
            .bind(individual).bind(vec![individual,id,target])
            .bind(OWNER_REVERT_MAX_PROVENANCE as i64+1).fetch_all(&mut *tx).await?;
        if inputs.len() > OWNER_REVERT_MAX_PROVENANCE {
            return Err(StoreError::Invalid(
                "owner development provenance exceeds limit",
            ));
        }
        manifest(&mut tx, individual, &inputs).await?;
        let loaded = load(&mut tx, individual, id, &persona).await?;
        let snapshot = loaded.head.snapshot()?;
        let at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        loaded
            .domain
            .revert(&snapshot, &persona, Uuid::new_v4(), reason, at)
            .map_err(|e| match e {
                DevelopmentError::Conflict | DevelopmentError::Transition => StoreError::StaleState,
                _ => invalid(e),
            })?;
        let expected = ExpectedState {
            version: state_version,
            privacy_epoch,
        };
        self.require_privacy_snapshot(generation)?;
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        // The receipt and mutation share the writer's transaction. The receipt hash
        // binds only the owner request, so a lost response can replay after CAS moves.
        self.transition_development_with_intent(
            expected,
            &inputs,
            id,
            expected_development_version,
            &DevelopmentCommand::Revert {
                reason: reason.to_owned(),
            },
            TransitionIntent {
                key,
                actor: "owner",
                route: OWNER_REVERT_ROUTE,
                hash,
            },
        )
        .await
    }
}

/// Only opaque historical counters are read; none of the old IDs become sources.
async fn generation_baseline(
    tx: &mut Tx<'_>,
    individual: Uuid,
    target: &str,
    scope_kind: Option<&str>,
    scope_id: Option<Uuid>,
    excluded: Option<Uuid>,
) -> Result<i64, StoreError> {
    let (maximum,all_removed):(i64,bool)=sqlx::query_as("WITH historical AS (SELECT id,version,provenance_removed FROM development_targets WHERE individual_id=$1 AND target=$2 AND scope_kind IS NOT DISTINCT FROM $3 AND scope_id IS NOT DISTINCT FROM $4 AND id IS DISTINCT FROM $5),versions AS (SELECT version FROM historical UNION ALL SELECT r.after_version FROM development_revisions r JOIN historical h ON h.id=r.target_id WHERE r.individual_id=$1) SELECT coalesce((SELECT max(version) FROM versions),0),coalesce((SELECT bool_and(provenance_removed) FROM historical),true)").bind(individual).bind(target).bind(scope_kind).bind(scope_id).bind(excluded).fetch_one(&mut **tx).await?;
    if !all_removed {
        return Err(StoreError::Invalid("development live baseline collision"));
    }
    maximum
        .checked_add(1)
        .ok_or(StoreError::Invalid("development version exhausted"))
}
pub(super) async fn immutable_seed(
    tx: &mut Tx<'_>,
    individual: Uuid,
) -> Result<(Value, Uuid), StoreError> {
    let (seed, seed_digest): (Value, String) =
        sqlx::query_as("SELECT seed,seed_digest FROM individuals WHERE id=$1")
            .bind(individual)
            .fetch_one(&mut **tx)
            .await?;
    crate::domain::config::PersonaSeed::validate(seed.clone())
        .map_err(|_| StoreError::Invalid("development immutable seed"))?;
    if digest(&seed)? != seed_digest {
        return Err(StoreError::Invalid("development seed digest"));
    }
    let birth:Option<Uuid>=sqlx::query_scalar("SELECT id FROM events WHERE individual_id=$1 AND kind='individual.created' ORDER BY sequence LIMIT 1").bind(individual).fetch_optional(&mut **tx).await?;
    let birth = birth.ok_or(StoreError::NotFound)?;
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE individual_id=$1 AND id=$2 AND source_kind='direct_observation' AND actor_id='system' AND payload->>'seed_digest'=$3)").bind(individual).bind(birth).bind(seed_digest).fetch_one(&mut **tx).await?;
    if !valid || !available(tx, individual, &[individual, birth]).await? {
        return Err(StoreError::NotFound);
    }
    Ok((seed, birth))
}
