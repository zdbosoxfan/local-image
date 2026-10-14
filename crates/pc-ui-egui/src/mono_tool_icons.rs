//! Compatibility API for shared sourced tool icons.
#[cfg(test)]
use crate::tool_icon_data::TOOL_ICONS;
pub use li_ui_kit::mono_tool_icons::*;
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
                // Line icons: 1/20 of the box (see color_icons' check); the filled Glossy glyphs used 1/12.
                assert!(mono.pixels.iter().filter(|p| p.a() > 100).count() > (size * size / 20) as usize, "empty monochrome tool: {name}/{size}");
                // Embedded SVG images need an explicit white base before egui multiplies the tint.
                assert!(mono.pixels.iter().all(|p| p.r() == p.a() && p.g() == p.a() && p.b() == p.a()), "mono tint base is not white: {name}");
            }
        }
    }
}
