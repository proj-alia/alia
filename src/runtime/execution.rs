//! A single accounted cognition step. The eventual cycle driver owns tools,
//! retry/backoff, and atomic proposal commit; this never writes model output.
use super::{model::*, scheduling::RunLease};
use crate::{
    adapters::store::{Store, StoreError},
    domain::cognition::{CapabilityInfo, ModelContext},
};
use std::time::Duration;
use uuid::Uuid;

#[cfg(test)]
#[allow(dead_code)] // Only this fixture module uses a subset of shared test helpers.
mod trial_dispatch_tests {
    include!("../../tests/support/development_dispatch_unit.rs");
}

pub struct PreparedDecision {
    pub(crate) context: ModelContext,
    pub(crate) result: ModelResult,
    pub(crate) reservation_id: Uuid,
    pub(crate) grounding_review_id: Option<Uuid>,
}
impl PreparedDecision {
    pub fn grounding_review_id(&self) -> Option<Uuid> {
        self.grounding_review_id
    }
    pub fn context(&self) -> &ModelContext {
        &self.context
    }
    pub fn result(&self) -> &ModelResult {
        &self.result
    }
}
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error(transparent)]
    Storage(#[from] StoreError),
    #[error(transparent)]
    Model(#[from] ModelFailure),
    #[error("request has already been dispatched")]
    AlreadyDispatched,
}

pub async fn model_step(
    store: &Store,
    provider: &dyn ModelProvider,
    lease: &RunLease,
    capabilities: Vec<CapabilityInfo>,
    limits: CallLimits,
) -> Result<PreparedDecision, StepError> {
    if !(1..=8192).contains(&limits.max_output_tokens)
        || !(1..=120).contains(&limits.timeout_seconds)
    {
        return Err(ModelFailure::before_request(ModelError::Limits).into());
    }
    let context = store.model_context(lease.run_id, capabilities).await?;
    let metadata = match lease.mode {
        crate::domain::cognition::RunMode::Foreground => ModelCallMetadata::foreground(&context)?,
        crate::domain::cognition::RunMode::Background => ModelCallMetadata::Background {
            work_id: lease.run_id,
        },
    };
    let allowance = provider.token_allowance(&context, limits)?;
    let (result, reservation_id) = accounted_call(
        store,
        lease,
        &context,
        allowance,
        limits,
        provider.mode(),
        evaluate_proposal(provider, &context, limits, metadata),
        |r| r.usage,
    )
    .await?;
    let mut prepared = PreparedDecision {
        context,
        result,
        reservation_id,
        grounding_review_id: None,
    };
    let needs_review = !crate::domain::grounding::targets(&prepared.result.proposal).is_empty();
    if needs_review
        && (provider.mode() == ProviderMode::Live || provider.supports_decision_review())
    {
        let review_context = crate::domain::grounding::review_context(
            &prepared.context,
            reservation_id,
            &prepared.result.proposal,
        )
        .map_err(|_| ModelFailure::before_request(ModelError::Limits))?;
        let allowance = provider.decision_review_allowance(&review_context, limits)?;
        let (result, review_reservation_id) = loop {
            let work = async {
                let result = provider
                    .review_decision(&review_context, limits, metadata)
                    .await?;
                if result.provider_mode != provider.mode() {
                    return Err(ModelFailure {
                        http: None,
                        error: ModelError::Protocol,
                        usage: result.usage,
                        request_sent: provider.mode() == ProviderMode::Live,
                    });
                }
                result
                    .assessment
                    .validate(&review_context)
                    .map_err(|error| ModelFailure {
                        http: None,
                        error: ModelError::Assessment(error),
                        usage: result.usage,
                        request_sent: provider.mode() == ProviderMode::Live,
                    })?;
                Ok(result)
            };
            let reviewed = accounted_call(
                store,
                lease,
                &review_context,
                allowance,
                limits,
                provider.mode(),
                work,
                |r| r.usage,
            )
            .await;
            match reviewed {
                Ok(result) => break result,
                Err(StepError::Model(failure)) => {
                    use super::worker::RetryCause;
                    let cause = match failure.error {
                        ModelError::Assessment(error) => Some(RetryCause::Assessment(error)),
                        ModelError::Protocol => Some(RetryCause::Protocol),
                        ModelError::Truncated => Some(RetryCause::Truncated),
                        ModelError::Transport | ModelError::Http(408 | 429 | 500..=599) => {
                            Some(RetryCause::Transient)
                        }
                        _ => None,
                    };
                    if let Some(cause) = cause
                        && store
                            .schedule_review_retry(lease, cause, reservation_id)
                            .await?
                            .is_some()
                    {
                        let delay = store.cycle_progress(lease).await?.retry_delay;
                        tokio::time::sleep(delay.min(store.remaining_cycle_time(lease).await?))
                            .await;
                        continue;
                    }
                    return Err(failure.into());
                }
                Err(error) => return Err(error),
            }
        };
        let review_id = store
            .record_decision_review(
                lease,
                &prepared,
                &review_context,
                &result,
                review_reservation_id,
            )
            .await?;
        if !result.assessment.supported() {
            return Err(StepError::Storage(StoreError::Proposal(
                crate::domain::cognition::ProposalError::Grounding,
            )));
        }
        prepared.grounding_review_id = Some(review_id);
    }
    Ok(prepared)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn accounted_call<T>(
    store: &Store,
    lease: &RunLease,
    context: &crate::domain::cognition::ModelContext,
    allowance: super::scheduling::TokenAllowance,
    limits: CallLimits,
    mode: ProviderMode,
    work: impl std::future::Future<Output = Result<T, ModelFailure>>,
    usage_of: fn(&T) -> Option<TokenUsage>,
) -> Result<(T, Uuid), StepError> {
    accounted_call_with_request_key(
        store,
        lease,
        context,
        Uuid::new_v4(),
        allowance,
        limits,
        mode,
        work,
        usage_of,
    )
    .await
}

/// Trial controllers persist their case/side request key before starting work.
/// An existing dispatched/settled reservation never executes this future again.
/// Recovery of its result remains the controller's responsibility, not a retry.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn accounted_call_with_request_key<T>(
    store: &Store,
    lease: &RunLease,
    context: &crate::domain::cognition::ModelContext,
    request_key: Uuid,
    allowance: super::scheduling::TokenAllowance,
    limits: CallLimits,
    mode: ProviderMode,
    work: impl std::future::Future<Output = Result<T, ModelFailure>>,
    usage_of: fn(&T) -> Option<TokenUsage>,
) -> Result<(T, Uuid), StepError> {
    if !(1..=8192).contains(&limits.max_output_tokens)
        || !(1..=120).contains(&limits.timeout_seconds)
        || allowance.output_cap != limits.max_output_tokens
        || request_key.is_nil()
    {
        return Err(ModelFailure::before_request(ModelError::Limits).into());
    }
    let reservation = store
        .reserve_model_call(lease, request_key, context, allowance)
        .await?;
    let dispatched = if context.trial_binding.is_some() {
        store
            .dispatch_model_call_with_context(lease, reservation.id, context, mode)
            .await?
    } else {
        store.dispatch_model_call(lease, reservation.id).await?
    };
    if !dispatched {
        return Err(StepError::AlreadyDispatched);
    }
    let remaining = match store.remaining_cycle_time(lease).await {
        Ok(duration) => duration.min(Duration::from_secs(limits.timeout_seconds)),
        Err(error) => {
            store.settle_model_call(reservation.id, None).await?;
            return Err(error.into());
        }
    };
    let result = {
        let work = tokio::time::timeout(remaining, work);
        tokio::pin!(work);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        heartbeat.tick().await;
        loop {
            tokio::select! {
                result=&mut work => break result.unwrap_or_else(|_|Err(ModelFailure {http:None,error:ModelError::Transport,usage:None,request_sent:mode==ProviderMode::Live})),
                _=heartbeat.tick()=>{
                    if let Err(error)=store.renew_lease(lease).await {
                        // Dropping HTTP cannot prove the remote call was cancelled.
                        store.settle_model_call(reservation.id,None).await?;
                        return Err(error.into());
                    }
                }
            }
        }
    };
    let usage = match &result {
        Ok(result) => usage_of(result),
        Err(error) => error.usage,
    };
    store.settle_model_call(reservation.id, usage).await?;
    if usage.is_some_and(|u| {
        u.input > allowance.input_upper_bound || u.output > u64::from(allowance.output_cap)
    }) {
        return Err(ModelFailure {
            http: None,
            error: ModelError::Limits,
            usage,
            request_sent: mode == ProviderMode::Live,
        }
        .into());
    }
    store.renew_lease(lease).await?; // no stale/private proposal leaves this step
    let result = result?;
    Ok((result, reservation.id))
}

/// Execute one admitted network read outside any database transaction. A lost
/// lease drops the response; recovery conservatively records the in-flight action
/// as unknown. The adapter never gets deployment credentials or model context.
pub async fn execute_web_action(
    store: &Store,
    reader: &dyn super::web::WebReader,
    lease: &RunLease,
    action: Uuid,
) -> Result<super::capability::ActionReceipt, StoreError> {
    use super::web::*;
    use crate::adapters::web_actions::WebStart;
    let dispatch = match store.begin_web_action(lease, action, reader.mode()).await? {
        WebStart::Receipt(receipt) => return Ok(receipt),
        WebStart::Dispatch(dispatch) => dispatch,
    };
    let timeout = store
        .remaining_cycle_time(lease)
        .await?
        .min(Duration::from_secs(20));
    let work = tokio::time::timeout(timeout, reader.read(&dispatch.target));
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    heartbeat.tick().await;
    let result = loop {
        tokio::select! {
            result=&mut work=>break result.unwrap_or_else(|_|Err(WebError::Timeout.into())),
            _=heartbeat.tick()=>{store.renew_lease(lease).await?;}
        }
    };
    let result = result.and_then(|doc| doc.validate().map(|_| doc));
    store.finish_web_action(&dispatch, &result).await
}

#[cfg(test)]
mod request_key_tests {
    use super::*;
    use crate::{
        domain::config::{PersonaSeed, RuntimeRules},
        runtime::capability::IMPLEMENTED_CAPABILITIES,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[sqlx::test]
    #[ignore = "actual disposable PostgreSQL; synthetic usage and no model/network"]
    async fn persisted_request_key_recovers_unsent_reservation_without_reexecuting_settled_work(
        pool: sqlx::PgPool,
    ) {
        let store = Store::new(pool.clone());
        store
            .initialize(
                "request-key-init",
                &PersonaSeed::parse(include_bytes!("../../config/persona.example.json")).unwrap(),
                &RuntimeRules::parse(
                    include_bytes!("../../config/runtime.example.json"),
                    IMPLEMENTED_CAPABILITIES,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        store
            .submit_message(
                "request-key-message",
                "合成评估请求，不是生产试用证明",
                None,
            )
            .await
            .unwrap();
        let lease = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
        let context = store.model_context(lease.run_id, vec![]).await.unwrap();
        let key = Uuid::new_v4();
        let allowance = super::super::scheduling::TokenAllowance {
            input_upper_bound: 100,
            output_cap: 50,
        };
        let limits = CallLimits {
            max_output_tokens: 50,
            timeout_seconds: 5,
        };
        let reserved = store
            .reserve_model_call(&lease, key, &context, allowance)
            .await
            .unwrap();
        let polled = AtomicUsize::new(0);
        let usage = TokenUsage {
            input: 10,
            output: 5,
            total: 15,
        };
        let work = || async {
            polled.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ModelFailure>(usage)
        };
        let (_, recovered) = accounted_call_with_request_key(
            &store,
            &lease,
            &context,
            key,
            allowance,
            limits,
            ProviderMode::Fixture,
            work(),
            |u| Some(*u),
        )
        .await
        .unwrap();
        assert_eq!(recovered, reserved.id);
        assert_eq!(polled.load(Ordering::SeqCst), 1);
        assert!(matches!(
            accounted_call_with_request_key(
                &store,
                &lease,
                &context,
                key,
                allowance,
                limits,
                ProviderMode::Fixture,
                work(),
                |u| Some(*u),
            )
            .await,
            Err(StepError::AlreadyDispatched)
        ));
        assert_eq!(polled.load(Ordering::SeqCst), 1);
        let (count, charged, calls): (i64,i64,i32) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM usage_reservations u WHERE u.run_id=r.id),(SELECT sum(charged_tokens)::bigint FROM usage_reservations u WHERE u.run_id=r.id),model_calls FROM runs r WHERE id=$1",
        ).bind(lease.run_id).fetch_one(&pool).await.unwrap();
        assert_eq!((count, charged, calls), (1, 15, 1));
    }
}
