//! Read-only history against real TCP HTTP and isolated PostgreSQL; synthetic data.
use alia::{
    adapters::store::Store,
    domain::history::{MemoryKind, NewMemory},
    runtime::Runtime,
    service,
};
use chrono::{DateTime, Utc};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

const TOKEN: &str = "isolated-memory-history-owner-token-not-production";
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
async fn send(request: RequestBuilder) -> (StatusCode, Value) {
    let response = request.send().await.unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let status = response.status();
    (status, response.json().await.unwrap())
}
impl Server {
    fn history(&self, target: Uuid) -> RequestBuilder {
        self.client
            .get(format!("{}/api/v1/memories/{target}/history", self.base))
            .bearer_auth(TOKEN)
    }
    async fn correction(&self, previous: Uuid, number: usize) -> (Uuid, Uuid) {
        let (_, view) = send(
            self.client
                .get(format!("{}/api/v1/memories/{previous}", self.base))
                .bearer_auth(TOKEN),
        )
        .await;
        let (status, result) = send(
            self.client
                .post(format!(
                    "{}/api/v1/memories/{previous}/revisions",
                    self.base
                ))
                .bearer_auth(TOKEN)
                .header("Idempotency-Key", Uuid::new_v4().to_string())
                .header("If-Match", format!("\"{}\"", view["memory"]["version"]))
                .json(
                    &json!({"content":format!("合成修订{number}"),"scope":"general",
                  "reason":format!("实际用户纠正理由{number}")}),
                ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{result}");
        (id(&result, "memory_id"), id(&result, "event_id"))
    }
}
fn id(value: &Value, key: &str) -> Uuid {
    serde_json::from_value(value[key].clone()).unwrap()
}
fn ids(value: &Value) -> Vec<Uuid> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| id(&item["memory"], "id"))
        .collect()
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
        let (status, value) = send(
            s.client
                .post(format!("{}/api/v1/initialize", s.base))
                .bearer_auth(TOKEN)
                .header("Idempotency-Key", "history-fixture-birth")
                .json(&json!({
                  "persona":serde_json::from_str::<Value>(include_str!("../config/persona.example.json")).unwrap(),
                  "runtime":serde_json::from_str::<Value>(include_str!("../config/runtime.example.json")).unwrap()
                })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{value}");
    }
    s
}
async fn original(s: &Server, text: &str) -> (Uuid, Uuid) {
    let event = s
        .store
        .submit_message(&Uuid::new_v4().to_string(), text, None)
        .await
        .unwrap()
        .event_id;
    let receipt = s
        .store
        .record_memories(
            &Uuid::new_v4().to_string(),
            s.store.snapshot().await.unwrap().unwrap().expected(),
            &[event],
            &[NewMemory {
                content: text.into(),
                scope: "general".into(),
                kind: MemoryKind::Semantic,
                source_event_ids: vec![event],
                supersedes: None,
            }],
        )
        .await
        .unwrap();
    (receipt.memory_ids[0], event)
}
async fn chain(s: &Server, count: usize) -> (Vec<Uuid>, Vec<Uuid>) {
    let (first, source) = original(s, "合成初始认识").await;
    let mut memories = vec![first];
    let mut events = vec![source];
    for number in 1..count {
        let (memory, event) = s.correction(*memories.last().unwrap(), number).await;
        memories.push(memory);
        events.push(event);
    }
    (memories, events)
}
async fn unchanged_snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(\
      'individuals',(SELECT jsonb_agg(to_jsonb(i) ORDER BY id) FROM individuals i),\
      'events',(SELECT count(*) FROM events),'memories',(SELECT count(*) FROM memories),\
      'outbox',(SELECT count(*) FROM outbox),'edges',(SELECT count(*) FROM derivation_edges),\
      'usage',(SELECT count(*) FROM usage_reservations),'actions',(SELECT count(*) FROM actions),\
      'receipts',(SELECT count(*) FROM idempotency_records))",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}
async fn block(pool: &PgPool, object: Uuid, blocked: bool) {
    sqlx::query("UPDATE objects SET blocked=$2 WHERE id=$1")
        .bind(object)
        .bind(blocked)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
#[ignore = "P04 history: isolated PostgreSQL/TCP and total lock required"]
async fn complete_chain_pages_have_true_reasons_and_do_not_write_anything(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (memories, events) = chain(&s, 33).await;
    let before = unchanged_snapshot(&pool).await;
    let (status, first) = send(s.history(memories[16])).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["items"].as_array().unwrap().len(), 30);
    assert!(first["next_cursor"].is_string());
    assert_eq!(first["target_memory_id"], memories[16].to_string());
    assert!(first["state_version"].as_i64().unwrap() > 1);
    assert_eq!(first["privacy_epoch"], 0);
    assert!(DateTime::parse_from_rfc3339(first["observed_at"].as_str().unwrap()).is_ok());
    let mut cursor: Option<String> = None;
    let mut all = Vec::new();
    loop {
        let mut request = s.history(memories[16]).query(&[("limit", "7")]);
        if let Some(cursor) = &cursor {
            request = request.query(&[("cursor", cursor)]);
        }
        let (status, page) = send(request).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        all.extend(page["items"].as_array().unwrap().iter().cloned());
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
        assert!(all.len() <= memories.len(), "pagination repeated rows");
    }
    assert_eq!(
        all.iter()
            .map(|item| id(&item["memory"], "id"))
            .collect::<Vec<_>>(),
        memories
    );
    assert!(
        all[0]["correction"].is_null(),
        "original reason was fabricated"
    );
    for index in 1..all.len() {
        assert_eq!(
            all[index]["memory"]["supersedes"],
            memories[index - 1].to_string()
        );
        assert_eq!(
            all[index]["memory"]["source_event_ids"],
            json!([events[index]])
        );
        assert_eq!(
            all[index]["correction"]["event_id"],
            events[index].to_string()
        );
        assert_eq!(
            all[index]["correction"]["reason"],
            format!("实际用户纠正理由{index}")
        );
        assert!(
            DateTime::parse_from_rfc3339(all[index]["correction"]["occurred_at"].as_str().unwrap())
                .is_ok()
        );
    }
    assert_eq!(before, unchanged_snapshot(&pool).await);
}

#[sqlx::test]
#[ignore = "P04 history: isolated PostgreSQL/TCP and total lock required"]
async fn tied_times_cursor_binding_and_concurrent_append_are_stable(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (memories, _) = chain(&s, 4).await;
    sqlx::query("UPDATE memories SET created_at='2026-01-01T00:00:00Z'")
        .execute(&pool)
        .await
        .unwrap();
    let mut ordered = memories.clone();
    ordered.sort();
    let target = memories[1];
    let (status, first) = send(s.history(target).query(&[("limit", "2")])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&first), ordered[..2]);
    let cursor = first["next_cursor"].as_str().unwrap();
    let (appended, _) = s.correction(memories[3], 4).await;
    let (status, next) = send(
        s.history(target)
            .query(&[("limit", "10"), ("cursor", cursor)]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(ids(&next), [ordered[2], ordered[3], appended]);
    assert!(next["next_cursor"].is_null());
    assert_eq!(
        send(s.history(memories[0]).query(&[("cursor", cursor)]))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (unrelated, _) = original(&s, "不是修订链的另一个记忆").await;
    let stamp = Utc::now().timestamp_micros();
    let fabricated = format!("v1:memory-history-asc:{target}:{stamp}:{unrelated}");
    assert_eq!(
        send(s.history(target).query(&[("cursor", fabricated)]))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut parts: Vec<String> = cursor.split(':').map(str::to_owned).collect();
    parts[3] = (parts[3].parse::<i64>().unwrap() + 1).to_string();
    assert_eq!(
        send(s.history(target).query(&[("cursor", parts.join(":"))]))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, singleton) = send(s.history(unrelated)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&singleton), [unrelated]);
}

#[sqlx::test]
#[ignore = "P04 history: isolated PostgreSQL/TCP and total lock required"]
async fn blocked_ancestors_successors_links_and_individual_never_leak(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (memories, events) = chain(&s, 3).await;
    let (public, _) = original(&s, "独立可见记忆").await;
    block(&pool, events[0], true).await;
    let before = unchanged_snapshot(&pool).await;
    let (status, missing) = send(s.history(memories[2])).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!missing.to_string().contains(&memories[2].to_string()));
    assert_eq!(ids(&send(s.history(public)).await.1), [public]);
    assert_eq!(before, unchanged_snapshot(&pool).await);
    block(&pool, events[0], false).await;
    let (_, page) = send(s.history(memories[0]).query(&[("limit", "2")])).await;
    let cursor = page["next_cursor"].as_str().unwrap();
    block(&pool, events[1], true).await;
    let (status, prefix) = send(s.history(memories[0])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&prefix), [memories[0]]);
    assert!(prefix["next_cursor"].is_null());
    assert!(!prefix.to_string().contains(&memories[1].to_string()));
    assert!(!prefix.to_string().contains(&events[1].to_string()));
    assert_eq!(
        send(s.history(memories[0]).query(&[("cursor", cursor)]))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // A malformed old link is not permission to expose a hidden predecessor ID.
    sqlx::query("UPDATE memories SET supersedes=$2 WHERE id=$1")
        .bind(public)
        .bind(memories[1])
        .execute(&pool)
        .await
        .unwrap();
    let (status, public_view) = send(s.history(public)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&public_view), [public]);
    assert!(public_view["items"][0]["memory"]["supersedes"].is_null());
    assert!(!public_view.to_string().contains(&memories[1].to_string()));
    let individual = s.store.snapshot().await.unwrap().unwrap().id;
    block(&pool, individual, true).await;
    assert_eq!(send(s.history(public)).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test]
#[ignore = "P04 history: isolated PostgreSQL/TCP and total lock required"]
async fn supersession_cycles_are_finite_and_foreign_or_other_scope_rows_are_excluded(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (memories, _) = chain(&s, 2).await;
    let (unrelated, _) = original(&s, "同库无关记录").await;
    sqlx::query("UPDATE memories SET supersedes=$2 WHERE id=$1")
        .bind(memories[0])
        .bind(memories[1])
        .execute(&pool)
        .await
        .unwrap();
    let before = unchanged_snapshot(&pool).await;
    let (status, cycle) = send(s.history(memories[0])).await;
    assert_eq!(status, StatusCode::OK, "{cycle}");
    assert_eq!(ids(&cycle), memories);
    assert_eq!(before, unchanged_snapshot(&pool).await);
    let foreign = Uuid::new_v4();
    // Do not weaken the single-identity schema to manufacture a foreign row.
    let error = sqlx::query("INSERT INTO individuals(id,singleton,origin,seed,seed_digest,persona,runtime_config,lifecycle) \
      SELECT $1,false,origin,seed,seed_digest,persona,runtime_config,'archived' FROM individuals WHERE singleton")
        .bind(foreign).execute(&pool).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    let foreign_only = Uuid::new_v4();
    let error = sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'memory')")
        .bind(foreign)
        .bind(foreign_only)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    let (status, owned) = send(s.history(memories[0])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&owned), memories);
    assert_eq!(send(s.history(foreign_only)).await.0, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE memories SET scope='event:unrelated-scope',supersedes=$2 WHERE id=$1")
        .bind(unrelated)
        .bind(memories[0])
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(ids(&send(s.history(memories[0])).await.1), memories);
    let other = send(s.history(unrelated)).await.1;
    assert_eq!(ids(&other), [unrelated]);
    assert!(other["items"][0]["memory"]["supersedes"].is_null());
}

async fn linked_event(
    pool: &PgPool,
    memory: Uuid,
    payload: Value,
    actor: &str,
    source_kind: &str,
    linked: bool,
) -> Uuid {
    let individual: Uuid = sqlx::query_scalar("SELECT id FROM individuals WHERE singleton")
        .fetch_one(pool)
        .await
        .unwrap();
    let event = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'event')")
        .bind(individual)
        .bind(event)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) \
      VALUES($1,$2,'memory.corrected',$3,$4,clock_timestamp(),$5,$2)")
        .bind(individual).bind(event).bind(source_kind).bind(actor).bind(payload).execute(pool).await.unwrap();
    if linked {
        sqlx::query(
            "INSERT INTO memory_sources(individual_id,memory_id,event_id) VALUES($1,$2,$3)",
        )
        .bind(individual)
        .bind(memory)
        .bind(event)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES($1,$2,$3)",
        )
        .bind(individual)
        .bind(event)
        .bind(memory)
        .execute(pool)
        .await
        .unwrap();
    }
    event
}

#[sqlx::test]
#[ignore = "P04 history: isolated PostgreSQL/TCP and total lock required"]
async fn correction_reasons_require_real_matching_owner_sources_not_similar_events(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (memories, events) = chain(&s, 2).await;
    let target = memories[1];
    let actual = json!({"previous_memory_id":memories[0],"text":"合成修订1","scope":"general","reason":"不能采用的伪理由"});
    linked_event(
        &pool,
        target,
        actual.clone(),
        "owner",
        "reported_claim",
        false,
    )
    .await;
    linked_event(
        &pool,
        target,
        actual.clone(),
        "alia",
        "reported_claim",
        true,
    )
    .await;
    linked_event(&pool, target, actual.clone(), "owner", "generated", true).await;
    for (field, value) in [
        ("previous_memory_id", json!(Uuid::new_v4())),
        ("text", json!("其他正文")),
        ("scope", json!("event:other")),
        ("reason", json!(42)),
        ("reason", json!(" \n\t ")),
    ] {
        let mut payload = actual.clone();
        payload[field] = value;
        linked_event(&pool, target, payload, "owner", "reported_claim", true).await;
    }
    let before = unchanged_snapshot(&pool).await;
    let (status, result) = send(s.history(target)).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(
        result["items"][1]["correction"]["event_id"],
        events[1].to_string()
    );
    assert_eq!(
        result["items"][1]["correction"]["reason"],
        "实际用户纠正理由1"
    );
    assert!(result["items"][0]["correction"].is_null());
    assert_eq!(before, unchanged_snapshot(&pool).await);
    // An actually linked later owner statement with matching provenance is reported as such.
    let mut valid = actual;
    valid["reason"] = json!("后续实际关联的用户理由");
    let later = linked_event(&pool, target, valid, "owner", "reported_claim", true).await;
    let (_, result) = send(s.history(target)).await;
    assert_eq!(
        result["items"][1]["correction"]["event_id"],
        later.to_string()
    );
    assert_eq!(
        result["items"][1]["correction"]["reason"],
        "后续实际关联的用户理由"
    );
}

#[sqlx::test]
#[ignore = "P04 history: isolated PostgreSQL/TCP and total lock required"]
async fn history_queries_auth_and_versions_are_strict_without_side_effects(pool: PgPool) {
    let s = setup(&pool, false).await;
    let missing = Uuid::new_v4();
    assert_eq!(send(s.history(missing)).await.0, StatusCode::CONFLICT);
    drop(s);
    let s = setup(&pool, true).await;
    let (memories, _) = chain(&s, 2).await;
    let before = unchanged_snapshot(&pool).await;
    for params in [
        vec![("limit", "0")],
        vec![("limit", "101")],
        vec![("limit", "-1")],
        vec![("limit", "x")],
        vec![("limit", "1"), ("limit", "2")],
        vec![("q", "text")],
        vec![("cursor", "x"), ("cursor", "y")],
        vec![("cursor", "")],
        vec![("cursor", "v1:documents:any:0:any")],
        vec![("order", "desc")],
    ] {
        let (status, value) = send(s.history(memories[0]).query(&params)).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{params:?}: {value}"
        );
    }
    let prefix = format!("{}/api/v1/memories", s.base);
    assert_eq!(
        send(s.client.get(format!("{prefix}/{}/history", memories[0])))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            s.history(memories[0])
                .header("Origin", "https://untrusted.example")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            s.client
                .get(format!("{prefix}/not-a-uuid/history"))
                .bearer_auth(TOKEN)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(send(s.history(missing)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        send(s.history(memories[0]).query(&[("limit", "100")]))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(before, unchanged_snapshot(&pool).await);
}
