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
        let mut runtime =
            serde_json::from_str::<Value>(include_str!("../config/runtime.example.json")).unwrap();
        runtime["capabilities"]["enabled"] = json!(alia::runtime::capability::LOCAL_CAPABILITIES);
        let response = server.client.post(format!("{}/api/v1/initialize",server.base)).bearer_auth(TOKEN)
            .header("Idempotency-Key","p05-fixture-init").json(&json!({
                "persona":serde_json::from_str::<Value>(include_str!("../config/persona.example.json")).unwrap(),
                "runtime":runtime
            })).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    server
}

async fn note(pool: &PgPool, title: &str, text: &str) -> Uuid {
    use alia::{
        adapters::store::Store,
        runtime::{
            execution::model_step,
            model::{CallLimits, FixtureProvider},
        },
    };
    let store = Store::new(pool.clone());
    store
        .submit_message(&Uuid::new_v4().to_string(), "隔离测试生成笔记", None)
        .await
        .unwrap();
    let mut lease = store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let caps = store.local_capabilities().await.unwrap();
    let proposal = json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[{"capability_id":"notes.write","input":{"title":title,"text":text}}],"development":null,"brief_reason":"隔离合成验收"});
    let ready = model_step(
        &store,
        &FixtureProvider {
            response: proposal.to_string(),
        },
        &lease,
        caps.clone(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    let receipt = store.commit_decision(&lease, &ready, &caps).await.unwrap();
    lease.state_version = receipt.state_version;
    let action = receipt.action_ids[0];
    let result = store.execute_local_action(&lease, action).await.unwrap();
    assert_eq!(result.status, "succeeded");
    sqlx::query("UPDATE runs SET status='committed' WHERE id=$1")
        .bind(lease.run_id)
        .execute(pool)
        .await
        .unwrap();
    action
}
async fn counts(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('identity',(SELECT md5(to_jsonb(i)::text) FROM individuals i),'events',(SELECT md5(coalesce(string_agg(to_jsonb(e)::text,'' ORDER BY sequence),'')) FROM events e),'notes',(SELECT md5(coalesce(string_agg(to_jsonb(n)::text,'' ORDER BY action_id),'')) FROM notes n),'objects',(SELECT count(*) FROM objects),'runs',(SELECT count(*) FROM runs),'actions',(SELECT count(*) FROM actions),'usage',(SELECT count(*) FROM usage_reservations),'outbox',(SELECT count(*) FROM outbox))").fetch_one(pool).await.unwrap()
}
fn items(v: &Value) -> &[Value] {
    v["items"].as_array().unwrap()
}
#[sqlx::test]
#[ignore = "P05 isolated real PostgreSQL/HTTP; run under shared heavy lock"]
async fn notes_real_save_paging_filter_cursor_and_no_read_mutation(pool: PgPool) {
    let server = setup(&pool, true).await;
    let mut ids = vec![];
    for n in 0..37 {
        ids.push(
            note(
                &pool,
                &if n == 0 {
                    "周青%_İΩ ".into()
                } else {
                    format!("笔记 {n:02}")
                },
                "body-private-not-searchable",
            )
            .await,
        );
    }
    sqlx::query("UPDATE notes SET created_at='2026-01-01T00:00:00Z'")
        .execute(&pool)
        .await
        .unwrap();
    let before = counts(&pool).await;
    let (s, first) = server.get("/api/v1/notes", &[]).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(items(&first).len(), 30);
    assert!(!first.to_string().contains("body-private"));
    assert_eq!(items(&first)[0].as_object().unwrap().len(), 7);
    let cursor = first["next_cursor"].as_str().unwrap();
    let (_, second) = server.get("/api/v1/notes", &[("cursor", cursor)]).await;
    assert_eq!(items(&second).len(), 7);
    assert!(second["next_cursor"].is_null());
    let all: Vec<_> = items(&first)
        .iter()
        .chain(items(&second))
        .map(|v| v["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .collect();
    ids.sort_by(|a, b| b.cmp(a));
    assert_eq!(all, ids);
    for q in ["%_", "İ", "周青%_İΩ "] {
        assert_eq!(
            items(&server.get("/api/v1/notes", &[("q", q)]).await.1).len(),
            1
        );
    }
    assert_eq!(
        items(
            &server
                .get("/api/v1/notes", &[("q", &ids[0].to_string()[..8])])
                .await
                .1
        )
        .len(),
        1
    );
    assert_eq!(
        items(
            &server
                .get("/api/v1/notes", &[("q", "body-private")])
                .await
                .1
        )
        .len(),
        0
    );
    assert_eq!(
        server
            .get("/api/v1/notes", &[("cursor", cursor), ("q", "笔记")])
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        server
            .get("/api/v1/notes", &[("cursor", cursor), ("q", " ")])
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        server
            .get("/api/v1/documents", &[("cursor", cursor)])
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(counts(&pool).await, before);
}
#[sqlx::test]
#[ignore = "P05 isolated real PostgreSQL/HTTP; run under shared heavy lock"]
async fn notes_unicode_scalar_fragment_eof_and_strict_ranges(pool: PgPool) {
    let server = setup(&pool, true).await;
    let body = format!("甲🙂e\u{301}乙{}", "文".repeat(3000));
    let id = note(&pool, "../../只为标题", &body).await;
    let before = counts(&pool).await;
    let path = format!("/api/v1/notes/{id}");
    let (s, v) = server.get(&path, &[]).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["text"], body.chars().take(2000).collect::<String>());
    assert_eq!(v["next_offset"], 2000);
    assert_eq!(v["action_id"], id.to_string());
    assert_eq!(v["capability_id"], "notes.write");
    let (_, v) = server
        .get(&path, &[("offset", "1"), ("max_chars", "3")])
        .await;
    assert_eq!(v["text"], "🙂e\u{301}");
    let len = body.chars().count().to_string();
    let (_, v) = server.get(&path, &[("offset", &len)]).await;
    assert_eq!(v["text"], "");
    assert!(v["next_offset"].is_null());
    for q in [
        vec![("offset", "3006")],
        vec![("max_chars", "0")],
        vec![("max_chars", "4097")],
        vec![("offset", "-1")],
        vec![("unknown", "x")],
        vec![("offset", "0"), ("offset", "1")],
    ] {
        assert_eq!(
            server.get(&path, &q).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        server.get("/api/v1/notes/not-uuid", &[]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(counts(&pool).await, before);
}
#[sqlx::test]
#[ignore = "P05 isolated real PostgreSQL/HTTP; run under shared heavy lock"]
async fn notes_visibility_ancestors_result_run_action_and_failed_states(pool: PgPool) {
    let server = setup(&pool, true).await;
    let safe = note(&pool, "独立笔记", "safe").await;
    let bad = note(&pool, "生成目标", "private").await;
    let (owner, run, result): (Uuid, Uuid, Uuid) =
        sqlx::query_as("SELECT individual_id,run_id,result_event_id FROM actions WHERE id=$1")
            .bind(bad)
            .fetch_one(&pool)
            .await
            .unwrap();
    let source = Uuid::new_v4();
    let middle = Uuid::new_v4();
    for id in [source, middle] {
        sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'event')")
            .bind(owner)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES($1,$2,$3),($1,$3,$4)").bind(owner).bind(source).bind(middle).bind(result).execute(&pool).await.unwrap();
    for blocked in [source, result, run, bad] {
        sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
            .bind(blocked)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            server.get(&format!("/api/v1/notes/{bad}"), &[]).await.0,
            StatusCode::NOT_FOUND
        );
        let (_, v) = server.get("/api/v1/notes", &[]).await;
        assert_eq!(items(&v).len(), 1);
        assert_eq!(items(&v)[0]["id"], safe.to_string());
        assert!(!v.to_string().contains(&bad.to_string()));
        sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
            .bind(blocked)
            .execute(&pool)
            .await
            .unwrap();
    }
    for status in ["failed", "unknown", "planned", "executing", "cancelled"] {
        sqlx::query("UPDATE actions SET status=$2 WHERE id=$1")
            .bind(bad)
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            server.get(&format!("/api/v1/notes/{bad}"), &[]).await.0,
            StatusCode::NOT_FOUND
        );
    }
    sqlx::query("UPDATE actions SET status='succeeded',result_event_id=NULL WHERE id=$1")
        .bind(bad)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        server.get(&format!("/api/v1/notes/{bad}"), &[]).await.0,
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        server.get("/api/v1/notes", &[]).await.0,
        StatusCode::NOT_FOUND
    );
}
#[sqlx::test]
#[ignore = "P05 isolated real PostgreSQL/HTTP; run under shared heavy lock"]
async fn notes_auth_initialization_and_query_validation(pool: PgPool) {
    let server = setup(&pool, false).await;
    assert_eq!(
        server.get("/api/v1/notes", &[]).await.0,
        StatusCode::CONFLICT
    );
    let unauth = server
        .client
        .get(format!("{}/api/v1/notes", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
    let cross = server
        .client
        .get(format!("{}/api/v1/notes", server.base))
        .bearer_auth(TOKEN)
        .header("Origin", "https://attacker.invalid")
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), StatusCode::FORBIDDEN);
    let _initialized = setup(&pool, true).await;
    for q in [
        vec![("q", "x"), ("q", "x")],
        vec![("limit", "0")],
        vec![("limit", "101")],
        vec![("unknown", "1")],
        vec![("q", "\n")],
        vec![("cursor", "v1:documents:bad:1:bad")],
    ] {
        assert_eq!(
            server.get("/api/v1/notes", &q).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        server
            .get("/api/v1/notes", &[("q", &"🙂".repeat(201))])
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
