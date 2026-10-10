use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lightcraft_engine::EngineError;
use lightcraft_engine::Session;
use lightcraft_engine::catalog::{Photo, PhotoId, Source};
use lightcraft_engine::import::{
    self, CameraDefault, Duplicate, EXTENSIONS, ImportCandidate, ImportDefaults, ImportMode, ImportOptions, ImportReport, Kept, Moved, Organize,
    Prepared, apply_import_defaults, civil, expand, has_import_look, import, is_supported, scan, system_clock,
};

static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let n = DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("lc_import_test_{}_{}_{}", tag, std::process::id(), n));
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn guard(p: PathBuf) -> Cleanup {
    Cleanup(p)
}

fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn make_photo() -> Photo {
    Photo::new(PhotoId(0), Source::File { path: "/tmp/x.jpg".to_string() }, "x.jpg", "JPG", 640, 480, "2026-01-01T00:00:00")
}

#[test]
fn supported_extensions_are_case_insensitive_and_unknown_rejected() {
    for ext in EXTENSIONS {
        assert!(is_supported(Path::new(&format!("a.{ext}"))));
        assert!(is_supported(Path::new(&format!("a.{}", ext.to_uppercase()))));
    }
    assert!(!is_supported(Path::new("a.txt")));
    assert!(!is_supported(Path::new("noext")));
}

#[test]
fn expand_recursive_skips_hidden_and_skip_and_keeps_sorted_order() {
    let dir = unique_dir("expand");
    let _g = guard(dir.clone());

    let skip = dir.join("skip");
    std::fs::create_dir_all(&skip).unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::create_dir_all(dir.join(".hidden")).unwrap();

    let a = write_file(&dir, "a.jpg", b"a");
    let b = write_file(&dir, "b.png", b"b");
    let c = write_file(&dir, "sub/c.jpg", b"c");
    write_file(&dir.join(".hidden"), "h.jpg", b"h");
    write_file(&skip, "s.jpg", b"s");

    let paths = expand(&[dir.to_string_lossy().to_string()], Some(&skip));

    assert_eq!(paths, vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string(), c.to_string_lossy().to_string()]);
}

#[test]
fn expand_dedups_duplicate_input_paths() {
    let dir = unique_dir("expand_dedup");
    let _g = guard(dir.clone());
    write_file(&dir, "a.jpg", b"a");
    let p = dir.join("a.jpg").to_string_lossy().to_string();

    let out = expand(&[p.clone(), p.clone()], None);
    assert_eq!(out, vec![p]);
}

#[test]
fn expand_top_level_unknown_extension_is_still_included() {
    let dir = unique_dir("expand_unknown");
    let _g = guard(dir.clone());
    let f = write_file(&dir, "weird.xyz", b"x");

    let paths = expand(&[f.to_string_lossy().to_string()], None);
    assert_eq!(paths, vec![f.to_string_lossy().to_string()]);
}

#[test]
fn organize_parse_all_aliases_and_template() {
    assert_eq!(Organize::parse("date"), Some(Organize::ByDay));
    assert_eq!(Organize::parse("day"), Some(Organize::ByDay));
    assert_eq!(Organize::parse("byDay"), Some(Organize::ByDay));
    assert_eq!(Organize::parse("month"), Some(Organize::ByMonth));
    assert_eq!(Organize::parse("byMonth"), Some(Organize::ByMonth));
    assert_eq!(Organize::parse("flat"), Some(Organize::Flat));
    assert_eq!(Organize::parse("none"), Some(Organize::Flat));
    assert_eq!(Organize::parse("intoOneFolder"), Some(Organize::Flat));
    assert_eq!(Organize::parse("x/{date:%Y}"), Some(Organize::Template("x/{date:%Y}".to_string())));
    assert_eq!(Organize::parse("random"), None);
}

#[test]
fn organize_folders_by_day_month_flat_and_static_template() {
    let mut p = make_photo();
    p.captured = Some("2024-05-06T07:08:09".to_string());

    assert_eq!(Organize::ByDay.folders(&p), vec!["2024".to_string(), "2024-05-06".to_string()]);
    assert_eq!(Organize::ByMonth.folders(&p), vec!["2024".to_string(), "2024-05".to_string()]);
    assert!(Organize::Flat.folders(&p).is_empty());

    let t = Organize::Template("static".to_string());
    assert_eq!(t.folders(&p), vec!["static".to_string()]);
}

#[test]
fn civil_round_trip_epoch_leap_and_negative() {
    assert_eq!(civil(0), "1970-01-01T00:00:00");
    assert_eq!(civil(-1), "1969-12-31T23:59:59");
    assert_eq!(civil(951_782_400), "2000-02-29T00:00:00");

    let s = civil(1_700_000_000);
    assert_eq!(s.len(), 19);
    assert_eq!(&s[4..5], "-");
    assert_eq!(&s[7..8], "-");
    assert_eq!(&s[10..11], "T");
    assert_eq!(&s[13..14], ":");
    assert_eq!(&s[16..17], ":");
}

#[test]
fn system_clock_is_iso_utc_format() {
    let s = system_clock();
    assert_eq!(s.len(), 19);
    assert_eq!(&s[4..5], "-");
    assert_eq!(&s[7..8], "-");
    assert_eq!(&s[10..11], "T");
    assert!(s.starts_with("20"));
}

#[test]
fn import_candidate_serde_round_trip() {
    let c = ImportCandidate {
        path: "/a.jpg".to_string(),
        name: "a.jpg".to_string(),
        format: "JPEG".to_string(),
        width: 1,
        height: 2,
        file_size: 3,
        captured: Some("2026-01-01T00:00:00".to_string()),
        duplicate: Some("content".to_string()),
        existing: Some(4),
        error: Some("bad".to_string()),
        preview_only: Some("raw".to_string()),
        ..Default::default()
    };

    let json = serde_json::to_string(&c).unwrap();
    let decoded: ImportCandidate = serde_json::from_str(&json).unwrap();
    assert_eq!(c, decoded);
    assert!(json.contains("\"fileSize\""));
    assert!(json.contains("\"previewOnly\""));
}

#[test]
fn candidate_thumb_job_returns_none_for_error_candidate() {
    let mut s = Session::new();
    let c = ImportCandidate { error: Some("bad".to_string()), ..Default::default() };
    assert!(s.candidate_thumb_job(&c, 64, 0).is_none());
}

#[test]
fn import_defaults_preset_for_matches_raw_per_camera_and_other() {
    let defaults = ImportDefaults {
        raw_preset: Some("raw".to_string()),
        per_camera: true,
        cameras: vec![CameraDefault { camera: "Canon EOS R5".to_string(), preset: Some("canon".to_string()) }],
        other_preset: Some("other".to_string()),
        ..Default::default()
    };

    assert_eq!(defaults.preset_for(true, "CANON eos r5"), Some("canon"));
    assert_eq!(defaults.preset_for(true, "Nikon Z 6"), Some("raw"));
    assert_eq!(defaults.preset_for(true, ""), Some("raw"));
    assert_eq!(defaults.preset_for(false, "Canon EOS R5"), Some("other"));

    let no_camera = ImportDefaults {
        raw_preset: Some("raw".to_string()),
        cameras: vec![CameraDefault { camera: "Canon EOS R5".to_string(), preset: Some("canon".to_string()) }],
        per_camera: false,
        ..Default::default()
    };
    assert_eq!(no_camera.preset_for(true, "Canon EOS R5"), Some("raw"));
}

#[test]
fn apply_import_defaults_fills_only_empty_metadata_gaps() {
    let mut p = make_photo();
    p.meta.copyright = "  ".to_string();
    p.meta.creator = "Alice".to_string();

    let mut s = Session::new();
    s.import_defaults.copyright = "Bob".to_string();
    s.import_defaults.creator = "Carol".to_string();

    apply_import_defaults(&s, &mut p);

    assert_eq!(p.meta.copyright, "Bob");
    assert_eq!(p.meta.creator, "Alice");
}

#[test]
fn no_import_preset_keeps_photo_with_import_look() {
    let s = Session::new();
    let mut p = make_photo();
    apply_import_defaults(&s, &mut p);
    assert!(has_import_look(&p));
}

#[test]
fn import_add_round_trip_undo_redo_and_scan_duplicate_path() {
    let dir = unique_dir("import_add");
    let _g = guard(dir.clone());
    let f = write_file(&dir, "a.jpg", b"not a real jpeg");

    let mut s = Session::new();
    let report = import(&mut s, &[f.to_string_lossy().to_string()], ImportMode::Add).unwrap();

    assert_eq!(report.imported.len(), 1);
    let id = report.imported[0];
    assert!(s.catalog.photo(PhotoId(id)).is_some());

    let cands = scan(&mut s, &[f.to_string_lossy().to_string()]);
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].duplicate.as_deref(), Some("path"));
    assert_eq!(cands[0].existing, Some(id));

    let label = s.undo_step().unwrap();
    assert!(label.contains("Add 1 Photo"));
    assert!(s.catalog.photo(PhotoId(id)).is_none());

    s.redo_step().unwrap();
    assert!(s.catalog.photo(PhotoId(id)).is_some());
}

#[test]
fn import_copy_without_destination_or_library_errors() {
    let dir = unique_dir("copy_err");
    let _g = guard(dir.clone());
    let f = write_file(&dir, "a.jpg", b"x");

    let mut s = Session::new();
    let opts = ImportOptions { mode: ImportMode::Copy, ..Default::default() };

    let e = import::import_with(&mut s, &[f.to_string_lossy().to_string()], &opts).unwrap_err();
    assert!(matches!(
        e,
        EngineError::Other(ref msg) if msg.contains("destination") || msg.contains("copying")
    ));
}

#[test]
fn import_move_local_true_errors() {
    let dir = unique_dir("move_local_err");
    let _g = guard(dir.clone());
    let f = write_file(&dir, "a.jpg", b"x");

    let mut s = Session::new();
    let opts = ImportOptions { mode: ImportMode::Move, local: true, ..Default::default() };

    let e = import::import_with(&mut s, &[f.to_string_lossy().to_string()], &opts).unwrap_err();
    assert!(matches!(
        e,
        EngineError::Other(ref msg) if msg.contains("browsing a folder")
    ));
}

#[test]
fn scan_empty_no_paths_returns_empty_and_probes_empty() {
    let mut s = Session::new();
    let cands = scan(&mut s, &[]);
    assert!(cands.is_empty());
    assert!(s.import_probes.is_empty());
}

#[test]
fn prepared_default_is_empty_and_rollback_noop() {
    let p = Prepared::default();
    assert!(p.is_empty());
    assert_eq!(p.len(), 0);
    p.rollback();
}

#[test]
fn import_defaults_serde_round_trip() {
    let d = ImportDefaults {
        raw_preset: Some("raw".to_string()),
        per_camera: true,
        cameras: vec![CameraDefault { camera: "Canon EOS R5".to_string(), preset: Some("canon".to_string()) }],
        other_preset: Some("other".to_string()),
        copyright: "C".to_string(),
        creator: "D".to_string(),
        metadata_preset: Some("meta".to_string()),
        auto_folder: Some("auto".to_string()),
        auto_copy: true,
        auto_album: Some("album".to_string()),
        look: Default::default(),
        camera_profiles_folder: "profiles".to_string(),
    };

    let json = serde_json::to_string(&d).unwrap();
    let back: ImportDefaults = serde_json::from_str(&json).unwrap();
    assert_eq!(d, back);
}

#[test]
fn import_report_serialization_shape() {
    let r = ImportReport {
        imported: vec![1, 2],
        duplicates: vec![Duplicate { path: "/a.jpg".to_string(), existing: Some(3), reason: "path" }],
        moved: vec![Moved { from: "/src/a.jpg".to_string(), to: "/dst/a.jpg".to_string(), sidecars: vec!["/dst/a.xmp".to_string()] }],
        kept: vec![Kept { path: "/kept.jpg".to_string(), reason: "kept reason".to_string() }],
        failed: vec![("/fail.jpg".to_string(), "bad read".to_string())],
        scanned: 4,
        sidecars: 1,
    };

    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["imported"], serde_json::json!([1, 2]));
    assert_eq!(v["scanned"], 4);
    assert_eq!(v["sidecars"], 1);
    assert_eq!(v["duplicates"][0]["reason"], "path");
    assert_eq!(v["moved"][0]["from"], "/src/a.jpg");
    assert_eq!(v["failed"][0][0], "/fail.jpg");
}
