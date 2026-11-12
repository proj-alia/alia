//! A source-grounded person reference is not a relationship or a new individual.
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub event_id: Uuid,
    pub start_char: usize,
    pub end_char: usize,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonRole {
    Subject,
    Mentioned,
}
impl PersonRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Subject => "subject",
            Self::Mentioned => "mentioned",
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryPersonAssertion {
    pub memory_id: Uuid,
    pub memory_version: i64,
    pub entity_id: Uuid,
    pub role: PersonRole,
    pub source: SourceSpan,
    pub memory_start_char: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub enum PersonResolution {
    Unknown,
    Unique(Uuid),
    Ambiguous,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct EntityReceipt {
    pub entity_id: Uuid,
    pub event_id: Uuid,
    pub version: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MemoryTarget {
    Existing { id: Uuid },
    Created { index: usize },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PersonTarget {
    Existing { id: Uuid },
    New { name: String },
    Prior { index: usize },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityProposal {
    pub memory: MemoryTarget,
    pub person: PersonTarget,
    pub role: PersonRole,
    pub source: SourceSpan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_start_char: Option<usize>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PersonRef {
    pub id: Uuid,
    pub label: String,
    pub aliases: Vec<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct PersonCatalog {
    pub items: Vec<PersonRef>,
    pub has_more: bool,
}

/// A saved, source-grounded assertion; its role is not inferred from name similarity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPersonRef {
    pub entity_id: Uuid,
    pub label: String,
    pub role: PersonRole,
    pub assertion_event_id: Uuid,
    pub source_event_id: Uuid,
    pub actor_id: String,
    pub source_kind: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
#[error("person evidence validation failed")]
pub enum EntityValidationError {
    SourceAbsent,
    SourceKind,
    UnobservedText,
    SourceRange,
    Name,
    MemoryReference,
    MemorySource,
    MemoryQuote,
    PersonReference,
    KnownName,
    PriorReference,
}
impl EntityValidationError {
    pub fn feedback(&self) -> (&'static str, &'static str) {
        match self {
            Self::SourceAbsent | Self::SourceKind | Self::UnobservedText => (
                "entity_unobserved_source",
                "人物来源只能是当前events中实际可见的消息text或成功tool.result的output.text；不能引用目录、未读全文或生成文本。",
            ),
            Self::SourceRange | Self::Name => (
                "entity_source_span",
                "source区间必须是名字本身，按Unicode字符从0计数、end不包含在区间中；不能含标点或多余字。",
            ),
            Self::MemoryReference => (
                "entity_memory_reference",
                "created.index必须对应本轮memory_changes下标；existing.id必须是当前可见记忆。",
            ),
            Self::MemorySource => (
                "entity_memory_source",
                "人物source.event_id必须也在对应记忆source_event_ids中。",
            ),
            Self::MemoryQuote => (
                "entity_memory_quote",
                "记忆正文必须逐字包含source中的同一名字。唯一出现时省略memory_start_char由程序定位；多次出现时须给出记忆正文的Unicode偏移，不能沿用来源偏移。",
            ),
            Self::PersonReference => (
                "entity_unknown_person",
                "existing人物ID及别名必须来自person_catalog；不能编造UUID。",
            ),
            Self::KnownName => (
                "entity_name_mismatch",
                "new.name必须逐字等于source区间；已知名字使用existing ID，不确定同名关系时省略关联，不宣称登记完成。",
            ),
            Self::PriorReference => (
                "entity_prior_reference",
                "prior.index只能引用更早的人物提案，并且引用的别名必须相同；不能自引用或前向引用。",
            ),
        }
    }
}

/// Resolve only literal matches; an omitted offset must identify one occurrence.
/// Enumerating character starts also detects overlapping matches.
pub fn memory_name_offset(
    content: &str,
    name: &str,
    explicit: Option<usize>,
) -> Result<usize, EntityValidationError> {
    if name.is_empty() {
        return Err(EntityValidationError::MemoryQuote);
    }
    let mut matches = content
        .char_indices()
        .enumerate()
        .filter_map(|(index, (byte, _))| content[byte..].starts_with(name).then_some(index));
    if let Some(index) = explicit {
        return matches
            .any(|found| found == index)
            .then_some(index)
            .ok_or(EntityValidationError::MemoryQuote);
    }
    let first = matches.next().ok_or(EntityValidationError::MemoryQuote)?;
    if matches.next().is_some() {
        return Err(EntityValidationError::MemoryQuote);
    }
    Ok(first)
}

pub fn validate_proposals(
    proposals: &[EntityProposal],
    created: &[super::history::NewMemory],
    context: &super::cognition::ModelContext,
) -> Result<(), EntityValidationError> {
    use super::history::SourceKind;
    for (index, p) in proposals.iter().enumerate() {
        let event = context
            .events
            .iter()
            .find(|e| e.id == p.source.event_id)
            .ok_or(EntityValidationError::SourceAbsent)?;
        if !matches!(
            event.source_kind,
            SourceKind::DirectObservation
                | SourceKind::ReportedClaim
                | SourceKind::ExternalDocument
        ) {
            return Err(EntityValidationError::SourceKind);
        }
        let text = match event.kind.as_str() {
            "message.received" => event.payload["text"].as_str(),
            "tool.result" if event.payload["status"] == "succeeded" => {
                event.payload["output"]["text"].as_str()
            }
            _ => None,
        }
        .ok_or(EntityValidationError::UnobservedText)?;
        if p.source.start_char >= p.source.end_char || p.source.end_char - p.source.start_char > 80
        {
            return Err(EntityValidationError::SourceRange);
        }
        let chars: Vec<char> = text.chars().collect();
        let name: String = chars
            .get(p.source.start_char..p.source.end_char)
            .ok_or(EntityValidationError::SourceRange)?
            .iter()
            .collect();
        if name.trim().is_empty() || name.trim() != name || name.chars().any(char::is_control) {
            return Err(EntityValidationError::Name);
        }
        let (content, sources) = match p.memory {
            MemoryTarget::Existing { id } => {
                let m = context
                    .memories
                    .iter()
                    .find(|m| m.id == id)
                    .ok_or(EntityValidationError::MemoryReference)?;
                (&m.content, &m.source_event_ids)
            }
            MemoryTarget::Created { index } => {
                let m = created
                    .get(index)
                    .ok_or(EntityValidationError::MemoryReference)?;
                (&m.content, &m.source_event_ids)
            }
        };
        if !sources.contains(&event.id) {
            return Err(EntityValidationError::MemorySource);
        }
        memory_name_offset(content, &name, p.memory_start_char)?;
        match &p.person {
            PersonTarget::Existing { id } => {
                if !context
                    .person_catalog
                    .items
                    .iter()
                    .any(|person| person.id == *id && person.aliases.contains(&name))
                {
                    return Err(EntityValidationError::PersonReference);
                }
            }
            PersonTarget::New { name: label } => {
                if label != &name
                    || context
                        .person_catalog
                        .items
                        .iter()
                        .any(|p| p.aliases.contains(label))
                {
                    return Err(EntityValidationError::KnownName);
                }
            }
            PersonTarget::Prior { index: prior } => {
                if *prior >= index {
                    return Err(EntityValidationError::PriorReference);
                }
                let previous = &proposals[*prior];
                let same = match &previous.person {
                    PersonTarget::New { name: label } => label == &name,
                    PersonTarget::Existing { id } => context
                        .person_catalog
                        .items
                        .iter()
                        .any(|p| p.id == *id && p.aliases.contains(&name)),
                    PersonTarget::Prior { .. } => false,
                };
                if !same {
                    return Err(EntityValidationError::PriorReference);
                }
            }
        }
    }
    Ok(())
}
