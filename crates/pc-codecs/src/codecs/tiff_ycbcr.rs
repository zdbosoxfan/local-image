//! YCbCr (PhotometricInterpretation 6) to RGB, after the `tiff` crate has decoded the samples.
//!
//! The crate decompresses JPEG data and upsamples subsampled chroma, but hands back the stored
//! Y, Cb and Cr values. This module turns them into RGB in place: ReferenceBlackWhite scales the
//! codes and YCbCrCoefficients gives the luma weights. JPEG-compressed data (compression 7) uses
//! the JPEG (JFIF) conversion, full range with the BT.601 weights, and ignores both tags, as
//! libtiff does when it asks the JPEG library for RGB.
//!
//! Sources: TIFF 6.0 Section 21 "YCbCr Images" (YCbCrCoefficients and its inverse equations,
//! pp. 90-91) and libtiff's `tif_color.c` (`TIFFYCbCrToRGBInit`, `TIFFYCbCrtoRGB`).
//!
//! Red and blue follow the spec. Green follows libtiff, which adds Y unscaled; the two agree only
//! when the weights sum to 1, as every standard set does. For other weights the spec's equation
//! tints neutral grays, so such files open here as they do in libtiff-based tools.

use super::tiff_ifd::{File, Ifd};
use crate::image::SampleType;

const TAG_YCBCR_COEFFICIENTS: u16 = 529;
const TAG_REFERENCE_BLACK_WHITE: u16 = 532;

/// BT.601 luma weights: the TIFF default for YCbCrCoefficients.
const BT601: [f64; 3] = [0.299, 0.587, 0.114];

/// How one file's YCbCr codes map to RGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct YCbCr {
    /// Luma weights of red, green and blue.
    luma: [f64; 3],
    /// Black and white codes of Y, Cb and Cr.
    ref_bw: [f64; 6],
}

/// The values of a numeric tag as floats, when it holds exactly `N` (`None` for any other count
/// or an unreadable value): RATIONAL, FLOAT, DOUBLE or an integer.
fn tag_floats<const N: usize>(f: &File<'_>, dir: &Ifd, tag: u16) -> Option<[f64; N]> {
    let e = dir.get(tag).filter(|e| e.count == N as u64)?;
    let float = |i: u64| -> Option<f64> {
        match e.ty {
            5 => {
                let at = e.value_at.checked_add(i.checked_mul(8)?)?;
                let (n, d) = (f.u32_at(at)?, f.u32_at(at.checked_add(4)?)?);
                (d != 0).then(|| f64::from(n) / f64::from(d))
            }
            11 => Some(f64::from(f32::from_bits(f.u32_at(e.value_at.checked_add(i.checked_mul(4)?)?)?))),
            12 => Some(f64::from_bits(f.u64_at(e.value_at.checked_add(i.checked_mul(8)?)?)?)),
            _ => f.uint(e, i).map(|v| v as f64),
        }
    };
    let mut out = [0.0; N];
    for (i, v) in out.iter_mut().enumerate() {
        *v = float(i as u64)?;
    }
    Some(out)
}

impl YCbCr {
    /// The conversion for directory `dir`, whose samples have `max` as their largest code
    /// (255 or 65535). Missing or unusable tags fall back to the TIFF defaults.
    pub(super) fn of(f: &File<'_>, dir: &Ifd, compression: u64, max: f64) -> Self {
        let defaults = Self::defaults(max);
        if compression == 7 {
            return defaults;
        }
        let luma = tag_floats::<3>(f, dir, TAG_YCBCR_COEFFICIENTS).filter(|l| l.iter().all(|c| c.is_finite() && *c > 0.0 && *c < 1.0)).unwrap_or(defaults.luma);
        let ref_bw = tag_floats::<6>(f, dir, TAG_REFERENCE_BLACK_WHITE)
            .filter(|r| r.iter().all(|v| v.is_finite()) && r.as_chunks::<2>().0.iter().all(|[black, white]| (white - black).abs() > f64::EPSILON))
            .unwrap_or(defaults.ref_bw);
        YCbCr { luma, ref_bw }
    }

    /// TIFF's defaults: BT.601 weights, full-range luma, chroma centred on the middle code.
    fn defaults(max: f64) -> Self {
        let mid = (max + 1.0) / 2.0;
        YCbCr { luma: BT601, ref_bw: [0.0, max, mid, max, mid, max] }
    }

    /// Converts interleaved Y, Cb, Cr pixels of `channels` samples (the first three are the
    /// colour) to R, G, B in place. Other sample types are left as they are.
    pub(super) fn convert(self, data: &mut [u8], channels: usize, sample: SampleType) {
        match sample {
            SampleType::U8 => {
                let t = Tables::new(&self, 255);
                for_each_pixel(data, channels, |px| {
                    if let [y, cb, cr, ..] = px {
                        let [r, g, b] = t.rgb(usize::from(*y), usize::from(*cb), usize::from(*cr));
                        (*y, *cb, *cr) = (r as u8, g as u8, b as u8);
                    }
                });
            }
            SampleType::U16 => {
                let t = Tables::new(&self, 65535);
                for_each_pixel(data, channels * 2, |px| {
                    if let [y0, y1, b0, b1, r0, r1, ..] = px {
                        let get = |a: u8, b: u8| usize::from(u16::from_ne_bytes([a, b]));
                        let [r, g, b] = t.rgb(get(*y0, *y1), get(*b0, *b1), get(*r0, *r1));
                        [*y0, *y1] = (r as u16).to_ne_bytes();
                        [*b0, *b1] = (g as u16).to_ne_bytes();
                        [*r0, *r1] = (b as u16).to_ne_bytes();
                    }
                });
            }
            _ => {}
        }
    }
}

/// Runs `f` on every `stride`-byte pixel, in parallel on native targets.
fn for_each_pixel(data: &mut [u8], stride: usize, f: impl Fn(&mut [u8]) + Sync + Send) {
    if stride == 0 {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        // Bands of whole pixels, so each task has a useful amount of work.
        let band = stride.saturating_mul(1 << 14);
        data.par_chunks_mut(band).for_each(|b| b.chunks_exact_mut(stride).for_each(&f));
    }
    #[cfg(target_arch = "wasm32")]
    data.chunks_exact_mut(stride).for_each(f);
}

/// Per-code contributions, so a pixel costs a few additions.
struct Tables {
    max: f64,
    /// Scaled luma of each Y code.
    y: Vec<f64>,
    /// Red and green contributions of each Cr code.
    cr_r: Vec<f64>,
    cr_g: Vec<f64>,
    /// Green and blue contributions of each Cb code.
    cb_g: Vec<f64>,
    cb_b: Vec<f64>,
}

impl Tables {
    fn new(c: &YCbCr, max: u32) -> Self {
        let m = f64::from(max);
        let [lr, lg, lb] = c.luma;
        let [y_black, y_white, cb_black, cb_white, cr_black, cr_white] = c.ref_bw;
        // TIFF 6.0: chroma codes span +-(max + 1) / 2 - 1 around the black code; with the
        // default ReferenceBlackWhite that is the code minus the middle code.
        let half = (m + 1.0) / 2.0 - 1.0;
        let codes = || (0..=max).map(f64::from);
        let y = codes().map(|v| (v - y_black) * m / (y_white - y_black)).collect();
        let cb: Vec<f64> = codes().map(|v| (v - cb_black) * half / (cb_white - cb_black)).collect();
        let cr: Vec<f64> = codes().map(|v| (v - cr_black) * half / (cr_white - cr_black)).collect();
        // R = Y + (2 - 2 Lr) Cr and B = Y + (2 - 2 Lb) Cb (TIFF 6.0 p. 91); green as libtiff:
        // G = Y - (Lr (2 - 2 Lr) / Lg) Cr - (Lb (2 - 2 Lb) / Lg) Cb.
        let (kr, kb) = (2.0 - 2.0 * lr, 2.0 - 2.0 * lb);
        Tables {
            max: m,
            y,
            cr_r: cr.iter().map(|v| kr * v).collect(),
            cr_g: cr.iter().map(|v| -lr * kr * v / lg).collect(),
            cb_g: cb.iter().map(|v| -lb * kb * v / lg).collect(),
            cb_b: cb.iter().map(|v| kb * v).collect(),
        }
    }

    /// R, G and B of one pixel, rounded and clamped to the code range.
    fn rgb(&self, y: usize, cb: usize, cr: usize) -> [u32; 3] {
        let at = |t: &[f64], i: usize| t.get(i).copied().unwrap_or(0.0);
        let yv = at(&self.y, y);
        let r = yv + at(&self.cr_r, cr);
        let g = yv + at(&self.cr_g, cr) + at(&self.cb_g, cb);
        let b = yv + at(&self.cb_b, cb);
        [r, g, b].map(|v| v.round().clamp(0.0, self.max) as u32)
    }
}
