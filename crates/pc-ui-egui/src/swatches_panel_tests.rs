#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
    app.run("file.new", serde_json::json!({"width":96,"height":64})).unwrap();
    app
}
fn harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    Harness::builder().with_size(egui::vec2(520., 760.)).build_ui_state(|ui, app| panel(app, ui), app)
}
fn click(h: &mut Harness<'_, PhotocraftApp>, label: &str) {
    h.get_by_label(label).click();
    h.run();
}
/// A command in the panel ☰ menu.
fn menu(h: &mut Harness<'_, PhotocraftApp>, label: &str) {
    click(h, "Swatches panel menu");
    click(h, label);
}
fn drag(h: &mut Harness<'_, PhotocraftApp>, from: egui::Pos2, to: egui::Pos2) {
    h.hover_at(from);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=10 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (i as f32 / 10.)));
        h.run_steps(1);
    }
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(3);
}
#[test]
fn shared_swatches_click_add_rename_search_library_and_restart() {
    let mut h = harness(app());
    h.run();
    let n = h.state().session.prefs().user_swatches.palette.swatches.len();
    click(&mut h, "New Swatch");
    assert_eq!(h.state().session.prefs().user_swatches.palette.swatches.len(), n + 1);
    click(&mut h, "Colour");
    menu(&mut h, "Swatch Options…");
    let field = h.get_by_label("Swatch name");
    field.click();
    h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
    field.type_text("Ocean Brand");
    h.run();
    click(&mut h, "Save Swatch");
    assert!(h.state().session.prefs().user_swatches.palette.swatches.iter().any(|w| w.name == "Ocean Brand"));
    h.get_by_label("Search swatches").click();
    h.get_by_label("Search swatches").type_text("Ocean");
    h.run();
    assert!(h.query_by_label("Ocean Brand").is_some());
    assert!(h.query_by_label("Starter 1").is_none());
    click(&mut h, "Swatches panel menu");
    click(&mut h, "Swatch Libraries ⏵");
    click(&mut h, "Open Color");
    assert!(h.state().session.prefs().user_swatches.palette.groups.iter().any(|g| g.name == "Open Color"));
    menu(&mut h, "List View");
    assert!(h.state().session.prefs().user_swatches.list_view);
    let json = h.state().session.prefs_to_json();
    let before = h.state().session.prefs().user_swatches.clone();
    let mut fresh = app();
    fresh.session.load_prefs_json(&json).unwrap();
    assert_eq!(fresh.session.prefs().user_swatches, before);
    let mut restarted = harness(fresh);
    restarted.run();
    assert_eq!(restarted.state().session.prefs().user_swatches, before);
}
#[test]
fn shared_swatches_drag_reorders_within_and_between_folders() {
    let mut h = harness(app());
    h.run();
    let a = h.get_by_label("Starter 1").rect().center();
    let b = h.get_by_label("Starter 3").rect().center();
    drag(&mut h, a, b);
    let names: Vec<_> = h.state().session.prefs().user_swatches.palette.swatches.iter().take(3).map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["Starter 2", "Starter 1", "Starter 3"]);
    click(&mut h, "New Group");
    let a = h.get_by_label("Starter 1").rect().center();
    let folder = h.get_by_label("Colour Group").rect().center();
    drag(&mut h, a, folder);
    let b = &h.state().session.prefs().user_swatches.palette;
    let id = b.groups.iter().find(|g| g.name == "Colour Group").unwrap().id;
    assert_eq!(b.swatches.iter().find(|w| w.name == "Starter 1").unwrap().group, Some(id));
}
#[test]
fn shared_swatches_personas_same_panel_alt_secondary_and_document_visibility() {
    let mut h = harness(app());
    h.run();
    assert!(h.query_by_label("Foreground").is_some());
    assert!(h.query_by_label("Background").is_some());
    h.get_by_label("Starter 3").click_modifiers(egui::Modifiers::ALT);
    h.run();
    let bg = h.state().session.tools.background;
    let fg = h.state().session.tools.foreground;
    assert_ne!(bg, fg);
    let before = h.state().session.prefs().user_swatches.clone();
    h.state_mut().session.edit_prefs(|p| p.toolbar.persona = Persona::Vector);
    h.run();
    assert!(h.query_by_label("Fill").is_some());
    assert!(h.query_by_label("Stroke").is_some());
    assert!(h.query_by_label("Foreground").is_none());
    assert!(h.query_by_label("Background").is_none());
    for label in ["New Swatch", "Search swatches", "New Group", "User Swatches", "Document Swatches", "Starter 3"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    assert_eq!(h.state().session.prefs().user_swatches, before);
    h.state_mut().session.edit_prefs(|p| p.toolbar.persona = Persona::Pixel);
    h.run();
    click(&mut h, "Document Swatches");
    click(&mut h, "New Swatch");
    assert_eq!(h.state().session.active().unwrap().doc.swatches.swatches.len(), 1);
}
#[test]
fn shared_swatches_import_menu_each_format_and_export_container() {
    let b = library("CMYK", None).unwrap();
    let p = swatches::to_palette(&b).unwrap().palette;
    let mut zip = photocraft_format::zip::ZipWriter::new();
    zip.add("colorset.xml", &photocraft_color::kpl::write_xml(&p).unwrap()).unwrap();
    let fixtures = [
        ("Palette.ase", ase::write(&p).unwrap()),
        ("Palette.aco", photocraft_color::aco::write(&p).unwrap()),
        ("Palette.gpl", photocraft_color::gpl::write(&p).unwrap()),
        ("Palette.kpl", zip.finish().unwrap()),
    ];
    for (name, bytes) in fixtures {
        let mut app = app();
        let name = name.to_owned();
        app.services.file_dialog = Some(crate::file_dialog::answering(move |_| Some(crate::FileDialogAnswer::File(name.clone(), Ok(bytes.clone())))));
        let mut h = harness(app);
        h.run();
        click(&mut h, "Document Swatches");
        menu(&mut h, "Import Swatches…");
        let b = &h.state().session.active().unwrap().doc.swatches;
        assert_eq!(b.swatches.len(), 4);
        assert_eq!(b.groups.len(), 1);
        assert!(b.swatches.iter().all(|w| matches!(w.paint, Fill::Solid(Color { mode: ColorMode::Rgb, .. }))));
        for ext in ["ase", "aco", "gpl", "kpl"] {
            let (bytes, _) = export(h.state(), Scope::Document, ext).unwrap();
            assert!(!bytes.is_empty());
            if ext == "kpl" {
                let zip = photocraft_format::zip::ZipReader::new(&bytes).unwrap();
                let xml = zip.read_by_name("colorset.xml", ase::MAX_BYTES).unwrap();
                assert_eq!(photocraft_color::kpl::read_xml(&xml).unwrap().palette.groups[0].swatches.len(), 4);
            }
        }
    }
}
#[test]
fn shared_swatches_live_edit_shape_in_pixel_mode_undo_and_stale_editor() {
    use photocraft_doc::vector_design::swatches::{PaintLink, SwatchRef};
    use photocraft_doc::{Document, Layer, LayerContent, SampleType, ShapeLayer, Size};
    let mut doc = Document::new("Pixel live", Size::new(96, 64), ColorMode::Rgb, SampleType::U8);
    let mut b = library("RGB", None).unwrap();
    b.swatches.truncate(1);
    b.swatches[0].name = "Live Red".into();
    b.swatches[0].global = true;
    let w = b.swatches[0].clone();
    let layer = Layer::new(
        "Shape",
        LayerContent::Shape(ShapeLayer { path: photocraft_vector::shapes::rect(5., 5., 20., 20.), fill: Some(w.paint.clone()), ..Default::default() }),
    );
    let id = layer.id;
    doc.layers.push(layer);
    b.links.push(PaintLink {
        target: PaintTarget { symbol: None, layer: id, paint: PaintSlot::Fill, stop: None },
        reference: SwatchRef { swatch: w.id, tint: 100. },
    });
    doc.swatches = b;
    let mut s = Session::new();
    s.add_document(doc, None);
    let mut h = harness(PhotocraftApp::new(s, crate::Services::default()));
    h.run();
    click(&mut h, "Document Swatches");
    click(&mut h, "Live Red");
    menu(&mut h, "Swatch Options…");
    h.get_by_label("Swatch hex colour").click();
    h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.get_by_label("Swatch hex colour").type_text("#00FF00");
    h.run();
    click(&mut h, "Save Swatch");
    let layer = h.state().session.active().unwrap().doc.layer(id).unwrap();
    let LayerContent::Shape(sh) = &layer.content else { panic!("shape") };
    assert_eq!(sh.fill, Some(Fill::Solid(Color::rgb(0., 1., 0.))));
    assert!(sh.cache.is_some());
    assert_eq!(h.state().session.prefs().toolbar.persona, Persona::Pixel);
    assert!(h.state_mut().session.undo());
    h.run();
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content else { panic!("shape") };
    assert_eq!(sh.fill, Some(w.paint));
    menu(&mut h, "Swatch Options…");
    h.state_mut()
        .session
        .edit("Change", |d, _| {
            d.name = "Changed".into();
            Ok(())
        })
        .unwrap();
    h.run();
    assert!(h.query_by_label("Save Swatch").is_none());
}
#[test]
fn shared_swatches_gpu_screenshots_dark_light_narrow_normal() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};
    use eframe::wgpu;
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let info = a.get_info();
                if cfg!(target_os = "macos") {
                    info.backend == wgpu::Backend::Metal && info.device_type != wgpu::DeviceType::Cpu
                } else {
                    info.device_type == wgpu::DeviceType::DiscreteGpu && info.name.contains("5090")
                }
            })
            .ok_or("hardware Metal (macOS) or RTX 5090 required")?;
        eprintln!("Swatches renderer: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let renderer = egui_kittest::wgpu::WgpuTestRenderer::from_setup(WgpuSetup::CreateNew(setup));
    // Reuse the real hardware renderer for all four captures; no GUI window.
    let mut h = Harness::builder().with_size(egui::vec2(520., 760.)).renderer(renderer).build_ui_state(
        |ui, app| {
            ui.set_min_height(ui.available_height());
            panel(app, ui);
        },
        app(),
    );
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/swatches-ux");
    std::fs::create_dir_all(&output).unwrap();
    for (theme, name) in [(crate::theme::ThemeKind::Studio, "dark"), (crate::theme::ThemeKind::StudioLight, "light")] {
        crate::theme::apply(&h.ctx, theme);
        for width in [250., 520.] {
            h.set_size(egui::vec2(width, 760.));
            h.run();
            h.render().unwrap().save(output.join(format!("{name}-{}.png", width as u32))).unwrap();
        }
    }
    let start = std::time::Instant::now();
    h.get_by_label("New Swatch").click();
    h.run();
    let _ = h.render().unwrap();
    let ms = start.elapsed().as_secs_f64() * 1000.;
    std::fs::write(output.join("runtime.txt"), format!("{}\nWarm Add Current Colour click, real panel + preference update + GPU render/readback: {ms:.3} ms\nHeadless UI diagnostic, not a desktop frame-rate or compositor benchmark.\n", if cfg!(target_os = "macos") { "hardware Metal" } else { "RTX 5090 / Vulkan" })).unwrap();
    eprintln!("Swatches warm click + GPU readback: {ms:.3} ms");
}

#[derive(Debug)]
struct MemoryDrop {
    path: std::path::PathBuf,
    bytes: Vec<u8>,
}
impl egui::DroppedFile for MemoryDrop {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.bytes.clone())
    }
    #[cfg(target_arch = "wasm32")]
    fn bytes_async(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + '_>> {
        Box::pin(async { Ok(self.bytes.clone()) })
    }
}
#[test]
fn shared_swatches_drop_import_boundaries_and_no_duplicate_import() {
    let mut h = Harness::builder().with_size(egui::vec2(520., 760.)).build_ui_state(
        |ui, app| {
            // Mirror the desktop pre-panel dispatch. Leave raw input present to
            // exercise the panel's same-frame duplicate guard as well.
            let files = ui.input(|i| i.raw.dropped_files.clone());
            let _ = route_drops(app, ui.ctx(), files, ui.ctx().pointer_latest_pos());
            panel(app, ui);
        },
        app(),
    );
    h.run();
    let before = h.state().session.prefs().user_swatches.palette.swatches.len();
    let at = h.get_by_label("Starter 1").rect().center();
    h.hover_at(at);
    let file: egui::DroppedFileHandle =
        std::sync::Arc::new(MemoryDrop { path: "Dropped.gpl".into(), bytes: b"GIMP Palette\nName: Dropped\n1 2 3 Drop Ink\n".to_vec() });
    h.input_mut().dropped_files.push(file.clone());
    h.run_steps(1);
    assert_eq!(h.state().session.prefs().user_swatches.palette.swatches.len(), before + 1);
    let ctx = h.ctx.clone();
    let result = route_drops(h.state_mut(), &ctx, vec![file], Some(egui::pos2(-100., -100.)));
    assert_eq!(result.len(), 1);
    let unrelated: egui::DroppedFileHandle = std::sync::Arc::new(MemoryDrop { path: "Photo.png".into(), bytes: vec![0] });
    assert_eq!(route_drops(h.state_mut(), &ctx, vec![unrelated], Some(at)).len(), 1);
}
#[test]
fn shared_swatches_native_document_roundtrip_and_empty_user_palette_restart() {
    let mut app = app();
    let palette = library("RGB", None).unwrap();
    apply(&mut app, Scope::Document, Action::Import { palette, replace: false }).unwrap();
    let before = app.session.active().unwrap().doc.swatches.clone();
    let bytes = photocraft_format::save_to_bytes(&app.session.active().unwrap().doc, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded.swatches, before);
    app.session.edit_prefs(|p| {
        p.user_swatches.initialized = true;
        p.user_swatches.palette = SwatchBook::default();
        p.user_swatches.list_view = true;
    });
    let json = app.session.prefs_to_json();
    let mut fresh = PhotocraftApp::new(Session::new(), crate::Services::default());
    fresh.session.load_prefs_json(&json).unwrap();
    let mut h = harness(fresh);
    h.run();
    assert!(h.state().session.prefs().user_swatches.palette.is_empty());
    assert!(h.state().session.prefs().user_swatches.list_view);
}
#[test]
fn shared_swatches_kpl_embedded_profile_spot_and_malformed_archive() {
    let mut app = app();
    let xml=b"<ColorSet version='2.0'><Group name='Profile Group'><ColorSetEntry name='Spot' spot='true'><RGB r='0.25' g='0.5' b='0.75' space='Adobe RGB'/><Position row='0' column='0'/></ColorSetEntry></Group></ColorSet>";
    let mut zip = photocraft_format::zip::ZipWriter::new();
    zip.add("colorset.xml", xml).unwrap();
    zip.add("profiles.xml", b"<Profiles><Profile name='Adobe RGB' filename='profile.icc' colorModelId='RGBA' colorDepthId='F32'/></Profiles>").unwrap();
    zip.add("profile.icc", &photocraft_cms::Builtin::AdobeRgbCompat.profile().to_bytes()).unwrap();
    let bytes = zip.finish().unwrap();
    import(&mut app, Scope::Document, "Custom.kpl", &bytes).unwrap();
    let w = &app.session.active().unwrap().doc.swatches.swatches[0];
    assert!(w.spot && w.global);
    let Fill::Solid(c) = w.paint else { panic!("solid") };
    assert_ne!(c, Color::rgb(0.25, 0.5, 0.75));
    for i in [0, 1, bytes.len() / 2, bytes.len() - 1] {
        assert!(import(&mut app, Scope::Document, "Bad.kpl", &bytes[..i]).is_err());
    }
    let mut duplicate = photocraft_format::zip::ZipWriter::new();
    duplicate.add("colorset.xml", xml).unwrap();
    duplicate.add("colorset.xml", xml).unwrap();
    assert!(import(&mut app, Scope::Document, "Bad.kpl", &duplicate.finish().unwrap()).is_err());
}

#[test]
fn shared_swatches_pixel_foreground_explicit_apply_and_vector_selected_fill() {
    use photocraft_doc::{Document, Layer, LayerContent, SampleType, ShapeLayer, Size};
    let mut doc = Document::new("Targets", Size::new(96, 64), ColorMode::Rgb, SampleType::U8);
    let layer = Layer::new(
        "Shape",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(5., 5., 20., 20.),
            fill: Some(Fill::Solid(Color::rgb(1., 0., 0.))),
            ..Default::default()
        }),
    );
    let id = layer.id;
    doc.layers.push(layer);
    doc.swatches.swatches.push(Swatch {
        id: SwatchId(1),
        name: "Live Blue".into(),
        paint: Fill::Solid(Color::rgb(0., 0., 1.)),
        global: true,
        spot: false,
        group: None,
        stops: vec![],
    });
    let mut s = Session::new();
    s.add_document(doc, None);
    let mut h = harness(PhotocraftApp::new(s, crate::Services::default()));
    h.run();
    click(&mut h, "Document Swatches");
    click(&mut h, "Live Blue");
    let target = PaintTarget { symbol: None, layer: id, paint: PaintSlot::Fill, stop: None };
    assert_eq!(photocraft_engine::vector_packages::colours::color(&h.state().session.active().unwrap().doc, &target).unwrap(), Color::rgb(1., 0., 0.));
    assert_eq!(h.state().session.tools.foreground, [0., 0., 1., 1.]);
    menu(&mut h, "Apply to Selected Objects");
    assert_eq!(h.state().session.active().unwrap().doc.swatches.links.len(), 1);
    assert_eq!(photocraft_engine::vector_packages::colours::color(&h.state().session.active().unwrap().doc, &target).unwrap(), Color::rgb(0., 0., 1.));
    assert!(h.state_mut().session.undo());
    h.state_mut().session.edit_prefs(|p| p.toolbar.persona = Persona::Vector);
    h.run();
    assert_eq!(h.state().session.tools.foreground, [0., 0., 1., 1.], "idle Fill display preserves drawing defaults");
    assert!(h.query_by_label("#FF0000").is_some(), "picker shows the selected object's Fill");
    click(&mut h, "Live Blue");
    assert_eq!(h.state().session.active().unwrap().doc.swatches.links.len(), 1);
    assert_eq!(photocraft_engine::vector_packages::colours::color(&h.state().session.active().unwrap().doc, &target).unwrap(), Color::rgb(0., 0., 1.));
}

/// UX-polish screenshots: 250/400 pt docks, dark/light, 1×/1.5×, into `target/ux-polish/`.
/// `UX_POLISH_PHASE` names the set (before/after). Real RTX 5090 renderer, no window.
#[test]
fn ux_polish_swatches_screens() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};
    use eframe::wgpu;
    let phase = std::env::var("UX_POLISH_PHASE").unwrap_or_else(|_| "after".into());
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ux-polish");
    std::fs::create_dir_all(&output).unwrap();
    for ppp in [1.0f32, 1.5] {
        let mut setup = WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
        setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
            let a = adapters
                .iter()
                .find(|a| {
                    let info = a.get_info();
                    if cfg!(target_os = "macos") {
                        info.backend == wgpu::Backend::Metal && info.device_type != wgpu::DeviceType::Cpu
                    } else {
                        info.device_type == wgpu::DeviceType::DiscreteGpu && info.name.contains("5090")
                    }
                })
                .ok_or("hardware Metal (macOS) or RTX 5090 required")?;
            Ok(a.clone())
        }));
        let renderer = egui_kittest::wgpu::WgpuTestRenderer::from_setup(WgpuSetup::CreateNew(setup));
        let mut h = Harness::builder().with_size(egui::vec2(250., 900.)).with_pixels_per_point(ppp).renderer(renderer).build_ui_state(
            |ui, app| {
                egui_extras::install_image_loaders(ui.ctx());
                ui.set_min_height(ui.available_height());
                panel(app, ui);
            },
            app(),
        );
        for (theme, name) in [(crate::theme::ThemeKind::Studio, "dark"), (crate::theme::ThemeKind::StudioLight, "light")] {
            crate::theme::apply(&h.ctx, theme);
            for width in [250., 400.] {
                h.set_size(egui::vec2(width, 900.));
                h.run();
                h.render().unwrap().save(output.join(format!("{phase}-swatches-{name}-{}-{ppp}x.png", width as u32))).unwrap();
            }
        }
        // After-only states: the panel menu and a swatch context menu (dark, 250 pt).
        crate::theme::apply(&h.ctx, crate::theme::ThemeKind::Studio);
        h.set_size(egui::vec2(250., 900.));
        h.run();
        if h.query_by_label("Swatches panel menu").is_some() {
            h.get_by_label("Swatches panel menu").click();
            h.run();
            h.render().unwrap().save(output.join(format!("{phase}-swatches-menu-dark-250-{ppp}x.png"))).unwrap();
            h.key_press(egui::Key::Escape);
            h.run();
            h.get_by_label("Starter 2").click_secondary();
            h.run();
            h.render().unwrap().save(output.join(format!("{phase}-swatches-context-dark-250-{ppp}x.png"))).unwrap();
        }
    }
}

fn disabled(h: &Harness<'_, PhotocraftApp>, label: &str) -> bool {
    use egui_kittest::kittest::NodeT;
    h.get_by_label(label).accesskit_node().is_disabled()
}
fn user_book(h: &Harness<'_, PhotocraftApp>) -> SwatchBook {
    h.state().session.prefs().user_swatches.palette.clone()
}
#[test]
fn ux_swatches_footer_icons_disable_until_selection_and_act() {
    let mut h = harness(app());
    h.run();
    // Old stacked buttons and hint lines are gone; the footer and ☰ carry the commands.
    for gone in ["Add Current Colour", "New Folder", "Delete Selected", "Move Selected", "Swatches menu", "Click: primary · Alt/right-click: secondary"] {
        assert!(h.query_by_label(gone).is_none(), "{gone}");
    }
    assert!(disabled(&h, "Delete Selected Swatch"), "nothing selected yet");
    assert!(!disabled(&h, "New Swatch"));
    assert!(!disabled(&h, "New Group"));
    let n = user_book(&h).swatches.len();
    // Disabled: a click does nothing.
    click(&mut h, "Delete Selected Swatch");
    assert_eq!(user_book(&h).swatches.len(), n);
    click(&mut h, "Starter 4");
    assert!(!disabled(&h, "Delete Selected Swatch"));
    click(&mut h, "New Swatch");
    let b = user_book(&h);
    assert_eq!(b.swatches.len(), n + 1);
    let added = b.swatches.iter().find(|w| w.name == "Colour").unwrap();
    assert_eq!(added.group, Some(1), "+ adds into the selected swatch's folder");
    click(&mut h, "Delete Selected Swatch");
    assert!(user_book(&h).swatches.iter().all(|w| w.name != "Colour"), "deletes the new (selected) swatch");
    click(&mut h, "Starter 4");
    click(&mut h, "New Group");
    let b = user_book(&h);
    let g = b.groups.iter().find(|g| g.name == "Colour Group").unwrap().id;
    assert_eq!(b.swatches.iter().find(|w| w.name == "Starter 4").unwrap().group, Some(g), "the selected swatch moves into the new group");
    // Keyboard: focus a footer icon, Enter activates it.
    h.get_by_label("New Swatch").focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(user_book(&h).swatches.len(), n + 1);
}
#[test]
fn ux_swatches_panel_menu_items() {
    let mut h = harness(app());
    h.run();
    click(&mut h, "Swatches panel menu");
    for label in ["Grid View", "List View", "Swatch Libraries ⏵", "Import Swatches…", "Export Swatches ⏵", "New Swatch from Current Colour", "New Swatch Group"]
    {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    for label in ["Swatch Options…", "Move to… ⏵", "Delete Swatch", "Apply to Selected Objects", "Folder Options…"] {
        assert!(disabled(&h, label), "{label} waits for a selection");
    }
    h.key_press(egui::Key::Escape);
    h.run();
    menu(&mut h, "List View");
    assert!(h.state().session.prefs().user_swatches.list_view);
    menu(&mut h, "Grid View");
    assert!(!h.state().session.prefs().user_swatches.list_view);
    let n = user_book(&h).swatches.len();
    menu(&mut h, "New Swatch from Current Colour");
    assert_eq!(user_book(&h).swatches.len(), n + 1);
    menu(&mut h, "New Swatch Group");
    assert!(user_book(&h).groups.iter().any(|g| g.name == "Colour Group"));
    // Folder Options… on the selected folder (its header was selected by New Group).
    menu(&mut h, "Folder Options…");
    assert!(h.query_by_label("Save Folder").is_some());
    click(&mut h, "Cancel");
    click(&mut h, "Starter 2");
    click(&mut h, "Swatches panel menu");
    click(&mut h, "Move to… ⏵");
    click(&mut h, "Move Later");
    let names: Vec<_> = user_book(&h).swatches.iter().filter(|w| w.group == Some(1)).take(3).map(|w| w.name.clone()).collect();
    assert_eq!(names, ["Starter 1", "Starter 3", "Starter 2"]);
    menu(&mut h, "Delete Swatch");
    assert!(user_book(&h).swatches.iter().all(|w| w.name != "Starter 2"));
}
#[test]
fn ux_swatches_context_menus_on_swatch_and_group() {
    let mut h = harness(app());
    h.run();
    let fg = h.state().session.tools.foreground;
    h.get_by_label("Starter 5").click_secondary();
    h.run();
    assert_eq!(h.state().session.tools.foreground, fg, "right-click opens the menu without recolouring");
    for label in ["Use as Foreground", "Use as Background", "Swatch Options…", "Move to Folder ⏵", "Move Earlier", "Move Later", "Delete Swatch"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    click(&mut h, "Use as Background");
    let c = super::super::SWATCHES[4];
    assert_eq!(h.state().session.tools.background, [c[0] as f32 / 255., c[1] as f32 / 255., c[2] as f32 / 255., 1.]);
    h.get_by_label("Starter 5").click_secondary();
    h.run();
    click(&mut h, "Move Earlier");
    let names: Vec<_> = user_book(&h).swatches.iter().take(5).map(|w| w.name.clone()).collect();
    assert_eq!(names[3], "Starter 5");
    h.get_by_label("Starter 5").click_secondary();
    h.run();
    click(&mut h, "Delete Swatch");
    assert!(user_book(&h).swatches.iter().all(|w| w.name != "Starter 5"));
    // Group header menu.
    h.get_by_label("Starter Colours").click_secondary();
    h.run();
    for label in ["New Swatch in Folder", "Rename Folder…", "Delete Folder and Swatches"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    let n = user_book(&h).swatches.len();
    click(&mut h, "New Swatch in Folder");
    assert_eq!(user_book(&h).swatches.iter().filter(|w| w.group == Some(1)).count(), n + 1);
    h.get_by_label("Starter Colours").click_secondary();
    h.run();
    click(&mut h, "Rename Folder…");
    assert!(h.query_by_label("Save Folder").is_some());
    click(&mut h, "Cancel");
    h.get_by_label("Starter Colours").click_secondary();
    h.run();
    click(&mut h, "Delete Folder and Swatches");
    assert!(user_book(&h).groups.is_empty());
}
#[test]
fn ux_swatches_segmented_scope_search_clear_and_hints_in_tooltips() {
    let mut h = harness(app());
    h.run();
    click(&mut h, "Document Swatches");
    click(&mut h, "New Swatch");
    assert_eq!(h.state().session.active().unwrap().doc.swatches.swatches.len(), 1);
    click(&mut h, "User Swatches");
    h.get_by_label("Search swatches").click();
    h.get_by_label("Search swatches").type_text("Starter 1");
    h.run();
    assert!(h.query_by_label("Starter 2").is_none());
    click(&mut h, "Clear search");
    assert!(h.query_by_label("Starter 2").is_some());
    assert!(h.query_by_label("Clear search").is_none());
    // The old hint line lives in the swatch tooltip now.
    h.get_by_label("Starter 2").hover();
    h.run_steps(30);
    assert!(h.query_by_label_contains("Alt-click: background").is_some());
}
#[test]
fn ux_swatches_empty_ungrouped_appears_last_while_dragging() {
    let mut h = harness(app());
    h.run();
    assert!(h.query_by_label("Ungrouped").is_none(), "hidden while empty");
    let from = h.get_by_label("Starter 1").rect().center();
    h.hover_at(from);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=4 {
        h.event(egui::Event::PointerMoved(from + egui::vec2(3. * i as f32, 3. * i as f32)));
        h.run_steps(1);
    }
    assert_eq!(h.get_by_label("Starter 1").rect().center(), from, "nothing above moves when the drop zone appears");
    let to = h.get_by_label("Ungrouped").rect().center();
    assert!(to.y > h.get_by_label("Starter 37").rect().bottom(), "drop zone sits after the folders");
    for i in 1..=6 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (i as f32 / 6.)));
        h.run_steps(1);
    }
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(3);
    assert_eq!(user_book(&h).swatches.iter().find(|w| w.name == "Starter 1").unwrap().group, None);
}

#[test]
fn dsfix_multiple_palette_drops_keep_earlier_error() {
    let mut h = Harness::builder().with_size(egui::vec2(520., 760.)).build_ui_state(|ui, app| panel(app, ui), app());
    h.run();
    let at = h.get_by_label("Starter 1").rect().center();
    h.hover_at(at);
    h.run();
    let bad: egui::DroppedFileHandle = std::sync::Arc::new(MemoryDrop { path: "Broken.gpl".into(), bytes: b"bad palette".to_vec() });
    let good: egui::DroppedFileHandle =
        std::sync::Arc::new(MemoryDrop { path: "Valid.gpl".into(), bytes: b"GIMP Palette\nName: Valid\n1 2 3 Good Ink\n".to_vec() });
    h.input_mut().dropped_files.extend([bad, good]);
    h.run_steps(1);
    assert!(h.state().ui.status_error, "successful second palette must not erase the earlier failure");
    let state: State = h.ctx.data(|d| d.get_temp(state_id()).unwrap());
    assert!(state.error.is_some());
    assert!(h.state().session.prefs().user_swatches.palette.swatches.iter().any(|s| s.name == "Good Ink"));
}
