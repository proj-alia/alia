//! Bounded trial execution. A projection is neither a trusted measurement nor
//! permission to dispatch: the trial-context adapter must validate its durable
//! work/case binding through the normal reservation and dispatch gates.
use crate::domain::development_trials::{
    MEASUREMENT_VERSION, TrialBinding, TrialPlan, TrialPlanDraft, TrialSide, TrialWork,
};
use crate::{
    domain::{
        cognition::ModelContext,
        config::PersonaSeed,
        development::{
            DevelopmentCandidate, DevelopmentTarget, StrategyScope, TargetSnapshot, TaskStrategy,
        },
    },
    runtime::scheduling::RunLease,
};
use chrono::{DateTime, Utc};

/// Project a frozen work item's subject onto its actual evaluation run. This is
/// correlation validation, not DB authorization: the adapter still reloads all
/// actual work/head/source/ledger state at each dispatch phase.
#[cfg(test)]
fn bound_case_context(
    base: &ModelContext,
    candidate: &DevelopmentCandidate,
    work: &TrialWork,
    binding: &TrialBinding,
    lease: &RunLease,
    now: DateTime<Utc>,
) -> Result<ModelContext, TrialRuntimeError> {
    if work.evaluation_run_id != lease.run_id
        || work.baseline.attempt_id == work.trial.attempt_id
        || work.baseline.side != TrialSide::Baseline
        || work.trial.side != TrialSide::Trial
        || !work.plan.cases.iter().any(|case| case == &work.case)
    {
        return Err(TrialRuntimeError::Binding);
    }
    for side in [&work.baseline, &work.trial] {
        if side.plan_id != work.plan.id
            || side.case_id != work.case.id
            || side.attempt_id.is_nil()
            || side.evaluation_run_id != lease.run_id
            || side.lease_generation != lease.generation
        {
            return Err(TrialRuntimeError::Binding);
        }
    }
    if binding
        != match binding.side {
            TrialSide::Baseline => &work.baseline,
            TrialSide::Trial => &work.trial,
        }
    {
        return Err(TrialRuntimeError::Binding);
    }
    plan_case_context(base, candidate, &work.plan, binding, lease, now)
}

pub(crate) fn plan_case_context(
    base: &ModelContext,
    candidate: &DevelopmentCandidate,
    plan: &TrialPlan,
    binding: &TrialBinding,
    lease: &RunLease,
    now: DateTime<Utc>,
) -> Result<ModelContext, TrialRuntimeError> {
    require_trial_lease(base, candidate, lease, now)?;
    let case = plan
        .cases
        .iter()
        .find(|case| case.id == binding.case_id)
        .ok_or(TrialRuntimeError::Binding)?;
    if plan.deadline <= now || plan.deadline > candidate.draft().deadline {
        return Err(TrialRuntimeError::Expired);
    }
    let before = candidate.before();
    if base.trial_binding.is_some()
        || plan.individual_id != base.individual_id
        || plan.development_id != candidate.id()
        || plan.development_version <= 0
        || &plan.draft != candidate.draft()
        || plan.proposer_id != candidate.proposer_id()
        || plan.evaluator_id == candidate.proposer_id()
        || plan.original_scope != before.scope
        || plan.target.scope != before.scope
        || plan.target.target != before.target
        || plan.target.version != before.version
        || plan.target.head_revision_id != before.head_revision_id
        || plan.target.value != before.value
        || plan.criterion_version != MEASUREMENT_VERSION
        || plan.id.is_nil()
        || plan.evaluator_id.is_nil()
        || plan.trial_event_id.is_nil()
        || binding.plan_id != plan.id
        || binding.evaluation_run_id != lease.run_id
        || binding.lease_generation != lease.generation
        || binding.attempt_id.is_nil()
    {
        return Err(TrialRuntimeError::Binding);
    }
    TrialPlanDraft {
        cases: plan.cases.clone(),
        deadline: plan.deadline,
    }
    .validate(now)
    .map_err(|_| TrialRuntimeError::Binding)?;
    if std::iter::once(candidate.id())
        .chain(std::iter::once(plan.trial_event_id))
        .chain(before.head_revision_id)
        .chain(candidate.draft().evidence_ids.iter().copied())
        .chain(candidate.draft().counterevidence_ids.iter().copied())
        .chain(case.source_event_ids.iter().copied())
        .chain(before.scope.as_ref().map(|scope| match scope {
            StrategyScope::Task(id) | StrategyScope::Goal(id) => *id,
        }))
        .any(|id| id.is_nil() || !base.input_manifest.contains(&id))
    {
        return Err(TrialRuntimeError::Sources);
    }
    if !base.events.iter().any(|event| {
        event.id == base.current_event_id
            && event.individual_id == base.individual_id
            && event
                .payload
                .get("text")
                .and_then(serde_json::Value::as_str)
                == Some(case.prompt.as_str())
    }) {
        return Err(TrialRuntimeError::Prompt);
    }
    let mut context = if before.target.persona_pointer().is_some() {
        let overlay = trial_context(base, candidate, &plan.target, lease, now)?;
        match binding.side {
            TrialSide::Baseline => base.clone(),
            TrialSide::Trial => overlay,
        }
    } else {
        PersonaSeed::validate(base.persona.clone()).map_err(|_| TrialRuntimeError::Persona)?;
        let mut context = base.clone();
        let development = context
            .development_context
            .as_mut()
            .ok_or(TrialRuntimeError::Sources)?;
        if development
            .effective_task_strategy
            .as_ref()
            .is_some_and(|strategy| {
                matches!(strategy.scope, StrategyScope::Task(_))
                    && Some(&strategy.scope) != before.scope.as_ref()
            })
        {
            return Err(TrialRuntimeError::Scope);
        }
        let value = match binding.side {
            TrialSide::Baseline => &before.value,
            TrialSide::Trial => &candidate.draft().proposed_value,
        };
        // No explicit original task strategy means the ordinary goal fallback
        // stays in place on the baseline side. Its scope is never rewritten.
        if !value.is_null() {
            let strategy: TaskStrategy =
                serde_json::from_value(value.clone()).map_err(|_| TrialRuntimeError::Scope)?;
            if Some(&strategy.scope) != before.scope.as_ref() {
                return Err(TrialRuntimeError::Scope);
            }
            development.effective_task_strategy = Some(strategy);
        }
        context
    };
    // Never serialize plan/cases/criteria into model input. Only the correlation
    // metadata travels; actual DB records remain the authority for its meaning.
    context.trial_binding = Some(binding.clone());
    Ok(context)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TrialRuntimeError {
    #[error("trial lease or snapshot no longer matches")]
    Stale,
    #[error("trial deadline or lease expired")]
    Expired,
    #[error("trial scope does not match this run")]
    Scope,
    #[error("trial inputs do not contain the complete candidate sources")]
    Sources,
    #[error("trial candidate does not preserve the persona contract")]
    Persona,
    #[error("trial work or case binding does not match")]
    Binding,
    #[error("trial case prompt is not the actual trigger input")]
    Prompt,
}

/// Construct a request copy without modifying the accepted head or baseline.
/// The caller must provide a source-checked plan and must still pass the copied
/// context through the program-authorized trial validation/accounting path.
pub(crate) fn trial_context(
    base: &ModelContext,
    candidate: &DevelopmentCandidate,
    current: &TargetSnapshot,
    lease: &RunLease,
    now: DateTime<Utc>,
) -> Result<ModelContext, TrialRuntimeError> {
    require_trial_lease(base, candidate, lease, now)?;
    let before = candidate.before();
    if current.target != before.target
        || current.scope != before.scope
        || current.version != before.version
        || current.head_revision_id != before.head_revision_id
        || current.value != before.value
    {
        return Err(TrialRuntimeError::Stale);
    }
    let development = base
        .development_context
        .as_ref()
        .ok_or(TrialRuntimeError::Sources)?;
    let head = development
        .heads
        .iter()
        .find(|head| {
            head.target == current.target
                && head.scope == current.scope
                && head.version == current.version
                && head.revision_id == current.head_revision_id
        })
        .ok_or(TrialRuntimeError::Stale)?;
    if std::iter::once(candidate.id())
        .chain(std::iter::once(head.target_id))
        .chain(current.head_revision_id)
        .chain(candidate.draft().evidence_ids.iter().copied())
        .chain(candidate.draft().counterevidence_ids.iter().copied())
        .any(|id| id.is_nil() || !base.input_manifest.contains(&id))
    {
        return Err(TrialRuntimeError::Sources);
    }
    PersonaSeed::validate(base.persona.clone()).map_err(|_| TrialRuntimeError::Persona)?;
    let mut trial = base.clone();
    if let Some(pointer) = current.target.persona_pointer() {
        if current.scope.is_some() || base.persona.pointer(pointer) != Some(&current.value) {
            return Err(TrialRuntimeError::Stale);
        }
        *trial
            .persona
            .pointer_mut(pointer)
            .ok_or(TrialRuntimeError::Persona)? = candidate.draft().proposed_value.clone();
        PersonaSeed::validate(trial.persona.clone()).map_err(|_| TrialRuntimeError::Persona)?;
    } else if current.target == DevelopmentTarget::TaskStrategy {
        let strategy: TaskStrategy =
            serde_json::from_value(candidate.draft().proposed_value.clone())
                .map_err(|_| TrialRuntimeError::Scope)?;
        if Some(&strategy.scope) != current.scope.as_ref() {
            return Err(TrialRuntimeError::Scope);
        }
        let id = match strategy.scope {
            StrategyScope::Task(id) if id == base.run_id => id,
            StrategyScope::Goal(id) if Some(id) == base.current_goal_id => {
                // A goal experiment cannot silently replace a more specific
                // accepted task strategy on this run.
                if development
                    .effective_task_strategy
                    .as_ref()
                    .is_some_and(|s| matches!(s.scope, StrategyScope::Task(_)))
                {
                    return Err(TrialRuntimeError::Scope);
                }
                id
            }
            _ => return Err(TrialRuntimeError::Scope),
        };
        if !base.input_manifest.contains(&id) {
            return Err(TrialRuntimeError::Sources);
        }
        trial
            .development_context
            .as_mut()
            .ok_or(TrialRuntimeError::Sources)?
            .effective_task_strategy = Some(strategy);
    }
    Ok(trial)
}

fn require_trial_lease(
    base: &ModelContext,
    candidate: &DevelopmentCandidate,
    lease: &RunLease,
    now: DateTime<Utc>,
) -> Result<(), TrialRuntimeError> {
    if now >= candidate.draft().deadline || now >= lease.deadline || now >= lease.expires_at {
        return Err(TrialRuntimeError::Expired);
    }
    if lease.owner_id.is_nil()
        || lease.run_id.is_nil()
        || lease.generation <= 0
        || base.now > now
        || base.individual_id != candidate.individual_id()
        || base.individual_id != lease.individual_id
        || base.run_id != lease.run_id
        || base.mode != lease.mode
        || base.state_version != lease.state_version
        || base.privacy_epoch != lease.privacy_epoch
        || base.config_version != lease.config_version
    {
        return Err(TrialRuntimeError::Stale);
    }
    Ok(())
}

// These tests exercise this registered module, rather than a second copy of it.
#[cfg(test)]
mod projection_tests {
    include!("../../tests/support/development_trials_projection.rs");
}

use crate::{
    adapters::{
        development_trial_context::prepare_case_context,
        store::{Store, StoreError},
    },
    domain::development_trials::{TrialAttemptState, TrialDisposition},
    runtime::{
        execution::{StepError, accounted_call_with_request_key},
        model::{
            CallLimits, ModelCallMetadata, ModelError, ModelFailure, ModelProvider, ProviderMode,
        },
        scheduling::RunOutcome,
        worker::{StopReason, WorkerOutcome},
    },
};

/// Idle admission only creates a durable sourced case. The original scheduler
/// still chooses and leases it, with foreground priority and one active run.
pub(crate) async fn enqueue_pending_case(store: &Store) -> Result<bool, StoreError> {
    let Some(case) = store
        .pending_development_trial_cases(1)
        .await?
        .into_iter()
        .next()
    else {
        return Ok(false);
    };
    match store
        .enqueue_development_trial_case(
            &format!("trial-enqueue:{}:{}", case.plan_id, case.case_id),
            case.plan_id,
            case.case_id,
        )
        .await
    {
        Ok(_) => Ok(true),
        Err(
            StoreError::Invalid("trial waits for normal idle admission") | StoreError::Inactive,
        ) => Ok(false),
        Err(error) => Err(error),
    }
}

fn terminal(state: TrialAttemptState) -> bool {
    matches!(
        state,
        TrialAttemptState::Recorded | TrialAttemptState::Unknown | TrialAttemptState::NotRun
    )
}

async fn finalize_plan(
    store: &Store,
    plan: &TrialPlan,
    interrupted: bool,
) -> Result<(), StoreError> {
    use crate::domain::development::Outcome;
    let reviewed = store
        .review_development_trial_plan(&format!("trial-review:{}", plan.id), plan.id)
        .await?;
    let disposition = if Utc::now() >= plan.draft.deadline {
        TrialDisposition::Expire
    } else if !interrupted && reviewed.outcome == Outcome::Passed {
        TrialDisposition::Accept
    } else {
        TrialDisposition::Reject {
            reason: if interrupted {
                "受控试运行中断；缺失或未知结果不能构成接受依据。".into()
            } else {
                "独立测量未通过；保留失败及未验证的全部场景与反例。".into()
            },
        }
    };
    store
        .finalize_development_trial_plan(
            &format!("trial-finalize:{}", plan.id),
            plan.id,
            disposition,
        )
        .await?;
    Ok(())
}

async fn execute_case(
    store: &Store,
    provider: &dyn ModelProvider,
    lease: &RunLease,
    work: &TrialWork,
) -> Result<(), StepError> {
    let recovered = store.recover_development_trial_work(work.plan.id).await?;
    for binding in [&work.baseline, &work.trial] {
        let state = recovered
            .attempts
            .iter()
            .find(|a| a.binding.attempt_id == binding.attempt_id)
            .ok_or(StoreError::Invalid("trial attempt absent"))?
            .state;
        if state == TrialAttemptState::Recorded {
            continue;
        }
        if terminal(state) {
            return Err(StepError::AlreadyDispatched);
        }
        let context =
            prepare_case_context(store, lease, work, binding, store.capabilities().await?).await?;
        let limits = CallLimits::default();
        let allowance = provider.token_allowance(&context, limits)?;
        let key = store
            .bind_development_trial_request(lease, binding, &context, allowance)
            .await?;
        let call = async {
            let mut result = provider
                .decide(
                    &context,
                    limits,
                    ModelCallMetadata::Background {
                        work_id: lease.run_id,
                    },
                )
                .await?;
            let failure = |error| ModelFailure {
                http: result.http.clone(),
                error,
                usage: result.usage,
                request_sent: provider.mode() == ProviderMode::Live,
            };
            if result.provider_mode != provider.mode() {
                return Err(failure(ModelError::Protocol));
            }
            // Parse the typed adapter output against the closed wire schema.
            // Production validate_context intentionally rejects trial bindings;
            // no production proposal commit is reachable from this driver.
            let bytes =
                serde_json::to_vec(&result.proposal).map_err(|_| failure(ModelError::Protocol))?;
            result.proposal = crate::domain::cognition::DecisionProposal::parse(&bytes)
                .map_err(|e| failure(ModelError::Proposal(e)))?;
            Ok(result)
        };
        let (result, reservation) = accounted_call_with_request_key(
            store,
            lease,
            &context,
            key,
            allowance,
            limits,
            provider.mode(),
            call,
            |r| r.usage,
        )
        .await?;
        store
            .complete_development_trial_attempt(lease, binding, reservation, &context, &result)
            .await?;
    }
    Ok(())
}

/// Root delegates a genuinely claimed run here before ordinary cognition. None
/// means no trial mapping; Root's independent evaluation-run guard still fails
/// closed for an expired/finished mapping which can no longer be claimed.
pub(crate) async fn drive_claimed_case(
    store: &Store,
    provider: &dyn ModelProvider,
    lease: &RunLease,
) -> Result<Option<WorkerOutcome>, StoreError> {
    let Some(work) = store.claim_development_trial_work(lease).await? else {
        return Ok(None);
    };
    match execute_case(store, provider, lease, &work).await {
        Ok(()) => {
            store.finish_development_trial_work(lease).await?;
            let all = store.recover_development_trial_work(work.plan.id).await?;
            if all.attempts.len() == work.plan.cases.len() * 2
                && all.attempts.iter().all(|a| terminal(a.state))
            {
                finalize_plan(store, &work.plan, false).await?;
            }
            Ok(Some(WorkerOutcome::Committed(lease.run_id)))
        }
        Err(StepError::Storage(
            StoreError::LeaseLost | StoreError::StaleState | StoreError::PrivacyUnavailable,
        )) => Ok(Some(WorkerOutcome::Fenced(lease.run_id))),
        Err(error) => {
            let reason = match &error {
                StepError::Storage(StoreError::BudgetUnavailable) => StopReason::Budget,
                StepError::Storage(StoreError::CallLimit) => StopReason::CallLimit,
                StepError::Model(f) => match f.error {
                    ModelError::Unavailable => StopReason::ModelUnavailable,
                    ModelError::UnverifiedTokenBound => StopReason::UnverifiedTokens,
                    ModelError::Protocol | ModelError::Proposal(_) => StopReason::ModelRejected,
                    _ => StopReason::ModelFailure,
                },
                _ => StopReason::ModelFailure,
            };
            // Release/settle via the original run-local accounting operation;
            // never retry an unknown side or manufacture a successful result.
            store.finish_run(lease, RunOutcome::Failed).await?;
            store.recover_development_trial_work(work.plan.id).await?;
            finalize_plan(store, &work.plan, true).await?;
            Ok(Some(WorkerOutcome::Stopped(lease.run_id, reason)))
        }
    }
}

#[cfg(test)]
#[allow(dead_code)] // The shared fixture also provides HTTP helpers unused here.
mod driver_tests {
    use crate as alia;
    include!("../../tests/support/development_context_fixture.rs");
    use crate::{
        domain::development_trials::*,
        runtime::{
            execution::model_step,
            model::{CallLimits, FixtureProvider},
            scheduling::RunLease,
        },
    };
    include!("../../tests/support/development_trial_run_fixture.rs");

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn driver_finishes_three_real_cases_and_rejects_fixture_growth(pool: PgPool) {
        let (s, lease, plan, _) = work(&pool).await;
        let initial = s.snapshot().await.unwrap().unwrap();
        let provider = FixtureProvider {
            response: no_changes().to_string(),
        };
        let first = super::drive_claimed_case(&s, &provider, &lease)
            .await
            .unwrap();
        assert_eq!(
            first,
            Some(crate::runtime::worker::WorkerOutcome::Committed(
                lease.run_id
            ))
        );
        for _ in 0..2 {
            assert!(super::enqueue_pending_case(&s).await.unwrap());
            let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
            assert!(matches!(
                super::drive_claimed_case(&s, &provider, &lease)
                    .await
                    .unwrap(),
                Some(crate::runtime::worker::WorkerOutcome::Committed(_))
            ));
        }
        let state: String = sqlx::query_scalar("SELECT status FROM developments WHERE id=$1")
            .bind(plan.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(state, "rejected");
        let calls: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations u JOIN development_trial_cases c ON c.evaluation_run_id=u.run_id WHERE c.plan_id=$1 AND u.status='settled' AND u.dispatched_at IS NOT NULL").bind(plan.id).fetch_one(&pool).await.unwrap();
        assert_eq!(calls, 6);
        let after = s.snapshot().await.unwrap().unwrap();
        assert_eq!(after.persona, initial.persona);
        assert!(!super::enqueue_pending_case(&s).await.unwrap());
        let http = Server::start(&pool).await;
        let (status, headers, detail) = http
            .get(&format!("/developments/{}", plan.development_id))
            .await;
        assert_eq!(status, StatusCode::OK);
        let record = &detail["development"];
        assert_eq!(record["status"], "rejected");
        assert_eq!(
            headers["etag"],
            format!("\"{}\"", record["version"].as_i64().unwrap())
        );
        assert_eq!(record["validations"].as_array().unwrap().len(), 1);
        let assessment = &record["validations"][0]["assessment"];
        assert_eq!(assessment["origin"], "IndependentMeasurement");
        assert_eq!(assessment["counter_review"], "Unverified");
        assert_eq!(assessment["observations"].as_array().unwrap().len(), 2);
        assert!(
            assessment["observations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|o| o["outcome"] == "Unverified")
        );
        let rendered = detail.to_string();
        assert!(!rendered.contains("fixture criterion"));
        assert!(!rendered.contains("explicit-synthetic-trial"));
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn renewed_database_lease_survives_expired_caller_copy_for_both_sides(pool: PgPool) {
        let (s, mut lease, _, _) = work(&pool).await;
        // The accounted-call heartbeat only renews the DB row. This models the
        // caller copy after a long baseline without sleeping or weakening SQL.
        lease.expires_at = Utc::now() - Duration::seconds(1);
        let result = super::drive_claimed_case(
            &s,
            &FixtureProvider {
                response: no_changes().to_string(),
            },
            &lease,
        )
        .await
        .unwrap();
        assert_eq!(
            result,
            Some(crate::runtime::worker::WorkerOutcome::Committed(
                lease.run_id
            ))
        );
        let calls: i32 = sqlx::query_scalar("SELECT model_calls FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(calls, 2);
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn prebinding_does_not_authenticate_forged_event_body(pool: PgPool) {
        let (s, lease, _, _) = work(&pool).await;
        let work = s
            .claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        let mut context = crate::adapters::development_trial_context::prepare_case_context(
            &s,
            &lease,
            &work,
            &work.baseline,
            s.capabilities().await.unwrap(),
        )
        .await
        .unwrap();
        // Include an actual extra source event with a false body when the
        // normal retrieval window contains only the controlled trigger.
        let source = work.case.source_event_ids[0];
        let value: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
            .bind(source)
            .fetch_one(&pool)
            .await
            .unwrap();
        let mut event: crate::domain::history::Event = serde_json::from_value(value).unwrap();
        event.payload = json!({"text":"forged source body"});
        context.events.push(event);
        let allowance = crate::runtime::scheduling::TokenAllowance {
            input_upper_bound: 100,
            output_cap: 50,
        };
        let key = s
            .bind_development_trial_request(&lease, &work.baseline, &context, allowance)
            .await
            .unwrap();
        assert!(
            s.reserve_model_call(&lease, key, &context, allowance)
                .await
                .is_err(),
            "prebinding a source ID is not authentication of its body"
        );
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn prebinding_rejects_changed_goal_success_condition_before_dispatch(pool: PgPool) {
        let (s, lease, _, goal) = work(&pool).await;
        let work = s
            .claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        let mut context = crate::adapters::development_trial_context::prepare_case_context(
            &s,
            &lease,
            &work,
            &work.baseline,
            s.capabilities().await.unwrap(),
        )
        .await
        .unwrap();
        context
            .goals
            .iter_mut()
            .find(|g| g.id == goal)
            .unwrap()
            .success_criterion = "invented completed condition".into();
        let allowance = crate::runtime::scheduling::TokenAllowance {
            input_upper_bound: 100,
            output_cap: 50,
        };
        let key = s
            .bind_development_trial_request(&lease, &work.baseline, &context, allowance)
            .await
            .unwrap();
        assert!(
            s.reserve_model_call(&lease, key, &context, allowance)
                .await
                .is_err()
        );
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn prebinding_rejects_forged_capability_permission_before_dispatch(pool: PgPool) {
        let (s, lease, _, _) = work(&pool).await;
        let work = s
            .claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        let mut context = crate::adapters::development_trial_context::prepare_case_context(
            &s,
            &lease,
            &work,
            &work.baseline,
            s.capabilities().await.unwrap(),
        )
        .await
        .unwrap();
        context.capabilities[0].authorized = !context.capabilities[0].authorized;
        let allowance = crate::runtime::scheduling::TokenAllowance {
            input_upper_bound: 100,
            output_cap: 50,
        };
        let key = s
            .bind_development_trial_request(&lease, &work.baseline, &context, allowance)
            .await
            .unwrap();
        assert!(
            s.reserve_model_call(&lease, key, &context, allowance)
                .await
                .is_err(),
            "a prebound body cannot grant a capability"
        );
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn preparation_still_rejects_another_owner_or_generation(pool: PgPool) {
        let (s, lease, _, _) = work(&pool).await;
        let work = s
            .claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        for wrong_generation in [false, true] {
            let mut other = lease.clone();
            if wrong_generation {
                other.generation += 1;
            } else {
                other.owner_id = Uuid::new_v4();
            }
            assert!(
                crate::adapters::development_trial_context::prepare_case_context(
                    &s,
                    &other,
                    &work,
                    &work.baseline,
                    s.capabilities().await.unwrap()
                )
                .await
                .is_err()
            );
        }
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn lost_provider_result_stops_once_and_persists_missing_sides(pool: PgPool) {
        use crate::domain::cognition::ModelContext;
        use crate::runtime::model::{
            ModelCallMetadata, ModelError, ModelFailure, ModelProvider, ModelResult, ProviderMode,
        };
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct Lost(Arc<AtomicUsize>);
        #[async_trait::async_trait]
        impl ModelProvider for Lost {
            fn mode(&self) -> ProviderMode {
                ProviderMode::Fixture
            }
            fn token_allowance(
                &self,
                _: &ModelContext,
                limits: CallLimits,
            ) -> Result<crate::runtime::scheduling::TokenAllowance, ModelFailure> {
                Ok(crate::runtime::scheduling::TokenAllowance {
                    input_upper_bound: 1,
                    output_cap: limits.max_output_tokens,
                })
            }
            async fn decide(
                &self,
                _: &ModelContext,
                _: CallLimits,
                _: ModelCallMetadata,
            ) -> Result<ModelResult, ModelFailure> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Err(ModelFailure {
                    http: None,
                    error: ModelError::Transport,
                    usage: None,
                    request_sent: false,
                })
            }
        }
        let (s, lease, plan, _) = work(&pool).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let result = super::drive_claimed_case(&s, &Lost(calls.clone()), &lease)
            .await
            .unwrap();
        assert_eq!(
            result,
            Some(crate::runtime::worker::WorkerOutcome::Stopped(
                lease.run_id,
                crate::runtime::worker::StopReason::ModelFailure
            ))
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let attempts: Vec<String> = sqlx::query_scalar(
            "SELECT state FROM development_trial_attempts WHERE plan_id=$1 ORDER BY state",
        )
        .bind(plan.id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            attempts.iter().filter(|s| s.as_str() == "unknown").count(),
            1
        );
        assert_eq!(
            attempts.iter().filter(|s| s.as_str() == "not_run").count(),
            5
        );
        let usage: (i64,i64) = sqlx::query_as("SELECT count(*),sum(charged_tokens)::bigint FROM usage_reservations WHERE run_id=$1 AND status='settled'").bind(lease.run_id).fetch_one(&pool).await.unwrap();
        assert_eq!(usage, (1, 4097));
        assert!(!super::enqueue_pending_case(&s).await.unwrap());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn real_task_subject_stays_separate_from_all_evaluation_runs(pool: PgPool) {
        use crate::domain::cognition::ModelContext;
        use crate::runtime::model::{
            ModelCallMetadata, ModelFailure, ModelProvider, ModelResult, ProviderMode,
        };
        use std::sync::{Arc, Mutex};
        struct Capture(Arc<Mutex<Vec<ModelContext>>>);
        #[async_trait::async_trait]
        impl ModelProvider for Capture {
            fn mode(&self) -> ProviderMode {
                ProviderMode::Fixture
            }
            fn token_allowance(
                &self,
                context: &ModelContext,
                limits: CallLimits,
            ) -> Result<crate::runtime::scheduling::TokenAllowance, ModelFailure> {
                FixtureProvider {
                    response: no_changes().to_string(),
                }
                .token_allowance(context, limits)
            }
            async fn decide(
                &self,
                context: &ModelContext,
                limits: CallLimits,
                metadata: ModelCallMetadata,
            ) -> Result<ModelResult, ModelFailure> {
                self.0.lock().unwrap().push(context.clone());
                FixtureProvider {
                    response: no_changes().to_string(),
                }
                .decide(context, limits, metadata)
                .await
            }
        }
        let (s, lease, old_plan, _) = work(&pool).await;
        s.finish_run(&lease, crate::runtime::scheduling::RunOutcome::Cancelled)
            .await
            .unwrap();
        let original: Uuid = sqlx::query_scalar(
            "SELECT id FROM runs WHERE mode='foreground' ORDER BY created_at LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut d = draft(
            old_plan.draft.evidence_ids[0],
            DevelopmentTarget::TaskStrategy,
            json!({"scope":{"kind":"task","id":original},"description":"此原任务先检查来源"}),
        );
        d.evidence_ids = old_plan.draft.evidence_ids.clone();
        let proposed = propose(&s, &pool, &d).await;
        let mut cases = old_plan.cases.clone();
        for case in &mut cases {
            case.id = Uuid::new_v4();
        }
        let manifest: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM objects WHERE NOT blocked ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        let plan = s
            .freeze_development_trial_plan(
                "separate-task-plan",
                s.snapshot().await.unwrap().unwrap().expected(),
                &manifest,
                proposed.development_id,
                proposed.development_version,
                old_plan.evaluator_id,
                ValidationRoute::CrossScenarioHabit,
                &TrialPlanDraft {
                    cases,
                    deadline: Utc::now() + Duration::hours(1),
                },
            )
            .await
            .unwrap();
        let contexts = Arc::new(Mutex::new(vec![]));
        let provider = Capture(contexts.clone());
        for case in &plan.cases {
            let run = s
                .enqueue_development_trial_case(
                    &format!("separate-task-case:{}", case.id),
                    plan.id,
                    case.id,
                )
                .await
                .unwrap();
            assert_ne!(run, original);
            let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
            assert_eq!(lease.run_id, run);
            assert!(matches!(
                super::drive_claimed_case(&s, &provider, &lease)
                    .await
                    .unwrap(),
                Some(crate::runtime::worker::WorkerOutcome::Committed(_))
            ));
        }
        let contexts = contexts.lock().unwrap();
        assert_eq!(contexts.len(), 6);
        for pair in contexts.chunks_exact(2) {
            assert_eq!(pair[0].run_id, pair[1].run_id);
            assert_ne!(pair[0].run_id, original);
            assert_eq!(pair[0].current_goal_id, pair[1].current_goal_id);
            assert!(
                pair[0]
                    .development_context
                    .as_ref()
                    .unwrap()
                    .effective_task_strategy
                    .is_none()
            );
            let strategy = pair[1]
                .development_context
                .as_ref()
                .unwrap()
                .effective_task_strategy
                .as_ref()
                .unwrap();
            assert_eq!(strategy.scope, StrategyScope::Task(original));
            for context in pair {
                assert_eq!(
                    context.trial_binding.as_ref().unwrap().evaluation_run_id,
                    context.run_id
                );
                assert!(
                    !serde_json::to_string(context)
                        .unwrap()
                        .contains("fixture criterion")
                );
            }
        }
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn actual_worker_recovers_recorded_baseline_after_restart_without_repaying(pool: PgPool) {
        use crate::runtime::model::{ModelCallMetadata, ModelProvider};
        let (s, lease, plan, _) = work(&pool).await;
        let work = s
            .claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        let context = crate::adapters::development_trial_context::prepare_case_context(
            &s,
            &lease,
            &work,
            &work.baseline,
            s.capabilities().await.unwrap(),
        )
        .await
        .unwrap();
        let provider = FixtureProvider {
            response: no_changes().to_string(),
        };
        let limits = CallLimits::default();
        let allowance = provider.token_allowance(&context, limits).unwrap();
        let key = s
            .bind_development_trial_request(&lease, &work.baseline, &context, allowance)
            .await
            .unwrap();
        let (result, reservation) = crate::runtime::execution::accounted_call_with_request_key(
            &s,
            &lease,
            &context,
            key,
            allowance,
            limits,
            provider.mode(),
            provider.decide(
                &context,
                limits,
                ModelCallMetadata::Background {
                    work_id: lease.run_id,
                },
            ),
            |r| r.usage,
        )
        .await
        .unwrap();
        s.complete_development_trial_attempt(
            &lease,
            &work.baseline,
            reservation,
            &context,
            &result,
        )
        .await
        .unwrap();
        // Persisted baseline exists; the process and its in-memory controller
        // are gone before the trial side. Advance only this disposable lease.
        sqlx::query(
            "UPDATE runs SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
        )
        .bind(lease.run_id)
        .execute(&pool)
        .await
        .unwrap();
        let worker = crate::runtime::worker::Worker::new(
            s.clone(),
            std::sync::Arc::new(provider),
            std::sync::Arc::new(crate::adapters::web::HttpWebReader),
            std::sync::Arc::new(tokio::sync::Notify::new()),
        );
        let recovered = worker.run_once().await;
        assert!(
            recovered.is_ok(),
            "a durable partial case must not remain stuck at its old generation: {recovered:?}"
        );
        let status: String = sqlx::query_scalar("SELECT status FROM developments WHERE id=$1")
            .bind(plan.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "rejected");
        let calls: i64 =
            sqlx::query_scalar("SELECT count(*) FROM usage_reservations WHERE run_id=$1")
                .bind(lease.run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(calls, 1);
        let recorded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM development_trial_attempts WHERE plan_id=$1 AND state='recorded'",
        )
        .bind(plan.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let not_run: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM development_trial_attempts WHERE plan_id=$1 AND state='not_run'",
        )
        .bind(plan.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((recorded, not_run), (1, 5));
    }

    async fn revoke_one_permission_without_changing_version(
        pool: &PgPool,
        s: &Store,
        context: &crate::domain::cognition::ModelContext,
    ) {
        let mut runtime = s.snapshot().await.unwrap().unwrap().runtime_config;
        let id = &context
            .capabilities
            .iter()
            .find(|c| c.authorized)
            .unwrap()
            .id;
        runtime["capabilities"]["enabled"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v.as_str() != Some(id.as_str()));
        // Isolate the permissions gate from the existing config-version fence.
        // A real settings update also increments the version and is refused by
        // the original lease validator before this new check is reached.
        sqlx::query("UPDATE individuals SET runtime_config=$1 WHERE id=$2")
            .bind(runtime)
            .bind(context.individual_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn dispatch_rechecks_actual_permissions_after_a_valid_reservation(pool: PgPool) {
        let (s, lease, _, _) = work(&pool).await;
        let work = s
            .claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        let context = crate::adapters::development_trial_context::prepare_case_context(
            &s,
            &lease,
            &work,
            &work.baseline,
            s.capabilities().await.unwrap(),
        )
        .await
        .unwrap();
        let allowance = crate::runtime::scheduling::TokenAllowance {
            input_upper_bound: 100,
            output_cap: 50,
        };
        let key = s
            .bind_development_trial_request(&lease, &work.baseline, &context, allowance)
            .await
            .unwrap();
        let reservation = s
            .reserve_model_call(&lease, key, &context, allowance)
            .await
            .unwrap();
        revoke_one_permission_without_changing_version(&pool, &s, &context).await;
        let result = s
            .dispatch_model_call_with_context(
                &lease,
                reservation.id,
                &context,
                crate::runtime::model::ProviderMode::Fixture,
            )
            .await;
        assert!(matches!(
            result,
            Err(crate::adapters::store::StoreError::Invalid(
                "trial capabilities do not match current registry and permissions"
            ))
        ));
        let sent: Option<chrono::DateTime<Utc>> =
            sqlx::query_scalar("SELECT dispatched_at FROM usage_reservations WHERE id=$1")
                .bind(reservation.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let calls: i32 = sqlx::query_scalar("SELECT model_calls FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(sent.is_none());
        assert_eq!(calls, 0);
        s.finish_run(&lease, crate::runtime::scheduling::RunOutcome::Failed)
            .await
            .unwrap();
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn result_rechecks_actual_permissions_and_preserves_settled_usage(pool: PgPool) {
        use crate::runtime::model::{ModelCallMetadata, ModelProvider};
        let (s, lease, _, _) = work(&pool).await;
        let work = s
            .claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        let context = crate::adapters::development_trial_context::prepare_case_context(
            &s,
            &lease,
            &work,
            &work.baseline,
            s.capabilities().await.unwrap(),
        )
        .await
        .unwrap();
        let provider = FixtureProvider {
            response: no_changes().to_string(),
        };
        let limits = CallLimits::default();
        let allowance = provider.token_allowance(&context, limits).unwrap();
        let key = s
            .bind_development_trial_request(&lease, &work.baseline, &context, allowance)
            .await
            .unwrap();
        let (result, reservation) = crate::runtime::execution::accounted_call_with_request_key(
            &s,
            &lease,
            &context,
            key,
            allowance,
            limits,
            provider.mode(),
            provider.decide(
                &context,
                limits,
                ModelCallMetadata::Background {
                    work_id: lease.run_id,
                },
            ),
            |r| r.usage,
        )
        .await
        .unwrap();
        let before_usage: (String, i64, bool) = sqlx::query_as(
            "SELECT status,charged_tokens,usage_unknown FROM usage_reservations WHERE id=$1",
        )
        .bind(reservation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            before_usage,
            (
                "settled".into(),
                (allowance.input_upper_bound + u64::from(allowance.output_cap)) as i64,
                true
            )
        );
        revoke_one_permission_without_changing_version(&pool, &s, &context).await;
        let recorded = s
            .complete_development_trial_attempt(
                &lease,
                &work.baseline,
                reservation,
                &context,
                &result,
            )
            .await;
        assert!(matches!(
            recorded,
            Err(crate::adapters::store::StoreError::Invalid(
                "trial capabilities do not match current registry and permissions"
            ))
        ));
        let event: Option<Uuid> = sqlx::query_scalar(
            "SELECT result_event_id FROM development_trial_attempts WHERE id=$1",
        )
        .bind(work.baseline.attempt_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(event.is_none());
        let usage: (String, i64, bool) = sqlx::query_as(
            "SELECT status,charged_tokens,usage_unknown FROM usage_reservations WHERE id=$1",
        )
        .bind(reservation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(usage, before_usage);
    }
}
