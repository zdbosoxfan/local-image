#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{ColorMode, Document, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
use std::sync::Arc;
struct Host {
    session: Session,
    proxy: State,
    draw: crate::clip_compound::State,
    flyout: FlyoutState,
    ids: Vec<LayerId>,
    error: Option<String>,
    preset: Preset,
}
fn host() -> Host {
    let mut session = Session::new();
    let mut doc = Document::new("Proxy fixture", Size::new(80, 80), ColorMode::Rgb, SampleType::U8);
    let mut shape = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(10., 10.), (60., 10.), (60., 60.), (10., 60.)])]),
        fill: Some(Fill::Solid(Color::rgb(1., 0., 0.))),
        stroke: Some(ShapeStroke { paint: Fill::Solid(Color::rgb(0., 0., 1.)), width: 4., ..Default::default() }),
        ..Default::default()
    };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
    let layer = Layer::new("Shape", LayerContent::Shape(shape));
    let id = layer.id;
    doc.layers.push(layer);
    session.add_document(doc, None);
    session.select_layer(id).unwrap();
    Host { session, proxy: State::default(), draw: Default::default(), flyout: Default::default(), ids: vec![id], error: None, preset: Preset::Illustrator }
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(800., 360.)).with_step_dt(1. / 60.).build_ui_state(
        |ui, h| {
            h.error = keys(ui.ctx(), &mut h.proxy, &mut h.session, &h.ids, true)
                .and_then(|_| widget(ui, &mut h.proxy, &mut h.session, &h.ids, &mut h.draw))
                .err()
                .map(|e| e.to_string());
            let registered = ["Pen", "Curvature", "Scale", "ShapeBuilder"];
            if let Some(tool) = tool_key(ui.ctx(), h.preset, &registered, &h.flyout.active) {
                h.flyout.active = tool;
            }
            flyouts(ui, &mut h.flyout, &registered, None);
        },
        host(),
    )
}
fn appearance(h: &Host) -> Appearance {
    Gesture::begin(&h.session, &h.ids).unwrap().request().edits[0].appearance.clone()
}
#[test]
fn toolbar46_proxy_clicks_keys_one_undo_and_defaults() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness();
    h.run_steps(3);
    let original = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    h.get_by_label("Active stroke").click();
    h.run_steps(3);
    assert_eq!(h.state().proxy.active, ActivePaint::Stroke);
    h.key_press(Key::X);
    h.run_steps(2);
    assert_eq!(h.state().proxy.active, ActivePaint::Fill);
    assert!(Arc::ptr_eq(&original, &h.state().session.active().unwrap().doc));
    h.key_press_modifiers(Modifiers::SHIFT, Key::X);
    h.run_steps(3);
    let a = appearance(h.state());
    assert_eq!(current(&a, ActivePaint::Fill), Some(Fill::Solid(Color::rgb(0., 0., 1.))));
    assert_eq!(current(&a, ActivePaint::Stroke), Some(Fill::Solid(Color::rgb(1., 0., 0.))));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    assert!(h.state_mut().session.undo());
    assert!(Arc::ptr_eq(&original, &h.state().session.active().unwrap().doc));
    h.get_by_label("Gradient").click();
    h.run_steps(3);
    assert!(matches!(current(&appearance(h.state()), ActivePaint::Fill), Some(Fill::Gradient { .. })));
    h.key_press(Key::Slash);
    h.run_steps(3);
    assert_eq!(current(&appearance(h.state()), ActivePaint::Fill), None);
    h.key_press(Key::D);
    h.run_steps(3);
    let a = appearance(h.state());
    assert_eq!(current(&a, ActivePaint::Fill), Some(Fill::Solid(Color::WHITE)));
    assert_eq!(current(&a, ActivePaint::Stroke), Some(Fill::Solid(Color::BLACK)));
    h.get_by_label("Colour").click();
    h.run_steps(3);
    let field = h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Paint colour hex");
    field.focus();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Paint colour hex").type_text("00ff00");
    h.run_steps(2);
    h.get_by_label("Apply paint colour").click();
    h.run_steps(3);
    assert_eq!(current(&appearance(h.state()), ActivePaint::Fill), Some(Fill::Solid(Color::rgb(0., 1., 0.))));
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
}
#[test]
fn toolbar46_narrow_toolbar_controls_popups_and_captured_release() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = Harness::builder().with_size(egui::vec2(800., 500.)).with_step_dt(1. / 60.).build_ui_state(
        |ui, h: &mut Host| {
            ui.set_width(46.);
            h.error = widget(ui, &mut h.proxy, &mut h.session, &h.ids, &mut h.draw).err().map(|e| e.to_string());
            flyouts(ui, &mut h.flyout, &["Pen", "Curvature"], None);
            assert!(ui.min_rect().width() <= 46.1, "toolbar expanded: {:?}", ui.min_rect());
        },
        host(),
    );
    h.run_steps(3);
    let history = h.state().session.active().unwrap().history.past_len();
    h.get_by_label("Swap fill and stroke").click();
    h.run_steps(3);
    assert_eq!(current(&appearance(h.state()), ActivePaint::Fill), Some(Fill::Solid(Color::rgb(0., 0., 1.))));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    h.get_by_label("Default fill and stroke").click();
    h.run_steps(3);
    assert_eq!(current(&appearance(h.state()), ActivePaint::Stroke), Some(Fill::Solid(Color::BLACK)));
    h.get_by_label("Gradient").click();
    h.run_steps(3);
    assert!(matches!(current(&appearance(h.state()), ActivePaint::Fill), Some(Fill::Gradient { .. })));
    h.get_by_label("None").click();
    h.run_steps(3);
    assert_eq!(current(&appearance(h.state()), ActivePaint::Fill), None);
    h.get_by_label("Colour").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Paint colour hex").focus();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Paint colour hex").type_text("00ff00");
    h.run_steps(2);
    h.get_by_label("Apply paint colour").click();
    h.run_steps(3);
    assert_eq!(current(&appearance(h.state()), ActivePaint::Fill), Some(Fill::Solid(Color::rgb(0., 1., 0.))));
    h.get_by_label("Draw Normal").click();
    h.run_steps(3);
    h.get_by_label("Draw Behind").click();
    h.run_steps(3);
    assert_eq!(h.state().draw.settings.mode, photocraft_engine::vector_packages::clip_compound::DrawMode::Behind);
    let p = h.get_by_label("Pen").rect().center();
    h.event(egui::Event::PointerMoved(p));
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(26);
    h.get_by_label("Curvature");
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.get_by_label("Curvature").click();
    h.run_steps(3);
    assert_eq!(h.state().flyout.active, "Curvature");
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    // Pointer release after a selection change must not capture the new targets.
    let p = h.get_by_label("Swap fill and stroke").rect().center();
    h.event(egui::Event::PointerMoved(p));
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    let original = h.state().session.active().unwrap().doc.clone();
    h.state_mut().ids.clear();
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    assert!(Arc::ptr_eq(&original, &h.state().session.active().unwrap().doc));
}
#[test]
fn toolbar46_pointer_hold_detached_preview_and_single_release() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness();
    h.run_steps(3);
    let original = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    let p = h.get_by_label("Swap fill and stroke").rect().center();
    h.event(egui::Event::PointerMoved(p));
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(4);
    assert!(Arc::ptr_eq(&original, &h.state().session.active().unwrap().doc));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    let layer = h.state().proxy.preview.as_ref().unwrap().layer(h.state().ids[0]).unwrap();
    assert_eq!(current(layer.vector.appearance.as_ref().unwrap(), ActivePaint::Fill), Some(Fill::Solid(Color::rgb(0., 0., 1.))));
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    assert!(h.state().proxy.preview.is_none());
    assert_eq!(current(&appearance(h.state()), ActivePaint::Fill), Some(Fill::Solid(Color::rgb(0., 0., 1.))));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    assert!(h.state().error.is_none());
}
#[test]
fn toolbar46_detached_proxy_rejects_stale_selection_locked_or_nonfinite_set() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = host();
    let doc = h.session.active().unwrap().doc.clone();
    let history = h.session.active().unwrap().history.past_len();
    h.proxy.begin(&h.session, &h.ids).unwrap();
    h.proxy.update(&h.session, &Action::Swap).unwrap();
    assert!(h.proxy.preview.is_some());
    assert!(Arc::ptr_eq(&doc, &h.session.active().unwrap().doc));
    assert_eq!(history, h.session.active().unwrap().history.past_len());
    assert!(h.proxy.release(&mut h.session, &[]).is_err());
    assert!(Arc::ptr_eq(&doc, &h.session.active().unwrap().doc));
    h.proxy.begin(&h.session, &h.ids).unwrap();
    h.session.execute("layer.setProps", serde_json::json!({"name":"Changed"})).unwrap();
    let doc = h.session.active().unwrap().doc.clone();
    assert!(h.proxy.update(&h.session, &Action::Default).is_err());
    assert!(Arc::ptr_eq(&doc, &h.session.active().unwrap().doc));
    let mut c = Color::BLACK;
    c.alpha = f32::NAN;
    assert!(h.proxy.apply(&mut h.session, &h.ids, &Action::Colour(c)).is_err());
    assert!(Arc::ptr_eq(&doc, &h.session.active().unwrap().doc));
    h.session.execute("layer.setProps", serde_json::json!({"locks":{"all":true}})).unwrap();
    let doc = h.session.active().unwrap().doc.clone();
    assert!(h.proxy.apply(&mut h.session, &h.ids, &Action::Swap).is_err());
    assert!(Arc::ptr_eq(&doc, &h.session.active().unwrap().doc));
}
#[test]
fn toolbar46_hold_flyout_curvature_and_preset_keyboard_switch() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness();
    h.run_steps(3);
    let p = h.get_by_label("Pen").rect().center();
    h.event(egui::Event::PointerMoved(p));
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(26);
    h.get_by_label("Curvature");
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.get_by_label("Curvature").click();
    h.run_steps(3);
    assert_eq!(h.state().flyout.active, "Curvature");
    h.state_mut().flyout.active = "Pen".into();
    h.key_press_modifiers(Modifiers::SHIFT, Key::Backtick);
    h.run_steps(3);
    assert_eq!(h.state().flyout.active, "Curvature");
    h.key_press(Key::S);
    h.run_steps(2);
    assert_eq!(h.state().flyout.active, "Scale");
    h.state_mut().preset = Preset::Affinity;
    h.key_press(Key::S);
    h.run_steps(2);
    assert_eq!(h.state().flyout.active, "ShapeBuilder");
}
#[test]
fn toolbar46_registered_icon_alpha_and_styles_at_16_20_40() {
    let _gpu_gate = crate::gpu_test_support::lock();
    for tool in photocraft_engine::toolsets::VECTOR_TOOLS {
        assert!(icons::ICONS.iter().any(|(id, _, _, _)| *id == tool.id), "missing {}", tool.id);
    }
    for (_, name, colour, mono) in icons::ICONS {
        for size in [16, 20, 40] {
            let svg = String::from_utf8_lossy(mono).replace("currentColor", "#ffffff");
            let c = egui_extras::image::load_svg_bytes_with_size(colour, egui::load::SizeHint::Width(size), &Default::default()).unwrap();
            let m = egui_extras::image::load_svg_bytes_with_size(svg.as_bytes(), egui::load::SizeHint::Width(size), &Default::default()).unwrap();
            assert_eq!(c.size, [size as usize; 2]);
            assert!(c.pixels.iter().filter(|p| p.a() > 100).count() > (size * size / 20) as usize, "empty {name}/{size}");
            assert!(c.pixels.iter().zip(&m.pixels).all(|(a, b)| a.a().abs_diff(b.a()) <= 1), "alpha {name}/{size}");
            assert!(c.pixels.first().unwrap().a() == 0 && c.pixels.last().unwrap().a() == 0, "padding {name}/{size}");
        }
    }
}
#[test]
fn toolbar46_contact_sheet_original_artwork() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // Actual resvg 16/20/40px output, both backgrounds/styles; no GPU or desktop window.
    let mut sheet = image::RgbaImage::from_pixel(1080, icons::ICONS.len() as u32 * 68, image::Rgba([31, 32, 35, 255]));
    for (row, (tool, _, colour, mono)) in icons::ICONS.iter().enumerate() {
        label(&mut sheet, 4, row as u32 * 68 + 26, tool, 1);
        for (i, (size, bg, bytes)) in
            [16, 20, 40].into_iter().flat_map(|size| [(size, 30, *colour), (size, 240, *colour), (size, 30, *mono), (size, 240, *mono)]).enumerate()
        {
            let svg = String::from_utf8_lossy(bytes).replace("currentColor", if bg < 128 { "#dedede" } else { "#343434" });
            let im = egui_extras::image::load_svg_bytes_with_size(svg.as_bytes(), egui::load::SizeHint::Width(size), &Default::default()).unwrap();
            let raw: Vec<_> = im.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
            let icon = image::RgbaImage::from_raw(size, size, raw).unwrap();
            let mut tile = image::RgbaImage::from_pixel(60, 60, image::Rgba([bg, bg, bg, 255]));
            image::imageops::overlay(&mut tile, &icon, i64::from((60 - size) / 2), i64::from((60 - size) / 2));
            image::imageops::overlay(&mut sheet, &tile, (i * 64 + 240) as i64, (row * 68 + 4) as i64);
        }
    }
    sheet.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/vector-tool-icons.png")).unwrap();
}

#[test]
#[ignore = "00 production toolbar/menu wiring and real RTX 5090 GPU milestone; owner speed rules"]
fn toolbar46_gpu_full_app_proxy_keys_icon_upload_and_pixel_parity() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use eframe::{egui_wgpu, wgpu};
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    create.native_adapter_selector = Some(Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") {
                    i.backend == wgpu::Backend::Metal && i.device_type != wgpu::DeviceType::Cpu
                } else {
                    i.name.contains("RTX 5090") && i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu
                }
            })
            .cloned()
            .ok_or_else(|| "Requires RTX 5090 / Vulkan / discrete".into())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(create);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(adapter));
    });
    for scale in [1., 2.] {
        let mut h =
            Harness::builder().with_size(egui::vec2(1280., 800.)).with_pixels_per_point(scale).with_max_steps(64).wgpu_setup(setup.clone()).build_eframe(
                |cc| {
                    crate::PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                    let mut s = Session::new();
                    s.execute("file.new", serde_json::json!({"width":128,"height":96})).unwrap();
                    let mut app = crate::PhotocraftApp::new(s, Default::default());
                    app.set_wgpu(cc.wgpu_render_state.as_ref().unwrap().clone());
                    app
                },
            );
        h.run_steps(6);
        assert!(h.state().gpu_active());
        #[cfg(not(target_os = "macos"))]
        assert!(h.state().perf.gpu_info.adapter.contains("RTX 5090"));
        // This gate intentionally fails on a widget-only branch: 00 must wire the real shell.
        h.get_by_label("Vector persona").click();
        h.run_steps(3);
        h.get_by_label("Active fill");
        for (_, name, _, _) in icons::ICONS {
            assert!(crate::color_icons::exists(name) && crate::mono_tool_icons::exists(name), "00 must register {name} in both runtime loaders");
        }
        h.state_mut()
            .run("shape.create", serde_json::json!({"kind":"rect","rect":[12,12,40,32],"fill":"#ff0000","stroke":{"color":"#0000ff","width":3.}}))
            .unwrap();
        h.run_steps(3);
        let before = h.state().session.active().unwrap().doc.clone();
        let history = h.state().session.active().unwrap().history.past_len();
        let canvas_before = h.render().unwrap();
        let p = h.get_by_label("Swap fill and stroke").rect().center();
        h.event(egui::Event::PointerMoved(p));
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.run_steps(4);
        assert!(Arc::ptr_eq(&before, &h.state().session.active().unwrap().doc));
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
        let canvas_preview = h.render().unwrap();
        let (_, _, preview_texels) = h.state().read_canvas_texels(before.id.0).unwrap();
        let canvas = h.state().last_canvas_rect;
        let pigment_counts = |image: &image::RgbaImage| {
            let mut counts = [0usize; 2];
            for y in canvas.min.y.ceil() as u32..canvas.max.y.floor() as u32 {
                for x in canvas.min.x.ceil() as u32..canvas.max.x.floor() as u32 {
                    let p = image.get_pixel((x as f32 * scale) as u32, (y as f32 * scale) as u32).0;
                    if p[0] > 200 && p[1] < 80 && p[2] < 80 {
                        counts[0] += 1;
                    }
                    if p[2] > 200 && p[1] < 80 && p[0] < 80 {
                        counts[1] += 1;
                    }
                }
            }
            counts
        };
        let original_pigments = pigment_counts(&canvas_before);
        let preview_pigments = pigment_counts(&canvas_preview);
        assert!(original_pigments[0] > 100 && preview_pigments[1] > 100);
        assert!(
            preview_pigments[0] < original_pigments[0] && preview_pigments[1] > original_pigments[1],
            "red fill must become blue through 00's detached canvas provider"
        );
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(3);
        assert_eq!(
            h.state().session.active().unwrap().history.past_len(),
            history + 1,
            "proxy status={} gesture preview={} scale={scale}",
            h.state().ui.status,
            h.state().ui.fill_stroke_proxy.preview.is_some()
        );
        h.render().unwrap();
        let (_, _, committed_texels) = h.state().read_canvas_texels(before.id.0).unwrap();
        // Release changes selection feedback. Compare every actual canvas texel rather
        // than counting colours in the screenshot's selection controls.
        assert_eq!(preview_texels.len(), committed_texels.len());
        let max = preview_texels.iter().zip(&committed_texels).flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs())).fold(0f32, f32::max);
        assert!(max <= 1. / 255., "proxy preview/release premultiplied canvas diff {max} at scale {scale}");
        assert!(h.state_mut().session.undo());
        assert!(Arc::ptr_eq(&before, &h.state().session.active().unwrap().doc));
        h.key_press_modifiers(Modifiers::SHIFT, Key::Backtick);
        h.run_steps(3);
        assert_eq!(h.state().session.prefs().toolbar.vector_tool, "Curvature");
        // Independent translation oracle for the actual Vector Ctrl+D dispatch.
        let ids = h.state().session.active().unwrap().selected_layers();
        let capture = photocraft_engine::vector_packages::transform_tools::Capture::begin(&h.state().session, &ids).unwrap();
        h.state_mut().run("vector.transform.apply", serde_json::json!({"capture":capture,"operation":{"kind":"affine","matrix":[1.,0.,0.,1.,5.,0.]}})).unwrap();
        let first_x = |app: &crate::PhotocraftApp| {
            let layer = app.session.active().unwrap().doc.layer(ids[0]).unwrap();
            let LayerContent::Shape(shape) = &layer.content else { panic!("shape target") };
            shape.path.subpaths[0].knots[0].anchor.x
        };
        let translated = first_x(h.state());
        let history = h.state().session.active().unwrap().history.past_len();
        h.key_press_modifiers(Modifiers::COMMAND, Key::D);
        h.run_steps(3);
        assert!((first_x(h.state()) - translated - 5.).abs() < 1e-9);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        // Pixel persona still deselects, while Vector Ctrl+D resolves Transform Again.
        let vector = crate::shortcut_dispatch::bindings(h.state());
        let chord = crate::shortcuts::parse("Cmd+D").unwrap();
        assert!(vector.iter().any(|(id, sc)| id == "vector.transform.again" && *sc == chord));
        h.get_by_label("Pixel persona").click();
        h.run_steps(3);
        let pixels = h.render().unwrap();
        h.key_press(Key::X);
        h.run_steps(3);
        let after = h.render().unwrap();
        // Paint defaults/swaps must never damage the Pixel canvas. Crop away changing chrome.
        let rect = h.state().last_canvas_rect;
        for y in rect.min.y.ceil() as u32..rect.max.y.floor() as u32 {
            for x in rect.min.x.ceil() as u32..rect.max.x.floor() as u32 {
                let a = pixels.get_pixel((x as f32 * scale) as u32, (y as f32 * scale) as u32);
                let b = after.get_pixel((x as f32 * scale) as u32, (y as f32 * scale) as u32);
                assert!(a.0.iter().zip(b.0).all(|(a, b)| a.abs_diff(b) <= 1));
            }
        }
        h.state_mut().run("select.rect", serde_json::json!({"x":1,"y":1,"width":10,"height":10})).unwrap();
        h.key_press_modifiers(Modifiers::COMMAND, Key::D);
        h.run_steps(3);
        assert!(h.state().session.active().unwrap().doc.selection.is_none(), "Pixel Ctrl+D must still deselect");
        let pixel = crate::shortcut_dispatch::bindings(h.state());
        assert!(pixel.iter().any(|(id, sc)| id == "select.deselect" && *sc == chord));
    }
}

#[test]
fn toolbar46_frozen_pixel_menu_keys_and_all_chords_parse() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let expected: serde_json::Value = serde_json::from_str(include_str!("pixel-menu-keys.json")).unwrap();
    let actual: Vec<_> = crate::menu_catalog::CATALOG.iter().filter_map(|(_, _, sc, id)| Some((*id, (*sc)?))).collect();
    assert_eq!(serde_json::json!(actual), expected);
    for binding in layout::ILLUSTRATOR.iter().chain(layout::AFFINITY).chain(layout::ACCELERATORS).chain(layout::PROXY_KEYS) {
        assert!(shortcut(binding.key).is_some(), "unparseable {}", binding.key);
    }
}

fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_lowercase() {
        'a' => [14, 17, 17, 31, 17, 17, 17],
        'b' => [30, 17, 17, 30, 17, 17, 30],
        'c' => [14, 17, 16, 16, 16, 17, 14],
        'd' => [30, 17, 17, 17, 17, 17, 30],
        'e' => [31, 16, 16, 30, 16, 16, 31],
        'f' => [31, 16, 16, 30, 16, 16, 16],
        'g' => [14, 17, 16, 23, 17, 17, 14],
        'h' => [17, 17, 17, 31, 17, 17, 17],
        'i' => [14, 4, 4, 4, 4, 4, 14],
        'j' => [7, 2, 2, 2, 2, 18, 12],
        'k' => [17, 18, 20, 24, 20, 18, 17],
        'l' => [16, 16, 16, 16, 16, 16, 31],
        'm' => [17, 27, 21, 21, 17, 17, 17],
        'n' => [17, 25, 21, 19, 17, 17, 17],
        'o' => [14, 17, 17, 17, 17, 17, 14],
        'p' => [30, 17, 17, 30, 16, 16, 16],
        'q' => [14, 17, 17, 17, 21, 18, 13],
        'r' => [30, 17, 17, 30, 20, 18, 17],
        's' => [15, 16, 16, 14, 1, 1, 30],
        't' => [31, 4, 4, 4, 4, 4, 4],
        'u' => [17, 17, 17, 17, 17, 17, 14],
        'v' => [17, 17, 17, 17, 17, 10, 4],
        'w' => [17, 17, 17, 21, 21, 21, 10],
        'x' => [17, 17, 10, 4, 10, 17, 17],
        'y' => [17, 17, 10, 4, 4, 4, 4],
        'z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        _ => [0; 7],
    }
}

fn label(sheet: &mut image::RgbaImage, x: u32, y: u32, text: &str, scale: u32) {
    for (i, ch) in text.chars().enumerate() {
        for (row, bits) in glyph(ch).iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            sheet.put_pixel(x + (i as u32 * 6 + col) * scale + dx, y + row as u32 * scale + dy, image::Rgba([218, 222, 231, 255]));
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "RTX 5090 icon upload milestone; owner speed rules prohibit GPU runs"]
fn toolbar46_gpu_upload_every_vector_icon_and_switch_styles() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use eframe::{egui_wgpu, wgpu};
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    create.native_adapter_selector = Some(Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") {
                    i.backend == wgpu::Backend::Metal && i.device_type != wgpu::DeviceType::Cpu
                } else {
                    i.name.contains("RTX 5090") && i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu
                }
            })
            .cloned()
            .ok_or_else(|| "Requires RTX 5090 / Vulkan / discrete".into())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(create);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |a| {
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(a));
    });
    for scale in [1., 2.] {
        let mut h = Harness::builder().with_size(egui::vec2(800., 600.)).with_pixels_per_point(scale).wgpu_setup(setup.clone()).build_ui_state(
            |ui, (colour, clicked)| {
                crate::color_icons::set_enabled(ui.ctx(), *colour);
                if ui.button("Switch vector icon style").clicked() {
                    *colour = !*colour;
                }
                egui::Grid::new("toolbar46-icon-gpu").show(ui, |ui| {
                    for (i, (tool, _, _, _)) in icons::ICONS.iter().enumerate() {
                        if tool_button(ui, tool, 40.).clicked() {
                            *clicked = true;
                        }
                        if i % 12 == 11 {
                            ui.end_row();
                        }
                    }
                });
            },
            (true, false),
        );
        h.run_steps(4);
        h.get_by_label("Curvature").click();
        h.run_steps(3);
        assert!(h.state().1);
        let colour = h.render().unwrap();
        let saturation = |im: &image::RgbaImage| im.pixels().filter(|p| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]) > 40).count();
        let count = saturation(&colour);
        assert!(count > (1000. * scale * scale) as usize);
        h.get_by_label("Switch vector icon style").click();
        h.run_steps(4);
        let mono = h.render().unwrap();
        assert!(saturation(&mono) < count / 8, "Missing monochrome upload");
    }
}

#[test]
fn toolbar46_multiple_targets_swap_individually_and_locked_tail_is_atomic() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = host();
    h.session.execute("shape.create", serde_json::json!({"kind":"rect","rect":[15,15,10,10],"fill":"#00ff00"})).unwrap();
    let second = h.session.active().unwrap().active_layer.unwrap();
    h.ids.push(second);
    let before = h.session.active().unwrap().doc.clone();
    let history = h.session.active().unwrap().history.past_len();
    h.proxy.apply(&mut h.session, &h.ids, &Action::Swap).unwrap();
    let result = Gesture::begin(&h.session, &h.ids).unwrap();
    assert_eq!(current(&result.request().edits[0].appearance, ActivePaint::Fill), Some(Fill::Solid(Color::rgb(0., 0., 1.))));
    assert_eq!(current(&result.request().edits[1].appearance, ActivePaint::Fill), None);
    assert_eq!(current(&result.request().edits[1].appearance, ActivePaint::Stroke), Some(Fill::Solid(Color::rgb(0., 1., 0.))));
    assert_eq!(h.session.active().unwrap().history.past_len(), history + 1);
    assert!(h.session.undo());
    assert!(Arc::ptr_eq(&before, &h.session.active().unwrap().doc));
    h.session.execute("layer.setProps", serde_json::json!({"layer":second.0,"locked":true})).unwrap();
    let before = h.session.active().unwrap().doc.clone();
    let history = h.session.active().unwrap().history.past_len();
    assert!(h.proxy.apply(&mut h.session, &h.ids, &Action::Default).is_err());
    assert!(Arc::ptr_eq(&before, &h.session.active().unwrap().doc));
    assert_eq!(h.session.active().unwrap().history.past_len(), history);
}

#[test]
fn toolbar46_swap_preserves_hidden_paints_and_stroke_metadata() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut a = Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(1., 0., 0.)))),
            AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke { paint: Fill::Solid(Color::rgb(0., 0., 1.)), width: 7., ..Default::default() },
                start_arrow: Some(photocraft_doc::vector_design::appearance::Arrowhead::Triangle),
                ..Default::default()
            }),
            AppearanceItem::Fill(FillItem {
                paint: Fill::Solid(Color::rgb(0., 1., 0.)),
                opacity: 0.4,
                blend: photocraft_doc::BlendMode::Multiply,
                visible: false,
            }),
        ],
    };
    let hidden = a.items[2].clone();
    edit(&mut a, ActivePaint::Fill, &Action::Swap);
    assert_eq!(a.items[2], hidden);
    assert_eq!(current(&a, ActivePaint::Fill), Some(Fill::Solid(Color::rgb(0., 0., 1.))));
    let AppearanceItem::Stroke(s) = &a.items[1] else { panic!("stroke retained") };
    assert_eq!(s.base.width, 7.);
    assert_eq!(s.start_arrow, Some(photocraft_doc::vector_design::appearance::Arrowhead::Triangle));
}

#[test]
fn review_additional_proxy_edits_only_selected_gradient_stop() {
    let mut h = host();
    h.proxy.apply(&mut h.session, &h.ids, &Action::Gradient).unwrap();
    let before = appearance(&h);
    h.proxy.selected_stop = Some((0, 1));
    h.proxy.apply(&mut h.session, &h.ids, &Action::Colour(Color::rgb(0., 1., 0.))).unwrap();
    let mut expected = before.clone();
    if let AppearanceItem::Fill(fill) = &mut expected.items[0]
        && let Fill::Gradient { stops, .. } = &mut fill.paint
    {
        stops[1].1 = Color::rgb(0., 1., 0.);
    }
    assert_eq!(appearance(&h), expected);
    assert!(h.session.undo());
    assert_eq!(appearance(&h), before);
}
