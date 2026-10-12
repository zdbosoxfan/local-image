//! Gesture replay semantics adapted from VectorCraft crates/tools/src/direct.rs tests,
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092, MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Synthetic geometry and native/GPU oracles are original Local Image tests.

//! #790: paths can be reshaped after they're drawn, as in Photoshop: the Direct Selection tool
//! (A), and the Pen with ⌘/Ctrl (Direct Selection) or ⌥/Alt (Convert Point).

use egui::Modifiers;
use photocraft_doc::vector::Path;
use photocraft_geom::Point;
use serde_json::json;

use super::{Drag, PathRef};
use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, tool_event};
use crate::state::Tool;

fn app(tool: Tool) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width": 200, "height": 200})).unwrap();
    app.sync_views();
    app.ui.tool = tool;
    app.ui.extras.snap = false;
    app
}

/// A square work path; knot 1 at (120, 40) is smooth with horizontal handles.
fn with_path(app: &mut PhotocraftApp) {
    let path = json!({"subpaths": [{"closed": true, "knots": [
        [40, 40],
        {"anchor": [120, 40], "in": [100, 40], "out": [140, 40], "smooth": true},
        [120, 120],
        [40, 120],
    ]}]});
    app.run("path.set", json!({"name": "work", "path": path})).unwrap();
}

fn work(app: &PhotocraftApp) -> Path {
    app.session.active().unwrap().doc.work_path.clone().unwrap()
}

fn anchor(app: &PhotocraftApp, k: usize) -> Point {
    work(app).subpaths[0].knots[k].anchor
}

fn steps(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.entries().len()
}

fn drag(app: &mut PhotocraftApp, from: [f64; 2], to: [f64; 2], mods: Modifiers) {
    tool_event(app, ToolEvent::Down { x: from[0], y: from[1], pressure: 1.0 }, mods);
    let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
    tool_event(app, ToolEvent::Move { x: mid[0], y: mid[1], pressure: 1.0 }, mods);
    tool_event(app, ToolEvent::Move { x: to[0], y: to[1], pressure: 1.0 }, mods);
    tool_event(app, ToolEvent::Up { x: to[0], y: to[1] }, mods);
}

fn click(app: &mut PhotocraftApp, at: [f64; 2], mods: Modifiers) {
    drag(app, at, at, mods);
}

#[test]
fn dragging_an_anchor_moves_it_in_one_undoable_step() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = app(Tool::DirectSelection);
    with_path(&mut app);
    let (before, n) = (work(&app), steps(&app));
    drag(&mut app, [40.5, 120.5], [50.5, 130.5], Modifiers::NONE);
    assert_eq!(anchor(&app, 3), Point::new(50.0, 130.0));
    assert_eq!(anchor(&app, 0), Point::new(40.0, 40.0), "only the dragged anchor moved");
    assert_eq!(steps(&app), n + 1, "one history step for the whole drag");
    assert_eq!((app.ui.direct_selection.target, app.ui.direct_selection.anchors.clone()), (Some(PathRef::Work), vec![[0, 3]]));
    assert!(app.session.undo());
    assert_eq!(work(&app), before);
}

#[test]
fn the_preview_is_what_the_drag_commits() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = app(Tool::DirectSelection);
    with_path(&mut app);
    tool_event(&mut app, ToolEvent::Down { x: 80.0, y: 120.0, pressure: 1.0 }, Modifiers::NONE);
    tool_event(&mut app, ToolEvent::Move { x: 80.0, y: 150.0, pressure: 1.0 }, Modifiers::NONE);
    let Some(Drag::Edit { preview: Some(shown), .. }) = app.ui.direct_selection.drag.clone() else { panic!("a segment drag with a preview") };
    assert_ne!(shown, work(&app), "the document changes only on release");
    tool_event(&mut app, ToolEvent::Up { x: 80.0, y: 150.0 }, Modifiers::NONE);
    assert_eq!(work(&app), shown);
    // The straight bottom segment moved with both its anchors.
    assert_eq!((anchor(&app, 2), anchor(&app, 3)), (Point::new(120.0, 150.0), Point::new(40.0, 150.0)));
}

#[test]
fn a_smooth_points_handles_turn_together_and_alt_breaks_them() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = app(Tool::DirectSelection);
    with_path(&mut app);
    click(&mut app, [120.0, 40.0], Modifiers::NONE);
    assert_eq!(app.ui.direct_selection.anchors, vec![[0, 1]], "a click selects the anchor and shows its handles");
    drag(&mut app, [140.0, 40.0], [120.0, 60.0], Modifiers::NONE);
    let k = work(&app).subpaths[0].knots[1];
    assert_eq!(k.out_ctrl, Point::new(120.0, 60.0));
    assert!((k.in_ctrl.x - 120.0).abs() < 1e-9 && (k.in_ctrl.y - 20.0).abs() < 1e-9, "the opposite handle turned with it: {:?}", k.in_ctrl);
    drag(&mut app, [120.0, 20.0], [100.0, 20.0], Modifiers::ALT);
    let k = work(&app).subpaths[0].knots[1];
    assert_eq!((k.in_ctrl, k.out_ctrl, k.smooth), (Point::new(100.0, 20.0), Point::new(120.0, 60.0), false));
}

#[test]
fn marquee_shift_and_empty_clicks_select_like_photoshop() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = app(Tool::DirectSelection);
    with_path(&mut app);
    drag(&mut app, [100.0, 100.0], [180.0, 180.0], Modifiers::NONE);
    assert_eq!(app.ui.direct_selection.anchors, vec![[0, 2]]);
    click(&mut app, [40.0, 120.0], Modifiers::SHIFT);
    assert_eq!(app.ui.direct_selection.anchors, vec![[0, 2], [0, 3]], "⇧-click adds");
    // Dragging one selected anchor moves them all.
    drag(&mut app, [40.0, 120.0], [40.0, 110.0], Modifiers::NONE);
    assert_eq!((anchor(&app, 2), anchor(&app, 3)), (Point::new(120.0, 110.0), Point::new(40.0, 110.0)));
    click(&mut app, [40.0, 110.0], Modifiers::SHIFT);
    assert_eq!(app.ui.direct_selection.anchors, vec![[0, 2]], "⇧-click on a selected anchor deselects it");
    click(&mut app, [80.0, 80.0], Modifiers::NONE);
    assert!(app.ui.direct_selection.anchors.is_empty(), "a click on empty canvas deselects");
    click(&mut app, [80.0, 40.0], Modifiers::ALT);
    assert_eq!(app.ui.direct_selection.anchors.len(), 4, "⌥-click selects the whole subpath");
}

#[test]
fn pen_with_command_edits_paths_and_ends_the_one_being_drawn() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = app(Tool::Pen);
    for p in [[30.0, 30.0], [90.0, 30.0]] {
        click(&mut app, p, Modifiers::NONE);
    }
    assert_eq!(app.shared_tools.pen.gesture.as_ref().map(|g| g.path().subpaths[0].knots.len()), Some(2));
    // ⌘-drag (Ctrl-drag off the Mac) on the second anchor: the path is finished and edited.
    drag(&mut app, [90.0, 30.0], [90.0, 70.0], Modifiers::COMMAND);
    assert!(app.ui.pen.is_none());
    assert_eq!(app.ui.tool, Tool::Pen, "the Pen stays the tool");
    let wp = work(&app);
    assert_eq!((wp.subpaths[0].closed, wp.subpaths[0].knots[1].anchor), (false, Point::new(90.0, 70.0)));
    // Without ⌘ the Pen draws again.
    click(&mut app, [150.0, 150.0], Modifiers::NONE);
    let draft = app.shared_tools.pen.gesture.as_ref().unwrap().path();
    assert_eq!(draft.subpaths[0], wp.subpaths[0], "the committed work path remains intact");
    assert_eq!(draft.subpaths.len(), 2);
    assert_eq!(draft.subpaths[1].knots.len(), 1);
    assert_eq!(draft.subpaths[1].knots[0].anchor, Point::new(150.0, 150.0));
}

#[test]
fn pen_with_alt_converts_points() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = app(Tool::Pen);
    with_path(&mut app);
    click(&mut app, [120.0, 40.0], Modifiers::ALT);
    assert_eq!(work(&app).subpaths[0].knots[1], photocraft_doc::Knot::corner(120.0, 40.0), "⌥-click makes a corner");
    drag(&mut app, [40.0, 120.0], [40.0, 140.0], Modifiers::ALT);
    let k = work(&app).subpaths[0].knots[3];
    assert_eq!((k.in_ctrl, k.out_ctrl, k.smooth), (Point::new(40.0, 100.0), Point::new(40.0, 140.0), true), "⌥-drag pulls out smooth handles");
    assert!(app.ui.pen.is_none(), "no pen path was started");
    // ⌥ off the path is a plain Pen click.
    click(&mut app, [170.0, 170.0], Modifiers::ALT);
    assert!(app.shared_tools.pen.gesture.is_some());
}

#[test]
fn shape_layer_paths_are_edited_too() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = app(Tool::DirectSelection);
    let id = app.run("shape.create", json!({"kind": "rect", "rect": [20, 20, 60, 60], "fill": "#3070c0"})).unwrap()["layer"].as_u64().unwrap();
    drag(&mut app, [80.0, 80.0], [100.0, 90.0], Modifiers::NONE);
    assert_eq!(app.ui.direct_selection.target, Some(PathRef::Layer(id)));
    let pending = app.ui.direct_selection.pending.take().expect("live conversion prompt");
    app.run("path.node.edit",json!({"targets":pending.targets,"operation":pending.operation,"convertLive":true,"document":pending.capture.document,"revision":pending.capture.revision})).unwrap();
    let info = app.run("shape.info", json!({"layer": id})).unwrap();
    assert_eq!(info["path"]["subpaths"][0]["knots"][2]["anchor"], json!([100.0, 90.0]));
    let b: [i64; 4] = serde_json::from_value(info["bounds"].clone()).unwrap();
    assert!(b[0] + b[2] >= 100 && b[1] + b[3] >= 90, "the shape re-rendered: {b:?}");
}

// Package-owned interaction host: uses real egui pointer/key events and the app canvas,
// with the exact router hooks supplied to 00. This does not stand in for full-app wiring.
type H = egui_kittest::Harness<'static, PhotocraftApp>;
fn node_harness() -> H {
    node_harness_at_scale(1.0)
}
fn node_harness_at_scale(ppp: f32) -> H {
    let mut app = app(Tool::DirectSelection);
    with_path(&mut app);
    app.ui.direct_selection.document = Some(app.session.active().unwrap().doc.id.0);
    app.ui.extras.rulers = false;
    app.ui.extras.snap = false;
    app.ui.views[0].zoom = 1.0;
    app.ui.views[0].center = [100.0, 100.0];
    app.ui.views[0].fit_pending = false;
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_pixels_per_point(ppp).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            super::frame(app, &ctx);
            egui::Panel::top("node-context").show(ui, |ui| super::context_ui(app, ui));
            egui::CentralPanel::default().show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                let v = &app.ui.views[0];
                let xf = crate::canvas::ViewXform { rect, zoom: v.zoom, center: v.center, flip: false };
                super::canvas_input(app, &ctx, 0, &xf, rect);
                let painter = ui.painter_at(rect);
                super::draw_overlay(app, &painter, &|p| xf.to_screen(p[0] as f32, p[1] as f32), egui::Color32::BLUE);
                app.last_canvas_rect = rect;
                ui.allocate_rect(rect, egui::Sense::hover());
            });
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, Default::default());
    h.run_steps(3);
    h
}
fn scr(h: &H, p: [f64; 2]) -> egui::Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    crate::canvas::ViewXform { rect: app.last_canvas_rect, zoom: v.zoom, center: v.center, flip: false }.to_screen(p[0] as f32, p[1] as f32)
}
fn ui_button(h: &mut H, p: [f64; 2], pressed: bool, mods: Modifiers) {
    h.event(egui::Event::ModifiersChanged(mods));
    h.event(egui::Event::PointerButton { pos: scr(h, p), button: egui::PointerButton::Primary, pressed, modifiers: mods });
    h.step();
}
fn ui_move(h: &mut H, p: [f64; 2], mods: Modifiers) {
    h.event(egui::Event::ModifiersChanged(mods));
    h.event(egui::Event::PointerMoved(scr(h, p)));
    h.step();
}
fn ui_drag(h: &mut H, a: [f64; 2], b: [f64; 2], mods: Modifiers) {
    ui_move(h, a, mods);
    ui_button(h, a, true, mods);
    ui_move(h, b, mods);
    ui_button(h, b, false, mods);
}
#[test]
fn node_kittest_marquee_two_of_four_delete_leaves_two_and_undo() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = node_harness();
    let before = work(h.state());
    let n = steps(h.state());
    ui_drag(&mut h, [25.0, 100.0], [135.0, 135.0], Modifiers::NONE);
    assert_eq!(h.state().ui.direct_selection.anchors, vec![[0, 2], [0, 3]]);
    h.key_press(egui::Key::Delete);
    h.step();
    assert_eq!(work(h.state()).subpaths[0].knots.len(), 2);
    assert_eq!(steps(h.state()), n + 1);
    h.state_mut().session.undo();
    assert_eq!(work(h.state()), before);
}
#[test]
fn node_kittest_double_click_adds_exact_curve_node() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = node_harness();
    let before = work(h.state());
    let n = steps(h.state());
    for _ in 0..2 {
        ui_button(&mut h, [80.0, 120.0], true, Modifiers::NONE);
        ui_button(&mut h, [80.0, 120.0], false, Modifiers::NONE);
    }
    assert_eq!(work(h.state()).subpaths[0].knots.len(), 5);
    assert_eq!(steps(h.state()), n + 1);
    h.state_mut().session.undo();
    assert_eq!(work(h.state()), before);
}
#[test]
fn node_kittest_vc_bend_shift_actual_drags_and_escape_detached_preview() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = node_harness();
    let before = work(h.state());
    let journal = h.state().session.journal.len();
    ui_button(&mut h, [80.0, 120.0], true, Modifiers::NONE);
    ui_move(&mut h, [80.0, 140.0], Modifiers::NONE);
    assert_eq!(work(h.state()), before);
    let shown = super::display_doc(h.state(), 0).unwrap();
    assert_ne!(shown.0.work_path.as_ref().unwrap(), &before);
    assert_eq!(h.state().session.journal.len(), journal);
    h.key_press(egui::Key::Escape);
    h.step();
    assert!(super::display_doc(h.state(), 0).is_none());
    ui_button(&mut h, [80.0, 140.0], false, Modifiers::NONE);
    assert_eq!(work(h.state()), before);
    assert_eq!(h.state().session.journal.len(), journal);
    ui_drag(&mut h, [40.0, 120.0], [40.0, 110.0], Modifiers::NONE);
    ui_drag(&mut h, [120.0, 40.0], [120.0, 30.0], Modifiers::SHIFT);
    assert_eq!(h.state().ui.direct_selection.anchors, vec![[0, 1], [0, 2], [0, 3]]);
    assert_eq!(anchor(h.state(), 3), Point::new(40.0, 100.0));
    assert_eq!(h.state().session.journal.len(), journal + 2);
}
#[test]
fn node_kittest_multi_curve_selection_moves_both_in_one_command() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = node_harness();
    let mut ids = Vec::new();
    for x in [20.0, 130.0] {
        let result = h.state_mut().run("shape.create", json!({"kind":"rect","rect":[x,30,30,30],"fill":"#000000"})).unwrap();
        let id = result["layer"].as_u64().unwrap();
        ids.push(id);
        h.state_mut()
            .session
            .edit("plain", |doc, _| {
                if let photocraft_doc::LayerContent::Shape(sh) = &mut doc.layer_mut(photocraft_doc::LayerId(id)).unwrap().content {
                    sh.live = None;
                }
                Ok(())
            })
            .unwrap();
    }
    h.state_mut().session.execute("layer.select", json!({"layer":ids[0]})).unwrap();
    h.state_mut().session.execute("layer.select", json!({"layer":ids[1],"mode":"add"})).unwrap();
    h.state_mut()
        .session
        .edit("shape-only fixture", |doc, _| {
            doc.work_path = None;
            Ok(())
        })
        .unwrap();
    ui_drag(&mut h, [10.0, 20.0], [175.0, 70.0], Modifiers::NONE);
    assert_eq!(h.state().ui.direct_selection.targets().len(), 2);
    let journal = h.state().session.journal.len();
    let n = steps(h.state());
    ui_drag(&mut h, [20.0, 30.0], [25.0, 35.0], Modifiers::NONE);
    for id in ids {
        let doc = &h.state().session.active().unwrap().doc;
        let photocraft_doc::LayerContent::Shape(sh) = &doc.layer(photocraft_doc::LayerId(id)).unwrap().content else { panic!("shape") };
        assert_eq!(sh.path.subpaths[0].knots[0].anchor.y, 35.0);
    }
    assert_eq!(steps(h.state()), n + 1);
    assert_eq!(h.state().session.journal.len(), journal + 1);
}
#[test]
fn node_gesture_staleness_and_tool_switch_cancel_safely() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = app(Tool::DirectSelection);
    with_path(&mut app);
    let before = work(&app);
    super::pointer(&mut app, ToolEvent::Down { x: 40.0, y: 120.0, pressure: 1.0 }, Modifiers::NONE);
    app.ui.tool = Tool::Move;
    super::pointer(&mut app, ToolEvent::Up { x: 50.0, y: 120.0 }, Modifiers::NONE);
    assert_eq!(work(&app), before);
    assert!(app.ui.direct_selection.drag.is_none());
    app.ui.tool = Tool::DirectSelection;
    super::pointer(&mut app, ToolEvent::Down { x: 40.0, y: 120.0, pressure: 1.0 }, Modifiers::NONE);
    let c = app.ui.direct_selection.drag.clone();
    app.session.execute("path.set", json!({"path":{"subpaths":[]}})).unwrap();
    super::pointer(&mut app, ToolEvent::Up { x: 50.0, y: 120.0 }, Modifiers::NONE);
    assert!(work(&app).subpaths.is_empty());
    assert!(c.is_some());
}

#[test]
fn node_kittest_modifier_only_frame_rebuilds_handle_preview_and_release() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = node_harness();
    ui_button(&mut h, [120.0, 40.0], true, Modifiers::NONE);
    ui_button(&mut h, [120.0, 40.0], false, Modifiers::NONE);
    let before = work(h.state());
    let journal = h.state().session.journal.len();
    ui_button(&mut h, [140.0, 40.0], true, Modifiers::NONE);
    ui_move(&mut h, [150.0, 60.0], Modifiers::NONE);
    let shown = super::display_doc(h.state(), 0).unwrap().0;
    let k = shown.work_path.as_ref().unwrap().subpaths[0].knots[1];
    assert!(k.smooth);
    assert!(((k.in_ctrl.x - k.anchor.x) * (k.out_ctrl.y - k.anchor.y) - (k.in_ctrl.y - k.anchor.y) * (k.out_ctrl.x - k.anchor.x)).abs() < 1e-9);
    h.event(egui::Event::ModifiersChanged(Modifiers::ALT));
    h.step();
    let shown = super::display_doc(h.state(), 0).unwrap().0;
    let k = shown.work_path.as_ref().unwrap().subpaths[0].knots[1];
    assert!(!k.smooth);
    assert_eq!(k.in_ctrl, before.subpaths[0].knots[1].in_ctrl);
    assert_eq!(work(h.state()), before);
    ui_button(&mut h, [150.0, 60.0], false, Modifiers::ALT);
    assert_eq!(work(h.state()), shown.work_path.clone().unwrap());
    assert_eq!(h.state().session.journal.len(), journal + 1);
}

fn full_app_node_gpu_harness() -> (H, u64) {
    use eframe::wgpu;
    let mut setup = crate::gpu_canvas::wgpu_setup();
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        let info = adapter.get_info();
        assert_eq!(info.backend, wgpu::Backend::Vulkan);
        assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
        assert!(info.name.contains("RTX 5090"), "{info:?}");
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        eprintln!("node full-app GPU adapter: {info:?}");
    });
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).wgpu_setup(setup).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("required GPU").clone());
        app
    });
    h.state_mut().run("file.new", json!({"width":200,"height":200,"background":"white"})).unwrap();
    h.state_mut().sync_views();
    let id = h.state_mut().run("shape.create", json!({"kind":"rect","rect":[40,40,80,80],"fill":"#000000"})).unwrap()["layer"].as_u64().unwrap();
    h.state_mut()
        .session
        .edit("curve fixture", |doc, _| {
            let photocraft_doc::LayerContent::Shape(sh) = &mut doc.layer_mut(photocraft_doc::LayerId(id)).unwrap().content else { panic!("shape") };
            sh.live = None;
            Ok(())
        })
        .unwrap();
    h.state_mut().ui.tool = Tool::DirectSelection;
    h.state_mut().ui.extras.rulers = false;
    h.state_mut().ui.extras.snap = false;
    h.state_mut().ui.view.show.smart_guides = false;
    h.state_mut().ui.views[0].zoom = 1.0;
    h.state_mut().ui.views[0].center = [100.0, 100.0];
    h.state_mut().ui.views[0].fit_pending = false;
    h.run_steps(4);
    assert!(h.state().gpu_active());
    assert_eq!(h.state().perf.gpu_info.canvas, "gpu");
    (h, id)
}

/// This gate deliberately runs the complete eframe app, using no test-host router injection.
/// 00 removes this dependency ignore after installing the documented hooks in the live app.
#[test]
#[ignore = "requires package 00 frame/canvas_input/display_doc hooks in the live app"]
fn node_full_app_gpu_drag_preview_release_escape_pixels_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let (mut h, id) = full_app_node_gpu_harness();
    let doc = h.state().session.active().unwrap().doc.clone();
    let revision = h.state().session.active().unwrap().revision;
    let n = steps(h.state());
    let journal = h.state().session.journal.len();
    // Full-app input and display hook: the pixel oracle is independent of the preview accessor.
    fn screen(h: &H, p: [f64; 2]) -> egui::Pos2 {
        let app = h.state();
        let v = &app.ui.views[0];
        crate::canvas::ViewXform { rect: crate::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false }
            .to_screen(p[0] as f32, p[1] as f32)
    }
    fn event(h: &mut H, p: [f64; 2], pressed: Option<bool>) {
        let pos = screen(h, p);
        h.event(egui::Event::PointerMoved(pos));
        if let Some(pressed) = pressed {
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        }
        h.step();
    }
    fn pixels(h: &mut H, expected: &photocraft_doc::Document) {
        h.render().expect("live app GPU readback, retaining pointer capture");
        let expected = photocraft_engine::vector_integration::project_document(expected).unwrap();
        let reference = photocraft_compose::flatten(&expected);
        let (_, size, pixels) = h.state().read_canvas_texels(expected.id.0).expect("actual GPU canvas texture");
        assert_eq!(size, [expected.size.width, expected.size.height]);
        assert_eq!(reference.px.len(), pixels.len());
        let max = reference
            .px
            .iter()
            .zip(pixels)
            .flat_map(|(a, b)| {
                (0..4).map(move |c| {
                    assert!(b[c].is_finite());
                    (b[c] - a[c] * if c == 3 { 1. } else { a[3] }).abs()
                })
            })
            .fold(0f32, f32::max);
        assert!(max <= 1. / 255., "live canvas max diff {max}");
    }
    event(&mut h, [120.0, 120.0], Some(true));
    event(&mut h, [140.0, 140.0], None);
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    assert_eq!(h.state().session.active().unwrap().revision, revision);
    let mut expected = (*doc).clone();
    let photocraft_doc::LayerContent::Shape(sh) = &mut expected.layer_mut(photocraft_doc::LayerId(id)).unwrap().content else { panic!("shape") };
    // Independent anchor oracle: original corner (120,120) plus the pointer delta.
    sh.path.subpaths[0].knots[2] = photocraft_doc::Knot::corner(140.0, 140.0);
    let mut shape = sh.clone();
    photocraft_engine::vector_cmds::refresh_shape(&expected, &mut shape);
    expected.layer_mut(photocraft_doc::LayerId(id)).unwrap().content = photocraft_doc::LayerContent::Shape(shape);
    pixels(&mut h, &expected);
    assert_eq!(h.state().session.journal.len(), journal);
    event(&mut h, [140.0, 140.0], Some(false));
    pixels(&mut h, &expected);
    assert_eq!(steps(h.state()), n + 1);
    assert_eq!(h.state().session.journal.len(), journal + 1);
    h.state_mut().session.undo();
    event(&mut h, [120.0, 120.0], Some(true));
    event(&mut h, [145.0, 130.0], None);
    let before = h.state().session.journal.len();
    h.key_press(egui::Key::Escape);
    h.step();
    event(&mut h, [145.0, 130.0], Some(false));
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    assert_eq!(h.state().session.journal.len(), before);
    pixels(&mut h, &doc);
}

#[test]
fn node_kittest_context_smooth_transform_and_alignment() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use egui_kittest::kittest::Queryable;
    let mut h = node_harness();
    ui_drag(&mut h, [20.0, 20.0], [160.0, 160.0], Modifiers::NONE);
    assert_eq!(h.state().ui.direct_selection.anchors.len(), 4);
    h.get_by_label("Smooth").click();
    h.run_steps(2);
    for k in &work(h.state()).subpaths[0].knots {
        let a = Point::new(k.in_ctrl.x - k.anchor.x, k.in_ctrl.y - k.anchor.y);
        let b = Point::new(k.out_ctrl.x - k.anchor.x, k.out_ctrl.y - k.anchor.y);
        assert!((a.x * b.y - a.y * b.x).abs() < 1e-9);
        assert!(k.smooth);
    }
    h.get_by_label("Transform selected nodes").click();
    h.run_steps(2);
    let n = steps(h.state());
    ui_drag(&mut h, [120.0, 120.0], [160.0, 160.0], Modifiers::NONE);
    assert_eq!(anchor(h.state(), 0), Point::new(40.0, 40.0));
    assert_eq!(anchor(h.state(), 2), Point::new(160.0, 160.0));
    assert_eq!(steps(h.state()), n + 1);
    h.get_by_label("Align nodes horizontally").click();
    h.run_steps(2);
    assert!(work(h.state()).subpaths[0].knots.iter().all(|k| k.anchor.y == 100.0));
}

#[test]
fn node_old_ui_settings_load_with_defaults_and_transient_state_is_not_saved() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let old = json!({"target":"work","anchors":[[0,1]],"segment":null});
    let ds: super::DirectSelection = serde_json::from_value(old).unwrap();
    assert!(ds.additional.is_empty());
    assert!(!ds.transform_mode);
    let saved = serde_json::to_value(ds).unwrap();
    assert!(saved.get("drag").is_none());
    assert!(saved.get("preview_doc").is_none());
    assert!(saved.get("pending").is_none());
}

#[test]
fn node_kittest_live_conversion_prompt_cancel_and_accept() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use egui_kittest::kittest::Queryable;
    let mut h = node_harness();
    let id = h.state_mut().run("shape.create", json!({"kind":"rect","rect":[20,20,60,60],"fill":"#000000"})).unwrap()["layer"].as_u64().unwrap();
    let before = h.state().session.active().unwrap().doc.clone();
    let n = steps(h.state());
    ui_drag(&mut h, [80.0, 80.0], [90.0, 90.0], Modifiers::NONE);
    h.run_steps(2);
    assert!(h.state().ui.direct_selection.pending.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert_eq!(steps(h.state()), n);
    assert!(h.state().ui.direct_selection.pending.is_none());
    ui_drag(&mut h, [80.0, 80.0], [90.0, 90.0], Modifiers::NONE);
    h.run_steps(2);
    h.get_by_label("Convert to curves").click();
    h.run_steps(2);
    let doc = &h.state().session.active().unwrap().doc;
    let photocraft_doc::LayerContent::Shape(sh) = &doc.layer(photocraft_doc::LayerId(id)).unwrap().content else { panic!("shape") };
    assert!(sh.live.is_none());
    assert_eq!(sh.path.subpaths[0].knots[2].anchor, Point::new(90.0, 90.0));
    assert_eq!(steps(h.state()), n + 1);
    h.state_mut().session.undo();
    assert_eq!(h.state().session.active().unwrap().doc, before);
}

#[test]
#[ignore = "release real-app benchmark; requires package 00 live node routing"]
// This ignored gate must compile in debug test builds, but reject a debug measurement
// at runtime. A const assertion would prevent the rest of the debug suite from building.
#[allow(clippy::assertions_on_constants)]
fn node_full_app_release_pointer_frame_benchmark_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use std::time::Instant;
    assert!(!cfg!(debug_assertions), "run this gate with --release");
    let (mut h, _) = full_app_node_gpu_harness();
    let mut doc = photocraft_doc::Document::new(
        "Node pointer benchmark",
        photocraft_doc::Size::new(6000, 4000),
        photocraft_doc::ColorMode::Rgb,
        photocraft_doc::SampleType::U8,
    );
    let mut first = None;
    for i in 0..500 {
        let (x, y) = (200.0 + (i % 25) as f64 * 32.0, 200.0 + (i / 25) as f64 * 32.0);
        let mut sh = photocraft_doc::ShapeLayer {
            path: Path::new(vec![photocraft_doc::Subpath::polygon(&[(x, y), (x + 20.0, y), (x + 20.0, y + 20.0), (x, y + 20.0)])]),
            fill: Some(photocraft_doc::Fill::Solid(photocraft_doc::Color::BLACK)),
            ..Default::default()
        };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
        let layer = photocraft_doc::Layer::new("Curve", photocraft_doc::LayerContent::Shape(sh));
        first.get_or_insert(layer.id);
        doc.layers.push(layer);
    }
    h.state_mut().session.add_document(doc, None);
    h.state_mut().sync_views();
    h.state_mut().session.select_layer(first.unwrap()).unwrap();
    let idx = h.state().session.active_index().unwrap();
    h.state_mut().ui.views[idx].center = [400.0, 400.0];
    h.state_mut().ui.views[idx].zoom = 1.0;
    h.state_mut().ui.views[idx].fit_pending = false;
    h.run_steps(4);
    fn position(h: &H, p: [f64; 2]) -> egui::Pos2 {
        let app = h.state();
        let v = &app.ui.views[app.session.active_index().unwrap()];
        crate::canvas::ViewXform { rect: crate::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false }
            .to_screen(p[0] as f32, p[1] as f32)
    }
    let press = position(&h, [220.0, 220.0]);
    h.event(egui::Event::PointerButton { pos: press, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    let mut samples = Vec::new();
    let mut uploads = 0;
    let start = Instant::now();
    for i in 1..=64 {
        let pos = position(&h, [220.0 + (i % 20) as f64, 220.0 + (i % 13) as f64]);
        let at = Instant::now();
        h.event(egui::Event::PointerMoved(pos));
        h.step();
        assert!(super::display_doc(h.state(), idx).is_some(), "real app must execute captured node preview");
        h.render().expect("real GPU/readback");
        samples.push(at.elapsed().as_secs_f64() * 1000.0);
        uploads += h.state().perf.gpu_uploads;
    }
    let end = position(&h, [224.0, 232.0]);
    let at = Instant::now();
    h.event(egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
    h.render().unwrap();
    let release = at.elapsed().as_secs_f64() * 1000.0;
    let first_frame = samples[0];
    samples.sort_by(f64::total_cmp);
    eprintln!(
        "node real-app 6000x4000/500 shapes: adapter={:?}, first={first_frame:.3}ms, median={:.3}ms p95={:.3}ms p99={:.3}ms, release={release:.3}ms, uploads={uploads}, total={:.3}ms",
        h.state().perf.gpu_info,
        samples[32],
        samples[60],
        samples[63],
        start.elapsed().as_secs_f64() * 1000.0
    );
}

#[test]
fn hidpi_node_hit_target_keeps_six_logical_points_at_every_zoom() {
    let _gpu_gate = crate::gpu_test_support::lock();
    for ppp in [1.0, 1.25, 2.0] {
        for zoom in [0.5, 1.0, 2.0] {
            let mut h = node_harness_at_scale(ppp);
            h.state_mut().ui.views[0].zoom = zoom;
            h.run_steps(3);
            let before = work(h.state());
            let outside = [40.0 - 7.0 / f64::from(zoom), 40.0];
            ui_drag(&mut h, outside, outside, Modifiers::NONE);
            assert!(h.state().ui.direct_selection.anchors.is_empty(), "outside six-point target at {ppp}x/{zoom}");
            assert_eq!(work(h.state()), before);
            h.run_steps(20); // A distinct gesture, never a double-click conversion.
            let start = [40.0 - 5.0 / f64::from(zoom), 40.0];
            ui_drag(&mut h, start, [start[0] + 10.0, start[1] + 10.0], Modifiers::NONE);
            assert_eq!(h.state().ui.direct_selection.anchors, vec![[0, 0]], "five-point hit at {ppp}x/{zoom}");
            assert_eq!(anchor(h.state(), 0), Point::new(50.0, 50.0), "drag at {ppp}x/{zoom}");
            assert!(h.state_mut().session.undo());
            assert_eq!(work(h.state()), before);
        }
    }
}
