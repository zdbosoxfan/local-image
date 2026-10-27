//! Edge refinement ("Select and Mask" / Refine Edge) and colour decontamination.
//!
//! * Soft edges come from the **guided filter** (K. He, J. Sun and X. Tang, "Guided Image
//!   Filtering", ECCV 2010 / PAMI 2013): filtering the binary selection with the image as guide
//!   fits a local linear model `α ≈ aᵀI + b` in every window, so the mask takes on the image's
//!   soft transitions (hair, blur) inside a band of `radius` pixels around the selection edge.
//!   "Smart radius" adapts the band per pixel to the transition width observed in the image.
//! * Then, in Photoshop's order: Smooth, Feather (Gaussian, σ = feather / 2), Contrast and
//!   Shift Edge (grey dilation / erosion).
//! * **Decontaminate colours** replaces fringe colours by the normalised-convolution average of
//!   nearby fully selected pixels, blended by `amount` and the fringe's transparency.
//!
//! Work is tiled: only tiles near the selection boundary are processed (in parallel on native
//! targets), so cost scales with the boundary length rather than the selection area.

use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::segment::{RgbImage, Sampler};
use crate::selection::Region;

// The editor keeps only document/surface adapters; numerical work lives in li-matte.
use li_matte::matting::{RefineParams, box_mean};
const TILE: i32 = 256;

fn guide(img: &RgbImage) -> li_matte::RgbImage<'_> {
    li_matte::RgbImage::borrowed(img.w, img.h, &img.px)
}

pub fn guided_filter_color(img: &RgbImage, p: &[f32], r: usize, eps: f32) -> Vec<f32> {
    li_matte::matting::guided_filter_color(&guide(img), p, r, eps)
}

pub fn edge_width(img: &RgbImage, r: usize) -> Vec<f32> {
    li_matte::matting::edge_width(&guide(img), r)
}

pub fn refine_buffer(img: Option<&RgbImage>, m: &[f32], w: usize, h: usize, p: &RefineParams) -> Vec<f32> {
    let img = img.map(guide);
    li_matte::matting::refine_buffer(img.as_ref(), m, w, h, p)
}

/// Most pixels one refine should cover. Its result is a dense [`Region`] over [`refine_area`]
/// (about two bytes a pixel while it is assembled), so callers refuse a larger refine up front
/// rather than allocate it: 2³⁰ pixels, a 32k × 32k selection.
pub const MAX_REFINE_AREA: u64 = 1 << 30;

/// The area [`refine_mask`] computes and allocates its result over: `content` grown by how far
/// `p` can spread it, within `canvas`. Empty when `content` misses the canvas.
pub fn refine_area(content: Rect, canvas: Rect, p: &RefineParams) -> Rect {
    let content = content.intersect(&canvas);
    if content.is_empty() {
        return Rect::EMPTY;
    }
    content.inflate(p.spread()).intersect(&canvas)
}

/// Refines a selection mask. `mask(r)` returns coverage over `r` (row-major); `content` bounds
/// its non-zero pixels; the result is clipped to `canvas`. `sampler` supplies the guide image
/// (only read when `radius > 0`). Returns `None` when nothing remains selected.
pub fn refine_mask(sampler: &dyn Sampler, mask: &(dyn Fn(Rect) -> Vec<f32> + Sync), content: Rect, canvas: Rect, p: &RefineParams) -> Option<Region> {
    let bbox = refine_area(content, canvas, p);
    if bbox.is_empty() {
        return None;
    }
    let halo = p.halo();
    let mut tiles = Vec::new();
    let mut y = bbox.y0;
    while y < bbox.y1 {
        let mut x = bbox.x0;
        while x < bbox.x1 {
            tiles.push(Rect::new(x, y, (x + TILE).min(bbox.x1), (y + TILE).min(bbox.y1)));
            x += TILE;
        }
        y += TILE;
    }
    let run = |t: &Rect| -> (Rect, Vec<u8>) {
        let r = t.inflate(halo).intersect(&canvas);
        let m = mask(r);
        let (rw, rh) = (r.width() as usize, r.height() as usize);
        let first = m.first().copied().unwrap_or(0.0);
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let (cx, cy, cw, ch) = ((t.x0 - r.x0) as usize, (t.y0 - r.y0) as usize, t.width() as usize, t.height() as usize);
        let core = |buf: &[f32]| -> Vec<u8> {
            let mut out = Vec::with_capacity(cw * ch);
            for yy in 0..ch {
                out.extend(buf[(cy + yy) * rw + cx..(cy + yy) * rw + cx + cw].iter().map(|v| q(*v)));
            }
            out
        };
        if (first == 0.0 || first == 1.0) && m.iter().all(|v| *v == first) {
            return (*t, vec![q(first); cw * ch]);
        }
        let img = p.uses_image().then(|| sampler.rgb(r));
        let out = refine_buffer(img.as_ref(), &m, rw, rh, p);
        (*t, core(&out))
    };
    #[cfg(not(target_arch = "wasm32"))]
    let results: Vec<(Rect, Vec<u8>)> = {
        use rayon::prelude::*;
        tiles.par_iter().map(run).collect()
    };
    #[cfg(target_arch = "wasm32")]
    let results: Vec<(Rect, Vec<u8>)> = tiles.iter().map(run).collect();
    let bw = bbox.width() as usize;
    let mut out = vec![0u8; bw * bbox.height() as usize];
    for (t, data) in results {
        let cw = t.width() as usize;
        for (yy, row) in data.chunks_exact(cw).enumerate() {
            let o = ((t.y0 - bbox.y0) as usize + yy) * bw + (t.x0 - bbox.x0) as usize;
            out[o..o + cw].copy_from_slice(row);
        }
    }
    crate::segment::trim_region(Region { bbox, mask: out })
}

/// A mask reader over a [`Region`] (zero outside its box).
pub fn region_reader(reg: &Region) -> impl Fn(Rect) -> Vec<f32> + Sync + '_ {
    move |r: Rect| {
        let mut v = Vec::with_capacity(r.width() as usize * r.height() as usize);
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                v.push(reg.at(x, y));
            }
        }
        v
    }
}

/// A mask reader over a selection surface (channel 0).
pub fn surface_reader(s: &Surface) -> impl Fn(Rect) -> Vec<f32> + Sync + '_ {
    move |r: Rect| crate::selection::mask_from_surface(Some(s), r)
}

/// GRAY8 surface (default 0) holding a region's coverage (a selection or a layer mask).
pub fn region_surface(reg: &Region) -> Surface {
    let mut s = Surface::from_interleaved(PixelFormat::GRAY8, reg.bbox, &reg.mask);
    s.prune();
    s
}

fn tiles_of(area: Rect) -> Vec<Rect> {
    let mut tiles = Vec::new();
    let mut y = area.y0;
    while y < area.y1 {
        let mut x = area.x0;
        while x < area.x1 {
            tiles.push(Rect::new(x, y, (x + TILE).min(area.x1), (y + TILE).min(area.y1)));
            x += TILE;
        }
        y += TILE;
    }
    tiles
}

fn par_tiles<T: Send>(tiles: &[Rect], f: impl Fn(&Rect) -> Option<T> + Sync + Send) -> Vec<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        tiles.par_iter().filter_map(f).collect()
    }
    #[cfg(target_arch = "wasm32")]
    {
        tiles.iter().filter_map(f).collect()
    }
}

/// Decontaminates colours in the soft fringe of `alpha`: each fringe pixel moves towards the
/// average colour of nearby fully selected pixels (normalised convolution with a box of radius
/// `max(radius, 3)`, widened ×4 where no such pixel is near), by `amount`% scaled by its
/// transparency. Works in the surface's own colour channels (any model / depth).
pub fn decontaminate(src: &Surface, alpha: &Region, radius: f32, amount: f32) -> Surface {
    let fmt = src.format();
    let n = fmt.channels();
    let nc = n - fmt.alpha as usize;
    let rad = radius.max(3.0).ceil() as usize;
    let halo = (rad * 4) as i32;
    let amount = amount.clamp(0.0, 100.0) / 100.0;
    let soft = |a: f32| a > 0.02 && a < 0.98;
    let results = par_tiles(&tiles_of(alpha.bbox), |t| {
        if !(t.y0..t.y1).any(|y| (t.x0..t.x1).any(|x| soft(alpha.at(x, y)))) {
            return None;
        }
        let r = t.inflate(halo);
        let (rw, rh) = (r.width() as usize, r.height() as usize);
        let mut px = Vec::new();
        src.read_region_into(r, &mut px);
        let al: Vec<f32> = (r.y0..r.y1).flat_map(|y| (r.x0..r.x1).map(move |x| alpha.at(x, y))).collect();
        let wts: Vec<f32> = (0..rw * rh).map(|i| if al[i] >= 0.98 { if fmt.alpha { px[i * n + n - 1] } else { 1.0 } } else { 0.0 }).collect();
        let den = box_mean(&wts, rw, rh, rad);
        let den2 = box_mean(&wts, rw, rh, rad * 4);
        let mut nums = Vec::with_capacity(nc);
        for c in 0..nc {
            let v: Vec<f32> = (0..rw * rh).map(|i| wts[i] * px[i * n + c]).collect();
            nums.push((box_mean(&v, rw, rh, rad), box_mean(&v, rw, rh, rad * 4)));
        }
        let (cx, cy, cw, ch) = ((t.x0 - r.x0) as usize, (t.y0 - r.y0) as usize, t.width() as usize, t.height() as usize);
        let mut out = Vec::with_capacity(cw * ch * n);
        for yy in 0..ch {
            for xx in 0..cw {
                let i = (cy + yy) * rw + cx + xx;
                let a = al[i];
                let k = if soft(a) { amount * ((1.0 - a) * 4.0).clamp(0.0, 1.0) } else { 0.0 };
                for c in 0..n {
                    let v = px[i * n + c];
                    if c >= nc || k == 0.0 {
                        out.push(v);
                        continue;
                    }
                    let f = if den[i] > 1e-4 {
                        nums[c].0[i] / den[i]
                    } else if den2[i] > 1e-4 {
                        nums[c].1[i] / den2[i]
                    } else {
                        v
                    };
                    out.push(v + (f - v) * k);
                }
            }
        }
        Some((*t, out))
    });
    let mut out = src.clone();
    for (t, data) in results {
        out.write_region(t, &data);
    }
    out
}

/// The pixels of `src` with their alpha multiplied by `alpha` (zero outside its box); the result
/// has an alpha channel even if `src` does not.
pub fn masked_copy(src: &Surface, alpha: &Region) -> Surface {
    let fmt = src.format();
    let n = fmt.channels();
    let out_fmt = PixelFormat { alpha: true, ..fmt };
    let m = out_fmt.channels();
    let results = par_tiles(&tiles_of(alpha.bbox), |t| {
        let mut px = Vec::new();
        src.read_region_into(*t, &mut px);
        let mut out = Vec::with_capacity(t.width() as usize * t.height() as usize * m);
        let mut any = false;
        let mut i = 0;
        for y in t.y0..t.y1 {
            for x in t.x0..t.x1 {
                let a = alpha.at(x, y);
                let p = &px[i * n..(i + 1) * n];
                out.extend_from_slice(&p[..m - 1]);
                let sa = if fmt.alpha { p[n - 1] } else { 1.0 };
                out.push(sa * a);
                any |= sa * a > 0.0;
                i += 1;
            }
        }
        any.then_some((*t, out))
    });
    let mut out = Surface::new(out_fmt);
    for (t, data) in results {
        out.write_region(t, &data);
    }
    out.prune();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::ImageSampler;
    use li_matte::matting::{EPS, guided_filter_gray};
    use photocraft_color::SampleType;

    #[test]
    fn editor_settings_and_zero_copy_adapter_parity() {
        let p: RefineParams = serde_json::from_str(r#"{"radius":6.0,"smartRadius":true,"feather":0.5,"shiftEdge":-25.0}"#).unwrap();
        assert_eq!(p.smooth, 0.0);
        assert_eq!(p.contrast, 0.0);
        assert_eq!(serde_json::from_str::<RefineParams>(&serde_json::to_string(&p).unwrap()).unwrap(), p);
        let (w, h) = (71, 53);
        let img = RgbImage::from_fn(w, h, |x, y| [x as f32 / w as f32, y as f32 / h as f32, 0.3]);
        let m: Vec<f32> = (0..w * h).map(|i| if i % w < 35 { 1.0 } else { 0.0 }).collect();
        let view = guide(&img);
        assert_eq!(view.px.as_ptr(), img.px.as_ptr());
        assert_eq!(refine_buffer(Some(&img), &m, w, h, &p), li_matte::matting::refine_buffer(Some(&view), &m, w, h, &p));
        let tri = li_matte::cf_matting::trimap_from_mask(&m, w, h, 3);
        assert_eq!(
            crate::cf_matting::closed_form_matting_tiled(&img, &tri, &Default::default(), 32, 8),
            li_matte::cf_matting::closed_form_matting_tiled(&view, &tri, &Default::default(), 32, 8)
        );
        assert_eq!(refine_buffer(None, &m, w, h, &Default::default()), m);
    }

    #[test]
    fn editor_buffer_goldens() {
        let (w, h) = (39, 23);
        let img = RgbImage::from_fn(w, h, |x, y| [x as f32 / w as f32, y as f32 / h as f32, ((x * 13 + y * 7) % 31) as f32 / 31.0]);
        let m: Vec<f32> = (0..w * h).map(|i| if i % w < 19 { 1.0 } else { 0.0 }).collect();
        let hash = |v: &[f32]| v.iter().fold(0xcbf29ce484222325u64, |h, v| (h ^ u64::from(v.to_bits())).wrapping_mul(0x100000001b3));
        let guide: Vec<f32> = img.px.iter().map(|p| p[0]).collect();
        let params = RefineParams { radius: 4.0, smart_radius: true, smooth: 23.0, feather: 1.0, contrast: 8.0, shift_edge: -20.0 };
        let tri = li_matte::cf_matting::trimap_from_mask(&m, w, h, 3);
        let alpha = crate::cf_matting::closed_form_matting(&img, &tri, &Default::default());
        let fg = crate::cf_matting::estimate_foreground(&img, &alpha);
        let actual = [
            hash(&guided_filter_gray(&guide, &m, w, h, 3, EPS)),
            hash(&guided_filter_color(&img, &m, 3, EPS)),
            hash(&refine_buffer(Some(&img), &m, w, h, &params)),
            hash(&alpha),
            hash(&fg.px.iter().flatten().copied().collect::<Vec<_>>()),
        ];
        assert_eq!(actual, [415457246788219432, 2673089081770112874, 4217930111347877778, 7289351520849440030, 6249743762746608584]);
    }

    /// Horizontal blend from `a` (left) to `b` (right) across a Gaussian-blurred edge at x = 40.
    fn blurred_edge(w: usize, h: usize, sigma: f32) -> (RgbImage, Vec<f32>) {
        let (a, b) = ([0.9f32, 0.8, 0.2], [0.1f32, 0.2, 0.6]);
        // True foreground fraction (a = foreground) = 1 − Φ((x − 40) / σ).
        let erf = |x: f32| {
            let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
            let y = 1.0 - (((((1.061_405_4 * t - 1.453_152_1) * t) + 1.421_413_7) * t - 0.284_496_74) * t + 0.254_829_6) * t * (-x * x).exp();
            if x >= 0.0 { y } else { -y }
        };
        let alpha: Vec<f32> = (0..w).map(|x| 0.5 * (1.0 - erf((x as f32 + 0.5 - 40.0) / (sigma * std::f32::consts::SQRT_2)))).collect();
        let img = RgbImage::from_fn(w, h, |x, _| [0, 1, 2].map(|c| a[c] * alpha[x] + b[c] * (1.0 - alpha[x])));
        (img, alpha)
    }

    #[test]
    fn box_mean_matches_naive() {
        let (w, h) = (13, 9);
        let src: Vec<f32> = (0..w * h).map(|i| ((i * 7) % 11) as f32).collect();
        let fast = box_mean(&src, w, h, 2);
        for y in 0..h {
            for x in 0..w {
                let (mut s, mut n) = (0.0, 0.0);
                for yy in y.saturating_sub(2)..(y + 3).min(h) {
                    for xx in x.saturating_sub(2)..(x + 3).min(w) {
                        s += src[yy * w + xx];
                        n += 1.0;
                    }
                }
                assert!((fast[y * w + x] - s / n).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn guided_filter_matting_softens_blurred_edge() {
        let (w, h) = (80usize, 20usize);
        let (img, truth) = blurred_edge(w, h, 3.0);
        let m: Vec<f32> = (0..w * h).map(|i| if (i % w) < 40 { 1.0 } else { 0.0 }).collect();
        let p = RefineParams { radius: 10.0, ..Default::default() };
        let a = refine_buffer(Some(&img), &m, w, h, &p);
        let row = &a[10 * w..11 * w];
        let soft = row.iter().filter(|v| **v > 0.05 && **v < 0.95).count();
        assert!(soft >= 6, "only {soft} soft pixels: {row:?}");
        for x in 0..w {
            assert!((row[x] - truth[x]).abs() < 0.12, "x={x}: {} vs {}", row[x], truth[x]);
            if x > 0 {
                assert!(row[x] <= row[x - 1] + 0.02, "not monotone at {x}");
            }
        }
        // Outside the band the mask is untouched.
        assert_eq!(row[5], 1.0);
        assert_eq!(row[75], 0.0);
        // Smart radius keeps a sharp edge sharp.
        let sharp = RgbImage::from_fn(w, h, |x, _| if x < 40 { [0.9, 0.8, 0.2] } else { [0.1, 0.2, 0.6] });
        let s = refine_buffer(Some(&sharp), &m, w, h, &RefineParams { radius: 10.0, smart_radius: true, ..Default::default() });
        let soft = s[10 * w..11 * w].iter().filter(|v| **v > 0.05 && **v < 0.95).count();
        assert!(soft <= 2, "sharp edge became soft: {soft}");
    }

    #[test]
    fn feather_contrast_and_shift() {
        let (w, h) = (60usize, 10usize);
        let m: Vec<f32> = (0..w * h).map(|i| if (i % w) < 30 { 1.0 } else { 0.0 }).collect();
        let f = refine_buffer(None, &m, w, h, &RefineParams { feather: 6.0, ..Default::default() });
        assert!(f[5 * w + 29] > 0.5 && f[5 * w + 29] < 0.8 && f[5 * w + 31] > 0.1);
        let c = refine_buffer(None, &f, w, h, &RefineParams { contrast: 100.0, ..Default::default() });
        assert!(c[5 * w + 29] == 1.0 && c[5 * w + 31] == 0.0);
        let grown = refine_buffer(None, &m, w, h, &RefineParams { shift_edge: 100.0, ..Default::default() });
        assert_eq!(grown[5 * w + 33], 1.0);
        assert_eq!(grown[5 * w + 34], 0.0);
        let shrunk = refine_buffer(None, &m, w, h, &RefineParams { shift_edge: -50.0, ..Default::default() });
        assert_eq!(shrunk[5 * w + 27], 1.0);
        assert_eq!(shrunk[5 * w + 28], 0.0);
        let sm = refine_buffer(None, &m, w, h, &RefineParams { smooth: 50.0, ..Default::default() });
        assert!(sm[5 * w + 10] == 1.0 && sm[5 * w + 50] == 0.0);
    }

    #[test]
    fn refine_mask_tiles_match_single_buffer() {
        // Tiled processing gives the same result as processing the whole image at once.
        let (w, h) = (600usize, 300usize);
        let (img, _) = blurred_edge(w, h, 2.0);
        let s = ImageSampler { img: &img, origin: (0, 0) };
        let mut mask = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 - 40.0, y as f32 - 150.0);
                mask[y * w + x] = if dx * dx * 0.05 + dy * dy < 120.0 * 120.0 || x < 40 { 255 } else { 0 };
            }
        }
        let reg = Region { bbox: Rect::new(0, 0, w as i32, h as i32), mask };
        let p = RefineParams { radius: 6.0, feather: 2.0, contrast: 10.0, ..Default::default() };
        let canvas = reg.bbox;
        let out = refine_mask(&s, &region_reader(&reg), canvas, canvas, &p).unwrap();
        let whole: Vec<f32> = reg.mask.iter().map(|v| *v as f32 / 255.0).collect();
        let full = refine_buffer(Some(&img), &whole, w, h, &p);
        let mut maxd = 0.0f32;
        for y in 0..h {
            for x in 0..w {
                maxd = maxd.max((out.at(x as i32, y as i32) - full[y * w + x]).abs());
            }
        }
        assert!(maxd <= 1.5 / 255.0, "tiles differ by {maxd}");
    }

    #[test]
    fn decontaminate_and_masked_copy_any_depth() {
        for sample in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let fmt = PixelFormat::RGBA8.with_sample(sample);
            let mut s = Surface::new(fmt);
            // Foreground red on the left, background blue on the right; fringe pixel at x = 10 is a mix.
            s.fill_rect(Rect::new(0, 0, 10, 8), &[1.0, 0.0, 0.0, 1.0]);
            s.fill_rect(Rect::new(10, 0, 20, 8), &[0.5, 0.0, 0.5, 1.0]);
            s.fill_rect(Rect::new(11, 0, 20, 8), &[0.0, 0.0, 1.0, 1.0]);
            let mask: Vec<u8> = (0..20 * 8)
                .map(|i| match i % 20 {
                    x if x < 10 => 255,
                    10 => 128,
                    _ => 0,
                })
                .collect();
            let a = Region { bbox: Rect::new(0, 0, 20, 8), mask };
            let d = decontaminate(&s, &a, 3.0, 100.0);
            let p = d.pixel(10, 4);
            assert!(p[0] > 0.9 && p[2] < 0.1, "{sample:?}: {p:?}");
            assert_eq!(d.pixel(15, 4), s.pixel(15, 4));
            let c = masked_copy(&d, &a);
            assert_eq!(c.format(), fmt);
            assert!((c.pixel(10, 4)[3] - 128.0 / 255.0).abs() < 0.01);
            assert_eq!(c.pixel(15, 4)[3], 0.0);
            assert_eq!(c.pixel(3, 3)[0], 1.0);
        }
        // Sources without alpha gain an alpha channel.
        let mut g = Surface::new(PixelFormat::GRAY8);
        g.fill_rect(Rect::new(0, 0, 4, 4), &[0.5]);
        let a = Region { bbox: Rect::new(0, 0, 2, 4), mask: vec![255; 8] };
        let c = masked_copy(&g, &a);
        assert_eq!(c.format(), PixelFormat::GRAYA8);
        assert_eq!(c.pixel(3, 1)[1], 0.0);
        assert!((c.pixel(1, 1)[0] - 0.5).abs() < 0.01 && c.pixel(1, 1)[1] == 1.0);
    }
}
