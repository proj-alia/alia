//! Real TCP/HTTP + disposable PostgreSQL; all fixture content is synthetic.
use alia::{
    adapters::store::Store,
    domain::history::{MemoryKind, NewMemory},
    runtime::Runtime,
    service,
};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

const TOKEN: &str = "p04-source-event-synthetic-owner-token";
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
    fn source(&self, id: Uuid) -> RequestBuilder {
        self.client
            .get(format!("{}/api/v1/source-events/{id}", self.base))
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
async fn assert_metadata(pool: &PgPool, id: Uuid, body: &Value) {
    let expected: Value = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'sequence',sequence,'kind',kind,\
        'source_kind',source_kind,'actor_id',actor_id,'occurred_at',occurred_at,'recorded_at',recorded_at) FROM events WHERE id=$1")
        .bind(id).fetch_one(pool).await.unwrap();
    assert_eq!(body["event"], expected);
    assert_eq!(body.as_object().unwrap().len(), 9);
    assert!(body["state_version"].as_i64().unwrap() > 0);
    assert_eq!(body["privacy_epoch"], 0);
    assert!(chrono::DateTime::parse_from_rfc3339(body["observed_at"].as_str().unwrap()).is_ok());
}

#[sqlx::test]
#[ignore = "P04 source events: isolated PostgreSQL/TCP and total lock required"]
async fn user_generated_and_real_correction_expose_only_actual_text(pool: PgPool) {
    let s = setup(&pool, true).await;
    let user = s
        .store
        .submit_message("source-user", "用户陈述不是世界事实", None)
        .await
        .unwrap();
    let generated = event(
        &pool,
        "message.sent",
        "generated",
        "alia",
        json!({"text":"生成回应不是独立证据", "provider":"FORBIDDEN-SIBLING"}),
    )
    .await;
    let memory = s
        .store
        .record_memories(
            "source-memory",
            s.store.snapshot().await.unwrap().unwrap().expected(),
            &[user.event_id],
            &[NewMemory {
                content: "合成初始记忆".into(),
                scope: "general".into(),
                kind: MemoryKind::Semantic,
                source_event_ids: vec![user.event_id],
                supersedes: None,
            }],
        )
        .await
        .unwrap()
        .memory_ids[0];
    let (status, correction) = send(s.client.post(format!("{}/api/v1/memories/{memory}/revisions",s.base))
        .bearer_auth(TOKEN).header("Idempotency-Key","source-correction").header("If-Match","\"1\"")
        .json(&json!({"content":"实际纠正后的陈述","scope":"general","reason":"FORBIDDEN-CORRECTION-REASON"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{correction}");
    let corrected: Uuid = serde_json::from_value(correction["event_id"].clone()).unwrap();
    let before = unchanged(&pool).await;
    for (id, text, source, actor) in [
        (
            user.event_id,
            "用户陈述不是世界事实",
            "reported_claim",
            "owner",
        ),
        (generated, "生成回应不是独立证据", "generated", "alia"),
        (corrected, "实际纠正后的陈述", "reported_claim", "owner"),
    ] {
        let (status, body) = send(s.source(id)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_metadata(&pool, id, &body).await;
        assert_eq!(body["event"]["source_kind"], source);
        assert_eq!(body["event"]["actor_id"], actor);
        assert_eq!(body["content_status"], "available");
        assert_eq!(
            body["content"],
            json!({"offset":0,"max_chars":2000,"total_chars":text.chars().count(),"text":text,"next_offset":null})
        );
        assert!(!body.to_string().contains("FORBIDDEN"));
        assert!(body["document_id"].is_null());
        if id == user.event_id {
            assert_eq!(body["run_id"], user.operation_id.to_string());
        } else {
            assert!(body["run_id"].is_null());
        }
    }
    assert_eq!(before, unchanged(&pool).await);
}

#[sqlx::test]
#[ignore = "P04 source events: isolated PostgreSQL/TCP and total lock required"]
async fn unicode_large_body_fragments_empty_text_and_eof_are_exact(pool: PgPool) {
    let s = setup(&pool, true).await;
    let text = "甲🙂e\u{301}\n".repeat(1800);
    assert!(text.len() >= 16 * 1024);
    let id = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":text}),
    )
    .await;
    let empty = event(
        &pool,
        "message.sent",
        "generated",
        "alia",
        json!({"text":""}),
    )
    .await;
    let before = unchanged(&pool).await;
    let (_, first) = send(s.source(id)).await;
    assert_eq!(
        first["content"]["text"],
        text.chars().take(2000).collect::<String>()
    );
    assert_eq!(first["content"]["next_offset"], 2000);
    let (_, fragment) = send(s.source(id).query(&[("offset", "1"), ("max_chars", "3")])).await;
    assert_eq!(fragment["content"]["text"], "🙂e\u{301}");
    let mut offset = 0;
    let mut joined = String::new();
    loop {
        let (status, body) = send(
            s.source(id)
                .query(&[("offset", offset.to_string()), ("max_chars", "4096".into())]),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["content"]["total_chars"], text.chars().count());
        joined.push_str(body["content"]["text"].as_str().unwrap());
        let Some(next) = body["content"]["next_offset"].as_u64() else {
            break;
        };
        assert!(next > offset);
        offset = next;
    }
    assert_eq!(joined, text);
    let len = text.chars().count().to_string();
    let (status, eof) = send(s.source(id).query(&[("offset", len.as_str())])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(eof["content"]["text"], "");
    assert!(eof["content"]["next_offset"].is_null());
    for offset in [(text.chars().count() + 1).to_string(), u32::MAX.to_string()] {
        let (status, body) = send(s.source(id).query(&[("offset", offset.as_str())])).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "offset={offset}: {body}"
        );
    }
    let (_, body) = send(s.source(empty)).await;
    assert_eq!(body["content_status"], "available");
    assert_eq!(body["content"]["total_chars"], 0);
    assert_eq!(body["content"]["text"], "");
    assert!(body["content"]["next_offset"].is_null());
    assert_eq!(
        send(s.source(empty).query(&[("offset", "1")])).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(before, unchanged(&pool).await);
}

#[sqlx::test]
#[ignore = "P04 source events: isolated PostgreSQL/TCP and total lock required"]
async fn internal_payload_and_nonstring_text_are_never_dumped(pool: PgPool) {
    let s = setup(&pool, true).await;
    let mut ids = vec![];
    for kind in [
        "model.feedback",
        "content.reviewed",
        "decision.committed",
        "message.drafted",
        "document.imported",
        "future.unknown",
    ] {
        ids.push(event(&pool,kind,"generated","runtime",json!({"text":"FORBIDDEN-TEXT","candidate":{"text":"FORBIDDEN-CANDIDATE"},
            "provider_response":"FORBIDDEN-PROVIDER","hidden_reasoning":"FORBIDDEN-REASONING","credential":"FORBIDDEN-CREDENTIAL",
            "instruction":"FORBIDDEN-INSTRUCTION","arbitrary":"FORBIDDEN-METADATA"})).await);
    }
    for text in [
        Value::Null,
        json!(7),
        json!({"nested":"FORBIDDEN-NESTED"}),
        json!(["FORBIDDEN-ARRAY"]),
    ] {
        ids.push(
            event(
                &pool,
                "message.received",
                "reported_claim",
                "owner",
                json!({"text":text}),
            )
            .await,
        );
    }
    let before = unchanged(&pool).await;
    for id in ids {
        let (status, body) = send(s.source(id).query(&[("max_chars", "4096")])).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_metadata(&pool, id, &body).await;
        assert_eq!(body["content_status"], "not_exposed");
        assert!(body["content"].is_null());
        assert!(body["document_id"].is_null());
        assert!(body["run_id"].is_null());
        assert!(!body.to_string().contains("FORBIDDEN"));
        assert_eq!(
            send(s.source(id).query(&[("offset", "1")])).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(before, unchanged(&pool).await);
}

#[sqlx::test]
#[ignore = "P04 source events: isolated PostgreSQL/TCP and total lock required"]
async fn links_require_real_document_trigger_or_result_and_visible_resources(pool: PgPool) {
    let s = setup(&pool, true).await;
    let imported = s
        .store
        .import_document(
            "source-doc",
            "合成来源资料",
            "FORBIDDEN-DOCUMENT-BODY",
            "合成标签",
        )
        .await
        .unwrap()
        .event_id;
    let user = s
        .store
        .submit_message("source-link-user", "真实触发", None)
        .await
        .unwrap();
    let fake = event(
        &pool,
        "document.imported",
        "external_document",
        "owner",
        json!({"document_id":imported,"run_id":user.operation_id,"text":"FORBIDDEN-FAKE"}),
    )
    .await;
    sqlx::query("UPDATE events SET correlation_id=$2 WHERE id=$1")
        .bind(fake)
        .bind(user.operation_id)
        .execute(&pool)
        .await
        .unwrap();
    let result = event(
        &pool,
        "action.completed",
        "direct_observation",
        "runtime",
        json!({"text":"FORBIDDEN-RESULT"}),
    )
    .await;
    let action = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'action' FROM individuals",
    )
    .bind(action)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments,status,result_event_id) \
        SELECT id,$1,$2,$3,1,'notes.write','1','{}','succeeded',$4 FROM individuals")
        .bind(action).bind(user.operation_id).bind(user.event_id).bind(result).execute(&pool).await.unwrap();
    let before = unchanged(&pool).await;
    let (_, doc) = send(s.source(imported)).await;
    assert_eq!(doc["document_id"], imported.to_string());
    assert_eq!(doc["content_status"], "not_exposed");
    assert!(!doc.to_string().contains("FORBIDDEN"));
    let (_, unproven) = send(s.source(fake)).await;
    assert!(unproven["document_id"].is_null());
    assert!(unproven["run_id"].is_null());
    let (_, linked) = send(s.source(result)).await;
    assert_eq!(linked["run_id"], user.operation_id.to_string());
    assert_eq!(linked["content_status"], "not_exposed");
    assert_eq!(before, unchanged(&pool).await);
    // Blocking an action hides its link; blocking a run hides trigger/result links.
    block(&pool, action, true).await;
    let (_, hidden) = send(s.source(result)).await;
    assert!(hidden["run_id"].is_null());
    block(&pool, action, false).await;
    let ancestor = event(
        &pool,
        "synthetic.ancestor",
        "reported_claim",
        "owner",
        json!({}),
    )
    .await;
    let middle = event(
        &pool,
        "synthetic.middle",
        "reported_claim",
        "owner",
        json!({}),
    )
    .await;
    edge(&pool, ancestor, middle).await;
    edge(&pool, middle, user.operation_id).await;
    block(&pool, ancestor, true).await;
    let before = unchanged(&pool).await;
    for id in [user.event_id, result] {
        let (status, body) = send(s.source(id)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body["run_id"].is_null());
        assert!(!body.to_string().contains(&user.operation_id.to_string()));
    }
    assert_eq!(before, unchanged(&pool).await);
    block(&pool, imported, true).await;
    assert_eq!(send(s.source(imported)).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test]
#[ignore = "P04 source events: isolated PostgreSQL/TCP and total lock required"]
async fn two_generation_event_and_identity_revocation_and_cycles_fail_closed(pool: PgPool) {
    let s = setup(&pool, true).await;
    let first = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"FORBIDDEN-ANCESTOR"}),
    )
    .await;
    let middle = event(&pool, "synthetic.middle", "inference", "alia", json!({})).await;
    let last = event(
        &pool,
        "message.sent",
        "generated",
        "alia",
        json!({"text":"FORBIDDEN-DESCENDANT"}),
    )
    .await;
    let unrelated = event(
        &pool,
        "message.sent",
        "generated",
        "alia",
        json!({"text":"可见独立事件"}),
    )
    .await;
    edge(&pool, first, middle).await;
    edge(&pool, middle, last).await;
    edge(&pool, last, middle).await;
    assert_eq!(send(s.source(last)).await.0, StatusCode::OK);
    block(&pool, first, true).await;
    let before = unchanged(&pool).await;
    for id in [first, middle, last] {
        let (status, body) = send(s.source(id)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(!body.to_string().contains("FORBIDDEN"));
        assert!(!body.to_string().contains(&id.to_string()));
    }
    assert_eq!(send(s.source(unrelated)).await.0, StatusCode::OK);
    assert_eq!(before, unchanged(&pool).await);
    let individual = s.store.snapshot().await.unwrap().unwrap().id;
    block(&pool, individual, true).await;
    let before = unchanged(&pool).await;
    assert_eq!(send(s.source(unrelated)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(before, unchanged(&pool).await);
}

#[sqlx::test]
#[ignore = "P04 source events: isolated PostgreSQL/TCP and total lock required"]
async fn strict_queries_auth_uninitialized_and_missing_objects_have_no_side_effects(pool: PgPool) {
    let s = setup(&pool, false).await;
    let missing = Uuid::new_v4();
    assert_eq!(send(s.source(missing)).await.0, StatusCode::CONFLICT);
    drop(s);
    let s = setup(&pool, true).await;
    let id = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"合成鉴权范围"}),
    )
    .await;
    let before = unchanged(&pool).await;
    for params in [
        vec![("offset", "-1")],
        vec![("offset", "4294967296")],
        vec![("offset", "")],
        vec![("offset", "x")],
        vec![("offset", "1\n")],
        vec![("offset", "1"), ("offset", "2")],
        vec![("max_chars", "1"), ("max_chars", "2")],
        vec![("max_chars", "0")],
        vec![("max_chars", "4097")],
        vec![("max_chars", "-1")],
        vec![("max_chars", "1\0")],
        vec![("unknown", "x")],
        vec![("offset\n", "0")],
    ] {
        let (status, body) = send(s.source(id).query(&params)).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{params:?}: {body}"
        );
    }
    assert_eq!(
        send(
            s.client
                .get(format!("{}/api/v1/source-events/{id}", s.base))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(s.source(id).bearer_auth("wrong-token")).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(s.source(id).header("Origin", "https://untrusted.example"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            s.client
                .get(format!("{}/api/v1/source-events/not-a-uuid", s.base))
                .bearer_auth(TOKEN)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(send(s.source(missing)).await.0, StatusCode::NOT_FOUND);
    let identity = s.store.snapshot().await.unwrap().unwrap().id;
    assert_eq!(send(s.source(identity)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(before, unchanged(&pool).await);
}
