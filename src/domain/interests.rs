//! Stable interest identity. Descriptions and status are revisable, not array IDs.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterestStatus {
    Active,
    Retired,
}
impl InterestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retired => "retired",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterestDraft {
    pub id: Option<String>,
    pub expected_version: Option<i64>,
    pub description: String,
    pub status: InterestStatus,
    pub event_ids: Vec<Uuid>,
    pub brief_basis: String,
}
#[derive(Debug, Clone)]
pub struct CurrentInterest {
    pub individual_id: Uuid,
    pub id: String,
    pub object_id: Uuid,
    pub version: i64,
    pub description: String,
    pub status: InterestStatus,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InterestError {
    #[error("invalid interest fields")]
    Invalid,
    #[error("interest requires actual available source events")]
    Evidence,
    #[error("interest identity or version does not match current state")]
    Current,
    #[error("interest revision has no change")]
    NoChange,
}
#[derive(Debug)]
pub struct ValidatedInterest {
    individual_id: Uuid,
    draft: InterestDraft,
    previous: Option<CurrentInterest>,
}
impl ValidatedInterest {
    pub fn individual_id(&self) -> Uuid {
        self.individual_id
    }
    pub fn draft(&self) -> &InterestDraft {
        &self.draft
    }
    pub fn previous(&self) -> Option<&CurrentInterest> {
        self.previous.as_ref()
    }
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && !s.contains('\0') && s.chars().count() <= max
}
impl InterestDraft {
    pub fn validate_shape(&self) -> Result<(), InterestError> {
        if !text(&self.description, 2000) || !text(&self.brief_basis, 1000) {
            return Err(InterestError::Invalid);
        }
        match (&self.id, self.expected_version) {
            (None, None) if self.status == InterestStatus::Active => {}
            (Some(id), Some(v)) if id.starts_with("interest:") && text(id, 160) && v > 0 => {}
            _ => return Err(InterestError::Invalid),
        }
        let events: HashSet<_> = self.event_ids.iter().copied().collect();
        if events.is_empty()
            || events.len() > 32
            || events.len() != self.event_ids.len()
            || events.contains(&Uuid::nil())
        {
            return Err(InterestError::Evidence);
        }
        Ok(())
    }
    pub fn validate(
        self,
        individual_id: Uuid,
        events: &HashSet<Uuid>,
        current: Option<&CurrentInterest>,
    ) -> Result<ValidatedInterest, InterestError> {
        self.validate_shape()?;
        if individual_id.is_nil() || !self.event_ids.iter().all(|id| events.contains(id)) {
            return Err(InterestError::Evidence);
        }
        match (&self.id, self.expected_version, current) {
            (None, None, None) => {}
            (Some(id), Some(version), Some(old))
                if old.individual_id == individual_id
                    && old.id == *id
                    && !old.object_id.is_nil()
                    && old.version == version
                    && version > 0 =>
            {
                if old.description == self.description && old.status == self.status {
                    return Err(InterestError::NoChange);
                }
            }
            _ => return Err(InterestError::Current),
        }
        Ok(ValidatedInterest {
            individual_id,
            draft: self,
            previous: current.cloned(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn draft(e: Uuid) -> InterestDraft {
        InterestDraft {
            id: None,
            expected_version: None,
            description: "可验证的问题".into(),
            status: InterestStatus::Active,
            event_ids: vec![e],
            brief_basis: "这次实际输入形成的可修订兴趣".into(),
        }
    }
    #[test]
    fn new_identity_is_program_assigned_and_cannot_start_retired() {
        let e = Uuid::new_v4();
        let mut d = draft(e);
        d.id = Some(format!("interest:{}", Uuid::new_v4()));
        assert_eq!(d.validate_shape(), Err(InterestError::Invalid));
        d.id = None;
        d.status = InterestStatus::Retired;
        assert_eq!(d.validate_shape(), Err(InterestError::Invalid));
    }
    #[test]
    fn relabel_retire_and_reactivate_keep_stable_legacy_identity() {
        let owner = Uuid::new_v4();
        let e = Uuid::new_v4();
        let old = CurrentInterest {
            individual_id: owner,
            id: format!("interest:{owner}:0"),
            object_id: Uuid::new_v4(),
            version: 4,
            description: "旧描述".into(),
            status: InterestStatus::Active,
        };
        let mut d = draft(e);
        d.id = Some(old.id.clone());
        d.expected_version = Some(4);
        d.status = InterestStatus::Retired;
        let v = d.validate(owner, &HashSet::from([e]), Some(&old)).unwrap();
        assert_eq!(v.previous().unwrap().object_id, old.object_id);
        assert_eq!(v.draft().id.as_ref(), Some(&old.id));
        assert_eq!(old.status, InterestStatus::Active);
    }
    #[test]
    fn wrong_owner_stale_version_and_noop_do_not_produce_changes() {
        let owner = Uuid::new_v4();
        let e = Uuid::new_v4();
        let old = CurrentInterest {
            individual_id: owner,
            id: format!("interest:{}", Uuid::new_v4()),
            object_id: Uuid::new_v4(),
            version: 2,
            description: "可验证的问题".into(),
            status: InterestStatus::Active,
        };
        let mut d = draft(e);
        d.id = Some(old.id.clone());
        d.expected_version = Some(2);
        assert!(matches!(
            d.clone().validate(owner, &HashSet::from([e]), Some(&old)),
            Err(InterestError::NoChange)
        ));
        d.description = "新描述".into();
        assert!(matches!(
            d.clone()
                .validate(Uuid::new_v4(), &HashSet::from([e]), Some(&old)),
            Err(InterestError::Current)
        ));
        d.expected_version = Some(1);
        assert!(matches!(
            d.validate(owner, &HashSet::from([e]), Some(&old)),
            Err(InterestError::Current)
        ));
    }
    #[test]
    fn fields_are_closed_and_unicode_limits_are_scalar_based() {
        let e = Uuid::new_v4();
        let mut d = draft(e);
        d.description = "我".repeat(2000);
        assert!(d.validate_shape().is_ok());
        d.description.push('们');
        assert_eq!(d.validate_shape(), Err(InterestError::Invalid));
        let mut v = serde_json::to_value(draft(e)).unwrap();
        v["personality"] = serde_json::json!("overwrite");
        assert!(serde_json::from_value::<InterestDraft>(v).is_err());
    }
    #[test]
    fn source_identity_is_distinct_from_the_interest_and_must_be_available() {
        let e = Uuid::new_v4();
        assert!(matches!(
            draft(e).validate(Uuid::new_v4(), &HashSet::new(), None),
            Err(InterestError::Evidence)
        ));
        let mut d = draft(e);
        d.event_ids.push(e);
        assert_eq!(d.validate_shape(), Err(InterestError::Evidence));
    }
}

/// Current interpretation plus its complete, immutable revision chain. A chain is
/// history of saved interests, not independent evidence about external events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterestRevisionView {
    pub id: Uuid,
    pub before_version: i64,
    pub after_version: i64,
    pub before_description: Option<String>,
    pub before_status: Option<InterestStatus>,
    pub after_description: String,
    pub after_status: InterestStatus,
    pub brief_basis: String,
    pub event_id: Uuid,
    pub previous_revision_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentInterestView {
    pub id: String,
    pub object_id: Uuid,
    pub version: i64,
    pub description: String,
    pub status: InterestStatus,
    pub origin_kind: String,
    pub event_ids: Vec<Uuid>,
    pub provenance_ids: Vec<Uuid>,
    pub basis_event_ids: Vec<Uuid>,
    pub revisions: Vec<InterestRevisionView>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterestContext {
    pub items: Vec<CurrentInterestView>,
    pub has_more: bool,
}
impl CurrentInterestView {
    pub fn validate_shape(&self) -> bool {
        let unique = |v: &[Uuid]| {
            !v.contains(&Uuid::nil()) && v.iter().collect::<HashSet<_>>().len() == v.len()
        };
        if !self.id.starts_with("interest:")
            || !text(&self.id, 160)
            || self.object_id.is_nil()
            || self.version <= 0
            || !text(&self.description, 2000)
            || !matches!(self.origin_kind.as_str(), "initial_seed" | "experience")
            || self.event_ids.is_empty()
            || self.provenance_ids.len() > 1024
            || !unique(&self.event_ids)
            || !unique(&self.provenance_ids)
            || !unique(&self.basis_event_ids)
            || !self.provenance_ids.contains(&self.object_id)
            || self
                .event_ids
                .iter()
                .any(|id| !self.provenance_ids.contains(id) || !self.basis_event_ids.contains(id))
            || self
                .basis_event_ids
                .iter()
                .any(|id| !self.provenance_ids.contains(id))
        {
            return false;
        }
        let mut version = if self.origin_kind == "initial_seed" {
            1
        } else {
            0
        };
        let mut previous = None;
        let mut state: Option<(&str, InterestStatus)> = None;
        let mut seen = HashSet::new();
        for r in &self.revisions {
            if r.id.is_nil()
                || !seen.insert(r.id)
                || r.before_version != version
                || version.checked_add(1) != Some(r.after_version)
                || r.previous_revision_id != previous
                || !self.provenance_ids.contains(&r.id)
                || !self.basis_event_ids.contains(&r.event_id)
                || !text(&r.after_description, 2000)
                || !text(&r.brief_basis, 1000)
                || (version == 0 && (r.before_description.is_some() || r.before_status.is_some()))
                || (version > 0
                    && (r
                        .before_description
                        .as_deref()
                        .is_none_or(|s| !text(s, 2000))
                        || r.before_status.is_none()))
                || state.is_some_and(|(description, status)| {
                    r.before_description.as_deref() != Some(description)
                        || r.before_status != Some(status)
                })
            {
                return false;
            }
            version = r.after_version;
            previous = Some(r.id);
            state = Some((&r.after_description, r.after_status));
        }
        version == self.version
            && state.is_none_or(|(d, s)| d == self.description && s == self.status)
            && (self.origin_kind != "experience" || !self.revisions.is_empty())
    }
}
impl InterestContext {
    pub fn valid_for(&self, c: &crate::domain::cognition::ModelContext) -> bool {
        let manifest: HashSet<_> = c.input_manifest.iter().copied().collect();
        let events: HashSet<_> = c
            .events
            .iter()
            .filter(|e| e.individual_id == c.individual_id)
            .map(|e| e.id)
            .collect();
        let mut ids = HashSet::new();
        let mut objects = HashSet::new();
        self.items.len() <= 32
            && self.items.iter().all(|v| {
                v.validate_shape()
                    && ids.insert(&v.id)
                    && objects.insert(v.object_id)
                    && v.provenance_ids.iter().all(|id| manifest.contains(id))
                    && v.basis_event_ids.iter().all(|id| events.contains(id))
            })
    }
}
impl InterestDraft {
    pub fn validate_context<'a>(
        &self,
        c: &'a crate::domain::cognition::ModelContext,
    ) -> Result<Option<&'a CurrentInterestView>, InterestError> {
        self.validate_shape()?;
        let events: HashSet<_> = c
            .events
            .iter()
            .filter(|e| {
                e.individual_id == c.individual_id
                    && e.source_kind != crate::domain::history::SourceKind::FictionalBackground
                    && e.kind != "decision.reviewed"
            })
            .map(|e| e.id)
            .collect();
        if self
            .event_ids
            .iter()
            .any(|id| !events.contains(id) || !c.input_manifest.contains(id))
        {
            return Err(InterestError::Evidence);
        }
        let Some(id) = &self.id else { return Ok(None) };
        let selection = c
            .interest_context
            .as_ref()
            .filter(|s| s.valid_for(c))
            .ok_or(InterestError::Current)?;
        let current = selection
            .items
            .iter()
            .find(|v| &v.id == id && Some(v.version) == self.expected_version)
            .ok_or(InterestError::Current)?;
        if current.description == self.description && current.status == self.status {
            return Err(InterestError::NoChange);
        }
        Ok(Some(current))
    }
}
