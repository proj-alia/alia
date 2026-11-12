//! P01-M3-REVIEW-01: trusted synthetic records are setup only, never an attacker capability.
//! Only cross-candidate assessment references, normal commands and scoped QA faults are tested.
//! Actual PostgreSQL boundaries with explicitly synthetic trusted execution records.
use alia::{
    adapters::{
        development::{DevelopmentCommand, DevelopmentReceipt},
        store::{Store, StoreError},
    },
    domain::{
        config::{PersonaSeed, RuntimeRules},
        development::*,
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
        "development-init",
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
async fn message(s: &Store) -> Uuid {
    s.submit_message(
        &Uuid::new_v4().to_string(),
        "这是实际记录的合成外部陈述；不作为真实模型成长证据",
        None,
    )
    .await
    .unwrap()
    .event_id
}
fn seed() -> Value {
    serde_json::from_slice(include_bytes!("../config/persona.example.json")).unwrap()
}
fn traits(value: f64) -> Value {
    let mut v = seed()["personality"]["traits"].clone();
    v["curiosity"] = json!(value);
    v
}
fn draft(event: Uuid, target: DevelopmentTarget, value: Value) -> DevelopmentDraft {
    DevelopmentDraft {
        target,
        before_version: 1,
        proposed_value: value,
        proposed_description: "根据实际经历作出有界调整".into(),
        evidence_ids: vec![event],
        counterevidence_ids: vec![],
        validation_plan: "实际独立场景与反证搜索验证；保留失败，不扩大权限".into(),
        deadline: Utc::now() + Duration::days(1),
    }
}
async fn event(pool: &PgPool, kind: &str, source: &str, actor: &str, payload: Value) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'event' FROM individuals")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,$1,$2,$3,$4,clock_timestamp(),$5,$1 FROM individuals").bind(id).bind(kind).bind(source).bind(actor).bind(payload).execute(pool).await.unwrap();
    id
}
async fn registered(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'development_verifier' FROM individuals").bind(id).execute(pool).await.unwrap();
    let registration = event(
        pool,
        "development.verifier.registered",
        "direct_observation",
        "system",
        json!({"verifier_id":id,"kind":"measurement"}),
    )
    .await;
    sqlx::query("INSERT INTO development_verifiers(individual_id,id,kind,registration_event_id) SELECT id,$1,'measurement',$2 FROM individuals").bind(id).bind(registration).execute(pool).await.unwrap();
    id
}
fn unique(mut ids: Vec<Uuid>) -> Vec<Uuid> {
    ids.sort();
    ids.dedup();
    ids
}
async fn propose_refs(pool: &PgPool, d: &DevelopmentDraft, proposer: Uuid) -> Vec<Uuid> {
    let mut ids = d.evidence_ids.clone();
    ids.extend(&d.counterevidence_ids);
    ids.push(proposer);
    let scope = if d.target == DevelopmentTarget::TaskStrategy {
        Some(
            serde_json::from_value::<TaskStrategy>(d.proposed_value.clone())
                .unwrap()
                .scope,
        )
    } else {
        None
    };
    let (k, id) = match scope {
        Some(StrategyScope::Goal(id)) => (Some("goal"), Some(id)),
        Some(StrategyScope::Task(id)) => (Some("task"), Some(id)),
        None => (None, None),
    };
    ids.extend(id);
    let name = serde_json::to_value(d.target)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let h:Option<(Uuid,Option<Uuid>)>=sqlx::query_as("SELECT id,current_revision_id FROM development_targets WHERE target=$1 AND scope_kind IS NOT DISTINCT FROM $2 AND scope_id IS NOT DISTINCT FROM $3").bind(name).bind(k).bind(id).fetch_optional(pool).await.unwrap();
    if let Some((target, head)) = h {
        ids.push(target);
        ids.extend(head)
    }
    unique(ids)
}
async fn propose(s: &Store, pool: &PgPool, d: &DevelopmentDraft) -> DevelopmentReceipt {
    let current = s.snapshot().await.unwrap().unwrap();
    s.propose_development(
        &Uuid::new_v4().to_string(),
        current.expected(),
        &propose_refs(pool, d, current.id).await,
        current.id,
        d,
    )
    .await
    .unwrap()
}
async fn refs(pool: &PgPool, id: Uuid, c: &DevelopmentCommand) -> Vec<Uuid> {
    let (target,before_head,draft,proposer,head,scope):(Uuid,Option<Uuid>,Value,Uuid,Option<Uuid>,Option<Uuid>)=sqlx::query_as("SELECT d.target_id,d.before_head,d.draft,d.proposer_id,t.current_revision_id,t.scope_id FROM developments d JOIN development_targets t ON t.individual_id=d.individual_id AND t.id=d.target_id WHERE d.id=$1").bind(id).fetch_one(pool).await.unwrap();
    let draft: DevelopmentDraft = serde_json::from_value(draft).unwrap();
    let mut ids = vec![id, target, proposer];
    ids.extend(before_head);
    ids.extend(head);
    ids.extend(scope);
    ids.extend(draft.evidence_ids);
    ids.extend(draft.counterevidence_ids);
    match c {
        DevelopmentCommand::StartTrial {
            source_event_ids, ..
        } => ids.extend(source_event_ids),
        DevelopmentCommand::RecordValidation {
            review_event_id,
            assessment,
            source_event_ids,
        } => {
            ids.push(*review_event_id);
            ids.push(assessment.reviewer_id);
            let registration: Option<Uuid> = sqlx::query_scalar(
                "SELECT registration_event_id FROM development_verifiers WHERE id=$1",
            )
            .bind(assessment.reviewer_id)
            .fetch_optional(pool)
            .await
            .unwrap();
            ids.extend(registration);
            ids.extend(source_event_ids);
            ids.extend(&assessment.reviewed_evidence_ids);
            ids.extend(&assessment.reviewed_counterevidence_ids);
            ids.extend(&assessment.counter_search_event_ids);
            for o in &assessment.observations {
                ids.extend(&o.source_event_ids);
                ids.push(o.result_event_id)
            }
        }
        _ => {}
    }
    unique(ids)
}
async fn transition(
    s: &Store,
    pool: &PgPool,
    id: Uuid,
    c: &DevelopmentCommand,
) -> DevelopmentReceipt {
    let version: i64 = sqlx::query_scalar("SELECT version FROM developments WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    s.transition_development(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        &refs(pool, id, c).await,
        id,
        version,
        c,
    )
    .await
    .unwrap()
}
async fn trial(s: &Store, pool: &PgPool, id: Uuid) -> DevelopmentReceipt {
    let draft: Value = sqlx::query_scalar("SELECT draft FROM developments WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    let d: DevelopmentDraft = serde_json::from_value(draft).unwrap();
    let mut ids = d.evidence_ids;
    ids.extend(d.counterevidence_ids);
    transition(
        s,
        pool,
        id,
        &DevelopmentCommand::StartTrial {
            route: ValidationRoute::CrossScenarioHabit,
            source_event_ids: ids,
        },
    )
    .await
}
async fn observed(
    pool: &PgPool,
    verifier: Uuid,
    trial: Option<Uuid>,
    role: &str,
    case: Option<Uuid>,
    context: Option<&str>,
    outcome: Option<&str>,
) -> Uuid {
    let payload = json!({"verifier_id":verifier,"trial_event_id":trial,"role":role,"case_id":case,"context":context,"outcome":outcome});
    let id = event(
        pool,
        "development.observed",
        "direct_observation",
        &format!("development-verifier:{verifier}"),
        payload,
    )
    .await;
    sqlx::query("INSERT INTO development_observations(individual_id,event_id,verifier_id,trial_event_id,role,case_id,context,outcome) SELECT id,$1,$2,$3,$4,$5,$6,$7 FROM individuals").bind(id).bind(verifier).bind(trial).bind(role).bind(case).bind(context).bind(outcome).execute(pool).await.unwrap();
    id
}
async fn review(pool: &PgPool, id: Uuid, verifier: Uuid, outcome: Outcome) -> DevelopmentCommand {
    review_contexts(
        pool,
        id,
        verifier,
        outcome,
        &[
            "independent task context",
            "independent relationship context",
        ],
    )
    .await
}
async fn review_contexts(
    pool: &PgPool,
    id: Uuid,
    verifier: Uuid,
    outcome: Outcome,
    contexts: &[&str],
) -> DevelopmentCommand {
    let value: Value = sqlx::query_scalar("SELECT draft FROM developments WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    let d: DevelopmentDraft = serde_json::from_value(value).unwrap();
    let trial: Uuid = sqlx::query_scalar(
        "SELECT event_id FROM development_steps WHERE development_id=$1 AND command ? 'StartTrial'",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    let search = observed(
        pool,
        verifier,
        Some(trial),
        "counter_search",
        None,
        None,
        Some("passed"),
    )
    .await;
    let mut observations = vec![];
    let mut source_ids = d.evidence_ids.clone();
    source_ids.extend(&d.counterevidence_ids);
    source_ids.push(search);
    let result = match outcome {
        Outcome::Passed => "passed",
        Outcome::Failed => "failed",
        Outcome::Unverified => "unverified",
    };
    for &context in contexts {
        let case = Uuid::new_v4();
        let result_event_id = observed(
            pool,
            verifier,
            Some(trial),
            "scenario_result",
            Some(case),
            Some(context),
            Some(result),
        )
        .await;
        source_ids.push(result_event_id);
        observations.push(ScenarioObservation {
            case_id: case,
            context: context.into(),
            source_event_ids: d.evidence_ids.clone(),
            result_event_id,
            outcome,
        });
    }
    let a = IndependentAssessment {
        review_id: Uuid::new_v4(),
        reviewer_id: verifier,
        origin: ReviewOrigin::IndependentMeasurement,
        reviewed_evidence_ids: d.evidence_ids,
        reviewed_counterevidence_ids: d.counterevidence_ids,
        counter_search_event_ids: vec![search],
        counter_review: Outcome::Passed,
        observations,
    };
    let review_event=event(pool,"development.review.recorded","direct_observation",&format!("development-verifier:{verifier}"),json!({"development_id":id,"trial_event_id":trial,"review_id":a.review_id,"verifier_id":verifier})).await;
    sqlx::query("INSERT INTO development_review_records(individual_id,event_id,development_id,trial_event_id,verifier_id,assessment) SELECT id,$1,$2,$3,$4,$5 FROM individuals").bind(review_event).bind(id).bind(trial).bind(verifier).bind(serde_json::to_value(&a).unwrap()).execute(pool).await.unwrap();
    for source in &source_ids {
        sqlx::query("INSERT INTO development_review_sources(individual_id,review_event_id,event_id) SELECT id,$1,$2 FROM individuals").bind(review_event).bind(source).execute(pool).await.unwrap();
    }
    DevelopmentCommand::RecordValidation {
        review_event_id: review_event,
        assessment: a,
        source_event_ids: unique(source_ids),
    }
}
async fn fingerprint(pool: &PgPool) -> Value {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut values = serde_json::Map::new();
    for table in tables {
        assert!(table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        let hash:String=sqlx::query_scalar(&format!("SELECT md5(COALESCE(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM \"{table}\" t")).fetch_one(pool).await.unwrap();
        values.insert(table, json!(hash));
    }
    Value::Object(values)
}

async fn version(pool: &PgPool, id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT version FROM developments WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn reopened(pool: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(3)
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated owned PostgreSQL, exact initial09B sources and global lock"]
async fn actual_review_reference_is_not_transferable_between_candidates_or_counted_twice_after_restart(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let counter = message(&s).await;
    let mut da = draft(e, DevelopmentTarget::Traits, traits(0.65));
    da.counterevidence_ids = vec![counter];
    let mut db = da.clone();
    db.proposed_value = traits(0.72);
    let a = propose(&s, &pool, &da).await;
    let b = propose(&s, &pool, &db).await;
    trial(&s, &pool, a.development_id).await;
    trial(&s, &pool, b.development_id).await;
    let verifier = registered(&pool).await;
    // Correct immutable server record exists only for A. B's request merely
    // references it; no trusted row, actor, source class or opaque token is forged.
    let review_a = review(&pool, a.development_id, verifier, Outcome::Passed).await;
    let before = fingerprint(&pool).await;
    let confused = s
        .transition_development(
            "cross-candidate-review",
            s.snapshot().await.unwrap().unwrap().expected(),
            &refs(&pool, b.development_id, &review_a).await,
            b.development_id,
            version(&pool, b.development_id).await,
            &review_a,
        )
        .await;
    assert!(matches!(
        confused,
        Err(StoreError::Invalid("missing trusted development review"))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let inputs = refs(&pool, a.development_id, &review_a).await;
    let accepted_review = s
        .transition_development(
            "record-review-exact",
            expected,
            &inputs,
            a.development_id,
            2,
            &review_a,
        )
        .await
        .unwrap();
    assert_eq!(accepted_review.development_version, 3);
    let reconnect = reopened(&pool).await;
    let restarted = Store::new(reconnect.clone());
    let before = fingerprint(&pool).await;
    assert_eq!(
        restarted
            .transition_development(
                "record-review-exact",
                expected,
                &inputs,
                a.development_id,
                2,
                &review_a
            )
            .await
            .unwrap(),
        accepted_review
    );
    assert_eq!(fingerprint(&pool).await, before);
    // A second key is a new attempt, not idempotent replay; the same review ID
    // must not become another scenario/validation after reconstruction.
    let duplicated = restarted
        .transition_development(
            "count-review-again",
            restarted.snapshot().await.unwrap().unwrap().expected(),
            &refs(&pool, a.development_id, &review_a).await,
            a.development_id,
            3,
            &review_a,
        )
        .await;
    assert!(matches!(
        duplicated,
        Err(StoreError::Invalid("development policy"))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let detail = restarted.development(a.development_id).await.unwrap();
    assert_eq!(
        detail["development"]["validations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let before = fingerprint(&pool).await;
    assert!(matches!(
        restarted
            .transition_development(
                "accept-with-other-review",
                restarted.snapshot().await.unwrap().unwrap().expected(),
                &refs(&pool, b.development_id, &DevelopmentCommand::Accept).await,
                b.development_id,
                2,
                &DevelopmentCommand::Accept
            )
            .await,
        Err(StoreError::Invalid("development policy"))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let done = transition(
        &restarted,
        &pool,
        a.development_id,
        &DevelopmentCommand::Accept,
    )
    .await;
    assert_eq!(done.status, "accepted");
    assert_eq!(
        restarted.snapshot().await.unwrap().unwrap().persona["personality"]["traits"],
        traits(0.65)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    reconnect.close().await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated owned PostgreSQL, exact initial09B sources and global lock"]
async fn competing_validated_accepts_and_failed_revert_preserve_the_current_head_across_restart(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let baseline = s.snapshot().await.unwrap().unwrap().persona;
    let e = message(&s).await;
    let counter = message(&s).await;
    let verifier = registered(&pool).await;
    let mut da = draft(e, DevelopmentTarget::Traits, traits(0.65));
    da.counterevidence_ids = vec![counter];
    let mut db = da.clone();
    db.proposed_value = traits(0.72);
    let a = propose(&s, &pool, &da).await;
    let b = propose(&s, &pool, &db).await;
    for id in [a.development_id, b.development_id] {
        trial(&s, &pool, id).await;
        let validation = review(&pool, id, verifier, Outcome::Passed).await;
        transition(&s, &pool, id, &validation).await;
    }
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let command = DevelopmentCommand::Accept;
    let refs_a = refs(&pool, a.development_id, &command).await;
    let refs_b = refs(&pool, b.development_id, &command).await;
    let (ra, rb) = tokio::join!(
        s.transition_development(
            "race-accept-a",
            expected,
            &refs_a,
            a.development_id,
            3,
            &command
        ),
        s.transition_development(
            "race-accept-b",
            expected,
            &refs_b,
            b.development_id,
            3,
            &command
        )
    );
    let (winner, loser, wanted) = match (ra, rb) {
        (Ok(a), Err(StoreError::StaleState)) => (a, b.development_id, traits(0.65)),
        (Err(StoreError::StaleState), Ok(b)) => (b, a.development_id, traits(0.72)),
        _ => panic!("expected exactly one real accepted transition and one stale contender"),
    };
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().persona["personality"]["traits"],
        wanted
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM development_revisions WHERE action='accept'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    let before = fingerprint(&pool).await;
    let retried = s
        .transition_development(
            "loser-new-global-state",
            s.snapshot().await.unwrap().unwrap().expected(),
            &refs(&pool, loser, &command).await,
            loser,
            3,
            &command,
        )
        .await;
    assert!(matches!(
        retried,
        Err(StoreError::Invalid("development policy"))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let reconnect = reopened(&pool).await;
    let restarted = Store::new(reconnect.clone());
    assert_eq!(
        restarted.development(winner.development_id).await.unwrap()["development"]["status"],
        "accepted"
    );
    assert_eq!(
        restarted.development(loser).await.unwrap()["development"]["status"],
        "trial"
    );
    let undo = DevelopmentCommand::Revert {
        reason: "实际有界撤回当前head，保留接受历史".into(),
    };
    let undo_expected = restarted.snapshot().await.unwrap().unwrap().expected();
    let undo_refs = refs(&pool, winner.development_id, &undo).await;
    // QA fault is scoped to the actual revert write; it does not forge a trial,
    // trusted reviewer record, accepted status, private token or projection.
    sqlx::raw_sql("CREATE FUNCTION p01_fail_revert_revision() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='revert' THEN RAISE EXCEPTION 'p01 synthetic revert insert fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER p01_fail_revert_revision BEFORE INSERT ON development_revisions FOR EACH ROW EXECUTE FUNCTION p01_fail_revert_revision();").execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert!(matches!(
        restarted
            .transition_development(
                "revert-exactly-once",
                undo_expected,
                &undo_refs,
                winner.development_id,
                4,
                &undo
            )
            .await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(fingerprint(&pool).await, before);
    assert_eq!(
        restarted.snapshot().await.unwrap().unwrap().persona["personality"]["traits"],
        wanted
    );
    sqlx::raw_sql("DROP TRIGGER p01_fail_revert_revision ON development_revisions; DROP FUNCTION p01_fail_revert_revision();").execute(&pool).await.unwrap();
    let reverted = restarted
        .transition_development(
            "revert-exactly-once",
            undo_expected,
            &undo_refs,
            winner.development_id,
            4,
            &undo,
        )
        .await
        .unwrap();
    assert_eq!(reverted.status, "reverted");
    assert_ne!(reverted.revision_id, winner.revision_id);
    let before = fingerprint(&pool).await;
    assert_eq!(
        restarted
            .transition_development(
                "revert-exactly-once",
                undo_expected,
                &undo_refs,
                winner.development_id,
                4,
                &undo
            )
            .await
            .unwrap(),
        reverted
    );
    assert_eq!(fingerprint(&pool).await, before);
    let current = restarted.snapshot().await.unwrap().unwrap().persona;
    assert_eq!(current, baseline);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM development_revisions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    reconnect.close().await;
}
