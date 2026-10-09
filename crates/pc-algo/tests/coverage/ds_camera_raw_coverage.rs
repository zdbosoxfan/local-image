use photocraft_algo::camera_raw::{CameraRaw, HSL_BANDS, MAX_CURVE_POINTS, Wheel, band_weights, curve_lut, develop, validate_curve};

fn gradient_img(w: usize, h: usize) -> Vec<[f32; 4]> {
    (0..w * h)
        .map(|i| {
            let x = (i % w) as f32 / w.max(1) as f32;
            let y = (i / w) as f32 / h.max(1) as f32;
            [0.2 + 0.6 * x, 0.3 + 0.4 * y, 0.5 - 0.3 * x * y, 1.0]
        })
        .collect()
}

fn flat_img(w: usize, h: usize, rgb: [f32; 3]) -> Vec<[f32; 4]> {
    vec![[rgb[0], rgb[1], rgb[2], 1.0]; w * h]
}

fn pseudo_random(i: usize) -> f32 {
    let a = (i as u64).wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let b = a ^ (a >> 33);
    let c = b.wrapping_mul(0xff51afd7ed558ccd);
    let d = c ^ (c >> 33);
    ((d >> 33) as f32) / (1u64 << 31) as f32
}

fn random_noise_img(w: usize, h: usize) -> Vec<[f32; 4]> {
    (0..w * h)
        .map(|i| {
            let n = (pseudo_random(i) - 0.5) * 0.4; // uniform in [-0.2, 0.2]
            let v = 0.5 + n;
            [v, v, v, 1.0]
        })
        .collect()
}

fn mean_channel(px: &[[f32; 4]], c: usize) -> f32 {
    px.iter().map(|q| q[c]).sum::<f32>() / px.len().max(1) as f32
}

fn var_channel(px: &[[f32; 4]], c: usize) -> f32 {
    let m = mean_channel(px, c);
    px.iter().map(|q| (q[c] - m).powi(2)).sum::<f32>() / px.len().max(1) as f32
}

#[test]
fn validate_curve_accepts_valid_and_rejects_invalid() {
    // valid: empty, two points, max points increasing by at least one
    assert!(validate_curve(&[]).is_ok());
    assert!(validate_curve(&[[0.0, 0.0], [255.0, 255.0]]).is_ok());
    let max_points: Vec<[f32; 2]> = (0..MAX_CURVE_POINTS).map(|i| [i as f32 * 17.0, i as f32 * 17.0]).collect();
    assert_eq!(max_points.len(), MAX_CURVE_POINTS);
    assert!(validate_curve(&max_points).is_ok());

    // invalid lengths
    assert!(validate_curve(&[[10.0, 10.0]]).is_err());
    let too_many: Vec<[f32; 2]> = (0..=MAX_CURVE_POINTS).map(|i| [i as f32 * 10.0, i as f32 * 10.0]).collect();
    assert!(validate_curve(&too_many).is_err());

    // coordinates out of range / non-finite
    for points in
        [vec![[-0.1, 128.0], [255.0, 255.0]], vec![[0.0, 0.0], [255.1, 255.0]], vec![[0.0, f32::NAN], [255.0, 255.0]], vec![[0.0, 0.0], [f32::INFINITY, 255.0]]]
    {
        assert!(validate_curve(&points).is_err(), "{:?}", points);
    }

    // inputs not increasing by at least one level
    assert!(validate_curve(&[[0.0, 0.0], [0.5, 255.0], [255.0, 255.0]]).is_err());
    assert!(validate_curve(&[[10.0, 0.0], [10.5, 255.0]]).is_err());
    // but one level exactly is fine
    assert!(validate_curve(&[[0.0, 0.0], [1.0, 255.0], [255.0, 255.0]]).is_ok());
}

#[test]
fn camera_raw_default_is_identity_and_develop_preserves() {
    let raw = CameraRaw::default();
    assert!(raw.is_identity());
    assert!(raw.validate().is_ok());

    let mut px = gradient_img(20, 15);
    let before = px.clone();
    develop(&mut px, 20, 15, &raw, false);
    assert_eq!(px, before);
}

#[test]
fn camera_raw_serialization_round_trip_and_default_from_empty() {
    let raw = CameraRaw {
        temperature: 25.0,
        tint: -10.0,
        exposure: 0.5,
        contrast: 20.0,
        highlights: -30.0,
        shadows: 15.0,
        whites: 10.0,
        blacks: -5.0,
        texture: 40.0,
        clarity: 50.0,
        dehaze: 30.0,
        vibrance: 25.0,
        saturation: -15.0,
        curve_highlights: 10.0,
        curve_lights: -5.0,
        curve_darks: 20.0,
        curve_shadows: -10.0,
        curve_splits: [30.0, 55.0, 80.0],
        point_curve: vec![[0.0, 10.0], [128.0, 140.0], [255.0, 250.0]],
        point_curve_red: vec![[0.0, 0.0], [255.0, 255.0]],
        point_curve_green: vec![[0.0, 5.0], [255.0, 250.0]],
        point_curve_blue: vec![[0.0, 0.0], [255.0, 255.0]],
        hsl_hue: [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0],
        hsl_sat: [-10.0, -20.0, -30.0, -40.0, -50.0, -60.0, -70.0, -80.0],
        hsl_lum: [5.0, -5.0, 10.0, -10.0, 15.0, -15.0, 20.0, -20.0],
        grade_shadows: Wheel { hue: 220.0, sat: 50.0, lum: -10.0 },
        grade_midtones: Wheel { hue: 45.0, sat: 30.0, lum: 5.0 },
        grade_highlights: Wheel { hue: 0.0, sat: 0.0, lum: 0.0 },
        grade_global: Wheel { hue: 180.0, sat: 20.0, lum: 0.0 },
        grade_blending: 60.0,
        grade_balance: -20.0,
        sharpen_amount: 50.0,
        sharpen_radius: 1.5,
        sharpen_detail: 30.0,
        sharpen_masking: 10.0,
        noise_luminance: 20.0,
        noise_luminance_detail: 40.0,
        noise_color: 15.0,
        noise_color_detail: 35.0,
        grain_amount: 25.0,
        grain_size: 30.0,
        grain_roughness: 60.0,
        vignette_amount: -40.0,
        vignette_midpoint: 45.0,
        vignette_roundness: 20.0,
        vignette_feather: 55.0,
        vignette_highlights: 30.0,
        vignette_style: "highlightPriority".to_string(),
        seed: 42,
        pixel_scale: 0.8,
    };

    let s = serde_json::to_string(&raw).unwrap();
    let back: CameraRaw = serde_json::from_str(&s).unwrap();
    assert_eq!(raw, back);

    let default_from_json: CameraRaw = serde_json::from_str("{}").unwrap();
    assert_eq!(default_from_json, CameraRaw::default());
}

#[test]
fn validate_and_validate_changes() {
    // invalid legacy curve still deserializes but fails validate
    let legacy_points: Vec<[f32; 2]> = (0..20).map(|i| [i as f32 * 12.0, i as f32 * 12.0]).collect();
    let legacy = CameraRaw { point_curve: legacy_points, ..Default::default() };
    assert!(legacy.validate().is_err());

    // validate_changes with same curve and another changed field is ok
    let edited = CameraRaw { exposure: 1.0, ..legacy.clone() };
    assert!(edited.validate_changes(&legacy).is_ok());

    // changed curve that is invalid should fail
    let broken = CameraRaw { point_curve: vec![[10.0, 10.0]], ..legacy.clone() };
    assert!(broken.validate_changes(&legacy).is_err());

    // default and valid curve pass
    assert!(CameraRaw::default().validate().is_ok());
    let valid = CameraRaw { point_curve: vec![[0.0, 0.0], [255.0, 255.0]], ..Default::default() };
    assert!(valid.validate().is_ok());
    assert!(valid.validate_changes(&CameraRaw::default()).is_ok());
}

#[test]
fn curve_lut_is_monotone_clamped_and_respects_points() {
    let pts = [[0.0, 0.0], [64.0, 40.0], [192.0, 220.0], [255.0, 255.0]];
    let lut = curve_lut(&pts, 256);
    assert_eq!(lut.len(), 256);
    assert!(lut.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    assert!(lut.windows(2).all(|p| p[1] >= p[0]));
    assert!((lut[64] - 40.0 / 255.0).abs() < 0.01);
    assert!((lut[192] - 220.0 / 255.0).abs() < 0.01);
    assert!((lut[0] - pts[0][1] / 255.0).abs() < 1e-6);
    assert!((lut[255] - pts[3][1] / 255.0).abs() < 1e-6);

    // with fewer than two points returns identity
    let identity = curve_lut(&[], 128);
    assert_eq!(identity.len(), 128);
    assert!(identity.windows(2).all(|p| (p[1] - p[0] - 1.0 / 127.0).abs() < 1e-6));
}

#[test]
fn band_weights_sum_to_one_and_peak_at_centers() {
    for h in (0..360).step_by(5) {
        let wts = band_weights(h as f32);
        let sum: f32 = wts.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4, "h={h} sum={sum}");
        assert!(wts.iter().all(|w| *w >= 0.0 && *w <= 1.0));
    }
    for (i, center) in HSL_BANDS.iter().enumerate() {
        let wts = band_weights(*center);
        assert!(wts[i] > 1.0 - 1e-4, "band {i} center {center} weight {}", wts[i]);
    }
}

#[test]
fn develop_empty_and_tiny_images_do_not_panic() {
    let mut empty: Vec<[f32; 4]> = Vec::new();
    develop(&mut empty, 0, 0, &CameraRaw::default(), false);
    develop(&mut empty, 0, 0, &CameraRaw { exposure: 1.0, ..Default::default() }, false);

    let mut one = vec![[0.4, 0.5, 0.6, 1.0]];
    let before = one[0];
    develop(&mut one, 1, 1, &CameraRaw::default(), false);
    assert_eq!(one[0], before);
    develop(&mut one, 1, 1, &CameraRaw { exposure: 0.5, ..Default::default() }, false);
    assert!(one[0].iter().all(|v| v.is_finite()));
}

#[test]
fn develop_odd_sizes_are_finite_and_in_range() {
    let (w, h) = (3, 5);
    let raw = CameraRaw {
        exposure: 0.7,
        contrast: 30.0,
        highlights: -20.0,
        shadows: 20.0,
        clarity: 40.0,
        texture: 30.0,
        dehaze: 25.0,
        vibrance: 20.0,
        saturation: 10.0,
        point_curve: vec![[0.0, 10.0], [128.0, 140.0], [255.0, 250.0]],
        hsl_hue: [5.0, -5.0, 10.0, -10.0, 15.0, -15.0, 20.0, -20.0],
        hsl_sat: [-10.0, 10.0, -5.0, 5.0, -15.0, 15.0, -20.0, 20.0],
        grade_shadows: Wheel { hue: 200.0, sat: 30.0, lum: -5.0 },
        grade_midtones: Wheel { hue: 50.0, sat: 20.0, lum: 5.0 },
        sharpen_amount: 30.0,
        noise_luminance: 20.0,
        grain_amount: 15.0,
        vignette_amount: -20.0,
        ..Default::default()
    };
    let mut px = gradient_img(w, h);
    develop(&mut px, w, h, &raw, false);
    assert!(px.iter().flatten().all(|v| v.is_finite()));
    assert!(px.iter().all(|q| q[..3].iter().all(|v| (0.0..=1.0).contains(v))));
}

#[test]
fn develop_exposure_lightens() {
    let (w, h) = (8, 8);
    let original = flat_img(w, h, [0.5, 0.5, 0.5]);
    let mut exposed = original.clone();
    develop(&mut exposed, w, h, &CameraRaw { exposure: 1.0, ..Default::default() }, false);
    assert!(mean_channel(&exposed, 1) > mean_channel(&original, 1) + 0.1);
}

#[test]
fn develop_temperature_shifts_warm() {
    let (w, h) = (8, 8);
    let original = flat_img(w, h, [0.5, 0.5, 0.5]);
    let mut warm = original.clone();
    develop(&mut warm, w, h, &CameraRaw { temperature: 60.0, ..Default::default() }, false);
    assert!(mean_channel(&warm, 0) > mean_channel(&original, 0));
    assert!(mean_channel(&warm, 2) < mean_channel(&original, 2));
}

#[test]
fn develop_saturation_negative_desaturates() {
    let (w, h) = (4, 4);
    let red = flat_img(w, h, [0.8, 0.2, 0.2]);
    let mut desat = red.clone();
    develop(&mut desat, w, h, &CameraRaw { saturation: -100.0, ..Default::default() }, false);
    for q in &desat {
        assert!((q[0] - q[1]).abs() < 1e-4, "{:?}", q);
        assert!((q[1] - q[2]).abs() < 1e-4, "{:?}", q);
    }
}

#[test]
fn develop_vignette_darkens_corners_keeps_center() {
    let (w, h) = (8, 6);
    let flat = flat_img(w, h, [0.5, 0.5, 0.5]);
    let mut vin = flat.clone();
    develop(&mut vin, w, h, &CameraRaw { vignette_amount: -80.0, ..Default::default() }, false);
    let corner = vin[0];
    let center = vin[(h / 2) * w + w / 2];
    assert!(corner[0] < 0.35, "corner {:?}", corner);
    assert!((center[0] - 0.5).abs() < 0.01, "center {:?}", center);
}

#[test]
fn develop_grain_deterministic_and_not_identity() {
    let (w, h) = (32, 24);
    let seed = 7;
    let raw = CameraRaw { grain_amount: 60.0, seed, ..Default::default() };
    let mut a = gradient_img(w, h);
    let mut b = gradient_img(w, h);
    develop(&mut a, w, h, &raw, false);
    develop(&mut b, w, h, &raw, false);
    assert_eq!(a, b);
    assert_ne!(a, gradient_img(w, h));
}

#[test]
fn develop_noise_reduction_reduces_variance_and_sharpen_increases() {
    let (w, h) = (36, 28);
    let noisy = random_noise_img(w, h);
    let original_var = var_channel(&noisy, 1);

    let mut nr = noisy.clone();
    // Set noise_luminance_detail to 0 to force maximum smoothing (keep = 0).
    develop(&mut nr, w, h, &CameraRaw { noise_luminance: 80.0, noise_luminance_detail: 0.0, ..Default::default() }, false);
    let nr_var = var_channel(&nr, 1);
    assert!(nr_var < original_var, "var after NR {} original {}", nr_var, original_var);

    let mut sharp = noisy.clone();
    develop(&mut sharp, w, h, &CameraRaw { sharpen_amount: 100.0, ..Default::default() }, false);
    let sharp_var = var_channel(&sharp, 1);
    assert!(sharp_var > original_var, "var after sharpen {} original {}", sharp_var, original_var);
}

#[test]
fn develop_float_keeps_overrange_and_non_float_clamps() {
    let (w, h) = (2, 2);
    let mut float_px = vec![[2.0, 1.5, 0.5, 1.0]; w * h];
    develop(&mut float_px, w, h, &CameraRaw { exposure: 0.5, ..Default::default() }, true);
    assert!(float_px[0][0] > 2.0, "float overrange kept: {:?}", float_px[0]);

    let mut clamp_px = vec![[2.0, 1.5, 0.5, 1.0]; w * h];
    develop(&mut clamp_px, w, h, &CameraRaw { exposure: 1.0, ..Default::default() }, false);
    assert!(clamp_px.iter().all(|q| q[..3].iter().all(|v| (0.0..=1.0).contains(v))));
}

#[test]
fn develop_preserves_alpha() {
    let (w, h) = (8, 6);
    let mut px = gradient_img(w, h);
    let raw = CameraRaw {
        exposure: 1.0,
        contrast: 50.0,
        saturation: 50.0,
        dehaze: 50.0,
        grain_amount: 50.0,
        vignette_amount: -50.0,
        sharpen_amount: 80.0,
        ..Default::default()
    };
    develop(&mut px, w, h, &raw, false);
    assert!(px.iter().all(|q| (q[3] - 1.0).abs() < 1e-6));
}
