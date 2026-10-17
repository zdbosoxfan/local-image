//! Production shared Pen/shape routes on a large vector document.
//! Original synthetic scene; timing includes the app frame and RTX GPU readback.
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, state::Tool};
use std::time::Instant;
type H = Harness<'static, PhotocraftApp>;
fn position(h: &H, p: [f32; 2]) -> Pos2 {
    let app = h.state();
    let view = &app.ui.views[0];
    ViewXform {
        rect: photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect),
        zoom: view.zoom,
        center: view.center,
        flip: false,
        rotation: view.rotation,
    }
    .to_screen(p[0], p[1])
}
fn button(h: &mut H, p: [f32; 2], pressed: bool) {
    let pos = position(h, p);
    h.event(Event::PointerMoved(pos));
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.run_steps(2);
}
fn frames(h: &mut H, moving: bool) -> Vec<f64> {
    (0..64)
        .map(|i| {
            let at = Instant::now();
            if moving {
                h.event(Event::PointerMoved(position(h, [360. + (i % 17) as f32, 360. + (i % 11) as f32])));
            }
            h.run_steps(2);
            std::hint::black_box(h.render().expect("RTX app readback"));
            at.elapsed().as_secs_f64() * 1000.
        })
        .collect()
}
#[test]
#[ignore = "release-build Pen/shape timing and cache benchmark on a real GPU; run with --release --ignored"]
fn shared_pen_shape_large_document_release_pointer_frames_rtx5090() {
    let _gate = crate::gpu_lock();
    if cfg!(debug_assertions) {
        panic!("run this measurement with --release");
    }
    for tool in [Tool::Pen, Tool::Rectangle] {
        let mut h: H =
            Harness::builder().with_size(egui::vec2(1200., 850.)).with_max_steps(128).wgpu_setup(photocraft_ui_egui::gpu_canvas::wgpu_setup()).build_eframe(
                |cc| {
                    let rs = cc.wgpu_render_state.as_ref().expect("RTX GPU required");
                    let info = rs.adapter.get_info();
                    #[cfg(not(target_os = "macos"))]
                    assert!(info.name.contains("5090"));
                    assert_eq!(info.backend, if cfg!(target_os = "macos") { eframe::wgpu::Backend::Metal } else { eframe::wgpu::Backend::Vulkan });
                    PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
                    app.set_wgpu(rs.clone());
                    app
                },
            );
        let mut doc = Document::new("Shared tools timing", Size::new(6000, 4000), ColorMode::Rgb, SampleType::U8);
        for i in 0..500 {
            let mut shape = ShapeLayer {
                path: photocraft_vector::shapes::rect(200. + (i % 25) as f64 * 32., 200. + (i / 25) as f64 * 32., 20., 20.),
                fill: Some(Fill::Solid(Color::BLACK)),
                ..Default::default()
            };
            photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
            doc.layers.push(Layer::new("Curve", LayerContent::Shape(shape)));
        }
        h.state_mut().session.add_document(doc, None);
        h.state_mut().sync_views();
        h.run_steps(3);
        h.get_by_label("Vector persona").click();
        h.run_steps(3);
        let app = h.state_mut();
        app.ui.tool = tool;
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.tool_options.vector_mode = "shape".into();
        app.ui.tool_options.shape_fill = true;
        app.ui.views[0].zoom = 2.;
        app.ui.views[0].center = [320., 320.];
        app.ui.views[0].fit_pending = false;
        h.run_steps(4);
        h.render().unwrap();
        let mut idle = frames(&mut h, false);
        idle.sort_by(f64::total_cmp);
        let revision = h.state().session.active().unwrap().revision;
        let journal = h.state().session.journal.len();
        if tool == Tool::Pen {
            button(&mut h, [260., 260.], true);
            button(&mut h, [260., 260.], false);
            button(&mut h, [360., 260.], true);
            button(&mut h, [360., 260.], false);
        }
        button(&mut h, if tool == Tool::Pen { [360., 360.] } else { [260., 260.] }, true);
        let memory_before = photocraft_testkit::perf::current_rss_bytes();
        let uploads_before = h.state().perf.gpu_uploads;
        let mut held = frames(&mut h, true);
        assert!(h.state().shared_tools.preview().is_some() || h.state().shared_tools.shape_preview().is_some(), "production shared preview required");
        assert_eq!(h.state().session.active().unwrap().revision, revision);
        assert_eq!(h.state().session.journal.len(), journal);
        let memory_after = photocraft_testkit::perf::current_rss_bytes();
        let uploads_after = h.state().perf.gpu_uploads;
        let first = held[0];
        held.sort_by(f64::total_cmp);
        let at = Instant::now();
        button(&mut h, [366., 366.], false);
        h.render().unwrap();
        let release = at.elapsed().as_secs_f64() * 1000.;
        if tool == Tool::Rectangle {
            assert_eq!(h.state().session.journal.len(), journal + 1);
        }
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        eprintln!(
            "shared {tool:?} 6000x4000/500 shapes zoom2/1200x850/64 frames, two app steps+GPU/readback: idle median={:.3}ms p95={:.3}ms; held first={first:.3}ms median={:.3}ms p95={:.3}ms p99={:.3}ms release={release:.3}ms; RSS before={memory_before:?} after={memory_after:?}; frame uploads before={uploads_before} after={uploads_after}; adapter={:?}",
            idle[32],
            idle[60],
            held[32],
            held[60],
            held[63],
            h.state().perf.gpu_info
        );
    }
}
