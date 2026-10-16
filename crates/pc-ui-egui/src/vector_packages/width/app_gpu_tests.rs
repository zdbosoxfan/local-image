//! Full-app width gate for 00's serialized tests/gpu/main.rs registration after router wiring.
//! Uses actual app input, preview selection, GPU canvas and release. No desktop windows.
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, ShapeStroke, Size, Subpath};
use photocraft_engine::Session;
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, state::Tool};

type H = Harness<'static, PhotocraftApp>;
fn harness() -> H {
    let mut setup = photocraft_ui_egui::gpu_canvas::wgpu_setup();
    photocraft_ui_egui::gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        let info = adapter.get_info();
        #[cfg(not(target_os = "macos"))]
        assert!(info.name.contains("RTX 5090"), "RTX 5090 required: {info:?}");
        assert_eq!(info.backend, if cfg!(target_os = "macos") { eframe::wgpu::Backend::Metal } else { eframe::wgpu::Backend::Vulkan });
        #[cfg(not(target_os = "macos"))]
        assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        #[cfg(target_os = "macos")]
        assert_ne!(info.device_type, eframe::wgpu::DeviceType::Cpu, "hardware Metal adapter required");
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), eframe::wgpu::TextureFormat::Rgba32Float);
        assert!(photocraft_ui_egui::gpu_canvas::supports_f16_canvas(adapter));
        eprintln!("width full-app GPU: {info:?}");
    });
    let mut doc = Document::with_background("Width GPU", Size::new(160, 120), ColorMode::Rgb, SampleType::F32, Color::WHITE);
    let mut sh = ShapeLayer {
        path: Path::new(vec![Subpath::polyline(&[(20.0, 60.0), (140.0, 60.0)])]),
        stroke: Some(ShapeStroke { width: 10.0, paint: Fill::Solid(Color::rgb(0.1, 0.4, 0.8)), opacity: 0.65, ..Default::default() }),
        ..Default::default()
    };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
    let l = Layer::new("Line", LayerContent::Shape(sh));
    let id = l.id;
    doc.layers.push(l);
    let mut session = Session::new();
    session.add_document(doc, None);
    session.select_layer(id).unwrap();
    session.edit_prefs(|p| {
        p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector;
        p.toolbar.vector_tool = "Width".into();
    });
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 780.0)).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(session, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("real GPU required").clone());
        app.sync_views();
        app.ui.views[0].zoom = 1.0;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].center = [80.0, 60.0];
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.extras = false;
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        // 01 registers this tool; 00 maps it to width_state lifecycle/input/display_doc hooks.
        app.ui.tool = Tool::Width;
        app
    });
    h.run_steps(4);
    assert!(h.state().gpu_active());
    #[cfg(not(target_os = "macos"))]
    assert!(h.state().perf.gpu_info.adapter.contains("RTX 5090"));
    assert_eq!(h.state().perf.gpu_info.backend, if cfg!(target_os = "macos") { "metal" } else { "vulkan" });
    h
}
fn pixels(h: &mut H) -> Vec<[u8; 4]> {
    h.run_steps(3);
    h.render().expect("actual RTX app readback");
    assert!(h.state().perf.gpu);
    let id = h.state().session.active().unwrap().doc.id;
    let (_, size, pixels) = h.state().read_canvas_texels(id.0).expect("actual canvas texture");
    assert_eq!(size, [160, 120]);
    pixels.into_iter().map(|p| p.map(|c| (c.clamp(0., 1.) * 255.).round() as u8)).collect()
}

fn parity(a: &[[u8; 4]], b: &[[u8; 4]]) {
    assert_eq!(a.len(), b.len());
    // kittest render returns premultiplied bytes; do not multiply them a second time.
    let max = a.iter().zip(b).flat_map(|(a, b)| (0..4).map(move |i| f32::from(a[i].abs_diff(b[i])) / 255.0)).fold(0.0, f32::max);
    assert!(max <= 1.0 / 255.0 + f32::EPSILON, "premultiplied canvas diff {max}");
}
fn event(h: &mut H, p: Pos2, pressed: Option<bool>) {
    match pressed {
        Some(pressed) => h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }),
        None => h.event(Event::PointerMoved(p)),
    }
    h.step();
}
#[test]
fn width_gpu_full_app_live_preview_release_cancel_and_undo_parity() {
    let _lock = super::gpu_lock();
    let mut h = harness();
    let baseline = pixels(&mut h);
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    let history = h.state().session.active().unwrap().history.past_len();
    let xf = ViewXform::active(h.state()).unwrap();
    let start = xf.to_screen(80.0, 60.0);
    let end = xf.to_screen(80.0, 40.0);
    event(&mut h, start, None);
    event(&mut h, start, Some(true));
    event(&mut h, end, None);
    assert!(
        h.state().width_state.is_dragging(),
        "tool={:?} error={:?} status={} events={:?} start={start:?} canvas={:?}",
        h.state().ui.tool,
        h.state().width_state.error,
        h.state().ui.status,
        h.ctx.input(|i| i.events.clone()),
        h.state().last_canvas_rect
    );
    assert!(h.state().width_state.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), journal);
    let live = pixels(&mut h);
    assert_ne!(baseline, live, "actual canvas must show detached width preview");
    event(&mut h, end, Some(false));
    h.hover_at(egui::pos2(1000.0, 15.0));
    parity(&live, &pixels(&mut h));
    assert_eq!(h.state().session.journal.len(), journal + 1);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    assert!(h.state_mut().session.undo());
    parity(&baseline, &pixels(&mut h));
    event(&mut h, start, None);
    event(&mut h, start, Some(true));
    event(&mut h, end, None);
    h.key_press(Key::Escape);
    h.run_steps(2);
    event(&mut h, end, Some(false));
    parity(&baseline, &pixels(&mut h));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
}
