//! local-image: hair-grade alpha matting from a trimap, and foreground colour estimation.
//!
//! * **Closed-form matting** (A. Levin, D. Lischinski, Y. Weiss, "A Closed-Form Solution to Natural
//!   Image Matting", PAMI 2008), as in GIMP/GEGL's `matting-levin`, but solved **matrix-free**
//!   (K. He, J. Sun, X. Tang, "Fast Matting Using Large Kernel Matting Laplacian Matrices", CVPR
//!   2010): the matting Laplacian `L` is applied with box filters in O(N) per product, and
//!   `(L + λD) α = λ D t` is solved by Jacobi-preconditioned conjugate gradients, coarse to fine,
//!   warm-started from the guided filter. No sparse direct solver, pure Rust.
//! * **Foreground estimation** (M. Germer, T. Uelwer, S. Conrad, S. Harmeling, "Fast Multi-Level
//!   Foreground Estimation", ICPR 2020): solves the compositing equation `I = αF + (1−α)B` for
//!   smooth F and B, so cutouts don't carry the old background's colour in their soft edges.
//!
//! Trimaps use 0 = background, 1 = foreground, anything in between = unknown.

use crate::matting::{box_mean, guided_filter_color};
use crate::segment::RgbImage;

/// Matting parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MattingParams {
    /// Window radius of the matting Laplacian (1 = 3×3, Levin's default; larger is smoother and
    /// handles wider fuzzy bands).
    pub radius: usize,
    /// Regulariser ε (Levin 1e-7; GEGL 1e-6).
    pub epsilon: f32,
    /// Weight of the known (trimap) pixels.
    pub lambda: f32,
    /// Conjugate-gradient iterations at the finest level.
    pub iterations: usize,
}

impl Default for MattingParams {
    fn default() -> Self {
        Self { radius: 1, epsilon: 1e-6, lambda: 100.0, iterations: 120 }
    }
}

fn is_known(t: f32) -> bool {
    !(0.02..=0.98).contains(&t)
}

/// Per-pixel precomputation of the window statistics: mean colour and the inverse regularised
/// covariance (symmetric 3×3, stored as 6 values).
struct Windows {
    w: usize,
    h: usize,
    r: usize,
    chans: [Vec<f32>; 3],
    mu: [Vec<f32>; 3],
    inv: [Vec<f32>; 6],
}

impl Windows {
    fn new(img: &RgbImage, r: usize, eps: f32) -> Self {
        let (w, h) = (img.w, img.h);
        let chans = [0, 1, 2].map(|c| img.px.iter().map(|p| p[c]).collect::<Vec<f32>>());
        let mu = [0, 1, 2].map(|c| box_mean(&chans[c], w, h, r));
        let pairs = [(0, 0), (0, 1), (0, 2), (1, 1), (1, 2), (2, 2)];
        let ii: Vec<Vec<f32>> =
            pairs.iter().map(|&(a, b)| box_mean(&chans[a].iter().zip(&chans[b]).map(|(x, y)| x * y).collect::<Vec<_>>(), w, h, r)).collect();
        let n = w * h;
        let mut inv: [Vec<f32>; 6] = std::array::from_fn(|_| vec![0.0; n]);
        let win = ((2 * r + 1) * (2 * r + 1)) as f64;
        for i in 0..n {
            let m = [mu[0][i] as f64, mu[1][i] as f64, mu[2][i] as f64];
            let e = eps as f64 / win;
            let s = |k: usize, a: usize, b: usize| ii[k][i] as f64 - m[a] * m[b];
            let (rr, rg, rb, gg, gb, bb) = (s(0, 0, 0) + e, s(1, 0, 1), s(2, 0, 2), s(3, 1, 1) + e, s(4, 1, 2), s(5, 2, 2) + e);
            let i00 = gg * bb - gb * gb;
            let i01 = gb * rb - rg * bb;
            let i02 = rg * gb - gg * rb;
            let i11 = rr * bb - rb * rb;
            let i12 = rb * rg - rr * gb;
            let i22 = rr * gg - rg * rg;
            let det = rr * i00 + rg * i01 + rb * i02;
            let d = if det.abs() < 1e-30 { 0.0 } else { 1.0 / det };
            for (k, v) in [i00, i01, i02, i11, i12, i22].into_iter().enumerate() {
                inv[k][i] = (v * d) as f32;
            }
        }
        Self { w, h, r, chans, mu, inv }
    }

    /// `L p` (He et al. 2010, Eq. 9–11), normalised by the window size.
    fn laplacian(&self, p: &[f32]) -> Vec<f32> {
        let (w, h, r) = (self.w, self.h, self.r);
        let n = w * h;
        let pm = box_mean(p, w, h, r);
        let ipm: [Vec<f32>; 3] = std::array::from_fn(|c| box_mean(&self.chans[c].iter().zip(p).map(|(a, b)| a * b).collect::<Vec<_>>(), w, h, r));
        let mut a: [Vec<f32>; 3] = std::array::from_fn(|_| vec![0.0; n]);
        let mut b = vec![0.0f32; n];
        for i in 0..n {
            let c = [ipm[0][i] - self.mu[0][i] * pm[i], ipm[1][i] - self.mu[1][i] * pm[i], ipm[2][i] - self.mu[2][i] * pm[i]];
            let v = &self.inv;
            let a0 = v[0][i] * c[0] + v[1][i] * c[1] + v[2][i] * c[2];
            let a1 = v[1][i] * c[0] + v[3][i] * c[1] + v[4][i] * c[2];
            let a2 = v[2][i] * c[0] + v[4][i] * c[1] + v[5][i] * c[2];
            a[0][i] = a0;
            a[1][i] = a1;
            a[2][i] = a2;
            b[i] = pm[i] - a0 * self.mu[0][i] - a1 * self.mu[1][i] - a2 * self.mu[2][i];
        }
        let am: [Vec<f32>; 3] = std::array::from_fn(|c| box_mean(&a[c], w, h, r));
        let bm = box_mean(&b, w, h, r);
        (0..n).map(|i| p[i] - (am[0][i] * self.chans[0][i] + am[1][i] * self.chans[1][i] + am[2][i] * self.chans[2][i] + bm[i])).collect()
    }
}

fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| *x as f64 * *y as f64).sum()
}

/// Solves `(L + λD) α = λ D t` by preconditioned conjugate gradients from `x0`.
fn solve(img: &RgbImage, trimap: &[f32], x0: Vec<f32>, p: &MattingParams, iterations: usize) -> Vec<f32> {
    let n = img.w * img.h;
    let win = Windows::new(img, p.radius, p.epsilon);
    // The Laplacian above is normalised (≈ 1 per unit), so λ is scaled the same way.
    let lambda = p.lambda / ((2 * p.radius + 1) * (2 * p.radius + 1)) as f32;
    let d: Vec<f32> = trimap.iter().map(|t| if is_known(*t) { lambda } else { 0.0 }).collect();
    let target: Vec<f32> = trimap.iter().map(|t| if *t >= 0.5 { 1.0 } else { 0.0 }).collect();
    let apply = |x: &[f32]| -> Vec<f32> {
        let lx = win.laplacian(x);
        (0..n).map(|i| lx[i] + d[i] * x[i]).collect()
    };
    let rhs: Vec<f32> = (0..n).map(|i| d[i] * target[i]).collect();
    // Jacobi preconditioner: diag(L) ≈ 1 − 1/|w| (normalised) plus the data term.
    let wn = ((2 * p.radius + 1) * (2 * p.radius + 1)) as f32;
    let m_inv: Vec<f32> = d.iter().map(|di| 1.0 / (1.0 - 1.0 / wn + di)).collect();
    let mut x = x0;
    let ax = apply(&x);
    let mut r: Vec<f32> = (0..n).map(|i| rhs[i] - ax[i]).collect();
    let mut z: Vec<f32> = (0..n).map(|i| r[i] * m_inv[i]).collect();
    let mut pdir = z.clone();
    let mut rz = dot(&r, &z);
    let tol = 1e-7 * dot(&rhs, &rhs).max(1e-12);
    for _ in 0..iterations {
        if dot(&r, &r) < tol {
            break;
        }
        let ap = apply(&pdir);
        let pap = dot(&pdir, &ap);
        if pap.abs() < 1e-30 {
            break;
        }
        let alpha = (rz / pap) as f32;
        for i in 0..n {
            x[i] += alpha * pdir[i];
            r[i] -= alpha * ap[i];
            z[i] = r[i] * m_inv[i];
        }
        let rz_new = dot(&r, &z);
        let beta = (rz_new / rz.max(1e-300)) as f32;
        rz = rz_new;
        for i in 0..n {
            pdir[i] = z[i] + beta * pdir[i];
        }
    }
    x.iter_mut().for_each(|v| *v = v.clamp(0.0, 1.0));
    x
}

fn downsample(img: &RgbImage) -> RgbImage {
    let (w, h) = (img.w.div_ceil(2), img.h.div_ceil(2));
    RgbImage::from_fn(w, h, |x, y| {
        let mut s = [0.0f32; 3];
        let mut n = 0.0;
        for dy in 0..2 {
            for dx in 0..2 {
                let (sx, sy) = (2 * x + dx, 2 * y + dy);
                if sx < img.w && sy < img.h {
                    let p = img.px[sy * img.w + sx];
                    s[0] += p[0];
                    s[1] += p[1];
                    s[2] += p[2];
                    n += 1.0;
                }
            }
        }
        [s[0] / n, s[1] / n, s[2] / n]
    })
}

/// Trimap at half size: known only where all contributing pixels agree.
fn downsample_trimap(t: &[f32], w: usize, h: usize) -> Vec<f32> {
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = vec![0.5f32; cw * ch];
    for y in 0..ch {
        for x in 0..cw {
            let mut vals = Vec::with_capacity(4);
            for dy in 0..2 {
                for dx in 0..2 {
                    let (sx, sy) = (2 * x + dx, 2 * y + dy);
                    if sx < w && sy < h {
                        vals.push(t[sy * w + sx]);
                    }
                }
            }
            out[y * cw + x] = if vals.iter().all(|v| *v <= 0.02) {
                0.0
            } else if vals.iter().all(|v| *v >= 0.98) {
                1.0
            } else {
                0.5
            };
        }
    }
    out
}

fn upsample(a: &[f32], cw: usize, ch: usize, w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        let fy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (ch - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(ch - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (cw - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(cw - 1);
            let top = a[y0 * cw + x0] * (1.0 - tx) + a[y0 * cw + x1] * tx;
            let bot = a[y1 * cw + x0] * (1.0 - tx) + a[y1 * cw + x1] * tx;
            out[y * w + x] = top * (1.0 - ty) + bot * ty;
        }
    }
    out
}

/// Alpha matte for `img` (colours 0–1) from `trimap` (`w × h`). Known pixels keep their trimap
/// value; the unknown band is solved. Large images are solved coarse to fine.
pub fn closed_form_matting(img: &RgbImage, trimap: &[f32], p: &MattingParams) -> Vec<f32> {
    let (w, h) = (img.w, img.h);
    assert_eq!(trimap.len(), w * h);
    if w < 4 || h < 4 || !trimap.iter().any(|t| !is_known(*t)) {
        return trimap.iter().map(|t| if *t >= 0.5 { 1.0 } else { 0.0 }).collect();
    }
    // Initial guess: coarse solution when the image is big, else the guided-filtered trimap.
    let x0 = if w.max(h) > 400 {
        let small = downsample(img);
        let st = downsample_trimap(trimap, w, h);
        let coarse = closed_form_matting(&small, &st, &MattingParams { iterations: p.iterations, ..*p });
        upsample(&coarse, small.w, small.h, w, h)
    } else {
        guided_filter_color(img, trimap, (p.radius * 4).max(4), 1e-4).into_iter().map(|v| v.clamp(0.0, 1.0)).collect()
    };
    let x0: Vec<f32> = x0.iter().zip(trimap).map(|(x, t)| if is_known(*t) { if *t >= 0.5 { 1.0 } else { 0.0 } } else { *x }).collect();
    let iters = if w.max(h) > 400 { p.iterations / 2 } else { p.iterations };
    let a = solve(img, trimap, x0, p, iters.max(10));
    a.iter().zip(trimap).map(|(a, t)| if is_known(*t) { if *t >= 0.5 { 1.0 } else { 0.0 } } else { *a }).collect()
}

/// [`closed_form_matting`] on overlapping tiles around the unknown band only, in parallel: cost
/// follows the length of the edge, not the image size. Tiles are `tile` px with `margin` px of
/// overlapping context; each tile's centre is kept.
pub fn closed_form_matting_tiled(img: &RgbImage, trimap: &[f32], p: &MattingParams, tile: usize, margin: usize) -> Vec<f32> {
    use rayon::prelude::*;
    let (w, h) = (img.w, img.h);
    let mut out: Vec<f32> = trimap.iter().map(|t| if *t >= 0.5 { 1.0 } else { 0.0 }).collect();
    let tile = tile.max(32);
    let mut jobs = Vec::new();
    for ty in (0..h).step_by(tile) {
        for tx in (0..w).step_by(tile) {
            let (x1, y1) = ((tx + tile).min(w), (ty + tile).min(h));
            let has_unknown = (ty..y1).any(|y| trimap[y * w + tx..y * w + x1].iter().any(|t| !is_known(*t)));
            if has_unknown {
                jobs.push((tx, ty, x1, y1));
            }
        }
    }
    let solved: Vec<((usize, usize, usize, usize), Vec<f32>, usize, usize, usize)> = jobs
        .par_iter()
        .map(|&(tx, ty, x1, y1)| {
            let (cx0, cy0) = (tx.saturating_sub(margin), ty.saturating_sub(margin));
            let (cx1, cy1) = ((x1 + margin).min(w), (y1 + margin).min(h));
            let (cw, ch) = (cx1 - cx0, cy1 - cy0);
            let sub = RgbImage::from_fn(cw, ch, |x, y| img.px[(cy0 + y) * w + cx0 + x]);
            let st: Vec<f32> = (0..ch).flat_map(|y| trimap[(cy0 + y) * w + cx0..(cy0 + y) * w + cx1].to_vec()).collect();
            let a = closed_form_matting(&sub, &st, p);
            ((tx, ty, x1, y1), a, cx0, cy0, cw)
        })
        .collect();
    for ((tx, ty, x1, y1), a, cx0, cy0, cw) in solved {
        for y in ty..y1 {
            for x in tx..x1 {
                out[y * w + x] = a[(y - cy0) * cw + (x - cx0)];
            }
        }
    }
    out
}

/// A trimap from a soft mask: definitely foreground where the mask, eroded by `band`, is ≥ 0.5;
/// definitely background where the dilated mask is < 0.5; unknown in between.
pub fn trimap_from_mask(mask: &[f32], w: usize, h: usize, band: usize) -> Vec<f32> {
    let fg = crate::matting::morph(mask, w, h, band, false);
    let bg = crate::matting::morph(mask, w, h, band, true);
    (0..w * h)
        .map(|i| {
            if fg[i] >= 0.5 {
                1.0
            } else if bg[i] < 0.5 {
                0.0
            } else {
                0.5
            }
        })
        .collect()
}

/// Foreground (and background) colours satisfying `I = αF + (1−α)B` with smooth F and B, coarse
/// to fine (Germer et al. 2020). Returns F, which is what a cutout should keep.
pub fn estimate_foreground(img: &RgbImage, alpha: &[f32]) -> RgbImage {
    let (w0, h0) = (img.w, img.h);
    let levels = ((w0.max(h0) as f64).log2().ceil() as usize).max(1);
    let (reg, grad) = (1e-5f32, 1.0f32);
    let mut f: Vec<[f32; 3]> = Vec::new();
    let mut b: Vec<[f32; 3]> = Vec::new();
    let (mut pw, mut ph) = (0usize, 0usize);
    for level in 0..=levels {
        let t = level as f64 / levels as f64;
        let w = ((w0 as f64).powf(t).round() as usize).clamp(1, w0);
        let h = ((h0 as f64).powf(t).round() as usize).clamp(1, h0);
        let im = resize_rgb(img, w, h);
        let al = resize_gray(alpha, w0, h0, w, h);
        // Previous level's estimates, resized (nearest), or the image mean at the first level.
        if f.is_empty() {
            let mean = im.px.iter().fold([0.0f32; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]).map(|v| v / (w * h) as f32);
            f = vec![mean; w * h];
            b = vec![mean; w * h];
        } else {
            f = resize_nearest(&f, pw, ph, w, h);
            b = resize_nearest(&b, pw, ph, w, h);
        }
        let iterations = if w.max(h) <= 32 { 10 } else { 2 };
        for _ in 0..iterations {
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    let a0 = al[i];
                    let a1 = 1.0 - a0;
                    let (mut a00, a01, mut a11) = (a0 * a0, a0 * a1, a1 * a1);
                    let px = im.px[i];
                    let mut b0 = [a0 * px[0], a0 * px[1], a0 * px[2]];
                    let mut b1 = [a1 * px[0], a1 * px[1], a1 * px[2]];
                    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let j = ny as usize * w + nx as usize;
                        let an = reg + grad * (a0 - al[j]).abs();
                        a00 += an;
                        a11 += an;
                        for c in 0..3 {
                            b0[c] += an * f[j][c];
                            b1[c] += an * b[j][c];
                        }
                    }
                    let det = a00 * a11 - a01 * a01;
                    let inv = if det.abs() < 1e-12 { 0.0 } else { 1.0 / det };
                    let (m00, m01, m11) = (inv * a11, -inv * a01, inv * a00);
                    for c in 0..3 {
                        f[i][c] = (m00 * b0[c] + m01 * b1[c]).clamp(0.0, 1.0);
                        b[i][c] = (m01 * b0[c] + m11 * b1[c]).clamp(0.0, 1.0);
                    }
                }
            }
        }
        pw = w;
        ph = h;
    }
    RgbImage { w: w0, h: h0, px: f }
}

fn resize_nearest<T: Copy>(src: &[T], sw: usize, sh: usize, w: usize, h: usize) -> Vec<T> {
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let sy = (y * sh / h).min(sh - 1);
        for x in 0..w {
            out.push(src[sy * sw + (x * sw / w).min(sw - 1)]);
        }
    }
    out
}

/// Area-average resize (downscale) or nearest (upscale), good enough for the pyramid.
fn resize_rgb(img: &RgbImage, w: usize, h: usize) -> RgbImage {
    if w == img.w && h == img.h {
        return RgbImage { w, h, px: img.px.clone() };
    }
    let mut px = vec![[0.0f32; 3]; w * h];
    for y in 0..h {
        let (y0, y1) = (y * img.h / h, ((y + 1) * img.h / h).max(y * img.h / h + 1).min(img.h));
        for x in 0..w {
            let (x0, x1) = (x * img.w / w, ((x + 1) * img.w / w).max(x * img.w / w + 1).min(img.w));
            let mut s = [0.0f32; 3];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = img.px[sy * img.w + sx];
                    s[0] += p[0];
                    s[1] += p[1];
                    s[2] += p[2];
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as f32;
            px[y * w + x] = [s[0] / n, s[1] / n, s[2] / n];
        }
    }
    RgbImage { w, h, px }
}

fn resize_gray(a: &[f32], sw: usize, sh: usize, w: usize, h: usize) -> Vec<f32> {
    if w == sw && h == sh {
        return a.to_vec();
    }
    let img = RgbImage { w: sw, h: sh, px: a.iter().map(|v| [*v, 0.0, 0.0]).collect() };
    resize_rgb(&img, w, h).px.into_iter().map(|p| p[0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A soft-edged disc over a textured background: the matte inside the unknown band must follow
    /// the true coverage much better than the binary trimap guess.
    fn scene(w: usize, h: usize) -> (RgbImage, Vec<f32>) {
        let (cx, cy, r) = (w as f32 / 2.0, h as f32 / 2.0, w as f32 / 4.0);
        let mut alpha = vec![0.0f32; w * h];
        let img = RgbImage::from_fn(w, h, |x, y| {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let a = ((r + 3.0 - d) / 6.0).clamp(0.0, 1.0);
            alpha[y * w + x] = a;
            let fg = [0.9, 0.3, 0.2];
            let bg = [0.1 + 0.05 * ((x / 7) % 2) as f32, 0.4, 0.8 - 0.05 * ((y / 5) % 2) as f32];
            [0, 1, 2].map(|c| a * fg[c] + (1.0 - a) * bg[c])
        });
        (img, alpha)
    }

    #[test]
    fn closed_form_recovers_a_soft_edge() {
        let (w, h) = (96, 96);
        let (img, truth) = scene(w, h);
        let hard: Vec<f32> = truth.iter().map(|a| if *a >= 0.5 { 1.0 } else { 0.0 }).collect();
        let tri = trimap_from_mask(&hard, w, h, 5);
        let a = closed_form_matting(&img, &tri, &MattingParams::default());
        let err = |m: &[f32]| m.iter().zip(&truth).zip(&tri).filter(|(_, t)| !is_known(**t)).map(|((x, y), _)| (x - y).abs()).sum::<f32>();
        let unknown = tri.iter().filter(|t| !is_known(**t)).count() as f32;
        let (e_hard, e_mat) = (err(&hard) / unknown, err(&a) / unknown);
        assert!(e_mat < e_hard * 0.5, "matting error {e_mat} vs hard mask {e_hard}");
        // Known pixels are kept exactly.
        assert!(a.iter().zip(&tri).all(|(x, t)| !is_known(*t) || (*x - if *t >= 0.5 { 1.0 } else { 0.0 }).abs() < 1e-6));
    }

    #[test]
    fn large_images_go_coarse_to_fine() {
        let (w, h) = (500, 420);
        let (img, truth) = scene(w, h);
        let hard: Vec<f32> = truth.iter().map(|a| if *a >= 0.5 { 1.0 } else { 0.0 }).collect();
        let tri = trimap_from_mask(&hard, w, h, 6);
        let a = closed_form_matting(&img, &tri, &MattingParams { iterations: 60, ..Default::default() });
        let unknown = tri.iter().filter(|t| !is_known(**t)).count() as f32;
        let e: f32 = a.iter().zip(&truth).zip(&tri).filter(|(_, t)| !is_known(**t)).map(|((x, y), _)| (x - y).abs()).sum::<f32>() / unknown;
        assert!(e < 0.2, "{e}");
    }

    #[test]
    fn foreground_estimation_removes_background_spill() {
        let (w, h) = (64, 64);
        let (img, alpha) = scene(w, h);
        let f = estimate_foreground(&img, &alpha);
        // In the soft band the estimated foreground is close to the true foreground colour.
        let mut worst: f32 = 0.0;
        for i in 0..w * h {
            if (0.3..0.7).contains(&alpha[i]) {
                let d = (f.px[i][0] - 0.9).abs() + (f.px[i][2] - 0.2).abs();
                worst = worst.max(d);
            }
        }
        assert!(worst < 0.35, "{worst}");
    }
}
