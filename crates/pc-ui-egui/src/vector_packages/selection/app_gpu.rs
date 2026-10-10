//! Real-app gates. These require 00's engine registration and route() hook. Running them
//! in a disposable registration copy proves the supplied hooks, not production wiring.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{PhotocraftApp, state::Tool as AppTool};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
type H = Harness<'static, PhotocraftApp>;
fn harness(count: usize) -> H {
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.backend == eframe::wgpu::Backend::Vulkan && i.device_type == eframe::wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
            })
            .ok_or_else(|| "RTX 5090 Vulkan discrete adapter required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), eframe::wgpu::TextureFormat::Rgba32Float);
        eprintln!("selection full-app GPU: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let mut setup = eframe::egui_wgpu::WgpuSetup::CreateNew(setup);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |_| {});
    let mut h = Harness::builder().with_size(egui::vec2(1280., 800.)).with_max_steps(64).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut session = Session::new();
        let mut doc = Document::new("GPU selection", Size::new(320, 240), ColorMode::Rgb, SampleType::U8);
        for i in 0..count {
            let (x, y) = ((i % 20) as f64 * 15. + 5., (i / 20) as f64 * 8. + 5.);
            let mut sh = ShapeLayer {
                path: Path::new(vec![Subpath::polygon(&[(x, y), (x + 6., y), (x + 6., y + 4.), (x, y + 4.)])]),
                fill: Some(Fill::Solid(Color::rgb(0.85, 0.1, 0.2))),
                ..Default::default()
            };
            sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
            doc.layers.push(Layer::new(format!("Object {i}"), LayerContent::Shape(sh)));
        }
        session.add_document(doc, None);
        session.edit_prefs(|p| {
            p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector;
            p.toolbar.vector_tool = "PathSelection".into();
        });
        let mut app = PhotocraftApp::new(session, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("real GPU renderer").clone());
        app.sync_views();
        app.ui.tool = AppTool::PathSelection;
        app.ui.extras.rulers = false;
        app.ui.tool_options.move_show_transform = false;
        app.ui.view.show.layer_edges = false;
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        app
    });
    h.run_steps(6);
    assert!(h.state().gpu_active());
    assert!(h.state().perf.gpu);
    h
}
fn pointer(h: &mut H, p: [f32; 2], button: Option<bool>, mods: Modifiers) {
    let xf = crate::canvas::ViewXform::active(h.state()).unwrap();
    let pos = xf.to_screen(p[0], p[1]);
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = button {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: mods });
    }
    h.step();
}
fn activate_h(h: &H, tool: Tool) {
    activate(h.state(), &h.ctx, tool, false, Scope::default());
}
fn interior_pixels(h: &mut H) -> Vec<[u8; 4]> {
    let img = h.render().expect("real GPU canvas readback");
    let xf = crate::canvas::ViewXform::active(h.state()).unwrap();
    let ppp = h.ctx.pixels_per_point();
    (0..500)
        .map(|i| {
            let p = xf.to_screen((i % 20) as f32 * 15. + 8., (i / 20) as f32 * 8. + 7.) * ppp;
            img.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0
        })
        .collect()
}
#[test]
fn selection_full_app_gpu_live_lasso_cancel_release_wand_add_rtx5090() {
    let mut h = harness(5);
    activate_h(&h, Tool::Lasso);
    // Keep the screenshot's cursor fixed so a cursor move cannot satisfy the overlay gate.
    pointer(&mut h, [3., 11.], None, Modifiers::NONE);
    let doc = h.state().session.active().unwrap().doc.clone();
    let revision = h.state().session.active().unwrap().revision;
    let journal = h.state().session.journal.len();
    let before = h.render().unwrap();
    pointer(&mut h, [3., 3.], Some(true), Modifiers::NONE);
    pointer(&mut h, [43., 3.], None, Modifiers::NONE);
    pointer(&mut h, [43., 11.], None, Modifiers::NONE);
    pointer(&mut h, [3., 11.], None, Modifiers::NONE);
    let b = bridge(&h.ctx);
    assert_eq!(b.state.preview.as_ref().unwrap().layers.len(), 3);
    assert!(b.state.captured());
    let live = h.render().unwrap();
    assert_ne!(before.as_raw(), live.as_raw(), "live overlays must reach real app GPU frames");
    h.key_press(egui::Key::Escape);
    h.step();
    pointer(&mut h, [3., 3.], Some(false), Modifiers::NONE);
    assert!(!bridge(&h.ctx).state.captured());
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    pointer(&mut h, [3., 3.], Some(true), Modifiers::NONE);
    pointer(&mut h, [43., 3.], None, Modifiers::NONE);
    pointer(&mut h, [43., 11.], None, Modifiers::NONE);
    pointer(&mut h, [3., 11.], None, Modifiers::NONE);
    pointer(&mut h, [3., 3.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().selected_layers().len(), 3);
    h.step(); // Layers paints before the canvas release; observe the requested repaint.
    assert_eq!(h.get_by_role_and_label(egui::accesskit::Role::Button, "Object 0").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
    assert_eq!(h.state().session.active().unwrap().revision, revision);
    assert_eq!(h.state().session.journal.len(), journal);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 0);
    activate_h(&h, Tool::Wand);
    pointer(&mut h, [8., 7.], Some(true), Modifiers::NONE);
    pointer(&mut h, [8., 7.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().selected_layers().len(), 5);
    h.render().unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, doc);
}
#[test]
#[ignore = "release real-app 500-object pointer-frame benchmark; run after 00 routing"]
fn selection_full_app_release_500_pointer_frames_under_16ms_rtx5090() {
    if cfg!(debug_assertions) {
        panic!("release build required");
    }
    let mut h = harness(500);
    assert!(bridge(&h.ctx).tool.is_none(), "ordinary Path Selection tool must draw menu selection too");
    let before = interior_pixels(&mut h);
    let mut baseline = Vec::new();
    for i in 0..5 {
        let p = crate::canvas::ViewXform::active(h.state()).unwrap().to_screen(300., 215. + (i % 2) as f32);
        h.event(Event::PointerMoved(p));
        let start = std::time::Instant::now();
        h.step();
        let ui = start.elapsed().as_secs_f64() * 1000.;
        h.render().unwrap();
        baseline.push((ui, start.elapsed().as_secs_f64() * 1000.));
    }
    eprintln!("500-shape app baseline (one selected), UI/total ms: {baseline:?}");
    let doc = h.state().session.active().unwrap().doc.clone();
    let mut b = bridge(&h.ctx);
    let reference = h.state().session.active().unwrap().doc.layers[0].id;
    execute(
        &mut h.state_mut().session,
        &mut b.state,
        Scope::default(),
        Query::Same { criterion: Same::FillColor, reference, compare: Compare::Equal, value: None, item: None },
    )
    .unwrap();
    h.ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
    h.step();
    h.render().unwrap();
    assert_eq!(h.state().session.active().unwrap().selected_layers().len(), 500);
    assert_eq!(bridge(&h.ctx).state.selection.as_ref().unwrap().bounds.len(), 500);
    h.run_steps(4);
    let mut times = Vec::new();
    let mut phases = Vec::new();
    for i in 0..30 {
        let p = crate::canvas::ViewXform::active(h.state()).unwrap().to_screen(300., 215. + (i % 2) as f32);
        // Harness::step runs one frame per queued event. Queue only the actual
        // pointer motion, so this measures one pointer frame, not an extra modifier frame.
        h.event(Event::PointerMoved(p));
        let start = std::time::Instant::now();
        h.step();
        let ui = start.elapsed().as_secs_f64() * 1000.;
        h.render().unwrap();
        let total = start.elapsed().as_secs_f64() * 1000.;
        phases.push((ui, total - ui, h.state().perf.spans.get("vector-selection").copied().unwrap_or_default()));
        times.push(total);
    }
    times.sort_by(f64::total_cmp);
    eprintln!(
        "selection 500 real-app pointer+GPU/readback warm ms median={:.3}, p95={:.3}, p99={:.3}; hardware={:?}",
        times[15],
        times[28],
        times[29],
        h.state().perf.gpu_info
    );
    eprintln!("500 selection frame phases (UI, GPU/readback, selection route) ms: {phases:?}; app perf={:?}", h.state().perf);
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    let rendered = h.render().unwrap();
    let xf = crate::canvas::ViewXform::active(h.state()).unwrap();
    let ppp = h.ctx.pixels_per_point();
    let visible = (0..500)
        .filter(|i| {
            let p = xf.to_screen((i % 20) as f32 * 15. + 8., (i / 20) as f32 * 8. + 5.) * ppp;
            (-2..=2).any(|dy| {
                (-2..=2).any(|dx| {
                    let x = p.x.floor() as i32 + dx;
                    let y = p.y.floor() as i32 + dy;
                    if x < 0 || y < 0 || x >= rendered.width() as i32 || y >= rendered.height() as i32 {
                        return false;
                    }
                    let [r, g, b, _] = rendered.get_pixel(x as u32, y as u32).0;
                    i16::from(b) - i16::from(r) > 40 && g > 80 && b > 150
                })
            })
        })
        .count();
    assert!(visible >= 495, "only {visible}/500 object edges show selection overlay pixels");
    let after = interior_pixels(&mut h);
    let max = before.iter().zip(after).flat_map(|(a, b)| (0..4).map(move |i| (i16::from(a[i]) - i16::from(b[i])).abs())).max().unwrap();
    eprintln!("selection overlay edges visible={visible}/500; canvas interior max diff={max}/255");
    assert!(max <= 1, "canvas interior diff {max}/255");
    assert!(times[28] < 16., "warm p95 {} ms exceeds 16 ms", times[28]);
}
#[test]
fn selection_full_app_gpu_group_circle_then_node_lasso_delete_03_rtx5090() {
    let mut h = harness(1);
    let leaf = h.state().session.active().unwrap().doc.layers[0].id;
    let mut layer = h.state().session.active().unwrap().doc.layers[0].clone();
    layer.name = "Circle".into();
    let LayerContent::Shape(sh) = &mut layer.content else { panic!() };
    sh.path = photocraft_vector::shapes::ellipse(5., 5., 6., 4.);
    sh.cache = Some(photocraft_vector::render_shape(sh, photocraft_doc::PixelFormat::RGBA8, photocraft_geom::Rect::new(0, 0, 320, 240)));
    let group = Layer::group("Selection Group", vec![layer]);
    let parent = group.id;
    h.state_mut()
        .session
        .edit("group fixture", |doc, active| {
            doc.layers = vec![group];
            *active = Some(parent);
            Ok(())
        })
        .unwrap();
    h.run_steps(3);
    activate_h(&h, Tool::Group);
    let doc = h.state().session.active().unwrap().doc.clone();
    let revision = h.state().session.active().unwrap().revision;
    let history = h.state().session.active().unwrap().history.past_len();
    for expected in [leaf, parent] {
        pointer(&mut h, [8., 7.], Some(true), Modifiers::NONE);
        pointer(&mut h, [8., 7.], Some(false), Modifiers::NONE);
        assert_eq!(h.state().session.active().unwrap().selected_layers(), [expected]);
        h.step(); // Layers paints before the canvas release.
        h.render().unwrap();
        let selected = if expected == leaf { "Circle" } else { "Selection Group" };
        let other = if expected == leaf { "Selection Group" } else { "Circle" };
        assert_eq!(h.get_by_role_and_label(egui::accesskit::Role::Button, selected).accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        assert_eq!(h.get_by_role_and_label(egui::accesskit::Role::Button, other).accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
    }
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    assert_eq!(h.state().session.active().unwrap().revision, revision);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    // The exact 03 frame hook owns Delete. Use a four-node diamond and real
    // app keyboard input after lasso publication, without a test-only delete handler.
    h.state_mut()
        .session
        .edit("curve fixture", |doc, active| {
            let l = doc.layer_mut(leaf).unwrap();
            let LayerContent::Shape(sh) = &mut l.content else { panic!() };
            sh.path = Path::new(vec![Subpath::polygon(&[(5., 7.), (8., 5.), (11., 7.), (8., 9.)])]);
            sh.cache = Some(photocraft_vector::render_shape(sh, photocraft_doc::PixelFormat::RGBA8, photocraft_geom::Rect::new(0, 0, 320, 240)));
            *active = Some(leaf);
            Ok(())
        })
        .unwrap();
    h.state_mut().session.active_mut().unwrap().selected_layers = vec![leaf];
    h.state_mut().ui.tool = AppTool::DirectSelection;
    let ctx = h.ctx.clone();
    activate(h.state(), &ctx, Tool::Lasso, true, Scope::default());
    h.run_steps(2);
    pointer(&mut h, [4., 4.], Some(true), Modifiers::NONE);
    pointer(&mut h, [12., 4.], None, Modifiers::NONE);
    pointer(&mut h, [12., 7.], None, Modifiers::NONE);
    assert!(bridge(&h.ctx).state.captured());
    h.key_press(egui::Key::Escape);
    h.step();
    pointer(&mut h, [4., 4.], Some(false), Modifiers::NONE);
    assert!(!bridge(&h.ctx).state.captured());
    assert!(h.state().ui.direct_selection.anchors.is_empty());
    for (p, button) in [([4., 4.], Some(true)), ([9., 4.], None), ([9., 8.], None), ([4., 8.], None), ([4., 4.], Some(false))] {
        pointer(&mut h, p, button, Modifiers::NONE);
    }
    assert_eq!(h.state().ui.direct_selection.anchors, [[0, 0], [0, 1]]);
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    h.key_press(egui::Key::Delete);
    h.step();
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(leaf).unwrap().content else { panic!() };
    assert_eq!(sh.path.subpaths[0].knots.len(), 2);
    assert_eq!(sh.path.subpaths[0].knots.iter().map(|k| (k.anchor.x, k.anchor.y)).collect::<Vec<_>>(), [(11., 7.), (8., 9.)]);
    assert_eq!(h.state().session.journal.len(), journal + 1, "Delete journal delta: {:?}", &h.state().session.journal[journal..]);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
