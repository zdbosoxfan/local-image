use lightcraft_develop::{DevelopSettings, Treatment, Wheel};
use lightcraft_pipeline::profiles::{LOOK_IDS, effective};
use std::borrow::Cow;

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
