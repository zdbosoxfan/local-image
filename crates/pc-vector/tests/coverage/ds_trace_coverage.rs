use photocraft_geom::Rect;
use photocraft_vector::trace::{contours, trace_mask};

fn rect(w: i32, h: i32) -> Rect {
    Rect::new(0, 0, w, h)
}

fn filled_rect_mask(w: usize, h: usize, x0: usize, x1: usize, y0: usize, y1: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; w * h];
    for y in y0..y1 {
        for x in x0..x1 {
            v[y * w + x] = 1.0;
        }
    }
    v
}

fn circle_mask(size: usize, radius: f64) -> Vec<f32> {
    let mut v = vec![0.0f32; size * size];
    let c = (size as f64 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f64 - c;
            let dy = y as f64 - c;
            if dx.hypot(dy) <= radius {
                v[y * size + x] = 1.0;
            }
        }
    }
    v
}

fn shoelace_area(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len();
    let sum: f64 = (0..n)
        .map(|i| {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            p.0 * q.1 - q.0 * p.1
        })
        .sum();
    sum * 0.5
}

#[test]
fn empty_rect_has_no_contours() {
    let r = rect(0, 0);
    let values: Vec<f32> = vec![];
    assert!(contours(&values, r, 0.0).is_empty());

    let p = trace_mask(&values, r, 0.0, 1.0);
    assert!(p.subpaths.is_empty());
}

#[test]
fn zero_width_has_no_contours() {
    let r = rect(0, 5);
    let values: Vec<f32> = vec![];
    assert!(contours(&values, r, 0.0).is_empty());
}

#[test]
fn zero_height_has_no_contours() {
    let r = rect(5, 0);
    let values: Vec<f32> = vec![];
    assert!(contours(&values, r, 0.0).is_empty());
}

#[test]
fn single_pixel_filled_yields_contour() {
    let r = rect(1, 1);
    let values = vec![1.0f32];
    let c = contours(&values, r, 0.0);
    assert_eq!(c.len(), 1);
    assert!(c[0].len() >= 4);
    assert!(c[0].iter().all(|p| p.0.is_finite() && p.1.is_finite()));

    let p = trace_mask(&values, r, 0.0, 1.0);
    assert_eq!(p.subpaths.len(), 1);
    // A single pixel produces at least one fitted Bézier segment (one or more knots).
    assert!(!p.subpaths[0].knots.is_empty());
}

#[test]
fn outside_matching_values_yields_no_contours() {
    let r = rect(4, 4);
    let values = vec![1.0f32; 16];
    assert!(contours(&values, r, 1.0).is_empty());
}

#[test]
fn outside_nan_does_not_panic() {
    let r = rect(4, 4);
    let values = vec![1.0f32; 16];
    // Finite output is not guaranteed when the outside value is NaN; we only check for no panic.
    let _ = contours(&values, r, f32::NAN);
}

#[test]
fn values_with_nan_do_not_panic() {
    let r = rect(4, 4);
    let mut values = vec![1.0f32; 16];
    values[5] = f32::NAN;
    values[6] = f32::NAN;
    // Non-finite coordinates may leak through; we only ensure the functions complete without panicking.
    let _ = contours(&values, r, 0.0);
    let _ = trace_mask(&values, r, 0.0, 1.0);
}

#[test]
fn threshold_exact_half_yields_contour() {
    let r = rect(3, 3);
    let values = vec![0.5f32; 9];
    let c = contours(&values, r, 0.0);
    assert_eq!(c.len(), 1);
}

#[test]
fn values_below_threshold_yield_no_contours() {
    let r = rect(4, 4);
    let values = vec![0.49f32; 16];
    assert!(contours(&values, r, 0.0).is_empty());
}

#[test]
fn filled_square_contour_has_negative_area() {
    let (w, h) = (8, 7);
    let values = filled_rect_mask(w, h, 2, 7, 2, 6);
    let r = rect(w as i32, h as i32);
    let c = contours(&values, r, 0.0);
    assert_eq!(c.len(), 1);
    let area = shoelace_area(&c[0]);
    assert!(area < 0.0, "area {area}");
}

#[test]
fn traced_rect_has_corner_knots() {
    let (w, h) = (40, 30);
    let values = filled_rect_mask(w, h, 5, 35, 5, 25);
    let r = rect(w as i32, h as i32);
    let p = trace_mask(&values, r, 0.0, 1.0);
    assert_eq!(p.subpaths.len(), 1);
    let knots = &p.subpaths[0].knots;
    assert!(knots.len() >= 4 && knots.len() <= 30, "{} knots", knots.len());
    assert!(knots.iter().filter(|k| !k.smooth).count() >= 4);
}

#[test]
fn trace_mask_is_deterministic() {
    let (w, h) = (32, 24);
    let values = filled_rect_mask(w, h, 6, 26, 5, 19);
    let r = rect(w as i32, h as i32);

    let a = trace_mask(&values, r, 0.0, 1.0);
    let b = trace_mask(&values, r, 0.0, 1.0);

    assert_eq!(a.subpaths.len(), b.subpaths.len());
    for (sa, sb) in a.subpaths.iter().zip(&b.subpaths) {
        assert_eq!(sa.knots.len(), sb.knots.len());
        for (ka, kb) in sa.knots.iter().zip(&sb.knots) {
            assert!((ka.anchor.x - kb.anchor.x).abs() < 1e-12);
            assert!((ka.anchor.y - kb.anchor.y).abs() < 1e-12);
            assert_eq!(ka.smooth, kb.smooth);
        }
    }
}

#[test]
fn tighter_tolerance_gives_more_knots_than_loose() {
    let size = 80;
    let values = circle_mask(size, 25.0);
    let r = rect(size as i32, size as i32);

    let loose = trace_mask(&values, r, 0.0, 4.0);
    let tight = trace_mask(&values, r, 0.0, 0.1);

    let loose_count: usize = loose.subpaths.iter().map(|s| s.knots.len()).sum();
    let tight_count: usize = tight.subpaths.iter().map(|s| s.knots.len()).sum();

    assert!(loose_count < tight_count, "loose={loose_count}, tight={tight_count}");
}

#[test]
fn contours_are_deterministic() {
    let (w, h) = (16, 16);
    let values = filled_rect_mask(w, h, 3, 13, 3, 13);
    let r = rect(w as i32, h as i32);

    let a = contours(&values, r, 0.0);
    let b = contours(&values, r, 0.0);
    assert_eq!(a, b);
}

#[test]
fn nested_contours_yield_two_loops() {
    let size = 20;
    let mut values = vec![0.0f32; size * size];
    for y in 4..16 {
        for x in 4..16 {
            values[y * size + x] = 1.0;
        }
    }
    for y in 8..12 {
        for x in 8..12 {
            values[y * size + x] = 0.0;
        }
    }
    let r = rect(size as i32, size as i32);
    let c = contours(&values, r, 0.0);
    assert_eq!(c.len(), 2);

    let p = trace_mask(&values, r, 0.0, 1.0);
    assert_eq!(p.subpaths.len(), 2);
}

#[test]
fn all_zeros_yield_no_subpaths() {
    let r = rect(10, 10);
    let values = vec![0.0f32; 100];
    let p = trace_mask(&values, r, 0.0, 1.0);
    assert!(p.subpaths.is_empty());
}

#[test]
fn zero_tolerance_is_clamped_and_does_not_panic() {
    let r = rect(5, 5);
    let values = vec![1.0f32; 25];
    let p = trace_mask(&values, r, 0.0, 0.0);
    assert!(!p.subpaths.is_empty());
}

#[test]
fn very_large_tolerance_still_returns_path() {
    let (w, h) = (8, 8);
    let values = filled_rect_mask(w, h, 2, 6, 2, 6);
    let r = rect(w as i32, h as i32);
    let p = trace_mask(&values, r, 0.0, 1000.0);
    assert!(!p.subpaths.is_empty());
}

#[test]
fn infinities_do_not_panic() {
    let r = rect(4, 4);
    let mut values = vec![0.0f32; 16];
    values[5] = f32::INFINITY;
    values[6] = f32::NEG_INFINITY;

    // The library does not promise finite output for non-finite input; we only ensure it doesn't panic.
    let _ = contours(&values, r, 0.0);
    let _ = trace_mask(&values, r, 0.0, 1.0);
}
