use super::*;
use crate::output::OutputSpace;

#[test]
fn destination_cube_is_identity_including_imaginary_primaries() {
    for space in OutputSpace::ALL {
        let from_out = space.rgb_space().to_space(&li_color::REC2020).to_f32();
        let m = lms_to(&space.from_working());
        let values = [0.0, 0.001, 0.18, 0.5, 0.99, 1.0];
        for r in values {
            for g in values {
                for b in values {
                    let rgb = [r, g, b];
                    let d = mul3(&from_out, rgb);
                    assert_eq!(output(d, rgb, &m, space), rgb, "{}: {rgb:?}", space.label());
                }
            }
        }
    }
    for space in OutputSpace::ALL {
        let rgb = [-5e-7, 0.0, 1e-6];
        let d = mul3(&space.rgb_space().to_space(&li_color::REC2020).to_f32(), rgb);
        assert_eq!(output(d, rgb, &lms_to(&space.from_working()), space), rgb);
    }
    let space = OutputSpace::ProPhoto;
    let rgb = [0.000_867_180_5, 0.209_963_34, 0.979_384_1];
    let d = mul3(&space.rgb_space().to_space(&li_color::REC2020).to_f32(), rgb);
    assert_eq!(output(d, rgb, &lms_to(&space.from_working()), space), rgb);
}

#[test]
fn mixed_negative_blue_is_contained_without_rotating_hue() {
    let d = [-0.010_398_38, -0.008_016_221, 0.403_574_4];
    let space = OutputSpace::ProPhoto;
    let to_out = space.from_working();
    let out = output(d, mul3(&to_out, d), &lms_to(&to_out), space);
    assert!(out.iter().all(|v| (0.0..=1.0).contains(v)), "{out:?}");
    let mapped = mul3(&space.rgb_space().to_space(&li_color::REC2020).to_f32(), out);
    let a = oklab_from_2020(d);
    let b = oklab_from_2020(mapped);
    let error = (a[2].atan2(a[1]) - b[2].atan2(b[1]) + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    assert!(error.abs() < 1e-3, "hue error {error}: {out:?}");
}

fn mapped_lab(space: OutputSpace, lab: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let d = oklab_to_2020(lab);
    let rgb = output(d, mul3(&space.from_working(), d), &lms_to(&space.from_working()), space);
    let working = mul3(&space.rgb_space().to_space(&li_color::REC2020).to_f32(), rgb);
    (oklab_from_2020(working), rgb)
}

#[test]
fn fixed_lab_search_chroma_is_monotone() {
    for space in OutputSpace::ALL {
        for l in [0.02, 0.2, 0.452_013_7, 0.6, 0.9, 0.98] {
            for h in [-2.5f32, -1.674_494_4, -1.67, -0.5, 0.0, 0.5, 1.9, 3.0] {
                let mut previous = 0.0f32;
                for ci in 0..=1000 {
                    let c = ci as f32 * 0.0006;
                    let rgb = output_lab(
                        [l, c * h.cos(), c * h.sin()],
                        &lms_to(&space.from_working()),
                        mul3(&space.from_working(), oklab_to_2020([l, c * h.cos(), c * h.sin()])),
                        space,
                    );
                    let lab = oklab_from_2020(mul3(&space.rgb_space().to_space(&li_color::REC2020).to_f32(), rgb));
                    let chroma = lab[1].hypot(lab[2]);
                    let tolerance = if blue_weight(space, h) > 0.0 { 1e-3 } else { 2e-6 };
                    assert!(chroma >= previous - tolerance, "{} at ({l}, {h}, {c}): {chroma} after {previous}", space.label());
                    assert!(in_cube(rgb, OUTPUT_EPS), "{}: {rgb:?}", space.label());
                    previous = chroma;
                }
            }
        }
    }
}

// Independent f64 channel polynomials from the production f32 matrices.
fn reference_faces(m: &M3, line: Line) -> [[f64; 4]; 6] {
    std::array::from_fn(|i| {
        let channel: [f64; 4] = std::array::from_fn(|k| {
            (0..3)
                .map(|j| {
                    let l = line.l as f64;
                    let d = line.d[j] as f64;
                    m[i / 2][j] as f64 * [l.powi(3), 3.0 * l * l * d, 3.0 * l * d * d, d.powi(3)][k]
                })
                .sum()
        });
        if i % 2 == 0 { channel } else { [1.0 - channel[0], -channel[1], -channel[2], -channel[3]] }
    })
}

fn reference_intervals(faces: &[[f64; 4]], hi: f64) -> Vec<(f64, f64)> {
    let at = |c: &[f64; 4], t: f64| ((c[3] * t + c[2]) * t + c[1]) * t + c[0];
    let mut cuts = vec![0.0, hi];
    for c in faces {
        let (a, b, d) = (3.0 * c[3], 2.0 * c[2], c[1]);
        let disc = b * b - 4.0 * a * d;
        let mut turns = vec![0.0, hi];
        if disc >= 0.0 {
            if a != 0.0 {
                for r in [(-b - disc.sqrt()) / (2.0 * a), (-b + disc.sqrt()) / (2.0 * a)] {
                    if r > 0.0 && r < hi {
                        turns.push(r);
                    }
                }
            } else if b != 0.0 {
                let r = -d / b;
                if r > 0.0 && r < hi {
                    turns.push(r);
                }
            }
        }
        turns.sort_by(f64::total_cmp);
        for pair in turns.windows(2) {
            let (mut lo, mut up) = (pair[0], pair[1]);
            let negative = at(c, lo) < 0.0;
            if negative == (at(c, up) < 0.0) {
                continue;
            }
            for _ in 0..48 {
                let mid = 0.5 * (lo + up);
                if (at(c, mid) < 0.0) == negative {
                    lo = mid;
                } else {
                    up = mid;
                }
            }
            cuts.push(0.5 * (lo + up));
        }
    }
    cuts.sort_by(f64::total_cmp);
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    for pair in cuts.windows(2) {
        let mid = 0.5 * (pair[0] + pair[1]);
        if faces.iter().all(|c| at(c, mid) >= 0.0) {
            if let Some(last) = intervals.last_mut().filter(|last| last.1 == pair[0]) {
                last.1 = pair[1];
            } else {
                intervals.push((pair[0], pair[1]));
            }
        }
    }
    intervals
}

#[test]
fn exact_clip_crosses_once_outside_the_windows() {
    let working = lms_to(&OutputSpace::Rec2020.from_working());
    for space in OutputSpace::ALL {
        let m = lms_to(&space.from_working());
        for l in [0.02, 0.2, 0.45, 0.8, 0.98] {
            for i in 0..720 {
                let h = -std::f32::consts::PI + i as f32 * std::f32::consts::TAU / 720.0;
                let line = Line::new([l, h.cos(), h.sin()]).unwrap();
                let unit = Line { l: 1.0, ..line };
                let lower: Vec<_> = reference_faces(&working, unit).into_iter().step_by(2).collect();
                let reach = reference_intervals(&lower, 2.0).last().unwrap().1;
                let c = (reach * l as f64) as f32;
                let mut faces = reference_faces(&m, line);
                for face in &mut faces {
                    face[0] += OUTPUT_EPS as f64;
                }
                let intervals = reference_intervals(&faces, c as f64);
                let in_window = match space {
                    OutputSpace::Srgb => (BLUE_SRGB[0]..=BLUE_SRGB[1]).contains(&h),
                    OutputSpace::Rec2020 => (BLUE_REC2020[0]..=BLUE_REC2020[1]).contains(&h),
                    _ => false,
                };
                if !in_window {
                    assert!(intervals.len() <= 1, "{space:?} L={l} h={h}: {intervals:?}");
                }
                let expected = intervals.first().unwrap().1;
                let actual = line.output_chroma(&m, c).unwrap();
                assert!((actual as f64 - expected).abs() < 3e-5, "{space:?} L={l} h={h}: {actual} vs {expected}");
            }
        }
    }
}

#[test]
fn pinch_windows_cover_every_pinched_hue() {
    let working = lms_to(&OutputSpace::Rec2020.from_working());
    for space in OutputSpace::ALL {
        let from_out = space.rgb_space().to_space(&li_color::REC2020).to_f32();
        let m = lms_to(&space.from_working());
        let mut pinched = 0;
        for vertex in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 0.0, 1.0]] {
            let lab = oklab_from_2020(mul3(&from_out, vertex));
            let center = lab[2].atan2(lab[1]);
            for i in -20_000..=20_000 {
                let h = (center + i as f32 * 1e-6 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                let line = Line::new([1.0, h.cos(), h.sin()]).unwrap();
                let lower: Vec<_> = reference_faces(&m, line).into_iter().step_by(2).collect();
                let intervals = reference_intervals(&lower, 2.0);
                if intervals.len() < 2 {
                    continue;
                }
                pinched += 1;
                match space {
                    OutputSpace::Srgb => assert!((BLUE_SRGB[0]..=BLUE_SRGB[1]).contains(&h), "sRGB h={h}: {intervals:?}"),
                    OutputSpace::Rec2020 => assert!((BLUE_REC2020[0]..=BLUE_REC2020[1]).contains(&h), "Rec2020 h={h}: {intervals:?}"),
                    OutputSpace::DisplayP3 | OutputSpace::AdobeRgb => panic!("unexpected pinch {space:?} h={h}: {intervals:?}"),
                    OutputSpace::ProPhoto => {
                        let lower: Vec<_> = reference_faces(&working, line).into_iter().step_by(2).collect();
                        let rec_reach = reference_intervals(&lower, 2.0).last().unwrap().1;
                        assert!(intervals[1].0 - rec_reach >= 0.5, "ProPhoto h={h}: {intervals:?} vs Rec2020 reach {rec_reach}");
                    }
                }
            }
        }
        if matches!(space, OutputSpace::Srgb | OutputSpace::Rec2020 | OutputSpace::ProPhoto) {
            assert!(pinched > 0, "no pinches found in {space:?}");
        }
    }
}

fn encoded_output(space: OutputSpace, d: [f32; 3]) -> [f32; 3] {
    let to_out = space.from_working();
    output(d, mul3(&to_out, d), &lms_to(&to_out), space).map(|v| 255.0 * space.trc().encode(v.clamp(0.0, 1.0)))
}

#[test]
fn small_nudges_move_the_output_little() {
    for space in OutputSpace::ALL {
        let mut checked = 0;
        for [lo, hi] in [BLUE_SRGB, BLUE_REC2020] {
            for offset in [-0.101, -0.1, -0.099, -0.05, -0.011, -0.01, -0.009, 0.0, 0.009, 0.01, 0.011, 0.05, 0.099, 0.1, 0.101] {
                for h in [lo + offset, hi + offset] {
                    for l in [0.2, 0.3, 0.45, 0.6, 0.8] {
                        for c in [0.05, 0.1, 0.2, 0.28, 0.31, 0.34, 0.38, 0.45] {
                            let d = oklab_to_2020([l, c * h.cos(), c * h.sin()]);
                            let max = d.into_iter().fold(0.0f32, f32::max);
                            if max > 1.0 || d.iter().any(|v| *v < -0.02 * max) {
                                continue;
                            }
                            checked += 1;
                            let a = encoded_output(space, d);
                            for sign in [[1.0, 1.0, 1.0], [1.0, -1.0, -1.0], [-1.0, 1.0, -1.0], [-1.0, -1.0, 1.0]] {
                                for (relative, step) in [(true, 6e-5 * max), (false, 5e-5)] {
                                    let b = encoded_output(space, std::array::from_fn(|i| d[i] + step * sign[i]));
                                    let limit = match (space, relative) {
                                        (OutputSpace::DisplayP3, _) => 2.44,
                                        (OutputSpace::AdobeRgb, _) => 4.16,
                                        (_, true) => 2.5,
                                        (_, false) => 3.0,
                                    };
                                    for i in 0..3 {
                                        assert!(
                                            (a[i] - b[i]).abs() <= limit,
                                            "{space:?} L={l} C={c} h={h} d={d:?} relative={relative}: {a:?} vs {b:?}, limit {limit}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 100, "{space:?}: insufficient physical samples");
    }
}

#[test]
fn hue_sweeps_through_the_blue_pinch_are_continuous() {
    for space in [OutputSpace::Srgb, OutputSpace::Rec2020] {
        let mut space_checked = 0;
        let [lo, hi] = if space == OutputSpace::Srgb { BLUE_SRGB } else { BLUE_REC2020 };
        for l in [0.3, 0.45, 0.6] {
            for c in [0.1, 0.2, 0.28, 0.31, 0.34, 0.38] {
                let mut previous = None;
                let mut checked = 0;
                for i in 0..=24_000 {
                    let h = 0.5 * (lo + hi) - 0.12 + i as f32 * 1e-5;
                    let d = oklab_to_2020([l, c * h.cos(), c * h.sin()]);
                    let max = d.into_iter().fold(0.0f32, f32::max);
                    if max > 1.0 || d.iter().any(|v| *v < -0.02 * max) {
                        previous = None;
                        continue;
                    }
                    let rgb = encoded_output(space, d);
                    if let Some(last) = previous {
                        let last: [f32; 3] = last;
                        for k in 0..3 {
                            assert!((rgb[k] - last[k]).abs() <= 0.2, "{space:?} L={l} C={c} h={h}: {last:?} vs {rgb:?}");
                        }
                        checked += 1;
                    }
                    previous = Some(rgb);
                    space_checked += 1;
                }
                // Some high-chroma, low-L rays lie wholly outside the physical input cone.
                if previous.is_some() {
                    assert!(checked > 100);
                }
            }
        }
        assert!(space_checked > 100, "no physical hue sweep in {space:?}");
    }
}

#[test]
fn negative_black_and_hdr_outputs_are_finite_and_contained() {
    for space in OutputSpace::ALL {
        let to_out = space.from_working();
        let m = lms_to(&to_out);
        for rgb in [[0.0; 3], [-1.0, -2.0, -3.0], [-0.2, 0.5, 0.1], [0.0, 0.0, 1.0], [1.0; 3], [1.0, -1.0, 0.1]] {
            for exposure in [1e-6, 0.01, 1.0, 1e3, 1e6, 1e30] {
                let d = rgb.map(|v| v * exposure);
                let out = output(d, mul3(&to_out, d), &m, space).map(|v| v.clamp(0.0, 1.0));
                assert!(out.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{}: {d:?} -> {out:?}", space.label());
            }
        }
        assert_eq!(output([-1.0; 3], mul3(&to_out, [-1.0; 3]), &m, space), [0.0; 3]);
        assert_eq!(output([0.0; 3], [0.0; 3], &m, space), [0.0; 3]);
    }
}

#[test]
fn out_of_range_lightness_reaches_the_hue_endpoint() {
    for space in OutputSpace::ALL {
        for h in [-2.5f32, -1.67, 0.5, 1.9, 3.0] {
            let (mapped, rgb) = mapped_lab(space, [2.0, 0.4 * h.cos(), 0.4 * h.sin()]);
            assert!(in_cube(rgb, OUTPUT_EPS));
            // Any small extra lightness at the mapped hue must be outside the destination cube.
            let lifted = oklab_to_2020([mapped[0] + 2e-5, mapped[1], mapped[2]]);
            assert!(!in_cube(mul3(&space.from_working(), lifted), 0.0), "{}: {rgb:?} stopped before the endpoint", space.label());
            assert!(mapped[0] > 0.99 && mapped[0] < 1.001, "{}: {mapped:?}", space.label());
        }
    }
}

#[test]
fn white_endpoint_matches_the_independent_upper_envelope_reference() {
    let d = [1.012_258_6, 1.038_184_2, 0.848_988_65];
    for (space, expected) in [
        (OutputSpace::Srgb, [1.0, 1.0, 0.996_406_0]),
        (OutputSpace::DisplayP3, [1.0, 1.0, 0.996_733_6]),
        (OutputSpace::AdobeRgb, [1.0, 0.999_664_4, 0.998_941_06]),
        (OutputSpace::ProPhoto, [1.0, 1.0, 0.998_805_05]),
        (OutputSpace::Rec2020, [1.0, 1.0, 0.997_633_76]),
    ] {
        let to_out = space.from_working();
        let out = output(d, mul3(&to_out, d), &lms_to(&to_out), space);
        for i in 0..3 {
            assert!((out[i] - expected[i]).abs() < 2e-5, "{}: {out:?} vs {expected:?}", space.label());
        }
    }
}

#[test]
fn near_neutral_highlights_reduce_lightness_without_increasing_chroma() {
    for space in OutputSpace::ALL {
        for h in [-2.5f32, 1.9898, 3.0] {
            for c in [1e-6, 1e-5, 1e-4, 5e-4] {
                let (lab, rgb) = mapped_lab(space, [1.01, c * h.cos(), c * h.sin()]);
                assert!(rgb.iter().all(|v| (0.0..=1.0).contains(v)), "{}: {rgb:?}", space.label());
                assert!(lab[0] < 1.001 && lab[0] > 0.99, "{}: {lab:?}", space.label());
                assert!(lab[1].hypot(lab[2]) <= c + 2e-6, "{}: {lab:?} exceeds input chroma {c}", space.label());
            }
        }
    }
}

#[test]
fn public_output_chroma_is_monotone_on_well_conditioned_rays() {
    for space in OutputSpace::ALL {
        for l in [0.2, 0.45, 0.6, 0.9] {
            for h in [-2.5f32, -1.67, -0.5, 0.0, 0.5, 1.9, 3.0] {
                let mut previous = 0.0f32;
                let mut checked = 0;
                for ci in 0..=1000 {
                    let c = ci as f32 * 0.0006;
                    let input = [l, c * h.cos(), c * h.sin()];
                    let d = oklab_to_2020(input);
                    let actual = oklab_from_2020(d);
                    // Signed cube roots near zero LMS amplify RGB round-off; those generated
                    // inputs no longer share the requested lightness/hue.
                    if (0..3).any(|i| (actual[i] - input[i]).abs() > 2e-6) {
                        continue;
                    }
                    checked += 1;
                    let (lab, _) = mapped_lab(space, input);
                    let chroma = lab[1].hypot(lab[2]);
                    let tolerance = if blue_weight(space, h) > 0.0 { 1e-3 } else { 2e-5 };
                    assert!(chroma >= previous - tolerance, "{} at ({l}, {h}, {c}): {chroma} after {previous}", space.label());
                    previous = chroma;
                }
                assert!(checked > 100, "insufficient well-conditioned samples at ({l}, {h})");
            }
        }
    }
}

#[test]
fn near_black_output_chroma_is_monotone_across_membership_boundary() {
    let space = OutputSpace::Srgb;
    let to_out = space.from_working();
    let from_out = space.rgb_space().to_space(&li_color::REC2020).to_f32();
    let m = lms_to(&to_out);
    let hue = 52.0 * std::f32::consts::TAU / 72.0;
    let mut previous = [0.0; 2];
    for ci in [408, 409] {
        let chroma = ci as f32 * 0.000_025;
        let input = oklab_to_2020([0.02, chroma * hue.cos(), chroma * hue.sin()]);
        let actual = oklab_from_2020(input);
        let hue_error = (actual[2].atan2(actual[1]) - hue + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        assert!((actual[0] - 0.02).abs() < 2e-8 && hue_error.abs() < 2e-6, "ill-conditioned input {actual:?}");
        let mapped = output(input, mul3(&to_out, input), &m, space);
        for (i, rgb) in [mapped, mapped.map(|v| v.clamp(0.0, 1.0))].into_iter().enumerate() {
            let lab = oklab_from_2020(mul3(&from_out, rgb));
            let chroma = lab[1].hypot(lab[2]);
            assert!(
                chroma >= previous[i] - 2e-6,
                "path {i}: output chroma {chroma} after {} at input chroma {}",
                previous[i],
                actual[1].hypot(actual[2])
            );
            previous[i] = chroma;
        }
    }
}

#[test]
fn captured_blue_pair_maps_continuously() {
    let space = OutputSpace::Srgb;
    let to_out = space.from_working();
    let m = lms_to(&to_out);
    let cpu = [0.04546731, 0.021514542, 0.86354965];
    let gpu = [0.04551213, 0.021517217, 0.8635499];
    let inside = mul3(&to_out, gpu);
    assert_eq!(output(gpu, inside, &m, space), inside);
    let encoded = |d| output(d, mul3(&to_out, d), &m, space).map(|v| space.trc().encode(v.clamp(0.0, 1.0)) * 255.0);
    let a = encoded(cpu);
    let b = encoded(gpu);
    for i in 0..3 {
        assert!((a[i] - b[i]).abs() <= 1.0, "channel {i}: {a:?} vs {b:?}");
    }
}
