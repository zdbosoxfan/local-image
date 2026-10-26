use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use photocraft_doc::GradientMethod;
use serde_json::json;

#[test]
fn appearance_method_keyboard_choice_changes_real_paint_with_one_undo() {
    let mut session = Session::new();
    session.execute("file.new", json!({"width":64,"height":48})).unwrap();
    let value = session.execute("shape.create", json!({"rect":[2,2,56,40],"fill":{"gradient":{}}})).unwrap();
    let id = LayerId(value["layer"].as_u64().unwrap());
    let mut h = Harness::builder().with_size(egui::vec2(250.0, 1000.0)).build_ui_state(
        |ui, data: &mut (State, Session, Vec<LayerId>)| {
            if !ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                return;
            }
            panel(ui, &mut data.0, &mut data.1, &data.2).unwrap();
        },
        (State::default(), session, vec![id]),
    );
    li_ui_kit::fonts::install_editor(&h.ctx);
    h.run_steps(3);
    let history = h.state().1.active().unwrap().history.past_len();
    let method = h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Method");
    assert_eq!(method.accesskit_node().value().as_deref(), Some("Classic"));
    method.focus();
    h.key_press(Key::Space);
    h.run_steps(3);
    h.get_by_label("Perceptual").focus();
    h.key_press(Key::Enter);
    h.run_steps(3);
    let layer = h.state().1.active().unwrap().doc.layer(id).unwrap();
    let AppearanceItem::Fill(item) = &layer.vector.appearance.as_ref().unwrap().items[0] else { panic!("fill item") };
    assert!(matches!(item.paint, Fill::Gradient { method: GradientMethod::Perceptual, .. }));
    assert_eq!(h.state().1.active().unwrap().history.past_len(), history + 1);
    h.state_mut().1.execute("edit.undo", json!({})).unwrap();
    assert!(h.state().1.active().unwrap().doc.layer(id).unwrap().vector.appearance.is_none());
}
