use photocraft_geom::Point;
use photocraft_paint::Dab;
use photocraft_paint::continuous::{LineTable, dab_reach, end_cap, segment};

fn assert_finite_in_range(values: &[f32], max_exclusive: f32, context: &str) {
    for (i, v) in values.iter().enumerate() {
        assert!(v.is_finite(), "{}: value at index {} is not finite: {}", context, i, v);
        assert!(*v >= 0.0 && *v < max_exclusive, "{}: value at index {} out of range: {}", context, i, v);
    }
}

#[test]
fn dab_reach_basic_bounds() {
    let d = Dab::round(Point::new(10.0, 20.0), 5.0, 1.0);
    let rect = dab_reach(&d);
    // center x = 10.0 floor = 10, rr = ceil(5.0) + 1 = 6
    assert_eq!(rect.x0, 4);
    assert_eq!(rect.y0, 14);
    assert_eq!(rect.x1, 17);
    assert_eq!(rect.y1, 27);
    assert_eq!(rect.width(), 13);
    assert_eq!(rect.height(), 13);
}

#[test]
fn dab_reach_zero_radius_and_negative_center() {
    let d = Dab::round(Point::new(-3.0, -7.0), 0.0, 1.0);
    let rect = dab_reach(&d);
    // rr = ceil(0.0) as i32 + 1 = 1
    assert_eq!(rect.width(), 3);
    assert_eq!(rect.height(), 3);
    assert_eq!(rect.x0, -4); // -3 - 1 = -4
    assert_eq!(rect.y0, -8);
}

#[test]
fn dab_reach_fractional_center_rounds_down() {
    let d = Dab::round(Point::new(2.3, 5.9), 2.0, 1.0);
    let rect = dab_reach(&d);
    // center.x.floor = 2, center.y.floor = 5, rr = ceil(2.0)+1 = 3
    assert_eq!(rect.x0, -1);
    assert_eq!(rect.y0, 2);
    assert_eq!(rect.x1, 6);
    assert_eq!(rect.y1, 9);
}

#[test]
fn end_cap_output_size_matches_rect() {
    let d = Dab::round(Point::new(4.0, 6.0), 3.0, 0.7);
    let mut out = Vec::new();
    let rect = end_cap(&d, 0.5, &mut out);
    assert_eq!(out.len(), (rect.width() as usize) * (rect.height() as usize));
}

#[test]
fn end_cap_alpha_zero_gives_zeros() {
    let d = Dab::round(Point::new(0.0, 0.0), 2.0, 0.0);
    let mut out = Vec::new();
    let _rect = end_cap(&d, 0.0, &mut out);
    assert!(!out.is_empty());
    for v in out.iter() {
        assert_eq!(*v, 0.0);
    }
}

#[test]
fn end_cap_full_alpha_center_high() {
    let d = Dab::round(Point::new(0.0, 0.0), 5.0, 1.0);
    let mut out = Vec::new();
    let _rect = end_cap(&d, 1.0, &mut out);
    let max_val = out.iter().cloned().fold(0.0f32, f32::max);
    assert!(max_val > 0.99 && max_val <= 1.0, "max coverage {} out of expected range", max_val);
    // also ensure no NaN/inf
    assert_finite_in_range(&out, 1.1, "end_cap full alpha");
}

#[test]
fn end_cap_values_in_range_and_finite() {
    let d = Dab::round(Point::new(0.5, -0.5), 3.0, 0.4);
    let mut out = Vec::new();
    let _ = end_cap(&d, 0.3, &mut out);
    assert!(!out.is_empty());
    assert_finite_in_range(&out, 0.8, "end_cap normal");
}

#[test]
fn end_cap_hardness_clamps_negative_and_over_one() {
    let d = Dab::round(Point::new(0.0, 0.0), 2.0, 0.5);
    let mut out = Vec::new();
    let _ = end_cap(&d, -1.0, &mut out);
    assert!(!out.is_empty());
    assert_finite_in_range(&out, 0.8, "end_cap negative hardness");

    let mut out2 = Vec::new();
    let _ = end_cap(&d, 2.0, &mut out2);
    assert!(!out2.is_empty());
    assert_finite_in_range(&out2, 0.8, "end_cap hardness over 1");
}

#[test]
fn segment_same_point_returns_zero_coverage() {
    let a = Dab::round(Point::new(3.0, 4.0), 2.0, 0.5);
    let b = a;
    let mut out = Vec::new();
    let rect = segment(&a, &b, 0.5, 0.25, &mut None, &mut out);
    assert!(!out.is_empty());
    for v in out.iter() {
        assert_eq!(*v, 0.0);
    }
    // rect should be union of two identical dab reaches
    let expected_rect = dab_reach(&a);
    assert_eq!(rect, expected_rect);
}

#[test]
fn segment_constant_radius_output_in_range() {
    let a = Dab::round(Point::new(0.0, 0.0), 2.0, 0.6);
    let b = Dab::round(Point::new(1.0, 0.0), 2.0, 0.6);
    let mut out = Vec::new();
    let _rect = segment(&a, &b, 0.5, 0.25, &mut None, &mut out);
    assert!(!out.is_empty());
    assert_finite_in_range(&out, 1.0, "segment constant");
    // ensure at least some non-zero coverage near the path
    let any_non_zero = out.iter().any(|v| *v > 0.0);
    assert!(any_non_zero, "expected non-zero coverage");
}

#[test]
fn segment_varying_radius_output_in_range() {
    let a = Dab::round(Point::new(0.0, 0.0), 1.0, 0.4);
    let b = Dab::round(Point::new(2.0, 0.0), 3.0, 0.8);
    let mut out = Vec::new();
    let _rect = segment(&a, &b, 0.5, 0.25, &mut None, &mut out);
    assert!(!out.is_empty());
    assert_finite_in_range(&out, 1.0, "segment varying");
    let any_non_zero = out.iter().any(|v| *v > 0.0);
    assert!(any_non_zero, "expected non-zero coverage");
}

#[test]
fn segment_deterministic_with_fresh_table_vs_none() {
    let a = Dab::round(Point::new(0.0, 0.0), 2.5, 0.5);
    let b = Dab::round(Point::new(1.5, 0.0), 2.5, 0.5);
    let hardness = 0.4;
    let spacing = 0.25;

    let mut out1 = Vec::new();
    let _ = segment(&a, &b, hardness, spacing, &mut None, &mut out1);

    let table = LineTable::new(&a, hardness);
    let mut table_opt = Some(table);
    let mut out2 = Vec::new();
    let _ = segment(&a, &b, hardness, spacing, &mut table_opt, &mut out2);

    assert_eq!(out1.len(), out2.len());
    for (x, y) in out1.iter().zip(out2.iter()) {
        assert!((x - y).abs() < 1e-5, "mismatch: {} vs {}", x, y);
    }
}

#[test]
fn segment_reuses_table_without_changing_result() {
    let a = Dab::round(Point::new(0.0, 0.0), 2.0, 0.7);
    let b1 = Dab::round(Point::new(1.0, 0.0), 2.0, 0.7);
    let b2 = Dab::round(Point::new(0.0, 1.0), 2.0, 0.7);
    let hardness = 0.6;
    let spacing = 0.25;

    // first call with None
    let mut out1 = Vec::new();
    let _ = segment(&a, &b1, hardness, spacing, &mut None, &mut out1);

    // create a table and use it for b1
    let table = LineTable::new(&a, hardness);
    let mut table_opt = Some(table);
    let mut out2 = Vec::new();
    let _ = segment(&a, &b1, hardness, spacing, &mut table_opt, &mut out2);
    assert_eq!(out1, out2, "result differs with pre-built table");

    // reuse same table (now inside table_opt) for b2, should still match a fresh None call for b2
    let mut out3 = Vec::new();
    let _ = segment(&a, &b2, hardness, spacing, &mut table_opt, &mut out3);
    let mut out4 = Vec::new();
    let _ = segment(&a, &b2, hardness, spacing, &mut None, &mut out4);
    assert_eq!(out3, out4, "result differs when reusing table for different end point");
}

#[test]
fn segment_varying_alpha_and_radius_handles_interpolation() {
    let a = Dab::round(Point::new(0.0, 0.0), 1.5, 0.2);
    let mut b = Dab::round(Point::new(2.0, 1.0), 3.0, 0.9);
    // ensure roundness differs as well
    b.roundness = 0.7;
    let mut out = Vec::new();
    let _rect = segment(&a, &b, 0.5, 0.25, &mut None, &mut out);
    assert!(!out.is_empty());
    assert_finite_in_range(&out, 1.0, "segment varying alpha/roundness");
}

#[test]
fn segment_nan_radius_no_panic_and_finite() {
    let mut a = Dab::round(Point::new(0.0, 0.0), 2.0, 0.5);
    a.radius = f32::NAN;
    let b = Dab::round(Point::new(1.0, 0.0), 2.0, 0.5);
    let mut out = Vec::new();
    // should not panic, output should be all zeros because step_px gives NaN for a.radius
    let _ = segment(&a, &b, 0.5, 0.25, &mut None, &mut out);
    assert!(!out.is_empty());
    assert_finite_in_range(&out, 1.0, "segment with NaN radius");
    let all_zero = out.iter().all(|v| *v == 0.0);
    assert!(all_zero, "expected all zero coverage for NaN radius");
}

#[test]
fn segment_inf_alpha_no_panic_and_finite() {
    let a = Dab::round(Point::new(0.0, 0.0), 2.0, f32::INFINITY);
    let b = Dab::round(Point::new(1.0, 0.0), 2.0, f32::INFINITY);
    let mut out = Vec::new();
    let _ = segment(&a, &b, 0.5, 0.25, &mut None, &mut out);
    assert!(!out.is_empty());
    assert_finite_in_range(&out, 1.0, "segment with inf alpha");
}

#[test]
fn line_table_creation_basic() {
    let d = Dab::round(Point::new(0.0, 0.0), 2.0, 0.5);
    let _table = LineTable::new(&d, 0.5);
    // just ensure no panic
}

#[test]
fn segment_does_not_panic_with_extreme_spacing() {
    let a = Dab::round(Point::new(0.0, 0.0), 2.0, 0.5);
    let b = Dab::round(Point::new(1.0, 0.0), 2.0, 0.5);
    let mut out = Vec::new();
    // spacing very small or very large, but finite
    for spacing in [0.01f32, 100.0f32] {
        let _ = segment(&a, &b, 0.5, spacing, &mut None, &mut out);
        assert!(!out.is_empty());
        assert_finite_in_range(&out, 1.0, &format!("segment with spacing {}", spacing));
    }
}

#[test]
fn end_cap_output_matches_expected_shape() {
    // Round brush, center at (0.5, 0.5) so that the pixel grid is symmetric about the center.
    // Hardness = 1.0, alpha = 1.0 -> half-dab coverage should be symmetric horizontally and vertically.
    let d = Dab::round(Point::new(0.5, 0.5), 4.0, 1.0);
    let mut out = Vec::new();
    let rect = end_cap(&d, 1.0, &mut out);
    let w = rect.width() as usize;
    let h = rect.height() as usize;

    // Center pixel index (within the rect) corresponds to pixel center (0.5, 0.5).
    let cx = rect.x0 + (w / 2) as i32;
    let cy = rect.y0 + (h / 2) as i32;
    let center_idx = ((cy - rect.y0) as usize) * w + ((cx - rect.x0) as usize);
    assert!(center_idx < out.len(), "center index out of bounds");
    let center_val = out[center_idx];
    assert!(center_val > 0.99 && center_val <= 1.0, "center coverage {}", center_val);

    // Horizontal symmetry: index mirroring (xx with w-1-xx) works because center is at 0.5.
    for yy in 0..h {
        for xx in 0..w / 2 {
            let left = out[yy * w + xx];
            let right = out[yy * w + (w - 1 - xx)];
            assert!((left - right).abs() < 1e-4, "horizontal asymmetry at row {}, col {} vs {}", yy, xx, w - 1 - xx);
        }
    }

    // Vertical symmetry: index mirroring (yy with h-1-yy).
    for yy in 0..h / 2 {
        for xx in 0..w {
            let top = out[yy * w + xx];
            let bottom = out[(h - 1 - yy) * w + xx];
            assert!((top - bottom).abs() < 1e-4, "vertical asymmetry at col {}, row {} vs {}", xx, yy, h - 1 - yy);
        }
    }
}
