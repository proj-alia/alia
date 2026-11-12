//! P05: real TCP/HTTP and disposable PostgreSQL; no workers or model calls.
use alia::{runtime::Runtime, service};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::path::PathBuf;
use uuid::Uuid;

const TOKEN: &str = "p05-isolated-relationship-test-token-not-production";
struct Server {
    base: String,
    client: Client,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn get(&self, path: &str, query: &[(&str, &str)]) -> (StatusCode, Value) {
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .bearer_auth(TOKEN)
            .query(query)
            .send()
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        let status = response.status();
        (status, response.json().await.unwrap())
    }
}
async fn setup(pool: &PgPool, initialize: bool) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = service::router(
        Runtime::new(pool.clone(), TOKEN).unwrap(),
        PathBuf::from("web/dist"),
    );
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let server = Server {
        base,
        client: Client::new(),
        task,
    };
    if initialize {
        let response = server.client.post(format!("{}/api/v1/initialize",server.base)).bearer_auth(TOKEN)
            .header("Idempotency-Key","p05-fixture-init").json(&json!({
                "persona":serde_json::from_str::<Value>(include_str!("../config/persona.example.json")).unwrap(),
                "runtime":serde_json::from_str::<Value>(include_str!("../config/runtime.example.json")).unwrap()
            })).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    server
}

async fn owner(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM individuals WHERE singleton")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn subject(pool: &PgPool, key: &str) -> Uuid {
    let id = Uuid::new_v4();
    let individual = owner(pool).await;
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'relationship')")
        .bind(individual)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO relationships(individual_id,subject_id,subject_key,stage,version,expectations,boundaries,commitments,unresolved) VALUES($1,$2,$3,'cooperation',7,'[\"保存的期待\"]','[\"保存的边界\"]','[\"保存的承诺\"]','[\"未解决事项\"]')").bind(individual).bind(id).bind(key).execute(pool).await.unwrap();
    id
}
async fn event(pool: &PgPool, subject: Option<Uuid>, kind: &str, text: Value) -> Uuid {
    let id = Uuid::new_v4();
    let individual = owner(pool).await;
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'event')")
        .bind(individual)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) VALUES($1,$2,$3,$4,'synthetic-actor','2026-01-01T00:00:00Z',jsonb_build_object('text',$5::jsonb,'provider_secret','PRIVATE-PROVIDER','review','PRIVATE-REVIEW'),$2)").bind(individual).bind(id).bind(kind).bind(if kind=="message.received"{"reported_claim"}else{"generated"}).bind(text).execute(pool).await.unwrap();
    if let Some(subject) = subject {
        sqlx::query(
            "INSERT INTO relationship_events(individual_id,subject_id,event_id) VALUES($1,$2,$3)",
        )
        .bind(individual)
        .bind(subject)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
    id
}
async fn blocked(pool: &PgPool, id: Uuid, value: bool) {
    sqlx::query("UPDATE objects SET blocked=$2 WHERE id=$1")
        .bind(id)
        .bind(value)
        .execute(pool)
        .await
        .unwrap();
}
async fn counts(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('individuals',(SELECT md5(jsonb_agg(to_jsonb(i) ORDER BY id)::text) FROM individuals i),'relationships',(SELECT md5(jsonb_agg(to_jsonb(r) ORDER BY subject_id)::text) FROM relationships r),'links',(SELECT md5(jsonb_agg(to_jsonb(r) ORDER BY subject_id,event_id)::text) FROM relationship_events r),'events',(SELECT md5(jsonb_agg(to_jsonb(e) ORDER BY sequence)::text) FROM events e),'objects',(SELECT md5(jsonb_agg(to_jsonb(o) ORDER BY id)::text) FROM objects o),'runs',(SELECT count(*) FROM runs),'actions',(SELECT count(*) FROM actions),'usage',(SELECT count(*) FROM usage_reservations),'outbox',(SELECT count(*) FROM outbox))").fetch_one(pool).await.unwrap()
}
fn items(v: &Value) -> &[Value] {
    v["items"].as_array().unwrap()
}
#[sqlx::test]
#[ignore = "P05 scoped actual HTTP/PG; run under project heavy lock"]
async fn relationships_directory_paging_versions_and_stored_detail(pool: PgPool) {
    let server = setup(&pool, true).await;
    let mut ids: Vec<Uuid> = vec![
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    ];
    for n in 0..37 {
        ids.push(subject(&pool, &format!("主体 {n}")).await);
    }
    ids.sort();
    let before = counts(&pool).await;
    let (s, first) = server.get("/api/v1/relationships", &[]).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(items(&first).len(), 30);
    assert_eq!(items(&first)[0]["subject_id"], ids[0].to_string());
    assert_eq!(items(&first)[0].as_object().unwrap().len(), 4);
    assert_eq!(first["individual_id"], owner(&pool).await.to_string());
    assert!(first["state_version"].is_number());
    assert!(first["privacy_epoch"].is_number());
    assert!(first["observed_at"].is_string());
    let cursor = first["next_cursor"].as_str().unwrap();
    let (_, second) = server
        .get("/api/v1/relationships", &[("cursor", cursor)])
        .await;
    assert_eq!(items(&second).len(), 8);
    assert!(second["next_cursor"].is_null());
    let all: Vec<_> = items(&first)
        .iter()
        .chain(items(&second))
        .map(|r| r["subject_id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .collect();
    assert_eq!(all, ids);
    let target = sqlx::query_scalar::<_, Uuid>(
        "SELECT subject_id FROM relationships WHERE subject_key='主体 0'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let (_, detail) = server
        .get(&format!("/api/v1/relationships/{target}"), &[])
        .await;
    assert_eq!(
        detail["relationship"]["expectations"],
        json!(["保存的期待"])
    );
    assert_eq!(detail["relationship"]["version"], 7);
    assert_ne!(
        detail["individual_id"],
        detail["relationship"]["subject_id"]
    );
    assert_eq!(detail["relationship"]["subject_key"], "主体 0");
    assert_eq!(counts(&pool).await, before);
    let anchor: Uuid = items(&first).last().unwrap()["subject_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    blocked(&pool, anchor, true).await;
    assert_eq!(
        server
            .get("/api/v1/relationships", &[("cursor", cursor)])
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
#[sqlx::test]
#[ignore = "P05 scoped actual HTTP/PG; run under project heavy lock"]
async fn relationship_timeline_linked_only_sequence_paging_and_concurrent_append(pool: PgPool) {
    let server = setup(&pool, true).await;
    let a = subject(&pool, "独立A").await;
    let b = subject(&pool, "独立B").await;
    let mut ids = vec![];
    for n in 0..37 {
        ids.push(
            event(
                &pool,
                Some(a),
                if n % 2 == 0 {
                    "message.received"
                } else {
                    "message.sent"
                },
                json!(format!("A {n}🙂")),
            )
            .await,
        );
    }
    event(&pool, Some(b), "message.received", json!("B-PRIVATE")).await;
    event(&pool, None, "message.sent", json!("UNLINKED-PRIVATE")).await;
    event(&pool, Some(a), "message.drafted", json!("DRAFT-PRIVATE")).await;
    event(
        &pool,
        Some(a),
        "runtime.internal",
        json!("INTERNAL-PRIVATE"),
    )
    .await;
    event(
        &pool,
        Some(a),
        "message.sent",
        json!({"secret":"OBJECT-PRIVATE"}),
    )
    .await;
    let path = format!("/api/v1/relationships/{a}/events");
    let before = counts(&pool).await;
    let (s, first) = server.get(&path, &[]).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(items(&first).len(), 30);
    assert_eq!(items(&first)[0]["id"], ids[36].to_string());
    assert_eq!(items(&first)[0].as_object().unwrap().len(), 7);
    assert_eq!(items(&first)[0]["source_kind"], "reported_claim");
    assert_eq!(items(&first)[1]["source_kind"], "generated");
    for private in [
        "B-PRIVATE",
        "UNLINKED",
        "DRAFT",
        "INTERNAL",
        "OBJECT",
        "PRIVATE-PROVIDER",
        "PRIVATE-REVIEW",
    ] {
        assert!(!first.to_string().contains(private));
    }
    let cursor = first["next_cursor"].as_str().unwrap();
    let (_, second) = server.get(&path, &[("cursor", cursor)]).await;
    assert_eq!(items(&second).len(), 7);
    assert!(second["next_cursor"].is_null());
    let all: Vec<_> = items(&first)
        .iter()
        .chain(items(&second))
        .map(|r| r["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .collect();
    ids.reverse();
    assert_eq!(all, ids);
    assert_eq!(counts(&pool).await, before);
    let newer = event(&pool, Some(a), "message.sent", json!("appended")).await;
    assert_eq!(
        items(&server.get(&path, &[]).await.1)[0]["id"],
        newer.to_string()
    );
    assert_eq!(
        server.get(&path, &[("cursor", cursor)]).await.1["items"],
        second["items"]
    );
    assert_eq!(
        server
            .get(
                &format!("/api/v1/relationships/{b}/events"),
                &[("cursor", cursor)]
            )
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let anchor: Uuid = items(&first).last().unwrap()["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    blocked(&pool, anchor, true).await;
    assert_eq!(
        server.get(&path, &[("cursor", cursor)]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(items(&server.get(&path, &[]).await.1).len(), 30);
}
#[sqlx::test]
#[ignore = "P05 scoped actual HTTP/PG; run under project heavy lock"]
async fn relationship_visibility_subject_sources_identity_and_independent_rows(pool: PgPool) {
    let server = setup(&pool, true).await;
    let a = subject(&pool, "撤销目标").await;
    let b = subject(&pool, "独立可见").await;
    let source = event(&pool, None, "runtime.fixture", json!("parent")).await;
    let middle = event(&pool, None, "runtime.fixture", json!("middle")).await;
    let msg = event(&pool, Some(a), "message.received", json!("revocable")).await;
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES($1,$2,$3),($1,$3,$4)").bind(owner(&pool).await).bind(source).bind(middle).bind(msg).execute(&pool).await.unwrap();
    blocked(&pool, source, true).await;
    let (_, v) = server
        .get(&format!("/api/v1/relationships/{a}/events"), &[])
        .await;
    assert!(items(&v).is_empty());
    assert!(!v.to_string().contains(&msg.to_string()));
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES($1,$2,$3),($1,$3,$2)").bind(owner(&pool).await).bind(middle).bind(a).execute(&pool).await.unwrap();
    for path in [
        format!("/api/v1/relationships/{a}"),
        format!("/api/v1/relationships/{a}/events"),
    ] {
        assert_eq!(server.get(&path, &[]).await.0, StatusCode::NOT_FOUND);
    }
    let (_, v) = server.get("/api/v1/relationships", &[]).await;
    assert!(items(&v).iter().any(|r| r["subject_id"] == b.to_string()));
    assert!(!v.to_string().contains(&a.to_string()));
    assert!(!v.to_string().contains("撤销目标"));
    blocked(&pool, source, false).await;
    assert_eq!(
        server
            .get(&format!("/api/v1/relationships/{a}"), &[])
            .await
            .0,
        StatusCode::OK
    );
    blocked(&pool, a, true).await;
    assert_eq!(
        server
            .get(&format!("/api/v1/relationships/{a}"), &[])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    blocked(&pool, owner(&pool).await, true).await;
    assert_eq!(
        server.get("/api/v1/relationships", &[]).await.0,
        StatusCode::NOT_FOUND
    );
}
#[sqlx::test]
#[ignore = "P05 scoped actual HTTP/PG; run under project heavy lock"]
async fn relationships_actual_owner_message_links_and_no_fabricated_history(pool: PgPool) {
    let server = setup(&pool, true).await;
    let subject: Uuid =
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let response = server
        .client
        .post(format!("{}/api/v1/messages", server.base))
        .bearer_auth(TOKEN)
        .header("Idempotency-Key", Uuid::new_v4().to_string())
        .json(&json!({"text":"真实HTTP收到的表达","conversation_id":Uuid::new_v4()}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let receipt: Value = response.json().await.unwrap();
    let before = counts(&pool).await;
    let (_, v) = server
        .get(&format!("/api/v1/relationships/{subject}/events"), &[])
        .await;
    assert_eq!(items(&v).len(), 1);
    assert_eq!(items(&v)[0]["text"], "真实HTTP收到的表达");
    assert_eq!(items(&v)[0]["source_kind"], "reported_claim");
    assert_eq!(items(&v)[0]["id"], receipt["event_id"]);
    assert_eq!(counts(&pool).await, before);
    assert_eq!(
        server
            .get(
                &format!("/api/v1/relationships/{}", owner(&pool).await),
                &[]
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let invalid = sqlx::query(
        "INSERT INTO relationship_events(individual_id,subject_id,event_id) VALUES($1,$2,$3)",
    )
    .bind(Uuid::new_v4())
    .bind(subject)
    .bind(
        receipt["event_id"]
            .as_str()
            .unwrap()
            .parse::<Uuid>()
            .unwrap(),
    )
    .execute(&pool)
    .await;
    assert!(invalid.is_err());
    assert_eq!(counts(&pool).await, before);
}
#[sqlx::test]
#[ignore = "P05 scoped actual HTTP/PG; run under project heavy lock"]
async fn relationships_strict_queries_unknown_targets_and_cursor_tampering(pool: PgPool) {
    let server = setup(&pool, true).await;
    let a = subject(&pool, "查询目标").await;
    for _ in 0..3 {
        event(&pool, Some(a), "message.received", json!("test")).await;
    }
    for path in [
        "/api/v1/relationships".to_string(),
        format!("/api/v1/relationships/{a}/events"),
    ] {
        for query in [
            vec![("limit", "0")],
            vec![("limit", "101")],
            vec![("limit", "-1")],
            vec![("limit", "1"), ("limit", "1")],
            vec![("cursor", "x"), ("cursor", "x")],
            vec![("unknown", "x")],
            vec![("cursor", "")],
            vec![("cursor", "v1:notes:bad:1:x")],
        ] {
            assert_eq!(
                server.get(&path, &query).await.0,
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }
    }
    assert_eq!(
        server
            .get(&format!("/api/v1/relationships/{a}"), &[("cursor", "x")])
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        server.get("/api/v1/relationships/not-uuid", &[]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    for path in [
        format!("/api/v1/relationships/{}", Uuid::new_v4()),
        format!("/api/v1/relationships/{}/events", Uuid::new_v4()),
    ] {
        assert_eq!(server.get(&path, &[]).await.0, StatusCode::NOT_FOUND);
    }
    let path = format!("/api/v1/relationships/{a}/events");
    let (_, v) = server.get(&path, &[("limit", "1")]).await;
    let cursor = v["next_cursor"].as_str().unwrap();
    let mut parts: Vec<_> = cursor.split(':').map(String::from).collect();
    parts[4] = "1".into();
    assert_eq!(
        server.get(&path, &[("cursor", &parts.join(":"))]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        server
            .get("/api/v1/relationships", &[("cursor", cursor)])
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
#[sqlx::test]
#[ignore = "P05 scoped actual HTTP/PG; run under project heavy lock"]
async fn relationships_auth_initialization_empty_and_foreign_origin(pool: PgPool) {
    let server = setup(&pool, false).await;
    assert_eq!(
        server.get("/api/v1/relationships", &[]).await.0,
        StatusCode::CONFLICT
    );
    let bad = server
        .client
        .get(format!("{}/api/v1/relationships", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    let bad = server
        .client
        .get(format!("{}/api/v1/relationships", server.base))
        .bearer_auth("wrong-token")
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    let cross = server
        .client
        .get(format!("{}/api/v1/relationships", server.base))
        .bearer_auth(TOKEN)
        .header("Origin", "https://attacker.invalid")
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), StatusCode::FORBIDDEN);
    let _initialized = setup(&pool, true).await;
    sqlx::query("DELETE FROM relationships")
        .execute(&pool)
        .await
        .unwrap();
    let before = counts(&pool).await;
    let (_, v) = server.get("/api/v1/relationships", &[]).await;
    assert!(items(&v).is_empty());
    assert!(v["next_cursor"].is_null());
    assert_eq!(counts(&pool).await, before);
}
