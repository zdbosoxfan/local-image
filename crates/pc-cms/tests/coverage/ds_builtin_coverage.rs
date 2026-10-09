use photocraft_cms::builtin::{Builtin, COATED_CMYK_ICC, COPYRIGHT, default_for, srgb};
use photocraft_cms::{CmsError, ColorSpace, Profile};

#[test]
fn all_variants_count_is_ten() {
    assert_eq!(Builtin::ALL.len(), 10);
}

#[test]
fn all_variants_have_unique_ids() {
    let ids: std::collections::HashSet<_> = Builtin::ALL.iter().map(|b| b.id()).collect();
    assert_eq!(ids.len(), 10);
}

#[test]
fn all_variants_have_nonempty_id_and_description() {
    for b in Builtin::ALL.iter() {
        assert!(!b.id().is_empty(), "empty id for {b:?}");
        assert!(!b.description().is_empty(), "empty description for {b:?}");
    }
}

#[test]
fn id_round_trips() {
    for b in Builtin::ALL.iter() {
        assert_eq!(Builtin::from_id(b.id()), Some(*b), "id round trip failed for {b:?}");
    }
}

#[test]
fn description_lookup_is_case_insensitive_exact() {
    for b in Builtin::ALL.iter() {
        assert_eq!(Builtin::from_id(b.description()), Some(*b), "description lookup failed for {b:?}");
    }
}

#[test]
fn all_variants_are_declared_in_order() {
    assert_eq!(Builtin::ALL[0], Builtin::Srgb);
    assert_eq!(Builtin::ALL[1], Builtin::DisplayP3);
    assert_eq!(Builtin::ALL[2], Builtin::AdobeRgbCompat);
    assert_eq!(Builtin::ALL[3], Builtin::ProPhotoCompat);
    assert_eq!(Builtin::ALL[4], Builtin::LinearSrgb);
    assert_eq!(Builtin::ALL[5], Builtin::Rec2020);
    assert_eq!(Builtin::ALL[6], Builtin::GrayGamma22);
    assert_eq!(Builtin::ALL[7], Builtin::SGray);
    assert_eq!(Builtin::ALL[8], Builtin::LabD50);
    assert_eq!(Builtin::ALL[9], Builtin::CoatedCmyk);
}

#[test]
fn profile_returns_same_static_reference() {
    for b in Builtin::ALL.iter() {
        let p1 = b.profile();
        let p2 = b.profile();
        assert!(std::ptr::eq(p1 as *const Profile, p2 as *const Profile), "profile reference changed for {b:?}");
    }
}

#[test]
fn profiles_are_distinct_objects() {
    for i in 0..Builtin::ALL.len() {
        for j in (i + 1)..Builtin::ALL.len() {
            let a = Builtin::ALL[i].profile();
            let b = Builtin::ALL[j].profile();
            assert!(!std::ptr::eq(a as *const Profile, b as *const Profile), "profiles {i} and {j} are the same object");
        }
    }
}

#[test]
fn srgb_fn_returns_srgb_builtin_profile() {
    assert!(std::ptr::eq(srgb() as *const Profile, Builtin::Srgb.profile() as *const Profile));
}

#[test]
fn default_for_rgb_is_srgb() {
    match default_for(ColorSpace::Rgb) {
        Some(p) => assert!(std::ptr::eq(p as *const Profile, Builtin::Srgb.profile() as *const Profile)),
        None => panic!("default_for(Rgb) returned None"),
    }
}

#[test]
fn default_for_gray_is_sgray() {
    match default_for(ColorSpace::Gray) {
        Some(p) => assert!(std::ptr::eq(p as *const Profile, Builtin::SGray.profile() as *const Profile)),
        None => panic!("default_for(Gray) returned None"),
    }
}

#[test]
fn default_for_cmyk_is_coated_cmyk() {
    match default_for(ColorSpace::Cmyk) {
        Some(p) => assert!(std::ptr::eq(p as *const Profile, Builtin::CoatedCmyk.profile() as *const Profile)),
        None => panic!("default_for(Cmyk) returned None"),
    }
}

#[test]
fn default_for_lab_is_lab_d50() {
    match default_for(ColorSpace::Lab) {
        Some(p) => assert!(std::ptr::eq(p as *const Profile, Builtin::LabD50.profile() as *const Profile)),
        None => panic!("default_for(Lab) returned None"),
    }
}

#[test]
fn from_id_accepts_common_aliases_case_insensitive() {
    let cases = [
        ("srgb", Builtin::Srgb),
        ("sRGB IEC61966-2.1", Builtin::Srgb),
        ("p3", Builtin::DisplayP3),
        ("display-p3", Builtin::DisplayP3),
        ("DisplayP3", Builtin::DisplayP3),
        ("adobergb", Builtin::AdobeRgbCompat),
        ("adobe-rgb-compat", Builtin::AdobeRgbCompat),
        ("ADOBERGB1998", Builtin::AdobeRgbCompat),
        ("PROPHOTORGB", Builtin::ProPhotoCompat),
        ("linear-srgb", Builtin::LinearSrgb),
        ("rec2020", Builtin::Rec2020),
        ("BT.2020", Builtin::Rec2020),
        ("gray", Builtin::GrayGamma22),
        ("sgray", Builtin::SGray),
        ("lab", Builtin::LabD50),
        ("cmyk", Builtin::CoatedCmyk),
        ("coated-cmyk", Builtin::CoatedCmyk),
        ("photocraft-coated-cmyk", Builtin::CoatedCmyk),
    ];
    for (input, expected) in cases {
        assert_eq!(Builtin::from_id(input), Some(expected), "input {input:?} failed");
    }
}

#[test]
fn from_id_normalizes_away_punctuation_and_spaces() {
    assert_eq!(Builtin::from_id("display p3"), Some(Builtin::DisplayP3));
    assert_eq!(Builtin::from_id("Display-P3"), Some(Builtin::DisplayP3));
    assert_eq!(Builtin::from_id("rec. 2020"), Some(Builtin::Rec2020));
    assert_eq!(Builtin::from_id("BT.2020"), Some(Builtin::Rec2020));
    assert_eq!(Builtin::from_id("adobe_RGB_compat"), Some(Builtin::AdobeRgbCompat));
    assert_eq!(Builtin::from_id("srgb "), Some(Builtin::Srgb));
}

#[test]
fn from_id_unknown_returns_none() {
    assert_eq!(Builtin::from_id(""), None);
    assert_eq!(Builtin::from_id("unknown"), None);
    assert_eq!(Builtin::from_id("random-profile"), None);
    assert_eq!(Builtin::from_id("not-a-builtin"), None);
    assert_eq!(Builtin::from_id("123"), None);
}

#[test]
fn copyright_tag_is_cc0() {
    assert!(COPYRIGHT.contains("CC0-1.0"));
}

#[test]
fn parse_empty_input_yields_truncated() {
    let result = Profile::parse(&[]);
    assert!(matches!(result, Err(CmsError::Truncated)));
}

#[test]
fn parse_bad_signature_yields_bad_signature() {
    // Take a valid built-in profile and replace the 'acsp' signature at offset 36.
    let mut data = COATED_CMYK_ICC.to_vec();
    data[36..40].copy_from_slice(b"nope");
    let result = Profile::parse(&data);
    assert!(matches!(result, Err(CmsError::BadSignature)));
}

#[test]
fn parse_truncated_header_yields_truncated() {
    // Less than the 128-byte ICC header.
    let truncated = &COATED_CMYK_ICC[..100];
    let result = Profile::parse(truncated);
    assert!(matches!(result, Err(CmsError::Truncated)));
}

#[test]
fn coated_cmyk_const_has_icc_signature() {
    assert!(COATED_CMYK_ICC.len() >= 128);
    assert_eq!(&COATED_CMYK_ICC[36..40], b"acsp", "COATED_CMYK_ICC does not have the ICC 'acsp' signature at offset 36");
}
