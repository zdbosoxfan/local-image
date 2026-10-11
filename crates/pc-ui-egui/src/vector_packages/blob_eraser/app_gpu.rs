//! Register in tests/gpu/main.rs AFTER applying integration/40.md's production hooks.
//! Requires PhotocraftApp::blob_eraser and Tool::{BlobBrush,VectorEraser}; no widget
//! replacement or authoritative-document swap substitutes for the real app router.
//! Tests are deliberately not executed under the owner's speed override.
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_doc::{Color, ColorMode, Document, SampleType, Size};
use photocraft_engine::{Session, toolsets::Persona, vector_packages::blob_eraser::Mode};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, gpu_canvas::GpuCanvas, state::Tool};
use std::sync::{Arc, Mutex};
type H = Harness<'static, PhotocraftApp>;
fn harness() -> (H, GpuCanvas) {
    let mut setup = photocraft_ui_egui::gpu_canvas::wgpu_setup();
    photocraft_ui_egui::gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        let info = adapter.get_info();
        assert!(info.name.contains("RTX 5090"), "{info:?}");
        assert_eq!(info.backend, eframe::wgpu::Backend::Vulkan);
        assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), eframe::wgpu::TextureFormat::Rgba32Float);
        assert!(photocraft_ui_egui::gpu_canvas::supports_f16_canvas(adapter));
        eprintln!("blob40 full-app hardware {info:?}");
    });
    let mut session = Session::new();
    session.add_document(Document::with_background("Area GPU app", Size::new(160, 160), ColorMode::Rgb, SampleType::F32, Color::WHITE), None);
    session.tools.foreground = [0., 0., 0., 1.];
    session.edit_prefs(|p| {
        p.toolbar.persona = Persona::Vector;
        p.toolbar.remember_tool(Persona::Vector, "blobBrush");
        p.interface.contextual_task_bar = false;
    });
    let observed = Arc::new(Mutex::new(None));
    let observer = observed.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1100., 760.)).with_max_steps(64).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(session, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("real GPU render state");
        *observer.lock().unwrap() = Some(GpuCanvas::new(rs));
        app.set_wgpu(rs.clone());
        app.sync_views();
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].center = [80., 80.];
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.extras = false;
        app.ui.tool = Tool::BlobBrush;
        app.blob_eraser.options.tip.size = 40.;
        app.blob_eraser.options.keep_selected = true;
        app
    });
    h.run_steps(4);
    let center = ViewXform::active(h.state()).unwrap().rect.center();
    h.state_mut().ui.views[0].center = [80. + center.x.fract(), 80. + center.y.fract()];
    h.run_steps(3);
    assert!(h.state().gpu_active());
    let gpu = observed.lock().unwrap().clone().expect("app canvas observer");
    (h, gpu)
}
fn pointer(h: &mut H, p: Pos2, pressed: Option<bool>, mods: Modifiers) {
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerMoved(p));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: mods });
    }
    h.step();
}
fn pixels(h: &mut H, gpu: &GpuCanvas, doc: &Document) -> Vec<[f32; 4]> {
    h.run_steps(2);
    h.render().expect("production GPU paint callback");
    let expected = photocraft_compose::flatten(doc);
    let (_, size, actual) = gpu.read_texels(doc.id.0).expect("actual app float canvas texture");
    assert_eq!(size, [160, 160]);
    assert_eq!(actual.len(), expected.px.len());
    let max =
        actual.iter().zip(&expected.px).flat_map(|(a, b)| (0..4).map(move |i| (a[i] - if i == 3 { b[i] } else { b[i] * b[3] }).abs())).fold(0_f32, f32::max);
    assert!(actual.iter().flatten().all(|v| v.is_finite()));
    assert!(max <= 1. / 255. + 1e-7, "premultiplied max diff={max}");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
    actual
}
#[test]
#[ignore = "owner speed rules: coordinator full-app RTX GPU acceptance after router integration"]
fn blob40_full_app_rtx_live_preview_release_erase_rectangle_cancel_and_undo() {
    let _gpu = crate::gpu_lock();
    let (mut h, gpu) = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let xf = ViewXform::active(h.state()).unwrap();
    let a = xf.to_screen(20., 70.);
    let b = xf.to_screen(140., 70.);
    pointer(&mut h, a, Some(true), Modifiers::NONE);
    pointer(&mut h, b, None, Modifiers::NONE);
    let preview = h.state().blob_eraser.preview().unwrap().document.clone();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    let live = pixels(&mut h, &gpu, &preview);
    assert!(live[70 * 160 + 80][0] < 0.01, "visible live blob");
    pointer(&mut h, b, Some(false), Modifiers::NONE);
    let committed = h.state().session.active().unwrap().doc.clone();
    assert_eq!(live, pixels(&mut h, &gpu, &committed));
    assert_eq!(h.state().session.journal.len(), 1);
    h.state_mut().ui.tool = Tool::VectorEraser;
    h.state_mut().blob_eraser.mode = Mode::Eraser;
    for rectangle in [false, true] {
        let a = xf.to_screen(if rectangle { 70. } else { 80. }, 20.);
        let b = xf.to_screen(if rectangle { 90. } else { 80. }, 120.);
        let mods = if rectangle { Modifiers::ALT } else { Modifiers::NONE };
        pointer(&mut h, a, Some(true), Modifiers::NONE);
        pointer(&mut h, b, None, mods);
        let preview = h.state().blob_eraser.preview().unwrap().document.clone();
        assert_eq!(h.state().session.active().unwrap().doc, committed);
        let live = pixels(&mut h, &gpu, &preview);
        pointer(&mut h, b, Some(false), mods);
        let erased = h.state().session.active().unwrap().doc.clone();
        assert_eq!(live, pixels(&mut h, &gpu, &erased));
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, committed);
    }
    pointer(&mut h, a, Some(true), Modifiers::NONE);
    pointer(&mut h, b, None, Modifiers::NONE);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    pointer(&mut h, b, Some(false), Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, committed);
    pixels(&mut h, &gpu, &committed);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
#[ignore = "owner speed rules: coordinator release real-app pointer-frame measurement"]
fn blob40_full_app_release_pointer_frame_measurement() {
    assert!(!cfg!(debug_assertions), "performance gate requires a release build");
    let _gpu = crate::gpu_lock();
    let (mut h, gpu) = harness();
    let xf = ViewXform::active(h.state()).unwrap();
    h.state_mut().blob_eraser.options.tip.size = 12.;
    pointer(&mut h, xf.to_screen(20., 70.), Some(true), Modifiers::NONE);
    let mut times = Vec::new();
    for i in 1..=32 {
        let start = std::time::Instant::now();
        pointer(&mut h, xf.to_screen(20. + i as f32 * 3.5, 70. + (i as f32 * 0.3).sin() * 12.), None, Modifiers::NONE);
        h.render().unwrap();
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let start = std::time::Instant::now();
    pointer(&mut h, xf.to_screen(132., 70. + (32_f32 * 0.3).sin() * 12.), Some(false), Modifiers::NONE);
    h.render().unwrap();
    let release = start.elapsed().as_secs_f64() * 1000.;
    let doc = h.state().session.active().unwrap().doc.clone();
    pixels(&mut h, &gpu, &doc);
    times.sort_by(f64::total_cmp);
    eprintln!("blob40 actual app pointer frames n=32 median_ms={} p95_ms={} p99_ms={} release_ms={release}", times[16], times[30], times[31]);
}
