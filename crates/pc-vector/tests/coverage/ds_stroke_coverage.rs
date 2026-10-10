use photocraft_doc::{LineCap, LineJoin};
use photocraft_vector::stroke::{dash_polyline, stroke_polygons};
use photocraft_vector::{Polyline, StrokeStyle};

fn polyline(pts: &[(f64, f64)], closed: bool) -> Polyline {
    Polyline { pts: pts.to_vec(), knot: vec![true; pts.len()], closed }
}

fn signed_area(p: &[(f64, f64)]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let a = p[i];
            let b = p[(i + 1) % n];
            a.0 * b.1 - a.1 * b.0
        })
        .sum::<f64>()
        * 0.5
}

#[test]
fn stroke_non_positive_width_returns_empty() {
    let pl = polyline(&[(0.0, 0.0), (1.0, 0.0)], false);
    for w in [0.0, -1.0, f64::NAN] {
        let style = StrokeStyle { width: w, ..Default::default() };
        assert!(stroke_polygons(std::slice::from_ref(&pl), &style, 0.1).is_empty());
    }
}

#[test]
fn stroke_infinite_width_returns_empty() {
    let pl = polyline(&[(0.0, 0.0), (1.0, 0.0)], false);
    let style = StrokeStyle { width: f64::INFINITY, ..Default::default() };
    assert!(stroke_polygons(std::slice::from_ref(&pl), &style, 0.1).is_empty());
}

#[test]
fn stroke_single_point_round_cap() {
    let pl = polyline(&[(10.0, 20.0)], false);
    let style = StrokeStyle { width: 2.0, cap: LineCap::Round, ..Default::default() };
    let polys = stroke_polygons(std::slice::from_ref(&pl), &style, 0.1);
    assert_eq!(polys.len(), 1);
    assert!(signed_area(&polys[0]) > 0.0);
    for p in &polys[0] {
        assert!(p.0.is_finite() && p.1.is_finite());
    }
}

#[test]
fn stroke_single_point_square_cap() {
    let pl = polyline(&[(10.0, 20.0)], false);
    let style = StrokeStyle { width: 2.0, cap: LineCap::Square, ..Default::default() };
    let polys = stroke_polygons(std::slice::from_ref(&pl), &style, 0.1);
    assert_eq!(polys.len(), 1);
    let p = &polys[0];
    assert_eq!(p.len(), 4);
    let area = signed_area(p).abs();
    assert!((area - 4.0).abs() < 1e-6);
}

#[test]
fn stroke_single_point_butt_cap_empty() {
    let pl = polyline(&[(10.0, 20.0)], false);
    let style = StrokeStyle { width: 2.0, cap: LineCap::Butt, ..Default::default() };
    let polys = stroke_polygons(std::slice::from_ref(&pl), &style, 0.1);
    assert!(polys.is_empty());
}

#[test]
fn stroke_two_point_line_round_caps() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0)], false);
    let style = StrokeStyle { width: 2.0, cap: LineCap::Round, ..Default::default() };
    let polys = stroke_polygons(std::slice::from_ref(&pl), &style, 0.1);
    assert_eq!(polys.len(), 3);
    for p in &polys {
        assert!(signed_area(p) > 0.0);
    }
}

#[test]
fn stroke_two_point_line_square_caps() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0)], false);
    let style = StrokeStyle { width: 2.0, cap: LineCap::Square, ..Default::default() };
    let polys = stroke_polygons(std::slice::from_ref(&pl), &style, 0.1);
    assert_eq!(polys.len(), 3);
    for p in &polys {
        assert!(signed_area(p) > 0.0);
    }
}

#[test]
fn stroke_two_point_line_butt_caps() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0)], false);
    let style = StrokeStyle { width: 2.0, cap: LineCap::Butt, ..Default::default() };
    let polys = stroke_polygons(std::slice::from_ref(&pl), &style, 0.1);
    assert_eq!(polys.len(), 1);
    assert!(signed_area(&polys[0]) > 0.0);
}

#[test]
fn stroke_polyline_join_types_positive_orientation() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], false);
    for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
        let style = StrokeStyle { width: 2.0, join, ..Default::default() };
        let polys = stroke_polygons(std::slice::from_ref(&pl), &style, 0.1);
        assert!(!polys.is_empty());
        for p in &polys {
            assert!(signed_area(p) > 0.0);
        }
    }
}

#[test]
fn stroke_smooth_join_reduces_polygons() {
    let small = Polyline { pts: vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.1)], knot: vec![true, false, true], closed: false };
    let style = StrokeStyle { width: 1.0, join: LineJoin::Round, cap: LineCap::Butt, ..Default::default() };
    let polys_small = stroke_polygons(&[small], &style, 0.1);

    let large = Polyline { pts: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], knot: vec![true, false, true], closed: false };
    let polys_large = stroke_polygons(&[large], &style, 0.1);

    assert!(polys_small.len() < polys_large.len());
}

#[test]
fn stroke_closed_polyline_produces_join_pieces() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], true);
    let style = StrokeStyle { width: 2.0, cap: LineCap::Butt, join: LineJoin::Bevel, ..Default::default() };
    let polys = stroke_polygons(std::slice::from_ref(&pl), &style, 0.1);
    assert!(!polys.is_empty());
    for p in &polys {
        assert!(signed_area(p) > 0.0);
    }
}

#[test]
fn dash_polyline_splits_by_length() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0)], false);
    let d = dash_polyline(&pl, &[2.0, 1.0], 0.0);
    assert_eq!(d.len(), 4);
    assert_eq!(d[1].pts.len(), 2);
    assert!((d[1].pts[0].0 - 3.0).abs() < 1e-9 && (d[1].pts[1].0 - 5.0).abs() < 1e-9);
    assert_eq!(d[3].pts.len(), 2);
    assert!((d[3].pts[1].0 - 10.0).abs() < 1e-9);

    let o = dash_polyline(&pl, &[2.0, 1.0], 1.0);
    assert_eq!(o[0].pts.len(), 2);
    assert!((o[0].pts[1].0 - 1.0).abs() < 1e-9);
}

#[test]
fn dash_polyline_offset_wraps_negative_and_large() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0)], false);
    let pattern = [2.0, 1.0];
    let d_neg = dash_polyline(&pl, &pattern, -1.0);
    let d_pos = dash_polyline(&pl, &pattern, 2.0);
    assert_eq!(d_neg.len(), d_pos.len());
    for (a, b) in d_neg.iter().zip(d_pos.iter()) {
        assert_eq!(a.pts.len(), b.pts.len());
        for (p, q) in a.pts.iter().zip(b.pts.iter()) {
            assert!((p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9);
        }
    }
}

#[test]
fn dash_polyline_odd_pattern_duplicates() {
    let pl = polyline(&[(0.0, 0.0), (4.0, 0.0)], false);
    let d = dash_polyline(&pl, &[1.0], 0.0);
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].pts.len(), 2);
    assert!((d[0].pts[0].0 - 0.0).abs() < 1e-9);
    assert!((d[0].pts[1].0 - 1.0).abs() < 1e-9);
    assert_eq!(d[1].pts.len(), 2);
    assert!((d[1].pts[0].0 - 2.0).abs() < 1e-9);
    assert!((d[1].pts[1].0 - 3.0).abs() < 1e-9);
}

#[test]
fn dash_polyline_zero_pattern_returns_original() {
    let pl = polyline(&[(0.0, 0.0), (1.0, 1.0)], false);
    let d = dash_polyline(&pl, &[], 0.0);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].pts, pl.pts);
    let d2 = dash_polyline(&pl, &[0.0, 0.0], 0.0);
    assert_eq!(d2.len(), 1);
    assert_eq!(d2[0].pts, pl.pts);
}

#[test]
fn dash_polyline_closed_adds_closing_segment() {
    let pl = polyline(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0)], true);
    let d = dash_polyline(&pl, &[2.0, 2.0], 0.0);
    assert!(!d.is_empty());
}

#[test]
fn stroke_polygons_with_dashes() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0)], false);
    let base = StrokeStyle { width: 2.0, cap: LineCap::Butt, join: LineJoin::Miter, ..Default::default() };
    let solid_style = StrokeStyle { dashes: vec![], ..base.clone() };
    let dashed_style = StrokeStyle { dashes: vec![2.0, 2.0], ..base };
    let solid = stroke_polygons(std::slice::from_ref(&pl), &solid_style, 0.1);
    let dashed = stroke_polygons(std::slice::from_ref(&pl), &dashed_style, 0.1);
    assert!(dashed.len() > solid.len());
}

#[test]
fn round_cap_tesselation_respects_tolerance() {
    let pl = polyline(&[(0.0, 0.0)], false);
    let style = StrokeStyle { width: 2.0, cap: LineCap::Round, ..Default::default() };
    let tol_fine = 0.01;
    let tol_coarse = 0.5;
    let fine = stroke_polygons(std::slice::from_ref(&pl), &style, tol_fine);
    let coarse = stroke_polygons(std::slice::from_ref(&pl), &style, tol_coarse);
    assert_eq!(fine.len(), 1);
    assert_eq!(coarse.len(), 1);
    assert!(fine[0].len() > coarse[0].len());
}

#[test]
fn stroke_output_deterministic_and_finite() {
    let pl = polyline(&[(0.0, 0.0), (5.0, 2.0), (7.0, -3.0)], false);
    let style = StrokeStyle { width: 1.5, cap: LineCap::Round, join: LineJoin::Round, miter_limit: 4.0, dashes: vec![], dash_offset: 0.0 };
    let a = stroke_polygons(std::slice::from_ref(&pl), &style, 0.05);
    let b = stroke_polygons(std::slice::from_ref(&pl), &style, 0.05);
    assert_eq!(a, b);
    for poly in &a {
        for p in poly {
            assert!(p.0.is_finite() && p.1.is_finite());
        }
    }
}

#[test]
fn stroke_no_panic_on_bad_inputs() {
    let empty_lines: &[Polyline] = &[];
    let style = StrokeStyle::default();
    assert!(stroke_polygons(empty_lines, &style, 0.1).is_empty());

    let empty_pl = polyline(&[], false);
    assert!(stroke_polygons(std::slice::from_ref(&empty_pl), &style, 0.1).is_empty());

    let nan_pl = polyline(&[(f64::NAN, 0.0), (1.0, 1.0)], false);
    let _ = stroke_polygons(std::slice::from_ref(&nan_pl), &style, 0.1);
}

#[test]
fn dash_no_panic_on_empty_points() {
    let pl = polyline(&[], false);
    let d = dash_polyline(&pl, &[1.0, 1.0], 0.0);
    assert_eq!(d.len(), 1);
    assert!(d[0].pts.is_empty());
}

#[test]
fn miter_limit_controls_bevel() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], false);
    let style_low = StrokeStyle { width: 2.0, join: LineJoin::Miter, miter_limit: 1.0, ..Default::default() };
    let polys_low = stroke_polygons(std::slice::from_ref(&pl), &style_low, 0.1);
    assert!(polys_low.iter().any(|p| p.len() == 3));

    let style_high = StrokeStyle { width: 2.0, join: LineJoin::Miter, miter_limit: 2.0, ..Default::default() };
    let polys_high = stroke_polygons(std::slice::from_ref(&pl), &style_high, 0.1);
    assert!(!polys_high.iter().any(|p| p.len() == 3));
}

#[test]
fn round_join_produces_arc_polygon() {
    let pl = polyline(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], false);
    let style_round = StrokeStyle { width: 2.0, join: LineJoin::Round, ..Default::default() };
    // Use a fine tolerance to ensure the arc is tessellated into more than 4 vertices.
    let polys = stroke_polygons(std::slice::from_ref(&pl), &style_round, 0.001);
    let has_arc = polys.iter().any(|p| p.len() > 4);
    assert!(has_arc);
}
