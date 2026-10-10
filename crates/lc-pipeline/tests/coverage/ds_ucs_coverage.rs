use lightcraft_pipeline::ucs;
use std::f32::consts::{PI, TAU};

#[test]
fn matrices_invert_approximately() {
    let m = ucs::mats();

    for i in 0..3 {
        let mut e = [0.0f32; 3];
        e[i] = 1.0;

        let rgb_xyz = ucs::mul(&m.rgb_to_xyz, ucs::mul(&m.xyz_to_rgb, e));
        assert!((rgb_xyz[i] - 1.0).abs() < 1e-5, "rgb_xyz basis {i}: {rgb_xyz:?}");
        for k in 0..3 {
            if k != i {
                assert!(rgb_xyz[k].abs() < 1e-5, "rgb_xyz basis {i}: {rgb_xyz:?}");
            }
        }

        let rgb_lms = ucs::mul(&m.rgb_to_lms, ucs::mul(&m.lms_to_rgb, e));
        assert!((rgb_lms[i] - 1.0).abs() < 1e-5, "rgb_lms basis {i}: {rgb_lms:?}");
        for k in 0..3 {
            if k != i {
                assert!(rgb_lms[k].abs() < 1e-5, "rgb_lms basis {i}: {rgb_lms:?}");
            }
        }
    }
}

#[test]
fn mul_is_linear() {
    let mat = ucs::mats().rgb_to_xyz;
    let v1 = [0.1f32, 0.2, 0.3];
    let v2 = [0.4f32, 0.5, 0.6];

    let a = ucs::mul(&mat, v1);
    let b = ucs::mul(&mat, v2);
    let sum = ucs::mul(&mat, [v1[0] + v2[0], v1[1] + v2[1], v1[2] + v2[2]]);

    for k in 0..3 {
        assert!((a[k] + b[k] - sum[k]).abs() < 1e-6, "linearity failed at {k}: {a:?} + {b:?} != {sum:?}");
    }

    let scaled = ucs::mul(&mat, [2.0 * v1[0], 2.0 * v1[1], 2.0 * v1[2]]);
    for k in 0..3 {
        assert!((scaled[k] - 2.0 * a[k]).abs() < 1e-6, "scaling failed at {k}: {scaled:?} vs {:?}", [2.0 * a[0], 2.0 * a[1], 2.0 * a[2]]);
    }
}

#[test]
fn y_l_star_roundtrip() {
    for y in [0.0f32, 1e-6, 1e-3, 0.1, 0.5, 1.0, 10.0, 100.0] {
        let l = ucs::y_to_l_star(y);
        assert!(l.is_finite(), "L* not finite for y={y}");
        if y > 0.0 {
            let back = ucs::l_star_to_y(l);
            assert!((back - y).abs() / y.max(1e-3) < 1e-3, "roundtrip failed for y={y}: back={back}, L*={l}");
        } else {
            assert_eq!(l, 0.0, "L* for y=0 should be 0");
        }
    }
}

#[test]
fn l_star_upper_bound_handles_extreme() {
    let l_max = ucs::y_to_l_star(f32::MAX);
    assert!(l_max.is_finite());
    // The function asymptotically approaches L_STAR_RANGE from below, but for f32::MAX the
    // computation may round to exactly L_STAR_RANGE.
    assert!(l_max <= ucs::L_STAR_RANGE);
    assert!((l_max - ucs::L_STAR_RANGE).abs() < 1e-3, "L* for f32::MAX should approach L_STAR_RANGE, got {l_max}");

    let l_inf = ucs::y_to_l_star(f32::INFINITY);
    assert!(l_inf.is_nan(), "L* for +inf should be NaN, got {l_inf}");
}

#[test]
fn xy_uv_roundtrip() {
    for (x, y) in [(0.64f32, 0.33f32), (0.3, 0.6), (0.15, 0.06), (0.3127, 0.3290)] {
        let uv = ucs::xy_to_uv(x, y);
        let back = ucs::uv_to_xy(uv);
        assert!((back[0] - x).abs() < 1e-4 && (back[1] - y).abs() < 1e-4, "xy -> uv -> xy failed for ({x},{y}): got ({},{})", back[0], back[1]);
    }
}

#[test]
fn xyz_xyy_clips_negative() {
    let xy = ucs::xyz_to_xyy([-1.0f32, -2.0, -3.0]);
    assert!((xy[0] - 0.31271).abs() < 1e-6, "negative xyz should clip to white x, got {}", xy[0]);
    assert!((xy[1] - 0.32902).abs() < 1e-6, "negative xyz should clip to white y, got {}", xy[1]);
    assert_eq!(xy[2], 0.0, "negative xyz should have zero luminance");
}

#[test]
fn xyz_xyy_roundtrip() {
    for xyz in [[0.1f32, 0.2, 0.3], [0.5, 0.4, 0.3], [0.0, 0.0, 0.0], [0.8, 0.9, 0.7]] {
        let xyy = ucs::xyz_to_xyy(xyz);
        let back = ucs::xyy_to_xyz(xyy);
        for k in 0..3 {
            assert!((back[k] - xyz[k]).abs() < 1e-4, "xyz -> xyY -> xyz failed for {xyz:?}: got {back:?} via {xyy:?}");
        }
    }
}

#[test]
fn yrg_lms_roundtrip() {
    for lms in [[0.2f32, 0.3, 0.4], [0.8, 0.1, 0.05], [0.0, 0.0, 0.0], [0.5, 0.5, 0.5]] {
        let yrg = ucs::lms_to_yrg(lms);
        let back = ucs::yrg_to_lms(yrg);
        for k in 0..3 {
            assert!((back[k] - lms[k]).abs() < 1e-4, "lms -> yrg -> lms failed for {lms:?}: got {back:?}");
        }
    }
}

#[test]
fn yrg_ych_roundtrip() {
    for yrg in [[0.1f32, 0.3, 0.4], [0.5, 0.2, 0.6], [0.8, 0.1, 0.1], [0.0, 0.2, 0.3]] {
        let ych = ucs::yrg_to_ych(yrg);
        let back = ucs::ych_to_yrg(ych);
        for k in 0..3 {
            assert!((back[k] - yrg[k]).abs() < 1e-4, "yrg -> ych -> yrg failed for {yrg:?}: got {back:?}");
        }
    }
}

#[test]
fn gamut_check_yrg_reduces_out_of_gamut() {
    let mut ych = [0.5f32, 10.0, 1.0, 0.0];
    ucs::gamut_check_yrg(&mut ych);

    let max_c_for_hue0 = 1.0 - ucs::YRG_WHITE[0] - ucs::YRG_WHITE[1];
    assert!(ych[1] > 0.0 && ych[1] <= max_c_for_hue0 + 1e-6, "chroma should be clipped to <= {max_c_for_hue0}, got {}", ych[1]);

    let yrg = ucs::ych_to_yrg(ych);
    assert!(yrg[0] >= -1e-6, "red component negative: {yrg:?}");
    assert!(yrg[1] >= -1e-6, "green component negative: {yrg:?}");
    assert!(yrg[0] + yrg[1] <= 1.0 + 1e-6, "red+green > 1: {yrg:?}");
}

#[test]
fn gamut_check_yrg_preserves_in_gamut() {
    let mut ych = [0.5f32, 0.1, 0.6, 0.8];
    let original_c = ych[1];
    ucs::gamut_check_yrg(&mut ych);

    assert!((ych[1] - original_c).abs() < 1e-7, "in-gamut chroma should not change, got {} vs {}", ych[1], original_c);
    assert_eq!(ych[2], 0.6);
    assert_eq!(ych[3], 0.8);
}

#[test]
fn jch_hsb_roundtrip() {
    for jch in [[0.5f32, 0.3, 0.6], [0.8, 0.1, -2.0], [0.2, 0.0, 1.0], [1.0, 0.4, PI]] {
        let hsb = ucs::jch_to_hsb(jch);
        let back = ucs::hsb_to_jch(hsb);
        for k in 0..3 {
            assert!((back[k] - jch[k]).abs() < 1e-4, "jch -> hsb -> jch failed for {jch:?}: got {back:?}");
        }
    }
}

#[test]
fn jch_hcb_roundtrip() {
    for jch in [[0.5f32, 0.3, 0.6], [0.8, 0.1, -2.0], [0.2, 0.0, 1.0], [1.0, 0.4, PI]] {
        let hcb = ucs::jch_to_hcb(jch);
        let back = ucs::hcb_to_jch(hcb);
        for k in 0..3 {
            assert!((back[k] - jch[k]).abs() < 1e-4, "jch -> hcb -> jch failed for {jch:?}: got {back:?}");
        }
    }
}

#[test]
fn soft_clip_bounds() {
    assert!((ucs::soft_clip(0.4, 0.5, 1.0) - 0.4).abs() < 1e-6);
    assert!((ucs::soft_clip(0.5, 0.5, 1.0) - 0.5).abs() < 1e-6);

    let v = ucs::soft_clip(0.7, 0.5, 1.0);
    // soft_clip compresses values above `soft` toward `hard`, so v must be within (soft, hard)
    // and less than the original input (0.7) because 0.7 > soft.
    assert!(v > 0.5 && v < 1.0 && v <= 0.7, "soft clip should map to (soft, hard) and not exceed input, got {v}");

    let high = ucs::soft_clip(10.0, 0.5, 1.0);
    assert!((high - 1.0).abs() < 1e-5, "very high input should approach hard, got {high}");
}

#[test]
fn gamut_lut_positive_and_finite() {
    let l = ucs::gamut_lut();
    assert_eq!(l.len(), ucs::GAMUT_N);
    assert!(l.iter().all(|v| v.is_finite() && *v > 0.0), "gamut LUT must contain only positive finite values");
}

#[test]
fn lookup_gamut_periodic() {
    let lut = ucs::gamut_lut();
    for hue in [0.0f32, 0.3, PI, -PI, 2.0] {
        let base = ucs::lookup_gamut(lut, hue);
        let plus = ucs::lookup_gamut(lut, hue + TAU);
        let minus = ucs::lookup_gamut(lut, hue - TAU);
        assert!((base - plus).abs() < 1e-5, "periodicity failed for {hue}: {base} vs {plus}");
        assert!((base - minus).abs() < 1e-5, "periodicity failed for {hue}: {base} vs {minus}");
    }
}

#[test]
fn lookup_gamut_single_element() {
    let lut = [0.75f32];
    for hue in [-10.0f32, 0.0, 1.0, 10.0] {
        let v = ucs::lookup_gamut(&lut, hue);
        assert!((v - 0.75).abs() < 1e-7, "single-element LUT should be constant, got {v}");
    }
}

#[test]
fn gamut_map_hsb_reduces_saturation() {
    let lw = ucs::y_to_l_star(1.0);
    let rgb = [0.2f32, 0.3, 0.4];
    let base = ucs::rgb_to_hsb(rgb, lw);
    let mut hsb = [base[0], 10.0, base[2]];

    ucs::gamut_map_hsb(&mut hsb, lw);
    assert!(hsb[1] < 10.0, "saturation should be reduced by gamut mapping, got {}", hsb[1]);
    assert!(hsb[1] >= 0.0, "saturation should not become negative, got {}", hsb[1]);
}

#[test]
fn gamut_map_hsb_preserves_hue_and_brightness() {
    let lw = ucs::y_to_l_star(1.0);
    let rgb = [0.2f32, 0.3, 0.4];
    let base = ucs::rgb_to_hsb(rgb, lw);
    let mut hsb = [base[0], 10.0, base[2]];
    let h0 = hsb[0];
    let b0 = hsb[2];

    ucs::gamut_map_hsb(&mut hsb, lw);

    assert_eq!(hsb[0], h0, "hue must not change");
    assert_eq!(hsb[2], b0, "brightness must not change");
}

#[test]
fn rgb_hsb_roundtrip() {
    let lw = ucs::y_to_l_star(1.0);
    for rgb in [[0.2f32, 0.3, 0.4], [0.8, 0.1, 0.05], [0.02, 0.5, 0.1], [0.18, 0.18, 0.18]] {
        let hsb = ucs::rgb_to_hsb(rgb, lw);
        let back = ucs::hsb_to_rgb(hsb, lw);
        for k in 0..3 {
            assert!((back[k] - rgb[k]).abs() < 2e-3, "rgb -> hsb -> rgb failed for {rgb:?}: got {back:?} via {hsb:?}");
        }
    }
}

#[test]
fn ucs_hue_of_srgb_hue_finite_range() {
    for deg in [0.0f64, 30.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0] {
        let h = ucs::ucs_hue_of_srgb_hue(deg);
        assert!(h.is_finite(), "UCS hue for {deg} deg is not finite");
        assert!((-PI..=PI).contains(&h), "UCS hue for {deg} deg out of range: {h}");
    }
}

#[test]
fn nan_inf_no_panic() {
    let lut = ucs::gamut_lut();
    let custom: [f32; ucs::GAMUT_N] = [0.5; ucs::GAMUT_N];
    let mut hsb = [f32::NAN, f32::NAN, f32::NAN];
    let mut ych = [f32::NAN; 4];

    let _ = ucs::y_to_l_star(f32::NAN);
    let _ = ucs::l_star_to_y(f32::INFINITY);
    let _ = ucs::xy_to_uv(f32::NAN, f32::INFINITY);
    let _ = ucs::uv_to_xy([f32::NAN, f32::INFINITY]);
    let _ = ucs::xyz_to_xyy([f32::NAN, f32::NAN, f32::NAN]);
    let _ = ucs::xyy_to_xyz([f32::NAN, f32::NAN, f32::NAN]);
    let _ = ucs::lms_to_yrg([f32::INFINITY; 3]);
    let _ = ucs::yrg_to_lms([f32::NAN; 3]);

    ucs::gamut_check_yrg(&mut ych);
    let _ = ucs::lookup_gamut(lut, f32::NAN);
    ucs::gamut_map_hsb_with(&mut hsb, f32::INFINITY, &custom);
    ucs::gamut_map_hsb(&mut hsb, f32::NAN);
}
