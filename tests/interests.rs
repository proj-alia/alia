//! Stable-ID persistence and actual SQL upgrade; synthetic records, no models.
use alia::{
    adapters::{
        interests::InterestReceipt,
        store::{Store, StoreError},
    },
    domain::{
        config::{PersonaSeed, RuntimeRules},
        history::ExpectedState,
        interests::{InterestDraft, InterestStatus},
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::borrow::Cow;
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "interest-init",
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
fn draft(e: Uuid, description: &str) -> InterestDraft {
    InterestDraft {
        id: None,
        expected_version: None,
        description: description.into(),
        status: InterestStatus::Active,
        event_ids: vec![e],
        brief_basis: "按这次实际输入建立可修订兴趣".into(),
    }
}
async fn current(pool: &PgPool, id: &str) -> (Uuid, i64) {
    sqlx::query_as("SELECT object_id,version FROM interests WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn inputs(pool: &PgPool, d: &InterestDraft) -> Vec<Uuid> {
    let mut refs = d.event_ids.clone();
    if let Some(id) = &d.id {
        refs.push(current(pool, id).await.0);
        let prev:Option<Uuid>=sqlx::query_scalar("SELECT id FROM interest_revisions WHERE interest_id=$1 ORDER BY after_version DESC,id DESC LIMIT 1").bind(id).fetch_optional(pool).await.unwrap();
        refs.extend(prev);
    }
    refs
}
async fn record(s: &Store, pool: &PgPool, d: &InterestDraft) -> InterestReceipt {
    s.record_interest(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        &inputs(pool, d).await,
        d,
    )
    .await
    .unwrap()
}
async fn edit(
    pool: &PgPool,
    id: &str,
    e: Uuid,
    description: &str,
    status: InterestStatus,
) -> InterestDraft {
    InterestDraft {
        id: Some(id.into()),
        expected_version: Some(current(pool, id).await.1),
        description: description.into(),
        status,
        event_ids: vec![e],
        brief_basis: "依据实际输入调整当前兴趣，身份保留".into(),
    }
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
        let digest:String=sqlx::query_scalar(&format!("SELECT md5(COALESCE(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM \"{t}\" t")).fetch_one(pool).await.unwrap();
        out.insert(t, json!(digest));
    }
    Value::Object(out)
}
fn items(v: &Value) -> &[Value] {
    v["items"].as_array().unwrap()
}

#[sqlx::test(migrations = false)]
#[ignore = "P04 interests: isolated PostgreSQL and total lock required"]
async fn real_legacy_migration_backfills_immutable_seed_once_and_preserves_goal_ids(pool: PgPool) {
    static ALL: sqlx::migrate::Migrator = sqlx::migrate!();
    let previous = sqlx::migrate::Migrator {
        migrations: Cow::Owned(ALL.iter().filter(|m| m.version < 27).cloned().collect()),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    previous.run(&pool).await.unwrap();
    let seed: Value =
        serde_json::from_slice(include_bytes!("../config/persona.example.json")).unwrap();
    let mut persona = seed.clone();
    persona["personality"]["initial_interests"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let config: Value =
        serde_json::from_slice(include_bytes!("../config/runtime.example.json")).unwrap();
    let digest = Sha256::digest(serde_json::to_vec(&seed).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let who = Uuid::new_v4();
    let birth = Uuid::new_v4();
    sqlx::query("INSERT INTO individuals(id,origin,seed,seed_digest,persona,runtime_config) VALUES($1,'{\"kind\":\"created\"}',$2,$3,$4,$5)").bind(who).bind(&seed).bind(&digest).bind(persona).bind(config).execute(&pool).await.unwrap();
    for (id, kind) in [(who, "identity"), (birth, "event")] {
        sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,$3)")
            .bind(who)
            .bind(id)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) VALUES($1,$2,'individual.created','direct_observation','system',clock_timestamp(),$3,$2)").bind(who).bind(birth).bind(json!({"seed_digest":digest})).execute(&pool).await.unwrap();
    edge(&pool, birth, who).await;
    let goal = Uuid::new_v4();
    let legacy = format!("interest:{who}:0");
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'goal')")
        .bind(who)
        .bind(goal)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition) VALUES($1,$2,'interest',$3,'旧兴趣目标','实际结果','探索','无依据则等待')").bind(who).bind(goal).bind(json!([legacy])).execute(&pool).await.unwrap();
    ALL.run(&pool).await.unwrap();
    let s = Store::new(pool.clone());
    let view = s.interests(true, 100).await.unwrap();
    let seed_interests = seed["personality"]["initial_interests"].as_array().unwrap();
    assert_eq!(items(&view).len(), seed_interests.len());
    for (n, description) in seed_interests.iter().enumerate() {
        let id = format!("interest:{who}:{n}");
        let item = items(&view).iter().find(|v| v["id"] == id).unwrap();
        assert_eq!(item["description"], *description);
        assert_eq!(item["origin_kind"], "initial_seed");
        assert_eq!(item["event_ids"], json!([birth]));
    }
    let before = fingerprint(&pool).await;
    ALL.run(&pool).await.unwrap();
    assert_eq!(fingerprint(&pool).await, before);
    let origins: Value = sqlx::query_scalar("SELECT origin_ids FROM goals WHERE id=$1")
        .bind(goal)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(origins, json!([legacy]));
}

#[sqlx::test]
#[ignore = "P04 interests: isolated PostgreSQL and total lock required"]
async fn add_relabel_retire_reactivate_and_reorder_never_replace_identity_or_birth_seed(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let seed: Value = sqlx::query_scalar("SELECT seed FROM individuals")
        .fetch_one(&pool)
        .await
        .unwrap();
    let e = message(&s, "一次新的实际探索").await;
    let first = record(&s, &pool, &draft(e, "探索语言与证据")).await;
    assert_eq!(
        serde_json::to_value(&first)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        6
    );
    let tail = first.interest_id.strip_prefix("interest:").unwrap();
    assert!(Uuid::parse_str(tail).is_ok());
    let original = current(&pool, &first.interest_id).await;
    let goal = Uuid::new_v4();
    let who = s.snapshot().await.unwrap().unwrap().id;
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'goal')")
        .bind(who)
        .bind(goal)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition) VALUES($1,$2,'interest',$3,'原目标','结果','探索','等待')").bind(who).bind(goal).bind(json!([first.interest_id])).execute(&pool).await.unwrap();
    for (description, status) in [
        ("更精确的兴趣描述", InterestStatus::Active),
        ("更精确的兴趣描述", InterestStatus::Retired),
        ("更精确的兴趣描述", InterestStatus::Active),
    ] {
        let d = edit(&pool, &first.interest_id, e, description, status).await;
        let r = record(&s, &pool, &d).await;
        assert_eq!(r.interest_id, first.interest_id);
        assert_eq!(current(&pool, &r.interest_id).await.0, original.0);
        let visible = s.interests(false, 100).await.unwrap();
        assert_eq!(
            items(&visible).iter().any(|v| v["id"] == r.interest_id),
            status == InterestStatus::Active
        );
    }
    sqlx::query("UPDATE individuals SET persona=jsonb_set(persona,'{personality,initial_interests}','[\"重新排序的模板内容\"]')").execute(&pool).await.unwrap();
    let restart = Store::new(pool.clone());
    let view = restart.interests(true, 100).await.unwrap();
    let item = items(&view)
        .iter()
        .find(|v| v["id"] == first.interest_id)
        .unwrap();
    assert_eq!(item["description"], "更精确的兴趣描述");
    assert_eq!(item["version"], 4);
    assert_eq!(item.as_object().unwrap().len(), 9);
    let history = restart
        .interest_history(&first.interest_id, 100)
        .await
        .unwrap();
    assert_eq!(items(&history).len(), 4);
    assert_eq!(items(&history)[0]["before_version"], 0);
    assert!(items(&history)[0]["before_description"].is_null());
    assert!(
        items(&history)
            .iter()
            .all(|v| v.as_object().unwrap().len() == 12)
    );
    let after: Value = sqlx::query_scalar("SELECT seed FROM individuals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, seed);
    let origins: Value = sqlx::query_scalar("SELECT origin_ids FROM goals WHERE id=$1")
        .bind(goal)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(origins, json!([first.interest_id]));
}

#[sqlx::test]
#[ignore = "P04 interests: isolated PostgreSQL and total lock required"]
async fn invalid_identity_owner_version_manifest_sources_noop_and_unknown_fields_refuse_atomically(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s, "实际来源").await;
    let r = record(&s, &pool, &draft(e, "当前描述")).await;
    let d = edit(&pool, &r.interest_id, e, "新的描述", InterestStatus::Active).await;
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let refs = inputs(&pool, &d).await;
    let before = fingerprint(&pool).await;
    let mut stale = d.clone();
    stale.expected_version = Some(99);
    assert!(matches!(
        s.record_interest("stale-object", expected, &refs, &stale)
            .await,
        Err(StoreError::StaleState)
    ));
    assert!(matches!(
        s.record_interest("missing-object", expected, &[e, r.revision_id], &d)
            .await,
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        s.record_interest(
            "missing-prev",
            expected,
            &[e, current(&pool, &r.interest_id).await.0],
            &d
        )
        .await,
        Err(StoreError::Invalid(_))
    ));
    let mut noop = d.clone();
    noop.description = "当前描述".into();
    assert!(matches!(
        s.record_interest("noop", expected, &refs, &noop).await,
        Err(StoreError::Invalid(_))
    ));
    let mut forged = draft(e, "伪造兴趣身份");
    forged.id = Some(format!("interest:{}", Uuid::new_v4()));
    assert!(matches!(
        s.record_interest("forged-new", expected, &[e], &forged)
            .await,
        Err(StoreError::Invalid(_))
    ));
    forged.expected_version = Some(1);
    assert!(matches!(
        s.record_interest("foreign-id", expected, &[e], &forged)
            .await,
        Err(StoreError::NotFound)
    ));
    let unknown = Uuid::new_v4();
    assert!(matches!(
        s.record_interest(
            "foreign-source",
            expected,
            &[unknown],
            &draft(unknown, "未知来源")
        )
        .await,
        Err(StoreError::NotFound)
    ));
    let mut value = serde_json::to_value(&d).unwrap();
    value["permissions"] = json!("expanded");
    assert!(serde_json::from_value::<InterestDraft>(value).is_err());
    assert_eq!(fingerprint(&pool).await, before);
    for (kind, source) in [
        ("background", "fictional_background"),
        ("decision.reviewed", "inference"),
    ] {
        let bad = event(&pool, kind, source).await;
        let before = fingerprint(&pool).await;
        assert!(matches!(
            s.record_interest(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[bad],
                &draft(bad, "禁止的依据")
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
        assert_eq!(fingerprint(&pool).await, before);
    }
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.record_interest("stale-privacy", expected, &refs, &d)
            .await,
        Err(StoreError::StaleState)
    ));
}

#[sqlx::test]
#[ignore = "P04 interests: isolated PostgreSQL and total lock required"]
async fn current_history_sources_and_predecessor_privacy_survive_missing_edges_and_private_replays(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let ancestor = message(&s, "私有源头").await;
    let mid = event(&pool, "message.received", "reported_claim").await;
    let leaf = event(&pool, "message.received", "reported_claim").await;
    edge(&pool, ancestor, mid).await;
    edge(&pool, mid, leaf).await;
    let first_d = draft(leaf, "私有兴趣");
    let old_expected = s.snapshot().await.unwrap().unwrap().expected();
    let first = s
        .record_interest("private-first", old_expected, &[leaf], &first_d)
        .await
        .unwrap();
    let safe = message(&s, "独立来源").await;
    let changed = edit(
        &pool,
        &first.interest_id,
        safe,
        "新的私有理解",
        InterestStatus::Active,
    )
    .await;
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let refs = inputs(&pool, &changed).await;
    let second = s
        .record_interest("private-next", expected, &refs, &changed)
        .await
        .unwrap();
    let independent = record(&s, &pool, &draft(safe, "独立可见兴趣")).await;
    let object = current(&pool, &first.interest_id).await.0;
    sqlx::query("DELETE FROM derivation_edges WHERE target_id=ANY($1)")
        .bind(vec![
            object,
            first.revision_id,
            first.event_id,
            second.revision_id,
            second.event_id,
        ])
        .execute(&pool)
        .await
        .unwrap();
    for revoked in [ancestor, object, first.revision_id] {
        block(&pool, revoked, true).await;
        let before = fingerprint(&pool).await;
        let list = s.interests(true, 100).await.unwrap();
        assert!(!items(&list).iter().any(|v| v["id"] == first.interest_id));
        assert!(
            items(&list)
                .iter()
                .any(|v| v["id"] == independent.interest_id)
        );
        assert!(matches!(
            s.interest_history(&first.interest_id, 100).await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            s.record_interest("private-next", expected, &refs, &changed)
                .await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            s.record_interest("private-first", old_expected, &[leaf], &first_d)
                .await,
            Err(StoreError::NotFound)
        ));
        let update = edit(
            &pool,
            &first.interest_id,
            safe,
            "不应重新出现",
            InterestStatus::Active,
        )
        .await;
        assert!(matches!(
            s.record_interest(
                "blocked-new",
                s.snapshot().await.unwrap().unwrap().expected(),
                &inputs(&pool, &update).await,
                &update
            )
            .await,
            Err(StoreError::NotFound)
        ));
        assert_eq!(fingerprint(&pool).await, before);
        block(&pool, revoked, false).await;
    }
}

#[sqlx::test]
#[ignore = "P04 interests: isolated PostgreSQL and total lock required"]
async fn late_outbox_failure_rolls_back_new_and_revised_interest_and_same_key_retries_once(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s, "实际形成来源").await;
    let r = record(&s, &pool, &draft(e, "旧描述")).await;
    let next = edit(
        &pool,
        &r.interest_id,
        e,
        "修订后的描述",
        InterestStatus::Retired,
    )
    .await;
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let refs = inputs(&pool, &next).await;
    let before = fingerprint(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION reject_interest_publish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic final publication failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_interest_publish BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION reject_interest_publish();").execute(&pool).await.unwrap();
    assert!(matches!(
        s.record_interest("same-intent", expected, &refs, &next)
            .await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    assert!(matches!(
        s.record_interest("new-fails", expected, &[e], &draft(e, "失败的新兴趣"))
            .await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("DROP TRIGGER reject_interest_publish ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let receipt = s
        .record_interest("same-intent", expected, &refs, &next)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(
        s.record_interest("same-intent", expected, &refs, &next)
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(fingerprint(&pool).await, before);
    assert_eq!(
        items(&s.interest_history(&r.interest_id, 100).await.unwrap()).len(),
        2
    );
    assert!(matches!(
        s.record_interest("stale-global", expected, &refs, &next)
            .await,
        Err(StoreError::StaleState)
    ));
}

#[sqlx::test]
#[ignore = "P04 interests: isolated PostgreSQL and total lock required"]
async fn bounded_rr_lists_and_history_are_readonly_and_do_not_reset_or_decay_interests(
    pool: PgPool,
) {
    let empty = Store::new(pool.clone());
    assert!(matches!(
        empty.interests(false, 30).await,
        Err(StoreError::NotInitialized)
    ));
    let unknown = Uuid::new_v4();
    assert!(matches!(
        empty
            .record_interest(
                "empty",
                ExpectedState {
                    version: 1,
                    privacy_epoch: 0
                },
                &[unknown],
                &draft(unknown, "未初始化")
            )
            .await,
        Err(StoreError::NotInitialized)
    ));
    let s = setup(&pool).await;
    let e = message(&s, "独立来源").await;
    let mut target = None;
    for n in 0..103 {
        let r = record(&s, &pool, &draft(e, &format!("兴趣{n}"))).await;
        if n == 0 {
            target = Some(r.interest_id);
        }
    }
    let target = target.unwrap();
    for n in 0..102 {
        let d = edit(
            &pool,
            &target,
            e,
            &format!("新描述{n}"),
            InterestStatus::Active,
        )
        .await;
        record(&s, &pool, &d).await;
    }
    let private = message(&s, "私有来源").await;
    for n in 0..3 {
        record(&s, &pool, &draft(private, &format!("私有{n}"))).await;
    }
    block(&pool, private, true).await;
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
    let list = reader.interests(false, 100).await.unwrap();
    assert_eq!(items(&list).len(), 100);
    assert_eq!(list["has_more"], true);
    assert!(
        !items(&list)
            .iter()
            .any(|v| v["description"].as_str().unwrap().starts_with("私有"))
    );
    let history = reader.interest_history(&target, 100).await.unwrap();
    assert_eq!(items(&history).len(), 100);
    assert_eq!(history["has_more"], true);
    assert_eq!(fingerprint(&pool).await, before);
    ro.close().await;
    for limit in [0, 101, u32::MAX] {
        assert!(matches!(
            s.interests(true, limit).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            s.interest_history(&target, limit).await,
            Err(StoreError::Invalid(_))
        ));
    }
    sqlx::query("UPDATE individuals SET lifecycle='paused'")
        .execute(&pool)
        .await
        .unwrap();
    let d = edit(
        &pool,
        &target,
        e,
        "受信任暂停期间调整",
        InterestStatus::Retired,
    )
    .await;
    record(&s, &pool, &d).await;
    sqlx::query("UPDATE individuals SET lifecycle='archived'")
        .execute(&pool)
        .await
        .unwrap();
    let d = edit(&pool, &target, e, "不能写入", InterestStatus::Active).await;
    assert!(matches!(
        s.record_interest(
            "archived",
            s.snapshot().await.unwrap().unwrap().expected(),
            &inputs(&pool, &d).await,
            &d
        )
        .await,
        Err(StoreError::Inactive)
    ));
    let usage: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(usage, 0);
}
