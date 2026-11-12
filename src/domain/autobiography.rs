//! Current interpretations of actual personal history, not replacement events.
//! Validation establishes identity and available references, not narrative truth.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

/// Selected interpretations and their complete source bundles. Historical
/// stances are distinguished from current stances and from observed events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutobiographyContext {
    pub entries: Vec<CurrentAutobiography>,
    pub belief_basis: Vec<HistoricalBelief>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentAutobiography {
    pub id: Uuid,
    pub version: i64,
    pub topic: String,
    pub title: String,
    pub interpretation: String,
    pub event_ids: Vec<Uuid>,
    pub belief_ids: Vec<Uuid>,
    pub supersedes: Option<Uuid>,
    pub provenance_ids: Vec<Uuid>,
    pub basis_event_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalBelief {
    pub id: Uuid,
    pub version: i64,
    pub statement: String,
    pub scope: String,
    pub position: super::beliefs::BeliefPosition,
    pub brief_basis: String,
    pub status: String,
    pub supersedes: Option<Uuid>,
    pub support_event_ids: Vec<Uuid>,
    pub counter_event_ids: Vec<Uuid>,
}

impl AutobiographyContext {
    pub fn valid_for(&self, context: &super::cognition::ModelContext) -> bool {
        let events: HashSet<_> = context
            .events
            .iter()
            .filter(|e| e.individual_id == context.individual_id)
            .map(|e| e.id)
            .collect();
        let actual: HashSet<_> = context
            .events
            .iter()
            .filter(|e| {
                e.individual_id == context.individual_id
                    && e.source_kind != super::history::SourceKind::FictionalBackground
                    && e.kind != "decision.reviewed"
            })
            .map(|e| e.id)
            .collect();
        let manifest: HashSet<_> = context.input_manifest.iter().copied().collect();
        let beliefs: HashSet<_> = self.belief_basis.iter().map(|b| b.id).collect();
        let mut entries = HashSet::new();
        self.entries.len() <= 3
            && self.belief_basis.len() <= 64
            && (!self.entries.is_empty() || !self.belief_basis.is_empty() || self.has_more)
            && beliefs.len() == self.belief_basis.len()
            && self.belief_basis.iter().all(|b| {
                !b.id.is_nil()
                    && b.version > 0
                    && manifest.contains(&b.id)
                    && matches!(b.status.as_str(), "current" | "superseded")
                    && b.supersedes
                        .is_none_or(|id| beliefs.contains(&id) && manifest.contains(&id))
                    && b.support_event_ids
                        .iter()
                        .chain(&b.counter_event_ids)
                        .all(|id| manifest.contains(id))
                    && (super::beliefs::BeliefDraft {
                        statement: b.statement.clone(),
                        scope: b.scope.clone(),
                        position: b.position,
                        support_event_ids: b.support_event_ids.clone(),
                        counter_event_ids: b.counter_event_ids.clone(),
                        supersedes: None,
                        brief_basis: b.brief_basis.clone(),
                    })
                    .validate(context.individual_id, &actual, None)
                    .is_ok()
            })
            && self.entries.iter().all(|a| {
                let sources: HashSet<_> = a.provenance_ids.iter().copied().collect();
                let basis: HashSet<_> = a.basis_event_ids.iter().copied().collect();
                !a.id.is_nil()
                    && a.version > 0
                    && entries.insert(a.id)
                    && !sources.is_empty()
                    && sources.len() <= 1024
                    && sources.len() == a.provenance_ids.len()
                    && !sources.contains(&Uuid::nil())
                    && sources.contains(&a.id)
                    && sources.is_subset(&manifest)
                    && !basis.is_empty()
                    && basis.len() == a.basis_event_ids.len()
                    && basis.is_subset(&events)
                    && basis.is_subset(&sources)
                    && a.supersedes.is_none_or(|id| sources.contains(&id))
                    && a.event_ids
                        .iter()
                        .chain(&a.belief_ids)
                        .all(|id| sources.contains(id))
                    && self
                        .belief_basis
                        .iter()
                        .filter(|b| sources.contains(&b.id))
                        .all(|b| {
                            b.supersedes.is_none_or(|id| sources.contains(&id))
                                && b.support_event_ids
                                    .iter()
                                    .chain(&b.counter_event_ids)
                                    .all(|id| sources.contains(id))
                        })
                    && (AutobiographyDraft {
                        topic: a.topic.clone(),
                        title: a.title.clone(),
                        interpretation: a.interpretation.clone(),
                        event_ids: a.event_ids.clone(),
                        belief_ids: a.belief_ids.clone(),
                        supersedes: None,
                    })
                    .validate(context.individual_id, &actual, &beliefs, None)
                    .is_ok()
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutobiographyDraft {
    pub topic: String,
    pub title: String,
    pub interpretation: String,
    pub event_ids: Vec<Uuid>,
    pub belief_ids: Vec<Uuid>,
    pub supersedes: Option<Uuid>,
}

/// Owned persistent state loaded by the caller, never accepted from a model.
#[derive(Debug, Clone)]
pub struct PreviousAutobiography {
    pub individual_id: Uuid,
    pub id: Uuid,
    pub topic: String,
    pub version: i64,
    pub superseded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AutobiographyError {
    #[error("invalid historical interpretation or topic")]
    Invalid,
    #[error("personal history requires available actual events and distinct stance references")]
    Evidence,
    #[error("historical revision does not match the current owned topic")]
    Previous,
}

#[derive(Debug)]
pub struct ValidatedAutobiography {
    individual_id: Uuid,
    draft: AutobiographyDraft,
    expected_previous: Option<(Uuid, i64)>,
}

impl ValidatedAutobiography {
    pub fn individual_id(&self) -> Uuid {
        self.individual_id
    }
    pub fn draft(&self) -> &AutobiographyDraft {
        &self.draft
    }
    pub fn expected_previous(&self) -> Option<(Uuid, i64)> {
        self.expected_previous
    }
}

impl AutobiographyDraft {
    pub fn validate_context(
        &self,
        context: &super::cognition::ModelContext,
    ) -> Result<ValidatedAutobiography, AutobiographyError> {
        let selection = context.autobiographical_context.as_ref();
        if selection.is_some_and(|s| !s.valid_for(context)) {
            return Err(AutobiographyError::Evidence);
        }
        let previous = match self.supersedes {
            Some(id) => {
                let old = selection
                    .into_iter()
                    .flat_map(|s| &s.entries)
                    .find(|a| a.id == id)
                    .ok_or(AutobiographyError::Previous)?;
                Some(PreviousAutobiography {
                    individual_id: context.individual_id,
                    id,
                    topic: old.topic.clone(),
                    version: old.version,
                    superseded: false,
                })
            }
            None => None,
        };
        if self.supersedes.is_none()
            && selection
                .into_iter()
                .flat_map(|s| &s.entries)
                .any(|a| a.topic == self.topic)
        {
            return Err(AutobiographyError::Previous);
        }
        let events = context
            .events
            .iter()
            .filter(|e| {
                e.individual_id == context.individual_id
                    && e.kind != "decision.reviewed"
                    && e.source_kind != super::history::SourceKind::FictionalBackground
            })
            .map(|e| e.id)
            .collect();
        let beliefs = selection
            .into_iter()
            .flat_map(|s| &s.belief_basis)
            .map(|b| b.id)
            .collect();
        let manifest: HashSet<_> = context.input_manifest.iter().copied().collect();
        if self
            .event_ids
            .iter()
            .chain(&self.belief_ids)
            .chain(self.supersedes.iter())
            .any(|id| !manifest.contains(id))
        {
            return Err(AutobiographyError::Evidence);
        }
        self.clone()
            .validate(context.individual_id, &events, &beliefs, previous.as_ref())
    }

    /// References are typed records. Reviewer packets expand every stance and
    /// predecessor into its full source basis rather than promoting it to fact.
    pub fn review_references(&self) -> Vec<Uuid> {
        let mut ids: Vec<_> = self
            .event_ids
            .iter()
            .chain(&self.belief_ids)
            .chain(self.supersedes.iter())
            .copied()
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Event/stance sets must be independently loaded with ownership, recursive
    /// visibility and source classification checks in the caller's transaction.
    pub fn validate(
        self,
        individual_id: Uuid,
        actual_event_ids: &HashSet<Uuid>,
        available_belief_ids: &HashSet<Uuid>,
        previous: Option<&PreviousAutobiography>,
    ) -> Result<ValidatedAutobiography, AutobiographyError> {
        let topic = self.topic.as_bytes();
        let valid_text =
            |s: &str, max| !s.trim().is_empty() && !s.contains('\0') && s.chars().count() <= max;
        if individual_id.is_nil()
            || topic.is_empty()
            || topic.len() > 80
            || !topic[0].is_ascii_lowercase()
            || !topic
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(b))
            || !valid_text(&self.title, 160)
            || !valid_text(&self.interpretation, 4000)
        {
            return Err(AutobiographyError::Invalid);
        }
        let events: HashSet<_> = self.event_ids.iter().copied().collect();
        let beliefs: HashSet<_> = self.belief_ids.iter().copied().collect();
        if events.is_empty()
            || events.len() > 32
            || events.len() != self.event_ids.len()
            || events.contains(&Uuid::nil())
            || !events.is_subset(actual_event_ids)
            || beliefs.len() > 8
            || beliefs.len() != self.belief_ids.len()
            || beliefs.contains(&Uuid::nil())
            || !beliefs.is_subset(available_belief_ids)
        {
            return Err(AutobiographyError::Evidence);
        }
        let expected_previous = match (self.supersedes, previous) {
            (None, None) => None,
            (Some(id), Some(old))
                if !id.is_nil()
                    && id == old.id
                    && old.individual_id == individual_id
                    && old.topic == self.topic
                    && old.version > 0
                    && !old.superseded =>
            {
                Some((id, old.version))
            }
            _ => return Err(AutobiographyError::Previous),
        };
        Ok(ValidatedAutobiography {
            individual_id,
            draft: self,
            expected_previous,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(event: Uuid) -> AutobiographyDraft {
        AutobiographyDraft {
            topic: "learning.first-project".into(),
            title: "第一次共同探索".into(),
            interpretation: "当时我依据已有资料提出方案；后来的反例改变了我的理解。".into(),
            event_ids: vec![event],
            belief_ids: vec![],
            supersedes: None,
        }
    }

    #[test]
    fn reconsideration_retains_topic_identity_and_does_not_mutate_previous_history() {
        let owner = Uuid::new_v4();
        let event = Uuid::new_v4();
        let belief = Uuid::new_v4();
        let old = PreviousAutobiography {
            individual_id: owner,
            id: Uuid::new_v4(),
            topic: "learning.first-project".into(),
            version: 4,
            superseded: false,
        };
        let mut d = draft(event);
        d.belief_ids = vec![belief];
        d.supersedes = Some(old.id);
        let v = d
            .validate(
                owner,
                &HashSet::from([event]),
                &HashSet::from([belief]),
                Some(&old),
            )
            .unwrap();
        assert_eq!(v.expected_previous(), Some((old.id, 4)));
        assert_eq!(v.individual_id(), owner);
        assert_eq!(v.draft().event_ids, vec![event]);
        assert_eq!(old.version, 4);
        assert!(!old.superseded);
    }

    #[test]
    fn stance_alone_cannot_become_a_personal_experience() {
        let event = Uuid::new_v4();
        let belief = Uuid::new_v4();
        let mut d = draft(event);
        d.event_ids.clear();
        d.belief_ids.push(belief);
        assert!(matches!(
            d.validate(
                Uuid::new_v4(),
                &HashSet::new(),
                &HashSet::from([belief]),
                None
            ),
            Err(AutobiographyError::Evidence)
        ));
    }

    #[test]
    fn unknown_or_repeated_sources_do_not_establish_history() {
        let event = Uuid::new_v4();
        let owner = Uuid::new_v4();
        assert!(matches!(
            draft(event).validate(owner, &HashSet::new(), &HashSet::new(), None),
            Err(AutobiographyError::Evidence)
        ));
        let mut d = draft(event);
        d.event_ids.push(event);
        assert!(matches!(
            d.validate(owner, &HashSet::from([event]), &HashSet::new(), None),
            Err(AutobiographyError::Evidence)
        ));
    }

    #[test]
    fn foreign_or_obsolete_predecessor_cannot_rewrite_the_current_topic() {
        let owner = Uuid::new_v4();
        let event = Uuid::new_v4();
        let id = Uuid::new_v4();
        for (individual_id, topic, version, superseded) in [
            (Uuid::new_v4(), "learning.first-project", 1, false),
            (owner, "relationship.first-meeting", 1, false),
            (owner, "learning.first-project", 0, false),
            (owner, "learning.first-project", 1, true),
        ] {
            let old = PreviousAutobiography {
                individual_id,
                id,
                topic: topic.into(),
                version,
                superseded,
            };
            let mut d = draft(event);
            d.supersedes = Some(id);
            assert!(matches!(
                d.validate(owner, &HashSet::from([event]), &HashSet::new(), Some(&old)),
                Err(AutobiographyError::Previous)
            ));
        }
    }

    #[test]
    fn topics_are_stable_keys_and_unicode_text_has_character_limits() {
        let event = Uuid::new_v4();
        for topic in ["", " General", "GENERAL", "topic/name", "话题"] {
            let mut d = draft(event);
            d.topic = topic.into();
            assert!(matches!(
                d.validate(
                    Uuid::new_v4(),
                    &HashSet::from([event]),
                    &HashSet::new(),
                    None
                ),
                Err(AutobiographyError::Invalid)
            ));
        }
        let mut d = draft(event);
        d.interpretation = "我".repeat(4000);
        assert!(
            d.clone()
                .validate(
                    Uuid::new_v4(),
                    &HashSet::from([event]),
                    &HashSet::new(),
                    None
                )
                .is_ok()
        );
        d.interpretation.push('们');
        assert!(matches!(
            d.validate(
                Uuid::new_v4(),
                &HashSet::from([event]),
                &HashSet::new(),
                None
            ),
            Err(AutobiographyError::Invalid)
        ));
    }
}
