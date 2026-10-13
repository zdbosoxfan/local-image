#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::native_test_provider::*;
use super::*;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[allow(dead_code)]
mod fixture {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../pc-pdf/fixtures/import/source.rs"));
}
// Drive the native dialog via its actual accessibility-labelled controls without a window.
struct DialogDriver {
    ctx: egui::Context,
    buttons: BTreeMap<String, egui::Pos2>,
    time: f64,
}
impl DialogDriver {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, crate::theme::ThemeKind::Pro);
        Self { ctx, buttons: BTreeMap::new(), time: 0.0 }
    }
    fn frame(&mut self, app: &mut photocraft_ui_egui::PhotocraftApp, events: Vec<egui::Event>) {
        self.time += 0.016;
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 650.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| photocraft_ui_egui::pdf_import::show(app, ui.ctx()),
        );
        output.textures_delta.clear();
        if let Some(update) = output.platform_output.accesskit_update {
            for (_, node) in update.nodes {
                if let (Some(label), Some(bounds)) = (node.label(), node.bounds()) {
                    self.buttons.insert(label.to_owned(), egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32));
                }
            }
        }
    }
    fn click(&mut self, app: &mut photocraft_ui_egui::PhotocraftApp, label: &str) {
        for _ in 0..3 {
            self.frame(app, vec![]);
        }
        let pos = *self.buttons.get(label).unwrap_or_else(|| panic!("missing {label}: {:?}", self.buttons));
        self.frame(
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
            ],
        );
        self.frame(app, vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
        self.frame(app, vec![]);
    }
    fn finish(&mut self, app: &mut photocraft_ui_egui::PhotocraftApp) {
        let until = Instant::now() + Duration::from_secs(15);
        while app.pdf_import.pending() {
            self.frame(app, vec![]);
            assert!(Instant::now() < until, "dialog timed out: {:?}", app.pdf_import.error);
            std::thread::sleep(Duration::from_millis(2));
        }
        for _ in 0..2 {
            self.frame(app, vec![]);
        }
    }
}

#[test]
#[ignore = "production Linux PDF provider and actual Open/Place/drop controls; run explicitly"]
fn pdf_raster_native_dialog_open_place_drop_and_missing_renderer() {
    use photocraft_engine::Session;
    use photocraft_ui_egui::{PhotocraftApp, Services};
    for jbig2 in [false, true] {
        for route in ["open", "place", "drop"] {
            let path = std::env::temp_dir().join(format!("li-pdf-native-{}-{jbig2}-{route}.pdf", std::process::id()));
            let bytes = fixture::fallback_codec(jbig2);
            fs::write(&path, &bytes).unwrap();
            let mut picker = Some((path.to_string_lossy().into_owned(), bytes.clone()));
            let services = Services {
                pdf_raster: Some(Arc::new(run)),
                pick_place_path: Some(Box::new(move || picker.take().map(|(name, _)| name))),
                ..Default::default()
            };
            let mut app = PhotocraftApp::new(Session::new(), services);
            app.background_jobs = true;
            if route != "open" {
                app.run("file.new", serde_json::json!({"width":256,"height":256,"background":"transparent"})).unwrap();
            }
            let before = app.session.active().map(|s| s.doc.clone());
            let mut driver = DialogDriver::new();
            if route == "open" {
                app.open_path(path.to_str().unwrap()).unwrap();
            } else if route == "place" {
                crate::menus::invoke(&mut app, &driver.ctx, "file.placeEmbedded", serde_json::json!({})).unwrap();
            } else {
                #[derive(Debug)]
                struct DropFile(PathBuf);
                impl egui::DroppedFile for DropFile {
                    fn path(&self) -> &Path {
                        &self.0
                    }
                    fn bytes(&self) -> std::result::Result<Vec<u8>, String> {
                        Err("native path must be used".into())
                    }
                }
                app.drop_canvas_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 500.0)));
                app.open_dropped(&driver.ctx, vec![Arc::new(DropFile(path.clone()))], Some(egui::pos2(10.0, 10.0)));
                app.place_next_dropped(&driver.ctx);
            }
            driver.finish(&mut app);
            assert!(app.pdf_import.info.is_none());
            driver.click(&mut app, "Use image fallback");
            driver.finish(&mut app);
            assert!(app.pdf_import.error.is_none(), "{:?}", app.pdf_import.error);
            let start = Instant::now();
            driver.click(&mut app, if route == "open" { "Open as image" } else { "Place as image" });
            driver.finish(&mut app);
            assert!(app.pdf_import.error.is_none(), "{:?}", app.pdf_import.error);
            eprintln!("real PDF {route}, jbig2={jbig2}: {:?}", start.elapsed());
            let state = app.session.active().unwrap();
            assert!(state.path.is_none());
            if let Some(before) = before {
                assert_eq!(state.doc.layer_count(), before.layer_count() + 2);
                assert!(app.session.undo());
                assert_eq!(app.session.active().unwrap().doc, before);
                assert!(app.session.redo());
            } else {
                assert_eq!(state.doc.size, photocraft_doc::Size::new(64, 64));
            }
            let pixels = photocraft_compose::render(&app.session.active().unwrap().doc, photocraft_doc::Rect::new(0, 0, 64, 64));
            let expected = if jbig2 { [0.0, 0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0, 1.0] };
            assert_eq!(pixels.get(16, 16), expected, "published page fidelity on {route}");
            let exported = photocraft_io::export(&app.session.active().unwrap().doc, "png", &photocraft_io::ExportOptions::default()).unwrap();
            let exported = image::load_from_memory(&exported.bytes).unwrap().to_rgba8();
            assert_eq!(exported.get_pixel(16, 16).0, if jbig2 { [0, 0, 0, 255] } else { [255, 0, 0, 255] });
            // Native save/reopen through the production document codec, preserving raster content.
            let saved = photocraft_format::save_to_bytes(&app.session.active().unwrap().doc, &photocraft_format::SaveOptions::default()).unwrap();
            let loaded = photocraft_format::load_from_bytes(&saved).unwrap();
            assert_eq!(loaded, *app.session.active().unwrap().doc);
            fs::remove_file(path).unwrap();
        }
    }
    let services = Services {
        pdf_raster: Some(Arc::new(|_, _| {
            Err(crate::pdf_raster::Error::Unavailable("missing /usr/bin/pdftoppm; install Poppler, bubblewrap and a systemd user session".into()))
        })),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(Session::new(), services);
    app.background_jobs = true;
    app.open_file("missing.pdf", &fixture::fallback_codec(false)).unwrap();
    let mut driver = DialogDriver::new();
    driver.finish(&mut app);
    driver.click(&mut app, "Use image fallback");
    driver.finish(&mut app);
    assert!(app.pdf_import.error.as_ref().unwrap().contains("install Poppler"));
    assert!(app.session.documents().is_empty());
}

#[test]
#[ignore = "real PDF renderer mid-render UI Cancel and stale destination; production Linux runtime"]
fn pdf_raster_native_dialog_cancel_mid_render_and_stale_target() {
    use std::sync::atomic::Ordering;
    let services = crate::Services { pdf_raster: Some(Arc::new(run)), ..Default::default() };
    let mut app = crate::PhotocraftApp::new(Session::new(), services);
    app.background_jobs = true;
    // Hundreds of full-page calculator shadings keep the actual pdftoppm busy.
    let slow = fixture::page(
        &"/S sh ".repeat(2000),
        "/Shading << /S 5 0 R >>",
        "",
        &[
            b"<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 64 0] /Function 6 0 R /Extend [true true] >>".to_vec(),
            fixture::stream("/FunctionType 4 /Domain [0 1] /Range [0 1 0 1 0 1]", b"{ pop 1 0 0 }"),
        ],
    );
    app.open_file("slow.pdf", &slow).unwrap();
    let mut driver = DialogDriver::new();
    driver.finish(&mut app);
    driver.click(&mut app, "Use image fallback");
    driver.finish(&mut app);
    app.pdf_import.dpi = 2400.0;
    driver.click(&mut app, "Open as image");
    let until = Instant::now() + Duration::from_secs(3);
    let unit = loop {
        if let Some(unit) = running_render_unit() {
            break unit;
        }
        driver.frame(&mut app, vec![]);
        assert!(Instant::now() < until, "renderer never became active: {:?}", app.pdf_import.error);
        std::thread::sleep(Duration::from_millis(2));
    };
    driver.click(&mut app, "Cancel");
    assert!(app.session.documents().is_empty());
    let until = Instant::now() + Duration::from_secs(3);
    while !stopped_render_unit(&unit) {
        assert!(Instant::now() < until, "owned render scope remained after Cancel");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.pdf_import.pending());
    // A completed real codec render waits at an injected delivery barrier. Changing
    // the captured selection before its delivery must discard those real pixels.
    let arrived = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));
    let arrived_worker = arrived.clone();
    let released_worker = released.clone();
    app.services.pdf_raster = Some(Arc::new(move |request, cancel| {
        let rendering = matches!(request.operation, crate::pdf_raster::Operation::Render { .. });
        let reply = run(request, cancel)?;
        if rendering {
            arrived_worker.store(true, Ordering::Relaxed);
            while !released_worker.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        Ok(reply)
    }));
    app.run("file.new", serde_json::json!({"width":128,"height":128})).unwrap();
    app.place_bytes("stale.pdf", fixture::fallback_codec(false), None).unwrap();
    driver.finish(&mut app);
    driver.click(&mut app, "Use image fallback");
    driver.finish(&mut app);
    driver.click(&mut app, "Place as image");
    let until = Instant::now() + Duration::from_secs(3);
    while !arrived.load(Ordering::Relaxed) {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(2));
    }
    app.run("layer.new.layer", serde_json::json!({"name":"Changed target"})).unwrap();
    let changed = app.session.active().unwrap().doc.clone();
    released.store(true, Ordering::Relaxed);
    driver.finish(&mut app);
    assert_eq!(app.session.active().unwrap().doc, changed);
    assert!(app.pdf_import.preview.is_none());
    assert!(app.pdf_import.warnings.is_empty());
}

#[test]
#[ignore = "RTX 5090 PDF import screenshots, dark/light, narrow/normal, 1×/1.5×; no window"]
fn pdf_raster_design_system_screens() {
    use crate::theme::ThemeKind;
    use egui_kittest::Harness;
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| a.get_info().device_type == eframe::wgpu::DeviceType::DiscreteGpu && a.get_info().name.contains("5090"))
            .ok_or("RTX 5090 required")?;
        eprintln!("PDF UI adapter: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let renderer = egui_kittest::wgpu::WgpuTestRenderer::from_setup(eframe::egui_wgpu::WgpuSetup::CreateNew(setup));
    let mut session = Session::new();
    session.execute("file.new", serde_json::json!({"width":128,"height":128})).unwrap();
    let mut state = State { raster_service: Some(Arc::new(run)), ..Default::default() };
    state.set_named_source("original-codec.pdf", fixture::fallback_codec(false)).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(400.0, 650.0)).renderer(renderer).build_ui_state(
        |ui, (state, session): &mut (State, Session)| {
            let id = egui::Id::new("pdf-shots-fonts");
            if !ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
                crate::theme::install_fonts(ui.ctx());
                crate::theme::apply(ui.ctx(), ThemeKind::Pro);
                ui.ctx().data_mut(|d| d.insert_temp(id, true));
                ui.ctx().request_repaint();
                return;
            }
            controls(ui, state, session);
        },
        (state, session),
    );
    h.run_steps(4);
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system/pdf-fallback");
    fs::create_dir_all(&output).unwrap();
    for image in [false, true] {
        h.state_mut().0.image_mode = image;
        h.state_mut().0.image_pages = usize::from(image);
        if image {
            h.state_mut().0.error = None;
        }
        for (theme, name) in [(ThemeKind::Pro, "dark"), (ThemeKind::StudioLight, "light")] {
            crate::theme::apply(&h.ctx, theme);
            for width in [250.0, 400.0] {
                for scale in [1.0, 1.5] {
                    h.set_pixels_per_point(scale);
                    h.run_steps(1); // egui applies zoom before accepting the new logical size.
                    h.set_size(egui::vec2(width, 650.0));
                    h.run_steps(4);
                    h.render().unwrap().save(output.join(format!("{name}-{}-{scale}-{}.png", width as u32, if image { "ready" } else { "choice" }))).unwrap();
                }
            }
        }
    }
}
