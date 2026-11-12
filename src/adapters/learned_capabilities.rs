//! Owner/source/budget bridge around the existing SDK. No registry or policy enablement.
//! The protocol release is deliberately unavailable in production until independent review.
use super::{
    sandbox::DockerCommand,
    sandbox_skills::{
        CandidateDraft, RegistrationGrant, SkillError, SkillPhase, SkillScope, SkillVault,
        SkillVersion, ValidationCases,
    },
    scheduling::require_lease,
    store::{Store, StoreError, advance, digest, edge, insert_event, outbox, state},
};
use crate::{
    domain::{Lifecycle, history::ExpectedState},
    runtime::{capability::ActionReceipt, scheduling::RunLease},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        io::AsRawFd,
    },
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

pub const CAPABILITY_ID: &str = "sandbox.python";
pub const BRIDGE_VERSION: &str = "1";
type Tx<'a> = Transaction<'a, Postgres>;
type ActionRow = (
    String,
    String,
    Value,
    String,
    Option<Value>,
    Option<i64>,
    i32,
);
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BridgeError {
    #[error("invalid learned capability request")]
    Invalid,
    #[error("learned capability source or version changed")]
    Stale,
    #[error("learned capability permission denied")]
    Denied,
    #[error("learned capability protocol release unavailable")]
    Unreleased,
    #[error("learned capability tool allowance unavailable")]
    Budget,
    #[error("learned capability action is unknown; never redispatch")]
    Unknown,
    #[error("learned capability storage unavailable")]
    Storage,
    #[error("learned capability filesystem unsafe or unavailable")]
    UnsafeStorage,
    #[error("learned capability resource busy")]
    Busy,
}
impl From<StoreError> for BridgeError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::StaleState
            | StoreError::LeaseLost
            | StoreError::PrivacyUnavailable
            | StoreError::NotFound => Self::Stale,
            StoreError::CallLimit => Self::Budget,
            StoreError::Inactive | StoreError::Proposal(_) => Self::Denied,
            StoreError::Invalid(_) | StoreError::IdempotencyConflict => Self::Invalid,
            _ => Self::Storage,
        }
    }
}
fn db(_: sqlx::Error) -> BridgeError {
    BridgeError::Storage
}
fn io(_: std::io::Error) -> BridgeError {
    BridgeError::UnsafeStorage
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Deliberately private fields and no Deserialize or production enable setter.
#[derive(Clone, Default)]
pub struct TrustedSkillPolicy {
    execution_release: bool,
}
impl TrustedSkillPolicy {
    pub fn unavailable() -> Self {
        Self::default()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillRef {
    pub skill_id: Uuid,
    pub version: i32,
    pub source_sha256: String,
}
/// Created by the trusted controller, not by a model's cases/passed boolean.
#[derive(Clone)]
pub struct TrustedCases {
    sdk: ValidationCases,
    fingerprint: String,
}
impl TrustedCases {
    pub fn new(cases: Vec<(String, String, String)>) -> Result<Self, BridgeError> {
        let fingerprint = sha(&serde_json::to_vec(&cases).map_err(|_| BridgeError::Invalid)?);
        let sdk = ValidationCases::new(cases).map_err(|_| BridgeError::Invalid)?;
        Ok(Self { sdk, fingerprint })
    }
}
#[derive(Clone, Debug)]
pub struct TrustedSkillGrant {
    id: Uuid,
    individual_id: Uuid,
    reference: SkillRef,
    expires_at: DateTime<Utc>,
}
#[derive(Clone)]
pub enum SkillCommand {
    Candidate(CandidateDraft),
    Revise {
        reference: SkillRef,
        draft: CandidateDraft,
    },
    Validate {
        reference: SkillRef,
        cases: TrustedCases,
    },
    Register {
        reference: SkillRef,
        grant: TrustedSkillGrant,
    },
    Invoke {
        reference: SkillRef,
        input_json: String,
        timeout_ms: u64,
    },
    Revoke {
        reference: SkillRef,
        reason: String,
    },
}
impl SkillCommand {
    /// The ordinary action arguments must match this canonical closed request.
    pub fn arguments(&self) -> Result<Value, BridgeError> {
        Ok(match self {
            Self::Candidate(d) => json!({"op":"candidate","draft":d}),
            Self::Revise { reference, draft } => {
                json!({"op":"revise","reference":reference,"draft":draft})
            }
            Self::Validate { reference, cases } => {
                json!({"op":"validate","reference":reference,"cases_sha256":cases.fingerprint,"case_count":cases.sdk.sandbox_calls_upper_bound()})
            }
            Self::Register { reference, grant } => {
                json!({"op":"register","reference":reference,"owner_grant_id":grant.id})
            }
            Self::Invoke {
                reference,
                input_json,
                timeout_ms,
            } => {
                crate::domain::config::parse_strict_json(input_json.as_bytes(), 8192)
                    .map_err(|_| BridgeError::Invalid)?;
                if !(100..=10000).contains(timeout_ms) {
                    return Err(BridgeError::Invalid);
                }
                json!({"op":"invoke","reference":reference,"input_json":input_json,"timeout_ms":timeout_ms})
            }
            Self::Revoke { reference, reason } => {
                if reason.trim().is_empty()
                    || reason.contains('\0')
                    || reason.chars().count() > 2000
                {
                    return Err(BridgeError::Invalid);
                }
                json!({"op":"revoke","reference":reference,"reason":reason})
            }
        })
    }
    fn reference(&self) -> Option<&SkillRef> {
        match self {
            Self::Candidate(_) => None,
            Self::Revise { reference, .. }
            | Self::Validate { reference, .. }
            | Self::Register { reference, .. }
            | Self::Invoke { reference, .. }
            | Self::Revoke { reference, .. } => Some(reference),
        }
    }
    fn slots(&self) -> u32 {
        match self {
            Self::Validate { cases, .. } => cases.sdk.sandbox_calls_upper_bound(),
            _ => 1,
        }
    }
    fn executes(&self) -> bool {
        matches!(self, Self::Validate { .. } | Self::Invoke { .. })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeReceipt {
    pub action: ActionReceipt,
    pub reference: SkillRef,
    pub sdk_operation_id: Option<Uuid>,
    pub sandbox_calls: u32,
    pub attested_executions: u32,
    pub reserved_slots: u32,
    pub charged_slots: u32,
    pub sdk_durable: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Epoch {
    individual_id: Uuid,
    privacy_epoch: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Seal {
    individual_id: Uuid,
    skill_id: Uuid,
    capsule_id: Uuid,
    privacy_epoch: i64,
}
#[derive(Clone)]
pub struct LearnedCapabilities {
    root: PathBuf,
    directory: Arc<File>,
    launcher: DockerCommand,
    policy: TrustedSkillPolicy,
    slots: Arc<tokio::sync::Semaphore>,
}
fn fd(dir: &File, name: &str) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd())).join(name)
}
fn private(file: &File, dir: bool, uid: u32) -> Result<(), BridgeError> {
    let m = file.metadata().map_err(io)?;
    if m.uid() != uid
        || m.mode() & 0o7777 != if dir { 0o700 } else { 0o600 }
        || if dir {
            !m.is_dir()
        } else {
            !m.is_file() || m.nlink() != 1
        }
    {
        return Err(BridgeError::UnsafeStorage);
    }
    Ok(())
}
fn file(dir: &File, name: &str, create: bool) -> Result<File, BridgeError> {
    let mut o = OpenOptions::new();
    o.read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    if create {
        o.create_new(true);
    }
    let f = o.open(fd(dir, name)).map_err(io)?;
    private(&f, false, dir.metadata().map_err(io)?.uid())?;
    Ok(f)
}
fn lock(dir: &File, name: &str) -> Result<File, BridgeError> {
    let f = file(dir, name, false)?;
    f.try_lock().map_err(|_| BridgeError::Busy)?;
    Ok(f)
}
fn read<T: serde::de::DeserializeOwned>(dir: &File, name: &str) -> Result<T, BridgeError> {
    let mut f = file(dir, name, false)?;
    if f.metadata().map_err(io)?.len() > 65536 {
        return Err(BridgeError::UnsafeStorage);
    }
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).map_err(io)?;
    let v = crate::domain::config::parse_strict_json(&bytes, 65536)
        .map_err(|_| BridgeError::UnsafeStorage)?;
    serde_json::from_value(v).map_err(|_| BridgeError::UnsafeStorage)
}
fn publish<T: Serialize>(dir: &File, name: &str, value: &T) -> Result<(), BridgeError> {
    let mut f = file(dir, name, true)?;
    f.write_all(&serde_json::to_vec(value).map_err(|_| BridgeError::Invalid)?)
        .map_err(io)?;
    f.sync_all().map_err(io)?;
    dir.sync_all().map_err(io)
}
fn replace<T: Serialize>(dir: &File, name: &str, value: &T) -> Result<(), BridgeError> {
    let tmp = format!("pending-{}", Uuid::new_v4());
    publish(dir, &tmp, value)?;
    fs::rename(fd(dir, &tmp), fd(dir, name)).map_err(io)?;
    dir.sync_all().map_err(io)
}
impl LearnedCapabilities {
    pub fn open(
        root: &Path,
        launcher: DockerCommand,
        policy: TrustedSkillPolicy,
    ) -> Result<Self, BridgeError> {
        if !root.is_absolute() {
            return Err(BridgeError::UnsafeStorage);
        }
        for ancestor in root.ancestors() {
            let m = fs::symlink_metadata(ancestor).map_err(io)?;
            if m.file_type().is_symlink()
                || !m.is_dir()
                || (ancestor != root && m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0)
            {
                return Err(BridgeError::UnsafeStorage);
            }
        }
        if root
            .components()
            .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
        {
            return Err(BridgeError::UnsafeStorage);
        }
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root)
            .map_err(io)?;
        let uid = fs::metadata("/proc/self").map_err(io)?.uid();
        private(&directory, true, uid)?;
        if !fd(&directory, "bridge.lock").exists() {
            match file(&directory, "bridge.lock", true) {
                Ok(f) => {
                    f.sync_all().map_err(io)?;
                    directory.sync_all().map_err(io)?
                }
                Err(_) if fd(&directory, "bridge.lock").exists() => {}
                Err(e) => return Err(e),
            }
        }
        Ok(Self {
            root: root.to_owned(),
            directory: Arc::new(directory),
            launcher,
            policy,
            slots: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }
    fn identity(&self) -> Result<(), BridgeError> {
        let a = self.directory.metadata().map_err(io)?;
        let b = fs::symlink_metadata(&self.root).map_err(io)?;
        if b.file_type().is_symlink() || (a.dev(), a.ino()) != (b.dev(), b.ino()) {
            return Err(BridgeError::UnsafeStorage);
        }
        Ok(())
    }
    fn epoch(&self) -> Result<Epoch, BridgeError> {
        self.identity()?;
        read(&self.directory, "authority.json")
    }
    fn current_epoch(&self, owner: Uuid, epoch: i64) -> Result<(), BridgeError> {
        let a = self.epoch()?;
        if a.individual_id != owner || a.privacy_epoch != epoch {
            return Err(BridgeError::Stale);
        }
        Ok(())
    }
    /// Explicit trusted first enable, never from a restored artifact. Existing
    /// monotonic metadata cannot be lowered by an old database snapshot.
    pub async fn initialize_authority(&self, store: &Store) -> Result<(), BridgeError> {
        let generation = store.privacy_generation();
        store.require_privacy_ready()?;
        let mut tx = store.pool.begin().await.map_err(db)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let s = super::store::read_state(&mut tx)
            .await?
            .ok_or(BridgeError::Stale)?;
        let _lock = lock(&self.directory, "bridge.lock")?;
        if fd(&self.directory, "authority.json").exists() {
            self.current_epoch(s.id, s.privacy_epoch)?;
        } else {
            // First enable after prior deletion requires Root's original-journal
            // recovery/provisioning integration; never guess a new anchor here.
            if s.privacy_epoch != 0 {
                return Err(BridgeError::Stale);
            }
            let jobs: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM privacy_operations)")
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
            if jobs {
                return Err(BridgeError::Stale);
            }
            publish(
                &self.directory,
                "authority.json",
                &Epoch {
                    individual_id: s.id,
                    privacy_epoch: s.privacy_epoch,
                },
            )?;
        }
        store.require_privacy_snapshot(generation)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }
}
#[derive(sqlx::FromRow)]
struct Binding {
    id: Uuid,
    capsule_id: Uuid,
    privacy_epoch: i64,
    current_version: i32,
    sdk_skill_id: Option<Uuid>,
    vault_name: Option<String>,
    invalidated: bool,
    source_sha256: String,
    phase: String,
}
struct Dispatch {
    lease: RunLease,
    action: Uuid,
    reference: SkillRef,
    capsule_id: Uuid,
    sdk: Option<SkillVersion>,
    vault_name: Option<String>,
    generation: u64,
    slots: u32,
    request_digest: String,
    source_ids: Vec<Uuid>,
    command: SkillCommand,
}
enum Begin {
    Receipt(BridgeReceipt),
    Dispatch(Box<Dispatch>),
}
struct SdkOutcome {
    result: Result<Value, SkillError>,
    sdk: Option<SkillVersion>,
    vault_name: Option<String>,
    phase: Option<&'static str>,
    operation: Option<Uuid>,
    calls: u32,
    attested: u32,
    durable: bool,
    report: Option<String>,
    cases: Option<String>,
}
async fn binding(tx: &mut Tx<'_>, owner: Uuid, r: &SkillRef) -> Result<Binding, BridgeError> {
    let b:Binding=sqlx::query_as("SELECT s.id,s.capsule_id,s.privacy_epoch,s.current_version,s.sdk_skill_id,s.vault_name,s.invalidated,v.source_sha256,v.phase FROM learned_skills s JOIN learned_skill_versions v ON (v.individual_id,v.skill_id,v.version)=(s.individual_id,s.id,s.current_version) WHERE s.individual_id=$1 AND s.id=$2").bind(owner).bind(r.skill_id).fetch_optional(&mut **tx).await.map_err(db)?.ok_or(BridgeError::Stale)?;
    if b.invalidated
        || b.current_version != r.version
        || b.source_sha256 != r.source_sha256
        || matches!(b.phase.as_str(), "unknown" | "invalidated" | "pending")
    {
        return Err(BridgeError::Stale);
    }
    Ok(b)
}
async fn sources(tx: &mut Tx<'_>, owner: Uuid, roots: &[Uuid]) -> Result<(), BridgeError> {
    super::development_context::require_sources(tx, owner, roots).await?;
    let graph = super::deletion::reference_graph_sql(tx).await?;
    let denied:bool=sqlx::query_scalar(&format!("WITH RECURSIVE refs(source,target) AS ({graph} UNION SELECT event_id,skill_id FROM learned_skill_sources WHERE individual_id=$1 UNION SELECT created_event_id,skill_id FROM learned_skill_versions WHERE individual_id=$1 UNION SELECT skill_id,id FROM learned_skill_grants WHERE individual_id=$1 UNION SELECT skill_id,action_id FROM learned_skill_actions WHERE individual_id=$1),closure(id) AS (SELECT unnest($2::uuid[]) UNION SELECT r.source FROM refs r JOIN closure c ON r.target=c.id) SELECT EXISTS(SELECT 1 FROM closure c LEFT JOIN objects o ON o.individual_id=$1 AND o.id=c.id WHERE o.id IS NULL OR o.blocked OR EXISTS(SELECT 1 FROM learned_skills s WHERE s.individual_id=$1 AND s.id=c.id AND s.invalidated))")) .bind(owner).bind(roots).fetch_one(&mut **tx).await.map_err(db)?;
    if denied {
        return Err(BridgeError::Stale);
    }
    Ok(())
}
fn enabled(config: &Value) -> bool {
    config["capabilities"]["enabled"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v == CAPABILITY_ID))
}
fn checked_draft(d: &CandidateDraft, owner: Uuid) -> Result<(), BridgeError> {
    if d.individual_id != owner
        || d.name.trim().is_empty()
        || d.name.chars().count() > 80
        || d.description.chars().count() > 2000
        || d.source.is_empty()
        || d.source.len() > 24576
        || [&d.name, &d.description, &d.source]
            .iter()
            .any(|s| s.contains('\0'))
        || d.source_event_ids.is_empty()
        || d.source_event_ids.len() > 32
        || d.source_event_ids.iter().any(Uuid::is_nil)
        || d.source_event_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != d.source_event_ids.len()
    {
        return Err(BridgeError::Invalid);
    }
    Ok(())
}
impl LearnedCapabilities {
    async fn begin(
        &self,
        store: &Store,
        lease: &RunLease,
        action: Uuid,
        command: SkillCommand,
    ) -> Result<Begin, BridgeError> {
        let arguments = command.arguments()?;
        let request_digest = digest(&arguments)?;
        let generation = store.privacy_generation();
        let mut tx = store.write().await?;
        let current = state(&mut tx).await?.ok_or(BridgeError::Stale)?;
        self.current_epoch(current.id, current.privacy_epoch)?;
        let row:Option<ActionRow>=sqlx::query_as("SELECT capability_id,capability_version,arguments,status,receipt,lease_generation,sequence FROM actions WHERE individual_id=$1 AND id=$2 AND run_id=$3").bind(lease.individual_id).bind(action).bind(lease.run_id).fetch_optional(&mut *tx).await.map_err(db)?;
        let (cap, version, args, status, old, old_generation, sequence) =
            row.ok_or(BridgeError::Invalid)?;
        if cap != CAPABILITY_ID
            || version != BRIDGE_VERSION
            || args != arguments
            || current.id != lease.individual_id
            || current.privacy_epoch != lease.privacy_epoch
        {
            return Err(BridgeError::Stale);
        }
        if let Some(old) = old {
            let old_digest:Option<String>=sqlx::query_scalar("SELECT request_digest FROM learned_skill_actions WHERE individual_id=$1 AND action_id=$2 AND status='settled'").bind(current.id).bind(action).fetch_optional(&mut *tx).await.map_err(db)?;
            let receipt: BridgeReceipt =
                serde_json::from_value(old).map_err(|_| BridgeError::Storage)?;
            if old_generation != Some(lease.generation)
                || old_digest.as_deref() != Some(&request_digest)
                || receipt.action.action_id != action
            {
                return Err(BridgeError::Stale);
            }
            sources(
                &mut tx,
                current.id,
                &[
                    current.id,
                    lease.run_id,
                    action,
                    receipt.reference.skill_id,
                    receipt.action.result_event_id,
                ],
            )
            .await?;
            store.require_privacy_snapshot(generation)?;
            tx.commit().await.map_err(db)?;
            return Ok(Begin::Receipt(receipt));
        }
        require_lease(&mut tx, lease).await?;
        if status != "planned" {
            return Err(BridgeError::Unknown);
        }
        if !enabled(&current.runtime_config) {
            return Err(BridgeError::Denied);
        }
        if command.executes() && !self.policy.execution_release {
            return Err(BridgeError::Unreleased);
        }
        let prior:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learned_skill_actions WHERE individual_id=$1 AND action_id=$2) OR EXISTS(SELECT 1 FROM actions WHERE run_id=$3 AND sequence<$4 AND status IN ('planned','executing','unknown'))").bind(current.id).bind(action).bind(lease.run_id).bind(sequence).fetch_one(&mut *tx).await.map_err(db)?;
        if prior {
            return Err(BridgeError::Unknown);
        }
        let (mut inputs,left):(Vec<Uuid>,f64)=sqlx::query_as("SELECT input_manifest,extract(epoch FROM deadline-clock_timestamp())::double precision FROM runs WHERE id=$1").bind(lease.run_id).fetch_one(&mut *tx).await.map_err(db)?;
        let needed = match &command {
            SkillCommand::Validate { .. } => 31.0,
            SkillCommand::Invoke { timeout_ms, .. } => *timeout_ms as f64 / 1000.0 + 1.0,
            _ => 6.0,
        };
        if left < needed {
            return Err(BridgeError::Stale);
        }
        inputs.extend([current.id, lease.run_id, action]);
        let (reference, capsule_id, sdk, vault_name) = if let SkillCommand::Candidate(draft) =
            &command
        {
            checked_draft(draft, current.id)?;
            if draft.source_event_ids.iter().any(|id| !inputs.contains(id)) {
                return Err(BridgeError::Invalid);
            }
            sources(&mut tx, current.id, &draft.source_event_ids).await?;
            let actual:i64=sqlx::query_scalar("SELECT count(*) FROM events WHERE individual_id=$1 AND id=ANY($2) AND source_kind<>'fictional_background' AND kind<>'decision.reviewed'").bind(current.id).bind(&draft.source_event_ids).fetch_one(&mut *tx).await.map_err(db)?;
            if actual as usize != draft.source_event_ids.len() {
                return Err(BridgeError::Invalid);
            }
            let id = Uuid::new_v4();
            let capsule = Uuid::new_v4();
            insert_event(
                &mut tx,
                current.id,
                id,
                "skill.candidate_requested",
                "inference",
                "alia",
                json!({"action_id":action,"source_event_ids":draft.source_event_ids}),
                lease.run_id,
                None,
            )
            .await?;
            sqlx::query("INSERT INTO learned_skills(individual_id,id,capsule_id,privacy_epoch,current_version) VALUES($1,$2,$3,$4,1)").bind(current.id).bind(id).bind(capsule).bind(current.privacy_epoch).execute(&mut *tx).await.map_err(db)?;
            let reference = SkillRef {
                skill_id: id,
                version: 1,
                source_sha256: sha(draft.source.as_bytes()),
            };
            sqlx::query("INSERT INTO learned_skill_versions(individual_id,skill_id,version,source_sha256,created_event_id,phase) VALUES($1,$2,1,$3,$2,'pending')").bind(current.id).bind(id).bind(&reference.source_sha256).execute(&mut *tx).await.map_err(db)?;
            for event in &draft.source_event_ids {
                sqlx::query("INSERT INTO learned_skill_sources(individual_id,skill_id,version,event_id) VALUES($1,$2,1,$3)").bind(current.id).bind(id).bind(event).execute(&mut *tx).await.map_err(db)?;
                edge(&mut tx, current.id, *event, id).await?;
            }
            for input in &inputs {
                edge(&mut tx, current.id, *input, id).await?;
            }
            (reference, capsule, None, None)
        } else {
            let reference = command.reference().ok_or(BridgeError::Invalid)?;
            if !inputs.contains(&reference.skill_id) {
                return Err(BridgeError::Invalid);
            }
            let b = binding(&mut tx, current.id, reference).await?;
            if b.privacy_epoch != current.privacy_epoch {
                return Err(BridgeError::Stale);
            }
            let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learned_skill_actions WHERE individual_id=$1 AND skill_id=$2 AND status IN ('running','unknown'))").bind(current.id).bind(b.id).fetch_one(&mut *tx).await.map_err(db)?;
            if pending {
                return Err(BridgeError::Unknown);
            }
            sources(&mut tx, current.id, &[b.id]).await?;
            let sdk = SkillVersion {
                individual_id: current.id,
                skill_id: b.sdk_skill_id.ok_or(BridgeError::Stale)?,
                version: u32::try_from(reference.version).map_err(|_| BridgeError::Invalid)?,
                source_sha256: reference.source_sha256.clone(),
            };
            let mut next = reference.clone();
            if let SkillCommand::Revise { draft, .. } = &command {
                checked_draft(draft, current.id)?;
                if draft.source_event_ids.iter().any(|id| !inputs.contains(id)) {
                    return Err(BridgeError::Invalid);
                }
                sources(&mut tx, current.id, &draft.source_event_ids).await?;
                let actual:i64=sqlx::query_scalar("SELECT count(*) FROM events WHERE individual_id=$1 AND id=ANY($2) AND source_kind<>'fictional_background' AND kind<>'decision.reviewed'").bind(current.id).bind(&draft.source_event_ids).fetch_one(&mut *tx).await.map_err(db)?;
                if actual as usize != draft.source_event_ids.len() {
                    return Err(BridgeError::Invalid);
                }
                next.version = reference
                    .version
                    .checked_add(1)
                    .ok_or(BridgeError::Invalid)?;
                next.source_sha256 = sha(draft.source.as_bytes());
                let e = Uuid::new_v4();
                insert_event(&mut tx,current.id,e,"skill.revision_requested","inference","alia",json!({"action_id":action,"skill_id":b.id,"source_event_ids":draft.source_event_ids}),lease.run_id,None).await?;
                sqlx::query("INSERT INTO learned_skill_versions(individual_id,skill_id,version,source_sha256,created_event_id,phase) VALUES($1,$2,$3,$4,$5,'pending')").bind(current.id).bind(b.id).bind(next.version).bind(&next.source_sha256).bind(e).execute(&mut *tx).await.map_err(db)?;
                for event in &draft.source_event_ids {
                    sqlx::query("INSERT INTO learned_skill_sources(individual_id,skill_id,version,event_id) VALUES($1,$2,$3,$4)").bind(current.id).bind(b.id).bind(next.version).bind(event).execute(&mut *tx).await.map_err(db)?;
                    edge(&mut tx, current.id, *event, e).await?;
                    edge(&mut tx, current.id, *event, b.id).await?;
                }
                edge(&mut tx, current.id, e, b.id).await?;
            }
            if matches!(&command, SkillCommand::Invoke { .. }) && b.phase != "registered" {
                return Err(BridgeError::Denied);
            }
            check_grant(
                &mut tx,
                current.id,
                &command,
                current.config_version,
                current.privacy_epoch,
            )
            .await?;
            inputs.push(b.id);
            (next, b.capsule_id, Some(sdk), b.vault_name)
        };
        inputs.sort();
        inputs.dedup();
        sources(&mut tx, current.id, &inputs).await?;
        let slots = command.slots();
        let limit = current.runtime_config["autonomy"]["max_tool_calls_per_cycle"]
            .as_i64()
            .ok_or(BridgeError::Invalid)?
            .min(3);
        let rows=sqlx::query("UPDATE runs SET tool_calls=tool_calls+$2 WHERE individual_id=$1 AND id=$3 AND tool_calls+$2<=$4").bind(current.id).bind(i64::from(slots)).bind(lease.run_id).bind(limit).execute(&mut *tx).await.map_err(db)?;
        if rows.rows_affected() != 1 {
            return Err(BridgeError::Budget);
        }
        sqlx::query("INSERT INTO learned_skill_actions(individual_id,action_id,skill_id,version,request_digest,lease_generation,config_version,privacy_epoch,reserved_slots,charged_slots,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,'running')").bind(current.id).bind(action).bind(reference.skill_id).bind(reference.version).bind(&request_digest).bind(lease.generation).bind(lease.config_version).bind(lease.privacy_epoch).bind(slots as i32).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE actions SET status='executing',lease_generation=$3,execution_mode='live',started_at=clock_timestamp() WHERE individual_id=$1 AND id=$2").bind(current.id).bind(action).bind(lease.generation).execute(&mut *tx).await.map_err(db)?;
        for input in &inputs {
            if *input != action {
                edge(&mut tx, current.id, *input, action).await?;
            }
        }
        store.require_privacy_snapshot(generation)?;
        self.current_epoch(current.id, current.privacy_epoch)?;
        tx.commit().await.map_err(db)?;
        Ok(Begin::Dispatch(Box::new(Dispatch {
            lease: lease.clone(),
            action,
            reference,
            capsule_id,
            sdk,
            vault_name,
            generation,
            slots,
            request_digest,
            source_ids: inputs,
            command,
        })))
    }
}
async fn check_grant(
    tx: &mut Tx<'_>,
    owner: Uuid,
    command: &SkillCommand,
    config: i64,
    epoch: i64,
) -> Result<(), BridgeError> {
    let grant=match command {SkillCommand::Register{grant,..}=>Some(grant.id),SkillCommand::Invoke{reference,..}=>sqlx::query_scalar("SELECT registration_grant_id FROM learned_skill_versions WHERE individual_id=$1 AND skill_id=$2 AND version=$3").bind(owner).bind(reference.skill_id).bind(reference.version).fetch_one(&mut **tx).await.map_err(db)?,_=>return Ok(())};
    let id = grant.ok_or(BridgeError::Denied)?;
    let r = command.reference().ok_or(BridgeError::Invalid)?;
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learned_skill_grants g JOIN events e ON (e.individual_id,e.id)=(g.individual_id,g.id) WHERE g.individual_id=$1 AND g.id=$2 AND g.skill_id=$3 AND g.version=$4 AND g.source_sha256=$5 AND g.scope='sandbox.python' AND g.expires_at>clock_timestamp() AND g.config_version=$6 AND g.privacy_epoch=$7 AND NOT g.revoked AND e.kind='skill.owner_grant' AND e.actor_id='owner' AND e.source_kind='direct_observation' AND e.payload->>'skill_id'=g.skill_id::text AND e.payload->>'version'=g.version::text AND e.payload->>'source_sha256'=g.source_sha256 AND (e.payload->>'expires_at')::timestamptz=g.expires_at AND e.payload->>'scope'=g.scope)").bind(owner).bind(id).bind(r.skill_id).bind(r.version).bind(&r.source_sha256).bind(config).bind(epoch).fetch_one(&mut **tx).await.map_err(db)?;
    if !valid {
        return Err(BridgeError::Denied);
    }
    if let SkillCommand::Register { grant, .. } = command
        && (grant.individual_id != owner || grant.reference != *r || grant.expires_at <= Utc::now())
    {
        return Err(BridgeError::Denied);
    }
    sources(tx, owner, &[id, r.skill_id]).await
}
impl LearnedCapabilities {
    fn capsule(&self, d: &Dispatch) -> Result<(PathBuf, File, File), BridgeError> {
        self.current_epoch(d.lease.individual_id, d.lease.privacy_epoch)?;
        let name = d.capsule_id.to_string();
        let path = self.root.join(&name);
        if matches!(d.command, SkillCommand::Candidate(_)) {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(fd(&self.directory, &name))
                .map_err(io)?;
            self.directory.sync_all().map_err(io)?;
            let dir = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&path)
                .map_err(io)?;
            private(&dir, true, self.directory.metadata().map_err(io)?.uid())?;
            let lease = file(&dir, "active.lock", true)?;
            lease.try_lock().map_err(|_| BridgeError::Busy)?;
            publish(
                &dir,
                "seal.json",
                &Seal {
                    individual_id: d.lease.individual_id,
                    skill_id: d.reference.skill_id,
                    capsule_id: d.capsule_id,
                    privacy_epoch: d.lease.privacy_epoch,
                },
            )?;
            Ok((path, dir, lease))
        } else {
            let dir = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&path)
                .map_err(io)?;
            private(&dir, true, self.directory.metadata().map_err(io)?.uid())?;
            let lease = lock(&dir, "active.lock")?;
            let seal: Seal = read(&dir, "seal.json")?;
            if seal.individual_id != d.lease.individual_id
                || seal.skill_id != d.reference.skill_id
                || seal.capsule_id != d.capsule_id
                || seal.privacy_epoch != d.lease.privacy_epoch
            {
                return Err(BridgeError::Stale);
            }
            Ok((path, dir, lease))
        }
    }
    fn vault(&self, d: &Dispatch) -> Result<SkillVault, BridgeError> {
        let name = d.vault_name.as_deref().ok_or(BridgeError::Stale)?;
        let suffix = name
            .strip_prefix("skills-")
            .ok_or(BridgeError::UnsafeStorage)?;
        if suffix.len() != 32
            || !suffix
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
        {
            return Err(BridgeError::UnsafeStorage);
        }
        SkillVault::open(
            &self.root.join(d.capsule_id.to_string()).join(name),
            self.launcher,
        )
        .map_err(|_| BridgeError::UnsafeStorage)
    }
    fn execute_sdk(&self, d: &Dispatch) -> Result<SdkOutcome, BridgeError> {
        let (path, _dir, _lease) = self.capsule(d)?;
        let vault = if matches!(d.command, SkillCommand::Candidate(_)) {
            SkillVault::create_in(&path, self.launcher).map_err(|_| BridgeError::UnsafeStorage)?
        } else {
            self.vault(d)?
        };
        let mut o = SdkOutcome {
            result: Err(SkillError::Unknown),
            sdk: None,
            vault_name: None,
            phase: None,
            operation: None,
            calls: 0,
            attested: 0,
            durable: false,
            report: None,
            cases: None,
        };
        o.vault_name = Some(
            vault
                .root()
                .file_name()
                .and_then(|x| x.to_str())
                .ok_or(BridgeError::UnsafeStorage)?
                .to_owned(),
        );
        let existing = d.sdk.as_ref();
        match &d.command {
            SkillCommand::Candidate(draft) => {
                let result = vault.create_candidate(draft.clone());
                o.durable = result.is_ok();
                match result {
                    Ok(v) => {
                        o.result = Ok(json!({"version":v}));
                        o.sdk = Some(v);
                        o.phase = Some("candidate")
                    }
                    Err(e) => o.result = Err(e),
                }
            }
            SkillCommand::Revise { draft, .. } => {
                let result = vault.revise(existing.ok_or(BridgeError::Stale)?, draft.clone());
                o.durable = result.is_ok();
                match result {
                    Ok(v) => {
                        o.result = Ok(json!({"version":v}));
                        o.sdk = Some(v);
                        o.phase = Some("candidate")
                    }
                    Err(e) => o.result = Err(e),
                }
            }
            SkillCommand::Validate { cases, .. } => {
                if !self.policy.execution_release {
                    return Err(BridgeError::Unreleased);
                }
                let attempt = vault.validate(existing.ok_or(BridgeError::Stale)?, &cases.sdk);
                o.operation = attempt.operation_id;
                o.calls = attempt.sandbox_calls;
                o.attested = attempt.attested_executions;
                o.durable = attempt.durable;
                match attempt.result {
                    Ok(verified) => {
                        o.report = Some(verified.report_sha256().into());
                        o.cases = Some(verified.case_set_sha256().into());
                        o.result = Ok(
                            json!({"report_sha256":o.report,"case_set_sha256":o.cases,"executions":attempt.executions}),
                        );
                        o.phase = Some("validated")
                    }
                    Err(e) => {
                        o.result = Err(e);
                        if o.durable {
                            o.phase = Some("candidate")
                        }
                    }
                }
            }
            SkillCommand::Register { grant, .. } => {
                let version = existing.ok_or(BridgeError::Stale)?;
                let view = vault
                    .view(version.individual_id, version.skill_id)
                    .map_err(|_| BridgeError::Stale)?;
                let verified = view.verified.ok_or(BridgeError::Stale)?;
                if verified.version() != version {
                    return Err(BridgeError::Stale);
                }
                let sdk_grant = RegistrationGrant::issue(
                    &verified,
                    grant.expires_at,
                    SkillScope::SandboxPython,
                )
                .map_err(|_| BridgeError::Denied)?;
                let result = vault.register(sdk_grant);
                o.durable = result.is_ok();
                match result {
                    Ok(r) => {
                        o.result = Ok(json!({"registration":r}));
                        o.phase = Some("registered")
                    }
                    Err(e) => o.result = Err(e),
                }
            }
            SkillCommand::Invoke {
                input_json,
                timeout_ms,
                ..
            } => {
                if !self.policy.execution_release {
                    return Err(BridgeError::Unreleased);
                }
                let attempt =
                    vault.invoke(existing.ok_or(BridgeError::Stale)?, input_json, *timeout_ms);
                o.operation = attempt.operation_id;
                o.calls = attempt.sandbox_calls;
                o.attested = attempt.attested_executions;
                o.durable = attempt.durable;
                o.result = attempt
                    .result
                    .map(|result| json!({"result":result,"execution":attempt.execution}));
            }
            SkillCommand::Revoke { reason, .. } => {
                let result = vault.revoke(existing.ok_or(BridgeError::Stale)?, reason);
                o.durable = result.is_ok();
                match result {
                    Ok(r) => {
                        o.result = Ok(json!({"revocation":r}));
                        o.phase = Some("revoked")
                    }
                    Err(e) => o.result = Err(e),
                }
            }
        }
        if o.sdk.is_none() {
            o.sdk = existing.cloned();
        }
        // A change of epoch rejects result publication even when SDK itself
        // returned a valid durable local receipt. Actual calls remain in o.
        if self
            .current_epoch(d.lease.individual_id, d.lease.privacy_epoch)
            .is_err()
        {
            o.result = Err(SkillError::Stale);
            o.durable = false;
        }
        Ok(o)
    }
    pub async fn execute(
        &self,
        store: &Store,
        lease: &RunLease,
        action: Uuid,
        command: SkillCommand,
    ) -> Result<BridgeReceipt, BridgeError> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| BridgeError::Busy)?;
        let admission = lease
            .deadline
            .signed_duration_since(Utc::now())
            .to_std()
            .map_err(|_| BridgeError::Stale)?
            .min(Duration::from_secs(5));
        let dispatch =
            match tokio::time::timeout(admission, self.begin(store, lease, action, command))
                .await
                .map_err(|_| BridgeError::Busy)??
            {
                Begin::Receipt(r) => return Ok(r),
                Begin::Dispatch(d) => Arc::new(*d),
            };
        let this = self.clone();
        let worker = dispatch.clone();
        let mut task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            this.execute_sdk(&worker)
        });
        // Keep lease renewal outside DB transactions spanning SDK execution.
        let outcome = loop {
            tokio::select! {
                result=&mut task=>break result.map_err(|_|BridgeError::Unknown)?,
                _=tokio::time::sleep(Duration::from_secs(5))=>{let _=store.renew_lease(lease).await;}
            }
        };
        tokio::time::timeout(
            Duration::from_secs(5),
            self.finish(store, &dispatch, outcome),
        )
        .await
        .map_err(|_| BridgeError::Unknown)?
    }
    pub async fn candidate(
        &self,
        store: &Store,
        lease: &RunLease,
        action: Uuid,
        draft: CandidateDraft,
    ) -> Result<BridgeReceipt, BridgeError> {
        self.execute(store, lease, action, SkillCommand::Candidate(draft))
            .await
    }
    pub async fn revise(
        &self,
        store: &Store,
        lease: &RunLease,
        action: Uuid,
        reference: SkillRef,
        draft: CandidateDraft,
    ) -> Result<BridgeReceipt, BridgeError> {
        self.execute(
            store,
            lease,
            action,
            SkillCommand::Revise { reference, draft },
        )
        .await
    }
    pub async fn validate(
        &self,
        store: &Store,
        lease: &RunLease,
        action: Uuid,
        reference: SkillRef,
        cases: TrustedCases,
    ) -> Result<BridgeReceipt, BridgeError> {
        self.execute(
            store,
            lease,
            action,
            SkillCommand::Validate { reference, cases },
        )
        .await
    }
    pub async fn register(
        &self,
        store: &Store,
        lease: &RunLease,
        action: Uuid,
        reference: SkillRef,
        grant: TrustedSkillGrant,
    ) -> Result<BridgeReceipt, BridgeError> {
        self.execute(
            store,
            lease,
            action,
            SkillCommand::Register { reference, grant },
        )
        .await
    }
    pub async fn invoke(
        &self,
        store: &Store,
        lease: &RunLease,
        action: Uuid,
        reference: SkillRef,
        input_json: String,
        deadline: std::time::Instant,
    ) -> Result<BridgeReceipt, BridgeError> {
        let timeout_ms = deadline
            .saturating_duration_since(std::time::Instant::now())
            .as_millis()
            .min(10000) as u64;
        self.execute(
            store,
            lease,
            action,
            SkillCommand::Invoke {
                reference,
                input_json,
                timeout_ms,
            },
        )
        .await
    }
    pub async fn revoke(
        &self,
        store: &Store,
        lease: &RunLease,
        action: Uuid,
        reference: SkillRef,
        reason: String,
    ) -> Result<BridgeReceipt, BridgeError> {
        self.execute(
            store,
            lease,
            action,
            SkillCommand::Revoke { reference, reason },
        )
        .await
    }
}
impl LearnedCapabilities {
    async fn finish(
        &self,
        store: &Store,
        d: &Dispatch,
        outcome: Result<SdkOutcome, BridgeError>,
    ) -> Result<BridgeReceipt, BridgeError> {
        // Accounting is allowed after lease/privacy invalidation. This transaction
        // can publish private/domain results only after the ordinary fresh gates.
        let mut tx = store.pool.begin().await.map_err(db)?;
        sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let row:Option<(String,i32,i32,String,i64)>=sqlx::query_as("SELECT status,reserved_slots,charged_slots,request_digest,lease_generation FROM learned_skill_actions WHERE individual_id=$1 AND action_id=$2 FOR UPDATE").bind(d.lease.individual_id).bind(d.action).fetch_optional(&mut *tx).await.map_err(db)?;
        let (status, reserved, charged, request, generation) = row.ok_or(BridgeError::Unknown)?;
        if request != d.request_digest
            || generation != d.lease.generation
            || reserved != d.slots as i32
        {
            return Err(BridgeError::Stale);
        }
        if status == "settled" {
            return Err(BridgeError::Invalid);
        }
        let actual = outcome.as_ref().ok().map(|o| o.calls);
        let attested = outcome.as_ref().ok().map(|o| o.attested);
        let counts_valid = actual.is_some_and(|n| n <= d.slots)
            && attested.zip(actual).is_some_and(|(a, n)| a <= n);
        let current = state(&mut tx).await?.ok_or(BridgeError::Stale)?;
        let usable = store.require_privacy_snapshot(d.generation).is_ok()
            && self
                .current_epoch(current.id, current.privacy_epoch)
                .is_ok()
            && current.id == d.lease.individual_id
            && current.privacy_epoch == d.lease.privacy_epoch
            && current.config_version == d.lease.config_version
            && enabled(&current.runtime_config)
            && require_lease(&mut tx, &d.lease).await.is_ok()
            && sources(&mut tx, current.id, &d.source_ids).await.is_ok()
            && check_grant(
                &mut tx,
                current.id,
                &d.command,
                current.config_version,
                current.privacy_epoch,
            )
            .await
            .is_ok();
        let known = status == "running"
            && counts_valid
            && usable
            && outcome.as_ref().is_ok_and(|o| o.durable);
        if !known {
            sqlx::query("UPDATE learned_skill_actions SET status='unknown',actual_calls=$3,attested_calls=$4,sdk_operation_id=$5,outcome=NULL,receipt=NULL WHERE individual_id=$1 AND action_id=$2").bind(d.lease.individual_id).bind(d.action).bind(if counts_valid{actual.map(|n|n as i32)}else{None}).bind(if counts_valid{attested.map(|n|n as i32)}else{None}).bind(outcome.as_ref().ok().and_then(|o|o.operation)).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("UPDATE actions SET status='unknown' WHERE individual_id=$1 AND id=$2 AND status='executing'").bind(d.lease.individual_id).bind(d.action).execute(&mut *tx).await.map_err(db)?;
            tx.commit().await.map_err(db)?;
            return Err(if usable {
                BridgeError::Unknown
            } else {
                BridgeError::Stale
            });
        }
        let o = outcome.map_err(|_| BridgeError::Unknown)?;
        let slots = o.calls.max(1) as i32;
        if charged > slots {
            sqlx::query("UPDATE runs SET tool_calls=tool_calls-$3 WHERE individual_id=$1 AND id=$2 AND tool_calls>=$3").bind(current.id).bind(d.lease.run_id).bind(charged-slots).execute(&mut *tx).await.map_err(db)?;
        }
        if let Some(sdk) = &o.sdk {
            if sdk.individual_id != current.id
                || sdk.version != d.reference.version as u32
                || sdk.source_sha256 != d.reference.source_sha256
            {
                return Err(BridgeError::Stale);
            }
            let updated=sqlx::query("UPDATE learned_skills SET sdk_skill_id=$3,vault_name=$4,current_version=$5 WHERE individual_id=$1 AND id=$2 AND NOT invalidated AND privacy_epoch=$6").bind(current.id).bind(d.reference.skill_id).bind(sdk.skill_id).bind(&o.vault_name).bind(d.reference.version).bind(current.privacy_epoch).execute(&mut *tx).await.map_err(db)?;
            if updated.rows_affected() != 1 {
                return Err(BridgeError::Stale);
            }
        }
        if let Some(phase) = o.phase {
            sqlx::query("UPDATE learned_skill_versions SET phase=$4,report_sha256=CASE WHEN $4='candidate' THEN NULL ELSE coalesce($5,report_sha256) END,case_set_sha256=CASE WHEN $4='candidate' THEN NULL ELSE coalesce($6,case_set_sha256) END WHERE individual_id=$1 AND skill_id=$2 AND version=$3").bind(current.id).bind(d.reference.skill_id).bind(d.reference.version).bind(phase).bind(&o.report).bind(&o.cases).execute(&mut *tx).await.map_err(db)?;
            if matches!(phase, "candidate" | "validated" | "revoked") {
                sqlx::query("UPDATE learned_skill_grants SET revoked=true WHERE individual_id=$1 AND skill_id=$2").bind(current.id).bind(d.reference.skill_id).execute(&mut *tx).await.map_err(db)?;
                sqlx::query("UPDATE learned_skill_versions SET registration_grant_id=NULL WHERE individual_id=$1 AND skill_id=$2").bind(current.id).bind(d.reference.skill_id).execute(&mut *tx).await.map_err(db)?;
            }
        }
        if let SkillCommand::Register { grant, .. } = &d.command {
            sqlx::query("UPDATE learned_skill_versions SET registration_grant_id=$4 WHERE individual_id=$1 AND skill_id=$2 AND version=$3").bind(current.id).bind(d.reference.skill_id).bind(d.reference.version).bind(grant.id).execute(&mut *tx).await.map_err(db)?;
        }
        let succeeded = o.result.is_ok();
        let event = Uuid::new_v4();
        let version = current
            .state_version
            .checked_add(1)
            .ok_or(BridgeError::Invalid)?;
        let payload = json!({"action_id":d.action,"capability_id":CAPABILITY_ID,"capability_version":BRIDGE_VERSION,"status":if succeeded{"succeeded"}else{"failed"},"executed":o.calls>0,"execution_mode":"sandbox","output":o.result,"sdk_operation_id":o.operation,"sandbox_calls":o.calls,"attested_executions":o.attested});
        if serde_json::to_vec(&payload)
            .map_err(|_| BridgeError::Storage)?
            .len()
            > 1024 * 1024
        {
            return Err(BridgeError::Unknown);
        }
        insert_event(
            &mut tx,
            current.id,
            event,
            "tool.result",
            "direct_observation",
            "capability",
            payload.clone(),
            d.lease.run_id,
            None,
        )
        .await?;
        for source in d
            .source_ids
            .iter()
            .copied()
            .chain([d.action, d.reference.skill_id])
        {
            if source != event {
                edge(&mut tx, current.id, source, event).await?;
            }
        }
        if let SkillCommand::Register { grant, .. } = &d.command {
            edge(&mut tx, current.id, grant.id, event).await?;
        }
        let receipt = BridgeReceipt {
            action: ActionReceipt {
                action_id: d.action,
                result_event_id: event,
                status: if succeeded { "succeeded" } else { "failed" }.into(),
                state_version: version,
                executed: o.calls > 0,
            },
            reference: d.reference.clone(),
            sdk_operation_id: o.operation,
            sandbox_calls: o.calls,
            attested_executions: o.attested,
            reserved_slots: d.slots,
            charged_slots: slots as u32,
            sdk_durable: o.durable,
        };
        sqlx::query("UPDATE learned_skill_actions SET status='settled',actual_calls=$3,attested_calls=$4,charged_slots=$5,sdk_operation_id=$6,outcome=$7,outcome_digest=$8,receipt=$9 WHERE individual_id=$1 AND action_id=$2").bind(current.id).bind(d.action).bind(o.calls as i32).bind(o.attested as i32).bind(slots).bind(o.operation).bind(&payload).bind(digest(&payload)?).bind(json!(receipt)).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE actions SET status=$3,result_event_id=$4,receipt=$5,result_digest=$6,finished_at=clock_timestamp() WHERE individual_id=$1 AND id=$2").bind(current.id).bind(d.action).bind(&receipt.action.status).bind(event).bind(json!(receipt)).bind(digest(&payload)?).execute(&mut *tx).await.map_err(db)?;
        let mut manifest = d.source_ids.clone();
        manifest.extend([event, d.reference.skill_id]);
        manifest.sort();
        manifest.dedup();
        sqlx::query(
            "UPDATE runs SET state_version=$3,input_manifest=$4 WHERE individual_id=$1 AND id=$2",
        )
        .bind(current.id)
        .bind(d.lease.run_id)
        .bind(version)
        .bind(manifest)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        advance(&mut tx, current.id, version).await?;
        outbox(
            &mut tx,
            current.id,
            "operation.updated",
            d.lease.run_id,
            version,
        )
        .await?;
        store.require_privacy_snapshot(d.generation)?;
        self.current_epoch(current.id, current.privacy_epoch)?;
        tx.commit().await.map_err(db)?;
        store.require_privacy_snapshot(d.generation)?;
        Ok(receipt)
    }
    /// Root owner/controller only. The resulting opaque handle never changes the
    /// runtime allow-list, and register rechecks both this proof and SDK evidence.
    pub async fn authorize_registration(
        &self,
        store: &Store,
        key: &str,
        expected: ExpectedState,
        reference: SkillRef,
        expires_at: DateTime<Utc>,
    ) -> Result<TrustedSkillGrant, BridgeError> {
        // Preserve the exact owner request for idempotency, then bind the
        // effective expiry to the existing SDK's millisecond precision. This
        // rounds down and never extends the requested permission period.
        let hash = digest(&json!({"reference":reference,"expires_at":expires_at}))?;
        let expires_at = DateTime::<Utc>::from_timestamp_millis(expires_at.timestamp_millis())
            .ok_or(BridgeError::Invalid)?;
        if expires_at <= Utc::now() {
            return Err(BridgeError::Invalid);
        }
        let mut tx = store.write().await?;
        let current = state(&mut tx).await?.ok_or(BridgeError::Stale)?;
        self.current_epoch(current.id, current.privacy_epoch)?;
        if !matches!(current.lifecycle, Lifecycle::Active | Lifecycle::Paused)
            || !enabled(&current.runtime_config)
        {
            return Err(BridgeError::Denied);
        }
        if let Some(old) = super::store::replay::<Value>(
            &mut tx,
            "owner",
            "/internal/learned-skills/grants",
            key,
            &hash,
        )
        .await?
        {
            let id: Uuid =
                serde_json::from_value(old["id"].clone()).map_err(|_| BridgeError::Storage)?;
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learned_skill_grants WHERE individual_id=$1 AND id=$2 AND skill_id=$3 AND version=$4 AND source_sha256=$5 AND expires_at=$6 AND config_version=$7 AND privacy_epoch=$8 AND NOT revoked)").bind(current.id).bind(id).bind(reference.skill_id).bind(reference.version).bind(&reference.source_sha256).bind(expires_at).bind(current.config_version).bind(current.privacy_epoch).fetch_one(&mut *tx).await.map_err(db)?;
            if !valid {
                return Err(BridgeError::Stale);
            }
            sources(&mut tx, current.id, &[id, reference.skill_id]).await?;
            return Ok(TrustedSkillGrant {
                id,
                individual_id: current.id,
                reference,
                expires_at,
            });
        }
        if expected.version != current.state_version
            || expected.privacy_epoch != current.privacy_epoch
        {
            return Err(BridgeError::Stale);
        }
        let b = binding(&mut tx, current.id, &reference).await?;
        if b.phase != "validated" {
            return Err(BridgeError::Denied);
        }
        sources(&mut tx, current.id, &[reference.skill_id]).await?;
        let id = Uuid::new_v4();
        let version = current.state_version + 1;
        insert_event(&mut tx,current.id,id,"skill.owner_grant","direct_observation","owner",json!({"skill_id":reference.skill_id,"version":reference.version,"source_sha256":reference.source_sha256,"expires_at":expires_at,"scope":CAPABILITY_ID}),id,Some(key)).await?;
        edge(&mut tx, current.id, reference.skill_id, id).await?;
        sqlx::query("INSERT INTO learned_skill_grants(individual_id,id,skill_id,version,source_sha256,expires_at,scope,config_version,privacy_epoch) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(current.id).bind(id).bind(reference.skill_id).bind(reference.version).bind(&reference.source_sha256).bind(expires_at).bind(CAPABILITY_ID).bind(current.config_version).bind(current.privacy_epoch).execute(&mut *tx).await.map_err(db)?;
        advance(&mut tx, current.id, version).await?;
        outbox(&mut tx, current.id, "state.changed", current.id, version).await?;
        super::store::save_receipt(
            &mut tx,
            "owner",
            "/internal/learned-skills/grants",
            key,
            &hash,
            &json!({"id":id}),
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(TrustedSkillGrant {
            id,
            individual_id: current.id,
            reference,
            expires_at,
        })
    }
    /// No caller can reconstruct and repeat an SDK dispatch from an input hash.
    /// Pending actions retain their already reserved slots as unknown.
    pub async fn recover(&self, store: &Store) -> Result<Vec<Uuid>, BridgeError> {
        let mut tx = store.write().await?;
        let current = state(&mut tx).await?.ok_or(BridgeError::Stale)?;
        self.current_epoch(current.id, current.privacy_epoch)?;
        let ids:Vec<Uuid>=sqlx::query_scalar("UPDATE learned_skill_actions SET status='unknown',outcome=NULL,receipt=NULL WHERE individual_id=$1 AND status='running' RETURNING action_id").bind(current.id).fetch_all(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE actions SET status='unknown' WHERE individual_id=$1 AND id=ANY($2) AND status='executing'").bind(current.id).bind(&ids).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(ids)
    }
    /// Caller owns the original deletion transaction. Advance external floor
    /// before acceptance; a rollback cannot make an old SDK capsule usable.
    pub async fn invalidate_in(
        &self,
        tx: &mut Tx<'_>,
        individual: Uuid,
        epoch: i64,
    ) -> Result<(), BridgeError> {
        let actual: (Uuid, i64) =
            sqlx::query_as("SELECT id,privacy_epoch FROM individuals WHERE singleton")
                .fetch_one(&mut **tx)
                .await
                .map_err(db)?;
        if actual != (individual, epoch) {
            return Err(BridgeError::Stale);
        }
        let _lock = lock(&self.directory, "bridge.lock")?;
        let old = self.epoch()?;
        if old.individual_id != individual || old.privacy_epoch > epoch {
            return Err(BridgeError::Stale);
        }
        replace(
            &self.directory,
            "authority.json",
            &Epoch {
                individual_id: individual,
                privacy_epoch: epoch,
            },
        )?;
        sqlx::query("UPDATE learned_skills SET invalidated=true WHERE individual_id=$1 AND privacy_epoch<$2").bind(individual).bind(epoch).execute(&mut **tx).await.map_err(db)?;
        sqlx::query("UPDATE learned_skill_versions SET phase='invalidated',report_sha256=NULL,case_set_sha256=NULL,registration_grant_id=NULL WHERE individual_id=$1 AND skill_id IN (SELECT id FROM learned_skills WHERE individual_id=$1 AND invalidated)").bind(individual).execute(&mut **tx).await.map_err(db)?;
        sqlx::query("UPDATE learned_skill_grants SET revoked=true WHERE individual_id=$1 AND privacy_epoch<$2").bind(individual).bind(epoch).execute(&mut **tx).await.map_err(db)?;
        sqlx::query("UPDATE events SET payload='{}' WHERE individual_id=$1 AND (id IN (SELECT s.id FROM learned_skills s WHERE s.individual_id=$1 AND s.invalidated) OR id IN (SELECT a.result_event_id FROM actions a JOIN learned_skill_actions l ON l.individual_id=a.individual_id AND l.action_id=a.id WHERE a.individual_id=$1 AND l.privacy_epoch<$2))").bind(individual).bind(epoch).execute(&mut **tx).await.map_err(db)?;
        sqlx::query("UPDATE actions SET arguments='{}',receipt=NULL,status=CASE WHEN status IN ('planned','executing') THEN 'unknown' ELSE status END WHERE individual_id=$1 AND id IN (SELECT action_id FROM learned_skill_actions WHERE individual_id=$1 AND privacy_epoch<$2)").bind(individual).bind(epoch).execute(&mut **tx).await.map_err(db)?;
        sqlx::query("UPDATE learned_skill_actions SET status='unknown',outcome=NULL,receipt=NULL WHERE individual_id=$1 AND privacy_epoch<$2").bind(individual).bind(epoch).execute(&mut **tx).await.map_err(db)?;
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct SkillCleanup {
    pub removed: u32,
    pub remaining: u32,
}
fn erase_private_tree(path: &Path, uid: u32) -> Result<(), BridgeError> {
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(path)
        .map_err(io)?;
    private(&dir, true, uid)?;
    for entry in fs::read_dir(fd(&dir, ".")).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        let meta = fs::symlink_metadata(fd(&dir, name.to_str().ok_or(BridgeError::UnsafeStorage)?))
            .map_err(io)?;
        if meta.file_type().is_symlink() {
            return Err(BridgeError::UnsafeStorage);
        }
        let child = fd(&dir, name.to_str().ok_or(BridgeError::UnsafeStorage)?);
        if meta.is_dir() {
            erase_private_tree(&child, uid)?;
        } else {
            let f = file(
                &dir,
                name.to_str().ok_or(BridgeError::UnsafeStorage)?,
                false,
            )?;
            private(&f, false, uid)?;
            fs::remove_file(&child).map_err(io)?;
        }
    }
    fs::remove_dir(path).map_err(io)
}
impl LearnedCapabilities {
    pub fn purge_invalidated(
        &self,
        individual: Uuid,
        epoch: i64,
    ) -> Result<SkillCleanup, BridgeError> {
        self.current_epoch(individual, epoch)?;
        let _root_lock = lock(&self.directory, "bridge.lock")?;
        let mut result = SkillCleanup {
            removed: 0,
            remaining: 0,
        };
        let uid = self.directory.metadata().map_err(io)?.uid();
        for entry in fs::read_dir(fd(&self.directory, ".")).map_err(io)? {
            let name = entry.map_err(io)?.file_name();
            let name = name.to_str().ok_or(BridgeError::UnsafeStorage)?;
            if matches!(name, "bridge.lock" | "authority.json") {
                continue;
            }
            let id = Uuid::parse_str(name).map_err(|_| BridgeError::UnsafeStorage)?;
            if id.to_string() != name {
                return Err(BridgeError::UnsafeStorage);
            }
            let path = fd(&self.directory, name);
            let dir = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
                .open(&path)
                .map_err(io)?;
            private(&dir, true, uid)?;
            let seal: Seal = read(&dir, "seal.json")?;
            if seal.capsule_id != id
                || seal.individual_id != individual
                || seal.privacy_epoch > epoch
            {
                return Err(BridgeError::Stale);
            }
            if seal.privacy_epoch == epoch {
                continue;
            }
            let _lease = match lock(&dir, "active.lock") {
                Ok(v) => v,
                Err(BridgeError::Busy) => {
                    result.remaining += 1;
                    continue;
                }
                Err(e) => return Err(e),
            };
            erase_private_tree(&path, uid)?;
            self.directory.sync_all().map_err(io)?;
            result.removed += 1;
        }
        Ok(result)
    }
    pub fn invalidated_clean(&self, individual: Uuid, epoch: i64) -> Result<bool, BridgeError> {
        self.current_epoch(individual, epoch)?;
        let _lock = lock(&self.directory, "bridge.lock")?;
        let uid = self.directory.metadata().map_err(io)?.uid();
        for entry in fs::read_dir(fd(&self.directory, ".")).map_err(io)? {
            let name = entry.map_err(io)?.file_name();
            let name = name.to_str().ok_or(BridgeError::UnsafeStorage)?;
            if matches!(name, "bridge.lock" | "authority.json") {
                continue;
            }
            let id = Uuid::parse_str(name).map_err(|_| BridgeError::UnsafeStorage)?;
            if id.to_string() != name {
                return Err(BridgeError::UnsafeStorage);
            }
            let dir = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
                .open(fd(&self.directory, name))
                .map_err(io)?;
            private(&dir, true, uid)?;
            let seal: Seal = read(&dir, "seal.json")?;
            if seal.individual_id != individual
                || seal.capsule_id != id
                || seal.privacy_epoch > epoch
            {
                return Err(BridgeError::Stale);
            }
            if seal.privacy_epoch < epoch {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// A restored DB cannot authorize old code by replaying its registered flag.
    /// Existing external epoch and actual SDK head/source/report must agree.
    pub async fn verify_restored(&self, store: &Store) -> Result<(), BridgeError> {
        let generation = store.privacy_generation();
        store.require_privacy_ready()?;
        let mut tx = store.pool.begin().await.map_err(db)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let s = super::store::read_state(&mut tx)
            .await?
            .ok_or(BridgeError::Stale)?;
        self.current_epoch(s.id, s.privacy_epoch)?;
        if !self.invalidated_clean(s.id, s.privacy_epoch)? {
            return Err(BridgeError::Unknown);
        }

        let rows:Vec<(Uuid,i32,String)>=sqlx::query_as("SELECT s.id,s.current_version,v.source_sha256 FROM learned_skills s JOIN learned_skill_versions v ON (v.individual_id,v.skill_id,v.version)=(s.individual_id,s.id,s.current_version) WHERE s.individual_id=$1 AND NOT s.invalidated ORDER BY s.id LIMIT 1025").bind(s.id).fetch_all(&mut *tx).await.map_err(db)?;
        if rows.len() > 1024 {
            return Err(BridgeError::Busy);
        }
        for (id, version, hash) in rows {
            let r = SkillRef {
                skill_id: id,
                version,
                source_sha256: hash,
            };
            let b = binding(&mut tx, s.id, &r).await?;
            sources(&mut tx, s.id, &[id]).await?;
            let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learned_skill_actions WHERE individual_id=$1 AND skill_id=$2 AND status IN ('running','unknown'))").bind(s.id).bind(id).fetch_one(&mut *tx).await.map_err(db)?;
            if pending || b.privacy_epoch != s.privacy_epoch {
                return Err(BridgeError::Unknown);
            }
            let name = b.vault_name.ok_or(BridgeError::Stale)?;
            let suffix = name
                .strip_prefix("skills-")
                .ok_or(BridgeError::UnsafeStorage)?;
            if suffix.len() != 32
                || !suffix
                    .bytes()
                    .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
            {
                return Err(BridgeError::UnsafeStorage);
            }
            let capsule = self.root.join(b.capsule_id.to_string());
            let capsule_dir = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
                .open(&capsule)
                .map_err(io)?;
            private(
                &capsule_dir,
                true,
                self.directory.metadata().map_err(io)?.uid(),
            )?;
            let seal: Seal = read(&capsule_dir, "seal.json")?;
            if seal.individual_id != s.id
                || seal.skill_id != id
                || seal.capsule_id != b.capsule_id
                || seal.privacy_epoch != s.privacy_epoch
            {
                return Err(BridgeError::Stale);
            }
            let path = capsule.join(name);
            let owner = s.id;
            let sdk_id = b.sdk_skill_id.ok_or(BridgeError::Stale)?;
            let launcher = self.launcher;
            let view = tokio::task::spawn_blocking(move || {
                SkillVault::open(&path, launcher)?.view(owner, sdk_id)
            })
            .await
            .map_err(|_| BridgeError::Unknown)?
            .map_err(|_| BridgeError::Stale)?;
            let phase = match view.phase {
                SkillPhase::Candidate => "candidate",
                SkillPhase::Validated => "validated",
                SkillPhase::Registered => "registered",
                SkillPhase::Revoked => "revoked",
            };
            if view.version.individual_id != s.id
                || view.version.skill_id != sdk_id
                || view.version.version != version as u32
                || view.version.source_sha256 != r.source_sha256
                || phase != b.phase
                || !view.pending_operations.is_empty()
            {
                return Err(BridgeError::Stale);
            }
            let mut actual_sources:Vec<Uuid>=sqlx::query_scalar("SELECT event_id FROM learned_skill_sources WHERE individual_id=$1 AND skill_id=$2 AND version=$3 ORDER BY event_id").bind(s.id).bind(id).bind(version).fetch_all(&mut *tx).await.map_err(db)?;
            let mut sdk_sources = view.source_event_ids.clone();
            sdk_sources.sort();
            actual_sources.sort();
            if sdk_sources != actual_sources {
                return Err(BridgeError::Stale);
            }
            if phase == "registered" {
                let command = SkillCommand::Invoke {
                    reference: r.clone(),
                    input_json: "null".into(),
                    timeout_ms: 100,
                };
                check_grant(&mut tx, s.id, &command, s.config_version, s.privacy_epoch).await?;
                let expires:DateTime<Utc>=sqlx::query_scalar("SELECT g.expires_at FROM learned_skill_versions v JOIN learned_skill_grants g ON (g.individual_id,g.id)=(v.individual_id,v.registration_grant_id) WHERE v.individual_id=$1 AND v.skill_id=$2 AND v.version=$3").bind(s.id).bind(id).bind(version).fetch_one(&mut *tx).await.map_err(db)?;
                if view.registration_expires_at.map(|d| d.timestamp_millis())
                    != Some(expires.timestamp_millis())
                {
                    return Err(BridgeError::Stale);
                }
            }
            let proof:(Option<String>,Option<String>)=sqlx::query_as("SELECT report_sha256,case_set_sha256 FROM learned_skill_versions WHERE individual_id=$1 AND skill_id=$2 AND version=$3").bind(s.id).bind(id).bind(version).fetch_one(&mut *tx).await.map_err(db)?;
            if proof.0.as_deref() != view.verified.as_ref().map(|v| v.report_sha256())
                || proof.1.as_deref() != view.verified.as_ref().map(|v| v.case_set_sha256())
            {
                return Err(BridgeError::Stale);
            }
        }
        store.require_privacy_snapshot(generation)?;
        self.current_epoch(s.id, s.privacy_epoch)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "learned_capabilities_tests.rs"]
mod allowed_positive_tests;
