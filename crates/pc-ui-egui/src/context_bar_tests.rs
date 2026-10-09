//! The Contextual Task Bar follows the selected item, and its buttons work: a press on the bar
//! no longer hides it (which made egui drop the press, so no button ever clicked).

use std::sync::Arc;

use egui::{Event, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use photocraft_doc::{Color, ColorMode, Document, LayerContent, LayerId, SampleType, Size};
use serde_json::json;

use super::*;
use crate::state::Tool;

fn app() -> PhotocraftApp {
    let doc = Document::with_background("bar", Size::new(120, 90), ColorMode::Rgb, SampleType::U8, Color::WHITE);
    let mut s = photocraft_engine::Session::new();
    s.add_document(doc, None);
    PhotocraftApp::new(s, crate::Services::default())
}

/// A closed pen path offers the bar (with the Pen tools), its Make Selection turns into the
/// selection bar, and Deselect or the Window toggle hides it.
#[test]
fn shows_after_a_closed_path_and_after_a_selection() {
    let mut app = app();
    assert_eq!(context(&app), None);
    let square = json!({"subpaths": [{"closed": true, "knots": [[10, 10], [60, 10], [60, 60], [10, 60]]}]});
    app.run("path.set", json!({ "path": square })).unwrap();
    app.ui.tool = crate::Tool::Move;
    assert_eq!(context(&app), None, "a path only offers the bar while it is selected (a Pen tool, the Paths panel)");
    app.ui.tool = crate::Tool::Pen;
    let (what, r) = context(&app).unwrap();
    assert_eq!(what, Context::Path);
    assert_eq!((r.x0, r.y0, r.x1, r.y1), (10, 10, 60, 60));
    // An open path doesn't.
    app.run("path.set", json!({ "path": {"subpaths": [{"closed": false, "knots": [[10, 10], [60, 10], [60, 60]]}]} })).unwrap();
    assert_eq!(context(&app), None);
    app.run("path.set", json!({ "path": square })).unwrap();
    app.run("path.toSelection", json!({ "name": "work" })).unwrap();
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Selection));
    let _ = menu(&mut app, TOGGLE_ID);
    assert_eq!(context(&app), None, "Window › Contextual Task Bar turns it off");
    assert_eq!(checked(&app, TOGGLE_ID), Some(false));
    let _ = menu(&mut app, TOGGLE_ID);
    app.run("select.deselect", json!({})).unwrap();
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Path), "deselected, the path offers it again");
    // Selected in the Paths panel, whatever the tool.
    app.ui.tool = crate::Tool::Move;
    app.ui.selected_path = Some("work".into());
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Path));
}

/// The selected layer's kind picks the bar, whatever tool made it; a selection or path wins.
#[test]
fn the_selected_layer_picks_the_bar() {
    let mut app = app();
    app.ui.tool = Tool::Move;
    let shape = LayerId(app.run("shape.create", json!({"kind": "rect", "rect": [10, 10, 40, 30], "fill": "#3070c0"})).unwrap()["layer"].as_u64().unwrap());
    let (what, r) = context(&app).unwrap();
    assert_eq!(what, Context::Shape(shape));
    assert_eq!((r.x0, r.y0, r.x1, r.y1), (10, 10, 50, 40));
    // Any tool that isn't painting: the Rectangle tool, Path Selection…
    for tool in [Tool::Rectangle, Tool::PathSelection, Tool::Hand] {
        app.ui.tool = tool;
        assert_eq!(context(&app).map(|c| c.0), Some(Context::Shape(shape)), "{tool:?}");
    }
    app.ui.tool = Tool::Brush;
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Shape(shape)), "the selected item's context survives changing tools");
    app.ui.tool = Tool::Move;
    let text = LayerId(app.run("type.create", json!({"text": "Hello", "size": 18, "x": 10, "y": 60})).unwrap()["layer"].as_u64().unwrap());
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Text(text)));
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("select.rect", json!({"x": 5, "y": 5, "width": 20, "height": 20})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Selection), "the selection comes first");
    app.run("select.deselect", json!({})).unwrap();
    let pixel = app.session.active().unwrap().active_layer.unwrap();
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Pixel(pixel)));
    // An AI-made layer offers Regenerate.
    Arc::make_mut(&mut app.session.active_mut().unwrap().doc).layer_mut(pixel).unwrap().generation =
        Some(json!({"command": "ai.generativeFill", "prompt": "a cat"}));
    app.session.active_mut().unwrap().revision += 1;
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Generated(pixel)));
    // The Background shows nothing (it's the document).
    let bg = app.session.active().unwrap().doc.layers[0].id;
    app.run("layer.select", json!({"layer": bg.0})).unwrap();
    assert_eq!(context(&app), None);
}

#[test]
fn unpainted_shapes_and_layers_without_preview_caches_keep_their_context() {
    let mut app = app();
    let id =
        LayerId(app.run("shape.create", json!({"kind": "rect", "rect": [10, 10, 40, 30], "fill": null, "stroke": null})).unwrap()["layer"].as_u64().unwrap());
    assert_eq!(context(&app), Some((Context::Shape(id), photocraft_geom::Rect::new(10, 10, 50, 40))));
    app.run("shape.edit", json!({"layer": id.0, "fill": "#3070c0"})).unwrap();
    app.run("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
    let id = app.session.active().unwrap().active_layer.unwrap();
    if let LayerContent::Smart(smart) = &mut Arc::make_mut(&mut app.session.active_mut().unwrap().doc).layer_mut(id).unwrap().content {
        smart.cache = None;
    }
    app.session.active_mut().unwrap().revision += 1;
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Smart(id)));
    app.run("layer.new.layer", json!({})).unwrap();
    let id = app.session.active().unwrap().active_layer.unwrap();
    assert_eq!(context(&app).map(|c| c.0), Some(Context::Pixel(id)));
}

#[test]
fn placed_generate_results_offer_variations_with_their_saved_settings() {
    let mut h = harness(240, 160);
    h.state_mut().run("layer.new.layer", json!({})).unwrap();
    let layer = active(&h);
    let g = json!({"command": "ai.generate", "model": "qwen", "variant": "int8", "prompt": "marble", "negative_prompt": "letters", "seed": 42,
        "mode": "fill", "width": 240, "height": 160, "steps": 8, "guidance": 3.0, "denoise": 0.75});
    let app = h.state_mut();
    app.run("select.rect", json!({"x": 10, "y": 10, "width": 40, "height": 40})).unwrap();
    app.run("edit.fill", json!({"color": "#3070c0"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    Arc::make_mut(&mut app.session.active_mut().unwrap().doc).layer_mut(layer).unwrap().generation = Some(g);
    app.session.active_mut().unwrap().revision += 1;
    app.ui.tool = Tool::Move;
    h.run_steps(3);
    bar_widget(&h, "Regenerate");
    let at = bar_widget(&h, "Variations");
    click_at(&mut h, at);
    assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
    let s = &h.state().ui.ai.generate;
    assert_eq!(s.prompt, "marble");
    assert_eq!(s.negative, "letters");
    assert_eq!(s.variant, "int8");
    assert_eq!((s.width, s.height, s.steps), (240, 160, 8));
    assert_eq!(s.seed, None, "a fresh seed for the next variation");
    assert_eq!(s.mode, crate::generate_ui::Mode::Fill);
    assert!(h.state().ui.panels.generate);
    assert!(!h.state().ui.dock.is_collapsed(crate::dock::Group::Generate));
    assert!(h.state().session.active().unwrap().doc.layer(layer).unwrap().visible);
}

// ------------------------------------------------------------------------------- in the app

fn harness(width: u32, height: u32) -> Harness<'static, PhotocraftApp> {
    with(|s| *s = State::default());
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": width, "height": height})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(s, crate::Services::default())
    });
    h.state_mut().background_jobs = false;
    h.run_steps(6);
    h
}

/// A real click: press and release in separate frames, as a hand does (the bar used to vanish on
/// the press frame, and egui dropped the click).
fn click_at(h: &mut Harness<'_, PhotocraftApp>, pos: Pos2) {
    h.hover_at(pos);
    h.step();
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.step();
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
}

/// The centre of the bar's button (or other widget) labelled `label`.
fn bar_widget(h: &Harness<'_, PhotocraftApp>, label: &str) -> Pos2 {
    let bar = h.ctx.memory(|m| m.area_rect(egui::Id::new("li-context-bar"))).unwrap_or_else(|| panic!("no bar; status: {}", h.state().ui.status));
    let found: Vec<_> = h.query_all(egui_kittest::kittest::by().label(label)).map(|n| n.rect()).collect();
    let r = found.iter().find(|r| bar.contains(r.center())).unwrap_or_else(|| panic!("no {label:?} on the bar {bar:?}: {found:?}"));
    r.center()
}

fn rect_path(x: f64, y: f64, w: f64, h: f64) -> serde_json::Value {
    let k = |x: f64, y: f64| json!({"anchor": [x, y]});
    json!({"subpaths": [{"closed": true, "knots": [k(x, y), k(x + w, y), k(x + w, y + h), k(x, y + h)]}]})
}

/// A white canvas with a red blob and a pen path around it, the Pen active.
fn pen_scene() -> Harness<'static, PhotocraftApp> {
    let mut h = harness(240, 160);
    let app = h.state_mut();
    app.run("select.rect", json!({"x": 100, "y": 60, "width": 30, "height": 30})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    app.run("path.set", json!({"name": "work", "path": rect_path(95.0, 55.0, 40.0, 40.0)})).unwrap();
    app.ui.tool = Tool::Pen;
    h.run_steps(4);
    assert_eq!(context(h.state()).map(|c| c.0), Some(Context::Path));
    h
}

fn pixel(h: &Harness<'_, PhotocraftApp>, id: LayerId, x: i32, y: i32) -> Vec<f32> {
    h.state().session.active().unwrap().doc.layer(id).unwrap().surface().unwrap().pixel(x, y)
}

fn active(h: &Harness<'_, PhotocraftApp>) -> LayerId {
    h.state().session.active().unwrap().active_layer.unwrap()
}

#[test]
fn the_pen_path_bar_offers_exactly_its_four_actions() {
    let h = pen_scene();
    for label in ["Make Selection", "Content-Aware Fill", "AI Fill", "Mask"] {
        bar_widget(&h, label);
    }
    let bar = h.ctx.memory(|m| m.area_rect(egui::Id::new("li-context-bar"))).unwrap();
    assert!(!h.query_all(egui_kittest::kittest::by().label("Remove")).any(|n| bar.contains(n.rect().center())), "Remove is a selection action");
}

#[test]
fn path_bar_make_selection_works() {
    let mut h = pen_scene();
    let at = bar_widget(&h, "Make Selection");
    click_at(&mut h, at);
    let sel = h.state().session.active().unwrap().doc.selection.as_ref().map(|s| s.content_bounds());
    let r = sel.unwrap_or_else(|| panic!("no selection; status: {}", h.state().ui.status));
    assert_eq!((r.x0, r.y0, r.width(), r.height()), (95, 55, 40, 40));
    assert_eq!(context(h.state()).map(|c| c.0), Some(Context::Selection), "the selection bar takes over");
}

#[test]
fn path_bar_content_aware_fill_works() {
    let mut h = pen_scene();
    let bg = active(&h);
    assert_eq!(&pixel(&h, bg, 115, 75)[..3], &[1.0, 0.0, 0.0]);
    let at = bar_widget(&h, "Content-Aware Fill");
    click_at(&mut h, at);
    assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
    let v = pixel(&h, bg, 115, 75);
    assert!(!(v[0] > 0.9 && v[1] < 0.1), "the blob was filled from around it: {v:?}");
}

#[test]
fn path_bar_content_aware_fill_on_a_smart_layer_samples_the_composite() {
    let mut h = pen_scene();
    h.state_mut().run("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
    let smart = active(&h);
    let before = h.state().session.active().unwrap().doc.layer(smart).unwrap().clone();
    h.run_steps(3);
    let at = bar_widget(&h, "Content-Aware Fill");
    click_at(&mut h, at);
    assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
    let d = h.state().session.active().unwrap();
    let filled = d.active_layer.unwrap();
    assert_ne!(filled, smart);
    assert_eq!(d.doc.layer(smart).unwrap(), &before);
    let v = pixel(&h, filled, 115, 75);
    assert!(v[1] > 0.9 && v[2] > 0.9 && v[3] > 0.9, "composite surroundings filled on the new layer: {v:?}");
    assert_eq!(pixel(&h, filled, 10, 10)[3], 0.0, "outside the path is transparent");
}

#[test]
fn clicking_a_saved_path_keeps_its_actions_with_move() {
    let mut h = pen_scene();
    let app = h.state_mut();
    app.run("path.set", json!({"name": "Saved outline", "path": rect_path(20.0, 20.0, 30.0, 40.0)})).unwrap();
    app.ui.tool = Tool::Move;
    app.ui.dock_tabs.layers = 2;
    crate::dock::reveal(app, crate::dock::Group::Layers);
    h.run_steps(3);
    let at = h.get_by_label("Saved outline").rect().center();
    click_at(&mut h, at);
    assert_eq!(h.state().ui.selected_path.as_deref(), Some("Saved outline"));
    let at = bar_widget(&h, "Make Selection");
    click_at(&mut h, at);
    let bounds = h.state().session.active().unwrap().doc.selection.as_ref().unwrap().content_bounds();
    assert_eq!(bounds, photocraft_geom::Rect::new(20, 20, 50, 60), "acts on the selected saved path, not the work path");
}

#[test]
fn path_bar_mask_adds_a_vector_mask() {
    let mut h = pen_scene();
    h.state_mut().run("layer.new.layer", json!({})).unwrap();
    h.state_mut().ui.tool = Tool::Pen;
    h.run_steps(3);
    let layer = active(&h);
    let at = bar_widget(&h, "Mask");
    click_at(&mut h, at);
    let l = h.state().session.active().unwrap().doc.layer(layer).unwrap().clone();
    let vm = l.vector_mask.unwrap_or_else(|| panic!("no vector mask; status: {}", h.state().ui.status));
    assert_eq!(vm.path.control_bounds(), Some((95.0, 55.0, 135.0, 95.0)));
    // On the Background (which can't take one) it's a layer mask from the path.
    let mut h = pen_scene();
    let at = bar_widget(&h, "Mask");
    click_at(&mut h, at);
    let d = h.state().session.active().unwrap();
    let l = d.doc.layer(d.active_layer.unwrap()).unwrap();
    assert!(l.mask.is_some(), "status: {}", h.state().ui.status);
}

/// Path and selection fill buttons open a focused prompt, and Back restores their actions.
#[test]
fn path_and_selection_fill_prompts_open_accept_text_and_close() {
    with(|s| s.fill_prompt.clear());
    let mut h = pen_scene();
    for label in ["AI Fill", "Generative Fill"] {
        if label == "Generative Fill" {
            h.state_mut().run("path.toSelection", json!({"name": "work"})).unwrap();
            h.run_steps(3);
        }
        let at = bar_widget(&h, label);
        click_at(&mut h, at);
        bar_widget(&h, "Generate");
        h.event(Event::Text("a cat".into()));
        h.run_steps(2);
        assert!(with(|s| s.fill_prompt.contains("a cat")), "the prompt has keyboard focus");
        let at = bar_widget(&h, "Back");
        click_at(&mut h, at);
        bar_widget(&h, label);
    }
}

#[test]
fn path_and_selection_ai_and_generated_layer_buttons_work() {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            li_ai::set_service_override(None);
            crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = None);
        }
    }
    let server = match li_ai::mock::MockComfy::start() {
        Ok(server) => server,
        Err(e)
            if e.kind() == std::io::ErrorKind::PermissionDenied
                || e.get_ref().and_then(|e| e.downcast_ref::<std::io::Error>()).is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied) =>
        {
            eprintln!("skipped: sandbox denies sockets for the mock HTTP server");
            return;
        }
        Err(e) => panic!("mock server: {e}"),
    };
    li_ai::set_service_override(Some(server.host().to_owned()));
    let _reset = Reset;
    let (model, variant) = crate::ai_ui::remove_engine("klein");
    let mut status = crate::ai_ui::EngineStatus { connected: true, checked: true, ..Default::default() };
    status.presets.insert(format!("{}:{variant}", model.key()), Ok(()));
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(status));
    let mut h = pen_scene();
    let bg = active(&h);
    let layers = h.state().session.active().unwrap().doc.walk().len();
    let at = bar_widget(&h, "AI Fill");
    click_at(&mut h, at);
    let at = bar_widget(&h, "Generate");
    click_at(&mut h, at);
    let d = h.state().session.active().unwrap();
    assert!(d.doc.selection.is_some(), "the path became the selection");
    assert_eq!(d.doc.walk().len(), layers + 1, "a Generative Fill layer was added; status: {}", h.state().ui.status);
    let l = d.doc.layer(d.active_layer.unwrap()).unwrap();
    assert_eq!(l.name, "Generative Fill");
    assert_eq!(l.generation.as_ref().and_then(|g| g["command"].as_str()), Some("ai.generativeFill"));
    // Reselect the generated layer with Move: its own actions return after deselecting.
    h.state_mut().ui.tool = Tool::Move;
    h.state_mut().run("select.deselect", json!({})).unwrap();
    h.run_steps(3);
    for label in ["Regenerate", "Variations"] {
        let old = active(&h);
        let generation = h.state().session.active().unwrap().doc.layer(old).unwrap().generation.clone().unwrap();
        let count = h.state().session.active().unwrap().doc.walk().len();
        let at = bar_widget(&h, label);
        click_at(&mut h, at);
        assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
        let d = h.state().session.active().unwrap();
        assert_eq!(d.doc.walk().len(), count + 1);
        assert!(!d.doc.layer(old).unwrap().visible);
        let new = d.doc.layer(d.active_layer.unwrap()).unwrap();
        assert_eq!(new.generation.as_ref().unwrap()["prompt"], generation["prompt"]);
        assert_ne!(new.generation.as_ref().unwrap()["seed"], generation["seed"]);
        h.state_mut().run("edit.undo", json!({})).unwrap();
        assert!(h.state().session.active().unwrap().doc.layer(old).unwrap().visible, "one undo restores the old result");
        h.state_mut().run("layer.select", json!({"layer": old.0})).unwrap();
        h.state_mut().run("select.deselect", json!({})).unwrap();
        h.run_steps(3);
    }
    let old = active(&h);
    server.fail_next();
    let at = bar_widget(&h, "Regenerate");
    click_at(&mut h, at);
    assert!(h.state().ui.status_error, "the mock rejected generation");
    assert!(h.state().session.active().unwrap().doc.layer(old).unwrap().visible, "a failed job keeps the old result visible");

    // Every AI button on the Selection bar dispatches too, including Back in the prompt.
    h.state_mut().run("layer.select", json!({"layer": bg.0})).unwrap();
    h.state_mut().ui.tool = Tool::RectMarquee;
    h.run_steps(3);
    let at = bar_widget(&h, "Generative Fill");
    click_at(&mut h, at);
    let at = bar_widget(&h, "Back");
    click_at(&mut h, at);
    bar_widget(&h, "Generative Fill");
    let at = bar_widget(&h, "Generative Fill");
    click_at(&mut h, at);
    let count = h.state().session.active().unwrap().doc.walk().len();
    let at = bar_widget(&h, "Generate");
    click_at(&mut h, at);
    assert_eq!(h.state().session.active().unwrap().doc.walk().len(), count + 1);
    let at = bar_widget(&h, "Remove");
    click_at(&mut h, at);
    let d = h.state().session.active().unwrap();
    assert_eq!(d.doc.walk().len(), count + 2, "Remove created a repair layer: {}", h.state().ui.status);
    assert!(d.doc.layer(d.active_layer.unwrap()).unwrap().name.starts_with("AI Remove"));
}

#[test]
fn selection_bar_buttons_work() {
    let mut h = pen_scene();
    h.state_mut().ui.tool = Tool::RectMarquee;
    h.state_mut().run("select.rect", json!({"x": 95, "y": 55, "width": 40, "height": 40})).unwrap();
    h.run_steps(3);
    let bg = active(&h);
    // Content-Aware Fill is on the bar itself.
    let at = bar_widget(&h, "Content-Aware Fill");
    click_at(&mut h, at);
    let v = pixel(&h, bg, 115, 75);
    assert!(!(v[0] > 0.9 && v[1] < 0.1), "filled: {v:?}; status: {}", h.state().ui.status);
    let at = bar_widget(&h, "Invert");
    click_at(&mut h, at);
    let sel = |h: &Harness<'_, PhotocraftApp>| h.state().session.active().unwrap().doc.selection.as_ref().map(|s| s.sample_channel(5, 5, 0));
    assert_eq!(sel(&h), Some(1.0), "inverted: the corner is selected");
    h.state_mut().run("layer.new.layer", json!({})).unwrap();
    h.run_steps(3);
    let layer = active(&h);
    let at = bar_widget(&h, "Mask");
    click_at(&mut h, at);
    assert!(h.state().session.active().unwrap().doc.layer(layer).unwrap().mask.is_some(), "status: {}", h.state().ui.status);
    h.state_mut().run("select.rect", json!({"x": 10, "y": 10, "width": 40, "height": 40})).unwrap();
    h.run_steps(3);
    let at = bar_widget(&h, "Deselect");
    click_at(&mut h, at);
    assert!(h.state().session.active().unwrap().doc.selection.is_none(), "deselected");
}

#[test]
fn selection_bar_more_opens_each_dialog_and_can_hide_the_bar() {
    for (label, command) in [("Feather…", "select.modify.feather"), ("Select and Mask…", "select.refineEdge"), ("Content-Aware Fill…", "edit.contentAwareFill")]
    {
        let mut h = pen_scene();
        h.state_mut().run("path.toSelection", json!({"name": "work"})).unwrap();
        h.run_steps(3);
        let at = bar_widget(&h, "More");
        click_at(&mut h, at);
        let at = h.get_by_label(label).rect().center();
        click_at(&mut h, at);
        let dialog = h.state().ui.dialogs.last().unwrap_or_else(|| panic!("{label} didn't open: {}", h.state().ui.status));
        assert_eq!(dialog.fields["__command"].as_str(), Some(command));
    }
    let mut h = pen_scene();
    h.state_mut().run("path.toSelection", json!({"name": "work"})).unwrap();
    h.run_steps(3);
    let at = bar_widget(&h, "More");
    click_at(&mut h, at);
    let at = h.get_by_label("Hide Contextual Task Bar").rect().center();
    click_at(&mut h, at);
    assert!(!enabled(h.state()));
}

/// Photoshop 101: with the Move tool and a shape layer selected, the options bar shows that
/// shape's options and editing its Fill changes the layer.
#[test]
fn move_tool_options_bar_edits_the_selected_shape() {
    let mut h = harness(240, 160);
    let id =
        LayerId(h.state_mut().run("shape.create", json!({"kind": "rect", "rect": [20, 20, 80, 50], "fill": "#3070c0"})).unwrap()["layer"].as_u64().unwrap());
    h.state_mut().run("shape.create", json!({"kind": "ellipse", "rect": [140, 20, 40, 40]})).unwrap();
    h.state_mut().ui.tool = Tool::Move;
    h.run_steps(4);
    h.state_mut().ui.dock.set_collapsed(crate::dock::Group::Properties, true);
    crate::dock::reveal(h.state_mut(), crate::dock::Group::Layers);
    h.run_steps(3);
    let row = crate::layer_row_ui::recorded(&h.ctx).into_iter().find(|r| r.layer == id.0).expect("the shape's Layers row").row;
    click_at(&mut h, row.center());
    assert_eq!(active(&h), id, "selecting a shape with Move restores its options");
    assert!(h.state().ui.dock.is_collapsed(crate::dock::Group::Properties), "keep Layers still during the double-click interval");
    h.run_steps(24);
    assert!(!h.state().ui.dock.is_collapsed(crate::dock::Group::Properties), "selection then reveals Properties");
    let fill = |h: &Harness<'_, PhotocraftApp>| match &h.state().session.active().unwrap().doc.layer(id).unwrap().content {
        LayerContent::Shape(sh) => sh.fill.clone(),
        _ => None,
    };
    // The options bar is the top-most Fill swatch.
    let swatch = h
        .query_all(egui_kittest::kittest::by().label("Set shape fill type"))
        .map(|n| n.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the options bar's Fill swatch");
    assert!(swatch.top() < 80.0, "in the options bar: {swatch:?}");
    let at = swatch.center();
    click_at(&mut h, at);
    let popup = h
        .ctx
        .memory(|m| {
            m.areas()
                .visible_layer_ids()
                .into_iter()
                .filter(|l| l.order == egui::Order::Foreground && l.id != egui::Id::new("li-context-bar"))
                .find_map(|l| m.area_rect(l.id))
        })
        .expect("the picker opened");
    let before = fill(&h);
    click_at(&mut h, popup.min + vec2(40.0, popup.height() * 0.3));
    assert_ne!(fill(&h), before, "the picker recoloured the shape");
    let at = h.get_by_label("No Color").rect().center();
    click_at(&mut h, at);
    assert_eq!(fill(&h), None);
    let at = h.get_by_label("Solid Color").rect().center();
    click_at(&mut h, at);
    assert_eq!(fill(&h), Some(photocraft_doc::Fill::Solid(Color::BLACK)), "No Color can become solid black directly");
    // The W field is bound to the layer too.
    assert_eq!(crate::vector_ui::active_shape(h.state()).map(|(l, _)| l), Some(id));
    // And the Contextual Task Bar shows the shape's actions.
    // Close the picker (Escape) and look at the bar.
    h.event(Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.event(Event::Key { key: egui::Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    bar_widget(&h, "Edit Path");
    bar_widget(&h, "Rasterize");
}

/// The options bar's W edits the selected shape through `shape.edit`, as Properties does.
#[test]
fn shape_options_share_the_shape_edit_params() {
    let mut app = app();
    let id =
        LayerId(app.run("shape.create", json!({"kind": "roundedRect", "rect": [10, 10, 40, 30], "radii": [4, 4, 4, 4]})).unwrap()["layer"].as_u64().unwrap());
    crate::vector_ui::apply_shape_edit(&mut app, id, Some(json!({"radii": [1, 2, 3, 4]})));
    crate::vector_ui::apply_shape_edit(&mut app, id, Some(json!({"rect": [12, 14, 40, 30]})));
    crate::vector_ui::apply_shape_edit(&mut app, id, Some(json!({"stroke": {"width": 3, "dashes": [4.0, 2.0]}})));
    let (_, sh) = crate::vector_ui::active_shape(&app).unwrap();
    assert_eq!(sh.live, Some(photocraft_doc::vector::LiveShape::Rect { rect: [12.0, 14.0, 40.0, 30.0], radii: [1.0, 2.0, 3.0, 4.0] }));
    assert_eq!(sh.stroke.map(|s| s.dashes), Some(vec![4.0, 2.0]));
}

fn drag_from(h: &mut Harness<'_, PhotocraftApp>, from: Pos2, delta: egui::Vec2) {
    h.hover_at(from);
    h.step();
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    for i in 1..=5 {
        h.event(Event::PointerMoved(from + delta * (i as f32 / 5.0)));
        h.step();
    }
    h.event(Event::PointerButton { pos: from + delta, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
}

#[test]
fn shape_geometry_and_stroke_drags_edit_the_layer_and_coalesce() {
    let mut h = harness(240, 160);
    let id =
        LayerId(h.state_mut().run("shape.create", json!({"kind": "rect", "rect": [20, 20, 80, 50], "fill": "#3070c0"})).unwrap()["layer"].as_u64().unwrap());
    h.state_mut().ui.tool = Tool::PathSelection;
    h.run_steps(3);
    let w = h.query_all(egui_kittest::kittest::by().label("W")).map(|n| n.rect()).min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
    assert!(w.top() < 80.0, "the Path Selection options bar");
    let history = h.state().session.active().unwrap().history.past_len();
    drag_from(&mut h, w.center(), vec2(40.0, 0.0));
    let (_, sh) = crate::vector_ui::active_shape(h.state()).unwrap();
    assert!(matches!(sh.live, Some(photocraft_doc::vector::LiveShape::Rect { rect, .. }) if rect[2] > 80.0));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1, "one undo step for the size drag");
    let at = h.get_by_label("Corners").rect().center();
    click_at(&mut h, at);
    let popup = h
        .ctx
        .memory(|m| {
            m.areas().visible_layer_ids().into_iter().filter(|l| l.order == egui::Order::Foreground && l.id != bar_id()).find_map(|l| m.area_rect(l.id))
        })
        .unwrap();
    let corner = h.query_all(egui_kittest::kittest::by().label("Top-left corner radius")).map(|n| n.rect()).find(|r| popup.contains(r.center())).unwrap();
    drag_from(&mut h, corner.center(), vec2(16.0, 0.0));
    assert!(
        matches!(crate::vector_ui::active_shape(h.state()).unwrap().1.live, Some(photocraft_doc::vector::LiveShape::Rect { radii, .. }) if radii[0] > 0.0 && radii[1..] == [0.0, 0.0, 0.0])
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let at = bar_widget(&h, "Set shape stroke width");
    let history = h.state().session.active().unwrap().history.past_len();
    let pos = h.ctx.memory(|m| m.area_rect(bar_id())).unwrap().min;
    // Changing stroke width changes the bounds, but must keep the bar under the held pointer.
    h.hover_at(at);
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    for i in 1..=5 {
        h.event(Event::PointerMoved(at + vec2(i as f32 * 4.0, 0.0)));
        h.step();
        assert_eq!(h.ctx.memory(|m| m.area_rect(bar_id())).unwrap().min, pos);
    }
    h.event(Event::PointerButton { pos: at + vec2(20.0, 0.0), button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    let (_, sh) = crate::vector_ui::active_shape(h.state()).unwrap();
    assert!(sh.stroke.unwrap().width > 0.0);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    let at = bar_widget(&h, "Edit Path");
    click_at(&mut h, at);
    assert_eq!(h.state().ui.tool, Tool::DirectSelection);
    assert_eq!(h.state().ui.selected_path.as_deref(), Some("layer"));
    let at = bar_widget(&h, "Rasterize");
    click_at(&mut h, at);
    assert!(matches!(h.state().session.active().unwrap().doc.layer(id).unwrap().content, LayerContent::Raster(_)));
}

#[test]
fn double_clicking_a_shape_thumbnail_opens_even_collapsed_fill_properties() {
    let mut h = harness(240, 160);
    let id =
        LayerId(h.state_mut().run("shape.create", json!({"kind": "rect", "rect": [20, 20, 80, 50], "fill": "#3070c0"})).unwrap()["layer"].as_u64().unwrap());
    h.state_mut().ui.tool = Tool::Move;
    // Give Appearance a visible hit target: clipped property headers still have accessibility
    // rects, and clicking one through the dock's clip can hit a Layers filter underneath it.
    for group in [crate::dock::Group::Color, crate::dock::Group::Character, crate::dock::Group::Navigator, crate::dock::Group::History] {
        h.state_mut().ui.dock.set_collapsed(group, true);
    }
    h.state_mut().ui.dock.heights.insert(crate::dock::Group::Properties, 400.0);
    h.run_steps(3);
    let at = h.get_by_label("Appearance").rect().center();
    click_at(&mut h, at);
    assert_eq!(h.ctx.data(|d| d.get_temp::<bool>(egui::Id::new(("props-section", "appearance")))), Some(false));
    h.state_mut().ui.dock.set_collapsed(crate::dock::Group::Properties, true);
    crate::dock::reveal(h.state_mut(), crate::dock::Group::Layers);
    h.run_steps(3);
    let rows = crate::layer_row_ui::recorded(&h.ctx);
    let row = rows.iter().find(|r| r.layer == id.0).unwrap_or_else(|| panic!("shape row missing: {rows:?}; dock: {:?}", h.state().ui.dock)).row;
    let thumb = egui::pos2(row.left() + 46.0, row.center().y);
    // Finish the earlier Appearance click sequence before the dock's rearranged row is clicked.
    h.run_steps(40);
    click_at(&mut h, thumb);
    assert!(h.state().ui.dock.is_collapsed(crate::dock::Group::Properties), "the first thumbnail click selects without moving Layers");
    click_at(&mut h, thumb);
    assert!(!h.state().ui.dock.is_collapsed(crate::dock::Group::Properties));
    assert!(h.ctx.data(|d| d.get_temp::<bool>(egui::Id::new(("props-section", "appearance")))).unwrap());
    let popup = h
        .ctx
        .memory(|m| {
            m.areas().visible_layer_ids().into_iter().filter(|l| l.order == egui::Order::Foreground && l.id != bar_id()).find_map(|l| m.area_rect(l.id))
        })
        .expect("thumbnail opened the fill picker");
    let before = crate::vector_ui::active_shape(h.state()).unwrap().1.fill;
    click_at(&mut h, popup.min + vec2(60.0, popup.height() * 0.5));
    assert_ne!(crate::vector_ui::active_shape(h.state()).unwrap().1.fill, before);
}

#[test]
fn studio_shape_properties_are_visible_and_editable() {
    let mut h = harness(240, 160);
    let ctx = h.ctx.clone();
    h.state_mut().set_theme(&ctx, crate::theme::ThemeKind::Studio);
    h.state_mut().run("shape.create", json!({"kind": "polygon", "rect": [20, 20, 80, 50], "sides": 5})).unwrap();
    h.state_mut().ui.tool = Tool::Move;
    h.run_steps(4);
    let sides = h.query_all(egui_kittest::kittest::by().label("Number of sides")).map(|n| n.rect()).max_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
    assert!(sides.top() > 80.0, "the floating Properties card");
    drag_from(&mut h, sides.center(), vec2(12.0, 0.0));
    assert!(matches!(crate::vector_ui::active_shape(h.state()).unwrap().1.live, Some(photocraft_doc::vector::LiveShape::Polygon { sides, .. }) if sides > 5));
}

#[test]
fn selecting_a_text_layer_shows_the_text_context() {
    let mut h = harness(240, 160);
    let id = LayerId(h.state_mut().run("type.create", json!({"text": "Hello", "size": 24, "x": 40, "y": 80})).unwrap()["layer"].as_u64().unwrap());
    h.state_mut().ui.tool = Tool::Move;
    h.run_steps(4);
    assert_eq!(context(h.state()).map(|c| c.0), Some(Context::Text(id)));
    bar_widget(&h, "Edit Text");
    bar_widget(&h, "Set the text color");
    // The Move tool's options bar carries the type options for it.
    assert!(has_layer_options(h.state()));
    let at = bar_widget(&h, "Edit Text");
    click_at(&mut h, at);
    assert_eq!(h.state().ui.text_edit.as_ref().map(|e| e.layer), Some(id.0), "editing its text");
}

/// Drawing a shape (or selecting one) brings Properties forward, where its controls live.
#[test]
fn a_new_shape_reveals_properties() {
    let mut h = harness(240, 160);
    h.state_mut().ui.dock.set_collapsed(crate::dock::Group::Properties, true);
    h.state_mut().ui.dock_tabs.properties = 1;
    h.run_steps(2);
    h.state_mut().run("shape.create", json!({"kind": "ellipse", "rect": [20, 20, 60, 40]})).unwrap();
    h.run_steps(2);
    let app = h.state();
    assert!(!app.ui.dock.is_collapsed(crate::dock::Group::Properties));
    assert!(app.ui.panels.properties);
    assert_eq!(app.ui.dock_tabs.properties, 0);
}
