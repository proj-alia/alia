//! CPU-only, pinned local text encoder. No network or database access.
use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

pub use crate::domain::retrieval::{DIMENSION, ENCODER_PROFILE as MODEL_VERSION};
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("local embedding assets missing or incompatible")]
    Assets,
    #[error("embedding input invalid or too large; chunk before indexing")]
    Input,
    #[error("local embedding execution failed or output invalid")]
    Inference,
}
#[derive(Clone, Copy)]
pub enum InputKind {
    Query,
    Passage,
}
impl InputKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Query => "query: ",
            Self::Passage => "passage: ",
        }
    }
}
#[derive(Deserialize)]
struct Manifest {
    files: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    path: String,
    size: u64,
    sha256: String,
}
// No Debug: private text and local paths must not enter error logging.
pub struct LocalEmbedding {
    model: TextEmbedding,
}
type TextWindows = (Vec<(usize, usize)>, Vec<String>);
impl LocalEmbedding {
    pub fn load(directory: &Path) -> Result<Self, EmbeddingError> {
        let manifest: Manifest =
            serde_json::from_str(include_str!("../../config/embedding-assets.json"))
                .map_err(|_| EmbeddingError::Assets)?;
        let mut assets = std::collections::HashMap::new();
        for asset in manifest.files {
            let file = std::fs::File::open(directory.join(&asset.path))
                .map_err(|_| EmbeddingError::Assets)?;
            let meta = file.metadata().map_err(|_| EmbeddingError::Assets)?;
            if !meta.is_file() || meta.len() != asset.size {
                return Err(EmbeddingError::Assets);
            }
            let mut data = Vec::new();
            file.take(asset.size + 1)
                .read_to_end(&mut data)
                .map_err(|_| EmbeddingError::Assets)?;
            if data.len() as u64 != asset.size
                || format!("{:x}", Sha256::digest(&data)) != asset.sha256
            {
                return Err(EmbeddingError::Assets);
            }
            assets.insert(asset.path, data);
        }
        let mut take = |name: &str| assets.remove(name).ok_or(EmbeddingError::Assets);
        let onnx = take("onnx/model.onnx")?;
        let files = TokenizerFiles {
            tokenizer_file: take("tokenizer.json")?,
            config_file: take("config.json")?,
            special_tokens_map_file: take("special_tokens_map.json")?,
            tokenizer_config_file: take("tokenizer_config.json")?,
        };
        let model = TextEmbedding::try_new_from_user_defined(
            UserDefinedEmbeddingModel::new(onnx, files).with_pooling(Pooling::Mean),
            InitOptionsUserDefined::new()
                .with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)
                .with_max_length(512)
                .with_intra_threads(2),
        )
        .map_err(|_| EmbeddingError::Inference)?;
        Ok(Self { model })
    }
    /// Complete Unicode windows with overlap, reduced if real tokens exceed 512.
    pub fn encode_memory(
        &mut self,
        text: &str,
    ) -> Result<Vec<crate::domain::retrieval::IndexedChunk>, EmbeddingError> {
        use crate::domain::retrieval::IndexedChunk;
        if text.trim().is_empty() || text.contains('\0') || text.chars().count() > 2000 {
            return Err(EmbeddingError::Input);
        }
        let (ranges, texts) = self.windows(text, InputKind::Passage)?;
        let mut vectors = Vec::new();
        for batch in texts.chunks(32) {
            vectors.extend(self.encode(batch, InputKind::Passage)?);
        }
        Ok(ranges
            .into_iter()
            .zip(vectors)
            .map(|((start_char, end_char), vector)| IndexedChunk {
                start_char,
                end_char,
                vector,
            })
            .collect())
    }
    fn windows(&self, text: &str, kind: InputKind) -> Result<TextWindows, EmbeddingError> {
        let chars: Vec<char> = text.chars().collect();
        let mut counter = self.model.tokenizer.clone();
        counter
            .with_truncation(None)
            .map_err(|_| EmbeddingError::Input)?;
        counter.with_padding(None);
        let mut ranges = Vec::new();
        let mut texts = Vec::new();
        let mut start = 0;
        while start < chars.len() {
            let mut end = (start + 256).min(chars.len());
            let part = loop {
                let part: String = chars[start..end].iter().collect();
                let count = counter
                    .encode(format!("{}{part}", kind.prefix()), true)
                    .map_err(|_| EmbeddingError::Input)?
                    .len();
                if count <= 512 {
                    break part;
                }
                if end - start <= 1 {
                    return Err(EmbeddingError::Input);
                }
                end = start + (end - start) / 2;
            };
            if !part.trim().is_empty() {
                ranges.push((start, end));
                texts.push(part);
            }
            if ranges.len() > 128 {
                return Err(EmbeddingError::Input);
            }
            if end == chars.len() {
                break;
            }
            start = end.saturating_sub(64).max(start + 1);
        }
        Ok((ranges, texts))
    }
    /// Long queries use complete token-checked windows, never tokenizer truncation.
    pub fn encode_query(&mut self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        if text.trim().is_empty() || text.contains('\0') || text.chars().count() > 1000 {
            return Err(EmbeddingError::Input);
        }
        match self.encode(&[text.to_owned()], InputKind::Query) {
            Ok(mut vectors) => return vectors.pop().ok_or(EmbeddingError::Inference),
            Err(EmbeddingError::Input) => {}
            Err(e) => return Err(e),
        }
        let (_, parts) = self.windows(text, InputKind::Query)?;
        let mut sum = vec![0.0; DIMENSION];
        for batch in parts.chunks(32) {
            for vector in self.encode(batch, InputKind::Query)? {
                for (total, value) in sum.iter_mut().zip(vector) {
                    *total += value;
                }
            }
        }
        let norm = sum.iter().map(|v| v * v).sum::<f32>().sqrt();
        if !norm.is_finite() || norm <= f32::EPSILON {
            return Err(EmbeddingError::Inference);
        }
        for value in &mut sum {
            *value /= norm;
        }
        if !crate::domain::retrieval::valid_vector(&sum) {
            return Err(EmbeddingError::Inference);
        }
        Ok(sum)
    }
    pub fn encode(
        &mut self,
        texts: &[String],
        kind: InputKind,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty()
            || texts.len() > 32
            || texts.iter().map(String::len).sum::<usize>() > 256 * 1024
        {
            return Err(EmbeddingError::Input);
        }
        let mut counter = self.model.tokenizer.clone();
        counter
            .with_truncation(None)
            .map_err(|_| EmbeddingError::Input)?;
        counter.with_padding(None);
        let mut inputs = Vec::with_capacity(texts.len());
        for text in texts {
            if text.trim().is_empty() || text.len() > 16384 || text.contains('\0') {
                return Err(EmbeddingError::Input);
            }
            let input = format!("{}{text}", kind.prefix());
            let encoded = counter
                .encode(input.as_str(), true)
                .map_err(|_| EmbeddingError::Input)?;
            if encoded.len() > 512 {
                return Err(EmbeddingError::Input);
            }
            inputs.push(input);
        }
        let vectors = self
            .model
            .embed(inputs, Some(8))
            .map_err(|_| EmbeddingError::Inference)?;
        if vectors.len() != texts.len() {
            return Err(EmbeddingError::Inference);
        }
        for v in &vectors {
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if v.len() != DIMENSION
                || v.iter().any(|x| !x.is_finite())
                || !(0.999..=1.001).contains(&norm)
            {
                return Err(EmbeddingError::Inference);
            }
        }
        Ok(vectors)
    }
}
/// One CPU operation at a time, including operations whose async waiter timed out.
/// Dropping the waiter prevents persistence, but cannot interrupt native ONNX code.
pub struct CpuMemoryEncoder {
    model: std::sync::Arc<std::sync::Mutex<LocalEmbedding>>,
    gate: std::sync::Arc<tokio::sync::Semaphore>,
}
impl CpuMemoryEncoder {
    pub fn new(model: LocalEmbedding) -> Self {
        Self {
            model: std::sync::Arc::new(std::sync::Mutex::new(model)),
            gate: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }
}
#[async_trait::async_trait]
impl crate::runtime::indexing::MemoryEncoder for CpuMemoryEncoder {
    async fn encode_query(
        &self,
        text: String,
    ) -> Result<Vec<f32>, crate::runtime::indexing::IndexEncodingError> {
        use crate::runtime::indexing::IndexEncodingError;
        let permit = self
            .gate
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| IndexEncodingError::Unavailable)?;
        let model = self.model.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            model
                .lock()
                .map_err(|_| IndexEncodingError::Unavailable)?
                .encode_query(&text)
                .map_err(|e| match e {
                    EmbeddingError::Input => IndexEncodingError::Invalid,
                    _ => IndexEncodingError::Unavailable,
                })
        })
        .await
        .map_err(|_| IndexEncodingError::Unavailable)?
    }
    async fn encode_memory(
        &self,
        text: String,
    ) -> Result<
        Vec<crate::domain::retrieval::IndexedChunk>,
        crate::runtime::indexing::IndexEncodingError,
    > {
        use crate::runtime::indexing::IndexEncodingError;
        let permit = self
            .gate
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| IndexEncodingError::Unavailable)?;
        let model = self.model.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            model
                .lock()
                .map_err(|_| IndexEncodingError::Unavailable)?
                .encode_memory(&text)
                .map_err(|e| match e {
                    EmbeddingError::Input => IndexEncodingError::Invalid,
                    _ => IndexEncodingError::Unavailable,
                })
        })
        .await
        .map_err(|_| IndexEncodingError::Unavailable)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_asset_is_rejected_without_path_disclosure() {
        let path = std::env::temp_dir().join(format!("PRIVATE_{}", uuid::Uuid::new_v4()));
        let error = LocalEmbedding::load(&path).err().unwrap().to_string();
        assert!(!error.contains("PRIVATE"));
    }
}
