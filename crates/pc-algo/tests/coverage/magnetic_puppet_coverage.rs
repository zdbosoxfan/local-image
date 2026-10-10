use photocraft_algo::magnetic::{self, Settings, Tracer, WIDTH_RANGE};
use photocraft_algo::puppet::{PuppetDensity, PuppetMesh, PuppetMode, PuppetPin, PuppetWarp};
use photocraft_geom::Rect;

fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Rect {
    Rect::new(x0, y0, x1, y1)
}

fn const_fetch(color: [u8; 4]) -> impl FnMut(Rect) -> Vec<[u8; 4]> {
    move |r| vec![color; r.width() as usize * r.height() as usize]
}

fn assert_close(a: f64, b: f64, tol: f64, msg: &str) {
    assert!((a - b).abs() <= tol, "{msg}: {a} vs {b}");
}

#[test]
fn settings_clamp_to_range() {
    let s = Settings::new(500.0, 2.0);
    assert_eq!(s.width, WIDTH_RANGE.1);
    assert_eq!(s.contrast, 1.0);

    let s = Settings::new(-5.0, -0.5);
    assert_eq!(s.width, WIDTH_RANGE.0);
    assert_eq!(s.contrast, 0.01);

    let s = Settings::new(128.0, 0.5);
    assert_eq!(s.width, 128.0);
    assert_eq!(s.contrast, 0.5);
}

#[test]
fn settings_nan_uses_defaults() {
    let s = Settings::new(f64::NAN, f32::NAN);
    assert_eq!(s.width, Settings::default().width);
    assert_eq!(s.contrast, Settings::default().contrast);
}

#[test]
fn tracer_reports_bounds() {
    let bounds = rect(2, 3, 20, 18);
    let tracer = Tracer::new(bounds);
    assert_eq!(tracer.bounds(), bounds);
}

#[test]
fn snap_constant_image_returns_point() {
    let bounds = rect(0, 0, 10, 10);
    let mut tracer = Tracer::new(bounds);
    let mut fetch = const_fetch([128, 128, 128, 255]);
    let p = [4.5, 4.5];
    let snapped = tracer.snap(&mut fetch, p, Settings::default()).expect("point inside");
    assert_close(snapped[0], p[0], 1e-6, "x");
    assert_close(snapped[1], p[1], 1e-6, "y");
}

#[test]
fn snap_vertical_edge_returns_subpixel_center() {
    let bounds = rect(0, 0, 20, 20);
    let edge_x = 10;
    let mut tracer = Tracer::new(bounds);
    let mut fetch = move |r: Rect| {
        let mut v = Vec::with_capacity(r.width() as usize * r.height() as usize);
        for _y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                if x < edge_x {
                    v.push([0, 0, 0, 255]);
                } else {
                    v.push([255, 255, 255, 255]);
                }
            }
        }
        v
    };
    let snapped = tracer.snap(&mut fetch, [10.0, 10.0], Settings::default()).expect("point inside");
    assert_close(snapped[0], 10.0, 0.2, "snapped x should be on edge");
}

#[test]
fn snap_clamps_point_to_bounds() {
    let bounds = rect(0, 0, 10, 10);
    let mut tracer = Tracer::new(bounds);
    let mut fetch = const_fetch([0, 0, 0, 255]);
    let snapped = tracer.snap(&mut fetch, [15.0, -5.0], Settings::default()).expect("clamped inside");
    assert_close(snapped[0], 10.0, 1e-6, "x clamped to max");
    assert_close(snapped[1], 0.0, 1e-6, "y clamped to min");
}

#[test]
fn snap_empty_bounds_returns_none() {
    let bounds = Rect::EMPTY;
    let mut tracer = Tracer::new(bounds);
    let mut fetch = const_fetch([0, 0, 0, 255]);
    assert!(tracer.snap(&mut fetch, [1.0, 1.0], Settings::default()).is_none());
}

#[test]
fn snap_non_finite_point_returns_none() {
    let bounds = rect(0, 0, 10, 10);
    let mut tracer = Tracer::new(bounds);
    let mut fetch = const_fetch([0, 0, 0, 255]);
    assert!(tracer.snap(&mut fetch, [f64::NAN, 1.0], Settings::default()).is_none());
    assert!(tracer.snap(&mut fetch, [1.0, f64::INFINITY], Settings::default()).is_none());
}

#[test]
fn trace_constant_image_keeps_endpoints_and_inside_bounds() {
    let bounds = rect(0, 0, 10, 10);
    let mut tracer = Tracer::new(bounds);
    let mut fetch = const_fetch([128, 128, 128, 255]);
    let from = [2.0, 2.0];
    let to = [8.0, 8.0];
    let path = tracer.trace(&mut fetch, from, to, &[], Settings::default());
    assert!(!path.is_empty());
    assert_close(path[0][0], from[0], 1e-6, "start x");
    assert_close(path[0][1], from[1], 1e-6, "start y");
    let last = path.last().unwrap();
    assert_close(last[0], to[0], 1e-6, "end x");
    assert_close(last[1], to[1], 1e-6, "end y");
    for p in &path {
        assert!(bounds.contains(p[0].floor() as i32, p[1].floor() as i32), "point inside bounds");
    }
}

#[test]
fn trace_empty_bounds_returns_empty() {
    let bounds = Rect::EMPTY;
    let mut tracer = Tracer::new(bounds);
    let mut fetch = const_fetch([0, 0, 0, 255]);
    let path = tracer.trace(&mut fetch, [0.0, 0.0], [1.0, 1.0], &[], Settings::default());
    assert!(path.is_empty());
}

#[test]
fn trace_non_finite_point_returns_empty() {
    let bounds = rect(0, 0, 10, 10);
    let mut tracer = Tracer::new(bounds);
    let mut fetch = const_fetch([0, 0, 0, 255]);
    assert!(tracer.trace(&mut fetch, [f64::NAN, 0.0], [1.0, 1.0], &[], Settings::default()).is_empty());
    assert!(tracer.trace(&mut fetch, [0.0, 0.0], [1.0, f64::NEG_INFINITY], &[], Settings::default()).is_empty());
}

#[test]
fn trace_deterministic() {
    let bounds = rect(0, 0, 20, 20);
    let mut tracer1 = Tracer::new(bounds);
    let mut tracer2 = Tracer::new(bounds);
    let mut fetch1 = |r: Rect| {
        let mut v = Vec::with_capacity(r.width() as usize * r.height() as usize);
        for _y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                let px = if (x + _y) % 2 == 0 { 0 } else { 255 };
                v.push([px, px, px, 255]);
            }
        }
        v
    };
    let mut fetch2 = fetch1;
    let from = [2.0, 3.0];
    let to = [17.0, 16.0];
    let path1 = tracer1.trace(&mut fetch1, from, to, &[], Settings::default());
    let path2 = tracer2.trace(&mut fetch2, from, to, &[], Settings::default());
    assert_eq!(path1, path2);
}

#[test]
fn snap_symmetric_under_mirror() {
    let bounds = rect(0, 0, 20, 20);
    let edge_x = 10;
    let mut tracer = Tracer::new(bounds);
    let mut fetch = move |r: Rect| {
        let mut v = Vec::with_capacity(r.width() as usize * r.height() as usize);
        for _y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                if x < edge_x {
                    v.push([0, 0, 0, 255]);
                } else {
                    v.push([255, 255, 255, 255]);
                }
            }
        }
        v
    };
    let left = tracer.snap(&mut fetch, [8.0, 10.0], Settings::default()).expect("point");
    let right = tracer.snap(&mut fetch, [12.0, 10.0], Settings::default()).expect("point");
    assert_close(left[0] - 10.0, 10.0 - right[0], 0.2, "symmetry around edge");
    assert_close(left[1], right[1], 1e-6, "y unchanged");
}

#[test]
fn simplify_removes_collinear_points() {
    let points = vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]];
    let simplified = magnetic::simplify(&points, 0.1);
    assert_eq!(simplified.len(), 2);
    assert_eq!(simplified[0], [0.0, 0.0]);
    assert_eq!(simplified[1], [3.0, 3.0]);
}

#[test]
fn simplify_keeps_few_points() {
    let points = vec![[0.0, 0.0], [1.0, 1.0]];
    let simplified = magnetic::simplify(&points, 0.5);
    assert_eq!(simplified.len(), 2);
}

#[test]
fn length_calculates_sum() {
    let points = vec![[0.0, 0.0], [3.0, 4.0], [6.0, 4.0]];
    let len = magnetic::length(&points);
    assert_close(len, 5.0 + 3.0, 1e-6, "length sum");
}

#[test]
fn fetch_wrong_length_treated_as_transparent() {
    let bounds = rect(0, 0, 10, 10);
    let mut tracer = Tracer::new(bounds);
    let mut fetch = |_r: Rect| Vec::<[u8; 4]>::new(); // always empty, wrong length
    let path = tracer.trace(&mut fetch, [1.0, 1.0], [8.0, 8.0], &[], Settings::default());
    assert!(!path.is_empty());
    // Path should still be within bounds and connect endpoints (after simplification).
    assert!(path.len() >= 2);
    assert_close(path[0][0], 1.0, 1e-6, "start x");
    assert_close(path.last().unwrap()[0], 8.0, 1e-6, "end x");
}

#[test]
fn alpha_transparent_pixels_no_edge() {
    let bounds = rect(0, 0, 20, 20);
    let edge_x = 10;
    let mut tracer = Tracer::new(bounds);
    let mut fetch = move |r: Rect| {
        let mut v = Vec::with_capacity(r.width() as usize * r.height() as usize);
        for _y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                if x < edge_x {
                    v.push([255, 0, 0, 0]); // transparent red
                } else {
                    v.push([0, 0, 255, 0]); // transparent blue
                }
            }
        }
        v
    };
    let p = [10.0, 10.0];
    let snapped = tracer.snap(&mut fetch, p, Settings::default()).expect("point");
    assert_close(snapped[0], p[0], 1e-6, "no edge between transparent colors");
}

#[test]
fn alpha_opaque_transparent_edge() {
    let bounds = rect(0, 0, 20, 20);
    let edge_x = 10;
    let mut tracer = Tracer::new(bounds);
    let mut fetch = move |r: Rect| {
        let mut v = Vec::with_capacity(r.width() as usize * r.height() as usize);
        for _y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                if x < edge_x {
                    v.push([255, 255, 255, 255]);
                } else {
                    v.push([0, 0, 0, 0]);
                }
            }
        }
        v
    };
    let snapped = tracer.snap(&mut fetch, [10.0, 10.0], Settings::default()).expect("point");
    assert!((snapped[0] - 10.0).abs() < 0.5, "edge between opaque and transparent snapped");
}

#[test]
fn cached_pixels_increases_after_read() {
    let bounds = rect(0, 0, 100, 100);
    let mut tracer = Tracer::new(bounds);
    let mut fetch = const_fetch([128, 128, 128, 255]);
    let before = tracer.cached_pixels();
    let _ = tracer.snap(&mut fetch, [50.0, 50.0], Settings::default());
    let after = tracer.cached_pixels();
    assert!(after > before, "cached_pixels should increase");
}

#[test]
fn max_guide_length_returns_guide() {
    let bounds = rect(0, 0, 300_000, 10);
    let mut tracer = Tracer::new(bounds);
    let mut fetch = const_fetch([0, 0, 0, 255]);
    let from = [0.0, 5.0];
    let to = [299_999.0, 5.0];
    let guide = vec![from, to];
    let path = tracer.trace(&mut fetch, from, to, &guide, Settings::default());
    assert_eq!(path.len(), 2, "guide longer than MAX_GUIDE_LENGTH returned as is");
    assert_close(path[0][0], from[0], 1e-6, "start x");
    assert_close(path[1][0], to[0], 1e-6, "end x");
}

#[test]
fn puppet_mode_parse_works() {
    assert_eq!(PuppetMode::parse("rigid"), Some(PuppetMode::Rigid));
    assert_eq!(PuppetMode::parse("Normal"), Some(PuppetMode::Normal));
    assert_eq!(PuppetMode::parse("distort"), Some(PuppetMode::Distort));
    assert_eq!(PuppetMode::parse("bogus"), None);
}

#[test]
fn puppet_density_parse_works() {
    assert_eq!(PuppetDensity::parse("fewer"), Some(PuppetDensity::Fewer));
    assert_eq!(PuppetDensity::parse("morepoints"), Some(PuppetDensity::More));
    assert_eq!(PuppetDensity::parse("Normal"), Some(PuppetDensity::Normal));
    assert_eq!(PuppetDensity::parse("bogus"), None);
}

#[test]
fn puppet_warp_is_identity() {
    let empty = PuppetWarp { pins: vec![], mode: PuppetMode::Normal, density: PuppetDensity::Normal, expansion: 2.0 };
    assert!(empty.is_identity());

    let unmoved = PuppetWarp {
        pins: vec![
            PuppetPin { src: [10.0, 10.0], dst: [10.0, 10.0], rotate: None, depth: 0 },
            PuppetPin { src: [20.0, 20.0], dst: [20.0, 20.0], rotate: Some(0.0), depth: 1 },
        ],
        mode: PuppetMode::Rigid,
        density: PuppetDensity::Fewer,
        expansion: -1.0,
    };
    assert!(unmoved.is_identity());
}

#[test]
fn puppet_warp_not_identity() {
    let moved = PuppetWarp {
        pins: vec![PuppetPin { src: [10.0, 10.0], dst: [12.0, 10.0], rotate: None, depth: 0 }],
        mode: PuppetMode::Normal,
        density: PuppetDensity::Normal,
        expansion: 2.0,
    };
    assert!(!moved.is_identity());

    let rotated = PuppetWarp {
        pins: vec![PuppetPin { src: [10.0, 10.0], dst: [10.0, 10.0], rotate: Some(90.0), depth: 0 }],
        mode: PuppetMode::Normal,
        density: PuppetDensity::Normal,
        expansion: 2.0,
    };
    assert!(!rotated.is_identity());
}

#[test]
fn puppet_mesh_default_empty() {
    let mesh = PuppetMesh::default();
    assert!(mesh.verts.is_empty());
    assert!(mesh.tris.is_empty());
    assert_eq!(mesh.spacing, 0.0);
}
