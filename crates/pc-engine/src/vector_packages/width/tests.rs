use super::*;
use photocraft_doc::{Color, ColorMode, Fill, Layer, Path, SampleType, ShapeLayer, ShapeStroke, Size, Subpath};
use std::sync::Arc;
fn setup() -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Width", Size::new(160, 120), ColorMode::Rgb, SampleType::U8);
    let mut ids = Vec::new();
    for y in [30.0, 80.0] {
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polyline(&[(20.0, y), (140.0, y)])]),
            stroke: Some(ShapeStroke { width: 10.0, paint: Fill::Solid(Color::BLACK), ..Default::default() }),
            ..Default::default()
        };
        crate::vector_cmds::refresh_shape(&doc, &mut sh);
        let l = Layer::new("Stroke", LayerContent::Shape(sh));
        ids.push(l.id);
        doc.layers.push(l);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, ids)
}
#[test]
fn width_package_engine_detached_preview_release_roundtrip_one_undo() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let mut g = Gesture::begin(&s, &ids, None).unwrap();
    g.set_point(0, None, WidthPoint { t: 0.5, left: 20.0, right: 4.0 }).unwrap();
    g.preset("lens").unwrap();
    let preview = preview_document(&s, g.request()).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert!(preview.layers.iter().all(|l| l.vector.appearance.is_some()));
    g.commit(&mut s).unwrap();
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert_eq!(s.journal.len(), 1);
    assert_eq!(*s.active().unwrap().doc, preview);
    let loaded = photocraft_format::load_from_bytes(&photocraft_format::save_to_bytes(&preview, &Default::default()).unwrap()).unwrap();
    assert_eq!(loaded, preview);
    for id in ids {
        let LayerContent::Shape(a) = &before.layer(id).unwrap().content else { panic!("shape") };
        let LayerContent::Shape(b) = &preview.layer(id).unwrap().content else { panic!("shape") };
        assert_eq!(a.path, b.path);
    }
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
    assert!(s.redo());
    assert_eq!(*s.active().unwrap().doc, preview);
}
#[test]
fn width_package_engine_atomic_stale_duplicate_nonshape_lock_and_invalid_final_target() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let mut g = Gesture::begin(&s, &ids, None).unwrap();
    g.preset("lens").unwrap();
    let valid = g.request().clone();
    let mut variants = Vec::new();
    let mut c = valid.clone();
    c.document += 1;
    variants.push(c);
    let mut c = valid.clone();
    c.revision += 1;
    variants.push(c);
    let mut c = valid.clone();
    c.edits[1].layer = LayerId(u64::MAX);
    variants.push(c);
    let mut c = valid.clone();
    c.edits[1].layer = ids[0];
    variants.push(c);
    let mut c = valid.clone();
    c.edits[1].item = 50;
    variants.push(c);
    let mut c = valid.clone();
    c.edits[1].profile.as_mut().unwrap().points.reverse();
    variants.push(c);
    for c in variants {
        assert!(s.execute("shape.width.set", json!(c)).is_err());
        assert_eq!(s.active().unwrap().doc, before);
        assert!(s.journal.is_empty());
        assert_eq!(s.active().unwrap().revision, revision);
    }
    assert!(!s.undo());
    let mut locked = (*before).clone();
    let child = locked.layers.pop().unwrap();
    let mut group = Layer::group("Locked", vec![child]);
    group.locks.all = true;
    locked.layers.push(group);
    s.active_mut().unwrap().doc = Arc::new(locked);
    assert!(s.execute("shape.width.set", json!(valid)).is_err());
    assert!(s.journal.is_empty());
}
#[test]
fn width_package_engine_point_add_move_delete_empty_and_old_load() {
    let (mut s, ids) = setup();
    let mut g = Gesture::begin(&s, &ids[..1], None).unwrap();
    let i = g.set_point(0, None, WidthPoint { t: 0.25, left: 10.0, right: 2.0 }).unwrap();
    assert_eq!(g.profile(0).unwrap().points.len(), 3);
    let i = g.set_point(0, Some(i), WidthPoint { t: 0.75, left: 10.0, right: 2.0 }).unwrap();
    assert_eq!(g.profile(0).unwrap().points[i].t, 0.75);
    let before = g.request().clone();
    assert!(g.set_point(0, None, WidthPoint { t: 0.5, left: f64::NAN, right: 2.0 }).is_err());
    assert_eq!(g.request(), &before);
    g.remove(0, i).unwrap();
    assert_eq!(g.profile(0).unwrap().points.len(), 2);
    g.set_profile(0, Some(WidthProfile::default())).unwrap();
    s.execute("shape.width.preview", json!(g.request())).unwrap();
    assert!(s.journal.is_empty());
    g.commit(&mut s).unwrap();
    assert_eq!(s.journal.len(), 1);
    let old = photocraft_format::load_from_bytes(include_bytes!("../../../../pc-format/tests/fixtures/vector-basic.pcraft")).unwrap();
    assert!(old.layers.iter().all(|l| l.vector.appearance.is_none()));
}
#[test]
fn width_package_engine_prepared_cache_bounds_and_budget() {
    let (mut s, ids) = setup();
    let mut g = Gesture::begin(&s, &ids[..1], None).unwrap();
    g.preset("lens").unwrap();
    let edits = prepare(&s, g.request()).unwrap();
    assert_eq!(edits.len(), 1);
    let cache = edits[0].shape.cache.as_ref().unwrap();
    assert!(cache.tile_count() <= 4);
    // Preparation counts padded tiles, before a full-document surface can be allocated.
    let mut doc = (*s.active().unwrap().doc).clone();
    doc.size = Size::new(1_000_000, 1_000_000);
    if let LayerContent::Shape(sh) = &mut doc.layer_mut(ids[0]).unwrap().content {
        sh.path = Path::new(vec![Subpath::polyline(&[(0.0, 0.0), (1.0, 999_999.0)])]);
    }
    s.active_mut().unwrap().doc = Arc::new(doc);
    let mut g = Gesture::begin(&s, &ids[..1], None).unwrap();
    g.preset("lens").unwrap();
    assert!(prepare(&s, g.request()).unwrap_err().to_string().contains("512 MiB"));
    assert!(s.journal.is_empty());
}
#[path = "gpu.rs"]
mod gpu;
