use super::*;
use photocraft_doc::{ColorMode, Fill, Layer, Path, SampleType, Size, Subpath};
use photocraft_geom::Affine;

fn setup() -> (Session, Vec<LayerId>) {
    let mut s = Session::new();
    let mut doc = Document::new("Vector transaction", Size::new(32, 32), ColorMode::Rgb, SampleType::U8);
    let path = Path::new(vec![Subpath::polygon(&[(1.0, 1.0), (8.0, 1.0), (8.0, 8.0), (1.0, 8.0)])]);
    let ids = (0..2)
        .map(|i| {
            let shape = ShapeLayer { path: path.clone(), fill: Some(Fill::Solid(photocraft_doc::Color::BLACK)), ..Default::default() };
            let l = Layer::new(format!("Shape {i}"), LayerContent::Shape(shape));
            let id = l.id;
            doc.layers.push(l);
            id
        })
        .collect();
    s.add_document(doc, None);
    (s, ids)
}
#[test]
fn vector_transaction_preview_commit_undo_redo_and_journal() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let before_revision = s.active().unwrap().revision;
    let mut edits = snapshot_shapes(&before, &ids).unwrap();
    for e in &mut edits {
        e.shape.path = e.shape.path.transform(&Affine::translate(10.0, 0.0));
    }
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    let params = json!({"edits": edits.iter().map(|e| json!({"layer": e.layer.0, "path": e.shape.path})).collect::<Vec<_>>()});
    s.execute("shape.applyPaths", params).unwrap();
    let after = s.active().unwrap().doc.clone();
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().revision, before_revision + 1);
    for id in &ids {
        let LayerContent::Shape(sh) = &after.layer(*id).unwrap().content else { panic!("shape") };
        assert!(sh.cache.as_ref().unwrap().sample_channel(12, 3, 3) > 0.99);
        assert_eq!(sh.path.control_bounds(), Some((11.0, 1.0, 18.0, 8.0)));
    }
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, after);
}
#[test]
fn vector_transaction_errors_leave_history_pixels_and_journal_untouched() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let path = json!({"subpaths":[]});
    for edits in [
        json!([]),
        json!([{"layer":ids[0].0,"path":path},{"layer":999999999,"path":path}]),
        json!([{"layer":ids[0].0,"path":path},{"layer":ids[0].0,"path":path}]),
        json!([{"layer":ids[0].0,"path":path},{"layer":ids[1].0,"path":{"subpaths":"invalid"}}]),
    ] {
        assert!(s.execute("shape.applyPaths", json!({"edits": edits})).is_err());
        assert_eq!(s.active().unwrap().doc, before);
        assert!(s.journal.is_empty());
        assert!(!s.undo());
    }
}
#[test]
fn vector_transaction_rejects_inherited_locks_and_non_shapes() {
    let (mut s, ids) = setup();
    let mut doc = (*s.active().unwrap().doc).clone();
    let mut parent = Layer::group("Locked", vec![doc.layers.remove(0)]);
    parent.locks.all = true;
    doc.layers.push(parent);
    let pixel = Layer::raster("Pixel", doc.pixel_format());
    let pixel_id = pixel.id;
    doc.layers.push(pixel);
    assert!(snapshot_shapes(&doc, &[ids[0]]).is_err());
    assert!(snapshot_shapes(&doc, &[pixel_id]).is_err());
    s.active_mut().unwrap().doc = std::sync::Arc::new(doc);
}
#[test]
fn vector_transaction_rejects_nonfinite_geometry_and_unrendered_edits() {
    let (mut s, ids) = setup();
    let mut edits = snapshot_shapes(&s.active().unwrap().doc, &ids).unwrap();
    assert!(apply_shape_edits(&mut s, "No cache", edits.clone()).is_err());
    edits[0].shape.path.subpaths[0].knots[0].anchor.x = f64::NAN;
    edits[0].shape.cache = Some(photocraft_doc::Surface::new(photocraft_doc::PixelFormat::RGBA8));
    assert!(apply_shape_edits(&mut s, "NaN", edits).is_err());
    assert!(!s.undo());
}
#[test]
fn vector_transaction_metadata_is_in_the_same_undo_state() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let mut edits = snapshot_shapes(&before, &ids).unwrap();
    for e in &mut edits {
        e.vector.constraints = Some(photocraft_doc::vector_design::structure::Constraints { right: true, fixed_width: true, ..Default::default() });
        crate::vector_cmds::refresh_shape(&before, &mut e.shape);
    }
    apply_shape_edits(&mut s, "Constraints with paths", edits).unwrap();
    assert!(s.active().unwrap().doc.layer(ids[0]).unwrap().vector.constraints.unwrap().right);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
