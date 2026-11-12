//! Shared read-path invariants. Synthetic vectors test mechanics, not recall quality.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        config::{PersonaSeed, RuntimeRules},
        history::{MemoryKind, NewMemory},
        retrieval::*,
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        execution::model_step,
        indexing::{IndexEncodingError, MemoryEncoder},
        model::{CallLimits, FixtureProvider},
        scheduling::RunLease,
    },
};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{Notify, Semaphore};
use uuid::Uuid;

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn semantic_primary_resists_generic_keyword_votes_and_keeps_unindexed_candidates(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (old, _) = memory(&s, "原子替换与落盘保证持久性").await;
    index(&s, old, 0).await;
    for i in 0..25 {
        let (id, _) = memory(&s, &format!("文件计划：烹饪学习与休息安排{i}")).await;
        index(&s, id, 1).await;
    }
    let (s, _) = encoded(s, false);
    let mut q = query("文件计划中怎样可靠保存");
    q.limit = 3;
    assert_eq!(
        s.retrieve_memories(&q).await.unwrap().memories()[0].id,
        old,
        "generic keyword presence must not override a stronger semantic match"
    );
    let (new, _) = memory(&s, "文件计划改为先做研究").await;
    q.at = Utc::now();
    q.limit = 10;
    assert!(
        s.retrieve_memories(&q)
            .await
            .unwrap()
            .memories()
            .iter()
            .any(|m| m.id == new),
        "unindexed matching data must remain retrievable"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn explicit_person_filter_separates_near_names_and_mentions(pool: PgPool) {
    use alia::domain::entities::{MemoryPersonAssertion, PersonRole, SourceSpan};
    let s = setup(&pool).await;
    let a = "沈遥喜欢红茶，沈瑶仅被提及。";
    let b = "沈瑶喜欢绿茶，沈遥仅被提及。";
    let (ma, sa) = memory(&s, a).await;
    let (mb, sb) = memory(&s, b).await;
    let span = |event, text: &str, name: &str| {
        let chars: Vec<char> = text.chars().collect();
        let needle: Vec<char> = name.chars().collect();
        let start = chars
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap();
        SourceSpan {
            event_id: event,
            start_char: start,
            end_char: start + needle.len(),
        }
    };
    let ea = s
        .register_person(
            "a",
            s.snapshot().await.unwrap().unwrap().expected(),
            "a",
            "沈遥",
            span(sa, a, "沈遥"),
        )
        .await
        .unwrap()
        .entity_id;
    let eb = s
        .register_person(
            "b",
            s.snapshot().await.unwrap().unwrap().expected(),
            "b",
            "沈瑶",
            span(sb, b, "沈瑶"),
        )
        .await
        .unwrap()
        .entity_id;
    for (m, source, text, name, id, role) in [
        (ma, sa, a, "沈遥", ea, PersonRole::Subject),
        (mb, sb, b, "沈瑶", eb, PersonRole::Subject),
        (mb, sb, b, "沈遥", ea, PersonRole::Mentioned),
    ] {
        let source = span(source, text, name);
        s.assert_memory_person(
            &Uuid::new_v4().to_string(),
            s.snapshot().await.unwrap().unwrap().expected(),
            MemoryPersonAssertion {
                memory_id: m,
                memory_version: 1,
                entity_id: id,
                role,
                source: source.clone(),
                memory_start_char: source.start_char,
            },
        )
        .await
        .unwrap();
    }
    index(&s, ma, 0).await;
    index(&s, mb, 0).await;
    let (s, _) = encoded(s, false);
    let mut q = query("喜欢什么茶");
    q.person_id = Some(ea);
    let response = s.public_retrieval(&q).await.unwrap();
    assert_eq!(response["items"].as_array().unwrap().len(), 1);
    assert_eq!(response["items"][0]["id"], ma.to_string());
    assert_eq!(
        response["items"][0]["person_assertions"][0]["role"],
        "subject"
    );
    q.person_id = Some(eb);
    assert_eq!(s.retrieve_memories(&q).await.unwrap().memories()[0].id, mb);
    q.person_id = Some(Uuid::new_v4());
    assert!(matches!(
        s.retrieve_memories(&q).await,
        Err(StoreError::NotFound)
    ));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn api_is_authenticated_strict_ranked_read_with_metadata_and_source_projection(pool: PgPool) {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let s = setup(&pool).await;
    let (id, _) = memory(&s, "tea preparation").await;
    index(&s, id, 0).await;
    let (s, _) = encoded(s, false);
    let token = "test-isolated-retrieval-token-long-enough";
    let mut rt = alia::runtime::Runtime::new(pool, token).unwrap();
    rt.store = s.clone();
    rt.indexing_enabled = true;
    for (path, auth, expected) in [
        ("/api/v1/memories/retrieve?q=tea", false, 401),
        ("/api/v1/memories/retrieve?q=tea", true, 200),
        ("/api/v1/memories/retrieve?q=tea&limit=0", true, 422),
        ("/api/v1/memories/retrieve?q=tea&unexpected=true", true, 422),
        (
            "/api/v1/memories/retrieve?q=tea&person_id=near-name",
            true,
            422,
        ),
    ] {
        let mut r = Request::builder().uri(path).header("host", "localhost");
        if auth {
            r = r.header("authorization", format!("Bearer {token}"));
        }
        let response = alia::service::router(rt.clone(), "web/dist".into())
            .oneshot(r.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        if expected == 200 {
            assert_eq!(body["retrieval"]["mode"], "hybrid");
            assert_eq!(body["retrieval"]["indexed"], 1);
            assert_eq!(body["items"][0]["id"], id.to_string());
            assert!(body["items"][0]["person_assertions"].is_array());
            assert_eq!(body["exhaustive"], false);
        }
    }
    assert_eq!(
        s.public_retrieval(&query("tea")).await.unwrap()["state_version"],
        s.snapshot().await.unwrap().unwrap().state_version
    );
}

fn vector(axis: usize) -> Vec<f32> {
    let mut v = vec![0.; DIMENSION];
    v[axis] = 1.;
    v
}
struct Encoder {
    calls: Arc<AtomicUsize>,
    fail: bool,
    gate: Option<(Arc<Notify>, Arc<Semaphore>)>,
}
#[async_trait::async_trait]
impl MemoryEncoder for Encoder {
    async fn encode_memory(&self, _: String) -> Result<Vec<IndexedChunk>, IndexEncodingError> {
        Err(IndexEncodingError::Unavailable)
    }
    async fn encode_query(&self, _: String) -> Result<Vec<f32>, IndexEncodingError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some((entered, gate)) = &self.gate {
            entered.notify_one();
            let _permit = gate.acquire().await.unwrap();
        }
        if self.fail {
            Err(IndexEncodingError::Unavailable)
        } else {
            Ok(vector(0))
        }
    }
}
fn encoded(s: Store, fail: bool) -> (Store, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        s.with_memory_encoder(Arc::new(Encoder {
            calls: calls.clone(),
            fail,
            gate: None,
        })),
        calls,
    )
}
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "init",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap(),
        &RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    s
}
async fn memory(s: &Store, text: &str) -> (Uuid, Uuid) {
    let source = s
        .submit_message(&Uuid::new_v4().to_string(), text, None)
        .await
        .unwrap()
        .event_id;
    let id = s
        .record_memories(
            &Uuid::new_v4().to_string(),
            s.snapshot().await.unwrap().unwrap().expected(),
            &[source],
            &[NewMemory {
                content: text.into(),
                kind: MemoryKind::Semantic,
                scope: "general".into(),
                source_event_ids: vec![source],
                supersedes: None,
            }],
        )
        .await
        .unwrap()
        .memory_ids[0];
    (id, source)
}
async fn index(s: &Store, id: Uuid, axis: usize) {
    let snap = s.prepare_memory_index(id).await.unwrap();
    s.commit_memory_index(
        &snap,
        &[IndexedChunk {
            start_char: 0,
            end_char: snap.content().chars().count(),
            vector: vector(axis),
        }],
    )
    .await
    .unwrap();
}
fn query(text: &str) -> RetrievalRequest {
    RetrievalRequest {
        text: text.into(),
        scope: None,
        subject_id: None,
        person_id: None,
        person_name: None,
        at: Utc::now(),
        include_history: false,
        limit: 10,
    }
}
async fn intent(s: &Store, input: Value) -> (RunLease, Uuid) {
    s.submit_message(&Uuid::new_v4().to_string(), "查找过去的经历", None)
        .await
        .unwrap();
    let mut lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let caps = s.local_capabilities().await.unwrap();
    let p = json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[{"capability_id":"memory.search","input":input}],"development":null,"brief_reason":"检索有来源的认识"});
    let ready = model_step(
        s,
        &FixtureProvider {
            response: p.to_string(),
        },
        &lease,
        caps.clone(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    let r = s.commit_decision(&lease, &ready, &caps).await.unwrap();
    lease.state_version = r.state_version;
    (lease, r.action_ids[0])
}
async fn payload(pool: &PgPool, id: Uuid) -> Value {
    sqlx::query_scalar("SELECT payload FROM events WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn hybrid_finds_old_semantic_match_and_exposes_missing_index(pool: PgPool) {
    let s = setup(&pool).await;
    let (old, _) = memory(&s, "原子替换与落盘保证持久性").await;
    index(&s, old, 0).await;
    for i in 0..25 {
        let (id, _) = memory(&s, &format!("烹饪菜谱编号{i}")).await;
        index(&s, id, 1).await;
    }
    let (s, calls) = encoded(s, false);
    let q = query("怎样安全保存文件？");
    let page = s.retrieve_memories(&q).await.unwrap();
    assert_eq!(page.memories()[0].id, old);
    assert_eq!(page.summary().mode, "hybrid");
    assert_eq!((page.summary().eligible, page.summary().indexed), (26, 26));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let (new, _) = memory(&s, "我希望先研究再实现原型").await;
    let page = s
        .retrieve_memories(&query("先研究再实现原型"))
        .await
        .unwrap();
    assert!(page.memories().iter().any(|m| m.id == new));
    assert_eq!((page.summary().eligible, page.summary().indexed), (27, 26));
    let lexical = Store::new(pool.clone())
        .retrieve_memories(&q)
        .await
        .unwrap();
    assert_eq!(lexical.summary().mode, "lexical");
    assert!(lexical.summary().profile.is_none());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn scope_time_source_and_index_generation_are_applied_before_ranking(pool: PgPool) {
    let s = setup(&pool).await;
    let (old, _) = memory(&s, "旧的一般认识").await;
    let (local, source) = memory(&s, "这次先写原型").await;
    sqlx::query("UPDATE memories SET validity='superseded',valid_from='2026-01-01',valid_until='2026-06-01' WHERE id=$1").bind(old).execute(&pool).await.unwrap();
    sqlx::query("UPDATE memories SET scope='event:local',valid_from='2026-06-01' WHERE id=$1")
        .bind(local)
        .execute(&pool)
        .await
        .unwrap();
    index(&s, old, 0).await;
    index(&s, local, 0).await;
    let (s, _) = encoded(s, false);
    assert!(
        s.retrieve_memories(&query("过去认识"))
            .await
            .unwrap()
            .memories()
            .is_empty()
    );
    let mut q = query("过去认识");
    q.at = "2026-03-01T00:00:00Z".parse().unwrap();
    q.include_history = true;
    assert_eq!(s.retrieve_memories(&q).await.unwrap().memories()[0].id, old);
    q = query("原型");
    q.scope = Some("event:local".into());
    assert_eq!(
        s.retrieve_memories(&q).await.unwrap().memories()[0].id,
        local
    );
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(s.retrieve_memories(&q).await.unwrap().summary().indexed, 0);
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        s.public_retrieval(&q).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn encoder_wait_holds_no_transaction_and_new_input_fences_old_selection(pool: PgPool) {
    let s = setup(&pool).await;
    memory(&s, "有来源的记忆").await;
    let single = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let entered = Arc::new(Notify::new());
    let gate = Arc::new(Semaphore::new(0));
    let waiting = Store::new(single.clone()).with_memory_encoder(Arc::new(Encoder {
        calls: Arc::new(AtomicUsize::new(0)),
        fail: false,
        gate: Some((entered.clone(), gate.clone())),
    }));
    let task = tokio::spawn(async move { waiting.public_retrieval(&query("记忆")).await });
    entered.notified().await;
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        Store::new(single.clone()).submit_message("foreground", "新输入优先", None),
    )
    .await
    .unwrap()
    .unwrap();
    gate.add_permits(1);
    assert!(matches!(task.await.unwrap(), Err(StoreError::StaleState)));
    single.close().await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn context_and_tool_use_shared_selection_and_replay_has_no_second_encoding(pool: PgPool) {
    let s = setup(&pool).await;
    let (id, source) = memory(&s, "共同研究过原子替换与落盘").await;
    index(&s, id, 0).await;
    let (s, calls) = encoded(s, false);
    let (mut lease, action) = intent(&s, json!({"query":"可靠保存文档"})).await;
    let ctx = s.model_context(lease.run_id, vec![]).await.unwrap();
    assert_eq!(ctx.retrieval.as_ref().unwrap().mode, "hybrid");
    assert!(ctx.memories.iter().any(|m| m.id == id));
    assert!(ctx.input_manifest.contains(&id) && ctx.input_manifest.contains(&source));
    let before = calls.load(Ordering::SeqCst);
    let r = s.execute_local_action(&lease, action).await.unwrap();
    assert_eq!(r.status, "succeeded");
    let result = payload(&pool, r.result_event_id).await;
    assert_eq!(result["output"]["retrieval"], "hybrid");
    assert_eq!(result["output"]["items"][0]["id"], id.to_string());
    assert_eq!(result["status"], "succeeded");
    lease.state_version = r.state_version;
    assert_eq!(s.execute_local_action(&lease, action).await.unwrap(), r);
    assert_eq!(calls.load(Ordering::SeqCst), before + 1);
    let count: i32 = sqlx::query_scalar("SELECT tool_calls FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let ancestor = s
        .import_document("revoked-parent", "父来源", "合成撤销测试", "隔离故障注入")
        .await
        .unwrap();
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES($1,$2,$3)")
        .bind(lease.individual_id)
        .bind(ancestor.event_id)
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(ancestor.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.execute_local_action(&lease, action).await,
        Err(StoreError::StaleState)
    ));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn encoder_failure_is_failed_tool_result_counted_once_not_empty_success(pool: PgPool) {
    let s = setup(&pool).await;
    memory(&s, "已有真实记忆").await;
    let (mut lease, action) = intent(&s, json!({"query":"真实记忆"})).await;
    let (s, calls) = encoded(s, true);
    let r = s.execute_local_action(&lease, action).await.unwrap();
    assert_eq!(r.status, "failed");
    assert!(r.executed);
    let result = payload(&pool, r.result_event_id).await;
    assert_eq!(result["output"]["error"], "retrieval_unavailable");
    assert!(result["output"].get("items").is_none());
    lease.state_version = r.state_version;
    assert_eq!(s.execute_local_action(&lease, action).await.unwrap(), r);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        s.retrieve_memories(&query("真实记忆")).await,
        Err(StoreError::RetrievalUnavailable)
    ));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn action_argument_tamper_during_encoding_cannot_commit_another_query(pool: PgPool) {
    let s = setup(&pool).await;
    let (lease, action) = intent(&s, json!({"query":"原查询"})).await;
    let entered = Arc::new(Notify::new());
    let gate = Arc::new(Semaphore::new(0));
    let s = s.with_memory_encoder(Arc::new(Encoder {
        calls: Arc::new(AtomicUsize::new(0)),
        fail: false,
        gate: Some((entered.clone(), gate.clone())),
    }));
    let task = tokio::spawn(async move { s.execute_local_action(&lease, action).await });
    entered.notified().await;
    sqlx::query("UPDATE actions SET arguments=$2 WHERE id=$1")
        .bind(action)
        .bind(json!({"query":"另一查询"}))
        .execute(&pool)
        .await
        .unwrap();
    gate.add_permits(1);
    assert!(matches!(task.await.unwrap(), Err(StoreError::StaleState)));
    let(status,result,calls):(String,Option<Uuid>,i32)=sqlx::query_as("SELECT a.status,a.result_event_id,r.tool_calls FROM actions a JOIN runs r ON r.id=a.run_id WHERE a.id=$1").bind(action).fetch_one(&pool).await.unwrap();
    assert_eq!((status, result, calls), ("planned".into(), None, 0));
}

fn name_span(event: Uuid, text: &str, name: &str) -> alia::domain::entities::SourceSpan {
    let start = text[..text.find(name).unwrap()].chars().count();
    alia::domain::entities::SourceSpan {
        event_id: event,
        start_char: start,
        end_char: start + name.chars().count(),
    }
}
async fn person(s: &Store, name: &str) -> Uuid {
    let source = s
        .submit_message(&Uuid::new_v4().to_string(), name, None)
        .await
        .unwrap()
        .event_id;
    s.register_person(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        &Uuid::new_v4().to_string(),
        name,
        name_span(source, name, name),
    )
    .await
    .unwrap()
    .entity_id
}
async fn alias(s: &Store, person: Uuid, name: &str) -> (Uuid, Uuid) {
    let source = s
        .submit_message(&Uuid::new_v4().to_string(), name, None)
        .await
        .unwrap()
        .event_id;
    let receipt = s
        .add_person_alias(
            &Uuid::new_v4().to_string(),
            s.snapshot().await.unwrap().unwrap().expected(),
            person,
            name_span(source, name, name),
        )
        .await
        .unwrap();
    (source, receipt.event_id)
}
async fn annotate_person(
    s: &Store,
    memory: Uuid,
    source: Uuid,
    text: &str,
    name: &str,
    id: Uuid,
    role: alia::domain::entities::PersonRole,
) -> Uuid {
    let span = name_span(source, text, name);
    s.assert_memory_person(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        alia::domain::entities::MemoryPersonAssertion {
            memory_id: memory,
            memory_version: 1,
            entity_id: id,
            role,
            memory_start_char: span.start_char,
            source: span,
        },
    )
    .await
    .unwrap()
    .event_id
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn exact_name_and_alias_qualification_never_merge_near_or_unknown_names(pool: PgPool) {
    use alia::domain::entities::PersonRole;
    let s = setup(&pool).await;
    let a = person(&s, "钟澄").await;
    let b = person(&s, "钟橙").await;
    let ta = "钟澄喜欢陶艺；钟橙只是旁听者。";
    let tb = "钟橙喜欢摄影；钟澄只是被提及。";
    let (ma, sa) = memory(&s, ta).await;
    let (mb, sb) = memory(&s, tb).await;
    annotate_person(&s, ma, sa, ta, "钟澄", a, PersonRole::Subject).await;
    annotate_person(&s, mb, sb, tb, "钟橙", b, PersonRole::Subject).await;
    annotate_person(&s, mb, sb, tb, "钟澄", a, PersonRole::Mentioned).await;
    alias(&s, a, "澄澄").await;
    index(&s, ma, 0).await;
    index(&s, mb, 0).await;
    // The same source policy must hold with and without a local encoder.
    for s in [s.clone(), encoded(s.clone(), false).0] {
        for name in ["钟澄", "澄澄"] {
            let mut q = query("他喜欢的活动");
            q.person_name = Some(name.into());
            let page = s.retrieve_memories(&q).await.unwrap();
            assert_eq!(page.summary().person_filter, "exact_alias");
            assert_eq!(page.summary().person_id, Some(a));
            assert_eq!(page.memories().len(), 1);
            assert_eq!(page.memories()[0].id, ma);
            assert_eq!(page.memories()[0].person_assertions[0].entity_id, a);
        }
        let mut unknown = query("喜欢什么");
        unknown.person_name = Some("钟澄宇".into());
        assert!(matches!(
            s.retrieve_memories(&unknown).await,
            Err(StoreError::NotFound)
        ));
        unknown.person_id = Some(a);
        assert!(matches!(
            s.retrieve_memories(&unknown).await,
            Err(StoreError::Invalid(_))
        ));
    }
    alias(&s, a, "小钟").await;
    alias(&s, b, "小钟").await;
    let mut q = query("喜欢什么");
    q.person_name = Some("小钟".into());
    assert!(matches!(
        s.retrieve_memories(&q).await,
        Err(StoreError::PersonAmbiguous)
    ));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn alias_only_empty_result_retains_resolution_provenance_and_cached_revocation(pool: PgPool) {
    let s = setup(&pool).await;
    let p = person(&s, "赵南").await;
    let (source, assertion) = alias(&s, p, "南南").await;
    let (s, calls) = encoded(s, false);
    let (lease, action) = intent(&s, json!({"query":"南南的兴趣","person_name":"南南"})).await;
    let first = s.execute_local_action(&lease, action).await.unwrap();
    let before = calls.load(Ordering::SeqCst);
    let output = payload(&pool, first.result_event_id).await;
    assert_eq!(output["status"], "succeeded");
    assert_eq!(output["output"]["items"], json!([]));
    assert_eq!(
        output["output"]["retrieval_details"]["person_id"],
        p.to_string()
    );
    for id in [p, source, assertion] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM derivation_edges WHERE source_id=$1 AND target_id=$2)",
        )
        .bind(id)
        .bind(first.result_event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            exists,
            "even an empty result must depend on the identity resolution"
        );
    }
    assert_eq!(
        s.execute_local_action(&lease, action)
            .await
            .unwrap()
            .result_event_id,
        first.result_event_id
    );
    assert_eq!(calls.load(Ordering::SeqCst), before);
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.execute_local_action(&lease, action).await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), before);
    let count: i32 = sqlx::query_scalar("SELECT tool_calls FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn memory_roles_reach_model_context_and_revoked_role_cannot_commit(pool: PgPool) {
    use alia::domain::entities::PersonRole;
    let s = setup(&pool).await;
    let a = person(&s, "许安").await;
    let b = person(&s, "徐安").await;
    let text = "许安喜欢木工，徐安只是被提及。";
    let (m, source) = memory(&s, text).await;
    let assertion = annotate_person(&s, m, source, text, "许安", a, PersonRole::Subject).await;
    annotate_person(&s, m, source, text, "徐安", b, PersonRole::Mentioned).await;
    index(&s, m, 0).await;
    let (s, _) = encoded(s, false);
    s.submit_message("ask", "许安喜欢什么活动？", None)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let caps = s.local_capabilities().await.unwrap();
    let context = s.model_context(lease.run_id, caps.clone()).await.unwrap();
    let memory = context.memories.iter().find(|row| row.id == m).unwrap();
    assert!(
        memory
            .person_assertions
            .iter()
            .any(|r| r.entity_id == a && matches!(r.role, PersonRole::Subject))
    );
    assert!(
        memory
            .person_assertions
            .iter()
            .any(|r| r.entity_id == b && matches!(r.role, PersonRole::Mentioned))
    );
    assert!(!memory.person_assertions_more);
    for id in [a, b, source, assertion] {
        assert!(context.input_manifest.contains(&id));
    }
    let ready=model_step(&s,&FixtureProvider {response:json!({"schema_version":1,"reply":"记录中许安喜欢木工，徐安只是被提及。","reply_evidence_ids":[m],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"依据已保存主体标注"}).to_string()},&lease,caps.clone(),CallLimits::default()).await.unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(assertion)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.commit_decision(&lease, &ready, &caps).await,
        Err(StoreError::StaleState | StoreError::LeaseLost)
    ));
    let replies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM events WHERE kind='message.sent' AND correlation_id=$1",
    )
    .bind(lease.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        replies, 0,
        "a revoked assertion must not produce an outgoing reply"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn ambiguous_alias_is_explicit_tool_failure_and_http_error_not_empty_success(pool: PgPool) {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let s = setup(&pool).await;
    let a = person(&s, "李沙").await;
    let b = person(&s, "李莎").await;
    alias(&s, a, "小李").await;
    alias(&s, b, "小李").await;
    let (lease, action) = intent(&s, json!({"query":"小李的兴趣","person_name":"小李"})).await;
    let result = s.execute_local_action(&lease, action).await.unwrap();
    let value = payload(&pool, result.result_event_id).await;
    assert_eq!(value["status"], "failed");
    assert_eq!(value["executed"], true);
    assert_eq!(value["output"]["error"], "person_ambiguous");
    assert!(value["output"].get("items").is_none());
    assert_eq!(
        s.execute_local_action(&lease, action)
            .await
            .unwrap()
            .result_event_id,
        result.result_event_id
    );
    let token = "test-person-query-access-token-long-enough";
    let mut rt = alia::runtime::Runtime::new(pool.clone(), token).unwrap();
    rt.store = s;
    let url = reqwest::Url::parse_with_params(
        "http://localhost/api/v1/memories/retrieve",
        &[("q", "兴趣"), ("person_name", "小李")],
    )
    .unwrap();
    let request = Request::builder()
        .uri(url.as_str())
        .header("host", "localhost")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = alia::service::router(rt, "web/dist".into())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 422);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["error"]["code"], "person_ambiguous");
    assert!(!body.to_string().contains(&a.to_string()));
    assert!(!body.to_string().contains(&b.to_string()));
    let count: i32 = sqlx::query_scalar("SELECT tool_calls FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn exact_name_discovery_is_not_limited_to_the_model_catalogue(pool: PgPool) {
    let s = setup(&pool).await;
    let mut names = Vec::new();
    for n in 0..23 {
        let name = format!("成员{n:02}");
        let id = person(&s, &name).await;
        names.push((id, name));
    }
    names.sort_by_key(|(id, _)| *id);
    let (target, name) = names.last().unwrap();
    s.submit_message("outside-catalogue", "核对成员的资料", None)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let context = s
        .model_context(lease.run_id, s.local_capabilities().await.unwrap())
        .await
        .unwrap();
    assert!(context.person_catalog.has_more);
    assert_eq!(context.person_catalog.items.len(), 20);
    assert!(!context.person_catalog.items.iter().any(|p| p.id == *target));
    let mut q = query("已知成员的活动");
    q.person_name = Some(name.clone());
    let page = s.public_retrieval(&q).await.unwrap();
    assert_eq!(page["retrieval"]["person_id"], target.to_string());
    assert_eq!(page["retrieval"]["person_filter"], "exact_alias");
    assert_eq!(page["items"], json!([]));
}
