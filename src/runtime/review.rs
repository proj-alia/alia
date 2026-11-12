use super::{
    execution::{StepError, accounted_call},
    model::*,
    scheduling::RunLease,
};
use crate::{adapters::store::Store, domain::cognition::ModelContext};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewReceipt {
    pub event_id: Uuid,
    pub state_version: i64,
    pub verdict: crate::domain::review::ReviewVerdict,
}

pub struct PreparedReview {
    pub(crate) context: ModelContext,
    pub(crate) result: GoalReviewResult,
    pub(crate) reservation_id: Uuid,
}
impl PreparedReview {
    pub fn context(&self) -> &ModelContext {
        &self.context
    }
    pub fn result(&self) -> &GoalReviewResult {
        &self.result
    }
    pub fn reservation_id(&self) -> Uuid {
        self.reservation_id
    }
}
pub async fn review_step(
    store: &Store,
    provider: &dyn ModelProvider,
    lease: &RunLease,
    context: ModelContext,
    limits: CallLimits,
) -> Result<PreparedReview, StepError> {
    if context.mode != crate::domain::cognition::RunMode::Background || lease.mode != context.mode {
        return Err(ModelFailure::before_request(ModelError::Configuration).into());
    }
    let input = context
        .goal_review
        .as_ref()
        .ok_or_else(|| ModelFailure::before_request(ModelError::Configuration))?;
    let allowance = provider.review_allowance(input, limits)?;
    let metadata = ModelCallMetadata::Background {
        work_id: lease.run_id,
    };
    let work = async {
        let result = provider.review(input, limits, metadata, &context).await?;
        if result.provider_mode != provider.mode() || result.assessment.validate(input).is_err() {
            return Err(ModelFailure {
                http: None,
                error: ModelError::Protocol,
                usage: result.usage,
                request_sent: provider.mode() == ProviderMode::Live,
            });
        }
        Ok(result)
    };
    let (result, reservation_id) = accounted_call(
        store,
        lease,
        &context,
        allowance,
        limits,
        provider.mode(),
        work,
        |r| r.usage,
    )
    .await?;
    Ok(PreparedReview {
        context,
        result,
        reservation_id,
    })
}
