//! Durable orchestration; inference and tools execute outside storage transactions.
use super::{
    contact::ContactReceipt,
    execution::{StepError, execute_web_action, model_step},
    model::{CallLimits, ModelError, ModelProvider},
    scheduling::RunLease,
    web::WebReader,
};
use crate::adapters::store::{Store, StoreError};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Notify, watch};
use uuid::Uuid;

#[cfg(test)]
#[allow(dead_code)] // This module reuses a subset of the explicit engineering fixtures.
mod development_worker_tests {
    use crate as alia;
    include!("../../tests/support/development_context_fixture.rs");
    use crate::{
        domain::development_trials::*,
        runtime::{
            execution::model_step,
            model::{
                CallLimits, FixtureProvider, ModelCallMetadata, ModelFailure, ModelProvider,
                ModelResult, ModelStatus, ProviderMode,
            },
            scheduling::RunLease,
            worker::{Worker, WorkerOutcome},
        },
    };
    use std::sync::{Arc, Mutex};
    use tokio::sync::Notify;
    include!("../../tests/support/development_trial_run_fixture.rs");

    struct RecordingFixture {
        fixture: FixtureProvider,
        contexts: Mutex<Vec<crate::domain::cognition::ModelContext>>,
    }
    #[async_trait::async_trait]
    impl ModelProvider for RecordingFixture {
        fn mode(&self) -> ProviderMode {
            self.fixture.mode()
        }
        fn status(&self) -> ModelStatus {
            self.fixture.status()
        }
        fn token_allowance(
            &self,
            context: &crate::domain::cognition::ModelContext,
            limits: CallLimits,
        ) -> Result<crate::runtime::scheduling::TokenAllowance, ModelFailure> {
            self.fixture.token_allowance(context, limits)
        }
        async fn decide(
            &self,
            context: &crate::domain::cognition::ModelContext,
            limits: CallLimits,
            metadata: ModelCallMetadata,
        ) -> Result<ModelResult, ModelFailure> {
            self.contexts.lock().unwrap().push(context.clone());
            self.fixture.decide(context, limits, metadata).await
        }
    }

    // Native scripted coverage of the Live-mode transaction branch only.
    // This performs no network inference and is never growth/model evidence.
    struct ScriptedTrialProvider {
        counter_case: Uuid,
    }
    #[async_trait::async_trait]
    impl ModelProvider for ScriptedTrialProvider {
        fn mode(&self) -> ProviderMode {
            ProviderMode::Live
        }
        fn status(&self) -> ModelStatus {
            ModelStatus {
                mode: self.mode(),
                readiness: crate::runtime::model::ModelReadiness::Ready,
            }
        }
        fn token_allowance(
            &self,
            context: &crate::domain::cognition::ModelContext,
            limits: CallLimits,
        ) -> Result<crate::runtime::scheduling::TokenAllowance, ModelFailure> {
            FixtureProvider {
                response: String::new(),
            }
            .token_allowance(context, limits)
        }
        async fn decide(
            &self,
            context: &crate::domain::cognition::ModelContext,
            limits: CallLimits,
            metadata: ModelCallMetadata,
        ) -> Result<ModelResult, ModelFailure> {
            let binding = context.trial_binding.as_ref().expect("real trial binding");
            assert!(
                !serde_json::to_string(context)
                    .unwrap()
                    .contains("fixture criterion")
            );
            let mut proposal = no_changes();
            if binding.side == TrialSide::Trial || binding.case_id == self.counter_case {
                proposal["reply"] = json!("fixture criterion");
            }
            let mut result = FixtureProvider {
                response: proposal.to_string(),
            }
            .decide(context, limits, metadata)
            .await?;
            result.provider_mode = self.mode();
            result.provider_model = "scripted-native-live-branch-not-real-inference".into();
            Ok(result)
        }
    }

    fn worker(store: &Store) -> Worker {
        Worker::new(
            store.clone(),
            Arc::new(FixtureProvider {
                response: no_changes().to_string(),
            }),
            Arc::new(crate::adapters::web::HttpWebReader),
            Arc::new(Notify::new()),
        )
    }

    async fn expire_unstarted(pool: &PgPool, lease: &RunLease) {
        sqlx::query(
            "UPDATE runs SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
        )
        .bind(lease.run_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn worker_admits_and_finishes_three_trial_cases_without_normal_cognition_writes(
        pool: PgPool,
    ) {
        let (store, old_lease, plan, goal) = work(&pool).await;
        let original = store.snapshot().await.unwrap().unwrap();
        let original_goal: Value =
            sqlx::query_scalar("SELECT to_jsonb(g) FROM goals g WHERE id=$1")
                .bind(goal)
                .fetch_one(&pool)
                .await
                .unwrap();
        // Recover an unstarted leased run through the original scheduler. Each
        // following case is admitted by run_once itself, not a test-side driver.
        expire_unstarted(&pool, &old_lease).await;
        for _ in 0..3 {
            let restored = Store::new(pool.clone());
            assert!(matches!(
                worker(&restored).run_once().await.unwrap(),
                WorkerOutcome::Committed(_)
            ));
        }
        let (status, runs, calls): (String, i64, i64) = sqlx::query_as("SELECT d.status,count(DISTINCT c.evaluation_run_id),count(DISTINCT u.id) FROM developments d JOIN development_trial_plans p ON p.development_id=d.id JOIN development_trial_cases c ON c.plan_id=p.id LEFT JOIN usage_reservations u ON u.run_id=c.evaluation_run_id WHERE d.id=$1 GROUP BY d.status")
            .bind(plan.development_id).fetch_one(&pool).await.unwrap();
        assert_eq!((status.as_str(), runs, calls), ("rejected", 3, 6));
        let writes: i64 = sqlx::query_scalar("SELECT count(*) FROM events e JOIN development_trial_cases c ON c.evaluation_run_id=e.correlation_id WHERE c.plan_id=$1 AND e.kind IN ('message.sent','memory.created','goal.created','decision.accepted')")
            .bind(plan.id).fetch_one(&pool).await.unwrap();
        assert_eq!(writes, 0);
        assert_eq!(
            store.snapshot().await.unwrap().unwrap().persona,
            original.persona
        );
        let goal_after: Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM goals g WHERE id=$1")
            .bind(goal)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(goal_after, original_goal);
        assert_eq!(
            worker(&store).run_once().await.unwrap(),
            WorkerOutcome::Idle
        );
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn newly_received_foreground_input_precedes_queued_trial_and_inherits_no_overlay(
        pool: PgPool,
    ) {
        let (store, old_lease, _, _) = work(&pool).await;
        expire_unstarted(&pool, &old_lease).await;
        let incoming = store
            .submit_message("foreground-priority", "先处理这条新输入", None)
            .await
            .unwrap();
        let provider = Arc::new(RecordingFixture {
            fixture: FixtureProvider {
                response: no_changes().to_string(),
            },
            contexts: Mutex::new(vec![]),
        });
        let foreground_worker = Worker::new(
            store.clone(),
            provider.clone(),
            Arc::new(crate::adapters::web::HttpWebReader),
            Arc::new(Notify::new()),
        );
        let outcome = foreground_worker.run_once().await.unwrap();
        assert_eq!(outcome, WorkerOutcome::Committed(incoming.operation_id));
        let calls: i64 =
            sqlx::query_scalar("SELECT count(*) FROM usage_reservations WHERE run_id=$1")
                .bind(old_lease.run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(calls, 0);
        // Inspect the request actually polled by the provider, rather than
        // trying to build a new context after the run has already committed.
        let contexts = provider.contexts.lock().unwrap();
        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].run_id, incoming.operation_id);
        assert_eq!(
            contexts[0].mode,
            crate::domain::cognition::RunMode::Foreground
        );
        assert!(contexts[0].trial_binding.is_none());
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL"]
    async fn expired_queued_trial_closes_without_a_ready_provider_or_another_charge(pool: PgPool) {
        let (store, old_lease, plan, goal) = work(&pool).await;
        let original_goal: Value =
            sqlx::query_scalar("SELECT to_jsonb(g) FROM goals g WHERE id=$1")
                .bind(goal)
                .fetch_one(&pool)
                .await
                .unwrap();
        // Explicit synthetic elapsed measurement window. The candidate itself
        // remains within its longer deadline; this must reject, not expire it.
        let deadline: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT clock_timestamp()-interval '1 second'")
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("UPDATE development_trial_plans SET deadline=$2,plan=jsonb_set(plan,'{deadline}',to_jsonb($2::timestamptz)) WHERE id=$1")
            .bind(plan.id).bind(deadline).execute(&pool).await.unwrap();
        expire_unstarted(&pool, &old_lease).await;
        let unavailable = Worker::new(
            store.clone(),
            Arc::new(crate::runtime::model::UnavailableProvider),
            Arc::new(crate::adapters::web::HttpWebReader),
            Arc::new(Notify::new()),
        );
        assert_eq!(
            unavailable.run_once().await.unwrap(),
            WorkerOutcome::Maintained
        );
        let status: String = sqlx::query_scalar("SELECT status FROM developments WHERE id=$1")
            .bind(plan.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "rejected");
        let (run_status, calls, pending): (String, i64, i64) = sqlx::query_as("SELECT r.status,(SELECT count(*) FROM usage_reservations WHERE run_id=r.id),(SELECT count(*) FROM development_trial_attempts WHERE plan_id=$2 AND state<>'not_run') FROM runs r WHERE r.id=$1")
            .bind(old_lease.run_id).bind(plan.id).fetch_one(&pool).await.unwrap();
        assert_eq!((run_status.as_str(), calls, pending), ("cancelled", 0, 0));
        let after = store.snapshot().await.unwrap().unwrap();
        assert_eq!(unavailable.run_once().await.unwrap(), WorkerOutcome::Idle);
        assert_eq!(
            store.snapshot().await.unwrap().unwrap().state_version,
            after.state_version
        );
        let goal_after: Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM goals g WHERE id=$1")
            .bind(goal)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(goal_after, original_goal);
    }

    #[sqlx::test]
    #[ignore = "requires isolated PostgreSQL; scripted native mode, not real model/growth evidence"]
    async fn scripted_live_branch_accepts_only_after_all_cases_without_production_reply(
        pool: PgPool,
    ) {
        let (store, old_lease, plan, _) = work(&pool).await;
        let initial = store.snapshot().await.unwrap().unwrap();
        expire_unstarted(&pool, &old_lease).await;
        let counter_case = plan
            .cases
            .iter()
            .find(|c| c.role == TrialCaseRole::CounterSearch)
            .unwrap()
            .id;
        let scripted = Worker::new(
            store.clone(),
            Arc::new(ScriptedTrialProvider { counter_case }),
            Arc::new(crate::adapters::web::HttpWebReader),
            Arc::new(Notify::new()),
        );
        for completed in 0..3 {
            assert!(matches!(
                scripted.run_once().await.unwrap(),
                WorkerOutcome::Committed(_)
            ));
            let status: String = sqlx::query_scalar("SELECT status FROM developments WHERE id=$1")
                .bind(plan.development_id)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(status, if completed == 2 { "accepted" } else { "trial" });
        }
        let (value, version, head): (Value, i64, Option<Uuid>) = sqlx::query_as("SELECT t.value,t.version,t.current_revision_id FROM development_targets t JOIN developments d ON d.individual_id=t.individual_id AND d.target_id=t.id WHERE t.individual_id=$1 AND d.id=$2")
            .bind(initial.id).bind(plan.development_id).fetch_one(&pool).await.unwrap();
        assert_eq!(value, plan.draft.proposed_value);
        assert_eq!(version, plan.target.version + 1);
        assert!(head.is_some());
        assert_eq!(
            store.snapshot().await.unwrap().unwrap().persona,
            initial.persona
        );
        let (calls, results, observations): (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM usage_reservations u JOIN development_trial_cases c ON c.individual_id=u.individual_id AND c.evaluation_run_id=u.run_id WHERE c.plan_id=$1),(SELECT count(*) FROM development_trial_attempts WHERE plan_id=$1 AND state='recorded'),(SELECT count(*) FROM development_trial_cases WHERE plan_id=$1 AND observation_event_id IS NOT NULL)")
            .bind(plan.id).fetch_one(&pool).await.unwrap();
        assert_eq!((calls, results, observations), (6, 6, 3));
        let public_writes: i64 = sqlx::query_scalar("SELECT count(*) FROM events e JOIN development_trial_cases c ON c.evaluation_run_id=e.correlation_id WHERE c.plan_id=$1 AND e.kind IN ('message.sent','memory.created','goal.created','decision.accepted')")
            .bind(plan.id).fetch_one(&pool).await.unwrap();
        assert_eq!(public_writes, 0);
    }
}

/// Fixed diagnostic vocabulary; never includes rejected provider output.
#[derive(Clone)]
pub enum RetryCause {
    Transient,
    Protocol,
    Assessment(crate::domain::grounding::AssessmentError),
    Truncated,
    Proposal(crate::domain::cognition::ProposalError),
}
impl RetryCause {
    pub fn repair(&self) -> bool {
        !matches!(self, Self::Transient)
    }
    pub fn feedback(&self) -> (&'static str, &'static str) {
        match self {
            Self::Assessment(error) => (
                error.code(),
                "审查结果未符合目标或引用约束；审查重试应保留同一候选，引用只从对应目标的allowed_evidence_ids选择。",
            ),
            Self::Transient => (
                "model_transient_failure",
                "上次模型请求没有可提交结果。费用已按实际或未知用量记账。",
            ),
            Self::Protocol => (
                "invalid_protocol",
                "上次响应未满足响应协议。请返回规定的完整JSON对象。",
            ),
            Self::Truncated => (
                "truncated_output",
                "上次输出被截断。缩短内容，在输出额度内提交完整JSON；不要留下半份笔记。",
            ),
            Self::Proposal(error) => error.feedback(),
        }
    }
}

pub struct CycleProgress {
    pub model_calls: i32,
    pub model_limit: i32,
    pub actions: Vec<(Uuid, String)>,
    pub retry_delay: Duration,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Budget,
    UnverifiedTokens,
    ModelUnavailable,
    ModelRejected,
    Grounding,
    ModelFailure,
    CapabilityUnavailable,
    CallLimit,
    Deadline,
}
impl StopReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::Budget => "budget_unavailable",
            Self::UnverifiedTokens => "unverified_token_bound",
            Self::ModelUnavailable => "model_unavailable",
            Self::ModelRejected => "model_rejected",
            Self::Grounding => "decision_grounding_failed",
            Self::ModelFailure => "model_failure",
            Self::CapabilityUnavailable => "capability_unavailable",
            Self::CallLimit => "call_limit",
            Self::Deadline => "deadline",
        }
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::Budget => "预算不足或已冻结，等待资源恢复。",
            Self::UnverifiedTokens => "模型用量上界尚未验证，未发送请求。",
            Self::ModelUnavailable => "模型配置或凭证不可用。",
            Self::ModelRejected => "模型建议未通过验证。",
            Self::Grounding => "候选内容的依据审查未通过，未发布回复或写入记忆。",
            Self::ModelFailure => "模型调用未产生有效结果。",
            Self::CapabilityUnavailable => "所需能力不可用。",
            Self::CallLimit => "本轮调用次数已用完；仅保留实际执行结果。",
            Self::Deadline => "本轮执行时间已用完。",
        }
    }
    pub fn waits_for_resource(self) -> bool {
        matches!(
            self,
            Self::Budget
                | Self::UnverifiedTokens
                | Self::ModelUnavailable
                | Self::CapabilityUnavailable
        )
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum WorkerOutcome {
    Idle,
    Maintained,
    Committed(Uuid),
    Stopped(Uuid, StopReason),
    Fenced(Uuid),
}
pub struct Worker {
    pub store: Store,
    provider: Arc<dyn ModelProvider>,
    reader: Arc<dyn WebReader>,
    wake: Arc<Notify>,
    owner: Uuid,
}
impl Worker {
    pub fn new(
        store: Store,
        provider: Arc<dyn ModelProvider>,
        reader: Arc<dyn WebReader>,
        wake: Arc<Notify>,
    ) -> Self {
        Self {
            store,
            provider,
            reader,
            wake,
            owner: Uuid::new_v4(),
        }
    }
    async fn stop(
        &self,
        lease: &RunLease,
        reason: StopReason,
    ) -> Result<WorkerOutcome, StoreError> {
        self.stop_with_feedback(lease, reason, None).await
    }
    async fn stop_with_feedback(
        &self,
        lease: &RunLease,
        reason: StopReason,
        feedback: Option<RetryCause>,
    ) -> Result<WorkerOutcome, StoreError> {
        match self
            .store
            .stop_cycle_with_feedback(lease, reason, feedback)
            .await
        {
            Ok(()) => Ok(WorkerOutcome::Stopped(lease.run_id, reason)),
            Err(StoreError::LeaseLost | StoreError::StaleState) => {
                Ok(WorkerOutcome::Fenced(lease.run_id))
            }
            Err(e) => Err(e),
        }
    }
    async fn run_cycle(&self, mut lease: RunLease) -> Result<WorkerOutcome, StoreError> {
        loop {
            let progress = match self.store.cycle_progress(&lease).await {
                Ok(p) => p,
                Err(StoreError::LeaseLost | StoreError::StaleState) => {
                    return Ok(WorkerOutcome::Fenced(lease.run_id));
                }
                Err(e) => return Err(e),
            };
            for (id, capability) in progress.actions {
                let result = if capability == "web.read_allowed" {
                    execute_web_action(&self.store, self.reader.as_ref(), &lease, id).await
                } else {
                    self.store.execute_local_action(&lease, id).await
                };
                match result {
                    Ok(r) => lease.state_version = r.state_version,
                    Err(StoreError::LeaseLost | StoreError::StaleState) => {
                        return Ok(WorkerOutcome::Fenced(lease.run_id));
                    }
                    Err(StoreError::CallLimit) => {
                        return self.stop(&lease, StopReason::CallLimit).await;
                    }
                    Err(StoreError::Proposal(_)) => {
                        return self.stop(&lease, StopReason::CapabilityUnavailable).await;
                    }
                    Err(e) => return Err(e),
                }
            }
            // Tools admitted by the last model call still run within their own cap.
            if progress.model_calls >= progress.model_limit {
                return self.stop(&lease, StopReason::CallLimit).await;
            }
            if !progress.retry_delay.is_zero() {
                match self.store.remaining_cycle_time(&lease).await {
                    Ok(left) if left > progress.retry_delay => {
                        tokio::time::sleep(progress.retry_delay).await
                    }
                    Ok(_) => return self.stop(&lease, StopReason::Deadline).await,
                    Err(StoreError::LeaseLost | StoreError::StaleState) => {
                        return Ok(WorkerOutcome::Fenced(lease.run_id));
                    }
                    Err(e) => return Err(e),
                }
            }
            let registry = self.store.capabilities().await?;
            let review = if self.provider.supports_goal_review() {
                self.store
                    .prepare_goal_review(&lease, registry.clone())
                    .await
            } else {
                Ok(None)
            };
            let error = match review {
                Err(e) => StepError::Storage(e),
                Ok(Some(context)) => {
                    match super::review::review_step(
                        &self.store,
                        self.provider.as_ref(),
                        &lease,
                        context,
                        CallLimits::default(),
                    )
                    .await
                    {
                        Ok(prepared) => {
                            match self.store.commit_goal_review(&lease, &prepared).await {
                                Ok(receipt) => {
                                    lease.state_version = receipt.state_version;
                                    continue;
                                }
                                Err(e) => StepError::Storage(e),
                            }
                        }
                        Err(e) => e,
                    }
                }
                Ok(None) => {
                    let step = model_step(
                        &self.store,
                        self.provider.as_ref(),
                        &lease,
                        registry.clone(),
                        CallLimits::default(),
                    )
                    .await;
                    match step {
                        Ok(prepared) => match self
                            .store
                            .commit_decision(&lease, &prepared, &registry)
                            .await
                        {
                            Ok(receipt) => {
                                lease.state_version = receipt.state_version;
                                if receipt.run_status == "committed" {
                                    return Ok(WorkerOutcome::Committed(lease.run_id));
                                }
                                continue;
                            }
                            Err(e) => StepError::Storage(e),
                        },
                        Err(e) => e,
                    }
                }
            };
            let terminal_feedback = match &error {
                StepError::Storage(StoreError::Proposal(e)) => {
                    Some(RetryCause::Proposal(e.clone()))
                }
                StepError::Model(failure) => match &failure.error {
                    ModelError::Proposal(e) => Some(RetryCause::Proposal(e.clone())),
                    ModelError::Protocol => Some(RetryCause::Protocol),
                    ModelError::Assessment(error) => Some(RetryCause::Assessment(*error)),
                    ModelError::Truncated => Some(RetryCause::Truncated),
                    _ => None,
                },
                _ => None,
            };
            let reason = match error {
                StepError::AlreadyDispatched => return Ok(WorkerOutcome::Fenced(lease.run_id)),
                StepError::Storage(StoreError::LeaseLost | StoreError::StaleState) => {
                    return Ok(WorkerOutcome::Fenced(lease.run_id));
                }
                StepError::Storage(StoreError::BudgetUnavailable) => StopReason::Budget,
                StepError::Storage(StoreError::CallLimit) => StopReason::CallLimit,
                StepError::Storage(StoreError::Proposal(
                    crate::domain::cognition::ProposalError::Grounding,
                )) => StopReason::Grounding,
                StepError::Storage(StoreError::Proposal(_) | StoreError::Invalid(_)) => {
                    StopReason::ModelRejected
                }
                StepError::Storage(e) => return Err(e),
                StepError::Model(failure) => {
                    let repair = matches!(
                        failure.error,
                        ModelError::Proposal(_)
                            | ModelError::Protocol
                            | ModelError::Assessment(_)
                            | ModelError::Truncated
                    );
                    let transient = matches!(
                        failure.error,
                        ModelError::Transport | ModelError::Http(408 | 429 | 500..=599)
                    );
                    if repair || transient {
                        let cause = match &failure.error {
                            ModelError::Proposal(e) => RetryCause::Proposal(e.clone()),
                            ModelError::Truncated => RetryCause::Truncated,
                            ModelError::Protocol => RetryCause::Protocol,
                            ModelError::Assessment(error) => RetryCause::Assessment(*error),
                            _ => RetryCause::Transient,
                        };
                        match self.store.schedule_model_retry(&lease, cause).await {
                            Ok(Some(version)) => {
                                lease.state_version = version;
                                continue;
                            }
                            Ok(None) => {}
                            Err(StoreError::LeaseLost | StoreError::StaleState) => {
                                return Ok(WorkerOutcome::Fenced(lease.run_id));
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    match failure.error {
                        ModelError::Unavailable
                        | ModelError::Configuration
                        | ModelError::Http(401 | 403) => StopReason::ModelUnavailable,
                        ModelError::UnverifiedTokenBound => StopReason::UnverifiedTokens,
                        ModelError::Proposal(_)
                        | ModelError::Protocol
                        | ModelError::Assessment(_)
                        | ModelError::Truncated => StopReason::ModelRejected,
                        ModelError::Limits => StopReason::Budget,
                        _ => StopReason::ModelFailure,
                    }
                }
            };
            return self
                .stop_with_feedback(&lease, reason, terminal_feedback)
                .await;
        }
    }
    async fn contacts(&self) -> Result<bool, StoreError> {
        let mut changed = false;
        for id in self.store.pending_contacts().await? {
            if !matches!(
                self.store.deliver_proactive(id).await?,
                ContactReceipt::Deferred { .. }
            ) {
                changed = true;
            }
        }
        Ok(changed)
    }
    pub async fn run_once(&self) -> Result<WorkerOutcome, StoreError> {
        if self.store.snapshot().await?.is_none() {
            return Ok(WorkerOutcome::Idle);
        }
        self.store.prune_notifications().await?;
        let mut lease = self.store.claim_run(self.owner).await?;
        let mut maintained = false;
        if lease.is_none() {
            // Deadline cleanup needs no inference resource. The storage
            // transaction rechecks that another worker has not claimed work
            // before it changes global state or settles an orphaned request.
            maintained = !self
                .store
                .expire_development_trial_plans(20)
                .await?
                .is_empty();
            self.store.tick_goals().await?;
            if matches!(
                self.provider.status().readiness,
                super::model::ModelReadiness::Ready | super::model::ModelReadiness::Fixture
            ) {
                self.store.tick_discovery().await?;
                super::development_trials::enqueue_pending_case(&self.store).await?;
            }
            lease = self.store.claim_run(self.owner).await?;
        }
        let Some(lease) = lease else {
            let delivered = self.contacts().await?;
            return Ok(if maintained || delivered {
                WorkerOutcome::Maintained
            } else {
                WorkerOutcome::Idle
            });
        };
        let result = match super::development_trials::drive_claimed_case(
            &self.store,
            self.provider.as_ref(),
            &lease,
        )
        .await?
        {
            Some(outcome) => outcome,
            None => self.run_cycle(lease).await?,
        };
        if !matches!(result, WorkerOutcome::Fenced(_)) {
            self.store.tick_goals().await?;
            self.contacts().await?;
        }
        Ok(result)
    }
    /// On shutdown drop in-flight work. Persistent leases/accounting are recovered
    /// conservatively after expiry; cancellation cannot prove a remote request stopped.
    pub async fn serve(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            if *shutdown.borrow() {
                break;
            }
            let result = tokio::select! {biased;
                _=shutdown.changed()=>break,
                result=self.run_once()=>result,
            };
            let delay = match result {
                Ok(WorkerOutcome::Idle) => self
                    .store
                    .idle_delay()
                    .await
                    .unwrap_or(Duration::from_secs(60)),
                Ok(_) => {
                    tokio::task::yield_now().await;
                    continue;
                }
                Err(_) => {
                    tracing::error!(
                        code = "worker_storage_failure",
                        "Worker will retry after a bounded delay"
                    );
                    Duration::from_secs(5)
                }
            };
            tokio::select! {biased;
                _=shutdown.changed()=>break,
                _=self.wake.notified()=>{},
                _=tokio::time::sleep(delay)=>{},
            }
        }
    }
}
