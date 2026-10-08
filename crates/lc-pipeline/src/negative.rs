//! Film negative conversion: inverts a scanned colour or black & white negative and simulates
//! printing it on paper.
//!
//! Ported to Rust from darktable's *negadoctor* module (`src/iop/negadoctor.c`,
//! <https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/negadoctor.c>,
//! darktable commit `733bd69f32cac7ff5e41025115942772add1f088`), Copyright (C) 2020-2026 darktable
//! developers, licensed under the GNU General Public License version 3 or (at your option) any
//! later version — the licence of Local Image too. See `licenses/darktable-NOTICE.md` and
//! `docs/PORTS.md`. Ported: the per-pixel model (`commit_params` + `_process_pixel`), the parameter
//! units, ranges and defaults, and the "auto" colour pickers (`apply_auto_Dmin`, `apply_auto_Dmax`,
//! `apply_auto_offset`, `apply_auto_black`, `apply_auto_exposure`).
//!
//! The model (Kodak Cineon densitometry): the scan's transmittance `T` is turned into density
//! above the film base, `D = log10(Dmin / T)`, rescaled by the film's range `Dmax`, white-balanced
//! and offset in density, turned back into the light a print paper receives, and printed:
//!
//! ```text
//! corrected = −D · wb_high / Dmax + offset · wb_high · wb_low
//! print     = (exposure · (1 + black − 10^corrected))^gamma, then a soft shoulder above `soft_clip`
//! ```
//!
//! In LightCraft the conversion runs on the white-balanced, scene-linear (linear Rec.2020, the
//! same working space as darktable's default) scan, right after white balance and before
//! everything else, because it changes what every later stage sees. Its output is a print:
//! display-referred values in 0..1, so the finish stage uses the display tone map (identity at
//! neutral settings) for converted negatives instead of the scene-referred camera curve.

use lightcraft_develop::{DevelopSettings, FilmRgb, FilmStock, Negative};
use lightcraft_geom::{Point, Rect};
use lightcraft_raster::Rgb32f;
use serde::Serialize;

use crate::{RenderRequest, SourceInfo, for_rows};

/// Darkest transmittance considered (−32 EV), as negadoctor's `THRESHOLD`.
pub const THRESHOLD: f32 = 2.328_306_4e-10;

/// The per-pixel parameters of the conversion (negadoctor's `commit_params`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NegParams {
    /// Film base transmittance per channel (B&W: the red component for all three).
    pub dmin: [f32; 3],
    /// Highlights white balance, premultiplied by `1 / Dmax`.
    pub wb_high: [f32; 3],
    /// Density offset per channel: `wb_high · offset · wb_low`.
    pub offset: [f32; 3],
    /// `−exposure · (1 + black)` (lets the print be written as one multiply-add).
    pub black: f32,
    pub exposure: f32,
    pub gamma: f32,
    pub soft_clip: f32,
    /// `1 − soft_clip`.
    pub soft_clip_comp: f32,
}

impl NegParams {
    /// `None` for slides (nothing to invert).
    pub fn new(n: &Negative) -> Option<NegParams> {
        let dmin = effective_dmin(n)?.map(|v| (v as f32).max(THRESHOLD));
        let d_max = (n.d_max as f32).max(1e-3);
        let wb_high = n.wb_high.to_array().map(|v| v as f32);
        let wb_low = n.wb_low.to_array().map(|v| v as f32);
        let exposure = n.exposure as f32;
        let soft_clip = n.soft_clip as f32;
        Some(NegParams {
            dmin,
            wb_high: wb_high.map(|v| v / d_max),
            offset: std::array::from_fn(|c| wb_high[c] * n.offset as f32 * wb_low[c]),
            black: -exposure * (1.0 + n.black as f32),
            exposure,
            gamma: n.gamma as f32,
            soft_clip,
            // (negadoctor allows a gloss of 1, where the shoulder vanishes: keep the division finite)
            soft_clip_comp: (1.0 - soft_clip).max(1e-6),
        })
    }

    /// Convert one white-balanced scene-linear pixel (negadoctor's `_process_pixel`).
    #[inline]
    pub fn convert(&self, pix: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|c| {
            // transmission → density, with D-min as the fulcrum: log_density = −log10(Dmin / T)
            let clamped = pix[c].max(THRESHOLD);
            let log_density = -(self.dmin[c] / clamped).log10();
            // density corrections in log space
            let corrected = self.wb_high[c] * log_density + self.offset[c];
            // print on paper: ((1 + black − 10^corrected) · exposure)^gamma
            let print_linear = (-(self.exposure * 10f32.powf(corrected) + self.black)).max(0.0);
            let print_gamma = print_linear.powf(self.gamma);
            // highlights shoulder (https://lists.gnu.org/archive/html/openexr-devel/2005-03/msg00009.html)
            if print_gamma > self.soft_clip {
                self.soft_clip + (1.0 - (-(print_gamma - self.soft_clip) / self.soft_clip_comp).exp()) * self.soft_clip_comp
            } else {
                print_gamma
            }
        })
    }
}

/// The film base the conversion uses: per channel for colour film, the red component for all
/// channels for B&W film; `None` for slides.
pub fn effective_dmin(n: &Negative) -> Option<[f64; 3]> {
    match n.film {
        FilmStock::Color => Some(n.dmin.to_array()),
        FilmStock::Bw => Some([n.dmin.r; 3]),
        FilmStock::Slide => None,
    }
}

/// The conversion `s` asks for: `None` when it is off (or the section is hidden with its eye
/// button) and for slides, which pass through unchanged.
pub fn params(s: &DevelopSettings) -> Option<NegParams> {
    if !s.negative.enabled || !s.section_enabled("negative") {
        return None;
    }
    NegParams::new(&s.negative)
}

/// Whether `s` converts a negative (the image after this stage is a display-referred print).
pub fn converts(s: &DevelopSettings) -> bool {
    params(s).is_some()
}

/// Convert `img` (white-balanced, scene-linear) in place; nothing happens when [`params`] is `None`.
pub fn apply(img: &mut Rgb32f, s: &DevelopSettings) {
    if let Some(k) = params(s) {
        apply_params(img, &k);
    }
}

pub fn apply_params(img: &mut Rgb32f, k: &NegParams) {
    let w = img.width;
    for_rows(&mut img.data, w, |_, row| {
        for p in row.iter_mut() {
            *p = k.convert(*p);
        }
    });
}

// ------------------------------------------------------------------------------------------------
// Auto pickers (negadoctor's colour-picker auto-tuners), on statistics of the stage's input.

/// Statistics of the conversion's input (the white-balanced scan) over an area.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct AreaStats {
    pub mean: [f32; 3],
    /// Per-channel minimum / maximum (or low / high percentiles, see [`area_stats`]).
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Pixels measured.
    pub n: usize,
}

/// Mean and per-channel extremes of `pixels`. `tail` (0..0.2) ignores that fraction of the
/// darkest and brightest values of each channel for the extremes (0: the true minimum and
/// maximum, as negadoctor's area pickers measure them; image-wide statistics use a small tail so a
/// speck of dust or a hot pixel doesn't decide). `None` for no pixels.
pub fn area_stats(pixels: &[[f32; 3]], tail: f32) -> Option<AreaStats> {
    let px: Vec<[f32; 3]> = pixels.iter().copied().filter(|p| p.iter().all(|v| v.is_finite())).collect();
    if px.is_empty() {
        return None;
    }
    let n = px.len();
    let mut mean = [0f64; 3];
    for p in &px {
        for c in 0..3 {
            mean[c] += p[c] as f64;
        }
    }
    let mean = mean.map(|v| (v / n as f64) as f32);
    let tail = tail.clamp(0.0, 0.2);
    let (mut min, mut max) = ([0f32; 3], [0f32; 3]);
    for c in 0..3 {
        let mut ch: Vec<f32> = px.iter().map(|p| p[c]).collect();
        ch.sort_by(|a, b| a.total_cmp(b));
        let k = ((n - 1) as f32 * tail).round() as usize;
        min[c] = ch[k];
        max[c] = ch[n - 1 - k];
    }
    Some(AreaStats { mean, min, max, n })
}

/// D-min from an area of the unexposed film rim (`apply_auto_Dmin`: its average colour). For B&W
/// film all three components get the area's mean grey (the conversion uses one base density).
pub fn auto_dmin(film: FilmStock, area: &AreaStats) -> FilmRgb {
    let m = area.mean.map(|v| (v.max(THRESHOLD) as f64).clamp(0.00001, 1.5));
    match film {
        FilmStock::Bw => FilmRgb::new((m[0] + m[1] + m[2]) / 3.0, (m[0] + m[1] + m[2]) / 3.0, (m[0] + m[1] + m[2]) / 3.0),
        _ => FilmRgb::from_array(m),
    }
}

/// D-max from the densest part of the negative (`apply_auto_Dmax`): the largest density range of
/// the three channels, so no white clips.
pub fn auto_dmax(n: &Negative, area: &AreaStats) -> f64 {
    let Some(dmin) = effective_dmin(n) else { return n.d_max };
    let d = (0..3).map(|c| (dmin[c] / (area.min[c].max(THRESHOLD) as f64)).log10()).fold(f64::MIN, f64::max);
    d.clamp(0.1, 6.0)
}

/// Scan exposure bias from the thinnest part of the negative (`apply_auto_offset`): rescales the
/// density range to 0..1, taking the smallest of the channels so no black clips.
pub fn auto_offset(n: &Negative, area: &AreaStats) -> f64 {
    let Some(dmin) = effective_dmin(n) else { return n.offset };
    let d_max = n.d_max.max(1e-3);
    let o = (0..3).map(|c| (dmin[c] / (area.max[c].max(THRESHOLD) as f64)).log10() / d_max).fold(f64::MAX, f64::min);
    o.clamp(-1.0, 1.0)
}

/// The corrected density of an input value (the exponent of the print model).
fn corrected(n: &Negative, dmin: f64, v: f32, c: usize) -> f64 {
    let wb_high = n.wb_high.to_array()[c];
    let wb_low = n.wb_low.to_array()[c];
    -(dmin / (v.max(THRESHOLD) as f64)).log10() * wb_high / n.d_max.max(1e-3) + wb_low * n.offset * wb_high
}

/// Paper black from the thinnest part of the negative (`apply_auto_black`): the print's darkest
/// value lands just above black (1 + black − 10^corrected = 0.1, before the paper grade).
pub fn auto_black(n: &Negative, area: &AreaStats) -> f64 {
    let Some(dmin) = effective_dmin(n) else { return n.black };
    let b = (0..3).map(|c| 0.1 - (1.0 - 10f64.powf(corrected(n, dmin[c], area.max[c], c)))).fold(f64::MIN, f64::max);
    b.clamp(-0.5, 0.5)
}

/// Print exposure from the densest part of the negative (`apply_auto_exposure`): the print's
/// brightest value lands at 0.96 (before the paper grade). Unlike negadoctor, the density offset
/// here includes the highlights white balance exactly as the conversion applies it (negadoctor's
/// picker leaves it out; the two agree at the neutral balance of 1).
pub fn auto_exposure(n: &Negative, area: &AreaStats) -> f64 {
    let Some(dmin) = effective_dmin(n) else { return n.exposure };
    let e = (0..3).map(|c| 0.96 / (1.0 - 10f64.powf(corrected(n, dmin[c], area.min[c], c)) + n.black)).filter(|v| *v > 0.0).fold(f64::MAX, f64::min);
    if e == f64::MAX { n.exposure } else { e.clamp(0.5, 2.0) }
}

/// The automatic D-max and scan exposure bias, in that order (each depends on the previous).
pub fn auto_range(n: &Negative, area: &AreaStats) -> Negative {
    let mut out = *n;
    out.d_max = auto_dmax(&out, area);
    out.offset = auto_offset(&out, area);
    out
}

/// The automatic paper black and print exposure, in that order.
pub fn auto_print(n: &Negative, area: &AreaStats) -> Negative {
    let mut out = *n;
    out.black = auto_black(&out, area);
    out.exposure = auto_exposure(&out, area);
    out
}

// ------------------------------------------------------------------------------------------------
// Sampling the stage's input from a source.

/// Longest edge of the proxy the samplers measure.
const SAMPLE_EDGE: usize = 768;

/// The conversion's input (the scan resampled through the frame, white-balanced) at proxy size,
/// and its output px → normalized oriented coordinates map.
fn stage_input(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings, apply_crop: bool) -> (Rgb32f, lightcraft_geom::Affine) {
    let req = RenderRequest { apply_crop, ..RenderRequest::fit(SAMPLE_EDGE, SAMPLE_EDGE) };
    let plan = crate::plan(src, info, s, &req);
    let mut img = plan.frame.sample(src, plan.w, plan.h);
    crate::local::white_balance(&mut img, info, &plan.settings);
    (img, plan.frame.out_to_norm(plan.w, plan.h))
}

/// Statistics of the conversion's input over `area` (normalized oriented-image coordinates, the
/// uncropped image: the film rim is usually cropped away). Extremes are the true ones, as
/// negadoctor's area pickers measure them. A tiny area measures the pixel at its centre.
pub fn sample_area(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings, area: Rect) -> Option<AreaStats> {
    let (img, to_norm) = stage_input(src, info, s, false);
    let area = Rect::from_points(Point::new(area.x0, area.y0), Point::new(area.x1, area.y1));
    let mut px = Vec::new();
    for y in 0..img.height {
        for x in 0..img.width {
            if area.contains(to_norm.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5))) {
                px.push(img.data[y * img.width + x]);
            }
        }
    }
    if px.is_empty() {
        let c = to_norm.inverse()?.apply(area.center());
        let (x, y) = (c.x.floor(), c.y.floor());
        if x < 0.0 || y < 0.0 || x >= img.width as f64 || y >= img.height as f64 {
            return None;
        }
        px.push(img.data[y as usize * img.width + x as usize]);
    }
    area_stats(&px, 0.0)
}

/// Statistics of the conversion's input over the photo as cropped (an inner 96 %: the film rim and
/// scanner holder are usually cropped away), with 0.1 % tails ignored for the extremes.
pub fn sample_image(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings) -> Option<AreaStats> {
    let (img, _) = stage_input(src, info, s, true);
    let (mx, my) = (img.width / 50, img.height / 50);
    let mut px = Vec::with_capacity(img.data.len());
    for y in my..img.height - my {
        px.extend_from_slice(&img.data[y * img.width + mx..y * img.width + img.width - mx]);
    }
    area_stats(&px, 0.001)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inverse of the conversion: the scan (transmittance) a film with base `dmin` and the
    /// settings `n` turns into the print value `out` (before the highlight shoulder, i.e. for
    /// outputs ≤ the gloss threshold).
    fn simulate_scan(n: &Negative, dmin: [f64; 3], out: [f32; 3]) -> [f32; 3] {
        let k = NegParams::new(n).unwrap();
        std::array::from_fn(|c| {
            assert!(out[c] <= k.soft_clip, "the shoulder is not inverted here");
            let print_linear = out[c].max(1e-6).powf(1.0 / k.gamma);
            // print_linear = −(exposure · 10^corrected + black)
            let ten = (-print_linear - k.black) / k.exposure;
            let corrected = ten.log10();
            let log_density = (corrected - k.offset[c]) / k.wb_high[c];
            // log_density = −log10(Dmin / T) → T = Dmin · 10^log_density
            (dmin[c] as f32 * 10f32.powf(log_density)).max(THRESHOLD)
        })
    }

    fn negative(film: FilmStock) -> Negative {
        Negative { enabled: true, film, dmin: FilmRgb::new(0.82, 0.41, 0.19), ..Negative::default() }
    }

    /// A positive test chart: a grey ramp plus colour patches (print values, 0.02..0.7).
    fn chart() -> Vec<[f32; 3]> {
        let mut v: Vec<[f32; 3]> = (0..24).map(|i| [0.02 + i as f32 * 0.028; 3]).collect();
        v.extend([[0.6, 0.2, 0.1], [0.1, 0.5, 0.15], [0.08, 0.15, 0.6], [0.65, 0.6, 0.12], [0.4, 0.1, 0.45], [0.3, 0.22, 0.17]]);
        v
    }

    #[test]
    fn orange_masked_colour_negative_converts_back_to_the_positive() {
        let n = negative(FilmStock::Color);
        let dmin = n.dmin.to_array();
        let positive = chart();
        // the film: orange base, densities from the positive through the inverse model
        let scan: Vec<[f32; 3]> = positive.iter().map(|p| simulate_scan(&n, dmin, *p)).collect();
        for s in scan.iter().take(24) {
            assert!(s[0] > s[1] && s[1] > s[2], "greys are orange on the film: {s:?}");
        }
        for (s, p) in scan.iter().zip(&positive) {
            assert!(s.iter().zip(dmin).all(|(v, d)| (*v as f64) <= d * 1.0001), "no part is clearer than the base: {s:?} for {p:?}");
        }
        // brighter print = denser negative (less light through)
        assert!(scan[23][1] < scan[0][1]);
        let k = NegParams::new(&n).unwrap();
        for (s, p) in scan.iter().zip(&positive) {
            let back = k.convert(*s);
            for c in 0..3 {
                assert!((back[c] - p[c]).abs() < 2e-3, "{back:?} vs {p:?}");
            }
        }
        // through the stage on an image, with the settings switch
        let mut img = Rgb32f::from_fn(scan.len(), 1, |x, _| scan[x]);
        let s = DevelopSettings { negative: n, ..Default::default() };
        apply(&mut img, &s);
        for (o, p) in img.data.iter().zip(&positive) {
            assert!((0..3).all(|c| (o[c] - p[c]).abs() < 2e-3), "{o:?} vs {p:?}");
        }
        // the film base itself prints (near) black, a clearer-than-base value is clamped
        let base = k.convert(dmin.map(|v| v as f32));
        assert!(base.iter().all(|v| *v < 1e-3), "{base:?}");
        assert!(k.convert([2.0, 2.0, 2.0]).iter().all(|v| *v == 0.0));
        // and NaN / negative / zero pixels don't leak
        for bad in [[f32::NAN; 3], [-1.0; 3], [0.0; 3]] {
            assert!(k.convert(bad).iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{bad:?}");
        }
    }

    #[test]
    fn black_and_white_negative_uses_one_base_density() {
        let n = Negative { d_max: 2.2, gamma: 5.0, exposure: 1.0, ..negative(FilmStock::Bw) };
        let k = NegParams::new(&n).unwrap();
        assert_eq!(k.dmin, [0.82; 3], "the red component for all channels");
        let grey = [0.82f64; 3];
        let positive: Vec<[f32; 3]> = (0..20).map(|i| [0.02 + i as f32 * 0.035; 3]).collect();
        let mut prev = -1.0;
        for p in &positive {
            let scan = simulate_scan(&n, grey, *p);
            let back = k.convert(scan);
            assert!((back[0] - p[0]).abs() < 2e-3 && back[0] == back[1] && back[1] == back[2], "{back:?} vs {p:?}");
            assert!(back[0] > prev, "monotone");
            prev = back[0];
        }
        // a neutral scan stays neutral whatever its D-min green/blue say
        let k2 = NegParams::new(&Negative { dmin: FilmRgb::new(0.82, 0.1, 0.9), ..n }).unwrap();
        let o = k2.convert([0.1, 0.1, 0.1]);
        assert!(o[0] == o[1] && o[1] == o[2], "{o:?}");
    }

    #[test]
    fn slides_and_disabled_settings_pass_through() {
        let mut s = DevelopSettings::default();
        assert!(params(&s).is_none(), "off by default");
        s.negative.film = FilmStock::Slide;
        s.negative.enabled = true;
        assert!(params(&s).is_none(), "slides are not inverted");
        let src = Rgb32f::from_fn(8, 8, |x, y| [x as f32 * 0.1, y as f32 * 0.1, 0.3]);
        let mut img = src.clone();
        apply(&mut img, &s);
        assert_eq!(img.data, src.data);
        s.negative.film = FilmStock::Color;
        assert!(converts(&s));
        s.set_section_enabled("negative", false);
        assert!(!converts(&s), "the section's eye button turns it off too");
    }

    #[test]
    fn highlights_roll_off_below_one() {
        let n = Negative { exposure: 2.0, ..negative(FilmStock::Color) };
        let k = NegParams::new(&n).unwrap();
        let mut prev = 0.0;
        for i in 0..40 {
            let t = 0.82 * 10f32.powf(-(i as f32) * 0.08);
            let o = k.convert([t; 3])[0];
            assert!(o >= prev && o <= 1.0, "{i}: {o}");
            prev = o;
        }
        assert!(prev > 0.95, "{prev}");
        // gloss 1 (no shoulder) stays finite
        let k = NegParams::new(&Negative { soft_clip: 1.0, ..n }).unwrap();
        assert!(k.convert([0.001; 3]).iter().all(|v| v.is_finite()));
    }

    #[test]
    fn area_statistics() {
        let px = [[0.1, 0.2, 0.3], [0.3, 0.2, 0.1], [0.2, 0.2, f32::NAN]];
        let a = area_stats(&px, 0.0).unwrap();
        assert_eq!(a.n, 2);
        assert_eq!(a.min, [0.1, 0.2, 0.1]);
        assert_eq!(a.max, [0.3, 0.2, 0.3]);
        assert!((a.mean[0] - 0.2).abs() < 1e-6);
        assert!(area_stats(&[], 0.0).is_none());
        // a tail ignores outliers
        let mut many: Vec<[f32; 3]> = (0..1000).map(|i| [0.1 + i as f32 * 1e-4; 3]).collect();
        many.push([50.0; 3]);
        assert!(area_stats(&many, 0.01).unwrap().max[0] < 0.2);
    }

    #[test]
    fn auto_pickers_recover_the_film() {
        // a film whose densest point prints white and whose thinnest prints black
        let truth = Negative { d_max: 1.8, offset: -0.05, ..negative(FilmStock::Color) };
        let dmin = truth.dmin.to_array();
        let rim = AreaStats { mean: dmin.map(|v| v as f32), min: dmin.map(|v| v as f32), max: dmin.map(|v| v as f32), n: 100 };
        // D-min from the rim
        let picked = auto_dmin(FilmStock::Color, &rim);
        assert!((picked.r - 0.82).abs() < 1e-6 && (picked.g - 0.41).abs() < 1e-6 && (picked.b - 0.19).abs() < 1e-6);
        let bw = auto_dmin(FilmStock::Bw, &rim);
        assert!(bw.r == bw.g && bw.g == bw.b);
        // the image: densities from 0.1 to 1.8 above the base in every channel
        let dense = dmin.map(|d| (d * 10f64.powf(-1.8)) as f32);
        let thin = dmin.map(|d| (d * 10f64.powf(-0.1)) as f32);
        let image = AreaStats { mean: thin, min: dense, max: thin, n: 1000 };
        let start = Negative { dmin: picked, ..Negative { enabled: true, ..Negative::default() } };
        let r = auto_range(&start, &image);
        assert!((r.d_max - 1.8).abs() < 1e-6, "{}", r.d_max);
        // offset = log10(Dmin / max) / Dmax
        assert!((r.offset - 0.1 / 1.8).abs() < 1e-6, "{}", r.offset);
        let p = auto_print(&r, &image);
        let k = NegParams::new(&p).unwrap();
        // the thinnest part prints dark, the densest bright, nothing clips
        let shadow = k.convert(thin);
        let light = k.convert(dense);
        assert!(shadow.iter().all(|v| *v < 0.01), "{shadow:?}");
        assert!(light.iter().all(|v| *v > 0.6 && *v < 1.0), "{light:?}");
        // the print model before gamma lands at negadoctor's targets: 0.1 (black) and 0.96 (white)
        let pl = |t: [f32; 3]| {
            (0..3).map(|c| -(k.exposure * 10f32.powf(k.wb_high[c] * -(k.dmin[c] / t[c]).log10() + k.offset[c]) + k.black)).collect::<Vec<_>>()
        };
        assert!(pl(dense).iter().all(|v| (*v - 0.96).abs() < 1e-3), "{:?}", pl(dense));
        assert!(pl(thin).iter().all(|v| (*v / k.exposure - 0.1).abs() < 1e-3), "{:?}", pl(thin));
        // the results stay in negadoctor's ranges
        let wild = AreaStats { mean: [1e-12; 3], min: [1e-12; 3], max: [1e3; 3], n: 1 };
        let w = auto_print(&auto_range(&start, &wild), &wild);
        assert!((0.1..=6.0).contains(&w.d_max) && (-1.0..=1.0).contains(&w.offset));
        assert!((-0.5..=0.5).contains(&w.black) && (0.5..=2.0).contains(&w.exposure));
        // slides: nothing to estimate
        let slide = Negative { film: FilmStock::Slide, ..start };
        assert_eq!(auto_print(&auto_range(&slide, &image), &image), slide);
    }

    #[test]
    fn sampling_a_rim_area_and_the_image() {
        // left 10 %: the orange film base; the rest: a darker negative image
        let base = [0.8f32, 0.4, 0.2];
        let src = Rgb32f::from_fn(200, 100, |x, _| if x < 20 { base } else { [0.2, 0.08, 0.03] });
        let s = DevelopSettings::default();
        let rim = sample_area(&src, &SourceInfo::default(), &s, Rect::new(0.01, 0.1, 0.08, 0.9)).unwrap();
        assert!(rim.n > 10);
        for c in 0..3 {
            assert!((rim.mean[c] - base[c]).abs() < 1e-3, "{rim:?}");
        }
        // reversed corners and a tiny (click) area work too
        let flipped = sample_area(&src, &SourceInfo::default(), &s, Rect::new(0.08, 0.9, 0.01, 0.1)).unwrap();
        assert_eq!(flipped.n, rim.n);
        let dot = sample_area(&src, &SourceInfo::default(), &s, Rect::new(0.05, 0.5, 0.05, 0.5)).unwrap();
        assert_eq!(dot.n, 1);
        assert!((dot.mean[0] - base[0]).abs() < 1e-3);
        assert!(sample_area(&src, &SourceInfo::default(), &s, Rect::new(2.0, 2.0, 3.0, 3.0)).is_none());
        let all = sample_image(&src, &SourceInfo::default(), &s).unwrap();
        assert!((all.min[0] - 0.2).abs() < 1e-3 && (all.max[0] - 0.8).abs() < 1e-3, "{all:?}");
    }
}
