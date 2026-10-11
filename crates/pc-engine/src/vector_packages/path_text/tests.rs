#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{
    Color, ColorMode, Knot, SampleType, Size, Subpath,
    text::{Orientation, TextShape},
};
fn session() -> (Session, LayerId) {
    let mut s = Session::new();
    let mut doc = Document::new("Path text", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    let path = Path::new(vec![Subpath { closed: false, knots: vec![Knot::corner(20., 100.), Knot::corner(220., 100.)], ..Default::default() }]);
    let mut shape = ShapeLayer { path, ..Default::default() };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let id = doc.insert_above(None, Layer::new("Curve", LayerContent::Shape(shape)));
    s.add_document(doc, None);
    (s, id)
}
fn request(s: &Session, id: LayerId, text: &str) -> Commit {
    let st = s.active().unwrap();
    let LayerContent::Shape(sh) = &st.doc.layer(id).unwrap().content else { panic!() };
    Commit {
        document: st.doc.id.0,
        revision: st.revision,
        edits: vec![Edit {
            target: Target::New { curve: id },
            path: PathText { path: sh.path.clone(), start: 0., end: 1., flip: false, align: PathTextAlign::Baseline },
            text: Some(text.into()),
            orientation: None,
            paragraph: None,
            style: Some(CharStyle { font_family: "Inter".into(), size_pt: 20., ..Default::default() }),
        }],
    }
}
fn publish(s: &mut Session, c: &Commit) -> LayerId {
    let value = s.execute("type.path.commit", serde_json::to_value(c).unwrap()).unwrap();
    LayerId(value["layers"][0].as_u64().unwrap())
}
fn update(s: &Session, id: LayerId) -> Commit {
    let st = s.active().unwrap();
    Commit {
        document: st.doc.id.0,
        revision: st.revision,
        edits: vec![Edit {
            target: Target::Text { layer: id },
            path: st.doc.layer(id).unwrap().vector.on_path.clone().unwrap(),
            text: None,
            style: None,
            orientation: None,
            paragraph: None,
        }],
    }
}
#[test]
fn path_text_preview_commit_journal_and_undo_are_atomic() {
    let (mut s, id) = session();
    let c = request(&s, id, "ABC");
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.entries().len();
    let revision = s.active().unwrap().revision;
    let preview = preview_document(&s, &c).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    let rendered = photocraft_compose::flatten(&preview);
    assert!(rendered.px.iter().any(|p| p[3] > 0.9));
    let text = publish(&mut s, &c);
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc), rendered);
    assert!(s.active().unwrap().doc.layer(text).unwrap().vector.on_path.is_some());
    assert_eq!(s.active().unwrap().revision, revision + 1);
    assert_eq!(s.active().unwrap().history.entries().len(), history + 1);
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
}
#[test]
fn path_text_multi_target_errors_publish_nothing_and_stale_docs_reject() {
    let (mut s, id) = session();
    let mut c = request(&s, id, "ABC");
    let before = s.active().unwrap().doc.clone();
    c.edits.push(Edit { target: Target::Text { layer: LayerId(u64::MAX) }, ..c.edits[0].clone() });
    assert!(s.execute("type.path.commit", serde_json::to_value(&c).unwrap()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    c.edits.pop();
    c.edits.push(c.edits[0].clone());
    assert!(preview_document(&s, &c).is_err());
    c.edits.pop();
    c.revision += 1;
    assert!(preview_document(&s, &c).is_err());
    c.revision -= 1;
    c.document += 1;
    assert!(preview_document(&s, &c).is_err());
    c.document -= 1;
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap().locks.all = true;
    assert!(preview_document(&s, &c).is_err());
}
#[test]
fn path_text_invalid_styles_spans_and_degenerate_curves_reject() {
    let (s, id) = session();
    let mut c = request(&s, id, "ABC");
    for start in [f64::NAN, f64::INFINITY, -1., 2.] {
        c.edits[0].path.start = start;
        assert!(preview_document(&s, &c).is_err());
    }
    c.edits[0].path.start = 0.;
    c.edits[0].path.end = -0.1;
    assert!(preview_document(&s, &c).is_err());
    c.edits[0].path.end = 1.;
    c.edits[0].style.as_mut().unwrap().size_pt = f32::NAN;
    assert!(preview_document(&s, &c).is_err());
    assert!(Sampler::new(&Path::default()).is_err());
    let mut p = c.edits[0].path.path.clone();
    p.subpaths[0].knots[1] = p.subpaths[0].knots[0];
    assert!(Sampler::new(&p).is_err());
}
#[test]
fn path_text_marker_nearest_uses_arc_distance_and_curve_hit_tolerance() {
    let (s, id) = session();
    let doc = &s.active().unwrap().doc;
    let (_, path) = curve_at(doc, Point::new(70., 102.), 3.).unwrap().unwrap();
    assert!((path.start - 0.25).abs() < 1e-6);
    assert_eq!(curve_at(doc, Point::new(70., 104.), 3.).unwrap(), None);
    assert_eq!(curve_at(doc, Point::new(70., 100.), 3.).unwrap().unwrap().0, id);
    let sampler = Sampler::new(&path.path).unwrap();
    assert!((sampler.fraction(Point::new(120., 90.)).unwrap() - 0.5).abs() < 1e-6);
}
#[test]
fn path_text_flip_preserves_runs_vertical_and_overflow() {
    let (mut s, id) = session();
    let c = request(&s, id, "AV office");
    let text_id = publish(&mut s, &c);
    let mut c = update(&s, text_id);
    c.edits[0].path.flip = true;
    c.edits[0].path.end = 0.2;
    let preview = preview_document(&s, &c).unwrap();
    let l = preview.layer(text_id).unwrap();
    let LayerContent::Text(t) = &l.content else { panic!() };
    let mut e = TextEngine::new();
    let result = project(&mut e, &preview, t, l.vector.on_path.as_ref().unwrap()).unwrap();
    assert!(result.overflow);
    assert!(result.clusters.iter().all(|c| (c.angle.abs() - std::f64::consts::PI).abs() < 1e-6));
    let original = s.active().unwrap().doc.layer(text_id).unwrap();
    let LayerContent::Text(before) = &original.content else { panic!() };
    assert_eq!(before.runs, t.runs);
    let vertical = TextLayer { orientation: Orientation::Vertical, shape: TextShape::Box { x: 0., y: 0., width: 10., height: 10. }, ..t.clone() };
    assert!(!project(&mut e, &preview, &vertical, l.vector.on_path.as_ref().unwrap()).unwrap().shaped.glyphs.is_empty());
}
fn iou(a: &photocraft_compose::Buffer, b: &photocraft_compose::Buffer) -> f64 {
    let mut union = 0;
    let mut intersection = 0;
    for (a, b) in a.px.iter().zip(&b.px) {
        if a[3] > 0.5 || b[3] > 0.5 {
            union += 1;
        }
        if a[3] > 0.5 && b[3] > 0.5 {
            intersection += 1;
        }
    }
    assert!(union > 0);
    intersection as f64 / union as f64
}
#[test]
fn path_text_converted_curves_match_render_and_keep_counters() {
    let (mut s, id) = session();
    let mut c = request(&s, id, "BOO ABC");
    let path = photocraft_vector::shapes::ellipse(28., 28., 200., 200.);
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap().content =
        LayerContent::Shape(ShapeLayer { path: path.clone(), fill: None, ..Default::default() });
    c.edits[0].path.path = path;
    c.edits[0].path.start = 0.25;
    let text = publish(&mut s, &c);
    let before = s.active().unwrap().doc.clone();
    let rendered = photocraft_compose::flatten(&before);
    let st = s.active().unwrap();
    let params = json!({"document":st.doc.id.0,"revision":st.revision,"layers":[text.0]});
    s.execute("type.path.convert", params).unwrap();
    let layer = s.active().unwrap().doc.layer(text).unwrap();
    assert!(matches!(layer.content, LayerContent::Shape(_)));
    assert!(layer.vector.on_path.is_none());
    let score = iou(&rendered, &photocraft_compose::flatten(&s.active().unwrap().doc));
    assert!(score > 0.97, "curve IoU={score}");
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn path_text_multicolor_conversion_keeps_paint_and_metadata() {
    let (mut s, id) = session();
    let c = request(&s, id, "ABC");
    let text = publish(&mut s, &c);
    {
        let l = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(text).unwrap();
        let LayerContent::Text(t) = &mut l.content else { panic!() };
        t.runs = vec![
            TextRun { len: 1, style: CharStyle { color: Color::rgb(1., 0., 0.), ..Default::default() } },
            TextRun { len: 2, style: CharStyle { color: Color::rgb(0., 0., 1.), ..Default::default() } },
        ];
    }
    let source = s.active().unwrap().doc.clone();
    let l = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(text).unwrap();
    let LayerContent::Text(t) = &mut l.content else { panic!() };
    refresh(&source, t, l.vector.on_path.as_ref().unwrap()).unwrap();
    let before = photocraft_compose::flatten(&s.active().unwrap().doc);
    let st = s.active().unwrap();
    let params = json!({"document":st.doc.id.0,"revision":st.revision,"layers":[text.0]});
    s.execute("type.path.convert", params).unwrap();
    let layer = s.active().unwrap().doc.layer(text).unwrap();
    assert_eq!(layer.children().unwrap().len(), 2);
    let fills: Vec<_> = layer
        .children()
        .unwrap()
        .iter()
        .map(|l| match &l.content {
            LayerContent::Shape(sh) => sh.fill.clone(),
            _ => panic!("color group must contain curves"),
        })
        .collect();
    assert!(fills.contains(&Some(Fill::Solid(Color::rgb(1., 0., 0.)))));
    assert!(fills.contains(&Some(Fill::Solid(Color::rgb(0., 0., 1.)))));
    assert!(iou(&before, &photocraft_compose::flatten(&s.active().unwrap().doc)) > 0.97);
}
#[test]
fn path_text_v1_arc_sampler_agrees_with_independent_cubic_oracle() {
    let (mut s, id) = session();
    let path = photocraft_vector::shapes::ellipse(28., 28., 200., 200.);
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap().content =
        LayerContent::Shape(ShapeLayer { path: path.clone(), ..Default::default() });
    let sampler = Sampler::new(&path).unwrap();
    use on_path::Curve;
    let expected = 200. * std::f64::consts::PI;
    assert!((sampler.length() - expected).abs() < 0.2);
    for i in 0..32 {
        let (p, t) = sampler.sample(sampler.length() * f64::from(i) / 32.);
        let radius = (p.x - 128.).hypot(p.y - 128.);
        assert!((radius - 100.).abs() < 0.5);
        let tangent = Point::new(-(p.y - 128.) / radius, (p.x - 128.) / radius);
        assert!((t.x * tangent.y - t.y * tangent.x).abs().asin().to_degrees() < 0.5);
    }
}
#[path = "gpu.rs"]
mod gpu;

#[test]
fn path_text_styled_typing_preserves_runs_and_native_roundtrip() {
    let (mut s, curve) = session();
    let c = request(&s, curve, "ABC");
    let id = publish(&mut s, &c);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let l = doc.layer_mut(id).unwrap();
    let LayerContent::Text(t) = &mut l.content else { panic!() };
    let red = CharStyle { color: Color::rgb(1., 0., 0.), ..Default::default() };
    let blue = CharStyle { color: Color::rgb(0., 0., 1.), ..Default::default() };
    t.runs = vec![TextRun { len: 1, style: red.clone() }, TextRun { len: 2, style: blue.clone() }];
    let mut c = update(&s, id);
    c.edits[0].text = Some("AéBC".into());
    publish(&mut s, &c);
    let doc = &s.active().unwrap().doc;
    let LayerContent::Text(t) = &doc.layer(id).unwrap().content else { panic!() };
    assert_eq!(t.runs, vec![TextRun { len: 3, style: red }, TextRun { len: 2, style: blue }]);
    let bytes = photocraft_format::save_to_bytes(doc, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded.layer(id).unwrap().vector.on_path, doc.layer(id).unwrap().vector.on_path);
    assert_eq!(photocraft_compose::flatten(&loaded), photocraft_compose::flatten(doc));
    let raw = serde_json::to_value(&c.edits[0].path.path).unwrap();
    let old: PathText = serde_json::from_value(json!({"path":raw})).unwrap();
    assert_eq!((old.start, old.end, old.flip, old.align), (0., 1., false, PathTextAlign::Baseline));
}

#[test]
fn review_regression_path_text_hit_skips_ineligible_foreground_curves() {
    let (s, id) = session();
    let mut doc = (*s.active().unwrap().doc).clone();
    for inverted in [false, true] {
        let mut path = match &doc.layer(id).unwrap().content {
            LayerContent::Shape(s) => s.path.clone(),
            _ => panic!(),
        };
        if inverted {
            path.inverted = true;
        } else {
            path.subpaths.push(path.subpaths[0].clone());
        }
        doc.layers.push(Layer::new("Ineligible foreground", LayerContent::Shape(ShapeLayer { path, ..Default::default() })));
    }
    assert_eq!(curve_at(&doc, Point::new(90., 100.), 6.).unwrap().unwrap().0, id);
}
