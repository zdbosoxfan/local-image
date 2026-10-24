//! Sony Pixel Shift reconstruction in camera RGB, before white balance.
//!
//! Source shot order/offsets follow the documented Sony mapping in
//! https://github.com/agriggio/make_arq/blob/master/make_arq.py (GPL-3.0-or-later).
//! Four consecutive shots provide R/G/G/B; sixteen shots add four half-pixel phases.
//! We use measured CFA samples, not demosaiced frame averaging. Repeated green disagreement
//! detects motion; a feathered mask blends in the first source, demosaiced with the normal
//! noise-adaptive Auto method. Shot brightness is equalized first (flicker, changing light).
//! Noise-aware disagreement follows RawTherapee `rtengine/pixelshift.cc`'s `greenDiff`
//! (GPL-3.0-or-later, pin 5f486d3678b34c74ba0c63571c17babe20935019), using
//! the existing camera noise profiles and Auto CFA estimator in normalized sensor space.
//! Red/blue samples inconsistent with the first shot's local same-colour range also fall back.
//! This conservative method trades resolution for safety in ambiguous chromatic fine detail.

use li_raster::{GrayFrame, RgbFrame};
use lightcraft_geom::Orientation;
use lightcraft_meta::Metadata;
use lightcraft_raw::{RawImage, Rect};
use rayon::prelude::*;

use crate::frame::camera_color;
use crate::{FrameColor, MergeError, Progress, Result, check};

pub struct PixelShiftResult {
    pub image: RgbFrame,
    pub color: FrameColor,
    pub orientation: Orientation,
    pub metadata: Metadata,
    pub baseline_exposure: f64,
    pub motion: GrayFrame,
    pub shots: usize,
    /// Value at or above which the sensor was saturated ([`crate::Frame::clip`]).
    pub clip: f32,
}

/// Output rows per parallel batch between cancellation checks.
const ROWS: usize = 64;

/// Photo-response non-uniformity between photosites, relative (RawTherapee `pixelshift.cc`
/// default `pixelShiftPrnu`, 1 %).
const PRNU: f32 = 0.01;

/// Shot gains closer to 1 than this are left alone: Sony's own composition does not equalize, and
/// the residual grid is invisible.
const EQUALIZE_MIN: f32 = 0.002;

// (y, x), indexed by the camera's 1-based shot_number minus one.
const WHOLE: [(usize, usize); 4] = [(0, 1), (1, 1), (1, 0), (0, 0)];
const HALF: [(usize, usize); 4] = [(0, 1), (0, 0), (1, 0), (1, 1)];

fn identity(raw: &RawImage) -> Result<&lightcraft_meta::SonyPixelShift> {
    raw.metadata.sony.as_ref().and_then(|s| s.pixel_shift.as_ref()).ok_or_else(|| {
        MergeError::Mismatch("Sony Pixel Shift capture information is missing; select the original Pixel Shift ARWs or one composed ARQ".into())
    })
}

/// The normalized value at which `raw`'s sensor saturates: the lowest colour plateau of
/// [`RawImage::saturation_points`] (Pixel Shift decoding skips the white-level estimate that
/// otherwise moves the white level onto it), else the white level.
fn saturation(raw: &RawImage) -> f32 {
    let points = raw.saturation_points();
    let Some(cfa) = &raw.cfa else { return 1.0 };
    let a = raw.active_area;
    let mut level = 1.0f32;
    for y in 0..cfa.height {
        for x in 0..cfa.width {
            if let Some(point) = points[cfa.color_at(a.x + x, a.y + y).min(2) as usize] {
                let black = raw.black.at(x, y, 0, 1);
                let range = raw.sensor_white(a.x + x, a.y + y, 0) - black;
                if range > 0.0 {
                    level = level.min((f32::from(point) - black) / range);
                }
            }
        }
    }
    level
}

/// Per shot and CFA colour, the gain that matches the shot's brightness to the first shot's
/// (RawTherapee `pixelshift.cc` equalBrightChannel). LED flicker, or window light changing during a
/// multi-second capture, otherwise leaves a fixed 2×2 grid over the frame, or trips motion
/// detection everywhere. The ratio of the medians of well-exposed samples: the shots see the same
/// scene shifted by at most a pixel, which moves the median of a whole frame negligibly.
fn shot_gains(samples: &[lightcraft_raw::NormalizedSamples<'_>], clip: f32) -> Vec<[f32; 3]> {
    let medians: Vec<[Option<f32>; 3]> = samples.par_iter().map(|s| colour_medians(s, clip)).collect();
    medians
        .iter()
        .map(|shot| {
            std::array::from_fn(|c| match (medians[0][c], shot[c]) {
                (Some(first), Some(this)) if this > 0.0 && (first / this - 1.0).abs() > EQUALIZE_MIN => first / this,
                _ => 1.0,
            })
        })
        .collect()
}

/// Median of each CFA colour's well-exposed samples (above deep shadow, below near-clipping) on a
/// bounded sparse grid of whole CFA tiles; `None` without enough samples for a stable median.
fn colour_medians(samples: &lightcraft_raw::NormalizedSamples<'_>, clip: f32) -> [Option<f32>; 3] {
    const MIN_SAMPLES: usize = 4096;
    let Some(cfa) = &samples.cfa else { return [None; 3] };
    let (tiles_x, tiles_y) = (samples.width / 2, samples.height / 2);
    let step = ((tiles_x * tiles_y) as f64 / 250_000.0).sqrt().ceil().max(1.0) as usize;
    let mut values: [Vec<f32>; 3] = Default::default();
    for ty in (0..tiles_y).step_by(step) {
        for tx in (0..tiles_x).step_by(step) {
            for (x, y) in [(2 * tx, 2 * ty), (2 * tx + 1, 2 * ty), (2 * tx, 2 * ty + 1), (2 * tx + 1, 2 * ty + 1)] {
                let v = samples.get(x, y, 0);
                if v > 0.02 && v < 0.9 * clip {
                    values[cfa.color_at(x, y).min(2) as usize].push(v);
                }
            }
        }
    }
    values.map(|mut v| {
        let middle = v.len() / 2;
        (v.len() >= MIN_SAMPLES).then(|| *v.select_nth_unstable_by(middle, f32::total_cmp).1)
    })
}

/// Fallback weights from per-pixel motion detections, after RawTherapee's defaults (blurred motion
/// map, isolated detections dropped, smooth transitions): a detection with fewer than three within
/// the colour lattice's reach is noise; the rest grow by the sampling footprint and fade over a
/// Gaussian edge, instead of switching abruptly between sharp measured colour and the softer
/// single-shot fallback.
fn motion_weights(hits: GrayFrame, factor: usize) -> GrayFrame {
    let (w, h) = (hits.width, hits.height);
    let reach = 2 * factor;
    let mut rows = GrayFrame::new(w, h);
    rows.data.par_chunks_mut(w).enumerate().for_each(|(y, out)| {
        let line = &hits.data[y * w..(y + 1) * w];
        for (x, count) in out.iter_mut().enumerate() {
            *count = line[x.saturating_sub(reach)..=(x + reach).min(w - 1)].iter().filter(|v| **v > 0.0).count() as f32;
        }
    });
    let mut kept = GrayFrame::new(w, h);
    kept.data.par_chunks_mut(w).enumerate().for_each(|(y, out)| {
        for (x, keep) in out.iter_mut().enumerate() {
            if hits.data[y * w + x] > 0.0 {
                let count: f32 = (y.saturating_sub(reach)..=(y + reach).min(h - 1)).map(|yy| rows.data[yy * w + x]).sum();
                *keep = if count >= 3.0 { 1.0 } else { 0.0 };
            }
        }
    });
    drop((rows, hits));
    let grown = kept.max_filter(factor);
    drop(kept);
    let mut weights = grown.gaussian(factor as f32);
    weights.map_in_place(|v| (2.0 * v).min(1.0));
    weights
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
        // conservative against false motion; variances of two observations add. The two
        // observations come from different photosites, whose gains differ (PRNU, and the
        // G1/G2 split for greens), so greenDiff's photo-response term is added too.
        let prnu = PRNU * 0.5 * (a + b).max(0.0);
        4.0 * (self.variance(channel, a) + self.variance(channel, b) + prnu * prnu).sqrt()
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
            motion: GrayFrame::new(image.width, image.height),
            image,
            color: camera_color(raw),
            orientation: raw.orientation,
            metadata: raw.metadata.clone(),
            baseline_exposure: raw.color.baseline_exposure,
            shots: first_id.shot_count as usize,
            clip: 1.0,
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
            || r.metadata.make != first.metadata.make
            || r.metadata.model != first.metadata.model
            || r.metadata.exposure_time != first.metadata.exposure_time
            || r.metadata.iso != first.metadata.iso
            || r.metadata.f_number != first.metadata.f_number
            || r.color != first.color
        {
            return Err(MergeError::Mismatch(
                "Pixel Shift sources must have matching Bayer layout, camera, crop, exposure and colour calibration".into(),
            ));
        }
    }
    // The shift pattern is in sensor coordinates; a tag that changed mid-capture is ignored.
    let orientation = crate::frame::common_orientation(&raws, 0);
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
    let clip = raws.par_iter().map(saturation).reduce(|| 1.0, f32::min);
    let noise = MotionNoise::new(first, &samples[0]);
    let gains = shot_gains(&samples, clip);
    let reconstruct = |x: usize, y: usize| -> ([f32; 3], bool) {
        let (sx, sy) = (origin_x + x / factor, origin_y + y / factor);
        let group = if factor == 1 { 0 } else { HALF.iter().position(|&(dy, dx)| dy == y % 2 && dx == x % 2).unwrap_or(0) };
        let mut values = [0.0; 3];
        let mut greens = [0.0; 2];
        let mut ng = 0;
        for (i, &(dy, dx)) in WHOLE.iter().enumerate() {
            let shot = group * 4 + i;
            let s = &samples[shot];
            let (px, py) = (sx - dx, sy - dy);
            let channel = s.cfa.as_ref().map_or(0, |c| c.color_at(px, py)) as usize;
            let v = s.get(px, py, 0);
            // Saturated samples stay saturated.
            let v = if v < clip { v * gains[shot][channel] } else { v };
            if channel == 1 {
                greens[ng] = v;
                ng += 1;
                values[1] += v * 0.5;
            } else {
                values[channel] = v;
            }
        }
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
        (values, chromatic_motion || greens.iter().any(|v| !v.is_finite()) || (greens[0] - greens[1]).abs() > tolerance)
    };
    let mut image = RgbFrame::new(w, h);
    let mut motion = GrayFrame::new(w, h);
    // Row groups bound cancellation latency without invoking the callback from workers.
    for (tile, (pixels, moved)) in image.data.chunks_mut(w * ROWS).zip(motion.data.chunks_mut(w * ROWS)).enumerate() {
        check(progress, 0.1 + 0.65 * (tile * ROWS) as f32 / h as f32, "Reconstructing measured colour samples")?;
        pixels.par_chunks_mut(w).zip(moved.par_chunks_mut(w)).enumerate().for_each(|(row, (pixels, moved))| {
            let y = tile * ROWS + row;
            for x in 0..w {
                let (values, moving) = reconstruct(x, y);
                pixels[x] = values;
                if moving {
                    moved[x] = 1.0;
                }
            }
        });
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
        let motion = motion_weights(motion, factor);
        for (tile, (pixels, weights)) in image.data.chunks_mut(w * ROWS).zip(motion.data.chunks(w * ROWS)).enumerate() {
            check(progress, 0.8 + 0.2 * (tile * ROWS) as f32 / h as f32, "Replacing moving areas")?;
            pixels.par_chunks_mut(w).zip(weights.par_chunks(w)).enumerate().for_each(|(row, (pixels, weights))| {
                let y = tile * ROWS + row;
                for (x, (pixel, &weight)) in pixels.iter_mut().zip(weights).enumerate() {
                    if weight > 0.0 {
                        // First shot has (+1,0) displacement, plus the first half phase
                        // (+0.5,0) in a sixteen-shot capture. Sample the uncropped fallback
                        // so default crop margins remain available at the output edges.
                        let px = origin_x as f32 - 1.0 + x as f32 / factor as f32 - if n == 16 { 0.5 } else { 0.0 };
                        let py = origin_y as f32 + y as f32 / factor as f32;
                        let single = fallback.sample_bilinear(px + 0.5, py + 0.5);
                        *pixel = if weight >= 1.0 { single } else { std::array::from_fn(|c| pixel[c] + (single[c] - pixel[c]) * weight) };
                    }
                }
            });
        }
        return Ok(PixelShiftResult {
            image,
            color: camera_color(&source),
            orientation,
            metadata: source.metadata,
            baseline_exposure: source.color.baseline_exposure,
            motion,
            shots: n,
            clip,
        });
    }
    check(progress, 1.0, "Pixel Shift ready")?;
    Ok(PixelShiftResult {
        image,
        color: camera_color(first),
        orientation,
        metadata: first.metadata.clone(),
        baseline_exposure: first.color.baseline_exposure,
        motion,
        shots: n,
        clip,
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
        sized_sources(count, 24, 20, scene)
    }

    fn sized_sources(count: usize, width: usize, height: usize, scene: impl Fn(f32, f32) -> [f32; 3]) -> Vec<RawImage> {
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
                let data = (0..width * height)
                    .map(|p| {
                        let (x, y) = (p % width, p / width);
                        scene(x as f32 + dx, y as f32 + dy)[cfa.color_at(x, y) as usize] * 1000.0 + 64.0
                    })
                    .collect();
                RawImage {
                    vendor_lens: None,
                    format: RawFormat::Arw,
                    width,
                    height,
                    cpp: 1,
                    data: RawData::F32(data),
                    cfa: Some(cfa),
                    bits: 14,
                    black: BlackLevel::uniform(64.0),
                    white: vec![1064.0],
                    active_area: Rect::new(0, 0, width, height),
                    crop: Rect::new(2, 2, width - 4, height - 4),
                    orientation: Orientation::Normal,
                    color: ColorData {
                        illuminant: [21, 0],
                        color_matrix: [Some(li_color::Mat3::IDENTITY), None],
                        forward_matrix: [Some(li_color::Mat3::IDENTITY), None],
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
                Some(result.clip),
            )
            .unwrap();
            let decoded = lightcraft_raw::decode(&dng).unwrap();
            assert_eq!(decoded.cpp, 3);
            assert!(decoded.cfa.is_none());
            let neutral = decoded.color.as_shot_neutral.unwrap();
            for (actual, expected) in neutral.into_iter().zip([0.5, 1.0, 2.0 / 3.0]) {
                assert!((actual - expected).abs() < 1e-6, "source WB is retained in merged DNG: {neutral:?}");
            }
            assert_eq!(decoded.color.color_matrix[0], Some(li_color::Mat3::IDENTITY));
            assert_eq!(decoded.color.forward_matrix[0], Some(li_color::Mat3::IDENTITY));
            assert_eq!(decoded.color.profile.tone_curve.as_ref().unwrap().points, vec![[0.0, 0.0], [0.5, 0.45], [1.0, 1.0]]);
            let developed = decoded.develop(lightcraft_raw::Method::Ahd).unwrap();
            assert_eq!(developed, result.image);
        }
    }

    #[test]
    fn pixelshift_reconstruction_spans_row_batches_exactly() {
        let result = merge_pixel_shift(sized_sources(16, 72, 100, scene), &crate::no_progress).unwrap();
        assert_eq!((result.image.width, result.image.height), (136, 192));
        let mut max_error = 0.0f32;
        for y in 0..result.image.height {
            for x in 0..result.image.width {
                let expected = scene(2.0 + x as f32 / 2.0, 2.0 + y as f32 / 2.0);
                for (a, b) in result.image.get(x, y).iter().zip(expected) {
                    max_error = max_error.max((a - b).abs());
                }
            }
        }
        assert!(max_error < 1e-6, "maximum camera RGB error {max_error}");
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
        let measured = merge_pixel_shift(sources(4), &crate::no_progress).unwrap();
        let result = merge_pixel_shift(raws, &crate::no_progress).unwrap();
        assert!(result.motion.data.iter().filter(|v| **v > 0.0).count() > 25);
        assert_eq!(result.motion.get(7, 7), 1.0);
        assert_eq!(result.image.get(7, 7), fallback.get(8, 9));
        assert_eq!(result.motion.get(17, 13), 0.0);
        // The fallback fades in over the motion's edge.
        let (i, weight) = result.motion.data.iter().copied().enumerate().find(|(_, v)| *v > 0.05 && *v < 0.95).expect("feathered edge");
        let (x, y) = (i % result.image.width, i / result.image.width);
        let single = fallback.sample_bilinear(x as f32 + 1.5, y as f32 + 2.5);
        for c in 0..3 {
            let want = measured.image.get(x, y)[c] + (single[c] - measured.image.get(x, y)[c]) * weight;
            assert!((result.image.get(x, y)[c] - want).abs() < 1e-6, "({x},{y}) blends measured and fallback by {weight}");
        }
    }

    #[test]
    fn pixelshift_equalizes_shot_brightness() {
        let gentle = |x: f32, y: f32| [0.3 + 0.0002 * x + 0.0001 * y, 0.35 + 0.0001 * x + 0.0002 * y, 0.4 + 0.0002 * x - 0.0001 * y];
        let clean = merge_pixel_shift(sized_sources(4, 160, 128, gentle), &crate::no_progress).unwrap();
        let mut raws = sized_sources(4, 160, 128, gentle);
        // The third shot caught a 3 % brighter phase of the room's LED lighting.
        if let RawData::F32(data) = &mut raws[2].data {
            data.iter_mut().for_each(|v| *v = (*v - 64.0) * 1.03 + 64.0);
        }
        let flickered = merge_pixel_shift(raws, &crate::no_progress).unwrap();
        assert!(flickered.motion.data.iter().all(|v| *v == 0.0));
        let largest =
            flickered.image.data.iter().zip(&clean.image.data).flat_map(|(a, b)| (0..3).map(move |c| (a[c] / b[c] - 1.0).abs())).fold(0.0, f32::max);
        assert!(largest < 0.002, "relative difference from the steady capture {largest}");
        let exact = clean.image.data.iter().enumerate().all(|(i, p)| {
            let expected = gentle(2.0 + (i % clean.image.width) as f32, 2.0 + (i / clean.image.width) as f32);
            (0..3).all(|c| (p[c] - expected[c]).abs() < 1e-6)
        });
        assert!(exact, "a steady capture is not equalized");
    }

    #[test]
    fn pixelshift_isolated_detection_is_not_motion() {
        let mut raws = sources(4);
        if let RawData::F32(data) = &mut raws[1].data {
            data[9 * 24 + 11] += 300.0;
        }
        let result = merge_pixel_shift(raws, &crate::no_progress).unwrap();
        assert!(result.motion.data.iter().all(|v| *v == 0.0), "a single outlying sample must not pull in the fallback");
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
        // Independent noiseless integer sensor measurements. A 20-DN temporal change (6 %
        // of the local green) is above quantization and the photosites' 1 % gain spread.
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
                        data[y * 24 + x] += 20;
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
        let image = RgbFrame::from_fn(12, 10, |x, y| [-0.001953125 - x as f32 * 0.00000003, 0.1234567 + y as f32 * 0.000001, 0.6]);
        let color = FrameColor::Camera(Box::default());
        let bytes = crate::write_linear_dng(&image, &color, Orientation::Normal, &Metadata::default(), 0.0, crate::DngSamples::Float, None).unwrap();
        let decoded = lightcraft_raw::decode(&bytes).unwrap();
        let developed = decoded.develop(lightcraft_raw::Method::Auto).unwrap();
        assert_eq!(developed, image, "FP32 linear DNG preserves negative sensor noise and fractional samples bit for bit");
    }

    #[test]
    fn pixelshift_clipped_samples_land_on_the_dng_white_level() {
        let mut raws = sources(4);
        for raw in &mut raws {
            if let RawData::F32(data) = &mut raw.data {
                for y in 6..14 {
                    for x in 6..16 {
                        data[y * 24 + x] = 1064.0;
                    }
                }
            }
        }
        let result = merge_pixel_shift(raws, &crate::no_progress).unwrap();
        assert_eq!(result.clip, 1.0);
        let dng = crate::write_linear_dng(
            &result.image,
            &result.color,
            result.orientation,
            &result.metadata,
            result.baseline_exposure,
            crate::DngSamples::Float,
            Some(result.clip),
        )
        .unwrap();
        let reopened = lightcraft_raw::decode(&dng).unwrap().normalized().unwrap();
        let at = |x: usize, y: usize| -> [f32; 3] { std::array::from_fn(|c| reopened.data[(y * reopened.width + x) * 3 + c]) };
        // Output (x, y) reads sensor (x + 2, y + 2) and its shifted neighbours.
        for y in 7..10 {
            for x in 7..12 {
                assert!(at(x, y).iter().all(|v| (0.99..=1.0).contains(v)), "clipped block at {x},{y}: {:?}", at(x, y));
            }
        }
        // Measured values keep their scale below the white level.
        let scene_value = scene(2.0, 2.0);
        for c in 0..3 {
            assert!((at(0, 0)[c] - scene_value[c]).abs() < 1e-5, "{:?} vs {scene_value:?}", at(0, 0));
        }
    }

    #[test]
    fn pixelshift_saturation_reads_the_sensor_plateau_in_normalized_units() {
        let (w, h) = (64, 64);
        let cfa = Cfa::bayer("RGGB").unwrap();
        // Half the frame measured (64..900 DN), half piled up 64 DN below the stated white.
        let data = (0..w * h).map(|p| if p % w < w / 2 { 64 + ((p % w) * 26 + p / w) as u16 } else { 1000 }).collect();
        let raw = RawImage {
            data: RawData::U16(data),
            width: w,
            height: h,
            active_area: Rect::new(0, 0, w, h),
            crop: Rect::new(0, 0, w, h),
            cfa: Some(cfa),
            ..sources(4).remove(0)
        };
        assert!((saturation(&raw) - 936.0 / 1000.0).abs() < 1e-6, "{}", saturation(&raw));
        let unclipped = RawImage { data: RawData::U16(vec![500; w * h]), ..raw };
        assert_eq!(saturation(&unclipped), 1.0);
    }

    #[test]
    fn pixelshift_ignores_an_orientation_tag_that_changed_mid_capture() {
        let mut raws = sources(4);
        raws[2].orientation = Orientation::Rotate90;
        let result = merge_pixel_shift(raws, &crate::no_progress).unwrap();
        assert_eq!(result.orientation, Orientation::Normal);
        assert_eq!((result.image.width, result.image.height), (20, 16));
        assert!(result.motion.data.iter().all(|v| *v == 0.0));
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
