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

/// How far a final, non-enlarging resample may pass [`antiring`]'s range, relative to each bound:
/// a cubic's acutance on low-contrast texture (up to about 0.7 stop) passes, a dip of 0.07 EV at
/// most beside an edge. An intermediate resample (a prefilter another resample follows) uses no
/// slack, so the dips do not compound, and neither does an enlargement, which has no finer
/// texture to keep.
pub const ANTIRING_SLACK: f32 = 0.05;

/// Ringing suppression for resampling scene-linear light: `v` (a weighted sum of samples) clamped to
/// `core`, the range of the samples in the kernel's positive lobe, widened by `slack` of each bound
/// ([`ANTIRING_SLACK`] for a final resample).
///
/// A cubic kernel's negative lobes over- and undershoot an edge by a fixed fraction of the step. In
/// scene-linear light a window six stops over the wall beside it is a step of 60× the wall, so the
/// undershoot turns the wall next to the window black. Measured relative to the range, the same
/// ringing on low-contrast texture is small and passes.
#[inline]
pub fn antiring(v: f32, core: (f32, f32), slack: f32) -> f32 {
    if core.0 > core.1 || core.0.is_nan() || core.1.is_nan() {
        return v;
    }
    v.max(core.0 - slack * core.0.abs()).min(core.1 + slack * core.1.abs())
}

/// How far a tap of weight `w` counts toward [`antiring`]'s range: in proportion to its weight up to
/// a tenth of the largest weight `w_max`, fully beyond. A tap entering or leaving the positive lobe
/// has a weight near zero, so it widens the range gradually and float noise in its position can
/// never switch the clamp.
#[inline]
pub(crate) fn antiring_taper(w: f32, w_max: f32) -> f32 {
    (w * 10.0 / w_max).clamp(0.0, 1.0)
}

/// `Σ value(k)·weight(k)` over `n` taps (in tap order, as the GPU kernels add) with ringing
/// suppressed ([`antiring`]). The range is measured about the positive lobe's weighted mean, each
/// tap counted as far as [`antiring_taper`] says.
#[inline]
pub(crate) fn antiring_taps(n: usize, value: impl Fn(usize) -> [f32; 3], weight: impl Fn(usize) -> f32, slack: f32) -> [f32; 3] {
    let w_max = (0..n).map(&weight).fold(0.0f32, f32::max);
    let (mut acc, mut lobe, mut lobe_w) = ([0.0f32; 3], [0.0f32; 3], 0.0f32);
    for k in 0..n {
        let (v, w) = (value(k), weight(k));
        let c = w * antiring_taper(w, w_max);
        for ch in 0..3 {
            acc[ch] += v[ch] * w;
            lobe[ch] += v[ch] * c;
        }
        lobe_w += c;
    }
    if lobe_w <= 0.0 {
        return acc;
    }
    let mean = lobe.map(|l| l / lobe_w);
    let mut core = [(0.0f32, 0.0f32); 3];
    for k in 0..n {
        let (v, t) = (value(k), antiring_taper(weight(k), w_max));
        for ch in 0..3 {
            let d = (v[ch] - mean[ch]) * t;
            core[ch] = (core[ch].0.min(d), core[ch].1.max(d));
        }
    }
    std::array::from_fn(|ch| antiring(acc[ch], (mean[ch] + core[ch].0, mean[ch] + core[ch].1), slack))
}

/// Taps of a 1-D resample whose output sample `i` is centred on source position `a·(i + 0.5) + b`
/// (source sample `j` covers `j..j + 1`). The kernel is widened by `max(|a|, 1)` so a reduction is
/// band-limited, and taps past either end read the edge sample. `a = src / dst, b = 0` places the
/// samples as [`weights`] does. Same layout as [`weights`]: (first source index, weights).
pub fn offset_weights(src: usize, dst: usize, a: f64, b: f64, filter: Filter) -> Vec<(usize, Vec<f32>)> {
    if src == 0 {
        return vec![(0, Vec::new()); dst];
    }
    // A kernel wider than the source reads nothing new and only lengthens the tap loop.
    let stretch = a.abs().clamp(1.0, src as f64 + 1.0);
    let reach = f64::from(filter.support()) * stretch;
    let last = src as i64 - 1;
    (0..dst)
        .map(|i| {
            // Continuous source index: sample `j` sits at `f == j`.
            let f = a * (i as f64 + 0.5) + b - 0.5;
            if !f.is_finite() {
                return ((if f > 0.0 { last } else { 0 }) as usize, vec![1.0]);
            }
            let (j0, j1) = ((f - reach).floor() as i64 + 1, (f + reach).ceil() as i64 - 1);
            let (j0, j1) = if j0 > j1 { (f.round() as i64, f.round() as i64) } else { (j0, j1) };
            let (lo, hi) = (j0.clamp(0, last), j1.clamp(0, last));
            let mut w = vec![0.0f32; (hi - lo + 1) as usize];
            for j in j0..=j1 {
                w[(j.clamp(lo, hi) - lo) as usize] += filter.eval(((j as f64 - f) / stretch) as f32);
            }
            let sum: f32 = w.iter().sum();
            if sum.abs() > 1e-8 {
                w.iter_mut().for_each(|v| *v /= sum);
            } else {
                let n = w.len() as f32;
                w.iter_mut().for_each(|v| *v = 1.0 / n);
            }
            (lo as usize, w)
        })
        .collect()
}

/// Separable resample of scene-linear RGB through precomputed taps (`wx` per output column, `wy`
/// per output row: [`weights`] or [`offset_weights`]), with ringing suppressed in each pass
/// ([`antiring`] with `slack`). Output is `wx.len() × wy.len()`.
pub fn resize_antiring(data: &[[f32; 3]], width: usize, height: usize, wx: &[(usize, Vec<f32>)], wy: &[(usize, Vec<f32>)], slack: f32) -> Vec<[f32; 3]> {
    let (w, h) = (wx.len(), wy.len());
    let mut out = vec![[0.0f32; 3]; w * h];
    if width == 0 || height == 0 || w == 0 || data.len() < width * height {
        return out;
    }
    const BAND: usize = 64;
    par_rows(&mut out, w * BAND, |band, rows| {
        let y0 = band * BAND;
        let taps = &wy[y0..y0 + rows.len() / w];
        let first = taps.iter().map(|t| t.0).min().unwrap_or(0).min(height - 1);
        let last = taps.iter().map(|t| t.0 + t.1.len()).max().unwrap_or(first).clamp(first, height);
        let mut horizontal = vec![[0.0f32; 3]; (last - first) * w];
        for (sy, out) in (first..last).zip(horizontal.chunks_exact_mut(w)) {
            let source = &data[sy * width..(sy + 1) * width];
            for (p, (start, weights)) in out.iter_mut().zip(wx) {
                let n = weights.len().min(source.len().saturating_sub(*start));
                *p = antiring_taps(n, |k| source[start + k], |k| weights[k], slack);
            }
        }
        let rows_held = last - first;
        for (row, (start, weights)) in rows.chunks_exact_mut(w).zip(taps) {
            let r0 = start.saturating_sub(first).min(rows_held);
            let n = weights.len().min(rows_held - r0);
            for (x, p) in row.iter_mut().enumerate() {
                *p = antiring_taps(n, |k| horizontal[(r0 + k) * w + x], |k| weights[k], slack);
            }
        }
    });
    out
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

/// The resampling choices both export dialogs offer (Library Export and the editor's Export As):
/// one list, one backend. Photoshop's Image Size order, with Lanczos after the cubics. The serde
/// name ([`ResampleChoice::key`]) is the editor's `image.imageSize` `resample` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResampleChoice {
    /// Photoshop's rule: Bicubic Smoother when enlarging, Bicubic Sharper when reducing.
    BicubicAutomatic,
    PreserveDetails,
    BicubicSmoother,
    BicubicSharper,
    Bicubic,
    Lanczos,
    Bilinear,
    Nearest,
}

impl ResampleChoice {
    pub const ALL: [Self; 8] = [
        Self::BicubicAutomatic,
        Self::PreserveDetails,
        Self::BicubicSmoother,
        Self::BicubicSharper,
        Self::Bicubic,
        Self::Lanczos,
        Self::Bilinear,
        Self::Nearest,
    ];

    /// The `resample` name (`"bicubicAutomatic"`, `"lanczos"`…).
    pub fn key(self) -> &'static str {
        match self {
            Self::BicubicAutomatic => "bicubicAutomatic",
            Self::PreserveDetails => "preserveDetails",
            Self::BicubicSmoother => "bicubicSmoother",
            Self::BicubicSharper => "bicubicSharper",
            Self::Bicubic => "bicubic",
            Self::Lanczos => "lanczos",
            Self::Bilinear => "bilinear",
            Self::Nearest => "nearest",
        }
    }

    /// The choice named `key` (also the editor's aliases `automatic` and `nearestNeighbor`).
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "automatic" => Some(Self::BicubicAutomatic),
            "nearestNeighbor" => Some(Self::Nearest),
            k => Self::ALL.into_iter().find(|c| c.key() == k),
        }
    }

    /// The English menu label (callers translate it).
    pub fn label(self) -> &'static str {
        match self {
            Self::BicubicAutomatic => "Bicubic Automatic",
            Self::PreserveDetails => "Preserve Details (enlargement)",
            Self::BicubicSmoother => "Bicubic Smoother (enlargement)",
            Self::BicubicSharper => "Bicubic Sharper (reduction)",
            Self::Bicubic => "Bicubic (smooth gradients)",
            Self::Lanczos => "Lanczos (sharpest)",
            Self::Bilinear => "Bilinear",
            Self::Nearest => "Nearest Neighbor (hard edges)",
        }
    }

    /// One-line English explanation for a tooltip or hint (callers translate it).
    pub fn hint(self) -> &'static str {
        match self {
            Self::BicubicAutomatic => "Bicubic Sharper when reducing, Bicubic Smoother when enlarging.",
            Self::PreserveDetails => "Lanczos with light sharpening when enlarging.",
            Self::BicubicSmoother => "Soft, clean enlargements (Mitchell cubic).",
            Self::BicubicSharper => "Keeps edge contrast when reducing (sharper cubic).",
            Self::Bicubic => "Balanced cubic for smooth gradients.",
            Self::Lanczos => "Most detail; may show slight halos on hard edges.",
            Self::Bilinear => "Soft and fast.",
            Self::Nearest => "Copies pixels: for pixel art and hard-edged graphics.",
        }
    }

    /// The kernel for a resize that enlarges (`true`) or reduces.
    pub fn resolve(self, enlarging: bool) -> Resample {
        match self {
            Self::BicubicAutomatic if enlarging => Resample::BicubicSmoother,
            Self::BicubicAutomatic => Resample::BicubicSharper,
            Self::PreserveDetails => Resample::PreserveDetails,
            Self::BicubicSmoother => Resample::BicubicSmoother,
            Self::BicubicSharper => Resample::BicubicSharper,
            Self::Bicubic => Resample::Bicubic,
            Self::Lanczos => Resample::Lanczos,
            Self::Bilinear => Resample::Bilinear,
            Self::Nearest => Resample::Nearest,
        }
    }
}

/// Samples a dense interleaved buffer can hold; read as `0..1` floats (u8 / 255, u16 / 65535).
pub trait Sample: Copy + Send + Sync {
    fn to_f32(self) -> f32;
}
impl Sample for f32 {
    #[inline]
    fn to_f32(self) -> f32 {
        self
    }
}
impl Sample for u16 {
    #[inline]
    fn to_f32(self) -> f32 {
        f32::from(self) / 65535.0
    }
}
impl Sample for u8 {
    #[inline]
    fn to_f32(self) -> f32 {
        f32::from(self) / 255.0
    }
}

struct Dense<'a, S> {
    data: &'a [S],
    width: usize,
    height: usize,
    channels: usize,
    alpha: bool,
}

impl<S: Sample> Source for Dense<'_, S> {
    fn channels(&self) -> usize {
        self.channels
    }
    fn alpha(&self) -> bool {
        self.alpha
    }
    fn content_bounds(&self) -> Region {
        Region::new(0, 0, self.width as i32, self.height as i32)
    }
    fn default_pixel(&self) -> Vec<f32> {
        vec![0.0; self.channels]
    }
    fn tile_size(&self) -> i32 {
        64
    }
    fn has_tiles_in(&self, _: Region) -> bool {
        true
    }
    fn read_region_into(&self, r: Region, out: &mut Vec<f32>) {
        let n = self.channels;
        out.clear();
        out.reserve(r.width() as usize * r.height() as usize * n);
        for y in r.y0..r.y1 {
            let y = y.clamp(0, self.height as i32 - 1) as usize;
            for x in r.x0..r.x1 {
                let x = x.clamp(0, self.width as i32 - 1) as usize;
                let i = (y * self.width + x) * n;
                out.extend(self.data[i..i + n].iter().map(|v| v.to_f32()));
            }
        }
    }
}

/// Resize a dense, interleaved `width × height` image of `channels` samples (the last one alpha
/// when `alpha`) to exactly `w × h` with the editor's Image Size engine: f64 taps, premultiplied
/// alpha, edges repeated (never faded), streamed in bounded reads, so the source is never copied
/// to float. Returns interleaved `0..1` floats. Preserve Details' enlargement sharpening is the
/// caller's (it is a filter, see `photocraft_algo::resample`).
#[allow(clippy::too_many_arguments)]
pub fn resize_interleaved<S: Sample>(data: &[S], width: usize, height: usize, channels: usize, alpha: bool, w: usize, h: usize, filter: Resample) -> Vec<f32> {
    let (w, h, n) = (w.max(1), h.max(1), channels.max(1));
    let mut out = vec![0.0f32; w * h * n];
    if width == 0 || height == 0 || data.len() < width * height * n {
        return out;
    }
    let src = Dense { data, width, height, channels: n, alpha };
    let full = src.content_bounds();
    let (sx, sy) = (w as f64 / width as f64, h as f64 / height as f64);
    for (r, d) in resize_source(&src, sx, sy, filter, Some(full), 1 << 24) {
        // `scaled_rect` rounds outwards, so a band can carry one extra column/row; drop it.
        let rw = r.width() as usize;
        for (row, y) in d.chunks_exact((rw * n).max(1)).zip(r.y0..r.y1) {
            let Ok(y) = usize::try_from(y) else { continue };
            if y >= h {
                break;
            }
            let x0 = r.x0.max(0) as usize;
            let take = rw.min(w.saturating_sub(x0));
            out[(y * w + x0) * n..(y * w + x0 + take) * n].copy_from_slice(&row[..take * n]);
        }
    }
    out
}

#[cfg(test)]
mod lanczos_tests {
    use super::*;

    #[test]
    fn choice_keys_round_trip_and_resolve() {
        for c in ResampleChoice::ALL {
            assert_eq!(ResampleChoice::from_key(c.key()), Some(c));
            assert_eq!(camel(c), c.key(), "serde camelCase name is the key");
        }
        assert_eq!(ResampleChoice::from_key("automatic"), Some(ResampleChoice::BicubicAutomatic));
        assert_eq!(ResampleChoice::BicubicAutomatic.resolve(true), Resample::BicubicSmoother);
        assert_eq!(ResampleChoice::BicubicAutomatic.resolve(false), Resample::BicubicSharper);
    }

    // The derive's `rename_all = "camelCase"` name, from the variant name (no serde_json here).
    fn camel(c: ResampleChoice) -> String {
        let d = format!("{c:?}");
        let mut s = d[..1].to_lowercase();
        s.push_str(&d[1..]);
        s
    }

    #[test]
    fn dense_resize_exact_size_and_flat_field() {
        let (w0, h0) = (37, 23);
        let data: Vec<u16> = (0..w0 * h0).flat_map(|_| [13107u16, 32768, 52428]).collect();
        for c in ResampleChoice::ALL {
            for (w, h) in [(11, 7), (80, 50), (37, 23)] {
                let out = resize_interleaved(&data, w0, h0, 3, false, w, h, c.resolve(w > w0));
                assert_eq!(out.len(), w * h * 3);
                for px in out.as_chunks::<3>().0 {
                    for (v, e) in px.iter().zip([0.2f32, 32768.0 / 65535.0, 0.8]) {
                        assert!((v - e).abs() < 1e-5, "{c:?} {w}x{h}: {px:?}");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod antiring_tests {
    use super::*;

    /// A window six to eight stops over the wall beside it: the plain Mitchell reduction rings the
    /// wall below zero (black after tone mapping); the anti-ringing resample keeps every pixel
    /// within the source's range, in every channel, give or take [`ANTIRING_SLACK`].
    #[test]
    fn scene_linear_reduction_never_rings_past_a_high_contrast_edge() {
        let (w0, h0) = (1203, 9);
        let wall = [0.05f32, 0.04, 0.03];
        let window = [0.18 * 256.0, 0.9 * 64.0, 1.2 * 100.0];
        let mut plain_rings = false;
        for edge in [600, 601, 602] {
            let src: Vec<[f32; 3]> = (0..w0 * h0).map(|i| if i % w0 < edge { wall } else { window }).collect();
            for w in [401, 300, 151, 960] {
                let (wx, wy) = (weights(w0, w, Filter::Mitchell), weights(h0, 3, Filter::Mitchell));
                plain_rings |= resize_pixels(&src, w0, h0, w, 3, Filter::Mitchell).iter().any(|p| p[0] < 0.0);
                let out = resize_antiring(&src, w0, h0, &wx, &wy, ANTIRING_SLACK);
                for p in &out {
                    for c in 0..3 {
                        let slack = ANTIRING_SLACK + 1e-5;
                        assert!(p[c] >= wall[c] * (1.0 - slack) && p[c] <= window[c] * (1.0 + slack), "edge {edge}, width {w}: {p:?}");
                    }
                }
            }
        }
        assert!(plain_rings, "the plain Mitchell reduction rings below zero");
    }

    /// Below 1.5:1 contrast the result is the plain weighted sum: fine texture keeps the kernel's
    /// response, and a flat field stays exact.
    #[test]
    fn low_contrast_texture_resamples_like_the_plain_kernel() {
        let (w0, h0) = (97, 41);
        let src: Vec<[f32; 3]> = (0..w0 * h0)
            .map(|i| {
                let (x, y) = ((i % w0) as f32, (i / w0) as f32);
                [0.2 + 0.03 * (x * 1.3).sin(), 0.3 + 0.05 * (y * 0.9 + x * 0.4).cos(), 0.25]
            })
            .collect();
        for (w, h) in [(40, 17), (61, 29)] {
            for f in [Filter::Mitchell, Filter::CatmullRom, Filter::Lanczos3] {
                let plain = resize_pixels(&src, w0, h0, w, h, f);
                let out = resize_antiring(&src, w0, h0, &weights(w0, w, f), &weights(h0, h, f), ANTIRING_SLACK);
                for (a, b) in out.iter().zip(&plain) {
                    for c in 0..3 {
                        assert!((a[c] - b[c]).abs() < 1e-6, "{f:?} {w}x{h}: {a:?} vs {b:?}");
                    }
                }
            }
        }
        let flat = vec![[0.42f32, 3.5, 0.0]; w0 * h0];
        for p in resize_antiring(&flat, w0, h0, &weights(w0, 30, Filter::Mitchell), &weights(h0, 13, Filter::Mitchell), 0.0) {
            assert!((p[0] - 0.42).abs() < 1e-6 && (p[1] - 3.5).abs() < 1e-5 && p[2] == 0.0, "{p:?}");
        }
    }

    /// Offset taps place output samples anywhere on the source: unit scale at a whole-pixel offset
    /// copies (and mirrors with a negative scale), `a = src/dst` matches the plain reduction, and
    /// taps past the edge read the edge sample.
    #[test]
    fn offset_taps_copy_mirror_scale_and_repeat_the_edge() {
        let dense = |(lo, w): &(usize, Vec<f32>), n: usize| {
            let mut d = vec![0.0f32; n];
            for (k, v) in w.iter().enumerate() {
                d[lo + k] += v;
            }
            d
        };
        let picks = |taps: Vec<(usize, Vec<f32>)>| taps.iter().map(|t| dense(t, 64).iter().position(|v| *v == 1.0)).collect::<Vec<_>>();
        assert_eq!(picks(offset_weights(64, 20, 1.0, 7.0, Filter::CatmullRom)), (7..27).map(Some).collect::<Vec<_>>());
        assert_eq!(picks(offset_weights(64, 64, -1.0, 64.0, Filter::CatmullRom)), (0..64).rev().map(Some).collect::<Vec<_>>());
        for f in [Filter::CatmullRom, Filter::Mitchell, Filter::Lanczos3] {
            // (the plain reduction drops the taps past the edge instead of repeating the edge)
            for (p, o) in weights(300, 100, f).iter().zip(&offset_weights(300, 100, 3.0, 0.0, f)).skip(3).take(94) {
                assert!(dense(p, 300).iter().zip(dense(o, 300)).all(|(a, b)| (a - b).abs() < 1e-6), "{f:?}");
            }
            assert_eq!(offset_weights(10, 1, 1.0, -3.0, f), vec![(0, vec![1.0])], "{f:?}: far past the edge reads the edge");
        }
    }

    #[test]
    fn antiring_passes_slight_overshoot_and_clamps_the_rest() {
        assert_eq!(antiring(0.97, (1.0, 2.0), 0.05), 0.97);
        assert_eq!(antiring(-0.1, (1.0, 2.0), 0.05), 0.95);
        assert_eq!(antiring(3.0, (1.0, 2.0), 0.05), 2.1);
        assert!((antiring(-0.3, (-0.1, 0.5), 0.05) + 0.105).abs() < 1e-6);
        assert_eq!(antiring(0.97, (1.0, 2.0), 0.0), 1.0);
        assert_eq!(antiring(0.7, (f32::INFINITY, f32::NEG_INFINITY), 0.05), 0.7);
    }
}
