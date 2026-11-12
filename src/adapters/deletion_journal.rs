//! Independent opaque-ID deletion intent journal. A durable intent is not completed erasure.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

const FORMAT: &str = "alia-deletion-journal/1";
const FORMAT_V2: &str = "alia-deletion-journal/2";
const MAX_RECORD: usize = 64 * 1024;
const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalIntent {
    pub operation_id: Uuid,
    pub individual_id: Uuid,
    pub target_memory_id: Uuid,
    pub expected_memory_version: i64,
    pub request_digest: String,
    pub root_event_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key_digest: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalAppend {
    pub sequence: u64,
    pub record_digest: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JournalError {
    #[error("invalid deletion journal intent")]
    InvalidIntent,
    #[error("invalid or incomplete deletion journal")]
    Corrupt,
    #[error("deletion journal operation conflict")]
    Conflict,
    #[error("unsafe deletion journal path or permissions")]
    UnsafePath,
    #[error("deletion journal size limit exceeded")]
    TooLarge,
    #[error("deletion journal I/O failed")]
    Io,
    #[error("deletion journal lock unavailable")]
    Lock,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    sequence: u64,
    previous_digest: Option<String>,
    intent: JournalIntent,
    record_digest: String,
}
fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn validate(intent: &JournalIntent) -> Result<(), JournalError> {
    let ids = [
        intent.operation_id,
        intent.individual_id,
        intent.target_memory_id,
    ];
    if ids.iter().any(Uuid::is_nil)
        || intent.expected_memory_version <= 0
        || !hex(&intent.request_digest)
        || intent
            .idempotency_key_digest
            .as_deref()
            .is_some_and(|v| !hex(v))
        || !(1..=256).contains(&intent.root_event_ids.len())
        || intent.root_event_ids.iter().any(Uuid::is_nil)
        || intent.root_event_ids.iter().collect::<HashSet<_>>().len() != intent.root_event_ids.len()
    {
        return Err(JournalError::InvalidIntent);
    }
    Ok(())
}
fn record_digest(record: &Envelope) -> Result<String, JournalError> {
    let mut canonical = serde_json::to_value(record).map_err(|_| JournalError::Corrupt)?;
    canonical
        .as_object_mut()
        .ok_or(JournalError::Corrupt)?
        .remove("record_digest");
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical).map_err(|_| JournalError::Corrupt)?)
    ))
}
fn regular_private(file: &File) -> Result<(), JournalError> {
    let metadata = file.metadata().map_err(|_| JournalError::Io)?;
    if !metadata.is_file() || metadata.mode() & 0o7777 != 0o600 || metadata.nlink() != 1 {
        return Err(JournalError::UnsafePath);
    }
    Ok(())
}
fn safe_parent(path: &Path) -> Result<(), JournalError> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(JournalError::UnsafePath);
    }
    let parent = path.parent().ok_or(JournalError::UnsafePath)?;
    for ancestor in parent.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|_| JournalError::Io)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || (metadata.mode() & 0o022 != 0 && metadata.mode() & 0o1000 == 0)
        {
            return Err(JournalError::UnsafePath);
        }
    }
    let metadata = std::fs::symlink_metadata(parent).map_err(|_| JournalError::Io)?;
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        return Err(JournalError::UnsafePath);
    }
    Ok(())
}

pub struct DeletionJournal {
    path: PathBuf,
    parent: File,
    file: Mutex<File>,
}
impl DeletionJournal {
    pub fn open(path: &Path) -> Result<Self, JournalError> {
        Self::open_mode(path, true, true)
    }
    /// Restore must not recreate a lost journal, even if it disappears between
    /// an earlier metadata check and this open syscall.
    pub fn open_existing(path: &Path) -> Result<Self, JournalError> {
        Self::open_mode(path, false, true)
    }
    pub fn try_open_existing(path: &Path) -> Result<Self, JournalError> {
        Self::open_mode(path, false, false)
    }
    fn open_mode(
        path: &Path,
        allow_create: bool,
        wait_for_lock: bool,
    ) -> Result<Self, JournalError> {
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()
                .map_err(|_| JournalError::Io)?
                .join(path)
        };
        safe_parent(&path)?;
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(path.parent().ok_or(JournalError::UnsafePath)?)
            .map_err(|_| JournalError::UnsafePath)?;
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && (!metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(JournalError::UnsafePath);
        }
        let mut options = OpenOptions::new();
        options
            .read(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
        let (file, created) = if !allow_create {
            (
                options.open(&path).map_err(|_| JournalError::UnsafePath)?,
                false,
            )
        } else {
            match options.create_new(true).open(&path) {
                Ok(file) => (file, true),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (
                    options
                        .create_new(false)
                        .open(&path)
                        .map_err(|_| JournalError::UnsafePath)?,
                    false,
                ),
                Err(_) => return Err(JournalError::Io),
            }
        };
        regular_private(&file)?;
        let journal = Self {
            path,
            parent,
            file: Mutex::new(file),
        };
        let verify = |file: &mut File| {
            journal.read_disk(file)?;
            if created {
                file.sync_all().map_err(|_| JournalError::Io)?;
                journal.parent.sync_all().map_err(|_| JournalError::Io)?;
            }
            Ok(())
        };
        if wait_for_lock {
            journal.with_lock(verify)?;
        } else {
            journal.try_with_lock(verify)?;
        }
        Ok(journal)
    }
    fn path_matches(&self, file: &File) -> Result<(), JournalError> {
        safe_parent(&self.path)?;
        regular_private(file)?;
        let named = std::fs::symlink_metadata(&self.path).map_err(|_| JournalError::UnsafePath)?;
        let actual = file.metadata().map_err(|_| JournalError::Io)?;
        let parent = std::fs::symlink_metadata(self.path.parent().ok_or(JournalError::UnsafePath)?)
            .map_err(|_| JournalError::UnsafePath)?;
        let opened_parent = self.parent.metadata().map_err(|_| JournalError::Io)?;
        if named.file_type().is_symlink()
            || (named.dev(), named.ino()) != (actual.dev(), actual.ino())
            || (parent.dev(), parent.ino()) != (opened_parent.dev(), opened_parent.ino())
            || actual.uid() != parent.uid()
        {
            return Err(JournalError::UnsafePath);
        }
        Ok(())
    }
    fn with_lock<T>(
        &self,
        action: impl FnOnce(&mut File) -> Result<T, JournalError>,
    ) -> Result<T, JournalError> {
        let mut file = self.file.lock().map_err(|_| JournalError::Lock)?;
        file.lock().map_err(|_| JournalError::Lock)?;
        let result = self.path_matches(&file).and_then(|()| action(&mut file));
        let unlock = file.unlock().map_err(|_| JournalError::Lock);
        result.and_then(|value| {
            unlock?;
            Ok(value)
        })
    }
    fn try_with_lock<T>(
        &self,
        action: impl FnOnce(&mut File) -> Result<T, JournalError>,
    ) -> Result<T, JournalError> {
        let mut file = self.file.try_lock().map_err(|_| JournalError::Lock)?;
        file.try_lock().map_err(|_| JournalError::Lock)?;
        let result = self.path_matches(&file).and_then(|()| action(&mut file));
        let unlock = file.unlock().map_err(|_| JournalError::Lock);
        result.and_then(|value| {
            unlock?;
            Ok(value)
        })
    }
    fn read_disk(&self, file: &mut File) -> Result<Vec<Envelope>, JournalError> {
        if file.metadata().map_err(|_| JournalError::Io)?.len() > MAX_BYTES as u64 {
            return Err(JournalError::TooLarge);
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|_| JournalError::Io)?;
        let mut bytes = Vec::new();
        (&mut *file)
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| JournalError::Io)?;
        if bytes.len() > MAX_BYTES {
            return Err(JournalError::TooLarge);
        }
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            return Err(JournalError::Corrupt);
        }
        let mut records = Vec::new();
        let mut previous = None;
        let mut operations = HashMap::new();
        if bytes.is_empty() {
            self.path_matches(file)?;
            return Ok(records);
        }
        for line in bytes[..bytes.len() - 1].split(|b| *b == b'\n') {
            if line.len() + 1 > MAX_RECORD {
                return Err(JournalError::TooLarge);
            }
            let record: Envelope =
                serde_json::from_slice(line).map_err(|_| JournalError::Corrupt)?;
            validate(&record.intent).map_err(|_| JournalError::Corrupt)?;
            let raw: serde_json::Value =
                serde_json::from_slice(line).map_err(|_| JournalError::Corrupt)?;
            if raw != serde_json::to_value(&record).map_err(|_| JournalError::Corrupt)?
                || !matches!(
                    (
                        record.format.as_str(),
                        record.intent.idempotency_key_digest.is_some()
                    ),
                    (FORMAT, false) | (FORMAT_V2, true)
                )
                || record.sequence != records.len() as u64 + 1
                || record.previous_digest != previous
                || !hex(&record.record_digest)
                || record_digest(&record)? != record.record_digest
                || operations.insert(record.intent.operation_id, ()).is_some()
            {
                return Err(JournalError::Corrupt);
            }
            previous = Some(record.record_digest.clone());
            records.push(record);
        }
        self.path_matches(file)?;
        Ok(records)
    }
    pub fn try_records(&self) -> Result<Vec<JournalIntent>, JournalError> {
        self.try_with_lock(|file| {
            Ok(self
                .read_disk(file)?
                .into_iter()
                .map(|r| r.intent)
                .collect())
        })
    }
    pub fn records(&self) -> Result<Vec<JournalIntent>, JournalError> {
        self.with_lock(|file| {
            Ok(self
                .read_disk(file)?
                .into_iter()
                .map(|record| record.intent)
                .collect())
        })
    }
    pub fn append(&self, intent: &JournalIntent) -> Result<JournalAppend, JournalError> {
        self.append_mode(intent, false)
    }
    pub fn try_append(&self, intent: &JournalIntent) -> Result<JournalAppend, JournalError> {
        self.append_mode(intent, true)
    }
    fn append_mode(
        &self,
        intent: &JournalIntent,
        nonblocking: bool,
    ) -> Result<JournalAppend, JournalError> {
        validate(intent)?;
        let action = |file: &mut File| {
            let records = self.read_disk(file)?;
            if let Some(record) = records
                .iter()
                .find(|r| r.intent.operation_id == intent.operation_id)
            {
                if record.intent != *intent {
                    return Err(JournalError::Conflict);
                }
                file.sync_all().map_err(|_| JournalError::Io)?;
                self.parent.sync_all().map_err(|_| JournalError::Io)?;
                self.path_matches(file)?;
                return Ok(JournalAppend {
                    sequence: record.sequence,
                    record_digest: record.record_digest.clone(),
                });
            }
            let mut record = Envelope {
                format: if intent.idempotency_key_digest.is_some() {
                    FORMAT_V2
                } else {
                    FORMAT
                }
                .into(),
                sequence: records.len() as u64 + 1,
                previous_digest: records.last().map(|r| r.record_digest.clone()),
                intent: intent.clone(),
                record_digest: String::new(),
            };
            record.record_digest = record_digest(&record)?;
            let mut bytes = serde_json::to_vec(
                &serde_json::to_value(&record).map_err(|_| JournalError::Corrupt)?,
            )
            .map_err(|_| JournalError::Corrupt)?;
            bytes.push(b'\n');
            if bytes.len() > MAX_RECORD
                || file.metadata().map_err(|_| JournalError::Io)?.len() + bytes.len() as u64
                    > MAX_BYTES as u64
            {
                return Err(JournalError::TooLarge);
            }
            file.write_all(&bytes).map_err(|_| JournalError::Io)?;
            file.sync_all().map_err(|_| JournalError::Io)?;
            self.parent.sync_all().map_err(|_| JournalError::Io)?;
            self.path_matches(file)?;
            Ok(JournalAppend {
                sequence: record.sequence,
                record_digest: record.record_digest,
            })
        };
        if nonblocking {
            self.try_with_lock(action)
        } else {
            self.with_lock(action)
        }
    }
}
