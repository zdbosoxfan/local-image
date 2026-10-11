use super::*;
use photocraft_doc::{
    Color, ColorMode, SampleType, Size, Subpath, TextLayer,
    text::{CharStyle, TextRun},
};
pub(super) fn fixture(text: bool) -> (Session, LayerId, LayerId) {
    let mut s = Session::new();
    let mut doc = Document::new("Envelope", Size::new(192, 160), ColorMode::Rgb, SampleType::U8);
    let mut sh = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(20.0, 30.0), (140.0, 30.0), (140.0, 130.0), (20.0, 130.0)])]),
        fill: Some(Fill::Solid(Color::BLACK)),
        ..Default::default()
    };
    crate::vector_cmds::refresh_shape(&doc, &mut sh);
    let shape = Layer::new("Path", LayerContent::Shape(sh));
    let shape_id = shape.id;
    let mut children = vec![shape];
    if text {
        let mut t = TextLayer { text: "OABC".into(), transform: photocraft_geom::Affine::translate(35.0, 85.0), ..Default::default() };
        t.runs = vec![TextRun { len: t.text.len(), style: CharStyle { size_pt: 18.0, color: Color::rgb(1.0, 0.0, 0.0), ..Default::default() } }];
        crate::type_cmds::refresh(&doc, &mut t);
        children.push(Layer::new("Text", LayerContent::Text(t)));
    }
    let group = Layer::group("Sources", children);
    let id = group.id;
    doc.layers.push(group);
    s.add_document(doc, None);
    (s, id, shape_id)
}
fn warp() -> EnvelopeSpec {
    EnvelopeSpec::Warp { style: "arch".into(), bend: 55.0, horizontal: 0.0, vertical: 0.0 }
}
fn commit(s: &mut Session, id: LayerId) {
    let mut g = Gesture::begin(s, &[id]).unwrap();
    g.update(warp()).unwrap();
    g.commit(s).unwrap();
}
#[test]
fn envelope_engine_preview_set_expand_undo_keep_sources_editable() {
    let (mut s, id, shape) = fixture(true);
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let mut g = Gesture::begin(&s, &[id]).unwrap();
    g.update(warp()).unwrap();
    let preview = preview_document(&s, &g.request).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, revision);
    assert!(s.journal.is_empty());
    let expected = photocraft_compose::render(&preview, preview.bounds());
    assert!(expected.px.iter().any(|p| p[3] > 0.5));
    assert!(expected.px.iter().any(|p| p[0] > 0.5 && p[1] < 0.1 && p[2] < 0.1 && p[3] > 0.9), "warped text must be visible independently of the black path");
    g.commit(&mut s).unwrap();
    let live = s.active().unwrap().doc.clone();
    assert_eq!(live.layer(shape).unwrap(), before.layer(shape).unwrap());
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().revision, revision + 1);
    let displayed = project_document(&live).unwrap();
    assert_eq!(photocraft_compose::render(&displayed, displayed.bounds()), expected);
    let expand = Gesture::begin(&s, &[id]).unwrap();
    s.execute("vector.envelope.expand", serde_json::to_value(expand.request).unwrap()).unwrap();
    let expanded = &s.active().unwrap().doc;
    assert!(expanded.layer(id).unwrap().vector.live.is_none());
    assert!(expanded.walk().iter().all(|(_, _, l)| !matches!(l.content, LayerContent::Text(_))));
    assert_eq!(photocraft_compose::render(expanded, expanded.bounds()), expected);
    assert_eq!(s.journal.len(), 2);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, live);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
}
#[test]
fn envelope_engine_stale_duplicate_overlapping_locked_and_malformed_publish_nothing() {
    let (mut s, id, shape) = fixture(false);
    let g = Gesture::begin(&s, &[id]).unwrap();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let mut c = g.request.clone();
    c.edits.push(c.edits[0].clone());
    assert!(s.execute("vector.envelope.set", serde_json::to_value(c).unwrap()).is_err());
    let mut c = g.request.clone();
    c.edits[0].sources.pop();
    assert!(preview_document(&s, &c).is_err());
    let mut c = g.request.clone();
    c.edits[0].spec = EnvelopeSpec::Mesh { rows: u32::MAX, cols: 1, points: vec![] };
    assert!(s.execute("vector.envelope.set", serde_json::to_value(c).unwrap()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert_eq!(s.active().unwrap().revision, revision);
    s.edit("Lock", |doc, _| {
        doc.layer_mut(shape).unwrap().locks.pixels = true;
        Ok(())
    })
    .unwrap();
    assert!(Gesture::begin(&s, &[id]).is_err());
    assert!(g.clone().commit(&mut s).is_err());
    assert!(s.journal.is_empty());
    s.undo();
    let g = Gesture::begin(&s, &[id]).unwrap();
    let (other, _, _) = fixture(false);
    s.add_document(other.active().unwrap().doc.as_ref().clone(), None);
    assert!(g.commit(&mut s).is_err());
}
#[test]
fn envelope_engine_live_source_edits_reproject_and_native_roundtrip() {
    let (mut s, id, shape) = fixture(true);
    commit(&mut s, id);
    let saved = s.active().unwrap().doc.clone();
    let a = project_document(&saved).unwrap();
    let replacement = Path::new(vec![Subpath::polygon(&[(20.0, 30.0), (130.0, 45.0), (110.0, 140.0), (30.0, 125.0)])]);
    s.execute("shape.applyPaths", json!({"edits":[{"layer":shape.0,"path":replacement}]})).unwrap();
    let b = project_document(&s.active().unwrap().doc).unwrap();
    assert_ne!(a.layer(shape), b.layer(shape));
    assert!(s.active().unwrap().doc.layer(id).unwrap().vector.live.is_some());
    let bytes = photocraft_format::save_to_bytes(&s.active().unwrap().doc, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded.layer(id).unwrap().vector.live, s.active().unwrap().doc.layer(id).unwrap().vector.live);
    assert_eq!(loaded.layer(shape).unwrap().content, s.active().unwrap().doc.layer(shape).unwrap().content);
    assert!(loaded.walk().iter().any(|(_, _, l)| matches!(l.content, LayerContent::Text(_))));
    assert_eq!(photocraft_compose::render(&project_document(&loaded).unwrap(), loaded.bounds()), photocraft_compose::render(&b, b.bounds()));
    s.undo();
    assert_eq!(s.active().unwrap().doc, saved);
}
#[test]
fn envelope_engine_display_of_locked_live_group_is_valid() {
    let (mut s, id, _) = fixture(false);
    commit(&mut s, id);
    s.edit("Lock", |d, _| {
        d.layer_mut(id).unwrap().locks.all = true;
        Ok(())
    })
    .unwrap();
    assert!(project_document(&s.active().unwrap().doc).is_ok());
    assert!(Gesture::begin(&s, &[id]).is_err());
}
#[test]
fn envelope_engine_multi_group_failure_is_atomic_and_nonjournaled_preview() {
    let (mut s, id, _) = fixture(false);
    let other =
        Layer::group("Second", vec![Layer::new("Unsupported", LayerContent::Raster(photocraft_doc::Surface::new(s.active().unwrap().doc.pixel_format())))]);
    let second = other.id;
    s.edit("Add", |d, _| {
        d.layers.push(other);
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    assert!(Gesture::begin(&s, &[id, second]).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    let g = Gesture::begin(&s, &[id]).unwrap();
    s.execute("vector.envelope.preview", serde_json::to_value(g.request).unwrap()).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
}
#[test]
fn envelope_engine_legacy_native_document_loads_without_envelope_fields() {
    let bytes = include_bytes!("../../../../pc-format/tests/fixtures/vector-v1.pcraft");
    let doc = photocraft_format::load_from_bytes(bytes).unwrap();
    assert_eq!(project_document(&doc).unwrap(), doc);
}
#[test]
fn envelope_engine_live_text_edit_changes_visible_projection_and_undo_restores() {
    let (mut s, id, _) = fixture(true);
    commit(&mut s, id);
    let before = s.active().unwrap().doc.clone();
    let a = project_document(&before).unwrap();
    let text = before.walk().into_iter().find(|(_, _, l)| matches!(l.content, LayerContent::Text(_))).unwrap().2.id;
    s.execute("type.edit", json!({"layer":text.0,"text":"B"})).unwrap();
    let b = project_document(&s.active().unwrap().doc).unwrap();
    assert_ne!(photocraft_compose::render(&a, a.bounds()), photocraft_compose::render(&b, b.bounds()));
    let LayerContent::Text(t) = &s.active().unwrap().doc.layer(text).unwrap().content else { panic!("editable text") };
    assert_eq!(t.text, "B");
    assert!(s.active().unwrap().doc.layer(id).unwrap().vector.live.is_some());
    s.undo();
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn envelope_engine_second_group_fit_error_never_publishes_first_group() {
    let (mut s, first, _) = fixture(false);
    let (other, second, _) = fixture(false);
    let layer = other.active().unwrap().doc.layer(second).unwrap().clone();
    s.edit("Fixture second", |doc, _| {
        doc.layers.push(layer);
        Ok(())
    })
    .unwrap();
    let mut gesture = Gesture::begin(&s, &[first, second]).unwrap();
    gesture.update(warp()).unwrap();
    gesture.request.edits[1].spec = EnvelopeSpec::TopObject { path: Path::default() };
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let history = s.active().unwrap().history.past_len();
    assert!(gesture.commit(&mut s).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, revision);
    assert_eq!(s.active().unwrap().history.past_len(), history);
    assert!(s.journal.is_empty());
}
#[test]
fn envelope_engine_preview_preserves_other_live_group_projection() {
    let (mut s, first, _) = fixture(false);
    let (other, second, _) = fixture(false);
    let layer = other.active().unwrap().doc.layer(second).unwrap().clone();
    s.edit("Fixture second", |doc, _| {
        doc.layers.push(layer);
        Ok(())
    })
    .unwrap();
    commit(&mut s, second);
    let displayed = project_document(&s.active().unwrap().doc).unwrap();
    let source = s.active().unwrap().doc.clone();
    let mut edit = Gesture::begin(&s, &[first]).unwrap();
    edit.update(EnvelopeSpec::Warp { style: "flag".into(), bend: 30.0, horizontal: 0.0, vertical: 0.0 }).unwrap();
    let preview = preview_document(&s, &edit.request).unwrap();
    assert_eq!(preview.layer(second), displayed.layer(second));
    assert_eq!(s.active().unwrap().doc, source);
}

#[test]
fn dsfix_v093_envelope_background_reused_across_drag_and_invalidated() {
    let (mut s, id, _) = fixture(false);
    let mut other = s.active().unwrap().doc.layer(id).unwrap().duplicate();
    other.vector.live = Some(LiveGroup::Envelope(warp()));
    let other_id = other.id;
    s.edit("other envelope", |doc, _| {
        doc.layers.push(other);
        Ok(())
    })
    .unwrap();
    let mut g = Gesture::begin(&s, &[id]).unwrap();
    let source = s.active().unwrap().doc.clone();
    let first = preview_background(&source, &g.request).unwrap();
    g.update(warp()).unwrap();
    let second = preview_background(&source, &g.request).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    let expected = project_document(&source).unwrap();
    assert_eq!(second.layer(other_id), expected.layer(other_id));
    let mut changed_targets = g.request.clone();
    changed_targets.edits.clear();
    let all = preview_background(&source, &changed_targets).unwrap();
    assert!(!Arc::ptr_eq(&first, &all));
    s.edit("new revision", |doc, _| {
        doc.name = "Changed".into();
        Ok(())
    })
    .unwrap();
    assert!(preview_document(&s, &g.request).is_err());
    let g = Gesture::begin(&s, &[id]).unwrap();
    let third = preview_background(&s.active().unwrap().doc, &g.request).unwrap();
    assert!(!Arc::ptr_eq(&first, &third));
}
