//! Actual clicks and typed precision drive the captured dialog and existing write service.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Artboard, Color, ColorMode, Document, Fill, Layer, LayerContent, Rect, SampleType, ShapeLayer, Size};
use std::sync::{Arc, Mutex};
struct Host {
    session: Session,
    state: State,
    services: crate::Services,
    written: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Whether the save dialog answers with a file (else it is cancelled).
    pick: bool,
}
fn harness(pick: bool) -> Harness<'static, Host> {
    let mut doc = Document::new("Dialog", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    let shape = Layer::new(
        "Curve",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(24., 40., 48., 64.),
            fill: Some(Fill::Solid(Color::BLACK)),
            ..Default::default()
        }),
    );
    let id = shape.id;
    let mut board = Layer::group("First board", vec![shape]);
    if let LayerContent::Group(g) = &mut board.content {
        g.artboard = Some(Artboard::new(Rect::new(16, 24, 112, 120)));
    }
    doc.layers.push(board);
    let mut session = Session::new();
    session.add_document(doc, None);
    session.select_layer(id).unwrap();
    let mut state = State::default();
    state.open(&session).unwrap();
    let written = Arc::new(Mutex::new(Vec::new()));
    let record = written.clone();
    let services = crate::Services {
        write: Some(Box::new(move |_, bytes| {
            record.lock().unwrap().push(bytes.to_vec());
            Ok(())
        })),
        ..Default::default()
    };
    Harness::builder().with_size(egui::vec2(700., 600.)).build_ui_state(
        |ui, h: &mut Host| {
            if let Some(action) = controls(ui, &mut h.state, str::to_owned) {
                match action {
                    Action::Export => {
                        let pick = h.pick;
                        if let Err(error) = save(&mut h.state, &mut h.session, &mut h.services, |_| pick.then(|| "synthetic.svg".into())) {
                            h.state.error = Some(error.to_string());
                        }
                    }
                    Action::Cancel => h.state.cancel(),
                }
            }
        },
        Host { session, state, services, written, pick },
    )
}
#[test]
fn svg_export_dialog_selection_curves_minify_and_export_click() {
    let mut h = harness(true);
    let revision = h.state().session.active().unwrap().revision;
    h.get_by_label("Selection").click();
    h.get_by_label("Text as curves").click();
    h.get_by_label("Minify").click();
    h.run();
    assert!(h.state().state.options.text_as_curves);
    assert!(h.state().state.options.minify);
    h.get_by_label("Export").click();
    h.run();
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    let written = h.state().written.lock().unwrap();
    assert_eq!(written.len(), 1);
    let text = std::str::from_utf8(&written[0]).unwrap();
    assert!(text.contains("viewBox=\"24 40 48 64\""));
    assert!(!text.contains('\n'));
    assert_eq!(h.state().session.active().unwrap().revision, revision);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn svg_export_dialog_artboard_and_cancel_click() {
    let mut h = harness(true);
    h.get_by_label("Artboard").click();
    h.run();
    h.get_by_label("First board").click();
    h.get_by_label("Export").click();
    h.run();
    assert_eq!(h.state().written.lock().unwrap().len(), 1);
    let text = h.state().written.lock().unwrap()[0].clone();
    assert!(std::str::from_utf8(&text).unwrap().contains("viewBox=\"16 24 96 96\""));
    let mut h = harness(true);
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().state.request.is_none());
    assert!(h.state().written.lock().unwrap().is_empty());
}
#[test]
fn svg_export_dialog_picker_cancel_and_stale_export_never_write() {
    let mut h = harness(false);
    h.get_by_label("Export").click();
    h.run();
    assert!(h.state().written.lock().unwrap().is_empty());
    let mut h = harness(true);
    h.state_mut()
        .session
        .edit("Late edit", |doc, _| {
            doc.name = "Updated".into();
            Ok(())
        })
        .unwrap();
    h.get_by_label("Export").click();
    h.run();
    assert!(h.state().written.lock().unwrap().is_empty());
    assert!(h.state().state.error.as_ref().unwrap().contains("stale"));
}
#[test]
fn svg_export_dialog_typed_decimal_places() {
    let mut h = harness(true);
    h.get_by_label("Decimal places").click();
    h.run();
    h.get_by_label("Decimal places").click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_by_label("Decimal places").type_text("3");
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().state.options.precision, 3);
}
