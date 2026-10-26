//! A stroked shape split into fill and stroke, cached independently of integer translation.
//! Parts are rasterised over their compiled geometry bounds and clipped to the canvas on use.
//! Photoshop draws the vector stroke above clipped layers and interior effects.
//! Document-depth rendering follows PhotoCraft #1495 (5355221f81e325a8682c7e3a7e03dd66cc00b5e6).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use photocraft_color::{Color, PixelFormat, SampleType};
use photocraft_doc::vector::{LiveShape, ShapeLayer};
use photocraft_doc::{Fill, Layer, LayerContent};
use photocraft_geom::{Rect, TILE_SIZE};
use photocraft_raster::Surface;
use photocraft_vector::{CompiledShape, DEFAULT_TOLERANCE};

struct Entry {
    parts: (Surface, Surface),
    bounds: (Rect, Rect),
    origin: (i32, i32),
    tick: u64,
    shape: ShapeLayer,
    pixels: Option<(u64, i64, i64)>,
    /// Huge geometry is clipped before allocating; canvas-anchored paint also depends on
    /// the exact frame. Both can only serve that canvas at that position.
    canvas: Option<Rect>,
}

fn cache() -> &'static Mutex<(HashMap<u64, Entry>, u64)> {
    static C: OnceLock<Mutex<(HashMap<u64, Entry>, u64)>> = OnceLock::new();
    C.get_or_init(|| Mutex::new((HashMap::new(), 0)))
}

/// Keep the previous capacity fix: a cyclic scan of more shapes than the capacity must not
/// evict every shape before the next plan rebuild (the layout benchmark has 33 stroked shapes).
const CAPACITY: usize = 1024;
/// Pixel memory the cached parts may hold together (the most recent entry is always kept).
const BUDGET_BYTES: usize = 512 << 20;

fn entry_bytes(e: &Entry) -> usize {
    let tiles = |s: &Surface| s.tiles().map(|(_, t)| t.bytes().len()).sum::<usize>();
    tiles(&e.parts.0) + tiles(&e.parts.1)
}

#[cfg(test)]
thread_local! {
    static RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn hash_color(c: &Color, h: &mut impl Hasher) {
    c.mode.hash(h);
    c.c.map(f32::to_bits).hash(h);
    c.alpha.to_bits().hash(h);
}

fn hash_fill(f: &Fill, h: &mut impl Hasher) {
    std::mem::discriminant(f).hash(h);
    match f {
        Fill::Solid(c) => hash_color(c, h),
        Fill::Gradient { method, stops, angle, scale, style, reverse, opacity_stops, midpoints, offset, dither, align } => {
            std::mem::discriminant(method).hash(h);
            stops.len().hash(h);
            for (p, c) in stops {
                p.to_bits().hash(h);
                hash_color(c, h);
            }
            angle.to_bits().hash(h);
            scale.to_bits().hash(h);
            std::mem::discriminant(style).hash(h);
            (reverse, dither, align).hash(h);
            opacity_stops.len().hash(h);
            for (p, a) in opacity_stops {
                (p.to_bits(), a.to_bits()).hash(h);
            }
            midpoints.len().hash(h);
            for m in midpoints {
                m.to_bits().hash(h);
            }
            (offset.0.to_bits(), offset.1.to_bits()).hash(h);
        }
        Fill::Pattern { name, scale, id, angle, link, phase } => {
            (name, id, link).hash(h);
            (scale.to_bits(), angle.to_bits(), phase.0.to_bits(), phase.1.to_bits()).hash(h);
        }
    }
}

fn paints(sh: &ShapeLayer) -> impl Iterator<Item = &Fill> {
    sh.fill.iter().chain(sh.stroke.iter().map(|s| &s.paint))
}

fn canvas_dependent(sh: &ShapeLayer) -> bool {
    sh.path.inverted || paints(sh).any(|f| matches!(f, Fill::Gradient { align: false, .. }))
}

/// Integer origin only: fractional position remains part of the cache state.
/// Inversion, canvas-anchored gradients and world-coordinate dither cannot reuse translated
/// pixels. Layer-aligned gradients without dither retain the group-move cache.
fn origin(sh: &ShapeLayer) -> (i32, i32) {
    if canvas_dependent(sh) || paints(sh).any(|f| matches!(f, Fill::Gradient { dither: true, .. })) {
        return (0, 0);
    }
    sh.path.control_bounds().map_or((0, 0), |b| (b.0.floor() as i32, b.1.floor() as i32))
}

fn pixel_identity(sh: &ShapeLayer, origin: (i32, i32)) -> Option<(u64, i64, i64)> {
    sh.cache.as_ref().map(|c| {
        let (source, dx, dy) = c.translation_identity();
        // The cache must move with the path. Moving only one invalidates the fitted stroke.
        (source, dx - i64::from(origin.0), dy - i64::from(origin.1))
    })
}

fn key(sh: &ShapeLayer, canvas: Rect, origin: (i32, i32), depth: SampleType) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    state_key_at(sh, canvas, origin, true).hash(&mut h);
    depth.hash(&mut h);
    h.finish()
}

/// Exact absolute shape state for the GPU plan key. Integer translations invalidate a plan's
/// world-space clips, but need not allocate Debug strings of every path on every plan lookup.
pub fn state_key(sh: &ShapeLayer) -> u64 {
    state_key_at(sh, Rect::EMPTY, (0, 0), false)
}

fn state_key_at(sh: &ShapeLayer, canvas: Rect, origin: (i32, i32), bucket: bool) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let (x, y) = (f64::from(origin.0), f64::from(origin.1));
    // Adding an integer to a Bézier handle can round its last f64 bits. Bucket relative
    // coordinates at 2^-20 px, then verify the EXACT translated coordinates on a hit below.
    // The bucket never authorizes reuse by itself, even for a subpixel edit below that size.
    let coordinate = |v: f64| if bucket { ((v * 1_048_576.0).round()).to_bits() } else { v.to_bits() };
    let point = |px: f64, py: f64| (coordinate(px - x), coordinate(py - y));
    sh.path.inverted.hash(&mut h);
    std::mem::discriminant(&sh.path.fill_rule).hash(&mut h);
    sh.path.subpaths.len().hash(&mut h);
    for s in &sh.path.subpaths {
        s.closed.hash(&mut h);
        std::mem::discriminant(&s.op).hash(&mut h);
        s.knots.len().hash(&mut h);
        for k in &s.knots {
            point(k.anchor.x, k.anchor.y).hash(&mut h);
            point(k.in_ctrl.x, k.in_ctrl.y).hash(&mut h);
            point(k.out_ctrl.x, k.out_ctrl.y).hash(&mut h);
            k.smooth.hash(&mut h);
        }
    }
    sh.fill.is_some().hash(&mut h);
    if let Some(f) = &sh.fill {
        hash_fill(f, &mut h);
    }
    sh.stroke.is_some().hash(&mut h);
    if let Some(s) = &sh.stroke {
        hash_fill(&s.paint, &mut h);
        (s.width.to_bits(), s.opacity.to_bits(), s.miter_limit.to_bits(), s.dash_offset.to_bits()).hash(&mut h);
        std::mem::discriminant(&s.align).hash(&mut h);
        std::mem::discriminant(&s.cap).hash(&mut h);
        std::mem::discriminant(&s.join).hash(&mut h);
        s.dashes.len().hash(&mut h);
        for d in &s.dashes {
            d.to_bits().hash(&mut h);
        }
    }
    sh.live.is_some().hash(&mut h);
    if let Some(l) = &sh.live {
        std::mem::discriminant(l).hash(&mut h);
        let rect = |r: [f64; 4]| (point(r[0], r[1]), r[2].to_bits(), r[3].to_bits());
        match l {
            LiveShape::Rect { rect: r, radii } => {
                rect(*r).hash(&mut h);
                radii.map(f64::to_bits).hash(&mut h);
            }
            LiveShape::Ellipse { rect: r } => rect(*r).hash(&mut h),
            LiveShape::Polygon { rect: r, sides, star_ratio } => (rect(*r), sides, star_ratio.to_bits()).hash(&mut h),
            LiveShape::Line { from, to, weight } => (point(from[0], from[1]), point(to[0], to[1]), weight.to_bits()).hash(&mut h),
        }
    }
    pixel_identity(sh, origin).hash(&mut h);
    if canvas_dependent(sh) {
        (canvas.x0, canvas.y0, canvas.x1, canvas.y1).hash(&mut h);
    }
    h.finish()
}

fn translated_live(live: &Option<LiveShape>, dx: i32, dy: i32) -> Option<LiveShape> {
    let (dx, dy) = (f64::from(dx), f64::from(dy));
    let rect = |r: [f64; 4]| [r[0] + dx, r[1] + dy, r[2], r[3]];
    let point = |p: [f64; 2]| [p[0] + dx, p[1] + dy];
    live.as_ref().map(|l| match l {
        LiveShape::Rect { rect: r, radii } => LiveShape::Rect { rect: rect(*r), radii: *radii },
        LiveShape::Ellipse { rect: r } => LiveShape::Ellipse { rect: rect(*r) },
        LiveShape::Polygon { rect: r, sides, star_ratio } => LiveShape::Polygon { rect: rect(*r), sides: *sides, star_ratio: *star_ratio },
        LiveShape::Line { from, to, weight } => LiveShape::Line { from: point(*from), to: point(*to), weight: *weight },
    })
}

fn matches_translation(old: &ShapeLayer, sh: &ShapeLayer, dx: i32, dy: i32) -> bool {
    if old.fill != sh.fill
        || old.stroke != sh.stroke
        || translated_live(&old.live, dx, dy) != sh.live
        || old.path.inverted != sh.path.inverted
        || old.path.fill_rule != sh.path.fill_rule
        || old.path.subpaths.len() != sh.path.subpaths.len()
    {
        return false;
    }
    let (dx, dy) = (f64::from(dx), f64::from(dy));
    old.path.subpaths.iter().zip(&sh.path.subpaths).all(|(a, b)| {
        a.op == b.op
            && a.closed == b.closed
            && a.knots.len() == b.knots.len()
            && a.knots.iter().zip(&b.knots).all(|(a, b)| {
                a.smooth == b.smooth
                    && [a.anchor, a.in_ctrl, a.out_ctrl].into_iter().zip([b.anchor, b.in_ctrl, b.out_ctrl]).all(|(a, b)| a.x + dx == b.x && a.y + dy == b.y)
            })
    })
}

/// Translate without resampling, then clip. Keeping the full bounded rendering in the cache
/// lets a shape move into view without reusing an entry that lost its off-canvas edge pixels.
fn placed(s: &Surface, bounds: Rect, dx: i32, dy: i32, canvas: Rect) -> Surface {
    let moved = bounds.translate(dx, dy);
    let clip = moved.intersect(&canvas);
    if clip.is_empty() {
        return Surface::new(s.format());
    }
    if canvas.contains_rect(&moved) && dx.rem_euclid(TILE_SIZE) == 0 && dy.rem_euclid(TILE_SIZE) == 0 {
        return s.translated(dx, dy);
    }
    let source = Rect::new(clip.x0.saturating_sub(dx), clip.y0.saturating_sub(dy), clip.x1.saturating_sub(dx), clip.y1.saturating_sub(dy));
    Surface::from_interleaved(s.format(), clip, &s.to_interleaved(source))
}

/// Appearance items are already merged in the shape cache in their explicit paint order.
/// Splitting the mirrored PSD stroke would lift it above clipping/effects a second time.
/// CPU and GPU callers must use this layer-aware boundary, not `split` on the PSD mirror.
pub fn split_layer(layer: &Layer, canvas: Rect, depth: SampleType) -> Option<(Surface, Surface)> {
    if layer.vector.appearance.is_some() {
        return None;
    }
    let LayerContent::Shape(shape) = &layer.content else { return None };
    split(shape, canvas, depth)
}

/// Stack effects use the merged cache's alpha, including explicit empty stacks. Legacy
/// filled shape effects may continue to use their analytic outline, as before.
pub fn uses_legacy_outline(layer: &Layer) -> bool {
    layer.vector.appearance.is_none()
}

/// (fill only, stroke only), RGBA at the document sample depth. `None` without a stroke.
/// Fitted strokes retain the shape cache's coverage and edge colour (psd-tools double-stroke).
pub fn split(sh: &ShapeLayer, canvas: Rect, depth: SampleType) -> Option<(Surface, Surface)> {
    sh.stroke.as_ref()?;
    let origin = origin(sh);
    let pixels = pixel_identity(sh, origin);
    let key = key(sh, canvas, origin, depth);
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.1 += 1;
        let tick = c.1;
        if let Some(e) = c.0.get_mut(&key)
            && let (Some(dx), Some(dy)) = (origin.0.checked_sub(e.origin.0), origin.1.checked_sub(e.origin.1))
            && e.parts.0.format().sample == depth
            && matches_translation(&e.shape, sh, dx, dy)
            && e.pixels == pixels
            && e.canvas.is_none_or(|clip| dx == 0 && dy == 0 && clip == canvas)
        {
            e.tick = tick;
            // Clone the sparse parts under the lock; copying pixels must not serialize renderers.
            let (parts, bounds) = (e.parts.clone(), e.bounds);
            drop(c);
            return Some((placed(&parts.0, bounds.0, dx, dy, canvas), placed(&parts.1, bounds.1, dx, dy, canvas)));
        }
    }
    #[cfg(test)]
    RENDERS.with(|r| r.set(r.get() + 1));
    let fmt = PixelFormat::RGBA8.with_sample(depth);
    let fill_only = ShapeLayer { stroke: None, cache: None, ..sh.clone() };
    let stroke_only = ShapeLayer { fill: None, cache: None, ..sh.clone() };
    let (fill, stroke) = (CompiledShape::new(&fill_only, DEFAULT_TOLERANCE, canvas), CompiledShape::new(&stroke_only, DEFAULT_TOLERANCE, canvas));
    let fb = fill.bounds().unwrap_or(Rect::EMPTY);
    let sb = stroke.bounds().unwrap_or(Rect::EMPTY);
    let cb = sh.cache.as_ref().map_or(Rect::EMPTY, crate::bounds::content_bounds);
    // Inverted fill is unbounded; everything else renders only its compiled fill/stroke bounds,
    // including miter joins and square caps rather than a guessed stroke-width margin.
    let area_pixels = |r: Rect| u64::from(r.width()) * u64::from(r.height());
    // The fitter also holds three float-RGBA inputs and an output buffer. A small path with
    // distant cached pixels must not make their union allocate a huge intermediate rectangle.
    let scoped = sh.path.inverted
        || area_pixels(fb).saturating_add(area_pixels(sb)) > (BUDGET_BYTES / fmt.bytes_per_pixel()) as u64
        || (sh.cache.is_some() && area_pixels(cb.union(&sb)) > (BUDGET_BYTES / 64) as u64);
    let (fb, sb) = if scoped { (fb.intersect(&canvas), sb.intersect(&canvas)) } else { (fb, sb) };
    let mut parts = (fill.render(fmt, fb), stroke.render(fmt, sb));
    if let Some(cache) = &sh.cache {
        let region = if cb.is_empty() {
            sb
        } else if sb.is_empty() {
            cb
        } else {
            cb.union(&sb)
        };
        fit_stroke(&parts.0, &mut parts.1, cache, if scoped { region.intersect(&canvas) } else { region });
    }
    let bounds = (crate::bounds::content_bounds(&parts.0), crate::bounds::content_bounds(&parts.1));
    let result = (placed(&parts.0, bounds.0, 0, 0, canvas), placed(&parts.1, bounds.1, 0, 0, canvas));
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    let tick = c.1;
    let shape = ShapeLayer { cache: None, psd_raw: None, ..sh.clone() };
    c.0.insert(key, Entry { parts, bounds, origin, tick, shape, pixels, canvas: (scoped || canvas_dependent(sh)).then_some(canvas) });
    let mut bytes: usize = c.0.values().map(entry_bytes).sum();
    while c.0.len() > 1 && (c.0.len() > CAPACITY || bytes > BUDGET_BYTES) {
        let Some(old) = c.0.iter().filter(|e| *e.0 != key).min_by_key(|e| e.1.tick).map(|e| *e.0) else { break };
        if let Some(e) = c.0.remove(&old) {
            bytes -= entry_bytes(&e);
        }
    }
    Some(result)
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
    let float = stroke.format().sample == SampleType::F32;
    for ((fp, sp), cp) in f.iter().zip(&s).zip(&c) {
        let mut p = *sp;
        if fp[3] < 1.0 - 1e-3 {
            let a = ((cp[3] - fp[3]) / (1.0 - fp[3])).clamp(0.0, 1.0);
            p = if sp[3] <= 0.0 {
                [cp[0], cp[1], cp[2], a]
            } else if a > 1e-3 {
                // Normal over the fill: a·p + f·(1 - a)·fill = pixels (premultiplied).
                let k = fp[3] * (1.0 - a);
                let un = |i: usize| {
                    let v = (cp[3] * cp[i] - k * fp[i]) / a;
                    if float { v } else { v.clamp(0.0, 1.0) }
                };
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
    use photocraft_doc::vector::{Path, ShapeStroke, Subpath};
    use photocraft_doc::{Fill, LayerContent};

    /// More stroked shapes than the old 32 entries: the second pass over them (the next plan
    /// rebuild) must rasterise none of them again.
    #[test]
    fn many_stroked_shapes_stay_cached_across_rebuilds() {
        let canvas = Rect::new(0, 0, 600, 400);
        let shapes: Vec<ShapeLayer> = (0..48)
            .map(|i| {
                let x = (i % 8) as f64 * 70.0 + 5.0;
                let y = (i / 8) as f64 * 60.0 + 5.0;
                let w = 50.0 + i as f64 / 64.0;
                let path = Path::new(vec![Subpath::polygon(&[(x, y), (x + w, y), (x + w, y + 40.0), (x, y + 40.0)])]);
                let stroke = ShapeStroke { width: 3.0, ..ShapeStroke::default() };
                ShapeLayer { path, fill: Some(Fill::Solid(Color::WHITE)), stroke: Some(stroke), live: None, cache: None, psd_raw: None }
            })
            .collect();
        let renders = || RENDERS.with(std::cell::Cell::get);
        let n0 = renders();
        for sh in &shapes {
            split(sh, canvas, SampleType::U8).expect("stroked");
        }
        assert_eq!(renders() - n0, shapes.len());
        for sh in &shapes {
            split(sh, canvas, SampleType::U8).expect("stroked");
        }
        assert_eq!(renders() - n0, shapes.len(), "the second rebuild hits the cache for every shape");
    }

    /// The implementation before translation caching: rasterise at the final position, clipped
    /// to the canvas, and fit the stroke to the shape cache there.
    fn fresh(sh: &ShapeLayer, canvas: Rect) -> (Surface, Surface) {
        let fill = ShapeLayer { stroke: None, cache: None, ..sh.clone() };
        let stroke = ShapeLayer { fill: None, cache: None, ..sh.clone() };
        let mut parts =
            (photocraft_vector::render_shape(&fill, PixelFormat::RGBA8, canvas), photocraft_vector::render_shape(&stroke, PixelFormat::RGBA8, canvas));
        if let Some(c) = &sh.cache {
            fit_stroke(&parts.0, &mut parts.1, c, canvas);
        }
        parts
    }

    fn assert_parts(actual: &(Surface, Surface), expected: &(Surface, Surface), canvas: Rect) {
        for (name, a, b) in [("fill", &actual.0, &expected.0), ("stroke", &actual.1, &expected.1)] {
            let (a, b) = (a.to_interleaved(canvas), b.to_interleaved(canvas));
            let differences: Vec<_> = a
                .iter()
                .zip(&b)
                .enumerate()
                .filter(|(i, (av, bv))| {
                    // Straight RGB behind zero alpha is not part of the rendered result. The area
                    // rasterizer can leave floating-point residue there after translating curves.
                    av != bv && (i % 4 == 3 || a[i / 4 * 4 + 3] != 0 || b[i / 4 * 4 + 3] != 0)
                })
                .take(8)
                .map(|(i, (a, b))| (canvas.x0 + (i / 4 % canvas.width() as usize) as i32, canvas.y0 + (i / 4 / canvas.width() as usize) as i32, i % 4, *a, *b))
                .collect();
            assert!(differences.is_empty(), "{name} bytes differ (x, y, channel, cached, fresh): {differences:?}");
        }
        let outside = canvas.inflate(3);
        for y in outside.y0..outside.y1 {
            for x in outside.x0..outside.x1 {
                if !canvas.contains(x, y) {
                    assert_eq!(actual.0.pixel(x, y), vec![0.0; 4]);
                    assert_eq!(actual.1.pixel(x, y), vec![0.0; 4]);
                }
            }
        }
    }

    fn translated(sh: &ShapeLayer, dx: i32, dy: i32) -> ShapeLayer {
        let mut moved = sh.clone();
        moved.path = sh.path.transform(&photocraft_geom::Affine::translate(f64::from(dx), f64::from(dy)));
        moved.cache = sh.cache.as_ref().map(|c| c.translated(dx, dy));
        if let Some(LiveShape::Ellipse { rect }) = &mut moved.live {
            rect[0] += f64::from(dx);
            rect[1] += f64::from(dy);
        }
        moved
    }

    #[test]
    fn integer_moves_hit_and_match_fresh_rasterisation() {
        use photocraft_doc::{GradientStyle, StrokeAlign};
        let canvas = Rect::new(-20, -15, 310, 285);
        for (i, align) in [StrokeAlign::Inside, StrokeAlign::Center, StrokeAlign::Outside].into_iter().enumerate() {
            for fmt in [None, Some(PixelFormat::RGBA8), Some(PixelFormat::RGBA16), Some(PixelFormat::RGBA32F)] {
                let live = LiveShape::Ellipse { rect: [17.25, 21.5, 49.5 + i as f64, 38.25] };
                let mut sh = ShapeLayer {
                    path: photocraft_vector::shapes::live_path(&live),
                    fill: Some(Fill::gradient(vec![(0.0, Color::rgba(0.7, 0.2, 0.1, 0.4)), (1.0, Color::WHITE)], 31.0, 0.8, GradientStyle::Linear, false)),
                    stroke: Some(ShapeStroke {
                        width: 2.75,
                        opacity: 0.8,
                        align,
                        dashes: vec![2.0, 1.25],
                        paint: Fill::gradient(vec![(0.0, Color::BLACK), (1.0, Color::rgb(0.2, 0.6, 0.9))], 17.0, 1.0, GradientStyle::Radial, true),
                        ..ShapeStroke::default()
                    }),
                    live: Some(live),
                    ..ShapeLayer::default()
                };
                if let Some(fmt) = fmt {
                    sh.cache = Some(photocraft_vector::render_shape(&sh, fmt, canvas));
                }
                let n0 = RENDERS.with(std::cell::Cell::get);
                assert_parts(&split(&sh, canvas, SampleType::U8).unwrap(), &fresh(&sh, canvas), canvas);
                let n1 = RENDERS.with(std::cell::Cell::get);
                assert_eq!(n1, n0 + 1);
                for (dx, dy) in [(37, 23), (-29, -35), (256, 256), (-256, 0), (300, -300), (0, 0)] {
                    let moved = translated(&sh, dx, dy);
                    assert_parts(&split(&moved, canvas, SampleType::U8).unwrap(), &fresh(&moved, canvas), canvas);
                    assert_eq!(RENDERS.with(std::cell::Cell::get), n1, "integer move ({dx}, {dy}) hits");
                }
            }
        }
    }

    #[test]
    fn pixel_aligned_stroked_shape_moves_hit_and_equal_fresh_bytes() {
        let canvas = Rect::new(0, 0, 360, 320);
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(15.25, 20.5), (70.75, 20.5), (70.75, 63.5), (15.25, 63.5)])]),
            fill: Some(Fill::Solid(Color::rgb(0.57, 0.39, 0.21))),
            stroke: Some(ShapeStroke { width: 3.5, ..ShapeStroke::default() }),
            ..ShapeLayer::default()
        };
        sh.cache = Some(photocraft_vector::render_shape(&sh, PixelFormat::RGBA8, canvas));
        split(&sh, canvas, SampleType::U8).unwrap();
        let n = RENDERS.with(std::cell::Cell::get);
        for (dx, dy) in [(37, 23), (-20, -17), (256, 256), (0, 0)] {
            let moved = translated(&sh, dx, dy);
            let (actual, expected) = (split(&moved, canvas, SampleType::U8).unwrap(), fresh(&moved, canvas));
            assert!(actual.0.to_interleaved(canvas) == expected.0.to_interleaved(canvas), "all fill bytes at ({dx}, {dy})");
            assert!(actual.1.to_interleaved(canvas) == expected.1.to_interleaved(canvas), "all stroke bytes at ({dx}, {dy})");
            assert_eq!(RENDERS.with(std::cell::Cell::get), n, "integer move hits the split cache");
        }
    }

    #[test]
    fn subpixel_edits_smaller_than_the_hash_bucket_are_not_reused() {
        let canvas = Rect::new(0, 0, 80, 70);
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(11.25, 12.5), (51.25, 12.5), (51.25, 52.5), (11.25, 52.5)])]),
            fill: Some(Fill::Solid(Color::rgb(0.17, 0.37, 0.67))),
            stroke: Some(ShapeStroke::default()),
            ..ShapeLayer::default()
        };
        split(&sh, canvas, SampleType::U8).unwrap();
        let n = RENDERS.with(std::cell::Cell::get);
        let old_key = key(&sh, canvas, origin(&sh), SampleType::U8);
        sh.path.subpaths[0].knots[1].anchor.x += 1e-8;
        assert_eq!(old_key, key(&sh, canvas, origin(&sh), SampleType::U8), "the test deliberately uses the same coordinate bucket");
        assert_parts(&split(&sh, canvas, SampleType::U8).unwrap(), &fresh(&sh, canvas), canvas);
        assert_eq!(RENDERS.with(std::cell::Cell::get), n + 1, "exact coordinates reject the old entry");
    }

    #[test]
    fn off_canvas_parts_are_not_lost_and_canvas_size_is_not_in_the_key() {
        let canvas = Rect::new(0, 0, 45, 50);
        let sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(-12.5, -9.25), (30.5, -9.25), (30.5, 32.75), (-12.5, 32.75)])]),
            fill: Some(Fill::Solid(Color::rgb(0.13, 0.27, 0.61))),
            stroke: Some(ShapeStroke { width: 7.25, ..ShapeStroke::default() }),
            ..ShapeLayer::default()
        };
        assert_parts(&split(&sh, canvas, SampleType::U8).unwrap(), &fresh(&sh, canvas), canvas);
        let n = RENDERS.with(std::cell::Cell::get);
        let larger = Rect::new(-40, -40, 100, 100);
        let moved = translated(&sh, 20, 25);
        assert_parts(&split(&moved, larger, SampleType::U8).unwrap(), &fresh(&moved, larger), larger);
        assert_eq!(RENDERS.with(std::cell::Cell::get), n);
    }

    #[test]
    fn fractional_moves_and_cache_edits_invalidate_the_split() {
        let canvas = Rect::new(0, 0, 90, 80);
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(10.25, 12.5), (63.5, 12.5), (63.5, 61.25), (10.25, 61.25)])]),
            fill: Some(Fill::Solid(Color::rgb(0.43, 0.23, 0.73))),
            stroke: Some(ShapeStroke { width: 0.25, ..ShapeStroke::default() }),
            ..ShapeLayer::default()
        };
        let mut pixels = Surface::new(PixelFormat::RGBA16);
        pixels.fill_rect(Rect::new(10, 12, 64, 62), &[0.6, 0.7, 0.8, 1.0]);
        sh.cache = Some(pixels);
        assert_parts(&split(&sh, canvas, SampleType::U8).unwrap(), &fresh(&sh, canvas), canvas);
        let n = RENDERS.with(std::cell::Cell::get);
        let mut moved = translated(&sh, 7, -5);
        assert_parts(&split(&moved, canvas, SampleType::U8).unwrap(), &fresh(&moved, canvas), canvas);
        assert_eq!(RENDERS.with(std::cell::Cell::get), n);
        moved.cache.as_mut().unwrap().write_pixel(17, 20, &[0.1, 0.2, 0.3, 0.5]);
        assert_parts(&split(&moved, canvas, SampleType::U8).unwrap(), &fresh(&moved, canvas), canvas);
        assert_eq!(RENDERS.with(std::cell::Cell::get), n + 1);
        sh.path = sh.path.transform(&photocraft_geom::Affine::translate(0.25, 0.0));
        assert_parts(&split(&sh, canvas, SampleType::U8).unwrap(), &fresh(&sh, canvas), canvas);
        assert_eq!(RENDERS.with(std::cell::Cell::get), n + 2);
        // The cache must move with the path, rather than retain pixels at its old position.
        let mut unlinked = translated(&sh, 5, 3);
        unlinked.cache = sh.cache.clone();
        assert_parts(&split(&unlinked, canvas, SampleType::U8).unwrap(), &fresh(&unlinked, canvas), canvas);
        assert_eq!(RENDERS.with(std::cell::Cell::get), n + 3);
    }

    #[test]
    fn inverted_fill_uses_the_current_canvas_and_absolute_path() {
        let canvas = Rect::new(0, 0, 80, 70);
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(10.0, 10.0), (30.0, 10.0), (30.0, 40.0), (10.0, 40.0)])]),
            fill: Some(Fill::Solid(Color::rgb(0.31, 0.51, 0.71))),
            stroke: Some(ShapeStroke::default()),
            ..ShapeLayer::default()
        };
        sh.path.inverted = true;
        for (sh, canvas) in [(sh.clone(), canvas), (translated(&sh, 7, -3), canvas), (sh.clone(), Rect::new(-10, -10, 95, 80))] {
            assert_parts(&split(&sh, canvas, SampleType::U8).unwrap(), &fresh(&sh, canvas), canvas);
        }
    }

    #[test]
    fn enormous_shapes_render_only_the_canvas_and_do_not_reuse_clipped_parts() {
        let canvas = Rect::new(0, 0, 40, 30);
        let sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(-100_000.0, -100_000.0), (100_000.0, -100_000.0), (100_000.0, 100_000.0), (-100_000.0, 100_000.0)])]),
            fill: Some(Fill::Solid(Color::rgb(0.47, 0.63, 0.81))),
            stroke: Some(ShapeStroke::default()),
            ..ShapeLayer::default()
        };
        let parts = split(&sh, canvas, SampleType::U8).unwrap();
        assert_parts(&parts, &fresh(&sh, canvas), canvas);
        assert!(parts.0.tile_count() <= 1);
        let n = RENDERS.with(std::cell::Cell::get);
        let moved = translated(&sh, 7, 3);
        assert_parts(&split(&moved, canvas, SampleType::U8).unwrap(), &fresh(&moved, canvas), canvas);
        assert_eq!(RENDERS.with(std::cell::Cell::get), n + 1);
        let larger = canvas.inflate(5);
        assert_parts(&split(&moved, larger, SampleType::U8).unwrap(), &fresh(&moved, larger), larger);
        assert_eq!(RENDERS.with(std::cell::Cell::get), n + 2);
    }

    #[test]
    fn distant_shape_cache_pixels_do_not_expand_the_fitter_allocation() {
        let canvas = Rect::new(0, 0, 50, 40);
        let mut pixels = Surface::new(PixelFormat::RGBA8);
        pixels.write_pixel(12, 13, &[0.3, 0.5, 0.7, 1.0]);
        pixels.write_pixel(100_000, 100_000, &[0.3, 0.5, 0.7, 1.0]);
        let sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(10.0, 10.0), (25.0, 10.0), (25.0, 25.0), (10.0, 25.0)])]),
            fill: Some(Fill::Solid(Color::rgb(0.27, 0.43, 0.65))),
            stroke: Some(ShapeStroke::default()),
            cache: Some(pixels),
            ..ShapeLayer::default()
        };
        assert_parts(&split(&sh, canvas, SampleType::U8).unwrap(), &fresh(&sh, canvas), canvas);
        let n = RENDERS.with(std::cell::Cell::get);
        let moved = translated(&sh, 7, 3);
        assert_parts(&split(&moved, canvas, SampleType::U8).unwrap(), &fresh(&moved, canvas), canvas);
        assert_eq!(RENDERS.with(std::cell::Cell::get), n + 1, "a clipped entry cannot supply another position");
    }

    #[test]
    fn translated_parts_keep_document_depth_and_hdr_edge_colours() {
        let canvas = Rect::new(0, 0, 120, 90);
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(13.25, 15.5), (65.75, 15.5), (65.75, 53.5), (13.25, 53.5)])]),
            fill: Some(Fill::gradient(vec![(0.0, Color::BLACK), (1.0, Color::WHITE)], 0.0, 1.0, photocraft_doc::GradientStyle::Linear, false)),
            stroke: Some(ShapeStroke { width: 2.75, paint: Fill::Solid(Color::rgb(1.7, 0.25, 0.5)), ..ShapeStroke::default() }),
            ..ShapeLayer::default()
        };
        // Identical pixel lineage at every depth: depth alone must distinguish entries.
        sh.cache = Some(photocraft_vector::render_shape(&sh, PixelFormat::RGBA32F, canvas));
        for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let fmt = PixelFormat::RGBA8.with_sample(depth);
            let parts = split(&sh, canvas, depth).unwrap();
            assert_eq!(parts.0.format(), fmt);
            assert_eq!(parts.1.format(), fmt);
            if depth == SampleType::F32 {
                assert!(parts.1.pixel(12, 30)[0] > 1.5, "fitted HDR edge colour must not clamp");
            }
            let n = RENDERS.with(std::cell::Cell::get);
            let moved = translated(&sh, 17, 11);
            let actual = split(&moved, canvas, depth).unwrap();
            let fill = ShapeLayer { stroke: None, cache: None, ..moved.clone() };
            let stroke = ShapeLayer { fill: None, cache: None, ..moved.clone() };
            let mut expected = (photocraft_vector::render_shape(&fill, fmt, canvas), photocraft_vector::render_shape(&stroke, fmt, canvas));
            fit_stroke(&expected.0, &mut expected.1, moved.cache.as_ref().unwrap(), canvas);
            for (a, b) in [(&actual.0, &expected.0), (&actual.1, &expected.1)] {
                assert_eq!(a.to_interleaved(canvas), b.to_interleaved(canvas), "{depth:?}: moved bytes match fresh rendering");
            }
            assert_eq!(RENDERS.with(std::cell::Cell::get), n, "each depth still reuses integer moves");
        }
    }

    #[test]
    fn appearance_stacks_never_split_the_psd_mirror_or_supply_legacy_effect_outline() {
        let shape = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(4., 4.), (12., 4.), (12., 12.), (4., 12.)])]),
            fill: Some(Fill::Solid(Color::WHITE)),
            stroke: Some(ShapeStroke::default()),
            ..Default::default()
        };
        let mut layer = photocraft_doc::Layer::new("Appearance", LayerContent::Shape(shape));
        let canvas = Rect::new(0, 0, 16, 16);
        assert!(split_layer(&layer, canvas, SampleType::U8).is_some());
        assert!(uses_legacy_outline(&layer));
        layer.vector.appearance = Some(Default::default());
        assert!(split_layer(&layer, canvas, SampleType::U8).is_none());
        assert!(!uses_legacy_outline(&layer));
        layer.vector.appearance.as_mut().unwrap().items.push(photocraft_doc::vector_design::appearance::AppearanceItem::Fill(
            photocraft_doc::vector_design::appearance::FillItem::new(Fill::Solid(Color::BLACK)),
        ));
        assert!(split_layer(&layer, canvas, SampleType::F32).is_none());
    }

    #[test]
    fn canvas_gradients_and_dither_repaint_at_their_world_position() {
        let canvas = Rect::new(0, 0, 140, 100);
        for stroke_paint in [false, true] {
            for (align, dither) in [(false, false), (true, true), (false, true)] {
                let paint = Fill::Gradient {
                    method: Default::default(),
                    stops: vec![(0.0, Color::BLACK), (1.0, Color::WHITE)],
                    angle: 0.0,
                    scale: 1.0,
                    style: photocraft_doc::GradientStyle::Linear,
                    reverse: false,
                    opacity_stops: vec![(0.0, 0.9), (1.0, 0.3)],
                    midpoints: vec![0.35],
                    offset: (0.1, 0.0),
                    dither,
                    align,
                };
                let sh = ShapeLayer {
                    path: Path::new(vec![Subpath::polygon(&[(17.25, 19.5), (69.75, 19.5), (69.75, 58.5), (17.25, 58.5)])]),
                    fill: Some(if stroke_paint { Fill::Solid(Color::rgb(0.15, 0.35, 0.55)) } else { paint.clone() }),
                    stroke: Some(ShapeStroke { width: 4.5, paint: if stroke_paint { paint } else { Fill::Solid(Color::BLACK) }, ..ShapeStroke::default() }),
                    ..ShapeLayer::default()
                };
                assert_parts(&split(&sh, canvas, SampleType::U8).unwrap(), &fresh(&sh, canvas), canvas);
                let n = RENDERS.with(std::cell::Cell::get);
                let moved = translated(&sh, 23, 11);
                assert_parts(&split(&moved, canvas, SampleType::U8).unwrap(), &fresh(&moved, canvas), canvas);
                assert_eq!(RENDERS.with(std::cell::Cell::get), n + 1, "world-position paint invalidates an integer move");
                let larger = Rect::new(-20, -10, 180, 130);
                assert_parts(&split(&moved, larger, SampleType::U8).unwrap(), &fresh(&moved, larger), larger);
                assert_eq!(RENDERS.with(std::cell::Cell::get), n + 1 + usize::from(!align), "canvas anchoring follows a changed canvas");
            }
        }
    }
    #[test]
    fn method_only_edits_invalidate_shape_parts_and_keep_translation_reuse() {
        let canvas = Rect::new(0, 0, 64, 32);
        let fill = Fill::gradient(vec![(0.0, Color::BLACK), (1.0, Color::WHITE)], 0.0, 1.0, photocraft_doc::GradientStyle::Linear, false);
        let mut shape = ShapeLayer {
            path: photocraft_vector::shapes::rect(4.0, 4.0, 40.0, 20.0),
            fill: Some(fill),
            stroke: Some(ShapeStroke::default()),
            ..Default::default()
        };
        let before = split(&shape, canvas, SampleType::F32).unwrap().0;
        let old_key = key(&shape, canvas, origin(&shape), SampleType::F32);
        if let Some(Fill::Gradient { method, .. }) = &mut shape.fill {
            *method = photocraft_doc::GradientMethod::Perceptual;
        }
        assert_ne!(old_key, key(&shape, canvas, origin(&shape), SampleType::F32));
        let after = split(&shape, canvas, SampleType::F32).unwrap().0;
        assert!((before.pixel(24, 12)[0] - after.pixel(24, 12)[0]).abs() > 0.1);
        shape.path = shape.path.transform(&photocraft_geom::Affine::translate(4.0, 0.0));
        let moved = split(&shape, canvas, SampleType::F32).unwrap().0;
        assert_eq!(after.pixel(24, 12), moved.pixel(28, 12));
    }
}
