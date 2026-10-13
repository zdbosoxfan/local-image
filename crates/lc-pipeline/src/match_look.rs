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
use lightcraft_color::{REC2020, SRGB, perceptual::oklab_from_2020, transfer::srgb_to_linear};
use lightcraft_develop::{DevelopSettings, LayerTools, Mask, MaskComponent, MaskShape, Treatment, WbMode, controls};
use lightcraft_raster::{Rgb32f, Rgba8};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const MAX_REFERENCES: usize = 10;
pub const PROXY_EDGE: usize = 60;
const SOURCE_EDGE: usize = PROXY_EDGE;
const FEATURES: usize = 30;
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
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
}
impl Default for Options {
    fn default() -> Self {
        Self {
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
        }
    }
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
    pub clusters: Vec<ColorCluster>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ColorCluster {
    pub ab: [f64; 2],
    pub weight: f64,
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
        if values.len() != 25 && values.len() != 27 && values.len() != FEATURES {
            return Err(serde::de::Error::custom("Invalid look feature count"));
        }
        let mut out = [0.0; FEATURES];
        out[..values.len()].copy_from_slice(&values);
        Ok(out)
    }
}
impl Look {
    pub fn valid(&self) -> bool {
        self.features.iter().chain(&self.support).chain(&self.signature).all(|v| v.is_finite())
            && self.clusters.len() <= 3
            && self
                .clusters
                .iter()
                .all(|c| c.ab.iter().all(|x| x.is_finite() && (-0.5..=0.5).contains(x)) && c.weight.is_finite() && (0.0..=1.0).contains(&c.weight))
            && self.features[..5].windows(2).all(|p| p[0] <= p[1] + 1e-8)
            && self.support.iter().all(|v| *v >= 0.0 && *v <= 1.0)
            && self.features.iter().enumerate().all(|(i, v)| {
                if [0, 1, 2, 3, 4, 7, 14].contains(&i) || (17..25).contains(&i) || i >= 27 {
                    (-0.001..=1.01).contains(v)
                } else {
                    (-0.5..=0.5).contains(v)
                }
            })
            && self.signature.iter().enumerate().all(|(i, v)| if i < 9 { (-0.001..=1.01).contains(v) } else { (-1.01..=1.01).contains(v) })
    }
}

pub fn measure(image: &Rgba8) -> Look {
    measure_with_faces(image, &[])
}
fn measure_with_faces(image: &Rgba8, faces: &[[f32; 4]]) -> Look {
    measure_impl(image, faces, true)
}
// Exact byte-domain decode cache; probe measurements do not need scene descriptors / clusters.
static SRGB_DECODE: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| std::array::from_fn(|i| srgb_to_linear(i as f32 / 255.0)));
fn measure_impl(image: &Rgba8, faces: &[[f32; 4]], descriptor: bool) -> Look {
    let convert = SRGB.to_space(&REC2020);
    measure_samples(
        image.width,
        image.height,
        image.data.iter().map(|p| oklab_from_2020(convert.apply_f32(std::array::from_fn(|i| SRGB_DECODE[p[i] as usize]))).map(f64::from)),
        faces,
        descriptor,
    )
}
fn measure_rendered(image: &crate::Rendered, faces: &[[f32; 4]], descriptor: bool) -> Look {
    match &image.deep {
        Some(deep) => measure_deep_impl(deep, faces, descriptor),
        None => measure_impl(&image.image, faces, descriptor),
    }
}
/// Measure linear float output without quantizing finite-difference probes to eight bits.
pub fn measure_deep(image: &crate::DeepImage) -> Look {
    measure_deep_impl(image, &[], true)
}
fn measure_deep_impl(image: &crate::DeepImage, faces: &[[f32; 4]], descriptor: bool) -> Look {
    let convert = image.space.rgb_space().to_space(&REC2020);
    match &image.samples {
        crate::DeepSamples::F32(pixels) => measure_samples(
            image.width,
            image.height,
            pixels.as_chunks::<3>().0.iter().map(|p| oklab_from_2020(convert.apply_f32(*p)).map(f64::from)),
            faces,
            descriptor,
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
        ),
    }
}
fn measure_samples(width: usize, height: usize, pixels: impl Iterator<Item = [f64; 3]>, faces: &[[f32; 4]], descriptor: bool) -> Look {
    let mut l = Vec::with_capacity(width * height);
    let mut sums = [0.0; FEATURES];
    let mut mass = [0.0; FEATURES];
    let mut hue = [0.0; 8];
    let mut labs = Vec::with_capacity(width * height);
    for (index, lab) in pixels.enumerate() {
        let [ll, a, b] = lab;
        l.push(ll);
        labs.push(lab);
        sums[25] += a;
        sums[26] += b;
        mass[25] += 1.0;
        mass[26] += 1.0;
        let c = a.hypot(b);
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
                sums[17 + band] += c * weight;
                mass[17 + band] += weight;
            }
            if descriptor {
                let h = angle.rem_euclid(std::f64::consts::TAU);
                hue[((h / std::f64::consts::TAU * 8.0) as usize).min(7)] += c;
            }
        }
    }
    l.sort_by(f64::total_cmp);
    let mut features = [0.0; FEATURES];
    let mut support = [0.0; FEATURES];
    if !l.is_empty() {
        for (i, q) in [0.05, 0.25, 0.5, 0.75, 0.95].iter().enumerate() {
            features[i] = l[((l.len() - 1) as f64 * q) as usize];
            support[i] = 1.0;
        }
        for i in 5..FEATURES {
            if mass[i] > 0.0 {
                features[i] = sums[i] / mass[i];
                support[i] = (mass[i] / l.len() as f64 * 20.0).min(1.0);
            }
        }
    }
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
    let clusters = if descriptor { color_clusters(&labs) } else { Vec::new() };
    let total = hue.iter().sum::<f64>();
    let mut signature = [0.0; 10];
    for i in 0..8 {
        signature[i] = hue[i] / total.max(1e-9);
    }
    signature[8] = features[4] - features[0];
    signature[9] = (features[3] - features[2]) - (features[2] - features[1]);
    Look { features, support, signature, clusters }
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
fn cdf_curve(source: &Look, goal: &Look) -> [f64; 5] {
    let x = &source.features;
    let mut y = std::array::from_fn(|i| goal.features[i]);
    for i in (0..2).rev() {
        let gap = x[i + 1] - x[i];
        if gap > 0.002 && x[i] > 0.002 {
            y[i] = y[i].clamp(y[i + 1] - 2.0 * gap, y[i + 1] - 0.5 * gap);
        } else {
            y[i] = x[i];
        }
    }
    for i in 3..5 {
        let gap = x[i] - x[i - 1];
        if gap > 0.002 && x[i] < 0.998 {
            y[i] = y[i].clamp(y[i - 1] + 0.5 * gap, y[i - 1] + 2.0 * gap);
        } else {
            y[i] = x[i];
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
    let (src, info) = measurement_source(src, info, 128);
    let mut req = RenderRequest::fit(128, 128);
    req.depth = crate::OutputDepth::F32Linear;
    let mut look = measure_rendered(&crate::render(&src, &info, settings, &req), &faces, true);
    look.signature = measure(&crate::render(&src, &info, &DevelopSettings::default(), &req).image).signature;
    look
}

/// Normalized soft similarity weights; every reference contributes, even with dissimilar scenes.
pub fn weights(references: &[Look], signature: &[f64; 10], consistent: bool) -> Vec<f64> {
    let mut w: Vec<f64> = references
        .iter()
        .map(|r| {
            if consistent {
                1.0
            } else {
                let distance: f64 = r.signature.iter().zip(signature).map(|(a, b)| (a - b).powi(2)).sum();
                (-8.0 * distance).exp().max(0.001)
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
    let mut out = Look { features: [0.0; FEATURES], support: [0.0; FEATURES], signature: [0.0; 10], clusters: Vec::new() };
    for i in 0..FEATURES {
        let samples: Vec<_> = references.iter().zip(w).filter(|(r, _)| r.support[i] > 0.0).map(|(r, w)| (r.features[i], w * r.support[i])).collect();
        out.features[i] = weighted_median_floor(&samples, if i < 5 || i == 14 { 0.03 } else { 0.005 });
        out.support[i] = references.iter().zip(w).map(|(r, w)| r.support[i] * w).sum();
    }
    for i in 0..10 {
        out.signature[i] = references.iter().zip(w).map(|(r, w)| r.signature[i] * w).sum();
    }
    // Prototypes remain per-reference in saved looks; choose the strongest reference's
    // prototypes here rather than averaging unrelated hues into a fictitious cluster.
    if let Some((r, _)) = references.iter().zip(w).max_by(|a, b| a.1.total_cmp(b.1)) {
        out.clusters = r.clusters.clone();
    }
    out
}
fn metric_weights(a: &Look, b: &Look, opts: Options) -> [f64; FEATURES] {
    std::array::from_fn(|i| {
        let enabled = match i {
            0..=4 => opts.normalize || opts.light_contrast,
            5..=6 => opts.normalize,
            7 => opts.color_adjust,
            8..=13 => opts.color_tone,
            14..=16 => opts.normalize || opts.color_adjust || opts.color_tone,
            17..=24 => opts.color_adjust,
            27..=29 => opts.color_adjust,
            _ => opts.normalize || opts.color_tone,
        };
        f64::from(enabled)
            * a.support[i].min(b.support[i])
            * match i {
                0 | 4 => 4.0,
                2 => 5.0,
                25 | 26 => 3.0,
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
    (0..FEATURES).map(|i| (a.features[i] - b.features[i]).powi(2) * w[i]).sum()
}

#[derive(Clone)]
enum Parameter {
    Slider(&'static str, f64),
    Grade(&'static str, bool),
    Temperature,
}
impl Parameter {
    fn value(&self, s: &DevelopSettings) -> f64 {
        match self {
            Self::Slider(id, _) => controls::get(s, id).unwrap_or(0.0),
            Self::Temperature => s.wb.temp.max(1.0).log2(),
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
        }
    }
    fn scale(&self) -> f64 {
        match self {
            Self::Temperature => 0.4,
            Self::Grade(..) => 18.0,
            Self::Slider("light.exposure", _) => 1.5,
            Self::Slider("wb.tint", _) => 25.0,
            Self::Slider(..) => 30.0,
        }
    }
    fn prior(&self) -> f64 {
        match self {
            Self::Temperature => 0.00004,
            Self::Slider("light.exposure", _) => 0.000015,
            Self::Slider("light.contrast", _) => 0.0008,
            Self::Slider("color.saturation" | "color.vibrance", _) => 0.0005,
            Self::Grade(..) => 0.0003,
            _ => 0.00015,
        }
    }
    fn bounds(&self, start: &DevelopSettings, wb: (f64, f64), layer: bool) -> (f64, f64) {
        let (lo, hi) = match self {
            Self::Temperature if layer => ((wb.0 * (-0.6f64).exp()).max(2000.0).log2(), (wb.0 * 0.6f64.exp()).min(50000.0).log2()),
            Self::Temperature => (2000f64.log2(), 50000f64.log2()),
            Self::Slider("light.exposure", _) => (-5.0, 5.0),
            Self::Slider("wb.tint", _) if layer => (wb.1 - 40.0, wb.1 + 40.0),
            Self::Slider("wb.tint", _) => (-80.0, 80.0),
            Self::Slider("light.contrast", _) => (-60.0, 60.0),
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
    if opts.normalize {
        if !opts.keep_exposure {
            v.push(Parameter::Slider("light.exposure", 0.15));
        }
        if !opts.keep_white_balance {
            v.push(Parameter::Temperature);
            v.push(Parameter::Slider("wb.tint", 3.0));
        }
    }
    if opts.light_contrast {
        // Endpoint tools precede Contrast; curves share the work rather than exhausting it.
        for id in [
            "light.blacks",
            "light.whites",
            "light.shadows",
            "light.highlights",
            "light.contrast",
            "curve.shadows",
            "curve.darks",
            "curve.lights",
            "curve.highlights",
        ] {
            v.push(Parameter::Slider(id, 6.0));
        }
    }
    if opts.color_adjust {
        for id in ["color.saturation", "color.vibrance"] {
            v.push(Parameter::Slider(id, 6.0));
        }
    }
    if opts.color_tone {
        for band in ["shadows", "midtones", "highlights"] {
            v.push(Parameter::Grade(band, false));
            v.push(Parameter::Grade(band, true));
        }
    }
    v
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

fn layer_settings(base: &DevelopSettings, edits: &DevelopSettings, opts: Options, wb: (f64, f64)) -> DevelopSettings {
    let mut d = base.clone();
    d.masks.push(Mask {
        id: base.masks.iter().map(|m| m.id).max().unwrap_or(0).saturating_add(1),
        name: "Match Look".into(),
        components: vec![MaskComponent { shape: MaskShape::Full, op: Default::default(), invert: false, name: None }],
        adjust: lightcraft_develop::LocalAdjustments {
            temp: if opts.normalize { (edits.wb.temp / wb.0).ln() * 100.0 / 0.6 } else { 0.0 },
            tint: if opts.normalize { (edits.wb.tint - wb.1) / 0.4 } else { 0.0 },
            ..Default::default()
        },
        tools: LayerTools {
            treatment: (opts.color_adjust && edits.treatment != base.treatment).then_some(edits.treatment),
            curve: opts.light_contrast.then_some(edits.curve.clone()),
            light: (opts.normalize || opts.light_contrast).then_some(edits.light),
            color: opts.color_adjust.then_some(edits.color),
            mixer: opts.color_adjust.then_some(edits.mixer),
            grading: opts.color_tone.then_some(edits.grading),
            ..Default::default()
        },
        ..Default::default()
    });
    d
}

/// Slider-space style amount. Normalize stays at full strength, including effective preset WB.
pub fn fade_controls(start: &DevelopSettings, full: &DevelopSettings, opts: Options) -> DevelopSettings {
    let opts = if start.treatment == Treatment::Bw && full.treatment == Treatment::Bw {
        Options { color_adjust: false, color_tone: false, ..opts }
    } else {
        opts
    };
    let mut d = start.clone();
    let amount = opts.strength.clamp(0.0, 125.0) / 100.0;
    if opts.color_adjust {
        d.treatment = full.treatment;
    }
    for p in parameters(opts) {
        let normalization = matches!(&p, Parameter::Temperature | Parameter::Slider("light.exposure" | "wb.tint", _));
        let factor = if normalization { 1.0 } else { amount };
        if matches!(p, Parameter::Temperature) {
            d.wb = full.wb;
        } else {
            p.set(&mut d, p.value(start) + (p.value(full) - p.value(start)) * factor);
        }
    }
    if opts.color_adjust {
        for id in PALETTE_CONTROLS {
            let from = controls::get(start, id).unwrap_or(0.0);
            controls::set(&mut d, id, from + (controls::get(full, id).unwrap_or(0.0) - from) * amount);
        }
    }
    // Preserve a disabled normalization group's original preset mode.
    if !opts.normalize || opts.keep_white_balance {
        d.wb = start.wb;
    }
    guard_curve(start, &mut d);
    d
}
/// Re-amount an applied result without solving or touching unrelated tools. Layer tools are
/// relative; their normalization adjustment remains unscaled.
pub fn amount_settings(before: &DevelopSettings, full: &DevelopSettings, opts: Options) -> DevelopSettings {
    if opts.new_layer {
        let mut d = full.clone();
        if let Some(layer) = d.masks.last_mut() {
            let mut end = DevelopSettings::default();
            if let Some(x) = layer.tools.light {
                end.light = x;
            }
            if let Some(x) = layer.tools.curve.clone() {
                end.curve = x;
            }
            if let Some(x) = layer.tools.color {
                end.color = x;
            }
            if let Some(x) = layer.tools.mixer {
                end.mixer = x;
            }
            if let Some(x) = layer.tools.grading {
                end.grading = x;
            }
            let faded = fade_controls(&DevelopSettings::default(), &end, Options { normalize: opts.normalize, keep_white_balance: true, ..opts });
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
        }
        d
    } else {
        fade_controls(before, full, opts)
    }
}

pub fn reset_group(before: &DevelopSettings, full: &DevelopSettings, opts: Options, group: &str) -> Option<DevelopSettings> {
    let mut d = full.clone();
    if opts.new_layer {
        let layer = d.masks.last_mut()?;
        match group {
            "normalize" => {
                layer.adjust.temp = 0.0;
                layer.adjust.tint = 0.0;
                if let Some(l) = &mut layer.tools.light {
                    l.exposure = 0.0;
                }
            }
            "light" => {
                layer.tools.curve = None;
                if let Some(l) = &mut layer.tools.light {
                    let exposure = l.exposure;
                    *l = Default::default();
                    l.exposure = exposure;
                }
            }
            "color" => {
                layer.tools.color = None;
                layer.tools.mixer = None;
                layer.tools.treatment = None;
            }
            "tone" => layer.tools.grading = None,
            _ => return None,
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
                d.curve = before.curve.clone();
            }
            "color" => {
                d.treatment = before.treatment;
                d.color = before.color;
                d.mixer = before.mixer;
            }
            "tone" => d.grading = before.grading,
            _ => return None,
        }
    }
    Some(d)
}

pub fn is_monochrome(look: &Look) -> bool {
    look.features[7] < 0.012 && (17..25).all(|i| look.support[i] < 0.05)
}

pub struct Solved {
    pub monochrome: bool,
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
}
impl Default for Techniques {
    fn default() -> Self {
        Self { warm_start: true, tone_projection: true, joint_refine: true }
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
    solve_impl(src, info, base, refs, opts, &[], techniques, cancel)
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
    solve_impl(src, info, base, refs, opts, faces, Techniques::default(), cancel)
}
#[allow(clippy::too_many_arguments)]
fn solve_impl(
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    base: &DevelopSettings,
    refs: &[Look],
    opts: Options,
    faces: &[[f32; 4]],
    techniques: Techniques,
    cancel: &dyn Fn() -> bool,
) -> Option<Solved> {
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
    let (fine_src, fine_info) = measurement_source(src, info, 128);
    let (src, info) = measurement_source(src, info, SOURCE_EDGE);
    let mut req = RenderRequest::fit(PROXY_EDGE, PROXY_EDGE);
    req.depth = crate::OutputDepth::F32Linear;
    let mut fine_req = RenderRequest::fit(128, 128);
    fine_req.depth = crate::OutputDepth::F32Linear;
    let cache = StageCache::default();
    let fine_cache = StageCache::default();
    let mut renders = 0;
    let mut evaluate = |d: &DevelopSettings, fine: bool| {
        renders += 1;
        let image = if fine {
            crate::render_cached(&fine_src, &fine_info, d, &fine_req, &fine_cache)
        } else {
            crate::render_cached(&src, &info, d, &req, &cache)
        };
        measure_rendered(&image, &faces, false)
    };
    let initial = evaluate(base, true);
    let signature = measure(&crate::render(&src, &info, &DevelopSettings::default(), &req).image).signature;
    let similarity =
        refs.iter().map(|r| (-2.0 * r.signature.iter().zip(signature).map(|(a, b)| (a - b).powi(2)).sum::<f64>()).exp()).fold(0.0, f64::max);
    let w = weights(refs, &signature, opts.consistent_batch);
    let mut goal = blend(refs, &w);
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
    let monochrome = is_monochrome(&goal) && opts.color_adjust;
    let target_bw = base.treatment == Treatment::Bw;
    let solver_opts = Options {
        color_adjust: opts.color_adjust && !monochrome && !target_bw,
        color_tone: opts.color_tone && !target_bw,
        protect_skin: opts.protect_skin,
        ..opts
    };
    let before_error = distance(&initial, &goal, opts);
    // Pixel-identical/self matches and zero Impact are exactly no-ops, including layer mode.
    if before_error < 1e-10 || (opts.strength <= 0.0 && (!opts.normalize || (opts.keep_white_balance && opts.keep_exposure))) {
        return Some(Solved {
            monochrome,
            settings: base.clone(),
            full_settings: base.clone(),
            before_error,
            after_error: before_error,
            weights: w,
            similarity,
            renders,
        });
    }
    let wb = crate::auto::current_wb(&info, base);
    let start = if opts.new_layer {
        DevelopSettings { wb: lightcraft_develop::WhiteBalance { mode: WbMode::Custom, temp: wb.0, tint: wb.1 }, ..Default::default() }
    } else {
        let mut d = base.clone();
        if opts.normalize && !opts.keep_white_balance && d.wb.mode != WbMode::Custom {
            d.wb.temp = wb.0;
            d.wb.tint = wb.1;
        }
        d
    };
    let mut edits = start.clone();
    if monochrome {
        edits.treatment = Treatment::Bw;
    }
    let layer_opts = Options { color_adjust: opts.color_adjust && !target_bw, color_tone: opts.color_tone && !target_bw, ..opts };
    let materialize = |e: &DevelopSettings| if opts.new_layer { layer_settings(base, e, layer_opts, wb) } else { e.clone() };
    let mut current = if monochrome { evaluate(&materialize(&edits), true) } else { initial.clone() };

    let params = parameters(solver_opts);
    let mut mw = metric_weights(&initial, &goal, solver_opts);
    if !faces.is_empty() && opts.protect_skin {
        for weight in &mut mw[14..=16] {
            *weight *= 3.0;
        }
    }
    let spread: [f64; FEATURES] = std::array::from_fn(|i| {
        let deviations: Vec<_> = refs.iter().zip(&w).map(|(r, w)| ((r.features[i] - goal.features[i]).abs(), *w)).collect();
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
        } else {
            look.features[i]
        }
    };
    let objective = |l: &Look, d: &DevelopSettings, penalty: f64| {
        let fit = (0..FEATURES).map(|i| huber(observation(l, i) - goal.features[i], spread[i] * 2.0) * mw[i]).sum::<f64>();
        let prior = params
            .iter()
            .map(|p| {
                let x = (p.value(d) - p.value(&start)) / p.scale();
                p.cost(opts) * (x * x + 0.08 * ((x * x + 0.01).sqrt() - 0.1))
            })
            .sum::<f64>();
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
    if techniques.warm_start && opts.normalize && !opts.keep_exposure && current.features[2] > 0.02 && goal.features[2] > 0.02 {
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
    for pass in 0..3 {
        let penalty = [1.6, 1.0, 0.6][pass];
        score = objective(&current, &edits, penalty);
        let pass_start_score = score;
        let coarse_current = evaluate(&materialize(&edits), false);
        let mut jac = Vec::new();
        for p in &params {
            if cancel() {
                return None;
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
                jac.push([0.0; FEATURES]);
                continue;
            }
            let measured = evaluate(&materialize(&probe), false);
            jac.push(std::array::from_fn::<_, FEATURES, _>(|i| (observation(&measured, i) - observation(&coarse_current, i)) / h));
        }
        if techniques.warm_start && pass == 0 && opts.normalize && !opts.keep_white_balance {
            let axes: Vec<_> =
                params.iter().enumerate().filter(|(_, p)| matches!(p, Parameter::Temperature | Parameter::Slider("wb.tint", _))).collect();
            if axes.len() == 2 {
                let mut normal = vec![vec![0.0; 2]; 2];
                let mut rhs = vec![0.0; 2];
                for j in 0..2 {
                    for k in 0..2 {
                        normal[j][k] = [5, 6, 25, 26].iter().map(|i| jac[axes[j].0][*i] * jac[axes[k].0][*i] * mw[*i]).sum();
                    }
                    normal[j][j] += axes[j].1.prior();
                    rhs[j] = [5, 6, 25, 26].iter().map(|i| jac[axes[j].0][*i] * (goal.features[*i] - current.features[*i]) * mw[*i]).sum();
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
        // RawTherapee populated-CDF map, slope guarded and projected onto editable tone
        // tools in a fixed order. Reuse this Jacobian: no extra basis renders are needed.
        if techniques.tone_projection && pass == 0 && opts.light_contrast {
            let curve = cdf_curve(&current, &goal);
            let mut residual = std::array::from_fn::<_, 5, _>(|i| curve[i] - current.features[i]);
            let mut projected = edits.clone();
            for (j, p) in params.iter().enumerate() {
                if !matches!(p,Parameter::Slider(id,_) if id.starts_with("light.") || id.starts_with("curve.")) {
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
    if solver_opts.color_adjust {
        let delta: [f64; 8] = std::array::from_fn(|i| {
            if current.support[17 + i].min(goal.support[17 + i]) < 0.05 || current.features[17 + i] < 1e-5 {
                return 0.0;
            }
            // Equalizer saturation nodes are 1 + sat / 200 (colorequal::apply).
            ((goal.features[17 + i] / current.features[17 + i] - 1.0) * 200.0).clamp(-40.0, 40.0)
        });
        if delta.iter().any(|d| d.abs() > 1.0) {
            let candidate = |scale: f64| {
                let mut d = edits.clone();
                for (id, delta) in PALETTE_CONTROLS.iter().zip(delta) {
                    let value = controls::get(&edits, id).unwrap_or(0.0);
                    controls::set(&mut d, id, (value + delta * scale).clamp((-55.0f64).min(value), 55.0f64.max(value)));
                }
                d
            };
            if cancel() {
                return None;
            }
            let probe = candidate(1.0);
            let measured = evaluate(&materialize(&probe), true);
            let mw = metric_weights(&current, &goal, opts);
            let (mut numerator, mut denominator) = (0.0, 0.0);
            for (i, weight) in mw.iter().enumerate() {
                let response = measured.features[i] - current.features[i];
                numerator += weight * response * (goal.features[i] - current.features[i]);
                denominator += weight * response * response;
            }
            let next_edits = candidate(if denominator > 1e-12 { (numerator / denominator).clamp(-1.0, 2.0) } else { 0.0 });
            if cancel() {
                return None;
            }
            let next = evaluate(&materialize(&next_edits), true);
            let palette_penalty = |d: &DevelopSettings| {
                PALETTE_CONTROLS
                    .iter()
                    .map(|id| 0.00015 * ((controls::get(d, id).unwrap_or(0.0) - controls::get(&start, id).unwrap_or(0.0)) / 30.0).powi(2))
                    .sum::<f64>()
            };
            let next_error = objective(&next, &next_edits, 1.0) + palette_penalty(&next_edits);
            let probe_error = objective(&measured, &probe, 1.0) + palette_penalty(&probe);
            if next_error < score && next_error <= probe_error {
                edits = next_edits;
                best_error = distance(&next, &goal, opts);
            } else if probe_error < score {
                edits = probe;
                best_error = distance(&measured, &goal, opts);
            }
        }
    }
    let full_settings = materialize(&edits);
    let mut faded = fade_controls(&start, &edits, opts);
    let strength = opts.strength.clamp(0.0, 125.0) / 100.0;
    // Cartesian grade components must be interpolated together (polar hue wrap is not linear).
    if solver_opts.color_tone {
        for band in ["shadows", "midtones", "highlights"] {
            let x = Parameter::Grade(band, false);
            let y = Parameter::Grade(band, true);
            x.set(&mut faded, x.value(&start) + (x.value(&edits) - x.value(&start)) * strength);
            y.set(&mut faded, y.value(&start) + (y.value(&edits) - y.value(&start)) * strength);
        }
    }
    let settings = if best_error + 1e-10 < before_error { materialize(&faded) } else { base.clone() };
    if cancel() {
        return None;
    }
    let after_error = distance(&evaluate(&settings, true), &goal, opts);
    Some(Solved { monochrome, settings, full_settings, before_error, after_error, weights: w, similarity, renders })
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
                assert_eq!(result.settings.masks.last().unwrap().tools.treatment, Some(Treatment::Bw));
            } else {
                assert_eq!(result.settings.treatment, Treatment::Bw);
                assert_eq!(result.settings.color, base.color);
            }
            let amount = amount_settings(&base, &result.full_settings, opts);
            if new_layer {
                assert_eq!(amount.masks.last().unwrap().tools.treatment, Some(Treatment::Bw));
            } else {
                assert_eq!(amount.treatment, Treatment::Bw);
            }
            let reset = reset_group(&base, &result.full_settings, opts, "color").unwrap();
            if new_layer {
                assert_eq!(reset.masks.last().unwrap().tools.treatment, None);
            } else {
                assert_eq!(reset.treatment, base.treatment);
            }
            assert!(result.after_error < result.before_error * 0.3, "{} {}", result.after_error, result.before_error);
        }
        for new_layer in [false,true] {
            let result=solve(&src,&SourceInfo::default(),&reference,&[analyze(&src,&SourceInfo::default(),&base)],Options {new_layer,..Default::default()},&||false).unwrap();
            assert_eq!(result.settings.treatment,Treatment::Bw);
            assert_eq!(result.settings.color,reference.color);
            assert_eq!(result.settings.grading,reference.grading);
            assert!(result.after_error.is_finite());
            if new_layer && result.settings.masks.len()>reference.masks.len() {
                let tools=&result.settings.masks.last().unwrap().tools;
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
        assert!(result.settings.light.whites < 0.0);
        let full = measure(&crate::render(&src, &info, &result.settings, &RenderRequest::fit(128, 128)).image);
        let (proxy, pi) = measurement_source(&src, &info, 128);
        let small = measure(&crate::render(&proxy, &pi, &result.settings, &RenderRequest::fit(128, 128)).image);
        for i in [0, 2, 4] {
            assert!((small.features[i] - full.features[i]).abs() < 0.03, "feature {i}: {:?}", small.features[i] - full.features[i]);
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
        assert!(solve(&src, &info, &d, &vec![r.clone(); 11], Options::default(), &|| false).is_none());
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
