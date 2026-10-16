use super::*;
use photocraft_doc::vector_design::drawing::{CornerKind, ShapeParameters as S};
use photocraft_doc::{ColorMode, SampleType, Size};
use std::f64::consts::PI;
fn session() -> Session {
    let mut s = Session::new();
    s.add_document(Document::new("Live shape tests", Size::new(160, 160), ColorMode::Rgb, SampleType::U8), None);
    s
}
fn design_shape() -> DesignShape {
    DesignShape { rect: [20., 30., 80., 60.], angle: 0., parameters: S::Rect { radii: [0.; 4], kinds: [CornerKind::Round; 4] } }
}
fn params(s: &Session) -> Value {
    let c = Capture::new(s).unwrap();
    json!({"document":c.document.0,"revision":c.revision})
}
fn create_shape(s: &mut Session, design: &DesignShape) -> LayerId {
    let mut p = params(s);
    p["design"] = json!(design);
    LayerId(s.execute("shape.design.create", p).unwrap()["layer"].as_u64().unwrap())
}
#[test]
fn shape_names_follow_kind_preserve_explicit_names_and_survive_native_roundtrip() {
    let mut s = session();
    let mut d = design_shape();
    let rectangle = create_shape(&mut s, &d);
    assert_eq!(s.active().unwrap().doc.layer(rectangle).unwrap().name, "Rectangle");
    d.parameters = S::Ellipse;
    let ellipse = create_shape(&mut s, &d);
    assert_eq!(s.active().unwrap().doc.layer(ellipse).unwrap().name, "Ellipse");
    let mut p = params(&s);
    p["design"] = json!(d);
    p["name"] = json!("  Authored badge  ");
    let id = LayerId(s.execute("shape.design.create", p).unwrap()["layer"].as_u64().unwrap());
    let created = s.active().unwrap().doc.clone();
    assert_eq!(created.layer(id).unwrap().name, "  Authored badge  ");
    d.rect[2] = 60.;
    let mut p = params(&s);
    p["edits"] = json!([{"layer":id.0,"design":d}]);
    s.execute("shape.design.set", p).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().name, "  Authored badge  ");
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, created);
    assert!(s.redo());
    let (bytes, _) = photocraft_format::PcraftWriter::new().save_zip(&s.active().unwrap().doc, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    for layer in &s.active().unwrap().doc.layers {
        assert_eq!(loaded.layer(layer.id).unwrap().name, layer.name);
    }
    let before = s.active().unwrap().doc.clone();
    let mut p = params(&s);
    p["design"] = json!(d);
    p["name"] = json!(17);
    assert!(s.execute("shape.design.create", p).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn live_shapes_create_preview_edit_convert_one_undo_each() {
    let mut s = session();
    let before = s.active().unwrap().doc.clone();
    let d = design_shape();
    let id = create_shape(&mut s, &d);
    let created = s.active().unwrap().doc.clone();
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, created);
    let mut changed = d;
    changed.angle = PI / 4.;
    let mut p = params(&s);
    p["design"] = json!(changed);
    let result = s.execute("shape.design.preview", p).unwrap();
    assert!(!result["path"].is_null());
    assert_eq!(s.active().unwrap().doc, created);
    assert_eq!(s.journal.len(), 1);
    let mut p = params(&s);
    p["edits"] = json!([{"layer":id.0,"design":changed}]);
    s.execute("shape.design.set", p).unwrap();
    let edited = s.active().unwrap().doc.clone();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, created);
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, edited);
    let mut p = params(&s);
    p["layers"] = json!([id.0]);
    s.execute("shape.design.convert", p).unwrap();
    let l = s.active().unwrap().doc.layer(id).unwrap();
    assert!(l.vector.live_shape.is_none());
    let LayerContent::Shape(sh) = &l.content else { panic!() };
    assert!(sh.live.is_none());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, edited);
}
#[test]
fn live_shapes_stale_atomic_targets_locks_and_invalid_parameters() {
    let mut s = session();
    let id = create_shape(&mut s, &design_shape());
    let before = s.active().unwrap().doc.clone();
    let journal = s.journal.len();
    let revision = s.active().unwrap().revision;
    for entries in [
        json!([{"layer":id.0,"design":design_shape()},{"layer":999999999,"design":design_shape()}]),
        json!([{"layer":id.0,"design":design_shape()},{"layer":id.0,"design":design_shape()}]),
        json!([{"layer":id.0,"design":{"rect":[0.,0.,0.,0.],"parameters":{"kind":"ellipse"}}}]),
    ] {
        let mut p = params(&s);
        p["edits"] = entries;
        assert!(s.execute("shape.design.set", p).is_err());
        assert_eq!(s.active().unwrap().doc, before);
        assert_eq!(s.journal.len(), journal);
        assert_eq!(s.active().unwrap().revision, revision);
    }
    let mut p = params(&s);
    p["revision"] = json!(revision - 1);
    p["design"] = json!(design_shape());
    assert!(s.execute("shape.design.create", p).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    let mut p = params(&s);
    p["design"] = json!(design_shape());
    p["above"] = json!("invalid ID");
    assert!(s.execute("shape.design.create", p).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), journal);
    let mut doc = (*before).clone();
    doc.layer_mut(id).unwrap().locks.pixels = true;
    s.active_mut().unwrap().doc = std::sync::Arc::new(doc);
    let before = s.active().unwrap().doc.clone();
    let mut p = params(&s);
    p["layers"] = json!([id.0]);
    assert!(s.execute("shape.design.convert", p).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn live_shapes_native_save_load_preserves_all_parameters_and_legacy_forms() {
    let mut s = session();
    for parameters in super::shape_tests::families() {
        let mut d = design_shape();
        d.rect = [0., 0., 100., 100.];
        d.parameters = parameters;
        create_shape(&mut s, &d);
    }
    for kind in ["rect", "roundedRect", "ellipse", "polygon", "star", "line"] {
        s.execute("shape.create", json!({"kind":kind,"rect":[5,5,30,20],"radii":[3,3,3,3],"from":[5,5],"to":[30,20]})).unwrap();
    }
    let doc = &s.active().unwrap().doc;
    let (bytes, _) = photocraft_format::PcraftWriter::new().save_zip(doc, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded.layers.len(), doc.layers.len());
    for (a, b) in doc.layers.iter().zip(&loaded.layers) {
        assert_eq!(a.vector, b.vector);
        let (LayerContent::Shape(sa), LayerContent::Shape(sb)) = (&a.content, &b.content) else { panic!() };
        assert_eq!(sa.path, sb.path);
        assert_eq!(sa.live, sb.live);
    }
}
#[test]
fn live_shapes_frozen_v1_document_loads_without_new_metadata() {
    let bytes = include_bytes!("../../../../pc-format/tests/fixtures/vector-basic.pcraft");
    let doc = photocraft_format::load_from_bytes(bytes).unwrap();
    assert!(!doc.layers.is_empty());
    assert!(doc.layers.iter().all(|l| l.vector.live_shape.is_none()));
}
#[test]
fn live_shapes_cache_allocation_is_shape_and_document_bounded() {
    let doc = Document::new("Large canvas", Size::new(6000, 4000), ColorMode::Rgb, SampleType::U8);
    let sh = prepare(&doc, &design_shape(), ShapeLayer { fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() }).unwrap();
    assert_eq!(sh.cache.as_ref().unwrap().tile_count(), 1);
    let mut outside = design_shape();
    outside.rect = [7000., 5000., 80., 60.];
    let sh = prepare(&doc, &outside, sh).unwrap();
    assert_eq!(sh.cache.as_ref().unwrap().tile_count(), 0);
}
