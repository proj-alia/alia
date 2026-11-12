use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        cognition::{CapabilityInfo, RunMode},
        config::{PersonaSeed, RuntimeRules},
    },
    runtime::{
        execution::{PreparedDecision, model_step},
        model::{CallLimits, FixtureProvider},
        scheduling::{DecisionReceipt, RunLease},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let store = Store::new(pool.clone());
    let seed = PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap();
    let config = RuntimeRules::parse(
        include_bytes!("../config/runtime.example.json"),
        &[
            "memory.search",
            "documents.read",
            "notes.write",
            "web.read_allowed",
        ],
    )
    .unwrap();
    store.initialize("create", &seed, &config).await.unwrap();
    store
}
async fn input(store: &Store) -> RunLease {
    store
        .submit_message(
            &Uuid::new_v4().to_string(),
            "我一般先研究，但这次先做原型。",
            None,
        )
        .await
        .unwrap();
    store.claim_run(Uuid::new_v4()).await.unwrap().unwrap()
}
fn proposal() -> Value {
    json!({"schema_version":1,"reply":"这次先做原型。","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"本次局部要求"})
}
fn capability() -> CapabilityInfo {
    CapabilityInfo {
        id: "notes.write".into(),
        version: "test-1".into(),
        available: true,
        authorized: true,
        description: "显式测试能力，不是生产实现".into(),
    }
}
async fn prepare(
    store: &Store,
    lease: &RunLease,
    p: Value,
    capabilities: Vec<CapabilityInfo>,
) -> PreparedDecision {
    model_step(
        store,
        &FixtureProvider {
            response: p.to_string(),
        },
        lease,
        capabilities,
        CallLimits::default(),
    )
    .await
    .unwrap()
}
async fn plan(store: &Store, lease: &RunLease) -> (PreparedDecision, DecisionReceipt) {
    let mut p = proposal();
    p["reply"] = json!("");
    p["tool_intents"] =
        json!([{"capability_id":"notes.write","input":{"title":"学习笔记","text":"待写入的内容"}}]);
    let ready = prepare(store, lease, p, vec![capability()]).await;
    let receipt = store
        .commit_decision(lease, &ready, &[capability()])
        .await
        .unwrap();
    (ready, receipt)
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn atomic_memory_goal_reply_and_replay_survive_connection_recreation(
    options: sqlx::postgres::PgPoolOptions,
    connection: sqlx::postgres::PgConnectOptions,
) {
    let pool = options.connect_with(connection.clone()).await.unwrap();
    let store = setup(&pool).await;
    let lease = input(&store).await;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    let mut p = proposal();
    p["reply_evidence_ids"] = json!([context.current_event_id]);
    p["memory_changes"] = json!([{"content":"通常先研究","kind":"semantic","scope":"general","source_event_ids":[context.current_event_id],"supersedes":null}]);
    p["goal_changes"] = json!([{"action":"create","origin_type":"event","origin_ids":[context.current_event_id.to_string()],"description":"阅读已导入资料","success_criterion":"得到有来源的笔记","next_step":"读取资料","stop_condition":"资料缺失时等待","priority":60}]);
    let ready = prepare(&store, &lease, p, vec![]).await;
    let (a, b) = tokio::join!(
        store.commit_decision(&lease, &ready, &[]),
        store.commit_decision(&lease, &ready, &[])
    );
    let receipt = a.unwrap();
    assert_eq!(receipt, b.unwrap());
    assert_eq!(receipt.run_status, "committed");
    assert_eq!(receipt.message_status.as_deref(), Some("delivered"));
    assert_eq!(receipt.memory_ids.len(), 1);
    assert_eq!(receipt.goal_ids.len(), 1);
    pool.close().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_with(connection)
        .await
        .unwrap();
    let restored = Store::new(pool.clone());
    let store = restored.clone();
    assert_eq!(
        restored.commit_decision(&lease, &ready, &[]).await.unwrap(),
        receipt
    );
    for (table, count) in [
        ("decision_commits", 1),
        ("memories", 1),
        ("goals", 1),
        ("outgoing_messages", 1),
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            count
        );
    }
    assert_eq!(
        store.snapshot().await.unwrap().unwrap().state_version,
        lease.state_version + 1
    );
    let origin: Value = sqlx::query_scalar("SELECT origin_ids FROM goals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(origin, json!([context.current_event_id.to_string()]));
    let sources: Vec<Uuid> =
        sqlx::query_scalar("SELECT source_id FROM derivation_edges WHERE target_id=$1")
            .bind(receipt.message_id.unwrap())
            .fetch_all(&pool)
            .await
            .unwrap();
    for id in &ready.context().input_manifest {
        assert!(sources.contains(id));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE kind='message.sent'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM relationship_events WHERE event_id=$1")
            .bind(receipt.message_id.unwrap())
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.commit_decision(&lease, &ready, &[]).await,
        Err(StoreError::StaleState)
    ));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn commit_after_new_input_or_lease_expiry_writes_no_partial_history(pool: PgPool) {
    let store = setup(&pool).await;
    let lease = input(&store).await;
    let ready = prepare(&store, &lease, proposal(), vec![]).await;
    store
        .submit_message("new", "改变当前任务", None)
        .await
        .unwrap();
    assert!(matches!(
        store.commit_decision(&lease, &ready, &[]).await,
        Err(StoreError::LeaseLost)
    ));
    let next = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let ready = prepare(&store, &next, proposal(), vec![]).await;
    sqlx::query(
        "UPDATE runs SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(next.run_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        store.commit_decision(&next, &ready, &[]).await,
        Err(StoreError::LeaseLost)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM decision_commits")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE kind='decision.accepted'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert!(
        sqlx::query_scalar::<_, i64>("SELECT charged_total FROM budget_windows")
            .fetch_one(&pool)
            .await
            .unwrap()
            > 0
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn incompatible_second_revision_rolls_back_every_decision_effect(pool: PgPool) {
    let store = setup(&pool).await;
    let lease = input(&store).await;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    let mut p = proposal();
    p["memory_changes"] = json!([{"content":"一般偏好","kind":"semantic","scope":"general","source_event_ids":[context.current_event_id],"supersedes":null}]);
    let ready = prepare(&store, &lease, p, vec![]).await;
    let original = store.commit_decision(&lease, &ready, &[]).await.unwrap();
    let lease = input(&store).await;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    let mut p = proposal();
    p["memory_changes"] = json!([{"content":"修订一","kind":"semantic","scope":"general","source_event_ids":[context.current_event_id],"supersedes":original.memory_ids[0]},{"content":"修订二","kind":"semantic","scope":"general","source_event_ids":[context.current_event_id],"supersedes":original.memory_ids[0]}]);
    let ready = prepare(&store, &lease, p, vec![]).await;
    assert!(matches!(
        store.commit_decision(&lease, &ready, &[]).await,
        Err(StoreError::Invalid("incompatible supersession scope"))
    ));
    let memories = store.memories().await.unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].validity, "active");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM decision_commits")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        store.snapshot().await.unwrap().unwrap().state_version,
        lease.state_version
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn actual_registry_and_config_are_rechecked_at_commit(pool: PgPool) {
    let store = setup(&pool).await;
    let lease = input(&store).await;
    let mut p = proposal();
    p["reply"] = json!("");
    p["tool_intents"] = json!([{"capability_id":"notes.write","input":{"title":"a","text":"b"}}]);
    let ready = prepare(&store, &lease, p, vec![capability()]).await;
    assert!(matches!(
        store.commit_decision(&lease, &ready, &[]).await,
        Err(StoreError::Proposal(_))
    ));
    let mut changed = capability();
    changed.version = "different".into();
    assert!(matches!(
        store.commit_decision(&lease, &ready, &[changed]).await,
        Err(StoreError::Proposal(_))
    ));
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{capabilities,enabled}','[]')").execute(&pool).await.unwrap();
    assert!(matches!(
        store.commit_decision(&lease, &ready, &[capability()]).await,
        Err(StoreError::Proposal(_))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM actions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn planned_action_is_not_success_and_blocks_new_cognition_until_results(pool: PgPool) {
    let store = setup(&pool).await;
    let mut lease = input(&store).await;
    let (_, receipt) = plan(&store, &lease).await;
    assert_eq!(receipt.run_status, "running");
    assert!(receipt.message_id.is_none());
    assert_eq!(receipt.action_ids.len(), 1);
    let row: (String, Option<Uuid>) = sqlx::query_as("SELECT status,result_event_id FROM actions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row, ("planned".into(), None));
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT tool_calls FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    lease.state_version = receipt.state_version;
    let result = model_step(
        &store,
        &FixtureProvider {
            response: proposal().to_string(),
        },
        &lease,
        vec![],
        CallLimits::default(),
    )
    .await;
    assert!(matches!(
        result,
        Err(alia::runtime::execution::StepError::Storage(
            StoreError::Invalid("action results pending")
        ))
    ));
    store
        .submit_message("new", "中断旧计划", None)
        .await
        .unwrap();
    store.claim_run(Uuid::new_v4()).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM actions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "cancelled"
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn uncertain_executing_action_is_never_replayed_on_recovery(pool: PgPool) {
    let store = setup(&pool).await;
    let lease = input(&store).await;
    let (_, receipt) = plan(&store, &lease).await;
    sqlx::query("UPDATE actions SET status='executing' WHERE id=$1")
        .bind(receipt.action_ids[0])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE runs SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(lease.run_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(store.claim_run(Uuid::new_v4()).await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM actions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "unknown"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "failed"
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn background_reply_is_only_a_private_pending_draft(pool: PgPool) {
    let store = setup(&pool).await;
    let msg = store.submit_message("bg", "背景观察", None).await.unwrap();
    sqlx::query("UPDATE runs SET mode='background' WHERE id=$1")
        .bind(msg.operation_id)
        .execute(&pool)
        .await
        .unwrap();
    let lease = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(lease.mode, RunMode::Background);
    let ready = prepare(&store, &lease, proposal(), vec![]).await;
    let receipt = store.commit_decision(&lease, &ready, &[]).await.unwrap();
    assert_eq!(receipt.message_status.as_deref(), Some("pending"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE kind='message.sent'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox WHERE event_type='message.created' AND resource_id=$1"
        )
        .bind(receipt.message_id.unwrap())
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn budget_freeze_and_blocked_source_prevent_prepared_reply_commit(pool: PgPool) {
    let store = setup(&pool).await;
    let lease = input(&store).await;
    let ready = prepare(&store, &lease, proposal(), vec![]).await;
    sqlx::query("UPDATE individuals SET budget_frozen=true")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.commit_decision(&lease, &ready, &[]).await,
        Err(StoreError::BudgetUnavailable)
    ));
    sqlx::query("UPDATE individuals SET budget_frozen=false")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(ready.context().current_event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.commit_decision(&lease, &ready, &[]).await,
        Err(StoreError::LeaseLost)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM decision_commits")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE kind='message.sent'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn goal_transitions_preserve_declared_evidence_without_self_certified_completion(
    pool: PgPool,
) {
    let store = setup(&pool).await;
    let lease = input(&store).await;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"create","origin_type":"event","origin_ids":[context.current_event_id.to_string()],"description":"探索资料","success_criterion":"核查一个问题","next_step":"读取","stop_condition":"没有资料则等待","priority":40}]);
    let ready = prepare(&store, &lease, p, vec![]).await;
    let goal = store
        .commit_decision(&lease, &ready, &[])
        .await
        .unwrap()
        .goal_ids[0];
    for (action, status) in [
        ("activate", "active"),
        ("wait", "waiting"),
        ("activate", "active"),
        ("abandon", "abandoned"),
    ] {
        let lease = input(&store).await;
        let context = store.model_context(lease.run_id, vec![]).await.unwrap();
        let mut p = proposal();
        p["goal_changes"] = json!([{"action":action,"goal_id":goal,"reason":"根据当前已收到的信息调整","evidence_ids":[context.current_event_id]}]);
        let ready = prepare(&store, &lease, p, vec![]).await;
        let receipt = store.commit_decision(&lease, &ready, &[]).await.unwrap();
        assert_eq!(
            store.commit_decision(&lease, &ready, &[]).await.unwrap(),
            receipt
        );
        let current: String = sqlx::query_scalar("SELECT status FROM goals WHERE id=$1")
            .bind(goal)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(current, status);
        let event: Value = sqlx::query_scalar(
            "SELECT payload FROM events WHERE kind='goal.changed' ORDER BY sequence DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            event["change"]["evidence_ids"],
            json!([context.current_event_id])
        );
        assert_eq!(event["to"], status);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE kind='goal.changed'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        5
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn local_document_catalog_survives_recency_and_filters_private_ancestry(pool: PgPool) {
    let store = setup(&pool).await;
    let old = store
        .import_document(
            "old-doc",
            "旧资料",
            "PRIVATE_BODY_NOT_READ",
            "owner supplied",
        )
        .await
        .unwrap();
    for _ in 0..10 {
        let lease = input(&store).await;
        let ready = prepare(&store, &lease, proposal(), vec![]).await;
        store.commit_decision(&lease, &ready, &[]).await.unwrap();
    }
    let lease = input(&store).await;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    assert!(!context.events.iter().any(|e| e.id == old.event_id));
    assert_eq!(context.document_catalog.items.len(), 1);
    assert_eq!(context.document_catalog.items[0].document_id, old.event_id);
    assert_eq!(context.document_catalog.items[0].characters, 21);
    assert!(!context.document_catalog.has_more);
    assert!(context.input_manifest.contains(&old.event_id));
    assert!(
        !serde_json::to_string(&context)
            .unwrap()
            .contains("PRIVATE_BODY_NOT_READ")
    );
    // Catalog inclusion is not an action or a read receipt.
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM actions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let ready = prepare(&store, &lease, proposal(), vec![]).await;
    store.commit_decision(&lease, &ready, &[]).await.unwrap();

    let middle = store
        .import_document("middle", "中间", "content", "owner supplied")
        .await
        .unwrap();
    let root = store
        .import_document("root", "根来源", "content", "owner supplied")
        .await
        .unwrap();
    let id = store.snapshot().await.unwrap().unwrap().id;
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES($1,$2,$3),($1,$3,$4)")
        .bind(id).bind(root.event_id).bind(middle.event_id).bind(old.event_id).execute(&pool).await.unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(root.event_id)
        .execute(&pool)
        .await
        .unwrap();
    let lease = input(&store).await;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    assert!(context.document_catalog.items.is_empty());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn local_document_catalog_is_bounded_and_discloses_omission(pool: PgPool) {
    let store = setup(&pool).await;
    let mut docs = vec![];
    for i in 0..23 {
        docs.push(
            store
                .import_document(
                    &format!("doc-{i}"),
                    &format!("资料{i}"),
                    "正文",
                    "owner supplied",
                )
                .await
                .unwrap()
                .event_id,
        );
    }
    let lease = input(&store).await;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    assert_eq!(context.document_catalog.items.len(), 20);
    assert!(context.document_catalog.has_more);
    assert_eq!(context.document_catalog.items[0].document_id, docs[22]);
    assert_eq!(context.document_catalog.items[19].document_id, docs[3]);
    assert!(
        context
            .document_catalog
            .items
            .iter()
            .all(|d| context.input_manifest.contains(&d.document_id))
    );
    assert!(serde_json::to_vec(&context).unwrap().len() <= 96 * 1024);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn model_person_links_are_atomic_inferred_replayed_and_advertised(pool: PgPool) {
    let s = setup(&pool).await;
    s.submit_message("people", "沈遥喜欢茶，也喝咖啡。", None)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let c = s.model_context(lease.run_id, vec![]).await.unwrap();
    let e = c.current_event_id;
    let mut p = proposal();
    p["memory_changes"] = json!([
 {"kind":"semantic","content":"沈遥喜欢茶。","scope":"general","source_event_ids":[e],"supersedes":null},
 {"kind":"semantic","content":"沈遥也喝咖啡。","scope":"general","source_event_ids":[e],"supersedes":null}]);
    p["entity_changes"] = json!([
 {"memory":{"kind":"created","index":0},"person":{"kind":"new","name":"沈遥"},"role":"subject","source":{"event_id":e,"start_char":0,"end_char":2},"memory_start_char":0},
 {"memory":{"kind":"created","index":1},"person":{"kind":"prior","index":0},"role":"subject","source":{"event_id":e,"start_char":0,"end_char":2}}]);
    let ready = prepare(&s, &lease, p, vec![]).await;
    let first = s.commit_decision(&lease, &ready, &[]).await.unwrap();
    assert_eq!(first, s.commit_decision(&lease, &ready, &[]).await.unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_entities")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM memory_person_links")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM events WHERE kind IN ('person.inferred','memory.person_inferred') AND actor_id='alia' AND source_kind='inference'").fetch_one(&pool).await.unwrap(),3);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    s.submit_message("later", "再想想沈遥的偏好", None)
        .await
        .unwrap();
    let next = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let c = s.model_context(next.run_id, vec![]).await.unwrap();
    assert_eq!(c.person_catalog.items.len(), 1);
    let person = &c.person_catalog.items[0];
    assert_eq!(person.aliases, vec!["沈遥"]);
    assert!(c.input_manifest.contains(&person.id));
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    let c = s.model_context(next.run_id, vec![]).await.unwrap();
    assert!(c.person_catalog.items.is_empty());
    assert!(s.commit_decision(&lease, &ready, &[]).await.is_err());
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn duplicate_model_entity_links_rollback_every_created_object(pool: PgPool) {
    let s = setup(&pool).await;
    s.submit_message("people", "沈遥喜欢茶。", None)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let c = s.model_context(lease.run_id, vec![]).await.unwrap();
    let e = c.current_event_id;
    let mut p = proposal();
    p["memory_changes"] = json!([{"kind":"semantic","content":"沈遥喜欢茶。","scope":"general","source_event_ids":[e],"supersedes":null}]);
    let annotation = json!({"memory":{"kind":"created","index":0},"person":{"kind":"new","name":"沈遥"},"role":"subject","source":{"event_id":e,"start_char":0,"end_char":2},"memory_start_char":0});
    let mut duplicate = annotation.clone();
    duplicate["person"] = json!({"kind":"prior","index":0});
    p["entity_changes"] = json!([annotation, duplicate]);
    let ready = prepare(&s, &lease, p, vec![]).await;
    assert!(s.commit_decision(&lease, &ready, &[]).await.is_err());
    for table in [
        "memories",
        "person_entities",
        "memory_person_links",
        "decision_commits",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM events WHERE kind IN ('person.inferred','memory.person_inferred')"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}
