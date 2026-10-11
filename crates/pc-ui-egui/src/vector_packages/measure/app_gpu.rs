//! Register in tests/gpu/main.rs after applying integration/44.md's REAL canvas hook.
//! These tests intentionally fail if activation never reaches production route().
//! Owner speed override forbids executing these GPU/release gates in package jobs.
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
use photocraft_doc::{Color, ColorMode, Document, Layer, LayerContent, SampleType, ShapeLayer, Size};
use photocraft_engine::{Session, toolsets::Persona};
use photocraft_ui_egui::{
    PhotocraftApp,
    canvas::ViewXform,
    gpu_canvas::GpuCanvas,
    measure::{self, Mode},
};
use std::sync::{Arc, Mutex};
type H = Harness<'static, PhotocraftApp>;
fn harness() -> (H, GpuCanvas) {
    let mut setup = photocraft_ui_egui::gpu_canvas::wgpu_setup();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(c) = &mut setup {
        c.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN;
    }
    photocraft_ui_egui::gpu_canvas::use_adapter_limits_with(&mut setup, |a| {
        let info = a.get_info();
        assert_eq!(info.backend, eframe::wgpu::Backend::Vulkan);
        assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        assert!(info.name.contains("RTX 5090"), "{info:?}");
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), eframe::wgpu::TextureFormat::Rgba32Float);
        eprintln!("measure44 hardware: {info:?}");
    });
    let observed = Arc::new(Mutex::new(None));
    let observer = observed.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1200., 800.)).with_max_steps(64).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut s = Session::new();
        let mut d = Document::with_background("Measure GPU", Size::new(400, 300), ColorMode::Rgb, SampleType::F32, Color::WHITE);
        let mut sh = ShapeLayer {
            path: photocraft_vector::shapes::rect(20., 20., 100., 50.),
            fill: Some(photocraft_doc::Fill::Solid(Color::BLACK)),
            ..Default::default()
        };
        sh.cache = Some(photocraft_vector::render_shape(&sh, d.pixel_format(), d.bounds()));
        d.layers.push(Layer::new("Rectangle", LayerContent::Shape(sh)));
        s.add_document(d, None);
        s.edit_prefs(|p| {
            p.toolbar.persona = Persona::Vector;
            p.interface.contextual_task_bar = false;
        });
        let mut app = PhotocraftApp::new(s, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("real app renderer");
        *observer.lock().unwrap() = Some(GpuCanvas::new(rs));
        app.set_wgpu(rs.clone());
        app.sync_views();
        app.ui.extras.rulers = false;
        app.ui.view.extras = false;
        app.ui.tool_options.move_show_transform = false;
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].center = [200., 150.];
        app
    });
    h.run_steps(5);
    assert!(h.state().gpu_active());
    let gpu = observed.lock().unwrap().clone().unwrap();
    (h, gpu)
}
fn pointer(h: &mut H, p: [f32; 2], button: Option<bool>) {
    let pos = ViewXform::active(h.state()).unwrap().to_screen(p[0], p[1]);
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = button {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
fn pixels(h: &mut H, gpu: &GpuCanvas) -> Vec<[f32; 4]> {
    h.render().unwrap();
    let st = h.state().session.active().unwrap();
    let (_, _, actual) = gpu.read_texels(st.doc.id.0).expect("actual float canvas");
    let reference = photocraft_compose::flatten(&st.doc);
    let max =
        actual.iter().zip(&reference.px).flat_map(|(a, b)| (0..4).map(move |i| (a[i] - if i == 3 { b[i] } else { b[i] * b[3] }).abs())).fold(0_f32, f32::max);
    assert!(max <= 1. / 255. + 1e-7, "premultiplied diff={max}");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
    actual
}
#[test]
#[ignore = "owner speed rules: full-app RTX acceptance after 00 canvas registration"]
fn measure44_full_app_rtx_overlay_live_release_hover_and_no_layer_render() {
    let _lock = crate::gpu_lock();
    let (mut h, gpu) = harness();
    let ctx = h.ctx.clone();
    measure::activate(h.state(), &ctx, Mode::Measure);
    h.run_steps(3);
    let doc = h.state().session.active().unwrap().doc.clone();
    let rev = h.state().session.active().unwrap().revision;
    let journal = h.state().session.journal.len();
    let cache = pixels(&mut h, &gpu);
    h.state_mut().perf.last_refresh = ""; // Sentinel: Perf records only real canvas refreshes.
    pointer(&mut h, [121., 71.], None);
    let before = h.render().unwrap();
    pointer(&mut h, [19., 19.], Some(true));
    pointer(&mut h, [121., 71.], None);
    let b = measure::bridge(&h.ctx);
    assert!(b.state.captured(), "00 must wire real canvas route");
    assert_eq!(b.state.line.as_ref().unwrap().b, [120., 70.]);
    let live = h.render().unwrap();
    assert_ne!(before.as_raw(), live.as_raw(), "measure overlay must reach GPU paint");
    assert_eq!(pixels(&mut h, &gpu), cache);
    assert_eq!(h.state().perf.last_refresh, "", "overlay must not re-render document layers");
    pointer(&mut h, [121., 71.], Some(false));
    assert!(!measure::bridge(&h.ctx).state.captured());
    assert_eq!(measure::bridge(&h.ctx).state.line, b.state.line);
    pixels(&mut h, &gpu);
    measure::activate(h.state(), &ctx, Mode::Area);
    pointer(&mut h, [60., 20.], None);
    assert_eq!(measure::bridge(&h.ctx).state.area.unwrap().area, 5000.);
    h.render().unwrap();
    assert_eq!(pixels(&mut h, &gpu), cache);
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    assert_eq!(h.state().session.journal.len(), journal);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 0);
}
#[test]
#[ignore = "owner speed rules: release real-app pointer-frame benchmark at integration"]
fn measure44_full_app_release_warm_pointer_frames_under_16ms() {
    assert!(!cfg!(debug_assertions), "release required");
    let _lock = crate::gpu_lock();
    let (mut h, gpu) = harness();
    let ctx = h.ctx.clone();
    measure::activate(h.state(), &ctx, Mode::Measure);
    h.run_steps(3);
    pointer(&mut h, [20., 20.], Some(true));
    assert!(measure::bridge(&ctx).state.captured(), "production route required");
    h.render().unwrap();
    h.state_mut().perf.last_refresh = "";
    let mut times = Vec::new();
    for i in 0..35 {
        let now = std::time::Instant::now();
        pointer(&mut h, [150. + i as f32, 100. + (i % 2) as f32], None);
        h.render().unwrap();
        if i >= 5 {
            times.push(now.elapsed().as_secs_f64() * 1000.);
        }
        assert_eq!(h.state().perf.last_refresh, "");
    }
    let now = std::time::Instant::now();
    pointer(&mut h, [184., 100.], Some(false));
    h.render().unwrap();
    let release = now.elapsed().as_secs_f64() * 1000.;
    pixels(&mut h, &gpu);
    times.sort_by(f64::total_cmp);
    eprintln!(
        "measure44 n=30 warm pointer+GPU/readback median={} p95={} p99={} release={release} hardware={:?}",
        times[15],
        times[28],
        times[29],
        h.state().perf.gpu_info
    );
    assert!(times[28] < 16., "warm p95={}ms", times[28]);
}
