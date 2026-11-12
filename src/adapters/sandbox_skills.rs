//! Private, unregistered skill vault. Root must separately enforce DB ownership,
//! source visibility/deletion, runtime permissions and the shared tool budget.
//! No caller supplies a host source/report path. No user Python runs on the host.
use super::sandbox::{
    DockerCommand, SKILL_PROTOCOL, SKILL_WRAPPER, SandboxExecutor, SandboxStatus,
    SkillSandboxOutcome, SkillSandboxRequest,
};
use crate::domain::config::parse_strict_json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const IMAGE: &str =
    "python@sha256:a5a7017efc5969e00ae2b91c8eb0631049a569652aa4ffc4501769049ae45c72";
pub const WRAPPER_VERSION: u32 = 1;
const SOURCE_LIMIT: usize = 24 * 1024;
const JSON_LIMIT: usize = 8 * 1024;
const RECORD_LIMIT: usize = 1024 * 1024;
const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum SkillError {
    #[error("invalid skill input")]
    Invalid,
    #[error("unsafe vault path")]
    UnsafePath,
    #[error("unsafe vault ownership, permissions or links")]
    UnsafeFile,
    #[error("vault operation deadline or lock contention")]
    Busy,
    #[error("vault integrity or audit transition failed")]
    Corrupt,
    #[error("incomplete durable vault transaction; recovery required")]
    Incomplete,
    #[error("vault I/O failed")]
    Io,
    #[error("skill not found")]
    NotFound,
    #[error("skill version or artifact changed")]
    Stale,
    #[error("skill revoked")]
    Revoked,
    #[error("skill lacks actual successful verification")]
    NotValidated,
    #[error("skill not registered")]
    NotRegistered,
    #[error("registration expired")]
    Expired,
    #[error("grant scope mismatch")]
    Scope,
    #[error("09 stdin script transport cannot contain full source and input")]
    TransportLimit,
    #[error("whole validation deadline exhausted")]
    Deadline,
    #[error("declared case or sandbox execution failed")]
    VerificationFailed,
    #[error("execution result is unknown or could not be durably committed")]
    Unknown,
}
impl From<std::io::Error> for SkillError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, SkillError> {
    serde_json::to_vec(value).map_err(|_| SkillError::Invalid)
}
fn closed<T: DeserializeOwned>(bytes: &[u8], cap: usize) -> Result<T, SkillError> {
    let value = parse_strict_json(bytes, cap).map_err(|_| SkillError::Corrupt)?;
    serde_json::from_value(value).map_err(|_| SkillError::Corrupt)
}
fn strict(text: &str) -> Result<Value, SkillError> {
    parse_strict_json(text.as_bytes(), JSON_LIMIT).map_err(|_| SkillError::Invalid)
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn execution_name(value: &str) -> bool {
    value.strip_prefix("alia-sandbox-").is_some_and(|v| {
        v.len() == 32
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateDraft {
    pub individual_id: Uuid,
    pub name: String,
    pub description: String,
    pub source: String,
    pub source_event_ids: Vec<Uuid>,
}
impl std::fmt::Debug for CandidateDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CandidateDraft")
            .field("individual_id", &self.individual_id)
            .field("source_bytes", &self.source.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillVersion {
    pub individual_id: Uuid,
    pub skill_id: Uuid,
    pub version: u32,
    pub source_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    name: String,
    description: String,
    source_event_ids: Vec<Uuid>,
}
fn validate_draft(draft: &CandidateDraft) -> Result<(), SkillError> {
    if draft.individual_id.is_nil()
        || draft.name.trim().is_empty()
        || draft.name.chars().count() > 80
        || draft.description.chars().count() > 2000
        || draft.name.contains('\0')
        || draft.description.contains('\0')
        || draft.source.is_empty()
        || draft.source.len() > SOURCE_LIMIT
        || draft.source.contains('\0')
        || !(1..=32).contains(&draft.source_event_ids.len())
        || draft.source_event_ids.iter().any(Uuid::is_nil)
        || draft.source_event_ids.iter().collect::<HashSet<_>>().len()
            != draft.source_event_ids.len()
    {
        return Err(SkillError::Invalid);
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    input_json: String,
    expected_json: String,
}
/// Construct only in the trusted test controller; never deserialize model cases.
#[derive(Clone, Debug)]
pub struct ValidationCases(Vec<Case>);
impl ValidationCases {
    /// Root must reserve this many slots from the EXISTING three-tool budget
    /// before starting validation. This never grants extra runtime allowance.
    pub fn sandbox_calls_upper_bound(&self) -> u32 {
        self.0.len() as u32
    }
    pub fn new(cases: Vec<(String, String, String)>) -> Result<Self, SkillError> {
        if !(1..=3).contains(&cases.len()) {
            return Err(SkillError::Invalid);
        }
        let mut result = Vec::new();
        let mut ids = HashSet::new();
        for (id, input_json, expected_json) in cases {
            if id.trim().is_empty()
                || id.chars().count() > 80
                || id.contains('\0')
                || !ids.insert(id.clone())
            {
                return Err(SkillError::Invalid);
            }
            strict(&input_json)?;
            strict(&expected_json)?;
            result.push(Case {
                id,
                input_json,
                expected_json,
            });
        }
        Ok(Self(result))
    }
}
fn check_cases(cases: &[Case]) -> Result<(), SkillError> {
    ValidationCases::new(
        cases
            .iter()
            .map(|c| (c.id.clone(), c.input_json.clone(), c.expected_json.clone()))
            .collect(),
    )
    .map(|_| ())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillPhase {
    Candidate,
    Validated,
    Registered,
    Revoked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    #[serde(rename = "sandbox.python")]
    SandboxPython,
}
/// Only a successfully replayed actual validation can construct this binding.
#[derive(Clone, Debug, Serialize)]
pub struct ValidatedSkill {
    version: SkillVersion,
    report_sha256: String,
    case_set_sha256: String,
}
impl ValidatedSkill {
    pub fn version(&self) -> &SkillVersion {
        &self.version
    }
    pub fn report_sha256(&self) -> &str {
        &self.report_sha256
    }
    pub fn case_set_sha256(&self) -> &str {
        &self.case_set_sha256
    }
}
/// Server-created Rust value, deliberately NOT Deserialize. This does not grant
/// runtime permission; the root controller must authorize before constructing it.
#[derive(Debug)]
pub struct RegistrationGrant(GrantRecord);
impl RegistrationGrant {
    pub fn issue(
        validated: &ValidatedSkill,
        expires_at: DateTime<Utc>,
        scope: SkillScope,
    ) -> Result<Self, SkillError> {
        if expires_at <= Utc::now() {
            return Err(SkillError::Expired);
        }
        Ok(Self(GrantRecord {
            id: Uuid::new_v4(),
            version: validated.version.clone(),
            report_sha256: validated.report_sha256.clone(),
            case_set_sha256: validated.case_set_sha256.clone(),
            image_digest: IMAGE.into(),
            wrapper_version: WRAPPER_VERSION,
            expires_ms: expires_at.timestamp_millis(),
            scope,
        }))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantRecord {
    id: Uuid,
    version: SkillVersion,
    report_sha256: String,
    case_set_sha256: String,
    image_digest: String,
    wrapper_version: u32,
    expires_ms: i64,
    scope: SkillScope,
}
#[derive(Clone, Debug, Serialize)]
pub struct DurableReceipt {
    pub sequence: u64,
    pub audit_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct RegistrationReceipt {
    pub version: SkillVersion,
    pub grant_id: Uuid,
    pub durable: DurableReceipt,
}
#[derive(Clone, Debug, Serialize)]
pub struct RevocationReceipt {
    pub version: SkillVersion,
    pub durable: DurableReceipt,
}
#[derive(Clone, Debug, Serialize)]
pub struct SkillView {
    pub version: SkillVersion,
    pub phase: SkillPhase,
    pub name: String,
    pub description: String,
    pub source_event_ids: Vec<Uuid>,
    pub verified: Option<ValidatedSkill>,
    pub registration_expires_at: Option<DateTime<Utc>>,
    pub pending_operations: Vec<Uuid>,
    pub pending_sandbox_calls_upper_bound: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEvidence {
    pub execution_id: String,
    pub status: SandboxStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub duration_ms: u64,
    pub image_digest: String,
    pub isolation_verified: bool,
    pub cleanup_passed: bool,
    pub protocol: String,
    pub source_sha256: String,
    pub input_sha256: String,
    pub wrapper_sha256: String,
}
impl From<SkillSandboxOutcome> for ExecutionEvidence {
    fn from(value: SkillSandboxOutcome) -> Self {
        let v = value.outcome;
        Self {
            execution_id: v.execution_id,
            status: v.status,
            exit_code: v.exit_code,
            stdout: v.stdout,
            stderr: v.stderr,
            output_truncated: v.output_truncated,
            duration_ms: v.duration_ms,
            image_digest: v.image_digest,
            isolation_verified: v.isolation_verified,
            cleanup_passed: v.cleanup_passed,
            protocol: value.protocol.into(),
            source_sha256: value.source_sha256,
            input_sha256: value.input_sha256,
            wrapper_sha256: value.wrapper_sha256,
        }
    }
}

fn evidence_shape(e: &ExecutionEvidence) -> bool {
    execution_name(&e.execution_id)
        && e.protocol == SKILL_PROTOCOL
        && e.wrapper_sha256 == hash(SKILL_WRAPPER.as_bytes())
        && valid_hash(&e.source_sha256)
        && valid_hash(&e.input_sha256)
        && e.image_digest == IMAGE
        && e.duration_ms <= 10000
        && e.stdout.len() <= 65536
        && e.stderr.len() <= 65536
}
fn result_json(e: &ExecutionEvidence) -> Result<Value, SkillError> {
    if !evidence_shape(e)
        || e.status != SandboxStatus::Succeeded
        || e.exit_code != Some(0)
        || !e.isolation_verified
        || !e.cleanup_passed
        || e.output_truncated
        || !e.stderr.is_empty()
    {
        return Err(SkillError::VerificationFailed);
    }
    parse_strict_json(e.stdout.as_bytes(), 65536).map_err(|_| SkillError::VerificationFailed)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completion {
    Complete,
    Failed,
    Stale,
    Revoked,
    Expired,
    Deadline,
    TransportLimit,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationReport {
    operation_id: Uuid,
    version: SkillVersion,
    image_digest: String,
    wrapper_version: u32,
    cases: Vec<Case>,
    case_set_sha256: String,
    protocol: String,
    wrapper_sha256: String,
    pub executions: Vec<ExecutionEvidence>,
    pub completion: Completion,
    pub elapsed_ms: u64,
}
#[derive(Debug, Serialize)]
pub struct ValidationAttempt {
    pub operation_id: Option<Uuid>,
    pub sandbox_calls: u32,
    pub attested_executions: u32,
    pub executions: Vec<ExecutionEvidence>,
    pub report_sha256: Option<String>,
    pub durable: bool,
    pub result: Result<ValidatedSkill, SkillError>,
}
#[derive(Debug, Serialize)]
pub struct InvocationAttempt {
    pub operation_id: Option<Uuid>,
    pub sandbox_calls: u32,
    pub attested_executions: u32,
    pub execution: Option<ExecutionEvidence>,
    pub durable: bool,
    pub result: Result<Value, SkillError>,
}

fn skill_request(
    source: &str,
    input_json: &str,
    timeout_ms: u64,
) -> Result<SkillSandboxRequest, SkillError> {
    if source.is_empty() || source.len() > SOURCE_LIMIT {
        return Err(SkillError::Invalid);
    }
    strict(input_json)?;
    Ok(SkillSandboxRequest {
        source: source.into(),
        input_json: input_json.into(),
        timeout_ms,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Head {
    format: u32,
    sequence: u64,
    last_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditEvent {
    format: u32,
    sequence: u64,
    previous_sha256: String,
    at_ms: i64,
    change: Change,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Change {
    Candidate {
        version: SkillVersion,
        metadata: Metadata,
    },
    ValidationStarted {
        operation_id: Uuid,
        version: SkillVersion,
        cases: Vec<Case>,
    },
    ValidationFinished {
        report: ValidationReport,
    },
    Registered {
        grant: GrantRecord,
    },
    Revoked {
        version: SkillVersion,
        reason: String,
    },
    InvocationStarted {
        operation_id: Uuid,
        version: SkillVersion,
        grant_id: Uuid,
        input_json: String,
        timeout_ms: u64,
    },
    InvocationFinished {
        operation_id: Uuid,
        execution: ExecutionEvidence,
        wrapper_sha256: String,
        completion: Completion,
        elapsed_ms: u64,
    },
}
#[derive(Clone)]
struct VersionState {
    version: SkillVersion,
    metadata: Metadata,
    phase: SkillPhase,
    epoch: u64,
    verified: Option<ValidatedSkill>,
    grant: Option<GrantRecord>,
}
#[derive(Clone)]
struct SkillState {
    individual: Uuid,
    latest: u32,
    versions: BTreeMap<u32, VersionState>,
}
#[derive(Clone)]
enum Pending {
    Validation {
        version: SkillVersion,
        cases: Vec<Case>,
        epoch: u64,
    },
    Invocation {
        version: SkillVersion,
        grant_id: Uuid,
        input_json: String,
        timeout_ms: u64,
        epoch: u64,
    },
}
#[derive(Clone)]
struct State {
    head: Head,
    skills: HashMap<Uuid, SkillState>,
    pending: HashMap<Uuid, Pending>,
    operations: HashSet<Uuid>,
    executions: HashSet<String>,
    grants: HashSet<Uuid>,
    reports: HashMap<Uuid, ValidationReport>,
}
impl State {
    fn empty() -> Self {
        Self {
            head: Head {
                format: 1,
                sequence: 0,
                last_sha256: ZERO.into(),
            },
            skills: HashMap::new(),
            pending: HashMap::new(),
            operations: HashSet::new(),
            executions: HashSet::new(),
            grants: HashSet::new(),
            reports: HashMap::new(),
        }
    }
    fn current(&self, version: &SkillVersion) -> Result<&VersionState, SkillError> {
        let skill = self
            .skills
            .get(&version.skill_id)
            .ok_or(SkillError::NotFound)?;
        if skill.individual != version.individual_id || skill.latest != version.version {
            return Err(SkillError::Stale);
        }
        let value = skill
            .versions
            .get(&version.version)
            .ok_or(SkillError::Stale)?;
        if value.version != *version {
            return Err(SkillError::Stale);
        }
        if value.phase == SkillPhase::Revoked {
            return Err(SkillError::Revoked);
        }
        Ok(value)
    }
    fn invocable(&self, version: &SkillVersion, now: i64) -> Result<&VersionState, SkillError> {
        let value = self.current(version)?;
        if value.phase != SkillPhase::Registered {
            return Err(SkillError::NotRegistered);
        }
        let grant = value.grant.as_ref().ok_or(SkillError::Corrupt)?;
        if grant.expires_ms <= now {
            return Err(SkillError::Expired);
        }
        Ok(value)
    }
}

pub struct SkillVault {
    root: PathBuf,
    directory: File,
    launcher: DockerCommand,
    uid: u32,
}
fn safe_absolute(path: &Path) -> Result<(), SkillError> {
    if !path.is_absolute() {
        return Err(SkillError::UnsafePath);
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        if !matches!(part, Component::RootDir | Component::Normal(_)) {
            return Err(SkillError::UnsafePath);
        }
        current.push(part.as_os_str());
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(SkillError::UnsafePath);
        }
    }
    Ok(())
}
fn check_meta(meta: &fs::Metadata, uid: u32, directory: bool) -> Result<(), SkillError> {
    if meta.uid() != uid
        || meta.mode() & 0o7777 != if directory { 0o700 } else { 0o600 }
        || if directory {
            !meta.is_dir()
        } else {
            !meta.is_file() || meta.nlink() != 1
        }
    {
        return Err(SkillError::UnsafeFile);
    }
    Ok(())
}
impl SkillVault {
    pub fn create_in(parent: &Path, launcher: DockerCommand) -> Result<Self, SkillError> {
        safe_absolute(parent)?;
        let uid = fs::metadata("/proc/self")?.uid();
        check_meta(&fs::symlink_metadata(parent)?, uid, true)?;
        let root = parent.join(format!("skills-{}", Uuid::new_v4().simple()));
        DirBuilder::new().mode(0o700).create(&root)?;
        let vault = Self::open_directory(&root, launcher, uid)?;
        vault.publish(".lock", b"")?;
        vault.publish(".head", &encoded(&State::empty().head)?)?;
        vault.directory.sync_all()?;
        File::open(parent)?.sync_all()?;
        Ok(vault)
    }
    fn open_directory(root: &Path, launcher: DockerCommand, uid: u32) -> Result<Self, SkillError> {
        safe_absolute(root)?;
        let before = fs::symlink_metadata(root)?;
        check_meta(&before, uid, true)?;
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(root)?;
        let after = directory.metadata()?;
        if (before.dev(), before.ino()) != (after.dev(), after.ino()) {
            return Err(SkillError::UnsafePath);
        }
        Ok(Self {
            root: root.to_owned(),
            directory,
            launcher,
            uid,
        })
    }
    pub fn open(root: &Path, launcher: DockerCommand) -> Result<Self, SkillError> {
        let vault = Self::open_directory(root, launcher, fs::metadata("/proc/self")?.uid())?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let _lock = vault.lock(deadline)?;
        vault.load(deadline)?;
        Ok(vault)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn anchor(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.directory.as_raw_fd()))
    }
    fn identity(&self) -> Result<(), SkillError> {
        safe_absolute(&self.root)?;
        let expected = self.directory.metadata()?;
        let current = fs::symlink_metadata(&self.root)?;
        check_meta(&current, self.uid, true)?;
        if (expected.dev(), expected.ino()) != (current.dev(), current.ino()) {
            return Err(SkillError::UnsafePath);
        }
        Ok(())
    }
    fn file(&self, name: &str) -> Result<File, SkillError> {
        let file = OpenOptions::new()
            .read(true)
            .write(name == ".lock")
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.anchor().join(name))?;
        check_meta(&file.metadata()?, self.uid, false)?;
        Ok(file)
    }
    fn read(&self, name: &str) -> Result<Vec<u8>, SkillError> {
        let mut file = self.file(name)?;
        if file.metadata()?.len() > RECORD_LIMIT as u64 {
            return Err(SkillError::Corrupt);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(RECORD_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > RECORD_LIMIT {
            return Err(SkillError::Corrupt);
        }
        Ok(bytes)
    }
    fn lock(&self, deadline: Instant) -> Result<File, SkillError> {
        self.identity()?;
        let file = self.file(".lock")?;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(file),
                Err(std::fs::TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(SkillError::Busy);
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(_) => return Err(SkillError::Io),
            }
        }
    }
    fn publish(&self, name: &str, bytes: &[u8]) -> Result<(), SkillError> {
        let pending = self
            .anchor()
            .join(format!(".pending-{}", Uuid::new_v4().simple()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&pending)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::hard_link(&pending, self.anchor().join(name))?;
        fs::remove_file(pending)?;
        self.directory.sync_all()?;
        Ok(())
    }
    fn head(&self, head: &Head) -> Result<(), SkillError> {
        let pending = self
            .anchor()
            .join(format!(".pending-{}", Uuid::new_v4().simple()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&pending)?;
        file.write_all(&encoded(head)?)?;
        file.sync_all()?;
        self.file(".head")?;
        fs::rename(pending, self.anchor().join(".head"))?;
        self.directory.sync_all()?;
        Ok(())
    }
    fn source_name(v: &SkillVersion) -> String {
        format!("source-{}-{}.py", v.skill_id.simple(), v.version)
    }
    fn source(&self, v: &SkillVersion) -> Result<String, SkillError> {
        let bytes = self.read(&Self::source_name(v))?;
        if bytes.is_empty() || bytes.len() > SOURCE_LIMIT || hash(&bytes) != v.source_sha256 {
            return Err(SkillError::Corrupt);
        }
        String::from_utf8(bytes).map_err(|_| SkillError::Corrupt)
    }
    fn load(&self, deadline: Instant) -> Result<State, SkillError> {
        self.identity()?;
        let head: Head = closed(&self.read(".head")?, 1024)?;
        if head.format != 1 || !valid_hash(&head.last_sha256) {
            return Err(SkillError::Corrupt);
        }
        let names = fs::read_dir(self.anchor())?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<Result<Vec<_>, _>>()?;
        if names
            .iter()
            .any(|n| n.to_string_lossy().starts_with(".pending-"))
        {
            return Err(SkillError::Incomplete);
        }
        let count = names
            .iter()
            .filter(|n| n.to_string_lossy().starts_with("event-"))
            .count();
        if count as u64 != head.sequence {
            return Err(SkillError::Incomplete);
        }
        let mut state = State::empty();
        let mut expected = HashSet::from([".lock".to_owned(), ".head".to_owned()]);
        for seq in 1..=head.sequence {
            if Instant::now() >= deadline {
                return Err(SkillError::Busy);
            }
            let name = format!("event-{seq:020}.json");
            let bytes = self.read(&name)?;
            expected.insert(name);
            let event: AuditEvent = closed(&bytes, RECORD_LIMIT)?;
            if event.format != 1
                || event.sequence != seq
                || event.previous_sha256 != state.head.last_sha256
            {
                return Err(SkillError::Corrupt);
            }
            self.apply(&mut state, &event)?;
            state.head = Head {
                format: 1,
                sequence: seq,
                last_sha256: hash(&bytes),
            };
        }
        if state.head.last_sha256 != head.last_sha256 {
            return Err(SkillError::Corrupt);
        }
        for skill in state.skills.values() {
            for version in skill.versions.values() {
                if Instant::now() >= deadline {
                    return Err(SkillError::Busy);
                }
                expected.insert(Self::source_name(&version.version));
                self.source(&version.version)?;
            }
        }
        if names
            .iter()
            .any(|n| !expected.contains(n.to_string_lossy().as_ref()))
            || expected.len() != names.len()
        {
            return Err(SkillError::Corrupt);
        }
        Ok(state)
    }
    fn append(&self, state: &mut State, change: Change) -> Result<DurableReceipt, SkillError> {
        let sequence = state
            .head
            .sequence
            .checked_add(1)
            .ok_or(SkillError::Corrupt)?;
        let event = AuditEvent {
            format: 1,
            sequence,
            previous_sha256: state.head.last_sha256.clone(),
            at_ms: Utc::now().timestamp_millis(),
            change,
        };
        let mut next = state.clone();
        self.apply(&mut next, &event)?;
        let bytes = encoded(&event)?;
        if bytes.len() > RECORD_LIMIT {
            return Err(SkillError::Invalid);
        }
        let head = Head {
            format: 1,
            sequence,
            last_sha256: hash(&bytes),
        };
        self.publish(&format!("event-{sequence:020}.json"), &bytes)?;
        self.head(&head)?;
        next.head = head.clone();
        *state = next;
        Ok(DurableReceipt {
            sequence,
            audit_sha256: head.last_sha256,
        })
    }
    fn apply(&self, state: &mut State, event: &AuditEvent) -> Result<(), SkillError> {
        match &event.change {
            Change::Candidate { version, metadata } => {
                if version.individual_id.is_nil()
                    || version.skill_id.is_nil()
                    || version.version == 0
                    || !valid_hash(&version.source_sha256)
                {
                    return Err(SkillError::Corrupt);
                }
                let source = self.source(version)?;
                validate_draft(&CandidateDraft {
                    individual_id: version.individual_id,
                    name: metadata.name.clone(),
                    description: metadata.description.clone(),
                    source,
                    source_event_ids: metadata.source_event_ids.clone(),
                })?;
                let skill = state
                    .skills
                    .entry(version.skill_id)
                    .or_insert_with(|| SkillState {
                        individual: version.individual_id,
                        latest: 0,
                        versions: BTreeMap::new(),
                    });
                if skill.individual != version.individual_id
                    || skill.latest.checked_add(1) != Some(version.version)
                {
                    return Err(SkillError::Corrupt);
                }
                skill.latest = version.version;
                skill.versions.insert(
                    version.version,
                    VersionState {
                        version: version.clone(),
                        metadata: metadata.clone(),
                        phase: SkillPhase::Candidate,
                        epoch: event.sequence,
                        verified: None,
                        grant: None,
                    },
                );
            }
            Change::ValidationStarted {
                operation_id,
                version,
                cases,
            } => {
                check_cases(cases)?;
                state.current(version)?;
                if operation_id.is_nil() || !state.operations.insert(*operation_id) {
                    return Err(SkillError::Corrupt);
                }
                let v = state
                    .skills
                    .get_mut(&version.skill_id)
                    .and_then(|s| s.versions.get_mut(&version.version))
                    .ok_or(SkillError::Corrupt)?;
                v.phase = SkillPhase::Candidate;
                v.verified = None;
                v.grant = None;
                v.epoch = event.sequence;
                state.pending.insert(
                    *operation_id,
                    Pending::Validation {
                        version: version.clone(),
                        cases: cases.clone(),
                        epoch: event.sequence,
                    },
                );
            }
            Change::ValidationFinished { report } => {
                let Some(Pending::Validation {
                    version,
                    cases,
                    epoch,
                }) = state.pending.get(&report.operation_id).cloned()
                else {
                    return Err(SkillError::Corrupt);
                };
                if version != report.version || cases != report.cases {
                    return Err(SkillError::Corrupt);
                }
                self.check_report(report)?;
                for outcome in &report.executions {
                    if !state.executions.insert(outcome.execution_id.clone()) {
                        return Err(SkillError::Corrupt);
                    }
                }
                let current = state.current(&version).ok();
                let eligible = current.is_some_and(|v| v.epoch == epoch);
                if report.completion == Completion::Complete && !eligible {
                    return Err(SkillError::Corrupt);
                }
                if eligible {
                    let v = state
                        .skills
                        .get_mut(&version.skill_id)
                        .and_then(|s| s.versions.get_mut(&version.version))
                        .ok_or(SkillError::Corrupt)?;
                    v.epoch = event.sequence;
                    if report.completion == Completion::Complete {
                        v.phase = SkillPhase::Validated;
                        v.verified = Some(ValidatedSkill {
                            version: version.clone(),
                            report_sha256: hash(&encoded(report)?),
                            case_set_sha256: report.case_set_sha256.clone(),
                        });
                    }
                }
                state.reports.insert(report.operation_id, report.clone());
                state.pending.remove(&report.operation_id);
            }
            Change::Registered { grant } => {
                let v = state.current(&grant.version)?;
                let verified = v.verified.as_ref().ok_or(SkillError::NotValidated)?;
                if grant.id.is_nil()
                    || grant.report_sha256 != verified.report_sha256
                    || grant.case_set_sha256 != verified.case_set_sha256
                    || grant.image_digest != IMAGE
                    || grant.wrapper_version != WRAPPER_VERSION
                    || grant.scope != SkillScope::SandboxPython
                    || grant.expires_ms <= event.at_ms
                    || state.grants.contains(&grant.id)
                {
                    return Err(SkillError::Corrupt);
                }
                state.grants.insert(grant.id);
                let v = state
                    .skills
                    .get_mut(&grant.version.skill_id)
                    .and_then(|s| s.versions.get_mut(&grant.version.version))
                    .ok_or(SkillError::Corrupt)?;
                v.grant = Some(grant.clone());
                v.phase = SkillPhase::Registered;
                v.epoch = event.sequence;
            }
            Change::Revoked { version, reason } => {
                state.current(version)?;
                if reason.trim().is_empty()
                    || reason.chars().count() > 2000
                    || reason.contains('\0')
                {
                    return Err(SkillError::Corrupt);
                }
                let v = state
                    .skills
                    .get_mut(&version.skill_id)
                    .and_then(|s| s.versions.get_mut(&version.version))
                    .ok_or(SkillError::Corrupt)?;
                v.phase = SkillPhase::Revoked;
                v.grant = None;
                v.epoch = event.sequence;
            }
            Change::InvocationStarted {
                operation_id,
                version,
                grant_id,
                input_json,
                timeout_ms,
            } => {
                if strict(input_json).is_err()
                    || !(100..=10000).contains(timeout_ms)
                    || operation_id.is_nil()
                    || state.operations.contains(operation_id)
                {
                    return Err(SkillError::Corrupt);
                }
                let v = state.invocable(version, event.at_ms)?;
                let grant = v.grant.as_ref().ok_or(SkillError::Corrupt)?;
                if grant.id != *grant_id {
                    return Err(SkillError::Corrupt);
                }
                let epoch = v.epoch;
                state.operations.insert(*operation_id);
                state.pending.insert(
                    *operation_id,
                    Pending::Invocation {
                        version: version.clone(),
                        grant_id: *grant_id,
                        input_json: input_json.clone(),
                        timeout_ms: *timeout_ms,
                        epoch,
                    },
                );
            }
            Change::InvocationFinished {
                operation_id,
                execution,
                wrapper_sha256,
                completion,
                elapsed_ms,
            } => {
                let Some(Pending::Invocation {
                    version,
                    grant_id,
                    input_json,
                    timeout_ms,
                    epoch,
                }) = state.pending.get(operation_id).cloned()
                else {
                    return Err(SkillError::Corrupt);
                };
                if !evidence_shape(execution)
                    || *wrapper_sha256 != hash(SKILL_WRAPPER.as_bytes())
                    || execution.source_sha256 != version.source_sha256
                    || execution.input_sha256 != hash(input_json.as_bytes())
                    || !state.executions.insert(execution.execution_id.clone())
                {
                    return Err(SkillError::Corrupt);
                }
                if *completion == Completion::Complete {
                    let v = state.invocable(&version, event.at_ms)?;
                    if v.epoch != epoch
                        || v.grant.as_ref().is_none_or(|g| g.id != grant_id)
                        || *elapsed_ms > timeout_ms
                    {
                        return Err(SkillError::Corrupt);
                    }
                    result_json(execution)?;
                }
                state.pending.remove(operation_id);
            }
        }
        Ok(())
    }
    fn check_report(&self, report: &ValidationReport) -> Result<(), SkillError> {
        check_cases(&report.cases)?;
        if report.operation_id.is_nil()
            || report.image_digest != IMAGE
            || report.protocol != SKILL_PROTOCOL
            || report.wrapper_version != WRAPPER_VERSION
            || report.wrapper_sha256 != hash(SKILL_WRAPPER.as_bytes())
            || report.case_set_sha256 != hash(&encoded(&report.cases)?)
            || report.executions.len() > report.cases.len()
        {
            return Err(SkillError::Corrupt);
        }
        self.source(&report.version)?;
        for (i, outcome) in report.executions.iter().enumerate() {
            if !evidence_shape(outcome)
                || outcome.source_sha256 != report.version.source_sha256
                || outcome.input_sha256 != hash(report.cases[i].input_json.as_bytes())
            {
                return Err(SkillError::Corrupt);
            }
        }
        if report.completion == Completion::Complete
            && (report.elapsed_ms > 30000
                || report.cases.len() != report.executions.len()
                || report
                    .executions
                    .iter()
                    .zip(&report.cases)
                    .any(|(o, c)| result_json(o).ok() != strict(&c.expected_json).ok()))
        {
            return Err(SkillError::Corrupt);
        }
        Ok(())
    }
    pub fn create_candidate(&self, draft: CandidateDraft) -> Result<SkillVersion, SkillError> {
        validate_draft(&draft)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let _lock = self.lock(deadline)?;
        let mut state = self.load(deadline)?;
        let version = SkillVersion {
            individual_id: draft.individual_id,
            skill_id: Uuid::new_v4(),
            version: 1,
            source_sha256: hash(draft.source.as_bytes()),
        };
        self.publish(&Self::source_name(&version), draft.source.as_bytes())?;
        self.append(
            &mut state,
            Change::Candidate {
                version: version.clone(),
                metadata: Metadata {
                    name: draft.name,
                    description: draft.description,
                    source_event_ids: draft.source_event_ids,
                },
            },
        )?;
        Ok(version)
    }
    pub fn revise(
        &self,
        current: &SkillVersion,
        draft: CandidateDraft,
    ) -> Result<SkillVersion, SkillError> {
        validate_draft(&draft)?;
        if current.individual_id != draft.individual_id {
            return Err(SkillError::Stale);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let _lock = self.lock(deadline)?;
        let mut state = self.load(deadline)?;
        let skill = state
            .skills
            .get(&current.skill_id)
            .ok_or(SkillError::NotFound)?;
        if skill.individual != current.individual_id
            || skill.latest != current.version
            || skill
                .versions
                .get(&current.version)
                .is_none_or(|v| v.version != *current)
        {
            return Err(SkillError::Stale);
        }
        let version = SkillVersion {
            individual_id: current.individual_id,
            skill_id: current.skill_id,
            version: current.version.checked_add(1).ok_or(SkillError::Invalid)?,
            source_sha256: hash(draft.source.as_bytes()),
        };
        self.publish(&Self::source_name(&version), draft.source.as_bytes())?;
        self.append(
            &mut state,
            Change::Candidate {
                version: version.clone(),
                metadata: Metadata {
                    name: draft.name,
                    description: draft.description,
                    source_event_ids: draft.source_event_ids,
                },
            },
        )?;
        Ok(version)
    }
    pub fn view(&self, individual: Uuid, skill: Uuid) -> Result<SkillView, SkillError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let _lock = self.lock(deadline)?;
        let state = self.load(deadline)?;
        let s = state.skills.get(&skill).ok_or(SkillError::NotFound)?;
        if s.individual != individual {
            return Err(SkillError::NotFound);
        }
        let v = s.versions.get(&s.latest).ok_or(SkillError::Corrupt)?;
        let pending = state
            .pending
            .iter()
            .filter_map(|(id, p)| {
                let version = match p {
                    Pending::Validation { version, .. } | Pending::Invocation { version, .. } => {
                        version
                    }
                };
                (version.skill_id == skill).then_some(*id)
            })
            .collect();
        Ok(SkillView {
            version: v.version.clone(),
            phase: v.phase,
            name: v.metadata.name.clone(),
            description: v.metadata.description.clone(),
            source_event_ids: v.metadata.source_event_ids.clone(),
            verified: v.verified.clone(),
            registration_expires_at: v
                .grant
                .as_ref()
                .and_then(|g| DateTime::from_timestamp_millis(g.expires_ms)),
            pending_operations: pending,
            pending_sandbox_calls_upper_bound: state
                .pending
                .values()
                .map(|p| match p {
                    Pending::Validation { version, cases, .. } if version.skill_id == skill => {
                        cases.len()
                    }
                    Pending::Invocation { version, .. } if version.skill_id == skill => 1,
                    _ => 0,
                })
                .sum(),
        })
    }
    pub fn validation_report(&self, operation: Uuid) -> Result<ValidationReport, SkillError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let _lock = self.lock(deadline)?;
        let state = self.load(deadline)?;
        state
            .reports
            .get(&operation)
            .cloned()
            .ok_or(SkillError::NotFound)
    }
    pub fn register(&self, grant: RegistrationGrant) -> Result<RegistrationReceipt, SkillError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let _lock = self.lock(deadline)?;
        let mut state = self.load(deadline)?;
        let g = grant.0;
        if g.expires_ms <= Utc::now().timestamp_millis() {
            return Err(SkillError::Expired);
        }
        let v = state.current(&g.version)?;
        let verified = v.verified.as_ref().ok_or(SkillError::NotValidated)?;
        if verified.report_sha256 != g.report_sha256
            || verified.case_set_sha256 != g.case_set_sha256
        {
            return Err(SkillError::Stale);
        }
        if g.scope != SkillScope::SandboxPython {
            return Err(SkillError::Scope);
        }
        let version = g.version.clone();
        let grant_id = g.id;
        let durable = self.append(&mut state, Change::Registered { grant: g })?;
        Ok(RegistrationReceipt {
            version,
            grant_id,
            durable,
        })
    }
    pub fn revoke(
        &self,
        version: &SkillVersion,
        reason: &str,
    ) -> Result<RevocationReceipt, SkillError> {
        if reason.trim().is_empty() || reason.chars().count() > 2000 || reason.contains('\0') {
            return Err(SkillError::Invalid);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let _lock = self.lock(deadline)?;
        let mut state = self.load(deadline)?;
        state.current(version)?;
        let durable = self.append(
            &mut state,
            Change::Revoked {
                version: version.clone(),
                reason: reason.into(),
            },
        )?;
        Ok(RevocationReceipt {
            version: version.clone(),
            durable,
        })
    }
    fn guard(
        &self,
        version: &SkillVersion,
        epoch: u64,
        deadline: Instant,
    ) -> Result<(), SkillError> {
        let _lock = self.lock(deadline)?;
        let state = self.load(deadline)?;
        let v = state.current(version)?;
        if v.epoch != epoch {
            return Err(SkillError::Stale);
        }
        Ok(())
    }
    pub fn validate(&self, version: &SkillVersion, cases: &ValidationCases) -> ValidationAttempt {
        let started = Instant::now();
        let deadline = started + Duration::from_secs(30);
        let mut attempt = ValidationAttempt {
            operation_id: None,
            sandbox_calls: 0,
            attested_executions: 0,
            executions: Vec::new(),
            report_sha256: None,
            durable: false,
            result: Err(SkillError::Unknown),
        };
        let prepared = (|| {
            check_cases(&cases.0)?;
            let executor = SandboxExecutor::new(IMAGE, self.launcher)
                .map_err(|_| SkillError::VerificationFailed)?;
            let _lock = self.lock(deadline)?;
            let mut state = self.load(deadline)?;
            state.current(version)?;
            let source = self.source(version)?;
            // Structured protocol keeps both full limits independently of fixed argv.
            let wrappers = cases
                .0
                .iter()
                .map(|c| skill_request(&source, &c.input_json, 10000))
                .collect::<Result<Vec<_>, _>>()?;
            let operation_id = Uuid::new_v4();
            let receipt = self.append(
                &mut state,
                Change::ValidationStarted {
                    operation_id,
                    version: version.clone(),
                    cases: cases.0.clone(),
                },
            )?;
            Ok::<_, SkillError>((executor, wrappers, operation_id, receipt.sequence))
        })();
        let (executor, wrappers, operation_id, epoch) = match prepared {
            Ok(v) => v,
            Err(e) => {
                attempt.result = Err(e);
                return attempt;
            }
        };
        attempt.operation_id = Some(operation_id);
        let mut completion = Completion::Complete;
        let mut failure = SkillError::VerificationFailed;
        for (index, mut request) in wrappers.into_iter().enumerate() {
            if let Err(e) = self.guard(version, epoch, deadline) {
                completion = completion_for(e);
                failure = e;
                break;
            }
            let left = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .saturating_sub(250);
            if left < 1200 {
                completion = Completion::Deadline;
                failure = SkillError::Deadline;
                break;
            }
            request.timeout_ms = left.min(10000) as u64;
            attempt.sandbox_calls += 1;
            let evidence: ExecutionEvidence = executor.execute_skill(&request).into();
            if evidence.isolation_verified {
                attempt.attested_executions += 1;
            }
            let matches = result_json(&evidence).ok() == strict(&cases.0[index].expected_json).ok();
            attempt.executions.push(evidence);
            if !matches {
                completion = Completion::Failed;
                break;
            }
        }
        let finished = (|| {
            let _lock = self.lock(deadline)?;
            let mut state = self.load(deadline)?;
            match state.current(version) {
                Ok(v) if v.epoch == epoch => {}
                Ok(_) => {
                    completion = Completion::Stale;
                    failure = SkillError::Stale;
                }
                Err(e) => {
                    completion = completion_for(e);
                    failure = e;
                }
            }
            let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            if elapsed_ms >= 30000 {
                completion = Completion::Deadline;
                failure = SkillError::Deadline;
            }
            let report = ValidationReport {
                operation_id,
                version: version.clone(),
                image_digest: IMAGE.into(),
                wrapper_version: WRAPPER_VERSION,
                cases: cases.0.clone(),
                case_set_sha256: hash(&encoded(&cases.0)?),
                protocol: SKILL_PROTOCOL.into(),
                wrapper_sha256: hash(SKILL_WRAPPER.as_bytes()),
                executions: attempt.executions.clone(),
                completion,
                elapsed_ms,
            };
            let report_sha = hash(&encoded(&report)?);
            self.append(&mut state, Change::ValidationFinished { report })?;
            attempt.durable = true;
            attempt.report_sha256 = Some(report_sha);
            if completion == Completion::Complete {
                state
                    .current(version)?
                    .verified
                    .clone()
                    .ok_or(SkillError::Unknown)
            } else {
                Err(failure)
            }
        })();
        attempt.result = finished;
        attempt
    }
    pub fn invoke(
        &self,
        version: &SkillVersion,
        input_json: &str,
        timeout_ms: u64,
    ) -> InvocationAttempt {
        let started = Instant::now();
        let deadline = started + Duration::from_millis(timeout_ms.min(10000));
        let mut attempt = InvocationAttempt {
            operation_id: None,
            sandbox_calls: 0,
            attested_executions: 0,
            execution: None,
            durable: false,
            result: Err(SkillError::Unknown),
        };
        let prepared = (|| {
            if !(100..=10000).contains(&timeout_ms) {
                return Err(SkillError::Invalid);
            }
            strict(input_json)?;
            let executor = SandboxExecutor::new(IMAGE, self.launcher)
                .map_err(|_| SkillError::VerificationFailed)?;
            let _lock = self.lock(deadline)?;
            let mut state = self.load(deadline)?;
            let v = state.invocable(version, Utc::now().timestamp_millis())?;
            let grant_id = v.grant.as_ref().ok_or(SkillError::Corrupt)?.id;
            let epoch = v.epoch;
            let request = skill_request(&self.source(version)?, input_json, timeout_ms)?;
            let operation_id = Uuid::new_v4();
            self.append(
                &mut state,
                Change::InvocationStarted {
                    operation_id,
                    version: version.clone(),
                    grant_id,
                    input_json: input_json.into(),
                    timeout_ms,
                },
            )?;
            Ok::<_, SkillError>((executor, request, operation_id, grant_id, epoch))
        })();
        let (executor, mut request, operation_id, grant_id, epoch) = match prepared {
            Ok(v) => v,
            Err(e) => {
                attempt.result = Err(e);
                return attempt;
            }
        };
        attempt.operation_id = Some(operation_id);
        let wrapper_sha256 = hash(SKILL_WRAPPER.as_bytes());
        let left = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .saturating_sub(100)
            .min(10000) as u64;
        // The SDK itself declines launch if less than its 1s cleanup reserve remains.
        // This still counts an execute attempt and persists its real timed-out outcome.
        attempt.sandbox_calls = 1;
        request.timeout_ms = left.max(100);
        let evidence: ExecutionEvidence = executor.execute_skill(&request).into();
        if evidence.isolation_verified {
            attempt.attested_executions = 1;
        }
        let actual = result_json(&evidence);
        attempt.execution = Some(evidence.clone());
        let finished = (|| {
            // A short separate persistence reserve records rejected/expired outcomes;
            // it cannot upgrade an execution that exceeded its original deadline.
            let finish_deadline = deadline.max(Instant::now() + Duration::from_millis(250));
            let _lock = self.lock(finish_deadline)?;
            let mut state = self.load(finish_deadline)?;
            let mut failure = actual.as_ref().err().copied();
            match state.invocable(version, Utc::now().timestamp_millis()) {
                Ok(v) if v.epoch == epoch && v.grant.as_ref().is_some_and(|g| g.id == grant_id) => {
                }
                Ok(_) => failure = Some(SkillError::Stale),
                Err(e) => failure = Some(e),
            }
            let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            if elapsed_ms > timeout_ms {
                failure = Some(SkillError::Deadline);
            }
            let completion = failure.map_or(Completion::Complete, completion_for);
            self.append(
                &mut state,
                Change::InvocationFinished {
                    operation_id,
                    execution: evidence,
                    wrapper_sha256,
                    completion,
                    elapsed_ms,
                },
            )?;
            attempt.durable = true;
            match failure {
                Some(e) => Err(e),
                None => actual,
            }
        })();
        attempt.result = finished;
        attempt
    }
}
fn completion_for(error: SkillError) -> Completion {
    match error {
        SkillError::Revoked => Completion::Revoked,
        SkillError::Expired => Completion::Expired,
        SkillError::Stale => Completion::Stale,
        SkillError::Deadline | SkillError::Busy => Completion::Deadline,
        SkillError::TransportLimit => Completion::TransportLimit,
        _ => Completion::Failed,
    }
}
