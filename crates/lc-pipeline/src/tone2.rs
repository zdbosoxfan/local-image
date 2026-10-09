//! global tone: the looks and the hue-preserving tone stage.
//!
//! Every look is a base curve from scene luminance to display-linear luminance, sampled into the
//! same log-spaced table as the tone map ([`ToneMap`]), so the CPU and the GPU evaluate it
//! identically:
//!
//! * **Adobe-like** ([`adobe`]): our own parametric filmic curve, calibrated to a Lightroom-like
//!   default rendition — middle grey brighter than the scene value, a soft toe, a long gentle
//!   shoulder with more than 5 EV of headroom above grey and no hard knee.
//! * **Sigmoid** ([`Sigmoid`]): darktable's generalized log-logistic curve (`src/iop/sigmoid.c`,
//!   GPL-3.0-or-later, see `docs/PORTS.md`), at its defaults (contrast 1.5, skew 0, display
//!   black 0.0152 %, white 100 %), with darktable's scene-referred default exposure of +0.7 EV.
//! * **Camera** ([`BaseCurve::Camera`]): a curve fitted to the file's embedded JPEG, or a maker's
//!   base curve (`the engine's base-curves module`) when the file has no usable preview.
//!
//! For scene-referred (raw) sources Contrast, Whites and Blacks reshape the scene's log exposure
//! before the base curve ([`scene_ev`]): contrast scales the slope about grey (for the sigmoid this
//! is exactly darktable's contrast, the film power), whites stretch or compress the range above
//! grey (less or more highlight headroom), blacks the range below. Rendered (display-referred)
//! sources keep their own tones (identity at neutral) and get the same sliders as gentle curves in
//! a perceptual domain ([`display_tone`]).
//!
//! Sigmoid and Soft Film use darktable sigmoid's per-channel method ([`tone_px`]): the curve on each
//! channel, then the middle channel corrected towards the input's hue with the channels' sum kept
//! (`_preserve_hue_and_energy`), blended by the photo's Hue Preservation. Bright saturated colours
//! then bleach smoothly towards white as their brightest channel reaches the shoulder — no hard
//! knee, nothing clipped — and 100 % keeps hues exactly (the DNG/Lightroom "hue-preserving RGB tone
//! curve" idea). Camera maps a luminance norm and scales chroma by the fitted curve; Hue
//! Preservation blends toward the per-channel result while holding output luminance. All looks
//! contain residual gamut excursions smoothly. darktable's ratio mode is [`compress`].

use crate::tone::{CHROMA_N, CameraTone, GREY, LUT_MAX_EV, LUT_MIN_EV, LUT_N, ToneMap};

/// darktable sigmoid's middle grey.
pub const SIGMOID_GREY: f32 = 0.1845;
/// darktable's scene-referred default exposure (+0.7 EV), which its sigmoid is designed around.
pub const SIGMOID_EXPOSURE: f32 = 0.7;

/// How the tone stage of a global render works (carried by its [`ToneMap`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneMethod {
    /// Hue preservation 0..1 (darktable sigmoid's "preserve hue"): 1 keeps every hue, 0 is the
    /// plain per-channel curve (bright colours shift towards the secondaries, like film).
    pub hue: f32,
    /// Camera fits map a luminance norm; sigmoid and Soft Film use per-channel mapping.
    pub luminance: bool,
}

/// darktable sigmoid's curve parameters (`dt_iop_sigmoid_data_t`), computed from its user
/// parameters as `commit_params` does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sigmoid {
    pub white_target: f32,
    pub black_target: f32,
    pub paper_exposure: f32,
    pub film_fog: f32,
    pub film_power: f32,
    pub paper_power: f32,
}

/// darktable's `_generalized_loglogistic_sigmoid`.
#[inline]
pub fn loglogistic(value: f32, magnitude: f32, paper_exp: f32, film_fog: f32, film_power: f32, paper_power: f32) -> f32 {
    let clamped = value.max(0.0);
    // the film + paper model rewritten in a form that is stable around zero
    let film_response = (film_fog + clamped).powf(film_power);
    let paper_response = magnitude * (film_response / (paper_exp + film_response)).powf(paper_power);
    if paper_response.is_nan() { magnitude } else { paper_response }
}

impl Sigmoid {
    /// darktable's defaults: contrast 1.5, skew 0, target white 100 %, target black 0.0152 %.
    pub fn default_curve() -> Sigmoid {
        Sigmoid::new(1.5, 0.0, 100.0, 0.0152)
    }

    /// darktable's `commit_params`: contrast (middle grey contrast), skew (−1..1), display white
    /// and black targets in percent.
    pub fn new(middle_grey_contrast: f32, contrast_skewness: f32, display_white_target: f32, display_black_target: f32) -> Sigmoid {
        const MIDDLE_GREY: f32 = SIGMOID_GREY;
        // a reference slope for no skew and a normalized display
        let ref_film_power = middle_grey_contrast;
        let ref_paper_power = 1.0f32;
        let ref_magnitude = 1.0f32;
        let ref_film_fog = 0.0f32;
        let ref_paper_exposure = (ref_film_fog + MIDDLE_GREY).powf(ref_film_power) * ((ref_magnitude / MIDDLE_GREY) - 1.0);
        let delta = 1e-6f32;
        let ref_slope = (loglogistic(MIDDLE_GREY + delta, ref_magnitude, ref_paper_exposure, ref_film_fog, ref_film_power, ref_paper_power)
            - loglogistic(MIDDLE_GREY - delta, ref_magnitude, ref_paper_exposure, ref_film_fog, ref_film_power, ref_paper_power))
            / 2.0
            / delta;
        // skew
        let paper_power = 5f32.powf(-contrast_skewness);
        // slope at low film power
        let temp_film_power = 1.0f32;
        let temp_white_target = 0.01 * display_white_target;
        let temp_white_grey_relation = (temp_white_target / MIDDLE_GREY).powf(1.0 / paper_power) - 1.0;
        let temp_paper_exposure = MIDDLE_GREY.powf(temp_film_power) * temp_white_grey_relation;
        let temp_slope = (loglogistic(MIDDLE_GREY + delta, temp_white_target, temp_paper_exposure, ref_film_fog, temp_film_power, paper_power)
            - loglogistic(MIDDLE_GREY - delta, temp_white_target, temp_paper_exposure, ref_film_fog, temp_film_power, paper_power))
            / 2.0
            / delta;
        // the film power that gives the reference slope
        let film_power = ref_slope / temp_slope;
        let white_target = 0.01 * display_white_target;
        let black_target = 0.01 * display_black_target;
        let white_grey_relation = (white_target / MIDDLE_GREY).powf(1.0 / paper_power) - 1.0;
        let white_black_relation = (black_target / white_target).powf(-1.0 / paper_power) - 1.0;
        let film_fog = MIDDLE_GREY * white_grey_relation.powf(1.0 / film_power)
            / (white_black_relation.powf(1.0 / film_power) - white_grey_relation.powf(1.0 / film_power));
        let paper_exposure = (film_fog + MIDDLE_GREY).powf(film_power) * white_grey_relation;
        Sigmoid { white_target, black_target, paper_exposure, film_fog, film_power, paper_power }
    }

    pub fn eval(&self, v: f32) -> f32 {
        loglogistic(v, self.white_target, self.paper_exposure, self.film_fog, self.film_power, self.paper_power)
    }
}

/// The Adobe-like base curve's parameters (our own parametric curve; nothing of Adobe's is used).
#[derive(Clone, Copy, Debug)]
struct AdobeLike {
    /// Display value of scene middle grey.
    grey_out: f64,
    /// log-log slope at grey (midtone contrast).
    slope: f64,
    /// log-log slope deep in the shadows (< `slope`: a soft toe that keeps shadow detail).
    toe_slope: f64,
    /// Width of the toe transition (EV).
    toe_width: f64,
    /// Shoulder sharpness (1 = Reinhard; higher = later, harder roll-off).
    shoulder: f64,
}

// Calibrated so a metered middle grey (about 3 EV below sensor clipping, ~0.09 here) lands near
// sRGB 118, scene grey 0.18 near 147, sensor white near 244, and each stop up to +5 EV above grey
// stays distinguishable.
const ADOBE: AdobeLike = AdobeLike { grey_out: 0.29, slope: 1.3, toe_slope: 1.0, toe_width: 2.5, shoulder: 1.6 };

impl AdobeLike {
    /// Log2 of the pre-shoulder value relative to grey at `x` EV from grey.
    fn shaped(&self, x: f64) -> f64 {
        if x >= 0.0 {
            self.slope * x
        } else {
            // slope `slope` at grey easing to `toe_slope` deep in the shadows
            let k = self.toe_width;
            self.toe_slope * x + (self.slope - self.toe_slope) * k * ((x / k).exp() - 1.0)
        }
    }

    fn shoulder_of(&self, t: f64) -> f64 {
        t / (1.0 + t.powf(self.shoulder)).powf(1.0 / self.shoulder)
    }

    /// The pre-shoulder grey value giving `grey_out` (bisection; the shoulder is monotone).
    fn grey_in(&self) -> f64 {
        let (mut lo, mut hi) = (1e-4f64, 10.0f64);
        for _ in 0..80 {
            let mid = (lo * hi).sqrt();
            if self.shoulder_of(mid) < self.grey_out { lo = mid } else { hi = mid }
        }
        (lo * hi).sqrt()
    }

    fn eval(&self, y: f64, g: f64) -> f64 {
        if y <= 0.0 {
            return 0.0;
        }
        let x = (y / GREY as f64).log2();
        self.shoulder_of(g * self.shaped(x).exp2())
    }
}

/// The Adobe-like base curve: scene luminance → display-linear luminance.
pub fn adobe(y: f32) -> f32 {
    static G: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    let g = *G.get_or_init(|| ADOBE.grey_in());
    ADOBE.eval(y as f64, g) as f32
}

/// A look's base curve.
// Built once per tone LUT; keeping the 32 camera knots inline avoids a heap allocation.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BaseCurve {
    Adobe,
    Sigmoid(Sigmoid),
    /// A camera curve: fitted to the embedded JPEG, a maker base curve, or a DNG profile's tone
    /// curve (scene → display luminance, with its chroma curve).
    Camera(CameraTone),
}

impl BaseCurve {
    pub fn eval(&self, y: f32) -> f32 {
        match self {
            BaseCurve::Adobe => adobe(y),
            BaseCurve::Sigmoid(s) => s.eval(y * SIGMOID_EXPOSURE.exp2()),
            BaseCurve::Camera(c) => c.apply(y),
        }
    }
}

/// `x`² / (`x` + `k`) for x ≥ 0: 0 with zero slope at 0, slope → 1 far out (the slider shapers).
#[inline]
fn ramp(x: f32, k: f32) -> f32 {
    if x <= 0.0 { 0.0 } else { x * x / (x + k) }
}

/// Contrast as a log slope about grey (the original mapping of the slider).
pub fn contrast_slope(contrast: f64) -> f32 {
    let c = (contrast / 100.0) as f32;
    if c >= 0.0 { 1.0 + 0.55 * c } else { 1.0 + 0.4 * c }
}

/// A base curve variant (Capture One-style film curves, under every look): how the look's curve is
/// reshaped before the sliders.
pub fn base_shape(base: lightcraft_develop::ToneBase) -> (f32, f64, f64) {
    use lightcraft_develop::ToneBase as B;
    // (slope factor, whites, blacks) in slider units
    match base {
        B::Standard => (1.0, 0.0, 0.0),
        // the shadows opened up (modern high dynamic range sensors)
        B::ExtraShadow => (1.0, 0.0, 45.0),
        B::HighContrast => (1.18, 10.0, -20.0),
        // Identity scene response; the output shoulder is applied in `ToneMap::scene`.
        B::Linear => (1.0, 0.0, 0.0),
    }
}

/// A scene-referred source's exposure `ev` (EV from grey) reshaped by Contrast, Whites and Blacks
/// (−100..100) before the base curve. Monotone, identity at 0/0/0, grey stays grey.
pub fn scene_ev(ev: f32, contrast: f64, whites: f64, blacks: f64) -> f32 {
    let mut e = ev * contrast_slope(contrast);
    let w = (whites / 100.0) as f32;
    let b = (blacks / 100.0) as f32;
    if w != 0.0 && e > 0.0 {
        // + stretches the range above grey (whites reach white sooner), − compresses it (more
        // highlight headroom); slope ≥ 0.55
        e += w * 0.45 * ramp(e, 1.5);
    }
    if b != 0.0 && e < 0.0 {
        // + compresses the range below grey (shadows lift), − stretches it (deeper blacks)
        e += b * 0.3 * ramp(-e, 1.5);
    }
    e
}

/// The tone curve of a rendered (display-referred) source: identity at neutral settings, with
/// Contrast, Whites and Blacks as monotone curves in a gamma-2.2 perceptual domain. Endpoints
/// stay put except what Whites + pushes past white (a short soft shoulder) and Blacks − crushes.
pub fn display_tone(y: f32, contrast: f64, whites: f64, blacks: f64) -> f32 {
    if contrast == 0.0 && whites == 0.0 && blacks == 0.0 {
        return y.clamp(0.0, 1.0);
    }
    let c = (contrast / 100.0) as f32;
    let w = (whites / 100.0) as f32;
    let b = (blacks / 100.0) as f32;
    let m = GREY.powf(1.0 / 2.2);
    let mut p = y.max(0.0).powf(1.0 / 2.2);
    if c != 0.0 && p <= 1.0 {
        // S-curve about grey (slope at 0 and 1 kept positive: |c| ≤ 1)
        p += c * 0.35 * (p - m) * (1.0 - (2.0 * p - 1.0).powi(2));
    }
    if w != 0.0 {
        let up = smooth(0.45, 1.0, p.min(1.0));
        p += w * 0.12 * up * (1.0 - p.min(1.0) * 0.5);
    }
    if b < 0.0 {
        // crush: the bottom `k` of the range goes to black through a soft knee; 1 stays 1
        let k = -b * 0.04;
        let e = 0.006;
        let soft = |v: f32| ((v - k) + ((v - k) * (v - k) + e * e).sqrt()) * 0.5;
        p = (soft(p) - soft(0.0)) / (soft(1.0) - soft(0.0)).max(1e-6);
    } else if b > 0.0 {
        // lift the lower tones, black stays black
        p += b * 0.48 * p * (1.0 - p.min(1.0)).powi(3);
    }
    let mut o = p.max(0.0).powf(2.2);
    // short soft shoulder: slope 1 at 0.95, asymptotically approaching 1.0
    if o > 0.95 {
        o = 0.95 + 0.05 * (1.0 - (-(o - 0.95) / 0.05).exp());
    }
    o.clamp(0.0, 1.0)
}

#[inline]
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The tone table entry `i` (EV from grey as [`ToneMap`] samples it).
fn ev_at(i: usize) -> f32 {
    LUT_MIN_EV + (LUT_MAX_EV - LUT_MIN_EV) * i as f32 / (LUT_N - 1) as f32
}

impl ToneMap {
    /// global tone map of a scene-referred source: `base` after the sliders' reshaping of
    /// the scene exposure ([`scene_ev`]), the variant `shape` ([`base_shape`]) first.
    pub fn scene(base: &BaseCurve, shape: lightcraft_develop::ToneBase, hue: f32, contrast: f64, whites: f64, blacks: f64) -> ToneMap {
        let (k, w0, b0) = base_shape(shape);
        let lut: Vec<f32> = (0..LUT_N)
            .map(|i| {
                let ev = scene_ev(scene_ev(ev_at(i) * k, 0.0, w0, b0), contrast, whites, blacks);
                let y = GREY * ev.exp2();
                if shape == lightcraft_develop::ToneBase::Linear {
                    // Linear scene response up to 80%, with a C1 output shoulder.
                    if y <= 0.8 { y } else { 0.8 + 0.2 * (1.0 - (-(y - 0.8) / 0.2).exp()) }
                } else {
                    base.eval(y).clamp(0.0, 1.0)
                }
            })
            .collect();
        let chroma = match base {
            BaseCurve::Camera(c) => *c.chroma(),
            _ => [1.0; CHROMA_N],
        };
        ToneMap::from_tables(lut, chroma, ToneMethod { hue: hue.clamp(0.0, 1.0), luminance: matches!(base, BaseCurve::Camera(_)) })
    }

    /// global tone map of a rendered source (or a converted negative): [`display_tone`].
    pub fn rendered(hue: f32, contrast: f64, whites: f64, blacks: f64) -> ToneMap {
        let lut: Vec<f32> = (0..LUT_N).map(|i| display_tone(GREY * ev_at(i).exp2(), contrast, whites, blacks)).collect();
        ToneMap::from_tables(lut, [1.0; CHROMA_N], ToneMethod { hue: hue.clamp(0.0, 1.0), luminance: false })
    }
}

/// The base curve of look `look` on a source with `info` (global): the Camera look uses the
/// camera's curve (fitted to its JPEG, else the maker's), else the DNG profile's.
/// The Soft Film look always uses our independently fitted curve.
pub fn base_curve(look: lightcraft_develop::Look, info: &crate::SourceInfo) -> BaseCurve {
    use lightcraft_develop::Look;
    match look {
        Look::Sigmoid => BaseCurve::Sigmoid(Sigmoid::default_curve()),
        Look::Camera => info.look_curve.or(info.camera_tone).or(info.profile_curve).map_or(BaseCurve::Adobe, BaseCurve::Camera),
        Look::Adobe => BaseCurve::Adobe,
    }
}

/// The global tone map for `s` on `info`'s source with these contrast / whites / blacks.
pub fn tone_map(s: &lightcraft_develop::DevelopSettings, info: &crate::SourceInfo, contrast: f64, whites: f64, blacks: f64) -> ToneMap {
    let hue = (s.look_options.hue_preservation / 100.0) as f32;
    // a converted negative is already a print, a rendered file already has its tones
    if crate::negative::converts(s) || !info.raw {
        return ToneMap::rendered(hue, contrast, whites, blacks);
    }
    ToneMap::scene(&base_curve(s.look, info), s.look_options.base, hue, contrast, whites, blacks)
}

/// global tone of scene-linear `c` (see the module docs); display linear out, in 0..1.
#[inline]
pub fn tone_px(tone: &ToneMap, v: &ToneMethod, c: [f32; 3]) -> [f32; 3] {
    // negative channels (out-of-gamut scene colours) desaturated to zero (darktable sigmoid)
    let c = desaturate_negative(c);
    if v.luminance {
        let y = lightcraft_color::luminance_2020(c);
        if y <= 0.0 {
            return [0.0; 3];
        }
        let o = tone.apply(y);
        let k = tone.chroma_scale(o);
        let per = c.map(|x| tone.apply(x));
        let py = lightcraft_color::luminance_2020(per).max(1e-9);
        let d = std::array::from_fn(|i| {
            let q = v.hue * c[i] * o / y + (1.0 - v.hue) * per[i] * o / py;
            o + (q - o) * k
        });
        return crate::finish::gamut_map(d, [0.2627, 0.6780, 0.0593]).0;
    }
    let per = c.map(|x| tone.apply(x));
    let (lo, mid, hi) = channel_order(c);
    let mut d = preserve_hue_and_energy(c, per, lo, mid, hi, v.hue);
    // a camera curve's chroma by display luminance
    let o = lightcraft_color::luminance_2020(d);
    let k = tone.chroma_scale(o);
    if k != 1.0 {
        d = d.map(|x| o + (x - o) * k);
    }
    crate::finish::gamut_map(d, [0.2627, 0.6780, 0.0593]).0
}

/// darktable sigmoid's `_desaturate_negative_values`.
#[inline]
pub fn desaturate_negative(c: [f32; 3]) -> [f32; 3] {
    let avg = ((c[0] + c[1] + c[2]) / 3.0).max(0.0);
    let mn = c[0].min(c[1]).min(c[2]);
    let f = if mn < 0.0 { -avg / (mn - avg) } else { 1.0 };
    c.map(|x| avg + f * (x - avg))
}

/// darktable sigmoid's `_pixel_channel_order`: indices of the (min, mid, max) channels.
#[inline]
pub fn channel_order(p: [f32; 3]) -> (usize, usize, usize) {
    if p[0] >= p[1] {
        if p[1] > p[2] {
            (2, 1, 0)
        } else if p[2] > p[0] {
            (1, 0, 2)
        } else if p[2] > p[1] {
            (1, 2, 0)
        } else {
            (2, 1, 0)
        }
    } else if p[0] >= p[2] {
        (2, 0, 1)
    } else if p[2] > p[1] {
        (0, 1, 2)
    } else {
        (0, 2, 1)
    }
}

/// darktable sigmoid's `_preserve_hue_and_energy`: the per-channel result `per` of `pix` with its
/// middle channel moved towards `pix`'s hue (by `hue`, 0..1), keeping the channels' sum.
#[inline]
pub fn preserve_hue_and_energy(pix: [f32; 3], per: [f32; 3], lo: usize, mid: usize, hi: usize, hue: f32) -> [f32; 3] {
    let chroma = pix[hi] - pix[lo];
    let midscale = if chroma != 0.0 { (pix[mid] - pix[lo]) / chroma } else { 0.0 };
    let full_hue_correction = per[lo] + (per[hi] - per[lo]) * midscale;
    let naive_hue_mid = (1.0 - hue) * per[mid] + hue * full_hue_correction;
    let per_channel_energy = per[0] + per[1] + per[2];
    let naive_hue_energy = per[lo] + naive_hue_mid + per[hi];
    let pix_lo_plus_mid = pix[lo] + pix[mid];
    let blend = if pix_lo_plus_mid != 0.0 { 2.0 * pix[lo] / pix_lo_plus_mid } else { 0.0 };
    let energy_target = blend * per_channel_energy + (1.0 - blend) * naive_hue_energy;
    let mut out = [0.0f32; 3];
    if naive_hue_mid <= per[mid] {
        let corrected_mid =
            ((1.0 - hue) * per[mid] + hue * (midscale * per[hi] + (1.0 - midscale) * (energy_target - per[hi]))) / (1.0 + hue * (1.0 - midscale));
        out[lo] = energy_target - per[hi] - corrected_mid;
        out[mid] = corrected_mid;
        out[hi] = per[hi];
    } else {
        let corrected_mid =
            ((1.0 - hue) * per[mid] + hue * (per[lo] * (1.0 - midscale) + midscale * (energy_target - per[lo]))) / (1.0 + hue * midscale);
        out[lo] = per[lo];
        out[mid] = corrected_mid;
        out[hi] = energy_target - per[lo] - corrected_mid;
    }
    out
}

/// darktable sigmoid's hyperbolic gamut compression (`process_loglogistic_rgb_ratio`): chroma
/// around the mapped norm `m` is compressed so the colour stays between the display's `black` and
/// `white`; colours whose limiting border is black pass unchanged, bright saturated colours
/// desaturate smoothly towards white.
#[inline]
pub fn compress(d: [f32; 3], m: f32, white: f32, black: f32) -> [f32; 3] {
    let mn = d[0].min(d[1]).min(d[2]);
    let mx = d[0].max(d[1]).max(d[2]);
    let eps = 1e-6f32;
    let border_white = (white - m) / (mx - m + eps);
    let border_black = (black - m) / (mn - m - eps);
    let border = border_white.min(border_black);
    let chroma = (m - mn) / (m + eps);
    let adjust = 1.0 / (chroma * border + eps);
    let hyper = 2.0 * chroma / (1.0 - chroma * chroma + eps) * adjust;
    let z = (hyper * hyper + 1.0).sqrt();
    let factor = hyper / (1.0 + z) * border;
    d.map(|x| m + factor * (x - m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_develop::ToneBase;

    fn camera_curve() -> CameraTone {
        let knots: [[f32; 2]; 32] = std::array::from_fn(|i| {
            let x = 0.002 * 1.2f32.powi(i as i32);
            [x, (1.0 - (-2.2 * x).exp()).min(0.999)]
        });
        CameraTone::new(knots).unwrap()
    }

    fn looks() -> Vec<(&'static str, BaseCurve)> {
        vec![("adobe", BaseCurve::Adobe), ("sigmoid", BaseCurve::Sigmoid(Sigmoid::default_curve())), ("camera", BaseCurve::Camera(camera_curve()))]
    }

    fn scene(base: &BaseCurve, c: f64, w: f64, b: f64) -> ToneMap {
        ToneMap::scene(base, ToneBase::Standard, 1.0, c, w, b)
    }

    #[test]
    fn sigmoid_matches_darktable_targets() {
        let s = Sigmoid::default_curve();
        // f(0) = display black, f(grey) = grey, f(∞) = white (darktable's commit_params goals)
        assert!((s.eval(0.0) - 0.000152).abs() < 2e-6, "{}", s.eval(0.0));
        assert!((s.eval(SIGMOID_GREY) - SIGMOID_GREY).abs() < 1e-4, "{}", s.eval(SIGMOID_GREY));
        assert!(s.eval(1e6) > 0.999 && s.eval(1e6) <= 1.0);
        // a higher contrast is steeper at grey
        let d = |s: &Sigmoid| (s.eval(SIGMOID_GREY * 1.01) - s.eval(SIGMOID_GREY / 1.01)) / (SIGMOID_GREY * (1.01 - 1.0 / 1.01));
        assert!(d(&Sigmoid::new(2.0, 0.0, 100.0, 0.0152)) > d(&s));
        assert!(s.film_power.is_finite() && s.film_fog > 0.0 && s.paper_exposure > 0.0);
    }

    #[test]
    fn looks_are_monotone_black_to_black_and_place_grey() {
        for (name, base) in looks() {
            for shape in ToneBase::ALL {
                for (c, w, b) in [(0.0, 0.0, 0.0), (100.0, 100.0, -100.0), (-100.0, -100.0, 100.0), (50.0, -40.0, 30.0)] {
                    let t = ToneMap::scene(&base, shape, 1.0, c, w, b);
                    assert!(t.apply(0.0) == 0.0, "{name}: black");
                    let mut prev = -1.0;
                    for i in 0..3000 {
                        let y = 1e-6 * 1.01f32.powi(i);
                        let o = t.apply(y);
                        assert!((0.0..=1.0).contains(&o), "{name} {shape:?} {c} {w} {b}: {o}");
                        assert!(o >= prev - 1e-6, "{name} {shape:?} {c} {w} {b} at {y}: {o} < {prev}");
                        prev = o;
                    }
                }
            }
            let t = scene(&base, 0.0, 0.0, 0.0);
            // the very darkest tones stay at black (below half an 8-bit level once encoded)
            assert!(lightcraft_color::transfer::linear_to_srgb(t.apply(1e-5)) * 255.0 < 0.5, "{name}: {}", t.apply(1e-5));
        }
        // grey placement: both brighter than the scene value (a metered grey sits ~3 EV below
        // sensor white, below 0.18)
        let adobe = scene(&BaseCurve::Adobe, 0.0, 0.0, 0.0).apply(GREY);
        assert!((0.27..0.31).contains(&adobe), "{adobe}");
        let sig = scene(&BaseCurve::Sigmoid(Sigmoid::default_curve()), 0.0, 0.0, 0.0).apply(GREY);
        assert!((0.28..0.34).contains(&sig), "{sig}");
        // sensor white (1.0) renders near white, with headroom left above it
        let white = scene(&BaseCurve::Adobe, 0.0, 0.0, 0.0).apply(1.0);
        assert!((0.88..0.97).contains(&white), "{white}");
    }

    #[test]
    fn adobe_like_has_five_stops_of_headroom_and_no_knee() {
        let t = scene(&BaseCurve::Adobe, 0.0, 0.0, 0.0);
        let enc = |y: f32| lightcraft_color::transfer::linear_to_srgb(t.apply(y)) * 255.0;
        // each stop up to +5 EV above grey is still at least about one 8-bit level brighter
        for ev in 0..5 {
            let (a, b) = (enc(GREY * 2f32.powi(ev)), enc(GREY * 2f32.powi(ev + 1)));
            assert!(b - a > 0.9, "+{ev}..{} EV: {a} → {b}", ev + 1);
        }
        assert!(enc(GREY * 32.0) < 254.9);
        // no hard knee: the slope (in EV) changes smoothly
        let slope = |ev: f32| (t.apply(GREY * (ev + 0.01).exp2()).ln() - t.apply(GREY * (ev - 0.01).exp2()).ln()) / 0.02;
        let mut prev = slope(-6.0);
        for i in 0..1100 {
            let s = slope(-6.0 + i as f32 * 0.01);
            assert!((s - prev).abs() < 0.02, "slope jumps at {} EV", -6.0 + i as f32 * 0.01);
            prev = s;
        }
    }

    #[test]
    fn sliders_move_the_right_way_on_every_look() {
        for (name, base) in looks() {
            let at = |c: f64, w: f64, b: f64, y: f32| scene(&base, c, w, b).apply(y);
            assert!(at(60.0, 0.0, 0.0, 0.03) < at(0.0, 0.0, 0.0, 0.03), "{name} contrast shadows");
            assert!(at(60.0, 0.0, 0.0, 0.8) > at(0.0, 0.0, 0.0, 0.8), "{name} contrast highlights");
            assert!(at(0.0, 60.0, 0.0, 0.8) > at(0.0, 0.0, 0.0, 0.8), "{name} whites");
            assert!(at(0.0, -60.0, 0.0, 0.8) < at(0.0, 0.0, 0.0, 0.8), "{name} whites −");
            assert!(at(0.0, 0.0, 60.0, 0.01) > at(0.0, 0.0, 0.0, 0.01), "{name} blacks");
            assert!(at(0.0, 0.0, -60.0, 0.01) < at(0.0, 0.0, 0.0, 0.01), "{name} blacks −");
            // each slider stays in its band: grey stays put under whites and blacks, and whites
            // barely move the shadows, blacks the highlights
            assert!((at(0.0, 80.0, -80.0, GREY) - at(0.0, 0.0, 0.0, GREY)).abs() < 1e-4, "{name} grey");
            assert!((at(0.0, 100.0, 0.0, 0.02) - at(0.0, 0.0, 0.0, 0.02)).abs() < 1e-5, "{name} whites in the shadows");
            assert!((at(0.0, 0.0, 100.0, 1.0) - at(0.0, 0.0, 0.0, 1.0)).abs() < 1e-5, "{name} blacks in the highlights");
        }
        // the base-curve variants
        let at = |shape: ToneBase, y: f32| ToneMap::scene(&BaseCurve::Adobe, shape, 1.0, 0.0, 0.0, 0.0).apply(y);
        assert!(at(ToneBase::ExtraShadow, 0.01) > at(ToneBase::Standard, 0.01));
        assert!(at(ToneBase::HighContrast, 0.02) < at(ToneBase::Standard, 0.02) && at(ToneBase::HighContrast, 0.7) > at(ToneBase::Standard, 0.7));
        assert!((at(ToneBase::Linear, 0.18) - 0.18).abs() < 1e-5);
        assert!((at(ToneBase::Linear, 0.5) - 0.5).abs() < 1e-5);
        assert!(at(ToneBase::Linear, 1.0) < 1.0);
        // and the same directions on a rendered source
        let at = |c: f64, w: f64, b: f64, y: f32| ToneMap::rendered(1.0, c, w, b).apply(y);
        assert!(at(60.0, 0.0, 0.0, 0.03) < at(0.0, 0.0, 0.0, 0.03) && at(60.0, 0.0, 0.0, 0.7) > at(0.0, 0.0, 0.0, 0.7));
        assert!(at(0.0, 60.0, 0.0, 0.8) > 0.8 && at(0.0, -60.0, 0.0, 0.8) < 0.8);
        assert!(at(0.0, 0.0, 60.0, 0.01) > 0.01 && at(0.0, 0.0, -60.0, 0.01) < 0.01);
    }

    #[test]
    fn rendered_sources_are_identity_at_neutral() {
        let t = ToneMap::rendered(0.75, 0.0, 0.0, 0.0);
        for i in 1..=95 {
            let y = i as f32 / 100.0;
            assert!((t.apply(y) - y).abs() < 2e-3, "{y} -> {}", t.apply(y));
        }
        let v = t.method();
        for c in [[0.6, 0.3, 0.1], [0.05, 0.4, 0.9], [0.9, 0.9, 0.85]] {
            let d = tone_px(&t, &v, c);
            assert!((0..3).all(|k| (d[k] - c[k]).abs() < 3e-3), "{c:?} → {d:?}");
        }
        for (c, w, b) in [(100.0, 100.0, -100.0), (-100.0, -100.0, 100.0), (60.0, -40.0, 30.0)] {
            let t = ToneMap::rendered(1.0, c, w, b);
            let mut prev = -1.0;
            for i in 0..1000 {
                let o = t.apply(i as f32 / 500.0);
                assert!(o >= prev - 1e-5, "{c} {w} {b}");
                prev = o;
            }
        }
    }

    fn hue(d: [f32; 3]) -> f32 {
        let lab = lightcraft_color::perceptual::oklab_from_2020(d);
        lab[2].atan2(lab[1])
    }

    #[test]
    fn full_hue_preservation_keeps_hue_and_bleaches_towards_white() {
        for (name, base) in looks() {
            let t = scene(&base, 0.0, 0.0, 0.0);
            let v = t.method();
            assert_eq!(v.hue, 1.0);
            // saturated ramps (orange, green, blue): RGB hue (the middle channel's place between
            // min and max) kept, saturation never rising near white, everything in range
            for col in [[1.0f32, 0.35, 0.08], [0.12, 0.9, 0.2], [0.1, 0.15, 1.0]] {
                let rgb_hue = |d: [f32; 3]| {
                    let (lo, mid, hi) = channel_order(d);
                    (d[mid] - d[lo]) / (d[hi] - d[lo]).max(1e-9)
                };
                let h0 = rgb_hue(col);
                let mut prev_sat = f32::MAX;
                let mut prev_y = 0.0;
                for i in 0..260 {
                    let k = 0.002 * 1.05f32.powi(i);
                    let d = tone_px(&t, &v, col.map(|x| x * k));
                    assert!(d.iter().all(|x| (0.0..=1.0).contains(x)), "{name} {col:?}×{k}: {d:?}");
                    let y = lightcraft_color::luminance_2020(d);
                    assert!(y >= prev_y - 1e-4, "{name} {col:?}×{k}: luminance falls");
                    prev_y = y;
                    let mx = d[0].max(d[1]).max(d[2]);
                    let sat = (mx - d[0].min(d[1]).min(d[2])) / mx.max(1e-6);
                    if mx - d[0].min(d[1]).min(d[2]) > 0.02 {
                        assert!((rgb_hue(d) - h0).abs() < 0.02, "{name} {col:?}×{k}: hue {} vs {h0}", rgb_hue(d));
                    }
                    if y > 0.5 {
                        assert!(sat <= prev_sat + 1e-3, "{name} {col:?}×{k}: saturation grows near white");
                        prev_sat = sat;
                    }
                }
                let top = tone_px(&t, &v, col.map(|x| x * 1e4));
                assert!(top.iter().all(|x| *x > 0.97), "{name}: very bright colours end at white: {top:?}");
            }
            // neutral stays neutral, black stays black
            let g = tone_px(&t, &v, [0.18; 3]);
            assert!((g[0] - g[1]).abs() < 1e-6 && (g[1] - g[2]).abs() < 1e-6);
            assert!(tone_px(&t, &v, [0.0; 3]).iter().all(|x| x.abs() < 1e-3));
            // an out-of-gamut scene colour (a negative channel) still maps into range
            let d = tone_px(&t, &v, [0.5, 0.2, -0.1]);
            assert!(d.iter().all(|x| (0.0..=1.0).contains(x)), "{d:?}");
        }
    }

    #[test]
    fn lower_hue_preservation_lets_bright_reds_drift_towards_yellow() {
        let full = ToneMap::scene(&BaseCurve::Adobe, ToneBase::Standard, 1.0, 0.0, 0.0, 0.0);
        let none = ToneMap::scene(&BaseCurve::Adobe, ToneBase::Standard, 0.0, 0.0, 0.0, 0.0);
        let c = [3.0, 0.6, 0.1];
        let (a, b) = (tone_px(&full, &full.method(), c), tone_px(&none, &none.method(), c));
        // the per-channel curve lifts the middle (green) channel relative to red
        assert!(b[1] / b[0] > a[1] / a[0], "{a:?} {b:?}");
        assert!((hue(a) - hue(b)).abs() > 0.01);
    }

    #[test]
    fn compression_leaves_black_limited_colours_alone() {
        // a mid-luminance colour whose nearest border is black: unchanged (darktable's design)
        let d = [0.3, 0.15, 0.09];
        let m = lightcraft_color::luminance_2020(d);
        let out = compress(d, m, 1.0, 0.0);
        assert!((0..3).all(|k| (out[k] - d[k]).abs() < 1e-4), "{out:?}");
        // past white: pulled inside
        let out = compress([1.4, 0.8, 0.5], 0.85, 1.0, 0.0);
        assert!(out.iter().all(|x| *x <= 1.0 && *x >= 0.0), "{out:?}");
    }
}

#[cfg(test)]
mod reference_vectors {
    use super::*;
    #[test]
    fn extracted_darktable_functions_match() {
        let mut worst = 0.0f32;
        for row in include_str!("../tests/fixtures/sigmoid.csv").lines() {
            let (kind, row) = row.split_once(',').unwrap();
            let v: Vec<f32> = row.split(',').map(|v| v.parse().unwrap()).collect();
            if kind == "curve" {
                worst = worst.max((Sigmoid::new(v[0], v[1], v[2], v[3]).eval(v[4]) - v[5]).abs());
            } else {
                let c = desaturate_negative([v[0], v[1], v[2]]);
                let per = c.map(|x| Sigmoid::default_curve().eval(x));
                let (lo, mid, hi) = channel_order(c);
                let out = preserve_hue_and_energy(c, per, lo, mid, hi, v[3]);
                for i in 0..3 {
                    worst = worst.max((out[i] - v[4 + i]).abs());
                }
            }
        }
        assert!(worst < 2e-6, "darktable C absolute error: {worst}");
    }
}
