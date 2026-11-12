//! Shared transaction ownership over actual PostgreSQL; fixture is synthetic.
// Only the proposal subset of this shared development fixture is needed here.
#![allow(dead_code)]
include!("support/development_context_fixture.rs");
use alia::adapters::development::propose_in_tx;
use alia::adapters::development::target_catalog_in;
use alia::adapters::development::transition_in_tx;

#[sqlx::test]
#[ignore = "requires isolated schema29 PostgreSQL; synthetic registered observations"]
async fn transition_kernel_rolls_back_applied_persona_and_head_without_top_level_receipt(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let p = propose(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::Traits, traits(0.61)),
    )
    .await;
    trial(&s, &pool, p.development_id).await;
    let review = review(&pool, p.development_id, verifier, Outcome::Passed).await;
    let validated = transition(&s, &pool, p.development_id, &review).await;
    let current = s.snapshot().await.unwrap().unwrap();
    let command = DevelopmentCommand::Accept;
    let inputs = refs(&pool, p.development_id, &command).await;
    let before = fingerprint(&pool).await;
    let outside: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM outbox),(SELECT count(*) FROM idempotency_records)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let applied = transition_in_tx(
        &mut tx,
        current.id,
        &inputs,
        p.development_id,
        validated.development_version,
        &command,
        None,
    )
    .await
    .unwrap();
    assert_eq!(applied.status, "accepted");
    assert!(applied.revision_id.is_some());
    let local: (Value, i64, i64, i64) = sqlx::query_as("SELECT persona,state_version,(SELECT count(*) FROM outbox),(SELECT count(*) FROM idempotency_records) FROM individuals WHERE id=$1")
        .bind(current.id).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(local.0["personality"]["traits"], traits(0.61));
    assert_eq!(
        (local.1, local.2, local.3),
        (current.state_version, outside.0, outside.1)
    );
    assert!(sqlx::query("SELECT 1/0").execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(fingerprint(&pool).await, before);
    let receipt = s
        .transition_development(
            "accept-after-outer-abort",
            current.expected(),
            &inputs,
            p.development_id,
            validated.development_version,
            &command,
        )
        .await
        .unwrap();
    assert_eq!(receipt.state_version, current.state_version + 1);
    assert_eq!(receipt.status, "accepted");
}

#[sqlx::test]
#[ignore = "requires isolated schema29 PostgreSQL; no provider calls"]
async fn target_catalog_is_read_only_closed_and_uses_actual_run_scope(pool: PgPool) {
    let s = setup(&pool).await;
    let input = s
        .submit_message("catalog-input", "合成目录读取", None)
        .await
        .unwrap();
    let current = s.snapshot().await.unwrap().unwrap();
    let before = fingerprint(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .unwrap();
    let catalog = target_catalog_in(&mut tx, current.id, input.operation_id, None)
        .await
        .unwrap();
    assert_eq!(catalog.len(), 5);
    assert_eq!(
        catalog
            .iter()
            .filter(|v| v.target != DevelopmentTarget::TaskStrategy)
            .count(),
        4
    );
    for v in &catalog {
        assert_eq!(v.before_version, 1);
        assert!(v.target_id.is_none() && v.head_revision_id.is_none());
        let mut value = serde_json::to_value(v).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 5);
        assert!(value.get("value").is_none());
        value["value"] = json!("not in metadata contract");
        assert!(serde_json::from_value::<DevelopmentTargetView>(value).is_err());
    }
    assert_eq!(
        catalog.last().unwrap().scope,
        Some(StrategyScope::Task(input.operation_id))
    );
    assert!(
        target_catalog_in(
            &mut tx,
            current.id,
            input.operation_id,
            Some(Uuid::new_v4())
        )
        .await
        .is_err()
    );
    assert!(
        target_catalog_in(&mut tx, current.id, Uuid::new_v4(), None)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "requires isolated schema29 PostgreSQL; synthetic accepted/removed metadata"]
async fn target_catalog_returns_real_head_and_next_removed_generation_without_old_ids(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let accepted = accepted(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::Traits, traits(0.61)),
        verifier,
    )
    .await;
    let current = s.snapshot().await.unwrap().unwrap();
    let run: Uuid = sqlx::query_scalar("SELECT id FROM runs WHERE trigger_event_id=$1")
        .bind(e)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .unwrap();
    let catalog = target_catalog_in(&mut tx, current.id, run, None)
        .await
        .unwrap();
    let head = catalog
        .iter()
        .find(|v| v.target == DevelopmentTarget::Traits)
        .unwrap();
    let old_target = head.target_id.unwrap();
    assert_eq!(head.before_version, 2);
    assert_eq!(head.head_revision_id, accepted.revision_id);
    tx.commit().await.unwrap();

    // This is a synthetic post-removal projection for catalog version arithmetic,
    // not a substitute for the separate real deletion/rebuild integration tests.
    for table in [
        "development_targets",
        "developments",
        "development_revisions",
    ] {
        sqlx::query(&format!("UPDATE {table} SET provenance_removed=true"))
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE individuals SET persona=seed")
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .unwrap();
    let catalog = target_catalog_in(&mut tx, current.id, run, None)
        .await
        .unwrap();
    let fresh = catalog
        .iter()
        .find(|v| v.target == DevelopmentTarget::Traits)
        .unwrap();
    assert_eq!(fresh.before_version, 3);
    assert!(fresh.target_id.is_none() && fresh.head_revision_id.is_none());
    assert!(
        !serde_json::to_string(&catalog)
            .unwrap()
            .contains(&old_target.to_string())
    );
    assert!(
        !serde_json::to_string(&catalog)
            .unwrap()
            .contains(&accepted.development_id.to_string())
    );
    tx.commit().await.unwrap();
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "requires isolated schema29 PostgreSQL; no provider calls"]
async fn proposed_kernel_leaves_global_commit_to_caller_and_rolls_back_late_failure(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let current = s.snapshot().await.unwrap().unwrap();
    let d = draft(e, DevelopmentTarget::Traits, traits(0.61));
    let inputs = propose_refs(&pool, &d, current.id).await;
    let before = fingerprint(&pool).await;
    let outside: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM outbox),(SELECT count(*) FROM idempotency_records)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let proposed = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        propose_in_tx(&mut tx, current.id, &inputs, current.id, &d, None),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(proposed.development_version, 1);
    let actual: (Uuid, Uuid, String, i64) = sqlx::query_as(
        "SELECT target_id,created_event_id,status,version FROM developments WHERE id=$1",
    )
    .bind(proposed.development_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        actual,
        (proposed.target_id, proposed.event_id, "proposed".into(), 1)
    );
    let local: (i64, Value, i64, i64) = sqlx::query_as("SELECT state_version,persona,(SELECT count(*) FROM outbox),(SELECT count(*) FROM idempotency_records) FROM individuals WHERE id=$1")
        .bind(current.id).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(
        local,
        (
            current.state_version,
            current.persona.clone(),
            outside.0,
            outside.1
        )
    );
    assert!(sqlx::query("SELECT 1/0").execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(fingerprint(&pool).await, before);

    // A later normal call can still allocate the same logical slot and use the
    // original wrapper's single global version, outbox and stable receipt.
    let receipt = s
        .propose_development(
            "after-outer-abort",
            current.expected(),
            &inputs,
            current.id,
            &d,
        )
        .await
        .unwrap();
    assert_eq!(receipt.state_version, current.state_version + 1);
    let committed = fingerprint(&pool).await;
    assert_eq!(
        s.propose_development(
            "after-outer-abort",
            current.expected(),
            &inputs,
            current.id,
            &d
        )
        .await
        .unwrap(),
        receipt
    );
    assert_eq!(fingerprint(&pool).await, committed);
}

#[sqlx::test]
#[ignore = "requires isolated schema29 PostgreSQL; no provider calls"]
async fn invalid_second_proposal_rolls_back_the_whole_outer_transaction(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let current = s.snapshot().await.unwrap().unwrap();
    let d = draft(e, DevelopmentTarget::Traits, traits(0.61));
    let inputs = propose_refs(&pool, &d, current.id).await;
    let before = fingerprint(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let first = propose_in_tx(
        &mut tx,
        current.id,
        &inputs,
        current.id,
        &d,
        Some("outer-proposal"),
    )
    .await
    .unwrap();
    let mut invalid = d.clone();
    invalid.before_version += 1;
    assert!(
        propose_in_tx(&mut tx, current.id, &inputs, current.id, &invalid, None)
            .await
            .is_err()
    );
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM developments WHERE id=$1)")
        .bind(first.development_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(exists);
    tx.rollback().await.unwrap();
    assert_eq!(fingerprint(&pool).await, before);
}
