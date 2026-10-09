//! Exposure independent guided filter, from darktable `eigf.h`, `gaussian.c`, and
//! `fast_guided_filter.h` at 733bd69f. GPL-3.0-or-later. No final coefficient averaging:
//! the linear luminance variance is normalized by mean × pixel, as in upstream.
use lightcraft_raster::Plane;

/// Upstream's corner-aligned bilinear interpolation (including its last-pixel weights).
pub fn interpolate(input: &[f32], w: usize, h: usize, ow: usize, oh: usize, nc: usize) -> Vec<f32> {
    let mut out = vec![0.0; ow * oh * nc];
    crate::for_rows(&mut out, ow * nc, |y, row| {
        let fy = y as f32 / oh as f32 * h as f32;
        let y0 = (fy.floor() as usize).min(h - 1);
        let y1 = (y0 + 1).min(h - 1);
        let dy = y1 as f32 - fy;
        for x in 0..ow {
            let fx = x as f32 / ow as f32 * w as f32;
            let x0 = (fx.floor() as usize).min(w - 1);
            let x1 = (x0 + 1).min(w - 1);
            let dx = x1 as f32 - fx;
            for c in 0..nc {
                let q = |x, y| input[(y * w + x) * nc + c];
                row[x * nc + c] = (1.0 - dy) * (q(x0, y1) * dx + q(x1, y1) * (1.0 - dx)) + dy * (q(x0, y0) * dx + q(x1, y0) * (1.0 - dx));
            }
        }
    });
    out
}

/// Deriche order-zero IIR Gaussian, column then row, with upstream endpoint initialization
/// and input clamps. Used by EIGF and color equalizer (unbounded means use ±f32::MAX).
pub fn gaussian(input: &[f32], w: usize, h: usize, nc: usize, sigma: f32, min: &[f32], max: &[f32]) -> Vec<f32> {
    if w * h == 0 {
        return Vec::new();
    }
    let alpha = 1.695 / sigma.max(0.01);
    let ema = (-alpha).exp();
    let ema2 = (-2.0 * alpha).exp();
    let b1 = -2.0 * ema;
    let b2 = ema2;
    let k = (1.0 - ema) * (1.0 - ema) / (1.0 + 2.0 * alpha * ema - ema2);
    let (a0, a1, a2, a3) = (k, k * (alpha - 1.0) * ema, k * (alpha + 1.0) * ema, -k * ema2);
    let cp = (a0 + a1) / (1.0 + b1 + b2);
    let cn = (a2 + a3) / (1.0 + b1 + b2);
    let mut tmp = vec![0.0; input.len()];
    let mut out = tmp.clone();
    // Columns write a transposed intermediate, so both independent sweeps can
    // borrow disjoint contiguous lines without unsafe strided writes. Each
    // channel retains exactly the upstream recurrence and operation order.
    let line_filter = |src: &[f32], dst: &mut [f32], line: usize, horizontal: bool| {
        let len = if horizontal { w } else { h };
        for c in 0..nc {
            let idx = |j| if horizontal { (j * h + line) * nc + c } else { (j * w + line) * nc + c };
            let clamp = |v: f32| v.clamp(min[c], max[c]);
            let mut xp = clamp(src[idx(0)]);
            let mut yp = xp * cp;
            let mut yb = yp;
            for j in 0..len {
                let xc = clamp(src[idx(j)]);
                let yc = a0 * xc + a1 * xp - b1 * yp - b2 * yb;
                dst[j * nc + c] = yc;
                xp = xc;
                yb = yp;
                yp = yc;
            }
            let mut xn = clamp(src[idx(len - 1)]);
            let mut xa = xn;
            let mut yn = xn * cn;
            let mut ya = yn;
            for j in (0..len).rev() {
                let xc = clamp(src[idx(j)]);
                let yc = a2 * xn + a3 * xa - b1 * yn - b2 * ya;
                dst[j * nc + c] += yc;
                xa = xn;
                xn = xc;
                ya = yn;
                yn = yc;
            }
        }
    };
    crate::for_rows(&mut tmp, h * nc, |x, col| line_filter(input, col, x, false));
    crate::for_rows(&mut out, w * nc, |y, row| line_filter(&tmp, row, y, true));
    out
}

pub fn mean(input: &[f32], w: usize, h: usize, nc: usize, sigma: f32) -> Vec<f32> {
    gaussian(input, w, h, nc, sigma, &vec![-f32::MAX; nc], &vec![f32::MAX; nc])
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub sigma: f32,
    pub feathering: f32,
    pub iterations: usize,
    pub geometric: bool,
    pub quantization: f32,
    pub quantize_min: f32,
    pub quantize_max: f32,
}
impl Params {
    pub fn new(sigma: f32, feathering: f32) -> Self {
        Self { sigma, feathering, iterations: 2, geometric: false, quantization: 0.0, quantize_min: 2f32.powi(-14), quantize_max: 4.0 }
    }
}

/// Faithful `fast_eigf_surface_blur`, both quantized/cross and self-guided paths.
/// Tiny images clamp downsampled dimensions to one (upstream can allocate a zero-sized buffer).
pub fn filter(input: &Plane, p: Params) -> Plane {
    if input.is_empty() {
        return input.clone();
    }
    let (w, h) = (input.width, input.height);
    let scale = p.sigma.clamp(1.0, 4.0);
    let (dw, dh) = (((w as f32 / scale) as usize).max(1), ((h as f32 / scale) as usize).max(1));
    let sigma = (p.sigma / scale).max(1.0);
    let mut out = input.clone();
    for it in 0..p.iterations {
        let ds = interpolate(&out.data, w, h, dw, dh, 1);
        let mask: Vec<f32> = out
            .data
            .iter()
            .map(|v| {
                if p.quantization == 0.0 {
                    *v
                } else {
                    ((v.log2() / p.quantization).floor() * p.quantization).exp2().clamp(p.quantize_min, p.quantize_max)
                }
            })
            .collect();
        let guide = interpolate(&mask, w, h, dw, dh, 1);
        let nc = if p.quantization == 0.0 { 2 } else { 4 };
        let mut moments = Vec::with_capacity(dw * dh * nc);
        for (&g, &m) in guide.iter().zip(&ds) {
            moments.extend([g, g * g]);
            if nc == 4 {
                moments.extend([m, m * g]);
            }
        }
        let mut min = vec![10_000_000.0f32; nc];
        let mut max = vec![0.0f32; nc];
        for pix in moments.chunks_exact(nc) {
            for c in 0..nc {
                min[c] = min[c].min(pix[c]);
                max[c] = max[c].max(pix[c]);
            }
        }
        let mut av = gaussian(&moments, dw, dh, nc, sigma, &min, &max);
        for pix in av.chunks_exact_mut(nc) {
            pix[1] -= pix[0] * pix[0];
            if nc == 4 {
                pix[3] -= pix[0] * pix[2];
            }
        }
        let av = interpolate(&av, dw, dh, w, h, nc);
        for (i, v) in out.data.iter_mut().enumerate() {
            let m = &av[i * nc..(i + 1) * nc];
            let ng = (m[0] * *v).max(1e-6);
            let var = m[1] / ng;
            let (a, b) = if nc == 2 {
                let a = var / (var + p.feathering);
                (a, m[0] - a * m[0])
            } else {
                let nm = (m[2] * mask[i]).max(1e-6);
                let cov = m[3] / (ng * nm).sqrt();
                let a = cov / (var + p.feathering);
                (a, m[2] - a * m[0])
            };
            let q = (*v * a + b).max(2f32.powi(-16));
            *v = if p.geometric && it + 1 == p.iterations { (*v * q).sqrt() } else { q };
        }
    }
    out
}
