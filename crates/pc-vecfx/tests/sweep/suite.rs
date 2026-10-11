#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
fn s(x: f64, y: f64, pressure: f64) -> Sample {
    Sample { point: Point::new(x, y), pressure }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= b.abs() * 0.005 + 1e-6, "{a} != {b}");
}
#[test]
fn sweep40_capsule_area_and_ellipse_minkowski_oracles() {
    for angle in [0_f64, 27., 90., 155.] {
        let a = angle.to_radians();
        let samples = [s(10., 20., 1.), s(10. + 100. * a.cos(), 20. + 100. * a.sin(), 1.)];
        let p = outline(&samples, Tip { size: 12., ..Default::default() }).unwrap();
        close(ops::area(&p).unwrap(), 100. * 12. + std::f64::consts::PI * 36.);
    }
    // Sweeping horizontally adds a rectangle whose height is the minor-axis diameter.
    let p = outline(&[s(0., 0., 1.), s(100., 0., 1.)], Tip { size: 20., roundness: 0.25, ..Default::default() }).unwrap();
    close(ops::area(&p).unwrap(), 500. + std::f64::consts::PI * 25.);
    let p = outline(&[s(0., 0., 1.), s(100., 0., 1.)], Tip { size: 20., roundness: 0.25, angle: 90., ..Default::default() }).unwrap();
    close(ops::area(&p).unwrap(), 2000. + std::f64::consts::PI * 25.);
}
#[test]
fn sweep40_pressure_dabs_stationary_changes_zero_and_reversed_rectangle() {
    let tip = Tip { size: 20., pressure: 1., ..Default::default() };
    close(ops::area(&outline(&[s(0., 0., 0.5)], tip).unwrap()).unwrap(), std::f64::consts::PI * 25.);
    let p = outline(&[s(0., 0., 0.), s(0., 0., 1.), s(0., 0., 0.2)], tip).unwrap();
    close(ops::area(&p).unwrap(), std::f64::consts::PI * 100.);
    assert!(outline(&[s(0., 0., 0.), s(10., 0., 0.)], tip).unwrap().is_empty());
    let p = outline(&[s(0., 0., 0.), s(100., 0., 1.)], tip).unwrap();
    assert!(ops::contains(&p, Point::new(50., 4.)).unwrap());
    assert!(!ops::contains(&p, Point::new(50., 6.)).unwrap());
    assert_eq!(ops::area(&rectangle(Point::new(70., 80.), Point::new(20., 30.)).unwrap()).unwrap(), 2500.);
}
#[test]
fn sweep40_256_random_continuous_sweeps_finite_containment_and_loops() {
    let mut seed = 0x40_u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as f64 / u32::MAX as f64
    };
    for _ in 0..256 {
        let samples = [s(random() * 200., random() * 200., random()), s(random() * 200., random() * 200., random())];
        let tip = Tip { size: 1. + random() * 30., angle: random() * 360., roundness: 0.1 + random() * 0.9, pressure: 1. };
        let p = outline(&samples, tip).unwrap();
        validate_path(&p).unwrap();
        ops::validate(&p).unwrap();
        assert!(p.subpaths.iter().all(|s| s.closed));
        for u in [0., 0.25, 0.5, 0.75, 1.] {
            assert!(
                ops::contains(&p, Point::new(samples[0].point.x * (1. - u) + samples[1].point.x * u, samples[0].point.y * (1. - u) + samples[1].point.y * u))
                    .unwrap()
            );
        }
    }
    let p = outline(&[s(0., 0., 1.), s(20., 20., 1.), s(0., 20., 1.), s(20., 0., 1.)], Tip::default()).unwrap();
    validate_path(&p).unwrap();
    assert!(ops::area(&p).unwrap() > 0.);
}
#[test]
fn sweep40_invalid_inputs_and_legacy_defaults() {
    assert_eq!(serde_json::from_str::<Tip>("{}").unwrap(), Tip::default());
    for v in [f64::NAN, f64::INFINITY, -1., 1001.] {
        assert!(outline(&[s(0., 0., 1.)], Tip { size: v, ..Default::default() }).is_err());
    }
    assert!(outline(&[], Tip::default()).is_err());
    assert!(outline(&[s(f64::NAN, 0., 1.)], Tip::default()).is_err());
    assert!(outline(&[s(0., 0., 1.1)], Tip::default()).is_err());
    assert!(rectangle(Point::new(f64::INFINITY, 0.), Point::default()).is_err());
}
