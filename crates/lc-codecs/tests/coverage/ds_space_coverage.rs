use lightcraft_codecs::space::{NamedSpace, SourceSpace, SpaceOrigin, Trc, rgb_to_xyz_d50};

#[test]
fn named_space_recognition_and_xyz_d50_consistency() {
    for n in NamedSpace::ALL {
        let m = n.to_xyz_d50();
        assert_eq!(NamedSpace::recognize(&m), Some(n));
        assert_eq!(rgb_to_xyz_d50(&n.rgb_space()), m);
    }
}

#[test]
fn source_space_to_working_preserves_white() {
    for n in NamedSpace::ALL {
        let s = SourceSpace::named(n, SpaceOrigin::Container);
        let w = s.to_working().apply([1.0, 1.0, 1.0]);
        for c in w {
            assert!((c - 1.0).abs() < 1e-5, "{}", n.name());
        }
    }
}

#[test]
fn source_space_to_same_space_is_identity() {
    for n in NamedSpace::ALL {
        let s = SourceSpace::named(n, SpaceOrigin::Container);
        let dst = n.rgb_space();
        let v = [0.25, 0.5, 0.75];
        assert_eq!(s.to_space(&dst).apply(v), v);
    }
}

#[test]
fn source_space_is_working_only_rec2020() {
    for n in NamedSpace::ALL {
        let s = SourceSpace::named(n, SpaceOrigin::Container);
        assert_eq!(s.is_working(), n == NamedSpace::Rec2020);
    }
}

#[test]
fn source_space_named_sets_expected_fields() {
    for n in NamedSpace::ALL {
        let s = SourceSpace::named(n, SpaceOrigin::Container);
        assert_eq!(s.named, Some(n));
        assert_eq!(s.to_xyz_d50, n.to_xyz_d50());
        let t = n.trc();
        assert_eq!(s.trc, Some([t.clone(), t.clone(), t]));
    }
}

#[test]
fn trc_linear_is_identity() {
    let c = Trc::Linear;
    for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
        assert_eq!(c.to_linear(x), x);
        assert_eq!(c.from_linear(x), x);
    }
    assert!(c.is_linear());
}

#[test]
fn trc_gamma_power_law_and_roundtrip() {
    let c = Trc::Gamma(2.2);
    assert!((c.to_linear(0.5) - 0.5f32.powf(2.2)).abs() < 1e-6);
    for x in [0.0, 0.1, 0.3, 0.5, 0.8, 1.0] {
        let y = c.to_linear(x);
        let x2 = c.from_linear(y);
        assert!((x2 - x).abs() < 1e-5, "{x} -> {y} -> {x2}");
    }
    assert!(!c.is_linear());
}

#[test]
fn trc_srgb_known_values_and_negative_mirror() {
    let c = Trc::Srgb;
    assert!((c.to_linear(0.04045) - 0.0031308).abs() < 1e-6);
    assert_eq!(c.to_linear(0.0), 0.0);
    assert_eq!(c.to_linear(1.0), 1.0);
    let x = 0.5;
    assert_eq!(c.to_linear(-x), -c.to_linear(x));
}

#[test]
fn trc_rec709_threshold_and_negative_mirror() {
    let c = Trc::Rec709;
    // Below threshold: v/4.5
    let low: f32 = 0.08;
    assert!((c.to_linear(low) - low / 4.5).abs() < 1e-6);
    // Above threshold: ((v+0.099)/1.099)^(1/0.45)
    let high: f32 = 0.1;
    let expected: f32 = ((high + 0.099f32) / 1.099f32).powf(1.0f32 / 0.45f32);
    assert!((c.to_linear(high) - expected).abs() < 1e-6);
    assert_eq!(c.to_linear(0.0), 0.0);
    assert_eq!(c.to_linear(1.0), 1.0);
    let x = 0.3;
    assert_eq!(c.to_linear(-x), -c.to_linear(x));
}

#[test]
fn trc_parametric_identity_variants() {
    // Empty parametric evaluates as identity, though is_linear does not classify it as such.
    let empty = Trc::Parametric(Vec::new());
    for x in [0.0, 0.5, 1.0] {
        assert!((empty.to_linear(x) - x).abs() < 1e-6);
        assert!((empty.from_linear(x) - x).abs() < 1e-6);
    }

    let one = Trc::Parametric(vec![1.0]);
    for x in [0.0, 0.5, 1.0] {
        assert!((one.to_linear(x) - x).abs() < 1e-6);
        assert!((one.from_linear(x) - x).abs() < 1e-6);
    }
    assert!(one.is_linear());

    let pow = Trc::Parametric(vec![2.0]);
    assert!((pow.to_linear(0.5) - 0.25).abs() < 1e-6);
    assert!(!pow.is_linear());
}

#[test]
fn trc_table_empty_and_single_entry() {
    let empty = Trc::Table(Vec::<u16>::new());
    for x in [0.0, 0.5, 1.0] {
        assert!((empty.to_linear(x) - x).abs() < 1e-6);
        assert!((empty.from_linear(x) - x).abs() < 1e-6);
    }
    assert!(empty.is_linear());

    // Single entry 256 means exponent 1.0, so identity.
    let ident = Trc::Table(vec![256u16]);
    for x in [0.0, 0.5, 1.0] {
        assert!((ident.to_linear(x) - x).abs() < 1e-6);
    }
    assert!(!ident.is_linear());

    // Single entry 512 means exponent 2.0.
    let pow = Trc::Table(vec![512u16]);
    assert!((pow.to_linear(0.5) - 0.25).abs() < 1e-6);
}

#[test]
fn trc_table_interpolation_odd_sizes() {
    // Size 2: linear ramp.
    let t2 = Trc::Table(vec![0u16, 65535]);
    assert!((t2.to_linear(0.0) - 0.0).abs() < 1e-6);
    assert!((t2.to_linear(0.5) - 0.5).abs() < 1e-6);
    assert!((t2.to_linear(1.0) - 1.0).abs() < 1e-6);

    // Size 3: values 0, 32768, 65535.
    let t3 = Trc::Table(vec![0u16, 32768, 65535]);
    assert!((t3.to_linear(0.0) - 0.0).abs() < 1e-6);
    assert!((t3.to_linear(0.25) - 0.25).abs() < 1e-5);
    assert!((t3.to_linear(0.5) - 0.5).abs() < 1e-5);
    assert!((t3.to_linear(0.75) - 0.75).abs() < 1e-5);
    assert!((t3.to_linear(1.0) - 1.0).abs() < 1e-6);
}

#[test]
fn trc_is_linear_detects_identity() {
    assert!(Trc::Linear.is_linear());
    assert!(Trc::Gamma(1.0).is_linear());
    assert!(Trc::Gamma(1.0 + 1e-5).is_linear());
    assert!(Trc::Parametric(vec![1.0]).is_linear());
    assert!(Trc::Table(Vec::<u16>::new()).is_linear());

    assert!(!Trc::Srgb.is_linear());
    assert!(!Trc::Gamma(2.2).is_linear());
    assert!(!Trc::Rec709.is_linear());
    assert!(!Trc::Parametric(vec![2.0]).is_linear());
    assert!(!Trc::Table(vec![0u16, 65535]).is_linear());
}

#[test]
fn trc_approx_eq_srgb_parametric() {
    let srgb = Trc::Srgb;
    let param = Trc::Parametric(vec![2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045]);
    assert!(srgb.approx_eq(&param, 1e-5));
    assert!(!srgb.approx_eq(&Trc::Gamma(2.2), 1e-3));
}

#[test]
fn trc_inverse_roundtrip_various_curves() {
    let curves = [
        Trc::Srgb,
        Trc::Gamma(2.2),
        Trc::Rec709,
        Trc::Parametric(vec![2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045]),
        Trc::Table(vec![0u16, 1000, 30000, 65535]),
        Trc::Table(vec![0u16, 65535]),
        Trc::Table(vec![256u16]),
    ];
    for c in curves {
        for i in 0..=20 {
            let x = i as f32 / 20.0;
            let y = c.to_linear(x);
            let x2 = c.from_linear(y);
            assert!((x2 - x).abs() < 1e-3, "curve {:?}, x {x}, got {x2}", c);
        }
    }
}

#[test]
fn trc_to_linear_nan_and_infinity() {
    let curves = [Trc::Linear, Trc::Srgb, Trc::Gamma(2.2), Trc::Rec709];
    for c in curves.iter() {
        assert!(c.to_linear(f32::NAN).is_nan());
        assert!(c.to_linear(f32::INFINITY).is_infinite());
        assert!(c.to_linear(f32::NEG_INFINITY).is_infinite());
    }
}

#[test]
fn trc_from_linear_does_not_panic_on_special_values() {
    let curves = [Trc::Linear, Trc::Srgb, Trc::Gamma(2.2), Trc::Rec709, Trc::Parametric(vec![2.4]), Trc::Table(vec![0u16, 65535])];
    for c in curves.iter() {
        let _ = c.from_linear(f32::NAN);
        let _ = c.from_linear(f32::INFINITY);
        let _ = c.from_linear(f32::NEG_INFINITY);
    }
}

#[test]
fn space_origin_is_copy_and_eq() {
    let o = SpaceOrigin::Container;
    let o2 = o;
    assert_eq!(o, o2);
    assert_ne!(o, SpaceOrigin::Untagged);
}

#[test]
fn named_space_trc_matches_expected() {
    assert_eq!(NamedSpace::Srgb.trc(), Trc::Srgb);
    assert_eq!(NamedSpace::DisplayP3.trc(), Trc::Srgb);
    assert_eq!(NamedSpace::AdobeRgb.trc(), Trc::Gamma(2.19921875f32));
    assert_eq!(NamedSpace::ProPhoto.trc(), Trc::Gamma(1.8));
    assert_eq!(NamedSpace::Rec2020.trc(), Trc::Rec709);
}
