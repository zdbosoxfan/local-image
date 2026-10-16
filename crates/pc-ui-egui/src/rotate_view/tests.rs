use super::*;
use crate::theme::ThemeKind;
use egui::{Event, Rect, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};

/// The shared CPU kittest renderer (`li-testkit`): no GPU or window.
fn cpu_renderer() -> li_testkit::SoftpaintRenderer {
    li_testkit::SoftpaintRenderer::with_clear(egui::Color32::BLACK)
}
type H = Harness<'static, PhotocraftApp>;
fn session() -> photocraft_engine::Session {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width":128,"height":96})).unwrap();
    s.execute("edit.fill", json!({"color":"#ff0000"})).unwrap();
    for (x, y, w, h, color) in [(48, 0, 80, 96, "#00ff00"), (0, 40, 48, 56, "#0000ff"), (48, 40, 80, 56, "#ffff00")] {
        s.execute("select.rect", json!({"x":x,"y":y,"width":w,"height":h})).unwrap();
        s.execute("edit.fill", json!({"color":color})).unwrap();
    }
    s.execute("select.deselect", json!({})).unwrap();
    s.edit_prefs(|p| {
        p.tools.overscroll = true;
        p.interface.contextual_task_bar = false;
        p.interface.canvas_border = photocraft_engine::prefs::CanvasBorder::None;
    });
    let d = s.active_mut().unwrap();
    d.saved_revision = d.revision;
    s
}
fn harness(theme: ThemeKind, size: Vec2, ppp: f32) -> H {
    let mut h = Harness::builder().with_size(size).with_pixels_per_point(ppp).with_max_steps(64).renderer(cpu_renderer()).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, theme);
        let mut app = PhotocraftApp::new(session(), Default::default());
        app.ui.theme = theme;
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.show.layer_edges = false;
        app.ui.view.show.pixel_grid = false;
        app
    });
    h.run_steps(6);
    h.state_mut().ui.views[0].zoom = 2.0;
    h.state_mut().ui.views[0].center = [64.0, 48.0];
    h.state_mut().ui.views[0].fit_pending = false;
    h.run_steps(2);
    h
}
fn key(h: &mut H, key: Key) {
    h.key_press(key);
    h.run_steps(3);
}
fn pointer(h: &mut H, p: Pos2, button: PointerButton, pressed: bool) {
    h.hover_at(p);
    h.step();
    h.event(Event::PointerButton { pos: p, button, pressed, modifiers: Modifiers::NONE });
    h.step();
}
fn drag(h: &mut H, from: Pos2, to: Pos2) {
    pointer(h, from, PointerButton::Primary, true);
    h.hover_at(to);
    h.step();
    pointer(h, to, PointerButton::Primary, false);
    h.run_steps(2);
}
fn angle(h: &H) -> f32 {
    h.state().ui.views[h.state().session.active_index().unwrap()].rotation.to_degrees()
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.01, "{a} != {b}");
}
fn typed_angle(h: &mut H, text: &str) {
    h.get_by_role_and_label(Role::SpinButton, "View angle").scroll_to_me();
    h.run_steps(5);
    h.get_by_role_and_label(Role::SpinButton, "View angle").click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(Event::Text(text.into()));
    h.key_press(Key::Enter);
    h.run_steps(3);
}
fn png(app: &PhotocraftApp) -> Vec<u8> {
    photocraft_io::export(&app.session.active().unwrap().doc, "png", &Default::default()).unwrap().bytes
}
fn artifact(h: &mut H, name: &str) {
    h.event(Event::PointerGone);
    h.step();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/rotate-view-activation");
    std::fs::create_dir_all(&dir).unwrap();
    h.render().unwrap().save(dir.join(name)).unwrap();
}
fn menu_rotate(h: &mut H) {
    h.get_by_label("View").click();
    h.run_steps(3);
    h.get_by_label("Rotate View").scroll_to_me();
    let mut previous = None;
    let mut visible = false;
    for _ in 0..32 {
        h.step();
        let r = h.get_by_label("Rotate View").rect();
        if crate::menu_nav::Nav::current(&h.ctx).views[0].contains_rect(r) && previous == Some(r) {
            visible = true;
            break;
        }
        previous = Some(r);
    }
    assert!(visible, "actual menu row must be reachable");
    h.get_by_label("Rotate View").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, Tool::RotateView);
}

#[test]
fn rotate_view_real_drag_typed_angle_reset_and_export_are_view_only() {
    let mut h = harness(ThemeKind::ProMedium, vec2(1200.0, 850.0), 1.25);
    let before = h.state().session.active().unwrap().doc.clone();
    let revision = h.state().session.active().unwrap().revision;
    let history = h.state().session.active().unwrap().history.past_len();
    let exported = png(h.state());
    menu_rotate(&mut h);
    let center = ViewXform::active(h.state()).unwrap().rect.center();
    pointer(&mut h, center + vec2(80.0, 0.0), PointerButton::Primary, true);
    h.hover_at(center + vec2(0.0, 80.0));
    h.run_steps(2);
    near(angle(&h), 90.0);
    assert!(h.state().ui.views[0].rotation_drag.is_some());
    assert!(h.state().drag.is_none(), "view drag never enters a document tool");
    key(&mut h, Key::Escape);
    near(angle(&h), 0.0);
    assert!(h.state().ui.views[0].rotation_drag.is_none());
    h.hover_at(center - vec2(80.0, 0.0));
    h.step();
    near(angle(&h), 0.0);
    assert!(h.state().ui.views[0].rotation_drag.is_none(), "cancelled held press cannot restart at the drag threshold");
    pointer(&mut h, center + vec2(0.0, 80.0), PointerButton::Primary, false);
    drag(&mut h, center + vec2(80.0, 0.0), center + vec2(0.0, 80.0));
    near(angle(&h), 90.0);
    let (sin, cos) = 23.0_f32.to_radians().sin_cos();
    let start = center + vec2(80.0, 0.0);
    let end = center + vec2(80.0 * cos, 80.0 * sin);
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    h.hover_at(start);
    h.step();
    h.event(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::SHIFT });
    h.step();
    h.hover_at(end);
    h.step();
    h.event(Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::SHIFT });
    h.step();
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.step();
    near(angle(&h), 113.0); // Shift does not introduce an unrequested snapping increment.
    typed_angle(&mut h, "37");
    near(angle(&h), 37.0);
    assert!(set_angle(h.state_mut(), f64::NAN).is_err());
    near(angle(&h), 37.0);
    artifact(&mut h, "cpu-view-37deg-125.png");
    assert_eq!(png(h.state()), exported, "actual PNG encoding ignores camera rotation");
    h.get_by_label("Reset View").scroll_to_me();
    h.run_steps(5);
    h.get_by_label("Reset View").click();
    h.run_steps(3);
    near(angle(&h), 0.0);
    let current = h.state().session.active().unwrap();
    assert!(std::sync::Arc::ptr_eq(&before, &current.doc));
    assert_eq!(current.revision, revision);
    assert_eq!(current.history.past_len(), history);
    assert!(!current.is_dirty());
    assert_eq!(png(h.state()), exported);
}

#[test]
fn rotate_view_default_r_saved_overrides_and_vector_object_rotate_are_distinct() {
    let mut h = harness(ThemeKind::ProMedium, vec2(1200.0, 850.0), 1.0);
    key(&mut h, Key::R);
    assert_eq!(h.state().ui.tool, Tool::RotateView);
    h.state_mut().session.edit_prefs(|p| {
        p.set_shortcut("tools.select.AiRemove", Some("R"));
    });
    key(&mut h, Key::V);
    key(&mut h, Key::R);
    assert_eq!(h.state().ui.tool, Tool::AiRemove);
    assert_eq!(h.state().session.prefs().shortcut_overrides(false)["tools.select.AiRemove"], "R");
    // AI Remove is a J healing-group member (Photoshop), but its own saved key still shows and wins.
    assert_eq!(crate::shortcuts::tool_shortcut_id(Tool::AiRemove), "tools.select.SpotHealing");
    assert_eq!(crate::shortcuts::tool_shortcut(&h.state().session, Tool::AiRemove), Some("R".into()));
    h.state_mut().session.edit_prefs(|p| {
        p.set_shortcut("tools.select.AiRemove", None);
        p.set_shortcut("tools.select.RotateView", Some(""));
    });
    assert_eq!(crate::shortcuts::tool_shortcut(&h.state().session, Tool::AiRemove), Some("J".into()));
    key(&mut h, Key::V);
    key(&mut h, Key::R);
    assert_eq!(h.state().ui.tool, Tool::Move, "explicit disable remains authoritative");
    h.state_mut().session.edit_prefs(|p| {
        p.set_shortcut("tools.select.RotateView", None);
    });
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    key(&mut h, Key::R);
    assert_eq!(h.state().ui.tool, Tool::Rotate, "Vector R retains object rotation");
    near(angle(&h), 0.0);
    h.state_mut().session.edit_prefs(|p| {
        p.set_shortcut("tools.select.RotateView", Some("F13"));
    });
    key(&mut h, Key::F13);
    assert_eq!(h.state().ui.tool, Tool::RotateView);
    near(angle(&h), 0.0);
    typed_angle(&mut h, "37");
    near(angle(&h), 37.0);
    key(&mut h, Key::R);
    assert_eq!(h.state().ui.tool, Tool::Rotate);
    near(angle(&h), 37.0);
}

#[test]
fn rotate_view_capture_cannot_cross_documents_and_match_rotation_is_truthful() {
    let mut h = harness(ThemeKind::ProMedium, vec2(1200.0, 850.0), 1.0);
    let first = h.state().session.active().unwrap().doc.id;
    set_angle(h.state_mut(), 25.0).unwrap();
    h.state_mut().run("file.new", json!({"width":64,"height":48})).unwrap();
    h.run_steps(3);
    let second = h.state().session.active().unwrap().doc.id;
    set_angle(h.state_mut(), -37.0).unwrap();
    h.state_mut().session.set_active(0);
    h.run_steps(3);
    key(&mut h, Key::R);
    let center = ViewXform::active(h.state()).unwrap().rect.center();
    pointer(&mut h, center + vec2(70.0, 0.0), PointerButton::Primary, true);
    assert!(h.state().ui.views[0].rotation_drag.is_some());
    h.state_mut().session.set_active(1);
    h.hover_at(center + vec2(0.0, 70.0));
    h.run_steps(2);
    near(angle(&h), -37.0);
    assert!(h.state().ui.views[0].rotation_drag.is_none());
    pointer(&mut h, center + vec2(0.0, 70.0), PointerButton::Primary, false);
    h.state_mut().run("document.move", json!({"document":0,"to":1})).unwrap();
    h.run_steps(2);
    let by_id = |h: &H, id| h.state().session.documents().iter().position(|d| d.doc.id == id).unwrap();
    near(h.state().ui.views[by_id(&h, first)].rotation.to_degrees(), 25.0);
    near(h.state().ui.views[by_id(&h, second)].rotation.to_degrees(), -37.0);
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "window.arrange.newWindowForDocument", json!({})).unwrap();
    h.state_mut().ui.windows[0].view.rotation = 80.0_f32.to_radians();
    assert!(h.state().ui.windows[0].view.rotation_drag.is_none());
    set_angle(h.state_mut(), 30.0).unwrap();
    near(h.state().ui.windows[0].view.rotation.to_degrees(), 80.0);
    let matched = crate::menus::invoke(h.state_mut(), &ctx, "window.arrange.matchRotation", json!({})).unwrap();
    near(matched["rotation"].as_f64().unwrap() as f32, 30.0);
    for v in &h.state().ui.views {
        near(v.rotation.to_degrees(), 30.0);
    }
    near(h.state().ui.windows[0].view.rotation.to_degrees(), 30.0);
    let legacy: View = serde_json::from_value(json!({"zoom":1.0,"center":[0.0,0.0],"fit_pending":false,"doc_size":[128,96]})).unwrap();
    assert_eq!(legacy.rotation, 0.0);
    assert!(legacy.rotation_drag.is_none());
    assert!(serde_json::to_value(&h.state().ui.views[0]).unwrap().get("rotation_drag").is_none());
}

#[test]
fn rotated_pan_zoom_fit_and_modal_keep_their_input_ownership() {
    let mut h = harness(ThemeKind::ProMedium, vec2(1200.0, 850.0), 1.25);
    key(&mut h, Key::R);
    set_angle(h.state_mut(), 37.0).unwrap();
    h.run_steps(2);
    let xf = ViewXform::active(h.state()).unwrap();
    let point = xf.to_screen(35.0, 20.0);
    h.hover_at(point);
    h.event(Event::Zoom(1.2));
    h.run_steps(2);
    let after = ViewXform::active(h.state()).unwrap().to_doc(point);
    assert!((after[0] - 35.0).abs() < 0.01 && (after[1] - 20.0).abs() < 0.01, "zoom anchor drifted: {after:?}");
    pointer(&mut h, point, PointerButton::Middle, true);
    h.hover_at(point + vec2(24.0, -15.0));
    h.step();
    pointer(&mut h, point + vec2(24.0, -15.0), PointerButton::Middle, false);
    let moved = ViewXform::active(h.state()).unwrap().to_screen(35.0, 20.0);
    assert!(moved.distance(point + vec2(24.0, -15.0)) < 0.02, "pan used document rather than screen axes");
    near(angle(&h), 37.0);
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "view.fitOnScreen", json!({})).unwrap();
    h.run_steps(2);
    near(angle(&h), 37.0);
    let xf = ViewXform::active(h.state()).unwrap();
    assert!(xf.rect.contains_rect(xf.doc_rect(h.state().session.active().unwrap().doc.bounds())));
    h.state_mut().ui.views[0].zoom = 8.0;
    h.state_mut().session.edit_prefs(|p| p.tools.overscroll = false);
    h.run_steps(3);
    let xf = ViewXform::active(h.state()).unwrap();
    let old = h.state().ui.views[0].center;
    let (bar, _) = crate::scrollbars::bar_rects(xf.rect, true, true);
    let bar = bar.unwrap();
    pointer(&mut h, egui::pos2(bar.right() - 5.0, bar.center().y), PointerButton::Primary, true);
    pointer(&mut h, egui::pos2(bar.right() - 5.0, bar.center().y), PointerButton::Primary, false);
    h.run_steps(2);
    assert_ne!(h.state().ui.views[0].center, old);
    near(angle(&h), 37.0);
    let before = h.state().ui.views[0].clone();
    crate::restoration_ui::open(h.state_mut());
    drag(&mut h, xf.rect.center() + vec2(70.0, 0.0), xf.rect.center() + vec2(0.0, 70.0));
    assert_eq!(h.state().ui.views[0], before, "first-frame modal blocks rotation/navigation");
    key(&mut h, Key::Escape);
    assert!(!crate::restoration_ui::is_open(h.state()));
    near(angle(&h), 37.0);
}

#[test]
fn rotated_canvas_pointer_paints_moves_nodes_and_hits_transform_and_crop_handles() {
    let mut h = harness(ThemeKind::ProMedium, vec2(1200.0, 850.0), 1.25);
    set_angle(h.state_mut(), 37.0).unwrap();
    h.run_steps(2);
    let quarter = ViewXform { rotation: std::f32::consts::FRAC_PI_2, ..ViewXform::active(h.state()).unwrap() };
    assert_eq!(resize_cursor(egui::CursorIcon::ResizeHorizontal, &quarter), egui::CursorIcon::ResizeVertical);
    assert_eq!(resize_cursor(egui::CursorIcon::ResizeNwSe, &quarter), egui::CursorIcon::ResizeNeSw);
    h.state_mut().run("tools.setBrush", json!({"brush":{"size":3,"hardness":1.0}})).unwrap();
    key(&mut h, Key::B);
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    let p = ViewXform::active(h.state()).unwrap().to_screen(25.0, 20.0);
    pointer(&mut h, p, PointerButton::Primary, true);
    pointer(&mut h, p, PointerButton::Primary, false);
    h.run_steps(2);
    let pixels = photocraft_engine::ai_cmds::flatten_rgba(&h.state().session.active().unwrap().doc).unwrap();
    assert!(pixels.get_pixel(25, 20).0[..3].iter().all(|c| *c < 20));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.state_mut().run("path.set", json!({"name":"work","path":{"subpaths":[{"closed":true,"knots":[[20,20],[80,20],[80,70],[20,70]]}]}})).unwrap();
    key(&mut h, Key::A);
    h.key_press_modifiers(Modifiers::SHIFT, Key::A);
    h.run_steps(2);
    assert_eq!(h.state().ui.tool, Tool::DirectSelection);
    let path_before = h.state().session.active().unwrap().doc.clone();
    let xf = ViewXform::active(h.state()).unwrap();
    drag(&mut h, xf.to_screen(20.0, 20.0), xf.to_screen(30.0, 25.0));
    let moved = h.state().session.active().unwrap().doc.work_path.as_ref().unwrap().subpaths[0].knots[0].anchor;
    assert!((moved.x - 30.0).abs() < 0.01 && (moved.y - 25.0).abs() < 0.01, "{moved:?}");
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, path_before);
    key(&mut h, Key::C);
    let xf = ViewXform::active(h.state()).unwrap();
    drag(&mut h, xf.to_screen(0.0, 0.0), xf.to_screen(10.0, 10.0));
    let crop = h.state().ui.crop_rect.unwrap();
    assert!((crop[0] - 10.0).abs() < 0.01 && (crop[1] - 10.0).abs() < 0.01, "{crop:?}");
    artifact(&mut h, "cpu-rotated-crop-handles.png");
    key(&mut h, Key::Escape);
    near(angle(&h), 37.0);
    h.state_mut().run("layer.new.layer", json!({"name":"Target"})).unwrap();
    h.state_mut().run("edit.fill", json!({"color":"#ff00ff"})).unwrap();
    key(&mut h, Key::V);
    let before_move = h.state().session.active().unwrap().doc.clone();
    let xf = ViewXform::active(h.state()).unwrap();
    drag(&mut h, xf.to_screen(30.0, 30.0), xf.to_screen(40.0, 35.0));
    let st = h.state().session.active().unwrap();
    let layer = st.doc.layer(st.active_layer.unwrap()).unwrap();
    let bounds = photocraft_engine::transform_cmds::transform_bounds(&st.doc, layer);
    assert_eq!((bounds.x0, bounds.y0), (10, 5));
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, before_move);
    h.state_mut().ui.tool_options.move_show_transform = true;
    h.run_steps(2);
    let xf = ViewXform::active(h.state()).unwrap();
    pointer(&mut h, xf.to_screen(128.0, 96.0), PointerButton::Primary, true);
    assert!(h.state().ui.transform.is_some(), "rotated visible handle enters Free Transform");
    pointer(&mut h, xf.to_screen(128.0, 96.0), PointerButton::Primary, false);
    key(&mut h, Key::Escape);
    assert_eq!(h.state().session.active().unwrap().doc, before_move);
    near(angle(&h), 37.0);
}

#[test]
fn rotate_view_mask_retention_and_compact_controls_remain_reachable() {
    for theme in [ThemeKind::ProMedium, ThemeKind::StudioLight] {
        let mut h = harness(theme, vec2(760.0, 480.0), 1.25);
        key(&mut h, Key::R);
        for label in ["View angle", "Reset View"] {
            let node = if label == "View angle" { h.get_by_role_and_label(Role::SpinButton, label) } else { h.get_by_role_and_label(Role::Button, label) };
            node.scroll_to_me();
            h.run_steps(5);
            let r = if label == "View angle" {
                h.get_by_role_and_label(Role::SpinButton, label).rect()
            } else {
                h.get_by_role_and_label(Role::Button, label).rect()
            };
            let clip: Rect = h.ctx.data(|d| d.get_temp(egui::Id::new(("theme-options-clip", Tool::RotateView)))).unwrap();
            assert!(clip.contains_rect(r) && h.ctx.content_rect().contains_rect(r), "{theme:?}/{label}: {r:?} outside {clip:?}");
        }
        typed_angle(&mut h, "37");
        near(angle(&h), 37.0);
        let before = h.state().session.active().unwrap().doc.clone();
        let exported = png(h.state());
        let mask = image::GrayImage::from_fn(128, 96, |x, y| image::Luma([if (88..93).contains(&x) && (18..23).contains(&y) { 255 } else { 0 }]));
        let expected = mask.as_raw().clone();
        crate::ai_remove_ui::detected(h.state_mut(), mask, false).unwrap();
        h.run_steps(2);
        key(&mut h, Key::R);
        assert_eq!(h.state().ui.tool, Tool::RotateView);
        assert_eq!(crate::ai_remove_ui::mask_for_test(h.state()).unwrap(), expected);
        let center = ViewXform::active(h.state()).unwrap().rect.center();
        pointer(&mut h, center - vec2(0.0, 60.0), PointerButton::Primary, true);
        h.hover_at(center - vec2(60.0, 0.0));
        h.step();
        assert!(h.state().ui.views[0].rotation_drag.is_some());
        key(&mut h, Key::Escape);
        near(angle(&h), 37.0);
        assert_eq!(crate::ai_remove_ui::mask_for_test(h.state()).unwrap(), expected, "Rotate Escape cannot cancel the Remove mask");
        pointer(&mut h, center - vec2(60.0, 0.0), PointerButton::Primary, false);
        artifact(&mut h, &format!("cpu-compact-mask-{theme:?}.png"));
        h.get_by_label("Reset View").scroll_to_me();
        h.run_steps(5);
        h.get_by_label("Reset View").click();
        h.run_steps(2);
        assert_eq!(crate::ai_remove_ui::mask_for_test(h.state()).unwrap(), expected);
        assert!(std::sync::Arc::ptr_eq(&before, &h.state().session.active().unwrap().doc));
        assert_eq!(png(h.state()), exported);
    }
}

#[test]
fn rotate_view_owns_r_and_ai_remove_cycles_in_the_j_healing_group() {
    let mut h = harness(ThemeKind::ProMedium, vec2(1200.0, 850.0), 1.0);
    key(&mut h, Key::R);
    assert_eq!(h.state().ui.tool, Tool::RotateView);
    key(&mut h, Key::J);
    assert_eq!(h.state().ui.tool, Tool::SpotHealing);
    let mut seen = vec![h.state().ui.tool];
    for _ in 0..6 {
        h.key_press_modifiers(Modifiers::SHIFT, Key::J);
        h.run_steps(3);
        seen.push(h.state().ui.tool);
    }
    assert!(seen.contains(&Tool::AiRemove), "Shift+J reaches AI Remove: {seen:?}");
    assert!(seen.iter().all(|t| t.key() == 'J'), "{seen:?}");
    assert_eq!(crate::shortcuts::tool_shortcut(&h.state().session, Tool::AiRemove), Some("J".into()));
    assert_eq!(crate::shortcuts::tool_shortcut(&h.state().session, Tool::RotateView), Some("R".into()));
}

#[test]
fn rotate_view_extra_window_controls_and_text_focus_are_local() {
    let main = View { rotation: 0.5, ..Default::default() };
    let extra = View { rotation: 1.0, ..Default::default() };
    let mut controls = Harness::builder().with_size(vec2(420.0, 100.0)).build_ui_state(
        |ui, state: &mut (View, View)| {
            ui.horizontal(|ui| options_for_view(&mut state.1, ui));
        },
        (main, extra),
    );
    controls.run_steps(3);
    controls.get_by_label("Reset View").click();
    controls.run_steps(2);
    assert_eq!(controls.state().0.rotation, 0.5);
    assert_eq!(controls.state().1.rotation, 0.0);
    let mut h = harness(ThemeKind::ProMedium, vec2(1200.0, 850.0), 1.0);
    // A normal layer: like Photoshop, the Background layer's Opacity/Fill fields are disabled.
    h.state_mut().run("layer.new.layer", json!({})).unwrap();
    h.run_steps(2);
    key(&mut h, Key::B);
    let layers = crate::dock::last_rects(&h.ctx).into_iter().find(|(g, _)| *g == crate::dock::Group::Layers).unwrap().1;
    let field =
        h.query_all_by_role(Role::SpinButton).map(|n| n.rect()).filter(|r| layers.contains(r.center())).min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
    pointer(&mut h, field.center(), PointerButton::Primary, true);
    pointer(&mut h, field.center(), PointerButton::Primary, false);
    assert_eq!(crate::shortcut_dispatch::Focus::of(&h.ctx), crate::shortcut_dispatch::Focus::Text);
    key(&mut h, Key::R);
    assert_eq!(h.state().ui.tool, Tool::Brush);
    near(angle(&h), 0.0);
}

#[test]
fn rotated_cpu_fallback_shadow_preserves_image_and_clear_pasteboard() {
    let mut h = harness(ThemeKind::StudioLight, vec2(1200.0, 850.0), 1.25);
    key(&mut h, Key::R);
    set_angle(h.state_mut(), 37.0).unwrap();
    h.event(Event::PointerGone);
    h.run_steps(2);
    let before = h.render().unwrap();
    h.state_mut().session.edit_prefs(|p| p.interface.canvas_border = photocraft_engine::prefs::CanvasBorder::DropShadow);
    h.run_steps(2);
    let after = h.render().unwrap();
    let xf = ViewXform::active(h.state()).unwrap();
    let pixel = |image: &image::RgbaImage, p: Pos2| image.get_pixel((p.x * 1.25) as u32, (p.y * 1.25) as u32).0;
    assert_eq!(pixel(&before, xf.to_screen(20.0, 20.0)), pixel(&after, xf.to_screen(20.0, 20.0)));
    assert_eq!(pixel(&before, xf.rect.min + vec2(15.0, 15.0)), pixel(&after, xf.rect.min + vec2(15.0, 15.0)));
    let outside = xf.to_screen(132.0, 48.0);
    let a = pixel(&before, outside);
    let b = pixel(&after, outside);
    assert!(
        a[..3].iter().map(|v| u32::from(*v)).sum::<u32>() > b[..3].iter().map(|v| u32::from(*v)).sum::<u32>() + 6,
        "rotated true edge needs shadow even inside its AABB: {a:?} -> {b:?}"
    );
    artifact(&mut h, "cpu-shadow-37deg-125.png");
}

#[test]
fn rotated_scrollbar_owns_vector_press_and_absent_right_bar_does_not_block_drawing() {
    let mut h = harness(ThemeKind::ProMedium, vec2(1200.0, 850.0), 1.25);
    h.state_mut().run("file.new", json!({"width":100,"height":1000})).unwrap();
    let index = h.state().session.active_index().unwrap();
    h.state_mut().ui.views[index].zoom = 1.0;
    h.state_mut().ui.views[index].center = [50.0, 500.0];
    h.state_mut().ui.views[index].rotation = std::f32::consts::FRAC_PI_2;
    h.state_mut().ui.views[index].fit_pending = false;
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    key(&mut h, Key::B);
    assert_eq!(h.state().ui.tool, Tool::VectorBrush);
    let xf = ViewXform::active(h.state()).unwrap();
    let (horizontal, vertical) = crate::scrollbars::visible_bar_rects(xf.rect, &h.state().ui.views[index], xf.flip);
    let horizontal = horizontal.expect("quarter-turn creates horizontal scrollbar");
    assert!(vertical.is_none(), "the upright vertical scrollbar must disappear");
    let before = h.state().session.active().unwrap().doc.clone();
    let revision = h.state().session.active().unwrap().revision;
    let history = h.state().session.active().unwrap().history.past_len();
    let center = h.state().ui.views[index].center;
    let track = egui::pos2(horizontal.right() - 4.0, horizontal.center().y);
    pointer(&mut h, track, PointerButton::Primary, true);
    assert!(!h.state().vector_brush.active(), "scrollbar press cannot begin a vector stroke");
    pointer(&mut h, track, PointerButton::Primary, false);
    h.run_steps(2);
    assert_ne!(h.state().ui.views[index].center, center, "actual scrollbar page click pans the view");
    let st = h.state().session.active().unwrap();
    assert!(std::sync::Arc::ptr_eq(&before, &st.doc));
    assert_eq!(st.revision, revision);
    assert_eq!(st.history.past_len(), history);
    let xf = ViewXform::active(h.state()).unwrap();
    let right = egui::pos2(xf.rect.right() - 5.0, xf.rect.center().y);
    pointer(&mut h, right, PointerButton::Primary, true);
    assert!(h.state().vector_brush.active(), "absent right scrollbar cannot reject a real canvas press");
    h.hover_at(right + vec2(-25.0, 8.0));
    h.step();
    pointer(&mut h, right + vec2(-25.0, 8.0), PointerButton::Primary, false);
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
