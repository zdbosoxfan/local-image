//! Monochrome tools retain their original two-pixel line geometry.
use egui::{Color32, Image};
pub fn svg(name: &str) -> Option<&'static [u8]> {
    crate::icons::tool_svg(name)
}
pub fn exists(name: &str) -> bool {
    svg(name).is_some()
}
pub fn image(name: &str, size: f32, tint: Color32) -> Option<Image<'static>> {
    Some(crate::icons::source_image(svg(name)?, crate::icons::StrokeStyle::Tool, size, tint))
}
pub fn raster(name: &str, pixels: u32) -> Result<egui::ColorImage, String> {
    crate::icons::raster(svg(name).ok_or_else(|| format!("unknown monochrome tool: {name}"))?, crate::icons::StrokeStyle::Tool, pixels, Color32::WHITE, false)
}
