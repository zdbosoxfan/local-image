use photocraft_algo::camera_raw::{CameraRaw, MAX_CURVE_POINTS, validate_curve};

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
