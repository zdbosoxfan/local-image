//! Integration gates, deliberately ignored under the owner's 2026-10-10 speed rules.
//! Production app only: no helper calls that bypass missing 00 routes, software skips or
//! library-only timing. Adapter/float checks are mandatory when the coordinator runs them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{PhotocraftApp, canvas::ViewXform, state::Tool};
use egui::{Event, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, SampleType, ShapeLayer, Size};
use photocraft_engine::{Session, toolsets::Persona};
use std::{sync::Arc, time::Instant};
type H = Harness<'static, PhotocraftApp>;
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn harness(count: usize) -> H {
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.name.contains("RTX 5090") && i.backend == eframe::wgpu::Backend::Vulkan && i.device_type == eframe::wgpu::DeviceType::DiscreteGpu
            })
            .ok_or_else(|| "view37 requires RTX 5090 / Vulkan / discrete adapter".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), eframe::wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(adapter));
        eprintln!("view37 GPU hardware {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    let mut setup = eframe::egui_wgpu::WgpuSetup::CreateNew(setup);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |_| {});
    let mut h = Harness::builder().with_size(egui::vec2(1280., 900.)).with_step_dt(1. / 60.).with_max_steps(64).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut doc = Document::new("View37 GPU", Size::new(640, 480), ColorMode::Rgb, SampleType::U8);
        let mut layers = Vec::new();
        for i in 0..count {
            let (x, y) =
                if count == 2 { (20.5 + (i as f64) * 20., 20.5 + (i as f64) * 20.) } else { (5.5 + (i % 100) as f64 * 6., 5.5 + (i / 100) as f64 * 8.) };
            let mut s = ShapeLayer {
                path: photocraft_vector::shapes::rect(x, y, if count == 2 { 80. } else { 4. }, if count == 2 { 80. } else { 5. }),
                fill: Some(Fill::Solid(Color::rgb(1., 0., 0.))),
                ..Default::default()
            };
            photocraft_engine::vector_cmds::refresh_shape(&doc, &mut s);
            layers.push(Layer::new("Path", LayerContent::Shape(s)));
        }
        doc.layers = vec![Layer::group("Isolate test group", layers)];
        let mut session = Session::new();
        session.add_document(doc, None);
        session.edit_prefs(|p| {
            p.toolbar.persona = Persona::Vector;
            p.transparency_and_gamut.grid_size = photocraft_engine::prefs::CheckerSize::None;
            p.interface.contextual_task_bar = false;
        });
        let mut app = PhotocraftApp::new(session, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().unwrap().clone());
        app.sync_views();
        app.ui.tool = Tool::Hand;
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].center = [320., 240.];
        app.ui.views[0].fit_pending = false;
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.show.layer_edges = false;
        app.ui.view.show.smart_guides = false;
        app
    });
    h.run_steps(3);
    assert!(h.state().gpu_active());
    h
}
fn toggle(h: &mut H, alt: bool) {
    h.key_press_modifiers(Modifiers { ctrl: true, command: true, alt, ..Modifiers::NONE }, Key::Y);
    h.run_steps(3);
}
fn pointer(h: &mut H, p: [f32; 2], press: Option<bool>) {
    let pos = ViewXform::active(h.state()).unwrap().to_screen(p[0], p[1]);
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = press {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
fn click(h: &mut H, p: [f32; 2]) {
    pointer(h, p, Some(true));
    pointer(h, p, Some(false));
}
fn double_click(h: &mut H, p: [f32; 2]) {
    h.run_steps(40); // Independent gesture, outside egui's triple-click history.
    click(h, p);
    click(h, p);
    h.run_steps(3);
}
fn canvas_pixels(h: &mut H) -> image::RgbaImage {
    let image = h.render().unwrap();
    assert!(h.state().perf.gpu);
    assert!(h.state().perf.gpu_fallback.is_none());
    image
}
#[test]
#[ignore = "owner speed rules; requires 00 view/overlay routes and real RTX 5090"]
fn view37_full_app_gpu_outline_deterministic_pixels_and_pixel_preview_cpu_parity() {
    let _lock = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(2);
    let source = h.state().session.active().unwrap().doc.clone();
    let rev = h.state().session.active().unwrap().revision;
    toggle(&mut h, false);
    assert_eq!(crate::isolation::bridge(&h.ctx).views.mode, Mode::Outline, "00 shortcut route is required");
    let first = canvas_pixels(&mut h);
    h.step();
    let second = canvas_pixels(&mut h);
    let xf = ViewXform::active(h.state()).unwrap();
    let origin = xf.to_screen(0., 0.);
    let frame = prepare(
        &source,
        &crate::isolation::bridge(&h.ctx).views,
        Scope { document: source.id, target: None },
        Affine::translate(f64::from(origin.x), f64::from(origin.y)).mul(&Affine::scale(f64::from(xf.zoom))),
        Rect::new(0, 0, first.width() as i32, first.height() as i32),
        &NativeGeometry,
    )
    .unwrap();
    let cpu = frame.overlay_buffer(Rect::new(0, 0, first.width() as i32, first.height() as i32)).unwrap();
    // Sample only the artwork, away from UI, guides, cursor and canvas boundaries.
    for y in 8..140 {
        for x in 8..140 {
            let p = xf.to_screen(x as f32, y as f32);
            let (sx, sy) = (p.x.floor() as u32, p.y.floor() as u32);
            assert_eq!(first.get_pixel(sx, sy), second.get_pixel(sx, sy));
            let expected = ((1. - cpu.get(sx as i32, sy as i32)[3]) * 255.).round() as i16;
            for c in 0..3 {
                assert!((i16::from(first.get_pixel(sx, sy)[c]) - expected).abs() <= 1, "device outline parity");
            }
        }
    }
    toggle(&mut h, true);
    assert_eq!(crate::isolation::bridge(&h.ctx).views.mode, Mode::PixelPreview);
    h.state_mut().ui.views[0].zoom = 8.;
    h.state_mut().ui.views[0].center = [70., 70.];
    h.run_steps(3);
    let image = canvas_pixels(&mut h);
    let xf = ViewXform::active(h.state()).unwrap();
    let cpu = photocraft_compose::flatten(&source);
    for y in 35..105 {
        for x in 35..105 {
            for offset in [0.125, 0.875] {
                let p = xf.to_screen(x as f32 + offset, y as f32 + offset);
                let color = cpu.get(x, y);
                let alpha = color[3];
                for (c, component) in color.iter().take(3).enumerate() {
                    let expected = ((*component * alpha + 1. - alpha) * 255.).round() as i16;
                    assert!((i16::from(image.get_pixel(p.x as u32, p.y as u32)[c]) - expected).abs() <= 1);
                }
            }
        }
    }
    assert_eq!(h.state().session.active().unwrap().doc, source);
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    assert!(h.state().session.journal.is_empty());
}
#[test]
#[ignore = "owner speed rules; requires 00 isolation/drawing routes and real RTX 5090"]
fn view37_full_app_gpu_isolate_live_rectangle_cancel_release_escape_and_persona_switch() {
    let _lock = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(2);
    h.state_mut().ui.tool = Tool::PathSelection;
    h.step();
    let before = h.state().session.active().unwrap().doc.clone();
    let group = before.layers[0].id;
    let rev = h.state().session.active().unwrap().revision;
    double_click(&mut h, [50., 50.]);
    assert!(crate::isolation::bridge(&h.ctx).isolation.scope.current().is_some(), "00 double-click route required");
    h.state_mut().ui.tool = Tool::Rectangle;
    h.step();
    pointer(&mut h, [150., 140.], Some(true));
    pointer(&mut h, [180., 170.], None);
    canvas_pixels(&mut h);
    h.key_press(Key::Escape);
    pointer(&mut h, [180., 170.], Some(false));
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    h.state_mut().ui.tool = Tool::PathSelection;
    h.step();
    double_click(&mut h, [50., 50.]);
    h.state_mut().ui.tool = Tool::Rectangle;
    h.step();
    pointer(&mut h, [150., 140.], Some(true));
    for p in [[160., 150.], [170., 160.], [180., 170.]] {
        pointer(&mut h, p, None);
        canvas_pixels(&mut h);
    }
    pointer(&mut h, [180., 170.], Some(false));
    h.run_steps(3);
    canvas_pixels(&mut h);
    assert_eq!(h.state().session.active().unwrap().doc.layer(group).unwrap().children().unwrap().len(), 3);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 1);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(crate::isolation::bridge(&h.ctx).isolation.scope.current().is_none());
    h.state_mut().ui.tool = Tool::PathSelection;
    h.step();
    double_click(&mut h, [50., 50.]);
    assert!(crate::isolation::bridge(&h.ctx).isolation.scope.current().is_some());
    h.get_by_label("Pixel persona").click();
    h.run_steps(3);
    assert!(crate::isolation::bridge(&h.ctx).isolation.scope.current().is_none());
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
#[ignore = "owner speed rules prohibit release builds/benchmarks; 00 integration then RTX release gate"]
// A runtime assertion lets debug builds compile the ignored acceptance gate.
#[allow(clippy::assertions_on_constants)]
fn view37_full_app_gpu_5000_paths_warm_pointer_frames_under_16ms_release() {
    assert!(!cfg!(debug_assertions), "performance gate requires release profile");
    let _lock = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(5000);
    toggle(&mut h, false);
    assert_eq!(crate::isolation::bridge(&h.ctx).views.mode, Mode::Outline);
    let source = h.state().session.active().unwrap().doc.clone();
    let revision = h.state().session.active().unwrap().revision;
    for _ in 0..4 {
        canvas_pixels(&mut h);
        h.step();
    }
    let mut times = Vec::new();
    pointer(&mut h, [610., 450.], Some(true));
    // No-pan Hand frames still follow the real press/move/release path and GPU readback.
    for _ in 0..48 {
        let start = Instant::now();
        pointer(&mut h, [610., 450.], None);
        canvas_pixels(&mut h);
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let start = Instant::now();
    pointer(&mut h, [610., 450.], Some(false));
    canvas_pixels(&mut h);
    let release = start.elapsed().as_secs_f64() * 1000.;
    times.sort_by(f64::total_cmp);
    eprintln!("view37 real-app release 5000 paths median={}ms p95={}ms p99={}ms release={release}ms", times[24], times[45], times[47]);
    assert!(times[45] < 16.);
    assert_eq!(h.state().session.active().unwrap().doc, source);
    assert_eq!(h.state().session.active().unwrap().revision, revision);
}
