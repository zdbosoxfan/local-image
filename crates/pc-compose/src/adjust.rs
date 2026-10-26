//! Evaluation of adjustment layers on a composite buffer.
//!
//! Formulas are documented approximations of Photoshop behaviour. Exact matching is tuned in
//! milestone M7 against Photoshop-rendered PSD composites (the oracle in `testkit`).

use li_color::convert::{D50, SRGB_TO_XYZ_D50, XYZ_D50_TO_SRGB, mat3_mul, srgb_to_linear};
use photocraft_color::SampleType;
use photocraft_doc::Adjustment;
use photocraft_doc::adjust::{CurvePoint, HueRange, LevelsChannel, ToneSpace};

use crate::Buffer;

/// Integer Legacy Brightness/Contrast pivots around Photoshop's mid-grey sample.
pub const LEGACY_PIVOT: f32 = 128.0 / 255.0;

/// Number of values in the table of a `size`³ Color Lookup (`size³ × 3`). `None` for a size below
/// 2 or one whose table length overflows: a stored document's size is untrusted. (Upstream
/// 5049a1d0 keeps this in `photocraft_doc::adjust::lut3d_len`; pc-doc is not ours to edit here.)
pub fn lut3d_len(size: u32) -> Option<usize> {
    if size < 2 {
        return None;
    }
    usize::try_from(size).ok()?.checked_pow(3)?.checked_mul(3)
}

/// The document tone curve used to linearize values for adjustments that
/// work in linear light (Exposure). Until ICC profiles are wired in (M8) this
/// approximates the Photoshop defaults: sRGB for colour documents and the
/// "Dot Gain 20%" grey profile (≈ gamma 1.73, fitted on the corpus) for
/// grayscale documents. 32-bit documents store linear light already.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transfer {
    /// sRGB piecewise curve.
    Srgb,
    /// Pure power law with this exponent.
    Gamma(f32),
}

impl Transfer {
    /// Default transfer for a document's colour mode and depth. 32-bit samples are linear, and
    /// Photoshop's Exposure works on them as stored (photoshop corpus rgb32 and gray32
    /// exposure.psd: within 0.5/255, against 69/255 through the 2.2 curve).
    pub fn for_document(mode: photocraft_color::ColorMode, depth: photocraft_color::SampleType) -> Self {
        if depth == photocraft_color::SampleType::F32 {
            return Transfer::Gamma(1.0);
        }
        match mode {
            photocraft_color::ColorMode::Grayscale | photocraft_color::ColorMode::Duotone | photocraft_color::ColorMode::Bitmap => Transfer::Gamma(1.732),
            _ => Transfer::Srgb,
        }
    }
    /// The tone curve Exposure linearises through: RGB documents use a pure 2.2 power, not the
    /// sRGB curve (psd-tools adjustment_nested_composition_4: an offset of 0.1738 lifts 76 to 134,
    /// not 136; ag-psd masks), grey documents their dot-gain gamma.
    pub fn for_exposure(self) -> Self {
        match self {
            Transfer::Srgb => Transfer::Gamma(2.2),
            t => t,
        }
    }
    fn decode(self, v: f32) -> f32 {
        match self {
            Transfer::Srgb => li_color::convert::srgb_to_linear(v.max(0.0)),
            Transfer::Gamma(g) => v.max(0.0).powf(g),
        }
    }
    fn encode(self, v: f32) -> f32 {
        match self {
            Transfer::Srgb => li_color::convert::linear_to_srgb(v.max(0.0)),
            Transfer::Gamma(g) => v.max(0.0).powf(1.0 / g),
        }
    }
}

/// Applies an adjustment assuming an sRGB document.
pub fn apply(adj: &Adjustment, buf: &mut Buffer) {
    apply_with(adj, buf, Transfer::Srgb);
}

/// Applies an adjustment with the document's tone transfer.
pub fn apply_with(adj: &Adjustment, buf: &mut Buffer, transfer: Transfer) {
    apply_depth(adj, buf, transfer, None);
}

/// Applies an adjustment with the document's tone transfer, in a document of `depth` (`None`:
/// unrounded). Integer depths work on whole levels like Photoshop's (see [`levels_q`] and
/// `adjustment_quantum`), 32-bit documents get its float Levels (see [`levels_float`]).
pub fn apply_depth(adj: &Adjustment, buf: &mut Buffer, transfer: Transfer, depth: Option<SampleType>) {
    match adj {
        Adjustment::Invert => map_rgb(buf, |c| [1.0 - c[0], 1.0 - c[1], 1.0 - c[2]]),
        Adjustment::Threshold { level } => map_rgb(buf, |c| [if threshold_white(c, *level, depth) { 1.0 } else { 0.0 }; 3]),
        Adjustment::Posterize { levels } => map_rgb(buf, |c| c.map(|v| posterize_in(v, *levels, depth))),
        Adjustment::BrightnessContrast { brightness, contrast, legacy: true } => {
            // Photoshop adds brightness before raised contrast and after lowered contrast.
            let b = brightness / 255.0;
            let c = contrast.clamp(-100.0, 99.0);
            let k = if c >= 0.0 { 1.0 / (1.0 - c / 100.0) } else { 1.0 + c / 100.0 };
            let (pre, post) = if c >= 0.0 { (b, 0.0) } else { (0.0, b) };
            let hdr = depth == Some(SampleType::F32);
            let pivot = if hdr { 127.0 / 255.0 } else { LEGACY_PIVOT };
            map_rgb(buf, |px| {
                px.map(|v| {
                    let out = (v + pre - pivot) * k + pivot + post;
                    if hdr { out } else { out.clamp(0.0, 1.0) }
                })
            })
        }
        Adjustment::BrightnessContrast { brightness, contrast, .. } => {
            // Modern (CS3+) Brightness/Contrast: smooth curves that pin pure black and white (see
            // `modern_brightness` and `modern_contrast`), brightness then contrast through one table.
            let table = modern_lut(*brightness, *contrast);
            map_rgb(buf, |px| px.map(|v| lut(&table, v)))
        }
        Adjustment::Exposure { exposure, offset, gamma } => {
            // In linear light: (lin·2^exposure + offset)^(1/gamma), then back
            // through the document tone curve (fitted on the corpus).
            let m = 2f32.powf(*exposure);
            let g = gamma.max(0.01);
            let transfer = transfer.for_exposure();
            let hdr = depth == Some(SampleType::F32);
            map_rgb(buf, |c| {
                c.map(|v| {
                    if hdr {
                        let lin = v * m + offset;
                        return if g == 1.0 { lin } else { lin.signum() * lin.abs().powf(1.0 / g) };
                    }
                    let lin = (transfer.decode(v) * m + offset).max(0.0).powf(1.0 / g);
                    transfer.encode(lin).clamp(0.0, 1.0)
                })
            })
        }
        Adjustment::Levels { master, per_channel, space: ToneSpace::Rgb, .. } if depth == Some(SampleType::F32) => {
            // The LUT domain cannot represent signed or unbounded samples.
            let master_identity = *master == LevelsChannel::default();
            let channel_identity = per_channel.each_ref().map(|ch| *ch == LevelsChannel::default());
            if !master_identity || channel_identity.iter().any(|&v| !v) {
                map_rgb(buf, |c| {
                    std::array::from_fn(|i| {
                        let v = if channel_identity[i] { c[i] } else { levels_float(&per_channel[i], c[i]) };
                        if master_identity { v } else { levels_float(master, v) }
                    })
                });
            }
        }
        Adjustment::Levels { space, .. } | Adjustment::Curves { space, .. } => {
            let identity = float_tone_identity(adj, depth);
            if identity == [true; 3] {
                return;
            }
            let luts = tone_luts_depth(adj, depth);
            apply_tone(buf, *space, &luts, identity);
        }
        Adjustment::HueSaturation { hue, saturation, lightness, colorize, ranges } => {
            let table = (!*colorize && ranges.iter().any(|r| !r.is_neutral())).then(|| hue_range_tables(ranges));
            map_rgb(buf, |c| {
                let (dh, ds, dl) = match &table {
                    Some(t) => {
                        // Range edits follow the pixel's original hue, in full however dull the colour
                        // (Photoshop's merged images). Greys stay grey: no hue to shift, no
                        // saturation to scale, and range Lightness moves towards a channel they share.
                        let h0 = rgb_to_hsl(c).0;
                        (lut(&t[0], h0), lut(&t[1], h0), lut(&t[2], h0))
                    }
                    None => (0.0, 0.0, 0.0),
                };
                let out = hue_saturation_in(c, *hue + dh, (*saturation / 100.0 + ds).clamp(-1.0, 1.0), (*lightness / 100.0).clamp(-1.0, 1.0), *colorize, depth);
                range_lightness(out, dl)
            })
        }
        Adjustment::Vibrance { vibrance, saturation } => {
            let (v, s) = (*vibrance / 100.0, *saturation / 100.0);
            map_rgb(buf, |c| {
                let (h, sat, l) = rgb_to_hsl(c);
                let boost = v * (1.0 - sat); // less saturated colours move more
                let ns = (sat * (1.0 + s) + boost * sat.max(0.1)).clamp(0.0, 1.0);
                hsl_to_rgb(h, ns, l)
            })
        }
        Adjustment::ChannelMixer { matrix, monochrome } => {
            let hdr = depth == Some(SampleType::F32);
            // Float source percentages truncate to 1/1024; constants count 255/512.
            let m = if hdr {
                matrix.map(|row| std::array::from_fn(|i| if i == 3 { row[i] * 255.0 / 512.0 } else { (row[i] * 1024.0).trunc() / 1024.0 }))
            } else {
                *matrix
            };
            map_rgb(buf, |c| {
                let mix = |row: &[f32; 4]| {
                    let out = row[0] * c[0] + row[1] * c[1] + row[2] * c[2] + row[3];
                    if hdr { out } else { out.clamp(0.0, 1.0) }
                };
                if *monochrome { [mix(&m[0]); 3] } else { [mix(&m[0]), mix(&m[1]), mix(&m[2])] }
            })
        }
        Adjustment::PhotoFilter { color, density, preserve_luminosity } => {
            let linear_doc = depth == Some(SampleType::F32);
            let m = photo_filter_matrix(*color, *density, *preserve_luminosity && linear_doc);
            let set_lum = *preserve_luminosity && !linear_doc;
            map_rgb(buf, |c| {
                let f = if linear_doc { mat3_mul(&m, c) } else { mat3_mul(&m, c.map(|v| transfer.decode(v))).map(|v| transfer.encode(v)) };
                let f = if set_lum { photocraft_color::blend::set_lum(f, photocraft_color::blend::lum(c)) } else { f };
                if linear_doc { f } else { f.map(|v| v.clamp(0.0, 1.0)) }
            })
        }
        Adjustment::BlackWhite { weights, tint } => map_rgb(buf, |c| {
            let top = if depth == Some(SampleType::F32) { 16.0 } else { 1.0 };
            let g = black_white_gray(c, weights).clamp(0.0, top);
            match tint {
                Some(t) if depth == Some(SampleType::F32) => {
                    photocraft_color::blend::blend_rgb_within(photocraft_color::BlendMode::Color, [g; 3], *t, f32::MAX)
                }
                Some(t) => photocraft_color::blend::set_lum(*t, g).map(|v| v.clamp(0.0, 1.0)),
                None => [g; 3],
            }
        }),
        Adjustment::GradientMap { stops, reverse, dither } => {
            let (w, x0, y0) = (buf.rect.width().max(1) as usize, buf.rect.x0, buf.rect.y0);
            for (i, p) in buf.px.iter_mut().enumerate() {
                if p[3] <= 0.0 {
                    continue;
                }
                // Driven by the W3C luminosity (0.3, 0.59, 0.11), not the Rec. 601 grey: photoshop
                // corpus rgb8 and rgb16 gradient-map.psd match within a level (2 levels off with
                // 0.299, 0.587, 0.114).
                let mut t = photocraft_color::blend::lum([p[0], p[1], p[2]]);
                if *reverse {
                    t = 1.0 - t;
                }
                let mut o = gradient(stops, t);
                if *dither {
                    let d = bayer4(x0 + (i % w) as i32, y0 + (i / w) as i32) / 255.0;
                    o = o.map(|v| (v + d).clamp(0.0, 1.0));
                }
                p[..3].copy_from_slice(&o);
            }
        }
        Adjustment::ColorBalance { shadows, midtones, highlights, preserve_luminosity } => {
            let tones = [*shadows, *midtones, *highlights];
            map_rgb(buf, |c| std::array::from_fn(|ch| color_balance_channel(tones, *preserve_luminosity, ch, c[ch])))
        }
        Adjustment::SelectiveColor { relative, adjustments } => map_rgb(buf, |c| selective_color(c, *relative, adjustments)),
        Adjustment::ColorLookup { lut: Some(table), size, tetrahedral, dither, domain, .. } if lut3d_len(*size).is_some_and(|len| table.len() >= len) => {
            let (n, w, x0, y0) = (*size as usize, buf.rect.width().max(1) as usize, buf.rect.x0, buf.rect.y0);
            for (i, p) in buf.px.iter_mut().enumerate() {
                if p[3] <= 0.0 {
                    continue;
                }
                let input = [p[0], p[1], p[2]];
                let input = domain.map_or(input, |domain| li_color::lut::normalize_domain(input, domain.min, domain.max));
                let mut o = lut3d_sample(table, n, input, *tetrahedral);
                if *dither {
                    let d = bayer4(x0 + (i % w) as i32, y0 + (i / w) as i32) / 255.0;
                    o = o.map(|v| (v + d).clamp(0.0, 1.0));
                }
                p[..3].copy_from_slice(&o);
            }
        }
        // Not evaluated: identity (still round-trips through PSD).
        Adjustment::ColorLookup { .. } | Adjustment::Unsupported { .. } => {}
    }
}

/// Untouched float RGB curve channels retain their original signed samples.
pub fn float_tone_identity(adj: &Adjustment, depth: Option<SampleType>) -> [bool; 3] {
    match adj {
        Adjustment::Curves { master, per_channel, space: ToneSpace::Rgb, .. } if depth == Some(SampleType::F32) => {
            std::array::from_fn(|i| photocraft_doc::adjust::is_identity_curve(master) && photocraft_doc::adjust::is_identity_curve(&per_channel[i]))
        }
        _ => [false; 3],
    }
}

fn apply_tone(buf: &mut Buffer, space: ToneSpace, luts: &[Vec<f32>; 4], identity: [bool; 3]) {
    match space {
        ToneSpace::Rgb => map_rgb(buf, |c| std::array::from_fn(|i| if identity[i] { c[i] } else { lut(&luts[i], c[i]) })),
        ToneSpace::Cmyk | ToneSpace::Lab => map_rgb(buf, |c| tone_in_space(space, luts, c)),
    }
}

/// Whether Threshold at `level` turns colour `c` white in a document of `depth` (`None`: unknown,
/// treated as 8-bit). Photoshop splits on the luminosity 0.30 R + 0.59 G + 0.11 B (the blend
/// modes' `lum`), not the Rec. 601 grey, of the document's whole sample values:
/// - 8 bits: white when that luminosity, rounded to a level with halves down, reaches the level;
/// - 16 bits: white when it reaches the level converted to 16 bits (128 → 16448);
/// - 32 bits: white when it reaches the level.
///
/// Exact on every pixel of psd-tools threshold_rgb.psd, threshold_16bits_rgb.psd and
/// threshold_16bits_grayscale.psd and of the photoshop corpus's rgb16 threshold-128.psd, where the
/// Rec. 601 grey split 28 and 100 pixels of the first two the other way. The sums of
/// `30 R + 59 G + 11 B` over whole samples are exact in f32 (at most 3 276 800).
pub fn threshold_white(c: [f32; 3], level: f32, depth: Option<SampleType>) -> bool {
    let t = (level * 255.0).round();
    let sum = |q: f32| {
        let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * q).round());
        30.0 * r + 59.0 * g + 11.0 * b
    };
    match depth {
        None | Some(SampleType::U8) => sum(255.0) > 100.0 * t - 50.0,
        Some(SampleType::U16) => sum(32768.0) >= 100.0 * (t * 32768.0 / 255.0).round(),
        Some(SampleType::F32) => (0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]) * 255.0 >= t,
    }
}

/// Color Balance on channel `ch` (0 red, 1 green, 2 blue) of `v` (0..1); `tones` are the
/// shadows, midtones and highlights slider triples (cyan–red, magenta–green, yellow–blue,
/// −100..100).
///
/// Reverse-engineered from Photoshop (a 16-bit gray ramp through every tone at ±10..±100, pairs of
/// tones, and random settings of all nine sliders, Preserve Luminosity on and off): the three
/// tones fold into one Levels per channel (input black point, input white point, midtone gamma),
/// within 0.8 of 255 everywhere. Folding matters: a lowered shadows and a raised highlights
/// slider cancel each other's gamma, which applying the tones one after another doesn't.
/// - Preserve Luminosity off: a slider only moves its own channel. Lowered shadows set the black
///   point to |shadows| levels, raised highlights set the white point to 255 − highlights levels,
///   and the gamma is 2^(m / 100) with m = midtones + (shadows + highlights) / 2.
/// - Preserve Luminosity on: each channel's black point is (max − own) shadows levels, its white
///   point 255 − (own − min) highlights levels, and m = own − (max + min) / 2 midtones, so with
///   several midtones sliders moved the shared offset depends on the two extremes, not their sum.
///
/// A gamma > 1 starts with slope 2^gamma at black ([`balance_gamma`]: up to 18 of 255 brighter
/// near black without it). When the black point reaches the white point, input at or above the
/// white point is white and the rest black.
pub fn color_balance_channel(tones: [[f32; 3]; 3], preserve_luminosity: bool, ch: usize, v: f32) -> f32 {
    let [shadows, midtones, highlights] = tones.map(|t| t.map(|s| if s.is_finite() { s.clamp(-100.0, 100.0) } else { 0.0 }));
    let own = |t: [f32; 3]| t.get(ch).copied().unwrap_or(0.0);
    let max = |t: [f32; 3]| t[0].max(t[1]).max(t[2]);
    let min = |t: [f32; 3]| t[0].min(t[1]).min(t[2]);
    let (black, white, m) = if preserve_luminosity {
        (max(shadows) - own(shadows), own(highlights) - min(highlights), own(midtones) - (max(midtones) + min(midtones)) / 2.0)
    } else {
        ((-own(shadows)).max(0.0), own(highlights).max(0.0), own(midtones) + (own(shadows) + own(highlights)) / 2.0)
    };
    let (b, w) = (black / 255.0, 1.0 - white / 255.0);
    let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    if v >= w {
        return 1.0;
    }
    if v <= b {
        return 0.0;
    }
    balance_gamma((v - b) / (w - b), 2f32.powf(m / 100.0))
}

/// The midtone gamma of Color Balance's per-channel Levels (`u` in 0..1): `u^(1/gamma)`, except
/// that a gamma > 1 (which would start with infinite slope) starts at slope 2^gamma and joins the
/// power curve in a cubic Hermite at twice the input where that line meets it (Photoshop,
/// measured on a 16-bit ramp at gamma 1.07..2: within 0.02 of 255).
fn balance_gamma(u: f32, gamma: f32) -> f32 {
    let e = 1.0 / gamma.max(0.01);
    let power = u.powf(e);
    if gamma <= 1.0 {
        return power;
    }
    let s0 = 2f32.powf(gamma);
    let u0 = (2.0 * s0.powf(1.0 / (e - 1.0))).min(1.0);
    if u0.is_nan() || u0 <= 0.0 || u >= u0 {
        return power;
    }
    let (p, d, t) = (u0.powf(e), e * u0.powf(e - 1.0), u / u0);
    let (t2, t3) = (t * t, t * t * t);
    ((t3 - 2.0 * t2 + t) * u0 * s0 + (3.0 * t2 - 2.0 * t3) * p + (t3 - t2) * u0 * d).clamp(0.0, 1.0)
}

/// Per-channel LUTs of a Levels or Curves adjustment, each channel's record composed with the
/// master (Photoshop applies the channel curve first, then the composite). Four rows: the three
/// `per_channel` channels then black (identity unless the space is CMYK). In Lab there is no
/// composite record, so the master is ignored.
/// Samples have `quantum` steps per unit (Levels works on whole levels; see [`levels_q`]).
pub fn tone_luts_q(adj: &Adjustment, quantum: Option<f32>) -> [Vec<f32>; 4] {
    let x = |k: usize| k as f32 / (LUT_SIZE - 1) as f32;
    match adj {
        Adjustment::Levels { master, per_channel, space, black } => {
            let ident = LevelsChannel::default();
            let m = if *space == ToneSpace::Lab { &ident } else { master };
            let row = |c: &LevelsChannel| (0..LUT_SIZE).map(|k| levels_q(m, levels_q(c, x(k), quantum), quantum)).collect();
            [row(&per_channel[0]), row(&per_channel[1]), row(&per_channel[2]), row(if *space == ToneSpace::Cmyk { black } else { &ident })]
        }
        Adjustment::Curves { master, per_channel, space, black } => {
            let m = if *space == ToneSpace::Lab { curve_lut(&[]) } else { curve_lut(master) };
            let row = |c: &[CurvePoint]| curve_lut(c).iter().map(|&v| lut(&m, v)).collect();
            [row(&per_channel[0]), row(&per_channel[1]), row(&per_channel[2]), row(if *space == ToneSpace::Cmyk { black } else { &[] })]
        }
        _ => std::array::from_fn(|_| (0..LUT_SIZE).map(x).collect()),
    }
}

/// [`tone_luts_q`] in a document of `depth` (`None`: unrounded). 32-bit Levels use
/// [`levels_float`], bounded to 0..1 for LUT consumers. Float RGB rendering evaluates it directly.
pub fn tone_luts_depth(adj: &Adjustment, depth: Option<SampleType>) -> [Vec<f32>; 4] {
    match (adj, depth) {
        (Adjustment::Levels { master, per_channel, space: ToneSpace::Rgb, .. }, Some(SampleType::F32)) => {
            let x = |k: usize| k as f32 / (LUT_SIZE - 1) as f32;
            let row = |c: &LevelsChannel| (0..LUT_SIZE).map(|k| levels_float(master, levels_float(c, x(k))).clamp(0.0, 1.0)).collect();
            [row(&per_channel[0]), row(&per_channel[1]), row(&per_channel[2]), (0..LUT_SIZE).map(x).collect()]
        }
        _ => tone_luts_q(adj, depth.and_then(crate::adjustment_quantum)),
    }
}

/// Applies channel LUTs to a display-RGB colour in CMYK (ink brightness, 1 - ink) or Lab space,
/// through the conversions the document's surfaces use. The change is added as a difference of
/// two round trips, so channels a curve leaves alone (and out-of-gamut colours) stay exact.
fn tone_in_space(space: ToneSpace, luts: &[Vec<f32>; 4], c: [f32; 3]) -> [f32; 3] {
    use li_color::convert::{lab_to_srgb, srgb_to_lab};
    use photocraft_color::convert::{cmyk_to_rgb, rgb_to_cmyk};
    let (before, after) = match space {
        ToneSpace::Cmyk => {
            let ink = rgb_to_cmyk(c);
            let out: [f32; 4] = std::array::from_fn(|i| 1.0 - lut(&luts[i], 1.0 - ink[i]));
            if out == ink {
                return c;
            }
            (cmyk_to_rgb(ink), cmyk_to_rgb(out))
        }
        ToneSpace::Lab => {
            let l = srgb_to_lab(c);
            let n = [l[0] / 100.0, (l[1] + 128.0) / 255.0, (l[2] + 128.0) / 255.0];
            let o: [f32; 3] = std::array::from_fn(|i| lut(&luts[i], n[i]));
            if o == n {
                return c;
            }
            let back = |v: [f32; 3]| lab_to_srgb([v[0] * 100.0, v[1] * 255.0 - 128.0, v[2] * 255.0 - 128.0]);
            (back(n), back(o))
        }
        ToneSpace::Rgb => return std::array::from_fn(|i| lut(&luts[i], c[i])),
    };
    std::array::from_fn(|i| (c[i] + after[i] - before[i]).clamp(0.0, 1.0))
}

/// Hue/Saturation's range edits as three hue-indexed tables (hue shift in degrees, saturation and
/// lightness as fractions), sampled at hue `k / (len - 1)` turns. Ranges add up where they overlap.
pub fn hue_range_tables(ranges: &[HueRange; 6]) -> [Vec<f32>; 3] {
    let at = |k: usize| {
        let deg = k as f32 / (LUT_SIZE - 1) as f32 * 360.0;
        ranges.iter().filter(|r| !r.is_neutral()).fold([0.0f32; 3], |acc, r| {
            let w = r.weight(deg);
            [acc[0] + w * r.hue, acc[1] + w * r.saturation / 100.0, acc[2] + w * r.lightness / 100.0]
        })
    };
    let rows: Vec<[f32; 3]> = (0..LUT_SIZE).map(at).collect();
    std::array::from_fn(|i| rows.iter().map(|r| r[i]).collect())
}

/// One Hue/Saturation evaluation: hue shift in degrees, saturation and lightness in -1..=1.
/// Colorize sets the hue and saturation instead of shifting them.
pub fn hue_saturation(c: [f32; 3], hue: f32, s: f32, l: f32, colorize: bool) -> [f32; 3] {
    let (mut hh, mut ss, ll) = rgb_to_hsl(c);
    if colorize {
        hh = hue.rem_euclid(360.0) / 360.0;
        ss = s.abs().max(0.25);
    } else {
        hh = (hh + hue / 360.0).rem_euclid(1.0);
        ss = (ss * (1.0 + s)).clamp(0.0, 1.0);
    }
    let mut rgb = hsl_to_rgb(hh, ss, ll);
    if l > 0.0 {
        rgb = rgb.map(|v| v + (1.0 - v) * l);
    } else if l < 0.0 {
        rgb = rgb.map(|v| v * (1.0 + l));
    }
    rgb
}

/// At 32 bits, scale chroma around lightness without clipping. HSL saturation is undefined at
/// lightness 0 or 1 and negative above 1, so it cannot carry bright or negative-channel colours.
pub fn hue_saturation_in(c: [f32; 3], hue: f32, s: f32, l: f32, colorize: bool, depth: Option<SampleType>) -> [f32; 3] {
    if depth != Some(SampleType::F32) || colorize {
        return hue_saturation(c, hue, s, l, colorize);
    }
    let lc = c.map(|v| if l > 0.0 { v + (1.0 - v) * l } else { v * (1.0 + l) });
    let (h, _, lightness) = rgb_to_hsl(lc);
    let h = (h + hue / 360.0).rem_euclid(1.0);
    let chroma = (lc[0].max(lc[1]).max(lc[2]) - lc[0].min(lc[1]).min(lc[2])) * hdr_saturation_factor(s);
    let lo = lightness - chroma / 2.0;
    // Reuse the shared hue conversion on a unit-chroma colour, avoiding HSL's singular divisor.
    hsl_to_rgb(h, 1.0, 0.5).map(|v| lo + chroma * v)
}

/// Photoshop's 32-bit saturation factor: lowering in 1/256 steps, raising in 254/255 steps,
/// quantized to 1/256 and capped at 32767/256. The shader uses the same integer division.
fn hdr_saturation_factor(s: f32) -> f32 {
    // Keep master plus range sums on their step: -0.46 + 0.96 is 0.49999997 in f32.
    let s = s.clamp(-1.0, 1.0);
    if s <= 0.0 {
        return 1.0 - (-s * 256.0 + 1e-4).floor() / 256.0;
    }
    let steps = (s * 254.0 + 1e-4).floor() as u32;
    (65280 / 255u32.saturating_sub(steps).max(1)).min(32767) as f32 / 256.0
}

/// A colour range's Lightness, applied after the master edits. It moves each channel towards the
/// pixel's brightest channel when raised and towards its darkest when lowered, not towards white
/// or black: Blues +50 turns (0, 29, 115) into (57, 71, 115), Blues -50 turns (102, 128, 204) into
/// (102, 115, 153) (measured in Photoshop 27.11).
pub fn range_lightness(c: [f32; 3], l: f32) -> [f32; 3] {
    let l = l.clamp(-1.0, 1.0);
    let target = if l > 0.0 { c[0].max(c[1]).max(c[2]) } else { c[0].min(c[1]).min(c[2]) };
    c.map(|v| v + (target - v) * l.abs())
}

/// Black & White grey value. Each colour is the sum of a grey part (its smallest channel), a
/// secondary-colour part (yellow, cyan or magenta: middle − smallest) and a primary part (red,
/// green or blue: largest − middle); the sliders (percent, Photoshop defaults 40 60 40 60 20 80 for
/// reds, yellows, greens, cyans, blues, magentas) weight the two colour parts.
pub fn black_white_gray(c: [f32; 3], weights: &[f32; 6]) -> f32 {
    let (r, g, b) = (c[0], c[1], c[2]);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let mid = r + g + b - max - min;
    // Primary: the largest channel; secondary: the two largest channels together.
    let primary = if r >= g && r >= b {
        0
    } else if g >= b {
        2
    } else {
        4
    };
    let secondary = if b <= r && b <= g {
        1 // red + green = yellow
    } else if r <= g {
        3 // green + blue = cyan
    } else {
        5 // red + blue = magenta
    };
    min + (mid - min) * weights[secondary] / 100.0 + (max - mid) * weights[primary] / 100.0
}

/// Ordered-dither offset in -0.5..0.5 (4×4 Bayer matrix) for document pixel (x, y).
pub fn bayer4(x: i32, y: i32) -> f32 {
    const M: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
    (M[((y & 3) * 4 + (x & 3)) as usize] + 0.5) / 16.0 - 0.5
}

/// Samples a 3D LUT (`n`³ RGB triplets, red fastest) at `c`, trilinear or tetrahedral.
pub fn lut3d_sample(lut: &[f32], n: usize, c: [f32; 3], tetrahedral: bool) -> [f32; 3] {
    li_color::lut::sample_rgb(lut, n, c, tetrahedral)
}

/// How much a colour belongs to each Selective Color range: reds, yellows, greens, cyans,
/// blues, magentas (by chroma within the hue sector), whites, neutrals and blacks (by lightness).
pub fn selective_color_weights(c: [f32; 3]) -> [f32; 9] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let mid = c[0] + c[1] + c[2] - max - min;
    let top = |i: usize| if c[i] >= max { max - mid } else { 0.0 };
    let bottom = |i: usize| if c[i] <= min { mid - min } else { 0.0 };
    [
        top(0),
        bottom(2),
        top(1),
        bottom(0),
        top(2),
        bottom(1),
        ((min - 0.5) * 2.0).max(0.0),
        (1.0 - (max - 0.5).abs() - (min - 0.5).abs()).clamp(0.0, 1.0),
        ((0.5 - max) * 2.0).max(0.0),
    ]
}

/// Photoshop Selective Color (approximation): each range shifts the cyan, magenta and yellow ink
/// (1 − R, G, B) by its percentage, plus its black percentage on every ink, weighted by how much
/// the colour belongs to the range. Relative mode scales the shift by the ink already present
/// (so it can't tint pure white); absolute adds it outright.
pub fn selective_color(c: [f32; 3], relative: bool, adj: &[[f32; 4]; 9]) -> [f32; 3] {
    let w = selective_color_weights(c);
    let mut delta = [0.0f32; 3];
    for (r, wr) in w.iter().enumerate() {
        if *wr <= 0.0 {
            continue;
        }
        let a = adj[r].map(|v| v / 100.0);
        for i in 0..3 {
            let ink = 1.0 - c[i];
            let d = if relative { (a[i] + a[3]) * ink } else { a[i] + a[3] };
            delta[i] += d * wr;
        }
    }
    std::array::from_fn(|i| (c[i] - delta[i]).clamp(0.0, 1.0))
}

const LUT_SIZE: usize = 4096;

fn map_rgb(buf: &mut Buffer, f: impl Fn([f32; 3]) -> [f32; 3]) {
    for p in &mut buf.px {
        if p[3] <= 0.0 {
            continue;
        }
        let r = f([p[0], p[1], p[2]]);
        p[0] = r[0];
        p[1] = r[1];
        p[2] = r[2];
    }
}

#[inline]
fn lut(table: &[f32], v: f32) -> f32 {
    let x = v.clamp(0.0, 1.0) * (table.len() - 1) as f32;
    let i = x.floor() as usize;
    let j = (i + 1).min(table.len() - 1);
    let f = x - i as f32;
    table[i] * (1.0 - f) + table[j] * f
}

/// Photo Filter's linear-light RGB matrix. Photoshop multiplies the pixel's D50 XYZ, relative to
/// white, by the filter colour's (mixed in by `density`): a fixed linear map, so it is folded into
/// one matrix. With `normalize_y` the map keeps luminance Y (32-bit Preserve Luminosity; 8/16-bit
/// documents instead restore the encoded luminosity like the Luminosity blend mode). Fitted on the
/// photoshop corpus `photo-filter.psd`: rgb16 within 0.6/255 everywhere.
pub fn photo_filter_matrix(color: [f32; 3], density: f32, normalize_y: bool) -> [[f32; 3]; 3] {
    let d = if density.is_finite() { density.clamp(0.0, 1.0) } else { 0.0 };
    let xyz = mat3_mul(&SRGB_TO_XYZ_D50, color.map(|v| srgb_to_linear(v.clamp(0.0, 1.0))));
    let mut s: [f32; 3] = std::array::from_fn(|i| 1.0 - d + d * xyz[i] / D50[i]);
    if normalize_y && s[1] > 1e-6 {
        let y = s[1];
        s = s.map(|v| v / y);
    }
    // XYZ_D50_TO_SRGB · diag(s) · SRGB_TO_XYZ_D50
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| XYZ_D50_TO_SRGB[r][k] * s[k] * SRGB_TO_XYZ_D50[k][c]).sum()))
}

/// Modern Brightness curve (one channel, `brightness` in [-150, 150]); pins pure black and white,
/// and `b = 0` is the identity. Raised, it is a line of slope `s = 1.375^(b/50)` from the origin
/// up to a knee at output 126/255, then a cubic Hermite to (1, 1) whose end slope is
/// `1 / (1 + 0.084 b)`; past about +110, where no such cubic stays below white, the roll-off is
/// `1 - (1 - y0)(1 - u)^α` instead (continuous slope `α` at the knee, flat at white). Lowered, it
/// is the inverse of the curve raised by as much.
///
/// Fit to every Photoshop measurement we have, all within about a level and a half: its ±50
/// samples (`modern_brightness_matches_photoshop`), the photoshop corpus's brightness-contrast.psd
/// (+25 with contrast +30) and psd-tools' brightnesscontrast_rgb.psd, whose masked layers give
/// +23, -31 and -95 (+150 within 6 levels there, where every curve family tried bottoms out).
pub fn modern_brightness(v: f32, brightness: f32) -> f32 {
    if brightness == 0.0 || !brightness.is_finite() {
        return v;
    }
    Brightness::new(brightness).eval(v)
}

/// [`modern_brightness`] for one slider value, its curve's constants worked out once.
struct Brightness {
    raise: bool,
    s: f32,
    e: f32,
    x0: f32,
    /// The knee's slope over the roll-off's secant (`α` of Fritsch–Carlson); a power roll-off
    /// replaces the Hermite when α² + β² > 9.
    alpha: f32,
    hermite: bool,
}

/// Output level of the Brightness curve's knee.
const BRIGHTNESS_KNEE: f32 = 126.0 / 255.0;

impl Brightness {
    fn new(brightness: f32) -> Self {
        let b = brightness.abs();
        let s = 1.375f32.powf(b / 50.0);
        let e = 1.0 / (1.0 + 0.084 * b);
        let x0 = (BRIGHTNESS_KNEE / s).min(1.0);
        let secant = (1.0 - BRIGHTNESS_KNEE) / (1.0 - x0).max(1e-6);
        let (alpha, beta) = (s / secant, e / secant);
        Brightness { raise: brightness > 0.0, s, e, x0, alpha, hermite: alpha * alpha + beta * beta <= 9.0 }
    }

    fn eval(&self, v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        if self.raise {
            return self.raised(v);
        }
        if v <= 0.0 || v >= 1.0 {
            return v;
        }
        // The inverse of the raised curve (monotone on [0, 1]), by bisection.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..32 {
            let mid = 0.5 * (lo + hi);
            if self.raised(mid) < v {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// The curve raised: the line, then a monotone roll-off to (1, 1).
    fn raised(&self, x: f32) -> f32 {
        let Brightness { s, e, x0, alpha, hermite, .. } = *self;
        let y0 = BRIGHTNESS_KNEE;
        if x <= x0 {
            return y0.min(s * x);
        }
        let h = (1.0 - x0).max(1e-6);
        let t = ((x - x0) / h).clamp(0.0, 1.0);
        let y = if hermite {
            let (t2, t3) = (t * t, t * t * t);
            (2.0 * t3 - 3.0 * t2 + 1.0) * y0 + (t3 - 2.0 * t2 + t) * h * s + (-2.0 * t3 + 3.0 * t2) + (t3 - t2) * h * e
        } else {
            1.0 - (1.0 - y0) * (1.0 - t).powf(alpha)
        };
        y.clamp(0.0, 1.0)
    }
}

/// Modern Brightness/Contrast as one channel's lookup table (4096 samples over 0..1):
/// [`modern_brightness`] then [`modern_contrast`], as Photoshop applies them.
pub fn modern_lut(brightness: f32, contrast: f32) -> Vec<f32> {
    let step = |k: usize, n: usize| k as f32 / (n - 1) as f32;
    let bright: Vec<f32> = if brightness == 0.0 || !brightness.is_finite() {
        (0..LUT_SIZE).map(|k| step(k, LUT_SIZE)).collect()
    } else if brightness > 0.0 {
        let curve = Brightness::new(brightness);
        (0..LUT_SIZE).map(|k| curve.raised(step(k, LUT_SIZE))).collect()
    } else {
        // Lowered: invert the raised curve, sampled four times finer, in one monotone pass.
        let curve = Brightness::new(-brightness);
        let fine = 4 * LUT_SIZE;
        let ys: Vec<f32> = (0..fine).map(|j| curve.raised(step(j, fine))).collect();
        let mut j = 0;
        (0..LUT_SIZE)
            .map(|k| {
                let y = step(k, LUT_SIZE);
                while j + 2 < fine && ys.get(j + 1).is_some_and(|&n| n < y) {
                    j += 1;
                }
                let (y0, y1) = (ys.get(j).copied().unwrap_or(0.0), ys.get(j + 1).copied().unwrap_or(1.0));
                let f = if y1 > y0 { ((y - y0) / (y1 - y0)).clamp(0.0, 1.0) } else { 0.0 };
                (step(j, fine) + f / (fine - 1) as f32).clamp(0.0, 1.0)
            })
            .collect()
    };
    bright.into_iter().map(|b| modern_contrast(b, contrast).clamp(0.0, 1.0)).collect()
}

/// Modern Contrast curve (one channel, `contrast` in [-50, 100]). A symmetric cubic-Hermite S-curve
/// pivoting at 0.5: endpoint slope `1 - c/128`, centre slope `1 + c/128`. Near-exact vs Photoshop
/// (≤0.5/255). `c = 0` is the identity; pins 0, 0.5 and 1.
pub fn modern_contrast(v: f32, contrast: f32) -> f32 {
    if contrast == 0.0 {
        return v;
    }
    let k = contrast / 128.0;
    let (end, mid) = (1.0 - k, 1.0 + k);
    // Cubic Hermite on [0, 0.5]: pinned (0,0) slope `end`, (0.5,0.5) slope `mid`. `t` in [0,1].
    let half = |t: f32| end * 0.5 * (t * t * t - 2.0 * t * t + t) + (-2.0 * t * t * t + 3.0 * t * t) * 0.5 + mid * 0.5 * (t * t * t - t * t);
    let v = v.clamp(0.0, 1.0);
    if v <= 0.5 { half(v / 0.5) } else { 1.0 - half((1.0 - v) / 0.5) }
}

/// Photoshop posterize: `n` equal input bins over 0..=255, output levels
/// `floor(k * 255 / (n - 1))` (exact on the corpus for 3, 7, 13 and 21 levels).
pub fn posterize(v: f32, levels: u32) -> f32 {
    let n = levels.clamp(2, 255) as f32;
    let x = (v.clamp(0.0, 1.0) * 255.0).round();
    let bin = (x * n / 256.0).floor().min(n - 1.0);
    (bin * 255.0 / (n - 1.0)).floor() / 255.0
}

/// [`posterize`] in a document of `depth` (`None`: unknown, treated as 8-bit). At 16 bits
/// Photoshop bins the 16-bit sample (`⌊v · n / 32768⌋`) and lands on 8-bit levels through a
/// 16-bit step: `⌊b · 32768 / (n - 1)⌋`, converted with `⌊(o · 255 + 16384) / 32768⌋`, so
/// 127.5 comes out as 128 but 42.5 as 42 (8 bits floor them all). Exact on every sample of
/// psd-tools posterize_16bits_rgb.psd and posterize_16bits_grayscale.psd (levels 2 to 21),
/// which the 8-bit rule missed by up to half the range.
pub fn posterize_in(v: f32, levels: u32, depth: Option<SampleType>) -> f32 {
    if depth != Some(SampleType::U16) {
        return posterize(v, levels);
    }
    let n = f64::from(levels.clamp(2, 255));
    let x = (f64::from(v.clamp(0.0, 1.0)) * 32768.0).round();
    let bin = (x * n / 32768.0).floor().min(n - 1.0);
    let step = (bin * 32768.0 / (n - 1.0)).floor();
    (((step * 255.0 + 16384.0) / 32768.0).floor() / 255.0) as f32
}

pub fn levels(ch: &LevelsChannel, v: f32) -> f32 {
    levels_q(ch, v, None)
}

/// [`levels`] on samples with `quantum` steps per unit (8-bit: 255). Photoshop works on whole
/// levels in three steps, each rounded half up to a whole level: the input-range stretch, the
/// midtone gamma ([`levels_gamma`]) and the output range. So a 44..214 range maps 45, 46, 47, 48
/// → 1.5, 3, 4.5, 6 → 2, 3, 5, 6 (psd-tools levels_grayscale, where the gamma toe magnifies the
/// steps), and an output range narrower than 0..255 maps neighbouring gamma levels to one level
/// (psd-tools levels_rgb: 43..48 → 58 through a 0.47 channel gamma and the master's 48..255).
/// In a LUT the result is a staircase that is exact at every level.
pub fn levels_q(ch: &LevelsChannel, v: f32, quantum: Option<f32>) -> f32 {
    let range = (ch.in_white - ch.in_black).max(1e-6);
    let quantum = quantum.filter(|q| q.is_finite() && *q >= 1.0);
    let whole = |x: f32| match quantum {
        Some(q) => (x * q + 0.5).floor() / q,
        None => x,
    };
    let u = match quantum {
        Some(q) => {
            let v = (v * q).round() / q;
            (((v - ch.in_black) / range).clamp(0.0, 1.0) * q + 0.5 + 1e-3).floor() / q
        }
        None => ((v - ch.in_black) / range).clamp(0.0, 1.0),
    };
    let t = whole(levels_gamma(u, ch.gamma.max(0.01)));
    whole(ch.out_black + t * (ch.out_white - ch.out_black))
}

/// Exponent of the shadow toe in [`levels_gamma`] (fitted; see there).
const LEVELS_TOE_POWER: f32 = 2.2;

/// Levels' midtone gamma `g` on `u` in 0..1: `u^(1/g)`, except that above 1, where that power
/// curve is infinitely steep at black, Photoshop starts along a toe `a·u − b·u^q` of slope
/// `a = 2^g` that joins the curve with the same value and slope. `q` = [`LEVELS_TOE_POWER`] is
/// fitted on whole-level oracles at gamma 1.16, 1.22, 1.3, 1.78 and 2.0 (psd-tools levels_rgb
/// and levels_grayscale, the photoshop corpus's rgb8 and gray8 levels.psd, a measured 0..255
/// ramp): all but 2 of their 1118 levels exact, the others within one. Below 1 the power curve is
/// exact (gamma 0.67 and 0.7: every level).
pub fn levels_gamma(u: f32, g: f32) -> f32 {
    let c = 1.0 / g;
    if g <= 1.0 {
        return u.powf(c);
    }
    let (a, q) = (2f32.powf(g), LEVELS_TOE_POWER);
    // Where the toe meets the curve: a·u1 − b·u1^q = u1^c and a − q·b·u1^(q−1) = c·u1^(c−1).
    let u1 = ((1.0 - c / q) / (a * (1.0 - 1.0 / q))).powf(1.0 / (1.0 - c));
    if u >= u1 || u1 <= 0.0 {
        return u.powf(c);
    }
    let b = (a * u1 - u1.powf(c)) / u1.powf(q);
    // Near gamma 1 the toe vanishes: its denominator underflows before the join point does.
    if !b.is_finite() {
        return u.powf(c);
    }
    a * u - b * u.powf(q)
}

/// [`levels`] in a 32-bit document. Neither the input is clipped to the input range nor the result
/// to the output range there, and the midtone gamma is a plain power curve, mirrored below the
/// black point (the oracle corpus' rgb32 and gray32 levels.psd: within 0.5/255, 10/255 off with
/// the clipped curve).
pub fn levels_float(ch: &LevelsChannel, v: f32) -> f32 {
    let t = (v - ch.in_black) / (ch.in_white - ch.in_black).max(1e-6);
    let t = if ch.gamma == 1.0 { t } else { t.signum() * t.abs().powf(1.0 / ch.gamma.max(0.01)) };
    ch.out_black + t * (ch.out_white - ch.out_black)
}

/// Photoshop curve: a natural cubic spline through the points (it may
/// overshoot between points, as Photoshop's does), constant outside the
/// first/last point, clamped to 0..=1, sampled into a LUT.
pub fn curve_lut(points: &[CurvePoint]) -> Vec<f32> {
    let mut pts: Vec<(f32, f32)> = points.iter().map(|p| (p.input, p.output)).collect();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
    if pts.len() < 2 {
        return (0..LUT_SIZE).map(|i| i as f32 / (LUT_SIZE - 1) as f32).collect();
    }
    let n = pts.len();
    // Second derivatives of the natural spline (tridiagonal solve).
    let mut m2 = vec![0.0f64; n];
    if n > 2 {
        let x: Vec<f64> = pts.iter().map(|p| f64::from(p.0)).collect();
        let y: Vec<f64> = pts.iter().map(|p| f64::from(p.1)).collect();
        let mut c = vec![0.0f64; n];
        let mut d = vec![0.0f64; n];
        for i in 1..n - 1 {
            let (h0, h1) = (x[i] - x[i - 1], x[i + 1] - x[i]);
            let a = h0 / 6.0;
            let b = (h0 + h1) / 3.0;
            let cc = h1 / 6.0;
            let r = (y[i + 1] - y[i]) / h1 - (y[i] - y[i - 1]) / h0;
            let denom = b - a * c[i - 1];
            c[i] = cc / denom;
            d[i] = (r - a * d[i - 1]) / denom;
        }
        for i in (1..n - 1).rev() {
            m2[i] = d[i] - c[i] * m2[i + 1];
        }
    }
    (0..LUT_SIZE)
        .map(|k| {
            let xv = k as f32 / (LUT_SIZE - 1) as f32;
            if xv <= pts[0].0 {
                return pts[0].1.clamp(0.0, 1.0);
            }
            if xv >= pts[n - 1].0 {
                return pts[n - 1].1.clamp(0.0, 1.0);
            }
            let i = pts.windows(2).position(|w| xv <= w[1].0).unwrap_or(n - 2);
            let (x0, y0) = (f64::from(pts[i].0), f64::from(pts[i].1));
            let (x1, y1) = (f64::from(pts[i + 1].0), f64::from(pts[i + 1].1));
            let h = x1 - x0;
            let t = f64::from(xv);
            let a = (x1 - t) / h;
            let b = (t - x0) / h;
            let y = a * y0 + b * y1 + ((a * a * a - a) * m2[i] + (b * b * b - b) * m2[i + 1]) * h * h / 6.0;
            (y as f32).clamp(0.0, 1.0)
        })
        .collect()
}

fn gradient(stops: &[(f32, [f32; 3])], t: f32) -> [f32; 3] {
    match stops {
        [] => [t; 3],
        [s] => s.1,
        _ => {
            if t <= stops[0].0 {
                return stops[0].1;
            }
            for w in stops.windows(2) {
                if t <= w[1].0 {
                    let k = if w[1].0 > w[0].0 { (t - w[0].0) / (w[1].0 - w[0].0) } else { 0.0 };
                    return std::array::from_fn(|i| w[0].1[i] + (w[1].1[i] - w[0].1[i]) * k);
                }
            }
            stops[stops.len() - 1].1
        }
    }
}

pub fn rgb_to_hsl(c: [f32; 3]) -> (f32, f32, f32) {
    let [h, s, l] = li_color::perceptual::rgb_to_hsl_turns(c);
    (h, s, l)
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s <= 0.0 {
        return [l; 3];
    }
    li_color::perceptual::hsl_turns_to_rgb(h, s, l)
}

#[cfg(test)]
mod lookup_tests {
    use super::*;

    fn identity(n: usize) -> Vec<f32> {
        let m = (n - 1) as f32;
        (0..n * n * n).flat_map(|i| [(i % n) as f32 / m, ((i / n) % n) as f32 / m, (i / (n * n)) as f32 / m]).collect()
    }

    #[test]
    fn identity_lut_is_identity_both_interpolations() {
        let t = identity(5);
        for c in [[0.1, 0.5, 0.9], [0.33, 0.77, 0.0], [1.0, 1.0, 1.0]] {
            for tet in [false, true] {
                let o = lut3d_sample(&t, 5, c, tet);
                for k in 0..3 {
                    assert!((o[k] - c[k]).abs() < 1e-5, "{c:?} {tet} {o:?}");
                }
            }
        }
    }

    #[test]
    fn selective_color_zero_is_identity_and_ranges_are_local() {
        let zero = [[0.0f32; 4]; 9];
        for c in [[0.9, 0.1, 0.1], [0.5, 0.5, 0.5], [0.2, 0.4, 0.8]] {
            assert_eq!(selective_color(c, true, &zero), c);
        }
        // +100 % cyan on reds only: red loses red, blue is untouched.
        let mut a = zero;
        a[0][0] = 100.0;
        let red = selective_color([0.9, 0.1, 0.1], false, &a);
        assert!(red[0] < 0.2, "{red:?}");
        assert_eq!(selective_color([0.1, 0.1, 0.9], false, &a), [0.1, 0.1, 0.9]);
        // Relative mode can't tint pure white; absolute can (via whites).
        let mut w = zero;
        w[6] = [0.0, 0.0, 50.0, 0.0];
        assert_eq!(selective_color([1.0; 3], true, &w), [1.0; 3]);
        assert!(selective_color([1.0; 3], false, &w)[2] < 0.6);
    }

    #[test]
    fn dither_offsets_are_balanced() {
        let sum: f32 = (0..4).flat_map(|y| (0..4).map(move |x| bayer4(x, y))).sum();
        assert!(sum.abs() < 1e-5);
        assert_eq!(bayer4(-4, -4), bayer4(0, 0));
    }
}

#[cfg(test)]
mod tone_tests {
    use super::*;
    use photocraft_geom::Rect;

    fn ramp() -> Buffer {
        let rect = Rect::new(0, 0, 8, 8);
        let px = (0..64).map(|i| [((i % 8) as f32) / 7.0, ((i / 8) as f32) / 7.0, 0.3, 1.0]).collect();
        Buffer { rect, px }
    }

    #[test]
    fn color_lookup_with_an_overflowing_stored_size_is_the_identity() {
        // Upstream 5049a1d0: a stored document's size is untrusted: (2²²)³ × 3 overflows, which once
        // wrapped past the table-length guard and indexed out of bounds.
        for size in [1 << 22, u32::MAX] {
            let a = Adjustment::ColorLookup {
                domain: None,
                name: "x".into(),
                lut: Some(std::sync::Arc::new(vec![0.5; 24])),
                size,
                tetrahedral: false,
                dither: false,
            };
            let mut b = ramp();
            apply(&a, &mut b);
            assert_eq!(b.px, ramp().px, "size {size}");
        }
    }

    #[test]
    fn lut3d_len_rejects_tiny_and_overflowing_sizes() {
        assert_eq!(lut3d_len(0), None);
        assert_eq!(lut3d_len(1), None);
        assert_eq!(lut3d_len(2), Some(24));
        assert_eq!(lut3d_len(33), Some(33 * 33 * 33 * 3));
        assert_eq!(lut3d_len(1 << 22), None, "(2^22)^3 overflows a 64-bit length");
        assert_eq!(lut3d_len(u32::MAX), None);
    }

    #[test]
    fn identity_tone_in_every_space_is_identity() {
        for space in [ToneSpace::Rgb, ToneSpace::Cmyk, ToneSpace::Lab] {
            let mut a = Adjustment::identity_curves();
            if let Adjustment::Curves { space: s, .. } = &mut a {
                *s = space;
            }
            let mut b = ramp();
            apply(&a, &mut b);
            for (x, y) in b.px.iter().zip(ramp().px.iter()) {
                for k in 0..3 {
                    assert!((x[k] - y[k]).abs() < 0.02, "{space:?} {x:?} {y:?}");
                }
            }
        }
    }

    #[test]
    fn cmyk_black_curve_darkens_and_lab_lightness_lifts() {
        let line = |a: f32, b: f32| vec![CurvePoint { input: 0.0, output: a }, CurvePoint { input: 1.0, output: b }];
        // Black brightness 1 -> 0.5: more black ink everywhere.
        let a = Adjustment::Curves {
            master: line(0.0, 1.0),
            per_channel: [line(0.0, 1.0), line(0.0, 1.0), line(0.0, 1.0)],
            space: ToneSpace::Cmyk,
            black: line(0.0, 0.5),
        };
        let mut b = Buffer { rect: Rect::new(0, 0, 1, 1), px: vec![[0.8, 0.8, 0.8, 1.0]] };
        apply(&a, &mut b);
        assert!(b.px[0][0] < 0.7, "{:?}", b.px[0]);
        // Lab lightness 0..1 -> 0.2..1 lifts a dark grey; a/b untouched keeps it grey.
        let a = Adjustment::Curves {
            master: line(1.0, 0.0), // ignored in Lab
            per_channel: [line(0.2, 1.0), line(0.0, 1.0), line(0.0, 1.0)],
            space: ToneSpace::Lab,
            black: Vec::new(),
        };
        let mut b = Buffer { rect: Rect::new(0, 0, 1, 1), px: vec![[0.2, 0.2, 0.2, 1.0]] };
        apply(&a, &mut b);
        let p = b.px[0];
        assert!(p[0] > 0.3 && (p[0] - p[2]).abs() < 0.02, "{p:?}");
    }

    #[test]
    fn hue_ranges_only_touch_their_colours() {
        let mut ranges = HueRange::defaults();
        ranges[0].saturation = -100.0; // desaturate reds
        let a = Adjustment::HueSaturation { hue: 0.0, saturation: 0.0, lightness: 0.0, colorize: false, ranges };
        let mut b = Buffer { rect: Rect::new(0, 0, 3, 1), px: vec![[0.9, 0.1, 0.1, 1.0], [0.1, 0.1, 0.9, 1.0], [0.5, 0.5, 0.5, 1.0]] };
        apply(&a, &mut b);
        let red = b.px[0];
        assert!((red[0] - red[1]).abs() < 0.05, "red desaturated {red:?}");
        assert!((b.px[1][2] - 0.9).abs() < 1e-4 && (b.px[1][0] - 0.1).abs() < 1e-4, "blue untouched {:?}", b.px[1]);
        assert_eq!(b.px[2], [0.5, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn black_white_weights() {
        let d = [40.0, 60.0, 40.0, 60.0, 20.0, 80.0];
        assert!((black_white_gray([0.5; 3], &d) - 0.5).abs() < 1e-6, "greys keep their value");
        assert!((black_white_gray([1.0, 0.0, 0.0], &d) - 0.4).abs() < 1e-6);
        assert!((black_white_gray([1.0, 1.0, 0.0], &d) - 0.6).abs() < 1e-6);
        let mut w = d;
        w[0] = 100.0;
        assert!(black_white_gray([1.0, 0.0, 0.0], &w) > black_white_gray([1.0, 0.0, 0.0], &d));
    }

    #[test]
    fn gradient_map_dither_stays_close() {
        let stops = vec![(0.0, [0.0; 3]), (1.0, [1.0; 3])];
        let mut a = ramp();
        let mut b = ramp();
        apply(&Adjustment::GradientMap { stops: stops.clone(), reverse: false, dither: false }, &mut a);
        apply(&Adjustment::GradientMap { stops, reverse: false, dither: true }, &mut b);
        assert!(a.px.iter().zip(&b.px).all(|(x, y)| (x[0] - y[0]).abs() <= 0.5 / 255.0 + 1e-6));
        assert!(a.px.iter().zip(&b.px).any(|(x, y)| x[0] != y[0]));
    }

    #[test]
    fn gradient_map_follows_the_w3c_luminosity() {
        // Photoshop's Gradient Map is driven by 0.3 R + 0.59 G + 0.11 B (photoshop corpus rgb8 and
        // rgb16 gradient-map.psd), not the Rec. 601 grey.
        let stops = vec![(0.0, [0.0; 3]), (1.0, [1.0; 3])];
        for (c, t) in [([0.0, 1.0, 0.0], 0.59), ([1.0, 0.0, 0.0], 0.3), ([0.0, 0.0, 1.0], 0.11), ([0.2, 0.6, 0.9], 0.513)] {
            let mut b = Buffer::filled(crate::Rect::new(0, 0, 1, 1), [c[0], c[1], c[2], 1.0]);
            apply(&Adjustment::GradientMap { stops: stops.clone(), reverse: false, dither: false }, &mut b);
            assert!((b.px[0][0] - t).abs() < 1e-5, "{c:?}: {}", b.px[0][0]);
        }
    }

    /// Pixel → Photoshop's split. 8-bit from psd-tools threshold_rgb.psd: the first three are
    /// white though their Rec. 601 grey rounds below 60, and (131, 109, 99) sits exactly on 114.5
    /// at level 115 (halves round down). 16-bit from threshold_16bits_rgb.psd and the photoshop
    /// corpus's rgb16 threshold-128.psd, whose mid grey 16448 is level 128 in 16 bits.
    #[test]
    fn threshold_splits_on_the_luminosity_like_photoshop() {
        let c8 = |r: u8, g: u8, b: u8| [r, g, b].map(|v| f32::from(v) / 255.0);
        for (c, level, white) in
            [(c8(87, 51, 31), 60u8, true), (c8(90, 49, 33), 60, true), (c8(68, 57, 50), 60, true), (c8(48, 63, 72), 60, false), (c8(131, 109, 99), 115, false)]
        {
            assert_eq!(threshold_white(c, f32::from(level) / 255.0, Some(SampleType::U8)), white, "8-bit {c:?} at {level}");
        }
        let c16 = |r: u16, g: u16, b: u16| [r, g, b].map(|v| f32::from(v) / 32768.0);
        for (c, level, white) in [
            (c16(8995, 7196, 6425), 60u8, false),
            (c16(11180, 6554, 3984), 60, false),
            (c16(16448, 16448, 16448), 128, true),
            (c16(16447, 16447, 16447), 128, false),
        ] {
            assert_eq!(threshold_white(c, f32::from(level) / 255.0, Some(SampleType::U16)), white, "16-bit {c:?} at {level}");
        }
        // Through the adjustment: neutral black or white.
        let mut b = Buffer::filled(Rect::new(0, 0, 1, 1), [87.0 / 255.0, 51.0 / 255.0, 31.0 / 255.0, 1.0]);
        apply_depth(&Adjustment::Threshold { level: 60.0 / 255.0 }, &mut b, Transfer::Srgb, Some(SampleType::U8));
        assert_eq!(b.px[0], [1.0, 1.0, 1.0, 1.0]);
    }

    /// 16-bit sample → Photoshop's 8-bit level, from psd-tools posterize_16bits_rgb.psd: bins
    /// split on the 16-bit sample (10794 and 10923 straddle a third of 32768), and the middle of 3
    /// levels is 128 where 8 bits give 127, while 21 levels keep 25 for 25.5.
    #[test]
    fn sixteen_bit_posterize_matches_photoshop() {
        let points: [(u32, u16, u8); 9] =
            [(3, 10794, 0), (3, 10923, 128), (3, 21845, 128), (3, 21974, 255), (13, 2504, 0), (13, 2542, 21), (13, 7453, 42), (13, 7582, 64), (21, 3341, 25)];
        for (levels, x, want) in points {
            let got = posterize_in(f32::from(x) / 32768.0, levels, Some(SampleType::U16));
            assert!((got * 255.0 - f32::from(want)).abs() < 1e-3, "{levels} levels at {x}: {} vs Photoshop {want}", got * 255.0);
        }
        // 8 bits keep their rule.
        assert_eq!(posterize_in(128.0 / 255.0, 3, Some(SampleType::U8)), posterize(128.0 / 255.0, 3));
    }

    /// Photoshop, measured (8-bit gray ramp, every channel): whole settings, where the tones fold
    /// into one Levels per channel. Midtones red +100 / magenta −100 (the shadow toe), a layer with
    /// Preserve Luminosity off and on, all nine sliders with Preserve Luminosity on, a black point
    /// past the white point, and lowered shadows with raised highlights (their gammas cancel: a
    /// straight line between the two points).
    #[test]
    fn color_balance_whole_settings_match_photoshop() {
        const X: [u8; 12] = [0, 4, 8, 16, 32, 64, 96, 128, 160, 192, 224, 255];
        type Case = ([[f32; 3]; 3], bool, [[u8; 12]; 3]);
        let cases: [Case; 6] = [
            (
                [[0.0; 3], [100.0, -100.0, 0.0], [0.0; 3]],
                false,
                [[0, 16, 30, 55, 90, 128, 156, 181, 202, 221, 239, 255], [0, 0, 0, 1, 4, 16, 36, 64, 100, 145, 197, 255], X],
            ),
            (
                [[-6.0, 22.0, 5.0], [-1.0, -3.0, -8.0], [-1.0, -4.0, -6.0]],
                false,
                [
                    [0, 0, 2, 9, 25, 56, 89, 122, 156, 188, 222, 255],
                    [0, 5, 9, 18, 35, 67, 100, 131, 163, 194, 225, 255],
                    [0, 3, 7, 14, 29, 60, 91, 123, 156, 189, 222, 255],
                ],
            ),
            (
                [[-6.0, 22.0, 5.0], [-1.0, -3.0, -8.0], [-1.0, -4.0, -6.0]],
                true,
                [
                    [0, 0, 0, 0, 5, 42, 80, 117, 154, 189, 226, 255],
                    [0, 4, 8, 16, 33, 66, 98, 130, 162, 195, 226, 255],
                    [0, 0, 0, 0, 15, 48, 82, 116, 151, 186, 221, 255],
                ],
            ),
            (
                [[40.0, -30.0, 20.0], [-50.0, 25.0, 60.0], [30.0, -70.0, 10.0]],
                true,
                [
                    [0, 1, 3, 9, 25, 69, 126, 193, 255, 255, 255, 255],
                    [0, 0, 0, 0, 0, 0, 46, 93, 136, 177, 217, 255],
                    [0, 0, 0, 0, 44, 107, 156, 199, 238, 255, 255, 255],
                ],
            ),
            (
                [[100.0, -100.0, 0.0], [0.0; 3], [-100.0, 100.0, 0.0]],
                true,
                [X, [0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255], [0, 0, 0, 0, 0, 0, 0, 130, 255, 255, 255, 255]],
            ),
            (
                [[-76.0, -2.0, 60.0], [0.0; 3], [99.0, 0.0, -30.0]],
                false,
                [
                    [0, 0, 0, 0, 0, 0, 71, 171, 255, 255, 255, 255],
                    [0, 2, 6, 14, 29, 61, 94, 126, 158, 191, 224, 255],
                    [0, 6, 11, 21, 39, 73, 106, 137, 168, 197, 227, 255],
                ],
            ),
        ];
        for (tones, preserve, want) in cases {
            for (ch, row) in want.iter().enumerate() {
                for (x, w) in X.iter().zip(row) {
                    let got = (color_balance_channel(tones, preserve, ch, f32::from(*x) / 255.0) * 255.0).round() as i32;
                    assert!((got - i32::from(*w)).abs() <= 1, "{tones:?} pl {preserve} ch {ch} at {x}: {got} vs Photoshop {w}");
                }
            }
        }
        // Hostile input stays in range.
        for v in [f32::NAN, -1.0, 2.0, f32::INFINITY] {
            let out = color_balance_channel([[f32::NAN, 1e9, -1e9]; 3], true, 7, v);
            assert!((0.0..=1.0).contains(&out), "{v}: {out}");
        }
    }

    #[test]
    fn color_balance_midtones_with_multiple_sliders_preserve_luminosity() {
        // A shared offset depends on the two extremes, not the sum of moved sliders.
        let tones = [[0.0; 3], [30.0, 0.0, 30.0], [0.0; 3]];
        for input in [0.25_f32, 0.5, 0.75] {
            for (ch, offset) in [-0.15_f32, 0.15, -0.15].into_iter().enumerate() {
                let expected = input.powf(2f32.powf(offset));
                let actual = color_balance_channel(tones, true, ch, input);
                assert!((actual - expected).abs() < 1e-6, "channel {ch}, input {input}: {actual} vs {expected}");
            }
            // Preserve Luminosity off must still affect only the moved channels.
            assert_eq!(color_balance_channel(tones, false, 1, input), input);
            // One moved slider uses half-strength compensation.
            let single = [[0.0; 3], [30.0, 0.0, 0.0], [0.0; 3]];
            assert!((color_balance_channel(single, true, 0, input) - input.powf(2f32.powf(-0.15))).abs() < 1e-6);
            assert!((color_balance_channel(single, true, 1, input) - input.powf(2f32.powf(0.15))).abs() < 1e-6);
        }
    }

    #[test]
    fn levels_and_curves_apply_each_channel_before_the_master() {
        // These two affine tone adjustments describe the same mapping:
        // channel(x) = 1 / 2 + x / 2, then master(x) = x / 2, so 0 maps to 0.25 (master first: 0.5).
        let line = |a: f32, b: f32| vec![CurvePoint { input: 0.0, output: a }, CurvePoint { input: 1.0, output: b }];
        let levels = Adjustment::Levels {
            master: LevelsChannel { out_white: 0.5, ..Default::default() },
            per_channel: std::array::from_fn(|_| LevelsChannel { out_black: 0.5, ..Default::default() }),
            space: ToneSpace::Rgb,
            black: Default::default(),
        };
        let curves =
            Adjustment::Curves { master: line(0.0, 0.5), per_channel: std::array::from_fn(|_| line(0.5, 1.0)), space: ToneSpace::Rgb, black: Vec::new() };
        for depth in [None, Some(SampleType::U8), Some(SampleType::U16), Some(SampleType::F32)] {
            let ll = tone_luts_depth(&levels, depth);
            let cl = tone_luts_depth(&curves, depth);
            for channel in 0..3 {
                for index in [0, LUT_SIZE / 4, LUT_SIZE / 2, 3 * LUT_SIZE / 4, LUT_SIZE - 1] {
                    assert!(
                        (ll[channel][index] - cl[channel][index]).abs() < 0.01,
                        "{depth:?} channel {channel} sample {index}: Levels {} vs Curves {}",
                        ll[channel][index],
                        cl[channel][index]
                    );
                }
            }
            assert!((cl[0][0] - 0.25).abs() < 0.01, "{depth:?}: {}", cl[0][0]);
            for adj in [&levels, &curves] {
                let mut pixel = Buffer { rect: Rect::new(0, 0, 1, 1), px: vec![[0.0, 0.0, 0.0, 1.0]] };
                apply_depth(adj, &mut pixel, Transfer::Srgb, depth);
                for channel in 0..3 {
                    assert!((pixel.px[0][channel] - 0.25).abs() < 0.01, "{depth:?}: {adj:?} rendered {:?}", pixel.px[0]);
                }
            }
        }
    }

    /// Photoshop 27.11, measured: a range's Lightness (Blues ±50) moves towards the brightest or
    /// darkest channel.
    #[test]
    fn range_lightness_matches_photoshop() {
        let measured = [
            ([0u8, 29, 115], 0.5, [57u8, 71, 115]),
            ([0, 64, 255], 0.5, [125, 157, 255]),
            ([102, 128, 204], 0.5, [152, 165, 204]),
            ([102, 128, 204], -0.5, [102, 115, 154]),
            ([0, 18, 115], -0.5, [0, 9, 58]),
        ];
        for (input, l, want) in measured {
            let got = range_lightness(input.map(|v| v as f32 / 255.0), l);
            for i in 0..3 {
                assert!((got[i] * 255.0 - want[i] as f32).abs() <= 2.5, "{input:?} channel {i}: {} vs {}", got[i] * 255.0, want[i]);
            }
        }
    }

    /// Photoshop's own render (merged image of psd-tools `adjustments/huesaturation_lightness_rgb.psd`,
    /// layer "Hue/Saturation 1"): a dull magenta gets the whole range edit, as a vivid one does.
    #[test]
    fn hue_ranges_apply_in_full_to_dull_colours() {
        let range = |lightness: f32, bounds: [f32; 4]| HueRange { hue: 0.0, saturation: 0.0, lightness, bounds };
        let ranges = [
            range(-11.0, [-57.0, 4.0, 82.0, 175.0]),
            range(100.0, [12.0, 45.0, 75.0, 105.0]),
            HueRange::neutral(2),
            HueRange::neutral(3),
            HueRange::neutral(4),
            range(100.0, [-117.0, -50.0, 39.0, 104.0]),
        ];
        let a = Adjustment::HueSaturation { hue: 0.0, saturation: 0.0, lightness: 17.0, colorize: false, ranges };
        let measured = [([126u8, 97, 120], [148u8, 148, 148]), ([131, 103, 125], [152, 152, 152]), ([95, 69, 90], [122, 122, 122])];
        for (input, want) in measured {
            let c = input.map(|v| v as f32 / 255.0);
            let mut b = Buffer { rect: Rect::new(0, 0, 1, 1), px: vec![[c[0], c[1], c[2], 1.0]] };
            apply(&a, &mut b);
            for (i, w) in want.iter().enumerate() {
                let got = b.px[0][i] * 255.0;
                assert!((got - *w as f32).abs() <= 2.0, "{input:?} channel {i}: {got} vs {w}");
            }
        }
        // Greys have no hue, so the ranges leave them alone: only the master Lightness moves them.
        let mut b = Buffer { rect: Rect::new(0, 0, 1, 1), px: vec![[0.4, 0.4, 0.4, 1.0]] };
        apply(&a, &mut b);
        let p = b.px[0];
        assert!((p[0] - p[1]).abs() < 1e-6 && (p[1] - p[2]).abs() < 1e-6 && (p[0] - (0.4 + 0.6 * 0.17)).abs() < 1e-4, "{p:?}");
    }

    fn hdr(adj: &Adjustment, c: [f32; 3]) -> [f32; 3] {
        let mut b = Buffer { rect: Rect::new(0, 0, 1, 1), px: vec![[c[0], c[1], c[2], 1.0]] };
        apply_depth(adj, &mut b, Transfer::Srgb, Some(SampleType::F32));
        [b.px[0][0], b.px[0][1], b.px[0][2]]
    }

    #[test]
    fn thirty_two_bit_saturation_steps_match_photoshop() {
        for (sat, want) in [
            (10.0, 539.0 / 512.0),
            (75.0, 2.4609375),
            (90.0, 2673.0 / 512.0),
            (99.0, 32.375),
            (100.0, 33023.0 / 512.0),
            (-10.0, 487.0 / 512.0),
            (-25.0, 0.875),
            (-50.0, 0.75),
            (-100.0, 0.5),
        ] {
            let adj = Adjustment::HueSaturation { hue: 0.0, saturation: sat, lightness: 0.0, colorize: false, ranges: HueRange::defaults() };
            let got = hdr(&adj, [1.0, 0.0, 0.0])[0];
            assert!((got - want).abs() < 1e-4, "saturation {sat}: {got}, Photoshop {want}");
        }
    }

    #[test]
    fn thirty_two_bit_hue_saturation_keeps_bright_colours() {
        let pale = [280.0 / 255.0, 230.0 / 255.0, 256.0 / 255.0];
        let near = |got: [f32; 3], want: [f32; 3]| got.iter().zip(want).all(|(g, w)| (g - w).abs() <= 1e-3 * w.abs().max(1.0));
        for (hue, sat, c, want) in [
            (40.0, 0.0, [2.0, 0.0, 0.0], [2.0, 1.33301, 0.0]),
            (40.0, 0.0, pale, [1.09804, 0.930687, 0.901961]),
            (0.0, 25.0, [2.0, 0.0, 0.0], [2.32812, -0.328125, -0.328125]),
            (0.0, 25.0, pale, [1.13021, 0.869792, 1.00521]),
            (0.0, -25.0, [2.0, 0.0, 0.0], [1.75, 0.25, 0.25]),
            (0.0, 100.0, [140.0 / 255.0, 115.0 / 255.0, 128.0 / 255.0], [6.77432, -5.77432, 0.750976]),
            (0.0, 100.0, [2.0, 0.0, 0.0], [128.996, -126.996, -126.996]),
        ] {
            let adj = Adjustment::HueSaturation { hue, saturation: sat, lightness: 0.0, colorize: false, ranges: HueRange::defaults() };
            let got = hdr(&adj, c);
            assert!(near(got, want), "hue {hue}, saturation {sat}, {c:?}: {got:?}, Photoshop {want:?}");
        }
        // A master plus a range edit that add up to a whole step stay on it: Reds +96 over a master
        // -46 scales red as much as a master +50 alone (in f32 the sum is 0.49999997).
        let mut ranges = HueRange::defaults();
        ranges[0].saturation = 96.0;
        let summed = Adjustment::HueSaturation { hue: 0.0, saturation: -46.0, lightness: 0.0, colorize: false, ranges };
        let single = Adjustment::HueSaturation { hue: 0.0, saturation: 50.0, lightness: 0.0, colorize: false, ranges: HueRange::defaults() };
        assert_eq!(hdr(&summed, [1.0, 0.0, 0.0]), hdr(&single, [1.0, 0.0, 0.0]));
        // Lightness 0 with a negative channel divides by zero the same way.
        let adj = Adjustment::HueSaturation { hue: 40.0, saturation: 25.0, lightness: 0.0, colorize: false, ranges: HueRange::defaults() };
        assert!(hdr(&adj, [0.25, -0.25, 0.0]).iter().all(|v| v.is_finite()));
    }
}

#[cfg(test)]
mod lut3d_guard_tests {
    use super::lut3d_sample;

    #[test]
    fn degenerate_or_short_tables_leave_the_colour_alone() {
        // Upstream 2842973d.
        let c = [0.2, 0.4, 0.6];
        for tetra in [false, true] {
            assert_eq!(lut3d_sample(&[], 0, c, tetra), c);
            assert_eq!(lut3d_sample(&[0.0; 3], 1, c, tetra), c);
            assert_eq!(lut3d_sample(&[0.0; 3], 2, c, tetra), c);
            assert_eq!(lut3d_sample(&[0.0; 24], usize::MAX, c, tetra), c);
        }
    }
}
