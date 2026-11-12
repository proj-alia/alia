//! Frozen, deterministic measurement contracts. Metadata is never authorization.
use super::development::{
    DevelopmentDraft, DevelopmentError, Outcome, StrategyScope, TargetSnapshot,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

pub const MEASUREMENT_VERSION: &str = "alia-json-comparison/1";
pub const MAX_TRIAL_CASES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementProgram {
    JsonComparisonV1,
}
impl MeasurementProgram {
    pub fn version(self) -> &'static str {
        MEASUREMENT_VERSION
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrialSide {
    Baseline,
    Trial,
}
impl TrialSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Trial => "trial",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrialCaseRole {
    Scenario,
    CounterSearch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrialCriterion {
    JsonEquals { pointer: String, expected: Value },
}
impl TrialCriterion {
    pub fn validate(&self) -> Result<(), DevelopmentError> {
        match self {
            Self::JsonEquals { pointer, expected } => {
                if pointer.len() > 512
                    || pointer.contains('\0')
                    || (!pointer.is_empty() && !pointer.starts_with('/'))
                    || serde_json::to_vec(expected)
                        .map_err(|_| DevelopmentError::Invalid)?
                        .len()
                        > 16 * 1024
                {
                    return Err(DevelopmentError::Invalid);
                }
                let mut chars = pointer.chars();
                while let Some(c) = chars.next() {
                    if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
                        return Err(DevelopmentError::Invalid);
                    }
                }
                Ok(())
            }
        }
    }
    /// Only the frozen structural condition is measured, not semantic maturity.
    pub fn matches(&self, actual: &Value) -> bool {
        match self {
            Self::JsonEquals { pointer, expected } => actual.pointer(pointer) == Some(expected),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialCase {
    pub id: Uuid,
    pub context: String,
    pub prompt: String,
    pub role: TrialCaseRole,
    pub criterion: TrialCriterion,
    pub source_event_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialPlanDraft {
    pub cases: Vec<TrialCase>,
    pub deadline: DateTime<Utc>,
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && !s.contains('\0') && s.chars().count() <= max
}
impl TrialPlanDraft {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), DevelopmentError> {
        if self.deadline <= now || !(3..=MAX_TRIAL_CASES).contains(&self.cases.len()) {
            return Err(DevelopmentError::Invalid);
        }
        let mut ids = HashSet::new();
        let mut contexts = HashSet::new();
        let mut scenario_prompts = HashSet::new();
        let mut counter = false;
        for case in &self.cases {
            if case.id.is_nil()
                || !ids.insert(case.id)
                || !text(&case.context, 200)
                || !text(&case.prompt, 4000)
                || !(1..=32).contains(&case.source_event_ids.len())
                || case.source_event_ids.iter().any(Uuid::is_nil)
                || case.source_event_ids.iter().collect::<HashSet<_>>().len()
                    != case.source_event_ids.len()
            {
                return Err(DevelopmentError::Invalid);
            }
            case.criterion.validate()?;
            match case.role {
                TrialCaseRole::Scenario => {
                    contexts.insert(case.context.trim());
                    scenario_prompts
                        .insert(case.prompt.split_whitespace().collect::<Vec<_>>().join(" "));
                }
                TrialCaseRole::CounterSearch => counter = true,
            }
        }
        if contexts.len() < 2 || scenario_prompts.len() < 2 || !counter {
            return Err(DevelopmentError::Unverified);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialPlan {
    pub id: Uuid,
    pub individual_id: Uuid,
    pub development_id: Uuid,
    pub development_version: i64,
    pub draft: DevelopmentDraft,
    pub proposer_id: Uuid,
    pub trial_event_id: Uuid,
    pub target: TargetSnapshot,
    pub original_scope: Option<StrategyScope>,
    pub evaluator_id: Uuid,
    pub criterion_version: String,
    pub cases_digest: String,
    pub cases: Vec<TrialCase>,
    pub deadline: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialBinding {
    pub plan_id: Uuid,
    pub case_id: Uuid,
    pub side: TrialSide,
    pub attempt_id: Uuid,
    pub evaluation_run_id: Uuid,
    pub lease_generation: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialWork {
    pub plan: TrialPlan,
    pub case: TrialCase,
    pub evaluation_run_id: Uuid,
    pub baseline: TrialBinding,
    pub trial: TrialBinding,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrialAttemptState {
    Pending,
    Reserved,
    Dispatched,
    Recorded,
    Unknown,
    NotRun,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialAttempt {
    pub binding: TrialBinding,
    pub state: TrialAttemptState,
    pub reservation_id: Option<Uuid>,
    pub result_event_id: Option<Uuid>,
    pub outcome: Outcome,
    pub production_measurement: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingTrialCase {
    pub plan_id: Uuid,
    pub case_id: Uuid,
    pub deadline: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialWorkStatus {
    pub plan_id: Uuid,
    pub attempts: Vec<TrialAttempt>,
    pub outcome: Outcome,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialReviewStatus {
    pub plan_id: Uuid,
    pub review_event_id: Uuid,
    pub outcome: Outcome,
}
/// Internal controller intent; model/HTTP JSON cannot deserialize this command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TrialDisposition {
    Accept,
    Reject { reason: String },
    Expire,
}

/// `None` is a missing/unknown/nonproduction side, never an assumed success.
pub fn measure_case(case: &TrialCase, baseline: Option<&Value>, trial: Option<&Value>) -> Outcome {
    let (Some(baseline), Some(trial)) = (baseline, trial) else {
        return Outcome::Unverified;
    };
    let before = case.criterion.matches(baseline);
    let after = case.criterion.matches(trial);
    match (case.role, before, after) {
        (_, _, false) => Outcome::Failed,
        (TrialCaseRole::Scenario, false, true) | (TrialCaseRole::CounterSearch, true, true) => {
            Outcome::Passed
        }
        _ => Outcome::Unverified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn case(role: TrialCaseRole) -> TrialCase {
        TrialCase {
            id: Uuid::new_v4(),
            context: "observable condition".into(),
            prompt: "controlled scenario".into(),
            role,
            criterion: TrialCriterion::JsonEquals {
                pointer: "/reply".into(),
                expected: json!("expected"),
            },
            source_event_ids: vec![Uuid::new_v4()],
        }
    }
    #[test]
    fn measured_improvement_requires_both_sides_and_does_not_credit_repetition() {
        let c = case(TrialCaseRole::Scenario);
        let yes = json!({"reply":"expected"});
        let no = json!({"reply":"other"});
        assert_eq!(measure_case(&c, Some(&no), Some(&yes)), Outcome::Passed);
        assert_eq!(
            measure_case(&c, Some(&yes), Some(&yes)),
            Outcome::Unverified
        );
        assert_eq!(measure_case(&c, None, Some(&yes)), Outcome::Unverified);
        assert_eq!(measure_case(&c, Some(&yes), Some(&no)), Outcome::Failed);
        let counter = case(TrialCaseRole::CounterSearch);
        assert_eq!(
            measure_case(&counter, Some(&yes), Some(&yes)),
            Outcome::Passed
        );
        assert_eq!(
            measure_case(&counter, Some(&no), Some(&yes)),
            Outcome::Unverified
        );
    }
    #[test]
    fn plan_requires_distinct_contexts_complete_counter_case_and_closed_criteria() {
        let now = Utc::now();
        let mut p = TrialPlanDraft {
            cases: vec![
                case(TrialCaseRole::Scenario),
                case(TrialCaseRole::Scenario),
                case(TrialCaseRole::CounterSearch),
            ],
            deadline: now + chrono::Duration::hours(1),
        };
        assert!(p.validate(now).is_err());
        p.cases[1].context = "different actual case context".into();
        assert!(
            p.validate(now).is_err(),
            "renaming one identical prompt is not a second scenario"
        );
        p.cases[1].prompt = "different controlled input".into();
        assert!(p.validate(now).is_ok());
        p.cases.pop();
        assert!(p.validate(now).is_err());
        assert!(
            serde_json::from_value::<TrialCriterion>(
                json!({"kind":"json_equals","pointer":"/reply","expected":"x","passed":true})
            )
            .is_err()
        );
        assert!(
            TrialCriterion::JsonEquals {
                pointer: "/~2bad".into(),
                expected: Value::Null
            }
            .validate()
            .is_err()
        );
    }
}
