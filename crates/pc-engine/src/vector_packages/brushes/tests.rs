#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{ColorMode, SampleType, Size};
use photocraft_geom::Point;
fn session() -> Session {
    let mut s = Session::new();
    s.add_document(Document::new("Brush", Size::new(160, 120), ColorMode::Rgb, SampleType::U8), None);
    s
}
fn samples() -> Vec<Sample> {
    vec![
        Sample { point: Point::new(20., 60.), pressure: 0.2 },
        Sample { point: Point::new(80., 60.), pressure: 1. },
        Sample { point: Point::new(140., 60.), pressure: 0.4 },
    ]
}
#[test]
fn brush17_engine_preview_commit_pressure_journal_undo_redo() {
    let mut s = session();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let c = Capture::begin(&s, Options::default()).unwrap();
    let p = prepare(&s, &c, &samples()).unwrap();
    let raw: Vec<_> = samples().iter().map(|p| json!({"x":p.point.x,"y":p.point.y,"pressure":p.pressure})).collect();
    let query = s.execute("vector.brush.preview", json!({"capture":c,"samples":raw})).unwrap();
    assert_eq!(query["detached"], true);
    assert_eq!(s.active().unwrap().revision, revision);
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert!(photocraft_compose::flatten(&p.document).px.iter().any(|p| p[3] > 0.9));
    let profile = p.stroke.profile.as_ref().unwrap();
    assert_eq!(profile.points[1].left, 4.);
    let expected = p.document.clone();
    p.commit(&mut s).unwrap();
    assert_eq!(s.active().unwrap().doc, expected);
    assert_eq!(s.active().unwrap().revision, revision + 1);
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.journal[0].0, "vector.brush.commit");
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, expected);
}
#[test]
fn brush17_engine_stale_malformed_samples_and_locked_parent_are_atomic() {
    let mut s = session();
    let c = Capture::begin(&s, Options::default()).unwrap();
    let p = prepare(&s, &c, &samples()).unwrap();
    s.edit("Other edit", |d, _| {
        d.name = "changed".into();
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    assert!(p.commit(&mut s).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    let c = Capture::begin(&s, Options::default()).unwrap();
    let mut bad = samples();
    bad[1].pressure = f64::INFINITY;
    assert!(prepare(&s, &c, &bad).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.execute("vector.brush.commit", json!({"capture":c,"samples":[{"x":0,"y":0,"pressure":1}]})).is_err());
    assert!(s.journal.is_empty());
    let child = Layer::raster("pixel", s.active().unwrap().doc.pixel_format());
    let id = child.id;
    let mut parent = Layer::group("locked", vec![child]);
    parent.locks.all = true;
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layers.push(parent);
    s.select_layer(id).unwrap();
    assert!(Capture::begin(&s, Options::default()).is_err());
}
#[test]
fn brush17_engine_native_bundle_resolves_resource_and_expands_same_pixels() {
    let mut s = session();
    for (_, brush) in geometry::presets() {
        let c = Capture::begin(&s, Options { brush, width: 4., ..Default::default() }).unwrap();
        prepare(&s, &c, &samples()).unwrap().commit(&mut s).unwrap();
    }
    let before = s.active().unwrap().doc.clone();
    let bytes = photocraft_format::save_to_bytes(&before, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    for layer in &loaded.layers {
        let a = layer.vector.appearance.as_ref().unwrap();
        let AppearanceItem::Stroke(stroke) = &a.items[0] else { panic!() };
        assert!(Resource::resolve(stroke.brush.as_ref().unwrap()).is_ok());
        assert!(!projection(layer).unwrap().is_empty());
    }
    let cpu = photocraft_compose::flatten(&before);
    let ids = before.layers.iter().map(|l| l.id).collect();
    let c = TargetCapture::begin(&s, ids).unwrap();
    s.execute("vector.brush.expand", json!({"capture":c})).unwrap();
    for layer in &s.active().unwrap().doc.layers {
        let LayerContent::Group(g) = &layer.content else { panic!() };
        assert!(!g.children.is_empty());
        assert!(g.children.iter().all(|l| l.vector.appearance.is_none()));
    }
    let expanded = photocraft_compose::flatten(&s.active().unwrap().doc);
    let max = cpu
        .px
        .iter()
        .zip(&expanded.px)
        .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
        .fold(0_f32, f32::max);
    assert!(max <= 1. / 255., "{max}");
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn brush17_engine_apply_validates_entire_set_before_publish() {
    let mut s = session();
    for _ in 0..2 {
        let c = Capture::begin(&s, Options::default()).unwrap();
        prepare(&s, &c, &samples()).unwrap().commit(&mut s).unwrap();
    }
    let before = s.active().unwrap().doc.clone();
    let ids: Vec<_> = before.layers.iter().map(|l| l.id).collect();
    let c = TargetCapture::begin(&s, ids.clone()).unwrap();
    let mut stroke = StrokeItem {
        base: ShapeStroke { width: 10., ..Default::default() },
        brush: Some(Resource { brush: Brush::Art(Default::default()), seed: 2 }.reference().unwrap()),
        ..Default::default()
    };
    let count = s.journal.len();
    stroke.base.opacity = f32::NAN;
    assert!(s.execute("vector.brush.apply", json!({"capture":c,"stroke":stroke})).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), count);
    stroke.base.opacity = 1.;
    s.execute("vector.brush.apply", json!({"capture":c,"stroke":stroke})).unwrap();
    assert_eq!(s.journal.len(), count + 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    let c = TargetCapture::begin(&s, ids.clone()).unwrap();
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(ids[1]).unwrap().locks.pixels = true;
    let before = s.active().unwrap().doc.clone();
    assert!(s.execute("vector.brush.apply", json!({"capture":c,"stroke":stroke})).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn brush17_engine_translucent_scatter_uses_single_coverage_and_existing_gradient_stack() {
    let mut s = session();
    s.tools.foreground = [1., 0., 0., 0.5];
    let brush = Brush::Scatter(geometry::Scatter { size: [1., 1.], spacing: [0.1, 0.1], scatter: [0., 0.], rotation: [0., 0.], ..Default::default() });
    let capture = Capture::begin(&s, Options { brush, width: 4., pressure: false, ..Default::default() }).unwrap();
    let preview = prepare(&s, &capture, &samples()).unwrap();
    let cpu = photocraft_compose::flatten(&preview.document);
    // Many overlapping stamps still have the single stroke's 50% alpha.
    let pixel = cpu.px[60 * 160 + 80];
    assert!((pixel[3] - 0.5).abs() <= 1. / 255., "{pixel:?}");
    let mut stroke = preview.stroke.clone();
    preview.commit(&mut s).unwrap();
    let id = s.active().unwrap().active_layer.unwrap();
    let c = TargetCapture::begin(&s, vec![id]).unwrap();
    s.execute("vector.brush.expand", json!({"capture":c})).unwrap();
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc).px, cpu.px);
    assert!(s.undo());
    stroke.base.paint = Fill::Gradient {
        method: Default::default(),
        stops: vec![(0., photocraft_doc::Color::rgb(1., 0., 0.)), (1., photocraft_doc::Color::rgb(0., 0., 1.))],
        angle: 0.,
        scale: 100.,
        style: photocraft_doc::GradientStyle::Linear,
        reverse: false,
        opacity_stops: Vec::new(),
        midpoints: Vec::new(),
        offset: (0., 0.),
        dither: false,
        align: true,
    };
    let c = TargetCapture::begin(&s, vec![id]).unwrap();
    s.execute("vector.brush.apply", json!({"capture":c,"stroke":stroke})).unwrap();
    let cpu = photocraft_compose::flatten(&s.active().unwrap().doc);
    let (a, b) = (cpu.px[60 * 160 + 30], cpu.px[60 * 160 + 130]);
    assert!(a[0] > a[2] && b[2] > b[0], "gradient colors must vary along brush coverage: {a:?}, {b:?}");
    assert!(a[3] > 0.99 && b[3] > 0.99);
}
