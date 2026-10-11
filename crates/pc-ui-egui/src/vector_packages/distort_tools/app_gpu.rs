//! Full PhotocraftApp RTX 5090 gates. Require 00's command, route, display-doc and context hooks.
//! Deliberately fail if integration is absent; never accept software or skipped adapters.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{PhotocraftApp, canvas::ViewXform};
use egui_kittest::Harness;
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
type H = Harness<'static, PhotocraftApp>;
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn harness(tool: Tool, nodes: usize) -> H {
    let mut setup = crate::gpu_canvas::wgpu_setup();
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |a| {
        let info = a.get_info();
        assert!(info.name.contains("RTX 5090"), "RTX 5090 required: {info:?}");
        assert_eq!(info.backend, eframe::wgpu::Backend::Vulkan);
        assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), eframe::wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(a));
        eprintln!("distort41 GPU {info:?}");
    });
    let mut h = Harness::builder().with_size(egui::vec2(1100., 780.)).with_step_dt(1. / 60.).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut doc = Document::with_background("Distort GPU", Size::new(240, 180), ColorMode::Rgb, SampleType::F32, Color::WHITE);
        let points = (0..nodes)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / nodes as f64;
                let r = 1. + 0.025 * (a * 7.).sin();
                (120. + 90. * r * a.cos(), 90. + 65. * r * a.sin())
            })
            .collect::<Vec<_>>();
        let mut shape =
            ShapeLayer { path: Path::new(vec![Subpath::polygon(&points)]), fill: Some(Fill::Solid(Color::rgba(0.1, 0.4, 0.8, 0.65))), ..Default::default() };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
        let l = Layer::new("Curve", LayerContent::Shape(shape));
        let id = l.id;
        doc.layers.push(l);
        let mut s = Session::new();
        s.add_document(doc, None);
        s.select_layer(id).unwrap();
        s.edit_prefs(|p| {
            p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector;
            p.interface.contextual_task_bar = false;
        });
        let mut app = PhotocraftApp::new(s, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("real GPU").clone());
        app.sync_views();
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].center = [120., 90.];
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.extras = false;
        app.ui.tool = crate::state::Tool::Hand;
        activate(&app, &cc.egui_ctx, Some(tool));
        app
    });
    h.run_steps(4);
    assert!(h.state().gpu_active());
    assert!(bridge(&h.ctx).routed, "00 must wire distort_tools::route into the real canvas");
    assert!(photocraft_engine::commands::find("object.liquify").is_some(), "00 must register distort_tools::specs");
    h
}
fn event(h: &mut H, at: [f32; 2], pressed: Option<bool>) {
    let xf = ViewXform::active(h.state()).unwrap();
    let pos = xf.to_screen(at[0], at[1]);
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
fn pixels(h: &mut H) -> Vec<[u8; 4]> {
    h.run_steps(2);
    assert!(h.state().perf.gpu, "actual app canvas must use GPU");
    let img = h.render().expect("GPU readback");
    let xf = ViewXform::active(h.state()).unwrap();
    let ppp = h.ctx.pixels_per_point();
    (0..180)
        .flat_map(|y| (0..240).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = xf.to_screen(x as f32 + 0.5, y as f32 + 0.5) * ppp;
            img.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0
        })
        .collect()
}
fn parity(a: &[[u8; 4]], b: &[[u8; 4]]) {
    assert_eq!(a.len(), b.len());
    let max = a.iter().zip(b).flat_map(|(a, b)| (0..4).map(move |i| a[i].abs_diff(b[i]))).max().unwrap_or(0);
    assert!(max <= 1, "premultiplied max diff {max}/255");
}
#[test]
#[ignore = "speed rules prohibit GPU runs; requires 00 runtime registry/canvas/display hooks"]
fn distort41_gpu_full_app_every_tool_live_pixels_release_one_journal_undo_cancel() {
    let _lock = GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for tool in Tool::ALL {
        let mut h = harness(tool, 64);
        let before = h.state().session.active().unwrap().doc.clone();
        let base = pixels(&mut h);
        let journal = h.state().session.journal.len();
        let history = h.state().session.active().unwrap().history.past_len();
        let start = if tool == Tool::PuppetWarp {
            let q = bridge(&h.ctx).state.pins[0];
            [q.at[0] as f32, q.at[1] as f32]
        } else {
            [205., 90.]
        };
        let end = [start[0] + 6., start[1] + 5.];
        event(&mut h, start, Some(true));
        event(&mut h, end, None);
        assert!(bridge(&h.ctx).state.preview.is_some());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.journal.len(), journal);
        let live = pixels(&mut h);
        assert_ne!(live, base, "live path preview must reach app canvas pixels: {tool:?}");
        event(&mut h, end, Some(false));
        parity(&live, &pixels(&mut h));
        assert_eq!(h.state().session.journal.len(), journal + 1);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        assert!(h.state_mut().session.undo());
        parity(&base, &pixels(&mut h));
        event(&mut h, start, Some(true));
        event(&mut h, end, None);
        h.key_press(Key::Escape);
        h.run_steps(1);
        event(&mut h, end, Some(false));
        assert_eq!(h.state().session.active().unwrap().doc, before);
        parity(&base, &pixels(&mut h));
        assert_eq!(h.state().session.journal.len(), journal + 1);
    }
}
#[test]
#[ignore = "speed rules prohibit release builds/benchmarks; requires integrated 00 app routes"]
#[allow(clippy::assertions_on_constants)]
fn distort41_gpu_release_app_pointer_frames_2000_nodes_at_least_60hz() {
    // This ignored gate must compile in debug but fail if invoked there.
    assert!(!cfg!(debug_assertions), "measure a release build");
    let _lock = GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for tool in Tool::ALL {
        let mut h = harness(tool, 2000);
        let journal = h.state().session.journal.len();
        let start = if tool == Tool::PuppetWarp {
            let p = bridge(&h.ctx).state.pins[0];
            [p.at[0] as f32, p.at[1] as f32]
        } else {
            [205., 90.]
        };
        let first = std::time::Instant::now();
        event(&mut h, start, Some(true));
        h.render().unwrap();
        let first = first.elapsed();
        let mut frames = Vec::new();
        let mut last = start;
        for i in 0..90 {
            let at = [start[0] + (i as f32 * 0.11).sin() * 4., start[1] + (i as f32 * 0.09).sin() * 3.];
            let t = std::time::Instant::now();
            event(&mut h, at, None);
            assert!(bridge(&h.ctx).state.preview.is_some());
            assert!(h.state().perf.gpu);
            h.render().unwrap();
            frames.push(t.elapsed().as_secs_f64() * 1000.);
            last = at;
        }
        frames.sort_by(f64::total_cmp);
        let live = pixels(&mut h);
        let t = std::time::Instant::now();
        // Release at the last actual sample, keeping preview and final pixels identical.
        event(&mut h, last, Some(false));
        h.render().unwrap();
        let release = t.elapsed();
        parity(&live, &pixels(&mut h));
        eprintln!(
            "distort41 {tool:?}: first={first:?}, pointer+app GPU readback ms median={} p95={} p99={}, release={release:?}",
            frames[45], frames[85], frames[89]
        );
        assert!(frames[85] <= 1000. / 60., "2000-node {tool:?} p95 {} ms", frames[85]);
        assert_eq!(h.state().session.journal.len(), journal + 1);
    }
}
