//! darktable color equalizer: UCS22, two-channel chromaticity-guided filters and Scharr
//! halo suppression (733bd69f, GPL-3+). Extensions: node positions are Lightroom's eight
//! hue bands, interpolated shape-preserving instead of upstream's periodic RBF; brightness
//! changes exponentially within soft bounds; pixel radii scale with the original 6000px long
//! edge. No fast/draft bypass of the colour filters.
use crate::{eigf, ucs};
use li_raster::RgbFrame;
use lightcraft_develop::{DevelopSettings, MIXER_HUES};
use std::f64::consts::{PI, TAU};
use std::sync::OnceLock;
pub fn hues() -> &'static [f32; 8] {
    static H: OnceLock<[f32; 8]> = OnceLock::new();
    H.get_or_init(|| MIXER_HUES.map(ucs::ucs_hue_of_srgb_hue))
}

/// A periodic, shape-preserving hue curve through the band nodes, sampled at 512 hues from −π:
/// a monotone cubic (PCHIP slopes) between neighbouring nodes, so it never leaves the range of
/// the two nodes around a hue. Upstream's cosine RBF assumes evenly spaced nodes; on the uneven
/// Lightroom bands it overshoots well past a node, beyond the slider's range.
pub fn hue_curve(nodes: [f32; 8], positions: [f32; 8]) -> Vec<f32> {
    let mut order: [usize; 8] = std::array::from_fn(|i| i);
    order.sort_by(|&a, &b| positions[a].total_cmp(&positions[b]));
    let x = order.map(|i| positions[i] as f64);
    let y = order.map(|i| nodes[i] as f64);
    let n = x.len();
    let width = |k: usize| if k + 1 < n { x[k + 1] - x[k] } else { x[0] + TAU - x[k] };
    let secant = |k: usize| (y[(k + 1) % n] - y[k]) / width(k);
    let slope: [f64; 8] = std::array::from_fn(|k| {
        let before = (k + n - 1) % n;
        let (d0, d1) = (secant(before), secant(k));
        if d0 * d1 <= 0. {
            return 0.;
        }
        let (h0, h1) = (width(before), width(k));
        let (w0, w1) = (2. * h1 + h0, h1 + 2. * h0);
        (w0 + w1) / (w0 / d0 + w1 / d1)
    });
    (0..512)
        .map(|i| {
            let hue = i as f64 * TAU / 512. - PI;
            let k = (0..n).rev().find(|&k| x[k] <= hue).unwrap_or(n - 1);
            let h = width(k);
            let t = (hue - x[k]).rem_euclid(TAU) / h;
            let (t2, t3) = (t * t, t * t * t);
            let v = (2. * t3 - 3. * t2 + 1.) * y[k]
                + (t3 - 2. * t2 + t) * h * slope[k]
                + (-2. * t3 + 3. * t2) * y[(k + 1) % n]
                + (t3 - t2) * h * slope[(k + 1) % n];
            v as f32
        })
        .collect()
}

/// The equalizer's hue tables: hue shift, saturation and brightness factors, and the B&W mix.
/// Each interpolates the bands' offsets from neutral, so an untouched band contributes nothing.
pub fn luts(s: &DevelopSettings) -> [Vec<f32>; 4] {
    let bands = s.mixer.bands();
    let curve = |v: [f32; 8]| hue_curve(v, *hues());
    let factor = |v: [f32; 8]| curve(v).into_iter().map(|d| 1. + d).collect();
    [
        curve(bands.map(|b| b.hue as f32 / 100. * 0.5)),
        factor(bands.map(|b| b.sat as f32 / 200.)),
        factor(bands.map(|b| b.lum as f32 / 200.)),
        curve(s.bw_mix.bands().map(|b| b as f32 / 100.)),
    ]
}

/// Brightness changes in stops of UCS brightness (luminance moves about 1.6 to 1.9 times as
/// far): darktable's factor `1 + 8c` has slope `8/ln 2` stops at neutral. The exponential keeps
/// every colour above black, and [`crate::primary::soft_bound`] holds the most saturated colours at ±100 to
/// about −2.5..+1.2 EV of luminance.
pub const BRIGHTNESS_EV: f32 = 8. / std::f32::consts::LN_2;
pub const BRIGHTNESS_MIN_EV: f32 = -1.5;
pub const BRIGHTNESS_MAX_EV: f32 = 1.0;

fn satweight(s: f32) -> f32 {
    static W: OnceLock<Vec<f32>> = OnceLock::new();
    let lut = W.get_or_init(|| (-4096..=4096).map(|i| (1. / (1. + (-60. * 0.5 * i as f64 / 4096.).exp())) as f32).collect());
    let t = 4096. * (1. + s.clamp(-1., 1. - 1. / 4096.));
    let i = t.floor() as usize;
    lut[i] + (t - i as f32) * (lut[i + 1] - lut[i])
}
fn blur(p: &[f32], w: usize, h: usize, nc: usize, sigma: f32) -> Vec<f32> {
    eigf::gaussian(p, w, h, nc, sigma, &vec![-1e9; nc], &vec![1e9; nc])
}

/// General two-channel guided covariance solve. Upstream prefilter and correction
/// filter differ only in covariance targets and brightness's first blur radius.
pub fn guided_uv(uv: &[f32], input: &[f32], w: usize, h: usize, sigma: f32, eps: f32, brightness: bool) -> Vec<f32> {
    let scaling = (sigma - 1.5).floor().clamp(1., 4.);
    let gs = (sigma / scaling).max(0.2);
    let (dw, dh) = (((w as f32 / scaling) as usize).max(1), ((h as f32 / scaling) as usize).max(1));
    let guide = eigf::interpolate(uv, w, h, dw, dh, 2);
    let target = eigf::interpolate(input, w, h, dw, dh, 2);
    let n = dw * dh;
    let mut cov = vec![0.; n * 4];
    let mut corr = cov.clone();
    for i in 0..n {
        let (u, v) = (guide[2 * i], guide[2 * i + 1]);
        cov[i * 4..i * 4 + 4].copy_from_slice(&[u * u, u * v, u * v, v * v]);
        corr[i * 4..i * 4 + 4].copy_from_slice(&[u * target[2 * i], v * target[2 * i], u * target[2 * i + 1], v * target[2 * i + 1]]);
    }
    let mean = blur(&guide, dw, dh, 2, gs);
    let mut mt = blur(&target, dw, dh, 2, gs);
    if brightness {
        let single: Vec<_> = target.as_chunks::<2>().0.iter().map(|c| c[1]).collect();
        let b = blur(&single, dw, dh, 1, 0.1 * gs);
        for i in 0..n {
            mt[2 * i + 1] = b[i];
        }
    }
    let cov = blur(&cov, dw, dh, 4, gs);
    let corr = blur(&corr, dw, dh, 4, gs);
    let mut a = vec![0.; n * 4];
    let mut b = vec![0.; n * 2];
    for i in 0..n {
        let (u, v) = (mean[2 * i], mean[2 * i + 1]);
        let s = [cov[4 * i] - u * u + eps, cov[4 * i + 1] - u * v, cov[4 * i + 2] - u * v, cov[4 * i + 3] - v * v + eps];
        let det = s[0] * s[3] - s[1] * s[2];
        if det.abs() > 4. * f32::EPSILON {
            for c in 0..2 {
                let cu = corr[4 * i + 2 * c] - u * mt[2 * i + c];
                let cv = corr[4 * i + 2 * c + 1] - v * mt[2 * i + c];
                a[4 * i + 2 * c] = (cu * s[3] - cv * s[1]) / det;
                a[4 * i + 2 * c + 1] = (-cu * s[2] + cv * s[0]) / det;
            }
        }
        for c in 0..2 {
            b[2 * i + c] = mt[2 * i + c] - a[4 * i + 2 * c] * u - a[4 * i + 2 * c + 1] * v;
        }
    }
    let a = eigf::interpolate(&blur(&a, dw, dh, 4, gs), dw, dh, w, h, 4);
    let b = eigf::interpolate(&blur(&b, dw, dh, 2, gs), dw, dh, w, h, 2);
    (0..w * h * 2)
        .map(|k| {
            let i = k / 2;
            let c = k % 2;
            a[4 * i + 2 * c] * uv[2 * i] + a[4 * i + 2 * c + 1] * uv[2 * i + 1] + b[k]
        })
        .collect()
}
fn gradient(s: &[f32], w: usize, h: usize, x: usize, y: usize) -> f32 {
    let x = x.clamp(1, w.saturating_sub(2).max(1));
    let y = y.clamp(1, h.saturating_sub(2).max(1));
    let p = |dx: isize, dy: isize| s[(y as isize + dy).clamp(0, h as isize - 1) as usize * w + (x as isize + dx).clamp(0, w as isize - 1) as usize];
    let gx = 47. / 255. * (p(-1, -1) - p(1, -1) + p(-1, 1) - p(1, 1)) + 162. / 255. * (p(-1, 0) - p(1, 0));
    let gy = 47. / 255. * (p(-1, -1) - p(-1, 1) + p(1, -1) - p(1, 1)) + 162. / 255. * (p(0, -1) - p(0, 1));
    (gx * gx + gy * gy).sqrt()
}
pub fn apply(img: &mut RgbFrame, s: &DevelopSettings, ppl: f64) {
    if s.mixer.is_neutral() && !crate::is_bw(s) {
        return;
    }
    let (w, h, n) = (img.width, img.height, img.len());
    if n == 0 {
        return;
    }
    let lw = ucs::y_to_l_star(1.);
    let scale = (ppl as f32 / 6000.).max(1e-3);
    let mut uv = vec![0.; 2 * n];
    let mut l = vec![0.; n];
    let mut saturation = vec![0.; n];
    crate::for_rows(&mut uv, w * 2, |y, row| {
        for x in 0..w {
            let c = img.get(x, y);
            let xy = ucs::xyz_to_xyy(ucs::mul(&ucs::mats().rgb_to_xyz, c));
            row[2 * x..2 * x + 2].copy_from_slice(&ucs::xy_to_uv(xy[0], xy[1]));
        }
    });
    crate::for_rows(&mut l, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            *v = ucs::y_to_l_star(ucs::mul(&ucs::mats().rgb_to_xyz, img.get(x, y))[1]);
        }
    });
    crate::for_rows(&mut saturation, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            let c = img.get(x, y);
            let min = c[0].min(c[1]).min(c[2]);
            let max = c[0].max(c[1]).max(c[2]);
            let d = max - min;
            *v = if max > 1.525_878_9e-5 && d > 1.525_878_9e-5 { d / max } else { 0. };
        }
    });
    let saturation = blur(&saturation, w, h, 1, scale.max(0.5));
    let pre = guided_uv(&uv, &uv, w, h, (0.75 * scale).max(0.2), 1e-5, false);
    for i in 0..n {
        let wt = satweight(saturation[i] - 0.1);
        for c in 0..2 {
            uv[2 * i + c] += wt * (pre[2 * i + c] - uv[2 * i + c]);
        }
    }
    let bands = s.mixer.bands();
    let [hue, sat, bright, bw] = luts(s);
    let max_b = bands.iter().map(|b| (b.lum as f32 / 200.).abs()).fold(0., f32::max);
    let mut hsb = vec![[0.; 4]; n];
    crate::for_rows(&mut hsb, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let j = ucs::luv_to_jch(l[i], lw, [uv[2 * i], uv[2 * i + 1]]);
            let c = ucs::jch_to_hsb(j);
            *v = [c[0], c[1], c[2], j[1]];
        }
    });
    let mut corrections = vec![0.; 2 * n];
    let mut dh = vec![0.; n];
    let mut grad = vec![0.; n];
    for i in 0..n {
        let colour = hsb[i];
        if colour[3] > 1.525_878_9e-5 {
            dh[i] = ucs::lookup_gamut(&hue, colour[0]);
            corrections[2 * i] = ucs::lookup_gamut(&sat, colour[0]);
            corrections[2 * i + 1] = colour[1] * (ucs::lookup_gamut(&bright, colour[0]) - 1.);
        } else {
            corrections[2 * i] = 1.;
        }
        grad[i] = 4. * max_b.sqrt() * scale * scale * (gradient(&saturation, w, h, i % w, i / w) - 0.02).max(0.).powi(2);
    }
    let grad = blur(&grad, w, h, 1, scale.max(0.5));
    let corr = guided_uv(&uv, &corrections, w, h, (0.5 * scale).max(0.2), 1e-6, true);
    crate::for_rows(&mut img.data, w, |y, row| {
        for (x, pixel) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let c = hsb[i];
            let mut h = [c[0], c[1], c[2]];
            h[0] += dh[i];
            h[1] = (h[1] * (1. + 2. * (corr[2 * i] - 1.) * satweight(saturation[i] - 0.1))).max(0.);
            let ev = BRIGHTNESS_EV * corr[2 * i + 1] * (1. - grad[i].clamp(0., 1.)) * satweight(saturation[i] - 0.1 - 0.01 * max_b);
            h[2] = (h[2] * crate::primary::soft_bound(ev, BRIGHTNESS_MIN_EV, BRIGHTNESS_MAX_EV).exp2()).max(0.);
            if crate::is_bw(s) {
                let y = ucs::l_star_to_y((h[2] * lw).clamp(0., ucs::L_STAR_UPPER));
                let gain = (ucs::lookup_gamut(&bw, h[0]) * 1.3).exp2();
                *pixel = [y * gain; 3];
            } else {
                ucs::gamut_map_hsb(&mut h, lw);
                *pixel = ucs::hsb_to_rgb(h, lw);
            }
        }
    });
}
