//! Actual PostgreSQL orchestration with explicit scripted inference, never live claims.
use alia::{
    adapters::{store::Store, web::HttpWebReader},
    domain::{
        cognition::{DecisionProposal, ModelContext, RunMode},
        completion::EXCERPT_CRITERION,
        config::{PersonaSeed, RuntimeRules},
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        execution::model_step,
        model::*,
        scheduling::TokenAllowance,
        worker::{RetryCause, StopReason, Worker, WorkerOutcome},
    },
};
use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Notify, watch};
use uuid::Uuid;
const BODY: &str = "甲乙丙丁🙂戊己庚辛壬癸。";
fn proposal() -> Value {
    json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"按实际观察推进"})
}
fn reply() -> Value {
    let mut p = proposal();
    p["reply"] = json!("收到。");
    p
}
type Script = dyn Fn(&ModelContext, usize) -> Result<Value, ModelError> + Send + Sync;
struct Scripted {
    script: Box<Script>,
    contexts: Mutex<Vec<ModelContext>>,
}
impl Scripted {
    fn new(
        f: impl Fn(&ModelContext, usize) -> Result<Value, ModelError> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            script: Box::new(f),
            contexts: Mutex::new(vec![]),
        })
    }
    fn count(&self) -> usize {
        self.contexts.lock().unwrap().len()
    }
}
fn result(p: Value) -> Result<ModelResult, ModelFailure> {
    Ok(ModelResult {
        http: None,
        proposal: DecisionProposal::parse(p.to_string().as_bytes())
            .map_err(|e| ModelFailure::before_request(e.into()))?,
        usage: None,
        provider_model: "scripted-fixture".into(),
        system_fingerprint: None,
        provider_mode: ProviderMode::Fixture,
    })
}
#[async_trait]
impl ModelProvider for Scripted {
    fn mode(&self) -> ProviderMode {
        ProviderMode::Fixture
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
    async fn decide(
        &self,
        c: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        let n = {
            let mut seen = self.contexts.lock().unwrap();
            let n = seen.len();
            seen.push(c.clone());
            n
        };
        result((self.script)(c, n).map_err(ModelFailure::before_request)?)
    }
}
fn worker(s: &Store, p: Arc<dyn ModelProvider>) -> Worker {
    Worker::new(
        s.clone(),
        p,
        Arc::new(HttpWebReader),
        Arc::new(Notify::new()),
    )
}
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    let mut rules: Value =
        serde_json::from_str(include_str!("../config/runtime.example.json")).unwrap();
    rules["contact"]["quiet_start"] = json!("00:00");
    rules["contact"]["quiet_end"] = json!("00:00");
    s.initialize(
        "create",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap(),
        &RuntimeRules::validate(rules, IMPLEMENTED_CAPABILITIES).unwrap(),
    )
    .await
    .unwrap();
    s
}
async fn count(pool: &PgPool, q: &str) -> i64 {
    sqlx::query_scalar(q).fetch_one(pool).await.unwrap()
}
async fn expire(pool: &PgPool) {
    sqlx::query("UPDATE runs SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE status='running'").execute(pool).await.unwrap();
}
async fn create_goal(s: &Store) -> Uuid {
    let m = s
        .submit_message(&Uuid::new_v4().to_string(), "请探索资料中的问题", None)
        .await
        .unwrap();
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"create","origin_type":"event","origin_ids":[m.event_id],"description":"探索资料","success_criterion":"发现一个有根据的结果","next_step":"读取已有资料","stop_condition":"没有可用资料则等待","priority":50}]);
    let ready = model_step(
        s,
        &FixtureProvider {
            response: p.to_string(),
        },
        &l,
        s.capabilities().await.unwrap(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    s.commit_decision(&l, &ready, &s.capabilities().await.unwrap())
        .await
        .unwrap()
        .goal_ids[0]
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn worker_runs_goal_read_note_completion_and_contact_across_restart(pool: PgPool) {
    let s = setup(&pool).await;
    let identity = s.snapshot().await.unwrap().unwrap().id;
    let doc = s
        .import_document("doc", "Unicode资料", BODY, "owner")
        .await
        .unwrap()
        .event_id;
    let p = Scripted::new(move |c, _| {
        let mut p = proposal();
        if c.mode == RunMode::Foreground {
            p["goal_changes"] = json!([{"action":"create","origin_type":"event","origin_ids":[doc],"description":"保存摘录","success_criterion":EXCERPT_CRITERION,"next_step":"实际读取资料","stop_condition":"资料缺失时等待","priority":50,"completion":{"kind":"source_excerpt","document_id":doc,"start_char":0,"end_char":8}}]);
        } else {
            let g = &c.goals[0];
            if c.verified_goal_completions.contains(&g.id) {
                let evidence = c
                    .events
                    .iter()
                    .find(|e| e.payload["capability_id"] == "notes.write")
                    .unwrap()
                    .id;
                p["goal_changes"] = json!([{"action":"complete","goal_id":g.id,"reason":"摘录与真实结果已核验","evidence_ids":[evidence]}]);
                p["reply"] = json!("已经保存了指定摘录和来源。");
                p["reply_evidence_ids"] = json!([evidence]);
            } else if c
                .events
                .iter()
                .any(|e| e.payload["capability_id"] == "documents.read")
            {
                p["tool_intents"] = json!([{"capability_id":"notes.write","input":{"title":"摘录","text":BODY.chars().take(8).collect::<String>(),"citations":[{"document_id":doc,"start_char":0,"end_char":8}]}}]);
            } else {
                p["goal_changes"] = json!([{"action":"activate","goal_id":g.id,"reason":"开始读取","evidence_ids":[doc]}]);
                p["tool_intents"] = json!([{"capability_id":"documents.read","input":{"document_id":doc,"max_chars":8}}]);
            }
        }
        Ok(p)
    });
    s.submit_message("request", "为我保存摘录", None)
        .await
        .unwrap();
    assert!(matches!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Committed(_)
    ));
    // New Store and Worker, no in-memory chat history.
    let restored = Store::new(pool.clone());
    assert!(matches!(
        worker(&restored, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Committed(_)
    ));
    assert_eq!(restored.snapshot().await.unwrap().unwrap().id, identity);
    assert_eq!(p.count(), 4);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM goals WHERE status='completed'").await,
        1
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM notes").await, 1);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM actions WHERE status='succeeded'"
        )
        .await,
        2
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='goal.completion_verified'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM outgoing_messages WHERE mode='background' AND status='delivered'"
        )
        .await,
        1
    );
    assert_eq!(
        worker(&restored, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Idle
    );
    assert_eq!(p.count(), 4);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn malformed_output_repairs_once_with_persistent_feedback_and_accounting(pool: PgPool) {
    let s = setup(&pool).await;
    s.submit_message("one", "你好", None).await.unwrap();
    let p = Scripted::new(|c, n| {
        if n == 0 {
            Err(ModelError::Protocol)
        } else {
            assert!(c.events.iter().any(|e| e.kind == "run.retry_scheduled"));
            Ok(reply())
        }
    });
    assert!(matches!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Committed(_)
    ));
    assert_eq!(p.count(), 2);
    assert_eq!(
        count(&pool, "SELECT sum(model_calls)::bigint FROM runs").await,
        2
    );
    assert_eq!(
        count(&pool, "SELECT sum(repair_attempts)::bigint FROM runs").await,
        1
    );
    assert_eq!(count(&pool,"SELECT count(*) FROM usage_reservations WHERE usage_unknown AND charged_tokens=reserved_tokens").await,2);
    s.submit_message("two", "再试", None).await.unwrap();
    let p = Scripted::new(|_, _| Err(ModelError::Protocol));
    assert!(matches!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Stopped(_, StopReason::ModelRejected)
    ));
    assert_eq!(p.count(), 2); // remaining third call cannot purchase another repair
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn transient_failures_stop_at_three_without_fabricated_reply(pool: PgPool) {
    let s = setup(&pool).await;
    s.submit_message("one", "你好", None).await.unwrap();
    let p = Scripted::new(|_, _| Err(ModelError::Http(503)));
    assert!(matches!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Stopped(_, StopReason::ModelFailure)
    ));
    assert_eq!(p.count(), 3);
    assert_eq!(
        count(&pool, "SELECT sum(model_retries)::bigint FROM runs").await,
        2
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='run.stopped'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='message.sent'"
        )
        .await,
        0
    );
    assert_eq!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Idle
    );
    assert_eq!(p.count(), 3);
}
struct Unverified;
#[async_trait]
impl ModelProvider for Unverified {
    fn mode(&self) -> ProviderMode {
        ProviderMode::Live
    }
    async fn decide(
        &self,
        _: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        panic!("Unverified provider must not dispatch")
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn unavailable_or_unverified_model_waits_without_calls_or_regression(pool: PgPool) {
    let s = setup(&pool).await;
    create_goal(&s).await;
    let p: Arc<dyn ModelProvider> = Arc::new(Unverified);
    assert!(matches!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Stopped(_, StopReason::UnverifiedTokens)
    ));
    assert_eq!(count(&pool,"SELECT count(*) FROM goals WHERE status='waiting' AND waiting_reason='unverified_token_bound' AND no_progress_cycles=0 AND resume_at IS NULL").await,1);
    assert_eq!(
        count(
            &pool,
            "SELECT sum(model_calls)::bigint FROM runs WHERE mode='background'"
        )
        .await,
        0
    );
    assert_eq!(worker(&s, p).run_once().await.unwrap(), WorkerOutcome::Idle);
    s.submit_message("unavailable", "你好", None).await.unwrap();
    assert!(matches!(
        worker(&s, Arc::new(UnavailableProvider))
            .run_once()
            .await
            .unwrap(),
        WorkerOutcome::Stopped(_, StopReason::ModelUnavailable)
    ));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn budget_wait_resumes_only_after_window_and_never_while_frozen(pool: PgPool) {
    let s = setup(&pool).await;
    create_goal(&s).await;
    sqlx::query("UPDATE budget_windows SET charged_background=50000,charged_total=54097")
        .execute(&pool)
        .await
        .unwrap();
    let p = Scripted::new(|_, _| Ok(proposal()));
    assert!(matches!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Stopped(_, StopReason::Budget)
    ));
    assert_eq!(p.count(), 0);
    assert_eq!(count(&pool,"SELECT count(*) FROM goals WHERE status='waiting' AND waiting_reason='budget_unavailable' AND resume_at>clock_timestamp() AND no_progress_cycles=0").await,1);
    assert_eq!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Idle
    );
    // Simulated boundary exercises scheduling, not real-time observation.
    sqlx::query("UPDATE goals SET resume_at=clock_timestamp()-interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE individuals SET budget_frozen=true")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Idle
    );
    sqlx::query("UPDATE individuals SET budget_frozen=false")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE budget_windows SET starts_at=clock_timestamp()-interval '2 days',ends_at=clock_timestamp()-interval '1 day'").execute(&pool).await.unwrap();
    assert!(matches!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Committed(_)
    ));
    assert_eq!(p.count(), 1);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='goal.resource_resumed'"
        )
        .await,
        1
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn admitted_final_action_executes_before_call_limit_and_survives_restart(pool: PgPool) {
    let s = setup(&pool).await;
    s.submit_message("one", "保存笔记", None).await.unwrap();
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut p = proposal();
    p["tool_intents"] =
        json!([{"capability_id":"notes.write","input":{"title":"实际笔记","text":"应当只写一次"}}]);
    let ready = model_step(
        &s,
        &FixtureProvider {
            response: p.to_string(),
        },
        &l,
        s.capabilities().await.unwrap(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    s.commit_decision(&l, &ready, &s.capabilities().await.unwrap())
        .await
        .unwrap();
    // Model allowance is already exhausted when a crashed worker recovers.
    sqlx::query("UPDATE runs SET model_calls=3")
        .execute(&pool)
        .await
        .unwrap();
    expire(&pool).await;
    let p = Scripted::new(|_, _| panic!("must execute admitted action without another model call"));
    assert!(matches!(
        worker(&Store::new(pool.clone()), p.clone())
            .run_once()
            .await
            .unwrap(),
        WorkerOutcome::Stopped(_, StopReason::CallLimit)
    ));
    assert_eq!(count(&pool, "SELECT count(*) FROM notes").await, 1);
    assert_eq!(
        count(&pool, "SELECT sum(tool_calls)::bigint FROM runs").await,
        1
    );
    assert_eq!(count(&pool,"SELECT count(*) FROM events WHERE kind='run.stopped' AND jsonb_array_length(payload->'result_event_ids')=1").await,1);
    assert_eq!(worker(&s, p).run_once().await.unwrap(), WorkerOutcome::Idle);
    assert_eq!(count(&pool, "SELECT count(*) FROM notes").await, 1);
}
struct Slow {
    started: Arc<Notify>,
    release: Arc<Notify>,
}
#[async_trait]
impl ModelProvider for Slow {
    fn mode(&self) -> ProviderMode {
        ProviderMode::Fixture
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
    async fn decide(
        &self,
        _: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        self.started.notify_one();
        self.release.notified().await;
        result(reply())
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn new_foreground_input_fences_inflight_result_and_preserves_cost(pool: PgPool) {
    let s = setup(&pool).await;
    create_goal(&s).await;
    let p = Arc::new(Slow {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    });
    let w = worker(&s, p.clone());
    let task = tokio::spawn(async move { w.run_once().await.unwrap() });
    tokio::time::timeout(Duration::from_secs(5), p.started.notified())
        .await
        .unwrap();
    let m = s
        .submit_message("urgent", "先处理这条消息", None)
        .await
        .unwrap();
    p.release.notify_one();
    assert!(matches!(task.await.unwrap(), WorkerOutcome::Fenced(_)));
    let p = Scripted::new(move |c, _| {
        assert_eq!(c.current_event_id, m.event_id);
        Ok(reply())
    });
    assert!(matches!(
        worker(&s, p).run_once().await.unwrap(),
        WorkerOutcome::Committed(_)
    ));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM runs WHERE mode='background' AND status='stale'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM outgoing_messages WHERE mode='background'"
        )
        .await,
        0
    );
    assert_eq!(count(&pool,"SELECT count(*) FROM usage_reservations WHERE mode='background' AND charged_tokens=reserved_tokens").await,1);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn shutdown_cancels_inflight_wait_and_restart_settles_unknown_usage(pool: PgPool) {
    let s = setup(&pool).await;
    s.submit_message("one", "你好", None).await.unwrap();
    let p = Arc::new(Slow {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    });
    let (tx, rx) = watch::channel(false);
    let task = tokio::spawn(worker(&s, p.clone()).serve(rx));
    tokio::time::timeout(Duration::from_secs(5), p.started.notified())
        .await
        .unwrap();
    tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM usage_reservations WHERE status='dispatched'"
        )
        .await,
        1
    );
    expire(&pool).await;
    let p = Scripted::new(|_, _| Ok(reply()));
    assert!(matches!(
        worker(&Store::new(pool.clone()), p)
            .run_once()
            .await
            .unwrap(),
        WorkerOutcome::Committed(_)
    ));
    assert_eq!(
        count(&pool, "SELECT sum(model_calls)::bigint FROM runs").await,
        2
    );
    assert_eq!(count(&pool,"SELECT count(*) FROM usage_reservations WHERE usage_unknown AND charged_tokens=reserved_tokens").await,2);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='message.sent'"
        )
        .await,
        1
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn idle_worker_is_quiet_wakes_on_notification_and_shuts_down(pool: PgPool) {
    let s = setup(&pool).await;
    let p = Scripted::new(|_, _| Ok(reply()));
    let wake = Arc::new(Notify::new());
    let (tx, rx) = watch::channel(false);
    let before = count(&pool, "SELECT count(*) FROM events").await;
    let task = tokio::spawn(
        Worker::new(s.clone(), p.clone(), Arc::new(HttpWebReader), wake.clone()).serve(rx),
    );
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(count(&pool, "SELECT count(*) FROM events").await, before);
    assert_eq!(p.count(), 0);
    s.submit_message("one", "你好", None).await.unwrap();
    wake.notify_one();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if count(&pool, "SELECT count(*) FROM runs WHERE status='committed'").await == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.count(), 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn restart_preserves_repair_limit_feedback_and_wakes_for_lease_recovery(pool: PgPool) {
    let s = setup(&pool).await;
    assert_eq!(s.idle_delay().await.unwrap(), Duration::from_secs(3600));
    s.submit_message("one", "你好", None).await.unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let p = Scripted::new(|_, _| Err(ModelError::Protocol));
    assert!(
        model_step(
            &s,
            p.as_ref(),
            &lease,
            s.capabilities().await.unwrap(),
            CallLimits::default()
        )
        .await
        .is_err()
    );
    assert!(
        s.schedule_model_retry(&lease, RetryCause::Protocol)
            .await
            .unwrap()
            .unwrap()
            > lease.state_version
    );
    let delay = s.idle_delay().await.unwrap();
    assert!(delay <= Duration::from_secs(30));
    // Kill before retry; restored worker must still see the consumed repair.
    expire(&pool).await;
    assert!(matches!(
        worker(&Store::new(pool.clone()), p.clone())
            .run_once()
            .await
            .unwrap(),
        WorkerOutcome::Stopped(_, StopReason::ModelRejected)
    ));
    assert_eq!(p.count(), 2);
    assert!(
        p.contexts.lock().unwrap()[1]
            .events
            .iter()
            .any(|e| e.kind == "run.retry_scheduled")
    );
    assert_eq!(
        count(&pool, "SELECT sum(repair_attempts)::bigint FROM runs").await,
        1
    );
    let stopped: Value = sqlx::query_scalar(
        "SELECT payload FROM events WHERE kind='run.stopped' ORDER BY sequence DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stopped["validation_code"], "invalid_protocol");
    assert!(stopped["instruction"].as_str().unwrap().contains("JSON"));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn stop_transaction_failure_does_not_leave_partial_wait_or_stop_event(pool: PgPool) {
    let s = setup(&pool).await;
    create_goal(&s).await;
    s.tick_goals().await.unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    sqlx::query("CREATE FUNCTION reject_worker_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test storage failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_worker_outbox BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION reject_worker_outbox()").execute(&pool).await.unwrap();
    assert!(
        s.stop_cycle(&lease, StopReason::UnverifiedTokens)
            .await
            .is_err()
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='run.stopped'"
        )
        .await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM goals WHERE status='waiting'").await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM runs WHERE status='running'").await,
        1
    );
    sqlx::query("DROP TRIGGER reject_worker_outbox ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    s.stop_cycle(&lease, StopReason::UnverifiedTokens)
        .await
        .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='run.stopped'"
        )
        .await,
        1
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM goals WHERE status='waiting'").await,
        1
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn domain_feedback_repairs_without_committing_rejected_memory(pool: PgPool) {
    let s = setup(&pool).await;
    s.submit_message("feedback", "只记录实际来源", None)
        .await
        .unwrap();
    let p = Scripted::new(|c, n| {
        if n == 0 {
            let mut value = reply();
            value["memory_changes"] = json!([{"kind":"semantic","content":"REJECTED_PRIVATE_TEXT","scope":"general","source_event_ids":[Uuid::new_v4()],"supersedes":null}]);
            Ok(value)
        } else {
            let feedback = c
                .events
                .iter()
                .find(|e| e.kind == "run.retry_scheduled")
                .unwrap();
            assert_eq!(feedback.payload["validation_code"], "unknown_evidence");
            assert!(
                !serde_json::to_string(c)
                    .unwrap()
                    .contains("REJECTED_PRIVATE_TEXT")
            );
            Ok(reply())
        }
    });
    assert!(matches!(
        worker(&s, p.clone()).run_once().await.unwrap(),
        WorkerOutcome::Committed(_)
    ));
    assert_eq!(p.count(), 2);
    assert_eq!(count(&pool, "SELECT count(*) FROM memories").await, 0);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE payload::text LIKE '%REJECTED_PRIVATE_TEXT%'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM usage_reservations WHERE status='settled'"
        )
        .await,
        2
    );
    assert_eq!(
        count(&pool, "SELECT sum(repair_attempts)::bigint FROM runs").await,
        1
    );
}
