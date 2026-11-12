//! Actual isolated PG storage tests. Simulated expiry is not elapsed lived time.
use alia::{
    adapters::{
        affect::AffectReceipt,
        store::{Store, StoreError},
    },
    domain::{
        affect::{AppraisalDraft, AttitudeDraft},
        config::{PersonaSeed, RuntimeRules},
        history::ExpectedState,
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "affect-init",
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
fn appraisal(event: Uuid) -> AppraisalDraft {
    AppraisalDraft {
        event_ids: vec![event],
        goal_id: None,
        subject_id: None,
        valence: -0.2,
        arousal: 0.5,
        goal_relevance: 0.2,
        expectation_gap: -1.,
        controllability: 0.5,
        uncertainty: 0.5,
        brief_basis: "合成实际事件的有限评价".into(),
    }
}
fn attitude(topic: &str, event: Uuid) -> AttitudeDraft {
    AttitudeDraft {
        topic: topic.into(),
        subject_id: None,
        stance: "愿意继续探索，同时检查实际依据".into(),
        event_ids: vec![event],
        supersedes: None,
        brief_basis: "这是个体对活动的倾向，不是对方情感".into(),
    }
}
async fn record(
    s: &Store,
    manifest: &[Uuid],
    a: Option<&AppraisalDraft>,
    long: Option<&AttitudeDraft>,
) -> AffectReceipt {
    s.record_affect(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        manifest,
        a,
        long,
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
async fn goal(pool: &PgPool, source: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'goal' FROM individuals")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition) SELECT id,$1,'event',$2,'合成目标','可检查结果','检查实际资料','无依据时等待' FROM individuals").bind(id).bind(json!([source])).execute(pool).await.unwrap();
    id
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
    let mut result = serde_json::Map::new();
    for t in tables {
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        let d:String=sqlx::query_scalar(&format!("SELECT md5(COALESCE(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM \"{t}\" t")).fetch_one(pool).await.unwrap();
        result.insert(t, json!(d));
    }
    Value::Object(result)
}
fn attitudes(v: &Value) -> &[Value] {
    v["attitudes"].as_array().unwrap()
}
fn id(v: &Value) -> Uuid {
    serde_json::from_value(v["id"].clone()).unwrap()
}

#[sqlx::test]
#[ignore = "P04 affect: isolated PostgreSQL and total lock required"]
async fn actual_goal_appraisal_restarts_with_bounded_weight_and_exact_simulated_expiry(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s, "同一实际输入可以对不同目标产生不同相关性").await;
    let g1 = goal(&pool, e).await;
    let g2 = goal(&pool, e).await;
    let who = subject(&pool).await;
    let mut low = appraisal(e);
    low.goal_id = Some(g1);
    low.subject_id = Some(who);
    let r1 = record(&s, &[e, g1, who], Some(&low), None).await;
    assert_eq!(
        serde_json::to_value(&r1)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        6
    );
    assert!(r1.appraisal_id.is_some());
    assert!(r1.attitude_id.is_none());
    assert!(r1.previous_attitude_id.is_none());
    let (created, expires): (DateTime<Utc>, DateTime<Utc>) =
        sqlx::query_as("SELECT created_at,expires_at FROM affective_appraisals WHERE id=$1")
            .bind(r1.appraisal_id.unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(expires - created, Duration::minutes(60));
    let first = s
        .affect_snapshot(created + Duration::microseconds(1), 30)
        .await
        .unwrap();
    assert_eq!(first.as_object().unwrap().len(), 7);
    assert_eq!(first["appraisal"]["goal_id"], g1.to_string());
    assert!((first["appraisal"]["attention_weight"].as_f64().unwrap() - 0.15).abs() < 1e-12);
    assert!(first["appraisal"].get("individual_id").is_none());
    let old: Value =
        sqlx::query_scalar("SELECT to_jsonb(a)-'status' FROM affective_appraisals a WHERE id=$1")
            .bind(r1.appraisal_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut high = low.clone();
    high.goal_id = Some(g2);
    high.goal_relevance = 0.8;
    let r2 = record(&s, &[e, g2, who], Some(&high), None).await;
    let (new_created, new_expires): (DateTime<Utc>, DateTime<Utc>) =
        sqlx::query_as("SELECT created_at,expires_at FROM affective_appraisals WHERE id=$1")
            .bind(r2.appraisal_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let restarted = Store::new(pool.clone());
    let before = fingerprint(&pool).await;
    let now = restarted.affect_snapshot(new_created, 30).await.unwrap();
    assert_eq!(id(&now["appraisal"]), r2.appraisal_id.unwrap());
    assert!((now["appraisal"]["attention_weight"].as_f64().unwrap() - 0.6).abs() < 1e-12);
    assert!(
        s.affect_snapshot(created, 30).await.unwrap()["appraisal"].is_null(),
        "do not resurrect a superseded earlier state at a historical clock"
    );
    assert!(s.affect_snapshot(new_expires, 30).await.unwrap()["appraisal"].is_null());
    assert!(
        s.affect_snapshot(new_expires + Duration::days(1), 30)
            .await
            .unwrap()["appraisal"]
            .is_null()
    );
    assert!(
        !s.affect_snapshot(new_expires - Duration::microseconds(1), 30)
            .await
            .unwrap()["appraisal"]
            .is_null()
    );
    assert_eq!(fingerprint(&pool).await, before);
    let after: Value =
        sqlx::query_scalar("SELECT to_jsonb(a)-'status' FROM affective_appraisals a WHERE id=$1")
            .bind(r1.appraisal_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, old);
}

#[sqlx::test]
#[ignore = "P04 affect: isolated PostgreSQL and total lock required"]
async fn stable_attitudes_keep_independent_topics_subjects_history_and_no_absence_penalty(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s, "一次真实活动").await;
    let subject1 = subject(&pool).await;
    let subject2 = subject(&pool).await;
    let mut d = attitude("cooperation.project", e);
    d.subject_id = Some(subject1);
    let first = record(&s, &[e, subject1], None, Some(&d)).await;
    let mut other = attitude("learning.art", e);
    other.subject_id = Some(subject2);
    let independent = record(&s, &[e, subject2], None, Some(&other)).await;
    let a = appraisal(e);
    let short = record(&s, &[e], Some(&a), None).await;
    let initial = s.affect_snapshot(Utc::now(), 30).await.unwrap();
    assert_eq!(attitudes(&initial).len(), 2);
    let before = fingerprint(&pool).await;
    let far = s
        .affect_snapshot(Utc::now() + Duration::days(365), 30)
        .await
        .unwrap();
    assert!(far["appraisal"].is_null());
    assert_eq!(far["attitudes"], initial["attitudes"]);
    assert_eq!(
        fingerprint(&pool).await,
        before,
        "time and no activity must not manufacture affection loss"
    );
    let new = message(&s, "新的实际反例令长期判断有所调整").await;
    let mut revision = d.clone();
    revision.event_ids = vec![new];
    revision.stance = "保留合作倾向，同时修正具体期待".into();
    revision.supersedes = first.attitude_id;
    let next = record(
        &s,
        &[new, subject1, first.attitude_id.unwrap()],
        None,
        Some(&revision),
    )
    .await;
    assert_eq!(next.previous_attitude_id, first.attitude_id);
    let view = s.affect_snapshot(Utc::now(), 30).await.unwrap();
    assert_eq!(id(&view["appraisal"]), short.appraisal_id.unwrap());
    assert_eq!(attitudes(&view).len(), 2);
    assert!(
        attitudes(&view)
            .iter()
            .any(|v| id(v) == independent.attitude_id.unwrap())
    );
    let old: (String, i64, String) =
        sqlx::query_as("SELECT stance,version,status FROM affective_attitudes WHERE id=$1")
            .bind(first.attitude_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old, (d.stance.clone(), 2, "superseded".into()));
    let before = fingerprint(&pool).await;
    for body in [
        d.clone(),
        {
            let mut x = revision.clone();
            x.subject_id = Some(subject2);
            x
        },
        {
            let mut x = revision.clone();
            x.topic = "different.topic".into();
            x
        },
        revision,
    ] {
        assert!(matches!(
            s.record_affect(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[e, new, subject1, subject2, first.attitude_id.unwrap()],
                None,
                Some(&body)
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
    }
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04 affect: isolated PostgreSQL and total lock required"]
async fn malformed_sources_targets_unknown_fields_and_nonfinite_values_are_refused_atomically(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s, "实际来源").await;
    let fake_goal = Uuid::new_v4();
    let fake_subject = Uuid::new_v4();
    for (id, kind) in [(fake_goal, "goal"), (fake_subject, "relationship")] {
        sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,$2 FROM individuals")
            .bind(id)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
    }
    let fictional = event(&pool, "seed.background", "fictional_background").await;
    let feedback = event(&pool, "decision.reviewed", "inference").await;
    let before = fingerprint(&pool).await;
    for a in [
        appraisal(e),
        appraisal(e),
        appraisal(e),
        appraisal(e),
        appraisal(e),
    ]
    .into_iter()
    .enumerate()
    .map(|(n, mut a)| {
        match n {
            0 => a.valence = f64::NAN,
            1 => a.arousal = f64::INFINITY,
            2 => a.expectation_gap = -1.1,
            3 => a.uncertainty = 1.1,
            _ => a.controllability = -0.1,
        };
        a
    }) {
        assert!(matches!(
            s.record_affect(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[e],
                Some(&a),
                None
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
    }
    for a in [appraisal(e), appraisal(e), appraisal(e), appraisal(e)]
        .into_iter()
        .enumerate()
        .map(|(n, mut a)| {
            match n {
                0 => a.event_ids = vec![],
                1 => a.event_ids = vec![e, e],
                2 => a.event_ids = vec![Uuid::nil()],
                _ => a.brief_basis = " ".into(),
            };
            a
        })
    {
        assert!(matches!(
            s.record_affect(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[e],
                Some(&a),
                None
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
    }
    for (goal_id, subject_id) in [
        (Some(fake_goal), None),
        (None, Some(fake_subject)),
        (Some(Uuid::new_v4()), None),
    ] {
        let mut a = appraisal(e);
        a.goal_id = goal_id;
        a.subject_id = subject_id;
        assert!(matches!(
            s.record_affect(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[e, fake_goal, fake_subject],
                Some(&a),
                None
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
    }
    for source in [fictional, feedback] {
        assert!(matches!(
            s.record_affect(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[source],
                Some(&appraisal(source)),
                None
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            s.record_affect(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[e, source],
                None,
                Some(&attitude("forbidden.extra", e))
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
    }
    let foreign = Uuid::new_v4();
    assert!(matches!(
        s.record_affect(
            "foreign",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[foreign],
            Some(&appraisal(foreign)),
            None
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
    let mut json = serde_json::to_value(appraisal(e)).unwrap();
    json["permissions"] = json!("expanded");
    assert!(serde_json::from_value::<AppraisalDraft>(json).is_err());
    assert_eq!(fingerprint(&pool).await, before);
    let r = record(&s, &[e], Some(&appraisal(e)), None).await;
    let before = fingerprint(&pool).await;
    let error = sqlx::query("UPDATE affective_appraisals SET arousal='NaN'::float8 WHERE id=$1")
        .bind(r.appraisal_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04 affect: isolated PostgreSQL and total lock required"]
async fn missing_edges_cannot_bypass_two_generation_goal_subject_source_or_predecessor_privacy(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let ancestor = message(&s, "私有实际源头").await;
    let middle = event(&pool, "message.received", "reported_claim").await;
    let leaf = event(&pool, "message.received", "reported_claim").await;
    edge(&pool, ancestor, middle).await;
    edge(&pool, middle, leaf).await;
    let g_ancestor = event(&pool, "message.received", "reported_claim").await;
    let g_middle = event(&pool, "message.received", "reported_claim").await;
    let g_leaf = event(&pool, "message.received", "reported_claim").await;
    edge(&pool, g_ancestor, g_middle).await;
    edge(&pool, g_middle, g_leaf).await;
    let g = goal(&pool, g_leaf).await;
    let s_ancestor = event(&pool, "message.received", "reported_claim").await;
    let s_middle = event(&pool, "message.received", "reported_claim").await;
    let s_leaf = event(&pool, "message.received", "reported_claim").await;
    edge(&pool, s_ancestor, s_middle).await;
    edge(&pool, s_middle, s_leaf).await;
    let who = subject(&pool).await;
    sqlx::query("INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT id,$1,$2 FROM individuals").bind(who).bind(s_leaf).execute(&pool).await.unwrap();
    let mut a = appraisal(leaf);
    a.goal_id = Some(g);
    a.subject_id = Some(who);
    let app_expected = s.snapshot().await.unwrap().unwrap().expected();
    let app_inputs = [leaf, g, who];
    let short = s
        .record_affect("private-app", app_expected, &app_inputs, Some(&a), None)
        .await
        .unwrap();
    let mut d = attitude("private.topic", leaf);
    d.subject_id = Some(who);
    let first = record(&s, &[leaf, who], None, Some(&d)).await;
    let safe = message(&s, "独立可见来源").await;
    let mut next = d.clone();
    next.event_ids = vec![safe];
    next.supersedes = first.attitude_id;
    let next_expected = s.snapshot().await.unwrap().unwrap().expected();
    let next_inputs = [safe, who, first.attitude_id.unwrap()];
    let long = s
        .record_affect(
            "private-attitude",
            next_expected,
            &next_inputs,
            None,
            Some(&next),
        )
        .await
        .unwrap();
    let independent = record(
        &s,
        &[safe],
        None,
        Some(&attitude("independent.topic", safe)),
    )
    .await;
    sqlx::query("DELETE FROM derivation_edges WHERE target_id=ANY($1)")
        .bind(vec![
            short.appraisal_id.unwrap(),
            short.event_id,
            first.attitude_id.unwrap(),
            first.event_id,
            long.attitude_id.unwrap(),
            long.event_id,
        ])
        .execute(&pool)
        .await
        .unwrap();
    for revoked in [ancestor, s_ancestor, g_ancestor, first.attitude_id.unwrap()] {
        block(&pool, revoked, true).await;
        let before = fingerprint(&pool).await;
        let view = s.affect_snapshot(Utc::now(), 100).await.unwrap();
        assert!(
            attitudes(&view)
                .iter()
                .any(|v| id(v) == independent.attitude_id.unwrap())
        );
        if revoked != first.attitude_id.unwrap() {
            assert!(view["appraisal"].is_null());
            assert!(matches!(
                s.record_affect("private-app", app_expected, &app_inputs, Some(&a), None)
                    .await,
                Err(StoreError::NotFound)
            ));
        }
        if revoked != g_ancestor {
            assert_eq!(attitudes(&view).len(), 1);
            assert!(matches!(
                s.record_affect(
                    "private-attitude",
                    next_expected,
                    &next_inputs,
                    None,
                    Some(&next)
                )
                .await,
                Err(StoreError::NotFound)
            ));
        }
        if revoked == g_ancestor {
            assert!(matches!(
                s.record_affect(
                    "blocked-goal-write",
                    s.snapshot().await.unwrap().unwrap().expected(),
                    &app_inputs,
                    Some(&a),
                    None
                )
                .await,
                Err(StoreError::NotFound)
            ));
        }
        if revoked != g_ancestor {
            assert!(matches!(
                s.record_affect(
                    "blocked-subject-write",
                    s.snapshot().await.unwrap().unwrap().expected(),
                    &next_inputs,
                    None,
                    Some(&next)
                )
                .await,
                Err(StoreError::NotFound)
            ));
        }
        assert_eq!(fingerprint(&pool).await, before);
        block(&pool, revoked, false).await;
    }
    let individual = s.snapshot().await.unwrap().unwrap().id;
    block(&pool, individual, true).await;
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.affect_snapshot(Utc::now(), 100).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        s.record_affect(
            "private-attitude",
            next_expected,
            &next_inputs,
            None,
            Some(&next)
        )
        .await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04 affect: isolated PostgreSQL and total lock required"]
async fn combined_update_is_atomic_through_late_outbox_and_retries_versions_and_pause(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s, "起初的实际输入").await;
    let who = subject(&pool).await;
    let mut a = appraisal(e);
    a.subject_id = Some(who);
    let mut d = attitude("atomic.topic", e);
    d.subject_id = Some(who);
    let old = record(&s, &[e, who], Some(&a), Some(&d)).await;
    let new = message(&s, "后来的实际输入").await;
    a.event_ids = vec![new];
    a.valence = 0.4;
    d.event_ids = vec![new];
    d.supersedes = old.attitude_id;
    d.stance = "重新评估具体合作倾向，保留历史".into();
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let manifest = [new, who, old.attitude_id.unwrap()];
    let before = fingerprint(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION reject_affect_publish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic final publication failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_affect_publish BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION reject_affect_publish();").execute(&pool).await.unwrap();
    assert!(matches!(
        s.record_affect("atomic-intent", expected, &manifest, Some(&a), Some(&d))
            .await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let view = s.affect_snapshot(Utc::now(), 30).await.unwrap();
    assert_eq!(id(&view["appraisal"]), old.appraisal_id.unwrap());
    assert_eq!(id(&attitudes(&view)[0]), old.attitude_id.unwrap());
    assert_eq!(attitudes(&view)[0]["version"], 1);
    sqlx::query("DROP TRIGGER reject_affect_publish ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let next = s
        .record_affect("atomic-intent", expected, &manifest, Some(&a), Some(&d))
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(
        s.record_affect("atomic-intent", expected, &manifest, Some(&a), Some(&d))
            .await
            .unwrap(),
        next
    );
    let mut changed = d.clone();
    changed.stance = "不同意图".into();
    assert!(matches!(
        s.record_affect(
            "atomic-intent",
            expected,
            &manifest,
            Some(&a),
            Some(&changed)
        )
        .await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert!(matches!(
        s.record_affect("stale", expected, &manifest, Some(&a), Some(&d))
            .await,
        Err(StoreError::StaleState)
    ));
    assert!(matches!(
        s.record_affect("empty", expected, &manifest, None, None)
            .await,
        Err(StoreError::Invalid(_))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("UPDATE individuals SET lifecycle='paused'")
        .execute(&pool)
        .await
        .unwrap();
    let paused = s.snapshot().await.unwrap().unwrap().expected();
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.record_affect("paused", paused, &[new], Some(&appraisal(new)), None)
            .await,
        Err(StoreError::Inactive)
    ));
    assert!(!s.affect_snapshot(Utc::now(), 30).await.unwrap()["appraisal"].is_null());
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("UPDATE individuals SET lifecycle='active',privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.record_affect("stale-privacy", paused, &[new], Some(&appraisal(new)), None)
            .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let actual: Vec<(String, String)> = sqlx::query_as(
        "SELECT source_kind,actor_id FROM events WHERE kind='affect.updated' ORDER BY sequence",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        actual,
        vec![
            ("inference".into(), "individual".into()),
            ("inference".into(), "individual".into())
        ]
    );
    let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM idempotency_records WHERE route='/internal/affect' AND key='atomic-intent'").fetch_one(&pool).await.unwrap();
    assert_eq!(receipts, 1);
}

#[sqlx::test]
#[ignore = "P04 affect: isolated PostgreSQL and total lock required"]
async fn bounded_readonly_snapshot_filters_before_limit_and_time_does_not_modify_relationships(
    pool: PgPool,
) {
    let uninitialized = Store::new(pool.clone());
    assert!(matches!(
        uninitialized.affect_snapshot(Utc::now(), 30).await,
        Err(StoreError::NotInitialized)
    ));
    let missing = Uuid::new_v4();
    assert!(matches!(
        uninitialized
            .record_affect(
                "uninitialized",
                ExpectedState {
                    version: 1,
                    privacy_epoch: 0
                },
                &[missing],
                Some(&appraisal(missing)),
                None
            )
            .await,
        Err(StoreError::NotInitialized)
    ));
    let s = setup(&pool).await;
    let empty = s.affect_snapshot(Utc::now(), 100).await.unwrap();
    assert!(empty["appraisal"].is_null());
    assert!(attitudes(&empty).is_empty());
    assert_eq!(empty["has_more"], false);
    let e = message(&s, "独立活动来源").await;
    let mut ids = vec![];
    for n in 0..103 {
        ids.push(
            record(
                &s,
                &[e],
                None,
                Some(&attitude(&format!("bounded.{n:03}"), e)),
            )
            .await
            .attitude_id
            .unwrap(),
        );
    }
    let private = message(&s, "私有活动来源").await;
    for n in 0..3 {
        record(
            &s,
            &[private],
            None,
            Some(&attitude(&format!("private.{n}"), private)),
        )
        .await;
    }
    block(&pool, private, true).await;
    sqlx::query(
        "UPDATE affective_attitudes SET created_at='2026-01-01T00:00:00Z' WHERE id=ANY($1)",
    )
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
    let readonly = Store::new(ro.clone());
    let before = fingerprint(&pool).await;
    let first = readonly
        .affect_snapshot(Utc::now() + Duration::days(365), 3)
        .await
        .unwrap();
    assert_eq!(attitudes(&first).len(), 3);
    assert_eq!(first["has_more"], true);
    ids.sort_by(|a, b| b.cmp(a));
    assert_eq!(
        attitudes(&first).iter().map(id).collect::<Vec<_>>(),
        ids[..3]
    );
    let hundred = readonly.affect_snapshot(Utc::now(), 100).await.unwrap();
    assert_eq!(attitudes(&hundred).len(), 100);
    assert_eq!(hundred["has_more"], true);
    assert!(
        attitudes(&hundred)
            .iter()
            .all(|v| v.as_object().unwrap().len() == 11)
    );
    assert_eq!(fingerprint(&pool).await, before);
    ro.close().await;
    for limit in [0, 101, u32::MAX] {
        assert!(matches!(
            s.affect_snapshot(Utc::now(), limit).await,
            Err(StoreError::Invalid(_))
        ));
    }
    let usage: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(usage, 0);
}
