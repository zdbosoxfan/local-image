//! Package 31 full-app acceptance, registered ONLY in pc-ui-egui/tests/gpu/main.rs by 00.
//! Requires 01's native Contour tool, 10's live canvas routing, and the Outline Stroke menu.
//! Original synthetic fixtures, CC0-1.0. No desktop window, adapter skip or software fallback.
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, LineCap, SampleType, ShapeLayer, ShapeStroke, Size, Subpath};
use photocraft_engine::Session;
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, gpu_canvas, state::Tool};
use serde_json::json;
use std::sync::Arc;
type H = Harness<'static, PhotocraftApp>;

fn harness(paths: Vec<ShapeLayer>, contour: bool) -> H {
    let mut create = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN;
    create.native_adapter_selector = Some(Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.name.contains("RTX 5090") && i.backend == eframe::wgpu::Backend::Vulkan && i.device_type == eframe::wgpu::DeviceType::DiscreteGpu
            })
            .cloned()
            .ok_or_else(|| "geometry31 requires RTX 5090/Vulkan/discrete".into())
    }));
    let mut setup = eframe::egui_wgpu::WgpuSetup::CreateNew(create);
    gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        let info = adapter.get_info();
        assert!(info.name.contains("RTX 5090"));
        assert_eq!(info.backend, eframe::wgpu::Backend::Vulkan);
        assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), eframe::wgpu::TextureFormat::Rgba32Float);
        assert!(gpu_canvas::supports_f16_canvas(adapter));
        eprintln!("geometry31 app hardware: {info:?}");
    });
    let mut doc = Document::with_background("Geometry quality", Size::new(160, 160), ColorMode::Rgb, SampleType::F32, Color::WHITE);
    let mut ids = Vec::new();
    for mut shape in paths {
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
        let layer = Layer::new("Quality shape", LayerContent::Shape(shape));
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    s.select_layer(ids[0]).unwrap();
    s.active_mut().unwrap().selected_layers = ids;
    if contour {
        s.edit_prefs(|p| {
            p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector;
            p.toolbar.vector_tool = "Contour".into();
        });
    }
    let mut h = Harness::builder().with_size(egui::vec2(1280., 800.)).with_pixels_per_point(1.).with_max_steps(64).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(s, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("required real GPU").clone());
        app.sync_views();
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].center = [80., 80.];
        app.ui.views[0].fit_pending = false;
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.show.layer_edges = false;
        app.ui.view.show.smart_guides = false;
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        if contour {
            // A final native tool is required. Never activate the Pen rehearsal sentinel.
            app.ui.tool = Tool::Contour;
        }
        app
    });
    h.run_steps(5);
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    if contour {
        h.get_by_label("Contour Tool").click();
    } else {
        h.get_by_label("Hand Tool").click();
    }
    h.run_steps(3);
    h
}
fn square(x: f64, y: f64, size: f64) -> ShapeLayer {
    ShapeLayer { path: photocraft_vector::shapes::rect(x, y, size, size), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() }
}
fn parity(h: &mut H, expected: &Document, _key: u64) -> Vec<[f32; 4]> {
    h.run_steps(3);
    h.render().expect("render the full offscreen app");
    let app = h.state();
    assert!(app.gpu_active() && app.perf.gpu);
    let (format, size, pixels) = app.read_canvas_texels(expected.id.0).expect("production app canvas texture");
    assert_eq!(format, eframe::wgpu::TextureFormat::Rgba16Float, "actual float canvas required");
    assert_eq!(size, [160, 160]);
    fn refresh(doc: &Document, layers: &mut [Layer]) {
        for layer in layers {
            match &mut layer.content {
                LayerContent::Shape(shape) => shape.cache = Some(photocraft_vector::render_shape(shape, doc.pixel_format(), doc.bounds())),
                LayerContent::Group(group) => refresh(doc, &mut group.children),
                _ => {}
            }
        }
    }
    // Rebuild plain fixture caches from paths so a stale prepared cache cannot also
    // contaminate the CPU reference. This never replaces the app's live document.
    let mut reference = expected.clone();
    refresh(expected, &mut reference.layers);
    let cpu = photocraft_compose::flatten(&reference);
    assert_eq!(cpu.px.len(), pixels.len());
    let mut max = 0.0f32;
    for (c, g) in cpu.px.iter().zip(&pixels) {
        for k in 0..4 {
            let want = if k == 3 { c[3] } else { c[k] * c[3] };
            assert!(g[k].is_finite());
            max = max.max((want - g[k]).abs());
        }
    }
    assert!(max <= 1. / 255., "actual app premultiplied max diff {max}");
    pixels
}
fn area(pixels: &[[f32; 4]]) -> f64 {
    pixels.iter().map(|p| f64::from(1. - p[0])).sum()
}
fn pointer(h: &mut H, pos: Pos2, button: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = button {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}

#[test]
fn geometry31_app_gpu_boolean_buttons_have_independent_area_and_atomic_undo() {
    let _gpu_gate = crate::gpu_lock();

    for (label, expected_area) in [("Add", 6800.), ("Subtract", 3200.), ("Intersect", 400.), ("Xor", 6400.), ("Divide", 6800.)] {
        let mut h = harness(vec![square(20., 20., 60.), square(60., 60., 60.)], false);
        let before = h.state().session.active().unwrap().doc.clone();
        let selection = h.state().session.active().unwrap().selected_layers();
        let history = h.state().session.active().unwrap().history.past_len();
        let journal = h.state().session.journal.len();
        parity(&mut h, &before, before.id.0);
        h.get_by_label(label).click();
        h.run_steps(3);
        assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
        let after = h.state().session.active().unwrap().doc.clone();
        assert_ne!(before, after, "actual geometry action must execute");
        let pixels = parity(&mut h, &after, after.id.0);
        assert!((area(&pixels) - expected_area).abs() < 0.5, "{label} area {}", area(&pixels));
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        assert_eq!(h.state().session.journal.len(), journal + 1);
        assert!(h.state_mut().session.undo());
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.active().unwrap().selected_layers(), selection);
        parity(&mut h, &before, before.id.0);
    }
}

#[test]
fn geometry31_app_gpu_contour_live_drag_release_cancel_and_stale_targets() {
    let _gpu_gate = crate::gpu_lock();

    for delta in [-10.0f32, 10.] {
        let mut h = harness(vec![square(20., 20., 100.)], true);
        let before = h.state().session.active().unwrap().doc.clone();
        let history = h.state().session.active().unwrap().history.past_len();
        let journal = h.state().session.journal.len();
        let xf = ViewXform::active(h.state()).unwrap();
        let a = xf.to_screen(120., 70.);
        let b = xf.to_screen(120. + delta, 70.);
        pointer(&mut h, a, Some(true));
        pointer(&mut h, a.lerp(b, 0.5), None);
        pointer(&mut h, b, None);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
        assert_eq!(h.state().session.journal.len(), journal);
        let (preview, key) = h.state().constructive_state.display_document(&h.state().session).expect("production detached contour preview");
        let pixels = parity(&mut h, &preview, key);
        let expected = if delta < 0. { 6400. } else { 14000. + std::f64::consts::PI * 100. };
        assert!((area(&pixels) - expected).abs() < 1., "round contour area {} != {expected}", area(&pixels));
        pointer(&mut h, b, Some(false));
        let after = h.state().session.active().unwrap().doc.clone();
        assert_eq!(after, preview);
        parity(&mut h, &after, after.id.0);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        assert_eq!(h.state().session.journal.len(), journal + 1);
        assert!(h.state_mut().session.undo());
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        parity(&mut h, &before, before.id.0);
    }
    for stale in [false, true] {
        let mut h = harness(vec![square(20., 20., 100.)], true);
        let before = h.state().session.active().unwrap().doc.clone();
        let xf = ViewXform::active(h.state()).unwrap();
        let (a, b) = (xf.to_screen(120., 70.), xf.to_screen(130., 70.));
        pointer(&mut h, a, Some(true));
        pointer(&mut h, b, None);
        let (preview, key) = h.state().constructive_state.display_document(&h.state().session).unwrap();
        parity(&mut h, &preview, key);
        if stale {
            h.state_mut().run("file.new", json!({"width":160,"height":160,"depth":32,"background":"white"})).unwrap();
        } else {
            h.key_press(egui::Key::Escape);
        }
        let current = h.state().session.active().unwrap().doc.clone();
        let journal = h.state().session.journal.len();
        pointer(&mut h, b, Some(false));
        assert_eq!(h.state().session.active().unwrap().doc, current);
        assert_eq!(h.state().session.journal.len(), journal);
        if !stale {
            assert_eq!(current, before);
        }
        parity(&mut h, &current, current.id.0);
    }
}

#[test]
fn geometry31_app_gpu_outline_stroke_menu_matches_original_dashes_and_undo() {
    let _gpu_gate = crate::gpu_lock();

    let shape = ShapeLayer {
        path: photocraft_doc::Path::new(vec![Subpath::polyline(&[(24., 64.), (80., 36.), (136., 84.)])]),
        stroke: Some(ShapeStroke { width: 8., cap: LineCap::Round, dashes: vec![2., 1.], ..Default::default() }),
        ..Default::default()
    };
    let mut h = harness(vec![shape], false);
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    let journal = h.state().session.journal.len();
    let original = parity(&mut h, &before, before.id.0);
    // 00 supplies this real menu route to V1 path.outlineStroke; never call the command
    // directly here to disguise an absent UI entry as integrated acceptance.
    h.get_by_label("Layer").click();
    h.run_steps(2);
    h.get_by_label("Geometry ⏵").click();
    h.run_steps(2);
    h.get_by_label("Outline Stroke").click();
    h.run_steps(3);
    let after = h.state().session.active().unwrap().doc.clone();
    assert_ne!(before, after);
    let expanded = parity(&mut h, &after, after.id.0);
    let max = original.iter().zip(&expanded).flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs())).fold(0.0f32, f32::max);
    assert!(max <= 1. / 255., "expanded native stroke pixel difference {max}");
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    assert_eq!(h.state().session.journal.len(), journal + 1);
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    parity(&mut h, &before, before.id.0);
}
