//! `egui_kittest` renderer on the CPU [`softpaint`](crate::softpaint) rasteriser.
//!
//! Drop-in for `egui_kittest::wgpu::WgpuTestRenderer` on UI that draws with meshes (text, widgets,
//! textured images): `Harness::builder().renderer(SoftpaintRenderer::new())`. It mirrors the wgpu
//! test renderer's frame: the content rect at the context's pixels-per-point, cleared to
//! transparent, premultiplied 8-bit gamma pixels. Paint callbacks (custom wgpu canvases) are not
//! drawn; tests of those stay on the GPU.

use egui::{Color32, TexturesDelta};
use egui_kittest::TestRenderer;

use crate::softpaint::{self, TextureStore};

/// Deterministic CPU renderer for `egui_kittest` harnesses: same pixels on every machine.
pub struct SoftpaintRenderer {
    textures: TextureStore,
    clear: Clear,
}

#[derive(Clone, Copy)]
enum Clear {
    Fixed(Color32),
    /// Read per frame from the context, e.g. the theme's canvas or chrome colour.
    FromContext(fn(&egui::Context) -> Color32),
}

impl Default for SoftpaintRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftpaintRenderer {
    /// Cleared to transparent, like the wgpu test renderer.
    pub fn new() -> Self {
        Self::with_clear(Color32::TRANSPARENT)
    }

    /// Clear each frame to `clear` instead (an opaque colour gives an opaque screenshot).
    pub fn with_clear(clear: Color32) -> Self {
        SoftpaintRenderer { textures: TextureStore::default(), clear: Clear::Fixed(clear) }
    }

    /// Clear each frame to a colour read from the context at render time (e.g.
    /// `|ctx| Tokens::get(ctx).canvas`), so theme switches inside a test are followed.
    pub fn with_clear_from(clear: fn(&egui::Context) -> Color32) -> Self {
        SoftpaintRenderer { textures: TextureStore::default(), clear: Clear::FromContext(clear) }
    }
}

impl TestRenderer for SoftpaintRenderer {
    fn handle_delta(&mut self, delta: &mut TexturesDelta) {
        self.textures.apply(std::mem::take(delta));
    }

    fn render(&mut self, ctx: &egui::Context, output: &egui::FullOutput) -> Result<image::RgbaImage, String> {
        // The scale the frame's shapes were laid out at.
        let ppp = output.pixels_per_point;
        let size = ctx.content_rect().size() * ppp;
        let (w, h) = (size.x.round().max(0.0) as u32, size.y.round().max(0.0) as u32);
        let clear = match self.clear {
            Clear::Fixed(c) => c,
            Clear::FromContext(f) => f(ctx),
        };
        let prims = ctx.tessellate(output.shapes.clone(), ppp);
        let img = softpaint::paint(&prims, &self.textures, [w as usize, h as usize], ppp, clear);
        let bytes = img.pixels.iter().flat_map(|p| p.to_array()).collect();
        image::RgbaImage::from_raw(w, h, bytes).ok_or_else(|| "softpaint: image size mismatch".to_owned())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use egui_kittest::Harness;

    fn ui(ui: &mut egui::Ui) {
        ui.heading("Local Image");
        ui.label("Deterministic CPU screenshot");
        let _ = ui.button("Apply");
        let mut v = 0.4_f32;
        ui.add(egui::Slider::new(&mut v, 0.0..=1.0));
    }

    fn shot(ppp: f32) -> image::RgbaImage {
        let mut h = Harness::builder().with_size(egui::vec2(220.0, 140.0)).with_pixels_per_point(ppp).renderer(SoftpaintRenderer::new()).build_ui(ui);
        h.run();
        h.render().unwrap()
    }

    /// The kittest adapter adds nothing of its own: for the same frame it returns exactly the
    /// bytes `softpaint::paint` produces (content rect × ppp, transparent clear, premultiplied).
    #[test]
    fn testkit_kittest_renderer_matches_softpaint() {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.5);
        let input = || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(220.0, 140.0))), ..Default::default() };
        let mut r = SoftpaintRenderer::new();
        let mut store = TextureStore::default();
        let mut out = ctx.run_ui(input(), ui);
        for _ in 0..2 {
            store.apply(out.textures_delta.clone());
            r.handle_delta(&mut out.textures_delta);
            out = ctx.run_ui(input(), ui);
        }
        store.apply(out.textures_delta.clone());
        r.handle_delta(&mut out.textures_delta);
        let via_kittest = r.render(&ctx, &out).unwrap();
        let ppp = ctx.pixels_per_point();
        let direct = softpaint::paint(&ctx.tessellate(out.shapes.clone(), ppp), &store, [330, 210], ppp, Color32::TRANSPARENT);
        assert_eq!((via_kittest.width(), via_kittest.height()), (330, 210));
        let direct_bytes: Vec<u8> = direct.pixels.iter().flat_map(|p| p.to_array()).collect();
        assert_eq!(via_kittest.as_raw(), &direct_bytes);
        assert!(direct.pixels.iter().filter(|p| p.a() != 0).count() > 500, "frame must not be empty");
    }

    /// A real harness renders on the CPU, deterministically, with no GPU or window.
    #[test]
    fn testkit_kittest_harness_renders_deterministically() {
        let a = shot(1.0);
        assert_eq!((a.width(), a.height()), (220, 140));
        let b = shot(1.0);
        assert_eq!(a.as_raw(), b.as_raw(), "two runs must give identical pixels");
        assert!(a.pixels().filter(|p| p.0[3] != 0).count() > 500, "frame must not be empty");
    }

    /// HiDPI: the image is the content rect in physical pixels.
    #[test]
    fn testkit_kittest_renderer_scales_with_pixels_per_point() {
        let a = shot(2.0);
        assert_eq!((a.width(), a.height()), (440, 280));
    }
}
