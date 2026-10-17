//! Sensor-domain HDR: normalize each sensor observation, register reduced CFA-free
//! proxies with the shared core, fuse the mosaic, then demosaic exactly once.
//!
//! Same-colour period-two tent interpolation follows darktable's documented CFA
//! warp design (§2.2):
//! <https://github.com/darktable-org/darktable/blob/master/dev-doc/HDR_Alignment_Design.md>.
//! The positive kernel cannot introduce ringing or mix CFA colours. Fractional
//! registration necessarily interpolates sparse sensor samples; this is not a
//! claim of recovering detail above the measured CFA sampling resolution.
//! Native black/white calibration, DNG opcodes 1/2, and normalized camera colour
//! use `lightcraft_raw`; no tone mapping or white balance is baked into the result.

use li_raster::{GrayFrame, RgbFrame};
use lightcraft_geom::{Homography, Point};
use lightcraft_raw::{Normalized, NormalizedSamples, RawImage, Rect};
use rayon::prelude::*;

use crate::frame::{Frame, FrameColor, exif_exposure};
use crate::hdr::{self, Deghost, HdrOptions, HdrResult, smoothstep};
use crate::{MergeError, Progress, Result, align, check};

fn decode_error(e: lightcraft_raw::RawError) -> MergeError {
    MergeError::Decode(e.to_string())
}

/// Camera metadata in the normalized sensor coordinates used by the output.
fn camera_color(raw: &RawImage) -> FrameColor {
    let mut color = lightcraft_raw::color::normalized_color(raw);
    if color.as_shot_neutral.is_none()
        && color.as_shot_white_xy.is_none()
        && let Some(wb) = raw.wb_multipliers.filter(|w| w.iter().all(|v| v.is_finite() && *v > 0.0))
    {
        let ratios = lightcraft_raw::color::normalization_ratios(raw);
        color.as_shot_neutral = Some(std::array::from_fn(|c| ratios[c] / f64::from(wb[c])));
    }
    FrameColor::Camera(Box::new(color))
}

/// Complete CFA tiles form an unbiased camera-RGB proxy without demosaicing the
/// inputs. Integer bins give an exact isotropic proxy→sensor coordinate mapping;
/// the small trailing partial bin is excluded only from registration, not fusion.
fn proxy(raw: &RawImage, samples: &NormalizedSamples<'_>, factor: usize, gain: [f32; 3]) -> Frame {
    let (w, h) = (samples.width / factor, samples.height / factor);
    let mut image = RgbFrame::new(w, h);
    image.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, pixel) in row.iter_mut().enumerate() {
            let (mut sums, mut counts) = ([0.0f64; 3], [0usize; 3]);
            for sy in y * factor..(y + 1) * factor {
                for sx in x * factor..(x + 1) * factor {
                    if samples.cpp == 3 {
                        for c in 0..3 {
                            sums[c] += f64::from(samples.get(sx, sy, c));
                            counts[c] += 1;
                        }
                    } else if let Some(cfa) = &samples.cfa {
                        let c = cfa.color_at(sx, sy) as usize;
                        sums[c] += f64::from(samples.get(sx, sy, 0));
                        counts[c] += 1;
                    } else {
                        for c in 0..3 {
                            sums[c] += f64::from(samples.get(sx, sy, 0));
                            counts[c] += 1;
                        }
                    }
                }
            }
            *pixel = std::array::from_fn(|c| (sums[c] / counts[c].max(1) as f64) as f32 * gain[c]);
        }
    });
    Frame {
        image,
        clip: gain.into_iter().fold(f32::INFINITY, f32::min),
        exposure: exif_exposure(&raw.metadata),
        raw: true,
        color: camera_color(raw),
        orientation: raw.orientation,
        metadata: raw.metadata.clone(),
        baseline_exposure: raw.color.baseline_exposure,
    }
}

/// Linear interpolation on one phase's lattice. `sx/sy` are pixel-centre
/// coordinates. At exact sensor centres this reads only the measured sample,
/// including the last row/column. A fractional footprint outside the sensor is
/// invalid rather than extended using unrelated colour sites.
fn sample_phase(samples: &NormalizedSamples<'_>, sx: f64, sy: f64, phase: (usize, usize), period: usize, channel: usize) -> Option<(f32, f32)> {
    let (px, py) = phase;
    if sx < 0.5 || sy < 0.5 || sx > samples.width as f64 - 0.5 || sy > samples.height as f64 - 0.5 {
        return None;
    }
    let (u, v) = ((sx - 0.5 - px as f64) / period as f64, (sy - 0.5 - py as f64) / period as f64);
    if !u.is_finite() || !v.is_finite() {
        return None;
    }
    let (ix, iy) = (u.floor(), v.floor());
    let (tx, ty) = (u - ix, v - iy);
    let (mut value, mut saturation) = (0.0, f32::NEG_INFINITY);
    for (dy, wy) in [(0, 1.0 - ty), (1, ty)] {
        for (dx, wx) in [(0, 1.0 - tx), (1, tx)] {
            let weight = wx * wy;
            // Zero-weight taps need not exist; identity resampling is exact at borders.
            if weight <= 1e-12 {
                continue;
            }
            let (x, y) = ((ix as isize + dx) * period as isize + px as isize, (iy as isize + dy) * period as isize + py as isize);
            if x < 0 || y < 0 || x as usize >= samples.width || y as usize >= samples.height {
                return None;
            }
            let observation = samples.get(x as usize, y as usize, channel);
            if !observation.is_finite() {
                return None;
            }
            value += weight as f32 * observation;
            saturation = saturation.max(observation);
        }
    }
    Some((value, saturation))
}

/// Preserve all channels together when a contributor is clipped. CFA inputs use
/// the local whole tile and every tap used to interpolate the requested phase;
/// saturation cannot disappear by averaging a clipped tap with a dark neighbour.
fn observation(samples: &NormalizedSamples<'_>, h: &Homography, x: usize, y: usize, channel: usize) -> Option<(f32, f32)> {
    if *h == Homography::IDENTITY {
        let value = samples.get(x, y, channel);
        let saturation = if samples.cpp == 3 {
            (0..3).map(|c| samples.get(x, y, c)).fold(f32::NEG_INFINITY, f32::max)
        } else if let Some(cfa) = &samples.cfa {
            let (ox, oy) = (x / cfa.width * cfa.width, y / cfa.height * cfa.height);
            (oy..(oy + cfa.height).min(samples.height))
                .flat_map(|sy| (ox..(ox + cfa.width).min(samples.width)).map(move |sx| samples.get(sx, sy, 0)))
                .fold(f32::NEG_INFINITY, f32::max)
        } else {
            value
        };
        return (value.is_finite() && saturation.is_finite()).then_some((value, saturation));
    }
    let point = h.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
    if samples.cpp == 3 {
        let (value, mut saturation) = sample_phase(samples, point.x, point.y, (0, 0), 1, channel)?;
        for c in 0..3 {
            saturation = saturation.max(sample_phase(samples, point.x, point.y, (0, 0), 1, c)?.1);
        }
        Some((value, saturation))
    } else if samples.cfa.is_some() {
        let (value, mut saturation) = sample_phase(samples, point.x, point.y, (x % 2, y % 2), 2, 0)?;
        // Nearest CFA tile checks all colours without reconstructing an RGB frame.
        let (ox, oy) = ((point.x - 0.5).floor().max(0.0) as usize / 2 * 2, (point.y - 0.5).floor().max(0.0) as usize / 2 * 2);
        for sy in oy..(oy + 2).min(samples.height) {
            for sx in ox..(ox + 2).min(samples.width) {
                saturation = saturation.max(samples.get(sx, sy, 0));
            }
        }
        Some((value, saturation))
    } else {
        sample_phase(samples, point.x, point.y, (0, 0), 1, 0)
    }
}

/// Merge calibrated raw brackets to reference-exposure scene-linear camera RGB.
/// Only small registration/deghost proxies are developed before fusion; sensor
/// data is merged before the single final demosaic. The result can be written as
/// a 32-bit LinearRaw DNG using [`crate::DngSamples::Float`].
///
/// Geometry/camera calibration must match. Bayer, monochrome and LinearRaw support
/// fractional alignment. Other CFA layouts support the exact unaligned path;
/// requesting alignment for an irregular CFA is rejected rather than treating it
/// as a Bayer lattice. Negative sensor noise remains signed until development.
pub fn merge_raw_hdr(raws: &[RawImage], options: &HdrOptions, progress: &Progress) -> Result<HdrResult> {
    check(progress, 0.0, "Checking raw brackets")?;
    if raws.len() < 2 {
        return Err(MergeError::TooFew(raws.len(), 2));
    }
    let first = &raws[0];
    for raw in raws {
        raw.validate().map_err(decode_error)?;
        if !matches!(raw.cpp, 1 | 3)
            || raw.active_area != first.active_area
            || raw.crop != first.crop
            || raw.cfa != first.cfa
            || raw.cpp != first.cpp
            || raw.orientation != first.orientation
            || raw.metadata.make != first.metadata.make
            || raw.metadata.model != first.metadata.model
            || raw.color.color_matrix != first.color.color_matrix
            || raw.color.forward_matrix != first.color.forward_matrix
            || raw.color.camera_calibration != first.color.camera_calibration
            || raw.color.illuminant != first.color.illuminant
        {
            return Err(MergeError::Mismatch("Raw HDR requires matching camera calibration, sensor layout, crop and orientation".into()));
        }
        if options.align && raw.cfa.as_ref().is_some_and(|c| !c.is_bayer()) {
            return Err(MergeError::Mismatch(
                "Subpixel raw HDR alignment currently requires a Bayer sensor; disable alignment for tripod brackets on other CFA layouts".into(),
            ));
        }
    }
    let samples: Vec<_> = raws.iter().map(|r| r.normalized_samples().map_err(decode_error)).collect::<Result<_>>()?;
    let (w, h, cpp) = (samples[0].width, samples[0].height, samples[0].cpp);
    if w < 4 || h < 4 {
        return Err(MergeError::Mismatch("Raw HDR sources must be at least four pixels per side".into()));
    }
    let period = samples[0].cfa.as_ref().map_or(1, |c| c.width.max(c.height));
    let factor = w.max(h).div_ceil(align::WORK_EDGE).div_ceil(period).max(1) * period;
    if w / factor < 2 || h / factor < 2 {
        return Err(MergeError::Mismatch("Raw HDR sensor aspect ratio is unsupported".into()));
    }
    let ratios: Vec<_> = raws.iter().map(|r| lightcraft_raw::color::normalized_color(r).analog_balance.unwrap_or([1.0; 3])).collect();
    if ratios.iter().flatten().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err(MergeError::Mismatch("Raw HDR requires finite positive camera analog balance".into()));
    }
    let mut work: Vec<_> = raws
        .iter()
        .zip(&samples)
        .enumerate()
        .map(|(i, (r, n))| proxy(r, n, factor, std::array::from_fn(|c| (ratios[0][c] / ratios[i][c]) as f32)))
        .collect();
    check(progress, 0.10, "Preparing sensor registration")?;
    let mut result = hdr::merge_hdr(work.clone(), &HdrOptions { align: options.align, deghost: Deghost::None }, &|fraction, stage| {
        progress(0.10 + fraction * 0.32, stage)
    })?;
    let (n, reference) = (raws.len(), result.reference);
    let k: Vec<f32> = result.ev.iter().map(|e| 2.0f64.powf(*e) as f32).collect();
    if k.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err(MergeError::Mismatch("Raw bracket exposure ratio is not finite".into()));
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| k[a].total_cmp(&k[b]).then(a.cmp(&b)));
    let mut valid = vec![None; n];
    for i in 0..n {
        if result.alignments[i].h != Homography::IDENTITY {
            let (warped, mask) = align::warp(&work[i].image, &result.alignments[i].h);
            work[i].image = warped;
            valid[i] = Some(mask);
        }
    }
    check(progress, 0.45, "Detecting sensor ghosts")?;
    let ghosts = options.deghost.params().map(|(threshold, radius)| hdr::deghost_maps(&work, &valid, &k, &order, reference, threshold, radius));
    for alignment in &mut result.alignments {
        alignment.h = align::scale_homography(&alignment.h, 1.0 / factor as f64);
        alignment.rms *= factor as f64;
    }
    let gains: Vec<[f32; 3]> = (0..n).map(|i| std::array::from_fn(|c| (ratios[reference][c] / ratios[i][c]) as f32)).collect();
    let mut merged = Normalized { width: w, height: h, cpp, cfa: samples[0].cfa.clone(), data: vec![0.0; w * h * cpp] };
    let darkest = order[0];
    // Row groups bound cancellation latency without invoking the callback from workers.
    for (tile_y, tile) in merged.data.chunks_mut(w * cpp * 64).enumerate() {
        check(progress, 0.5 + 0.3 * (tile_y * 64) as f32 / h as f32, "Fusing sensor observations")?;
        tile.par_chunks_mut(w * cpp).enumerate().for_each(|(row_y, row)| {
            let y = tile_y * 64 + row_y;
            for x in 0..w {
                for sample in 0..cpp {
                    let color = samples[0].cfa.as_ref().map_or(sample.min(2), |c| c.color_at(x, y) as usize);
                    let (mut sum, mut weights) = (0.0f64, 0.0f64);
                    for i in 0..n {
                        let Some((value, saturation)) = observation(&samples[i], &result.alignments[i].h, x, y, sample) else { continue };
                        let mut weight = k[i] * (1.0 - smoothstep(0.80, 0.96, saturation));
                        if i == darkest {
                            weight += 1e-6;
                        }
                        if i == reference {
                            weight += 1e-9;
                        }
                        if i != reference
                            && let Some((maps, ghost_factor)) = &ghosts
                        {
                            let scale = (factor * ghost_factor) as f32;
                            let ghost = hdr::sample_plane(&maps[i], (x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
                            weight *= 1.0 - ghost;
                        }
                        if weight > 0.0 {
                            sum += f64::from(weight) * f64::from(value) * f64::from(gains[i][color]) / f64::from(k[i]);
                            weights += f64::from(weight);
                        }
                    }
                    row[x * cpp + sample] = if weights > 0.0 { (sum / weights) as f32 } else { samples[reference].get(x, y, sample) };
                }
            }
        });
    }
    check(progress, 0.82, "Developing merged sensor")?;
    if merged.data.iter().any(|v| !v.is_finite()) {
        return Err(MergeError::Mismatch("Raw HDR fusion produced non-finite sensor samples".into()));
    }
    // AHD preserves scene-linear values above white, unlike algorithms whose
    // absolute [0,1] intermediate clamps assume a single-exposure sensor mosaic.
    result.radiance = lightcraft_raw::demosaic(&merged, lightcraft_raw::Method::Ahd);
    drop(merged);
    let ref_raw = &raws[reference];
    // The output has no source opcode list. Apply every operation in its declared
    // camera-RGB order after fusion, before cropping; splitting lens operations
    // out loses mandatory DJI corrections and changes mixed/per-plane lists.
    lightcraft_raw::opcodes::apply_list3(&ref_raw.opcodes.list3, &mut result.radiance);
    result.ghost = if let Some((maps, ghost_factor)) = &ghosts {
        let mut ghost = GrayFrame::new(w, h);
        let scale = (factor * ghost_factor) as f32;
        ghost.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, pixel) in row.iter_mut().enumerate() {
                *pixel = (0..n)
                    .filter(|&i| i != reference)
                    .map(|i| hdr::sample_plane(&maps[i], (x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale))
                    .fold(0.0, f32::max);
            }
        });
        ghost
    } else {
        GrayFrame::new(0, 0)
    };
    // The deghost overlay follows the same geometric sequence as the radiance.
    // It is scalar metadata: use the green plane and never apply shading/gain.
    if !result.ghost.data.is_empty() {
        let mut overlay = result.ghost.map(|v| [v; 3]);
        for op in &ref_raw.opcodes.list3 {
            match op {
                lightcraft_raw::Opcode::WarpRectilinear { planes, center } => {
                    if let Some(plane) = planes.get(1).or_else(|| planes.first()) {
                        overlay = lightcraft_raw::opcodes::warp_rectilinear(&overlay, &[*plane], *center);
                    }
                }
                lightcraft_raw::Opcode::WarpFisheye { planes, center } => {
                    if let Some(plane) = planes.get(1).or_else(|| planes.first()) {
                        overlay = lightcraft_raw::opcodes::warp_fisheye(&overlay, &[*plane], *center);
                    }
                }
                _ => {}
            }
        }
        result.ghost = overlay.map(|p| p[1]);
    }
    let crop = ref_raw.crop.clipped(w, h);
    let crop = if crop.width == 0 || crop.height == 0 { Rect::new(0, 0, w, h) } else { crop };
    if crop != Rect::new(0, 0, w, h) {
        result.radiance = result.radiance.into_crop(crop.x, crop.y, crop.width, crop.height);
        if result.ghost.width > 0 {
            result.ghost = GrayFrame {
                width: crop.width,
                height: crop.height,
                data: (crop.y..crop.y + crop.height)
                    .flat_map(|y| result.ghost.data[y * w + crop.x..y * w + crop.x + crop.width].iter().copied())
                    .collect(),
            };
        }
        // Public alignment coordinates correspond to the returned cropped image.
        let shift = Homography([1.0, 0.0, crop.x as f64, 0.0, 1.0, crop.y as f64, 0.0, 0.0, 1.0]);
        let unshift = Homography([1.0, 0.0, -(crop.x as f64), 0.0, 1.0, -(crop.y as f64), 0.0, 0.0, 1.0]);
        for alignment in &mut result.alignments {
            alignment.h = unshift.mul(&alignment.h).mul(&shift);
        }
    }
    result.color = camera_color(ref_raw);
    result.raw = true;
    result.orientation = ref_raw.orientation;
    result.metadata = crate::frame::developed_metadata(ref_raw);
    result.baseline_exposure = ref_raw.color.baseline_exposure;
    check(progress, 1.0, "Raw HDR ready")?;
    Ok(result)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lightcraft_raw::{BlackLevel, Cfa, ColorData, RawData, RawFormat};

    fn capture(w: usize, h: usize, exposure: f32, scene: impl Fn(f32, f32) -> [f32; 3], shift: (f32, f32)) -> RawImage {
        let cfa = Cfa::bayer("RGGB").unwrap();
        let black = BlackLevel { repeat_cols: 2, repeat_rows: 2, values: vec![32.0, 48.0, 48.0, 64.0], ..Default::default() };
        let data = (0..w * h)
            .map(|p| {
                let (x, y) = (p % w, p / w);
                let c = cfa.color_at(x, y) as usize;
                let b = black.at(x, y, 0, 1);
                b + (scene(x as f32 + 0.5 + shift.0, y as f32 + 0.5 + shift.1)[c] * exposure).min(1.0) * 4000.0
            })
            .collect();
        RawImage {
            vendor_lens: None,
            format: RawFormat::Arw,
            width: w,
            height: h,
            cpp: 1,
            data: RawData::F32(data),
            cfa: Some(cfa),
            bits: 32,
            black,
            white: vec![4032.0, 4048.0, 4064.0, 4048.0],
            active_area: Rect::new(0, 0, w, h),
            crop: Rect::new(0, 0, w, h),
            orientation: Default::default(),
            color: ColorData {
                illuminant: [21, 0],
                color_matrix: [Some(li_color::Mat3::IDENTITY), None],
                baseline_exposure: 0.25,
                ..Default::default()
            },
            wb_multipliers: Some([2.0, 1.0, 1.5]),
            linearized: true,
            opcodes: Default::default(),
            metadata: lightcraft_meta::Metadata {
                exposure_time: Some(f64::from(exposure) / 100.0),
                iso: Some(100),
                f_number: Some(4.0),
                make: Some("Synthetic".into()),
                model: Some("HDR sensor".into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn j19_raw_same_phase_subpixel_is_affine_exact_and_colour_preserving() {
        let scene = |x: f32, y: f32| [0.10 + 0.0001 * x, 0.30 + 0.0002 * y, 0.60 + 0.0001 * (x + y)];
        let raw = capture(40, 32, 1.0, scene, (0.0, 0.0));
        let samples = raw.normalized_samples().unwrap();
        for py in 0..2 {
            for px in 0..2 {
                let channel = raw.cfa.as_ref().unwrap().color_at(px, py) as usize;
                for (sx, sy) in [(12.75, 10.25), (18.10, 23.95)] {
                    let (value, _) = sample_phase(&samples, sx, sy, (px, py), 2, 0).unwrap();
                    let expected = scene(sx as f32, sy as f32)[channel];
                    assert!((value - expected).abs() < 2e-7, "phase ({px},{py}): {value} != {expected}");
                }
            }
        }
        let border = sample_phase(&samples, 39.5, 31.5, (1, 1), 2, 0).unwrap().0;
        assert!((border - scene(39.5, 31.5)[2]).abs() < 2e-7);
        assert!(sample_phase(&samples, 39.75, 31.5, (1, 1), 2, 0).is_none());
        // Interpolation must not hide a saturated observation in a dark average.
        let mut clipped = raw.clone();
        if let RawData::F32(data) = &mut clipped.data {
            data[0] = 4032.0;
            data[2] = 32.0;
        }
        let clipped = clipped.normalized_samples().unwrap();
        let (value, peak) = sample_phase(&clipped, 1.5, 0.5, (0, 0), 2, 0).unwrap();
        assert!((value - 0.5).abs() < 1e-7);
        assert!((peak - 1.0).abs() < 1e-7);
        assert_eq!(1.0 - smoothstep(0.80, 0.96, peak), 0.0);
    }

    fn window_scene(x: f32, y: f32) -> [f32; 3] {
        let value = if (95.0..145.0).contains(&x) && (25.0..105.0).contains(&y) { 4.0 } else { 0.001 + x * 0.0005 + y * 0.0002 };
        [value * 0.7, value, value * 0.4]
    }

    fn cfa_rmse(result: &HdrResult, scene: impl Fn(f32, f32) -> [f32; 3], border: usize) -> f64 {
        let (w, h) = (result.radiance.width, result.radiance.height);
        let cfa = Cfa::bayer("RGGB").unwrap();
        let (mut sum, mut count) = (0.0, 0);
        for y in border..h - border {
            for x in border..w - border {
                let c = cfa.color_at(x, y) as usize;
                let error = f64::from(result.radiance.get(x, y)[c] - scene(x as f32 + 0.5, y as f32 + 0.5)[c]);
                sum += error * error;
                count += 1;
            }
        }
        (sum / count as f64).sqrt()
    }

    fn old_frames(raws: &[RawImage]) -> Vec<Frame> {
        raws.iter()
            .map(|raw| Frame {
                image: raw.develop(lightcraft_raw::Method::Ahd).unwrap(),
                clip: 1.0,
                exposure: exif_exposure(&raw.metadata),
                raw: true,
                color: camera_color(raw),
                orientation: raw.orientation,
                metadata: raw.metadata.clone(),
                baseline_exposure: raw.color.baseline_exposure,
            })
            .collect()
    }

    #[test]
    fn j19_raw_hdr_preserves_window_headroom_and_demosaic_first_cfa_parity() {
        let raws: Vec<_> = [0.125, 1.0, 8.0].into_iter().map(|e| capture(192, 128, e, window_scene, (0.0, 0.0))).collect();
        let options = HdrOptions { align: false, deghost: Deghost::None };
        let merged = merge_raw_hdr(&raws, &options, &crate::no_progress).unwrap();
        let old = hdr::merge_hdr(old_frames(&raws), &options, &crate::no_progress).unwrap();
        let (new_rmse, old_rmse) = (cfa_rmse(&merged, window_scene, 8), cfa_rmse(&old, window_scene, 8));
        eprintln!("j19 CFA window RMSE new={new_rmse:.9} old={old_rmse:.9}");
        assert_eq!(merged.reference, 1);
        assert!(merged.radiance.get(120, 60)[1] > 3.99);
        assert!(new_rmse < 2e-6, "CFA radiance error {new_rmse}");
        assert!(new_rmse <= old_rmse + 2e-6, "sensor fusion {new_rmse} vs demosaic-first {old_rmse}");
        assert_eq!(merged.baseline_exposure, 0.25);
        assert!(matches!(&merged.color, FrameColor::Camera(_)));
        if let FrameColor::Camera(color) = &merged.color {
            assert_eq!(color.as_shot_neutral, Some([0.5, 1.0, 2.0 / 3.0]));
        }
    }

    #[test]
    fn j19_raw_hdr_motion_strengths_overlay_and_crop_match() {
        let mut raws: Vec<_> = [0.25, 1.0, 4.0].into_iter().map(|e| capture(192, 128, e, |_, _| [0.18, 0.2, 0.16], (0.0, 0.0))).collect();
        raws[2] = capture(
            192,
            128,
            4.0,
            |x, y| if (65.0..95.0).contains(&x) && (40.0..75.0).contains(&y) { [0.03, 0.02, 0.02] } else { [0.18, 0.2, 0.16] },
            (0.0, 0.0),
        );
        for raw in &mut raws {
            raw.crop = Rect::new(3, 5, 180, 116);
        }
        let none = merge_raw_hdr(&raws, &HdrOptions { align: false, deghost: Deghost::None }, &crate::no_progress).unwrap();
        let mut covered = Vec::new();
        for strength in [Deghost::Low, Deghost::Medium, Deghost::High] {
            let result = merge_raw_hdr(&raws, &HdrOptions { align: false, deghost: strength }, &crate::no_progress).unwrap();
            assert_eq!((result.radiance.width, result.radiance.height), (180, 116));
            assert_eq!((result.ghost.width, result.ghost.height), (180, 116));
            let p = (80 - 3, 55 - 5);
            assert!(result.ghost.data[p.1 * result.ghost.width + p.0] > 0.9);
            assert!((result.radiance.get(p.0, p.1)[1] - 0.2).abs() < 0.004);
            assert!((none.radiance.get(p.0, p.1)[1] - 0.2).abs() > 0.1);
            covered.push(result.ghost.data.iter().filter(|&&v| v > 0.05).count());
        }
        assert!(covered.windows(2).all(|v| v[1] >= v[0]), "strength coverage {covered:?}");
    }

    #[test]
    fn j19_raw_hdr_handheld_pipeline_recovers_fractional_sensor_shift() {
        let scene = lightcraft_scenes::demo_library().into_iter().find(|s| s.kind == lightcraft_scenes::Kind::OceanSunset).unwrap().render(384, 280);
        let mut levels: Vec<_> = scene.data.iter().map(|p| p[1]).collect();
        levels.sort_by(f32::total_cmp);
        let gain = 0.18 / levels[levels.len() / 2].max(1e-6);
        let sample = |x: f32, y: f32| scene.sample_bilinear(x + 16.0, y + 16.0).map(|v| v * gain);
        let shifts = [(2.25, -1.5), (0.0, 0.0), (-3.25, 1.75)];
        let raws: Vec<_> = [0.5, 1.0, 2.0].into_iter().zip(shifts).map(|(e, shift)| capture(352, 248, e, sample, shift)).collect();
        let result = merge_raw_hdr(&raws, &HdrOptions { align: true, deghost: Deghost::None }, &crate::no_progress).unwrap();
        assert_eq!(result.reference, 1);
        let mut largest = 0.0f64;
        for i in [0, 2] {
            eprintln!(
                "j19 handheld frame {i}: H={:?} model={} inliers={} rms={:.6}",
                result.alignments[i].h.0, result.alignments[i].model, result.alignments[i].inliers, result.alignments[i].rms
            );
            assert_ne!(result.alignments[i].model, "identity", "registration must be measured");
            assert!(result.alignments[i].inliers >= 5);
            for point in [Point::new(80.5, 80.5), Point::new(270.5, 160.5)] {
                let warped = result.alignments[i].h.apply(point);
                let want = Point::new(point.x - f64::from(shifts[i].0), point.y - f64::from(shifts[i].1));
                largest = largest.max(warped.dist(want));
            }
        }
        eprintln!("j19 handheld sensor alignment maximum error={largest:.6}px");
        assert!(largest < 0.5, "fractional sensor alignment error={largest}");
    }

    #[test]
    fn j19_raw_hdr_rejects_invalid_calibration_and_cancels() {
        let mut raws = vec![capture(24, 20, 0.5, |_, _| [0.18; 3], (0.0, 0.0)), capture(24, 20, 1.0, |_, _| [0.18; 3], (0.0, 0.0))];
        assert!(matches!(merge_raw_hdr(&raws, &HdrOptions::default(), &|_, _| false), Err(MergeError::Cancelled)));
        raws[1].color.color_matrix[0] = None;
        assert!(matches!(merge_raw_hdr(&raws, &HdrOptions::default(), &crate::no_progress), Err(MergeError::Mismatch(_))));
        raws[1].color = raws[0].color.clone();
        raws[0].cfa = Some(Cfa::xtrans());
        raws[1].cfa = Some(Cfa::xtrans());
        assert!(matches!(merge_raw_hdr(&raws, &HdrOptions::default(), &crate::no_progress), Err(MergeError::Mismatch(_))));
    }

    /// Owner originals and all derivatives stay outside versioned fixtures.
    /// Full decoding is sequential; only three 128×128 normalized CFA crops are
    /// retained. This is a real sensor smoke/parity sample, not a calibrated
    /// quality measurement (there is no independent scene radiance ground truth).
    #[test]
    #[ignore = "local owner originals; set LOCAL_IMAGE_OWNER_SHOTS to the authorized session root"]
    fn j19_owner_shots_raw_hdr_small_sensor_crop() {
        use std::io::Read;
        let Some(root) = std::env::var_os("LOCAL_IMAGE_OWNER_SHOTS") else {
            eprintln!("j19 owner sample skipped: LOCAL_IMAGE_OWNER_SHOTS is unset");
            return;
        };
        let root = std::path::PathBuf::from(root);
        let mut paths = Vec::new();
        for directory in [root.join("Capture"), root.join("Selects")] {
            let Ok(entries) = std::fs::read_dir(directory) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("arw")) {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        let mut headers = Vec::new();
        let mut selected = None;
        // TIFF EXIF is near the beginning of the source; reading only a prefix
        // avoids repeatedly reading/decompressing full sensor images for grouping.
        for path in paths.into_iter().take(48) {
            let mut bytes = Vec::new();
            std::fs::File::open(&path).unwrap().take(4 * 1024 * 1024).read_to_end(&mut bytes).unwrap();
            let metadata = lightcraft_meta::extract(&bytes);
            if let (Some(ev), Some(time)) = (exif_exposure(&metadata), metadata.capture_time) {
                headers.push((path, ev, time.unix_seconds()));
            }
            if headers.len() >= 3 {
                let recent = &headers[headers.len() - 3..];
                let mut evs: Vec<_> = recent.iter().map(|h| h.1).collect();
                evs.sort_by(f64::total_cmp);
                let oldest = recent.iter().map(|h| h.2).min().unwrap();
                let newest = recent.iter().map(|h| h.2).max().unwrap();
                if newest - oldest <= 12 && evs.windows(2).all(|v| v[1] - v[0] >= 0.5) {
                    selected = Some(recent.iter().map(|h| h.0.clone()).collect::<Vec<_>>());
                    break;
                }
            }
        }
        let selected = selected.expect("authorized owner sample must contain a three-exposure bracket in the first 48 ARWs");
        let mut crops = Vec::new();
        for path in selected {
            let bytes = std::fs::read(path).unwrap();
            let raw = lightcraft_raw::decode(&bytes).unwrap();
            drop(bytes);
            let normalized = raw.normalized_samples().unwrap();
            let (x0, y0) = ((normalized.width - 128) / 2, (normalized.height - 128) / 2);
            let mut data = Vec::with_capacity(128 * 128 * normalized.cpp);
            for y in y0..y0 + 128 {
                for x in x0..x0 + 128 {
                    for c in 0..normalized.cpp {
                        data.push(normalized.get(x, y, c));
                    }
                }
            }
            eprintln!(
                "j19 owner raw layout: cpp={} cfa={} source={}x{}",
                normalized.cpp,
                normalized.cfa.is_some(),
                normalized.width,
                normalized.height
            );
            let color = match camera_color(&raw) {
                FrameColor::Camera(color) => *color,
                FrameColor::Linear { .. } => ColorData::default(),
            };
            crops.push(RawImage {
                vendor_lens: None,
                format: RawFormat::Dng,
                width: 128,
                height: 128,
                cpp: normalized.cpp,
                data: RawData::F32(data),
                cfa: normalized.cfa.as_ref().map(|c| c.shifted(x0, y0)),
                black: BlackLevel::default(),
                white: vec![1.0],
                active_area: Rect::new(0, 0, 128, 128),
                crop: Rect::new(0, 0, 128, 128),
                bits: 32,
                color,
                orientation: raw.orientation,
                wb_multipliers: None,
                linearized: true,
                opcodes: Default::default(),
                metadata: raw.metadata.clone(),
            });
        }
        let options = HdrOptions { align: false, deghost: Deghost::None };
        let start = std::time::Instant::now();
        let result = merge_raw_hdr(&crops, &options, &crate::no_progress).unwrap();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let old = hdr::merge_hdr(old_frames(&crops), &options, &crate::no_progress).unwrap();
        assert!(result.radiance.data.iter().flatten().all(|v| v.is_finite()));
        assert_eq!((result.radiance.width, result.radiance.height), (128, 128));
        let reference = &crops[result.reference];
        let mut difference = 0.0f64;
        let mut observations = 0;
        for y in 8..120 {
            for x in 8..120 {
                for sample in 0..reference.cpp {
                    let c = reference.cfa.as_ref().map_or(sample, |cfa| cfa.color_at(x, y) as usize);
                    difference += f64::from(result.radiance.get(x, y)[c] - old.radiance.get(x, y)[c]).powi(2);
                    observations += 1;
                }
            }
        }
        let difference = (difference / observations as f64).sqrt();
        let max_radiance = result.radiance.data.iter().flatten().copied().fold(f32::NEG_INFINITY, f32::max);
        eprintln!(
            "j19 owner 3x128px sensor crop: fusion_ms={elapsed:.3} sensor_old_difference_RMSE={difference:.9} max_linear={max_radiance:.6} EV={:?}",
            result.ev
        );
        assert!(max_radiance > 0.0);
    }
}
