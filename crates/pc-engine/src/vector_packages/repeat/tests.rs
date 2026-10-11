use super::*;
use photocraft_doc::{Color, ColorMode, Fill, Path, SampleType, ShapeLayer, Size, Subpath};
use std::sync::Arc;

fn fixture() -> (Session, LayerId) {
    let mut doc = Document::new("Repeat fixture", Size::new(128, 96), ColorMode::Rgb, SampleType::U8);
    let shape = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(10., 12.), (20., 12.), (20., 18.), (10., 18.)])]),
        fill: Some(Fill::Solid(Color::rgb(0.8, 0.1, 0.2))),
        ..Default::default()
    };
    let l = Layer::new("Original", LayerContent::Shape(shape));
    let id = l.id;
    doc.layers.push(l);
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, id)
}
fn grid() -> RepeatSpec {
    RepeatSpec::Grid { rows: 2, cols: 3, spacing: [4., 2.] }
}
fn create(s: &mut Session, id: LayerId) -> LayerId {
    let mut p = Capture::new(s, &[id]).unwrap().params();
    p["spec"] = serde_json::to_value(grid()).unwrap();
    LayerId(s.execute("vector.repeat.create", p).unwrap()["layer"].as_u64().unwrap())
}
fn assert_unchanged(s: &mut Session, before: &Arc<Document>, rev: u64, journal: usize) {
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, before));
    assert_eq!(s.active().unwrap().revision, rev);
    assert_eq!(s.journal.len(), journal);
}
#[test]
fn repeat19_engine_create_preview_set_expand_and_undo_are_atomic() {
    let (mut s, source) = fixture();
    let before = s.active().unwrap().doc.clone();
    let group = create(&mut s, source);
    let live = s.active().unwrap().doc.clone();
    assert_eq!(live.layer(group).unwrap().children().unwrap(), before.layers.as_slice());
    assert_eq!(s.journal.len(), 1);
    let c = Capture::new(&s, &[group]).unwrap();
    let spec = RepeatSpec::Grid { rows: 2, cols: 4, spacing: [7., 2.] };
    let projected = preview(&s, &c, &spec).unwrap();
    assert_eq!(projected.layer(group).unwrap().children().unwrap().len(), 8);
    assert_unchanged(&mut s, &live, c.revision, 1);
    let mut p = c.params();
    p["spec"] = serde_json::to_value(spec).unwrap();
    s.execute("vector.repeat.set", p).unwrap();
    let edited = s.active().unwrap().doc.clone();
    assert_eq!(edited.layer(group).unwrap().children().unwrap(), before.layers.as_slice());
    let params = Capture::new(&s, &[group]).unwrap().params();
    s.execute("vector.repeat.expand", params).unwrap();
    let expanded = s.active().unwrap().doc.clone();
    assert!(expanded.layer(group).unwrap().vector.live.is_none());
    assert_eq!(s.journal.len(), 3);
    let projected = project_document(&edited).unwrap();
    assert_eq!(photocraft_compose::flatten(&expanded).px, photocraft_compose::flatten(&projected).px);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, edited);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, live);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, live);
}
#[test]
fn repeat19_engine_projection_exact_pixels_cache_bounds_and_source_edit_refresh() {
    let (mut s, source) = fixture();
    let group = create(&mut s, source);
    let first = project_document(&s.active().unwrap().doc).unwrap();
    let pixels = photocraft_compose::flatten(&first);
    for row in 0..2 {
        for col in 0..3 {
            let x = 10 + 14 * col;
            let y = 12 + 8 * row;
            assert!(pixels.px[(y * 128 + x) as usize][3] > 0.99);
            assert_eq!(pixels.px[(y * 128 + x + 10) as usize][3], 0.);
        }
    }
    for (_, _, l) in first.walk() {
        if let Some(surface) = l.surface() {
            assert_eq!(surface.content_bounds().width(), 10);
            assert_eq!(surface.content_bounds().height(), 6);
        }
    }
    let shape = Path::new(vec![Subpath::polygon(&[(10., 12.), (22., 12.), (22., 18.), (10., 18.)])]);
    s.execute("shape.applyPaths", json!({"edits":[{"layer":source.0,"path":shape}]})).unwrap();
    let next = project_document(&s.active().unwrap().doc).unwrap();
    assert_ne!(photocraft_compose::flatten(&next).px, pixels.px);
    // Geometric source width changed: the second column starts at 10+12+4=26.
    let children = next.layer(group).unwrap().children().unwrap();
    let sh = children[1].children().unwrap()[0].surface().unwrap();
    assert_eq!(sh.content_bounds().x0, 26);
    assert_eq!(sh.content_bounds().width(), 12);
}
#[test]
fn repeat19_engine_stale_revision_document_target_and_lock_errors_publish_nothing() {
    let (mut s, id) = fixture();
    let group = create(&mut s, id);
    let stale = Capture::new(&s, &[group]).unwrap();
    s.select_layer(group).unwrap();
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    let mut p = stale.params();
    p["spec"] = serde_json::to_value(grid()).unwrap();
    assert!(s.execute("vector.repeat.set", p).is_err());
    assert_unchanged(&mut s, &before, rev, 1);
    for ids in [vec![group, group], vec![group, id], vec![group, LayerId(u64::MAX)]] {
        let mut p = json!({"document":before.id.0,"revision":rev,"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()});
        p["spec"] = serde_json::to_value(grid()).unwrap();
        assert!(s.execute("vector.repeat.set", p).is_err());
        assert_unchanged(&mut s, &before, rev, 1);
    }
    let mut locked = (*before).clone();
    locked.layer_mut(id).unwrap().locks.position = true;
    s.active_mut().unwrap().doc = Arc::new(locked);
    let locked = s.active().unwrap().doc.clone();
    let p = json!({"document":locked.id.0,"revision":rev,"layers":[group.0]});
    assert!(s.execute("vector.repeat.expand", p).is_err());
    assert_unchanged(&mut s, &locked, rev, 1);
    s.add_document(Document::new("Other", Size::new(8, 8), ColorMode::Rgb, SampleType::U8), None);
    assert!(stale.check(&s).is_err());
    assert!(s.active().unwrap().doc.layers.is_empty());
}
#[test]
fn repeat19_engine_entire_multi_target_set_preflight_prevents_partial_edits() {
    let (mut s, id) = fixture();
    let first = create(&mut s, id);
    s.edit("second fixture", |doc, _| {
        let second = doc.layer(first).unwrap().duplicate();
        doc.layers.push(second);
        Ok(())
    })
    .unwrap();
    let second = s.active().unwrap().doc.layers[1].id;
    let mut doc = (*s.active().unwrap().doc).clone();
    doc.layer_mut(second).unwrap().vector.live = None;
    s.active_mut().unwrap().doc = Arc::new(doc);
    let before = s.active().unwrap().doc.clone();
    let c = Capture::new(&s, &[first, second]).unwrap();
    let mut p = c.params();
    p["spec"] = serde_json::to_value(RepeatSpec::Mirror { origin: [30., 0.], angle: 0. }).unwrap();
    assert!(s.execute("vector.repeat.set", p).is_err());
    assert_unchanged(&mut s, &before, c.revision, 1);
}
#[test]
fn repeat19_engine_invalid_grid_nonfinite_paint_and_aggregate_cache_budget_refused() {
    let (mut s, id) = fixture();
    let before = s.active().unwrap().doc.clone();
    let c = Capture::new(&s, &[id]).unwrap();
    for spec in [
        json!({"kind":"grid","rows":1000,"cols":1000,"spacing":[0,0]}),
        json!({"kind":"grid","rows":2,"cols":3,"spacing":[-10,0]}),
        json!({"kind":"mirror","origin":[0,null],"angle":0}),
    ] {
        let mut p = c.params();
        p["spec"] = spec;
        assert!(s.execute("vector.repeat.create", p).is_err());
        assert_unchanged(&mut s, &before, c.revision, 0);
    }
    let mut huge = (*before).clone();
    huge.size = Size::new(5000, 5000);
    if let LayerContent::Shape(sh) = &mut huge.layer_mut(id).unwrap().content {
        sh.path = Path::new(vec![Subpath::polygon(&[(0., 0.), (5000., 0.), (5000., 5000.), (0., 5000.)])]);
    }
    s.active_mut().unwrap().doc = Arc::new(huge);
    let before = s.active().unwrap().doc.clone();
    let mut p = c.params();
    p["spec"] = json!({"kind":"radial","count":1,"center":[0,0],"angle":0});
    assert!(s.execute("vector.repeat.create", p).unwrap_err().to_string().contains("budget"));
    assert_unchanged(&mut s, &before, c.revision, 0);
    let mut invalid = (*before).clone();
    if let LayerContent::Shape(sh) = &mut invalid.layer_mut(id).unwrap().content {
        sh.stroke = Some(photocraft_doc::ShapeStroke { width: f32::NAN, ..Default::default() });
    }
    assert!(
        project_document(&{
            let mut doc = invalid;
            let source = doc.layers.remove(0);
            let mut group = Layer::group("repeat", vec![source]);
            group.vector.live = Some(LiveGroup::Repeat(grid()));
            doc.layers.push(group);
            doc
        })
        .is_err()
    );
}
#[test]
fn repeat19_engine_native_roundtrip_retains_sources_parameters_and_old_documents() {
    let (mut s, id) = fixture();
    let group = create(&mut s, id);
    let live = s.active().unwrap().doc.clone();
    let bytes = photocraft_format::save_to_bytes(&live, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded.layers[0].vector.live, Some(LiveGroup::Repeat(grid())));
    assert_eq!(loaded.layers[0].children().unwrap().len(), 1);
    let LayerContent::Shape(source) = &loaded.layers[0].children().unwrap()[0].content else { panic!() };
    assert_eq!(source.path.control_bounds(), Some((10., 12., 20., 18.)));
    assert_eq!(photocraft_compose::flatten(&project_document(&loaded).unwrap()).px, photocraft_compose::flatten(&project_document(&live).unwrap()).px);
    assert!(live.layer(group).unwrap().vector.live.is_some());
}

#[test]
fn repeat19_native_codec_existing_v1_document_fixture_still_loads() {
    // This must run independently even while the shared live-group tag hook is pending.
    let old = photocraft_format::load_from_bytes(include_bytes!("../../../../pc-format/tests/fixtures/vector-v1.pcraft")).unwrap();
    assert!(!old.walk().iter().any(|(_, _, l)| l.vector.live.is_some()));
}

#[path = "gpu_tests.rs"]
mod gpu;
