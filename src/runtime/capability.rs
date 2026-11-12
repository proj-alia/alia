//! Actual installed local capabilities. Registration is distinct from permission.
use crate::domain::cognition::CapabilityInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const LOCAL_CAPABILITIES: &[&str] = &["documents.read", "notes.write", "memory.search"];
pub const IMPLEMENTED_CAPABILITIES: &[&str] = &[
    "documents.read",
    "notes.write",
    "memory.search",
    "web.read_allowed",
];
pub const LOCAL_VERSION: &str = "1";

pub trait Capability {
    fn info(&self) -> CapabilityInfo;
    fn validate_input(&self, input: &Value) -> Result<(), ToolInputError>;
}

#[derive(Clone, Copy)]
pub enum LocalCapability {
    ReadDocument,
    WriteNote,
    SearchMemory,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid capability input")]
pub struct ToolInputError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadDocument {
    pub document_id: Uuid,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "document_chars")]
    pub max_chars: usize,
}
fn document_chars() -> usize {
    4096
}
fn memory_limit() -> u32 {
    5
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteNote {
    pub title: String,
    pub text: String,
    #[serde(default)]
    pub citations: Vec<crate::domain::completion::Citation>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchMemory {
    pub query: String,
    #[serde(default = "memory_limit")]
    pub limit: u32,
    pub person_id: Option<Uuid>,
    pub person_name: Option<String>,
}
pub(crate) enum LocalInput {
    ReadDocument(ReadDocument),
    WriteNote(WriteNote),
    SearchMemory(SearchMemory),
}

impl LocalCapability {
    pub fn resolve(id: &str) -> Option<Self> {
        match id {
            "documents.read" => Some(Self::ReadDocument),
            "notes.write" => Some(Self::WriteNote),
            "memory.search" => Some(Self::SearchMemory),
            _ => None,
        }
    }
    pub(crate) fn parse(&self, input: &Value) -> Result<LocalInput, ToolInputError> {
        match self {
            Self::ReadDocument => {
                let p: ReadDocument =
                    serde_json::from_value(input.clone()).map_err(|_| ToolInputError)?;
                if p.offset > 1048576 || !(1..=4096).contains(&p.max_chars) {
                    return Err(ToolInputError);
                }
                Ok(LocalInput::ReadDocument(p))
            }
            Self::WriteNote => {
                let p: WriteNote =
                    serde_json::from_value(input.clone()).map_err(|_| ToolInputError)?;
                if p.title.trim().is_empty()
                    || p.title.len() > 512
                    || p.title.chars().count() > 200
                    || p.text.trim().is_empty()
                    || p.text.len() > 16384
                    || p.text.chars().count() > 12000
                    || p.title.contains('\0')
                    || p.text.contains('\0')
                    || p.citations.len() > 8
                    || p.citations.iter().any(|c| !c.valid())
                {
                    return Err(ToolInputError);
                }
                Ok(LocalInput::WriteNote(p))
            }
            Self::SearchMemory => {
                let p: SearchMemory =
                    serde_json::from_value(input.clone()).map_err(|_| ToolInputError)?;
                if p.query.trim().is_empty()
                    || p.query.chars().count() > 1000
                    || p.query.contains('\0')
                    || !(1..=5).contains(&p.limit)
                    || (p.person_id.is_some() && p.person_name.is_some())
                    || p.person_name.as_ref().is_some_and(|name| {
                        name.trim().is_empty()
                            || name != name.trim()
                            || name.chars().count() > 80
                            || name.chars().any(char::is_control)
                    })
                {
                    return Err(ToolInputError);
                }
                Ok(LocalInput::SearchMemory(p))
            }
        }
    }
}
impl Capability for LocalCapability {
    fn info(&self) -> CapabilityInfo {
        let (id, description) = match self {
            Self::ReadDocument => (
                "documents.read",
                "读取明确导入的纯文本。参数 {document_id: UUID, offset?: 非负Unicode字符位置默认0, max_chars?: 1..4096默认4096}；返回分段与下一位置；text不超过2048字符时附可直接引用的citation，否则为null。需要短引文时设置max_chars≤2048。不读取系统路径。",
            ),
            Self::WriteNote => (
                "notes.write",
                "创建个人笔记，数据库按动作ID去重。参数 {title: 非空标题≤200字符且≤512 UTF-8字节, text: 非空正文≤12000字符且≤16384 UTF-8字节, citations?: [{document_id: UUID,start_char,end_char}]}。引用使用Unicode字符半开区间，最多8条、每条1..2048字符；正文须包含该范围完整连续原文（不能删句或分散引用）；可复制documents.read返回的citation并逐字保留text，分析另写。且当前运行、该目标的形成运行或同目标先前循环实际读过完整范围。标题不是文件路径，不执行代码。",
            ),
            Self::SearchMemory => (
                "memory.search",
                "检索当前任务与一般范围内的有效记忆。参数 {query: 非空文本≤1000字符, limit?: 1..5默认5, person_id?: 当前person_catalog中的人物UUID, person_name?: 姓名或已保存别名≤80字符}。person_id与person_name互斥；指定人物只检索事实主体关联。姓名必须逐字匹配，不做近名合并；同名返回person_ambiguous，未知姓名返回person_unavailable，不回退全库。目录截断时可用明确姓名查询，解析不创建人物。返回真实来源、主体/提及标注、实际检索模式和索引覆盖。编码失败明确报告，不静默降级。",
            ),
        };
        CapabilityInfo {
            id: id.into(),
            version: if matches!(self, Self::SearchMemory) {
                "3"
            } else {
                LOCAL_VERSION
            }
            .into(),
            available: true,
            authorized: false,
            description: description.into(),
        }
    }
    fn validate_input(&self, input: &Value) -> Result<(), ToolInputError> {
        self.parse(input).map(|_| ())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportedDocument {
    pub event_id: Uuid,
    pub state_version: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionReceipt {
    pub action_id: Uuid,
    pub result_event_id: Uuid,
    pub status: String,
    pub state_version: i64,
    pub executed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn capabilities_reject_host_paths_commands_and_invalid_ranges() {
        assert!(
            LocalCapability::ReadDocument
                .validate_input(&json!({"path":"/etc/passwd"}))
                .is_err()
        );
        assert!(
            LocalCapability::WriteNote
                .validate_input(&json!({"title":"x","text":"x","command":"anything"}))
                .is_err()
        );
        for max in [0, 4097] {
            assert!(
                LocalCapability::ReadDocument
                    .validate_input(&json!({"document_id":Uuid::new_v4(),"max_chars":max}))
                    .is_err()
            );
        }
        assert!(
            LocalCapability::SearchMemory
                .validate_input(&json!({"query":"x","limit":6}))
                .is_err()
        );
        assert!(
            LocalCapability::ReadDocument
                .validate_input(&json!({"document_id":Uuid::new_v4()}))
                .is_ok()
        );
        assert!(
            LocalCapability::SearchMemory
                .validate_input(&json!({"query":"有效"}))
                .is_ok()
        );
    }
}
