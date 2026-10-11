#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use kurbo::ParamCurveArea;
use photocraft_doc::{Color, Fill, FillRule, PathOp, Subpath};
fn square() -> Path {
    photocraft_vector::shapes::rect(8., 8., 100., 100.)
}
fn run(path: &Path, kind: EffectKind) -> Vec<Path> {
    evaluate(path, &[PathEffect::new(kind)], Options::default()).unwrap()
}
fn area(p: &Path) -> f64 {
    p.subpaths
        .iter()
        .flat_map(|s| s.segments())
        .map(|c| CubicBez::new(Point::new(c[0].x, c[0].y), Point::new(c[1].x, c[1].y), Point::new(c[2].x, c[2].y), Point::new(c[3].x, c[3].y)).signed_area())
        .sum::<f64>()
        .abs()
}
#[test]
fn path_effects45_zero_identities_preserve_cubics_holes_and_operations() {
    let mut p = photocraft_vector::shapes::ellipse(10., 20., 80., 40.);
    p.subpaths.push(photocraft_vector::shapes::ellipse(20., 25., 30., 20.).subpaths.remove(0).with_op(PathOp::Join));
    p.fill_rule = FillRule::EvenOdd;
    p.inverted = true;
    for kind in [
        EffectKind::Twist { angle: 0. },
        EffectKind::PuckerBloat { amount: 0. },
        EffectKind::ZigZag { size: 0., relative: false, ridges: 20, points: Points::Smooth },
        EffectKind::Roughen { size: 0., relative: true, detail: 90., points: Points::Corner, seed: u64::MAX },
    ] {
        assert_eq!(run(&p, kind), vec![p.clone()]);
    }
}
#[test]
fn path_effects45_round_square_uses_circle_area_oracle_and_open_endpoints() {
    let p = square();
    for r in [1., 10., 30.] {
        let out = run(&p, EffectKind::RoundCorners { radius: r }).remove(0);
        let expected = 10000. - (4. - std::f64::consts::PI) * r * r;
        assert!((area(&out) - expected).abs() < 2e-8 * r * r + 1e-8, "{} {expected}", area(&out));
    }
    let p = Path::new(vec![Subpath::polyline(&[(0., 0.), (20., 0.), (20., 20.)])]);
    let out = run(&p, EffectKind::RoundCorners { radius: 4. }).remove(0);
    assert!(!out.subpaths[0].closed);
    assert_eq!(out.subpaths[0].knots[0].anchor, p.subpaths[0].knots[0].anchor);
    assert_eq!(out.subpaths[0].knots.last().unwrap().anchor, p.subpaths[0].knots[2].anchor);
}
#[test]
fn path_effects45_offset_is_v1_and_shape_conversion_matches_analytic_bounds() {
    let p = square();
    for (join, v1) in [
        (OffsetJoin::Round, photocraft_pathops::Join::Round),
        (OffsetJoin::Bevel, photocraft_pathops::Join::Bevel),
        (OffsetJoin::Miter, photocraft_pathops::Join::Miter),
    ] {
        for d in [-4., 0., 8.] {
            assert_eq!(run(&p, EffectKind::OffsetPath { distance: d, join, miter: 4. }), vec![photocraft_pathops::offset_path(&p, d, v1, 4.).unwrap()]);
        }
    }
    for shape in [ShapeKind::Rectangle, ShapeKind::RoundedRectangle, ShapeKind::Ellipse] {
        let out = run(&p, EffectKind::ConvertToShape { shape, extra: [20., 10.], radius: 10. }).remove(0);
        let b = photocraft_pathops::bounds(&out).unwrap().unwrap();
        assert_eq!(b, (-2., 3., 118., 113.));
        let expected = match shape {
            ShapeKind::Rectangle => 13200.,
            ShapeKind::RoundedRectangle => 13200. - (4. - std::f64::consts::PI) * 100.,
            ShapeKind::Ellipse => std::f64::consts::PI * 60. * 55.,
        };
        assert!((area(&out) - expected).abs() / expected < 0.0003);
    }
}
#[test]
fn path_effects45_roughen_seed_repeatability_dpi_and_corner_smooth() {
    let p = square();
    let kind = EffectKind::Roughen { size: 3., relative: false, detail: 10., points: Points::Corner, seed: 123 };
    let a = run(&p, kind.clone());
    assert_eq!(a, run(&p, kind.clone()));
    assert_eq!(a[0].subpaths[0].knots.len(), 56);
    let mut other = kind.clone();
    if let EffectKind::Roughen { seed, .. } = &mut other {
        *seed = 124;
    }
    assert_ne!(a, run(&p, other));
    for k in &a[0].subpaths[0].knots {
        assert_eq!(k.anchor, k.in_ctrl);
        assert_eq!(k.anchor, k.out_ctrl);
    }
    let hi = evaluate(&p, &[PathEffect::new(kind)], Options { dpi: 144., ..Default::default() }).unwrap();
    assert_eq!(hi[0].subpaths[0].knots.len(), 28);
    let smooth = run(&p, EffectKind::Roughen { size: 3., relative: false, detail: 10., points: Points::Smooth, seed: 123 });
    for (a, b) in a[0].subpaths[0].knots.iter().zip(&smooth[0].subpaths[0].knots) {
        assert_eq!(a.anchor, b.anchor);
        assert!(b.smooth);
    }
}
#[test]
fn path_effects45_zigzag_line_normal_amplitude_and_pucker_handles() {
    let line = Path::new(vec![Subpath::polyline(&[(0., 0.), (100., 0.)])]);
    let out = run(&line, EffectKind::ZigZag { size: 5., relative: false, ridges: 3, points: Points::Corner }).remove(0);
    let expected = [(0., -5.), (25., 5.), (50., -5.), (75., 5.), (100., -5.)];
    for (k, (x, y)) in out.subpaths[0].knots.iter().zip(expected) {
        assert!((k.anchor.x - x).abs() < 1e-8);
        assert!((k.anchor.y - y).abs() < 1e-8);
    }
    let p = square();
    let out = run(&p, EffectKind::PuckerBloat { amount: 100. }).remove(0);
    assert_eq!(out.subpaths[0].knots[0].anchor, photocraft_geom::Point::new(33., 33.));
    assert!(out.subpaths[0].knots[0].out_ctrl.y < 8.);
}
#[test]
fn path_effects45_nonlinear_free_distort_and_twist_dense_independent_oracles() {
    let p = photocraft_vector::shapes::ellipse(10., 20., 80., 40.);
    let c = [[0., 0.], [1., 0.2], [0.8, 1.], [0.1, 0.9]];
    let out = run(&p, EffectKind::FreeDistort { corners: c }).remove(0);
    let b = photocraft_pathops::bounds(&p).unwrap().unwrap();
    for sp in &p.subpaths {
        for seg in sp.segments() {
            for i in 0..101 {
                let t = i as f64 / 100.;
                let u = 1. - t;
                let x = u * u * u * seg[0].x + 3. * u * u * t * seg[1].x + 3. * u * t * t * seg[2].x + t * t * t * seg[3].x;
                let y = u * u * u * seg[0].y + 3. * u * u * t * seg[1].y + 3. * u * t * t * seg[2].y + t * t * t * seg[3].y;
                let (sx, sy) = ((x - b.0) / (b.2 - b.0), (y - b.1) / (b.3 - b.1));
                let weights = [(1. - sx) * (1. - sy), sx * (1. - sy), sx * sy, (1. - sx) * sy];
                let q = photocraft_geom::Point::new(
                    b.0 + (b.2 - b.0) * (0..4).map(|j| c[j][0] * weights[j]).sum::<f64>(),
                    b.1 + (b.3 - b.1) * (0..4).map(|j| c[j][1] * weights[j]).sum::<f64>(),
                );
                assert!(photocraft_pathops::nearest(&out, q).unwrap().unwrap().4 <= 0.01);
            }
        }
    }
    let out = run(&p, EffectKind::Twist { angle: 120. }).remove(0);
    let (cx, cy) = (50., 40.);
    let r = 80f64.hypot(40.) / 2.;
    for sp in &p.subpaths {
        for seg in sp.segments() {
            let cb =
                CubicBez::new(Point::new(seg[0].x, seg[0].y), Point::new(seg[1].x, seg[1].y), Point::new(seg[2].x, seg[2].y), Point::new(seg[3].x, seg[3].y));
            for i in 0..101 {
                let q = cb.eval(i as f64 / 100.);
                let dx = q.x - cx;
                let dy = q.y - cy;
                let a = -120f64.to_radians() * (1. - dx.hypot(dy) / r).max(0.);
                let q = photocraft_geom::Point::new(cx + dx * a.cos() - dy * a.sin(), cy + dx * a.sin() + dy * a.cos());
                assert!(photocraft_pathops::nearest(&out, q).unwrap().unwrap().4 < 0.01);
            }
        }
    }
}
#[test]
fn path_effects45_order_toggle_targets_and_copy_paint_order() {
    let a = PathEffect::new(EffectKind::Transform { scale: [200., 100.], move_by: [0.; 2], angle: 0., reflect: [false; 2], copies: 0 });
    let b = PathEffect::new(EffectKind::OffsetPath { distance: 5., join: OffsetJoin::Miter, miter: 4. });
    let first = evaluate(&square(), &[a.clone(), b.clone()], Options::default()).unwrap();
    let second = evaluate(&square(), &[b.clone(), a.clone()], Options::default()).unwrap();
    assert_ne!(first, second);
    assert_eq!(first, evaluate(&square(), &[a, b], Options::default()).unwrap());
    let sh = ShapeLayer {
        path: square(),
        fill: Some(Fill::Solid(Color::rgba(1., 0., 0., 0.5))),
        stroke: Some(photocraft_doc::ShapeStroke { width: 2., ..Default::default() }),
        ..Default::default()
    };
    let mut e = PathEffect::new(EffectKind::Twist { angle: 90. });
    e.target = EffectTarget::Item { index: 1 };
    let scene = project(&sh, None, &[e.clone()], Options::default()).unwrap();
    assert_eq!(scene[0].path, sh.path);
    assert_ne!(scene[1].path, sh.path);
    e.enabled = false;
    assert_eq!(project(&sh, None, &[e], Options::default()).unwrap()[1].path, sh.path);
    let scene = project(
        &sh,
        None,
        &[PathEffect::new(EffectKind::Transform { scale: [100.; 2], move_by: [5., 0.], angle: 0., reflect: [false; 2], copies: 3 })],
        Options::default(),
    )
    .unwrap();
    assert_eq!(scene.iter().map(|p| p.source_item).collect::<Vec<_>>(), vec![0, 1, 0, 1, 0, 1, 0, 1]);
    for (i, paths) in scene.as_chunks::<2>().0.iter().enumerate() {
        assert_eq!(paths[0].path.subpaths[0].knots[0].anchor.x, 8. + i as f64 * 5.);
    }
}
#[test]
fn path_effects45_rejects_invalid_parameters_targets_and_growth_without_panic() {
    for e in [
        EffectKind::Twist { angle: f64::NAN },
        EffectKind::RoundCorners { radius: -1. },
        EffectKind::Roughen { size: f64::INFINITY, relative: true, detail: 10., points: Points::Smooth, seed: 0 },
    ] {
        assert!(evaluate(&square(), &[PathEffect::new(e)], Options::default()).is_err());
    }
    let e = PathEffect { target: EffectTarget::Item { index: 2 }, ..PathEffect::new(EffectKind::Twist { angle: 0. }) };
    assert_eq!(evaluate(&square(), &[e], Options::default()).unwrap_err(), Error::Target);
    let e = PathEffect::new(EffectKind::Transform { scale: [100.; 2], move_by: [0.; 2], angle: 0., reflect: [false; 2], copies: 1000 });
    assert_eq!(evaluate(&square(), &[e.clone(), e], Options::default()).unwrap_err(), Error::Budget);
    let mut p = square();
    p.subpaths[0].knots[0].anchor.x = f64::NAN;
    assert!(evaluate(&p, &[], Options::default()).is_err());
}

#[test]
fn path_effects45_tiny_free_distort_and_large_coordinate_finite_bounds() {
    for (x, size) in [(0., 0.001), (100000., 10000.)] {
        let p = photocraft_vector::shapes::rect(x, x, size, size);
        let corners = [[0., 0.], [2., 0.], [2., 1.], [0., 1.]];
        let out = run(&p, EffectKind::FreeDistort { corners }).remove(0);
        let bounds = photocraft_pathops::bounds(&out).unwrap().unwrap();
        assert!((bounds.0 - x).abs() < 1e-7);
        assert!((bounds.2 - (x + 2. * size)).abs() < 1e-7);
        assert!((bounds.3 - (x + size)).abs() < 1e-7);
        assert!(out.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).all(|p| p.x.is_finite() && p.y.is_finite()));
    }
}

#[test]
fn path_effects45_transform_scale_reflect_rotate_and_copies_independent_matrix() {
    let p = square();
    let angle = 30f64;
    let e = EffectKind::Transform { scale: [150., 50.], move_by: [7., -3.], angle, reflect: [true, false], copies: 3 };
    let paths = run(&p, e);
    assert_eq!(paths.len(), 4);
    for (i, path) in paths.iter().enumerate() {
        for (source, result) in p.subpaths[0].knots.iter().zip(&path.subpaths[0].knots) {
            let (mut x, mut y) = (source.anchor.x, source.anchor.y);
            for _ in 0..i {
                let dx = -(x - 58.) * 1.5;
                let dy = (y - 58.) * 0.5;
                let c = angle.to_radians().cos();
                let s = angle.to_radians().sin();
                x = 58. + 7. + dx * c + dy * s;
                y = 58. - 3. - dx * s + dy * c;
            }
            assert!((result.anchor.x - x).abs() < 1e-9);
            assert!((result.anchor.y - y).abs() < 1e-9);
        }
    }
}
