//! Synthetic GPL-3.0-or-later interpolation and tangent oracles; no external corpus.
#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}
#[test]
fn anchor33_curvature_interpolates_four_clicks_with_g1_and_a_true_corner() {
    let points = [(p(0., 0.), false), (p(30., 60.), false), (p(90., 10.), false), (p(140., 80.), false)];
    let path = fit(&points, false).unwrap();
    let sp = &path.subpaths[0];
    for (k, (point, _)) in sp.knots.iter().zip(points) {
        assert_eq!(k.anchor, point);
    }
    for pair in sp.segments().windows(2) {
        assert_eq!(pair[0][3], pair[1][0]);
        let incoming = p(pair[0][3].x - pair[0][2].x, pair[0][3].y - pair[0][2].y);
        let outgoing = p(pair[1][1].x - pair[1][0].x, pair[1][1].y - pair[1][0].y);
        assert!((incoming.x * outgoing.y - incoming.y * outgoing.x).abs() < 1e-12);
        assert!(incoming.x * outgoing.x + incoming.y * outgoing.y > 0.);
    }
    let mut corner = points;
    corner[1].1 = true;
    let path = fit(&corner, false).unwrap();
    let sp = &path.subpaths[0];
    let k = sp.knots[1];
    assert!(!k.smooth);
    assert_eq!(k.in_ctrl, k.anchor);
    assert_eq!(k.out_ctrl, k.anchor);
    // A retracted endpoint's first nonzero derivative follows its own segment, not the
    // common smooth tangent. These two incident rays are deliberately non-collinear.
    let segments = sp.segments();
    let a = segments[0][0];
    let b = segments[1][2];
    let cross = (k.anchor.x - a.x) * (b.y - k.anchor.y) - (k.anchor.y - a.y) * (b.x - k.anchor.x);
    assert!(cross.abs() > 100.);
    // Independent polynomial midpoint, with the neighboring click controlling the tangent.
    let seg = fit(&points, false).unwrap().subpaths[0].segments()[1];
    let midpoint = p((seg[0].x + 3. * seg[1].x + 3. * seg[2].x + seg[3].x) / 8., (seg[0].y + 3. * seg[1].y + 3. * seg[2].y + seg[3].y) / 8.);
    assert!((midpoint.x - 58.75).abs() < 1e-12 && (midpoint.y - 34.375).abs() < 1e-12);
}
#[test]
fn anchor33_curvature_closed_seam_is_smooth_and_duplicates_are_retained() {
    let points = [(p(0., 0.), false), (p(60., 0.), false), (p(60., 60.), false), (p(0., 60.), false)];
    let path = fit(&points, true).unwrap();
    let segments = path.subpaths[0].segments();
    assert_eq!(segments.len(), 4);
    assert_eq!(segments[3][3], points[0].0);
    for k in &path.subpaths[0].knots {
        assert!(k.smooth);
        assert_eq!(k.in_ctrl.x + k.out_ctrl.x, 2. * k.anchor.x);
        assert_eq!(k.in_ctrl.y + k.out_ctrl.y, 2. * k.anchor.y);
    }
    let same = [(p(1., 1.), false); 4];
    assert_eq!(fit(&same, false).unwrap().subpaths[0].knots.len(), 4);
    assert!(fit(&same, true).unwrap().subpaths[0].knots.iter().all(|k| !k.smooth));
}
#[test]
fn anchor33_curvature_rejects_nonfinite_oversize_and_overflow_and_handles_short_paths() {
    for bad in [f64::NAN, f64::INFINITY, 1_000_001.] {
        assert!(fit(&[(p(bad, 0.), false)], false).is_err());
    }
    assert!(fit(&vec![(p(1., 1.), false); MAX_POINTS + 1], false).is_err());
    assert!(fit(&[(p(0., 0.), false), (p(1., 1.), false)], true).is_err());
    assert!(fit(&[(p(-1e6, 0.), false), (p(1e6, 0.), false), (p(1e6, 1.), false)], false).is_err());
    assert!(fit(&[], false).unwrap().subpaths.is_empty());
    assert_eq!(fit(&[(p(1., 2.), false)], false).unwrap().subpaths[0].knots[0], Knot::corner(1., 2.));
}
