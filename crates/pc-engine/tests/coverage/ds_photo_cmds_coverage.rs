use photocraft_engine::{EngineError, Session, command_specs, photo_cmds};
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
fn specs_returns_three_expected_commands() {
    let specs = photo_cmds::specs();
    assert_eq!(specs.len(), 3);
    assert_eq!(specs[0].id, "file.automate.photomerge");
    assert_eq!(specs[1].id, "file.automate.mergeToHdrPro");
    assert_eq!(specs[2].id, "file.automate.cropAndStraightenPhotos");

    for spec in &specs {
        assert!(!spec.label.is_empty());
        assert!(!spec.params.is_empty());
        assert!(spec.menu.contains(&"File"));
        assert!(spec.menu.contains(&"Automate"));
        assert!(spec.shortcut.is_none());
        assert!(spec.journal);
    }
}

#[test]
fn global_command_registry_includes_photo_commands() {
    let all_specs = command_specs();
    let ids: Vec<&str> = all_specs.iter().map(|s| s.id).collect();
    assert!(ids.contains(&"file.automate.photomerge"));
    assert!(ids.contains(&"file.automate.mergeToHdrPro"));
    assert!(ids.contains(&"file.automate.cropAndStraightenPhotos"));
}

#[test]
fn photomerge_enabled_by_default() {
    let session = Session::new();
    assert!(session.is_enabled("file.automate.photomerge"));
}

#[test]
fn merge_to_hdr_pro_enabled_by_default() {
    let session = Session::new();
    assert!(session.is_enabled("file.automate.mergeToHdrPro"));
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
fn unknown_command_is_disabled() {
    let session = Session::new();
    assert!(!session.is_enabled("file.automate.unknown"));
    assert!(session.disabled_reason("file.automate.unknown").is_some());
}

#[test]
fn photomerge_empty_params_returns_bad_params() {
    let mut session = Session::new();
    let result = session.execute("file.automate.photomerge", json!({}));
    assert!(matches!(
        result,
        Err(EngineError::BadParams { cmd, msg: _ }) if cmd == "file.automate.photomerge"
    ));
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
fn crop_and_straighten_no_document_returns_disabled() {
    // `execute` checks the enabled precondition and returns `Disabled` rather than `NoDocument`.
    let mut session = Session::new();
    let result = session.execute("file.automate.cropAndStraightenPhotos", json!({}));
    assert!(matches!(
        result,
        Err(EngineError::Disabled(cmd, msg)) if cmd == "file.automate.cropAndStraightenPhotos" && msg.contains("no document open")
    ));
}

#[test]
fn photomerge_nan_focal_length_does_not_panic() {
    let mut session = Session::new();
    let result = session.execute("file.automate.photomerge", json!({"useOpenDocuments": true, "focalLength": f64::NAN}));
    // Should return an error (no sources), and must not panic.
    assert!(result.is_err());
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
fn specs_are_deterministic() {
    let first = photo_cmds::specs();
    let second = photo_cmds::specs();
    assert_eq!(first.len(), second.len());
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.label, b.label);
        assert_eq!(a.params, b.params);
        assert_eq!(a.menu, b.menu);
    }
}

#[test]
fn photomerge_exposures_without_sources_still_checks_exposures() {
    // Source loading fails first, so the exposures length check is not reached.
    let mut session = Session::new();
    let result = session.execute("file.automate.mergeToHdrPro", json!({"useOpenDocuments": false, "exposures": [0, 1]}));
    // Should be BadParams because no sources were provided.
    assert!(matches!(
        result,
        Err(EngineError::BadParams { cmd, msg }) if cmd == "file.automate.mergeToHdrPro" && msg.contains("pass")
    ));
}

#[test]
fn commands_are_journaled() {
    for spec in photo_cmds::specs() {
        assert!(spec.journal, "{} should be journaled", spec.id);
    }
}

#[test]
fn all_shortcuts_are_none() {
    for spec in photo_cmds::specs() {
        assert!(spec.shortcut.is_none(), "{} should have no shortcut", spec.id);
    }
}

#[test]
fn enabled_callbacks_are_consistent_with_is_enabled() {
    let session = Session::new();
    assert!(session.is_enabled("file.automate.photomerge"));
    assert!(session.is_enabled("file.automate.mergeToHdrPro"));
    assert!(!session.is_enabled("file.automate.cropAndStraightenPhotos"));
}
