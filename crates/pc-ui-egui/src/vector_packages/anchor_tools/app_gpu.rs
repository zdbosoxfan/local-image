//! Full-app RTX 5090 acceptance. Compiled here; execution awaits 00's route/display hooks.
//! These tests deliberately fail without those hooks or with a software adapter.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{PhotocraftApp, canvas::ViewXform};
use egui_kittest::Harness;
use photocraft_doc::{Color, ColorMode, Layer, Path, SampleType, ShapeLayer, Size, Subpath};
type H = Harness<'static, PhotocraftApp>;
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn harness(tool: Tool) -> H {
    let mut setup = crate::gpu_canvas::wgpu_setup();
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |a| {
        let info = a.get_info();
        assert!(info.name.contains("RTX 5090"), "RTX 5090 required: {info:?}");
        assert_eq!(info.backend, eframe::wgpu::Backend::Vulkan);
        assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), eframe::wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(a));
        eprintln!("anchor33 app GPU: {info:?}");
    });
    let mut h = Harness::builder().with_size(egui::vec2(1100., 780.)).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut doc = Document::with_background("Anchor GPU", Size::new(240, 180), ColorMode::Rgb, SampleType::F32, Color::WHITE);
        let mut shape = ShapeLayer {
            path: Path::new(vec![Subpath::polyline(&[(20., 90.), (120., 90.), (220., 90.)])]),
            stroke: Some(ShapeStroke { width: 6., paint: Fill::Solid(Color::rgb(0.1, 0.4, 0.8)), opacity: 0.65, ..Default::default() }),
            ..Default::default()
        };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
        let l = Layer::new("Curve", LayerContent::Shape(shape));
        let id = l.id;
        doc.layers.push(l);
        let mut s = Session::new();
        s.add_document(doc, None);
        s.select_layer(id).unwrap();
        s.edit_prefs(|p| p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector);
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
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        activate(&app, &cc.egui_ctx, Some(tool));
        app
    });
    h.run_steps(4);
    assert!(h.state().gpu_active());
    assert!(bridge(&h.ctx).routed, "00 must wire anchor_tools::route into the real canvas");
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
    h.run_steps(3);
    assert!(h.state().perf.gpu, "actual app canvas must use GPU");
    let img = h.render().expect("GPU readback");
    let xf = ViewXform::active(h.state()).unwrap();
    let ppp = h.ctx.pixels_per_point();
    let mut pixels = Vec::new();
    for y in 0..180 {
        for x in 0..240 {
            let p = xf.to_screen(x as f32 + 0.5, y as f32 + 0.5) * ppp;
            pixels.push(img.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0);
        }
    }
    pixels
}
fn parity(a: &[[u8; 4]], b: &[[u8; 4]]) {
    assert_eq!(a.len(), b.len());
    let max = a.iter().zip(b).flat_map(|(a, b)| (0..4).map(move |i| a[i].abs_diff(b[i]))).max().unwrap_or(0);
    assert!(max <= 1, "premultiplied max diff {max}/255");
}
#[test]
#[ignore = "speed rules prohibit GPU runs; requires 00 app routing/display hooks"]
fn anchor33_gpu_full_app_add_delete_convert_preview_release_and_cancel() {
    let _lock = GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for (tool, start, end) in
        [(Tool::AddAnchor, [70., 90.], [70., 90.]), (Tool::DeleteAnchor, [120., 90.], [120., 90.]), (Tool::AnchorPoint, [120., 90.], [140., 120.])]
    {
        let mut h = harness(tool);
        let before = h.state().session.active().unwrap().doc.clone();
        let baseline = pixels(&mut h);
        let journal = h.state().session.journal.len();
        let history = h.state().session.active().unwrap().history.past_len();
        event(&mut h, start, Some(true));
        event(&mut h, end, None);
        assert!(bridge(&h.ctx).state.preview.is_some());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.journal.len(), journal);
        let live = pixels(&mut h);
        if tool == Tool::AnchorPoint {
            assert_ne!(live, baseline, "live geometry must reach canvas pixels");
        }
        event(&mut h, end, Some(false));
        parity(&live, &pixels(&mut h));
        assert_eq!(h.state().session.journal.len(), journal + 1);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        assert!(h.state_mut().session.undo());
        parity(&baseline, &pixels(&mut h));
        event(&mut h, start, Some(true));
        event(&mut h, end, None);
        h.key_press(Key::Escape);
        h.run_steps(2);
        event(&mut h, end, Some(false));
        assert_eq!(h.state().session.active().unwrap().doc, before);
        parity(&baseline, &pixels(&mut h));
        assert_eq!(h.state().session.journal.len(), journal + 1);
    }
}
#[test]
#[ignore = "speed rules prohibit GPU runs; requires 00 app routing/display hooks"]
fn anchor33_gpu_full_app_curvature_preview_finish_one_journal_and_undo() {
    let _lock = GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Tool::Curvature);
    let before = h.state().session.active().unwrap().doc.clone();
    let baseline = pixels(&mut h);
    for p in [[30., 30.], [110., 140.], [200., 30.]] {
        event(&mut h, p, Some(true));
        event(&mut h, p, Some(false));
    }
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    assert!(bridge(&h.ctx).state.preview.is_some());
    // Hide controls via the normal Extras route, and disable the optional rubber band so
    // the pixel comparison covers filled app preview rather than feedback strokes.
    let mut b = bridge(&h.ctx);
    b.state.options.rubber_band = false;
    save_bridge(&h.ctx, b);
    h.hover_at(egui::pos2(1000., 15.));
    let live = pixels(&mut h);
    assert_ne!(baseline, live);
    h.key_press(Key::Escape);
    h.run_steps(2);
    parity(&live, &pixels(&mut h));
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    parity(&baseline, &pixels(&mut h));
    // Existing-path edits use the registered Curvature command, including a fresh
    // Delete immediately after release. This also gates 00's command registration.
    event(&mut h, [120., 90.], Some(true));
    event(&mut h, [140., 120.], None);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    let live_edit = pixels(&mut h);
    assert_ne!(baseline, live_edit);
    event(&mut h, [140., 120.], Some(false));
    parity(&live_edit, &pixels(&mut h));
    assert_eq!(h.state().session.journal.len(), 2);
    assert!(bridge(&h.ctx).state.active());
    assert!(!bridge(&h.ctx).state.captured());
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(h.state().session.journal.len(), 3);
    let st = h.state().session.active().unwrap();
    let LayerContent::Shape(sh) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!() };
    assert_eq!(sh.path.subpaths[0].knots.len(), 2);
    parity(&baseline, &pixels(&mut h));
    assert!(h.state_mut().session.undo());
    parity(&live_edit, &pixels(&mut h));
    assert!(h.state_mut().session.undo());
    parity(&baseline, &pixels(&mut h));
}
