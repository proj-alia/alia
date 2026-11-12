//! Targeted private-vault tests. Real skill validation ALWAYS calls real Docker.
mod domain {
    pub use alia::domain::config;
}
#[path = "../src/adapters/sandbox.rs"]
pub mod sandbox;
#[path = "../src/adapters/sandbox_skills.rs"]
pub mod sandbox_skills;
use chrono::{Duration as CDuration, Utc};
use sandbox::{
    DockerCommand, SKILL_PROTOCOL, SKILL_WRAPPER, SandboxExecutor, SandboxStatus,
    SkillSandboxRequest,
};
use sandbox_skills::*;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;
struct Private(PathBuf);
impl Private {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("alia-skills-test-{}", Uuid::new_v4().simple()));
        DirBuilder::new().mode(0o700).create(&p).unwrap();
        Self(p)
    }
    fn vault(&self) -> SkillVault {
        SkillVault::create_in(&self.0, DockerCommand::SudoNoPassword).unwrap()
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn draft(source: &str) -> CandidateDraft {
    let candidate = CandidateDraft {
        individual_id: Uuid::new_v4(),
        name: "QA skill".into(),
        description: "synthetic private validation".into(),
        source: {
            let stamp = format!("\n# QA_SKILL_OWNER_{}\n", Uuid::new_v4().simple());
            if source.len() + stamp.len() <= 24576 {
                format!("{source}{stamp}")
            } else {
                source.into()
            }
        },
        source_event_ids: vec![Uuid::new_v4()],
    };
    record(
        &format!("source-ledger-{}", sha(candidate.source.as_bytes())),
        &json!({"source_sha256":sha(candidate.source.as_bytes())}),
    );
    candidate
}
fn cases(values: &[(&str, &str, &str)]) -> ValidationCases {
    ValidationCases::new(
        values
            .iter()
            .map(|(id, i, e)| (id.to_string(), i.to_string(), e.to_string()))
            .collect(),
    )
    .unwrap()
}
fn source_file(root: &Path, v: &SkillVersion) -> PathBuf {
    root.join(format!("source-{}-{}.py", v.skill_id.simple(), v.version))
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn record(name: &str, v: &impl Serialize) {
    if let Ok(out) = std::env::var("ALIA_SKILL_REPORT_DIR") {
        let p = Path::new(&out).join(format!("{name}.json"));
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(p)
            .unwrap();
        f.write_all(&serde_json::to_vec_pretty(v).unwrap()).unwrap();
        f.sync_all().unwrap();
    }
}
fn record_validation(name: &str, vault: &SkillVault, attempt: &ValidationAttempt) {
    record(name, attempt);
    if attempt.durable {
        let report = vault
            .validation_report(attempt.operation_id.unwrap())
            .unwrap();
        record(&format!("{name}-artifact"), &report);
    }
    let mut files = serde_json::Map::new();
    for entry in fs::read_dir(vault.root()).unwrap() {
        let e = entry.unwrap();
        let m = e.metadata().unwrap();
        files.insert(
            e.file_name().to_string_lossy().into_owned(),
            json!({"sha256":sha(&fs::read(e.path()).unwrap()),
            "mode":m.mode()&0o7777,"nlink":m.nlink(),"bytes":m.len()}),
        );
    }
    record(&format!("{name}-vault-files"), &Value::Object(files));
}
fn control(arguments: &[&str]) -> std::process::Output {
    let config = std::env::var("ALIA_SKILL_CONTROL_CONFIG")
        .expect("private test controller config required");
    Command::new("/usr/bin/sudo")
        .args([
            "-n",
            "/usr/bin/docker",
            "--config",
            &config,
            "--host",
            "unix:///var/run/docker.sock",
        ])
        .args(arguments)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap()
}

fn admission() {
    assert_eq!(std::env::var("ALIA_SANDBOX_ACTUAL").as_deref(), Ok("1"));
    let fd = std::env::var("ALIA_QA_LOCK_FD").unwrap();
    let inherited = fs::metadata(format!("/proc/self/fd/{fd}")).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/tmp/alia-project-heavy.lock")
        .unwrap();
    assert_eq!(
        (inherited.dev(), inherited.ino()),
        (
            lock.metadata().unwrap().dev(),
            lock.metadata().unwrap().ino()
        )
    );
    assert!(matches!(lock.try_lock(), Err(fs::TryLockError::WouldBlock)));
}
fn register(
    vault: &SkillVault,
    verified: &ValidatedSkill,
    expires: chrono::DateTime<Utc>,
) -> RegistrationReceipt {
    vault
        .register(RegistrationGrant::issue(verified, expires, SkillScope::SandboxPython).unwrap())
        .unwrap()
}
fn child_recover(root: &Path, v: &SkillVersion, phase: SkillPhase) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "new_process_recovery", "--nocapture"])
        .env("ALIA_SKILL_RECOVERY_ROOT", root)
        .env(
            "ALIA_SKILL_RECOVERY_VERSION",
            serde_json::to_string(v).unwrap(),
        )
        .env(
            "ALIA_SKILL_RECOVERY_PHASE",
            serde_json::to_string(&phase).unwrap(),
        )
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("SKILL_RECOVERY_OK"));
}
#[test]
fn new_process_recovery() {
    if let Ok(root) = std::env::var("ALIA_SKILL_RECOVERY_ROOT") {
        let v: SkillVersion =
            serde_json::from_str(&std::env::var("ALIA_SKILL_RECOVERY_VERSION").unwrap()).unwrap();
        let phase: SkillPhase =
            serde_json::from_str(&std::env::var("ALIA_SKILL_RECOVERY_PHASE").unwrap()).unwrap();
        let vault = SkillVault::open(Path::new(&root), DockerCommand::SudoNoPassword).unwrap();
        let view = vault.view(v.individual_id, v.skill_id).unwrap();
        assert_eq!(view.version, v);
        assert_eq!(view.phase, phase);
        if matches!(phase, SkillPhase::Validated | SkillPhase::Registered) {
            assert!(view.verified.is_some());
        }
        println!("SKILL_RECOVERY_OK");
        return;
    }
    let dir = Private::new();
    let vault = dir.vault();
    let v = vault
        .create_candidate(draft("def run(input_json): return input_json\n"))
        .unwrap();
    child_recover(vault.root(), &v, SkillPhase::Candidate);
}
#[test]
fn candidate_versions_preserve_identity_old_source_and_never_self_register() {
    let dir = Private::new();
    let vault = dir.vault();
    let d = draft("def run(x): return x\n");
    let v = vault.create_candidate(d.clone()).unwrap();
    assert_eq!(v.version, 1);
    assert!(!v.skill_id.is_nil());
    let old = fs::read(source_file(vault.root(), &v)).unwrap();
    let mut changed = d;
    changed.source = "def run(x): return {'new': x}\n".into();
    let next = vault.revise(&v, changed).unwrap();
    assert_eq!(v.skill_id, next.skill_id);
    assert_eq!(next.version, 2);
    assert_ne!(v.source_sha256, next.source_sha256);
    assert_eq!(fs::read(source_file(vault.root(), &v)).unwrap(), old);
    assert_eq!(
        vault.view(next.individual_id, next.skill_id).unwrap().phase,
        SkillPhase::Candidate
    );
    assert_eq!(
        vault.invoke(&next, "null", 10000).result,
        Err(SkillError::NotRegistered)
    );
    assert_eq!(vault.invoke(&v, "null", 10000).sandbox_calls, 0);
}
#[test]
fn metadata_and_closed_requests_reject_unknown_identity_and_bounds() {
    let dir = Private::new();
    let vault = dir.vault();
    let good = draft("def run(x): return x\n");
    let mut variants = Vec::new();
    let mut bad = good.clone();
    bad.individual_id = Uuid::nil();
    variants.push(bad);
    let mut bad = good.clone();
    bad.name = "a".repeat(81);
    variants.push(bad);
    let mut bad = good.clone();
    bad.description = "x".repeat(2001);
    variants.push(bad);
    let mut bad = good.clone();
    bad.source = "x".repeat(24577);
    variants.push(bad);
    let mut bad = good.clone();
    bad.source_event_ids.clear();
    variants.push(bad);
    let mut bad = good.clone();
    bad.source_event_ids = vec![bad.source_event_ids[0]; 2];
    variants.push(bad);
    let mut bad = good.clone();
    bad.source_event_ids = vec![Uuid::new_v4(); 33];
    variants.push(bad);
    for bad in variants {
        assert!(vault.create_candidate(bad).is_err());
    }
    let mut value = serde_json::to_value(good).unwrap();
    value["registered"] = true.into();
    assert!(serde_json::from_value::<CandidateDraft>(value).is_err());
    for extra in ["argv", "mode", "mount", "registered"] {
        let mut value =
            json!({"source":"def run(x): return x","input_json":"null","timeout_ms":10000});
        value[extra] = true.into();
        assert!(serde_json::from_value::<SkillSandboxRequest>(value).is_err());
    }
}
#[test]
fn strict_case_controller_rejects_duplicates_nan_tail_and_oversize() {
    let three = ValidationCases::new(
        (0..3)
            .map(|i| (i.to_string(), "null".into(), "null".into()))
            .collect(),
    )
    .unwrap();
    assert_eq!(three.sandbox_calls_upper_bound(), 3);
    assert!(
        ValidationCases::new(
            (0..4)
                .map(|i| (i.to_string(), "null".into(), "null".into()))
                .collect()
        )
        .is_err()
    );
    for input in ["{\"a\":1,\"a\":2}", "NaN", "1e400", "null false", ""] {
        assert!(ValidationCases::new(vec![("case".into(), input.into(), "null".into())]).is_err());
    }
    assert!(ValidationCases::new(vec![]).is_err());
    assert!(ValidationCases::new(vec![("same".into(), "null".into(), "null".into()); 2]).is_err());
    assert!(
        ValidationCases::new(vec![(
            "case".into(),
            format!("\"{}\"", "x".repeat(8191)),
            "null".into()
        )])
        .is_err()
    );
}
#[test]
fn private_permissions_links_symlinks_and_source_tampering_fail_closed() {
    for mode in [
        "source",
        "head",
        "permissions",
        "symlink",
        "hardlink",
        "unknown",
    ] {
        let dir = Private::new();
        let vault = dir.vault();
        let v = vault
            .create_candidate(draft("def run(x): return x\n"))
            .unwrap();
        let p = source_file(vault.root(), &v);
        match mode {
            "source" => fs::write(&p, "def run(x): return 999\n").unwrap(),
            "head" => {
                let p = vault.root().join(".head");
                let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
                v["passed"] = true.into();
                fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
            }
            "permissions" => fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap(),
            "symlink" => {
                let outside = dir.0.join("outside");
                fs::write(&outside, fs::read(&p).unwrap()).unwrap();
                fs::remove_file(&p).unwrap();
                std::os::unix::fs::symlink(outside, &p).unwrap();
            }
            "hardlink" => fs::hard_link(&p, dir.0.join("outside")).unwrap(),
            "unknown" => fs::write(vault.root().join("arbitrary-source.py"), "pass").unwrap(),
            _ => unreachable!(),
        }
        assert!(
            SkillVault::open(vault.root(), DockerCommand::SudoNoPassword).is_err(),
            "{mode}"
        );
    }
    let dir = Private::new();
    let linked = dir.0.join("link");
    std::os::unix::fs::symlink(&dir.0, &linked).unwrap();
    assert!(SkillVault::create_in(&linked, DockerCommand::Native).is_err());
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(SkillVault::create_in(&dir.0, DockerCommand::Native).is_err());
}
#[test]
fn tail_truncation_and_pending_partial_publication_cannot_resurrect_state() {
    for pending in [false, true] {
        let dir = Private::new();
        let vault = dir.vault();
        vault
            .create_candidate(draft("def run(x): return x\n"))
            .unwrap();
        if pending {
            fs::write(vault.root().join(".pending-fault"), "incomplete").unwrap();
        } else {
            fs::remove_file(vault.root().join("event-00000000000000000001.json")).unwrap();
        }
        assert!(SkillVault::open(vault.root(), DockerCommand::Native).is_err());
    }
}
#[test]
fn concurrent_revision_lock_has_one_winner_and_durable_next_version() {
    let dir = Private::new();
    let vault = Arc::new(dir.vault());
    let d = draft("def run(x): return x\n");
    let v = vault.create_candidate(d.clone()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let vault = vault.clone();
        let v = v.clone();
        let d = d.clone();
        let b = barrier.clone();
        threads.push(std::thread::spawn(move || {
            b.wait();
            vault.revise(&v, d)
        }));
    }
    let values: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(values.iter().filter(|v| v.is_ok()).count(), 1);
    assert_eq!(
        vault
            .view(v.individual_id, v.skill_id)
            .unwrap()
            .version
            .version,
        2
    );
}
fn running_owned(expected_source: &str) -> (String, String) {
    for _ in 0..300 {
        let output = control(&[
            "container",
            "ls",
            "--quiet",
            "--no-trunc",
            "--filter",
            "label=io.alia.sandbox.role=oneshot",
        ]);
        assert!(output.status.success());
        for id in String::from_utf8(output.stdout).unwrap().lines() {
            let inspect = control(&["container", "inspect", id, "--format", "{{json .}}"]);
            if !inspect.status.success() {
                continue;
            }
            let v: Value = serde_json::from_slice(&inspect.stdout).unwrap();
            let name = v["Name"].as_str().unwrap().trim_start_matches('/');
            if v["State"]["Running"] == true
                && v["Config"]["Labels"]["io.alia.sandbox.execution"] == name
                && v["Config"]["Labels"]["io.alia.sandbox.protocol"] == SKILL_PROTOCOL
                && v["Config"]["Image"] == IMAGE
                && v["Config"]["Labels"]["io.alia.sandbox.source"] == expected_source
            {
                return (id.into(), name.into());
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no actual owned skill container became running");
}
fn wait_pending(vault: &SkillVault, v: &SkillVersion) {
    for _ in 0..300 {
        if !vault
            .view(v.individual_id, v.skill_id)
            .unwrap()
            .pending_operations
            .is_empty()
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("invocation never durably started");
}
fn actual_verified(vault: &SkillVault, source: &str) -> (SkillVersion, ValidatedSkill) {
    let v = vault.create_candidate(draft(source)).unwrap();
    let attempt = vault.validate(&v, &cases(&[("base", "0", "0")]));
    record_validation(&format!("verify-{}", v.skill_id), vault, &attempt);
    assert_eq!(attempt.sandbox_calls, 1);
    assert!(attempt.durable);
    let verified = attempt.result.unwrap();
    (v, verified)
}
#[test]
#[ignore = "explicit owned Docker skill protocol and vault tests"]
fn structured_protocol_full_boundaries_and_fixed_argv_are_actual() {
    admission();
    let executor = SandboxExecutor::new(IMAGE, DockerCommand::SudoNoPassword).unwrap();
    let mut source = format!(
        "# QA_BOUNDARY_{}\ndef run(input_json): return input_json\n#",
        Uuid::new_v4().simple()
    );
    source.push_str(&"\u{1}".repeat(24576 - source.len()));
    let input = format!("\"{}\"", "i".repeat(8190));
    assert_eq!(source.len(), 24576);
    assert_eq!(input.len(), 8192);
    record(
        "structured-boundary-source",
        &json!({"source_sha256":sha(source.as_bytes())}),
    );
    let result = executor.execute_skill(&SkillSandboxRequest {
        source: source.clone(),
        input_json: input.clone(),
        timeout_ms: 10000,
    });
    record("structured-boundary", &result);
    assert_eq!(
        result.outcome.status,
        SandboxStatus::Succeeded,
        "{result:?}"
    );
    assert!(result.outcome.isolation_verified && result.outcome.cleanup_passed);
    assert_eq!(
        serde_json::from_str::<Value>(&result.outcome.stdout).unwrap(),
        serde_json::from_str::<Value>(&input).unwrap()
    );
    assert_eq!(result.source_sha256, sha(source.as_bytes()));
    assert_eq!(result.input_sha256, sha(input.as_bytes()));
    assert_eq!(result.wrapper_sha256, sha(SKILL_WRAPPER.as_bytes()));
    let source = "# PRIVATE_SKILL_SOURCE_SENTINEL\ndef run(input_json):\n import os\n return {'argv':open('/proc/self/cmdline','rb').read().decode().split(chr(0))[:-1], 'env':dict(os.environ), 'input':input_json}\n";
    record(
        "structured-argv-source",
        &json!({"source_sha256":sha(source.as_bytes())}),
    );
    let result = executor.execute_skill(&SkillSandboxRequest {
        source: source.into(),
        input_json: "\"PRIVATE_INPUT_SENTINEL\"".into(),
        timeout_ms: 10000,
    });
    record("structured-argv", &result);
    assert_eq!(result.outcome.status, SandboxStatus::Succeeded);
    let value: Value = serde_json::from_str(&result.outcome.stdout).unwrap();
    assert_eq!(
        value["argv"],
        json!(["/usr/local/bin/python", "-I", "-B", "-c", SKILL_WRAPPER])
    );
    assert!(
        !value["argv"]
            .to_string()
            .contains("PRIVATE_SKILL_SOURCE_SENTINEL")
    );
    assert!(!value["argv"].to_string().contains("PRIVATE_INPUT_SENTINEL"));
    assert_eq!(value["env"], json!({"PATH":"/usr/local/bin:/usr/bin:/bin"}));
    for (n, input) in ["{\"a\":1,\"a\":2}", "NaN", "null false", "1e400"]
        .iter()
        .enumerate()
    {
        let result = executor.execute_skill(&SkillSandboxRequest {
            source: source.into(),
            input_json: (*input).into(),
            timeout_ms: 10000,
        });
        record(&format!("structured-invalid-{n}"), &result);
        assert_eq!(result.outcome.status, SandboxStatus::Unavailable);
        assert!(!result.outcome.isolation_verified);
        assert!(result.outcome.cleanup_passed);
    }
}
#[test]
#[ignore = "explicit owned Docker skill protocol and vault tests"]
fn vault_actual_validation_registration_invoke_and_process_recovery() {
    admission();
    let dir = Private::new();
    let vault = dir.vault();
    let d = draft("def run(input_json): return input_json + 1\n");
    let v = vault.create_candidate(d.clone()).unwrap();
    let attempt = vault.validate(&v, &cases(&[("first", "1", "2"), ("second", "2", "3")]));
    record_validation("normal-validation", &vault, &attempt);
    assert_eq!(attempt.sandbox_calls, 2);
    assert_eq!(attempt.attested_executions, 2);
    assert!(attempt.durable);
    let verified = attempt.result.unwrap();
    let old = RegistrationGrant::issue(
        &verified,
        Utc::now() + CDuration::minutes(5),
        SkillScope::SandboxPython,
    )
    .unwrap();
    let refreshed = vault.validate(&v, &cases(&[("new-case-set", "3", "4")]));
    record_validation("normal-new-case-validation", &vault, &refreshed);
    assert_eq!(refreshed.sandbox_calls, 1);
    let verified = refreshed.result.unwrap();
    assert!(matches!(vault.register(old), Err(SkillError::Stale)));
    let grant = RegistrationGrant::issue(
        &verified,
        Utc::now() + CDuration::minutes(5),
        SkillScope::SandboxPython,
    )
    .unwrap();
    record(
        "normal-registration",
        &register(&vault, &verified, Utc::now() + CDuration::minutes(5)),
    );
    child_recover(vault.root(), &v, SkillPhase::Registered);
    let invoked = vault.invoke(&v, "7", 10000);
    record("normal-invoke", &invoked);
    assert_eq!(invoked.result, Ok(json!(8)));
    assert_eq!(invoked.sandbox_calls, 1);
    assert!(invoked.durable);
    let next = vault.revise(&v, d).unwrap();
    assert_eq!(next.version, 2);
    assert!(matches!(vault.register(grant), Err(SkillError::Stale)));
    assert_eq!(vault.invoke(&v, "7", 10000).sandbox_calls, 0);
    assert_eq!(
        vault.invoke(&next, "7", 10000).result,
        Err(SkillError::NotRegistered)
    );
    assert_eq!(
        vault.view(v.individual_id, v.skill_id).unwrap().phase,
        SkillPhase::Candidate
    );
}
#[test]
#[ignore = "explicit owned Docker skill protocol and vault tests"]
fn vault_full_boundary_validation_and_invoke_are_actual() {
    admission();
    let dir = Private::new();
    let vault = dir.vault();
    let mut source = format!(
        "# QA_BOUNDARY_{}\ndef run(input_json): return input_json\n#",
        Uuid::new_v4().simple()
    );
    source.push_str(&"\u{1}".repeat(24576 - source.len()));
    let input = format!("\"{}\"", "v".repeat(8190));
    let v = vault.create_candidate(draft(&source)).unwrap();
    let attempt = vault.validate(&v, &cases(&[("maximum", &input, &input)]));
    record_validation("vault-boundary-validation", &vault, &attempt);
    assert!(attempt.durable);
    let verified = attempt.result.unwrap();
    register(&vault, &verified, Utc::now() + CDuration::minutes(5));
    let result = vault.invoke(&v, &input, 10000);
    record("vault-boundary-invoke", &result);
    assert_eq!(
        result.result,
        Ok(serde_json::from_str::<Value>(&input).unwrap())
    );
    assert_eq!(result.sandbox_calls, 1);
}
#[test]
#[ignore = "explicit owned Docker skill protocol and vault tests"]
fn vault_actual_failures_logs_duplicate_nan_and_case_mismatch_never_verify() {
    admission();
    for (name, source) in [
        ("wrong-case", "def run(x): return False\n"),
        (
            "extra-log",
            "def run(x):\n print('extra log')\n return True\n",
        ),
        (
            "duplicate-json",
            "def run(x):\n import os\n os.write(1,b'{\"a\":1,\"a\":2}')\n os._exit(0)\n",
        ),
        ("nan", "def run(x): return float('nan')\n"),
        (
            "missing-function-early-exit",
            "print('true')\nraise SystemExit(0)\n",
        ),
        (
            "stderr-log",
            "def run(x):\n import sys\n sys.stderr.write('extra log')\n return True\n",
        ),
    ] {
        let dir = Private::new();
        let vault = dir.vault();
        let v = vault.create_candidate(draft(source)).unwrap();
        let attempt = vault.validate(&v, &cases(&[("failure", "null", "true")]));
        record_validation(name, &vault, &attempt);
        assert_eq!(attempt.sandbox_calls, 1);
        assert!(attempt.durable);
        assert_eq!(attempt.result.unwrap_err(), SkillError::VerificationFailed);
        assert_eq!(
            vault.view(v.individual_id, v.skill_id).unwrap().phase,
            SkillPhase::Candidate
        );
        assert_eq!(vault.invoke(&v, "null", 10000).sandbox_calls, 0);
    }
}
fn copy_vault(from: &Path, parent: &Path, label: &str) -> PathBuf {
    let dest = parent.join(label);
    DirBuilder::new().mode(0o700).create(&dest).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let e = entry.unwrap();
        let target = dest.join(e.file_name());
        fs::copy(e.path(), &target).unwrap();
        fs::set_permissions(target, fs::Permissions::from_mode(0o600)).unwrap();
    }
    dest
}
fn reseal(root: &Path, mutate: impl Fn(&mut Value)) {
    let mut names: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("event-")
        })
        .collect();
    names.sort();
    let mut previous = "0".repeat(64);
    for p in names {
        let mut value: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        mutate(&mut value);
        value["previous_sha256"] = previous.into();
        let bytes = serde_json::to_vec(&value).unwrap();
        previous = sha(&bytes);
        fs::write(p, bytes).unwrap();
    }
    let p = root.join(".head");
    let mut head: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    head["last_sha256"] = previous.into();
    fs::write(p, serde_json::to_vec(&head).unwrap()).unwrap();
}
#[test]
#[ignore = "explicit owned Docker skill protocol and vault tests"]
fn vault_real_artifact_rebinding_and_resealed_forgery_are_rejected() {
    admission();
    let dir = Private::new();
    let vault = dir.vault();
    let (v, verified) = actual_verified(&vault, "def run(x): return x\n");
    register(&vault, &verified, Utc::now() + CDuration::minutes(5));
    for mode in [
        "source", "version", "image", "case", "protocol", "wrapper", "unknown",
    ] {
        let clone = copy_vault(vault.root(), &dir.0, mode);
        reseal(&clone, |event| {
            if event["change"]["kind"] == "validation_finished" {
                let r = &mut event["change"]["data"]["report"];
                match mode {
                    "source" => r["version"]["source_sha256"] = "f".repeat(64).into(),
                    "version" => r["version"]["version"] = json!(2),
                    "image" => r["image_digest"] = "python@sha256:wrong".into(),
                    "case" => r["cases"][0]["expected_json"] = "999".into(),
                    "protocol" => r["protocol"] = "python-raw/v1".into(),
                    "wrapper" => r["wrapper_sha256"] = "0".repeat(64).into(),
                    "unknown" => r["passed"] = true.into(),
                    _ => unreachable!(),
                }
            }
        });
        assert!(
            SkillVault::open(&clone, DockerCommand::SudoNoPassword).is_err(),
            "{mode}"
        );
    }
    let mut wrong = v.clone();
    wrong.source_sha256 = "a".repeat(64);
    assert_eq!(vault.invoke(&wrong, "0", 10000).sandbox_calls, 0);
    assert!(
        RegistrationGrant::issue(
            &verified,
            Utc::now() - CDuration::seconds(1),
            SkillScope::SandboxPython
        )
        .is_err()
    );
    child_recover(vault.root(), &v, SkillPhase::Registered);
    record(
        "resealed-forgery-controls",
        &json!({"source_version_image_case_protocol_wrapper_unknown_rejected":true,"original_process_recovery":true}),
    );
}
const WAIT_SOURCE: &str = "def run(input_json):\n import time\n if input_json == 'wait': time.sleep(2)\n return input_json\n";
#[test]
#[ignore = "explicit owned Docker skill protocol and vault tests"]
fn vault_actual_revocation_and_new_version_fence_inflight_results() {
    admission();
    for mode in ["revocation", "version"] {
        let dir = Private::new();
        let vault = Arc::new(dir.vault());
        let (v, verified) = actual_verified(&vault, WAIT_SOURCE);
        register(&vault, &verified, Utc::now() + CDuration::minutes(5));
        let worker = vault.clone();
        let version = v.clone();
        let thread = std::thread::spawn(move || worker.invoke(&version, "\"wait\"", 10000));
        wait_pending(&vault, &v);
        let (id, handle) = running_owned(&v.source_sha256);
        if mode == "revocation" {
            record(
                "revocation-receipt",
                &vault.revoke(&v, "controlled in-flight revocation").unwrap(),
            );
        } else {
            let mut d = draft("def run(x): return x\n");
            d.individual_id = v.individual_id;
            vault.revise(&v, d).unwrap();
        }
        let result = thread.join().unwrap();
        record(&format!("inflight-{mode}"), &result);
        assert_eq!(result.sandbox_calls, 1);
        assert!(result.durable);
        assert!(result.execution.as_ref().unwrap().cleanup_passed);
        assert_eq!(
            result.result,
            Err(if mode == "revocation" {
                SkillError::Revoked
            } else {
                SkillError::Stale
            })
        );
        record(
            &format!("inflight-{mode}-identity"),
            &json!({"actual_id":id,"execution_id":handle,"guarded":true}),
        );
        if mode == "revocation" {
            child_recover(vault.root(), &v, SkillPhase::Revoked);
            assert_eq!(
                vault.invoke(&v, "0", 10000).result,
                Err(SkillError::Revoked)
            );
        } else {
            let latest = vault.view(v.individual_id, v.skill_id).unwrap().version;
            child_recover(vault.root(), &latest, SkillPhase::Candidate);
        }
    }
}
#[test]
#[ignore = "explicit owned Docker skill protocol and vault tests"]
fn vault_actual_expiry_and_tampering_make_inflight_results_unusable() {
    admission();
    for mode in ["expiry", "tamper"] {
        let dir = Private::new();
        let vault = Arc::new(dir.vault());
        let (v, verified) = actual_verified(&vault, WAIT_SOURCE);
        register(
            &vault,
            &verified,
            Utc::now()
                + if mode == "expiry" {
                    CDuration::milliseconds(1300)
                } else {
                    CDuration::minutes(5)
                },
        );
        let worker = vault.clone();
        let version = v.clone();
        let thread = std::thread::spawn(move || worker.invoke(&version, "\"wait\"", 10000));
        wait_pending(&vault, &v);
        running_owned(&v.source_sha256);
        let p = source_file(vault.root(), &v);
        let original = fs::read(&p).unwrap();
        if mode == "tamper" {
            fs::write(&p, "def run(x): return 'changed'\n").unwrap();
        }
        let result = thread.join().unwrap();
        record(&format!("inflight-{mode}"), &result);
        assert_eq!(result.sandbox_calls, 1);
        assert!(result.execution.as_ref().unwrap().cleanup_passed);
        if mode == "expiry" {
            assert_eq!(result.result, Err(SkillError::Expired));
            assert!(result.durable);
            assert_eq!(vault.invoke(&v, "0", 10000).sandbox_calls, 0);
        } else {
            assert!(result.result.is_err());
            assert!(!result.durable);
            fs::write(p, original).unwrap();
            let reopened = SkillVault::open(vault.root(), DockerCommand::SudoNoPassword).unwrap();
            let view = reopened.view(v.individual_id, v.skill_id).unwrap();
            assert_eq!(view.pending_operations.len(), 1);
            assert_eq!(view.pending_sandbox_calls_upper_bound, 1);
            child_recover(vault.root(), &v, SkillPhase::Registered);
        }
    }
}
#[test]
#[ignore = "explicit owned Docker skill protocol and vault tests"]
fn vault_actual_time_output_and_nonzero_cleanup_are_reported() {
    admission();
    let dir = Private::new();
    let vault = dir.vault();
    let source = "def run(x):\n if x == 'loop':\n  while True: pass\n if x == 'output': print('x'*100000)\n if x == 'exit': raise SystemExit(7)\n return x\n";
    let (v, verified) = actual_verified(&vault, source);
    register(&vault, &verified, Utc::now() + CDuration::minutes(5));
    for (name, timeout, status) in [
        ("loop", 2500, SandboxStatus::TimedOut),
        ("output", 10000, SandboxStatus::OutputLimit),
        ("exit", 10000, SandboxStatus::CodeFailed),
    ] {
        let before = Instant::now();
        let result = vault.invoke(&v, &format!("\"{name}\""), timeout);
        record(&format!("limit-{name}"), &result);
        assert!(before.elapsed() < Duration::from_millis(timeout + 500));
        assert_eq!(result.sandbox_calls, 1);
        assert!(result.result.is_err());
        assert_eq!(result.execution.as_ref().unwrap().status, status);
        assert!(result.execution.as_ref().unwrap().cleanup_passed);
        assert!(result.durable);
    }
}
#[test]
#[ignore = "explicit owned Docker skill protocol and vault tests"]
fn vault_actual_cleanup_uncertainty_cannot_validate_or_register() {
    admission();
    let dir = Private::new();
    let vault = Arc::new(dir.vault());
    let v = vault
        .create_candidate(draft(
            "def run(x):\n import time\n time.sleep(2)\n return x\n",
        ))
        .unwrap();
    let worker = vault.clone();
    let version = v.clone();
    let thread =
        std::thread::spawn(move || worker.validate(&version, &cases(&[("cleanup", "0", "0")])));
    wait_pending(&vault, &v);
    let (id, handle) = running_owned(&v.source_sha256);
    let removed = control(&["container", "rm", "--force", &id]);
    assert!(removed.status.success());
    let result = thread.join().unwrap();
    record_validation("cleanup-uncertain-validation", &vault, &result);
    assert_eq!(result.sandbox_calls, 1);
    assert!(result.result.is_err());
    assert!(!result.executions[0].cleanup_passed);
    assert_eq!(
        result.executions[0].status,
        SandboxStatus::InfrastructureFailed
    );
    assert_eq!(result.executions[0].execution_id, handle);
    assert_eq!(
        vault.view(v.individual_id, v.skill_id).unwrap().phase,
        SkillPhase::Candidate
    );
    record(
        "cleanup-fault-control",
        &json!({"actual_id":id,"execution_id":handle,"exact_owned_fault_teardown":true}),
    );
}
