#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
fn session() -> Session {
    let mut doc = Document::new("Export", Size::new(96, 96), ColorMode::Rgb, SampleType::U8);
    doc.layers.push(Layer::new(
        "Curve",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(12., 12., 64., 64.),
            fill: Some(Fill::Solid(Color::BLACK)),
            ..Default::default()
        }),
    ));
    let mut s = Session::new();
    s.add_document(doc, None);
    s
}
fn request(s: &Session) -> Request {
    let st = s.active().unwrap();
    Request { document: st.doc.id.0, revision: st.revision, scope: Scope::Selection { layers: st.selected_layers() }, options: Default::default() }
}
#[test]
fn pdf_export_command_is_detached_and_non_journaled() {
    let mut s = session();
    let r = request(&s);
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    let result = s.execute("file.pdf.export", serde_json::to_value(&r).unwrap()).unwrap();
    assert_eq!(result["mime"], "application/pdf");
    assert!(!result["bytes"].as_array().unwrap().is_empty());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, r.revision);
    assert_eq!(s.active().unwrap().history.past_len(), history);
    assert!(s.journal.is_empty());
}
#[test]
fn pdf_export_rejects_stale_and_entire_missing_target_set() {
    let s = session();
    let mut r = request(&s);
    r.revision += 1;
    assert!(export(&s, &r, &|| false).is_err());
    r = request(&s);
    if let Scope::Selection { layers } = &mut r.scope {
        layers.push(LayerId(0));
    }
    assert!(export(&s, &r, &|| false).is_err());
    r = request(&s);
    r.document += 1;
    assert!(export(&s, &r, &|| false).is_err());
}
#[test]
fn pdf_export_cancelled_has_no_side_effects() {
    let s = session();
    let r = request(&s);
    assert!(export(&s, &r, &|| true).is_err());
    assert_eq!(s.active().unwrap().revision, r.revision);
    assert!(s.journal.is_empty());
}
#[test]
fn pdf_export_old_options_load_with_defaults() {
    assert_eq!(serde_json::from_str::<ExportOptions>("{}").unwrap(), ExportOptions::default());
}
