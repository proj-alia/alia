//! Durable scheduling contracts. A lease is not permission to bypass policy checks.
use crate::domain::cognition::RunMode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLease {
    pub individual_id: Uuid,
    pub run_id: Uuid,
    pub owner_id: Uuid,
    pub generation: i64,
    pub expires_at: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
    pub state_version: i64,
    pub privacy_epoch: i64,
    pub config_version: i64,
    pub mode: RunMode,
}

/// Budget arithmetic input, supplied by a model adapter with a verified counting
/// contract. This value alone is not evidence of tokenizer correctness.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct TokenAllowance {
    pub input_upper_bound: u64,
    pub output_cap: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reservation {
    pub id: Uuid,
    pub run_id: Uuid,
    pub window_id: Uuid,
    pub status: String,
    pub reserved_tokens: i64,
    pub charged_tokens: i64,
    pub usage_unknown: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Failed,
    Cancelled,
    Stale,
}
impl RunOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Stale => "stale",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionReceipt {
    pub decision_event_id: Uuid,
    pub memory_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub belief_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub autobiography_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub appraisal_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attitude_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationship_revision_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub development_ids: Vec<Uuid>,
    pub goal_ids: Vec<Uuid>,
    pub action_ids: Vec<Uuid>,
    pub message_id: Option<Uuid>,
    pub message_status: Option<String>,
    pub state_version: i64,
    pub run_status: String,
}
