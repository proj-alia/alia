//! Actual PG tests for trusted helper boundaries. Synthetic contexts/decisions are
//! explicit fixtures; the private build adds only the pending ModelContext field.
use alia::{
    adapters::{
        interests::{apply_model_changes, enrich_context},
        store::{Store, StoreError},
    },
    domain::{
        cognition::{DecisionProposal, ModelContext},
        config::{PersonaSeed, RuntimeRules},
        history::Event,
        interests::*,
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "interest-context-init",
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
async fn event(pool: &PgPool, text: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'event' FROM individuals")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,$1,'message.received','reported_claim','synthetic',clock_timestamp(),jsonb_build_object('text',$2::text),$1 FROM individuals").bind(id).bind(text).execute(pool).await.unwrap();
    id
}
fn draft(e: Uuid, description: &str) -> InterestDraft {
    InterestDraft {
        id: None,
        expected_version: None,
        description: description.into(),
        status: InterestStatus::Active,
        event_ids: vec![e],
        brief_basis: "依据本轮真实记录的合成输入作出可追踪调整".into(),
    }
}
async fn record(s: &Store, pool: &PgPool, d: &InterestDraft) -> String {
    let mut refs = d.event_ids.clone();
    if let Some(id) = &d.id {
        let object: Uuid = sqlx::query_scalar("SELECT object_id FROM interests WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        refs.push(object);
        let previous:Option<Uuid>=sqlx::query_scalar("SELECT id FROM interest_revisions WHERE interest_id=$1 ORDER BY after_version DESC LIMIT 1").bind(id).fetch_optional(pool).await.unwrap();
        refs.extend(previous);
    }
    s.record_interest(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        &refs,
        d,
    )
    .await
    .unwrap()
    .interest_id
}
async fn context(s: &Store, pool: &PgPool, source: Uuid) -> ModelContext {
    let state = s.snapshot().await.unwrap().unwrap();
    // The fixture starts a new author run only after its previous synthetic run
    // is finished; production's one-running-individual constraint stays intact.
    sqlx::query("UPDATE runs SET status='committed',finished_at=clock_timestamp() WHERE individual_id=$1 AND status='running'")
        .bind(state.id).execute(pool).await.unwrap();
    let run = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'run')")
        .bind(state.id)
        .bind(run)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,status) VALUES($1,$2,$3,'foreground',$4,$5,$6,'running')").bind(state.id).bind(run).bind(source).bind(state.state_version).bind(state.privacy_epoch).bind(state.config_version).execute(pool).await.unwrap();
    let mut c: ModelContext =
        serde_json::from_slice(include_bytes!("../evals/model_context.example.json")).unwrap();
    c.individual_id = state.id;
    c.run_id = run;
    c.state_version = state.state_version;
    c.privacy_epoch = state.privacy_epoch;
    c.config_version = state.config_version;
    c.persona = state.persona;
    c.current_event_id = source;
    c.current_scope = "synthetic-interest-scope".into();
    c.current_goal_id = None;
    c.now = chrono::Utc::now();
    let event: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
        .bind(source)
        .fetch_one(pool)
        .await
        .unwrap();
    c.events = vec![serde_json::from_value::<Event>(event).unwrap()];
    c.memories.clear();
    c.goals.clear();
    c.interests.clear();
    c.goal_origin_catalog.items.clear();
    c.input_manifest = vec![state.id, run, source];
    c.interest_context = None;
    c.selected_beliefs = None;
    c.autobiographical_context = None;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .unwrap();
    enrich_context(&mut tx, &mut c).await.unwrap();
    tx.commit().await.unwrap();
    c
}
async fn decision(
    tx: &mut Transaction<'_, Postgres>,
    c: &ModelContext,
) -> Result<Uuid, StoreError> {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'event')")
        .bind(c.individual_id)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) VALUES($1,$2,'decision.accepted','generated','alia',clock_timestamp(),'{\"brief_reason\":\"synthetic driver only\"}',$3)").bind(c.individual_id).bind(id).bind(c.run_id).execute(&mut **tx).await?;
    Ok(id)
}
async fn commit(
    pool: &PgPool,
    c: &ModelContext,
    changes: &[InterestDraft],
) -> Result<Vec<Uuid>, StoreError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
        .execute(&mut *tx)
        .await?;
    let decision = decision(&mut tx, c).await?;
    let version = c.state_version + 1;
    let ids = apply_model_changes(&mut tx, c, changes, decision, version).await?;
    sqlx::query("UPDATE individuals SET state_version=$2 WHERE id=$1 AND state_version=$3")
        .bind(c.individual_id)
        .bind(version)
        .bind(c.state_version)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO outbox(individual_id,event_type,resource_id,version) VALUES($1,'state.changed',$1,$2)").bind(c.individual_id).bind(version).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(ids)
}
fn edit(
    c: &ModelContext,
    id: &str,
    e: Uuid,
    description: &str,
    status: InterestStatus,
) -> InterestDraft {
    let v = c
        .interest_context
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|v| v.id == id)
        .unwrap();
    InterestDraft {
        id: Some(id.into()),
        expected_version: Some(v.version),
        description: description.into(),
        status,
        event_ids: vec![e],
        brief_basis: "有明确依据的修订；保留原身份和历史".into(),
    }
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
        let value:String=sqlx::query_scalar(&format!("SELECT md5(COALESCE(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM \"{t}\" t")).fetch_one(pool).await.unwrap();
        out.insert(t, json!(value));
    }
    Value::Object(out)
}

#[sqlx::test]
#[ignore = "P04-11 actual isolated PostgreSQL and minimal private field bridge required"]
async fn rename_retire_restart_restore_keep_identity_complete_history_and_old_goal_origin(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = event(&pool, "first actual source").await;
    let id = record(&s, &pool, &draft(source, "初始探索方向")).await;
    let goal = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'goal' FROM individuals")
        .bind(goal)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition) SELECT id,$1,'interest',$2,'旧兴趣目标','实际记录','探索','无来源停止' FROM individuals").bind(goal).bind(json!([id])).execute(&pool).await.unwrap();
    let mut stable = None;
    for (description, status) in [
        ("重新命名后的同一兴趣", InterestStatus::Active),
        ("明确退休，不因缺席扣分", InterestStatus::Retired),
        ("重启后按新依据恢复", InterestStatus::Active),
    ] {
        let source = event(&pool, description).await;
        let reopened = Store::new(pool.clone());
        let c = context(&reopened, &pool, source).await;
        let view = c
            .interest_context
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|v| v.id == id)
            .unwrap();
        assert!(c.interest_context.as_ref().unwrap().valid_for(&c));
        if let Some(old) = stable {
            assert_eq!(old, view.object_id)
        } else {
            stable = Some(view.object_id)
        }
        if view.status == InterestStatus::Retired {
            assert!(!c.interests.iter().any(|v| v.id == id));
            assert!(
                !c.goal_origin_catalog
                    .items
                    .iter()
                    .any(|v| v.origin_type == "interest" && v.origin_id == id)
            );
            let raw = json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[{"action":"create","origins":[{"origin_type":"interest","origin_id":id}],"description":"不能以退休兴趣创建新目标","success_criterion":"有真实记录","next_step":"探索","stop_condition":"无来源停止","priority":50}],"tool_intents":[],"development":null,"brief_reason":"synthetic"});
            let proposal =
                DecisionProposal::parse(serde_json::to_string(&raw).unwrap().as_bytes()).unwrap();
            assert!(proposal.validate_context(&c).is_err());
        }
        let ids = commit(&pool, &c, &[edit(&c, &id, source, description, status)])
            .await
            .unwrap();
        assert_eq!(ids, vec![view.object_id]);
    }
    let source = event(&pool, "final context").await;
    let c = context(&s, &pool, source).await;
    let v = c
        .interest_context
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|v| v.id == id)
        .unwrap();
    assert_eq!(v.version, 4);
    assert_eq!(v.revisions.len(), 4);
    assert_eq!(v.status, InterestStatus::Active);
    for old in &v.revisions {
        assert!(v.provenance_ids.contains(&old.id));
        assert!(v.basis_event_ids.contains(&old.event_id));
        assert!(c.events.iter().any(|e| e.id == old.event_id));
    }
    let origins: Value = sqlx::query_scalar("SELECT origin_ids FROM goals WHERE id=$1")
        .bind(goal)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(origins, json!([id]));
    let persona = s.snapshot().await.unwrap().unwrap().persona;
    assert_eq!(
        persona["personality"]["initial_interests"],
        serde_json::from_slice::<Value>(include_bytes!("../config/persona.example.json")).unwrap()
            ["personality"]["initial_interests"]
    );
}

#[sqlx::test]
#[ignore = "P04-11 actual isolated PostgreSQL and minimal private field bridge required"]
async fn context_enrichment_and_empty_apply_are_read_only_and_actual_sources_are_complete(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = event(&pool, "actual read-only source").await;
    let id = record(&s, &pool, &draft(source, "有完整依据的兴趣")).await;
    let mut c = context(&s, &pool, source).await;
    let before = fingerprint(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .unwrap();
    enrich_context(&mut tx, &mut c).await.unwrap();
    let ids = apply_model_changes(&mut tx, &c, &[], Uuid::nil(), -1)
        .await
        .unwrap();
    assert!(ids.is_empty());
    tx.commit().await.unwrap();
    assert_eq!(fingerprint(&pool).await, before);
    let v = c
        .interest_context
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|v| v.id == id)
        .unwrap();
    assert_eq!(v.origin_kind, "experience");
    assert!(v.event_ids.contains(&source));
    assert!(v.provenance_ids.contains(&v.object_id));
    assert!(v.basis_event_ids.contains(&source));
    assert!(serde_json::to_vec(&c).unwrap().len() <= 96 * 1024);
}

#[sqlx::test]
#[ignore = "P04-11 actual isolated PostgreSQL and minimal private field bridge required"]
async fn model_only_observed_versions_and_complete_payloads_can_write_and_new_ids_are_program_assigned(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = event(&pool, "actual evidence").await;
    let id = record(&s, &pool, &draft(source, "original")).await;
    let c = context(&s, &pool, source).await;
    for n in 0..6 {
        let mut context = c.clone();
        let mut d = edit(&c, &id, source, "changed", InterestStatus::Active);
        match n {
            0 => d.expected_version = Some(999),
            1 => {
                context.interest_context = None;
            }
            2 => {
                context
                    .interest_context
                    .as_mut()
                    .unwrap()
                    .items
                    .retain(|v| v.id != id);
            }
            3 => d.event_ids = vec![Uuid::new_v4()],
            4 => {
                context
                    .interest_context
                    .as_mut()
                    .unwrap()
                    .items
                    .iter_mut()
                    .find(|v| v.id == id)
                    .unwrap()
                    .description = "forged context".into();
            }
            _ => {
                d.id = Some(format!("interest:{}", Uuid::new_v4()));
                d.expected_version = None;
            }
        }
        let before = fingerprint(&pool).await;
        assert!(commit(&pool, &context, &[d]).await.is_err());
        assert_eq!(fingerprint(&pool).await, before);
    }
    let fresh = commit(&pool, &c, &[draft(source, "程序分配新ID")])
        .await
        .unwrap();
    assert_eq!(fresh.len(), 1);
    let new_id: String = sqlx::query_scalar("SELECT id FROM interests WHERE object_id=$1")
        .bind(fresh[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(new_id.starts_with("interest:"));
    assert_ne!(new_id, id);
    let before = fingerprint(&pool).await;
    assert!(matches!(
        commit(
            &pool,
            &c,
            &[edit(
                &c,
                &id,
                source,
                "stale global",
                InterestStatus::Retired
            )]
        )
        .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04-11 actual isolated PostgreSQL and minimal private field bridge required"]
async fn missing_edges_and_source_association_revocation_block_old_chain_keep_independent_items(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let old = event(&pool, "old private actual basis").await;
    let id = record(&s, &pool, &draft(old, "private interest")).await;
    let fresh = event(&pool, "later current basis").await;
    let c = context(&s, &pool, fresh).await;
    commit(
        &pool,
        &c,
        &[edit(
            &c,
            &id,
            fresh,
            "renamed private interest",
            InterestStatus::Retired,
        )],
    )
    .await
    .unwrap();
    let safe = event(&pool, "independent interest basis").await;
    let independent = record(&s, &pool, &draft(safe, "independent")).await;
    let frozen = context(&s, &pool, safe).await;
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM interest_sources WHERE interest_id=$1 AND event_id=$2")
        .bind(&id)
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    let c = context(&s, &pool, safe).await;
    let selection = c.interest_context.unwrap();
    assert!(!selection.items.iter().any(|v| v.id == id));
    assert!(selection.items.iter().any(|v| v.id == independent));
    let before = fingerprint(&pool).await;
    assert!(
        commit(
            &pool,
            &frozen,
            &[edit(
                &frozen,
                &id,
                safe,
                "cannot revive private history",
                InterestStatus::Active
            )]
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04-11 actual isolated PostgreSQL and minimal private field bridge required"]
async fn same_transaction_late_failure_and_multiple_changes_do_not_partially_commit(pool: PgPool) {
    let s = setup(&pool).await;
    let source = event(&pool, "atomic basis").await;
    let id = record(&s, &pool, &draft(source, "atomic current")).await;
    let c = context(&s, &pool, source).await;
    let d = edit(
        &c,
        &id,
        source,
        "retired by actual decision",
        InterestStatus::Retired,
    );
    sqlx::raw_sql("CREATE FUNCTION interest_model_late_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic model late rollback'; END IF; RETURN NEW; END $$; CREATE TRIGGER interest_model_late_fail BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION interest_model_late_fail();").execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert!(
        commit(
            &pool,
            &c,
            &[
                d.clone(),
                draft(source, "new item in same failed transaction")
            ]
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("DROP TRIGGER interest_model_late_fail ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let ids = commit(
        &pool,
        &c,
        &[d, draft(source, "new item in same committed transaction")],
    )
    .await
    .unwrap();
    assert_eq!(ids.len(), 2);
    let states: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM outbox WHERE event_type='state.changed' AND version=$1",
    )
    .bind(c.state_version + 1)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(states, 1);
}

#[sqlx::test]
#[ignore = "P04-11 actual isolated PostgreSQL and minimal private field bridge required"]
async fn bounded_selection_preserves_complete_bundles_and_never_truncates_sources_or_events(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = event(&pool, "bounded actual source").await;
    let id = record(&s, &pool, &draft(source, "source-overflow interest")).await;
    let extra:Vec<Uuid>=sqlx::query_scalar("INSERT INTO objects(individual_id,id,kind) SELECT id,gen_random_uuid(),'event' FROM individuals CROSS JOIN generate_series(1,1025) RETURNING id").fetch_all(&pool).await.unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT individual_id,id,'synthetic.basis','reported_claim','synthetic',clock_timestamp(),'{}',id FROM objects WHERE id=ANY($1)").bind(&extra).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO interest_sources(individual_id,interest_id,event_id) SELECT id,$1,unnest($2::uuid[]) FROM individuals").bind(&id).bind(&extra).execute(&pool).await.unwrap();
    let c = context(&s, &pool, source).await;
    let selected = c.interest_context.as_ref().unwrap();
    assert!(selected.has_more);
    assert!(!selected.items.iter().any(|v| v.id == id));
    assert!(!c.interests.iter().any(|v| v.id == id));
    assert!(!c.events.iter().any(|e| extra.contains(&e.id)));
    let huge = event(&pool, &"界".repeat(40_000)).await;
    let large_id = record(&s, &pool, &draft(huge, "whole large event must not be cut")).await;
    let trigger = event(&pool, "independent small trigger").await;
    let c = context(&s, &pool, trigger).await;
    assert!(
        !c.interest_context
            .as_ref()
            .unwrap()
            .items
            .iter()
            .any(|v| v.id == large_id)
    );
    assert!(!c.events.iter().any(|e| e.id == huge));
    assert!(!c.input_manifest.contains(&huge));
    assert!(c.interest_context.as_ref().unwrap().has_more);
    assert!(serde_json::to_vec(&c).unwrap().len() <= 96 * 1024);
    for n in 0..35 {
        record(&s, &pool, &draft(trigger, &format!("bounded visible {n}"))).await;
    }
    let c = context(&s, &pool, trigger).await;
    assert_eq!(c.interest_context.as_ref().unwrap().items.len(), 32);
    assert!(c.interest_context.as_ref().unwrap().has_more);
    assert!(c.interest_context.as_ref().unwrap().valid_for(&c));
    for v in &c.interest_context.as_ref().unwrap().items {
        assert!(v.provenance_ids.len() <= 1024);
        assert!(v.validate_shape());
    }
}

#[sqlx::test]
#[ignore = "P04-11 actual isolated PostgreSQL and minimal private field bridge required"]
async fn stale_object_versions_duplicate_changes_unobserved_events_and_retired_goal_gate_refuse(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = event(&pool, "strict typed source").await;
    let id = record(&s, &pool, &draft(source, "strict current")).await;
    let c = context(&s, &pool, source).await;
    let d = edit(&c, &id, source, "strict renamed", InterestStatus::Retired);
    let before = fingerprint(&pool).await;
    assert!(commit(&pool, &c, &[d.clone(), d.clone()]).await.is_err());
    assert_eq!(fingerprint(&pool).await, before);
    assert!(
        commit(
            &pool,
            &c,
            &[
                draft(source, "one"),
                draft(source, "two"),
                draft(source, "three")
            ]
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    let unobserved = event(&pool, "actual but unobserved").await;
    let mut other = d.clone();
    other.event_ids = vec![unobserved];
    let before = fingerprint(&pool).await;
    assert!(commit(&pool, &c, &[other]).await.is_err());
    assert_eq!(fingerprint(&pool).await, before);
    // A concurrent object edit without global-version movement must still fail the row CAS.
    sqlx::query("UPDATE interests SET version=version+1 WHERE id=$1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(commit(&pool, &c, &[d]).await.is_err());
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("UPDATE interests SET version=version-1 WHERE id=$1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    let retired = InterestDraft {
        id: Some(id.clone()),
        expected_version: Some(1),
        description: "explicitly retired".into(),
        status: InterestStatus::Retired,
        event_ids: vec![source],
        brief_basis: "trusted retirement".into(),
    };
    record(&s, &pool, &retired).await;
    let mut tx = pool.begin().await.unwrap();
    assert!(
        alia::adapters::interests::active_object(&mut tx, c.individual_id, &id)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}
