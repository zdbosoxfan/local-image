//! Colour tool rendering shared by Compositing and Develop. egui_extras uses usvg/resvg;
//! SVG materials are preserved, and textures belong to the current egui context.

use std::collections::HashMap;

use egui::{Color32, ColorImage, Context, Image, TextureHandle, Vec2};

use crate::color_icon_data::COLOR_ICONS;

pub fn exists(name: &str) -> bool {
    COLOR_ICONS.iter().any(|(n, _)| *n == name)
}

pub fn set_enabled(ctx: &Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new((env!("CARGO_PKG_NAME"), "colour-tool-icons")), enabled));
}

pub fn enabled(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp(egui::Id::new((env!("CARGO_PKG_NAME"), "colour-tool-icons")))).unwrap_or(true)
}

/// Parse with usvg and render with resvg, at the physical pixel size. No SVG rewriting/tint.
pub fn raster(name: &str, pixels: u32, disabled: bool) -> Result<ColorImage, String> {
    let bytes = COLOR_ICONS.iter().find(|(n, _)| *n == name).ok_or_else(|| format!("unknown colour icon: {name}"))?.1;
    let mut image = egui_extras::image::load_svg_bytes_with_size(bytes, egui::load::SizeHint::Width(pixels), &Default::default())?;
    if disabled {
        for p in &mut image.pixels {
            let [r, g, b, a] = p.to_srgba_unmultiplied();
            let grey = ((u32::from(r) * 54 + u32::from(g) * 183 + u32::from(b) * 19) / 256) as u8;
            *p = Color32::from_rgba_unmultiplied(grey, grey, grey, (u16::from(a) * 45 / 100) as u8);
        }
    }
    Ok(image)
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Key {
    name: &'static str,
    size: u32,
    scale: u32,
    disabled: bool,
}

#[derive(Clone, Default)]
struct Cache(HashMap<Key, TextureHandle>);

pub fn image(ui: &egui::Ui, name: &str, size: f32, disabled: bool) -> Option<Image<'static>> {
    if !exists(name) {
        return None;
    }
    let name = COLOR_ICONS.iter().find(|(n, _)| *n == name)?.0;
    let scale = ui.ctx().pixels_per_point();
    let size = size.clamp(1.0, 512.0);
    let key = Key { name, size: size.to_bits(), scale: scale.to_bits(), disabled };
    let cache_id = egui::Id::new((env!("CARGO_PKG_NAME"), "colour-tool-textures"));
    let cached = ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<Cache>(cache_id).0.get(&key).cloned());
    let texture = cached.or_else(|| {
        let pixels = (size * scale).round().clamp(1.0, 2048.0) as u32;
        let pixels = raster(name, pixels, disabled).ok()?;
        let texture = ui.ctx().load_texture(format!("colour-tool/{name}/{:?}", key), pixels, egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|d| {
            let cache = d.get_temp_mut_or_default::<Cache>(cache_id);
            // Display-scale changes should not retain an unbounded history of textures.
            if cache.0.len() >= 512 {
                cache.0.clear();
            }
            cache.0.insert(key, texture.clone());
        });
        Some(texture)
    })?;
    Some(Image::new((texture.id(), Vec2::splat(size))).fit_to_exact_size(Vec2::splat(size)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_svgs_parse_render_and_disable_at_toolbar_sizes() {
        for (name, bytes) in COLOR_ICONS {
            assert!(std::str::from_utf8(bytes).unwrap().contains("viewBox=\"0 0 24 24\""), "{name}");
            for size in [20, 24, 48] {
                let image = raster(name, size, false).unwrap();
                assert_eq!(image.size, [size as usize; 2]);
                assert!(image.pixels.iter().filter(|p| p.a() > 100).count() > (size * size / 12) as usize, "empty icon: {name}/{size}");
                let disabled = raster(name, size, true).unwrap();
                assert!(disabled.pixels.iter().all(|p| p.r() == p.g() && p.g() == p.b() && p.a() <= 115), "{name}");
            }
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
