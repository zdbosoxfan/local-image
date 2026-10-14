//! Compatibility facade: colour policy comes from the current module's tokens.
#[cfg(test)]
pub use li_ui_kit::color_icons::raster;
#[allow(unused_imports)] // Compatibility exports are used by different shells and tests.
pub use li_ui_kit::color_icons::{enabled, exists, raster_tinted, set_enabled, tint};
pub fn image(ui: &egui::Ui, name: &str, size: f32, disabled: bool) -> Option<egui::Image<'static>> {
    li_ui_kit::color_icons::image_with(ui, name, size, disabled, crate::theme::Tokens::get(ui.ctx()).icon, ui.visuals().dark_mode)
}
#[cfg(test)]
use egui::{Color32, Context};
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_svgs_parse_render_and_disable_at_toolbar_sizes() {
        for (name, _) in crate::color_icon_data::TOOL_FAMILIES {
            assert!(std::str::from_utf8(crate::mono_tool_icons::svg(name).unwrap()).unwrap().contains("viewBox=\"0 0 24 24\""), "{name}");
            for size in [20, 24, 48] {
                let image = raster(name, size, false).unwrap();
                assert_eq!(image.size, [size as usize; 2]);
                // 1/20 of the box, as the vector table's check: a 2 px line icon such as Line (a single
                // diagonal) or More (three dots) covers ~6%, the retired filled glyphs 1/12 or more.
                assert!(image.pixels.iter().filter(|p| p.a() > 100).count() > (size * size / 20) as usize, "empty icon: {name}/{size}");
                let disabled = raster(name, size, true).unwrap();
                assert!(disabled.pixels.iter().all(|p| p.r() == p.g() && p.g() == p.b() && p.a() <= 115), "{name}");
            }
        }
    }

    #[test]
    fn family_tint_is_a_subtle_hue_on_the_theme_icon_colour() {
        let icon = Color32::from_gray(222);
        // Neutral families keep the theme colour; coloured families move toward their hue.
        assert_eq!(tint("hand", icon, true), icon);
        let brush = tint("brush", icon, true);
        assert_ne!(brush, icon);
        assert!(brush.r() > brush.b(), "paint tools lean orange: {brush:?}");
        // Light themes use the deeper shade so the tint keeps its contrast on a pale background.
        let light = tint("brush", Color32::from_gray(52), false);
        assert!(light.r() > light.b() && u16::from(light.r()) + u16::from(light.g()) + u16::from(light.b()) < 3 * 160, "{light:?}");
        for (name, _) in crate::color_icon_data::TOOL_FAMILIES {
            assert!(crate::mono_tool_icons::exists(name), "family row without an icon: {name}");
        }
    }

    #[test]
    fn texture_cache_tracks_name_size_scale_and_disabled() {
        let ctx = Context::default();
        let run = |name, size, disabled| {
            let mut id = None;
            let mut output = ctx.run_ui(Default::default(), |ui| {
                if let egui::ImageSource::Texture(texture) = image(ui, name, size, disabled).unwrap().source(ui.ctx()) {
                    id = Some(texture.id);
                }
            });
            // This test inspects texture identities without a renderer.
            output.textures_delta.clear();
            id.unwrap()
        };
        let a = run("brush", 24.0, false);
        assert_eq!(a, run("brush", 24.0, false));
        assert_ne!(a, run("brush", 20.0, false));
        assert_ne!(a, run("crop", 24.0, false));
        assert_ne!(a, run("brush", 24.0, true));
        ctx.set_pixels_per_point(2.0);
        assert_ne!(a, run("brush", 24.0, false));
    }
}
