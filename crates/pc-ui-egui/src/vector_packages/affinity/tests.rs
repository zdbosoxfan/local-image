#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
struct Host {
    state: State,
    session: Session,
    canvas: Rect,
}
fn harness() -> Harness<'static, Host> {
    let mut session = Session::new();
    session.execute("file.new", serde_json::json!({"width":128,"height":128,"background":"transparent"})).unwrap();
    Harness::builder().with_size(egui::vec2(700., 500.)).build_ui_state(
        |ui, h| {
            controls(ui, &mut h.state, &mut h.session);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(400., 200.), egui::Sense::click_and_drag());
            h.canvas = rect;
            input(&mut h.state, &mut h.session, ui.ctx(), rect, |p| p - rect.min.to_vec2(), true).unwrap();
        },
        Host {
            state: State { source: include_bytes!("../../../../pc-affinity/tests/fixtures/warnings.afphoto").to_vec(), ..Default::default() },
            session,
            canvas: Rect::NOTHING,
        },
    )
}
#[test]
fn affinity_ui_type_open_place_warning_and_undo() {
    let mut h = harness();
    let unrelated = photocraft_doc::Layer::group("Future", vec![]);
    let unrelated_id = unrelated.id;
    h.state_mut()
        .session
        .edit("Existing same name", move |doc, _| {
            doc.layers.push(unrelated);
            Ok(())
        })
        .unwrap();
    h.get_by_role(egui::accesskit::Role::TextInput).click();
    h.event(Event::Text("Imported artwork".into()));
    h.run_steps(2);
    assert_eq!(h.state().state.name, "Imported artwork");
    let before = h.state().session.active().unwrap().doc.clone();
    h.get_by_label("Place Affinity").click();
    h.run_steps(2);
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    h.get_by_label("unsupported-record: Future").click();
    h.run_steps(2);
    let s = h.state().session.active().unwrap();
    assert_eq!(s.doc.layer(s.active_layer.unwrap()).unwrap().name, "Future");
    assert_ne!(s.active_layer, Some(unrelated_id));
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(
        warning_target(&h.state().state, &h.state().session, h.state().state.warnings.iter().find(|w| w.object.as_deref() == Some("Future")).unwrap())
            .is_none()
    );
    h.get_by_label("Open Affinity").click();
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().doc.name, "Imported artwork");
    assert_eq!(h.state().session.documents().len(), 2);
}
#[test]
fn affinity_ui_live_drag_release_escape_cancel_stale_and_focus_loss() {
    let mut h = harness();
    h.get_by_label("Preview Affinity").click();
    h.run_steps(2);
    let before = h.state().session.active().unwrap().doc.clone();
    let revision = h.state().session.active().unwrap().revision;
    let a = h.state().canvas.min + egui::vec2(20., 20.);
    let b = a + egui::vec2(25., 15.);
    h.event(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.event(Event::PointerMoved(b));
    h.step();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.active().unwrap().revision, revision);
    let preview = h.state().state.preview.clone().unwrap();
    h.event(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.step();
    assert!(!h.state().state.pending());
    assert_eq!(h.state().session.active().unwrap().revision, revision + 1);
    assert_eq!(photocraft_compose::flatten(&preview), photocraft_compose::flatten(&h.state().session.active().unwrap().doc));
    assert!(h.state_mut().session.undo());
    for cancel in [0, 1, 2] {
        h.get_by_label("Preview Affinity").click();
        h.run_steps(2);
        assert!(h.state().state.pending());
        match cancel {
            0 => h.key_press(Key::Escape),
            1 => h.get_by_label("Cancel").click(),
            _ => h.event(Event::WindowFocused(false)),
        };
        h.run_steps(2);
        assert!(!h.state().state.pending());
    }
    h.event(Event::WindowFocused(true));
    h.step();
    h.get_by_label("Preview Affinity").click();
    h.run_steps(2);
    h.state_mut().session.execute("layer.new.layer", serde_json::json!({})).unwrap();
    h.step();
    assert!(!h.state().state.pending());
}
#[test]
fn affinity_ui_preview_fallback_warning_selects_pixels_and_place_refuses() {
    let mut h = harness();
    h.state_mut().state.source = include_bytes!("../../../../pc-affinity/tests/fixtures/preview.afphoto").to_vec();
    h.get_by_label("Place Affinity").click();
    h.run_steps(2);
    assert!(h.state().state.error.is_some());
    h.get_by_label("Open Affinity").click();
    h.run_steps(2);
    assert!(h.state().state.error.is_none());
    h.get_by_label("preview-fallback: Affinity preview").click();
    h.run_steps(2);
    let s = h.state().session.active().unwrap();
    assert!(matches!(s.doc.layer(s.active_layer.unwrap()).unwrap().content, photocraft_doc::LayerContent::Raster(_)));
}
