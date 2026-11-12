//! Real TCP HTTP + isolated SQLx PostgreSQL. Synthetic data; no worker/model calls.
use alia::{runtime::Runtime, service};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::path::PathBuf;
use uuid::Uuid;
const TOKEN: &str = "isolated-console-query-token-no-production";
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
    async fn get(&self, path: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        let status = response.status();
        (status, response.json().await.unwrap())
    }
    async fn query(&self, name: &str, params: &[(&str, &str)]) -> (StatusCode, Value) {
        let response = self
            .client
            .get(format!("{}/api/v1/{name}", self.base))
            .query(params)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap();
        let status = response.status();
        (status, response.json().await.unwrap())
    }
}
async fn setup(pool: &PgPool) -> Server {
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
    let response=server.client.post(format!("{}/api/v1/initialize",server.base)).bearer_auth(TOKEN)
        .header("Idempotency-Key","query-fixture-birth").json(&json!({
            "persona":serde_json::from_str::<Value>(include_str!("../config/persona.example.json")).unwrap(),
            "runtime":serde_json::from_str::<Value>(include_str!("../config/runtime.example.json")).unwrap()
        })).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    server
}
async fn run(server: &Server, pool: &PgPool, index: i32, mode: &str, code: &str) -> Uuid {
    let response: Value = server
        .client
        .post(format!("{}/api/v1/messages", server.base))
        .bearer_auth(TOKEN)
        .header("Idempotency-Key", format!("query-message-{index}"))
        .json(&json!({"text":"private-body-marker endpoint secret-provider-text"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id: Uuid = serde_json::from_value(response["operation_id"].clone()).unwrap();
    sqlx::query("UPDATE runs SET status='failed',mode=$2,error_code=$3,created_at='2026-01-01T00:00:00Z'::timestamptz + $4*interval '1 second' WHERE id=$1")
        .bind(id).bind(mode).bind(code).bind(index).execute(pool).await.unwrap();
    sqlx::query("UPDATE events SET actor_id=$2,kind='test.query_%',occurred_at='2026-01-01T00:00:00Z'::timestamptz + $3*interval '1 second' WHERE id=(SELECT trigger_event_id FROM runs WHERE id=$1)")
        .bind(id).bind(format!("PublicActor{index}")).bind(index).execute(pool).await.unwrap();
    id
}
fn items(value: &Value) -> &[Value] {
    value["items"].as_array().unwrap()
}
#[sqlx::test]
#[ignore = "isolated real PostgreSQL and TCP HTTP; P02 locked runner"]
async fn runs_combined_filters_pages_time_edges_and_legacy(pool: PgPool) {
    let server = setup(&pool).await;
    let mut expected = vec![];
    for i in 0..8 {
        let id = run(
            &server,
            &pool,
            i,
            if i % 2 == 0 {
                "background"
            } else {
                "foreground"
            },
            "model_failure",
        )
        .await;
        if i == 2 || i == 4 {
            expected.push(id.to_string());
        }
    }
    expected.reverse();
    let filters = [
        ("q", "MODEL_FAILURE"),
        ("mode", "background"),
        ("status", "failed"),
        ("from", "2026-01-01T00:00:02Z"),
        ("to", "2026-01-01T00:00:06Z"),
        ("limit", "1"),
    ];
    let (status, first) = server.query("runs", &filters).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(items(&first)[0]["id"], expected[0]);
    let cursor = first["next_cursor"].as_str().unwrap();
    let mut next = filters.to_vec();
    next.push(("cursor", cursor));
    let (_, second) = server.query("runs", &next).await;
    assert_eq!(items(&second)[0]["id"], expected[1]);
    assert!(second["next_cursor"].is_null());
    next[0] = ("q", "model_unavailable");
    assert_eq!(
        server.query("runs", &next).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    next[0] = ("q", "model_failure");
    assert_eq!(server.query("runs", &next).await.0, StatusCode::OK);
    next[3] = ("from", "2026-01-01T00:00:02+00:00");
    assert_eq!(server.query("runs", &next).await.0, StatusCode::OK);
    let (_, old) = server.get("/api/v1/runs?limit=1").await;
    let old_cursor = old["next_cursor"].as_str().unwrap();
    assert!(old_cursor.starts_with("v1:runs:"));
    assert_eq!(
        server
            .query("runs", &[("cursor", old_cursor), ("limit", "1")])
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        server
            .query("runs", &[("cursor", old_cursor), ("q", " ")])
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        server
            .query("runs", &[("cursor", old_cursor), ("status", "failed")])
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(items(&server.query("runs", &[("q", " model_failure ")]).await.1).is_empty());
    let id = expected[0].clone();
    assert_eq!(items(&server.query("runs", &[("q", &id)]).await.1).len(), 1);
    // Identical timestamps still paginate deterministically by UUID.
    sqlx::query("UPDATE runs SET created_at='2026-01-01T00:00:00Z'")
        .execute(&pool)
        .await
        .unwrap();
    let mut seen = vec![];
    let mut cursor = None::<String>;
    loop {
        let mut params = vec![("q", "model_failure"), ("limit", "2")];
        if let Some(c) = cursor.as_deref() {
            params.push(("cursor", c));
        }
        let (_, v) = server.query("runs", &params).await;
        seen.extend(
            items(&v)
                .iter()
                .map(|v| v["id"].as_str().unwrap().to_owned()),
        );
        cursor = v["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 8);
    let mut sorted = seen.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    sorted.dedup();
    assert_eq!(seen, sorted);
}
#[sqlx::test]
#[ignore = "isolated real PostgreSQL and TCP HTTP; P02 locked runner"]
async fn audit_searches_all_pages_literal_metadata_and_time_endpoints(pool: PgPool) {
    let server = setup(&pool).await;
    for i in 0..6 {
        run(&server, &pool, i, "foreground", "model_failure").await;
    }
    let base = [
        ("q", "TEST.QUERY_%"),
        ("from", "2026-01-01T00:00:01Z"),
        ("to", "2026-01-01T00:00:05Z"),
        ("limit", "2"),
    ];
    let (_, first) = server.query("audit", &base).await;
    assert_eq!(items(&first).len(), 2);
    assert_eq!(items(&first)[0]["actor_id"], "PublicActor1");
    let mut next = base.to_vec();
    next.push(("cursor", first["next_cursor"].as_str().unwrap()));
    let (_, second) = server.query("audit", &next).await;
    assert_eq!(items(&second).len(), 2);
    assert_eq!(items(&second)[1]["actor_id"], "PublicActor4");
    assert!(second["next_cursor"].is_null());
    next[0] = ("q", "PublicActor");
    assert_eq!(
        server.query("audit", &next).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    for query in [
        "private-body-marker",
        "endpoint",
        "secret-provider-text",
        "not_a_wildcard%",
        "query__",
    ] {
        assert!(items(&server.query("audit", &[("q", query)]).await.1).is_empty());
    }
    assert_eq!(
        items(&server.query("audit", &[("q", "PUBLICACTOR5")]).await.1).len(),
        1
    );
    let (_, old) = server.get("/api/v1/audit?limit=1").await;
    assert!(
        old["next_cursor"]
            .as_str()
            .unwrap()
            .starts_with("v1:audit:")
    );
    assert_eq!(
        server
            .query("audit", &[("cursor", old["next_cursor"].as_str().unwrap())])
            .await
            .0,
        StatusCode::OK
    );
    sqlx::query("UPDATE events SET actor_id='İSTANBUL' WHERE actor_id='PublicActor5'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        items(&server.query("audit", &[("q", "İSTANBUL")]).await.1).len(),
        1,
        "Unicode case matching must use the same normalization for query and metadata"
    );
}
#[sqlx::test]
#[ignore = "isolated real PostgreSQL and TCP HTTP; P02 locked runner"]
async fn filters_do_not_leak_recursive_hidden_fields_or_raw_errors(pool: PgPool) {
    let server = setup(&pool).await;
    let hidden = run(
        &server,
        &pool,
        0,
        "foreground",
        "raw-provider-secret-marker",
    )
    .await;
    let visible = run(
        &server,
        &pool,
        1,
        "foreground",
        "raw-provider-secret-marker",
    )
    .await;
    // Block a source ancestor, leaving the dependent run/event rows untouched.
    let source: Uuid = sqlx::query_scalar("SELECT trigger_event_id FROM runs WHERE id=$1")
        .bind(hidden)
        .fetch_one(&pool)
        .await
        .unwrap();
    let leaf: Uuid = sqlx::query_scalar("SELECT trigger_event_id FROM runs WHERE id=$1")
        .bind(visible)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE events SET correlation_id=$1 WHERE id=$2")
        .bind(source)
        .bind(leaf)
        .execute(&pool)
        .await
        .unwrap();
    for name in ["runs", "audit"] {
        assert!(
            items(
                &server
                    .query(name, &[("q", "raw-provider-secret-marker")])
                    .await
                    .1
            )
            .is_empty()
        );
    }
    assert_eq!(
        items(
            &server
                .query("runs", &[("q", "unclassified_failure")])
                .await
                .1
        )
        .len(),
        2
    );
    assert_eq!(
        items(&server.query("audit", &[("q", &source.to_string())]).await.1).len(),
        2
    );
    let ancestor = Uuid::new_v4();
    let middle = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind,blocked) SELECT individual_id,$2,'event',true FROM objects WHERE id=$1 UNION ALL SELECT individual_id,$3,'event',false FROM objects WHERE id=$1")
        .bind(source).bind(ancestor).bind(middle).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT individual_id,$2,$3 FROM objects WHERE id=$1 UNION ALL SELECT individual_id,$3,$1 FROM objects WHERE id=$1")
        .bind(source).bind(ancestor).bind(middle).execute(&pool).await.unwrap();
    let directly_blocked: bool = sqlx::query_scalar("SELECT blocked FROM objects WHERE id=$1")
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        !directly_blocked,
        "only the distant ancestor is directly blocked"
    );
    assert!(items(&server.query("runs", &[("q", &hidden.to_string())]).await.1).is_empty());
    assert_eq!(
        server.get(&format!("/api/v1/runs/{hidden}")).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, audit) = server.query("audit", &[("q", "PublicActor1")]).await;
    assert_eq!(items(&audit).len(), 1);
    assert!(items(&audit)[0]["correlation_id"].is_null());
    assert!(items(&server.query("audit", &[("q", &source.to_string())]).await.1).is_empty());
    for name in ["runs", "audit"] {
        let (_, v) = server.query(name, &[]).await;
        let encoded = v.to_string();
        for secret in [
            "private-body-marker",
            "raw-provider-secret-marker",
            "secret-provider-text",
            TOKEN,
        ] {
            assert!(!encoded.contains(secret));
        }
    }
}
#[sqlx::test]
#[ignore = "isolated real PostgreSQL and TCP HTTP; P02 locked runner"]
async fn query_validation_auth_and_origin_remain_strict(pool: PgPool) {
    let server = setup(&pool).await;
    for path in [
        "runs?q=x&q=y",
        "runs?mode=other",
        "runs?mode=foreground&mode=background",
        "runs?status=failed&status=queued",
        "runs?cursor=x&cursor=y",
        "runs?to=x&to=y",
        "audit?cursor=v1:audit:-1",
        "audit?cursor=v1:audit:0",
        "audit?cursor=v2:audit:wrong:1",
        "audit?unknown=1",
        "runs?status=other",
        "runs?limit=101",
        "runs?limit=0",
        "runs?cursor=garbage",
        "runs?from=no",
        "runs?from=2026-01-01T01:00:00%2B01:00",
        "runs?from=2026-01-01T00:00:00Z&to=2026-01-01T00:00:00Z",
        "runs?unknown=1",
        "audit?q=x&q=y",
        "audit?from=x&from=y",
        "audit?status=failed",
        "audit?mode=background",
        "audit?order=asc",
        "audit?cursor=v1:runs:bad",
        "audit?limit=1&limit=2",
        "audit?to=no",
        "runs?q=%00",
    ] {
        let (s, v) = server.get(&format!("/api/v1/{path}")).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {v}");
    }
    let too_long = "界".repeat(201);
    assert_eq!(
        server.query("audit", &[("q", &too_long)]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let allowed = "界".repeat(200);
    assert_eq!(
        server.query("runs", &[("q", &allowed)]).await.0,
        StatusCode::OK
    );
    assert_eq!(
        server
            .client
            .get(format!("{}/api/v1/runs", server.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .client
            .get(format!("{}/api/v1/audit", server.base))
            .bearer_auth(TOKEN)
            .header("Origin", "https://foreign.invalid")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
}
