//! Package 41 independent geometry oracles; synthetic GPL-3.0-or-later inputs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use kurbo::{CubicBez, ParamCurve};
use photocraft_doc::{FillRule, PathOp, Subpath};
fn square() -> Path {
    Path::new(vec![Subpath::polygon(&[(-20., -20.), (20., -20.), (20., 20.), (-20., 20.)])])
}
// Polygon integral over dense cubic evaluations, independent of the deformation/refitter.
fn area(path: &Path) -> f64 {
    path.subpaths
        .iter()
        .map(|sp| {
            let points: Vec<_> = sp
                .segments()
                .iter()
                .flat_map(|s| {
                    let c = CubicBez::new((s[0].x, s[0].y), (s[1].x, s[1].y), (s[2].x, s[2].y), (s[3].x, s[3].y));
                    (0..128).map(move |i| c.eval(i as f64 / 128.))
                })
                .collect();
            points.iter().zip(points.iter().cycle().skip(1)).take(points.len()).map(|(a, b)| a.x * b.y - a.y * b.x).sum::<f64>() / 2.
        })
        .sum::<f64>()
        .abs()
}
#[test]
fn distort41_zero_intensity_and_zero_pressure_are_exact_identity_all_tools() {
    let mut p = square();
    p.subpaths.push(Subpath::polyline(&[(-100., 30.), (100., 30.)]).with_op(PathOp::Subtract));
    p.fill_rule = FillRule::EvenOdd;
    p.inverted = true;
    for kind in [Kind::Warp, Kind::Twirl, Kind::Pucker, Kind::Bloat, Kind::Scallop, Kind::Crystallize, Kind::Wrinkle] {
        for pressure in [false, true] {
            let opts = Options { intensity: 0., use_pressure: pressure, detail: 10., complexity: 15., ..Options::new(kind) };
            assert_eq!(apply(&p, &[Sample::new(0., 0., 1.), Sample::new(15., 10., 0.4)], opts, 7).unwrap(), p);
        }
        let opts = Options { use_pressure: true, ..Options::new(kind) };
        assert_eq!(apply(&p, &[Sample::new(0., 0., 0.), Sample::new(15., 10., 0.)], opts, 7).unwrap(), p);
    }
}
#[test]
fn distort41_bloat_and_pucker_area_is_monotone_with_intensity_and_repeated_dabs() {
    let p = square();
    let original = area(&p);
    for kind in [Kind::Bloat, Kind::Pucker] {
        let mut last = original;
        for intensity in [0.1, 0.3, 0.5, 0.8, 1.] {
            let out =
                apply(&p, &[Sample::new(0., 0., 1.)], Options { width: 160., height: 160., intensity, simplify_on: false, ..Options::new(kind) }, 1).unwrap();
            let a = area(&out);
            if kind == Kind::Bloat {
                assert!(a > last, "{a} <= {last}");
            } else {
                assert!(a < last, "{a} >= {last}");
            }
            last = a;
        }
        let opts = Options { width: 160., height: 160., simplify_on: false, ..Options::new(kind) };
        let mut last = original;
        for n in 1..5 {
            let out = apply(&p, &vec![Sample::new(0., 0., 1.); n], opts, 1).unwrap();
            let a = area(&out);
            if kind == Kind::Bloat {
                assert!(a > last);
            } else {
                assert!(a < last);
            }
            last = a;
        }
    }
}
#[test]
fn distort41_warp_adaptively_bends_a_line_and_preserves_unreached_subpaths() {
    let mut p = Path::new(vec![Subpath::polyline(&[(0., 0.), (200., 0.)]), Subpath::polyline(&[(0., 200.), (200., 200.)]).with_op(PathOp::Exclude)]);
    p.fill_rule = FillRule::EvenOdd;
    let out =
        apply(&p, &[Sample::new(100., 0., 1.), Sample::new(100., 20., 1.)], Options { width: 60., height: 80., simplify_on: false, ..Default::default() }, 0)
            .unwrap();
    assert!(out.subpaths[0].knots.len() > 2);
    assert!(out.subpaths[0].knots.iter().any(|k| k.anchor.y > 4.));
    assert_eq!(out.subpaths[1], p.subpaths[1]);
    assert_eq!(out.fill_rule, p.fill_rule);
    assert_eq!(out.subpaths[0].knots[0].anchor, p.subpaths[0].knots[0].anchor);
    let long = Path::new(vec![Subpath::polyline(&[(0., 0.), (100_000., 0.)])]);
    let out = apply(
        &long,
        &[Sample::new(11_111., 0., 1.), Sample::new(11_111., 1., 1.)],
        Options { width: 4., height: 4., intensity: 1., simplify_on: false, ..Default::default() },
        0,
    )
    .unwrap();
    assert!(out.subpaths[0].knots.iter().any(|k| k.anchor.y > 0.01), "narrow brush must find a long curve between sample locations");
}
#[test]
fn distort41_noisy_tools_seed_repeatability_global_budget_and_256_generated_paths() {
    for case in 0..256 {
        let points = (0..8)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 8.;
                let r = 10. + ((case * 17 + i * 23) % 40) as f64;
                (a.cos() * r, a.sin() * r)
            })
            .collect::<Vec<_>>();
        let mut p = Path::new(vec![Subpath::polygon(&points), Subpath::polyline(&[(-70., 3.), (70., 3.)])]);
        p.subpaths[1].op = PathOp::Join;
        for kind in [Kind::Scallop, Kind::Crystallize, Kind::Wrinkle] {
            let opts = Options {
                width: 120.,
                height: 90.,
                angle: (case % 180) as f64,
                intensity: 0.7,
                detail: 10.,
                complexity: 15.,
                horizontal: 1.,
                node_budget: 64,
                ..Options::new(kind)
            };
            let samples = [Sample::new(0., 0., 1.), Sample::new(8., 4., 0.4)];
            let a = apply(&p, &samples, opts, case as u64).unwrap();
            assert_eq!(a, apply(&p, &samples, opts, case as u64).unwrap());
            validate_path(&a, 64).unwrap();
            assert_eq!(a.subpaths[1].op, PathOp::Join);
            if kind == Kind::Wrinkle {
                assert_ne!(a, apply(&p, &samples, opts, case as u64 + 1).unwrap());
            }
        }
    }
}
#[test]
fn distort41_incremental_preview_equals_replay_and_pressure_scales_intensity() {
    let p = square();
    let opts = Options { kind: Kind::Warp, use_pressure: true, simplify_on: true, ..Default::default() };
    let samples = [Sample::new(20., 0., 0.2), Sample::new(30., 8., 0.5), Sample::new(40., 4., 1.)];
    let mut stroke = Stroke::new(p.clone(), opts, 0).unwrap();
    for i in 0..samples.len() {
        stroke.push(samples[i]).unwrap();
        assert_eq!(stroke.path().unwrap(), apply(&p, &samples[..=i], opts, 0).unwrap());
    }
    let low = apply(&p, &[Sample::new(20., 0., 0.2), Sample::new(40., 0., 0.2)], opts, 0).unwrap();
    let high = apply(&p, &[Sample::new(20., 0., 1.), Sample::new(40., 0., 1.)], opts, 0).unwrap();
    let max_x = |p: &Path| p.subpaths.iter().flat_map(|s| &s.knots).map(|k| k.anchor.x).fold(f64::NEG_INFINITY, f64::max);
    assert!(max_x(&high) > max_x(&low));
}
#[test]
fn distort41_rejects_nonfinite_and_unbounded_settings_samples_and_work() {
    let p = square();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1., 1_000_001.] {
        assert!(apply(&p, &[Sample::new(0., 0., 1.)], Options { intensity: value, ..Default::default() }, 0).is_err());
    }
    assert!(apply(&p, &[Sample::new(f64::NAN, 0., 1.)], Default::default(), 0).is_err());
    assert!(apply(&p, &[Sample::new(0., 0., -0.1)], Default::default(), 0).is_err());
    assert!(apply(&p, &[Sample::new(-1000., 0., 1.), Sample::new(1000., 0., 1.)], Options { width: 0.5, height: 0.5, ..Default::default() }, 0).is_err());
    let defaults: Options = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults, Options::default());
    assert!(serde_json::from_str::<Options>(r#"{"extra":1}"#).is_err());
}
