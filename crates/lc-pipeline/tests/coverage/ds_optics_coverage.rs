use lightcraft_develop::{DevelopSettings, EmbeddedLens, EmbeddedVignette, EmbeddedWarp};
use lightcraft_geom::{Affine, Orientation, Point};
use lightcraft_pipeline::optics::{Warp, defringe, embedded_vignette_gain, embedded_warp, estimate_lateral_ca, reorient_lens};
use lightcraft_raster::Rgb32f;

// Helper: a synthetic image with strong radial edges and known lateral CA.
// Red is magnified by `ar`, blue by `ab`, both relative to green.
fn ca_grid(w: usize, h: usize, ar: f64, ab: f64) -> Rgb32f {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let cell = w as f64 / 10.0;
    Rgb32f::from_fn(w, h, |x, y| {
        let px = x as f64 + 0.5;
        let py = y as f64 + 0.5;
        let sample = |x: f64, y: f64| -> f32 {
            let fx = (x / cell).fract() - 0.5;
            let fy = (y / cell).fract() - 0.5;
            let d = fx.hypot(fy) * cell - cell * 0.28;
            (0.05 + 0.85 * (d / 1.2).clamp(-0.5, 0.5) + 0.425) as f32
        };
        let at = |a: f64| {
            let sx = cx + (px - cx) / (1.0 + a);
            let sy = cy + (py - cy) / (1.0 + a);
            sample(sx, sy)
        };
        [at(ar), at(0.0), at(ab)]
    })
}

#[test]
fn warp_identity_defaults() {
    let wp = Warp::identity(300.0, 200.0);
    assert!(wp.is_identity());
    assert!(!wp.moves_pixels());
    assert!(!wp.per_channel());
    assert!(!wp.has_gain());
    let p = Point::new(42.0, 17.0);
    assert_eq!(wp.to_corrected(p), p);
    assert_eq!(wp.from_corrected(p), p);
    assert_eq!(wp.corrected_to_source(p, 0), p);
    assert_eq!(wp.to_source(p, 1), p);
    assert!((wp.gain(p) - 1.0).abs() < 1e-6);
}

#[test]
fn warp_from_settings_zero_dims_no_panic() {
    let s = DevelopSettings::default();
    let wp = Warp::from_settings(0.0, 0.0, &s, None);
    assert!(wp.is_identity());
    // Even with zero size, hd and edges are safe due to max(1e-9) and half-pixel margins.
    let p = Point::new(0.0, 0.0);
    assert_eq!(wp.to_source(p, 1), p);
    assert!((wp.gain(p) - 1.0).abs() < 1e-6);
}

#[test]
fn warp_from_settings_odd_dimensions() {
    let s = DevelopSettings::default();
    let wp = Warp::from_settings(301.0, 201.0, &s, None);
    assert!(wp.is_identity());
    let c = Point::new(301.0 / 2.0, 201.0 / 2.0);
    assert!(wp.to_source(c, 1).dist(c) < 1e-9);
    let corner = Point::new(0.0, 0.0);
    assert_eq!(wp.to_source(corner, 1), corner);
}

#[test]
fn manual_distortion_direction_and_center() {
    let mut s = DevelopSettings::default();
    // Positive distortion corrects barrel: corners sample from further in.
    s.optics.distortion = 50.0;
    let wp = Warp::from_settings(300.0, 200.0, &s, None);
    let corner = Point::new(0.0, 0.0);
    let q = wp.to_source(corner, 1);
    assert!(q.x > 0.0 && q.y > 0.0);
    let c = Point::new(150.0, 100.0);
    assert!(wp.to_source(c, 1).dist(c) < 1e-9);
}

#[test]
fn manual_vignette_brightens_corners() {
    let mut s = DevelopSettings::default();
    s.optics.vignetting = 100.0;
    let wp = Warp::from_settings(300.0, 200.0, &s, None);
    let center = Point::new(150.0, 100.0);
    assert!((wp.gain(center) - 1.0).abs() < 1e-6);
    let corner = Point::new(0.0, 0.0);
    assert!(wp.gain(corner) > 2.0);
}

#[test]
fn ca_per_channel_and_scaling() {
    let mut s = DevelopSettings::default();
    s.optics.ca_red = 100.0; // 0.003 scale
    s.optics.ca_blue = 50.0; // 0.0015 scale
    let wp = Warp::from_settings(300.0, 200.0, &s, None);
    assert!(wp.per_channel());
    let p = Point::new(170.0, 100.0); // 20 px right of center
    let green = wp.to_source(p, 1);
    let red = wp.to_source(p, 0);
    // Red plane should be pushed slightly outward compared to green.
    assert!((red.x - green.x).abs() > 0.05);
    assert!((red.x - 150.0) > (green.x - 150.0));
}

#[test]
fn embedded_warp_identity_maps_center() {
    let warp = EmbeddedWarp { planes: [[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]; 3], center: Point::new(0.5, 0.5), radius: 0.6 };
    let w = 100.0;
    let h = 80.0;
    let p = Point::new(10.0, 20.0);
    let q = embedded_warp(&warp, p, 1, w, h);
    assert!(q.dist(p) < 1e-9);
}

#[test]
fn embedded_vignette_gain_zero_coeff_identity() {
    let v = EmbeddedVignette { k: [0.0; 5], center: Point::new(0.5, 0.5), radius: 0.6 };
    let w = 100.0;
    let h = 80.0;
    for x in [0.0, 50.0, 100.0] {
        for y in [0.0, 40.0, 80.0] {
            let g = embedded_vignette_gain(&v, Point::new(x, y), w, h);
            assert!((g - 1.0).abs() < 1e-9);
        }
    }
}

#[test]
fn embedded_vignette_gain_nonzero_increases_gain() {
    let v = EmbeddedVignette { k: [1.0, 0.0, 0.0, 0.0, 0.0], center: Point::new(0.5, 0.5), radius: 0.6 };
    let w = 100.0;
    let h = 80.0;
    let center = Point::new(50.0, 40.0);
    assert!((embedded_vignette_gain(&v, center, w, h) - 1.0).abs() < 1e-9);
    let off_center = Point::new(75.0, 40.0);
    assert!(embedded_vignette_gain(&v, off_center, w, h) > 1.1);
}

#[test]
fn reorient_lens_normal_returns_same() {
    let lens = EmbeddedLens {
        warp: Some(EmbeddedWarp { planes: [[1.0, 0.01, 0.0, 0.0, 0.001, 0.002]; 3], center: Point::new(0.4, 0.5), radius: 0.6 }),
        vignette: Some(EmbeddedVignette { k: [0.2, 0.0, 0.0, 0.0, 0.0], center: Point::new(0.4, 0.5), radius: 0.6 }),
    };
    let r = reorient_lens(&lens, Orientation::Normal, 300.0, 200.0);
    let w = r.warp.unwrap();
    let v = r.vignette.unwrap();
    assert!((w.center.x - lens.warp.unwrap().center.x).abs() < 1e-9);
    assert!((w.center.y - lens.warp.unwrap().center.y).abs() < 1e-9);
    assert!((w.radius - lens.warp.unwrap().radius).abs() < 1e-9);
    assert_eq!(w.planes, lens.warp.unwrap().planes);
    assert!((v.center.x - lens.vignette.unwrap().center.x).abs() < 1e-9);
    assert!((v.center.y - lens.vignette.unwrap().center.y).abs() < 1e-9);
    assert!((v.radius - lens.vignette.unwrap().radius).abs() < 1e-9);
    assert_eq!(v.k, lens.vignette.unwrap().k);
}

#[test]
fn reorient_lens_rotate90_keeps_mapping() {
    let lens = EmbeddedLens {
        warp: Some(EmbeddedWarp { planes: [[1.0, 0.01, 0.0, 0.0, 0.001, 0.002]; 3], center: Point::new(0.4, 0.5), radius: 0.6 }),
        vignette: Some(EmbeddedVignette { k: [0.2, 0.0, 0.0, 0.0, 0.0], center: Point::new(0.4, 0.5), radius: 0.6 }),
    };
    let r = reorient_lens(&lens, Orientation::Rotate90, 300.0, 200.0);
    let w = r.warp.unwrap();
    // Rotate 90° cw: (x, y) -> (h - y, x)
    let c = w.center;
    assert!((c.x - 0.5).abs() < 1e-9 && (c.y - 0.4).abs() < 1e-9, "{c:?}");
    let p = Point::new(50.0, 30.0);
    let s0 = embedded_warp(&lens.warp.unwrap(), p, 0, 300.0, 200.0);
    let m = |q: Point| {
        let (x, y) = Orientation::Rotate90.map(q.x, q.y, 300.0, 200.0);
        Point::new(x, y)
    };
    let s1 = embedded_warp(&w, m(p), 0, 200.0, 300.0);
    assert!(s1.dist(m(s0)) < 1e-6, "{s1:?} vs {:?}", m(s0));
}

#[test]
fn estimate_lateral_ca_tiny_image_zero() {
    // 10x10 is below the 32px threshold; estimate should be exactly [0, 0].
    let img = Rgb32f::from_fn(10, 10, |x, y| [x as f32 / 10.0, y as f32 / 10.0, 0.5]);
    let [r, b] = estimate_lateral_ca(&img);
    assert_eq!(r, 0.0);
    assert_eq!(b, 0.0);
}

#[test]
fn estimate_lateral_ca_clean_image_zero() {
    // Uniform image has no edges; estimator should return zero.
    let img = Rgb32f::filled(600, 400, [0.5, 0.5, 0.5]);
    let [r, b] = estimate_lateral_ca(&img);
    assert!(r.abs() < 1e-6 && b.abs() < 1e-6);
}

#[test]
fn estimate_lateral_ca_recovers_synthetic() {
    let img = ca_grid(600, 400, 0.004, -0.003);
    let [r, b] = estimate_lateral_ca(&img);
    assert!((r - 0.004).abs() < 0.001, "red {r}");
    assert!((b + 0.003).abs() < 0.001, "blue {b}");
}

#[test]
fn estimate_lateral_ca_deterministic() {
    let img = ca_grid(500, 350, 0.002, 0.001);
    let a = estimate_lateral_ca(&img);
    let b = estimate_lateral_ca(&img);
    assert_eq!(a, b);
}

#[test]
fn defringe_noop_when_zero() {
    let mut s = DevelopSettings::default();
    s.optics.defringe_purple_amount = 0.0;
    s.optics.defringe_green_amount = 0.0;
    let mut img = Rgb32f::from_fn(40, 20, |x, _| if x < 20 { [0.9; 3] } else { [0.02; 3] });
    let before = img.data.clone();
    defringe(&mut img, &s, 1.0);
    assert_eq!(img.data, before);
}

#[test]
fn defringe_removes_purple_at_edges_only() {
    let mut s = DevelopSettings::default();
    s.optics.defringe_purple_amount = 10.0;
    let purple = [0.25f32, 0.05, 0.45];
    let mut img = Rgb32f::from_fn(40, 20, |x, _| if x < 20 { [0.9; 3] } else { [0.02; 3] });
    for y in 0..20 {
        img.set(20, y, purple);
    }
    let mut flat = Rgb32f::filled(40, 20, purple);
    defringe(&mut img, &s, 1.0);
    defringe(&mut flat, &s, 1.0);
    let chroma = |p: [f32; 3]| {
        let l = lightcraft_color::perceptual::oklab_from_2020(p);
        l[1].hypot(l[2])
    };
    assert!(chroma(img.get(20, 10)) < chroma(purple) * 0.3);
    assert!((chroma(flat.get(20, 10)) - chroma(purple)).abs() < 1e-4);
}

#[test]
fn defringe_empty_image_no_panic() {
    let mut img = Rgb32f::new(0, 0);
    let mut s = DevelopSettings::default();
    s.optics.defringe_purple_amount = 100.0;
    defringe(&mut img, &s, 1.0);
    // Just reaching this point means no panic.
}

#[test]
fn warp_block_coverage_empty_block_returns_none() {
    let wp = Warp::identity(300.0, 200.0);
    let affine = Affine::IDENTITY;
    assert_eq!(wp.block_coverage(&affine, 5, 5, 0, 1), None);
    assert_eq!(wp.block_coverage(&affine, 0, 1, 5, 5), None);
    assert_eq!(wp.block_coverage(&affine, 10, 2, 0, 1), None);
}

#[test]
fn warp_frame_outside_image_returns_none() {
    let wp = Warp::identity(100.0, 80.0);
    let affine = Affine::IDENTITY;
    let inside = wp.frame(&affine, 50, 40);
    assert!(inside.is_some());
    let outside = wp.frame(&affine, 150, 120);
    assert!(outside.is_none());
    assert!(!wp.covers(&affine, 150, 120));
}

#[test]
fn warp_roundtrip_to_corrected_from_corrected_identity() {
    let wp = Warp::identity(250.0, 180.0);
    let p = Point::new(33.0, 77.0);
    let c = wp.to_corrected(p);
    assert_eq!(c, p);
    let back = wp.from_corrected(c);
    assert_eq!(back, p);
}
