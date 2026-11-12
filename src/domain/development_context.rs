//! Current development heads are provenance, not evidence of subjective growth.
use super::development::{DevelopmentTarget, StrategyScope, TaskStrategy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentDevelopmentHead {
    pub target_id: Uuid,
    pub target: DevelopmentTarget,
    pub version: i64,
    pub revision_id: Option<Uuid>,
    pub scope: Option<StrategyScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentContext {
    pub seed_event_id: Uuid,
    pub heads: Vec<CurrentDevelopmentHead>,
    /// Exact task scope takes precedence over its goal scope. Never permissions.
    pub effective_task_strategy: Option<TaskStrategy>,
    pub source_ids: Vec<Uuid>,
}
