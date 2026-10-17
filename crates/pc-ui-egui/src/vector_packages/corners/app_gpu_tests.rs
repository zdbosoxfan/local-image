//! Corners full-app gate, registered in tests/gpu/main.rs. Uses the actual app canvas,
//! actual pointer capture, normal shape-split GPU plan and release. No desktop window or skip.
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_doc::vector_design::drawing::CornerKind;
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
use photocraft_engine::{Session, vector_packages::corners::project_document};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform};
use std::time::Instant;

type H = Harness<'static, PhotocraftApp>;
fn harness(kind: CornerKind, star: bool) -> H {
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
        eprintln!("corner app GPU: {info:?}");
    });
    let mut doc = Document::with_background("Corners GPU", Size::new(128, 128), ColorMode::Rgb, SampleType::F32, Color::WHITE);
    let path = if star {
        photocraft_vector::shapes::polygon(14.0, 14.0, 100.0, 100.0, 5, 0.45)
    } else {
        Path::new(vec![Subpath::polygon(&[(14.0, 14.0), (114.0, 14.0), (114.0, 114.0), (14.0, 114.0)])])
    };
    let mut shape = ShapeLayer { path, fill: Some(Fill::Solid(Color::rgba(0.16, 0.47, 0.82, 0.7))), ..Default::default() };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
    let layer = Layer::new("Corners", LayerContent::Shape(shape));
    let id = layer.id;
    doc.layers.push(layer);
    let mut session = Session::new();
    session.add_document(doc, None);
    session.select_layer(id).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 780.0)).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(session, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("real GPU required").clone());
        app.sync_views();
        app.ui.views[0].zoom = 1.0;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].center = [64.0, 64.0];
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.show.layer_edges = false;
        app.ui.view.show.smart_guides = false;
        app.ui.tool_options.move_auto_select = false;
        app.ui.tool_options.move_show_transform = false;
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        app.ui.tool = photocraft_ui_egui::state::Tool::Corner;
        app.shared_tools.corners.layer = Some(id);
        app.shared_tools.corners.kind = kind;
        app.shared_tools.corners.active = true;
        app
    });
    h.run_steps(3);
    // Compare native canvas pixels at exactly aligned source-pixel centres; no resampling oracle
    // is needed. Separate scaled-view tests own 2/4/8x coverage.
    let center = xf(&h).rect.center();
    h.state_mut().ui.views[0].center = [64.0 + center.x.fract(), 64.0 + center.y.fract()];
    h.run_steps(2);
    assert!(h.state().gpu_active());
    assert_eq!(xf(&h).zoom, 1.0);
    h
}
fn xf(h: &H) -> ViewXform {
    let app = h.state();
    let v = &app.ui.views[0];
    ViewXform {
        rect: photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect),
        zoom: v.zoom,
        center: v.center,
        flip: app.ui.view.flip_horizontal,
        rotation: v.rotation,
    }
}
fn pointer(h: &mut H, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.step();
}
fn button(h: &mut H, p: Pos2, pressed: bool) {
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}
fn assert_canvas(h: &mut H, expected: &Document) {
    h.render().expect("actual app GPU paint callbacks");
    let (_, size, pixels) = h.state().read_canvas_texels(expected.id.0).expect("actual float canvas texture");
    assert_eq!(size, [expected.size.width, expected.size.height]);
    let cpu = photocraft_compose::render(expected, expected.bounds());
    let mut max = 0.0f32;
    let mut checked = 0;
    for y in 8..120 {
        for x in 8..120 {
            let actual = pixels[(y * size[0] + x) as usize];
            let want = cpu.get(x as i32, y as i32);
            for c in 0..4 {
                assert!(actual[c].is_finite());
                let premultiplied = if c == 3 { want[c] } else { want[c] * want[3] };
                max = max.max((actual[c] - premultiplied).abs());
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 112 * 112);
    assert!(max <= 1. / 255. + 1e-7, "premultiplied app canvas max diff={max}");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
}

#[test]
fn corners_full_app_gpu_preview_release_cancel_and_undo() {
    let _gpu_gate = crate::gpu_lock();

    for (kind, star) in [(CornerKind::Round, true), (CornerKind::Inverted, false)] {
        let mut h = harness(kind, star);
        let before = h.state().session.active().unwrap().doc.clone();
        let (handle, p) = h.state().shared_tools.corners.widgets(&h.state().session, 1.0)[0];
        let start = xf(&h).to_screen(p.x as f32, p.y as f32);
        let delta = egui::vec2((handle.u.x + handle.v.x) as f32, (handle.u.y + handle.v.y) as f32) * 16.0;
        pointer(&mut h, start);
        button(&mut h, start, true);
        pointer(&mut h, start + delta);
        assert!(h.state().shared_tools.corners.is_dragging());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
        let preview = h.state().shared_tools.corners.display_document(&h.state().session).unwrap().0;
        assert_ne!(preview.as_ref(), before.as_ref());
        assert_canvas(&mut h, &preview);
        button(&mut h, start + delta, false);
        assert!(h.state().shared_tools.corners.error.is_none(), "{:?}", h.state().shared_tools.corners.error);
        assert!(!h.state().shared_tools.corners.is_dragging());
        assert_eq!(h.state().session.journal.len(), 1);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), 1);
        let committed = project_document(&h.state().session.active().unwrap().doc).unwrap();
        assert_eq!(committed, *preview);
        assert_canvas(&mut h, &committed);
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(!h.state_mut().session.undo());
        h.run_steps(2);
        pointer(&mut h, start);
        button(&mut h, start, true);
        pointer(&mut h, start + delta);
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        button(&mut h, start + delta, false);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.journal.len(), 1);
        assert_canvas(&mut h, &before);
    }
}
#[test]
#[ignore = "release real-app pointer-frame benchmark; run explicitly on RTX 5090"]
// This ignored benchmark must compile in debug, but must refuse execution there.
#[allow(clippy::assertions_on_constants)]
fn corners_full_app_gpu_pointer_frame_measurements() {
    let _gpu_gate = crate::gpu_lock();

    assert!(!cfg!(debug_assertions), "release build required");

    let mut h = harness(CornerKind::Round, true);
    let (handle, p) = h.state().shared_tools.corners.widgets(&h.state().session, 1.0)[0];
    let start = xf(&h).to_screen(p.x as f32, p.y as f32);
    let delta = egui::vec2((handle.u.x + handle.v.x) as f32, (handle.u.y + handle.v.y) as f32) * 16.0;
    pointer(&mut h, start);
    let t = Instant::now();
    button(&mut h, start, true);
    h.render().unwrap();
    let first = t.elapsed().as_secs_f64() * 1000.0;
    assert!(h.state().shared_tools.corners.is_dragging());
    let mut pointer_ms = Vec::new();
    let mut render_ms = Vec::new();
    for i in 0..40 {
        let t = Instant::now();
        pointer(&mut h, start + delta * ((i % 20) as f32 / 20.0));
        pointer_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        assert!(h.state().shared_tools.corners.is_dragging());
        assert!(h.state().session.journal.is_empty());
        let t = Instant::now();
        h.render().unwrap();
        render_ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let t = Instant::now();
    button(&mut h, start + delta, false);
    h.render().unwrap();
    let release = t.elapsed().as_secs_f64() * 1000.0;
    assert!(!h.state().shared_tools.corners.is_dragging());
    assert_eq!(h.state().session.journal.len(), 1);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 1);
    pointer_ms.sort_by(f64::total_cmp);
    render_ms.sort_by(f64::total_cmp);
    eprintln!(
        "corner app GPU {:?}; press={first}ms release={release}ms; UI median/p95/p99={}/{}/{}ms; GPU/readback median/p95/p99={}/{}/{}ms; uploads={}",
        h.state().perf.gpu_info,
        pointer_ms[20],
        pointer_ms[37],
        pointer_ms[39],
        render_ms[20],
        render_ms[37],
        render_ms[39],
        h.state().perf.gpu_uploads
    );
}
