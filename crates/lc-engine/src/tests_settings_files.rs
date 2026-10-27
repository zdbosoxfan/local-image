//! Issue #103: a damaged or unreadable settings file (prefs.json, presets.json, view.json) is
//! never silently replaced by the defaults: a damaged one is kept aside, an unreadable one is not
//! written this session, and the user is told.

use std::sync::{Arc, Mutex};

use lightcraft_catalog::{MemStore, Store};
use serde_json::json;

use crate::Session;
use crate::library::LibraryStores;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lc-settings-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn damaged_prefs_are_kept_aside_and_reported() {
    let lib = temp_dir("damaged");
    let bad = br#"{"exportPresets": [{"name": "Proof", "par"#;
    std::fs::write(lib.join("prefs.json"), bad).unwrap();
    let mut s = Session::new().with_fs();
    s.open_library(&lib, false).unwrap();
    let kept: Vec<_> = std::fs::read_dir(&lib)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("prefs.json.corrupt-"))
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(std::fs::read(lib.join(&kept[0])).unwrap(), bad);
    let warnings = s.take_library_warnings();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("prefs.json is damaged") && warnings[0].contains(&kept[0]), "{warnings:?}");
    assert!(s.take_library_warnings().is_empty(), "each warning is handed out once");
    let info = s.execute("library.info", &json!({})).unwrap();
    assert_eq!(info["settingsWarnings"].as_array().map(Vec::len), Some(1), "{info}");
    // saving goes on (the damaged original is safe in its copy)
    s.execute("library.preferences", &json!({"cacheMb": 300})).unwrap();
    assert!(String::from_utf8(std::fs::read(lib.join("prefs.json")).unwrap()).unwrap().contains("300"));
    assert_eq!(std::fs::read(lib.join(&kept[0])).unwrap(), bad);
    drop(s);
    let _ = std::fs::remove_dir_all(&lib);
}

/// `presets.json` and `prefs.json` can't be read (locked by another program, an I/O error).
#[derive(Clone, Default)]
struct Unreadable {
    files: MemStore,
    unreadable: Arc<Mutex<Vec<String>>>,
}

impl Store for Unreadable {
    fn read(&mut self, name: &str) -> std::io::Result<Option<Vec<u8>>> {
        if self.unreadable.lock().unwrap().iter().any(|n| n == name) {
            return Err(std::io::Error::other("locked by another process"));
        }
        self.files.read(name)
    }
    fn write_atomic(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        self.files.write_atomic(name, data)
    }
    fn append(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        self.files.append(name, data)
    }
    fn truncate(&mut self, name: &str, len: u64) -> std::io::Result<()> {
        self.files.truncate(name, len)
    }
    fn describe(&self) -> String {
        "unreadable".into()
    }
}

#[test]
fn unreadable_settings_are_not_overwritten() {
    let files = Unreadable::default();
    let presets = br#"{"user": [{"id": "user.mine", "name": "Mine", "group": "User Presets", "settings": {}}]}"#.to_vec();
    let prefs = br#"{"cacheMb": 777}"#.to_vec();
    files.files.set("presets.json", presets.clone());
    files.files.set("prefs.json", prefs.clone());
    *files.unreadable.lock().unwrap() = vec!["presets.json".into(), "prefs.json".into()];
    let mut s = Session::new();
    let stores = LibraryStores { dir: "unreadable".into(), catalog: Box::new(MemStore::new()), files: Box::new(files.clone()), on_disk: false };
    s.open_library_in(stores, true).unwrap();
    let warnings = s.take_library_warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings.iter().all(|w| w.contains("couldn't be read") && w.contains("won't overwrite")), "{warnings:?}");

    // a preset change and a preference change: neither file is replaced by defaults
    s.execute("preset.create", &json!({"name": "New"})).unwrap();
    let e = s.execute("library.preferences", &json!({"cacheMb": 300}));
    assert!(e.is_err(), "saving preferences is refused, not done over the unread file");
    assert_eq!(files.files.get("presets.json").unwrap(), presets);
    assert_eq!(files.files.get("prefs.json").unwrap(), prefs);
}

#[cfg(unix)]
#[test]
fn unrelated_prefs_link_is_untouched_and_saving_is_refused() {
    let lib = temp_dir("prefs-link");
    let target = lib.join("notes.txt");
    let bytes = b"These notes are not JSON preferences.";
    std::fs::write(&target, bytes).unwrap();
    std::os::unix::fs::symlink(&target, lib.join("prefs.json")).unwrap();
    let mut session = Session::new().with_fs();
    session.open_library(&lib, false).unwrap();
    let error = session.save_prefs().unwrap_err().to_string();
    assert!(error.contains("prefs.json") && error.contains(&target.display().to_string()), "{error}");
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    let warnings = session.take_library_warnings();
    assert!(warnings.iter().any(|w| w.contains("prefs.json") && w.contains(&target.display().to_string())), "{warnings:?}");
    assert!(!std::fs::read_dir(&lib).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("prefs.json.corrupt-")));
    drop(session);
    std::fs::remove_dir_all(lib).unwrap();
}

#[cfg(unix)]
#[test]
fn refused_log_link_warning_reaches_library_and_close_refuses_saving() {
    let lib = temp_dir("log-link-warning");
    let mut session = Session::new().with_fs();
    session.open_library(&lib, false).unwrap();
    session.close_library().unwrap();
    drop(session);
    let target = lib.join("notes.txt");
    let bytes = b"Leave these notes alone.";
    std::fs::write(&target, bytes).unwrap();
    let log = lib.join("catalog.log");
    std::fs::remove_file(&log).unwrap();
    std::os::unix::fs::symlink(&target, &log).unwrap();
    let snapshot = std::fs::read(lib.join("catalog.snap")).unwrap();
    let mut session = Session::new().with_fs();
    session.open_library(&lib, false).unwrap();
    let warnings = session.take_library_warnings();
    assert!(warnings.iter().any(|w| w.contains(&log.display().to_string()) && w.contains(&target.display().to_string())), "{warnings:?}");
    assert!(session.execute("album.create", &json!({"name": "Unsaved"})).is_err());
    assert!(session.close_library().unwrap_err().to_string().contains(&target.display().to_string()));
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    assert_eq!(std::fs::read(lib.join("catalog.snap")).unwrap(), snapshot);
    drop(session);
    std::fs::remove_dir_all(lib).unwrap();
}

#[cfg(unix)]
#[test]
fn relocated_settings_links_remain_writable() {
    let lib = temp_dir("relocated-settings");
    let moved = lib.join("another-disk");
    std::fs::create_dir(&moved).unwrap();
    for (name, bytes) in [("prefs.json", "{}"), ("presets.json", "{}"), ("view.json", "{}"), ("smart-sort-tagsets.json", "[]")] {
        let target = moved.join(name);
        std::fs::write(&target, bytes).unwrap();
        std::os::unix::fs::symlink(&target, lib.join(name)).unwrap();
    }
    let mut session = Session::new().with_fs();
    session.open_library(&lib, true).unwrap();
    assert!(session.take_library_warnings().is_empty());
    session.set_cache_mb(321).unwrap();
    session.execute("preset.create", &json!({"name": "Relocated"})).unwrap();
    session.save_smart_tag_sets().unwrap();
    session.source = crate::LibrarySource::Picks;
    session.close_library().unwrap();
    let prefs: serde_json::Value = serde_json::from_slice(&std::fs::read(moved.join("prefs.json")).unwrap()).unwrap();
    assert_eq!(prefs["cacheMb"], 321);
    let presets: serde_json::Value = serde_json::from_slice(&std::fs::read(moved.join("presets.json")).unwrap()).unwrap();
    assert!(presets["user"].as_array().unwrap().iter().any(|p| p["name"] == "Relocated"));
    let view: serde_json::Value = serde_json::from_slice(&std::fs::read(moved.join("view.json")).unwrap()).unwrap();
    assert_eq!(view["source"], json!({"kind": "picks"}));
    for name in ["prefs.json", "presets.json", "view.json", "smart-sort-tagsets.json"] {
        assert_eq!(std::fs::read_link(lib.join(name)).unwrap(), moved.join(name));
    }
    drop(session);
    std::fs::remove_dir_all(lib).unwrap();
}

#[cfg(unix)]
#[test]
fn linked_read_errors_warn_with_target_and_refuse_saving() {
    for name in ["catalog.log", "catalog.snap", "prefs.json"] {
        let lib = temp_dir(&format!("read-error-{name}"));
        let mut session = Session::new().with_fs();
        session.open_library(&lib, false).unwrap();
        session.close_library().unwrap();
        drop(session);
        let snapshot = std::fs::read(lib.join("catalog.snap")).unwrap();
        let log = std::fs::read(lib.join("catalog.log")).unwrap();
        let target = lib.join("target-directory");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("notes.txt"), b"keep these notes").unwrap();
        let link = lib.join(name);
        if link.exists() {
            std::fs::remove_file(&link).unwrap();
        }
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut session = Session::new().with_fs();
        session.open_library(&lib, false).unwrap();
        let warnings = session.take_library_warnings();
        assert!(
            warnings.iter().any(|w| w.contains(&link.display().to_string()) && w.contains(&target.display().to_string())),
            "{name}: {warnings:?}"
        );
        let error = if name == "prefs.json" {
            session.save_prefs().unwrap_err()
        } else {
            assert!(session.execute("album.create", &json!({"name": "Unsaved"})).is_err());
            session.close_library().unwrap_err()
        };
        assert!(error.to_string().contains(&target.display().to_string()), "{name}: {error}");
        if name != "catalog.snap" {
            assert_eq!(std::fs::read(lib.join("catalog.snap")).unwrap(), snapshot);
        }
        if name != "catalog.log" {
            assert_eq!(std::fs::read(lib.join("catalog.log")).unwrap(), log);
        }
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
        assert_eq!(std::fs::read(target.join("notes.txt")).unwrap(), b"keep these notes");
        drop(session);
        std::fs::remove_dir_all(lib).unwrap();
    }
}
