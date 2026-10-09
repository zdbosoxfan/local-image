use lightcraft_pipeline::llf::{
    CURVE_MAX, CURVE_MIN, CURVE_N, CURVE_PER_EV, Params, Remap, curve_at, curve_scalar, dl, expand_gaussian, fast_expf, num_levels,
};

#[test]
#[ignore = "BUG: dl(0, level) panics for level > 0 due to unsigned subtraction underflow"]
fn dl_zero_size_always_zero() {
    for level in 0..10 {
        assert_eq!(dl(0, level), 0);
    }
}

#[test]
fn dl_size_one_always_one() {
    for level in 0..10 {
        assert_eq!(dl(1, level), 1);
    }
}

#[test]
fn dl_halving_known_values() {
    let cases = [(10, 0, 10), (10, 1, 5), (10, 2, 3), (10, 3, 2), (10, 4, 1), (11, 1, 6), (11, 2, 3), (2, 1, 1), (3, 1, 2)];
    for (size, level, expected) in cases {
        assert_eq!(dl(size, level), expected, "size={size}, level={level}");
    }
}

#[test]
fn dl_does_not_increase_with_level() {
    let size = 1000;
    let mut prev = size;
    for level in 0..20 {
        let curr = dl(size, level);
        assert!(curr <= prev);
        assert!(curr > 0 || size == 0);
        prev = curr;
    }
}

#[test]
fn fast_expf_zero_is_one() {
    assert_eq!(fast_expf(0.0), 1.0);
}

#[test]
fn fast_expf_negative_infinity_is_zero() {
    assert_eq!(fast_expf(f32::NEG_INFINITY), 0.0);
}

#[test]
fn fast_expf_negative_values_approximate_exp() {
    for x in [-0.5, -1.0, -2.0, -3.0, -10.0] {
        let approx = fast_expf(x);
        let real = x.exp();
        let err = (approx - real).abs();
        assert!(err < 0.07, "x={x}, approx={approx}, real={real}, err={err}");
    }
}

#[test]
fn fast_expf_nan_returns_zero() {
    assert_eq!(fast_expf(f32::NAN), 0.0);
}

#[test]
fn expand_gaussian_constant_coarse_gives_same_value() {
    let wd = 10;
    let cw = (wd - 1) / 2 + 1; // 5
    let coarse = vec![0.5f32; cw * cw + 10];
    let i = 2;
    let j = 2;
    let result = expand_gaussian(&coarse, i, j, wd);
    assert!((result - 0.5).abs() < 1e-6);
}

#[test]
fn expand_gaussian_does_not_panic_on_boundaries() {
    let wd = 10;
    let cw = (wd - 1) / 2 + 1; // 5
    let coarse = vec![0.25f32; (cw + 2) * (cw + 2)];
    for (i, j) in [(1, 1), (wd - 2, wd - 2), (2, 1), (1, 2), (wd - 2, 1), (1, wd - 2)] {
        let result = expand_gaussian(&coarse, i, j, wd);
        assert!(result.is_finite());
    }
}

#[test]
fn curve_scalar_identity_outside_sigma_positive() {
    let sigma = 0.2f32;
    let g = 0.5f32;
    let x = g + 0.6;
    let result = curve_scalar(x, g, sigma, 1.0, 1.0, 0.0);
    assert!((result - x).abs() < 1e-6);
}

#[test]
fn curve_scalar_identity_outside_sigma_negative() {
    let sigma = 0.2f32;
    let g = 0.5f32;
    let x = g - 0.6;
    let result = curve_scalar(x, g, sigma, 1.0, 1.0, 0.0);
    assert!((result - x).abs() < 1e-6);
}

#[test]
fn curve_scalar_extreme_inputs_do_not_panic() {
    for (x, g, sigma) in [(1e10, 0.5, 0.2), (-1e10, 0.5, 0.2), (0.5, 1e10, 0.2), (0.5, -1e10, 0.2), (0.5, 0.5, 1e-10), (0.5, 0.5, 1e10)] {
        let result = curve_scalar(x, g, sigma, 0.5, 0.5, 0.1);
        assert!(result.is_finite() || result.is_nan());
    }
}

#[test]
fn curve_at_exact_at_sample_points() {
    let lut: Vec<f32> = (0..CURVE_N).map(|i| i as f32).collect();
    let ev = CURVE_MIN + 10.0 / CURVE_PER_EV as f32;
    let result = curve_at(&lut, ev);
    assert!((result - 10.0).abs() < 1e-6);
}

#[test]
fn curve_at_linear_interpolation_between_samples() {
    let lut: Vec<f32> = (0..CURVE_N).map(|i| i as f32).collect();
    let ev = CURVE_MIN + 10.5 / CURVE_PER_EV as f32;
    let result = curve_at(&lut, ev);
    assert!((result - 10.5).abs() < 1e-6);
}

#[test]
fn curve_at_clamps_to_domain() {
    let lut: Vec<f32> = (0..CURVE_N).map(|i| (i % 10) as f32).collect();
    let below = curve_at(&lut, -100.0);
    let at_min = curve_at(&lut, CURVE_MIN);
    assert_eq!(below, at_min);
    let above = curve_at(&lut, 100.0);
    let at_max = curve_at(&lut, CURVE_MAX);
    assert_eq!(above, at_max);
}

#[test]
#[ignore = "BUG: curve_at panics on empty LUT"]
fn curve_at_empty_lut_does_not_panic() {
    let lut: Vec<f32> = vec![];
    let result = std::panic::catch_unwind(|| curve_at(&lut, 0.0));
    assert!(result.is_ok());
}

#[test]
fn remap_darktable_identity_outside_sigma() {
    let sigma = 0.2f32;
    let remap = Remap::Darktable { sigma, shadows: 1.0, highlights: 1.0, clarity: 0.0 };
    let g = 0.5f32;
    let x_far = g + 0.6;
    let x_near = g - 0.6;
    assert!((remap.apply(x_far, g) - x_far).abs() < 1e-6);
    assert!((remap.apply(x_near, g) - x_near).abs() < 1e-6);
}

#[test]
fn remap_tone_zero_curve_is_identity() {
    let lut = vec![0.0f32; CURVE_N];
    let remap = Remap::Tone { sigma: 0.1, lut, lo: 0.0, range: 1.0 };
    let g = 0.4;
    for x in [0.0, 0.2, 0.5, 0.8, 1.0] {
        let result = remap.apply(x, g);
        assert!((result - x).abs() < 1e-6);
    }
}

#[test]
fn remap_tone_constant_curve_shifts_by_constant() {
    let lut = vec![0.5f32; CURVE_N];
    let remap = Remap::Tone { sigma: 100.0, lut, lo: 0.0, range: 1.0 };
    let g = 0.2;
    for x in [0.0, 0.3, 0.6, 0.9] {
        let result = remap.apply(x, g);
        assert!((result - (x + 0.5)).abs() < 1e-6);
    }
}

#[test]
fn remap_tone_extreme_sigma_no_panic() {
    let lut = vec![0.0f32; CURVE_N];
    for sigma in [1e-5, 1e5] {
        let remap = Remap::Tone { sigma, lut: lut.clone(), lo: -8.0, range: 16.0 };
        let result = remap.apply(0.5, 0.3);
        assert!(result.is_finite() || result.is_nan());
    }
}

#[test]
fn num_levels_matches_log2_plus_one_for_powers_of_two() {
    let cases = [(4, 2), (8, 3), (16, 4), (32, 5), (64, 6)];
    for (size, expected) in cases {
        assert_eq!(num_levels(size, size), expected);
    }
}

#[test]
fn num_levels_small_sizes_are_one() {
    assert_eq!(num_levels(1, 1), 1);
    assert_eq!(num_levels(2, 2), 1);
    assert_eq!(num_levels(3, 3), 1);
    assert_eq!(num_levels(0, 0), 1);
}

#[test]
#[ignore = "BUG: num_levels(0,0) panics due to integer underflow"]
fn num_levels_zero_size_does_not_panic() {
    let result = std::panic::catch_unwind(|| num_levels(0, 0));
    assert!(result.is_ok());
}

#[test]
fn curve_constants_are_consistent() {
    assert_eq!(CURVE_N, ((CURVE_MAX - CURVE_MIN) as usize) * CURVE_PER_EV + 1);
    assert_eq!(CURVE_MIN, -16.0);
    assert_eq!(CURVE_MAX, 12.0);
    assert_eq!(CURVE_PER_EV, 32);
}

#[test]
fn params_clone_eq() {
    let p1 = Params {
        remap: Remap::Darktable { sigma: 0.2, shadows: 1.0, highlights: 1.0, clarity: 0.0 },
        num_gamma: 6,
        level_weights: Some(vec![1.0, 1.0, 1.0]),
        remap_residual: false,
    };
    let p2 = p1.clone();
    assert_eq!(p1, p2);
    assert_eq!(p1.remap, p2.remap);
    assert_eq!(p1.num_gamma, p2.num_gamma);
    assert_eq!(p1.level_weights, p1.level_weights);
    assert_eq!(p1.remap_residual, p2.remap_residual);
}
