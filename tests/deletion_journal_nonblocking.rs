//! Real file-lock contention; private test files only, no production journal.
use alia::adapters::deletion_journal::{DeletionJournal, JournalError, JournalIntent};
use std::{
    fs::{self, OpenOptions},
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

struct PrivateTemp(PathBuf);
impl PrivateTemp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("alia-root-journal-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn journal(&self) -> PathBuf {
        self.0.join("deletions.jsonl")
    }
}
impl Drop for PrivateTemp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn intent() -> JournalIntent {
    JournalIntent {
        operation_id: Uuid::new_v4(),
        individual_id: Uuid::new_v4(),
        target_memory_id: Uuid::new_v4(),
        expected_memory_version: 1,
        request_digest: "a".repeat(64),
        root_event_ids: vec![Uuid::new_v4()],
        idempotency_key_digest: None,
    }
}

#[test]
fn independent_held_file_lock_rejects_all_try_operations_without_writing() {
    let temp = PrivateTemp::new();
    let path = temp.journal();
    let journal = DeletionJournal::open(&path).unwrap();
    let external = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    external.lock().unwrap();
    let record = intent();
    let before = fs::read(&path).unwrap();
    let start = Instant::now();
    assert!(matches!(journal.try_records(), Err(JournalError::Lock)));
    assert_eq!(journal.try_append(&record), Err(JournalError::Lock));
    assert!(matches!(
        DeletionJournal::try_open_existing(&path),
        Err(JournalError::Lock)
    ));
    assert!(start.elapsed() < Duration::from_millis(500));
    assert_eq!(fs::read(&path).unwrap(), before);
    external.unlock().unwrap();
    let saved = journal.try_append(&record).unwrap();
    assert_eq!(saved.sequence, 1);
    assert_eq!(journal.try_append(&record).unwrap(), saved);
    let reopened = DeletionJournal::try_open_existing(&path).unwrap();
    assert!(reopened.try_records().unwrap() == vec![record]);
}

#[test]
fn nonblocking_open_never_creates_a_missing_journal_and_preserves_corruption() {
    let temp = PrivateTemp::new();
    let path = temp.journal();
    assert!(DeletionJournal::try_open_existing(&path).is_err());
    assert!(!path.exists());
    let journal = DeletionJournal::open(&path).unwrap();
    journal.try_append(&intent()).unwrap();
    let invalid = b"{\"format\":\"corrupt-test-record\"}\n";
    fs::write(&path, invalid).unwrap();
    assert!(matches!(journal.try_records(), Err(JournalError::Corrupt)));
    assert!(matches!(
        DeletionJournal::try_open_existing(&path),
        Err(JournalError::Corrupt)
    ));
    assert_eq!(fs::read(&path).unwrap(), invalid);
}
