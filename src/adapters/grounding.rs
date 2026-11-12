//! Bind a separate, accounted assessment to an exact candidate before state writes.
use super::{goals::unavailable, scheduling::require_lease, store::*};
use crate::{
    domain::{
        cognition::{ModelContext, ProposalError},
        grounding::*,
    },
    runtime::{
        execution::PreparedDecision,
        model::{DecisionReviewResult, ProviderMode},
        scheduling::{RunLease, TokenAllowance},
    },
};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
fn checker_version() -> &'static str {
    static VERSION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        format!(
            "decision_grounding/1:{}",
            digest(&json!([RULES, SCHEMA, PACKET_VERSION])).expect("static review policy")
        )
    });
    &VERSION
}
#[derive(sqlx::FromRow)]
struct ReviewReceipt {
    review_reservation_id: Uuid,
    draft_digest: String,
    context_digest: String,
    review_context_digest: String,
    assessment: Value,
    supported: bool,
    checker_version: String,
    provider_mode: String,
}
pub(super) async fn accounted(
    tx: &mut Transaction<'_, Postgres>,
    lease: &RunLease,
    reservation: Uuid,
    context: &ModelContext,
) -> Result<(), StoreError> {
    let row:Option<(String,i64,i64,i64)>=sqlx::query_as("SELECT request_digest,input_bound,output_cap,lease_generation FROM usage_reservations WHERE id=$1 AND individual_id=$2 AND run_id=$3 AND status='settled' AND dispatched_at IS NOT NULL").bind(reservation).bind(lease.individual_id).bind(lease.run_id).fetch_optional(&mut **tx).await?;
    let (expected, input, output, generation) =
        row.ok_or(StoreError::Invalid("unsettled decision assessment"))?;
    let actual = digest(
        &json!({"context":context,"allowance":TokenAllowance{input_upper_bound:input as u64,output_cap:output as u32}}),
    )?;
    if expected != actual || generation != lease.generation {
        return Err(StoreError::StaleState);
    }
    Ok(())
}

impl Store {
    pub(crate) async fn record_decision_review(
        &self,
        lease: &RunLease,
        decision: &PreparedDecision,
        context: &ModelContext,
        result: &DecisionReviewResult,
        review_reservation: Uuid,
    ) -> Result<Uuid, StoreError> {
        let rebuilt = review_context(
            &decision.context,
            decision.reservation_id,
            &decision.result.proposal,
        )
        .map_err(|_| StoreError::Invalid("assessment context"))?;
        if digest(&rebuilt)? != digest(context)?
            || decision.result.provider_mode != result.provider_mode
            || review_reservation == decision.reservation_id
            || result.assessment.validate(context).is_err()
        {
            return Err(StoreError::StaleState);
        }
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        super::development_context::require_context(&mut tx, context).await?;
        if (
            context.individual_id,
            context.run_id,
            context.state_version,
            context.privacy_epoch,
            context.config_version,
        ) != (
            lease.individual_id,
            lease.run_id,
            lease.state_version,
            lease.privacy_epoch,
            lease.config_version,
        ) || unavailable(&mut tx, lease.individual_id, &context.input_manifest).await?
        {
            return Err(StoreError::StaleState);
        }
        accounted(&mut tx, lease, decision.reservation_id, &decision.context).await?;
        accounted(&mut tx, lease, review_reservation, context).await?;
        let old:Option<(Uuid,Uuid,String,Value)>=sqlx::query_as("SELECT event_id,review_reservation_id,draft_digest,assessment FROM decision_reviews WHERE author_reservation_id=$1").bind(decision.reservation_id).fetch_optional(&mut *tx).await?;
        if let Some((event, reservation, hash, assessment)) = old {
            if reservation != review_reservation
                || hash != digest(&decision.result.proposal)?
                || assessment != serde_json::to_value(&result.assessment)?
            {
                return Err(StoreError::IdempotencyConflict);
            }
            if unavailable(&mut tx, lease.individual_id, &[event]).await? {
                return Err(StoreError::StaleState);
            }
            return Ok(event);
        }
        let event = Uuid::new_v4();
        let supported = result.assessment.supported();
        insert_event(&mut tx,lease.individual_id,event,"decision.reviewed","inference","content_reviewer",json!({"checker_version":checker_version(),"supported":supported,"assessment":result.assessment,"candidate":decision.result.proposal,"candidate_status":"generated_draft_not_experience","author_reservation_id":decision.reservation_id,"review_reservation_id":review_reservation,"provider_mode":result.provider_mode,"reviewer_model":result.provider_model}),lease.run_id,None).await?;
        for source in &context.input_manifest {
            edge(&mut tx, lease.individual_id, *source, event).await?;
        }
        sqlx::query("INSERT INTO decision_reviews(author_reservation_id,review_reservation_id,individual_id,run_id,event_id,lease_generation,state_version,privacy_epoch,config_version,draft_digest,context_digest,review_context_digest,assessment,supported,checker_version,provider_mode) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
            .bind(decision.reservation_id).bind(review_reservation).bind(lease.individual_id).bind(lease.run_id).bind(event).bind(lease.generation).bind(lease.state_version).bind(lease.privacy_epoch).bind(lease.config_version).bind(digest(&decision.result.proposal)?).bind(digest(&decision.context)?).bind(digest(context)?).bind(serde_json::to_value(&result.assessment)?).bind(supported).bind(checker_version()).bind(match result.provider_mode{ProviderMode::Live=>"live",ProviderMode::Fixture=>"fixture"}).execute(&mut *tx).await?;
        // Run-local validation metadata, not an individual-state transition.
        outbox(
            &mut tx,
            lease.individual_id,
            "operation.updated",
            lease.run_id,
            lease.state_version,
        )
        .await?;
        tx.commit().await?;
        Ok(event)
    }
    pub(super) async fn require_decision_review(
        tx: &mut Transaction<'_, Postgres>,
        lease: &RunLease,
        decision: &PreparedDecision,
    ) -> Result<Option<Uuid>, StoreError> {
        let needed = !targets(&decision.result.proposal).is_empty();
        if decision.grounding_review_id.is_none() {
            return if needed && decision.result.provider_mode == ProviderMode::Live {
                Err(ProposalError::Grounding.into())
            } else {
                Ok(None)
            };
        }
        let review = decision.grounding_review_id.ok_or(StoreError::StaleState)?;
        let row=sqlx::query_as::<_, ReviewReceipt>("SELECT review_reservation_id,draft_digest,context_digest,review_context_digest,assessment,supported,checker_version,provider_mode FROM decision_reviews WHERE author_reservation_id=$1 AND individual_id=$2 AND run_id=$3 AND event_id=$4 AND lease_generation=$5 AND state_version=$6 AND privacy_epoch=$7 AND config_version=$8")
            .bind(decision.reservation_id).bind(lease.individual_id).bind(lease.run_id).bind(review).bind(lease.generation).bind(lease.state_version).bind(lease.privacy_epoch).bind(lease.config_version).fetch_optional(&mut **tx).await?;
        let ReviewReceipt {
            review_reservation_id: reservation,
            draft_digest: draft,
            context_digest: original,
            review_context_digest: reviewed,
            assessment,
            supported,
            checker_version: checker,
            provider_mode: mode,
        } = row.ok_or(ProposalError::Grounding)?;
        let context = review_context(
            &decision.context,
            decision.reservation_id,
            &decision.result.proposal,
        )
        .map_err(|_| StoreError::Invalid("review reconstruction"))?;
        let assessment: DecisionAssessment = serde_json::from_value(assessment)?;
        let expected_mode = match decision.result.provider_mode {
            ProviderMode::Live => "live",
            ProviderMode::Fixture => "fixture",
        };
        if !supported
            || !assessment.supported()
            || assessment.validate(&context).is_err()
            || draft != digest(&decision.result.proposal)?
            || original != digest(&decision.context)?
            || reviewed != digest(&context)?
            || checker != checker_version()
            || mode != expected_mode
            || unavailable(tx, lease.individual_id, &[review]).await?
        {
            return Err(ProposalError::Grounding.into());
        }
        accounted(tx, lease, reservation, &context).await?;
        Ok(Some(review))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
    async fn a_live_result_without_a_review_receipt_cannot_use_the_fixture_path(
        pool: sqlx::PgPool,
    ) {
        use crate::{
            domain::config::{PersonaSeed, RuntimeRules},
            runtime::{
                capability::IMPLEMENTED_CAPABILITIES,
                execution::model_step,
                model::{CallLimits, FixtureProvider},
            },
        };
        let s = Store::new(pool.clone());
        s.initialize(
            "init",
            &PersonaSeed::parse(include_bytes!("../../config/persona.example.json")).unwrap(),
            &RuntimeRules::parse(
                include_bytes!("../../config/runtime.example.json"),
                IMPLEMENTED_CAPABILITIES,
            )
            .unwrap(),
        )
        .await
        .unwrap();
        s.submit_message("input", "实际输入", None).await.unwrap();
        let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
        let caps = s.local_capabilities().await.unwrap();
        let mut ready=model_step(&s,&FixtureProvider{response:json!({"schema_version":1,"reply":"未核实的候选。","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"合成夹具"}).to_string()},&lease,caps.clone(),CallLimits::default()).await.unwrap();
        // Adversarial private-field mutation only in this internal test.
        ready.result.provider_mode = ProviderMode::Live;
        assert!(matches!(
            s.commit_decision(&lease, &ready, &caps).await,
            Err(StoreError::Proposal(ProposalError::Grounding))
        ));
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM events WHERE kind='message.sent'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
}
