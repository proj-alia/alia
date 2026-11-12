//! Executable development policy. Database ownership and real review provenance
//! remain the trusted adapter's responsibility, never a deserialized model flag.
use super::config::PersonaSeed;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevelopmentTarget {
    #[serde(rename = "persona.traits")]
    Traits,
    #[serde(rename = "persona.values")]
    Values,
    #[serde(rename = "persona.expression")]
    Expression,
    #[serde(rename = "persona.cognitive_preferences")]
    CognitivePreferences,
    #[serde(rename = "task_strategy")]
    TaskStrategy,
}
impl DevelopmentTarget {
    pub fn persona_pointer(self) -> Option<&'static str> {
        match self {
            Self::Traits => Some("/personality/traits"),
            Self::Values => Some("/personality/values"),
            Self::Expression => Some("/expression"),
            Self::CognitivePreferences => Some("/personality/cognitive_preferences"),
            Self::TaskStrategy => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum StrategyScope {
    Goal(Uuid),
    Task(Uuid),
}
impl StrategyScope {
    fn valid(&self) -> bool {
        match self {
            Self::Goal(id) | Self::Task(id) => !id.is_nil(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStrategy {
    pub scope: StrategyScope,
    pub description: String,
}
impl TaskStrategy {
    fn validate(&self) -> Result<(), DevelopmentError> {
        if !self.scope.valid() || !text(&self.description, 2000) {
            return Err(DevelopmentError::Invalid);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentDraft {
    pub target: DevelopmentTarget,
    pub before_version: i64,
    pub proposed_value: Value,
    pub proposed_description: String,
    pub evidence_ids: Vec<Uuid>,
    pub counterevidence_ids: Vec<Uuid>,
    pub validation_plan: String,
    pub deadline: DateTime<Utc>,
}
/// Observed proposal metadata, never a trusted authorization or persona value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentTargetView {
    pub target: DevelopmentTarget,
    pub scope: Option<StrategyScope>,
    pub before_version: i64,
    pub target_id: Option<Uuid>,
    pub head_revision_id: Option<Uuid>,
}
/// Trusted current target head; a global state version is checked separately by storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSnapshot {
    pub target: DevelopmentTarget,
    pub version: i64,
    pub head_revision_id: Option<Uuid>,
    pub value: Value,
    pub scope: Option<StrategyScope>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DevelopmentError {
    #[error("invalid development fields or target scope")]
    Invalid,
    #[error("candidate does not change its permitted target")]
    NoChange,
    #[error("candidate does not satisfy the unchanged complete persona schema")]
    Persona,
    #[error("development deadline expired or expiry requested before deadline")]
    Deadline,
    #[error("development transition is not allowed")]
    Transition,
    #[error("evidence is incomplete, unavailable or only transient/self-reported")]
    Evidence,
    #[error("review is not independent or not bound to this candidate and trial")]
    Review,
    #[error("independent validation is incomplete or contains a failure")]
    Unverified,
    #[error("a later target revision conflicts with this operation")]
    Conflict,
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && !s.contains('\0') && s.chars().count() <= max
}
fn ids(v: &[Uuid], min: usize, max: usize) -> bool {
    v.len() >= min
        && v.len() <= max
        && !v.contains(&Uuid::nil())
        && v.iter().copied().collect::<HashSet<_>>().len() == v.len()
}
fn hash(v: &impl Serialize) -> Result<[u8; 32], DevelopmentError> {
    Ok(Sha256::digest(serde_json::to_vec(v).map_err(|_| DevelopmentError::Invalid)?).into())
}
fn apply_value(
    target: DevelopmentTarget,
    scope: &Option<StrategyScope>,
    value: &Value,
    persona: &Value,
) -> Result<ApplicationValue, DevelopmentError> {
    if let Some(pointer) = target.persona_pointer() {
        if scope.is_some() {
            return Err(DevelopmentError::Invalid);
        }
        PersonaSeed::validate(persona.clone()).map_err(|_| DevelopmentError::Persona)?;
        let mut after = persona.clone();
        *after
            .pointer_mut(pointer)
            .ok_or(DevelopmentError::Persona)? = value.clone();
        PersonaSeed::validate(after.clone()).map_err(|_| DevelopmentError::Persona)?;
        Ok(ApplicationValue::Persona(after))
    } else {
        let scope = scope
            .as_ref()
            .filter(|s| s.valid())
            .ok_or(DevelopmentError::Invalid)?;
        let strategy = if value.is_null() {
            None
        } else {
            let s: TaskStrategy =
                serde_json::from_value(value.clone()).map_err(|_| DevelopmentError::Invalid)?;
            s.validate()?;
            if &s.scope != scope {
                return Err(DevelopmentError::Conflict);
            }
            Some(s)
        };
        Ok(ApplicationValue::TaskStrategy {
            scope: scope.clone(),
            strategy,
        })
    }
}
#[derive(Debug, Clone)]
pub struct DevelopmentCandidate {
    id: Uuid,
    individual_id: Uuid,
    proposer_id: Uuid,
    draft: DevelopmentDraft,
    before: TargetSnapshot,
    prepared_at: DateTime<Utc>,
    binding: [u8; 32],
}
impl DevelopmentCandidate {
    pub fn prepare(
        id: Uuid,
        individual_id: Uuid,
        proposer_id: Uuid,
        draft: DevelopmentDraft,
        current: &TargetSnapshot,
        current_persona: &Value,
        now: DateTime<Utc>,
    ) -> Result<Self, DevelopmentError> {
        if id.is_nil()
            || individual_id.is_nil()
            || proposer_id.is_nil()
            || draft.before_version <= 0
            || !text(&draft.proposed_description, 2000)
            || !text(&draft.validation_plan, 2000)
            || current.head_revision_id.is_some_and(|v| v.is_nil())
        {
            return Err(DevelopmentError::Invalid);
        }
        if draft.deadline <= now {
            return Err(DevelopmentError::Deadline);
        }
        if current.target != draft.target || current.version != draft.before_version {
            return Err(DevelopmentError::Conflict);
        }
        if !ids(&draft.evidence_ids, 1, 32)
            || !ids(&draft.counterevidence_ids, 0, 32)
            || draft
                .evidence_ids
                .iter()
                .any(|v| draft.counterevidence_ids.contains(v))
        {
            return Err(DevelopmentError::Evidence);
        }
        if let Some(pointer) = draft.target.persona_pointer() {
            if current.scope.is_some() || current_persona.pointer(pointer) != Some(&current.value) {
                return Err(DevelopmentError::Conflict);
            }
        } else {
            apply_value(
                draft.target,
                &current.scope,
                &current.value,
                current_persona,
            )?;
            if draft.proposed_value.is_null() {
                return Err(DevelopmentError::Invalid);
            }
        }
        apply_value(
            draft.target,
            &current.scope,
            &draft.proposed_value,
            current_persona,
        )?;
        if current.value == draft.proposed_value {
            return Err(DevelopmentError::NoChange);
        }
        let binding = hash(
            &json!({"id":id,"individual_id":individual_id,"proposer_id":proposer_id,"draft":draft,"before_value":current.value,"before_head":current.head_revision_id,"scope":current.scope}),
        )?;
        Ok(Self {
            id,
            individual_id,
            proposer_id,
            draft,
            before: current.clone(),
            prepared_at: now,
            binding,
        })
    }
    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn individual_id(&self) -> Uuid {
        self.individual_id
    }
    pub fn proposer_id(&self) -> Uuid {
        self.proposer_id
    }
    pub fn draft(&self) -> &DevelopmentDraft {
        &self.draft
    }
    pub fn before(&self) -> &TargetSnapshot {
        &self.before
    }
}

// These are server-loaded facts/measurements, deliberately NOT Deserialize.
// The trusted adapter must verify actual ownership, records, source class and
// reviewer independence before supplying them; this module does not query a DB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SourceClass {
    ObservedOutcome,
    ExternalReport,
    SignificantEvent,
    ModelSelfReport,
    TransientAffect,
    InitialSetting,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServerSource {
    pub id: Uuid,
    pub individual_id: Uuid,
    pub class: SourceClass,
    pub recorded_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ValidationRoute {
    CrossScenarioHabit,
    SignificantEvent { event_id: Uuid },
}
#[derive(Debug)]
pub struct TrialAuthorization {
    binding: [u8; 32],
    route: ValidationRoute,
    sources: Vec<ServerSource>,
    issued_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ReviewOrigin {
    IndependentHuman,
    IndependentMeasurement,
    AuthorSelfAssessment,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Passed,
    Failed,
    Unverified,
}
#[derive(Debug, Clone, Serialize)]
pub struct ScenarioObservation {
    pub case_id: Uuid,
    pub context: String,
    pub source_event_ids: Vec<Uuid>,
    pub result_event_id: Uuid,
    pub outcome: Outcome,
}
#[derive(Debug, Clone, Serialize)]
pub struct IndependentAssessment {
    pub review_id: Uuid,
    pub reviewer_id: Uuid,
    pub origin: ReviewOrigin,
    pub reviewed_evidence_ids: Vec<Uuid>,
    pub reviewed_counterevidence_ids: Vec<Uuid>,
    pub counter_search_event_ids: Vec<Uuid>,
    pub counter_review: Outcome,
    pub observations: Vec<ScenarioObservation>,
}
#[derive(Debug)]
pub struct VerifiedReview {
    binding: [u8; 32],
    trial_binding: [u8; 32],
    assessment: IndependentAssessment,
    sources: Vec<ServerSource>,
    trial_started_at: DateTime<Utc>,
    reviewed_at: DateTime<Utc>,
}
pub struct TrustedValidator;
fn sources_valid(
    candidate: &DevelopmentCandidate,
    sources: &[ServerSource],
    now: DateTime<Utc>,
) -> bool {
    ids(
        &sources.iter().map(|s| s.id).collect::<Vec<_>>(),
        1,
        usize::MAX,
    ) && sources
        .iter()
        .all(|s| s.individual_id == candidate.individual_id && s.recorded_at <= now)
}
fn same_ids(a: &[Uuid], b: &[Uuid]) -> bool {
    a.iter().copied().collect::<HashSet<_>>() == b.iter().copied().collect::<HashSet<_>>()
        && a.len() == b.len()
}
impl TrustedValidator {
    pub fn authorize_trial(
        candidate: &DevelopmentCandidate,
        sources: Vec<ServerSource>,
        route: ValidationRoute,
        now: DateTime<Utc>,
    ) -> Result<TrialAuthorization, DevelopmentError> {
        if now < candidate.prepared_at || now >= candidate.draft.deadline {
            return Err(DevelopmentError::Deadline);
        }
        if !sources_valid(candidate, &sources, now)
            || candidate
                .draft
                .evidence_ids
                .iter()
                .chain(&candidate.draft.counterevidence_ids)
                .any(|id| {
                    !sources
                        .iter()
                        .any(|s| s.id == *id && s.recorded_at <= candidate.prepared_at)
                })
        {
            return Err(DevelopmentError::Evidence);
        }
        if !candidate.draft.evidence_ids.iter().any(|id| {
            sources.iter().any(|s| {
                s.id == *id
                    && matches!(
                        s.class,
                        SourceClass::ObservedOutcome
                            | SourceClass::ExternalReport
                            | SourceClass::SignificantEvent
                    )
            })
        }) {
            return Err(DevelopmentError::Evidence);
        }
        if let ValidationRoute::SignificantEvent { event_id } = &route
            && (!candidate.draft.evidence_ids.contains(event_id)
                || !sources
                    .iter()
                    .any(|s| s.id == *event_id && s.class == SourceClass::SignificantEvent))
        {
            return Err(DevelopmentError::Evidence);
        }
        Ok(TrialAuthorization {
            binding: candidate.binding,
            route,
            sources,
            issued_at: now,
        })
    }
    pub fn review(
        development: &Development,
        assessment: IndependentAssessment,
        sources: Vec<ServerSource>,
        now: DateTime<Utc>,
    ) -> Result<VerifiedReview, DevelopmentError> {
        development.require_trial(now)?;
        let candidate = &development.candidate;
        let started = development
            .trial_started_at
            .ok_or(DevelopmentError::Transition)?;
        if !sources_valid(candidate, &sources, now)
            || assessment.review_id.is_nil()
            || assessment.reviewer_id.is_nil()
            || assessment.reviewer_id == candidate.proposer_id
            || assessment.origin == ReviewOrigin::AuthorSelfAssessment
        {
            return Err(DevelopmentError::Review);
        }
        if !same_ids(
            &assessment.reviewed_evidence_ids,
            &candidate.draft.evidence_ids,
        ) || !same_ids(
            &assessment.reviewed_counterevidence_ids,
            &candidate.draft.counterevidence_ids,
        ) || !ids(&assessment.counter_search_event_ids, 1, usize::MAX)
            || assessment.observations.is_empty()
        {
            return Err(DevelopmentError::Review);
        }
        for id in candidate
            .draft
            .evidence_ids
            .iter()
            .chain(&candidate.draft.counterevidence_ids)
        {
            let original = development
                .trial_sources
                .iter()
                .find(|s| s.id == *id)
                .ok_or(DevelopmentError::Review)?;
            if !sources.iter().any(|s| s == original) {
                return Err(DevelopmentError::Review);
            }
        }
        let measured = |id: Uuid| {
            sources.iter().any(|s| {
                s.id == id && s.class == SourceClass::ObservedOutcome && s.recorded_at >= started
            })
        };
        if assessment.counter_search_event_ids.iter().any(|id| {
            !measured(*id)
                || candidate.draft.evidence_ids.contains(id)
                || candidate.draft.counterevidence_ids.contains(id)
        }) {
            return Err(DevelopmentError::Review);
        }
        let mut cases = HashSet::new();
        let mut results = HashSet::new();
        for o in &assessment.observations {
            if o.case_id.is_nil()
                || !cases.insert(o.case_id)
                || !text(&o.context, 200)
                || !ids(&o.source_event_ids, 1, usize::MAX)
                || o.source_event_ids
                    .iter()
                    .any(|id| !sources.iter().any(|s| s.id == *id))
                || !measured(o.result_event_id)
                || !results.insert(o.result_event_id)
                || candidate.draft.evidence_ids.contains(&o.result_event_id)
                || candidate
                    .draft
                    .counterevidence_ids
                    .contains(&o.result_event_id)
            {
                return Err(DevelopmentError::Review);
            }
        }
        Ok(VerifiedReview {
            binding: candidate.binding,
            trial_binding: development
                .trial_binding
                .ok_or(DevelopmentError::Transition)?,
            assessment,
            sources,
            trial_started_at: started,
            reviewed_at: now,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DevelopmentStatus {
    Proposed,
    Trial,
    Accepted,
    Rejected,
    Expired,
    Reverted,
}
#[derive(Debug, Clone, Serialize)]
pub struct ValidationRecord {
    pub assessment: IndependentAssessment,
    pub sources: Vec<ServerSource>,
    pub recorded_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize)]
pub struct TransitionRecord {
    pub from: Option<DevelopmentStatus>,
    pub to: DevelopmentStatus,
    pub occurred_at: DateTime<Utc>,
    pub before_value: Value,
    pub after_value: Value,
    pub reason: String,
    pub review_ids: Vec<Uuid>,
    pub revision_id: Option<Uuid>,
}
#[derive(Debug, Clone, Serialize)]
pub enum ApplicationValue {
    Persona(Value),
    TaskStrategy {
        scope: StrategyScope,
        strategy: Option<TaskStrategy>,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct ApplicationIntent {
    pub target: DevelopmentTarget,
    pub scope: Option<StrategyScope>,
    pub expected_version: i64,
    pub expected_head_revision_id: Option<Uuid>,
    pub revision_id: Uuid,
    pub next_version: i64,
    pub before_value: Value,
    pub after_value: Value,
    pub application: ApplicationValue,
}
#[derive(Debug, Clone)]
pub struct Development {
    candidate: DevelopmentCandidate,
    status: DevelopmentStatus,
    route: Option<ValidationRoute>,
    trial_sources: Vec<ServerSource>,
    trial_started_at: Option<DateTime<Utc>>,
    trial_binding: Option<[u8; 32]>,
    validations: Vec<ValidationRecord>,
    history: Vec<TransitionRecord>,
    accepted_revision: Option<(Uuid, i64)>,
}
impl Development {
    pub fn propose(candidate: DevelopmentCandidate) -> Self {
        let record = TransitionRecord {
            from: None,
            to: DevelopmentStatus::Proposed,
            occurred_at: candidate.prepared_at,
            before_value: candidate.before.value.clone(),
            after_value: candidate.draft.proposed_value.clone(),
            reason: candidate.draft.proposed_description.clone(),
            review_ids: vec![],
            revision_id: None,
        };
        Self {
            candidate,
            status: DevelopmentStatus::Proposed,
            route: None,
            trial_sources: vec![],
            trial_started_at: None,
            trial_binding: None,
            validations: vec![],
            history: vec![record],
            accepted_revision: None,
        }
    }
    pub fn candidate(&self) -> &DevelopmentCandidate {
        &self.candidate
    }
    pub fn status(&self) -> DevelopmentStatus {
        self.status
    }
    pub fn validations(&self) -> &[ValidationRecord] {
        &self.validations
    }
    pub fn history(&self) -> &[TransitionRecord] {
        &self.history
    }
    pub fn trial_sources(&self) -> &[ServerSource] {
        &self.trial_sources
    }
    fn require_time(&self, now: DateTime<Utc>) -> Result<(), DevelopmentError> {
        if now < self.candidate.prepared_at
            || self.history.last().is_some_and(|r| now < r.occurred_at)
            || self.validations.last().is_some_and(|r| now < r.recorded_at)
        {
            return Err(DevelopmentError::Invalid);
        }
        Ok(())
    }
    fn require_trial(&self, now: DateTime<Utc>) -> Result<(), DevelopmentError> {
        self.require_time(now)?;
        if self.status != DevelopmentStatus::Trial {
            return Err(DevelopmentError::Transition);
        }
        if now >= self.candidate.draft.deadline {
            return Err(DevelopmentError::Deadline);
        }
        Ok(())
    }
    fn transition(
        &self,
        to: DevelopmentStatus,
        now: DateTime<Utc>,
        reason: String,
        revision_id: Option<Uuid>,
    ) -> Self {
        let mut next = self.clone();
        next.history.push(TransitionRecord {
            from: Some(self.status),
            to,
            occurred_at: now,
            before_value: self.candidate.before.value.clone(),
            after_value: self.candidate.draft.proposed_value.clone(),
            reason,
            review_ids: self
                .validations
                .iter()
                .map(|v| v.assessment.review_id)
                .collect(),
            revision_id,
        });
        next.status = to;
        next
    }
    pub fn start_trial(
        &self,
        authorization: &TrialAuthorization,
        now: DateTime<Utc>,
    ) -> Result<Self, DevelopmentError> {
        if self.status != DevelopmentStatus::Proposed {
            return Err(DevelopmentError::Transition);
        }
        if now >= self.candidate.draft.deadline {
            return Err(DevelopmentError::Deadline);
        }
        self.require_time(now)?;
        if authorization.binding != self.candidate.binding || now < authorization.issued_at {
            return Err(DevelopmentError::Evidence);
        }
        let mut next = self.transition(
            DevelopmentStatus::Trial,
            now,
            "bounded trial started with source authorization".into(),
            None,
        );
        next.route = Some(authorization.route.clone());
        next.trial_sources = authorization.sources.clone();
        next.trial_started_at = Some(now);
        next.trial_binding = Some(hash(&json!({
            "candidate": self.candidate.binding,
            "route": authorization.route,
            "sources": authorization.sources,
            "authorized_at": authorization.issued_at,
            "started_at": now
        }))?);
        Ok(next)
    }
    pub fn record_validation(
        &self,
        review: &VerifiedReview,
        now: DateTime<Utc>,
    ) -> Result<Self, DevelopmentError> {
        self.require_trial(now)?;
        if review.binding != self.candidate.binding
            || self.trial_binding != Some(review.trial_binding)
            || self.trial_started_at != Some(review.trial_started_at)
            || now < review.reviewed_at
            || self
                .validations
                .iter()
                .any(|v| v.assessment.review_id == review.assessment.review_id)
        {
            return Err(DevelopmentError::Review);
        }
        let mut next = self.clone();
        next.validations.push(ValidationRecord {
            assessment: review.assessment.clone(),
            sources: review.sources.clone(),
            recorded_at: now,
        });
        Ok(next)
    }
    fn intent(
        &self,
        current: &TargetSnapshot,
        persona: &Value,
        new_revision: Uuid,
        after_value: &Value,
    ) -> Result<ApplicationIntent, DevelopmentError> {
        if current.target != self.candidate.draft.target
            || current.scope != self.candidate.before.scope
            || new_revision.is_nil()
            || new_revision == self.candidate.id
            || self.candidate.before.head_revision_id == Some(new_revision)
            || current.head_revision_id == Some(new_revision)
        {
            return Err(DevelopmentError::Conflict);
        }
        if let Some(pointer) = current.target.persona_pointer()
            && persona.pointer(pointer) != Some(&current.value)
        {
            return Err(DevelopmentError::Conflict);
        }
        let application = apply_value(current.target, &current.scope, after_value, persona)?;
        Ok(ApplicationIntent {
            target: current.target,
            scope: current.scope.clone(),
            expected_version: current.version,
            expected_head_revision_id: current.head_revision_id,
            revision_id: new_revision,
            next_version: current
                .version
                .checked_add(1)
                .ok_or(DevelopmentError::Conflict)?,
            before_value: current.value.clone(),
            after_value: after_value.clone(),
            application,
        })
    }
    pub fn accept(
        &self,
        current: &TargetSnapshot,
        persona: &Value,
        new_revision: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(Self, ApplicationIntent), DevelopmentError> {
        self.require_trial(now)?;
        if current.version != self.candidate.before.version
            || current.head_revision_id != self.candidate.before.head_revision_id
            || current.value != self.candidate.before.value
        {
            return Err(DevelopmentError::Conflict);
        }
        if self.validations.is_empty()
            || self.validations.iter().any(|v| {
                v.assessment.counter_review != Outcome::Passed
                    || v.assessment
                        .observations
                        .iter()
                        .any(|o| o.outcome != Outcome::Passed)
            })
        {
            return Err(DevelopmentError::Unverified);
        }
        let contexts: HashSet<_> = self
            .validations
            .iter()
            .flat_map(|v| v.assessment.observations.iter().map(|o| o.context.as_str()))
            .collect();
        let cases: Vec<_> = self
            .validations
            .iter()
            .flat_map(|v| &v.assessment.observations)
            .collect();
        if cases
            .iter()
            .map(|o| o.case_id)
            .collect::<HashSet<_>>()
            .len()
            != cases.len()
            || cases
                .iter()
                .map(|o| o.result_event_id)
                .collect::<HashSet<_>>()
                .len()
                != cases.len()
        {
            return Err(DevelopmentError::Review);
        }
        match &self.route {
            Some(ValidationRoute::CrossScenarioHabit) if contexts.len() >= 2 => {}
            Some(ValidationRoute::SignificantEvent { .. }) if !cases.is_empty() => {}
            _ => return Err(DevelopmentError::Unverified),
        }
        let intent = self.intent(
            current,
            persona,
            new_revision,
            &self.candidate.draft.proposed_value,
        )?;
        let mut next = self.transition(
            DevelopmentStatus::Accepted,
            now,
            "independent complete validation accepted".into(),
            Some(new_revision),
        );
        next.accepted_revision = Some((new_revision, intent.next_version));
        Ok((next, intent))
    }
    pub fn reject(&self, reason: &str, now: DateTime<Utc>) -> Result<Self, DevelopmentError> {
        self.require_time(now)?;
        if !matches!(
            self.status,
            DevelopmentStatus::Proposed | DevelopmentStatus::Trial
        ) {
            return Err(DevelopmentError::Transition);
        }
        if now >= self.candidate.draft.deadline {
            return Err(DevelopmentError::Deadline);
        }
        if !text(reason, 2000) {
            return Err(DevelopmentError::Invalid);
        }
        Ok(self.transition(DevelopmentStatus::Rejected, now, reason.into(), None))
    }
    pub fn expire(&self, now: DateTime<Utc>) -> Result<Self, DevelopmentError> {
        self.require_time(now)?;
        if !matches!(
            self.status,
            DevelopmentStatus::Proposed | DevelopmentStatus::Trial
        ) {
            return Err(DevelopmentError::Transition);
        }
        if now < self.candidate.draft.deadline {
            return Err(DevelopmentError::Deadline);
        }
        Ok(self.transition(
            DevelopmentStatus::Expired,
            now,
            "actual deadline reached".into(),
            None,
        ))
    }
    pub fn revert(
        &self,
        current: &TargetSnapshot,
        persona: &Value,
        new_revision: Uuid,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<(Self, ApplicationIntent), DevelopmentError> {
        self.require_time(now)?;
        if self.status != DevelopmentStatus::Accepted {
            return Err(DevelopmentError::Transition);
        }
        if !text(reason, 2000) {
            return Err(DevelopmentError::Invalid);
        }
        let (accepted, version) = self.accepted_revision.ok_or(DevelopmentError::Transition)?;
        if current.head_revision_id != Some(accepted)
            || current.version != version
            || current.value != self.candidate.draft.proposed_value
        {
            return Err(DevelopmentError::Conflict);
        }
        let intent = self.intent(current, persona, new_revision, &self.candidate.before.value)?;
        let mut next = self.transition(
            DevelopmentStatus::Reverted,
            now,
            reason.into(),
            Some(new_revision),
        );
        if let Some(record) = next.history.last_mut() {
            record.before_value = intent.before_value.clone();
            record.after_value = intent.after_value.clone();
        }
        Ok((next, intent))
    }
}
