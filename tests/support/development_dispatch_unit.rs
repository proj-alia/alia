use crate as alia;
include!("development_context_fixture.rs");
use crate::{
    adapters::store::StoreError,
    domain::{cognition::ModelContext, development_trials::*},
    runtime::{
        execution::{StepError, model_step},
        model::{CallLimits, FixtureProvider, ModelResult, ProviderMode, TokenUsage},
        scheduling::{RunLease, TokenAllowance},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
include!("development_trial_run_fixture.rs");

async fn prepare_side(
    s: &Store,
    lease: &RunLease,
    work: &TrialWork,
    side: &TrialBinding,
) -> ModelContext {
    crate::adapters::development_trial_context::prepare_case_context(
        s,
        lease,
        work,
        side,
        s.capabilities().await.unwrap(),
    )
    .await
    .unwrap()
}

async fn invoke(
    s: &Store,
    lease: &RunLease,
    context: &ModelContext,
    key: Uuid,
    polled: Arc<AtomicUsize>,
) -> Result<(ModelResult, Uuid), StepError> {
    super::accounted_call_with_request_key(
        s,
        lease,
        context,
        key,
        TokenAllowance {
            input_upper_bound: 100,
            output_cap: 50,
        },
        CallLimits {
            max_output_tokens: 50,
            timeout_seconds: 10,
        },
        ProviderMode::Fixture,
        async move {
            polled.fetch_add(1, Ordering::SeqCst);
            Ok(ModelResult {
                proposal: crate::domain::cognition::DecisionProposal::parse(
                    no_changes().to_string().as_bytes(),
                )
                .unwrap(),
                usage: Some(TokenUsage {
                    input: 1,
                    output: 1,
                    total: 2,
                }),
                provider_model: "explicit-synthetic-trial".into(),
                system_fingerprint: None,
                provider_mode: ProviderMode::Fixture,
                http: None,
            })
        },
        |r| r.usage,
    )
    .await
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn paired_trial_calls_use_original_ledger_and_fixture_never_certifies_growth(pool: PgPool) {
    let (s, lease, _, _) = work(&pool).await;
    let work = s
        .claim_development_trial_work(&lease)
        .await
        .unwrap()
        .unwrap();
    let polled = Arc::new(AtomicUsize::new(0));
    let initial = s.snapshot().await.unwrap().unwrap();
    for binding in [&work.baseline, &work.trial] {
        let context = prepare_side(&s, &lease, &work, binding).await;
        let allowance = TokenAllowance {
            input_upper_bound: 100,
            output_cap: 50,
        };
        let key = s
            .bind_development_trial_request(&lease, binding, &context, allowance)
            .await
            .unwrap();
        assert_eq!(key, binding.attempt_id);
        let (result, reservation) = invoke(&s, &lease, &context, key, polled.clone())
            .await
            .unwrap();
        let attempt = s
            .complete_development_trial_attempt(&lease, binding, reservation, &context, &result)
            .await
            .unwrap();
        assert_eq!(attempt.state, TrialAttemptState::Recorded);
        assert!(!attempt.production_measurement);
        assert_eq!(attempt.outcome, Outcome::Unverified);
        assert!(matches!(
            invoke(&s, &lease, &context, key, polled.clone()).await,
            Err(StepError::AlreadyDispatched)
        ));
    }
    assert_eq!(polled.load(Ordering::SeqCst), 2);
    let charges: (i64, i64) = sqlx::query_as("SELECT count(*),sum(charged_tokens)::bigint FROM usage_reservations WHERE run_id=$1 AND status='settled'")
        .bind(lease.run_id).fetch_one(&pool).await.unwrap();
    assert_eq!(charges, (2, 4));
    let calls: i32 = sqlx::query_scalar("SELECT model_calls FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(calls, 2);
    let after = s.snapshot().await.unwrap().unwrap();
    assert_eq!(after.state_version, initial.state_version);
    assert_eq!(after.persona, initial.persona);
    let messages: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM events WHERE correlation_id=$1 AND kind='message.sent'",
    )
    .bind(lease.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(messages, 0);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn mode_attestation_and_dispatch_marker_rollback_together_before_polling_provider(
    pool: PgPool,
) {
    let (s, lease, _, _) = work(&pool).await;
    let work = s
        .claim_development_trial_work(&lease)
        .await
        .unwrap()
        .unwrap();
    let context = prepare_side(&s, &lease, &work, &work.baseline).await;
    let allowance = TokenAllowance {
        input_upper_bound: 100,
        output_cap: 50,
    };
    let key = s
        .bind_development_trial_request(&lease, &work.baseline, &context, allowance)
        .await
        .unwrap();
    let reservation = s
        .reserve_model_call(&lease, key, &context, allowance)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.dispatch_model_call(&lease, reservation.id).await,
        Err(StoreError::Invalid("evaluation run requires trial runtime"))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("CREATE FUNCTION reject_trial_dispatch() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'synthetic dispatch rollback'; END$$")
        .execute(&pool).await.unwrap();
    sqlx::query(&format!("CREATE TRIGGER reject_trial_dispatch BEFORE UPDATE ON usage_reservations FOR EACH ROW WHEN (NEW.id='{}'::uuid AND NEW.status='dispatched') EXECUTE FUNCTION reject_trial_dispatch()",reservation.id))
        .execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    let polled = Arc::new(AtomicUsize::new(0));
    assert!(matches!(
        invoke(&s, &lease, &context, key, polled.clone()).await,
        Err(StepError::Storage(StoreError::Database(_)))
    ));
    assert_eq!(polled.load(Ordering::SeqCst), 0);
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("DROP TRIGGER reject_trial_dispatch ON usage_reservations")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION reject_trial_dispatch()")
        .execute(&pool)
        .await
        .unwrap();
    invoke(&s, &lease, &context, key, polled.clone())
        .await
        .unwrap();
    assert_eq!(polled.load(Ordering::SeqCst), 1);
    let actual: (Option<String>, String) = sqlx::query_as("SELECT a.dispatch_mode,u.status FROM development_trial_attempts a JOIN usage_reservations u ON u.id=a.reservation_id WHERE a.id=$1")
        .bind(work.baseline.attempt_id).fetch_one(&pool).await.unwrap();
    assert_eq!(actual, (Some("fixture".into()), "settled".into()));
}
