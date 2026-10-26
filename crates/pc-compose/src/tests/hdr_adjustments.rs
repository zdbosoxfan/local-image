use super::*;

// Measured ramps from upstream 2c163bf7, Photoshop 27.11.
#[test]
fn legacy_brightness_contrast_matches_photoshop() {
    const X: [u8; 17] = [0, 16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 255];
    let measured: [(f32, f32, [u8; 17]); 7] = [
        (40.0, 0.0, [40, 56, 72, 88, 104, 120, 136, 152, 168, 184, 200, 216, 232, 248, 255, 255, 255]),
        (-40.0, 0.0, [0, 0, 0, 8, 24, 40, 56, 72, 88, 104, 120, 136, 152, 168, 184, 200, 215]),
        (0.0, 30.0, [0, 0, 0, 14, 37, 60, 82, 105, 128, 151, 174, 196, 219, 242, 255, 255, 255]),
        (0.0, -30.0, [38, 49, 61, 72, 83, 94, 106, 117, 128, 139, 150, 162, 173, 184, 195, 207, 217]),
        (40.0, 30.0, [3, 25, 48, 71, 94, 117, 139, 162, 185, 208, 231, 253, 255, 255, 255, 255, 255]),
        (100.0, 0.0, [100, 116, 132, 148, 164, 180, 196, 212, 228, 244, 255, 255, 255, 255, 255, 255, 255]),
        (0.0, 80.0, [0, 0, 0, 0, 0, 0, 0, 48, 128, 208, 255, 255, 255, 255, 255, 255, 255]),
    ];
    for (brightness, contrast, want) in measured {
        let rect = Rect::new(0, 0, 17, 1);
        let px = X.iter().map(|x| [f32::from(*x) / 255.0; 4]).map(|p| [p[0], p[1], p[2], 1.0]).collect();
        let mut b = Buffer { rect, px };
        adjust::apply(&Adjustment::BrightnessContrast { brightness, contrast, legacy: true }, &mut b);
        for (i, (p, w)) in b.px.iter().zip(want).enumerate() {
            let got = (p[0] * 255.0).round() as i32;
            assert!((got - i32::from(w)).abs() <= 1, "b {brightness} c {contrast} at {}: {got} vs Photoshop {w}", X[i]);
        }
    }
}

fn evaluate(adj: &Adjustment, input: [f32; 4], depth: SampleType) -> [f32; 4] {
    let mut buf = Buffer::filled(Rect::new(0, 0, 1, 1), input);
    adjust::apply_depth(adj, &mut buf, adjust::Transfer::for_document(ColorMode::Rgb, depth), Some(depth));
    buf.px[0]
}

#[test]
fn float_tones_keep_signed_highlights_and_alpha() {
    let id = LevelsChannel::default();
    let levels = Adjustment::Levels { master: id.clone(), per_channel: [id.clone(), id.clone(), id.clone()], space: Default::default(), black: id.clone() };
    let curve = vec![CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 1.0, output: 1.0 }];
    let curves =
        Adjustment::Curves { master: curve.clone(), per_channel: [curve.clone(), curve.clone(), curve.clone()], space: Default::default(), black: curve };
    let exposure = Adjustment::Exposure { exposure: 0.0, offset: 0.0, gamma: 1.0 };
    for adj in [&levels, &curves, &exposure] {
        for value in [-0.5, 0.0, 0.25, 1.0, 2.0, 8.0] {
            let input = [value, 2.0, -0.5, 0.375];
            assert_eq!(evaluate(adj, input, SampleType::F32), input, "{adj:?}");
        }
        assert_eq!(evaluate(adj, [-0.5, 2.0, 8.0, 0.0], SampleType::F32), [-0.5, 2.0, 8.0, 0.0]);
    }
    let master = LevelsChannel { in_black: 0.1, in_white: 0.9, gamma: 1.4, out_black: 0.05, out_white: 0.8 };
    let red = LevelsChannel { in_black: 0.2, in_white: 0.8, gamma: 0.7, out_black: -0.1, out_white: 1.2 };
    let adj = Adjustment::Levels { master: master.clone(), per_channel: [red.clone(), id.clone(), id.clone()], space: Default::default(), black: id };
    let reference = |ch: &LevelsChannel, v: f64| {
        let t = (v - f64::from(ch.in_black)) / f64::from(ch.in_white - ch.in_black);
        f64::from(ch.out_black) + t.signum() * t.abs().powf(1.0 / f64::from(ch.gamma)) * f64::from(ch.out_white - ch.out_black)
    };
    for v in [-0.5, 0.0, 0.3, 1.0, 2.0] {
        let out = evaluate(&adj, [v, v, v, 0.375], SampleType::F32);
        let want = [reference(&master, reference(&red, f64::from(v))), reference(&master, f64::from(v)), reference(&master, f64::from(v))];
        for (actual, expected) in out[..3].iter().zip(want) {
            assert!((f64::from(*actual) - expected).abs() < 2e-5 * expected.abs().max(1.0), "{v}: {out:?} vs {want:?}");
        }
        assert_eq!(out[3], 0.375);
    }
    let signed = Adjustment::Exposure { exposure: 1.0, offset: -0.25, gamma: 2.0 };
    let out = evaluate(&signed, [-0.375, 0.125, 2.125, 0.375], SampleType::F32);
    assert_eq!(out, [-1.0, 0.0, 2.0, 0.375]);
    let up = Adjustment::Exposure { exposure: 2.0, offset: 0.0, gamma: 1.0 };
    let down = Adjustment::Exposure { exposure: -2.0, offset: 0.0, gamma: 1.0 };
    let input = [-0.5, 2.0, 8.0, 0.375];
    assert_eq!(evaluate(&down, evaluate(&up, input, SampleType::F32), SampleType::F32), input);
    for depth in [SampleType::U8, SampleType::U16] {
        assert!(evaluate(&up, input, depth)[..3].iter().all(|v| (0.0..=1.0).contains(v)));
    }
}

// Measured tint samples from upstream 836fd520, Photoshop 27.11.
#[test]
fn black_white_tint_matches_photoshop() {
    let weights = [40.0, 60.0, 40.0, 60.0, 20.0, 80.0];
    let measured: [([u8; 3], u8, [u8; 3]); 6] = [
        ([255, 0, 0], 24, [81, 1, 1]),
        ([255, 0, 0], 120, [255, 64, 64]),
        ([255, 0, 0], 240, [255, 235, 235]),
        ([0xe1, 0xd3, 0xb3], 120, [133, 119, 87]),
        ([0xe1, 0xd3, 0xb3], 24, [33, 24, 1]),
        ([0x40, 0x20, 0x10], 120, [144, 112, 96]),
    ];
    for (tint, grey, want) in measured {
        let tint = Some(tint.map(|v| v as f32 / 255.0));
        let mut b = Buffer { rect: Rect::new(0, 0, 1, 1), px: vec![[grey as f32 / 255.0; 4]] };
        b.px[0][3] = 1.0;
        adjust::apply(&Adjustment::BlackWhite { weights, tint }, &mut b);
        for (i, w) in want.iter().enumerate() {
            let got = b.px[0][i] * 255.0;
            assert!((got - *w as f32).abs() <= 2.5, "tint {tint:?} grey {grey} channel {i}: {got} vs {w}");
        }
    }
}

// Native float measurements attributed to upstream 620bccef, Photoshop 27.11.
#[test]
fn float_legacy_mixer_and_tint_match_measured_values() {
    let near = |adj: &Adjustment, c: [f32; 3], want: [f32; 3], tolerance: f32| {
        let out = evaluate(adj, [c[0], c[1], c[2], 0.375], SampleType::F32);
        assert!(out[..3].iter().zip(want).all(|(v, w)| (v - w).abs() <= tolerance), "{c:?}: {out:?} vs {want:?}");
        assert_eq!(out[3], 0.375);
    };
    let bc = Adjustment::BrightnessContrast { brightness: 25.0, contrast: 30.0, legacy: true };
    near(&bc, [0.4910, 0.0009, 0.9301], [0.6280, -0.0721, 1.2553], 2e-4);
    let matrix = [[0.8, 0.3, -0.1, 0.0], [0.1, 0.9, 0.0, 0.05], [0.0, 0.2, 0.7, 0.0]];
    let mixer = Adjustment::ChannelMixer { matrix, monochrome: false };
    for (c, want) in [
        ([1.0, 0.0, 0.0], [0.7998047, 0.1245117, 0.0]),
        ([0.0, 1.0, 0.0], [0.2998047, 0.9243164, 0.1992188]),
        ([0.0, 0.0, 1.0], [-0.0996094, 0.0249023, 0.6992188]),
        ([0.0, 0.0, 0.0], [0.0, 0.0249023, 0.0]),
        ([2.0, -0.5, 1.0], [1.35009765625, -0.2255859375, 0.599609375]),
    ] {
        near(&mixer, c, want, 1e-6);
    }
    near(&Adjustment::ChannelMixer { matrix, monochrome: true }, [2.0, -0.5, 1.0], [1.35009765625; 3], 1e-6);
    let bw = Adjustment::BlackWhite { weights: [40.0, 60.0, 40.0, 60.0, 20.0, 80.0], tint: Some([0.88235295, 0.827451, 0.7019608]) };
    near(&bw, [1.0; 3], [1.0522, 0.9973, 0.8718], 2e-4);
    for depth in [SampleType::U8, SampleType::U16] {
        assert!(evaluate(&bw, [1.0; 4], depth).iter().all(|v| *v <= 1.0));
    }
}

#[test]
fn float_photo_filter_applies_xyz_scaling_to_signed_linear_samples() {
    use li_color::convert::{D50, SRGB_TO_XYZ_D50, XYZ_D50_TO_SRGB, mat3_mul, srgb_to_linear};
    let color = [0.966_445_6, 0.761_806_3, 0.0];
    for preserve in [false, true] {
        let adj = Adjustment::PhotoFilter { color, density: 0.4, preserve_luminosity: preserve };
        let filter = mat3_mul(&SRGB_TO_XYZ_D50, color.map(srgb_to_linear));
        let mut scale: [f32; 3] = std::array::from_fn(|i| 0.6 + 0.4 * filter[i] / D50[i]);
        if preserve {
            scale = scale.map(|v| v / scale[1]);
        }
        for c in [[-0.5, 2.0, 8.0], [1.0, 1.0, 0.0], [1.0, 0.0, 0.0]] {
            let xyz = mat3_mul(&SRGB_TO_XYZ_D50, c);
            let expected = mat3_mul(&XYZ_D50_TO_SRGB, std::array::from_fn(|i| xyz[i] * scale[i]));
            let out = evaluate(&adj, [c[0], c[1], c[2], 0.375], SampleType::F32);
            assert!(out[..3].iter().zip(expected).all(|(v, w)| (v - w).abs() < 4e-6 * w.abs().max(1.0)), "preserve={preserve}: {out:?} vs {expected:?}");
            assert_eq!(out[3], 0.375);
            if preserve {
                let got_xyz = mat3_mul(&SRGB_TO_XYZ_D50, [out[0], out[1], out[2]]);
                assert!((got_xyz[1] - xyz[1]).abs() < 2e-5);
            }
        }
    }
}
