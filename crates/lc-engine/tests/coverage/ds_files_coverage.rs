use lightcraft_engine::catalog::MediaKind;
use lightcraft_engine::files::{self, RawOptions};
use lightcraft_engine::{Session, develop};
use std::fs;

/// A valid 1x1 transparent PNG (RGBA, non-interlaced).
fn tiny_png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44,
        0x41, 0x54, // IDAT chunk (length 10)
        0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, // IDAT CRC
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82, // IEND chunk
    ]
}

#[test]
fn raw_options_default_is_default() {
    let opts = RawOptions::default();
    assert!(opts.is_default());
    assert_eq!(opts.key(), 0);
}

#[test]
fn raw_options_of_default_settings_is_default() {
    let settings = develop::DevelopSettings::default();
    let opts = RawOptions::of(&settings);
    assert!(opts.is_default());
    assert_eq!(opts, RawOptions::default());
}

#[test]
fn raw_options_non_default_key_odd() {
    let opts =
        RawOptions { demosaic: develop::Demosaic::Amaze, dual_threshold: 0.5, highlights: develop::HighlightMode::Reconstruct, capture_radius: true };
    assert!(!opts.is_default());
    let key = opts.key();
    assert_ne!(key, 0);
    assert_eq!(key & 1, 1, "key must be odd to distinguish from default");
}

#[test]
fn raw_options_key_deterministic() {
    let a = RawOptions { demosaic: develop::Demosaic::Ppg, dual_threshold: 0.2, highlights: develop::HighlightMode::Opposed, capture_radius: false };
    let b = a;
    assert_eq!(a.key(), b.key());
}

#[test]
fn probe_bytes_valid_png() {
    let bytes = tiny_png();
    let info = files::probe_bytes("test.png", &bytes).expect("valid PNG should probe");
    assert_eq!(info.width, 1);
    assert_eq!(info.height, 1);
    assert_eq!(info.kind, MediaKind::Image);
    assert_eq!(info.format, "PNG");
    assert_eq!(info.file_size, bytes.len() as u64);
    assert!(info.content_hash.is_some());
}

#[test]
fn probe_bytes_empty_bytes_err() {
    assert!(files::probe_bytes("x", &[]).is_err());
}

#[test]
fn probe_bytes_random_bytes_err() {
    assert!(files::probe_bytes("x", &[0xDE, 0xAD, 0xBE, 0xEF]).is_err());
}

#[test]
fn probe_bytes_truncated_png_err() {
    let mut bytes = tiny_png();
    bytes.truncate(20);
    assert!(files::probe_bytes("bad.png", &bytes).is_err());
}

#[test]
fn probe_bytes_content_hash_deterministic() {
    let bytes = tiny_png();
    let a = files::probe_bytes("a.png", &bytes).unwrap();
    let b = files::probe_bytes("b.png", &bytes).unwrap();
    assert_eq!(a.content_hash, b.content_hash);
}

#[test]
fn probe_bytes_unsupported_raw_err() {
    let minimal_cr3 = b"\0\0\0\x18ftypcrx \0\0\0\x01crx isom";
    assert!(files::probe_bytes("x.cr3", minimal_cr3).is_err());
}

#[test]
fn load_bytes_valid_png() {
    let bytes = tiny_png();
    let (img, src) = files::load_bytes(&bytes, 16).expect("PNG should decode");
    assert_eq!(img.width, 1);
    assert_eq!(img.height, 1);
    assert!(!src.raw);
    assert!(img.data.iter().all(|p| p.iter().all(|v| v.is_finite())));
}

#[test]
fn load_bytes_with_default_options_matches_load_bytes() {
    let bytes = tiny_png();
    let opts = RawOptions::default();
    let (img1, info1) = files::load_bytes(&bytes, 8).unwrap();
    let (img2, info2) = files::load_bytes_with(&bytes, 8, &opts).unwrap();
    assert_eq!(img1.width, img2.width);
    assert_eq!(img1.height, img2.height);
    assert_eq!(info1.raw, info2.raw);
}

#[test]
fn load_bytes_empty_bytes_err() {
    assert!(files::load_bytes(&[], 8).is_err());
}

#[test]
fn load_bytes_invalid_bytes_err() {
    assert!(files::load_bytes(&[1, 2, 3], 8).is_err());
}

#[test]
fn load_bytes_max_edge_larger_keeps_original() {
    let bytes = tiny_png();
    let (img, _) = files::load_bytes(&bytes, 100).unwrap();
    assert_eq!((img.width, img.height), (1, 1));
}

#[test]
fn load_embedded_preview_non_raw_none() {
    let bytes = tiny_png();
    assert!(files::load_embedded_preview(&bytes, 64).is_none());
}

#[test]
fn embedded_preview_srgb_non_raw_none() {
    let bytes = tiny_png();
    assert!(files::embedded_preview_srgb(&bytes, 64).is_none());
}

#[test]
fn fs_hooks_loader_and_probe_work() {
    let dir = std::env::temp_dir().join(format!("lc-files-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tiny.png");
    fs::write(&path, tiny_png()).unwrap();

    let (loader, prober) = files::fs_hooks();
    let info = prober(path.to_str().unwrap()).expect("probe should work");
    assert_eq!(info.width, 1);
    assert_eq!(info.height, 1);

    let (img, src) = loader(path.to_str().unwrap(), 8).expect("load should work");
    assert_eq!(img.width, 1);
    assert!(!src.raw);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fs_loader_with_works() {
    let dir = std::env::temp_dir().join(format!("lc-files-with-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tiny.png");
    fs::write(&path, tiny_png()).unwrap();

    let loader = files::fs_loader_with();
    let opts = RawOptions::default();
    let (img, _) = loader(path.to_str().unwrap(), 8, &opts).expect("load with options should work");
    assert_eq!(img.width, 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fs_preview_loader_returns_none_for_png() {
    let dir = std::env::temp_dir().join(format!("lc-preview-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tiny.png");
    fs::write(&path, tiny_png()).unwrap();

    let preview = files::fs_preview_loader();
    assert!(preview(path.to_str().unwrap(), 32).is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn session_with_fs_sets_hooks() {
    let session = Session::new().with_fs();
    assert!(session.media.file_loader.is_some());
    assert!(session.media.file_loader_with.is_some());
    assert!(session.media.file_probe.is_some());
    assert!(session.media.preview_loader.is_some());
}
