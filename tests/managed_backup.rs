//! Actual PostgreSQL boundaries with explicitly synthetic trusted execution records.
use alia::{
    adapters::{
        development::{DevelopmentCommand, DevelopmentReceipt},
        store::Store,
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

use alia::adapters::managed_backup::{BackupFailureCode, BackupLimits, ManagedBackupManager};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration as StdDuration;
struct Vault {
    path: PathBuf,
    manager: ManagedBackupManager,
}
impl Vault {
    fn new() -> Self {
        Self::with_limits(BackupLimits::default())
    }
    fn with_limits(limits: BackupLimits) -> Self {
        let path = std::env::temp_dir().join(format!("alia-p05-backup-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        let manager = ManagedBackupManager::open(&path, limits).unwrap();
        Self { path, manager }
    }
    fn artifact(&self, id: Uuid, name: &str) -> PathBuf {
        self.path.join("artifacts").join(id.to_string()).join(name)
    }
}
impl Drop for Vault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
async fn object(pool: &PgPool, kind: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,$2 FROM individuals")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    id
}
async fn memory(pool: &PgPool, source: Uuid, text: &str) -> Uuid {
    let id = object(pool, "memory").await;
    sqlx::query("INSERT INTO memories(individual_id,id,kind,content,scope) SELECT id,$1,'semantic',$2,'general' FROM individuals").bind(id).bind(text).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,$1,$2 FROM individuals").bind(id).bind(source).execute(pool).await.unwrap();
    id
}
async fn source_event(pool: &PgPool, text: &str) -> Uuid {
    event(
        pool,
        "message.received",
        "reported_claim",
        "owner",
        json!({"text":text}),
    )
    .await
}
async fn connect_restored(admin: &PgPool, name: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(3)
        .connect_with(admin.connect_options().as_ref().clone().database(name))
        .await
        .unwrap()
}
async fn fingerprint(pool: &PgPool) -> BTreeMap<String, String> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut result = BTreeMap::new();
    for name in names {
        assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let h:String=sqlx::query_scalar(&format!("SELECT md5(coalesce(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM {name} t")).fetch_one(pool).await.unwrap();
        result.insert(name, h);
    }
    result
}
async fn vectors(pool: &PgPool, memory: Uuid) {
    sqlx::query("INSERT INTO memory_vector_sets(individual_id,memory_id,profile,memory_version,privacy_epoch,content_digest,chunk_count) SELECT id,$1,'synthetic-backup-vector',1,privacy_epoch,repeat('a',64),1 FROM individuals").bind(memory).execute(pool).await.unwrap();
    let mut v = vec!["0"; 384];
    v[0] = "1";
    let vector = format!("[{}]", v.join(","));
    sqlx::query("INSERT INTO memory_vectors(individual_id,memory_id,profile,ordinal,start_char,end_char,embedding) SELECT id,$1,'synthetic-backup-vector',0,0,1,$2::vector FROM individuals").bind(memory).bind(vector).execute(pool).await.unwrap();
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_real_pg_dump_restore_exact_identity_history_vectors_and_readonly_source(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let e = source_event(&pool, "PUBLIC-ARCHIVE-🙂").await;
    let m = memory(&pool, e, "PUBLIC-ARCHIVE-🙂").await;
    vectors(&pool, m).await;
    let before = fingerprint(&pool).await;
    let backup = v.manager.create(&s, j.journal.clone()).await.unwrap();
    assert_eq!(before, fingerprint(&pool).await);
    assert!(backup.archive_bytes > 0);
    assert_eq!(
        v.manager
            .verify_current(&s, backup.backup_id)
            .await
            .unwrap(),
        backup
    );
    let body = fs::read(v.artifact(backup.backup_id, "archive.pgcustom")).unwrap();
    assert!(body.starts_with(b"PGDMP"));
    let meta = fs::read_to_string(v.artifact(backup.backup_id, "manifest.json")).unwrap();
    for forbidden in [
        "PGPASSWORD",
        "DATABASE_URL",
        "instance_token",
        "deletions.jsonl",
        "127.0.0.1",
    ] {
        assert!(!meta.contains(forbidden));
    }
    let receipt = v
        .manager
        .restore_current_isolated(&s, backup.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap();
    assert_eq!(receipt.status, "verified_fenced");
    assert_eq!(receipt.individual_id, backup.individual_id);
    let restored = connect_restored(&pool, &receipt.database_name()).await;
    assert_eq!(before, fingerprint(&restored).await);
    restored.close().await;
    v.manager
        .cleanup_recovery(&pool, receipt.recovery_id)
        .await
        .unwrap();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(receipt.database_name())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!exists);
    assert_eq!(before, fingerprint(&pool).await);
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_old_snapshot_replays_completed_delete_no_vector_or_persona_revival(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let private = source_event(&pool, "OLD-BACKUP-PRIVATE").await;
    let m = memory(&pool, private, "OLD-BACKUP-PRIVATE").await;
    vectors(&pool, m).await;
    let verifier = registered(&pool).await;
    let private_dev = accepted(
        &s,
        &pool,
        &draft(private, DevelopmentTarget::Traits, traits(0.37)),
        verifier,
    )
    .await;
    let public = source_event(&pool, "INDEPENDENT-LEGAL-RETAINED").await;
    let mut expression = seed()["expression"].clone();
    expression["tone"] = json!("独立保留语气");
    let public_dev = accepted(
        &s,
        &pool,
        &draft(public, DevelopmentTarget::Expression, expression.clone()),
        verifier,
    )
    .await;
    let billed_run = object(&pool, "run").await;
    sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,input_manifest) SELECT id,$1,$2,'foreground',state_version,privacy_epoch,config_version,$3 FROM individuals").bind(billed_run).bind(private).bind(vec![private,m]).execute(&pool).await.unwrap();
    let window = Uuid::new_v4();
    let billing = Uuid::new_v4();
    let request = Uuid::new_v4();
    sqlx::query("INSERT INTO budget_windows(id,individual_id,timezone,starts_at,ends_at,charged_total) SELECT $1,id,'UTC',clock_timestamp()-interval '1 day',clock_timestamp()+interval '1 day',48 FROM individuals").bind(window).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO usage_reservations(id,individual_id,run_id,lease_generation,request_key,request_digest,window_id,mode,input_bound,output_cap,reserved_tokens,status,charged_tokens,usage_unknown) SELECT $1,id,$2,0,$3,repeat('e',64),$4,'foreground',44,4,48,'settled',48,true FROM individuals").bind(billing).bind(billed_run).bind(request).bind(window).execute(&pool).await.unwrap();
    let backup = v.manager.create(&s, j.journal.clone()).await.unwrap();
    assert_eq!(backup.privacy_epoch, 0);
    let deleted = s
        .begin_memory_deletion("backup-delete", m, 1, j.journal.clone())
        .await
        .unwrap();
    // Root owns wiring this hook into that original transaction. The manager hook
    // is exercised with the real committed intent/job/epoch here, never mocked.
    let mut tx = pool.begin().await.unwrap();
    v.manager
        .invalidate_in(
            &mut tx,
            deleted.individual_id,
            deleted.privacy_epoch,
            j.journal.clone(),
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        v.manager
            .verify_current(&s, backup.backup_id)
            .await
            .is_err()
    );
    assert!(
        v.manager
            .restore_current_isolated(&s, backup.backup_id, &pool, &j.dir.join("deletions.jsonl"))
            .await
            .is_err()
    );
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 1);
    let receipt = v
        .manager
        .recover_quarantined(backup.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap();
    assert!(receipt.privacy_epoch >= deleted.privacy_epoch);
    assert_eq!(receipt.status, "verified_fenced");
    let restored = connect_restored(&pool, &receipt.database_name()).await;
    let restored_store = Store::new(restored.clone());
    let state = restored_store.snapshot().await.unwrap().unwrap();
    assert_eq!(state.id, backup.individual_id);
    assert_eq!(
        state.persona["personality"]["traits"],
        seed()["personality"]["traits"]
    );
    assert_eq!(state.persona["expression"], expression);
    assert!(
        restored_store
            .development(private_dev.development_id)
            .await
            .is_err()
    );
    assert!(
        restored_store
            .development(public_dev.development_id)
            .await
            .is_ok()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory_vectors WHERE memory_id=$1")
        .bind(m)
        .fetch_one(&restored)
        .await
        .unwrap();
    assert_eq!(count, 0);
    for table in [
        "events",
        "memories",
        "developments",
        "development_targets",
        "development_steps",
        "development_revisions",
    ] {
        let hidden:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {table} t WHERE to_jsonb(t)::text LIKE '%OLD-BACKUP-PRIVATE%')")).fetch_one(&restored).await.unwrap();
        assert!(!hidden, "private content revived in {table}");
    }
    let actual:(i64,bool,Option<i64>,String,Uuid)=sqlx::query_as("SELECT charged_tokens,usage_unknown,actual_total,status,request_key FROM usage_reservations WHERE id=$1").bind(billing).fetch_one(&restored).await.unwrap();
    assert_eq!(actual, (48, true, None, "settled".into(), request));
    restored.close().await;
    v.manager
        .cleanup_recovery(&pool, receipt.recovery_id)
        .await
        .unwrap();
    let cleanup = v
        .manager
        .purge_invalidated(deleted.individual_id, deleted.privacy_epoch)
        .unwrap();
    assert_eq!(cleanup.remaining, 0);
    assert_eq!(cleanup.removed, 1);
    assert!(
        v.manager
            .invalidated_clean(deleted.individual_id, deleted.privacy_epoch)
            .unwrap()
    );
    assert!(!v.artifact(backup.backup_id, "archive.pgcustom").exists());
}

#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_checksum_manifest_epoch_schema_and_missing_journal_refused(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let backup = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let path = v.artifact(backup.backup_id, "archive.pgcustom");
    let original = fs::read(&path).unwrap();
    fs::write(&path, &original[..original.len() / 2]).unwrap();
    assert_eq!(
        v.manager
            .verify_current(&s, backup.backup_id)
            .await
            .unwrap_err()
            .code,
        BackupFailureCode::Invalid
    );
    fs::write(&path, &original).unwrap();
    let manifest = v.artifact(backup.backup_id, "manifest.json");
    let original_meta = fs::read(&manifest).unwrap();
    let mut forged: Value = serde_json::from_slice(&original_meta).unwrap();
    forged["privacy_epoch"] = json!(1);
    fs::write(&manifest, serde_json::to_vec(&forged).unwrap()).unwrap();
    assert!(
        v.manager
            .verify_current(&s, backup.backup_id)
            .await
            .is_err()
    );
    fs::write(&manifest, &original_meta).unwrap();
    let absent = j.dir.join("lost-original.jsonl");
    assert!(
        v.manager
            .restore_current_isolated(&s, backup.backup_id, &pool, &absent)
            .await
            .is_err()
    );
    assert!(!absent.exists());
    sqlx::query("ALTER TABLE memories ADD COLUMN new_unknown_private text")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        v.manager
            .create(&s, j.journal.clone())
            .await
            .unwrap_err()
            .code,
        BackupFailureCode::Unsupported
    );
    sqlx::query("ALTER TABLE memories DROP COLUMN new_unknown_private")
        .execute(&pool)
        .await
        .unwrap();
    let mut bad: Value = serde_json::from_slice(&original_meta).unwrap();
    bad["format"] = json!("alia-managed-pg-backup/0");
    fs::write(&manifest, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(
        v.manager
            .recover_quarantined(backup.backup_id, &pool, &j.dir.join("deletions.jsonl"))
            .await
            .is_err()
    );
    fs::write(&manifest, &original_meta).unwrap();
    assert!(v.manager.verify_current(&s, backup.backup_id).await.is_ok());
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_actual_size_limit_and_pg_dump_lock_timeout_no_partial_publication(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let small = Vault::with_limits(BackupLimits {
        max_archive_bytes: 16,
        timeout: StdDuration::from_secs(10),
    });
    let error = small
        .manager
        .create(&s, j.journal.clone())
        .await
        .unwrap_err();
    assert_eq!(error.code, BackupFailureCode::TooLarge);
    assert!(error.cleanup_complete);
    assert_eq!(
        fs::read_dir(small.path.join("artifacts")).unwrap().count(),
        0
    );
    let quick = Vault::with_limits(BackupLimits {
        max_archive_bytes: 1024 * 1024 * 64,
        timeout: StdDuration::from_secs(2),
    });
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE pg_catalog.pg_description IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let manager = quick.manager.clone();
    let source = s.clone();
    let journal = j.journal.clone();
    let start = std::time::Instant::now();
    let job = tokio::spawn(async move { manager.create(&source, journal).await });
    wait_dump_lock(&pool).await;
    let error = job.await.unwrap().unwrap_err();
    assert_eq!(error.code, BackupFailureCode::Timeout);
    assert!(start.elapsed() < StdDuration::from_secs(5));
    blocker.rollback().await.unwrap();
    // Cancellation can leave a private pending entry while the bounded worker
    // finishes. It cannot have a registered/public manifest or unowned cleanup.
    tokio::time::sleep(StdDuration::from_millis(150)).await;
    for entry in fs::read_dir(quick.path.join("artifacts")).unwrap() {
        let entry = entry.unwrap();
        assert!(!entry.path().join("manifest.json").exists());
    }
}
#[test]
fn managed_backup_private_root_symlink_ancestor_and_invalid_limit_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let v = Vault::new();
    let parent = std::env::temp_dir().join(format!("alia-backup-links-{}", Uuid::new_v4()));
    fs::DirBuilder::new().mode(0o700).create(&parent).unwrap();
    symlink(&v.path, parent.join("linked-root")).unwrap();
    assert!(
        ManagedBackupManager::open(&parent.join("linked-root"), BackupLimits::default()).is_err()
    );
    symlink(&parent, parent.join("ancestor")).unwrap();
    assert!(
        ManagedBackupManager::open(
            &parent.join("ancestor").join("linked-root"),
            BackupLimits::default()
        )
        .is_err()
    );
    fs::set_permissions(&v.path, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(ManagedBackupManager::open(&v.path, BackupLimits::default()).is_err());
    fs::set_permissions(&v.path, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        ManagedBackupManager::open(
            &v.path,
            BackupLimits {
                max_archive_bytes: 0,
                timeout: StdDuration::from_secs(1)
            }
        )
        .is_err()
    );
    fs::remove_dir_all(parent).unwrap();
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_artifact_links_and_mode_never_follow_or_modify_outside_file(pool: PgPool) {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let b = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let path = v.artifact(b.backup_id, "archive.pgcustom");
    let body = fs::read(&path).unwrap();
    let alias = j.dir.join("hardlink");
    fs::hard_link(&path, &alias).unwrap();
    assert_eq!(
        v.manager
            .verify_current(&s, b.backup_id)
            .await
            .unwrap_err()
            .code,
        BackupFailureCode::UnsafePath
    );
    fs::remove_file(&alias).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(v.manager.verify_current(&s, b.backup_id).await.is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let outside = j.dir.join("outside");
    fs::write(&outside, b"outside-untouched").unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&outside, &path).unwrap();
    assert!(v.manager.verify_current(&s, b.backup_id).await.is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"outside-untouched");
    fs::remove_file(&path).unwrap();
    fs::write(&path, &body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(v.manager.verify_current(&s, b.backup_id).await.is_ok());
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_original_authority_refuses_empty_or_wrong_log_and_forged_epoch(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let e = source_event(&pool, "AUTHORITY-PRIVATE").await;
    let m = memory(&pool, e, "AUTHORITY-PRIVATE").await;
    let b = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let r = s
        .begin_memory_deletion("authority-delete", m, 1, j.journal.clone())
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    v.manager
        .invalidate_in(&mut tx, r.individual_id, r.privacy_epoch, j.journal.clone())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 1);
    let manifest = v.artifact(b.backup_id, "manifest.json");
    let original = fs::read(&manifest).unwrap();
    let mut forged: Value = serde_json::from_slice(&original).unwrap();
    forged["privacy_epoch"] = json!(r.privacy_epoch);
    fs::write(&manifest, serde_json::to_vec(&forged).unwrap()).unwrap();
    assert_eq!(
        v.manager
            .verify_current(&s, b.backup_id)
            .await
            .unwrap_err()
            .code,
        BackupFailureCode::Invalid
    );
    fs::write(&manifest, &original).unwrap();
    let empty = Journal::new();
    let denied = v
        .manager
        .recover_quarantined(b.backup_id, &pool, &empty.dir.join("deletions.jsonl"))
        .await
        .unwrap_err();
    assert_eq!(denied.code, BackupFailureCode::Journal);
    assert!(denied.cleanup_complete);
    let wrong = Journal::new();
    let mut intent = j.journal.records().unwrap()[0].clone();
    intent.individual_id = Uuid::new_v4();
    wrong.journal.append(&intent).unwrap();
    assert_eq!(
        v.manager
            .recover_quarantined(b.backup_id, &pool, &wrong.dir.join("deletions.jsonl"))
            .await
            .unwrap_err()
            .code,
        BackupFailureCode::Journal
    );
    assert_eq!(fs::read_dir(v.path.join("recoveries")).unwrap().count(), 0);
    let receipt = v
        .manager
        .recover_quarantined(b.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap();
    v.manager
        .cleanup_recovery(&pool, receipt.recovery_id)
        .await
        .unwrap();
}
async fn wait_dump_lock(pool: &PgPool) {
    for _ in 0..500 {
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND application_name IN ('alia-managed-backup','pg_dump')").fetch_one(pool).await.unwrap();
        if n > 0 {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
    let waits:Vec<(String,Option<String>,String)>=sqlx::query_as("SELECT application_name,wait_event_type,CASE WHEN query LIKE 'LOCK TABLE %' THEN 'dump_lock' WHEN query LIKE '%pg_constraint%' THEN 'schema_constraints' WHEN query LIKE '%pg_description%' THEN 'dump_catalog' ELSE 'other' END FROM pg_stat_activity WHERE datname=current_database()").fetch_all(pool).await.unwrap();
    panic!("actual pg_dump table lock was not observed; safe stage classes: {waits:?}")
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_exported_snapshot_stays_version_n_while_real_late_write_commits(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let before = s.snapshot().await.unwrap().unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE pg_catalog.pg_description IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let manager = v.manager.clone();
    let source = s.clone();
    let journal = j.journal.clone();
    let job = tokio::spawn(async move { manager.create(&source, journal).await });
    wait_dump_lock(&pool).await;
    s.submit_message("late-source-write", "POST-SNAPSHOT-LATE", None)
        .await
        .unwrap();
    assert!(s.snapshot().await.unwrap().unwrap().state_version > before.state_version);
    blocker.rollback().await.unwrap();
    let b = job.await.unwrap().unwrap();
    assert_eq!(b.state_version, before.state_version);
    let receipt = v
        .manager
        .restore_current_isolated(&s, b.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap();
    let target = connect_restored(&pool, &receipt.database_name()).await;
    let late: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM events WHERE payload->>'text'='POST-SNAPSHOT-LATE')",
    )
    .fetch_one(&target)
    .await
    .unwrap();
    assert!(!late);
    target.close().await;
    v.manager
        .cleanup_recovery(&pool, receipt.recovery_id)
        .await
        .unwrap();
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_concurrent_independent_delete_invalidates_waiting_dump_and_leased_cleanup(
    pool: PgPool,
) {
    use std::fs::OpenOptions;
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let e = source_event(&pool, "CONCURRENT-PRIVATE").await;
    let m = memory(&pool, e, "CONCURRENT-PRIVATE").await;
    let old = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(v.artifact(old.backup_id, "lease.lock"))
        .unwrap();
    lease.lock().unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE pg_catalog.pg_description IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let manager = v.manager.clone();
    let source = s.clone();
    let journal = j.journal.clone();
    let job = tokio::spawn(async move { manager.create(&source, journal).await });
    wait_dump_lock(&pool).await;
    let other = Store::new(pool.clone());
    let r = other
        .begin_memory_deletion("concurrent-delete", m, 1, j.journal.clone())
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    v.manager
        .invalidate_in(&mut tx, r.individual_id, r.privacy_epoch, j.journal.clone())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let failure = tokio::time::timeout(StdDuration::from_secs(3), job)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(failure.code, BackupFailureCode::Stale);
    assert!(
        !s.privacy_generation().eq(&other.privacy_generation()),
        "test requires independent process gates; durable epoch floor is the shared barrier"
    );
    let busy = v
        .manager
        .purge_invalidated(r.individual_id, r.privacy_epoch)
        .unwrap();
    assert_eq!(busy.remaining, 1);
    assert!(v.artifact(old.backup_id, "archive.pgcustom").exists());
    assert!(
        v.manager
            .invalidated_clean(r.individual_id, r.privacy_epoch)
            .is_err()
    );
    lease.unlock().unwrap();
    drop(lease);
    blocker.rollback().await.unwrap();
    let clean = v
        .manager
        .purge_invalidated(r.individual_id, r.privacy_epoch)
        .unwrap();
    assert_eq!(clean.remaining, 0);
    assert_eq!(clean.removed, 1);
    assert!(
        v.manager
            .invalidated_clean(r.individual_id, r.privacy_epoch)
            .unwrap()
    );
    assert_eq!(other.purge_privacy_operations(16).await.unwrap(), 1);
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_unknown_index_default_or_sqlx_metadata_column_rejected(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    sqlx::query("CREATE INDEX unknown_backup_index ON events(actor_id)")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        v.manager
            .create(&s, j.journal.clone())
            .await
            .unwrap_err()
            .code,
        BackupFailureCode::Unsupported
    );
    sqlx::query("DROP INDEX unknown_backup_index")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE memories ALTER COLUMN validity SET DEFAULT 'disputed'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        v.manager
            .create(&s, j.journal.clone())
            .await
            .unwrap_err()
            .code,
        BackupFailureCode::Unsupported
    );
    sqlx::query("ALTER TABLE memories ALTER COLUMN validity SET DEFAULT 'active'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE _sqlx_migrations ADD COLUMN unknown_credential text")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        v.manager
            .create(&s, j.journal.clone())
            .await
            .unwrap_err()
            .code,
        BackupFailureCode::Unsupported
    );
    sqlx::query("ALTER TABLE _sqlx_migrations DROP COLUMN unknown_credential")
        .execute(&pool)
        .await
        .unwrap();
    assert!(v.manager.create(&s, j.journal.clone()).await.is_ok());
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_real_restore_permission_failure_cleans_exact_database_and_preserves_source(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let b = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let before = fingerprint(&pool).await;
    let role = format!("backup_unpriv_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!(
        "CREATE ROLE \"{role}\" LOGIN CREATEDB NOSUPERUSER PASSWORD '{password}'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let options = pool
        .connect_options()
        .as_ref()
        .clone()
        .username(&role)
        .password(&password);
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .unwrap();
    let failure = v
        .manager
        .restore_current_isolated(&s, b.backup_id, &admin, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap_err();
    assert_eq!(failure.code, BackupFailureCode::ProcessFailed);
    assert!(failure.cleanup_complete);
    let id = failure.operation_id.unwrap();
    let name = format!("alia_restore_{}", id.simple());
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(name)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!exists);
    admin.close().await;
    sqlx::query(&format!("DROP ROLE \"{role}\""))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(before, fingerprint(&pool).await);
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_cleanup_refuses_changed_recovery_owner_marker(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let b = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let receipt = v
        .manager
        .restore_current_isolated(&s, b.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap();
    let name = receipt.database_name();
    sqlx::query(&format!(
        "COMMENT ON DATABASE \"{name}\" IS 'external-owner-do-not-drop'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        v.manager
            .cleanup_recovery(&pool, receipt.recovery_id)
            .await
            .unwrap_err()
            .code,
        BackupFailureCode::UnsafePath
    );
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(&name)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(exists);
    sqlx::query(&format!(
        "COMMENT ON DATABASE \"{name}\" IS 'alia-managed-recovery/1:{}'",
        receipt.recovery_id
    ))
    .execute(&pool)
    .await
    .unwrap();
    v.manager
        .cleanup_recovery(&pool, receipt.recovery_id)
        .await
        .unwrap();
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_explicit_first_enable_authority_and_preserved_clean_epoch_backup(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let before = fingerprint(&pool).await;
    v.manager
        .initialize_authority(&s, j.journal.clone())
        .await
        .unwrap();
    let state = s.snapshot().await.unwrap().unwrap();
    assert!(
        v.manager
            .invalidated_clean(state.id, state.privacy_epoch)
            .unwrap()
    );
    assert_eq!(before, fingerprint(&pool).await);
    let e = source_event(&pool, "DELETE-THEN-CLEAN-BACKUP").await;
    let m = memory(&pool, e, "DELETE-THEN-CLEAN-BACKUP").await;
    let old = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let r = s
        .begin_memory_deletion("then-clean", m, 1, j.journal.clone())
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    v.manager
        .invalidate_in(&mut tx, r.individual_id, r.privacy_epoch, j.journal.clone())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    s.purge_privacy_operations(16).await.unwrap();
    let deleted = v
        .manager
        .purge_invalidated(r.individual_id, r.privacy_epoch)
        .unwrap();
    assert_eq!(deleted.removed, 1);
    assert!(!v.artifact(old.backup_id, "archive.pgcustom").exists());
    let clean = v.manager.create(&s, j.journal.clone()).await.unwrap();
    assert_eq!(clean.privacy_epoch, r.privacy_epoch);
    assert!(v.manager.verify_current(&s, clean.backup_id).await.is_ok());
    let retained = v
        .manager
        .purge_invalidated(r.individual_id, r.privacy_epoch)
        .unwrap();
    assert_eq!(retained.removed, 0);
    assert_eq!(retained.remaining, 0);
    assert!(
        v.manager
            .invalidated_clean(r.individual_id, r.privacy_epoch)
            .unwrap()
    );
}
#[sqlx::test]
#[ignore = "P05 actual owned PG/custom archive; serial private resources"]
async fn managed_backup_nonblocking_original_journal_no_waiting_worker(pool: PgPool) {
    use std::fs::OpenOptions;
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::with_limits(BackupLimits {
        max_archive_bytes: 64 * 1024 * 1024,
        timeout: StdDuration::from_secs(2),
    });
    let b = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(j.dir.join("deletions.jsonl"))
        .unwrap();
    file.lock().unwrap();
    let start = std::time::Instant::now();
    let failed = v.manager.create(&s, j.journal.clone()).await.unwrap_err();
    let elapsed = start.elapsed();
    file.unlock().unwrap();
    assert_eq!(
        failed.code,
        BackupFailureCode::Busy,
        "original-log lock contention must fail promptly, without leaving a blocking worker"
    );
    assert!(elapsed < StdDuration::from_millis(500));
    file.lock().unwrap();
    let start = std::time::Instant::now();
    let failed = v
        .manager
        .restore_current_isolated(&s, b.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap_err();
    let elapsed = start.elapsed();
    file.unlock().unwrap();
    assert_eq!(failed.code, BackupFailureCode::Busy);
    assert!(elapsed < StdDuration::from_millis(500));
    assert!(j.journal.records().unwrap().is_empty());
    assert!(v.manager.create(&s, j.journal.clone()).await.is_ok());
}

#[sqlx::test]
#[ignore = "P05 actual owned PG; verify timeout under real writer gate"]
async fn managed_backup_query_verify_deadline_and_recovery_after_writer_unlock(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let b = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let bounded = ManagedBackupManager::open(
        &v.path,
        BackupLimits {
            timeout: StdDuration::from_millis(250),
            ..BackupLimits::default()
        },
    )
    .unwrap();
    let mut held = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
        .execute(&mut *held)
        .await
        .unwrap();
    let result = tokio::time::timeout(
        StdDuration::from_secs(1),
        bounded.verify_current(&s, b.backup_id),
    )
    .await;
    held.rollback().await.unwrap();
    assert!(
        matches!(&result, Ok(Err(e)) if e.code == BackupFailureCode::Timeout),
        "manager verify must report its own deadline, got {result:?}"
    );
    assert_eq!(v.manager.verify_current(&s, b.backup_id).await.unwrap(), b);
}

#[sqlx::test]
#[ignore = "P05 actual owned PG; precise recovery cleanup timeout and retry"]
async fn managed_backup_query_cleanup_deadline_preserves_exact_pending_recovery(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let b = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let receipt = v
        .manager
        .restore_current_isolated(&s, b.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap();
    let bounded = ManagedBackupManager::open(
        &v.path,
        BackupLimits {
            timeout: StdDuration::from_millis(250),
            ..BackupLimits::default()
        },
    )
    .unwrap();
    let single = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(StdDuration::from_secs(30))
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let held = single.acquire().await.unwrap();
    let result = tokio::time::timeout(
        StdDuration::from_secs(1),
        bounded.cleanup_recovery(&single, receipt.recovery_id),
    )
    .await;
    drop(held);
    single.close().await;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(receipt.database_name())
            .fetch_one(&pool)
            .await
            .unwrap();
    let pending = v
        .path
        .join("recoveries")
        .join(receipt.recovery_id.to_string());
    let private_receipt_preserved = pending.join("receipt.json").is_file();
    // Always clean the exactly owned target, including on the RED implementation.
    v.manager
        .cleanup_recovery(&pool, receipt.recovery_id)
        .await
        .unwrap();
    assert!(exists && private_receipt_preserved);
    assert!(!pending.exists());
    assert!(
        matches!(&result, Ok(Err(e)) if e.code == BackupFailureCode::Timeout && !e.cleanup_complete && e.operation_id == Some(receipt.recovery_id)),
        "manager cleanup must leave a safe retryable receipt at its own deadline, got {result:?}"
    );
}

#[sqlx::test]
#[ignore = "P05 actual owned PG; unknown database creation must retain retry evidence"]
async fn managed_backup_uncertain_creation_cannot_erase_receipt_on_absent_database(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let b = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let receipt = v
        .manager
        .restore_current_isolated(&s, b.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap();
    let pending = v
        .path
        .join("recoveries")
        .join(receipt.recovery_id.to_string());
    let receipt_file = pending.join("receipt.json");
    let mut stored: Value = serde_json::from_slice(&fs::read(&receipt_file).unwrap()).unwrap();
    // Simulate persisted state after a CREATE DATABASE request whose outcome is
    // unknown. A late server-side creation could still follow an absent row.
    sqlx::query(&format!(
        "DROP DATABASE \"{}\" WITH (FORCE)",
        receipt.database_name()
    ))
    .execute(&pool)
    .await
    .unwrap();
    stored["status"] = json!("creating_fenced");
    fs::write(&receipt_file, serde_json::to_vec(&stored).unwrap()).unwrap();
    let result = v.manager.cleanup_recovery(&pool, receipt.recovery_id).await;
    let preserved = receipt_file.is_file();
    if preserved {
        // The fixture knows its own DROP completed. This is not a production
        // API for declaring an uncertain CREATE safe or bypassing ownership.
        stored["status"] = json!("verified_fenced");
        fs::write(&receipt_file, serde_json::to_vec(&stored).unwrap()).unwrap();
        v.manager
            .cleanup_recovery(&pool, receipt.recovery_id)
            .await
            .unwrap();
    }
    assert!(
        matches!(&result, Err(e) if e.code == BackupFailureCode::Incomplete && e.operation_id == Some(receipt.recovery_id) && !e.cleanup_complete)
            && preserved,
        "unknown creation must preserve its opaque receipt instead of declaring cleanup complete: {result:?}"
    );
}

#[sqlx::test]
#[ignore = "P05 actual owned PG; definite CREATE denial permits precise cleanup"]
async fn managed_backup_uncertain_creation_definite_server_denial_cleans_metadata(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let b = v.manager.create(&s, j.journal.clone()).await.unwrap();
    let before = fingerprint(&pool).await;
    let role = format!("backup_no_create_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!(
        "CREATE ROLE \"{role}\" LOGIN NOCREATEDB NOSUPERUSER PASSWORD '{password}'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            pool.connect_options()
                .as_ref()
                .clone()
                .username(&role)
                .password(&password),
        )
        .await
        .unwrap();
    let failure = v
        .manager
        .restore_current_isolated(&s, b.backup_id, &admin, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap_err();
    let id = failure.operation_id.unwrap();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(format!("alia_restore_{}", id.simple()))
            .fetch_one(&pool)
            .await
            .unwrap();
    admin.close().await;
    sqlx::query(&format!("DROP ROLE \"{role}\""))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(failure.code, BackupFailureCode::Database);
    assert!(failure.cleanup_complete);
    assert!(!exists);
    assert!(!v.path.join("recoveries").join(id.to_string()).exists());
    assert_eq!(before, fingerprint(&pool).await);
}

// Reuse the P05 real plan/accounting fixture; no provider or model call.
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
#[ignore = "P05 actual schema30 PG17 archive/isolated recovery; no models"]
async fn schema30_backup_private_trial_copies_roundtrip_and_original_journal_removes_them(
    pool: PgPool,
) {
    let (s, target, plan) = trial_private_fixture(&pool).await;
    let j = Journal::new();
    let v = Vault::new();
    let before = fingerprint(&pool).await;
    let backup = v.manager.create(&s, j.journal.clone()).await.unwrap();
    assert_eq!(backup.schema_version, 30);
    let fresh = v
        .manager
        .restore_current_isolated(&s, backup.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap();
    let restored = connect_restored(&pool, &fresh.database_name()).await;
    assert_eq!(
        fingerprint(&restored).await,
        before,
        "every table including private measurement rows must roundtrip"
    );
    restored.close().await;
    v.manager
        .cleanup_recovery(&pool, fresh.recovery_id)
        .await
        .unwrap();
    let deleted = s
        .begin_memory_deletion("trial-backup-delete", target, 1, j.journal.clone())
        .await
        .unwrap();
    // Root still owns atomic installation of this hook; exercise its real
    // committed epoch/job in a separate explicit TX without claiming integration.
    let mut tx = pool.begin().await.unwrap();
    v.manager
        .invalidate_in(
            &mut tx,
            deleted.individual_id,
            deleted.privacy_epoch,
            j.journal.clone(),
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 1);
    assert!(
        v.manager
            .verify_current(&s, backup.backup_id)
            .await
            .is_err()
    );
    let source_after = fingerprint(&pool).await;
    let receipt = v
        .manager
        .recover_quarantined(backup.backup_id, &pool, &j.dir.join("deletions.jsonl"))
        .await
        .unwrap();
    assert_eq!(receipt.status, "verified_fenced");
    let restored = connect_restored(&pool, &receipt.database_name()).await;
    trial_clean(&restored, plan.id).await;
    for table in [
        "development_trial_plans",
        "development_trial_cases",
        "development_trial_attempts",
        "events",
    ] {
        let remains: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM {table} t WHERE to_jsonb(t)::text LIKE '%TRIAL-PRIVATE%')"
        ))
        .fetch_one(&restored)
        .await
        .unwrap();
        assert!(!remains, "private body revived in {table}");
    }
    let recovered = Store::new(restored.clone());
    let state = recovered.snapshot().await.unwrap().unwrap();
    assert_eq!(state.id, backup.individual_id);
    assert!(
        recovered
            .recover_development_trial_work(plan.id)
            .await
            .is_err()
    );
    let archive = recovered.export_individual().await.unwrap();
    assert!(archive.tables["development_trial_attempts"].is_empty());
    assert_eq!(archive.tables["usage_reservations"].len(), 2);
    restored.close().await;
    v.manager
        .cleanup_recovery(&pool, receipt.recovery_id)
        .await
        .unwrap();
    assert_eq!(
        fingerprint(&pool).await,
        source_after,
        "isolated recovery changed source"
    );
    let remains: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=ANY($1))")
            .bind(vec![fresh.database_name(), receipt.database_name()])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!remains, "owned recovery database leaked");
}
