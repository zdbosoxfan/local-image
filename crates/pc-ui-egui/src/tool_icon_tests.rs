use std::collections::HashSet;

use egui::{Color32, Rect, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;

use crate::{PhotocraftApp, color_icon_data, color_icons, icons, state::Tool};

#[allow(dead_code)] // Reuse Develop's CPU painter for these windowless pixel checks.
#[rustfmt::skip] // The source is formatted with Develop's own rustfmt configuration.
#[path = "../../lc-ui-egui/src/softpaint.rs"]
pub(super) mod cpu;

#[test]
fn every_tool_and_group_member_has_a_distinct_colour_icon_and_none_are_unused() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut used = HashSet::new();
    let mut distinct = HashSet::new();
    for t in Tool::ALL {
        let name = icons::tool_icon_name(t);
        assert!(color_icons::exists(name), "{t:?}: {name}");
        assert!(crate::mono_tool_icons::exists(name), "{t:?}: monochrome {name}");
        if photocraft_engine::toolsets::ALL_TOOLS.contains(&format!("{t:?}").as_str()) {
            assert!(distinct.insert(name), "distinct legacy Pixel tools share {name}");
            used.insert(name);
        } else {
            used.insert(name);
        }
    }
    use photocraft_algo::liquify::LiquifyTool as L;
    let mut liquify = L::ALL.to_vec();
    liquify.extend([L::ReconstructAll, L::FreezeAll, L::ThawAll, L::InvertFreeze]);
    for tool in liquify {
        let name = crate::liquify_ui::tool_icon_name(tool);
        assert!(color_icons::exists(name), "{tool:?}");
        assert!(distinct.insert(name), "Liquify shares {name}");
        used.insert(name);
    }
    used.extend(["edit-toolbar", "quick-mask"]);
    used.extend(crate::vector_tool_icon_data::ICONS.iter().map(|(_, name, _, _)| *name));
    used.extend(color_icon_data::DEVELOP_TOOL_ICONS.iter().map(|(_, name)| *name));
    for (name, _) in color_icon_data::COLOR_ICONS {
        assert!(used.contains(name), "unused colour icon: {name}");
    }
    let disk: HashSet<_> = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons/color"))
        .unwrap()
        .map(|e| e.unwrap().path().file_stem().unwrap().to_str().unwrap().to_owned())
        .collect();
    let embedded: HashSet<_> = color_icon_data::COLOR_ICONS
        .iter()
        .map(|(n, _)| n.to_string())
        .chain(crate::vector_tool_icon_data::ICONS.iter().map(|(_, n, _, _)| n.to_string()))
        .collect();
    assert_eq!(disk, embedded, "every SVG must be embedded");
    let mono_disk: HashSet<_> = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons/tools"))
        .unwrap()
        .map(|e| e.unwrap().path().file_stem().unwrap().to_str().unwrap().to_owned())
        .collect();
    let mono_embedded: HashSet<_> = crate::tool_icon_data::TOOL_ICONS
        .iter()
        .map(|(n, _)| n.to_string())
        .chain(crate::vector_tool_icon_data::ICONS.iter().map(|(_, n, _, _)| n.to_string()))
        .collect();
    assert_eq!(mono_disk, embedded, "every colour icon has matching monochrome artwork");
    assert_eq!(mono_disk, mono_embedded, "every monochrome SVG is embedded");
}

#[test]
fn formerly_shared_glyphs_have_distinct_rendered_silhouettes() {
    let _gpu_gate = crate::gpu_test_support::lock();
    for (a, b) in [
        ("blur", "smooth"),
        ("smudge", "forward-warp"),
        ("healing", "spot-healing"),
        ("linear-mask", "luminance-range"),
        ("dodge", "zoom-in"),
        ("arrow-left-right", "move"),
        ("mouse-pointer-2", "direct-select"),
    ] {
        for size in [20, 24, 48] {
            let a_image = color_icons::raster(a, size, false).unwrap();
            let b_image = color_icons::raster(b, size, false).unwrap();
            // Alpha compares the artwork's silhouette, independent of family colour.
            let different = a_image.pixels.iter().zip(&b_image.pixels).filter(|(a, b)| a.a().abs_diff(b.a()) > 50).count();
            assert!(different > (size * size / 8) as usize, "{a}/{b} look alike at {size}px: {different} silhouette pixels differ");
        }
    }
}

#[test]
fn dodge_tool_icon_has_a_solid_paddle_head_in_both_styles() {
    let _gpu_gate = crate::gpu_test_support::lock();
    for size in [16, 20, 40] {
        let center = ((1.5 + 8.0 * 0.875) * f64::from(size) / 24.0).floor() as usize;
        for image in [color_icons::raster("dodge", size, false).unwrap(), crate::mono_tool_icons::raster("dodge", size).unwrap()] {
            for y in center - 1..=center + 1 {
                for x in center - 1..=center + 1 {
                    assert_eq!(image.pixels[y * size as usize + x].a(), 255, "Dodge must have a filled paddle head at {size}px");
                }
            }
        }
    }
}

#[test]
fn liquify_tool_strip_and_global_action_icons_are_clickable_in_both_modes() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use photocraft_algo::liquify::LiquifyTool as L;
    for mode in ["colour", "monochrome"] {
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 120, "height": 80})).unwrap();
        session.execute("layer.new.layer", json!({})).unwrap();
        session.prefs.edit(|p| {
            p.interface.language = "en".into();
            p.interface.tool_icons = mode.into();
        });
        let mut h = Harness::builder().with_size(vec2(1280.0, 900.0)).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut app = PhotocraftApp::new(session, Default::default());
            app.background_jobs = false;
            crate::liquify_ui::open(&mut app, &cc.egui_ctx).unwrap();
            app
        });
        h.run_steps(4);
        for tool in [L::Smooth, L::ForwardWarp, L::Freeze, L::Thaw] {
            let label = tool.label();
            h.get_by_role_and_label(egui::accesskit::Role::Button, label).click();
            h.run_steps(3);
            assert_eq!(h.state().distort.liquify.as_ref().unwrap().opts.tool, tool, "{mode}: {label}");
        }
        for (label, tool) in [("Mask All", L::FreezeAll), ("Invert All", L::InvertFreeze), ("None", L::ThawAll), ("Reconstruct", L::ReconstructAll)] {
            // Click the new 20 px image immediately before the existing text button.
            // Reconstruct also names a strip tool; the wider text button is the global action.
            let action = h.get_all_by_label(label).max_by(|a, b| a.rect().width().total_cmp(&b.rect().width())).unwrap();
            let at = action.rect().left_center() - vec2(18.0, 0.0);
            h.hover_at(at);
            h.step();
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
                h.step();
            }
            h.run_steps(3);
            assert_eq!(h.state().distort.liquify.as_ref().unwrap().strokes.last().unwrap().tool, tool, "{mode}: {label} icon");
        }
    }
}

fn toolbar_pixels(app: &mut PhotocraftApp, ppp: f32) -> egui::ColorImage {
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::Studio);
    ctx.set_pixels_per_point(ppp);
    let mut textures = cpu::TextureStore::default();
    let mut image = None;
    for i in 0..4 {
        let mut input =
            egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(320.0, 1000.0))), time: Some(i as f64 / 60.0), ..Default::default() };
        input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
        let mut output = ctx.run_ui(input, |ui| {
            crate::prefs_ui::tick(app, ui.ctx());
            crate::panels::toolbar(app, ui);
        });
        textures.apply(std::mem::take(&mut output.textures_delta));
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        image = Some(cpu::paint(&primitives, &textures, [(320.0 * ppp) as usize, (1000.0 * ppp) as usize], ppp, Color32::from_gray(31)));
    }
    image.unwrap()
}

fn saturated_toolbar_pixels(image: &egui::ColorImage, ppp: f32) -> usize {
    // Exclude the foreground/background swatches and decorative bottom wash.
    let w = image.size[0];
    (0..(700.0 * ppp) as usize)
        .flat_map(|y| (0..(46.0 * ppp) as usize).map(move |x| image.pixels[y * w + x]))
        .filter(|p| p.r().max(p.g()).max(p.b()) - p.r().min(p.g()).min(p.b()) > 65)
        .count()
}

#[test]
fn clicking_preferences_switches_the_real_toolbar_pixels_and_saves() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.session.prefs.edit(|p| {
        p.interface.theme = photocraft_engine::prefs::Theme::Studio;
        p.interface.language = "en".into();
    });
    let mut h = Harness::builder().with_size(vec2(1280.0, 1000.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, crate::theme::ThemeKind::Studio);
        app
    });
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Brush Tool").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, Tool::Brush);
    let colour = toolbar_pixels(h.state_mut(), 1.0);
    let colour_count = saturated_toolbar_pixels(&colour, 1.0);
    assert!(colour_count > 300, "colour tools must have saturated pixels: {colour_count}");
    let ctx = h.ctx.clone();
    let dialog = crate::menus::invoke(h.state_mut(), &ctx, "edit.preferences.interface", json!({})).unwrap()["dialog"].as_u64().unwrap();
    h.state_mut().ui.dialog_mut(dialog).unwrap().fields.insert("section".into(), json!("interface"));
    h.run_steps(3);
    h.get_by_value("Colour (default)").click();
    h.run_steps(2);
    h.get_by_label("Monochrome").click();
    h.run_steps(2);
    h.get_by_label("Apply").click();
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().interface.tool_icons, "monochrome");
    let mono = toolbar_pixels(h.state_mut(), 1.0);
    let mono_count = saturated_toolbar_pixels(&mono, 1.0);
    assert!(mono_count < colour_count / 5, "monochrome {mono_count}, colour {colour_count}");
    let mut restored = photocraft_engine::Session::new();
    restored.load_prefs_json(&h.state().session.prefs_to_json()).unwrap();
    assert_eq!(restored.prefs().interface.tool_icons, "monochrome");
    h.get_by_value("Monochrome").click();
    h.run_steps(2);
    h.get_by_label("Colour (default)").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().interface.tool_icons, "colour");
    let colour_2x = toolbar_pixels(h.state_mut(), 2.0);
    assert!(saturated_toolbar_pixels(&colour_2x, 2.0) > colour_count * 2);
}

/// Exercise SVG loading, texture upload, theme tint and real tool-button clicks with egui-wgpu.
/// Explicitly requires Vulkan hardware: software rendering and adapter skips cannot pass it.
#[test]
#[cfg(not(target_arch = "wasm32"))]
#[ignore = "requires a real Vulkan GPU; run explicitly with LOCAL_IMAGE_TEST_ADAPTER"]
fn gpu_tool_icons_upload_and_switch_styles_on_hardware() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use eframe::{egui_wgpu, wgpu};
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let requested = std::env::var("LOCAL_IMAGE_TEST_ADAPTER").unwrap_or_default().to_lowercase();
        let adapter = adapters
            .iter()
            .find(|a| {
                let info = a.get_info();
                info.backend == wgpu::Backend::Vulkan && info.device_type == wgpu::DeviceType::DiscreteGpu && info.name.to_lowercase().contains(&requested)
            })
            .ok_or_else(|| format!("required discrete Vulkan adapter is unavailable: {requested}"))?;
        eprintln!("tool icon GPU test: {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    for scale in [1.0, 2.0] {
        let mut h = Harness::builder()
            .with_size(vec2(600.0, 480.0))
            .with_pixels_per_point(scale)
            .wgpu_setup(egui_wgpu::WgpuSetup::CreateNew(setup.clone()))
            .build_ui_state(
                |ui, (colour, clicked)| {
                    egui_extras::install_image_loaders(ui.ctx());
                    color_icons::set_enabled(ui.ctx(), *colour);
                    if ui.button("Switch tool style").clicked() {
                        *colour = !*colour;
                    }
                    // A 32 px button paints a 20 px glyph: 20/40 physical px at these scales.
                    egui::Grid::new("all-tool-artwork").show(ui, |ui| {
                        for (i, (name, _)) in color_icon_data::COLOR_ICONS.iter().enumerate() {
                            if icons::tool_button(ui, name, 32.0, false, name).clicked() {
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
        h.get_by_role_and_label(egui::accesskit::Role::Button, "brush").click();
        h.run_steps(2);
        assert!(h.state().1, "GPU-backed tool button remains clickable");
        let colour = h.render().unwrap();
        let saturation = |img: &image::RgbaImage| img.pixels().filter(|p| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]) > 40).count();
        let colourful = saturation(&colour);
        assert!(colourful > (2000.0 * scale * scale) as usize, "missing GPU colour textures: {colourful}");
        h.get_by_label("Switch tool style").click();
        h.run_steps(4);
        let mono = h.render().unwrap();
        assert!(!h.state().0);
        assert!(saturation(&mono) < colourful / 10, "monochrome textures must use theme tint");

        // Also exercise the actual application toolbar. Timings include the whole headless
        // app frame, GPU submission and readback, not just standalone SVG equivalence.
        let mut session = photocraft_engine::Session::new();
        session.prefs.edit(|p| {
            p.interface.language = "en".into();
            p.interface.theme = photocraft_engine::prefs::Theme::Studio;
        });
        let mut app = Harness::builder()
            .with_size(vec2(1280.0, 1000.0))
            .with_pixels_per_point(scale)
            .wgpu_setup(egui_wgpu::WgpuSetup::CreateNew(setup.clone()))
            .build_eframe(move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, crate::theme::ThemeKind::Studio);
                let mut app = PhotocraftApp::new(session, Default::default());
                app.background_jobs = false;
                app
            });
        app.run_steps(3);
        app.get_by_role_and_label(egui::accesskit::Role::Button, "Brush Tool").click();
        app.run_steps(2);
        assert_eq!(app.state().ui.tool, Tool::Brush);
        for mode in ["colour", "monochrome"] {
            app.state_mut().session.prefs.edit(|p| p.interface.tool_icons = mode.into());
            let start = std::time::Instant::now();
            app.step();
            app.render().expect("real application GPU toolbar");
            let first_ms = start.elapsed().as_secs_f64() * 1000.0;
            let mut samples = Vec::new();
            for _ in 0..6 {
                let start = std::time::Instant::now();
                app.step();
                app.render().expect("warm application GPU toolbar");
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "real app tool icons: scale={scale}, style={mode}, first frame+readback={first_ms:.3} ms, warm median={:.3} ms, warm max={:.3} ms",
                (samples[2] + samples[3]) / 2.0,
                samples[5]
            );
        }
    }
}

#[test]
fn hardware_gpu_renders_compositing_and_develop_colour_icons_at_both_scales() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use eframe::wgpu;
    use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state};
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    for scale in [1.0, 2.0] {
        let rs = create_render_state(crate::gpu_canvas::wgpu_setup(), Default::default());
        let info = rs.adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            assert!(std::env::var_os("LOCAL_IMAGE_REQUIRE_GPU").is_none(), "hardware GPU required, got {}", info.name);
            eprintln!("skipped: hardware GPU unavailable ({})", info.name);
            return;
        }
        eprintln!("colour icons: {} ({:?}), scale {scale}", info.name, info.backend);
        let renderer = WgpuTestRenderer::from_render_state(rs.clone());
        let mut h = Harness::builder().with_size(vec2(800.0, 700.0)).with_pixels_per_point(scale).renderer(renderer).build_ui_state(
            |ui, rects: &mut Vec<(Rect, Rect)>| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::from_gray(31));
                rects.clear();
                // These are the same embedded SVGs, rasterizer and texture cache used by both
                // Compositing and Develop. Exercise every enabled and disabled texture upload.
                for row in color_icon_data::COLOR_ICONS.chunks(10) {
                    ui.horizontal(|ui| {
                        for (name, _) in row {
                            let enabled = ui.add(color_icons::image(ui, name, 20.0, false).unwrap()).rect;
                            let disabled = ui.add(color_icons::image(ui, name, 20.0, true).unwrap()).rect;
                            rects.push((enabled, disabled));
                        }
                    });
                }
            },
            Vec::new(),
        );
        h.run_steps(3);
        let pixels = h.render().unwrap();
        let counts = |rect: Rect| {
            let mut visible = 0;
            let mut saturated = 0;
            for y in (rect.top() * scale).ceil() as u32..(rect.bottom() * scale).floor() as u32 {
                for x in (rect.left() * scale).ceil() as u32..(rect.right() * scale).floor() as u32 {
                    let p = pixels.get_pixel(x, y).0;
                    visible += usize::from(p[0].max(p[1]).max(p[2]) > 45);
                    saturated += usize::from(p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]) > 40);
                }
            }
            (visible, saturated)
        };
        assert_eq!(h.state().len(), color_icon_data::COLOR_ICONS.len());
        for ((name, _), (enabled, disabled)) in color_icon_data::COLOR_ICONS.iter().zip(h.state()) {
            assert!(counts(*enabled).0 > 10, "{name} vanished on the GPU at scale {scale}");
            assert_eq!(counts(*disabled).1, 0, "{name} disabled texture retained colour");
        }
        assert!(h.state().iter().map(|(enabled, _)| counts(*enabled).1).sum::<usize>() > 1000);
        // Drop all renderer/texture owners before the final device/queue handles.
        drop(h);
        rs.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(30)) }).unwrap();
        drop(rs);
    }
}

#[test]
fn approved_tool_icons_are_restored_exactly_and_source_replacements_have_filled_faces() {
    let approved: serde_json::Value = serde_json::from_str(include_str!("../../../docs/design/tool-icons-pre-sourced.json")).unwrap();
    let approved_mono: serde_json::Value = serde_json::from_str(include_str!("../../../docs/design/tool-icons-approved-mono.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../assets/icons/source-manifest.json")).unwrap();
    let mut restored = 0;
    for tool in manifest["tools"].as_array().unwrap() {
        let name = tool["id"].as_str().unwrap();
        if tool["mode"] == "approved" {
            restored += 1;
            let colour = color_icon_data::COLOR_ICONS.iter().find(|(id, _)| *id == name).unwrap().1;
            let mono = crate::tool_icon_data::TOOL_ICONS.iter().find(|(id, _)| *id == name).unwrap().1;
            assert_eq!(colour, approved[name].as_str().unwrap().as_bytes(), "approved colour changed: {name}");
            assert_eq!(mono, approved_mono[name].as_str().unwrap().as_bytes(), "approved mono changed: {name}");
        }
    }
    assert_eq!(restored, 88);
    for size in [20, 40] {
        // The accepted gradient must retain an opaque face, rather than an empty outline.
        let gradient = color_icons::raster("blend", size, false).unwrap();
        let center = size as usize / 2;
        assert_eq!(gradient.pixels[center * size as usize + center].a(), 255);
        // Brush, eraser and marquee segments need substantial filled material at toolbar size.
        for name in ["brush", "eraser", "square-dashed", "circle-dashed", "crop"] {
            let image = color_icons::raster(name, size, false).unwrap();
            let material = image
                .pixels
                .iter()
                .filter(|pixel| {
                    let [r, g, b, a] = pixel.to_srgba_unmultiplied();
                    a > 200 && r.max(g).max(b) - r.min(g).min(b) > 30
                })
                .count();
            assert!(material > (size * size / 20) as usize, "replacement lost its filled family material: {name}/{size}");
        }
    }
}
