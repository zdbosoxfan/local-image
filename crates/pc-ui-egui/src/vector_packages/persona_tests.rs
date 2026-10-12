use std::sync::Arc;

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_engine::Session;

use super::*;

pub(super) fn harness() -> Harness<'static, PhotocraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut s = Session::new();
        s.execute("file.new", json!({"width":256,"height":192,"background":"white"})).unwrap();
        PhotocraftApp::new(s, Default::default())
    });
    h.run_steps(6);
    h
}

pub(super) fn pointer(h: &mut Harness<'_, PhotocraftApp>, pos: Pos2, down: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = down {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(3);
}

pub(super) fn drag(h: &mut Harness<'_, PhotocraftApp>, from: Pos2, to: Pos2) {
    pointer(h, from, Some(true));
    pointer(h, from.lerp(to, 0.5), None);
    pointer(h, to, None);
    pointer(h, to, Some(false));
}

#[test]
fn persona_registry_icons_exist_in_the_existing_icon_system() {
    for tool in VECTOR_TOOLS {
        assert!(crate::icons::exists(tool.icon), "{}: {}", tool.id, tool.icon);
    }
}

#[test]
fn persona_reserved_tool_memory_survives_a_provider_not_yet_registered() {
    let mut h = harness();
    h.state_mut().session.load_prefs_json(r#"{"toolbar":{"persona":"vector","vectorTool":"VectorPencil","vectorSet":"vector.illustration"}}"#).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.vector_tool, "VectorPencil");
    h.get_by_label("Pixel persona").click();
    h.run_steps(3);
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.vector_tool, "VectorPencil");
    h.get_by_label("Pen Tool").click();
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.vector_tool, "Pen", "an actual tool choice updates the memory");
}

#[test]
fn persona_tool_and_set_memories_restore_after_actual_switches_and_pref_reload() {
    let mut h = harness();
    h.get_by_label("Brush Tool").click();
    h.run_steps(3);
    h.get_by_label("Tool Set: All Tools").click();
    h.run_steps(3);
    h.get_by_label("Retouching").click();
    h.run_steps(3);
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    h.get_by_label("Pen Tool").click();
    h.run_steps(3);
    h.get_by_label("Tool Set: All Tools").click();
    h.run_steps(3);
    h.get_by_label("Photographer").click();
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.active().id, "vector.photographer");
    h.get_by_label("Pixel persona").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, Tool::Brush);
    assert_eq!(h.state().session.prefs().toolbar.active().id, "retouching");
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, Tool::Pen);
    assert_eq!(h.state().session.prefs().toolbar.active().id, "vector.photographer");
    let json = h.state().session.prefs_to_json();
    let mut loaded = photocraft_engine::Session::new();
    loaded.load_prefs_json(&json).unwrap();
    assert_eq!(loaded.prefs().toolbar.pixel_tool, "Brush");
    assert_eq!(loaded.prefs().toolbar.vector_tool, "Pen");
    assert_eq!(loaded.prefs().toolbar.active_set, "retouching");
    assert_eq!(loaded.prefs().toolbar.vector_set, "vector.photographer");
}

#[test]
fn persona_clicks_and_shortcuts_keep_document_dock_selection_and_history() {
    let mut h = harness();
    h.state_mut().run("select.rect", json!({"x":10,"y":10,"width":30,"height":40})).unwrap();
    h.run_steps(3);
    let st = h.state().session.active().unwrap();
    let doc = st.doc.clone();
    let history = st.history.past_len();
    let selected = st.selected_layers();
    let dock = serde_json::to_value(&h.state().ui.dock).unwrap();
    let rows = h.get_all_by_label("Background").count();
    assert_eq!(rows, 1, "one actual Layers row");
    h.get_by_label("Brush Tool");
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.persona, Persona::Vector);
    h.get_by_label("Pen Tool");
    assert!(h.query_by_label("Brush Tool").is_none());
    let vector = crate::panels::visible_sections(h.state());
    assert!(!vector.iter().flatten().any(|(_, tools)| tools.contains(&Tool::Brush)));
    let mods = Modifiers { command: true, shift: true, ..Default::default() };
    h.key_press_modifiers(mods, Key::Num1);
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.persona, Persona::Pixel);
    h.key_press_modifiers(mods, Key::Num2);
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.persona, Persona::Vector);
    let st = h.state().session.active().unwrap();
    assert!(Arc::ptr_eq(&st.doc, &doc), "switching must not publish a document edit");
    assert_eq!((st.selected_layers(), st.history.past_len()), (selected, history));
    assert_eq!(serde_json::to_value(&h.state().ui.dock).unwrap(), dock);
    assert_eq!(h.get_all_by_label("Background").count(), rows);
}

#[test]
fn persona_pen_drag_is_detached_and_switch_cancels_it_twenty_times() {
    let mut h = harness();
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    h.get_by_label("Pen Tool").click();
    h.run_steps(3);
    let doc = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    for _ in 0..20 {
        let from = h.state().last_canvas_rect.center();
        pointer(&mut h, from, Some(true));
        pointer(&mut h, from + egui::vec2(18.0, 12.0), None);
        assert!(h.state().shared_tools.pen.gesture.as_ref().is_some_and(|g| g.captured() && !g.path().is_empty()));
        assert!(Arc::ptr_eq(&h.state().session.active().unwrap().doc, &doc));
        // Interrupt a held press with the actual persona shortcut; its release cannot commit.
        h.key_press_modifiers(Modifiers { command: true, shift: true, ..Default::default() }, Key::Num1);
        h.run_steps(3);
        pointer(&mut h, from, Some(false));
        assert!(h.state().ui.pen.is_none());
        assert!(h.state().shared_tools.pen.gesture.is_none());
        h.get_by_label("Vector persona").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.tool, Tool::Pen, "Vector remembers its last tool");
    }
    assert!(Arc::ptr_eq(&h.state().session.active().unwrap().doc, &doc));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
}

#[test]
fn persona_vector_shape_on_pixels_and_pixel_prompt_cancel_are_real_canvas_gestures() {
    let mut h = harness();
    let original = h.state().session.active().unwrap().doc.layers[0].clone();
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    h.get_by_label("Rectangle Tool").click();
    h.run_steps(3);
    let center = h.state().last_canvas_rect.center();
    drag(&mut h, center - egui::vec2(40.0, 30.0), center + egui::vec2(40.0, 30.0));
    let st = h.state().session.active().unwrap();
    assert_eq!(st.doc.layers.len(), 2);
    let shape = st.doc.layer(st.active_layer.unwrap()).unwrap();
    assert!(matches!(shape.content, LayerContent::Shape(_)));
    assert!(st.doc.layer(original.id).is_some_and(|l| l.name == original.name && matches!(l.content, LayerContent::Raster(_))));
    let doc = st.doc.clone();
    let history = st.history.past_len();
    h.get_by_label("Pixel persona").click();
    h.run_steps(3);
    h.get_by_label("Brush Tool").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, Tool::Brush);
    assert!(crate::rasterize_prompt::needed(h.state()).is_some(), "the active shape needs rasterization");
    let center = h.state().last_canvas_rect.center();
    drag(&mut h, center, center + egui::vec2(12.0, 8.0));
    assert!(h.state().ui.dialogs.iter().any(|d| crate::rasterize_prompt::owns(&d.fields)));
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(Arc::ptr_eq(&h.state().session.active().unwrap().doc, &doc));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
}

#[test]
fn persona_vector_custom_set_click_rename_restart_delete() {
    let mut h = harness();
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "edit.toolbar", json!({})).unwrap();
    h.run_steps(3);
    h.get_by_label("New").click();
    h.run_steps(3);
    h.get_by_label("Rename").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Tool Set Name").focus();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Tool Set Name").type_text("Logos");
    h.run_steps(2);
    h.get_by_label("Save Name").click();
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let saved = h.state().session.prefs_to_json();
    let mut restarted = harness();
    restarted.state_mut().session.load_prefs_json(&saved).unwrap();
    restarted.run_steps(3);
    assert_eq!(restarted.state().session.prefs().toolbar.active().name, "Logos");
    assert_eq!(restarted.state().session.prefs().toolbar.persona, Persona::Vector);
    let ctx = restarted.ctx.clone();
    crate::menus::invoke(restarted.state_mut(), &ctx, "edit.toolbar", json!({})).unwrap();
    restarted.run_steps(3);
    restarted.get_by_label("Delete").click();
    restarted.run_steps(3);
    restarted.get_by_label("OK").click();
    restarted.run_steps(3);
    let mut reload = Session::new();
    reload.load_prefs_json(&restarted.state().session.prefs_to_json()).unwrap();
    assert!(reload.prefs().toolbar.sets.is_empty());
    assert_eq!(reload.prefs().toolbar.active().id, "vector.allTools");
}

#[test]
fn persona_geometry_rejects_stale_and_mixed_targets_before_command() {
    let mut h = harness();
    two_shapes(h.state_mut());
    let request = GeometryRequest::capture(h.state(), Geometry::Add, false).unwrap();
    h.state_mut().run("layer.setProps", json!({"name":"Changed"})).unwrap();
    let doc = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    assert!(request.execute(h.state_mut()).is_err());
    assert!(Arc::ptr_eq(&doc, &h.state().session.active().unwrap().doc));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    let st = h.state_mut().session.active_mut().unwrap();
    st.selected_layers = st.doc.layers.iter().map(|l| l.id).collect();
    assert!(GeometryRequest::capture(h.state(), Geometry::Add, false).is_err());
}

#[test]
fn persona_pointer_guard_rejects_same_frame_stale_release_and_nonfinite_input() {
    use crate::canvas::ToolEvent;
    let mut h = harness();
    h.state_mut().ui.tool = Tool::Pen;
    let mut guard = PointerGuard::default();
    assert!(!guard.intercept(h.state_mut(), ToolEvent::Down { x: 30.0, y: 40.0, pressure: 1.0 }));
    crate::vector_ui::pen_down(h.state_mut(), 30.0, 40.0);
    h.state_mut().run("file.new", json!({"width":256,"height":192})).unwrap();
    let doc = h.state().session.active().unwrap().doc.clone();
    assert!(guard.intercept(h.state_mut(), ToolEvent::Up { x: 30.0, y: 40.0 }));
    assert!(h.state().ui.pen.is_none());
    assert!(Arc::ptr_eq(&doc, &h.state().session.active().unwrap().doc));
    for (x, y, pressure) in [(f64::NAN, 1.0, 1.0), (1.0, f64::INFINITY, 1.0), (1.0, 1.0, f32::NAN), (1e10, 1.0, 1.0)] {
        assert!(guard.intercept(h.state_mut(), ToolEvent::Down { x, y, pressure }));
        assert!(Arc::ptr_eq(&doc, &h.state().session.active().unwrap().doc));
    }
}

#[test]
fn persona_rasterize_prompt_capture_rejects_document_or_revision_changes() {
    let mut h = harness();
    let mut fields = serde_json::Map::new();
    capture_rasterize_prompt(h.state(), &mut fields).unwrap();
    validate_rasterize_prompt(h.state(), &fields).unwrap();
    h.state_mut().run("shape.create", json!({"kind":"rect","rect":[20,20,40,30]})).unwrap();
    assert!(validate_rasterize_prompt(h.state(), &fields).is_err());
    capture_rasterize_prompt(h.state(), &mut fields).unwrap();
    h.state_mut().run("file.new", json!({"width":256,"height":192})).unwrap();
    assert!(validate_rasterize_prompt(h.state(), &fields).is_err());
}

pub(super) fn two_shapes(app: &mut PhotocraftApp) {
    app.run("shape.create", json!({"kind":"rect","rect":[20,20,80,60],"fill":"#dc3020"})).unwrap();
    let first = app.session.active().unwrap().active_layer.unwrap();
    app.run("shape.create", json!({"kind":"rect","rect":[60,20,80,60],"fill":"#dc3020"})).unwrap();
    let st = app.session.active_mut().unwrap();
    st.selected_layers = vec![first, st.active_layer.unwrap()];
}

/// Required integration gate: run explicitly after 00 registers the actual V1 commands.
#[test]
#[ignore = "requires V1 path.boolean registration by package 00"]
fn persona_geometry_actual_add_and_undo_restore_both_shapes() {
    let mut h = harness();
    two_shapes(h.state_mut());
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    assert!(h.state().session.is_enabled("path.boolean"), "V1 must be integrated; absence is not a passing gate");
    let st = h.state().session.active().unwrap();
    let doc = st.doc.clone();
    let selected = st.selected_layers();
    let history = st.history.past_len();
    h.get_by_label("Add").click();
    h.run_steps(3);
    let st = h.state().session.active().unwrap();
    assert_eq!(st.doc.layers.len(), 2);
    assert_eq!(st.history.past_len(), history + 1);
    let LayerContent::Shape(shape) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("union shape") };
    let bounds = shape.cache.as_ref().expect("V1 prepares a shape cache").content_bounds();
    assert_eq!((bounds.x0, bounds.y0, bounds.x1, bounds.y1), (20, 20, 140, 80));
    // Independent rectangle-union oracle: 80*60 + 80*60 - 40*60 = 7200 covered pixels.
    let pixels = photocraft_compose::flatten(&st.doc);
    let red = pixels.px.iter().filter(|p| p[0] > 0.5 && p[1] < 0.3 && p[2] < 0.3).count();
    assert_eq!(red, 7200);
    assert!(h.state_mut().session.undo());
    let st = h.state().session.active().unwrap();
    assert!(Arc::ptr_eq(&doc, &st.doc));
    assert_eq!(st.selected_layers(), selected);
}

#[test]
fn persona_unbound_pixel_keys_and_saved_pixel_overrides_do_not_leave_vector() {
    use photocraft_engine::vector_toolbar_layout::Preset;
    for preset in [Preset::Illustrator, Preset::Affinity] {
        let mut h = harness();
        h.state_mut().session.edit_prefs(|p| {
            p.toolbar.vector_keyboard.preset = preset;
            p.shortcuts.insert("tools.select.SpotHealing".into(), "J".into());
        });
        h.get_by_label("Vector persona").click();
        h.run_steps(3);
        let doc = h.state().session.active().unwrap().doc.clone();
        let keys: &[Key] = if preset == Preset::Affinity { &[Key::J, Key::E, Key::L] } else { &[Key::J] };
        for key in keys {
            let tool = h.state().ui.tool;
            h.key_press(*key);
            h.run_steps(3);
            assert_eq!(h.state().session.prefs().toolbar.persona, Persona::Vector, "{preset:?}/{key:?}");
            assert_eq!(h.state().ui.tool, tool, "an unbound key keeps the selected tool");
            assert!(Arc::ptr_eq(&doc, &h.state().session.active().unwrap().doc));
        }
        assert!(crate::shortcuts::tool_shortcut(&h.state().session, Tool::SpotHealing).is_none());
        h.state_mut().session.edit_prefs(|p| {
            p.shortcuts.insert("tools.select.SpotHealing".into(), "V".into());
        });
        h.get_by_label("Pen Tool").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.tool, Tool::Pen);
        h.key_press(Key::V);
        h.run_steps(3);
        assert_eq!(h.state().ui.tool, Tool::Move, "ignored Pixel-only override must not steal Vector V");
    }
}

#[test]
fn persona_photo_admission_rechecks_document_and_respects_later_explicit_choices() {
    let mut h = harness();
    assert_eq!(h.state().ui.tool, Tool::Move, "fresh editor starts with Move");
    let a = h.state().session.active().unwrap().doc.id;
    h.state_mut().run("file.new", json!({"width":32,"height":32})).unwrap();
    let b = h.state().session.active().unwrap().doc.id;
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    let tool = h.state().ui.tool;
    assert!(!photo_admitted(h.state_mut(), a), "inactive photo cannot claim admission");
    assert!(photo_admitted(h.state_mut(), b));
    h.state_mut().session.set_active(0);
    h.state_mut().ui.crop_rect = Some([1., 2., 20., 20.]);
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.persona, Persona::Vector);
    assert_eq!(h.state().ui.tool, tool);
    assert_eq!(h.state().ui.crop_rect, Some([1., 2., 20., 20.]), "another document's draft survives");
    assert!(photo_admitted(h.state_mut(), a));
    let ctx = h.ctx.clone();
    select(h.state_mut(), &ctx, Persona::Vector);
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.persona, Persona::Vector, "explicit later persona choice wins");
    assert!(photo_admitted(h.state_mut(), a));
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().toolbar.persona, Persona::Pixel);
    assert_eq!(h.state().ui.tool, Tool::Move);
}
