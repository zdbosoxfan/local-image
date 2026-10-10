//! Theme-tinted version of the same approved/mixed geometry used by colour tool icons.
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::tool_icon_data::TOOL_ICONS;
use egui::{Color32, Image, Vec2};

fn white_icons() -> &'static HashMap<&'static str, Arc<[u8]>> {
    static MAP: OnceLock<HashMap<&'static str, Arc<[u8]>>> = OnceLock::new();
    MAP.get_or_init(|| {
        TOOL_ICONS
            .iter()
            .map(|(name, bytes)| {
                let svg = String::from_utf8_lossy(bytes).replace("currentColor", "#ffffff");
                (*name, Arc::from(svg.into_bytes().into_boxed_slice()))
            })
            .collect()
    })
}

pub fn exists(name: &str) -> bool {
    white_icons().contains_key(name)
}

pub fn image(name: &str, size: f32, tint: Color32) -> Option<Image<'static>> {
    let bytes = white_icons().get(name)?.clone();
    Some(Image::from_bytes(format!("bytes://sourced-tool-icons/{name}.svg"), egui::load::Bytes::Shared(bytes)).fit_to_exact_size(Vec2::splat(size)).tint(tint))
}

#[cfg(test)]
pub fn raster(name: &str, pixels: u32) -> Result<egui::ColorImage, String> {
    let bytes = white_icons().get(name).ok_or_else(|| format!("unknown monochrome tool: {name}"))?;
    egui_extras::image::load_svg_bytes_with_size(bytes, egui::load::SizeHint::Width(pixels), &Default::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_style_has_the_same_rendered_silhouette_at_actual_sizes() {
        for (name, _) in TOOL_ICONS {
            for size in [16, 20, 40] {
                let mono = raster(name, size).unwrap();
                let colour = crate::color_icons::raster(name, size, false).unwrap();
                assert_eq!(mono.size, colour.size, "{name}");
                // RGB is the only difference: antialiased alpha, holes and optical padding agree.
                assert!(mono.pixels.iter().zip(&colour.pixels).all(|(a, b)| a.a().abs_diff(b.a()) <= 1), "style geometry differs: {name}/{size}");
                assert!(mono.pixels.iter().filter(|p| p.a() > 100).count() > (size * size / 12) as usize, "empty monochrome tool: {name}/{size}");
                // Embedded SVG images need an explicit white base before egui multiplies the tint.
                assert!(mono.pixels.iter().all(|p| p.r() == p.a() && p.g() == p.a() && p.b() == p.a()), "mono tint base is not white: {name}");
            }
        }
    }
}
