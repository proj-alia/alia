//! Real PG and private SDK metadata. These tests never run Python or Docker.
//! The future runtime permission flag is an explicit synthetic DB prerequisite;
//! no production registry/schema has been enabled by this test fixture.
use alia::{
    adapters::{
        learned_capabilities::*, sandbox::DockerCommand, sandbox_skills::CandidateDraft,
        store::Store,
    },
    domain::{
        config::{PersonaSeed, RuntimeRules},
        history::ExpectedState,
    },
    runtime::{capability::IMPLEMENTED_CAPABILITIES, scheduling::RunLease},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
};
use uuid::Uuid;
struct Private {
    path: PathBuf,
}
impl Private {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("alia-p05-bridge-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self { path }
    }
    fn manager(&self) -> LearnedCapabilities {
        LearnedCapabilities::open(
            &self.path,
            DockerCommand::Native,
            TrustedSkillPolicy::unavailable(),
        )
        .unwrap()
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
async fn setup(pool: &PgPool, allow: bool) -> Store {
    sqlx::raw_sql(include_str!(
        "../work/p05-m4-production-bridge/SCHEMA_DRAFT.sql"
    ))
    .execute(pool)
    .await
    .unwrap();
    let s = Store::new(pool.clone());
    s.initialize(
        "bridge-init",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap(),
        &RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    if allow {
        sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{capabilities,enabled}',runtime_config->'capabilities'->'enabled' || '[\"sandbox.python\"]'::jsonb),config_version=config_version+1,state_version=state_version+1").execute(pool).await.unwrap();
    }
    s
}
async fn start(s: &Store) -> (RunLease, Uuid) {
    let i = s
        .submit_message(
            &Uuid::new_v4().to_string(),
            "actual synthetic source for metadata-only bridge",
            None,
        )
        .await
        .unwrap();
    (
        s.claim_run(Uuid::new_v4()).await.unwrap().unwrap(),
        i.event_id,
    )
}
fn candidate(owner: Uuid, event: Uuid, body: &str) -> SkillCommand {
    SkillCommand::Candidate(CandidateDraft {
        individual_id: owner,
        name: "private metadata candidate".into(),
        description: "not an executed or validated capability".into(),
        source: body.into(),
        source_event_ids: vec![event],
    })
}
async fn plan(pool: &PgPool, lease: &RunLease, command: &SkillCommand, extra: &[Uuid]) -> Uuid {
    let action = Uuid::new_v4();
    let decision = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    for (id, kind) in [(decision, "event"), (action, "action")] {
        sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,$3)")
            .bind(lease.individual_id)
            .bind(id)
            .bind(kind)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) VALUES($1,$2,'decision.accepted','generated','alia',clock_timestamp(),'{\"synthetic\":true}',$3)").bind(lease.individual_id).bind(decision).bind(lease.run_id).execute(&mut *tx).await.unwrap();
    let sequence: i32 =
        sqlx::query_scalar("SELECT coalesce(max(sequence),0)+1 FROM actions WHERE run_id=$1")
            .bind(lease.run_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    sqlx::query("INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(lease.individual_id).bind(action).bind(lease.run_id).bind(decision).bind(sequence).bind(CAPABILITY_ID).bind(BRIDGE_VERSION).bind(command.arguments().unwrap()).execute(&mut *tx).await.unwrap();
    let mut ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT input_manifest || ARRAY[individual_id,trigger_event_id] FROM runs WHERE id=$1",
    )
    .bind(lease.run_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    ids.extend(extra);
    ids.sort();
    ids.dedup();
    sqlx::query("UPDATE runs SET input_manifest=$2 WHERE id=$1")
        .bind(lease.run_id)
        .bind(ids)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    action
}
async fn counts(pool: &PgPool, run: Uuid) -> (i32, i64, i64) {
    (
        sqlx::query_scalar("SELECT tool_calls FROM runs WHERE id=$1")
            .bind(run)
            .fetch_one(pool)
            .await
            .unwrap(),
        sqlx::query_scalar("SELECT count(*) FROM learned_skills")
            .fetch_one(pool)
            .await
            .unwrap(),
        sqlx::query_scalar("SELECT count(*) FROM learned_skill_actions")
            .fetch_one(pool)
            .await
            .unwrap(),
    )
}
#[sqlx::test]
#[ignore = "P05 M4 actual owned PostgreSQL; no Docker/Python"]
async fn candidate_revision_revoke_use_actual_sdk_metadata_and_original_three_tool_slots(
    pool: PgPool,
) {
    let s = setup(&pool, true).await;
    let dir = Private::new();
    let m = dir.manager();
    m.initialize_authority(&s).await.unwrap();
    let (mut lease, event) = start(&s).await;
    let c = candidate(
        lease.individual_id,
        event,
        "def run(value):\n    return value\n",
    );
    let a = plan(&pool, &lease, &c, &[]).await;
    let first = m.execute(&s, &lease, a, c).await.unwrap();
    lease.state_version = first.action.state_version;
    assert_eq!(
        (
            first.sandbox_calls,
            first.attested_executions,
            first.charged_slots
        ),
        (0, 0, 1)
    );
    assert!(!first.action.executed);
    m.verify_restored(&s).await.unwrap();
    let draft = CandidateDraft {
        individual_id: lease.individual_id,
        name: "revision".into(),
        description: "private metadata only".into(),
        source: "def run(value):\n    return {\"value\": value}\n".into(),
        source_event_ids: vec![event],
    };
    let c = SkillCommand::Revise {
        reference: first.reference.clone(),
        draft,
    };
    let a = plan(&pool, &lease, &c, &[first.reference.skill_id]).await;
    let second = m.execute(&s, &lease, a, c).await.unwrap();
    lease.state_version = second.action.state_version;
    assert_eq!(second.reference.version, 2);
    assert_eq!(second.reference.skill_id, first.reference.skill_id);
    m.verify_restored(&s).await.unwrap();
    let c = SkillCommand::Revoke {
        reference: second.reference.clone(),
        reason: "owner requests revocation".into(),
    };
    let a = plan(&pool, &lease, &c, &[]).await;
    let third = m.execute(&s, &lease, a, c).await.unwrap();
    lease.state_version = third.action.state_version;
    assert_eq!(counts(&pool, lease.run_id).await, (3, 1, 3));
    m.verify_restored(&s).await.unwrap();
    let c = candidate(lease.individual_id, event, "def run(x):\n    return None\n");
    let a = plan(&pool, &lease, &c, &[]).await;
    assert!(matches!(
        m.execute(&s, &lease, a, c).await,
        Err(BridgeError::Budget)
    ));
    assert_eq!(counts(&pool, lease.run_id).await, (3, 1, 3));
    let versions: i64 = sqlx::query_scalar("SELECT count(*) FROM learned_skill_versions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(versions, 2);
}
#[sqlx::test]
#[ignore = "P05 M4 actual owned PostgreSQL; disabled permission/protocol"]
async fn permission_and_unreviewed_protocol_reject_before_reservation_or_execution(pool: PgPool) {
    let s = setup(&pool, false).await;
    let dir = Private::new();
    let m = dir.manager();
    m.initialize_authority(&s).await.unwrap();
    let (lease, event) = start(&s).await;
    let c = candidate(lease.individual_id, event, "def run(x):\n    return x\n");
    let a = plan(&pool, &lease, &c, &[]).await;
    assert!(matches!(
        m.execute(&s, &lease, a, c).await,
        Err(BridgeError::Denied)
    ));
    assert_eq!(counts(&pool, lease.run_id).await, (0, 0, 0));
}
#[sqlx::test]
#[ignore = "P05 M4 actual owned PostgreSQL; closed protocol is not executed"]
async fn candidate_is_not_validation_registration_or_permission_and_protocol_gate_is_closed(
    pool: PgPool,
) {
    let s = setup(&pool, true).await;
    let dir = Private::new();
    let m = dir.manager();
    m.initialize_authority(&s).await.unwrap();
    let (mut lease, event) = start(&s).await;
    let c = candidate(lease.individual_id, event, "def run(x):\n    return x\n");
    let a = plan(&pool, &lease, &c, &[]).await;
    let r = m.execute(&s, &lease, a, c).await.unwrap();
    lease.state_version = r.action.state_version;
    let cases = TrustedCases::new(vec![("identity".into(), "1".into(), "1".into())]).unwrap();
    let c = SkillCommand::Validate {
        reference: r.reference.clone(),
        cases,
    };
    let a = plan(&pool, &lease, &c, &[r.reference.skill_id]).await;
    let before = counts(&pool, lease.run_id).await;
    assert!(matches!(
        m.execute(&s, &lease, a, c).await,
        Err(BridgeError::Unreleased)
    ));
    assert_eq!(counts(&pool, lease.run_id).await, before);
    let state = s.snapshot().await.unwrap().unwrap();
    assert!(matches!(
        m.authorize_registration(
            &s,
            "grant-no-validation",
            ExpectedState {
                version: state.state_version,
                privacy_epoch: state.privacy_epoch
            },
            r.reference,
            Utc::now() + chrono::Duration::minutes(10)
        )
        .await,
        Err(BridgeError::Denied)
    ));
}
use chrono::Utc;
#[sqlx::test]
#[ignore = "P05 M4 actual owned PostgreSQL; real SDK files and immutable replay"]
async fn same_action_replays_once_but_changed_input_and_revoked_missing_edge_ancestor_refuse(
    pool: PgPool,
) {
    let s = setup(&pool, true).await;
    let dir = Private::new();
    let m = dir.manager();
    m.initialize_authority(&s).await.unwrap();
    let (lease, event) = start(&s).await;
    let c = candidate(lease.individual_id, event, "def run(x):\n    return x\n");
    let a = plan(&pool, &lease, &c, &[]).await;
    let r = m.execute(&s, &lease, a, c.clone()).await.unwrap();
    let before = counts(&pool, lease.run_id).await;
    let replay = m.execute(&s, &lease, a, c.clone()).await.unwrap();
    assert_eq!(json!(r), json!(replay));
    assert_eq!(counts(&pool, lease.run_id).await, before);
    let changed = candidate(lease.individual_id, event, "def run(x):\n    return None\n");
    assert!(matches!(
        m.execute(&s, &lease, a, changed).await,
        Err(BridgeError::Stale)
    ));
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM learned_skill_sources WHERE event_id=$1")
        .bind(event)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(event)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        m.execute(&s, &lease, a, c).await,
        Err(BridgeError::Stale)
    ));
    assert_eq!(counts(&pool, lease.run_id).await, before);
}
#[sqlx::test]
#[ignore = "P05 M4 actual owned PostgreSQL; real filesystem side effect plus late DB fault"]
async fn sdk_success_followed_by_database_failure_recovers_unknown_without_redispatch(
    pool: PgPool,
) {
    let s = setup(&pool, true).await;
    let dir = Private::new();
    let m = dir.manager();
    m.initialize_authority(&s).await.unwrap();
    let (lease, event) = start(&s).await;
    let c = candidate(lease.individual_id, event, "def run(x):\n    return x\n");
    let a = plan(&pool, &lease, &c, &[]).await;
    sqlx::raw_sql("CREATE FUNCTION bridge_fail_result() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind='tool.result' THEN RAISE EXCEPTION 'synthetic late bridge fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER bridge_fail_result BEFORE INSERT ON events FOR EACH ROW EXECUTE FUNCTION bridge_fail_result()").execute(&pool).await.unwrap();
    assert!(matches!(
        m.execute(&s, &lease, a, c.clone()).await,
        Err(BridgeError::Storage)
    ));
    sqlx::raw_sql("DROP TRIGGER bridge_fail_result ON events; DROP FUNCTION bridge_fail_result()")
        .execute(&pool)
        .await
        .unwrap();
    let capsule: Uuid = sqlx::query_scalar("SELECT capsule_id FROM learned_skills")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        fs::read_dir(dir.path.join(capsule.to_string()))
            .unwrap()
            .any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("skills-"))
    );
    assert_eq!(m.recover(&s).await.unwrap(), vec![a]);
    assert!(matches!(
        m.execute(&s, &lease, a, c).await,
        Err(BridgeError::Unknown)
    ));
    assert_eq!(counts(&pool, lease.run_id).await, (1, 1, 1));
    let ledger:(String,Option<i32>,i32,Option<Value>)=sqlx::query_as("SELECT status,actual_calls,charged_slots,receipt FROM learned_skill_actions WHERE action_id=$1").bind(a).fetch_one(&pool).await.unwrap();
    assert_eq!(ledger, ("unknown".into(), None, 1, None));
}
#[sqlx::test]
#[ignore = "P05 M4 actual owned PostgreSQL; capsule cleanup lease and external epoch"]
async fn invalidation_removes_whole_private_capsule_and_prevents_old_db_epoch_revival(
    pool: PgPool,
) {
    let s = setup(&pool, true).await;
    let dir = Private::new();
    let m = dir.manager();
    m.initialize_authority(&s).await.unwrap();
    let (lease, event) = start(&s).await;
    let c = candidate(lease.individual_id, event, "def run(x):\n    return x\n");
    let a = plan(&pool, &lease, &c, &[]).await;
    let _r = m.execute(&s, &lease, a, c).await.unwrap();
    let capsule: Uuid = sqlx::query_scalar("SELECT capsule_id FROM learned_skills")
        .fetch_one(&pool)
        .await
        .unwrap();
    let path = dir.path.join(capsule.to_string());
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path.join("active.lock"))
        .unwrap();
    held.lock().unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE individuals SET privacy_epoch=privacy_epoch+1")
        .execute(&mut *tx)
        .await
        .unwrap();
    m.invalidate_in(&mut tx, lease.individual_id, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        m.purge_invalidated(lease.individual_id, 1)
            .unwrap()
            .remaining,
        1
    );
    drop(held);
    let cleanup = m.purge_invalidated(lease.individual_id, 1).unwrap();
    assert_eq!((cleanup.removed, cleanup.remaining), (1, 0));
    assert!(!path.exists());
    let private:(Value,Option<Value>)=sqlx::query_as("SELECT a.arguments,l.outcome FROM actions a JOIN learned_skill_actions l ON l.action_id=a.id WHERE a.id=$1").bind(a).fetch_one(&pool).await.unwrap();
    assert_eq!(private, (json!({}), None));
    sqlx::query("UPDATE individuals SET privacy_epoch=0")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        m.verify_restored(&s).await,
        Err(BridgeError::Stale)
    ));
    assert!(matches!(
        m.initialize_authority(&s).await,
        Err(BridgeError::Stale)
    ));
}

#[sqlx::test]
#[ignore = "P05 M4 actual owned PG; public admission deadline while real writer gate held"]
async fn admission_deadline_never_launches_or_reserves_after_lock_wait(pool: PgPool) {
    let s = setup(&pool, true).await;
    let dir = Private::new();
    let m = dir.manager();
    m.initialize_authority(&s).await.unwrap();
    let (mut lease, event) = start(&s).await;
    let c = candidate(lease.individual_id, event, "def run(x):\n    return x\n");
    let a = plan(&pool, &lease, &c, &[]).await;
    let mut held = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
        .execute(&mut *held)
        .await
        .unwrap();
    lease.deadline = Utc::now() + chrono::Duration::milliseconds(150);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        m.execute(&s, &lease, a, c),
    )
    .await;
    held.rollback().await.unwrap();
    assert!(matches!(result, Ok(Err(BridgeError::Busy))));
    assert_eq!(counts(&pool, lease.run_id).await, (0, 0, 0));
}
#[sqlx::test]
#[ignore = "P05 M4 actual owned PG; registered booleans cannot replace actual SDK head"]
async fn restored_sql_phase_or_missing_sdk_vault_cannot_grant_execution(pool: PgPool) {
    let s = setup(&pool, true).await;
    let dir = Private::new();
    let m = dir.manager();
    m.initialize_authority(&s).await.unwrap();
    let (lease, event) = start(&s).await;
    let c = candidate(lease.individual_id, event, "def run(x):\n    return x\n");
    let a = plan(&pool, &lease, &c, &[]).await;
    let r = m.execute(&s, &lease, a, c).await.unwrap();
    sqlx::query("UPDATE learned_skill_versions SET phase='registered',report_sha256=repeat('a',64),case_set_sha256=repeat('b',64)").execute(&pool).await.unwrap();
    assert!(matches!(
        m.verify_restored(&s).await,
        Err(BridgeError::Stale)
    ));
    sqlx::query("UPDATE learned_skill_versions SET phase='candidate',report_sha256=NULL,case_set_sha256=NULL").execute(&pool).await.unwrap();
    m.verify_restored(&s).await.unwrap();
    let capsule: Uuid = sqlx::query_scalar("SELECT capsule_id FROM learned_skills WHERE id=$1")
        .bind(r.reference.skill_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let path = dir.path.join(capsule.to_string());
    let missing = dir.path.join(format!("saved-{}", Uuid::new_v4()));
    fs::rename(&path, &missing).unwrap();
    let result = m.verify_restored(&s).await;
    fs::rename(&missing, &path).unwrap();
    assert!(matches!(
        result,
        Err(BridgeError::Stale) | Err(BridgeError::UnsafeStorage)
    ));
}

#[sqlx::test]
#[ignore = "P05 M4 actual owned PG; owner intent timestamp identity, no protocol execution"]
async fn owner_grant_nanosecond_expiry_matches_its_actual_persisted_event_and_pg_value(
    pool: PgPool,
) {
    let s = setup(&pool, true).await;
    let dir = Private::new();
    let m = dir.manager();
    m.initialize_authority(&s).await.unwrap();
    let (lease, event) = start(&s).await;
    let c = candidate(lease.individual_id, event, "def run(x):\n    return x\n");
    let a = plan(&pool, &lease, &c, &[]).await;
    let r = m.execute(&s, &lease, a, c).await.unwrap();
    // Explicit structural fixture for the trusted owner-intent writer. It does
    // not supply an SDK ValidatedSkill or prove actual sandbox validation; the
    // SDK registration path still independently requires its opaque proof.
    sqlx::query("UPDATE learned_skill_versions SET phase='validated'")
        .execute(&pool)
        .await
        .unwrap();
    let current = s.snapshot().await.unwrap().unwrap();
    let expiry =
        chrono::DateTime::<Utc>::from_timestamp(Utc::now().timestamp() + 600, 123_456_999).unwrap();
    let grant = m
        .authorize_registration(
            &s,
            "nanosecond-intent",
            current.expected(),
            r.reference.clone(),
            expiry,
        )
        .await
        .unwrap();
    let command = SkillCommand::Register {
        reference: r.reference,
        grant,
    };
    let id: Uuid =
        serde_json::from_value(command.arguments().unwrap()["owner_grant_id"].clone()).unwrap();
    let matches:bool=sqlx::query_scalar("SELECT (e.payload->>'expires_at')::timestamptz=g.expires_at FROM learned_skill_grants g JOIN events e ON (e.individual_id,e.id)=(g.individual_id,g.id) WHERE g.id=$1").bind(id).fetch_one(&pool).await.unwrap();
    assert!(
        matches,
        "a real owner intent must not fail grant binding because JSON rounded nanoseconds differently from SQLx/PostgreSQL storage"
    );
}
