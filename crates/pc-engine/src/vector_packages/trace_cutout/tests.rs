use super::*;
#[path = "gpu.rs"]
mod gpu;
use pc_trace::Preset;
use photocraft_doc::{ColorMode, SampleType, Size};
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
fn setup() -> (Session, LayerId) {
    let mut session = Session::new();
    let mut doc = Document::new("Cutout transactions", Size::new(64, 64), ColorMode::Rgb, SampleType::U8);
    let layer = Layer::raster("Source", doc.pixel_format());
    let target = layer.id;
    doc.layers.push(layer);
    session.add_document(doc, None);
    (session, target)
}
fn preview() -> Result<Trace> {
    evaluate(
        Image { width: 2, height: 2, rgba: &[200, 0, 0, 255, 0, 0, 200, 255, 200, 0, 0, 255, 0, 0, 200, 255] },
        &Params::for_preset(Preset::PixelArt),
        || false,
    )
}
#[test]
fn cutout_preview_prepared_commit_single_undo_and_redo() -> TestResult {
    let (mut session, target) = setup();
    let before = session.active().ok_or("no active document")?.doc.clone();
    let revision = session.active().ok_or("no active document")?.revision;
    let capture = Capture::begin(&session, target)?;
    assert_eq!(capture.document(), before.id);
    assert_eq!(capture.revision(), revision);
    assert_eq!(capture.target(), target);
    let prepared = capture.prepare(preview()?, Point::new(20., 20.), || false)?;
    assert_eq!(session.active().ok_or("no active document")?.doc, before);
    assert!(session.journal.is_empty());
    assert!(!session.active().ok_or("no active document")?.history.can_undo());
    let id = prepared.commit(&mut session)?;
    let after = session.active().ok_or("no active document")?.doc.clone();
    let LayerContent::Group(group) = &after.layer(id).ok_or("group missing")?.content else {
        return Err("expected group".into());
    };
    assert_eq!(group.children.len(), 2);
    for child in &group.children {
        let LayerContent::Shape(shape) = &child.content else {
            return Err("expected shape".into());
        };
        let cache = shape.cache.as_ref().ok_or("cache missing")?;
        assert!(cache.content_bounds().width() <= 4 && cache.content_bounds().height() <= 4);
    }
    assert_eq!(session.active().ok_or("no active document")?.revision, revision + 1);
    assert!(session.undo());
    assert_eq!(session.active().ok_or("no active document")?.doc, before);
    assert!(!session.undo());
    assert!(session.redo());
    assert_eq!(session.active().ok_or("no active document")?.doc, after);
    Ok(())
}
#[test]
fn cutout_stale_revision_document_and_target_locks_publish_nothing() -> TestResult {
    for mode in 0..3 {
        let (mut session, target) = setup();
        let prepared = Capture::begin(&session, target)?.prepare(preview()?, Point::new(0., 0.), || false)?;
        if mode == 0 {
            session.edit("External edit", |doc, _| {
                doc.name = "Changed".into();
                Ok(())
            })?;
        } else if mode == 1 {
            session.add_document(Document::new("Other", Size::new(64, 64), ColorMode::Rgb, SampleType::U8), None);
        } else {
            let state = session.active_mut().ok_or("no document")?;
            let mut doc = (*state.doc).clone();
            doc.layer_mut(target).ok_or("no target")?.locks.all = true;
            state.doc = Arc::new(doc);
        }
        let before = session.active().ok_or("no document")?.doc.clone();
        let revision = session.active().ok_or("no document")?.revision;
        assert!(prepared.commit(&mut session).is_err());
        assert_eq!(session.active().ok_or("no document")?.doc, before);
        assert_eq!(session.active().ok_or("no document")?.revision, revision);
        assert!(session.journal.is_empty());
    }
    Ok(())
}
#[test]
fn cutout_invalid_cancelled_preparation_and_inherited_locks() -> TestResult {
    let (mut session, target) = setup();
    let before = session.active().ok_or("no document")?.doc.clone();
    let capture = Capture::begin(&session, target)?;
    assert!(capture.clone().prepare(preview()?, Point::new(f64::NAN, 0.), || false).is_err());
    assert!(capture.clone().prepare(preview()?, Point::new(64., 0.), || false).is_err());
    assert!(capture.clone().prepare(preview()?, Point::new(0., 0.), || true).is_err());
    let mut bad = preview()?;
    bad.layers[0].path.subpaths[0].knots[0].anchor.x = f64::INFINITY;
    assert!(capture.prepare(bad, Point::new(0., 0.), || false).is_err());
    assert_eq!(session.active().ok_or("no document")?.doc, before);
    assert!(!session.undo());
    let mut doc = (*before).clone();
    let source = doc.layers.remove(0);
    let mut parent = Layer::group("Locked parent", vec![source]);
    parent.locks.pixels = true;
    let group_id = parent.id;
    doc.layers.push(parent);
    session.active_mut().ok_or("no document")?.doc = Arc::new(doc);
    assert!(Capture::begin(&session, target).is_err());
    assert!(Capture::begin(&session, group_id).is_err());
    assert!(Capture::begin(&session, LayerId(u64::MAX)).is_err());
    Ok(())
}
