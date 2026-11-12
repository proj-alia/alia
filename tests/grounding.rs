//! Synthetic assessments exercise accounting and gates; they do not establish semantic quality.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        cognition::*,
        config::{PersonaSeed, RuntimeRules},
        grounding::*,
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        execution::{StepError, model_step},
        model::*,
        scheduling::{RunLease, TokenAllowance},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::sync::{Notify, Semaphore};
use uuid::Uuid;
struct Reviewer {
    proposal: Value,
    verdict: Support,
    omit: bool,
    mode: ProviderMode,
    enabled: bool,
    gate: Option<(Arc<Notify>, Arc<Semaphore>)>,
    bad_reference_once: AtomicBool,
    author_calls: AtomicUsize,
    review_calls: AtomicUsize,
}
impl Reviewer {
    fn new(proposal: Value, verdict: Support) -> Self {
        Self {
            proposal,
            verdict,
            omit: false,
            mode: ProviderMode::Fixture,
            enabled: true,
            gate: None,
            bad_reference_once: AtomicBool::new(false),
            author_calls: AtomicUsize::new(0),
            review_calls: AtomicUsize::new(0),
        }
    }
}
#[async_trait::async_trait]
impl ModelProvider for Reviewer {
    fn mode(&self) -> ProviderMode {
        self.mode
    }
    fn token_allowance(
        &self,
        _: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        Ok(TokenAllowance {
            input_upper_bound: 1,
            output_cap: l.max_output_tokens,
        })
    }
    fn supports_decision_review(&self) -> bool {
        self.enabled
    }
    fn decision_review_allowance(
        &self,
        c: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        if !self.enabled {
            return Err(ModelFailure::before_request(ModelError::Unavailable));
        }
        assert!(c.decision_review.is_some());
        self.token_allowance(c, l)
    }
    async fn decide(
        &self,
        _: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        self.author_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ModelResult {
            proposal: DecisionProposal::parse(self.proposal.to_string().as_bytes()).unwrap(),
            usage: Some(TokenUsage {
                input: 1,
                output: 1,
                total: 2,
            }),
            provider_model: "synthetic-assessment-test".into(),
            provider_mode: self.mode,
            http: None,
            system_fingerprint: None,
        })
    }
    async fn review_decision(
        &self,
        c: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<DecisionReviewResult, ModelFailure> {
        self.review_calls.fetch_add(1, Ordering::SeqCst);
        if let Some((entered, gate)) = &self.gate {
            entered.notify_one();
            let _permit = gate.acquire().await.unwrap();
        }
        let p = &c.decision_review.as_ref().unwrap().proposal;
        let mut checks: Vec<ContentCheck> = if self.omit {
            vec![]
        } else {
            targets(p)
                .into_iter()
                .map(|target| {
                    let evidence_ids = match target {
                        ReviewTarget::Appraisal { index } => {
                            p.appraisal_changes[index].event_ids.clone()
                        }
                        ReviewTarget::Attitude { index } => {
                            p.attitude_changes[index].event_ids.clone()
                        }
                        ReviewTarget::Relationship { index } => {
                            p.relationship_changes[index].event_ids.clone()
                        }
                        ReviewTarget::Reply => p.reply_evidence_ids.clone(),
                        ReviewTarget::Memory { index } => {
                            p.memory_changes[index].source_event_ids.clone()
                        }
                        ReviewTarget::Autobiography { index } => {
                            p.autobiography_changes[index].review_references()
                        }
                        ReviewTarget::Belief { index } => {
                            let b = &p.belief_changes[index];
                            let mut ids: Vec<_> = b
                                .support_event_ids
                                .iter()
                                .chain(&b.counter_event_ids)
                                .copied()
                                .collect();
                            ids.sort();
                            ids.dedup();
                            ids
                        }
                        ReviewTarget::PersonAssertion { index } => {
                            vec![p.entity_changes[index].source.event_id]
                        }
                        ReviewTarget::Development => {
                            let d = p.development.as_ref().unwrap();
                            d.evidence_ids
                                .iter()
                                .chain(&d.counterevidence_ids)
                                .copied()
                                .collect()
                        }
                    };
                    ContentCheck {
                        target,
                        verdict: self.verdict,
                        reason: "显式合成审查意见，非语义验收".into(),
                        evidence_ids,
                    }
                })
                .collect()
        };
        if self.bad_reference_once.swap(false, Ordering::SeqCst) {
            checks[0].evidence_ids = vec![Uuid::new_v4()];
        }
        Ok(DecisionReviewResult {
            assessment: DecisionAssessment {
                schema_version: 1,
                checks,
            },
            usage: Some(TokenUsage {
                input: 1,
                output: 1,
                total: 2,
            }),
            provider_model: "synthetic-review-test".into(),
            provider_mode: self.mode,
        })
    }
}
async fn setup(pool: &PgPool) -> (Store, RunLease) {
    let s = Store::new(pool.clone());
    s.initialize(
        "init",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap(),
        &RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    s.submit_message("input", "陈辰说自己喜欢木工；没有提供性别。", None)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    (s, lease)
}
async fn candidate(s: &Store, lease: &RunLease) -> Value {
    let c = s
        .model_context(lease.run_id, s.local_capabilities().await.unwrap())
        .await
        .unwrap();
    json!({"schema_version":1,"reply":"陈辰提到喜欢木工。","reply_evidence_ids":[c.current_event_id],"memory_changes":[{"content":"陈辰提到喜欢木工。","kind":"semantic","scope":"general","source_event_ids":[c.current_event_id],"supersedes":null}],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"保存有来源陈述"})
}
async fn counts(pool: &PgPool, run: Uuid) -> (i32, i64, i64, i64) {
    sqlx::query_as("SELECT r.model_calls,(SELECT count(*) FROM memories),(SELECT count(*) FROM events WHERE kind='message.sent'),(SELECT count(*) FROM decision_reviews) FROM runs r WHERE r.id=$1").bind(run).fetch_one(pool).await.unwrap()
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn supported_content_requires_two_accounted_calls_and_a_source_bound_receipt(pool: PgPool) {
    let (s, lease) = setup(&pool).await;
    let p = candidate(&s, &lease).await;
    let provider = Reviewer::new(p, Support::Supported);
    let caps = s.local_capabilities().await.unwrap();
    let ready = model_step(&s, &provider, &lease, caps.clone(), CallLimits::default())
        .await
        .unwrap();
    let review = ready.grounding_review_id().unwrap();
    assert_eq!(counts(&pool, lease.run_id).await, (2, 0, 0, 1));
    let original = ready.context().execution_budget.as_ref().unwrap();
    assert_eq!(
        (
            original.model_calls_used,
            original.model_calls_remaining,
            original.model_call_limit
        ),
        (0, 3, 3)
    );
    assert!(
        ready
            .context()
            .goal_origin_catalog
            .items
            .iter()
            .any(|o| o.origin_type == "event"
                && o.origin_id == ready.context().current_event_id.to_string())
    );
    assert!(
        ready
            .context()
            .goal_origin_catalog
            .items
            .iter()
            .all(|o| o.label != "decision.reviewed")
    );
    let refreshed = s.model_context(lease.run_id, caps.clone()).await.unwrap();
    let balance = refreshed.execution_budget.unwrap();
    assert_eq!(
        (balance.model_calls_used, balance.model_calls_remaining),
        (2, 1)
    );
    assert_eq!(
        (balance.tool_calls_used, balance.tool_calls_remaining),
        (0, 3)
    );
    let receipt = s.commit_decision(&lease, &ready, &caps).await.unwrap();
    assert_eq!(counts(&pool, lease.run_id).await, (2, 1, 1, 1));
    let replay = s.commit_decision(&lease, &ready, &caps).await.unwrap();
    assert_eq!(receipt.decision_event_id, replay.decision_event_id);
    let linked: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM derivation_edges WHERE source_id=$1 AND target_id=$2)",
    )
    .bind(review)
    .bind(receipt.decision_event_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(linked);
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(review)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.commit_decision(&lease, &ready, &caps).await,
        Err(StoreError::StaleState)
    ));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn rejected_drafts_never_become_memory_or_reply_and_do_not_pollute_later_runs(pool: PgPool) {
    let (s, lease) = setup(&pool).await;
    let mut p = candidate(&s, &lease).await;
    p["reply"] = json!("陈辰是女性。");
    p["memory_changes"][0]["content"] = json!("陈辰是女性。");
    let provider = Reviewer::new(p, Support::Unsupported);
    let error = model_step(
        &s,
        &provider,
        &lease,
        s.local_capabilities().await.unwrap(),
        CallLimits::default(),
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(
        error,
        StepError::Storage(StoreError::Proposal(ProposalError::Grounding))
    ));
    assert_eq!(counts(&pool, lease.run_id).await, (2, 0, 0, 1));
    let c = s
        .model_context(lease.run_id, s.local_capabilities().await.unwrap())
        .await
        .unwrap();
    let feedback = c
        .events
        .iter()
        .find(|e| e.kind == "decision.reviewed")
        .unwrap();
    assert_eq!(feedback.payload["supported"], false);
    assert_eq!(
        feedback.payload["candidate_status"],
        "generated_draft_not_experience"
    );
    s.submit_message("next", "你当前有哪些可靠记录？", None)
        .await
        .unwrap();
    let next = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let c = s
        .model_context(next.run_id, s.local_capabilities().await.unwrap())
        .await
        .unwrap();
    assert!(c.memories.is_empty());
    assert!(!c.events.iter().any(|e| e.kind == "decision.reviewed"));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn missing_assessment_coverage_is_charged_protocol_failure_not_approval(pool: PgPool) {
    let (s, lease) = setup(&pool).await;
    let mut provider = Reviewer::new(candidate(&s, &lease).await, Support::Supported);
    provider.omit = true;
    assert!(matches!(
        model_step(
            &s,
            &provider,
            &lease,
            s.local_capabilities().await.unwrap(),
            CallLimits::default()
        )
        .await,
        Err(StepError::Model(ModelFailure {
            error: ModelError::Assessment(AssessmentError::Schema),
            ..
        }))
    ));
    assert_eq!(counts(&pool, lease.run_id).await, (3, 0, 0, 0));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn a_review_repair_preserves_the_author_candidate_and_uses_the_third_call(pool: PgPool) {
    let (s, lease) = setup(&pool).await;
    let provider = Reviewer::new(candidate(&s, &lease).await, Support::Supported);
    provider.bad_reference_once.store(true, Ordering::SeqCst);
    let caps = s.local_capabilities().await.unwrap();
    let before = s.snapshot().await.unwrap().unwrap().state_version;
    let ready = model_step(&s, &provider, &lease, caps.clone(), CallLimits::default())
        .await
        .unwrap();
    assert_eq!(provider.author_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider.review_calls.load(Ordering::SeqCst), 2);
    assert_eq!(counts(&pool, lease.run_id).await, (3, 0, 0, 1));
    assert_eq!(s.snapshot().await.unwrap().unwrap().state_version, before);
    let meta: (i32, i32) =
        sqlx::query_as("SELECT model_retries,repair_attempts FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(meta, (1, 1));
    let feedback:Value=sqlx::query_scalar("SELECT payload FROM events WHERE kind='decision.review_retry_scheduled' AND correlation_id=$1").bind(lease.run_id).fetch_one(&pool).await.unwrap();
    assert_eq!(feedback["validation_code"], "assessment_evidence");
    assert_eq!(feedback["role"], "content_reviewer");
    let charged: i64 = sqlx::query_scalar(
        "SELECT sum(charged_tokens)::bigint FROM usage_reservations WHERE run_id=$1",
    )
    .bind(lease.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(charged, 6);
    s.commit_decision(&lease, &ready, &caps).await.unwrap();
    assert_eq!(counts(&pool, lease.run_id).await, (3, 1, 1, 1));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn a_late_publication_failure_rolls_back_the_reviewed_memory_goal_and_reply(pool: PgPool) {
    let (s, lease) = setup(&pool).await;
    let mut p = candidate(&s, &lease).await;
    let event = p["reply_evidence_ids"][0].clone();
    p["reply"] = json!("已记住陈辰提到喜欢木工，并保留一个待研究的问题。");
    p["goal_changes"] = json!([{"action":"create","origins":[{"origin_type":"event","origin_id":event}],"description":"了解木工知识","success_criterion":"形成有来源的笔记","next_step":"查找可用资料","stop_condition":"无资料时等待","priority":50}]);
    let provider = Reviewer::new(p, Support::Supported);
    let caps = s.local_capabilities().await.unwrap();
    let ready = model_step(&s, &provider, &lease, caps.clone(), CallLimits::default())
        .await
        .unwrap();
    let before = s.snapshot().await.unwrap().unwrap().state_version;
    // Fail after memory, goal, outgoing message and relationship writes, at the
    // final public notification. Nothing claiming those writes may be visible.
    sqlx::query("CREATE FUNCTION fail_publication() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='message.created' THEN RAISE EXCEPTION 'synthetic publication failure'; END IF; RETURN NEW; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_publication BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION fail_publication()").execute(&pool).await.unwrap();
    let error = s.commit_decision(&lease, &ready, &caps).await.unwrap_err();
    assert!(
        matches!(error, StoreError::Database(sqlx::Error::Database(ref e)) if e.message()=="synthetic publication failure")
    );
    assert_eq!(counts(&pool, lease.run_id).await, (2, 0, 0, 1));
    assert_eq!(s.snapshot().await.unwrap().unwrap().state_version, before);
    for table in ["goals", "outgoing_messages", "decision_commits"] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    let notifications: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox o JOIN events e ON e.id=o.resource_id WHERE o.event_type='message.created' AND e.kind='message.sent' AND e.correlation_id=$1")
        .bind(lease.run_id).fetch_one(&pool).await.unwrap();
    assert_eq!(notifications, 0);
    sqlx::query("DROP TRIGGER fail_publication ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION fail_publication()")
        .execute(&pool)
        .await
        .unwrap();
    let receipt = s.commit_decision(&lease, &ready, &caps).await.unwrap();
    assert_eq!(receipt.memory_ids.len(), 1);
    assert_eq!(receipt.goal_ids.len(), 1);
    assert_eq!(receipt.message_status.as_deref(), Some("delivered"));
    assert_eq!(
        s.commit_decision(&lease, &ready, &caps).await.unwrap(),
        receipt
    );
    assert_eq!(counts(&pool, lease.run_id).await, (2, 1, 1, 1));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn new_input_during_review_fences_results_without_holding_a_database_transaction(
    pool: PgPool,
) {
    let (s, lease) = setup(&pool).await;
    let entered = Arc::new(Notify::new());
    let gate = Arc::new(Semaphore::new(0));
    let mut p = Reviewer::new(candidate(&s, &lease).await, Support::Supported);
    p.gate = Some((entered.clone(), gate.clone()));
    let worker = s.clone();
    let copy = lease.clone();
    let task = tokio::spawn(async move {
        model_step(
            &worker,
            &p,
            &copy,
            worker.local_capabilities().await.unwrap(),
            CallLimits::default(),
        )
        .await
    });
    if tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .is_err()
    {
        let early = task.await;
        panic!(
            "reviewer was not entered: {}",
            match early {
                Ok(Err(e)) => format!("{e}"),
                _ => "unexpected early outcome".into(),
            }
        );
    }
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        s.submit_message("new", "新输入使旧状态失效。", None),
    )
    .await
    .unwrap()
    .unwrap();
    gate.add_permits(1);
    assert!(matches!(
        task.await.unwrap(),
        Err(StepError::Storage(
            StoreError::LeaseLost | StoreError::StaleState
        ))
    ));
    assert_eq!(counts(&pool, lease.run_id).await, (2, 0, 0, 0));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn author_tool_answer_and_assessment_share_the_three_call_cap(pool: PgPool) {
    let (s, mut lease) = setup(&pool).await;
    let mut p = candidate(&s, &lease).await;
    p["reply"] = json!("");
    p["memory_changes"] = json!([]);
    p["reply_evidence_ids"] = json!([]);
    p["tool_intents"] = json!([{"capability_id":"memory.search","input":{"query":"木工"}}]);
    let provider = Reviewer::new(p, Support::Supported);
    let caps = s.local_capabilities().await.unwrap();
    let plan = model_step(&s, &provider, &lease, caps.clone(), CallLimits::default())
        .await
        .unwrap();
    assert!(plan.grounding_review_id().is_none());
    let r = s.commit_decision(&lease, &plan, &caps).await.unwrap();
    lease.state_version = r.state_version;
    let result = s
        .execute_local_action(&lease, r.action_ids[0])
        .await
        .unwrap();
    lease.state_version = result.state_version;
    let provider = Reviewer::new(candidate(&s, &lease).await, Support::Supported);
    let answer = model_step(&s, &provider, &lease, caps.clone(), CallLimits::default())
        .await
        .unwrap();
    s.commit_decision(&lease, &answer, &caps).await.unwrap();
    assert_eq!(counts(&pool, lease.run_id).await, (3, 1, 1, 1));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn missing_live_checker_and_exhausted_review_budget_fail_closed(pool: PgPool) {
    let (s, lease) = setup(&pool).await;
    let mut provider = Reviewer::new(candidate(&s, &lease).await, Support::Supported);
    provider.mode = ProviderMode::Live;
    provider.enabled = false;
    assert!(matches!(
        model_step(
            &s,
            &provider,
            &lease,
            s.local_capabilities().await.unwrap(),
            CallLimits::default()
        )
        .await,
        Err(StepError::Model(ModelFailure {
            error: ModelError::Unavailable,
            ..
        }))
    ));
    assert_eq!(counts(&pool, lease.run_id).await, (1, 0, 0, 0));
    // Two more paid author calls exhaust the cap; no fourth checker is admitted.
    let _ = model_step(
        &s,
        &provider,
        &lease,
        s.local_capabilities().await.unwrap(),
        CallLimits::default(),
    )
    .await;
    let provider = Reviewer::new(candidate(&s, &lease).await, Support::Supported);
    assert!(matches!(
        model_step(
            &s,
            &provider,
            &lease,
            s.local_capabilities().await.unwrap(),
            CallLimits::default()
        )
        .await,
        Err(StepError::Storage(StoreError::CallLimit))
    ));
    assert_eq!(counts(&pool, lease.run_id).await, (3, 0, 0, 0));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn review_accounting_tamper_cannot_authorize_a_commit(pool: PgPool) {
    let (s, lease) = setup(&pool).await;
    let p = Reviewer::new(candidate(&s, &lease).await, Support::Supported);
    let caps = s.local_capabilities().await.unwrap();
    let ready = model_step(&s, &p, &lease, caps.clone(), CallLimits::default())
        .await
        .unwrap();
    sqlx::query("UPDATE usage_reservations SET request_digest='tampered' WHERE id=(SELECT review_reservation_id FROM decision_reviews WHERE event_id=$1)").bind(ready.grounding_review_id().unwrap()).execute(&pool).await.unwrap();
    assert!(matches!(
        s.commit_decision(&lease, &ready, &caps).await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(counts(&pool, lease.run_id).await, (2, 0, 0, 1));
}
