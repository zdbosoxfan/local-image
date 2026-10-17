use photocraft_engine::{EngineError, Session};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(label: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let name = format!("pc_photo_{}_{}_{}", label, std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
    let dir = base.join(name);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn crop_and_straighten_disabled_without_document() {
    let session = Session::new();
    assert!(!session.is_enabled("file.automate.cropAndStraightenPhotos"));
    let reason = session.disabled_reason("file.automate.cropAndStraightenPhotos");
    assert!(reason.is_some());
    assert!(reason.unwrap().contains("no document open"));
}

#[test]
fn photomerge_invalid_layout_returns_bad_params() {
    let mut session = Session::new();
    let result = session.execute("file.automate.photomerge", json!({"paths": ["nonexistent"], "layout": "zigzag"}));
    // Layout validation happens before source loading, so this error is returned.
    assert!(matches!(
        result,
        Err(EngineError::BadParams { cmd, msg }) if cmd == "file.automate.photomerge" && msg.contains("unknown layout")
    ));
}

#[test]
fn photomerge_no_sources_returns_bad_params() {
    let mut session = Session::new();
    let result = session.execute("file.automate.photomerge", json!({"useOpenDocuments": true}));
    assert!(matches!(
        result,
        Err(EngineError::BadParams { cmd, msg }) if cmd == "file.automate.photomerge" && msg.contains("pass")
    ));

    let result = session.execute("file.automate.photomerge", json!({"paths": []}));
    assert!(matches!(
        result,
        Err(EngineError::BadParams { cmd, msg }) if cmd == "file.automate.photomerge" && msg.contains("pass")
    ));
}

#[test]
fn merge_to_hdr_pro_empty_params_returns_bad_params() {
    let mut session = Session::new();
    let result = session.execute("file.automate.mergeToHdrPro", json!({}));
    assert!(matches!(
        result,
        Err(EngineError::BadParams { cmd, msg }) if cmd == "file.automate.mergeToHdrPro" && msg.contains("pass")
    ));
}

#[test]
fn photomerge_null_params_returns_error_no_panic() {
    let mut session = Session::new();
    let result = session.execute("file.automate.photomerge", Value::Null);
    assert!(result.is_err());
}

#[test]
fn temp_file_with_invalid_image_returns_error() {
    let dir = unique_temp_dir("invalid_img");
    let file_path = dir.join("fake.png");
    let mut f = fs::File::create(&file_path).unwrap();
    f.write_all(b"this is not a valid image").unwrap();
    f.sync_all().unwrap();

    let mut session = Session::new();
    let result = session.execute("file.automate.photomerge", json!({"paths": [file_path.to_str().unwrap()]}));
    // Read or import should fail with an error.
    assert!(result.is_err());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn empty_directory_as_paths_returns_error() {
    let dir = unique_temp_dir("empty_dir");
    let mut session = Session::new();
    let result = session.execute("file.automate.photomerge", json!({"paths": [dir.to_str().unwrap()]}));
    // Passing a directory path (as an array element) causes read_file to fail.
    assert!(result.is_err());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn enabled_callbacks_are_consistent_with_is_enabled() {
    let session = Session::new();
    assert!(session.is_enabled("file.automate.photomerge"));
    assert!(session.is_enabled("file.automate.mergeToHdrPro"));
    assert!(!session.is_enabled("file.automate.cropAndStraightenPhotos"));
}
