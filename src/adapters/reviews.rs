//! Version-bound, evidence-linked opinions. A reviewer cannot execute actions or
//! finish a goal; completion rechecks the same artifacts in its own transaction.
use super::{
    completion::{VerifiedCompletion, citation_sources},
    goals::unavailable,
    scheduling::require_lease,
    store::{Store, StoreError, advance, digest, edge, insert_event, state},
};
use crate::{
    domain::{
        cognition::{CapabilityInfo, ModelContext, RunMode},
        completion::Citation,
        review::{
            GoalAssessment, GoalReviewInput, ReviewArtifact, ReviewObservation, ReviewSource,
            ReviewVerdict,
        },
    },
    runtime::{
        review::{PreparedReview, ReviewReceipt},
        scheduling::{RunLease, TokenAllowance},
    },
};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
const CHECKER: &str = "evidence_review/2";
type Note = (Uuid, Uuid, Uuid, String, String, Value);

async fn snapshot(
    tx: &mut Tx<'_>,
    individual: Uuid,
    goal: Uuid,
) -> Result<Option<GoalReviewInput>, StoreError> {
    if unavailable(tx, individual, &[individual, goal]).await? {
        return Ok(None);
    }
    let row:Option<(i64,String,String,String)>=sqlx::query_as("SELECT review_revision,description,success_criterion,stop_condition FROM goals WHERE individual_id=$1 AND id=$2 AND status='active' AND completion IS NULL")
        .bind(individual).bind(goal).fetch_optional(&mut **tx).await?;
    let Some((goal_revision, description, success_criterion, stop_condition)) = row else {
        return Ok(None);
    };
    // Do not silently sample away a contradictory artifact. Large tasks need an
    // explicit consolidation mechanism before they can be reviewed in M1.
    let notes:Vec<Note>=sqlx::query_as("SELECT a.id,a.run_id,a.result_event_id,n.title,n.body,n.citations FROM notes n JOIN actions a ON (a.individual_id,a.id)=(n.individual_id,n.action_id) JOIN runs r ON (r.individual_id,r.id)=(a.individual_id,a.run_id) JOIN individuals i ON i.id=r.individual_id WHERE r.individual_id=$1 AND (r.goal_id=$2 OR r.id=(SELECT formed_in_run FROM goals WHERE individual_id=$1 AND id=$2)) AND a.status='succeeded' AND r.privacy_epoch=i.privacy_epoch ORDER BY n.created_at,a.id LIMIT 9")
        .bind(individual).bind(goal).fetch_all(&mut **tx).await?;
    if notes.is_empty() || notes.len() > 8 {
        return Ok(None);
    }
    let mut input = GoalReviewInput {
        goal_id: goal,
        goal_revision,
        description,
        success_criterion,
        stop_condition,
        artifacts: vec![],
        sources: vec![],
        observations: vec![],
        evidence_ids: vec![goal],
    };
    for (action, run, result, title, body, raw) in notes {
        if unavailable(tx, individual, &[action, result]).await? {
            return Ok(None);
        }
        let citations: Vec<Citation> = serde_json::from_value(raw)?;
        for citation in &citations {
            let Some(ids) = citation_sources(tx, individual, run, action, citation, &body).await?
            else {
                return Ok(None);
            };
            input.evidence_ids.extend(ids.iter().copied());
            if !input.sources.iter().any(|s| s.citation == *citation) {
                let text: String = sqlx::query_scalar(
                    "SELECT body FROM documents WHERE individual_id=$1 AND event_id=$2",
                )
                .bind(individual)
                .bind(citation.document_id)
                .fetch_one(&mut **tx)
                .await?;
                let Some(text) = citation.excerpt(&text) else {
                    return Ok(None);
                };
                input.sources.push(ReviewSource {
                    citation: citation.clone(),
                    text,
                    read_event_ids: ids
                        .into_iter()
                        .filter(|id| *id != citation.document_id)
                        .collect(),
                });
            }
        }
        input.evidence_ids.extend([action, result]);
        input.artifacts.push(ReviewArtifact {
            action_id: action,
            result_event_id: result,
            title,
            body,
            citations,
        });
    }
    if input.sources.is_empty() {
        return Ok(None);
    }
    // Include all actual reading observations for this goal, including passages
    // the author did not select as citations. No silent truncation of counterevidence.
    let reads:Vec<(Uuid,Value)>=sqlx::query_as("SELECT e.id,e.payload FROM actions a JOIN runs r ON (r.individual_id,r.id)=(a.individual_id,a.run_id) JOIN individuals i ON i.id=r.individual_id JOIN events e ON (e.individual_id,e.id)=(a.individual_id,a.result_event_id) WHERE r.individual_id=$1 AND (r.goal_id=$2 OR r.id=(SELECT formed_in_run FROM goals WHERE individual_id=$1 AND id=$2)) AND r.privacy_epoch=i.privacy_epoch AND a.status='succeeded' AND a.capability_id IN ('documents.read','web.read_allowed') ORDER BY e.sequence LIMIT 17")
        .bind(individual).bind(goal).fetch_all(&mut **tx).await?;
    if reads.len() > 16 {
        return Ok(None);
    }
    for (event_id, payload) in reads {
        let document_id = payload["output"]["document_id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or(StoreError::Invalid("read document id"))?;
        if unavailable(tx, individual, &[event_id, document_id]).await? {
            return Ok(None);
        }
        let Some(text) = payload["output"]["text"].as_str() else {
            return Ok(None);
        };
        let start_char = payload["output"]["offset"].as_u64().unwrap_or(0) as usize;
        let body: Option<String> =
            sqlx::query_scalar("SELECT body FROM documents WHERE individual_id=$1 AND event_id=$2")
                .bind(individual)
                .bind(document_id)
                .fetch_optional(&mut **tx)
                .await?;
        let Some(body) = body else { return Ok(None) };
        if text.is_empty()
            || body
                .chars()
                .skip(start_char)
                .take(text.chars().count())
                .ne(text.chars())
        {
            return Ok(None);
        }
        input.observations.push(ReviewObservation {
            event_id,
            document_id,
            start_char,
            text: text.into(),
        });
        input.evidence_ids.extend([event_id, document_id]);
    }
    input.evidence_ids.sort();
    input.evidence_ids.dedup();
    if input.evidence_ids.len() > 128 || serde_json::to_vec(&input)?.len() > 48 * 1024 {
        return Ok(None);
    }
    Ok(Some(input))
}
async fn saved(
    tx: &mut Tx<'_>,
    individual: Uuid,
    input: &GoalReviewInput,
) -> Result<Option<(Uuid, GoalAssessment)>, StoreError> {
    let row:Option<(Uuid,Value)>=sqlx::query_as("SELECT r.event_id,r.assessment FROM goal_reviews r JOIN individuals i ON i.id=r.individual_id JOIN events e ON (e.individual_id,e.id)=(r.individual_id,r.event_id) WHERE r.individual_id=$1 AND r.goal_id=$2 AND r.goal_revision=$3 AND r.privacy_epoch=i.privacy_epoch AND r.input_digest=$4 AND e.payload->>'checker_version'=$5")
        .bind(individual).bind(input.goal_id).bind(input.goal_revision).bind(digest(input)?).bind(CHECKER).fetch_optional(&mut **tx).await?;
    let Some((event, raw)) = row else {
        return Ok(None);
    };
    if unavailable(tx, individual, &[event]).await? {
        return Ok(None);
    }
    let assessment: GoalAssessment = serde_json::from_value(raw)?;
    assessment
        .validate(input)
        .map_err(|_| StoreError::Invalid("stored goal assessment"))?;
    Ok(Some((event, assessment)))
}
/// Only a review of the exact current snapshot is actionable feedback.
pub(super) async fn feedback(
    tx: &mut Tx<'_>,
    individual: Uuid,
    goal: Uuid,
) -> Result<Option<Uuid>, StoreError> {
    let Some(input) = snapshot(tx, individual, goal).await? else {
        return Ok(None);
    };
    Ok(saved(tx, individual, &input).await?.map(|(id, _)| id))
}
pub(super) async fn verify(
    tx: &mut Tx<'_>,
    individual: Uuid,
    goal: Uuid,
) -> Result<Option<VerifiedCompletion>, StoreError> {
    let Some(input) = snapshot(tx, individual, goal).await? else {
        return Ok(None);
    };
    let Some((event, assessment)) = saved(tx, individual, &input).await? else {
        return Ok(None);
    };
    if assessment.verdict != ReviewVerdict::Met {
        return Ok(None);
    }
    let mut evidence = input.evidence_ids.clone();
    evidence.push(event);
    Ok(Some(VerifiedCompletion {
        checker_version: CHECKER,
        contract_digest: digest(&input)?,
        contract: None,
        note_action_id: input.artifacts[0].action_id,
        result_event_id: event,
        evidence_ids: evidence,
    }))
}
pub(super) async fn validate_context(
    tx: &mut Tx<'_>,
    context: &ModelContext,
) -> Result<(), StoreError> {
    let Some(input) = &context.goal_review else {
        return Ok(());
    };
    let goal: Option<Uuid> =
        sqlx::query_scalar("SELECT goal_id FROM runs WHERE id=$1 AND individual_id=$2")
            .bind(context.run_id)
            .bind(context.individual_id)
            .fetch_one(&mut **tx)
            .await?;
    if context.mode != RunMode::Background
        || goal != Some(input.goal_id)
        || input
            .evidence_ids
            .iter()
            .any(|id| !context.input_manifest.contains(id))
        || unavailable(tx, context.individual_id, &context.input_manifest).await?
    {
        return Err(StoreError::StaleState);
    }
    let actual = snapshot(tx, context.individual_id, input.goal_id)
        .await?
        .ok_or(StoreError::StaleState)?;
    if digest(&actual)? != digest(input)? {
        return Err(StoreError::StaleState);
    }
    Ok(())
}
impl Store {
    pub async fn prepare_goal_review(
        &self,
        lease: &RunLease,
        registry: Vec<CapabilityInfo>,
    ) -> Result<Option<ModelContext>, StoreError> {
        if lease.mode != RunMode::Background {
            return Ok(None);
        }
        let mut context = self.model_context(lease.run_id, registry).await?;
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        if context.state_version != lease.state_version
            || context.privacy_epoch != lease.privacy_epoch
        {
            return Err(StoreError::StaleState);
        }
        let goal: Option<Uuid> = sqlx::query_scalar("SELECT goal_id FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&mut *tx)
            .await?;
        let Some(goal) = goal else { return Ok(None) };
        let Some(input) = snapshot(&mut tx, lease.individual_id, goal).await? else {
            return Ok(None);
        };
        if saved(&mut tx, lease.individual_id, &input).await?.is_some() {
            return Ok(None);
        }
        context
            .input_manifest
            .extend(input.evidence_ids.iter().copied());
        context.input_manifest.sort();
        context.input_manifest.dedup();
        if unavailable(&mut tx, lease.individual_id, &context.input_manifest).await? {
            return Err(StoreError::StaleState);
        }
        context.goal_review = Some(input);
        // The separate request sends only the review input, not the acting persona.
        tx.commit().await?;
        Ok(Some(context))
    }
    pub async fn commit_goal_review(
        &self,
        lease: &RunLease,
        prepared: &PreparedReview,
    ) -> Result<ReviewReceipt, StoreError> {
        let context = &prepared.context;
        let input = context
            .goal_review
            .as_ref()
            .ok_or(StoreError::Invalid("missing review input"))?;
        let assessment = &prepared.result.assessment;
        assessment
            .validate(input)
            .map_err(|_| StoreError::Invalid("invalid goal assessment"))?;
        if context.individual_id != lease.individual_id
            || context.run_id != lease.run_id
            || context.mode != RunMode::Background
            || lease.mode != RunMode::Background
            || context.state_version != lease.state_version
            || context.privacy_epoch != lease.privacy_epoch
            || context.config_version != lease.config_version
        {
            return Err(StoreError::StaleState);
        }
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let accounting:Option<(String,i64,i64,i64)>=sqlx::query_as("SELECT request_digest,input_bound,output_cap,lease_generation FROM usage_reservations WHERE id=$1 AND individual_id=$2 AND run_id=$3 AND status='settled' AND dispatched_at IS NOT NULL")
            .bind(prepared.reservation_id).bind(current.id).bind(lease.run_id).fetch_optional(&mut *tx).await?;
        let (expected, bound, cap, generation) =
            accounting.ok_or(StoreError::Invalid("unsettled model review"))?;
        if generation != lease.generation
            || expected
                != digest(
                    &json!({"context":context,"allowance":TokenAllowance{input_upper_bound:bound as u64,output_cap:cap as u32}}),
                )?
        {
            return Err(StoreError::StaleState);
        }
        let previous:Option<(Uuid,i64,i64,Value)>=sqlx::query_as("SELECT event_id,state_version,privacy_epoch,assessment FROM goal_reviews WHERE reservation_id=$1 AND individual_id=$2")
            .bind(prepared.reservation_id).bind(current.id).fetch_optional(&mut *tx).await?;
        if let Some((event_id, state_version, epoch, old)) = previous {
            if digest(&old)? != digest(assessment)? {
                return Err(StoreError::IdempotencyConflict);
            }
            if epoch != current.privacy_epoch
                || unavailable(&mut tx, current.id, &[event_id]).await?
            {
                return Err(StoreError::StaleState);
            }
            return Ok(ReviewReceipt {
                event_id,
                state_version,
                verdict: assessment.verdict,
            });
        }
        require_lease(&mut tx, lease).await?;
        let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals WHERE id=$1")
            .bind(current.id)
            .fetch_one(&mut *tx)
            .await?;
        if frozen {
            return Err(StoreError::BudgetUnavailable);
        }
        let goal: Option<Uuid> = sqlx::query_scalar("SELECT goal_id FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&mut *tx)
            .await?;
        if goal != Some(input.goal_id)
            || unavailable(&mut tx, current.id, &context.input_manifest).await?
        {
            return Err(StoreError::StaleState);
        }
        let actual = snapshot(&mut tx, current.id, input.goal_id)
            .await?
            .ok_or(StoreError::StaleState)?;
        if digest(&actual)? != digest(input)? {
            return Err(StoreError::StaleState);
        }
        if saved(&mut tx, current.id, input).await?.is_some() {
            return Err(StoreError::StaleState);
        }
        let unfinished:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM actions WHERE run_id=$1 AND status IN ('planned','executing','unknown')) OR EXISTS(SELECT 1 FROM usage_reservations WHERE run_id=$1 AND status IN ('reserved','dispatched'))")
            .bind(lease.run_id).fetch_one(&mut *tx).await?;
        if unfinished {
            return Err(StoreError::Invalid("unfinished action or call"));
        }
        let event_id = Uuid::new_v4();
        let state_version = current.state_version + 1;
        insert_event(&mut tx,current.id,event_id,"goal.reviewed","inference","goal_reviewer",
            json!({"goal_id":input.goal_id,"assessment":assessment,"checker_version":CHECKER,"input_digest":digest(input)?,"provider_model":prepared.result.provider_model,"provider_mode":prepared.result.provider_mode}),lease.run_id,None).await?;
        for source in &context.input_manifest {
            edge(&mut tx, current.id, *source, event_id).await?;
        }
        sqlx::query("INSERT INTO goal_reviews(individual_id,event_id,goal_id,goal_revision,privacy_epoch,input_digest,reservation_id,assessment,state_version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(current.id).bind(event_id).bind(input.goal_id).bind(input.goal_revision).bind(current.privacy_epoch).bind(digest(input)?).bind(prepared.reservation_id).bind(serde_json::to_value(assessment)?).bind(state_version).execute(&mut *tx).await?;
        require_lease(&mut tx, lease).await?;
        sqlx::query("UPDATE runs SET state_version=$2,input_manifest=array_append(input_manifest,$3) WHERE id=$1").bind(lease.run_id).bind(state_version).bind(event_id).execute(&mut *tx).await?;
        advance(&mut tx, current.id, state_version).await?;
        tx.commit().await?;
        Ok(ReviewReceipt {
            event_id,
            state_version,
            verdict: assessment.verdict,
        })
    }
}
