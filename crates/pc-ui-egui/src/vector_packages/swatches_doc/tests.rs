#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{ColorMode, Document, Layer, LayerContent, SampleType, ShapeLayer, Size};
pub(crate) fn access() -> Access {
    // Test-only fake of the owned document storage provider. Production uses 00's fields.
    Access {
        read: |d| serde_json::from_str(d.metadata.xmp.as_deref().unwrap_or("{}")).map_err(|e| EngineError::Other(e.to_string())),
        write: |d, b| {
            d.metadata.xmp = Some(serde_json::to_string(&b).map_err(|e| EngineError::Other(e.to_string()))?);
            Ok(())
        },
    }
}
pub(crate) fn session() -> Session {
    let mut d = Document::new("UI colours", Size::new(96, 32), ColorMode::Rgb, SampleType::U8);
    for (i, c) in [Color::rgb(0.2, 0.4, 0.6), Color::rgb(0.8, 0.2, 0.1), Color::rgb(0.1, 0.8, 0.2)].iter().enumerate() {
        let mut sh = ShapeLayer { path: photocraft_vector::shapes::rect((i * 30) as f64, 0., 20., 20.), fill: Some(Fill::Solid(*c)), ..Default::default() };
        photocraft_engine::vector_cmds::refresh_shape(&d, &mut sh);
        d.layers.push(Layer::new("Object", LayerContent::Shape(sh)));
    }
    let mut book = SwatchBook::default();
    book.swatches.push(Swatch {
        id: SwatchId(1),
        name: "Brand".into(),
        paint: Fill::Solid(Color::rgb(0.2, 0.4, 0.6)),
        global: false,
        spot: false,
        group: None,
        stops: vec![],
    });
    (access().write)(&mut d, book).unwrap();
    let mut s = Session::new();
    s.add_document(d, None);
    s
}
struct Host {
    s: Session,
    state: State,
    error: Option<String>,
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(1000., 700.)).with_step_dt(1. / 60.).build_ui_state(
        |ui, h| {
            let event = panel(ui, &h.s, access(), &Fill::Solid(Color::BLACK), &[], &mut h.state);
            if let Ok(Some(Event::Commit(r))) = event {
                h.error = colours::commit(&mut h.s, access(), &r).err().map(|e| e.to_string());
            } else if let Err(e) = event {
                h.error = Some(e.to_string());
            }
        },
        Host { s: session(), state: State::default(), error: None },
    )
}
fn double_click(h: &mut Harness<'_, Host>, pos: egui::Pos2) {
    // Finish earlier click sequences, including egui's 600 ms triple-click window.
    h.run_steps(40);
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(1);
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos, pressed, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    }
    h.run();
}
#[test]
fn colour39_kittest_swatch_double_click_global_toggle_preview_cancel_and_apply() {
    let mut h = harness();
    h.run();
    let before = h.state().s.active().unwrap().doc.clone();
    let pos = h.get_by_label("Brand").rect().center();
    double_click(&mut h, pos);
    assert!(h.state().state.editing());
    assert!(h.state().state.preview.is_some());
    h.get_by_label("Global Colour").click();
    h.run();
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state().state.preview.as_ref().unwrap().book.swatches[0].global);
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().state.preview.is_none());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(!h.state().s.active().unwrap().history.can_undo());
    let pos = h.get_by_label("Brand").rect().center();
    double_click(&mut h, pos);
    assert!(h.state().state.editing());
    h.get_by_label("Global Colour").click();
    h.run();
    h.get_by_label("Apply").click();
    h.run();
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert!((access().read)(&h.state().s.active().unwrap().doc).unwrap().swatches[0].global);
    assert_eq!(h.state().s.active().unwrap().history.past_len(), 1);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
#[test]
fn colour39_kittest_add_used_and_document_revision_cancels_editor() {
    let mut h = harness();
    h.get_by_label("Add Used Colours").click();
    h.run();
    assert_eq!((access().read)(&h.state().s.active().unwrap().doc).unwrap().swatches.len(), 3);
    let pos = h.get_by_label("Brand").rect().center();
    double_click(&mut h, pos);
    h.state_mut()
        .s
        .edit("Change", |d, _| {
            d.name = "Changed".into();
            Ok(())
        })
        .unwrap();
    h.run();
    assert!(!h.state().state.editing());
    assert!(h.state().state.preview.is_none());
}
