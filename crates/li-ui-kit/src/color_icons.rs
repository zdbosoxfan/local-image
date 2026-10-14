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
