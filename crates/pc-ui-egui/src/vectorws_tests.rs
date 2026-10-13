#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::{PhotocraftApp, theme::ThemeKind};
use egui_kittest::Harness;
use photocraft_doc::{ColorMode, Document, Layer, LayerContent, SampleType, ShapeLayer, Size};
use photocraft_engine::{Session, toolsets::Persona};
use serde_json::json;

fn app() -> PhotocraftApp {
    let mut session = Session::new();
    let mut doc = Document::new("Vector studio", Size::new(320, 240), ColorMode::Rgb, SampleType::U8);
    let mut shape = ShapeLayer {
        path: photocraft_vector::shapes::rect(30., 30., 170., 120.),
        fill: Some(photocraft_doc::Fill::Solid(photocraft_doc::Color::rgb(0.2, 0.5, 0.8))),
        ..Default::default()
    };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let layer = Layer::new("Badge", LayerContent::Shape(shape));
    let id = layer.id;
    doc.layers.push(layer);
    session.add_document(doc, None);
    session.select_layer(id).unwrap();
    session.edit_prefs(|p| p.toolbar.persona = Persona::Vector);
    let mut app = PhotocraftApp::new(session, Default::default());
    crate::workspace_ui::apply_vector(&mut app);
    app
}

#[test]
fn vectorws_unlock_show_shortcuts_profile_override_and_undo() {
    let mut app = app();
    let id = app.session.active().unwrap().active_layer.unwrap();
    app.session
        .edit("Lock and hide", |doc, _| {
            let l = doc.layer_mut(id).unwrap();
            l.locks.all = true;
            l.visible = false;
            Ok(())
        })
        .unwrap();
    let before = app.session.active().unwrap().doc.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1440., 900.)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, ThemeKind::ProMedium);
        app
    });
    h.run_steps(4);
    let keys = crate::shortcut_dispatch::binding_strings(h.state());
    assert!(keys.contains(&("vector.object.unlockAll".into(), "Cmd+Alt+Shift+2".into())));
    h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::ALT | egui::Modifiers::SHIFT, egui::Key::Num2);
    h.run_steps(3);
    assert!(!h.state().session.active().unwrap().doc.layer(id).unwrap().locks.all);
    h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::ALT | egui::Modifiers::SHIFT, egui::Key::Num3);
    h.run_steps(3);
    assert!(h.state().session.active().unwrap().doc.layer(id).unwrap().visible);
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(id));
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.state_mut().session.edit_prefs(|p| {
        p.shortcut_profiles.editor.duplicate("Vector custom").unwrap();
    });
    h.state_mut()
        .session
        .execute("prefs.set", json!({"path":"shortcutProfiles.editor.custom.Vector custom.overrides.vector.object.unlockAll","value":"Cmd+Shift+L"}))
        .unwrap();
    h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::L);
    h.run_steps(3);
    assert!(!h.state().session.active().unwrap().doc.layer(id).unwrap().locks.all);
    assert!(h.state_mut().session.undo());
    h.state_mut().session.edit_prefs(|p| {
        p.shortcut_profiles.editor.active = "Photoshop".into();
        p.toolbar.vector_keyboard.preset = photocraft_engine::vector_toolbar_layout::Preset::Affinity;
    });
    let restore = if cfg!(target_os = "macos") {
        // Synthetic input needs both the physical Mac key and egui's logical Command flag.
        egui::Modifiers::CTRL | egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND
    } else {
        egui::Modifiers::COMMAND | egui::Modifiers::ALT | egui::Modifiers::SHIFT
    };
    h.key_press_modifiers(restore, egui::Key::L);
    h.run_steps(3);
    h.key_press_modifiers(restore, egui::Key::H);
    h.run_steps(3);
    assert!(!h.state().session.active().unwrap().doc.layer(id).unwrap().locks.all);
    assert!(h.state().session.active().unwrap().doc.layer(id).unwrap().visible);
}

#[test]
fn vectorws_release_commands_visible_and_execute_existing_eligible_operations() {
    use photocraft_engine::vector_packages::clip_compound::{Action, Capture, prepare};
    let mut app = app();
    let mut second = Layer::new("Clip", LayerContent::Shape(ShapeLayer { path: photocraft_vector::shapes::rect(50., 50., 40., 30.), ..Default::default() }));
    if let LayerContent::Shape(shape) = &mut second.content {
        let d = &app.session.active().unwrap().doc;
        shape.cache = Some(photocraft_vector::render_shape(shape, d.pixel_format(), d.bounds()));
    }
    let second_id = second.id;
    app.session
        .edit("Add clip", move |doc, _| {
            doc.layers.push(second);
            Ok(())
        })
        .unwrap();
    let first = app.session.active().unwrap().active_layer.unwrap();
    app.session.active_mut().unwrap().selected_layers = vec![first, second_id];
    let ctx = egui::Context::default();
    crate::menus::invoke(&mut app, &ctx, "object.clippingMask.make", json!({})).unwrap();
    let capture = Capture::selected(&app.session).unwrap();
    assert!(prepare(&app.session, &capture, &Action::ReleaseClip).is_ok());
    let rows = crate::menus::menu_items(&app);
    assert!(rows.iter().any(|row| row.id == "object.clippingMask.release" && row.enabled && row.path == vec!["Object", "Clipping Mask"]));
    crate::menus::invoke(&mut app, &ctx, "object.clippingMask.release", json!({})).unwrap();
    let ids: Vec<_> = app.session.active().unwrap().doc.layers.iter().map(|layer| layer.id).collect();
    app.session.active_mut().unwrap().active_layer = ids.first().copied();
    app.session.active_mut().unwrap().selected_layers = ids;
    crate::menus::invoke(&mut app, &ctx, "object.compoundPath.make", json!({})).unwrap();
    let capture = Capture::selected(&app.session).unwrap();
    assert!(prepare(&app.session, &capture, &Action::ReleaseCompound).is_ok());
    let rows = crate::menus::menu_items(&app);
    assert!(rows.iter().any(|row| row.id == "object.compoundPath.release" && row.enabled && row.path == vec!["Object", "Compound Path"]));
    crate::menus::invoke(&mut app, &ctx, "object.compoundPath.release", json!({})).unwrap();
    assert_eq!(app.session.active().unwrap().doc.layers.len(), 2);
}

#[test]
#[ignore = "physical RTX 5090 Vulkan screenshots; run explicitly"]
fn vectorws_rtx5090_workspace_screens_and_interactive_frames() {
    use eframe::{
        egui_wgpu::{WgpuSetup, WgpuSetupCreateNew},
        wgpu,
    };
    use egui_kittest::kittest::Queryable;
    let _gate = crate::gpu_test_support::lock();
    let mut create = WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    create.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") {
                    i.backend == wgpu::Backend::Metal && i.device_type != wgpu::DeviceType::Cpu
                } else {
                    i.name.contains("RTX 5090") && i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu
                }
            })
            .ok_or("RTX 5090 Vulkan required")?;
        eprintln!("vectorws physical GPU: {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    let mut setup = WgpuSetup::CreateNew(create);
    crate::gpu_canvas::use_adapter_limits(&mut setup);
    let mut h = Harness::builder().with_size(egui::vec2(1440., 900.)).with_max_steps(64).wgpu_setup(setup).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, ThemeKind::ProMedium);
        let mut app = app();
        app.set_wgpu(cc.wgpu_render_state.as_ref().unwrap().clone());
        app
    });
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/vectorws-ux");
    std::fs::create_dir_all(&out).unwrap();
    h.run_steps(6);
    for (theme, name) in [(ThemeKind::ProMedium, "dark"), (ThemeKind::StudioLight, "light")] {
        h.state_mut().session.execute("prefs.set", json!({"path":"interface.theme","value":theme.id()})).unwrap();
        for (width, height) in [(1440., 900.), (1024., 700.)] {
            h.set_size(egui::vec2(width, height));
            for dock_width in [250., 400.] {
                crate::panels::request_dock_width(&h.ctx, dock_width);
                h.run_steps(4);
                assert!(crate::dock::last_strips(&h.ctx).iter().any(|s| s.tab_ids.contains(&crate::dock::Tab::Appearance)));
                assert!(!h.state().ui.panels.properties, "Vector selections must preserve the chosen studio instead of opening Pixel Properties");
                h.render().unwrap().save(out.join(format!("vector-{name}-{}x{}-dock{}.png", width as u32, height as u32, dock_width as u32))).unwrap();
            }
        }
    }
    h.set_size(egui::vec2(1440., 900.));
    h.run_steps(4);
    let t = std::time::Instant::now();
    h.get_by_label("Swatches panel tab").click();
    h.run_steps(3);
    let _ = h.render().unwrap();
    h.get_by_label("Color panel tab").click();
    h.run_steps(3);
    let _ = h.render().unwrap();
    eprintln!("vectorws real app: two panel clicks + six frames + two GPU readbacks {:.2} ms", t.elapsed().as_secs_f64() * 1000.);
    assert!(h.state().gpu.is_some());
}
