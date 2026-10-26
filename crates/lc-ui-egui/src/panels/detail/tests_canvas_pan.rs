use std::time::Duration;

use egui::{Event, PointerButton, Pos2, vec2};
use serde_json::json;

use crate::headless::Headless;
use crate::state::{RightPanel, ViewMode, Zoom};
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(40);

fn setup(panel: RightPanel, tool: &str, zoom: Zoom) -> Headless {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.view = ViewMode::Detail;
    app.ui.right = panel;
    app.ui.tool = tool.into();
    app.ui.zoom = zoom;
    app.ui.filmstrip = false;
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    assert!(h.settle(T));
    h
}

fn drag(h: &mut Headless, button: PointerButton, from: Pos2, to: Pos2) -> egui::CursorIcon {
    let modifiers = egui::Modifiers::NONE;
    h.app.synthetic.push(Event::PointerMoved(from));
    h.app.synthetic.push(Event::PointerButton { pos: from, button, pressed: true, modifiers });
    h.step();
    for i in 1..=6 {
        h.app.synthetic.push(Event::PointerMoved(from + (to - from) * (i as f32 / 6.0)));
        h.step();
    }
    let cursor = h.last_cursor;
    h.app.synthetic.push(Event::PointerButton { pos: to, button, pressed: false, modifiers });
    h.step();
    h.step();
    cursor
}

#[test]
fn a_middle_drag_pans_with_clone_and_primary_still_paints() {
    let mut h = setup(RightPanel::Remove, "clone", Zoom::Percent(100));
    let at = h.app.canvas_rect.unwrap().center();
    let pan = h.app.ui.pan;
    let cursor = drag(&mut h, PointerButton::Middle, at, at + vec2(-150.0, -100.0));
    assert_eq!(cursor, egui::CursorIcon::Grabbing, "the hand shows while panning");
    assert_ne!(h.app.ui.pan, pan, "the middle drag pans");
    let id = h.app.session.active().unwrap();
    assert!(h.app.session.develop_of(id).unwrap().spots.is_empty());
    let pan = h.app.ui.pan;
    let at = h.app.canvas_rect.unwrap().center();
    drag(&mut h, PointerButton::Primary, at, at + vec2(40.0, 0.0));
    assert_eq!(h.app.session.develop_of(id).unwrap().spots.len(), 1, "a left drag paints a spot");
    assert_eq!(h.app.ui.pan, pan);
}

#[test]
fn middle_pan_preserves_every_tool_at_zoom_and_fit() {
    for zoom in [Zoom::Percent(100), Zoom::Fit] {
        for (panel, tool) in [
            (RightPanel::Remove, "remove"),
            (RightPanel::Remove, "heal"),
            (RightPanel::Remove, "clone"),
            (RightPanel::Masking, "brush"),
            (RightPanel::Masking, "linear"),
            (RightPanel::Masking, "radial"),
            (RightPanel::Crop, ""),
            (RightPanel::Crop, "guidedUpright"),
            (RightPanel::RedEye, ""),
        ] {
            let mut h = setup(panel, tool, zoom);
            if panel == RightPanel::Masking {
                h.app.run("mask.add", json!({"kind":"radial", "center":[0.3,0.4], "rx":0.1,"ry":0.1})).unwrap();
            }
            if tool == "guidedUpright" {
                h.app.run("geometry.guides", json!({"guides":[[0.2,0.3,0.8,0.3]]})).unwrap();
            }
            assert!(h.settle(T));
            let id = h.app.session.active().unwrap();
            let before = h.app.session.develop_of(id).unwrap();
            let undo = h.app.session.undo.len();
            let pan = h.app.ui.pan;
            let at = h.app.canvas_rect.unwrap().center();
            let cursor = drag(&mut h, PointerButton::Middle, at, at + vec2(-90.0, -50.0));
            assert_eq!(cursor, egui::CursorIcon::Grabbing, "{panel:?}/{tool}/{zoom:?}");
            if zoom == Zoom::Fit {
                assert_eq!(h.app.ui.pan, pan, "Fit stays still");
            } else {
                assert_ne!(h.app.ui.pan, pan, "{panel:?}/{tool} pans");
            }
            assert_eq!(h.app.ui.right, panel);
            assert_eq!(h.app.ui.tool, tool);
            assert_eq!(*h.app.session.develop_of(id).unwrap(), *before, "panning edits no tool data");
            assert_eq!(h.app.session.undo.len(), undo);
            assert!(h.app.gesture.is_none());
        }
    }
}

#[test]
fn right_drag_does_not_pan_and_right_click_keeps_the_menu() {
    let mut h = setup(RightPanel::None, "", Zoom::Percent(100));
    let pan = h.app.ui.pan;
    let at = h.app.canvas_rect.unwrap().center();
    drag(&mut h, PointerButton::Secondary, at, at + vec2(-90.0, -50.0));
    assert_eq!(h.app.ui.pan, pan, "only middle or primary drags pan");
    let r = h.request("ui.click", json!({"x":at.x,"y":at.y,"button":"right"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(egui::Popup::is_any_open(&h.view.ctx), "right click opens the photo menu");
}

#[test]
fn navigator_and_covering_ui_own_their_presses() {
    let mut h = setup(RightPanel::Crop, "guidedUpright", Zoom::Percent(100));
    let id = h.app.session.active().unwrap();
    let before = h.app.session.develop_of(id).unwrap();
    let nav = h.app.widgets.iter().find(|(name, _)| name == "canvas:navigator").unwrap().1;
    let pan = h.app.ui.pan;
    drag(&mut h, PointerButton::Primary, nav.center(), nav.center() + vec2(-20.0, 0.0));
    assert_ne!(h.app.ui.pan, pan, "Navigator still navigates");
    assert_eq!(*h.app.session.develop_of(id).unwrap(), *before);
    let pan = h.app.ui.pan;
    drag(&mut h, PointerButton::Middle, nav.center(), nav.center() + vec2(-90.0, -50.0));
    assert_eq!(h.app.ui.pan, pan, "middle drag from Navigator cannot reach the canvas");
    let at = h.app.canvas_rect.unwrap().center();
    let r = h.request("ui.click", json!({"x":at.x - 30.0,"y":at.y - 30.0,"button":"right"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(egui::Popup::is_any_open(&h.view.ctx));
    let pan = h.app.ui.pan;
    drag(&mut h, PointerButton::Middle, at, at + vec2(-90.0, -50.0));
    assert_eq!(h.app.ui.pan, pan, "menu press belongs to its layer");
    egui::Popup::close_all(&h.view.ctx);
    h.app.host_modal = true;
    h.step();
    drag(&mut h, PointerButton::Middle, at, at + vec2(-90.0, -50.0));
    assert_eq!(h.app.ui.pan, pan, "host modal blocks canvas input");
    assert_eq!(*h.app.session.develop_of(id).unwrap(), *before);
}

#[test]
fn middle_pan_keeps_a_new_gradient_ready_to_draw() {
    for tool in ["linear", "radial"] {
        let mut h = setup(RightPanel::Edit, "", Zoom::Percent(100));
        h.app.run(&format!("tool.{tool}"), json!({})).unwrap();
        assert!(h.settle(T));
        let id = h.app.session.active().unwrap();
        let before = h.app.session.develop_of(id).unwrap();
        let pending = h.app.ui.pending_mask_shape;
        assert!(pending.is_some(), "the new gradient is ready to draw");
        let undo = h.app.session.undo.len();
        let at = h.app.canvas_rect.unwrap().center();
        drag(&mut h, PointerButton::Middle, at, at + vec2(-90.0, -50.0));
        assert_eq!(*h.app.session.develop_of(id).unwrap(), *before, "middle release cannot reshape the new gradient");
        assert_eq!(h.app.ui.pending_mask_shape, pending);
        assert_eq!(h.app.ui.pending_mask_origin, None);
        assert_eq!(h.app.session.undo.len(), undo);
        drag(&mut h, PointerButton::Primary, at, at + vec2(90.0, 50.0));
        assert_ne!(*h.app.session.develop_of(id).unwrap(), *before, "the primary button still draws");
        assert_eq!(h.app.ui.pending_mask_shape, None);
    }
}
