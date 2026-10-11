use super::*;
use photocraft_doc::{Color, ColorMode, Fill, LayerContent, SampleType, ShapeLayer, Size};
pub(super) fn fixture() -> Session {
    let mut s = Session::new();
    let mut doc = Document::new("Blend", Size::new(160, 128), ColorMode::Rgb, SampleType::U8);
    for (x, r, c) in [(25., 8., Color::BLACK), (125., 14., Color::WHITE)] {
        let mut sh = ShapeLayer { path: photocraft_vector::shapes::ellipse(x - r, 50. - r, 2. * r, 2. * r), fill: Some(Fill::Solid(c)), ..Default::default() };
        sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
        doc.layers.push(Layer::new("Circle", LayerContent::Shape(sh)));
    }
    s.add_document(doc, None);
    s
}
pub(super) fn create(s: &mut Session, n: u32) -> LayerId {
    let ids = s.active().unwrap().doc.layers.iter().map(|l| l.id).collect();
    let capture = Capture::begin(s, ids).unwrap();
    let v = s
        .execute(
            "vector.blend.create",
            json!({"capture":capture,"spec":BlendSpec{spacing:photocraft_doc::vector_design::live::BlendSpacing::Steps(n),..Default::default()}}),
        )
        .unwrap();
    LayerId(v["layer"].as_u64().unwrap())
}
#[test]
fn blend18_engine_create_detached_options_expand_undo_and_journal() {
    let mut s = fixture();
    let original = s.active().unwrap().doc.clone();
    let id = create(&mut s, 5);
    assert_eq!(s.journal.len(), 1);
    let live = s.active().unwrap().doc.clone();
    assert_eq!(live.layer(id).unwrap().children().unwrap(), &original.layers);
    let capture = Capture::begin(&s, vec![id]).unwrap();
    let request = Request {
        capture: capture.clone(),
        operation: Operation::Options { spec: BlendSpec { spacing: photocraft_doc::vector_design::live::BlendSpacing::Steps(1), ..Default::default() } },
    };
    let detached = preview(&s, &request).unwrap();
    assert_eq!(detached.display.layer(id).unwrap().children().unwrap().len(), 3);
    assert_eq!(s.active().unwrap().doc, live);
    assert_eq!(s.journal.len(), 1);
    let projected = evaluator::project_document(&live).unwrap();
    s.execute("vector.blend.expand", json!({"capture":capture})).unwrap();
    assert_eq!(s.journal.len(), 2);
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap(), projected.layer(id).unwrap());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, live);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, original);
}
#[test]
fn blend18_engine_stale_duplicate_missing_and_locked_targets_publish_nothing() {
    let mut s = fixture();
    let id = create(&mut s, 1);
    let mut capture = Capture::begin(&s, vec![id]).unwrap();
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    let journal = s.journal.len();
    capture.revision += 1;
    assert!(s.execute("vector.blend.expand", json!({"capture":capture})).is_err());
    capture.revision = rev;
    capture.layers = vec![id, id];
    assert!(s.execute("vector.blend.expand", json!({"capture":capture})).is_err());
    capture.layers = vec![LayerId(u64::MAX)];
    assert!(s.execute("vector.blend.expand", json!({"capture":capture})).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, rev);
    assert_eq!(s.journal.len(), journal);
    s.edit("Lock key", |doc, _| {
        doc.layer_mut(id).unwrap().children_mut().unwrap()[1].locks.pixels = true;
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let journal = s.journal.len();
    let capture = Capture { document: before.id.0, revision, layers: vec![id] };
    assert!(s.execute("vector.blend.expand", json!({"capture":capture})).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, revision);
    assert_eq!(s.journal.len(), journal);
}
#[test]
fn blend18_engine_key_edits_reproject_and_spine_options_stay_atomic() {
    let mut s = fixture();
    let id = create(&mut s, 1);
    let key = s.active().unwrap().doc.layer(id).unwrap().children().unwrap()[0].id;
    let a = evaluator::project_document(&s.active().unwrap().doc).unwrap();
    s.execute("shape.applyPaths", json!({"edits":[{"layer":key,"path":photocraft_vector::shapes::ellipse(5.,30.,40.,40.)}]})).unwrap();
    let b = evaluator::project_document(&s.active().unwrap().doc).unwrap();
    assert_ne!(a.layer(id).unwrap().children().unwrap()[1], b.layer(id).unwrap().children().unwrap()[1]);
    let mut spec = spec(&s, id).unwrap();
    spec.spine = Some(photocraft_doc::Path::new(vec![photocraft_doc::Subpath::polyline(&[(25., 50.), (75., 100.), (125., 50.)])]));
    let before = s.active().unwrap().doc.clone();
    let capture = Capture::begin(&s, vec![id]).unwrap();
    s.execute("vector.blend.options", json!({"capture":capture,"spec":spec})).unwrap();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn blend18_engine_create_rejects_nonadjacent_keys_and_preserves_paint_order() {
    let mut s = fixture();
    let ids = s.active().unwrap().doc.layers.iter().map(|l| l.id).collect::<Vec<_>>();
    let capture = Capture::begin(&s, vec![ids[1], ids[0]]).unwrap();
    let v = s.execute("vector.blend.create", json!({"capture":capture,"spec":BlendSpec::default()})).unwrap();
    let id = LayerId(v["layer"].as_u64().unwrap());
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().children().unwrap().iter().map(|l| l.id).collect::<Vec<_>>(), ids);
    assert!(s.undo());
    s.edit("Middle", |doc, _| {
        doc.layers.insert(1, Layer::raster("Other", doc.pixel_format()));
        Ok(())
    })
    .unwrap();
    let capture = Capture::begin(&s, ids).unwrap();
    let before = s.active().unwrap().doc.clone();
    assert!(s.execute("vector.blend.create", json!({"capture":capture,"spec":BlendSpec::default()})).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
