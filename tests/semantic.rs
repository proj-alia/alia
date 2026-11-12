//! Real PostgreSQL vector invariants with explicitly synthetic unit vectors.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        config::{PersonaSeed, RuntimeRules},
        history::{MemoryKind, NewMemory},
        retrieval::*,
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
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
    s
}
async fn memory(s: &Store, text: &str) -> (Uuid, Uuid) {
    let source = s
        .submit_message(&Uuid::new_v4().to_string(), text, None)
        .await
        .unwrap()
        .event_id;
    let state = s.snapshot().await.unwrap().unwrap();
    let r = s
        .record_memories(
            &Uuid::new_v4().to_string(),
            state.expected(),
            &[source],
            &[NewMemory {
                content: text.into(),
                kind: MemoryKind::Semantic,
                scope: "general".into(),
                source_event_ids: vec![source],
                supersedes: None,
            }],
        )
        .await
        .unwrap();
    (r.memory_ids[0], source)
}
fn vector(axis: usize) -> Vec<f32> {
    let mut v = vec![0.0; DIMENSION];
    v[axis] = 1.0;
    v
}
fn query() -> SemanticQuery {
    SemanticQuery {
        vector: vector(0),
        profile: INDEX_PROFILE.into(),
        scope: None,
        subject_id: None,
        person_id: None,
        at: Utc::now(),
        include_history: false,
        limit: 10,
    }
}
async fn index(s: &Store, id: Uuid, axis: usize) {
    let snapshot = s.prepare_memory_index(id).await.unwrap();
    s.commit_memory_index(
        &snapshot,
        &[IndexedChunk {
            start_char: 0,
            end_char: snapshot.content().chars().count(),
            vector: vector(axis),
        }],
    )
    .await
    .unwrap();
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn exact_ranking_restart_profile_and_source_fencing(pool: PgPool) {
    let s = setup(&pool).await;
    let (a, source) = memory(&s, "相关记忆").await;
    let (b, _) = memory(&s, "另一记忆").await;
    index(&s, a, 0).await;
    index(&s, b, 1).await;
    let restored = Store::new(pool.clone());
    let hits = restored.search_semantic(&query()).await.unwrap();
    assert_eq!(hits.iter().map(|m| m.id).collect::<Vec<_>>(), vec![a, b]);
    let mut wrong = query();
    wrong.profile = "other-space".into();
    assert!(restored.search_semantic(&wrong).await.is_err());
    wrong = query();
    wrong.vector[0] = f32::NAN;
    assert!(restored.search_semantic(&wrong).await.is_err());
    let pending = s.prepare_memory_index(a).await.unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.commit_memory_index(
            &pending,
            &[IndexedChunk {
                start_char: 0,
                end_char: 4,
                vector: vector(0)
            }]
        )
        .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(
        s.search_semantic(&query())
            .await
            .unwrap()
            .iter()
            .map(|m| m.id)
            .collect::<Vec<_>>(),
        vec![b]
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn complete_atomic_index_and_stale_content_are_enforced(pool: PgPool) {
    let s = setup(&pool).await;
    let (id, _) = memory(&s, "甲乙丙丁").await;
    index(&s, id, 0).await;
    let snapshot = s.prepare_memory_index(id).await.unwrap();
    assert!(
        s.commit_memory_index(
            &snapshot,
            &[IndexedChunk {
                start_char: 0,
                end_char: 2,
                vector: vector(1)
            }]
        )
        .await
        .is_err()
    );
    sqlx::query("CREATE FUNCTION reject_vector() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_vector BEFORE INSERT ON memory_vectors FOR EACH ROW EXECUTE FUNCTION reject_vector()").execute(&pool).await.unwrap();
    assert!(
        s.commit_memory_index(
            &snapshot,
            &[IndexedChunk {
                start_char: 0,
                end_char: 4,
                vector: vector(1)
            }]
        )
        .await
        .is_err()
    );
    assert_eq!(s.search_semantic(&query()).await.unwrap()[0].id, id);
    sqlx::query("DROP TRIGGER reject_vector ON memory_vectors")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET content='内容变了' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.search_semantic(&query()).await.unwrap().is_empty());
    assert!(matches!(
        s.commit_memory_index(
            &snapshot,
            &[IndexedChunk {
                start_char: 0,
                end_char: 4,
                vector: vector(0)
            }]
        )
        .await,
        Err(StoreError::StaleState)
    ));
    index(&s, id, 0).await;
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.search_semantic(&query()).await.unwrap().is_empty());
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn history_scope_relationship_and_incomplete_sets_filter_before_limit(pool: PgPool) {
    let s = setup(&pool).await;
    let (old, _) = memory(&s, "过去认识").await;
    let (local, source) = memory(&s, "本次例外").await;
    sqlx::query("UPDATE memories SET validity='superseded',valid_from='2026-01-01',valid_until='2026-06-01' WHERE id=$1").bind(old).execute(&pool).await.unwrap();
    sqlx::query("UPDATE memories SET scope='event:local',valid_from='2026-06-01' WHERE id=$1")
        .bind(local)
        .execute(&pool)
        .await
        .unwrap();
    index(&s, old, 0).await;
    index(&s, local, 0).await;
    assert!(s.search_semantic(&query()).await.unwrap().is_empty());
    let mut q = query();
    q.at = "2026-03-01T00:00:00Z".parse().unwrap();
    q.include_history = true;
    assert_eq!(s.search_semantic(&q).await.unwrap()[0].id, old);
    q = query();
    q.scope = Some("event:local".into());
    assert_eq!(s.search_semantic(&q).await.unwrap()[0].id, local);
    q.subject_id = Some(Uuid::new_v4());
    assert!(s.search_semantic(&q).await.unwrap().is_empty());
    q.subject_id = Some(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT subject_id FROM relationship_events WHERE event_id=$1",
        )
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap(),
    );
    assert_eq!(s.search_semantic(&q).await.unwrap()[0].id, local);
    sqlx::query("DELETE FROM memory_vectors WHERE memory_id=$1")
        .bind(local)
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.search_semantic(&q).await.unwrap().is_empty());
}

fn span(event: Uuid, text: &str, name: &str) -> alia::domain::entities::SourceSpan {
    let byte = text.find(name).unwrap();
    let start = text[..byte].chars().count();
    alia::domain::entities::SourceSpan {
        event_id: event,
        start_char: start,
        end_char: start + name.chars().count(),
    }
}
async fn person(s: &Store, key: &str, name: &str, event: Uuid, text: &str) -> Uuid {
    s.register_person(
        key,
        s.snapshot().await.unwrap().unwrap().expected(),
        key,
        name,
        span(event, text, name),
    )
    .await
    .unwrap()
    .entity_id
}
async fn link(
    s: &Store,
    memory: Uuid,
    source: Uuid,
    text: &str,
    name: &str,
    entity: Uuid,
    role: alia::domain::entities::PersonRole,
) {
    let start = span(source, text, name).start_char;
    s.assert_memory_person(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        alia::domain::entities::MemoryPersonAssertion {
            memory_id: memory,
            memory_version: 1,
            entity_id: entity,
            role,
            source: span(source, text, name),
            memory_start_char: start,
        },
    )
    .await
    .unwrap();
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn person_filter_distinguishes_subject_from_mentions_and_blocks_legacy_paths(pool: PgPool) {
    use alia::domain::{cognition::MemoryQuery, entities::PersonRole};
    let s = setup(&pool).await;
    let a = "🙂赵芸喜欢红茶，赵云只在附注中提到。";
    let b = "赵云习惯绿茶，赵芸仅被提到。";
    let (ma, sa) = memory(&s, a).await;
    let (mb, sb) = memory(&s, b).await;
    let ea = person(&s, "a", "赵芸", sa, a).await;
    let eb = person(&s, "b", "赵云", sb, b).await;
    link(&s, ma, sa, a, "赵芸", ea, PersonRole::Subject).await;
    link(&s, mb, sb, b, "赵云", eb, PersonRole::Subject).await;
    link(&s, mb, sb, b, "赵芸", ea, PersonRole::Mentioned).await;
    index(&s, ma, 0).await;
    index(&s, mb, 0).await;
    let mut q = query();
    q.person_id = Some(ea);
    assert_eq!(
        s.search_semantic(&q)
            .await
            .unwrap()
            .iter()
            .map(|m| m.id)
            .collect::<Vec<_>>(),
        vec![ma]
    );
    q.person_id = Some(eb);
    assert_eq!(s.search_semantic(&q).await.unwrap()[0].id, mb);
    // Correct the current role without erasing the earlier assertion event.
    link(&s, mb, sb, b, "赵芸", ea, PersonRole::Subject).await;
    let mut corrected = query();
    corrected.person_id = Some(ea);
    assert_eq!(s.search_semantic(&corrected).await.unwrap().len(), 2);
    link(&s, mb, sb, b, "赵芸", ea, PersonRole::Mentioned).await;
    assert_eq!(s.search_semantic(&corrected).await.unwrap().len(), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM events WHERE kind='memory.person_asserted'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        5
    );

    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(ea)
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.search_semantic(&query()).await.unwrap().is_empty());
    assert!(
        s.search_memories(&MemoryQuery {
            text: "茶".into(),
            scope: None,
            subject_id: None,
            at: Utc::now(),
            limit: 10
        })
        .await
        .unwrap()
        .is_empty()
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn person_homonyms_aliases_and_replay_never_guess_or_resurrect(pool: PgPool) {
    use alia::domain::entities::PersonResolution;
    let s = setup(&pool).await;
    let text = "赵芸也叫小芸。";
    let (_, source) = memory(&s, text).await;
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let proof = span(source, text, "赵芸");
    let a = s
        .register_person("register", expected, "person-a", "赵芸", proof.clone())
        .await
        .unwrap();
    assert_eq!(
        s.register_person("register", expected, "person-a", "赵芸", proof)
            .await
            .unwrap()
            .entity_id,
        a.entity_id
    );
    s.add_person_alias(
        "alias",
        s.snapshot().await.unwrap().unwrap().expected(),
        a.entity_id,
        span(source, text, "小芸"),
    )
    .await
    .unwrap();
    assert_eq!(
        s.resolve_person("小芸").await.unwrap(),
        PersonResolution::Unique(a.entity_id)
    );
    let another = person(&s, "person-b", "赵芸", source, text).await;
    assert_ne!(another, a.entity_id);
    assert_eq!(
        s.resolve_person("赵芸").await.unwrap(),
        PersonResolution::Ambiguous
    );
    assert_eq!(
        s.resolve_person("赵雲").await.unwrap(),
        PersonResolution::Unknown
    );
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        s.resolve_person("赵芸").await.unwrap(),
        PersonResolution::Unknown
    );
    assert!(
        s.register_person(
            "register",
            expected,
            "person-a",
            "赵芸",
            span(source, text, "赵芸")
        )
        .await
        .is_err()
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn person_assertions_reject_unrelated_sources_stale_versions_and_changed_content(
    pool: PgPool,
) {
    use alia::domain::entities::{MemoryPersonAssertion, PersonRole};
    let s = setup(&pool).await;
    let text = "赵芸选择红茶。";
    let (id, source) = memory(&s, text).await;
    let (_, unrelated) = memory(&s, text).await;
    let e = person(&s, "person", "赵芸", source, text).await;
    let assertion = |event, version| MemoryPersonAssertion {
        memory_id: id,
        memory_version: version,
        entity_id: e,
        role: PersonRole::Subject,
        source: span(event, text, "赵芸"),
        memory_start_char: 0,
    };
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    assert!(
        s.assert_memory_person("unrelated", expected, assertion(unrelated, 1))
            .await
            .is_err()
    );
    assert!(
        s.assert_memory_person("stale", expected, assertion(source, 2))
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM memory_person_links")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    s.assert_memory_person("good", expected, assertion(source, 1))
        .await
        .unwrap();
    index(&s, id, 0).await;
    let mut q = query();
    q.person_id = Some(e);
    assert_eq!(s.search_semantic(&q).await.unwrap()[0].id, id);
    sqlx::query("UPDATE memories SET content='赵云选择绿茶。' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    index(&s, id, 0).await;
    assert!(s.search_semantic(&q).await.unwrap().is_empty());
}
