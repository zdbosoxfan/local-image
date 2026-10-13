#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{ColorMode, SampleType, Size};
struct Host {
    session: Session,
    brush: State,
    rect: egui::Rect,
    pressure: f32,
    error: Option<String>,
}
fn harness() -> Harness<'static, Host> {
    let mut session = Session::new();
    session.add_document(Document::new("Brush UI", Size::new(200, 200), ColorMode::Rgb, SampleType::U8), None);
    let mut h = Harness::builder().with_size(vec2(1300., 400.)).build_ui_state(
        |ui, h| {
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            ui.horizontal_wrapped(|ui| context_bar(ui, &mut h.brush));
            let (response, _) = ui.allocate_painter(vec2(200., 200.), egui::Sense::click_and_drag());
            h.rect = response.rect;
            let xf = ViewXform { rect: h.rect, zoom: 1., center: [100., 100.], flip: false, rotation: 0.0 };
            h.error = canvas_input_with_pressure(&mut h.brush, &mut h.session, &response, &xf, h.pressure).err().map(|e| e.to_string());
        },
        Host { session, brush: State::default(), rect: egui::Rect::NOTHING, pressure: 1., error: None },
    );
    crate::PhotocraftApp::setup_context(&h.ctx, Default::default());
    h.run_steps(3);
    h
}
fn event(h: &mut Harness<'_, Host>, p: (f32, f32), kind: u8, pressure: f32) {
    h.state_mut().pressure = pressure;
    let pos = h.state().rect.min + vec2(p.0, p.1);
    h.event(if kind == 1 {
        egui::Event::PointerMoved(pos)
    } else {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: kind == 0, modifiers: Modifiers::NONE }
    });
    h.run_steps(1);
}
#[test]
fn brush17_kittest_preset_pressure_drag_detached_preview_release_and_undo() {
    let mut h = harness();
    h.get_by_label("Pattern").click();
    h.run_steps(2);
    assert!(matches!(h.state().brush.options.brush, engine::geometry::Brush::Pattern(_)));
    h.get_by_label("Round").click();
    h.run_steps(2);
    let before = h.state().session.active().unwrap().doc.clone();
    event(&mut h, (20., 80.), 0, 0.2);
    event(&mut h, (80., 80.), 1, 1.);
    event(&mut h, (160., 80.), 1, 0.4);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    let p = h.state().brush.preview().unwrap();
    let profile = p.stroke.profile.as_ref().unwrap();
    assert!((profile.points[0].left - 0.8).abs() < 1e-6);
    assert_eq!(profile.points[1].left, 4.);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    assert!(photocraft_compose::flatten(&p.document).px.iter().any(|p| p[3] > 0.9));
    let expected = p.document.clone();
    event(&mut h, (160., 80.), 2, 0.4);
    assert_eq!(h.state().session.active().unwrap().doc, expected);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn brush17_kittest_types_width_and_cancels_escape_focus_and_preset_switch() {
    let mut h = harness();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Brush width").click();
    h.run_steps(2);
    h.event(egui::Event::Text("12".into()));
    h.event(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().brush.options.width, 12.);
    event(&mut h, (20., 80.), 0, 1.);
    event(&mut h, (80., 80.), 1, 1.);
    h.event(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert!(!h.state().brush.active());
    assert!(h.state().brush.preview().is_none());
    event(&mut h, (80., 80.), 2, 1.);
    event(&mut h, (20., 80.), 0, 1.);
    event(&mut h, (80., 80.), 1, 1.);
    h.event(egui::Event::WindowFocused(false));
    h.run_steps(1);
    assert!(!h.state().brush.active());
    h.event(egui::Event::WindowFocused(true));
    h.run_steps(1);
    event(&mut h, (80., 80.), 2, 1.);
    // Automation can own a captured tool gesture while the physical mouse clicks
    // context controls; do not pretend a held mouse can click a second widget.
    {
        let host = h.state_mut();
        host.brush.pointer(&mut host.session, ToolEvent::Down { x: 20., y: 80., pressure: 1. }).unwrap();
        host.brush.pointer(&mut host.session, ToolEvent::Move { x: 80., y: 80., pressure: 1. }).unwrap();
    }
    h.get_by_label("Scatter").click();
    h.run_steps(2);
    assert!(!h.state().brush.active());
    assert!(h.state().session.active().unwrap().doc.layers.is_empty());
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn brush17_kittest_stale_revision_tool_switch_and_old_settings_cancel() {
    let mut h = harness();
    event(&mut h, (20., 80.), 0, 1.);
    event(&mut h, (80., 80.), 1, 1.);
    h.state_mut()
        .session
        .edit("Change document", |doc, _| {
            doc.name = "changed".into();
            Ok(())
        })
        .unwrap();
    assert!(h.state().brush.display_doc(&h.state().session).is_none());
    h.run_steps(1);
    assert!(!h.state().brush.active());
    event(&mut h, (80., 80.), 2, 1.);
    assert!(h.state().session.active().unwrap().doc.layers.is_empty());
    let old: State = serde_json::from_str("{}").unwrap();
    assert_eq!(old.options, Options::default());
    event(&mut h, (20., 80.), 0, 1.);
    event(&mut h, (80., 80.), 1, 1.);
    let ctx = h.ctx.clone();
    let host = h.state_mut();
    host.brush.frame(&host.session, &ctx, false);
    assert!(!host.brush.active());
    assert!(host.brush.preview().is_none());
}
