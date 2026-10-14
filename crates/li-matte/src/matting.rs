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

use serde::{Deserialize, Serialize};

use crate::RgbImage;
use crate::distance::edt;

/// Select and Mask parameters (Photoshop's units).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RefineParams {
    /// Edge-detection band radius in pixels (0 = no edge detection).
    pub radius: f32,
    /// Adapt the band to the edge's observed softness.
    pub smart_radius: bool,
    /// 0–100.
    pub smooth: f32,
    /// Pixels.
    pub feather: f32,
    /// 0–100.
    pub contrast: f32,
    /// −100–100 (% of `max(radius, 4 px)`).
    pub shift_edge: f32,
}

impl Default for RefineParams {
    fn default() -> Self {
        Self { radius: 0.0, smart_radius: false, smooth: 0.0, feather: 0.0, contrast: 0.0, shift_edge: 0.0 }
    }
}

/// Guided-filter regulariser (colour guide, samples in 0–1).
pub const EPS: f32 = 1e-4;

impl RefineParams {
    fn gf_radius(&self) -> usize {
        self.radius.ceil().clamp(1.0, 64.0) as usize
    }
    fn smooth_sigma(&self) -> f32 {
        self.smooth.clamp(0.0, 100.0) / 100.0 * 4.0
    }
    fn feather_sigma(&self) -> f32 {
        self.feather.max(0.0) / 2.0
    }
    fn shift_px(&self) -> i32 {
        (self.shift_edge.clamp(-100.0, 100.0) / 100.0 * self.radius.max(4.0)).round().clamp(-64.0, 64.0) as i32
    }
    pub fn uses_image(&self) -> bool {
        self.radius > 0.0
    }
    /// Pixels read around an output pixel.
    pub fn halo(&self) -> i32 {
        let edge = if self.uses_image() {
            let g = self.gf_radius() as i32;
            (self.radius.ceil() as i32).max(2 * g + if self.smart_radius { g + 1 } else { 0 })
        } else {
            0
        };
        edge + (3.0 * self.smooth_sigma()).ceil() as i32 + (3.0 * self.feather_sigma()).ceil() as i32 + self.shift_px().abs() + 2
    }
    /// How far the refined mask can extend beyond the original.
    pub fn spread(&self) -> i32 {
        (if self.uses_image() { self.radius.ceil() as i32 } else { 0 })
            + (3.0 * self.smooth_sigma()).ceil() as i32
            + (3.0 * self.feather_sigma()).ceil() as i32
            + self.shift_px().max(0)
            + 1
    }
}

pub use li_raster::blur::box_mean;

/// Guided filter with a grayscale guide.
pub fn guided_filter_gray(guide: &[f32], p: &[f32], w: usize, h: usize, r: usize, eps: f32) -> Vec<f32> {
    guided_gray(guide, p, w, h, eps, false, |v| box_mean(v, w, h, r))
}

/// Gaussian-weighted grayscale guided filter for log-EV Develop masks and Auto Mask.
/// This preserves Develop's box Gaussian policy, nonnegative variance and output clamp.
pub fn guided_filter_gaussian(guide: &[f32], p: &[f32], w: usize, h: usize, sigma: f32, eps: f32) -> Vec<f32> {
    guided_gray(guide, p, w, h, eps, true, |v| li_raster::blur::gaussian_with_policy(v, w, h, sigma, li_raster::blur::GaussianPolicy::Boxes))
        .into_iter()
        .map(|v| v.clamp(0.0, 1.0))
        .collect()
}

fn guided_gray(guide: &[f32], p: &[f32], w: usize, h: usize, eps: f32, clamp_variance: bool, mean: impl Fn(&[f32]) -> Vec<f32>) -> Vec<f32> {
    assert_eq!(guide.len(), w * h);
    assert_eq!(p.len(), w * h);
    let mi = mean(guide);
    let mp = mean(p);
    let ip: Vec<f32> = guide.iter().zip(p).map(|(a, b)| a * b).collect();
    let ii: Vec<f32> = guide.iter().map(|a| a * a).collect();
    let mip = mean(&ip);
    let mii = mean(&ii);
    let mut a = vec![0.0f32; w * h];
    let mut b = vec![0.0f32; w * h];
    for i in 0..w * h {
        let var = mii[i] - mi[i] * mi[i];
        let var = if clamp_variance { var.max(0.0) } else { var };
        let cov = mip[i] - mi[i] * mp[i];
        a[i] = cov / (var + eps);
        b[i] = mp[i] - a[i] * mi[i];
    }
    let ma = mean(&a);
    let mb = mean(&b);
    (0..w * h).map(|i| ma[i] * guide[i] + mb[i]).collect()
}

/// Guided filter with a colour guide (He et al., Eq. 19–21: `a_k = (Σ_k + εU)⁻¹ cov_k(I, p)`).
pub fn guided_filter_color(guide: &RgbImage<'_>, p: &[f32], r: usize, eps: f32) -> Vec<f32> {
    let (w, h) = (guide.w, guide.h);
    let n = w * h;
    let ch = |c: usize| -> Vec<f32> { guide.px.iter().map(|q| q[c]).collect() };
    let chans = [ch(0), ch(1), ch(2)];
    let m: Vec<Vec<f32>> = chans.iter().map(|c| box_mean(c, w, h, r)).collect();
    let mp = box_mean(p, w, h, r);
    let mip: Vec<Vec<f32>> = chans.iter().map(|c| box_mean(&c.iter().zip(p).map(|(a, b)| a * b).collect::<Vec<_>>(), w, h, r)).collect();
    let pairs = [(0, 0), (0, 1), (0, 2), (1, 1), (1, 2), (2, 2)];
    let mii: Vec<Vec<f32>> = pairs.iter().map(|&(a, b)| box_mean(&chans[a].iter().zip(&chans[b]).map(|(x, y)| x * y).collect::<Vec<_>>(), w, h, r)).collect();
    let mut av = [vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n]];
    let mut bv = vec![0.0f32; n];
    for i in 0..n {
        let mu = [m[0][i] as f64, m[1][i] as f64, m[2][i] as f64];
        let s = |k: usize, a: usize, b: usize| mii[k][i] as f64 - mu[a] * mu[b];
        let e = eps as f64;
        let (rr, rg, rb, gg, gb, bb) = (s(0, 0, 0) + e, s(1, 0, 1), s(2, 0, 2), s(3, 1, 1) + e, s(4, 1, 2), s(5, 2, 2) + e);
        let cov = [mip[0][i] as f64 - mu[0] * mp[i] as f64, mip[1][i] as f64 - mu[1] * mp[i] as f64, mip[2][i] as f64 - mu[2] * mp[i] as f64];
        // Inverse of the symmetric 3×3 matrix.
        let i00 = gg * bb - gb * gb;
        let i01 = gb * rb - rg * bb;
        let i02 = rg * gb - gg * rb;
        let i11 = rr * bb - rb * rb;
        let i12 = rb * rg - rr * gb;
        let i22 = rr * gg - rg * rg;
        let det = rr * i00 + rg * i01 + rb * i02;
        let (a0, a1, a2) = if det.abs() < 1e-30 {
            (0.0, 0.0, 0.0)
        } else {
            (
                (i00 * cov[0] + i01 * cov[1] + i02 * cov[2]) / det,
                (i01 * cov[0] + i11 * cov[1] + i12 * cov[2]) / det,
                (i02 * cov[0] + i12 * cov[1] + i22 * cov[2]) / det,
            )
        };
        av[0][i] = a0 as f32;
        av[1][i] = a1 as f32;
        av[2][i] = a2 as f32;
        bv[i] = (mp[i] as f64 - a0 * mu[0] - a1 * mu[1] - a2 * mu[2]) as f32;
    }
    let ma: Vec<Vec<f32>> = av.iter().map(|a| box_mean(a, w, h, r)).collect();
    let mb = box_mean(&bv, w, h, r);
    (0..n).map(|i| ma[0][i] * chans[0][i] + ma[1][i] * chans[1][i] + ma[2][i] * chans[2][i] + mb[i]).collect()
}

/// Shared exact-small / running-sum-large Gaussian (replicated borders).
pub fn gaussian_blur(src: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    let mut out = src.to_vec();
    li_raster::blur::gaussian_n(&mut out, w, h, 1, sigma, false);
    out
}

/// Grey dilation (`grow`) or erosion by about `r` pixels: alternating 3×3 square and cross steps
/// (an octagonal structuring element).
pub fn morph(src: &[f32], w: usize, h: usize, r: usize, grow: bool) -> Vec<f32> {
    let mut cur = src.to_vec();
    let mut nxt = cur.clone();
    let pick = |a: f32, b: f32| if grow { a.max(b) } else { a.min(b) };
    for k in 0..r {
        let square = k % 2 == 0;
        for y in 0..h {
            for x in 0..w {
                let mut v = cur[y * w + x];
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if (dx != 0 && dy != 0 && !square) || (dx == 0 && dy == 0) {
                            continue;
                        }
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            v = pick(v, cur[ny as usize * w + nx as usize]);
                        }
                    }
                }
                nxt[y * w + x] = v;
            }
        }
        std::mem::swap(&mut cur, &mut nxt);
    }
    cur
}

/// Running max (or min) over a window of radius `r` (edge-clipped), van Herk / Gil–Werman:
/// three comparisons per sample whatever the radius.
fn running_extreme(src: &[f32], out: &mut [f32], r: usize, max: bool, g: &mut Vec<f32>, hb: &mut Vec<f32>) {
    let n = src.len();
    let k = 2 * r + 1;
    let f = |a: f32, b: f32| if max { a.max(b) } else { a.min(b) };
    let pad = if max { f32::NEG_INFINITY } else { f32::INFINITY };
    let m = (n + 2 * r).div_ceil(k) * k;
    g.clear();
    g.resize(m, pad);
    hb.clear();
    hb.resize(m, pad);
    let p = |j: usize| if j >= r && j < r + n { src[j - r] } else { pad };
    for j in 0..m {
        g[j] = if j % k == 0 { p(j) } else { f(g[j - 1], p(j)) };
    }
    for j in (0..m).rev() {
        hb[j] = if j % k == k - 1 || j == m - 1 { p(j) } else { f(hb[j + 1], p(j)) };
    }
    for (i, o) in out.iter_mut().enumerate() {
        *o = f(hb[i], g[i + 2 * r]);
    }
}

/// Running max (or min) over a square window of radius `r` (edge-clipped), separable.
fn max_square(src: &[f32], w: usize, h: usize, r: usize, max: bool) -> Vec<f32> {
    let (mut g, mut hb) = (Vec::new(), Vec::new());
    let mut tmp = vec![0.0f32; w * h];
    for y in 0..h {
        running_extreme(&src[y * w..(y + 1) * w], &mut tmp[y * w..(y + 1) * w], r, max, &mut g, &mut hb);
    }
    let mut out = vec![0.0f32; w * h];
    let (mut col, mut res) = (vec![0.0f32; h], vec![0.0f32; h]);
    for x in 0..w {
        for y in 0..h {
            col[y] = tmp[y * w + x];
        }
        running_extreme(&col, &mut res, r, max, &mut g, &mut hb);
        for y in 0..h {
            out[y * w + x] = res[y];
        }
    }
    out
}

/// Per-pixel transition width (pixels) of the guide's luminance: local range / local max
/// gradient over a window of radius `r` (a sharp step gives ~2, a ramp of width L gives ~L).
pub fn edge_width(img: &RgbImage<'_>, r: usize) -> Vec<f32> {
    let (w, h) = (img.w, img.h);
    let y: Vec<f32> = img.px.iter().map(|p| li_color::convert::rgb_to_gray([p[0], p[1], p[2]])).collect();
    let mut g = vec![0.0f32; w * h];
    for yy in 0..h {
        for xx in 0..w {
            let at = |x: usize, y2: usize| y[y2 * w + x];
            let gx = (at((xx + 1).min(w - 1), yy) - at(xx.saturating_sub(1), yy)) / 2.0;
            let gy = (at(xx, (yy + 1).min(h - 1)) - at(xx, yy.saturating_sub(1))) / 2.0;
            g[yy * w + xx] = (gx * gx + gy * gy).sqrt();
        }
    }
    let gm = max_square(&g, w, h, r, true);
    let hi = max_square(&y, w, h, r, true);
    let lo = max_square(&y, w, h, r, false);
    (0..w * h).map(|i| (hi[i] - lo[i]) / gm[i].max(1e-4)).collect()
}

/// Refines a soft mask buffer `m` (`w × h`) with `img` as guide (required when `radius > 0`).
pub fn refine_buffer(img: Option<&RgbImage<'_>>, m: &[f32], w: usize, h: usize, p: &RefineParams) -> Vec<f32> {
    let mut a = m.to_vec();
    if let (true, Some(img)) = (p.uses_image(), img) {
        let bin: Vec<bool> = m.iter().map(|v| *v >= 0.5).collect();
        let mut boundary = vec![false; w * h];
        let mut any = false;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let v = bin[i];
                let diff = (x > 0 && bin[i - 1] != v) || (x + 1 < w && bin[i + 1] != v) || (y > 0 && bin[i - w] != v) || (y + 1 < h && bin[i + w] != v);
                boundary[i] = diff;
                any |= diff;
            }
        }
        if any {
            let dist = edt(&boundary, w, h);
            let r = p.gf_radius();
            let q = guided_filter_color(img, m, r, EPS);
            let widths = p.smart_radius.then(|| edge_width(img, r));
            for i in 0..w * h {
                let reff = match &widths {
                    Some(wd) => wd[i].clamp(1.5, p.radius.max(1.5)),
                    None => p.radius,
                };
                let wgt = (reff + 1.0 - dist[i]).clamp(0.0, 1.0);
                if wgt > 0.0 {
                    a[i] = m[i] + (q[i].clamp(0.0, 1.0) - m[i]) * wgt;
                }
            }
        }
    }
    let ss = p.smooth_sigma();
    if ss >= 0.3 {
        a = gaussian_blur(&a, w, h, ss);
        let gain = 1.0 + ss;
        for v in &mut a {
            *v = ((*v - 0.5) * gain + 0.5).clamp(0.0, 1.0);
        }
    }
    a = gaussian_blur(&a, w, h, p.feather_sigma());
    if p.contrast > 0.0 {
        let gain = 1.0 / (1.0 - 0.99 * p.contrast.clamp(0.0, 100.0) / 100.0);
        for v in &mut a {
            *v = ((*v - 0.5) * gain + 0.5).clamp(0.0, 1.0);
        }
    }
    let d = p.shift_px();
    if d != 0 {
        a = morph(&a, w, h, d.unsigned_abs() as usize, d > 0);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn max_square_matches_naive() {
        let (w, h) = (17, 11);
        let src: Vec<f32> = (0..w * h).map(|i| ((i * 37) % 23) as f32).collect();
        for r in [1usize, 2, 5] {
            for max in [true, false] {
                let fast = max_square(&src, w, h, r, max);
                for y in 0..h {
                    for x in 0..w {
                        let mut v = if max { f32::MIN } else { f32::MAX };
                        for yy in y.saturating_sub(r)..(y + r + 1).min(h) {
                            for xx in x.saturating_sub(r)..(x + r + 1).min(w) {
                                v = if max { v.max(src[yy * w + xx]) } else { v.min(src[yy * w + xx]) };
                            }
                        }
                        assert_eq!(fast[y * w + x], v, "r={r} max={max} ({x},{y})");
                    }
                }
            }
        }
    }
}
