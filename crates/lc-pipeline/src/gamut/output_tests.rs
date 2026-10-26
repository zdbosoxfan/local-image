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
                    let rgb = output_lab([l, c * h.cos(), c * h.sin()], &lms_to(&space.from_working()), mul3(&space.from_working(), oklab_to_2020([l, c * h.cos(), c * h.sin()])), space);
                    let lab = oklab_from_2020(mul3(&space.rgb_space().to_space(&li_color::REC2020).to_f32(), rgb));
                    let chroma = lab[1].hypot(lab[2]);
                    assert!(chroma >= previous - 2e-6, "{} at ({l}, {h}, {c}): {chroma} after {previous}", space.label());
                    assert!(in_cube(rgb, OUTPUT_EPS), "{}: {rgb:?}", space.label());
                    previous = chroma;
                }
            }
        }
    }
}

#[test]
fn inward_search_reaches_the_nearest_interval_including_blue_reentry() {
    // A separate f64 evaluation and fine inward scan does not use the production cubic roots.
    for space in OutputSpace::ALL {
        let m = lms_to(&space.from_working());
        for (l, c, h) in [(0.452_013_7f32, 0.34f32, -1.674_494_4f32), (0.45, 0.35, -1.67), (0.63, 0.5, 0.5), (0.85, 0.4, 1.9), (0.3, 0.6, 3.0)] {
            let d = oklab_to_2020([l, c * h.cos(), c * h.sin()]);
            let [l, a, b] = oklab_from_2020(d);
            let chroma = a.hypot(b);
            let dir: [f64; 3] = std::array::from_fn(|i| (mats().from_lab[i][1] * (a / chroma) + mats().from_lab[i][2] * (b / chroma)) as f64);
            let at = |c: f64| -> [f64; 3] { std::array::from_fn(|i| (0..3).map(|j| m[i][j] as f64 * (l as f64 + c * dir[j]).powi(3)).sum()) };
            let eps = OUTPUT_EPS as f64 * (16.0 * (l as f64).powi(3)).min(1.0);
            let inside = |c| at(c).iter().all(|v| *v >= -eps && *v <= 1.0 + eps);
            let step = chroma as f64 / 20_000.0;
            let seed = (0..=20_000).rev().map(|i| i as f64 * step).find(|c| inside(*c)).expect("valid neutral");
            let (mut lo, mut hi) = (seed, (seed + step).min(chroma as f64));
            for _ in 0..50 {
                let mid = (lo + hi) / 2.0;
                if inside(mid) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let rgb = output(d, mul3(&space.from_working(), d), &m, space);
            let lab = oklab_from_2020(mul3(&space.rgb_space().to_space(&li_color::REC2020).to_f32(), rgb));
            let actual = lab[1].hypot(lab[2]);
            assert!((actual as f64 - lo).abs() < 3e-5, "{} at ({l}, {chroma}, {h}): {actual} vs inward reference {lo}", space.label());
        }
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
                    assert!(chroma >= previous - 2e-5, "{} at ({l}, {h}, {c}): {chroma} after {previous}", space.label());
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
