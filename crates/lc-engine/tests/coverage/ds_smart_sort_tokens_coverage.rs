use lightcraft_engine::smart_sort::tokens::{DEFAULT_FOLDER_PATTERN, TokenValues, dedupe_paths, expand_file, expand_folder, validate};
use std::path::{Path, PathBuf};

fn norm(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
fn default_folder_pattern_is_valid_for_folders_only() {
    assert!(validate(DEFAULT_FOLDER_PATTERN, false).is_ok());
    assert!(validate(DEFAULT_FOLDER_PATTERN, true).is_err());
}

#[test]
fn default_token_values_expand_to_empty_and_defaults() {
    let v = TokenValues::default();
    assert_eq!(v.event, "");
    assert_eq!(v.folder, "");
    assert_eq!(v.person, None);
    assert_eq!(v.session, None);
    assert_eq!(v.captured, None);
    assert_eq!(v.camera, None);
    assert_eq!(v.original, None);
    assert_eq!(v.seq, None);

    let path = expand_folder("{event}/{folder}", &v).unwrap();
    assert_eq!(norm(&path), "");

    let file = expand_file("{event}_{folder}", &v).unwrap();
    assert_eq!(file, "_");
}

#[test]
fn validate_rejects_absolute_folder_pattern() {
    let err = validate("/{event}", false).unwrap_err();
    assert!(err.contains("folder pattern must be relative"));
    assert!(validate("\\{event}", false).is_err());
}

#[test]
fn validate_file_pattern_rejects_directory_separators() {
    let err = validate("{event}/{folder}", true).unwrap_err();
    assert!(err.contains("file pattern cannot contain '/'"));
    assert!(validate("{event}\\{folder}", true).is_err());
}

#[test]
fn validate_rejects_malformed_placeholders() {
    assert!(validate("{event", false).unwrap_err().contains("unclosed token placeholder"));
    assert!(validate("event}", false).unwrap_err().contains("unmatched '}'"));
    assert!(validate("{}", false).unwrap_err().contains("empty token placeholder"));
}

#[test]
fn validate_folder_rejects_file_only_tokens() {
    for pattern in ["{seq}", "{seq:4}", "{original}"] {
        let err = validate(pattern, false).unwrap_err();
        assert!(err.contains("token not allowed in folder pattern"), "pattern {pattern}: {err}");
    }
}

#[test]
fn validate_file_accepts_sequence_and_original() {
    assert!(validate("{seq}", true).is_ok());
    assert!(validate("{seq:4}", true).is_ok());
    assert!(validate("{original}", true).is_ok());
}

#[test]
fn validate_sequence_padding_boundaries() {
    assert!(validate("{seq:1}", true).is_ok());
    assert!(validate("{seq:9}", true).is_ok());

    for pattern in ["{seq:0}", "{seq:10}", "{seq:abc}", "{seq:}"] {
        assert!(validate(pattern, true).is_err(), "expected error for {pattern}");
    }
}

#[test]
fn unknown_token_is_an_error() {
    let err = validate("{event}/{wat}", false).unwrap_err();
    assert!(err.contains("unknown token {wat}"));

    let err = validate("{event}_{wat}", true).unwrap_err();
    assert!(err.contains("unknown token {wat}"));
}

#[test]
fn expand_folder_drops_empty_segments() {
    let v = TokenValues { event: "E".into(), folder: "F".into(), session: Some("S".into()), ..Default::default() };
    let path = expand_folder("{event}/{person}/{session}/{folder}", &v).unwrap();
    assert_eq!(norm(&path), "E/S/F");
}

#[test]
fn expand_folder_sanitizes_component_per_segment() {
    let v = TokenValues { event: "Event: side?".into(), folder: "F\\lder*".into(), ..Default::default() };
    let path = expand_folder("{event}/{folder}", &v).unwrap();
    assert_eq!(norm(&path), "Event_ side_/F_lder_");
}

#[test]
fn expand_folder_trims_dots_and_whitespace() {
    let v = TokenValues { event: "  ..  ".into(), folder: "F".into(), ..Default::default() };
    let path = expand_folder("{event}/{folder}", &v).unwrap();
    assert_eq!(norm(&path), "F");

    let file = expand_file("{event}", &v).unwrap();
    assert_eq!(file, "");
}

#[test]
fn capture_date_extracts_iso_date() {
    let v = TokenValues { captured: Some("2026-10-05T12:34:56".into()), ..Default::default() };
    assert_eq!(norm(&expand_folder("{date}", &v).unwrap()), "2026-10-05");

    let v = TokenValues { captured: Some("2026-10-0é".into()), ..Default::default() };
    assert_eq!(norm(&expand_folder("{date}", &v).unwrap()), "No date");

    let v = TokenValues { captured: Some("short".into()), ..Default::default() };
    assert_eq!(norm(&expand_folder("{date}", &v).unwrap()), "No date");

    let v = TokenValues::default();
    assert_eq!(norm(&expand_folder("{date}", &v).unwrap()), "No date");
}

#[test]
fn camera_defaults_when_missing_or_empty() {
    let v = TokenValues::default();
    assert_eq!(norm(&expand_folder("{camera}", &v).unwrap()), "Unknown camera");

    let v = TokenValues { camera: Some("".into()), ..Default::default() };
    assert_eq!(norm(&expand_folder("{camera}", &v).unwrap()), "Unknown camera");

    let v = TokenValues { camera: Some("Sony A7".into()), ..Default::default() };
    assert_eq!(norm(&expand_folder("{camera}", &v).unwrap()), "Sony A7");
}

#[test]
fn sequence_token_requires_sequence_number() {
    let v = TokenValues::default();

    let err = expand_file("{seq}", &v).unwrap_err();
    assert!(err.contains("sequence token requires a sequence number"));

    let err = expand_file("{seq:4}", &v).unwrap_err();
    assert!(err.contains("sequence token requires a sequence number"));
}

#[test]
fn sequence_values_and_padding() {
    let v = TokenValues { seq: Some(7), ..Default::default() };

    assert_eq!(expand_file("{seq}", &v).unwrap(), "7");
    assert_eq!(expand_file("{seq:1}", &v).unwrap(), "7");
    assert_eq!(expand_file("{seq:4}", &v).unwrap(), "0007");
    assert_eq!(expand_file("{seq:9}", &v).unwrap(), "000000007");

    // Deterministic
    assert_eq!(expand_file("{seq:4}", &v).unwrap(), expand_file("{seq:4}", &v).unwrap());
}

#[test]
fn dedupe_paths_adds_case_insensitive_suffix() {
    let paths = vec![
        ("one".to_string(), PathBuf::from("one/two")),
        ("two".to_string(), PathBuf::from("one/two")),
        ("three".to_string(), PathBuf::from("ONE/two")),
        ("four".to_string(), PathBuf::from("one/three")),
    ];

    let out = dedupe_paths(paths);

    assert_eq!(out.len(), 4);
    assert_eq!(out[0].0, "one");
    assert_eq!(norm(&out[0].1), "one/two");
    assert_eq!(out[1].0, "two");
    assert_eq!(norm(&out[1].1), "one/two (2)");
    assert_eq!(out[2].0, "three");
    assert_eq!(norm(&out[2].1), "ONE/two (3)");
    assert_eq!(out[3].0, "four");
    assert_eq!(norm(&out[3].1), "one/three");
}

#[test]
fn dedupe_paths_avoids_existing_suffixes() {
    let paths = vec![
        ("a".to_string(), PathBuf::from("one/two")),
        ("b".to_string(), PathBuf::from("one/two (2)")),
        ("c".to_string(), PathBuf::from("ONE/two")),
    ];

    let out = dedupe_paths(paths);

    assert_eq!(out.len(), 3);
    assert_eq!(norm(&out[2].1), "ONE/two (3)");
}

#[test]
fn dedupe_paths_is_deterministic() {
    let input = || {
        vec![("a".to_string(), PathBuf::from("one/two")), ("b".to_string(), PathBuf::from("one/two")), ("c".to_string(), PathBuf::from("ONE/three"))]
    };

    let first = dedupe_paths(input());
    let second = dedupe_paths(input());

    assert_eq!(first, second);
}

#[test]
fn token_values_serde_round_trip() {
    let v = TokenValues {
        event: "E".into(),
        folder: "F".into(),
        person: Some("P".into()),
        session: Some("S".into()),
        captured: Some("2026-10-05T12:34:56".into()),
        camera: Some("Cam".into()),
        original: Some("IMG_0001".into()),
        seq: Some(42),
    };

    let json = serde_json::to_value(&v).unwrap();
    let back: TokenValues = serde_json::from_value(json.clone()).unwrap();

    assert_eq!(back, v);
    assert_eq!(serde_json::to_value(&back).unwrap(), json);
}
