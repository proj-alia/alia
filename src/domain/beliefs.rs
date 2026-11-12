//! Evidence-bound revisions of an individual's current understanding.
//! A held position is a stance, not certification that a statement is true.
//! Persistence must recheck versions and source revocation inside its transaction.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeliefPosition {
    Tentative,
    Held,
    Questioned,
}

/// A bounded selection of current stances, never a complete world model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedBeliefs {
    pub items: Vec<CurrentBelief>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentBelief {
    pub id: Uuid,
    pub version: i64,
    pub statement: String,
    pub scope: String,
    pub position: BeliefPosition,
    pub brief_basis: String,
    pub support_event_ids: Vec<Uuid>,
    pub counter_event_ids: Vec<Uuid>,
}

impl CurrentBelief {
    pub fn evidence_ids(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.support_event_ids
            .iter()
            .chain(&self.counter_event_ids)
            .copied()
    }
}

impl SelectedBeliefs {
    /// Every selected stance retains its entire declared basis in this snapshot.
    /// This checks provenance availability, not whether a statement is true.
    pub fn valid_for(&self, context: &super::cognition::ModelContext) -> bool {
        let events: HashSet<_> = context
            .events
            .iter()
            .filter(|e| e.kind != "decision.reviewed" && e.individual_id == context.individual_id)
            .map(|e| e.id)
            .collect();
        let manifest: HashSet<_> = context.input_manifest.iter().copied().collect();
        let mut seen = HashSet::new();
        self.items.len() <= 6
            && (!self.items.is_empty() || self.has_more)
            && self.items.iter().all(|b| {
                !b.id.is_nil()
                    && seen.insert(b.id)
                    && b.version > 0
                    && (b.scope == "general" || b.scope == context.current_scope)
                    && manifest.contains(&b.id)
                    && b.evidence_ids().all(|id| manifest.contains(&id))
                    && (BeliefDraft {
                        statement: b.statement.clone(),
                        scope: b.scope.clone(),
                        position: b.position,
                        support_event_ids: b.support_event_ids.clone(),
                        counter_event_ids: b.counter_event_ids.clone(),
                        supersedes: None,
                        brief_basis: b.brief_basis.clone(),
                    })
                    .validate(context.individual_id, &events, None)
                    .is_ok()
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeliefDraft {
    pub statement: String,
    pub scope: String,
    pub position: BeliefPosition,
    pub support_event_ids: Vec<Uuid>,
    pub counter_event_ids: Vec<Uuid>,
    pub supersedes: Option<Uuid>,
    pub brief_basis: String,
}

/// Loaded state, never an identity supplied by the proposing model.
#[derive(Debug, Clone)]
pub struct PreviousBelief {
    pub individual_id: Uuid,
    pub id: Uuid,
    pub version: i64,
    pub scope: String,
    pub superseded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BeliefError {
    #[error("invalid belief text or identity")]
    Invalid,
    #[error("belief evidence is missing, duplicated, or unavailable")]
    Evidence,
    #[error("belief revision does not match the current owned version")]
    Previous,
    #[error("a local exception cannot supersede a belief in another scope")]
    Scope,
}

#[derive(Debug)]
pub struct ValidatedBelief {
    draft: BeliefDraft,
    individual_id: Uuid,
    expected_previous: Option<(Uuid, i64)>,
}

impl ValidatedBelief {
    pub fn draft(&self) -> &BeliefDraft {
        &self.draft
    }

    pub fn individual_id(&self) -> Uuid {
        self.individual_id
    }

    pub fn expected_previous(&self) -> Option<(Uuid, i64)> {
        self.expected_previous
    }
}

impl BeliefDraft {
    pub fn validate_context(
        &self,
        context: &super::cognition::ModelContext,
    ) -> Result<ValidatedBelief, BeliefError> {
        if self.scope != "general" && self.scope != context.current_scope {
            return Err(BeliefError::Scope);
        }
        let previous = match self.supersedes {
            None => None,
            Some(id) => {
                let selection = context
                    .selected_beliefs
                    .as_ref()
                    .ok_or(BeliefError::Previous)?;
                if !selection.valid_for(context) {
                    return Err(BeliefError::Evidence);
                }
                let b = selection
                    .items
                    .iter()
                    .find(|b| b.id == id)
                    .ok_or(BeliefError::Previous)?;
                Some(PreviousBelief {
                    individual_id: context.individual_id,
                    id: b.id,
                    version: b.version,
                    scope: b.scope.clone(),
                    superseded: false,
                })
            }
        };
        let events = context
            .events
            .iter()
            .filter(|e| e.individual_id == context.individual_id && e.kind != "decision.reviewed")
            .map(|e| e.id)
            .collect();
        self.clone()
            .validate(context.individual_id, &events, previous.as_ref())
    }

    pub fn validate(
        self,
        individual_id: Uuid,
        visible_event_ids: &HashSet<Uuid>,
        previous: Option<&PreviousBelief>,
    ) -> Result<ValidatedBelief, BeliefError> {
        fn text(value: &str, max: usize) -> bool {
            !value.trim().is_empty() && !value.contains('\0') && value.chars().count() <= max
        }
        if individual_id.is_nil()
            || !text(&self.statement, 2000)
            || !text(&self.scope, 200)
            || !text(&self.brief_basis, 1000)
        {
            return Err(BeliefError::Invalid);
        }
        let support: HashSet<_> = self.support_event_ids.iter().copied().collect();
        let counter: HashSet<_> = self.counter_event_ids.iter().copied().collect();
        let all: HashSet<_> = support.union(&counter).copied().collect();
        if support.len() != self.support_event_ids.len()
            || counter.len() != self.counter_event_ids.len()
            || all.is_empty()
            || all.len() > 32
            || all.contains(&Uuid::nil())
            || !all.is_subset(visible_event_ids)
            || (self.position == BeliefPosition::Held && support.is_empty())
        {
            return Err(BeliefError::Evidence);
        }
        // One event may contain both supporting and challenging observations.
        // Keep both roles instead of fabricating mutually exclusive evidence.
        let expected_previous = match (self.supersedes, previous) {
            (None, None) => None,
            (Some(id), Some(old))
                if !id.is_nil()
                    && id == old.id
                    && old.individual_id == individual_id
                    && old.version > 0
                    && !old.superseded =>
            {
                if old.scope != self.scope {
                    return Err(BeliefError::Scope);
                }
                Some((id, old.version))
            }
            _ => return Err(BeliefError::Previous),
        };
        Ok(ValidatedBelief {
            draft: self,
            individual_id,
            expected_previous,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(event: Uuid) -> BeliefDraft {
        BeliefDraft {
            statement: "一般先研究；当前任务的例外另行判断".into(),
            scope: "general".into(),
            position: BeliefPosition::Tentative,
            support_event_ids: vec![event],
            counter_event_ids: vec![],
            supersedes: None,
            brief_basis: "来源是对方的陈述，保留后续修正可能".into(),
        }
    }

    #[test]
    fn substantial_reconsideration_keeps_support_and_counterevidence_without_mutating_history() {
        let owner = Uuid::new_v4();
        let event = Uuid::new_v4();
        let old = PreviousBelief {
            individual_id: owner,
            id: Uuid::new_v4(),
            version: 7,
            scope: "general".into(),
            superseded: false,
        };
        let mut changed = draft(event);
        changed.statement = "先前的一般理解已不足以解释当前证据".into();
        changed.position = BeliefPosition::Questioned;
        changed.support_event_ids.clear();
        changed.counter_event_ids.push(event);
        changed.supersedes = Some(old.id);
        let valid = changed
            .validate(owner, &HashSet::from([event]), Some(&old))
            .unwrap();
        assert_eq!(valid.expected_previous(), Some((old.id, 7)));
        assert_eq!(valid.draft().counter_event_ids, vec![event]);
        assert_eq!(valid.individual_id(), owner);
        assert!(!old.superseded);
        assert_eq!(old.version, 7);
    }

    #[test]
    fn local_exception_cannot_replace_general_understanding() {
        let event = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let old = PreviousBelief {
            individual_id: owner,
            id: Uuid::new_v4(),
            version: 1,
            scope: "general".into(),
            superseded: false,
        };
        let mut value = draft(event);
        value.supersedes = Some(old.id);
        value.scope = "task:prototype".into();
        assert_eq!(
            value
                .validate(owner, &HashSet::from([event]), Some(&old))
                .unwrap_err(),
            BeliefError::Scope
        );
    }

    #[test]
    fn revoked_unknown_duplicate_and_unsupported_held_evidence_are_rejected() {
        let event = Uuid::new_v4();
        let owner = Uuid::new_v4();
        assert_eq!(
            draft(event)
                .validate(owner, &HashSet::new(), None)
                .unwrap_err(),
            BeliefError::Evidence
        );
        let mut duplicate = draft(event);
        duplicate.support_event_ids.push(event);
        assert_eq!(
            duplicate
                .validate(owner, &HashSet::from([event]), None)
                .unwrap_err(),
            BeliefError::Evidence
        );
        let mut held = draft(event);
        held.position = BeliefPosition::Held;
        held.support_event_ids.clear();
        held.counter_event_ids.push(event);
        assert_eq!(
            held.validate(owner, &HashSet::from([event]), None)
                .unwrap_err(),
            BeliefError::Evidence
        );
    }

    #[test]
    fn another_individual_or_superseded_version_cannot_be_revised() {
        let event = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let mut old = PreviousBelief {
            individual_id: Uuid::new_v4(),
            id: Uuid::new_v4(),
            version: 1,
            scope: "general".into(),
            superseded: false,
        };
        let mut value = draft(event);
        value.supersedes = Some(old.id);
        assert_eq!(
            value
                .clone()
                .validate(owner, &HashSet::from([event]), Some(&old))
                .unwrap_err(),
            BeliefError::Previous
        );
        old.individual_id = owner;
        old.superseded = true;
        assert_eq!(
            value
                .validate(owner, &HashSet::from([event]), Some(&old))
                .unwrap_err(),
            BeliefError::Previous
        );
    }

    #[test]
    fn mixed_event_roles_are_preserved_and_unknown_fields_cannot_modify_permissions() {
        let event = Uuid::new_v4();
        let mut value = draft(event);
        value.counter_event_ids.push(event);
        let valid = value
            .validate(Uuid::new_v4(), &HashSet::from([event]), None)
            .unwrap();
        assert_eq!(
            valid.draft().support_event_ids,
            valid.draft().counter_event_ids
        );
        let mut json = serde_json::to_value(valid.draft()).unwrap();
        json["permissions"] = serde_json::json!(["execute-anything"]);
        assert!(serde_json::from_value::<BeliefDraft>(json).is_err());
    }
}
