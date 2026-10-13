//! Production-router acceptance, registered by 00 after integration/35.md is applied.
//! This gate deliberately requires the real app state/preview routes. An isolated widget
//! or temporarily replacing the app's authoritative document does not satisfy it.
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
use photocraft_engine::{Session, toolsets::Persona};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, clip_compound::Settings, gpu_canvas::GpuCanvas};
use std::sync::{Arc, Mutex};
type H = Harness<'static, PhotocraftApp>;
fn harness() -> (H, GpuCanvas) {
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
        eprintln!("clip35 full app {info:?}");
    });
    let mut doc = Document::with_background("Clip app", Size::new(160, 160), ColorMode::Rgb, SampleType::F32, Color::WHITE);
    let mut shape = ShapeLayer { path: photocraft_vector::shapes::ellipse(30., 30., 80., 80.), fill: Some(Fill::Solid(Color::WHITE)), ..Default::default() };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
    let ellipse = Layer::new("Ellipse", LayerContent::Shape(shape));
    let id = ellipse.id;
    doc.layers.push(ellipse);
    let mut session = Session::new();
    session.add_document(doc, None);
    session.select_layer(id).unwrap();
    session.tools.foreground = [0., 0., 0., 1.];
    session.edit_prefs(|p| {
        p.toolbar.persona = Persona::Vector;
        p.toolbar.remember_tool(Persona::Vector, "Rectangle");
    });
    let observer = Arc::new(Mutex::new(None));
    let capture = observer.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1000., 720.)).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(session, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("real GPU required");
        *capture.lock().unwrap() = Some(GpuCanvas::new(rs));
        app.set_wgpu(rs.clone());
        app.sync_views();
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].center = [80., 80.];
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.show.layer_edges = false;
        app.ui.view.show.smart_guides = false;
        app.ui.tool = photocraft_ui_egui::state::Tool::Rectangle;
        app.ui.tool_options.shape_fill = true;
        app.ui.tool_options.stroke_width = 0.;
        app.ui.tool_options.corner_radius = 0.;
        app.clip_compound_state.settings = Settings { mode: photocraft_engine::vector_packages::clip_compound::DrawMode::Inside };
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        app
    });
    h.run_steps(4);
    let center = ViewXform::active(h.state()).unwrap().rect.center();
    h.state_mut().ui.views[0].center = [80. + center.x.fract(), 80. + center.y.fract()];
    h.run_steps(3);
    assert!(h.state().gpu_active());
    let gpu = observer.lock().unwrap().clone().expect("actual app GPU observer");
    (h, gpu)
}
fn pointer(h: &mut H, p: Pos2, button: Option<bool>) {
    h.event(Event::PointerMoved(p));
    if let Some(pressed) = button {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
fn assert_canvas(h: &mut H, gpu: &GpuCanvas, doc: &Document) {
    h.state_mut().ui.view.extras = false;
    h.run_steps(3);
    h.render().expect("actual GPU app paint callbacks");
    let expected = photocraft_compose::flatten(doc);
    // Read the actual app's float document texture; an 8-bit screenshot adds
    // display transfer/quantization and is not a linear premultiplied pixel oracle.
    let (_, size, pixels) = gpu.read_texels(doc.id.0).expect("actual app canvas texture");
    assert_eq!(size, [160, 160]);
    assert_eq!(pixels.len(), expected.px.len());
    let mut max = 0f32;
    for (actual, expected) in pixels.iter().zip(&expected.px) {
        for k in 0..4 {
            assert!(actual[k].is_finite());
            max = max.max((actual[k] - if k == 3 { expected[k] } else { expected[k] * expected[3] }).abs());
        }
    }
    assert!(max <= 1. / 255. + 1e-7, "premultiplied app max diff={max}");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
}
#[test]
fn clip35_full_app_gpu_draw_inside_live_pointer_preview_release_cancel_and_undo() {
    let _gpu_gate = crate::gpu_lock();

    let (mut h, gpu) = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let xf = ViewXform::active(h.state()).unwrap();
    let a = xf.to_screen(10., 10.);
    let b = xf.to_screen(130., 130.);
    pointer(&mut h, a, Some(true));
    pointer(&mut h, b, None);
    assert!(h.state().shared_tools.shape_preview().is_some(), "shared Rectangle owns the held preview");
    let ctx = h.ctx.clone();
    let preview = photocraft_ui_egui::canvas::display_doc(h.state_mut(), &ctx, 0).0;
    let center = photocraft_compose::flatten(&preview).get(80, 80);
    assert!(center[..3].iter().all(|c| *c <= 1e-6) && center[3] > 0.99, "visible black draw preview");
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    assert_canvas(&mut h, &gpu, &preview);
    pointer(&mut h, b, Some(false));
    let committed = h.state().session.active().unwrap().doc.clone();
    assert_eq!(h.state().session.journal.len(), 1);
    assert_canvas(&mut h, &gpu, &committed);
    h.key_press_modifiers(Modifiers { alt: true, ..Modifiers::COMMAND }, egui::Key::Num7);
    h.run_steps(2);
    let released = h.state().session.active().unwrap().doc.clone();
    assert_canvas(&mut h, &gpu, &released);
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_canvas(&mut h, &gpu, &before);
    pointer(&mut h, a, Some(true));
    pointer(&mut h, b, None);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    pointer(&mut h, b, Some(false));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_canvas(&mut h, &gpu, &before);
}
