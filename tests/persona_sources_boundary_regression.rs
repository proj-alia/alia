//! Independent P04 phase-boundary checks; synthetic setup, no network provider.
// This target uses only the task-strategy subset of the shared scenario fixture.
#![allow(dead_code)]
include!("support/development_context_fixture.rs");

#[sqlx::test]
#[ignore = "requires disposable schema29 PostgreSQL; synthetic observations only"]
async fn accepted_task_strategy_must_survive_its_own_model_reservation_and_commit(pool: PgPool) {
    let s = setup(&pool).await;
    let server = Server::start(&pool).await;
    let (_, _, goal) = server
        .call(
            "POST",
            "/goals",
            &[("idempotency-key", "review-task-goal")],
            Some(&json!({"description":"独立运行边界","success_criterion":"有来源结果","next_step":"检查","stop_condition":"无资料等待"}).to_string()),
            true,
        )
        .await;
    let g = Uuid::parse_str(goal["goal_id"].as_str().unwrap()).unwrap();
    let input = s
        .submit_message("review-task-input", "合成任务策略运行", None)
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET goal_id=$2 WHERE id=$1")
        .bind(input.operation_id)
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    let verifier = registered(&pool).await;
    accepted(
        &s,
        &pool,
        &draft(
            input.event_id,
            DevelopmentTarget::TaskStrategy,
            json!({"scope":StrategyScope::Task(input.operation_id),"description":"先检查记录，再作答"}),
        ),
        verifier,
    )
    .await;
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(lease.run_id, input.operation_id);
    let before = s.model_context(lease.run_id, vec![]).await.unwrap();
    let strategy = before
        .development_context
        .as_ref()
        .unwrap()
        .effective_task_strategy
        .as_ref()
        .unwrap();
    assert_eq!(strategy.scope, StrategyScope::Task(lease.run_id));
    let response = json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"synthetic phase boundary, not growth proof"}).to_string();
    let ready = alia::runtime::execution::model_step(
        &s,
        &alia::runtime::model::FixtureProvider { response },
        &lease,
        vec![],
        alia::runtime::model::CallLimits::default(),
    )
    .await
    .unwrap();
    let after = s.model_context(lease.run_id, vec![]).await.unwrap();
    let old_sources = &ready
        .context()
        .development_context
        .as_ref()
        .unwrap()
        .source_ids;
    let new_sources = &after.development_context.as_ref().unwrap().source_ids;
    let added: Vec<_> = new_sources
        .iter()
        .filter(|id| !old_sources.contains(id))
        .collect();
    let committed = s.commit_decision(&lease, &ready, &[]).await;
    let accounting: (i64, i64) = sqlx::query_as("SELECT count(*),coalesce(sum(charged_tokens),0)::bigint FROM usage_reservations WHERE run_id=$1 AND status='settled'")
        .bind(lease.run_id).fetch_one(&pool).await.unwrap();
    assert!(
        committed.is_ok(),
        "normal own reservation invalidated accepted task strategy: commit={committed:?}, source_count={}->{}, added={added:?}, settled/charged={accounting:?}",
        old_sources.len(),
        new_sources.len()
    );
}
