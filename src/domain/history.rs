use super::Lifecycle;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndividualState {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub origin: Value,
    pub seed: Value,
    pub seed_digest: String,
    pub persona: Value,
    pub runtime_config: Value,
    pub lifecycle: Lifecycle,
    pub state_version: i64,
    pub privacy_epoch: i64,
    pub config_version: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedState {
    pub version: i64,
    pub privacy_epoch: i64,
}

impl IndividualState {
    pub fn expected(&self) -> ExpectedState {
        ExpectedState {
            version: self.state_version,
            privacy_epoch: self.privacy_epoch,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    DirectObservation,
    ReportedClaim,
    ExternalDocument,
    Inference,
    Generated,
    FictionalBackground,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub individual_id: Uuid,
    pub id: Uuid,
    pub sequence: i64,
    pub kind: String,
    pub source_kind: SourceKind,
    pub actor_id: String,
    pub occurred_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub payload: Value,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Working,
    Episodic,
    Semantic,
    Procedural,
    Autobiographical,
    Relational,
    Affective,
    Prospective,
    Meta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewMemory {
    pub content: String,
    pub kind: MemoryKind,
    pub scope: String,
    pub source_event_ids: Vec<Uuid>,
    pub supersedes: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub individual_id: Uuid,
    pub id: Uuid,
    pub content: String,
    pub kind: MemoryKind,
    pub scope: String,
    pub validity: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub version: i64,
    pub source_event_ids: Vec<Uuid>,
    pub supersedes: Option<Uuid>,
    #[serde(default)]
    pub person_assertions: Vec<super::entities::MemoryPersonRef>,
    #[serde(default)]
    pub person_assertions_more: bool,
}
impl Memory {
    pub fn person_evidence_ids(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.person_assertions
            .iter()
            .flat_map(|p| [p.entity_id, p.source_event_id, p.assertion_event_id])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Initialization {
    pub individual_id: Uuid,
    pub genesis_event_id: Uuid,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubmittedMessage {
    pub individual_id: Uuid,
    pub event_id: Uuid,
    pub operation_id: Uuid,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordedMemories {
    pub memory_ids: Vec<Uuid>,
    pub event_id: Uuid,
    pub version: i64,
}
