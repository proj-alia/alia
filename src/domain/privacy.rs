//! Deletion acceptance is a durable block, distinct from physical completion.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionStatus {
    Blocked,
    Purging,
    RetryPending,
    Completed,
}
impl DeletionStatus {
    pub fn can_transition(self, next: Self) -> bool {
        use DeletionStatus::*;
        matches!(
            (self, next),
            (Blocked, Purging | RetryPending)
                | (Purging, Completed | RetryPending)
                | (RetryPending, Purging)
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeletionReceipt {
    pub individual_id: Uuid,
    pub operation_id: Uuid,
    pub target_id: Uuid,
    pub state_version: i64,
    pub privacy_epoch: i64,
    pub status: DeletionStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeletionProgress {
    pub individual_id: Uuid,
    pub operation_id: Uuid,
    pub target_id: Uuid,
    pub state_version: i64,
    pub privacy_epoch: i64,
    pub status: DeletionStatus,
    pub observed_at: DateTime<Utc>,
    pub last_error: Option<String>,
}
#[cfg(test)]
mod tests {
    use super::DeletionStatus::*;
    #[test]
    fn accepting_and_retrying_never_skip_actual_purge() {
        assert!(!Blocked.can_transition(Completed));
        assert!(!RetryPending.can_transition(Completed));
        assert!(Blocked.can_transition(Purging));
        assert!(Purging.can_transition(RetryPending));
        assert!(RetryPending.can_transition(Purging));
        assert!(Purging.can_transition(Completed));
        for next in [Blocked, Purging, RetryPending, Completed] {
            assert!(!Completed.can_transition(next));
        }
    }
}
