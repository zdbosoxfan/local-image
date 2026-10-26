#[test]
fn closing_documents_releases_navigator_textures_and_pending_results() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    let ctx = egui::Context::default();
    let textures = ctx.tex_manager();
    let baseline = textures.read().num_allocated();
    for _ in 0..8 {
        app.run("file.new", json!({"width": 2, "height": 2})).unwrap();
        let doc = app.session.active().unwrap().doc.clone();
        let tex = ctx.load_texture("navigator-test", egui::ColorImage::new([32, 32], vec![Color32::WHITE; 32 * 32]), TextureOptions::LINEAR);
        app.navigator_textures.insert(doc.id, (std::sync::Arc::downgrade(&doc), 0, tex));
        #[cfg(not(target_arch = "wasm32"))]
        let pending = {
            let slot = std::sync::Arc::new(std::sync::Mutex::new(Some(egui::ColorImage::new([32, 32], vec![Color32::WHITE; 32 * 32]))));
            let weak = std::sync::Arc::downgrade(&slot);
            app.navigator_jobs.insert(doc.id, (std::sync::Arc::downgrade(&doc), 0, slot, 0.0));
            weak
        };
        app.run("file.close", json!({"discardDocuments": [doc.id.0]})).unwrap();
        app.sync_views();
        assert!(app.navigator_textures.is_empty());
        #[cfg(not(target_arch = "wasm32"))]
        assert!(pending.upgrade().is_none(), "late result ownership must end with the closed tab");
        assert_eq!(textures.read().num_allocated(), baseline);
    }
    // Consume queued headless upload/free deltas without creating a GPU renderer.
    let mut output = ctx.run_ui(Default::default(), |_| {});
    output.textures_delta.clear();
}

#[test]
fn windows_draws_its_own_crosshair() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // #737: Windows' inverting crosshair vanishes over mid-grey; the canvas draws one instead.
    let ctx = egui::Context::default();
    let painter = egui::Painter::new(ctx, egui::LayerId::background(), egui::Rect::EVERYTHING);
    let p = egui::pos2(10.0, 10.0);
    assert_eq!(super::visible_crosshair(egui::CursorIcon::Crosshair, &painter, p, true), egui::CursorIcon::None);
    assert_eq!(super::visible_crosshair(egui::CursorIcon::Crosshair, &painter, p, false), egui::CursorIcon::Crosshair);
    assert_eq!(super::visible_crosshair(egui::CursorIcon::Move, &painter, p, true), egui::CursorIcon::Move);
}

use super::*;

/// #569: displays 1 (sRGB) and 4 (Display P3) side by side, and a document filled with an
/// sRGB colour.
fn app_on_two_displays() -> PhotocraftApp {
    use photocraft_engine::color_cmds::resolve_profile;
    use photocraft_engine::display_color::Display;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 64, "height": 64})).unwrap();
    app.run("edit.fill", json!({"color": "#cc8040"})).unwrap();
    let icc = |id: &str| Some(resolve_profile(id, None, None).unwrap().to_bytes());
    app.session.color.set_displays(Ok(vec![
        Display { compositor_managed: false, id: 1, name: "A".into(), frame: [0.0, 0.0, 100.0, 100.0], profile_name: None, icc: icc("srgb") },
        Display { compositor_managed: false, id: 4, name: "B".into(), frame: [100.0, 0.0, 100.0, 100.0], profile_name: None, icc: icc("display-p3") },
    ]));
    app
}

#[test]
fn cpu_canvas_textures_are_per_display() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // Two windows of one document on different displays: each has its own texture of
    // monitor values, and drawing them in turn re-renders neither.
    let mut app = app_on_two_displays();
    let ctx = egui::Context::default();
    let id = app.session.documents()[0].doc.id;
    let a = ensure_texture(&mut app, &ctx, 0, Some(1)).unwrap().0;
    let b = ensure_texture(&mut app, &ctx, 0, Some(4)).unwrap().0;
    assert_ne!(a, b);
    app.perf.last_refresh = "";
    for _ in 0..3 {
        assert_eq!(ensure_texture(&mut app, &ctx, 0, Some(1)).unwrap().0, a);
        assert_eq!(ensure_texture(&mut app, &ctx, 0, Some(4)).unwrap().0, b);
    }
    assert_eq!(app.perf.last_refresh, "", "no re-render");
    let doc = app.session.documents()[0].doc.clone();
    assert!(canvas_display(&app, &doc, Some(1)).0.unwrap().is_identity());
    assert!(!canvas_display(&app, &doc, Some(4)).0.unwrap().is_identity());
    assert_ne!(app.canvases[&(id, 1)].tex_preview_key, app.canvases[&(id, 4)].tex_preview_key);
    // An edit updates both.
    app.run("edit.fill", json!({"color": "#2060c0"})).unwrap();
    ensure_texture(&mut app, &ctx, 0, Some(1));
    ensure_texture(&mut app, &ctx, 0, Some(4));
    let rev = app.session.documents()[0].revision;
    assert_eq!((app.canvases[&(id, 1)].tex_revision, app.canvases[&(id, 4)].tex_revision), (rev, rev));
}

#[test]
fn gpu_display_luts_are_per_display() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let Ok(rs) = std::panic::catch_unwind(|| egui_kittest::wgpu::create_render_state(crate::gpu_canvas::wgpu_setup(), Default::default())) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let mut app = app_on_two_displays();
    app.set_wgpu(rs);
    let doc = app.session.documents()[0].doc.clone();
    let key = doc.id.0;
    // sRGB on the sRGB display needs no LUT; on the P3 display it does.
    assert_eq!(sync_display_lut(&mut app, &doc, key, Some(1)), 0);
    assert_eq!(sync_display_lut(&mut app, &doc, key, Some(4)), 1);
    let gpu = app.gpu.clone().unwrap();
    let (s1, s4) = (gpu.display_lut_signature(key, 1).unwrap(), gpu.display_lut_signature(key, 4).unwrap());
    assert_ne!(s1.0, s4.0);
    assert!(gpu.has_display_lut(key, 4) && !gpu.has_display_lut(key, 1));
    // Drawing one window leaves the other's LUT alone.
    for _ in 0..3 {
        assert_eq!(sync_display_lut(&mut app, &doc, key, Some(1)), 0);
        assert_eq!(sync_display_lut(&mut app, &doc, key, Some(4)), 1);
    }
    assert_eq!((gpu.display_lut_signature(key, 1), gpu.display_lut_signature(key, 4)), (Some(s1), Some(s4)));
    assert!(gpu.has_display_lut(key, 4));
    // The shared texture's key is the same for both displays.
    assert_eq!(texture_key(canvas_display(&app, &doc, Some(1)).0.as_deref()), texture_key(canvas_display(&app, &doc, Some(4)).0.as_deref()));
}

#[test]
fn tab_context_uses_clicked_document_and_existing_close_commands() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let items = tab_context_items(2, 3);
    assert_eq!(items.iter().map(|(_, id, _, _)| *id).collect::<Vec<_>>(), ["file.close", "file.closeOthers", "file.closeAll"]);
    assert_eq!(items[0].2, json!({"document": 2}));
    assert_eq!(items[1].2, json!({"document": 2}));
    assert_eq!(items[2].2, json!({}));
    assert!(!tab_context_items(0, 1)[1].3);
    assert!(items.iter().all(|(_, id, _, _)| photocraft_engine::commands::find(id).is_some()));
}

/// Photoshop's document tab × is after the title on Windows and Linux, before it on macOS (#619).
#[test]
fn document_tab_close_button_sits_on_the_platform_side() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use egui::os::OperatingSystem as Os;
    use egui_kittest::kittest::Queryable;
    for (os, after_title) in [(Os::Windows, true), (Os::Nix, true), (Os::Mac, false)] {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
        app.sync_views();
        // Drawn from the second frame, once the Pro theme is set.
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(600.0, 40.0)).build_ui_state(
            |ui, (app, ready): &mut (PhotocraftApp, bool)| {
                if *ready {
                    tabs(app, ui);
                }
            },
            (app, false),
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
        h.ctx.set_os(os);
        h.state_mut().1 = true;
        h.run_steps(2);
        let tab = h.get_by_label_contains("Untitled @").rect();
        let x = h.get_by_label("Close").rect();
        assert!(tab.contains_rect(x), "{os:?}: the × is inside its tab");
        assert_eq!(x.center().x > tab.center().x, after_title, "{os:?}: × at {x:?} in tab {tab:?}");
        h.get_by_label("Close").click();
        h.run_steps(2);
        assert!(h.state().0.session.documents().is_empty(), "{os:?}: the × closes the document");
    }
}

#[test]
fn tab_close_others_prompts_for_unsaved_nonactive_document() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 8, "height": 8, "name": "Keep"})).unwrap();
    app.run("file.new", json!({"width": 8, "height": 8, "name": "Edited"})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    assert_eq!(app.session.active_index(), Some(1));
    let (_, id, params, _) = tab_context_items(0, 2)[1].clone();
    crate::menus::invoke(&mut app, &egui::Context::default(), id, params).unwrap();
    assert!(app.discard.is_some(), "close others must ask before discarding the edited tab");
    assert_eq!(app.session.documents().len(), 2);
}

#[test]
fn wayland_start_screen_hint_does_not_claim_file_drop_works() {
    let _gpu_gate = crate::gpu_test_support::lock();
    assert_ne!(start_screen_drop_hint(true), start_screen_drop_hint(false));
}

#[test]
fn pointer_moves_are_bounded_by_the_press_and_release() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use egui::{Event, PointerButton, pos2};
    let button = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    let events = [
        Event::PointerMoved(pos2(1.0, 0.0)),
        button(pos2(2.0, 0.0), true),
        Event::PointerMoved(pos2(3.0, 0.0)),
        Event::PointerMoved(pos2(4.0, 0.0)),
        button(pos2(5.0, 0.0), false),
        Event::PointerMoved(pos2(6.0, 0.0)),
    ];
    // The press is in this frame: only the moves between press and release count.
    assert_eq!(pointer_moves(&events, PointerButton::Primary, false), vec![pos2(3.0, 0.0), pos2(4.0, 0.0)]);
    // The button was already down when the frame began: the move before the release counts,
    // the one after it does not.
    assert_eq!(pointer_moves(&events, PointerButton::Primary, true), vec![pos2(1.0, 0.0), pos2(3.0, 0.0), pos2(4.0, 0.0)]);
}

#[test]
fn freehand_tools_are_the_ones_that_follow_a_path() {
    let _gpu_gate = crate::gpu_test_support::lock();
    for t in [
        Tool::Brush,
        Tool::Pencil,
        Tool::MixerBrush,
        Tool::Eraser,
        Tool::BackgroundEraser,
        Tool::CloneStamp,
        Tool::Smudge,
        Tool::Dodge,
        Tool::Lasso,
        Tool::QuickSelection,
    ] {
        assert!(freehand_tool(t), "{t:?} paints or retouches along a path");
    }
    for t in [Tool::Move, Tool::Eyedropper, Tool::Gradient, Tool::Crop, Tool::RectMarquee, Tool::Type, Tool::Hand] {
        assert!(!freehand_tool(t), "{t:?} is driven by the pointer's latest position");
    }
}

#[test]
fn clone_stamp_centre_appears_only_with_option() {
    let _gpu_gate = crate::gpu_test_support::lock();
    assert!(!brush_tip_centre(Tool::CloneStamp, false, false, 20.0));
    assert!(brush_tip_centre(Tool::CloneStamp, true, false, 20.0));
    assert!(brush_tip_centre(Tool::Healing, true, false, 20.0));
    assert!(!brush_tip_centre(Tool::Healing, false, false, 20.0));
    assert!(brush_tip_centre(Tool::CloneStamp, false, true, 20.0));
    assert!(brush_tip_centre(Tool::Brush, false, false, 20.0));
    assert!(!brush_tip_centre(Tool::QuickSelection, false, false, 20.0));
    assert!(brush_tip_centre(Tool::BackgroundEraser, false, false, 2.0));
}

#[test]
fn view_transform_roundtrip() {
    let _gpu_gate = crate::gpu_test_support::lock();
    for flip in [false, true] {
        let xf = ViewXform { rect: Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 600.0)), zoom: 2.5, center: [320.0, 240.0], flip, rotation: 0.0 };
        let s = xf.to_screen(10.0, 20.0);
        let d = xf.to_doc(s);
        assert!((d[0] - 10.0).abs() < 1e-3 && (d[1] - 20.0).abs() < 1e-3);
        assert_eq!(xf.to_screen(320.0, 240.0), xf.rect.center());
        // Flipped, document x grows to the left.
        assert_eq!(xf.to_screen(330.0, 240.0).x > xf.rect.center().x, !flip);
        let r = xf.doc_rect(DRect::new(0, 0, 10, 10));
        assert!(r.width() > 0.0 && r.height() > 0.0);
    }
}

#[test]
fn eyedropper_drag_updates_the_sampled_colour() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 40, "height": 20, "background": "transparent"})).unwrap();
    app.run("shape.create", json!({"kind": "rect", "rect": [0, 0, 20, 20], "fill": "#ff0000"})).unwrap();
    app.run("shape.create", json!({"kind": "rect", "rect": [20, 0, 20, 20], "fill": "#00ff00"})).unwrap();
    app.ui.tool = Tool::Eyedropper;

    tool_event(&mut app, ToolEvent::Down { x: 10.0, y: 10.0, pressure: 1.0 }, egui::Modifiers::NONE);
    assert!(app.session.tools.foreground[0] > 0.99 && app.session.tools.foreground[1] < 0.01);

    tool_event(&mut app, ToolEvent::Move { x: 30.0, y: 10.0, pressure: 1.0 }, egui::Modifiers::NONE);
    assert!(app.session.tools.foreground[1] > 0.99 && app.session.tools.foreground[0] < 0.01);
}

#[test]
fn alt_with_a_painting_tool_samples_the_foreground_instead_of_painting() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 40, "height": 20, "background": "transparent"})).unwrap();
    app.run("shape.create", json!({"kind": "rect", "rect": [0, 0, 20, 20], "fill": "#ff0000"})).unwrap();
    app.run("shape.create", json!({"kind": "rect", "rect": [20, 0, 20, 20], "fill": "#00ff00"})).unwrap();
    let alt = egui::Modifiers::ALT;
    for tool in [Tool::Brush, Tool::Pencil, Tool::Gradient, Tool::PaintBucket] {
        app.ui.tool = tool;
        app.run("tools.setColors", json!({"foreground": [0.0, 0.0, 1.0, 1.0], "background": [1.0, 1.0, 1.0, 1.0]})).unwrap();
        let rev = app.session.active().unwrap().revision;
        tool_event(&mut app, ToolEvent::Down { x: 10.0, y: 10.0, pressure: 1.0 }, alt);
        assert_eq!(app.session.tools.foreground, [1.0, 0.0, 0.0, 1.0], "{tool:?}: ⌥-click sets the foreground");
        // A drag keeps sampling, even after ⌥ is let go; the background is untouched.
        tool_event(&mut app, ToolEvent::Move { x: 30.0, y: 10.0, pressure: 1.0 }, egui::Modifiers::NONE);
        assert_eq!(app.session.tools.foreground, [0.0, 1.0, 0.0, 1.0], "{tool:?}");
        tool_event(&mut app, ToolEvent::Up { x: 30.0, y: 10.0 }, egui::Modifiers::NONE);
        assert_eq!(app.session.tools.background, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(app.session.active().unwrap().revision, rev, "{tool:?}: sampling must not edit the document");
        assert!(app.drag.is_none() && !app.alt_sampling);
    }
    // Agents get the same through `ui.pointer` (MCP `ui_pointer`).
    app.ui.tool = Tool::Brush;
    let ctx = egui::Context::default();
    let events = json!([{"kind": "down", "x": 10, "y": 10}, {"kind": "up", "x": 10, "y": 10}]);
    let (req, _rx) = crate::control::ControlRequest::new("ui.pointer", json!({"modifiers": {"alt": true}, "events": events}));
    let _ = crate::control::handle(&mut app, &ctx, &req);
    assert_eq!(app.session.tools.foreground, [1.0, 0.0, 0.0, 1.0]);
    app.run("tools.setColors", json!({"foreground": [0.0, 1.0, 0.0, 1.0]})).unwrap();
    // Without ⌥ the Brush paints again, and ⌥ pressed mid-stroke doesn't switch to sampling.
    app.run("layer.new.layer", json!({})).unwrap();
    let rev = app.session.active().unwrap().revision;
    tool_event(&mut app, ToolEvent::Down { x: 5.0, y: 5.0, pressure: 1.0 }, egui::Modifiers::NONE);
    tool_event(&mut app, ToolEvent::Move { x: 30.0, y: 5.0, pressure: 1.0 }, alt);
    tool_event(&mut app, ToolEvent::Up { x: 30.0, y: 5.0 }, alt);
    assert_eq!(app.session.tools.foreground, [0.0, 1.0, 0.0, 1.0]);
    assert!(app.session.active().unwrap().revision > rev, "the stroke was painted");
    // Control+Alt stays the brush-resize gesture, never a sample.
    assert!(!alt_samples(Tool::Brush, egui::Modifiers { alt: true, ctrl: true, ..Default::default() }));
    assert!(!alt_samples(Tool::Eraser, alt), "⌥ with the Eraser is not the Eyedropper");
}

#[test]
fn brush_drag_shows_the_real_stroke_and_commits_it() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // The drag used to draw a hard, flat stand-in and only showed the soft brush on release.
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 120, "height": 60, "background": "transparent"})).unwrap();
    app.run("tools.setBrush", json!({"brush": {"size": 20, "hardness": 0.0}})).unwrap();
    app.ui.tool = Tool::Brush;
    let m = egui::Modifiers::NONE;
    let alpha = |d: &Document, x, y| d.layers[0].surface().unwrap().rgba(x, y)[3];
    let rev = app.session.documents()[0].revision;
    tool_event(&mut app, ToolEvent::Down { x: 10.0, y: 30.0, pressure: 1.0 }, m);
    for x in [40.0, 70.0, 100.0] {
        tool_event(&mut app, ToolEvent::Move { x, y: 30.0, pressure: 1.0 }, m);
    }
    let (shown, key) = display_doc(&mut app, &egui::Context::default(), 0);
    assert_ne!(key, 0);
    // A partial value near the edge: the soft tip (which reaches past its radius), not a hard
    // flat stand-in.
    assert!(alpha(&shown, 40, 30) > 0.5 && (0.01..0.95).contains(&alpha(&shown, 40, 38)), "soft stroke while drawing");
    assert_eq!(alpha(&app.session.documents()[0].doc, 40, 30), 0.0, "not committed yet");
    // The canvas redraws only what the stroke touched.
    let dk = canvas_display(&app, &app.session.documents()[0].doc, None).1;
    let d = damage_since(&app, 0, (rev, dk), (rev, key), dk, None).unwrap();
    // (The soft tip's reach takes the stroke's ends to the canvas edges, so only its height is bounded.)
    assert!(d.contains(40, 30) && !d.contains(40, 2) && d.height() < 60, "{d:?}");
    tool_event(&mut app, ToolEvent::Up { x: 100.0, y: 30.0 }, m);
    let doc = app.session.documents()[0].doc.clone();
    assert!(app.live_stroke.is_none() && display_doc(&mut app, &egui::Context::default(), 0).1 == 0);
    assert_eq!((alpha(&doc, 40, 30), alpha(&doc, 40, 38)), (alpha(&shown, 40, 30), alpha(&shown, 40, 38)), "commit matches the preview");
}

#[test]
fn long_brush_drag_bounds_damage_and_reuses_the_commit() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 512, "height": 64, "background": "transparent"})).unwrap();
    app.run("tools.setBrush", json!({"size": 4, "hardness": 1.0, "smoothing": {"amount": 0.0}})).unwrap();
    app.ui.tool = Tool::Brush;
    let m = egui::Modifiers::NONE;
    tool_event(&mut app, ToolEvent::Down { x: 10.0, y: 32.0, pressure: 1.0 }, m);
    let first = app.live_stroke.as_ref().unwrap().display_key();
    let mut damage = app.live_stroke.as_ref().unwrap().damage_bounds;
    for i in 1..400 {
        tool_event(&mut app, ToolEvent::Move { x: 10.0 + i as f64, y: 32.0, pressure: 1.0 }, m);
        let live = app.live_stroke.as_ref().unwrap();
        assert!(live.damage.len() <= 128);
        damage = damage.union(live.damage.last().unwrap());
    }
    let live = app.live_stroke.as_ref().unwrap();
    assert!(live.since(first).unwrap().contains(200, 32), "old views catch up to the whole stroke");
    assert!(live.since(first).unwrap().contains_rect(&damage));
    assert_eq!(live.since(live.display_key()), Some(DRect::EMPTY));
    let shown = live.stroke.doc.layers[0].surface().unwrap().clone();
    tool_event(&mut app, ToolEvent::Up { x: 409.0, y: 32.0 }, m);
    assert_eq!(app.session.live_commits_reused(), 1);
    assert_eq!(&shown, app.session.active().unwrap().doc.layers[0].surface().unwrap());
}

/// A device that allows smaller textures than MAX_TEXTURE (2048 on some older GPUs and WebGL
/// contexts): the canvas texture is reduced to fit it. A 2600 px document was uploaded at full
/// size, which egui asserts on and the backend can't create.
#[test]
fn cpu_canvas_texture_fits_the_device_limit() {
    let _gpu_gate = crate::gpu_test_support::lock();
    for (side, (w, h), want) in [(2048, (2600, 1700), [1300, 850]), (2048, (5000, 2000), [1666, 666]), (8192, (5000, 2000), [2500, 1000])] {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": w, "height": h})).unwrap();
        let ctx = egui::Context::default();
        ctx.input_mut(|i| i.max_texture_side = side);
        let (tex, _) = ensure_texture(&mut app, &ctx, 0, None).unwrap();
        let mut delta = ctx.tex_manager().write().take_delta();
        let sizes: Vec<_> = delta.set.get(&tex).into_iter().flatten().map(|d| d.image.size()).collect();
        delta.clear();
        assert_eq!(sizes, vec![want], "{w}x{h} on a {side} px device");
    }
}

#[test]
fn live_stroke_uses_pen_tilt_and_updates_a_reduced_texture_partially() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // Pen tilt drives the size; the preview must use the tilt the commit gets. The document
    // is wider than MAX_TEXTURE, so the CPU texture is reduced (factor 2) and each step
    // must update only the reduced pixels it touched.
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    let ctx = egui::Context::default();
    ctx.input_mut(|i| i.max_texture_side = 8192);
    app.run("file.new", json!({"width": 4097, "height": 90, "background": "transparent"})).unwrap();
    let tilt = json!({"size": 30, "hardness": 1.0, "spacing": 0.05, "shapeDynamics": {"enabled": true, "size": {"control": "penTilt"}}});
    app.run("tools.setBrush", json!({ "brush": tilt })).unwrap();
    app.ui.tool = Tool::Brush;
    assert_eq!(ensure_texture(&mut app, &ctx, 0, None).map(|t| t.1), Some(0.5));
    let m = egui::Modifiers::NONE;
    app.stylus.feed.set(Some(crate::stylus::PenSample { pressure: 1.0, tilt_x: 60.0, tilt_y: 0.0, rotation: 0.0, eraser: false }));
    tool_event(&mut app, ToolEvent::Down { x: 100.0, y: 45.0, pressure: 1.0 }, m);
    for x in [300.0, 600.0, 900.0] {
        tool_event(&mut app, ToolEvent::Move { x, y: 45.0, pressure: 1.0 }, m);
        ensure_texture(&mut app, &ctx, 0, None);
        assert_eq!(app.perf.last_refresh, "rect");
        assert!(app.perf.last_refresh_px < 4097 * 90 / 4, "{}", app.perf.last_refresh_px);
    }
    let shown = display_doc(&mut app, &egui::Context::default(), 0).0;
    tool_event(&mut app, ToolEvent::Up { x: 900.0, y: 45.0 }, m);
    ensure_texture(&mut app, &ctx, 0, None);
    assert_eq!(app.perf.last_refresh, "rect", "the commit refreshes only the stroke");
    let doc = app.session.documents()[0].doc.clone();
    let (a, b) = (doc.layers[0].surface().unwrap(), shown.layers[0].surface().unwrap());
    // 60° tilt shrinks the dab well below 30 px: the preview shows that size, as committed.
    assert!(a.rgba(500, 45)[3] > 0.5 && a.rgba(500, 45 + 12)[3] == 0.0);
    // (The smoothed tail catches up to the end point only when the stroke finishes.)
    assert!((0..90).all(|y| (0..700).all(|x| a.rgba(x, y) == b.rgba(x, y))), "commit matches the preview");
}

/// Brush drag along y = 40 with one canvas frame per pointer move; returns the document the
/// canvas showed at the last move, the committed one, and whether the release refreshed only
/// the stroke's rectangle.
fn drag_frames(smoothing: f32, xs: &[f64]) -> (std::sync::Arc<Document>, std::sync::Arc<Document>, bool, PhotocraftApp) {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    let ctx = egui::Context::default();
    app.run("file.new", json!({"width": 200, "height": 80, "background": "transparent"})).unwrap();
    app.run("tools.setBrush", json!({"brush": {"size": 16, "hardness": 0.5}})).unwrap();
    // What the options bar's Smoothing field writes.
    app.session.tools.brush.smoothing.amount = smoothing;
    app.ui.tool = Tool::Brush;
    ensure_texture(&mut app, &ctx, 0, None);
    let m = egui::Modifiers::NONE;
    tool_event(&mut app, ToolEvent::Down { x: 10.0, y: 40.0, pressure: 1.0 }, m);
    for &x in xs {
        tool_event(&mut app, ToolEvent::Move { x, y: 40.0 + (x / 7.0).sin() * 8.0, pressure: 1.0 }, m);
        ensure_texture(&mut app, &ctx, 0, None);
    }
    let live = display_doc(&mut app, &egui::Context::default(), 0).0;
    let last = *xs.last().unwrap();
    tool_event(&mut app, ToolEvent::Up { x: last, y: 40.0 + (last / 7.0).sin() * 8.0 }, m);
    let (next, key) = display_doc(&mut app, &egui::Context::default(), 0);
    assert_eq!(key, 0, "the frame after release shows the committed document");
    ensure_texture(&mut app, &ctx, 0, None);
    let partial = app.perf.last_refresh == "rect";
    (live, next, partial, app)
}

fn same_pixels(a: &Document, b: &Document) -> bool {
    let (a, b) = (a.layers[0].surface().unwrap(), b.layers[0].surface().unwrap());
    (0..80).all(|y| (0..200).all(|x| a.rgba(x, y) == b.rgba(x, y)))
}

#[test]
fn release_shows_nothing_new_without_smoothing() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // #73: at 0 % the stroke follows the pointer; the last frame drawn while dragging is
    // exactly the committed stroke, so release changes nothing on screen.
    let (live, done, partial, app) = drag_frames(0.0, &[40.0, 70.0, 100.0, 130.0, 160.0]);
    assert!(same_pixels(&live, &done), "preview at the last move = committed stroke");
    assert!(partial, "the handover refreshes only the stroke");
    let p = app.session.journal.iter().rev().find(|(id, _)| id == "paint.stroke").map(|(_, p)| p.clone()).unwrap();
    assert!(p.get("smoothing").is_none(), "the commit uses the session brush's smoothing, not a hard-coded one");
}

#[test]
fn smoothed_stroke_shows_its_catch_up_tail_while_drawing() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // #73: with smoothing the brush lags behind the pointer and catches up at the end; the
    // preview draws that tail live, so no frame after release is missing the end.
    let xs = [30.0, 50.0, 70.0, 90.0, 110.0, 130.0, 150.0, 170.0];
    let (live, done, partial, _) = drag_frames(0.5, &xs);
    assert!(same_pixels(&live, &done), "preview at the last move = committed stroke, tail included");
    assert!(partial);
    let end = |d: &Document| (0..200).rev().find(|&x| (0..80).any(|y| d.layers[0].surface().unwrap().rgba(x, y)[3] > 0.0)).unwrap();
    assert!(end(&live) >= 170, "the end reaches the pointer while drawing ({})", end(&live));
    // The options-bar value reaches the stroke: 50 % smooths the wiggle, 0 % doesn't.
    let (_, rough, _, _) = drag_frames(0.0, &xs);
    assert!(!same_pixels(&rough, &done), "smoothing changes the stroke");
    // And the commit is what `paint.stroke` gives with that smoothing.
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 200, "height": 80, "background": "transparent"})).unwrap();
    s.execute("tools.setBrush", json!({"brush": {"size": 16, "hardness": 0.5, "smoothing": {"amount": 0.5}}})).unwrap();
    let mut pts = vec![json!([10.0, 40.0, 1.0])];
    pts.extend(xs.iter().map(|&x| json!([x, 40.0 + (x / 7.0).sin() * 8.0, 1.0])));
    // (Round tips without dynamics draw the same for any seed; canvas strokes are freehand.)
    s.execute("paint.stroke", json!({"points": pts, "seed": 0, "freehand": true})).unwrap();
    assert!(same_pixels(&s.active().unwrap().doc, &done));
}

#[test]
fn smoothing_preview_redraws_the_tail_each_step() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // The tail drawn at one step must not linger once the brush moves on: a sharp turn would
    // leave a stale tail behind if it weren't restored.
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 200, "height": 120, "background": "transparent"})).unwrap();
    app.run("tools.setBrush", json!({"brush": {"size": 10, "hardness": 1.0, "smoothing": {"amount": 0.6}}})).unwrap();
    app.ui.tool = Tool::Brush;
    let m = egui::Modifiers::NONE;
    tool_event(&mut app, ToolEvent::Down { x: 10.0, y: 20.0, pressure: 1.0 }, m);
    for (x, y) in [(60.0, 20.0), (110.0, 20.0), (110.0, 70.0), (110.0, 110.0), (60.0, 110.0)] {
        tool_event(&mut app, ToolEvent::Move { x, y, pressure: 1.0 }, m);
    }
    let live = display_doc(&mut app, &egui::Context::default(), 0).0;
    tool_event(&mut app, ToolEvent::Up { x: 60.0, y: 110.0 }, m);
    let done = app.session.documents()[0].doc.clone();
    let (a, b) = (live.layers[0].surface().unwrap(), done.layers[0].surface().unwrap());
    assert!((0..120).all(|y| (0..200).all(|x| a.rgba(x, y) == b.rgba(x, y))), "no stale tails in the preview");
}

/// Shapes the drag preview paints for `app` this frame.
fn drag_preview_shapes(app: &mut PhotocraftApp, ctx: &egui::Context, zoom: f32) -> Vec<egui::epaint::ClippedShape> {
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let ctx = ui.ctx();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let painter = ctx.layer_painter(egui::LayerId::background()).with_clip_rect(rect);
        let xf = ViewXform { rect, zoom, center: [60.0, 30.0], flip: false, rotation: 0.0 };
        draw_drag_preview(app, &painter, &xf);
    });
    let egui::FullOutput { mut textures_delta, shapes, .. } = out;
    textures_delta.clear();
    shapes
}

#[test]
fn brush_drags_paint_no_stand_in_shape_over_the_canvas() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // #189: v0.1 drew the stroke being dragged as a foreground-coloured egui polyline as wide
    // as the brush; zoomed in, egui tessellated it into hard black wedges fanning out from the
    // start. The canvas shows the real stroke (`LiveStroke`) and nothing is drawn over it.
    let ctx = egui::Context::default();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 120, "height": 60})).unwrap();
    app.run("tools.setBrush", json!({"brush": {"size": 30, "hardness": 0.0}})).unwrap();
    for tool in [Tool::Brush, Tool::Eraser] {
        app.ui.tool = tool;
        let idle = drag_preview_shapes(&mut app, &ctx, 12.0).len();
        tool_event(&mut app, ToolEvent::Down { x: 20.0, y: 30.0, pressure: 1.0 }, egui::Modifiers::NONE);
        for i in 1..40 {
            let t = f64::from(i);
            tool_event(&mut app, ToolEvent::Move { x: 20.0 + t, y: 30.0 + (t / 3.0).sin() * 4.0, pressure: 1.0 }, egui::Modifiers::NONE);
        }
        assert!(app.live_stroke.is_some(), "{tool:?}: the canvas shows the live stroke");
        assert_eq!(drag_preview_shapes(&mut app, &ctx, 12.0).len(), idle, "{tool:?}: no overlay while dragging");
        tool_event(&mut app, ToolEvent::Up { x: 59.0, y: 30.0 }, egui::Modifiers::NONE);
    }
}

#[test]
fn retouch_drags_show_the_footprint_without_wedges() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // The retouching tools' trail was the same brush-wide polyline (#189): now a mask of the
    // footprint, with nothing outside the brush radius of the path, however small the steps.
    let ctx = egui::Context::default();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 120, "height": 60})).unwrap();
    app.run("tools.setBrush", json!({"brush": {"size": 30}})).unwrap();
    app.ui.tool = Tool::Dodge;
    tool_event(&mut app, ToolEvent::Down { x: 20.0, y: 30.0, pressure: 1.0 }, egui::Modifiers::NONE);
    let mut pts = vec![[20.0, 30.0]];
    for i in 1..80 {
        let t = f64::from(i) * 0.5;
        let p = [20.0 + t, 30.0 + (t / 2.0).sin() * 3.0];
        tool_event(&mut app, ToolEvent::Move { x: p[0], y: p[1], pressure: 1.0 }, egui::Modifiers::NONE);
        pts.push(p);
        if i % 20 == 0 {
            // One mesh for the trail, however long the drag.
            let shapes = drag_preview_shapes(&mut app, &ctx, 12.0);
            assert!(!shapes.iter().any(|s| matches!(s.shape, egui::Shape::Path(_))), "no polyline");
        }
    }
    drag_preview_shapes(&mut app, &ctx, 12.0);
    let trail = app.trail.as_ref().unwrap();
    assert_eq!(trail.scale(), 1.0);
    for y in 0..60 {
        for x in 0..120 {
            let d = pts.iter().map(|p| (x as f64 + 0.5 - p[0]).hypot(y as f64 + 0.5 - p[1])).fold(f64::MAX, f64::min);
            if d < 14.0 {
                assert_eq!(trail.coverage(x, y), 255, "({x}, {y}) inside the footprint");
            } else if d > 16.0 {
                assert_eq!(trail.coverage(x, y), 0, "({x}, {y}) outside the footprint");
            }
        }
    }
    tool_event(&mut app, ToolEvent::Up { x: 59.5, y: 30.0 }, egui::Modifiers::NONE);
    drag_preview_shapes(&mut app, &ctx, 12.0);
    assert!(app.trail.is_none(), "the trail ends with the drag");
}

#[test]
fn layer_style_dialog_previews_live_and_cancel_restores() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 16, "height": 16})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    let baseline_key = display_doc(&mut app, &egui::Context::default(), 0).1;
    let fx = |d: &Document| d.layers.iter().map(|l| l.effects.items.len()).sum::<usize>();
    let id = crate::layer_style::open(&mut app, Some("colorOverlay")).unwrap();
    let (shown, key) = display_doc(&mut app, &egui::Context::default(), 0);
    assert_eq!((fx(&shown), fx(&app.session.documents()[0].doc)), (1, 0), "previewed, not committed");
    // Adding a stroke through the dialog's instance list re-renders the canvas.
    if let Some(d) = app.ui.dialog_mut(id) {
        d.fields["effects"].as_array_mut().unwrap().push(json!({"id": "fx9", "kind": "stroke", "on": true, "params": {"size": 3}}));
    }
    let (shown, key2) = display_doc(&mut app, &egui::Context::default(), 0);
    assert_eq!(fx(&shown), 2);
    assert_ne!(key, key2, "an edit re-renders the canvas");
    app.ui.close_dialog(id);
    let (shown, key) = display_doc(&mut app, &egui::Context::default(), 0);
    assert_eq!((fx(&shown), key), (0, baseline_key));
    assert!(app.style_preview.is_none());
}

#[test]
fn zoom_steps_monotone() {
    let _gpu_gate = crate::gpu_test_support::lock();
    assert_eq!(zoom_step(1.0, 1), 2.0);
    assert_eq!(zoom_step(1.0, -1), 0.6667);
    assert_eq!(zoom_step(0.4, 1), 0.5);
    assert_eq!(zoom_step(32.0, 1), 32.0);
}

#[test]
fn damage_grows_by_nested_effect_reach() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use photocraft_doc::{Effect, Layer};
    let fmt = photocraft_color::PixelFormat::RGBA8;
    assert_eq!(effect_reach(&[Layer::raster("plain", fmt)]), 0);
    let mut inner = Layer::raster("inner", fmt);
    inner.effects.items.push(Effect::default_drop_shadow());
    let m = photocraft_compose::effects::margin(&inner);
    let mut group = Layer::group("g", vec![inner.clone()]);
    group.effects.items.push(Effect::default_drop_shadow());
    assert_eq!(effect_reach(std::slice::from_ref(&inner)), m);
    // A child's edit moves the group's shape, whose effects reach further.
    assert_eq!(effect_reach(&[group.clone()]), 2 * m);
    group.visible = false;
    assert_eq!(effect_reach(&[group]), 0);
}

#[cfg(test)]
mod transform_controls_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn move_transform_controls_start_free_transform_for_vector_shapes() {
        let _gpu_gate = crate::gpu_test_support::lock();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.execute("file.new", json!({"width": 200, "height": 200})).unwrap();
        app.session.execute("shape.create", json!({"kind": "rect", "rect": [20, 30, 80, 40], "fill": "#ff0000"})).unwrap();
        app.sync_views();
        app.ui.tool = Tool::Move;
        app.ui.tool_options.move_show_transform = true;

        let xf = ViewXform { rect: Rect::from_min_size(Pos2::ZERO, vec2(200.0, 200.0)), zoom: 1.0, center: [100.0, 100.0], flip: false, rotation: 0.0 };
        let r = transform_controls_rect(&app, &xf).expect("shape layers have transform bounds");
        assert!(transform_controls_hit(r, r.right_bottom()));

        let ctx = egui::Context::default();
        assert!(begin_transform_controls_at(&mut app, &ctx, &xf, r.right_bottom()));
        let t = app.ui.transform.as_ref().expect("control press starts Free Transform");
        assert_eq!(t.rect, [20.0, 30.0, 100.0, 70.0]);
    }

    #[test]
    fn hiding_extras_hides_the_move_transform_controls() {
        // #6699: as in Photoshop, View > Extras (CommandH) hides the Move tool's transform box and
        // handles; again brings them back. Show Transform Controls and the layer stay as they were.
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.execute("file.new", json!({"width": 200, "height": 200})).unwrap();
        app.session.execute("shape.create", json!({"kind": "rect", "rect": [20, 30, 80, 40], "fill": "#ff0000"})).unwrap();
        app.sync_views();
        app.ui.tool = Tool::Move;
        app.ui.tool_options.move_show_transform = true;
        let xf = ViewXform { rect: Rect::from_min_size(Pos2::ZERO, vec2(200.0, 200.0)), zoom: 1.0, center: [100.0, 100.0], flip: false, rotation: 0.0 };
        let ctx = egui::Context::default();
        let shown = transform_controls_rect(&app, &xf).expect("the box shows with Extras on");
        let layer = app.session.active().unwrap().doc.layers.clone();
        let painted = |app: &mut PhotocraftApp| {
            let out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let painter = ui.ctx().layer_painter(egui::LayerId::background());
                draw_transform_controls(app, &painter, &xf);
            });
            let egui::FullOutput { mut textures_delta, shapes, .. } = out;
            textures_delta.clear();
            shapes.len()
        };
        // The shape's outline, the box and its eight handles.
        assert!(painted(&mut app) > 8, "the box and handles are drawn");

        crate::menus::invoke(&mut app, &ctx, "view.extras", json!({})).unwrap();
        assert!(!app.ui.view.extras);
        assert!(transform_controls_rect(&app, &xf).is_none(), "no box with Extras hidden");
        assert_eq!(painted(&mut app), 0, "nothing is drawn");
        // Hidden handles don't act: a press on a corner doesn't start Free Transform.
        assert!(!begin_transform_controls_at(&mut app, &ctx, &xf, shown.right_bottom()));
        assert!(app.ui.transform.is_none());
        assert!(app.ui.tool_options.move_show_transform, "Show Transform Controls is unchanged");
        assert_eq!(app.session.active().unwrap().doc.layers, layer, "the layer is unchanged");

        crate::menus::invoke(&mut app, &ctx, "view.extras", json!({})).unwrap();
        assert!(app.ui.view.extras);
        assert_eq!(transform_controls_rect(&app, &xf), Some(shown), "CommandH again restores the box");
    }

    #[test]
    fn transform_control_hit_keeps_move_interior_free_and_has_rotation_band() {
        let _gpu_gate = crate::gpu_test_support::lock();
        let r = Rect::from_min_max(pos2(20.0, 30.0), pos2(100.0, 70.0));
        assert!(transform_controls_hit(r, r.left_top()));
        assert!(transform_controls_hit(r, pos2(60.0, 18.0)));
        assert!(!transform_controls_hit(r, r.center()));
        assert!(!transform_controls_hit(r, pos2(60.0, 4.0)));
    }
}

#[cfg(test)]
mod display_dither_tests {
    #[test]
    fn display_cpu_identity_high_depth_dithers_but_eight_bit_stays_exact() {
        let color = photocraft_engine::color_cmds::ColorState::default();
        let d = color.srgb_preview_display().unwrap();
        let buf = photocraft_compose::Buffer::filled(photocraft_geom::Rect::new(0, 0, 8, 8), [0.5, 0.5, 0.5, 1.0]);
        let high = super::display_image(Some(&d), &buf, photocraft_doc::SampleType::U16);
        let low = super::display_image(Some(&d), &buf, photocraft_doc::SampleType::U8);
        assert!(high.pixels.iter().any(|p| p.r() == 127));
        assert!(high.pixels.iter().any(|p| p.r() == 128));
        assert!(low.pixels.iter().all(|p| p.r() == 128));
        let tile = photocraft_compose::Buffer::filled(photocraft_geom::Rect::new(3, 2, 5, 4), [0.5, 0.5, 0.5, 1.0]);
        let part = super::display_image(Some(&d), &tile, photocraft_doc::SampleType::U16);
        assert_eq!(part.pixels[0], high.pixels[2 * 8 + 3]);
        assert_eq!(part.pixels[3], high.pixels[3 * 8 + 4]);
    }
}

#[test]
#[ignore = "manual timing of the real CPU canvas path; run explicitly with --nocapture"]
fn cpu_canvas_interactive_display_timing() {
    let mut session = photocraft_engine::Session::new();
    session.execute("file.new", serde_json::json!({"width":2048,"height":2048,"depth":32,"background":"white"})).unwrap();
    session.color.settings.monitor_profile = "display-p3".into();
    let mut app = crate::PhotocraftApp::new(session, Default::default());
    assert!(app.gpu.is_none());
    let ctx = egui::Context::default();
    let mut runs = Vec::new();
    for _ in 0..6 {
        app.canvases.clear();
        let start = std::time::Instant::now();
        assert!(ensure_texture(&mut app, &ctx, 0, None).is_some());
        runs.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    runs.remove(0);
    runs.sort_by(f64::total_cmp);
    eprintln!("dsfix CPU canvas ensure_texture 2048x2048 f32 -> P3 median {:.3} ms; all {:?}", runs[runs.len() / 2], runs);
}

#[test]
fn fixed_size_px_is_exactly_the_size() {
    use super::fixed_size_px;
    // Down-right from (10.4, 20.6): 10 x 10 from the closest pixel edges.
    assert_eq!(fixed_size_px([10.4, 20.6], [20.4, 30.6], 10.0, 10.0), [10.0, 21.0, 20.0, 31.0]);
    // Up-left: the far corner is the low one; still exactly the size.
    assert_eq!(fixed_size_px([10.4, 20.6], [-53.6, -11.4], 64.0, 32.0), [-54.0, -11.0, 10.0, 21.0]);
    // 1 x 1 is one pixel, and a fractional size rounds to whole pixels.
    assert_eq!(fixed_size_px([3.5, 3.5], [4.5, 4.5], 1.0, 1.0), [4.0, 4.0, 5.0, 5.0]);
    assert_eq!(fixed_size_px([0.0, 0.0], [2.6, 2.4], 2.6, 2.4), [0.0, 0.0, 3.0, 2.0]);
}

#[test]
fn cpu_canvas_draws_large_documents_at_full_resolution() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 3 * MAX_TEXTURE, "height": 8, "background": "white"})).unwrap();
    app.run("select.rect", json!({"x": 101, "y": 0, "width": 1, "height": 8})).unwrap();
    app.run("edit.fill", json!({"color": "#000000"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    app.sync_views();
    let ctx = egui::Context::default();
    ctx.input_mut(|input| input.max_texture_side = 16384);
    let view = View { zoom: 1.0, center: [500.0, 4.0], fit_pending: false, ..Default::default() };
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 8.0));
    let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 80.0))), ..Default::default() }, |ui| {
        canvas_view(&mut app, ui, 0, rect, view.clone(), false);
    });
    let sharp = output.shapes.iter().any(|shape| {
        let egui::Shape::Mesh(mesh) = &shape.shape else { return false };
        let Some(image) = output.textures_delta.set.get(&mesh.texture_id).and_then(|updates| updates.last()) else { return false };
        let egui::ImageData::Color(image) = &image.image;
        image.size == [1024, 8] && image.pixels[100] == Color32::WHITE && image.pixels[101] == Color32::BLACK && image.pixels[102] == Color32::WHITE
    });
    output.textures_delta.clear();
    assert!(sharp, "the canvas draws the visible one-pixel line from full-resolution pixels");
}

#[test]
fn cpu_canvas_detail_plan_shows_large_documents_at_full_resolution() {
    let size = [12000, 8000];
    // A 1920x1080 view in the middle (`visible_doc_rect` adds 2 px around it).
    let screen = DRect::new(4998, 2998, 6922, 4082);
    for limit in [16384, 8192, 4096, 2048] {
        let ctx = egui::Context::default();
        ctx.input_mut(|input| input.max_texture_side = limit);
        let side = texture_side(&ctx);
        assert!(texture_factor(size, side) > 1, "the whole document is reduced on a {limit} px device");
        for zoom in [1.0, 2.0, 8.0] {
            let plan = detail_plan(size, side, zoom, screen).unwrap_or_else(|| panic!("{limit} px device at {zoom}"));
            assert_eq!(plan.factor, 1, "one texel per document pixel at {zoom} on a {limit} px device");
            assert!(plan.region.contains_rect(&screen), "{plan:?}");
            assert!(plan.region.width() <= 1924 + 2 * 256 && plan.region.height() <= 1084 + 2 * 256, "about the view's size: {plan:?}");
            assert!(plan.region.width() < side && plan.region.height() < side, "{plan:?}");
        }
    }
    // A small pan reuses the region (no new composite).
    assert_eq!(detail_plan(size, 4096, 1.0, screen), detail_plan(size, 4096, 1.0, screen.translate(8, -8)));
    // A window wider than a small device's limit coarsens rather than exceed it.
    let wide = DRect::new(0, 0, 3000, 2000);
    let plan = detail_plan(size, 2048, 1.0, wide).unwrap();
    assert_eq!(plan.factor, 2);
    assert!(plan.region.width() / 2 < 2048 && plan.region.contains_rect(&wide), "{plan:?}");
    // Zoomed out a little: finer than the reduced whole, as fine as the screen.
    let plan = detail_plan(size, 4096, 0.4, DRect::new(0, 0, 4800, 2700)).unwrap();
    assert_eq!(plan.factor, 2);
    // Zoomed out far enough, the reduced whole is as fine as the screen: nothing to add.
    let whole = DRect::new(0, 0, 12000, 8000);
    assert_eq!(detail_plan(size, 4096, 0.25, whole), None);
    assert_eq!(detail_plan(size, 4096, 1.0 / 3.0, whole), None, "a detail as large as the document would not fit");
    // A document that fits is uploaded whole at full resolution.
    assert_eq!(detail_plan([4000, 3000], 4096, 1.0, screen), None);
    // Nothing of the document in view, or a broken zoom.
    assert_eq!(detail_plan(size, 4096, 1.0, DRect::new(-500, -500, -10, -10)), None);
    for zoom in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(detail_plan(size, 4096, zoom, screen), None, "{zoom}");
    }
    assert_eq!(detail_plan([0, 0], 4096, 1.0, screen), None);
    assert_eq!(detail_plan([u32::MAX, 8], 4096, 1.0, screen), None);
}

#[test]
fn cpu_canvas_detail_shows_a_large_documents_own_pixels_at_100_percent() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 3 * MAX_TEXTURE, "height": 8})).unwrap();
    app.run("select.rect", json!({"x": 101, "y": 0, "width": 1, "height": 8})).unwrap();
    app.run("edit.fill", json!({"color": "#000000"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    let ctx = egui::Context::default();
    ctx.input_mut(|i| i.max_texture_side = 16384);
    let uploads = |ctx: &egui::Context, tex: egui::TextureId| {
        let mut delta = ctx.tex_manager().write().take_delta();
        let seen: Vec<_> = delta.set.get(&tex).into_iter().flatten().map(|d| (d.pos, d.options, d.image.clone())).collect();
        delta.clear();
        seen
    };
    let row = |image: &egui::ImageData| {
        let egui::ImageData::Color(image) = image;
        (image.size, image.pixels.clone())
    };
    let view = InView { visible: DRect::new(0, 0, 1000, 8), zoom: 1.0, key: 1 };
    let t = ensure_texture_in_view(&mut app, &ctx, 0, None, Some(view)).unwrap();
    assert!(t.scale < 1.0, "the whole document is reduced");
    let (detail, area) = t.detail.expect("a detail texture at 100 %");
    assert_eq!(area, [0.0, 0.0, 1024.0, 8.0]);
    let crisp = TextureOptions { magnification: egui::TextureFilter::Nearest, ..TextureOptions::LINEAR };
    let up = uploads(&ctx, detail);
    assert_eq!(up.len(), 1);
    let (pos, options, image) = &up[0];
    assert_eq!((*pos, *options), (None, crisp));
    let (size, px) = row(image);
    assert_eq!(size, [1024, 8]);
    assert_eq!((px[100], px[101], px[102]), (Color32::WHITE, Color32::BLACK, Color32::WHITE), "the 1 px line stays 1 px");
    // Unchanged: nothing is composited again, and a small pan keeps the region.
    let panned = InView { visible: DRect::new(4, 0, 1004, 8), ..view };
    assert_eq!(ensure_texture_in_view(&mut app, &ctx, 0, None, Some(panned)).unwrap().detail, Some((detail, area)));
    assert!(uploads(&ctx, detail).is_empty());
    // The Navigator's call (no view) leaves the view's detail alone.
    ensure_texture(&mut app, &ctx, 0, None).unwrap();
    assert_eq!(app.canvases.values().map(|c| c.detail.len()).sum::<usize>(), 1);
    // An edit in view updates just its texels.
    app.run("select.rect", json!({"x": 300, "y": 0, "width": 1, "height": 8})).unwrap();
    ensure_texture_in_view(&mut app, &ctx, 0, None, Some(view)).unwrap();
    uploads(&ctx, detail);
    app.run("edit.fill", json!({"color": "#000000"})).unwrap();
    assert_eq!(ensure_texture_in_view(&mut app, &ctx, 0, None, Some(view)).unwrap().detail.map(|d| d.0), Some(detail));
    let up = uploads(&ctx, detail);
    assert_eq!(up.len(), 1);
    let (pos, options, image) = &up[0];
    assert_eq!(*options, crisp);
    let [x, y] = pos.expect("a partial update");
    let (size, px) = row(image);
    assert!(y == 0 && x < 300 && x + size[0] > 300, "the edit's texels: {x} {size:?}");
    assert_eq!((px[299 - x], px[300 - x]), (Color32::WHITE, Color32::BLACK));
    // Zoomed out, the reduced texture is enough: the detail goes.
    let far = InView { zoom: 0.25, ..view };
    assert!(ensure_texture_in_view(&mut app, &ctx, 0, None, Some(far)).unwrap().detail.is_none());
    assert!(app.canvases.values().all(|c| c.detail.is_empty()));
}
