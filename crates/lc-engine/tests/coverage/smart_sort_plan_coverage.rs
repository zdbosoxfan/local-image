use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use lightcraft_engine::Session;
use lightcraft_engine::catalog::{Catalog, Match, Rule};
use lightcraft_engine::smart_sort::SortPreset;
use lightcraft_engine::smart_sort::plan;

fn temp_subdir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("time before epoch").as_nanos();
    let path = std::env::temp_dir().join(format!("lightcraft-smart-sort-{tag}-{nanos}"));
    std::fs::create_dir_all(&path).expect("failed to create temp dir");
    path
}

#[test]
fn sanitize_trims_dots_and_whitespace_per_component() {
    assert_eq!(plan::sanitize("  foo..  /bar  ").unwrap(), "foo/bar");
    assert_eq!(plan::sanitize(".../   ...").unwrap(), "Untitled/Untitled");
}

#[test]
fn sanitize_replaces_invalid_and_control_chars() {
    assert_eq!(plan::sanitize("a<b>c:d\"e|f?g*h").unwrap(), "a_b_c_d_e_f_g_h");
    assert_eq!(plan::sanitize("a\\b").unwrap(), "a_b");
    assert_eq!(plan::sanitize("a\nb\tc").unwrap(), "a_b_c");
}

#[test]
fn sanitize_rejects_absolute_paths_and_drive_prefixes() {
    assert!(plan::sanitize("/absolute").is_err());
    assert!(plan::sanitize("\\absolute").is_err());
    assert!(plan::sanitize("C:foo").is_err());
    // Colon not at byte index 1 is treated as an invalid filename character.
    assert_eq!(plan::sanitize(":a").unwrap(), "_a");
}

#[test]
fn tag_rules_empty_tags_uses_any_and_no_rules() {
    let rules = plan::tag_rules("Parent", &[], false);
    assert!(matches!(rules.mode, Match::Any));
    assert!(rules.rules.is_empty());

    let rules_all = plan::tag_rules("Parent", &[], true);
    assert!(matches!(rules_all.mode, Match::Any));
    assert!(rules_all.rules.is_empty());
}

#[test]
fn tag_rules_any_mode_builds_exact_keyword_rules() {
    let tags = vec!["One".to_string(), "Two ".to_string()];
    let rules = plan::tag_rules("Parent", &tags, false);

    assert!(matches!(rules.mode, Match::Any));
    assert_eq!(rules.rules.len(), 2);

    match &rules.rules[0] {
        Rule::Field { field, op, value } => {
            assert_eq!(field, "keywords");
            assert_eq!(op, "is");
            assert_eq!(value, "Parent|One");
        }
        other => panic!("unexpected rule: {other:?}"),
    }

    match &rules.rules[1] {
        Rule::Field { field, op, value } => {
            assert_eq!(field, "keywords");
            assert_eq!(op, "is");
            assert_eq!(value, "Parent|Two");
        }
        other => panic!("unexpected rule: {other:?}"),
    }
}

#[test]
fn default_folders_adds_disabled_unsorted_after_keyword_folders() {
    let preset = SortPreset::default();
    // Default categories are empty, so only the Unsorted folder is appended.
    let folders = plan::default_folders(&preset);
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "Unsorted");
    assert!(!folders[0].enabled);
    assert!(folders[0].unsorted);
}

#[test]
fn plan_with_enabled_unsorted_folder_includes_unsorted_even_when_empty() {
    let cat = Catalog::new();
    let folders = vec![plan::FolderDef { name: "Unsorted".into(), unsorted: true, enabled: true, ..Default::default() }];
    let result = plan::plan(&cat, &[], &folders).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "Unsorted");
    assert!(result[0].ids.is_empty());
}

#[test]
fn check_destination_rejects_empty_and_blank() {
    let session = Session::new();
    let err = plan::check_destination(&session, "").unwrap_err();
    assert!(err.contains("choose an export destination"));
    let err = plan::check_destination(&session, "   ").unwrap_err();
    assert!(err.contains("choose an export destination"));
}

#[test]
fn check_destination_accepts_temp_dir() {
    let dir = temp_subdir("check_dest");
    let session = Session::new();
    let path = dir.to_str().unwrap();
    plan::check_destination(&session, path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
