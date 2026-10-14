//! Product-independent tap construction and separable resampling.
//! PC uses double-precision kernel evaluation and bounded sparse reads (#713).
//! Legacy LC tap arithmetic is retained for existing GPU table and byte parity.
use crate::blur::{premultiply, unpremultiply};
use crate::{Region, par_rows};
use serde::{Deserialize, Serialize};
// One polynomial/kernel definition evaluated at the adapter's required precision.
// Legacy f32 cutoffs/order remain deliberate compatibility policies, not separate kernels.
macro_rules! kernel_weight {
    ($filter:expr,$x:expr,$tiny:expr,$pi:expr) => {{
        let x = $x.abs();
        match $filter {
            Filter::Box | Filter::Nearest => {
                if x <= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            Filter::Bilinear | Filter::Triangle => (1.0 - x).max(0.0),
            Filter::Gaussian => (-x * x / 0.5).exp(),
            Filter::CatmullRom | Filter::BicubicSharper => {
                let a = if $filter == Filter::BicubicSharper { -0.75 } else { -0.5 };
                if x < 1.0 {
                    (a + 2.0) * x * x * x - (a + 3.0) * x * x + 1.0
                } else if x < 2.0 {
                    a * x * x * x - 5.0 * a * x * x + 8.0 * a * x - 4.0 * a
                } else {
                    0.0
                }
            }
            Filter::Mitchell => {
                let (b, c) = (1.0 / 3.0, 1.0 / 3.0);
                if x < 1.0 {
                    ((12.0 - 9.0 * b - 6.0 * c) * x * x * x + (-18.0 + 12.0 * b + 6.0 * c) * x * x + (6.0 - 2.0 * b)) / 6.0
                } else if x < 2.0 {
                    ((-b - 6.0 * c) * x * x * x + (6.0 * b + 30.0 * c) * x * x + (-12.0 * b - 48.0 * c) * x + (8.0 * b + 24.0 * c)) / 6.0
                } else {
                    0.0
                }
            }
            Filter::Lanczos3 => {
                if x < $tiny {
                    1.0
                } else if x < 3.0 {
                    let px = $pi * x;
                    3.0 * px.sin() * (px / 3.0).sin() / (px * px)
                } else {
                    0.0
                }
            }
        }
    }};
}

/// Resampling method (Photoshop's Image Size options).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Resample {
    Nearest,
    Bilinear,
    #[default]
    Bicubic,
    Lanczos,
    /// Lanczos followed by a light sharpening when enlarging.
    PreserveDetails,
    /// local-image: Photoshop's Bicubic Smoother, for enlarging: the Mitchell–Netravali cubic
    /// (B = C = 1/3), which trades some sharpness for smoother transitions.
    BicubicSmoother,
    /// local-image: Photoshop's Bicubic Sharper, for reducing: a Keys cubic with a = -0.75, which
    /// keeps more edge contrast than Catmull–Rom.
    BicubicSharper,
}

impl Resample {
    fn kernel(self) -> Filter {
        match self {
            Self::Nearest => Filter::Box,
            Self::Bilinear => Filter::Bilinear,
            Self::Bicubic => Filter::CatmullRom,
            Self::BicubicSmoother => Filter::Mitchell,
            Self::BicubicSharper => Filter::BicubicSharper,
            Self::Lanczos | Self::PreserveDetails => Filter::Lanczos3,
        }
    }
    pub fn support(self) -> f64 {
        f64::from(self.kernel().support())
    }
    pub fn weight(self, x: f64) -> f64 {
        kernel_weight!(self.kernel(), x, 1e-9, std::f64::consts::PI)
    }
}

/// Per-output-sample source window and normalized weights.
struct Taps {
    start: i32,
    weights: Vec<f32>,
    /// Total weight of taps outside the source's content, which all read the default pixel.
    outside: f32,
}

/// Taps for mapping output coordinate `o` (in `[dst0, dst1)`) to the source
/// via `u = (o + 0.5) / scale - 0.5`. With `edge` (`[first, last]` source index), taps outside it
/// read the nearest sample inside it: the image repeats its edge instead of fading out.
/// Taps outside `content` (`[first, last]`) all read the default pixel, so they are folded into
/// `outside` instead of being read: the read window never grows past the content, however small
/// the scale (#713).
fn taps(dst0: i32, dst1: i32, scale: f64, filter: Resample, edge: Option<(i32, i32)>, content: (i32, i32)) -> Vec<Taps> {
    let stretch = if scale < 1.0 { 1.0 / scale } else { 1.0 };
    let support = filter.support() * stretch;
    let clamp = |i: i32| edge.map_or(i, |(a, b)| i.clamp(a, b));
    (dst0..dst1)
        .map(|o| {
            let u = (o as f64 + 0.5) / scale - 0.5;
            if filter == Resample::Nearest && scale >= 1.0 {
                let i = clamp((u + 0.5).floor() as i32);
                return if (content.0..=content.1).contains(&i) {
                    Taps { start: i, weights: vec![1.0], outside: 0.0 }
                } else {
                    Taps { start: content.0, weights: Vec::new(), outside: 1.0 }
                };
            }
            let lo = (u - support).ceil() as i32;
            let hi = (u + support).floor() as i32;
            let mut w: Vec<f64> = (lo..=hi).map(|i| filter.weight((i as f64 - u) / stretch)).collect();
            let s: f64 = w.iter().sum();
            if s.abs() > 1e-12 {
                for v in &mut w {
                    *v /= s;
                }
            }
            // Fold the weights of taps beyond the edge onto the edge sample.
            let start = clamp(lo);
            let mut folded = vec![0.0f32; (clamp(hi) - start + 1).max(1) as usize];
            for (i, v) in (lo..=hi).zip(w) {
                if let Some(slot) = folded.get_mut((clamp(i) - start) as usize) {
                    *slot += v as f32;
                }
            }
            // Keep the part inside the content; the rest reads the default pixel.
            let (a, b) = (start.max(content.0), (start + folded.len() as i32 - 1).min(content.1));
            if a > b {
                return Taps { start: content.0, weights: Vec::new(), outside: folded.iter().sum() };
            }
            let (i0, i1) = ((a - start) as usize, (b - start) as usize + 1);
            let outside = folded.get(..i0).unwrap_or(&[]).iter().chain(folded.get(i1..).unwrap_or(&[])).sum();
            Taps { start: a, weights: folded.get(i0..i1).unwrap_or(&[]).to_vec(), outside }
        })
        .collect()
}

/// Output rectangle for `r` scaled by `(sx, sy)` about the origin.
pub fn scaled_rect(r: Region, sx: f64, sy: f64) -> Region {
    if r.is_empty() {
        return Region::EMPTY;
    }
    Region::new((r.x0 as f64 * sx).floor() as i32, (r.y0 as f64 * sy).floor() as i32, (r.x1 as f64 * sx).ceil() as i32, (r.y1 as f64 * sy).ceil() as i32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Box,
    Bilinear,
    Mitchell,
    Lanczos3,
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    BicubicSharper,
}

impl Filter {
    pub fn support(self) -> f32 {
        match self {
            Filter::Box | Filter::Nearest => 0.5,
            Filter::Bilinear | Filter::Triangle => 1.0,
            Filter::Mitchell | Filter::CatmullRom | Filter::BicubicSharper => 2.0,
            Filter::Gaussian => 3.0,
            Filter::Lanczos3 => 3.0,
        }
    }
    pub(crate) fn eval(self, x: f32) -> f32 {
        kernel_weight!(self, x, 1e-6, std::f32::consts::PI)
    }
}

/// Contributions for each output coordinate: (first source index, weights) — the exact taps
/// [`resize_pixels`] uses (GPU kernels upload these tables).
pub fn weights(src: usize, dst: usize, filter: Filter) -> Vec<(usize, Vec<f32>)> {
    if src == 0 || dst == 0 {
        return vec![(0, Vec::new()); dst];
    }
    if filter == Filter::Nearest {
        return (0..dst).map(|i| (((i as u128 * 2 + 1) * src as u128 / (dst as u128 * 2)) as usize, vec![1.0])).collect();
    }
    let scale = src as f32 / dst as f32;
    let fscale = scale.max(1.0);
    let support = filter.support() * fscale;
    (0..dst)
        .map(|i| {
            let center = (i as f32 + 0.5) * scale;
            let lo = ((center - support).floor() as isize).max(0) as usize;
            let hi = ((center + support).ceil() as usize).min(src);
            let mut w: Vec<f32> = (lo..hi).map(|j| filter.eval((j as f32 + 0.5 - center) / fscale)).collect();
            let sum: f32 = w.iter().sum();
            if sum.abs() > 1e-8 {
                w.iter_mut().for_each(|v| *v /= sum);
            } else if !w.is_empty() {
                let n = w.len() as f32;
                w.iter_mut().for_each(|v| *v = 1.0 / n);
            }
            (lo, w)
        })
        .collect()
}

/// Pixel types that can be resampled (linear combination).
pub trait Pixel: Copy + Default + Send + Sync {
    fn zero() -> Self;
    /// Convert straight colour for alpha-aware interpolation. Custom opaque pixels need no override.
    fn premultiplied(self) -> Self {
        self
    }
    fn straight(self) -> Self {
        self
    }
    fn madd(self, o: Self, w: f32) -> Self;
}

impl Pixel for f32 {
    fn zero() -> Self {
        0.0
    }
    #[inline]
    fn madd(self, o: Self, w: f32) -> Self {
        self + o * w
    }
}

impl Pixel for [f32; 3] {
    fn zero() -> Self {
        [0.0; 3]
    }
    #[inline]
    fn madd(self, o: Self, w: f32) -> Self {
        [self[0] + o[0] * w, self[1] + o[1] * w, self[2] + o[2] * w]
    }
}

impl Pixel for [f32; 4] {
    fn premultiplied(self) -> Self {
        [self[0] * self[3], self[1] * self[3], self[2] * self[3], self[3]]
    }
    fn straight(self) -> Self {
        let a = self[3];
        if a > 1e-7 { [self[0] / a, self[1] / a, self[2] / a, a.clamp(0.0, 1.0)] } else { [0.0; 4] }
    }
    fn zero() -> Self {
        [0.0; 4]
    }
    #[inline]
    fn madd(self, o: Self, w: f32) -> Self {
        [self[0] + o[0] * w, self[1] + o[1] * w, self[2] + o[2] * w, self[3] + o[3] * w]
    }
}

/// Resize to `w × h` (separable; horizontal pass then vertical pass).
pub fn resize_pixels<T: Pixel>(data: &[T], width: usize, height: usize, w: usize, h: usize, filter: Filter) -> Vec<T> {
    let (w, h) = (w.max(1), h.max(1));
    if width == 0 || height == 0 {
        return vec![T::zero(); w * h];
    }
    if width == w && height == h {
        return data.to_vec();
    }
    if filter == Filter::Nearest {
        let mut out = vec![T::zero(); w * h];
        par_rows(&mut out, w, |y, row| {
            let sy = ((y as u128 * 2 + 1) * height as u128 / (h as u128 * 2)) as usize;
            for (x, p) in row.iter_mut().enumerate() {
                let sx = ((x as u128 * 2 + 1) * width as u128 / (w as u128 * 2)) as usize;
                *p = data[sy * width + sx];
            }
        });
        return out;
    }
    let wx = weights(width, w, filter);
    let wy = weights(height, h, filter);
    let mut out = vec![T::zero(); w * h];
    const BAND: usize = 64;
    // Stream source rows into output bands; no full-height horizontal intermediate.
    par_rows(&mut out, w * BAND, |band, rows| {
        let y0 = band * BAND;
        let taps = &wy[y0..y0 + rows.len() / w];
        let first = taps.iter().map(|t| t.0).min().unwrap_or(0);
        let last = taps.iter().map(|t| t.0 + t.1.len()).max().unwrap_or(first);
        let mut horizontal = vec![T::zero(); w];
        for sy in first..last {
            horizontal.fill(T::zero());
            let source = &data[sy * width..(sy + 1) * width];
            for (p, (start, weights)) in horizontal.iter_mut().zip(&wx) {
                for (i, weight) in weights.iter().enumerate() {
                    *p = p.madd(source[start + i].premultiplied(), *weight);
                }
            }
            for (row, (start, weights)) in rows.chunks_exact_mut(w).zip(taps) {
                if let Some(weight) = sy.checked_sub(*start).and_then(|i| weights.get(i)) {
                    for (p, sample) in row.iter_mut().zip(&horizontal) {
                        *p = p.madd(*sample, *weight);
                    }
                }
            }
        }
        for p in rows {
            *p = p.straight();
        }
    });
    out
}
/// 2× box reduction, repeating odd-sized edges. Alpha is interpolated premultiplied.
pub fn half_pixels<T: Pixel>(data: &[T], width: usize, height: usize) -> Vec<T> {
    let w = width.div_ceil(2).max(1);
    let h = height.div_ceil(2).max(1);
    let mut out = vec![T::zero(); w * h];
    if width == 0 || height == 0 {
        return out;
    }
    par_rows(&mut out, w, |y, row| {
        for (x, p) in row.iter_mut().enumerate() {
            let at = |x: usize, y: usize| data[y.min(height - 1) * width + x.min(width - 1)].premultiplied();
            *p = T::zero()
                .madd(at(2 * x, 2 * y), 0.25)
                .madd(at(2 * x + 1, 2 * y), 0.25)
                .madd(at(2 * x, 2 * y + 1), 0.25)
                .madd(at(2 * x + 1, 2 * y + 1), 0.25)
                .straight();
        }
    });
    out
}

/// Streaming source: absent tile rows are represented by the default pixel.
pub trait Source: Sync {
    fn channels(&self) -> usize;
    fn alpha(&self) -> bool;
    fn content_bounds(&self) -> Region;
    fn default_pixel(&self) -> Vec<f32>;
    fn tile_size(&self) -> i32;
    fn has_tiles_in(&self, r: Region) -> bool;
    fn read_region_into(&self, r: Region, out: &mut Vec<f32>);
}
pub fn resize_source(s: &impl Source, sx: f64, sy: f64, filter: Resample, edge: Option<Region>, budget: usize) -> Vec<(Region, Vec<f32>)> {
    if !sx.is_finite() || !sy.is_finite() || sx <= 0.0 || sy <= 0.0 {
        return Vec::new();
    }
    let src = s.content_bounds();
    let dst = scaled_rect(src, sx, sy);
    if dst.is_empty() {
        return Vec::new();
    }
    let n = s.channels();
    let alpha = s.alpha();
    // With `edge`, every tap is clamped into the edge area, which holds the content.
    let area = edge.unwrap_or(src);
    let hx = taps(dst.x0, dst.x1, sx, filter, edge.map(|r| (r.x0, r.x1 - 1)), (area.x0, area.x1 - 1));
    let vy = taps(dst.y0, dst.y1, sy, filter, edge.map(|r| (r.y0, r.y1 - 1)), (area.y0, area.y1 - 1));
    let x_lo = hx.iter().filter(|t| !t.weights.is_empty()).map(|t| t.start).min().unwrap_or(area.x0);
    let x_hi = hx.iter().map(|t| t.start + t.weights.len() as i32).max().unwrap_or(area.x0).max(x_lo);
    let mut def = s.default_pixel();
    premultiply(&mut def, n, alpha);
    let dw = dst.width() as usize;
    const BAND: usize = 64;
    let bands: Vec<(usize, usize)> = (0..vy.len()).step_by(BAND).map(|b| (b, (b + BAND).min(vy.len()))).collect();
    // Horizontal pass of one source row (`row` spans `x_lo..x_hi`) into `dw` samples.
    let horizontal = |row: &[f32], out: &mut [f32]| {
        for (t, d) in hx.iter().zip(out.chunks_exact_mut(n)) {
            for (k, w) in t.weights.iter().enumerate() {
                let sx_ = (t.start + k as i32 - x_lo) as usize;
                if let Some(sp) = row.get(sx_ * n..(sx_ + 1) * n) {
                    for c in 0..n {
                        d[c] += sp[c] * w;
                    }
                }
            }
            for c in 0..n {
                d[c] += def[c] * t.outside;
            }
        }
    };
    // A source row of nothing but the default pixel, after the horizontal pass.
    let def_row = {
        let row: Vec<f32> = def.iter().copied().cycle().take((x_hi - x_lo) as usize * n).collect();
        let mut out = vec![0.0f32; dw * n];
        horizontal(&row, &mut out);
        out
    };
    // At most one tile row per read anyway, so this always fits an `i32`.
    let rows_per_read = (budget / ((x_hi - x_lo).max(1) as usize * n).max(dw * n)).clamp(1, s.tile_size() as usize) as i32;
    let run = |&(b0, b1): &(usize, usize)| -> (Region, Vec<f32>) {
        let band = vy.get(b0..b1).unwrap_or(&[]);
        let y_lo = band.iter().filter(|t| !t.weights.is_empty()).map(|t| t.start).min().unwrap_or(area.y0);
        let y_hi = band.iter().map(|t| t.start + t.weights.len() as i32).max().unwrap_or(area.y0).max(y_lo);
        let mut res = vec![0.0f32; band.len() * dw * n];
        // Source rows stream through in bounded reads and go straight into the output rows that
        // use them, so memory is the read budget plus this band's output, whatever the scale or
        // the source size (#713). In ascending row order, as one read of the whole window would.
        let add = |res: &mut [f32], y: i32, row: &[f32]| {
            for (t, d) in band.iter().zip(res.chunks_exact_mut(dw * n)) {
                if let Some(w) = t.weights.get((y - t.start) as usize).filter(|_| y >= t.start) {
                    for (dv, sv) in d.iter_mut().zip(row) {
                        *dv += sv * w;
                    }
                }
            }
        };
        let mut rows = Vec::new();
        let mut hrows = Vec::new();
        let mut y = y_lo;
        while y < y_hi {
            let y1 = ((y.div_euclid(s.tile_size()) + 1) * s.tile_size()).min(y_hi);
            if s.has_tiles_in(Region::new(x_lo, y, x_hi, y1)) {
                let mut c0 = y;
                while c0 < y1 {
                    let c1 = (c0 + rows_per_read).min(y1);
                    let read = Region::new(x_lo, c0, x_hi, c1);
                    s.read_region_into(read, &mut rows);
                    premultiply(&mut rows, n, alpha);
                    hrows.clear();
                    hrows.resize((c1 - c0) as usize * dw * n, 0.0);
                    let rw = read.width() as usize * n;
                    for (row, out) in rows.chunks_exact(rw.max(1)).zip(hrows.chunks_exact_mut(dw * n)) {
                        horizontal(row, out);
                    }
                    for (r, h) in (c0..c1).zip(hrows.chunks_exact(dw * n)) {
                        add(&mut res, r, h);
                    }
                    c0 = c1;
                }
            } else {
                // Nothing stored in this tile row: every row in it is the default row, so each
                // output row takes it once, with the summed weight of those rows.
                for (t, d) in band.iter().zip(res.chunks_exact_mut(dw * n)) {
                    let (a, b) = ((y - t.start).max(0) as usize, (y1 - t.start).max(0) as usize);
                    let w: f32 = t.weights.get(a.min(t.weights.len())..b.min(t.weights.len())).unwrap_or(&[]).iter().sum();
                    if w != 0.0 {
                        for (dv, sv) in d.iter_mut().zip(&def_row) {
                            *dv += sv * w;
                        }
                    }
                }
            }
            y = y1;
        }
        for (t, d) in band.iter().zip(res.chunks_exact_mut(dw * n)) {
            for (dv, sv) in d.iter_mut().zip(&def_row) {
                *dv += sv * t.outside;
            }
        }
        if alpha {
            for px in res.chunks_exact_mut(n) {
                px[n - 1] = px[n - 1].clamp(0.0, 1.0);
            }
        }
        unpremultiply(&mut res, n, alpha);
        (Region::new(dst.x0, dst.y0 + b0 as i32, dst.x1, dst.y0 + b1 as i32), res)
    };
    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    let parts: Vec<(Region, Vec<f32>)> = {
        use rayon::prelude::*;
        bands.par_iter().map(run).collect()
    };
    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    let parts: Vec<(Region, Vec<f32>)> = bands.iter().map(run).collect();
    parts
}
