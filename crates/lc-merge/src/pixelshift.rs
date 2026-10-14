//! Sony Pixel Shift reconstruction in camera RGB, before white balance.
//!
//! Source shot order/offsets follow the documented Sony mapping in
//! https://github.com/agriggio/make_arq/blob/master/make_arq.py (GPL-3.0-or-later).
//! Four consecutive shots provide R/G/G/B; sixteen shots add four half-pixel phases.
//! We use measured CFA samples, not demosaiced frame averaging. Repeated green disagreement
//! detects motion; a dilated mask uses only the first source, demosaiced with the normal noise-adaptive Auto method.
//! Noise-aware disagreement follows RawTherapee `rtengine/pixelshift.cc`'s `greenDiff`
//! (GPL-3.0-or-later, pin 5f486d3678b34c74ba0c63571c17babe20935019), using
//! the existing camera noise profiles and Auto CFA estimator in normalized sensor space.
//! Red/blue samples inconsistent with the first shot's local same-colour range also fall back.
//! This conservative method trades resolution for safety in ambiguous chromatic fine detail.

use lightcraft_geom::Orientation;
use lightcraft_meta::Metadata;
use lightcraft_raster::{Plane, Rgb32f};
use lightcraft_raw::{RawImage, Rect};

use crate::{FrameColor, MergeError, Progress, Result, check};

pub struct PixelShiftResult {
    pub image: Rgb32f,
    pub color: FrameColor,
    pub orientation: Orientation,
    pub metadata: Metadata,
    pub baseline_exposure: f64,
    pub motion: Plane,
    pub shots: usize,
}

// (y, x), indexed by the camera's 1-based shot_number minus one.
const WHOLE: [(usize, usize); 4] = [(0, 1), (1, 1), (1, 0), (0, 0)];
const HALF: [(usize, usize); 4] = [(0, 1), (0, 0), (1, 0), (1, 1)];

fn identity(raw: &RawImage) -> Result<&lightcraft_meta::SonyPixelShift> {
    raw.metadata.sony.as_ref().and_then(|s| s.pixel_shift.as_ref()).ok_or_else(|| {
        MergeError::Mismatch("Sony Pixel Shift capture information is missing; select the original Pixel Shift ARWs or one composed ARQ".into())
    })
}

/// Pixels have already been white-normalized, so carry the corresponding normalized
/// camera matrices/balance and translate Sony's vendor WB multipliers into DNG neutral.
fn merged_camera_color(raw: &RawImage) -> FrameColor {
    let mut color = lightcraft_raw::color::normalized_color(raw);
    if color.as_shot_neutral.is_none()
        && color.as_shot_white_xy.is_none()
        && let Some(wb) = raw.wb_multipliers.filter(|wb| wb.iter().all(|v| v.is_finite() && *v > 0.0))
    {
        let ratios = lightcraft_raw::color::normalization_ratios(raw);
        color.as_shot_neutral = Some(std::array::from_fn(|c| ratios[c] / f64::from(wb[c])));
    }
    FrameColor::Camera(Box::new(color))
}

struct MotionNoise {
    profile: Option<lightcraft_raw::camera_data::NoiseProfile>,
    ratios: [f32; 3],
    quantization_variance: [f32; 3],
    measured_variance: f32,
    amplitude: f32,
}

impl MotionNoise {
    fn new(raw: &RawImage, samples: &lightcraft_raw::NormalizedSamples<'_>) -> Self {
        let profile = lightcraft_raw::camera_data::noise_profile(
            raw.metadata.make.as_deref().unwrap_or_default(),
            raw.metadata.model.as_deref().unwrap_or_default(),
            raw.metadata.iso.unwrap_or(0),
        );
        let ratios = lightcraft_raw::color::normalization_ratios(raw).map(|r| r as f32);
        let mut quantization_variance = [0.0f32; 3];
        if let Some(cfa) = &samples.cfa {
            for y in 0..cfa.height {
                for x in 0..cfa.width {
                    let channel = cfa.color_at(x, y) as usize;
                    let range = (raw.sensor_white(raw.active_area.x + x, raw.active_area.y + y, 0) - raw.black.at(x, y, 0, 1)).max(1.0);
                    let quantum = if matches!(&raw.data, lightcraft_raw::RawData::U16(_)) { 1.0 / range } else { f32::EPSILON };
                    quantization_variance[channel] = quantization_variance[channel].max(quantum * quantum / 12.0);
                }
            }
        }
        // Reuse the normal Auto demosaic's robust same-CFA high-pass estimator.
        // Nine bounded native tiles cover the active area; no mosaic resampling or
        // full sensor copy. As in noise_sigma, use the lower quartile to avoid texture.
        let (w, h) = (samples.width.min(128), samples.height.min(128));
        let mut estimates = Vec::with_capacity(9);
        for gy in 0..3 {
            for gx in 0..3 {
                let (ox, oy) = ((samples.width - w) * gx / 2, (samples.height - h) * gy / 2);
                let tile = lightcraft_raw::Normalized {
                    width: w,
                    height: h,
                    cpp: 1,
                    cfa: samples.cfa.as_ref().map(|c| c.shifted(ox, oy)),
                    data: (0..h).flat_map(|y| (0..w).map(move |x| samples.get(ox + x, oy + y, 0))).collect(),
                };
                let sigma = lightcraft_raw::demosaic::noise_sigma(&tile);
                let nominal = profile.map_or(0.0, |p| {
                    let sum: f32 = tile
                        .data
                        .iter()
                        .enumerate()
                        .map(|(i, &signal)| {
                            let channel = tile.cfa.as_ref().map_or(1, |c| c.color_at(i % w, i / w)) as usize;
                            let r = ratios[channel];
                            (p.a[channel] * r * signal.max(0.0) + p.b[channel] * r * r).max(quantization_variance[channel])
                        })
                        .sum();
                    sum / tile.data.len().max(1) as f32
                });
                if sigma.is_finite() {
                    estimates.push((sigma.max(0.0), nominal));
                }
            }
        }
        estimates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let (sigma, nominal) = estimates.get(estimates.len() / 4).copied().unwrap_or((0.0, 0.0));
        let measured_variance = sigma * sigma;
        // Lab fits can understate the original CFA noise: interpolation, processing
        // and a particular capture depart from the profile conditions. Reuse the
        // existing profiled-denoise amplitude adaptation (bounded at 64), retaining
        // measured channel ratios and never lowering a laboratory variance here.
        let amplitude = if profile.is_some() && nominal > 0.0 { (measured_variance / nominal).clamp(1.0, 64.0) } else { 1.0 };
        Self { profile, ratios, quantization_variance, measured_variance, amplitude }
    }

    fn variance(&self, channel: usize, signal: f32) -> f32 {
        // darktable noiseprofiles/denoiseprofile.c: V(X) = a E[X] + b in sensor
        // camera coordinates before WB. For normalized channel Y=rX, a'=ra,
        // b'=r²b; r includes this source's black-subtracted white ranges.
        let variance = self.profile.map_or(self.measured_variance, |p| {
            let r = self.ratios[channel];
            (p.a[channel] * r * signal.max(0.0) + p.b[channel] * r * r) * self.amplitude
        });
        variance.max(self.quantization_variance[channel])
    }

    fn difference_limit(&self, channel: usize, a: f32, b: f32) -> f32 {
        // RawTherapee pixelshift.cc's greenDiff: compare disagreement to independent
        // shot/read noise, rather than a percent-of-white heuristic. Four sigma is
        // conservative against false motion; variances of two observations add.
        4.0 * (self.variance(channel, a) + self.variance(channel, b)).sqrt()
    }
}

/// Merge one composed ARQ or the complete four/sixteen source ARWs. Capture metadata determines
/// order; ordinary bursts, missing/repeated shots, mixed groups and exposures are rejected.
/// Default crops are retained wherever the active area supplies neighbouring sensor samples.
/// A crop touching the left/top active border loses only that one incomplete colour strip.
pub fn merge_pixel_shift(mut raws: Vec<RawImage>, progress: &Progress) -> Result<PixelShiftResult> {
    check(progress, 0.0, "Checking Pixel Shift capture")?;
    if raws.is_empty() {
        return Err(MergeError::TooFew(0, 1));
    }
    let n = raws.len();
    let first_id = *identity(&raws[0])?;
    if n == 1 {
        let raw = &raws[0];
        if first_id.shot_number != 0 || !matches!(first_id.shot_count, 4 | 16) || raw.cpp != 3 || raw.cfa.is_some() {
            return Err(MergeError::Mismatch("select all 4 or 16 original Pixel Shift shots, or one composed ARQ".into()));
        }
        let image = raw.develop(lightcraft_raw::Method::Ahd).map_err(|e| MergeError::Decode(e.to_string()))?;
        check(progress, 1.0, "Pixel Shift ready")?;
        return Ok(PixelShiftResult {
            motion: Plane::new(image.width, image.height),
            image,
            color: merged_camera_color(raw),
            orientation: raw.orientation,
            metadata: raw.metadata.clone(),
            baseline_exposure: raw.color.baseline_exposure,
            shots: first_id.shot_count as usize,
        });
    }
    if !matches!(n, 4 | 16) || first_id.shot_count as usize != n {
        return Err(MergeError::Mismatch("select one complete 4-shot or 16-shot Pixel Shift capture".into()));
    }
    let mut seen = vec![false; n];
    for r in &raws {
        let id = identity(r)?;
        if id.group_id != first_id.group_id || id.shot_count != first_id.shot_count || id.shot_number == 0 || id.shot_number as usize > n {
            return Err(MergeError::Mismatch("the selected photos belong to different or composed Pixel Shift captures".into()));
        }
        let slot = &mut seen[id.shot_number as usize - 1];
        if *slot {
            return Err(MergeError::Mismatch("a Pixel Shift shot is repeated; select each source once".into()));
        }
        *slot = true;
    }
    raws.sort_by_key(|r| r.metadata.sony.as_ref().and_then(|s| s.pixel_shift.as_ref()).map_or(0, |p| p.shot_number));
    let first = &raws[0];
    for r in &raws {
        if r.cpp != 1
            || !r.cfa.as_ref().is_some_and(|c| c.is_bayer())
            || r.width != first.width
            || r.height != first.height
            || r.active_area != first.active_area
            || r.crop != first.crop
            || r.cfa != first.cfa
            || r.orientation != first.orientation
            || r.metadata.make != first.metadata.make
            || r.metadata.model != first.metadata.model
            || r.metadata.exposure_time != first.metadata.exposure_time
            || r.metadata.iso != first.metadata.iso
            || r.metadata.f_number != first.metadata.f_number
            || r.color != first.color
        {
            return Err(MergeError::Mismatch(
                "Pixel Shift sources must have matching Bayer layout, camera, crop, orientation, exposure and colour calibration".into(),
            ));
        }
    }
    let crop = first.crop.clipped(first.active_area.width, first.active_area.height);
    let crop = if crop.width == 0 || crop.height == 0 { Rect::new(0, 0, first.active_area.width, first.active_area.height) } else { crop };
    if crop.width < 4 || crop.height < 4 {
        return Err(MergeError::Mismatch("Pixel Shift source is too small".into()));
    }
    let factor = if n == 16 { 2 } else { 1 };
    let (origin_x, origin_y) = (crop.x.max(1), crop.y.max(1));
    let w = (crop.width - (origin_x - crop.x)).checked_mul(factor).ok_or_else(|| MergeError::Mismatch("Pixel Shift dimensions overflow".into()))?;
    let h = (crop.height - (origin_y - crop.y)).checked_mul(factor).ok_or_else(|| MergeError::Mismatch("Pixel Shift dimensions overflow".into()))?;
    if !w.checked_mul(h).and_then(|p| p.checked_mul(3)).is_some_and(|p| p <= lightcraft_raw::MAX_SAMPLES && p <= isize::MAX as usize / 4) {
        return Err(MergeError::Mismatch("Pixel Shift output exceeds the supported DNG dimensions".into()));
    }
    // Validate the sample storage before allocating any full-resolution output buffers.
    let samples = raws.iter().map(|r| r.normalized_samples().map_err(|e| MergeError::Decode(e.to_string()))).collect::<Result<Vec<_>>>()?;
    let noise = MotionNoise::new(first, &samples[0]);
    let mut image = Rgb32f::new(w, h);
    let mut motion = Plane::new(w, h);
    for y in 0..h {
        check(progress, 0.1 + 0.65 * y as f32 / h as f32, "Reconstructing measured colour samples")?;
        for x in 0..w {
            let (sx, sy) = (origin_x + x / factor, origin_y + y / factor);
            let group = if factor == 1 { 0 } else { HALF.iter().position(|&(dy, dx)| dy == y % 2 && dx == x % 2).unwrap_or(0) };
            let mut values = [0.0; 3];
            let mut greens = [0.0; 2];
            let mut ng = 0;
            for (i, &(dy, dx)) in WHOLE.iter().enumerate() {
                let s = &samples[group * 4 + i];
                let (px, py) = (sx - dx, sy - dy);
                let channel = s.cfa.as_ref().map_or(0, |c| c.color_at(px, py)) as usize;
                let v = s.get(px, py, 0);
                if channel == 1 {
                    greens[ng] = v;
                    ng += 1;
                    values[1] += v * 0.5;
                } else {
                    values[channel] = v;
                }
            }
            image.data[y * w + x] = values;
            // Read/shot noise tolerance. Disagreement is evaluated before WB: a neutral
            // threshold must not be amplified by the red/blue white-balance multipliers.
            let tolerance = noise.difference_limit(1, greens[0], greens[1]);
            let reference = &samples[0];
            // A second independent test catches motion with little green contrast.
            // Use a neighbourhood interval, rather than demosaic residuals: a static
            // high-contrast edge is allowed its local range without false colour ghosts.
            let (rx, ry) = (sx - 1, sy);
            let chromatic_motion = [0, 2].iter().any(|&channel| {
                let mut lo = f32::INFINITY;
                let mut hi = f32::NEG_INFINITY;
                // A Bayer colour lies on a two-pixel lattice. A two-pixel
                // radius brackets the first-shot half phase as well as whole shifts;
                // a one-pixel radius can leave just one red/blue observation.
                for yy in ry.saturating_sub(2)..=(ry + 2).min(reference.height - 1) {
                    for xx in rx.saturating_sub(2)..=(rx + 2).min(reference.width - 1) {
                        if reference.cfa.as_ref().is_some_and(|c| c.color_at(xx, yy) as usize == channel) {
                            let v = reference.get(xx, yy, 0);
                            lo = lo.min(v);
                            hi = hi.max(v);
                        }
                    }
                }
                let tolerance = noise.difference_limit(channel, values[channel], (lo + hi) * 0.5);
                !values[channel].is_finite() || values[channel] < lo - tolerance || values[channel] > hi + tolerance
            });
            if chromatic_motion || greens.iter().any(|v| !v.is_finite()) || (greens[0] - greens[1]).abs() > tolerance {
                motion.data[y * w + x] = 1.0;
            }
        }
    }
    drop(samples);
    if motion.data.iter().any(|v| *v > 0.0) {
        check(progress, 0.8, "Developing motion fallback")?;
        // Do not apply geometry before merge; the normal Develop lens pipeline owns it.
        let mut source = raws.remove(0);
        drop(raws);
        source
            .opcodes
            .list3
            .retain(|o| !matches!(o, lightcraft_raw::Opcode::WarpRectilinear { .. } | lightcraft_raw::Opcode::FixVignetteRadial { .. }));
        source.crop = Rect::new(0, 0, source.active_area.width, source.active_area.height);
        let fallback = source.develop(lightcraft_raw::Method::Auto).map_err(|e| MergeError::Decode(e.to_string()))?;
        let mut expanded = Plane::new(w, h);
        let radius = 2 * factor;
        // Separable sliding-window dilation costs O(pixels), independent of camera
        // resolution and mask radius. Retain the original first pass until it completes.
        for y in 0..h {
            let mut count = (0..=radius.min(w - 1)).filter(|&x| motion.data[y * w + x] > 0.0).count();
            for x in 0..w {
                expanded.data[y * w + x] = if count > 0 { 1.0 } else { 0.0 };
                if x >= radius && motion.data[y * w + x - radius] > 0.0 {
                    count -= 1;
                }
                if x + radius + 1 < w && motion.data[y * w + x + radius + 1] > 0.0 {
                    count += 1;
                }
            }
        }
        for x in 0..w {
            let mut count = (0..=radius.min(h - 1)).filter(|&y| expanded.data[y * w + x] > 0.0).count();
            for y in 0..h {
                motion.data[y * w + x] = if count > 0 { 1.0 } else { 0.0 };
                if y >= radius && expanded.data[(y - radius) * w + x] > 0.0 {
                    count -= 1;
                }
                if y + radius + 1 < h && expanded.data[(y + radius + 1) * w + x] > 0.0 {
                    count += 1;
                }
            }
        }
        drop(expanded);
        for y in 0..h {
            check(progress, 0.8 + 0.2 * y as f32 / h as f32, "Replacing moving areas")?;
            for x in 0..w {
                if motion.data[y * w + x] > 0.0 {
                    // First shot has (+1,0) displacement, plus the first half phase
                    // (+0.5,0) in a sixteen-shot capture. Sample the uncropped fallback
                    // so default crop margins remain available at the output edges.
                    let px = origin_x as f32 - 1.0 + x as f32 / factor as f32 - if n == 16 { 0.5 } else { 0.0 };
                    let py = origin_y as f32 + y as f32 / factor as f32;
                    image.data[y * w + x] = fallback.sample_bilinear(px + 0.5, py + 0.5);
                }
            }
        }
        return Ok(PixelShiftResult {
            image,
            color: merged_camera_color(&source),
            orientation: source.orientation,
            metadata: source.metadata,
            baseline_exposure: source.color.baseline_exposure,
            motion,
            shots: n,
        });
    }
    check(progress, 1.0, "Pixel Shift ready")?;
    Ok(PixelShiftResult {
        image,
        color: merged_camera_color(first),
        orientation: first.orientation,
        metadata: first.metadata.clone(),
        baseline_exposure: first.color.baseline_exposure,
        motion,
        shots: n,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use lightcraft_meta::{SonyMetadata, SonyPixelShift};
    use lightcraft_raw::{BlackLevel, Cfa, ColorData, OpcodeLists, RawData, RawFormat};

    // Independent scene sampled on the known physical sensor lattice. Each colour has
    // a different slope, so an incorrect offset, phase or colour assignment fails.
    fn scene(x: f32, y: f32) -> [f32; 3] {
        [0.2 + 0.002 * x + 0.001 * y, 0.3 + 0.001 * x + 0.003 * y, 0.4 + 0.003 * x + 0.002 * y]
    }

    fn sources(count: usize) -> Vec<RawImage> {
        (0..count)
            .map(|i| {
                let cfa = Cfa::bayer("RGGB").unwrap();
                // Independently transcribed absolute sensor displacements in capture
                // order from pinned make_arq.py's get_frames/get_sony_frame_data. Do not
                // derive these fixtures from the reconstruction's WHOLE/HALF constants.
                const FOUR_ABSOLUTE: [(f32, f32); 4] = [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)];
                const SIXTEEN_ABSOLUTE: [(f32, f32); 16] = [
                    (0.0, 1.5),
                    (1.0, 1.5),
                    (1.0, 0.5),
                    (0.0, 0.5),
                    (0.0, 1.0),
                    (1.0, 1.0),
                    (1.0, 0.0),
                    (0.0, 0.0),
                    (0.5, 1.0),
                    (1.5, 1.0),
                    (1.5, 0.0),
                    (0.5, 0.0),
                    (0.5, 1.5),
                    (1.5, 1.5),
                    (1.5, 0.5),
                    (0.5, 0.5),
                ];
                let (dy, dx) = if count == 16 { SIXTEEN_ABSOLUTE[i] } else { FOUR_ABSOLUTE[i] };
                let data = (0..24 * 20)
                    .map(|p| {
                        let (x, y) = (p % 24, p / 24);
                        scene(x as f32 + dx, y as f32 + dy)[cfa.color_at(x, y) as usize] * 1000.0 + 64.0
                    })
                    .collect();
                RawImage {
                    vendor_lens: None,
                    format: RawFormat::Arw,
                    width: 24,
                    height: 20,
                    cpp: 1,
                    data: RawData::F32(data),
                    cfa: Some(cfa),
                    bits: 14,
                    black: BlackLevel::uniform(64.0),
                    white: vec![1064.0],
                    active_area: Rect::new(0, 0, 24, 20),
                    crop: Rect::new(2, 2, 20, 16),
                    orientation: Orientation::Normal,
                    color: ColorData {
                        illuminant: [21, 0],
                        color_matrix: [Some(lightcraft_color::Mat3::IDENTITY), None],
                        forward_matrix: [Some(lightcraft_color::Mat3::IDENTITY), None],
                        profile: lightcraft_raw::profile::ProfileLook {
                            tone_curve: Some(lightcraft_raw::profile::ToneCurve { points: vec![[0.0, 0.0], [0.5, 0.45], [1.0, 1.0]] }),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    wb_multipliers: Some([2.0, 1.0, 1.5]),
                    linearized: false,
                    opcodes: OpcodeLists::default(),
                    metadata: Metadata {
                        make: Some("SONY".into()),
                        model: Some("Synthetic Pixel Shift".into()),
                        sony: Some(SonyMetadata {
                            pixel_shift: Some(SonyPixelShift { group_id: 123, shot_number: (i + 1) as u8, shot_count: count as u8 }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                }
            })
            .collect()
    }

    #[test]
    fn pixelshift_four_and_sixteen_reconstruct_measured_rgb_and_develop_linear_dng() {
        for count in [4, 16] {
            let mut raws = sources(count);
            raws.reverse(); // selection order must not affect sensor order
            let result = merge_pixel_shift(raws, &crate::no_progress).unwrap();
            let factor = if count == 16 { 2 } else { 1 };
            assert_eq!((result.image.width, result.image.height), (20 * factor, 16 * factor));
            assert!(result.motion.data.iter().all(|v| *v == 0.0));
            let mut max_error = 0.0f32;
            for y in 0..result.image.height {
                for x in 0..result.image.width {
                    let expected = scene(2.0 + x as f32 / factor as f32, 2.0 + y as f32 / factor as f32);
                    for (a, b) in result.image.get(x, y).iter().zip(expected) {
                        max_error = max_error.max((a - b).abs());
                    }
                }
            }
            println!("Pixel Shift {count}-shot synthetic maximum absolute camera RGB error {max_error:.9}");
            assert!(max_error < 1e-6, "{count} shots: maximum camera RGB error {max_error}");
            let dng = crate::write_linear_dng(
                &result.image,
                &result.color,
                result.orientation,
                &result.metadata,
                result.baseline_exposure,
                crate::DngSamples::Float,
            )
            .unwrap();
            let decoded = lightcraft_raw::decode(&dng).unwrap();
            assert_eq!(decoded.cpp, 3);
            assert!(decoded.cfa.is_none());
            let neutral = decoded.color.as_shot_neutral.unwrap();
            for (actual, expected) in neutral.into_iter().zip([0.5, 1.0, 2.0 / 3.0]) {
                assert!((actual - expected).abs() < 1e-6, "source WB is retained in merged DNG: {neutral:?}");
            }
            assert_eq!(decoded.color.color_matrix[0], Some(lightcraft_color::Mat3::IDENTITY));
            assert_eq!(decoded.color.forward_matrix[0], Some(lightcraft_color::Mat3::IDENTITY));
            assert_eq!(decoded.color.profile.tone_curve.as_ref().unwrap().points, vec![[0.0, 0.0], [0.5, 0.45], [1.0, 1.0]]);
            let developed = decoded.develop(lightcraft_raw::Method::Ahd).unwrap();
            assert_eq!(developed, result.image);
        }
    }

    #[test]
    fn pixelshift_motion_uses_one_source_instead_of_false_colours() {
        let mut raws = sources(4);
        if let RawData::F32(data) = &mut raws[1].data {
            for y in 7..12 {
                for x in 7..12 {
                    data[y * 24 + x] += 400.0;
                }
            }
        }
        let mut fallback_raw = raws[0].clone();
        fallback_raw.crop = Rect::new(0, 0, 24, 20);
        let fallback = fallback_raw.develop(lightcraft_raw::Method::Auto).unwrap();
        let result = merge_pixel_shift(raws, &crate::no_progress).unwrap();
        assert!(result.motion.data.iter().filter(|v| **v > 0.0).count() > 25);
        assert_eq!(result.motion.get(7, 7), 1.0);
        assert_eq!(result.image.get(7, 7), fallback.get(8, 9));
        assert_eq!(result.motion.get(17, 13), 0.0);
    }

    #[test]
    fn pixelshift_chromatic_motion_and_sixteen_phase_fallback() {
        for count in [4, 16] {
            let mut raws = sources(count);
            let frame = if count == 16 { 6 } else { 2 };
            if let RawData::F32(data) = &mut raws[frame].data {
                // Blue-only change: green-repeat testing alone cannot see this motion.
                for y in 7..12 {
                    for x in 7..12 {
                        if x % 2 == 1 && y % 2 == 1 {
                            data[y * 24 + x] += 500.0;
                        }
                    }
                }
            }
            let mut reference_raw = raws[0].clone();
            reference_raw.crop = Rect::new(0, 0, 24, 20);
            let reference = reference_raw.develop(lightcraft_raw::Method::Auto).unwrap();
            let result = merge_pixel_shift(raws, &crate::no_progress).unwrap();
            let factor = if count == 16 { 2 } else { 1 };
            let (x, y) = (6 * factor, 6 * factor);
            assert_eq!(result.motion.get(x, y), 1.0);
            let expected = reference.sample_bilinear(x as f32 / factor as f32 + if count == 16 { 1.0 } else { 1.5 }, 2.5 + y as f32 / factor as f32);
            assert_eq!(result.image.get(x, y), expected);
        }
    }

    #[test]
    fn pixelshift_measured_noise_detects_subtle_green_motion() {
        let mut raws = sources(4);
        // Independent noiseless integer sensor measurements. A five-DN temporal
        // change is above quantization noise but below the former percentage limit.
        for raw in &mut raws {
            let data = std::mem::replace(&mut raw.data, RawData::U16(Vec::new()));
            if let RawData::F32(values) = data {
                raw.data = RawData::U16(values.into_iter().map(|v| v.round() as u16).collect());
            }
        }
        if let RawData::U16(data) = &mut raws[1].data {
            for y in 7..12 {
                for x in 7..12 {
                    if (x + y) % 2 == 1 {
                        data[y * 24 + x] += 5;
                    }
                }
            }
        }
        let mut reference_raw = raws[0].clone();
        reference_raw.crop = Rect::new(0, 0, 24, 20);
        let reference = reference_raw.develop(lightcraft_raw::Method::Auto).unwrap();
        let result = merge_pixel_shift(raws, &crate::no_progress).unwrap();
        assert_eq!(result.motion.get(6, 7), 1.0);
        assert_eq!(result.image.get(6, 7), reference.get(7, 9));
        assert_eq!(result.motion.get(18, 14), 0.0);
    }

    #[test]
    fn pixelshift_float_dng_preserves_negative_and_fractional_sensor_samples() {
        let image = Rgb32f::from_fn(12, 10, |x, y| [-0.001953125 - x as f32 * 0.00000003, 0.1234567 + y as f32 * 0.000001, 0.6]);
        let color = FrameColor::Camera(Box::default());
        let bytes = crate::write_linear_dng(&image, &color, Orientation::Normal, &Metadata::default(), 0.0, crate::DngSamples::Float).unwrap();
        let decoded = lightcraft_raw::decode(&bytes).unwrap();
        let developed = decoded.develop(lightcraft_raw::Method::Auto).unwrap();
        assert_eq!(developed, image, "FP32 linear DNG preserves negative sensor noise and fractional samples bit for bit");
    }

    #[test]
    fn pixelshift_rejects_bursts_duplicates_mixed_captures_and_exposures_and_cancels() {
        let mut raws = sources(4);
        raws[0].metadata.sony = None;
        assert!(merge_pixel_shift(raws, &crate::no_progress).is_err());
        let mut raws = sources(4);
        raws[1] = raws[0].clone();
        assert!(merge_pixel_shift(raws, &crate::no_progress).is_err());
        let mut raws = sources(4);
        raws[1].metadata.sony.as_mut().unwrap().pixel_shift.as_mut().unwrap().group_id += 1;
        assert!(merge_pixel_shift(raws, &crate::no_progress).is_err());
        let mut raws = sources(4);
        raws[1].metadata.iso = Some(200);
        assert!(merge_pixel_shift(raws, &crate::no_progress).is_err());
        assert!(matches!(merge_pixel_shift(sources(4), &|_, _| false), Err(MergeError::Cancelled)));
    }

    #[test]
    #[ignore = "small real CC0 Sony capture measurement; requires LOCAL_IMAGE_PIXELSHIFT_SAMPLES"]
    fn pixelshift_real_cc0_four_shots_match_sony_composed_arq() {
        let dir = std::path::PathBuf::from(std::env::var_os("LOCAL_IMAGE_PIXELSHIFT_SAMPLES").unwrap());
        let mut raws = (3251..=3254)
            .map(|id| lightcraft_raw::decode_for_pixel_shift(&std::fs::read(dir.join(format!("{id}.ARW"))).unwrap(), 1 << 30).unwrap())
            .collect::<Vec<_>>();
        println!("Sony source active/crop={:?}/{:?} black={:?} white={:?}", raws[0].active_area, raws[0].crop, raws[0].black, raws[0].white);
        let a = raws[0].active_area;
        let original_crop = raws[0].crop;
        let crop = Rect::new(original_crop.x + 512, original_crop.y + 512, 128, 128);
        for raw in &mut raws {
            raw.crop = crop;
        }
        let result = merge_pixel_shift(raws, &crate::no_progress).unwrap();
        let arq = lightcraft_raw::decode_for_pixel_shift(&std::fs::read(dir.join("3255.ARQ")).unwrap(), 2usize << 30).unwrap();
        println!("Sony ARQ active/crop={:?}/{:?} black={:?} white={:?}", arq.active_area, arq.crop, arq.black, arq.white);
        let reference = arq.normalized_samples().unwrap();
        let (ox, oy) = (a.x + crop.x.max(1) - arq.active_area.x, a.y + crop.y.max(1) - arq.active_area.y);
        let (mut total, mut maximum, mut count) = (0.0f64, 0.0f32, 0usize);
        let mut changed_channels = [0usize; 3];
        let mut worst = (0, 0, 0);
        for y in 0..result.image.height {
            for x in 0..result.image.width {
                if result.motion.get(x, y) > 0.0 {
                    continue;
                }
                for channel in 0..3 {
                    let error = (result.image.get(x, y)[channel] - reference.get(ox + x, oy + y, channel)).abs();
                    total += error as f64;
                    if error > maximum {
                        maximum = error;
                        worst = (x, y, channel);
                    }
                    if error > 2e-7 {
                        changed_channels[channel] += 1;
                    }
                    count += 1;
                }
            }
        }
        assert!(count > result.image.data.len(), "at least one third of the crop must retain measured colour");
        let mean = total / count as f64;
        let motion_pixels = result.motion.data.iter().filter(|v| **v > 0.0).count();
        let motion_fraction = motion_pixels as f64 / result.image.data.len() as f64;
        let metric = format!(
            "CC0 Sony A7RIII IDs3251–3255; 128x128 native crop; {count} static channel samples; mean absolute normalized RGB error={mean:.9}; maximum={maximum:.9}; motion fallback pixels={motion_pixels}; motion fraction={motion_fraction:.6}\n"
        );
        println!("{metric}Differences beyond f32 tolerance by RGB channel={changed_channels:?}; worst crop pixel={worst:?}");
        std::fs::write(dir.join("merge-quality.txt"), metric).unwrap();
        assert!(mean < 1e-7 && maximum < 2e-7, "Sony ARQ comparison mean error {mean}, maximum {maximum}");
    }
}
