//! Histograms of display-encoded images (what the Histogram panel shows).

use serde::Serialize;

pub use li_raster::histogram::{LumaWeights, RgbHistogram, clipping, preview_stride};

use crate::Rgba8;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Histogram {
    pub r: Vec<u32>,
    pub g: Vec<u32>,
    pub b: Vec<u32>,
    pub luma: Vec<u32>,
    pub total: u32,
}

impl Histogram {
    pub const BINS: usize = 256;

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
    fn counts_and_stats() {
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
}
