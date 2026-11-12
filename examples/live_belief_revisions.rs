//! Four synthetic-human-input stages; actual model/worker writes only.
//! No fixture inserts, manual belief records, clock edits or provider raw logs.
use alia::{
    adapters::{connect_database, model::ChatProvider, store::Store, web::HttpWebReader},
    domain::{
        config::{PersonaSeed, RuntimeRules},
        grounding::PACKET_VERSION,
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        model::{ModelProvider, ModelReadiness, ProviderMode},
        worker::{Worker, WorkerOutcome},
    },
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tokio::sync::Notify;
use uuid::Uuid;

#[path = "support/observed_provider.rs"]
mod observed_provider;
const CASES: &str = include_str!("../evals/p01/belief_writes_cases_v1.json");
const PHASES: [&str; 4] = ["form", "revise", "local", "restore-unknown"];

fn input_for(phase: &str) -> Result<(usize, String), &'static str> {
    let index = PHASES
        .iter()
        .position(|p| *p == phase)
        .ok_or("unknown_phase")?;
    let cases: Value = serde_json::from_str(CASES).map_err(|_| "invalid_frozen_cases")?;
    if cases["stages"][index]["id"] != phase || PACKET_VERSION != "target-citations/5" {
        return Err("case_or_packet_version_mismatch");
    }
    Ok((
        index,
        cases["stages"][index]["input"]
            .as_str()
            .ok_or("missing_fixed_input")?
            .into(),
    ))
}

fn safe_database(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    let name = url.path().trim_start_matches('/');
    url.scheme() == "postgres"
        && url.host_str() == Some("127.0.0.1")
        && url.username() == "aliaqa"
        && url.port().is_some()
        && url.query().is_none()
        && url.fragment().is_none()
        && name.strip_prefix("alia_live_test_").is_some_and(|s| {
            s.len() == 32
                && s.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
}

// Match the actual domain Event projection used by ModelContext/grounding.
// PostgreSQL to_jsonb adds idempotency_key and uses a different UTC spelling;
// neither difference is a missing or altered model source.
fn event_projection(rows: Vec<Value>) -> Result<Vec<Value>, &'static str> {
    rows.into_iter()
        .map(|row| {
            let event: alia::domain::history::Event =
                serde_json::from_value(row).map_err(|_| "event_projection_decode_failed")?;
            serde_json::to_value(event).map_err(|_| "event_projection_encode_failed")
        })
        .collect()
}

/// A separate, repeatable-read DB capture, never reconstructed from model output.
/// Only this dedicated synthetic DB's stances and their original source events.
async fn snapshot(pool: &PgPool) -> Result<Value, &'static str> {
    let mut tx = pool.begin().await.map_err(|_| "snapshot_begin_failed")?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|_| "snapshot_readonly_failed")?;
    let state: Value = sqlx::query_scalar("SELECT jsonb_build_object('individual_id',id,'state_version',state_version,'privacy_epoch',privacy_epoch,'config_version',config_version,'budget_frozen',budget_frozen,'limits',jsonb_build_object('budgets',runtime_config->'budgets','max_model_calls_per_cycle',runtime_config#>'{autonomy,max_model_calls_per_cycle}','max_tool_calls_per_cycle',runtime_config#>'{autonomy,max_tool_calls_per_cycle}')) FROM individuals WHERE singleton")
        .fetch_one(&mut *tx).await.map_err(|_| "snapshot_identity_failed")?;
    let counts = sqlx::query("SELECT (SELECT count(*) FROM beliefs) AS beliefs, (SELECT count(*) FROM events WHERE kind='message.received') AS inputs, (SELECT count(*) FROM objects WHERE blocked) AS blocked")
        .fetch_one(&mut *tx).await.map_err(|_| "snapshot_count_failed")?;
    let beliefs: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(b) || jsonb_build_object('support_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='support' ORDER BY event_id),'counter_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='counter' ORDER BY event_id)) FROM beliefs b ORDER BY created_at,id LIMIT 32")
        .fetch_all(&mut *tx).await.map_err(|_| "snapshot_beliefs_failed")?;
    let inputs: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(e) FROM events e WHERE kind='message.received' ORDER BY sequence LIMIT 5",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| "snapshot_inputs_failed")?;
    let source_count: i64 = sqlx::query_scalar("SELECT count(*) FROM events e WHERE kind='message.received' OR EXISTS(SELECT 1 FROM belief_evidence b WHERE (b.individual_id,b.event_id)=(e.individual_id,e.id))")
        .fetch_one(&mut *tx).await.map_err(|_| "snapshot_source_count_failed")?;
    let sources: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE kind='message.received' OR EXISTS(SELECT 1 FROM belief_evidence b WHERE (b.individual_id,b.event_id)=(e.individual_id,e.id)) ORDER BY sequence LIMIT 128")
        .fetch_all(&mut *tx).await.map_err(|_| "snapshot_sources_failed")?;
    let belief_event_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM events WHERE kind IN ('belief.formed','belief.revised')",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "snapshot_belief_event_count_failed")?;
    let belief_events: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE kind IN ('belief.formed','belief.revised') ORDER BY sequence LIMIT 32")
        .fetch_all(&mut *tx).await.map_err(|_| "snapshot_belief_events_failed")?;
    tx.commit().await.map_err(|_| "snapshot_end_failed")?;
    Ok(
        json!({"capture_id":Uuid::new_v4(),"observed_at":chrono::Utc::now(),
        "transaction":"repeatable_read_read_only","state":state,
        "belief_count":counts.get::<i64,_>("beliefs"),"input_count":counts.get::<i64,_>("inputs"),
        "blocked_object_count":counts.get::<i64,_>("blocked"),"source_count":source_count,
        "belief_event_count":belief_event_count,"beliefs":beliefs,"inputs":event_projection(inputs)?,
        "source_events":event_projection(sources)?,"belief_events":event_projection(belief_events)?,
        "coverage":"dedicated_synthetic_db_case_records; limits explicit, no complete-history claim"}),
    )
}

async fn execute(report: &mut Value) -> Result<(), &'static str> {
    let phase = report["phase"].as_str().ok_or("missing_phase")?.to_owned();
    let (index, input) = input_for(&phase)?;
    let url = std::env::var("DATABASE_URL").map_err(|_| "missing_test_database")?;
    if !safe_database(&url) {
        return Err("refusing_non_owned_test_database");
    }
    let tokenizer =
        std::env::var("ALIA_TOKENIZER_PATH").map_err(|_| "missing_explicit_tokenizer")?;
    if !std::path::Path::new(&tokenizer).is_absolute() {
        return Err("tokenizer_must_be_absolute");
    }
    let pool = connect_database(&url)
        .await
        .map_err(|_| "test_database_unavailable")?;
    let store = Store::new(pool.clone());
    store.migrate().await.map_err(|_| "migration_failed")?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM individuals")
        .fetch_one(&pool)
        .await
        .map_err(|_| "identity_count_failed")?;
    if index == 0 {
        if count != 0 {
            return Err("initial_database_not_empty");
        }
        let persona = PersonaSeed::parse(include_bytes!("../config/persona.example.json"))
            .map_err(|_| "persona_invalid")?;
        let rules = RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .map_err(|_| "rules_invalid")?;
        store
            .initialize("belief-live-initialize", &persona, &rules)
            .await
            .map_err(|_| "initialization_failed")?;
    } else if count != 1 {
        return Err("restored_identity_missing");
    }
    report["before"] = snapshot(&pool).await?;
    if report["before"]["input_count"] != index as u64
        || report["before"]["blocked_object_count"] != 0
    {
        return Err("phase_order_or_privacy_state_invalid");
    }
    let limits = &report["before"]["state"]["limits"];
    if limits["budgets"]["total_tokens_per_day"] != 200000
        || limits["budgets"]["background_tokens_per_day"] != 50000
        || limits["max_model_calls_per_cycle"] != 3
        || limits["max_tool_calls_per_cycle"] != 3
    {
        return Err("original_limits_changed");
    }
    let provider = ChatProvider::from_env().map_err(|_| "provider_unavailable")?;
    if provider.mode() != ProviderMode::Live
        || !matches!(provider.status().readiness, ModelReadiness::Ready)
    {
        return Err("real_provider_not_ready");
    }
    report["input_accounting"] = json!(provider.input_accounting());
    let conversation = Uuid::new_v4();
    let submitted = store
        .submit_message(&format!("belief-live-{phase}"), &input, Some(conversation))
        .await
        .map_err(|_| "input_submit_failed")?;
    report["submitted"] =
        serde_json::to_value(&submitted).map_err(|_| "submitted_projection_failed")?;
    report["conversation_id"] = json!(conversation);
    let provider =
        Arc::new(observed_provider::ObservedProvider::with_belief_observations(provider));
    let worker = Worker::new(
        store,
        provider.clone(),
        Arc::new(HttpWebReader),
        Arc::new(Notify::new()),
    );
    let outcome = worker.run_once().await;
    report["worker_outcome"] = match &outcome {
        Ok(value) => json!(format!("{value:?}")),
        Err(_) => json!("storage_failure"),
    };
    report["worker_committed"] =
        json!(matches!(outcome, Ok(WorkerOutcome::Committed(id)) if id==submitted.operation_id));
    report["provider_observations"] = json!(provider.belief_observations());
    report["after"] = snapshot(&pool).await?;
    let commits: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(c) FROM decision_commits c WHERE run_id=$1 ORDER BY created_at,reservation_id")
        .bind(submitted.operation_id).fetch_all(&pool).await.map_err(|_| "commit_capture_failed")?;
    let reviews: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(r) FROM decision_reviews r WHERE run_id=$1 ORDER BY created_at,author_reservation_id")
        .bind(submitted.operation_id).fetch_all(&pool).await.map_err(|_| "review_capture_failed")?;
    let replies: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE kind='message.sent' AND correlation_id=$1 ORDER BY sequence")
        .bind(submitted.operation_id).fetch_all(&pool).await.map_err(|_| "reply_capture_failed")?;
    let decisions: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE kind='decision.accepted' AND correlation_id=$1 ORDER BY sequence")
        .bind(submitted.operation_id).fetch_all(&pool).await.map_err(|_| "decision_capture_failed")?;
    let run: Value = sqlx::query_scalar("SELECT jsonb_build_object('individual_id',individual_id,'id',id,'trigger_event_id',trigger_event_id,'state_version',state_version,'privacy_epoch',privacy_epoch,'config_version',config_version,'status',status,'mode',mode,'model_calls',model_calls,'tool_calls',tool_calls,'error_code',error_code,'created_at',created_at,'finished_at',finished_at) FROM runs WHERE id=$1")
        .bind(submitted.operation_id).fetch_one(&pool).await.map_err(|_| "run_capture_failed")?;
    let diagnostics: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'kind',kind,'payload',payload) FROM events WHERE correlation_id=$1 AND kind IN ('run.retry_scheduled','run.stopped','decision.review_retry_scheduled') ORDER BY sequence")
        .bind(submitted.operation_id).fetch_all(&pool).await.map_err(|_| "diagnostic_capture_failed")?;
    let usage_count: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .map_err(|_| "usage_count_failed")?;
    let usage: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'individual_id',individual_id,'run_id',run_id,'window_id',window_id,'mode',mode,'status',status,'input_bound',input_bound,'output_cap',output_cap,'actual_input',actual_input,'actual_output',actual_output,'actual_total',actual_total,'charged_tokens',charged_tokens,'usage_unknown',usage_unknown,'dispatched_at',dispatched_at,'request_digest',request_digest) FROM usage_reservations ORDER BY created_at,id LIMIT 32")
        .fetch_all(&pool).await.map_err(|_| "usage_capture_failed")?;
    report["commits"] = json!(commits);
    report["reviews"] = json!(reviews);
    report["replies"] = json!(event_projection(replies)?);
    report["run"] = run;
    report["diagnostics"] = json!(diagnostics);
    report["decision_events"] = json!(event_projection(decisions)?);
    report["ledger_snapshot"] = json!({"reservation_count":usage_count,"records":usage,"cumulative_do_not_sum_across_stages":true});
    pool.close().await;
    if report["worker_committed"] != true {
        return Err("cycle_not_committed");
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut report = json!({"format":"alia-p01-belief-stage/1","data_mode":"synthetic_inputs_real_provider",
        "phase":std::env::var("ALIA_BELIEF_PHASE").unwrap_or_default(),"packet_version":PACKET_VERSION,
        "cases_sha256":format!("{:x}",Sha256::digest(CASES.as_bytes())),
        "process_id":std::process::id(),"process_nonce":Uuid::new_v4(),"started_at":chrono::Utc::now(),
        "worker_committed":false,"failure":null,"semantic_verdict":"unverified","live_model_pass":null});
    let result = execute(&mut report).await;
    if let Err(code) = result {
        report["failure"] = json!(code);
    }
    report["finished_at"] = json!(chrono::Utc::now());
    println!("{report}");
    if result.is_err() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_fixed_inputs_are_distinct_and_no_phase_fallback_exists() {
        let mut inputs = std::collections::HashSet::new();
        for (i, phase) in PHASES.iter().enumerate() {
            let (index, text) = input_for(phase).unwrap();
            assert_eq!(i, index);
            assert!(inputs.insert(text));
        }
        assert!(input_for("unknown").is_err());
        let cases: Value = serde_json::from_str(CASES).unwrap();
        let policy = json!([
            alia::domain::grounding::RULES,
            alia::domain::grounding::SCHEMA,
            PACKET_VERSION
        ]);
        assert_eq!(
            cases["baseline"]["review_checker_version"],
            format!(
                "decision_grounding/1:{:x}",
                Sha256::digest(serde_json::to_vec(&policy).unwrap())
            )
        );
    }
    #[test]
    fn disposable_database_guard_rejects_production_and_ambiguous_urls() {
        let good = "postgres://aliaqa:synthetic@127.0.0.1:5432/alia_live_test_00000000000000000000000000000001";
        assert!(safe_database(good));
        for bad in [
            good.replace("127.0.0.1", "localhost"),
            good.replace("aliaqa:", "alia:"),
            good.replace("/alia_live_test_", "/production_"),
            format!("{good}?sslmode=disable"),
            good.replace("00000000000000000000000000000001", "a"),
            good.replace("postgres://", "https://"),
        ] {
            assert!(!safe_database(&bad));
        }
    }
    #[test]
    fn database_events_match_typed_model_sources_without_internal_extra_fields() {
        let value = json!({"id":Uuid::from_u128(1),"individual_id":Uuid::from_u128(2),
            "sequence":1,"kind":"message.received","source_kind":"reported_claim","actor_id":"owner",
            "occurred_at":"2026-11-12T12:00:00+00:00","recorded_at":"2026-11-12T12:00:00+00:00",
            "payload":{"text":"Actual synthetic input"},"correlation_id":Uuid::from_u128(3),
            "causation_id":null,"idempotency_key":"internal-key-not-model-source"});
        let projected = event_projection(vec![value]).unwrap().pop().unwrap();
        assert_eq!(projected["occurred_at"], "2026-11-12T12:00:00Z");
        assert!(projected.get("idempotency_key").is_none());
        assert_eq!(projected["payload"]["text"], "Actual synthetic input");
        assert!(event_projection(vec![json!({"id":"missing-other-fields"})]).is_err());
    }
}
