use alia::{
    adapters::store::{Store, StoreError},
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        execution::model_step,
        model::{CallLimits, FixtureProvider},
        scheduling::{DecisionReceipt, RunLease},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "create",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap(),
        &RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    s
}
fn proposal() -> Value {
    json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"测试实际目标路径"})
}
async fn commit(s: &Store, l: &RunLease, p: Value) -> Result<DecisionReceipt, StoreError> {
    let caps = s.capabilities().await.unwrap();
    let ready = model_step(
        s,
        &FixtureProvider {
            response: p.to_string(),
        },
        l,
        caps.clone(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    s.commit_decision(l, &ready, &caps).await
}
async fn create(s: &Store, priority: u8) -> (Uuid, Uuid) {
    let m = s
        .submit_message(&Uuid::new_v4().to_string(), "探索资料并记录发现", None)
        .await
        .unwrap();
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let c = s.model_context(m.operation_id, vec![]).await.unwrap();
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"create","origin_type":"event","origin_ids":[c.current_event_id],"description":"探索已有资料","success_criterion":"形成可核查的笔记","next_step":"检查资料","stop_condition":"没有资料则等待","priority":priority}]);
    let r = commit(s, &l, p).await.unwrap();
    (r.goal_ids[0], c.current_event_id)
}
async fn due(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE goals SET due_at=clock_timestamp() WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}
async fn next(s: &Store) -> RunLease {
    let run = s.tick_goals().await.unwrap().unwrap();
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(run, l.run_id);
    l
}
async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn due_goal_is_unique_persists_and_keeps_original_evidence(
    options: sqlx::postgres::PgPoolOptions,
    connection: sqlx::postgres::PgConnectOptions,
) {
    let pool = options.connect_with(connection.clone()).await.unwrap();
    let s = setup(&pool).await;
    let (goal, origin) = create(&s, 50).await;
    // Push the original observation beyond the ordinary eight-event window.
    for i in 0..10 {
        s.import_document(&format!("doc{i}"), "资料", "正文", "测试导入")
            .await
            .unwrap();
    }
    let (a, b) = tokio::join!(s.tick_goals(), s.tick_goals());
    let run = a.unwrap().or(b.unwrap()).unwrap();
    assert_eq!(
        count(&pool, "SELECT count(*) FROM runs WHERE goal_id IS NOT NULL").await,
        1
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM events WHERE kind='goal.due'").await,
        1
    );
    pool.close().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_with(connection)
        .await
        .unwrap();
    let s = Store::new(pool);
    assert!(s.tick_goals().await.unwrap().is_none());
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(l.run_id, run);
    let c = s.model_context(run, vec![]).await.unwrap();
    assert_eq!(c.goals[0].id, goal);
    assert!(c.events.iter().any(|e| e.id == origin));
    assert!(c.input_manifest.contains(&origin));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn idle_paused_and_foreground_work_do_not_start_background(pool: PgPool) {
    let s = setup(&pool).await;
    let baseline = count(&pool, "SELECT count(*) FROM events").await;
    for _ in 0..3 {
        assert!(s.tick_goals().await.unwrap().is_none());
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM events").await, baseline);
    let (g, _) = create(&s, 20).await;
    sqlx::query("UPDATE individuals SET autonomy_paused=true")
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.tick_goals().await.unwrap().is_none());
    sqlx::query("UPDATE individuals SET autonomy_paused=false")
        .execute(&pool)
        .await
        .unwrap();
    let m = s
        .submit_message("foreground", "先处理当前消息", None)
        .await
        .unwrap();
    assert!(s.tick_goals().await.unwrap().is_none());
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(l.run_id, m.operation_id);
    assert!(s.tick_goals().await.unwrap().is_none());
    commit(&s, &l, proposal()).await.unwrap();
    let l = next(&s).await;
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT goal_id FROM runs WHERE id=$1")
            .bind(l.run_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        g
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn priority_and_goal_state_are_checked_before_claim(pool: PgPool) {
    let s = setup(&pool).await;
    create(&s, 10).await;
    let (high, _) = create(&s, 90).await;
    let run = s.tick_goals().await.unwrap().unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT goal_id FROM runs WHERE id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap(),
        high
    );
    sqlx::query("UPDATE goals SET status='abandoned' WHERE id=$1")
        .bind(high)
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.claim_run(Uuid::new_v4()).await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "stale"
    );
    assert!(s.tick_goals().await.unwrap().is_some());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn three_empty_cycles_sleep_and_reactivation_requires_fresh_evidence(pool: PgPool) {
    let s = setup(&pool).await;
    let (goal, origin) = create(&s, 50).await;
    for n in 1..=3 {
        due(&pool, goal).await;
        let l = next(&s).await;
        commit(&s, &l, proposal()).await.unwrap();
        assert!(s.tick_goals().await.unwrap().is_none());
        assert_eq!(
            sqlx::query_scalar::<_, i32>("SELECT no_progress_cycles FROM goals WHERE id=$1")
                .bind(goal)
                .fetch_one(&pool)
                .await
                .unwrap(),
            n
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT waiting_reason FROM goals WHERE id=$1")
            .bind(goal)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "no_progress"
    );
    let baseline = count(&pool, "SELECT count(*) FROM events").await;
    for _ in 0..3 {
        assert!(s.tick_goals().await.unwrap().is_none());
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM events").await, baseline);
    let input = s
        .submit_message("new", "新增资料可以继续研究", None)
        .await
        .unwrap();
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let c = s.model_context(input.operation_id, vec![]).await.unwrap();
    let mut p = proposal();
    p["goal_changes"] =
        json!([{"action":"activate","goal_id":goal,"reason":"继续","evidence_ids":[]}]);
    assert!(matches!(
        commit(&s, &l, p).await,
        Err(StoreError::Invalid("goal needs new external evidence"))
    ));
    // Rewording the goal with the old origin cannot bypass no_progress.
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"create","origin_type":"event","origin_ids":[origin],"description":"换个名字探索资料","success_criterion":"记录新发现","next_step":"研究","stop_condition":"无资料等待","priority":50}]);
    assert!(matches!(
        commit(&s, &l, p).await,
        Err(StoreError::Invalid(
            "resume the existing goal with new evidence"
        ))
    ));
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"activate","goal_id":goal,"reason":"新增资料改变条件","evidence_ids":[c.current_event_id]}]);
    commit(&s, &l, p).await.unwrap();
    assert!(s.tick_goals().await.unwrap().is_some());
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT no_progress_cycles FROM goals WHERE id=$1")
            .bind(goal)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn actual_notes_count_once_while_rephrased_titles_and_memory_do_not(pool: PgPool) {
    let s = setup(&pool).await;
    let (goal, _) = create(&s, 50).await;
    for n in 0..3 {
        due(&pool, goal).await;
        let mut l = next(&s).await;
        let mut p = proposal();
        p["tool_intents"] = json!([{"capability_id":"notes.write","input":{"title":format!("标题{n}"),"text":"同一个实际产物"}}]);
        let r = commit(&s, &l, p).await.unwrap();
        l.state_version = r.state_version;
        let a = s.execute_local_action(&l, r.action_ids[0]).await.unwrap();
        l.state_version = a.state_version;
        commit(&s, &l, proposal()).await.unwrap();
        s.tick_goals().await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i32>("SELECT no_progress_cycles FROM goals WHERE id=$1")
                .bind(goal)
                .fetch_one(&pool)
                .await
                .unwrap(),
            n
        );
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM notes").await, 3);
    assert_eq!(count(&pool, "SELECT count(*) FROM goal_progress").await, 1);
    assert_eq!(
        count(&pool, "SELECT sum(novel_results)::bigint FROM goal_cycles").await,
        1
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn repeated_document_page_does_not_count_but_new_page_does(pool: PgPool) {
    let s = setup(&pool).await;
    let doc = s
        .import_document("doc", "实验资料", "甲乙丙丁", "owner")
        .await
        .unwrap();
    let (goal, _) = create(&s, 50).await;
    for (offset, expected) in [(0, 0), (0, 1), (2, 0)] {
        due(&pool, goal).await;
        let mut l = next(&s).await;
        let mut p = proposal();
        p["tool_intents"] = json!([{"capability_id":"documents.read","input":{"document_id":doc.event_id,"offset":offset,"max_chars":2}}]);
        let r = commit(&s, &l, p).await.unwrap();
        l.state_version = r.state_version;
        let a = s.execute_local_action(&l, r.action_ids[0]).await.unwrap();
        l.state_version = a.state_version;
        commit(&s, &l, proposal()).await.unwrap();
        s.tick_goals().await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i32>("SELECT no_progress_cycles FROM goals WHERE id=$1")
                .bind(goal)
                .fetch_one(&pool)
                .await
                .unwrap(),
            expected
        );
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM goal_progress").await, 2);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn deletion_barrier_and_epoch_change_prevent_reexecution(pool: PgPool) {
    let s = setup(&pool).await;
    let (goal, origin) = create(&s, 50).await;
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(origin)
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.tick_goals().await.unwrap().is_none());
    assert_eq!(
        count(&pool, "SELECT count(*) FROM events WHERE kind='goal.due'").await,
        0
    );
    // Undo only this test-injected barrier to test a distinct in-flight epoch change.
    sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
        .bind(origin)
        .execute(&pool)
        .await
        .unwrap();
    let l = next(&s).await;
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    s.claim_run(Uuid::new_v4()).await.unwrap();
    assert!(s.tick_goals().await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT waiting_reason FROM goals WHERE id=$1")
            .bind(goal)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "source_unavailable"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id=$1")
            .bind(l.run_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "stale"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn failure_in_trigger_or_cycle_record_rolls_back_every_state_change(pool: PgPool) {
    let s = setup(&pool).await;
    let (goal, _) = create(&s, 50).await;
    let version = s.snapshot().await.unwrap().unwrap().state_version;
    sqlx::raw_sql("CREATE FUNCTION fail_goal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind IN ('goal.due','goal.cycle_evaluated') THEN RAISE EXCEPTION 'test fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER goal_fault BEFORE INSERT ON events FOR EACH ROW EXECUTE FUNCTION fail_goal();").execute(&pool).await.unwrap();
    assert!(s.tick_goals().await.is_err());
    assert_eq!(
        count(&pool, "SELECT count(*) FROM runs WHERE goal_id IS NOT NULL").await,
        0
    );
    assert_eq!(s.snapshot().await.unwrap().unwrap().state_version, version);
    sqlx::query("ALTER TABLE events DISABLE TRIGGER goal_fault")
        .execute(&pool)
        .await
        .unwrap();
    let l = next(&s).await;
    commit(&s, &l, proposal()).await.unwrap();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER goal_fault")
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.tick_goals().await.is_err());
    assert_eq!(count(&pool, "SELECT count(*) FROM goal_cycles").await, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT no_progress_cycles FROM goals WHERE id=$1")
            .bind(goal)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER goal_fault ON events")
        .execute(&pool)
        .await
        .unwrap();
    s.tick_goals().await.unwrap();
    s.tick_goals().await.unwrap();
    assert_eq!(count(&pool, "SELECT count(*) FROM goal_cycles").await, 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn discovery_is_once_per_source_concurrent_and_can_choose_no_goal(pool: PgPool) {
    let s = setup(&pool).await;
    assert!(s.tick_discovery().await.unwrap().is_none());
    let doc = s
        .import_document(
            "new",
            "新资料",
            "只是一份可读资料，不是用户任务",
            "owner import",
        )
        .await
        .unwrap();
    let (a, b) = tokio::join!(s.tick_discovery(), s.tick_discovery());
    let ids: Vec<Uuid> = [a.unwrap(), b.unwrap()].into_iter().flatten().collect();
    assert_eq!(ids.len(), 1);
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(lease.run_id, ids[0]);
    let context = s
        .model_context(lease.run_id, s.capabilities().await.unwrap())
        .await
        .unwrap();
    assert_eq!(context.current_event_id, doc.event_id);
    assert!(context.goals.is_empty());
    assert_eq!(context.mode, alia::domain::cognition::RunMode::Background);
    // Empty decision is a valid choice, not a failed forced goal.
    commit(&s, &lease, proposal()).await.unwrap();
    let restored = Store::new(pool.clone());
    assert!(restored.tick_discovery().await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM goals")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM discovery_occurrences")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    // Virtual time tests only the deterministic cooldown; this is not a live run.
    s.import_document("second", "另一资料", "新内容", "owner import")
        .await
        .unwrap();
    assert!(s.tick_discovery().await.unwrap().is_none());
    sqlx::query("UPDATE discovery_occurrences SET created_at=clock_timestamp()-interval '2 hours'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.tick_discovery().await.unwrap().is_some());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn discovery_respects_pause_freeze_foreground_and_blocked_sources(pool: PgPool) {
    let s = setup(&pool).await;
    let doc = s
        .import_document("new", "资料", "内容", "owner import")
        .await
        .unwrap();
    sqlx::query("UPDATE individuals SET autonomy_paused=true")
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.tick_discovery().await.unwrap().is_none());
    sqlx::query("UPDATE individuals SET autonomy_paused=false,budget_frozen=true")
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.tick_discovery().await.unwrap().is_none());
    sqlx::query("UPDATE individuals SET budget_frozen=false")
        .execute(&pool)
        .await
        .unwrap();
    s.submit_message("foreground", "现在先回应", None)
        .await
        .unwrap();
    assert!(s.tick_discovery().await.unwrap().is_none());
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    commit(&s, &lease, proposal()).await.unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(doc.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.tick_discovery().await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM discovery_occurrences")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn discovery_recovers_undispatched_resource_wait_without_duplicate_dispatch(pool: PgPool) {
    let s = setup(&pool).await;
    s.import_document("doc", "资料", "内容", "owner import")
        .await
        .unwrap();
    let first = s.tick_discovery().await.unwrap().unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    s.stop_cycle(&lease, alia::runtime::worker::StopReason::Budget)
        .await
        .unwrap();
    assert!(s.tick_discovery().await.unwrap().is_none());
    sqlx::query("UPDATE discovery_occurrences SET created_at=clock_timestamp()-interval '2 hours'")
        .execute(&pool)
        .await
        .unwrap();
    let next = s.tick_discovery().await.unwrap().unwrap();
    assert_ne!(next, first);
    assert!(s.tick_discovery().await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM discovery_occurrences")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    commit(&s, &lease, proposal()).await.unwrap();
    sqlx::query("UPDATE discovery_occurrences SET created_at=clock_timestamp()-interval '2 hours'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.tick_discovery().await.unwrap().is_none());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn mixed_goal_motives_survive_restart_recency_and_source_revocation(
    options: sqlx::postgres::PgPoolOptions,
    connection: sqlx::postgres::PgConnectOptions,
) {
    let pool = options.connect_with(connection.clone()).await.unwrap();
    let s = setup(&pool).await;
    let input = s
        .submit_message("mixed-input", "这个观察值得与自己的兴趣一起研究", None)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let context = s.model_context(input.operation_id, vec![]).await.unwrap();
    let interest = context.interests[0].id.clone();
    let refs = json!([{"origin_type":"event","origin_id":input.event_id},{"origin_type":"interest","origin_id":interest}]);
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"create","origins":refs,"description":"有观察和兴趣的探索","success_criterion":"实际有来源的产物","next_step":"检查资料","stop_condition":"完成或缺少条件","priority":50}]);
    let receipt = commit(&s, &lease, p).await.unwrap();
    let goal = receipt.goal_ids[0];
    let saved: (String, Value) =
        sqlx::query_as("SELECT origin_type,origin_ids FROM goals WHERE id=$1")
            .bind(goal)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved, ("mixed".into(), json!([input.event_id, interest])));
    let changed:Value=sqlx::query_scalar("SELECT payload->'change'->'origins' FROM events WHERE kind='goal.changed' AND payload->>'goal_id'=$1").bind(goal.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(changed, refs);
    for i in 0..12 {
        s.import_document(
            &format!("mixed-doc-{i}"),
            "后来的资料",
            "真实测试文本",
            "合成来源",
        )
        .await
        .unwrap();
    }
    pool.close().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_with(connection)
        .await
        .unwrap();
    let restored = Store::new(pool.clone());
    let run = restored.tick_goals().await.unwrap().unwrap();
    let lease = restored.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(run, lease.run_id);
    let current = restored.model_context(run, vec![]).await.unwrap();
    assert_eq!(current.current_goal_id, Some(goal));
    assert!(current.events.iter().any(|e| e.id == input.event_id));
    assert!(current.input_manifest.contains(&input.event_id));
    sqlx::query("UPDATE objects SET blocked=true WHERE individual_id=$1 AND id=$2")
        .bind(current.individual_id)
        .bind(input.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(restored.model_context(run, vec![]).await.is_err());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn changed_origin_labels_do_not_clone_exhausted_causes_but_new_observations_can(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let first = s
        .submit_message("first-cause", "这是第一次观察", None)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let c = s.model_context(lease.run_id, vec![]).await.unwrap();
    let interest = c.interests[0].id.clone();
    let make = |origins: Value| json!({"action":"create","origins":origins,"description":"有来源的目标","success_criterion":"可检查的实际结果","next_step":"检查资料","stop_condition":"完成或无法推进时停止","priority":50});
    let mut p = proposal();
    p["goal_changes"] = json!([make(
        json!([{"origin_type":"event","origin_id":first.event_id},{"origin_type":"interest","origin_id":interest}])
    )]);
    let old = commit(&s, &lease, p).await.unwrap().goal_ids[0];
    // Explicit synthetic stop fixture; not a claim of autonomous completion.
    sqlx::query(
        "UPDATE goals SET status='waiting',waiting_reason='no_progress',due_at=NULL,origin_ids=jsonb_set(origin_ids,'{0}',to_jsonb(upper(origin_ids->>0)),false) WHERE id=$1",
    )
    .bind(old)
    .execute(&pool)
    .await
    .unwrap();
    let next = s
        .submit_message("new-cause", "出现了不同的新观察", None)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    for origins in [
        json!([{"origin_type":"event","origin_id":first.event_id}]),
        json!([{"origin_type":"interest","origin_id":interest}]),
    ] {
        let mut p = proposal();
        p["goal_changes"] = json!([make(origins)]);
        assert!(matches!(
            commit(&s, &lease, p).await,
            Err(StoreError::Invalid(
                "resume the existing goal with new evidence"
            ))
        ));
    }
    let mut p = proposal();
    p["goal_changes"] = json!([make(
        json!([{"origin_type":"event","origin_id":next.event_id},{"origin_type":"interest","origin_id":interest}])
    )]);
    let fresh = commit(&s, &lease, p).await.unwrap().goal_ids[0];
    assert_ne!(fresh, old);
    assert_eq!(count(&pool, "SELECT count(*) FROM goals").await, 2);
    let state: String = sqlx::query_scalar("SELECT status FROM goals WHERE id=$1")
        .bind(old)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "waiting");
}
