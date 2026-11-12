use super::{
    config::parse_strict_json,
    history::{Event, Memory, NewMemory, SourceKind},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, sync::LazyLock};
use uuid::Uuid;

pub const DECISION_SCHEMA: &str = include_str!("../../schemas/decision.schema.json");
static VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
    jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&serde_json::from_str::<Value>(DECISION_SCHEMA).expect("embedded schema"))
        .expect("valid embedded decision schema")
});

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Foreground,
    Background,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CapabilityInfo {
    pub id: String,
    pub version: String,
    pub available: bool,
    pub authorized: bool,
    pub description: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InterestRef {
    pub id: String,
    pub description: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GoalView {
    pub id: Uuid,
    #[serde(default = "default_goal_priority")]
    pub priority: u8,
    pub description: String,
    pub status: String,
    pub success_criterion: String,
    pub next_step: String,
    #[serde(default)]
    pub stop_condition: String,
    #[serde(default)]
    pub waiting_reason: Option<String>,
    #[serde(default)]
    pub no_progress_cycles: u32,
    #[serde(default)]
    pub completion: Option<super::completion::CompletionContract>,
}
fn default_goal_priority() -> u8 {
    50
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DocumentRef {
    pub document_id: Uuid,
    pub title: String,
    pub source_label: String,
    pub characters: i64,
}

/// Available metadata, not evidence that the document has been read.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct DocumentCatalog {
    pub items: Vec<DocumentRef>,
    pub has_more: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CompletionEvidenceRef {
    pub goal_id: Uuid,
    pub result_event_id: Uuid,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ExecutionBudgetView {
    pub model_calls_used: u32,
    pub model_calls_remaining: u32,
    pub model_call_limit: u32,
    pub tool_calls_used: u32,
    pub tool_calls_remaining: u32,
    pub tool_call_limit: u32,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct GoalOriginCatalog {
    pub items: Vec<GoalOriginOption>,
    pub has_more: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct GoalOriginOption {
    pub origin_type: String,
    pub origin_id: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    tag = "origin_type",
    content = "origin_id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GoalOriginRef {
    Event(Uuid),
    Interest(String),
}
impl GoalOriginRef {
    pub fn storage_projection(origins: &[Self]) -> (&'static str, Vec<String>) {
        let events = origins.iter().any(|o| matches!(o, Self::Event(_)));
        let interests = origins.iter().any(|o| matches!(o, Self::Interest(_)));
        let kind = if events && interests {
            "mixed"
        } else if events {
            "event"
        } else {
            "interest"
        };
        (
            kind,
            origins
                .iter()
                .map(|o| match o {
                    Self::Event(id) => id.to_string(),
                    Self::Interest(id) => id.clone(),
                })
                .collect(),
        )
    }
    pub fn from_storage(kind: &str, ids: &[String]) -> Result<Vec<Self>, ProposalError> {
        if !matches!(kind, "event" | "interest" | "mixed") {
            return Err(ProposalError::GoalOrigin);
        }
        let origins: Vec<_> = ids
            .iter()
            .map(|id| match kind {
                "interest" => Ok(Self::Interest(id.clone())),
                "event" => Uuid::parse_str(id)
                    .map(Self::Event)
                    .map_err(|_| ProposalError::GoalOrigin),
                "mixed" if id.starts_with("interest:") => Ok(Self::Interest(id.clone())),
                "mixed" => Uuid::parse_str(id)
                    .map(Self::Event)
                    .map_err(|_| ProposalError::GoalOrigin),
                _ => Err(ProposalError::GoalOrigin),
            })
            .collect::<Result<_, _>>()?;
        if kind == "mixed" && Self::storage_projection(&origins).0 != "mixed" {
            return Err(ProposalError::GoalOrigin);
        }
        Ok(origins)
    }
}
pub(crate) fn is_goal_origin_event(event: &Event) -> bool {
    !event.kind.starts_with("goal.")
        && !event.kind.starts_with("decision.")
        && !event.kind.starts_with("run.")
        && !matches!(event.kind.as_str(), "message.sent" | "message.drafted")
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ModelContext {
    /// Captured before reserving this author call; remaining includes this call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_budget: Option<ExecutionBudgetView>,
    #[serde(default)]
    pub goal_origin_catalog: GoalOriginCatalog,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_review: Option<super::grounding::DecisionReviewDraft>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval: Option<super::retrieval::RetrievalSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_beliefs: Option<super::beliefs::SelectedBeliefs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autobiographical_context: Option<super::autobiography::AutobiographyContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social_context: Option<super::social::SocialContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interest_context: Option<super::interests::InterestContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_context: Option<super::development_context::DevelopmentContext>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub development_targets: Vec<super::development::DevelopmentTargetView>,
    /// Correlation metadata only; persisted trial state is the authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_binding: Option<super::development_trials::TrialBinding>,
    pub individual_id: Uuid,
    pub run_id: Uuid,
    pub state_version: i64,
    pub privacy_epoch: i64,
    pub config_version: i64,
    pub mode: RunMode,
    pub now: DateTime<Utc>,
    pub persona: Value,
    pub relationship: Value,
    pub current_event_id: Uuid,
    pub current_scope: String,
    #[serde(default)]
    pub current_goal_id: Option<Uuid>,
    #[serde(default)]
    pub completion_evidence: Vec<CompletionEvidenceRef>,
    pub events: Vec<Event>,
    pub memories: Vec<Memory>,
    pub goals: Vec<GoalView>,
    pub interests: Vec<InterestRef>,
    pub capabilities: Vec<CapabilityInfo>,
    #[serde(default)]
    pub document_catalog: DocumentCatalog,
    #[serde(default)]
    pub person_catalog: super::entities::PersonCatalog,
    pub input_manifest: Vec<Uuid>,
    /// Populated only by a real result evaluator; not by a model's completion claim.
    pub verified_goal_completions: Vec<Uuid>,
    /// Runtime-only review payload; ordinary decision contexts leave this absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_review: Option<super::review::GoalReviewInput>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionProposal {
    pub schema_version: u32,
    pub reply: String,
    pub reply_evidence_ids: Vec<Uuid>,
    pub memory_changes: Vec<NewMemory>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub belief_changes: Vec<super::beliefs::BeliefDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub autobiography_changes: Vec<super::autobiography::AutobiographyDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub appraisal_changes: Vec<super::affect::AppraisalDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attitude_changes: Vec<super::affect::AttitudeDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationship_changes: Vec<super::relationships::RelationshipRevisionDraft>,
    #[serde(default)]
    pub entity_changes: Vec<super::entities::EntityProposal>,
    pub goal_changes: Vec<GoalChange>,
    pub tool_intents: Vec<ToolIntent>,
    pub development: Option<super::development::DevelopmentDraft>,
    pub brief_reason: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum GoalChange {
    Create {
        origins: Vec<GoalOriginRef>,
        description: String,
        success_criterion: String,
        next_step: String,
        stop_condition: String,
        priority: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        completion: Option<super::completion::CompletionContract>,
    },
    Activate {
        goal_id: Uuid,
        reason: String,
        evidence_ids: Vec<Uuid>,
    },
    Wait {
        goal_id: Uuid,
        reason: String,
        evidence_ids: Vec<Uuid>,
    },
    Complete {
        goal_id: Uuid,
        reason: String,
        evidence_ids: Vec<Uuid>,
    },
    Abandon {
        goal_id: Uuid,
        reason: String,
        evidence_ids: Vec<Uuid>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolIntent {
    pub capability_id: String,
    pub input: Value,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ProposalError {
    #[error("invalid source-bound social state change")]
    Social,
    #[error("invalid autobiographical interpretation or source")]
    Autobiography,
    #[error("invalid proposal JSON or schema")]
    Schema,
    #[error("development target, value, version, scope or evidence is invalid")]
    Development,
    #[error("invalid proposal field: {0}")]
    SchemaField(&'static str),
    #[error("evidence is absent from the authorized context")]
    UnknownEvidence,
    #[error("memory scope or revision is incompatible with this context")]
    Scope,
    #[error("external or generated material cannot be the sole evidence of personal experience")]
    UnobservedExperience,
    #[error("person proposal lacks visible literal evidence or has an invalid reference")]
    Entity,
    #[error("person proposal evidence is invalid")]
    EntityProposal(super::entities::EntityValidationError),
    #[error("goal origin or transition is invalid")]
    Goal,
    #[error("a paused or abandoned goal cannot be combined with tool execution")]
    GoalActionConflict,
    #[error("completion and tool execution must be separate decisions")]
    CompletionActionConflict,
    #[error("completion must cite its verified result event")]
    CompletionEvidenceMissing,
    #[error("goal completion has not been verified from actual results")]
    UnverifiedCompletion,
    #[error("capability is unavailable or unauthorized")]
    Capability,
    #[error("a tool intent cannot include a final reply before the result exists")]
    PrematureReply,
    #[error("candidate content did not pass evidence review")]
    Grounding,
    #[error("belief evidence, scope, or current predecessor is invalid")]
    Belief,
    #[error("goal origin is absent, internal or incorrectly typed")]
    GoalOrigin,
}

impl ProposalError {
    pub fn feedback(&self) -> (&'static str, &'static str) {
        match self {
            Self::Social => (
                "invalid_social_change",
                "评价最多1项，长期态度及关系各最多2项且不重复topic/subject；引用实际events和当前已观察对象/前身/版本。只改变自己的有限理解，不替用户断言感情，不以缺席惩罚关系，不修改人格或权限。",
            ),
            Self::Autobiography => (
                "invalid_autobiography",
                "autobiography_changes最多2项，topic不可重复。只引用实际events及autobiographical_context中带完整依据的belief_basis；supersedes须是同topic的当前解释，不能把背景/推测改写成亲历或修改人格权限。",
            ),
            Self::Belief => (
                "invalid_belief",
                "belief_changes最多3项；作用域仅general或current_scope，支持/反对来源必须是当前实际events。supersedes只能用当前selected_beliefs中的同作用域前身；不得重复修订同一信念或伪造事实。",
            ),
            Self::GoalOrigin => (
                "invalid_goal_origin",
                "origins逐条填写目录中的origin_type和origin_id；事件与兴趣可以共同支持目标，但每条类型必须正确且ID实际可见。不要用内部goal/decision/run通知、虚构ID或未分配目标ID。",
            ),
            Self::Grounding => (
                "decision_grounding_failed",
                "候选回复或记忆的依据审查未通过。查看本轮decision.reviewed反馈；不能把被拒草稿当作事实，不扩大权限或调用预算。",
            ),
            Self::EntityProposal(error) => error.feedback(),
            Self::Entity => (
                "invalid_entity_proposal",
                "人物来源必须是当前实际可见文本的名字区间，记忆必须引用同一来源且正文包含原字；使用已知人物ID或更早提案的prior引用。已登记名字不能当new，同名不猜测合并；不确定时省略关联。",
            ),
            Self::GoalActionConflict => (
                "goal_action_conflict",
                "等待工具回执由运行时处理，不是goal wait。带tool_intents时不要同时wait或abandon目标；保留active并等待真实结果。",
            ),
            Self::Schema => (
                "invalid_schema",
                "必须输出完整JSON对象，包含全部必填字段；禁止未知或重复字段。",
            ),
            Self::SchemaField("goal_changes") => (
                "invalid_goal_fields",
                "检查goal_changes对应action的必填字段、字段类型和枚举；create不能混入已有目标的goal_id。",
            ),
            Self::SchemaField("memory_changes") => (
                "invalid_memory_fields",
                "检查memory_changes条目的必填字段、kind枚举、scope与source_event_ids格式。",
            ),
            Self::SchemaField("tool_intents") => (
                "invalid_tool_fields",
                "tool_intents每项仅含capability_id和input；具体输入遵循该能力schema。",
            ),
            Self::SchemaField(_) => (
                "invalid_field",
                "某个已知字段未满足schema；检查类型、长度、枚举与必填值。",
            ),
            Self::Development => (
                "invalid_development",
                "development仅提出类型化候选，复制development_targets中的真实target/scope/before_version；引用当前实际事件及反证，提供proposed_value、验证计划和未来期限。提案不直接改变人格，不修改身份/权限，不把候选当已试用或已接受。",
            ),
            Self::UnknownEvidence => (
                "unknown_evidence",
                "回复来源只用当前events、memories或selected_beliefs中的立场ID；新记忆/信念的事件来源只用当前实际events，不能引用新生成ID或把信念当世界事实证明。",
            ),
            Self::Scope => (
                "invalid_scope",
                "记忆scope只允许general或current_scope；修订不得跨作用域或覆盖已替代记忆。",
            ),
            Self::UnobservedExperience => (
                "unobserved_experience",
                "外部资料、推测或生成文本不能单独作为亲历记忆的证据。",
            ),
            Self::Goal => (
                "invalid_goal",
                "检查目标来源与当前状态；使用实际事件或interests的ID，不引用run/goal/decision通知创建新目标，不改变用户暂停的目标。",
            ),
            Self::CompletionActionConflict => (
                "completion_action_conflict",
                "complete与tool_intents必须分开提交；写入新产物会改变审查快照，需要重新评估。完成时tool_intents必须为空。",
            ),
            Self::CompletionEvidenceMissing => (
                "completion_evidence_missing",
                "complete的evidence_ids必须包含completion_evidence中该goal_id对应的result_event_id；目标ID、资料ID或笔记动作ID不能替代结果事件。",
            ),
            Self::UnverifiedCompletion => (
                "unverified_completion",
                "只有verified_goal_completions内的目标才能完成，必须引用真实验证结果；否则继续工作或等待。",
            ),
            Self::Capability => (
                "unavailable_capability",
                "只提出当前可用且已授权能力；不要虚构工具或改变权限。",
            ),
            Self::PrematureReply => (
                "premature_reply",
                "tool_intents非空时reply必须为空；需要先回复时仅回复并保存目标，稍后再研究。",
            ),
        }
    }
}

impl DecisionProposal {
    pub fn parse(raw: &[u8]) -> Result<Self, ProposalError> {
        let mut value = parse_strict_json(raw, 128 * 1024).map_err(|_| ProposalError::Schema)?;
        if let Some(error) = VALIDATOR.iter_errors(&value).next() {
            // Only names from our embedded schema cross the boundary. Never
            // format the error/instance: it may contain untrusted private text.
            let path = error.schema_path().to_string();
            for field in [
                "schema_version",
                "reply",
                "reply_evidence_ids",
                "memory_changes",
                "belief_changes",
                "autobiography_changes",
                "appraisal_changes",
                "attitude_changes",
                "relationship_changes",
                "entity_changes",
                "goal_changes",
                "tool_intents",
                "development",
                "brief_reason",
            ] {
                let prefix = format!("/properties/{field}/");
                if path.starts_with(&prefix) {
                    return Err(ProposalError::SchemaField(field));
                }
            }
            return Err(ProposalError::Schema);
        }
        // Validate the old wire shape first, then preserve every declared type.
        // Wrongly labelled IDs are never reclassified or silently discarded.
        if let Some(goals) = value["goal_changes"].as_array_mut() {
            for goal in goals {
                if goal["action"] == "create" && goal.get("origins").is_none() {
                    let kind = goal["origin_type"].clone();
                    if kind == "event"
                        && goal["origin_ids"]
                            .as_array()
                            .ok_or(ProposalError::GoalOrigin)?
                            .iter()
                            .any(|id| id.as_str().and_then(|s| Uuid::parse_str(s).ok()).is_none())
                    {
                        return Err(ProposalError::GoalOrigin);
                    }
                    let origins: Vec<_> = goal["origin_ids"]
                        .as_array()
                        .ok_or(ProposalError::GoalOrigin)?
                        .iter()
                        .map(|id| serde_json::json!({"origin_type":kind,"origin_id":id}))
                        .collect();
                    let object = goal.as_object_mut().ok_or(ProposalError::GoalOrigin)?;
                    object.remove("origin_type");
                    object.remove("origin_ids");
                    object.insert("origins".into(), serde_json::Value::Array(origins));
                }
            }
        }
        let proposal: Self = serde_json::from_value(value).map_err(|_| ProposalError::Schema)?;
        if proposal.brief_reason.trim().is_empty() {
            return Err(ProposalError::Schema);
        }
        Ok(proposal)
    }

    pub fn validate_context(&self, context: &ModelContext) -> Result<(), ProposalError> {
        if context.goal_review.is_some()
            || context.decision_review.is_some()
            || context.trial_binding.is_some()
        {
            return Err(ProposalError::UnverifiedCompletion);
        }
        // Typed providers are still untrusted; direct construction must not bypass schema checks.
        Self::parse(&serde_json::to_vec(self).map_err(|_| ProposalError::Schema)?)?;
        if let Some(draft) = &self.development {
            super::development_proposals::validate(context, draft)?;
        }
        if !self.tool_intents.is_empty()
            && self
                .goal_changes
                .iter()
                .any(|g| matches!(g, GoalChange::Wait { .. } | GoalChange::Abandon { .. }))
        {
            return Err(ProposalError::GoalActionConflict);
        }
        let visible: HashSet<_> = context
            .events
            .iter()
            .filter(|e| e.kind != "decision.reviewed")
            .map(|e| e.id)
            .chain(context.memories.iter().map(|m| m.id))
            .chain(
                context
                    .selected_beliefs
                    .iter()
                    .flat_map(|b| b.items.iter().map(|b| b.id)),
            )
            .chain(
                context
                    .autobiographical_context
                    .iter()
                    .flat_map(|s| s.entries.iter().map(|a| a.id)),
            )
            .collect();
        if context
            .selected_beliefs
            .as_ref()
            .is_some_and(|b| !b.valid_for(context))
            || context
                .autobiographical_context
                .as_ref()
                .is_some_and(|a| !a.valid_for(context))
        {
            return Err(ProposalError::UnknownEvidence);
        }
        let mut visible = visible;
        if let Some(social) = &context.social_context {
            if !social.valid_for(context) {
                return Err(ProposalError::UnknownEvidence);
            }
            visible.extend(social.ids());
        }
        let evidence_valid = |ids: &[Uuid]| ids.iter().all(|id| visible.contains(id));
        let mut predecessors = HashSet::new();
        if self.belief_changes.len() > 3 {
            return Err(ProposalError::Belief);
        }
        for belief in &self.belief_changes {
            if belief.supersedes.is_some_and(|id| !predecessors.insert(id)) {
                return Err(ProposalError::Belief);
            }
            belief
                .validate_context(context)
                .map_err(|_| ProposalError::Belief)?;
        }
        let mut topics = HashSet::new();
        if self.autobiography_changes.len() > 2 {
            return Err(ProposalError::Autobiography);
        }
        for interpretation in &self.autobiography_changes {
            if !topics.insert(&interpretation.topic) {
                return Err(ProposalError::Autobiography);
            }
            interpretation
                .validate_context(context)
                .map_err(|_| ProposalError::Autobiography)?;
        }
        if self.appraisal_changes.len() > 1
            || self.attitude_changes.len() > 2
            || self.relationship_changes.len() > 2
        {
            return Err(ProposalError::Social);
        }
        for change in &self.appraisal_changes {
            change
                .validate_context(context)
                .map_err(|_| ProposalError::Social)?;
        }
        let mut topics = HashSet::new();
        for change in &self.attitude_changes {
            if !topics.insert(&change.topic) {
                return Err(ProposalError::Social);
            }
            change
                .validate_context(context)
                .map_err(|_| ProposalError::Social)?;
        }
        let mut subjects = HashSet::new();
        for change in &self.relationship_changes {
            if !subjects.insert(change.subject_id) {
                return Err(ProposalError::Social);
            }
            change
                .validate_context(context)
                .map_err(|_| ProposalError::Social)?;
        }
        if !evidence_valid(&self.reply_evidence_ids) {
            return Err(ProposalError::UnknownEvidence);
        }
        for memory in &self.memory_changes {
            if memory.content.trim().is_empty() {
                return Err(ProposalError::Schema);
            }
            if memory.scope != "general" && memory.scope != context.current_scope {
                return Err(ProposalError::Scope);
            }
            let sources: Vec<_> = memory
                .source_event_ids
                .iter()
                .map(|id| {
                    context
                        .events
                        .iter()
                        .find(|e| e.id == *id && e.kind != "decision.reviewed")
                })
                .collect();
            if sources.iter().any(|e| e.is_none()) {
                return Err(ProposalError::UnknownEvidence);
            }
            if matches!(
                memory.kind,
                super::history::MemoryKind::Episodic | super::history::MemoryKind::Autobiographical
            ) && !sources.iter().flatten().any(|e| {
                matches!(
                    e.source_kind,
                    SourceKind::DirectObservation | SourceKind::ReportedClaim
                )
            }) {
                return Err(ProposalError::UnobservedExperience);
            }
            if let Some(id) = memory.supersedes {
                let previous = context
                    .memories
                    .iter()
                    .find(|m| m.id == id)
                    .ok_or(ProposalError::UnknownEvidence)?;
                if previous.scope != memory.scope || previous.validity == "superseded" {
                    return Err(ProposalError::Scope);
                }
            }
        }
        super::entities::validate_proposals(&self.entity_changes, &self.memory_changes, context)
            .map_err(ProposalError::EntityProposal)?;
        let mut changed_goals = HashSet::new();
        for change in &self.goal_changes {
            match change {
                GoalChange::Create {
                    origins,
                    description,
                    success_criterion,
                    next_step,
                    stop_condition,
                    completion,
                    ..
                } => {
                    if [description, success_criterion, next_step, stop_condition]
                        .iter()
                        .any(|s| s.trim().is_empty())
                    {
                        return Err(ProposalError::Goal);
                    }
                    if let Some(contract) = completion {
                        let doc = contract.citation().document_id;
                        if !contract.valid_for(success_criterion)
                            || !context.events.iter().any(|e| {
                                (e.id == doc
                                    && matches!(
                                        e.kind.as_str(),
                                        "document.imported" | "document.fetched"
                                    ))
                                    || (e.kind == "tool.result"
                                        && e.payload["status"] == "succeeded"
                                        && e.payload["output"]["document_id"] == doc.to_string())
                            })
                        {
                            return Err(ProposalError::Goal);
                        }
                    }
                    for origin in origins {
                        let valid = match origin {
                            GoalOriginRef::Event(id) => context
                                .events
                                .iter()
                                .any(|e| e.id == *id && is_goal_origin_event(e)),
                            GoalOriginRef::Interest(id) => {
                                context.interests.iter().any(|interest| interest.id == *id)
                            }
                        };
                        if !valid {
                            return Err(ProposalError::GoalOrigin);
                        }
                    }
                }
                GoalChange::Activate {
                    goal_id,
                    reason,
                    evidence_ids,
                }
                | GoalChange::Wait {
                    goal_id,
                    reason,
                    evidence_ids,
                }
                | GoalChange::Complete {
                    goal_id,
                    reason,
                    evidence_ids,
                }
                | GoalChange::Abandon {
                    goal_id,
                    reason,
                    evidence_ids,
                } => {
                    if !changed_goals.insert(goal_id) || reason.trim().is_empty() {
                        return Err(ProposalError::Goal);
                    }
                    let goal = context
                        .goals
                        .iter()
                        .find(|g| g.id == *goal_id)
                        .ok_or(ProposalError::Goal)?;
                    if !evidence_valid(evidence_ids) {
                        return Err(ProposalError::UnknownEvidence);
                    }
                    // A deliberate owner wait is a control boundary. The model
                    // cannot undo it during an unrelated foreground discussion.
                    if goal.waiting_reason.as_deref() == Some("owner_wait") {
                        return Err(ProposalError::Goal);
                    }
                    let allowed = match change {
                        GoalChange::Activate { .. } => {
                            matches!(goal.status.as_str(), "proposed" | "waiting")
                        }
                        GoalChange::Wait { .. } => {
                            matches!(goal.status.as_str(), "proposed" | "active")
                        }
                        GoalChange::Complete { .. } => {
                            if !self.tool_intents.is_empty() {
                                return Err(ProposalError::CompletionActionConflict);
                            }
                            if evidence_ids.is_empty() {
                                return Err(ProposalError::CompletionEvidenceMissing);
                            }
                            if !context.verified_goal_completions.contains(goal_id) {
                                return Err(ProposalError::UnverifiedCompletion);
                            }
                            goal.status == "active"
                        }
                        GoalChange::Abandon { .. } => {
                            matches!(goal.status.as_str(), "proposed" | "active" | "waiting")
                        }
                        _ => false,
                    };
                    if !allowed {
                        return Err(ProposalError::Goal);
                    }
                }
            }
        }
        if !self.tool_intents.is_empty() && !self.reply.trim().is_empty() {
            return Err(ProposalError::PrematureReply);
        }
        for tool in &self.tool_intents {
            if !context
                .capabilities
                .iter()
                .any(|c| c.id == tool.capability_id && c.available && c.authorized)
            {
                return Err(ProposalError::Capability);
            }
        }
        Ok(())
    }
}

/// Deterministic lexical fallback for M1, not a claim of semantic search quality.
pub fn query_terms(query: &str) -> Vec<String> {
    let lowercase = query.to_lowercase();
    let mut terms = HashSet::new();
    for word in lowercase
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
    {
        if word.is_ascii() {
            terms.insert(word.to_owned());
        } else {
            let chars: Vec<_> = word.chars().collect();
            for pair in chars.windows(2) {
                terms.insert(pair.iter().collect());
            }
            if chars.len() == 1 {
                terms.insert(word.to_owned());
            }
        }
    }
    let mut terms: Vec<_> = terms.into_iter().collect();
    terms.sort();
    terms.truncate(64);
    terms
}

pub struct MemoryQuery {
    pub text: String,
    pub scope: Option<String>,
    pub subject_id: Option<Uuid>,
    pub at: DateTime<Utc>,
    pub limit: u32,
}
