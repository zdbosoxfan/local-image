#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::swatches_doc::tests::{access, session};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
struct Host {
    s: Session,
    state: State,
    error: Option<String>,
}
fn harness() -> Harness<'static, Host> {
    let s = session();
    let ids = s.active().unwrap().doc.layers.iter().map(|l| l.id).collect::<Vec<_>>();
    let mut state = State::default();
    state.open(&s, access(), &ids).unwrap();
    Harness::builder().with_size(egui::vec2(1000., 800.)).build_ui_state(
        |ui, h| match dialog(ui.ctx(), &h.s, access(), &mut h.state) {
            Ok(Some(plan)) => h.error = recolor::commit(&mut h.s, access(), &plan).err().map(|e| e.to_string()),
            Err(e) => h.error = Some(e.to_string()),
            _ => {}
        },
        Host { s, state, error: None },
    )
}
#[test]
fn colour39_kittest_recolor_assign_typing_preview_cancel() {
    let mut h = harness();
    h.run();
    let before = h.state().s.active().unwrap().doc.clone();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Assign Row 1").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("FF0000".into()));
    h.run();
    assert_eq!(h.state().state.plan.as_ref().unwrap().rows[0].to, Color::rgb(1., 0., 0.));
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Assign Row 1").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("bad".into()));
    h.run();
    assert!(h.state().state.preview.is_none());
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled());
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Assign Row 1").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("FF0000".into()));
    h.run();
    assert!(h.state().state.preview.is_some());
    h.get_by_label("Preview").click();
    h.run();
    assert!(h.state().state.preview.is_none());
    h.get_by_label("Preview").click();
    h.run();
    assert!(h.state().state.preview.is_some());
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().state.plan.is_none());
    assert!(h.state().state.preview.is_none());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(!h.state().s.active().unwrap().history.can_undo());
}
#[test]
fn colour39_kittest_recolor_real_row_drag_assign_apply_one_undo() {
    let mut h = harness();
    h.run();
    let before = h.state().s.active().unwrap().doc.clone();
    let from = h.get_by_label("Colour 1").rect().center();
    let to = h.get_by_label("Row 2").rect().center();
    h.event(egui::Event::PointerMoved(from));
    h.event(egui::Event::PointerButton { pos: from, pressed: true, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(from + egui::vec2(8., 0.)));
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(to));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: to, pressed: false, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    h.run();
    assert_eq!(h.state().state.plan.as_ref().unwrap().rows.len(), 2);
    assert!(h.state().state.plan.as_ref().unwrap().rows.iter().any(|r| r.from.contains(&0) && r.from.contains(&1)));
    h.get_by_label("Preserve Lightness").click();
    h.get_by_label("Save as Swatch Group").click();
    h.run();
    h.get_by_label("Apply").click();
    h.run();
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert_eq!(h.state().s.active().unwrap().history.past_len(), 1);
    assert_eq!((access().read)(&h.state().s.active().unwrap().doc).unwrap().groups.len(), 1);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
