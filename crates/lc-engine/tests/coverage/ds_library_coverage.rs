use lightcraft_engine::catalog::FsStore;
use lightcraft_engine::library::LibraryStores;
use lightcraft_engine::{EngineError, Session};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("lc_engine_library_{}_{}_{}", label, std::process::id(), n));
        std::fs::create_dir_all(&path).unwrap();
        TestDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn open_close_creates_library() {
    let dir = TestDir::new("open_close");
    let mut s = Session::new();
    let _report = s.open_library(dir.path(), false).unwrap();
    assert!(s.library.is_some());
    let lib = s.library.as_ref().unwrap();
    assert!(lib.on_disk);
    assert_eq!(lib.dir, dir.path().to_path_buf());
    assert_eq!(lib.thumbs_dir(), dir.path().join("thumbs"));
    assert_eq!(lib.originals_dir(), dir.path().join("Originals"));
    s.close_library().unwrap();
    assert!(std::fs::read_dir(dir.path()).unwrap().count() > 0);
}

#[test]
fn open_library_twice_reuses_lock() {
    let dir = TestDir::new("twice");
    let mut s = Session::new();
    s.open_library(dir.path(), false).unwrap();
    s.open_library(dir.path(), false).unwrap();
    s.close_library().unwrap();
}

#[test]
fn library_lock_conflict() {
    let dir = TestDir::new("conflict");
    let mut s1 = Session::new();
    s1.open_library(dir.path(), false).unwrap();

    let mut s2 = Session::new();
    let result = s2.open_library(dir.path(), false);
    assert!(matches!(result, Err(EngineError::LibraryInUse(_))));

    s1.close_library().unwrap();
    drop(s1);

    let mut s2 = Session::new();
    s2.open_library(dir.path(), false).unwrap();
    s2.close_library().unwrap();
}

#[test]
fn reopen_after_close() {
    let dir = TestDir::new("reopen");
    let mut s = Session::new();
    s.open_library(dir.path(), false).unwrap();
    s.close_library().unwrap();
    drop(s);

    let mut s2 = Session::new();
    s2.open_library(dir.path(), false).unwrap();
    assert!(s2.library.is_some());
    s2.close_library().unwrap();
}

#[test]
fn prefs_round_trip() {
    let dir = TestDir::new("prefs");
    let mut s = Session::new();
    s.open_library(dir.path(), false).unwrap();

    s.cache_mb = 7;
    s.forget_local_days = 14;
    s.recent_keywords = vec!["a".to_string(), "b".to_string()];
    s.smart_previews_dir = Some(dir.path().join("custom_sp"));
    s.save_prefs().unwrap();
    s.close_library().unwrap();
    drop(s);

    let mut s2 = Session::new();
    s2.open_library(dir.path(), false).unwrap();
    assert_eq!(s2.cache_mb, 7);
    assert_eq!(s2.forget_local_days, 14);
    assert_eq!(s2.recent_keywords, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(s2.smart_previews_dir, Some(dir.path().join("custom_sp")));
    s2.close_library().unwrap();
}

#[test]
fn set_cache_mb_updates_and_round_trips() {
    let dir = TestDir::new("cache_mb");
    let mut s = Session::new();
    s.open_library(dir.path(), false).unwrap();

    s.set_cache_mb(3).unwrap();
    assert_eq!(s.cache_mb, 3);
    assert_eq!(s.cache_bytes(), 3 << 20);
    s.close_library().unwrap();
    drop(s);

    let mut s2 = Session::new();
    s2.open_library(dir.path(), false).unwrap();
    assert_eq!(s2.cache_mb, 3);
    assert_eq!(s2.cache_bytes(), 3 << 20);
    s2.close_library().unwrap();
}

#[test]
fn default_cache_bytes_positive() {
    let dir = TestDir::new("default_cache");
    let mut s = Session::new();
    s.open_library(dir.path(), false).unwrap();
    assert!(s.cache_bytes() > 0);
    assert!(s.cache_bytes() < u64::MAX);
    s.close_library().unwrap();
}

#[test]
fn persist_no_pending_ok() {
    let dir = TestDir::new("persist_empty");
    let mut s = Session::new();
    s.open_library(dir.path(), false).unwrap();
    s.persist().unwrap();
    assert!(s.unsaved().is_none());
    s.close_library().unwrap();
}

#[test]
fn malformed_prefs_set_aside_and_uses_defaults() {
    let dir = TestDir::new("malformed_prefs");
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(dir.path().join("prefs.json"), b"{ not json").unwrap();

    let mut s = Session::new();
    s.open_library(dir.path(), false).unwrap();

    let warnings = s.take_library_warnings();
    assert!(!warnings.is_empty());

    let corrupt_exists =
        std::fs::read_dir(dir.path()).unwrap().filter_map(|e| e.ok()).any(|e| e.file_name().to_string_lossy().starts_with("prefs.json.corrupt-"));
    assert!(corrupt_exists);

    // The damaged file was set aside, so writing a new prefs.json is allowed.
    s.save_prefs().unwrap();

    s.set_cache_mb(5).unwrap();
    assert_eq!(s.cache_mb, 5);

    s.close_library().unwrap();
}

#[test]
fn take_library_warnings_empty_by_default() {
    let dir = TestDir::new("warnings_empty");
    let mut s = Session::new();
    s.open_library(dir.path(), false).unwrap();
    assert!(s.take_library_warnings().is_empty());
    s.close_library().unwrap();
}

#[test]
fn close_library_called_twice_is_ok() {
    let dir = TestDir::new("close_twice");
    let mut s = Session::new();
    s.open_library(dir.path(), false).unwrap();
    s.close_library().unwrap();
    s.close_library().unwrap();
}

#[test]
fn open_library_in_with_fs_store() {
    let dir = TestDir::new("open_in");
    let catalog_store = FsStore::open(dir.path()).unwrap();
    let file_store = FsStore::open(dir.path()).unwrap();

    let stores = LibraryStores { dir: dir.path().to_path_buf(), catalog: Box::new(catalog_store), files: Box::new(file_store), on_disk: true };

    let mut s = Session::new();
    s.open_library_in(stores, false).unwrap();
    assert!(s.library.is_some());
    s.close_library().unwrap();
}

#[test]
fn open_library_with_seed_demo_populates_catalog() {
    let dir = TestDir::new("seed_demo");
    let mut s = Session::new();
    s.open_library(dir.path(), true).unwrap();
    assert!(s.library.is_some());

    let visible = s.visible_cloned();
    assert!(!visible.is_empty());
    s.close_library().unwrap();
    drop(s);

    let mut s2 = Session::new();
    s2.open_library(dir.path(), false).unwrap();
    assert!(!s2.visible_cloned().is_empty());
    s2.close_library().unwrap();
}
