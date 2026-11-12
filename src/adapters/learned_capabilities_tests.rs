//! Only the previously permitted normal run(input)->JSON path. No protocol
//! negative/adversarial probe is implemented or dispatched from this module.
use super::*;
use crate::{
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::{capability::IMPLEMENTED_CAPABILITIES, scheduling::RunOutcome},
};
use sqlx::PgPool;
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
async fn plan(pool: &PgPool, s: &Store, command: &SkillCommand, refs: &[Uuid]) -> (RunLease, Uuid) {
    let input = s
        .submit_message(
            &Uuid::new_v4().to_string(),
            "synthetic trusted normal-path SDK bridge",
            None,
        )
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
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
    sqlx::query("INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments) VALUES($1,$2,$3,$4,1,$5,$6,$7)").bind(lease.individual_id).bind(action).bind(lease.run_id).bind(decision).bind(CAPABILITY_ID).bind(BRIDGE_VERSION).bind(command.arguments().unwrap()).execute(&mut *tx).await.unwrap();
    let mut inputs = vec![lease.individual_id, input.event_id];
    inputs.extend(refs);
    inputs.sort();
    inputs.dedup();
    sqlx::query("UPDATE runs SET input_manifest=$2 WHERE id=$1")
        .bind(lease.run_id)
        .bind(inputs)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (lease, action)
}
async fn finish(s: &Store, mut lease: RunLease, receipt: &BridgeReceipt) {
    lease.state_version = receipt.action.state_version;
    s.finish_run(&lease, RunOutcome::Failed).await.unwrap();
}
#[sqlx::test]
#[ignore = "P05 M4 allowed normal path; actual owned PG and shared sandbox lock required"]
async fn m4_allowed_normal_sdk_validation_registration_invocation_revocation_recovery(
    pool: PgPool,
) {
    sqlx::raw_sql(include_str!(
        "../../work/p05-m4-production-bridge/SCHEMA_DRAFT.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let s = Store::new(pool.clone());
    s.initialize(
        "allowed-positive",
        &PersonaSeed::parse(include_bytes!("../../config/persona.example.json")).unwrap(),
        &RuntimeRules::parse(
            include_bytes!("../../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE individuals SET runtime_config=jsonb_set(runtime_config,'{capabilities,enabled}',runtime_config->'capabilities'->'enabled' || '[\"sandbox.python\"]'::jsonb),state_version=state_version+1,config_version=config_version+1").execute(&pool).await.unwrap();
    let root = std::env::temp_dir().join(format!("alia-p05-allowed-{}", Uuid::new_v4()));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let _temp = Temp(root.clone());
    // Private fields are accessible only to this cfg(test) child. There is no
    // production method/Deserialize or HTTP/model flag that enables execution.
    let manager = LearnedCapabilities::open(
        &root,
        DockerCommand::SudoNoPassword,
        TrustedSkillPolicy {
            execution_release: true,
        },
    )
    .unwrap();
    manager.initialize_authority(&s).await.unwrap();
    let owner = s.snapshot().await.unwrap().unwrap().id;
    let imported = s
        .import_document(
            "normal-source",
            "identity function",
            "A permitted static normal input/output case.",
            "synthetic engineering fixture",
        )
        .await
        .unwrap()
        .event_id;
    let command = SkillCommand::Candidate(CandidateDraft {
        individual_id: owner,
        name: "normal identity".into(),
        description: "explicit engineering fixture, not product protocol approval".into(),
        source: "def run(value):\n    return value\n".into(),
        source_event_ids: vec![imported],
    });
    let (lease, action) = plan(&pool, &s, &command, &[imported]).await;
    let candidate = manager.execute(&s, &lease, action, command).await.unwrap();
    finish(&s, lease, &candidate).await;
    let reference = candidate.reference;
    let cases = TrustedCases::new(vec![
        ("integer".into(), "1".into(), "1".into()),
        (
            "object".into(),
            "{\"ok\":true}".into(),
            "{\"ok\":true}".into(),
        ),
        ("array".into(), "[1,2]".into(), "[1,2]".into()),
    ])
    .unwrap();
    let command = SkillCommand::Validate {
        reference: reference.clone(),
        cases,
    };
    let (lease, action) = plan(&pool, &s, &command, &[reference.skill_id]).await;
    let validated = manager.execute(&s, &lease, action, command).await.unwrap();
    assert_eq!(
        (
            validated.sandbox_calls,
            validated.attested_executions,
            validated.reserved_slots,
            validated.charged_slots
        ),
        (3, 3, 3, 3)
    );
    assert_eq!(validated.action.status, "succeeded");
    finish(&s, lease, &validated).await;
    let current = s.snapshot().await.unwrap().unwrap();
    let grant = manager
        .authorize_registration(
            &s,
            "actual-owner-grant",
            current.expected(),
            reference.clone(),
            Utc::now() + chrono::Duration::minutes(10),
        )
        .await
        .unwrap();
    let command = SkillCommand::Register {
        reference: reference.clone(),
        grant,
    };
    let (mut lease, action) = plan(&pool, &s, &command, &[reference.skill_id]).await;
    let registered = manager.execute(&s, &lease, action, command).await.unwrap();
    lease.state_version = registered.action.state_version;
    assert_eq!(registered.sandbox_calls, 0);
    let restored = LearnedCapabilities::open(
        &root,
        DockerCommand::SudoNoPassword,
        TrustedSkillPolicy::unavailable(),
    )
    .unwrap();
    restored.verify_restored(&s).await.unwrap();
    let command = SkillCommand::Invoke {
        reference: reference.clone(),
        input_json: "{\"value\":42}".into(),
        timeout_ms: 10000,
    };
    // A second ordinary planned action in the same actual run uses the same
    // trusted fixture mechanism but preserves action ordering and original quota.
    let invoke = Uuid::new_v4();
    let decision: Uuid = sqlx::query_scalar("SELECT decision_event_id FROM actions WHERE id=$1")
        .bind(action)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'action')")
        .bind(owner)
        .bind(invoke)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments) VALUES($1,$2,$3,$4,2,$5,$6,$7)").bind(owner).bind(invoke).bind(lease.run_id).bind(decision).bind(CAPABILITY_ID).bind(BRIDGE_VERSION).bind(command.arguments().unwrap()).execute(&pool).await.unwrap();
    let invoked = manager
        .execute(&s, &lease, invoke, command.clone())
        .await
        .unwrap();
    lease.state_version = invoked.action.state_version;
    assert_eq!(
        (
            invoked.sandbox_calls,
            invoked.attested_executions,
            invoked.charged_slots
        ),
        (1, 1, 1)
    );
    assert_eq!(invoked.action.status, "succeeded");
    let replay = manager.execute(&s, &lease, invoke, command).await.unwrap();
    assert_eq!(replay.action, invoked.action);
    let result: Value =
        sqlx::query_scalar("SELECT outcome FROM learned_skill_actions WHERE action_id=$1")
            .bind(invoke)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(result["output"]["Ok"]["result"], json!({"value":42}));
    let command = SkillCommand::Revoke {
        reference: reference.clone(),
        reason: "trusted owner ends registration".into(),
    };
    let revoke = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,'action')")
        .bind(owner)
        .bind(revoke)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments) VALUES($1,$2,$3,$4,3,$5,$6,$7)").bind(owner).bind(revoke).bind(lease.run_id).bind(decision).bind(CAPABILITY_ID).bind(BRIDGE_VERSION).bind(command.arguments().unwrap()).execute(&pool).await.unwrap();
    let revoked = manager.execute(&s, &lease, revoke, command).await.unwrap();
    assert_eq!(revoked.action.status, "succeeded");
    assert_eq!(revoked.sandbox_calls, 0);
    restored.verify_restored(&s).await.unwrap();
    let calls: i32 = sqlx::query_scalar("SELECT tool_calls FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(calls, 3);
    let grants: bool = sqlx::query_scalar("SELECT bool_and(revoked) FROM learned_skill_grants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(grants);
    let totals:(i64,i64,i64)=sqlx::query_as("SELECT sum(actual_calls)::bigint,sum(attested_calls)::bigint,sum(charged_slots)::bigint FROM learned_skill_actions").fetch_one(&pool).await.unwrap();
    assert_eq!(totals, (4, 4, 7));
    println!(
        "P05_M4_SDK_EXECUTION_COUNTS={}",
        json!({"sandbox_calls":totals.0,"attested_executions":totals.1,"charged_tool_slots":totals.2})
    );
}
