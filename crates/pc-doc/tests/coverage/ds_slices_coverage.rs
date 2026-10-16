use photocraft_doc::{
    ColorMode, Document, LayerId, Rect, SampleType, Size, Slice, SliceKind, SliceOrigin, Slices,
    slices::{auto_slices, base_name, resolve},
};

#[test]
fn slice_origin_code_round_trip() {
    for origin in [SliceOrigin::Auto, SliceOrigin::Layer, SliceOrigin::User] {
        assert_eq!(SliceOrigin::from_code(origin.code()), origin);
    }
    assert_eq!(SliceOrigin::from_code(0), SliceOrigin::Auto);
    assert_eq!(SliceOrigin::from_code(1), SliceOrigin::Layer);
    assert_eq!(SliceOrigin::from_code(999), SliceOrigin::User);
    assert_eq!(SliceOrigin::Auto.id(), "auto");
    assert_eq!(SliceOrigin::Layer.id(), "layer");
    assert_eq!(SliceOrigin::User.id(), "user");
}

#[test]
fn slice_kind_code_round_trip() {
    for kind in [SliceKind::NoImage, SliceKind::Image, SliceKind::Table] {
        assert_eq!(SliceKind::from_code(kind.code()), kind);
    }
    assert_eq!(SliceKind::from_code(3), SliceKind::Image);
    assert_eq!(SliceKind::from_code(999), SliceKind::Image);
    assert_eq!(SliceKind::Image.id(), "image");
    assert_eq!(SliceKind::NoImage.id(), "noImage");
    assert_eq!(SliceKind::Table.id(), "table");
}

#[test]
fn slice_kind_from_id_accepts_aliases() {
    assert_eq!(SliceKind::from_id("image"), Some(SliceKind::Image));
    assert_eq!(SliceKind::from_id("img"), Some(SliceKind::Image));
    assert_eq!(SliceKind::from_id("noImage"), Some(SliceKind::NoImage));
    assert_eq!(SliceKind::from_id("none"), Some(SliceKind::NoImage));
    assert_eq!(SliceKind::from_id("no-image"), Some(SliceKind::NoImage));
    assert_eq!(SliceKind::from_id("table"), Some(SliceKind::Table));
    assert_eq!(SliceKind::from_id("bogus"), None);
    assert_eq!(SliceKind::from_id(""), None);
}

#[test]
fn slice_default_values() {
    let s = Slice::default();
    assert_eq!(s.id, 0);
    assert_eq!(s.group_id, 0);
    assert_eq!(s.origin, SliceOrigin::User);
    assert_eq!(s.layer, None);
    assert_eq!(s.name, "");
    assert_eq!(s.rect, Rect::default());
    assert_eq!(s.kind, SliceKind::Image);
    assert_eq!(s.url, "");
    assert_eq!(s.target, "");
    assert_eq!(s.message, "");
    assert_eq!(s.alt, "");
    assert!(!s.cell_text_is_html);
    assert_eq!(s.cell_text, "");
    assert_eq!(s.horizontal_align, 0);
    assert_eq!(s.vertical_align, 0);
    assert_eq!(s.background, None);
    assert_eq!(s.outsets, [0; 4]);
}

#[test]
fn slices_next_id_basic() {
    let empty = Slices::default();
    assert_eq!(empty.next_id(), Some(1));

    let s = Slices { list: vec![Slice { id: 3, ..Default::default() }, Slice { id: 1, ..Default::default() }], ..Default::default() };
    assert_eq!(s.next_id(), Some(4));
}

#[test]
fn slices_next_id_exhausted() {
    let s = Slices { list: vec![Slice { id: u32::MAX - 1, ..Default::default() }], ..Default::default() };
    assert_eq!(s.next_id(), Some(u32::MAX));

    let exhausted = Slices { list: vec![Slice { id: u32::MAX, ..Default::default() }], ..Default::default() };
    assert_eq!(exhausted.next_id(), None);
}

#[test]
fn slices_get_and_get_mut() {
    let mut s = Slices { list: vec![Slice { id: 7, name: "a".into(), ..Default::default() }], ..Default::default() };
    assert_eq!(s.get(7).unwrap().name, "a");
    assert!(s.get(8).is_none());

    s.get_mut(7).unwrap().name = "b".into();
    assert_eq!(s.get(7).unwrap().name, "b");
}

#[test]
fn slices_serialization_round_trip() {
    let slice = Slice {
        id: 12,
        group_id: 3,
        origin: SliceOrigin::Layer,
        layer: Some(LayerId(42)),
        name: "hero".into(),
        rect: Rect::new(-10, 20, 300, 400),
        kind: SliceKind::NoImage,
        url: "https://example.com".into(),
        target: "_blank".into(),
        message: "Hello".into(),
        alt: "Alt".into(),
        cell_text_is_html: true,
        cell_text: "<b>x</b>".into(),
        horizontal_align: 2,
        vertical_align: 1,
        background: Some([1, 2, 3, 4]),
        outsets: [1, -2, 3, -4],
    };
    let slices = Slices { group_name: "site".into(), list: vec![slice] };

    let json = serde_json::to_string(&slices).unwrap();
    let loaded: Slices = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded, slices);
}

#[test]
fn slice_deserialization_missing_fields_defaults() {
    let loaded: Slice = serde_json::from_str("{}").unwrap();
    assert_eq!(loaded, Slice::default());
}

#[test]
fn malformed_slice_json_returns_err() {
    assert!(serde_json::from_str::<Slice>("not json").is_err());
    assert!(serde_json::from_str::<Slices>(r#"{"list":[{"id":"oops"}]}"#).is_err());
}

#[test]
fn auto_slices_empty_canvas_returns_empty() {
    let empty_width = Rect::new(10, 10, 10, 20);
    assert!(empty_width.is_empty());
    assert_eq!(auto_slices(empty_width, &[]), Vec::<Rect>::new());

    let empty_height = Rect::new(0, 0, 10, 0);
    assert!(empty_height.is_empty());
    assert_eq!(auto_slices(empty_height, &[]), Vec::<Rect>::new());

    let zero = Rect::new(0, 0, 0, 0);
    assert_eq!(auto_slices(zero, &[]), Vec::<Rect>::new());
}

#[test]
fn auto_slices_taken_covering_full_canvas_returns_empty() {
    let canvas = Rect::new(0, 0, 10, 20);
    let taken = [canvas];
    assert_eq!(auto_slices(canvas, &taken), Vec::<Rect>::new());
}

#[test]
fn auto_slices_partitions_canvas_around_user_slice() {
    let canvas = Rect::new(0, 0, 100, 80);
    let user = Rect::new(20, 10, 60, 30);
    let auto = auto_slices(canvas, &[user]);

    let auto_area: i64 = auto.iter().map(|r| i64::from(r.width()) * i64::from(r.height())).sum();
    let user_area = i64::from(user.width()) * i64::from(user.height());
    assert_eq!(auto_area + user_area, 100 * 80);

    for (i, a) in auto.iter().enumerate() {
        assert!(a.intersect(&user).is_empty());
        for b in &auto[i + 1..] {
            assert!(a.intersect(b).is_empty(), "{a:?} overlaps {b:?}");
        }
    }
    assert!(!auto.is_empty());
}

#[test]
fn auto_slices_taken_outside_canvas_ignored() {
    let canvas = Rect::new(0, 0, 10, 10);

    let outside = Rect::new(20, 20, 30, 30);
    assert!(outside.intersect(&canvas).is_empty());
    assert_eq!(auto_slices(canvas, &[outside]), vec![canvas]);

    let partial = Rect::new(-5, -5, 5, 5);
    let auto = auto_slices(canvas, &[partial]);
    let auto_area: i64 = auto.iter().map(|r| i64::from(r.width()) * i64::from(r.height())).sum();
    let clipped = partial.intersect(&canvas);
    assert_eq!(auto_area + i64::from(clipped.width()) * i64::from(clipped.height()), 100);
    assert!(auto.iter().all(|r| r.intersect(&clipped).is_empty()));
}

#[test]
fn auto_slices_merges_vertically_adjacent_free_runs() {
    let canvas = Rect::new(0, 0, 100, 100);
    let taken = [Rect::new(0, 0, 20, 30), Rect::new(0, 30, 20, 60)];
    let auto = auto_slices(canvas, &taken);

    assert_eq!(auto.len(), 2);
    assert_eq!(auto[0], Rect::new(20, 0, 100, 60));
    assert_eq!(auto[1], Rect::new(0, 60, 100, 100));
}

#[test]
fn auto_slices_deterministic_order() {
    let canvas = Rect::new(0, 0, 30, 30);
    let taken = [Rect::new(5, 5, 10, 10), Rect::new(20, 20, 25, 25)];
    let first = auto_slices(canvas, &taken);

    for _ in 0..5 {
        assert_eq!(auto_slices(canvas, &taken), first);
    }
}

#[test]
fn resolve_no_slices_uses_base_name() {
    let d = Document::new("site.psd", Size::new(100, 80), ColorMode::Rgb, SampleType::U8);
    let resolved = resolve(&d);

    assert_eq!(resolved.len(), 1);
    let r = &resolved[0];
    assert_eq!(r.number, 1);
    assert_eq!(r.id, None);
    assert_eq!(r.origin, SliceOrigin::Auto);
    assert_eq!(r.rect, d.bounds());
    assert_eq!(r.name, "site");
    assert_eq!(r.kind, SliceKind::Image);
}

#[test]
fn resolve_multiple_slices_numbering_and_names() {
    let mut d = Document::new("site.psd", Size::new(100, 80), ColorMode::Rgb, SampleType::U8);
    d.slices.list.push(Slice { id: 1, rect: Rect::new(20, 10, 60, 30), ..Default::default() });

    let r = resolve(&d);
    assert_eq!(r.len(), 5);

    let numbers: Vec<usize> = r.iter().map(|s| s.number).collect();
    assert_eq!(numbers, (1..=5).collect::<Vec<_>>());

    assert_eq!(r[0].name, "site_01");
    assert_eq!(r[2].id, Some(1));
    assert_eq!(r[2].name, "site_03");
}

#[test]
fn resolve_uses_user_slice_name() {
    let mut d = Document::new("site.psd", Size::new(100, 80), ColorMode::Rgb, SampleType::U8);
    d.slices.list.push(Slice { id: 1, name: "logo".into(), rect: Rect::new(20, 10, 60, 30), ..Default::default() });

    let r = resolve(&d);
    let user = r.iter().find(|s| s.id == Some(1)).unwrap();
    assert_eq!(user.name, "logo");
}

#[test]
fn resolve_propagates_origin_and_kind() {
    let mut d = Document::new("a.psd", Size::new(50, 50), ColorMode::Rgb, SampleType::U8);
    d.slices.list.push(Slice {
        id: 7,
        origin: SliceOrigin::Layer,
        layer: Some(LayerId(3)),
        kind: SliceKind::NoImage,
        rect: Rect::new(0, 0, 10, 10),
        ..Default::default()
    });

    let r = resolve(&d);
    let s = r.iter().find(|s| s.id == Some(7)).unwrap();
    assert_eq!(s.origin, SliceOrigin::Layer);
    assert_eq!(s.kind, SliceKind::NoImage);
}

#[test]
fn resolve_clips_out_of_bounds_slice() {
    let mut d = Document::new("a.psd", Size::new(100, 100), ColorMode::Rgb, SampleType::U8);
    d.slices.list.push(Slice { id: 1, rect: Rect::new(-20, -20, 50, 50), ..Default::default() });

    let r = resolve(&d);
    let user = r.iter().find(|s| s.id == Some(1)).unwrap();
    assert_eq!(user.rect, Rect::new(0, 0, 50, 50));

    d.slices.list.clear();
    d.slices.list.push(Slice { id: 2, rect: Rect::new(200, 200, 300, 300), ..Default::default() });
    let r = resolve(&d);
    assert!(r.iter().all(|s| s.id != Some(2)));
}

#[test]
fn base_name_sanitizes_and_falls_back() {
    let mut d = Document::new("My Site.psd", Size::new(1, 1), ColorMode::Rgb, SampleType::U8);
    assert_eq!(base_name(&d), "My_Site");

    d.name = "a/b\\c.psd".into();
    assert_eq!(base_name(&d), "a_b_c");

    d.name = "___".into();
    assert_eq!(base_name(&d), "___");

    d.name = "..".into();
    assert_eq!(base_name(&d), "_");

    d.name = ".".into();
    assert_eq!(base_name(&d), "Untitled");

    d.name = "".into();
    assert_eq!(base_name(&d), "Untitled");
}

#[test]
fn resolve_zero_sized_document_does_not_panic() {
    let d = Document::new("empty.psd", Size::new(0, 0), ColorMode::Rgb, SampleType::U8);
    let r = resolve(&d);
    assert!(r.is_empty());
}
