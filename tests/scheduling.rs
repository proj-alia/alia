use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        cognition::RunMode,
        config::{PersonaSeed, RuntimeRules},
    },
    runtime::{model::TokenUsage, scheduling::*},
};
use serde_json::json;
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
fn allowance() -> TokenAllowance {
    TokenAllowance {
        input_upper_bound: 100,
        output_cap: 50,
    }
}
async fn active(store: &Store) -> RunLease {
    store
        .submit_message(&Uuid::new_v4().to_string(), "测试预算与执行", None)
        .await
        .unwrap();
    store.claim_run(Uuid::new_v4()).await.unwrap().unwrap()
}
async fn reserve(store: &Store, lease: &RunLease) -> Reservation {
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    store
        .reserve_model_call(lease, Uuid::new_v4(), &context, allowance())
        .await
        .unwrap()
}
async fn expire(pool: &PgPool, lease: &RunLease) {
    sqlx::query(
        "UPDATE runs SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(lease.run_id)
    .execute(pool)
    .await
    .unwrap();
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn concurrent_claims_and_dispatch_are_single_winner(pool: PgPool) {
    let store = setup(&pool).await;
    store
        .submit_message("one", "第一个输入", None)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        store.claim_run(Uuid::new_v4()),
        store.claim_run(Uuid::new_v4())
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(usize::from(a.is_some()) + usize::from(b.is_some()), 1);
    let lease = a.or(b).unwrap();
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    let key = Uuid::new_v4();
    let (a, b) = tokio::join!(
        store.reserve_model_call(&lease, key, &context, allowance()),
        store.reserve_model_call(&lease, key, &context, allowance())
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.id, b.id);
    let (a, b) = tokio::join!(
        store.dispatch_model_call(&lease, a.id),
        store.dispatch_model_call(&lease, b.id)
    );
    assert_eq!(usize::from(a.unwrap()) + usize::from(b.unwrap()), 1);
    let calls: i32 = sqlx::query_scalar("SELECT model_calls FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(calls, 1);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn crash_before_dispatch_releases_and_after_dispatch_charges_full(pool: PgPool) {
    let store = setup(&pool).await;
    let old = active(&store).await;
    let unspent = reserve(&store, &old).await;
    expire(&pool, &old).await;
    let next = Store::new(pool.clone())
        .claim_run(Uuid::new_v4())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.run_id, old.run_id);
    assert!(next.generation > old.generation);
    assert_eq!(next.deadline, old.deadline);
    assert!(matches!(
        store.renew_lease(&old).await,
        Err(StoreError::LeaseLost)
    ));
    assert!(matches!(
        store.dispatch_model_call(&old, unspent.id).await,
        Err(StoreError::LeaseLost)
    ));
    let charge: i64 =
        sqlx::query_scalar("SELECT charged_tokens FROM usage_reservations WHERE id=$1")
            .bind(unspent.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(charge, 0);
    let sent = reserve(&store, &next).await;
    assert!(store.dispatch_model_call(&next, sent.id).await.unwrap());
    expire(&pool, &next).await;
    let recovered = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert!(recovered.generation > next.generation);
    let row: (i64, bool) =
        sqlx::query_as("SELECT charged_tokens,usage_unknown FROM usage_reservations WHERE id=$1")
            .bind(sent.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, (150, true));
    let late = store
        .settle_model_call(
            sent.id,
            Some(TokenUsage {
                input: 80,
                output: 20,
                total: 100,
            }),
        )
        .await
        .unwrap();
    assert_eq!(late.charged_tokens, 150);
    assert!(late.usage_unknown);
    let calls: i32 = sqlx::query_scalar("SELECT model_calls FROM runs WHERE id=$1")
        .bind(recovered.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(calls, 1);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn foreground_preempts_old_background_without_erasing_cost(pool: PgPool) {
    let store = setup(&pool).await;
    let background = store
        .submit_message("bg", "后台观察触发", None)
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET mode='background' WHERE id=$1")
        .bind(background.operation_id)
        .execute(&pool)
        .await
        .unwrap();
    let old = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(old.mode, RunMode::Background);
    let sent = reserve(&store, &old).await;
    store.dispatch_model_call(&old, sent.id).await.unwrap();
    let new = store
        .submit_message("fg", "现在先处理我的问题", None)
        .await
        .unwrap();
    assert!(matches!(
        store.renew_lease(&old).await,
        Err(StoreError::LeaseLost)
    ));
    let next = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(next.run_id, new.operation_id);
    assert_eq!(next.mode, RunMode::Foreground);
    let status: String = sqlx::query_scalar("SELECT status FROM runs WHERE id=$1")
        .bind(old.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "stale");
    let total: (i64, i64) =
        sqlx::query_as("SELECT charged_total,charged_background FROM budget_windows WHERE id=$1")
            .bind(sent.window_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total, (150, 150));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn reservations_enforce_total_budget_and_settle_exactly_once(pool: PgPool) {
    let store = setup(&pool).await;
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(jsonb_set(runtime_config,'{budgets,total_tokens_per_day}','200'),'{budgets,background_tokens_per_day}','180')").execute(&pool).await.unwrap();
    let lease = active(&store).await;
    let held = reserve(&store, &lease).await;
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    assert!(matches!(
        store
            .reserve_model_call(&lease, Uuid::new_v4(), &context, allowance())
            .await,
        Err(StoreError::CallLimit)
    ));
    store.dispatch_model_call(&lease, held.id).await.unwrap();
    let usage = Some(TokenUsage {
        input: 80,
        output: 20,
        total: 100,
    });
    let (a, b) = tokio::join!(
        store.settle_model_call(held.id, usage),
        store.settle_model_call(held.id, usage)
    );
    assert_eq!(a.unwrap().charged_tokens, 100);
    assert_eq!(b.unwrap().charged_tokens, 100);
    let total: i64 = sqlx::query_scalar("SELECT charged_total FROM budget_windows WHERE id=$1")
        .bind(held.window_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 100);
    assert!(matches!(
        store
            .reserve_model_call(&lease, Uuid::new_v4(), &context, allowance())
            .await,
        Err(StoreError::BudgetUnavailable)
    ));
    assert!(matches!(
        store
            .settle_model_call(
                held.id,
                Some(TokenUsage {
                    input: 99,
                    output: 1,
                    total: 100
                })
            )
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn background_has_separate_limit_and_missing_usage_is_not_free(pool: PgPool) {
    let store = setup(&pool).await;
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(jsonb_set(runtime_config,'{budgets,total_tokens_per_day}','1000'),'{budgets,background_tokens_per_day}','200')").execute(&pool).await.unwrap();
    let input = store.submit_message("bg", "后台触发", None).await.unwrap();
    sqlx::query("UPDATE runs SET mode='background' WHERE id=$1")
        .bind(input.operation_id)
        .execute(&pool)
        .await
        .unwrap();
    let lease = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let held = reserve(&store, &lease).await;
    store.dispatch_model_call(&lease, held.id).await.unwrap();
    let unknown = store.settle_model_call(held.id, None).await.unwrap();
    assert_eq!(unknown.charged_tokens, 150);
    assert!(unknown.usage_unknown);
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    assert!(matches!(
        store
            .reserve_model_call(&lease, Uuid::new_v4(), &context, allowance())
            .await,
        Err(StoreError::BudgetUnavailable)
    ));
    store.finish_run(&lease, RunOutcome::Failed).await.unwrap();
    let fg = active(&store).await;
    assert_eq!(reserve(&store, &fg).await.reserved_tokens, 150);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn provider_bound_breach_freezes_future_dispatch_even_if_total_is_small(pool: PgPool) {
    let store = setup(&pool).await;
    let lease = active(&store).await;
    let held = reserve(&store, &lease).await;
    store.dispatch_model_call(&lease, held.id).await.unwrap();
    store
        .settle_model_call(
            held.id,
            Some(TokenUsage {
                input: 101,
                output: 1,
                total: 102,
            }),
        )
        .await
        .unwrap();
    let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(frozen);
    let context = store.model_context(lease.run_id, vec![]).await.unwrap();
    assert!(matches!(
        store
            .reserve_model_call(&lease, Uuid::new_v4(), &context, allowance())
            .await,
        Err(StoreError::BudgetUnavailable)
    ));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn call_limit_survives_recovery_and_deadline_does_not_restart(pool: PgPool) {
    let store = setup(&pool).await;
    let lease = active(&store).await;
    for _ in 0..3 {
        let held = reserve(&store, &lease).await;
        store.dispatch_model_call(&lease, held.id).await.unwrap();
        store
            .settle_model_call(
                held.id,
                Some(TokenUsage {
                    input: 1,
                    output: 1,
                    total: 2,
                }),
            )
            .await
            .unwrap();
    }
    expire(&pool, &lease).await;
    let next = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(next.deadline, lease.deadline);
    let context = store.model_context(next.run_id, vec![]).await.unwrap();
    assert!(matches!(
        store
            .reserve_model_call(&next, Uuid::new_v4(), &context, allowance())
            .await,
        Err(StoreError::CallLimit)
    ));
    sqlx::query("UPDATE runs SET deadline=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(next.run_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.claim_run(Uuid::new_v4()).await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id=$1")
            .bind(next.run_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "failed"
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn pause_and_privacy_epoch_prevent_new_dispatch(pool: PgPool) {
    let store = setup(&pool).await;
    let input = store.submit_message("bg", "后台触发", None).await.unwrap();
    sqlx::query("UPDATE runs SET mode='background' WHERE id=$1")
        .bind(input.operation_id)
        .execute(&pool)
        .await
        .unwrap();
    let lease = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let held = reserve(&store, &lease).await;
    sqlx::query("UPDATE individuals SET autonomy_paused=true")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.dispatch_model_call(&lease, held.id).await,
        Err(StoreError::LeaseLost)
    ));
    assert!(store.claim_run(Uuid::new_v4()).await.unwrap().is_none());
    let fg = active(&store).await;
    assert_eq!(fg.mode, RunMode::Foreground);
    let held = reserve(&store, &fg).await;
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.dispatch_model_call(&fg, held.id).await,
        Err(StoreError::LeaseLost)
    ));
    assert!(store.claim_run(Uuid::new_v4()).await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT sum(charged_total)::bigint FROM budget_windows")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn timezone_change_does_not_create_extra_budget_before_window_ends(pool: PgPool) {
    let store = setup(&pool).await;
    let lease = active(&store).await;
    let held = reserve(&store, &lease).await;
    store.dispatch_model_call(&lease, held.id).await.unwrap();
    store.settle_model_call(held.id, None).await.unwrap();
    store.finish_run(&lease, RunOutcome::Failed).await.unwrap();
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{contact,timezone}',$1),config_version=config_version+1").bind(json!("Pacific/Auckland")).execute(&pool).await.unwrap();
    let next = active(&store).await;
    let second = reserve(&store, &next).await;
    assert_eq!(second.window_id, held.window_id);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT charged_total FROM budget_windows WHERE id=$1")
            .bind(held.window_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        150
    );
    for (day, hours) in [
        ("2026-03-08T12:00:00Z", 23.0),
        ("2026-11-01T12:00:00Z", 25.0),
    ] {
        let duration:f64=sqlx::query_scalar("SELECT EXTRACT(epoch FROM (((date_trunc('day',$1::timestamptz AT TIME ZONE 'America/New_York')+interval '1 day') AT TIME ZONE 'America/New_York')-(date_trunc('day',$1::timestamptz AT TIME ZONE 'America/New_York') AT TIME ZONE 'America/New_York')))::double precision/3600")
            .bind(day.parse::<chrono::DateTime<chrono::Utc>>().unwrap()).fetch_one(&pool).await.unwrap();
        assert_eq!(duration, hours);
    }
}

struct AccountingProvider {
    preempt: Option<Store>,
    sleep: bool,
}
#[async_trait::async_trait]
impl alia::runtime::model::ModelProvider for AccountingProvider {
    fn mode(&self) -> alia::runtime::model::ProviderMode {
        alia::runtime::model::ProviderMode::Fixture
    }
    fn token_allowance(
        &self,
        _: &alia::domain::cognition::ModelContext,
        _: alia::runtime::model::CallLimits,
    ) -> Result<TokenAllowance, alia::runtime::model::ModelFailure> {
        Ok(allowance())
    }
    async fn decide(
        &self,
        _: &alia::domain::cognition::ModelContext,
        _: alia::runtime::model::CallLimits,
        _: alia::runtime::model::ModelCallMetadata,
    ) -> Result<alia::runtime::model::ModelResult, alia::runtime::model::ModelFailure> {
        if self.sleep {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
        if let Some(store) = &self.preempt {
            store
                .submit_message("preemption", "新指令到达", None)
                .await
                .unwrap();
        }
        let proposal=alia::domain::cognition::DecisionProposal::parse(json!({"schema_version":1,"reply":"受控测试结果","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"测试"}).to_string().as_bytes()).unwrap();
        Ok(alia::runtime::model::ModelResult {
            proposal,
            usage: Some(TokenUsage {
                input: 80,
                output: 20,
                total: 100,
            }),
            provider_model: "accounting-test-double".into(),
            system_fingerprint: None,
            provider_mode: alia::runtime::model::ProviderMode::Fixture,
            http: None,
        })
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn managed_step_accounts_before_return_and_drops_preempted_result(pool: PgPool) {
    use alia::runtime::{
        execution::{StepError, model_step},
        model::CallLimits,
    };
    let store = setup(&pool).await;
    let lease = active(&store).await;
    let limits = CallLimits {
        max_output_tokens: 50,
        timeout_seconds: 5,
    };
    let ready = model_step(
        &store,
        &AccountingProvider {
            preempt: None,
            sleep: false,
        },
        &lease,
        vec![],
        limits,
    )
    .await
    .unwrap();
    assert_eq!(ready.result().proposal.reply, "受控测试结果");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT charged_total FROM budget_windows")
            .fetch_one(&pool)
            .await
            .unwrap(),
        100
    );
    let failure = model_step(
        &store,
        &AccountingProvider {
            preempt: Some(store.clone()),
            sleep: false,
        },
        &lease,
        vec![],
        limits,
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(failure, StepError::Storage(StoreError::LeaseLost)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT charged_total FROM budget_windows")
            .fetch_one(&pool)
            .await
            .unwrap(),
        200
    );
    assert!(store.memories().await.unwrap().is_empty());
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
async fn managed_timeout_charges_unknown_and_unverified_provider_never_dispatches(pool: PgPool) {
    use alia::runtime::{
        execution::{StepError, model_step},
        model::*,
    };
    let store = setup(&pool).await;
    let lease = active(&store).await;
    let failure = model_step(
        &store,
        &AccountingProvider {
            preempt: None,
            sleep: true,
        },
        &lease,
        vec![],
        CallLimits {
            max_output_tokens: 50,
            timeout_seconds: 1,
        },
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(
        failure,
        StepError::Model(ModelFailure {
            error: ModelError::Transport,
            ..
        })
    ));
    let row: (i64, bool) =
        sqlx::query_as("SELECT charged_tokens,usage_unknown FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, (150, true));
    let live = alia::adapters::model::ChatProvider::new(
        "https://api.deepseek.com",
        "deepseek-flash",
        "test-do-not-send",
        alia::adapters::model::ChatDialect::DeepSeek,
    )
    .unwrap();
    let failure = model_step(
        &store,
        &live,
        &lease,
        vec![],
        CallLimits {
            max_output_tokens: 50,
            timeout_seconds: 1,
        },
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(
        failure,
        StepError::Model(ModelFailure {
            error: ModelError::UnverifiedTokenBound,
            request_sent: false,
            ..
        })
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn foreground_queue_wins_and_never_started_inputs_rebase_without_reinitializing(
    pool: PgPool,
) {
    let store = setup(&pool).await;
    let bg = store.submit_message("bg", "后台排队", None).await.unwrap();
    sqlx::query("UPDATE runs SET mode='background' WHERE id=$1")
        .bind(bg.operation_id)
        .execute(&pool)
        .await
        .unwrap();
    let fg = store.submit_message("fg", "前台排队", None).await.unwrap();
    let lease = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(lease.run_id, fg.operation_id);
    store.finish_run(&lease, RunOutcome::Failed).await.unwrap();
    let next = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(next.run_id, bg.operation_id);
    assert_eq!(next.state_version, lease.state_version);
    assert_eq!(next.individual_id, lease.individual_id);
    assert!(
        store
            .model_context(next.run_id, vec![])
            .await
            .unwrap()
            .events
            .iter()
            .any(|e| e.id == bg.event_id)
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn blocked_manifest_cannot_dispatch_and_reconciliation_releases_reservation(pool: PgPool) {
    let store = setup(&pool).await;
    let old = store
        .submit_message("old", "即将删除的来源", None)
        .await
        .unwrap();
    let lease = active(&store).await;
    let held = reserve(&store, &lease).await;
    // The oldest foreground trigger is included in the new consistent context.
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(old.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.dispatch_model_call(&lease, held.id).await,
        Err(StoreError::LeaseLost)
    ));
    store.claim_run(Uuid::new_v4()).await.unwrap();
    let row: (String, i64) =
        sqlx::query_as("SELECT status,charged_tokens FROM usage_reservations WHERE id=$1")
            .bind(held.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, ("settled".into(), 0));
    assert!(matches!(
        store.finish_run(&lease, RunOutcome::Failed).await,
        Err(StoreError::LeaseLost)
    ));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn mode_and_complete_manifest_are_checked_before_budget_reservation(pool: PgPool) {
    let store = setup(&pool).await;
    let lease = active(&store).await;
    let mut wrong_mode = lease.clone();
    wrong_mode.mode = RunMode::Background;
    assert!(matches!(
        store.renew_lease(&wrong_mode).await,
        Err(StoreError::LeaseLost)
    ));
    let mut context = store.model_context(lease.run_id, vec![]).await.unwrap();
    let subject = Uuid::parse_str(context.relationship["subject_id"].as_str().unwrap()).unwrap();
    context.input_manifest.retain(|id| *id != subject);
    assert!(matches!(
        store
            .reserve_model_call(&lease, Uuid::new_v4(), &context, allowance())
            .await,
        Err(StoreError::Invalid("relationship manifest"))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
