//! Layer rendering against Photoshop: stroke edges, shadow knockout, pass-through clipping bases
//! and shape bounds.

use super::*;

#[test]
fn a_wide_outside_stroke_is_round_not_a_polygon() {
    // A point source dilated by 60 px must stay a circle. The 5 × 5 chamfer's octagonal level set
    // made the boundary swing ~2.5 px at this radius (~10 px at 245 px).
    let mut d = doc_white(170, 170);
    let mut l = solid_layer("dot", Rect::new(85, 85, 86, 86), [1.0, 0.0, 0.0, 1.0]);
    l.effects.items = vec![stroke(60.0, StrokePosition::Outside)];
    d.layers.push(l);
    let buf = render(&d, Rect::new(0, 0, 170, 170));
    let at = |x: i32, y: i32| buf.px[(y as usize) * 170 + x as usize];
    let (mut lo, mut hi) = (f64::MAX, 0.0f64);
    let stroke_at = |x: i32, y: i32| x >= 0 && y >= 0 && x < 170 && y < 170 && at(x, y)[2] > 0.5 && at(x, y)[0] < 0.5;
    for k in 0..72 {
        let th = (k as f64) * std::f64::consts::TAU / 72.0;
        let (dx, dy) = (th.cos(), th.sin());
        let mut r = 0.0f64;
        let mut t = 0.0f64;
        while t < 84.0 {
            if stroke_at((85.0 + dx * t).round() as i32, (85.0 + dy * t).round() as i32) {
                r = t;
            }
            t += 0.25;
        }
        lo = lo.min(r);
        hi = hi.max(r);
    }
    assert!(hi - lo <= 2.0, "the 60 px stroke boundary swings {lo}..{hi} px — a polygon, not a circle");
}

#[test]
fn the_stroke_metric_switches_at_the_fitted_threshold() {
    // The widest outside stroke any Photoshop oracle matches with the chamfer is 18 px
    // (ag-psd read-write/strokes): its octagon deviates 0.48 px from a circle there; 20 px keeps
    // it. Wider strokes are round.
    assert!(!effects::stroke_outside_exact(18.0));
    assert!(effects::stroke_outside_exact(effects::STROKE_CHAMFER_MAX));
    assert!(effects::stroke_outside_exact(60.0));
}
