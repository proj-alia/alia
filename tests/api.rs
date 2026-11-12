use alia::{
    adapters::web::HttpWebReader,
    domain::history::{MemoryKind, NewMemory},
    runtime::{
        Runtime,
        model::FixtureProvider,
        worker::{Worker, WorkerOutcome},
    },
    service,
};
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{path::PathBuf, sync::Arc};
use tower::ServiceExt;
use uuid::Uuid;
const TOKEN: &str = "test-instance-token-for-isolated-api-tests";
fn runtime(pool: &PgPool) -> Runtime {
    Runtime::new(pool.clone(), TOKEN).unwrap()
}
fn app(rt: &Runtime) -> Router {
    service::router(rt.clone(), PathBuf::from("web/dist"))
}
async fn raw(
    app: Router,
    method: &str,
    path: &str,
    body: &str,
    extra: &[(&str, &str)],
) -> (StatusCode, HeaderMap, Value) {
    let mut r = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("content-type", "application/json")
        .header("host", "localhost:3000");
    for (k, v) in extra {
        r = r.header(*k, *v);
    }
    let response = app
        .oneshot(r.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let v = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("non-JSON response for {method} {path}: {status}"));
    (status, headers, v)
}
async fn call(
    app: Router,
    method: &str,
    path: &str,
    v: Value,
    key: Option<&str>,
    version: Option<i64>,
) -> (StatusCode, HeaderMap, Value) {
    let etag = version.map(|v| format!("\"{v}\""));
    let mut h = vec![];
    if let Some(k) = key {
        h.push(("idempotency-key", k));
    }
    if let Some(v) = &etag {
        h.push(("if-match", v.as_str()));
    }
    raw(app, method, path, &v.to_string(), &h).await
}
fn initialization() -> Value {
    json!({"persona":serde_json::from_str::<Value>(include_str!("../config/persona.example.json")).unwrap(),"runtime":serde_json::from_str::<Value>(include_str!("../config/runtime.example.json")).unwrap()})
}
async fn init(rt: &Runtime) -> Value {
    let (s, _, v) = call(
        app(rt),
        "POST",
        "/api/v1/initialize",
        initialization(),
        Some("birth"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v
}
async fn version(rt: &Runtime) -> i64 {
    let (s, _, v) = call(app(rt), "GET", "/api/v1/state", Value::Null, None, None).await;
    assert_eq!(s, StatusCode::OK);
    v["version"].as_i64().unwrap()
}
async fn count(pool: &PgPool, q: &str) -> i64 {
    sqlx::query_scalar(q).fetch_one(pool).await.unwrap()
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn initialization_state_and_duplicate_requests_survive_new_router(pool: PgPool) {
    let rt = runtime(&pool);
    let (s, _, v) = call(app(&rt), "GET", "/api/v1/state", Value::Null, None, None).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(v["error"]["code"], "not_initialized");
    let created = init(&rt).await;
    let fresh = runtime(&pool);
    let (s, h, v) = call(app(&fresh), "GET", "/api/v1/state", Value::Null, None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["etag"], "\"1\"");
    assert_eq!(h["cache-control"], "no-store");
    assert_eq!(v["individual_id"], created["individual_id"]);
    assert_eq!(v["model"]["readiness"], "unavailable");
    assert_eq!(v["capabilities"].as_array().unwrap().len(), 4);
    assert!(!v.to_string().contains(TOKEN));
    assert_eq!(
        call(
            app(&fresh),
            "POST",
            "/api/v1/initialize",
            initialization(),
            Some("birth"),
            None
        )
        .await
        .2,
        created
    );
    assert_eq!(
        call(
            app(&fresh),
            "POST",
            "/api/v1/initialize",
            initialization(),
            Some("other"),
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let mut other = initialization();
    other["persona"]["identity"]["display_name"] = json!("另一个名称");
    assert_eq!(
        call(
            app(&fresh),
            "POST",
            "/api/v1/initialize",
            other,
            Some("birth"),
            None
        )
        .await
        .2["error"]["code"],
        "idempotency_conflict"
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM individuals").await, 1);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn api_message_runs_in_same_individual_and_history_is_paginated(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let input = json!({"text":"你好，我偏好先研究","conversation_id":Uuid::new_v4()});
    let (s, _, receipt) = call(
        app(&rt),
        "POST",
        "/api/v1/messages",
        input.clone(),
        Some("message"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::ACCEPTED);
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/messages",
            input,
            Some("message"),
            None
        )
        .await
        .2,
        receipt
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM runs").await, 1);
    let operation = format!(
        "/api/v1/operations/{}",
        receipt["operation_id"].as_str().unwrap()
    );
    assert_eq!(
        call(app(&rt), "GET", &operation, Value::Null, None, None)
            .await
            .2["status"],
        "queued"
    );
    let fixture=FixtureProvider{response:json!({"schema_version":1,"reply":"我会记得这次交流。","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"回应实际输入"}).to_string()};
    assert!(matches!(
        Worker::new(
            rt.store.clone(),
            Arc::new(fixture),
            Arc::new(HttpWebReader),
            rt.wake.clone()
        )
        .run_once()
        .await
        .unwrap(),
        WorkerOutcome::Committed(_)
    ));
    let completed = call(app(&rt), "GET", &operation, Value::Null, None, None)
        .await
        .2;
    assert_eq!(completed["status"], "committed");
    assert_eq!(completed["message_ids"].as_array().unwrap().len(), 1);
    let first = call(
        app(&rt),
        "GET",
        "/api/v1/messages?limit=1",
        Value::Null,
        None,
        None,
    )
    .await
    .2;
    assert_eq!(first["items"][0]["role"], "user");
    let url = format!(
        "/api/v1/messages?limit=1&cursor={}",
        first["next_cursor"].as_str().unwrap()
    );
    let second = call(app(&rt), "GET", &url, Value::Null, None, None).await.2;
    assert_eq!(second["items"][0]["role"], "assistant");
    assert!(second["next_cursor"].is_null());
    let latest = call(
        app(&rt),
        "GET",
        "/api/v1/messages?order=desc&limit=1",
        Value::Null,
        None,
        None,
    )
    .await
    .2;
    assert_eq!(latest["items"][0]["role"], "assistant");
    let older = call(
        app(&rt),
        "GET",
        &format!(
            "/api/v1/messages?order=desc&limit=1&cursor={}",
            latest["next_cursor"].as_str().unwrap()
        ),
        Value::Null,
        None,
        None,
    )
    .await
    .2;
    assert_eq!(older["items"][0]["role"], "user");
    assert!(older["next_cursor"].is_null());

    assert_eq!(
        call(
            app(&rt),
            "GET",
            "/api/v1/operations/bad",
            Value::Null,
            None,
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(
            app(&rt),
            "GET",
            &format!("/api/v1/operations/{}", Uuid::new_v4()),
            Value::Null,
            None,
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn malformed_unknown_duplicate_and_oversized_input_never_writes(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    for raw_body in [
        "{",
        r#"{"text":"a","extra":1}"#,
        r#"{"text":"a","text":"b"}"#,
        r#"{"text":7}"#,
        r#"{"text":"a","conversation_id":"bad"}"#,
        r#"{"text":"\u0000"}"#,
    ] {
        let (status, _, v) = raw(
            app(&rt),
            "POST",
            "/api/v1/messages",
            raw_body,
            &[("idempotency-key", "bad")],
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
        assert!(v["error"]["request_id"].is_string());
    }
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/messages",
            json!({"text":"hello"}),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/messages",
            json!({"text":"x".repeat(16385)}),
            Some("large"),
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/messages",
            json!({"text":"x".repeat(140000)}),
            Some("very-large"),
            None
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM runs").await, 0);
    for url in [
        "/api/v1/messages?limit=0",
        "/api/v1/messages?limit=101",
        "/api/v1/messages?cursor=v1:goals:bad",
        "/api/v1/messages?unexpected=1",
        "/api/v1/memories?validity=deleted",
    ] {
        assert_eq!(
            call(app(&rt), "GET", url, Value::Null, None, None).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let mut input = initialization();
    input["runtime"]["capabilities"]["enabled"] = json!(["shell.execute"]);
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/initialize",
            input,
            Some("bad-config"),
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn origin_auth_methods_and_unimplemented_routes_are_distinct(pool: PgPool) {
    let rt = runtime(&pool);
    for origin in [
        "https://evil.example",
        "null",
        "http://localhost:3001",
        "http://localhost:3000/path",
        "http://user@localhost:3000",
    ] {
        assert_eq!(
            raw(
                app(&rt),
                "GET",
                "/api/v1/state",
                "null",
                &[("origin", origin)]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        raw(
            app(&rt),
            "GET",
            "/api/v1/state",
            "null",
            &[("origin", "http://localhost:3000")]
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        raw(
            app(&rt),
            "GET",
            "/api/v1/state",
            "null",
            &[("sec-fetch-site", "cross-site")]
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        raw(
            app(&rt),
            "GET",
            "/api/v1/state",
            "null",
            &[("authorization", "Bearer other")]
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(app(&rt), "GET", "/api/v1/unknown", Value::Null, None, None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(app(&rt), "POST", "/api/v1/state", json!({}), None, None)
            .await
            .0,
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        call(app(&rt), "POST", "/api/v1/exports", Value::Null, None, None)
            .await
            .0,
        StatusCode::NOT_IMPLEMENTED
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn pause_settings_and_lifecycle_have_atomic_versioned_idempotency(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let v = version(&rt).await;
    let (s, h, paused) = call(
        app(&rt),
        "POST",
        "/api/v1/autonomy/pause",
        json!({}),
        Some("pause"),
        Some(v),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["etag"], format!("\"{}\"", v + 1));
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/autonomy/pause",
            json!({}),
            Some("pause"),
            Some(v)
        )
        .await
        .2,
        paused
    );
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/autonomy/resume",
            json!({}),
            Some("stale"),
            Some(v)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    // Pause blocks background; a new foreground message is still accepted.
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/messages",
            json!({"text":"暂停后仍可交流"}),
            Some("still-chat"),
            None
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    let v = version(&rt).await;
    let mut rules = initialization()["runtime"].clone();
    rules["budgets"]["background_tokens_per_day"] = json!(12345);
    let saved = call(
        app(&rt),
        "PUT",
        "/api/v1/settings",
        rules.clone(),
        Some("settings"),
        Some(v),
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK);
    assert_eq!(saved.2["config_version"], 2);
    assert_eq!(
        call(
            app(&rt),
            "PUT",
            "/api/v1/settings",
            rules,
            Some("settings"),
            Some(v)
        )
        .await
        .2,
        saved.2
    );
    for action in ["archive", "restore", "resume", "terminate"] {
        let v = version(&rt).await;
        assert_eq!(
            call(
                app(&rt),
                "POST",
                "/api/v1/lifecycle/transitions",
                json!({"action":action}),
                Some(action),
                Some(v)
            )
            .await
            .0,
            StatusCode::OK
        );
        if action == "restore" {
            assert_eq!(
                call(
                    app(&rt),
                    "POST",
                    "/api/v1/messages",
                    json!({"text":"不允许"}),
                    Some("paused-message"),
                    None
                )
                .await
                .0,
                StatusCode::CONFLICT
            );
        }
    }
    let v = version(&rt).await;
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/lifecycle/transitions",
            json!({"action":"resume"}),
            Some("revive"),
            Some(v)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM individuals").await, 1);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn concurrent_owner_changes_cannot_overwrite_each_other(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let (a, b) = tokio::join!(
        call(
            app(&rt),
            "POST",
            "/api/v1/autonomy/pause",
            json!({}),
            Some("a"),
            Some(1)
        ),
        call(
            app(&rt),
            "POST",
            "/api/v1/autonomy/resume",
            json!({}),
            Some("b"),
            Some(1)
        )
    );
    assert_eq!(
        [a.0, b.0].iter().filter(|s| **s == StatusCode::OK).count(),
        1
    );
    assert_eq!(
        [a.0, b.0]
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='autonomy.changed'"
        )
        .await,
        1
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn document_import_and_recursive_visibility_respect_source_barriers(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let (s, _, doc) = call(
        app(&rt),
        "POST",
        "/api/v1/documents",
        json!({"title":"资料","text":"文".repeat(60000),"source_label":"owner"}),
        Some("doc"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{doc}");
    let source = Uuid::parse_str(doc["event_id"].as_str().unwrap()).unwrap();
    let state = rt.store.snapshot().await.unwrap().unwrap();
    let memories = rt
        .store
        .record_memories(
            "mem",
            state.expected(),
            &[source],
            &[NewMemory {
                content: "具有独立来源的认识".into(),
                scope: "general".into(),
                kind: MemoryKind::Semantic,
                source_event_ids: vec![source],
                supersedes: None,
            }],
        )
        .await
        .unwrap();
    let m = rt
        .store
        .submit_message("message", "隐私经历", None)
        .await
        .unwrap();
    // Source -> memory -> message -> run: no direct blocked flag on descendants.
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES($1,$2,$3)")
        .bind(state.id)
        .bind(memories.memory_ids[0])
        .bind(m.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(app(&rt), "GET", "/api/v1/messages", Value::Null, None, None)
            .await
            .2["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(
            app(&rt),
            "GET",
            "/api/v1/memories?q=认识",
            Value::Null,
            None,
            None
        )
        .await
        .2["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    for path in ["/api/v1/messages", "/api/v1/memories"] {
        assert!(
            call(app(&rt), "GET", path, Value::Null, None, None).await.2["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(
        call(
            app(&rt),
            "GET",
            &format!("/api/v1/operations/{}", m.operation_id),
            Value::Null,
            None,
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn failed_owner_transaction_is_retryable_and_returns_no_database_details(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    sqlx::query("CREATE FUNCTION api_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'private-database-detail'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER api_failure BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION api_failure()").execute(&pool).await.unwrap();
    let (s, _, v) = call(
        app(&rt),
        "POST",
        "/api/v1/autonomy/pause",
        json!({}),
        Some("retry"),
        Some(1),
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error"]["retryable"], true);
    assert!(!v.to_string().contains("private-database-detail"));
    assert_eq!(version(&rt).await, 1);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='autonomy.changed'"
        )
        .await,
        0
    );
    sqlx::query("DROP TRIGGER api_failure ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/autonomy/pause",
            json!({}),
            Some("retry"),
            Some(1)
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn goal_views_and_counts_drop_blocked_ancestors_without_mutating_history(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let receipt = rt
        .store
        .submit_message("source", "请探索资料", None)
        .await
        .unwrap();
    let provider=FixtureProvider{response:json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[{"action":"create","origin_type":"event","origin_ids":[receipt.event_id],"description":"探索问题","success_criterion":"整理一个有依据的结论","next_step":"阅读资料","stop_condition":"没有资料则等待","priority":50}],"tool_intents":[],"development":null,"brief_reason":"基于请求形成目标"}).to_string()};
    assert!(matches!(
        Worker::new(
            rt.store.clone(),
            Arc::new(provider),
            Arc::new(HttpWebReader),
            rt.wake.clone()
        )
        .run_once()
        .await
        .unwrap(),
        WorkerOutcome::Committed(_)
    ));
    let goals = call(app(&rt), "GET", "/api/v1/goals", Value::Null, None, None)
        .await
        .2;
    assert_eq!(goals["items"].as_array().unwrap().len(), 1);
    assert_eq!(goals["items"][0]["description"], "探索问题");
    assert_eq!(
        call(app(&rt), "GET", "/api/v1/state", Value::Null, None, None)
            .await
            .2["goals"]["proposed"],
        1
    );
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(receipt.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        call(app(&rt), "GET", "/api/v1/goals", Value::Null, None, None)
            .await
            .2["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        call(app(&rt), "GET", "/api/v1/state", Value::Null, None, None)
            .await
            .2["goals"],
        json!({})
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM goals").await, 1);
}

fn goal_draft() -> Value {
    json!({"description":"理解测试资料中的一个问题","success_criterion":"保存有来源的研究笔记","next_step":"先阅读相关资料","stop_condition":"来源不可用或预算耗尽时等待"})
}
async fn owner_goal(rt: &Runtime, key: &str) -> Value {
    let (s, _, v) = call(
        app(rt),
        "POST",
        "/api/v1/goals",
        goal_draft(),
        Some(key),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn owner_goal_creation_is_durable_sourced_idempotent_and_scheduled(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let (a, b) = tokio::join!(owner_goal(&rt, "same-goal"), owner_goal(&rt, "same-goal"));
    assert_eq!(a, b);
    let id = Uuid::parse_str(a["goal_id"].as_str().unwrap()).unwrap();
    let event = Uuid::parse_str(a["event_id"].as_str().unwrap()).unwrap();
    let kind: (String, String) = sqlx::query_as("SELECT kind,source_kind FROM events WHERE id=$1")
        .bind(event)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        kind,
        ("user.goal_requested".into(), "reported_claim".into())
    );
    let origins: Value = sqlx::query_scalar("SELECT origin_ids FROM goals WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(origins, json!([event]));
    assert_eq!(count(&pool, "SELECT count(*) FROM goals").await, 1);
    let restored = runtime(&pool);
    assert_eq!(owner_goal(&restored, "same-goal").await, a);
    let queued = restored.store.tick_goals().await.unwrap().unwrap();
    let goal: Uuid = sqlx::query_scalar("SELECT goal_id FROM runs WHERE id=$1")
        .bind(queued)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(goal, id);
    let mut changed = goal_draft();
    changed["next_step"] = json!("不同步骤");
    let (s, _, _) = call(
        app(&rt),
        "POST",
        "/api/v1/goals",
        changed,
        Some("same-goal"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn owner_goal_transitions_fence_queued_work_and_preserve_terminal_history(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let created = owner_goal(&rt, "g").await;
    let id = created["goal_id"].as_str().unwrap();
    let path = format!("/api/v1/goals/{id}/transitions");
    let old = rt.store.tick_goals().await.unwrap().unwrap();
    let wait = json!({"action":"wait","reason":"先等待更多资料"});
    let (s, h, w) = call(app(&rt), "POST", &path, wait.clone(), Some("wait"), Some(1)).await;
    assert_eq!(s, StatusCode::OK, "{w}");
    assert_eq!(h["etag"], "\"2\"");
    assert_eq!(w["status"], "waiting");
    let (s, _, replayed) = call(app(&rt), "POST", &path, wait, Some("wait"), Some(1)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(w, replayed);
    let status: String = sqlx::query_scalar("SELECT status FROM runs WHERE id=$1")
        .bind(old)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "stale");
    let (s, _, _) = call(
        app(&rt),
        "POST",
        &path,
        json!({"action":"activate","reason":"现在继续"}),
        Some("activate"),
        Some(2),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let next = rt.store.tick_goals().await.unwrap().unwrap();
    assert_ne!(old, next);
    let (s, _, abandoned) = call(
        app(&rt),
        "POST",
        &path,
        json!({"action":"abandon","reason":"研究方向已改变"}),
        Some("abandon"),
        Some(3),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{abandoned}");
    assert_eq!(abandoned["status"], "abandoned");
    assert_eq!(abandoned["goal_version"], 4);
    let (s, _, _) = call(
        app(&rt),
        "POST",
        &path,
        json!({"action":"activate","reason":"重开旧目标"}),
        Some("terminal"),
        Some(4),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='goal.owner_changed'"
        )
        .await,
        3
    );
    assert!(rt.store.tick_goals().await.unwrap().is_none());
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn owner_no_progress_resume_requires_fresh_external_evidence(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let old = rt
        .store
        .submit_message("old", "旧资料", None)
        .await
        .unwrap();
    let created = owner_goal(&rt, "g").await;
    let id = Uuid::parse_str(created["goal_id"].as_str().unwrap()).unwrap();
    let path = format!("/api/v1/goals/{id}/transitions");
    sqlx::query("UPDATE goals SET status='waiting',waiting_reason='no_progress',no_progress_cycles=3,waiting_since_sequence=(SELECT max(sequence) FROM events),due_at=NULL WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    for (key, evidence) in [
        ("empty", json!([])),
        ("old", json!([old.event_id])),
        ("invented", json!([Uuid::new_v4()])),
        ("self", json!([created["event_id"]])),
    ] {
        let (s, _, _) = call(
            app(&rt),
            "POST",
            &path,
            json!({"action":"activate","reason":"仅仅要求继续","evidence_ids":evidence}),
            Some(key),
            Some(1),
        )
        .await;
        assert!(
            matches!(s, StatusCode::UNPROCESSABLE_ENTITY | StatusCode::NOT_FOUND),
            "{key}: {s}"
        );
    }
    let fresh = rt
        .store
        .import_document("fresh", "新发现", "新的可检查资料内容", "隔离测试资料")
        .await
        .unwrap();
    // Imported document IDs refer to source events, not arbitrary objects.
    let event: Uuid = sqlx::query_scalar("SELECT event_id FROM documents LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let _ = fresh;
    let (s, _, r) = call(
        app(&rt),
        "POST",
        &path,
        json!({"action":"activate","reason":"有了新的资料","evidence_ids":[event]}),
        Some("fresh-resume"),
        Some(1),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let n: i32 = sqlx::query_scalar("SELECT no_progress_cycles FROM goals WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    // Fewer than three failures are not erased by waiting then activating.
    sqlx::query("UPDATE goals SET no_progress_cycles=2 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    call(
        app(&rt),
        "POST",
        &path,
        json!({"action":"wait","reason":"暂停一下"}),
        Some("again-wait"),
        Some(2),
    )
    .await;
    let (s, _, _) = call(
        app(&rt),
        "POST",
        &path,
        json!({"action":"activate","reason":"继续"}),
        Some("again-active"),
        Some(3),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let n: i32 = sqlx::query_scalar("SELECT no_progress_cycles FROM goals WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 2);
    let revision: i64 = sqlx::query_scalar("SELECT review_revision FROM goals WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        revision, 4,
        "owner intent changes must invalidate semantic reviews"
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn owner_goal_concurrency_privacy_and_validation_never_silently_overwrite(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    for (key, body) in [
        (
            "empty",
            json!({"description":" ","success_criterion":"x","next_step":"x","stop_condition":"x"}),
        ),
        (
            "unknown",
            json!({"description":"x","success_criterion":"x","next_step":"x","stop_condition":"x","status":"completed"}),
        ),
        (
            "long",
            json!({"description":"字".repeat(2001),"success_criterion":"x","next_step":"x","stop_condition":"x"}),
        ),
    ] {
        let (s, _, _) = call(app(&rt), "POST", "/api/v1/goals", body, Some(key), None).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM goals").await, 0);
    let goal = owner_goal(&rt, "g").await;
    let path = format!(
        "/api/v1/goals/{}/transitions",
        goal["goal_id"].as_str().unwrap()
    );
    let (a, b) = tokio::join!(
        call(
            app(&rt),
            "POST",
            &path,
            json!({"action":"wait","reason":"等待"}),
            Some("one"),
            Some(1)
        ),
        call(
            app(&rt),
            "POST",
            &path,
            json!({"action":"abandon","reason":"放弃"}),
            Some("two"),
            Some(1)
        )
    );
    assert!(
        (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
            || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT)
    );
    let event = Uuid::parse_str(goal["event_id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(event)
        .execute(&pool)
        .await
        .unwrap();
    let (s, _, _) = call(
        app(&rt),
        "POST",
        "/api/v1/goals",
        goal_draft(),
        Some("g"),
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "replay must not expose blocked goal"
    );
    let (s, _, _) = call(
        app(&rt),
        "POST",
        &path,
        json!({"action":"abandon","reason":"继续尝试"}),
        Some("blocked"),
        Some(2),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, _, list) = call(app(&rt), "GET", "/api/v1/goals", Value::Null, None, None).await;
    assert!(list["items"].as_array().unwrap().is_empty());
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn owner_goal_transaction_failure_rolls_back_origin_and_receipt(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let before = version(&rt).await;
    sqlx::raw_sql("CREATE FUNCTION fail_goal_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='goal.changed' THEN RAISE EXCEPTION 'test'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_goal_outbox BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION fail_goal_outbox();").execute(&pool).await.unwrap();
    let (s, _, _) = call(
        app(&rt),
        "POST",
        "/api/v1/goals",
        goal_draft(),
        Some("retryable"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(version(&rt).await, before);
    assert_eq!(count(&pool, "SELECT count(*) FROM goals").await, 0);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM events WHERE kind='user.goal_requested'"
        )
        .await,
        0
    );
    sqlx::query("DROP TRIGGER fail_goal_outbox ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    owner_goal(&rt, "retryable").await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn owner_goal_change_fences_running_work_without_erasing_its_accounting(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let g = owner_goal(&rt, "g").await;
    let path = format!(
        "/api/v1/goals/{}/transitions",
        g["goal_id"].as_str().unwrap()
    );
    rt.store.tick_goals().await.unwrap().unwrap();
    let lease = rt.store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    // A running request is not declared physically cancelled by a UI transition.
    let (s, _, _) = call(
        app(&rt),
        "POST",
        &path,
        json!({"action":"wait","reason":"停止旧方向"}),
        Some("wait-live"),
        Some(1),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(rt.store.cycle_progress(&lease).await.is_err());
    let (s, _, _) = call(
        app(&rt),
        "POST",
        &path,
        json!({"action":"activate","reason":"重新考虑下一步"}),
        Some("activate-live"),
        Some(2),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(rt.store.claim_run(Uuid::new_v4()).await.unwrap().is_none());
    let old: String = sqlx::query_scalar("SELECT status FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old, "stale");
    let outcome: String = sqlx::query_scalar("SELECT outcome FROM goal_cycles WHERE run_id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(outcome, "superseded_by_owner");
    let next = rt.store.tick_goals().await.unwrap().unwrap();
    assert_ne!(next, lease.run_id);
    let n: i32 = sqlx::query_scalar("SELECT no_progress_cycles FROM goals LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    // Budget settlement itself is covered by the scheduling integration cases.
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn memory_person_projection_is_current_sourced_and_privacy_filtered(pool: PgPool) {
    use alia::domain::entities::{MemoryPersonAssertion, PersonRole, SourceSpan};
    let rt = runtime(&pool);
    init(&rt).await;
    let source = rt
        .store
        .submit_message("person-source", "周岚喜欢乌龙茶。", None)
        .await
        .unwrap()
        .event_id;
    let state = rt.store.snapshot().await.unwrap().unwrap();
    let memory = rt
        .store
        .record_memories(
            "person-memory",
            state.expected(),
            &[source],
            &[NewMemory {
                content: "周岚喜欢乌龙茶。".into(),
                scope: "general".into(),
                kind: MemoryKind::Semantic,
                source_event_ids: vec![source],
                supersedes: None,
            }],
        )
        .await
        .unwrap()
        .memory_ids[0];
    let proof = SourceSpan {
        event_id: source,
        start_char: 0,
        end_char: 2,
    };
    let person = rt
        .store
        .register_person(
            "person",
            rt.store.snapshot().await.unwrap().unwrap().expected(),
            "person-key",
            "周岚",
            proof.clone(),
        )
        .await
        .unwrap()
        .entity_id;
    let receipt = rt
        .store
        .assert_memory_person(
            "person-link",
            rt.store.snapshot().await.unwrap().unwrap().expected(),
            MemoryPersonAssertion {
                memory_id: memory,
                memory_version: 1,
                entity_id: person,
                role: PersonRole::Subject,
                source: proof,
                memory_start_char: 0,
            },
        )
        .await
        .unwrap();
    let (status, headers, body) =
        call(app(&rt), "GET", "/api/v1/memories", Value::Null, None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["cache-control"], "no-store");
    let item = &body["items"][0];
    assert_eq!(item["id"], memory.to_string());
    assert_eq!(item["person_assertions_more"], false);
    let assertions = item["person_assertions"].as_array().unwrap();
    assert_eq!(assertions.len(), 1);
    assert_eq!(assertions[0]["entity_id"], person.to_string());
    assert_eq!(assertions[0]["label"], "周岚");
    assert_eq!(assertions[0]["role"], "subject");
    assert_eq!(assertions[0]["source_kind"], "reported_claim");
    assert_eq!(assertions[0]["actor_id"], "owner");
    assert_eq!(
        assertions[0]["assertion_event_id"],
        receipt.event_id.to_string()
    );
    assert_eq!(assertions[0]["source_event_id"], source.to_string());
    assert!(!body.to_string().contains("person-key"));
    // Metadata must disappear even when broken data bypasses normal version updates.
    sqlx::query("UPDATE memories SET content='周岚的正文被故障修改' WHERE id=$1")
        .bind(memory)
        .execute(&pool)
        .await
        .unwrap();
    let (_, _, changed) = call(app(&rt), "GET", "/api/v1/memories", Value::Null, None, None).await;
    assert_eq!(changed["items"][0]["person_assertions"], json!([]));
    sqlx::query("UPDATE memories SET content='周岚喜欢乌龙茶。' WHERE id=$1")
        .bind(memory)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(person)
        .execute(&pool)
        .await
        .unwrap();
    let (_, _, hidden) = call(app(&rt), "GET", "/api/v1/memories", Value::Null, None, None).await;
    assert_eq!(hidden["items"], json!([]));
    assert!(!hidden.to_string().contains("周岚"));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn console_management_redacts_keys_endpoints_and_custom_model_names(pool: PgPool) {
    use alia::adapters::model::{ChatDialect, ChatProvider};
    use alia::runtime::model::ModelProvider;
    let mut rt = runtime(&pool);
    init(&rt).await;
    let provider = ChatProvider::new(
        "https://private-host.example.test/v1",
        "private-model-marker",
        "sentinel-api-secret-never-expose",
        ChatDialect::DeepSeek,
    )
    .unwrap();
    rt.model_connection = provider.public_connection_info();
    rt.model_status = provider.status();
    let (status, headers, body) = call(
        app(&rt),
        "GET",
        "/api/v1/management",
        Value::Null,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(body["model"]["credential_configured"], true);
    assert_eq!(body["model"]["model"], "custom_deployment_model");
    assert_eq!(body["fallback"]["supported"], false);
    assert_eq!(
        body["extensions"]["capabilities"].as_array().unwrap().len(),
        4
    );
    for private in [
        "sentinel-api-secret-never-expose",
        "private-host",
        "private-model-marker",
        "authorization",
    ] {
        assert!(!body.to_string().contains(private));
    }
    for path in [
        "/api/v1/management",
        "/api/v1/diagnostics",
        "/api/v1/runs",
        "/api/v1/audit",
        "/api/v1/persona",
    ] {
        assert_eq!(
            raw(
                app(&rt),
                "GET",
                path,
                "null",
                &[("authorization", "Bearer wrong")]
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn sandbox_is_strict_and_read_only_even_for_hostile_configuration(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let before = rt.store.public_state().await.unwrap();
    let events = count(&pool, "SELECT count(*) FROM events").await;
    let cases = [
        ("persona", initialization()["persona"].to_string(), true),
        ("runtime", initialization()["runtime"].to_string(), true),
        (
            "persona",
            "{\"schema_version\":1,\"schema_version\":1}".into(),
            false,
        ),
        (
            "decision_schema",
            "{\"shell\":\"do not execute\",\"api_key\":\"private-input-marker\"}".into(),
            false,
        ),
    ];
    for (kind, text, valid) in cases {
        let (s, _, v) = call(
            app(&rt),
            "POST",
            "/api/v1/sandbox/validate",
            json!({"kind":kind,"document_text":text}),
            None,
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["valid"], valid);
        assert_eq!(v["execution"], "not_executed");
        assert_eq!(v["state_modified"], false);
        assert!(!v.to_string().contains("private-input-marker"));
    }
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/sandbox/validate",
            json!({"kind":"shell","document_text":"{}"}),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/sandbox/validate",
            json!({"kind":"persona","document_text":"{}","execute":true}),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(
            app(&rt),
            "POST",
            "/api/v1/sandbox/validate",
            json!({"kind":"persona","document_text":"x".repeat(65537)}),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(rt.store.public_state().await.unwrap(), before);
    assert_eq!(count(&pool, "SELECT count(*) FROM events").await, events);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM usage_reservations").await,
        0
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM actions").await, 0);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn trace_spans_have_real_timing_without_raw_payload_and_obey_source_revocation(pool: PgPool) {
    let rt = runtime(&pool);
    init(&rt).await;
    let message = rt
        .store
        .submit_message("trace-source", "private-trigger-marker", None)
        .await
        .unwrap();
    let fixture=FixtureProvider{response:json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[{"capability_id":"notes.write","input":{"title":"synthetic trace","text":"private-note-body-marker"}}],"development":null,"brief_reason":"explicit fixture; no cloud call"}).to_string()};
    Worker::new(
        rt.store.clone(),
        Arc::new(fixture),
        Arc::new(HttpWebReader),
        rt.wake.clone(),
    )
    .run_once()
    .await
    .unwrap();
    let path = format!("/api/v1/runs/{}", message.operation_id);
    let (status, _, body) = call(app(&rt), "GET", &path, Value::Null, None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["payload_policy"], "metadata_only");
    let spans = body["spans"].as_array().unwrap();
    assert!(!spans.is_empty());
    let times: Vec<_> = spans
        .iter()
        .map(|span| {
            chrono::DateTime::parse_from_rfc3339(
                span["started_at"]
                    .as_str()
                    .or_else(|| span["created_at"].as_str())
                    .unwrap(),
            )
            .unwrap()
        })
        .collect();
    assert!(
        times.windows(2).all(|pair| pair[0] <= pair[1]),
        "model and tool spans must be interleaved by actual observations"
    );
    let tools = spans
        .iter()
        .filter(|s| s["kind"] == "tool")
        .collect::<Vec<_>>();
    assert!(!tools.is_empty());
    for tool in tools {
        assert_eq!(tool["status"], "succeeded");
        assert!(tool["started_at"].is_string());
        assert!(tool["finished_at"].is_string());
        assert!(tool["duration_ms"].as_f64().unwrap() >= 0.0);
        assert_eq!(tool["parent_id"], message.operation_id.to_string());
    }
    assert!(!body.to_string().contains("private-note-body-marker"));
    assert!(!body.to_string().contains("private-trigger-marker"));
    let (_, _, audit) = call(
        app(&rt),
        "GET",
        "/api/v1/audit?limit=100",
        Value::Null,
        None,
        None,
    )
    .await;
    assert!(!audit.to_string().contains("private-note-body-marker"));
    assert!(
        audit["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event.get("payload").is_none())
    );
    sqlx::query("UPDATE runs SET error_code='private-error-marker' WHERE id=$1")
        .bind(message.operation_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, _, masked) = call(app(&rt), "GET", "/api/v1/runs", Value::Null, None, None).await;
    assert!(!masked.to_string().contains("private-error-marker"));
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(message.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(app(&rt), "GET", &path, Value::Null, None, None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(app(&rt), "GET", "/api/v1/runs", Value::Null, None, None)
            .await
            .2["items"],
        json!([])
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn console_pagination_diagnostics_and_persona_birth_comparison_are_actual_state(
    pool: PgPool,
) {
    let rt = runtime(&pool);
    init(&rt).await;
    for n in 0..3 {
        rt.store
            .submit_message(&format!("page-{n}"), "synthetic page", None)
            .await
            .unwrap();
    }
    let (_, _, first) = call(
        app(&rt),
        "GET",
        "/api/v1/runs?limit=1",
        Value::Null,
        None,
        None,
    )
    .await;
    let cursor = first["next_cursor"].as_str().unwrap();
    let (_, _, next) = call(
        app(&rt),
        "GET",
        &format!("/api/v1/runs?limit=1&cursor={cursor}"),
        Value::Null,
        None,
        None,
    )
    .await;
    assert_ne!(first["items"][0]["id"], next["items"][0]["id"]);
    for path in [
        "/api/v1/runs?status=made_up",
        "/api/v1/runs?limit=0",
        "/api/v1/runs?cursor=v1:audit:5",
        "/api/v1/audit?cursor=v1:audit:NaN",
    ] {
        assert_eq!(
            call(app(&rt), "GET", path, Value::Null, None, None).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let (_, _, diagnostics) = call(
        app(&rt),
        "GET",
        "/api/v1/diagnostics",
        Value::Null,
        None,
        None,
    )
    .await;
    assert_eq!(diagnostics["database"], "reachable");
    assert_eq!(diagnostics["schema_version"], 29);
    assert_eq!(diagnostics["runs"]["queued"], 3);
    assert_eq!(diagnostics["raw_logs_available"], false);
    let (_, _, birth) = call(app(&rt), "GET", "/api/v1/persona", Value::Null, None, None).await;
    assert_eq!(birth["initial"], birth["current"]);
    assert_eq!(birth["rollback_supported"], false);
    // Fault-injected current configuration must not overwrite the actual birth record.
    sqlx::query("UPDATE individuals SET persona=jsonb_set(persona,'{expression,tone}','\"test changed tone\"')").execute(&pool).await.unwrap();
    let (status, _, changed) =
        call(app(&rt), "GET", "/api/v1/persona", Value::Null, None, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(changed["error"]["code"], "invalid_request");
    assert!(changed.get("current").is_none());
    let persisted = rt.store.snapshot().await.unwrap().unwrap();
    assert_eq!(persisted.seed, birth["initial"]);
    assert_ne!(persisted.persona, birth["current"]);
}
