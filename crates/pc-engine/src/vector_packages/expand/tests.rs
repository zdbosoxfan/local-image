#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::{
    appearance::{Appearance, FillItem},
    drawing::{DesignShape, ShapeParameters},
    live::RepeatSpec,
};
use photocraft_doc::{Color, ColorMode, LayerId, SampleType, ShapeStroke, Size};
use std::sync::Arc;
pub(super) fn fixture() -> (Session, LayerId) {
    let mut doc = Document::new("Expand 36", Size::new(96, 96), ColorMode::Rgb, SampleType::F32);
    let sh = ShapeLayer { path: photocraft_vector::shapes::rect(20., 20., 50., 50.), ..Default::default() };
    let mut l = Layer::new("Four paints", LayerContent::Shape(sh));
    let fill = |color| AppearanceItem::Fill(FillItem { paint: Fill::Solid(color), opacity: 0.7, blend: photocraft_color::BlendMode::Normal, visible: true });
    let stroke = |color, width| {
        AppearanceItem::Stroke(StrokeItem { base: ShapeStroke { width, paint: Fill::Solid(color), opacity: 0.6, ..Default::default() }, ..Default::default() })
    };
    l.vector.appearance =
        Some(Appearance { items: vec![fill(Color::rgb(0.8, 0.1, 0.1)), stroke(Color::BLACK, 8.), fill(Color::rgb(0.1, 0.8, 0.1)), stroke(Color::WHITE, 2.)] });
    l.opacity = 0.8;
    refresh_tree(&doc, &mut l, &mut 0).unwrap();
    let id = l.id;
    doc.layers.push(l);
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, id)
}
pub(super) fn request(s: &Session, ids: &[LayerId]) -> Request {
    Request { capture: Capture::begin(s, ids).unwrap(), options: Default::default(), appearance: true }
}
pub(super) fn max_diff(a: &Document, b: &Document) -> f32 {
    let a = photocraft_compose::flatten(a);
    let b = photocraft_compose::flatten(b);
    assert_eq!(a.px.len(), b.px.len());
    a.px.iter()
        .zip(&b.px)
        .flat_map(|(a, b)| {
            (0..4).map(move |c| {
                assert!(a[c].is_finite() && b[c].is_finite());
                if c == 3 { (a[c] - b[c]).abs() } else { (a[c] * a[3] - b[c] * b[3]).abs() }
            })
        })
        .fold(0., f32::max)
}
#[test]
fn object36_expand_four_paints_cpu_composite_order_metadata_native_and_one_undo() {
    let (mut s, id) = fixture();
    let before = s.active().unwrap().doc.clone();
    let r = request(&s, &[id]);
    let preview = prepare(&s, &r).unwrap();
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    assert!(s.journal.is_empty());
    let group = preview.document.layer(id).unwrap();
    let children = group.children().unwrap();
    assert_eq!(children.len(), 4);
    assert_eq!(group.opacity, 0.8);
    assert!(group.vector.appearance.is_none());
    for l in children {
        let LayerContent::Shape(sh) = &l.content else { panic!("ordinary path") };
        assert!(sh.stroke.is_none());
        assert!(l.vector.appearance.is_none());
        assert!(sh.cache.is_some());
    }
    let diff = max_diff(&before, &preview.document);
    assert!(diff <= 1. / 255., "premultiplied CPU max diff {diff}");
    s.execute("object.expand", json!(r)).unwrap();
    let committed = &s.active().unwrap().doc;
    assert_eq!(committed.layer(id).unwrap().children().unwrap().len(), 4);
    assert!(max_diff(committed, &preview.document) <= 1. / 255.);
    for (a, b) in committed.layer(id).unwrap().children().unwrap().iter().zip(children) {
        let (LayerContent::Shape(a), LayerContent::Shape(b)) = (&a.content, &b.content) else { panic!("plain paths") };
        assert_eq!(a.path, b.path);
        assert_eq!(a.fill, b.fill);
    }
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    let bytes = photocraft_format::save_to_bytes(&preview.document, &Default::default()).unwrap();
    assert_eq!(photocraft_format::load_from_bytes(&bytes).unwrap(), preview.document);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn object36_expand_unchecked_stroke_remains_stroke_and_default_options_load() {
    let (s, id) = fixture();
    let mut r = request(&s, &[id]);
    r.appearance = false;
    r.options.stroke = false;
    let p = prepare(&s, &r).unwrap();
    let c = p.document.layer(id).unwrap().children().unwrap();
    assert!(matches!(&c[1].content,LayerContent::Shape(sh) if sh.stroke.is_some()));
    assert!(matches!(&c[3].content,LayerContent::Shape(sh) if sh.stroke.is_some()));
    assert!(max_diff(&s.active().unwrap().doc, &p.document) <= 1. / 255.);
    let v = json!({"capture":r.capture});
    let parsed: Request = serde_json::from_value(v).unwrap();
    assert_eq!(parsed.options, Options::default());
}
#[test]
fn object36_expand_live_shape_repeat_and_symbol_match_projected_pixels() {
    let mut doc = Document::new("Live owners", Size::new(140, 100), ColorMode::Rgb, SampleType::F32);
    let design = DesignShape { rect: [10., 12., 20., 18.], angle: 0., parameters: ShapeParameters::Ellipse };
    let sh = ShapeLayer { path: super::super::drawing::geometry::generate(&design).unwrap(), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    let mut l = Layer::new("Ellipse", LayerContent::Shape(sh));
    l.vector.live_shape = Some(design);
    let id = l.id;
    refresh_tree(&doc, &mut l, &mut 0).unwrap();
    doc.layers.push(l);
    let mut s = Session::new();
    s.add_document(doc, None);
    let before = s.active().unwrap().doc.clone();
    let p = prepare(&s, &request(&s, &[id])).unwrap();
    assert!(max_diff(&before, &p.document) <= 1. / 255.);
    assert!(p.document.walk().iter().all(|(_, _, l)| l.vector.live_shape.is_none()));
    let mut doc = (*before).clone();
    let mut group = Layer::group("Repeat", doc.layers.clone());
    group.vector.live = Some(LiveGroup::Repeat(RepeatSpec::Grid { rows: 2, cols: 3, spacing: [5., 5.] }));
    if let Some(children) = group.children_mut() {
        children[0].vector.live_shape = None;
    }
    let id = group.id;
    doc.layers = vec![group];
    s.active_mut().unwrap().doc = Arc::new(doc);
    let projected = super::super::repeat::project_document(&s.active().unwrap().doc).unwrap();
    let p = prepare(&s, &request(&s, &[id])).unwrap();
    assert!(max_diff(&projected, &p.document) <= 1. / 255.);
    assert!(p.document.layer(id).unwrap().vector.live.is_none());
    let mut doc = (*before).clone();
    doc.layers[0].vector.live_shape = None;
    let mut session = Session::new();
    session.add_document(doc, None);
    let st = session.active().unwrap();
    let out =
        session.execute("symbol.create", json!({"document":st.doc.id.0,"revision":st.revision,"layers":[st.doc.layers[0].id.0],"name":"Ellipse"})).unwrap();
    let id = LayerId(out["layer"].as_u64().unwrap());
    let source = session.active().unwrap().doc.clone();
    let p = prepare(&session, &request(&session, &[id])).unwrap();
    assert!(max_diff(&source, &p.document) <= 1. / 255.);
    assert!(p.document.walk().iter().all(|(_, _, l)| !matches!(l.content, LayerContent::Symbol(_))));
}
#[test]
fn object36_expand_invalid_blend_envelope_last_target_are_atomic_and_explain() {
    let (mut s, id) = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    let mut group = Layer::group("Blend", vec![doc.layers[0].duplicate()]);
    group.vector.live = Some(LiveGroup::Blend(Default::default()));
    let unavailable = group.id;
    doc.layers.push(group);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let before = s.active().unwrap().doc.clone();
    let r = request(&s, &[id, unavailable]);
    let reason = disabled_reason(&s, &r).unwrap();
    assert!(!reason.is_empty(), "invalid one-child blend explains its failure");
    assert!(s.execute("object.expand", json!(r)).is_err());
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    assert!(s.journal.is_empty());
    assert_eq!(s.active().unwrap().history.past_len(), 0);
    let mut doc = (*before).clone();
    doc.layer_mut(unavailable).unwrap().vector.live = Some(LiveGroup::Envelope(photocraft_doc::vector_design::live::EnvelopeSpec::Warp {
        style: "unsupported-style".into(),
        bend: 0.,
        horizontal: 0.,
        vertical: 0.,
    }));
    s.active_mut().unwrap().doc = Arc::new(doc);
    assert!(!disabled_reason(&s, &request(&s, &[unavailable])).unwrap().is_empty());
    let mut r = request(&s, &[id]);
    r.capture.revision += 1;
    assert!(s.execute("object.expand", json!(r)).is_err());
    assert!(s.journal.is_empty());
}
#[test]
fn object36_expand_variable_width_uses_owner_outline() {
    let (mut s, id) = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    let l = doc.layer_mut(id).unwrap();
    let LayerContent::Shape(sh) = &mut l.content else { panic!("shape") };
    sh.path = Path::new(vec![photocraft_doc::Subpath::polyline(&[(20., 48.), (70., 48.)])]);
    let stroke =
        StrokeItem { base: ShapeStroke { width: 10., ..Default::default() }, profile: Some(super::super::width::geometry::uniform(5.)), ..Default::default() };
    l.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Stroke(stroke)] });
    let mut l = l.clone();
    refresh_tree(&doc, &mut l, &mut 0).unwrap();
    *doc.layer_mut(id).unwrap() = l;
    s.active_mut().unwrap().doc = Arc::new(doc);
    let p = prepare(&s, &request(&s, &[id])).unwrap();
    assert!(max_diff(&s.active().unwrap().doc, &p.document) <= 1. / 255.);
}
#[cfg(test)]
#[path = "gpu.rs"]
mod gpu;

#[test]
fn object36_expand_live_corners_brush_and_path_text_use_owner_projections() {
    use photocraft_doc::vector_design::drawing::{Corner, CornerKind, KnotCorner};
    let (mut s, id) = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    let l = doc.layer_mut(id).unwrap();
    l.vector.appearance = None;
    l.vector.corners = vec![KnotCorner { subpath: 0, knot: 0, corner: Corner { radius: 8., kind: CornerKind::Round } }];
    if let LayerContent::Shape(sh) = &mut l.content {
        sh.fill = Some(Fill::Solid(Color::BLACK));
    }
    let projected = super::super::corners::project_document(&doc).unwrap();
    s.active_mut().unwrap().doc = Arc::new(doc);
    let p = prepare(&s, &request(&s, &[id])).unwrap();
    assert!(max_diff(&projected, &p.document) <= 1. / 255.);
    assert!(p.document.layer(id).unwrap().vector.corners.is_empty());

    let mut s = Session::new();
    s.add_document(Document::new("Brush expansion", Size::new(160, 120), ColorMode::Rgb, SampleType::F32), None);
    let capture = super::super::brushes::Capture::begin(&s, Default::default()).unwrap();
    let samples = vec![
        super::super::pencil::fitting::Sample { point: photocraft_geom::Point::new(20., 60.), pressure: 0.2 },
        super::super::pencil::fitting::Sample { point: photocraft_geom::Point::new(80., 60.), pressure: 1. },
        super::super::pencil::fitting::Sample { point: photocraft_geom::Point::new(140., 60.), pressure: 0.4 },
    ];
    super::super::brushes::prepare(&s, &capture, &samples).unwrap().commit(&mut s).unwrap();
    let source = s.active().unwrap().doc.clone();
    let id = source.layers[0].id;
    let r = request(&s, &[id]);
    let p = prepare(&s, &r).unwrap();
    assert!(max_diff(&source, &p.document) <= 1. / 255.);
    let journal = s.journal.len();
    s.execute("object.expand", json!(r)).unwrap();
    assert_eq!(s.journal.len(), journal + 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, source);

    use photocraft_doc::vector_design::text::{PathText, PathTextAlign};
    let mut s = Session::new();
    let mut doc = Document::new("Path text expansion", Size::new(256, 160), ColorMode::Rgb, SampleType::F32);
    let path = Path::new(vec![photocraft_doc::Subpath::polyline(&[(20., 100.), (220., 100.)])]);
    let mut t = photocraft_doc::TextLayer { text: "ABC".into(), font_family: "Inter".into(), size_pt: 20., ..Default::default() };
    let path_text = PathText { path, start: 0., end: 1., flip: false, align: PathTextAlign::Baseline };
    super::super::path_text::refresh(&doc, &mut t, &path_text).unwrap();
    let mut l = Layer::new("Type", LayerContent::Text(t));
    l.vector.on_path = Some(path_text);
    let id = l.id;
    doc.layers.push(l);
    s.add_document(doc, None);
    let before = s.active().unwrap().doc.clone();
    let r = request(&s, &[id]);
    let p = prepare(&s, &r).unwrap();
    assert!(photocraft_compose::flatten(&before).px.iter().any(|p| p[3] > 0.9));
    // Owner 12's text and vector rasterizers use different edge coverage. Its public
    // conversion contract checks filled-glyph coverage IoU >0.97, not exact AA samples.
    let a = photocraft_compose::flatten(&before);
    let b = photocraft_compose::flatten(&p.document);
    let intersection = a.px.iter().zip(&b.px).filter(|(a, b)| a[3] >= 0.5 && b[3] >= 0.5).count();
    let union = a.px.iter().zip(&b.px).filter(|(a, b)| a[3] >= 0.5 || b[3] >= 0.5).count();
    assert!(union > 0 && intersection as f64 / union as f64 > 0.97);
    s.execute("object.expand", json!(r)).unwrap();
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}

#[test]
fn review_regression_object36_fill_stroke_only_cannot_discard_live_group_or_path_text_geometry() {
    let (mut s, id) = fixture();
    let source = s.active().unwrap().doc.clone();
    let mut group = Layer::group("Repeat", source.layers.clone());
    group.vector.live = Some(LiveGroup::Repeat(RepeatSpec::Grid { rows: 2, cols: 2, spacing: [2., 2.] }));
    let gid = group.id;
    let mut doc = (*source).clone();
    doc.layers = vec![group];
    s.active_mut().unwrap().doc = Arc::new(doc);
    let mut r = request(&s, &[gid]);
    r.appearance = false;
    r.options.object = false;
    let before = s.active().unwrap().doc.clone();
    assert!(prepare(&s, &r).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    let mut layer = source.layer(id).unwrap().clone();
    layer.vector.on_path = Some(photocraft_doc::vector_design::text::PathText {
        path: photocraft_vector::shapes::rect(1., 1., 4., 4.),
        start: 0.,
        end: 1.,
        flip: false,
        align: photocraft_doc::vector_design::text::PathTextAlign::Baseline,
    });
    assert!(shape_expansion(&layer, r.options, false).is_err());
    assert!(s.journal.is_empty());
}
