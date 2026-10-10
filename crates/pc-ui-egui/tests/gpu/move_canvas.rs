//! Real egui presses/motions and the app's wgpu paint callbacks, with no no-adapter skip.
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_doc::LayerId;
use photocraft_geom::Rect;
use photocraft_ui_egui::{
    PhotocraftApp,
    canvas::ViewXform,
    control::{ControlRequest, handle},
};
use serde_json::json;
use std::time::Instant;

type H = Harness<'static, PhotocraftApp>;

fn harness(ppp: f32) -> H {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_pixels_per_point(ppp)
        .wgpu_setup(photocraft_ui_egui::gpu_canvas::wgpu_setup())
        .build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
            app.set_wgpu(cc.wgpu_render_state.as_ref().expect("a real GPU is required").clone());
            app
        });
    h.run_steps(3);
    assert!(h.state().gpu_active());
    assert_ne!(h.state().perf.gpu_info.device_type, "cpu", "software rasterizers do not count");
    assert!(!h.state().perf.gpu_info.adapter.to_lowercase().contains("llvmpipe"));
    h
}

fn setup(h: &mut H, w: u32, height: u32, zoom: f32) {
    h.state_mut().run("file.new", json!({"width": w, "height": height, "background": "white"})).unwrap();
    h.state_mut().sync_views();
    let ctx = h.ctx.clone();
    let (req, _) = ControlRequest::new("ui.set", json!({"tool": "move", "zoom": zoom, "center": [w / 2, height / 2]}));
    handle(h.state_mut(), &ctx, &req);
    let app = h.state_mut();
    app.ui.extras.rulers = false;
    app.ui.extras.snap = false;
    app.ui.view.show.smart_guides = false;
    app.ui.view.show.layer_edges = false;
    app.ui.tool_options.move_show_transform = false;
    app.session.edit_prefs(|p| {
        p.interface.show_bounding_box_when_dragging_layer = false;
        p.interface.contextual_task_bar = false;
    });
    app.ui.tool_options.move_auto_select = false;
}

fn layer(h: &mut H, name: &str, b: Rect, color: [f32; 4]) -> LayerId {
    h.state_mut().run("layer.new.layer", json!({"name": name})).unwrap();
    let id = h.state().session.active().unwrap().active_layer.unwrap();
    h.state_mut()
        .session
        .edit("paint", |d, _| {
            d.layer_mut(id).unwrap().surface_mut().unwrap().fill_rect(b, &color);
            Ok(())
        })
        .unwrap();
    id
}

fn screen(h: &H, x: f32, y: f32) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    ViewXform { rect: photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: app.ui.view.flip_horizontal }
        .to_screen(x, y)
}

fn button(h: &mut H, p: Pos2, pressed: bool) {
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}

fn pointer(h: &mut H, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.step();
}

fn check_pixels(h: &mut H, ids: &[LayerId], dx: i32, dy: i32) {
    // Harness::render paints its own 16-point triangle cursor. A PointerGone frame preserves
    // the held Move gesture while allowing an unobstructed comparison of the canvas pixels.
    h.event(Event::PointerGone);
    h.step();
    let st = h.state().session.active().unwrap();
    let expected = photocraft_engine::layer_multi_cmds::moved(&st.doc, ids, dx, dy).unwrap();
    let buf = photocraft_compose::render(&expected, expected.bounds());
    let image = h.render().expect("render actual app canvas");
    let ppp = h.ctx.pixels_per_point();
    let mut checked = 0;
    for y in (15..285).step_by(7) {
        for x in (15..385).step_by(7) {
            let p = screen(h, x as f32 + 0.5, y as f32 + 0.5) * ppp;
            if !h.state().last_canvas_rect.contains(p / ppp) {
                continue;
            }
            let pixel = image.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0;
            let e = buf.px[y * 400 + x];
            for c in 0..3 {
                assert!((f32::from(pixel[c]) / 255.0 - e[c]).abs() <= 4.0 / 255.0, "({x},{y}) channel {c}: {:?} expected {:?}", pixel, e);
            }
            checked += 1;
        }
    }
    assert!(checked > 1000);
}

#[test]
fn move_canvas_gpu_planes_preserve_stack_order_alpha_flip_and_hidpi() {
    let _gpu = crate::gpu_lock();
    for (ppp, flip) in [(1.0, false), (2.0, true)] {
        let mut h = harness(ppp);
        setup(&mut h, 400, 300, 1.0);
        let a = layer(&mut h, "Moving A", Rect::new(40, 45, 150, 220), [0.95, 0.2, 0.1, 0.6]);
        layer(&mut h, "Stationary middle", Rect::new(100, 60, 250, 240), [0.1, 0.9, 0.2, 0.5]);
        let b = layer(&mut h, "Moving B", Rect::new(170, 80, 310, 255), [0.1, 0.2, 0.9, 0.7]);
        layer(&mut h, "Stationary top", Rect::new(240, 50, 360, 200), [0.8, 0.2, 0.8, 0.4]);
        h.state_mut().session.execute("layer.select", json!({"layer": a.0})).unwrap();
        h.state_mut().session.execute("layer.select", json!({"layer": b.0, "mode": "add"})).unwrap();
        h.state_mut().ui.view.flip_horizontal = flip;
        h.run_steps(3);
        let p = screen(&h, 80.0, 120.0);
        pointer(&mut h, p);
        let steps = h.state().session.active().unwrap().history.past_len();
        button(&mut h, p, true);
        let cache = h.state().perf.spans["move cache"];
        for (dx, dy) in [(1, 0), (19, 11), (32, 17)] {
            let q = p + egui::vec2(if flip { -(dx as f32) } else { dx as f32 }, dy as f32);
            pointer(&mut h, q);
            assert_eq!(h.state().perf.last_refresh, "gpu-move-offset");
            assert_eq!(h.state().perf.gpu_uploads, 0);
            assert_eq!(h.state().perf.spans["move cache"], cache, "no per-frame pixel work");
            check_pixels(&mut h, &[a, b], dx, dy);
        }
        let q = p + egui::vec2(if flip { -32.0 } else { 32.0 }, 17.0);
        button(&mut h, q, false);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), steps + 1);
        check_pixels(&mut h, &[], 0, 0);
    }
}

#[test]
fn move_canvas_gpu_brings_off_canvas_raster_pixels_into_view() {
    let _gpu = crate::gpu_lock();
    let mut h = harness(1.0);
    setup(&mut h, 400, 300, 1.0);
    let id = layer(&mut h, "Off canvas", Rect::new(-40, 40, 60, 180), [0.9, 0.1, 0.1, 1.0]);
    h.run_steps(3);
    let p = screen(&h, 40.0, 100.0);
    pointer(&mut h, p);
    button(&mut h, p, true);
    pointer(&mut h, p + egui::vec2(100.0, 0.0));
    assert_eq!(h.state().perf.last_refresh, "gpu-move-offset");
    check_pixels(&mut h, &[id], 100, 0);
    button(&mut h, p + egui::vec2(100.0, 0.0), false);
}

#[test]
fn move_canvas_backdrop_dependent_blend_keeps_live_reference_preview() {
    let _gpu = crate::gpu_lock();
    let mut h = harness(1.0);
    setup(&mut h, 400, 300, 1.0);
    layer(&mut h, "Below", Rect::new(40, 40, 340, 260), [0.3, 0.8, 0.5, 1.0]);
    let id = layer(&mut h, "Multiply", Rect::new(80, 80, 200, 240), [0.8, 0.3, 0.7, 0.8]);
    h.state_mut()
        .session
        .edit("blend", |d, _| {
            d.layer_mut(id).unwrap().blend = photocraft_color::BlendMode::Multiply;
            Ok(())
        })
        .unwrap();
    h.run_steps(3);
    let p = screen(&h, 120.0, 120.0);
    pointer(&mut h, p);
    button(&mut h, p, true);
    pointer(&mut h, p + egui::vec2(19.0, 11.0));
    assert_ne!(h.state().perf.last_refresh, "gpu-move-offset");
    check_pixels(&mut h, &[id], 19, 11);
    button(&mut h, p + egui::vec2(19.0, 11.0), false);
}

#[test]
#[ignore = "interactive 48 MP app benchmark; run explicitly on the owner's RTX 5090"]
fn move_canvas_bench_8000x6000_real_app() {
    let _gpu = crate::gpu_lock();
    let mut h = harness(1.0);
    eprintln!("GPU: {:?}", h.state().perf.gpu_info);
    assert!(h.state().perf.gpu_info.adapter.contains("5090"));
    assert_eq!(h.state().perf.gpu_info.backend, "vulkan");
    setup(&mut h, 8000, 6000, 0.08);
    for i in 0..12 {
        layer(&mut h, "Below", Rect::new(i * 180, i * 120, 4000 + i * 180, 3000 + i * 120), [0.1, 0.4, 0.7, 0.12]);
    }
    let id = layer(&mut h, "48 MP moving layer", Rect::new(0, 0, 8000, 6000), [0.8, 0.2, 0.1, 0.65]);
    for i in 0..12 {
        layer(&mut h, "Above", Rect::new(i * 200, i * 150, 1500 + i * 200, 1200 + i * 150), [0.2, 0.8, 0.4, 0.18]);
    }
    h.state_mut().session.execute("layer.select", json!({"layer": id.0})).unwrap();
    h.run_steps(3);
    h.render().unwrap();
    let p = screen(&h, 4000.0, 3000.0);
    pointer(&mut h, p);
    let t = Instant::now();
    button(&mut h, p, true);
    h.render().unwrap();
    let press = t.elapsed().as_secs_f64() * 1000.0;
    let cache = h.state().perf.spans["move cache"];
    let mut cpu = Vec::new();
    let mut total = Vec::new();
    for i in 1..=120 {
        let t = Instant::now();
        pointer(&mut h, p + egui::vec2(i as f32 * 0.5, (i as f32 * 0.07).sin() * 12.0));
        cpu.push(t.elapsed().as_secs_f64() * 1000.0);
        h.render().unwrap(); // Includes actual GPU execution and screenshot readback.
        total.push(t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(h.state().perf.last_refresh, "gpu-move-offset");
        assert_eq!(h.state().perf.gpu_uploads, 0);
        assert_eq!(h.state().perf.spans["move cache"], cache);
    }
    let summary = |name: &str, mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        eprintln!(
            "{name}: median {:.3} ms, p95 {:.3}, p99 {:.3}, max {:.3} (n={})",
            v[v.len() / 2],
            v[v.len() * 95 / 100],
            v[v.len() * 99 / 100],
            v[v.len() - 1],
            v.len()
        );
    };
    eprintln!(
        "26 layers, 8000x6000, viewport {:?}, zoom 8%; prepared press+render {:.3} ms; idle cache preparation {:.3} ms",
        h.state().last_canvas_rect,
        press,
        cache
    );
    summary("app UI frame", cpu);
    summary("UI + GPU render + screenshot readback", total);
    let t = Instant::now();
    button(&mut h, p + egui::vec2(60.0, 0.0), false);
    h.render().unwrap();
    eprintln!("release + real composite + readback: {:.3} ms", t.elapsed().as_secs_f64() * 1000.0);
}
