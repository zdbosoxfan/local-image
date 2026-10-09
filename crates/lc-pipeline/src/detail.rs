//! Process version 2026's detail tools ([`DevelopSettings::process`]), with Lightroom's Detail
//! panel semantics:
//!
//! - **Sharpening** (Amount 0–150, Radius 0.5–3 px, Detail 0–100, Masking 0–100): an unsharp
//!   mask on log luminance at the photo's own pixel scale. Detail moves it from a halo-controlled
//!   unsharp mask (the overshoot past the local 3 × 3 range is damped, as RawTherapee's "halo
//!   control") towards a deconvolution-like term (two Van Cittert iterations: more weight on the
//!   finest frequencies). Masking keeps it to edges: a mask from the smoothed gradient magnitude.
//!   Texture keeps its own, wider band (`local`).
//! - **Noise reduction** (luminance: Amount, Detail, Contrast; colour: Amount, Detail,
//!   Smoothness): in a variance-stabilised space, √Y and the opponent pair √r − √g, √b − √g (the
//!   square root is the core of the Anscombe transform: shot noise becomes about as strong in the
//!   shadows as in the highlights, so one threshold per scale fits every tone). Each channel is
//!   split into à-trous wavelet levels (B3 spline, scales 1 … 64 px, like darktable's and
//!   RawTherapee's wavelet denoisers) and every level's coefficients are shrunk (non-negative
//!   garrote) against the noise measured on the photo itself (median absolute deviation per
//!   level and channel on a grid of tiles, so no camera profile is needed and previews, whose
//!   noise is averaged, are measured as they are). Detail lowers the finest levels' thresholds,
//!   Contrast the coarser luminance levels' (local contrast survives); Smoothness adds coarse
//!   chroma levels for low-frequency blotches. Chroma smoothing eases where luminance has an
//!   edge at the same scale, and large chroma coefficients (colour edges) are kept, so colour
//!   does not bleed across edges.
//! - **Dehaze**: the dark channel prior (He, Sun & Tang 2009) with a min-filter patch, a coloured
//!   airlight from the haziest region, the transmission refined by a guided filter (He et al.
//!   2010) so it follows the photo's edges (no halos), a transmission floor and highlight
//!   protection; negative amounts add haze with the same model.
//!
//! All of it is written from the published papers; no third-party code. Legacy settings keep the
//! earlier formulas (`local`, `finish`) bit for bit. Every function here has a WGSL twin in
//! `lightcraft-gpu` (`map.wgsl`, `blur.wgsl`, `finish.wgsl`); keep them in step.

use lightcraft_color::luminance_2020;
use lightcraft_develop::DevelopSettings;
use lightcraft_raster::blur::{gaussian, gaussian_fine, min_filter};
use lightcraft_raster::resample::{Filter, Pixel, resize};
use lightcraft_raster::{Image, Plane, Rgb32f, par_rows};

use crate::for_rows;


#[inline]
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Output pixels per pixel of the original photo: under 1 for a downscaled preview, and a binned
/// raw proxy (sensor scale > 1) counts too. Pixel-sized tools (the sharpening radius, noise grain)
/// scale by it, so a preview shows what the full-size render shows at that size.
pub fn out_per_orig(px_per_long: f64, src_long: usize, sensor_scale: f32) -> f32 {
    (px_per_long as f32 / src_long.max(1) as f32) / sensor_scale.max(1e-3)
}

// ---------------------------------------------------------------------------------------------
// Sharpening

/// Under this blur radius (output px) sharpening has nothing left to act on (a preview much
/// smaller than the photo, where the full-size result would not show either) and is skipped.
pub const SHARP_MIN_SIGMA: f32 = 0.1;

/// Blur radius (output px) of the sharpening planes: the Radius slider, in original pixels.
pub fn sharp_sigma(s: &DevelopSettings, out_per_orig: f32) -> f32 {
    s.detail.sharpen_radius.clamp(0.5, 3.0) as f32 * out_per_orig
}

/// The blurred planes sharpening reads: `log_l` blurred once and twice by `sigma` (true Gaussian
/// kernels: the radii are a pixel or two).
pub fn sharp_planes(log_l: &Plane, sigma: f32) -> (Plane, Plane) {
    let b1 = gaussian_fine(log_l, sigma);
    let b2 = gaussian_fine(&b1, sigma);
    (b1, b2)
}

/// Per-pixel constants of sharpening (the photo's, or a develop layer's).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SharpK {
    /// Detail (0..1): mix of the deconvolution-like term into the unsharp mask.
    pub detail: f32,
    /// Share of the overshoot past the local 3 × 3 range that is kept (halo control: 0.15 at
    /// Detail 0, 1 = none at Detail 100).
    pub halo: f32,
    /// Masking threshold in EV of edge contrast (0: no mask).
    pub mask_t: f32,
    /// Smoothed gradient (EV / px) → edge contrast (EV), for the plane's blur.
    pub edge_k: f32,
}

impl SharpK {
    /// `detail`, `masking` as 0..1; `sigma` the planes' blur radius (output px).
    pub fn new(detail: f32, masking: f32, sigma: f32) -> SharpK {
        let d = detail.clamp(0.0, 1.0);
        let m = masking.clamp(0.0, 1.0);
        // the gradient is read from the twice-blurred plane (σ√2); a step of c EV blurred by s
        // peaks at c / (s·√(2π)) EV/px (resampling adds about half a pixel of blur)
        let sm = sigma * std::f32::consts::SQRT_2;
        SharpK {
            detail: d,
            halo: 0.15 + 0.85 * d,
            mask_t: if m > 0.0 { 0.1 * m + 1.5 * m * m } else { 0.0 },
            edge_k: 2.5 * (sm * sm + 0.25).sqrt(),
        }
    }
}

/// The Masking slider's mask at `(x, y)` of `w × h` planes: 1 on edges, 0 on smooth areas (1
/// everywhere at Masking 0). `b2` is the twice-blurred plane.
#[inline]
pub fn sharp_mask_at(b2: &[f32], w: usize, h: usize, x: usize, y: usize, k: &SharpK) -> f32 {
    if k.mask_t <= 0.0 {
        return 1.0;
    }
    let (xl, xr) = (x.saturating_sub(1), (x + 1).min(w - 1));
    let (yu, yd) = (y.saturating_sub(1), (y + 1).min(h - 1));
    let gx = (b2[y * w + xr] - b2[y * w + xl]) * 0.5;
    let gy = (b2[yd * w + x] - b2[yu * w + x]) * 0.5;
    let e = (gx * gx + gy * gy).sqrt() * k.edge_k;
    smooth(0.5 * k.mask_t, k.mask_t, e)
}

/// Sharpening's log2 gain at `(x, y)` for `amount` (Amount / 100; negative, from a local
/// Sharpness below 0, blurs towards the first blur), on log luminance `l` with its blurs `b1`,
/// `b2`.
#[allow(clippy::too_many_arguments)]
#[inline]
pub fn sharpen_at(amount: f32, k: &SharpK, l: &[f32], b1: &[f32], b2: &[f32], w: usize, h: usize, x: usize, y: usize) -> f32 {
    let i = y * w + x;
    let lc = l[i];
    let h1 = lc - b1[i];
    if amount < 0.0 {
        return amount.max(-1.0) * h1;
    }
    // unsharp mask, towards the deconvolution-like 2·h1 − blur(h1) with Detail
    let hb = b1[i] - b2[i];
    let det = h1 + k.detail * (h1 - hb);
    let (x0, x1, y0, y1) = (x.saturating_sub(1), (x + 1).min(w - 1), y.saturating_sub(1), (y + 1).min(h - 1));
    let (mut mn, mut mx) = (lc, lc);
    for yy in y0..=y1 {
        for v in &l[yy * w + x0..=yy * w + x1] {
            mn = mn.min(*v);
            mx = mx.max(*v);
        }
    }
    let mut v = lc + amount * det;
    if v > mx {
        v = mx + (v - mx) * k.halo;
    } else if v < mn {
        v = mn + (v - mn) * k.halo;
    }
    (v - lc) * sharp_mask_at(b2, w, h, x, y, k)
}

/// The Masking slider's mask as a plane (the Alt-drag preview, [`crate::Overlay::SharpenMask`]).
pub fn sharp_mask_plane(b2: &Plane, k: &SharpK) -> Plane {
    let (w, h) = (b2.width, b2.height);
    let mut out = Plane::new(w, h);
    for_rows(&mut out.data, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            *v = sharp_mask_at(&b2.data, w, h, x, y, k);
        }
    });
    out
}

// ---------------------------------------------------------------------------------------------
// Noise reduction

/// Most à-trous levels (scales 1 … 64 px).
pub const NR_MAX_LEVELS: usize = 7;

/// Standard deviation of the à-trous (B3-spline) detail levels per unit of white noise (Starck &
/// Murtagh): noise falls with scale, so thresholds do too.
pub const ATROUS_WHITE: [f32; NR_MAX_LEVELS] = [0.889, 0.200, 0.086, 0.041, 0.020, 0.010, 0.005];

/// Noise reduction parameters (process 2026), all sliders as 0..1, with the number of wavelet
/// levels each part works on at this output scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NrParams {
    pub lum: f32,
    pub detail: f32,
    pub contrast: f32,
    pub col: f32,
    pub col_detail: f32,
    /// Levels shrunk in luminance and in chroma (at full size 5, and 4 … 7 with Smoothness;
    /// fewer at preview scale, where the coarse scales of the photo are fine ones).
    pub levels_l: usize,
    pub levels_c: usize,
}

impl NrParams {
    pub fn levels(&self) -> usize {
        self.levels_l.max(self.levels_c)
    }
}

/// Noise-reduction parameters (process 2026) at `out_per_orig` (see [`out_per_orig`]); `None`
/// when both amounts are 0.
pub fn nr_params(s: &DevelopSettings, out_per_orig: f32) -> Option<NrParams> {
    let d = &s.detail;
    let f = |v: f64| (v / 100.0).clamp(0.0, 1.0) as f32;
    let (lum, col) = (f(d.nr_luminance), f(d.nr_color));
    if lum <= 0.0 && col <= 0.0 {
        return None;
    }
    // each level is twice the scale of the previous: a preview at 1/4 size drops two levels
    let drop = (1.0 / out_per_orig.clamp(1.0 / 64.0, 1.0)).log2().round() as isize;
    let lv = |full: usize| (full as isize - drop).clamp(1, NR_MAX_LEVELS as isize) as usize;
    let smooth = f(d.nr_color_smoothness);
    Some(NrParams {
        lum,
        detail: f(d.nr_detail),
        contrast: f(d.nr_contrast),
        col,
        col_detail: f(d.nr_color_detail),
        levels_l: if lum > 0.0 { lv(5) } else { 0 },
        levels_c: if col > 0.0 { lv(4 + (3.0 * smooth).round() as usize) } else { 0 },
    })
}

/// The working channels: (√Y, √r − √g, √b − √g). The square root stabilises shot-noise variance
/// (the core of the Anscombe transform: noise about as strong in the shadows as in the
/// highlights, so one threshold per level fits every tone); the opponent pair carries chroma.
#[inline]
pub fn cnr_split(c: [f32; 3]) -> [f32; 3] {
    let q = c.map(|v| v.max(0.0).sqrt());
    [luminance_2020(c).max(0.0).sqrt(), q[0] - q[1], q[2] - q[1]]
}

/// The colour of luminance `y` with opponent channels `ca`, `cb` (inverse of [`cnr_split`]:
/// √g solves the luminance equation, then the luminance is matched exactly).
#[inline]
pub fn cnr_join(y: f32, ca: f32, cb: f32) -> [f32; 3] {
    const WR: f32 = 0.2627;
    const WB: f32 = 0.0593;
    if y <= 0.0 {
        return [0.0; 3];
    }
    let b = WR * ca + WB * cb;
    let c = WR * ca * ca + WB * cb * cb - y;
    let sg = (-b + (b * b - c).max(0.0).sqrt()).max(0.0);
    let q = [(sg + ca).max(0.0), sg, (sg + cb).max(0.0)];
    let rgb = q.map(|v| v * v);
    let y1 = luminance_2020(rgb);
    if y1 > 1e-12 { rgb.map(|v| v * (y / y1)) } else { [y; 3] }
}

/// One à-trous step: the B3-spline kernel `[1 4 6 4 1] / 16` with holes, taps `2^level` px apart
/// (separable, clamped edges). Summation order: centre, ±1 tap, ±2 taps (the GPU twin's).
pub fn atrous<T: Pixel>(img: &Image<T>, level: usize) -> Image<T> {
    let (w, h) = (img.width, img.height);
    let s = 1isize << level;
    let tap = |acc: T, a: T, b: T, k: f32| acc.madd(a, k).madd(b, k);
    let mut a = Image::<T>::new(w, h);
    let lastx = w as isize - 1;
    par_rows(&mut a.data, w, |y, row| {
        let src = &img.data[y * w..(y + 1) * w];
        let at = |x: isize| src[x.clamp(0, lastx) as usize];
        for (x, o) in row.iter_mut().enumerate() {
            let x = x as isize;
            let acc = T::zero().madd(at(x), 0.375);
            let acc = tap(acc, at(x - s), at(x + s), 0.25);
            *o = tap(acc, at(x - 2 * s), at(x + 2 * s), 0.0625);
        }
    });
    let mut b = Image::<T>::new(w, h);
    let lasty = h as isize - 1;
    par_rows(&mut b.data, w, |y, row| {
        let r = |dy: isize| {
            let yy = (y as isize + dy).clamp(0, lasty) as usize;
            &a.data[yy * w..(yy + 1) * w]
        };
        let (c, u1, d1, u2, d2) = (r(0), r(-s), r(s), r(-2 * s), r(2 * s));
        for (x, o) in row.iter_mut().enumerate() {
            let acc = T::zero().madd(c[x], 0.375);
            let acc = tap(acc, u1[x], d1[x], 0.25);
            *o = tap(acc, u2[x], d2[x], 0.0625);
        }
    });
    b
}

/// Noise estimated from the photo itself: per level and channel, the standard deviation of the
/// noise in the detail coefficients (working-channel units).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoiseEst {
    pub sigma: [[f32; 3]; NR_MAX_LEVELS],
}

/// Tile size and the margin left out of the statistics (the coarse levels' clamped edges).
pub const NR_TILE: usize = 160;
const NR_TILE_MARGIN: usize = 32;

/// The tiles the noise is measured on: (x0, y0, width, height), a 4 × 2 grid over the image.
pub fn nr_tiles(w: usize, h: usize) -> Vec<(usize, usize, usize, usize)> {
    let (tw, th) = (NR_TILE.min(w), NR_TILE.min(h));
    let mut out = Vec::new();
    for j in 0..2 {
        for i in 0..4 {
            let cx = ((2 * i + 1) * w / 8) as isize - (tw / 2) as isize;
            let cy = ((2 * j + 1) * h / 4) as isize - (th / 2) as isize;
            out.push((cx.clamp(0, (w - tw) as isize) as usize, cy.clamp(0, (h - th) as isize) as usize, tw, th));
        }
    }
    out
}

/// The tiles of [`nr_tiles`] copied out of `img`.
pub fn nr_tile_images(img: &Rgb32f) -> Vec<Rgb32f> {
    nr_tiles(img.width, img.height)
        .into_iter()
        .map(|(x0, y0, tw, th)| Rgb32f::from_fn(tw, th, |x, y| img.data[(y0 + y) * img.width + x0 + x]))
        .collect()
}

/// Noise floor (working units): a clean photo still gets some smoothing.
const NR_FLOOR: f32 = 0.002;

/// The noise of every level from the tiles (linear RGB, see [`nr_tile_images`]): the median
/// absolute deviation of each level's coefficients (a robust estimate: edges and texture are the
/// few large ones). A level's estimate is kept between what white noise of the finest level's
/// strength gives there and a few times that (demosaicing correlates noise: coarser levels hold
/// more than white noise would; more than that is texture, not noise).
pub fn noise_estimate(tiles: &[Rgb32f], levels: usize) -> NoiseEst {
    let mut samples: Vec<[Vec<f32>; 3]> = (0..levels).map(|_| [Vec::new(), Vec::new(), Vec::new()]).collect();
    for t in tiles {
        let m = NR_TILE_MARGIN.min(t.width / 4).min(t.height / 4);
        let mut c = t.map(cnr_split);
        for (j, sj) in samples.iter_mut().enumerate() {
            let cn = atrous(&c, j);
            for y in m..t.height - m {
                for x in m..t.width - m {
                    let i = y * t.width + x;
                    for k in 0..3 {
                        sj[k].push((c.data[i][k] - cn.data[i][k]).abs());
                    }
                }
            }
            c = cn;
        }
    }
    let mut est = NoiseEst::default();
    let med = |v: &mut Vec<f32>| {
        if v.is_empty() {
            return 0.0;
        }
        let k = v.len() / 2;
        v.select_nth_unstable_by(k, |a, b| a.total_cmp(b));
        v[k] / 0.6745
    };
    let mut raw = [[0f32; 3]; NR_MAX_LEVELS];
    for (j, sj) in samples.iter_mut().enumerate() {
        for k in 0..3 {
            raw[j][k] = med(&mut sj[k]);
        }
    }
    for k in 0..3 {
        // pixel noise from the finest level
        let base = (raw[0][k] / ATROUS_WHITE[0]).max(NR_FLOOR);
        let cap = if k == 0 { 3.0 } else { 4.0 };
        for j in 0..NR_MAX_LEVELS {
            let white = base * ATROUS_WHITE[j];
            est.sigma[j][k] = if j < levels { raw[j][k].clamp(white, cap * white) } else { white };
        }
    }
    est
}

/// Thresholds of level `j`: luminance, chroma a, chroma b, and the luminance detail that holds
/// chroma smoothing back (0: none). Detail lowers the finest levels' thresholds, Contrast the
/// coarser luminance levels' (local contrast is kept).
pub fn level_thresholds(p: &NrParams, e: &NoiseEst, j: usize) -> [f32; 4] {
    let s = e.sigma[j.min(NR_MAX_LEVELS - 1)];
    let ml = match j {
        0 => 1.2 - 0.8 * p.detail,
        1 => 1.1 - 0.6 * p.detail,
        _ => 1.0 - 0.75 * p.contrast,
    };
    let mc = match j {
        0 => 1.2 - 0.8 * p.col_detail,
        1 => 1.1 - 0.5 * p.col_detail,
        _ => 1.0,
    };
    let tl = if j < p.levels_l { 3.0 * p.lum * s[0] * ml } else { 0.0 };
    let (ta, tb) = if j < p.levels_c { (3.5 * p.col * s[1] * mc, 3.5 * p.col * s[2] * mc) } else { (0.0, 0.0) };
    [tl, ta, tb, 4.0 * s[0]]
}

/// Non-negative garrote shrinkage of coefficient `d` by threshold `t` (0: kept): small
/// coefficients (noise) go, large ones (edges) stay with little bias.
#[inline]
pub fn shrink(d: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return d;
    }
    let d2 = d * d;
    if d2 <= t * t { 0.0 } else { d - t * t / d }
}

/// One pixel's detail coefficients `d` shrunk by thresholds `t` (see [`level_thresholds`]):
/// chroma smoothing eases where luminance has an edge at that scale (colour edges coincide with
/// luminance edges), so colour does not bleed across them.
#[inline]
pub fn wav_shrink_px(d: [f32; 3], t: [f32; 4]) -> [f32; 3] {
    let g = if t[3] > 0.0 {
        let q = d[0] / t[3];
        1.0 / (1.0 + q * q)
    } else {
        1.0
    };
    [shrink(d[0], t[0]), shrink(d[1], t[1] * g), shrink(d[2], t[2] * g)]
}

/// The denoised colour of `c` from its shrunk working channels `v`: luminance and/or chroma
/// replaced (the other kept exactly).
#[inline]
pub fn nr_join(c: [f32; 3], v: [f32; 3], lum: bool, col: bool) -> [f32; 3] {
    let y = luminance_2020(c);
    let y1 = if lum { v[0].max(0.0) * v[0].max(0.0) } else { y };
    if col {
        return cnr_join(y1, v[1], v[2]);
    }
    if y > 1e-12 { c.map(|x| (x * (y1 / y)).max(0.0)) } else { c.map(|x| x.max(0.0)) }
}

/// The working channels of `v` denoised by wavelet shrinkage: Σ shrink(levels) + coarse residual.
pub fn wavelet_shrink(v: &Rgb32f, p: &NrParams, e: &NoiseEst) -> Rgb32f {
    let w = v.width;
    let mut out = Rgb32f::new(v.width, v.height);
    let mut c = v.clone();
    for j in 0..p.levels() {
        let cn = atrous(&c, j);
        let t = level_thresholds(p, e, j);
        for_rows(&mut out.data, w, |y, row| {
            for (x, o) in row.iter_mut().enumerate() {
                let i = y * w + x;
                let (a, b) = (c.data[i], cn.data[i]);
                let d = wav_shrink_px([a[0] - b[0], a[1] - b[1], a[2] - b[2]], t);
                *o = [o[0] + d[0], o[1] + d[1], o[2] + d[2]];
            }
        });
        c = cn;
    }
    for_rows(&mut out.data, w, |y, row| {
        for (x, o) in row.iter_mut().enumerate() {
            let r = c.data[y * w + x];
            *o = [o[0] + r[0], o[1] + r[1], o[2] + r[2]];
        }
    });
    out
}

/// Noise reduction (process 2026) of the white-balanced image, in place: the noise measured on
/// the photo's own tiles, then wavelet shrinkage of luminance and chroma.
pub fn denoise(img: &mut Rgb32f, p: &NrParams) {
    let est = noise_estimate(&nr_tile_images(img), p.levels());
    let v = img.map(cnr_split);
    let out = wavelet_shrink(&v, p, &est);
    let (lum, col) = (p.levels_l > 0, p.levels_c > 0);
    let w = img.width;
    for_rows(&mut img.data, w, |y, row| {
        for (x, c) in row.iter_mut().enumerate() {
            *c = nr_join(*c, out.data[y * w + x], lum, col);
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Dehaze

/// The dark channel and its refinement run on a grid of about this many pixels along the long
/// edge (the transmission is smooth; the guided filter's apply step at full size keeps edges).
pub const HAZE_GRID: usize = 1024;
/// Guided-filter ε of the transmission refinement (√luminance²).
pub const HAZE_EPS: f32 = 1e-3;
/// Transmission floor.
pub const HAZE_T0: f32 = 0.1;

/// Sizes of the dehaze grid for a `w × h` image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazeDims {
    /// Box-subsampling step to the grid, and the grid's size.
    pub step: usize,
    pub lw: usize,
    pub lh: usize,
    /// Min-filter patch radius (grid px; 0.8 % of the long edge).
    pub patch: usize,
    /// Guided-filter window (grid px).
    pub sigma: f32,
}

pub fn haze_dims(w: usize, h: usize) -> HazeDims {
    let step = w.max(h).div_ceil(HAZE_GRID).max(1);
    let (lw, lh) = (w.div_ceil(step).max(1), h.div_ceil(step).max(1));
    let patch = ((lw.max(lh) as f32 * 0.008).round() as usize).max(1);
    HazeDims { step, lw, lh, patch, sigma: 3.0 * patch as f32 }
}

/// The guide of the transmission refinement: √luminance.
#[inline]
pub fn haze_guide(c: [f32; 3]) -> f32 {
    luminance_2020(c).max(0.0).sqrt().min(4.0)
}

/// The airlight-normalized dark channel of one pixel.
#[inline]
pub fn haze_dark(c: [f32; 3], air: [f32; 3]) -> f32 {
    (c[0] / air[0]).min(c[1] / air[1]).min(c[2] / air[2]).clamp(0.0, 1.0)
}

fn percentile(mut v: Vec<f32>, p: usize) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let k = (v.len() * p / 1000).min(v.len() - 1);
    v.select_nth_unstable_by(k, |a, b| a.total_cmp(b));
    v[k]
}

/// The airlight (before exposure) from the grid image `lo`: the mean colour of its haziest region
/// (the top 0.1 % of the patch dark channel). Kept at least as bright as 80 % of the photo's
/// 99th-percentile luminance (a photo without a bright veil is dehazed only where it is that
/// bright), and its colour at most 75 % saturated (a vivid "airlight" is an object, not haze).
pub fn haze_airlight(lo: &Rgb32f, patch: usize) -> [f32; 3] {
    let dark = min_filter(&lo.map(|c| c[0].min(c[1]).min(c[2])), patch);
    if dark.data.is_empty() {
        return [1.0; 3];
    }
    let thr = percentile(dark.data.clone(), 999);
    let (mut acc, mut n) = ([0f64; 3], 0usize);
    for (d, c) in dark.data.iter().zip(&lo.data) {
        if *d >= thr {
            for k in 0..3 {
                acc[k] += c[k].max(0.0) as f64;
            }
            n += 1;
        }
    }
    let a = acc.map(|v| (v / n.max(1) as f64) as f32);
    let y99 = percentile(lo.data.iter().map(|c| luminance_2020(*c)).collect(), 990);
    let target = (0.8 * y99).max(0.05);
    let ya = luminance_2020(a);
    let a = if ya >= target {
        a
    } else if ya > 1e-6 {
        a.map(|v| v * target / ya)
    } else {
        [target; 3]
    };
    let ya = luminance_2020(a);
    a.map(|v| (ya + (v - ya) * 0.75).max(0.02 * ya).max(1e-4))
}

/// Blurred cross guided-filter coefficients (a, b) of `p` guided by `guide` (the `xguided_*`
/// kernels' twin).
pub fn guided_cross_coeffs(guide: &Plane, p: &Plane, sigma: f32, eps: f32) -> (Plane, Plane) {
    let mi = gaussian(guide, sigma);
    let mp = gaussian(p, sigma);
    let cip = gaussian(&guide.zip_map(p, |a, b| a * b), sigma);
    let cii = gaussian(&guide.map(|a| a * a), sigma);
    let mut a = Plane::new(p.width, p.height);
    let mut b = Plane::new(p.width, p.height);
    for i in 0..p.data.len() {
        let var = (cii.data[i] - mi.data[i] * mi.data[i]).max(0.0);
        let k = (cip.data[i] - mi.data[i] * mp.data[i]) / (var + eps);
        a.data[i] = k;
        b.data[i] = mp.data[i] - k * mi.data[i];
    }
    (gaussian(&a, sigma), gaussian(&b, sigma))
}

/// The refined, airlight-normalized dark channel (0..1) of the white-balanced image `img` at its
/// own size, and the airlight (before exposure). Transmission is `1 − ω·d` (linear in `d`, so
/// the amount needs no new plane).
pub fn haze_plane(img: &Rgb32f) -> (Plane, [f32; 3]) {
    let (w, h) = (img.width, img.height);
    let d = haze_dims(w, h);
    let lo = if d.step > 1 { resize(img, d.lw, d.lh, Filter::Box) } else { img.clone() };
    let air = haze_airlight(&lo, d.patch);
    let dn = min_filter(&lo.map(|c| haze_dark(c, air)), d.patch);
    let (ma, mb) = guided_cross_coeffs(&lo.map(haze_guide), &dn, d.sigma, HAZE_EPS);
    let (ma, mb) = if d.step > 1 { (resize(&ma, w, h, Filter::Bilinear), resize(&mb, w, h, Filter::Bilinear)) } else { (ma, mb) };
    let mut out = Plane::new(w, h);
    for_rows(&mut out.data, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            let i = y * w + x;
            *v = (ma.data[i] * haze_guide(img.data[i]) + mb.data[i]).clamp(0.0, 1.0);
        }
    });
    (out, air)
}

/// Dehaze of scene-linear `c` by `dz` (−1..1) with the refined dark channel `d` and the airlight
/// `air` (after exposure). Positive: the haze model inverted, `J = (I − A)/t + A`, with the
/// transmission floored and pixels brighter than the airlight (light sources) protected.
/// Negative: haze added with the same model, `A + (I − A)·f`, more where the scene is already hazy.
#[inline]
pub fn dehaze_px(c: [f32; 3], dz: f32, d: f32, air: [f32; 3]) -> [f32; 3] {
    let ya = luminance_2020(air).max(1e-6);
    if dz > 0.0 {
        let mut t = (1.0 - 0.95 * dz.min(1.0) * d).max(HAZE_T0);
        let hp = smooth(1.0, 2.0, luminance_2020(c) / ya);
        t += (1.0 - t) * hp;
        [0, 1, 2].map(|k| ((c[k] - air[k]) / t + air[k]).max(0.0))
    } else {
        let m = (-dz).min(1.0);
        let te = (1.0 - 0.95 * d).max(HAZE_T0);
        let f = (1.0 - 0.5 * m) * te.powf(m);
        [0, 1, 2].map(|k| (air[k] + (c[k] - air[k]) * f).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cnr_split_and_join_round_trip() {
        for c in [[0.2f32, 0.3, 0.1], [0.9, 0.05, 0.4], [0.0, 0.5, 0.0], [0.01, 0.012, 0.009], [2.0, 1.5, 0.2]] {
            let v = cnr_split(c);
            let back = cnr_join(luminance_2020(c), v[1], v[2]);
            for k in 0..3 {
                assert!((back[k] - c[k]).abs() < 1e-4 * (1.0 + c[k]), "{c:?} → {back:?}");
            }
        }
        // any chroma keeps the luminance
        let j = cnr_join(0.3, 0.4, -0.2);
        assert!((luminance_2020(j) - 0.3).abs() < 1e-5);
    }

    #[test]
    fn sharp_taps_follow_the_radius() {
        let a = SharpK::new(0.0, 0.0, 1.0);
        assert_eq!((a.halo, a.mask_t), (0.15, 0.0));
        let b = SharpK::new(1.0, 1.0, 1.0);
        assert!(b.halo == 1.0 && b.mask_t > 1.0);
    }

    #[test]
    fn haze_dims_scale_with_the_image() {
        let full = haze_dims(6000, 4000);
        let prev = haze_dims(1500, 1000);
        assert_eq!((full.step, full.lw, full.lh), (6, 1000, 667));
        assert_eq!((prev.step, prev.lw), (2, 750));
        assert_eq!(full.patch, 8);
    }
}
