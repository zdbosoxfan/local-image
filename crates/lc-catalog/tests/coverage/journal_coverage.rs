use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use lightcraft_catalog::Op;
use lightcraft_catalog::journal::{Journal, SNAPSHOT, SnapshotPolicy, decode_record};
use lightcraft_catalog::store::{FsStore, Store};

static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestDir(std::path::PathBuf);

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn new_test_dir() -> TestDir {
    let n = DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("lc-catalog-journal-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    TestDir(path)
}

fn new_store(dir: &Path) -> Box<dyn Store> {
    Box::new(FsStore::open(dir).unwrap())
}

fn set_browsed(folder: &str) -> Op {
    Op::SetBrowsed { folder: folder.to_string(), at: Some("2024-01-01T12:00:00".to_string()) }
}

#[test]
fn decode_record_rejects_malformed_input() {
    assert_eq!(decode_record("not-json"), None);
    assert_eq!(decode_record("{\"seq\":1,\"crc\":1,\"op\":{}"), None);
    assert_eq!(decode_record("{\"seq\":\"x\",\"crc\":1,\"op\":{}}"), None);
    assert_eq!(decode_record("{\"seq\":1,\"crc\":1,\"op\":{}}"), None);
    assert_eq!(decode_record("{\"seq\":1,\"crc\":1,\"op\":{\"op\":\"setBrowsed\",\"folder\":\"x\",\"at\":null}}"), None);
}

#[test]
fn journal_append_increments_seq_and_log() {
    let dir = new_test_dir();
    let store = new_store(&dir.0);
    let (mut j, _, _) = Journal::open(store).unwrap();
    let op = set_browsed("append");
    j.append(&[op]).unwrap();
    assert_eq!(j.seq(), 1);
    assert_eq!(j.log_records(), 1);
    assert!(j.log_bytes() > 0);
}

#[test]
fn journal_wants_snapshot_uses_policy_thresholds() {
    let dir = new_test_dir();
    let store = new_store(&dir.0);
    let (mut j, _, _) = Journal::open(store).unwrap();
    j.policy = SnapshotPolicy { max_records: 2, max_bytes: u64::MAX };
    assert!(!j.wants_snapshot());
    let op = set_browsed("threshold");
    j.append(&[op.clone(), op]).unwrap();
    assert!(j.wants_snapshot());
}

#[test]
fn journal_stats_tracks_appends_and_snapshots() {
    let dir = new_test_dir();
    let (mut j, mut catalog, _) = Journal::open(new_store(&dir.0)).unwrap();
    let op = set_browsed("stats");
    catalog.apply(op.clone()).unwrap();
    j.append(&[op]).unwrap();

    let stats = j.stats();
    assert_eq!(stats.appends, 1);
    assert!(stats.last_append_ms >= 0.0);
    assert_eq!(stats.snapshots, 0);

    j.snapshot(&catalog).unwrap();
    let stats = j.stats();
    assert_eq!(stats.snapshots, 1);
    assert_eq!(j.log_records(), 0);
}

#[test]
fn journal_poll_without_pending_returns_false() {
    let dir = new_test_dir();
    let (mut j, _, _) = Journal::open(new_store(&dir.0)).unwrap();
    assert!(!j.snapshot_running());
    assert!(!j.snapshot_written());
    assert!(!j.poll().unwrap());
}

#[test]
fn journal_persists_empty_snapshot_for_new_library_with_zero_seq() {
    let dir = new_test_dir();
    let (_, catalog, report) = Journal::open(new_store(&dir.0)).unwrap();
    let snapshot = std::fs::read_to_string(dir.0.join(SNAPSHOT)).unwrap();
    assert!(snapshot.contains("\"seq\":0"));
    assert!(snapshot.contains(catalog.to_snapshot().as_str()));
    assert!(report.created);
}
