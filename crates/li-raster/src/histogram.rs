//! Bounded histograms of straight RGBA samples in the supplied colour domain.
//! No transfer function or colour conversion is applied. Luma weights must match
//! the working RGB primaries; the default is Rec.709/sRGB.

use serde::Serialize;

use crate::frame::Rgba8;

pub const BINS: usize = 256;

/// Normalized, nonnegative luminance coefficients for an RGB working space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LumaWeights([f64; 3]);

impl LumaWeights {
    pub const REC709: Self = Self([0.2126, 0.7152, 0.0722]);
    pub const REC2020: Self = Self([0.2627, 0.6780, 0.0593]);

    /// Normalize positive finite coefficients. Invalid weights are rejected rather
    /// than silently producing a different histogram.
    pub fn new(weights: [f64; 3]) -> Option<Self> {
        let sum = weights.iter().sum::<f64>();
        if weights.iter().any(|v| !v.is_finite() || *v < 0.0) || !sum.is_finite() || sum <= 0.0 {
            return None;
        }
        Some(Self(weights.map(|v| v / sum)))
    }

    pub fn coefficients(self) -> [f64; 3] {
        self.0
    }
}

impl Default for LumaWeights {
    fn default() -> Self {
        Self::REC709
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbHistogram {
    pub channels: [[u64; BINS]; 3],
    pub luma: [u64; BINS],
    pub samples: u64,
    pub transparent: u64,
    pub invalid: u64,
    /// Finite samples outside 0..=1 are included in edge bins and counted separately.
    pub underflow: [u64; 3],
    pub overflow: [u64; 3],
    /// Exact endpoints before bin rounding: near-white values are not clipped.
    pub shadows: [u64; 3],
    pub highlights: [u64; 3],
}

impl Default for RgbHistogram {
    fn default() -> Self {
        Self {
            channels: [[0; BINS]; 3],
            luma: [0; BINS],
            samples: 0,
            transparent: 0,
            invalid: 0,
            underflow: [0; 3],
            overflow: [0; 3],
            shadows: [0; 3],
            highlights: [0; 3],
        }
    }
}

impl RgbHistogram {
    /// Each visible pixel counts once, including partially transparent pixels.
    /// Non-finite pixels are rejected as a whole; transparent RGB cannot add peaks.
    pub fn from_rgba(pixels: &[[f32; 4]]) -> Self {
        Self::from_rgba_with_weights(pixels, LumaWeights::default())
    }

    pub fn from_rgba_with_weights(pixels: &[[f32; 4]], weights: LumaWeights) -> Self {
        let mut h = Self::default();
        for &pixel in pixels {
            h.add_rgba(pixel, weights);
        }
        h
    }

    /// Grid sampling for a row-major image, retaining exact float clipping rules.
    /// A zero stride means one. Missing/trailing pixels are safely ignored.
    pub fn from_rgba_sampled(pixels: &[[f32; 4]], width: usize, height: usize, stride: usize, weights: LumaWeights) -> Self {
        let mut h = Self::default();
        if width == 0 {
            return h;
        }
        for row in pixels.chunks(width).take(height).step_by(stride.max(1)) {
            for &pixel in row.iter().step_by(stride.max(1)) {
                h.add_rgba(pixel, weights);
            }
        }
        h
    }

    /// Historical Library preview contract: count encoded RGB even with zero alpha,
    /// use integer (54 R + 183 G + 19 B)/256 luma and the ~1 MP grid sampler.
    /// This deliberately preserves saved preview statistics and display-bin clipping.
    pub fn from_srgb8_preview(pixels: &[[u8; 4]], width: usize, height: usize) -> Self {
        let mut h = Self::default();
        if width == 0 {
            return h;
        }
        let stride = preview_stride(pixels.len());
        for row in pixels.chunks(width).take(height).step_by(stride) {
            for pixel in row.iter().step_by(stride) {
                h.samples = h.samples.saturating_add(1);
                for (c, &value) in pixel.iter().take(3).enumerate() {
                    let count = &mut h.channels[c][value as usize];
                    *count = count.saturating_add(1);
                    if value == 0 {
                        h.shadows[c] = h.shadows[c].saturating_add(1);
                    } else if value == 255 {
                        h.highlights[c] = h.highlights[c].saturating_add(1);
                    }
                }
                let luma = (u32::from(pixel[0]) * 54 + u32::from(pixel[1]) * 183 + u32::from(pixel[2]) * 19) >> 8;
                h.luma[luma as usize] = h.luma[luma as usize].saturating_add(1);
            }
        }
        h
    }

    fn add_rgba(&mut self, pixel: [f32; 4], weights: LumaWeights) {
        if !pixel[3].is_finite() {
            self.invalid = self.invalid.saturating_add(1);
            return;
        }
        if pixel[3] <= 0.0 {
            self.transparent = self.transparent.saturating_add(1);
            return;
        }
        if !pixel.iter().all(|v| v.is_finite()) {
            self.invalid = self.invalid.saturating_add(1);
            return;
        }
        self.samples = self.samples.saturating_add(1);
        for (c, &value) in pixel.iter().take(3).enumerate() {
            if value <= 0.0 {
                self.shadows[c] = self.shadows[c].saturating_add(1);
            }
            if value >= 1.0 {
                self.highlights[c] = self.highlights[c].saturating_add(1);
            }
            if value < 0.0 {
                self.underflow[c] = self.underflow[c].saturating_add(1);
            } else if value > 1.0 {
                self.overflow[c] = self.overflow[c].saturating_add(1);
            }
            let bin = (value.clamp(0.0, 1.0) * (BINS - 1) as f32 + 0.5) as usize;
            self.channels[c][bin] = self.channels[c][bin].saturating_add(1);
        }
        // f64 avoids overflow when finite HDR float values are weighted and added.
        let luma = pixel.iter().take(3).zip(weights.0).map(|(&v, w)| f64::from(v) * w).sum::<f64>();
        let bin = (luma.clamp(0.0, 1.0) * (BINS - 1) as f64 + 0.5) as usize;
        self.luma[bin] = self.luma[bin].saturating_add(1);
    }
}

/// The Library's established ~1 MP preview grid sampling policy.
pub fn preview_stride(pixel_count: usize) -> usize {
    ((pixel_count as f64 / 1_000_000.0).sqrt().ceil() as usize).max(1)
}

/// Visible straight RGBA clipping bit masks (bit 0=R, 1=G, 2=B) at exact
/// endpoints ≤0 / ≥1. Transparent and non-finite pixels clip nothing.
pub fn clipping(pixel: [f32; 4]) -> (u8, u8) {
    if pixel[3] <= 0.0 || !pixel.iter().all(|v| v.is_finite()) {
        return (0, 0);
    }
    let mut shadows = 0;
    let mut highlights = 0;
    for (c, &value) in pixel.iter().take(3).enumerate() {
        if value <= 0.0 {
            shadows |= 1 << c;
        }
        if value >= 1.0 {
            highlights |= 1 << c;
        }
    }
    (shadows, highlights)
}

/// Histogram of a display-encoded 8-bit image (what Develop's Histogram panel shows): the
/// [`RgbHistogram::from_srgb8_preview`] counts as `u32` bins plus summary statistics.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Histogram {
    pub r: Vec<u32>,
    pub g: Vec<u32>,
    pub b: Vec<u32>,
    pub luma: Vec<u32>,
    pub total: u32,
}

impl Histogram {
    pub const BINS: usize = BINS;

    pub fn of_srgb8(img: &Rgba8) -> Histogram {
        let h = RgbHistogram::from_srgb8_preview(&img.data, img.width, img.height);
        let counts = |bins: &[u64]| bins.iter().map(|&count| u32::try_from(count).unwrap_or(u32::MAX)).collect();
        Histogram {
            r: counts(&h.channels[0]),
            g: counts(&h.channels[1]),
            b: counts(&h.channels[2]),
            luma: counts(&h.luma),
            total: u32::try_from(h.samples).unwrap_or(u32::MAX),
        }
    }

    /// Fraction of pixels clipped at black / white in any channel (approximate: per channel max).
    pub fn clipping(&self) -> (f32, f32) {
        if self.total == 0 {
            return (0.0, 0.0);
        }
        let t = self.total as f32;
        let lo = self.r[0].max(self.g[0]).max(self.b[0]) as f32 / t;
        let hi = self.r[255].max(self.g[255]).max(self.b[255]) as f32 / t;
        (lo, hi)
    }

    /// Mean luma in 0..1.
    pub fn mean_luma(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        self.luma.iter().enumerate().map(|(i, &c)| i as f32 * c as f32).sum::<f32>() / (255.0 * self.total as f32)
    }

    /// Luma value (0..1) below which `q` of the pixels fall.
    pub fn percentile(&self, q: f32) -> f32 {
        let target = (q.clamp(0.0, 1.0) * self.total as f32) as u32;
        let mut acc = 0;
        for (i, &c) in self.luma.iter().enumerate() {
            acc += c;
            if acc >= target {
                return i as f32 / 255.0;
            }
        }
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_histogram_counts_and_stats() {
        let img = Rgba8::from_fn(10, 10, |x, _| if x < 5 { [0, 0, 0, 255] } else { [255, 255, 255, 255] });
        let h = Histogram::of_srgb8(&img);
        assert_eq!(h.total, 100);
        assert_eq!(h.r[0], 50);
        assert_eq!(h.luma[255], 50);
        let (lo, hi) = h.clipping();
        assert!((lo - 0.5).abs() < 1e-6 && (hi - 0.5).abs() < 1e-6);
        assert!((h.mean_luma() - 0.5).abs() < 1e-3);
        assert_eq!(h.percentile(0.25), 0.0);
        assert_eq!(h.percentile(0.75), 1.0);
    }

    #[test]
    fn j01_shared_histogram_lc_adapter_preview_parity() {
        let img = Rgba8::from_fn(256, 1, |x, _| [x as u8, (255 - x) as u8, (x * 73 % 256) as u8, (x % 2 * 255) as u8]);
        let h = Histogram::of_srgb8(&img);
        let mut expected_luma = vec![0; 256];
        let mut expected_b = vec![0; 256];
        for x in 0..256 {
            let b = x * 73 % 256;
            expected_b[b] += 1;
            expected_luma[(x * 54 + (255 - x) * 183 + b * 19) >> 8] += 1;
        }
        assert_eq!(h.r, vec![1; 256]);
        assert_eq!(h.g, vec![1; 256]);
        assert_eq!(h.b, expected_b);
        assert_eq!(h.luma, expected_luma);
        assert_eq!(h.total, 256);
        // A >1 MP image retains the same 2D grid and ignores unsampled odd rows/columns.
        let img = Rgba8::from_fn(1001, 1000, |x, y| if x % 2 == 0 && y % 2 == 0 { [255; 4] } else { [0; 4] });
        let h = Histogram::of_srgb8(&img);
        assert_eq!(h.total, 250_500);
        assert_eq!(h.r[255], h.total);
        assert_eq!(h.luma[255], h.total);
        assert_eq!(h.r[0], 0);
        assert_eq!(h.clipping(), (0.0, 1.0));
        assert_eq!(h.mean_luma(), 1.0);
        assert_eq!(h.percentile(0.5), 1.0);
    }

    #[test]
    fn j01_shared_histogram_luma_weights_primary_goldens() {
        let pixels = [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0], [0.5, 0.5, 0.5, 1.0]];
        for (weights, bins) in [(LumaWeights::REC709, [54, 182, 18, 128]), (LumaWeights::REC2020, [67, 173, 15, 128])] {
            let h = RgbHistogram::from_rgba_with_weights(&pixels, weights);
            let mut expected = [0; BINS];
            for bin in bins {
                expected[bin] += 1;
            }
            assert_eq!(h.luma, expected);
            assert_eq!(h.samples, 4);
        }
        assert_eq!(RgbHistogram::from_rgba(&pixels), RgbHistogram::from_rgba_with_weights(&pixels, LumaWeights::REC709));
        assert_eq!(LumaWeights::new([2.0, 6.0, 2.0]).map(LumaWeights::coefficients), Some([0.2, 0.6, 0.2]));
        for weights in [[0.0; 3], [-1.0, 2.0, 0.0], [f64::NAN, 1.0, 0.0], [f64::INFINITY, 1.0, 0.0]] {
            assert_eq!(LumaWeights::new(weights), None);
        }
    }

    #[test]
    fn j01_shared_histogram_float_contract_and_stride_goldens() {
        let pixels = [[0.0001, 0.9999, 0.5, 1.0], [-2.0, 4.0, 0.5, 1.0], [f32::NAN, 1.0, 1.0, 0.0], [0.0, f32::INFINITY, 1.0, 1.0], [0.5, 0.5, 0.5, f32::NAN]];
        let h = RgbHistogram::from_rgba(&pixels);
        assert_eq!((h.samples, h.transparent, h.invalid), (2, 1, 2));
        assert_eq!(h.shadows, [1, 0, 0]);
        assert_eq!(h.highlights, [0, 1, 0]);
        assert_eq!(h.underflow, [1, 0, 0]);
        assert_eq!(h.overflow, [0, 1, 0]);
        let mut luma = [0; BINS];
        luma[192] = 1;
        luma[255] = 1;
        assert_eq!(h.luma, luma);
        let pixels: Vec<_> = (0..20).map(|i| [i as f32 / 255.0; 4]).collect();
        let sampled = RgbHistogram::from_rgba_sampled(&pixels, 5, 4, 2, LumaWeights::REC709);
        let exact = RgbHistogram::from_rgba(&[pixels[0], pixels[2], pixels[4], pixels[10], pixels[12], pixels[14]]);
        assert_eq!(sampled, exact);
        assert_eq!(RgbHistogram::from_rgba_sampled(&pixels, 5, 4, 0, LumaWeights::REC709), RgbHistogram::from_rgba(&pixels));
        assert_eq!(RgbHistogram::from_rgba_sampled(&pixels, 0, 4, 2, LumaWeights::REC709), RgbHistogram::default());
    }

    #[test]
    fn j01_shared_histogram_preview_byte_goldens() {
        let pixels = [[255, 0, 0, 0], [0, 255, 0, 255], [0, 0, 255, 127], [128, 128, 128, 255], [255; 4]];
        let h = RgbHistogram::from_srgb8_preview(&pixels, 5, 1);
        let mut expected = [0; BINS];
        for bin in [53, 182, 18, 128, 255] {
            expected[bin] += 1;
        }
        assert_eq!(h.luma, expected);
        assert_eq!((h.samples, h.transparent, h.invalid), (5, 0, 0));
        assert_eq!((preview_stride(1_000_000), preview_stride(1_000_001), preview_stride(4_000_001)), (1, 2, 3));
        let pixels: Vec<_> = (0..=255).map(|i| [i, i, i, 255]).collect();
        let h = RgbHistogram::from_srgb8_preview(&pixels, 16, 16);
        assert_eq!(h.luma, [1; BINS]);
        assert_eq!(h.channels, [[1; BINS]; 3]);
    }
}
