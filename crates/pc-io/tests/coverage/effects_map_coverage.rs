use photocraft_color::{BlendMode, Color};
use photocraft_doc::{Effect, FxCommon, FxPaint, Shadow, StrokeFx, StrokePosition};
use photocraft_io::effects_map::{parse_lfx2, parse_lrfx, write_lfx2};
use photocraft_psd::descriptor::VersionedDescriptor;

fn default_drop_shadow() -> Shadow {
    match Effect::default_drop_shadow() {
        Effect::DropShadow(s) => s,
        _ => panic!("expected drop shadow"),
    }
}

fn default_stroke_fx() -> StrokeFx {
    StrokeFx { common: FxCommon::new(BlendMode::Normal, 1.0), size: 3.0, position: StrokePosition::Outside, paint: FxPaint::Color(Color::BLACK) }
}

fn roundtrip_eq(a: &[Effect], b: &[Effect]) -> bool {
    write_lfx2(true, a) == write_lfx2(true, b)
}

#[test]
fn parse_lfx2_empty_input_returns_none() {
    assert_eq!(parse_lfx2(&[]), None);
    assert_eq!(parse_lfx2(&[0, 0]), None);
    assert_eq!(parse_lfx2(&[0, 0, 0, 0]), None);
}

#[test]
fn parse_lrfx_empty_input_returns_none() {
    assert_eq!(parse_lrfx(&[]), None);
    assert_eq!(parse_lrfx(&[0, 0]), None);
    assert_eq!(parse_lrfx(&[0, 0, 0, 1]), None);
}

#[test]
fn write_lfx2_empty_roundtrip_preserves_master_switch() {
    let data_true = write_lfx2(true, &[]);
    let (master_true, effects_true) = parse_lfx2(&data_true).unwrap();
    assert!(master_true);
    assert!(effects_true.is_empty());

    let data_false = write_lfx2(false, &[]);
    let (master_false, effects_false) = parse_lfx2(&data_false).unwrap();
    assert!(!master_false);
    assert!(effects_false.is_empty());
}

#[test]
fn single_instance_uses_single_key() {
    let fx = vec![Effect::default_drop_shadow(), Effect::Stroke(default_stroke_fx())];
    let data = write_lfx2(true, &fx);
    let (vd, _) = VersionedDescriptor::parse_prefix(&data[4..]).unwrap();
    assert!(vd.descriptor.get("DrSh").is_some());
    assert!(vd.descriptor.get("FrFX").is_some());
    assert!(vd.descriptor.get("dropShadowMulti").is_none());
    assert!(vd.descriptor.get("frameFXMulti").is_none());
}

#[test]
fn parse_lrfx_multiple_effects() {
    let mut d = vec![0, 0, 0, 2]; // version 0, count 2

    // First effect: drop shadow (dsdw)
    let mut rec1 = vec![0u8; 41];
    rec1[0..4].copy_from_slice(&2u32.to_be_bytes()); // version
    rec1[4..8].copy_from_slice(&(5u32 << 16).to_be_bytes()); // blur = 5.0
    rec1[8..12].copy_from_slice(&0u32.to_be_bytes()); // intensity
    rec1[12..16].copy_from_slice(&(45u32 << 16).to_be_bytes()); // angle = 45.0
    rec1[16..20].copy_from_slice(&(2u32 << 16).to_be_bytes()); // distance = 2.0
    // color: blue
    rec1[20] = 0;
    rec1[21] = 0; // space
    rec1[22] = 0;
    rec1[23] = 0; // red = 0
    rec1[24] = 0;
    rec1[25] = 0; // green = 0
    rec1[26] = 0xff;
    rec1[27] = 0xff; // blue = 1.0
    rec1[28] = 0;
    rec1[29] = 0; // alpha
    rec1[30..34].copy_from_slice(b"8BIM");
    rec1[34..38].copy_from_slice(b"scrn"); // Screen legacy key
    rec1[38] = 1; // enabled
    rec1[39] = 0; // use_global false
    rec1[40] = 200; // opacity = 200/255

    // Second effect: solid fill (sofi)
    // Layout:
    //  0..4   dummy/version (unused)
    //  4..8   dummy (unused)
    //  8..12  blend mode key ("Nrml" for Normal)
    // 12..22  color (space + RGB + alpha)
    // 22      opacity
    // 23      enabled
    let mut rec2 = vec![0u8; 24];
    rec2[0..4].copy_from_slice(&0u32.to_be_bytes()); // dummy/version
    // bytes 4..8 left as zero (unused)
    rec2[8..12].copy_from_slice(b"Nrml"); // Normal legacy key
    rec2[12] = 0;
    rec2[13] = 0; // space
    rec2[14] = 0xff;
    rec2[15] = 0xff; // red = 1.0
    rec2[16] = 0;
    rec2[17] = 0; // green = 0
    rec2[18] = 0;
    rec2[19] = 0; // blue = 0
    rec2[20] = 0;
    rec2[21] = 0; // alpha
    rec2[22] = 255; // opacity = 1.0
    rec2[23] = 1; // enabled

    let rec1_len = rec1.len() as u32;
    let rec2_len = rec2.len() as u32;

    d.extend_from_slice(b"8BIMdsdw");
    d.extend_from_slice(&rec1_len.to_be_bytes());
    d.extend(rec1);

    d.extend_from_slice(b"8BIMsofi");
    d.extend_from_slice(&rec2_len.to_be_bytes());
    d.extend(rec2);

    let (_, fx) = parse_lrfx(&d).unwrap();
    assert_eq!(fx.len(), 2);

    match &fx[0] {
        Effect::DropShadow(s) => {
            assert_eq!(s.size, 5.0);
            assert_eq!(s.distance, 2.0);
            assert_eq!(s.common.blend, BlendMode::Screen);
            assert!((s.common.opacity - 200.0 / 255.0).abs() < 1e-6);
            assert_eq!(s.color.c[2], 1.0);
            assert!(!s.use_global_light);
        }
        other => panic!("unexpected first effect: {other:?}"),
    }

    match &fx[1] {
        Effect::ColorOverlay { common, color } => {
            assert_eq!(common.blend, BlendMode::Normal);
            assert_eq!(common.opacity, 1.0);
            assert_eq!(color.c[0], 1.0);
        }
        other => panic!("unexpected second effect: {other:?}"),
    }
}

#[test]
fn parse_lrfx_truncated_returns_none() {
    // Valid header but missing record data
    let mut d = vec![0, 0, 0, 1];
    d.extend_from_slice(b"8BIMdsdw");
    d.extend_from_slice(&100u32.to_be_bytes()); // size larger than remaining
    d.extend_from_slice(&[0; 10]); // incomplete record
    assert_eq!(parse_lrfx(&d), None);

    // Truncated header
    let d2 = vec![0, 0, 0, 2, b'8', b'B', b'I'];
    assert_eq!(parse_lrfx(&d2), None);
}

#[test]
fn parse_lrfx_bad_signature_returns_none() {
    let mut d = vec![0, 0, 0, 1];
    d.extend_from_slice(b"XXXXdsdw");
    d.extend_from_slice(&0u32.to_be_bytes());
    assert_eq!(parse_lrfx(&d), None);
}

#[test]
fn nan_and_inf_floats_do_not_panic_and_roundtrip() {
    // Test with NaN and infinity in fields that go through unit floats.
    let shadow = Shadow { distance: f32::NAN, size: f32::INFINITY, spread: f32::NEG_INFINITY, ..default_drop_shadow() };
    let effect = Effect::DropShadow(shadow);
    let data = write_lfx2(true, std::slice::from_ref(&effect));
    let parsed = parse_lfx2(&data);
    assert!(parsed.is_some());
    let (_, effects) = parsed.unwrap();
    assert_eq!(effects.len(), 1);
    // Whether NaN/inf are preserved after conversion to f64 and back to f32 is not guaranteed,
    // but we only require no panic and the parser returns some effect.
    assert!(matches!(effects[0], Effect::DropShadow(_)));
}

#[test]
fn boundary_values_roundtrip() {
    let mut effect = Effect::default_drop_shadow();
    if let Effect::DropShadow(ref mut s) = effect {
        s.common.opacity = 0.0;
        s.distance = 0.0;
        s.spread = 0.0;
        s.size = 0.0;
        s.angle = 360.0;
        s.noise = 0.0;
    }
    let data = write_lfx2(true, std::slice::from_ref(&effect));
    let (_, parsed) = parse_lfx2(&data).unwrap();
    assert!(roundtrip_eq(std::slice::from_ref(&effect), &parsed));

    // Max opacity
    let mut max_effect = Effect::default_drop_shadow();
    if let Effect::DropShadow(ref mut s) = max_effect {
        s.common.opacity = 1.0;
        s.distance = 1e6;
        s.size = 1e6;
        s.spread = 1e6;
        s.noise = 1.0;
    }
    let data_max = write_lfx2(true, std::slice::from_ref(&max_effect));
    let (_, parsed_max) = parse_lfx2(&data_max).unwrap();
    assert!(roundtrip_eq(std::slice::from_ref(&max_effect), &parsed_max));
}
