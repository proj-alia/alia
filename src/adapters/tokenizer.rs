//! Pinned public GLM text tokenizer. No downloads, model keys, or prompt logging.
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
use tokenizers::Tokenizer;

pub const TOKENIZER_SHA256: &str =
    "19e773648cb4e65de8660ea6365e10acca112d42a854923df93db4a6f333a82d";
pub const TEMPLATE_SHA256: &str =
    "2614a3c896d53e265f564c23701173c0049f985dacfbae79fc4bcd9f5ba083f4";
const MAX_FILE: u64 = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("pinned tokenizer unavailable, incompatible, or input unsupported")]
pub struct CountingError;

// Not Debug: neither prompt content nor environment paths should enter logs.
pub struct PinnedTokenizer(Tokenizer);
impl PinnedTokenizer {
    pub fn load(path: &Path) -> Result<Self, CountingError> {
        let file = File::open(path).map_err(|_| CountingError)?;
        let meta = file.metadata().map_err(|_| CountingError)?;
        if !meta.is_file() || meta.len() > MAX_FILE {
            return Err(CountingError);
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CountingError)?;
        if bytes.len() as u64 > MAX_FILE
            || format!("{:x}", Sha256::digest(&bytes)) != TOKENIZER_SHA256
        {
            return Err(CountingError);
        }
        let tokenizer = Tokenizer::from_bytes(bytes).map_err(|_| CountingError)?;
        if tokenizer.get_truncation().is_some() || tokenizer.get_padding().is_some() {
            return Err(CountingError);
        }
        Ok(Self(tokenizer))
    }
    /// Complete pinned Jinja branch for exactly system/user strings, low effort,
    /// no tools/history, and add_generation_prompt=true. Verified against Jinja
    /// by check_model_budget.py. Count includes all role/control tokens.
    pub fn count(&self, system: &str, user: &str) -> Result<u64, CountingError> {
        if system.len().saturating_add(user.len()) > 256 * 1024 {
            return Err(CountingError);
        }
        let prompt = format!(
            "[gMASK]<sop><|system|>Reasoning Effort: Low<|system|>{system}<|user|>{user}<|assistant|><think>"
        );
        self.0
            .encode(prompt, false)
            .map(|e| e.len() as u64)
            .map_err(|_| CountingError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_or_missing_artifacts_fail_without_exposing_paths_or_contents() {
        let path = std::env::temp_dir().join(format!("alia-tokenizer-{}", uuid::Uuid::new_v4()));
        assert!(PinnedTokenizer::load(&path).is_err());
        std::fs::write(&path, b"PRIVATE_REJECTED_ARTIFACT").unwrap();
        let error = PinnedTokenizer::load(&path).err().unwrap();
        assert!(!error.to_string().contains("PRIVATE"));
        assert!(!error.to_string().contains(path.to_str().unwrap()));
        std::fs::remove_file(path).unwrap();
    }
}
