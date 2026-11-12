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
use sqlx::PgPool;
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

use alia::adapters::deletion_journal::DeletionJournal;
use std::{collections::BTreeMap, fs, os::unix::fs::DirBuilderExt, path::PathBuf, sync::Arc};
struct Journal {
    dir: PathBuf,
    journal: Arc<DeletionJournal>,
}
impl Journal {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("alia-p05-export-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let journal = Arc::new(DeletionJournal::open(&dir.join("deletions.jsonl")).unwrap());
        Self { dir, journal }
    }
}
impl Drop for Journal {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
async fn object(pool: &PgPool, kind: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(id).bind(kind).execute(pool).await.unwrap();
    id
}
async fn memory(pool: &PgPool, source: Uuid, text: &str) -> Uuid {
    let id = object(pool, "memory").await;
    sqlx::query("INSERT INTO memories(individual_id,id,kind,content,scope) SELECT id,$1,'semantic',$2,'general' FROM individuals WHERE singleton").bind(id).bind(text).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(id).bind(source).execute(pool).await.unwrap();
    id
}
async fn fingerprint(pool: &PgPool) -> BTreeMap<String, String> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname=current_schema() ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut hashes = BTreeMap::new();
    for table in tables {
        assert!(
            table
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        );
        let hash:String=sqlx::query_scalar(&format!("SELECT md5(coalesce(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM {table} t")).fetch_one(pool).await.unwrap();
        hashes.insert(table, hash);
    }
    hashes
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn owner_future_contract_after_actual_delete_allows_fresh_message(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let old: Uuid =
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let input = s
        .submit_message("owner-before", "PRIVATE-OWNER-HISTORY", None)
        .await
        .unwrap();
    let target = memory(&pool, input.event_id, "PRIVATE-OWNER-HISTORY").await;
    s.begin_memory_deletion("owner-delete", target, 1, j.journal.clone())
        .await
        .unwrap();
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 1);
    let next = s
        .submit_message("owner-after", "NEW-LEGAL-EXPERIENCE", None)
        .await;
    assert!(
        next.is_ok(),
        "new legal interaction failed after deleting old relationship: {next:?}"
    );
    let new: Uuid =
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(old, new);
    let blocked: bool = sqlx::query_scalar("SELECT blocked FROM objects WHERE id=$1")
        .bind(old)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(blocked);
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    let after: Uuid =
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, new);
}
async fn private_source(pool: &PgPool, label: &str) -> Uuid {
    event(
        pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":label}),
    )
    .await
}
async fn private_accepted(
    s: &Store,
    pool: &PgPool,
    label: &str,
) -> (Uuid, Uuid, DevelopmentReceipt) {
    let source = private_source(pool, label).await;
    let verifier = registered(pool).await;
    let d = draft(source, DevelopmentTarget::Traits, traits(0.31));
    let receipt = accepted(s, pool, &d, verifier).await;
    let memory = memory(pool, source, label).await;
    (source, memory, receipt)
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_begin_rebuilds_persona_actual_fk_without_derivation_edges(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let (_, target, private) = private_accepted(&s, &pool, "DEVELOPMENT-PRIVATE").await;
    let independent = private_source(&pool, "PUBLIC-EXPRESSION-EVIDENCE").await;
    let verifier = registered(&pool).await;
    let mut expression = seed()["expression"].clone();
    expression["tone"] = json!("独立保留的温和语气");
    let public = accepted(
        &s,
        &pool,
        &draft(
            independent,
            DevelopmentTarget::Expression,
            expression.clone(),
        ),
        verifier,
    )
    .await;
    let old = s.snapshot().await.unwrap().unwrap();
    assert_eq!(old.persona["personality"]["traits"], traits(0.31));
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    let private_ids = s
        .memory_deletion_closure(target, 1)
        .await
        .unwrap()
        .object_ids;
    sqlx::query(
        "DELETE FROM development_dependencies WHERE object_id=ANY($1) OR source_id=ANY($1)",
    )
    .bind(&private_ids)
    .execute(&pool)
    .await
    .unwrap();
    let before = fingerprint(&pool).await;
    let r = s
        .begin_memory_deletion("dev-begin", target, 1, j.journal.clone())
        .await
        .unwrap();
    assert!(s.privacy_ready());
    let current = s.snapshot().await.unwrap().unwrap();
    assert_eq!(current.id, old.id);
    assert_eq!(current.seed, old.seed);
    assert_eq!(current.runtime_config, old.runtime_config);
    assert_eq!(
        current.persona["personality"]["traits"],
        seed()["personality"]["traits"]
    );
    assert_eq!(current.persona["expression"], expression);
    assert!(s.development(private.development_id).await.is_err());
    assert!(s.development(public.development_id).await.is_ok());
    let removed: (bool, String) =
        sqlx::query_as("SELECT provenance_removed,status FROM developments WHERE id=$1")
            .bind(private.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(removed, (true, "accepted".into()));
    assert!(matches!(
        s.export_individual().await,
        Err(StoreError::PrivacyUnavailable)
    ));
    let same = s
        .begin_memory_deletion("dev-begin", target, 1, j.journal.clone())
        .await
        .unwrap();
    assert_eq!(r, same);
    assert_ne!(fingerprint(&pool).await, before);
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 1);
    for table in [
        "events",
        "memories",
        "development_targets",
        "developments",
        "development_steps",
        "development_revisions",
        "development_review_records",
        "development_observations",
    ] {
        let private_left:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {table} t WHERE to_jsonb(t)::text LIKE '%DEVELOPMENT-PRIVATE%')")).fetch_one(&pool).await.unwrap();
        assert!(!private_left, "private body in {table}");
    }
    let archived = s.export_individual().await.unwrap();
    let schema_version: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(matches!(schema_version, 29 | 30));
    assert_eq!(
        archived.tables.len(),
        if schema_version == 30 { 61 } else { 56 }
    );
    let text = String::from_utf8(archived.to_json_bytes().unwrap()).unwrap();
    assert!(!text.contains("DEVELOPMENT-PRIVATE"));
    assert!(!text.contains(&private.development_id.to_string()));
    assert!(text.contains(&public.development_id.to_string()));
    for table in [
        "development_targets",
        "developments",
        "development_evidence",
        "development_steps",
        "development_revisions",
        "development_dependencies",
        "development_verifiers",
        "development_observations",
        "development_review_records",
        "development_review_sources",
    ] {
        assert!(
            !archived.tables[table].is_empty(),
            "missing actual legal family table {table}"
        );
    }
    let after = fingerprint(&pool).await;
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert_eq!(after, fingerprint(&pool).await);
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_completed_restore_rebuilds_and_cleans_again(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let (_, target, receipt) = private_accepted(&s, &pool, "RESTORED-PRIVATE").await;
    let (head, oldvalue): (Uuid, Value) =
        sqlx::query_as("SELECT target_id,before_value FROM developments WHERE id=$1")
            .bind(receipt.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    s.begin_memory_deletion("restore-delete", target, 1, j.journal.clone())
        .await
        .unwrap();
    s.purge_privacy_operations(16).await.unwrap();
    // Simulate a managed restore containing a completed ledger but restored bodies/
    // cached persona. The independent real journal remains authoritative.
    sqlx::query("UPDATE developments SET draft=jsonb_build_object('private','RESTORED-PRIVATE'),before_value=$2,provenance_removed=false WHERE id=$1").bind(receipt.development_id).bind(oldvalue).execute(&pool).await.unwrap();
    sqlx::query("UPDATE development_targets SET value=$2,provenance_removed=false WHERE id=$1")
        .bind(head)
        .bind(traits(0.31))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE individuals SET persona=jsonb_set(persona,'{personality,traits}',$1)")
        .bind(traits(0.31))
        .execute(&pool)
        .await
        .unwrap();
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert!(s.privacy_ready());
    assert_eq!(s.snapshot().await.unwrap().unwrap().persona, seed());
    let clean: (bool, Value) =
        sqlx::query_as("SELECT provenance_removed,draft FROM developments WHERE id=$1")
            .bind(receipt.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(clean, (true, json!({})));
    assert!(s.export_individual().await.is_ok());
    let stable = fingerprint(&pool).await;
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert_eq!(stable, fingerprint(&pool).await);
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_unproven_head_invalid_seed_unknown_schema_fail_closed(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let (_, target, receipt) = private_accepted(&s, &pool, "FAILCLOSED-PRIVATE").await;
    sqlx::query("UPDATE development_targets SET value='{}' WHERE id=(SELECT target_id FROM developments WHERE id=$1)").bind(receipt.development_id).execute(&pool).await.unwrap();
    assert!(s.export_individual().await.is_err());
    assert!(s.replay_deletion_journal(j.journal.clone()).await.is_err());
    assert!(!s.privacy_ready());
    sqlx::query("UPDATE development_targets SET value=$1")
        .bind(traits(0.31))
        .execute(&pool)
        .await
        .unwrap();
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    sqlx::query("ALTER TABLE development_revisions ADD COLUMN unknown_private text")
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(
        s.begin_memory_deletion("unknown-delete", target, 1, j.journal.clone())
            .await
            .is_err()
    );
    assert_eq!(before, fingerprint(&pool).await);
    assert!(s.export_individual().await.is_err());
    sqlx::query("ALTER TABLE development_revisions DROP COLUMN unknown_private")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE individuals SET seed_digest=repeat('0',64)")
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.export_individual().await.is_err());
    assert!(s.replay_deletion_journal(j.journal.clone()).await.is_err());
    assert!(!s.privacy_ready());
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_late_begin_failure_rolls_back_projection_then_journal_recovers(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let (_, target, _) = private_accepted(&s, &pool, "ROLLBACK-PRIVATE").await;
    sqlx::query("CREATE FUNCTION p05_reject_job() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'synthetic late failure';END$$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER p05_reject_job BEFORE INSERT ON privacy_operations FOR EACH ROW EXECUTE FUNCTION p05_reject_job()").execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert!(
        s.begin_memory_deletion("rollback-delete", target, 1, j.journal.clone())
            .await
            .is_err()
    );
    assert!(!s.privacy_ready());
    assert_eq!(before, fingerprint(&pool).await);
    sqlx::query("DROP TRIGGER p05_reject_job ON privacy_operations")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION p05_reject_job()")
        .execute(&pool)
        .await
        .unwrap();
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert!(s.privacy_ready());
    assert_eq!(s.snapshot().await.unwrap().unwrap().persona, seed());
    assert!(s.export_individual().await.is_ok());
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_new_generation_legal_propose_survives_old_journal(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let (_, target, old) = private_accepted(&s, &pool, "OLD-GENERATION-PRIVATE").await;
    let (oldhead,maxversion):(Uuid,i64)=sqlx::query_as("SELECT target_id,max(t.version) FROM developments d JOIN development_targets t ON t.id=d.target_id WHERE d.id=$1 GROUP BY target_id").bind(old.development_id).fetch_one(&pool).await.unwrap();
    s.begin_memory_deletion("old-generation", target, 1, j.journal.clone())
        .await
        .unwrap();
    s.purge_privacy_operations(16).await.unwrap();
    let source = private_source(&pool, "NEW-GENERATION-LEGAL").await;
    let verifier = registered(&pool).await;
    let mut d = draft(source, DevelopmentTarget::Traits, traits(0.66));
    d.before_version = maxversion + 1;
    // Only a legitimate source and current identity, no old private head ID.
    let current = s.snapshot().await.unwrap().unwrap();
    let fresh = s
        .propose_development(
            "new-generation-propose",
            current.expected(),
            &[source, current.id],
            current.id,
            &d,
        )
        .await
        .expect("P04 must select only live targets and initialize max historical+1");
    trial(&s, &pool, fresh.development_id).await;
    let review = review(&pool, fresh.development_id, verifier, Outcome::Passed).await;
    transition(&s, &pool, fresh.development_id, &review).await;
    transition(&s, &pool, fresh.development_id, &DevelopmentCommand::Accept).await;
    let newhead: Uuid = sqlx::query_scalar("SELECT target_id FROM developments WHERE id=$1")
        .bind(fresh.development_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(newhead, oldhead);
    let newversion: i64 = sqlx::query_scalar("SELECT version FROM development_targets WHERE id=$1")
        .bind(newhead)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(newversion, maxversion + 2);
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
    assert!(reopened.development(fresh.development_id).await.is_ok());
    assert!(reopened.development(old.development_id).await.is_err());
    let export = String::from_utf8(
        reopened
            .export_individual()
            .await
            .unwrap()
            .to_json_bytes()
            .unwrap(),
    )
    .unwrap();
    assert!(export.contains("NEW-GENERATION-LEGAL"));
    assert!(!export.contains("OLD-GENERATION-PRIVATE"));
    assert!(!export.contains(&oldhead.to_string()));
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_task_strategy_sources_delete_scope_projection_not_identity(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let source = private_source(&pool, "PRIVATE-STRATEGY-EVIDENCE").await;
    let target = memory(&pool, source, "PRIVATE-STRATEGY-EVIDENCE").await;
    let goal = object(&pool, "goal").await;
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,description,success_criterion,next_step,stop_condition) SELECT id,$1,'event','Independent goal','Actual result','Inspect','Stop if unavailable' FROM individuals").bind(goal).execute(&pool).await.unwrap();
    let verifier = registered(&pool).await;
    let strategy = json!({"scope":{"kind":"goal","id":goal},"description":"PRIVATE-STRATEGY-BODY"});
    let accepted = accepted(
        &s,
        &pool,
        &draft(source, DevelopmentTarget::TaskStrategy, strategy),
        verifier,
    )
    .await;
    let identity = s.snapshot().await.unwrap().unwrap();
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM development_dependencies")
        .execute(&pool)
        .await
        .unwrap();
    s.begin_memory_deletion("strategy-delete", target, 1, j.journal.clone())
        .await
        .unwrap();
    let now = s.snapshot().await.unwrap().unwrap();
    assert_eq!(now.id, identity.id);
    assert_eq!(now.persona, identity.persona);
    assert_eq!(now.runtime_config, identity.runtime_config);
    assert!(s.development(accepted.development_id).await.is_err());
    let blocked_goal: bool = sqlx::query_scalar("SELECT blocked FROM objects WHERE id=$1")
        .bind(goal)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!blocked_goal);
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 1);
    let text = String::from_utf8(
        s.export_individual()
            .await
            .unwrap()
            .to_json_bytes()
            .unwrap(),
    )
    .unwrap();
    assert!(!text.contains("PRIVATE-STRATEGY"));
    assert!(text.contains("Independent goal"));
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_review_observation_fk_removal_invalidates_applied_persona(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let (_, _, receipt) = private_accepted(&s, &pool, "INDEPENDENT-TRIAL").await;
    let observed:Uuid=sqlx::query_scalar("SELECT event_id FROM development_observations WHERE role='scenario_result' ORDER BY event_id LIMIT 1").fetch_one(&pool).await.unwrap();
    let target = memory(&pool, observed, "TRIAL-RESULT-PRIVATE").await;
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM development_dependencies")
        .execute(&pool)
        .await
        .unwrap();
    s.begin_memory_deletion("review-fk-delete", target, 1, j.journal.clone())
        .await
        .unwrap();
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().persona,
        seed(),
        "removed validation source left applied persona"
    );
    assert!(s.development(receipt.development_id).await.is_err());
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 1);
    assert!(s.export_individual().await.is_ok());
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_strategy_revert_null_is_legal_and_private_purge_failure_keeps_safe_persona(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let source = private_source(&pool, "STRATEGY-REVERT-PUBLIC").await;
    let goal = object(&pool, "goal").await;
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,description,success_criterion,next_step,stop_condition) SELECT id,$1,'event','Independent goal','Result','Inspect','Stop' FROM individuals").bind(goal).execute(&pool).await.unwrap();
    let verifier = registered(&pool).await;
    let strategy =
        json!({"scope":{"kind":"goal","id":goal},"description":"Scoped verified strategy"});
    let r = accepted(
        &s,
        &pool,
        &draft(source, DevelopmentTarget::TaskStrategy, strategy),
        verifier,
    )
    .await;
    transition(
        &s,
        &pool,
        r.development_id,
        &DevelopmentCommand::Revert {
            reason: "Actual scoped reversal".into(),
        },
    )
    .await;
    assert!(
        s.export_individual().await.is_ok(),
        "reverted strategy null baseline must remain a legal complete history"
    );
    let (_, target, receipt) = private_accepted(&s, &pool, "PURGE-FAULT-PRIVATE").await;
    s.begin_memory_deletion("purge-fault", target, 1, j.journal.clone())
        .await
        .unwrap();
    sqlx::query("CREATE FUNCTION p05_fail_development_purge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic late development purge failure';END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER p05_fail_development_purge BEFORE UPDATE ON development_steps FOR EACH ROW EXECUTE FUNCTION p05_fail_development_purge()").execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 0);
    assert_eq!(s.snapshot().await.unwrap().unwrap().persona, seed());
    assert!(s.development(receipt.development_id).await.is_err());
    let after = fingerprint(&pool).await;
    for (table, value) in before {
        if table != "privacy_operations" {
            assert_eq!(after[&table], value, "purge failed to rollback {table}");
        }
    }
    let status: String =
        sqlx::query_scalar("SELECT status FROM privacy_operations WHERE target_id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "retry_pending");
    sqlx::query("DROP TRIGGER p05_fail_development_purge ON development_steps")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION p05_fail_development_purge()")
        .execute(&pool)
        .await
        .unwrap();
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert!(s.privacy_ready());
    assert!(s.export_individual().await.is_ok());
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_missing_review_sources_cannot_prove_cached_persona(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let (_, _, receipt) = private_accepted(&s, &pool, "PROOF-EVIDENCE").await;
    sqlx::query("DELETE FROM development_review_sources WHERE review_event_id IN (SELECT event_id FROM development_review_records WHERE development_id=$1)").bind(receipt.development_id).execute(&pool).await.unwrap();
    assert!(
        s.export_individual().await.is_err(),
        "cached persona accepted after actual validation source associations disappeared"
    );
    assert!(s.replay_deletion_journal(j.journal.clone()).await.is_err());
    assert!(!s.privacy_ready());
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_initial_head_version_and_immutable_birth_seed_fail_closed(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let source = private_source(&pool, "INITIAL-HEAD-EVIDENCE").await;
    let p = propose(
        &s,
        &pool,
        &draft(source, DevelopmentTarget::Traits, traits(0.5)),
    )
    .await;
    sqlx::query("UPDATE development_targets SET version=99 WHERE id=(SELECT target_id FROM developments WHERE id=$1)").bind(p.development_id).execute(&pool).await.unwrap();
    assert!(
        s.export_individual().await.is_err(),
        "head baseline version ignored actual prior generations"
    );
    assert!(s.replay_deletion_journal(j.journal.clone()).await.is_err());
    assert!(!s.privacy_ready());
    sqlx::query("UPDATE development_targets SET version=1")
        .execute(&pool)
        .await
        .unwrap();
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    // Seed and its adjacent digest may not replace the immutable birth binding.
    let mut fake = seed();
    fake["expression"]["tone"] = json!("tampered adjacent seed");
    let digest = format!(
        "{:x}",
        <sha2::Sha256 as sha2::Digest>::digest(serde_json::to_vec(&fake).unwrap())
    );
    sqlx::query("UPDATE individuals SET seed=$1,seed_digest=$2,persona=$1")
        .bind(fake)
        .bind(digest)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        s.export_individual().await.is_err(),
        "adjacent seed/hash tamper bypassed actual birth anchor"
    );
    assert!(s.replay_deletion_journal(j.journal.clone()).await.is_err());
    assert!(!s.privacy_ready());
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_privacy_missing_candidate_evidence_receipts_fail_closed(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let source = private_source(&pool, "VALID-PRIMARY-EVIDENCE").await;
    let counter = private_source(&pool, "REMOVED-COUNTER-EVIDENCE-ASSOCIATION").await;
    let mut d = draft(source, DevelopmentTarget::Traits, traits(0.45));
    d.counterevidence_ids = vec![counter];
    let verifier = registered(&pool).await;
    let receipt = accepted(&s, &pool, &d, verifier).await;
    sqlx::query(
        "DELETE FROM development_evidence WHERE development_id=$1 AND role='counterevidence'",
    )
    .bind(receipt.development_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(s.export_individual().await.is_err());
    assert!(s.replay_deletion_journal(j.journal.clone()).await.is_err());
    assert!(!s.privacy_ready());
}
#[sqlx::test]
#[ignore = "P05 owned PostgreSQL under parent global lock"]
async fn development_export_large_legal_source_graph_keeps_full_archive_limit(pool: PgPool) {
    let s = setup(&pool).await;
    let source = private_source(&pool, "PUBLIC-GRAPH-ROOT").await;
    let p = propose(
        &s,
        &pool,
        &draft(source, DevelopmentTarget::Traits, traits(0.7)),
    )
    .await;
    let head: Uuid = sqlx::query_scalar("SELECT target_id FROM developments WHERE id=$1")
        .bind(p.development_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,md5('public-development-source-'||n::text)::uuid,'event' FROM individuals CROSS JOIN generate_series(1,10001)n").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,md5('public-development-source-'||n::text)::uuid,'message.received','reported_claim','owner',clock_timestamp(),'{}',md5('public-development-source-'||n::text)::uuid FROM individuals CROSS JOIN generate_series(1,10001)n").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO development_dependencies(individual_id,object_id,source_id) SELECT id,$1,md5('public-development-source-'||n::text)::uuid FROM individuals CROSS JOIN generate_series(1,10001)n").bind(head).execute(&pool).await.unwrap();
    // A correlation identifier need not be an owned provenance object.
    sqlx::query("UPDATE events SET correlation_id=$2 WHERE id=$1")
        .bind(source)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    let archive = s
        .export_individual()
        .await
        .expect("export must not inherit deletion-preview10000 ancestor cap");
    assert_eq!(archive.tables["events"].len(), 10004); // genesis, source, proposal, synthetic10001
    assert_eq!(
        archive.tables["development_dependencies"]
            .iter()
            .filter(|d| d["object_id"] == head.to_string())
            .count(),
        10002
    );
    assert!(archive.to_json_bytes().unwrap().len() < 64 * 1024 * 1024);
    assert_eq!(before, fingerprint(&pool).await);
}

// Schema30 engineering fixtures use actual plan APIs and the original billing
// API with synthetic usage. No provider/model is invoked or quality certified.
async fn trial_private_fixture(
    pool: &PgPool,
) -> (Store, Uuid, alia::domain::development_trials::TrialPlan) {
    use alia::{
        domain::development_trials::*,
        runtime::{
            model::TokenUsage,
            scheduling::{RunOutcome, TokenAllowance},
        },
    };
    let s = setup(pool).await;
    let source = s
        .submit_message("trial-private-input", "TRIAL-PRIVATE-CONTEXT", None)
        .await
        .unwrap()
        .event_id;
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut reservations = Vec::new();
    for known in [true, false] {
        let context = s
            .model_context(lease.run_id, s.local_capabilities().await.unwrap())
            .await
            .unwrap();
        let r = s
            .reserve_model_call(
                &lease,
                Uuid::new_v4(),
                &context,
                TokenAllowance {
                    input_upper_bound: 40,
                    output_cap: 20,
                },
            )
            .await
            .unwrap();
        assert!(s.dispatch_model_call(&lease, r.id).await.unwrap());
        s.settle_model_call(
            r.id,
            known.then_some(TokenUsage {
                input: 30,
                output: 18,
                total: 48,
            }),
        )
        .await
        .unwrap();
        reservations.push(r.id);
    }
    s.finish_run(&lease, RunOutcome::Cancelled).await.unwrap();
    let p = propose(
        &s,
        pool,
        &draft(source, DevelopmentTarget::Traits, traits(0.61)),
    )
    .await;
    let evaluator = s
        .register_development_measurement(
            "trial-private-program",
            MeasurementProgram::JsonComparisonV1,
        )
        .await
        .unwrap();
    let plan = s
        .freeze_development_trial_plan(
            "trial-private-plan",
            s.snapshot().await.unwrap().unwrap().expected(),
            &refs(
                pool,
                p.development_id,
                &DevelopmentCommand::StartTrial {
                    route: ValidationRoute::CrossScenarioHabit,
                    source_event_ids: vec![source],
                },
            )
            .await,
            p.development_id,
            p.development_version,
            evaluator,
            ValidationRoute::CrossScenarioHabit,
            &TrialPlanDraft {
                cases: (0..3)
                    .map(|i| {
                        TrialCase {
                        id: Uuid::new_v4(),
                        context: format!("TRIAL-PRIVATE-CONTEXT-{i}"),
                        prompt: match i {
                        0 => "TRIAL-PRIVATE-PROMPT: 核验新资料的来源，并区分观察与推断。",
                        1 => "TRIAL-PRIVATE-PROMPT: 发现旧结论与新证据矛盾时，保留反证并修改结论。",
                        _ => "TRIAL-PRIVATE-PROMPT: 搜索推翻当前判断的反例，并明确未知部分。",
                    }.into(),
                        role: if i == 2 {
                            TrialCaseRole::CounterSearch
                        } else {
                            TrialCaseRole::Scenario
                        },
                        criterion: TrialCriterion::JsonEquals {
                            pointer: "/reply".into(),
                            expected: json!("TRIAL-PRIVATE-GOLDEN"),
                        },
                        source_event_ids: vec![source],
                    }
                    })
                    .collect(),
                deadline: Utc::now() + Duration::minutes(30),
            },
        )
        .await
        .unwrap();
    let attempts: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM development_trial_attempts WHERE plan_id=$1 ORDER BY id LIMIT 2",
    )
    .bind(plan.id)
    .fetch_all(pool)
    .await
    .unwrap();
    for (i, attempt) in attempts.iter().enumerate() {
        sqlx::query("UPDATE development_trial_attempts SET context_json=$2,request_digest=repeat('a',64),request_sources=$3,input_bound=40,output_cap=20,result=$4,state=$5,outcome='passed',reservation_id=$6 WHERE id=$1")
            .bind(attempt).bind(json!({"text":"TRIAL-PRIVATE-CONTEXT"})).bind(vec![source]).bind(json!({"reply":"TRIAL-PRIVATE-RESULT"})).bind(if i==0 {"recorded"} else {"unknown"}).bind(reservations[i]).execute(pool).await.unwrap();
    }
    let target = memory(pool, source, "TRIAL-PRIVATE-MEMORY").await;
    // Require typed FK/source references to carry the deletion without the
    // convenience graph; retain all actual ownership and reservation rows.
    sqlx::query("DELETE FROM derivation_edges")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM development_dependencies")
        .execute(pool)
        .await
        .unwrap();
    (s, target, plan)
}
async fn trial_clean(pool: &PgPool, plan: Uuid) {
    let (plans,cases,attempts):(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM development_trial_plans WHERE id=$1 AND plan='{}' AND status='stale'),(SELECT count(*) FROM development_trial_cases WHERE plan_id=$1 AND case_data='{}' AND status='not_run'),(SELECT count(*) FROM development_trial_attempts WHERE plan_id=$1 AND context_json IS NULL AND request_digest IS NULL AND input_bound IS NULL AND output_cap IS NULL AND result IS NULL AND state IN ('unknown','not_run') AND outcome='unverified')").bind(plan).fetch_one(pool).await.unwrap();
    assert_eq!((plans, cases, attempts), (1, 3, 6));
    let charges:Vec<(i64,bool,Option<i64>)>=sqlx::query_as("SELECT charged_tokens,usage_unknown,actual_total FROM usage_reservations ORDER BY charged_tokens").fetch_all(pool).await.unwrap();
    assert_eq!(charges, vec![(48, false, Some(48)), (60, true, None)]);
    let associations:i64=sqlx::query_scalar("SELECT count(*) FROM development_trial_attempts WHERE plan_id=$1 AND reservation_id IS NOT NULL").bind(plan).fetch_one(pool).await.unwrap();
    assert_eq!(associations, 2);
}
#[sqlx::test]
#[ignore = "P05 isolated schema30 PostgreSQL, no model"]
async fn schema30_trial_delete_cleans_private_copies_preserves_original_ledger(pool: PgPool) {
    let (s, target, plan) = trial_private_fixture(&pool).await;
    let j = Journal::new();
    let before = fingerprint(&pool).await;
    s.begin_memory_deletion("trial-delete", target, 1, j.journal.clone())
        .await
        .unwrap();
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 1);
    trial_clean(&pool, plan.id).await;
    assert_eq!(
        fingerprint(&pool).await["usage_reservations"],
        before["usage_reservations"]
    );
    assert_eq!(
        fingerprint(&pool).await["budget_windows"],
        before["budget_windows"]
    );
    assert_eq!(
        fingerprint(&pool).await["development_measurement_programs"],
        before["development_measurement_programs"]
    );
    assert!(s.recover_development_trial_work(plan.id).await.is_err());
}
#[sqlx::test]
#[ignore = "P05 isolated schema30 PostgreSQL, no model"]
async fn schema30_trial_purge_late_failure_rolls_back_same_transaction(pool: PgPool) {
    let (s, target, plan) = trial_private_fixture(&pool).await;
    let j = Journal::new();
    s.begin_memory_deletion("trial-rollback", target, 1, j.journal.clone())
        .await
        .unwrap();
    sqlx::query("CREATE FUNCTION p05_trial_late_fail() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'synthetic late fault after trial scrub'; END$$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER p05_trial_late_fail BEFORE UPDATE ON development_steps FOR EACH ROW EXECUTE FUNCTION p05_trial_late_fail()").execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 0);
    let after = fingerprint(&pool).await;
    for name in [
        "development_measurement_programs",
        "development_trial_plans",
        "development_trial_cases",
        "development_trial_attempts",
        "development_trial_case_sources",
        "usage_reservations",
        "budget_windows",
    ] {
        assert_eq!(before[name], after[name], "partial purge committed {name}");
    }
    sqlx::raw_sql("DROP TRIGGER p05_trial_late_fail ON development_steps; DROP FUNCTION p05_trial_late_fail()").execute(&pool).await.unwrap();
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    trial_clean(&pool, plan.id).await;
}
#[sqlx::test]
#[ignore = "P05 isolated schema30 PostgreSQL, no model"]
async fn schema30_trial_clean_proof_rejects_suppressed_scrub_and_replay_repairs_completed_restore(
    pool: PgPool,
) {
    let (s, target, plan) = trial_private_fixture(&pool).await;
    let j = Journal::new();
    s.begin_memory_deletion("trial-proof", target, 1, j.journal.clone())
        .await
        .unwrap();
    sqlx::query("CREATE FUNCTION p05_trial_suppress() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RETURN OLD; END$$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER p05_trial_suppress BEFORE UPDATE ON development_trial_attempts FOR EACH ROW EXECUTE FUNCTION p05_trial_suppress()").execute(&pool).await.unwrap();
    assert_eq!(
        s.purge_privacy_operations(16).await.unwrap(),
        0,
        "an unsanitized copy must prevent completion"
    );
    let status: String =
        sqlx::query_scalar("SELECT status FROM privacy_operations WHERE target_id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "retry_pending");
    sqlx::raw_sql("DROP TRIGGER p05_trial_suppress ON development_trial_attempts; DROP FUNCTION p05_trial_suppress()").execute(&pool).await.unwrap();
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    trial_clean(&pool, plan.id).await;
    // Simulate old restored private rows alongside a completed deletion ledger.
    // This is journal recovery, not a claim of pg_dump/pg_restore coverage.
    sqlx::query("UPDATE development_trial_plans SET plan=$2,status='active' WHERE id=$1")
        .bind(plan.id)
        .bind(json!({"private":"TRIAL-PRIVATE-RESTORED"}))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE development_trial_cases SET case_data=$2,status='finished' WHERE plan_id=$1",
    )
    .bind(plan.id)
    .bind(json!({"criterion":"TRIAL-PRIVATE-GOLDEN"}))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE development_trial_attempts SET context_json=$2,request_digest=repeat('b',64),input_bound=40,output_cap=20,result=$2,state='recorded',outcome='passed' WHERE plan_id=$1").bind(plan.id).bind(json!({"private":"TRIAL-PRIVATE-RESTORED"})).execute(&pool).await.unwrap();
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert!(s.privacy_ready());
    trial_clean(&pool, plan.id).await;
    let stable = fingerprint(&pool).await;
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert_eq!(fingerprint(&pool).await, stable);
}

#[sqlx::test]
#[ignore = "P05 schema30 personal export on isolated PG; no models"]
async fn schema30_trial_export_complete_redacted_and_hidden_after_delete(pool: PgPool) {
    let (s, target, plan) = trial_private_fixture(&pool).await;
    sqlx::query("UPDATE development_trial_attempts SET context_json=jsonb_set(context_json,'{nested}', $2) WHERE plan_id=$1 AND context_json IS NOT NULL")
        .bind(plan.id).bind(json!({"authorization":"SYNTHETIC-OPERATIONAL-TOKEN","public":"owned context detail"})).execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    let archive = s.export_individual().await.unwrap();
    assert_eq!(archive.tables.len(), 61);
    for (table, count) in [
        ("development_measurement_programs", 1),
        ("development_trial_plans", 1),
        ("development_trial_cases", 3),
        ("development_trial_attempts", 6),
        ("development_trial_case_sources", 3),
    ] {
        assert_eq!(
            archive.tables[table].len(),
            count,
            "missing owned rows in {table}"
        );
    }
    let bytes = archive.to_json_bytes().unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("TRIAL-PRIVATE-GOLDEN"));
    assert!(text.contains("owned context detail"));
    assert!(!text.contains("SYNTHETIC-OPERATIONAL-TOKEN"));
    assert!(
        archive.tables["development_trial_attempts"]
            .iter()
            .all(|a| a.get("request_digest").is_none() && a.get("lease_generation").is_none())
    );
    assert_eq!(fingerprint(&pool).await, before, "export must be read-only");
    let j = Journal::new();
    s.begin_memory_deletion("trial-export-delete", target, 1, j.journal.clone())
        .await
        .unwrap();
    assert!(s.export_individual().await.is_err());
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 1);
    let after = s.export_individual().await.unwrap();
    for table in [
        "development_trial_plans",
        "development_trial_cases",
        "development_trial_attempts",
        "development_trial_case_sources",
    ] {
        assert!(
            after.tables[table].is_empty(),
            "deleted associations exported from {table}"
        );
    }
    let text = String::from_utf8(after.to_json_bytes().unwrap()).unwrap();
    assert!(!text.contains("TRIAL-PRIVATE"));
    assert!(!text.contains(&plan.id.to_string()));
    assert_eq!(after.tables["usage_reservations"].len(), 2);
    assert!(
        after.tables["usage_reservations"]
            .iter()
            .any(|r| r["usage_unknown"] == true
                && r["actual_total"].is_null()
                && r["charged_tokens"] == 60)
    );
}
