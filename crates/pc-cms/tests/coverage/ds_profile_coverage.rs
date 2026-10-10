use photocraft_cms::math::D50;
use photocraft_cms::profile::{ColorSpace, Lut, LutKind, Pcs, Profile, ProfileClass};
use photocraft_cms::{Builtin, CmsError, Curve, Intent};

fn base_srgb() -> Profile {
    Builtin::Srgb.profile().clone()
}

fn make_gray_profile() -> Profile {
    let mut p = base_srgb();
    p.color_space = ColorSpace::Gray;
    p.gray_trc = Some(Curve::Gamma(2.2));
    p.trc = None;
    p.matrix = None;
    p.a2b = Default::default();
    p.b2a = Default::default();
    p
}

#[test]
fn parse_empty_returns_truncated() {
    let err = Profile::parse(&[]).unwrap_err();
    assert_eq!(err, CmsError::Truncated);
}

#[test]
fn parse_short_returns_truncated() {
    let bytes = vec![0u8; 100];
    let err = Profile::parse(&bytes).unwrap_err();
    assert_eq!(err, CmsError::Truncated);
}

#[test]
fn parse_bad_signature_returns_bad_signature() {
    let bytes = vec![0u8; 132];
    let err = Profile::parse(&bytes).unwrap_err();
    assert_eq!(err, CmsError::BadSignature);
}

#[test]
fn parse_tag_count_too_large_returns_invalid() {
    let mut bytes = vec![0u8; 132];
    bytes[0..4].copy_from_slice(&132u32.to_be_bytes());
    bytes[36..40].copy_from_slice(b"acsp");
    bytes[128..132].copy_from_slice(&1025u32.to_be_bytes());
    let err = Profile::parse(&bytes).unwrap_err();
    assert!(matches!(err, CmsError::Invalid(_)));
}

#[test]
fn parse_truncated_tag_data_returns_truncated() {
    let mut bytes = vec![0u8; 144];
    bytes[0..4].copy_from_slice(&144u32.to_be_bytes());
    bytes[36..40].copy_from_slice(b"acsp");
    bytes[128..132].copy_from_slice(&1u32.to_be_bytes());
    bytes[132..136].copy_from_slice(b"desc");
    bytes[136..140].copy_from_slice(&200u32.to_be_bytes()); // offset out of range
    bytes[140..144].copy_from_slice(&10u32.to_be_bytes());
    let err = Profile::parse(&bytes).unwrap_err();
    assert_eq!(err, CmsError::Truncated);
}

#[test]
fn builtin_srgb_parse_roundtrip() {
    let original = base_srgb();
    let bytes = original.to_bytes();
    let parsed = Profile::parse(&bytes).unwrap();
    assert_eq!(parsed.content_hash(), original.content_hash());
    assert_eq!(parsed.color_space, ColorSpace::Rgb);
    assert_eq!(parsed.channels(), 3);
}

#[test]
fn to_bytes_roundtrip_preserves_description() {
    let mut modified = base_srgb();
    modified.description = "Roundtrip description".to_string();
    let modified = modified.with_encoded_bytes();
    let bytes = modified.to_bytes();
    let reparsed = Profile::parse(&bytes).unwrap();
    assert_eq!(reparsed.description, "Roundtrip description");
}

#[test]
fn content_hash_changes_with_description() {
    let base = base_srgb();
    let base_hash = base.content_hash();
    let mut modified = base.clone();
    modified.description = "changed".to_string();
    let modified = modified.with_encoded_bytes();
    assert_ne!(modified.content_hash(), base_hash);
}

#[test]
fn channels_reports_color_space_channels() {
    let mut p = base_srgb();
    p.color_space = ColorSpace::Gray;
    assert_eq!(p.channels(), 1);
    p.color_space = ColorSpace::Cmyk;
    assert_eq!(p.channels(), 4);
    p.color_space = ColorSpace::Rgb;
    assert_eq!(p.channels(), 3);
    p.color_space = ColorSpace::Color(7);
    assert_eq!(p.channels(), 7);
}

#[test]
fn same_colors_self_true() {
    let p = base_srgb();
    assert!(p.same_colors(&p));
}

#[test]
fn same_colors_true_despite_different_description() {
    let base = base_srgb();
    let mut modified = base.clone();
    modified.description = "Different description".to_string();
    let modified = modified.with_encoded_bytes();
    assert_ne!(modified.content_hash(), base.content_hash());
    assert!(base.same_colors(&modified));
}

#[test]
fn same_colors_false_when_matrix_changed() {
    let base = base_srgb();
    let mut changed = base.clone();
    if let Some(m) = &mut changed.matrix {
        // Shift red primary X by 2% – should exceed SAME_COLORS_MAX_DELTA_E
        m[0][0] *= 1.02;
    }
    let changed = changed.with_encoded_bytes();
    assert_ne!(changed.content_hash(), base.content_hash());
    assert!(!base.same_colors(&changed));
}

#[test]
fn media_white_v2_display_returns_d50() {
    let mut p = base_srgb();
    p.version = (2, 0x10);
    p.class = ProfileClass::Display;
    p.white_point = [0.9, 1.0, 1.1]; // not D50
    assert_eq!(p.media_white(), D50);
}

#[test]
fn media_white_v4_returns_white_point() {
    let mut p = base_srgb();
    p.version = (4, 0x30);
    p.class = ProfileClass::Display;
    p.white_point = [0.8, 0.9, 1.0];
    assert_eq!(p.media_white(), [0.8, 0.9, 1.0]);
}

#[test]
fn gray_as_rgb_returns_rgb_view() {
    let gray = make_gray_profile();
    let rgb = gray.gray_as_rgb().expect("gray_as_rgb should succeed");
    assert_eq!(rgb.color_space, ColorSpace::Rgb);
    assert!(rgb.gray_trc.is_none());
    assert!(rgb.matrix.is_some());
    assert!(rgb.trc.is_some());
    let trc = rgb.trc.as_ref().unwrap();
    assert_eq!(trc.len(), 3);
    assert_eq!(trc[0], trc[1]);
    assert_eq!(trc[1], trc[2]);
    assert_eq!(trc[0], Curve::Gamma(2.2));
}

#[test]
fn gray_as_rgb_requires_gray_trc_and_xyz() {
    let base = base_srgb();
    assert!(base.gray_as_rgb().is_none()); // no gray_trc

    let mut gray_lab = make_gray_profile();
    gray_lab.pcs = Pcs::Lab;
    assert!(gray_lab.gray_as_rgb().is_none()); // pcs not Xyz
}

#[test]
fn is_matrix_shaper_true_for_matrix_profiles() {
    let srgb = base_srgb();
    assert!(srgb.is_matrix_shaper());

    let gray = make_gray_profile();
    assert!(gray.is_matrix_shaper());
}

#[test]
fn is_matrix_shaper_false_with_lut() {
    let mut p = base_srgb();
    p.a2b[0] = Some(Lut { kind: LutKind::Lut8, inputs: 3, outputs: 3, stages: vec![] });
    assert!(!p.is_matrix_shaper());
}

#[test]
fn supports_intent_all_true_for_matrix_profile() {
    let p = base_srgb();
    for intent in Intent::ALL {
        assert!(p.supports_intent(intent, true));
        assert!(p.supports_intent(intent, false));
    }
}

#[test]
fn supports_intent_false_for_missing_lut() {
    let mut p = base_srgb();
    p.a2b[0] = None;
    p.a2b[1] = Some(Lut { kind: LutKind::Lut8, inputs: 3, outputs: 3, stages: vec![] });
    p.a2b[2] = None;
    assert!(p.supports_intent(Intent::RelativeColorimetric, true));
    assert!(!p.supports_intent(Intent::Perceptual, true));
    assert!(!p.supports_intent(Intent::Saturation, true));
}

#[test]
fn device_to_pcs_srgb_ok() {
    let p = base_srgb();
    let (stages, pcs) = p.device_to_pcs(Intent::RelativeColorimetric).unwrap();
    assert_eq!(pcs, Pcs::Xyz);
    assert!(!stages.is_empty());
}

#[test]
fn pcs_to_device_srgb_ok() {
    let p = base_srgb();
    let (stages, pcs) = p.pcs_to_device(Intent::RelativeColorimetric).unwrap();
    assert_eq!(pcs, Pcs::Xyz);
    assert!(!stages.is_empty());
}

#[test]
fn device_to_pcs_unsupported_cmyk_returns_err() {
    let mut p = base_srgb();
    p.color_space = ColorSpace::Cmyk;
    p.matrix = None;
    p.trc = None;
    p.gray_trc = None;
    p.a2b = Default::default();
    p.b2a = Default::default();
    let res = p.device_to_pcs(Intent::RelativeColorimetric);
    assert!(matches!(res, Err(CmsError::Unsupported(_))));
}

#[test]
fn profile_class_sig_roundtrip() {
    let classes = [
        ProfileClass::Input,
        ProfileClass::Display,
        ProfileClass::Output,
        ProfileClass::DeviceLink,
        ProfileClass::ColorSpace,
        ProfileClass::Abstract,
        ProfileClass::NamedColor,
        ProfileClass::Unknown(0xdeadbeef),
    ];
    for c in classes {
        assert_eq!(ProfileClass::from_sig(c.sig()), c);
    }
}

#[test]
fn color_space_sig_roundtrip() {
    let spaces = [
        ColorSpace::Xyz,
        ColorSpace::Lab,
        ColorSpace::Luv,
        ColorSpace::YCbCr,
        ColorSpace::Yxy,
        ColorSpace::Rgb,
        ColorSpace::Gray,
        ColorSpace::Hsv,
        ColorSpace::Hls,
        ColorSpace::Cmyk,
        ColorSpace::Cmy,
        ColorSpace::Color(3),
        ColorSpace::Color(12),
        ColorSpace::Unknown(0x12345678),
    ];
    for s in spaces {
        assert_eq!(ColorSpace::from_sig(s.sig()), s);
    }
}
