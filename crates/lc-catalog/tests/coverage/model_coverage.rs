use lightcraft_catalog::{Catalog, CatalogError, ColorLabel, CopyrightStatus, Flag, MediaKind, Meta, Op, Photo, PhotoId, Source, Stack, StackId};
use serde_json::{Value, from_str, to_string};

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
