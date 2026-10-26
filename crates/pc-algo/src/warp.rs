//! Mesh warps (Edit › Transform › Warp, smart-object warps): resampling a surface through any
//! forward map, such as a [`photocraft_geom::warp::Warp`] (bicubic Bezier patches or a preset).
//!
//! The source rectangle is subdivided into a fine grid whose vertices are mapped forward; each
//! cell's two triangles are rasterized in destination space with the source position
//! interpolated barycentrically, then the source is sampled there (premultiplied, nearest /
//! bilinear / Photoshop’s Keys bicubic, like Free Transform). Cells are a few pixels across in both
//! the source and the destination (a thin layer stretched far gets as many cells as its warped
//! size needs), so the piecewise-affine inverse is within a small fraction of a pixel of the true
//! one; affine maps (the identity included) are reproduced exactly. Where the warp folds over itself the
//! later (lower-right) cell wins. Work is split per destination tile (parallel on native).

use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::transform::Interp;

/// Grid cell size (source px) used to approximate the map.
const CELL: f64 = 4.0;
/// Most cells along one axis (bounds memory and time on huge layers).
const MAX_CELLS: usize = 512;

/// Places a smart object's source image (`src`, its pixels in `src_rect`) in the document:
/// through `warp` (source space) and then the affine `t` (source → document) in one resampling
/// pass, an exact shift for whole-pixel translations (so conversions and re-renders are
/// lossless), bicubic otherwise. Shared by the engine's re-render and PSD export's filter cache.
pub fn place_source(src: &Surface, src_rect: Rect, t: &photocraft_geom::Affine, warp: Option<&photocraft_geom::warp::Warp>) -> Surface {
    let [a, b, c, d, e, f] = t.m;
    if let Some(w) = warp.filter(|w| !w.is_identity()) {
        let map = |x: f64, y: f64| {
            let (u, v) = w.map(x, y);
            (a * u + c * v + e, b * u + d * v + f)
        };
        return warp_mesh_surface(src, src_rect, &map, Interp::Bicubic);
    }
    let near = |x: f64, y: f64| (x - y).abs() < 1e-9;
    if near(a, 1.0) && near(b, 0.0) && near(c, 0.0) && near(d, 1.0) && near(e, e.round()) && near(f, f.round()) {
        return crate::resample::translate_surface(src, e.round() as i32, f.round() as i32);
    }
    crate::transform::warp_surface(src, src_rect, &crate::transform::Homography([a, c, e, b, d, f, 0.0, 0.0, 1.0]), Interp::Bicubic)
}

/// [`place_source`] through a projective map `h` (source → document; Distort, Perspective): the
/// warp first (in source space), then `h`.
pub fn place_source_projective(src: &Surface, src_rect: Rect, h: &crate::transform::Homography, warp: Option<&photocraft_geom::warp::Warp>) -> Surface {
    if let Some(w) = warp.filter(|w| !w.is_identity()) {
        let map = |x: f64, y: f64| {
            let (u, v) = w.map(x, y);
            h.apply(u, v)
        };
        return warp_mesh_surface(src, src_rect, &map, Interp::Bicubic);
    }
    crate::transform::warp_surface(src, src_rect, h, Interp::Bicubic)
}

/// Warps the content of `src` inside `src_rect` through the forward map `f` (source document
/// coordinates → destination document coordinates). Output has `src`'s format with alpha
/// added; pixels outside the warped area are transparent.
pub fn warp_mesh_surface(src: &Surface, src_rect: Rect, f: &(dyn Fn(f64, f64) -> (f64, f64) + Sync), interp: Interp) -> Surface {
    if src_rect.is_empty() {
        let fmt = src.format();
        return Surface::new(PixelFormat::new(fmt.mode, fmt.sample, true));
    }
    let (sw, sh) = (f64::from(src_rect.width()), f64::from(src_rect.height()));
    // Cells are a few pixels across on both sides of the map: a thin source stretched far (a
    // 3 px column scaled into a wide band, then bent) needs many cells, or it comes out straight.
    let (lu, lv) = mapped_lengths(src_rect, f);
    let cells = |s: f64, d: f64| ((s.max(d) / CELL).ceil() as usize).clamp(1, MAX_CELLS);
    let (nx, ny) = (cells(sw, lu), cells(sh, lv));
    // Forward-mapped vertices (and their source positions).
    let mut verts = Vec::with_capacity((nx + 1) * (ny + 1));
    for j in 0..=ny {
        for i in 0..=nx {
            let sx = f64::from(src_rect.x0) + sw * i as f64 / nx as f64;
            let sy = f64::from(src_rect.y0) + sh * j as f64 / ny as f64;
            let (dx, dy) = f(sx, sy);
            verts.push(([dx, dy], [sx, sy]));
        }
    }
    let w1 = nx + 1;
    let mut tris = Vec::with_capacity(nx * ny * 2);
    for j in 0..ny {
        for i in 0..nx {
            let a = j * w1 + i;
            tris.push([a, a + 1, a + w1 + 1]);
            tris.push([a, a + w1 + 1, a + w1]);
        }
    }
    warp_triangles(src, src_rect, &verts, &tris, interp)
}

/// Probe lines per axis [`mapped_lengths`] follows through the map.
const PROBE: usize = 16;

/// The longest destination length of a source row and of a source column under `f`, measured
/// along polylines through a [`PROBE`]-step grid. Non-finite steps count as zero.
fn mapped_lengths(src_rect: Rect, f: &(dyn Fn(f64, f64) -> (f64, f64) + Sync)) -> (f64, f64) {
    let (x0, y0) = (f64::from(src_rect.x0), f64::from(src_rect.y0));
    let (sw, sh) = (f64::from(src_rect.width()), f64::from(src_rect.height()));
    let n = PROBE + 1;
    let mut pts = Vec::with_capacity(n * n);
    for j in 0..n {
        for i in 0..n {
            let (dx, dy) = f(x0 + sw * i as f64 / PROBE as f64, y0 + sh * j as f64 / PROBE as f64);
            pts.push([dx, dy]);
        }
    }
    let step = |a: [f64; 2], b: [f64; 2]| {
        let d = (b[0] - a[0]).hypot(b[1] - a[1]);
        if d.is_finite() { d } else { 0.0 }
    };
    let (mut lu, mut lv) = (0.0f64, 0.0f64);
    for k in 0..n {
        let (mut row, mut col) = (0.0, 0.0);
        for m in 1..n {
            if let (Some(&a), Some(&b)) = (pts.get(k * n + m - 1), pts.get(k * n + m)) {
                row += step(a, b);
            }
            if let (Some(&a), Some(&b)) = (pts.get((m - 1) * n + k), pts.get(m * n + k)) {
                col += step(a, b);
            }
        }
        lu = lu.max(row);
        lv = lv.max(col);
    }
    (lu, lv)
}

/// Destination tiles one triangle render may keep bookkeeping for (a 1M-tile render is 69 Gpx).
const MAX_DST_TILES: u64 = 1 << 20;
/// Source pixels one destination tile may read densely.
const MAX_FOOTPRINT_PIXELS: u64 = 1 << 25;

/// Rasterizes textured triangles: each vertex is `(destination, source)` in document pixels and
/// each triangle samples `src` (restricted to `src_rect`) through the barycentric
/// interpolation of its source positions. Later triangles in `tris` draw over earlier ones (the
/// order is the depth order). Output has `src`'s format with alpha added.
pub fn warp_triangles(src: &Surface, src_rect: Rect, verts: &[([f64; 2], [f64; 2])], tris: &[[usize; 3]], interp: Interp) -> Surface {
    let mut fmt = src.format();
    let converted;
    let src = if fmt.alpha {
        src
    } else {
        fmt = PixelFormat::new(fmt.mode, fmt.sample, true);
        converted = src.convert(fmt);
        &converted
    };
    let mut out = Surface::new(fmt);
    if src_rect.is_empty() || tris.is_empty() {
        return out;
    }
    if verts.iter().any(|v| !v.0[0].is_finite() || !v.0[1].is_finite()) {
        return out;
    }
    let lim = f64::from(1 << 20);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for v in tris.iter().flatten().map(|&i| &verts[i]) {
        x0 = x0.min(v.0[0]);
        y0 = y0.min(v.0[1]);
        x1 = x1.max(v.0[0]);
        y1 = y1.max(v.0[1]);
    }
    let dst = Rect::new(x0.max(-lim).floor() as i32 - 1, y0.max(-lim).floor() as i32 - 1, x1.min(lim).ceil() as i32 + 1, y1.min(lim).ceil() as i32 + 1);
    // Bookkeeping is kept per destination tile, so a mesh flung across the whole coordinate range
    // would ask for millions of entries before drawing anything: draw nothing instead.
    let ts_px = photocraft_geom::TILE_SIZE as u64;
    if u64::from(dst.width()).div_ceil(ts_px).saturating_add(1).saturating_mul(u64::from(dst.height()).div_ceil(ts_px).saturating_add(1)) > MAX_DST_TILES {
        return out;
    }
    // Triangles: (vertex indices), binned per destination tile.
    let tiles: Vec<Rect> = dst.tiles().map(|tc| tc.rect().intersect(&dst)).filter(|r| !r.is_empty()).collect();
    let mut bins: Vec<Vec<[usize; 3]>> = vec![Vec::new(); tiles.len()];
    let ts = photocraft_geom::TILE_SIZE;
    // Bin origin on the tile grid (the first tile is clipped to `dst`, so its x0/y0 may not be).
    let tx0 = tiles.iter().map(|t| t.x0).min().unwrap_or(0).div_euclid(ts) * ts;
    let ty0 = tiles.iter().map(|t| t.y0).min().unwrap_or(0).div_euclid(ts) * ts;
    let cols = tiles.iter().map(|t| (t.x0 - tx0) / ts).max().unwrap_or(0) as usize + 1;
    let grid_of = |t: &Rect| ((t.y0 - ty0) / ts) as usize * cols + ((t.x0 - tx0) / ts) as usize;
    let mut lookup = vec![usize::MAX; cols * (tiles.iter().map(|t| (t.y0 - ty0) / ts).max().unwrap_or(0) as usize + 1)];
    for (k, t) in tiles.iter().enumerate() {
        lookup[grid_of(t)] = k;
    }
    for &tri in tris {
        let ps = tri.map(|v| verts[v].0);
        let bx0 = ps.iter().map(|p| p[0]).fold(f64::MAX, f64::min).floor() as i32;
        let by0 = ps.iter().map(|p| p[1]).fold(f64::MAX, f64::min).floor() as i32;
        let bx1 = ps.iter().map(|p| p[0]).fold(f64::MIN, f64::max).ceil() as i32;
        let by1 = ps.iter().map(|p| p[1]).fold(f64::MIN, f64::max).ceil() as i32;
        let (cx0, cy0) = ((bx0.max(dst.x0) - tx0).div_euclid(ts), (by0.max(dst.y0) - ty0).div_euclid(ts));
        let (cx1, cy1) = ((bx1.min(dst.x1 - 1) - tx0).div_euclid(ts), (by1.min(dst.y1 - 1) - ty0).div_euclid(ts));
        for cy in cy0.max(0)..=cy1 {
            for cx in cx0.max(0)..=cx1 {
                let g = cy as usize * cols + cx as usize;
                if let Some(&k) = lookup.get(g)
                    && k != usize::MAX
                {
                    bins[k].push(tri);
                }
            }
        }
    }
    let n = fmt.channels();
    let a = n - 1;
    let work = |(t, tris): (&Rect, &Vec<[usize; 3]>)| -> Option<(Rect, Vec<f32>)> {
        if tris.is_empty() {
            return None;
        }
        let w = t.width() as usize;
        let h = t.height() as usize;
        // Source position per destination pixel (NaN = not covered).
        let mut uv = vec![[f64::NAN; 2]; w * h];
        for tri in tris {
            let [(p0, s0), (p1, s1), (p2, s2)] = tri.map(|v| verts[v]);
            let det = (p1[0] - p0[0]) * (p2[1] - p0[1]) - (p2[0] - p0[0]) * (p1[1] - p0[1]);
            if det.abs() < 1e-12 {
                continue;
            }
            let bx0 = (p0[0].min(p1[0]).min(p2[0]) - 0.5).floor().max(f64::from(t.x0)) as i32;
            let by0 = (p0[1].min(p1[1]).min(p2[1]) - 0.5).floor().max(f64::from(t.y0)) as i32;
            let bx1 = (p0[0].max(p1[0]).max(p2[0]) + 0.5).ceil().min(f64::from(t.x1)) as i32;
            let by1 = (p0[1].max(p1[1]).max(p2[1]) + 0.5).ceil().min(f64::from(t.y1)) as i32;
            let eps = -1e-9;
            for y in by0..by1 {
                let py = f64::from(y) + 0.5;
                for x in bx0..bx1 {
                    let px = f64::from(x) + 0.5;
                    let l1 = ((px - p0[0]) * (p2[1] - p0[1]) - (p2[0] - p0[0]) * (py - p0[1])) / det;
                    let l2 = ((p1[0] - p0[0]) * (py - p0[1]) - (px - p0[0]) * (p1[1] - p0[1])) / det;
                    let l0 = 1.0 - l1 - l2;
                    if l0 < eps || l1 < eps || l2 < eps {
                        continue;
                    }
                    let o = (y - t.y0) as usize * w + (x - t.x0) as usize;
                    uv[o] = [l0 * s0[0] + l1 * s1[0] + l2 * s2[0], l0 * s0[1] + l1 * s1[1] + l2 * s2[1]];
                }
            }
        }
        // Source footprint of the covered pixels.
        let (mut fx0, mut fy0, mut fx1, mut fy1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for q in uv.iter().filter(|q| !q[0].is_nan()) {
            fx0 = fx0.min(q[0]);
            fy0 = fy0.min(q[1]);
            fx1 = fx1.max(q[0]);
            fy1 = fy1.max(q[1]);
        }
        if fx0 > fx1 {
            return None;
        }
        let foot = Rect::new(
            (fx0.floor() as i32).saturating_sub(3),
            (fy0.floor() as i32).saturating_sub(3),
            (fx1.ceil() as i32).saturating_add(3),
            (fy1.ceil() as i32).saturating_add(3),
        )
        .intersect(&src_rect);
        // Sparse components far apart in the source can land in one tile: the dense window
        // between them is not read (the tile stays empty). The pixel-area cap is checked first:
        // `has_tiles_in` walks the source tile grid, so an oversized footprint must return
        // without paying for that scan.
        if foot.is_empty() || foot.size().area() > MAX_FOOTPRINT_PIXELS || !src.has_tiles_in(foot) {
            return None;
        }
        let mut px = src.read_region(foot);
        for p in px.chunks_exact_mut(n) {
            let al = p[a];
            for v in &mut p[..a] {
                *v *= al;
            }
        }
        let fw = foot.width() as usize;
        let at = |x: i32, y: i32, c: usize| -> f64 {
            if x < foot.x0 || y < foot.y0 || x >= foot.x1 || y >= foot.y1 {
                0.0
            } else {
                f64::from(px[((y - foot.y0) as usize * fw + (x - foot.x0) as usize) * n + c])
            }
        };
        let mut outp = vec![0.0f32; w * h * n];
        let mut any = false;
        let mut acc = [0.0f64; 8];
        for (o, q) in uv.iter().enumerate() {
            if q[0].is_nan() {
                continue;
            }
            let (u, v) = (q[0] - 0.5, q[1] - 0.5);
            acc[..n].fill(0.0);
            match interp {
                Interp::Nearest => {
                    let (ix, iy) = ((u + 0.5).floor() as i32, (v + 0.5).floor() as i32);
                    for (c, s) in acc.iter_mut().enumerate().take(n) {
                        *s = at(ix, iy, c);
                    }
                }
                Interp::Bilinear => {
                    let (ix, iy) = (u.floor() as i32, v.floor() as i32);
                    let (fx, fy) = (u - f64::from(ix), v - f64::from(iy));
                    for (dy, wy) in [(0, 1.0 - fy), (1, fy)] {
                        for (dx, wx) in [(0, 1.0 - fx), (1, fx)] {
                            for (c, s) in acc.iter_mut().enumerate().take(n) {
                                *s += at(ix + dx, iy + dy, c) * wx * wy;
                            }
                        }
                    }
                }
                Interp::Bicubic => {
                    // Snap to the pixel grid when within float noise, so identity is exact.
                    let snap = |t: f64| if (t - t.round()).abs() < 1e-7 { t.round() } else { t };
                    let (u, v) = (snap(u), snap(v));
                    let (ix, iy) = (u.floor() as i32, v.floor() as i32);
                    let (wx, wy) = (crate::transform::bicubic(u - f64::from(ix)), crate::transform::bicubic(v - f64::from(iy)));
                    for (j, wyj) in wy.iter().enumerate() {
                        for (i, wxi) in wx.iter().enumerate() {
                            let k = wxi * wyj;
                            if k == 0.0 {
                                continue;
                            }
                            for (c, s) in acc.iter_mut().enumerate().take(n) {
                                *s += at(ix - 1 + i as i32, iy - 1 + j as i32, c) * k;
                            }
                        }
                    }
                }
            }
            let al = acc[a].clamp(0.0, 1.0);
            // Float noise (a vertex a hair off the pixel grid) must not leave invisible
            // colour behind in transparent pixels.
            if al <= 1e-6 {
                continue;
            }
            any = true;
            let b = o * n;
            for c in 0..a {
                outp[b + c] = (acc[c] / al).clamp(0.0, 1.0) as f32;
            }
            outp[b + a] = al as f32;
        }
        any.then_some((*t, outp))
    };
    #[cfg(not(target_arch = "wasm32"))]
    let done: Vec<(Rect, Vec<f32>)> = {
        use rayon::prelude::*;
        tiles.par_iter().zip(bins.par_iter()).filter_map(work).collect()
    };
    #[cfg(target_arch = "wasm32")]
    let done: Vec<(Rect, Vec<f32>)> = tiles.iter().zip(bins.iter()).filter_map(work).collect();
    for (r, v) in done {
        out.write_region(r, &v);
    }
    out.prune();
    out
}

/// [`warp_mesh_surface`] for single-channel surfaces whose outside reads as their default value
/// (layer masks, selections): the content is warped with alpha and flattened back onto the
/// default.
pub fn warp_mesh_gray(s: &Surface, f: &(dyn Fn(f64, f64) -> (f64, f64) + Sync), interp: Interp) -> Surface {
    let default = s.default_pixel().first().copied().unwrap_or(0.0);
    let fmt = s.format();
    let src = s.content_bounds();
    let mut out = Surface::with_default(fmt, &[default]);
    if src.is_empty() {
        return out;
    }
    let with_alpha = PixelFormat::new(fmt.mode, fmt.sample, true);
    let mut tmp = Surface::new(with_alpha);
    let v = s.read_region(src);
    tmp.write_region(src, &v.iter().flat_map(|g| [*g, 1.0]).collect::<Vec<f32>>());
    let w = warp_mesh_surface(&tmp, src, f, interp);
    let cover = src.union(&w.content_bounds());
    out.write_region(cover, &vec![default; cover.width() as usize * cover.height() as usize]);
    let b = w.content_bounds();
    if !b.is_empty() {
        let px = w.read_region(b);
        let flat: Vec<f32> = px.as_chunks::<2>().0.iter().map(|p| p[0] * p[1] + default * (1.0 - p[1])).collect();
        out.write_region(b, &flat);
    }
    out.prune();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::{ColorMode, SampleType};
    use photocraft_geom::warp::{BezierMesh, Warp, WarpStyle};

    #[test]
    fn a_thin_layer_stretched_into_a_curve_keeps_the_curve() {
        // A 3 px wide column stretched 100x and bent into an arch. A grid sized in source pixels
        // (one cell across) turned the arch into a straight band.
        let mut src = Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::U8, true));
        src.fill_rect(Rect::new(0, 0, 3, 40), &[1.0, 0.0, 0.0, 1.0]);
        let arch = |x: f64, y: f64| {
            let dx = x * 100.0;
            let t = (dx - 150.0) / 150.0;
            (dx, y + 40.0 * (1.0 - t * t))
        };
        let out = warp_mesh_surface(&src, Rect::new(0, 0, 3, 40), &arch, Interp::Bilinear);
        let alpha = |x: i32, y: i32| {
            let mut p = [0.0f32; 4];
            out.read_pixel(x, y, &mut p);
            p[3]
        };
        // The band follows the arch: 40 px lower in the middle, about 36 px a third of the way in.
        assert!(alpha(150, 60) > 0.99 && alpha(150, 20) < 0.01, "mid {} {}", alpha(150, 60), alpha(150, 20));
        assert!(alpha(100, 55) > 0.99 && alpha(100, 20) < 0.01, "third {} {}", alpha(100, 55), alpha(100, 20));
    }

    fn sample(s: SampleType) -> Surface {
        let mut surf = Surface::new(PixelFormat::new(ColorMode::Rgb, s, true));
        for y in 0..24 {
            for x in 0..40 {
                let v = ((x * 7 + y * 13) % 17) as f32 / 16.0;
                surf.write_pixel(x + 5, y + 3, &[v, 1.0 - v, (x as f32) / 40.0, 1.0]);
            }
        }
        surf
    }

    #[test]
    fn a_mesh_flung_across_the_coordinate_range_gets_no_per_tile_bookkeeping() {
        // About 8000 × 8000 destination tiles from three vertices.
        let src = sample(SampleType::U8);
        let far = 1_000_000.0;
        let verts = [([-far, -far], [0.0, 0.0]), ([far, -far], [40.0, 0.0]), ([0.0, far], [0.0, 24.0])];
        let t = std::time::Instant::now();
        let out = warp_triangles(&src, Rect::new(0, 0, 40, 24), &verts, &[[0, 1, 2]], Interp::Nearest);
        assert!(out.content_bounds().is_empty());
        assert!(t.elapsed().as_secs() < 5);
    }

    #[test]
    fn identity_warp_is_a_no_op_at_every_depth() {
        for s in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let src = sample(s);
            let r = src.content_bounds();
            let b = [f64::from(r.x0), f64::from(r.y0), f64::from(r.x1), f64::from(r.y1)];
            for w in [Warp::none(b), Warp::custom(BezierMesh::identity(b, 1, 1), b), Warp::preset(WarpStyle::Arc, 0.0, b)] {
                for interp in [Interp::Nearest, Interp::Bilinear, Interp::Bicubic] {
                    let out = warp_mesh_surface(&src, r, &|x, y| w.map(x, y), interp);
                    assert_eq!(out.content_bounds(), r, "{s:?} {interp:?}");
                    let (a, o) = (src.read_region(r), out.read_region(r));
                    let worst = a.iter().zip(&o).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
                    assert!(worst <= 1.0 / 255.0, "{s:?} {interp:?}: {worst}");
                }
            }
        }
    }

    #[test]
    fn translation_and_known_arc_points() {
        let src = sample(SampleType::U8);
        let r = src.content_bounds();
        let out = warp_mesh_surface(&src, r, &|x, y| (x + 10.0, y - 2.0), Interp::Bilinear);
        assert_eq!(out.content_bounds(), r.translate(10, -2));
        assert_eq!(out.pixel(20, 10), src.pixel(10, 12));
        // Arc 50 %: the centre column keeps its pixels, the ends drop.
        let b = [f64::from(r.x0), f64::from(r.y0), f64::from(r.x1), f64::from(r.y1)];
        let w = Warp::preset(WarpStyle::Arc, 50.0, b);
        let arc = warp_mesh_surface(&src, r, &|x, y| w.map(x, y), Interp::Bilinear);
        let cx = (r.x0 + r.x1) / 2;
        let (p, q) = (arc.pixel(cx, 10), src.pixel(cx, 10));
        assert!(p.iter().zip(&q).all(|(a, b)| (a - b).abs() < 0.05), "{p:?} vs {q:?}");
        let (ex, ey) = w.map(f64::from(r.x0) + 2.5, f64::from(r.y0) + 2.5);
        let e = arc.pixel(ex.floor() as i32, ey.floor() as i32);
        assert!(e[3] > 0.2, "a point near the top-left corner lands at its mapped position: {e:?}");
        assert!(arc.content_bounds().y1 > r.y1 + 2, "ends drop below the original box");
    }

    #[test]
    fn gray_masks_keep_their_default() {
        let mut m = Surface::with_default(PixelFormat::GRAY8, &[1.0]);
        m.fill_rect(Rect::new(4, 4, 10, 10), &[0.0]);
        let o = warp_mesh_gray(&m, &|x, y| (x + 5.0, y), Interp::Bilinear);
        assert_eq!(o.pixel(0, 0), vec![1.0]);
        assert_eq!(o.pixel(12, 6), vec![0.0]);
        assert_eq!(o.pixel(5, 6), vec![1.0]);
    }
}
