//! Independent vector-quality metrics. Raster scores use sRGB bytes; ΔE_ok is 100× Oklab distance.
//! MS-SSIM is our implementation of Wang et al. (2003), with Gaussian local windows.
//! The fidelity composite and coherent-error opening are ported from vtracer-bench
//! @ 928ed0a6f654408e28fb741b6133d4c456bd0160; no dssim-core code or dependency.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors (fidelity and patch detector).
// SPDX-License-Identifier: MIT OR Apache-2.0
use image::RgbImage;
use kurbo::{CubicBez, ParamCurve, ParamCurveNearest};
use photocraft_doc::Path;
use serde_json::json;

fn matching(a: &RgbImage, b: &RgbImage) -> Result<(usize, usize), String> {
    if a.dimensions() != b.dimensions() || a.width() == 0 || a.height() == 0 {
        return Err("metrics require nonempty equal-sized images".into());
    }
    Ok((a.width() as usize, a.height() as usize))
}
fn gaussian(values: &[f64], w: usize, h: usize) -> Vec<f64> {
    let mut weights = [0.; 11];
    for (i, v) in weights.iter_mut().enumerate() {
        *v = (-0.5 * ((i as f64 - 5.) / 1.5).powi(2)).exp();
    }
    let norm: f64 = weights.iter().sum();
    for v in &mut weights {
        *v /= norm;
    }
    let mut tmp = vec![0.; w * h];
    let mut out = vec![0.; w * h];
    for y in 0..h {
        for x in 0..w {
            for (i, &v) in weights.iter().enumerate() {
                let xx = (x as isize + i as isize - 5).clamp(0, w as isize - 1) as usize;
                tmp[y * w + x] += values[y * w + xx] * v;
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            for (i, &v) in weights.iter().enumerate() {
                let yy = (y as isize + i as isize - 5).clamp(0, h as isize - 1) as usize;
                out[y * w + x] += tmp[yy * w + x] * v;
            }
        }
    }
    out
}
fn local_ssim(a: &[f64], b: &[f64], w: usize, h: usize) -> (f64, f64) {
    let ma = gaussian(a, w, h);
    let mb = gaussian(b, w, h);
    let aa = gaussian(&a.iter().map(|v| v * v).collect::<Vec<_>>(), w, h);
    let bb = gaussian(&b.iter().map(|v| v * v).collect::<Vec<_>>(), w, h);
    let ab = gaussian(&a.iter().zip(b).map(|(a, b)| a * b).collect::<Vec<_>>(), w, h);
    let mut lum = 0.;
    let mut cs = 0.;
    for i in 0..a.len() {
        let va = (aa[i] - ma[i] * ma[i]).max(0.);
        let vb = (bb[i] - mb[i] * mb[i]).max(0.);
        let cov = ab[i] - ma[i] * mb[i];
        lum += (2. * ma[i] * mb[i] + 0.0001) / (ma[i] * ma[i] + mb[i] * mb[i] + 0.0001);
        cs += (2. * cov + 0.0009) / (va + vb + 0.0009);
    }
    (lum / a.len() as f64, cs / a.len() as f64)
}
fn down(a: &[f64], w: usize, h: usize) -> Vec<f64> {
    let nw = w.div_ceil(2);
    let nh = h.div_ceil(2);
    let mut out = vec![0.; nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            let mut n = 0.;
            let mut v = 0.;
            for dy in 0..2 {
                for dx in 0..2 {
                    let (xx, yy) = (x * 2 + dx, y * 2 + dy);
                    if xx < w && yy < h {
                        v += a[yy * w + xx];
                        n += 1.;
                    }
                }
            }
            out[y * nw + x] = v / n;
        }
    }
    out
}
/// Five-scale Wang MS-SSIM. Tiny inputs use all available scales with normalized weights.
pub fn ssim_ms(a: &RgbImage, b: &RgbImage) -> Result<f64, String> {
    let (mut w, mut h) = matching(a, b)?;
    let lum = |p: &image::Rgb<u8>| (0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])) / 255.;
    let mut a: Vec<f64> = a.pixels().map(lum).collect();
    let mut b: Vec<f64> = b.pixels().map(lum).collect();
    let mut components = Vec::new();
    for level in 0..5 {
        let (l, c) = local_ssim(&a, &b, w, h);
        components.push((l, c));
        if level == 4 || w < 2 || h < 2 {
            break;
        }
        a = down(&a, w, h);
        b = down(&b, w, h);
        w = w.div_ceil(2);
        h = h.div_ceil(2);
    }
    let weights = [0.0448, 0.2856, 0.3001, 0.2363, 0.1333];
    let norm: f64 = weights[..components.len()].iter().sum();
    let mut result = 1.;
    for (i, &(l, c)) in components.iter().enumerate() {
        let v = if i + 1 == components.len() { l * c } else { c };
        result *= v.clamp(0., 1.).powf(weights[i] / norm);
    }
    Ok(result.clamp(0., 1.))
}
pub fn oklab(rgb: [u8; 3]) -> [f64; 3] {
    let lin = |x: u8| {
        let v = f64::from(x) / 255.;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let [r, g, b] = rgb.map(lin);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}
pub fn delta_e(a: [u8; 3], b: [u8; 3]) -> f64 {
    let a = oklab(a);
    let b = oklab(b);
    100. * a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt()
}
pub fn delta_e_ok(a: &RgbImage, b: &RgbImage) -> Result<(f64, f64), String> {
    matching(a, b)?;
    let mut distances: Vec<f64> = a.pixels().zip(b.pixels()).map(|(a, b)| delta_e(a.0, b.0)).collect();
    let mean = distances.iter().sum::<f64>() / distances.len() as f64;
    distances.sort_by(f64::total_cmp);
    let p95 = distances[(distances.len() * 95).div_ceil(100).saturating_sub(1)];
    Ok((mean, p95))
}
#[derive(Debug, Clone, Copy)]
pub struct Fidelity {
    pub fidelity: f64,
    pub ms_ssim: f64,
    pub rmse: f64,
    pub psnr: f64,
    pub patch: f64,
    pub patch_mass: f64,
}
impl Fidelity {
    pub fn json(self) -> serde_json::Value {
        json!({"fidelity":self.fidelity,"ms_ssim":self.ms_ssim,"rmse":self.rmse,"psnr":self.psnr,"patch":self.patch,"patch_mass":self.patch_mass})
    }
}
/// vtracer-bench weighted geometric composite, with our MS-SSIM replacing dssim.
pub fn fidelity(a: &RgbImage, b: &RgbImage) -> Result<Fidelity, String> {
    let (w, h) = matching(a, b)?;
    let mut mask = vec![false; w * h];
    let mut sse = 0.;
    for (i, (a, b)) in a.pixels().zip(b.pixels()).enumerate() {
        let d2 = a.0.iter().zip(b.0).map(|(a, b)| (f64::from(*a) - f64::from(b)).powi(2)).sum::<f64>();
        mask[i] = d2 > 24. * 24.;
        sse += d2;
    }
    let at = |m: &[bool], x: isize, y: isize| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && m[y as usize * w + x as usize];
    let mut eroded = vec![false; w * h];
    let offsets = [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)];
    for y in 0..h {
        for x in 0..w {
            eroded[y * w + x] = offsets.iter().all(|&(dx, dy)| at(&mask, x as isize + dx, y as isize + dy));
        }
    }
    let mut opened = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            opened[y * w + x] = offsets.iter().any(|&(dx, dy)| at(&eroded, x as isize + dx, y as isize + dy));
        }
    }
    let mut sumsq: f64 = 0.;
    for seed in 0..opened.len() {
        if !opened[seed] {
            continue;
        }
        opened[seed] = false;
        let mut todo = vec![seed];
        let mut area = 0.;
        while let Some(i) = todo.pop() {
            area += 1.;
            let (x, y) = (i % w, i / w);
            for (dx, dy) in &offsets[1..] {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if at(&opened, nx, ny) {
                    let j = ny as usize * w + nx as usize;
                    opened[j] = false;
                    todo.push(j);
                }
            }
        }
        sumsq += area * area;
    }
    let patch_mass: f64 = sumsq.sqrt();
    let patch = 2f64.powf(-patch_mass / (w * h) as f64 / 0.005);
    let rmse = (sse / (w * h * 3) as f64).sqrt();
    let psnr = 20. * (255. / rmse.max(1e-6)).log10();
    let score = 1. - (1. + rmse).ln() / 256f64.ln();
    let ms_ssim = ssim_ms(a, b)?;
    Ok(Fidelity { fidelity: (score * ms_ssim * ms_ssim * patch).max(0.).powf(0.25), ms_ssim, rmse, psnr, patch, patch_mass })
}
pub fn iou(a: &[f32], b: &[f32], threshold: f32) -> Result<f64, String> {
    if a.len() != b.len() || !threshold.is_finite() || a.iter().chain(b).any(|v| !v.is_finite()) {
        return Err("invalid IoU inputs".into());
    }
    let mut intersection = 0;
    let mut union = 0;
    for (&a, &b) in a.iter().zip(b) {
        let a = a >= threshold;
        let b = b >= threshold;
        intersection += usize::from(a && b);
        union += usize::from(a || b);
    }
    Ok(if union == 0 { 1. } else { intersection as f64 / union as f64 })
}
pub fn max_abs_diff(a: &[f32], b: &[f32]) -> Result<f32, String> {
    if a.len() != b.len() || a.iter().chain(b).any(|v| !v.is_finite()) {
        return Err("invalid difference inputs".into());
    }
    Ok(a.iter().zip(b).map(|(a, b)| (a - b).abs()).fold(0., f32::max))
}
pub fn is_finite(path: &Path) -> bool {
    path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).all(|p| p.x.is_finite() && p.y.is_finite())
}
pub fn node_count(path: &Path) -> usize {
    path.subpaths.iter().map(|s| s.knots.len()).sum()
}
fn curves(path: &Path) -> Vec<CubicBez> {
    path.subpaths
        .iter()
        .flat_map(|s| s.segments())
        .map(|p| {
            let p = p.map(|p| kurbo::Point::new(p.x, p.y));
            CubicBez::new(p[0], p[1], p[2], p[3])
        })
        .collect()
}
/// Sampled symmetric boundary Hausdorff, with independent cubic nearest-point queries.
pub fn hausdorff(a: &Path, b: &Path, samples: usize) -> Result<f64, String> {
    if !is_finite(a) || !is_finite(b) || samples < 2 {
        return Err("invalid Hausdorff inputs".into());
    }
    let a = curves(a);
    let b = curves(b);
    if a.is_empty() && b.is_empty() {
        return Ok(0.);
    }
    if a.is_empty() || b.is_empty() {
        return Ok(f64::INFINITY);
    }
    let distance = |a: &[CubicBez], b: &[CubicBez]| {
        a.iter()
            .flat_map(|c| (0..=samples).map(move |i| c.eval(i as f64 / samples as f64)))
            .map(|p| b.iter().map(|c| c.nearest(p, 1e-6).distance_sq).fold(f64::INFINITY, f64::min).sqrt())
            .fold(0., f64::max)
    };
    Ok(distance(&a, &b).max(distance(&b, &a)))
}
/// Brute-force flattened cubic intersections; shares no tracing/region code.
/// Adjacent endpoints and tangent contacts are excluded; proper crossings are returned.
pub fn self_intersections(path: &Path) -> Vec<(usize, usize, f64, f64)> {
    let mut lines = Vec::new();
    let mut segment = 0;
    for s in &path.subpaths {
        for p in s.segments() {
            let c = CubicBez::new((p[0].x, p[0].y), (p[1].x, p[1].y), (p[2].x, p[2].y), (p[3].x, p[3].y));
            let mut last = (0., c.p0);
            let mut bez = kurbo::BezPath::new();
            bez.move_to(c.p0);
            bez.curve_to(c.p1, c.p2, c.p3);
            let mut pts = Vec::new();
            kurbo::flatten(bez, 0.05, |el| {
                if let kurbo::PathEl::LineTo(p) = el {
                    pts.push(p)
                }
            });
            let count = pts.len();
            for (i, p) in pts.into_iter().enumerate() {
                let t = (i + 1) as f64 / count as f64;
                lines.push((segment, last.1, p, last.0, t));
                last = (t, p);
            }
            segment += 1;
        }
    }
    let mut out = Vec::new();
    for i in 0..lines.len() {
        for j in i + 1..lines.len() {
            let (ia, a, b, ta, tb) = lines[i];
            let (ib, c, d, tc, td) = lines[j];
            let ab = b - a;
            let cd = d - c;
            let det = ab.cross(cd);
            if det.abs() < 1e-12 {
                continue;
            }
            let ac = c - a;
            let t = ac.cross(cd) / det;
            let u = ac.cross(ab) / det;
            if t > 1e-7 && t < 1. - 1e-7 && u > 1e-7 && u < 1. - 1e-7 {
                out.push((ia, ib, ta + (tb - ta) * t, tc + (td - tc) * u));
            }
        }
    }
    out
}
