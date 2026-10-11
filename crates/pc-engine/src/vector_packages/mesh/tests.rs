#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Layer, SampleType, ShapeLayer, Size};
pub(super) fn fixture() -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Mesh fixture", Size::new(100, 100), ColorMode::Rgb, SampleType::F32);
    let mut ids = Vec::new();
    for _ in 0..2 {
        let mut shape =
            ShapeLayer { path: photocraft_vector::shapes::rect(5., 5., 90., 90.), fill: Some(Fill::Solid(Color::rgb(0.2, 0.4, 0.8))), ..Default::default() };
        shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
        let layer = Layer::new("Mesh shape", LayerContent::Shape(shape));
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, ids)
}
fn mesh() -> MeshData {
    Mesh::regular([5., 5., 95., 95.], 2, 2, mesh_paint::Appearance::Flat, 0., |_| [0.2, 0.4, 0.8, 1.]).unwrap().into()
}
fn read(_: &Layer) -> Result<Option<MeshData>> {
    Ok(Some(mesh()))
}
fn must_not_write(_: &mut ShapeEdit, _: &MeshData) -> Result<()> {
    panic!("validation must finish before write")
}
fn must_not_render(_: &Document, _: &ShapeEdit) -> Result<Surface> {
    panic!("validation must finish before render")
}
fn hooks() -> Hooks {
    Hooks { read, write: must_not_write, render: must_not_render, ..NATIVE }
}
#[test]
fn mesh43_engine_wire_roundtrip_defaults_and_bounded_numbers() {
    let m = mesh();
    assert_eq!(serde_json::from_value::<MeshData>(json!(m)).unwrap(), m);
    let mut wire = json!(m);
    wire.as_object_mut().unwrap().remove("dither");
    wire.as_object_mut().unwrap().remove("seed");
    for p in wire["points"].as_array_mut().unwrap() {
        p.as_object_mut().unwrap().remove("opacity");
    }
    let loaded: MeshData = parse(&wire).unwrap();
    assert!(loaded.dither);
    assert_eq!(loaded.seed, 0);
    assert!(loaded.points.iter().all(|p| p.opacity == 1.));
    loaded.validate().unwrap();
    let mut bad = loaded;
    bad.points[0].opacity = f32::INFINITY;
    assert!(bad.validate().is_err());
}
#[test]
fn mesh43_engine_create_captures_targets_and_samples_legacy_fill_without_edits() {
    let (s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let g = Gesture::begin(&s, &ids, Some(&Options { rows: 2, cols: 3, appearance: Appearance::ToCenter, highlight: 50. })).unwrap();
    assert_eq!(g.request().edits.len(), 2);
    assert_eq!((g.request().edits[0].mesh.rows, g.request().edits[0].mesh.cols), (2, 3));
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert_eq!(s.active().unwrap().history.past_len(), 0);
}
#[test]
fn mesh43_engine_invalid_last_target_duplicate_locked_stale_are_atomic() {
    let (mut s, ids) = fixture();
    let g = Gesture::begin_with(&s, &ids, None, hooks()).unwrap();
    let valid = g.request().clone();
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    let mut bad = valid.clone();
    bad.edits[1].mesh.points[0].position[0] = f64::NAN;
    assert!(prepare_with(&s, &bad, hooks()).unwrap_err().to_string().contains("finite"));
    let mut bad = valid.clone();
    bad.edits[1].layer = LayerId(u64::MAX);
    assert!(prepare_with(&s, &bad, hooks()).is_err());
    let mut bad = valid.clone();
    bad.edits[1].layer = ids[0];
    assert!(prepare_with(&s, &bad, hooks()).is_err());
    let mut bad = valid.clone();
    bad.document += 1;
    assert!(prepare_with(&s, &bad, hooks()).is_err());
    let mut bad = valid.clone();
    bad.revision += 1;
    assert!(prepare_with(&s, &bad, hooks()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, rev);
    assert!(s.journal.is_empty());
    let mut locked = (*before).clone();
    locked.layer_mut(ids[1]).unwrap().locks.pixels = true;
    s.active_mut().unwrap().doc = std::sync::Arc::new(locked);
    assert!(prepare_with(&s, &valid, hooks()).is_err());
    assert!(s.journal.is_empty());
}
#[test]
fn mesh43_engine_gesture_update_rejects_last_target_without_partial_mutation() {
    let (s, ids) = fixture();
    let mut g = Gesture::begin_with(&s, &ids, None, hooks()).unwrap();
    let before = g.request().clone();
    assert!(g.update(|id, m| m.set_color(0, [1., 0., 0., 1.], if id == ids[1] { 2. } else { 1. })).is_err());
    assert_eq!(g.request(), &before);
}
#[test]
fn mesh43_engine_empty_target_and_create_options_reject_without_history() {
    let (s, ids) = fixture();
    assert!(Gesture::begin(&s, &[], Some(&Options::default())).is_err());
    assert!(Gesture::begin(&s, &[ids[0], ids[0]], Some(&Options::default())).is_err());
    assert!(Gesture::begin(&s, &ids, Some(&Options { rows: 51, ..Default::default() })).is_err());
    assert!(s.journal.is_empty());
}
#[test]
fn review_regression_mesh43_engine_command_specs_journal_edits_only_and_native_schema_is_used() {
    let commands = specs();
    assert_eq!(commands.len(), 4);
    for c in &commands {
        assert_eq!(c.journal, c.id == "object.mesh.set" || c.id == "object.mesh.create");
    }
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let g = Gesture::begin(&s, &ids, Some(&Options::default())).unwrap();
    let result = (commands.iter().find(|c| c.id == "object.mesh.set").unwrap().run)(&mut s, &json!(g.request()));
    result.unwrap();
    assert_ne!(s.active().unwrap().doc, before);
    assert!((NATIVE.read)(s.active().unwrap().doc.layer(ids[0]).unwrap()).unwrap().is_some());
    assert!(s.journal.is_empty()); // Calling the handler bypasses Session::execute journaling.
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn mesh43_engine_old_v1_native_document_loads() {
    let old = photocraft_format::load_from_bytes(include_bytes!("../../../../pc-format/tests/fixtures/vector-v1.pcraft")).unwrap();
    for l in &old.layers {
        assert!(read_native(l).unwrap().is_none());
    }
}
#[test]
fn mesh43_native_save_load_undo_redo_preview_and_release() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let g = Gesture::begin(&s, &ids, Some(&Options { rows: 2, cols: 2, ..Default::default() })).unwrap();
    let shown = preview_document(&s, g.request()).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    g.commit(&mut s).unwrap();
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert_eq!(*s.active().unwrap().doc, shown);
    let bytes = photocraft_format::save_to_bytes(&shown, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(shown, loaded);
    for id in &ids {
        assert_eq!(read_native(shown.layer(*id).unwrap()).unwrap(), read_native(loaded.layer(*id).unwrap()).unwrap());
    }
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    assert_eq!(*s.active().unwrap().doc, shown);
}
#[path = "gpu_tests.rs"]
mod gpu_tests;
