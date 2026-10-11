#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
struct Host {
    s: Session,
    state: State,
    ids: Vec<LayerId>,
}
pub(super) fn test_access() -> Access {
    Access {
        read: |l| {
            l.psd_blocks
                .iter()
                .find(|(k, _)| k == b"tF45")
                .map_or_else(|| Ok(Vec::new()), |(_, v)| serde_json::from_slice(v).map_err(|e| EngineError::Other(e.to_string())))
        },
        write: |l, e| {
            l.psd_blocks.retain(|(k, _)| k != b"tF45");
            if !e.is_empty() {
                l.psd_blocks.push((*b"tF45", serde_json::to_vec(&e).map_err(|e| EngineError::Other(e.to_string()))?.into()));
            }
            Ok(())
        },
    }
}
fn fixture() -> Host {
    let mut doc = Document::new("UI45", Size::new(128, 96), ColorMode::Rgb, SampleType::F32);
    let mut layer = Layer::new(
        "Source",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(20., 20., 70., 50.),
            fill: Some(Fill::Solid(Color::rgb(0.2, 0.5, 0.8))),
            ..Default::default()
        }),
    );
    engine::refresh(&doc, &mut layer, test_access()).unwrap();
    let id = layer.id;
    doc.layers.push(layer);
    let mut s = Session::new();
    s.add_document(doc, None);
    Host { s, state: State::default(), ids: vec![id] }
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(900., 650.)).build_ui_state(
        |ui, h| {
            ui.menu_button("Effect", |ui| h.state.menu(ui, &h.s, &h.ids, EffectTarget::Layer, test_access(), str::to_owned));
            h.state.panel(ui, &mut h.s, &h.ids, test_access(), engine::commit_with_access, str::to_owned);
            h.state.dialog(ui.ctx(), &mut h.s, test_access(), engine::commit_with_access, str::to_owned);
        },
        fixture(),
    )
}
fn click(h: &mut Harness<'_, Host>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
}
fn effects(h: &Host) -> Vec<PathEffect> {
    (test_access().read)(h.s.active().unwrap().doc.layer(h.ids[0]).unwrap()).unwrap()
}
fn add(h: &mut Harness<'_, Host>, group: &str, label: &str) {
    click(h, "Effect");
    h.get_by_label_contains(group).click();
    h.run_steps(3);
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    click(h, label);
}
fn type_number(h: &mut Harness<'_, Host>, label: &str, text: &str) {
    let input = h.get_by_role_and_label(egui::accesskit::Role::SpinButton, label);
    input.click();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, label).type_text(text);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}
#[test]
fn path_effects45_ui_menu_preview_type_cancel_commit_edit_and_hide() {
    let mut h = harness();
    let before = h.state().s.active().unwrap().doc.clone();
    add(&mut h, "Distort & Transform", "Twist");
    assert!(h.state().state.is_open());
    type_number(&mut h, "Angle", "90");
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert_eq!(h.state().s.active().unwrap().history.past_len(), 0);
    let preview = h.state().state.preview.clone().unwrap();
    assert_ne!(photocraft_compose::flatten(&preview).px, photocraft_compose::flatten(&before).px);
    click(&mut h, "Preview");
    assert!(h.state().state.preview.is_none());
    click(&mut h, "Preview");
    assert!(h.state().state.preview.is_some());
    click(&mut h, "Cancel");
    assert_eq!(h.state().s.active().unwrap().doc, before);
    add(&mut h, "Stylize", "Round Corners");
    type_number(&mut h, "Radius", "8");
    click(&mut h, "OK");
    assert_eq!(effects(h.state()).len(), 1);
    assert_eq!(h.state().s.active().unwrap().history.past_len(), 1);
    click(&mut h, "Round Corners");
    type_number(&mut h, "Radius", "12");
    click(&mut h, "OK");
    assert_eq!(effects(h.state())[0].effect, EffectKind::RoundCorners { radius: 12. });
    click(&mut h, "Effect visible");
    assert!(!effects(h.state())[0].enabled);
    assert_eq!(h.state().s.active().unwrap().history.past_len(), 3);
    assert!(h.state_mut().s.undo());
    assert!(effects(h.state())[0].enabled);
}
#[test]
fn path_effects45_ui_slider_drag_reorder_delete_and_stale_cancel() {
    let mut h = harness();
    add(&mut h, "Distort & Transform", "Pucker & Bloat");
    let before = h.state().s.active().unwrap().doc.clone();
    let rect = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Amount").rect();
    let from = rect.center();
    let to = egui::pos2(rect.left() + rect.width() * 0.8, from.y);
    h.event(egui::Event::PointerMoved(from));
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(to));
    h.run_steps(2);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state().state.preview.is_some());
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    click(&mut h, "OK");
    assert!(matches!(effects(h.state())[0].effect,EffectKind::PuckerBloat{amount} if amount.abs()>1.));
    add(&mut h, "Stylize", "Offset Path");
    click(&mut h, "OK");
    h.get_all_by_label("Move effect up").nth(1).unwrap().click();
    h.run_steps(3);
    assert!(matches!(effects(h.state())[0].effect, EffectKind::OffsetPath { .. }));
    h.get_all_by_label("Remove effect").nth(1).unwrap().click();
    h.run_steps(3);
    assert_eq!(effects(h.state()).len(), 1);
    click(&mut h, "Offset Path");
    h.state_mut()
        .s
        .edit("unrelated", |d, _| {
            d.name = "New revision".into();
            Ok(())
        })
        .unwrap();
    h.run_steps(2);
    assert!(!h.state().state.is_open());
    assert!(h.state().state.preview.is_none());
}

#[test]
fn path_effects45_ui_drag_reorders_effect_rows() {
    let mut h = harness();
    add(&mut h, "Stylize", "Round Corners");
    click(&mut h, "OK");
    add(&mut h, "Stylize", "Offset Path");
    click(&mut h, "OK");
    let from = h.get_all_by_label("Drag effect").nth(1).unwrap().rect().center();
    let to = h.get_all_by_label("Drag effect").next().unwrap().rect().center();
    h.event(egui::Event::PointerMoved(from));
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(from + egui::vec2(0., -8.)));
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(to));
    h.run_steps(2);
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(3);
    assert!(h.state().state.error.is_none());
    assert!(matches!(effects(h.state())[0].effect, EffectKind::OffsetPath { .. }));
    assert_eq!(h.state().s.active().unwrap().history.past_len(), 3);
}
