//! A stroked shape layer split into its fill and its vector stroke (rasterised over the canvas),
//! for shapes with clipped layers: Photoshop draws the stroke above the clipped layers. Cached
//! per shape state, so tiles (and the GPU's textures) reuse one rasterisation.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use photocraft_color::PixelFormat;
use photocraft_doc::vector::ShapeLayer;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

/// (fill, stroke), last use, and the shape's pixels the stroke was fitted to (kept alive so
/// their tile addresses, part of the key, can't be reused).
type Entry = ((Surface, Surface), u64, Option<Surface>);

fn cache() -> &'static Mutex<(HashMap<u64, Entry>, u64)> {
    static C: OnceLock<Mutex<(HashMap<u64, Entry>, u64)>> = OnceLock::new();
    C.get_or_init(|| Mutex::new((HashMap::new(), 0)))
}

/// Entries kept. Every plan rebuild looks up every stroked shape in turn, so a document with more
/// stroked shapes than this missed on all of them (LRU under a cyclic scan): the layout benchmark's
/// 33 stroked shapes re-rasterised over the whole canvas on each layer edit, ~100 ms a frame.
const CAPACITY: usize = 1024;
/// Pixel memory the cached parts may hold together (the most recent entry is always kept).
const BUDGET_BYTES: usize = 512 << 20;

fn entry_bytes(e: &Entry) -> usize {
    let tiles = |s: &Surface| s.tiles().map(|(_, t)| t.bytes().len()).sum::<usize>();
    tiles(&e.0.0) + tiles(&e.0.1)
}

#[cfg(test)]
thread_local! {
    static RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// (fill only, stroke only) of a stroked shape, rendered over `canvas` (RGBA8, as the CPU
/// compositor's split always was). `None` without a stroke. Where the shape's pixels
/// (Photoshop's rendering, `sh.cache`) reach past our fill, the part the fill doesn't explain is
/// stroke: the stroke follows Photoshop's coverage (psd-tools double-stroke-effects).
pub fn split(sh: &ShapeLayer, canvas: Rect) -> Option<(Surface, Surface)> {
    sh.stroke.as_ref()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (canvas.x0, canvas.y0, canvas.x1, canvas.y1).hash(&mut h);
    format!("{:?}{:?}{:?}{:?}", sh.path, sh.fill, sh.stroke, sh.live).hash(&mut h);
    if let Some(c) = &sh.cache {
        format!("{:?}{:?}", c.format(), c.default_pixel()).hash(&mut h);
        for (k, t) in c.tiles() {
            (k.tx, k.ty, std::sync::Arc::as_ptr(t) as usize).hash(&mut h);
        }
    }
    let key = h.finish();
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.1 += 1;
        let tick = c.1;
        if let Some(e) = c.0.get_mut(&key) {
            e.1 = tick;
            return Some(e.0.clone());
        }
    }
    #[cfg(test)]
    RENDERS.with(|r| r.set(r.get() + 1));
    let fmt = PixelFormat::RGBA8;
    let fill_only = ShapeLayer { stroke: None, cache: None, ..sh.clone() };
    let stroke_only = ShapeLayer { fill: None, cache: None, ..sh.clone() };
    let mut parts = (photocraft_vector::render_shape(&fill_only, fmt, canvas), photocraft_vector::render_shape(&stroke_only, fmt, canvas));
    if let Some(cache) = &sh.cache {
        fit_stroke(&parts.0, &mut parts.1, cache, canvas);
    }
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    let tick = c.1;
    c.0.insert(key, (parts.clone(), tick, sh.cache.clone()));
    let mut bytes: usize = c.0.values().map(entry_bytes).sum();
    while c.0.len() > 1 && (c.0.len() > CAPACITY || bytes > BUDGET_BYTES) {
        let Some(old) = c.0.iter().filter(|e| *e.0 != key).min_by_key(|e| (e.1).1).map(|e| *e.0) else { break };
        if let Some(e) = c.0.remove(&old) {
            bytes -= entry_bytes(&e);
        }
    }
    Some(parts)
}

/// Fits the stroke part to the shape's own pixels: where they reach past the fill, the stroke
/// covers what the fill doesn't (`(pixels - fill) / (1 - fill)`), in the pixels' colour where we
/// drew no stroke, and elsewhere in the colour that, drawn over the fill, gives back the pixels'
/// colour. Our stroke's own colour would be wrong where the file's fill covers more of the pixel
/// than ours (shapes snapped to whole pixels with a stroke thinner than a pixel): the fitted
/// stroke is then mostly the file's fill, and an edge drawn in the stroke's full colour is far
/// too dark.
fn fit_stroke(fill: &Surface, stroke: &mut Surface, cache: &Surface, canvas: Rect) {
    // Where either has pixels (the stroke may reach past the shape's pixels, and vice versa).
    let (cb, sb) = (crate::bounds::content_bounds(cache), crate::bounds::content_bounds(stroke));
    let r = if cb.is_empty() {
        sb
    } else if sb.is_empty() {
        cb
    } else {
        cb.union(&sb)
    }
    .intersect(&canvas);
    if r.is_empty() {
        return;
    }
    let n = r.width() as usize * r.height() as usize;
    let (mut f, mut s, mut c) = (vec![[0.0f32; 4]; n], vec![[0.0f32; 4]; n], vec![[0.0f32; 4]; n]);
    fill.read_rgba_into(r, &mut f);
    stroke.read_rgba_into(r, &mut s);
    cache.read_rgba_into(r, &mut c);
    let mut out = Vec::with_capacity(n * 4);
    for ((fp, sp), cp) in f.iter().zip(&s).zip(&c) {
        let mut p = *sp;
        if fp[3] < 1.0 - 1e-3 {
            let a = ((cp[3] - fp[3]) / (1.0 - fp[3])).clamp(0.0, 1.0);
            p = if sp[3] <= 0.0 {
                [cp[0], cp[1], cp[2], a]
            } else if a > 1e-3 {
                // Normal over the fill: a·p + f·(1 - a)·fill = pixels (premultiplied).
                let k = fp[3] * (1.0 - a);
                let un = |i: usize| ((cp[3] * cp[i] - k * fp[i]) / a).clamp(0.0, 1.0);
                [un(0), un(1), un(2), a]
            } else {
                [sp[0], sp[1], sp[2], a]
            };
        }
        out.extend_from_slice(&p);
    }
    stroke.write_region(r, &out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::Color;
    use photocraft_doc::Fill;
    use photocraft_doc::vector::{Path, ShapeStroke, Subpath};

    /// More stroked shapes than the old 32 entries: the second pass over them (the next plan
    /// rebuild) must rasterise none of them again.
    #[test]
    fn many_stroked_shapes_stay_cached_across_rebuilds() {
        let canvas = Rect::new(0, 0, 600, 400);
        let shapes: Vec<ShapeLayer> = (0..48)
            .map(|i| {
                let x = (i % 8) as f64 * 70.0 + 5.0;
                let y = (i / 8) as f64 * 60.0 + 5.0;
                let path = Path::new(vec![Subpath::polygon(&[(x, y), (x + 50.0, y), (x + 50.0, y + 40.0), (x, y + 40.0)])]);
                let stroke = ShapeStroke { width: 3.0, ..ShapeStroke::default() };
                ShapeLayer { path, fill: Some(Fill::Solid(Color::WHITE)), stroke: Some(stroke), live: None, cache: None, psd_raw: None }
            })
            .collect();
        let renders = || RENDERS.with(std::cell::Cell::get);
        let n0 = renders();
        for sh in &shapes {
            split(sh, canvas).expect("stroked");
        }
        assert_eq!(renders() - n0, shapes.len());
        for sh in &shapes {
            split(sh, canvas).expect("stroked");
        }
        assert_eq!(renders() - n0, shapes.len(), "the second rebuild hits the cache for every shape");
    }
}
