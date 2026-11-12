//! Independent fixed-root-privacy-http-v2 boundary regressions; no model calls.
//! Run only against the fixed source plus this test in an owned disposable PG.
use alia::{
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::{Runtime, capability::IMPLEMENTED_CAPABILITIES},
    service,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt, path::PathBuf, time::Duration};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "p01-isolated-privacy-boundary-test-token";

struct Journal {
    directory: PathBuf,
    path: PathBuf,
}
impl Journal {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("alia-p01-privacy-{}", Uuid::new_v4()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        Self {
            path: directory.join("deletion.jsonl"),
            directory,
        }
    }
}
impl Drop for Journal {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

async fn setup(pool: &PgPool, journal: &Journal) -> Runtime {
    let mut rt = Runtime::new(pool.clone(), TOKEN).unwrap();
    rt.configure_deletion_journal(Some(&journal.path))
        .await
        .unwrap();
    rt.store
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
    rt
}

async fn request(rt: &Runtime, method: &str, path: &str, headers: &[(&str, &str)]) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {TOKEN}"));
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    service::router(rt.clone(), PathBuf::from("web/dist"))
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn delete(rt: &Runtime, id: Uuid, key: &str) -> (StatusCode, Value) {
    let response = request(
        rt,
        "DELETE",
        &format!("/api/v1/memories/{id}"),
        &[("idempotency-key", key), ("if-match", "\"1\"")],
    )
    .await;
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

// Controlled, ordinary stored source and memory plus its normal metadata hint.
// No production source, privacy flags, wall clock or runtime results are patched.
async fn memory_with_notification(pool: &PgPool) -> (Uuid, Uuid) {
    let event = Uuid::new_v4();
    let memory = Uuid::new_v4();
    for (id, kind) in [(event, "event"), (memory, "memory")] {
        sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,$2 FROM individuals")
            .bind(id)
            .bind(kind)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,$1,'message.received','reported_claim','owner',clock_timestamp(),$2,$1 FROM individuals")
        .bind(event).bind(json!({"text":"SYNTHETIC-P01-PRIVATE-BODY"})).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memories(individual_id,id,kind,content,scope) SELECT id,$1,'semantic','SYNTHETIC-P01-PRIVATE-BODY','test' FROM individuals")
        .bind(memory).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,$1,$2 FROM individuals")
        .bind(memory).bind(event).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO outbox(individual_id,event_type,resource_id,version) SELECT id,'memory.changed',$1,1 FROM individuals")
        .bind(memory).execute(pool).await.unwrap();
    (memory, event)
}

async fn head(rt: &Runtime) -> String {
    rt.store.public_state().await.unwrap()["event_cursor"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn subscribe(rt: &Runtime, cursor: &str) -> Body {
    let response = request(rt, "GET", "/api/v1/events", &[("last-event-id", cursor)]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let mut body = response.into_body();
    let first = frame(&mut body).await.expect("missing initial greeting");
    assert!(first.contains("connected"));
    body
}

async fn frame(body: &mut Body) -> Option<String> {
    tokio::time::timeout(Duration::from_secs(3), body.frame())
        .await
        .expect("bounded SSE wait")
        .map(|f| String::from_utf8(f.unwrap().into_data().unwrap().to_vec()).unwrap())
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires explicitly owned disposable PostgreSQL and frozen v2 source"]
async fn already_queued_sse_hint_must_not_disclose_a_source_blocked_after_subscribe(pool: PgPool) {
    let journal = Journal::new();
    let rt = setup(&pool, &journal).await;
    let cursor = head(&rt).await;
    let (target, _) = memory_with_notification(&pool).await;
    // The existing stream has already snapshotted this hint but has not sent it.
    let mut body = subscribe(&rt, &cursor).await;
    let (status, receipt) = delete(&rt, target, "queued-hint").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(receipt["status"], "blocked");
    assert_eq!(
        request(&rt, "GET", &format!("/api/v1/memories/{target}"), &[])
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let fresh = rt
        .store
        .notifications(Some(cursor.parse().unwrap()))
        .await
        .unwrap();
    assert!(
        !fresh
            .items
            .iter()
            .filter_map(|n| n.resource.as_ref())
            .any(|(_, id, _)| *id == target)
    );
    let late = frame(&mut body).await;
    assert!(
        late.as_ref()
            .is_none_or(|f| !f.contains(&target.to_string())),
        "pre-delete SSE queue disclosed a blocked memory UUID after DELETE returned 202"
    );
    assert!(
        late.as_ref()
            .is_none_or(|f| !f.contains("SYNTHETIC-P01-PRIVATE-BODY"))
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires explicitly owned disposable PostgreSQL and frozen v2 source"]
async fn already_open_idle_sse_must_close_when_durable_delete_outcome_is_unknown(pool: PgPool) {
    let journal = Journal::new();
    let rt = setup(&pool, &journal).await;
    let cursor = head(&rt).await;
    let mut body = subscribe(&rt, &cursor).await; // Starts with an empty queue.
    let (target, _) = memory_with_notification(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION p01_fail_accept() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic p01 acceptance fault'; END $$; CREATE TRIGGER p01_fail_accept BEFORE INSERT ON privacy_operations FOR EACH ROW EXECUTE FUNCTION p01_fail_accept();")
        .execute(&pool).await.unwrap();
    let (status, receipt) = delete(&rt, target, "unknown-idle-hint").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(receipt["error"]["code"], "privacy_unavailable");
    assert!(!rt.store.privacy_ready());
    assert_eq!(rt.deletion_journal().unwrap().records().unwrap().len(), 1);
    assert_eq!(
        request(&rt, "GET", &format!("/api/v1/memories/{target}"), &[])
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let late = frame(&mut body).await;
    // End the old subscription: its HTTP status can no longer be changed to 503.
    assert!(
        late.is_none(),
        "idle SSE polled and emitted another frame while all new private HTTP access was fenced"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires explicitly owned disposable PostgreSQL and frozen v2 source"]
async fn cancelling_a_delete_after_durable_intent_keeps_clones_closed_until_replay(pool: PgPool) {
    let journal = Journal::new();
    let rt = setup(&pool, &journal).await;
    let (target, _) = memory_with_notification(&pool).await;
    let key = i64::from_be_bytes(Uuid::new_v4().as_bytes()[..8].try_into().unwrap());
    let mut barrier = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(key)
        .execute(&mut *barrier)
        .await
        .unwrap();
    sqlx::raw_sql(&format!("CREATE FUNCTION p01_wait_accept() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock({key}::bigint); RETURN NEW; END $$; CREATE TRIGGER p01_wait_accept BEFORE INSERT ON privacy_operations FOR EACH ROW EXECUTE FUNCTION p01_wait_accept();"))
        .execute(&pool).await.unwrap();
    let request_rt = rt.clone();
    let task =
        tokio::spawn(async move { delete(&request_rt, target, "cancelled-acceptance").await });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'INSERT INTO privacy_operations%')")
                .fetch_one(&pool).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("request never reached the actual post-fsync insert barrier");
    assert_eq!(rt.deletion_journal().unwrap().records().unwrap().len(), 1);
    assert!(!rt.store.privacy_ready());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        request(&rt, "GET", "/api/v1/state", &[]).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    barrier.rollback().await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(5),
        sqlx::raw_sql(
            "DROP TRIGGER p01_wait_accept ON privacy_operations; DROP FUNCTION p01_wait_accept();",
        )
        .execute(&pool),
    )
    .await
    .unwrap()
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), rt.privacy_maintenance_once())
        .await
        .unwrap()
        .unwrap();
    assert!(rt.store.privacy_ready());
    let (status, receipt) = delete(&rt, target, "cancelled-acceptance").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(receipt["status"], "completed");
    assert_eq!(
        request(&rt, "GET", &format!("/api/v1/memories/{target}"), &[])
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM privacy_operations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let mut restarted = Runtime::new(pool.clone(), TOKEN).unwrap();
    restarted
        .configure_deletion_journal(Some(&journal.path))
        .await
        .unwrap();
    assert_eq!(
        delete(&restarted, target, "cancelled-acceptance").await.1["operation_id"],
        receipt["operation_id"]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires explicitly owned disposable PostgreSQL and frozen v2 source"]
async fn verified_recovery_must_not_revive_unpolled_old_sse_queues_or_idle_streams(pool: PgPool) {
    let journal = Journal::new();
    let rt = setup(&pool, &journal).await;
    let before = head(&rt).await;
    let (target, _) = memory_with_notification(&pool).await;
    let mut queued = subscribe(&rt, &before).await;
    let mut idle = subscribe(&rt, &head(&rt).await).await;
    let old_generation = rt.store.privacy_generation();
    sqlx::raw_sql("CREATE FUNCTION p01_recovery_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic p01 recovery fault'; END $$; CREATE TRIGGER p01_recovery_fault BEFORE INSERT ON privacy_operations FOR EACH ROW EXECUTE FUNCTION p01_recovery_fault();")
        .execute(&pool).await.unwrap();
    let (status, failure) = delete(&rt, target, "unpolled-across-recovery").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(failure["error"]["code"], "privacy_unavailable");
    assert!(!rt.store.privacy_ready());
    assert!(rt.store.privacy_generation() > old_generation);
    assert_eq!(rt.deletion_journal().unwrap().records().unwrap().len(), 1);
    // Neither stream is polled while the gate is closed. A readiness-only guard
    // would miss the entire failure interval after recovery reopens access.
    sqlx::raw_sql("DROP TRIGGER p01_recovery_fault ON privacy_operations; DROP FUNCTION p01_recovery_fault();")
        .execute(&pool).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), rt.privacy_maintenance_once())
        .await
        .unwrap()
        .unwrap();
    assert!(rt.store.privacy_ready());
    assert!(rt.store.privacy_generation() > old_generation);
    let (status, recovered) = delete(&rt, target, "unpolled-across-recovery").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(recovered["status"], "completed");
    assert_eq!(
        request(&rt, "GET", &format!("/api/v1/memories/{target}"), &[])
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let new_cursor = head(&rt).await;
    let (independent, _) = memory_with_notification(&pool).await;
    assert!(
        frame(&mut queued).await.is_none(),
        "recovered readiness revived an unpolled old private queue"
    );
    assert!(
        frame(&mut idle).await.is_none(),
        "recovered readiness revived an old idle subscription"
    );
    // A fresh authorized subscription must still deliver an independent source;
    // closing every stream forever cannot satisfy this recovery regression.
    assert_eq!(
        request(&rt, "GET", &format!("/api/v1/memories/{independent}"), &[])
            .await
            .status(),
        StatusCode::OK
    );
    let mut fresh = subscribe(&rt, &new_cursor).await;
    let hint = frame(&mut fresh)
        .await
        .expect("fresh independent hint missing");
    assert!(hint.contains(&independent.to_string()));
    assert!(!hint.contains(&target.to_string()));
    assert!(!hint.contains("SYNTHETIC-P01-PRIVATE-BODY"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM privacy_operations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
