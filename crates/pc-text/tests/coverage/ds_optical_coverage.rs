use photocraft_text::optical::{Cache, Profile, SAMPLES, kern_from_features, pair_features, sample_y, size_adjust};

fn assert_approx_eq(a: f32, b: f32, eps: f32) {
    assert!((a - b).abs() < eps, "assertion failed: {a} !~= {b} (epsilon {eps})");
}

#[test]
fn sample_y_is_monotonic_and_within_expected_range() {
    assert_eq!(SAMPLES, 48);
    for i in 0..SAMPLES - 1 {
        let y = sample_y(i);
        let y_next = sample_y(i + 1);
        assert!(y < y_next, "sample_y not increasing at index {i}");
        assert!((-0.25..=1.0).contains(&y), "sample_y({i}) out of range: {y}");
    }
    assert!(sample_y(SAMPLES - 1) <= 1.0);
}

#[test]
fn sample_y_follows_linear_formula() {
    let expected0 = -0.25 + 1.25 * (0.5 / SAMPLES as f32);
    assert_approx_eq(sample_y(0), expected0, 1e-6);

    let expected_last = -0.25 + 1.25 * ((SAMPLES as f32 - 0.5) / SAMPLES as f32);
    assert_approx_eq(sample_y(SAMPLES - 1), expected_last, 1e-6);
}

#[test]
fn size_adjust_returns_zero_for_non_positive_and_non_finite() {
    let values = [0.0f32, -1.0, f32::NEG_INFINITY, f32::INFINITY, f32::NAN];
    for &v in &values {
        let adjust = size_adjust(v);
        assert!(!adjust.is_nan(), "size_adjust({v:?}) returned NaN");
        assert_eq!(adjust, 0.0, "size_adjust({v:?}) not zero");
    }
}

#[test]
fn size_adjust_is_monotonic_decreasing_between_15_and_95() {
    let mut prev = size_adjust(15.0);
    assert!(prev > 0.0);
    for size in 16..=95 {
        let cur = size_adjust(size as f32);
        assert!(cur < prev, "size_adjust not decreasing at {size}");
        prev = cur;
    }
    assert_eq!(size_adjust(95.0), 0.0);
}

#[test]
fn size_adjust_clamps_below_15_and_above_95() {
    let at_15 = size_adjust(15.0);
    assert_approx_eq(size_adjust(10.0), at_15, 1e-6);
    assert_approx_eq(size_adjust(0.1), at_15, 1e-6);
    assert_eq!(size_adjust(95.0), 0.0);
    assert_eq!(size_adjust(120.0), 0.0);
}

#[test]
fn kern_from_features_clamps_to_limit() {
    // f much looser than reference -> strong negative kern, clamped to -400
    assert_eq!(kern_from_features((10.0, 0.0), (0.0, 0.0)), -400.0);
    // f much tighter -> strong positive kern, clamped to 400
    assert_eq!(kern_from_features((0.0, 0.0), (10.0, 0.0)), 400.0);
}

#[test]
fn kern_from_features_handles_non_finite() {
    assert_eq!(kern_from_features((f32::NAN, 0.0), (0.0, 0.0)), 0.0);
    assert_eq!(kern_from_features((f32::INFINITY, 0.0), (0.0, 0.0)), 0.0);
    assert_eq!(kern_from_features((0.0, f32::NAN), (0.0, 0.0)), 0.0);
    assert_eq!(kern_from_features((0.0, 0.0), (f32::INFINITY, 0.0)), 0.0);
    assert_eq!(kern_from_features((0.0, 0.0), (0.0, f32::NAN)), 0.0);
}

#[test]
fn kern_from_features_reference_identity_gives_constant_offset() {
    let f = (0.4, 0.1);
    let k = kern_from_features(f, f);
    assert_approx_eq(k, -3.3, 0.001); // C0 * 1000
}

#[test]
fn kern_from_features_direction() {
    // f has larger min gap (looser) -> negative kern (tighter)
    let k1 = kern_from_features((0.5, 0.0), (0.3, 0.0));
    assert!(k1 < 0.0);
    // f has smaller min gap (tighter) -> positive kern
    let k2 = kern_from_features((0.3, 0.0), (0.5, 0.0));
    assert!(k2 > 0.0);
}

#[test]
fn kern_from_features_pin_exact_formula() {
    // Manually computed from the published coefficients
    let f = (0.4, 0.1);
    let reference = (0.3, 0.2);
    let k = kern_from_features(f, reference);
    assert_approx_eq(k, -1.88, 0.001);
}

#[test]
fn pair_features_same_profile_constant_gap() {
    let p = Profile { left: std::array::from_fn(|_| Some(0.5f32)), right: std::array::from_fn(|_| Some(0.5f32)), advance: 1.0 };
    let features = pair_features(&p, &p).expect("expected features");
    assert_approx_eq(features.0, 1.0, 1e-6);
    assert_approx_eq(features.1, 0.0, 1e-6);
}

#[test]
fn pair_features_empty_profile_returns_none() {
    let empty = Profile { left: [None; SAMPLES], right: [None; SAMPLES], advance: 0.0 };
    assert!(pair_features(&empty, &empty).is_none());

    let with_ink = Profile { left: std::array::from_fn(|_| Some(0.5f32)), right: std::array::from_fn(|_| Some(0.5f32)), advance: 1.0 };
    assert!(pair_features(&empty, &with_ink).is_none());
    assert!(pair_features(&with_ink, &empty).is_none());
}

#[test]
fn pair_features_ignores_samples_outside_band() {
    // Indices 0..=5 have sample_y < -0.1 (outside band -0.1..1.0).
    let mut left = [None; SAMPLES];
    let mut right = [None; SAMPLES];
    for i in 0..=5 {
        left[i] = Some(0.5);
        right[i] = Some(0.5);
    }
    let p = Profile { left, right, advance: 1.0 };
    assert!(pair_features(&p, &p).is_none());
}

#[test]
fn pair_features_computes_min_and_capped_excess() {
    // Constant gap 0.3 at all in-band samples.
    let mut l_right = std::array::from_fn(|_| Some(0.1f32));
    let r_left = std::array::from_fn(|_| Some(0.2f32));
    // Modify index 10 (in band) to have gap 0.4 (extra 0.1 > DEPTH so capped to 0.04)
    l_right[10] = Some(0.2);

    let l = Profile { left: [None; SAMPLES], right: l_right, advance: 1.0 };
    let r = Profile { left: r_left, right: [None; SAMPLES], advance: 1.0 };
    let features = pair_features(&l, &r).expect("expected features");

    // min gap = 0.3
    assert_approx_eq(features.0, 0.3, 1e-6);
    // Excess: only one sample had extra gap, capped at DEPTH=0.04.
    // Number of in-band samples: 48 total minus 6 below -0.1 = 42.
    let expected_excess = 0.04 / 42.0;
    assert_approx_eq(features.1, expected_excess, 1e-6);
}

#[test]
fn profile_is_empty_checks_left_side_only() {
    let p_empty_left = Profile { left: [None; SAMPLES], right: std::array::from_fn(|_| Some(0.5f32)), advance: 1.0 };
    assert!(p_empty_left.is_empty());

    let mut left_with_ink = [None; SAMPLES];
    left_with_ink[0] = Some(0.1);
    let p_with_ink = Profile { left: left_with_ink, right: [None; SAMPLES], advance: 1.0 };
    assert!(!p_with_ink.is_empty());
}

#[test]
fn cache_pair_with_invalid_font_data_returns_none() {
    let mut cache = Cache::default();
    assert_eq!(cache.pair(0, &[], 0, &[], 0, 0), None);

    let junk = [0u8; 16];
    assert_eq!(cache.pair(0, &junk, 0, &[], 0, 0), None);

    let junk2 = [0xFFu8; 64];
    assert_eq!(cache.pair(1, &junk2, 0, &[], 0, 0), None);
}

#[test]
fn cache_pair_is_deterministic_for_invalid_input() {
    let mut cache = Cache::default();
    let junk = [0u8; 32];
    let r1 = cache.pair(0, &junk, 0, &[], 0, 0);
    let r2 = cache.pair(0, &junk, 0, &[], 0, 0);
    assert_eq!(r1, r2);
    assert_eq!(r1, None);
}

#[test]
fn cache_pair_does_not_panic_on_many_bad_inputs() {
    let mut cache = Cache::default();
    for i in 0..10 {
        let data = vec![i as u8; i * 3];
        let _ = cache.pair(i as u64, &data, i as u32, &[], i as u32, (i + 1) as u32);
    }
}
