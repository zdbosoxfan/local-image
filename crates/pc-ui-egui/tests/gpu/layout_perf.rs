//! Layout documents (#125, #128) through the real app: the Move tool shows the layers at the
//! pointer while dragging and commits one `layer.translate` (one undo step) that lands where the
//! pointer went; Auto-Select takes the layer under the pointer; selecting a layer recomposites
//! nothing and hiding one recomposites only its area; and the GPU compositor (which composites
//! small layers over their bounds only) matches the CPU reference on the whole document.

#[path = "../support/layout_doc.rs"]
mod layout_doc;

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_engine::layer_multi_cmds::{layer_bounds, move_targets, moved};
use photocraft_geom::Rect;
use photocraft_ui_egui::canvas::ViewXform;
use photocraft_ui_egui::state::Tool;
use photocraft_ui_egui::{PhotocraftApp, Services};
use serde_json::json;

type H = Harness<'static, PhotocraftApp>;

/// The app (CPU canvas) with the small layout document open.
fn app() -> (H, layout_doc::Handles) {
    let (doc, handles) = layout_doc::build(layout_doc::Spec::small());
    let h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_pixels_per_point(1.0).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
        app.session.open_document(doc, None);
        app.sync_views();
        app
    });
    let mut h = h;
    h.run_steps(6);
    (h, handles)
}

fn doc(h: &H) -> std::sync::Arc<Document> {
    h.state().session.active().expect("document").doc.clone()
}

fn bounds(h: &H, id: LayerId) -> Rect {
    layer_bounds(doc(h).layer(id).expect("layer")).expect("bounds")
}

fn screen(h: &H, p: [f64; 2]) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    let rect = photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect);
    ViewXform { rect, zoom: v.zoom, center: v.center, flip: false }.to_screen(p[0] as f32, p[1] as f32)
}

/// A point where `id` (or, for a group, one of its layers) is the topmost layer with pixels.
fn point_on(h: &H, id: LayerId) -> [f64; 2] {
    fn holds(l: &photocraft_doc::Layer, id: LayerId) -> bool {
        l.id == id || l.children().is_some_and(|c| c.iter().any(|c| holds(c, id)))
    }
    let d = doc(h);
    let b = bounds(h, id);
    let layer = d.layer(id).expect("layer");
    for y in b.y0..b.y1 {
        for x in b.x0..b.x1 {
            if photocraft_engine::pick_cmds::layers_at(&d, x, y).first().is_some_and(|&top| holds(layer, top)) {
                return [x as f64 + 0.5, y as f64 + 0.5];
            }
        }
    }
    panic!("layer {id:?} has no visible pixel");
}

/// Press at document point `from`, move by `d` in `steps` frames; returns before releasing.
fn press_and_drag(h: &mut H, from: [f64; 2], d: [f64; 2], steps: usize) {
    let p0 = screen(h, from);
    h.event(Event::PointerMoved(p0));
    h.step();
    h.event(Event::PointerButton { pos: p0, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    for i in 1..=steps {
        let f = i as f64 / steps as f64;
        h.event(Event::PointerMoved(screen(h, [from[0] + d[0] * f, from[1] + d[1] * f])));
        h.step();
    }
}

fn release(h: &mut H, at: [f64; 2]) {
    let p = screen(h, at);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
}

/// Zoom so one document pixel is one screen point (pointer deltas are then exact).
fn actual_pixels(h: &mut H) {
    let app = h.state_mut();
    let size = app.session.active().map(|s| s.doc.size).expect("doc");
    app.ui.views[0].zoom = 1.0;
    app.ui.views[0].center = [size.width as f32 / 2.0, size.height as f32 / 2.0];
    app.ui.views[0].fit_pending = false;
    // View › Snap and smart guides would pull the layers onto their neighbours' edges.
    app.ui.extras.snap = false;
    app.ui.view.show.smart_guides = false;
    h.run_steps(2);
}

#[test]
fn move_drag_follows_the_pointer_and_is_one_undo_step() {
    let _gpu = crate::gpu_lock();
    let (mut h, k) = app();
    actual_pixels(&mut h);
    h.state_mut().ui.tool = Tool::Move;
    h.state_mut().ui.tool_options.move_auto_select = false;
    for id in [k.text, k.shape, k.group, k.smart, k.pixel] {
        h.state_mut().run("layer.select", json!({"layer": id.0})).expect("select");
        h.run_steps(2);
        let before = bounds(&h, id);
        let undo = h.state().session.active().expect("doc").history.entries().len();
        let from = point_on(&h, id);
        press_and_drag(&mut h, from, [40.0, 25.0], 6);
        // While dragging the document is untouched (no history step per pointer move) and the
        // canvas shows the moved layers.
        assert_eq!(bounds(&h, id), before, "the drag commits nothing until release");
        assert_eq!(h.state().session.active().expect("doc").history.entries().len(), undo);
        assert!(h.state().perf.spans.contains_key("move preview"), "the canvas shows the layers moving");
        release(&mut h, [from[0] + 40.0, from[1] + 25.0]);
        assert_eq!(bounds(&h, id), before.translate(40, 25), "the layer lands where the pointer went");
        assert_eq!(h.state().session.active().expect("doc").history.entries().len(), undo + 1, "one undo step");
        h.state_mut().run("edit.undo", json!({})).expect("undo");
        assert_eq!(bounds(&h, id), before);
    }
}

#[test]
fn auto_select_moves_the_layer_under_the_pointer() {
    let _gpu = crate::gpu_lock();
    let (mut h, k) = app();
    actual_pixels(&mut h);
    h.state_mut().ui.tool = Tool::Move;
    h.state_mut().ui.tool_options.move_auto_select = true;
    h.state_mut().run("layer.select", json!({"layer": k.pixel.0})).expect("select");
    h.run_steps(2);
    let (pixel_before, text_before) = (bounds(&h, k.pixel), bounds(&h, k.text));
    let from = point_on(&h, k.text);
    press_and_drag(&mut h, from, [-15.0, 10.0], 4);
    release(&mut h, [from[0] - 15.0, from[1] + 10.0]);
    assert_eq!(h.state().session.active().and_then(|s| s.active_layer), Some(k.text));
    assert_eq!(bounds(&h, k.text), text_before.translate(-15, 10));
    assert_eq!(bounds(&h, k.pixel), pixel_before, "the previously selected layer stays");
}

#[test]
fn a_move_tool_click_selects_without_moving() {
    let _gpu = crate::gpu_lock();
    // Snapping stays on (the default): it used to pull the release point onto a nearby edge, so
    // a plain click nudged the layer (and recomposited the whole document).
    let (mut h, k) = app();
    h.state_mut().ui.tool = Tool::Move;
    h.state_mut().ui.tool_options.move_auto_select = true;
    for id in [k.text, k.shape, k.pixel, k.text] {
        let before = (bounds(&h, id), h.state().session.active().expect("doc").history.entries().len());
        let p = screen(&h, point_on(&h, id));
        h.event(Event::PointerMoved(p));
        h.step();
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.step();
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
        assert_eq!(h.state().session.active().and_then(|s| s.active_layer), Some(id), "the click selects the layer under it");
        assert_eq!((bounds(&h, id), h.state().session.active().expect("doc").history.entries().len()), before, "and moves nothing");
    }
}

#[test]
fn selecting_recomposites_nothing_and_hiding_only_the_layer() {
    let _gpu = crate::gpu_lock();
    let (mut h, k) = app();
    for id in [k.text, k.group, k.smart, k.shape] {
        h.state_mut().perf.last_refresh = "untouched";
        h.state_mut().run("layer.select", json!({"layer": id.0})).expect("select");
        h.run_steps(3);
        assert_eq!(h.state().perf.last_refresh, "untouched", "selecting a layer must not recomposite");
    }
    // The Properties panel's Levels histogram isn't recomputed for a selection change either.
    let levels = doc(&h).layers.iter().rev().find(|l| matches!(l.content, LayerContent::Adjustment(_))).map(|l| l.id).expect("levels layer");
    h.state_mut().run("layer.select", json!({"layer": levels.0})).expect("select");
    h.run_steps(3);
    assert!(h.state().perf.spans.contains_key("histogram"), "the Levels editor shows a histogram");
    h.state_mut().perf.spans.remove("histogram");
    h.state_mut().run("layer.select", json!({"layer": levels.0})).expect("select");
    h.run_steps(3);
    assert!(!h.state().perf.spans.contains_key("histogram"), "selecting changed no pixels");
    // Hiding a text layer refreshes its area, not the whole canvas.
    h.state_mut().run("layer.setProps", json!({"layer": k.text.0, "visible": false})).expect("hide");
    h.run_steps(2);
    assert_eq!(h.state().perf.last_refresh, "rect");
    let d = doc(&h);
    let reduced = h.state().perf.last_refresh_px;
    assert!(reduced < d.size.area() / 4, "{reduced} px refreshed");
}

#[test]
fn the_move_preview_is_what_the_move_commits() {
    let _gpu = crate::gpu_lock();
    let (d, k) = layout_doc::build(layout_doc::Spec::small());
    let mut s = photocraft_engine::Session::new();
    s.open_document(d, None);
    for id in [k.text, k.shape, k.group, k.smart, k.pixel, k.closed_group] {
        s.execute("layer.select", json!({"layer": id.0})).expect("select");
        let before = s.active().expect("doc").doc.clone();
        let ids = move_targets(&before, &[id]);
        let preview = moved(&before, &ids, 13, -7).expect("preview");
        s.execute("layer.translate", json!({"dx": 13, "dy": -7})).expect("move");
        let after = s.active().expect("doc");
        // The damage the canvas refreshes covers where the layer was and is.
        let dmg = after.last_damage.unwrap_or_else(|| panic!("{id:?} {}: a move has a damage rect", before.layer(id).map_or("?", |l| l.content.kind_name())));
        let b0 = photocraft_compose::composite_bounds(before.layer(id).expect("layer"), before.bounds()).expect("bounds");
        assert!(dmg.contains_rect(&b0) && dmg.contains_rect(&b0.translate(13, -7).intersect(&before.bounds())), "{dmg:?} vs {b0:?}");
        let (a, b) = (photocraft_compose::flatten(&preview), photocraft_compose::flatten(&after.doc));
        let worst = a.px.iter().zip(&b.px).flat_map(|(p, q)| (0..4).map(move |c| (p[c] * p[3] - q[c] * q[3]).abs())).fold(0.0f32, f32::max);
        assert!(worst <= 2.0 / 255.0, "{id:?}: preview differs from the commit by {}/255", worst * 255.0);
        s.execute("edit.undo", json!({})).expect("undo");
    }
}

#[test]
fn layout_document_composites_on_the_gpu_like_the_cpu() {
    let _gpu = crate::gpu_lock();
    use eframe::wgpu;
    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        let mut f = std::pin::pin!(f);
        loop {
            if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }
            std::thread::yield_now();
        }
    }
    let Ok(adapter) = block_on(wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    if photocraft_gpu::Compositor::preferred_acc_format(&adapter) != wgpu::TextureFormat::Rgba32Float {
        eprintln!("skipping: adapter can't render Rgba32Float");
        return;
    }
    let Ok((device, queue)) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())) else { return };
    let Ok(mut comp) = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float) else { return };
    let (d, _) = layout_doc::build(layout_doc::Spec::small());
    let (_, texts) = layout_doc::count(&d);
    assert!(texts >= 10);
    assert!(d.walk().iter().any(|(_, _, l)| matches!(l.content, LayerContent::Smart(_))));
    let cpu = photocraft_compose::flatten(&d);
    let gpu = photocraft_gpu::render_to_vec(&mut comp, &device, &queue, &d, d.bounds()).expect("the layout composites on the GPU");
    let mut worst = 0.0f32;
    for (c, g) in cpu.px.iter().zip(&gpu) {
        for k in 0..4 {
            let (a, b) = if k == 3 { (c[3], g[3]) } else { (c[k] * c[3], g[k] * g[3]) };
            worst = worst.max((a - b).abs());
        }
    }
    assert!(worst <= 2.0 / 255.0, "GPU differs from the CPU by {}/255", worst * 255.0);
}

/// Read the actual app canvas during a group drag, including stroked shapes with effects,
/// against a CPU render that cannot hit the translated split entry. No adapter skips: this
/// regression must exercise GPU pixels and the real pointer-event preview/commit path.
#[test]
fn stroked_group_drag_gpu_pixels_match_fresh_render_and_commit() {
    let _gpu = crate::gpu_lock();
    let (document, handles) = layout_doc::build(layout_doc::Spec::small());
    let observer = std::sync::Arc::new(std::sync::Mutex::new(None));
    let capture = observer.clone();
    let mut h: H = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_pixels_per_point(1.0)
        .with_max_steps(64)
        .wgpu_setup(photocraft_ui_egui::gpu_canvas::wgpu_setup())
        .build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
            let rs = cc.wgpu_render_state.as_ref().expect("GPU render state");
            let info = rs.adapter.get_info();
            eprintln!("group drag GPU: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
            assert_ne!(info.device_type, eframe::wgpu::DeviceType::Cpu, "a software adapter does not cover this regression");
            *capture.lock().expect("observer") = Some(photocraft_ui_egui::gpu_canvas::GpuCanvas::new(rs));
            app.set_wgpu(rs.clone());
            app.session.open_document(document, None);
            app.sync_views();
            app
        });
    h.run_steps(6);
    actual_pixels(&mut h);
    h.state_mut().ui.tool = Tool::Move;
    h.state_mut().ui.tool_options.move_auto_select = false;
    h.state_mut().run("layer.select", json!({"layer": handles.group.0})).expect("select group");
    h.run_steps(2);
    let before = doc(&h);
    let old_bounds = bounds(&h, handles.group);
    let history = h.state().session.active().expect("document").history.entries().len();
    let gpu = observer.lock().expect("observer").clone().expect("GPU canvas");
    let compare = |expected: &Document, phase: &str| {
        // Mint fresh identities for the expected cached shape pixels. This forces the split
        // fitter to render at this exact final position rather than use the moved entry.
        let mut fresh = expected.clone();
        fn reset(layers: &mut [photocraft_doc::Layer]) {
            for l in layers {
                if let LayerContent::Shape(sh) = &mut l.content
                    && let Some(c) = &mut sh.cache
                {
                    let r = photocraft_compose::bounds::content_bounds(c);
                    *c = photocraft_raster::Surface::from_interleaved(c.format(), r, &c.to_interleaved(r));
                }
                if let Some(children) = l.children_mut() {
                    reset(children);
                }
            }
        }
        reset(&mut fresh.layers);
        let cpu = photocraft_compose::flatten(&fresh);
        let (_, size, pixels) = gpu.read_texels(expected.id.0).expect("GPU app canvas readback");
        assert_eq!(size, [expected.size.width, expected.size.height]);
        let (mut worst, mut at) = (0.0f32, 0usize);
        for (i, (c, g)) in cpu.px.iter().zip(&pixels).enumerate() {
            for k in 0..4 {
                let want = if k == 3 { c[3] } else { c[k] * c[3] };
                if (want - g[k]).abs() > worst {
                    worst = (want - g[k]).abs();
                    at = i;
                }
            }
        }
        assert!(
            worst <= 2.0 / 255.0,
            "{phase}: app canvas differs by {}/255 at ({}, {}): CPU {:?}, GPU {:?}",
            worst * 255.0,
            at % size[0] as usize,
            at / size[0] as usize,
            cpu.px[at],
            pixels[at]
        );
    };
    compare(&before, "before drag");
    let from = point_on(&h, handles.group);
    let start = screen(&h, from);
    h.event(Event::PointerMoved(start));
    h.step();
    h.event(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    for i in 1..=6 {
        let (dx, dy) = (i * 7, i * 4);
        h.event(Event::PointerMoved(screen(&h, [from[0] + f64::from(dx), from[1] + f64::from(dy)])));
        h.step();
        // Canvas painting precedes tool-event handling in the app's UI pass. Present the newly
        // computed preview in the next frame while the pointer is still held at this position.
        h.step();
        assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none(), "real GPU preview");
        assert!(h.state().perf.spans.contains_key("move preview"), "the pointer drives a live Move preview");
        assert_eq!(bounds(&h, handles.group), old_bounds, "no commit during drag");
        compare(&moved(&before, &[handles.group], dx, dy).expect("expected preview"), &format!("move {dx}, {dy}"));
    }
    release(&mut h, [from[0] + 42.0, from[1] + 24.0]);
    assert_eq!(bounds(&h, handles.group), old_bounds.translate(42, 24));
    assert_eq!(h.state().session.active().expect("document").history.entries().len(), history + 1);
    compare(&doc(&h), "committed");
    h.state_mut().run("edit.undo", json!({})).expect("undo");
    h.run_steps(3);
    assert_eq!(bounds(&h, handles.group), old_bounds);
    compare(&doc(&h), "undone");
}

#[test]
fn psd_group_preview_preserves_stroked_shape_pixel_identity() {
    let _gpu = crate::gpu_lock();
    let (document, _) = layout_doc::build(layout_doc::Spec::full());
    let bytes = photocraft_io::export(&document, "layout.psd", &Default::default()).expect("PSD export").bytes;
    let document = photocraft_io::import("layout.psd", &bytes).expect("PSD import").document;
    let group = document.walk().into_iter().find(|(_, _, l)| l.name == "Card 1" && l.is_group()).expect("card group").2.id;
    let preview = moved(&document, &[group], 37, 23).expect("group preview");
    for layer in document.layer(group).expect("group").children().expect("children") {
        let LayerContent::Shape(original) = &layer.content else { continue };
        let LayerContent::Shape(shifted) = &preview.layer(layer.id).expect("moved shape").content else { panic!("shape") };
        let original = original.cache.as_ref().expect("original cache");
        let shifted = shifted.cache.as_ref().expect("shifted cache");
        assert_eq!(shifted.translation_identity(), (original.translation_identity().0, 37, 23), "{} keeps its PSD pixel source", layer.name);
    }
}
