//! One-shot, unregistered Python sandbox. Call from a separately admitted blocking task.
//! No caller chooses flags, environment, mounts, executable or network policy.
use crate::domain::config::parse_strict_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::{Read, Write},
    net::Shutdown,
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt},
        net::UnixStream,
        process::CommandExt,
    },
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;

const CAP: usize = 65536;
const MEMORY: u64 = 128 * 1024 * 1024;
const OWNER: &str = "io.alia.sandbox.execution";
const TMPFS: &str = "rw,nosuid,nodev,noexec,size=16777216,mode=1777";
const CLEANUP_MS: u64 = 1000;

#[derive(Clone, Copy, Debug)]
pub enum DockerCommand {
    Native,
    SudoNoPassword,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxRequest {
    pub source: String,
    pub timeout_ms: u64,
}
/// Separate bounded data protocol. User content never enters -c argv.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSandboxRequest {
    pub source: String,
    pub input_json: String,
    pub timeout_ms: u64,
}
impl std::fmt::Debug for SkillSandboxRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillSandboxRequest")
            .field("source_bytes", &self.source.len())
            .field("input_bytes", &self.input_json.len())
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
}
pub const SKILL_PROTOCOL: &str = "python-skill/v1";
const SKILL_CARRIER_LIMIT: usize = 160 * 1024;
const SKILL_MARKER: &str = "ALIA_SANDBOX_SKILL_VERIFIED_V1\n";
#[derive(Debug, Serialize)]
pub struct SkillSandboxOutcome {
    pub outcome: SandboxOutcome,
    pub protocol: &'static str,
    pub source_sha256: String,
    pub input_sha256: String,
    pub wrapper_sha256: String,
}
enum ExecutionInput<'a> {
    Raw(&'a str),
    Skill {
        carrier: Vec<u8>,
        source_sha256: String,
        input_sha256: String,
    },
    Invalid,
}
impl ExecutionInput<'_> {
    fn labels_match(&self, value: &Value) -> bool {
        match self {
            Self::Skill {
                source_sha256,
                input_sha256,
                ..
            } => {
                value["Config"]["Labels"]["io.alia.sandbox.source"] == *source_sha256
                    && value["Config"]["Labels"]["io.alia.sandbox.input"] == *input_sha256
            }
            _ => true,
        }
    }
}

fn skill_carrier(request: &SkillSandboxRequest) -> Option<Vec<u8>> {
    if request.source.is_empty()
        || request.source.len() > 24576
        || request.input_json.len() > 8192
        || parse_strict_json(request.input_json.as_bytes(), 8192).is_err()
    {
        return None;
    }
    let source = serde_json::to_string(&request.source).ok()?;
    // input is already one complete strict JSON value; keep its exact bounded bytes.
    let bytes = format!("{{\"source\":{source},\"input\":{}}}", request.input_json).into_bytes();
    (bytes.len() <= SKILL_CARRIER_LIMIT).then_some(bytes)
}

impl std::fmt::Debug for SandboxRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SandboxRequest")
            .field("source_bytes", &self.source.len())
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    Succeeded,
    CodeFailed,
    TimedOut,
    ResourceExceeded,
    OutputLimit,
    Unavailable,
    InfrastructureFailed,
}

#[derive(Debug, Serialize)]
pub struct SandboxOutcome {
    /// Exact unique container name, including on ambiguous create/cleanup failure.
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
}

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("invalid official Python digest reference")]
    InvalidImage,
    #[error("sandbox launcher unavailable")]
    Unavailable,
}

pub struct SandboxExecutor {
    image: String,
    docker: DockerCommand,
}

struct Client {
    path: std::path::PathBuf,
    hard_deadline: Instant,
    launchers_clean: std::cell::Cell<bool>,
}
impl Client {
    fn new(hard_deadline: Instant) -> std::io::Result<Self> {
        let path = std::path::Path::new("/tmp")
            .join(format!("alia-sandbox-client-{}", Uuid::new_v4().simple()));
        DirBuilder::new().mode(0o700).create(&path)?;
        let client = Self {
            path,
            hard_deadline,
            launchers_clean: std::cell::Cell::new(true),
        };
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(client.path.join("config.json"))?
            .write_all(b"{}")?;
        Ok(client)
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Capture {
    code: Option<i32>,
    out: Vec<u8>,
    err: Vec<u8>,
    timeout: bool,
    overflow: bool,
    reaped: bool,
}
impl Capture {
    fn ok(&self) -> bool {
        self.code == Some(0) && !self.timeout && !self.overflow && self.reaped
    }
    fn json(&self) -> Option<Value> {
        self.ok()
            .then(|| serde_json::from_slice(&self.out).ok())
            .flatten()
    }
}

fn terminate(command: DockerCommand, child: &mut Child, deadline: Instant) -> bool {
    // Safe std process groups + a fixed system kill executable; never a shell.
    let mut kill = match command {
        DockerCommand::Native => Command::new("/usr/bin/kill"),
        DockerCommand::SudoNoPassword => {
            let mut c = Command::new("/usr/bin/sudo");
            c.args(["-n", "/usr/bin/kill"]);
            c
        }
    };
    kill.env_clear()
        .args(["-KILL", "--", &format!("-{}", child.id())])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut helper_clean = false;
    if let Ok(mut killer) = kill.spawn() {
        while Instant::now() < deadline {
            if matches!(killer.try_wait(), Ok(Some(_))) {
                helper_clean = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if !helper_clean {
            let _ = killer.kill();
            helper_clean = matches!(killer.try_wait(), Ok(Some(_)));
            if !helper_clean {
                // A broken privileged launcher must not turn teardown into an
                // unbounded wait. Record incomplete cleanup via the return value.
                std::thread::spawn(move || {
                    let _ = killer.wait();
                });
            }
        }
    }
    let _ = child.kill();
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return helper_clean;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    helper_clean && matches!(child.try_wait(), Ok(Some(_)))
}

fn read_bounded(
    stream: &mut UnixStream,
    bytes: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<(bool, bool)> {
    let mut buffer = [0u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return Ok((true, false)),
            Ok(n) => {
                let space = cap.saturating_sub(bytes.len());
                bytes.extend_from_slice(&buffer[..space.min(n)]);
                if n > space {
                    return Ok((false, true));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok((false, false)),
            Err(e) => return Err(e),
        }
    }
}

impl SandboxExecutor {
    pub fn new(image_digest: &str, command: DockerCommand) -> Result<Self, SandboxError> {
        let hash = image_digest
            .strip_prefix("python@sha256:")
            .ok_or(SandboxError::InvalidImage)?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(SandboxError::InvalidImage);
        }
        if !std::path::Path::new("/usr/bin/docker").is_file()
            || (matches!(command, DockerCommand::SudoNoPassword)
                && !std::path::Path::new("/usr/bin/sudo").is_file())
        {
            return Err(SandboxError::Unavailable);
        }
        Ok(Self {
            image: image_digest.into(),
            docker: command,
        })
    }

    fn cli(
        &self,
        client: &Client,
        args: &[String],
        input: &[u8],
        deadline: Instant,
        out_cap: usize,
    ) -> Capture {
        let failed = || Capture {
            code: None,
            out: vec![],
            err: vec![],
            timeout: Instant::now() >= deadline,
            overflow: false,
            reaped: true,
        };
        if Instant::now() >= deadline {
            return failed();
        }
        let Ok((mut input_socket, child_input)) = UnixStream::pair() else {
            return failed();
        };
        let Ok((mut output_socket, child_output)) = UnixStream::pair() else {
            return failed();
        };
        let Ok((mut error_socket, child_error)) = UnixStream::pair() else {
            return failed();
        };
        for socket in [&input_socket, &output_socket, &error_socket] {
            if socket.set_nonblocking(true).is_err() {
                return failed();
            }
        }
        let mut command = match self.docker {
            DockerCommand::Native => Command::new("/usr/bin/docker"),
            DockerCommand::SudoNoPassword => {
                let mut c = Command::new("/usr/bin/sudo");
                c.args(["-n", "/usr/bin/docker"]);
                c
            }
        };
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &client.path)
            .current_dir(&client.path)
            .args([
                "--config",
                client.path.to_str().unwrap_or("/nonexistent"),
                "--host",
                "unix:///var/run/docker.sock",
            ])
            .args(args)
            .process_group(0)
            .stdin(Stdio::from(std::os::fd::OwnedFd::from(child_input)))
            .stdout(Stdio::from(std::os::fd::OwnedFd::from(child_output)))
            .stderr(Stdio::from(std::os::fd::OwnedFd::from(child_error)));
        let Ok(mut child) = command.spawn() else {
            return failed();
        };
        // Retained Stdio peers in Command would hide EOF after the child exits.
        drop(command);
        let mut capture = Capture {
            code: None,
            out: vec![],
            err: vec![],
            timeout: false,
            overflow: false,
            reaped: false,
        };
        let (mut offset, mut input_done, mut output_done, mut error_done) =
            (0, false, false, false);
        loop {
            if !input_done {
                match input_socket.write(&input[offset..]) {
                    Ok(n) => offset += n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                    Err(_) => {
                        input_done = true;
                    }
                }
                if offset == input.len() {
                    let _ = input_socket.shutdown(Shutdown::Write);
                    input_done = true;
                }
            }
            let stdout = read_bounded(&mut output_socket, &mut capture.out, out_cap);
            let stderr = read_bounded(&mut error_socket, &mut capture.err, CAP);
            match (stdout, stderr) {
                (Ok((o, a)), Ok((e, b))) => {
                    output_done |= o;
                    error_done |= e;
                    capture.overflow |= a || b;
                }
                _ => break,
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    capture.code = status.code();
                    capture.reaped = true;
                }
                Ok(None) => (),
                Err(_) => break,
            }
            capture.timeout = Instant::now() >= deadline;
            if capture.timeout || capture.overflow || (capture.reaped && output_done && error_done)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if !capture.reaped {
            capture.reaped = terminate(
                self.docker,
                &mut child,
                client
                    .hard_deadline
                    .min(Instant::now() + Duration::from_millis(50)),
            );
            capture.code = child.try_wait().ok().flatten().and_then(|s| s.code());
        }
        if !capture.reaped {
            client.launchers_clean.set(false);
            // SIGKILL was issued; reap without blocking the caller on a broken launcher.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        capture
    }

    fn inspect(&self, client: &Client, id: &str, deadline: Instant) -> Option<Value> {
        self.cli(
            client,
            &strings(&["container", "inspect", id]),
            &[],
            deadline,
            CAP,
        )
        .json()?
        .as_array()?
        .first()
        .cloned()
    }

    fn owned(value: &Value, name: &str, id: Option<&str>) -> bool {
        value["Name"] == format!("/{name}")
            && value["Config"]["Labels"][OWNER] == name
            && value["Config"]["Labels"]["io.alia.sandbox.role"] == "oneshot"
            && value["Id"].as_str().is_some_and(valid_id)
            && id.is_none_or(|id| value["Id"] == id)
    }

    fn cleanup(&self, client: &Client, name: &str, id: Option<&str>, deadline: Instant) -> bool {
        let Some(value) = self.inspect(client, id.unwrap_or(name), deadline) else {
            // A failed/ambiguous create is not certified absent from an arbitrary daemon error.
            return false;
        };
        if !Self::owned(&value, name, id) {
            return false;
        }
        let Some(id) = value["Id"].as_str() else {
            return false;
        };
        let removed = self.cli(
            client,
            &strings(&["container", "rm", "--force", id]),
            &[],
            deadline,
            CAP,
        );
        if !removed.ok() {
            return false;
        }
        let list = self.cli(
            client,
            &strings(&[
                "container",
                "ls",
                "--all",
                "--quiet",
                "--no-trunc",
                "--filter",
                &format!("id={id}"),
            ]),
            &[],
            deadline,
            CAP,
        );
        list.ok() && list.out.iter().all(u8::is_ascii_whitespace)
    }

    #[cfg(test)]
    #[allow(dead_code)] // A different targeted integration binary exercises this fixture.
    pub(crate) fn foreign_cleanup_probe(&self) -> Value {
        let hard = Instant::now() + Duration::from_secs(10);
        let client = Client::new(hard).unwrap();
        let name = format!("alia-sandbox-{}", Uuid::new_v4().simple());
        let made = self.cli(
            &client,
            &strings(&[
                "container",
                "create",
                "--pull=never",
                "--name",
                &name,
                "--network=none",
                "--read-only",
                "--user=65534:65534",
                "--label",
                &format!("{OWNER}=foreign-control"),
                "--label",
                "io.alia.sandbox.role=oneshot",
                &self.image,
                "python",
                "-c",
                "pass",
            ]),
            &[],
            hard,
            CAP,
        );
        assert!(made.ok());
        let id = std::str::from_utf8(&made.out).unwrap().trim();
        assert!(valid_id(id));
        let refused = !self.cleanup(&client, &name, Some(id), hard);
        let still_there = self
            .inspect(&client, id, hard)
            .is_some_and(|v| v["Id"] == id && v["Config"]["Labels"][OWNER] == "foreign-control");
        // Separately tracked test fixture, never a generic cleanup bypass.
        let removed = still_there
            && self
                .cli(
                    &client,
                    &strings(&["container", "rm", "--force", id]),
                    &[],
                    hard,
                    CAP,
                )
                .ok();
        let absent = self.cli(
            &client,
            &strings(&[
                "container",
                "ls",
                "--all",
                "--quiet",
                "--no-trunc",
                "--filter",
                &format!("id={id}"),
            ]),
            &[],
            hard,
            CAP,
        );
        serde_json::json!({"execution_id":name,"actual_id":id,"mismatched_owner_refused":refused,
            "foreign_fixture_preserved_before_owned_teardown":still_there,
            "manual_fixture_cleanup":removed && absent.ok() && absent.out.iter().all(u8::is_ascii_whitespace)})
    }

    pub fn execute(&self, request: &SandboxRequest) -> SandboxOutcome {
        let input = if request.source.is_empty() || request.source.len() > 32768 {
            ExecutionInput::Invalid
        } else {
            ExecutionInput::Raw(&request.source)
        };
        self.execute_inner(input, request.timeout_ms)
    }
    pub fn execute_skill(&self, request: &SkillSandboxRequest) -> SkillSandboxOutcome {
        let started = Instant::now();
        let source_sha256 = if request.source.len() <= 24576 {
            format!("{:x}", Sha256::digest(request.source.as_bytes()))
        } else {
            String::new()
        };
        let input_sha256 = if request.input_json.len() <= 8192 {
            format!("{:x}", Sha256::digest(request.input_json.as_bytes()))
        } else {
            String::new()
        };
        let input = skill_carrier(request).map_or(ExecutionInput::Invalid, |carrier| {
            ExecutionInput::Skill {
                carrier,
                source_sha256: source_sha256.clone(),
                input_sha256: input_sha256.clone(),
            }
        });
        let mut outcome = self.execute_inner(input, request.timeout_ms);
        outcome.duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        SkillSandboxOutcome {
            outcome,
            protocol: SKILL_PROTOCOL,
            source_sha256,
            input_sha256,
            wrapper_sha256: format!("{:x}", Sha256::digest(SKILL_WRAPPER.as_bytes())),
        }
    }
    fn execute_inner(&self, input: ExecutionInput<'_>, timeout_ms: u64) -> SandboxOutcome {
        let started = Instant::now();
        let name = format!("alia-sandbox-{}", Uuid::new_v4().simple());
        let mut outcome = SandboxOutcome {
            execution_id: name.clone(),
            status: SandboxStatus::Unavailable,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            output_truncated: false,
            duration_ms: 0,
            image_digest: self.image.clone(),
            isolation_verified: false,
            cleanup_passed: true,
        };
        if matches!(input, ExecutionInput::Invalid) || !(100..=10000).contains(&timeout_ms) {
            outcome.stderr = "invalid sandbox request".into();
            return outcome;
        }
        // The requested deadline includes setup, user execution AND owned teardown.
        // Too little time to reserve teardown means no container is launched.
        if timeout_ms <= CLEANUP_MS {
            outcome.status = SandboxStatus::TimedOut;
            outcome.stderr = "deadline cannot admit isolated launch and cleanup".into();
            return outcome;
        }
        let hard = started + Duration::from_millis(timeout_ms);
        let deadline = hard - Duration::from_millis(CLEANUP_MS);
        let Ok(client) = Client::new(hard) else {
            outcome.stderr = "sandbox client unavailable".into();
            return outcome;
        };
        let mut attempted = false;
        let mut id = None;
        let mut run = || -> Result<(), &'static str> {
            let info = self
                .cli(
                    &client,
                    &strings(&["info", "--format", "{{json .}}"]),
                    &[],
                    deadline,
                    CAP,
                )
                .json()
                .ok_or("sandbox engine unavailable")?;
            if !supported_engine(&info) {
                return Err("mandatory sandbox controls unavailable");
            }
            let image = self
                .cli(
                    &client,
                    &strings(&["image", "inspect", &self.image]),
                    &[],
                    deadline,
                    CAP,
                )
                .json()
                .and_then(|v| v.as_array()?.first().cloned())
                .ok_or("pinned sandbox image unavailable")?;
            let image_id = image["Id"]
                .as_str()
                .filter(|s| s.starts_with("sha256:"))
                .ok_or("sandbox image invalid")?;
            if !image["RepoDigests"]
                .as_array()
                .is_some_and(|v| v.iter().any(|v| v == &self.image))
                || !empty(&image["Config"]["Volumes"])
            {
                return Err("sandbox image policy unavailable");
            }
            let skill_mode = matches!(input, ExecutionInput::Skill { .. });
            let marker = if skill_mode {
                SKILL_MARKER.to_owned()
            } else {
                format!("ALIA_SANDBOX_VERIFIED_{}\n", Uuid::new_v4().simple())
            };
            let stdin = match &input {
                ExecutionInput::Raw(source) => bootstrap(source, &marker).into_bytes(),
                ExecutionInput::Skill { carrier, .. } => carrier.clone(),
                ExecutionInput::Invalid => unreachable!(),
            };
            let mut args = strings(&[
                "container",
                "create",
                "--pull=never",
                "--name",
                &name,
                "--label",
                &format!("{OWNER}={name}"),
                "--label",
                "io.alia.sandbox.role=oneshot",
                "--network=none",
                "--read-only",
                "--cap-drop=ALL",
                "--security-opt=no-new-privileges=true",
                "--user=65534:65534",
                "--oom-kill-disable=false",
                "--pids-limit=64",
                "--memory=134217728",
                "--memory-swap=134217728",
                "--cpus=1",
                "--cgroupns=private",
                "--ipc=private",
                "--tmpfs",
                &format!("/workspace:{TMPFS}"),
                "--tmpfs",
                &format!("/tmp:{TMPFS}"),
                "--workdir=/workspace",
                "--log-driver=none",
                "--no-healthcheck",
                "--interactive",
                "--entrypoint=/usr/local/bin/python",
            ]);
            if let ExecutionInput::Skill {
                source_sha256,
                input_sha256,
                ..
            } = &input
            {
                args.extend(strings(&[
                    "--label",
                    "io.alia.sandbox.protocol=python-skill/v1",
                    "--label",
                    &format!("io.alia.sandbox.source={source_sha256}"),
                    "--label",
                    &format!("io.alia.sandbox.input={input_sha256}"),
                ]));
            }
            args.push(self.image.clone());
            if skill_mode {
                args.extend(strings(&["-I", "-B", "-c", SKILL_WRAPPER]));
            } else {
                args.extend(strings(&["-I", "-B", "-"]));
            }
            attempted = true;
            let created = self.cli(&client, &args, &[], deadline, CAP);
            if !created.ok() {
                return Err("sandbox creation unavailable");
            }
            let value = std::str::from_utf8(&created.out)
                .ok()
                .map(str::trim)
                .filter(|s| valid_id(s))
                .ok_or("sandbox create identity invalid")?;
            id = Some(value.to_owned());
            let before = self
                .inspect(&client, value, deadline)
                .ok_or("sandbox inspection unavailable")?;
            if !Self::owned(&before, &name, Some(value))
                || !input.labels_match(&before)
                || !(if skill_mode {
                    skill_policy(&before, &self.image, image_id, &image["Config"]["Env"])
                } else {
                    policy(&before, &self.image, image_id, &image["Config"]["Env"])
                })
            {
                return Err("sandbox isolation policy mismatch");
            }
            let result = self.cli(
                &client,
                &strings(&["container", "start", "--attach", "--interactive", value]),
                &stdin,
                deadline,
                CAP + marker.len(),
            );
            let verified = result.out.starts_with(marker.as_bytes());
            outcome.isolation_verified = verified;
            let mut limited = result.overflow;
            outcome.status = if result.timeout {
                SandboxStatus::TimedOut
            } else if result.overflow {
                SandboxStatus::OutputLimit
            } else if !verified {
                SandboxStatus::Unavailable
            } else {
                SandboxStatus::InfrastructureFailed
            };
            if verified {
                let (stdout, stdout_cut) = bounded_text(&result.out[marker.len()..]);
                let (stderr, stderr_cut) = bounded_text(&result.err);
                limited |= stdout_cut || stderr_cut;
                outcome.stdout = stdout;
                outcome.stderr = stderr;
            }
            outcome.output_truncated = limited;
            if limited && !result.timeout {
                outcome.status = SandboxStatus::OutputLimit;
            }
            // Stop before inspecting state on deadline/output termination. Never let the
            // user process keep running while waiting for a captured future to finish.
            if result.timeout || result.overflow {
                let _ = self.cli(
                    &client,
                    &strings(&["container", "kill", value]),
                    &[],
                    hard,
                    CAP,
                );
            }
            let after = self
                .inspect(&client, value, hard)
                .ok_or("sandbox final state unavailable")?;
            if !Self::owned(&after, &name, Some(value)) {
                return Err("sandbox final identity mismatch");
            }
            outcome.exit_code = after["State"]["ExitCode"]
                .as_i64()
                .and_then(|v| i32::try_from(v).ok());
            if !result.timeout && !limited && verified && result.reaped {
                outcome.status = if after["State"]["OOMKilled"] == true {
                    SandboxStatus::ResourceExceeded
                } else if after["State"]["Running"] == false && outcome.exit_code == Some(0) {
                    SandboxStatus::Succeeded
                } else if after["State"]["Running"] == false && outcome.exit_code.is_some() {
                    SandboxStatus::CodeFailed
                } else {
                    SandboxStatus::InfrastructureFailed
                };
            }
            if !verified {
                return Err("mandatory in-container controls unavailable");
            }
            Ok(())
        };
        if let Err(error) = run() {
            outcome.stdout.clear();
            outcome.stderr = error.into();
            if Instant::now() >= deadline {
                outcome.status = SandboxStatus::TimedOut;
            }
        }
        if attempted {
            outcome.cleanup_passed = self.cleanup(&client, &name, id.as_deref(), hard);
        }
        outcome.cleanup_passed &= client.launchers_clean.get();
        outcome.cleanup_passed &= fs::remove_dir_all(&client.path).is_ok();
        enforce_cleanup(&mut outcome);
        outcome.duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        outcome
    }
}

fn enforce_cleanup(outcome: &mut SandboxOutcome) {
    if !outcome.cleanup_passed {
        outcome.status = SandboxStatus::InfrastructureFailed;
        outcome.stderr = "sandbox owned cleanup incomplete; retain execution handle".into();
    }
}

fn supported_engine(info: &Value) -> bool {
    info["OSType"] == "linux"
        && info["CgroupVersion"] == "2"
        && info["MemoryLimit"] == true
        && info["SwapLimit"] == true
        && info["PidsLimit"] == true
        && info["SecurityOptions"]
            .as_array()
            .is_some_and(|v| v.iter().any(|s| s == "name=seccomp,profile=builtin"))
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| (*s).to_owned()).collect()
}
fn valid_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn empty(v: &Value) -> bool {
    v.is_null()
        || v.as_array().is_some_and(Vec::is_empty)
        || v.as_object().is_some_and(|m| m.is_empty())
}
fn bounded_text(bytes: &[u8]) -> (String, bool) {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    let truncated = text.len() > CAP;
    let mut end = text.len().min(CAP);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    (text, truncated)
}
fn policy(v: &Value, image: &str, image_id: &str, image_env: &Value) -> bool {
    policy_for(
        v,
        image,
        image_id,
        image_env,
        &serde_json::json!(["-I", "-B", "-"]),
    )
}
fn skill_policy(v: &Value, image: &str, image_id: &str, image_env: &Value) -> bool {
    v["Config"]["Labels"]["io.alia.sandbox.protocol"] == SKILL_PROTOCOL
        && policy_for(
            v,
            image,
            image_id,
            image_env,
            &serde_json::json!(["-I", "-B", "-c", SKILL_WRAPPER]),
        )
}
fn policy_for(v: &Value, image: &str, image_id: &str, image_env: &Value, command: &Value) -> bool {
    let h = &v["HostConfig"];
    let c = &v["Config"];
    v["Image"] == image_id
        && c["Image"] == image
        && c["User"] == "65534:65534"
        && c["WorkingDir"] == "/workspace"
        && c["Entrypoint"] == serde_json::json!(["/usr/local/bin/python"])
        && c["Cmd"] == *command
        && c["Tty"] == false
        && c["OpenStdin"] == true
        && c["Env"] == *image_env
        && empty(&c["Volumes"])
        && empty(&c["ExposedPorts"])
        && h["NetworkMode"] == "none"
        && h["ReadonlyRootfs"] == true
        && h["Privileged"] == false
        && h["CapDrop"] == serde_json::json!(["ALL"])
        && empty(&h["CapAdd"])
        && h["SecurityOpt"] == serde_json::json!(["no-new-privileges=true"])
        && h["PidsLimit"] == 64
        && h["Memory"] == MEMORY
        && h["MemorySwap"] == MEMORY
        && h["OomKillDisable"] == false
        && h["AutoRemove"] == false
        && h["NanoCpus"] == 1_000_000_000u64
        && h["CgroupnsMode"] == "private"
        && h["IpcMode"] == "private"
        && h["PidMode"] == ""
        && h["UTSMode"] == ""
        && h["UsernsMode"] == ""
        && h["PublishAllPorts"] == false
        && empty(&h["PortBindings"])
        && empty(&h["Binds"])
        && empty(&h["Mounts"])
        && empty(&h["VolumesFrom"])
        && empty(&h["Devices"])
        && empty(&h["DeviceRequests"])
        && empty(&h["DeviceCgroupRules"])
        && empty(&h["GroupAdd"])
        && h["LogConfig"]["Type"] == "none"
        && h["RestartPolicy"]["Name"] == "no"
        && c["Healthcheck"]["Test"] == serde_json::json!(["NONE"])
        && h["Tmpfs"] == serde_json::json!({"/workspace":TMPFS,"/tmp":TMPFS})
        && v["Mounts"].as_array().is_some_and(|m| {
            m.iter().all(|m| {
                m["Type"] == "tmpfs"
                    && (m["Destination"] == "/workspace" || m["Destination"] == "/tmp")
            })
        })
}

macro_rules! isolation_prelude {
    () => {r#"import os, sys
try:
    assert sys.flags.isolated == 1 and sys.dont_write_bytecode
    assert os.getuid() == 65534 and os.getgid() == 65534 and os.getcwd() == '/workspace'
    status = dict(line.split(':', 1) for line in open('/proc/self/status') if ':' in line)
    assert status['NoNewPrivs'].strip() == '1' and status['Seccomp'].strip() == '2'
    assert int(status['CapEff'].strip(),16) == 0
    def cg(name): return open('/sys/fs/cgroup/'+name).read().strip()
    assert cg('memory.max') == '134217728' and cg('memory.swap.max') == '0' and cg('pids.max') == '64'
    quota, period = cg('cpu.max').split()
    assert quota != 'max' and int(quota) == int(period)
    mounts = [line.split() for line in open('/proc/self/mountinfo')]
    for destination in ['/workspace','/tmp']:
        assert os.stat(destination).st_mode & 0o7777 == 0o1777
        record = next(m for m in mounts if m[4] == destination)
        assert {'rw','nosuid','nodev','noexec'} <= set(record[5].split(','))
        assert 'tmpfs' in record and 'size=16384k' in record[-1]
    assert 'ro' in next(m for m in mounts if m[4] == '/')[5].split(',')
    assert set(line.split(':')[0].strip() for line in open('/proc/net/dev') if ':' in line) == {'lo'}
except BaseException:
    sys.stderr.write('mandatory sandbox controls unavailable\n')
    sys.exit(125)
os.environ.clear()
os.environ['PATH']='/usr/local/bin:/usr/bin:/bin'
"#};
}
pub const SKILL_WRAPPER: &str = concat!(
    isolation_prelude!(),
    r#"sys.stdout.write('ALIA_SANDBOX_SKILL_VERIFIED_V1\n'); sys.stdout.flush()
import json, inspect, ast
# The host strictly validates the original <=8KiB input before constructing this carrier.
def _pairs(items):
    result={}
    for key,value in items:
        if key in result: raise ValueError('duplicate JSON key')
        result[key]=value
    return result
def _constant(value): raise ValueError('nonfinite JSON')
raw=sys.stdin.buffer.read(163841)
if len(raw)>163840: raise ValueError('carrier limit')
data=json.loads(raw,object_pairs_hook=_pairs,parse_constant=_constant)
if not isinstance(data,dict) or set(data)!={'source','input'}: raise ValueError('carrier shape')
source=data['source']
if not isinstance(source,str) or not 1<=len(source.encode('utf-8'))<=24576: raise ValueError('source limit')
tree=ast.parse(source,filename='<alia-skill>',mode='exec')
definitions=[node for node in tree.body if isinstance(node,(ast.FunctionDef,ast.AsyncFunctionDef)) and node.name=='run']
if len(definitions)!=1 or not isinstance(definitions[0],ast.FunctionDef): raise TypeError('top-level def run required')
namespace={'__name__':'alia_skill'}
exec(compile(tree,'<alia-skill>','exec'),namespace)
function=namespace.get('run')
if not inspect.isfunction(function): raise TypeError('def run required')
parameters=list(inspect.signature(function).parameters.values())
if len(parameters)!=1 or parameters[0].kind not in (inspect.Parameter.POSITIONAL_ONLY,inspect.Parameter.POSITIONAL_OR_KEYWORD): raise TypeError('one input required')
result=function(data['input'])
print(json.dumps(result,ensure_ascii=False,allow_nan=False,separators=(',',':')))
"#
);
fn bootstrap(source: &str, marker: &str) -> String {
    let encoded: String = source
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!(
        "{}sys.stdout.write({marker:?}); sys.stdout.flush()\nexec(compile(bytes.fromhex('{encoded}'), '<sandbox>', 'exec'), {{'__name__':'__main__'}})\n",
        isolation_prelude!()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_and_debug_never_accept_extra_authority_or_expose_source() {
        assert!(
            serde_json::from_value::<SandboxRequest>(
                serde_json::json!({"source":"secret","timeout_ms":100,"mount":"/"})
            )
            .is_err()
        );
        assert!(
            !format!(
                "{:?}",
                SandboxRequest {
                    source: "private-source-marker".into(),
                    timeout_ms: 100
                }
            )
            .contains("private-source-marker")
        );
        for image in [
            "python:latest",
            "alpine@sha256:aaaa",
            "python@sha256:../escape",
            "python@sha256:FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        ] {
            assert!(matches!(
                SandboxExecutor::new(image, DockerCommand::Native),
                Err(SandboxError::InvalidImage)
            ));
        }
    }
    #[test]
    fn ownership_never_accepts_same_name_foreign_label_or_changed_actual_id() {
        let id = "a".repeat(64);
        let name = "alia-sandbox-fixture";
        let v = serde_json::json!({"Id":id,"Name":format!("/{name}"),"Config":{"Labels":{OWNER:name,"io.alia.sandbox.role":"oneshot"}}});
        assert!(SandboxExecutor::owned(&v, name, Some(&id)));
        let mut foreign = v.clone();
        foreign["Config"]["Labels"][OWNER] = "foreign".into();
        assert!(!SandboxExecutor::owned(&foreign, name, Some(&id)));
        assert!(!SandboxExecutor::owned(&v, name, Some(&"b".repeat(64))));
    }
    #[test]
    fn invalid_requests_and_unadmittable_deadlines_launch_nothing() {
        let s = SandboxExecutor::new(
            &format!("python@sha256:{}", "a".repeat(64)),
            DockerCommand::Native,
        )
        .unwrap();
        for r in [
            SandboxRequest {
                source: String::new(),
                timeout_ms: 10000,
            },
            SandboxRequest {
                source: "x".repeat(32769),
                timeout_ms: 10000,
            },
            SandboxRequest {
                source: "print(1)".into(),
                timeout_ms: 99,
            },
        ] {
            let o = s.execute(&r);
            assert_eq!(o.status, SandboxStatus::Unavailable);
            assert!(o.cleanup_passed);
            assert!(!o.isolation_verified);
        }
        let o = s.execute(&SandboxRequest {
            source: "print(1)".into(),
            timeout_ms: 100,
        });
        assert_eq!(o.status, SandboxStatus::TimedOut);
        assert!(o.cleanup_passed);
    }
    #[test]
    fn output_unicode_and_source_encoding_stay_bounded_and_not_arguments() {
        let (text, truncated) = bounded_text(&vec![0xff; CAP]);
        assert!(text.len() <= CAP && truncated);
        let b = bootstrap("print('private-source-marker')", "nonce\n");
        assert!(!b.contains("private-source-marker"));
        assert!(b.contains("bytes.fromhex"));
    }

    #[test]
    fn failed_cleanup_never_reports_success_or_loses_handle() {
        let mut o = SandboxOutcome {
            execution_id: "exact-owned-handle".into(),
            status: SandboxStatus::Succeeded,
            exit_code: Some(0),
            stdout: "actual result".into(),
            stderr: String::new(),
            output_truncated: false,
            duration_ms: 0,
            image_digest: String::new(),
            isolation_verified: true,
            cleanup_passed: false,
        };
        enforce_cleanup(&mut o);
        assert_eq!(o.status, SandboxStatus::InfrastructureFailed);
        assert_eq!(o.execution_id, "exact-owned-handle");
        assert_eq!(o.stdout, "actual result");
    }

    #[test]
    fn mandatory_policy_mismatches_fail_before_execution() {
        let v = serde_json::json!({"Image":"sha256:actual","Mounts":[],"Config":{
            "Image":"python@sha256:reference","User":"65534:65534","WorkingDir":"/workspace",
            "Entrypoint":["/usr/local/bin/python"],"Cmd":["-I","-B","-"],"Tty":false,"OpenStdin":true,
            "Env":[],"Volumes":null,"ExposedPorts":null,"Healthcheck":{"Test":["NONE"]}},
            "HostConfig":{"NetworkMode":"none","ReadonlyRootfs":true,"Privileged":false,
            "CapDrop":["ALL"],"CapAdd":null,"SecurityOpt":["no-new-privileges=true"],"PidsLimit":64,
            "Memory":MEMORY,"MemorySwap":MEMORY,"OomKillDisable":false,"AutoRemove":false,"NanoCpus":1000000000,
            "CgroupnsMode":"private","IpcMode":"private","PidMode":"","UTSMode":"","UsernsMode":"",
            "PublishAllPorts":false,"PortBindings":{},"Binds":null,"Mounts":null,"VolumesFrom":null,
            "Devices":[],"DeviceRequests":null,"DeviceCgroupRules":null,"GroupAdd":null,
            "LogConfig":{"Type":"none"},"RestartPolicy":{"Name":"no"},"Tmpfs":{"/workspace":TMPFS,"/tmp":TMPFS}}});
        assert!(policy(
            &v,
            "python@sha256:reference",
            "sha256:actual",
            &serde_json::json!([])
        ));
        let mut skill = v.clone();
        skill["Config"]["Cmd"] = serde_json::json!(["-I", "-B", "-c", SKILL_WRAPPER]);
        skill["Config"]["Labels"] = serde_json::json!({"io.alia.sandbox.protocol":SKILL_PROTOCOL});
        assert!(skill_policy(
            &skill,
            "python@sha256:reference",
            "sha256:actual",
            &serde_json::json!([])
        ));
        for command in [
            serde_json::json!(["-I", "-B", "-"]),
            serde_json::json!(["-I", "-B", "-c", "arbitrary"]),
            serde_json::json!(["-I", "-B", "-c", SKILL_WRAPPER, "extra"]),
        ] {
            let mut bad = skill.clone();
            bad["Config"]["Cmd"] = command;
            assert!(!skill_policy(
                &bad,
                "python@sha256:reference",
                "sha256:actual",
                &serde_json::json!([])
            ));
        }
        skill["Config"]["Labels"]["io.alia.sandbox.protocol"] = "python-raw/v1".into();
        assert!(!skill_policy(
            &skill,
            "python@sha256:reference",
            "sha256:actual",
            &serde_json::json!([])
        ));
        for (field, value) in [
            ("NetworkMode", serde_json::json!("host")),
            ("ReadonlyRootfs", serde_json::json!(false)),
            ("Privileged", serde_json::json!(true)),
            ("PidsLimit", serde_json::json!(0)),
            ("Memory", serde_json::json!(0)),
            ("MemorySwap", serde_json::json!(-1)),
            ("NanoCpus", serde_json::json!(0)),
            ("CapDrop", serde_json::json!([])),
            ("SecurityOpt", serde_json::json!(["seccomp=unconfined"])),
            ("Binds", serde_json::json!(["/:/host"])),
            ("Devices", serde_json::json!([{"PathOnHost":"/dev/x"}])),
            ("Tmpfs", serde_json::json!({})),
        ] {
            let mut changed = v.clone();
            changed["HostConfig"][field] = value;
            assert!(
                !policy(
                    &changed,
                    "python@sha256:reference",
                    "sha256:actual",
                    &serde_json::json!([])
                ),
                "{field}"
            );
        }
        for (field, value) in [
            ("User", serde_json::json!("0")),
            ("Env", serde_json::json!(["SECRET=bad"])),
            ("Cmd", serde_json::json!(["-c", "host source"])),
            ("Volumes", serde_json::json!({"/host":{}})),
        ] {
            let mut changed = v.clone();
            changed["Config"][field] = value;
            assert!(
                !policy(
                    &changed,
                    "python@sha256:reference",
                    "sha256:actual",
                    &serde_json::json!([])
                ),
                "{field}"
            );
        }
    }

    #[test]
    fn unsupported_engine_controls_are_unavailable_not_weaker_fallbacks() {
        let good = serde_json::json!({"OSType":"linux","CgroupVersion":"2","MemoryLimit":true,
            "SwapLimit":true,"PidsLimit":true,"SecurityOptions":["name=seccomp,profile=builtin"]});
        assert!(supported_engine(&good));
        for field in [
            "OSType",
            "CgroupVersion",
            "MemoryLimit",
            "SwapLimit",
            "PidsLimit",
            "SecurityOptions",
        ] {
            let mut changed = good.clone();
            changed[field] = Value::Null;
            assert!(!supported_engine(&changed));
        }
    }
    #[test]
    fn structured_preflight_keeps_limits_strict_json_and_debug_private() {
        let good = SkillSandboxRequest {
            source: "def run(x): return x\n".into(),
            input_json: "null".into(),
            timeout_ms: 10000,
        };
        let carrier = skill_carrier(&good).unwrap();
        let data: Value = serde_json::from_slice(&carrier).unwrap();
        assert_eq!(data["source"], good.source);
        assert_eq!(data["input"], Value::Null);
        for value in ["NaN", "1e400", "{} {}", "{\"x\":1,\"x\":2}"] {
            let mut bad = good.clone();
            bad.input_json = value.into();
            assert!(skill_carrier(&bad).is_none());
        }
        let mut maximum = good.clone();
        maximum.source = "\u{1}".repeat(24576);
        maximum.input_json = format!("\"{}\"", "x".repeat(8190));
        assert!(skill_carrier(&maximum).unwrap().len() <= SKILL_CARRIER_LIMIT);
        maximum.source.push('x');
        assert!(skill_carrier(&maximum).is_none());
        let mut large = good.clone();
        large.input_json = " ".repeat(8193);
        assert!(skill_carrier(&large).is_none());
        let mut secret = good;
        secret.source = "PRIVATE_SOURCE".into();
        secret.input_json = "\"PRIVATE_INPUT\"".into();
        let debug = format!("{secret:?}");
        assert!(!debug.contains("PRIVATE_SOURCE") && !debug.contains("PRIVATE_INPUT"));
    }
    #[test]
    fn structured_protocol_cannot_reuse_raw_or_arbitrary_c_argv() {
        assert!(SKILL_WRAPPER.starts_with(isolation_prelude!()));
        assert!(SKILL_WRAPPER.contains("namespace.get('run')"));
        assert!(!SKILL_WRAPPER.contains("PRIVATE_SOURCE"));
        let wrong = serde_json::json!({"source":"pass","input_json":"null","timeout_ms":10000,"mode":"raw"});
        assert!(serde_json::from_value::<SkillSandboxRequest>(wrong).is_err());
    }
}
