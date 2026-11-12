//! Actual owner HTTP/startup/maintenance against isolated PostgreSQL and journals.
use alia::{
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::{Runtime, capability::IMPLEMENTED_CAPABILITIES},
    service,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt, path::PathBuf};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "root-private-http-isolated-test-token";
struct Journal {
    directory: PathBuf,
    path: PathBuf,
}
impl Journal {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("alia-root-privacy-{}", Uuid::new_v4()));
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
fn app(rt: &Runtime) -> Router {
    service::router(rt.clone(), PathBuf::from("web/dist"))
}
async fn request(
    rt: &Runtime,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {TOKEN}"));
    for (key, value) in headers {
        builder = builder.header(*key, *value);
    }
    let response = app(rt)
        .oneshot(builder.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn memory(pool: &PgPool, marker: &str) -> (Uuid, Uuid) {
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
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,$1,'message.received','reported_claim','owner',clock_timestamp(),jsonb_build_object('text',$2::text),$1 FROM individuals").bind(event).bind(marker).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memories(individual_id,id,kind,content,scope) SELECT id,$1,'semantic',$2,'test' FROM individuals").bind(memory).bind(marker).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,$1,$2 FROM individuals").bind(memory).bind(event).execute(pool).await.unwrap();
    (memory, event)
}
async fn delete(rt: &Runtime, target: Uuid, key: &str) -> (StatusCode, Value) {
    request(
        rt,
        "DELETE",
        &format!("/api/v1/memories/{target}"),
        &[("idempotency-key", key), ("if-match", "\"1\"")],
        "",
    )
    .await
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires explicitly disposable PostgreSQL"]
async fn accepted_deletion_is_opaque_blocked_then_actually_purged_and_replayed(pool: PgPool) {
    let journal = Journal::new();
    let rt = setup(&pool, &journal).await;
    let (target, event) = memory(&pool, "private-delete-marker").await;
    let (other, _) = memory(&pool, "independent-marker").await;
    let (status, management) = request(&rt, "GET", "/api/v1/management", &[], "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(management["privacy"]["deletion_enabled"], true);
    let before = rt.store.snapshot().await.unwrap().unwrap();
    let (status, receipt) = delete(&rt, target, "same-intention").await;
    assert_eq!(status, StatusCode::ACCEPTED, "{receipt}");
    assert_eq!(receipt.as_object().unwrap().len(), 6);
    assert_eq!(receipt["status"], "blocked");
    assert!(receipt["privacy_epoch"].as_i64().unwrap() > before.privacy_epoch);
    assert!(!receipt.to_string().contains("private-delete-marker"));
    assert_eq!(
        request(&rt, "GET", &format!("/api/v1/memories/{target}"), &[], "")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(delete(&rt, target, "same-intention").await.1, receipt);
    assert_eq!(
        delete(&rt, other, "same-intention").await.0,
        StatusCode::CONFLICT
    );
    let operation = receipt["operation_id"].as_str().unwrap();
    let path = format!("/api/v1/privacy-operations/{operation}");
    assert_eq!(
        request(&rt, "GET", &path, &[], "").await.1["status"],
        "blocked"
    );
    assert_eq!(rt.privacy_maintenance_once().await.unwrap(), 1);
    let progress = request(&rt, "GET", &path, &[], "").await.1;
    assert_eq!(progress["status"], "completed");
    assert!(progress["last_error"].is_null());
    let body: String = sqlx::query_scalar("SELECT content FROM memories WHERE id=$1")
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
    let payload: Value = sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(event)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!body.contains("private-delete-marker"));
    assert_eq!(payload, json!({}));
    assert_eq!(
        request(&rt, "GET", &format!("/api/v1/memories/{other}"), &[], "")
            .await
            .0,
        StatusCode::OK
    );
    let mut restarted = Runtime::new(pool.clone(), TOKEN).unwrap();
    restarted
        .configure_deletion_journal(Some(&journal.path))
        .await
        .unwrap();
    let (status, replay) = delete(&restarted, target, "same-intention").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(replay["operation_id"], receipt["operation_id"]);
    assert_eq!(replay["status"], "completed");
    assert_eq!(restarted.privacy_maintenance_once().await.unwrap(), 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires explicitly disposable PostgreSQL"]
async fn invalid_http_intentions_do_not_write_and_disabled_deletion_is_honest(pool: PgPool) {
    let journal = Journal::new();
    let rt = setup(&pool, &journal).await;
    let (target, _) = memory(&pool, "unchanged").await;
    let path = format!("/api/v1/memories/{target}");
    for (url, headers, body) in [
        (path.clone(), vec![], ""),
        (
            path.clone(),
            vec![("idempotency-key", "bad"), ("if-match", "W/\"1\"")],
            "",
        ),
        (
            path.clone(),
            vec![("idempotency-key", "bad"), ("if-match", "\"0\"")],
            "",
        ),
        (
            path.clone(),
            vec![
                ("idempotency-key", "bad"),
                ("idempotency-key", "duplicate"),
                ("if-match", "\"1\""),
            ],
            "",
        ),
        (
            path.clone(),
            vec![("idempotency-key", "bad"), ("if-match", "\"1\"")],
            "{}",
        ),
        (
            format!("{path}?ignored=true"),
            vec![("idempotency-key", "bad"), ("if-match", "\"1\"")],
            "",
        ),
    ] {
        assert_eq!(
            request(&rt, "DELETE", &url, &headers, body).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM privacy_operations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0);
    assert_eq!(
        request(
            &rt,
            "GET",
            &format!("/api/v1/privacy-operations/{}", Uuid::new_v4()),
            &[],
            ""
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let mut disabled = Runtime::new(pool.clone(), TOKEN).unwrap();
    disabled.configure_deletion_journal(None).await.unwrap();
    let (status, body) = delete(&disabled, target, "disabled").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "feature_unavailable");
    assert_eq!(
        request(&disabled, "GET", "/api/v1/management", &[], "")
            .await
            .1["privacy"]["deletion_enabled"],
        false
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires explicitly disposable PostgreSQL"]
async fn startup_refuses_missing_or_corrupt_journal_and_keeps_all_clones_quarantined(pool: PgPool) {
    let journal = Journal::new();
    let rt = setup(&pool, &journal).await;
    let (target, _) = memory(&pool, "pending").await;
    assert_eq!(delete(&rt, target, "startup").await.0, StatusCode::ACCEPTED);
    let mut missing = Runtime::new(pool.clone(), TOKEN).unwrap();
    assert!(missing.configure_deletion_journal(None).await.is_err());
    assert!(!missing.store.privacy_ready());
    let absent = journal.directory.join("missing.jsonl");
    assert!(
        missing
            .configure_deletion_journal(Some(&absent))
            .await
            .is_err()
    );
    assert!(!absent.exists());
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&journal.path)
        .unwrap();
    file.write_all(b"truncated").unwrap();
    file.sync_all().unwrap();
    let mut broken = Runtime::new(pool.clone(), TOKEN).unwrap();
    let clone = broken.store.clone();
    assert!(
        broken
            .configure_deletion_journal(Some(&journal.path))
            .await
            .is_err()
    );
    assert!(!clone.privacy_ready());
    let (status, body) = request(&broken, "GET", "/api/v1/state", &[], "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "privacy_unavailable");
    let unauthorized = app(&broken)
        .oneshot(
            Request::builder()
                .uri("/api/v1/state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let health = app(&broken)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires explicitly disposable PostgreSQL"]
async fn durable_intent_after_http_commit_failure_is_recovered_before_access_reopens(pool: PgPool) {
    let journal = Journal::new();
    let rt = setup(&pool, &journal).await;
    let (target, _) = memory(&pool, "fsync-then-rollback-marker").await;
    sqlx::raw_sql("CREATE FUNCTION fail_privacy_accept() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'qa fault'; END $$; CREATE TRIGGER qa_privacy_accept BEFORE INSERT ON privacy_operations FOR EACH ROW EXECUTE FUNCTION fail_privacy_accept();").execute(&pool).await.unwrap();
    let (status, body) = delete(&rt, target, "recover-http-intention").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "privacy_unavailable");
    assert!(!rt.store.privacy_ready());
    assert_eq!(rt.deletion_journal().unwrap().records().unwrap().len(), 1);
    assert_eq!(
        request(&rt, "GET", "/api/v1/memories", &[], "").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    sqlx::raw_sql("DROP TRIGGER qa_privacy_accept ON privacy_operations; DROP FUNCTION fail_privacy_accept();").execute(&pool).await.unwrap();
    rt.privacy_maintenance_once().await.unwrap();
    assert!(rt.store.privacy_ready());
    let (status, receipt) = delete(&rt, target, "recover-http-intention").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(receipt["status"], "completed");
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM privacy_operations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jobs, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires explicitly disposable PostgreSQL"]
async fn actual_background_loop_purges_without_user_messages_and_stops_on_shutdown(pool: PgPool) {
    let journal = Journal::new();
    let rt = setup(&pool, &journal).await;
    let (target, _) = memory(&pool, "background-marker").await;
    let task = tokio::spawn(rt.clone().serve_privacy(rt.shutdown.subscribe()));
    let (status, receipt) = delete(&rt, target, "background").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let operation = Uuid::parse_str(receipt["operation_id"].as_str().unwrap()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            if rt.store.privacy_operation(operation).await.unwrap().status
                == alia::domain::privacy::DeletionStatus::Completed
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    rt.shutdown.send(true).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    let calls: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(calls, 0);
}
