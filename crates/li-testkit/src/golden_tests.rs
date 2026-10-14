//! Golden pin for the moved rasteriser: a fixed scene exercising fractional edges, vertex-colour
//! gradients, translucent overlap, linear/nearest texture sampling, clip rects and HiDPI scale must
//! hash to the value `lc-ui-egui`'s `softpaint` produced before the move (J7 parity gate).
#![allow(clippy::unwrap_used)]
use std::sync::Arc;

use egui::epaint::{ClippedPrimitive, Mesh, Primitive, Vertex};
use egui::{Color32, ColorImage, Rect, TextureFilter, TextureId, pos2};

use crate::softpaint::{CpuTexture, TextureStore, paint};

/// FNV-1a 64 over the premultiplied RGBA bytes (stable across Rust versions, unlike `Hasher`).
fn fnv1a(img: &ColorImage) -> u64 {
    img.pixels.iter().flat_map(|p| p.to_array()).fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3))
}

fn scene() -> (Vec<ClippedPrimitive>, TextureStore) {
    let mut store = TextureStore::default();
    store.insert(TextureId::default(), CpuTexture::linear(Arc::new(ColorImage::new([1, 1], vec![Color32::WHITE]))));
    let checker: Vec<Color32> = (0..16).map(|i| if (i + i / 4) % 2 == 0 { Color32::from_rgb(230, 40, 20) } else { Color32::from_rgb(20, 60, 220) }).collect();
    store.insert(TextureId::User(1), CpuTexture::linear(Arc::new(ColorImage::new([4, 4], checker.clone()))));
    store.insert(
        TextureId::User(2),
        CpuTexture { image: Arc::new(ColorImage::new([4, 4], checker)), magnification: TextureFilter::Nearest, minification: TextureFilter::Linear },
    );

    let mut prims = Vec::new();
    // Gradient triangle with fractional vertices (anti-aliasing fringe-like edges).
    let mut m = Mesh::default();
    let v = |x: f32, y: f32, c: Color32| Vertex { pos: pos2(x, y), uv: pos2(0.0, 0.0), color: c };
    m.vertices = vec![v(1.3, 2.7, Color32::RED), v(37.6, 5.1, Color32::GREEN), v(12.2, 30.9, Color32::BLUE)];
    m.indices = vec![0, 1, 2];
    prims.push(ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(m) });
    // Translucent premultiplied quad overlapping it.
    let mut m = Mesh::default();
    m.add_colored_rect(Rect::from_min_max(pos2(8.25, 8.75), pos2(30.5, 22.25)), Color32::from_rgba_premultiplied(40, 80, 20, 140));
    prims.push(ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(m) });
    // Linear-magnified texture, clipped.
    let mut m = Mesh::with_texture(TextureId::User(1));
    m.add_rect_with_uv(Rect::from_min_max(pos2(2.0, 24.0), pos2(26.0, 40.0)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    prims.push(ClippedPrimitive { clip_rect: Rect::from_min_max(pos2(4.0, 25.0), pos2(22.0, 38.0)), primitive: Primitive::Mesh(m) });
    // Nearest-magnified, tinted texture.
    let mut m = Mesh::with_texture(TextureId::User(2));
    m.add_rect_with_uv(
        Rect::from_min_max(pos2(28.0, 24.0), pos2(40.0, 40.0)),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::from_rgba_premultiplied(200, 200, 200, 220),
    );
    prims.push(ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(m) });
    (prims, store)
}

#[test]
fn testkit_softpaint_golden_unchanged_by_move() {
    let (prims, store) = scene();
    let img = paint(&prims, &store, [64, 64], 1.5, Color32::from_rgb(18, 18, 18));
    let again = paint(&prims, &store, [64, 64], 1.5, Color32::from_rgb(18, 18, 18));
    assert_eq!(img.pixels, again.pixels);
    assert_eq!(fnv1a(&img), GOLDEN, "softpaint output changed: {:#018x}", fnv1a(&img));
}

const GOLDEN: u64 = 0x05cd_7856_8cde_9f88;
