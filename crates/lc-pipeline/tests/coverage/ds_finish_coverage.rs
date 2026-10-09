use lightcraft_develop::{DevelopSettings, LocalAdjustments};
use lightcraft_pipeline::{
    OutputSpace, SourceInfo,
    finish::{
        DITHER_HASH, FinishParams, GRAIN_HASH, MASK_SUMS, MASK_TERMS, SRGB_LUT_N, defringe_weight, dither8, gamut_map, mask_terms, refine_saturation,
        soft_gamut, srgb_lut,
    },
    geometry::Frame,
};

fn zero_local_adjustments() -> LocalAdjustments {
    LocalAdjustments {
        exposure: 0.0,
        temp: 0.0,
        tint: 0.0,
        contrast: 0.0,
        highlights: 0.0,
        shadows: 0.0,
        whites: 0.0,
        blacks: 0.0,
        texture: 0.0,
        clarity: 0.0,
        dehaze: 0.0,
        saturation: 0.0,
        hue: 0.0,
        sharpness: 0.0,
        noise: 0.0,
        moire: 0.0,
        defringe: 0.0,
        color_sat: 0.0,
        color_hue: 0.0,
        ..Default::default()
    }
}

#[test]
fn constants_positive_and_ordered() {
    assert_eq!(MASK_SUMS, 17);
    assert_eq!(MASK_TERMS, 21);
    assert!(DITHER_HASH.iter().all(|&v| v != 0));
    assert!(GRAIN_HASH.iter().all(|&v| v != 0));
    assert_eq!(SRGB_LUT_N, 4096);
}

#[test]
fn soft_gamut_identity_for_interior_colors() {
    assert_eq!(soft_gamut([0.3, 0.3, 0.3]), [0.3, 0.3, 0.3]);
    assert_eq!(soft_gamut([0.5, 0.2, 0.3]), [0.5, 0.2, 0.3]);
    assert_eq!(soft_gamut([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
}

#[test]
fn soft_gamut_outputs_are_finite_and_do_not_increase_max_channel() {
    let samples = [[1.0, 0.2, 0.8], [1.0, 1.0, 0.0], [1.5, -0.5, 2.0], [0.1, -0.2, 0.3], [10.0, -10.0, 5.0]];
    for c in samples {
        let out = soft_gamut(c);
        assert!(out.iter().all(|v| v.is_finite()), "non-finite for {c:?}: {out:?}");
        let orig_max = c.iter().fold(f32::NEG_INFINITY, |a, b| a.max(*b));
        let out_max = out.iter().fold(f32::NEG_INFINITY, |a, b| a.max(*b));
        assert!(out_max <= orig_max + 1e-6, "max channel increased: {c:?} -> {out:?}");
    }
}

#[test]
fn soft_gamut_is_continuous_near_threshold() {
    let e = 1e-5;
    let a = soft_gamut([1.0, 0.2 - e, 0.8]);
    let b = soft_gamut([1.0, 0.2 + e, 0.8]);
    assert!((a[1] - b[1]).abs() < 3e-5);
}

#[test]
fn soft_gamut_is_monotonic_in_channel_distance() {
    let mut prev = 1.0f32;
    for i in 0..3000 {
        let d = i as f32 / 1000.0;
        let out = soft_gamut([1.0, 1.0 - d, 0.8]);
        assert!(out[1] <= prev + 1e-6, "non-monotonic at d={d}: {} > {}", out[1], prev);
        prev = out[1];
    }
}

#[test]
fn gamut_map_identity_for_in_gamut_colors() {
    let luma = [0.2126, 0.7152, 0.0722];
    let c = [0.5, 0.4, 0.3];
    let (mapped, t) = gamut_map(c, luma);
    assert_eq!(mapped, c);
    assert_eq!(t, 1.0);

    let (mapped, t) = gamut_map([0.0, 0.0, 0.0], luma);
    assert_eq!(mapped, [0.0; 3]);
    assert_eq!(t, 1.0);
}

#[test]
fn gamut_map_maps_out_of_gamut_to_valid_range() {
    let luma = [0.2126, 0.7152, 0.0722];
    let tests = [[1.2, 0.5, 0.3], [-0.2, 0.5, 0.3], [1.5, -0.5, 2.0], [2.0, 2.0, 2.0]];
    for c in tests {
        let (mapped, t) = gamut_map(c, luma);
        assert!(mapped.iter().all(|v| *v >= 0.0 && *v <= 1.0), "out of range for {c:?}: {mapped:?}");
        assert!((0.0..=1.0).contains(&t), "t out of range: {t}");
        assert!(t < 1.0 || c.iter().all(|v| *v >= 0.0 && *v <= 1.0), "t should be < 1.0 for out-of-gamut input {c:?}");
    }
}

#[test]
fn gamut_map_preserves_luma_within_range() {
    let luma = [0.2126, 0.7152, 0.0722];
    let c = [1.3, 0.4, 0.1];
    let (mapped, _) = gamut_map(c, luma);
    let luma_in = luma[0] * c[0] + luma[1] * c[1] + luma[2] * c[2];
    let luma_out = luma[0] * mapped[0] + luma[1] * mapped[1] + luma[2] * mapped[2];
    assert!((luma_in - luma_out).abs() < 1e-5);
}

#[test]
fn gamut_map_handles_non_finite_without_panic() {
    let luma = [0.2126, 0.7152, 0.0722];
    let _ = gamut_map([f32::NAN, 1.0, 2.0], luma);
    let _ = gamut_map([f32::INFINITY, f32::NEG_INFINITY, 0.5], luma);
}

#[test]
fn dither8_endpoints_are_exact() {
    for y in 0..4 {
        for x in 0..4 {
            for ch in 0..3 {
                assert_eq!(dither8(0.0, x, y, ch), 0);
                assert_eq!(dither8(1.0, x, y, ch), 255);
            }
        }
    }
}

#[test]
fn dither8_is_deterministic_and_channel_independent() {
    for y in 0..16 {
        for x in 0..16 {
            let a = dither8(0.501, x, y, 0);
            assert_eq!(a, dither8(0.501, x, y, 1));
            assert_eq!(a, dither8(0.501, x, y, 2));
            assert_eq!(a, dither8(0.501, x, y, 0));
        }
    }
}

#[test]
fn dither8_is_unbiased_on_average() {
    const W: usize = 128;
    const H: usize = 128;
    let mut sum = 0u64;
    for y in 0..H {
        for x in 0..W {
            sum += dither8(0.501, x, y, 0) as u64;
        }
    }
    let avg = sum as f64 / (W * H) as f64;
    assert!((avg - 0.501 * 255.0).abs() < 0.02, "average too far: {avg}");
}

#[test]
fn dither8_handles_extreme_floats() {
    assert_eq!(dither8(-1.0, 0, 0, 0), 0);
    assert_eq!(dither8(2.0, 0, 0, 0), 255);
    let v_nan = dither8(f32::NAN, 0, 0, 0);
    let v_inf = dither8(f32::INFINITY, 0, 0, 0);
    assert!((0..=255).contains(&v_nan));
    assert_eq!(v_inf, 255);
}

#[test]
fn srgb_lut_has_expected_len_and_endpoints() {
    let lut = srgb_lut();
    assert_eq!(lut.len(), SRGB_LUT_N + 1);
    assert_eq!(lut[0], 0.0);
    // `linear_to_srgb(1.0)` may not be exactly 1.0 due to floating-point rounding in the formula
    // (e.g., 0.99999994). This is acceptable for a LUT indexed by integers; the table is used with
    // interpolation and the exact endpoint matters less than the overall accuracy.
    assert!((lut[SRGB_LUT_N] - 1.0).abs() < 1e-6, "last LUT entry should be very close to 1.0, got {}", lut[SRGB_LUT_N]);
}

#[test]
fn srgb_lut_is_monotonic_increasing() {
    let lut = srgb_lut();
    for w in lut.windows(2) {
        assert!(w[0] <= w[1]);
    }
}

#[test]
fn srgb_lut_matches_standard_srgb_on_key_points() {
    let lut = srgb_lut();
    let srgb = |v: f32| -> f32 {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
    };
    for i in [0, 1, 1024, 2048, 3072, 4095, 4096] {
        let expected = srgb(i as f32 / SRGB_LUT_N as f32);
        assert!((lut[i] - expected).abs() < 1e-6, "at {i}: got {}, expected {}", lut[i], expected);
    }
}

#[test]
fn refine_saturation_restores_and_scales_saturation() {
    let before = [0.5, 0.3, 0.2];
    let after = [0.7, 0.35, 0.15];
    let sat = |e: [f32; 3]| (e[0] - e[2]) / (0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2]);

    assert_eq!(refine_saturation(before, after, 1.0), after);

    let r = refine_saturation(before, after, 0.0);
    assert!((sat(r) - sat(before)).abs() < 1e-4);

    let half = refine_saturation(before, after, 0.5);
    assert!(sat(half) > sat(before) && sat(half) < sat(after));
}

#[test]
fn refine_saturation_keeps_luma() {
    let luma = |e: [f32; 3]| 0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2];
    let before = [0.6, 0.4, 0.3];
    let after = [0.8, 0.5, 0.2];
    let r = refine_saturation(before, after, 0.3);
    assert!((luma(r) - luma(after)).abs() < 1e-5);
}

#[test]
fn refine_saturation_returns_after_when_before_is_neutral() {
    let before = [0.5, 0.5, 0.5];
    let after = [0.7, 0.3, 0.2];
    assert_eq!(refine_saturation(before, after, 0.0), after);
    assert_eq!(refine_saturation(before, after, 1.0), after);
}

#[test]
fn defringe_weight_is_non_negative_and_bounded() {
    let colors = [[0.5, 0.4, 0.3], [0.2, 0.5, 0.3], [0.3, 0.2, 0.5], [-0.1, 0.2, 0.3], [1.0, 0.8, 0.7]];
    for det in [-2.0, -0.3, 0.0, 0.2, 2.0] {
        for c in colors {
            let w = defringe_weight(c, det);
            assert!(w.is_finite());
            assert!((0.0..=1.0).contains(&w), "w={w} for {c:?} det={det}");
        }
    }
}

#[test]
fn defringe_weight_is_higher_on_purple_or_green_edges() {
    let neutral = [0.5, 0.5, 0.5];
    let purple = [0.5, 0.1, 0.5];
    let det = 0.5;
    let wn = defringe_weight(neutral, det);
    let wp = defringe_weight(purple, det);
    assert!(wp > wn);
}

#[test]
fn mask_terms_length_and_zero_for_zero_adjustments() {
    let j = zero_local_adjustments();
    let terms = mask_terms(&j);
    assert_eq!(terms.len(), MASK_TERMS);
    assert_eq!(terms[..MASK_SUMS], [0.0; MASK_SUMS]);
    assert_eq!(terms[MASK_SUMS], 0.0);
    assert_eq!(terms[MASK_SUMS + 1], 0.0);
    assert_eq!(terms[MASK_SUMS + 2], 0.0);
    assert_eq!(terms[MASK_SUMS + 3], 0.0);
}

#[test]
fn mask_terms_exposure_temp_tint_contrast_mapping() {
    let mut j = zero_local_adjustments();
    j.exposure = 0.8;
    let t = mask_terms(&j);
    assert_eq!(t[0], 0.8);

    j = zero_local_adjustments();
    j.temp = 100.0;
    let t = mask_terms(&j);
    assert_eq!(t[1], 1.0);

    j = zero_local_adjustments();
    j.tint = 50.0;
    let t = mask_terms(&j);
    assert_eq!(t[2], 0.5);

    j = zero_local_adjustments();
    j.contrast = 30.0;
    let t = mask_terms(&j);
    assert_eq!(t[3], 0.3);
}

#[test]
fn mask_terms_color_overlay_activation_sets_flags_and_amount() {
    let mut j = zero_local_adjustments();
    j.color_sat = 100.0;
    j.color_hue = 0.0;
    let terms = mask_terms(&j);
    assert_eq!(terms[MASK_SUMS], 1.0);
    assert_eq!(terms[MASK_SUMS + 3], (100.0 / 100.0) as f32);
    assert!(terms[MASK_SUMS + 1].is_finite());
    assert!(terms[MASK_SUMS + 2].is_finite());
}

#[test]
fn finish_params_new_populates_dimensions_and_gain() {
    let settings = DevelopSettings::default();
    let info = SourceInfo::default();
    let frame = Frame::new(100, 80, &settings, true);
    let fp = FinishParams::new(&settings, &frame, &info, 50, 40, 0.5, OutputSpace::Srgb);

    assert_eq!(fp.w, 50);
    assert_eq!(fp.h, 40);
    assert_eq!(fp.px_per_long, 0.5);
    assert_eq!(fp.gain, 2f32.powf(settings.light.exposure as f32));
    assert_eq!(fp.ev, settings.light.exposure as f32);
    assert!(fp.to_out.iter().all(|row| row.iter().all(|v| v.is_finite())));
}

#[test]
fn finish_params_with_planes_updates_pixel_scale_fields() {
    let settings = DevelopSettings::default();
    let info = SourceInfo::default();
    let frame = Frame::new(100, 100, &settings, true);
    let fp = FinishParams::new(&settings, &frame, &info, 100, 100, 1.0, OutputSpace::Srgb);

    let fp = fp.with_planes(Some(1.5), Some(([0.2, 0.3, 0.4], 2.0)));
    assert_eq!(fp.sharp_sigma, 1.5);
    assert_eq!(fp.haze_air, [0.2, 0.3, 0.4]);
    assert_eq!(fp.haze_distance, 2.0);
}
