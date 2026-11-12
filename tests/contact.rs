use alia::{
    adapters::store::{Store, StoreError},
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        contact::{ContactReceipt, ContactWait},
        execution::model_step,
        model::{CallLimits, FixtureProvider},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let store = Store::new(pool.clone());
    let seed = PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap();
    let mut config: Value =
        serde_json::from_slice(include_bytes!("../config/runtime.example.json")).unwrap();
    config["contact"]["quiet_start"] = json!("00:00");
    config["contact"]["quiet_end"] = json!("00:00");
    let rules = RuntimeRules::validate(config, IMPLEMENTED_CAPABILITIES).unwrap();
    store.initialize("create", &seed, &rules).await.unwrap();
    store
}
// Explicit synthetic background observation: creates no real autonomous goal.
// The actual model_step, decision transaction and delivery storage are exercised.
async fn draft(pool: &PgPool, store: &Store) -> Uuid {
    let input = store
        .submit_message(&Uuid::new_v4().to_string(), "测试环境观察", None)
        .await
        .unwrap();
    sqlx::query("UPDATE events SET kind='test.observation' WHERE id=(SELECT trigger_event_id FROM runs WHERE id=$1)").bind(input.operation_id).execute(pool).await.unwrap();
    sqlx::query("UPDATE runs SET mode='background' WHERE id=$1")
        .bind(input.operation_id)
        .execute(pool)
        .await
        .unwrap();
    let lease = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let p = json!({"schema_version":1,"reply":"有一个带来源的发现可以分享。","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"测试应用内分享"});
    let ready = model_step(
        store,
        &FixtureProvider {
            response: p.to_string(),
        },
        &lease,
        vec![],
        CallLimits::default(),
    )
    .await
    .unwrap();
    store
        .commit_decision(&lease, &ready, &[])
        .await
        .unwrap()
        .message_id
        .unwrap()
}
fn delivered(r: ContactReceipt) -> Uuid {
    match r {
        ContactReceipt::Delivered { message_id } => message_id,
        _ => panic!("expected delivery, got {r:?}"),
    }
}
async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn concurrent_delivery_replays_after_all_connections_close(
    options: sqlx::postgres::PgPoolOptions,
    connection: sqlx::postgres::PgConnectOptions,
) {
    let pool = options.connect_with(connection.clone()).await.unwrap();
    let store = setup(&pool).await;
    let id = draft(&pool, &store).await;
    let (a, b) = tokio::join!(store.deliver_proactive(id), store.deliver_proactive(id));
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    let message = delivered(a.clone());
    assert_eq!(
        count(
            &pool,
            "SELECT sum(delivered_count)::bigint FROM contact_windows"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='message.sent'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM outbox b JOIN events e ON e.id=b.resource_id WHERE b.event_type='message.created' AND e.kind='message.sent'"
        )
        .await,
        1
    );
    assert_eq!(count(&pool,"SELECT count(*) FROM relationship_events e JOIN events v ON v.id=e.event_id WHERE v.kind='message.sent'").await,1);
    let edge: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM derivation_edges WHERE source_id=$1 AND target_id=$2)",
    )
    .bind(id)
    .bind(message)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(edge);
    pool.close().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_with(connection)
        .await
        .unwrap();
    assert_eq!(Store::new(pool).deliver_proactive(id).await.unwrap(), a);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn distinct_drafts_share_interval_and_daily_quota_even_after_content_removal(pool: PgPool) {
    let store = setup(&pool).await;
    let first = draft(&pool, &store).await;
    let second = draft(&pool, &store).await;
    let (a, b) = tokio::join!(
        store.deliver_proactive(first),
        store.deliver_proactive(second)
    );
    let results = [a.unwrap(), b.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, ContactReceipt::Delivered { .. }))
            .count(),
        1
    );
    assert!(results.contains(&ContactReceipt::Deferred {
        reason: ContactWait::MinimumInterval
    }));
    let pending = if matches!(results[0], ContactReceipt::Deferred { .. }) {
        first
    } else {
        second
    };
    sqlx::query(
        "UPDATE contact_windows SET last_delivered_at=clock_timestamp()-interval '4 hours'",
    )
    .execute(&pool)
    .await
    .unwrap();
    delivered(store.deliver_proactive(pending).await.unwrap());
    let third = draft(&pool, &store).await;
    sqlx::query("UPDATE events SET payload='{}' WHERE kind='message.sent'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE contact_windows SET last_delivered_at=clock_timestamp()-interval '4 hours'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        store.deliver_proactive(third).await.unwrap(),
        ContactReceipt::Deferred {
            reason: ContactWait::DailyLimit
        }
    );
    assert_eq!(
        count(
            &pool,
            "SELECT sum(delivered_count)::bigint FROM contact_windows"
        )
        .await,
        2
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn quiet_hours_pause_and_disabled_autonomy_defer_without_delivery(pool: PgPool) {
    let store = setup(&pool).await;
    let id = draft(&pool, &store).await;
    for assignment in [
        "autonomy_paused=true",
        "lifecycle='paused'",
        "runtime_config=jsonb_set(runtime_config,'{autonomy,enabled}','false')",
    ] {
        sqlx::query(&format!("UPDATE individuals SET {assignment}"))
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            store.deliver_proactive(id).await.unwrap(),
            ContactReceipt::Deferred {
                reason: ContactWait::Paused
            }
        );
        sqlx::query("UPDATE individuals SET autonomy_paused=false,lifecycle='active',runtime_config=jsonb_set(runtime_config,'{autonomy,enabled}','true')").execute(&pool).await.unwrap();
    }
    // Bound the current UTC minute with a wrapping quiet interval; no clock override.
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(jsonb_set(jsonb_set(runtime_config,'{contact,timezone}','\"UTC\"'),'{contact,quiet_start}',to_jsonb(to_char(clock_timestamp() AT TIME ZONE 'UTC'-interval '1 hour','HH24:MI'))),'{contact,quiet_end}',to_jsonb(to_char(clock_timestamp() AT TIME ZONE 'UTC'+interval '1 hour','HH24:MI')))").execute(&pool).await.unwrap();
    assert_eq!(
        store.deliver_proactive(id).await.unwrap(),
        ContactReceipt::Deferred {
            reason: ContactWait::QuietHours
        }
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='message.sent'"
        )
        .await,
        0
    );
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(jsonb_set(runtime_config,'{contact,quiet_start}','\"00:00\"'),'{contact,quiet_end}','\"00:00\"')").execute(&pool).await.unwrap();
    delivered(store.deliver_proactive(id).await.unwrap());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn new_input_and_deleted_ancestors_cancel_stale_drafts(pool: PgPool) {
    let store = setup(&pool).await;
    let old = draft(&pool, &store).await;
    store
        .submit_message("new", "现在先不要继续旧话题", None)
        .await
        .unwrap();
    assert_eq!(
        store.deliver_proactive(old).await.unwrap(),
        ContactReceipt::Cancelled {
            reason: "new_input".into()
        }
    );
    sqlx::query("UPDATE runs SET status='cancelled' WHERE status='queued'")
        .execute(&pool)
        .await
        .unwrap();
    let id = draft(&pool, &store).await;
    // Block a grandparent, not the draft. The recursive check must catch it.
    sqlx::query("UPDATE objects SET blocked=true WHERE id=(SELECT trigger_event_id FROM runs WHERE id=(SELECT run_id FROM outgoing_messages WHERE draft_event_id=$1))").bind(id).execute(&pool).await.unwrap();
    let result = store.deliver_proactive(id).await.unwrap();
    assert_eq!(
        result,
        ContactReceipt::Cancelled {
            reason: "source_unavailable".into()
        }
    );
    assert_eq!(store.deliver_proactive(id).await.unwrap(), result);
    let epoch = draft(&pool, &store).await;
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store.deliver_proactive(epoch).await.unwrap(),
        ContactReceipt::Cancelled {
            reason: "source_unavailable".into()
        }
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='message.sent'"
        )
        .await,
        0
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn fault_during_outbox_rolls_back_receipt_history_and_quota(pool: PgPool) {
    let store = setup(&pool).await;
    let id = draft(&pool, &store).await;
    let version = store.snapshot().await.unwrap().unwrap().state_version;
    sqlx::raw_sql("CREATE FUNCTION fail_contact() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='message.created' THEN RAISE EXCEPTION 'test fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER contact_fault BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION fail_contact();").execute(&pool).await.unwrap();
    assert!(matches!(
        store.deliver_proactive(id).await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(
        count(&pool, "SELECT count(*) FROM contact_windows").await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='message.sent'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM outgoing_messages WHERE status='pending'"
        )
        .await,
        1
    );
    assert_eq!(
        store.snapshot().await.unwrap().unwrap().state_version,
        version
    );
    sqlx::query("DROP TRIGGER contact_fault ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    delivered(store.deliver_proactive(id).await.unwrap());
    assert_eq!(
        count(
            &pool,
            "SELECT sum(delivered_count)::bigint FROM contact_windows"
        )
        .await,
        1
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn timezone_change_cannot_reset_open_quota_and_clock_rollback_defers(pool: PgPool) {
    let store = setup(&pool).await;
    let id = draft(&pool, &store).await;
    delivered(store.deliver_proactive(id).await.unwrap());
    let id = draft(&pool, &store).await;
    sqlx::query("UPDATE contact_windows SET delivered_count=2,last_delivered_at=clock_timestamp()-interval '4 hours';").execute(&pool).await.unwrap();
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{contact,timezone}','\"Pacific/Kiritimati\"')").execute(&pool).await.unwrap();
    assert_eq!(
        store.deliver_proactive(id).await.unwrap(),
        ContactReceipt::Deferred {
            reason: ContactWait::DailyLimit
        }
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM contact_windows").await,
        1
    );
    sqlx::query("UPDATE contact_windows SET delivered_count=0,last_delivered_at=clock_timestamp()+interval '1 hour'").execute(&pool).await.unwrap();
    assert_eq!(
        store.deliver_proactive(id).await.unwrap(),
        ContactReceipt::Deferred {
            reason: ContactWait::ClockRollback
        }
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn expired_day_opens_new_window_but_keeps_interval_and_handles_dst(pool: PgPool) {
    let store = setup(&pool).await;
    let id = draft(&pool, &store).await;
    delivered(store.deliver_proactive(id).await.unwrap());
    let id = draft(&pool, &store).await;
    // Simulated day boundary in durable state, not a real-time observation claim.
    sqlx::query("UPDATE contact_windows SET starts_at=clock_timestamp()-interval '25 hours',ends_at=clock_timestamp()-interval '1 second',delivered_count=2").execute(&pool).await.unwrap();
    assert_eq!(
        store.deliver_proactive(id).await.unwrap(),
        ContactReceipt::Deferred {
            reason: ContactWait::MinimumInterval
        }
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM contact_windows").await,
        2
    );
    sqlx::query("UPDATE contact_windows SET last_delivered_at=clock_timestamp()-interval '4 hours' WHERE last_delivered_at IS NOT NULL").execute(&pool).await.unwrap();
    delivered(store.deliver_proactive(id).await.unwrap());
    for (date, hours) in [("2026-03-08", 23), ("2026-11-01", 25)] {
        let seconds:f64=sqlx::query_scalar("SELECT extract(epoch FROM ((($1::date+1)::timestamp AT TIME ZONE 'America/New_York')-($1::date::timestamp AT TIME ZONE 'America/New_York')))::float8").bind(date).fetch_one(&pool).await.unwrap();
        assert_eq!(seconds, hours as f64 * 3600.0);
    }
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn busy_zero_allowance_archived_and_deleted_receipts_never_send(pool: PgPool) {
    let store = setup(&pool).await;
    let id = draft(&pool, &store).await;
    let work = store
        .submit_message("busy", "测试正在运行的后台任务", None)
        .await
        .unwrap();
    sqlx::query("UPDATE events SET kind='test.observation' WHERE id=(SELECT trigger_event_id FROM runs WHERE id=$1)").bind(work.operation_id).execute(&pool).await.unwrap();
    // Queued foreground work has priority even before it acquires its lease.
    assert_eq!(
        store.deliver_proactive(id).await.unwrap(),
        ContactReceipt::Deferred {
            reason: ContactWait::Busy
        }
    );
    sqlx::query("UPDATE runs SET mode='background' WHERE id=$1")
        .bind(work.operation_id)
        .execute(&pool)
        .await
        .unwrap();
    let lease = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(
        store.deliver_proactive(id).await.unwrap(),
        ContactReceipt::Deferred {
            reason: ContactWait::Busy
        }
    );
    sqlx::query(
        "UPDATE runs SET status='cancelled',lease_owner=NULL,lease_expires_at=NULL WHERE id=$1",
    )
    .bind(lease.run_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{contact,max_proactive_per_day}','0')").execute(&pool).await.unwrap();
    assert_eq!(
        store.deliver_proactive(id).await.unwrap(),
        ContactReceipt::Deferred {
            reason: ContactWait::DailyLimit
        }
    );
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{contact,max_proactive_per_day}','2')").execute(&pool).await.unwrap();
    let message = delivered(store.deliver_proactive(id).await.unwrap());
    let archived = draft(&pool, &store).await;
    sqlx::query("UPDATE individuals SET lifecycle='archived'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store.deliver_proactive(archived).await.unwrap(),
        ContactReceipt::Cancelled {
            reason: "inactive".into()
        }
    );
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(message)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.deliver_proactive(id).await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='message.sent'"
        )
        .await,
        1
    );
}
