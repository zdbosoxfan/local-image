use photocraft_io::abr_map::{AbrImport, MAX_PATTERN_EDGE, MAX_TIP_EDGE, read_abr};

#[test]
fn empty_input_returns_err() {
    let result = read_abr(b"", "test.abr");
    match result {
        Err(e) => assert!(e.starts_with("not a readable Photoshop brush file:")),
        Ok(_) => panic!("expected error for empty input"),
    }
}

#[test]
fn non_abr_random_input_returns_err() {
    let bytes = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02, 0x03];
    let result = read_abr(&bytes, "random.bin");
    match result {
        Err(e) => assert!(e.starts_with("not a readable Photoshop brush file:")),
        Ok(_) => panic!("expected error for random non-ABR bytes"),
    }
}

#[test]
fn magic_8bab_truncated_returns_err() {
    // v6 magic without a proper header/payload
    let bytes = b"8BAB\x00\x00";
    let result = read_abr(bytes, "truncated.abr");
    match result {
        Err(e) => assert!(e.starts_with("not a readable Photoshop brush file:")),
        Ok(_) => panic!("expected error for truncated 8BAB input"),
    }
}

#[test]
fn wrong_magic_8bps_returns_err() {
    // PSD magic is not an ABR file
    let bytes = b"8BPS\x00\x01";
    let result = read_abr(bytes, "wrong.abr");
    match result {
        Err(e) => assert!(e.starts_with("not a readable Photoshop brush file:")),
        Ok(_) => panic!("expected error for 8BPS (PSD) magic"),
    }
}

#[test]
fn minimal_v6_header_without_sections_returns_err() {
    // Starts like a v6 ABR but has no sections; parser should reject cleanly.
    let bytes = [
        0x38, 0x42, 0x41, 0x42, // "8BAB"
        0x00, 0x06, // version 6
        0x00, 0x02, // minor version
        0x00, 0x00, 0x00, 0x00, // no sections, likely EOF / missing data
    ];
    let result = read_abr(&bytes, "minimal-v6.abr");
    match result {
        Err(e) => assert!(e.starts_with("not a readable Photoshop brush file:")),
        Ok(_) => panic!("expected error for minimal v6 header without sections"),
    }
}

#[test]
fn unicode_group_name_does_not_break_error_path() {
    // The group string is only used after a successful parse; malformed input
    // should still produce the same error prefix regardless of group contents.
    let result = read_abr(b"", "тест/🖌️.abr");
    match result {
        Err(e) => assert!(e.starts_with("not a readable Photoshop brush file:")),
        Ok(_) => panic!("expected error for malformed input with unicode group"),
    }
}

#[test]
fn empty_group_name_does_not_break_error_path() {
    let result = read_abr(b"", "");
    match result {
        Err(e) => assert!(e.starts_with("not a readable Photoshop brush file:")),
        Ok(_) => panic!("expected error for malformed input with empty group"),
    }
}

#[test]
fn constants_are_exact() {
    assert_eq!(MAX_TIP_EDGE, 2500);
    assert_eq!(MAX_PATTERN_EDGE, 1024);
}

#[test]
fn default_abr_import_is_empty() {
    let import = AbrImport::default();
    assert!(import.presets.is_empty());
    assert!(import.warnings.is_empty());
    assert_eq!(import.version, 0);
}

#[test]
fn abr_import_derive_clone_and_debug() {
    let import = AbrImport::default();
    let cloned = import.clone();
    assert!(cloned.presets.is_empty());
    assert!(cloned.warnings.is_empty());
    assert_eq!(cloned.version, 0);
    // Debug formatting should not panic.
    let _ = format!("{import:?}");
}

#[test]
fn read_abr_errors_are_deterministic_for_malformed_input() {
    let bytes = [0xFF, 0xFF, 0x00, 0x01, 0x02, 0x03];
    let r1 = read_abr(&bytes, "test.abr");
    let r2 = read_abr(&bytes, "test.abr");
    match (r1, r2) {
        (Err(e1), Err(e2)) => assert_eq!(e1, e2),
        _ => panic!("expected deterministic errors for malformed input"),
    }
}

#[test]
fn read_abr_does_not_panic_on_large_invalid_input() {
    // 100 KB of bytes with an invalid version prefix (0xFFFF) to force an early
    // parse error; should never panic and should return Err in well under a second.
    let mut bytes = vec![0u8; 100_000];
    bytes[0] = 0xFF;
    bytes[1] = 0xFF;
    // Fill the rest with a pattern so the parser has non-zero data to reject.
    for (i, b) in bytes.iter_mut().enumerate().skip(2) {
        *b = (i % 251) as u8;
    }
    let result = read_abr(&bytes, "large-invalid.abr");
    match result {
        Err(e) => assert!(e.starts_with("not a readable Photoshop brush file:")),
        Ok(_) => panic!("expected error for large invalid input"),
    }
}
