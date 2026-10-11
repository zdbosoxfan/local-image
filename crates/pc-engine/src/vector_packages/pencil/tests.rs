#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{ColorMode, SampleType, Size, Subpath};
fn session() -> Session {
    let mut session = Session::new();
    session.add_document(Document::new("Pencil", Size::new(160, 160), ColorMode::Rgb, SampleType::U8), None);
    session
}
fn samples() -> Vec<Sample> {
    [(20., 20.), (40., 35.), (70., 20.), (110., 50.)].into_iter().map(|(x, y)| Sample { point: Point::new(x, y), pressure: 0.5 }).collect()
}
#[test]
fn pencil_preview_commit_undo_restores_native_bytes_and_journal() {
    let mut s = session();
    let before = s.active().unwrap().doc.clone();
    let bytes = photocraft_format::save_to_bytes(&before, &Default::default()).unwrap();
    let capture = Capture::begin(&s, Operation::Create { above: None }, Default::default()).unwrap();
    let preview = prepare(&s, &capture, &samples(), false).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert!(!s.active().unwrap().history.can_undo());
    let shown = preview.document.clone();
    assert_eq!(preview.pressure, samples());
    preview.commit(&mut s).unwrap();
    assert_eq!(s.active().unwrap().doc, shown);
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(photocraft_format::save_to_bytes(&s.active().unwrap().doc, &Default::default()).unwrap(), bytes);
    assert!(!s.undo());
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, shown);
}
#[test]
fn pencil_commands_preview_without_journaling_and_reject_staleness() {
    let mut s = session();
    let c = Capture::begin(&s, Operation::Create { above: None }, Default::default()).unwrap();
    let preview = s.execute("path.pencil.preview", json!({"capture":c,"samples":[[10.,10.,1.],[40.,50.,0.5],[80.,20.,1.]]})).unwrap();
    assert!(s.journal.is_empty());
    s.edit("Concurrent edit", |doc, _| {
        doc.name = "Changed".into();
        Ok(())
    })
    .unwrap();
    let changed = s.active().unwrap().doc.clone();
    assert!(s.execute("path.pencil.commit", preview).is_err());
    assert_eq!(s.active().unwrap().doc, changed);
    assert!(s.journal.is_empty());
    let c = Capture::begin(&s, Operation::Create { above: None }, Default::default()).unwrap();
    let preview = prepare(&s, &c, &samples(), false).unwrap();
    s.add_document(Document::new("Other", Size::new(160, 160), ColorMode::Rgb, SampleType::U8), None);
    assert!(preview.commit(&mut s).is_err());
}
#[test]
fn pencil_continue_both_ends_and_sculpt_keep_outside_anchors() {
    for start in [false, true] {
        let mut s = session();
        let base = Path::new(vec![Subpath::polyline(&[(20., 20.), (40., 20.), (60., 20.), (80., 20.)])]);
        let layer = Layer::new("Base", LayerContent::Shape(ShapeLayer { path: base.clone(), stroke: Some(Default::default()), ..Default::default() }));
        let id = layer.id;
        Arc::make_mut(&mut s.active_mut().unwrap().doc).layers.push(layer);
        let c = Capture::begin(&s, Operation::Continue { layer: id, subpath: 0, at_start: start }, Default::default()).unwrap();
        let pts = if start { vec![(20., 20.), (10., 30.), (5., 40.)] } else { vec![(80., 20.), (90., 30.), (110., 40.)] };
        let pts: Vec<_> = pts.into_iter().map(|(x, y)| Sample { point: Point::new(x, y), pressure: 1. }).collect();
        let p = prepare(&s, &c, &pts, false).unwrap();
        for k in &base.subpaths[0].knots[1..3] {
            assert!(p.path().subpaths[0].knots.contains(k));
        }
        p.commit(&mut s).unwrap();
        assert!(s.undo());
        let c = Capture::begin(&s, Operation::Sculpt { layer: id, subpath: 0, first: 1, last: 2 }, Default::default()).unwrap();
        let p = prepare(&s, &c, &samples(), false).unwrap();
        assert_eq!(p.path().subpaths[0].knots[0], base.subpaths[0].knots[0]);
        assert_eq!(p.path().subpaths[0].knots.last(), base.subpaths[0].knots.last());
    }
}
#[test]
fn pencil_invalid_locked_target_and_cache_budget_are_atomic() {
    let mut s = session();
    let layer = Layer::new("Base", LayerContent::Shape(Default::default()));
    let id = layer.id;
    let mut parent = Layer::group("Locked", vec![layer]);
    parent.locks.all = true;
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layers.push(parent);
    let before = s.active().unwrap().doc.clone();
    assert!(Capture::begin(&s, Operation::Continue { layer: id, subpath: 0, at_start: false }, Default::default()).is_err());
    assert!(Capture::begin(&s, Operation::Create { above: Some(id) }, Default::default()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.active().unwrap().history.can_undo());
    let options = Options { smoothness: f64::NAN, ..Default::default() };
    assert!(Capture::begin(&s, Operation::Create { above: None }, options).is_err());
    Arc::make_mut(&mut s.active_mut().unwrap().doc).size = Size::new(16000, 16000);
    let c = Capture::begin(&s, Operation::Create { above: None }, Default::default()).unwrap();
    let pts = [Sample { point: Point::new(0., 0.), pressure: 1. }, Sample { point: Point::new(16000., 16000.), pressure: 1. }];
    assert!(prepare(&s, &c, &pts, false).is_err());
}
#[test]
fn pencil_old_options_load_defaults() {
    assert_eq!(serde_json::from_str::<Options>("{}").unwrap(), Options::default());
}
#[test]
fn pencil_sculpt_commit_rejects_tampered_outside_anchors() {
    let mut s = session();
    let layer = Layer::new(
        "Curve",
        LayerContent::Shape(ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(10., 20.), (40., 20.), (70., 20.), (100., 20.)])]), ..Default::default() }),
    );
    let id = layer.id;
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layers.push(layer);
    let before = s.active().unwrap().doc.clone();
    let c = Capture::begin(&s, Operation::Sculpt { layer: id, subpath: 0, first: 1, last: 2 }, Default::default()).unwrap();
    let preview = prepare(&s, &c, &samples(), false).unwrap();
    let mut path = preview.path().clone();
    path.subpaths[0].knots[0].anchor.x += 3.0;
    assert!(s.execute("path.pencil.commit", json!({"capture":c,"path":path})).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert!(!s.active().unwrap().history.can_undo());
}
#[test]
fn pencil_sculpt_range_commit_retains_unedited_anchors_and_undo() {
    let mut s = session();
    let path = Path::new(vec![Subpath::polyline(&[(10., 20.), (100., 20.), (140., 20.)])]);
    let layer = Layer::new("Curve", LayerContent::Shape(ShapeLayer { path: path.clone(), ..Default::default() }));
    let id = layer.id;
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layers.push(layer);
    let c = Capture::begin(
        &s,
        Operation::SculptRange { layer: id, subpath: 0, from: CurvePosition { segment: 0, t: 0.25 }, to: CurvePosition { segment: 0, t: 0.75 } },
        Default::default(),
    )
    .unwrap();
    let preview = prepare(&s, &c, &samples(), false).unwrap();
    for k in &path.subpaths[0].knots {
        assert!(preview.path().subpaths[0].knots.iter().any(|n| n.anchor == k.anchor));
    }
    preview.commit(&mut s).unwrap();
    assert!(s.undo());
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert_eq!(sh.path, path);
}

#[test]
fn pencil_continue_commit_rejects_extension_at_uncaptured_endpoint() {
    let mut s = session();
    let layer = Layer::new(
        "Curve",
        LayerContent::Shape(ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(20., 20.), (60., 20.), (100., 20.)])]), ..Default::default() }),
    );
    let id = layer.id;
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layers.push(layer);
    let before = s.active().unwrap().doc.clone();
    let c = Capture::begin(&s, Operation::Continue { layer: id, subpath: 0, at_start: false }, Default::default()).unwrap();
    let preview = prepare(&s, &c, &samples(), false).unwrap();
    let mut path = preview.path().clone();
    path.subpaths[0].knots.insert(0, photocraft_doc::Knot::corner(0., 20.));
    assert!(s.execute("path.pencil.commit", json!({"capture":c,"path":path})).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.active().unwrap().history.can_undo());
    assert!(s.journal.is_empty());
}

#[test]
fn review_followup_pencil_rejects_zero_length_sculpt_interval_at_capture() {
    let mut s = session();
    let path = Path::new(vec![Subpath::polyline(&[(10., 10.), (20., 20.), (30., 10.)])]);
    let layer = Layer::new("Curve", LayerContent::Shape(ShapeLayer { path, ..Default::default() }));
    let id = layer.id;
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layers.push(layer);
    assert!(Capture::begin(&s, Operation::Sculpt { layer: id, subpath: 0, first: 1, last: 1 }, Default::default()).is_err());
}
