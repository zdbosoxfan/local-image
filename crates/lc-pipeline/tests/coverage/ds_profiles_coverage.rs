use lightcraft_develop::{DevelopSettings, Treatment, Wheel};
use lightcraft_geom::Point;
use lightcraft_pipeline::profiles::{LOOK_IDS, effective};
use std::borrow::Cow;

#[test]
fn look_ids_matches_expected() {
    let expected = &[
        "lc.neutral",
        "lc.vivid",
        "lc.landscape",
        "lc.portrait",
        "lc.mono",
        "lc.film.warm-print",
        "lc.film.cool-fade",
        "lc.film.golden-hour",
        "lc.film.faded-slide",
        "lc.cine.teal-amber",
        "lc.cine.night-blue",
        "lc.cine.desert-heat",
        "lc.cine.neon-dusk",
        "lc.muted.matte-soft",
        "lc.muted.bleached",
        "lc.muted.pastel-haze",
        "lc.muted.quiet-green",
        "lc.filmsim.vivid-slide",
        "lc.filmsim.natural-slide",
        "lc.filmsim.portrait-negative",
        "lc.filmsim.consumer-negative",
        "lc.filmsim.cinema-negative",
        "lc.filmsim.instant",
        "lc.filmsim.classic-bw",
        "lc.filmsim.high-speed-bw",
        "lc.bw.mono-rich",
        "lc.bw.red-filter",
        "lc.bw.soft",
        "lc.bw.sepia",
    ];
    assert_eq!(LOOK_IDS, expected);
}

#[test]
fn effective_identity_ids_are_borrowed() {
    let mut s = DevelopSettings::default();
    for id in ["", "lc.color"] {
        s.profile.id = id.to_string();
        s.profile.amount = 100.0;
        assert!(matches!(effective(&s), Cow::Borrowed(_)), "id {id:?}");
    }
}

#[test]
fn effective_zero_amount_borrowed_for_all_looks() {
    let mut s = DevelopSettings::default();
    for &id in LOOK_IDS {
        s.profile.id = id.to_string();
        s.profile.amount = 0.0;
        assert!(matches!(effective(&s), Cow::Borrowed(_)), "id {id}");
    }
}

#[test]
fn effective_unknown_id_borrowed() {
    let mut s = DevelopSettings::default();
    s.profile.id = "not.a.real.profile".to_string();
    s.profile.amount = 100.0;
    assert!(matches!(effective(&s), Cow::Borrowed(_)));
}

#[test]
fn effective_known_id_owned_and_different() {
    let mut s = DevelopSettings::default();
    for &id in LOOK_IDS {
        s.profile.id = id.to_string();
        s.profile.amount = 100.0;
        let result = effective(&s);
        assert!(matches!(result, Cow::Owned(_)), "id {id}");
        assert_ne!(*result, s, "id {id}");
    }
}

#[test]
fn effective_does_not_modify_original() {
    let mut s = DevelopSettings::default();
    for &id in LOOK_IDS {
        s.profile.id = id.to_string();
        s.profile.amount = 100.0;
        let original = s.clone();
        let _ = effective(&s);
        assert_eq!(s, original, "id {id}");
    }
}

#[test]
fn effective_deterministic() {
    let mut s = DevelopSettings::default();
    for &id in LOOK_IDS {
        s.profile.id = id.to_string();
        s.profile.amount = 100.0;
        let first = effective(&s).into_owned();
        let second = effective(&s).into_owned();
        assert_eq!(first, second, "id {id}");
    }
}

#[test]
fn effective_linear_scaling_of_slider_deltas() {
    let mut base = DevelopSettings::default();
    base.light.contrast = 5.0;
    for &id in LOOK_IDS {
        base.profile.id = id.to_string();
        let at = |a: f64| {
            let mut s = base.clone();
            s.profile.amount = a;
            effective(&s).into_owned()
        };
        let half = at(50.0);
        let full = at(100.0);
        let double = at(200.0);
        let delta_full = full.light.contrast - base.light.contrast;
        let delta_half = half.light.contrast - base.light.contrast;
        let delta_double = double.light.contrast - base.light.contrast;
        assert!((delta_full - 2.0 * delta_half).abs() < 1e-9, "id {id}");
        assert!((delta_double - 2.0 * delta_full).abs() < 1e-9, "id {id}");
    }
}

#[test]
fn effective_clamps_amount_to_0_for_negative() {
    let mut s = DevelopSettings::default();
    s.profile.id = "lc.vivid".to_string();
    s.profile.amount = -50.0;
    assert!(matches!(effective(&s), Cow::Borrowed(_)));
}

#[test]
fn effective_clamps_amount_to_2_for_over_200() {
    let mut s_300 = DevelopSettings::default();
    s_300.profile.id = "lc.vivid".to_string();
    s_300.profile.amount = 300.0;

    let mut s_200 = s_300.clone();
    s_200.profile.amount = 200.0;

    let mut e_300 = effective(&s_300).into_owned();
    let e_200 = effective(&s_200).into_owned();
    e_300.profile = e_200.profile.clone();
    assert_eq!(e_300, e_200);
}

#[test]
fn effective_clamps_amount_to_2_for_infinity() {
    let mut s_inf = DevelopSettings::default();
    s_inf.profile.id = "lc.vivid".to_string();
    s_inf.profile.amount = f64::INFINITY;

    let mut s_200 = s_inf.clone();
    s_200.profile.amount = 200.0;

    let mut e_inf = effective(&s_inf).into_owned();
    let e_200 = effective(&s_200).into_owned();
    e_inf.profile = e_200.profile.clone();
    assert_eq!(e_inf, e_200);
}

#[test]
fn effective_with_nan_amount_does_not_panic() {
    let mut s = DevelopSettings::default();
    s.profile.id = "lc.vivid".to_string();
    s.profile.amount = f64::NAN;
    let result = effective(&s);
    assert!(matches!(result, Cow::Owned(_)));
}

#[test]
fn effective_with_nan_slider_value_does_not_panic() {
    let mut s = DevelopSettings::default();
    s.light.contrast = f64::NAN;
    s.profile.id = "lc.vivid".to_string();
    s.profile.amount = 100.0;
    let result = effective(&s);
    assert!(matches!(result, Cow::Owned(_)));
}

#[test]
fn effective_clamps_modified_sliders() {
    let mut s = DevelopSettings::default();
    s.light.contrast = 500.0;
    s.profile.id = "lc.vivid".to_string();
    s.profile.amount = 100.0;
    let e = effective(&s);
    assert_eq!(e.light.contrast, 100.0);

    s.light.contrast = -500.0;
    let e = effective(&s);
    assert_eq!(e.light.contrast, -100.0);
}

#[test]
fn bw_looks_set_treatment_to_bw() {
    let bw_ids = ["lc.mono", "lc.filmsim.classic-bw", "lc.filmsim.high-speed-bw", "lc.bw.mono-rich", "lc.bw.red-filter", "lc.bw.soft", "lc.bw.sepia"];
    let mut s = DevelopSettings::default();
    for &id in &bw_ids {
        s.profile.id = id.to_string();
        s.profile.amount = 100.0;
        let e = effective(&s);
        assert_eq!(e.treatment, Treatment::Bw, "id {id}");
    }
}

#[test]
fn non_bw_looks_do_not_change_treatment() {
    let bw_ids = ["lc.mono", "lc.filmsim.classic-bw", "lc.filmsim.high-speed-bw", "lc.bw.mono-rich", "lc.bw.red-filter", "lc.bw.soft", "lc.bw.sepia"];
    let base = DevelopSettings { treatment: Treatment::Color, ..DevelopSettings::default() };
    for &id in LOOK_IDS {
        if bw_ids.contains(&id) {
            continue;
        }
        let mut s = base.clone();
        s.profile.id = id.to_string();
        s.profile.amount = 100.0;
        let e = effective(&s);
        assert_eq!(e.treatment, s.treatment, "id {id} should not change treatment");
    }
}

#[test]
fn fade_transform_applied_correctly() {
    let mut s = DevelopSettings::default();
    s.curve.master = vec![Point::new(0.0, 0.0), Point::new(0.5, 0.6), Point::new(1.0, 1.0)];
    s.profile.id = "lc.film.warm-print".to_string();
    s.profile.amount = 100.0; // k = 1.0
    let e = effective(&s);
    let ys: Vec<f64> = e.curve.master.iter().map(|p| p.y).collect();
    // black=0.04, white=0.97 => b=0.04, w=0.97, y' = 0.04 + y*(0.93)
    let expected = [0.04, 0.04 + 0.6 * 0.93, 0.97];
    for (actual, want) in ys.iter().zip(expected.iter()) {
        assert!((actual - want).abs() < 1e-9, "expected {want}, got {actual}");
    }
}

#[test]
fn fade_does_not_modify_other_curve_sliders() {
    let mut s = DevelopSettings::default();
    s.curve.shadows = 0.25;
    s.curve.highlights = -0.3;
    s.curve.darks = 0.4;
    s.curve.lights = -0.2;
    let original_shadows = s.curve.shadows;
    let original_highlights = s.curve.highlights;
    let original_darks = s.curve.darks;
    let original_lights = s.curve.lights;

    s.profile.id = "lc.film.warm-print".to_string();
    s.profile.amount = 100.0;
    let e = effective(&s);

    assert_eq!(e.curve.shadows, original_shadows);
    assert_eq!(e.curve.highlights, original_highlights);
    assert_eq!(e.curve.darks, original_darks);
    assert_eq!(e.curve.lights, original_lights);
}

#[test]
fn wheel_composition_with_user_wheel() {
    let mut s = DevelopSettings::default();
    s.grading.shadows = Wheel { hue: 0.0, sat: 10.0, lum: 0.0 };
    s.profile.id = "lc.cine.teal-amber".to_string();
    s.profile.amount = 100.0;
    let e = effective(&s);

    // User vector: (10, 0)
    // Profile adds shadow wheel: hue=195, sat=30
    let ua = 10.0 * 0.0f64.to_radians().cos();
    let ub = 10.0 * 0.0f64.to_radians().sin();
    let pa = 30.0 * 195.0f64.to_radians().cos();
    let pb = 30.0 * 195.0f64.to_radians().sin();
    let sum_a = ua + pa;
    let sum_b = ub + pb;
    let expected_sat = sum_a.hypot(sum_b).clamp(0.0, 100.0);
    let expected_hue = if expected_sat > 1e-9 { sum_b.atan2(sum_a).to_degrees().rem_euclid(360.0) } else { 195.0 };

    assert!((e.grading.shadows.sat - expected_sat).abs() < 1e-9, "sat");
    assert!((e.grading.shadows.hue - expected_hue).abs() < 1e-9, "hue");
}

#[test]
fn wheel_saturation_clamps_to_100() {
    let mut s = DevelopSettings::default();
    s.grading.shadows = Wheel { hue: 265.0, sat: 90.0, lum: 0.0 };
    s.profile.id = "lc.cine.neon-dusk".to_string(); // adds shadow wheel hue=265, sat=22
    s.profile.amount = 100.0;
    let e = effective(&s);

    assert_eq!(e.grading.shadows.sat, 100.0);
    // Since both vectors point in same direction, hue remains 265
    assert!((e.grading.shadows.hue - 265.0).abs() < 1e-9);
}

#[test]
fn effective_preserves_profile_field() {
    let mut s = DevelopSettings::default();
    for &id in LOOK_IDS {
        s.profile.id = id.to_string();
        s.profile.amount = 100.0;
        let e = effective(&s);
        assert_eq!(e.profile, s.profile, "id {id}");
    }
}

#[test]
fn effective_preserves_grain() {
    let mut s = DevelopSettings::default();
    for &id in LOOK_IDS {
        s.profile.id = id.to_string();
        s.profile.amount = 100.0;
        let e = effective(&s);
        assert_eq!(e.grain, s.grain, "id {id}");
    }
}

#[test]
fn look_ids_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for &id in LOOK_IDS {
        assert!(seen.insert(id), "duplicate id {id}");
    }
}
