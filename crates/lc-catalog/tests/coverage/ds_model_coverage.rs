use lightcraft_catalog::{
    Album, AlbumId, Analysis, Catalog, CatalogError, ColorLabel, CopyrightStatus, Flag, HistoryStep, MediaKind, Meta, Op, Photo, PhotoId, Source,
    Stack, StackId, Version,
};
use serde_json::{Value, from_str, to_string};
use std::sync::Arc;
use std::{
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_temp_dir(test_name: &str) -> PathBuf {
    let mut dir = env::temp_dir();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    dir.push(format!("lc_cat_test_{}_{}_{}", test_name, std::process::id(), nanos));
    dir
}

#[test]
fn photo_id_transparent_serde_and_ord() {
    let id = PhotoId(42);
    let s = to_string(&id).unwrap();
    assert_eq!(s, "42");
    let back: PhotoId = from_str(&s).unwrap();
    assert_eq!(id, back);
    assert!(PhotoId(1) < PhotoId(2));
}

#[test]
fn album_id_and_stack_id_serde() {
    let a = AlbumId(7);
    let sa = to_string(&a).unwrap();
    assert_eq!(sa, "7");
    assert_eq!(from_str::<AlbumId>(&sa).unwrap(), a);

    let st = StackId(3);
    let ss = to_string(&st).unwrap();
    assert_eq!(ss, "3");
    assert_eq!(from_str::<StackId>(&ss).unwrap(), st);
}

#[test]
fn media_kind_serde_lowercase() {
    assert_eq!(to_string(&MediaKind::Image).unwrap(), r#""image""#);
    assert_eq!(to_string(&MediaKind::Raw).unwrap(), r#""raw""#);
    assert_eq!(to_string(&MediaKind::Video).unwrap(), r#""video""#);

    assert_eq!(from_str::<MediaKind>(r#""raw""#).unwrap(), MediaKind::Raw);
    assert_eq!(from_str::<MediaKind>(r#""video""#).unwrap(), MediaKind::Video);
    assert!(from_str::<MediaKind>(r#""Bogus""#).is_err());
}

#[test]
fn flag_parse_and_serde() {
    assert_eq!(Flag::parse("pick"), Some(Flag::Pick));
    assert_eq!(Flag::parse("picked"), Some(Flag::Pick));
    assert_eq!(Flag::parse("flagged"), Some(Flag::Pick));
    assert_eq!(Flag::parse("Reject"), Some(Flag::Reject));
    assert_eq!(Flag::parse("rejected"), Some(Flag::Reject));
    assert_eq!(Flag::parse("none"), Some(Flag::None));
    assert_eq!(Flag::parse("unflagged"), Some(Flag::None));
    assert_eq!(Flag::parse(""), Some(Flag::None));
    assert_eq!(Flag::parse("bogus"), None);

    assert_eq!(to_string(&Flag::Pick).unwrap(), r#""pick""#);
    assert_eq!(from_str::<Flag>(r#""reject""#).unwrap(), Flag::Reject);
    assert!(from_str::<Flag>(r#""unknown""#).is_err());
}

#[test]
fn color_label_parse_and_all() {
    assert_eq!(ColorLabel::ALL.len(), 5);
    assert_eq!(ColorLabel::parse("Red"), Some(ColorLabel::Red));
    assert_eq!(ColorLabel::parse("red"), Some(ColorLabel::Red));
    assert_eq!(ColorLabel::parse("YELLOW"), Some(ColorLabel::Yellow));
    assert_eq!(ColorLabel::parse("green"), Some(ColorLabel::Green));
    assert_eq!(ColorLabel::parse("Blue"), Some(ColorLabel::Blue));
    assert_eq!(ColorLabel::parse("Purple"), Some(ColorLabel::Purple));
    assert_eq!(ColorLabel::parse("pink"), None);

    assert_eq!(to_string(&ColorLabel::Red).unwrap(), r#""red""#);
    assert_eq!(from_str::<ColorLabel>(r#""yellow""#).unwrap(), ColorLabel::Yellow);
}

#[test]
fn copyright_status_parse_label_marked() {
    assert_eq!(CopyrightStatus::parse("unknown"), Some(CopyrightStatus::Unknown));
    assert_eq!(CopyrightStatus::parse(""), Some(CopyrightStatus::Unknown));
    assert_eq!(CopyrightStatus::parse("copyrighted"), Some(CopyrightStatus::Copyrighted));
    assert_eq!(CopyrightStatus::parse("publicDomain"), Some(CopyrightStatus::PublicDomain));
    assert_eq!(CopyrightStatus::parse("public domain"), Some(CopyrightStatus::PublicDomain));
    assert_eq!(CopyrightStatus::parse("public-domain"), Some(CopyrightStatus::PublicDomain));
    assert_eq!(CopyrightStatus::parse("bogus"), None);

    assert_eq!(CopyrightStatus::Unknown.marked(), None);
    assert_eq!(CopyrightStatus::Copyrighted.marked(), Some(true));
    assert_eq!(CopyrightStatus::PublicDomain.marked(), Some(false));
    assert_eq!(CopyrightStatus::from_marked(None), CopyrightStatus::Unknown);
    assert_eq!(CopyrightStatus::from_marked(Some(true)), CopyrightStatus::Copyrighted);
    assert_eq!(CopyrightStatus::from_marked(Some(false)), CopyrightStatus::PublicDomain);
    assert!(CopyrightStatus::Unknown.is_unknown());
    assert_eq!(CopyrightStatus::PublicDomain.id(), "publicDomain");
    assert_eq!(CopyrightStatus::Copyrighted.label(), "Copyrighted");
}

#[test]
fn source_serde_tagged() {
    let file = Source::File { path: "/tmp/photo.jpg".into() };
    let s = to_string(&file).unwrap();
    let v: Value = from_str(&s).unwrap();
    assert_eq!(v["type"], "file");
    assert_eq!(v["path"], "/tmp/photo.jpg");
    assert_eq!(from_str::<Source>(&s).unwrap(), file);

    let demo = Source::Demo { scene: 42 };
    let s2 = to_string(&demo).unwrap();
    let v2: Value = from_str(&s2).unwrap();
    assert_eq!(v2["type"], "demo");
    assert_eq!(v2["scene"], 42);
    assert_eq!(from_str::<Source>(&s2).unwrap(), demo);
}

#[test]
fn meta_default_skips_empty_and_roundtrip() {
    let m = Meta::default();
    let s = to_string(&m).unwrap();
    let v: Value = from_str(&s).unwrap();
    // copyright_status skipped because Unknown, regions skipped because empty Vec
    assert!(v.get("copyright_status").is_none());
    assert!(v.get("regions").is_none());
    // keywords is not skipped (no skip_serializing_if), so it appears as an empty array
    let keywords = v.get("keywords").expect("keywords field should be present");
    assert!(keywords.is_array());
    assert_eq!(keywords.as_array().unwrap().len(), 0);
    assert_eq!(v["camera"], "");

    let m2 = Meta {
        camera: "Sony".into(),
        focal_mm: Some(35.0),
        gps: Some((12.34, 56.78)),
        copyright_status: CopyrightStatus::Copyrighted,
        keywords: vec!["tag".into()],
        ..Meta::default()
    };
    let s2 = to_string(&m2).unwrap();
    let back: Meta = from_str(&s2).unwrap();
    assert_eq!(m2, back);
    assert_eq!(back.copyright_status, CopyrightStatus::Copyrighted);
}

#[test]
fn photo_new_defaults_and_methods() {
    let p = Photo::new(PhotoId(10), Source::Demo { scene: 1 }, "demo.jpg", "JPEG", 800, 600, "2026-01-02T10:00:00");
    assert_eq!(p.id, PhotoId(10));
    assert_eq!(p.kind, MediaKind::Image);
    assert_eq!(p.rating, 0);
    assert_eq!(p.flag, Flag::None);
    assert_eq!(p.label, None);
    assert_eq!(p.file_size, 0);
    assert_eq!(p.captured, None);
    assert_eq!(p.date(), "2026-01-02T10:00:00");
    assert!(p.in_library());
    assert!(!p.develops_raw());
    assert!(!p.is_edited());
}

#[test]
fn photo_captured_date_precedence() {
    let mut p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 10, 10, "2026-01-02T10:00:00");
    p.captured = Some("2025-12-31T23:59:00".into());
    assert_eq!(p.date(), "2025-12-31T23:59:00");
    p.captured = None;
    assert_eq!(p.date(), "2026-01-02T10:00:00");
}

#[test]
fn photo_in_library_conditions() {
    let mut p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    assert!(p.in_library());
    p.deleted = true;
    assert!(!p.in_library());
    p.deleted = false;
    p.local = true;
    assert!(!p.in_library());
}

#[test]
fn photo_develops_raw() {
    let mut p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.arw", "ARW", 10, 10, "2026-01-01T00:00:00");
    p.kind = MediaKind::Raw;
    assert!(p.develops_raw());
    p.preview_only = Some("unsupported".into());
    assert!(!p.develops_raw());
    p.preview_only = None;
    p.kind = MediaKind::Image;
    assert!(!p.develops_raw());
}

#[test]
fn photo_relative_wb_formats() {
    for (format, expected) in [("ARW", true), ("NEF", true), ("NRW", true), ("RW2", true), ("RWL", true), ("RAW", true), ("DNG", false)] {
        let mut p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, &format!("a.{}", format.to_lowercase()), format, 10, 10, "2026-01-01T00:00:00");
        p.kind = MediaKind::Raw;
        p.as_shot_wb = Some((5200.0, 4.0));
        assert_eq!(p.relative_wb(), expected, "format {format}");
        p.measured_wb = true;
        assert!(!p.relative_wb(), "measured_wb true for {format}");
    }
}

#[test]
fn stack_top_position_and_serde() {
    let s = Stack { id: StackId(1), photos: vec![PhotoId(10), PhotoId(20), PhotoId(30)], collapsed: true };
    assert_eq!(s.top(), PhotoId(10));
    assert_eq!(s.position(PhotoId(20)), Some(1));
    assert_eq!(s.position(PhotoId(99)), None);

    let json = to_string(&s).unwrap();
    let back: Stack = from_str(&json).unwrap();
    assert_eq!(s, back);
    assert!(back.collapsed);
}

#[test]
fn analysis_default_serde_and_non_finite_handling() {
    let a = Analysis::default();
    assert_eq!(a.sharpness, 0.0);
    assert_eq!(a.clipped, 0.0);
    assert_eq!(a.group, None);
    assert!(!a.best);

    let json = to_string(&a).unwrap();
    let back: Analysis = from_str(&json).unwrap();
    assert_eq!(a, back);

    // serde_json serializes non-finite floats as null (JSON cannot represent them)
    let nan = Analysis { sharpness: f32::NAN, ..Analysis::default() };
    let s = to_string(&nan).unwrap();
    let v: Value = from_str(&s).unwrap();
    assert!(v["sharpness"].is_null());

    let inf = Analysis { clipped: f32::INFINITY, ..Analysis::default() };
    let s2 = to_string(&inf).unwrap();
    let v2: Value = from_str(&s2).unwrap();
    assert!(v2["clipped"].is_null());

    // Deserializing null into a non-optional f32 should fail
    assert!(from_str::<Analysis>(&s).is_err());
    assert!(from_str::<Analysis>(&s2).is_err());
}

#[test]
fn album_new_is_smart_and_serde() {
    let a = Album::new(AlbumId(5), "Trip");
    assert!(!a.is_smart());
    assert_eq!(a.photos, Vec::<PhotoId>::new());

    let mut b = Album::new(AlbumId(6), "Favorites");
    b.photos = vec![PhotoId(1), PhotoId(2)];
    b.cover = Some(PhotoId(1));
    let json = to_string(&b).unwrap();
    let back: Album = from_str(&json).unwrap();
    assert_eq!(b, back);
}

#[test]
fn version_and_history_step_serde() {
    let photo = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    let dev: Arc<_> = photo.develop.clone();

    let v = Version { name: "v1".into(), created: "2026-01-01".into(), settings: dev.clone(), auto: true };
    let h = HistoryStep { label: "edit".into(), settings: dev.clone() };

    let vj = to_string(&v).unwrap();
    let hj = to_string(&h).unwrap();
    let v2: Version = from_str(&vj).unwrap();
    let h2: HistoryStep = from_str(&hj).unwrap();

    assert_eq!(v.name, v2.name);
    assert_eq!(v.created, v2.created);
    assert_eq!(v.auto, v2.auto);
    assert_eq!(h.label, h2.label);
}

#[test]
fn photo_serde_roundtrip_with_optional_fields() {
    let mut p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.arw", "ARW", 10, 10, "2026-01-01T00:00:00");
    p.kind = MediaKind::Raw;
    p.file_size = 12345;
    p.captured = Some("2025-01-01T00:00:00".into());
    p.duration = Some(1.5);
    p.as_shot_wb = Some((5200.0, 4.0));
    p.content_hash = Some("abc123".into());
    p.copy_of = Some(PhotoId(2));
    p.copy_name = Some("Copy 1".into());
    p.local = true;
    p.deleted = true;

    let json = to_string(&p).unwrap();
    let back: Photo = from_str(&json).unwrap();
    assert_eq!(p, back);
    assert_eq!(back.duration, Some(1.5));
    assert_eq!(back.as_shot_wb, Some((5200.0, 4.0)));
    assert_eq!(back.copy_of, Some(PhotoId(2)));
}

#[test]
fn photo_json_file_roundtrip() {
    let dir = unique_temp_dir("photo_json");
    fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join("photo.json");

    let p = Photo::new(PhotoId(7), Source::Demo { scene: 3 }, "demo.jpg", "JPEG", 640, 480, "2026-03-01T08:00:00");
    fs::write(&file_path, to_string(&p).unwrap()).unwrap();
    let back: Photo = from_str(&fs::read_to_string(&file_path).unwrap()).unwrap();
    assert_eq!(p, back);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn catalog_apply_add_remove_photo_roundtrip() {
    let mut cat = Catalog::new();
    let photo = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    let inv = cat.apply(Op::AddPhoto { photo: Box::new(photo.clone()) }).unwrap();
    assert!(cat.photo(PhotoId(1)).is_some());
    assert_eq!(cat.len(), 1);

    match inv {
        Op::RemovePhoto { id } => assert_eq!(id, PhotoId(1)),
        _ => panic!("expected RemovePhoto inverse"),
    }

    cat.apply(inv).unwrap();
    assert_eq!(cat.len(), 0);
    assert!(cat.photo(PhotoId(1)).is_none());
}

#[test]
fn catalog_apply_set_rating_validation() {
    let mut cat = Catalog::new();
    let p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    cat.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();

    let inv = cat.apply(Op::SetRating { id: PhotoId(1), rating: 4 }).unwrap();
    assert_eq!(cat.photo(PhotoId(1)).unwrap().rating, 4);
    match inv {
        Op::SetRating { id, rating } => {
            assert_eq!(id, PhotoId(1));
            assert_eq!(rating, 0);
        }
        _ => panic!("expected SetRating inverse"),
    }

    let err = cat.apply(Op::SetRating { id: PhotoId(1), rating: 6 }).unwrap_err();
    assert_eq!(err, CatalogError::Invalid("rating must be 0..=5".into()));
    assert_eq!(cat.photo(PhotoId(1)).unwrap().rating, 4);
}

#[test]
fn catalog_apply_batch_rollback() {
    let mut cat = Catalog::new();
    let p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    let ops = vec![Op::AddPhoto { photo: Box::new(p.clone()) }, Op::SetRating { id: PhotoId(1), rating: 6 }];
    let err = cat.apply(Op::Batch { ops }).unwrap_err();
    assert_eq!(err, CatalogError::Invalid("rating must be 0..=5".into()));
    assert_eq!(cat.len(), 0);
    assert!(cat.photo(PhotoId(1)).is_none());
}

#[test]
fn catalog_snapshot_roundtrip() {
    let mut cat = Catalog::new();
    let p = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    cat.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();

    let snap = cat.to_snapshot();
    let restored = Catalog::from_snapshot(&snap).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored.photo(PhotoId(1)).unwrap().id, PhotoId(1));
    assert_eq!(restored.photo(PhotoId(1)).unwrap().file_name, "a.jpg");
}

#[test]
fn catalog_replay_torn_final_line_ignored() {
    let mut cat = Catalog::new();
    let photo1 = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    let photo2 = Photo::new(PhotoId(2), Source::Demo { scene: 1 }, "b.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    let mut log = String::new();
    log.push_str(&Catalog::op_to_log_line(&Op::AddPhoto { photo: Box::new(photo1) }));
    log.push_str(&Catalog::op_to_log_line(&Op::AddPhoto { photo: Box::new(photo2) }));
    log.push_str("not-json");

    let n = cat.replay(&log).unwrap();
    assert_eq!(n, 2);
    assert_eq!(cat.len(), 2);
}

#[test]
fn catalog_op_to_log_line_ends_newline() {
    let op = Op::SetRating { id: PhotoId(1), rating: 5 };
    let line = Catalog::op_to_log_line(&op);
    assert!(line.ends_with('\n'));
    assert_eq!(from_str::<Op>(&line).unwrap(), op);
}

#[test]
fn catalog_replay_corrupt_middle_line_returns_err() {
    let mut cat = Catalog::new();
    let photo1 = Photo::new(PhotoId(1), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    let photo2 = Photo::new(PhotoId(2), Source::Demo { scene: 1 }, "b.jpg", "JPEG", 10, 10, "2026-01-01T00:00:00");
    let mut log = String::new();
    log.push_str(&Catalog::op_to_log_line(&Op::AddPhoto { photo: Box::new(photo1) }));
    log.push_str("not-json\n");
    log.push_str(&Catalog::op_to_log_line(&Op::AddPhoto { photo: Box::new(photo2) }));

    let res = cat.replay(&log);
    assert!(res.is_err());
    assert!(matches!(res.err().unwrap(), CatalogError::Corrupt(_)));
}
