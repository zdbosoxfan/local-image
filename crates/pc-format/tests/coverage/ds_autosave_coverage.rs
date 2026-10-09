use photocraft_format::autosave::RecoveryInfo;
use photocraft_format::{Autosaver, RecoveryEntry, RecoveryStore, discard_recovery, list_recovery, recover};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn make_temp_dir(tag: &str) -> PathBuf {
    static CNT: AtomicU64 = AtomicU64::new(0);
    let n = CNT.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("photocraft-autosave-tests-{}-{}-{}", std::process::id(), tag, n));
    std::fs::create_dir_all(&p).unwrap();
    p
}

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn info(key: &str, saved_at: u64) -> RecoveryInfo {
    RecoveryInfo {
        key: key.to_string(),
        document_name: "test-document".to_string(),
        original_path: Some("/tmp/original.pcraft".to_string()),
        saved_at,
        revision: 1,
    }
}

fn write_sidecar(dir: &Path, key: &str, saved_at: u64) {
    let json = serde_json::to_vec_pretty(&info(key, saved_at)).unwrap();
    std::fs::write(dir.join(format!("{key}.json")), json).unwrap();
}

fn make_entry(dir: &Path, key: &str, saved_at: u64) -> RecoveryEntry {
    write_sidecar(dir, key, saved_at);
    let bundle = dir.join(format!("{key}.pcraft"));
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(bundle.join("manifest.json"), b"{}").unwrap();
    RecoveryEntry { info: info(key, saved_at), bundle }
}

#[test]
fn recovery_info_serde_roundtrip() {
    let original = info("key-123", 1234567890);
    let bytes = serde_json::to_vec(&original).unwrap();
    let decoded: RecoveryInfo = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn list_recovery_empty_dir() {
    let dir = make_temp_dir("empty");
    let _cleanup = Cleanup(dir.clone());
    assert!(list_recovery(&dir).is_empty());
}

#[test]
fn list_recovery_ignores_non_json_files() {
    let dir = make_temp_dir("non_json");
    let _cleanup = Cleanup(dir.clone());
    std::fs::write(dir.join("readme.txt"), b"hello").unwrap();
    assert!(list_recovery(&dir).is_empty());
}

#[test]
fn list_recovery_requires_manifest_file() {
    let dir = make_temp_dir("no_manifest");
    let _cleanup = Cleanup(dir.clone());
    let key = "doc";
    write_sidecar(&dir, key, 100);
    std::fs::create_dir_all(dir.join(format!("{key}.pcraft"))).unwrap();
    assert!(list_recovery(&dir).is_empty());
}

#[test]
fn list_recovery_returns_valid_entry() {
    let dir = make_temp_dir("valid");
    let _cleanup = Cleanup(dir.clone());
    let expected = make_entry(&dir, "doc-1", 111);
    let entries = list_recovery(&dir);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0], expected);
}

#[test]
fn list_recovery_sorts_newest_first() {
    let dir = make_temp_dir("sort");
    let _cleanup = Cleanup(dir.clone());
    make_entry(&dir, "a", 100);
    make_entry(&dir, "b", 300);
    make_entry(&dir, "c", 200);
    let keys: Vec<_> = list_recovery(&dir).into_iter().map(|e| e.info.key).collect();
    assert_eq!(keys, vec!["b", "c", "a"]);
}

#[test]
fn list_recovery_sorts_equal_saved_at_by_key_ascending() {
    let dir = make_temp_dir("sort_equal");
    let _cleanup = Cleanup(dir.clone());
    make_entry(&dir, "b", 100);
    make_entry(&dir, "a", 100);
    make_entry(&dir, "c", 100);
    let keys: Vec<_> = list_recovery(&dir).into_iter().map(|e| e.info.key).collect();
    assert_eq!(keys, vec!["a", "b", "c"]);
}

#[test]
fn list_recovery_rejects_sidecar_with_mismatched_key() {
    let dir = make_temp_dir("mismatch");
    let _cleanup = Cleanup(dir.clone());
    let bad = info("inside", 123);
    let json = serde_json::to_vec(&bad).unwrap();
    std::fs::write(dir.join("outside.json"), json).unwrap();
    std::fs::create_dir_all(dir.join("inside.pcraft")).unwrap();
    std::fs::write(dir.join("inside.pcraft/manifest.json"), b"{}").unwrap();
    assert!(list_recovery(&dir).is_empty());
}

#[test]
fn list_recovery_rejects_malformed_sidecar_json() {
    let dir = make_temp_dir("bad_json");
    let _cleanup = Cleanup(dir.clone());
    std::fs::write(dir.join("a.json"), b"{not json").unwrap();
    assert!(list_recovery(&dir).is_empty());
}

#[test]
fn recover_malformed_bundle_returns_err() {
    let dir = make_temp_dir("recover_err");
    let _cleanup = Cleanup(dir.clone());
    let entry = make_entry(&dir, "bad", 10);
    std::fs::write(entry.bundle.join("manifest.json"), b"not a valid manifest").unwrap();
    let result = recover(&entry);
    assert!(result.is_err());
}

#[test]
fn discard_recovery_removes_bundle_and_sidecar() {
    let dir = make_temp_dir("discard_remove");
    let _cleanup = Cleanup(dir.clone());
    let entry = make_entry(&dir, "del", 10);
    let sidecar = dir.join("del.json");
    assert!(entry.bundle.exists());
    assert!(sidecar.exists());
    let res = discard_recovery(&dir, &entry);
    assert!(res.is_ok());
    assert!(!entry.bundle.exists());
    assert!(!sidecar.exists());
}

#[test]
fn discard_recovery_missing_entry_is_ok() {
    let dir = make_temp_dir("discard_missing");
    let _cleanup = Cleanup(dir.clone());
    let entry = RecoveryEntry { info: info("ghost", 10), bundle: dir.join("ghost.pcraft") };
    let res = discard_recovery(&dir, &entry);
    assert!(res.is_ok());
    assert!(!entry.bundle.exists());
}

#[test]
fn autosaver_bundle_path_is_sanitized() {
    let dir = make_temp_dir("autosaver_path");
    let _cleanup = Cleanup(dir.clone());
    let saver = Autosaver::new(&dir, "a/b c*");
    assert_eq!(saver.bundle_path(), dir.join("a_b_c_.pcraft"));
    drop(saver);
}

#[test]
fn autosaver_flush_without_requests_returns_none() {
    let dir = make_temp_dir("autosaver_flush_empty");
    let _cleanup = Cleanup(dir.clone());
    let saver = Autosaver::new(&dir, "empty");
    let result = saver.flush();
    assert!(result.is_none());
}

#[test]
fn recovery_store_discard_without_savers_is_ok() {
    let dir = make_temp_dir("store_empty");
    let _cleanup = Cleanup(dir.clone());
    let mut store = RecoveryStore::new(&dir);
    let res = store.discard(42);
    assert!(res.is_ok());
}

#[test]
fn recovery_store_adopt_and_discard_removes_adopted_entry() {
    let dir = make_temp_dir("store_adopt");
    let _cleanup = Cleanup(dir.clone());
    let key = "adopted-key";
    make_entry(&dir, key, 10);
    let mut store = RecoveryStore::new(&dir);
    store.adopt(7, key);
    let res = store.discard(7);
    assert!(res.is_ok());
    assert!(!dir.join(format!("{key}.pcraft")).exists());
    assert!(!dir.join(format!("{key}.json")).exists());
}

#[test]
fn recovery_store_recover_skips_invalid_bundles() {
    let dir = make_temp_dir("store_recover_invalid");
    let _cleanup = Cleanup(dir.clone());
    let entry = make_entry(&dir, "bad", 10);
    std::fs::write(entry.bundle.join("manifest.json"), b"not json").unwrap();
    let store = RecoveryStore::new(&dir);
    assert!(store.recover().is_empty());
}
