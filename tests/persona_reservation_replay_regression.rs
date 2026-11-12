//! Independent P04 phase-boundary checks; synthetic setup, no network provider.
// This target uses only the task-strategy subset of the shared scenario fixture.
#![allow(dead_code)]
include!("support/development_context_fixture.rs");

#[sqlx::test]
#[ignore = "requires disposable schema29 PostgreSQL; synthetic observations only"]
async fn accepted_task_strategy_must_replay_the_same_undispatched_author_reservation(pool: PgPool) {
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
    let key = Uuid::new_v4();
    let allowance = alia::runtime::scheduling::TokenAllowance {
        input_upper_bound: 100,
        output_cap: 50,
    };
    let first = s
        .reserve_model_call(&lease, key, &before, allowance)
        .await
        .unwrap();
    let stable = fingerprint(&pool).await;
    let replay = s.reserve_model_call(&lease, key, &before, allowance).await;
    assert_eq!(
        fingerprint(&pool).await,
        stable,
        "reservation replay must not mutate state"
    );
    let accounting: (i64, i64, i64) = sqlx::query_as("SELECT count(*),coalesce(sum(reserved_tokens),0)::bigint,coalesce(sum(charged_tokens),0)::bigint FROM usage_reservations WHERE run_id=$1")
        .bind(lease.run_id).fetch_one(&pool).await.unwrap();
    assert!(
        replay.is_ok(),
        "same key/context/allowance after own reserve must recover the existing reservation; first={}, replay={replay:?}, count/reserved/charged={accounting:?}",
        first.id
    );
    assert_eq!(replay.unwrap().id, first.id);
}
