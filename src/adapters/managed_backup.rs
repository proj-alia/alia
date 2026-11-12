//! Private PG17 archives and fenced recovery. No HTTP, scheduler or production activation.
use super::{
    deletion_journal::DeletionJournal,
    privacy_development::verify_development_projection_in,
    store::{Store, digest},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{
    ConnectOptions, PgPool, Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        io::AsRawFd,
    },
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;
const FORMAT: &str = "alia-managed-pg-backup/1";
const SCHEMA: &str = "eb00e02e410278d5ca86d0efed7ea3364dba839f48a01615f68ad6f5129e5976";
const CATALOG_FINGERPRINT: &str =
    "7f22bd56d77da3d9711120a634b031183900b0e754215b122c28457fee6dc52a";
const CATALOG_QUERY: &str = r#"SELECT jsonb_build_object(
 'columns',(SELECT jsonb_agg(jsonb_build_array(table_name,column_name,data_type,udt_name,is_nullable,column_default,is_identity,identity_generation,is_generated,generation_expression,collation_name,character_maximum_length,numeric_precision,numeric_scale,datetime_precision) ORDER BY table_name,ordinal_position) FROM information_schema.columns WHERE table_schema='public'),
 'constraints',(SELECT jsonb_agg(jsonb_build_array(c.relname,k.conname,pg_get_constraintdef(k.oid,true)) ORDER BY c.relname,k.conname) FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'),
 'indexes',(SELECT jsonb_agg(jsonb_build_array(indexname,indexdef) ORDER BY indexname) FROM pg_indexes WHERE schemaname='public'),
 'relations',(SELECT jsonb_agg(jsonb_build_array(c.relname,c.relkind::text,c.relpersistence::text,c.relrowsecurity,c.relforcerowsecurity) ORDER BY c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind IN ('r','p','v','m','f','S')),
 'sequences',(SELECT jsonb_agg(jsonb_build_array(sequencename,data_type::text,start_value,min_value,max_value,increment_by,cycle,cache_size) ORDER BY sequencename) FROM pg_sequences WHERE schemaname='public'),
 'types',(SELECT jsonb_agg(jsonb_build_array(t.typname,t.typtype::text,t.typelem::regtype::text,t.typrelid::regclass::text,t.typnotnull,t.typdefault) ORDER BY t.typname) FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='public')
)"#;
const DUMP: &str = "/usr/lib/postgresql/17/bin/pg_dump";
const RESTORE: &str = "/usr/lib/postgresql/17/bin/pg_restore";
const META_MAX: u64 = 65536;
const WRITE_GATE: i64 = 1095518529;
static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupFailureCode {
    Invalid,
    UnsafePath,
    Unsupported,
    Busy,
    Stale,
    TooLarge,
    Timeout,
    ToolUnavailable,
    ProcessFailed,
    Database,
    Io,
    Journal,
    Incomplete,
    CleanupFailed,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("managed backup operation failed: {code:?}")]
pub struct BackupError {
    pub code: BackupFailureCode,
    pub operation_id: Option<Uuid>,
    pub cleanup_complete: bool,
}
fn error(code: BackupFailureCode) -> BackupError {
    BackupError {
        code,
        operation_id: None,
        cleanup_complete: false,
    }
}
fn io(_: std::io::Error) -> BackupError {
    error(BackupFailureCode::Io)
}
fn db(_: sqlx::Error) -> BackupError {
    error(BackupFailureCode::Database)
}
#[derive(Debug, Clone, Copy)]
pub struct BackupLimits {
    pub max_archive_bytes: u64,
    pub timeout: Duration,
}
impl Default for BackupLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 1024 * 1024 * 1024,
            timeout: Duration::from_secs(120),
        }
    }
}
impl BackupLimits {
    fn validate(self) -> Result<Self, BackupError> {
        if self.max_archive_bytes == 0
            || self.max_archive_bytes > 1024 * 1024 * 1024
            || self.timeout.is_zero()
            || self.timeout > Duration::from_secs(600)
        {
            Err(error(BackupFailureCode::Invalid))
        } else {
            Ok(self)
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupManifest {
    pub format: String,
    pub application_version: String,
    pub backup_id: Uuid,
    pub individual_id: Uuid,
    pub state_version: i64,
    pub privacy_epoch: i64,
    pub config_version: i64,
    pub created_at: DateTime<Utc>,
    pub schema_version: i64,
    pub schema_fingerprint: String,
    pub catalog_fingerprint: String,
    pub archive_profile: String,
    pub database_settings: Value,
    pub migration_fingerprint: String,
    pub postgres_major: u32,
    pub archive_bytes: u64,
    pub archive_sha256: String,
    pub journal_records: u64,
    pub journal_digest: String,
    pub extensions: BTreeMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    format: String,
    individual_id: Uuid,
    privacy_epoch: i64,
    journal_records: u64,
    journal_digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    format: String,
    backup_id: Uuid,
    individual_id: Uuid,
    privacy_epoch: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupCleanup {
    pub removed: u32,
    pub remaining: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreReceipt {
    pub recovery_id: Uuid,
    pub backup_id: Uuid,
    pub individual_id: Uuid,
    pub state_version: i64,
    pub privacy_epoch: i64,
    pub status: String,
}
impl RestoreReceipt {
    pub fn database_name(&self) -> String {
        recovery_name(self.recovery_id)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogEntry {
    format: String,
    backup_id: Uuid,
    individual_id: Uuid,
    privacy_epoch: i64,
    manifest_digest: String,
}
#[derive(Clone)]
pub struct ManagedBackupManager {
    root: Arc<File>,
    artifacts: Arc<File>,
    recoveries: Arc<File>,
    catalog: Arc<File>,
    limits: BackupLimits,
}
fn fd_path(dir: &File, name: &str) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd())).join(name)
}
fn private(file: &File, mode: u32, directory: bool) -> Result<(), BackupError> {
    let m = file.metadata().map_err(io)?;
    if (directory && !m.is_dir())
        || (!directory && (!m.is_file() || m.nlink() != 1))
        || m.mode() & 0o7777 != mode
    {
        return Err(error(BackupFailureCode::UnsafePath));
    }
    Ok(())
}
fn open_dir(path: &Path) -> Result<File, BackupError> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| error(BackupFailureCode::UnsafePath))?;
    private(&f, 0o700, true)?;
    Ok(f)
}
fn open_file(dir: &File, name: &str, create: bool) -> Result<File, BackupError> {
    let mut op = OpenOptions::new();
    op.read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    if create {
        op.create_new(true);
    }
    let f = op
        .open(fd_path(dir, name))
        .map_err(|_| error(BackupFailureCode::UnsafePath))?;
    private(&f, 0o600, false)?;
    if f.metadata().map_err(io)?.uid() != dir.metadata().map_err(io)?.uid() {
        return Err(error(BackupFailureCode::UnsafePath));
    }
    Ok(f)
}
fn subdir(dir: &File, name: &str, create: bool) -> Result<File, BackupError> {
    if create {
        match fs::DirBuilder::new().mode(0o700).create(fd_path(dir, name)) {
            Ok(()) => dir.sync_all().map_err(io)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io(e)),
        }
    }
    let f = open_dir(&fd_path(dir, name))?;
    if f.metadata().map_err(io)?.uid() != dir.metadata().map_err(io)?.uid() {
        return Err(error(BackupFailureCode::UnsafePath));
    }
    Ok(f)
}
fn lock(dir: &File) -> Result<File, BackupError> {
    let f = match open_file(dir, "lease.lock", true) {
        Ok(f) => f,
        Err(_) => open_file(dir, "lease.lock", false)?,
    };
    f.try_lock().map_err(|_| error(BackupFailureCode::Busy))?;
    Ok(f)
}
fn lock_existing(dir: &File) -> Result<File, BackupError> {
    let f = open_file(dir, "lease.lock", false)?;
    f.try_lock().map_err(|_| error(BackupFailureCode::Busy))?;
    Ok(f)
}
fn new_lease(dir: &File) -> Result<File, BackupError> {
    let f = open_file(dir, "lease.lock", true)?;
    f.try_lock().map_err(|_| error(BackupFailureCode::Busy))?;
    Ok(f)
}
fn read_meta<T: serde::de::DeserializeOwned>(dir: &File, name: &str) -> Result<T, BackupError> {
    let mut f = open_file(dir, name, false)?;
    if f.metadata().map_err(io)?.len() > META_MAX {
        return Err(error(BackupFailureCode::TooLarge));
    }
    let mut b = vec![];
    (&mut f)
        .take(META_MAX + 1)
        .read_to_end(&mut b)
        .map_err(io)?;
    if b.len() as u64 > META_MAX {
        return Err(error(BackupFailureCode::TooLarge));
    }
    let value = crate::domain::config::parse_strict_json(&b, META_MAX as usize)
        .map_err(|_| error(BackupFailureCode::Invalid))?;
    serde_json::from_value(value).map_err(|_| error(BackupFailureCode::Invalid))
}
fn write_new(dir: &File, name: &str, value: &impl Serialize) -> Result<(), BackupError> {
    let bytes = serde_json::to_vec(value).map_err(|_| error(BackupFailureCode::Invalid))?;
    if bytes.len() as u64 > META_MAX {
        return Err(error(BackupFailureCode::TooLarge));
    }
    let mut f = open_file(dir, name, true)?;
    f.write_all(&bytes).map_err(io)?;
    f.sync_all().map_err(io)?;
    dir.sync_all().map_err(io)
}
fn replace_meta(dir: &File, name: &str, value: &impl Serialize) -> Result<(), BackupError> {
    let temp = format!("{}.pending", Uuid::new_v4());
    write_new(dir, &temp, value)?;
    if fs::symlink_metadata(fd_path(dir, name)).is_ok() {
        let _ = open_file(dir, name, false)?;
    }
    fs::rename(fd_path(dir, &temp), fd_path(dir, name)).map_err(io)?;
    dir.sync_all().map_err(io)
}
fn hex(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn strict_id(name: &str) -> Result<Uuid, BackupError> {
    let id = Uuid::parse_str(name).map_err(|_| error(BackupFailureCode::UnsafePath))?;
    if id.is_nil() || id.to_string() != name {
        return Err(error(BackupFailureCode::UnsafePath));
    }
    Ok(id)
}
fn child_dirs(dir: &File) -> Result<Vec<Uuid>, BackupError> {
    let mut ids = vec![];
    for e in fs::read_dir(fd_path(dir, ".")).map_err(io)? {
        let e = e.map_err(io)?;
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| error(BackupFailureCode::UnsafePath))?;
        let id = strict_id(&name)?;
        if ids.len() >= 1024 {
            return Err(error(BackupFailureCode::TooLarge));
        }
        let _ = subdir(dir, &name, false)?;
        ids.push(id)
    }
    ids.sort();
    Ok(ids)
}
fn erase_artifact(parent: &File, id: Uuid) -> Result<(), BackupError> {
    let dir = subdir(parent, &id.to_string(), false)?;
    for e in fs::read_dir(fd_path(&dir, ".")).map_err(io)? {
        let e = e.map_err(io)?;
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| error(BackupFailureCode::UnsafePath))?;
        if !matches!(
            name.as_str(),
            "lease.lock" | "pending.json" | "archive.part" | "archive.pgcustom" | "manifest.json"
        ) {
            return Err(error(BackupFailureCode::UnsafePath));
        }
        let _ = open_file(&dir, &name, false)?;
        fs::remove_file(fd_path(&dir, &name)).map_err(io)?;
    }
    dir.sync_all().map_err(io)?;
    fs::remove_dir(fd_path(parent, &id.to_string())).map_err(io)?;
    parent.sync_all().map_err(io)
}
fn check_manifest(m: &BackupManifest, id: Uuid, limits: BackupLimits) -> Result<(), BackupError> {
    if m.format != FORMAT
        || m.application_version != env!("CARGO_PKG_VERSION")
        || m.backup_id != id
        || id.is_nil()
        || m.individual_id.is_nil()
        || m.state_version <= 0
        || m.privacy_epoch < 0
        || m.config_version <= 0
        || m.schema_version != 30
        || m.schema_fingerprint != SCHEMA
        || m.catalog_fingerprint != CATALOG_FINGERPRINT
        || m.archive_profile != "pg17-custom-uncompressed"
        || !m.database_settings.is_object()
        || m.postgres_major != 17
        || m.archive_bytes == 0
        || m.archive_bytes > limits.max_archive_bytes
        || !hex(&m.archive_sha256)
        || !hex(&m.journal_digest)
        || !hex(&m.migration_fingerprint)
    {
        return Err(error(BackupFailureCode::Invalid));
    }
    Ok(())
}
fn hash_file(f: &mut File, maximum: u64) -> Result<(u64, String), BackupError> {
    private(f, 0o600, false)?;
    f.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut hash = Sha256::new();
    let mut n = 0;
    let mut buf = [0u8; 65536];
    loop {
        let used = f.read(&mut buf).map_err(io)?;
        if used == 0 {
            break;
        }
        n += used as u64;
        if n > maximum {
            return Err(error(BackupFailureCode::TooLarge));
        }
        hash.update(&buf[..used]);
    }
    f.seek(SeekFrom::Start(0)).map_err(io)?;
    Ok((n, format!("{:x}", hash.finalize())))
}
fn decode_percent(value: &str) -> Result<String, BackupError> {
    let mut result = vec![];
    let b = value.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return Err(error(BackupFailureCode::Unsupported));
            }
            let part = std::str::from_utf8(&b[i + 1..i + 3])
                .map_err(|_| error(BackupFailureCode::Unsupported))?;
            result.push(
                u8::from_str_radix(part, 16).map_err(|_| error(BackupFailureCode::Unsupported))?,
            );
            i += 3;
        } else {
            result.push(b[i]);
            i += 1
        }
    }
    String::from_utf8(result).map_err(|_| error(BackupFailureCode::Unsupported))
}
fn database_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn command(tool: &str, options: &PgConnectOptions) -> Result<Command, BackupError> {
    let m = fs::symlink_metadata(tool).map_err(|_| error(BackupFailureCode::ToolUnavailable))?;
    if !m.is_file() || m.file_type().is_symlink() || m.uid() != 0 || m.mode() & 0o022 != 0 {
        return Err(error(BackupFailureCode::ToolUnavailable));
    }
    let database = options
        .get_database()
        .ok_or_else(|| error(BackupFailureCode::Unsupported))?;
    if !database_name(database)
        || !database_name(options.get_username())
        || options.get_options().is_some()
    {
        return Err(error(BackupFailureCode::Unsupported));
    }
    let url = options.to_url_lossy();
    let mut cmd = Command::new(tool);
    cmd.env_clear()
        .env("LANG", "C")
        .env(
            "PGHOST",
            options.get_socket().map_or_else(
                || options.get_host().to_owned(),
                |p| p.to_string_lossy().into_owned(),
            ),
        )
        .env("PGPORT", options.get_port().to_string())
        .env("PGUSER", options.get_username())
        .env("PGDATABASE", database)
        .env("PGCONNECT_TIMEOUT", "5")
        .env("PGPASSFILE", "/dev/null")
        .env("PGSERVICEFILE", "/dev/null")
        .env("PGSYSCONFDIR", "/nonexistent")
        .env("PGAPPNAME", "alia-managed-backup");
    if let Some(password) = url.password() {
        cmd.env("PGPASSWORD", decode_percent(password)?);
    }
    for (k, v) in url.query_pairs() {
        let env = match k.as_ref() {
            "sslmode" => "PGSSLMODE",
            "sslrootcert" => "PGSSLROOTCERT",
            "sslcert" => "PGSSLCERT",
            "sslkey" => "PGSSLKEY",
            "statement-cache-capacity" => continue,
            _ => return Err(error(BackupFailureCode::Unsupported)),
        };
        if v.contains('\0') || v.contains('\n') || v.contains("-----BEGIN") {
            return Err(error(BackupFailureCode::Unsupported));
        }
        cmd.env(env, v.as_ref());
    }
    Ok(cmd)
}
struct ProcessOutput {
    bytes: u64,
    sha256: String,
    captured: Vec<u8>,
}
fn run_process(
    mut cmd: Command,
    input: Option<File>,
    output: Option<File>,
    limit: u64,
    timeout: Duration,
    guard: impl Fn() -> bool,
) -> Result<ProcessOutput, BackupError> {
    cmd.stdin(input.map_or_else(Stdio::null, Stdio::from))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|_| error(BackupFailureCode::ToolUnavailable))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| error(BackupFailureCode::ProcessFailed))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| error(BackupFailureCode::ProcessFailed))?;
    let failed = Arc::new(AtomicBool::new(false));
    let writer_failed = failed.clone();
    let reader = thread::spawn(move || {
        let mut stdout = stdout;
        let mut file = output;
        let mut capture = vec![];
        let mut hash = Sha256::new();
        let mut bytes = 0;
        let mut buf = [0u8; 65536];
        let result = (|| {
            loop {
                let n = stdout.read(&mut buf).map_err(io)?;
                if n == 0 {
                    break;
                }
                bytes += n as u64;
                if bytes > limit {
                    return Err(error(BackupFailureCode::TooLarge));
                }
                hash.update(&buf[..n]);
                if let Some(f) = file.as_mut() {
                    f.write_all(&buf[..n]).map_err(io)?
                } else {
                    capture.extend_from_slice(&buf[..n]);
                }
            }
            if let Some(f) = file {
                f.sync_all().map_err(io)?
            }
            Ok(ProcessOutput {
                bytes,
                sha256: format!("{:x}", hash.finalize()),
                captured: capture,
            })
        })();
        if result.is_err() {
            writer_failed.store(true, Ordering::Release);
        }
        result
    });
    let diagnostics = thread::spawn(move || {
        let mut stderr = stderr;
        let mut any = false;
        let mut buf = [0u8; 4096];
        loop {
            match stderr.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => any = true,
                Err(_) => {
                    any = true;
                    break;
                }
            }
        }
        any
    });
    let start = Instant::now();
    let mut reason = None;
    let status = loop {
        if !guard() {
            reason = Some(error(BackupFailureCode::Stale));
        } else if start.elapsed() >= timeout {
            reason = Some(error(BackupFailureCode::Timeout));
        } else if failed.load(Ordering::Acquire) {
            reason = Some(error(BackupFailureCode::ProcessFailed));
        }
        if reason.is_some() {
            let _ = child.kill();
            break child.wait().map_err(io);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(io(e));
            }
        }
    };
    let data = reader
        .join()
        .map_err(|_| error(BackupFailureCode::ProcessFailed))?;
    let had_diagnostics = diagnostics
        .join()
        .map_err(|_| error(BackupFailureCode::ProcessFailed))?;
    if let Some(reason) = reason {
        if reason.code == BackupFailureCode::ProcessFailed
            && let Err(e) = data
        {
            return Err(e);
        }
        return Err(reason);
    }
    if !status?.success() || had_diagnostics {
        return Err(error(BackupFailureCode::ProcessFailed));
    }
    data
}
fn probe(mut cmd: Command, name: &str, timeout: Duration) -> Result<(), BackupError> {
    cmd.arg("--version");
    let out = run_process(
        cmd,
        None,
        None,
        1024,
        timeout.min(Duration::from_secs(2)),
        || true,
    )?;
    let expected = format!("{name} (PostgreSQL) 17.");
    if !out.captured.starts_with(expected.as_bytes()) {
        return Err(error(BackupFailureCode::Unsupported));
    }
    Ok(())
}
async fn records(
    journal: Arc<DeletionJournal>,
    timeout: Duration,
) -> Result<Vec<super::deletion_journal::JournalIntent>, BackupError> {
    tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || journal.try_records()),
    )
    .await
    .map_err(|_| error(BackupFailureCode::Timeout))?
    .map_err(|_| error(BackupFailureCode::Journal))?
    .map_err(journal_error)
}
async fn original_log(
    path: PathBuf,
    timeout: Duration,
) -> Result<Arc<DeletionJournal>, BackupError> {
    tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || DeletionJournal::try_open_existing(&path)),
    )
    .await
    .map_err(|_| error(BackupFailureCode::Timeout))?
    .map_err(|_| error(BackupFailureCode::Journal))?
    .map(Arc::new)
    .map_err(journal_error)
}
fn checkpoint(
    records: &[super::deletion_journal::JournalIntent],
    individual: Uuid,
) -> Result<(u64, String), BackupError> {
    if records
        .iter()
        .any(|r| r.individual_id != individual || r.idempotency_key_digest.is_none())
    {
        return Err(error(BackupFailureCode::Journal));
    }
    Ok((
        records.len() as u64,
        digest(&records).map_err(|_| error(BackupFailureCode::Journal))?,
    ))
}
fn prefix(
    records: &[super::deletion_journal::JournalIntent],
    individual: Uuid,
    count: u64,
    hash: &str,
) -> Result<(), BackupError> {
    let _ = checkpoint(records, individual)?;
    let count = usize::try_from(count).map_err(|_| error(BackupFailureCode::Journal))?;
    let head = records
        .get(..count)
        .ok_or_else(|| error(BackupFailureCode::Journal))?;
    if digest(&head).map_err(|_| error(BackupFailureCode::Journal))? != hash {
        return Err(error(BackupFailureCode::Journal));
    }
    Ok(())
}
#[derive(Clone)]
struct Source {
    individual: Uuid,
    version: i64,
    epoch: i64,
    config: i64,
    migrations: String,
    extensions: BTreeMap<String, String>,
    database_settings: Value,
}
async fn schema(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<(String, BTreeMap<String, String>), BackupError> {
    sqlx::query("SET LOCAL search_path=pg_catalog,public")
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    let catalog: Value = sqlx::query_scalar(CATALOG_QUERY)
        .fetch_one(&mut **tx)
        .await
        .map_err(db)?;
    if digest(&catalog).map_err(|_| error(BackupFailureCode::Unsupported))? != CATALOG_FINGERPRINT {
        return Err(error(BackupFailureCode::Unsupported));
    }
    let columns:Value=sqlx::query_scalar("SELECT jsonb_agg(jsonb_build_array(table_name,column_name,data_type,udt_name) ORDER BY table_name,ordinal_position) FROM information_schema.columns WHERE table_schema='public' AND table_name<>'_sqlx_migrations'").fetch_one(&mut **tx).await.map_err(db)?;
    let constraints:Value=sqlx::query_scalar("SELECT jsonb_agg(jsonb_build_array(c.relname,k.conname,pg_get_constraintdef(k.oid,true)) ORDER BY c.relname,k.conname) FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relname<>'_sqlx_migrations'").fetch_one(&mut **tx).await.map_err(db)?;
    if digest(&json!({"columns":columns,"constraints":constraints}))
        .map_err(|_| error(BackupFailureCode::Unsupported))?
        != SCHEMA
    {
        return Err(error(BackupFailureCode::Unsupported));
    }
    let unknown:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_namespace WHERE nspname NOT IN ('public','information_schema') AND nspname NOT LIKE 'pg\\_%') OR EXISTS(SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public' AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_proc'::regclass AND d.objid=p.oid AND d.deptype='e')) OR EXISTS(SELECT 1 FROM pg_trigger WHERE NOT tgisinternal) OR EXISTS(SELECT 1 FROM pg_policy) OR EXISTS(SELECT 1 FROM pg_foreign_server) OR EXISTS(SELECT 1 FROM pg_largeobject_metadata)").fetch_one(&mut **tx).await.map_err(db)?;
    if unknown {
        return Err(error(BackupFailureCode::Unsupported));
    }
    let server: String = sqlx::query_scalar("SHOW server_version_num")
        .fetch_one(&mut **tx)
        .await
        .map_err(db)?;
    if server.parse::<u32>().ok().map(|n| n / 10000) != Some(17) {
        return Err(error(BackupFailureCode::Unsupported));
    }
    let actual: Vec<(i64, bool, Vec<u8>)> =
        sqlx::query_as("SELECT version,success,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut **tx)
            .await
            .map_err(db)?;
    let expected: Vec<_> = MIGRATIONS.iter().filter(|m| m.version <= 30).collect();
    if actual.len() != 30
        || expected.len() != 30
        || actual
            .iter()
            .zip(expected)
            .any(|((version, success, checksum), m)| {
                *version != m.version || !*success || checksum.as_slice() != m.checksum.as_ref()
            })
    {
        return Err(error(BackupFailureCode::Unsupported));
    }
    let extensions: Vec<(String, String)> =
        sqlx::query_as("SELECT extname,extversion FROM pg_extension ORDER BY extname")
            .fetch_all(&mut **tx)
            .await
            .map_err(db)?;
    if extensions
        .iter()
        .any(|(name, _)| !matches!(name.as_str(), "plpgsql" | "vector"))
    {
        return Err(error(BackupFailureCode::Unsupported));
    }
    Ok((
        digest(&actual).map_err(|_| error(BackupFailureCode::Unsupported))?,
        extensions.into_iter().collect(),
    ))
}
async fn source(tx: &mut Transaction<'_, Postgres>) -> Result<Source, BackupError> {
    let (migrations, extensions) = schema(tx).await?;
    let database_settings:Value=sqlx::query_scalar("SELECT jsonb_build_object('encoding',pg_encoding_to_char(encoding),'collate',datcollate,'ctype',datctype,'provider',datlocprovider::text,'locale',to_jsonb(d)->>'datlocale','icu_locale',to_jsonb(d)->>'daticulocale','icu_rules',to_jsonb(d)->>'daticurules','collversion',datcollversion) FROM pg_database d WHERE datname=current_database()").fetch_one(&mut **tx).await.map_err(db)?;
    let (individual,version,epoch,config,runtime):(Uuid,i64,i64,i64,Value)=sqlx::query_as("SELECT id,state_version,privacy_epoch,config_version,runtime_config FROM individuals WHERE singleton").fetch_one(&mut **tx).await.map_err(db)?;
    crate::domain::config::RuntimeRules::validate(
        runtime,
        crate::runtime::capability::IMPLEMENTED_CAPABILITIES,
    )
    .map_err(|_| error(BackupFailureCode::Unsupported))?;
    verify_development_projection_in(tx, individual)
        .await
        .map_err(|_| error(BackupFailureCode::Invalid))?;
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM privacy_operations WHERE individual_id=$1 AND status<>'completed')").bind(individual).fetch_one(&mut **tx).await.map_err(db)?;
    if pending {
        return Err(error(BackupFailureCode::Incomplete));
    }
    Ok(Source {
        individual,
        version,
        epoch,
        config,
        migrations,
        extensions,
        database_settings,
    })
}
async fn journal_matches(
    tx: &mut Transaction<'_, Postgres>,
    s: &Source,
    records: &[super::deletion_journal::JournalIntent],
) -> Result<(), BackupError> {
    let _ = checkpoint(records, s.individual)?;
    let jobs:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('operation_id',id,'individual_id',individual_id,'target_memory_id',target_id,'expected_memory_version',expected_memory_version,'request_digest',request_digest,'root_event_ids',root_event_ids,'idempotency_key_digest',idempotency_key_digest) FROM privacy_operations WHERE individual_id=$1 ORDER BY journal_sequence").bind(s.individual).fetch_all(&mut **tx).await.map_err(db)?;
    let expected = serde_json::to_value(records).map_err(|_| error(BackupFailureCode::Journal))?;
    if Value::Array(jobs) != expected {
        return Err(error(BackupFailureCode::Journal));
    }
    Ok(())
}
impl ManagedBackupManager {
    pub fn open(root: &Path, limits: BackupLimits) -> Result<Self, BackupError> {
        let limits = limits.validate()?;
        if !root.is_absolute()
            || root
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(error(BackupFailureCode::UnsafePath));
        }
        for p in root.ancestors() {
            let m = fs::symlink_metadata(p).map_err(io)?;
            if !m.is_dir()
                || m.file_type().is_symlink()
                || (m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0)
            {
                return Err(error(BackupFailureCode::UnsafePath));
            }
        }
        let root = open_dir(root)?;
        for e in fs::read_dir(fd_path(&root, ".")).map_err(io)? {
            let name = e.map_err(io)?.file_name();
            if !matches!(
                name.to_str(),
                Some("lease.lock" | "authority.json" | "artifacts" | "recoveries" | "catalog")
            ) {
                return Err(error(BackupFailureCode::UnsafePath));
            }
        }
        let artifacts = subdir(&root, "artifacts", true)?;
        let recoveries = subdir(&root, "recoveries", true)?;
        let catalog = subdir(&root, "catalog", true)?;
        Ok(Self {
            root: Arc::new(root),
            artifacts: Arc::new(artifacts),
            recoveries: Arc::new(recoveries),
            catalog: Arc::new(catalog),
            limits,
        })
    }
    fn authority(&self) -> Result<Authority, BackupError> {
        let a: Authority = read_meta(&self.root, "authority.json")?;
        if a.format != "alia-backup-authority/1"
            || a.individual_id.is_nil()
            || a.privacy_epoch < 0
            || !hex(&a.journal_digest)
        {
            return Err(error(BackupFailureCode::Invalid));
        }
        Ok(a)
    }
    fn update_authority(&self, a: Authority) -> Result<(), BackupError> {
        let _lock = lock(&self.root)?;
        if fs::symlink_metadata(fd_path(&self.root, "authority.json")).is_ok() {
            let old = self.authority()?;
            if old.individual_id != a.individual_id
                || old.privacy_epoch > a.privacy_epoch
                || old.journal_records > a.journal_records
                || (old.journal_records == a.journal_records
                    && old.journal_digest != a.journal_digest)
            {
                return Err(error(BackupFailureCode::Stale));
            }
        }
        replace_meta(&self.root, "authority.json", &a)
    }
    fn floor_allows(&self, id: Uuid, epoch: i64) -> bool {
        self.authority()
            .is_ok_and(|a| a.individual_id == id && a.privacy_epoch == epoch)
    }
    async fn fresh(
        &self,
        store: &Store,
        s: &Source,
        generation: u64,
    ) -> Result<Transaction<'_, Postgres>, BackupError> {
        store
            .require_privacy_snapshot(generation)
            .map_err(|_| error(BackupFailureCode::Stale))?;
        let mut tx = store.pool.begin().await.map_err(db)?;
        sqlx::query("SELECT pg_advisory_xact_lock_shared($1)")
            .bind(WRITE_GATE)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let current = source(&mut tx).await?;
        if current.individual != s.individual
            || current.epoch != s.epoch
            || current.migrations != s.migrations
            || current.extensions != s.extensions
            || current.database_settings != s.database_settings
            || !self.floor_allows(s.individual, s.epoch)
        {
            return Err(error(BackupFailureCode::Stale));
        }
        store
            .require_privacy_snapshot(generation)
            .map_err(|_| error(BackupFailureCode::Stale))?;
        Ok(tx)
    }
    /// Explicit first-enable provisioning against a currently verified live DB
    /// and original log. Restore never creates/replaces missing authority.
    pub async fn initialize_authority(
        &self,
        store: &Store,
        journal: Arc<DeletionJournal>,
    ) -> Result<(), BackupError> {
        tokio::time::timeout(self.limits.timeout, async {
            let generation = store.privacy_generation();
            store
                .require_privacy_snapshot(generation)
                .map_err(|_| error(BackupFailureCode::Stale))?;
            let log = records(journal, self.limits.timeout).await?;
            let mut tx = store.pool.begin().await.map_err(db)?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            let s = source(&mut tx).await?;
            journal_matches(&mut tx, &s, &log).await?;
            let (count, hash) = checkpoint(&log, s.individual)?;
            self.update_authority(Authority {
                format: "alia-backup-authority/1".into(),
                individual_id: s.individual,
                privacy_epoch: s.epoch,
                journal_records: count,
                journal_digest: hash,
            })?;
            store
                .require_privacy_snapshot(generation)
                .map_err(|_| error(BackupFailureCode::Stale))?;
            tx.commit().await.map_err(db)?;
            let checked = self.fresh(store, &s, generation).await?;
            checked.commit().await.map_err(db)?;
            Ok(())
        })
        .await
        .map_err(|_| error(BackupFailureCode::Timeout))?
    }
    pub async fn create(
        &self,
        store: &Store,
        journal: Arc<DeletionJournal>,
    ) -> Result<BackupManifest, BackupError> {
        let id = Uuid::new_v4();
        let result =
            tokio::time::timeout(self.limits.timeout, self.create_inner(id, store, journal))
                .await
                .unwrap_or_else(|_| Err(error(BackupFailureCode::Timeout)));
        match result {
            Ok(v) => Ok(v),
            Err(mut e) => {
                e.operation_id = Some(id);
                e.cleanup_complete = self.cleanup_failed_backup(id).is_ok();
                Err(e)
            }
        }
    }
    async fn create_inner(
        &self,
        id: Uuid,
        store: &Store,
        journal: Arc<DeletionJournal>,
    ) -> Result<BackupManifest, BackupError> {
        let generation = store.privacy_generation();
        store
            .require_privacy_snapshot(generation)
            .map_err(|_| error(BackupFailureCode::Stale))?;
        let log = records(journal, self.limits.timeout).await?;
        let mut tx = store.pool.begin().await.map_err(db)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let s = source(&mut tx).await?;
        journal_matches(&mut tx, &s, &log).await?;
        let (count, journal_digest) = checkpoint(&log, s.individual)?;
        self.update_authority(Authority {
            format: "alia-backup-authority/1".into(),
            individual_id: s.individual,
            privacy_epoch: s.epoch,
            journal_records: count,
            journal_digest: journal_digest.clone(),
        })?;
        let snapshot: String = sqlx::query_scalar("SELECT pg_export_snapshot()")
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
        if snapshot.is_empty()
            || snapshot.len() > 128
            || !snapshot.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-')
        {
            return Err(error(BackupFailureCode::Invalid));
        }
        let (dir, lease) = {
            let _registry = lock(&self.root)?;
            if !self.floor_allows(s.individual, s.epoch) {
                return Err(error(BackupFailureCode::Stale));
            }
            if child_dirs(&self.artifacts)?.len() >= 1024 {
                return Err(error(BackupFailureCode::TooLarge));
            }
            let name = id.to_string();
            fs::DirBuilder::new()
                .mode(0o700)
                .create(fd_path(&self.artifacts, &name))
                .map_err(io)?;
            self.artifacts.sync_all().map_err(io)?;
            let dir = subdir(&self.artifacts, &name, false)?;
            let lease = new_lease(&dir)?;
            write_new(
                &dir,
                "pending.json",
                &Pending {
                    format: "alia-backup-pending/1".into(),
                    backup_id: id,
                    individual_id: s.individual,
                    privacy_epoch: s.epoch,
                },
            )?;
            (dir, lease)
        };
        let file = open_file(&dir, "archive.part", true)?;
        let options = store.pool.connect_options();
        let probe_cmd = command(DUMP, &options)?;
        let mut cmd = command(DUMP, &options)?;
        cmd.args([
            "--format=custom",
            "--compress=none",
            "--no-owner",
            "--no-privileges",
            "--no-tablespaces",
            "--no-password",
            "--snapshot",
            &snapshot,
        ]);
        let manager = self.clone();
        let observed_store = store.clone();
        let worker_lease = lease.try_clone().map_err(io)?;
        let individual = s.individual;
        let epoch = s.epoch;
        let limits = self.limits;
        let output = tokio::task::spawn_blocking(move || {
            let _lease = worker_lease;
            probe(probe_cmd, "pg_dump", limits.timeout)?;
            run_process(
                cmd,
                None,
                Some(file),
                limits.max_archive_bytes,
                limits.timeout,
                || {
                    observed_store.privacy_ready()
                        && observed_store.privacy_generation() == generation
                        && manager.floor_allows(individual, epoch)
                },
            )
        })
        .await
        .map_err(|_| error(BackupFailureCode::ProcessFailed))??;
        tx.commit().await.map_err(db)?;
        if output.bytes == 0 {
            return Err(error(BackupFailureCode::ProcessFailed));
        }
        let manifest = BackupManifest {
            format: FORMAT.into(),
            application_version: env!("CARGO_PKG_VERSION").into(),
            backup_id: id,
            individual_id: s.individual,
            state_version: s.version,
            privacy_epoch: s.epoch,
            config_version: s.config,
            created_at: Utc::now(),
            schema_version: 30,
            schema_fingerprint: SCHEMA.into(),
            catalog_fingerprint: CATALOG_FINGERPRINT.into(),
            archive_profile: "pg17-custom-uncompressed".into(),
            database_settings: s.database_settings.clone(),
            migration_fingerprint: s.migrations.clone(),
            postgres_major: 17,
            archive_bytes: output.bytes,
            archive_sha256: output.sha256,
            journal_records: count,
            journal_digest,
            extensions: s.extensions.clone(),
        };
        let final_tx = self.fresh(store, &s, generation).await?;
        let _root_lock = lock(&self.root)?;
        if !self.floor_allows(s.individual, s.epoch) {
            return Err(error(BackupFailureCode::Stale));
        }
        fs::hard_link(
            fd_path(&dir, "archive.part"),
            fd_path(&dir, "archive.pgcustom"),
        )
        .map_err(io)?;
        fs::remove_file(fd_path(&dir, "archive.part")).map_err(io)?;
        dir.sync_all().map_err(io)?;
        write_new(&dir, "manifest.json", &manifest)?;
        write_new(
            &self.catalog,
            &format!("{id}.json"),
            &CatalogEntry {
                format: "alia-backup-catalog/1".into(),
                backup_id: id,
                individual_id: s.individual,
                privacy_epoch: s.epoch,
                manifest_digest: digest(&manifest)
                    .map_err(|_| error(BackupFailureCode::Invalid))?,
            },
        )?;
        store
            .require_privacy_snapshot(generation)
            .map_err(|_| error(BackupFailureCode::Stale))?;
        final_tx.commit().await.map_err(db)?;
        Ok(manifest)
    }
    fn cleanup_failed_backup(&self, id: Uuid) -> Result<(), BackupError> {
        if fs::symlink_metadata(fd_path(&self.artifacts, &id.to_string())).is_err() {
            return Ok(());
        }
        let dir = subdir(&self.artifacts, &id.to_string(), false)?;
        let lease = lock_existing(&dir)?;
        self.remove_catalog(id)?;
        let result = erase_artifact(&self.artifacts, id);
        drop(lease);
        result
    }
    fn authenticated_manifest(&self, dir: &File, id: Uuid) -> Result<BackupManifest, BackupError> {
        let m: BackupManifest = read_meta(dir, "manifest.json")?;
        check_manifest(&m, id, self.limits)?;
        let c: CatalogEntry = read_meta(&self.catalog, &format!("{id}.json"))?;
        if c.format != "alia-backup-catalog/1"
            || c.backup_id != id
            || c.individual_id != m.individual_id
            || c.privacy_epoch != m.privacy_epoch
            || c.manifest_digest != digest(&m).map_err(|_| error(BackupFailureCode::Invalid))?
        {
            return Err(error(BackupFailureCode::Invalid));
        }
        Ok(m)
    }
    fn remove_catalog(&self, id: Uuid) -> Result<(), BackupError> {
        let name = format!("{id}.json");
        if fs::symlink_metadata(fd_path(&self.catalog, &name)).is_ok() {
            let _ = open_file(&self.catalog, &name, false)?;
            fs::remove_file(fd_path(&self.catalog, &name)).map_err(io)?;
            self.catalog.sync_all().map_err(io)?;
        }
        Ok(())
    }
    fn archive(&self, id: Uuid) -> Result<(BackupManifest, File, File), BackupError> {
        if id.is_nil() {
            return Err(error(BackupFailureCode::Invalid));
        }
        let dir = subdir(&self.artifacts, &id.to_string(), false)?;
        let lease = lock_existing(&dir)?;
        let m = self.authenticated_manifest(&dir, id)?;
        let mut file = open_file(&dir, "archive.pgcustom", false)?;
        let (bytes, hash) = hash_file(&mut file, self.limits.max_archive_bytes)?;
        if bytes != m.archive_bytes || hash != m.archive_sha256 {
            return Err(error(BackupFailureCode::Invalid));
        }
        Ok((m, file, lease))
    }
    pub async fn verify_current(
        &self,
        store: &Store,
        id: Uuid,
    ) -> Result<BackupManifest, BackupError> {
        tokio::time::timeout(self.limits.timeout, self.verify_current_inner(store, id))
            .await
            .map_err(|_| error(BackupFailureCode::Timeout))?
    }
    async fn verify_current_inner(
        &self,
        store: &Store,
        id: Uuid,
    ) -> Result<BackupManifest, BackupError> {
        let generation = store.privacy_generation();
        store
            .require_privacy_snapshot(generation)
            .map_err(|_| error(BackupFailureCode::Stale))?;
        let (m, _file, _lease) = self.archive(id)?;
        let s = Source {
            individual: m.individual_id,
            version: m.state_version,
            epoch: m.privacy_epoch,
            config: m.config_version,
            migrations: m.migration_fingerprint.clone(),
            extensions: m.extensions.clone(),
            database_settings: m.database_settings.clone(),
        };
        let tx = self.fresh(store, &s, generation).await?;
        tx.commit().await.map_err(db)?;
        Ok(m)
    }
    pub async fn invalidate_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        individual: Uuid,
        privacy_epoch: i64,
        journal: Arc<DeletionJournal>,
    ) -> Result<(), BackupError> {
        tokio::time::timeout(
            self.limits.timeout,
            self.invalidate_inner(tx, individual, privacy_epoch, journal),
        )
        .await
        .map_err(|_| error(BackupFailureCode::Timeout))?
    }
    async fn invalidate_inner(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        individual: Uuid,
        privacy_epoch: i64,
        journal: Arc<DeletionJournal>,
    ) -> Result<(), BackupError> {
        let (id, epoch): (Uuid, i64) =
            sqlx::query_as("SELECT id,privacy_epoch FROM individuals WHERE singleton")
                .fetch_one(&mut **tx)
                .await
                .map_err(db)?;
        if id != individual || epoch != privacy_epoch || epoch <= 0 {
            return Err(error(BackupFailureCode::Invalid));
        }
        let log = records(journal, self.limits.timeout).await?;
        let (count, journal_digest) = checkpoint(&log, individual)?;
        let s = Source {
            individual,
            epoch,
            version: 1,
            config: 1,
            migrations: String::new(),
            extensions: BTreeMap::new(),
            database_settings: Value::Null,
        };
        journal_matches(tx, &s, &log).await?;
        self.update_authority(Authority {
            format: "alia-backup-authority/1".into(),
            individual_id: individual,
            privacy_epoch: epoch,
            journal_records: count,
            journal_digest,
        })
    }
    pub fn purge_invalidated(
        &self,
        individual: Uuid,
        privacy_epoch: i64,
    ) -> Result<BackupCleanup, BackupError> {
        let a = self.authority()?;
        if a.individual_id != individual || a.privacy_epoch != privacy_epoch {
            return Err(error(BackupFailureCode::Stale));
        }
        let mut result = BackupCleanup {
            removed: 0,
            remaining: 0,
        };
        for id in child_dirs(&self.artifacts)? {
            let dir = subdir(&self.artifacts, &id.to_string(), false)?;
            let lease = match lock_existing(&dir) {
                Ok(f) => f,
                Err(e) if e.code == BackupFailureCode::Busy => {
                    result.remaining += 1;
                    continue;
                }
                Err(e) => return Err(e),
            };
            let keep = self.authenticated_manifest(&dir, id).is_ok_and(|m| {
                if m.individual_id != individual || m.privacy_epoch != privacy_epoch {
                    return false;
                }
                open_file(&dir, "archive.pgcustom", false)
                    .and_then(|mut f| hash_file(&mut f, self.limits.max_archive_bytes))
                    .is_ok_and(|(n, h)| n == m.archive_bytes && h == m.archive_sha256)
            });
            if !keep {
                self.remove_catalog(id)?;
                erase_artifact(&self.artifacts, id)?;
                result.removed += 1;
            }
            drop(lease);
        }
        Ok(result)
    }
    pub fn invalidated_clean(
        &self,
        individual: Uuid,
        privacy_epoch: i64,
    ) -> Result<bool, BackupError> {
        let a = self.authority()?;
        if a.individual_id != individual || a.privacy_epoch != privacy_epoch {
            return Err(error(BackupFailureCode::Stale));
        }
        for id in child_dirs(&self.artifacts)? {
            let dir = subdir(&self.artifacts, &id.to_string(), false)?;
            let _lease = lock_existing(&dir)?;
            let m = match self.authenticated_manifest(&dir, id) {
                Ok(m) => m,
                Err(_) => return Ok(false),
            };
            if check_manifest(&m, id, self.limits).is_err()
                || m.individual_id != individual
                || m.privacy_epoch != privacy_epoch
            {
                return Ok(false);
            }
            let mut file = open_file(&dir, "archive.pgcustom", false)?;
            let (n, h) = hash_file(&mut file, self.limits.max_archive_bytes)?;
            if n != m.archive_bytes || h != m.archive_sha256 {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
fn recovery_name(id: Uuid) -> String {
    format!("alia_restore_{}", id.simple())
}
impl ManagedBackupManager {
    pub async fn restore_current_isolated(
        &self,
        source_store: &Store,
        id: Uuid,
        admin: &PgPool,
        journal_path: &Path,
    ) -> Result<RestoreReceipt, BackupError> {
        let _ = self.verify_current(source_store, id).await?;
        self.restore_attempt(id, admin, journal_path, false).await
    }
    pub async fn recover_quarantined(
        &self,
        id: Uuid,
        admin: &PgPool,
        journal_path: &Path,
    ) -> Result<RestoreReceipt, BackupError> {
        self.restore_attempt(id, admin, journal_path, true).await
    }
    async fn restore_attempt(
        &self,
        id: Uuid,
        admin: &PgPool,
        journal_path: &Path,
        allow_stale: bool,
    ) -> Result<RestoreReceipt, BackupError> {
        let recovery_id = Uuid::new_v4();
        let result = tokio::time::timeout(
            self.limits.timeout,
            self.restore_inner(recovery_id, id, admin, journal_path, allow_stale),
        )
        .await
        .unwrap_or_else(|_| Err(error(BackupFailureCode::Timeout)));
        match result {
            Ok(r) => Ok(r),
            Err(mut e) => {
                e.operation_id = Some(recovery_id);
                e.cleanup_complete = self.cleanup_recovery(admin, recovery_id).await.is_ok();
                Err(e)
            }
        }
    }
    async fn restore_inner(
        &self,
        recovery_id: Uuid,
        id: Uuid,
        admin: &PgPool,
        journal_path: &Path,
        allow_stale: bool,
    ) -> Result<RestoreReceipt, BackupError> {
        let authority = self.authority()?;
        let (m, file, archive_lease) = self.archive(id)?;
        if m.individual_id != authority.individual_id
            || m.privacy_epoch > authority.privacy_epoch
            || (!allow_stale && m.privacy_epoch != authority.privacy_epoch)
        {
            return Err(error(BackupFailureCode::Stale));
        }
        if !journal_path.is_absolute() {
            return Err(error(BackupFailureCode::UnsafePath));
        }
        let vault = self.root.metadata().map_err(io)?;
        for parent in journal_path.ancestors() {
            if let Ok(meta) = fs::symlink_metadata(parent)
                && (meta.dev(), meta.ino()) == (vault.dev(), vault.ino())
            {
                return Err(error(BackupFailureCode::UnsafePath));
            }
        }
        let journal = original_log(journal_path.to_owned(), self.limits.timeout).await?;
        let before_log = records(journal.clone(), self.limits.timeout).await?;
        prefix(
            &before_log,
            m.individual_id,
            m.journal_records,
            &m.journal_digest,
        )?;
        prefix(
            &before_log,
            m.individual_id,
            authority.journal_records,
            &authority.journal_digest,
        )?;
        let settings:Value=sqlx::query_scalar("SELECT jsonb_build_object('encoding',pg_encoding_to_char(encoding),'collate',datcollate,'ctype',datctype,'provider',datlocprovider::text,'locale',to_jsonb(d)->>'datlocale','icu_locale',to_jsonb(d)->>'daticulocale','icu_rules',to_jsonb(d)->>'daticurules','collversion',datcollversion) FROM pg_database d WHERE datname='template0'").fetch_one(admin).await.map_err(db)?;
        let mut target_locale = settings.clone();
        let mut source_locale = m.database_settings.clone();
        target_locale
            .as_object_mut()
            .ok_or_else(|| error(BackupFailureCode::Invalid))?
            .remove("collversion");
        source_locale
            .as_object_mut()
            .ok_or_else(|| error(BackupFailureCode::Invalid))?
            .remove("collversion");
        if target_locale != source_locale {
            return Err(error(BackupFailureCode::Unsupported));
        }
        let (dir, lease) = {
            let _registry = lock(&self.root)?;
            if child_dirs(&self.recoveries)?.len() >= 16 {
                return Err(error(BackupFailureCode::TooLarge));
            }
            let recovery_dir = recovery_id.to_string();
            fs::DirBuilder::new()
                .mode(0o700)
                .create(fd_path(&self.recoveries, &recovery_dir))
                .map_err(io)?;
            self.recoveries.sync_all().map_err(io)?;
            let dir = subdir(&self.recoveries, &recovery_dir, false)?;
            let lease = new_lease(&dir)?;
            write_new(
                &dir,
                "receipt.json",
                &RestoreReceipt {
                    recovery_id,
                    backup_id: id,
                    individual_id: m.individual_id,
                    state_version: m.state_version,
                    privacy_epoch: m.privacy_epoch,
                    status: "creating_fenced".into(),
                },
            )?;
            (dir, lease)
        };
        let name = recovery_name(recovery_id);
        if let Err(e) = sqlx::query(&format!("CREATE DATABASE \"{name}\" TEMPLATE template0"))
            .execute(admin)
            .await
        {
            // Explicit permission denial is known to precede creation. Other
            // server/transport failures remain uncertain: an absent row alone
            // cannot erase evidence of a possible late creation.
            if matches!(&e, sqlx::Error::Database(e) if e.code().as_deref() == Some("42501")) {
                let mut rejected: RestoreReceipt = read_meta(&dir, "receipt.json")?;
                rejected.status = "create_rejected_fenced".into();
                replace_meta(&dir, "receipt.json", &rejected)?;
            }
            return Err(db(e));
        }
        sqlx::query(&format!(
            "COMMENT ON DATABASE \"{name}\" IS 'alia-managed-recovery/1:{recovery_id}'"
        ))
        .execute(admin)
        .await
        .map_err(db)?;
        let options = admin.connect_options().as_ref().clone().database(&name);
        let probe_cmd = command(RESTORE, &options)?;
        let mut cmd = command(RESTORE, &options)?;
        cmd.args([
            "--format=custom",
            "--single-transaction",
            "--exit-on-error",
            "--no-owner",
            "--no-privileges",
            "--no-tablespaces",
            "--no-publications",
            "--no-subscriptions",
            "--no-password",
            "--dbname",
            &name,
        ]);
        let worker_lease = lease.try_clone().map_err(io)?;
        let worker_archive_lease = archive_lease.try_clone().map_err(io)?;
        let limits = self.limits;
        tokio::task::spawn_blocking(move || {
            let _leases = (worker_lease, worker_archive_lease);
            probe(probe_cmd, "pg_restore", limits.timeout)?;
            run_process(cmd, Some(file), None, 4096, limits.timeout, || true)
        })
        .await
        .map_err(|_| error(BackupFailureCode::ProcessFailed))??;
        let target = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await
            .map_err(db)?;
        let restore_result = self.verify_restored(&target, &m, &authority, journal).await;
        target.close().await;
        let receipt = restore_result?;
        replace_meta(
            &dir,
            "receipt.json",
            &RestoreReceipt {
                recovery_id,
                backup_id: id,
                individual_id: receipt.individual,
                state_version: receipt.version,
                privacy_epoch: receipt.epoch,
                status: "verified_fenced".into(),
            },
        )?;
        read_meta(&dir, "receipt.json")
    }
    async fn verify_restored(
        &self,
        target: &PgPool,
        m: &BackupManifest,
        authority: &Authority,
        journal: Arc<DeletionJournal>,
    ) -> Result<Source, BackupError> {
        let store = Store::new(target.clone());
        store.fence_privacy();
        let mut tx = target.begin().await.map_err(db)?;
        let restored = source(&mut tx).await?;
        if restored.individual != m.individual_id
            || restored.version != m.state_version
            || restored.epoch != m.privacy_epoch
            || restored.config != m.config_version
            || restored.migrations != m.migration_fingerprint
            || restored.extensions != m.extensions
            || restored.database_settings != m.database_settings
        {
            return Err(error(BackupFailureCode::Invalid));
        }
        tx.commit().await.map_err(db)?;
        store
            .try_replay_deletion_journal(journal.clone())
            .await
            .map_err(|_| error(BackupFailureCode::Journal))?;
        store.fence_privacy();
        let log = records(journal, self.limits.timeout).await?;
        prefix(
            &log,
            m.individual_id,
            authority.journal_records,
            &authority.journal_digest,
        )?;
        let mut tx = target.begin().await.map_err(db)?;
        let final_state = source(&mut tx).await?;
        journal_matches(&mut tx, &final_state, &log).await?;
        if final_state.individual != m.individual_id || final_state.epoch < authority.privacy_epoch
        {
            return Err(error(BackupFailureCode::Stale));
        }
        let current_authority = self.authority()?;
        if current_authority != *authority {
            return Err(error(BackupFailureCode::Stale));
        }
        tx.commit().await.map_err(db)?;
        Ok(final_state)
    }
    pub async fn cleanup_recovery(&self, admin: &PgPool, id: Uuid) -> Result<(), BackupError> {
        tokio::time::timeout(self.limits.timeout, self.cleanup_recovery_inner(admin, id))
            .await
            .map_err(|_| BackupError {
                code: BackupFailureCode::Timeout,
                operation_id: Some(id),
                cleanup_complete: false,
            })?
    }
    async fn cleanup_recovery_inner(&self, admin: &PgPool, id: Uuid) -> Result<(), BackupError> {
        if id.is_nil() {
            return Err(error(BackupFailureCode::Invalid));
        }
        let path = fd_path(&self.recoveries, &id.to_string());
        if fs::symlink_metadata(&path).is_err() {
            return Ok(());
        }
        let dir = subdir(&self.recoveries, &id.to_string(), false)?;
        let lease = lock_existing(&dir)?;
        let receipt: RestoreReceipt = read_meta(&dir, "receipt.json")?;
        if receipt.recovery_id != id
            || !matches!(
                receipt.status.as_str(),
                "creating_fenced" | "create_rejected_fenced" | "verified_fenced"
            )
        {
            return Err(error(BackupFailureCode::UnsafePath));
        }
        let name = recovery_name(id);
        let comment: Option<Option<String>> = sqlx::query_scalar(
            "SELECT shobj_description(oid,'pg_database') FROM pg_database WHERE datname=$1",
        )
        .bind(&name)
        .fetch_optional(admin)
        .await
        .map_err(db)?;
        if let Some(comment) = comment {
            if comment.as_deref() != Some(&format!("alia-managed-recovery/1:{id}")) {
                return Err(error(BackupFailureCode::UnsafePath));
            }
            sqlx::query(&format!("DROP DATABASE \"{name}\" WITH (FORCE)"))
                .execute(admin)
                .await
                .map_err(db)?;
        } else if receipt.status == "creating_fenced" {
            return Err(BackupError {
                code: BackupFailureCode::Incomplete,
                operation_id: Some(id),
                cleanup_complete: false,
            });
        }
        for e in fs::read_dir(fd_path(&dir, ".")).map_err(io)? {
            let e = e.map_err(io)?;
            let n = e
                .file_name()
                .into_string()
                .map_err(|_| error(BackupFailureCode::UnsafePath))?;
            if !matches!(n.as_str(), "lease.lock" | "receipt.json") {
                return Err(error(BackupFailureCode::UnsafePath));
            }
            let _ = open_file(&dir, &n, false)?;
            fs::remove_file(fd_path(&dir, &n)).map_err(io)?;
        }
        fs::remove_dir(path).map_err(io)?;
        self.recoveries.sync_all().map_err(io)?;
        drop(lease);
        Ok(())
    }
}

fn journal_error(e: super::deletion_journal::JournalError) -> BackupError {
    if e == super::deletion_journal::JournalError::Lock {
        error(BackupFailureCode::Busy)
    } else {
        error(BackupFailureCode::Journal)
    }
}
