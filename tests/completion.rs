use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        cognition::{DecisionProposal, ProposalError},
        completion::EXCERPT_CRITERION,
        config::{PersonaSeed, RuntimeRules},
    },
    runtime::{
        capability::{ActionReceipt, IMPLEMENTED_CAPABILITIES},
        execution::{PreparedDecision, model_step},
        model::{CallLimits, FixtureProvider},
        scheduling::{DecisionReceipt, RunLease},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
const BODY: &str = "甲乙丙丁🙂戊己庚辛壬癸。";
fn citation(doc: Uuid) -> Value {
    json!({"document_id":doc,"start_char":0,"end_char":8})
}
fn proposal() -> Value {
    json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"按可验证交付条件执行"})
}
async fn prepare(s: &Store, l: &RunLease, p: Value) -> PreparedDecision {
    model_step(
        s,
        &FixtureProvider {
            response: p.to_string(),
        },
        l,
        s.capabilities().await.unwrap(),
        CallLimits::default(),
    )
    .await
    .unwrap()
}
async fn commit(s: &Store, l: &mut RunLease, p: Value) -> DecisionReceipt {
    let ready = prepare(s, l, p).await;
    let r = s
        .commit_decision(l, &ready, &s.capabilities().await.unwrap())
        .await
        .unwrap();
    l.state_version = r.state_version;
    r
}
async fn setup(pool: &PgPool) -> (Store, Uuid) {
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
    let doc = s
        .import_document("doc", "Unicode资料", BODY, "owner")
        .await
        .unwrap()
        .event_id;
    (s, doc)
}
async fn goal(s: &Store, doc: Uuid) -> Uuid {
    s.submit_message(&Uuid::new_v4().to_string(), "为我保存指定片段及来源", None)
        .await
        .unwrap();
    let mut l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"create","origin_type":"event","origin_ids":[doc],"description":"保存资料摘录","success_criterion":EXCERPT_CRITERION,"next_step":"先实际读取片段","stop_condition":"资料缺失时等待","priority":50,"completion":{"kind":"source_excerpt","document_id":doc,"start_char":0,"end_char":8}}]);
    commit(s, &mut l, p).await.goal_ids[0]
}
async fn begin(s: &Store, g: Uuid, doc: Uuid, segments: &[(usize, usize)]) -> RunLease {
    s.tick_goals().await.unwrap().unwrap();
    let mut l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut p = proposal();
    p["goal_changes"] =
        json!([{"action":"activate","goal_id":g,"reason":"开始摘录","evidence_ids":[doc]}]);
    p["tool_intents"]=json!(segments.iter().map(|(offset,max)|json!({"capability_id":"documents.read","input":{"document_id":doc,"offset":offset,"max_chars":max}})).collect::<Vec<_>>());
    if segments.is_empty() {
        // Keep the cycle open via a real non-document action. Memory lookup is
        // deliberately insufficient evidence that the source body was read.
        p["tool_intents"] = json!([{"capability_id":"memory.search","input":{"query":"资料"}}]);
    }
    let r = commit(s, &mut l, p).await;
    for a in r.action_ids {
        let r = s.execute_local_action(&l, a).await.unwrap();
        assert_eq!(r.status, "succeeded");
        l.state_version = r.state_version;
    }
    l
}
async fn note(
    s: &Store,
    l: &mut RunLease,
    doc: Uuid,
    text: &str,
    with_citation: bool,
) -> ActionReceipt {
    let mut p = proposal();
    let mut input = json!({"title":"摘录笔记","text":text});
    if with_citation {
        input["citations"] = json!([citation(doc)]);
    }
    p["tool_intents"] = json!([{"capability_id":"notes.write","input":input}]);
    let r = commit(s, l, p).await;
    let a = s.execute_local_action(l, r.action_ids[0]).await.unwrap();
    l.state_version = a.state_version;
    a
}
fn complete(g: Uuid, result: Uuid) -> Value {
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"complete","goal_id":g,"reason":"指定摘录与读取证据已核验","evidence_ids":[result]}]);
    p
}
async fn count(pool: &PgPool, q: &str) -> i64 {
    sqlx::query_scalar(q).fetch_one(pool).await.unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn actual_unicode_reads_note_and_verified_completion_are_atomic_and_replayable(pool: PgPool) {
    let (s, doc) = setup(&pool).await;
    let g = goal(&s, doc).await;
    let mut l = begin(&s, g, doc, &[(0, 4), (4, 4)]).await;
    let a = note(
        &s,
        &mut l,
        doc,
        &format!("摘录：{}", BODY.chars().take(8).collect::<String>()),
        true,
    )
    .await;
    assert_eq!(a.status, "succeeded");
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    assert!(c.verified_goal_completions.contains(&g));
    let ready = prepare(&s, &l, complete(g, a.result_event_id)).await;
    let r = s.commit_decision(&l, &ready, &[]).await.unwrap();
    assert_eq!(r.run_status, "committed");
    assert_eq!(s.commit_decision(&l, &ready, &[]).await.unwrap(), r);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM goals WHERE id=$1")
            .bind(g)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "completed"
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='goal.completion_verified'"
        )
        .await,
        1
    );
    let proof: Value = sqlx::query_scalar(
        "SELECT payload->'proof' FROM events WHERE kind='goal.completion_verified'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(proof["checker_version"], "source_excerpt/2");
    assert_eq!(proof["note_action_id"], a.action_id.to_string());
    assert!(s.tick_goals().await.unwrap().is_none());
    assert_eq!(count(&pool, "SELECT count(*) FROM goal_cycles").await, 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn unread_citation_fails_instead_of_creating_false_evidence(pool: PgPool) {
    let (s, doc) = setup(&pool).await;
    let g = goal(&s, doc).await;
    let mut l = begin(&s, g, doc, &[]).await;
    let a = note(&s, &mut l, doc, BODY, true).await;
    assert_eq!(a.status, "failed");
    assert!(a.executed);
    let error: Value = sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(a.result_event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        error["output"]["error"],
        "citation_not_observed_or_mismatched"
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM notes").await, 0);
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    assert!(c.verified_goal_completions.is_empty());
    assert_eq!(
        DecisionProposal::parse(complete(g, a.result_event_id).to_string().as_bytes())
            .unwrap()
            .validate_context(&c),
        Err(ProposalError::UnverifiedCompletion)
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn incomplete_read_coverage_and_incorrect_quote_do_not_verify(pool: PgPool) {
    let (s, doc) = setup(&pool).await;
    let g = goal(&s, doc).await;
    let mut l = begin(&s, g, doc, &[(0, 4)]).await;
    let a = note(&s, &mut l, doc, BODY, true).await;
    assert_eq!(a.status, "failed");
    assert!(
        s.model_context(l.run_id, vec![])
            .await
            .unwrap()
            .verified_goal_completions
            .is_empty()
    );
    commit(&s, &mut l, proposal()).await;
    s.tick_goals().await.unwrap();
    sqlx::query("UPDATE goals SET due_at=clock_timestamp() WHERE id=$1")
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    s.tick_goals().await.unwrap();
    let mut l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut p = proposal();
    p["tool_intents"] = json!([{"capability_id":"documents.read","input":{"document_id":doc}}]);
    let r = commit(&s, &mut l, p).await;
    let a = s.execute_local_action(&l, r.action_ids[0]).await.unwrap();
    l.state_version = a.state_version;
    let a = note(&s, &mut l, doc, "这段文字并不是原文。", true).await;
    assert_eq!(a.status, "failed");
    let error: Value = sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(a.result_event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(error["output"]["error"], "citation_quote_missing");
    assert!(!error["output"].to_string().contains(BODY));
    assert_eq!(count(&pool, "SELECT count(*) FROM notes").await, 0);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn uncited_notes_and_unrelated_reads_cannot_complete_the_goal(pool: PgPool) {
    let (s, doc) = setup(&pool).await;
    s.submit_message("prior", "读一下资料", None).await.unwrap();
    let mut unrelated = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut p = proposal();
    p["tool_intents"] = json!([{"capability_id":"documents.read","input":{"document_id":doc}}]);
    let r = commit(&s, &mut unrelated, p).await;
    let a = s
        .execute_local_action(&unrelated, r.action_ids[0])
        .await
        .unwrap();
    unrelated.state_version = a.state_version;
    commit(&s, &mut unrelated, proposal()).await;
    let g = goal(&s, doc).await;
    let mut l = begin(&s, g, doc, &[]).await;
    let a = note(&s, &mut l, doc, BODY, true).await;
    assert_eq!(a.status, "failed");
    let a = note(&s, &mut l, doc, BODY, false).await;
    assert_eq!(a.status, "succeeded");
    assert!(
        s.model_context(l.run_id, vec![])
            .await
            .unwrap()
            .verified_goal_completions
            .is_empty()
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn same_goal_read_history_survives_new_cycle_and_connection_recreation(
    options: sqlx::postgres::PgPoolOptions,
    connection: sqlx::postgres::PgConnectOptions,
) {
    let pool = options.connect_with(connection.clone()).await.unwrap();
    let (s, doc) = setup(&pool).await;
    let g = goal(&s, doc).await;
    let mut l = begin(&s, g, doc, &[(0, 8)]).await;
    commit(&s, &mut l, proposal()).await;
    s.tick_goals().await.unwrap();
    pool.close().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_with(connection)
        .await
        .unwrap();
    let s = Store::new(pool.clone());
    sqlx::query("UPDATE goals SET due_at=clock_timestamp() WHERE id=$1")
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    s.tick_goals().await.unwrap();
    let mut l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let a = note(&s, &mut l, doc, BODY, true).await;
    assert_eq!(a.status, "succeeded");
    commit(&s, &mut l, complete(g, a.result_event_id)).await;
    assert_eq!(
        count(&pool, "SELECT count(*) FROM goals WHERE status='completed'").await,
        1
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn changed_source_is_rechecked_at_commit_and_not_trusted_from_context(pool: PgPool) {
    let (s, doc) = setup(&pool).await;
    let g = goal(&s, doc).await;
    let mut l = begin(&s, g, doc, &[(0, 8)]).await;
    let a = note(&s, &mut l, doc, BODY, true).await;
    let ready = prepare(&s, &l, complete(g, a.result_event_id)).await;
    // Inject a storage fault without changing the version: final evaluator must still check.
    sqlx::query("UPDATE documents SET body='此处被故障注入换成了别的内容' WHERE event_id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.commit_decision(&l, &ready, &[]).await,
        Err(StoreError::Proposal(ProposalError::UnverifiedCompletion))
    ));
    assert_eq!(
        count(&pool, "SELECT count(*) FROM goals WHERE status='completed'").await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='goal.completion_verified'"
        )
        .await,
        0
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn deleted_source_blocks_prepared_completion(pool: PgPool) {
    let (s, doc) = setup(&pool).await;
    let g = goal(&s, doc).await;
    let mut l = begin(&s, g, doc, &[(0, 8)]).await;
    let a = note(&s, &mut l, doc, BODY, true).await;
    let ready = prepare(&s, &l, complete(g, a.result_event_id)).await;
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.commit_decision(&l, &ready, &[]).await.is_err());
    assert_eq!(
        count(&pool, "SELECT count(*) FROM goals WHERE status='completed'").await,
        0
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn verification_event_fault_rolls_back_goal_and_receipt(pool: PgPool) {
    let (s, doc) = setup(&pool).await;
    let g = goal(&s, doc).await;
    let mut l = begin(&s, g, doc, &[(0, 8)]).await;
    let a = note(&s, &mut l, doc, BODY, true).await;
    let ready = prepare(&s, &l, complete(g, a.result_event_id)).await;
    sqlx::raw_sql("CREATE FUNCTION fail_verify() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind='goal.completion_verified' THEN RAISE EXCEPTION 'test fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER verify_fault BEFORE INSERT ON events FOR EACH ROW EXECUTE FUNCTION fail_verify();").execute(&pool).await.unwrap();
    assert!(s.commit_decision(&l, &ready, &[]).await.is_err());
    assert_eq!(
        count(&pool, "SELECT count(*) FROM goals WHERE status='completed'").await,
        0
    );
    sqlx::query("DROP TRIGGER verify_fault ON events")
        .execute(&pool)
        .await
        .unwrap();
    s.commit_decision(&l, &ready, &[]).await.unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='goal.completion_verified'"
        )
        .await,
        1
    );
}
