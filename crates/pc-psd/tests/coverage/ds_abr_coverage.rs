use photocraft_psd::Descriptor;
use photocraft_psd::abr::{AbrSample, LegacyBrush, LegacyTip, MAX_EDGE, parse, write_v6, write_v12};

fn sample(id: &str, width: u32, height: u32, depth: u16) -> AbrSample {
    let bpp = usize::from(depth / 8);
    let len = (width as usize).checked_mul(height as usize).unwrap().checked_mul(bpp).unwrap();
    let data = (0..len).map(|i| (i * 37 % 251) as u8).collect();
    AbrSample { id: id.to_string(), width, height, depth, data }
}

fn computed_brush(diameter: u16, hardness: u16, angle: i16, roundness: u16, spacing: u16) -> LegacyBrush {
    LegacyBrush { name: String::new(), spacing, anti_alias: true, tip: LegacyTip::Computed { diameter, hardness, angle, roundness } }
}

fn sampled_brush(name: &str, spacing: u16, anti_alias: bool, sample: AbrSample) -> LegacyBrush {
    LegacyBrush { name: name.to_string(), spacing, anti_alias, tip: LegacyTip::Sampled(sample) }
}

#[test]
fn parse_empty_and_truncated_version_errors() {
    assert!(parse(&[]).is_err());
    assert!(parse(&[0]).is_err());
    assert!(parse(&[0, 0]).is_err());
}

#[test]
fn parse_unknown_version_errors() {
    for v in [0u16, 5, 11, u16::MAX] {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&v.to_be_bytes());
        assert!(parse(&bytes).is_err(), "version {v} should be unsupported");
    }
}

#[test]
fn v1_computed_round_trip() {
    let brush = computed_brush(19, 80, -30, 50, 25);
    let bytes = write_v12(1, std::slice::from_ref(&brush), false).unwrap();
    let parsed = parse(&bytes).unwrap();
    assert_eq!(parsed.version, 1);
    assert_eq!(parsed.subversion, 0);
    assert_eq!(parsed.legacy, vec![brush]);
    assert!(parsed.samples.is_empty());
    assert!(parsed.warnings.is_empty());
}

#[test]
fn v2_sampled_8bit_round_trip_raw_and_rle() {
    for rle in [false, true] {
        let s = sample("", 13, 9, 8);
        let brush = sampled_brush("Leaf ✓", 40, false, s);
        let bytes = write_v12(2, std::slice::from_ref(&brush), rle).unwrap();
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.legacy, vec![brush], "rle={rle}");
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    }
}

#[test]
fn v2_sampled_16bit_round_trip_raw_and_rle() {
    for rle in [false, true] {
        // v1/v2 do not store per-sample IDs; the parser always returns an empty ID.
        let s = sample("", 5, 4, 16);
        let brush = sampled_brush("Deep", 33, true, s);
        let bytes = write_v12(2, std::slice::from_ref(&brush), rle).unwrap();
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.legacy, vec![brush], "rle={rle}");
        assert!(parsed.warnings.is_empty());
    }
}

#[test]
fn v1_sample_name_is_ignored() {
    let s = sample("", 4, 4, 8);
    let brush = sampled_brush("ignored", 10, true, s);
    let bytes = write_v12(1, std::slice::from_ref(&brush), false).unwrap();
    let parsed = parse(&bytes).unwrap();
    assert_eq!(parsed.legacy[0].name, "");
}

#[test]
fn sample_to_unit_8bit_maps_0_255() {
    let data = vec![0, 51, 102, 153, 204, 255];
    let s = AbrSample { id: "x".into(), width: 3, height: 2, depth: 8, data };
    let units = s.to_unit();
    assert_eq!(units.len(), 6);
    for (i, &u) in units.iter().enumerate() {
        assert!(u.is_finite());
        assert!((0.0..=1.0).contains(&u));
        assert!((u - s.data[i] as f32 / 255.0).abs() < f32::EPSILON);
    }
}

#[test]
fn sample_to_unit_16bit_big_endian() {
    let data = vec![0x00, 0x00, 0x80, 0x00, 0xff, 0xff];
    let s = AbrSample { id: "y".into(), width: 3, height: 1, depth: 16, data };
    let units = s.to_unit();
    assert_eq!(units.len(), 3);
    assert!(units[0].abs() < f32::EPSILON);
    assert!((units[1] - 0x8000 as f32 / 65535.0).abs() < 1e-6);
    assert!((units[2] - 1.0).abs() < f32::EPSILON);
    for u in &units {
        assert!(u.is_finite());
        assert!((0.0..=1.0).contains(u));
    }
}

#[test]
fn v6_round_trip_empty_presets() {
    for sub in [1u16, 2] {
        for rle in [false, true] {
            let samples = vec![sample("$a", 20, 11, 8), sample("$b", 5, 5, 16)];
            let bytes = write_v6(sub, &samples, &[], &[], rle).unwrap();
            let parsed = parse(&bytes).unwrap();
            assert_eq!((parsed.version, parsed.subversion), (6, sub));
            assert_eq!(parsed.samples, samples, "sub={sub} rle={rle}");
            assert!(parsed.presets.is_empty());
            assert!(parsed.patterns.is_empty());
            assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        }
    }
}

#[test]
fn v6_round_trip_with_preset_descriptor() {
    let s = sample("$id", 10, 10, 8);
    let preset = Descriptor::new("brushPreset");
    let bytes = write_v6(1, std::slice::from_ref(&s), &[], std::slice::from_ref(&preset), false).unwrap();
    let parsed = parse(&bytes).unwrap();
    assert_eq!(parsed.samples, vec![s]);
    assert_eq!(parsed.presets, vec![preset]);
    assert!(parsed.warnings.is_empty());
}

#[test]
fn v6_with_no_brushes_errors() {
    let bytes = write_v6(1, &[], &[], &[], false).unwrap();
    assert!(parse(&bytes).is_err());
}

#[test]
fn zero_brushes_v12_file_is_not_readable() {
    let bytes = write_v12(1, &[], false).unwrap();
    assert!(parse(&bytes).is_err());
}

#[test]
fn write_v12_rejects_invalid_version() {
    let brush = computed_brush(10, 50, 0, 100, 25);
    for v in [0u16, 3, 5, 6, u16::MAX] {
        assert!(write_v12(v, std::slice::from_ref(&brush), false).is_err(), "version {v} should be rejected");
    }
}

#[test]
fn write_v6_rejects_invalid_subversion() {
    assert!(write_v6(0, &[], &[], &[], false).is_err());
    assert!(write_v6(3, &[], &[], &[], false).is_err());
}

#[test]
fn write_v12_rejects_sample_exceeding_max_edge() {
    let mut s = sample("", 2, 2, 8);
    s.width = MAX_EDGE + 1;
    let brush = sampled_brush("", 1, true, s);
    assert!(write_v12(1, &[brush], false).is_err());
}

#[test]
fn write_v6_rejects_invalid_sample_depth_or_dimensions() {
    let mut s = sample("$a", 2, 2, 8);
    s.depth = 9;
    assert!(write_v6(1, &[s], &[], &[], false).is_err());

    let s2 = sample("$b", 0, 2, 8);
    assert!(write_v6(1, &[s2], &[], &[], false).is_err());

    let s3 = sample("$c", 2, 0, 8);
    assert!(write_v6(1, &[s3], &[], &[], false).is_err());
}

#[test]
fn parse_truncated_round_trip_files_does_not_panic() {
    let v1_bytes = write_v12(1, &[computed_brush(15, 70, 45, 60, 20)], false).unwrap();
    let v2_bytes = write_v12(2, &[sampled_brush("x", 20, true, sample("", 7, 7, 8))], true).unwrap();
    let v6_bytes = write_v6(2, &[sample("$a", 30, 30, 8)], &[], &[Descriptor::new("brushPreset")], true).unwrap();

    for bytes in [&v1_bytes, &v2_bytes, &v6_bytes] {
        for cut in 0..=bytes.len() {
            let _ = parse(&bytes[..cut]);
        }
    }
}

#[test]
fn parse_corrupted_v6_no_panic() {
    let good = write_v6(1, &[sample("$a", 8, 8, 8)], &[], &[], true).unwrap();
    for i in 0..good.len() {
        let mut b = good.clone();
        b[i] ^= 0x5a;
        let _ = parse(&b);
    }
}
