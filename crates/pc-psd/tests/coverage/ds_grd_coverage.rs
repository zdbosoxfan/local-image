use photocraft_psd::grd::{self, GrdColor, GrdGradient, GrdStop};

fn gradient(name: &str, noise: bool, stops: Vec<GrdStop>, opacity: Vec<(f32, f32, f32)>) -> GrdGradient {
    GrdGradient { name: name.to_string(), noise, stops, opacity }
}

fn basic_stops() -> Vec<GrdStop> {
    vec![
        GrdStop { location: 0.0, midpoint: 0.5, color: GrdColor::Foreground },
        GrdStop { location: 0.25, midpoint: 0.3, color: GrdColor::Rgb([1.0, 0.5, 0.0]) },
        GrdStop { location: 1.0, midpoint: 0.5, color: GrdColor::Background },
    ]
}

fn basic_opacity() -> Vec<(f32, f32, f32)> {
    vec![(0.0, 1.0, 0.5), (1.0, 0.25, 0.5)]
}

fn sample() -> Vec<GrdGradient> {
    vec![gradient("Sunset ✓", false, basic_stops(), basic_opacity()), gradient("Noise", true, vec![], vec![])]
}

#[test]
fn parse_rejects_empty_input() {
    assert!(grd::parse(b"").is_err());
}

#[test]
fn parse_rejects_wrong_signature() {
    assert!(grd::parse(b"8BPS\0\x05").is_err());
    assert!(grd::parse(b"    \0\x05").is_err());
    assert!(grd::parse(b"8BGR").is_err()); // too short
}

#[test]
fn parse_rejects_unsupported_version() {
    // Version 3 is explicitly rejected
    let mut v3 = b"8BGR".to_vec();
    v3.extend_from_slice(&3u16.to_be_bytes());
    v3.extend_from_slice(&[0; 32]);
    assert!(grd::parse(&v3).is_err());

    // Version 1, 6, etc.
    for version in [1u16, 4, 6, 0xFFFF] {
        let mut data = b"8BGR".to_vec();
        data.extend_from_slice(&version.to_be_bytes());
        data.extend_from_slice(&[0; 32]);
        assert!(grd::parse(&data).is_err(), "version {version} should be rejected");
    }
}

#[test]
fn parse_rejects_empty_gradient_list() {
    let data = grd::write(&[]);
    assert!(grd::parse(&data).is_err());
}

#[test]
fn write_and_parse_roundtrip_basic() {
    let original = sample();
    let bytes = grd::write(&original);
    let parsed = grd::parse(&bytes).expect("parse should succeed");
    assert_eq!(parsed, original);
}

#[test]
fn write_and_parse_roundtrip_noise_gradient() {
    let original = vec![gradient("Pure Noise", true, vec![], vec![])];
    let bytes = grd::write(&original);
    let parsed = grd::parse(&bytes).expect("parse should succeed");
    assert_eq!(parsed, original);
}

#[test]
fn write_and_parse_roundtrip_multiple_gradients() {
    // Use locations that are integer multiples of 1/4096 so the
    // integer-roundtrip in `write` is lossless.
    let mut gradients = sample();
    gradients.push(gradient("Third", false, vec![GrdStop { location: 0.5, midpoint: 0.5, color: GrdColor::Rgb([0.0, 0.0, 1.0]) }], vec![(0.25, 0.75, 0.5)]));
    let bytes = grd::write(&gradients);
    let parsed = grd::parse(&bytes).expect("parse should succeed");
    assert_eq!(parsed, gradients);
}

#[test]
fn write_is_deterministic() {
    let gradients = sample();
    let bytes1 = grd::write(&gradients);
    let bytes2 = grd::write(&gradients);
    assert_eq!(bytes1, bytes2);
}

#[test]
fn parse_truncations_do_not_panic() {
    let data = grd::write(&sample());
    for cut in 0..data.len() {
        let _ = grd::parse(&data[..cut]);
    }
}

#[test]
fn parse_corruptions_do_not_panic() {
    let data = grd::write(&sample());
    for i in 0..data.len() {
        let mut corrupted = data.clone();
        corrupted[i] ^= 0x5A;
        let _ = grd::parse(&corrupted);
    }
}

#[test]
fn parse_clamps_locations_to_unit_interval() {
    let stops = vec![
        GrdStop { location: -1.0, midpoint: 0.5, color: GrdColor::Rgb([1.0, 1.0, 1.0]) },
        GrdStop { location: 2.0, midpoint: 0.5, color: GrdColor::Rgb([0.0, 0.0, 0.0]) },
    ];
    let gradients = vec![gradient("Clamp Location", false, stops, vec![])];
    let bytes = grd::write(&gradients);
    let parsed = grd::parse(&bytes).expect("parse should succeed");
    assert_eq!(parsed[0].stops.len(), 2);
    assert_eq!(parsed[0].stops[0].location, 0.0);
    assert_eq!(parsed[0].stops[1].location, 1.0);
}

#[test]
fn parse_clamps_midpoints_to_unit_interval() {
    let stops = vec![
        GrdStop { location: 0.5, midpoint: -1.0, color: GrdColor::Rgb([0.5, 0.5, 0.5]) },
        GrdStop { location: 0.5, midpoint: 2.0, color: GrdColor::Rgb([0.5, 0.5, 0.5]) },
    ];
    let gradients = vec![gradient("Clamp Midpoint", false, stops, vec![])];
    let bytes = grd::write(&gradients);
    let parsed = grd::parse(&bytes).expect("parse should succeed");
    assert_eq!(parsed[0].stops[0].midpoint, 0.0);
    assert_eq!(parsed[0].stops[1].midpoint, 1.0);
}

#[test]
fn parse_clamps_opacity_to_unit_interval() {
    let opacity = vec![(-1.0, -1.0, 0.5), (2.0, 2.0, 0.5)];
    let gradients = vec![gradient("Clamp Opacity", false, vec![], opacity)];
    let bytes = grd::write(&gradients);
    let parsed = grd::parse(&bytes).expect("parse should succeed");
    assert_eq!(parsed[0].opacity.len(), 2);
    assert_eq!(parsed[0].opacity[0].0, 0.0); // location
    assert_eq!(parsed[0].opacity[0].1, 0.0); // opacity
    assert_eq!(parsed[0].opacity[1].0, 1.0);
    assert_eq!(parsed[0].opacity[1].1, 1.0);
}

#[test]
fn non_rgb_colors_are_written_as_black() {
    // The public write function only supports RGB, Foreground, Background.
    // Other colour models are encoded as RGB(0,0,0).
    let stops = vec![GrdStop { location: 0.5, midpoint: 0.5, color: GrdColor::Gray(0.5) }];
    let gradients = vec![gradient("Gray becomes black", false, stops, vec![])];
    let bytes = grd::write(&gradients);
    let parsed = grd::parse(&bytes).expect("parse should succeed");
    assert_eq!(parsed[0].stops[0].color, GrdColor::Rgb([0.0, 0.0, 0.0]));
}

#[test]
fn name_roundtrip_preserves_unicode() {
    let name = "Gradient − 日本 ";
    let gradients = vec![gradient(name, false, vec![], vec![])];
    let bytes = grd::write(&gradients);
    let parsed = grd::parse(&bytes).expect("parse should succeed");
    assert_eq!(parsed[0].name, name);
}

#[test]
fn roundtrip_with_extreme_floats_clamps_or_defaults() {
    // Stops:
    // - location/midpoint are converted to i32: NaN → 0, +∞ → i32::MAX, -∞ → i32::MIN.
    //   After parse/clamp: NaN → 0.0, +∞ → 1.0, -∞ → 0.0.
    // Opacity:
    // - location behaves like stops.
    // - opacity value is stored as UnitFloat, but `num` filters non‑finite values and
    //   falls back to 100.0, so NaN, +∞, -∞ all become 1.0 after division/clamp.
    // - midpoint behaves like stops.
    let stops = vec![
        GrdStop { location: f32::NAN, midpoint: f32::NAN, color: GrdColor::Rgb([0.0, 0.0, 0.0]) },
        GrdStop { location: f32::INFINITY, midpoint: f32::INFINITY, color: GrdColor::Rgb([1.0, 1.0, 1.0]) },
        GrdStop { location: f32::NEG_INFINITY, midpoint: f32::NEG_INFINITY, color: GrdColor::Rgb([0.5, 0.5, 0.5]) },
    ];
    let opacity =
        vec![(f32::NAN, f32::NAN, f32::NAN), (f32::INFINITY, f32::INFINITY, f32::INFINITY), (f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY)];
    let gradients = vec![gradient("Extreme floats", false, stops, opacity)];
    let bytes = grd::write(&gradients);
    let parsed = grd::parse(&bytes).expect("parse should succeed");

    // Stop assertions
    assert_eq!(parsed[0].stops[0].location, 0.0);
    assert_eq!(parsed[0].stops[0].midpoint, 0.0);
    assert_eq!(parsed[0].stops[1].location, 1.0);
    assert_eq!(parsed[0].stops[1].midpoint, 1.0);
    assert_eq!(parsed[0].stops[2].location, 0.0);
    assert_eq!(parsed[0].stops[2].midpoint, 0.0);

    // Opacity assertions
    assert_eq!(parsed[0].opacity[0].0, 0.0); // location (NaN → 0)
    assert_eq!(parsed[0].opacity[0].1, 1.0); // opacity value (NaN → default 100% → 1.0)
    assert_eq!(parsed[0].opacity[0].2, 0.0); // midpoint (NaN → 0)

    assert_eq!(parsed[0].opacity[1].0, 1.0); // location (+∞ → 1)
    assert_eq!(parsed[0].opacity[1].1, 1.0); // opacity value (+∞ → default 100% → 1.0)
    assert_eq!(parsed[0].opacity[1].2, 1.0); // midpoint (+∞ → 1)

    assert_eq!(parsed[0].opacity[2].0, 0.0); // location (-∞ → 0)
    assert_eq!(parsed[0].opacity[2].1, 1.0); // opacity value (-∞ → default 100% → 1.0)
    assert_eq!(parsed[0].opacity[2].2, 0.0); // midpoint (-∞ → 0)
}
