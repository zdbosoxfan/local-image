use photocraft_color::Color;
use photocraft_color::blend::BlendMode;
use photocraft_doc::{Bevel, BevelStyle, BevelTechnique, Contour, FxCommon, GradientStyle};
use photocraft_geom::Rect;

use photocraft_compose::effects::{self, BevelPaint, FieldKind};

fn bevel(style: BevelStyle, technique: BevelTechnique) -> Bevel {
    Bevel {
        enabled: true,
        style,
        technique,
        depth: 1.0,
        up: true,
        size: 5.0,
        soften: 0.0,
        angle: 90.0,
        altitude: 30.0,
        use_global_light: false,
        gloss_contour: Contour::Linear,
        highlight: FxCommon::new(BlendMode::Screen, 0.75),
        highlight_color: Color::WHITE,
        shadow: FxCommon::new(BlendMode::Multiply, 0.75),
        shadow_color: Color::BLACK,
        contour: None,
        texture: None,
    }
}

#[test]
fn hash_noise_deterministic_in_range() {
    for x in -10..=10 {
        for y in -5..=5 {
            let v = effects::hash_noise(x, y);
            assert!((0.0..=1.0).contains(&v), "({x},{y}) -> {v}");
            assert_eq!(v, effects::hash_noise(x, y));
        }
    }
}

#[test]
fn gradient_units_axis_aligned() {
    let (chord, len) = effects::gradient_units(0.0, 100.0, 50.0);
    assert!((chord - 100.0).abs() < 1e-5);
    assert!((len - 100.0).abs() < 1e-5);

    let (chord, len) = effects::gradient_units(90.0, 100.0, 50.0);
    assert!((chord - 50.0).abs() < 1e-5);
    assert!((len - 50.0).abs() < 1e-5);
}

#[test]
fn gradient_units_square_diagonal() {
    let (chord, len) = effects::gradient_units(45.0, 100.0, 100.0);
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let expected_chord = 100.0 / s;
    assert!((chord - expected_chord).abs() < 0.1);
    assert!((len - 100.0).abs() < 0.1);
}

#[test]
fn gradient_t_linear_endpoints() {
    let b = Rect::new(0, 0, 100, 100);
    let t = |x: f32| effects::gradient_t(GradientStyle::Linear, 0.0, 1.0, false, (0.0, 0.0), b, x, 50.0);
    assert!(t(0.0) < 1e-6);
    assert!((t(50.5) - 0.5).abs() < 1e-5);
    assert!(t(100.0) > 0.99);
}

#[test]
fn gradient_t_reverse() {
    let b = Rect::new(0, 0, 100, 100);
    let t_fwd = effects::gradient_t(GradientStyle::Linear, 0.0, 1.0, false, (0.0, 0.0), b, 0.0, 50.0);
    let t_rev = effects::gradient_t(GradientStyle::Linear, 0.0, 1.0, true, (0.0, 0.0), b, 0.0, 50.0);
    assert!(t_fwd < 0.01);
    assert!(t_rev > 0.99);
}

#[test]
fn gradient_t_reflected() {
    let b = Rect::new(0, 0, 100, 100);
    let t = |x: f32| effects::gradient_t(GradientStyle::Reflected, 0.0, 1.0, false, (0.0, 0.0), b, x, 50.0);
    assert!(t(50.5) < 1e-5);
    assert!((t(0.0) - 1.0).abs() < 1e-5);
    assert!(t(100.0) > 0.98 && t(100.0) < 1.0);
}

#[test]
fn gradient_t_radial_center_and_edge() {
    let b = Rect::new(0, 0, 100, 100);
    let t = |x: f32, y: f32| effects::gradient_t(GradientStyle::Radial, 0.0, 1.0, false, (0.0, 0.0), b, x, y);
    assert!(t(50.0, 50.0) < 1e-6);
    assert!((t(50.0, 0.0) - 1.0).abs() < 1e-5);
    assert!((t(0.0, 50.0) - 1.0).abs() < 1e-5);
}

#[test]
fn gradient_t_diamond_and_angle() {
    let b = Rect::new(0, 0, 100, 100);
    let diamond = effects::gradient_t(GradientStyle::Diamond, 0.0, 1.0, false, (0.0, 0.0), b, 75.0, 75.0);
    assert!((diamond - 1.0).abs() < 1e-5);
    let angle = effects::gradient_t(GradientStyle::Angle, 0.0, 1.0, false, (0.0, 0.0), b, 50.0, 0.0);
    assert!((angle - 0.75).abs() < 1e-3);
}

#[test]
fn spread_split_rounds_grow_and_blur() {
    let (grow, blur) = effects::spread_split(5.0, 0.02);
    assert_eq!(grow, 0.0);
    assert!((blur - 4.9).abs() < 1e-6);

    let (grow, blur) = effects::spread_split(5.0, 1.0);
    assert_eq!(grow, 5.0);
    assert_eq!(blur, 0.0);

    let (grow, blur) = effects::spread_split(10.0, 0.5);
    assert_eq!(grow, 5.0);
    assert!((blur - 5.0).abs() < 1e-6);
}

#[test]
fn glow_gradient_gain_clamps_nan_inf() {
    assert!((effects::glow_gradient_gain(0.5) - 4.0).abs() < 1e-6);
    assert!((effects::glow_gradient_gain(1.0) - 1.0).abs() < 1e-6);
    assert!((effects::glow_gradient_gain(f32::NAN) - 1.0).abs() < 1e-6);
    assert!((effects::glow_gradient_gain(f32::INFINITY) - 1.0).abs() < 1e-6);
}

#[test]
fn tent_kernel_identity_and_width5() {
    assert_eq!(effects::tent_kernel(1.0), (0, vec![1.0]));

    let (r, k) = effects::tent_kernel(5.0);
    assert_eq!(r, 4);
    assert_eq!(k.len(), 9);
    let sum: f32 = k.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6);
    assert!((k[4] - 5.0 / 25.0).abs() < 1e-6);
    assert!((k[0] - 1.0 / 25.0).abs() < 1e-6);
    for i in 0..k.len() {
        assert!((k[i] - k[k.len() - 1 - i]).abs() < 1e-6);
    }
}

#[test]
fn tent_kernel_fractional_width() {
    let (r, k) = effects::tent_kernel(4.0);
    assert_eq!(r, 4);
    let sum: f32 = k.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6);
    assert!(k[0] > 0.0 && k[0] < 1.0 / 25.0);
}

#[test]
fn bevel_geom_inner_smooth() {
    let b = bevel(BevelStyle::InnerBevel, BevelTechnique::Smooth);
    let g = effects::bevel_geom(&b);
    assert_eq!(g.paint, BevelPaint::Inner);
    assert!((g.width - 5.0).abs() < 1e-6);
    assert!((g.depth - 5.0).abs() < 1e-6);
    assert!(!g.pillow);
    assert!((g.chisel_soft - 0.0).abs() < 1e-6);
}

#[test]
fn bevel_geom_emboss_rounds_up_and_chisel_soft() {
    let emb = bevel(BevelStyle::Emboss, BevelTechnique::Smooth);
    let g = effects::bevel_geom(&emb);
    assert_eq!(g.paint, BevelPaint::Both);
    assert!((g.width - 3.0).abs() < 1e-6); // ceil(5 / 2)
    assert!((g.depth - 3.0).abs() < 1e-6);

    let chisel = bevel(BevelStyle::OuterBevel, BevelTechnique::ChiselSoft);
    let g = effects::bevel_geom(&chisel);
    assert_eq!(g.paint, BevelPaint::Outer);
    assert!((g.width - 5.0).abs() < 1e-6);
    assert!((g.depth - 5.0).abs() < 1e-6);
    assert!((g.chisel_soft - 1.25).abs() < 1e-6); // size / 4 for size 5
}

#[test]
fn bevel_geom_up_down_sign() {
    let mut b = bevel(BevelStyle::InnerBevel, BevelTechnique::Smooth);
    b.up = false;
    let g = effects::bevel_geom(&b);
    assert!((g.depth + 5.0).abs() < 1e-6);
}

#[test]
fn bevel_chisel_h_formulas() {
    let h_in = effects::bevel_chisel_h(BevelPaint::Inner, 5.0, 2.0, 0.0);
    assert!((h_in - 0.4).abs() < 1e-6);

    let h_out = effects::bevel_chisel_h(BevelPaint::Outer, 5.0, 0.0, 2.0);
    assert!((h_out - 0.6).abs() < 1e-6);

    let h_both_in = effects::bevel_chisel_h(BevelPaint::Both, 5.0, 1.0, 0.0);
    assert!((h_both_in - 0.7).abs() < 1e-6);

    let h_both_out = effects::bevel_chisel_h(BevelPaint::Both, 5.0, 0.0, 1.0);
    assert!((h_both_out - 0.3).abs() < 1e-6);
}

#[test]
fn distance_field_outside_single_pixel() {
    let alpha = vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let d = effects::distance_field(FieldKind::Outside, alpha, 3, 3);
    assert!((d[4] + 0.5).abs() < 1e-6);
    assert!((d[1] - 0.5).abs() < 1e-6);
    assert!((d[0] - (std::f32::consts::SQRT_2 - 0.5)).abs() < 1e-5);
}

#[test]
fn distance_field_inside_single_pixel() {
    let alpha = vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let d = effects::distance_field(FieldKind::Inside, alpha, 3, 3);
    assert!((d[4] - 0.5).abs() < 1e-6);
    assert!((d[1] + 0.5).abs() < 1e-6);
}

#[test]
fn distance_field_chamfer_variants_finite() {
    let alpha: Vec<f32> = (0..25).map(|i| if i == 12 { 1.0 } else { 0.0 }).collect();
    let d_out = effects::distance_field(FieldKind::StrokeOutside, alpha.clone(), 5, 5);
    let d_in = effects::distance_field(FieldKind::StrokeInside, alpha.clone(), 5, 5);
    let d_vec = effects::distance_field(FieldKind::StrokeOutsideVector, alpha.clone(), 5, 5);
    let d_choke = effects::distance_field(FieldKind::ChokeInside, alpha, 5, 5);

    assert!(d_out.iter().all(|v| v.is_finite()));
    assert!(d_in.iter().all(|v| v.is_finite()));
    assert!(d_vec.iter().all(|v| v.is_finite()));
    assert!(d_choke.iter().all(|v| v.is_finite()));
}

#[test]
fn outline_share_clamps() {
    assert!((effects::outline_share(0.5, 0.5) - 1.0).abs() < 1e-6);
    assert!((effects::outline_share(0.0, 0.5) - 1.0).abs() < 1e-6);
    assert!((effects::outline_share(1.0, 0.5) - 0.0).abs() < 1e-6);
    assert!((effects::outline_share(0.5, 0.0) - 0.5).abs() < 1e-6);
}

#[test]
fn ranged_lut_full_range_returns_none() {
    assert!(effects::ranged_lut(&Contour::Linear, 1.0).is_none());
    assert!(effects::ranged_lut(&Contour::Linear, f32::NAN).is_none());
    assert!(effects::ranged_lut(&Contour::Linear, f32::INFINITY).is_none());

    let some = effects::ranged_lut(&Contour::Linear, 0.5).unwrap();
    assert_eq!(some.len(), 4096);
    assert!((some[0] - 0.0).abs() < 1e-6);
    assert!((some[4095] - 1.0).abs() < 1e-6);
}
