use egui::{CursorIcon, Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use photocraft_doc::LayerId;
use photocraft_geom::Rect;
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::ViewXform;
use crate::state::Tool;

type H = Harness<'static, PhotocraftApp>;

fn harness() -> (H, LayerId, LayerId) {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 400, "height": 300, "background": "white"})).unwrap();
    let mut ids = Vec::new();
    for x in [80, 240] {
        app.run("layer.new.layer", json!({})).unwrap();
        app.session
            .edit("paint", |doc, a| {
                doc.layer_mut(a.unwrap()).unwrap().surface_mut().unwrap().fill_rect(Rect::new(x, 100, x + 40, 140), &[1.0, 0.0, 0.0, 1.0]);
                Ok(())
            })
            .unwrap();
        ids.push(app.session.active().unwrap().active_layer.unwrap());
    }
    app.sync_views();
    app.ui.views[0].zoom = 1.0;
    app.ui.views[0].center = [200.0, 150.0];
    app.ui.views[0].fit_pending = false;
    app.ui.extras.rulers = false;
    app.ui.extras.snap = false;
    app.ui.view.show.smart_guides = false;
    app.ui.tool = Tool::Move;
    let mut h = Harness::builder().with_size(vec2(900.0, 650.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, &ctx);
            let width = ctx.data(|d| d.get_temp::<f32>(egui::Id::new("test-panel-width"))).unwrap_or(150.0);
            egui::Panel::left("panel").exact_size(width).show(ui, |ui| {
                let _ = ui.button("Panel");
            });
            egui::CentralPanel::default().show(ui, |ui| {
                // Deliberately change the canvas widget's identity every frame.
                let id = if ctx.input(|i| i.key_down(Key::Space)) { 0 } else { ctx.cumulative_frame_nr() };
                ui.push_id(id, |ui| crate::canvas::document_area(app, ui));
            });
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, Default::default());
    h.run_steps(4);
    (h, ids[0], ids[1])
}

fn screen(h: &H, x: f32, y: f32) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    ViewXform {
        rect: crate::rulers::content_rect(app, app.last_canvas_rect),
        zoom: v.zoom,
        center: v.center,
        flip: app.ui.view.flip_horizontal,
        rotation: v.rotation,
    }
    .to_screen(x, y)
}

fn button(h: &mut H, p: Pos2, pressed: bool, mods: Modifiers) {
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: mods });
    h.step();
}

fn pointer(h: &mut H, p: Pos2, mods: Modifiers) {
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerMoved(p));
    h.step();
}

fn bounds(h: &H, id: LayerId) -> Rect {
    h.state().session.active().unwrap().doc.layer(id).unwrap().surface().unwrap().content_bounds()
}

fn preview_bounds(h: &mut H, id: LayerId) -> Rect {
    crate::move_ui::display_doc(h.state_mut(), 0).unwrap().0.layer(id).unwrap().surface().unwrap().content_bounds()
}

fn history(h: &H) -> usize {
    h.state().session.active().unwrap().history.past_len()
}

#[test]
fn move_input_first_pixel_is_live_and_release_is_one_undo_step() {
    let (mut h, a, b) = harness();
    let p = screen(&h, 100.0, 120.0);
    pointer(&mut h, p, Modifiers::NONE);
    let steps = history(&h);
    button(&mut h, p, true, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(a));
    pointer(&mut h, p + vec2(1.0, 0.0), Modifiers::NONE);
    assert_eq!(preview_bounds(&mut h, a), Rect::new(81, 100, 121, 140), "first pixel, button still held");
    assert_eq!(history(&h), steps);
    pointer(&mut h, p + vec2(25.0, 7.0), Modifiers::NONE);
    assert_eq!(preview_bounds(&mut h, a), Rect::new(105, 107, 145, 147));
    // Mouse-up at a newer point than the last motion event must use that newer point.
    button(&mut h, p + vec2(30.0, 9.0), false, Modifiers::NONE);
    assert_eq!(bounds(&h, a), Rect::new(110, 109, 150, 149));
    assert_eq!(bounds(&h, b), Rect::new(240, 100, 280, 140));
    assert_eq!(history(&h), steps + 1);
    h.state_mut().session.undo();
    assert_eq!(bounds(&h, a), Rect::new(80, 100, 120, 140));
}

#[test]
fn move_input_survives_panels_pointer_gone_reentry_relayout_and_outside_release() {
    let (mut h, a, _) = harness();
    let p = screen(&h, 100.0, 120.0);
    pointer(&mut h, p, Modifiers::NONE);
    button(&mut h, p, true, Modifiers::NONE);
    pointer(&mut h, pos2(60.0, p.y), Modifiers::NONE);
    assert!(super::captured(&h.ctx));
    h.event(Event::PointerGone);
    h.step();
    assert!(super::captured(&h.ctx));
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("test-panel-width"), 220.0f32));
    pointer(&mut h, p + vec2(20.0, 5.0), Modifiers::NONE);
    assert_eq!(crate::move_ui::drag_offset(h.state()), Some((20, 5)));
    assert_eq!(preview_bounds(&mut h, a), Rect::new(100, 105, 140, 145));
    button(&mut h, pos2(50.0, p.y), false, Modifiers::NONE);
    assert!(h.state().drag.is_none() && !super::captured(&h.ctx));
    let p = screen(&h, 100.0, 120.0);
    h.state_mut().ui.tool_options.move_auto_select = false;
    pointer(&mut h, p, Modifiers::NONE);
    button(&mut h, p, true, Modifiers::NONE);
    pointer(&mut h, p + vec2(1.0, 0.0), Modifiers::NONE);
    assert_eq!(crate::move_ui::drag_offset(h.state()), Some((1, 0)), "next gesture works");
    button(&mut h, p + vec2(1.0, 0.0), false, Modifiers::NONE);
}

#[test]
fn move_input_modifiers_change_constraint_without_dropping_capture() {
    let (mut h, a, _) = harness();
    let p = screen(&h, 100.0, 120.0);
    pointer(&mut h, p, Modifiers::NONE);
    button(&mut h, p, true, Modifiers::NONE);
    pointer(&mut h, p + vec2(20.0, 7.0), Modifiers::SHIFT);
    assert_eq!(preview_bounds(&mut h, a), Rect::new(100, 100, 140, 140));
    pointer(&mut h, p + vec2(20.0, 7.0), Modifiers::NONE);
    assert_eq!(preview_bounds(&mut h, a), Rect::new(100, 107, 140, 147));
    h.key_down(Key::Space);
    h.step();
    pointer(&mut h, p + vec2(25.0, 8.0), Modifiers::NONE);
    assert_eq!(crate::move_ui::drag_offset(h.state()), Some((25, 8)));
    button(&mut h, p + vec2(25.0, 8.0), false, Modifiers::NONE);
    h.key_up(Key::Space);
    h.step();
    assert_eq!(bounds(&h, a), Rect::new(105, 108, 145, 148));
}

#[test]
fn move_input_alt_duplicates_multiple_layers_live_once_and_undo_redo_together() {
    let (mut h, a, b) = harness();
    h.state_mut().session.execute("layer.select", json!({"layer": a.0, "mode": "add"})).unwrap();
    let steps = history(&h);
    let count = h.state().session.active().unwrap().doc.layers.len();
    let journal = h.state().session.journal.len();
    let p = screen(&h, 100.0, 120.0);
    pointer(&mut h, p, Modifiers::ALT);
    button(&mut h, p, true, Modifiers::ALT);
    pointer(&mut h, p + vec2(1.0, 0.0), Modifiers::ALT);
    let copies = crate::move_mods::copy_source(h.state()).unwrap().1;
    assert_eq!(history(&h), steps, "no history edits while held");
    assert_eq!(h.state().session.journal.len(), journal, "no journal edits while held");
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), count);
    assert_eq!(copies.len(), 2);
    assert!(!copies.contains(&a) && !copies.contains(&b));
    assert_eq!(preview_bounds(&mut h, copies[0]), Rect::new(81, 100, 121, 140));
    pointer(&mut h, p + vec2(20.0, 5.0), Modifiers::NONE);
    button(&mut h, p + vec2(20.0, 5.0), false, Modifiers::NONE);
    assert_eq!(history(&h), steps + 1);
    assert_eq!(h.state().session.journal.len(), journal + 1);
    let committed_copies = h.state().session.active().unwrap().selected_layers();
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), count + 2);
    assert_eq!(bounds(&h, a), Rect::new(80, 100, 120, 140));
    assert_eq!(bounds(&h, b), Rect::new(240, 100, 280, 140));
    h.state_mut().session.undo();
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), count);
    h.state_mut().session.redo();
    assert_eq!(bounds(&h, committed_copies[0]), Rect::new(100, 105, 140, 145));
}

#[test]
fn move_input_plain_multi_move_and_arrow_nudges() {
    let (mut h, a, b) = harness();
    h.state_mut().session.execute("layer.select", json!({"layer": a.0, "mode": "add"})).unwrap();
    let p = screen(&h, 100.0, 120.0);
    pointer(&mut h, p, Modifiers::NONE);
    button(&mut h, p, true, Modifiers::NONE);
    pointer(&mut h, p + vec2(10.0, 2.0), Modifiers::NONE);
    assert_eq!(preview_bounds(&mut h, b), Rect::new(250, 102, 290, 142));
    button(&mut h, p + vec2(10.0, 2.0), false, Modifiers::NONE);
    h.key_press(Key::ArrowRight);
    h.step();
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowUp);
    h.step();
    assert_eq!(bounds(&h, a), Rect::new(91, 92, 131, 132));
    assert_eq!(bounds(&h, b), Rect::new(251, 92, 291, 132));
}

#[test]
fn move_input_ctrl_click_toggles_auto_select_and_group_mode_picks_group() {
    let (mut h, a, b) = harness();
    let p = screen(&h, 100.0, 120.0);
    pointer(&mut h, p, Modifiers::CTRL);
    button(&mut h, p, true, Modifiers::CTRL);
    button(&mut h, p, false, Modifiers::CTRL);
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(b));
    h.state_mut().ui.tool_options.move_auto_select = false;
    button(&mut h, p, true, Modifiers::CTRL);
    button(&mut h, p, false, Modifiers::CTRL);
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(a));
    let group = h.state_mut().session.execute("layer.new.groupFromLayers", json!({"name": "G"})).unwrap()["layer"].as_u64().unwrap();
    h.state_mut().session.execute("layer.select", json!({"layer": b.0})).unwrap();
    h.state_mut().ui.tool_options.move_target = "group".into();
    h.state_mut().ui.tool_options.move_auto_select = true;
    button(&mut h, p, true, Modifiers::NONE);
    pointer(&mut h, p + vec2(10.0, 0.0), Modifiers::NONE);
    button(&mut h, p + vec2(10.0, 0.0), false, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(LayerId(group)));
    assert_eq!(bounds(&h, a), Rect::new(90, 100, 130, 140));
}

#[test]
fn move_input_selected_pixels_cut_copy_and_nudge_leave_unselected_pixels() {
    for copy in [false, true] {
        let (mut h, a, _) = harness();
        h.state_mut().session.execute("layer.select", json!({"layer": a.0})).unwrap();
        h.state_mut().session.execute("select.rect", json!({"x": 80, "y": 100, "width": 10, "height": 40})).unwrap();
        let steps = history(&h);
        let p = screen(&h, 85.0, 120.0);
        let mods = if copy { Modifiers::ALT } else { Modifiers::NONE };
        pointer(&mut h, p, mods);
        button(&mut h, p, true, mods);
        pointer(&mut h, p + vec2(80.0, 0.0), mods);
        let shown = crate::move_ui::display_doc(h.state_mut(), 0).unwrap().0;
        let s = shown.layer(a).unwrap().surface().unwrap();
        assert_eq!(s.rgba(85, 120)[3], if copy { 1.0 } else { 0.0 });
        assert_eq!(s.rgba(165, 120)[3], 1.0);
        assert_eq!(s.rgba(100, 120)[3], 1.0);
        button(&mut h, p + vec2(80.0, 0.0), false, mods);
        assert_eq!(history(&h), steps + 1);
        assert_eq!(h.state().session.active().unwrap().doc.selection.as_ref().unwrap().content_bounds(), Rect::new(160, 100, 170, 140));
        h.key_press(Key::ArrowRight);
        h.step();
        assert_eq!(h.state().session.active().unwrap().doc.selection.as_ref().unwrap().content_bounds(), Rect::new(161, 100, 171, 140));
    }
}

#[test]
fn move_input_escape_and_focus_loss_cancel_and_allow_the_next_drag() {
    for focus_loss in [false, true] {
        let (mut h, a, _) = harness();
        let steps = history(&h);
        let count = h.state().session.active().unwrap().doc.layers.len();
        let p = screen(&h, 100.0, 120.0);
        pointer(&mut h, p, Modifiers::ALT);
        button(&mut h, p, true, Modifiers::ALT);
        pointer(&mut h, p + vec2(10.0, 0.0), Modifiers::ALT);
        if focus_loss {
            h.event(Event::WindowFocused(false));
        } else {
            h.key_press(Key::Escape);
        }
        h.step();
        assert!(h.state().drag.is_none(), "focus_loss={focus_loss}");
        assert_eq!(history(&h), steps);
        assert_eq!(h.state().session.active().unwrap().doc.layers.len(), count);
        assert_eq!(bounds(&h, a), Rect::new(80, 100, 120, 140));
    }
}

#[test]
fn move_input_native_move_and_temporary_hand_cursors() {
    let (mut h, _, _) = harness();
    let p = screen(&h, 100.0, 120.0);
    // Photoshop's Move tool keeps the normal arrow while hovering and dragging; only the Hand
    // (here Space) shows a hand.
    pointer(&mut h, p, Modifiers::NONE);
    assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::Default);
    button(&mut h, p, true, Modifiers::NONE);
    pointer(&mut h, p + vec2(10.0, 0.0), Modifiers::NONE);
    assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::Default);
    button(&mut h, p + vec2(10.0, 0.0), false, Modifiers::NONE);
    h.key_down(Key::Space);
    h.step();
    assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::Grab);
    let center = h.state().ui.views[0].center;
    button(&mut h, p, true, Modifiers::NONE);
    pointer(&mut h, p + vec2(30.0, 0.0), Modifiers::NONE);
    assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::Grabbing);
    assert_ne!(h.state().ui.views[0].center, center, "Space really pans the view");
    button(&mut h, p + vec2(30.0, 0.0), false, Modifiers::NONE);
}

#[test]
fn move_input_smart_guides_use_the_auto_selected_layer_and_canvas_edges() {
    let (mut h, a, _) = harness();
    h.state_mut().ui.extras.snap = true;
    h.state_mut().ui.view.show.smart_guides = true;
    let p = screen(&h, 100.0, 120.0);
    pointer(&mut h, p, Modifiers::NONE);
    button(&mut h, p, true, Modifiers::NONE);
    // A's left edge is at 80. A delta of -75 is within the 8 px canvas-edge snap.
    pointer(&mut h, p + vec2(-75.0, 0.0), Modifiers::NONE);
    assert_eq!(preview_bounds(&mut h, a).x0, 0);
    assert!(!h.state().prefs_rt.snap_lines.is_empty());
    // Holding Control bypasses snapping, without ending the drag.
    pointer(&mut h, p + vec2(-75.0, 0.0), Modifiers::CTRL);
    assert_eq!(preview_bounds(&mut h, a).x0, 5);
    button(&mut h, p + vec2(-75.0, 0.0), false, Modifiers::CTRL);
}

#[test]
fn move_input_snaps_to_canvas_center_and_other_layers_and_command_bypasses() {
    for canvas_center in [true, false] {
        let (mut h, a, _) = harness();
        h.state_mut().ui.extras.snap = true;
        h.state_mut().ui.view.show.smart_guides = !canvas_center;
        h.state_mut().ui.view.snap_to.layers = !canvas_center;
        h.state_mut().ui.view.snap_to.document_bounds = canvas_center;
        let p = screen(&h, 100.0, 120.0);
        pointer(&mut h, p, Modifiers::NONE);
        button(&mut h, p, true, Modifiers::NONE);
        let delta = if canvas_center { vec2(95.0, 25.0) } else { vec2(155.0, 0.0) };
        pointer(&mut h, p + delta, Modifiers::NONE);
        assert_eq!(preview_bounds(&mut h, a), if canvas_center { Rect::new(180, 130, 220, 170) } else { Rect::new(240, 100, 280, 140) });
        let command = Modifiers { mac_cmd: true, command: true, ..Modifiers::NONE };
        pointer(&mut h, p + delta, command);
        assert_eq!(preview_bounds(&mut h, a).x0, 80 + delta.x as i32);
        button(&mut h, p + delta, false, command);
    }
}

#[test]
fn move_input_transform_controls_frame_and_scale_multiple_layers_together() {
    let (mut h, a, b) = harness();
    h.state_mut().session.execute("layer.select", json!({"layer": a.0, "mode": "add"})).unwrap();
    h.state_mut().ui.tool_options.move_show_transform = true;
    h.run_steps(2);
    let p = screen(&h, 280.0, 120.0); // Union's right edge handle, far from A's box.
    let steps = history(&h);
    pointer(&mut h, p, Modifiers::NONE);
    button(&mut h, p, true, Modifiers::NONE);
    assert_eq!(h.state().ui.transform.as_ref().unwrap().rect, [80.0, 100.0, 280.0, 140.0]);
    pointer(&mut h, p + vec2(100.0, 0.0), Modifiers::NONE);
    button(&mut h, p + vec2(100.0, 0.0), false, Modifiers::NONE);
    h.key_press(Key::Enter);
    h.step();
    assert_eq!(history(&h), steps + 1);
    let (ab, bb) = (bounds(&h, a), bounds(&h, b));
    assert!(ab.width() >= 60 && bb.width() >= 60, "both layers scale: {ab:?} {bb:?}");
    h.state_mut().session.undo();
    assert_eq!(bounds(&h, a), Rect::new(80, 100, 120, 140));
    assert_eq!(bounds(&h, b), Rect::new(240, 100, 280, 140));
}

#[test]
fn move_input_a_click_does_not_snap_or_edit_and_batched_tiny_drag_moves() {
    let (mut h, a, _) = harness();
    let p = screen(&h, 100.0, 120.0);
    let steps = history(&h);
    pointer(&mut h, p, Modifiers::NONE);
    button(&mut h, p, true, Modifiers::NONE);
    button(&mut h, p, false, Modifiers::NONE);
    assert_eq!(history(&h), steps);
    assert_eq!(bounds(&h, a), Rect::new(80, 100, 120, 140));
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerMoved(p + vec2(1.0, 0.0)));
    h.event(Event::PointerButton { pos: p + vec2(1.0, 0.0), button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
    assert_eq!(bounds(&h, a), Rect::new(81, 100, 121, 140));
    assert_eq!(history(&h), steps + 1);
}

#[test]
fn ds2_move_escape_uses_event_modifiers_and_dialog_open_cancels_capture() {
    for dialog in [false, true] {
        let (mut h, a, _) = harness();
        let p = screen(&h, 100., 120.);
        pointer(&mut h, p, Modifiers::NONE);
        button(&mut h, p, true, Modifiers::NONE);
        pointer(&mut h, p + vec2(20., 0.), Modifiers::NONE);
        assert!(super::captured(&h.ctx));
        let steps = history(&h);
        if dialog {
            h.event(Event::Key { key: Key::L, physical_key: Some(Key::L), pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
        } else {
            h.event(Event::Key { key: Key::Escape, physical_key: Some(Key::Escape), pressed: true, repeat: false, modifiers: Modifiers::CTRL });
        }
        h.event(Event::ModifiersChanged(Modifiers::NONE));
        h.step();
        if dialog {
            assert!(!h.state().ui.dialogs.is_empty());
        }
        assert!(!super::captured(&h.ctx));
        assert!(h.state().drag.is_none());
        button(&mut h, p + vec2(20., 0.), false, Modifiers::NONE);
        assert_eq!(bounds(&h, a), Rect::new(80, 100, 120, 140));
        assert_eq!(history(&h), steps);
    }
}

#[test]
fn ds2_repeated_move_frames_reuse_target_list_and_preview_document() {
    let (mut h, a, _) = harness();
    let p = screen(&h, 100., 120.);
    pointer(&mut h, p, Modifiers::NONE);
    button(&mut h, p, true, Modifiers::NONE);
    pointer(&mut h, p + vec2(20., 0.), Modifiers::NONE);
    let (shown, key) = crate::move_ui::display_doc(h.state_mut(), 0).unwrap();
    for _ in 0..30 {
        let (again, k) = crate::move_ui::display_doc(h.state_mut(), 0).unwrap();
        assert_eq!(k, key);
        assert!(std::sync::Arc::ptr_eq(&shown, &again));
    }
    assert_eq!(shown.layer(a).unwrap().surface().unwrap().content_bounds(), Rect::new(100, 100, 140, 140));
}
