use alia::{
    adapters::store::{Store, StoreError},
    domain::config::{PersonaSeed, RuntimeRules},
    domain::history::{MemoryKind, NewMemory, SourceKind},
};
use serde_json::json;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

fn seed() -> PersonaSeed {
    PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap()
}
fn config() -> RuntimeRules {
    RuntimeRules::parse(
        include_bytes!("../config/runtime.example.json"),
        &[
            "memory.search",
            "documents.read",
            "notes.write",
            "web.read_allowed",
        ],
    )
    .unwrap()
}
fn memory(source: Uuid, content: &str, scope: &str) -> NewMemory {
    NewMemory {
        content: content.into(),
        kind: MemoryKind::Semantic,
        scope: scope.into(),
        source_event_ids: vec![source],
        supersedes: None,
    }
}

// A real two-generation provenance chain, plus an independent public branch.
// Blocking the original source must also hide summaries whose own object is
// still unblocked; treating only the immediate row as private is insufficient.
struct InternalPrivacyFixture {
    store: Store,
    source_event: Uuid,
    first: alia::domain::history::RecordedMemories,
    second: alia::domain::history::RecordedMemories,
    public_memory: Uuid,
    original_expected: alia::domain::history::ExpectedState,
}

async fn internal_privacy_fixture(pool: &PgPool) -> InternalPrivacyFixture {
    let store = Store::new(pool.clone());
    store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let source = store
        .submit_message("private-source", "合成私有来源", None)
        .await
        .unwrap();
    let original_expected = store.snapshot().await.unwrap().unwrap().expected();
    let first = store
        .record_memories(
            "private-first",
            original_expected,
            &[source.event_id],
            &[memory(source.event_id, "合成私有认识", "general")],
        )
        .await
        .unwrap();
    let second = store
        .record_memories(
            "private-second",
            store.snapshot().await.unwrap().unwrap().expected(),
            &[first.event_id],
            &[memory(first.event_id, "对私有记录的再次组织", "general")],
        )
        .await
        .unwrap();
    let public_source = store
        .submit_message("public-source", "独立公开来源", None)
        .await
        .unwrap();
    let public_memory = store
        .record_memories(
            "public-record",
            store.snapshot().await.unwrap().unwrap().expected(),
            &[public_source.event_id],
            &[memory(public_source.event_id, "独立公开认识", "general")],
        )
        .await
        .unwrap()
        .memory_ids[0];
    assert_eq!(store.memories().await.unwrap().len(), 3);
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source.event_id)
        .execute(pool)
        .await
        .unwrap();
    InternalPrivacyFixture {
        store,
        source_event: source.event_id,
        first,
        second,
        public_memory,
        original_expected,
    }
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn internal_privacy_events_hide_blocked_ancestors_without_hiding_unrelated_history(
    pool: PgPool,
) {
    let f = internal_privacy_fixture(&pool).await;
    let events = f.store.events(0, 100).await.unwrap();
    for id in [f.source_event, f.first.event_id, f.second.event_id] {
        assert!(
            !events.iter().any(|e| e.id == id),
            "internal history exposed an event with a blocked ancestor: {id}"
        );
    }
    assert!(
        events
            .iter()
            .any(|e| e.payload.to_string().contains("独立公开来源"))
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn internal_privacy_memories_hide_all_generations_but_keep_unrelated_records(pool: PgPool) {
    let f = internal_privacy_fixture(&pool).await;
    let memories = f.store.memories().await.unwrap();
    assert_eq!(
        memories.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![f.public_memory],
        "internal memory history must filter the whole revoked-source closure"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn internal_privacy_new_memory_cannot_use_an_unblocked_child_of_a_revoked_source(
    pool: PgPool,
) {
    let f = internal_privacy_fixture(&pool).await;
    let expected = f.store.snapshot().await.unwrap().unwrap().expected();
    let before: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM memories),(SELECT count(*) FROM events),\
         (SELECT count(*) FROM outbox),(SELECT count(*) FROM idempotency_records)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let result = f
        .store
        .record_memories(
            "revoked-derived-write",
            expected,
            &[f.first.event_id],
            &[memory(
                f.first.event_id,
                "不得从撤销来源继续派生",
                "general",
            )],
        )
        .await;
    assert!(matches!(result, Err(StoreError::Invalid(_))), "{result:?}");
    let after: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM memories),(SELECT count(*) FROM events),\
         (SELECT count(*) FROM outbox),(SELECT count(*) FROM idempotency_records)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after, before,
        "rejected writes must have no partial effects"
    );
    assert_eq!(
        f.store.snapshot().await.unwrap().unwrap().state_version,
        expected.version
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn internal_privacy_idempotent_replay_does_not_expose_a_revoked_receipt(pool: PgPool) {
    let f = internal_privacy_fixture(&pool).await;
    let result = f
        .store
        .record_memories(
            "private-first",
            f.original_expected,
            &[f.source_event],
            &[memory(f.source_event, "合成私有认识", "general")],
        )
        .await;
    assert!(matches!(result, Err(StoreError::NotFound)), "{result:?}");
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn initialization_survives_reconnection_and_never_reloads_seed(
    options: PgPoolOptions,
    connection: PgConnectOptions,
) {
    let pool = options.connect_with(connection.clone()).await.unwrap();
    let store = Store::new(pool.clone());
    let mut original = seed().as_value().clone();
    original["identity"]["fictional_background"] =
        json!({"content":"设定中曾经生活在月球", "source_label":"novel"});
    let validated = PersonaSeed::validate(original.clone()).unwrap();
    let created = store
        .initialize("create", &validated, &config())
        .await
        .unwrap();
    let first = store.snapshot().await.unwrap().unwrap();
    assert_eq!(first.id, created.individual_id);
    assert_eq!(first.seed, original);
    assert_eq!(first.persona, original);
    assert_eq!(first.seed_digest.len(), 64);
    assert_eq!(
        store
            .initialize("create", &validated, &config())
            .await
            .unwrap(),
        created
    );
    let events = store.events(0, 100).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "individual.created");
    assert_eq!(events[0].source_kind, SourceKind::DirectObservation);
    assert!(!events[0].payload.to_string().contains("月球"));
    drop(store);
    pool.close().await;
    let pool = PgPoolOptions::new().connect_with(connection).await.unwrap();
    let store = Store::new(pool.clone());
    let restored = store.snapshot().await.unwrap().unwrap();
    assert_eq!(restored.id, first.id);
    assert_eq!(restored.created_at, first.created_at);
    assert_eq!(restored.seed_digest, first.seed_digest);
    let mut altered = seed().as_value().clone();
    altered["identity"]["display_name"] = json!("different template");
    let altered = PersonaSeed::validate(altered).unwrap();
    assert!(matches!(
        store.initialize("new-key", &altered, &config()).await,
        Err(StoreError::AlreadyInitialized)
    ));
    assert_eq!(store.snapshot().await.unwrap().unwrap().persona, original);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM relationships")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn concurrent_initialization_creates_exactly_one_individual(pool: PgPool) {
    let a = Store::new(pool.clone());
    let b = a.clone();
    let template = seed();
    let settings = config();
    let (one, two) = tokio::join!(
        a.initialize("one", &template, &settings),
        b.initialize("two", &template, &settings)
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let error = one.err().or(two.err()).unwrap();
    assert!(matches!(error, StoreError::AlreadyInitialized));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM individuals")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(a.events(0, 100).await.unwrap().len(), 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn retried_messages_share_event_run_and_outbox(pool: PgPool) {
    let store = Store::new(pool.clone());
    store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let other = store.clone();
    let (one, two) = tokio::join!(
        store.submit_message("message", "一般喜欢先研究", None),
        other.submit_message("message", "一般喜欢先研究", None)
    );
    let one = one.unwrap();
    assert_eq!(one, two.unwrap());
    assert!(matches!(
        store.submit_message("message", "换成另一句话", None).await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert_eq!(store.snapshot().await.unwrap().unwrap().state_version, 2);
    assert_eq!(store.events(0, 100).await.unwrap().len(), 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM runs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM relationship_events")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox WHERE event_type='message.created'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    let run: (Uuid, i64, String) =
        sqlx::query_as("SELECT trigger_event_id,state_version,status FROM runs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(run, (one.event_id, 2, "queued".into()));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn invalid_second_memory_rolls_back_entire_batch(pool: PgPool) {
    let store = Store::new(pool.clone());
    store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let message = store
        .submit_message("message", "先研究", None)
        .await
        .unwrap();
    let expected = store.snapshot().await.unwrap().unwrap().expected();
    let changes = [
        memory(message.event_id, "偏好先研究", "general"),
        memory(Uuid::new_v4(), "无来源", "general"),
    ];
    assert!(matches!(
        store
            .record_memories("bad", expected, &[message.event_id], &changes)
            .await,
        Err(StoreError::Invalid(_))
    ));
    assert!(store.memories().await.unwrap().is_empty());
    assert_eq!(
        store.snapshot().await.unwrap().unwrap().state_version,
        expected.version
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox WHERE event_type='memory.changed'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let committed = store
        .record_memories("good", expected, &[message.event_id], &changes[..1])
        .await
        .unwrap();
    assert_eq!(
        store
            .record_memories("good", expected, &[message.event_id], &changes[..1])
            .await
            .unwrap(),
        committed
    );
    assert_eq!(store.memories().await.unwrap().len(), 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn foreground_input_and_privacy_epoch_fence_old_memory_commits(pool: PgPool) {
    let store = Store::new(pool.clone());
    store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let old = store
        .submit_message("old", "过去的内容", None)
        .await
        .unwrap();
    let expected = store.snapshot().await.unwrap().unwrap().expected();
    let change = [memory(old.event_id, "背景认识", "general")];
    store
        .submit_message("new", "这次的新要求", None)
        .await
        .unwrap();
    assert!(matches!(
        store
            .record_memories("stale", expected, &[old.event_id], &change)
            .await,
        Err(StoreError::StaleState)
    ));
    let expected = store.snapshot().await.unwrap().unwrap().expected();
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store
            .record_memories("privacy", expected, &[old.event_id], &change)
            .await,
        Err(StoreError::StaleState)
    ));
    assert!(store.memories().await.unwrap().is_empty());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn scope_corrections_preserve_original_events_and_full_input_provenance(pool: PgPool) {
    let store = Store::new(pool.clone());
    store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let old = store
        .submit_message("old", "通常先研究", None)
        .await
        .unwrap();
    let receipt = store
        .record_memories(
            "remember",
            store.snapshot().await.unwrap().unwrap().expected(),
            &[old.event_id],
            &[memory(old.event_id, "一般偏好先研究", "general")],
        )
        .await
        .unwrap();
    let new = store
        .submit_message("new", "本次先原型", None)
        .await
        .unwrap();
    let unrelated = store
        .submit_message("extra", "另一个输入也参与上下文", None)
        .await
        .unwrap();
    let expected = store.snapshot().await.unwrap().unwrap().expected();
    let mut change = memory(new.event_id, "本次先原型", "task:prototype");
    change.supersedes = Some(receipt.memory_ids[0]);
    let manifest = [
        old.event_id,
        new.event_id,
        unrelated.event_id,
        receipt.memory_ids[0],
    ];
    assert!(matches!(
        store
            .record_memories("wrong-scope", expected, &manifest, &[change.clone()])
            .await,
        Err(StoreError::Invalid(_))
    ));
    change.supersedes = None;
    let local = store
        .record_memories("local", expected, &manifest, &[change])
        .await
        .unwrap();
    let memories = store.memories().await.unwrap();
    assert_eq!(memories.len(), 2);
    assert!(memories.iter().all(|m| m.validity == "active"));
    let ancestors: Vec<Uuid> =
        sqlx::query_scalar("SELECT source_id FROM derivation_edges WHERE target_id=$1")
            .bind(local.memory_ids[0])
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(ancestors.len(), 4);
    assert!(ancestors.contains(&unrelated.event_id));
    let original = store
        .events(0, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|e| e.id == old.event_id)
        .unwrap();
    assert_eq!(original.payload["text"], "通常先研究");
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn blocked_and_non_event_sources_are_rejected(pool: PgPool) {
    let store = Store::new(pool.clone());
    let birth = store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let current = store.snapshot().await.unwrap().unwrap();
    assert!(matches!(
        store
            .record_memories(
                "not-event",
                current.expected(),
                &[birth.individual_id],
                &[memory(birth.individual_id, "bad", "general")]
            )
            .await,
        Err(StoreError::Invalid(_))
    ));
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(birth.genesis_event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store
            .record_memories(
                "blocked",
                current.expected(),
                &[birth.genesis_event_id],
                &[memory(birth.genesis_event_id, "bad", "general")]
            )
            .await,
        Err(StoreError::Invalid(_))
    ));
    assert!(store.events(0, 100).await.unwrap().is_empty());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn valid_correction_preserves_the_superseded_memory_and_its_source(pool: PgPool) {
    let store = Store::new(pool);
    store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let old = store
        .submit_message("old", "我一般喜欢详细解释", None)
        .await
        .unwrap();
    let previous = store
        .record_memories(
            "remember",
            store.snapshot().await.unwrap().unwrap().expected(),
            &[old.event_id],
            &[memory(old.event_id, "喜欢详细解释", "general")],
        )
        .await
        .unwrap();
    let new = store
        .submit_message("new", "今后请一般先给简短结论", None)
        .await
        .unwrap();
    let mut correction = memory(new.event_id, "一般先给简短结论", "general");
    correction.supersedes = Some(previous.memory_ids[0]);
    store
        .record_memories(
            "correct",
            store.snapshot().await.unwrap().unwrap().expected(),
            &[new.event_id, previous.memory_ids[0]],
            &[correction],
        )
        .await
        .unwrap();
    let memories = store.memories().await.unwrap();
    assert_eq!(memories.len(), 2);
    let old_memory = memories
        .iter()
        .find(|m| m.id == previous.memory_ids[0])
        .unwrap();
    assert_eq!(old_memory.validity, "superseded");
    assert_eq!(old_memory.content, "喜欢详细解释");
    assert_eq!(old_memory.source_event_ids, vec![old.event_id]);
    assert!(
        memories
            .iter()
            .any(|m| m.validity == "active" && m.supersedes == Some(old_memory.id))
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn foreground_and_background_share_identity_and_only_one_state_version_commits(pool: PgPool) {
    let foreground =
        alia::runtime::Runtime::new(pool, "integration-test-token-with-sufficient-length").unwrap();
    let background = foreground.clone();
    let born = foreground
        .store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let received = foreground
        .store
        .submit_message("hello", "共同研究一个问题", None)
        .await
        .unwrap();
    let bg_state = background.store.snapshot().await.unwrap().unwrap();
    assert_eq!(bg_state.id, born.individual_id);
    assert_eq!(bg_state.state_version, received.version);
    let inputs = [received.event_id];
    let a = [memory(received.event_id, "第一次理解", "general")];
    let b = [memory(received.event_id, "另一次理解", "general")];
    let (one, two) = tokio::join!(
        foreground
            .store
            .record_memories("a", bg_state.expected(), &inputs, &a),
        background
            .store
            .record_memories("b", bg_state.expected(), &inputs, &b)
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    assert!(matches!(
        one.err().or(two.err()).unwrap(),
        StoreError::StaleState
    ));
    assert_eq!(foreground.store.memories().await.unwrap().len(), 1);
    assert_eq!(
        background
            .store
            .snapshot()
            .await
            .unwrap()
            .unwrap()
            .state_version,
        bg_state.state_version + 1
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn restored_rules_require_real_registered_capabilities_without_resetting_persona(
    pool: PgPool,
) {
    let runtime =
        alia::runtime::Runtime::new(pool, "integration-test-token-with-sufficient-length").unwrap();
    runtime
        .store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let before = runtime.store.snapshot().await.unwrap().unwrap();
    assert!(matches!(
        runtime.verify_saved_configuration(&[]).await,
        Err(alia::runtime::RestoreError::Configuration(
            alia::domain::config::ConfigurationError::UnimplementedCapability
        ))
    ));
    runtime
        .verify_saved_configuration(&[
            "memory.search",
            "documents.read",
            "notes.write",
            "web.read_allowed",
        ])
        .await
        .unwrap();
    let after = runtime.store.snapshot().await.unwrap().unwrap();
    assert_eq!(before.id, after.id);
    assert_eq!(before.persona, after.persona);
    assert_eq!(before.state_version, after.state_version);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn retrieval_filters_scope_time_subject_status_and_blocked_provenance(pool: PgPool) {
    use alia::domain::cognition::MemoryQuery;
    use chrono::{Duration, Utc};
    let store = Store::new(pool.clone());
    store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let msg = store
        .submit_message("pref", "一般先研究，当前先原型", None)
        .await
        .unwrap();
    let created = store
        .record_memories(
            "batch",
            store.snapshot().await.unwrap().unwrap().expected(),
            &[msg.event_id],
            &[
                memory(msg.event_id, "通常先研究", "general"),
                memory(msg.event_id, "本次先原型", "task:current"),
                memory(msg.event_id, "其他任务原型", "task:other"),
                memory(msg.event_id, "过时原型", "general"),
                memory(msg.event_id, "争议原型", "general"),
                memory(msg.event_id, "未来原型", "general"),
            ],
        )
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET valid_until=now()-interval '1 minute' WHERE id=$1")
        .bind(created.memory_ids[3])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET validity='disputed' WHERE id=$1")
        .bind(created.memory_ids[4])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET valid_from=now()+interval '1 day' WHERE id=$1")
        .bind(created.memory_ids[5])
        .execute(&pool)
        .await
        .unwrap();
    let owner: Uuid =
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut q = MemoryQuery {
        text: "原型".into(),
        scope: Some("task:current".into()),
        subject_id: Some(owner),
        at: Utc::now(),
        limit: 20,
    };
    let found = store.search_memories(&q).await.unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].id, created.memory_ids[1]);
    q.subject_id = Some(Uuid::new_v4());
    assert!(store.search_memories(&q).await.unwrap().is_empty());
    q.subject_id = None;
    q.scope = None;
    assert_eq!(store.search_memories(&q).await.unwrap().len(), 1);
    q.at = Utc::now() + Duration::days(2);
    assert_eq!(store.search_memories(&q).await.unwrap().len(), 2);
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(msg.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.search_memories(&q).await.unwrap().is_empty());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn restored_context_retrieves_old_evidence_and_fixture_is_explicit(pool: PgPool) {
    use alia::runtime::model::*;
    let store = Store::new(pool.clone());
    store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let old = store
        .submit_message("old", "通常先研究", None)
        .await
        .unwrap();
    let remembered = store
        .record_memories(
            "memory",
            store.snapshot().await.unwrap().unwrap().expected(),
            &[old.event_id],
            &[memory(old.event_id, "通常先研究再行动", "general")],
        )
        .await
        .unwrap();
    for i in 0..12 {
        store
            .submit_message(&format!("filler-{i}"), "无关输入", None)
            .await
            .unwrap();
    }
    let now = store
        .submit_message("current", "这次先做原型", None)
        .await
        .unwrap();
    let restored = Store::new(pool.clone());
    let context = restored
        .model_context(now.operation_id, vec![])
        .await
        .unwrap();
    assert_eq!(context.individual_id, old.individual_id);
    assert!(context.events.iter().any(|e| e.id == old.event_id));
    assert!(
        context
            .memories
            .iter()
            .any(|m| m.id == remembered.memory_ids[0])
    );
    for id in [old.event_id, now.event_id, remembered.memory_ids[0]] {
        assert!(context.input_manifest.contains(&id));
    }
    assert!(serde_json::to_vec(&context).unwrap().len() <= 96 * 1024);
    let fixture=FixtureProvider{response:json!({"schema_version":1,"reply":"这次先原型","reply_evidence_ids":[old.event_id,now.event_id],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"局部例外"}).to_string()};
    let result = evaluate_proposal(
        &fixture,
        &context,
        CallLimits::default(),
        ModelCallMetadata::foreground(&context).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(result.provider_mode, ProviderMode::Fixture);
    assert!(result.usage.is_none());
    assert!(matches!(
        store.model_context(old.operation_id, vec![]).await,
        Err(StoreError::StaleState)
    ));
    store.submit_message("newer", "新消息", None).await.unwrap();
    assert!(matches!(
        store.model_context(now.operation_id, vec![]).await,
        Err(StoreError::StaleState)
    ));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn context_drops_blocked_evidence_and_rejects_blocked_trigger(pool: PgPool) {
    let store = Store::new(pool.clone());
    store
        .initialize("create", &seed(), &config())
        .await
        .unwrap();
    let old = store
        .submit_message("old", "不再允许使用的信息", None)
        .await
        .unwrap();
    store
        .record_memories(
            "mem",
            store.snapshot().await.unwrap().unwrap().expected(),
            &[old.event_id],
            &[memory(old.event_id, "不再允许使用的信息", "general")],
        )
        .await
        .unwrap();
    let current = store
        .submit_message("current", "当前消息", None)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(old.event_id)
        .execute(&pool)
        .await
        .unwrap();
    let context = store
        .model_context(current.operation_id, vec![])
        .await
        .unwrap();
    assert!(context.memories.is_empty());
    assert!(!context.input_manifest.contains(&old.event_id));
    assert!(!context.events.iter().any(|e| e.id == old.event_id));
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(current.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.model_context(current.operation_id, vec![]).await,
        Err(StoreError::Invalid(_))
    ));
}
