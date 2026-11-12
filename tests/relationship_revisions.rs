//! Actual PostgreSQL revisions with explicit synthetic human-input fixtures.
use alia::{
    adapters::{
        relationship_revisions::RelationshipRevisionReceipt,
        store::{Store, StoreError},
    },
    domain::{
        affect::{AppraisalDraft, AttitudeDraft},
        config::{PersonaSeed, RuntimeRules},
        relationships::{RelationshipDelta, RelationshipRevisionDraft},
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "relation-init",
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
async fn subject(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'relationship' FROM individuals",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO relationships(individual_id,subject_id,subject_key) SELECT id,$1,$2 FROM individuals").bind(id).bind(format!("fixture-{id}")).execute(pool).await.unwrap();
    id
}
async fn version(pool: &PgPool, id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT version FROM relationships WHERE subject_id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn draft(pool: &PgPool, subject: Uuid, e: Uuid, stage: &str) -> RelationshipRevisionDraft {
    RelationshipRevisionDraft {
        subject_id: subject,
        expected_relationship_version: version(pool, subject).await,
        changes: RelationshipDelta {
            stage: Some(stage.into()),
            ..Default::default()
        },
        event_ids: vec![e],
        brief_basis: "按这次实际陈述修订有限记录，不认证对方情感".into(),
    }
}
async fn inputs(pool: &PgPool, d: &RelationshipRevisionDraft) -> Vec<Uuid> {
    let mut ids = vec![d.subject_id];
    ids.extend(&d.event_ids);
    let previous:Option<Uuid>=sqlx::query_scalar("SELECT id FROM relationship_revisions WHERE subject_id=$1 ORDER BY after_version DESC,id DESC LIMIT 1").bind(d.subject_id).fetch_optional(pool).await.unwrap();
    ids.extend(previous);
    ids
}
async fn record(
    s: &Store,
    pool: &PgPool,
    d: &RelationshipRevisionDraft,
) -> RelationshipRevisionReceipt {
    s.revise_relationship(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        &inputs(pool, d).await,
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
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,$1,$2,$3,'synthetic-fixture',clock_timestamp(),'{\"text\":\"合成来源\"}',$1 FROM individuals").bind(id).bind(kind).bind(source).execute(pool).await.unwrap();
    id
}
async fn edge(pool: &PgPool, source: Uuid, target: Uuid) {
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT id,$1,$2 FROM individuals ON CONFLICT DO NOTHING").bind(source).bind(target).execute(pool).await.unwrap();
}
async fn block(pool: &PgPool, id: Uuid, blocked: bool) {
    sqlx::query("UPDATE objects SET blocked=$2 WHERE id=$1")
        .bind(id)
        .bind(blocked)
        .execute(pool)
        .await
        .unwrap();
}
async fn fingerprint(pool: &PgPool) -> Value {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut out = serde_json::Map::new();
    for t in tables {
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        let d:String=sqlx::query_scalar(&format!("SELECT md5(COALESCE(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM \"{t}\" t")).fetch_one(pool).await.unwrap();
        out.insert(t, json!(d));
    }
    Value::Object(out)
}
fn items(v: &Value) -> &[Value] {
    v["items"].as_array().unwrap()
}
fn id(v: &Value) -> Uuid {
    serde_json::from_value(v["id"].clone()).unwrap()
}

#[sqlx::test]
#[ignore = "P04 relationships: isolated PostgreSQL and total lock required"]
async fn disagreement_and_repair_records_keep_actual_history_sources_and_restart(pool: PgPool) {
    let s = setup(&pool).await;
    let who: Uuid =
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let e = message(&s, "我不同意无依据的概括，请记录分歧和边界").await;
    let original: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
        .bind(e)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut first = draft(&pool, who, e, "checking_disagreement").await;
    first.changes.unresolved = Some(vec!["一次概括尚待核查".into()]);
    first.changes.boundaries = Some(vec!["不替对方断言感情".into()]);
    let r1 = record(&s, &pool, &first).await;
    assert_eq!(
        serde_json::to_value(&r1)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        6
    );
    let new = message(
        &s,
        "我确认只在这件事上按新资料调整，保留原边界，之后先核查再提出约定",
    )
    .await;
    let mut revised = draft(&pool, who, new, "limited_agreement").await;
    revised.changes.unresolved = Some(vec![]);
    revised.changes.expectations = Some(vec!["遇到缺口先核查".into()]);
    revised.changes.commitments = Some(vec!["仅在明确范围内继续探索".into()]);
    let r2 = record(&s, &pool, &revised).await;
    let restart = Store::new(pool.clone());
    let before = fingerprint(&pool).await;
    let history = restart.relationship_history(who, 30).await.unwrap();
    assert_eq!(history.as_object().unwrap().len(), 6);
    assert_eq!(items(&history).len(), 2);
    let latest = &items(&history)[0];
    assert_eq!(id(latest), r2.revision_id);
    assert_eq!(latest["previous_revision_id"], r1.revision_id.to_string());
    assert_eq!(latest["after_state"]["unresolved"], json!([]));
    assert_eq!(
        latest["after_state"]["boundaries"],
        json!(first.changes.boundaries.as_ref().unwrap())
    );
    assert_eq!(latest["after_state"].as_object().unwrap().len(), 5);
    assert_eq!(latest.as_object().unwrap().len(), 10);
    assert_eq!(
        items(&history)[1]["after_state"]["stage"],
        "checking_disagreement"
    );
    assert_eq!(items(&history)[1]["event_ids"], json!([e]));
    assert_eq!(fingerprint(&pool).await, before);
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
        .bind(e)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, original);
    let event_types:Vec<(String,String)>=sqlx::query_as("SELECT source_kind,actor_id FROM events WHERE kind='relationship.revised' ORDER BY sequence").fetch_all(&pool).await.unwrap();
    assert_eq!(
        event_types,
        vec![
            ("inference".into(), "individual".into()),
            ("inference".into(), "individual".into())
        ]
    );
}

#[sqlx::test]
#[ignore = "P04 relationships: isolated PostgreSQL and total lock required"]
async fn message_version_gaps_stale_targets_noop_and_manifest_or_owner_errors_refuse_atomically(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let who: Uuid =
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let e = message(&s, "实际消息一").await;
    let d = draft(&pool, who, e, "known").await;
    let first = record(&s, &pool, &d).await;
    message(&s, "消息独立递增关系版本一").await;
    message(&s, "消息独立递增关系版本二").await;
    let new = message(&s, "新的明确范围").await;
    let next = draft(&pool, who, new, "clarified").await;
    assert!(next.expected_relationship_version > first.relationship_version);
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let manifest = inputs(&pool, &next).await;
    let before = fingerprint(&pool).await;
    let mut stale = next.clone();
    stale.expected_relationship_version = first.relationship_version;
    assert!(matches!(
        s.revise_relationship("stale-target", expected, &manifest, &stale)
            .await,
        Err(StoreError::StaleState)
    ));
    assert!(matches!(
        s.revise_relationship("missing-prev", expected, &[who, new], &next)
            .await,
        Err(StoreError::Invalid(_))
    ));
    let mut noop = next.clone();
    noop.changes = RelationshipDelta::default();
    assert!(matches!(
        s.revise_relationship("noop", expected, &manifest, &noop)
            .await,
        Err(StoreError::Invalid(_))
    ));
    let mut unchanged = next.clone();
    unchanged.changes.stage = Some("known".into());
    assert!(matches!(
        s.revise_relationship("unchanged", expected, &manifest, &unchanged)
            .await,
        Err(StoreError::Invalid(_))
    ));
    let mut missing = next.clone();
    missing.event_ids = vec![Uuid::new_v4()];
    assert!(matches!(
        s.revise_relationship("missing-source", expected, &manifest, &missing)
            .await,
        Err(StoreError::Invalid(_))
    ));
    let foreign = Uuid::new_v4();
    let mut wrong = next.clone();
    wrong.subject_id = foreign;
    assert!(matches!(
        s.revise_relationship("foreign", expected, &[foreign, new], &wrong)
            .await,
        Err(StoreError::NotFound)
    ));
    let error =
        sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'relationship')")
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
    let r = s
        .revise_relationship("actual-gap", expected, &manifest, &next)
        .await
        .unwrap();
    let h = s.relationship_history(who, 30).await.unwrap();
    assert_eq!(
        items(&h)[0]["previous_revision_id"],
        first.revision_id.to_string()
    );
    assert_eq!(
        items(&h)[0]["before_version"],
        next.expected_relationship_version
    );
    assert_eq!(
        r.relationship_version,
        next.expected_relationship_version + 1
    );
    assert!(matches!(
        s.revise_relationship("stale-global", expected, &manifest, &next)
            .await,
        Err(StoreError::StaleState)
    ));
    for (kind, source) in [
        ("background", "fictional_background"),
        ("decision.reviewed", "inference"),
    ] {
        let bad = event(&pool, kind, source).await;
        let body = draft(&pool, who, bad, "bad-basis").await;
        let before = fingerprint(&pool).await;
        assert!(matches!(
            s.revise_relationship(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &inputs(&pool, &body).await,
                &body
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
        assert_eq!(fingerprint(&pool).await, before);
    }
}

#[sqlx::test]
#[ignore = "P04 relationships: isolated PostgreSQL and total lock required"]
async fn independent_subjects_and_affective_layers_do_not_replace_each_other(pool: PgPool) {
    let s = setup(&pool).await;
    let who = subject(&pool).await;
    let other = subject(&pool).await;
    let e = message(&s, "同一活动的实际来源").await;
    let app = AppraisalDraft {
        event_ids: vec![e],
        goal_id: None,
        subject_id: None,
        valence: -0.5,
        arousal: 0.7,
        goal_relevance: 0.5,
        expectation_gap: -0.5,
        controllability: 0.4,
        uncertainty: 0.6,
        brief_basis: "短期活动评价".into(),
    };
    let long = AttitudeDraft {
        topic: "independent.activity".into(),
        subject_id: None,
        stance: "保留对活动的长期探索倾向".into(),
        event_ids: vec![e],
        supersedes: None,
        brief_basis: "不等于关系stage或他人感情".into(),
    };
    s.record_affect(
        "mood",
        s.snapshot().await.unwrap().unwrap().expected(),
        &[e],
        Some(&app),
        Some(&long),
    )
    .await
    .unwrap();
    let before = s.affect_snapshot(Utc::now(), 30).await.unwrap();
    let other_before: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM relationships r WHERE subject_id=$1")
            .bind(other)
            .fetch_one(&pool)
            .await
            .unwrap();
    let persona_before: Value = sqlx::query_scalar("SELECT persona FROM individuals")
        .fetch_one(&pool)
        .await
        .unwrap();
    let d = draft(&pool, who, e, "cooperative_with_boundaries").await;
    record(&s, &pool, &d).await;
    let after = s.affect_snapshot(Utc::now(), 30).await.unwrap();
    assert_eq!(before["appraisal"], after["appraisal"]);
    assert_eq!(before["attitudes"], after["attitudes"]);
    let far = s
        .affect_snapshot(Utc::now() + Duration::days(365), 30)
        .await
        .unwrap();
    assert!(far["appraisal"].is_null());
    assert_eq!(far["attitudes"], before["attitudes"]);
    assert_eq!(
        items(&s.relationship_history(who, 30).await.unwrap()).len(),
        1
    );
    assert!(items(&s.relationship_history(other, 30).await.unwrap()).is_empty());
    let other_after: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM relationships r WHERE subject_id=$1")
            .bind(other)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(other_before, other_after);
    let persona_after: Value = sqlx::query_scalar("SELECT persona FROM individuals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(persona_before, persona_after);
}

#[sqlx::test]
#[ignore = "P04 relationships: isolated PostgreSQL and total lock required"]
async fn source_subject_and_previous_revision_revocation_block_private_history_even_without_edges(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let who = subject(&pool).await;
    let other = subject(&pool).await;
    let ancestor = message(&s, "私有来源祖先").await;
    let middle = event(&pool, "message.received", "reported_claim").await;
    let leaf = event(&pool, "message.received", "reported_claim").await;
    edge(&pool, ancestor, middle).await;
    edge(&pool, middle, leaf).await;
    let d = draft(&pool, who, leaf, "checking").await;
    let old_expected = s.snapshot().await.unwrap().unwrap().expected();
    let old_inputs = inputs(&pool, &d).await;
    let first = s
        .revise_relationship("private-first", old_expected, &old_inputs, &d)
        .await
        .unwrap();
    let safe = message(&s, "独立来源").await;
    let next = draft(&pool, who, safe, "clarified").await;
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let manifest = inputs(&pool, &next).await;
    let second = s
        .revise_relationship("private-next", expected, &manifest, &next)
        .await
        .unwrap();
    let independent = record(&s, &pool, &draft(&pool, other, safe, "independent").await).await;
    sqlx::query("DELETE FROM derivation_edges WHERE target_id=ANY($1)")
        .bind(vec![
            who,
            first.revision_id,
            first.event_id,
            second.revision_id,
            second.event_id,
        ])
        .execute(&pool)
        .await
        .unwrap();
    for revoked in [ancestor, who, first.revision_id] {
        block(&pool, revoked, true).await;
        let before = fingerprint(&pool).await;
        assert!(matches!(
            s.relationship_history(who, 100).await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            s.revise_relationship("private-next", expected, &manifest, &next)
                .await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            s.revise_relationship("private-first", old_expected, &old_inputs, &d)
                .await,
            Err(StoreError::NotFound)
        ));
        let h = s.relationship_history(other, 30).await.unwrap();
        assert_eq!(id(&items(&h)[0]), independent.revision_id);
        let body = draft(&pool, who, safe, "new").await;
        assert!(matches!(
            s.revise_relationship(
                "blocked-new",
                s.snapshot().await.unwrap().unwrap().expected(),
                &inputs(&pool, &body).await,
                &body
            )
            .await,
            Err(StoreError::NotFound)
        ));
        assert_eq!(fingerprint(&pool).await, before);
        block(&pool, revoked, false).await;
    }
    // An actual shared message link is a barrier even when its derivation edge is absent.
    let linked = event(&pool, "message.received", "reported_claim").await;
    sqlx::query("INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT id,$1,$2 FROM individuals").bind(who).bind(linked).execute(&pool).await.unwrap();
    block(&pool, linked, true).await;
    assert!(matches!(
        s.relationship_history(who, 30).await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(
        items(&s.relationship_history(other, 30).await.unwrap()).len(),
        1
    );
}

#[sqlx::test]
#[ignore = "P04 relationships: isolated PostgreSQL and total lock required"]
async fn late_publication_failure_rolls_back_current_history_sources_and_same_key_retries_once(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let who = subject(&pool).await;
    let e = message(&s, "有依据的记录").await;
    let d = draft(&pool, who, e, "new-understanding").await;
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let manifest = inputs(&pool, &d).await;
    let before = fingerprint(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION reject_relation_publish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic final publication failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_relation_publish BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION reject_relation_publish();").execute(&pool).await.unwrap();
    assert!(matches!(
        s.revise_relationship("same-intent", expected, &manifest, &d)
            .await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    assert!(items(&s.relationship_history(who, 30).await.unwrap()).is_empty());
    sqlx::query("DROP TRIGGER reject_relation_publish ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let r = s
        .revise_relationship("same-intent", expected, &manifest, &d)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(
        s.revise_relationship("same-intent", expected, &manifest, &d)
            .await
            .unwrap(),
        r
    );
    assert_eq!(fingerprint(&pool).await, before);
    assert_eq!(
        items(&s.relationship_history(who, 30).await.unwrap()).len(),
        1
    );
    let mut changed = d.clone();
    changed.brief_basis = "不同意图".into();
    assert!(matches!(
        s.revise_relationship("same-intent", expected, &manifest, &changed)
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM events WHERE kind='relationship.revised'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
#[ignore = "P04 relationships: isolated PostgreSQL and total lock required"]
async fn bounded_readonly_history_scopes_before_limit_and_lifecycle_gates_preserve_state(
    pool: PgPool,
) {
    let empty = Store::new(pool.clone());
    assert!(matches!(
        empty.relationship_history(Uuid::new_v4(), 30).await,
        Err(StoreError::NotInitialized)
    ));
    let s = setup(&pool).await;
    let who = subject(&pool).await;
    let other = subject(&pool).await;
    let e = message(&s, "真实范围来源").await;
    assert!(items(&s.relationship_history(who, 30).await.unwrap()).is_empty());
    let mut revisions = vec![];
    for n in 0..103 {
        let d = draft(&pool, who, e, &format!("stage-{n}")).await;
        revisions.push(record(&s, &pool, &d).await.revision_id);
    }
    let private = message(&s, "另一个关系的私有来源").await;
    for n in 0..3 {
        let d = draft(&pool, other, private, &format!("other-stage-{n}")).await;
        record(&s, &pool, &d).await;
    }
    block(&pool, private, true).await;
    sqlx::query(
        "UPDATE relationship_revisions SET created_at='2026-01-01T00:00:00Z' WHERE id=ANY($1)",
    )
    .bind(&revisions)
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
    let reader = Store::new(ro.clone());
    let before = fingerprint(&pool).await;
    let first = reader.relationship_history(who, 3).await.unwrap();
    revisions.sort_by(|a, b| b.cmp(a));
    assert_eq!(
        items(&first).iter().map(id).collect::<Vec<_>>(),
        revisions[..3]
    );
    assert_eq!(first["has_more"], true);
    let bounded = reader.relationship_history(who, 100).await.unwrap();
    assert_eq!(items(&bounded).len(), 100);
    assert_eq!(bounded["has_more"], true);
    assert!(
        items(&bounded)
            .iter()
            .all(|v| v["subject_id"] == who.to_string())
    );
    assert_eq!(fingerprint(&pool).await, before);
    ro.close().await;
    for limit in [0, 101, u32::MAX] {
        assert!(matches!(
            s.relationship_history(who, limit).await,
            Err(StoreError::Invalid(_))
        ));
    }
    sqlx::query("UPDATE individuals SET lifecycle='paused'")
        .execute(&pool)
        .await
        .unwrap();
    let d = draft(&pool, who, e, "paused-trusted-management").await;
    record(&s, &pool, &d).await;
    for lifecycle in ["archived", "terminated"] {
        sqlx::query("UPDATE individuals SET lifecycle=$1")
            .bind(lifecycle)
            .execute(&pool)
            .await
            .unwrap();
        let d = draft(&pool, who, e, "inactive").await;
        let before = fingerprint(&pool).await;
        assert!(matches!(
            s.revise_relationship(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &inputs(&pool, &d).await,
                &d
            )
            .await,
            Err(StoreError::Inactive)
        ));
        assert_eq!(fingerprint(&pool).await, before);
    }
    let usage: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(usage, 0);
}
