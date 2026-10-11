use photocraft_doc::adjust::{Adjustment, CurvePoint, HueRange, LevelsChannel, ToneSpace};
use std::sync::Arc;

#[test]
fn identity_curve_detects_identity_and_edge_cases() {
    // Empty slice is identity (length < 2).
    assert!(photocraft_doc::adjust::is_identity_curve(&[]));
    // Single point is identity.
    assert!(photocraft_doc::adjust::is_identity_curve(&[CurvePoint { input: 0.5, output: 0.5 }]));
    // Exact two-point identity.
    assert!(photocraft_doc::adjust::is_identity_curve(&[CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 1.0, output: 1.0 },]));
    // Two points not identity.
    assert!(!photocraft_doc::adjust::is_identity_curve(&[CurvePoint { input: 0.0, output: 0.1 }, CurvePoint { input: 1.0, output: 1.0 },]));
    // More than two points is never matched by the pattern, so false.
    assert!(!photocraft_doc::adjust::is_identity_curve(&[
        CurvePoint { input: 0.0, output: 0.0 },
        CurvePoint { input: 0.5, output: 0.5 },
        CurvePoint { input: 1.0, output: 1.0 },
    ]));
}

#[test]
fn levels_channel_default_is_identity() {
    let lc = LevelsChannel::default();
    assert_eq!(lc.in_black, 0.0);
    assert_eq!(lc.in_white, 1.0);
    assert_eq!(lc.gamma, 1.0);
    assert_eq!(lc.out_black, 0.0);
    assert_eq!(lc.out_white, 1.0);
}

#[test]
fn tone_space_defaults_to_rgb() {
    assert_eq!(ToneSpace::default(), ToneSpace::Rgb);
}

#[test]
fn hue_range_neutral_bounds_match_photoshop_defaults() {
    let expected = [
        [-45.0, -15.0, 15.0, 45.0],   // reds
        [15.0, 45.0, 75.0, 105.0],    // yellows
        [75.0, 105.0, 135.0, 165.0],  // greens
        [135.0, 165.0, 195.0, 225.0], // cyans
        [195.0, 225.0, 255.0, 285.0], // blues
        [255.0, 285.0, 315.0, 345.0], // magentas
    ];
    for i in 0..6 {
        let hr = HueRange::neutral(i);
        let b = hr.bounds;
        assert!(
            (b[0] - expected[i][0]).abs() < 1e-6
                && (b[1] - expected[i][1]).abs() < 1e-6
                && (b[2] - expected[i][2]).abs() < 1e-6
                && (b[3] - expected[i][3]).abs() < 1e-6,
            "neutral({}) bounds {:?} != {:?}",
            i,
            b,
            expected[i]
        );
    }
}

#[test]
fn hue_range_defaults_are_six_neutral_ranges() {
    let defaults = HueRange::defaults();
    assert_eq!(defaults.len(), 6);
    for (i, hr) in defaults.iter().enumerate() {
        assert_eq!(hr, &HueRange::neutral(i));
    }
}

#[test]
fn hue_range_is_neutral_true_only_when_all_zero() {
    let neutral = HueRange::neutral(0);
    assert!(neutral.is_neutral());
    let non_neutral = HueRange { hue: 1.0, saturation: 0.0, lightness: 0.0, bounds: neutral.bounds };
    assert!(!non_neutral.is_neutral());
    let non_neutral2 = HueRange { hue: 0.0, saturation: 0.1, lightness: 0.0, bounds: neutral.bounds };
    assert!(!non_neutral2.is_neutral());
    let non_neutral3 = HueRange { hue: 0.0, saturation: 0.0, lightness: -0.1, bounds: neutral.bounds };
    assert!(!non_neutral3.is_neutral());
}

#[test]
fn hue_range_canonical_bounds_preserves_default_reds() {
    let reds = HueRange::neutral(0);
    let canonical = HueRange::canonical_bounds(reds.bounds);
    let expected = [-45.0, -15.0, 15.0, 45.0];
    for i in 0..4 {
        assert!((canonical[i] - expected[i]).abs() < 1e-6);
    }
}

#[test]
fn hue_range_canonical_bounds_normalises_wrapped_and_non_finite() {
    // Rewritten reds in wrapped positive degrees: 315, 345, 15, 45 -> canonical -45,-15,15,45
    let wrapped = [315.0, 345.0, 15.0, 45.0];
    let canonical = HueRange::canonical_bounds(wrapped);
    let expected = [-45.0, -15.0, 15.0, 45.0];
    for i in 0..4 {
        assert!((canonical[i] - expected[i]).abs() < 1e-6);
    }
    // Non-finite returns the default reds.
    let non_finite = [f32::NAN, 0.0, 0.0, 0.0];
    let canonical = HueRange::canonical_bounds(non_finite);
    let expected = [-45.0, -15.0, 15.0, 45.0];
    for i in 0..4 {
        assert!((canonical[i] - expected[i]).abs() < 1e-6);
    }
    let inf = [f32::INFINITY, 0.0, 0.0, 0.0];
    let canonical = HueRange::canonical_bounds(inf);
    for i in 0..4 {
        assert!((canonical[i] - expected[i]).abs() < 1e-6);
    }
}

#[test]
fn hue_range_weight_handles_wrap_and_fades() {
    let reds = HueRange::neutral(0);
    // Center of reds.
    assert!((reds.weight(0.0) - 1.0).abs() < 1e-6);
    // Wrapped equivalent.
    assert!((reds.weight(350.0) - 1.0).abs() < 1e-6);
    // Boundaries at 30° -> half weight.
    assert!((reds.weight(30.0) - 0.5).abs() < 1e-5);
    assert!((reds.weight(330.0) - 0.5).abs() < 1e-5);
    // Outside range.
    assert!((reds.weight(60.0) - 0.0).abs() < 1e-6);
    assert!((reds.weight(180.0) - 0.0).abs() < 1e-6);

    let blues = HueRange::neutral(4);
    assert!((blues.weight(240.0) - 1.0).abs() < 1e-6);
    assert!((blues.weight(120.0) - 0.0).abs() < 1e-6);
}

#[test]
fn hue_range_weight_never_panics_for_extreme_bounds() {
    // Degenerate bounds where all values equal: no division by zero.
    let odd = HueRange { bounds: [10.0, 10.0, 10.0, 10.0], ..HueRange::neutral(0) };
    assert!(odd.weight(10.0).is_finite());
    assert!(odd.weight(200.0).is_finite());
    // NaN bounds: `rel` maps non-finite to 0.0, so no panic.
    let nan = HueRange { bounds: [f32::NAN; 4], ..HueRange::neutral(0) };
    assert!(nan.weight(10.0).is_finite());
    // Infinity in bounds also handled.
    let inf = HueRange { bounds: [f32::INFINITY, 0.0, 0.0, 0.0], ..HueRange::neutral(0) };
    assert!(inf.weight(10.0).is_finite());
}

#[test]
fn hue_range_weight_stays_in_unit_range_for_all_hues() {
    // Test all integer degrees for both normal and potentially problematic bounds.
    let normal = HueRange::neutral(2); // greens
    for deg in 0..360 {
        let w = normal.weight(deg as f32);
        assert!(w.is_finite() && (0.0..=1.0).contains(&w), "weight({}) = {} out of range", deg, w);
    }
    // Degenerate bounds
    let degenerate = HueRange { bounds: [30.0, 30.0, 30.0, 30.0], ..HueRange::neutral(0) };
    for deg in 0..360 {
        let w = degenerate.weight(deg as f32);
        assert!(w.is_finite() && (0.0..=1.0).contains(&w), "degenerate weight({}) = {} out of range", deg, w);
    }
}

#[test]
fn adjustment_labels_are_stable() {
    let cases = [
        (Adjustment::BrightnessContrast { brightness: 0.0, contrast: 0.0, legacy: false }, "Brightness/Contrast"),
        (Adjustment::identity_levels(), "Levels"),
        (Adjustment::identity_curves(), "Curves"),
        (Adjustment::Exposure { exposure: 0.0, offset: 0.0, gamma: 1.0 }, "Exposure"),
        (Adjustment::Vibrance { vibrance: 0.0, saturation: 0.0 }, "Vibrance"),
        (Adjustment::default_hue_saturation(), "Hue/Saturation"),
        (Adjustment::ColorBalance { shadows: [0.0; 3], midtones: [0.0; 3], highlights: [0.0; 3], preserve_luminosity: false }, "Color Balance"),
        (Adjustment::BlackWhite { weights: [0.0; 6], tint: None }, "Black & White"),
        (Adjustment::PhotoFilter { color: [0.0; 3], density: 0.0, preserve_luminosity: false }, "Photo Filter"),
        (Adjustment::ChannelMixer { matrix: [[0.0; 4]; 3], monochrome: false }, "Channel Mixer"),
        (Adjustment::ColorLookup { name: "x".into(), lut: None, size: 0, tetrahedral: false, dither: false }, "Color Lookup"),
        (Adjustment::Invert, "Invert"),
        (Adjustment::Posterize { levels: 2 }, "Posterize"),
        (Adjustment::Threshold { level: 0.5 }, "Threshold"),
        (Adjustment::GradientMap { stops: vec![], reverse: false, dither: false }, "Gradient Map"),
        (Adjustment::SelectiveColor { relative: true, adjustments: [[0.0; 4]; 9] }, "Selective Color"),
        (Adjustment::Unsupported { psd_key: "k".into(), raw: vec![] }, "Adjustment"),
    ];
    for (adj, expected) in cases {
        assert_eq!(adj.label(), expected);
    }
}

#[test]
fn default_hue_saturation_is_neutral() {
    let adj = Adjustment::default_hue_saturation();
    match adj {
        Adjustment::HueSaturation { hue, saturation, lightness, colorize, ranges } => {
            assert_eq!(hue, 0.0);
            assert_eq!(saturation, 0.0);
            assert_eq!(lightness, 0.0);
            assert!(!colorize);
            assert_eq!(ranges, HueRange::defaults());
        }
        _ => panic!("expected HueSaturation"),
    }
}

#[test]
fn identity_curves_constructs_correct_curve() {
    let adj = Adjustment::identity_curves();
    match adj {
        Adjustment::Curves { master, per_channel, space, black } => {
            assert_eq!(space, ToneSpace::Rgb);
            assert!(black.is_empty());
            let expected_line = || vec![CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 1.0, output: 1.0 }];
            assert_eq!(master, expected_line());
            for ch in per_channel.iter() {
                assert_eq!(ch, &expected_line());
            }
        }
        _ => panic!("expected Curves"),
    }
}

#[test]
fn identity_levels_constructs_correct_levels() {
    let adj = Adjustment::identity_levels();
    match adj {
        Adjustment::Levels { master, per_channel, space, black } => {
            assert_eq!(space, ToneSpace::Rgb);
            assert_eq!(master, LevelsChannel::default());
            for ch in per_channel.iter() {
                assert_eq!(ch, &LevelsChannel::default());
            }
            assert_eq!(black, LevelsChannel::default());
        }
        _ => panic!("expected Levels"),
    }
}

#[test]
fn adjustment_serialization_round_trips_all_variants() {
    let adjustments = vec![
        Adjustment::BrightnessContrast { brightness: 0.5, contrast: -0.2, legacy: true },
        Adjustment::Levels {
            master: LevelsChannel { in_black: 0.1, in_white: 0.9, gamma: 1.1, out_black: 0.0, out_white: 1.0 },
            per_channel: [
                LevelsChannel { in_black: 0.0, in_white: 1.0, gamma: 0.8, out_black: 0.1, out_white: 0.9 },
                LevelsChannel::default(),
                LevelsChannel::default(),
            ],
            space: ToneSpace::Cmyk,
            black: LevelsChannel { in_black: 0.0, in_white: 1.0, gamma: 1.2, out_black: 0.0, out_white: 1.0 },
        },
        Adjustment::Curves {
            master: vec![CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 0.5, output: 0.6 }, CurvePoint { input: 1.0, output: 1.0 }],
            per_channel: [vec![CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 1.0, output: 1.0 }], vec![], vec![]],
            space: ToneSpace::Lab,
            black: vec![],
        },
        Adjustment::Exposure { exposure: 1.0, offset: 0.1, gamma: 1.2 },
        Adjustment::Vibrance { vibrance: 0.3, saturation: -0.1 },
        Adjustment::HueSaturation { hue: 10.0, saturation: 20.0, lightness: -5.0, colorize: true, ranges: HueRange::defaults() },
        Adjustment::ColorBalance { shadows: [0.1, -0.2, 0.3], midtones: [0.4, 0.5, 0.6], highlights: [-0.1, 0.0, 0.2], preserve_luminosity: true },
        Adjustment::BlackWhite { weights: [0.2, 0.3, 0.4, 0.1, 0.0, 0.0], tint: Some([0.5, 0.5, 0.5]) },
        Adjustment::PhotoFilter { color: [1.0, 0.8, 0.6], density: 0.25, preserve_luminosity: false },
        Adjustment::ChannelMixer { matrix: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]], monochrome: false },
        Adjustment::ColorLookup { name: "test".into(), lut: Some(Arc::new(vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0])), size: 2, tetrahedral: true, dither: true },
        Adjustment::Invert,
        Adjustment::Posterize { levels: 8 },
        Adjustment::Threshold { level: 0.5 },
        Adjustment::GradientMap { stops: vec![(0.0, [0.0, 0.0, 0.0]), (1.0, [1.0, 1.0, 1.0])], reverse: false, dither: true },
        Adjustment::SelectiveColor { relative: true, adjustments: [[0.1, -0.2, 0.3, 0.0]; 9] },
        Adjustment::Unsupported { psd_key: "key".into(), raw: vec![1, 2, 3] },
    ];

    for (idx, adj) in adjustments.iter().enumerate() {
        let json = serde_json::to_string(adj).expect("serialization failed");
        let decoded: Adjustment = serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(&decoded, adj, "round trip failed for variant index {}", idx);
    }
}

#[test]
fn adjustment_deserialization_accepts_old_formats_with_defaults() {
    // Old HueSaturation without ranges -> ranges default.
    let a: Adjustment = serde_json::from_str(r#"{"HueSaturation":{"hue":1.0,"saturation":2.0,"lightness":3.0,"colorize":false}}"#).unwrap();
    match a {
        Adjustment::HueSaturation { ranges, .. } => assert_eq!(ranges, HueRange::defaults()),
        _ => panic!("expected HueSaturation"),
    }

    // Old Curves without space -> ToneSpace::Rgb.
    let c: Adjustment = serde_json::from_str(r#"{"Curves":{"master":[],"per_channel":[[],[],[]],"black":[]}}"#).unwrap();
    match c {
        Adjustment::Curves { space, .. } => assert_eq!(space, ToneSpace::Rgb),
        _ => panic!("expected Curves"),
    }

    // Old GradientMap without dither -> dither false.
    let g: Adjustment = serde_json::from_str(r#"{"GradientMap":{"stops":[],"reverse":true}}"#).unwrap();
    match g {
        Adjustment::GradientMap { dither, .. } => assert!(!dither),
        _ => panic!("expected GradientMap"),
    }
}

#[test]
fn adjustment_deserialization_rejects_malformed_input() {
    let result: Result<Adjustment, _> = serde_json::from_str("not json");
    assert!(result.is_err());
    let result: Result<Adjustment, _> = serde_json::from_str(r#"{"UnknownVariant":{}}"#);
    assert!(result.is_err());
    let result: Result<Adjustment, _> = serde_json::from_str(r#"{"BrightnessContrast":{"brightness":"wrong"}}"#);
    assert!(result.is_err());
}
