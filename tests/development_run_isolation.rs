//! Actual schema30 runs and accounting; all observations/model outputs below
//! are explicitly synthetic engineering fixtures, not growth evidence.
#![allow(dead_code)]
include!("support/development_context_fixture.rs");
use alia::{
    adapters::store::StoreError,
    domain::development_trials::*,
    runtime::{
        execution::{PreparedDecision, model_step},
        model::{CallLimits, FixtureProvider},
        scheduling::{RunLease, TokenAllowance},
        worker::{RetryCause, StopReason},
    },
};

include!("support/development_trial_run_fixture.rs");

async fn prepare(s: &Store, lease: &RunLease) -> PreparedDecision {
    model_step(
        s,
        &FixtureProvider {
            response: no_changes().to_string(),
        },
        lease,
        s.capabilities().await.unwrap(),
        CallLimits::default(),
    )
    .await
    .unwrap()
}

fn rejected<T: std::fmt::Debug>(value: Result<T, StoreError>) {
    assert!(
        matches!(
            value,
            Err(StoreError::Invalid("evaluation run requires trial runtime"))
        ),
        "{value:?}"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn actual_evaluation_run_cannot_reserve_through_unmarked_normal_context(pool: PgPool) {
    let (s, lease, _, _) = work(&pool).await;
    let context = s
        .model_context(lease.run_id, s.capabilities().await.unwrap())
        .await
        .unwrap();
    assert!(context.trial_binding.is_none());
    let before = fingerprint(&pool).await;
    rejected(
        s.reserve_model_call(
            &lease,
            Uuid::new_v4(),
            &context,
            TokenAllowance {
                input_upper_bound: 100,
                output_cap: 50,
            },
        )
        .await,
    );
    assert_eq!(fingerprint(&pool).await, before);
    rejected(s.schedule_model_retry(&lease, RetryCause::Transient).await);
    assert_eq!(fingerprint(&pool).await, before);
}

// Correlation fault injection: hide just the durable run mapping while making a
// genuine normal request, then restore it. Never fabricate a model receipt.
async fn hide_mapping(pool: &PgPool, plan: &TrialPlan, run: Uuid) {
    sqlx::query("UPDATE development_trial_cases SET evaluation_run_id=NULL WHERE plan_id=$1 AND evaluation_run_id=$2")
        .bind(plan.id).bind(run).execute(pool).await.unwrap();
}
async fn restore_mapping(pool: &PgPool, plan: &TrialPlan, run: Uuid) {
    sqlx::query(
        "UPDATE development_trial_cases SET evaluation_run_id=$2 WHERE plan_id=$1 AND id=$3",
    )
    .bind(plan.id)
    .bind(run)
    .bind(plan.cases[0].id)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn dispatch_rechecks_actual_run_mapping_without_new_charge(pool: PgPool) {
    let (s, lease, plan, _) = work(&pool).await;
    hide_mapping(&pool, &plan, lease.run_id).await;
    let context = s
        .model_context(lease.run_id, s.capabilities().await.unwrap())
        .await
        .unwrap();
    let reservation = s
        .reserve_model_call(
            &lease,
            Uuid::new_v4(),
            &context,
            TokenAllowance {
                input_upper_bound: 100,
                output_cap: 50,
            },
        )
        .await
        .unwrap();
    restore_mapping(&pool, &plan, lease.run_id).await;
    let before = fingerprint(&pool).await;
    rejected(s.dispatch_model_call(&lease, reservation.id).await);
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn fresh_and_cached_decision_cannot_publish_from_actual_evaluation_run(pool: PgPool) {
    let (s, lease, plan, _) = work(&pool).await;
    hide_mapping(&pool, &plan, lease.run_id).await;
    let prepared = prepare(&s, &lease).await;
    restore_mapping(&pool, &plan, lease.run_id).await;
    let before = fingerprint(&pool).await;
    rejected(
        s.commit_decision(&lease, &prepared, &s.capabilities().await.unwrap())
            .await,
    );
    assert_eq!(fingerprint(&pool).await, before);
    hide_mapping(&pool, &plan, lease.run_id).await;
    let receipt = s
        .commit_decision(&lease, &prepared, &s.capabilities().await.unwrap())
        .await
        .unwrap();
    restore_mapping(&pool, &plan, lease.run_id).await;
    let before = fingerprint(&pool).await;
    rejected(
        s.commit_decision(&lease, &prepared, &s.capabilities().await.unwrap())
            .await,
    );
    assert_eq!(fingerprint(&pool).await, before);
    // The admitted normal commit above remains historical, never undone to
    // counterfeit a rejection or refund the already settled synthetic call.
    assert!(receipt.decision_event_id != Uuid::nil());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn evaluation_budget_stop_does_not_pause_goal_or_count_no_progress(pool: PgPool) {
    let (s, lease, _, goal) = work(&pool).await;
    let before = s.snapshot().await.unwrap().unwrap();
    let original: Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM goals g WHERE id=$1")
        .bind(goal)
        .fetch_one(&pool)
        .await
        .unwrap();
    s.stop_cycle(&lease, StopReason::Budget).await.unwrap();
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().state_version,
        before.state_version
    );
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM goals g WHERE id=$1")
        .bind(goal)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, original);
    // Also exclude a legacy/faulty due timestamp; a failed comparison is not a
    // failed production goal cycle even when the shared run has a goal_id.
    sqlx::query("UPDATE runs SET goal_due_at=clock_timestamp() WHERE id=$1")
        .bind(lease.run_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.tick_goals().await.unwrap().is_none());
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM goals g WHERE id=$1")
        .bind(goal)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, original);
    let cycles: i64 = sqlx::query_scalar("SELECT count(*) FROM goal_cycles WHERE run_id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cycles, 0);
    let kind: String = sqlx::query_scalar(
        "SELECT kind FROM events WHERE correlation_id=$1 ORDER BY sequence DESC LIMIT 1",
    )
    .bind(lease.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kind, "development.trial_run_stopped");
}
