//! HDR toning of rendered sources; the merge itself is `lightcraft_merge::editor::hdr`.
#![allow(clippy::needless_range_loop)]
use crate::photo_util::{linear_to_srgb, srgb_to_linear};
use crate::tone::HdrToning;
#[cfg(test)]
use lightcraft_merge::editor::hdr::{MergeOptions, Merged, estimate_exposures, merge, merge_with_progress, mtb_offset, response_curve};
fn luma(c: [f32; 4]) -> f32 {
    li_color::luminance_709([c[0], c[1], c[2]])
}
// Tone mapping

/// The four 16/8-bit conversion methods of Merge to HDR Pro / HDR Toning.
#[derive(Clone, Debug, PartialEq)]
pub enum ToneMethod {
    LocalAdaptation(HdrToning),
    ExposureGamma { exposure: f32, gamma: f32 },
    HighlightCompression,
    EqualizeHistogram,
}

/// Tone maps linear radiance to display-encoded (sRGB curve) values in `0..=1`.
pub fn tone_map(px: &mut [[f32; 4]], w: usize, h: usize, method: &ToneMethod) {
    let n = px.iter().filter(|q| q[3] > 0.0).count().max(1);
    let log_mean = (px.iter().filter(|q| q[3] > 0.0).map(|q| (luma(*q).max(1e-6) as f64).ln()).sum::<f64>() / n as f64).exp() as f32;
    match method {
        ToneMethod::LocalAdaptation(p) => {
            // Key the scene to middle grey, compress in log luminance, then encode.
            let k = 0.18 / log_mean.max(1e-6);
            for q in px.iter_mut() {
                for c in 0..3 {
                    q[c] *= k;
                }
            }
            crate::tone::hdr_toning(px, w, h, p);
            for q in px.iter_mut() {
                for c in 0..3 {
                    q[c] = linear_to_srgb(q[c].clamp(0.0, 1.0));
                }
            }
        }
        ToneMethod::ExposureGamma { exposure, gamma } => {
            let k = 2f32.powf(*exposure);
            let g = gamma.clamp(0.1, 10.0);
            for q in px.iter_mut() {
                for c in 0..3 {
                    q[c] = linear_to_srgb((q[c] * k).clamp(0.0, 1.0)).powf(1.0 / g);
                }
            }
        }
        ToneMethod::HighlightCompression => {
            let k = 0.18 / log_mean.max(1e-6);
            for q in px.iter_mut() {
                let l = luma(*q) * k;
                let ld = l / (1.0 + l);
                let r = if l > 1e-9 { ld / l * k } else { 0.0 };
                for c in 0..3 {
                    q[c] = linear_to_srgb((q[c] * r).clamp(0.0, 1.0));
                }
            }
        }
        ToneMethod::EqualizeHistogram => {
            const BINS: usize = 1024;
            let lv: Vec<f32> = px.iter().map(|q| luma(*q).max(1e-7).ln()).collect();
            let (lo, hi) = lv.iter().zip(px.iter()).filter(|(_, q)| q[3] > 0.0).fold((f32::MAX, f32::MIN), |(a, b), (v, _)| (a.min(*v), b.max(*v)));
            let span = (hi - lo).max(1e-6);
            let mut hist = vec![0f64; BINS];
            for (v, q) in lv.iter().zip(px.iter()) {
                if q[3] > 0.0 {
                    hist[(((v - lo) / span) * (BINS - 1) as f32) as usize] += 1.0;
                }
            }
            let total: f64 = hist.iter().sum::<f64>().max(1.0);
            let mut cdf = vec![0f32; BINS];
            let mut acc = 0.0;
            for b in 0..BINS {
                acc += hist[b];
                cdf[b] = (acc / total) as f32;
            }
            for (v, q) in lv.iter().zip(px.iter_mut()) {
                let target = cdf[(((v - lo) / span).clamp(0.0, 1.0) * (BINS - 1) as f32) as usize];
                // Display value from the CDF (encoded), keep colour ratios in linear light.
                let tl = srgb_to_linear(target);
                let l = luma(*q).max(1e-7);
                for c in 0..3 {
                    q[c] = linear_to_srgb((q[c] / l * tl).clamp(0.0, 1.0));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A radiance map with a wide range, the camera's response a gamma-ish curve.
    fn scene(w: usize, h: usize) -> Vec<[f32; 3]> {
        (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
                let base = 0.002 * 2f32.powf(x * 12.0); // 12 stops left to right
                let tex = 1.0 + 0.3 * ((y * 40.0).sin());
                [base * tex, base * tex * 0.8, base * tex * 0.6]
            })
            .collect()
    }

    fn camera(e: f32) -> f32 {
        // A smooth non-sRGB response.
        (e.max(0.0).powf(0.45)).min(1.0)
    }

    fn shoot(sc: &[[f32; 3]], dt: f32) -> Vec<[f32; 4]> {
        sc.iter().map(|r| [camera(r[0] * dt), camera(r[1] * dt), camera(r[2] * dt), 1.0]).map(|p| p.map(|v| (v * 255.0).round() / 255.0)).collect()
    }

    #[test]
    fn exposure_estimation_and_merge_recover_radiance() {
        let (w, h) = (192usize, 64usize);
        let sc = scene(w, h);
        let dts = [0.25f32, 1.0, 4.0, 16.0];
        let shots: Vec<Vec<[f32; 4]>> = dts.iter().map(|d| shoot(&sc, *d)).collect();
        let refs: Vec<&[[f32; 4]]> = shots.iter().map(Vec::as_slice).collect();
        let est = estimate_exposures(&refs);
        // Ratios between consecutive exposures are 4× (estimated through an sRGB guess, so loose).
        for k in 1..4 {
            let r = est[k] / est[k - 1];
            assert!((2.0..8.0).contains(&r), "ratio {r} ({est:?})");
        }
        let m =
            merge(w, h, &refs, &MergeOptions { exposures: dts.iter().map(|d| *d as f64).collect(), remove_ghosts: false, ghost_base: None, response: None })
                .unwrap();
        // Radiance proportional to the scene: check log-ratios across the 12-stop ramp.
        let probe = |x: usize| m.px[(h / 2) * w + x][1] as f64 / sc[(h / 2) * w + x][1] as f64;
        let k0 = probe(w / 2);
        // Within the bracket's range (brighter pixels clip in every exposure).
        for x in [10, 50, 100, 150, 170] {
            let r = probe(x) / k0;
            assert!((r.ln()).abs() < 0.25, "x {x}: ratio {r}");
        }
        assert!(m.stops > 9.0, "stops {}", m.stops);
        // The recovered response is monotonic.
        assert!(m.response[0].windows(2).all(|p| p[1] > p[0]));
    }

    #[test]
    fn ghosts_come_from_the_base_exposure() {
        let (w, h) = (64usize, 64usize);
        let sc: Vec<[f32; 3]> = vec![[0.2, 0.2, 0.2]; w * h];
        let mut shots: Vec<Vec<[f32; 4]>> = [0.5f32, 1.0, 2.0].iter().map(|d| shoot(&sc, *d)).collect();
        // A "person" only in the third exposure.
        for y in 20..40 {
            for x in 20..40 {
                shots[2][y * w + x] = [0.2, 0.2, 0.2, 1.0];
            }
        }
        let refs: Vec<&[[f32; 4]]> = shots.iter().map(Vec::as_slice).collect();
        let opts = |g| MergeOptions { exposures: vec![0.5, 1.0, 2.0], remove_ghosts: g, ghost_base: Some(1), response: None };
        let plain = merge(w, h, &refs, &opts(false)).unwrap();
        let clean = merge(w, h, &refs, &opts(true)).unwrap();
        let at = |m: &Merged, x: usize, y: usize| m.px[y * w + x][1];
        let bg = at(&clean, 5, 5);
        assert!((at(&clean, 30, 30) / bg - 1.0).abs() < 0.15, "ghost kept: {} vs {}", at(&clean, 30, 30), bg);
        assert!((at(&plain, 30, 30) / at(&plain, 5, 5) - 1.0).abs() > 0.2);
        assert!(clean.ghost_fraction > 0.05 && clean.ghost_base == 1);
    }

    #[test]
    fn mtb_finds_shifts() {
        let (w, h) = (128usize, 96usize);
        let img: Vec<f32> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                0.5 + 0.4 * ((x * 0.21).sin() * (y * 0.17).cos())
            })
            .collect();
        let (dx, dy) = (5i32, -3i32);
        let shifted: Vec<f32> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as i32 - dx, (i / w) as i32 - dy);
                // Darker exposure of the same scene, shifted.
                img[(y.clamp(0, h as i32 - 1) as usize) * w + x.clamp(0, w as i32 - 1) as usize] * 0.6
            })
            .collect();
        let (ox, oy) = mtb_offset(w, h, &shifted, &img, 4);
        assert_eq!((ox, oy), (dx, dy));
    }

    #[test]
    fn tone_mapping_methods_produce_display_range() {
        let (w, h) = (96usize, 32usize);
        let sc = scene(w, h);
        for method in [
            ToneMethod::LocalAdaptation(HdrToning { radius: 7.0, strength: 0.52, ..Default::default() }),
            ToneMethod::ExposureGamma { exposure: 0.0, gamma: 1.0 },
            ToneMethod::HighlightCompression,
            ToneMethod::EqualizeHistogram,
        ] {
            let mut px: Vec<[f32; 4]> = sc.iter().map(|r| [r[0], r[1], r[2], 1.0]).collect();
            tone_map(&mut px, w, h, &method);
            assert!(px.iter().all(|q| q[..3].iter().all(|v| (0.0..=1.0).contains(v))));
            let row = |x: usize| px[(h / 2) * w + x][1];
            assert!(row(w - 2) > row(2), "{method:?}: not increasing");
            if !matches!(method, ToneMethod::ExposureGamma { .. }) {
                // Compression keeps both ends of a 12-stop ramp visible.
                assert!(row(4) > 0.01 && row(w - 4) < 0.999, "{method:?}: {} {}", row(4), row(w - 4));
            }
        }
    }
}

#[cfg(test)]
mod parity {
    use super::*;
    #[test]
    fn rendered_hdr_shared_backend_parity_and_progress() {
        let (w, h) = (12, 8);
        let images: Vec<Vec<[f32; 4]>> = [0.5, 1.0, 2.0]
            .iter()
            .map(|e| {
                (0..w * h)
                    .map(|i| {
                        let l = (0.05 + i as f32 * 0.003) * e;
                        [linear_to_srgb(l), linear_to_srgb(l * 0.8), linear_to_srgb(l * 0.6), 0.7]
                    })
                    .collect()
            })
            .collect();
        let imgs: Vec<_> = images.iter().map(Vec::as_slice).collect();
        let response = std::array::from_fn(|_| (0..256).map(|i| (srgb_to_linear(i as f32 / 255.0).max(1e-5) as f64).ln()).collect());
        let opts = MergeOptions { exposures: vec![0.5, 1.0, 2.0], remove_ghosts: false, ghost_base: Some(1), response: Some(response) };
        let actual = merge(w, h, &imgs, &opts).unwrap();
        let shared = lightcraft_merge::editor::hdr::merge(w, h, &imgs, &opts).unwrap();
        // Goldens captured from an independent reference implementation.
        let golden = [
            [0.04999948, 0.03999894, 0.029998705, 0.7],
            [0.100998975, 0.08079933, 0.060598865, 0.7],
            [0.20900197, 0.16719988, 0.12540069, 0.7],
            [0.33500087, 0.2680009, 0.20100203, 0.7],
        ];
        for (index, expected) in [0, 17, 53, 95].into_iter().zip(golden) {
            for (actual, expected) in actual.px[index].iter().zip(expected) {
                assert!((actual - expected).abs() < 2e-7);
            }
        }
        assert!((actual.stops - 2.6915786076994146).abs() < 1e-6);

        assert_eq!(actual.px, shared.px);
        assert_eq!(actual.response, shared.response);
        assert_eq!(actual.ghost_base, 1);
        assert_eq!(actual.ghost_mask, vec![0.0; w * h]);
        for (i, p) in actual.px.iter().enumerate() {
            assert!((p[1] - (0.05 + i as f32 * 0.003) * 0.8).abs() < 0.0003);
            assert_eq!(p[3], 0.7)
        }
        let cancelled = merge_with_progress(w, h, &imgs, &opts, &|f, _| f < 0.6).unwrap_err();
        assert_eq!(cancelled, "cancelled");
    }
    #[test]
    fn rendered_hdr_rejects_invalid_exposures_and_curves() {
        let image = vec![[0.5, 0.5, 0.5, 1.0]; 4];
        let imgs = [image.as_slice()];
        let mut opts = MergeOptions { exposures: vec![f64::NAN], remove_ghosts: false, ghost_base: None, response: None };
        assert!(merge(2, 2, &imgs, &opts).is_err());
        opts.exposures[0] = 1.0;
        opts.response = Some([vec![], vec![], vec![]]);
        assert!(merge(2, 2, &imgs, &opts).is_err());
    }
}

#[cfg(test)]
mod response_golden {
    use super::*;
    #[test]
    fn rendered_response_matches_legacy_numeric_golden() {
        let response = response_curve(&[vec![12, 30, 80, 100, 180], vec![21, 53, 116, 145, 237]], &[0.0, std::f64::consts::LN_2], 40.0);
        let golden = [-4.224069419829905, -2.9735496780359574, -1.2379416971656003, -3.720555285230523e-15, 0.8528256965735144, 1.5984181879277115];
        for (i, expected) in [0, 16, 64, 128, 192, 255].into_iter().zip(golden) {
            assert!((response[i] - expected).abs() < 1e-8, "response[{i}] {} != {expected}", response[i]);
        }
    }
}

#[cfg(test)]
mod alpha_coverage {
    use super::*;
    #[test]
    fn rendered_hdr_uncovered_warp_pixels_do_not_contaminate_radiance() {
        let (w, h) = (16, 12);
        let reference = vec![[linear_to_srgb(0.2), linear_to_srgb(0.16), linear_to_srgb(0.12), 1.0]; w * h];
        let mut moving = vec![[linear_to_srgb(0.1), linear_to_srgb(0.08), linear_to_srgb(0.06), 1.0]; w * h];
        for (i, p) in moving.iter_mut().enumerate() {
            if i % w < 5 {
                *p = [0.9, 0.8, 0.7, 0.0];
            }
        }
        let response = std::array::from_fn(|_| (0..256).map(|i| (srgb_to_linear(i as f32 / 255.0).max(1e-5) as f64).ln()).collect());
        let opts = MergeOptions { exposures: vec![1.0, 0.5], remove_ghosts: true, ghost_base: Some(0), response: Some(response) };
        let out = merge(w, h, &[reference.as_slice(), moving.as_slice()], &opts).unwrap();
        for (i, p) in out.px.iter().enumerate() {
            assert!((p[0] - 0.2).abs() < 0.00005, "pixel {i}: {p:?}");
            assert_eq!(p[3], 1.0);
        }
        assert!(out.ghost_mask.iter().all(|v| *v == 0.0));
        let mut invalid = moving;
        invalid.fill([0.9, 0.8, 0.7, 0.0]);
        let no_coverage = merge(w, h, &[invalid.as_slice(), invalid.as_slice()], &opts).unwrap();
        assert!(no_coverage.px.iter().all(|p| *p == [0.0; 4]));
    }
}
