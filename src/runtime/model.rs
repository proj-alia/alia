use crate::domain::cognition::RunMode;
use crate::domain::cognition::{DecisionProposal, ModelContext, ProposalError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMode {
    Live,
    Fixture,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelReadiness {
    Ready,
    Unavailable,
    UnverifiedTokenBound,
    Fixture,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ModelStatus {
    pub mode: ProviderMode,
    pub readiness: ModelReadiness,
}

#[derive(Clone, Default, Serialize)]
pub struct ModelConnectionInfo {
    pub provider: Option<&'static str>,
    pub model: Option<&'static str>,
    pub credential_configured: bool,
    pub input_accounting: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

/// Transport routing only: never serialize into the model prompt.
#[derive(Clone, Copy)]
pub enum ModelCallMetadata {
    Foreground { conversation_id: Option<Uuid> },
    Background { work_id: Uuid },
}
impl ModelCallMetadata {
    pub fn foreground(context: &ModelContext) -> Result<Self, ModelFailure> {
        if context.mode != RunMode::Foreground {
            return Err(ModelFailure::before_request(ModelError::Configuration));
        }
        let event = context
            .events
            .iter()
            .find(|e| e.id == context.current_event_id)
            .ok_or_else(|| ModelFailure::before_request(ModelError::Configuration))?;
        let conversation_id = match event.payload.get("conversation_id") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(
                value
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .ok_or_else(|| ModelFailure::before_request(ModelError::Configuration))?,
            ),
        };
        Ok(Self::Foreground { conversation_id })
    }
    pub fn session_id(self, context: &ModelContext) -> Result<String, ModelFailure> {
        match (self, &context.mode) {
            (Self::Foreground { conversation_id }, RunMode::Foreground) => {
                Ok(match conversation_id {
                    Some(id) => format!("alia_{}_conversation_{id}", context.individual_id),
                    None => format!("alia_{}_foreground_default", context.individual_id),
                })
            }
            (Self::Background { work_id }, RunMode::Background) => {
                Ok(format!("alia_{}_work_{work_id}", context.individual_id))
            }
            _ => Err(ModelFailure::before_request(ModelError::Configuration)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    GatewayBlocked,
    Authentication,
    SubscriptionRequired,
    SessionMissing,
    Quota,
    RateLimit,
    Unknown,
}

/// Only validated identifiers and enum categories may cross the adapter boundary.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModelHttpDiagnostic {
    pub status: u16,
    pub request_id: Option<String>,
    pub category: Option<ProviderErrorKind>,
}

#[derive(Clone, Copy)]
pub struct CallLimits {
    pub max_output_tokens: u32,
    pub timeout_seconds: u64,
}
impl Default for CallLimits {
    fn default() -> Self {
        Self {
            max_output_tokens: 4096,
            timeout_seconds: 45,
        }
    }
}

pub struct ModelResult {
    pub http: Option<ModelHttpDiagnostic>,
    pub proposal: DecisionProposal,
    pub usage: Option<TokenUsage>,
    pub provider_model: String,
    pub system_fingerprint: Option<String>,
    pub provider_mode: ProviderMode,
}

pub struct GoalReviewResult {
    pub assessment: crate::domain::review::GoalAssessment,
    pub usage: Option<TokenUsage>,
    pub provider_model: String,
    pub provider_mode: ProviderMode,
}
pub struct DecisionReviewResult {
    pub assessment: crate::domain::grounding::DecisionAssessment,
    pub usage: Option<TokenUsage>,
    pub provider_model: String,
    pub provider_mode: ProviderMode,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("provider has no verified token bound for managed execution")]
    UnverifiedTokenBound,
    #[error("model credentials are unavailable")]
    Unavailable,
    #[error("invalid model deployment configuration")]
    Configuration,
    #[error("model request failed or timed out")]
    Transport,
    #[error("provider returned HTTP status {0}")]
    Http(u16),
    #[error("provider returned an invalid or oversized response")]
    Protocol,
    #[error(transparent)]
    Assessment(#[from] crate::domain::grounding::AssessmentError),
    #[error("model response was truncated")]
    Truncated,
    #[error("model call limits are invalid")]
    Limits,
    #[error(transparent)]
    Proposal(#[from] ProposalError),
}

#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct ModelFailure {
    pub http: Option<ModelHttpDiagnostic>,
    pub error: ModelError,
    pub usage: Option<TokenUsage>,
    pub request_sent: bool,
}
impl ModelFailure {
    pub fn before_request(error: ModelError) -> Self {
        Self {
            error,
            http: None,
            usage: None,
            request_sent: false,
        }
    }
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn mode(&self) -> ProviderMode;
    fn status(&self) -> ModelStatus {
        ModelStatus {
            mode: self.mode(),
            readiness: ModelReadiness::UnverifiedTokenBound,
        }
    }
    /// Must account for the adapter's actual prompt framing and all generation.
    /// Live adapters remain unavailable to the managed runner until verified.
    fn token_allowance(
        &self,
        _: &ModelContext,
        _: CallLimits,
    ) -> Result<super::scheduling::TokenAllowance, ModelFailure> {
        Err(ModelFailure::before_request(
            ModelError::UnverifiedTokenBound,
        ))
    }
    fn supports_goal_review(&self) -> bool {
        false
    }
    fn supports_decision_review(&self) -> bool {
        false
    }
    fn decision_review_allowance(
        &self,
        _: &ModelContext,
        _: CallLimits,
    ) -> Result<super::scheduling::TokenAllowance, ModelFailure> {
        Err(ModelFailure::before_request(
            ModelError::UnverifiedTokenBound,
        ))
    }
    async fn review_decision(
        &self,
        _: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<DecisionReviewResult, ModelFailure> {
        Err(ModelFailure::before_request(ModelError::Unavailable))
    }
    fn review_allowance(
        &self,
        _: &crate::domain::review::GoalReviewInput,
        _: CallLimits,
    ) -> Result<super::scheduling::TokenAllowance, ModelFailure> {
        Err(ModelFailure::before_request(
            ModelError::UnverifiedTokenBound,
        ))
    }
    async fn review(
        &self,
        _: &crate::domain::review::GoalReviewInput,
        _: CallLimits,
        _: ModelCallMetadata,
        _: &ModelContext,
    ) -> Result<GoalReviewResult, ModelFailure> {
        Err(ModelFailure::before_request(
            ModelError::UnverifiedTokenBound,
        ))
    }
    async fn decide(
        &self,
        context: &ModelContext,
        limits: CallLimits,
        metadata: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure>;
}

/// Missing credentials are a real unavailable capability, never a fixture fallback.
pub struct UnavailableProvider;
#[async_trait]
impl ModelProvider for UnavailableProvider {
    fn status(&self) -> ModelStatus {
        ModelStatus {
            mode: self.mode(),
            readiness: ModelReadiness::Unavailable,
        }
    }
    fn mode(&self) -> ProviderMode {
        ProviderMode::Live
    }
    fn token_allowance(
        &self,
        _: &ModelContext,
        _: CallLimits,
    ) -> Result<super::scheduling::TokenAllowance, ModelFailure> {
        Err(ModelFailure::before_request(ModelError::Unavailable))
    }
    async fn decide(
        &self,
        _: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        Err(ModelFailure::before_request(ModelError::Unavailable))
    }
}

/// Explicitly injected test double; deployment has no environment switch to select it.
pub struct FixtureProvider {
    pub response: String,
}
#[async_trait]
impl ModelProvider for FixtureProvider {
    fn status(&self) -> ModelStatus {
        ModelStatus {
            mode: self.mode(),
            readiness: ModelReadiness::Fixture,
        }
    }
    fn token_allowance(
        &self,
        _: &ModelContext,
        limits: CallLimits,
    ) -> Result<super::scheduling::TokenAllowance, ModelFailure> {
        // Explicit test double. This arithmetic is never a claim about live tokenizers.
        Ok(super::scheduling::TokenAllowance {
            input_upper_bound: 1,
            output_cap: limits.max_output_tokens,
        })
    }
    fn mode(&self) -> ProviderMode {
        ProviderMode::Fixture
    }
    async fn decide(
        &self,
        _: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        let proposal = DecisionProposal::parse(self.response.as_bytes())
            .map_err(|e| ModelFailure::before_request(e.into()))?;
        Ok(ModelResult {
            http: None,
            proposal,
            usage: None,
            provider_model: "fixture".into(),
            system_fingerprint: None,
            provider_mode: ProviderMode::Fixture,
        })
    }
}

/// One call after the runner has reserved budget and recorded its input manifest.
/// Does not grant an execution lease or commit any state. The caller must fence commits.
pub async fn evaluate_proposal(
    provider: &dyn ModelProvider,
    context: &ModelContext,
    limits: CallLimits,
    metadata: ModelCallMetadata,
) -> Result<ModelResult, ModelFailure> {
    let result = provider.decide(context, limits, metadata).await?;
    if result.provider_mode != provider.mode() {
        return Err(ModelFailure {
            http: result.http.clone(),
            error: ModelError::Protocol,
            usage: result.usage,
            request_sent: provider.mode() == ProviderMode::Live,
        });
    }
    result
        .proposal
        .validate_context(context)
        .map_err(|error| ModelFailure {
            http: result.http.clone(),
            error: error.into(),
            usage: result.usage,
            request_sent: provider.mode() == ProviderMode::Live,
        })?;
    Ok(result)
}
