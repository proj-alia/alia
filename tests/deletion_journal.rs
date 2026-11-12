//! Actual private temporary files and independent file descriptions/processes; no production paths.
use alia::adapters::deletion_journal::{DeletionJournal, JournalError, JournalIntent};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    path::PathBuf,
    process::Command,
    sync::Arc,
};
use uuid::Uuid;
struct Temp {
    dir: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("alia-p05-journal-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        Self { dir }
    }
    fn path(&self) -> PathBuf {
        self.dir.join("deletions.jsonl")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700));
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn intent() -> JournalIntent {
    JournalIntent {
        operation_id: Uuid::new_v4(),
        individual_id: Uuid::new_v4(),
        target_memory_id: Uuid::new_v4(),
        expected_memory_version: 3,
        idempotency_key_digest: None,
        request_digest: "a".repeat(64),
        root_event_ids: vec![Uuid::new_v4(), Uuid::new_v4()],
    }
}
#[test]
fn restore_open_never_creates_missing_or_replaced_journal() {
    let temp = Temp::new();
    let path = temp.path();
    assert!(DeletionJournal::open_existing(&path).is_err());
    assert!(!path.exists());
    DeletionJournal::open(&path)
        .unwrap()
        .append(&intent())
        .unwrap();
    assert_eq!(
        DeletionJournal::open_existing(&path)
            .unwrap()
            .records()
            .unwrap()
            .len(),
        1
    );
    fs::remove_file(&path).unwrap();
    assert!(DeletionJournal::open_existing(&path).is_err());
    assert!(!path.exists());
    symlink(temp.dir.join("absent-target"), &path).unwrap();
    assert!(DeletionJournal::open_existing(&path).is_err());
    assert!(!temp.dir.join("absent-target").exists());
}
fn canonical_digest(value: &Value) -> String {
    let mut v = value.clone();
    v.as_object_mut().unwrap().remove("record_digest");
    format!("{:x}", Sha256::digest(serde_json::to_vec(&v).unwrap()))
}
fn write_records(path: &std::path::Path, values: &[Value]) {
    let mut bytes = Vec::new();
    for v in values {
        bytes.extend(serde_json::to_vec(v).unwrap());
        bytes.push(b'\n');
    }
    fs::write(path, bytes).unwrap();
}
#[test]
fn journal_real_persistence_reopen_exact_replay_conflict_and_canonical_chain() {
    let temp = Temp::new();
    let path = temp.path();
    let a = intent();
    let b = intent();
    let journal = DeletionJournal::open(&path).unwrap();
    assert!(journal.records().unwrap().is_empty());
    let first = journal.append(&a).unwrap();
    assert_eq!(first.sequence, 1);
    let second = journal.append(&b).unwrap();
    assert_eq!(second.sequence, 2);
    assert_eq!(journal.append(&a).unwrap(), first);
    let mut changed = a.clone();
    changed.expected_memory_version += 1;
    assert_eq!(journal.append(&changed), Err(JournalError::Conflict));
    drop(journal);
    let reopened = DeletionJournal::open(&path).unwrap();
    assert!(reopened.records().unwrap() == vec![a, b]);
    let bytes = fs::read(&path).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let values: Vec<Value> = text
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(values.len(), 2);
    assert!(values[0]["previous_digest"].is_null());
    assert_eq!(values[1]["previous_digest"], values[0]["record_digest"]);
    for v in values {
        assert_eq!(v["record_digest"], canonical_digest(&v));
        assert_eq!(v.as_object().unwrap().len(), 5);
        assert_eq!(v["intent"].as_object().unwrap().len(), 6);
    }
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
        0o600
    );
    assert_eq!(
        fs::metadata(&temp.dir).unwrap().permissions().mode() & 0o7777,
        0o700
    );
    for forbidden in ["user-key", "password", "原文", "body", "text", "secret"] {
        assert!(!text.contains(forbidden));
    }
}
#[test]
fn journal_two_independent_handles_and_shared_handle_threads_no_lost_updates() {
    let temp = Temp::new();
    let path = temp.path();
    let a = Arc::new(DeletionJournal::open(&path).unwrap());
    let b = Arc::new(DeletionJournal::open(&path).unwrap());
    let mut handles = vec![];
    for client in [a.clone(), a, b.clone(), b] {
        handles.push(std::thread::spawn(move || {
            for _ in 0..12 {
                client.append(&intent()).unwrap();
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    let j = DeletionJournal::open(&path).unwrap();
    assert_eq!(j.records().unwrap().len(), 48);
}
#[test]
fn journal_process_lock_roundtrip() {
    const FLAG: &str = "ALIA_P05_JOURNAL_CHILD";
    if let Some(path) = std::env::var_os(FLAG) {
        let journal = DeletionJournal::open(std::path::Path::new(&path)).unwrap();
        for _ in 0..8 {
            journal.append(&intent()).unwrap();
        }
        return;
    }
    let temp = Temp::new();
    let path = temp.path();
    DeletionJournal::open(&path).unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut children = vec![];
    for _ in 0..2 {
        children.push(
            Command::new(&exe)
                .arg("--exact")
                .arg("journal_process_lock_roundtrip")
                .arg("--nocapture")
                .env(FLAG, &path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("journal child exceeded bounded deadline");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    assert_eq!(
        DeletionJournal::open(&path)
            .unwrap()
            .records()
            .unwrap()
            .len(),
        16
    );
}
#[test]
fn journal_truncated_tail_corruption_and_duplicate_operations_fail_closed() {
    let temp = Temp::new();
    let path = temp.path();
    let j = DeletionJournal::open(&path).unwrap();
    j.append(&intent()).unwrap();
    j.append(&intent()).unwrap();
    let good = fs::read(&path).unwrap();
    for truncated in [good.len() - 1, good.len() - 10] {
        fs::write(&path, &good[..truncated]).unwrap();
        assert!(matches!(j.records(), Err(JournalError::Corrupt)));
        assert_eq!(j.append(&intent()), Err(JournalError::Corrupt));
        assert_eq!(fs::read(&path).unwrap(), good[..truncated]);
        assert!(matches!(
            DeletionJournal::open(&path),
            Err(JournalError::Corrupt)
        ));
    }
    fs::write(&path, &good).unwrap();
    let original: Vec<Value> = std::str::from_utf8(&good)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    for mutation in 0..5 {
        let mut v = original.clone();
        match mutation {
            0 => v[1]["previous_digest"] = json!("b".repeat(64)),
            1 => v[1]["sequence"] = json!(9),
            2 => v[1]["record_digest"] = json!("0".repeat(64)),
            3 => v[1]["format"] = json!("alia-deletion-journal/9"),
            _ => v[1]["intent"] = v[0]["intent"].clone(),
        };
        if mutation != 2 {
            v[1]["record_digest"] = json!(canonical_digest(&v[1]));
        }
        write_records(&path, &v);
        assert!(matches!(j.records(), Err(JournalError::Corrupt)));
        assert_eq!(j.append(&intent()), Err(JournalError::Corrupt));
    }
}
#[test]
fn journal_unknown_duplicate_and_noncanonical_values_are_rejected() {
    let temp = Temp::new();
    let path = temp.path();
    let j = DeletionJournal::open(&path).unwrap();
    let mut i = intent();
    i.operation_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".parse().unwrap();
    j.append(&i).unwrap();
    let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for mutation in 0..6 {
        let mut v = original.clone();
        match mutation {
            0 => {
                v["unknown"] = json!("must refuse");
            }
            1 => {
                v["intent"]["body"] = json!("SENSITIVE-TEST-TEXT");
            }
            2 => v["intent"]["operation_id"] = json!(i.operation_id.to_string().to_uppercase()),
            3 => v["intent"]["expected_memory_version"] = json!(3.0),
            4 => v["intent"]["request_digest"] = json!("A".repeat(64)),
            _ => v["intent"]["root_event_ids"] = json!([i.root_event_ids[0], i.root_event_ids[0]]),
        };
        v["record_digest"] = json!(canonical_digest(&v));
        write_records(&path, &[v]);
        let e = j.records().err().unwrap();
        assert_eq!(e, JournalError::Corrupt);
        assert!(!format!("{e:?} {e}").contains("SENSITIVE"));
    }
    let text = serde_json::to_string(&original).unwrap();
    for duplicated in [
        text.replacen("\"sequence\":1", "\"sequence\":1,\"sequence\":1", 1),
        text.replacen(
            "\"expected_memory_version\":3",
            "\"expected_memory_version\":3,\"expected_memory_version\":3",
            1,
        ),
    ] {
        assert_ne!(duplicated, text);
        fs::write(&path, format!("{duplicated}\n")).unwrap();
        assert_eq!(j.records().err(), Some(JournalError::Corrupt));
    }
}
#[test]
fn journal_invalid_intents_never_write_and_size_bounds_are_explicit() {
    let temp = Temp::new();
    let path = temp.path();
    let j = DeletionJournal::open(&path).unwrap();
    let valid = intent();
    for mutation in 0..7 {
        let mut v = valid.clone();
        match mutation {
            0 => v.operation_id = Uuid::nil(),
            1 => v.individual_id = Uuid::nil(),
            2 => v.target_memory_id = Uuid::nil(),
            3 => v.expected_memory_version = 0,
            4 => v.request_digest = "bad".into(),
            5 => v.root_event_ids.clear(),
            _ => v.root_event_ids = vec![Uuid::new_v4(); 257],
        };
        assert_eq!(j.append(&v), Err(JournalError::InvalidIntent));
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
    }
    fs::write(&path, vec![b' '; 65536]).unwrap();
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(b"\n").unwrap();
    assert_eq!(j.records().err(), Some(JournalError::TooLarge));
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    assert_eq!(j.records().err(), Some(JournalError::TooLarge));
    assert_eq!(j.append(&valid), Err(JournalError::TooLarge));
}
#[test]
fn journal_symlink_nonregular_permissions_hardlink_and_replacement_refuse_confirmation() {
    let temp = Temp::new();
    let path = temp.path();
    let j = DeletionJournal::open(&path).unwrap();
    j.append(&intent()).unwrap();
    let good = fs::read(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        DeletionJournal::open(&path),
        Err(JournalError::UnsafePath)
    ));
    assert_eq!(j.append(&intent()), Err(JournalError::UnsafePath));
    assert_eq!(fs::read(&path).unwrap(), good);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let link = temp.dir.join("link");
    symlink(&path, &link).unwrap();
    assert!(matches!(
        DeletionJournal::open(&link),
        Err(JournalError::UnsafePath)
    ));
    fs::remove_file(&link).unwrap();
    fs::hard_link(&path, &link).unwrap();
    assert!(matches!(
        DeletionJournal::open(&path),
        Err(JournalError::UnsafePath)
    ));
    fs::remove_file(&link).unwrap();
    let moved = temp.dir.join("old");
    fs::rename(&path, &moved).unwrap();
    symlink(&moved, &path).unwrap();
    assert_eq!(j.append(&intent()), Err(JournalError::UnsafePath));
    assert_eq!(fs::read(&moved).unwrap(), good);
    fs::remove_file(&path).unwrap();
    fs::rename(&moved, &path).unwrap();
    let directory = temp.dir.join("directory");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    assert!(matches!(
        DeletionJournal::open(&directory),
        Err(JournalError::UnsafePath)
    ));
    let parent_link = temp.dir.join("parent-link");
    symlink(&directory, &parent_link).unwrap();
    assert!(matches!(
        DeletionJournal::open(&parent_link.join("new")),
        Err(JournalError::UnsafePath)
    ));
    fs::set_permissions(&temp.dir, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        DeletionJournal::open(&path),
        Err(JournalError::UnsafePath)
    ));
    assert_eq!(j.append(&intent()), Err(JournalError::UnsafePath));
}

#[test]
fn journal_write_failure_is_not_acknowledged() {
    const FLAG: &str = "ALIA_P05_JOURNAL_LIMIT_CHILD";
    if let Some(path) = std::env::var_os(FLAG) {
        let journal = DeletionJournal::open(std::path::Path::new(&path)).unwrap();
        let mut value = intent();
        value.root_event_ids = (0..64).map(|_| Uuid::new_v4()).collect();
        assert_eq!(journal.append(&value), Err(JournalError::Io));
        assert_eq!(journal.records().err(), Some(JournalError::Corrupt));
        return;
    }
    let temp = Temp::new();
    let path = temp.path();
    DeletionJournal::open(&path).unwrap();
    // Only this subprocess receives RLIMIT_FSIZE. Ignore XFSZ so write_all returns EFBIG
    // after a real partial file write; no unsafe signal/resource syscall in project code.
    let mut child=Command::new("/bin/bash").args(["-c","trap '' XFSZ; ulimit -f 1; exec \"$1\" --exact journal_write_failure_is_not_acknowledged --nocapture","alia-p05-journal-failure"]).arg(std::env::current_exe().unwrap()).env(FLAG,&path).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("limited journal child exceeded deadline");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let length = fs::metadata(&path).unwrap().len();
    assert!(length > 0 && length <= 1024);
    assert!(matches!(
        DeletionJournal::open(&path),
        Err(JournalError::Corrupt)
    ));
}

#[test]
fn journal_v2_idempotency_metadata_preserves_legacy_v1_chain() {
    let temp = Temp::new();
    let path = temp.path();
    let journal = DeletionJournal::open(&path).unwrap();
    let old = intent();
    journal.append(&old).unwrap();
    let legacy_line = fs::read(&path).unwrap();
    let mut new = intent();
    new.idempotency_key_digest = Some("b".repeat(64));
    let receipt = journal.append(&new).unwrap();
    assert_eq!(receipt.sequence, 2);
    assert!(fs::read(&path).unwrap().starts_with(&legacy_line));
    drop(journal);
    let reopened = DeletionJournal::open(&path).unwrap();
    let records = reopened.records().unwrap();
    assert!(records[0].idempotency_key_digest.is_none());
    assert_eq!(
        records[1].idempotency_key_digest.as_deref(),
        Some("b".repeat(64).as_str())
    );
    assert_eq!(reopened.append(&new).unwrap(), receipt);
    let values: Vec<Value> = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect();
    assert_eq!(values[0]["format"], "alia-deletion-journal/1");
    assert_eq!(values[0]["intent"].as_object().unwrap().len(), 6);
    assert_eq!(values[1]["format"], "alia-deletion-journal/2");
    assert_eq!(values[1]["intent"].as_object().unwrap().len(), 7);
    for (version, metadata) in [
        ("alia-deletion-journal/1", json!("b".repeat(64))),
        ("alia-deletion-journal/2", Value::Null),
        ("alia-deletion-journal/3", json!("b".repeat(64))),
    ] {
        let mut bad = values[1].clone();
        bad["format"] = json!(version);
        bad["intent"]["idempotency_key_digest"] = metadata;
        bad["record_digest"] = json!(canonical_digest(&bad));
        write_records(&path, &[values[0].clone(), bad]);
        assert_eq!(reopened.records().err(), Some(JournalError::Corrupt));
    }
}
