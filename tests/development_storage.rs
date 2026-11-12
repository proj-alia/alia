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
    let h:Option<(Uuid,Option<Uuid>)>=sqlx::query_as("SELECT id,current_revision_id FROM development_targets WHERE target=$1 AND scope_kind IS NOT DISTINCT FROM $2 AND scope_id IS NOT DISTINCT FROM $3 AND NOT provenance_removed").bind(name).bind(k).bind(id).fetch_optional(pool).await.unwrap();
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
async fn accepted(
    s: &Store,
    pool: &PgPool,
    d: &DevelopmentDraft,
    verifier: Uuid,
) -> DevelopmentReceipt {
    let r = propose(s, pool, d).await;
    trial(s, pool, r.development_id).await;
    let review = review(pool, r.development_id, verifier, Outcome::Passed).await;
    transition(s, pool, r.development_id, &review).await;
    transition(s, pool, r.development_id, &DevelopmentCommand::Accept).await
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

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn actual_apply_restart_revert_preserves_other_path_and_seed(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let before = s.snapshot().await.unwrap().unwrap();
    let a = accepted(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::Traits, traits(0.65)),
        verifier,
    )
    .await;
    let state = s.snapshot().await.unwrap().unwrap();
    assert_eq!(state.persona["personality"]["traits"], traits(0.65));
    assert_eq!(state.seed, before.seed);
    assert_eq!(state.runtime_config, before.runtime_config);
    assert_eq!(state.origin, before.origin);
    let mut expression = seed()["expression"].clone();
    expression["tone"] = json!("独立后继表达变化");
    let b = accepted(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::Expression, expression.clone()),
        verifier,
    )
    .await;
    assert_eq!(b.status, "accepted");
    let reconnect = PgPoolOptions::new()
        .max_connections(3)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let restarted = Store::new(reconnect.clone());
    let view = restarted.development(a.development_id).await.unwrap();
    assert_eq!(view["development"]["status"], "accepted");
    assert_eq!(
        view["development"]["validations"].as_array().unwrap().len(),
        1
    );
    let undo = transition(
        &restarted,
        &reconnect,
        a.development_id,
        &DevelopmentCommand::Revert {
            reason: "有新依据后撤回该目标，不抹历史".into(),
        },
    )
    .await;
    assert_eq!(undo.status, "reverted");
    assert_ne!(undo.revision_id, a.revision_id);
    let state = restarted.snapshot().await.unwrap().unwrap();
    assert_eq!(
        state.persona["personality"]["traits"],
        before.persona["personality"]["traits"]
    );
    assert_eq!(state.persona["expression"], expression);
    assert_eq!(state.seed_digest, before.seed_digest);
    let detail = restarted.development(a.development_id).await.unwrap();
    let transitions = detail["development"]["transitions"].as_array().unwrap();
    assert_eq!(transitions.len(), 4);
    assert_eq!(transitions[2]["to"], "Accepted");
    assert_eq!(transitions[3]["before_value"], traits(0.65));
    assert_eq!(
        transitions[3]["after_value"],
        seed()["personality"]["traits"]
    );
    let revisions: i64 = sqlx::query_scalar("SELECT count(*) FROM development_revisions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(revisions, 3);
    reconnect.close().await;
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn actual_idempotency_versions_and_late_failure_roll_back_projection_and_history(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let d = draft(e, DevelopmentTarget::Traits, traits(0.65));
    let snapshot = s.snapshot().await.unwrap().unwrap();
    let inputs = propose_refs(&pool, &d, snapshot.id).await;
    let p = s
        .propose_development(
            "proposal-idempotent",
            snapshot.expected(),
            &inputs,
            snapshot.id,
            &d,
        )
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    let same = s
        .propose_development(
            "proposal-idempotent",
            snapshot.expected(),
            &inputs,
            snapshot.id,
            &d,
        )
        .await
        .unwrap();
    assert_eq!(same, p);
    assert_eq!(fingerprint(&pool).await, before);
    trial(&s, &pool, p.development_id).await;
    let review = review(&pool, p.development_id, verifier, Outcome::Passed).await;
    transition(&s, &pool, p.development_id, &review).await;
    let current = s.snapshot().await.unwrap().unwrap();
    let inputs = refs(&pool, p.development_id, &DevelopmentCommand::Accept).await;
    let version: i64 = sqlx::query_scalar("SELECT version FROM developments WHERE id=$1")
        .bind(p.development_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION development_test_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic late outbox failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER development_test_fail BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION development_test_fail();").execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert!(
        s.transition_development(
            "accept-exactly-once",
            current.expected(),
            &inputs,
            p.development_id,
            version,
            &DevelopmentCommand::Accept
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().persona,
        current.persona
    );
    sqlx::query("DROP TRIGGER development_test_fail ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let a = s
        .transition_development(
            "accept-exactly-once",
            current.expected(),
            &inputs,
            p.development_id,
            version,
            &DevelopmentCommand::Accept,
        )
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(
        s.transition_development(
            "accept-exactly-once",
            current.expected(),
            &inputs,
            p.development_id,
            version,
            &DevelopmentCommand::Accept
        )
        .await
        .unwrap(),
        a
    );
    assert_eq!(fingerprint(&pool).await, before);
    assert!(matches!(
        s.transition_development(
            "accept-stale-version",
            current.expected(),
            &inputs,
            p.development_id,
            version,
            &DevelopmentCommand::Accept
        )
        .await,
        Err(StoreError::StaleState)
    ));
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn failures_unverified_reviews_and_actual_expiry_are_terminal_or_remain_unaccepted(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    for result in [Outcome::Failed, Outcome::Unverified] {
        let p = propose(
            &s,
            &pool,
            &draft(e, DevelopmentTarget::Traits, traits(0.65)),
        )
        .await;
        trial(&s, &pool, p.development_id).await;
        let failed = review(&pool, p.development_id, verifier, result).await;
        transition(&s, &pool, p.development_id, &failed).await;
        let later = review(&pool, p.development_id, verifier, Outcome::Passed).await;
        transition(&s, &pool, p.development_id, &later).await;
        let before = fingerprint(&pool).await;
        let command = DevelopmentCommand::Accept;
        let v: i64 = sqlx::query_scalar("SELECT version FROM developments WHERE id=$1")
            .bind(p.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            s.transition_development(
                "cannot-wash-failure",
                s.snapshot().await.unwrap().unwrap().expected(),
                &refs(&pool, p.development_id, &command).await,
                p.development_id,
                v,
                &command
            )
            .await
            .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
        let view = s.development(p.development_id).await.unwrap();
        assert_eq!(view["development"]["status"], "trial");
        assert_eq!(
            view["development"]["validations"].as_array().unwrap().len(),
            2
        );
        transition(
            &s,
            &pool,
            p.development_id,
            &DevelopmentCommand::Reject {
                reason: "保留失败，拒绝该提案".into(),
            },
        )
        .await;
    }
    let mut d = draft(e, DevelopmentTarget::Expression, {
        let mut v = seed()["expression"].clone();
        v["tone"] = json!("有界试用");
        v
    });
    d.deadline = Utc::now() + Duration::milliseconds(1000);
    let p = propose(&s, &pool, &d).await;
    trial(&s, &pool, p.development_id).await;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let expired = transition(&s, &pool, p.development_id, &DevelopmentCommand::Expire).await;
    assert_eq!(expired.status, "expired");
    assert_eq!(
        s.development(p.development_id).await.unwrap()["development"]["status"],
        "expired"
    );
    let before = fingerprint(&pool).await;
    let cmd = DevelopmentCommand::Accept;
    assert!(
        s.transition_development(
            "expired-cannot-accept",
            s.snapshot().await.unwrap().unwrap().expected(),
            &refs(&pool, p.development_id, &cmd).await,
            p.development_id,
            expired.development_version,
            &cmd
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn forged_identity_source_verifier_and_review_evidence_have_no_side_effects(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let current = s.snapshot().await.unwrap().unwrap();
    let d = draft(e, DevelopmentTarget::Traits, traits(0.65));
    let before = fingerprint(&pool).await;
    for proposer in [Uuid::new_v4(), Uuid::nil()] {
        assert!(
            s.propose_development(
                "bad-proposer",
                current.expected(),
                &propose_refs(&pool, &d, proposer).await,
                proposer,
                &d
            )
            .await
            .is_err()
        );
    }
    assert_eq!(fingerprint(&pool).await, before);
    for (kind, source) in [
        ("model.self_description", "generated"),
        ("synthetic.background", "fictional_background"),
        ("decision.reviewed", "inference"),
        ("affect.updated", "inference"),
        ("tool.result", "external_document"),
    ] {
        let invalid = event(
            &pool,
            kind,
            source,
            "synthetic",
            json!({"text":"cannot establish personality evidence"}),
        )
        .await;
        let d = draft(invalid, DevelopmentTarget::Traits, traits(0.65));
        let current = s.snapshot().await.unwrap().unwrap();
        let before = fingerprint(&pool).await;
        assert!(
            s.propose_development(
                "bad-source",
                current.expected(),
                &propose_refs(&pool, &d, current.id).await,
                current.id,
                &d
            )
            .await
            .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
    }
    let p = propose(&s, &pool, &d).await;
    trial(&s, &pool, p.development_id).await;
    let verifier = registered(&pool).await;
    let legitimate = review(&pool, p.development_id, verifier, Outcome::Passed).await;
    for mutation in 0..6 {
        let mut cmd = legitimate.clone();
        let DevelopmentCommand::RecordValidation {
            review_event_id,
            assessment,
            source_event_ids,
        } = &mut cmd
        else {
            unreachable!()
        };
        match mutation {
            0 => *review_event_id = Uuid::new_v4(),
            1 => assessment.reviewer_id = current.id,
            2 => assessment.origin = ReviewOrigin::AuthorSelfAssessment,
            3 => assessment.observations[0].outcome = Outcome::Failed,
            4 => {
                source_event_ids.pop();
            }
            _ => assessment.counter_search_event_ids.clear(),
        };
        let before = fingerprint(&pool).await;
        let version: i64 = sqlx::query_scalar("SELECT version FROM developments WHERE id=$1")
            .bind(p.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            s.transition_development(
                "forged-review",
                s.snapshot().await.unwrap().unwrap().expected(),
                &refs(&pool, p.development_id, &cmd).await,
                p.development_id,
                version,
                &cmd
            )
            .await
            .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
    }
    // A row labelled measurement is insufficient without a matching actual registration event.
    sqlx::query("UPDATE events SET actor_id='model' WHERE kind='development.verifier.registered'")
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(
        s.transition_development(
            "fake-registration",
            s.snapshot().await.unwrap().unwrap().expected(),
            &refs(&pool, p.development_id, &legitimate).await,
            p.development_id,
            2,
            &legitimate
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn later_target_revisions_and_aba_values_cannot_be_overwritten_by_old_revert(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let first = accepted(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::Traits, traits(0.65)),
        verifier,
    )
    .await;
    let mut second = draft(e, DevelopmentTarget::Traits, traits(0.4));
    second.before_version = 2;
    accepted(&s, &pool, &second, verifier).await;
    let mut third = draft(e, DevelopmentTarget::Traits, traits(0.65));
    third.before_version = 3;
    accepted(&s, &pool, &third, verifier).await;
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().persona["personality"]["traits"],
        traits(0.65)
    );
    let before = fingerprint(&pool).await;
    let cmd = DevelopmentCommand::Revert {
        reason: "旧提案禁止覆盖新修订，即使值相等".into(),
    };
    assert!(
        s.transition_development(
            "old-revert-aba",
            s.snapshot().await.unwrap().unwrap().expected(),
            &refs(&pool, first.development_id, &cmd).await,
            first.development_id,
            first.development_version,
            &cmd
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    let view = s.developments(false, 100).await.unwrap();
    assert_eq!(view["items"].as_array().unwrap().len(), 1);
    let view = s.developments(true, 100).await.unwrap();
    assert_eq!(view["items"].as_array().unwrap().len(), 3);
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn source_revocation_without_general_edges_blocks_detail_list_write_and_private_replay(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let d = draft(e, DevelopmentTarget::Traits, traits(0.65));
    let p = propose(&s, &pool, &d).await;
    trial(&s, &pool, p.development_id).await;
    let proof = review(&pool, p.development_id, verifier, Outcome::Passed).await;
    transition(&s, &pool, p.development_id, &proof).await;
    let command = DevelopmentCommand::Accept;
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let inputs = refs(&pool, p.development_id, &command).await;
    let a = s
        .transition_development(
            "accepted-private-replay",
            expected,
            &inputs,
            p.development_id,
            3,
            &command,
        )
        .await
        .unwrap();
    let independent = message(&s).await;
    let mut expression = seed()["expression"].clone();
    expression["tone"] = json!("独立来源的表达");
    let other = propose(
        &s,
        &pool,
        &draft(independent, DevelopmentTarget::Expression, expression),
    )
    .await;
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.development(a.development_id).await,
        Err(StoreError::NotFound)
    ));
    let list = s.developments(true, 100).await.unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["id"], json!(other.development_id));
    assert!(matches!(
        s.transition_development(
            "accepted-private-replay",
            expected,
            &inputs,
            p.development_id,
            3,
            &command
        )
        .await,
        Err(StoreError::NotFound)
    ));
    let before = fingerprint(&pool).await;
    let cmd = DevelopmentCommand::Revert {
        reason: "撤销来源不能再读写私有提案".into(),
    };
    assert!(
        s.transition_development(
            "revoked-write",
            s.snapshot().await.unwrap().unwrap().expected(),
            &refs(&pool, a.development_id, &cmd).await,
            a.development_id,
            a.development_version,
            &cmd
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    // The review registration FK is also a provenance dependency without general edges.
    sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    let registration: Uuid =
        sqlx::query_scalar("SELECT registration_event_id FROM development_verifiers WHERE id=$1")
            .bind(verifier)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(registration)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.development(a.development_id).await,
        Err(StoreError::NotFound)
    ));
    assert!(s.development(other.development_id).await.is_ok());
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn significant_experience_can_change_values_after_one_actual_independent_scenario(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let verifier = registered(&pool).await;
    let significant = observed(&pool, verifier, None, "significant_event", None, None, None).await;
    let values = json!([{"id":"respect","description":"对明确边界赋予更高优先级","priority":1.0},{"id":"truthfulness","description":"保持独立事实核查","priority":0.6}]);
    let d = draft(significant, DevelopmentTarget::Values, values.clone());
    let p = propose(&s, &pool, &d).await;
    transition(
        &s,
        &pool,
        p.development_id,
        &DevelopmentCommand::StartTrial {
            route: ValidationRoute::SignificantEvent {
                event_id: significant,
            },
            source_event_ids: vec![significant],
        },
    )
    .await;
    let proof = review_contexts(
        &pool,
        p.development_id,
        verifier,
        Outcome::Passed,
        &["independent significant experience check"],
    )
    .await;
    transition(&s, &pool, p.development_id, &proof).await;
    let a = transition(&s, &pool, p.development_id, &DevelopmentCommand::Accept).await;
    assert_eq!(a.status, "accepted");
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().persona["personality"]["values"],
        values
    );
    let view = s.development(p.development_id).await.unwrap();
    assert_eq!(
        view["development"]["validations"][0]["assessment"]["observations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn scoped_strategy_requires_actual_goal_or_task_and_is_separate_from_persona(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let before = s.snapshot().await.unwrap().unwrap();
    let goal = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'goal' FROM individuals")
        .bind(goal)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,description,success_criterion,next_step,stop_condition) SELECT id,$1,'event','合成独立目标','有结果','测试','无依据停止' FROM individuals").bind(goal).execute(&pool).await.unwrap();
    let strategy =
        json!({"scope":{"kind":"goal","id":goal},"description":"先核查目标资料，再尝试有界动作"});
    let a = accepted(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::TaskStrategy, strategy.clone()),
        verifier,
    )
    .await;
    assert_eq!(s.snapshot().await.unwrap().unwrap().persona, before.persona);
    let saved: Value =
        sqlx::query_scalar("SELECT value FROM development_targets WHERE target='task_strategy'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved, strategy);
    transition(
        &s,
        &pool,
        a.development_id,
        &DevelopmentCommand::Revert {
            reason: "撤回该目标策略".into(),
        },
    )
    .await;
    let value: Value =
        sqlx::query_scalar("SELECT value FROM development_targets WHERE target='task_strategy'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(value.is_null());
    let unknown = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'goal' FROM individuals")
        .bind(unknown)
        .execute(&pool)
        .await
        .unwrap();
    let invalid = draft(
        e,
        DevelopmentTarget::TaskStrategy,
        json!({"scope":{"kind":"goal","id":unknown},"description":"对象存在但没有实际goal行"}),
    );
    let current = s.snapshot().await.unwrap().unwrap();
    let before = fingerprint(&pool).await;
    assert!(
        s.propose_development(
            "not-actual-goal",
            current.expected(),
            &propose_refs(&pool, &invalid, current.id).await,
            current.id,
            &invalid
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    let run = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'run' FROM individuals")
        .bind(run)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version) SELECT id,$1,$2,'background',state_version,privacy_epoch,config_version FROM individuals").bind(run).bind(e).execute(&pool).await.unwrap();
    let task = draft(
        e,
        DevelopmentTarget::TaskStrategy,
        json!({"scope":{"kind":"task","id":run},"description":"仅对该持久task/run适用"}),
    );
    let receipt = propose(&s, &pool, &task).await;
    assert_eq!(
        s.development(receipt.development_id).await.unwrap()["development"]["scope"],
        json!({"kind":"task","id":run})
    );
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn list_filters_before_limit_reads_are_side_effect_free_and_restoration_rejects_tampering(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let d = draft(e, DevelopmentTarget::Traits, traits(0.65));
    let mut good = vec![];
    for _ in 0..103 {
        good.push(propose(&s, &pool, &d).await.development_id)
    }
    let hidden = message(&s).await;
    for _ in 0..3 {
        propose(
            &s,
            &pool,
            &draft(hidden, DevelopmentTarget::Traits, traits(0.7)),
        )
        .await;
    }
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(hidden)
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    let view = s.developments(true, 100).await.unwrap();
    assert_eq!(view["items"].as_array().unwrap().len(), 100);
    assert_eq!(view["has_more"], true);
    assert_eq!(
        s.developments(true, 1).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(s.development(good[0]).await.is_ok());
    for n in [0, 101] {
        assert!(s.developments(true, n).await.is_err())
    }
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("UPDATE developments SET status='accepted' WHERE id=$1")
        .bind(good[0])
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.development(good[0]).await.is_err());
    sqlx::query("UPDATE developments SET status='proposed',draft=jsonb_set(draft,'{proposed_value,curiosity}','0.5') WHERE id=$1").bind(good[0]).execute(&pool).await.unwrap();
    assert!(s.development(good[0]).await.is_err());
    s.fence_privacy();
    assert!(matches!(
        s.developments(true, 1).await,
        Err(StoreError::PrivacyUnavailable)
    ));
    assert!(matches!(
        s.development(good[1]).await,
        Err(StoreError::PrivacyUnavailable)
    ));
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn late_proposal_failure_stale_epoch_and_missing_manifest_leave_no_rows(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let d = draft(e, DevelopmentTarget::Traits, traits(0.65));
    let current = s.snapshot().await.unwrap().unwrap();
    let inputs = propose_refs(&pool, &d, current.id).await;
    sqlx::raw_sql("CREATE FUNCTION development_propose_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic proposal rollback'; END IF; RETURN NEW; END $$; CREATE TRIGGER development_propose_fail BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION development_propose_fail();").execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert!(
        s.propose_development(
            "propose-late-rollback",
            current.expected(),
            &inputs,
            current.id,
            &d
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("DROP TRIGGER development_propose_fail ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let p = s
        .propose_development(
            "propose-late-rollback",
            current.expected(),
            &inputs,
            current.id,
            &d,
        )
        .await
        .unwrap();
    let cmd = DevelopmentCommand::StartTrial {
        route: ValidationRoute::CrossScenarioHabit,
        source_event_ids: vec![e],
    };
    let expected = s.snapshot().await.unwrap().unwrap().expected();
    let inputs = refs(&pool, p.development_id, &cmd).await;
    let before = fingerprint(&pool).await;
    let mut stale = expected;
    stale.privacy_epoch += 1;
    assert!(matches!(
        s.transition_development("stale-epoch", stale, &inputs, p.development_id, 1, &cmd)
            .await,
        Err(StoreError::StaleState)
    ));
    assert!(
        s.transition_development(
            "missing-manifest",
            expected,
            &[e],
            p.development_id,
            1,
            &cmd
        )
        .await
        .is_err()
    );
    assert!(matches!(
        s.transition_development(
            "wrong-object-version",
            expected,
            &inputs,
            p.development_id,
            2,
            &cmd
        )
        .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn scope_origin_and_run_input_fks_block_ancestor_revocation_without_edges(pool: PgPool) {
    let s = setup(&pool).await;
    let private = message(&s).await;
    let public = message(&s).await;
    let goal = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'goal' FROM individuals")
        .bind(goal)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition) SELECT id,$1,'event',$2,'源归属测试','实际','检查','停止' FROM individuals").bind(goal).bind(json!([private])).execute(&pool).await.unwrap();
    let run = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'run' FROM individuals")
        .bind(run)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,input_manifest,mode,state_version,privacy_epoch,config_version) SELECT id,$1,$2,$3,'background',state_version,privacy_epoch,config_version FROM individuals").bind(run).bind(public).bind(vec![private]).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(private)
        .execute(&pool)
        .await
        .unwrap();
    for scope in [StrategyScope::Goal(goal), StrategyScope::Task(run)] {
        let d = draft(
            public,
            DevelopmentTarget::TaskStrategy,
            json!({"scope":scope,"description":"公开陈述不能洗白scope私有祖先"}),
        );
        let current = s.snapshot().await.unwrap().unwrap();
        let before = fingerprint(&pool).await;
        assert!(matches!(
            s.propose_development(
                "scope-private-ancestor",
                current.expected(),
                &propose_refs(&pool, &d, current.id).await,
                current.id,
                &d
            )
            .await,
            Err(StoreError::NotFound)
        ));
        assert_eq!(fingerprint(&pool).await, before);
    }
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL and total lock required"]
async fn concurrent_expected_state_and_mid_transaction_privacy_fence_fail_closed(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let d = draft(e, DevelopmentTarget::Traits, traits(0.65));
    let current = s.snapshot().await.unwrap().unwrap();
    let inputs = propose_refs(&pool, &d, current.id).await;
    let (a, b) = tokio::join!(
        s.propose_development("concurrent-a", current.expected(), &inputs, current.id, &d),
        s.propose_development("concurrent-b", current.expected(), &inputs, current.id, &d)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(a, Err(StoreError::StaleState)) || matches!(b, Err(StoreError::StaleState)));
    sqlx::raw_sql("CREATE FUNCTION development_fence_wait() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.3); RETURN NEW; END $$; CREATE TRIGGER development_fence_wait BEFORE INSERT ON development_steps FOR EACH ROW EXECUTE FUNCTION development_fence_wait();").execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    let current = s.snapshot().await.unwrap().unwrap();
    let inputs = propose_refs(&pool, &d, current.id).await;
    let cloned = s.clone();
    let expected = current.expected();
    let id = current.id;
    let writer = tokio::spawn(async move {
        cloned
            .propose_development("fence-during-write", expected, &inputs, id, &d)
            .await
    });
    let mut observed = false;
    for _ in 0..200 {
        let sleeping:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event='PgSleep')").fetch_one(&pool).await.unwrap();
        if sleeping {
            observed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        observed,
        "must observe actual in-flight database mutation before fencing"
    );
    s.fence_privacy();
    assert!(matches!(
        writer.await.unwrap(),
        Err(StoreError::PrivacyUnavailable)
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

async fn wait_for_advisory(pool: &PgPool, count: i64) {
    for _ in 0..200 {
        let waiting:i64=sqlx::query_scalar("SELECT count(*) FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE a.datname=current_database() AND l.locktype='advisory' AND NOT l.granted").fetch_one(pool).await.unwrap();
        if waiting >= count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("expected actual advisory-locked owner writers");
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL required"]
async fn owner_revert_replays_stable_intent_after_lost_response_and_global_changes(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let a = accepted(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::Traits, traits(0.65)),
        verifier,
    )
    .await;
    // Existing internal key is a different namespace; owner intent remains independent.
    let expression = {
        let mut v = seed()["expression"].clone();
        v["tone"] = json!("independent internal proposal");
        v
    };
    let d = draft(e, DevelopmentTarget::Expression, expression);
    let current = s.snapshot().await.unwrap().unwrap();
    s.propose_development(
        "stable-owner-key",
        current.expected(),
        &propose_refs(&pool, &d, current.id).await,
        current.id,
        &d,
    )
    .await
    .unwrap();
    let r = s
        .owner_revert_development(
            "stable-owner-key",
            a.development_id,
            a.development_version,
            "owner requested reversal",
        )
        .await
        .unwrap();
    assert_eq!(r.status, "reverted");
    assert_eq!(r.development_version, a.development_version + 1);
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().persona["personality"]["traits"],
        seed()["personality"]["traits"]
    );
    message(&s).await;
    let before = fingerprint(&pool).await;
    assert_eq!(
        s.owner_revert_development(
            "stable-owner-key",
            a.development_id,
            a.development_version,
            "owner requested reversal"
        )
        .await
        .unwrap(),
        r
    );
    assert_eq!(fingerprint(&pool).await, before);
    for (id, version, reason) in [
        (
            a.development_id,
            a.development_version + 1,
            "owner requested reversal",
        ),
        (a.development_id, a.development_version, "changed reason"),
        (
            Uuid::new_v4(),
            a.development_version,
            "owner requested reversal",
        ),
    ] {
        assert!(matches!(
            s.owner_revert_development("stable-owner-key", id, version, reason)
                .await,
            Err(StoreError::IdempotencyConflict)
        ));
    }
    assert_eq!(fingerprint(&pool).await, before);
    let records: i64 =
        sqlx::query_scalar("SELECT count(*) FROM idempotency_records WHERE key='stable-owner-key'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(records, 2);
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.owner_revert_development(
            "stable-owner-key",
            a.development_id,
            a.development_version,
            "owner requested reversal"
        )
        .await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL required"]
async fn owner_revert_receipt_and_mutation_roll_back_together_and_concurrent_retry_is_exactly_once(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let a = accepted(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::Traits, traits(0.65)),
        verifier,
    )
    .await;
    sqlx::raw_sql("CREATE FUNCTION owner_revert_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic owner revert rollback'; END IF; RETURN NEW; END $$; CREATE TRIGGER owner_revert_fail BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION owner_revert_fail();").execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert!(
        s.owner_revert_development(
            "owner-atomic",
            a.development_id,
            a.development_version,
            "atomic owner reversal"
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("DROP TRIGGER owner_revert_fail ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
        .execute(&mut *gate)
        .await
        .unwrap();
    let mut writers = vec![];
    for _ in 0..2 {
        let cloned = s.clone();
        let id = a.development_id;
        let v = a.development_version;
        writers.push(tokio::spawn(async move {
            cloned
                .owner_revert_development("owner-atomic", id, v, "atomic owner reversal")
                .await
        }));
    }
    wait_for_advisory(&pool, 2).await;
    gate.commit().await.unwrap();
    let first = writers.remove(0).await.unwrap().unwrap();
    let second = writers.remove(0).await.unwrap().unwrap();
    assert_eq!(first, second);
    let (receipts,revisions):(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM idempotency_records WHERE actor_id='owner' AND key='owner-atomic'),(SELECT count(*) FROM development_revisions WHERE action='revert')").fetch_one(&pool).await.unwrap();
    assert_eq!((receipts, revisions), (1, 1));
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL required"]
async fn owner_revert_does_not_retry_cas_after_rr_planning_or_trust_old_head(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let a = accepted(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::Traits, traits(0.65)),
        verifier,
    )
    .await;
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
        .execute(&mut *gate)
        .await
        .unwrap();
    let cloned = s.clone();
    let id = a.development_id;
    let version = a.development_version;
    let writer = tokio::spawn(async move {
        cloned
            .owner_revert_development("owner-raced-state", id, version, "do not silently retry")
            .await
    });
    wait_for_advisory(&pool, 1).await;
    sqlx::query("UPDATE individuals SET state_version=state_version+1")
        .execute(&mut *gate)
        .await
        .unwrap();
    gate.commit().await.unwrap();
    let before = fingerprint(&pool).await;
    assert!(matches!(writer.await.unwrap(), Err(StoreError::StaleState)));
    assert_eq!(fingerprint(&pool).await, before);
    let mut successor = draft(e, DevelopmentTarget::Traits, traits(0.4));
    successor.before_version = 2;
    accepted(&s, &pool, &successor, verifier).await;
    let mut aba = draft(e, DevelopmentTarget::Traits, traits(0.65));
    aba.before_version = 3;
    accepted(&s, &pool, &aba, verifier).await;
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.owner_revert_development(
            "owner-old-head",
            a.development_id,
            a.development_version,
            "must not overwrite later revisions"
        )
        .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P04-09B isolated actual PostgreSQL required"]
async fn owner_revert_limits_complete_provenance_and_rejects_invalid_requests_without_writes(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let verifier = registered(&pool).await;
    let a = accepted(
        &s,
        &pool,
        &draft(e, DevelopmentTarget::Traits, traits(0.65)),
        verifier,
    )
    .await;
    for (id, v, reason) in [
        (Uuid::nil(), a.development_version, "reason"),
        (a.development_id, 0, "reason"),
        (a.development_id, a.development_version, " "),
        (a.development_id, a.development_version, "bad\0reason"),
    ] {
        let before = fingerprint(&pool).await;
        assert!(
            s.owner_revert_development("owner-invalid", id, v, reason)
                .await
                .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
    }
    let long = "x".repeat(2001);
    assert!(
        s.owner_revert_development("owner-long", a.development_id, a.development_version, &long)
            .await
            .is_err()
    );
    assert!(matches!(
        s.owner_revert_development(
            "owner-unknown",
            Uuid::new_v4(),
            a.development_version,
            "reason"
        )
        .await,
        Err(StoreError::NotFound)
    ));
    let ids:Vec<Uuid>=sqlx::query_scalar("INSERT INTO objects(individual_id,id,kind) SELECT id,gen_random_uuid(),'event' FROM individuals CROSS JOIN generate_series(1,4097) RETURNING id").fetch_all(&pool).await.unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT individual_id,id,'synthetic.owner.provenance','reported_claim','synthetic',clock_timestamp(),'{}',id FROM objects WHERE id=ANY($1)").bind(&ids).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO development_dependencies(individual_id,object_id,source_id) SELECT id,$1,unnest($2::uuid[]) FROM individuals").bind(a.development_id).bind(&ids).execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.owner_revert_development(
            "owner-too-many-inputs",
            a.development_id,
            a.development_version,
            "must not truncate provenance"
        )
        .await,
        Err(StoreError::Invalid(
            "owner development provenance exceeds limit"
        ))
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

use alia::adapters::deletion_journal::DeletionJournal;
use std::{fs, os::unix::fs::DirBuilderExt, path::PathBuf, sync::Arc};
struct PrivateJournal {
    dir: PathBuf,
    journal: Arc<DeletionJournal>,
}
impl PrivateJournal {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("alia-p04-development-removal-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let journal = Arc::new(DeletionJournal::open(&dir.join("deletions.jsonl")).unwrap());
        Self { dir, journal }
    }
}
impl Drop for PrivateJournal {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
async fn private_memory(pool: &PgPool, source: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'memory' FROM individuals",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO memories(individual_id,id,kind,content,scope) SELECT id,$1,'semantic','synthetic private development source','private' FROM individuals").bind(id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,$1,$2 FROM individuals").bind(id).bind(source).execute(pool).await.unwrap();
    id
}
async fn remove_development(s: &Store, pool: &PgPool, source: Uuid, j: &PrivateJournal) {
    let memory = private_memory(pool, source).await;
    s.begin_memory_deletion(&Uuid::new_v4().to_string(), memory, 1, j.journal.clone())
        .await
        .unwrap();
    assert!(s.privacy_ready());
    s.purge_privacy_operations(16).await.unwrap();
}

#[sqlx::test]
#[ignore = "P04-10 isolated actual schema29 PostgreSQL required"]
async fn removed_generation_renews_seed_baseline_accepts_and_survives_restart_and_old_journal(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = PrivateJournal::new();
    let source = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"P04-10-PRIVATE-OLD"}),
    )
    .await;
    let verifier = registered(&pool).await;
    let old = accepted(
        &s,
        &pool,
        &draft(source, DevelopmentTarget::Traits, traits(0.31)),
        verifier,
    )
    .await;
    let oldhead: Uuid = sqlx::query_scalar("SELECT target_id FROM developments WHERE id=$1")
        .bind(old.development_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    remove_development(&s, &pool, source, &j).await;
    assert_eq!(s.snapshot().await.unwrap().unwrap().persona, seed());
    let status: String =
        sqlx::query_scalar("SELECT status FROM developments WHERE id=$1 AND provenance_removed")
            .bind(old.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "accepted");
    assert!(s.development(old.development_id).await.is_err());
    assert!(
        s.owner_revert_development(
            "old-removed-owner",
            old.development_id,
            old.development_version,
            "cannot revive removed history"
        )
        .await
        .is_err()
    );
    let fresh = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"P04-10-LEGAL-NEW"}),
    )
    .await;
    let mut d = draft(fresh, DevelopmentTarget::Traits, traits(0.66));
    d.before_version = 3;
    let current = s.snapshot().await.unwrap().unwrap();
    let inputs = vec![current.id, fresh];
    let p = s
        .propose_development(
            "new-generation-idempotent",
            current.expected(),
            &inputs,
            current.id,
            &d,
        )
        .await
        .unwrap();
    let (newhead, base, value): (Uuid, i64, Value) = sqlx::query_as(
        "SELECT id,version,value FROM development_targets WHERE NOT provenance_removed",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(newhead, oldhead);
    assert_eq!(base, 3);
    assert_eq!(value, seed()["personality"]["traits"]);
    let old_dependency:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_dependencies dep JOIN objects o ON o.individual_id=dep.individual_id AND o.id=dep.source_id WHERE dep.object_id=ANY($1) AND (o.blocked OR dep.source_id=$2))").bind(vec![newhead,p.development_id]).bind(oldhead).fetch_one(&pool).await.unwrap();
    assert!(!old_dependency);
    trial(&s, &pool, p.development_id).await;
    let proof = review(
        &pool,
        p.development_id,
        registered(&pool).await,
        Outcome::Passed,
    )
    .await;
    transition(&s, &pool, p.development_id, &proof).await;
    transition(&s, &pool, p.development_id, &DevelopmentCommand::Accept).await;
    assert_eq!(
        s.propose_development(
            "new-generation-idempotent",
            current.expected(),
            &inputs,
            current.id,
            &d
        )
        .await
        .unwrap(),
        p
    );
    let before = fingerprint(&pool).await;
    let reopened = Store::new(pool.clone());
    reopened
        .replay_deletion_journal(j.journal.clone())
        .await
        .unwrap();
    assert_eq!(fingerprint(&pool).await, before);
    assert_eq!(
        reopened.snapshot().await.unwrap().unwrap().persona["personality"]["traits"],
        traits(0.66)
    );
    assert!(reopened.development(p.development_id).await.is_ok());
    assert!(reopened.development(old.development_id).await.is_err());
    let blocked: bool = sqlx::query_scalar("SELECT blocked FROM objects WHERE id=$1")
        .bind(oldhead)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(blocked);
    let export = String::from_utf8(
        reopened
            .export_individual()
            .await
            .unwrap()
            .to_json_bytes()
            .unwrap(),
    )
    .unwrap();
    assert!(export.contains("P04-10-LEGAL-NEW"));
    assert!(!export.contains("P04-10-PRIVATE-OLD"));
    assert!(!export.contains(&oldhead.to_string()));
}

#[sqlx::test]
#[ignore = "P04-10 isolated actual schema29 PostgreSQL required"]
async fn removed_flags_alone_fail_closed_for_reads_writes_and_cached_receipts(pool: PgPool) {
    let s = setup(&pool).await;
    let source = message(&s).await;
    let verifier = registered(&pool).await;
    let a = accepted(
        &s,
        &pool,
        &draft(source, DevelopmentTarget::Traits, traits(0.65)),
        verifier,
    )
    .await;
    let cached = s
        .owner_revert_development(
            "removed-flag-cached",
            a.development_id,
            a.development_version,
            "cache must still verify provenance",
        )
        .await
        .unwrap();
    for table in [
        "developments",
        "development_targets",
        "development_revisions",
    ] {
        let id = match table {
            "developments" => a.development_id,
            "development_targets" => {
                sqlx::query_scalar("SELECT target_id FROM developments WHERE id=$1")
                    .bind(a.development_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
            }
            _ => a.revision_id.unwrap(),
        };
        sqlx::query(&format!(
            "UPDATE {table} SET provenance_removed=true WHERE id=$1"
        ))
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let blocked: bool = sqlx::query_scalar("SELECT blocked FROM objects WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!blocked, "flag itself must reject even before object block");
        let before = fingerprint(&pool).await;
        assert!(matches!(
            s.development(a.development_id).await,
            Err(StoreError::NotFound)
        ));
        assert!(
            s.developments(true, 100).await.unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            s.owner_revert_development(
                "removed-flag-owner",
                a.development_id,
                cached.development_version,
                "removed is not accepted"
            )
            .await
            .is_err()
        );
        assert!(matches!(
            s.owner_revert_development(
                "removed-flag-cached",
                a.development_id,
                a.development_version,
                "cache must still verify provenance"
            )
            .await,
            Err(StoreError::NotFound)
        ));
        assert_eq!(fingerprint(&pool).await, before);
        sqlx::query(&format!(
            "UPDATE {table} SET provenance_removed=false WHERE id=$1"
        ))
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
}

#[sqlx::test]
#[ignore = "P04-10 isolated actual schema29 PostgreSQL required"]
async fn new_generation_baseline_rejects_wrong_counter_value_seed_and_competing_version(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = PrivateJournal::new();
    let source = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"old generation for baseline proof"}),
    )
    .await;
    let old = accepted(
        &s,
        &pool,
        &draft(source, DevelopmentTarget::Traits, traits(0.4)),
        registered(&pool).await,
    )
    .await;
    remove_development(&s, &pool, source, &j).await;
    sqlx::query("UPDATE development_targets SET version=1 WHERE provenance_removed")
        .execute(&pool)
        .await
        .unwrap();
    let fresh = event(
        &pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":"new baseline proof"}),
    )
    .await;
    let mut d = draft(fresh, DevelopmentTarget::Traits, traits(0.65));
    let current = s.snapshot().await.unwrap().unwrap();
    let inputs = vec![current.id, fresh];
    let before = fingerprint(&pool).await;
    assert!(
        s.propose_development(
            "wrong-baseline-reset",
            current.expected(),
            &inputs,
            current.id,
            &d
        )
        .await
        .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    d.before_version = 3;
    let (a, b) = tokio::join!(
        s.propose_development(
            "new-generation-race-a",
            current.expected(),
            &inputs,
            current.id,
            &d
        ),
        s.propose_development(
            "new-generation-race-b",
            current.expected(),
            &inputs,
            current.id,
            &d
        )
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(a, Err(StoreError::StaleState)) || matches!(b, Err(StoreError::StaleState)));
    let p = a.or(b).unwrap();
    let head: Uuid = sqlx::query_scalar("SELECT target_id FROM developments WHERE id=$1")
        .bind(p.development_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    for n in 0..4 {
        match n {
            0 => {
                sqlx::query("UPDATE development_targets SET version=1 WHERE id=$1")
                    .bind(head)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            1 => {
                sqlx::query("UPDATE development_targets SET version=3,value=$2 WHERE id=$1")
                    .bind(head)
                    .bind(traits(0.2))
                    .execute(&pool)
                    .await
                    .unwrap();
                sqlx::query(
                    "UPDATE individuals SET persona=jsonb_set(persona,'{personality,traits}',$1)",
                )
                .bind(traits(0.2))
                .execute(&pool)
                .await
                .unwrap();
            }
            2 => {
                sqlx::query("UPDATE development_targets SET value=$2 WHERE id=$1")
                    .bind(head)
                    .bind(seed()["personality"]["traits"].clone())
                    .execute(&pool)
                    .await
                    .unwrap();
                sqlx::query("UPDATE individuals SET persona=seed,seed_digest=repeat('0',64)")
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            _ => {
                let digest=sqlx::query_scalar::<_,String>("SELECT payload->>'seed_digest' FROM events WHERE kind='individual.created' ORDER BY sequence LIMIT 1").fetch_one(&pool).await.unwrap();
                sqlx::query("UPDATE individuals SET seed_digest=$1")
                    .bind(digest)
                    .execute(&pool)
                    .await
                    .unwrap();
                sqlx::query("UPDATE events SET actor_id='model' WHERE kind='individual.created'")
                    .execute(&pool)
                    .await
                    .unwrap();
            }
        }
        let before = fingerprint(&pool).await;
        assert!(s.development(p.development_id).await.is_err());
        assert_eq!(fingerprint(&pool).await, before);
    }
    let old_status: String = sqlx::query_scalar("SELECT status FROM developments WHERE id=$1")
        .bind(old.development_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old_status, "accepted");
}
