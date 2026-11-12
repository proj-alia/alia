use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        beliefs::{BeliefDraft, BeliefPosition},
        config::{PersonaSeed, RuntimeRules},
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use sqlx::PgPool;
use uuid::Uuid;

async fn setup(pool: &PgPool) -> Store {
    let store = Store::new(pool.clone());
    store
        .initialize(
            "init",
            &PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap(),
            &RuntimeRules::parse(
                include_bytes!("../config/runtime.example.json"),
                IMPLEMENTED_CAPABILITIES,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    store
}
fn draft(event: Uuid) -> BeliefDraft {
    BeliefDraft {
        statement: "对方通常希望先研究".into(),
        scope: "general".into(),
        position: BeliefPosition::Tentative,
        support_event_ids: vec![event],
        counter_event_ids: vec![],
        supersedes: None,
        brief_basis: "依据已收到的陈述，尚非独立核实事实".into(),
    }
}
async fn counts(pool: &PgPool) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM beliefs),(SELECT count(*) FROM events),(SELECT count(*) FROM outbox),(SELECT count(*) FROM idempotency_records)")
        .fetch_one(pool).await.unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn belief_reconsideration_preserves_history_counterevidence_and_original_events(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = s
        .submit_message("source", "我一般希望先研究", None)
        .await
        .unwrap()
        .event_id;
    let first = s
        .record_belief(
            "first",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[source],
            &draft(source),
        )
        .await
        .unwrap();
    let original: serde_json::Value = sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap();
    let counter = s
        .submit_message("counter", "重新审视：以前的表述不能覆盖全部情况", None)
        .await
        .unwrap()
        .event_id;
    let mut changed = draft(source);
    changed.statement = "先前的一般理解需要重新核实".into();
    changed.position = BeliefPosition::Questioned;
    changed.counter_event_ids = vec![counter];
    changed.supersedes = Some(first.belief_id);
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let inputs = [first.belief_id, source, counter];
    let receipt = s
        .record_belief("revise", expected, &inputs, &changed)
        .await
        .unwrap();
    assert_ne!(first.belief_id, receipt.belief_id);
    assert_eq!(receipt.version, expected.version + 1);
    assert_eq!(
        s.record_belief("revise", expected, &inputs, &changed)
            .await
            .unwrap(),
        receipt
    );
    let history = Store::new(pool.clone()).beliefs(true).await.unwrap();
    assert_eq!(history.len(), 2);
    let old = history
        .iter()
        .find(|b| b["id"] == first.belief_id.to_string())
        .unwrap();
    assert_eq!(old["statement"], "对方通常希望先研究");
    assert_eq!(old["status"], "superseded");
    assert_eq!(old["version"], 2);
    let current = s.beliefs(false).await.unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0]["position"], "questioned");
    assert_eq!(
        current[0]["counter_event_ids"],
        serde_json::json!([counter])
    );
    assert_eq!(current[0]["supersedes"], first.belief_id.to_string());
    let after: serde_json::Value = sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, original);
    let kind: String = sqlx::query_scalar("SELECT source_kind FROM events WHERE id=$1")
        .bind(receipt.event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        kind, "inference",
        "beliefs must not promote an interpretation to a factual observation"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn belief_local_exception_and_stale_context_cannot_overwrite_general_understanding(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = s
        .submit_message("source", "先研究", None)
        .await
        .unwrap()
        .event_id;
    let first = s
        .record_belief(
            "first",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[source],
            &draft(source),
        )
        .await
        .unwrap();
    let before = s.snapshot().await.unwrap().unwrap().expected();
    let initial = counts(&pool).await;
    let mut local = draft(source);
    local.scope = "task:prototype".into();
    local.supersedes = Some(first.belief_id);
    assert!(matches!(
        s.record_belief("exception", before, &[source, first.belief_id], &local)
            .await,
        Err(StoreError::Invalid(_))
    ));
    assert_eq!(counts(&pool).await, initial);
    s.submit_message("new-input", "新的当前信息", None)
        .await
        .unwrap();
    assert!(matches!(
        s.record_belief("stale", before, &[source], &draft(source))
            .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(s.beliefs(false).await.unwrap().len(), 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn revoked_belief_ancestor_blocks_history_new_derivatives_and_idempotent_receipts(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = s
        .submit_message("source", "合成私人来源", None)
        .await
        .unwrap()
        .event_id;
    let first = s
        .record_belief(
            "first",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[source],
            &draft(source),
        )
        .await
        .unwrap();
    let counter = s
        .submit_message("counter", "新观察", None)
        .await
        .unwrap()
        .event_id;
    let mut revision = draft(counter);
    revision.supersedes = Some(first.belief_id);
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let inputs = [first.belief_id, counter];
    let next = s
        .record_belief("second", expected, &inputs, &revision)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.beliefs(true).await.unwrap().is_empty());
    assert!(matches!(
        s.record_belief("second", expected, &inputs, &revision)
            .await,
        Err(StoreError::NotFound)
    ));
    let before = counts(&pool).await;
    assert!(matches!(
        s.record_belief(
            "derived",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[next.event_id],
            &draft(next.event_id)
        )
        .await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(counts(&pool).await, before);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn failed_final_belief_publication_rolls_back_revision_and_all_side_effects(pool: PgPool) {
    let s = setup(&pool).await;
    let source = s
        .submit_message("source", "最初陈述", None)
        .await
        .unwrap()
        .event_id;
    let first = s
        .record_belief(
            "first",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[source],
            &draft(source),
        )
        .await
        .unwrap();
    let new = s
        .submit_message("new-source", "需要修订", None)
        .await
        .unwrap()
        .event_id;
    let mut revision = draft(new);
    revision.supersedes = Some(first.belief_id);
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let before = counts(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION reject_belief_publish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'injected final publication failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_belief_publish BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION reject_belief_publish();").execute(&pool).await.unwrap();
    assert!(matches!(
        s.record_belief("revision", expected, &[first.belief_id, new], &revision)
            .await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(counts(&pool).await, before);
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().state_version,
        expected.version
    );
    assert_eq!(s.beliefs(false).await.unwrap()[0]["version"], 1);
    sqlx::query("DROP TRIGGER reject_belief_publish ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    s.record_belief("revision", expected, &[first.belief_id, new], &revision)
        .await
        .unwrap();
    assert_eq!(s.beliefs(false).await.unwrap().len(), 1);
    assert_eq!(s.beliefs(true).await.unwrap().len(), 2);
}
