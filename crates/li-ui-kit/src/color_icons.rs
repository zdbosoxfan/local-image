//! Colour tool-icon mode shared by Compositing and Develop: the same monochrome line icon, tinted
//! by its family (color_icon_data.rs) as a subtle hue on the theme's icon colour. Rasterized with
//! usvg/resvg at the physical pixel size; textures belong to the current egui context.

use egui::{Color32, ColorImage, Context, Image, Vec2};

/// How far the family hue pulls the theme icon colour: enough to colour-code families, little
/// enough that the toolbar still reads as one monochrome set.
const TINT: f32 = 0.7;

pub fn exists(name: &str) -> bool {
    crate::mono_tool_icons::exists(name) && crate::color_icon_data::family(name).is_some()
}

pub fn set_enabled(ctx: &Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new((env!("CARGO_PKG_NAME"), "colour-tool-icons")), enabled));
}

/// Monochrome is the default (owner decision 2026-10-13); preferences switch colour on.
pub fn enabled(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp(egui::Id::new((env!("CARGO_PKG_NAME"), "colour-tool-icons")))).unwrap_or(false)
}

/// The colour-mode tint for `name`: the theme `icon` colour pulled toward the family hue. Neutral
/// families keep the icon colour.
pub fn tint(name: &str, icon: Color32, dark: bool) -> Color32 {
    let Some((on_dark, on_light)) = crate::color_icon_data::family(name).and_then(crate::color_icon_data::family_hue) else {
        return icon;
    };
    let hue = if dark { on_dark } else { on_light };
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * TINT).round() as u8;
    Color32::from_rgb(mix(icon.r(), hue.r()), mix(icon.g(), hue.g()), mix(icon.b(), hue.b()))
}

/// Render the line icon in `colour`. Disabled icons are greyed and faded, never tinted.
pub fn raster_tinted(name: &str, pixels: u32, colour: Color32, disabled: bool) -> Result<ColorImage, String> {
    crate::icons::raster(
        crate::mono_tool_icons::svg(name).ok_or_else(|| format!("unknown tool icon: {name}"))?,
        crate::icons::StrokeStyle::Tool,
        pixels,
        colour,
        disabled,
    )
}

/// The colour-mode raster on a dark theme with a light-grey icon colour (checks and review sheets).
pub fn raster(name: &str, pixels: u32, disabled: bool) -> Result<ColorImage, String> {
    raster_tinted(name, pixels, tint(name, Color32::from_gray(222), true), disabled)
}

/// Colour rendering uses the same bounded per-context texture backend as every other icon.
pub fn image_with(ui: &egui::Ui, name: &str, size: f32, disabled: bool, icon: Color32, dark: bool) -> Option<Image<'static>> {
    if !exists(name) {
        return None;
    }
    let size = size.clamp(1.0, 512.0);
    let pixels = (size * ui.ctx().pixels_per_point()).round().clamp(1.0, 2048.0) as u32;
    let colour = tint(name, icon, dark);
    let id = crate::icons::texture(ui.ctx(), crate::mono_tool_icons::svg(name)?, crate::icons::StrokeStyle::Tool, pixels, colour, disabled)?;
    Some(Image::new((id, Vec2::splat(size))).fit_to_exact_size(Vec2::splat(size)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color_icon_data::TOOL_FAMILIES;
    use crate::tool_icon_data::TOOL_ICONS;
    use egui::Context;

    #[test]
    fn all_svgs_parse_render_and_disable_at_toolbar_sizes() {
        for (name, _) in TOOL_FAMILIES {
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
        for (name, _) in TOOL_FAMILIES {
            assert!(crate::mono_tool_icons::exists(name), "family row without an icon: {name}");
        }
    }

    #[test]
    fn texture_cache_tracks_name_size_scale_and_disabled() {
        let ctx = Context::default();
        let run = |name, size, disabled| {
            let mut id = None;
            let mut output = ctx.run_ui(Default::default(), |ui| {
                if let egui::ImageSource::Texture(texture) =
                    image_with(ui, name, size, disabled, crate::theme::Tokens::get(ui.ctx()).icon, ui.visuals().dark_mode).unwrap().source(ui.ctx())
                {
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

    #[test]
    fn every_tool_style_has_the_same_rendered_silhouette_at_actual_sizes() {
        for (name, _) in TOOL_ICONS {
            for size in [16, 20, 40] {
                let mono = crate::mono_tool_icons::raster(name, size).unwrap();
                let colour = raster(name, size, false).unwrap();
                assert_eq!(mono.size, colour.size, "{name}");
                // RGB is the only difference: antialiased alpha, holes and optical padding agree.
                assert!(mono.pixels.iter().zip(&colour.pixels).all(|(a, b)| a.a().abs_diff(b.a()) <= 1), "style geometry differs: {name}/{size}");
                // Line icons: 1/20 of the box (see the colour check above); the filled Glossy glyphs used 1/12.
                assert!(mono.pixels.iter().filter(|p| p.a() > 100).count() > (size * size / 20) as usize, "empty monochrome tool: {name}/{size}");
                // Embedded SVG images need an explicit white base before egui multiplies the tint.
                assert!(mono.pixels.iter().all(|p| p.r() == p.a() && p.g() == p.a() && p.b() == p.a()), "mono tint base is not white: {name}");
            }
        }
    }
}
