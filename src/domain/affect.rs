//! Bounded appraisal and separately revised attitudes, not subjective measures.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

pub const APPRAISAL_LIFETIME_SECONDS: i32 = 3600;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppraisalDraft {
    pub event_ids: Vec<Uuid>,
    pub goal_id: Option<Uuid>,
    pub subject_id: Option<Uuid>,
    pub valence: f64,
    pub arousal: f64,
    pub goal_relevance: f64,
    pub expectation_gap: f64,
    pub controllability: f64,
    pub uncertainty: f64,
    pub brief_basis: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttitudeDraft {
    pub topic: String,
    pub subject_id: Option<Uuid>,
    pub stance: String,
    pub event_ids: Vec<Uuid>,
    pub supersedes: Option<Uuid>,
    pub brief_basis: String,
}
#[derive(Debug, Clone)]
pub struct PreviousAttitude {
    pub individual_id: Uuid,
    pub id: Uuid,
    pub topic: String,
    pub subject_id: Option<Uuid>,
    pub version: i64,
    pub superseded: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AffectError {
    #[error("invalid bounded affect fields")]
    Invalid,
    #[error("affect requires actual available source events")]
    Evidence,
    #[error("affect target is not an actual owned goal or relationship")]
    Target,
    #[error("attitude revision requires the current same-topic and same-subject predecessor")]
    Previous,
}
fn range(v: f64, min: f64, max: f64) -> bool {
    v.is_finite() && (min..=max).contains(&v)
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && !s.contains('\0') && s.chars().count() <= max
}
fn sources(ids: &[Uuid]) -> bool {
    !ids.is_empty()
        && ids.len() <= 32
        && !ids.contains(&Uuid::nil())
        && ids.iter().copied().collect::<HashSet<_>>().len() == ids.len()
}
fn target(id: Option<Uuid>, available: &HashSet<Uuid>) -> bool {
    id.is_none_or(|id| !id.is_nil() && available.contains(&id))
}
pub fn attention_weight(
    goal_relevance: f64,
    expectation_gap: f64,
    uncertainty: f64,
) -> Result<f64, AffectError> {
    if !range(goal_relevance, 0., 1.)
        || !range(expectation_gap, -1., 1.)
        || !range(uncertainty, 0., 1.)
    {
        return Err(AffectError::Invalid);
    }
    Ok(
        (goal_relevance * (0.5 + 0.5 * expectation_gap.abs()) * (0.5 + 0.5 * uncertainty))
            .clamp(0., 1.),
    )
}
#[derive(Debug)]
pub struct ValidatedAppraisal {
    individual_id: Uuid,
    draft: AppraisalDraft,
}
impl ValidatedAppraisal {
    pub fn individual_id(&self) -> Uuid {
        self.individual_id
    }
    pub fn draft(&self) -> &AppraisalDraft {
        &self.draft
    }
}
impl AppraisalDraft {
    pub fn validate_shape(&self) -> Result<(), AffectError> {
        if !text(&self.brief_basis, 1000)
            || !range(self.valence, -1., 1.)
            || !range(self.expectation_gap, -1., 1.)
            || [
                self.arousal,
                self.goal_relevance,
                self.controllability,
                self.uncertainty,
            ]
            .iter()
            .any(|v| !range(*v, 0., 1.))
        {
            return Err(AffectError::Invalid);
        }
        if !sources(&self.event_ids) {
            return Err(AffectError::Evidence);
        }
        Ok(())
    }
    pub fn validate(
        self,
        individual_id: Uuid,
        events: &HashSet<Uuid>,
        goals: &HashSet<Uuid>,
        subjects: &HashSet<Uuid>,
    ) -> Result<ValidatedAppraisal, AffectError> {
        self.validate_shape()?;
        if individual_id.is_nil() || !self.event_ids.iter().all(|id| events.contains(id)) {
            return Err(AffectError::Evidence);
        }
        if !target(self.goal_id, goals) || !target(self.subject_id, subjects) {
            return Err(AffectError::Target);
        }
        Ok(ValidatedAppraisal {
            individual_id,
            draft: self,
        })
    }
}
#[derive(Debug)]
pub struct ValidatedAttitude {
    individual_id: Uuid,
    draft: AttitudeDraft,
    expected_previous: Option<(Uuid, i64)>,
}
impl ValidatedAttitude {
    pub fn individual_id(&self) -> Uuid {
        self.individual_id
    }
    pub fn draft(&self) -> &AttitudeDraft {
        &self.draft
    }
    pub fn expected_previous(&self) -> Option<(Uuid, i64)> {
        self.expected_previous
    }
}
impl AttitudeDraft {
    pub fn validate_shape(&self) -> Result<(), AffectError> {
        let t = self.topic.as_bytes();
        if t.is_empty()
            || t.len() > 80
            || !t[0].is_ascii_lowercase()
            || !t
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(b))
            || !text(&self.stance, 2000)
            || !text(&self.brief_basis, 1000)
        {
            return Err(AffectError::Invalid);
        }
        if !sources(&self.event_ids) {
            return Err(AffectError::Evidence);
        }
        Ok(())
    }
    pub fn validate(
        self,
        individual_id: Uuid,
        events: &HashSet<Uuid>,
        subjects: &HashSet<Uuid>,
        previous: Option<&PreviousAttitude>,
    ) -> Result<ValidatedAttitude, AffectError> {
        self.validate_shape()?;
        if individual_id.is_nil() || !self.event_ids.iter().all(|id| events.contains(id)) {
            return Err(AffectError::Evidence);
        }
        if !target(self.subject_id, subjects) {
            return Err(AffectError::Target);
        }
        let expected_previous = match (self.supersedes, previous) {
            (None, None) => None,
            (Some(id), Some(old))
                if !id.is_nil()
                    && id == old.id
                    && old.individual_id == individual_id
                    && old.topic == self.topic
                    && old.subject_id == self.subject_id
                    && old.version > 0
                    && !old.superseded =>
            {
                Some((id, old.version))
            }
            _ => return Err(AffectError::Previous),
        };
        Ok(ValidatedAttitude {
            individual_id,
            draft: self,
            expected_previous,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn appraisal(event: Uuid) -> AppraisalDraft {
        AppraisalDraft {
            event_ids: vec![event],
            goal_id: None,
            subject_id: None,
            valence: 0.,
            arousal: 0.5,
            goal_relevance: 0.4,
            expectation_gap: -0.5,
            controllability: 0.5,
            uncertainty: 0.5,
            brief_basis: "由同一实际输入形成的有限评价".into(),
        }
    }
    fn attitude(event: Uuid) -> AttitudeDraft {
        AttitudeDraft {
            topic: "project.cooperation".into(),
            subject_id: None,
            stance: "愿意继续探索，但保留对依据的检查".into(),
            event_ids: vec![event],
            supersedes: None,
            brief_basis: "基于一次实际活动的可修订倾向".into(),
        }
    }
    #[test]
    fn same_event_and_distinct_goal_relevance_produce_bounded_attention() {
        let e = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let g1 = Uuid::new_v4();
        let g2 = Uuid::new_v4();
        let mut low = appraisal(e);
        low.goal_id = Some(g1);
        low.goal_relevance = 0.2;
        let mut high = low.clone();
        high.goal_id = Some(g2);
        high.goal_relevance = 0.8;
        let events = HashSet::from([e]);
        let goals = HashSet::from([g1, g2]);
        let low = low
            .validate(owner, &events, &goals, &HashSet::new())
            .unwrap();
        let high = high
            .validate(owner, &events, &goals, &HashSet::new())
            .unwrap();
        let weight = |v: &ValidatedAppraisal| {
            attention_weight(
                v.draft().goal_relevance,
                v.draft().expectation_gap,
                v.draft().uncertainty,
            )
            .unwrap()
        };
        assert!(weight(&high) > weight(&low));
        assert!((weight(&high) - 0.45).abs() < 1e-12);
        assert_eq!(attention_weight(1., -1., 1.).unwrap(), 1.);
        assert_eq!(attention_weight(0., 1., 1.).unwrap(), 0.);
    }
    #[test]
    fn nonfinite_and_out_of_range_dimensions_never_become_state() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1.01, -1.01] {
            let mut a = appraisal(Uuid::new_v4());
            a.valence = value;
            assert_eq!(a.validate_shape(), Err(AffectError::Invalid));
        }
        for field in 0..4 {
            let mut a = appraisal(Uuid::new_v4());
            match field {
                0 => a.arousal = -0.01,
                1 => a.goal_relevance = 1.01,
                2 => a.controllability = f64::NAN,
                _ => a.uncertainty = f64::INFINITY,
            };
            assert_eq!(a.validate_shape(), Err(AffectError::Invalid));
        }
        assert!(attention_weight(f64::NAN, 0., 0.).is_err());
    }
    #[test]
    fn events_and_actual_targets_are_required_and_unknown_fields_rejected() {
        let e = Uuid::new_v4();
        let mut a = appraisal(e);
        a.subject_id = Some(Uuid::new_v4());
        assert!(matches!(
            a.clone().validate(
                Uuid::new_v4(),
                &HashSet::from([e]),
                &HashSet::new(),
                &HashSet::new()
            ),
            Err(AffectError::Target)
        ));
        a.subject_id = None;
        a.event_ids.push(e);
        assert_eq!(a.validate_shape(), Err(AffectError::Evidence));
        let mut v = serde_json::to_value(appraisal(e)).unwrap();
        v["permissions"] = serde_json::json!("expanded");
        assert!(serde_json::from_value::<AppraisalDraft>(v).is_err());
    }
    #[test]
    fn attitude_revision_requires_same_subject_topic_owner_and_positive_current_version() {
        let owner = Uuid::new_v4();
        let event = Uuid::new_v4();
        let id = Uuid::new_v4();
        let subject = Uuid::new_v4();
        let mut d = attitude(event);
        d.subject_id = Some(subject);
        d.supersedes = Some(id);
        let valid = PreviousAttitude {
            individual_id: owner,
            id,
            topic: d.topic.clone(),
            subject_id: Some(subject),
            version: 4,
            superseded: false,
        };
        let saved = d
            .clone()
            .validate(
                owner,
                &HashSet::from([event]),
                &HashSet::from([subject]),
                Some(&valid),
            )
            .unwrap();
        assert_eq!(saved.expected_previous(), Some((id, 4)));
        for change in 0..5 {
            let mut previous = valid.clone();
            match change {
                0 => previous.individual_id = Uuid::new_v4(),
                1 => previous.topic = "other.topic".into(),
                2 => previous.subject_id = None,
                3 => previous.version = 0,
                _ => previous.superseded = true,
            };
            assert!(matches!(
                d.clone().validate(
                    owner,
                    &HashSet::from([event]),
                    &HashSet::from([subject]),
                    Some(&previous)
                ),
                Err(AffectError::Previous)
            ));
        }
    }
    #[test]
    fn short_state_and_attitude_have_no_absence_decay_or_personality_fields() {
        let e = Uuid::new_v4();
        let mut d = attitude(e);
        d.stance = "我".repeat(2000);
        assert!(d.validate_shape().is_ok());
        d.stance.push('们');
        assert_eq!(d.validate_shape(), Err(AffectError::Invalid));
        for key in ["absence_count", "decay", "personality", "expires_at"] {
            let mut v = serde_json::to_value(attitude(e)).unwrap();
            v[key] = serde_json::json!(1);
            assert!(serde_json::from_value::<AttitudeDraft>(v).is_err());
        }
        let mut empty = attitude(e);
        empty.event_ids.clear();
        assert_eq!(empty.validate_shape(), Err(AffectError::Evidence));
        assert_eq!(APPRAISAL_LIFETIME_SECONDS, 3600);
    }
}
