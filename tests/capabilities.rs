use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        config::{PersonaSeed, RuntimeRules},
        history::{MemoryKind, NewMemory},
    },
    runtime::{
        capability::LOCAL_CAPABILITIES,
        execution::model_step,
        model::{CallLimits, FixtureProvider},
        scheduling::RunLease,
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
#[test]
fn person_search_targets_are_mutually_exclusive_and_unicode_bounded() {
    use alia::runtime::capability::{Capability, LocalCapability};
    let search = LocalCapability::SearchMemory;
    for input in [
        json!({"query":"兴趣","person_name":"周青"}),
        json!({"query":"兴趣","person_id":Uuid::new_v4()}),
    ] {
        let proposal = json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[{"capability_id":"memory.search","input":input}],"development":null,"brief_reason":"检验实际提案schema与能力契约一致"});
        assert!(
            alia::domain::cognition::DecisionProposal::parse(proposal.to_string().as_bytes())
                .is_ok()
        );
    }
    for valid in [
        json!({"query":"兴趣"}),
        json!({"query":"兴趣","person_name":"周青"}),
        json!({"query":"兴趣","person_name":"甲".repeat(80)}),
    ] {
        assert!(search.validate_input(&valid).is_ok());
    }
    for invalid in [
        json!({"query":"兴趣","person_name":""}),
        json!({"query":"兴趣","person_name":" 周青"}),
        json!({"query":"兴趣","person_name":"周\n青"}),
        json!({"query":"兴趣","person_name":"甲".repeat(81)}),
        json!({"query":"兴趣","person_name":"周青","person_id":Uuid::new_v4()}),
    ] {
        assert!(search.validate_input(&invalid).is_err());
    }
}
async fn setup(pool: &PgPool) -> Store {
    let store = Store::new(pool.clone());
    let seed = PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap();
    let mut config: Value =
        serde_json::from_slice(include_bytes!("../config/runtime.example.json")).unwrap();
    config["capabilities"]["enabled"] = json!(LOCAL_CAPABILITIES);
    store
        .initialize(
            "create",
            &seed,
            &RuntimeRules::validate(config, LOCAL_CAPABILITIES).unwrap(),
        )
        .await
        .unwrap();
    store
}
async fn lease(store: &Store) -> RunLease {
    store
        .submit_message(
            &Uuid::new_v4().to_string(),
            "阅读资料并记下真正的发现",
            None,
        )
        .await
        .unwrap();
    store.claim_run(Uuid::new_v4()).await.unwrap().unwrap()
}
async fn intent(store: &Store, lease: &mut RunLease, tools: Value) -> Vec<Uuid> {
    let capabilities = store.local_capabilities().await.unwrap();
    assert!(capabilities.iter().all(|c| c.authorized && c.available));
    let p = json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":tools,"development":null,"brief_reason":"执行已注册的实际本地能力"});
    let ready = model_step(
        store,
        &FixtureProvider {
            response: p.to_string(),
        },
        lease,
        capabilities.clone(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    let receipt = store
        .commit_decision(lease, &ready, &capabilities)
        .await
        .unwrap();
    lease.state_version = receipt.state_version;
    receipt.action_ids
}
async fn result(pool: &PgPool, id: Uuid) -> Value {
    sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn imported_utf8_text_is_bounded_external_and_never_a_host_path(pool: PgPool) {
    let store = setup(&pool).await;
    let doc = store
        .import_document("doc", "原作背景", "甲🙂乙丙丁", "用户明确导入")
        .await
        .unwrap();
    assert_eq!(
        doc,
        store
            .import_document("doc", "原作背景", "甲🙂乙丙丁", "用户明确导入")
            .await
            .unwrap()
    );
    assert!(matches!(
        store
            .import_document("doc", "原作背景", "不同内容", "用户明确导入")
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    let mut lease = lease(&store).await;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    assert!(
        context
            .events
            .iter()
            .any(|e| e.id == doc.event_id && e.payload["title"] == "原作背景")
    );
    assert!(
        !serde_json::to_string(&context)
            .unwrap()
            .contains("甲🙂乙丙丁")
    );
    let actions=intent(&store,&mut lease,json!([
        {"capability_id":"documents.read","input":{"document_id":doc.event_id,"offset":1,"max_chars":2}},
        {"capability_id":"documents.read","input":{"document_id":doc.event_id,"offset":3,"max_chars":2}}
    ])).await;
    assert!(matches!(
        store.execute_local_action(&lease, actions[1]).await,
        Err(StoreError::Invalid("preceding action unfinished"))
    ));
    let first = store
        .execute_local_action(&lease, actions[0])
        .await
        .unwrap();
    let payload = result(&pool, first.result_event_id).await;
    assert_eq!(payload["output"]["text"], "🙂乙");
    assert_eq!(payload["output"]["next_offset"], 3);
    assert_eq!(payload["output"]["citation"]["start_char"], 1);
    assert_eq!(payload["output"]["citation"]["end_char"], 3);
    lease.state_version = first.state_version;
    let second = store
        .execute_local_action(&lease, actions[1])
        .await
        .unwrap();
    assert_eq!(
        result(&pool, second.result_event_id).await["output"]["text"],
        "丙丁"
    );
    lease.state_version = second.state_version;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    for id in [first.result_event_id, second.result_event_id] {
        let event = context.events.iter().find(|e| e.id == id).unwrap();
        assert!(matches!(
            event.source_kind,
            alia::domain::history::SourceKind::ExternalDocument
        ));
        assert!(context.input_manifest.contains(&id));
    }
    let old = store.snapshot().await.unwrap().unwrap().runtime_config;
    let mut p = json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"声称经历了文中场景"});
    p["memory_changes"] = json!([{"content":"我经历过故事中的事情","kind":"episodic","scope":"general","source_event_ids":[first.result_event_id],"supersedes":null}]);
    assert!(
        alia::domain::cognition::DecisionProposal::parse(p.to_string().as_bytes())
            .unwrap()
            .validate_context(&context)
            .is_err()
    );
    assert_eq!(store.snapshot().await.unwrap().unwrap().runtime_config, old);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn note_commit_is_once_across_concurrency_and_recovery_then_reply_uses_real_result(
    pool: PgPool,
) {
    let store = setup(&pool).await;
    let mut lease = lease(&store).await;
    let actions=intent(&store,&mut lease,json!([{"capability_id":"notes.write","input":{"title":"../../仅为标题","text":"已经核查的有限结论"}}])).await;
    let (a, b) = tokio::join!(
        store.execute_local_action(&lease, actions[0]),
        store.execute_local_action(&lease, actions[0])
    );
    let receipt = a.unwrap();
    assert_eq!(receipt, b.unwrap());
    assert!(receipt.executed);
    assert_eq!(receipt.status, "succeeded");
    let restored = Store::new(pool.clone());
    assert_eq!(
        restored
            .execute_local_action(&lease, actions[0])
            .await
            .unwrap(),
        receipt
    );
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM notes),(SELECT count(*) FROM events WHERE kind='tool.result')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1));
    let tools: i32 = sqlx::query_scalar("SELECT tool_calls FROM runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tools, 1);
    let body: String = sqlx::query_scalar("SELECT body FROM notes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(body, "已经核查的有限结论");
    lease.state_version = receipt.state_version;
    let caps = store.local_capabilities().await.unwrap();
    let p = json!({"schema_version":1,"reply":"笔记已写入。","reply_evidence_ids":[receipt.result_event_id],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"依据实际工具回执"});
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
    assert!(
        ready
            .context()
            .events
            .iter()
            .any(|e| e.id == receipt.result_event_id)
    );
    let decision = store.commit_decision(&lease, &ready, &caps).await.unwrap();
    assert_eq!(decision.run_status, "committed");
    assert_eq!(
        store
            .execute_local_action(&lease, actions[0])
            .await
            .unwrap(),
        receipt
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn invalid_arguments_and_missing_document_have_distinct_execution_receipts(pool: PgPool) {
    let store = setup(&pool).await;
    let mut lease = lease(&store).await;
    let actions=intent(&store,&mut lease,json!([
        {"capability_id":"notes.write","input":{"title":"说明","text":"   "}},
        {"capability_id":"documents.read","input":{"document_id":Uuid::new_v4(),"offset":0,"max_chars":10}},
        {"capability_id":"notes.write","input":{"title":"说明","text":"字".repeat(6000)}}
    ])).await;
    for (i, id) in actions.iter().enumerate() {
        let r = store.execute_local_action(&lease, *id).await.unwrap();
        assert_eq!(r.status, "failed");
        assert_eq!(r.executed, i == 1);
        assert_eq!(
            result(&pool, r.result_event_id).await["output"]["error"],
            if i == 1 {
                "document_unavailable"
            } else {
                "invalid_input"
            }
        );
        lease.state_version = r.state_version;
    }
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT tool_calls FROM runs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn lease_permissions_versions_and_call_limits_are_checked_before_note_write(pool: PgPool) {
    let store = setup(&pool).await;
    let mut lease = lease(&store).await;
    let actions = intent(
        &store,
        &mut lease,
        json!([{"capability_id":"notes.write","input":{"title":"待保存","text":"正文"}}]),
    )
    .await;
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{capabilities,enabled}','[]')").execute(&pool).await.unwrap();
    assert!(matches!(
        store.execute_local_action(&lease, actions[0]).await,
        Err(StoreError::Proposal(_))
    ));
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{capabilities,enabled}',$1)").bind(json!(LOCAL_CAPABILITIES)).execute(&pool).await.unwrap();
    sqlx::query("UPDATE actions SET capability_version='obsolete'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.execute_local_action(&lease, actions[0]).await,
        Err(StoreError::Proposal(_))
    ));
    sqlx::query("UPDATE actions SET capability_version='1'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET tool_calls=3")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.execute_local_action(&lease, actions[0]).await,
        Err(StoreError::CallLimit)
    ));
    sqlx::query(
        "UPDATE runs SET tool_calls=0,lease_expires_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        store.execute_local_action(&lease, actions[0]).await,
        Err(StoreError::LeaseLost)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let recovered = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert!(recovered.generation > lease.generation);
    let actual = store
        .execute_local_action(&recovered, actions[0])
        .await
        .unwrap();
    assert_eq!(actual.status, "succeeded");
    assert!(matches!(
        store.execute_local_action(&lease, actions[0]).await,
        Err(StoreError::StaleState)
    ));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn blocked_import_and_result_dependencies_cannot_be_replayed(pool: PgPool) {
    let store = setup(&pool).await;
    let doc = store
        .import_document("doc", "导入", "外部资料内容", "来源")
        .await
        .unwrap();
    let mut lease = lease(&store).await;
    let ids=intent(&store,&mut lease,json!([{"capability_id":"documents.read","input":{"document_id":doc.event_id,"offset":0,"max_chars":10}}])).await;
    let r = store.execute_local_action(&lease, ids[0]).await.unwrap();
    lease.state_version = r.state_version;
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(doc.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.execute_local_action(&lease, ids[0]).await,
        Err(StoreError::StaleState)
    ));
    assert!(store.model_context(lease.run_id, vec![]).await.is_err());
    assert!(matches!(
        store
            .import_document("doc", "导入", "外部资料内容", "来源")
            .await,
        Err(StoreError::StaleState)
    ));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn memory_search_has_scope_provenance_and_no_authority_to_change_policy(pool: PgPool) {
    let store = setup(&pool).await;
    let a = store.submit_message("a", "一般先研究", None).await.unwrap();
    let state = store.snapshot().await.unwrap().unwrap();
    store
        .record_memories(
            "memory",
            state.expected(),
            &[a.event_id],
            &[
                NewMemory {
                    content: "一般先研究".into(),
                    kind: MemoryKind::Semantic,
                    scope: "general".into(),
                    source_event_ids: vec![a.event_id],
                    supersedes: None,
                },
                NewMemory {
                    content: "当前先研究特殊项目".into(),
                    kind: MemoryKind::Semantic,
                    scope: format!("event:{}", a.event_id),
                    source_event_ids: vec![a.event_id],
                    supersedes: None,
                },
            ],
        )
        .await
        .unwrap();
    // Claim the oldest pending input and finish it so the next trigger has a different scope.
    let old = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    store
        .finish_run(&old, alia::runtime::scheduling::RunOutcome::Cancelled)
        .await
        .unwrap();
    let mut lease = lease(&store).await;
    let ids = intent(
        &store,
        &mut lease,
        json!([{"capability_id":"memory.search","input":{"query":"先研究","limit":5}}]),
    )
    .await;
    let r = store.execute_local_action(&lease, ids[0]).await.unwrap();
    let payload = result(&pool, r.result_event_id).await;
    let items = payload["output"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["scope"], "general");
    assert_eq!(items[0]["source_event_ids"], json!([a.event_id]));
    let edge: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM derivation_edges WHERE source_id=$1 AND target_id=$2)",
    )
    .bind(a.event_id)
    .bind(r.result_event_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(edge);
    assert_eq!(
        store.snapshot().await.unwrap().unwrap().runtime_config["capabilities"]["self_grant_permissions"],
        false
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn failure_to_save_receipt_rolls_back_note_and_call_count_then_retries_once(pool: PgPool) {
    let store = setup(&pool).await;
    let mut lease = lease(&store).await;
    let ids = intent(
        &store,
        &mut lease,
        json!([{"capability_id":"notes.write","input":{"title":"事务","text":"正文"}}]),
    )
    .await;
    sqlx::raw_sql("CREATE FUNCTION fail_result() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind='tool.result' THEN RAISE EXCEPTION 'injected receipt failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_result BEFORE INSERT ON events FOR EACH ROW EXECUTE FUNCTION fail_result();").execute(&pool).await.unwrap();
    assert!(store.execute_local_action(&lease, ids[0]).await.is_err());
    let snapshot:(i64,i32,String)=sqlx::query_as("SELECT (SELECT count(*) FROM notes),(SELECT tool_calls FROM runs),(SELECT status FROM actions)").fetch_one(&pool).await.unwrap();
    assert_eq!(snapshot, (0, 0, "planned".into()));
    assert_eq!(
        store.snapshot().await.unwrap().unwrap().state_version,
        lease.state_version
    );
    sqlx::query("DROP TRIGGER fail_result ON events")
        .execute(&pool)
        .await
        .unwrap();
    let receipt = store.execute_local_action(&lease, ids[0]).await.unwrap();
    assert_eq!(receipt.status, "succeeded");
    assert_eq!(
        store.execute_local_action(&lease, ids[0]).await.unwrap(),
        receipt
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}
