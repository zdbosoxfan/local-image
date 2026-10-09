use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use lightcraft_catalog::journal::{Journal, LOG, SNAPSHOT, SnapshotPolicy, VERSION, decode_record, encode_record};
use lightcraft_catalog::store::{FsStore, Store};
use lightcraft_catalog::{Catalog, CatalogError, Op};

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

fn snapshot_bytes_with_version(seq: u64, version: u32, catalog: &Catalog) -> Vec<u8> {
    format!("{{\"format\":\"lightcraft-catalog\",\"version\":{version},\"seq\":{seq},\"catalog\":{}}}\n", catalog.to_snapshot()).into_bytes()
}

fn snapshot_bytes(seq: u64, catalog: &Catalog) -> Vec<u8> {
    snapshot_bytes_with_version(seq, VERSION, catalog)
}

fn set_browsed(folder: &str) -> Op {
    Op::SetBrowsed { folder: folder.to_string(), at: Some("2024-01-01T12:00:00".to_string()) }
}

#[test]
fn encode_record_roundtrip_set_browsed() {
    let op = set_browsed("test");
    let line = encode_record(42, &op);
    let (seq, decoded) = decode_record(&line).expect("valid record");
    assert_eq!(seq, 42);
    assert_eq!(decoded, op);
}

#[test]
fn encode_record_does_not_contain_newline_and_is_deterministic() {
    let op = set_browsed("test");
    let a = encode_record(7, &op);
    let b = encode_record(7, &op);
    assert_eq!(a, b);
    assert!(!a.contains('\n'));
    assert!(a.starts_with("{\"seq\":7,"));
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
fn journal_open_new_library_marks_created_and_writes_snapshot() {
    let dir = new_test_dir();
    let store = new_store(&dir.0);
    let (j, catalog, report) = Journal::open(store).unwrap();
    assert!(report.created);
    assert_eq!(j.seq(), 0);
    assert_eq!(j.snapshot_seq(), 0);
    assert!(catalog.is_empty());
    assert!(dir.0.join(SNAPSHOT).exists());
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
fn journal_open_replays_appended_ops() {
    let dir = new_test_dir();
    let store = new_store(&dir.0);
    let (mut j, mut catalog, _) = Journal::open(store).unwrap();
    let op = set_browsed("replay");
    catalog.apply(op.clone()).unwrap();
    j.append(&[op]).unwrap();
    drop(j);

    let store = new_store(&dir.0);
    let (j2, _, report) = Journal::open(store).unwrap();
    assert_eq!(report.replayed, 1);
    assert_eq!(j2.seq(), 1);
    assert_eq!(j2.log_records(), 1);
}

#[test]
fn journal_snapshot_compacts_log_and_reopens_cleanly() {
    let dir = new_test_dir();
    let store = new_store(&dir.0);
    let (mut j, mut catalog, _) = Journal::open(store).unwrap();
    let op = set_browsed("compact");
    catalog.apply(op.clone()).unwrap();
    j.append(&[op]).unwrap();
    j.snapshot(&catalog).unwrap();
    assert_eq!(j.seq(), 1);
    assert_eq!(j.snapshot_seq(), 1);
    assert_eq!(j.log_records(), 0);
    assert_eq!(j.log_bytes(), 0);
    drop(j);

    let store = new_store(&dir.0);
    let (j2, _, report) = Journal::open(store).unwrap();
    assert_eq!(j2.seq(), 1);
    assert_eq!(j2.snapshot_seq(), 1);
    assert_eq!(report.replayed, 0);
}

#[test]
fn journal_snapshot_with_unlogged_includes_unlogged_seq() {
    let dir = new_test_dir();
    let store = new_store(&dir.0);
    let (mut j, mut catalog, _) = Journal::open(store).unwrap();

    let op1 = set_browsed("logged");
    catalog.apply(op1.clone()).unwrap();
    j.append(&[op1]).unwrap();

    let op2 = set_browsed("unlogged");
    catalog.apply(op2.clone()).unwrap();
    j.snapshot_with_unlogged(&catalog, 1).unwrap();

    assert_eq!(j.seq(), 2);
    assert_eq!(j.snapshot_seq(), 2);
    assert_eq!(j.log_records(), 0);
    drop(j);

    let store = new_store(&dir.0);
    let (j2, _, report) = Journal::open(store).unwrap();
    assert_eq!(j2.seq(), 2);
    assert_eq!(j2.snapshot_seq(), 2);
    assert_eq!(report.replayed, 0);
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
fn journal_open_handles_torn_tail() {
    let dir = new_test_dir();
    let catalog = Catalog::new();
    std::fs::write(dir.0.join(SNAPSHOT), snapshot_bytes(0, &catalog).as_slice()).unwrap();

    let record = encode_record(1, &set_browsed("torn"));
    let mut log = record.into_bytes();
    log.push(b'\n');
    log.extend_from_slice(b"{\"seq\":2,\"crc\":1,\"op\":{\"op\":\"setBrowsed\"");
    std::fs::write(dir.0.join(LOG), &log).unwrap();

    let (j, _, report) = Journal::open(new_store(&dir.0)).unwrap();
    assert_eq!(report.replayed, 1);
    assert_eq!(j.seq(), 1);
    assert!(report.torn_bytes > 0);
}

#[test]
fn journal_open_preserves_damaged_log_and_recovers_to_last_good() {
    let dir = new_test_dir();
    let catalog = Catalog::new();
    std::fs::write(dir.0.join(SNAPSHOT), snapshot_bytes(0, &catalog).as_slice()).unwrap();

    let good1 = encode_record(1, &set_browsed("good1"));
    let good2 = encode_record(2, &set_browsed("good2"));
    let mut log = good1.into_bytes();
    log.push(b'\n');
    log.extend_from_slice(b"garbage-not-a-record\n");
    log.extend(good2.into_bytes());
    log.push(b'\n');
    std::fs::write(dir.0.join(LOG), &log).unwrap();

    let (j, _, report) = Journal::open(new_store(&dir.0)).unwrap();
    assert_eq!(report.replayed, 1);
    assert_eq!(j.seq(), 1);
    let damaged = report.damaged.expect("damaged log preserved");
    assert!(dir.0.join(&damaged).exists());
}

#[test]
fn journal_open_refuses_newer_snapshot_without_modifying_it() {
    let dir = new_test_dir();
    let newer = b"{\"format\":\"lightcraft-catalog\",\"version\":3}";
    std::fs::write(dir.0.join(SNAPSHOT), newer).unwrap();

    match Journal::open(new_store(&dir.0)) {
        Err(CatalogError::Newer(_)) => {}
        Err(other) => panic!("expected CatalogError::Newer, got {other}"),
        Ok(_) => panic!("expected Journal::open to fail for newer snapshot"),
    }
    let after = std::fs::read(dir.0.join(SNAPSHOT)).unwrap();
    assert_eq!(after, newer);
}

#[test]
fn journal_open_upgrades_older_snapshot_in_place() {
    let dir = new_test_dir();
    let catalog = Catalog::new();
    std::fs::write(dir.0.join(SNAPSHOT), snapshot_bytes_with_version(0, 1, &catalog).as_slice()).unwrap();

    let (j, _, report) = Journal::open(new_store(&dir.0)).unwrap();
    assert_eq!(report.upgraded_from, Some(1));
    assert_eq!(j.snapshot_seq(), 0);

    let rewritten = std::fs::read(dir.0.join(SNAPSHOT)).unwrap();
    assert!(String::from_utf8_lossy(&rewritten).contains("\"version\":2"));
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
fn journal_background_snapshot_compacts_like_normal_snapshot() {
    let dir = new_test_dir();
    let (mut j, mut catalog, _) = Journal::open(new_store(&dir.0)).unwrap();
    let op = set_browsed("bg");
    catalog.apply(op.clone()).unwrap();
    j.append(&[op]).unwrap();
    j.snapshot_in_background(&catalog).unwrap();
    j.wait().unwrap();
    assert_eq!(j.snapshot_seq(), 1);
    assert_eq!(j.seq(), 1);
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
fn journal_wait_without_pending_is_ok() {
    let dir = new_test_dir();
    let (mut j, _, _) = Journal::open(new_store(&dir.0)).unwrap();
    assert!(j.wait().is_ok());
}

#[test]
fn journal_describe_returns_nonempty_string() {
    let dir = new_test_dir();
    let (j, _, _) = Journal::open(new_store(&dir.0)).unwrap();
    assert!(!j.describe().is_empty());
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
