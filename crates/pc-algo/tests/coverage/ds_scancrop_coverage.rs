use photocraft_algo::scancrop::{FoundPhoto, find_photos};

// Helper: generate a scan with given photo specs. Spec: (center_x, center_y, width, height, angle_deg)
fn scan_with_photos(w: usize, h: usize, specs: &[(f64, f64, f64, f64, f64)]) -> Vec<[f32; 4]> {
    let mut pixels = vec![[0.97f32, 0.97, 0.96, 1.0]; w * h];
    for &(cx, cy, width, height, angle_deg) in specs {
        let angle = angle_deg.to_radians();
        let (s, c) = angle.sin_cos();
        for y in 0..h {
            for x in 0..w {
                let dx = x as f64 + 0.5 - cx;
                let dy = y as f64 + 0.5 - cy;
                let u = c * dx + s * dy;
                let v = -s * dx + c * dy;
                if u.abs() < width / 2.0 && v.abs() < height / 2.0 {
                    // dark rectangle, high contrast with background
                    pixels[y * w + x] = [0.15, 0.25, 0.35, 1.0];
                }
            }
        }
    }
    pixels
}

fn assert_close(a: f64, b: f64, tol: f64, label: &str) {
    assert!((a - b).abs() < tol, "{}: {} vs {} (tol {})", label, a, b, tol);
}

#[test]
fn empty_on_tiny_images() {
    assert!(find_photos(0, 0, &[]).is_empty());
    let w = 7;
    let h = 7;
    let pixels = vec![[0.5; 4]; w * h];
    assert!(find_photos(w, h, &pixels).is_empty());
    let w = 8;
    let h = 7;
    let pixels = vec![[0.5; 4]; w * h];
    assert!(find_photos(w, h, &pixels).is_empty());
}

#[test]
fn empty_on_uniform_background() {
    let w = 64;
    let h = 64;
    let pixels = vec![[0.8, 0.8, 0.8, 1.0]; w * h];
    assert!(find_photos(w, h, &pixels).is_empty());
}

#[test]
fn single_axis_aligned_rectangle() {
    let w = 200;
    let h = 150;
    let spec = (100.0, 75.0, 80.0, 70.0, 0.0);
    let pixels = scan_with_photos(w, h, &[spec]);
    let found = find_photos(w, h, &pixels);
    assert_eq!(found.len(), 1, "found: {:?}", found);
    let f = found[0];
    assert_close(f.center[0], 100.0, 3.0, "center_x");
    assert_close(f.center[1], 75.0, 3.0, "center_y");
    assert_close(f.width, 80.0, 5.0, "width");
    assert_close(f.height, 70.0, 5.0, "height");
    assert!(f.angle.abs() < 2.0, "angle: {}", f.angle);
}

#[test]
fn single_rotated_rectangle() {
    let w = 300;
    let h = 300;
    let spec = (150.0, 150.0, 100.0, 60.0, 12.0);
    let pixels = scan_with_photos(w, h, &[spec]);
    let found = find_photos(w, h, &pixels);
    assert_eq!(found.len(), 1, "found: {:?}", found);
    let f = found[0];
    assert_close(f.center[0], 150.0, 3.0, "center_x");
    assert_close(f.center[1], 150.0, 3.0, "center_y");
    // width/height: algorithm may not swap because angle 12 is within -45..45
    assert_close(f.width, 100.0, 8.0, "width");
    assert_close(f.height, 60.0, 8.0, "height");
    assert_close(f.angle, 12.0, 3.0, "angle");
}

#[test]
fn multiple_rectangles_reading_order() {
    let w = 400;
    let h = 400;
    let specs = [(100.0, 50.0, 80.0, 50.0, 0.0), (300.0, 200.0, 70.0, 60.0, 0.0), (150.0, 350.0, 100.0, 40.0, 0.0)];
    let pixels = scan_with_photos(w, h, &specs);
    let found = find_photos(w, h, &pixels);
    assert_eq!(found.len(), 3, "found: {:?}", found);
    // Reading order: top-to-bottom, then left-to-right.
    assert!(found[0].center[1] < found[1].center[1], "bad order: {:?}", found);
    assert!(found[1].center[1] < found[2].center[1], "bad order: {:?}", found);
    assert_close(found[0].center[0], 100.0, 3.0, "first x");
    assert_close(found[0].center[1], 50.0, 3.0, "first y");
    assert_close(found[1].center[0], 300.0, 3.0, "second x");
    assert_close(found[1].center[1], 200.0, 3.0, "second y");
    assert_close(found[2].center[0], 150.0, 3.0, "third x");
    assert_close(found[2].center[1], 350.0, 3.0, "third y");
}

#[test]
fn corners_zero_angle_exact() {
    let p = FoundPhoto { center: [10.0, 20.0], width: 40.0, height: 20.0, angle: 0.0, area: 0.5 };
    let c = p.corners();
    assert_eq!(c, [[-10.0, 10.0], [30.0, 10.0], [30.0, 30.0], [-10.0, 30.0],]);
}

#[test]
fn corners_rotated_consistency() {
    let p = FoundPhoto { center: [5.0, -5.0], width: 30.0, height: 10.0, angle: 30.0, area: 0.3 };
    let c = p.corners();

    // centroid equals center
    let mid = [(c[0][0] + c[1][0] + c[2][0] + c[3][0]) / 4.0, (c[0][1] + c[1][1] + c[2][1] + c[3][1]) / 4.0];
    assert_close(mid[0], 5.0, 1e-9, "mid x");
    assert_close(mid[1], -5.0, 1e-9, "mid y");

    // side lengths must equal width and height
    let d01 = ((c[0][0] - c[1][0]).powi(2) + (c[0][1] - c[1][1]).powi(2)).sqrt();
    let d12 = ((c[1][0] - c[2][0]).powi(2) + (c[1][1] - c[2][1]).powi(2)).sqrt();
    let d23 = ((c[2][0] - c[3][0]).powi(2) + (c[2][1] - c[3][1]).powi(2)).sqrt();
    let d30 = ((c[3][0] - c[0][0]).powi(2) + (c[3][1] - c[0][1]).powi(2)).sqrt();
    assert_close(d01, 30.0, 1e-9, "width d01");
    assert_close(d12, 10.0, 1e-9, "height d12");
    assert_close(d23, 30.0, 1e-9, "width d23");
    assert_close(d30, 10.0, 1e-9, "height d30");

    // all corners finite
    assert!(c.iter().all(|p| p.iter().all(|v| v.is_finite())));
}

#[test]
fn outputs_finite_and_in_range() {
    let w = 250;
    let h = 250;
    let specs = [(100.0, 100.0, 80.0, 60.0, 5.0), (150.0, 200.0, 60.0, 40.0, -20.0)];
    let pixels = scan_with_photos(w, h, &specs);
    let found = find_photos(w, h, &pixels);
    assert_eq!(found.len(), 2);
    for f in &found {
        assert!(f.center[0].is_finite());
        assert!(f.center[1].is_finite());
        assert!(f.width.is_finite() && f.width > 0.0);
        assert!(f.height.is_finite() && f.height > 0.0);
        assert!(f.angle.is_finite());
        assert!(f.angle >= -45.0 && f.angle <= 45.0);
        assert!(f.area.is_finite());
        assert!(f.area > 0.0 && f.area <= 1.0);
    }
}

#[test]
fn deterministic() {
    let w = 200;
    let h = 200;
    let spec = (100.0, 100.0, 70.0, 50.0, -10.0);
    let pixels = scan_with_photos(w, h, &[spec]);
    let a = find_photos(w, h, &pixels);
    let b = find_photos(w, h, &pixels);
    assert_eq!(a, b);
}

#[test]
fn odd_dimensions_supported() {
    let w = 123;
    let h = 87;
    let spec = (60.0, 40.0, 50.0, 30.0, 0.0);
    let pixels = scan_with_photos(w, h, &[spec]);
    let found = find_photos(w, h, &pixels);
    assert_eq!(found.len(), 1);
    let f = found[0];
    assert_close(f.center[0], 60.0, 3.0, "x");
    assert_close(f.center[1], 40.0, 3.0, "y");
}

#[test]
fn downsampled_large_image_detects_photo() {
    let w = 1000;
    let h = 800;
    let spec = (500.0, 400.0, 200.0, 150.0, 0.0);
    let pixels = scan_with_photos(w, h, &[spec]);
    let found = find_photos(w, h, &pixels);
    assert_eq!(found.len(), 1);
    let f = found[0];
    assert_close(f.center[0], 500.0, 5.0, "x");
    assert_close(f.center[1], 400.0, 5.0, "y");
    assert_close(f.width, 200.0, 10.0, "width");
    assert_close(f.height, 150.0, 10.0, "height");
}

#[test]
fn area_fraction_reasonable() {
    let w = 200;
    let h = 200;
    // 60x40 rectangle, total area 40000 -> fraction 0.06
    let spec = (100.0, 100.0, 60.0, 40.0, 0.0);
    let pixels = scan_with_photos(w, h, &[spec]);
    let found = find_photos(w, h, &pixels);
    assert_eq!(found.len(), 1);
    let f = found[0];
    assert!(f.area > 0.03 && f.area < 0.1, "area should be around 0.06, got {}", f.area);
}
