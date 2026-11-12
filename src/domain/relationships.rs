//! Saved relationship interpretations, separate from mood and others' feelings.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipState {
    pub stage: String,
    pub expectations: Vec<String>,
    pub boundaries: Vec<String>,
    pub commitments: Vec<String>,
    pub unresolved: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RelationshipDelta {
    pub stage: Option<String>,
    pub expectations: Option<Vec<String>>,
    pub boundaries: Option<Vec<String>>,
    pub commitments: Option<Vec<String>>,
    pub unresolved: Option<Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipRevisionDraft {
    pub subject_id: Uuid,
    pub expected_relationship_version: i64,
    pub changes: RelationshipDelta,
    pub event_ids: Vec<Uuid>,
    pub brief_basis: String,
}
#[derive(Debug, Clone)]
pub struct CurrentRelationship {
    pub individual_id: Uuid,
    pub subject_id: Uuid,
    pub version: i64,
    pub state: RelationshipState,
}
#[derive(Debug, Clone)]
pub struct PreviousRelationshipRevision {
    pub individual_id: Uuid,
    pub subject_id: Uuid,
    pub id: Uuid,
    pub after_version: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RelationshipRevisionError {
    #[error("invalid relationship change fields")]
    Invalid,
    #[error("relationship revision requires actual available events")]
    Evidence,
    #[error("relationship revision target or version does not match current state")]
    Target,
    #[error("relationship revision has no actual change")]
    NoChange,
    #[error("previous revision is not from this current owned relationship")]
    Previous,
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && !s.contains('\0') && s.chars().count() <= max
}
fn strings(values: &[String]) -> bool {
    values.len() <= 16
        && values.iter().all(|v| text(v, 2000))
        && values.iter().collect::<HashSet<_>>().len() == values.len()
}
impl RelationshipState {
    pub fn validate(&self) -> Result<(), RelationshipRevisionError> {
        if !text(&self.stage, 80)
            || [
                &self.expectations,
                &self.boundaries,
                &self.commitments,
                &self.unresolved,
            ]
            .iter()
            .any(|v| !strings(v))
        {
            return Err(RelationshipRevisionError::Invalid);
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct ValidatedRelationshipRevision {
    individual_id: Uuid,
    draft: RelationshipRevisionDraft,
    before: RelationshipState,
    after: RelationshipState,
    previous_id: Option<Uuid>,
}
impl ValidatedRelationshipRevision {
    pub fn individual_id(&self) -> Uuid {
        self.individual_id
    }
    pub fn draft(&self) -> &RelationshipRevisionDraft {
        &self.draft
    }
    pub fn before(&self) -> &RelationshipState {
        &self.before
    }
    pub fn after(&self) -> &RelationshipState {
        &self.after
    }
    pub fn previous_revision_id(&self) -> Option<Uuid> {
        self.previous_id
    }
}
impl RelationshipRevisionDraft {
    pub fn validate_shape(&self) -> Result<(), RelationshipRevisionError> {
        if self.subject_id.is_nil()
            || self.expected_relationship_version <= 0
            || !text(&self.brief_basis, 1000)
        {
            return Err(RelationshipRevisionError::Invalid);
        }
        let events: HashSet<_> = self.event_ids.iter().copied().collect();
        if events.is_empty()
            || events.len() > 32
            || events.len() != self.event_ids.len()
            || events.contains(&Uuid::nil())
        {
            return Err(RelationshipRevisionError::Evidence);
        }
        if self.changes.stage.as_ref().is_some_and(|s| !text(s, 80))
            || [
                &self.changes.expectations,
                &self.changes.boundaries,
                &self.changes.commitments,
                &self.changes.unresolved,
            ]
            .iter()
            .any(|v| v.as_ref().is_some_and(|v| !strings(v)))
        {
            return Err(RelationshipRevisionError::Invalid);
        }
        Ok(())
    }
    pub fn validate(
        self,
        individual_id: Uuid,
        current: &CurrentRelationship,
        events: &HashSet<Uuid>,
        previous: Option<&PreviousRelationshipRevision>,
    ) -> Result<ValidatedRelationshipRevision, RelationshipRevisionError> {
        self.validate_shape()?;
        current.state.validate()?;
        if individual_id.is_nil()
            || current.individual_id != individual_id
            || current.subject_id != self.subject_id
            || current.version != self.expected_relationship_version
        {
            return Err(RelationshipRevisionError::Target);
        }
        if !self.event_ids.iter().all(|id| events.contains(id)) {
            return Err(RelationshipRevisionError::Evidence);
        }
        if previous.is_some_and(|p| {
            p.individual_id != individual_id
                || p.subject_id != self.subject_id
                || p.id.is_nil()
                || p.after_version <= 0
                || p.after_version > current.version
        }) {
            return Err(RelationshipRevisionError::Previous);
        }
        let mut after = current.state.clone();
        if let Some(v) = &self.changes.stage {
            after.stage = v.clone();
        }
        for (change, field) in [
            (&self.changes.expectations, &mut after.expectations),
            (&self.changes.boundaries, &mut after.boundaries),
            (&self.changes.commitments, &mut after.commitments),
            (&self.changes.unresolved, &mut after.unresolved),
        ] {
            if let Some(v) = change {
                *field = v.clone();
            }
        }
        if after == current.state {
            return Err(RelationshipRevisionError::NoChange);
        }
        Ok(ValidatedRelationshipRevision {
            individual_id,
            draft: self,
            before: current.state.clone(),
            after,
            previous_id: previous.map(|p| p.id),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> RelationshipState {
        RelationshipState {
            stage: "acquainted".into(),
            expectations: vec!["遇到不确定时核查".into()],
            boundaries: vec!["不替对方断言感情".into()],
            commitments: vec![],
            unresolved: vec!["一次实际分歧尚待核对".into()],
        }
    }
    fn draft(subject: Uuid, event: Uuid) -> RelationshipRevisionDraft {
        RelationshipRevisionDraft {
            subject_id: subject,
            expected_relationship_version: 7,
            changes: RelationshipDelta {
                unresolved: Some(vec![]),
                ..Default::default()
            },
            event_ids: vec![event],
            brief_basis: "新输入为本次调整提供了有限依据".into(),
        }
    }
    #[test]
    fn omitted_fields_keep_state_and_explicit_empty_clears_only_selected_field() {
        let owner = Uuid::new_v4();
        let subject = Uuid::new_v4();
        let e = Uuid::new_v4();
        let current = CurrentRelationship {
            individual_id: owner,
            subject_id: subject,
            version: 7,
            state: state(),
        };
        let v = draft(subject, e)
            .validate(owner, &current, &HashSet::from([e]), None)
            .unwrap();
        assert!(v.after().unresolved.is_empty());
        assert_eq!(v.after().boundaries, current.state.boundaries);
        assert_eq!(v.before().unresolved, current.state.unresolved);
        assert_eq!(current.version, 7);
    }
    #[test]
    fn empty_null_noop_or_unregistered_fields_do_not_create_revisions() {
        let owner = Uuid::new_v4();
        let subject = Uuid::new_v4();
        let e = Uuid::new_v4();
        let current = CurrentRelationship {
            individual_id: owner,
            subject_id: subject,
            version: 7,
            state: state(),
        };
        for change in [
            RelationshipDelta::default(),
            RelationshipDelta {
                stage: Some("acquainted".into()),
                ..Default::default()
            },
        ] {
            let mut d = draft(subject, e);
            d.changes = change;
            assert!(matches!(
                d.validate(owner, &current, &HashSet::from([e]), None),
                Err(RelationshipRevisionError::NoChange)
            ));
        }
        for key in [
            "personality",
            "permissions",
            "other_person_feelings",
            "affect",
        ] {
            let mut v = serde_json::to_value(RelationshipDelta::default()).unwrap();
            v[key] = serde_json::json!("change");
            assert!(serde_json::from_value::<RelationshipDelta>(v).is_err());
        }
    }
    #[test]
    fn lists_and_unicode_bounds_require_distinct_nonempty_saved_strings() {
        let e = Uuid::new_v4();
        let mut d = draft(Uuid::new_v4(), e);
        d.changes.boundaries = Some(vec!["边".repeat(2000)]);
        assert!(d.validate_shape().is_ok());
        d.changes.boundaries = Some(vec!["边".repeat(2001)]);
        assert_eq!(d.validate_shape(), Err(RelationshipRevisionError::Invalid));
        for values in [
            vec![" ".into()],
            vec!["x".into(), "x".into()],
            vec!["x".into(); 17],
        ] {
            d.changes.boundaries = Some(values);
            assert_eq!(d.validate_shape(), Err(RelationshipRevisionError::Invalid));
        }
    }
    #[test]
    fn actual_current_version_and_owner_are_independent_of_revision_count() {
        let owner = Uuid::new_v4();
        let subject = Uuid::new_v4();
        let e = Uuid::new_v4();
        let current = CurrentRelationship {
            individual_id: owner,
            subject_id: subject,
            version: 7,
            state: state(),
        };
        let previous = PreviousRelationshipRevision {
            individual_id: owner,
            subject_id: subject,
            id: Uuid::new_v4(),
            after_version: 3,
        };
        assert!(
            draft(subject, e)
                .validate(owner, &current, &HashSet::from([e]), Some(&previous))
                .is_ok()
        );
        for (version, other_owner) in [(6, owner), (7, Uuid::new_v4())] {
            let mut c = current.clone();
            c.version = version;
            c.individual_id = other_owner;
            assert!(matches!(
                draft(subject, e).validate(owner, &c, &HashSet::from([e]), Some(&previous)),
                Err(RelationshipRevisionError::Target)
            ));
        }
    }
    #[test]
    fn a_previous_revision_from_another_subject_or_future_version_is_invalid() {
        let owner = Uuid::new_v4();
        let subject = Uuid::new_v4();
        let e = Uuid::new_v4();
        let c = CurrentRelationship {
            individual_id: owner,
            subject_id: subject,
            version: 7,
            state: state(),
        };
        for (s, v) in [(Uuid::new_v4(), 2), (subject, 8), (subject, 0)] {
            let p = PreviousRelationshipRevision {
                individual_id: owner,
                subject_id: s,
                id: Uuid::new_v4(),
                after_version: v,
            };
            assert!(matches!(
                draft(subject, e).validate(owner, &c, &HashSet::from([e]), Some(&p)),
                Err(RelationshipRevisionError::Previous)
            ));
        }
        assert!(matches!(
            draft(subject, e).validate(owner, &c, &HashSet::new(), None),
            Err(RelationshipRevisionError::Evidence)
        ));
    }
}
