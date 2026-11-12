//! Real TCP/HTTP + disposable PostgreSQL; all fixture content is synthetic.
use alia::{adapters::store::Store, runtime::Runtime, service};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

const TOKEN: &str = "p04-beliefs-synthetic-owner-token";
struct Server {
    base: String,
    client: Client,
    store: Store,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    fn belief(&self, id: Uuid) -> RequestBuilder {
        self.client
            .get(format!("{}/api/v1/beliefs/{id}", self.base))
            .bearer_auth(TOKEN)
    }
}
async fn send(request: RequestBuilder) -> (StatusCode, Value) {
    let response = request.send().await.unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let status = response.status();
    (status, response.json().await.unwrap())
}
async fn setup(pool: &PgPool, initialize: bool) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let runtime = Runtime::new(pool.clone(), TOKEN).unwrap();
    let store = runtime.store.clone();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = service::router(runtime, PathBuf::from("web/dist"));
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let s = Server {
        base,
        store,
        task,
        client: Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap(),
    };
    if initialize {
        let (status, body) = send(s.client.post(format!("{}/api/v1/initialize", s.base))
            .bearer_auth(TOKEN).header("Idempotency-Key", "source-fixture-init")
            .json(&json!({"persona":serde_json::from_str::<Value>(include_str!("../config/persona.example.json")).unwrap(),
                "runtime":serde_json::from_str::<Value>(include_str!("../config/runtime.example.json")).unwrap()}))).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    s
}
// Explicit persisted-event fixtures exercise projection/privacy independently of models.
async fn event(pool: &PgPool, kind: &str, source: &str, actor: &str, payload: Value) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'event' FROM individuals")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) \
        SELECT id,$1,$2,$3,$4,clock_timestamp(),$5,$6 FROM individuals")
        .bind(id).bind(kind).bind(source).bind(actor).bind(payload).bind(Uuid::new_v4())
        .execute(pool).await.unwrap();
    id
}
async fn edge(pool: &PgPool, source: Uuid, target: Uuid) {
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT id,$1,$2 FROM individuals")
        .bind(source).bind(target).execute(pool).await.unwrap();
}
async fn block(pool: &PgPool, id: Uuid, blocked: bool) {
    sqlx::query("UPDATE objects SET blocked=$2 WHERE id=$1")
        .bind(id)
        .bind(blocked)
        .execute(pool)
        .await
        .unwrap();
}
async fn unchanged(pool: &PgPool) -> Value {
    // Include every actual public base table, not only event/usage counts.
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut digests = serde_json::Map::new();
    for table in tables {
        assert!(table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        let digest: String = sqlx::query_scalar(&format!(
            "SELECT md5(COALESCE(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM \"{table}\" t"))
            .fetch_one(pool).await.unwrap();
        digests.insert(table, json!(digest));
    }
    Value::Object(digests)
}

async fn list(s: &Server, params: &[(&str, &str)]) -> (StatusCode, Value) {
    send(
        s.client
            .get(format!("{}/api/v1/beliefs", s.base))
            .bearer_auth(TOKEN)
            .query(params),
    )
    .await
}
async fn belief(
    pool: &PgPool,
    statement: &str,
    scope: &str,
    previous: Option<Uuid>,
    support: &[Uuid],
    counter: &[Uuid],
    time: i64,
) -> Uuid {
    let id = Uuid::new_v4();
    if let Some(previous) = previous {
        sqlx::query("UPDATE beliefs SET status='superseded',version=version+1,superseded_at='2026-01-01T00:00:00Z'::timestamptz+$2*interval '1 second' WHERE id=$1")
   .bind(previous).bind(time).execute(pool).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'belief' FROM individuals",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO beliefs(individual_id,id,statement,scope,position,brief_basis,supersedes,created_at) SELECT id,$1,$2,$3,'held','保存的依据 İΩ %_ ',$4,'2026-01-01T00:00:00Z'::timestamptz+$5*interval '1 second' FROM individuals")
  .bind(id).bind(statement).bind(scope).bind(previous).bind(time).execute(pool).await.unwrap();
    for (role, ids) in [("support", support), ("counter", counter)] {
        for event in ids {
            sqlx::query("INSERT INTO belief_evidence(individual_id,belief_id,event_id,role) SELECT id,$1,$2,$3 FROM individuals")
    .bind(id).bind(event).bind(role).execute(pool).await.unwrap();
        }
    }
    id
}
fn items(v: &Value) -> &[Value] {
    v["items"].as_array().unwrap()
}
fn id(v: &Value) -> Uuid {
    serde_json::from_value(v["id"].clone()).unwrap()
}

#[sqlx::test]
#[ignore = "P04 belief view: isolated PostgreSQL/TCP and total lock required"]
async fn real_stances_paging_status_scope_both_roles_and_chronology(pool: PgPool) {
    let s = setup(&pool, true).await;
    let source = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"合成证据"}),
    )
    .await;
    let mut ids = vec![];
    for n in 0..35 {
        ids.push(
            belief(
                &pool,
                &format!("认识 {n:02}"),
                if n % 2 == 0 { "general" } else { "event:local" },
                None,
                &[source],
                &[source],
                n,
            )
            .await,
        );
    }
    let revised = belief(
        &pool,
        "修订后保留两类来源",
        "general",
        Some(ids[0]),
        &[source],
        &[source],
        40,
    )
    .await;
    let before = unchanged(&pool).await;
    let (status, first) = list(&s, &[]).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(items(&first).len(), 30);
    assert_eq!(id(&items(&first)[0]), revised);
    assert_eq!(first.as_object().unwrap().len(), 6);
    let cursor = first["next_cursor"].as_str().unwrap();
    let (_, second) = list(&s, &[("cursor", cursor)]).await;
    assert_eq!(items(&second).len(), 5);
    assert!(second["next_cursor"].is_null());
    let all: Vec<_> = items(&first).iter().chain(items(&second)).collect();
    assert_eq!(all.len(), 35);
    assert!(!all.iter().any(|v| id(v) == ids[0]));
    for b in all {
        assert_eq!(b.as_object().unwrap().len(), 12);
        assert_eq!(b["support_event_ids"], json!([source]));
        assert_eq!(b["counter_event_ids"], json!([source]));
    }
    let (_, history) = list(&s, &[("status", "superseded")]).await;
    assert_eq!(items(&history).len(), 1);
    let old = &items(&history)[0];
    assert_eq!(id(old), ids[0]);
    assert_eq!(old["version"], 2);
    assert!(old["superseded_at"].is_string());
    let (_, detail) = send(s.belief(revised)).await;
    assert_eq!(detail["belief"]["supersedes"], ids[0].to_string());
    assert_eq!(detail.as_object().unwrap().len(), 5);
    let (_, scoped) = list(&s, &[("scope", "event:local"), ("limit", "100")]).await;
    assert_eq!(items(&scoped).len(), 17);
    assert!(items(&scoped).iter().all(|b| b["scope"] == "event:local"));
    let (_, all) = list(&s, &[("status", "all"), ("limit", "100")]).await;
    assert_eq!(items(&all).len(), 36);
    assert_eq!(before, unchanged(&pool).await);
}

#[sqlx::test]
#[ignore = "P04 belief view: isolated PostgreSQL/TCP and total lock required"]
async fn unicode_literal_filters_bound_cursors_ties_append_and_revoked_anchor(pool: PgPool) {
    let s = setup(&pool, true).await;
    let e = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"证据"}),
    )
    .await;
    let target = belief(&pool, " 周青🙂%_ Case ", "scope Ω ", None, &[e], &[], 0).await;
    let other = belief(&pool, "plain", "general", None, &[e], &[], 0).await;
    for q in ["%_", "🙂", "case", " 周青", "İΩ", "依据"] {
        let (status, v) = list(&s, &[("q", q), ("limit", "100")]).await;
        assert_eq!(status, StatusCode::OK, "{q}: {v}");
        assert!(items(&v).iter().any(|b| id(b) == target));
    }
    let (_, scoped) = list(&s, &[("scope", "scope Ω ")]).await;
    assert_eq!(items(&scoped).len(), 1);
    assert!(items(&list(&s, &[("scope", "scope Ω")]).await.1).is_empty());
    assert!(items(&list(&s, &[("q", "plain ")]).await.1).is_empty());
    let (_, first) = list(&s, &[("status", "all"), ("limit", "1")]).await;
    let cursor = first["next_cursor"].as_str().unwrap();
    let first_id = id(&items(&first)[0]);
    assert_eq!(first_id, target.max(other));
    let appended = belief(&pool, "newer append", "general", None, &[e], &[], 5).await;
    let (_, second) = list(&s, &[("status", "all"), ("limit", "1"), ("cursor", cursor)]).await;
    assert_eq!(id(&items(&second)[0]), target.min(other));
    assert_ne!(id(&items(&second)[0]), appended);
    for params in [
        vec![("cursor", cursor)],
        vec![("cursor", cursor), ("status", "current")],
        vec![("cursor", cursor), ("status", "all"), ("scope", "general")],
        vec![("cursor", cursor), ("status", "all"), ("q", "plain")],
    ] {
        assert_eq!(list(&s, &params).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut forged: Vec<_> = cursor.split(':').map(str::to_owned).collect();
    forged[4] = Uuid::new_v4().to_string();
    let forged = forged.join(":");
    assert_eq!(
        list(&s, &[("status", "all"), ("cursor", &forged)]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    block(&pool, first_id, true).await;
    assert_eq!(
        list(&s, &[("status", "all"), ("cursor", cursor)]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test]
#[ignore = "P04 belief view: isolated PostgreSQL/TCP and total lock required"]
async fn all_evidence_and_predecessors_are_recursive_privacy_barriers_even_without_edges(
    pool: PgPool,
) {
    let s = setup(&pool, true).await;
    let support = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"支持"}),
    )
    .await;
    let ancestor = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"私有反证上游"}),
    )
    .await;
    let middle = event(&pool, "synthetic.middle", "inference", "alia", json!({})).await;
    let counter = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"反证"}),
    )
    .await;
    edge(&pool, ancestor, middle).await;
    edge(&pool, middle, counter).await;
    let old = belief(
        &pool,
        "私有旧认识",
        "general",
        None,
        &[support],
        &[counter],
        0,
    )
    .await;
    let current = belief(
        &pool,
        "私有新认识",
        "general",
        Some(old),
        &[support],
        &[],
        1,
    )
    .await;
    let safe = belief(
        &pool,
        "可见独立认识",
        "event:safe",
        None,
        &[support],
        &[],
        2,
    )
    .await;
    block(&pool, ancestor, true).await;
    let before = unchanged(&pool).await;
    for target in [old, current] {
        let (status, v) = send(s.belief(target)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(!v.to_string().contains(&target.to_string()));
    }
    let (_, all) = list(&s, &[("status", "all")]).await;
    assert_eq!(items(&all).len(), 1);
    assert_eq!(id(&items(&all)[0]), safe);
    assert!(!all.to_string().contains("私有"));
    assert_eq!(before, unchanged(&pool).await);
    block(&pool, ancestor, false).await;
    block(&pool, old, true).await;
    assert_eq!(send(s.belief(current)).await.0, StatusCode::NOT_FOUND);
    block(&pool, old, false).await;
    block(&pool, support, true).await;
    assert!(items(&list(&s, &[("status", "all")]).await.1).is_empty());
    block(&pool, support, false).await;
    // A two-node predecessor cycle terminates rather than hanging.
    sqlx::query("UPDATE beliefs SET supersedes=$2 WHERE id=$1")
        .bind(old)
        .bind(current)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(send(s.belief(current)).await.0, StatusCode::OK);
    let individual = s.store.snapshot().await.unwrap().unwrap().id;
    block(&pool, individual, true).await;
    let before = unchanged(&pool).await;
    assert_eq!(list(&s, &[]).await.0, StatusCode::NOT_FOUND);
    assert_eq!(send(s.belief(safe)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(before, unchanged(&pool).await);
}

#[sqlx::test]
#[ignore = "P04 belief view: isolated PostgreSQL/TCP and total lock required"]
async fn strict_query_auth_and_uninitialized_errors_are_read_only(pool: PgPool) {
    let s = setup(&pool, false).await;
    assert_eq!(list(&s, &[]).await.0, StatusCode::CONFLICT);
    assert_eq!(send(s.belief(Uuid::new_v4())).await.0, StatusCode::CONFLICT);
    drop(s);
    let s = setup(&pool, true).await;
    let e = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({}),
    )
    .await;
    let b = belief(&pool, "合法认识", "general", None, &[e], &[], 0).await;
    let before = unchanged(&pool).await;
    for params in [
        vec![("q", " ")],
        vec![("q", "x\n")],
        vec![("q", "x"), ("q", "y")],
        vec![("scope", "")],
        vec![("scope", " \t")],
        vec![("scope", "general"), ("scope", "general")],
        vec![("status", "")],
        vec![("status", "held")],
        vec![("status", "all"), ("status", "all")],
        vec![("limit", "0")],
        vec![("limit", "101")],
        vec![("limit", "-1")],
        vec![("limit", "1"), ("limit", "2")],
        vec![("cursor", "")],
        vec![("cursor", "x"), ("cursor", "y")],
        vec![("unknown", "x")],
    ] {
        let (status, v) = list(&s, &params).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{params:?}: {v}");
    }
    let long = "🙂".repeat(201);
    for key in ["q", "scope"] {
        assert_eq!(
            list(&s, &[(key, &long)]).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        send(s.belief(b).query(&[("q", "")])).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        send(s.belief(Uuid::new_v4())).await.0,
        StatusCode::NOT_FOUND
    );
    for path in ["/beliefs".to_owned(), format!("/beliefs/{b}")] {
        assert_eq!(
            send(s.client.get(format!("{}/api/v1{path}", s.base)))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                s.client
                    .get(format!("{}/api/v1{path}", s.base))
                    .bearer_auth(TOKEN)
                    .header("Origin", "https://untrusted.example")
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        send(
            s.client
                .get(format!("{}/api/v1/beliefs/not-a-uuid", s.base))
                .bearer_auth(TOKEN)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(before, unchanged(&pool).await);
}
