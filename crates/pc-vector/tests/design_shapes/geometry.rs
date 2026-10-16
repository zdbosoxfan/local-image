//! Independent area/length oracles. Synthetic fixtures: see SOURCES.md.
use super::geometry;
use photocraft_doc::vector_design::drawing::{CornerKind, DesignShape, ShapeParameters as S};
use photocraft_doc::{Path, PathOp};
use std::f64::consts::{PI, TAU};

pub fn families() -> Vec<S> {
    vec![
        S::Rect { radii: [0.; 4], kinds: [CornerKind::Round; 4] },
        S::Ellipse,
        S::Polygon { sides: 6, curved: 0., corner: 0. },
        S::Star { points: 5, inner: 0.4, corner: 0. },
        S::DoubleStar { points: 5, inner: 0.3, secondary: 0.7, corner: 0. },
        S::SquareStar { points: 4, inner: 0.4, corner: 0. },
        S::Triangle { top: 0.25 },
        S::Trapezoid { top_left: 0.2, top_right: 0.8 },
        S::Diamond { top: 0.3 },
        S::Donut { hole: 0.5 },
        S::Pie { start: 0., end: PI },
        S::Segment { start: 0., end: PI, inner: 0.5 },
        S::Arrow { from: [10., 50.], to: [90., 50.], head_width: 30., head_length: 20., shaft: 10., double: false },
        S::Callout { ellipse: false, tail: [50., 130.], radius: 0. },
        S::Callout { ellipse: true, tail: [50., 130.], radius: 0. },
        S::Cog { teeth: 8, inner: 0.7, hole: 0.2, curve: 0. },
        S::Heart { depth: 0.5 },
        S::Tear { bend: 0. },
        S::Crescent { offset: 1. },
        S::Cloud { bubbles: 6 },
        S::Spiral { center: [50., 50.], radius: 40., decay: 0.8, turns: 3. },
        S::Arc { start: 0., end: PI },
        S::Line { from: [10., 10.], to: [90., 70.] },
        S::RectGrid { rows: 3, cols: 4 },
        S::PolarGrid { rings: 3, spokes: 8 },
    ]
}
fn shape(parameters: S) -> DesignShape {
    DesignShape { rect: [0., 0., 100., 100.], angle: 0., parameters }
}
// Dense cubic quadrature is independent of the shape's anchor construction and rasterizer.
fn area(path: &Path) -> f64 {
    path.subpaths
        .iter()
        .filter(|s| s.closed)
        .map(|sp| {
            let mut sum = 0.;
            for q in sp.segments() {
                let mut last = q[0];
                for i in 1..=1024 {
                    let t = i as f64 / 1024.;
                    let u = 1. - t;
                    let x = u * u * u * q[0].x + 3. * u * u * t * q[1].x + 3. * u * t * t * q[2].x + t * t * t * q[3].x;
                    let y = u * u * u * q[0].y + 3. * u * u * t * q[1].y + 3. * u * t * t * q[2].y + t * t * t * q[3].y;
                    sum += last.x * y - x * last.y;
                    last = photocraft_geom::Point::new(x, y);
                }
            }
            sum / 2.
        })
        .sum::<f64>()
        .abs()
}
fn near(actual: f64, expected: f64, tol: f64) {
    assert!((actual - expected).abs() <= tol, "{actual} vs {expected} (tol {tol})");
}
#[test]
fn live_shapes_angle_handles_preserve_clockwise_wrapped_and_full_sweeps() {
    use geometry::HandleKind;
    for (start, end) in [(1., -PI), (6. * TAU + 0.2, 6. * TAU + PI), (0.7, 0.7 - TAU)] {
        let mut s = shape(S::Arc { start, end });
        s.angle = 0.4;
        for kind in [HandleKind::Start, HandleKind::End] {
            let h = geometry::handles(&s).unwrap().into_iter().find(|h| h.kind == kind).unwrap();
            let edited = geometry::drag_handle(&s, kind, h.position).unwrap();
            let S::Arc { start: actual_start, end: actual_end } = edited.parameters else { panic!("arc required") };
            near(actual_start, start, 1e-10);
            near(actual_end, end, 1e-10);
        }
    }
    let s = shape(S::Arc { start: 0., end: -PI });
    let edited = geometry::drag_handle(&s, HandleKind::End, photocraft_geom::Point::new(50., 0.)).unwrap();
    assert!(matches!(edited.parameters, S::Arc { end, .. } if (end + PI / 2.).abs() < 1e-10));
}
#[test]
fn live_shapes_analytic_areas_and_holes() {
    for (params, expected) in [
        (S::Rect { radii: [0.; 4], kinds: [CornerKind::Round; 4] }, 10000.),
        (S::Ellipse, 2500. * PI),
        (S::Polygon { sides: 6, curved: 0., corner: 0. }, 3. * 2500. * (TAU / 6.).sin()),
        (S::Star { points: 5, inner: 0.4, corner: 0. }, 5. * 2500. * 0.4 * (PI / 5.).sin()),
        (S::DoubleStar { points: 5, inner: 0.3, secondary: 0.7, corner: 0. }, 5. * 2500. * 0.3 * 1.7 * (PI / 10.).sin()),
        (S::SquareStar { points: 4, inner: 0.4, corner: 0. }, 4. * 1250. * (1. + 0.4 * 0.4) * (PI / 4.).sin()),
        (S::Triangle { top: 0.25 }, 5000.),
        (S::Trapezoid { top_left: 0.2, top_right: 0.8 }, 8000.),
        (S::Diamond { top: 0.3 }, 5000.),
        (S::Donut { hole: 0.5 }, 2500. * PI * 0.75),
        (S::Pie { start: 0., end: PI }, 1250. * PI),
        (S::Segment { start: 0., end: PI, inner: 0.5 }, 1250. * PI * 0.75),
        (S::Arrow { from: [10., 50.], to: [90., 50.], head_width: 30., head_length: 20., shaft: 10., double: false }, 900.),
        (S::Cog { teeth: 8, inner: 0.7, hole: 0.2, curve: 0. }, 4. * 2500. * 1.7_f64.powi(2) * (TAU / 32.).sin() - 2500. * PI * 0.04),
        (S::Crescent { offset: 1. }, 2500. * (PI / 3. + 3_f64.sqrt() / 2.)),
        (S::Cloud { bubbles: 6 }, 2500. * PI * (0.85 * 0.85 + 0.15 * 0.15 / 2.)),
    ] {
        near(area(&geometry::generate(&shape(params.clone())).unwrap()), expected, expected * 0.0004 + 0.1);
    }
    for params in [S::Donut { hole: 0.5 }, S::Cog { teeth: 8, inner: 0.7, hole: 0.2, curve: 0. }, S::Segment { start: 0., end: TAU, inner: 0.5 }] {
        let path = geometry::generate(&shape(params)).unwrap();
        assert_eq!(path.subpaths[1].op, PathOp::Join);
        let coverage = photocraft_vector::path_coverage(&path, photocraft_geom::Rect::new(49, 49, 2, 2));
        assert!(coverage.iter().all(|v| *v < 0.001));
    }
}
#[test]
fn live_shapes_corner_kinds_have_distinct_analytic_cuts() {
    assert!(geometry::generate(&shape(S::Rect { radii: [50.; 4], kinds: [CornerKind::Straight; 4] })).is_err());
    for (kind, cut) in [(CornerKind::Round, 100. - 25. * PI), (CornerKind::Inverted, 25. * PI), (CornerKind::Chamfer, 50.), (CornerKind::Straight, 100.)] {
        let s = shape(S::Rect { radii: [10.; 4], kinds: [kind; 4] });
        near(area(&geometry::generate(&s).unwrap()), 10000. - 4. * cut, 0.1);
    }
}
#[test]
fn live_shapes_every_family_is_finite_rotates_and_has_handles() {
    for params in families() {
        let mut s = shape(params);
        let path = geometry::generate(&s).unwrap();
        assert!(!path.is_empty(), "{s:?}");
        assert!(!geometry::handles(&s).unwrap().is_empty());
        assert!(path.subpaths.iter().flat_map(|sp| &sp.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).all(|p| p.x.is_finite() && p.y.is_finite()));
        s.angle = PI / 2.;
        let rotated = geometry::generate(&s).unwrap();
        near(area(&path), area(&rotated), 0.0001);
        for (a, b) in path.subpaths.iter().flat_map(|s| &s.knots).zip(rotated.subpaths.iter().flat_map(|s| &s.knots)) {
            near(b.anchor.x, 100. - a.anchor.y, 1e-9);
            near(b.anchor.y, a.anchor.x, 1e-9);
        }
    }
}
#[test]
fn live_shapes_bounds_and_grid_lengths() {
    for params in [
        S::Rect { radii: [10.; 4], kinds: [CornerKind::Round; 4] },
        S::Ellipse,
        S::Donut { hole: 0.5 },
        S::RectGrid { rows: 3, cols: 4 },
        S::PolarGrid { rings: 3, spokes: 8 },
    ] {
        let s = shape(params);
        let path = geometry::generate(&s).unwrap();
        let bounds = path.control_bounds().unwrap();
        for (actual, expected) in [bounds.0, bounds.1, bounds.2, bounds.3].into_iter().zip([0., 0., 100., 100.]) {
            near(actual, expected, 1e-9);
        }
    }
    let grid = geometry::generate(&shape(S::RectGrid { rows: 3, cols: 4 })).unwrap();
    assert_eq!(grid.subpaths.len(), 6);
    let length: f64 = grid.subpaths.iter().flat_map(|s| s.segments()).map(|q| (q[3].x - q[0].x).hypot(q[3].y - q[0].y)).sum();
    near(length, 900., 1e-9);
    let grid = geometry::generate(&shape(S::PolarGrid { rings: 3, spokes: 8 })).unwrap();
    assert_eq!(grid.subpaths.len(), 11);
    assert_eq!(grid.subpaths.iter().filter(|s| !s.closed).count(), 8);
}
#[test]
fn live_shapes_all_family_analytic_bounds() {
    for params in families() {
        // Extrema of the mathematical fixtures, independently of knots/control bounds.
        let expected = match &params {
            S::Rect { .. }
            | S::Ellipse
            | S::Triangle { .. }
            | S::Trapezoid { .. }
            | S::Diamond { .. }
            | S::Donut { .. }
            | S::Cog { .. }
            | S::RectGrid { .. }
            | S::PolarGrid { .. } => [0., 0., 100., 100.],
            S::Polygon { .. } => [50. - 25. * 3_f64.sqrt(), 0., 50. + 25. * 3_f64.sqrt(), 100.],
            S::Star { .. } | S::DoubleStar { .. } => [50. - 50. * (PI / 10.).cos(), 0., 50. + 50. * (PI / 10.).cos(), 50. + 50. * (PI / 5.).cos()],
            S::SquareStar { .. } => {
                let r = 50. * (PI / 8.).cos();
                [50. - r, 50. - r, 50. + r, 50. + r]
            }
            S::Pie { .. } | S::Segment { .. } | S::Arc { .. } => [0., 50., 100., 100.],
            S::Arrow { .. } => [10., 35., 90., 65.],
            S::Callout { .. } => [0., 0., 100., 130.],
            S::Heart { .. } => {
                // Upper-lobe y(t)=25-150t+225t²-65t³; solve y'(t)=0.
                let t = (450. - 85500_f64.sqrt()) / 390.;
                [0., 25. - 150. * t + 225. * t * t - 65. * t * t * t, 100., 100.]
            }
            S::Tear { .. } => [12.5, 0., 87.5, 100.], // x(t)=50 +/-150t(1-t).
            S::Crescent { .. } => [0., 0., 75., 100.],
            S::Cloud { .. } => {
                // Six-bubble polar radius r(a)=.85-.15cos(6a). Solve d(r cos(a))/da=0.
                let (mut lo, mut hi) = (0.01, PI / 6.);
                for _ in 0..64 {
                    let a = (lo + hi) / 2.;
                    let r = 0.85 - 0.15 * (6. * a).cos();
                    if 0.9 * (6. * a).sin() * a.cos() - r * a.sin() > 0. {
                        lo = a;
                    } else {
                        hi = a;
                    }
                }
                let a = (lo + hi) / 2.;
                let r = 50. * (0.85 - 0.15 * (6. * a).cos()) * a.cos();
                [50. - r, 0., 50. + r, 100.]
            }
            S::Spiral { .. } => {
                // r=40exp(k*a); coordinate extrema solve tan(a)=k and tan(a)=-1/k.
                let k = 0.8_f64.ln() / (PI / 2.);
                let ax = PI + k.atan();
                let ay = (-1. / k).atan();
                [50. + 40. * (k * ax).exp() * ax.cos(), 50. + 40. * (k * (ay + PI)).exp() * (ay + PI).sin(), 90., 50. + 40. * (k * ay).exp() * ay.sin()]
            }
            S::Line { .. } => [10., 10., 90., 70.],
        };
        let path = geometry::generate(&shape(params.clone())).unwrap();
        let mut actual = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for q in path.subpaths.iter().flat_map(|sp| sp.segments()) {
            for i in 0..=1024 {
                let t = i as f64 / 1024.;
                let u = 1. - t;
                let x = u.powi(3) * q[0].x + 3. * u * u * t * q[1].x + 3. * u * t * t * q[2].x + t.powi(3) * q[3].x;
                let y = u.powi(3) * q[0].y + 3. * u * u * t * q[1].y + 3. * u * t * t * q[2].y + t.powi(3) * q[3].y;
                actual[0] = actual[0].min(x);
                actual[1] = actual[1].min(y);
                actual[2] = actual[2].max(x);
                actual[3] = actual[3].max(y);
            }
        }
        for (a, e) in actual.into_iter().zip(expected) {
            assert!((a - e).abs() <= 0.025, "{params:?}: {actual:?} vs {expected:?}");
        }
    }
}
#[test]
fn live_shapes_reject_nonfinite_degenerate_and_unbounded_inputs() {
    for params in families() {
        let valid = shape(params);
        for v in [f64::NAN, f64::INFINITY, -f64::INFINITY, 1e20] {
            let mut s = valid.clone();
            s.angle = v;
            assert!(geometry::generate(&s).is_err());
            for i in 0..4 {
                let mut s = valid.clone();
                s.rect[i] = v;
                assert!(geometry::generate(&s).is_err());
            }
        }
        for i in [2, 3] {
            let mut s = valid.clone();
            s.rect[i] = 0.;
            assert!(geometry::generate(&s).is_err());
        }
    }
    for params in [
        S::Star { points: 0, inner: 0.5, corner: 0. },
        S::Star { points: 5, inner: 0., corner: 0. },
        S::Donut { hole: 1. },
        S::Pie { start: 0., end: 0. },
        S::Cloud { bubbles: u32::MAX },
        S::RectGrid { rows: 0, cols: 1 },
        S::Spiral { center: [0.; 2], radius: 1., decay: 0., turns: 1. },
        S::Line { from: [0.; 2], to: [0.; 2] },
    ] {
        assert!(geometry::generate(&shape(params)).is_err());
    }
}
#[test]
fn live_shapes_optional_parameters_change_geometry() {
    for (a, b) in [
        (S::Polygon { sides: 5, curved: 0., corner: 0. }, S::Polygon { sides: 5, curved: 0.4, corner: 4. }),
        (S::Star { points: 5, inner: 0.5, corner: 0. }, S::Star { points: 5, inner: 0.5, corner: 4. }),
        (S::Cog { teeth: 8, inner: 0.7, hole: 0.2, curve: 0. }, S::Cog { teeth: 8, inner: 0.7, hole: 0.2, curve: 0.5 }),
        (S::Heart { depth: 0.2 }, S::Heart { depth: 0.8 }),
        (S::Tear { bend: 0. }, S::Tear { bend: 0.6 }),
    ] {
        assert_ne!(geometry::generate(&shape(a)).unwrap(), geometry::generate(&shape(b)).unwrap());
    }
}
#[test]
fn live_shapes_arcs_spirals_and_callouts_geometry_oracles() {
    let callout = geometry::generate(&shape(S::Callout { ellipse: false, tail: [50., 130.], radius: 0. })).unwrap();
    near(area(&callout), 10300., 1e-6);
    let arc = geometry::generate(&shape(S::Arc { start: 0., end: PI })).unwrap();
    assert!(!arc.subpaths[0].closed);
    let k = &arc.subpaths[0].knots;
    near(k[0].anchor.x, 100., 1e-9);
    near(k[0].anchor.y, 50., 1e-9);
    near(k.last().unwrap().anchor.x, 0., 1e-9);
    near(k.last().unwrap().anchor.y, 50., 1e-9);
    let spiral = geometry::generate(&shape(S::Spiral { center: [50., 50.], radius: 40., decay: 0.8, turns: 3. })).unwrap();
    assert!(!spiral.subpaths[0].closed);
    let k = &spiral.subpaths[0].knots;
    near(k[0].anchor.x, 90., 1e-9);
    near(k.last().unwrap().anchor.x, 50. + 40. * 0.8_f64.powi(12), 1e-9);
    near(k.last().unwrap().anchor.y, 50., 1e-9);
    let line = geometry::generate(&shape(S::Line { from: [10., 10.], to: [90., 70.] })).unwrap();
    assert!(!line.subpaths[0].closed);
    near(
        (line.subpaths[0].knots[1].anchor.x - line.subpaths[0].knots[0].anchor.x)
            .hypot(line.subpaths[0].knots[1].anchor.y - line.subpaths[0].knots[0].anchor.y),
        100.,
        1e-9,
    );
    let chord = geometry::generate(&shape(S::Segment { start: 0., end: PI / 2., inner: 0. })).unwrap();
    near(area(&chord), 1250. * (PI / 2. - 1.), 0.3);
}
#[test]
fn live_shapes_custom_curve_moments_and_every_parameter_is_bounded() {
    // Exact Green integrals for the declared original cubic templates (SOURCES.md).
    near(area(&geometry::generate(&shape(S::Heart { depth: 0.5 })).unwrap()), 2500. * 348. / 125., 0.02);
    for bend in [0., 0.5, -0.5] {
        near(area(&geometry::generate(&shape(S::Tear { bend })).unwrap()), 2500. * 219. / 100., 0.02);
    }
    fn corrupt_numbers(value: &serde_json::Value) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        match value {
            serde_json::Value::Number(_) => out.push(serde_json::json!(1e300)),
            serde_json::Value::Array(a) => {
                for (i, v) in a.iter().enumerate() {
                    for bad in corrupt_numbers(v) {
                        let mut changed = a.clone();
                        changed[i] = bad;
                        out.push(serde_json::Value::Array(changed));
                    }
                }
            }
            serde_json::Value::Object(a) => {
                for (k, v) in a {
                    for bad in corrupt_numbers(v) {
                        let mut changed = a.clone();
                        changed.insert(k.clone(), bad);
                        out.push(serde_json::Value::Object(changed));
                    }
                }
            }
            _ => {}
        }
        out
    }
    for params in families() {
        for value in corrupt_numbers(&serde_json::json!(params)) {
            if let Ok(parameters) = serde_json::from_value::<S>(value) {
                assert!(geometry::generate(&shape(parameters)).is_err());
            }
        }
    }
}
#[test]
fn live_shapes_256_deterministic_rotation_properties() {
    let cases = families();
    let mut seed = 0x0202_u64;
    for i in 0..256 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut s = shape(cases[i % cases.len()].clone());
        let base = geometry::generate(&s).unwrap();
        s.angle = ((seed >> 32) as f64 / u32::MAX as f64 - 0.5) * TAU * 100.;
        let result = geometry::generate(&s).unwrap();
        near(area(&base), area(&result), 1e-6);
        assert_eq!(base.subpaths.len(), result.subpaths.len());
        assert!(result.subpaths.iter().flat_map(|sp| &sp.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).all(|p| p.x.is_finite() && p.y.is_finite()));
    }
}
