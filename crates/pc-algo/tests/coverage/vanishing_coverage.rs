use photocraft_algo::vanishing::{Edge, Scene, VpPlane, clone_stroke, paste};
use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

type V3 = [f64; 3];

fn project(f: f64, c: [f64; 2], p: V3) -> [f64; 2] {
    [c[0] + f * p[0] / p[2], c[1] + f * p[1] / p[2]]
}

fn floor_quad(f: f64, c: [f64; 2]) -> VpPlane {
    let (rx, ry) = (0.6f64, 0.4f64);
    let rot = |p: V3| {
        let (s, co) = rx.sin_cos();
        let q = [p[0], co * p[1] - s * p[2], s * p[1] + co * p[2]];
        let (s2, c2) = ry.sin_cos();
        [c2 * q[0] + s2 * q[2], q[1], -s2 * q[0] + c2 * q[2]]
    };
    let corners = [[-1.0, -0.5, 0.0], [1.0, -0.5, 0.0], [1.0, 0.5, 0.0], [-1.0, 0.5, 0.0]].map(|p: V3| project(f, c, add(rot(p), [0.0, 0.0, 4.0])));
    VpPlane { corners }
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn rect_plane() -> VpPlane {
    VpPlane { corners: [[10.0, 10.0], [110.0, 10.0], [110.0, 210.0], [10.0, 210.0]] }
}

#[test]
fn vp_plane_homography_valid_rectangle() {
    let plane = rect_plane();
    let h = plane.homography().expect("homography should exist");
    // Map unit square corners to quad corners
    let p00 = h.apply(0.0, 0.0);
    let p10 = h.apply(1.0, 0.0);
    let p11 = h.apply(1.0, 1.0);
    let p01 = h.apply(0.0, 1.0);
    assert!((p00.0 - 10.0).abs() < 1e-9 && (p00.1 - 10.0).abs() < 1e-9);
    assert!((p10.0 - 110.0).abs() < 1e-9 && (p10.1 - 10.0).abs() < 1e-9);
    assert!((p11.0 - 110.0).abs() < 1e-9 && (p11.1 - 210.0).abs() < 1e-9);
    assert!((p01.0 - 10.0).abs() < 1e-9 && (p01.1 - 210.0).abs() < 1e-9);
}

#[test]
fn vp_plane_homography_degenerate_still_returns_some() {
    let plane = VpPlane { corners: [[5.0; 2]; 4] };
    let h = plane.homography().expect("homography should still be constructed");
    // Degenerate homography cannot be inverted.
    assert!(h.inverse().is_none());
}

#[test]
fn vp_plane_focal_parallel_edges_returns_none() {
    let plane = rect_plane();
    assert_eq!(plane.focal([50.0, 100.0]), None);
}

#[test]
fn vp_plane_focal_nan_corners_returns_none() {
    let plane = VpPlane { corners: [[0.0, 0.0], [100.0, 0.0], [100.0, f64::NAN], [0.0, 100.0]] };
    assert_eq!(plane.focal([50.0, 50.0]), None);
}

#[test]
fn vp_plane_lift_survives_degenerate_without_panic() {
    let plane = VpPlane { corners: [[0.0; 2]; 4] };
    let lifted = plane.lift(1.0, [0.0, 0.0]);
    // Ensure no panic and all points are finite (even if huge)
    for p in lifted.pts.iter() {
        for v in p.iter() {
            assert!(v.is_finite());
        }
    }
}

#[test]
fn lifted_project_behind_returns_none() {
    let c = [400.0, 300.0];
    let plane = floor_quad(500.0, c);
    let lifted = plane.lift(500.0, c);
    let behind = [0.0, 0.0, -1.0];
    let at_zero = [0.0, 0.0, 0.0];
    assert!(lifted.project(behind).is_none());
    assert!(lifted.project(at_zero).is_none());
    let front = [0.0, 0.0, 1.0];
    assert!(lifted.project(front).is_some());
}

#[test]
fn scene_new_uses_provided_focal() {
    let c = [400.0, 300.0];
    let plane = rect_plane();
    let scene = Scene::new(plane, c, Some(123.0), 800.0);
    assert_eq!(scene.focal, 123.0);
    assert_eq!(scene.center, c);
    assert_eq!(scene.planes.len(), 1);
}

#[test]
fn scene_new_fallback_when_no_focal_and_plane_not_perspective() {
    let c = [400.0, 300.0];
    let plane = rect_plane();
    let scene = Scene::new(plane, c, None, 800.0);
    assert_eq!(scene.focal, 800.0);
}

#[test]
fn scene_tear_off_all_edges_does_not_panic() {
    let c = [400.0, 300.0];
    let mut scene = Scene::new(floor_quad(500.0, c), c, None, 800.0);
    for edge in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
        let _ = scene.tear_off(0, edge, 45.0, 0.5);
    }
    assert_eq!(scene.planes.len(), 5);
}

#[test]
fn scene_frame_valid_returns_some() {
    let c = [400.0, 300.0];
    let scene = Scene::new(floor_quad(500.0, c), c, None, 800.0);
    let (h, lu, lv) = scene.frame(0).expect("frame");
    assert!(lu > 0.0 && lv > 0.0);
    let (x, y) = h.apply(0.0, 0.0);
    assert!(x.is_finite() && y.is_finite());
}

#[test]
fn scene_frame_degenerate_returns_some() {
    let scene = Scene { planes: vec![VpPlane { corners: [[0.0; 2]; 4] }], focal: 500.0, center: [0.0, 0.0] };
    let frame = scene.frame(0).expect("frame should still be constructed");
    assert!(frame.0.inverse().is_none());
}

#[test]
fn scene_plane_at_inside_returns_some() {
    let c = [400.0, 300.0];
    let plane = floor_quad(500.0, c);
    let scene = Scene::new(plane.clone(), c, None, 800.0);
    let h = plane.homography().expect("homography");
    let (x, y) = h.apply(0.5, 0.5);
    assert_eq!(scene.plane_at([x, y]), Some(0));
}

#[test]
fn scene_plane_at_outside_returns_none() {
    let c = [400.0, 300.0];
    let plane = floor_quad(500.0, c);
    let scene = Scene::new(plane, c, None, 800.0);
    assert_eq!(scene.plane_at([9999.0, 9999.0]), None);
}

#[test]
fn paste_empty_src_rect_returns_none() {
    let c = [400.0, 300.0];
    let scene = Scene::new(floor_quad(500.0, c), c, None, 800.0);
    let img = Surface::new(PixelFormat::RGBA8);
    assert!(paste(&scene, 0, &img, Rect::new(0, 0, 0, 10), [0.0, 0.0], 0.5).is_none());
    assert!(paste(&scene, 0, &img, Rect::new(0, 0, 10, 0), [0.0, 0.0], 0.5).is_none());
}

#[test]
fn paste_zero_width_does_not_panic() {
    let c = [400.0, 300.0];
    let scene = Scene::new(floor_quad(500.0, c), c, None, 800.0);
    let mut img = Surface::new(PixelFormat::RGBA8);
    img.fill_rect(Rect::new(0, 0, 10, 10), &[0.5, 0.5, 0.5, 1.0]);
    let out = paste(&scene, 0, &img, Rect::new(0, 0, 10, 10), [0.5, 0.5], 0.0);
    assert!(out.is_some());
    // Output should be finite (possibly empty)
    let out = out.unwrap();
    assert!(out.content_bounds().is_empty() || out.read_region(out.content_bounds()).iter().all(|v| v.is_finite()));
}

#[test]
fn clone_stroke_empty_points_returns_zero() {
    let c = [400.0, 300.0];
    let scene = Scene::new(floor_quad(500.0, c), c, None, 800.0);
    let mut surf = Surface::new(PixelFormat::RGBA8);
    let count = clone_stroke(&scene, &mut surf, [0.0, 0.0], &[], 10.0, 0.5, 1.0);
    assert_eq!(count, 0);
}

#[test]
fn clone_stroke_source_not_on_plane_returns_zero() {
    let c = [400.0, 300.0];
    let scene = Scene::new(floor_quad(500.0, c), c, None, 800.0);
    let mut surf = Surface::new(PixelFormat::RGBA8);
    let count = clone_stroke(&scene, &mut surf, [9999.0, 9999.0], &[[400.0, 300.0]], 10.0, 0.5, 1.0);
    assert_eq!(count, 0);
}

#[test]
fn clone_stroke_destination_not_on_plane_returns_zero() {
    let c = [400.0, 300.0];
    let scene = Scene::new(floor_quad(500.0, c), c, None, 800.0);
    let mut surf = Surface::new(PixelFormat::RGBA8);
    let h = scene.planes[0].homography().unwrap();
    let (sx, sy) = h.apply(0.5, 0.5);
    let count = clone_stroke(&scene, &mut surf, [sx, sy], &[[9999.0, 9999.0]], 10.0, 0.5, 1.0);
    assert_eq!(count, 0);
}
