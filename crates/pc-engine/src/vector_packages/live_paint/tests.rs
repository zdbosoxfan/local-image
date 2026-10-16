#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, SampleType, Size, Subpath};
pub(super) fn fixture() -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Region Paint", Size::new(160, 128), ColorMode::Rgb, SampleType::F32);
    let mut ids = Vec::new();
    for (i, path) in [
        photocraft_vector::shapes::ellipse(16., 24., 80., 80.),
        photocraft_vector::shapes::ellipse(56., 24., 80., 80.),
        photocraft_doc::Path::new(vec![Subpath::polyline(&[(0., 64.), (150., 64.)])]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut layer = Layer::new(
            "Source",
            LayerContent::Shape(ShapeLayer { path, fill: if i < 2 { Some(Fill::Solid(Color::BLACK)) } else { None }, ..Default::default() }),
        );
        if let LayerContent::Shape(sh) = &mut layer.content {
            crate::vector_cmds::refresh_shape(&doc, sh);
        }
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, ids)
}
fn flood_action(options: FloodOptions) -> Action {
    Action::Flood { points: vec![[32., 40.], [76., 40.]], paint: Fill::Solid(Color::rgb(1., 0., 0.)), options }
}
#[test]
fn live_paint42_flood_preview_exact_area_coverage_release_and_single_undo() {
    for insertion in [Insertion::Inside, Insertion::InBetween] {
        let (mut s, ids) = fixture();
        let before = s.active().unwrap().doc.clone();
        let g = Gesture::begin(&s, &ids).unwrap();
        let action = flood_action(FloodOptions { insertion, ..Default::default() });
        let p = g.preview(&s, action.clone()).unwrap();
        assert!(p.changed);
        assert_eq!(p.layers.len(), 2);
        assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
        assert!(s.journal.is_empty());
        let mut area = 0.;
        for id in &p.layers {
            let LayerContent::Shape(sh) = &p.model.layer(*id).unwrap().content else { panic!() };
            area += ops::area(&sh.path).unwrap();
            assert!(sh.cache.is_some());
        }
        assert!((area - std::f64::consts::PI * 40. * 40. / 2.).abs() < 1.);
        let expected = photocraft_compose::flatten(&p.display).px;
        let output = g.commit(&mut s, action).unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(s.journal.len(), 1);
        assert_eq!(s.active().unwrap().revision, g.capture.revision + 1);
        assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc).px, expected);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
        assert!(!s.undo());
    }
}
#[test]
fn live_paint42_flood_modes_transparency_smart_refill_and_knockout_oracle() {
    for (mode, expected) in [(FillMode::AddOnTop, [0.5, 0., 0., 1.]), (FillMode::SmartRefill, [0.5, 0., 0., 1.]), (FillMode::Knockout, [1., 0., 0., 0.5])] {
        let (mut s, ids) = fixture();
        // Use the exclusive left region, with no other painted source below it.
        let g = Gesture::begin(&s, &ids).unwrap();
        let mut color = Color::rgb(1., 0., 0.);
        color.alpha = 0.5;
        let a = Action::Flood { points: vec![[32., 40.]], paint: Fill::Solid(color), options: FloodOptions { mode, ..Default::default() } };
        g.commit(&mut s, a).unwrap();
        let pixel = photocraft_compose::flatten(&s.active().unwrap().doc).get(32, 40);
        for c in 0..4 {
            assert!((pixel[c] - expected[c]).abs() < 1. / 255., "{mode:?} {pixel:?}");
        }
    }
    let (mut s, ids) = fixture();
    let g = Gesture::begin(&s, &ids).unwrap();
    let a = Action::Flood { points: vec![[32., 40.]], paint: Fill::Solid(Color::BLACK), options: Default::default() };
    assert!(!g.preview(&s, a.clone()).unwrap().changed);
    assert!(g.commit(&mut s, a).unwrap().is_empty());
    // Session.execute currently journals even a no-op edit command; pixels/history don't change.
    assert!(!s.undo());
}
#[test]
fn live_paint42_stale_wrong_document_locked_and_invalid_targets_are_atomic() {
    let (mut s, ids) = fixture();
    let capture = Capture::begin(&s, &ids).unwrap();
    let r = Request { capture: capture.clone(), action: flood_action(Default::default()) };
    s.active_mut().unwrap().revision += 1;
    let before = s.active().unwrap().doc.clone();
    assert!(commit(&mut s, &r).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
    let mut capture = capture;
    capture.revision = s.active().unwrap().revision;
    capture.document += 1;
    assert!(prepare(&s, &Request { capture, action: flood_action(Default::default()) }).is_err());
    assert!(Capture::begin(&s, &[ids[0], ids[0]]).is_err());
    s.edit("Lock", |doc, _| {
        doc.layer_mut(ids[1]).unwrap().locks.pixels = true;
        Ok(())
    })
    .unwrap();
    assert!(Capture::begin(&s, &ids).is_err());
}
#[test]
fn review_regression_live_paint42_queries_are_unjournaled_and_missing_data_fails_closed() {
    let (mut s, ids) = fixture();
    let c = Capture::begin(&s, &ids).unwrap();
    let before = s.active().unwrap().doc.clone();
    let info = s.execute("livePaint.info", json!({"capture":c})).unwrap();
    assert_eq!(info["faces"].as_array().unwrap().len(), 6);
    let sample = s.execute("livePaint.sample", json!({"capture":c,"point":[32.,40.]})).unwrap();
    assert_eq!(sample["paint"], serde_json::to_value(Fill::Solid(Color::BLACK)).unwrap());
    let selected = s.execute("livePaint.select", json!({"capture":c,"points":[[32.,40.],[76.,40.]]})).unwrap();
    assert_eq!(selected["faces"].as_array().unwrap().len(), 2);
    let mismatched = Request { capture: c, action: flood_action(Default::default()) };
    assert!(s.execute("livePaint.make", serde_json::to_value(mismatched).unwrap()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert!(!s.undo());
    let mut group = Layer::group("Sources", Vec::new());
    evaluator::write_spec(&mut group, &LivePaintSpec::default()).unwrap();
    let empty_id = group.id;
    let mut detached = (*s.active().unwrap().doc).clone();
    detached.layers = vec![group];
    let mut empty = Session::new();
    empty.add_document(detached, None);
    assert!(Gesture::begin(&empty, &[empty_id]).is_err());
    let g = Gesture::begin(&s, &ids).unwrap();
    g.commit(&mut s, Action::Make { gap: Gap::None }).unwrap();
    assert_eq!(s.journal.len(), 1);
    assert!(s.active().unwrap().doc.walk().iter().any(|(_, _, l)| evaluator::read_spec(l).unwrap().is_some()));
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn live_paint42_visible_boundary_fill_merges_same_color_regions() {
    let (s, ids) = fixture();
    let g = Gesture::begin(&s, &ids).unwrap();
    let p = g
        .preview(
            &s,
            Action::Flood {
                points: vec![[32., 40.]],
                paint: Fill::Solid(Color::rgb(1., 0., 0.)),
                options: FloodOptions { visible_boundaries: true, ..Default::default() },
            },
        )
        .unwrap();
    let count = p.layers.len();
    assert_eq!(count, 2);
    let expected = g.graph.merge(&(0..6).collect()).unwrap();
    let area = ops::area(&expected).unwrap();
    let actual = p
        .layers
        .iter()
        .map(|id| {
            let LayerContent::Shape(sh) = &p.model.layer(*id).unwrap().content else { panic!() };
            ops::area(&sh.path).unwrap()
        })
        .sum::<f64>();
    assert!((actual - area).abs() < 1e-5);
}
#[test]
#[ignore = "00 LiveGroup data dependency and integrated native projection gate; speed rules"]
fn live_paint42_native_group_edit_merge_expand_release_save_load() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let group = Gesture::begin(&s, &ids).unwrap().commit(&mut s, Action::Make { gap: Gap::Small }).unwrap()[0];
    let g = Gesture::begin(&s, &[group]).unwrap();
    g.commit(&mut s, Action::Paint { points: vec![[76., 40.]], paint: Some(Fill::Solid(Color::rgb(1., 0., 0.))), edges: false, width: 1., tolerance: 4. })
        .unwrap();
    let paint = Gesture::begin(&s, &[group]).unwrap().spec.faces.clone();
    let circle = ids[1];
    let path = if let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(circle).unwrap().content { sh.path.clone() } else { panic!() };
    let path = path.transform(&photocraft_geom::Affine::translate(5., 0.));
    s.execute("shape.applyPaths", json!({"edits":[{"layer":circle.0,"path":path}]})).unwrap();
    let g = Gesture::begin(&s, &[group]).unwrap();
    assert_eq!(g.spec.faces, paint);
    let i = g.graph.face_at(Point::new(76., 40.)).unwrap().unwrap();
    assert_eq!(g.graph.face_paint(i, &g.sources, &g.spec).unwrap(), Some(Fill::Solid(Color::rgb(1., 0., 0.))));
    let r = 40f64;
    let d = 45f64;
    let lens = 2. * r * r * (d / (2. * r)).acos() - 0.5 * d * (4. * r * r - d * d).sqrt();
    assert!((g.graph.faces[i].area - lens / 2.).abs() < 1.);
    g.commit(&mut s, Action::Paint { points: vec![[70., 64.]], paint: Some(Fill::Solid(Color::rgb(0., 0., 1.))), edges: true, width: 2., tolerance: 1. })
        .unwrap();
    let extra = Layer::new(
        "Merge source",
        LayerContent::Shape(ShapeLayer { path: photocraft_doc::Path::new(vec![Subpath::polyline(&[(78., 0.), (78., 120.)])]), ..Default::default() }),
    );
    let extra_id = extra.id;
    s.edit("Add merge source", |doc, _| {
        doc.layers.push(extra);
        Ok(())
    })
    .unwrap();
    Gesture::begin(&s, &[group, extra_id]).unwrap().commit(&mut s, Action::Merge).unwrap();
    let g = Gesture::begin(&s, &[group]).unwrap();
    for point in [Point::new(72., 40.), Point::new(84., 40.)] {
        let i = g.graph.face_at(point).unwrap().unwrap();
        assert_eq!(g.graph.face_paint(i, &g.sources, &g.spec).unwrap(), Some(Fill::Solid(Color::rgb(1., 0., 0.))));
    }
    for point in [Point::new(70., 64.), Point::new(84., 64.)] {
        let i = g.graph.edge_at(point, 0.1).unwrap().unwrap();
        assert_eq!(g.graph.edge_paint(i, &g.sources, &g.spec).unwrap().unwrap().width, 2.);
    }
    let document = s.active().unwrap().doc.clone();
    let bytes = photocraft_format::save_to_bytes(&document, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    let a = evaluator::project_document(&document).unwrap();
    let b = evaluator::project_document(&loaded).unwrap();
    assert_eq!(photocraft_compose::flatten(&a).px, photocraft_compose::flatten(&b).px);
    let g = Gesture::begin(&s, &[group]).unwrap();
    g.commit(&mut s, Action::Expand).unwrap();
    assert!(s.active().unwrap().doc.walk().into_iter().all(|(_, _, l)| l.vector.live.is_none()));
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc).px, photocraft_compose::flatten(&a).px);
    assert!(s.undo());
    let g = Gesture::begin(&s, &[group]).unwrap();
    g.commit(&mut s, Action::Release).unwrap();
    assert_eq!(s.active().unwrap().doc.layers.len(), before.layers.len() + 1);
    assert!(s.active().unwrap().doc.walk().into_iter().all(|(_, _, l)| l.vector.live.is_none()));
}

#[test]
fn review_regression_live_paint42_flood_mesh_rejects_without_discarding_native_paint() {
    let (mut s, ids) = fixture();
    let source = s.active().unwrap().doc.clone();
    let mut edit = crate::vector_api::snapshot_shapes(&source, &[ids[0]]).unwrap().remove(0);
    let mesh =
        photocraft_vector::mesh_paint::Mesh::regular([16., 24., 96., 104.], 1, 1, photocraft_vector::mesh_paint::Appearance::Flat, 0., |_| [1., 0., 0., 1.])
            .unwrap();
    (super::super::mesh::NATIVE.write)(&mut edit, &mesh.into()).unwrap();
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(ids[0]).unwrap().vector = edit.vector;
    let before = s.active().unwrap().doc.clone();
    let g = Gesture::begin(&s, &ids).unwrap();
    let err = g.commit(&mut s, flood_action(Default::default())).unwrap_err();
    assert!(err.to_string().contains("Flood Fill requires ordinary fill/stroke paints"), "{err}");
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert!(!s.undo());
}
