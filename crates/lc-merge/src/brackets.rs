//! Conservative shoot-wide bracket grouping. Exposure/time gates follow Lightroom's
//! documented batch-HDR requirement for consistently bracketed captures; scene evidence
//! prevents nearby rooms/camera positions from being combined. Uncertain singles stay out.
//! The grouping thresholds are our own policy, not a reproduction of Adobe's algorithm.

use serde::{Deserialize, Serialize};

use crate::Frame;

/// Small, exposure-independent descriptor; the original sensor/photo need not be retained.
#[derive(Clone, Debug)]
pub struct BracketProbe {
    pub exposure: Option<f64>,
    pub capture_seconds: Option<f64>,
    /// Shutter time in seconds; long exposures (and in-camera dark frames) delay the next shot.
    pub exposure_seconds: Option<f64>,
    pub width: usize,
    pub height: usize,
    pub camera: (Option<String>, Option<String>),
    /// Centred, unit-length log-luminance grid. Flat/black frames have a zero descriptor.
    pub scene: [f32; 64],
}

impl BracketProbe {
    pub fn from_frame(frame: &Frame) -> Self {
        let mut scene = [0.0; 64];
        if frame.width() > 0 && frame.height() > 0 {
            for gy in 0..8 {
                for gx in 0..8 {
                    let x0 = gx * frame.width() / 8;
                    let y0 = gy * frame.height() / 8;
                    let x1 = ((gx + 1) * frame.width() / 8).max(x0 + 1).min(frame.width());
                    let y1 = ((gy + 1) * frame.height() / 8).max(y0 + 1).min(frame.height());
                    let mut sum = 0.0;
                    let mut count = 0;
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let p = frame.image.data[y * frame.width() + x];
                            let luma = 0.25 * p[0] + 0.6 * p[1] + 0.15 * p[2];
                            if luma.is_finite() {
                                sum += luma.max(1e-6).ln();
                                count += 1;
                            }
                        }
                    }
                    scene[gy * 8 + gx] = sum / count.max(1) as f32;
                }
            }
            let mean = scene.iter().sum::<f32>() / 64.0;
            for v in &mut scene {
                *v -= mean;
            }
            let norm = scene.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 1e-4 {
                for v in &mut scene {
                    *v /= norm;
                }
            } else {
                scene.fill(0.0);
            }
        }
        Self {
            exposure: frame.exposure.filter(|e| e.is_finite()),
            capture_seconds: frame.metadata.capture_time.map(|t| t.unix_seconds() as f64 + t.millis as f64 / 1000.0),
            exposure_seconds: frame.metadata.exposure_time.filter(|t| t.is_finite() && *t > 0.0),
            width: frame.width(),
            height: frame.height(),
            camera: (frame.metadata.make.clone(), frame.metadata.model.clone()),
            scene,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BracketOptions {
    pub max_gap_seconds: f64,
    pub ev_tolerance: f64,
    pub min_ev_span: f64,
    pub min_scene_correlation: f32,
}

impl Default for BracketOptions {
    fn default() -> Self {
        Self { max_gap_seconds: 10.0, ev_tolerance: 0.2, min_ev_span: 0.5, min_scene_correlation: 0.8 }
    }
}

/// Return disjoint index groups in capture order. Requires time + EV + scene evidence,
/// distinct exposures, and evenly spaced EV steps (any capture order). A repeated EV,
/// time gap, changed scene/camera, or missing metadata starts a new set. Never discards
/// singles from the caller's source list; they simply do not appear in the result.
pub fn detect_groups(probes: &[BracketProbe], options: &BracketOptions) -> Vec<Vec<usize>> {
    if !options.max_gap_seconds.is_finite()
        || options.max_gap_seconds < 0.0
        || !options.ev_tolerance.is_finite()
        || options.ev_tolerance < 0.0
        || !options.min_ev_span.is_finite()
        || options.min_ev_span <= options.ev_tolerance
        || !options.min_scene_correlation.is_finite()
        || !(0.0..=1.0).contains(&options.min_scene_correlation)
    {
        return Vec::new();
    }
    let known = |p: &BracketProbe| {
        p.exposure.is_some_and(f64::is_finite)
            && p.capture_seconds.is_some_and(f64::is_finite)
            && p.width > 0
            && p.height > 0
            && p.scene.iter().all(|v| v.is_finite())
            && p.scene.iter().map(|v| v * v).sum::<f32>() > 0.5
    };
    let mut indices: Vec<_> = (0..probes.len()).collect();
    indices.sort_by(|&a, &b| probes[a].capture_seconds.unwrap_or_default().total_cmp(&probes[b].capture_seconds.unwrap_or_default()).then(a.cmp(&b)));
    let mut groups = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for i in indices {
        let p = &probes[i];
        if !known(p) {
            finish(&mut current, probes, options, &mut groups);
            continue;
        }
        let joins = current.last().is_none_or(|&last| {
            let first = &probes[current[0]];
            let prior = &probes[last];
            let correlation: f32 = first.scene.iter().zip(p.scene).map(|(a, b)| a * b).sum();
            // The gap between shots excludes exposure time, whether the camera stamps the start or
            // the end of an exposure, and the dark frame of long-exposure noise reduction.
            let exposing = 2.0 * prior.exposure_seconds.unwrap_or(0.0) + p.exposure_seconds.unwrap_or(0.0);
            p.width == first.width
                && p.height == first.height
                && p.camera == first.camera
                && p.capture_seconds.unwrap_or_default() - prior.capture_seconds.unwrap_or_default() - exposing <= options.max_gap_seconds
                && correlation >= options.min_scene_correlation
                && current.iter().all(|&j| (probes[j].exposure.unwrap_or_default() - p.exposure.unwrap_or_default()).abs() > options.ev_tolerance)
        });
        if !joins {
            finish(&mut current, probes, options, &mut groups);
        }
        current.push(i);
    }
    finish(&mut current, probes, options, &mut groups);
    groups
}

fn finish(current: &mut Vec<usize>, probes: &[BracketProbe], options: &BracketOptions, groups: &mut Vec<Vec<usize>>) {
    if current.len() >= 2 {
        let mut ev: Vec<_> = current.iter().map(|&i| probes[i].exposure.unwrap_or_default()).collect();
        ev.sort_by(f64::total_cmp);
        let step = (ev[ev.len() - 1] - ev[0]) / (ev.len() - 1) as f64;
        if ev[ev.len() - 1] - ev[0] >= options.min_ev_span && ev.windows(2).all(|p| ((p[1] - p[0]) - step).abs() <= options.ev_tolerance) {
            groups.push(std::mem::take(current));
            return;
        }
    }
    current.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use li_raster::RgbFrame;

    fn probe(ev: f64, seconds: f64, scene: bool) -> BracketProbe {
        let image = RgbFrame::from_fn(32, 24, |x, y| {
            let v = if scene { (x * x + y * 3 + 1) as f32 } else { (y * y + x * 3 + 1) as f32 } / 2048.0;
            [v * 2f32.powf(ev as f32); 3]
        });
        let mut p = BracketProbe::from_frame(&Frame::from_linear_rec2020(image, Some(ev)));
        p.capture_seconds = Some(seconds);
        p
    }

    #[test]
    fn brackets_split_ev_cycles_time_and_scene() {
        let p = vec![
            probe(0.0, 0.0, true),
            probe(-2.0, 1.0, true),
            probe(2.0, 2.0, true),
            probe(0.0, 3.0, true),
            probe(-2.0, 4.0, true),
            probe(2.0, 5.0, true),
            probe(0.0, 6.0, false),
            probe(-2.0, 7.0, false),
            probe(2.0, 8.0, false),
            probe(0.0, 30.0, true),
            probe(-2.0, 31.0, true),
            probe(2.0, 32.0, true),
        ];
        assert_eq!(detect_groups(&p, &BracketOptions::default()), vec![vec![0, 1, 2], vec![3, 4, 5], vec![6, 7, 8], vec![9, 10, 11]]);
    }

    #[test]
    fn brackets_allow_for_long_exposures_between_shots() {
        // A twilight set: +2 and +4 EV frames of 4 s and 15 s push the next shot's start back.
        let shots = [(0.0, 0.0, 1.0), (-2.0, 1.5, 0.25), (2.0, 3.0, 4.0), (-4.0, 13.5, 0.0625), (4.0, 14.2, 15.0)];
        let mut p: Vec<_> = shots
            .iter()
            .map(|&(ev, start, shutter)| {
                let mut probe = probe(ev, start, true);
                probe.exposure_seconds = Some(shutter);
                probe
            })
            .collect();
        assert_eq!(detect_groups(&p, &BracketOptions::default()), vec![vec![0, 1, 2, 3, 4]]);
        // The next set, shot after the long frame plus a real pause, stays separate.
        for &(ev, start, shutter) in &shots {
            let mut probe = probe(ev, 60.0 + start, true);
            probe.exposure_seconds = Some(shutter);
            p.push(probe);
        }
        assert_eq!(detect_groups(&p, &BracketOptions::default()), vec![vec![0, 1, 2, 3, 4], vec![5, 6, 7, 8, 9]]);
    }

    #[test]
    fn brackets_reject_unknown_flat_irregular_and_invalid_options() {
        let mut p = vec![probe(0.0, 0.0, true), probe(1.0, 1.0, true), probe(4.0, 2.0, true)];
        assert!(detect_groups(&p, &BracketOptions::default()).is_empty());
        p[1].capture_seconds = None;
        p[2].scene.fill(0.0);
        assert!(detect_groups(&p, &BracketOptions::default()).is_empty());
        p[1].capture_seconds = Some(1.0);
        let opts = BracketOptions { ev_tolerance: f64::NAN, ..Default::default() };
        assert!(detect_groups(&p, &opts).is_empty());
    }

    #[test]
    fn brackets_sort_capture_order_and_keep_indices() {
        let p = vec![probe(2.0, 2.0, true), probe(-2.0, 0.0, true), probe(0.0, 1.0, true)];
        assert_eq!(detect_groups(&p, &BracketOptions::default()), vec![vec![1, 2, 0]]);
    }
}
