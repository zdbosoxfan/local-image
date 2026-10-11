use super::*;
use pc_trace::{Image, Params, cleanup::Options};
use photocraft_doc::{ColorMode, Document, SampleType, Size};
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
fn setup() -> (Session, Vec<LayerId>) {
    let mut session = Session::new();
    let mut doc = Document::new("Cleanup transaction", Size::new(64, 48), ColorMode::Rgb, SampleType::U8);
    let mut ids = Vec::new();
    for name in ["Source one", "Source two"] {
        let layer = Layer::raster(name, doc.pixel_format());
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    session.add_document(doc, None);
    (session, ids)
}
fn output() -> std::result::Result<Output, pc_trace::Error> {
    let mut rgba = vec![255; 32 * 32 * 4];
    for y in 6..26 {
        for x in 6..26 {
            rgba[(y * 32 + x) * 4..][..3].fill(0);
        }
    }
    super::super::evaluate(Image { width: 32, height: 32, rgba: &rgba }, &Params::default(), &Options { collinear: true, ..Options::default() }, || false)
}
#[test]
fn trace_cleanup_detached_preview_commit_and_single_undo() -> TestResult {
    let (mut session, ids) = setup();
    let before = session.active().ok_or("no doc")?.doc.clone();
    let rev = session.active().ok_or("no doc")?.revision;
    let capture = capture(&session, &ids)?;
    let out = output()?;
    let preview = prepare(&session, &capture, &out, [4, 3], || false)?;
    assert_eq!(session.active().ok_or("no doc")?.doc, before);
    assert_eq!(session.active().ok_or("no doc")?.revision, rev);
    assert!(session.journal.is_empty());
    let group = commit(&mut session, preview, "Vectorized", || false)?;
    let state = session.active().ok_or("no doc")?;
    assert!(state.doc.layer(group).is_some());
    assert_eq!(state.revision, rev + 1);
    assert!(session.undo());
    assert_eq!(session.active().ok_or("no doc")?.doc, before);
    assert!(!session.undo());
    assert!(session.redo());
    Ok(())
}
#[test]
fn trace_cleanup_rejects_stale_locked_entire_set_nonfinite_and_cancel() -> TestResult {
    let (mut session, ids) = setup();
    assert!(capture(&session, &[ids[0], ids[0]]).is_err());
    assert!(capture(&session, &[ids[0], LayerId(u64::MAX)]).is_err());
    let capture = capture(&session, &ids)?;
    let out = output()?;
    let before = session.active().ok_or("no doc")?.doc.clone();
    assert!(prepare(&session, &capture, &out, [0, 0], || true).is_err());
    let mut bad = out.clone();
    bad.trace.layers[0].path.subpaths[0].knots[0].anchor.x = f64::NAN;
    assert!(prepare(&session, &capture, &bad, [0, 0], || false).is_err());
    let preview = prepare(&session, &capture, &out, [0, 0], || false)?;
    assert!(commit(&mut session, preview, "Cancelled", || true).is_err());
    assert_eq!(session.active().ok_or("no doc")?.doc, before);
    assert!(!session.undo());
    let preview = prepare(&session, &capture, &out, [0, 0], || false)?;
    session.active_mut().ok_or("no doc")?.revision += 1;
    assert!(commit(&mut session, preview, "Stale", || false).is_err());
    session.active_mut().ok_or("no doc")?.revision -= 1;
    let preview = prepare(&session, &capture, &out, [0, 0], || false)?;
    let mut locked = (*before).clone();
    locked.layer_mut(ids[1]).ok_or("no layer")?.locks.all = true;
    session.active_mut().ok_or("no doc")?.doc = std::sync::Arc::new(locked.clone());
    assert!(commit(&mut session, preview, "Locked second target", || false).is_err());
    assert_eq!(*session.active().ok_or("no doc")?.doc, locked);
    assert!(!session.undo());
    assert!(session.journal.is_empty());
    Ok(())
}
