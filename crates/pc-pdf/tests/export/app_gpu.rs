//! Full PhotocraftApp offscreen canvas: pointer preview/release then public export; public import gate after 22.
//! Package 00 registers this in pc-ui-egui/tests/gpu/main.rs after 22; it is not an
//! isolated-widget proxy. No software adapters or successful skips are accepted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
#[cfg(feature = "pdf-import-integrated")]
use photocraft_doc::vector_design::exchange::ImportOptions;
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
use photocraft_engine::vector_packages::pdf_export::{Request, Scope};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, gpu_canvas::GpuCanvas, state::Tool};
use serde_json::json;
use std::sync::{Arc, Mutex};
type H = Harness<'static, PhotocraftApp>;
fn screen(h: &H, x: f32, y: f32) -> Pos2 {
    let app = h.state();
    let view = &app.ui.views[0];
    ViewXform {
        rect: photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect),
        zoom: view.zoom,
        center: view.center,
        flip: false,
        rotation: view.rotation,
    }
    .to_screen(x, y)
}
fn present(h: &mut H) {
    h.run_steps(2);
    h.render().expect("offscreen real app callbacks");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
}
fn parity(gpu: &GpuCanvas, doc: &Document) {
    let (_, size, actual) = gpu.read_texels(doc.id.0).expect("real canvas texture");
    assert_eq!(size, [256, 256]);
    let expected = photocraft_compose::flatten(doc);
    let max =
        actual.iter().zip(&expected.px).flat_map(|(a, b)| (0..4).map(move |k| (a[k] - if k == 3 { b[k] } else { b[k] * b[3] }).abs())).fold(0_f32, f32::max);
    eprintln!("PDF full-app canvas premultiplied max {max:.9}");
    assert!(max <= 1.0 / 255.0, "full-app canvas max {max}");
}
fn preview_pixels(h: &mut H, id: photocraft_doc::LayerId, dx: i32, dy: i32) {
    // Match the established Move canvas harness: remove its artificial triangle
    // cursor without releasing the held gesture, then inspect the actual frame.
    h.event(Event::PointerGone);
    h.run_steps(2);
    let st = h.state().session.active().unwrap();
    let expected = photocraft_engine::layer_multi_cmds::moved(&st.doc, &[id], dx, dy).unwrap();
    let expected = photocraft_compose::flatten(&expected);
    let frame = h.render().expect("actual held-pointer app frame");
    let ppp = h.ctx.pixels_per_point();
    let mut max = 0_f32;
    for y in 0..256 {
        for x in 0..256 {
            let p = screen(h, x as f32 + 0.5, y as f32 + 0.5) * ppp;
            let actual = frame.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0;
            let expected = expected.px[y * 256 + x];
            for k in 0..4 {
                max = max.max((f32::from(actual[k]) / 255.0 - if k == 3 { expected[k] } else { expected[k] * expected[3] }).abs());
            }
        }
    }
    eprintln!("PDF held-pointer preview premultiplied max {max:.9}");
    assert!(max <= 1.0 / 255.0, "actual pointer-preview frame premultiplied max {max}");
}
#[test]
fn pdf_export_app_real_gpu_preview_release_and_menu_dialog() {
    let _gpu_gate = crate::gpu_lock();

    let observer = Arc::new(Mutex::new(None));
    let capture = observer.clone();
    let mut h: H =
        Harness::builder().with_size(egui::vec2(1200., 800.)).with_max_steps(64).wgpu_setup(photocraft_ui_egui::gpu_canvas::wgpu_setup()).build_eframe(
            move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let rs = cc.wgpu_render_state.as_ref().expect("RTX render state");
                let info = rs.adapter.get_info();
                #[cfg(not(target_os = "macos"))]
                assert!(info.name.contains("RTX 5090"), "{info:?}");
                assert_eq!(info.backend, if cfg!(target_os = "macos") { eframe::wgpu::Backend::Metal } else { eframe::wgpu::Backend::Vulkan });
                #[cfg(not(target_os = "macos"))]
                assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
                #[cfg(target_os = "macos")]
                assert_ne!(info.device_type, eframe::wgpu::DeviceType::Cpu, "hardware Metal adapter required");
                assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&rs.adapter), eframe::wgpu::TextureFormat::Rgba32Float);
                eprintln!("PDF app adapter {info:?}");
                *capture.lock().unwrap() = Some(GpuCanvas::new(rs));
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
                app.set_wgpu(rs.clone());
                app
            },
        );
    let gpu = observer.lock().unwrap().clone().unwrap();
    let mut doc = Document::new("PDF app", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = 96.;
    doc.layers.push(Layer::new("Background", LayerContent::Fill(Fill::Solid(Color::WHITE))));
    let mut shape =
        ShapeLayer { path: photocraft_vector::shapes::rect(40., 40., 128., 128.), fill: Some(Fill::Solid(Color::rgb(0.2, 0.4, 0.8))), ..Default::default() };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let layer = Layer::new("Curve", LayerContent::Shape(shape));
    let id = layer.id;
    doc.layers.push(layer);
    h.state_mut().session.open_document(doc, None);
    h.state_mut().sync_views();
    h.state_mut().run("layer.select", json!({"layer":id.0})).unwrap();
    {
        let app = h.state_mut();
        app.ui.tool = Tool::Move;
        app.ui.tool_options.move_auto_select = false;
        app.ui.tool_options.move_show_transform = false;
        app.ui.extras.snap = false;
        app.ui.extras.rulers = false;
        app.ui.view.show.smart_guides = false;
        app.ui.view.show.layer_edges = false;
        app.session.edit_prefs(|p| {
            p.interface.show_bounding_box_when_dragging_layer = false;
            p.interface.contextual_task_bar = false;
        });
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].center = [128., 128.];
        app.ui.views[0].fit_pending = false;
    }
    present(&mut h);
    parity(&gpu, &h.state().session.active().unwrap().doc);
    let history = h.state().session.active().unwrap().history.past_len();
    let revision = h.state().session.active().unwrap().revision;
    let down = screen(&h, 80., 80.);
    let end = screen(&h, 100., 90.);
    h.event(Event::PointerMoved(down));
    h.event(Event::PointerButton { pos: down, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.event(Event::PointerMoved(end));
    present(&mut h);
    preview_pixels(&mut h, id, 20, 10);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    assert_eq!(h.state().session.active().unwrap().revision, revision);
    h.event(Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    present(&mut h);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    let original = h.state().session.active().unwrap().doc.clone();
    let LayerContent::Shape(shape) = &original.layer(id).unwrap().content else {
        panic!("moved vector shape");
    };
    assert_eq!(shape.path.subpaths[0].knots[0].anchor, photocraft_geom::Point::new(60., 50.));
    parity(&gpu, &original);
    let st = h.state().session.active().unwrap();
    let request = Request { document: st.doc.id.0, revision: st.revision, scope: Scope::Document, options: Default::default() };
    let output = h.state_mut().session.execute("file.pdf.export", serde_json::to_value(&request).unwrap()).unwrap();
    let bytes: Vec<u8> = serde_json::from_value(output["bytes"].clone()).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.7"));
    let written = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let record = written.clone();
    h.state_mut().services.pick_save = Some(Box::new(|_| Some("fixture.pdf".into())));
    h.state_mut().services.write = Some(Box::new(move |_, bytes| {
        record.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }));
    // Export As is enabled where the app has its raster exporter; PDF itself goes through pick_save/write.
    h.state_mut().services.export = Some(Box::new(|_, _, _| Err("raster export is not used by this PDF test".into())));
    h.get_by_label("File").click();
    h.run_steps(2);
    h.get_by_label("Export ⏵").hover();
    h.run_steps(3);
    // The visible PDF… item became a format of the unified Export As dialog (3b4604a1).
    h.get_by_label_contains("Export As…").click();
    h.run_steps(3);
    h.get(egui_kittest::kittest::By::new().role(egui::accesskit::Role::ComboBox).value("PNG")).click();
    h.run_steps(2);
    h.get_by_label("PDF").click();
    h.run_steps(3);
    h.get_by_label("Text as curves").click();
    h.run_steps(2);
    h.get_by_label("Compress PDF").click();
    h.run_steps(2);
    h.get_by_label("Export").click();
    h.run_steps(3);
    assert!(h.state().ui.pdf_export.error.is_none(), "{:?}", h.state().ui.pdf_export.error);
    assert_eq!(written.lock().unwrap().len(), 1);
    assert!(h.state().ui.pdf_export.request.is_none());
    assert_eq!(h.state().session.active().unwrap().revision, revision + 1);
    // A real raster layer produces the normal, visible app warning notice.
    let format = h.state().session.active().unwrap().doc.pixel_format();
    h.state_mut()
        .session
        .edit("Fixture pixel layer", |doc, _| {
            let mut surface = photocraft_doc::Surface::new(format);
            surface.write_region(photocraft_geom::Rect::new(8, 8, 16, 16), &[0.2, 0.1, 0.5, 1.0].repeat(64));
            doc.layers.push(Layer::new("Pixels", LayerContent::Raster(surface)));
            Ok(())
        })
        .unwrap();
    let ctx = h.ctx.clone();
    photocraft_ui_egui::menus::invoke(h.state_mut(), &ctx, "file.export.exportAs", json!({ "format": "pdf" })).unwrap();
    h.run_steps(3);
    h.get_by_label("Export").click();
    h.run_steps(3);
    assert_eq!(written.lock().unwrap().len(), 2);
    assert!(!h.state().ui.notices.is_empty());
    assert!(h.state().ui.notices.iter().any(|n| n.lines.iter().any(|line| line.contains("embedded"))));
    let warning = h.state().ui.notices.iter().flat_map(|n| &n.lines).find(|line| line.contains("embedded")).unwrap().clone();
    // Notice lines are shown under Details / History since the compact notice row (f0558183).
    h.get_by_label("Details / History").click();
    h.run_steps(2);
    let _ = h.get_by_label(&warning);

    #[cfg(feature = "pdf-import-integrated")]
    roundtrip(&mut h, &gpu, &original, &bytes);
}
#[cfg(feature = "pdf-import-integrated")]
fn roundtrip(h: &mut Harness<'static, PhotocraftApp>, gpu: &GpuCanvas, original: &Document, bytes: &[u8]) {
    let scene = photocraft_pdf::import::import(bytes, &ImportOptions { dpi: original.resolution_dpi, ..Default::default() }).unwrap();
    let mut imported = Document::new("Imported", scene.size, ColorMode::Rgb, SampleType::U8);
    imported.layers = scene.layers;
    imported.resolution_dpi = scene.resolution_dpi;
    let a = photocraft_compose::flatten(original);
    let b = photocraft_compose::flatten(&imported);
    assert_eq!(a.rect, b.rect);
    let max =
        a.px.iter()
            .zip(&b.px)
            .flat_map(|(a, b)| (0..4).map(move |k| (if k == 3 { a[k] - b[k] } else { a[k] * a[3] - b[k] * b[3] }).abs()))
            .fold(0_f32, f32::max);
    assert!(max <= 1. / 255., "PDF import/export premultiplied diff {max}");
    h.state_mut().session.open_document(imported, None);
    h.state_mut().sync_views();
    present(h);
    parity(gpu, &h.state().session.active().unwrap().doc);
}
