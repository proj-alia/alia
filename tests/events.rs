use alia::{
    adapters::notifications::NotificationError,
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::Runtime,
    service,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use std::{path::PathBuf, time::Duration};
use tower::ServiceExt;
const TOKEN: &str = "isolated-event-stream-testing-token-value";
fn runtime(pool: &PgPool) -> Runtime {
    Runtime::new(pool.clone(), TOKEN).unwrap()
}
async fn init(rt: &Runtime) {
    let seed = PersonaSeed::validate(
        serde_json::from_str(include_str!("../config/persona.example.json")).unwrap(),
    )
    .unwrap();
    let config = RuntimeRules::validate(
        serde_json::from_str(include_str!("../config/runtime.example.json")).unwrap(),
        alia::runtime::capability::IMPLEMENTED_CAPABILITIES,
    )
    .unwrap();
    rt.store.initialize("birth", &seed, &config).await.unwrap();
}
async fn request(rt: &Runtime, path: &str, headers: &[(&str, &str)]) -> Response {
    let mut request = Request::builder()
        .uri(path)
        .header("authorization", format!("Bearer {TOKEN}"));
    for (k, v) in headers {
        request = request.header(*k, *v);
    }
    service::router(rt.clone(), PathBuf::from("web/dist"))
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}
async fn frame(body: &mut Body) -> String {
    let frame = tokio::time::timeout(Duration::from_secs(3), body.frame())
        .await
        .expect("stream stalled")
        .expect("stream ended")
        .unwrap();
    String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap()
}
async fn head(rt: &Runtime) -> String {
    rt.store.public_state().await.unwrap()["event_cursor"]
        .as_str()
        .unwrap()
        .to_owned()
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn stream_replay_is_metadata_only_and_does_not_execute_commands(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let before = head(&rt).await;
    let receipt = rt
        .store
        .submit_message("message", "PRIVATE BODY MUST NOT BE IN SSE", None)
        .await
        .unwrap();
    let response = request(&rt, "/api/v1/events", &[("last-event-id", &before)]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let mut body = response.into_body();
    assert!(frame(&mut body).await.contains("connected"));
    let batch = rt
        .store
        .notifications(Some(before.parse().unwrap()))
        .await
        .unwrap();
    let mut delivered = vec![];
    for notice in &batch.items {
        let s = frame(&mut body).await;
        assert!(s.contains(&format!("id: {}", notice.sequence)));
        assert!(!s.contains("PRIVATE BODY"));
        delivered.push(s);
    }
    assert!(
        delivered
            .iter()
            .any(|s| s.contains(&receipt.event_id.to_string()))
    );
    drop(body);
    let restarted = runtime(&pool);
    let mut replay = request(&restarted, "/api/v1/events", &[("last-event-id", &before)])
        .await
        .into_body();
    frame(&mut replay).await;
    for expected in delivered {
        assert_eq!(frame(&mut replay).await, expected);
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn cursor_validation_and_snapshot_handoff(pool: PgPool) {
    let rt = runtime(&pool);
    assert_eq!(
        request(&rt, "/api/v1/events", &[]).await.status(),
        StatusCode::CONFLICT
    );
    init(&rt).await;
    for path in [
        "/api/v1/events?after=-1",
        "/api/v1/events?after=+1",
        "/api/v1/events?after=9223372036854775808",
        "/api/v1/events?after=1&after=2",
        "/api/v1/events?unknown=1",
        "/api/v1/events?after=9999",
    ] {
        assert_eq!(
            request(&rt, path, &[]).await.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path}"
        );
    }
    assert_eq!(
        request(&rt, "/api/v1/events?after=0", &[("last-event-id", "1")])
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        request(
            &rt,
            "/api/v1/events",
            &[("last-event-id", "0"), ("last-event-id", "0")]
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let baseline = head(&rt).await;
    let mut body = request(&rt, "/api/v1/events", &[("last-event-id", &baseline)])
        .await
        .into_body();
    frame(&mut body).await;
    rt.store
        .submit_message("later", "after snapshot", None)
        .await
        .unwrap();
    let update = frame(&mut body).await;
    assert!(!update.contains("connected"));
    assert!(update.contains("data:"));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn blocked_sources_and_unknown_types_only_advance_cursor(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let before = head(&rt).await.parse().unwrap();
    let receipt = rt
        .store
        .submit_message("blocked", "private source", None)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(receipt.event_id)
        .execute(&pool)
        .await
        .unwrap();
    let individual = rt.store.snapshot().await.unwrap().unwrap().id;
    sqlx::query("INSERT INTO outbox(individual_id,event_type,resource_id,version) VALUES($1,'future.unrecognized',$1,1)").bind(individual).execute(&pool).await.unwrap();
    let batch = rt.store.notifications(Some(before)).await.unwrap();
    assert!(batch.items.iter().any(|n| n.resource.is_none()));
    assert!(
        !batch
            .items
            .iter()
            .filter_map(|n| n.resource.as_ref())
            .any(|(_, id, _)| *id == receipt.event_id || *id == receipt.operation_id)
    );
    let mut body = request(
        &rt,
        "/api/v1/events",
        &[("last-event-id", &before.to_string())],
    )
    .await
    .into_body();
    frame(&mut body).await;
    for n in batch.items {
        let s = frame(&mut body).await;
        if n.resource.is_none() {
            assert!(!s.contains("data:"));
            assert!(s.contains("cursor"));
        }
        assert!(!s.contains(&receipt.event_id.to_string()));
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn expiration_watermark_survives_empty_outbox_and_restart(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let high = head(&rt).await;
    sqlx::query("UPDATE outbox SET created_at=now()-interval '8 days'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        rt.store.notifications(Some(0)).await,
        Err(NotificationError::Expired)
    ));
    let r = request(&rt, "/api/v1/events?after=0", &[]).await;
    assert_eq!(r.status(), StatusCode::GONE);
    let error: Value =
        serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(error["error"]["code"], "event_cursor_expired");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let rt = runtime(&pool);
    assert_eq!(head(&rt).await, high);
    assert!(
        rt.store
            .notifications(Some(high.parse().unwrap()))
            .await
            .unwrap()
            .items
            .is_empty()
    );
    rt.store
        .submit_message("new", "new observation", None)
        .await
        .unwrap();
    assert!(
        !rt.store
            .notifications(Some(high.parse().unwrap()))
            .await
            .unwrap()
            .items
            .is_empty()
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn retention_is_bounded_and_preserves_clock_reversed_suffix(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let id = rt.store.snapshot().await.unwrap().unwrap().id;
    sqlx::query("INSERT INTO outbox(individual_id,event_type,resource_id,version,created_at) SELECT $1,'state.changed',$1,1,now()-interval '8 days' FROM generate_series(1,1100)").bind(id).execute(&pool).await.unwrap();
    // The first live row pins the suffix, despite expired timestamps behind it.
    rt.store.prune_notifications().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1101);
    sqlx::query("UPDATE outbox SET created_at=now()-interval '8 days'")
        .execute(&pool)
        .await
        .unwrap();
    rt.store.prune_notifications().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 101);
    assert!(matches!(
        rt.store.notifications(Some(1000)).await,
        Err(NotificationError::Expired)
    ));
    rt.store.prune_notifications().await.unwrap();
    assert_eq!(head(&rt).await, "1101");
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn idle_stream_releases_database_and_ends_on_shutdown(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let baseline = head(&rt).await;
    let response = request(&rt, "/api/v1/events", &[("last-event-id", &baseline)]).await;
    let mut body = response.into_body();
    frame(&mut body).await;
    // A subscribed stream holds only a semaphore permit, not a DB transaction.
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND state='idle in transaction'").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
    rt.shutdown.send_replace(true);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), body.frame())
            .await
            .unwrap()
            .is_none()
    );
    drop(body);
    assert_eq!(rt.event_slots.available_permits(), 16);
    assert_eq!(
        request(&rt, "/api/v1/events", &[]).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn subscription_limit_is_released_on_disconnect_and_batches_are_bounded(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let mut connections = vec![];
    for _ in 0..16 {
        let r = request(&rt, "/api/v1/events", &[]).await;
        assert_eq!(r.status(), StatusCode::OK);
        connections.push(r);
    }
    assert_eq!(
        request(&rt, "/api/v1/events", &[]).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    connections.pop();
    assert_eq!(
        request(&rt, "/api/v1/events", &[]).await.status(),
        StatusCode::OK
    );
    drop(connections);
    assert_eq!(rt.event_slots.available_permits(), 16);
    let id = rt.store.snapshot().await.unwrap().unwrap().id;
    sqlx::query("INSERT INTO outbox(individual_id,event_type,resource_id,version) SELECT $1,'state.changed',$1,1 FROM generate_series(1,210)").bind(id).execute(&pool).await.unwrap();
    let mut cursor = 0;
    let mut total = 0;
    loop {
        let batch = rt.store.notifications(Some(cursor)).await.unwrap();
        if batch.items.is_empty() {
            break;
        }
        assert!(batch.items.len() <= 100);
        total += batch.items.len();
        cursor = batch.items.last().unwrap().sequence;
    }
    assert_eq!(total, 211);
}
