//! Queue/recovery fault injection with synthetic vectors, never a quality metric.
use alia::{
    adapters::{
        IndexFailure,
        store::{Store, StoreError},
    },
    domain::{
        config::{PersonaSeed, RuntimeRules},
        history::{MemoryKind, NewMemory},
        retrieval::{DIMENSION, IndexedChunk},
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        indexing::{IndexEncodingError, IndexOutcome, IndexWorker, MemoryEncoder},
    },
};
use sqlx::PgPool;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
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
    let id = s
        .record_memories(
            &Uuid::new_v4().to_string(),
            s.snapshot().await.unwrap().unwrap().expected(),
            &[source],
            &[NewMemory {
                content: text.into(),
                scope: "general".into(),
                kind: MemoryKind::Semantic,
                source_event_ids: vec![source],
                supersedes: None,
            }],
        )
        .await
        .unwrap()
        .memory_ids[0];
    (id, source)
}
fn chunks(text: &str) -> Vec<IndexedChunk> {
    let mut vector = vec![0.0; DIMENSION];
    vector[0] = 1.0;
    vec![IndexedChunk {
        start_char: 0,
        end_char: text.chars().count(),
        vector,
    }]
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn concurrent_index_claim_restart_and_missing_chunk_repair(pool: PgPool) {
    let s = setup(&pool).await;
    let (id, _) = memory(&s, "有来源的记忆").await;
    let version = s.snapshot().await.unwrap().unwrap().state_version;
    let (a, b) = tokio::join!(
        s.claim_memory_index(Uuid::new_v4()),
        s.claim_memory_index(Uuid::new_v4())
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.is_some(), b.is_some());
    let lease = a.or(b).unwrap();
    assert_eq!(lease.memory_id(), id);
    s.finish_memory_index(&lease, &chunks(lease.content()))
        .await
        .unwrap();
    assert!(matches!(
        s.finish_memory_index(&lease, &chunks(lease.content()))
            .await,
        Err(StoreError::LeaseLost)
    ));
    let restored = Store::new(pool.clone());
    assert!(
        restored
            .claim_memory_index(Uuid::new_v4())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().state_version,
        version,
        "derived indexing must not change personality/runtime state"
    );
    assert_eq!(count(&pool, "memory_vector_sets").await, 1);
    sqlx::query("DELETE FROM memory_vectors")
        .execute(&pool)
        .await
        .unwrap();
    let retry = restored
        .claim_memory_index(Uuid::new_v4())
        .await
        .unwrap()
        .unwrap();
    restored
        .finish_memory_index(&retry, &chunks(retry.content()))
        .await
        .unwrap();
    assert_eq!(count(&pool, "memory_vectors").await, 1);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn expired_leases_are_fenced_and_attempts_do_not_reset_on_restart(pool: PgPool) {
    let s = setup(&pool).await;
    memory(&s, "待计算记忆").await;
    let first = s.claim_memory_index(Uuid::new_v4()).await.unwrap().unwrap();
    for _ in 0..2 {
        sqlx::query(
            "UPDATE memory_index_jobs SET lease_expires_at=clock_timestamp()-interval '1 second'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let next = Store::new(pool.clone())
            .claim_memory_index(Uuid::new_v4())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            s.finish_memory_index(&first, &chunks(first.content()))
                .await,
            Err(StoreError::LeaseLost)
        ));
        assert_eq!(next.content(), first.content());
    }
    sqlx::query(
        "UPDATE memory_index_jobs SET lease_expires_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        s.claim_memory_index(Uuid::new_v4())
            .await
            .unwrap()
            .is_none()
    );
    let row: (String, i32) = sqlx::query_as("SELECT status,attempts FROM memory_index_jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row, ("failed".into(), 3));
    assert_eq!(count(&pool, "memory_vectors").await, 0);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn content_epoch_and_source_changes_never_commit_obsolete_vectors(pool: PgPool) {
    let s = setup(&pool).await;
    let (id, source) = memory(&s, "原始记忆").await;
    let first = s.claim_memory_index(Uuid::new_v4()).await.unwrap().unwrap();
    sqlx::query("UPDATE memories SET content='故障改文' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        s.finish_memory_index(&first, &chunks(first.content()))
            .await
            .is_err()
    );
    let second = s.claim_memory_index(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(second.content(), "故障改文");
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        s.finish_memory_index(&second, &chunks(second.content()))
            .await
            .is_err()
    );
    let third = s.claim_memory_index(Uuid::new_v4()).await.unwrap().unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        s.finish_memory_index(&third, &chunks(third.content()))
            .await
            .is_err()
    );
    assert!(
        s.claim_memory_index(Uuid::new_v4())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(count(&pool, "memory_vector_sets").await, 0);
    let status: String = sqlx::query_scalar("SELECT status FROM memory_index_jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "cancelled");
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn pause_and_foreground_work_prevent_index_claims_and_submission(pool: PgPool) {
    let s = setup(&pool).await;
    memory(&s, "等待索引").await;
    let lease = s.claim_memory_index(Uuid::new_v4()).await.unwrap().unwrap();
    sqlx::query("UPDATE individuals SET autonomy_paused=true")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        s.finish_memory_index(&lease, &chunks(lease.content()))
            .await
            .is_err()
    );
    assert!(
        s.claim_memory_index(Uuid::new_v4())
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("UPDATE individuals SET autonomy_paused=false")
        .execute(&pool)
        .await
        .unwrap();
    s.finish_memory_index(&lease, &chunks(lease.content()))
        .await
        .unwrap();
    memory(&s, "另一记忆").await;
    s.submit_message("priority", "现在先交流", None)
        .await
        .unwrap();
    assert!(
        s.claim_memory_index(Uuid::new_v4())
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("UPDATE runs SET status='cancelled' WHERE status='queued'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        s.claim_memory_index(Uuid::new_v4())
            .await
            .unwrap()
            .is_some()
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn index_and_ready_status_commit_together_and_failures_back_off(pool: PgPool) {
    let s = setup(&pool).await;
    memory(&s, "可恢复记忆").await;
    let lease = s.claim_memory_index(Uuid::new_v4()).await.unwrap().unwrap();
    sqlx::query("CREATE FUNCTION reject_index_ready() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.status='ready' THEN RAISE EXCEPTION 'synthetic fault'; END IF; RETURN NEW; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_index_ready BEFORE UPDATE ON memory_index_jobs FOR EACH ROW EXECUTE FUNCTION reject_index_ready()").execute(&pool).await.unwrap();
    assert!(
        s.finish_memory_index(&lease, &chunks(lease.content()))
            .await
            .is_err()
    );
    assert_eq!(count(&pool, "memory_vectors").await, 0);
    sqlx::query("DROP TRIGGER reject_index_ready ON memory_index_jobs")
        .execute(&pool)
        .await
        .unwrap();
    s.fail_memory_index(&lease, IndexFailure::Encoding)
        .await
        .unwrap();
    assert!(
        s.claim_memory_index(Uuid::new_v4())
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("UPDATE memory_index_jobs SET available_at=clock_timestamp()-interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    let retry = s.claim_memory_index(Uuid::new_v4()).await.unwrap().unwrap();
    s.finish_memory_index(&retry, &chunks(retry.content()))
        .await
        .unwrap();
    assert_eq!(count(&pool, "memory_vectors").await, 1);
}
struct SyntheticEncoder(AtomicUsize);
#[async_trait::async_trait]
impl MemoryEncoder for SyntheticEncoder {
    async fn encode_memory(&self, text: String) -> Result<Vec<IndexedChunk>, IndexEncodingError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(chunks(&text))
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn worker_is_idle_without_work_and_indexes_without_cloud_calls(pool: PgPool) {
    let s = setup(&pool).await;
    let model = Arc::new(SyntheticEncoder(AtomicUsize::new(0)));
    let worker = IndexWorker::new(s.clone(), model.clone());
    assert_eq!(worker.run_once().await.unwrap(), IndexOutcome::Idle);
    let (id, _) = memory(&s, "本地维护的经历").await;
    assert_eq!(worker.run_once().await.unwrap(), IndexOutcome::Indexed(id));
    assert_eq!(worker.run_once().await.unwrap(), IndexOutcome::Idle);
    assert_eq!(model.0.load(Ordering::SeqCst), 1);
    assert_eq!(count(&pool, "usage_reservations").await, 0);
    let (tx, rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(worker.serve(rx));
    tx.send(true).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
}

struct GatedEncoder {
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MemoryEncoder for GatedEncoder {
    async fn encode_memory(&self, text: String) -> Result<Vec<IndexedChunk>, IndexEncodingError> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(chunks(&text))
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn encoding_holds_no_database_transaction_and_rechecks_revoked_source(pool: PgPool) {
    let s = setup(&pool).await;
    let (id, source) = memory(&s, "计算中的记忆").await;
    let encoder = Arc::new(GatedEncoder {
        started: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let worker = IndexWorker::new(s.clone(), encoder.clone());
    let task = tokio::spawn(async move { worker.run_once().await });
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        encoder.started.notified(),
    )
    .await
    .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        s.submit_message("during-indexing", "前台输入不等待编码", None),
    )
    .await
    .unwrap()
    .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    encoder.release.notify_one();
    assert_eq!(task.await.unwrap().unwrap(), IndexOutcome::Fenced(id));
    assert_eq!(count(&pool, "memory_vectors").await, 0);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn shutdown_drops_pending_encoding_without_late_database_submission(pool: PgPool) {
    let s = setup(&pool).await;
    memory(&s, "关闭时待处理的记忆").await;
    let encoder = Arc::new(GatedEncoder {
        started: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let worker = IndexWorker::new(s, encoder.clone());
    let (tx, rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(worker.serve(rx));
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        encoder.started.notified(),
    )
    .await
    .unwrap();
    tx.send(true).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    encoder.release.notify_one();
    assert_eq!(count(&pool, "memory_vectors").await, 0);
    let row: (String, i32) = sqlx::query_as("SELECT status,attempts FROM memory_index_jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        row,
        ("running".into(), 1),
        "restart recovers the durable lease, not an invented success"
    );
}
