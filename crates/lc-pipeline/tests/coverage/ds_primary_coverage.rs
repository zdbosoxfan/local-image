use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::primary::{self, HsMethod};

#[test]
#[allow(clippy::assertions_on_constants)]
fn default_method_is_li_tone() {
    assert_eq!(primary::DEFAULT_HS_METHOD, HsMethod::LiTone);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn proxy_pixels_is_positive() {
    assert!(primary::PROXY_PIXELS > 0);
}

#[test]
fn hs_method_equality_and_copy() {
    let a = HsMethod::LiTone;
    let b = a;
    assert_eq!(a, b);
    assert_ne!(a, HsMethod::Eigf);
    let c = HsMethod::Eigf;
    assert_eq!(c, HsMethod::Eigf);
}

#[test]
fn smooth_left_boundary_is_zero() {
    assert_eq!(primary::smooth(0.0, 1.0, 0.0), 0.0);
}

#[test]
fn smooth_right_boundary_is_one() {
    assert_eq!(primary::smooth(0.0, 1.0, 1.0), 1.0);
}

#[test]
fn smooth_clamps_below_and_above() {
    assert_eq!(primary::smooth(0.0, 1.0, -0.5), 0.0);
    assert_eq!(primary::smooth(0.0, 1.0, 1.5), 1.0);
    assert_eq!(primary::smooth(-2.0, 3.0, -10.0), 0.0);
    assert_eq!(primary::smooth(-2.0, 3.0, 10.0), 1.0);
}

#[test]
fn smooth_midpoint_is_half() {
    let v = primary::smooth(0.0, 1.0, 0.5);
    assert!((v - 0.5).abs() < 1e-6);
}

#[test]
fn smooth_is_monotonic_increasing() {
    let mut prev = primary::smooth(0.0, 1.0, -0.1);
    for i in 0..=100 {
        let x = i as f32 / 100.0;
        let v = primary::smooth(0.0, 1.0, x);
        assert!(v >= prev);
        prev = v;
    }
}

#[test]
fn smooth_handles_reversed_bounds() {
    assert_eq!(primary::smooth(1.0, 0.0, 0.0), 1.0);
    assert_eq!(primary::smooth(1.0, 0.0, 1.0), 0.0);
    let mid = primary::smooth(1.0, 0.0, 0.5);
    assert!((mid - 0.5).abs() < 1e-6);
}

#[test]
fn tone_gain_neutral_sliders_are_zero_everywhere() {
    for l in [-10.0, -5.0, -1.0, 0.0, 1.0, 5.0, 10.0] {
        let v = primary::tone_gain(l as f32, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(v, 0.0);
    }
}

#[test]
fn tone_gain_is_zero_at_middle_grey() {
    let v = primary::tone_gain(0.0, 0.4, 0.3, 0.5, 0.2);
    assert!((v - 0.0).abs() < 1e-6);
}

#[test]
fn tone_gain_extreme_low_combines_shadows_and_blacks() {
    let v = primary::tone_gain(-100.0, 0.0, 0.25, 0.0, 0.25);
    // 1.8 * shadows + 0.8 * blacks = 0.45 + 0.2
    assert!((v - 0.65).abs() < 1e-5);
}

#[test]
fn tone_gain_extreme_high_combines_highlights_and_whites() {
    let v = primary::tone_gain(100.0, 0.25, 0.0, 0.25, 0.0);
    // 1.6 * highlights + 0.8 * whites = 0.4 + 0.2
    assert!((v - 0.6).abs() < 1e-5);
}

#[test]
fn tone_gain_is_bounded_for_typical_sliders() {
    for l in [-20.0, -5.0, 0.0, 3.0, 8.0, 20.0] {
        let v = primary::tone_gain(l, 0.5, 0.5, 0.5, 0.5);
        assert!(v.is_finite());
        assert!(v.abs() < 5.0);
    }
}

#[test]
fn tone_gain_does_not_panic_on_nan_or_infinity() {
    let _ = primary::tone_gain(f32::NAN, 0.5, 0.5, 0.5, 0.5);
    let _ = primary::tone_gain(f32::INFINITY, 0.5, 0.5, 0.5, 0.5);
    let _ = primary::tone_gain(0.0, f32::NAN, 0.5, 0.5, 0.5);
    let _ = primary::tone_gain(0.0, 0.0, f32::INFINITY, 0.0, -f32::INFINITY);
}

#[test]
fn log_light_uniform_mid_gray_is_near_zero() {
    let v = primary::log_light([0.18, 0.18, 0.18]);
    assert!(v.abs() < 1e-5);
}

#[test]
fn log_light_white_is_about_2_47() {
    let v = primary::log_light([1.0, 1.0, 1.0]);
    assert!((v - 2.47393).abs() < 1e-3);
}

#[test]
fn log_light_black_is_large_negative() {
    let v = primary::log_light([0.0, 0.0, 0.0]);
    assert!(v < -20.0);
    assert!(v > -22.0);
    assert!(v.is_finite());
}

#[test]
fn log_light_saturated_color_is_finite_and_reasonable() {
    let v = primary::log_light([1.0, 0.0, 0.0]);
    assert!(v.is_finite());
    assert!(v > 1.0);
    assert!(v < 3.0);
}

#[test]
fn log_light_does_not_panic_on_nan_or_infinity() {
    let _ = primary::log_light([f32::NAN, 0.0, 0.0]);
    let _ = primary::log_light([f32::INFINITY, 0.0, -f32::INFINITY]);
    let _ = primary::log_light([0.0, f32::NAN, f32::NAN]);
}

#[test]
fn stages_default_all_false() {
    let s: DevelopSettings = Default::default();
    let stages = primary::Stages::of(&s);
    assert!(!stages.tone);
    assert!(!stages.clarity);
    assert!(!stages.texture);
    assert!(!stages.structure);
    assert!(!stages.equalizer);
    assert!(!stages.balance);
    assert!(!stages.skin);
    assert!(!stages.any());
    assert!(!stages.proxy());
}

#[test]
fn stages_any_and_proxy_from_fields() {
    let tone = primary::Stages { tone: true, ..Default::default() };
    assert!(tone.any());
    assert!(tone.proxy());

    let clarity = primary::Stages { clarity: true, ..Default::default() };
    assert!(clarity.any());
    assert!(clarity.proxy());

    let texture = primary::Stages { texture: true, ..Default::default() };
    assert!(texture.any());
    assert!(!texture.proxy());

    let structure = primary::Stages { structure: true, ..Default::default() };
    assert!(structure.any());
    assert!(!structure.proxy());
}

#[test]
fn active_and_related_default_false() {
    let s: DevelopSettings = Default::default();
    assert!(!primary::active(&s));
    assert!(!primary::needs_proxy(&s));
    assert!(!primary::needs_clip(&s));
    assert!(!primary::layers_active(&s));
}

#[test]
fn negative_highlights_enables_tone_proxy_active_and_clip() {
    let mut s: DevelopSettings = Default::default();
    s.light.highlights = -50.0;
    let stages = primary::Stages::of(&s);
    assert!(stages.tone);
    assert!(primary::needs_proxy(&s));
    assert!(primary::active(&s));
    assert!(primary::needs_clip(&s));
}

#[test]
fn positive_clarity_enables_clarity_and_proxy() {
    let mut s: DevelopSettings = Default::default();
    s.effects.clarity = 25.0;
    let stages = primary::Stages::of(&s);
    assert!(stages.clarity);
    assert!(primary::needs_proxy(&s));
    assert!(primary::active(&s));
    assert!(!primary::needs_clip(&s));
}

#[test]
fn skin_tone_reference_enables_skin_stage() {
    let mut s: DevelopSettings = Default::default();
    s.skin_tone.reference = Some([0.5, 0.5, 0.5]);
    s.skin_tone.uniformity = 10.0;
    let stages = primary::Stages::of(&s);
    assert!(stages.skin);
    assert!(primary::active(&s));
    // skin itself does not require the geometric proxy
    assert!(!primary::needs_proxy(&s));
}

#[test]
fn remaining_clears_primary_tools_but_keeps_exposure() {
    let mut s: DevelopSettings = Default::default();
    s.light.exposure = 1.5;
    s.light.highlights = -30.0;
    s.effects.clarity = 20.0;
    s.effects.texture = 10.0;
    s.color.vibrance = 5.0;
    s.skin_tone.reference = Some([0.4, 0.4, 0.4]);
    s.skin_tone.uniformity = 7.0;

    let rem = primary::remaining(&s);
    assert!((rem.light.exposure - 1.5).abs() < 1e-9);
    assert_eq!(rem.light.highlights, 0.0);
    assert_eq!(rem.effects.clarity, 0.0);
    assert_eq!(rem.effects.texture, 0.0);
    assert_eq!(rem.effects.structure, 0.0);
    assert_eq!(rem.color.vibrance, 0.0);
    assert_eq!(rem.color.saturation, 0.0);
    assert_eq!(rem.skin_tone, Default::default());

    let stages = primary::Stages::of(&rem);
    assert!(!stages.tone);
    assert!(!stages.clarity);
    assert!(!stages.texture);
    assert!(!stages.structure);
    assert!(!stages.balance);
    assert!(!stages.skin);
    assert!(!primary::active(&rem));
}
