use lightcraft_pipeline::upright::{Analysis, Segment, analyze, detect_segments, solve_guided, vanishing_point};
use lightcraft_raster::Plane;

fn plane_from(data: Vec<f32>, width: usize, height: usize) -> Plane {
    Plane { width, height, data }
}

fn uniform_plane(width: usize, height: usize, value: f32) -> Plane {
    plane_from(vec![value; width * height], width, height)
}

fn vertical_line_plane(width: usize, height: usize, x: usize, thickness: usize, brightness: f32, background: f32) -> Plane {
    let mut data = vec![background; width * height];
    for y in 0..height {
        for dx in 0..thickness {
            let xx = x + dx;
            if xx < width {
                data[y * width + xx] = brightness;
            }
        }
    }
    plane_from(data, width, height)
}

fn vertical_lines_plane(width: usize, height: usize, xs: &[usize], thickness: usize, brightness: f32, background: f32) -> Plane {
    let mut data = vec![background; width * height];
    for y in 0..height {
        for &x in xs {
            for dx in 0..thickness {
                let xx = x + dx;
                if xx < width {
                    data[y * width + xx] = brightness;
                }
            }
        }
    }
    plane_from(data, width, height)
}

fn horizontal_line_plane(width: usize, height: usize, y: usize, thickness: usize, brightness: f32, background: f32) -> Plane {
    let mut data = vec![background; width * height];
    for dy in 0..thickness {
        let yy = y + dy;
        if yy < height {
            for x in 0..width {
                data[yy * width + x] = brightness;
            }
        }
    }
    plane_from(data, width, height)
}

fn grid_plane(width: usize, height: usize) -> Plane {
    let mut data = vec![1.0f32; width * height];
    let xs = [28usize, 64, 100];
    let ys = [28usize, 64, 100];
    let thickness = 2usize;
    for y in 0..height {
        for &lx in &xs {
            for dx in 0..thickness {
                let xx = lx + dx;
                if xx < width {
                    data[y * width + xx] = 0.0;
                }
            }
        }
    }
    for x in 0..width {
        for &ly in &ys {
            for dy in 0..thickness {
                let yy = ly + dy;
                if yy < height {
                    data[yy * width + x] = 0.0;
                }
            }
        }
    }
    plane_from(data, width, height)
}

fn tilt_from_vertical(s: &Segment) -> f64 {
    let dx = s.b.x - s.a.x;
    let dy = s.b.y - s.a.y;
    dx.abs().atan2(dy.abs()).to_degrees()
}

fn assert_identity(m: &[f64; 9]) {
    let expected = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    for i in 0..9 {
        assert!((m[i] - expected[i]).abs() < 1e-9, "element {i}: got {}, expected {}", m[i], expected[i]);
    }
}

#[test]
fn detect_segments_empty_for_tiny_plane() {
    let p = uniform_plane(15, 15, 0.5);
    assert!(detect_segments(&p).is_empty());
}

#[test]
fn detect_segments_empty_for_uniform_plane() {
    let p = uniform_plane(64, 64, 0.5);
    assert!(detect_segments(&p).is_empty());
}

#[test]
fn detect_segments_finds_vertical_line() {
    let p = vertical_line_plane(64, 64, 30, 3, 1.0, 0.0);
    let segs = detect_segments(&p);
    assert!(!segs.is_empty());
    let vertical_count = segs.iter().filter(|s| tilt_from_vertical(s) < 30.0).count();
    assert!(vertical_count >= 1, "expected at least one near-vertical segment");
}

#[test]
fn detect_segments_finds_horizontal_line() {
    let p = horizontal_line_plane(64, 64, 30, 3, 1.0, 0.0);
    let segs = detect_segments(&p);
    assert!(!segs.is_empty());
    let horizontal_count = segs.iter().filter(|s| tilt_from_vertical(s) > 60.0).count();
    assert!(horizontal_count >= 1, "expected at least one near-horizontal segment");
}

#[test]
fn detect_segments_deterministic() {
    let p = vertical_line_plane(64, 64, 30, 3, 1.0, 0.0);
    let a = detect_segments(&p);
    let b = detect_segments(&p);
    assert_eq!(a, b);
}

#[test]
fn detected_segments_have_positive_length() {
    let p = vertical_line_plane(64, 64, 30, 3, 1.0, 0.0);
    let segs = detect_segments(&p);
    assert!(!segs.is_empty());
    for s in &segs {
        assert!(s.len() > 0.0);
        assert!(!s.is_empty());
    }
}

#[test]
fn vanishing_point_none_for_empty() {
    assert!(vanishing_point(&[]).is_none());
}

#[test]
fn vanishing_point_none_for_single_segment() {
    let p = vertical_line_plane(64, 64, 30, 3, 1.0, 0.0);
    let segs = detect_segments(&p);
    let seg = segs[0];
    assert!(vanishing_point(std::slice::from_ref(&seg)).is_none());
}

#[test]
fn vanishing_point_some_for_parallel_vertical_segments() {
    let p = vertical_lines_plane(64, 64, &[20, 44], 3, 1.0, 0.0);
    let segs = detect_segments(&p);
    let vertical: Vec<Segment> = segs.into_iter().filter(|s| tilt_from_vertical(s) < 30.0).collect();
    assert!(vertical.len() >= 2, "expected at least two vertical segments, got {}", vertical.len());
    let vp = vanishing_point(&vertical);
    assert!(vp.is_some());
    let (v, total) = vp.unwrap();
    assert!(v[1].abs() > 0.9, "vertical VP should be near [0, ±1, 0], got {:?}", v);
    assert!(v[0].abs() < 0.1);
    assert!(v[2].abs() < 0.1);
    assert!(total > 0.0);
}

#[test]
fn analyze_empty_segments_is_default() {
    let a = analyze(&[]);
    assert!(a.vertical.is_empty());
    assert!(a.horizontal.is_empty());
    assert!(a.vertical_vp.is_none());
    assert!(a.horizontal_vp.is_none());
}

#[test]
fn analyze_grid_finds_both_vps() {
    let p = grid_plane(128, 128);
    let segs = detect_segments(&p);
    let a = analyze(&segs);
    assert!(!a.vertical.is_empty(), "expected vertical segments");
    assert!(!a.horizontal.is_empty(), "expected horizontal segments");
    assert!(a.vertical_vp.is_some(), "expected vertical VP");
    assert!(a.horizontal_vp.is_some(), "expected horizontal VP");
}

#[test]
fn solve_guided_empty_guides_is_identity() {
    let h = solve_guided(&[]);
    assert_identity(&h.0);
}

#[test]
fn solve_guided_single_vertical_guide_is_identity() {
    let p = vertical_line_plane(64, 64, 30, 3, 1.0, 0.0);
    let segs = detect_segments(&p);
    let vertical: Vec<Segment> = segs.into_iter().filter(|s| tilt_from_vertical(s) < 30.0).collect();
    assert!(!vertical.is_empty());
    let h = solve_guided(std::slice::from_ref(&vertical[0]));
    assert_identity(&h.0);
}

#[test]
fn analysis_default_is_empty() {
    let a = Analysis::default();
    assert!(a.vertical.is_empty());
    assert!(a.horizontal.is_empty());
    assert!(a.vertical_vp.is_none());
    assert!(a.horizontal_vp.is_none());
}

#[test]
fn detect_segments_odd_size_vertical_line() {
    let p = vertical_line_plane(33, 37, 15, 3, 1.0, 0.0);
    assert!(!detect_segments(&p).is_empty());
}

#[test]
fn detect_segments_nan_plane_empty_no_panic() {
    let mut data = vec![0.0f32; 64 * 64];
    for d in data.iter_mut() {
        *d = f32::NAN;
    }
    let p = plane_from(data, 64, 64);
    let segs = detect_segments(&p);
    assert!(segs.is_empty());
}

#[test]
fn detect_segments_inf_plane_empty_no_panic() {
    let p = uniform_plane(64, 64, f32::INFINITY);
    let segs = detect_segments(&p);
    assert!(segs.is_empty());
}
