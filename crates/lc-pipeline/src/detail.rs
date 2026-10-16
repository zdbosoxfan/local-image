//! Detail tools: pixel-scale log-luminance USM, profiled edge-aware wavelet NR and dehaze.
//! Sharpening is our own design, with local-range halo suppression and edge masking.
//! NR and haze are faithful scalar darktable ports; see the submodules for provenance,
//! parameter mapping and documented host extensions.

pub mod haze;
pub mod nr;
pub use haze::{Haze, dehaze_px, haze_plane};
pub use nr::{NrParams, denoise, nr_params};

use lightcraft_develop::DevelopSettings;
use lightcraft_raster::Plane;
use lightcraft_raster::blur::gaussian_fine;

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
    /// Share of the overshoot past the local 3 × 3 range that is kept (halo control: 0.03 at
    /// Detail 0, 0.15 at Detail 100).
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
        SharpK { detail: d, halo: 0.03 + 0.12 * d, mask_t: if m > 0.0 { 0.1 * m + 1.5 * m * m } else { 0.0 }, edge_k: 2.5 * (sm * sm + 0.25).sqrt() }
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
