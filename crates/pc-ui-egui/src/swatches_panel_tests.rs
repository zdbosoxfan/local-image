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
    click(&mut h, "Add Current Colour");
    assert_eq!(h.state().session.prefs().user_swatches.palette.swatches.len(), n + 1);
    click(&mut h, "Colour");
    click(&mut h, "Swatch Options…");
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
    click(&mut h, "Swatches menu");
    click(&mut h, "Swatch Libraries ⏵");
    click(&mut h, "Open Color");
    assert!(h.state().session.prefs().user_swatches.palette.groups.iter().any(|g| g.name == "Open Color"));
    click(&mut h, "Swatches menu");
    click(&mut h, "List with names");
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
    click(&mut h, "New Folder");
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
    for label in ["Add Current Colour", "Search swatches", "New Folder", "User Swatches", "Document Swatches", "Starter 3"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    assert_eq!(h.state().session.prefs().user_swatches, before);
    h.state_mut().session.edit_prefs(|p| p.toolbar.persona = Persona::Pixel);
    h.run();
    click(&mut h, "Document Swatches");
    click(&mut h, "Add Current Colour");
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
        app.services.pick_open = Some(Box::new(move || Some((name.clone(), Ok(bytes.clone())))));
        let mut h = harness(app);
        h.run();
        click(&mut h, "Document Swatches");
        click(&mut h, "Swatches menu");
        click(&mut h, "Import Swatches…");
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
    click(&mut h, "Swatch Options…");
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
    click(&mut h, "Swatch Options…");
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
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| a.get_info().device_type == wgpu::DeviceType::DiscreteGpu && a.get_info().name.contains("5090"))
            .ok_or("RTX 5090 required")?;
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
    h.get_by_label("Add Current Colour").click();
    h.run();
    let _ = h.render().unwrap();
    let ms = start.elapsed().as_secs_f64() * 1000.;
    std::fs::write(output.join("runtime.txt"), format!("RTX 5090 / Vulkan\nWarm Add Current Colour click, real panel + preference update + GPU render/readback: {ms:.3} ms\nHeadless UI diagnostic, not a desktop frame-rate or compositor benchmark.\n")).unwrap();
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
    click(&mut h, "Apply to Selected Objects");
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
