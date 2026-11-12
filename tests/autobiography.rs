//! Actual isolated PostgreSQL, trusted APIs and explicit synthetic source fixtures.
//! No provider, HTTP mutation, model learning or production initialization.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        autobiography::AutobiographyDraft,
        beliefs::{BeliefDraft, BeliefPosition},
        config::{PersonaSeed, RuntimeRules},
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "auto-init",
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
async fn message(s: &Store, text: &str) -> Uuid {
    s.submit_message(&Uuid::new_v4().to_string(), text, None)
        .await
        .unwrap()
        .event_id
}
fn draft(topic: &str, event: Uuid) -> AutobiographyDraft {
    AutobiographyDraft {
        topic: topic.into(),
        title: "合成经历的当前理解".into(),
        interpretation: "这是一种有来源的解释，不是对世界事实或共同经历的认证。".into(),
        event_ids: vec![event],
        belief_ids: vec![],
        supersedes: None,
    }
}
async fn record(
    s: &Store,
    manifest: &[Uuid],
    d: &AutobiographyDraft,
) -> alia::adapters::autobiography::AutobiographyReceipt {
    s.record_autobiography(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        manifest,
        d,
    )
    .await
    .unwrap()
}
async fn event(pool: &PgPool, kind: &str, source: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'event' FROM individuals")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,$1,$2,$3,'synthetic-fixture',clock_timestamp(),'{\"text\":\"合成来源\"}',$1 FROM individuals")
  .bind(id).bind(kind).bind(source).execute(pool).await.unwrap();
    id
}
async fn stance(s: &Store, support: Uuid, counter: Option<Uuid>, previous: Option<Uuid>) -> Uuid {
    let d = BeliefDraft {
        statement: "我对这段历史持保留理解".into(),
        scope: "general".into(),
        position: BeliefPosition::Questioned,
        support_event_ids: vec![support],
        counter_event_ids: counter.into_iter().collect(),
        supersedes: previous,
        brief_basis: "同时保留支持、反证和旧来源".into(),
    };
    let mut inputs = vec![support];
    inputs.extend(counter);
    inputs.extend(previous);
    s.record_belief(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        &inputs,
        &d,
    )
    .await
    .unwrap()
    .belief_id
}
async fn block(pool: &PgPool, id: Uuid, blocked: bool) {
    sqlx::query("UPDATE objects SET blocked=$2 WHERE id=$1")
        .bind(id)
        .bind(blocked)
        .execute(pool)
        .await
        .unwrap();
}
async fn edge(pool: &PgPool, source: Uuid, target: Uuid) {
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT id,$1,$2 FROM individuals ON CONFLICT DO NOTHING").bind(source).bind(target).execute(pool).await.unwrap();
}
async fn fingerprint(pool: &PgPool) -> Value {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut result = serde_json::Map::new();
    for table in tables {
        assert!(table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        let digest:String=sqlx::query_scalar(&format!("SELECT md5(COALESCE(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM \"{table}\" t")).fetch_one(pool).await.unwrap();
        result.insert(table, json!(digest));
    }
    Value::Object(result)
}
fn items(v: &Value) -> &[Value] {
    v["items"].as_array().unwrap()
}
fn id(v: &Value) -> Uuid {
    serde_json::from_value(v["id"].clone()).unwrap()
}

#[sqlx::test]
#[ignore = "P04 autobiography: isolated PostgreSQL and total lock required"]
async fn formation_restart_revision_preserve_original_events_and_interpretation_history(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let user = message(&s, "我曾这样描述这次探索，尚非独立核实事实").await;
    let generated = event(&pool, "message.sent", "generated").await;
    let imported = s
        .import_document(
            "auto-doc",
            "合成资料",
            "记录资料已导入，不是已阅读",
            "合成外部来源",
        )
        .await
        .unwrap()
        .event_id;
    let originals: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=ANY($1) ORDER BY id")
            .bind(vec![user, generated, imported])
            .fetch_all(&pool)
            .await
            .unwrap();
    let mut old = draft("learning.first-project", user);
    old.title = "起初的理解".into();
    old.interpretation = "当时我把已有陈述理解为一种通常期待。".into();
    old.event_ids = vec![user, generated, imported];
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let receipt = s
        .record_autobiography("auto-first", expected, &old.event_ids, &old)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&receipt)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(receipt.version, expected.version + 1);
    assert_eq!(receipt.autobiography_version, 1);
    assert!(receipt.previous_autobiography_id.is_none());
    let newer = message(&s, "后来的反例使之前的理解需要改变").await;
    let mut next = draft(&old.topic, newer);
    next.title = "重新理解".into();
    next.interpretation = "后来的实际输入改变了我对那段历史的解释，原始陈述仍保留。".into();
    next.supersedes = Some(receipt.autobiography_id);
    let before = s.snapshot().await.unwrap().unwrap().expected();
    let inputs = [receipt.autobiography_id, newer];
    let revised = s
        .record_autobiography("auto-revise", before, &inputs, &next)
        .await
        .unwrap();
    assert_ne!(receipt.autobiography_id, revised.autobiography_id);
    assert_eq!(
        revised.previous_autobiography_id,
        Some(receipt.autobiography_id)
    );
    assert_eq!(
        s.record_autobiography("auto-revise", before, &inputs, &next)
            .await
            .unwrap(),
        revised
    );
    let restarted = Store::new(pool.clone());
    let read_before = fingerprint(&pool).await;
    let history = restarted.autobiography(true, 30).await.unwrap();
    assert_eq!(history.as_object().unwrap().len(), 6);
    assert_eq!(items(&history).len(), 2);
    assert_eq!(id(&items(&history)[0]), revised.autobiography_id);
    assert_eq!(items(&history)[0]["interpretation"], next.interpretation);
    assert_eq!(
        items(&history)[0]["supersedes"],
        receipt.autobiography_id.to_string()
    );
    let previous = &items(&history)[1];
    assert_eq!(previous["title"], old.title);
    assert_eq!(previous["interpretation"], old.interpretation);
    assert_eq!(previous["version"], 2);
    assert_eq!(previous["status"], "superseded");
    assert!(previous["superseded_at"].is_string());
    assert_eq!(previous.as_object().unwrap().len(), 11);
    let mut actual: Vec<Uuid> = serde_json::from_value(previous["event_ids"].clone()).unwrap();
    actual.sort();
    let mut expected = old.event_ids.clone();
    expected.sort();
    assert_eq!(actual, expected);
    let current = restarted.autobiography(false, 30).await.unwrap();
    assert_eq!(items(&current).len(), 1);
    assert_eq!(id(&items(&current)[0]), revised.autobiography_id);
    assert_eq!(fingerprint(&pool).await, read_before);
    let after: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=ANY($1) ORDER BY id")
            .bind(vec![user, generated, imported])
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(after, originals);
    let events:Vec<(String,String,String)>=sqlx::query_as("SELECT kind,source_kind,actor_id FROM events WHERE kind LIKE 'autobiography.%' ORDER BY sequence").fetch_all(&pool).await.unwrap();
    assert_eq!(
        events,
        vec![
            (
                "autobiography.formed".into(),
                "inference".into(),
                "individual".into()
            ),
            (
                "autobiography.revised".into(),
                "inference".into(),
                "individual".into()
            )
        ]
    );
}

#[sqlx::test]
#[ignore = "P04 autobiography: isolated PostgreSQL and total lock required"]
async fn topic_predecessor_ownership_manifest_version_and_idempotency_are_atomic(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s, "合成实际输入").await;
    let d = draft("project.main", e);
    let first = record(&s, &[e], &d).await;
    let before = fingerprint(&pool).await;
    let mut wrong_topic = d.clone();
    wrong_topic.topic = "project.other".into();
    wrong_topic.supersedes = Some(first.autobiography_id);
    let mut missing = d.clone();
    missing.topic = "project.missing".into();
    missing.event_ids = vec![Uuid::new_v4()];
    for (body, manifest) in [
        (d.clone(), vec![e]),
        (wrong_topic, vec![e, first.autobiography_id]),
        (missing, vec![e]),
    ] {
        assert!(matches!(
            s.record_autobiography(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &manifest,
                &body
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
    }
    for manifest in [vec![], vec![e, e], vec![Uuid::nil()]] {
        assert!(matches!(
            s.record_autobiography(
                "bad-manifest",
                s.snapshot().await.unwrap().unwrap().expected(),
                &manifest,
                &draft("project.x", e)
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
    }
    let foreign = Uuid::new_v4();
    assert!(matches!(
        s.record_autobiography(
            "foreign",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[foreign],
            &draft("project.x", foreign)
        )
        .await,
        Err(StoreError::NotFound)
    ));
    let error = sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'event')")
        .bind(Uuid::new_v4())
        .bind(foreign)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    assert_eq!(fingerprint(&pool).await, before);
    let mut revised = d.clone();
    revised.supersedes = Some(first.autobiography_id);
    revised.interpretation = "这次保留前身并修订当前解释。".into();
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let inputs = [e, first.autobiography_id];
    let next = s
        .record_autobiography("same-intent", expected, &inputs, &revised)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(
        s.record_autobiography("same-intent", expected, &inputs, &revised)
            .await
            .unwrap(),
        next
    );
    let mut changed = revised.clone();
    changed.interpretation = "不同请求不能复用同一幂等意图".into();
    assert!(matches!(
        s.record_autobiography("same-intent", expected, &inputs, &changed)
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert!(matches!(
        s.record_autobiography("stale-state", expected, &inputs, &revised)
            .await,
        Err(StoreError::StaleState)
    ));
    assert!(matches!(
        s.record_autobiography(
            "obsolete-predecessor",
            s.snapshot().await.unwrap().unwrap().expected(),
            &inputs,
            &revised
        )
        .await,
        Err(StoreError::Invalid(_))
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04 autobiography: isolated PostgreSQL and total lock required"]
async fn linked_stances_need_complete_counter_and_predecessor_basis_and_actual_experience(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let old_event = message(&s, "早先的实际输入").await;
    let previous = stance(&s, old_event, None, None).await;
    let counter = message(&s, "真实反证").await;
    let support = message(&s, "后续实际输入").await;
    let current = stance(&s, support, Some(counter), Some(previous)).await;
    let mut d = draft("learning.stance", support);
    d.belief_ids = vec![current];
    let complete = [support, counter, old_event, previous, current];
    let before = fingerprint(&pool).await;
    for manifest in [
        vec![support, current],
        vec![support, current, counter],
        vec![support, current, previous, old_event],
    ] {
        assert!(matches!(
            s.record_autobiography(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &manifest,
                &d
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
    }
    let mut stance_only = d.clone();
    stance_only.event_ids.clear();
    assert!(matches!(
        s.record_autobiography(
            "stance-only",
            s.snapshot().await.unwrap().unwrap().expected(),
            &complete,
            &stance_only
        )
        .await,
        Err(StoreError::Invalid(_))
    ));
    let mut event_is_stance = d.clone();
    event_is_stance.event_ids = vec![current];
    assert!(matches!(
        s.record_autobiography(
            "wrong-kind",
            s.snapshot().await.unwrap().unwrap().expected(),
            &complete,
            &event_is_stance
        )
        .await,
        Err(StoreError::Invalid(_))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let r = record(&s, &complete, &d).await;
    let view = s.autobiography(false, 30).await.unwrap();
    assert_eq!(items(&view)[0]["belief_ids"], json!([current]));
    assert_eq!(items(&view)[0]["event_ids"], json!([support]));
    assert_eq!(id(&items(&view)[0]), r.autobiography_id);
    for (kind, source) in [
        ("seed.background", "fictional_background"),
        ("decision.reviewed", "inference"),
    ] {
        let forbidden = event(&pool, kind, source).await;
        let before = fingerprint(&pool).await;
        assert!(matches!(
            s.record_autobiography(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[forbidden],
                &draft("forbidden.source", forbidden)
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            s.record_autobiography(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[support, forbidden],
                &draft("forbidden.extra", support)
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
        assert_eq!(fingerprint(&pool).await, before);
    }
    // A malformed saved stance cannot turn reviewer feedback into actual basis.
    let feedback = event(&pool, "decision.reviewed", "inference").await;
    let bad = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'belief' FROM individuals",
    )
    .bind(bad)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO beliefs(individual_id,id,statement,scope,position,brief_basis) SELECT id,$1,'合成非法来源认识','general','questioned','夹具反例' FROM individuals").bind(bad).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO belief_evidence(individual_id,belief_id,event_id,role) SELECT id,$1,$2,'counter' FROM individuals").bind(bad).bind(feedback).execute(&pool).await.unwrap();
    let mut body = draft("forbidden.stance", support);
    body.belief_ids = vec![bad];
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.record_autobiography(
            "bad-stance",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[support, bad, feedback],
            &body
        )
        .await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04 autobiography: isolated PostgreSQL and total lock required"]
async fn missing_edges_still_block_revoked_sources_stances_predecessors_and_replays(pool: PgPool) {
    let s = setup(&pool).await;
    let ancestor = message(&s, "私有源头").await;
    let middle = event(&pool, "message.received", "reported_claim").await;
    let leaf = event(&pool, "message.received", "reported_claim").await;
    edge(&pool, ancestor, middle).await;
    edge(&pool, middle, leaf).await;
    let counter = message(&s, "保留的反证").await;
    let b = stance(&s, leaf, Some(counter), None).await;
    let mut old = draft("private.history", leaf);
    old.belief_ids = vec![b];
    let old_expected = s.snapshot().await.unwrap().unwrap().expected();
    let old_inputs = [leaf, counter, b];
    let first = s
        .record_autobiography("private-first", old_expected, &old_inputs, &old)
        .await
        .unwrap();
    let safe = message(&s, "独立公开输入").await;
    let mut next = draft(&old.topic, safe);
    next.supersedes = Some(first.autobiography_id);
    let next_expected = s.snapshot().await.unwrap().unwrap().expected();
    let next_inputs = [safe, first.autobiography_id];
    let second = s
        .record_autobiography("private-next", next_expected, &next_inputs, &next)
        .await
        .unwrap();
    let independent = record(&s, &[safe], &draft("independent.topic", safe)).await;
    sqlx::query("DELETE FROM derivation_edges WHERE target_id=ANY($1)")
        .bind(vec![
            b,
            first.autobiography_id,
            first.event_id,
            second.autobiography_id,
            second.event_id,
        ])
        .execute(&pool)
        .await
        .unwrap();
    for revoked in [ancestor, counter, b, first.autobiography_id] {
        block(&pool, revoked, true).await;
        let before = fingerprint(&pool).await;
        for include_history in [false, true] {
            let read = s.autobiography(include_history, 100).await.unwrap();
            assert_eq!(items(&read).len(), 1);
            assert_eq!(id(&items(&read)[0]), independent.autobiography_id);
        }
        assert!(matches!(
            s.record_autobiography("private-next", next_expected, &next_inputs, &next)
                .await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            s.record_autobiography("private-first", old_expected, &old_inputs, &old)
                .await,
            Err(StoreError::NotFound)
        ));
        let mut derived = draft("private.derivative", safe);
        derived.supersedes = Some(second.autobiography_id);
        assert!(matches!(
            s.record_autobiography(
                "blocked-new",
                s.snapshot().await.unwrap().unwrap().expected(),
                &[safe, second.autobiography_id],
                &derived
            )
            .await,
            Err(StoreError::NotFound)
        ));
        assert_eq!(fingerprint(&pool).await, before);
        block(&pool, revoked, false).await;
    }
    let individual = s.snapshot().await.unwrap().unwrap().id;
    block(&pool, individual, true).await;
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.autobiography(true, 100).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        s.record_autobiography("private-next", next_expected, &next_inputs, &next)
            .await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04 autobiography: isolated PostgreSQL and total lock required"]
async fn late_outbox_failure_rolls_back_everything_and_same_key_retry_publishes_once(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s, "旧输入").await;
    let first = record(&s, &[e], &draft("rollback.topic", e)).await;
    let new = message(&s, "新输入").await;
    let mut d = draft("rollback.topic", new);
    d.supersedes = Some(first.autobiography_id);
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let inputs = [new, first.autobiography_id];
    let before = fingerprint(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION reject_auto_publish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic late publication failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_auto_publish BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION reject_auto_publish();").execute(&pool).await.unwrap();
    assert!(matches!(
        s.record_autobiography("rollback-intent", expected, &inputs, &d)
            .await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let old = s.autobiography(false, 30).await.unwrap();
    assert_eq!(id(&items(&old)[0]), first.autobiography_id);
    assert_eq!(items(&old)[0]["version"], 1);
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().state_version,
        expected.version
    );
    sqlx::query("DROP TRIGGER reject_auto_publish ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let r = s
        .record_autobiography("rollback-intent", expected, &inputs, &d)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(
        s.record_autobiography("rollback-intent", expected, &inputs, &d)
            .await
            .unwrap(),
        r
    );
    assert_eq!(fingerprint(&pool).await, before);
    assert_eq!(items(&s.autobiography(true, 100).await.unwrap()).len(), 2);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM events WHERE kind='autobiography.revised'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let receipt_count:i64=sqlx::query_scalar("SELECT count(*) FROM idempotency_records WHERE route='/internal/autobiography' AND key='rollback-intent'").fetch_one(&pool).await.unwrap();
    assert_eq!(receipt_count, 1);
    let edges: i64 = sqlx::query_scalar("SELECT count(*) FROM derivation_edges WHERE target_id=$1")
        .bind(r.autobiography_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(edges, 3);
}

#[sqlx::test]
#[ignore = "P04 autobiography: isolated PostgreSQL and total lock required"]
async fn bounded_filter_first_reads_are_deterministic_readonly_and_lifecycle_gated(pool: PgPool) {
    let uninitialized = Store::new(pool.clone());
    assert!(matches!(
        uninitialized.autobiography(false, 30).await,
        Err(StoreError::NotInitialized)
    ));
    let missing = Uuid::new_v4();
    assert!(matches!(
        uninitialized
            .record_autobiography(
                "uninitialized",
                alia::domain::history::ExpectedState {
                    version: 1,
                    privacy_epoch: 0
                },
                &[missing],
                &draft("empty", missing)
            )
            .await,
        Err(StoreError::NotInitialized)
    ));
    let s = setup(&pool).await;
    let empty = s.autobiography(false, 100).await.unwrap();
    assert!(items(&empty).is_empty());
    assert_eq!(empty["has_more"], false);
    let safe = message(&s, "可见范围来源").await;
    let mut ids = vec![];
    for n in 0..103 {
        ids.push(
            record(&s, &[safe], &draft(&format!("topic.{n:03}"), safe))
                .await
                .autobiography_id,
        );
    }
    let private = message(&s, "不可见来源").await;
    for n in 0..3 {
        record(&s, &[private], &draft(&format!("hidden.{n}"), private)).await;
    }
    block(&pool, private, true).await;
    sqlx::query("UPDATE autobiographies SET created_at='2026-01-01T00:00:00Z' WHERE id=ANY($1)")
        .bind(&ids)
        .execute(&pool)
        .await
        .unwrap();
    let ro = PgPoolOptions::new()
        .max_connections(2)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET default_transaction_read_only=on")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let read_only = Store::new(ro.clone());
    let before = fingerprint(&pool).await;
    let first = read_only.autobiography(false, 3).await.unwrap();
    assert_eq!(items(&first).len(), 3);
    assert_eq!(first["has_more"], true);
    assert!(
        items(&first)
            .iter()
            .all(|v| v["topic"].as_str().unwrap().starts_with("topic."))
    );
    ids.sort_by(|a, b| b.cmp(a));
    assert_eq!(items(&first).iter().map(id).collect::<Vec<_>>(), ids[..3]);
    let bounded = read_only.autobiography(true, 100).await.unwrap();
    assert_eq!(items(&bounded).len(), 100);
    assert_eq!(bounded["has_more"], true);
    assert_eq!(fingerprint(&pool).await, before);
    ro.close().await;
    for limit in [0, 101, u32::MAX] {
        assert!(matches!(
            s.autobiography(false, limit).await,
            Err(StoreError::Invalid(_))
        ));
    }
    sqlx::query("UPDATE individuals SET lifecycle='paused'")
        .execute(&pool)
        .await
        .unwrap();
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.record_autobiography("paused", expected, &[safe], &draft("paused.new", safe))
            .await,
        Err(StoreError::Inactive)
    ));
    assert_eq!(items(&s.autobiography(false, 3).await.unwrap()).len(), 3);
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("UPDATE individuals SET lifecycle='active',privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.record_autobiography(
            "stale-privacy",
            expected,
            &[safe],
            &draft("stale.privacy", safe)
        )
        .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let usage: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(usage, 0);
}
