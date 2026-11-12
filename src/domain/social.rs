//! Source-bound self-appraisals and relationship interpretations, not feelings
//! attributed to someone else or evidence of subjective consciousness.
use super::{
    affect::{AppraisalDraft, AttitudeDraft, PreviousAttitude},
    cognition::ModelContext,
    relationships::{
        CurrentRelationship, PreviousRelationshipRevision, RelationshipRevisionDraft,
        RelationshipState,
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentAppraisal {
    pub id: Uuid,
    pub draft: AppraisalDraft,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub attention_weight: f64,
    pub provenance_ids: Vec<Uuid>,
    pub basis_event_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentAttitude {
    pub id: Uuid,
    pub version: i64,
    pub draft: AttitudeDraft,
    pub provenance_ids: Vec<Uuid>,
    pub basis_event_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentRelation {
    pub subject_id: Uuid,
    pub version: i64,
    pub state: RelationshipState,
    pub latest_revision_id: Option<Uuid>,
    pub latest_revision_version: Option<i64>,
    pub provenance_ids: Vec<Uuid>,
    pub basis_event_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SocialContext {
    pub appraisal: Option<CurrentAppraisal>,
    pub attitudes: Vec<CurrentAttitude>,
    pub relationships: Vec<CurrentRelation>,
    pub has_more: bool,
}
impl SocialContext {
    pub fn ids(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.appraisal
            .iter()
            .map(|a| a.id)
            .chain(self.attitudes.iter().map(|a| a.id))
            .chain(self.relationships.iter().map(|r| r.subject_id))
    }
    /// The source material remains typed and complete. A saved interpretation
    /// supports a statement about this individual's view, not another's feelings.
    pub fn evidence(&self, c: &ModelContext, id: Uuid) -> Option<serde_json::Value> {
        let (record, ids) = if let Some(a) = self.appraisal.as_ref().filter(|a| a.id == id) {
            (serde_json::to_value(a).ok()?, &a.basis_event_ids)
        } else if let Some(a) = self.attitudes.iter().find(|a| a.id == id) {
            (serde_json::to_value(a).ok()?, &a.basis_event_ids)
        } else {
            let r = self.relationships.iter().find(|r| r.subject_id == id)?;
            (
                serde_json::json!({"id":id,"relationship":r}),
                &r.basis_event_ids,
            )
        };
        let basis = ids
            .iter()
            .map(|id| c.events.iter().find(|e| e.id == *id))
            .collect::<Option<Vec<_>>>()?;
        Some(
            serde_json::json!({"type":"social_interpretation","proof_scope":"individual_appraisal_attitude_or_relationship_not_world_fact_or_others_feelings","record":record,"basis_events":basis}),
        )
    }
    pub fn valid_for(&self, c: &ModelContext) -> bool {
        let manifest: HashSet<_> = c.input_manifest.iter().copied().collect();
        let events: HashSet<_> = c
            .events
            .iter()
            .filter(|e| e.individual_id == c.individual_id)
            .map(|e| e.id)
            .collect();
        let actual = actual_events(c);
        let bundle = |id: Uuid, sources: &[Uuid], basis: &[Uuid]| {
            let ids: HashSet<_> = sources.iter().copied().collect();
            let event_ids: HashSet<_> = basis.iter().copied().collect();
            !id.is_nil()
                && ids.len() <= 1024
                && ids.len() == sources.len()
                && ids.contains(&id)
                && !ids.contains(&Uuid::nil())
                && ids.is_subset(&manifest)
                && event_ids.len() == basis.len()
                && event_ids.is_subset(&events)
                && event_ids.is_subset(&ids)
        };
        let mut seen = HashSet::new();
        self.attitudes.len() <= 4
            && self.relationships.len() <= 4
            && self.appraisal.as_ref().is_none_or(|a| {
                let expected = super::affect::attention_weight(
                    a.draft.goal_relevance,
                    a.draft.expectation_gap,
                    a.draft.uncertainty,
                );
                a.draft.validate_shape().is_ok()
                    && a.created_at <= c.now
                    && a.expires_at > c.now
                    && a.expires_at > a.created_at
                    && a.attention_weight.is_finite()
                    && expected.is_ok_and(|v| (v - a.attention_weight).abs() < 1e-9)
                    && bundle(a.id, &a.provenance_ids, &a.basis_event_ids)
                    && a.draft
                        .event_ids
                        .iter()
                        .all(|id| actual.contains(id) && a.provenance_ids.contains(id))
                    && a.draft
                        .goal_id
                        .iter()
                        .chain(a.draft.subject_id.iter())
                        .all(|id| a.provenance_ids.contains(id))
            })
            && self.attitudes.iter().all(|a| {
                seen.insert(a.id)
                    && a.version > 0
                    && a.draft.validate_shape().is_ok()
                    && bundle(a.id, &a.provenance_ids, &a.basis_event_ids)
                    && a.draft
                        .event_ids
                        .iter()
                        .all(|id| actual.contains(id) && a.provenance_ids.contains(id))
                    && a.draft
                        .subject_id
                        .iter()
                        .chain(a.draft.supersedes.iter())
                        .all(|id| a.provenance_ids.contains(id))
            })
            && self.relationships.iter().all(|r| {
                seen.insert(r.subject_id)
                    && r.version > 0
                    && r.state.validate().is_ok()
                    && bundle(r.subject_id, &r.provenance_ids, &r.basis_event_ids)
                    && match (r.latest_revision_id, r.latest_revision_version) {
                        (None, None) => true,
                        (Some(id), Some(v)) => {
                            v > 0 && v <= r.version && r.provenance_ids.contains(&id)
                        }
                        _ => false,
                    }
            })
    }
    pub fn current_at(&self, at: DateTime<Utc>) -> bool {
        self.appraisal
            .as_ref()
            .is_none_or(|a| a.created_at <= at && at < a.expires_at)
    }
}
fn actual_events(c: &ModelContext) -> HashSet<Uuid> {
    c.events
        .iter()
        .filter(|e| {
            e.individual_id == c.individual_id
                && e.source_kind != super::history::SourceKind::FictionalBackground
                && e.kind != "decision.reviewed"
        })
        .map(|e| e.id)
        .collect()
}
impl AppraisalDraft {
    pub fn validate_context(
        &self,
        c: &ModelContext,
    ) -> Result<super::affect::ValidatedAppraisal, super::affect::AffectError> {
        let subjects = c
            .social_context
            .iter()
            .flat_map(|s| &s.relationships)
            .map(|r| r.subject_id)
            .collect();
        self.clone().validate(
            c.individual_id,
            &actual_events(c),
            &c.goals.iter().map(|g| g.id).collect(),
            &subjects,
        )
    }
}
impl AttitudeDraft {
    pub fn validate_context(
        &self,
        c: &ModelContext,
    ) -> Result<super::affect::ValidatedAttitude, super::affect::AffectError> {
        let subjects = c
            .social_context
            .iter()
            .flat_map(|s| &s.relationships)
            .map(|r| r.subject_id)
            .collect();
        let previous = match self.supersedes {
            None => None,
            Some(id) => {
                let old = c
                    .social_context
                    .iter()
                    .flat_map(|s| &s.attitudes)
                    .find(|a| a.id == id)
                    .ok_or(super::affect::AffectError::Previous)?;
                Some(PreviousAttitude {
                    individual_id: c.individual_id,
                    id,
                    topic: old.draft.topic.clone(),
                    subject_id: old.draft.subject_id,
                    version: old.version,
                    superseded: false,
                })
            }
        };
        if self.supersedes.is_none()
            && c.social_context
                .iter()
                .flat_map(|s| &s.attitudes)
                .any(|a| a.draft.topic == self.topic)
        {
            return Err(super::affect::AffectError::Previous);
        }
        self.clone().validate(
            c.individual_id,
            &actual_events(c),
            &subjects,
            previous.as_ref(),
        )
    }
}
impl RelationshipRevisionDraft {
    pub fn validate_context(
        &self,
        c: &ModelContext,
    ) -> Result<
        super::relationships::ValidatedRelationshipRevision,
        super::relationships::RelationshipRevisionError,
    > {
        let old = c
            .social_context
            .iter()
            .flat_map(|s| &s.relationships)
            .find(|r| r.subject_id == self.subject_id)
            .ok_or(super::relationships::RelationshipRevisionError::Target)?;
        let current = CurrentRelationship {
            individual_id: c.individual_id,
            subject_id: old.subject_id,
            version: old.version,
            state: old.state.clone(),
        };
        let previous =
            old.latest_revision_id
                .zip(old.latest_revision_version)
                .map(|(id, after_version)| PreviousRelationshipRevision {
                    individual_id: c.individual_id,
                    subject_id: old.subject_id,
                    id,
                    after_version,
                });
        self.clone().validate(
            c.individual_id,
            &current,
            &actual_events(c),
            previous.as_ref(),
        )
    }
}

/// Small, deterministic attention modulation. It does not change persisted goal
/// priorities or override the run's current goal, permissions, facts or budgets.
pub fn order_attention(c: &mut ModelContext) {
    let appraisal = c.social_context.as_ref().and_then(|s| s.appraisal.as_ref());
    let goal = appraisal.and_then(|a| a.draft.goal_id);
    let weight = appraisal.map_or(0., |a| a.attention_weight * 10.);
    let current = c.current_goal_id;
    c.goals.sort_by(|a, b| {
        (b.id == current.unwrap_or(Uuid::nil()))
            .cmp(&(a.id == current.unwrap_or(Uuid::nil())))
            .then_with(|| {
                (f64::from(b.priority) + if Some(b.id) == goal { weight } else { 0. }).total_cmp(
                    &(f64::from(a.priority) + if Some(a.id) == goal { weight } else { 0. }),
                )
            })
    });
}
