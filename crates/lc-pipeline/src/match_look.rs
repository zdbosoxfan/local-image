//! Deterministic CPU measure-and-match. References contain finished-image statistics, never
//! Develop settings. A bounded finite-difference descent renders each target through the real
//! pipeline and solves its own editable controls. Capture One's group/Impact UX is the model:
//! https://support.captureone.com/hc/en-us/articles/22188770298269-Match-Look-Tool
//! Solver v2: original bounded LM/IRLS fit in normalized editable-slider space.
//! GPL-3.0-or-later adaptations: darktable exposure.c (percentile/log EV) and colormapping.c
//! (deterministic a/b clusters and Shepard weights), and RawTherapee histmatching.cc
//! (Alberto Griggio, 2018: populated-CDF matching -> sparse curve). See docs/PORTS.md and
//! licenses/{darktable,rawtherapee}-NOTICE.md. No upstream C/C++ is built or shipped.
use crate::{RenderRequest, SourceInfo, StageCache};
use li_color::spline::MonotoneCurve;
use li_color::{REC2020, SRGB, perceptual::oklab_from_2020, transfer::srgb_to_linear};
use lightcraft_develop::{CurveMode, ToneCurve};
use lightcraft_develop::{DevelopSettings, Mask, MaskComponent, MaskShape, Treatment, WbMode, controls};
use lightcraft_geom::Point;
use lightcraft_raster::{Rgb32f, Rgba8};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[cfg(test)]
mod adjustments_tests;
mod appearance;
mod audit_measurement;
mod chroma_statistics;
mod content;
mod descriptor;
mod order_statistics;
#[cfg(test)]
mod quality_tests;
mod regularization;

pub const MAX_REFERENCES: usize = 20;
pub const PROXY_EDGE: usize = 60;
const SOURCE_EDGE: usize = PROXY_EDGE;
pub const FINE_EDGE: usize = 256;
pub const CALIBRATION_EDGE: usize = 512;
const LEGACY_FEATURES: usize = 59;
const FEATURES: usize = LEGACY_FEATURES + appearance::FEATURES;
// Match the existing robust color-statistic floor: sub-code-value texture is not reliable
// evidence. An additive floor keeps flat scenes / 8-bit quantization from dominating fit.
const LOCAL_CONTRAST_FLOOR: f64 = 0.005;
const MASTER_KNOTS: [f64; 5] = [0.1, 0.25, 0.5, 0.75, 0.95];
const PALETTE_PRIOR: f64 = 0.00025;
const PALETTE_CONTROLS: [&str; 8] = [
    "mixer.red.sat",
    "mixer.orange.sat",
    "mixer.yellow.sat",
    "mixer.green.sat",
    "mixer.aqua.sat",
    "mixer.blue.sat",
    "mixer.purple.sat",
    "mixer.magenta.sat",
];

/// Individual editable tools, subordinate to the four group switches. Light maps to
/// this editor's endpoint/contrast sliders and luminance curve; there is no invented
/// Brightness or Levels setting. Balance maps to the four editable grading wheels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Adjustments {
    pub exposure: bool,
    pub white_balance: bool,
    pub contrast: bool,
    pub highlights: bool,
    pub shadows: bool,
    pub whites: bool,
    pub blacks: bool,
    pub luma_curve: bool,
    pub saturation: bool,
    pub vibrance: bool,
    pub color_editor: bool,
    pub black_white: bool,
    pub master_balance: bool,
    pub shadows_balance: bool,
    pub midtones_balance: bool,
    pub highlights_balance: bool,
    pub red_curve: bool,
    pub green_curve: bool,
    pub blue_curve: bool,
}
impl Default for Adjustments {
    fn default() -> Self {
        Self {
            exposure: true,
            white_balance: true,
            contrast: true,
            highlights: true,
            shadows: true,
            whites: true,
            blacks: true,
            luma_curve: true,
            saturation: true,
            vibrance: true,
            color_editor: true,
            black_white: true,
            master_balance: true,
            shadows_balance: true,
            midtones_balance: true,
            highlights_balance: true,
            red_curve: true,
            green_curve: true,
            blue_curve: true,
        }
    }
}
/// Worker-local insertion point for replacing an app-owned Match Look layer.
/// Skipped in Options serialization; only the engine can restore catalog ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerPlacement {
    pub index: usize,
    pub id: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    #[serde(skip)]
    pub layer_placement: Option<LayerPlacement>,
    pub adjustments: Adjustments,
    pub normalize: bool,
    pub light_contrast: bool,
    pub color_adjust: bool,
    pub color_tone: bool,
    pub strength: f64,
    pub new_layer: bool,
    pub consistent_batch: bool,
    pub protect_skin: bool,
    pub keep_white_balance: bool,
    pub keep_exposure: bool,
    pub contrast_guard: bool,
    pub region_aware: bool,
    pub appearance_aware: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            layer_placement: None,
            adjustments: Adjustments::default(),
            normalize: true,
            light_contrast: true,
            color_adjust: true,
            color_tone: true,
            strength: 100.0,
            new_layer: false,
            consistent_batch: false,
            protect_skin: true,
            keep_white_balance: false,
            keep_exposure: false,
            contrast_guard: true,
            region_aware: false,
            appearance_aware: false,
        }
    }
}

impl Options {
    pub fn exposure_enabled(self) -> bool {
        self.normalize && self.adjustments.exposure && !self.keep_exposure
    }
    pub fn white_balance_enabled(self) -> bool {
        self.normalize && self.adjustments.white_balance && !self.keep_white_balance
    }
    pub fn light_enabled(self) -> bool {
        self.light_contrast
            && (self.adjustments.contrast
                || self.adjustments.highlights
                || self.adjustments.shadows
                || self.adjustments.whites
                || self.adjustments.blacks
                || self.adjustments.luma_curve)
    }
    pub fn color_enabled(self) -> bool {
        self.color_adjust
            && (self.adjustments.saturation || self.adjustments.vibrance || self.adjustments.color_editor || self.adjustments.black_white)
    }
    pub fn tone_enabled(self) -> bool {
        self.color_tone
            && (self.adjustments.master_balance
                || self.adjustments.shadows_balance
                || self.adjustments.midtones_balance
                || self.adjustments.highlights_balance
                || self.adjustments.red_curve
                || self.adjustments.green_curve
                || self.adjustments.blue_curve)
    }
    fn effective(mut self) -> Self {
        self.normalize = self.exposure_enabled() || self.white_balance_enabled();
        self.light_contrast = self.light_enabled();
        self.color_adjust = self.color_enabled();
        self.color_tone = self.tone_enabled();
        self
    }
    fn channel_enabled(self, channel: usize) -> bool {
        self.color_tone && [self.adjustments.red_curve, self.adjustments.green_curve, self.adjustments.blue_curve][channel]
    }
    fn wheel_enabled(self, band: &str) -> bool {
        self.color_tone
            && match band {
                "global" => self.adjustments.master_balance,
                "shadows" => self.adjustments.shadows_balance,
                "midtones" => self.adjustments.midtones_balance,
                "highlights" => self.adjustments.highlights_balance,
                _ => false,
            }
    }
}
/// Restore every excluded tool from the editable starting state, including shared
/// style copies and curve guards. For layers the starting state is neutral/relative.
fn preserve_excluded(start: &DevelopSettings, edited: &mut DevelopSettings, opts: Options) {
    if !opts.exposure_enabled() {
        edited.light.exposure = start.light.exposure;
    }
    if !opts.white_balance_enabled() {
        edited.wb = start.wb;
    }
    for (name, on) in [
        ("contrast", opts.adjustments.contrast),
        ("highlights", opts.adjustments.highlights),
        ("shadows", opts.adjustments.shadows),
        ("whites", opts.adjustments.whites),
        ("blacks", opts.adjustments.blacks),
    ] {
        if !opts.light_contrast || !on {
            let id = format!("light.{name}");
            controls::set(edited, &id, controls::get(start, &id).unwrap());
        }
    }
    if !opts.light_contrast || !opts.adjustments.luma_curve {
        restore_light_curve(&mut edited.curve, &start.curve);
    }
    if !opts.color_adjust || !opts.adjustments.saturation {
        edited.color.saturation = start.color.saturation;
    }
    if !opts.color_adjust || !opts.adjustments.vibrance {
        edited.color.vibrance = start.color.vibrance;
    }
    if !opts.color_adjust || !opts.adjustments.color_editor {
        edited.mixer = start.mixer;
    }
    if !opts.color_adjust || !opts.adjustments.black_white {
        edited.treatment = start.treatment;
    }
    for c in 0..3 {
        if !opts.channel_enabled(c) {
            *channel_mut(&mut edited.curve, c) = channel(&start.curve, c).clone();
        }
    }
    if !opts.wheel_enabled("global") {
        edited.grading.global = start.grading.global;
    }
    if !opts.wheel_enabled("shadows") {
        edited.grading.shadows = start.grading.shadows;
    }
    if !opts.wheel_enabled("midtones") {
        edited.grading.midtones = start.grading.midtones;
    }
    if !opts.wheel_enabled("highlights") {
        edited.grading.highlights = start.grading.highlights;
    }
    // These unselected wheel-combination controls are never fit by Match Look.
    edited.grading.blending = start.grading.blending;
    edited.grading.balance = start.grading.balance;
}

/// OkLab lightness quantiles, neutral a/b, chroma, three tonal a/b pairs, and skin L/a/b.
/// Missing neutral/skin pixels carry no objective weight. Signature is hue occupancy plus tone
/// shape, measured on the unedited source, so creative grading doesn't select its own reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Look {
    #[serde(with = "feature_array")]
    pub features: [f64; FEATURES],
    #[serde(with = "feature_array")]
    pub support: [f64; FEATURES],
    pub signature: [f64; 10],
    #[serde(default)]
    pub content: Vec<f64>,
    #[serde(default)]
    pub hue_distribution: [f64; 8],
    #[serde(default)]
    pub clusters: Vec<ColorCluster>,
    #[serde(default = "reference_importance")]
    pub importance: f64,
    /// Finished references explicitly rendered as B&W remain monochrome even with split toning.
    #[serde(default)]
    pub monochrome: bool,
    #[serde(default)]
    pub band_mass: [f64; 8],
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ColorCluster {
    pub ab: [f64; 2],
    pub weight: f64,
}
fn reference_importance() -> f64 {
    1.0
}
// Preserve existing v1 saved looks (the two added global color moments default unsupported).
mod feature_array {
    use super::FEATURES;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[f64; FEATURES], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[f64; FEATURES], D::Error> {
        let values = Vec::<f64>::deserialize(d)?;
        if values.len() != 25
            && values.len() != 27
            && values.len() != 30
            && values.len() != 33
            && values.len() != 41
            && values.len() != 49
            && values.len() != 59
            && values.len() != FEATURES
        {
            return Err(serde::de::Error::custom("Invalid look feature count"));
        }
        let mut out = [0.0; FEATURES];
        out[..values.len()].copy_from_slice(&values);
        Ok(out)
    }
}
impl Look {
    pub fn valid(&self) -> bool {
        self.importance.is_finite()
            && (self.content.is_empty() || self.content.len() == 64)
            && self.content.iter().all(|v| v.is_finite() && (-1.0..=1.0).contains(v))
            && (1.0..=2.0).contains(&self.importance)
            && self.features.iter().chain(&self.support).chain(&self.signature).chain(&self.hue_distribution).all(|v| v.is_finite())
            && self.clusters.len() <= 3
            && self
                .clusters
                .iter()
                .all(|c| c.ab.iter().all(|x| x.is_finite() && (-0.5..=0.5).contains(x)) && c.weight.is_finite() && (0.0..=1.0).contains(&c.weight))
            && self.features[..5].windows(2).all(|p| p[0] <= p[1] + 1e-8)
            && self.support.iter().all(|v| *v >= 0.0 && *v <= 1.0)
            && self.features.iter().enumerate().all(|(i, v)| {
                if i >= LEGACY_FEATURES {
                    return appearance::valid_value(i - LEGACY_FEATURES, *v);
                }
                if [0, 1, 2, 3, 4, 7, 14].contains(&i) || (17..25).contains(&i) || (27..=32).contains(&i) {
                    (-0.001..=1.01).contains(v)
                } else if i == 49 {
                    (0.0..=10.0).contains(v)
                } else if (41..49).contains(&i) || i >= 50 && (i - 50) % 3 == 0 {
                    (-1.01..=1.01).contains(v)
                } else {
                    (-0.5..=0.5).contains(v)
                }
            })
            && self.hue_distribution.iter().chain(&self.band_mass).all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self.signature.iter().enumerate().all(|(i, v)| if i < 9 { (-0.001..=1.01).contains(v) } else { (-1.01..=1.01).contains(v) })
    }
}

pub fn measure(image: &Rgba8) -> Look {
    measure_with_faces(image, &[])
}

/// Internal near-match audit statistics, not reference analysis or saved Looks.
/// Legacy fields stay complete. Disabled appearance axes have zero audit weight;
/// enabling appearance preserves the full public measurement exactly.
pub fn measure_for_audit(image: &Rgba8, opts: Options) -> Look {
    audit_measurement::measure(image, opts)
}
fn measure_with_faces(image: &Rgba8, faces: &[[f32; 4]]) -> Look {
    measure_impl(image, faces, true)
}
// Exact byte-domain decode cache; probe measurements do not need scene descriptors / clusters.
static SRGB_DECODE: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| std::array::from_fn(|i| srgb_to_linear(i as f32 / 255.0)));
fn measure_impl(image: &Rgba8, faces: &[[f32; 4]], descriptor: bool) -> Look {
    measure_impl_with_membership(image, faces, descriptor, None)
}
fn measure_impl_with_membership(image: &Rgba8, faces: &[[f32; 4]], descriptor: bool, membership: Option<&appearance::Membership>) -> Look {
    let convert = SRGB.to_space(&REC2020);
    measure_samples(
        image.width,
        image.height,
        image.data.iter().map(|p| oklab_from_2020(convert.apply_f32(std::array::from_fn(|i| SRGB_DECODE[p[i] as usize]))).map(f64::from)),
        faces,
        descriptor,
        membership,
    )
}
fn measure_rendered(image: &crate::Rendered, faces: &[[f32; 4]], descriptor: bool) -> Look {
    match &image.deep {
        Some(deep) => measure_deep_impl(deep, faces, descriptor),
        None => measure_impl(&image.image, faces, descriptor),
    }
}
fn rendered_membership(image: &crate::Rendered) -> appearance::Membership {
    let labs: Vec<[f64; 3]> = if let Some(deep) = &image.deep {
        let convert = deep.space.rgb_space().to_space(&REC2020);
        match &deep.samples {
            crate::DeepSamples::F32(pixels) => {
                pixels.as_chunks::<3>().0.iter().map(|p| oklab_from_2020(convert.apply_f32(*p)).map(f64::from)).collect()
            }
            crate::DeepSamples::U16(pixels) => pixels
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| oklab_from_2020(convert.apply_f32(p.map(|c| deep.space.trc().decode(c as f32 / 65535.0)))).map(f64::from))
                .collect(),
        }
    } else {
        let convert = SRGB.to_space(&REC2020);
        image
            .image
            .data
            .iter()
            .map(|p| oklab_from_2020(convert.apply_f32(std::array::from_fn(|i| SRGB_DECODE[p[i] as usize]))).map(f64::from))
            .collect()
    };
    appearance::memberships_with_dimensions(&labs, image.image.width, image.image.height)
}
fn measure_rendered_with_membership(image: &crate::Rendered, faces: &[[f32; 4]], membership: Option<&appearance::Membership>) -> Look {
    match &image.deep {
        Some(deep) => measure_deep_impl_with_membership(deep, faces, false, membership),
        None => measure_impl_with_membership(&image.image, faces, false, membership),
    }
}
/// Measure linear float output without quantizing finite-difference probes to eight bits.
pub fn measure_deep(image: &crate::DeepImage) -> Look {
    measure_deep_impl(image, &[], true)
}
fn measure_deep_impl(image: &crate::DeepImage, faces: &[[f32; 4]], descriptor: bool) -> Look {
    measure_deep_impl_with_membership(image, faces, descriptor, None)
}
fn measure_deep_impl_with_membership(
    image: &crate::DeepImage,
    faces: &[[f32; 4]],
    descriptor: bool,
    membership: Option<&appearance::Membership>,
) -> Look {
    let convert = image.space.rgb_space().to_space(&REC2020);
    match &image.samples {
        crate::DeepSamples::F32(pixels) => measure_samples(
            image.width,
            image.height,
            pixels.as_chunks::<3>().0.iter().map(|p| oklab_from_2020(convert.apply_f32(*p)).map(f64::from)),
            faces,
            descriptor,
            membership,
        ),
        crate::DeepSamples::U16(pixels) => measure_samples(
            image.width,
            image.height,
            pixels
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| oklab_from_2020(convert.apply_f32(p.map(|c| image.space.trc().decode(c as f32 / 65535.0)))).map(f64::from)),
            faces,
            descriptor,
            membership,
        ),
    }
}
fn measure_samples(
    width: usize,
    height: usize,
    pixels: impl Iterator<Item = [f64; 3]>,
    faces: &[[f32; 4]],
    descriptor: bool,
    membership: Option<&appearance::Membership>,
) -> Look {
    measure_samples_with_quantiles(width, height, pixels, faces, descriptor, membership, order_statistics::luminance)
}
#[allow(clippy::too_many_arguments)]
fn measure_samples_with_quantiles(
    width: usize,
    height: usize,
    pixels: impl Iterator<Item = [f64; 3]>,
    faces: &[[f32; 4]],
    descriptor: bool,
    membership: Option<&appearance::Membership>,
    quantiles: impl FnOnce(&mut [f64]) -> [f64; 7],
) -> Look {
    measure_samples_with_reductions::<true>(width, height, pixels, faces, descriptor, membership, quantiles, chroma_statistics::moments)
}
#[allow(clippy::too_many_arguments)]
fn measure_samples_with_reductions<const REUSE_CHROMA: bool>(
    width: usize,
    height: usize,
    pixels: impl Iterator<Item = [f64; 3]>,
    faces: &[[f32; 4]],
    descriptor: bool,
    membership: Option<&appearance::Membership>,
    quantiles: impl FnOnce(&mut [f64]) -> [f64; 7],
    chroma_moments: impl FnOnce(&[[f64; 3]], &[f64]) -> (f64, f64),
) -> Look {
    let mut l = Vec::with_capacity(width * height);
    let mut sums = [0.0; FEATURES];
    let mut mass = [0.0; FEATURES];
    let mut hue = [0.0; 8];
    let mut hue_vector = [[0.0; 2]; 8];
    let centers = crate::colorops::band_hues().map(|h| f64::from(h).sin_cos());
    let mut labs = Vec::with_capacity(width * height);
    // Solver measurements are at most 512² pixels: at most 2 MiB of temporary
    // scratch, released with this measurement and never retained across photos.
    let mut chromas = Vec::with_capacity(if REUSE_CHROMA { width * height } else { 0 });
    for (index, lab) in pixels.enumerate() {
        let [ll, a, b] = lab;
        l.push(ll);
        labs.push(lab);
        sums[25] += a;
        sums[26] += b;
        mass[25] += 1.0;
        mass[26] += 1.0;
        let c = a.hypot(b);
        if REUSE_CHROMA {
            chromas.push(c);
        }
        let neutral = (1.0 - c / 0.08).clamp(0.0, 1.0).powi(2) * f64::from(ll > 0.15 && ll < 0.9);
        for (i, v, w) in [(5, a, neutral), (6, b, neutral), (7, c, 1.0)] {
            sums[i] += v * w;
            mass[i] += w;
        }
        let band = if ll < 0.4 {
            0
        } else if ll < 0.7 {
            1
        } else {
            2
        };
        for (i, v) in [(8 + band * 2, a), (9 + band * 2, b)] {
            sums[i] += v;
            mass[i] += 1.0;
        }
        // Conservative chromatic skin window. No face model is loaded or required; observations
        // outside the window have zero weight, so blue skies can't become a skin target.
        let in_face = faces.is_empty()
            || faces.iter().any(|r| {
                let x = (index % width.max(1)) as f32 / width.max(1) as f32;
                let y = (index / width.max(1)) as f32 / height.max(1) as f32;
                x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]
            });
        let skin = if faces.is_empty() {
            in_face && ll > 0.12 && ll < 0.9 && a > 0.008 && b > 0.008 && a < 0.16 && b < 0.16 && c < 0.2
        } else {
            in_face && ll > 0.08 && ll < 0.94 && c < 0.25
        };
        if skin {
            for (i, v) in lab.into_iter().enumerate() {
                sums[14 + i] += v;
                mass[14 + i] += 1.0;
            }
        }
        if c > 0.02 {
            let angle = b.atan2(a);
            for (band, weight) in crate::colorops::band_weights(angle as f32).iter().enumerate() {
                let weight = f64::from(*weight);
                if weight == 0.0 {
                    continue;
                }
                sums[17 + band] += c * weight;
                mass[17 + band] += weight;
                sums[41 + band] += ll * c * weight;
                mass[41 + band] += c * weight;
                let (sin, cos) = centers[band];
                hue_vector[band][0] += (a * cos + b * sin) * weight;
                hue_vector[band][1] += (b * cos - a * sin) * weight;
            }
            if descriptor {
                let h = angle.rem_euclid(std::f64::consts::TAU);
                hue[((h / std::f64::consts::TAU * 8.0) as usize).min(7)] += c;
            }
        }
    }
    let (_, chroma_variation) = chroma_moments(&labs, &chromas);
    let quantiles = quantiles(&mut l);
    let mut features = [0.0; FEATURES];
    let mut support = [0.0; FEATURES];
    if !l.is_empty() {
        for (i, value) in quantiles[1..6].iter().enumerate() {
            features[i] = *value;
            support[i] = 1.0;
        }
        for i in 5..FEATURES {
            if mass[i] > 0.0 {
                features[i] = sums[i] / mass[i];
                support[i] = (mass[i] / l.len() as f64 * 20.0).min(1.0);
            }
        }
    }
    if !l.is_empty() {
        for (i, value) in [(30, quantiles[0]), (31, quantiles[6])] {
            features[i] = value;
            support[i] = 1.0;
        }
        // Summed-area box mean: same clipped 5×5 support, linear work in pixel count.
        let stride = width + 1;
        let mut integral = vec![0.0; (height + 1) * stride];
        for y in 0..height {
            let mut row = 0.0;
            for x in 0..width {
                row += labs[y * width + x][0];
                integral[(y + 1) * stride + x + 1] = integral[y * stride + x + 1] + row;
            }
        }
        let mut local = 0.0;
        for y in 0..height {
            for x in 0..width {
                let (x0, x1, y0, y1) = (x.saturating_sub(2), (x + 3).min(width), y.saturating_sub(2), (y + 3).min(height));
                let sum = integral[y1 * stride + x1] - integral[y0 * stride + x1] - integral[y1 * stride + x0] + integral[y0 * stride + x0];
                local += (labs[y * width + x][0] - sum / ((x1 - x0) * (y1 - y0)) as f64).abs();
            }
        }
        features[32] = local / l.len() as f64;
        support[32] = 1.0;
        for band in 0..8 {
            features[33 + band] = hue_vector[band][1].atan2(hue_vector[band][0]) / std::f64::consts::TAU;
            support[33 + band] = support[17 + band];
            features[41 + band] -= features[2];
        }
    }
    features[49] = chroma_variation.min(10.0);
    support[49] = f64::from(!labs.is_empty());
    // Relative luminance zones remove exposure/content-area bias. Robust Shepard weights
    // around each zone's neutral core keep an isolated saturated object from defining its cast.
    let span = (features[4] - features[0]).max(0.05);
    let mut zone_sum = [[0.0; 2]; 3];
    let mut zone_chroma = [0.0; 3];
    let mut zone_mass = [0.0; 3];
    for [ll, a, b] in &labs {
        let z = ((*ll - features[0]) / span).clamp(0.0, 1.0);
        let chroma = a.hypot(*b);
        for band in 0..3 {
            let w = (1.0 - (z - [0.0, 0.5, 1.0][band]).abs() * 2.0).max(0.0);
            let w = w / (1.0 + (a * a + b * b) / 0.04);
            zone_sum[band][0] += a * w;
            zone_sum[band][1] += b * w;
            zone_chroma[band] += chroma * w;
            zone_mass[band] += w;
        }
    }
    for band in 0..3 {
        if zone_mass[band] > 0.0 {
            features[27 + band] = zone_chroma[band] / zone_mass[band];
            support[27 + band] = (zone_mass[band] / labs.len().max(1) as f64 * 20.0).min(1.0);
            for axis in 0..2 {
                features[8 + band * 2 + axis] = zone_sum[band][axis] / zone_mass[band];
                support[8 + band * 2 + axis] = (zone_mass[band] / labs.len().max(1) as f64 * 20.0).min(1.0);
            }
        }
    }
    let own_membership;
    let membership = match membership {
        Some(m) => m,
        None => {
            own_membership = appearance::memberships_with_dimensions(&labs, width, height);
            &own_membership
        }
    };
    let (conditional, confidence) = appearance::measure(&labs, membership);
    features[LEGACY_FEATURES..].copy_from_slice(&conditional);
    support[LEGACY_FEATURES..].copy_from_slice(&confidence);
    let clusters = if descriptor { color_clusters(&labs) } else { Vec::new() };
    let total = hue.iter().sum::<f64>();
    let mut signature = [0.0; 10];
    for i in 0..8 {
        signature[i] = hue[i] / total.max(1e-9);
    }
    signature[8] = features[4] - features[0];
    signature[9] = (features[3] - features[2]) - (features[2] - features[1]);
    Look {
        features,
        support,
        hue_distribution: std::array::from_fn(|i| signature[i]),
        content: if descriptor { content::fingerprint(&labs, width, height) } else { Vec::new() },
        signature,
        clusters,
        importance: 1.0,
        monochrome: false,
        band_mass: std::array::from_fn(|i| mass[17 + i] / labs.len().max(1) as f64),
    }
}

/// Interpolated weighted median (sample-center CDF), with 3-MAD outlier rejection.
/// Two equally weighted references have their midpoint, avoiding an arbitrary tie bias.
fn weighted_median(samples: &[(f64, f64)]) -> f64 {
    weighted_median_floor(samples, 0.005)
}
fn weighted_median_floor(samples: &[(f64, f64)], floor: f64) -> f64 {
    let mut v: Vec<_> = samples.iter().copied().filter(|(x, w)| x.is_finite() && *w > 0.0).collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    if v.is_empty() {
        return 0.0;
    }
    let median = |v: &[(f64, f64)]| {
        let total = v.iter().map(|x| x.1).sum::<f64>();
        let mut sum = 0.0;
        let mut previous = (v[0].0, 0.0);
        for &(x, w) in v {
            let position = (sum + w * 0.5) / total;
            if position >= 0.5 {
                let t = ((0.5 - previous.1) / (position - previous.1).max(1e-12)).clamp(0.0, 1.0);
                return previous.0 + (x - previous.0) * t;
            }
            previous = (x, position);
            sum += w;
        }
        v.last().map_or(0.0, |v| v.0)
    };
    let m = median(&v);
    if v.len() < 4 {
        return m;
    }
    let mut deviations: Vec<_> = v.iter().map(|(x, w)| ((x - m).abs(), *w)).collect();
    deviations.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mad = median(&deviations).max(floor);
    v.retain(|(x, _)| (x - m).abs() <= 3.0 * mad);
    median(&v)
}
/// Adaptation of darktable kmeans/get_clusters: deterministic zone initializers, soft Shepard
/// assignment (power 2), and a small fixed iteration budget; no random or platform ordering.
fn color_clusters(labs: &[[f64; 3]]) -> Vec<ColorCluster> {
    if labs.is_empty() {
        return Vec::new();
    }
    let mut centers = [[0.0; 2]; 3];
    for (i, c) in centers.iter_mut().enumerate() {
        let p = labs[(labs.len() - 1) * i / 2];
        *c = [p[1], p[2]];
    }
    let stride = (labs.len() / 128).max(1);
    let mut mass = [0.0; 3];
    for _ in 0..3 {
        let mut sum = [[0.0; 2]; 3];
        mass = [0.0; 3];
        for p in labs.iter().step_by(stride) {
            let weights = centers.map(|c| 1.0 / ((p[1] - c[0]).powi(2) + (p[2] - c[1]).powi(2) + 0.0001));
            let total = weights.iter().sum::<f64>();
            for i in 0..3 {
                let w = weights[i] / total;
                mass[i] += w;
                sum[i][0] += p[1] * w;
                sum[i][1] += p[2] * w;
            }
        }
        for i in 0..3 {
            if mass[i] > 0.0 {
                centers[i] = sum[i].map(|x| x / mass[i]);
            }
        }
    }
    let total = mass.iter().sum::<f64>().max(1e-9);
    (0..3).map(|i| ColorCluster { ab: centers[i], weight: mass[i] / total }).collect()
}
/// Sparse equal-CDF quantile map, excluding unpopulated/clipped tails and bounding slope.
/// Port/adaptation of RawTherapee getCdf/findMatch/mappingToCurve to stored quantile knots.
pub fn cdf_curve(source: &Look, goal: &Look) -> [f64; 5] {
    let x = &source.features;
    let mut y = std::array::from_fn(|i| goal.features[i]);
    for i in (0..2).rev() {
        let gap = x[i + 1] - x[i];
        if gap > 0.002 {
            y[i] = y[i].clamp(y[i + 1] - 2.0 * gap, y[i + 1] - 0.5 * gap);
        } else {
            y[i] = y[i + 1] - gap;
        }
    }
    for i in 3..5 {
        let gap = x[i] - x[i - 1];
        if gap > 0.002 {
            y[i] = y[i].clamp(y[i - 1] + 0.5 * gap, y[i - 1] + 2.0 * gap);
        } else {
            y[i] = y[i - 1] + gap;
        }
    }
    y
}

// The pipeline intentionally builds tone guides from source geometry (up to 2 MP), even for
// small requested outputs. Bound the source too, so a solver probe stays small throughout.
fn measurement_source(src: &Arc<Rgb32f>, info: &SourceInfo, edge: usize) -> (Arc<Rgb32f>, SourceInfo) {
    if src.width.max(src.height) <= edge {
        return (src.clone(), info.clone());
    }
    let image = lightcraft_raster::resample::fit(src, edge, edge, lightcraft_raster::resample::Filter::Box);
    let mut info = info.clone();
    info.sensor_scale *= src.width.max(src.height) as f32 / image.width.max(image.height).max(1) as f32;
    (Arc::new(image), info)
}

fn framed_faces(src: &Rgb32f, info: &SourceInfo, settings: &DevelopSettings, faces: &[[f32; 4]]) -> Vec<[f32; 4]> {
    use lightcraft_geom::{Point, Rect};
    if faces.is_empty() {
        return Vec::new();
    }
    let frame = crate::frame_for(src, info, settings, true);
    let transform = frame.norm_to_out(1, 1);
    faces
        .iter()
        .filter_map(|r| {
            let oriented = settings.orientation.map_norm_rect(Rect::from_xywh(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64));
            let mut min = Point::new(1.0, 1.0);
            let mut max = Point::new(0.0, 0.0);
            for p in oriented.corners() {
                // Invert the existing green-plane sampling warp at each corner (including lens /
                // perspective), then apply the same crop/straighten/flips as the rendered photo.
                let mut q = Point::new(p.x * frame.ow, p.y * frame.oh);
                if let Some(warp) = &frame.warp {
                    let source = q;
                    for _ in 0..6 {
                        let f = warp.to_source(q, 1);
                        let x = warp.to_source(Point::new(q.x + 1.0, q.y), 1);
                        let y = warp.to_source(Point::new(q.x, q.y + 1.0), 1);
                        let (a, b, c, d) = (x.x - f.x, y.x - f.x, x.y - f.y, y.y - f.y);
                        let det = a * d - b * c;
                        if det.abs() < 1e-8 {
                            break;
                        }
                        let (rx, ry) = (f.x - source.x, f.y - source.y);
                        q.x -= (d * rx - b * ry) / det;
                        q.y -= (-c * rx + a * ry) / det;
                    }
                }
                let p = transform.apply(Point::new(q.x / frame.ow, q.y / frame.oh));
                min.x = min.x.min(p.x);
                min.y = min.y.min(p.y);
                max.x = max.x.max(p.x);
                max.y = max.y.max(p.y);
            }
            let x = min.x.clamp(0.0, 1.0);
            let y = min.y.clamp(0.0, 1.0);
            let w = (max.x.clamp(0.0, 1.0) - x).max(0.0);
            let h = (max.y.clamp(0.0, 1.0) - y).max(0.0);
            (w > 0.001 && h > 0.001).then_some([x as f32, y as f32, w as f32, h as f32])
        })
        .collect()
}
pub fn analyze(src: &Arc<Rgb32f>, info: &SourceInfo, settings: &DevelopSettings) -> Look {
    analyze_with_faces(src, info, settings, &[])
}
pub fn analyze_with_faces(src: &Arc<Rgb32f>, info: &SourceInfo, settings: &DevelopSettings, faces: &[[f32; 4]]) -> Look {
    let faces = framed_faces(src, info, settings, faces);
    let (signature_src, signature_info) = measurement_source(src, info, 128);
    let (src, info) = measurement_source(src, info, CALIBRATION_EDGE);
    let edge = CALIBRATION_EDGE.min(src.width.max(src.height));
    let mut req = RenderRequest::fit(edge, edge);
    req.depth = crate::OutputDepth::F32Linear;
    let rendered = crate::render(&src, &info, settings, &req);
    let masks = region_masks(&rendered, &src, &info, settings, &faces);
    let mut look = measure_rendered(&rendered, &faces, true);
    measure_regions(&mut look, &rendered, &masks);
    look.monochrome = settings.treatment == Treatment::Bw;
    let ungraded = measure(&crate::render(&signature_src, &signature_info, &DevelopSettings::default(), &RenderRequest::fit(128, 128)).image);
    look.signature = ungraded.signature;
    look.content = ungraded.content;
    look
}

/// Reuse the pipeline's quick Sky/Subject selection (including stored model masks) and cached
/// face rectangles. This optional prototype requires support on both images; no model is loaded.
fn region_masks(
    image: &crate::Rendered,
    src: &Rgb32f,
    info: &SourceInfo,
    settings: &DevelopSettings,
    faces: &[[f32; 4]],
) -> [lightcraft_raster::Plane; 3] {
    let convert = SRGB.to_space(&REC2020);
    let rgb = Rgb32f::from_fn(image.image.width, image.image.height, |x, y| {
        let p = image.image.get(x, y);
        convert.apply_f32(std::array::from_fn(|i| SRGB_DECODE[p[i] as usize]))
    });
    let log = lightcraft_raster::Plane::from_fn(rgb.width, rgb.height, |x, y| {
        let p = rgb.get(x, y);
        li_color::luminance_2020(p).max(1e-6).log2()
    });
    let frame = crate::frame_for(src, info, settings, true);
    let stored = |sky: bool| {
        settings.masks.iter().flat_map(|m| &m.components).find_map(|c| match &c.shape {
            MaskShape::Sky { seg: Some(_) } if sky => Some(c.shape.clone()),
            MaskShape::Subject { seg: Some(_) } if !sky => Some(c.shape.clone()),
            _ => None,
        })
    };
    let sky = crate::masks::shape_alpha(&stored(true).unwrap_or(MaskShape::Sky { seg: None }), &frame, rgb.width, rgb.height, &rgb, &log, 0.0);
    let mut subject =
        crate::masks::shape_alpha(&stored(false).unwrap_or(MaskShape::Subject { seg: None }), &frame, rgb.width, rgb.height, &rgb, &log, 0.0);
    for (i, p) in subject.data.iter_mut().enumerate() {
        let x = (i % rgb.width) as f32 / rgb.width as f32;
        let y = (i / rgb.width) as f32 / rgb.height as f32;
        if faces.iter().any(|r| x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]) {
            *p = 1.0;
        }
        *p *= 1.0 - sky.data[i];
    }
    let rest = lightcraft_raster::Plane::from_fn(rgb.width, rgb.height, |x, y| (1.0 - sky.get(x, y) - subject.get(x, y)).max(0.0));
    [sky, subject, rest]
}
fn measure_regions(look: &mut Look, image: &crate::Rendered, masks: &[lightcraft_raster::Plane; 3]) {
    let convert = SRGB.to_space(&REC2020);
    let labs = image
        .image
        .data
        .iter()
        .map(|p| oklab_from_2020(convert.apply_f32(std::array::from_fn(|i| SRGB_DECODE[p[i] as usize]))).map(f64::from))
        .collect::<Vec<_>>();
    for (region, mask) in masks.iter().enumerate() {
        let total = mask.data.iter().map(|v| f64::from(*v)).sum::<f64>();
        if total < labs.len() as f64 * 0.05 {
            continue;
        }
        for axis in 0..3 {
            let i = 50 + region * 3 + axis;
            look.features[i] = labs.iter().zip(&mask.data).map(|(p, w)| p[axis] * f64::from(*w)).sum::<f64>() / total
                - if axis == 0 { look.features[2] } else { 0.0 };
            look.support[i] = (total / labs.len().max(1) as f64).min(1.0);
        }
    }
}

/// Normalized soft similarity weights; every reference contributes, even with dissimilar scenes.
pub fn weights(references: &[Look], signature: &[f64; 10], consistent: bool) -> Vec<f64> {
    weights_with_content(references, signature, &[], consistent)
}
fn weights_with_content(references: &[Look], signature: &[f64; 10], layout: &[f64], consistent: bool) -> Vec<f64> {
    let mut w: Vec<f64> = references
        .iter()
        .map(|r| {
            r.importance
                * if consistent {
                    1.0
                } else {
                    let distance: f64 = r.signature.iter().zip(signature).map(|(a, b)| (a - b).powi(2)).sum();
                    (-8.0 * distance).exp().max(0.001) * (1.0 + 8.0 * content::confidence(&r.content, layout))
                }
        })
        .collect();
    let sum = w.iter().sum::<f64>().max(1e-9);
    for v in &mut w {
        *v /= sum;
    }
    w
}
pub fn blend(references: &[Look], w: &[f64]) -> Look {
    let mut out = Look {
        features: [0.0; FEATURES],
        support: [0.0; FEATURES],
        signature: [0.0; 10],
        content: Vec::new(),
        hue_distribution: [0.0; 8],
        clusters: Vec::new(),
        importance: 1.0,
        monochrome: references.iter().all(is_monochrome),
        band_mass: std::array::from_fn(|i| references.iter().zip(w).map(|(r, w)| r.band_mass[i] * w).sum()),
    };
    for i in 0..FEATURES {
        let samples: Vec<_> = references.iter().zip(w).filter(|(r, _)| r.support[i] > 0.0).map(|(r, w)| (r.features[i], w * r.support[i])).collect();
        out.features[i] = weighted_median_floor(&samples, if i < 5 || i == 14 { 0.03 } else { 0.005 });
        out.support[i] = references.iter().zip(w).map(|(r, w)| r.support[i] * w).sum();
    }
    for i in 0..10 {
        out.signature[i] = references.iter().zip(w).map(|(r, w)| r.signature[i] * w).sum();
    }
    for i in 0..8 {
        out.hue_distribution[i] = references.iter().zip(w).map(|(r, w)| r.hue_distribution[i] * w).sum();
    }
    // Prototypes remain per-reference in saved looks; choose the strongest reference's
    // prototypes here rather than averaging unrelated hues into a fictitious cluster.
    if let Some((r, _)) = references.iter().zip(w).max_by(|a, b| a.1.total_cmp(b.1)) {
        out.clusters = r.clusters.clone();
        out.content = r.content.clone();
    }
    out
}
/// Adapt scene brightness and keep the full rendered tone objective. Exposure, HDR and
/// tone controls are not limited to an incremental point curve's slope cone. The returned
/// legacy reach field is only curve comfort, not physical attainability of the full pipeline.
pub fn reachable_goal(initial: &Look, reference: &Look, opts: Options, similarity: f64) -> (Look, f64) {
    let opts = opts.effective();
    let mut adapted = reference.clone();
    let s = if opts.consistent_batch { 1.0 } else { similarity.sqrt().clamp(0.35, 1.0) };
    let median = initial.features[2] + (reference.features[2] - initial.features[2]) * s;
    for i in [0, 1, 2, 3, 4, 14, 30, 31] {
        adapted.features[i] += median - reference.features[2];
    }
    let mut reachable = adapted.clone();
    reachable.features[..5].copy_from_slice(&cdf_curve(initial, &adapted));
    if initial.support[30] > 0.0 && adapted.support[30] > 0.0 {
        let gap = (initial.features[0] - initial.features[30]).max(0.0);
        reachable.features[30] = adapted.features[30].clamp(reachable.features[0] - gap * 2.0, reachable.features[0] - gap * 0.5);
    }
    if initial.support[31] > 0.0 && adapted.support[31] > 0.0 {
        let gap = (initial.features[31] - initial.features[4]).max(0.0);
        reachable.features[31] = adapted.features[31].clamp(reachable.features[4] + gap * 0.5, reachable.features[4] + gap * 2.0);
    }
    let w = [4.0, 1.0, 5.0, 1.0, 4.0];
    let gap: f64 = (0..5).map(|i| (initial.features[i] - adapted.features[i]).abs() * w[i]).sum();
    let limited: f64 = (0..5).map(|i| (reachable.features[i] - adapted.features[i]).abs() * w[i]).sum();
    for i in [0, 1, 2, 3, 4, 14, 30, 31] {
        adapted.features[i] = adapted.features[i].clamp(0.0, 1.0);
    }
    (adapted, if gap < 1e-10 { 1.0 } else { (1.0 - limited / gap).clamp(0.0, 1.0) })
}

fn metric_weights(a: &Look, b: &Look, opts: Options) -> [f64; FEATURES] {
    let opts = opts.effective();
    let accepted_mass: f64 = (0..9).map(|g| a.support[LEGACY_FEATURES + g * 7].min(b.support[LEGACY_FEATURES + g * 7])).sum();
    let appearance_enabled = opts.appearance_aware && accepted_mass > 1e-8;
    std::array::from_fn(|i| {
        if i >= LEGACY_FEATURES {
            let axis = (i - LEGACY_FEATURES) % 7;
            let enabled = if axis < 5 { opts.normalize || opts.light_contrast } else { opts.normalize || opts.color_adjust || opts.color_tone };
            return if appearance_enabled && enabled {
                a.support[i].min(b.support[i]) / accepted_mass * if axis < 5 { 1.0 } else { 5.0 }
            } else {
                0.0
            };
        }
        let enabled = match i {
            0..=4 => opts.normalize || opts.light_contrast,
            5..=6 => opts.normalize,
            7 => opts.color_adjust,
            8..=13 => opts.color_tone,
            14..=16 => !is_monochrome(b) && (opts.normalize || opts.color_adjust || opts.color_tone),
            17..=24 => opts.color_adjust,
            27..=29 | 33..=48 => opts.color_adjust,
            30..=31 => opts.light_contrast,
            32 => false, // Texture is diagnostic only; matching never fits clarity/detail.
            49 => false,
            50..=58 => opts.region_aware && (opts.color_tone || opts.normalize),
            _ => opts.normalize || opts.color_tone,
        };
        f64::from(enabled)
            * if appearance_enabled { 0.05 } else { 1.0 }
            * a.support[i].min(b.support[i])
            * match i {
                0 | 4 => 4.0,
                2 => 5.0,
                25 | 26 => 3.0,
                30..=32 => 2.0,
                33..=48 => 3.0,
                14..=16 => {
                    if opts.protect_skin {
                        1.0
                    } else {
                        0.5
                    }
                }
                5..=24 => 3.0,
                _ => 1.0,
            }
    })
}
pub fn distance(a: &Look, b: &Look, opts: Options) -> f64 {
    let w = metric_weights(a, b, opts);
    (0..FEATURES)
        .map(|i| {
            let delta = a.features[i] - b.features[i];
            let delta = if i == 32 {
                0.06 * ((a.features[i].max(0.0) + LOCAL_CONTRAST_FLOOR) / (b.features[i].max(0.0) + LOCAL_CONTRAST_FLOOR)).ln()
            } else if (33..41).contains(&i) {
                (delta + 0.5).rem_euclid(1.0) - 0.5
            } else {
                delta
            };
            delta.powi(2) * w[i]
        })
        .sum()
}

// Sparse editable channel curves. Keep existing vertices, insert only the three fitting
// knots, and constrain each move between its neighbors. The renderer already uses a
// monotone cubic interpolator, so no hidden LUT is stored in the Develop result.
fn channel(c: &ToneCurve, index: usize) -> &Vec<Point> {
    [&c.red, &c.green, &c.blue][index]
}
fn channel_mut(c: &mut ToneCurve, index: usize) -> &mut Vec<Point> {
    match index {
        0 => &mut c.red,
        1 => &mut c.green,
        _ => &mut c.blue,
    }
}
fn curve_value(points: &[Point], x: f64) -> f64 {
    MonotoneCurve::new(&points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>()).eval(x)
}
fn set_channel(c: &mut ToneCurve, index: usize, x: f64, value: f64) {
    set_curve_point(channel_mut(c, index), x, value);
}
fn set_curve_point(points: &mut Vec<Point>, x: f64, value: f64) {
    if (curve_value(points, x) - value).abs() < 1e-12 {
        return;
    }
    if points.len() < 2 {
        *points = vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)];
    }
    points.sort_by(|a, b| a.x.total_cmp(&b.x));
    let left = points.iter().rev().find(|p| p.x < x - 1e-9).copied().unwrap_or(Point::new(0.0, 0.0));
    let right = points.iter().find(|p| p.x > x + 1e-9).copied().unwrap_or(Point::new(1.0, 1.0));
    // Do not invert or flatten tones to satisfy a reference. Flat/inverted existing
    // segments have no fitting freedom; their original points are retained.
    let margin = ((right.y - left.y) * 0.02).max(0.0);
    if right.y - left.y <= 1e-9 {
        return;
    }
    let value = value.clamp(left.y + margin, right.y - margin);
    if let Some(p) = points.iter_mut().find(|p| (p.x - x).abs() < 1e-9) {
        p.y = value;
    } else {
        points.push(Point::new(x, value));
        points.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
}
fn fade_channel(from: &[Point], to: &[Point], amount: f64) -> Vec<Point> {
    if amount == 0.0 || from == to {
        return from.to_vec();
    }
    if amount == 1.0 {
        return to.to_vec();
    }
    let mut xs = from.iter().chain(to).map(|p| p.x).chain([0.0, 1.0]).collect::<Vec<_>>();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let before = xs.iter().map(|x| curve_value(from, *x)).collect::<Vec<_>>();
    let delta = xs.iter().zip(&before).map(|(x, b)| curve_value(to, *x) - b).collect::<Vec<_>>();
    let mut safe = amount;
    // Overdrive extrapolates the edit, but never creates a new reversal/plateau or
    // leaves the encoded cube. Shrink the entire contribution, retaining its shape.
    for i in 0..xs.len() {
        if delta[i] > 0.0 {
            safe = safe.min((1.0 - before[i]) / delta[i]);
        }
        if delta[i] < 0.0 {
            safe = safe.min(-before[i] / delta[i]);
        }
        if i > 0 && delta[i] < delta[i - 1] {
            safe = safe.min((before[i] - before[i - 1]).max(0.0) * 0.98 / (delta[i - 1] - delta[i]));
        }
    }
    xs.into_iter().enumerate().map(|(i, x)| Point::new(x, before[i] + delta[i] * safe.max(0.0))).collect()
}
fn restore_light_curve(curve: &mut ToneCurve, before: &ToneCurve) {
    let channels = [curve.red.clone(), curve.green.clone(), curve.blue.clone()];
    *curve = before.clone();
    [curve.red, curve.green, curve.blue] = channels;
}
fn restore_channel_curves(curve: &mut ToneCurve, before: &ToneCurve) {
    curve.red = before.red.clone();
    curve.green = before.green.clone();
    curve.blue = before.blue.clone();
}

#[derive(Clone)]
enum Parameter {
    Slider(&'static str, f64),
    Grade(&'static str, bool),
    Temperature,
    Channel(usize, usize),
    Master(usize),
}
impl Parameter {
    fn value(&self, s: &DevelopSettings) -> f64 {
        match self {
            Self::Slider(id, _) => controls::get(s, id).unwrap_or(0.0),
            Self::Temperature => s.wb.temp.max(1.0).log2(),
            Self::Channel(c, knot) => curve_value(channel(&s.curve, *c), (*knot + 1) as f64 * 0.25),
            Self::Master(knot) => curve_value(&s.curve.master, MASTER_KNOTS[*knot]),
            Self::Grade(band, y) => {
                let hue = controls::get(s, &format!("grading.{band}.hue")).unwrap_or(0.0).to_radians();
                let sat = controls::get(s, &format!("grading.{band}.sat")).unwrap_or(0.0);
                sat * if *y { hue.sin() } else { hue.cos() }
            }
        }
    }
    fn step(&self) -> f64 {
        match self {
            Self::Slider(_, step) => *step,
            Self::Grade(..) => 4.0,
            Self::Temperature => 0.06,
            Self::Channel(..) => 0.015,
            Self::Master(..) => 0.025,
        }
    }
    fn scale(&self) -> f64 {
        match self {
            Self::Temperature => 0.4,
            Self::Channel(..) => 0.1,
            Self::Master(..) => 0.2,
            Self::Grade(..) => 18.0,
            Self::Slider("light.exposure", _) => 1.5,
            Self::Slider("wb.tint", _) => 25.0,
            Self::Slider(..) => 30.0,
        }
    }
    fn prior(&self) -> f64 {
        match self {
            Self::Temperature => 0.00004,
            Self::Channel(..) => 0.0003,
            Self::Master(..) => 0.00004,
            Self::Slider("light.exposure", _) => 0.000015,
            Self::Slider("light.contrast", _) => 0.0008,
            Self::Slider("color.saturation" | "color.vibrance", _) => 0.0005,
            Self::Grade(..) => 0.0003,
            _ => 0.00015,
        }
    }
    fn bounds(&self, start: &DevelopSettings, wb: (f64, f64), layer: bool) -> (f64, f64) {
        let (lo, hi) = match self {
            Self::Channel(..) => ((self.value(start) - 0.16).max(0.0), (self.value(start) + 0.16).min(1.0)),
            Self::Master(..) => ((self.value(start) - 0.4).max(0.0), (self.value(start) + 0.4).min(1.0)),
            Self::Temperature if layer => ((wb.0 * (-0.6f64).exp()).max(2000.0).log2(), (wb.0 * 0.6f64.exp()).min(50000.0).log2()),
            Self::Temperature => ((wb.0 * (-1.0f64).exp()).max(2000.0).log2(), (wb.0 * 1.0f64.exp()).min(50000.0).log2()),
            Self::Slider("light.exposure", _) => (-5.0, 5.0),
            Self::Slider("wb.tint", _) if layer => (wb.1 - 40.0, wb.1 + 40.0),
            Self::Slider("wb.tint", _) => ((wb.1 - 40.0).max(-150.0), (wb.1 + 40.0).min(150.0)),
            Self::Slider("light.contrast", _) => (-60.0, 60.0),
            Self::Slider("light.highlights" | "light.shadows", _) => (-100.0, 100.0),
            Self::Slider("color.saturation", _) => (-55.0, 55.0),
            Self::Slider("color.vibrance", _) => (-65.0, 65.0),
            Self::Slider(id, _) if id.starts_with("curve.") => (-55.0, 55.0),
            Self::Slider(..) => (-65.0, 65.0),
            Self::Grade(..) => (-45.0, 45.0),
        };
        (lo.min(self.value(start)), hi.max(self.value(start)))
    }
    fn cost(&self, opts: Options) -> f64 {
        self.prior() * if matches!(self, Self::Slider("light.contrast", _)) && !opts.contrast_guard { 0.25 } else { 1.0 }
    }
    fn bounded(&self, value: f64, start: &DevelopSettings, wb: (f64, f64), layer: bool) -> f64 {
        let (lo, hi) = self.bounds(start, wb, layer);
        value.clamp(lo + 1e-7, hi - 1e-7)
    }
    fn advance(&self, value: f64, dx: f64, start: &DevelopSettings, wb: (f64, f64), layer: bool) -> f64 {
        let (lo, hi) = self.bounds(start, wb, layer);
        let mid = (lo + hi) * 0.5;
        let radius = (hi - lo) * 0.5;
        let x = ((value - mid) / radius).clamp(-0.999, 0.999);
        let u = x.atanh();
        let derivative = (radius * (1.0 - x * x)).max(self.scale() * 0.25);
        mid + radius * (u + dx / derivative).tanh()
    }
    fn set(&self, s: &mut DevelopSettings, value: f64) {
        match self {
            Self::Channel(c, knot) => set_channel(&mut s.curve, *c, (*knot + 1) as f64 * 0.25, value),
            Self::Master(knot) => set_curve_point(&mut s.curve.master, MASTER_KNOTS[*knot], value),
            Self::Temperature => {
                s.wb.mode = WbMode::Custom;
                controls::set(s, "wb.temp", value.exp2());
            }
            Self::Slider(id, _) => {
                controls::set(s, id, value);
                if id.starts_with("wb.") {
                    s.wb.mode = WbMode::Custom;
                }
            }
            Self::Grade(band, y) => {
                let x0 = Self::Grade(band, false).value(s);
                let y0 = Self::Grade(band, true).value(s);
                let (x, y) = if *y { (x0, value) } else { (value, y0) };
                controls::set(s, &format!("grading.{band}.hue"), y.atan2(x).to_degrees().rem_euclid(360.0));
                controls::set(s, &format!("grading.{band}.sat"), x.hypot(y).min(100.0));
            }
        }
    }
}
fn parameters(opts: Options) -> Vec<Parameter> {
    let mut v = Vec::new();
    if opts.exposure_enabled() {
        v.push(Parameter::Slider("light.exposure", 0.15));
    }
    if opts.white_balance_enabled() {
        v.extend([Parameter::Temperature, Parameter::Slider("wb.tint", 3.0)]);
    }
    if opts.light_contrast {
        if opts.adjustments.luma_curve {
            v.extend((0..MASTER_KNOTS.len()).map(Parameter::Master));
        }
        for (id, on) in [
            ("light.blacks", opts.adjustments.blacks),
            ("light.whites", opts.adjustments.whites),
            ("light.shadows", opts.adjustments.shadows),
            ("light.highlights", opts.adjustments.highlights),
            ("light.contrast", opts.adjustments.contrast),
            ("curve.shadows", opts.adjustments.luma_curve),
            ("curve.darks", opts.adjustments.luma_curve),
            ("curve.lights", opts.adjustments.luma_curve),
            ("curve.highlights", opts.adjustments.luma_curve),
        ] {
            if on {
                v.push(Parameter::Slider(id, 6.0));
            }
        }
    }
    if opts.color_adjust {
        if opts.adjustments.saturation {
            v.push(Parameter::Slider("color.saturation", 6.0));
        }
        if opts.adjustments.vibrance {
            v.push(Parameter::Slider("color.vibrance", 6.0));
        }
    }
    for c in 0..3 {
        if opts.channel_enabled(c) {
            for knot in 0..3 {
                v.push(Parameter::Channel(c, knot));
            }
        }
    }
    for band in ["shadows", "midtones", "highlights", "global"] {
        if opts.wheel_enabled(band) {
            v.extend([Parameter::Grade(band, false), Parameter::Grade(band, true)]);
        }
    }
    v
}
fn channel_delta(start: &DevelopSettings, current: &DevelopSettings) -> [f64; 9] {
    std::array::from_fn(|i| {
        let parameter = Parameter::Channel(i / 3, i % 3);
        (parameter.value(current) - parameter.value(start)) / parameter.scale()
    })
}
fn palette_scale(axis: &str) -> f64 {
    if axis == "sat" { 30.0 } else { 20.0 }
}
fn palette_prior(start: &DevelopSettings, current: &DevelopSettings) -> f64 {
    PALETTE_CONTROLS
        .iter()
        .map(|id| {
            ["sat", "hue", "lum"]
                .iter()
                .map(|axis| {
                    let id = id.replace("sat", axis);
                    let delta = (controls::get(current, &id).unwrap_or(0.0) - controls::get(start, &id).unwrap_or(0.0)) / palette_scale(axis);
                    PALETTE_PRIOR * delta * delta
                })
                .sum::<f64>()
        })
        .sum()
}
fn palette_candidate(current: &DevelopSettings, direction: &[f64; 8], axis: &str, scale: f64) -> DevelopSettings {
    let mut result = current.clone();
    for (id, delta) in PALETTE_CONTROLS.iter().zip(direction) {
        let id = id.replace("sat", axis);
        let value = controls::get(current, &id).unwrap_or(0.0);
        let bound: f64 = match axis {
            "hue" => 40.0,
            "lum" => 30.0,
            _ => 55.0,
        };
        controls::set(&mut result, &id, (value + delta * scale).clamp((-bound).min(value), bound.max(value)));
    }
    result
}
// Small deterministic dense solve, used for the regularized normal equations (at most 20 axes).
fn linear_solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for k in 0..n {
        let mut pivot = k;
        for i in k + 1..n {
            if a[i][k].abs() > a[pivot][k].abs() {
                pivot = i;
            }
        }
        a.swap(k, pivot);
        b.swap(k, pivot);
        let divisor = a[k][k];
        if divisor.abs() < 1e-14 {
            continue;
        }
        for j in k..n {
            a[k][j] /= divisor;
        }
        b[k] /= divisor;
        for i in 0..n {
            if i != k {
                let factor = a[i][k];
                for j in k..n {
                    a[i][j] -= factor * a[k][j];
                }
                b[i] -= factor * b[k];
            }
        }
    }
    b
}
/// Project the incremental region curve into the same slope cone as the CDF seed. The
/// editor's monotonic LUT repair otherwise turns opposing region values into flat plateaus.
/// Evaluate its existing analytic region basis, then shrink only the new curve contribution.
fn guard_curve(start: &DevelopSettings, d: &mut DevelopSettings) {
    let from = [start.curve.shadows, start.curve.darks, start.curve.lights, start.curve.highlights];
    let to = [d.curve.shadows, d.curve.darks, d.curve.lights, d.curve.highlights];
    let delta: [f64; 4] = std::array::from_fn(|i| to[i] - from[i]);
    if delta.iter().all(|x| x.abs() < 1e-9) {
        return;
    }
    let split = [0.0, d.curve.split_shadows / 100.0, d.curve.split_mid / 100.0, d.curve.split_highlights / 100.0, 1.0];
    let feasible = |factor: f64| {
        let value = |x: f64| {
            let mut change = 0.0;
            for i in 0..4 {
                let center = (split[i] + split[i + 1]) * 0.5;
                let half = (split[i + 1] - split[i]) * 0.75 + 0.05;
                let t = ((x - center) / half.max(0.01)).clamp(-1.0, 1.0);
                change += delta[i] * factor / 100.0 * 0.22 * (0.5 + 0.5 * (t * std::f64::consts::PI).cos());
            }
            x + change * 4.0 * x * (1.0 - x)
        };
        let mut previous = value(0.0);
        for i in 1..=64 {
            let next = value(i as f64 / 64.0);
            let slope = (next - previous) * 64.0;
            if !(0.52..=1.98).contains(&slope) {
                return false;
            }
            previous = next;
        }
        true
    };
    if feasible(1.0) {
        return;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..10 {
        let middle = (lo + hi) * 0.5;
        if feasible(middle) {
            lo = middle;
        } else {
            hi = middle;
        }
    }
    let values: [f64; 4] = std::array::from_fn(|i| from[i] + delta[i] * lo);
    [d.curve.shadows, d.curve.darks, d.curve.lights, d.curve.highlights] = values;
}

/// Merge only selected Match Look controls into a layer's existing editable tools.
/// Components, opacity, legacy/manual tools and excluded controls belong to the template.
fn merge_layer_controls(mask: &mut Mask, edits: &DevelopSettings, opts: Options) {
    let original = mask.tools.view((6500.0, 0.0));
    let mut selected = edits.clone();
    preserve_excluded(&original, &mut selected, opts);
    if opts.exposure_enabled() || opts.light_enabled() {
        mask.tools.light = Some(selected.light);
    }
    if (opts.light_contrast && opts.adjustments.luma_curve) || (0..3).any(|c| opts.channel_enabled(c)) {
        mask.tools.curve = Some(selected.curve);
    }
    if opts.color_adjust && (opts.adjustments.saturation || opts.adjustments.vibrance) {
        mask.tools.color = Some(selected.color);
    }
    if opts.color_adjust && opts.adjustments.color_editor {
        mask.tools.mixer = Some(selected.mixer);
    }
    if ["global", "shadows", "midtones", "highlights"].iter().any(|band| opts.wheel_enabled(band)) {
        mask.tools.grading = Some(selected.grading);
    }
}
fn template_layer(before: &DevelopSettings, opts: Options) -> Option<&Mask> {
    opts.layer_placement.and_then(|placement| before.masks.iter().find(|mask| mask.id == placement.id))
}
fn relative_start(before: &DevelopSettings, opts: Options) -> DevelopSettings {
    template_layer(before, opts).map_or_else(DevelopSettings::default, |mask| mask.tools.view((6500.0, 0.0)))
}
fn relative_wb(before: &DevelopSettings, opts: Options, photo: (f64, f64)) -> (f64, f64) {
    template_layer(before, opts).map_or(photo, |mask| {
        mask.tools.wb.map_or((photo.0 * (mask.adjust.temp * 0.6 / 100.0).exp(), photo.1 + mask.adjust.tint * 0.4), |wb| (wb.temp, wb.tint))
    })
}
fn layer_settings(base: &DevelopSettings, edits: &DevelopSettings, opts: Options, wb: (f64, f64)) -> DevelopSettings {
    let mut d = base.clone();
    // Match Look treatment is global even when color/tone adjustments use a layer.
    if opts.color_adjust && opts.adjustments.black_white {
        d.treatment = edits.treatment;
    }
    let mut mask = template_layer(base, opts).cloned().unwrap_or_else(|| Mask {
        id: opts.layer_placement.map_or_else(|| base.masks.iter().map(|m| m.id).max().unwrap_or(0).saturating_add(1), |placement| placement.id),
        name: "Match Look".into(),
        components: vec![MaskComponent { shape: MaskShape::Full, op: Default::default(), invert: false, name: None }],
        ..Default::default()
    });
    if opts.white_balance_enabled() {
        // An absolute legacy/manual layer WB otherwise takes precedence over relative
        // normalization in the renderer. Convert this selected tool to the relative form.
        mask.tools.wb = None;
        mask.adjust.temp = (edits.wb.temp / wb.0).ln() * 100.0 / 0.6;
        mask.adjust.tint = (edits.wb.tint - wb.1) / 0.4;
    }
    merge_layer_controls(&mut mask, edits, opts);
    if let Some(placement) = opts.layer_placement {
        if let Some(existing) = d.masks.iter().position(|mask| mask.id == placement.id) {
            d.masks.remove(existing);
        }
        d.masks.insert(placement.index.min(d.masks.len()), mask);
    } else {
        d.masks.push(mask);
    }
    d
}
/// Select the explicitly owned layer, or the newly appended layer from an initial fit.
/// An unchanged/no-op result must never reinterpret an existing unrelated last mask.
fn result_layer_index(before: &DevelopSettings, full: &DevelopSettings, opts: Options) -> Option<usize> {
    if let Some(placement) = opts.layer_placement {
        full.masks.iter().position(|mask| mask.id == placement.id)
    } else {
        (full.masks.len() > before.masks.len()).then(|| full.masks.len() - 1)
    }
}

/// Copy only the color controls selected by Match Look. Tone curves belong to Color
/// Tone independently of the luminance/master curve and its Light settings.
pub fn copy_color_controls(destination: &mut DevelopSettings, shared: &DevelopSettings, opts: Options) {
    if opts.color_adjust {
        if opts.adjustments.saturation {
            destination.color.saturation = shared.color.saturation;
        }
        if opts.adjustments.vibrance {
            destination.color.vibrance = shared.color.vibrance;
        }
        if opts.adjustments.color_editor {
            destination.mixer = shared.mixer;
        }
        if opts.adjustments.black_white {
            destination.treatment = shared.treatment;
        }
    }
    for c in 0..3 {
        if opts.channel_enabled(c) {
            *channel_mut(&mut destination.curve, c) = channel(&shared.curve, c).clone();
        }
    }
    if opts.wheel_enabled("global") {
        destination.grading.global = shared.grading.global;
    }
    if opts.wheel_enabled("shadows") {
        destination.grading.shadows = shared.grading.shadows;
    }
    if opts.wheel_enabled("midtones") {
        destination.grading.midtones = shared.grading.midtones;
    }
    if opts.wheel_enabled("highlights") {
        destination.grading.highlights = shared.grading.highlights;
    }
}
/// Extract editable color controls from an applied result. Layer tools are relative;
/// treatment is global. The returned value does not contain masks or geometry.
pub fn shared_color_settings(before: &DevelopSettings, full: &DevelopSettings, opts: Options) -> DevelopSettings {
    let mut shared = DevelopSettings::default();
    if opts.new_layer {
        if let Some(layer) = result_layer_index(before, full, opts).and_then(|index| full.masks.get(index)) {
            shared.color = layer.tools.color.unwrap_or_default();
            shared.mixer = layer.tools.mixer.unwrap_or_default();
            shared.grading = layer.tools.grading.unwrap_or_default();
            shared.curve = layer.tools.curve.clone().unwrap_or_default();
        }
        shared.treatment = full.treatment;
        let extracted = shared;
        shared = DevelopSettings::default();
        copy_color_controls(&mut shared, &extracted, opts);
    } else {
        copy_color_controls(&mut shared, full, opts);
    }
    shared
}
fn shared_amount(before: &DevelopSettings, full: &DevelopSettings, opts: Options, result: &mut DevelopSettings) {
    if !opts.consistent_batch {
        return;
    }
    if opts.strength == 0.0 {
        copy_color_controls(result, before, opts);
        return;
    }
    let shared =
        fade_controls(&DevelopSettings::default(), full, Options { consistent_batch: false, normalize: false, light_contrast: false, ..opts });
    copy_color_controls(result, &shared, opts);
}

/// Slider-space style amount. Normalize stays at full strength, including effective preset WB.
pub fn fade_controls(start: &DevelopSettings, full: &DevelopSettings, opts: Options) -> DevelopSettings {
    let opts = opts.effective();
    let opts = if start.treatment == Treatment::Bw && full.treatment == Treatment::Bw {
        Options { color_adjust: false, color_tone: false, ..opts }
    } else {
        opts
    };
    let mut d = start.clone();
    let amount = opts.strength.clamp(0.0, 125.0) / 100.0;
    if opts.color_adjust && opts.adjustments.black_white {
        d.treatment = full.treatment;
    }
    for p in parameters(opts) {
        if matches!(p, Parameter::Channel(..) | Parameter::Master(..)) {
            continue;
        }
        let normalization = matches!(&p, Parameter::Temperature | Parameter::Slider("light.exposure" | "wb.tint", _));
        let factor = if normalization { 1.0 } else { amount };
        if matches!(p, Parameter::Temperature | Parameter::Slider("wb.tint", _)) {
            d.wb = full.wb;
        } else {
            p.set(&mut d, p.value(start) + (p.value(full) - p.value(start)) * factor);
        }
    }
    if opts.light_contrast && opts.adjustments.luma_curve && start.curve.mode == CurveMode::Luminance {
        d.curve.master = fade_channel(&start.curve.master, &full.curve.master, amount);
    }
    if opts.color_adjust && opts.adjustments.color_editor {
        for id in PALETTE_CONTROLS {
            for axis in ["sat", "hue", "lum"] {
                let id = id.replace("sat", axis);
                let id = id.as_str();
                let from = controls::get(start, id).unwrap_or(0.0);
                controls::set(&mut d, id, from + (controls::get(full, id).unwrap_or(0.0) - from) * amount);
            }
        }
    }
    for c in 0..3 {
        if opts.channel_enabled(c) {
            *channel_mut(&mut d.curve, c) = fade_channel(channel(&start.curve, c), channel(&full.curve, c), amount);
        }
    }
    // Cartesian grading hue/saturation are interpolated by parameters() above.
    // Luminance is part of a selected wheel and can also come from a shared batch style.
    if opts.wheel_enabled("global") {
        d.grading.global.lum = start.grading.global.lum + (full.grading.global.lum - start.grading.global.lum) * amount;
    }
    if opts.wheel_enabled("shadows") {
        d.grading.shadows.lum = start.grading.shadows.lum + (full.grading.shadows.lum - start.grading.shadows.lum) * amount;
    }
    if opts.wheel_enabled("midtones") {
        d.grading.midtones.lum = start.grading.midtones.lum + (full.grading.midtones.lum - start.grading.midtones.lum) * amount;
    }
    if opts.wheel_enabled("highlights") {
        d.grading.highlights.lum = start.grading.highlights.lum + (full.grading.highlights.lum - start.grading.highlights.lum) * amount;
    }
    if amount == 1.0 {
        copy_color_controls(&mut d, full, opts);
    }
    // Preserve a disabled normalization group's original preset mode.
    if !opts.white_balance_enabled() {
        d.wb = start.wb;
    }
    guard_curve(start, &mut d);
    preserve_excluded(start, &mut d, opts);
    d
}
/// Re-amount an applied result without solving or touching unrelated tools. Layer tools are
/// relative; their normalization adjustment remains unscaled.
pub fn amount_settings(before: &DevelopSettings, full: &DevelopSettings, opts: Options) -> DevelopSettings {
    let opts = opts.effective();
    if opts.new_layer {
        let Some(index) = result_layer_index(before, full, opts) else {
            return full.clone();
        };
        let mut d = full.clone();
        if !opts.color_adjust || !opts.adjustments.black_white || (opts.consistent_batch && opts.strength == 0.0) {
            d.treatment = before.treatment;
        }
        if let Some(layer) = d.masks.get_mut(index) {
            let original = relative_start(before, opts);
            let end = layer.tools.view((6500.0, 0.0));
            let faded = fade_controls(&original, &end, Options { keep_white_balance: true, ..opts });
            // `faded` already restores excluded fields from the baseline template.
            // Do not restore them from the fully fitted mask when exclusions change.
            if layer.tools.light.is_some() {
                layer.tools.light = Some(faded.light);
            }
            if layer.tools.curve.is_some() {
                layer.tools.curve = Some(faded.curve);
            }
            if layer.tools.color.is_some() {
                layer.tools.color = Some(faded.color);
            }
            if layer.tools.mixer.is_some() {
                layer.tools.mixer = Some(faded.mixer);
            }
            if layer.tools.grading.is_some() {
                layer.tools.grading = Some(faded.grading);
            }
            if !opts.white_balance_enabled() {
                let adjust = template_layer(before, opts).map(|mask| &mask.adjust);
                layer.adjust.temp = adjust.map_or(0.0, |a| a.temp);
                layer.adjust.tint = adjust.map_or(0.0, |a| a.tint);
            }
        }
        d
    } else {
        let mut result = fade_controls(before, full, opts);
        shared_amount(before, full, opts, &mut result);
        result
    }
}

pub fn reset_group(before: &DevelopSettings, full: &DevelopSettings, opts: Options, group: &str) -> Option<DevelopSettings> {
    let mut d = full.clone();
    if opts.new_layer {
        let index = result_layer_index(before, full, opts)?;
        let original = relative_start(before, opts);
        let layer = d.masks.get_mut(index)?;
        let current = layer.tools.view((6500.0, 0.0));
        let restored = reset_group(&original, &current, Options { new_layer: false, ..opts }, group)?;
        merge_layer_controls(layer, &restored, opts);
        let template = template_layer(before, opts);
        if group == "normalize" && opts.white_balance_enabled() {
            layer.tools.wb = template.and_then(|mask| mask.tools.wb);
            layer.adjust.temp = template.map_or(0.0, |mask| mask.adjust.temp);
            layer.adjust.tint = template.map_or(0.0, |mask| mask.adjust.tint);
        }
        // Sparse sections that were absent before fitting remain absent after a full reset.
        if group == "color" {
            if restored.color == Default::default() && template.is_none_or(|m| m.tools.color.is_none()) {
                layer.tools.color = None;
            }
            if restored.mixer == Default::default() && template.is_none_or(|m| m.tools.mixer.is_none()) {
                layer.tools.mixer = None;
            }
        }
        if group == "tone" && restored.grading == Default::default() && template.is_none_or(|m| m.tools.grading.is_none()) {
            layer.tools.grading = None;
        }
        if group == "color" && opts.color_adjust && opts.adjustments.black_white {
            d.treatment = before.treatment;
        }
    } else {
        match group {
            "normalize" => {
                d.wb = before.wb;
                d.light.exposure = before.light.exposure;
            }
            "light" => {
                let exposure = d.light.exposure;
                d.light = before.light;
                d.light.exposure = exposure;
                restore_light_curve(&mut d.curve, &before.curve);
            }
            "color" => {
                d.treatment = before.treatment;
                d.color = before.color;
                d.mixer = before.mixer;
            }
            "tone" => {
                d.grading = before.grading;
                restore_channel_curves(&mut d.curve, &before.curve);
            }
            _ => return None,
        }
    }
    if !opts.new_layer {
        preserve_excluded(full, &mut d, opts);
    }
    Some(d)
}

pub fn is_monochrome(look: &Look) -> bool {
    look.monochrome
        || (look.features[7] < 0.012 && (17..25).all(|i| look.support[i] < 0.05))
        || (look.features[7] < 0.045
            && look.features[49] < 0.9
            && (17..25).filter(|i| look.support[*i] > 0.05).count() <= 2
            && look.hue_distribution.iter().copied().fold(0.0, f64::max) >= 0.8)
}

pub struct Solved {
    pub monochrome: bool,
    pub reach: f64,
    pub fit: f64,
    pub goal: Look,
    pub settings: DevelopSettings,
    pub full_settings: DevelopSettings,
    pub before_error: f64,
    pub after_error: f64,
    pub weights: Vec<f64>,
    pub similarity: f64,
    pub renders: usize,
}
/// Bounded closed-loop solve, sequential reductions and a stable parameter order. Cancellation
/// is checked before every render; cancellation returns None and the caller commits nothing.
pub fn solve(
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    base: &DevelopSettings,
    refs: &[Look],
    opts: Options,
    cancel: &dyn Fn() -> bool,
) -> Option<Solved> {
    solve_with_faces(src, info, base, refs, opts, &[], cancel)
}
/// Diagnostic technique switches for reproducible quality ablations. Production uses all three.
#[derive(Clone, Copy)]
pub struct Techniques {
    pub warm_start: bool,
    pub tone_projection: bool,
    pub joint_refine: bool,
    pub parallel_probes: bool,
}
impl Default for Techniques {
    fn default() -> Self {
        Self { warm_start: true, tone_projection: true, joint_refine: true, parallel_probes: true }
    }
}
#[allow(clippy::too_many_arguments)]
pub fn solve_with_techniques(
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    base: &DevelopSettings,
    refs: &[Look],
    opts: Options,
    techniques: Techniques,
    cancel: &dyn Fn() -> bool,
) -> Option<Solved> {
    solve_impl::<false>(src, info, base, refs, opts, &[], techniques, None, cancel)
}
#[allow(clippy::too_many_arguments)]
pub fn solve_with_faces(
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    base: &DevelopSettings,
    refs: &[Look],
    opts: Options,
    faces: &[[f32; 4]],
    cancel: &dyn Fn() -> bool,
) -> Option<Solved> {
    solve_impl::<false>(src, info, base, refs, opts, faces, Techniques::default(), None, cancel)
}
/// Batches already parallelize independent photos. Keep the small Jacobian probes in each
/// photo sequential to avoid competing nested probe queues; the render kernels share the pool.
#[allow(clippy::too_many_arguments)]
pub fn solve_batch_with_faces(
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    base: &DevelopSettings,
    refs: &[Look],
    opts: Options,
    faces: &[[f32; 4]],
    cancel: &dyn Fn() -> bool,
) -> Option<Solved> {
    lightcraft_raster::sequential(|| {
        solve_impl::<false>(src, info, base, refs, opts, faces, Techniques { parallel_probes: false, ..Default::default() }, None, cancel)
    })
}
/// Batch consistency refines each target's normalization and light while holding
/// shared editable color controls fixed. This is this app's defined lock list; Capture
/// One does not publish its full list. Caller selects the shared solution once per batch.
#[allow(clippy::too_many_arguments)]
pub fn solve_batch_locked_color_with_faces(
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    base: &DevelopSettings,
    refs: &[Look],
    opts: Options,
    faces: &[[f32; 4]],
    shared: &DevelopSettings,
    cancel: &dyn Fn() -> bool,
) -> Option<Solved> {
    lightcraft_raster::sequential(|| {
        solve_impl::<false>(src, info, base, refs, opts, faces, Techniques { parallel_probes: false, ..Default::default() }, Some(shared), cancel)
    })
}
#[allow(clippy::too_many_arguments)]
fn solve_impl<const FULL_DESCRIPTOR: bool>(
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    base: &DevelopSettings,
    refs: &[Look],
    opts: Options,
    faces: &[[f32; 4]],
    techniques: Techniques,
    shared: Option<&DevelopSettings>,
    cancel: &dyn Fn() -> bool,
) -> Option<Solved> {
    let opts = opts.effective();
    if !opts.strength.is_finite()
        || !(0.0..=125.0).contains(&opts.strength)
        || refs.is_empty()
        || refs.len() > MAX_REFERENCES
        || refs.iter().any(|r| !r.valid())
        || cancel()
    {
        return None;
    }
    let faces = framed_faces(src, info, base, faces);
    let (cal_src, cal_info) = measurement_source(src, info, CALIBRATION_EDGE);
    let (fine_src, fine_info) = measurement_source(src, info, FINE_EDGE);
    let (src, info) = measurement_source(src, info, SOURCE_EDGE);
    let edge = PROXY_EDGE.min(src.width.max(src.height));
    let mut req = RenderRequest::fit(edge, edge);
    req.depth = crate::OutputDepth::F32Linear;
    req.statistics_only = !opts.region_aware;
    let fine_edge = FINE_EDGE.min(fine_src.width.max(fine_src.height));
    let mut fine_req = RenderRequest::fit(fine_edge, fine_edge);
    fine_req.depth = crate::OutputDepth::F32Linear;
    fine_req.statistics_only = !opts.region_aware;
    let cache = StageCache::default();
    let fine_cache = StageCache::default();
    let cal_edge = CALIBRATION_EDGE.min(cal_src.width.max(cal_src.height));
    let mut cal_req = RenderRequest::fit(cal_edge, cal_edge);
    cal_req.depth = crate::OutputDepth::F32Linear;
    cal_req.statistics_only = !opts.region_aware;
    let cal_render = crate::render(&cal_src, &cal_info, base, &cal_req);
    let cal_membership = if opts.appearance_aware { rendered_membership(&cal_render) } else { appearance::disabled() };
    let mut calibrated = measure_rendered_with_membership(&cal_render, &faces, Some(&cal_membership));
    let fine_render = crate::render_cached(&fine_src, &fine_info, base, &fine_req, &fine_cache);
    let fine_membership = Some(if opts.appearance_aware { rendered_membership(&fine_render) } else { appearance::disabled() });
    let fine_initial = measure_rendered_with_membership(&fine_render, &faces, fine_membership.as_ref());
    let coarse_membership =
        Some(if opts.appearance_aware { rendered_membership(&crate::render(&src, &info, base, &req)) } else { appearance::disabled() });
    let offset: [f64; FEATURES] = std::array::from_fn(|i| {
        if i < 5 || i == 7 || (27..=32).contains(&i) || i >= LEGACY_FEATURES { calibrated.features[i] - fine_initial.features[i] } else { 0.0 }
    });
    let fine_regions = opts.region_aware.then(|| region_masks(&fine_render, &fine_src, &fine_info, base, &faces));
    if opts.region_aware {
        let masks = region_masks(&cal_render, &cal_src, &cal_info, base, &faces);
        measure_regions(&mut calibrated, &cal_render, &masks);
    }
    let coarse_regions = opts.region_aware.then(|| region_masks(&crate::render(&src, &info, base, &req), &src, &info, base, &faces));
    let renders = std::cell::Cell::new(3 + usize::from(opts.region_aware));
    let evaluate = |d: &DevelopSettings, fine: bool| {
        renders.set(renders.get() + 1);
        let image = if fine {
            crate::render_cached(&fine_src, &fine_info, d, &fine_req, &fine_cache)
        } else {
            crate::render_cached(&src, &info, d, &req, &cache)
        };
        let mut look = measure_rendered_with_membership(&image, &faces, if fine { fine_membership.as_ref() } else { coarse_membership.as_ref() });
        if let Some(masks) = if fine { fine_regions.as_ref() } else { coarse_regions.as_ref() } {
            measure_regions(&mut look, &image, masks);
        }
        if fine {
            for (f, delta) in look.features.iter_mut().zip(offset) {
                *f += delta;
            }
        }
        look
    };
    let mut initial = fine_initial.clone();
    for (f, delta) in initial.features.iter_mut().zip(offset) {
        *f += delta;
    }
    if let Some(masks) = &fine_regions {
        measure_regions(&mut initial, &fine_render, masks);
    }
    // Reference analysis uses the same 128px source/geometry for descriptors. A 60px
    // fitting proxy can lose enough structure to misclassify an edited copy of this capture.
    let (signature_src, signature_info) = measurement_source(&fine_src, &fine_info, 128);
    let descriptor_image = crate::render(&signature_src, &signature_info, &DevelopSettings::default(), &RenderRequest::fit(128, 128));
    // The legacy full measurement remains a test oracle; the solver reads only
    // signature/content, whose arithmetic is independent of appearance statistics.
    let target_descriptor = if FULL_DESCRIPTOR { measure(&descriptor_image.image) } else { descriptor::measure(&descriptor_image.image) };
    let signature = target_descriptor.signature;
    let similarity = refs
        .iter()
        .map(|r| {
            let color = (-2.0 * r.signature.iter().zip(signature).map(|(a, b)| (a - b).powi(2)).sum::<f64>()).exp();
            color.max(content::confidence(&r.content, &target_descriptor.content))
        })
        .fold(0.0, f64::max);
    let w = weights_with_content(refs, &signature, &target_descriptor.content, opts.consistent_batch);
    let reference_goal = blend(refs, &w);
    let goal = reference_goal.clone();
    let adaptation = if opts.consistent_batch { 1.0 } else { similarity.sqrt().clamp(0.35, 1.0) };
    let (goal, reach) = reachable_goal(&initial, &goal, opts, similarity);
    let mut goal = goal;
    // A neutral cluster is an illuminant fallback only when the neutral mask has no support.
    for look in [&mut goal] {
        if look.support[5] < 0.05
            && let Some(c) = look.clusters.iter().min_by(|a, b| a.ab[0].hypot(a.ab[1]).total_cmp(&b.ab[0].hypot(b.ab[1])))
        {
            look.features[5] = c.ab[0];
            look.features[6] = c.ab[1];
            look.support[5] = 0.1;
            look.support[6] = 0.1;
        }
    }
    let monochrome = is_monochrome(&goal) && opts.color_adjust && opts.adjustments.black_white;
    let target_bw = base.treatment == Treatment::Bw && shared.is_none();
    let solver_opts = Options {
        color_adjust: opts.color_adjust && !monochrome && !target_bw,
        color_tone: opts.color_tone && !target_bw,
        protect_skin: opts.protect_skin && !monochrome && !target_bw,
        ..opts
    };
    let before_error = distance(&initial, &goal, opts);
    // Pixel-identical/self matches and zero Impact are exactly no-ops, including layer mode.
    if (!opts.normalize && !opts.light_contrast && !opts.color_adjust && !opts.color_tone)
        || (opts.strength <= 0.0 && !opts.exposure_enabled() && !opts.white_balance_enabled())
        || (shared.is_none() && (distance(&calibrated, &reference_goal, opts) < 1e-10 || before_error < 1e-7))
    {
        return Some(Solved {
            monochrome,
            reach,
            fit: 1.0,
            goal: goal.clone(),
            settings: base.clone(),
            full_settings: base.clone(),
            before_error: 0.0,
            after_error: 0.0,
            weights: w,
            similarity,
            renders: renders.get(),
        });
    }
    let wb = crate::auto::current_wb(&info, base);
    let start = if opts.new_layer {
        let local_wb = relative_wb(base, opts, wb);
        DevelopSettings {
            wb: lightcraft_develop::WhiteBalance { mode: WbMode::Custom, temp: local_wb.0, tint: local_wb.1 },
            treatment: base.treatment,
            ..relative_start(base, opts)
        }
    } else {
        let mut d = base.clone();
        if opts.white_balance_enabled() && d.wb.mode != WbMode::Custom {
            d.wb.temp = wb.0;
            d.wb.tint = wb.1;
        }
        d
    };
    let mut edits = start.clone();
    if let Some(shared) = shared {
        copy_color_controls(&mut edits, shared, opts);
    }
    if monochrome && shared.is_none() {
        edits.treatment = Treatment::Bw;
        if goal.features[7] >= 0.012 && opts.color_tone && goal.hue_distribution.iter().copied().fold(0.0, f64::max) >= 0.8 {
            let dominant = goal.hue_distribution.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map_or(0, |(i, _)| i);
            if opts.wheel_enabled("midtones") {
                edits.grading.midtones.hue = (dominant as f64 + 0.5) * 45.0;
                edits.grading.midtones.sat = 0.0;
            }
            if opts.wheel_enabled("global") {
                edits.grading.global.hue = goal.features[26].atan2(goal.features[25]).to_degrees().rem_euclid(360.0);
                edits.grading.global.sat = (100.0 * goal.features[7] / 0.1).min(45.0);
            }
        }
    }
    let layer_opts = Options { color_adjust: opts.color_adjust && !target_bw, color_tone: opts.color_tone && !target_bw, ..opts };
    let materialize = |e: &DevelopSettings| if opts.new_layer { layer_settings(base, e, layer_opts, wb) } else { e.clone() };
    let mut current = if monochrome || shared.is_some() { evaluate(&materialize(&edits), true) } else { initial.clone() };

    let fit_opts = if shared.is_some() { Options { color_adjust: false, color_tone: false, ..solver_opts } } else { solver_opts };
    let mut params = parameters(fit_opts);
    // Existing RGB master curves retain their meaning. Fit Light with the light
    // sliders in that case; a new layer has its own luminance-mode curve.
    if start.curve.mode != CurveMode::Luminance {
        params.retain(|p| !matches!(p, Parameter::Master(..)) && !matches!(p, Parameter::Slider(id, _) if id.starts_with("curve.")));
    }
    let coupled_curves = params.iter().any(|p| matches!(p, Parameter::Channel(..)));
    let mut mw = metric_weights(&initial, &goal, solver_opts);
    for i in [5, 6, 25, 26] {
        mw[i] *= 0.5 + 0.5 * adaptation;
    }
    if !faces.is_empty() && solver_opts.protect_skin {
        for weight in &mut mw[14..=16] {
            *weight *= 3.0;
        }
    }
    let spread: [f64; FEATURES] = std::array::from_fn(|i| {
        let deviations: Vec<_> = refs.iter().zip(&w).map(|(r, w)| ((r.features[i] - reference_goal.features[i]).abs(), *w)).collect();
        weighted_median(&deviations).max(if i < 5 { 0.06 } else { 0.025 })
    });
    for i in 0..FEATURES {
        let floor = if i < 5 { 0.06 } else { 0.025 };
        mw[i] *= (floor / spread[i]).powi(2);
    }
    let huber = |r: f64, scale: f64| {
        let a = r.abs();
        if a <= scale { r * r } else { 2.0 * scale * a - scale * scale }
    };
    let skin_anchor = std::cell::Cell::new(initial.features[16].atan2(initial.features[15]));
    let skin_enforce = std::cell::Cell::new(false);
    // A warm-neutral window is an optional observation, not a face anchor. Once
    // pixels leave that window, its empty statistic must not masquerade as black
    // skin. Fit confidence-scaled residuals (and their finite-difference Jacobian).
    // Cached face observations keep their existing weight and hue guard.
    let observation = |look: &Look, i: usize| {
        if faces.is_empty() && (14..=16).contains(&i) {
            let confidence = (look.support[i] / initial.support[i].max(1e-12)).min(1.0).sqrt();
            goal.features[i] + (look.features[i] - goal.features[i]) * confidence
        } else if i == 32 {
            // Local contrast has a much smaller physical range than lightness. A log-ratio
            // residual measures relative texture strength while preserving the stored statistic.
            goal.features[i] + 0.06 * ((look.features[i].max(0.0) + LOCAL_CONTRAST_FLOOR) / (goal.features[i].max(0.0) + LOCAL_CONTRAST_FLOOR)).ln()
        } else if (33..41).contains(&i) {
            let delta = (look.features[i] - goal.features[i] + 0.5).rem_euclid(1.0) - 0.5;
            let confidence = (look.support[i] / initial.support[i].max(1e-12)).min(1.0).sqrt();
            goal.features[i] + delta * confidence
        } else {
            look.features[i]
        }
    };
    let objective = |l: &Look, d: &DevelopSettings, penalty: f64| {
        let fit = (0..FEATURES).map(|i| huber(observation(l, i) - goal.features[i], spread[i] * 2.0) * mw[i]).sum::<f64>();
        let mut prior = params
            .iter()
            .map(|p| {
                let x = (p.value(d) - p.value(&start)) / p.scale();
                p.cost(opts) * (x * x + 0.08 * ((x * x + 0.01).sqrt() - 0.1))
            })
            .sum::<f64>();
        if coupled_curves {
            prior += regularization::energy(&channel_delta(&start, d));
        }
        if solver_opts.color_adjust && opts.adjustments.color_editor && shared.is_none() {
            prior += palette_prior(&start, d);
        }
        let skin = if solver_opts.protect_skin && !faces.is_empty() && initial.support[14] > 0.05 && l.support[14] > 0.05 {
            let h0 = skin_anchor.get();
            let h = l.features[16].atan2(l.features[15]);
            let dh = (h - h0 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
            // Cached face regions are a reliable anchor: style fitting stays within 16°,
            // leaving headroom for 125% Impact. WB warm start establishes the normalized anchor.
            if !faces.is_empty() && skin_enforce.get() && dh.abs() > 16f64.to_radians() {
                return f64::INFINITY;
            }
            0.003 * (dh.abs() - 20f64.to_radians()).max(0.0).powi(2)
        } else {
            0.0
        };
        fit + penalty * prior + skin
    };
    let mut score = objective(&current, &edits, 1.0);
    if techniques.warm_start && opts.exposure_enabled() && current.features[2] > 0.02 && goal.features[2] > 0.02 {
        let mut seed = edits.clone();
        let p = Parameter::Slider("light.exposure", 0.15);
        p.set(&mut seed, p.bounded(p.value(&start) + 3.0 * (goal.features[2] / current.features[2]).log2(), &start, wb, opts.new_layer));
        if cancel() {
            return None;
        }
        let rendered = evaluate(&materialize(&seed), true);
        let next = objective(&rendered, &seed, 1.0);
        if next < score {
            edits = seed;
            current = rendered;
        }
    }
    let mut probe_renders = 0;
    let mut previous_columns = vec![[0.0; FEATURES]; params.len()];
    let mut previous_values = params.iter().map(|p| p.value(&edits)).collect::<Vec<_>>();
    let mut refresh_all = false;
    let passes = 4;
    for pass in 0..passes {
        let penalty = [1.6, 1.0, 0.6, 0.5][pass];
        score = objective(&current, &edits, penalty);
        let pass_start_score = score;
        let coarse_current = evaluate(&materialize(&edits), false);
        if cancel() {
            return None;
        }
        // Probe renders are independent. Each owns its cache; indexed collection fixes reduction
        // order regardless of the background pool's width. Small moves reuse the last basis.
        let mut refresh = vec![pass == 0 || refresh_all; params.len()];
        if pass > 0 && !refresh_all {
            // Keep a bounded render budget, refreshing the most changed axes first. The
            // remaining columns are still valid secant approximations for the line search.
            let mut changed = params
                .iter()
                .enumerate()
                .filter_map(|(j, p)| {
                    let step = (p.value(&edits) - previous_values[j]).abs() / p.scale();
                    (step > 0.2).then_some((j, step))
                })
                .collect::<Vec<_>>();
            changed.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let reserve = (passes - pass) * 3 + 12;
            let budget = (60 + params.len() * 3).saturating_sub(renders.get() + probe_renders + reserve);
            for (j, _) in changed.into_iter().take(budget) {
                refresh[j] = true;
            }
        }
        let probe_column = |(j, p): (usize, &Parameter)| {
            if !refresh[j] {
                return (previous_columns[j], false);
            }
            let x = p.value(&edits);
            let mut probe = edits.clone();
            p.set(&mut probe, p.bounded(x + p.step(), &start, wb, opts.new_layer));
            guard_curve(&start, &mut probe);
            let mut h = (p.value(&probe) - x) / p.scale();
            if h.abs() < 1e-8 {
                p.set(&mut probe, p.bounded(x - p.step(), &start, wb, opts.new_layer));
                guard_curve(&start, &mut probe);
                h = (p.value(&probe) - x) / p.scale();
            }
            if h.abs() < 1e-8 {
                return ([0.0; FEATURES], false);
            }
            let image = crate::render_cached(&src, &info, &materialize(&probe), &req, &cache.fork_cpu());
            let mut measured = measure_rendered_with_membership(&image, &faces, coarse_membership.as_ref());
            if let Some(masks) = &coarse_regions {
                measure_regions(&mut measured, &image, masks);
            }
            (std::array::from_fn(|i| (observation(&measured, i) - observation(&coarse_current, i)) / h), true)
        };
        let columns: Vec<_> = if !techniques.parallel_probes || rayon::current_num_threads() == 1 {
            params.iter().enumerate().map(probe_column).collect()
        } else {
            use rayon::prelude::*;
            params.par_iter().enumerate().map(probe_column).collect()
        };
        // evaluate owns the render counter; count independent probes at the end via a side total.
        probe_renders += columns.iter().filter(|(_, rendered)| *rendered).count();
        let jac = columns.into_iter().map(|(column, _)| column).collect::<Vec<_>>();
        refresh_all = false;
        previous_columns.clone_from(&jac);
        previous_values = params.iter().map(|p| p.value(&edits)).collect();
        if cancel() {
            return None;
        }
        if techniques.warm_start && pass == 0 && opts.white_balance_enabled() {
            let axes: Vec<_> =
                params.iter().enumerate().filter(|(_, p)| matches!(p, Parameter::Temperature | Parameter::Slider("wb.tint", _))).collect();
            if axes.len() == 2 {
                let wb_features: &[usize] = if opts.appearance_aware && mw[LEGACY_FEATURES + 5] > 0.0 {
                    &[LEGACY_FEATURES + 5, LEGACY_FEATURES + 6]
                } else {
                    &[5, 6, 25, 26]
                };
                let mut normal = vec![vec![0.0; 2]; 2];
                let mut rhs = vec![0.0; 2];
                for j in 0..2 {
                    for k in 0..2 {
                        normal[j][k] = wb_features.iter().map(|i| jac[axes[j].0][*i] * jac[axes[k].0][*i] * mw[*i]).sum();
                    }
                    normal[j][j] += axes[j].1.prior();
                    rhs[j] = wb_features.iter().map(|i| jac[axes[j].0][*i] * (goal.features[*i] - current.features[*i]) * mw[*i]).sum();
                }
                let step = linear_solve(normal, rhs);
                let mut seed = edits.clone();
                for ((_, p), dx) in axes.iter().zip(step) {
                    let value = p.advance(p.value(&edits), dx * p.scale(), &start, wb, opts.new_layer);
                    p.set(&mut seed, value);
                }
                if cancel() {
                    return None;
                }
                let measured = evaluate(&materialize(&seed), true);
                let next = objective(&measured, &seed, penalty);
                if next < score {
                    edits = seed;
                    current = measured;
                    score = next;
                }
            }
        }
        if pass == 0 {
            skin_anchor.set(current.features[16].atan2(current.features[15]));
            skin_enforce.set(true);
        }
        // Project populated tone quantiles onto editable tools through their real renderer
        // derivatives. The master curve is monotone, but legitimate HDR compression is
        // not limited to the parametric region curve's twofold incremental slope cone.
        if techniques.tone_projection && pass == 0 && opts.light_contrast && opts.adjustments.luma_curve {
            let mut residual = std::array::from_fn::<_, 5, _>(|i| goal.features[i] - current.features[i]);
            let mut projected = edits.clone();
            for (j, p) in params.iter().enumerate() {
                if !matches!(p, Parameter::Master(..)) && !matches!(p,Parameter::Slider(id,_) if id.starts_with("light.") || id.starts_with("curve."))
                {
                    continue;
                }
                let denominator = (0..5).map(|i| jac[j][i] * jac[j][i] * mw[i]).sum::<f64>() + p.cost(opts) * 2.0;
                let dx = ((0..5).map(|i| jac[j][i] * residual[i] * mw[i]).sum::<f64>() / denominator.max(1e-10)).clamp(-1.0, 1.0);
                let value = p.advance(p.value(&projected), dx * p.scale(), &start, wb, opts.new_layer);
                p.set(&mut projected, value);
                for i in 0..5 {
                    residual[i] -= jac[j][i] * dx;
                }
            }
            if cancel() {
                return None;
            }
            guard_curve(&start, &mut projected);
            let l = evaluate(&materialize(&projected), true);
            let next = objective(&l, &projected, penalty);
            if next < score {
                edits = projected;
                current = l;
                score = next;
            }
        }
        if !techniques.joint_refine {
            break;
        }
        // A warm WB / CDF correction changes the local basis. Recompute on the next proxy
        // pass rather than taking a joint step with that obsolete Jacobian.
        if pass == 0 && score < pass_start_score - 1e-6 {
            refresh_all = true;
            continue;
        }
        let robust: [f64; FEATURES] =
            std::array::from_fn(|i| mw[i] * (spread[i] * 2.0 / (goal.features[i] - observation(&current, i)).abs().max(spread[i] * 2.0)));
        let n = params.len();
        let mut a = vec![vec![0.0; n]; n];
        let mut rhs = vec![0.0; n];
        for j in 0..n {
            for k in 0..n {
                a[j][k] = (0..FEATURES).map(|i| jac[j][i] * jac[k][i] * robust[i]).sum();
            }
            let x = (params[j].value(&edits) - params[j].value(&start)) / params[j].scale();
            let prior = params[j].cost(opts) * penalty * (1.0 + 0.04 / (x * x + 0.01).sqrt());
            a[j][j] += prior + 0.00002;
            rhs[j] = (0..FEATURES).map(|i| jac[j][i] * (goal.features[i] - observation(&current, i)) * robust[i]).sum::<f64>()
                - prior * (params[j].value(&edits) - params[j].value(&start)) / params[j].scale();
        }
        let delta = channel_delta(&start, &edits);
        for (j, parameter) in params.iter().enumerate() {
            if let Parameter::Channel(c, knot) = parameter {
                let index = c * 3 + knot;
                rhs[j] -= penalty * (0..9).map(|k| regularization::matrix(index, k) * delta[k]).sum::<f64>();
                for (k, other) in params.iter().enumerate() {
                    if let Parameter::Channel(c, knot) = other {
                        a[j][k] += penalty * regularization::matrix(index, c * 3 + knot);
                    }
                }
            }
        }
        let step = linear_solve(a, rhs);
        let old_score = score;
        let mut accepted = false;
        for factor in [1.0, 0.5] {
            let mut candidate = edits.clone();
            for (p, dx) in params.iter().zip(&step) {
                p.set(&mut candidate, p.advance(p.value(&edits), dx.clamp(-2.5, 2.5) * p.scale() * factor, &start, wb, opts.new_layer));
            }
            if cancel() {
                return None;
            }
            guard_curve(&start, &mut candidate);
            let measured = evaluate(&materialize(&candidate), true);
            let next = objective(&measured, &candidate, penalty);
            if next < score {
                edits = candidate;
                current = measured;
                score = next;
                accepted = true;
                break;
            }
        }
        if !accepted || old_score - score < 1e-6 {
            break;
        }
    }
    score = objective(&current, &edits, 1.0);
    let mut best_error = distance(&current, &goal, opts);
    // Fit the palette last: changing a hue band activates the equalizer's guided filters
    // and RBF tables. Band-chroma errors supply one joint direction; a real render supplies
    // its derivative and verifies the bounded step. The output is eight editable sliders.
    if solver_opts.color_adjust && opts.adjustments.color_editor && shared.is_none() {
        for axis in ["sat", "hue", "lum"] {
            let mut delta: [f64; 8] = std::array::from_fn(|i| {
                let feature = match axis {
                    "hue" => 33 + i,
                    "lum" => 41 + i,
                    _ => 17 + i,
                };
                if current.support[feature].min(goal.support[feature]) < 0.05 {
                    return 0.0;
                }
                match axis {
                    "hue" => (((goal.features[feature] - current.features[feature] + 0.5).rem_euclid(1.0) - 0.5) * 200.0 * std::f64::consts::TAU)
                        .clamp(-40.0, 40.0),
                    "lum" => ((goal.features[feature] - current.features[feature]) * 100.0).clamp(-30.0, 30.0),
                    _ => ((goal.features[feature] / current.features[feature].max(1e-5) - 1.0) * 200.0).clamp(-40.0, 40.0),
                }
            });
            if axis == "lum" {
                let enabled = std::array::from_fn::<_, 8, _>(|i| current.support[41 + i].min(goal.support[41 + i]) >= 0.05);
                let total = (0..8).filter(|i| enabled[*i]).map(|i| current.band_mass[i]).sum::<f64>();
                let mean = (0..8).filter(|i| enabled[*i]).map(|i| delta[i] * current.band_mass[i]).sum::<f64>() / total.max(1e-9);
                for i in 0..8 {
                    if enabled[i] {
                        delta[i] -= mean;
                    }
                }
            }
            if !delta.iter().any(|d| d.abs() > 0.5) {
                continue;
            }
            if cancel() {
                return None;
            }
            let probe = palette_candidate(&edits, &delta, axis, 1.0);
            // Keep the measured and fitted direction identical near a control bound.
            // Scaling the unclipped proposal would otherwise overshoot a small derivative.
            let direction: [f64; 8] = std::array::from_fn(|i| {
                let id = PALETTE_CONTROLS[i].replace("sat", axis);
                controls::get(&probe, &id).unwrap_or(0.0) - controls::get(&edits, &id).unwrap_or(0.0)
            });
            let candidate = |scale: f64| palette_candidate(&edits, &direction, axis, scale);
            let coarse = evaluate(&materialize(&edits), false);
            let measured = evaluate(&materialize(&probe), false);
            let (mut numerator, mut denominator) = (0.0, 0.0);
            for (i, weight) in mw.iter().enumerate() {
                let response = observation(&measured, i) - observation(&coarse, i);
                numerator += weight * response * (goal.features[i] - observation(&current, i));
                denominator += weight * response * response;
            }
            // The final palette direction needs the same bounded prior as joint tools;
            // otherwise unregularized hue/saturation sliders pile up at their limits.
            for id in PALETTE_CONTROLS {
                let id = id.replace("sat", axis);
                let at = controls::get(&edits, &id).unwrap_or(0.0);
                let direction = (controls::get(&probe, &id).unwrap_or(at) - at) / palette_scale(axis);
                let value = (at - controls::get(&start, &id).unwrap_or(at)) / palette_scale(axis);
                numerator -= PALETTE_PRIOR * direction * value;
                denominator += PALETTE_PRIOR * direction * direction;
            }
            let next_edits = candidate(if denominator > 1e-12 { (numerator / denominator).clamp(-1.0, 2.0) } else { 0.0 });
            if cancel() {
                return None;
            }
            let next = evaluate(&materialize(&next_edits), true);
            let next_score = objective(&next, &next_edits, 1.0);
            if next_score < score {
                edits = next_edits;
                current = next;
                score = next_score;
            }
        }
        best_error = distance(&current, &goal, opts);
    }
    // Palette / grading adjustments can move brightness as a side effect. Refit the
    // normalized median once, against the same adapted reachable goal, after the style fit.
    if opts.exposure_enabled() && (current.features[2] - goal.features[2]).abs() > 0.005 {
        let p = Parameter::Slider("light.exposure", 0.15);
        let mut candidate = edits.clone();
        let correction = 3.0 * (goal.features[2].max(0.01) / current.features[2].max(0.01)).log2();
        p.set(&mut candidate, p.bounded(p.value(&edits) + correction, &start, wb, opts.new_layer));
        if cancel() {
            return None;
        }
        let measured = evaluate(&materialize(&candidate), true);
        if objective(&measured, &candidate, 1.0) < objective(&current, &edits, 1.0) {
            edits = candidate;
            current = measured;
            best_error = distance(&current, &goal, opts);
        }
    }
    preserve_excluded(&start, &mut edits, opts);
    let full_settings = materialize(&edits);
    let mut faded = fade_controls(&start, &edits, opts);
    if shared.is_some() {
        shared_amount(&start, &edits, opts, &mut faded);
    }
    let mut settings = if shared.is_some() || best_error + 1e-10 < before_error {
        if opts.strength == 100.0 { full_settings.clone() } else { materialize(&faded) }
    } else {
        base.clone()
    };
    if opts.consistent_batch && opts.strength == 0.0 && opts.color_adjust && opts.adjustments.black_white {
        settings.treatment = base.treatment;
    }
    if cancel() {
        return None;
    }
    let after_error = if settings == full_settings {
        distance(&current, &goal, opts)
    } else if settings == *base {
        before_error
    } else {
        distance(&evaluate(&settings, true), &goal, opts)
    };
    renders.set(renders.get() + probe_renders);
    Some(Solved {
        monochrome,
        reach,
        fit: (1.0 - after_error / before_error.max(1e-12)).clamp(0.0, 1.0),
        goal,
        settings,
        full_settings,
        before_error,
        after_error,
        weights: w,
        similarity,
        renders: renders.get(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(gain: f32) -> Arc<Rgb32f> {
        Arc::new(Rgb32f::from_fn(64, 48, |x, y| {
            let v = 0.02 + 0.5 * x as f32 / 63.0;
            if y < 32 { [v * gain; 3] } else { [v * 1.3 * gain, v * 0.8 * gain, v * 0.6 * gain] }
        }))
    }
    fn solve_to(src: &Arc<Rgb32f>, base: &DevelopSettings, reference: &Arc<Rgb32f>, edits: &DevelopSettings, opts: Options) -> Solved {
        let info = SourceInfo::default();
        solve(src, &info, base, &[analyze(reference, &info, edits)], opts, &|| false).unwrap()
    }
    #[test]
    fn match_look_revision2_monochrome_treatment_amount_reset_and_target() {
        let src = source(1.0);
        let base = DevelopSettings::default();
        let reference = DevelopSettings { treatment: Treatment::Bw, ..Default::default() };
        let refs = [analyze(&src, &SourceInfo::default(), &reference)];
        for new_layer in [false, true] {
            let opts = Options { new_layer, strength: 50.0, ..Default::default() };
            let result = solve(&src, &SourceInfo::default(), &base, &refs, opts, &|| false).unwrap();
            assert!(result.monochrome);
            if new_layer {
                assert_eq!(result.settings.treatment, Treatment::Bw);
                assert_eq!(result.settings.masks.last().unwrap().tools.treatment, None);
            } else {
                assert_eq!(result.settings.treatment, Treatment::Bw);
                assert_eq!(result.settings.color, base.color);
            }
            let amount = amount_settings(&base, &result.full_settings, opts);
            if new_layer {
                assert_eq!(amount.treatment, Treatment::Bw);
                assert_eq!(amount.masks.last().unwrap().tools.treatment, None);
            } else {
                assert_eq!(amount.treatment, Treatment::Bw);
            }
            let reset = reset_group(&base, &result.full_settings, opts, "color").unwrap();
            if new_layer {
                assert_eq!(reset.treatment, base.treatment);
                assert_eq!(reset.masks.last().unwrap().tools.treatment, None);
            } else {
                assert_eq!(reset.treatment, base.treatment);
            }
            assert!(result.after_error < result.before_error * 0.3, "{} {}", result.after_error, result.before_error);
        }
        for new_layer in [false, true] {
            let result = solve(
                &src,
                &SourceInfo::default(),
                &reference,
                &[analyze(&src, &SourceInfo::default(), &base)],
                Options { new_layer, ..Default::default() },
                &|| false,
            )
            .unwrap();
            assert_eq!(result.settings.treatment, Treatment::Bw);
            assert_eq!(result.settings.color, reference.color);
            assert_eq!(result.settings.grading, reference.grading);
            assert!(result.after_error.is_finite());
            if new_layer && result.settings.masks.len() > reference.masks.len() {
                let tools = &result.settings.masks.last().unwrap().tools;
                assert!(tools.treatment.is_none() && tools.color.is_none() && tools.grading.is_none());
            }
        }
        let disabled = solve(&src, &SourceInfo::default(), &base, &refs, Options { color_adjust: false, ..Default::default() }, &|| false).unwrap();
        assert_eq!(disabled.settings.treatment, base.treatment);
    }
    #[test]
    fn match_look_revision2_feature_mad_floor_and_float_quantization() {
        let samples = [
            (0.40, 1.0),
            (0.401, 1.0),
            (0.402, 1.0),
            (0.403, 1.0),
            (0.404, 1.0),
            (0.405, 1.0),
            (0.406, 1.0),
            (0.480, 1.0),
            (0.481, 1.0),
            (0.482, 1.0),
        ];
        let references = samples
            .iter()
            .map(|(lightness, _)| {
                let mut look = analyze(&source(1.0), &SourceInfo::default(), &DevelopSettings::default());
                look.features[2] = *lightness;
                look
            })
            .collect::<Vec<_>>();
        let goal = blend(&references, &[0.1; 10]);
        assert_eq!(goal.features[2], weighted_median_floor(&samples, 0.03));
        assert!(goal.features[2] > weighted_median_floor(&samples, 0.005));
        assert_eq!(weighted_median_floor(&[(0.1, 1.0), (0.2, 1.0), (0.9, 1.0)], 0.005), 0.2);
        let make =
            |value| crate::DeepImage { width: 1, height: 1, space: crate::OutputSpace::Srgb, samples: crate::DeepSamples::F32(vec![value; 3]) };
        let a = make(0.2);
        let b = make(0.2001);
        assert_eq!(a.to_rgba8(), b.to_rgba8());
        assert!(measure_deep(&b).features[2] > measure_deep(&a).features[2]);
    }
    #[test]
    fn match_look_revision2_warm_surface_has_no_face_hue_constraint() {
        let src = Arc::new(Rgb32f::from_fn(64, 48, |x, y| [0.075 + x as f32 * 0.0001, 0.04 + y as f32 * 0.0001, 0.03]));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let faces = [[0.0, 0.0, 1.0, 1.0]];
        let before = analyze_with_faces(&src, &info, &base, &faces);
        let hue = |look: &Look| look.features[16].atan2(look.features[15]);
        let shift = |look: &Look| {
            ((hue(look) - hue(&before) + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI).abs().to_degrees()
        };
        // Construct a rendered 60-degree cast, independent of the UI wheel's HSV hue domain.
        let mut reference = base.clone();
        reference.grading.global.hue = 240.0;
        let (mut low, mut high) = (0.0, 100.0);
        for _ in 0..24 {
            reference.grading.global.sat = (low + high) * 0.5;
            let look = analyze_with_faces(&src, &info, &reference, &faces);
            if shift(&look) < 60.0 {
                low = reference.grading.global.sat;
            } else {
                high = reference.grading.global.sat;
            }
        }
        let look = analyze_with_faces(&src, &info, &reference, &faces);
        assert!((shift(&look) - 60.0).abs() < 0.01);
        let opts = Options { normalize: false, light_contrast: false, color_adjust: false, ..Default::default() };
        let no_face = solve_with_faces(&src, &info, &base, std::slice::from_ref(&look), opts, &[], &|| false).unwrap();
        let no_face_shift = shift(&analyze_with_faces(&src, &info, &no_face.settings, &faces));
        let with_face = solve_with_faces(&src, &info, &base, &[look], opts, &faces, &|| false).unwrap();
        let face_shift = shift(&analyze_with_faces(&src, &info, &with_face.settings, &faces));
        println!("MATCH_LOOK_SKIN goal_shift=60 no_faces={no_face_shift:.3} faces={face_shift:.3}");
        assert!(no_face_shift >= 40.0, "warm surface should be free to change hue: {no_face_shift}");
        assert!(face_shift <= 20.0, "cached skin remains protected: {face_shift}");
    }
    #[test]
    fn match_look_revision2_raw_clipped_proxy_and_whites() {
        let src = Arc::new(Rgb32f::from_fn(512, 384, |x, _| if x > 410 { [2.0; 3] } else { [0.08 + x as f32 / 700.0; 3] }));
        let info = SourceInfo {
            raw: true,
            raw_clip_level: Some(1.0),
            clip_confidence: Some(Arc::new(crate::ClipConfidence {
                width: 512,
                height: 384,
                data: (0..512 * 384).map(|i| if i % 512 > 410 { 1.0 } else { 0.0 }).collect(),
            })),
            look_curve: crate::basecurves::camera("Sony", "ILCE-1"),
            ..Default::default()
        };
        let mut goal = DevelopSettings::default();
        goal.light.whites = -30.0;
        goal.light.highlights = -20.0;
        let reference = analyze(&src, &info, &goal);
        let result = solve(
            &src,
            &info,
            &DevelopSettings::default(),
            &[reference],
            Options { normalize: false, color_tone: false, color_adjust: false, ..Default::default() },
            &|| false,
        )
        .unwrap();
        assert!(result.after_error < result.before_error * 0.5, "clipped highlights must improve against the reachable goal");
        let full = measure_rendered(
            &crate::render(&src, &info, &result.settings, &RenderRequest { depth: crate::OutputDepth::F32Linear, ..RenderRequest::fit(512, 512) }),
            &[],
            true,
        );
        let small = analyze(&src, &info, &result.settings);
        for i in [0, 2, 4] {
            assert!((small.features[i] - full.features[i]).abs() < 0.01, "feature {i}");
        }
    }

    #[test]
    fn match_look_self_match_is_exact_noop() {
        let src = source(1.0);
        let mut d = DevelopSettings::default();
        d.light.contrast = 32.0;
        d.color.saturation = -12.0;
        for new_layer in [false, true] {
            let s = solve_to(&src, &d, &src, &d, Options { new_layer, ..Default::default() });
            assert_eq!(s.settings, d);
            assert_eq!(s.after_error, 0.0);
        }
    }
    #[test]
    fn match_look_differs_from_sync_under_different_light() {
        let reference = source(1.0);
        let target = source(0.4);
        let mut edited = DevelopSettings::default();
        edited.light.exposure = 0.3;
        edited.light.contrast = 15.0;
        let opts = Options { color_adjust: false, color_tone: false, ..Default::default() };
        let solved = solve_to(&target, &DevelopSettings::default(), &reference, &edited, opts);
        let goal = analyze(&reference, &SourceInfo::default(), &edited);
        let sync = analyze(&target, &SourceInfo::default(), &edited);
        assert!(solved.after_error < distance(&sync, &goal, opts) * 0.5, "{} vs sync {}", solved.after_error, distance(&sync, &goal, opts));
        assert!((solved.settings.light.exposure - edited.light.exposure).abs() > 0.5);
    }
    #[test]
    fn match_look_similarity_weights_ten_references_and_consistent_blend() {
        let mut refs = vec![analyze(&source(1.0), &SourceInfo::default(), &DevelopSettings::default()); 10];
        for (i, r) in refs.iter_mut().enumerate() {
            r.signature = [0.0; 10];
            r.signature[i % 8] = 1.0;
            r.features[2] = i as f64 / 10.0;
        }
        let w = weights(&refs, &refs[0].signature, false);
        assert!(w[0] > w[1] * 100.0);
        assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        let w = weights(&refs, &refs[0].signature, true);
        assert_eq!(w, vec![0.1; 10]);
        assert!((blend(&refs, &w).features[2] - 0.45).abs() < 1e-12);
    }
    #[test]
    fn match_look_disabled_groups_preserve_controls() {
        let mut d = DevelopSettings::default();
        d.color.vibrance = 11.0;
        d.grading.highlights.hue = 211.0;
        d.grading.highlights.sat = 9.0;
        let opts = Options { normalize: false, color_adjust: false, color_tone: false, ..Default::default() };
        let s = solve_to(&source(0.5), &d, &source(1.0), &DevelopSettings::default(), opts);
        assert_eq!(s.settings.wb, d.wb);
        assert_eq!(s.settings.color, d.color);
        assert_eq!(s.settings.grading, d.grading);
        assert_eq!(s.settings.crop, d.crop);
        assert_eq!(s.settings.detail, d.detail);
        assert!(s.after_error < s.before_error);
        let no = Options { normalize: false, light_contrast: false, color_adjust: false, color_tone: false, ..Default::default() };
        assert_eq!(solve_to(&source(0.5), &d, &source(1.0), &DevelopSettings::default(), no).settings, d);
    }
    #[test]
    fn match_look_strength_zero_half_full_are_editable() {
        let src = source(0.5);
        let r = source(1.0);
        let base = DevelopSettings::default();
        let opts = Options { color_adjust: false, color_tone: false, ..Default::default() };
        let full = solve_to(&src, &base, &r, &base, opts);
        let half = solve_to(&src, &base, &r, &base, Options { strength: 50.0, ..opts });
        assert_eq!(full.settings.light.exposure, half.settings.light.exposure, "Normalize ignores style Strength");
        let zero = solve_to(&src, &base, &r, &base, Options { strength: 0.0, ..opts });
        assert_eq!(zero.settings.light.exposure, full.settings.light.exposure);
        assert_eq!(zero.settings.light.contrast, base.light.contrast);
        assert_eq!(zero.settings.curve, base.curve);
        assert!(half.after_error < half.before_error);
        // With Normalize fixed, style amount is a slider interpolation rather than a promise
        // of monotonically decreasing distribution error (users may prefer a partial style).
        for id in [
            "light.contrast",
            "light.whites",
            "light.blacks",
            "light.shadows",
            "light.highlights",
            "curve.shadows",
            "curve.darks",
            "curve.lights",
            "curve.highlights",
        ] {
            let from = controls::get(&base, id).unwrap();
            let to = controls::get(&full.settings, id).unwrap();
            let mid = controls::get(&half.settings, id).unwrap();
            assert!((mid - (from + (to - from) * 0.5)).abs() < 1e-9, "Style fade {id}");
        }
    }
    #[test]
    fn match_look_new_layer_builds_on_finished_stack() {
        let mut base = DevelopSettings::default();
        base.light.contrast = 12.0;
        let src = source(0.5);
        let reference = source(1.0);
        let opts = Options { new_layer: true, color_adjust: false, color_tone: false, ..Default::default() };
        let s = solve_to(&src, &base, &reference, &DevelopSettings::default(), opts);
        assert_eq!(s.settings.light, base.light);
        assert_eq!(s.settings.masks.len(), 1);
        assert_eq!(s.settings.masks[0].components[0].shape, MaskShape::Full);
        assert!(s.settings.masks[0].tools.light.unwrap().exposure > 0.0);
        assert!(s.after_error < s.before_error * 0.25);
        let second = solve_to(&src, &s.settings, &source(1.2), &DevelopSettings::default(), opts);
        assert_eq!(second.settings.masks.len(), 2);
        assert_eq!(second.settings.masks[0], s.settings.masks[0]);
    }
    #[test]
    fn match_look_skin_window_protects_portrait_features() {
        let src = Arc::new(Rgb32f::from_fn(64, 48, |x, y| if y < 32 { [0.03 + x as f32 * 0.006; 3] } else { [0.3, 0.15, 0.09] }));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let reference = analyze(&src, &info, &base);
        assert!(reference.support[14] > 0.5, "skin support {:?}", reference.support);
        let cast = Arc::new(Rgb32f::from_fn(64, 48, |x, y| {
            let p = src.get(x, y);
            [p[0] * 0.7, p[1] * 0.8, p[2]]
        }));
        let s = solve(&cast, &info, &base, std::slice::from_ref(&reference), Options::default(), &|| false).unwrap();
        let after = analyze(&cast, &info, &s.settings);
        let before = analyze(&cast, &info, &base);
        let skin = |l: &Look| (14..17).map(|i| (l.features[i] - reference.features[i]).powi(2)).sum::<f64>();
        assert!(skin(&after) < skin(&before), "{} -> {}", skin(&before), skin(&after));
    }
    #[test]
    fn match_look_deterministic_at_one_and_four_threads() {
        let src = source(0.5);
        let refs = [analyze(&source(1.0), &SourceInfo::default(), &DevelopSettings::default())];
        let run = |n| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build()
                .unwrap()
                .install(|| solve(&src, &SourceInfo::default(), &DevelopSettings::default(), &refs, Options::default(), &|| false).unwrap())
        };
        let one = run(1);
        let four = run(4);
        assert_eq!(one.settings, four.settings);
        assert_eq!(one.after_error, four.after_error);
        assert_eq!(one.weights, four.weights);
        let batch = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap().install(|| {
            solve_batch_with_faces(&src, &SourceInfo::default(), &DevelopSettings::default(), &refs, Options::default(), &[], &|| false).unwrap()
        });
        assert_eq!(one.settings, batch.settings);
        assert_eq!(one.after_error, batch.after_error);
    }
    #[test]
    fn match_look_cancel_and_invalid_analysis_never_produce_edits() {
        let src = source(0.5);
        let info = SourceInfo::default();
        let d = DevelopSettings::default();
        let r = analyze(&source(1.0), &info, &d);
        let count = std::cell::Cell::new(0);
        assert!(
            solve(&src, &info, &d, std::slice::from_ref(&r), Options::default(), &|| {
                count.set(count.get() + 1);
                count.get() > 6
            })
            .is_none()
        );
        assert!(solve(&src, &info, &d, &vec![r.clone(); MAX_REFERENCES + 1], Options::default(), &|| false).is_none());
        let mut invalid = r;
        invalid.features[0] = f64::NAN;
        assert!(solve(&src, &info, &d, &[invalid], Options::default(), &|| false).is_none());
        let empty = measure(&Rgba8::new(0, 0));
        assert!(empty.valid());
        assert_eq!(empty.support, [0.0; FEATURES]);
    }
    #[test]
    fn match_look_color_tone_solves_editable_grading() {
        let src = source(1.0);
        let base = DevelopSettings::default();
        let mut edited = base.clone();
        edited.grading.shadows.hue = 225.0;
        edited.grading.shadows.sat = 20.0;
        edited.grading.highlights.hue = 45.0;
        edited.grading.highlights.sat = 15.0;
        let opts = Options { normalize: false, light_contrast: false, color_adjust: false, ..Default::default() };
        let s = solve_to(&src, &base, &src, &edited, opts);
        assert!(s.after_error < s.before_error * 0.6, "{} -> {}", s.before_error, s.after_error);
        assert_eq!(s.settings.light, base.light);
        assert_eq!(s.settings.wb, base.wb);
        assert_ne!(s.settings.grading, base.grading);
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn match_look_can_move_inwards_from_slider_maximum() {
        let source = Arc::new(Rgb32f::from_fn(64, 48, |x, _| [0.0002 + x as f32 * 0.00007; 3]));
        let info = SourceInfo::default();
        let mut base = DevelopSettings::default();
        base.light.exposure = 5.0;
        let mut reference = base.clone();
        reference.light.exposure = 3.0;
        let look = analyze(&source, &info, &reference);
        let options = Options { light_contrast: false, color_adjust: false, color_tone: false, ..Default::default() };
        let result = solve(&source, &info, &base, &[look], options, &|| false).unwrap();
        assert!(result.settings.light.exposure < 5.0);
        assert!(result.after_error < result.before_error);
    }
    #[test]
    fn match_look_strength_preserves_high_existing_grading_amounts() {
        let source = Arc::new(Rgb32f::from_fn(64, 48, |x, _| [0.01 + x as f32 * 0.004; 3]));
        let info = SourceInfo::default();
        let mut base = DevelopSettings::default();
        base.grading.shadows.hue = 45.0;
        base.grading.shadows.sat = 100.0;
        let look = analyze(&source, &info, &DevelopSettings::default());
        let options = Options { normalize: false, light_contrast: false, color_adjust: false, ..Default::default() };
        let full = solve(&source, &info, &base, std::slice::from_ref(&look), options, &|| false).unwrap();
        let half = solve(&source, &info, &base, &[look], Options { strength: 50.0, ..options }, &|| false).unwrap();
        for y in [false, true] {
            let parameter = Parameter::Grade("shadows", y);
            let expected = (parameter.value(&base) + parameter.value(&full.settings)) * 0.5;
            assert!((parameter.value(&half.settings) - expected).abs() < 1e-8);
        }
        assert!(half.settings.grading.shadows.sat > 60.0);
    }
}

#[cfg(test)]
mod palette_proxy_tests {
    use super::*;
    #[test]
    fn match_look_palette_correction_is_measured_and_editable() {
        let src = Arc::new(Rgb32f::from_fn(128, 64, |x, y| {
            let v = 0.08 + y as f32 * 0.003;
            if x < 64 { [v * 1.5, v * 0.5, v * 0.2] } else { [v * 0.2, v * 0.45, v * 1.5] }
        }));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let mut reference = base.clone();
        reference.mixer.orange.sat = 30.0;
        reference.mixer.blue.sat = -45.0;
        let look = analyze(&src, &info, &reference);
        let options = Options { normalize: false, light_contrast: false, color_tone: false, ..Default::default() };
        let solved = solve(&src, &info, &base, &[look], options, &|| false).unwrap();
        assert!(solved.after_error < solved.before_error * 0.7, "{} -> {}", solved.before_error, solved.after_error);
        assert!(!solved.settings.mixer.is_neutral());
        assert!(solved.renders < 30);
        assert_eq!(solved.settings.wb, base.wb);
        assert_eq!(solved.settings.light, base.light);
    }
    #[test]
    fn match_look_small_source_solve_improves_independent_large_render() {
        let source = |gain| {
            Arc::new(Rgb32f::from_fn(512, 384, |x, y| {
                let v = 0.02 + x as f32 * 0.0008 + y as f32 * 0.0001;
                if y < 260 { [v * gain; 3] } else { [v * 1.3 * gain, v * 0.8 * gain, v * 0.6 * gain] }
            }))
        };
        let reference = source(1.0);
        let target = source(0.4);
        let info = SourceInfo { raw: true, ..Default::default() };
        let base = DevelopSettings::default();
        let mut edited = base.clone();
        edited.light.exposure = 0.25;
        edited.light.contrast = 15.0;
        let look = analyze(&reference, &info, &edited);
        let options = Options { color_adjust: false, color_tone: false, ..Default::default() };
        let solved = solve(&target, &info, &base, &[look], options, &|| false).unwrap();
        let req = RenderRequest::fit(512, 512);
        let goal = measure(&crate::render(&reference, &info, &edited, &req).image);
        let synced = measure(&crate::render(&target, &info, &edited, &req).image);
        let matched = measure(&crate::render(&target, &info, &solved.settings, &req).image);
        assert!(distance(&matched, &goal, options) < distance(&synced, &goal, options) * 0.5);
    }
}

#[cfg(test)]
mod v2_tests {
    use super::*;
    fn source() -> Arc<Rgb32f> {
        Arc::new(Rgb32f::from_fn(64, 48, |x, y| {
            let v = 0.01 + x as f32 * 0.006;
            if y < 24 { [v; 3] } else { [v * 1.1, v * 0.8, v * 0.6] }
        }))
    }
    #[test]
    fn match_look_v2_cdf_monotone_populated_slopes_and_outlier_median() {
        let mut a = measure(&Rgba8::from_fn(64, 48, |x, _| [x as u8 * 3; 4]));
        let mut b = a.clone();
        a.features[..5].copy_from_slice(&[0.1, 0.3, 0.5, 0.7, 0.9]);
        b.features[..5].copy_from_slice(&[0.49, 0.495, 0.5, 0.505, 0.51]);
        let curve = cdf_curve(&a, &b);
        for i in 1..5 {
            let slope = (curve[i] - curve[i - 1]) / (a.features[i] - a.features[i - 1]);
            assert!((0.499999..=2.000001).contains(&slope));
        }
        assert!((weighted_median(&[(0.4, 1.0), (0.41, 1.0), (0.42, 1.0), (0.95, 1.0)]) - 0.41).abs() < 0.015);
        assert_eq!(weighted_median(&[(0.2, 0.5), (0.8, 0.5)]), 0.5);
    }
    #[test]
    fn match_look_v2_raw_presets_effective_start_normalize_and_locks() {
        let src = source();
        let info = SourceInfo { raw: true, relative_wb: false, as_shot_temp: 4300.0, as_shot_tint: 8.0, ..Default::default() };
        for mode in [WbMode::Tungsten, WbMode::Daylight, WbMode::Auto, WbMode::AsShot] {
            let mut preset = DevelopSettings::default();
            preset.wb.mode = mode;
            preset.wb.temp = 11800.0;
            preset.wb.tint = 37.0;
            let wb = crate::auto::current_wb(&info, &preset);
            let mut effective = preset.clone();
            effective.wb.mode = WbMode::Custom;
            effective.wb.temp = wb.0;
            effective.wb.tint = wb.1;
            let mut reference = effective.clone();
            reference.light.exposure = 0.4;
            reference.wb.temp *= 1.1;
            let looks = [analyze(&src, &info, &reference)];
            let opts = Options { color_tone: false, color_adjust: false, strength: 50.0, ..Default::default() };
            let a = solve(&src, &info, &preset, &looks, opts, &|| false).unwrap();
            let b = solve(&src, &info, &effective, &looks, opts, &|| false).unwrap();
            assert!((a.settings.wb.temp - b.settings.wb.temp).abs() < 0.01, "{mode:?}: {} vs {}", a.settings.wb.temp, b.settings.wb.temp);
            assert!((a.settings.wb.tint - b.settings.wb.tint).abs() < 0.01);
            let locked = solve(&src, &info, &preset, &looks, Options { keep_white_balance: true, keep_exposure: true, ..opts }, &|| false).unwrap();
            assert_eq!(locked.settings.wb, preset.wb);
            assert_eq!(locked.settings.light.exposure, preset.light.exposure);
        }
    }
    #[test]
    fn match_look_v2_relative_layer_wb_tracks_later_global_temperature() {
        let src = source();
        let info = SourceInfo::default();
        let mut base = DevelopSettings::default();
        base.wb.mode = WbMode::Custom;
        let mut reference = base.clone();
        reference.wb.temp = 9000.0;
        reference.wb.tint = 10.0;
        let opts = Options { new_layer: true, light_contrast: false, color_adjust: false, color_tone: false, ..Default::default() };
        let result = solve(&src, &info, &base, &[analyze(&src, &info, &reference)], opts, &|| false).unwrap();
        let mask = result.settings.masks.last().unwrap();
        assert!(mask.tools.wb.is_none());
        assert!(mask.adjust.temp.abs() > 0.01);
        let a = crate::render(&src, &info, &result.settings, &RenderRequest::fit(64, 48)).image;
        let mut later = result.settings;
        later.wb.temp = 4500.0;
        let b = crate::render(&src, &info, &later, &RenderRequest::fit(64, 48)).image;
        assert_ne!(a, b, "Relative layer WB must retain global WB edits");
    }
    #[test]
    fn match_look_v2_style_amount_overdrive_and_normalization_kept() {
        let mut start = DevelopSettings::default();
        start.light.contrast = 12.0;
        start.wb.temp = 2850.0;
        let mut full = start.clone();
        full.light.exposure = 3.2;
        full.wb.mode = WbMode::Custom;
        full.wb.temp = 13000.0;
        full.light.contrast = 32.0;
        full.curve.shadows = 20.0;
        let opts = Options { strength: 125.0, ..Default::default() };
        let d = amount_settings(&start, &full, opts);
        assert_eq!(d.light.exposure, 3.2);
        assert_eq!(d.wb.temp, 13000.0);
        assert_eq!(d.light.contrast, 37.0);
        let reset = reset_group(&start, &full, opts, "light").unwrap();
        assert_eq!(reset.light.contrast, start.light.contrast);
        assert_eq!(reset.light.exposure, 3.2);
        assert_eq!(reset.curve, start.curve);
    }
    #[test]
    fn match_look_v2_cached_faces_exclude_brown_background_and_need_no_model() {
        let image = Rgba8::from_fn(40, 40, |x, _| if x < 20 { [125, 70, 42, 255] } else { [195, 150, 113, 255] });
        let all = measure(&image);
        let face = measure_with_faces(&image, &[[0.5, 0.0, 0.5, 1.0]]);
        assert!(face.support[14] > 0.1);
        assert!(face.features[14] > all.features[14] + 0.02);
    }
    #[test]
    fn match_look_v2_face_skin_hue_guard_and_crop_mapping() {
        let src = Arc::new(Rgb32f::from_fn(64, 48, |x, _| if x < 32 { [0.06, 0.025, 0.013] } else { [0.12, 0.16, 0.20] }));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let faces = [[0.0, 0.0, 0.5, 1.0]];
        let mut reference = base.clone();
        reference.grading.midtones.hue = 240.0;
        reference.grading.midtones.sat = 70.0;
        let look = analyze_with_faces(&src, &info, &reference, &faces);
        let opts = Options { normalize: false, light_contrast: false, color_adjust: false, ..Default::default() };
        let result = solve_with_faces(&src, &info, &base, &[look], opts, &faces, &|| false).unwrap();
        let before = analyze_with_faces(&src, &info, &base, &faces);
        let after = analyze_with_faces(&src, &info, &result.settings, &faces);
        let delta = (after.features[16].atan2(after.features[15]) - before.features[16].atan2(before.features[15])).abs();
        assert!(delta <= 20f64.to_radians(), "Skin hue moved {} degrees", delta.to_degrees());
        let mut crop = base.clone();
        crop.crop.geometry.rect = lightcraft_geom::Rect::from_xywh(0.0, 0.0, 0.5, 1.0);
        assert_eq!(framed_faces(&src, &info, &crop, &faces), vec![[0.0, 0.0, 1.0, 1.0]]);
    }
    #[test]
    fn match_look_v2_zero_style_without_normalize_is_noop_even_as_layer() {
        let src = source();
        let base = DevelopSettings::default();
        let mut ref_edits = base.clone();
        ref_edits.color.saturation = 35.0;
        let info = SourceInfo::default();
        let look = analyze(&src, &info, &ref_edits);
        for new_layer in [false, true] {
            let opts = Options { normalize: false, strength: 0.0, new_layer, ..Default::default() };
            let d = solve(&src, &info, &base, std::slice::from_ref(&look), opts, &|| false).unwrap();
            assert_eq!(d.settings, base);
        }
    }
    #[test]
    fn match_look_probe_measurement_features_are_identical_to_analysis() {
        let image = Rgba8::from_fn(64, 48, |x, y| [(x * 4) as u8, (y * 5) as u8, ((x + y) * 2) as u8, 255]);
        let full = measure_with_faces(&image, &[]);
        let probe = measure_impl(&image, &[], false);
        assert_eq!(full.features, probe.features);
        assert_eq!(full.support, probe.support);
        for i in 0..256 {
            assert_eq!(SRGB_DECODE[i], srgb_to_linear(i as f32 / 255.0));
        }
    }
    #[test]
    fn match_look_fitted_curve_and_overdrive_have_no_plateaus() {
        let start = DevelopSettings::default();
        let mut d = start.clone();
        d.curve.darks = 53.0;
        d.curve.lights = -47.0;
        d.curve.highlights = -33.0;
        d.curve.shadows = -15.0;
        guard_curve(&start, &mut d);
        assert!(d.curve.darks < 53.0);
        for strength in [100.0, 125.0] {
            let faded = fade_controls(&start, &d, Options { strength, ..Default::default() });
            let lut = crate::finish::curve_luts(&faded.curve).unwrap();
            let values = &lut.tables[0].v;
            for pair in values.windows(2) {
                let slope = (pair[1] - pair[0]) * (values.len() - 1) as f32;
                assert!(slope > 0.45 && slope < 2.05, "Unsafe fitted slope {slope}");
            }
        }
    }
}

#[cfg(test)]
mod revision3_tests {
    use super::*;
    fn mono(lo: f32, hi: f32, edge: usize) -> Arc<Rgb32f> {
        Arc::new(Rgb32f::from_fn(edge, edge * 3 / 4, |x, _| [lo + (hi - lo) * x as f32 / (edge - 1) as f32; 3]))
    }
    #[test]
    fn match_look_revision3_statistics_render_preserves_complete_deep_pipeline() {
        let src = mono(0.005, 1.4, 128);
        let info = SourceInfo { raw: true, raw_clip_level: Some(1.0), ..Default::default() };
        let mut base = DevelopSettings::default();
        base.light.exposure = 0.2;
        base.grading.shadows.hue = 35.0;
        base.grading.shadows.sat = 12.0;
        base.crop.geometry.rect = lightcraft_geom::Rect::from_xywh(0.1, 0.1, 0.8, 0.8);
        let wb = crate::auto::current_wb(&info, &base);
        for settings in [
            base.clone(),
            layer_settings(
                &base,
                &DevelopSettings { light: lightcraft_develop::Light { contrast: 10.0, ..Default::default() }, ..Default::default() },
                Options::default(),
                wb,
            ),
        ] {
            let request = RenderRequest { depth: crate::OutputDepth::F32Linear, ..RenderRequest::fit(128, 128) };
            let full = crate::render_cached(&src, &info, &settings, &request, &StageCache::default());
            let stats = crate::render_cached(&src, &info, &settings, &RenderRequest { statistics_only: true, ..request }, &StageCache::default());
            assert_eq!(full.deep, stats.deep);
            assert!(stats.image.data.is_empty());
            assert_eq!(stats.histogram.total, 0);
            assert_eq!(measure_rendered(&full, &[], false), measure_rendered(&stats, &[], false));
        }
    }
    #[test]
    fn match_look_revision3_thin_structure_native_quantiles() {
        let src = Arc::new(Rgb32f::from_fn(512, 384, |x, _| {
            if x % 16 == 0 {
                [0.02; 3]
            } else if x > 508 {
                [2.0; 3]
            } else {
                [0.3; 3]
            }
        }));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let full = measure_rendered(
            &crate::render(&src, &info, &base, &RenderRequest { depth: crate::OutputDepth::F32Linear, ..RenderRequest::fit(512, 512) }),
            &[],
            true,
        );
        let analyzed = analyze(&src, &info, &base);
        assert!((analyzed.features[0] - full.features[0]).abs() < 0.01);
        let (small, si) = measurement_source(&src, &info, 128);
        let biased = measure_rendered(
            &crate::render(&small, &si, &base, &RenderRequest { depth: crate::OutputDepth::F32Linear, ..RenderRequest::fit(128, 128) }),
            &[],
            true,
        );
        assert!((biased.features[0] - full.features[0]).abs() > 0.04, "fixture must catch the former resolution defect");
        let mut target = base.clone();
        target.light.exposure = 0.2;
        let reference = analyze(&src, &info, &target);
        let solved = solve(
            &src,
            &info,
            &base,
            std::slice::from_ref(&reference),
            Options { color_adjust: false, color_tone: false, ..Default::default() },
            &|| false,
        )
        .unwrap();
        let after = analyze(&src, &info, &solved.settings);
        assert!((after.features[0] - solved.goal.features[0]).abs() < 0.015, "{} vs {}", after.features[0], solved.goal.features[0]);
    }
    #[test]
    fn match_look_revision3_reachable_flat_scene() {
        let src = mono(0.09, 0.15, 128);
        let wide = mono(0.01, 0.8, 128);
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let reference = analyze(&wide, &info, &base);
        let result = solve(
            &src,
            &info,
            &base,
            &[reference],
            Options { consistent_batch: true, color_adjust: false, color_tone: false, ..Default::default() },
            &|| false,
        )
        .unwrap();
        println!(
            "FLAT reach={} fit={} error={} before={} contrast={}",
            result.reach, result.fit, result.after_error, result.before_error, result.settings.light.contrast
        );
        assert!(result.reach < 0.6);
        // The original test measured against a narrowed surrogate goal. Preserve the
        // actual wide reference and honestly report the residual for this difficult input.
        assert!(result.after_error < result.before_error);
        assert!(result.after_error > 0.1);
        assert_eq!(result.goal.features[..5], analyze(&wide, &info, &base).features[..5]);
        assert!(result.settings.light.contrast.abs() < 59.0);
        for id in ["curve.shadows", "curve.darks", "curve.lights", "curve.highlights"] {
            assert!(controls::get(&result.settings, id).unwrap().abs() < 54.0);
        }
    }
    #[test]
    fn match_look_revision3_scene_adaptive_median_and_wb_bounds() {
        let src = Arc::new(Rgb32f::from_fn(128, 96, |x, _| {
            let v = 0.5 + x as f32 / 512.0;
            [v * 0.1, v, v]
        }));
        let reference = Arc::new(Rgb32f::from_fn(128, 96, |x, _| {
            let v = 0.015 + x as f32 / 4096.0;
            [v, v * 0.05, v * 0.05]
        }));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let refs = [analyze(&reference, &info, &base)];
        let initial = analyze(&src, &info, &base);
        let result = solve(&src, &info, &base, &refs, Options::default(), &|| false).unwrap();
        let full = solve(&src, &info, &base, &refs, Options { consistent_batch: true, ..Default::default() }, &|| false).unwrap();
        let after = analyze(&src, &info, &result.settings);
        let consistent = analyze(&src, &info, &full.settings);
        println!(
            "ADAPT_GOAL {:?} actual {:?} light {:?} error={} before={}",
            &result.goal.features[..5],
            &after.features[..5],
            result.settings.light,
            result.after_error,
            result.before_error
        );
        println!(
            "ADAPT similarity={} initial={} goal={} after={} consistent={} wb={}",
            result.similarity, initial.features[2], refs[0].features[2], after.features[2], consistent.features[2], result.settings.wb.temp
        );
        assert!(result.similarity < 0.3);
        assert!((0.55..0.75).contains(&after.features[2]));
        assert!((consistent.features[2] - refs[0].features[2]).abs() < 0.04);
        assert!(result.settings.wb.temp / base.wb.temp <= 1.0f64.exp() + 1e-6);
        let color_gap = |look: &Look| (look.features[25] - refs[0].features[25]).hypot(look.features[26] - refs[0].features[26]);
        assert!(color_gap(&after) < color_gap(&initial), "bounded WB and editable Color Tone must move color toward the reference");
        let mut similar_style = base.clone();
        similar_style.light.exposure = 0.3;
        let similar = solve(&src, &info, &base, &[analyze(&src, &info, &similar_style)], Options::default(), &|| false).unwrap();
        assert!(similar.similarity > 0.9);
        assert!((analyze(&src, &info, &similar.settings).features[2] - analyze(&src, &info, &similar_style).features[2]).abs() < 0.04);

        assert!((result.settings.wb.tint - base.wb.tint).abs() <= 40.0 + 1e-6);
        let reference_span = result.goal.features[4] - result.goal.features[0];
        assert!(((after.features[4] - after.features[0]) - reference_span).abs() < 0.03);
    }
    #[test]
    fn match_look_revision3_toned_bw_preserves_color_controls() {
        let src = mono(0.01, 0.7, 128);
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let mut edited = base.clone();
        edited.treatment = Treatment::Bw;
        edited.grading.global.hue = 35.0;
        edited.grading.global.sat = 20.0;
        let reference = analyze(&src, &info, &edited);
        println!(
            "SEPIA c={} variation={} bands={:?} sig={:?}",
            reference.features[7],
            reference.features[49],
            &reference.support[17..25],
            &reference.signature[..8]
        );
        assert!(is_monochrome(&reference));
        let result = solve_with_faces(&src, &info, &base, &[reference], Options::default(), &[[0.0, 0.0, 1.0, 1.0]], &|| false).unwrap();
        assert_eq!(result.settings.treatment, Treatment::Bw);
        assert_eq!(result.settings.color, base.color);
        let hue = result.settings.grading.global.hue;
        assert!((hue - 35.0).abs() < 25.0, "toning hue {hue}");
        assert!(result.after_error < result.before_error * 0.3, "{} -> {}", result.before_error, result.after_error);
    }
    #[test]
    fn match_look_revision3_balanced_split_tone_keeps_bw_treatment() {
        let src = mono(0.01, 0.7, 128);
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let mut desired = base.clone();
        desired.treatment = Treatment::Bw;
        desired.grading.shadows = lightcraft_develop::Wheel { hue: 220.0, sat: 20.0, lum: 0.0 };
        desired.grading.highlights = lightcraft_develop::Wheel { hue: 35.0, sat: 20.0, lum: 0.0 };
        let reference = analyze(&src, &info, &desired);
        assert!(is_monochrome(&reference));
        let result = solve(&src, &info, &base, &[reference], Options::default(), &|| false).unwrap();
        assert_eq!(result.settings.treatment, Treatment::Bw);
        assert_eq!(result.settings.color, base.color);
        assert!(result.after_error < result.before_error * 0.5);
    }
    #[test]
    fn match_look_endpoints_are_editable_and_texture_does_not_fit_clarity() {
        let src = Arc::new(Rgb32f::from_fn(128, 96, |x, y| {
            let v = if x < 16 { 0.001 } else { 0.05 + x as f32 / 200.0 };
            [v + if x >= 16 && (x / 4 + y / 4) % 2 == 0 { 0.01 } else { 0.0 }; 3]
        }));
        let info = SourceInfo::default();
        let mut base = DevelopSettings::default();
        base.light.blacks = 15.0;
        let mut desired = base.clone();
        desired.light.blacks = -15.0;
        let reference = analyze(&src, &info, &desired);
        let result = solve(
            &src,
            &info,
            &base,
            std::slice::from_ref(&reference),
            Options { normalize: false, color_adjust: false, color_tone: false, ..Default::default() },
            &|| false,
        )
        .unwrap();
        let after = analyze(&src, &info, &result.settings);
        println!(
            "SHAPE black={} clarity={} L1={} goal={} fit={}",
            result.settings.light.blacks, result.settings.effects.clarity, after.features[30], result.goal.features[30], result.fit
        );
        assert!(result.settings.light.blacks < base.light.blacks);
        assert!((after.features[30] - result.goal.features[30]).abs() < 0.03);
        let texture = |amplitude: f32| {
            Arc::new(Rgb32f::from_fn(128, 96, |x, y| {
                let v = 0.04 + x as f32 / 180.0;
                [v + if (x / 4 + y / 4) % 2 == 0 { amplitude } else { -amplitude }; 3]
            }))
        };
        let textured = texture(0.008);
        let double = texture(0.016);
        let original = analyze(&textured, &info, &DevelopSettings::default());
        let reference = analyze(&double, &info, &DevelopSettings::default());
        assert!(reference.features[32] > original.features[32] * 1.8, "texture fixture must nearly double local contrast");
        let detail = solve(
            &textured,
            &info,
            &DevelopSettings::default(),
            &[reference],
            Options { normalize: false, color_adjust: false, color_tone: false, ..Default::default() },
            &|| false,
        )
        .unwrap();
        println!("CLARITY doubled texture result={}", detail.settings.effects.clarity);
        assert_eq!(detail.settings.effects, DevelopSettings::default().effects);
        assert_eq!(metric_weights(&original, &detail.goal, Options::default())[32], 0.0);
        let same = solve(&src, &info, &base, &[analyze(&src, &info, &base)], Options::default(), &|| false).unwrap();
        assert_eq!(same.settings.effects.clarity, 0.0);
    }
    #[test]
    fn match_look_revision3_palette_hue_and_luminance_joint_directions() {
        use li_color::perceptual::hsv_to_rgb;
        let matrix = SRGB.to_space(&REC2020);
        let src = Arc::new(Rgb32f::from_fn(256, 128, |x, y| {
            let hue = lightcraft_develop::MIXER_HUES[(x / 32).min(7)] as f32;
            matrix.apply_f32(hsv_to_rgb(hue, 0.65, 0.3 + y as f32 / 200.0).map(srgb_to_linear))
        }));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let mut desired = base.clone();
        controls::set(&mut desired, "mixer.orange.hue", 15.0);
        controls::set(&mut desired, "mixer.blue.hue", -15.0);
        controls::set(&mut desired, "mixer.orange.lum", 10.0);
        controls::set(&mut desired, "mixer.blue.lum", -10.0);
        let reference = analyze(&src, &info, &desired);
        let result = solve(
            &src,
            &info,
            &base,
            &[reference],
            Options { normalize: false, light_contrast: false, color_tone: false, ..Default::default() },
            &|| false,
        )
        .unwrap();
        println!(
            "PALETTE orange={} blue={} orangeLum={} blueLum={} error={} before={} renders={}",
            result.settings.mixer.orange.hue,
            result.settings.mixer.blue.hue,
            result.settings.mixer.orange.lum,
            result.settings.mixer.blue.lum,
            result.after_error,
            result.before_error,
            result.renders
        );
        assert!((result.settings.mixer.orange.hue - 15.0).abs() < 8.0);
        assert!((result.settings.mixer.blue.hue + 15.0).abs() < 8.0);
        assert!(result.settings.mixer.orange.lum > 0.0);
        assert!(result.settings.mixer.blue.lum < 0.0);
        assert!(result.after_error < result.before_error * 0.4);
        assert!(result.renders <= 50);
    }
    #[test]
    fn match_look_revision3_region_support_pin_and_legacy_looks() {
        let src = Arc::new(Rgb32f::from_fn(128, 96, |x, y| {
            if y < 40 {
                [0.15, 0.3, 0.6]
            } else if x > 40 && x < 85 {
                [0.25, 0.12, 0.07]
            } else {
                [0.08, 0.1, 0.04]
            }
        }));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let look = analyze(&src, &info, &base);
        assert!(look.support[50..LEGACY_FEATURES].chunks(3).all(|region| region[0] > 0.05));
        let mut pinned = look.clone();
        pinned.importance = 2.0;
        let w = weights(&[look.clone(), pinned], &look.signature, false);
        assert!((w[1] - 2.0 / 3.0).abs() < 1e-12);
        let mut reference = base.clone();
        reference.grading.highlights.hue = 220.0;
        reference.grading.highlights.sat = 12.0;
        let result =
            solve(&src, &info, &base, &[analyze(&src, &info, &reference)], Options { region_aware: true, ..Default::default() }, &|| false).unwrap();
        assert!(result.after_error < result.before_error);
        let mut value = serde_json::to_value(&look).unwrap();
        for count in [25, 27, 30, 33, 41, 49] {
            value["features"] = serde_json::json!(&look.features[..count]);
            value["support"] = serde_json::json!(&look.support[..count]);
            let old: Look = serde_json::from_value(value.clone()).unwrap();
            assert!(old.support[count..].iter().all(|v| *v == 0.0));
            assert!(old.valid());
        }
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    #[test]
    fn match_look_channel_curves_amount_resets_and_layer_groups() {
        let mut before = DevelopSettings::default();
        before.curve.red = vec![Point::new(0.0, 0.0), Point::new(0.4, 0.35), Point::new(1.0, 1.0)];
        before.effects.clarity = 24.0;
        before.effects.dehaze = 7.0;
        let mut full = before.clone();
        full.light.exposure = 0.8;
        full.curve.darks = 12.0;
        set_channel(&mut full.curve, 0, 0.5, 0.54);
        set_channel(&mut full.curve, 2, 0.75, 0.68);
        let opts = Options::default();
        let zero = amount_settings(&before, &full, Options { strength: 0.0, ..opts });
        assert_eq!(zero.curve, before.curve);
        assert_eq!(zero.effects, before.effects);
        assert_eq!(zero.light.exposure, full.light.exposure);
        let half = amount_settings(&before, &full, Options { strength: 50.0, ..opts });
        assert!((curve_value(&half.curve.blue, 0.75) - 0.715).abs() < 1e-10);
        for amount in [40.0, 65.0, 100.0, 125.0] {
            let faded = amount_settings(&before, &full, Options { strength: amount, ..opts });
            for c in 0..3 {
                let pts = channel(&faded.curve, c);
                assert!(pts.windows(2).all(|p| p[1].y > p[0].y));
                let curve = MonotoneCurve::new(&pts.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>());
                let values = (0..=1024).map(|i| curve.eval(i as f64 / 1024.0)).collect::<Vec<_>>();
                assert!(values.windows(2).all(|p| p[1] > p[0]));
                assert!(values.iter().all(|v| (0.0..=1.0).contains(v)));
            }
        }
        let light = reset_group(&before, &full, opts, "light").unwrap();
        assert_eq!(light.curve.darks, before.curve.darks);
        assert_eq!(light.curve.red, full.curve.red);
        let tone = reset_group(&before, &full, opts, "tone").unwrap();
        assert_eq!(tone.curve.darks, full.curve.darks);
        assert_eq!(tone.curve.red, before.curve.red);
        for (light_contrast, color_tone) in [(true, false), (false, true), (true, true)] {
            let opts = Options { light_contrast, color_tone, new_layer: true, ..opts };
            let mut edits = DevelopSettings::default();
            if light_contrast {
                edits.curve.darks = 12.0;
            }
            if color_tone {
                edits.curve.blue = full.curve.blue.clone();
            }
            let layer = layer_settings(&before, &edits, opts, (6500.0, 0.0));
            assert_eq!(layer.effects, before.effects);
            assert!(layer.masks.last().unwrap().tools.effects.is_none());
            let curve = layer.masks.last().unwrap().tools.curve.as_ref().unwrap();
            assert_eq!(curve, &edits.curve);
            let amount = amount_settings(&before, &layer, Options { strength: 50.0, ..opts });
            let amount_curve = amount.masks.last().unwrap().tools.curve.as_ref().unwrap();
            if color_tone {
                assert!((curve_value(&amount_curve.blue, 0.75) - 0.715).abs() < 1e-10);
            }
        }
    }
    #[test]
    fn match_look_color_tone_fits_sparse_editable_curves_and_excludes_them_when_disabled() {
        let src = Arc::new(Rgb32f::from_fn(128, 96, |x, y| {
            let v = 0.006 + x as f32 / 180.0;
            if y < 48 { [v; 3] } else { [v * 0.8, v * 0.9, v] }
        }));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let mut desired = base.clone();
        desired.curve.red = vec![Point::new(0.0, 0.0), Point::new(0.25, 0.31), Point::new(0.5, 0.5), Point::new(0.75, 0.70), Point::new(1.0, 1.0)];
        desired.curve.blue = vec![Point::new(0.0, 0.0), Point::new(0.25, 0.21), Point::new(0.5, 0.5), Point::new(0.75, 0.80), Point::new(1.0, 1.0)];
        let refs = [analyze(&src, &info, &desired)];
        for new_layer in [false, true] {
            let opts = Options { normalize: false, light_contrast: false, color_adjust: false, new_layer, ..Default::default() };
            let result = solve(&src, &info, &base, &refs, opts, &|| false).unwrap();
            assert!(result.after_error < result.before_error * 0.5, "{} / {}", result.after_error, result.before_error);
            let curve = if new_layer { result.settings.masks.last().unwrap().tools.curve.as_ref().unwrap() } else { &result.settings.curve };
            assert!(!curve.red.is_empty() || !curve.green.is_empty() || !curve.blue.is_empty());
            assert_eq!(result.settings.effects, base.effects);
            let disabled = solve(&src, &info, &base, &refs, Options { color_tone: false, ..opts }, &|| false).unwrap();
            assert_eq!(disabled.settings, base);
        }
    }
    #[test]
    fn match_look_twenty_references_and_legacy_statistics() {
        let src = Arc::new(Rgb32f::from_fn(32, 24, |x, _| [0.01 + x as f32 / 50.0; 3]));
        let look = analyze(&src, &SourceInfo::default(), &DevelopSettings::default());
        assert!(
            solve(&src, &SourceInfo::default(), &DevelopSettings::default(), &vec![look.clone(); MAX_REFERENCES], Options::default(), &|| false)
                .is_some()
        );
        assert!(
            solve(&src, &SourceInfo::default(), &DevelopSettings::default(), &vec![look; MAX_REFERENCES + 1], Options::default(), &|| false)
                .is_none()
        );
    }
}

#[cfg(test)]
mod target_contract_tests {
    use super::*;
    #[test]
    fn match_look_full_pipeline_goal_keeps_reference_tone_and_physical_endpoints() {
        let image = Rgba8::from_fn(64, 48, |x, _| [80 + x as u8; 4]);
        let initial = measure(&image);
        let mut reference = initial.clone();
        reference.features[..5].copy_from_slice(&[0.1, 0.4, 0.6, 0.8, 0.95]);
        reference.features[30] = 0.02;
        reference.features[31] = 0.99;
        let (goal, _) = reachable_goal(&initial, &reference, Options { consistent_batch: true, ..Default::default() }, 1.0);
        assert_eq!(goal.features[..5], reference.features[..5]);
        assert_ne!(goal.features[..5], cdf_curve(&initial, &reference));
        let (adapted, _) = reachable_goal(&initial, &reference, Options::default(), 0.0);
        assert!([0, 1, 2, 3, 4, 14, 30, 31].iter().all(|i| (0.0..=1.0).contains(&adapted.features[*i])));
    }
}
