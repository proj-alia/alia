use alia::{
    adapters::store::{Store, StoreError},
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::{
        execution::{execute_web_action, model_step},
        model::{CallLimits, FixtureProvider},
        scheduling::RunLease,
        web::*,
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Notify;
use uuid::Uuid;
struct Reader {
    calls: AtomicUsize,
    text: String,
    started: Notify,
    release: Notify,
    blocked: bool,
}
impl Reader {
    fn new(text: &str, blocked: bool) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            text: text.into(),
            started: Notify::new(),
            release: Notify::new(),
            blocked,
        }
    }
}
#[async_trait::async_trait]
impl WebReader for Reader {
    fn mode(&self) -> CapabilityMode {
        CapabilityMode::Fixture
    }
    async fn read(&self, target: &WebTarget) -> Result<WebDocument, WebFailure> {
        assert_eq!(target.host(), "allowed.example");
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        if self.blocked {
            self.release.notified().await;
        }
        Ok(WebDocument {
            media_type: "text/plain".into(),
            text: self.text.clone(),
            http_status: 200,
        })
    }
}
async fn setup(pool: &PgPool) -> Store {
    let store = Store::new(pool.clone());
    let seed = PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap();
    let mut config: Value =
        serde_json::from_slice(include_bytes!("../config/runtime.example.json")).unwrap();
    config["capabilities"]["allowed_web_hosts"] = json!(["allowed.example"]);
    let rules = RuntimeRules::validate(
        config,
        &["notes.write", "documents.read", "memory.search", WEB_ID],
    )
    .unwrap();
    store.initialize("create", &seed, &rules).await.unwrap();
    store
}
async fn plan(store: &Store, url: &str) -> (RunLease, Uuid) {
    store
        .submit_message(&Uuid::new_v4().to_string(), "读取允许的公开资料", None)
        .await
        .unwrap();
    let mut lease = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let caps = store.capabilities().await.unwrap();
    let p = json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[{"capability_id":WEB_ID,"input":{"url":url}}],"development":null,"brief_reason":"获得实际资料后再判断"});
    let ready = model_step(
        store,
        &FixtureProvider {
            response: p.to_string(),
        },
        &lease,
        caps.clone(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    let receipt = store.commit_decision(&lease, &ready, &caps).await.unwrap();
    lease.state_version = receipt.state_version;
    (lease, receipt.action_ids[0])
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn web_observation_is_external_paged_and_preserves_complete_body_and_sources(pool: PgPool) {
    let store = setup(&pool).await;
    let (mut lease, action) = plan(&store, "https://allowed.example/page").await;
    let text = "资料".repeat(2500);
    let reader = Reader::new(&text, false);
    let receipt = execute_web_action(&store, &reader, &lease, action)
        .await
        .unwrap();
    assert_eq!(receipt.status, "succeeded");
    assert!(receipt.executed);
    lease.state_version = receipt.state_version;
    let payload: Value = sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(receipt.result_event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payload["execution_mode"], "fixture");
    assert_eq!(
        payload["output"]["text"].as_str().unwrap().chars().count(),
        4096
    );
    assert_eq!(payload["output"]["next_offset"], 4096);
    assert!(payload["output"]["citation"].is_null());
    let doc: Uuid = serde_json::from_value(payload["output"]["document_id"].clone()).unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT body FROM documents WHERE event_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap(),
        text
    );
    let kind: String = sqlx::query_scalar("SELECT source_kind FROM events WHERE id=$1")
        .bind(receipt.result_event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kind, "external_document");
    let context = store
        .model_context(lease.run_id, store.capabilities().await.unwrap())
        .await
        .unwrap();
    assert!(
        context
            .events
            .iter()
            .any(|e| e.id == receipt.result_event_id)
    );
    let p = json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[{"capability_id":"documents.read","input":{"document_id":doc,"offset":4096,"max_chars":4096}}],"development":null,"brief_reason":"继续读完整资料"});
    let caps = store.capabilities().await.unwrap();
    let ready = model_step(
        &store,
        &FixtureProvider {
            response: p.to_string(),
        },
        &lease,
        caps.clone(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    let next = store.commit_decision(&lease, &ready, &caps).await.unwrap();
    lease.state_version = next.state_version;
    let read = store
        .execute_local_action(&lease, next.action_ids[0])
        .await
        .unwrap();
    let tail: Value = sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(read.result_event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        tail["output"]["text"].as_str().unwrap().chars().count(),
        904
    );
    let dependent: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM derivation_edges WHERE source_id=$1 AND target_id=$2)",
    )
    .bind(doc)
    .bind(read.result_event_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(dependent);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn concurrent_read_is_dispatched_once_and_replay_never_fetches_again(pool: PgPool) {
    let store = setup(&pool).await;
    let (lease, action) = plan(&store, "https://allowed.example/page").await;
    let reader = Arc::new(Reader::new("一次实际观察", true));
    let task = tokio::spawn({
        let store = store.clone();
        let lease = lease.clone();
        let reader = reader.clone();
        async move { execute_web_action(&store, reader.as_ref(), &lease, action).await }
    });
    reader.started.notified().await;
    assert!(matches!(
        execute_web_action(&store, reader.as_ref(), &lease, action).await,
        Err(StoreError::Invalid("web action already dispatched"))
    ));
    reader.release.notify_one();
    let receipt = task.await.unwrap().unwrap();
    assert_eq!(
        execute_web_action(&store, reader.as_ref(), &lease, action)
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(reader.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT tool_calls FROM runs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn unauthorized_url_rejection_is_a_nonexecuted_receipt_before_any_network(pool: PgPool) {
    let store = setup(&pool).await;
    let (lease, action) = plan(&store, "https://other.example/page").await;
    let reader = Reader::new("must not read", false);
    let receipt = execute_web_action(&store, &reader, &lease, action)
        .await
        .unwrap();
    assert_eq!(receipt.status, "failed");
    assert!(!receipt.executed);
    assert_eq!(reader.calls.load(Ordering::SeqCst), 0);
    let payload: Value = sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(receipt.result_event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payload["output"]["error"], "host_denied");
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT tool_calls FROM runs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn permission_and_call_limit_changes_prevent_dispatch(pool: PgPool) {
    let store = setup(&pool).await;
    let (lease, action) = plan(&store, "https://allowed.example/page").await;
    let reader = Reader::new("must not read", false);
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{capabilities,enabled}','[]')").execute(&pool).await.unwrap();
    assert!(matches!(
        execute_web_action(&store, &reader, &lease, action).await,
        Err(StoreError::Proposal(_))
    ));
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{capabilities,enabled}',$1)").bind(json!([WEB_ID])).execute(&pool).await.unwrap();
    sqlx::query("UPDATE runs SET tool_calls=3")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        execute_web_action(&store, &reader, &lease, action).await,
        Err(StoreError::CallLimit)
    ));
    assert_eq!(reader.calls.load(Ordering::SeqCst), 0);
    sqlx::query("UPDATE runs SET tool_calls=0")
        .execute(&pool)
        .await
        .unwrap();
    store
        .submit_message("new", "新输入优先", None)
        .await
        .unwrap();
    assert!(matches!(
        execute_web_action(&store, &reader, &lease, action).await,
        Err(StoreError::LeaseLost)
    ));
    assert_eq!(reader.calls.load(Ordering::SeqCst), 0);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn privacy_change_while_reading_discards_body_and_recovery_does_not_replay(pool: PgPool) {
    let store = setup(&pool).await;
    let (lease, action) = plan(&store, "https://allowed.example/page").await;
    let reader = Arc::new(Reader::new("不得保存的资料", true));
    let task = tokio::spawn({
        let store = store.clone();
        let lease = lease.clone();
        let reader = reader.clone();
        async move { execute_web_action(&store, reader.as_ref(), &lease, action).await }
    });
    reader.started.notified().await;
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    reader.release.notify_one();
    assert!(matches!(task.await.unwrap(), Err(StoreError::LeaseLost)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE kind='tool.result'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert!(store.claim_run(Uuid::new_v4()).await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM actions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "unknown"
    );
    assert_eq!(reader.calls.load(Ordering::SeqCst), 1);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn killed_read_keeps_attempt_count_and_never_automatically_replays(pool: PgPool) {
    let store = setup(&pool).await;
    let (lease, action) = plan(&store, "https://allowed.example/page").await;
    let reader = Arc::new(Reader::new("never returns", true));
    let task = tokio::spawn({
        let store = store.clone();
        let lease = lease.clone();
        let reader = reader.clone();
        async move { execute_web_action(&store, reader.as_ref(), &lease, action).await }
    });
    reader.started.notified().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    sqlx::query("UPDATE runs SET lease_expires_at=clock_timestamp()-interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.claim_run(Uuid::new_v4()).await.unwrap().is_none());
    let row: (String, String, i32) = sqlx::query_as(
        "SELECT a.status,r.status,r.tool_calls FROM actions a JOIN runs r ON r.id=a.run_id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, ("unknown".into(), "failed".into(), 1));
    assert!(
        execute_web_action(&store, reader.as_ref(), &lease, action)
            .await
            .is_err()
    );
    assert_eq!(reader.calls.load(Ordering::SeqCst), 1);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn invalid_adapter_body_is_failed_without_partial_artifact(pool: PgPool) {
    let store = setup(&pool).await;
    let (lease, action) = plan(&store, "https://allowed.example/page").await;
    let reader = Reader::new(&"a".repeat(MAX_WEB_BYTES + 1), false);
    let receipt = execute_web_action(&store, &reader, &lease, action)
        .await
        .unwrap();
    assert_eq!(receipt.status, "failed");
    assert!(receipt.executed);
    let payload: Value = sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(receipt.result_event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payload["output"]["error"], "too_large");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn receipt_storage_failure_does_not_replay_network_or_leave_document_body(pool: PgPool) {
    let store = setup(&pool).await;
    let (lease, action) = plan(&store, "https://allowed.example/page").await;
    let reader = Reader::new("读取成功但提交失败的内容", false);
    sqlx::raw_sql("CREATE FUNCTION fail_web_result() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind='tool.result' THEN RAISE EXCEPTION 'injected result failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_web_result BEFORE INSERT ON events FOR EACH ROW EXECUTE FUNCTION fail_web_result();").execute(&pool).await.unwrap();
    assert!(
        execute_web_action(&store, &reader, &lease, action)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT tool_calls FROM runs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    sqlx::query("DROP TRIGGER fail_web_result ON events")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        execute_web_action(&store, &reader, &lease, action)
            .await
            .is_err()
    );
    assert_eq!(reader.calls.load(Ordering::SeqCst), 1);
}
