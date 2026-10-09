use std::fs;
use std::path::{Path, PathBuf};

use lightcraft_engine::Session;
use lightcraft_engine::catalog::{Catalog, ColorLabel, Flag, MediaKind, Meta, Photo, PhotoId, Source};
use lightcraft_engine::develop::{DevelopSettings, Mask};
use lightcraft_engine::sidecar::{
    DevelopPatch, SidecarData, SidecarNaming, XmpPrefs, find_sidecar, merge_into, parse_sidecar, read_packet, sidecar_packet, sidecar_path,
};

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let mut p = std::env::temp_dir();
        let unique = format!(
            "lc_sidecar_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        p.push(unique);
        fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn make_photo(id: u64, path: &str, develop: Option<DevelopSettings>) -> Photo {
    let mut p = Photo::new(PhotoId(id), Source::File { path: path.into() }, "IMG_1.jpg", "JPEG", 40, 30, "2026-01-01T00:00:00");
    p.rating = 4;
    p.flag = Flag::Pick;
    p.label = Some(ColorLabel::Purple);
    p.captured = Some("2025-06-07T08:09:10".into());
    p.meta = Meta {
        title: "Dune & sky".into(),
        caption: "Line <one>\nline two".into(),
        copyright: "© 2026 Me".into(),
        creator: "Ann Lee".into(),
        location: "Namib".into(),
        keywords: vec!["desert".into(), "red".into()],
        gps: Some((-24.75, 15.3)),
        ..Default::default()
    };
    if let Some(d) = develop {
        p.develop = std::sync::Arc::new(d);
    }
    p
}

fn default_develop_with_changes() -> DevelopSettings {
    let mut d = DevelopSettings::default();
    d.light.exposure = 0.75;
    d.effects.clarity = 22.0;
    d.crop.geometry.angle = 2.5;
    d.masks.push(Mask { id: 1, ..Default::default() });
    d
}

#[test]
fn sidecar_path_stem_and_full() {
    assert_eq!(sidecar_path("/a/b/IMG_1.CR3", SidecarNaming::Stem), PathBuf::from("/a/b/IMG_1.xmp"));
    assert_eq!(sidecar_path("/a/b/IMG_1.CR3", SidecarNaming::Full), PathBuf::from("/a/b/IMG_1.CR3.xmp"));
}

#[test]
fn defaults_are_sensible() {
    assert_eq!(SidecarNaming::default(), SidecarNaming::Stem);
    let prefs = XmpPrefs::default();
    assert!(!prefs.auto_write);
    assert_eq!(prefs.naming, SidecarNaming::Stem);
}

#[test]
fn find_sidecar_prefers_configured_naming_and_case() {
    let td = TempDir::new();
    let original = td.path().join("IMG_1.CR3");
    let original_str = original.to_str().unwrap();
    let stem_xmp = td.path().join("IMG_1.xmp");
    let stem_upper = td.path().join("IMG_1.XMP");
    let full_xmp = td.path().join("IMG_1.CR3.xmp");

    // No sidecar
    assert_eq!(find_sidecar(original_str, SidecarNaming::Stem), None);

    // Prefer configured naming, exact case
    fs::write(&stem_xmp, b"x").unwrap();
    assert_eq!(find_sidecar(original_str, SidecarNaming::Stem), Some(stem_xmp.clone()));
    fs::remove_file(&stem_xmp).unwrap();

    // Upper-case extension accepted
    fs::write(&stem_upper, b"x").unwrap();
    assert_eq!(find_sidecar(original_str, SidecarNaming::Stem), Some(stem_upper.clone()));
    fs::remove_file(&stem_upper).unwrap();

    // Fallback to other naming
    fs::write(&full_xmp, b"x").unwrap();
    assert_eq!(find_sidecar(original_str, SidecarNaming::Stem), Some(full_xmp.clone()));
}

#[test]
fn parse_sidecar_minimal_valid_returns_defaults() {
    let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"></rdf:RDF></x:xmpmeta>"#;
    let sc = parse_sidecar(xmp, false).unwrap();
    assert_eq!(sc.rating, None);
    assert_eq!(sc.flag, None);
    assert_eq!(sc.label, None);
    assert_eq!(sc.title, None);
    assert_eq!(sc.keywords, None);
    assert!(sc.develop.is_none());
}

#[test]
fn parse_sidecar_reads_rating_label_flag_from_packet() {
    let mut p = make_photo(7, "/x/IMG_1.jpg", None);
    p.rating = 3;
    p.flag = Flag::Reject;
    p.label = Some(ColorLabel::Red);
    let packet = sidecar_packet(&p, &Catalog::new());
    let sc = parse_sidecar(&packet, false).unwrap();
    assert_eq!(sc.rating, Some(3));
    assert_eq!(sc.flag, Some(Flag::Reject));
    assert_eq!(sc.label, Some(Some(ColorLabel::Red)));
}

#[test]
fn parse_sidecar_rating_reject_and_clamps() {
    let xmp_neg = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="-1"/></rdf:RDF></x:xmpmeta>"#;
    let sc = parse_sidecar(xmp_neg, false).unwrap();
    assert_eq!(sc.rating, Some(0));
    assert_eq!(sc.flag, Some(Flag::Reject));

    let xmp_high = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="9"/></rdf:RDF></x:xmpmeta>"#;
    let sc = parse_sidecar(xmp_high, false).unwrap();
    assert_eq!(sc.rating, Some(5));
    assert_eq!(sc.flag, None);
}

#[test]
fn parse_sidecar_develop_full_via_sidecar_packet() {
    let p = make_photo(7, "/x/IMG_1.jpg", Some(default_develop_with_changes()));
    let packet = sidecar_packet(&p, &Catalog::new());
    let sc = parse_sidecar(&packet, false).unwrap();
    match sc.develop {
        Some(DevelopPatch::Full(d)) => {
            assert_eq!(d.light.exposure, p.develop.light.exposure);
            assert_eq!(d.effects.clarity, p.develop.effects.clarity);
            assert_eq!(d.crop.geometry.angle, p.develop.crop.geometry.angle);
            assert_eq!(d.masks.len(), p.develop.masks.len());
        }
        other => panic!("expected Full develop, got {:?}", other),
    }
}

#[test]
fn parse_sidecar_develop_partial_crs() {
    let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
        crs:Exposure2012="-0.40" crs:Vibrance="+12"/></rdf:RDF></x:xmpmeta>"#;
    let sc = parse_sidecar(xmp, true).unwrap();
    match sc.develop {
        Some(DevelopPatch::Partial(_)) => {}
        other => panic!("expected Partial develop, got {:?}", other),
    }
}

#[test]
fn merge_into_no_sidecar_data_leaves_photo_unchanged() {
    let mut p = make_photo(1, "/x/IMG_1.jpg", None);
    let before = p.clone();
    let sc = SidecarData::default();
    let changed = merge_into(&mut p, &sc, "now");
    assert!(!changed);
    assert_eq!(p.rating, before.rating);
    assert_eq!(p.flag, before.flag);
    assert_eq!(p.label, before.label);
    assert_eq!(p.meta, before.meta);
    assert_eq!(p.captured, before.captured);
    assert_eq!(p.develop, before.develop);
}

#[test]
fn merge_into_sets_fields_and_captured_only_when_missing() {
    let mut p = make_photo(1, "/x/IMG_1.jpg", None);
    p.captured = Some("existing".into());
    let sc = SidecarData { rating: Some(2), title: Some("New Title".into()), captured: Some("new capture".into()), ..Default::default() };

    let changed = merge_into(&mut p, &sc, "now");
    assert!(!changed); // no develop change
    assert_eq!(p.rating, 2);
    assert_eq!(p.meta.title, "New Title");
    assert_eq!(p.captured.as_deref(), Some("existing"));

    let mut p2 = make_photo(2, "/x/IMG_2.jpg", None);
    p2.captured = None;
    merge_into(&mut p2, &sc, "now");
    assert_eq!(p2.captured.as_deref(), Some("new capture"));
}

#[test]
fn merge_into_develop_full_sets_develop_and_edited() {
    let mut p = make_photo(1, "/x/IMG_1.jpg", None);
    let changed_settings = default_develop_with_changes();
    let sc = SidecarData { develop: Some(DevelopPatch::Full(Box::new(changed_settings.clone()))), ..Default::default() };

    let changed = merge_into(&mut p, &sc, "2026-10-01T00:00:00");
    assert!(changed);
    assert_eq!(*p.develop, changed_settings);
    assert_eq!(p.edited.as_deref(), Some("2026-10-01T00:00:00"));
}

#[test]
fn sidecar_packet_roundtrip_restores_all_fields() {
    let p = make_photo(7, "/x/IMG_1.jpg", Some(default_develop_with_changes()));
    let packet = sidecar_packet(&p, &Catalog::new());
    let sc = parse_sidecar(&packet, false).unwrap();

    let mut q = Photo::new(PhotoId(7), p.source.clone(), "IMG_1.jpg", "JPEG", 40, 30, "2026-01-01T00:00:00");
    q.captured = p.captured.clone();
    q.meta.gps = p.meta.gps;
    assert!(merge_into(&mut q, &sc, "now"));
    assert_eq!(q.develop, p.develop);
    assert_eq!(q.meta, p.meta);
    assert_eq!((q.rating, q.flag, q.label), (p.rating, p.flag, p.label));
    assert_eq!(q.edited.as_deref(), Some("now"));
}

#[test]
fn sidecar_packet_lc_properties_are_parsed_back() {
    let mut p = make_photo(1, "/x/IMG_1.jpg", None);
    p.flag = Flag::Pick;
    p.meta.location = "Namib".into();
    let packet = sidecar_packet(&p, &Catalog::new());
    let sc = parse_sidecar(&packet, false).unwrap();
    assert_eq!(sc.flag, Some(Flag::Pick));
    assert_eq!(sc.location, Some("Namib".to_string()));
    // Develop settings are also present
    assert!(matches!(sc.develop, Some(DevelopPatch::Full(_))));
}

#[test]
fn sidecar_data_resolve_label_empty_clears_label() {
    let sc = SidecarData { label_text: Some("".into()), ..Default::default() };
    let resolved = sc.resolve_label(&Catalog::new());
    assert_eq!(resolved.label, Some(None));
}

#[test]
fn read_packet_finds_sidecar_for_raw() {
    let td = TempDir::new();
    let original = td.path().join("IMG_1.jpg");
    let original_str = original.to_str().unwrap();
    let sidecar = td.path().join("IMG_1.xmp");
    fs::write(&sidecar, "dummy xmp").unwrap();
    let res = read_packet(original_str, MediaKind::Raw, SidecarNaming::Stem);
    assert_eq!(res, Some(("dummy xmp".to_string(), sidecar.clone())));
}

#[test]
fn read_packet_no_sidecar_returns_none_for_raw_without_embedded_xmp() {
    let td = TempDir::new();
    let original = td.path().join("IMG_1.jpg");
    let original_str = original.to_str().unwrap();
    // No sidecar, original file doesn't exist -> embedded extraction fails -> None
    let res = read_packet(original_str, MediaKind::Raw, SidecarNaming::Stem);
    assert_eq!(res, None);
}

#[test]
fn session_sidecar_naming_default_and_non_existent_photo() {
    let mut session = Session::new();
    assert_eq!(session.sidecar_naming(PhotoId(123)), SidecarNaming::Stem);
    session.xmp.naming = SidecarNaming::Full;
    assert_eq!(session.sidecar_naming(PhotoId(123)), SidecarNaming::Full);
}

#[test]
fn session_save_sidecar_no_photo_errors() {
    let session = Session::new();
    assert!(session.save_sidecar(PhotoId(123)).is_err());
}

#[test]
fn session_read_sidecar_op_no_photo_errors() {
    let session = Session::new();
    assert!(session.read_sidecar_op(PhotoId(123)).is_err());
}

// Text with no XML markup is not an XMP packet and is rejected (import logs and skips it; an explicit
// sidecar read reports it). Markup with no recognised properties stays a valid, empty sidecar.
#[test]
fn parse_sidecar_malformed_returns_err() {
    assert!(parse_sidecar("not a valid xmp", false).is_err());
    assert!(parse_sidecar("", false).is_err());
}
