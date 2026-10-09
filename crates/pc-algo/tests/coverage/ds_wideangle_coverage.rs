use photocraft_algo::wideangle::*;

#[test]
fn wide_model_parse_all_valid_variants() {
    assert_eq!(WideModel::parse("auto"), Some(WideModel::Auto));
    assert_eq!(WideModel::parse("fisheye"), Some(WideModel::Fisheye));
    assert_eq!(WideModel::parse("perspective"), Some(WideModel::Perspective));
    assert_eq!(WideModel::parse("fullSpherical"), Some(WideModel::FullSpherical));
    assert_eq!(WideModel::parse("spherical"), Some(WideModel::FullSpherical));
}

#[test]
fn wide_model_parse_invalid_returns_none() {
    assert_eq!(WideModel::parse(""), None);
    assert_eq!(WideModel::parse("foo"), None);
    assert_eq!(WideModel::parse("AUTO"), None);
    assert_eq!(WideModel::parse(" fullSpherical"), None);
    assert_eq!(WideModel::parse("fullSpherical "), None);
}

#[test]
fn wide_model_default_is_auto() {
    assert_eq!(WideModel::default(), WideModel::Auto);
}

#[test]
fn orientation_default_is_free() {
    assert_eq!(Orientation::default(), Orientation::Free);
}

#[test]
fn wide_angle_default_fields() {
    let wa = WideAngle::default();
    assert_eq!(wa.model, WideModel::Auto);
    assert_eq!(wa.focal_length, 0.0);
    assert_eq!(wa.crop_factor, 1.0);
    assert_eq!(wa.scale, 100.0);
    assert!(wa.constraints.is_empty());
}

#[test]
fn constraint_orientation_is_set() {
    let c = Constraint { a: [0.0, 0.0], b: [1.0, 1.0], orientation: Orientation::Free };
    assert_eq!(c.orientation, Orientation::Free);
    let c = Constraint { a: [2.0, 3.0], b: [4.0, 5.0], orientation: Orientation::Horizontal };
    assert_eq!(c.orientation, Orientation::Horizontal);
    let c = Constraint { a: [2.0, 3.0], b: [4.0, 5.0], orientation: Orientation::Vertical };
    assert_eq!(c.orientation, Orientation::Vertical);
}

fn sample_camera(model: WideModel, f: f64, c: [f64; 2], size: [f64; 2]) -> Camera {
    Camera { model, f, c, size }
}

#[test]
fn camera_perspective_round_trip() {
    let cam = sample_camera(WideModel::Perspective, 100.0, [300.0, 200.0], [600.0, 400.0]);
    for (x, y) in [(10.0, 20.0), (300.0, 200.0), (590.0, 380.0), (150.5, 250.25)] {
        let ray = cam.ray(x, y);
        let px = cam.pixel(ray).unwrap();
        assert!((px[0] - x).abs() < 1e-6, "x: {} vs {}", px[0], x);
        assert!((px[1] - y).abs() < 1e-6, "y: {} vs {}", px[1], y);
    }
}

#[test]
fn camera_fisheye_round_trip() {
    let cam = sample_camera(WideModel::Fisheye, 106.6666667, [240.0, 160.0], [480.0, 320.0]);
    for (x, y) in [(10.0, 20.0), (240.0, 160.0), (470.0, 300.0), (60.0, 70.0)] {
        let ray = cam.ray(x, y);
        let px = cam.pixel(ray).unwrap();
        assert!((px[0] - x).abs() < 1e-6, "x: {} vs {}", px[0], x);
        assert!((px[1] - y).abs() < 1e-6, "y: {} vs {}", px[1], y);
    }
}

#[test]
fn camera_full_spherical_round_trip() {
    let cam = sample_camera(WideModel::FullSpherical, 1.0, [400.0, 200.0], [800.0, 400.0]);
    for (x, y) in [(0.0, 0.0), (400.0, 200.0), (799.0, 399.0), (123.0, 77.0)] {
        let ray = cam.ray(x, y);
        let px = cam.pixel(ray).unwrap();
        assert!((px[0] - x).abs() < 1e-6, "x: {} vs {}", px[0], x);
        assert!((px[1] - y).abs() < 1e-6, "y: {} vs {}", px[1], y);
    }
}

#[test]
fn camera_perspective_behind_returns_none() {
    let cam = sample_camera(WideModel::Perspective, 100.0, [0.0, 0.0], [100.0, 100.0]);
    let behind = [0.0, 0.0, -1.0];
    assert_eq!(cam.pixel(behind), None);
    let zero_z = [0.0, 0.0, 0.0];
    assert_eq!(cam.pixel(zero_z), None);
}

#[test]
fn camera_fisheye_zero_direction_returns_center() {
    let cam = sample_camera(WideModel::Fisheye, 100.0, [50.0, 60.0], [200.0, 200.0]);
    let center = cam.pixel([0.0, 0.0, 1.0]).unwrap();
    assert!((center[0] - 50.0).abs() < 1e-9 && (center[1] - 60.0).abs() < 1e-9);
    let small = cam.pixel([1e-13, 1e-13, 1.0]).unwrap();
    assert!((small[0] - 50.0).abs() < 1e-9 && (small[1] - 60.0).abs() < 1e-9);
}

#[test]
fn camera_ray_is_unit_vector() {
    let cameras = [
        sample_camera(WideModel::Perspective, 100.0, [300.0, 200.0], [600.0, 400.0]),
        sample_camera(WideModel::Fisheye, 106.6667, [240.0, 160.0], [480.0, 320.0]),
        sample_camera(WideModel::FullSpherical, 1.0, [400.0, 200.0], [800.0, 400.0]),
    ];
    for cam in &cameras {
        for (x, y) in [(0.0, 0.0), (cam.c[0], cam.c[1]), (cam.size[0] - 1.0, cam.size[1] - 1.0), (123.4, 234.5)] {
            let ray = cam.ray(x, y);
            let len = (ray[0] * ray[0] + ray[1] * ray[1] + ray[2] * ray[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-9, "model {:?}, len {}", cam.model, len);
        }
    }
}

#[test]
fn camera_arc_perspective_straight() {
    let cam = sample_camera(WideModel::Perspective, 100.0, [300.0, 200.0], [600.0, 400.0]);
    let a = [50.0, 50.0];
    let b = [550.0, 80.0];
    let arc = cam.arc(a, b, 10);
    assert_eq!(arc.len(), 11);
    assert!((arc[0][0] - a[0]).abs() < 1e-6 && (arc[0][1] - a[1]).abs() < 1e-6);
    assert!((arc[10][0] - b[0]).abs() < 1e-6 && (arc[10][1] - b[1]).abs() < 1e-6);
    let mid = arc[5];
    let expected_y = 50.0 + 0.06 * (mid[0] - 50.0);
    assert!((mid[1] - expected_y).abs() < 1.0, "mid {:?}, expected_y {}", mid, expected_y);
}

#[test]
fn camera_arc_fisheye_bows_away_from_center() {
    let cam = sample_camera(WideModel::Fisheye, 106.6667, [240.0, 160.0], [480.0, 320.0]);
    let a = [50.0, 60.0];
    let b = [550.0, 60.0];
    let arc = cam.arc(a, b, 10);
    let mid_y = arc[5][1];
    assert!(mid_y < 50.0, "expected bow upward, mid_y {}", mid_y);
    assert!(mid_y < arc[0][1] && mid_y < arc[10][1]);
}

#[test]
fn wide_mesh_triangles_count_and_indices() {
    let nx = 3;
    let ny = 2;
    let w1 = nx + 1;
    let nv = w1 * (ny + 1);
    let verts: Vec<([f64; 2], [f64; 2])> = (0..nv).map(|_| ([0.0, 0.0], [0.0, 0.0])).collect();
    let mesh = WideMesh { nx, ny, verts, curves: Vec::new(), residual: 0.0 };
    let tris = mesh.triangles();
    assert_eq!(tris.len(), nx * ny * 2);
    for tri in &tris {
        assert!(tri[0] < nv && tri[1] < nv && tri[2] < nv, "index out of bounds: {:?}", tri);
        assert_ne!(tri[0], tri[1]);
        assert_ne!(tri[1], tri[2]);
        assert_ne!(tri[2], tri[0]);
    }
}

#[test]
fn wide_mesh_triangles_empty() {
    let mesh = WideMesh { nx: 0, ny: 0, verts: Vec::new(), curves: Vec::new(), residual: 0.0 };
    let tris = mesh.triangles();
    assert!(tris.is_empty());
}

#[test]
fn camera_arc_identical_points_short() {
    let cam = sample_camera(WideModel::Perspective, 100.0, [0.0, 0.0], [100.0, 100.0]);
    let p = [50.0, 50.0];
    let arc = cam.arc(p, p, 5);
    assert_eq!(arc.len(), 6);
    for q in &arc {
        assert!((q[0] - 50.0).abs() < 1e-6 && (q[1] - 50.0).abs() < 1e-6);
    }
}
