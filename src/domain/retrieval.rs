//! Versioned derived vectors; not identity, beliefs, or model-written state.
use chrono::{DateTime, Utc};
use uuid::Uuid;
pub const DIMENSION: usize = 384;
pub const ENCODER_PROFILE: &str = "multilingual-e5-small/614241f622f53c4eeff9890bdc4f31cfecc418b3/onnx-f32/mean-l2/prefix-v1/fastembed7.7/ort2rc15/tok0.23.2";
pub const INDEX_PROFILE: &str =
    "e5-small-614241f6/f32-mean-l2-prefix1-fastembed7.7-ort2rc15-tok0.23.2/char256-overlap64-v1";
pub struct IndexedChunk {
    pub start_char: usize,
    pub end_char: usize,
    pub vector: Vec<f32>,
}
pub struct SemanticQuery {
    pub vector: Vec<f32>,
    pub profile: String,
    pub scope: Option<String>,
    pub subject_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub at: DateTime<Utc>,
    pub include_history: bool,
    pub limit: u32,
}
pub fn valid_vector(v: &[f32]) -> bool {
    v.len() == DIMENSION
        && v.iter().all(|x| x.is_finite())
        && (0.999..=1.001).contains(&v.iter().map(|x| x * x).sum::<f32>().sqrt())
}

#[derive(Clone)]
pub struct RetrievalRequest {
    pub text: String,
    pub scope: Option<String>,
    pub subject_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub person_name: Option<String>,
    pub at: DateTime<Utc>,
    pub include_history: bool,
    pub limit: u32,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RetrievalSummary {
    pub mode: String,
    pub profile: Option<String>,
    pub ranking: String,
    pub eligible: i64,
    pub indexed: i64,
    pub query_characters: usize,
    pub query_limited: bool,
    pub scope: Option<String>,
    pub person_id: Option<Uuid>,
    #[serde(default = "unrecorded_filter")]
    pub person_filter: String,
    pub include_history: bool,
    pub at: DateTime<Utc>,
}
fn unrecorded_filter() -> String {
    "unrecorded".into()
}
pub struct RetrievalPage {
    pub(crate) individual: Uuid,
    pub(crate) state_version: i64,
    pub(crate) privacy_epoch: i64,
    pub(crate) memories: Vec<crate::domain::history::Memory>,
    pub(crate) summary: RetrievalSummary,
    pub(crate) request: RetrievalRequest,
    pub(crate) person_evidence: Vec<Uuid>,
}
impl RetrievalPage {
    pub fn memories(&self) -> &[crate::domain::history::Memory] {
        &self.memories
    }
    pub fn summary(&self) -> &RetrievalSummary {
        &self.summary
    }
    pub fn state_version(&self) -> i64 {
        self.state_version
    }
}
