use lightcraft_geom::Homography;
use lightcraft_merge::linalg;
use lightcraft_merge::pano::Projection;
use lightcraft_merge::pano::blend::{Blender, Buf, down, push_pull, up};
use lightcraft_merge::pano::camera::{Camera, bundle_adjust, focals_from_homography, initial_cameras, largest_component, straighten};
use lightcraft_merge::{MergeError, PanoOptions, no_progress, stitch};

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n < 1e-12 {
        return v;
    }
    [v[0] / n, v[1] / n, v[2] / n]
}

fn assert_direction_close(a: [f64; 3], b: [f64; 3], eps: f64) {
    let na = normalize(a);
    let nb = normalize(b);
    let dot = na[0] * nb[0] + na[1] * nb[1] + na[2] * nb[2];
    assert!((dot - 1.0).abs() < eps, "directions not close: {a:?} vs {b:?}, dot {dot}");
}

#[test]
fn buf_new_has_correct_dimensions_and_zeros() {
    let b = Buf::new(3, 2, 2);
    assert_eq!(b.w, 3);
    assert_eq!(b.h, 2);
    assert_eq!(b.c, 2);
    assert_eq!(b.data.len(), 12);
    assert!(b.data.iter().all(|&v| v == 0.0));
}

#[test]
fn down_constant_preserves_value_and_halves_dimensions() {
    let mut b = Buf::new(5, 3, 2);
    b.data.fill(2.5);
    let d = down(&b);
    assert_eq!(d.w, 3);
    assert_eq!(d.h, 2);
    assert_eq!(d.c, 2);
    assert!(d.data.iter().all(|&v| (v - 2.5).abs() < 1e-6));
}

#[test]
fn up_constant_preserves_value() {
    let mut b = Buf::new(4, 4, 1);
    b.data.fill(3.0);
    let u = up(&b, 8, 8);
    assert_eq!(u.w, 8);
    assert_eq!(u.h, 8);
    assert!(u.data.iter().all(|&v| (v - 3.0).abs() < 1e-6));
}

#[test]
fn push_pull_all_positive_weights_unchanged() {
    let mut vals = Buf::new(4, 4, 2);
    for (i, x) in vals.data.iter_mut().enumerate() {
        *x = (i as f32 * 1.3).sin();
    }
    let orig = vals.data.clone();
    let wgt = vec![1.0f32; 4 * 4];
    push_pull(&mut vals, &wgt);
    assert_eq!(vals.data, orig);
}

#[test]
fn push_pull_all_zero_weights_unchanged() {
    let mut vals = Buf::new(4, 4, 1);
    vals.data.fill(0.42);
    let orig = vals.data.clone();
    let wgt = vec![0.0f32; 4 * 4];
    push_pull(&mut vals, &wgt);
    assert_eq!(vals.data, orig);
}

#[test]
fn push_pull_fills_holes_from_valid_pixels() {
    let (w, h) = (20, 10);
    let mut vals = Buf::new(w, h, 1);
    let mut wgt = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w / 2 {
            vals.data[y * w + x] = 0.7;
            wgt[y * w + x] = 1.0;
        }
    }
    push_pull(&mut vals, &wgt);
    assert!(vals.data.iter().all(|&v| (v - 0.7).abs() < 1e-4));
}

#[test]
fn blender_no_tiles_returns_zero_canvas() {
    let b = Blender::new(8, 8, 3);
    let out = b.finish();
    assert_eq!(out.w, 8);
    assert_eq!(out.h, 8);
    assert!(out.data.iter().all(|&v| v == 0.0));
}

#[test]
fn blender_single_full_tile_reconstructs_exactly() {
    let (w, h) = (32, 24);
    let mut vals = Buf::new(w, h, 3);
    for (i, x) in vals.data.iter_mut().enumerate() {
        *x = ((i * 37) % 101) as f32 / 101.0;
    }
    let mut mask = Buf::new(w, h, 1);
    mask.data.fill(1.0);
    let mut b = Blender::new(w, h, 3);
    b.add(0, 0, vals.clone(), mask);
    let out = b.finish();
    for (a, b) in vals.data.iter().zip(&out.data) {
        assert!((a - b).abs() < 1e-4);
    }
}

#[test]
fn blender_two_tiles_blend_without_a_step() {
    let (w, h) = (128, 32);
    let mut b = Blender::new(w, h, 4);
    for (val, side) in [(0.0f32, 0), (1.0, 1)] {
        let mut v = Buf::new(w, h, 3);
        v.data.fill(val);
        let mut m = Buf::new(w, h, 1);
        for y in 0..h {
            for x in 0..w {
                m.data[y * w + x] = if (x >= w / 2) == (side == 1) { 1.0 } else { 0.0 };
            }
        }
        b.add(0, 0, v, m);
    }
    let out = b.finish();
    let row: Vec<f32> = (0..w).map(|x| out.data[(16 * w + x) * 3]).collect();
    let max_step = row.windows(2).map(|p| (p[1] - p[0]).abs()).fold(0.0, f32::max);
    assert!(max_step < 0.15, "max step {max_step}");
    assert!(row[2].abs() < 0.02 && (row[w - 3] - 1.0).abs() < 0.02);
}

#[test]
fn blender_zero_mask_tile_is_ignored() {
    let (w, h) = (16, 16);
    let mut vals = Buf::new(w, h, 3);
    vals.data.fill(1.0);
    let mut mask = Buf::new(w, h, 1);
    mask.data.fill(0.0);
    let mut b = Blender::new(w, h, 3);
    b.add(0, 0, vals, mask);
    let out = b.finish();
    assert!(out.data.iter().all(|&v| v == 0.0));
}

#[test]
fn camera_ray_project_round_trip() {
    let cam = Camera { f: 100.0, r: linalg::I3, cx: 50.0, cy: 40.0 };
    for &(x, y) in &[(0.0, 0.0), (50.0, 40.0), (10.0, 70.0), (99.9, 79.9)] {
        let Some((px, py)) = cam.project(cam.ray(x, y)) else {
            panic!("project returned None for pixel ({x},{y})");
        };
        assert!((px - x).abs() < 1e-6 && (py - y).abs() < 1e-6, "round trip failed: ({x},{y}) -> ({px},{py})");
    }
}

#[test]
fn camera_ray_project_round_trip_with_rotation() {
    let r = linalg::rodrigues([0.1, 0.2, 0.05]);
    let cam = Camera { f: 200.0, r, cx: 100.0, cy: 80.0 };
    for &(x, y) in &[(0.0, 0.0), (100.0, 80.0), (25.0, 25.0), (199.9, 159.9)] {
        let Some((px, py)) = cam.project(cam.ray(x, y)) else {
            panic!("project returned None for pixel ({x},{y})");
        };
        assert!((px - x).abs() < 1e-6 && (py - y).abs() < 1e-6, "rotated round trip failed: ({x},{y}) -> ({px},{py})");
    }
}

#[test]
fn camera_project_returns_none_when_behind() {
    let cam = Camera { f: 100.0, r: linalg::I3, cx: 50.0, cy: 40.0 };
    assert!(cam.project([0.0, 0.0, 0.0]).is_none());
    assert!(cam.project([0.0, 0.0, -1.0]).is_none());
    assert!(cam.project([1.0, 1.0, 0.0]).is_none());
}

#[test]
fn focals_from_homography_recovers_f() {
    let f = 900.0;
    let r = linalg::rodrigues([0.05, 0.3, 0.02]);
    let k = [[f, 0.0, 0.0], [0.0, f, 0.0], [0.0, 0.0, 1.0]];
    let kinv = [[1.0 / f, 0.0, 0.0], [0.0, 1.0 / f, 0.0], [0.0, 0.0, 1.0]];
    let h = linalg::mul3(&linalg::mul3(&k, &r), &kinv);
    let hh = Homography([h[0][0], h[0][1], h[0][2], h[1][0], h[1][1], h[1][2], h[2][0], h[2][1], h[2][2]]);
    let (a, b) = focals_from_homography(&hh);
    assert!((a.unwrap() - f).abs() < 1.0, "{a:?}");
    assert!((b.unwrap() - f).abs() < 1.0, "{b:?}");
}

#[test]
fn focals_from_homography_zero_matrix_returns_none() {
    let z = Homography([0.0; 9]);
    assert_eq!(focals_from_homography(&z), (None, None));
}

#[test]
fn largest_component_no_pairs_returns_single_node() {
    let comp = largest_component(3, &[]);
    assert_eq!(comp.len(), 1);
    assert!(comp[0] < 3);
}

#[test]
fn largest_component_returns_largest_connected_set() {
    let make_pair =
        |i: usize, j: usize| lightcraft_merge::pano::camera::PairMatch { i, j, h: Homography([0.0; 9]), pts: Vec::new(), levels: Vec::new() };
    let pairs = vec![make_pair(0, 1), make_pair(1, 2), make_pair(2, 0)];
    let mut comp = largest_component(4, &pairs);
    comp.sort_unstable();
    assert_eq!(comp, vec![0, 1, 2]);
}

#[test]
fn initial_cameras_single_root_identity() {
    let sizes = [(100, 80), (200, 100), (300, 150)];
    let cams = initial_cameras(&[1], &sizes, &[], 120.0);
    assert_eq!(cams.len(), 3);
    assert!(cams[0].is_none());
    assert!(cams[2].is_none());
    let c = cams[1].expect("camera 1 should be set");
    assert!((c.f - 120.0).abs() < 1e-12);
    assert_eq!(c.r, linalg::I3);
    assert!((c.cx - 100.0).abs() < 1e-12 && (c.cy - 50.0).abs() < 1e-12);
}

#[test]
fn initial_cameras_empty_ids_returns_all_none() {
    let sizes = [(10, 10), (20, 20)];
    let cams = initial_cameras(&[], &sizes, &[], 100.0);
    assert_eq!(cams, vec![None, None]);
}

#[test]
fn straighten_single_camera_does_not_panic() {
    let mut cams = vec![Some(Camera { f: 100.0, r: linalg::I3, cx: 50.0, cy: 50.0 })];
    straighten(&mut cams);
    let c = cams[0].expect("camera still present");
    assert!(c.f.is_finite());
    for row in c.r {
        for v in row {
            assert!(v.is_finite());
        }
    }
}

#[test]
fn bundle_adjust_no_pairs_unchanged() {
    let cams = vec![Some(Camera { f: 100.0, r: linalg::I3, cx: 50.0, cy: 50.0 }), None];
    let adj = bundle_adjust(&cams, &[], 0, 10);
    assert_eq!(adj.cameras, cams);
    assert_eq!(adj.rms_before, 0.0);
    assert_eq!(adj.rms, 0.0);
}

#[test]
fn projection_spherical_round_trip() {
    for d in [[0.3, 0.1, 1.0], [0.0, 0.0, 1.0], [-0.2, 0.4, 1.0], [0.9, -0.3, 1.0]] {
        let Some((u, v)) = Projection::Spherical.forward(d, 100.0) else {
            panic!("spherical forward returned None for {d:?}");
        };
        let d2 = Projection::Spherical.inverse(u, v, 100.0);
        assert_direction_close(d, d2, 1e-6);
    }
}

#[test]
fn projection_cylindrical_round_trip() {
    for d in [[0.3, 0.1, 1.0], [0.0, 0.0, 1.0], [-0.2, 0.4, 1.0], [0.9, -0.3, 1.0]] {
        let Some((u, v)) = Projection::Cylindrical.forward(d, 100.0) else {
            panic!("cylindrical forward returned None for {d:?}");
        };
        let d2 = Projection::Cylindrical.inverse(u, v, 100.0);
        assert_direction_close(d, d2, 1e-6);
    }
}

#[test]
fn projection_perspective_round_trip() {
    for d in [[0.3, 0.1, 1.0], [0.0, 0.0, 1.0], [-0.2, 0.4, 1.0], [0.9, -0.3, 1.0]] {
        let Some((u, v)) = Projection::Perspective.forward(d, 100.0) else {
            panic!("perspective forward returned None for {d:?}");
        };
        let d2 = Projection::Perspective.inverse(u, v, 100.0);
        assert_direction_close(d, d2, 1e-6);
    }
}

#[test]
fn projection_perspective_behind_returns_none() {
    assert!(Projection::Perspective.forward([0.0, 0.0, 0.0], 100.0).is_none());
    assert!(Projection::Perspective.forward([1.0, 0.0, 0.0], 100.0).is_none());
    assert!(Projection::Perspective.forward([0.0, 0.0, -1.0], 100.0).is_none());
}

#[test]
fn stitch_with_too_few_frames_errors() {
    let err = stitch(vec![], &PanoOptions::default(), &no_progress).unwrap_err();
    match err {
        MergeError::TooFew(n, need) => {
            assert_eq!(n, 0);
            assert_eq!(need, 2);
        }
        other => panic!("expected TooFew, got {other:?}"),
    }
}
