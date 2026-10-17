use lightcraft_engine::smart_sort::tokens::{DEFAULT_FOLDER_PATTERN, TokenValues, expand_file, expand_folder, validate};
use std::path::Path;

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
