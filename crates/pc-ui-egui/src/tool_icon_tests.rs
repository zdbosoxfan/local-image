use std::collections::HashSet;

use egui::{Color32, Rect, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;

use crate::{PhotocraftApp, color_icon_data, color_icons, icons, state::Tool};

#[allow(dead_code)] // Reuse Develop's CPU painter for these windowless pixel checks.
#[path = "../../lc-ui-egui/src/softpaint.rs"]
pub(super) mod cpu;

#[test]
fn every_tool_and_group_member_has_a_distinct_colour_icon_and_none_are_unused() {
    let mut used = HashSet::new();
    for t in Tool::ALL {
        let name = icons::tool_icon_name(t);
        assert!(color_icons::exists(name), "{t:?}: {name}");
        assert!(used.insert(name), "distinct tools share {name}");
    }
    use photocraft_algo::liquify::LiquifyTool as L;
    let mut liquify = L::ALL.to_vec();
    liquify.extend([L::ReconstructAll, L::FreezeAll, L::ThawAll, L::InvertFreeze]);
    for tool in liquify {
        let name = crate::liquify_ui::tool_icon_name(tool);
        assert!(color_icons::exists(name), "{tool:?}");
        assert!(used.insert(name), "Liquify shares {name}");
    }
    used.extend(["edit-toolbar", "quick-mask"]);
    used.extend(color_icon_data::DEVELOP_TOOL_ICONS.iter().map(|(_, name)| *name));
    for (name, _) in color_icon_data::COLOR_ICONS {
        assert!(used.contains(name), "unused colour icon: {name}");
    }
    let disk: HashSet<_> = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons/color"))
        .unwrap()
        .map(|e| e.unwrap().path().file_stem().unwrap().to_str().unwrap().to_owned())
        .collect();
    let embedded: HashSet<_> = color_icon_data::COLOR_ICONS.iter().map(|(n, _)| n.to_string()).collect();
    assert_eq!(disk, embedded, "every SVG must be embedded");
}

#[test]
fn formerly_shared_glyphs_have_distinct_rendered_silhouettes() {
    for (a, b) in [("blur", "smooth"), ("smudge", "forward-warp"), ("healing", "spot-healing"), ("linear-mask", "luminance-range")] {
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
fn liquify_tool_strip_and_global_action_icons_are_clickable_in_both_modes() {
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
