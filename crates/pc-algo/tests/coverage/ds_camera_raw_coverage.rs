use photocraft_algo::camera_raw::{CameraRaw, MAX_CURVE_POINTS, Wheel, validate_curve};

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
fn camera_raw_default_is_identity() {
    let raw = CameraRaw::default();
    assert!(raw.is_identity());
    assert!(raw.validate().is_ok());
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
