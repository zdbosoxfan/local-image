//! Shared bounds-limited paint projection; pc-vector owns all path rasterization.
//! Original Local Image code, GPL-3.0-or-later. Exports/display/Expand use the same PaintPaths.
use super::*;
use photocraft_color::{BlendMode, PixelFormat, blend::composite};
use photocraft_doc::Pattern;
use photocraft_geom::{Rect as PixelRect, TILE_SIZE};
use photocraft_vector::appearance::{CompiledAppearance, StrokeProjection};
pub const MAX_CACHE_BYTES: u128 = 512 << 20;
pub struct Compiled<'a> {
    plans: Vec<(CompiledAppearance<'a>, BlendMode)>,
    bounds: PixelRect,
}
impl<'a> Compiled<'a> {
    pub fn new(scene: &[PaintPath], patterns: &'a [Pattern], canvas: PixelRect, projection: &dyn StrokeProjection) -> Result<Self> {
        let mut plans = Vec::new();
        let mut bounds = PixelRect::EMPTY;
        for p in scene {
            let mut item = p.item.clone();
            let blend = match &mut item {
                AppearanceItem::Fill(f) => {
                    let b = f.blend;
                    if b != BlendMode::Dissolve {
                        f.blend = BlendMode::Normal;
                    }
                    b
                }
                AppearanceItem::Stroke(s) => {
                    let b = s.blend;
                    if b != BlendMode::Dissolve {
                        s.blend = BlendMode::Normal;
                    }
                    b
                }
            };
            let shape = ShapeLayer { path: p.path.clone(), ..Default::default() };
            let a = Appearance { items: vec![item] };
            let plan = CompiledAppearance::with_projection(&shape, Some(&a), patterns, canvas, projection).map_err(Error::Geometry)?;
            let b = plan.bounds();
            if !b.is_empty() {
                bounds = if bounds.is_empty() { b } else { bounds.union(&b) };
            }
            plans.push((plan, if blend == BlendMode::Dissolve { BlendMode::Normal } else { blend }));
        }
        Ok(Self { plans, bounds })
    }
    /// Conservative bytes for detached ordinary child caches, prepared before allocation.
    pub fn expanded_cache_bytes(&self, format: PixelFormat) -> u128 {
        self.plans.iter().map(|(plan, _)| plan.cache_bytes(format)).sum()
    }
    pub fn bounds(&self) -> PixelRect {
        self.bounds
    }
    pub fn cache_bytes(&self, format: PixelFormat) -> u128 {
        if self.bounds.is_empty() {
            return 0;
        }
        let b = self.bounds;
        let nx = (b.x1 - 1).div_euclid(TILE_SIZE) - b.x0.div_euclid(TILE_SIZE) + 1;
        let ny = (b.y1 - 1).div_euclid(TILE_SIZE) - b.y0.div_euclid(TILE_SIZE) + 1;
        nx as u128 * ny as u128 * TILE_SIZE as u128 * TILE_SIZE as u128 * format.bytes_per_pixel() as u128
    }
    pub fn render_rgba(&self, r: PixelRect) -> Result<Vec<[f32; 4]>> {
        if r.size().area() > 1_048_576 {
            return Err(Error::Budget);
        }
        let mut pixels = vec![[0.; 4]; r.size().area() as usize];
        for (plan, blend) in &self.plans {
            if plan.bounds().intersect(&r).is_empty() {
                continue;
            }
            let rgba = plan.render_rgba(r).map_err(Error::Geometry)?;
            for (dest, src) in pixels.iter_mut().zip(rgba) {
                *dest = composite(*blend, *dest, src, 1.);
            }
        }
        Ok(pixels)
    }
}
