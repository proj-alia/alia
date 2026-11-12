//! Real isolated PG and private durable journals. Synthetic content, no live models.
use alia::{
    adapters::{
        deletion_journal::{DeletionJournal, JournalIntent},
        store::{Store, StoreError},
    },
    domain::{
        config::{PersonaSeed, RuntimeRules},
        privacy::DeletionStatus,
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt, path::PathBuf, sync::Arc};
use uuid::Uuid;
struct Journal {
    dir: PathBuf,
    path: PathBuf,
    journal: Arc<DeletionJournal>,
}
impl Journal {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("alia-p05-pipeline-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let path = dir.join("deletion.jsonl");
        let journal = Arc::new(DeletionJournal::open(&path).unwrap());
        Self { dir, path, journal }
    }
}
impl Drop for Journal {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
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
async fn owner(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM individuals")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn obj(pool: &PgPool, kind: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,$2 FROM individuals")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    id
}
async fn event(pool: &PgPool, text: &str) -> Uuid {
    let id = obj(pool, "event").await;
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id,idempotency_key) SELECT id,$1,'message.received','reported_claim',$2,clock_timestamp(),jsonb_build_object('text',$2::text),$1,$2 FROM individuals").bind(id).bind(text).execute(pool).await.unwrap();
    id
}
async fn memory(pool: &PgPool, source: Uuid, text: &str) -> Uuid {
    let id = obj(pool, "memory").await;
    sqlx::query("INSERT INTO memories(individual_id,id,kind,content,scope) SELECT id,$1,'semantic',$2,$2 FROM individuals").bind(id).bind(text).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,$1,$2 FROM individuals").bind(id).bind(source).execute(pool).await.unwrap();
    id
}
async fn edge(pool: &PgPool, source: Uuid, target: Uuid) {
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT id,$1,$2 FROM individuals ON CONFLICT DO NOTHING").bind(source).bind(target).execute(pool).await.unwrap();
}
async fn fingerprint(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('identity',(SELECT to_jsonb(i) FROM individuals i),'events',(SELECT md5(jsonb_agg(to_jsonb(e) ORDER BY id)::text) FROM events e),'objects',(SELECT md5(jsonb_agg(to_jsonb(o) ORDER BY id)::text) FROM objects o),'memories',(SELECT md5(jsonb_agg(to_jsonb(m) ORDER BY id)::text) FROM memories m),'jobs',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM privacy_operations p),'outbox',(SELECT count(*) FROM outbox),'usage',(SELECT md5(jsonb_agg(to_jsonb(u) ORDER BY id)::text) FROM usage_reservations u))").fetch_one(pool).await.unwrap()
}
fn hash(v: &str) -> String {
    format!("{:x}", Sha256::digest(v.as_bytes()))
}
fn intent(individual: Uuid, target: Uuid, root: Uuid, key: &str) -> JournalIntent {
    let key = hash(key);
    let request =
        json!({"idempotency_key_digest":key,"target_memory_id":target,"expected_memory_version":1});
    JournalIntent {
        operation_id: Uuid::new_v4(),
        individual_id: individual,
        target_memory_id: target,
        expected_memory_version: 1,
        root_event_ids: vec![root],
        idempotency_key_digest: Some(key),
        request_digest: format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&request).unwrap())
        ),
    }
}
#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_lost_acceptance_idempotency_current_progress_and_independent_read(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let root = event(&pool, "PRIVATE-FIRST").await;
    let target = memory(&pool, root, "PRIVATE-FIRST").await;
    let safe_event = event(&pool, "SAFE-SECOND").await;
    let safe = memory(&pool, safe_event, "SAFE-SECOND").await;
    let before = s.snapshot().await.unwrap().unwrap();
    let receipt = s
        .begin_memory_deletion("PRIVATE-FIRST-key", target, 1, j.journal.clone())
        .await
        .unwrap();
    assert_eq!(receipt.status, DeletionStatus::Blocked);
    assert!(receipt.privacy_epoch > before.privacy_epoch);
    assert!(receipt.state_version > before.state_version);
    assert!(s.privacy_ready());
    assert_eq!(
        s.begin_memory_deletion("PRIVATE-FIRST-key", target, 1, j.journal.clone())
            .await
            .unwrap(),
        receipt
    );
    assert!(matches!(
        s.begin_memory_deletion("PRIVATE-FIRST-key", safe, 1, j.journal.clone())
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert!(matches!(
        s.begin_memory_deletion("PRIVATE-FIRST-key", target, 2, j.journal.clone())
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert!(
        !fs::read_to_string(&j.path)
            .unwrap()
            .contains("PRIVATE-FIRST")
    );
    assert_eq!(j.journal.records().unwrap().len(), 1);
    assert_eq!(s.purge_privacy_operations(1).await.unwrap(), 1);
    let p = s.privacy_operation(receipt.operation_id).await.unwrap();
    assert_eq!(p.status, DeletionStatus::Completed);
    assert!(p.last_error.is_none());
    assert_eq!(
        s.begin_memory_deletion("PRIVATE-FIRST-key", target, 1, j.journal.clone())
            .await
            .unwrap()
            .status,
        DeletionStatus::Completed
    );
    assert_eq!(s.purge_privacy_operations(16).await.unwrap(), 0);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT content FROM memories WHERE id=$1")
            .bind(safe)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "SAFE-SECOND"
    );
    assert!(matches!(
        s.privacy_operation(Uuid::new_v4()).await,
        Err(StoreError::NotFound)
    ));
    let clean = fingerprint(&pool).await;
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert_eq!(fingerprint(&pool).await, clean);
    assert!(s.privacy_ready());
}
#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_fsync_then_late_db_rollback_fences_all_clones_and_recovers_exactly_once(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let clone = s.clone();
    let j = Journal::new();
    let root = event(&pool, "ROLLBACK-PRIVATE").await;
    let target = memory(&pool, root, "ROLLBACK-PRIVATE").await;
    let before = fingerprint(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION p05_fail_job() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic late job failure'; END $$; CREATE TRIGGER p05_late BEFORE INSERT ON privacy_operations FOR EACH ROW EXECUTE FUNCTION p05_fail_job()") .execute(&pool).await.unwrap();
    assert!(
        s.begin_memory_deletion("lost-job-key", target, 1, j.journal.clone())
            .await
            .is_err()
    );
    assert!(!s.privacy_ready());
    assert!(!clone.privacy_ready());
    assert!(matches!(
        clone.snapshot().await,
        Err(StoreError::PrivacyUnavailable)
    ));
    assert!(matches!(
        clone
            .submit_message("must-not-write", "no new context", None)
            .await,
        Err(StoreError::PrivacyUnavailable)
    ));
    assert_eq!(j.journal.records().unwrap().len(), 1);
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::raw_sql("DROP TRIGGER p05_late ON privacy_operations; DROP FUNCTION p05_fail_job()")
        .execute(&pool)
        .await
        .unwrap();
    let reopened = Arc::new(DeletionJournal::open(&j.path).unwrap());
    s.replay_deletion_journal(reopened.clone()).await.unwrap();
    assert!(clone.privacy_ready());
    let receipt = s
        .begin_memory_deletion("lost-job-key", target, 1, reopened.clone())
        .await
        .unwrap();
    assert_eq!(receipt.status, DeletionStatus::Completed);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE kind='privacy.blocked'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let after = fingerprint(&pool).await;
    s.replay_deletion_journal(reopened).await.unwrap();
    assert_eq!(fingerprint(&pool).await, after);
}
#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_purge_fault_atomic_retry_pending_and_unknown_schema_kind_fail_closed(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let root = event(&pool, "RETRY-PRIVATE").await;
    let target = memory(&pool, root, "RETRY-PRIVATE").await;
    let receipt = s
        .begin_memory_deletion("retry", target, 1, j.journal.clone())
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION p05_fail_purge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic purge rollback'; END $$; CREATE TRIGGER p05_purge BEFORE UPDATE ON memories FOR EACH ROW EXECUTE FUNCTION p05_fail_purge()").execute(&pool).await.unwrap();
    assert_eq!(s.purge_privacy_operations(1).await.unwrap(), 0);
    assert_eq!(
        s.privacy_operation(receipt.operation_id)
            .await
            .unwrap()
            .status,
        DeletionStatus::RetryPending
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT content FROM memories WHERE id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "RETRY-PRIVATE"
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT blocked FROM objects WHERE id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    sqlx::raw_sql("DROP TRIGGER p05_purge ON memories; DROP FUNCTION p05_fail_purge(); ALTER TABLE memories ADD COLUMN future_private text").execute(&pool).await.unwrap();
    assert_eq!(s.purge_privacy_operations(1).await.unwrap(), 0);
    assert_eq!(
        s.privacy_operation(receipt.operation_id)
            .await
            .unwrap()
            .status,
        DeletionStatus::RetryPending
    );
    sqlx::raw_sql("ALTER TABLE memories DROP COLUMN future_private; ALTER TABLE objects DROP CONSTRAINT objects_kind_check").execute(&pool).await.unwrap();
    sqlx::query("UPDATE objects SET kind='future_kind' WHERE id=$1")
        .bind(target)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(s.purge_privacy_operations(1).await.unwrap(), 0);
    assert_eq!(
        s.privacy_operation(receipt.operation_id)
            .await
            .unwrap()
            .status,
        DeletionStatus::RetryPending
    );
    sqlx::query("UPDATE objects SET kind='memory' WHERE id=$1")
        .bind(target)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(s.purge_privacy_operations(1).await.unwrap(), 1);
    assert_eq!(
        s.privacy_operation(receipt.operation_id)
            .await
            .unwrap()
            .status,
        DeletionStatus::Completed
    );
}
#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_completed_old_backup_reexpands_graph_and_cleans_new_descendants(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let root = event(&pool, "RESTORED-PRIVATE").await;
    let target = memory(&pool, root, "RESTORED-PRIVATE").await;
    let receipt = s
        .begin_memory_deletion("restore-key", target, 1, j.journal.clone())
        .await
        .unwrap();
    s.purge_privacy_operations(1).await.unwrap();
    // Labeled simulated old DB restoration: real SQL puts old plaintext back while
    // completed metadata remains, then a late reference missing in original job.
    s.fence_privacy();
    sqlx::query(
        "UPDATE events SET payload=jsonb_build_object('text','RESTORED-PRIVATE') WHERE id=$1",
    )
    .bind(root)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET content='RESTORED-PRIVATE' WHERE id=$1")
        .bind(target)
        .execute(&pool)
        .await
        .unwrap();
    let late = memory(&pool, root, "LATE-RESTORED-PRIVATE").await;
    let old_epoch = receipt.privacy_epoch;
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert!(s.privacy_ready());
    let p = s.privacy_operation(receipt.operation_id).await.unwrap();
    assert_eq!(p.status, DeletionStatus::Completed);
    assert!(p.privacy_epoch > old_epoch);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT content FROM memories WHERE id=$1")
            .bind(late)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "[redacted]"
    );
    let fixed = fingerprint(&pool).await;
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert_eq!(fingerprint(&pool).await, fixed);
}
#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_journal_corrupt_legacy_wrong_identity_and_missing_database_refuse_readiness(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let root = event(&pool, "STARTUP-PRIVATE").await;
    let target = memory(&pool, root, "STARTUP-PRIVATE").await;
    let id = owner(&pool).await;
    let legacy = Journal::new();
    let mut v = intent(id, target, root, "legacy");
    v.idempotency_key_digest = None;
    legacy.journal.append(&v).unwrap();
    assert!(
        s.replay_deletion_journal(legacy.journal.clone())
            .await
            .is_err()
    );
    assert!(!s.privacy_ready());
    let wrong = Journal::new();
    wrong
        .journal
        .append(&intent(Uuid::new_v4(), target, root, "wrong"))
        .unwrap();
    assert!(
        s.replay_deletion_journal(wrong.journal.clone())
            .await
            .is_err()
    );
    assert!(!s.privacy_ready());
    let bad = Journal::new();
    bad.journal
        .append(&intent(id, target, root, "corrupt"))
        .unwrap();
    let bytes = fs::read(&bad.path).unwrap();
    fs::write(&bad.path, &bytes[..bytes.len() - 1]).unwrap();
    assert!(
        s.replay_deletion_journal(bad.journal.clone())
            .await
            .is_err()
    );
    assert!(!s.privacy_ready());
    let valid = Journal::new();
    valid
        .journal
        .append(&intent(id, target, root, "missing-db"))
        .unwrap();
    sqlx::query("TRUNCATE individuals CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        s.replay_deletion_journal(valid.journal.clone())
            .await
            .is_err()
    );
    assert!(!s.privacy_ready());
}
#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_actual_dispatched_budget_and_old_context_lease_fenced_without_refund(
    pool: PgPool,
) {
    use alia::runtime::{model::TokenUsage, scheduling::TokenAllowance};
    let s = setup(&pool).await;
    let j = Journal::new();
    let submitted = s
        .submit_message("lease-source", "actual queued input", None)
        .await
        .unwrap();
    let target = memory(&pool, submitted.event_id, "old selected knowledge").await;
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let caps = s.local_capabilities().await.unwrap();
    let context = s.model_context(lease.run_id, caps).await.unwrap();
    let allowance = TokenAllowance {
        input_upper_bound: 10,
        output_cap: 7,
    };
    let reserve = s
        .reserve_model_call(&lease, Uuid::new_v4(), &context, allowance)
        .await
        .unwrap();
    assert!(s.dispatch_model_call(&lease, reserve.id).await.unwrap());
    s.begin_memory_deletion("lease-delete", target, 1, j.journal.clone())
        .await
        .unwrap();
    let charged: (i64, bool) =
        sqlx::query_as("SELECT charged_tokens,usage_unknown FROM usage_reservations WHERE id=$1")
            .bind(reserve.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(charged, (17, true));
    assert!(
        s.reserve_model_call(&lease, Uuid::new_v4(), &context, allowance)
            .await
            .is_err()
    );
    assert!(s.model_context(lease.run_id, vec![]).await.is_err());
    let late = s
        .settle_model_call(
            reserve.id,
            Some(TokenUsage {
                input: 3,
                output: 2,
                total: 5,
            }),
        )
        .await
        .unwrap();
    assert_eq!(late.charged_tokens, 17);
    s.purge_privacy_operations(1).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT charged_tokens FROM usage_reservations WHERE id=$1")
            .bind(reserve.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        17
    );
}

#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_at_rest_all_known_body_json_vector_and_relationship_revision_fields_cleared(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let marker = "P05_ERASE_ALL_PRIVATE";
    let root = event(&pool, marker).await;
    let target = memory(&pool, root, marker).await;
    let independent = event(&pool, "SAFE-INDEPENDENT").await;
    let safe = memory(&pool, independent, "SAFE-INDEPENDENT").await;
    sqlx::query("INSERT INTO documents(individual_id,event_id,title,source_label,body) SELECT id,$1,$2,$2,$2 FROM individuals").bind(root).bind(marker).execute(&pool).await.unwrap();
    let goal = obj(&pool, "goal").await;
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition,waiting_reason,completion) SELECT id,$1,'event',jsonb_build_array($2::uuid::text),$3,$3,$3,$3,$3,jsonb_build_object('private',$3::text) FROM individuals").bind(goal).bind(root).bind(marker).execute(&pool).await.unwrap();
    let subject = obj(&pool, "relationship").await;
    sqlx::query("INSERT INTO relationships(individual_id,subject_id,subject_key,stage,expectations,boundaries,commitments,unresolved) SELECT id,$1,$2,$2,jsonb_build_array($2::text),jsonb_build_array($2::text),jsonb_build_array($2::text),jsonb_build_array($2::text) FROM individuals").bind(subject).bind(marker).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT id,$1,$2 FROM individuals").bind(subject).bind(root).execute(&pool).await.unwrap();
    let revision = obj(&pool, "relationship_revision").await;
    sqlx::query("INSERT INTO relationship_revisions(individual_id,id,subject_id,before_version,after_version,before_state,after_state,brief_basis) SELECT id,$1,$2,1,2,jsonb_build_object('private',$3::text),jsonb_build_object('private',$3::text),$3 FROM individuals").bind(revision).bind(subject).bind(marker).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO relationship_revision_events(individual_id,revision_id,event_id) SELECT id,$1,$2 FROM individuals").bind(revision).bind(root).execute(&pool).await.unwrap();
    let stance = obj(&pool, "belief").await;
    sqlx::query("INSERT INTO beliefs(individual_id,id,statement,scope,position,brief_basis) SELECT id,$1,$2,$2,'held',$2 FROM individuals").bind(stance).bind(marker).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO belief_evidence(individual_id,belief_id,event_id,role) SELECT id,$1,$2,'counter' FROM individuals").bind(stance).bind(root).execute(&pool).await.unwrap();
    let story = obj(&pool, "autobiography").await;
    sqlx::query("INSERT INTO autobiographies(individual_id,id,topic,title,interpretation) SELECT id,$1,'private_topic',$2,$2 FROM individuals").bind(story).bind(marker).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO autobiography_events(individual_id,autobiography_id,event_id) SELECT id,$1,$2 FROM individuals").bind(story).bind(independent).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO autobiography_beliefs(individual_id,autobiography_id,belief_id) SELECT id,$1,$2 FROM individuals").bind(story).bind(stance).execute(&pool).await.unwrap();
    let appraisal = obj(&pool, "affective_state").await;
    sqlx::query("INSERT INTO affective_appraisals(individual_id,id,goal_id,subject_id,valence,arousal,goal_relevance,expectation_gap,controllability,uncertainty,brief_basis,expires_at) SELECT id,$1,$2,$3,0.5,0.5,0.5,0.5,0.5,0.5,$4,clock_timestamp()+interval '1 hour' FROM individuals").bind(appraisal).bind(goal).bind(subject).bind(marker).execute(&pool).await.unwrap();
    let attitude = obj(&pool, "affective_attitude").await;
    sqlx::query("INSERT INTO affective_attitudes(individual_id,id,topic,subject_id,stance,brief_basis) SELECT id,$1,'private_attitude',$2,$3,$3 FROM individuals").bind(attitude).bind(subject).bind(marker).execute(&pool).await.unwrap();
    for id in [appraisal, attitude] {
        sqlx::query("INSERT INTO affective_sources(individual_id,object_id,event_id) SELECT id,$1,$2 FROM individuals").bind(id).bind(root).execute(&pool).await.unwrap();
    }
    let entity = obj(&pool, "entity").await;
    sqlx::query("INSERT INTO person_entities(individual_id,id,external_key,label) SELECT id,$1,$2,$2 FROM individuals").bind(entity).bind(marker).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO person_aliases(individual_id,entity_id,surface,source_event_id,assertion_event_id) SELECT id,$1,$2,$3,$3 FROM individuals").bind(entity).bind(marker).bind(root).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO memory_person_links(individual_id,memory_id,memory_version,content_digest,entity_id,role,source_event_id,assertion_event_id) SELECT id,$1,1,$2,$3,'subject',$4,$4 FROM individuals").bind(target).bind(hash(marker)).bind(entity).bind(root).execute(&pool).await.unwrap();
    let run = obj(&pool, "run").await;
    sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,goal_id,mode,state_version,privacy_epoch,config_version,input_manifest) SELECT id,$1,$2,$3,'foreground',state_version,privacy_epoch,config_version,$4 FROM individuals").bind(run).bind(root).bind(goal).bind(vec![root,target]).execute(&pool).await.unwrap();
    let decision = event(&pool, marker).await;
    let result = event(&pool, marker).await;
    let reply = event(&pool, marker).await;
    let review = event(&pool, marker).await;
    sqlx::query("UPDATE events SET correlation_id=$2 WHERE id=ANY($1)")
        .bind(vec![decision, result, reply, review])
        .bind(run)
        .execute(&pool)
        .await
        .unwrap();
    let action = obj(&pool, "action").await;
    sqlx::query("INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments,status,result_event_id,receipt) SELECT id,$1,$2,$3,1,'notes.write','1',jsonb_build_object('text',$4::text),'succeeded',$5,jsonb_build_object('private',$4::text) FROM individuals").bind(action).bind(run).bind(decision).bind(marker).bind(result).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO notes(individual_id,action_id,title,body,citations) SELECT id,$1,$2,$2,jsonb_build_array(jsonb_build_object('document_id',$3::uuid,'start_char',0,'end_char',1)) FROM individuals").bind(action).bind(marker).bind(root).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO outgoing_messages(individual_id,draft_event_id,run_id,mode,privacy_epoch,status,delivered_event_id,delivered_at,cancellation_reason) SELECT id,$1,$2,'foreground',privacy_epoch,'delivered',$1,clock_timestamp(),$3 FROM individuals").bind(reply).bind(run).bind(marker).execute(&pool).await.unwrap();
    let window = Uuid::new_v4();
    sqlx::query("INSERT INTO budget_windows(id,individual_id,timezone,starts_at,ends_at,charged_total) SELECT $1,id,'UTC',clock_timestamp()-interval '1 day',clock_timestamp()+interval '1 day',22 FROM individuals").bind(window).execute(&pool).await.unwrap();
    let mut reservations = vec![];
    for _ in 0..2 {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO usage_reservations(id,individual_id,run_id,lease_generation,request_key,request_digest,window_id,mode,input_bound,output_cap,reserved_tokens,status,charged_tokens,actual_input,actual_output,actual_total,settled_at) SELECT $1,id,$2,0,$3,$4,$5,'foreground',3,8,11,'settled',11,3,8,11,clock_timestamp() FROM individuals").bind(id).bind(run).bind(Uuid::new_v4()).bind(hash(marker)).bind(window).execute(&pool).await.unwrap();
        reservations.push(id);
    }
    sqlx::query("INSERT INTO decision_commits(reservation_id,individual_id,run_id,lease_generation,privacy_epoch,proposal_digest,decision_event_id,receipt) SELECT $1,id,$2,0,0,$3,$4,jsonb_build_object('private',$5::text) FROM individuals").bind(reservations[0]).bind(run).bind(hash(marker)).bind(decision).bind(marker).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO decision_reviews(author_reservation_id,review_reservation_id,individual_id,run_id,event_id,lease_generation,state_version,privacy_epoch,config_version,draft_digest,context_digest,review_context_digest,assessment,supported,checker_version,provider_mode) SELECT $1,$2,id,$3,$4,0,1,0,1,$5,$5,$5,jsonb_build_object('private',$6::text),true,$6,'fixture' FROM individuals").bind(reservations[0]).bind(reservations[1]).bind(run).bind(review).bind(hash(marker)).bind(marker).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO goal_reviews(individual_id,event_id,goal_id,goal_revision,privacy_epoch,input_digest,reservation_id,assessment,state_version) SELECT id,$1,$2,1,0,$3,$4,jsonb_build_object('private',$5::text),1 FROM individuals").bind(review).bind(goal).bind(hash(marker)).bind(reservations[1]).bind(marker).execute(&pool).await.unwrap();
    let embedding = format!("[1,{}]", vec!["0"; 383].join(","));
    sqlx::query("INSERT INTO memory_vector_sets(individual_id,memory_id,profile,memory_version,privacy_epoch,content_digest,chunk_count) SELECT id,$1,'p05',1,0,$2,1 FROM individuals").bind(target).bind(hash(marker)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO memory_vectors(individual_id,memory_id,profile,ordinal,start_char,end_char,embedding) SELECT id,$1,'p05',0,0,1,$2::vector FROM individuals").bind(target).bind(embedding).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO memory_index_jobs(individual_id,memory_id,profile,memory_version,privacy_epoch,content_digest,status) SELECT id,$1,'p05',1,0,$2,'queued' FROM individuals").bind(target).bind(hash(marker)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO idempotency_records(actor_id,route,key,body_digest,response) VALUES('owner','p05',$1,$2,jsonb_build_object('memory_id',$3::uuid,'text',$1::text)),('owner','p05','safe-key',$2,jsonb_build_object('memory_id',$4::uuid))").bind(marker).bind(hash(marker)).bind(target).bind(safe).execute(&pool).await.unwrap();
    edge(&pool, root, target).await;
    edge(&pool, target, root).await;
    let billing_before: Value =
        sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(u) ORDER BY id) FROM usage_reservations u")
            .fetch_one(&pool)
            .await
            .unwrap();
    let receipt = s
        .begin_memory_deletion("all-tables", target, 1, j.journal.clone())
        .await
        .unwrap();
    assert_eq!(s.purge_privacy_operations(1).await.unwrap(), 1);
    assert_eq!(
        s.privacy_operation(receipt.operation_id)
            .await
            .unwrap()
            .status,
        DeletionStatus::Completed
    );
    let tables:Vec<String>=sqlx::query_scalar("SELECT tablename FROM pg_tables WHERE schemaname=current_schema() AND left(tablename,1)<>'_' ORDER BY tablename").fetch_all(&pool).await.unwrap();
    for table in tables {
        assert!(table.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} t WHERE to_jsonb(t)::text LIKE $1"
        ))
        .bind(format!("%{marker}%"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "raw private marker remains in {table}");
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM memory_vectors WHERE memory_id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM memory_index_jobs WHERE memory_id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT content FROM memories WHERE id=$1")
            .bind(safe)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "SAFE-INDEPENDENT"
    );
    assert_eq!(
        sqlx::query_scalar::<_, Value>(
            "SELECT jsonb_agg(to_jsonb(u) ORDER BY id) FROM usage_reservations u"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        billing_before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM idempotency_records WHERE key='safe-key'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_after_actual_erasure_before_complete_fault_and_restart_replay(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let root = event(&pool, "FINAL-COMMIT-PRIVATE").await;
    let target = memory(&pool, root, "FINAL-COMMIT-PRIVATE").await;
    let r = s
        .begin_memory_deletion("final-commit", target, 1, j.journal.clone())
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION p05_fail_complete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.status='completed' THEN RAISE EXCEPTION 'synthetic after-erasure failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER p05_complete BEFORE UPDATE ON privacy_operations FOR EACH ROW EXECUTE FUNCTION p05_fail_complete()").execute(&pool).await.unwrap();
    assert_eq!(s.purge_privacy_operations(1).await.unwrap(), 0);
    assert_eq!(
        s.privacy_operation(r.operation_id).await.unwrap().status,
        DeletionStatus::RetryPending
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT payload->>'text' FROM events WHERE id=$1")
            .bind(root)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "FINAL-COMMIT-PRIVATE"
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT blocked FROM objects WHERE id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    sqlx::raw_sql(
        "DROP TRIGGER p05_complete ON privacy_operations; DROP FUNCTION p05_fail_complete()",
    )
    .execute(&pool)
    .await
    .unwrap();
    let restarted = Store::new(pool.clone());
    let reopened = Arc::new(DeletionJournal::open(&j.path).unwrap());
    restarted
        .replay_deletion_journal(reopened.clone())
        .await
        .unwrap();
    assert_eq!(
        restarted
            .privacy_operation(r.operation_id)
            .await
            .unwrap()
            .status,
        DeletionStatus::Completed
    );
    let clean = fingerprint(&pool).await;
    restarted.replay_deletion_journal(reopened).await.unwrap();
    assert_eq!(fingerprint(&pool).await, clean);
}
#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_all_lifecycles_queued_planned_cancellation_and_bounds(pool: PgPool) {
    let s = setup(&pool).await;
    let j = Journal::new();
    for (n, lifecycle) in ["active", "paused", "archived", "terminated"]
        .iter()
        .enumerate()
    {
        sqlx::query("UPDATE individuals SET lifecycle=$1")
            .bind(lifecycle)
            .execute(&pool)
            .await
            .unwrap();
        let source = event(&pool, "LIFECYCLE-PRIVATE").await;
        let target = memory(&pool, source, "LIFECYCLE-PRIVATE").await;
        let run = obj(&pool, "run").await;
        sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,input_manifest) SELECT id,$1,$2,'foreground',state_version,privacy_epoch,config_version,$3 FROM individuals").bind(run).bind(source).bind(vec![target]).execute(&pool).await.unwrap();
        let action = obj(&pool, "action").await;
        sqlx::query("INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments) SELECT id,$1,$2,$3,1,'notes.write','1','{}' FROM individuals").bind(action).bind(run).bind(source).execute(&pool).await.unwrap();
        let r = s
            .begin_memory_deletion(&format!("lifecycle-{n}"), target, 1, j.journal.clone())
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id=$1")
                .bind(run)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "stale"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM actions WHERE id=$1")
                .bind(action)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "cancelled"
        );
        assert_eq!(s.purge_privacy_operations(1).await.unwrap(), 1);
        assert_eq!(
            s.privacy_operation(r.operation_id).await.unwrap().status,
            DeletionStatus::Completed
        );
    }
    for limit in [0, 17] {
        assert!(matches!(
            s.purge_privacy_operations(limit).await,
            Err(StoreError::Invalid(_))
        ));
    }
    for key in ["", "bad key"] {
        assert!(matches!(
            s.begin_memory_deletion(key, Uuid::new_v4(), 1, j.journal.clone())
                .await,
            Err(StoreError::Invalid(_))
        ));
    }
}
#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_persisted_journal_binding_mismatch_refuses_replay_without_data_revival(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let root = event(&pool, "BINDING-PRIVATE").await;
    let target = memory(&pool, root, "BINDING-PRIVATE").await;
    let r = s
        .begin_memory_deletion("binding", target, 1, j.journal.clone())
        .await
        .unwrap();
    let original: String =
        sqlx::query_scalar("SELECT journal_digest FROM privacy_operations WHERE id=$1")
            .bind(r.operation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE privacy_operations SET journal_digest=$2 WHERE id=$1")
        .bind(r.operation_id)
        .bind("0".repeat(64))
        .execute(&pool)
        .await
        .unwrap();
    assert!(s.replay_deletion_journal(j.journal.clone()).await.is_err());
    assert!(!s.privacy_ready());
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT blocked FROM objects WHERE id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    sqlx::query("UPDATE privacy_operations SET journal_digest=$2 WHERE id=$1")
        .bind(r.operation_id)
        .bind(original)
        .execute(&pool)
        .await
        .unwrap();
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
    assert_eq!(
        s.privacy_operation(r.operation_id).await.unwrap().status,
        DeletionStatus::Completed
    );
}

#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_identity_seed_cannot_be_erased_as_ordinary_memory(pool: PgPool) {
    let s = setup(&pool).await;
    let birth: Uuid = sqlx::query_scalar("SELECT id FROM events WHERE kind='individual.created'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let target = memory(&pool, birth, "birth-seed-derived memory").await;
    assert!(matches!(
        s.memory_deletion_closure(target, 1).await,
        Err(StoreError::Invalid("identity anchor deletion unsupported"))
    ));
    sqlx::query(
        "DELETE FROM derivation_edges WHERE individual_id=$1 AND source_id=$2 AND target_id=$1",
    )
    .bind(owner(&pool).await)
    .bind(birth)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        s.memory_deletion_closure(target, 1).await,
        Err(StoreError::Invalid("identity anchor deletion unsupported"))
    ));
}

#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_stable_logical_interest_mapping_current_and_history_without_derivation_edges(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let j = Journal::new();
    let source = event(&pool, "INTEREST-PRIVATE").await;
    let target = memory(&pool, source, "INTEREST-PRIVATE").await;
    let object = obj(&pool, "interest").await;
    let logical = format!("interest:{}", Uuid::new_v4());
    sqlx::query("INSERT INTO interests(individual_id,id,object_id,description,origin_kind) SELECT id,$1,$2,'INTEREST-PRIVATE','experience' FROM individuals").bind(&logical).bind(object).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO interest_sources(individual_id,interest_id,event_id) SELECT id,$1,$2 FROM individuals").bind(&logical).bind(source).execute(&pool).await.unwrap();
    let first = obj(&pool, "interest_revision").await;
    let newer = obj(&pool, "interest_revision").await;
    for (id, previous, before, after) in [
        (first, None, 0_i64, 1_i64),
        (newer, Some(first), 1_i64, 2_i64),
    ] {
        sqlx::query("INSERT INTO interest_revisions(individual_id,id,interest_id,before_version,after_version,before_description,after_description,after_status,brief_basis,event_id,previous_revision_id) SELECT id,$1,$2,$3,$4,'INTEREST-PRIVATE','INTEREST-PRIVATE','active','INTEREST-PRIVATE',$5,$6 FROM individuals").bind(id).bind(&logical).bind(before).bind(after).bind(source).bind(previous).execute(&pool).await.unwrap();
    }
    let goal = obj(&pool, "goal").await;
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition) SELECT id,$1,'interest',jsonb_build_array($2::text),'INTEREST-PRIVATE','INTEREST-PRIVATE','INTEREST-PRIVATE','INTEREST-PRIVATE' FROM individuals").bind(goal).bind(&logical).execute(&pool).await.unwrap();
    let closure = s.memory_deletion_closure(target, 1).await.unwrap();
    for id in [object, first, newer, goal] {
        assert!(closure.object_ids.contains(&id));
    }
    assert!(closure.root_event_ids.contains(&source));
    assert!(!closure.root_event_ids.contains(&object));
    let r = s
        .begin_memory_deletion("interest-delete", target, 1, j.journal.clone())
        .await
        .unwrap();
    assert_eq!(s.purge_privacy_operations(1).await.unwrap(), 1);
    assert_eq!(
        s.privacy_operation(r.operation_id).await.unwrap().status,
        DeletionStatus::Completed
    );
    let stored: (String, String, String) =
        sqlx::query_as("SELECT id,description,status FROM interests WHERE object_id=$1")
            .bind(object)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, (logical, "[redacted]".into(), "retired".into()));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM interest_revisions WHERE before_description IS NOT NULL OR after_description<>'[redacted]' OR brief_basis<>'[redacted]'").fetch_one(&pool).await.unwrap(),0);
    s.replay_deletion_journal(j.journal.clone()).await.unwrap();
}

#[sqlx::test]
#[ignore = "P05 actual private PG/journal; entire parent heavy lock"]
async fn deletion_concurrent_purgers_count_one_completion_and_notification(pool: PgPool) {
    let s = setup(&pool).await;
    let other = s.clone();
    let j = Journal::new();
    for n in 0..4 {
        let root = event(&pool, "CONCURRENT-PRIVATE").await;
        let target = memory(&pool, root, "CONCURRENT-PRIVATE").await;
        let r = s
            .begin_memory_deletion(&format!("concurrent-{n}"), target, 1, j.journal.clone())
            .await
            .unwrap();
        let before: i64 =
            sqlx::query_scalar("SELECT count(*) FROM outbox WHERE event_type='privacy.changed'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let (a, b) = tokio::join!(
            s.purge_privacy_operations(1),
            other.purge_privacy_operations(1)
        );
        assert_eq!(a.unwrap() + b.unwrap(), 1, "same operation completed twice");
        let after: i64 =
            sqlx::query_scalar("SELECT count(*) FROM outbox WHERE event_type='privacy.changed'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after - before, 1, "duplicate completion notification");
        assert_eq!(
            s.privacy_operation(r.operation_id).await.unwrap().status,
            DeletionStatus::Completed
        );
    }
}
