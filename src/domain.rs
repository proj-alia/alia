//! Pure state rules. Transport, database and model types stay outside this module.
use serde::{Deserialize, Serialize};
pub mod affect;
pub mod autobiography;
pub mod beliefs;
pub mod cognition;
pub mod completion;
pub mod config;
pub mod development;
pub mod development_context;
pub mod development_proposals;
pub mod development_trials;
pub mod entities;
pub mod export;
pub mod grounding;
pub mod history;
pub mod interests;
pub mod privacy;
pub mod relationships;
pub mod retrieval;
pub mod review;
pub mod social;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Active,
    Paused,
    Archived,
    Terminated,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleCommand {
    Pause,
    Resume,
    Archive,
    Restore,
    Terminate,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid lifecycle transition")]
pub struct InvalidTransition;

impl Lifecycle {
    pub fn transition(self, command: LifecycleCommand) -> Result<Self, InvalidTransition> {
        use LifecycleCommand as C;
        match (self, command) {
            (Self::Active, C::Pause) => Ok(Self::Paused),
            (Self::Paused, C::Resume) => Ok(Self::Active),
            (Self::Active | Self::Paused, C::Archive) => Ok(Self::Archived),
            (Self::Archived, C::Restore) => Ok(Self::Paused),
            (Self::Active | Self::Paused | Self::Archived, C::Terminate) => Ok(Self::Terminated),
            _ => Err(InvalidTransition),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archived_individual_must_restore_to_paused_before_resuming() {
        let state = Lifecycle::Active
            .transition(LifecycleCommand::Archive)
            .unwrap();
        assert!(state.transition(LifecycleCommand::Resume).is_err());
        let restored = state.transition(LifecycleCommand::Restore).unwrap();
        assert_eq!(restored, Lifecycle::Paused);
        assert_eq!(
            restored.transition(LifecycleCommand::Resume).unwrap(),
            Lifecycle::Active
        );
    }

    #[test]
    fn terminated_identity_cannot_be_reanimated_by_any_command() {
        for command in [
            LifecycleCommand::Pause,
            LifecycleCommand::Resume,
            LifecycleCommand::Archive,
            LifecycleCommand::Restore,
            LifecycleCommand::Terminate,
        ] {
            assert!(Lifecycle::Terminated.transition(command).is_err());
        }
    }
}
