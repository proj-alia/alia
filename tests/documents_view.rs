//! P05: real TCP/HTTP and disposable PostgreSQL; no workers or model calls.
use alia::{runtime::Runtime, service};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::path::PathBuf;
use uuid::Uuid;

const TOKEN: &str = "p05-isolated-document-test-token-not-production";
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
    async fn import(&self, title: &str, text: &str, source: &str) -> Uuid {
        let response = self
            .client
            .post(format!("{}/api/v1/documents", self.base))
            .bearer_auth(TOKEN)
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .json(&json!({"title":title,"text":text,"source_label":source}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.unwrap();
        body["event_id"].as_str().unwrap().parse().unwrap()
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
async fn counts(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('identity_digest',(SELECT md5(to_jsonb(i)::text) FROM individuals i),'events_digest',(SELECT md5(coalesce(string_agg(to_jsonb(e)::text,'' ORDER BY e.sequence),'')) FROM events e),'documents_digest',(SELECT md5(coalesce(string_agg(to_jsonb(d)::text,'' ORDER BY d.event_id),'')) FROM documents d),'objects',(SELECT count(*) FROM objects),'runs',(SELECT count(*) FROM runs),'actions',(SELECT count(*) FROM actions),'usage',(SELECT count(*) FROM usage_reservations),'outbox',(SELECT count(*) FROM outbox))")
        .fetch_one(pool).await.unwrap()
}
fn items(value: &Value) -> &[Value] {
    value["items"].as_array().unwrap()
}

#[sqlx::test]
#[ignore = "P05 isolated real PostgreSQL/HTTP; execute under shared heavy lock"]
async fn document_directory_full_paging_literal_search_and_bound_cursor(pool: PgPool) {
    let server = setup(&pool, true).await;
    let mut ids = vec![];
    for n in 0..37 {
        let title = if n == 0 {
            "周青%_İΩ".to_string()
        } else {
            format!("资料 {n:02}")
        };
        let id = server
            .import(&title, "body-not-in-directory", "来源 X ")
            .await;
        sqlx::query("UPDATE events SET recorded_at='2026-01-01T00:00:00Z'::timestamptz+$2*interval '1 second' WHERE id=$1")
            .bind(id).bind(n).execute(&pool).await.unwrap();
        ids.push(id);
    }
    let before = counts(&pool).await;
    let (status, first) = server.get("/api/v1/documents", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(items(&first).len(), 30);
    assert_eq!(items(&first)[0]["id"], ids[36].to_string());
    assert!(!first.to_string().contains("body-not-in-directory"));
    assert_eq!(items(&first)[0].as_object().unwrap().len(), 5);
    let cursor = first["next_cursor"].as_str().unwrap();
    let (status, second) = server.get("/api/v1/documents", &[("cursor", cursor)]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(items(&second).len(), 7);
    assert_eq!(items(&second)[6]["id"], ids[0].to_string());
    assert!(second["next_cursor"].is_null());
    for q in ["%_", "İ", "来源 X "] {
        let (status, result) = server.get("/api/v1/documents", &[("q", q)]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(items(&result).len(), if q == "来源 X " { 30 } else { 1 });
    }
    assert_eq!(
        server
            .get("/api/v1/documents", &[("cursor", cursor), ("q", "资料")])
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        server
            .get("/api/v1/documents", &[("cursor", cursor), ("q", " ")])
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        items(
            &server
                .get("/api/v1/documents", &[("q", "body-not-in-directory")])
                .await
                .1
        )
        .len(),
        0
    );
    assert_eq!(counts(&pool).await, before);
    sqlx::query("UPDATE events SET recorded_at='2026-01-01T00:00:00Z' WHERE id=ANY($1)")
        .bind(&ids)
        .execute(&pool)
        .await
        .unwrap();
    let tied_before = counts(&pool).await;
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.reverse();
    let (_, tied) = server.get("/api/v1/documents", &[("limit", "1")]).await;
    assert_eq!(items(&tied)[0]["id"], sorted[0].to_string());
    let (_, tie_next) = server
        .get(
            "/api/v1/documents",
            &[
                ("limit", "1"),
                ("cursor", tied["next_cursor"].as_str().unwrap()),
            ],
        )
        .await;
    assert_eq!(items(&tie_next)[0]["id"], sorted[1].to_string());
    assert_eq!(counts(&pool).await, tied_before);
}

#[sqlx::test]
#[ignore = "P05 isolated real PostgreSQL/HTTP; execute under shared heavy lock"]
async fn document_unicode_slices_eof_and_large_import_preserve_read_only_state(pool: PgPool) {
    let server = setup(&pool, true).await;
    let id = server
        .import("字符资料", "甲🙂e\u{301}乙", "明确来源")
        .await;
    let large = server
        .import(
            "长资料",
            &"文".repeat(60000),
            "大于普通128KiB正文限制的原导入",
        )
        .await;
    let before = counts(&pool).await;
    let path = format!("/api/v1/documents/{id}");
    let (status, body) = server.get(&path, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_chars"], 5);
    assert_eq!(body["max_chars"], 2000);
    assert_eq!(body["document_id"], id.to_string());
    assert_eq!(body["event_id"], id.to_string());
    assert_eq!(body["source_kind"], "external_document");
    assert!(body["next_offset"].is_null());
    let (_, emoji) = server
        .get(&path, &[("offset", "1"), ("max_chars", "1")])
        .await;
    assert_eq!(emoji["text"], "🙂");
    assert_eq!(emoji["next_offset"], 2);
    let (_, combining) = server
        .get(&path, &[("offset", "2"), ("max_chars", "2")])
        .await;
    assert_eq!(combining["text"], "e\u{301}");
    assert_eq!(combining["next_offset"], 4);
    let (_, end) = server.get(&path, &[("offset", "5")]).await;
    assert_eq!(end["text"], "");
    assert!(end["next_offset"].is_null());
    assert_eq!(
        server.get(&path, &[("offset", "6")]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (_, long) = server
        .get(
            &format!("/api/v1/documents/{large}"),
            &[("max_chars", "4096")],
        )
        .await;
    assert_eq!(long["text"].as_str().unwrap().chars().count(), 4096);
    assert_eq!(long["total_chars"], 60000);
    assert_eq!(long["next_offset"], 4096);
    assert_eq!(counts(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P05 isolated real PostgreSQL/HTTP; execute under shared heavy lock"]
async fn document_two_generation_revocation_removes_directory_search_and_cached_slice(
    pool: PgPool,
) {
    let server = setup(&pool, true).await;
    let source = server.import("源资料", "祖先", "来源").await;
    let middle = server.import("中间资料", "中间", "来源").await;
    let target = server
        .import("唯一隐私资料", "private-target", "来源")
        .await;
    let safe = server.import("独立资料", "safe-text", "来源").await;
    let individual: Uuid = sqlx::query_scalar("SELECT id FROM individuals")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES($1,$2,$3),($1,$3,$4)")
        .bind(individual).bind(source).bind(middle).bind(target).execute(&pool).await.unwrap();
    let path = format!("/api/v1/documents/{target}");
    assert_eq!(server.get(&path, &[]).await.0, StatusCode::OK);
    sqlx::query("UPDATE objects SET blocked=true WHERE individual_id=$1 AND id=$2")
        .bind(individual)
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    let before = counts(&pool).await;
    let (status, hidden) = server.get(&path, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!hidden.to_string().contains("private-target"));
    assert_eq!(
        server
            .get(&format!("/api/v1/documents/{middle}"), &[])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, all) = server.get("/api/v1/documents", &[]).await;
    assert_eq!(items(&all).len(), 1);
    assert_eq!(items(&all)[0]["id"], safe.to_string());
    assert_eq!(
        items(
            &server
                .get("/api/v1/documents", &[("q", "唯一隐私资料")])
                .await
                .1
        )
        .len(),
        0
    );
    assert_eq!(
        server
            .get(&format!("/api/v1/documents/{safe}"), &[])
            .await
            .1["text"],
        "safe-text"
    );
    assert_eq!(counts(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P05 isolated real PostgreSQL/HTTP; execute under shared heavy lock"]
async fn document_strict_queries_auth_errors_and_uninitialized_are_explicit(pool: PgPool) {
    let server = setup(&pool, false).await;
    assert_eq!(
        server.get("/api/v1/documents", &[]).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        server
            .get(&format!("/api/v1/documents/{}", Uuid::new_v4()), &[])
            .await
            .0,
        StatusCode::CONFLICT
    );
    let unauth = server
        .client
        .get(format!("{}/api/v1/documents", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
    let cross = server
        .client
        .get(format!("{}/api/v1/documents", server.base))
        .bearer_auth(TOKEN)
        .header("Origin", "https://untrusted.invalid")
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), StatusCode::FORBIDDEN);
    for query in [
        vec![("q", "a"), ("q", "b")],
        vec![("limit", "1"), ("limit", "2")],
        vec![("cursor", "x"), ("cursor", "y")],
        vec![("unknown", "x")],
        vec![("limit", "0")],
        vec![("limit", "101")],
        vec![("limit", "-1")],
        vec![("cursor", "v1:runs:0")],
        vec![("q", "\0")],
        vec![("q", "a\nb")],
    ] {
        assert_eq!(
            server.get("/api/v1/documents", &query).await.0,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{query:?}"
        );
    }
    assert_eq!(
        server
            .get("/api/v1/documents", &[("q", &"甲".repeat(201))])
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    for query in [
        vec![("offset", "-1")],
        vec![("offset", "1048577")],
        vec![("max_chars", "0")],
        vec![("max_chars", "4097")],
        vec![("offset", "0"), ("offset", "1")],
        vec![("max_chars", "1"), ("max_chars", "2")],
        vec![("path", "/etc/passwd")],
    ] {
        assert_eq!(
            server
                .get(&format!("/api/v1/documents/{}", Uuid::new_v4()), &query)
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{query:?}"
        );
    }
    assert_eq!(
        server.get("/api/v1/documents/not-a-uuid", &[]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
