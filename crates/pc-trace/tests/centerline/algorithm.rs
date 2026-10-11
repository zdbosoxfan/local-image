#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_testkit::vector::is_finite;
use proptest::prelude::*;

fn params() -> Params {
    Params { adaptive: false, speckle: 0, ..Params::for_preset(Preset::LineArt) }
}
fn pixels(w: usize, h: usize, on: impl Fn(usize, usize) -> bool) -> Vec<u8> {
    (0..w * h).flat_map(|i| if on(i % w, i / w) { [0, 0, 0, 255] } else { [255; 4] }).collect()
}
fn run(w: usize, h: usize, data: &[u8]) -> CenterlineTrace {
    trace(Image { width: w as u32, height: h as u32, rgba: data }, &params()).unwrap()
}
fn valid(out: &CenterlineTrace) {
    for s in &out.strokes {
        assert!(is_finite(&s.path));
        assert_eq!(s.path.subpaths.len(), 1);
        assert!(!s.path.subpaths[0].closed);
        assert!(s.path.subpaths[0].knots.len() >= 2);
        assert!(s.stroke.width.is_finite() && s.stroke.width >= 1.);
        if let Some(p) = &s.profile {
            assert_eq!(p.points.first().unwrap().t, 0.);
            assert!((p.points.last().unwrap().t - 1.).abs() < 1e-12);
            assert!(p.points.windows(2).all(|p| p[0].t < p[1].t));
            assert!(p.points.iter().all(|p| p.left.is_finite() && p.right.is_finite() && p.left >= 0.5 && p.right >= 0.5));
        }
    }
}
#[test]
fn centerline_exact_distance_matches_exhaustive_euclidean_oracle() {
    // Independent oracle enumerates all background sites, including the exterior.
    for (w, h) in [(1, 1), (1, 19), (19, 1), (23, 17)] {
        let mask: Vec<bool> = (0..w * h).map(|i| (i * 37 + i / 7) % 11 != 0).collect();
        let mut c = Control { cancel: &|| false, remaining: WORK_BUDGET };
        let actual = distance::squared(&mask, w, h, &mut c).unwrap();
        for y in 0..h {
            for x in 0..w {
                let mut expected = u32::MAX;
                for by in -1..=h as i32 {
                    for bx in -1..=w as i32 {
                        if bx >= 0 && by >= 0 && bx < w as i32 && by < h as i32 && mask[by as usize * w + bx as usize] {
                            continue;
                        }
                        let (dx, dy) = (x as i32 - bx, y as i32 - by);
                        expected = expected.min((dx * dx + dy * dy) as u32);
                    }
                }
                assert_eq!(actual.squared[y * w + x], expected, "{w}x{h} at {x},{y}");
                let (bx, by) = actual.sites[y * w + x];
                assert_eq!(((x as i32 - bx).pow(2) + (y as i32 - by).pow(2)) as u32, expected);
            }
        }
    }
}
#[test]
fn centerline_axis_widths_and_centres_have_analytic_odd_even_oracle() {
    for thick in 1..=10 {
        for vertical in [false, true] {
            let data = pixels(80, 80, |x, y| {
                let (along, across) = if vertical { (y, x) } else { (x, y) };
                (8..72).contains(&along) && (30..30 + thick).contains(&across)
            });
            let out = run(80, 80, &data);
            valid(&out);
            assert_eq!(out.strokes.len(), 1, "thickness {thick}, vertical {vertical}");
            let stroke = &out.strokes[0];
            assert_eq!(stroke.stroke.width, thick as f32);
            // A pixel strip [30,30+thick] has the geometric midline below.
            let midline = 30. + thick as f64 / 2.;
            let sub = &stroke.path.subpaths[0];
            for knot in &sub.knots {
                let (along, across) = if vertical { (knot.anchor.y, knot.anchor.x) } else { (knot.anchor.x, knot.anchor.y) };
                if (20. ..60.).contains(&along) {
                    assert!((across - midline).abs() <= 0.05, "{across} vs {midline}");
                }
            }
        }
    }
}
#[test]
fn centerline_junctions_retain_all_arms_and_shared_endpoints() {
    for arms in [3, 4] {
        let data =
            pixels(33, 33, |x, y| ((4..29).contains(&x) && y == 16) || (x == 16 && (4..=16).contains(&y)) || (arms == 4 && x == 16 && (17..29).contains(&y)));
        let out = run(33, 33, &data);
        valid(&out);
        assert_eq!(out.strokes.len(), arms);
        let mut endpoints = Vec::new();
        for stroke in &out.strokes {
            let k = &stroke.path.subpaths[0].knots;
            endpoints.extend([k.first().unwrap().anchor, k.last().unwrap().anchor]);
        }
        let junction = endpoints.iter().find(|p| endpoints.iter().filter(|q| *q == *p).count() == arms).unwrap();
        assert!((junction.x - 16.5).abs() < 1. && (junction.y - 16.5).abs() < 1., "{junction:?}, endpoints {endpoints:?}");
        for tip in [(4.5, 16.5), (28.5, 16.5), (16.5, 4.5)] {
            assert!(endpoints.iter().any(|p| (p.x - tip.0).hypot(p.y - tip.1) <= 0.01));
        }
    }
}
#[test]
fn centerline_loop_remains_an_open_path_with_no_missing_seam() {
    // Diamond ring has a known 8-connected cycle and no junctions.
    let data = pixels(31, 31, |x, y| x.abs_diff(15) + y.abs_diff(15) == 10);
    let out = run(31, 31, &data);
    valid(&out);
    assert_eq!(out.strokes.len(), 1);
    let s = &out.strokes[0].path.subpaths[0];
    assert_eq!(s.knots.first().unwrap().anchor, s.knots.last().unwrap().anchor);
    assert!(s.knots.len() >= 4);
}
// Adapted behavioural cases from VectorCraft crates/trace/src/tests.rs at the
// same pin as the port (MIT OR Apache-2.0, ArtCraft Team and contributors).
#[test]
fn centerline_upstream_diagonal_capsule_and_circular_ring() {
    use kurbo::{ParamCurve, ParamCurveArclen};
    let data = pixels(200, 120, |x, y| {
        let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
        let t = (((px - 20.) * 160. + (py - 20.) * 80.) / (160f64.powi(2) + 80f64.powi(2))).clamp(0., 1.);
        (px - 20. - 160. * t).hypot(py - 20. - 80. * t) <= 1.5
    });
    let p = Params { detail: 0.8, ..params() };
    let out = trace(Image { width: 200, height: 120, rgba: &data }, &p).unwrap();
    valid(&out);
    assert_eq!(out.strokes.len(), 1);
    assert!((2. ..=4.).contains(&f64::from(out.strokes[0].stroke.width)));
    let knots = &out.strokes[0].path.subpaths[0].knots;
    // The shared T1 fitter differs from VC's line-snapping fitter: allow a few
    // cubics, and independently bound their deviation from the analytic line.
    assert!(knots.len() <= 8, "{} anchors", knots.len());
    for c in out.strokes[0].path.subpaths[0].segments() {
        let pt = |p: photocraft_geom::Point| kurbo::Point::new(p.x, p.y);
        let curve = kurbo::CubicBez::new(pt(c[0]), pt(c[1]), pt(c[2]), pt(c[3]));
        for i in 0..=64 {
            let p = curve.eval(i as f64 / 64.);
            let t = (((p.x - 20.) * 160. + (p.y - 20.) * 80.) / (160f64.powi(2) + 80f64.powi(2))).clamp(0., 1.);
            assert!((p.x - 20. - 160. * t).hypot(p.y - 20. - 80. * t) <= 1.5, "deviation at {p:?}");
        }
    }
    for want in [(20., 20.), (180., 100.)] {
        assert!([knots.first().unwrap(), knots.last().unwrap()].iter().any(|k| (k.anchor.x - want.0).hypot(k.anchor.y - want.1) < 3.));
    }
    let data = pixels(120, 120, |x, y| ((x as f64 + 0.5 - 60.).hypot(y as f64 + 0.5 - 60.) - 40.).abs() <= 1.5);
    let out = trace(Image { width: 120, height: 120, rgba: &data }, &p).unwrap();
    valid(&out);
    assert_eq!(out.strokes.len(), 1, "ring topology");
    let length: f64 = out.strokes[0].path.subpaths[0]
        .segments()
        .into_iter()
        .map(|c| {
            let pt = |p: photocraft_geom::Point| kurbo::Point::new(p.x, p.y);
            kurbo::CubicBez::new(pt(c[0]), pt(c[1]), pt(c[2]), pt(c[3])).arclen(1e-6)
        })
        .sum();
    assert!((length - std::f64::consts::TAU * 40.).abs() / (std::f64::consts::TAU * 40.) < 0.05, "ring length {length}");
}
#[test]
fn centerline_changing_strip_width_retains_a_distance_profile() {
    let data = pixels(100, 50, |x, y| (10..90).contains(&x) && y.abs_diff(25) <= if x < 50 { 1 } else { 3 });
    let out = run(100, 50, &data);
    valid(&out);
    assert_eq!(out.strokes.len(), 1);
    let stroke = &out.strokes[0];
    let profile = stroke.profile.as_ref().expect("changing width");
    assert_eq!(stroke.stroke.width, 7.);
    assert!(profile.points.iter().any(|p| (p.left - 1.5).abs() < 1e-12));
    assert!(profile.points.iter().any(|p| (p.left - 3.5).abs() < 1e-12));
}
#[test]
fn centerline_singletons_thin_transparent_and_speckle() {
    for (w, h) in [(1, 1), (1, 65), (65, 1)] {
        let data = pixels(w, h, |_, _| true);
        let out = run(w, h, &data);
        valid(&out);
        assert_eq!(out.strokes.len(), 1);
        assert_eq!(out.strokes[0].stroke.width, 1.);
        let transparent = vec![0; w * h * 4];
        assert!(run(w, h, &transparent).strokes.is_empty());
    }
    let mut p = params();
    p.speckle = 2;
    assert!(trace(Image { width: 1, height: 1, rgba: &[0, 0, 0, 255] }, &p).unwrap().strokes.is_empty());
    p.speckle = 0;
    p.ignore_color = Some([0; 3]);
    assert!(trace(Image { width: 1, height: 1, rgba: &[0, 0, 0, 255] }, &p).unwrap().strokes.is_empty());
    p.ignore_color = None;
    p.use_alpha = false;
    assert_eq!(trace(Image { width: 1, height: 1, rgba: &[0; 4] }, &p).unwrap().strokes.len(), 1);
}
#[test]
fn centerline_cancels_inside_every_long_stage_and_enforces_budgets() {
    let data = pixels(128, 128, |x, y| (20..108).contains(&x) && (35..93).contains(&y));
    for limit in [0, 5, 20, 60, 120, 250, 400] {
        let count = std::cell::Cell::new(0);
        let result = trace_with_cancel(Image { width: 128, height: 128, rgba: &data }, &params(), || {
            let n = count.get();
            count.set(n + 1);
            n >= limit
        });
        assert!(matches!(result, Err(Error::Cancelled)), "{limit}: {result:?}");
        assert_eq!(count.get(), limit + 1);
    }
    assert!(matches!(trace(Image { width: 16000, height: 16000, rgba: &[] }, &params()), Err(Error::PixelBudget(_))));
    let mut c = Control { cancel: &|| false, remaining: 1 };
    assert!(matches!(c.work(2), Err(Error::BoundaryBudget)));
}
#[test]
fn centerline_deterministic_across_concurrent_workers() {
    let data = pixels(128, 128, |x, y| x.abs_diff(64) + y.abs_diff(64) == 30 || (x == 20 && (10..60).contains(&y)));
    let expected = serde_json::to_vec(&run(128, 128, &data)).unwrap();
    std::thread::scope(|scope| {
        for workers in [1, 2, 4] {
            let jobs: Vec<_> = (0..workers).map(|_| scope.spawn(|| serde_json::to_vec(&run(128, 128, &data)).unwrap())).collect();
            for job in jobs {
                assert_eq!(job.join().unwrap(), expected);
            }
        }
    });
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn centerline_random_small_inputs_are_finite_open_and_deterministic((w,h,values) in (1usize..14,1usize..14).prop_flat_map(|(w,h)|(Just(w),Just(h),prop::collection::vec(any::<bool>(),w*h)))) {
        let data:Vec<_> = values.iter().flat_map(|&b|if b{[0,0,0,255]}else{[255;4]}).collect();
        let out=run(w,h,&data);valid(&out);
        prop_assert_eq!(serde_json::to_vec(&out).unwrap(),serde_json::to_vec(&run(w,h,&data)).unwrap());
    }
}
