//! Full PhotocraftApp input/render acceptance. Register this source only after the
//! production hooks in integration/16.md land. No widget-host injection is used here.
use eframe::egui_wgpu::{self, wgpu};
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, Fill, Layer, LayerContent, ShapeLayer};
use photocraft_engine::vector_packages::layout as engine;
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform};
use serde_json::json;
type H = Harness<'static, PhotocraftApp>;
fn gpu() -> egui_wgpu::WgpuSetup {
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = wgpu::Backends::VULKAN;
    create.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.name.contains("RTX 5090") && i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu
            })
            .ok_or_else(|| "RTX 5090 Vulkan discrete required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        eprintln!("layout16 full app GPU: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(create);
    photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut setup);
    setup
}
fn harness(zoom: f32, ppp: f32) -> H {
    let mut h = Harness::builder().with_size(egui::vec2(1500., 1000.)).with_pixels_per_point(ppp).wgpu_setup(gpu()).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().unwrap().clone());
        app
    });
    h.state_mut().run("file.new", json!({"width":256,"height":192,"background":"white"})).unwrap();
    h.state_mut().sync_views();
    h.state_mut()
        .session
        .edit("Mixed crop fixture", |doc, active| {
            let mut sh =
                ShapeLayer { path: engine::crop_path([20., 20., 80., 60.]).unwrap(), fill: Some(Fill::Solid(Color::rgb(0.8, 0.2, 0.1))), ..Default::default() };
            sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
            let layer = Layer::new("Layout shape", LayerContent::Shape(sh));
            let id = layer.id;
            doc.layers.push(layer);
            let mut raster = Layer::raster("Clipped photograph", doc.pixel_format());
            raster.clipped = true;
            raster.surface_mut().unwrap().fill_rect(doc.bounds(), &[0.1, 0.5, 0.9, 0.5]);
            doc.layers.push(raster);
            *active = Some(id);
            Ok(())
        })
        .unwrap();
    let app = h.state_mut();
    app.ui.tool = photocraft_ui_egui::state::Tool::Move;
    app.ui.extras.rulers = false;
    app.ui.extras.snap = false;
    app.ui.views[0].zoom = zoom;
    app.ui.views[0].center = [60., 50.];
    app.ui.views[0].fit_pending = false;
    h.run_steps(5);
    // The actual selection context button opens the production Transform panel.
    h.get_by_label("Transform and Layout").click();
    h.run_steps(3);
    assert!(h.state().shared_tools.layout.open);
    h.state_mut().shared_tools.layout.snapping.enabled = false;
    assert!(h.state().gpu_active());
    h
}
fn pointer(h: &mut H, x: f32, y: f32, pressed: Option<bool>) {
    let p = ViewXform::active(h.state()).unwrap().to_screen(x, y);
    h.event(Event::PointerMoved(p));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
}
fn assert_pixels_equal(h: &H, a: &image::RgbaImage, b: &image::RgbaImage, xs: std::ops::Range<i32>, ys: std::ops::Range<i32>) {
    let xf = ViewXform::active(h.state()).unwrap();
    let ppp = h.ctx.pixels_per_point();
    for y in ys {
        for x in xs.clone() {
            let p = xf.to_screen(x as f32 + 0.5, y as f32 + 0.5) * ppp;
            let p = a.get_pixel(p.x.floor() as u32, p.y.floor() as u32);
            let q = b.get_pixel(
                (xf.to_screen(x as f32 + 0.5, y as f32 + 0.5).x * ppp).floor() as u32,
                (xf.to_screen(x as f32 + 0.5, y as f32 + 0.5).y * ppp).floor() as u32,
            );
            assert!(p.0.iter().zip(q.0).all(|(a, b)| a.abs_diff(b) <= 1), "premultiplied live/release pixel {x},{y}: {p:?} {q:?}");
        }
    }
}
#[test]
fn layout16_app_rtx_pointer_crop_live_release_cancel_undo_zoom_parity() {
    let _gpu_gate = crate::gpu_lock();
    for (zoom, ppp) in [(1., 1.), (2., 2.), (4., 1.), (8., 1.)] {
        let mut h = harness(zoom, ppp);
        h.get_by_label("Vector Crop tool").click();
        h.run_steps(3);
        // Keep the zoomed handle clear of the floating panel; this changes only window arrangement.
        h.state_mut().shared_tools.layout.open = false;
        h.run_steps(2);
        let before = h.state().session.active().unwrap().doc.clone();
        let journal = h.state().session.journal.len();
        pointer(&mut h, 100., 80., Some(true));
        pointer(&mut h, 75., 65., None);
        h.run_steps(2);
        let detached = h.state().shared_tools.layout.preview.clone().expect("production live crop preview");
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.journal.len(), journal);
        let live = h.render().unwrap();
        let xf = ViewXform::active(h.state()).unwrap();
        let outside = xf.to_screen(90., 45.) * h.ctx.pixels_per_point();
        let pixel = live.get_pixel(outside.x.floor() as u32, outside.y.floor() as u32);
        assert_eq!(&pixel.0[..3], &[255, 255, 255], "crop reveals the white background outside its right edge");

        pointer(&mut h, 75., 65., Some(false));
        h.run_steps(3);
        let release = h.render().unwrap();
        assert_eq!(h.state().session.active().unwrap().doc.layers, detached.layers);
        assert!(h.state().gpu_active());
        assert_pixels_equal(&h, &live, &release, 25..70, 25..60);
        assert_eq!(h.state().session.journal.len(), journal + 1);
        assert!(h.state_mut().session.undo());
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        pointer(&mut h, 100., 80., Some(true));
        pointer(&mut h, 75., 65., None);
        h.key_press(Key::Escape);
        h.run_steps(2);
        pointer(&mut h, 75., 65., Some(false));
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().shared_tools.layout.preview.is_none());
    }
}
#[test]
fn layout16_app_rtx_pointer_resize_constraints_and_point_transform() {
    let _gpu_gate = crate::gpu_lock();
    let mut h = harness(1., 1.);
    let id = h.state().session.active().unwrap().active_layer.unwrap();
    h.state_mut()
        .session
        .edit("Constrained parent", |doc, active| {
            let index = doc.layers.iter().position(|l| l.id == id).unwrap();
            let mut shape = doc.layers.remove(index);
            shape.vector.constraints = Some(photocraft_doc::vector_design::structure::Constraints {
                right: true,
                fixed_width: true,
                top: true,
                fixed_height: true,
                ..Default::default()
            });
            let mut board = Layer::group("Board", vec![shape]);
            if let LayerContent::Group(g) = &mut board.content {
                g.artboard = Some(photocraft_doc::Artboard {
                    rect: photocraft_geom::Rect::new(10, 10, 130, 100),
                    background: photocraft_doc::ArtboardBackground::Transparent,
                    preset: String::new(),
                    drawing_scale: None,
                });
            }
            *active = Some(board.id);
            doc.layers.push(board);
            Ok(())
        })
        .unwrap();
    h.run_steps(3);
    h.get_by_label("Vector Artboard tool").click();
    h.run_steps(3);
    let before = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, 130., 100., Some(true));
    pointer(&mut h, 170., 120., None);
    h.run_steps(2);
    let detached = h.state().shared_tools.layout.preview.clone().expect("production resize preview");
    assert_eq!(h.state().session.active().unwrap().doc, before);
    let live = h.render().unwrap();
    pointer(&mut h, 170., 120., Some(false));
    h.run_steps(3);
    let release = h.render().unwrap();
    assert_pixels_equal(&h, &live, &release, 65..95, 25..65);
    assert_eq!(h.state().session.active().unwrap().doc.layers, detached.layers);
    let b = engine::bounds(h.state().session.active().unwrap().doc.layer(id).unwrap()).unwrap();
    assert_eq!(b, [60., 20., 80., 60.]);
    assert!(h.state_mut().session.undo());
    h.state_mut().run("layer.select", json!({"layer":id.0})).unwrap();
    h.run_steps(3);
    h.get_by_label("Point Transform tool").click();
    h.run_steps(3);
    pointer(&mut h, 100., 20., Some(true));
    pointer(&mut h, 100., 80., None);
    assert!(h.state().shared_tools.layout.preview.is_some());
    pointer(&mut h, 100., 80., Some(false));
    assert!(h.state().shared_tools.layout.preview.is_none());
    assert!(h.state_mut().session.undo());
}
#[test]
#[ignore = "release real-app pointer/frame measurement, run explicitly"]
fn layout16_app_release_pointer_frames() {
    let _gpu_gate = crate::gpu_lock();
    if cfg!(debug_assertions) {
        panic!("run with --release");
    }
    let mut h = harness(1., 1.);
    h.get_by_label("Vector Crop tool").click();
    h.run_steps(3);
    pointer(&mut h, 100., 80., Some(true));
    pointer(&mut h, 90., 70., None);
    let mut times = Vec::new();
    for i in 0..30 {
        let start = std::time::Instant::now();
        pointer(&mut h, 90. - i as f32 * 0.25, 70., None);
        h.render().expect("real app GPU readback");
        times.push(start.elapsed().as_secs_f64() * 1000.);
        assert!(h.state().shared_tools.layout.preview.is_some());
    }
    times.sort_by(f64::total_cmp);
    let start = std::time::Instant::now();
    pointer(&mut h, 82.75, 70., Some(false));
    h.render().unwrap();
    eprintln!(
        "layout16 real-app pointer UI+GPU+readback: n=30 median={:.3}ms p95={:.3}ms p99={:.3}ms release={:.3}ms identity={:?}",
        (times[14] + times[15]) / 2.,
        times[28],
        times[29],
        start.elapsed().as_secs_f64() * 1000.,
        h.state().perf.gpu_info
    );
}

#[test]
fn layout16_app_actual_preset_cards_design_units_and_old_settings() {
    let _gpu_gate = crate::gpu_lock();
    use photocraft_doc::vector_design::structure::DesignUnits;
    use photocraft_ui_egui::state::{DialogKind, UiState};
    let old = serde_json::to_value(UiState::default()).unwrap();
    assert!(old.get("vector_layout").is_none());
    let loaded: UiState = serde_json::from_value(old).unwrap();
    assert!(serde_json::to_value(loaded).unwrap().get("shared_tools").is_none());
    for (category, unit, display) in [
        ("Photo", DesignUnits::Pixels, "in"),
        ("Print", DesignUnits::Millimeters, "mm"),
        ("Web", DesignUnits::Points, "pt"),
        ("Mobile", DesignUnits::Points, "pt"),
        ("Film & Video", DesignUnits::Points, "pt"),
        ("Art & Illustration", DesignUnits::Points, "pt"),
    ] {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let mut h = Harness::builder().with_size(egui::vec2(1400., 900.)).build_ui_state(|ui, app| photocraft_ui_egui::dialogs::show(app, ui.ctx()), app);
        PhotocraftApp::setup_context(&h.ctx, Default::default());
        h.state_mut().ui.open_dialog(DialogKind::NewDocument, UiState::new_document_fields());
        h.run_steps(3);
        h.get_by_label(category).click();
        h.run_steps(2);
        // Preset cards are custom painted. Their location follows the observed grid heading.
        let heading = h.get_by_label_contains("BLANK DOCUMENT PRESETS (").rect();
        let p = egui::pos2(heading.left() + 82., heading.bottom() + 60.);
        h.hover_at(p);
        h.run_steps(1);
        h.drag_at(p);
        h.run_steps(1);
        h.drop_at(p);
        h.run_steps(2);
        let fields = &h.state().ui.dialogs[0].fields;
        assert!(fields.get("__preset").is_some(), "clicked {category} preset card");
        assert_eq!(fields["__unit"], display);
        assert_eq!(fields["designUnits"], json!(unit));
        let expected = (fields["width"].as_u64().unwrap() as u32, fields["height"].as_u64().unwrap() as u32);
        h.get_by_label("Create").click();
        h.run_steps(3);
        assert!(h.state().ui.dialogs.is_empty());
        let doc = &h.state().session.active().unwrap().doc;
        assert_eq!(doc.design_units, unit);
        let (ruler, ppi) = photocraft_ui_egui::vector_packages::layout_app_hooks::ruler_units(h.state());
        let expected_unit = match unit {
            DesignUnits::Pixels => photocraft_engine::prefs::Unit::Pixels,
            DesignUnits::Points => photocraft_engine::prefs::Unit::Points,
            DesignUnits::Millimeters => photocraft_engine::prefs::Unit::Millimeters,
            DesignUnits::Inches => photocraft_engine::prefs::Unit::Inches,
        };
        assert_eq!(ruler, expected_unit);
        let inch_units = match unit {
            DesignUnits::Pixels => f64::from(doc.resolution_dpi),
            DesignUnits::Points => 72.,
            DesignUnits::Millimeters => 25.4,
            DesignUnits::Inches => 1.,
        };
        assert!((ruler.to_px(inch_units, f64::from(doc.resolution_dpi), f64::from(doc.size.width), ppi) - f64::from(doc.resolution_dpi)).abs() < 1e-8);

        assert_eq!((doc.size.width, doc.size.height), expected);
    }
}

#[test]
fn layout16_app_existing_move_keeps_clip_and_path_translation_together() {
    let _gpu_gate = crate::gpu_lock();
    let mut h = harness(1., 1.);
    h.get_by_label("Vector Crop tool").click();
    h.run_steps(3);
    pointer(&mut h, 100., 80., Some(true));
    pointer(&mut h, 75., 65., Some(false));
    h.run_steps(3);
    let id = h.state().session.active().unwrap().active_layer.unwrap();
    let before = h.state().session.active().unwrap().doc.clone();
    let path = before.layer(id).unwrap().vector.clip.clone().unwrap();
    h.state_mut().ui.tool = photocraft_ui_egui::state::Tool::Move;
    h.state_mut().ui.tool_options.move_auto_select = false;
    h.state_mut().ui.view.show.smart_guides = false;
    h.run_steps(3);
    pointer(&mut h, 50., 40., Some(true));
    pointer(&mut h, 70., 50., None);
    pointer(&mut h, 70., 50., Some(false));
    h.run_steps(3);
    let doc = &h.state().session.active().unwrap().doc;
    assert_eq!(doc.layer(id).unwrap().vector.clip, Some(path.transform(&photocraft_geom::Affine::translate(20., 10.))));
    assert_eq!(engine::bounds(doc.layer(id).unwrap()).unwrap(), [40., 30., 80., 60.]);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
