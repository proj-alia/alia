//! Owner corrections against real TCP HTTP and isolated PostgreSQL; no live model.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        entities::{MemoryPersonAssertion, PersonRole, SourceSpan},
        history::{MemoryKind, NewMemory},
        retrieval::{DIMENSION, INDEX_PROFILE, IndexedChunk, RetrievalRequest, SemanticQuery},
    },
    runtime::{
        Runtime,
        execution::model_step,
        model::{CallLimits, FixtureProvider},
    },
    service,
};
use chrono::Utc;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::path::PathBuf;
use uuid::Uuid;

const TOKEN: &str = "isolated-memory-correction-owner-token-not-production";

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
struct Reply {
    status: StatusCode,
    etag: Option<String>,
    value: Value,
}
async fn response(request: RequestBuilder) -> Reply {
    let r = request.send().await.unwrap();
    assert_eq!(r.headers()["cache-control"], "no-store");
    assert_eq!(r.headers()["x-content-type-options"], "nosniff");
    let status = r.status();
    let etag = r.headers().get("etag").map(|v| v.to_str().unwrap().into());
    Reply {
        status,
        etag,
        value: r.json().await.unwrap(),
    }
}
impl Server {
    fn get(&self, path: &str) -> RequestBuilder {
        self.client
            .get(format!("{}{path}", self.base))
            .bearer_auth(TOKEN)
    }
    fn revise(&self, memory: Uuid, key: &str, version: i64, content: &str) -> RequestBuilder {
        self.client
            .post(format!("{}/api/v1/memories/{memory}/revisions", self.base))
            .bearer_auth(TOKEN)
            .header("Idempotency-Key", key)
            .header("If-Match", format!("\"{version}\""))
            .json(&json!({"content":content,"scope":"general","reason":"纠正合成记忆"}))
    }
    async fn lifecycle(&self, action: &str) {
        let version = self.store.snapshot().await.unwrap().unwrap().state_version;
        let r = response(
            self.client
                .post(format!("{}/api/v1/lifecycle/transitions", self.base))
                .bearer_auth(TOKEN)
                .header("Idempotency-Key", Uuid::new_v4().to_string())
                .header("If-Match", format!("\"{version}\""))
                .json(&json!({"action":action})),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.value);
    }
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
        client: Client::new(),
    };
    if initialize {
        let r = response(s.client.post(format!("{}/api/v1/initialize",s.base)).bearer_auth(TOKEN)
            .header("Idempotency-Key","revision-fixture-birth").json(&json!({
              "persona":serde_json::from_str::<Value>(include_str!("../config/persona.example.json")).unwrap(),
              "runtime":serde_json::from_str::<Value>(include_str!("../config/runtime.example.json")).unwrap()
            }))).await;
        assert_eq!(r.status, StatusCode::CREATED);
    }
    s
}
async fn memory(s: &Server, text: &str) -> (Uuid, Uuid) {
    let event = s
        .store
        .submit_message(&Uuid::new_v4().to_string(), text, None)
        .await
        .unwrap()
        .event_id;
    let recorded = s
        .store
        .record_memories(
            &Uuid::new_v4().to_string(),
            s.store.snapshot().await.unwrap().unwrap().expected(),
            &[event],
            &[NewMemory {
                kind: MemoryKind::Semantic,
                content: text.into(),
                scope: "general".into(),
                source_event_ids: vec![event],
                supersedes: None,
            }],
        )
        .await
        .unwrap();
    (recorded.memory_ids[0], event)
}
async fn snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(\
      'state',(SELECT to_jsonb(i) FROM individuals i),\
      'events',(SELECT count(*) FROM events),\
      'memories',(SELECT count(*) FROM memories),\
      'edges',(SELECT count(*) FROM derivation_edges),\
      'outbox',(SELECT count(*) FROM outbox),\
      'receipts',(SELECT count(*) FROM idempotency_records),\
      'usage',(SELECT count(*) FROM usage_reservations))",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}
fn id(value: &Value, key: &str) -> Uuid {
    serde_json::from_value(value[key].clone()).unwrap()
}
fn chunks(text: &str) -> Vec<IndexedChunk> {
    let mut vector = vec![0.0; DIMENSION];
    vector[0] = 1.0;
    vec![IndexedChunk {
        start_char: 0,
        end_char: text.chars().count(),
        vector,
    }]
}

#[sqlx::test]
#[ignore = "requires isolated real PostgreSQL; P04 locked targeted runner"]
async fn owner_revision_chain_retains_sources_and_replays_without_duplicate_effects(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (old, source) = memory(&s, "一般偏好先做原型。").await;
    let original: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap();
    let before = s.store.snapshot().await.unwrap().unwrap();
    let view = response(s.get(&format!("/api/v1/memories/{old}"))).await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.etag.as_deref(), Some("\"1\""));
    assert_eq!(view.value["memory"]["version"], 1);
    assert_eq!(view.value["state_version"], before.state_version);
    let changed = response(s.revise(old, "correct-general", 1, "一般偏好先研究。")).await;
    assert_eq!(changed.status, StatusCode::CREATED, "{}", changed.value);
    assert_eq!(changed.etag.as_deref(), Some("\"1\""));
    assert_eq!(changed.value["previous_memory_id"], old.to_string());
    assert_eq!(changed.value["previous_memory_version"], 2);
    assert_eq!(changed.value["memory_version"], 1);
    assert_eq!(changed.value["version"], before.state_version + 1);
    let new = id(&changed.value, "memory_id");
    let event = id(&changed.value, "event_id");
    assert_ne!(new, old);
    let old_view = response(s.get(&format!("/api/v1/memories/{old}"))).await;
    assert_eq!(old_view.etag.as_deref(), Some("\"2\""));
    assert_eq!(old_view.value["memory"]["validity"], "superseded");
    assert_eq!(old_view.value["memory"]["content"], "一般偏好先做原型。");
    assert_eq!(
        old_view.value["memory"]["source_event_ids"],
        json!([source])
    );
    let new_view = response(s.get(&format!("/api/v1/memories/{new}"))).await;
    assert_eq!(new_view.value["memory"]["content"], "一般偏好先研究。");
    assert_eq!(new_view.value["memory"]["kind"], "semantic");
    assert_eq!(new_view.value["memory"]["scope"], "general");
    assert_eq!(new_view.value["memory"]["supersedes"], old.to_string());
    assert_eq!(new_view.value["memory"]["source_event_ids"], json!([event]));
    assert_eq!(
        old_view.value["memory"]["valid_until"],
        new_view.value["memory"]["valid_from"]
    );
    let claim: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
        .bind(event)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(claim["actor_id"], "owner");
    assert_eq!(claim["source_kind"], "reported_claim");
    assert_eq!(claim["payload"]["text"], "一般偏好先研究。");
    let after_source: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        original, after_source,
        "a correction rewrote the original factual event"
    );
    let stable = snapshot(&pool).await;
    let replay = response(s.revise(old, "correct-general", 1, "一般偏好先研究。")).await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert_eq!(changed.value, replay.value);
    assert_eq!(stable, snapshot(&pool).await);
    let collision = response(s.revise(old, "correct-general", 1, "不同的新陈述")).await;
    assert_eq!(collision.status, StatusCode::CONFLICT);
    assert_eq!(collision.value["error"]["code"], "idempotency_conflict");
    let historical = response(s.revise(old, "rewrite-history", 2, "不能再改旧条目")).await;
    assert_eq!(historical.status, StatusCode::CONFLICT);
    let twice = response(s.revise(new, "second-correction", 1, "一般偏好先核对来源再研究。")).await;
    assert_eq!(twice.status, StatusCode::CREATED);
    let latest = id(&twice.value, "memory_id");
    let ancestry: i64 = sqlx::query_scalar("WITH RECURSIVE ancestors(id) AS (\
      SELECT $1::uuid UNION SELECT source_id FROM derivation_edges d JOIN ancestors a ON d.target_id=a.id\
      ) SELECT count(*) FROM ancestors WHERE id=$2")
        .bind(latest).bind(source).fetch_one(&pool).await.unwrap();
    assert_eq!(ancestry, 1);
    let current = response(s.get("/api/v1/memories?validity=active")).await;
    let current_ids: Vec<_> = current.value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| id(m, "id"))
        .collect();
    assert_eq!(current_ids, [latest]);
    assert_eq!(snapshot(&pool).await["usage"], 0);
}

#[sqlx::test]
#[ignore = "requires isolated real PostgreSQL; P04 locked targeted runner"]
async fn concurrent_owner_revisions_only_one_version_advances_and_same_key_replays(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (old, _) = memory(&s, "第一个并发来源").await;
    let before = s.store.snapshot().await.unwrap().unwrap().state_version;
    let (a, b) = tokio::join!(
        response(s.revise(old, "concurrent-a", 1, "新认识甲")),
        response(s.revise(old, "concurrent-b", 1, "新认识乙"))
    );
    let statuses = [a.status, b.status];
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CREATED)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1
    );
    assert_eq!(
        s.store.snapshot().await.unwrap().unwrap().state_version,
        before + 1
    );
    let (other, _) = memory(&s, "第二个并发来源").await;
    let before = s.store.snapshot().await.unwrap().unwrap().state_version;
    let (a, b) = tokio::join!(
        response(s.revise(other, "same-key", 1, "同一个新陈述")),
        response(s.revise(other, "same-key", 1, "同一个新陈述"))
    );
    assert_eq!(a.status, StatusCode::CREATED);
    assert_eq!(b.status, StatusCode::CREATED);
    assert_eq!(a.value, b.value);
    assert_eq!(
        s.store.snapshot().await.unwrap().unwrap().state_version,
        before + 1
    );
}

#[sqlx::test]
#[ignore = "requires isolated real PostgreSQL; P04 locked targeted runner"]
async fn memory_revision_http_rejects_unsafe_inputs_and_keeps_state_unchanged(pool: PgPool) {
    let s = setup(&pool, false).await;
    let absent = Uuid::new_v4();
    assert_eq!(
        response(s.get(&format!("/api/v1/memories/{absent}")))
            .await
            .status,
        StatusCode::CONFLICT
    );
    let s = {
        drop(s);
        setup(&pool, true).await
    };
    let (old, _) = memory(&s, "严格输入的合成来源").await;
    let url = format!("{}/api/v1/memories/{old}/revisions", s.base);
    let before = snapshot(&pool).await;
    let valid = json!({"content":"更正内容","scope":"general","reason":"理由"});
    for (field, value) in [
        ("content", json!(" ")),
        ("content", json!("字".repeat(2001))),
        ("content", json!("含\0字符")),
        ("scope", json!("不同任务")),
        ("scope", json!(" ")),
        ("scope", json!("字".repeat(201))),
        ("scope", json!("ge\0neral")),
        ("reason", json!(" ")),
        ("reason", json!("字".repeat(1001))),
        ("reason", json!("理\0由")),
        ("extra", json!(true)),
        ("reason", Value::Null),
    ] {
        let mut body = valid.clone();
        body[field] = value;
        let r = response(
            s.client
                .post(&url)
                .bearer_auth(TOKEN)
                .header("Idempotency-Key", "invalid-body")
                .header("If-Match", "\"1\"")
                .json(&body),
        )
        .await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "field {field}: {}",
            r.value
        );
    }
    for body in [
        r#"{"content":"a","content":"b","scope":"general","reason":"r"}"#,
        r#"{"content":"a","scope":"general"}"#,
    ] {
        let r = response(
            s.client
                .post(&url)
                .bearer_auth(TOKEN)
                .header("Idempotency-Key", "invalid-json")
                .header("If-Match", "\"1\"")
                .header("Content-Type", "application/json")
                .body(body),
        )
        .await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    for version in ["1", "W/\"1\"", "\"0\"", "\"-1\"", "\"1\",\"2\""] {
        let r = response(
            s.client
                .post(&url)
                .bearer_auth(TOKEN)
                .header("Idempotency-Key", "invalid-header")
                .header("If-Match", version)
                .json(&valid),
        )
        .await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{version}: {}",
            r.value
        );
    }
    for name in ["If-Match", "Idempotency-Key"] {
        let r = response(
            s.client
                .post(&url)
                .bearer_auth(TOKEN)
                .header("Idempotency-Key", "duplicate-header")
                .header("If-Match", "\"1\"")
                .header(
                    name,
                    if name == "If-Match" {
                        "\"1\""
                    } else {
                        "duplicate-header"
                    },
                )
                .json(&valid),
        )
        .await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    assert_eq!(
        response(
            s.client
                .post(&url)
                .bearer_auth(TOKEN)
                .header("Idempotency-Key", "missing-version")
                .json(&valid)
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        response(
            s.client
                .post(&url)
                .bearer_auth(TOKEN)
                .header("If-Match", "\"1\"")
                .json(&valid)
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        response(
            s.client
                .post(&url)
                .header("Idempotency-Key", "no-token")
                .header("If-Match", "\"1\"")
                .json(&valid)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        response(
            s.revise(old, "cross-site", 1, "不能跨站更正")
                .header("Origin", "https://untrusted.example")
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        response(
            s.client
                .post(&url)
                .bearer_auth(TOKEN)
                .header("Idempotency-Key", "wrong-type")
                .header("If-Match", "\"1\"")
                .body("{}")
        )
        .await
        .status,
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    for suffix in ["?q=x", "?q=x&q=y"] {
        assert_eq!(
            response(s.get(&format!("/api/v1/memories/{old}{suffix}")))
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        response(s.get("/api/v1/memories/not-a-uuid")).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        response(s.get(&format!("/api/v1/memories/{absent}")))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        response(s.revise(absent, "not-found", 1, "无目标"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let deleted = s
        .client
        .delete(format!("{}/api/v1/memories/{old}", s.base))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NOT_IMPLEMENTED);
    assert_eq!(before, snapshot(&pool).await);
    let maximum = response(
        s.client
            .post(&url)
            .bearer_auth(TOKEN)
            .header("Idempotency-Key", "k".repeat(128))
            .header("If-Match", "\"1\"")
            .json(
                &json!({"content":"字".repeat(2000),"scope":"general","reason":"由".repeat(1000)}),
            ),
    )
    .await;
    assert_eq!(maximum.status, StatusCode::CREATED, "{}", maximum.value);
    let current = response(s.get(&format!(
        "/api/v1/memories/{}",
        id(&maximum.value, "memory_id")
    )))
    .await;
    assert_eq!(
        current.value["memory"]["content"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        2000,
        "Unicode character limits must not turn into UTF-8 byte limits"
    );
}

#[sqlx::test]
#[ignore = "requires isolated real PostgreSQL; P04 locked targeted runner"]
async fn memory_revision_lifecycle_and_scope_boundaries(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (old, _) = memory(&s, "一般偏好").await;
    s.lifecycle("pause").await;
    let changed = response(s.revise(old, "paused-correction", 1, "暂停期间也可更正陈述")).await;
    assert_eq!(changed.status, StatusCode::CREATED);
    let current = id(&changed.value, "memory_id");
    s.lifecycle("archive").await;
    let archived = snapshot(&pool).await;
    assert_eq!(
        response(s.revise(current, "archived-correction", 1, "归档不能写"))
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(archived, snapshot(&pool).await);
    s.lifecycle("restore").await;
    s.lifecycle("terminate").await;
    let terminated = snapshot(&pool).await;
    assert_eq!(
        response(s.revise(current, "terminated-correction", 1, "终止不能写"))
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(terminated, snapshot(&pool).await);
}

#[sqlx::test]
#[ignore = "requires isolated real PostgreSQL; P04 locked targeted runner"]
async fn revoked_ancestor_blocks_new_corrections_reads_and_idempotent_receipts(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (first, source) = memory(&s, "第一代私有来源").await;
    let once = response(s.revise(first, "first-correction", 1, "第二代私有认识")).await;
    assert_eq!(once.status, StatusCode::CREATED);
    let second = id(&once.value, "memory_id");
    let twice = response(s.revise(second, "second-correction", 1, "第三代私有认识")).await;
    assert_eq!(twice.status, StatusCode::CREATED);
    let third = id(&twice.value, "memory_id");
    let (public, _) = memory(&s, "独立公开认识").await;
    for blocked in [source, first, id(&once.value, "event_id"), third] {
        sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
            .bind(blocked)
            .execute(&pool)
            .await
            .unwrap();
        let before = snapshot(&pool).await;
        assert_eq!(
            response(s.get(&format!("/api/v1/memories/{third}")))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            response(s.revise(third, "after-revoke", 1, "不能复活"))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            response(s.revise(second, "second-correction", 1, "第三代私有认识"))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(before, snapshot(&pool).await);
        let visible = response(s.get(&format!("/api/v1/memories/{public}"))).await;
        assert_eq!(visible.status, StatusCode::OK);
        assert_eq!(visible.value["memory"]["content"], "独立公开认识");
        sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
            .bind(blocked)
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[sqlx::test]
#[ignore = "requires isolated real PostgreSQL; P04 locked targeted runner"]
async fn late_outbox_failure_rolls_back_correction_and_allows_same_key_retry(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (old, _) = memory(&s, "故障注入旧记忆").await;
    sqlx::query("CREATE FUNCTION reject_p04_state_notice() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic owner correction fault'; END IF; RETURN NEW; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_p04_state_notice BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION reject_p04_state_notice()").execute(&pool).await.unwrap();
    let before = snapshot(&pool).await;
    let failed = response(s.revise(old, "atomic-correction", 1, "故障后新认识")).await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(failed.value["error"]["code"], "storage_unavailable");
    assert!(!failed.value.to_string().contains("synthetic owner"));
    assert_eq!(
        before,
        snapshot(&pool).await,
        "late failure left partial writes"
    );
    let original = response(s.get(&format!("/api/v1/memories/{old}"))).await;
    assert_eq!(original.value["memory"]["validity"], "active");
    assert_eq!(original.value["memory"]["version"], 1);
    sqlx::query("DROP TRIGGER reject_p04_state_notice ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let recovered = response(s.revise(old, "atomic-correction", 1, "故障后新认识")).await;
    assert_eq!(recovered.status, StatusCode::CREATED);
    let stable = snapshot(&pool).await;
    assert_eq!(
        response(s.revise(old, "atomic-correction", 1, "故障后新认识"))
            .await
            .value,
        recovered.value
    );
    assert_eq!(stable, snapshot(&pool).await);
}

#[sqlx::test]
#[ignore = "requires isolated real PostgreSQL; P04 locked targeted runner"]
async fn correction_fences_pending_index_and_person_assertions_without_faking_new_index(
    pool: PgPool,
) {
    let s = setup(&pool, true).await;
    let (old, source) = memory(&s, "沈遥喜欢茶。").await;
    let person = s
        .store
        .register_person(
            "person",
            s.store.snapshot().await.unwrap().unwrap().expected(),
            "p04-synthetic-person",
            "沈遥",
            SourceSpan {
                event_id: source,
                start_char: 0,
                end_char: 2,
            },
        )
        .await
        .unwrap();
    s.store
        .assert_memory_person(
            "person-memory",
            s.store.snapshot().await.unwrap().unwrap().expected(),
            MemoryPersonAssertion {
                memory_id: old,
                memory_version: 1,
                entity_id: person.entity_id,
                role: PersonRole::Subject,
                source: SourceSpan {
                    event_id: source,
                    start_char: 0,
                    end_char: 2,
                },
                memory_start_char: 0,
            },
        )
        .await
        .unwrap();
    let original = response(s.get(&format!("/api/v1/memories/{old}"))).await;
    assert_eq!(
        original.value["memory"]["person_assertions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let pending = s.store.prepare_memory_index(old).await.unwrap();
    let lease = s
        .store
        .claim_memory_index(Uuid::new_v4())
        .await
        .unwrap()
        .unwrap();
    s.store
        .commit_memory_index(&pending, &chunks(pending.content()))
        .await
        .unwrap();
    let changed = response(s.revise(old, "correct-person-memory", 1, "沈遥喜欢咖啡。")).await;
    assert_eq!(changed.status, StatusCode::CREATED);
    let new = id(&changed.value, "memory_id");
    assert!(matches!(
        s.store
            .commit_memory_index(&pending, &chunks(pending.content()))
            .await,
        Err(StoreError::StaleState)
    ));
    assert!(matches!(
        s.store
            .finish_memory_index(&lease, &chunks(lease.content()))
            .await,
        Err(StoreError::LeaseLost | StoreError::StaleState)
    ));
    let next = response(s.get(&format!("/api/v1/memories/{new}"))).await;
    assert_eq!(next.value["memory"]["person_assertions"], json!([]));
    let previous = response(s.get(&format!("/api/v1/memories/{old}"))).await;
    assert_eq!(previous.value["memory"]["person_assertions"], json!([]));
    let new_vectors: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory_vector_sets WHERE memory_id=$1")
            .bind(new)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        new_vectors, 0,
        "owner correction pretended to have an embedding"
    );
    let mut vector = vec![0.; DIMENSION];
    vector[0] = 1.;
    let query = SemanticQuery {
        vector,
        profile: INDEX_PROFILE.into(),
        scope: None,
        subject_id: None,
        person_id: None,
        at: Utc::now(),
        include_history: false,
        limit: 10,
    };
    assert!(
        s.store.search_semantic(&query).await.unwrap().is_empty(),
        "stale vector remained currently eligible"
    );
    let found = s
        .store
        .retrieve_memories(&RetrievalRequest {
            text: "咖啡".into(),
            scope: None,
            subject_id: None,
            person_id: None,
            person_name: None,
            at: Utc::now(),
            include_history: false,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(
        found.memories().iter().map(|m| m.id).collect::<Vec<_>>(),
        [new]
    );
    assert_eq!(found.summary().indexed, 0);
    assert!(found.memories()[0].person_assertions.is_empty());
    let fresh = s.store.prepare_memory_index(new).await.unwrap();
    s.store
        .commit_memory_index(&fresh, &chunks(fresh.content()))
        .await
        .unwrap();
    assert_eq!(
        s.store
            .search_semantic(&query)
            .await
            .unwrap()
            .iter()
            .map(|m| m.id)
            .collect::<Vec<_>>(),
        [new]
    );
}

#[sqlx::test]
#[ignore = "requires isolated real PostgreSQL; explicit fixture model only"]
async fn correction_rejects_late_prepared_model_commit_but_preserves_its_usage(pool: PgPool) {
    let s = setup(&pool, true).await;
    let (old, _) = memory(&s, "旧认识").await;
    let question = s
        .store
        .submit_message("pending-model", "请基于旧认识回答", None)
        .await
        .unwrap();
    let lease = s.store.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let proposal = json!({"schema_version":1,"reply":"过时的合成回复","reply_evidence_ids":[question.event_id],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"明确fixture，仅验证版本屏障"});
    let prepared = model_step(
        &s.store,
        &FixtureProvider {
            response: proposal.to_string(),
        },
        &lease,
        vec![],
        CallLimits::default(),
    )
    .await
    .unwrap();
    assert!(
        prepared.context().memories.iter().any(|m| m.id == old),
        "fixture must actually observe the memory before its correction"
    );
    let usage: Value =
        sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(u) ORDER BY id) FROM usage_reservations u")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(usage.as_array().unwrap().len(), 1);
    assert_eq!(usage[0]["status"], "settled");
    assert_eq!(
        response(s.revise(old, "while-model-pending", 1, "纠正后的认识"))
            .await
            .status,
        StatusCode::CREATED
    );
    assert!(matches!(
        s.store.commit_decision(&lease, &prepared, &[]).await,
        Err(StoreError::StaleState | StoreError::LeaseLost)
    ));
    let after: Value =
        sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(u) ORDER BY id) FROM usage_reservations u")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        usage, after,
        "rejecting stale output erased accounted fixture use"
    );
    let sent: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE kind='message.sent'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sent, 0);
}
