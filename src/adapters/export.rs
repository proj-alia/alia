//! In-memory personal portability. No files, transport, imports or model calls.
use super::{
    api::existing,
    store::{Store, StoreError, digest},
};
use crate::domain::export::{
    EXPORT_FORMAT, EXPORT_MAX_BYTES, EXPORT_MAX_TABLE_ROWS, ExportError, ExportOmission,
    IndividualExport, json_size,
};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;
const SCHEMA_FINGERPRINT: &str = "e0642b8f8ecb874ccc8b3b2497383d84338785d1955de3a06928a76491870848";
const SCHEMA30_FINGERPRINT: &str =
    "eb00e02e410278d5ca86d0efed7ea3364dba839f48a01615f68ad6f5129e5976";
// Shared actual provenance graph: export avoids the preview10000-row cap.
async fn visibility(tx: &mut Transaction<'_, Postgres>) -> Result<String, StoreError> {
    let graph = super::deletion::reference_graph_sql(tx).await?;
    Ok(format!(
        r#"WITH RECURSIVE refs(source,target) AS ({graph}), owned_refs(source,target) AS (
 SELECT r.source,r.target FROM refs r JOIN objects s ON s.individual_id=$1 AND s.id=r.source
 JOIN objects t ON t.individual_id=$1 AND t.id=r.target),
 hidden(id) AS (SELECT id FROM objects WHERE individual_id=$1 AND blocked
 UNION SELECT unnest(object_ids) FROM privacy_operations WHERE individual_id=$1
 UNION SELECT unnest(root_event_ids) FROM privacy_operations WHERE individual_id=$1
 UNION SELECT id FROM development_targets WHERE individual_id=$1 AND provenance_removed
 UNION SELECT id FROM developments WHERE individual_id=$1 AND provenance_removed
 UNION SELECT id FROM development_revisions WHERE individual_id=$1 AND provenance_removed
 UNION SELECT r.target FROM owned_refs r JOIN hidden h ON r.source=h.id)
"#
    ))
}
struct Table {
    name: &'static str,
    keys: &'static [&'static str],
    text_keys: &'static [&'static str],
    deps: &'static [&'static str],
    omit: &'static [&'static str],
    json: &'static [&'static str],
    skip: Option<&'static str>,
}
const TABLES: &[Table] = &[
    Table {
        name: "development_measurement_programs",
        keys: &["individual_id", "verifier_id"],
        text_keys: &[],
        deps: &["verifier_id", "registration_event_id"],
        omit: &[],
        json: &["policy"],
        skip: None,
    },
    Table {
        name: "development_trial_plans",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &[
            "id",
            "development_id",
            "trial_event_id",
            "evaluator_id",
            "review_event_id",
        ],
        omit: &[],
        json: &["plan"],
        skip: None,
    },
    Table {
        name: "development_trial_cases",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "plan_id", "evaluation_run_id", "observation_event_id"],
        omit: &["lease_generation"],
        json: &["case_data"],
        skip: None,
    },
    Table {
        name: "development_trial_attempts",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &[
            "id",
            "plan_id",
            "case_id",
            "evaluation_run_id",
            "result_event_id",
        ],
        omit: &["lease_generation", "request_digest"],
        json: &["context_json", "result"],
        skip: None,
    },
    Table {
        name: "development_trial_case_sources",
        keys: &["individual_id", "case_id", "event_id"],
        text_keys: &[],
        deps: &["case_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "development_targets",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "scope_id", "current_revision_id"],
        omit: &[],
        json: &["value"],
        skip: None,
    },
    Table {
        name: "developments",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &[
            "id",
            "target_id",
            "proposer_id",
            "before_head",
            "created_event_id",
        ],
        omit: &[],
        json: &["draft", "before_value"],
        skip: None,
    },
    Table {
        name: "development_evidence",
        keys: &["individual_id", "development_id", "event_id"],
        text_keys: &[],
        deps: &["development_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "development_steps",
        keys: &["individual_id", "development_id", "version"],
        text_keys: &[],
        deps: &["development_id", "event_id", "revision_id"],
        omit: &[],
        json: &["command", "transition_record", "validation_record"],
        skip: None,
    },
    Table {
        name: "development_revisions",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &[
            "id",
            "target_id",
            "development_id",
            "event_id",
            "previous_revision_id",
        ],
        omit: &[],
        json: &["before_value", "after_value"],
        skip: None,
    },
    Table {
        name: "development_dependencies",
        keys: &["individual_id", "object_id", "source_id"],
        text_keys: &[],
        deps: &["object_id", "source_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "development_verifiers",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "registration_event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "development_observations",
        keys: &["individual_id", "event_id"],
        text_keys: &[],
        deps: &["event_id", "verifier_id", "trial_event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "development_review_records",
        keys: &["individual_id", "event_id"],
        text_keys: &[],
        deps: &[
            "event_id",
            "development_id",
            "trial_event_id",
            "verifier_id",
        ],
        omit: &[],
        json: &["assessment"],
        skip: None,
    },
    Table {
        name: "development_review_sources",
        keys: &["individual_id", "review_event_id", "event_id"],
        text_keys: &[],
        deps: &["review_event_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "actions",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "run_id", "decision_event_id", "result_event_id"],
        omit: &["lease_generation"],
        json: &["arguments", "receipt"],
        skip: None,
    },
    Table {
        name: "affective_appraisals",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "goal_id", "subject_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "affective_attitudes",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "subject_id", "supersedes"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "affective_sources",
        keys: &["individual_id", "object_id", "event_id"],
        text_keys: &[],
        deps: &["object_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "autobiographies",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "supersedes"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "autobiography_beliefs",
        keys: &["individual_id", "autobiography_id", "belief_id"],
        text_keys: &[],
        deps: &["autobiography_id", "belief_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "autobiography_events",
        keys: &["individual_id", "autobiography_id", "event_id"],
        text_keys: &[],
        deps: &["autobiography_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "belief_evidence",
        keys: &["individual_id", "belief_id", "event_id", "role"],
        text_keys: &["role"],
        deps: &["belief_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "beliefs",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "supersedes"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "budget_windows",
        keys: &["id"],
        text_keys: &[],
        deps: &[],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "contact_windows",
        keys: &["id"],
        text_keys: &[],
        deps: &[],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "decision_commits",
        keys: &["reservation_id"],
        text_keys: &[],
        deps: &["run_id", "decision_event_id"],
        omit: &["lease_generation", "proposal_digest"],
        json: &["receipt"],
        skip: None,
    },
    Table {
        name: "decision_reviews",
        keys: &["author_reservation_id"],
        text_keys: &[],
        deps: &["run_id", "event_id"],
        omit: &[
            "lease_generation",
            "draft_digest",
            "context_digest",
            "review_context_digest",
        ],
        json: &["assessment"],
        skip: None,
    },
    Table {
        name: "derivation_edges",
        keys: &["individual_id", "source_id", "target_id"],
        text_keys: &[],
        deps: &["source_id", "target_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "discovery_occurrences",
        keys: &["individual_id", "source_event_id"],
        text_keys: &[],
        deps: &["source_event_id", "run_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "documents",
        keys: &["individual_id", "event_id"],
        text_keys: &[],
        deps: &["event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "event_stream_watermarks",
        keys: &["individual_id"],
        text_keys: &[],
        deps: &[],
        omit: &[],
        json: &[],
        skip: Some(
            "Subscriber cursor and retention bookkeeping is operational, not personal history.",
        ),
    },
    Table {
        name: "events",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "causation_id"],
        omit: &["idempotency_key"],
        json: &["payload"],
        skip: None,
    },
    Table {
        name: "goal_cycles",
        keys: &["individual_id", "run_id"],
        text_keys: &[],
        deps: &["run_id", "goal_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "goal_progress",
        keys: &["individual_id", "goal_id", "content_digest"],
        text_keys: &["content_digest"],
        deps: &["goal_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "goal_reviews",
        keys: &["individual_id", "event_id"],
        text_keys: &[],
        deps: &["goal_id", "event_id"],
        omit: &["input_digest"],
        json: &["assessment"],
        skip: None,
    },
    Table {
        name: "goals",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "formed_in_run"],
        omit: &[],
        json: &["origin_ids", "completion"],
        skip: None,
    },
    Table {
        name: "idempotency_records",
        keys: &["actor_id", "route", "key"],
        text_keys: &["actor_id", "route", "key"],
        deps: &[],
        omit: &[],
        json: &["response"],
        skip: Some(
            "Command replay keys, request hashes and duplicated replay responses are operational, not portable personal history.",
        ),
    },
    Table {
        name: "individuals",
        keys: &["id"],
        text_keys: &[],
        deps: &["id"],
        omit: &["lease_generation"],
        json: &["origin", "seed", "persona", "runtime_config"],
        skip: None,
    },
    Table {
        name: "interest_revisions",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "previous_revision_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "interest_sources",
        keys: &["individual_id", "interest_id", "event_id"],
        text_keys: &["interest_id"],
        deps: &["event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "interests",
        keys: &["individual_id", "id"],
        text_keys: &["id"],
        deps: &["object_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "memories",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "supersedes"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "memory_index_jobs",
        keys: &["individual_id", "memory_id", "profile"],
        text_keys: &["profile"],
        deps: &["memory_id"],
        omit: &[
            "lease_owner",
            "lease_expires_at",
            "generation",
            "error_code",
        ],
        json: &[],
        skip: None,
    },
    Table {
        name: "memory_person_links",
        keys: &["individual_id", "memory_id", "entity_id"],
        text_keys: &[],
        deps: &[
            "memory_id",
            "entity_id",
            "source_event_id",
            "assertion_event_id",
        ],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "memory_sources",
        keys: &["individual_id", "memory_id", "event_id"],
        text_keys: &[],
        deps: &["memory_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "memory_vector_sets",
        keys: &["individual_id", "memory_id", "profile"],
        text_keys: &["profile"],
        deps: &["memory_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "memory_vectors",
        keys: &["individual_id", "memory_id", "profile", "ordinal"],
        text_keys: &["profile"],
        deps: &["memory_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "notes",
        keys: &["individual_id", "action_id"],
        text_keys: &[],
        deps: &["action_id"],
        omit: &[],
        json: &["citations"],
        skip: None,
    },
    Table {
        name: "objects",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "outbox",
        keys: &["sequence"],
        text_keys: &[],
        deps: &[],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "outgoing_messages",
        keys: &["individual_id", "draft_event_id"],
        text_keys: &[],
        deps: &["draft_event_id", "run_id", "delivered_event_id"],
        omit: &["cancellation_reason"],
        json: &[],
        skip: None,
    },
    Table {
        name: "person_aliases",
        keys: &["individual_id", "entity_id", "surface"],
        text_keys: &["surface"],
        deps: &["entity_id", "source_event_id", "assertion_event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "person_entities",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "privacy_operations",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &[],
        omit: &[
            "target_id",
            "expected_memory_version",
            "idempotency_key_digest",
            "request_digest",
            "journal_sequence",
            "journal_digest",
            "root_event_ids",
            "object_ids",
            "last_error",
        ],
        json: &[],
        skip: None,
    },
    Table {
        name: "relationship_events",
        keys: &["individual_id", "subject_id", "event_id"],
        text_keys: &[],
        deps: &["subject_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "relationship_revision_events",
        keys: &["individual_id", "revision_id", "event_id"],
        text_keys: &[],
        deps: &["revision_id", "event_id"],
        omit: &[],
        json: &[],
        skip: None,
    },
    Table {
        name: "relationship_revisions",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "subject_id", "previous_revision_id"],
        omit: &[],
        json: &["before_state", "after_state"],
        skip: None,
    },
    Table {
        name: "relationships",
        keys: &["individual_id", "subject_id"],
        text_keys: &[],
        deps: &["subject_id"],
        omit: &[],
        json: &["expectations", "boundaries", "commitments", "unresolved"],
        skip: None,
    },
    Table {
        name: "runs",
        keys: &["individual_id", "id"],
        text_keys: &[],
        deps: &["id", "trigger_event_id", "goal_id"],
        omit: &[
            "lease_owner",
            "lease_generation",
            "lease_expires_at",
            "retry_after",
            "error_code",
        ],
        json: &[],
        skip: None,
    },
    Table {
        name: "usage_reservations",
        keys: &["id"],
        text_keys: &[],
        deps: &[],
        omit: &["lease_generation", "request_key", "request_digest"],
        json: &[],
        skip: None,
    },
];
// Each accepted fingerprint fixes the complete column/constraint inventory.
// Old schema29 exports retain their exact table set and manifest fingerprint.
fn tables(trials: bool) -> impl Iterator<Item = &'static Table> {
    TABLES.iter().filter(move |t| {
        trials
            || !matches!(
                t.name,
                "development_measurement_programs"
                    | "development_trial_plans"
                    | "development_trial_cases"
                    | "development_trial_attempts"
                    | "development_trial_case_sources"
            )
    })
}
const KNOWN_KINDS: &[&str] = &[
    "identity",
    "event",
    "memory",
    "goal",
    "run",
    "relationship",
    "action",
    "entity",
    "belief",
    "autobiography",
    "affective_state",
    "affective_attitude",
    "relationship_revision",
    "interest",
    "interest_revision",
    "development_target",
    "development",
    "development_revision",
    "development_verifier",
];
fn export_error(error: ExportError) -> StoreError {
    match error {
        ExportError::TooLarge => StoreError::Invalid("personal export exceeds 64 MiB"),
        ExportError::TooManyRows => {
            StoreError::Invalid("personal export table exceeds 100000 rows")
        }
        ExportError::Invalid => StoreError::Invalid("invalid personal export JSON"),
    }
}
fn sensitive_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().replace('-', "_").as_str(),
        "token"
            | "access_token"
            | "refresh_token"
            | "api_key"
            | "access_key"
            | "secret_key"
            | "credential"
            | "credentials"
            | "password"
            | "secret"
            | "authorization"
            | "headers"
            | "request_headers"
            | "response_headers"
            | "endpoint"
            | "provider_endpoint"
            | "model_endpoint"
            | "deployment_address"
            | "deployment_url"
            | "base_url"
            | "api_base"
            | "database_url"
            | "connection_string"
            | "proxy"
            | "http_proxy"
            | "https_proxy"
            | "provider_error"
            | "raw_error"
            | "error"
            | "error_code"
            | "last_error"
            | "diagnostic"
            | "diagnostics"
            | "provider_request"
            | "provider_response"
            | "idempotency_key"
            | "idempotency_key_digest"
            | "request_key"
            | "request_id"
            | "request_digest"
            | "key_digest"
            | "journal_digest"
            | "journal_sequence"
            | "lease_owner"
    )
}
fn scrub(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|key, _| !sensitive_key(key));
            for child in object.values_mut() {
                scrub(child);
            }
        }
        Value::Array(items) => {
            for child in items {
                scrub(child);
            }
        }
        _ => {}
    }
}
fn omissions(trials: bool) -> Vec<ExportOmission> {
    let mut result = vec![];
    for table in tables(trials) {
        if let Some(reason) = table.skip {
            result.push(ExportOmission {
                table: table.name.into(),
                columns: vec!["*".into()],
                reason: reason.into(),
            });
            continue;
        }
        result.push(ExportOmission{table:table.name.into(),columns:vec!["*".into()],reason:"Only actual owned rows with complete available dependencies are included; unavailable rows and links are omitted without counts or IDs.".into()});
        if !table.omit.is_empty() {
            result.push(ExportOmission{table:table.name.into(),columns:table.omit.iter().map(|s|(*s).into()).collect(),reason:"Raw command/request/journal associations, lock leases or provider diagnostics are operational and excluded.".into()});
        }
        if !table.json.is_empty() {
            result.push(ExportOmission{table:table.name.into(),columns:table.json.iter().map(|s|(*s).into()).collect(),reason:"Structured credential/token/authorization/header/endpoint/proxy/raw-error/request-key/journal-key fields are recursively removed; personal text strings and external-document provenance remain verbatim, with no semantic password inference.".into()});
        }
    }
    result.push(ExportOmission{table:"events".into(),columns:vec!["payload".into()],reason:"Deletion audit operation metadata is retained without erased target/object/source ID associations.".into()});
    result.push(ExportOmission{table:"usage_reservations".into(),columns:vec!["run_id".into()],reason:"The original billing entry remains; its run association is omitted if that run is unavailable. No usage or charges are regenerated.".into()});
    result.push(ExportOmission{table:"_sqlx_migrations".into(),columns:vec!["*".into()],reason:"Migration implementation bookkeeping is excluded; actual portable schema is bound by schema_fingerprint. This is not a restorable backup.".into()});
    result
}
async fn schema(tx: &mut Transaction<'_, Postgres>) -> Result<&'static str, StoreError> {
    let columns:Value=sqlx::query_scalar("SELECT jsonb_agg(jsonb_build_array(table_name,column_name,data_type,udt_name) ORDER BY table_name,ordinal_position) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name<>'_sqlx_migrations'").fetch_one(&mut **tx).await?;
    let constraints:Value=sqlx::query_scalar("SELECT jsonb_agg(jsonb_build_array(c.relname,k.conname,pg_get_constraintdef(k.oid,true)) ORDER BY c.relname,k.conname) FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relname<>'_sqlx_migrations'").fetch_one(&mut **tx).await?;
    match digest(&json!({"columns":columns,"constraints":constraints}))?.as_str() {
        SCHEMA_FINGERPRINT => Ok(SCHEMA_FINGERPRINT),
        SCHEMA30_FINGERPRINT => Ok(SCHEMA30_FINGERPRINT),
        _ => Err(StoreError::Invalid("personal export schema unsupported")),
    }
}
async fn privacy_pending(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
) -> Result<(), StoreError> {
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM privacy_operations WHERE individual_id=$1 AND status<>'completed')").bind(individual).fetch_one(&mut **tx).await?;
    if pending {
        return Err(StoreError::PrivacyUnavailable);
    }
    Ok(())
}
fn query(table: &Table, graph: &str) -> String {
    let owner = if table.name == "individuals" {
        "t.id=$1"
    } else {
        "t.individual_id=$1"
    };
    let mut filters = vec![owner.into()];
    for column in table.deps {
        filters.push(format!(
            "NOT EXISTS(SELECT 1 FROM hidden h WHERE h.id=t.{column})"
        ));
    }
    if table.name == "interest_sources" {
        filters.push("EXISTS(SELECT 1 FROM interests i WHERE i.individual_id=$1 AND i.id=t.interest_id AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.id=i.object_id))".into());
    }
    if table.name == "outbox" {
        filters.push("NOT EXISTS(SELECT 1 FROM hidden h WHERE h.id=t.resource_id)".into());
        filters.push("(EXISTS(SELECT 1 FROM objects o WHERE o.individual_id=$1 AND o.id=t.resource_id) OR EXISTS(SELECT 1 FROM budget_windows w WHERE w.individual_id=$1 AND w.id=t.resource_id) OR EXISTS(SELECT 1 FROM privacy_operations p WHERE p.individual_id=$1 AND p.id=t.resource_id))".into());
    }
    if table.name == "privacy_operations" {
        filters.push("t.status='completed'".into());
    }
    let removed = table
        .omit
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(",");
    let mut projection = format!("to_jsonb(t)-ARRAY[{removed}]::text[]");
    if table.name == "events" {
        projection = format!(
            "CASE WHEN t.kind LIKE 'privacy.%' THEN jsonb_set(({projection}),'{{payload}}',t.payload-ARRAY['target_id','object_ids','root_event_ids']::text[]) ELSE ({projection}) END"
        );
    }
    if table.name == "usage_reservations" {
        projection = format!(
            "({projection})-CASE WHEN EXISTS(SELECT 1 FROM hidden h WHERE h.id=t.run_id) THEN ARRAY['run_id']::text[] ELSE ARRAY[]::text[] END"
        );
    }
    let order = table
        .keys
        .iter()
        .map(|key| {
            if table.text_keys.contains(key) {
                format!("t.{key} COLLATE \"C\"")
            } else {
                format!("t.{key}")
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let sort_columns = table
        .keys
        .iter()
        .enumerate()
        .map(|(n, k)| format!("t.{k} AS export_sort_{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let outer_order = table
        .keys
        .iter()
        .enumerate()
        .map(|(n, k)| {
            if table.text_keys.contains(k) {
                format!("owned.export_sort_{n} COLLATE \"C\"")
            } else {
                format!("owned.export_sort_{n}")
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    // Avoid fetching a giant single jsonb value. PG's whitespace-bearing JSON text
    // can be at most twice the compact encoding; this bound never rejects a legal
    // <=64MiB row. Exact compact accounting follows in Rust, before retaining rows.
    format!(
        "{graph} SELECT octet_length(item::text)>{},CASE WHEN octet_length(item::text)>{} THEN NULL ELSE item END FROM (SELECT {projection} AS item,{sort_columns} FROM {} t WHERE {} ORDER BY {order} LIMIT {}) owned ORDER BY {outer_order}",
        EXPORT_MAX_BYTES * 2,
        EXPORT_MAX_BYTES * 2,
        table.name,
        filters.join(" AND "),
        EXPORT_MAX_TABLE_ROWS + 1
    )
}
// Read-only revision guards also catch real billing writes without a cognitive
// state-version bump. These constant-memory checksums never enter the archive.
async fn row_revisions(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    trials: bool,
) -> Result<BTreeMap<String, String>, StoreError> {
    let mut result = BTreeMap::new();
    for table in tables(trials) {
        let owner = if table.name == "idempotency_records" {
            "true"
        } else if table.name == "individuals" {
            "t.id=$1"
        } else {
            "t.individual_id=$1"
        };
        let keys = table
            .keys
            .iter()
            .map(|key| format!("t.{key}"))
            .collect::<Vec<_>>()
            .join(",");
        let tag = format!("jsonb_build_array({keys})::text||':'||t.xmin::text");
        let sql = format!(
            "SELECT concat(count(*),':',coalesce(sum(t.xmin::text::bigint),0)::text,':',coalesce(sum(hashtextextended({tag},0)::numeric),0)::text,':',coalesce(sum(hashtextextended({tag},1)::numeric),0)::text) FROM {} t WHERE ({owner}) AND $1::uuid IS NOT NULL",
            table.name
        );
        let revision: String = sqlx::query_scalar(&sql)
            .bind(individual)
            .fetch_one(&mut **tx)
            .await?;
        result.insert(table.name.into(), revision);
    }
    Ok(result)
}
impl Store {
    pub async fn export_individual(&self) -> Result<IndividualExport, StoreError> {
        let generation = self.privacy_generation();
        self.require_privacy_snapshot(generation)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let schema_fingerprint = schema(&mut tx).await?;
        let trials = schema_fingerprint == SCHEMA30_FINGERPRINT;
        let individual = existing(&mut tx).await?;
        privacy_pending(&mut tx, individual).await?;
        let graph = visibility(&mut tx).await?;
        super::privacy_development::verify_development_projection_in(&mut tx, individual).await?;
        let hidden_identity: bool = sqlx::query_scalar(&format!(
            "{graph} SELECT EXISTS(SELECT 1 FROM hidden WHERE id=$1)"
        ))
        .bind(individual)
        .fetch_one(&mut *tx)
        .await?;
        if hidden_identity {
            return Err(StoreError::NotFound);
        }
        let unknown_kind: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM objects WHERE individual_id=$1 AND NOT(kind=ANY($2)))",
        )
        .bind(individual)
        .bind(KNOWN_KINDS)
        .fetch_one(&mut *tx)
        .await?;
        if unknown_kind {
            return Err(StoreError::Invalid(
                "personal export object kind unsupported",
            ));
        }
        let (version,epoch,observed_at):(i64,i64,DateTime<Utc>)=sqlx::query_as("SELECT state_version,privacy_epoch,transaction_timestamp() FROM individuals WHERE id=$1").bind(individual).fetch_one(&mut *tx).await?;
        let revisions = row_revisions(&mut tx, individual, trials).await?;
        let mut archive = IndividualExport {
            format: EXPORT_FORMAT.into(),
            individual_id: individual,
            state_version: version,
            privacy_epoch: epoch,
            observed_at,
            schema_fingerprint: schema_fingerprint.into(),
            tables: tables(trials)
                .map(|t| (t.name.to_owned(), vec![]))
                .collect::<BTreeMap<_, _>>(),
            omissions: omissions(trials),
        };
        let mut total = json_size(&archive, EXPORT_MAX_BYTES).map_err(export_error)?;
        for table in tables(trials) {
            if table.skip.is_some() {
                continue;
            }
            self.require_privacy_snapshot(generation)?;
            let sql = query(table, &graph);
            let mut stream = sqlx::query_as::<_, (bool, Option<Value>)>(&sql)
                .bind(individual)
                .fetch(&mut *tx);
            let rows = archive
                .tables
                .get_mut(table.name)
                .ok_or(StoreError::Invalid("invalid personal export table"))?;
            while let Some((oversized, row)) = stream.try_next().await? {
                self.require_privacy_snapshot(generation)?;
                if rows.len() >= EXPORT_MAX_TABLE_ROWS {
                    return Err(export_error(ExportError::TooManyRows));
                }
                if oversized {
                    return Err(export_error(ExportError::TooLarge));
                }
                let mut row = row.ok_or(StoreError::Invalid("invalid personal export row"))?;
                scrub(&mut row);
                let comma = usize::from(!rows.is_empty());
                let available = EXPORT_MAX_BYTES
                    .checked_sub(total + comma)
                    .ok_or_else(|| export_error(ExportError::TooLarge))?;
                let row_bytes = json_size(&row, available).map_err(export_error)?;
                total += comma + row_bytes;
                rows.push(row);
            }
        }
        privacy_pending(&mut tx, individual).await?;
        tx.commit().await?;
        // Real bounded serialization and strict duplicate-key validation happen before
        // the final fresh DB/generation read. No partial archive escapes on any error.
        let encoded = archive.to_json_bytes().map_err(export_error)?;
        drop(encoded);
        self.require_privacy_snapshot(generation)?;
        let mut final_tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *final_tx)
            .await?;
        schema(&mut final_tx).await?;
        let fresh:Option<(Uuid,i64,i64,bool)>=sqlx::query_as("SELECT id,state_version,privacy_epoch,EXISTS(SELECT 1 FROM privacy_operations p WHERE p.individual_id=i.id AND p.status<>'completed') FROM individuals i WHERE singleton").fetch_optional(&mut *final_tx).await?;
        super::privacy_development::verify_development_projection_in(&mut final_tx, individual)
            .await?;
        let final_revisions = row_revisions(&mut final_tx, individual, trials).await?;
        final_tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        if final_revisions != revisions {
            return Err(StoreError::StaleState);
        }
        match fresh {
            Some((id, v, e, false)) if id == individual && v == version && e == epoch => {
                Ok(archive)
            }
            Some((_, _, _, true)) => Err(StoreError::PrivacyUnavailable),
            _ => Err(StoreError::StaleState),
        }
    }
}
