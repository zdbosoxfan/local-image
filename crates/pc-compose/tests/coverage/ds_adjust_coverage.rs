use photocraft_compose::adjust::{
    Transfer, bayer4, black_white_gray, hsl_to_rgb, hue_saturation, lut3d_sample, modern_brightness, modern_contrast, photo_filter_matrix, posterize,
    rgb_to_hsl, selective_color, selective_color_weights,
};

fn identity_lut(n: usize) -> Vec<f32> {
    let mut v = vec![0.0; n * n * n * 3];
    let m = (n - 1) as f32;
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let idx = ((b * n + g) * n + r) * 3;
                v[idx] = r as f32 / m;
                v[idx + 1] = g as f32 / m;
                v[idx + 2] = b as f32 / m;
            }
        }
    }
    v
}

#[test]
fn transfer_exposure_curve_for_srgb_is_gamma_22() {
    assert_eq!(Transfer::Srgb.for_exposure(), Transfer::Gamma(2.2));
    assert_eq!(Transfer::Gamma(1.0).for_exposure(), Transfer::Gamma(1.0));
    assert_eq!(Transfer::Gamma(1.732).for_exposure(), Transfer::Gamma(1.732));
}

#[test]
fn bayer4_values_are_finite_and_in_range() {
    for y in -20..20 {
        for x in -20..20 {
            let d = bayer4(x, y);
            assert!(d.is_finite(), "non-finite at ({x},{y})");
            assert!(d.abs() <= 0.5, "out of range at ({x},{y}): {d}");
        }
    }
}

#[test]
fn bayer4_is_periodic_with_period_4() {
    assert_eq!(bayer4(-4, -4), bayer4(0, 0));
    assert_eq!(bayer4(-1, -1), bayer4(3, 3));
    assert_eq!(bayer4(8, 9), bayer4(0, 1));
}

#[test]
fn bayer4_offsets_sum_to_zero() {
    let sum: f32 = (0..4).flat_map(|y| (0..4).map(move |x| bayer4(x, y))).sum();
    assert!(sum.abs() < 1e-6, "sum was {sum}");
}

#[test]
fn posterize_levels_2_produces_black_and_white() {
    assert_eq!(posterize(0.4, 2), 0.0);
    assert_eq!(posterize(0.6, 2), 1.0);
    // boundary: 128/255 is the threshold
    assert_eq!(posterize(127.0 / 255.0, 2), 0.0);
    assert_eq!(posterize(129.0 / 255.0, 2), 1.0);
}

#[test]
fn posterize_clamps_levels_to_2() {
    assert_eq!(posterize(0.6, 0), posterize(0.6, 2));
    assert_eq!(posterize(0.6, 1), posterize(0.6, 2));
}

#[test]
fn posterize_levels_3_uses_expected_output_levels() {
    // Output levels for 3 posterize are 0, 127/255, 1.0 (see Photoshop exact behavior).
    assert_eq!(posterize(0.0, 3), 0.0);
    let mid = posterize(0.5, 3);
    assert!((mid - 127.0 / 255.0).abs() < 1e-7, "expected 127/255, got {mid}");
    assert!((posterize(1.0, 3) - 1.0).abs() < 1e-7);
}

#[test]
fn modern_brightness_zero_is_identity() {
    for v in [0.0, 0.25, 0.5, 0.75, 1.0] {
        assert!((modern_brightness(v, 0.0) - v).abs() < 1e-6);
    }
}

#[test]
fn modern_brightness_pins_endpoints() {
    assert_eq!(modern_brightness(0.0, 100.0), 0.0);
    assert_eq!(modern_brightness(1.0, 100.0), 1.0);
    assert_eq!(modern_brightness(0.0, -100.0), 0.0);
    assert_eq!(modern_brightness(1.0, -100.0), 1.0);
}

#[test]
fn modern_brightness_positive_lifts_midtones() {
    assert!(modern_brightness(0.5, 50.0) > 0.5);
    assert!(modern_brightness(0.5, -50.0) < 0.5);
}

#[test]
fn modern_contrast_zero_is_identity() {
    for v in [0.0, 0.25, 0.5, 0.75, 1.0] {
        assert!((modern_contrast(v, 0.0) - v).abs() < 1e-6);
    }
}

#[test]
fn modern_contrast_pins_endpoints_and_midpoint() {
    assert_eq!(modern_contrast(0.0, 50.0), 0.0);
    assert_eq!(modern_contrast(1.0, 50.0), 1.0);
    assert_eq!(modern_contrast(0.5, 50.0), 0.5);
}

#[test]
fn modern_contrast_increases_difference() {
    assert!(modern_contrast(0.25, 50.0) < 0.25);
    assert!(modern_contrast(0.75, 50.0) > 0.75);
}

#[test]
fn rgb_to_hsl_known_values() {
    let (h, s, l) = rgb_to_hsl([1.0, 0.0, 0.0]);
    assert!((h - 0.0).abs() < 1e-6);
    assert!((s - 1.0).abs() < 1e-6);
    assert!((l - 0.5).abs() < 1e-6);
}

#[test]
fn hsl_to_rgb_known_values() {
    let red = hsl_to_rgb(0.0, 1.0, 0.5);
    assert!((red[0] - 1.0).abs() < 1e-6 && (red[1]).abs() < 1e-6 && (red[2]).abs() < 1e-6);
    let green = hsl_to_rgb(1.0 / 3.0, 1.0, 0.5);
    assert!((green[1] - 1.0).abs() < 1e-6 && (green[0]).abs() < 1e-6 && (green[2]).abs() < 1e-6);
    let blue = hsl_to_rgb(2.0 / 3.0, 1.0, 0.5);
    assert!((blue[2] - 1.0).abs() < 1e-6 && (blue[0]).abs() < 1e-6 && (blue[1]).abs() < 1e-6);
}

#[test]
fn rgb_hsl_roundtrip() {
    let colors = [[0.2, 0.4, 0.8], [0.9, 0.1, 0.3], [0.7, 0.7, 0.2], [0.1, 0.5, 0.1], [0.11, 0.22, 0.33]];
    for c in colors {
        let (h, s, l) = rgb_to_hsl(c);
        let out = hsl_to_rgb(h, s, l);
        for i in 0..3 {
            assert!((out[i] - c[i]).abs() < 1e-5, "roundtrip failed {c:?} -> {out:?}");
        }
    }
}

#[test]
fn hue_saturation_identity_on_neutral() {
    let c = [0.5, 0.5, 0.5, 1.0];
    // hue shift, saturation and lightness zero, colorize false
    let out = hue_saturation([c[0], c[1], c[2]], 90.0, 0.0, 0.0, false);
    for i in 0..3 {
        assert!((out[i] - 0.5).abs() < 1e-6);
    }
}

#[test]
fn hue_saturation_colorize_sets_hue_and_saturation() {
    let gray = [0.5, 0.5, 0.5];
    let red = hue_saturation(gray, 0.0, 1.0, 0.0, true);
    assert!((red[0] - 1.0).abs() < 1e-6 && (red[1]).abs() < 1e-6 && (red[2]).abs() < 1e-6);

    let green = hue_saturation(gray, 120.0, 1.0, 0.0, true);
    assert!((green[1] - 1.0).abs() < 1e-6 && (green[0]).abs() < 1e-6 && (green[2]).abs() < 1e-6);
}

#[test]
fn hue_saturation_lightness_adjusts() {
    let c = [0.5, 0.5, 0.5];
    let brighter = hue_saturation(c, 0.0, 0.0, 0.5, false);
    assert!(brighter[0] > 0.7, "brighter: {brighter:?}");
    let darker = hue_saturation(c, 0.0, 0.0, -0.5, false);
    assert!(darker[0] < 0.3, "darker: {darker:?}");
}

#[test]
fn black_white_gray_gray_unchanged() {
    let weights = [40.0, 60.0, 40.0, 60.0, 20.0, 80.0];
    for v in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let g = black_white_gray([v; 3], &weights);
        assert!((g - v).abs() < 1e-6, "{v} -> {g}");
    }
}

#[test]
fn black_white_gray_primary_weights() {
    let weights = [40.0, 60.0, 40.0, 60.0, 20.0, 80.0];
    assert!((black_white_gray([1.0, 0.0, 0.0], &weights) - 0.4).abs() < 1e-6);
    assert!((black_white_gray([1.0, 1.0, 0.0], &weights) - 0.6).abs() < 1e-6);
    assert!((black_white_gray([0.0, 0.0, 1.0], &weights) - 0.2).abs() < 1e-6);
}

#[test]
fn selective_color_weights_pure_colors() {
    let red = selective_color_weights([1.0, 0.0, 0.0]);
    assert_eq!(red[0], 1.0);
    assert!(red.iter().skip(1).all(|&w| w == 0.0));

    let yellow = selective_color_weights([1.0, 1.0, 0.0]);
    assert_eq!(yellow[1], 1.0);
    assert!(yellow.iter().enumerate().all(|(i, &w)| i == 1 || w == 0.0));

    let blue = selective_color_weights([0.0, 0.0, 1.0]);
    assert_eq!(blue[4], 1.0);
    assert!(blue.iter().enumerate().all(|(i, &w)| i == 4 || w == 0.0));
}

#[test]
fn selective_color_zero_is_identity() {
    let zero = [[0.0f32; 4]; 9];
    for c in [[0.9, 0.1, 0.1], [0.5, 0.5, 0.5], [0.2, 0.4, 0.8]] {
        for relative in [true, false] {
            let out = selective_color(c, relative, &zero);
            for i in 0..3 {
                assert!((out[i] - c[i]).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn selective_color_relative_does_not_tint_white() {
    let mut adj = [[0.0f32; 4]; 9];
    adj[6] = [0.0, 0.0, 50.0, 0.0]; // cyan on whites
    let out = selective_color([1.0; 3], true, &adj);
    assert_eq!(out, [1.0; 3]);
}

#[test]
fn lut3d_sample_identity_both_interpolations() {
    let n = 5;
    let table = identity_lut(n);
    let samples = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [0.2, 0.4, 0.8], [0.7, 0.1, 0.9], [0.33, 0.67, 0.5]];
    for c in samples {
        for tetrahedral in [false, true] {
            let out = lut3d_sample(&table, n, c, tetrahedral);
            for i in 0..3 {
                assert!((out[i] - c[i]).abs() < 1e-5, "c={c:?} tet={tetrahedral} out={out:?}");
            }
        }
    }
}

#[test]
fn photo_filter_matrix_density_zero_is_identity() {
    let m = photo_filter_matrix([1.0, 0.0, 0.0], 0.0, false);
    for r in 0..3 {
        for c in 0..3 {
            let expected = if r == c { 1.0 } else { 0.0 };
            // Matrix multiplication with D50 adaption introduces tiny floating-point error.
            assert!((m[r][c] - expected).abs() < 1e-5, "m[{r}][{c}]={}", m[r][c]);
        }
    }
}

#[test]
fn photo_filter_matrix_non_finite_density_is_identity() {
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let m = photo_filter_matrix([1.0, 0.0, 0.0], invalid, false);
        for r in 0..3 {
            for c in 0..3 {
                let expected = if r == c { 1.0 } else { 0.0 };
                assert!((m[r][c] - expected).abs() < 1e-5, "density {invalid} m[{r}][{c}]={}", m[r][c]);
            }
        }
    }
}

#[test]
fn photo_filter_matrix_density_one_is_finite() {
    for normalize in [false, true] {
        let m = photo_filter_matrix([0.2, 0.7, 0.4], 1.0, normalize);
        for r in 0..3 {
            for c in 0..3 {
                assert!(m[r][c].is_finite(), "non-finite at {r},{c} normalize={normalize}");
            }
        }
    }
}
