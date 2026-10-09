//! The fast local Laplacian filter (Paris, Hasinoff & Kautz 2011; Aubry et al. 2014), ported from
//! darktable's `src/common/locallaplacian.c` (`local_laplacian_internal` in its regular mode:
//! `ll_pad_input` replication padding by 2^(levels−1), `gauss_reduce` / `gauss_expand` /
//! `ll_expand_gaussian` / `ll_fill_boundary1/2`, `apply_curve` / `curve_scalar` with its quadratic
//! Bézier shadows / highlights blend and the `dt_fast_expf` midtone detail term, γ sampled at
//! `(k + ½)/n`, output Laplacians interpolated between the two γ bracketing the input's Gaussian
//! level); GPL-3.0-or-later, see `docs/PORTS.md`. [`local_laplacian`] reproduces upstream's output
//! (fixtures from the C code: `tests/fixtures/darktable`).
//!
//! Extensions, each off in the faithful entry point (documented in `licenses/darktable-NOTICE.md`):
//! * the number of γ samples is a parameter (upstream: 6). On log luminance a finer grid is needed
//!   for small σ; more samples only make the interpolation more accurate;
//! * the γ loop accumulates each sample's Laplacians as it goes (upstream keeps every sample's
//!   pyramid in memory): same sums in the same order, a fraction of the memory;
//! * optional per-level weights (a band of scales: Clarity works on mid-size detail only);
//! * a second remapping, [`Remap::Tone`] (ours): a tone curve `f` (EV) applied with the value at the
//!   neighbourhood's level γ near γ — so detail rides along unchanged — and with the pixel's own
//!   value across edges, blended by a Gaussian of `x − γ`; with it the coarsest level is remapped
//!   like the others (upstream copies the input's), so the curve moves the global level too.
//!
//! CPU reference; native Clarity kernels are in lc-gpu/wgsl/primary.wgsl. Unused SIMD
//! overread lanes are clamped.

use lightcraft_raster::Plane;

use crate::for_rows;

/// darktable's `dl`: width/height of pyramid level `level`.
#[inline]
pub fn dl(size: usize, level: usize) -> usize {
    let mut s = size;
    for _ in 0..level {
        s = (s - 1) / 2 + 1;
    }
    s
}

/// darktable's `dt_fast_expf` (bit trick; meant for x ≤ 0).
#[inline]
pub fn fast_expf(x: f32) -> f32 {
    let k0 = (1_065_353_216.0f32 + x * 11_401_300.0f32) as i32;
    f32::from_bits(k0.max(0) as u32)
}

/// darktable's `ll_expand_gaussian` (needs 1 ≤ i < wd−1 and the matching j).
#[inline]
pub fn expand_gaussian(coarse: &[f32], i: usize, j: usize, wd: usize) -> f32 {
    let cw = (wd - 1) / 2 + 1;
    let ind = (j / 2) * cw + i / 2;
    let c = |k: usize| coarse[k];
    match (i & 1) + 2 * (j & 1) {
        0 => {
            let s = 6.0f32 * (c(ind - cw) + c(ind - 1) + 6.0f32 * c(ind) + c(ind + 1) + c(ind + cw))
                + c(ind - cw - 1)
                + c(ind - cw + 1)
                + c(ind + cw - 1)
                + c(ind + cw + 1);
            (4.0f64 / 256.0 * s as f64) as f32
        }
        1 => {
            let s = 24.0f64 * (c(ind) + c(ind + 1)) as f64 + 4.0f64 * (c(ind - cw) + c(ind - cw + 1) + c(ind + cw) + c(ind + cw + 1)) as f64;
            (4.0f64 / 256.0 * s) as f32
        }
        2 => {
            let s = 24.0f64 * (c(ind) + c(ind + cw)) as f64 + 4.0f64 * (c(ind - 1) + c(ind + 1) + c(ind + cw - 1) + c(ind + cw + 1)) as f64;
            (4.0f64 / 256.0 * s) as f32
        }
        _ => 0.25f32 * (c(ind) + c(ind + 1) + c(ind + cw) + c(ind + cw + 1)),
    }
}

fn fill_boundary1(p: &mut [f32], wd: usize, ht: usize) {
    for j in 1..ht - 1 {
        p[j * wd] = p[j * wd + 1];
        p[j * wd + wd - 1] = p[j * wd + wd - 2];
    }
    p.copy_within(wd..2 * wd, 0);
    p.copy_within(wd * (ht - 2)..wd * (ht - 1), wd * (ht - 1));
}

fn fill_boundary2(p: &mut [f32], wd: usize, ht: usize) {
    for j in 1..ht - 1 {
        p[j * wd] = p[j * wd + 1];
        if wd & 1 == 1 {
            p[j * wd + wd - 1] = p[j * wd + wd - 2];
        } else {
            p[j * wd + wd - 2] = p[j * wd + wd - 3];
            p[j * wd + wd - 1] = p[j * wd + wd - 2];
        }
    }
    p.copy_within(wd..2 * wd, 0);
    if ht & 1 == 0 {
        p.copy_within(wd * (ht - 3)..wd * (ht - 2), wd * (ht - 2));
    }
    p.copy_within(wd * (ht - 2)..wd * (ht - 1), wd * (ht - 1));
}

/// darktable's `_convolve_14641_vert` of one column (`r` the five rows' values).
#[inline]
fn conv_vert(r: [f32; 5]) -> f32 {
    let r0 = r[0] + r[4];
    let r1 = r[1] + r[2] + r[3];
    let r0 = r0 + r[2] + r[2];
    let t = r1 * 4.0;
    r0 + t
}

/// darktable's `gauss_reduce`: blur with 1 4 6 4 1 and keep every other pixel; the one-pixel
/// border is copied from its neighbours.
pub fn gauss_reduce(input: &[f32], wd: usize, ht: usize) -> Vec<f32> {
    let (cw, ch) = ((wd - 1) / 2 + 1, (ht - 1) / 2 + 1);
    let mut coarse = vec![0.0f32; cw * ch];
    if ch > 2 && cw > 2 {
        let (_, inner) = coarse.split_at_mut(cw);
        let inner = &mut inner[..cw * (ch - 2)];
        for_rows(inner, cw, |jj, out| {
            let j = jj + 1;
            let base0 = 2 * (j - 1) * wd;
            let col_v = |c: usize| {
                let c = c.min(wd - 1);
                conv_vert([input[base0 + c], input[base0 + wd + c], input[base0 + 2 * wd + c], input[base0 + 3 * wd + c], input[base0 + 4 * wd + c]])
            };
            let mut b = 0usize;
            let mut left = [col_v(0), col_v(1), col_v(2), col_v(3)];
            let mut col = 0usize;
            while col + 3 < cw {
                b += 4;
                let right = [col_v(b), col_v(b + 1), col_v(b + 2), col_v(b + 3)];
                let conv = [left[0], left[1] * 4.0, left[2] * 6.0, left[3] * 4.0];
                out[col + 1] = (conv[0] + conv[1] + conv[2] + conv[3] + right[0]) / 256.0;
                out[col + 2] = (left[2] + 4.0 * (left[3] + right[1]) + 6.0 * right[0] + right[2]) / 256.0;
                left = right;
                col += 2;
            }
            if cw % 2 == 1 {
                b += 4;
                let right = col_v(b);
                let conv = [left[0], left[1] * 4.0, left[2] * 6.0, left[3] * 4.0];
                out[cw - 2] = (conv[0] + conv[1] + conv[2] + conv[3] + right) / 256.0;
            }
        });
    }
    if ch >= 2 && cw >= 2 {
        fill_boundary1(&mut coarse, cw, ch);
    }
    coarse
}

/// darktable's `gauss_expand` of `coarse` to `wd × ht`.
pub fn gauss_expand(coarse: &[f32], wd: usize, ht: usize) -> Vec<f32> {
    let mut fine = vec![0.0f32; wd * ht];
    let (xe, ye) = ((wd - 1) & !1, (ht - 1) & !1);
    for_rows(&mut fine, wd, |j, row| {
        if j >= 1 && j < ye {
            for i in 1..xe {
                row[i] = expand_gaussian(coarse, i, j, wd);
            }
        }
    });
    fill_boundary2(&mut fine, wd, ht);
    fine
}

/// darktable's `ll_laplacian` at fine (i, j).
#[inline]
fn laplacian(coarse: &[f32], fine: &[f32], i: usize, j: usize, wd: usize, ht: usize) -> f32 {
    let ci = i.clamp(1, ((wd - 1) & !1) - 1);
    let cj = j.clamp(1, ((ht - 1) & !1) - 1);
    fine[j * wd + i] - expand_gaussian(coarse, ci, cj, wd)
}

/// darktable's `curve_scalar`.
#[inline]
pub fn curve_scalar(x: f32, g: f32, sigma: f32, shadows: f32, highlights: f32, clarity: f32) -> f32 {
    let c = x - g;
    let mut val;
    if c > 2.0 * sigma {
        val = g + sigma + shadows * (c - sigma);
    } else if c < -2.0 * sigma {
        val = g - sigma + highlights * (c + sigma);
    } else if c > 0.0 {
        let t = (c / (2.0 * sigma)).clamp(0.0, 1.0);
        let t2 = t * t;
        let mt = 1.0 - t;
        val = g + sigma * 2.0 * mt * t + t2 * (sigma + sigma * shadows);
    } else {
        let t = (-c / (2.0 * sigma)).clamp(0.0, 1.0);
        let t2 = t * t;
        let mt = 1.0 - t;
        val = g - sigma * 2.0 * mt * t + t2 * (-sigma - sigma * highlights);
    }
    val += clarity * c * fast_expf(-c * c / (2.0 * sigma * sigma / 3.0));
    val
}

/// Entries per EV of a [`Remap::Tone`] curve, and its domain (EV).
pub const CURVE_PER_EV: usize = 32;
pub const CURVE_MIN: f32 = -16.0;
pub const CURVE_MAX: f32 = 12.0;
pub const CURVE_N: usize = ((CURVE_MAX - CURVE_MIN) as usize) * CURVE_PER_EV + 1;

/// A tone curve (EV → EV change) sampled for [`Remap::Tone`], linearly interpolated.
#[inline]
pub fn curve_at(lut: &[f32], ev: f32) -> f32 {
    let t = (ev.clamp(CURVE_MIN, CURVE_MAX) - CURVE_MIN) * CURVE_PER_EV as f32;
    let i = (t as usize).min(lut.len() - 2);
    let f = t - i as f32;
    lut[i] + (lut[i + 1] - lut[i]) * f
}

/// The remapping of each γ sample.
#[derive(Clone, Debug, PartialEq)]
pub enum Remap {
    /// darktable's `curve_scalar`.
    Darktable { sigma: f32, shadows: f32, highlights: f32, clarity: f32 },
    /// A tone curve `lut` (EV → EV change) on a plane that maps EV `e` to `(e − lo) / range`: the
    /// remapped value is `x + (f(γ)·K + f(x)·(1 − K)) / range` with `K = exp(−1.5 (x − γ)² / σ²)`.
    Tone { sigma: f32, lut: Vec<f32>, lo: f32, range: f32 },
}

impl Remap {
    #[inline]
    pub fn apply(&self, x: f32, g: f32) -> f32 {
        match self {
            Remap::Darktable { sigma, shadows, highlights, clarity } => curve_scalar(x, g, *sigma, *shadows, *highlights, *clarity),
            Remap::Tone { sigma, lut, lo, range } => {
                let t = x - g;
                let k = (-1.5 * t * t / (sigma * sigma)).exp();
                let fg = curve_at(lut, lo + g * range);
                let fx = curve_at(lut, lo + x * range);
                x + (fg * k + fx * (1.0 - k)) / range
            }
        }
    }
}

/// Parameters of [`local_laplacian_with`].
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    pub remap: Remap,
    /// Number of γ samples (darktable: 6).
    pub num_gamma: usize,
    /// Weight of each Laplacian level's change (`None`: all 1, as upstream).
    pub level_weights: Option<Vec<f32>>,
    /// Remap the coarsest level too (upstream: it is the input's).
    pub remap_residual: bool,
}

/// Number of pyramid levels darktable uses for a `wd × ht` image.
pub fn num_levels(wd: usize, ht: usize) -> usize {
    let m = wd.min(ht).max(1);
    ((usize::BITS - 1 - m.leading_zeros()) as usize).max(1)
}

/// darktable's `local_laplacian_internal` (regular mode) on a one-channel plane in 0..1 with its
/// own `curve_scalar` and 6 γ samples: the faithful port.
pub fn local_laplacian(input: &Plane, sigma: f32, shadows: f32, highlights: f32, clarity: f32) -> Plane {
    local_laplacian_with(
        input,
        &Params { remap: Remap::Darktable { sigma, shadows, highlights, clarity }, num_gamma: 6, level_weights: None, remap_residual: false },
    )
}

/// [`local_laplacian`] with the extensions of [`Params`].
pub fn local_laplacian_with(input: &Plane, p: &Params) -> Plane {
    let (wd, ht) = (input.width, input.height);
    if wd < 4 || ht < 4 {
        return input.clone();
    }
    let nl = num_levels(wd, ht).min(30);
    let last = nl - 1;
    let max_supp = 1usize << last;
    let (w, h) = (2 * max_supp + wd, 2 * max_supp + ht);
    // pad by replication
    let mut pad0 = vec![0.0f32; w * h];
    for_rows(&mut pad0[max_supp * w..(max_supp + ht) * w], w, |j, row| {
        let src = input.row(j);
        for (i, o) in row.iter_mut().enumerate() {
            *o = src[i.saturating_sub(max_supp).min(wd - 1)];
        }
    });
    replicate_rows(&mut pad0, w, h, max_supp);
    // Gaussian pyramid of the padded input (the coarsest level goes straight to the output)
    let mut padded: Vec<Vec<f32>> = vec![pad0];
    for l in 1..last {
        let next = gauss_reduce(&padded[l - 1], dl(w, l - 1), dl(h, l - 1));
        padded.push(next);
    }
    let top_in = gauss_reduce(&padded[last - 1], dl(w, last - 1), dl(h, last - 1));
    let ng = p.num_gamma.max(2);
    let gamma: Vec<f32> = (0..ng).map(|k| (k as f32 + 0.5) / ng as f32).collect();
    // per level: each pixel's bracketing γ (hi) and interpolation weight a
    let bracket = |v: f32| {
        let mut hi = 1;
        while hi < ng - 1 && gamma[hi] <= v {
            hi += 1;
        }
        let lo = hi - 1;
        (hi, ((v - gamma[lo]) / (gamma[hi] - gamma[lo])).clamp(0.0, 1.0))
    };
    let mut acc: Vec<Vec<f32>> = (0..=last).map(|l| vec![0.0f32; dl(w, l) * dl(h, l)]).collect();
    for k in 0..ng {
        // remapped image (inner part, then replicated borders) and its Gaussian pyramid
        let mut b0 = vec![0.0f32; w * h];
        let g = gamma[k];
        for_rows(&mut b0[max_supp * w..(h - max_supp) * w], w, |jj, row| {
            let src = &padded[0][(jj + max_supp) * w..(jj + max_supp + 1) * w];
            for i in max_supp..w - max_supp {
                row[i] = p.remap.apply(src[i], g);
            }
            let (l, r) = (row[max_supp], row[w - max_supp - 1]);
            row[..max_supp].fill(l);
            row[w - max_supp..].fill(r);
        });
        replicate_rows(&mut b0, w, h, max_supp);
        let mut buf: Vec<Vec<f32>> = vec![b0];
        for l in 1..=last {
            let next = gauss_reduce(&buf[l - 1], dl(w, l - 1), dl(h, l - 1));
            buf.push(next);
        }
        for l in (0..last).rev() {
            let (pw, ph) = (dl(w, l), dl(h, l));
            let wl = p.level_weights.as_ref().map_or(1.0, |v| v.get(l).copied().unwrap_or(0.0));
            if wl == 0.0 {
                continue;
            }
            let (fine, coarse, pl) = (&buf[l], &buf[l + 1], &padded[l]);
            for_rows(&mut acc[l], pw, |j, row| {
                for (i, o) in row.iter_mut().enumerate() {
                    let (hi, a) = bracket(pl[j * pw + i]);
                    let wk = if k == hi - 1 {
                        1.0 - a
                    } else if k == hi {
                        a
                    } else {
                        continue;
                    };
                    let lap = laplacian(coarse, fine, i, j, pw, ph);
                    *o += lap * wk * wl;
                }
            });
        }
        if p.remap_residual {
            let top = &buf[last];
            for (o, (v, f)) in acc[last].iter_mut().zip(top_in.iter().zip(top)) {
                let (hi, a) = bracket(*v);
                if k == hi - 1 {
                    *o += f * (1.0 - a);
                } else if k == hi {
                    *o += f * a;
                }
            }
        }
    }
    // assemble the output pyramid coarse to fine
    let mut out = if p.remap_residual { std::mem::take(&mut acc[last]) } else { top_in };
    for l in (0..last).rev() {
        let (pw, ph) = (dl(w, l), dl(h, l));
        let mut fine = gauss_expand(&out, pw, ph);
        for (f, a) in fine.iter_mut().zip(&acc[l]) {
            *f += *a;
        }
        out = fine;
    }
    Plane::from_fn(wd, ht, |i, j| out[(j + max_supp) * w + max_supp + i])
}

/// darktable's `pad_by_replication`: the `pad` top rows copy row `pad`, the bottom ones row
/// `h − pad − 1`.
fn replicate_rows(buf: &mut [f32], w: usize, h: usize, pad: usize) {
    for j in 0..pad {
        buf.copy_within(pad * w..(pad + 1) * w, j * w);
        buf.copy_within((h - pad - 1) * w..(h - pad) * w, (h - pad + j) * w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_expf_is_darktables() {
        assert_eq!(fast_expf(0.0), f32::from_bits(0x3f80_0000));
        assert!((fast_expf(-1.0) - (-1.0f32).exp()).abs() < 0.07);
        assert_eq!(fast_expf(-200.0), 0.0);
    }

    #[test]
    fn identity_curve_keeps_the_image() {
        let v = Plane::from_fn(50, 37, |x, y| 0.3 + 0.2 * ((x as f32 * 0.3).sin() * (y as f32 * 0.2).cos()) + if x > 25 { 0.3 } else { 0.0 });
        let o = local_laplacian(&v, 0.2, 1.0, 1.0, 0.0);
        let err = v.data.iter().zip(&o.data).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(err < 2e-4, "{err}");
    }

    #[test]
    fn a_constant_tone_shift_moves_everything() {
        let v = Plane::from_fn(40, 30, |x, y| 0.4 + 0.1 * ((x + y) % 3) as f32 + if x > 20 { 0.3 } else { 0.0 });
        let lut = vec![0.5f32; CURVE_N];
        let p = Params { remap: Remap::Tone { sigma: 0.05, lut, lo: -8.0, range: 16.0 }, num_gamma: 24, level_weights: None, remap_residual: true };
        let o = local_laplacian_with(&v, &p);
        for (a, b) in v.data.iter().zip(&o.data) {
            assert!(((b - a) * 16.0 - 0.5).abs() < 2e-3, "{a} {b}");
        }
    }
}
