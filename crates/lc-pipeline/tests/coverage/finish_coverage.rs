use lightcraft_develop::{DevelopSettings, LocalAdjustments};
use lightcraft_pipeline::{
    OutputSpace, SourceInfo,
    finish::{FinishParams, MASK_SUMS, MASK_TERMS, SRGB_LUT_N, defringe_weight, dither8, gamut_map, mask_terms, refine_saturation, srgb_lut},
    gamut,
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
fn output_knee_outputs_are_finite_and_do_not_increase_max_channel() {
    let samples = [[1.0, 0.2, 0.8], [1.0, 1.0, 0.0], [1.5, -0.5, 2.0], [0.1, -0.2, 0.3], [10.0, -10.0, 5.0]];
    let to_out = OutputSpace::Srgb.from_working();
    let from_out = OutputSpace::Srgb.rgb_space().to_space(&li_color::REC2020).to_f32();
    for c in samples {
        let d: [f32; 3] = std::array::from_fn(|i| (0..3).map(|j| from_out[i][j] * c[j]).sum());
        let out = gamut::output(d, c, &gamut::lms_to(&to_out));
        assert!(out.iter().all(|v| v.is_finite()), "non-finite for {c:?}: {out:?}");
        let orig_max = c.iter().fold(f32::NEG_INFINITY, |a, b| a.max(*b));
        let out_max = out.iter().fold(f32::NEG_INFINITY, |a, b| a.max(*b));
        assert!(out_max <= orig_max + 1e-6, "max channel increased: {c:?} -> {out:?}");
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
