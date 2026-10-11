//! Original CC0 synthetic geometry corpus; independently checked against native coverage.
//! See SOURCES.md. External Inkscape goldens remain coordinator-owned (quality plan A.1).
use photocraft_doc::{FillRule, Path, PathOp, Subpath};
use photocraft_geom::{Affine, Rect};
use photocraft_pathops as ops;
use photocraft_testkit::vector as metrics;
use photocraft_vector as vector;

#[test]
fn tricky_native_corpus_and_shrinking_ratchet() {
    let square = vector::shapes::rect(8.0, 8.0, 32.0, 32.0);
    let ellipse = vector::shapes::ellipse(8.0, 8.0, 32.0, 32.0);
    let bow = Path::new(vec![Subpath::polygon(&[(8.0, 8.0), (40.0, 40.0), (8.0, 40.0), (40.0, 8.0)])]);
    let mut hole = square.clone();
    hole.subpaths.extend(vector::shapes::rect(16.0, 16.0, 16.0, 16.0).subpaths.into_iter().map(|s| s.with_op(PathOp::Join)));
    hole.fill_rule = FillRule::EvenOdd;
    let mut reversed_hole = hole.clone();
    reversed_hole.fill_rule = FillRule::NonZero;
    reversed_hole.subpaths[1].knots.reverse();
    let spike = Path::new(vec![Subpath::polygon(&[(8.0, 8.0), (40.0, 8.0), (40.0, 24.0), (48.0, 24.0), (40.0, 24.0), (40.0, 40.0), (8.0, 40.0)])]);
    let star = vector::shapes::polygon(4.0, 4.0, 40.0, 40.0, 5, 0.4);
    let mut touching_hole = vector::shapes::rect(8.0, 8.0, 32.0, 32.0);
    let mut inner = vector::shapes::rect(8.0, 16.0, 16.0, 16.0).subpaths.remove(0);
    inner.knots.reverse();
    inner.op = PathOp::Join;
    touching_hole.subpaths.push(inner);
    let cases = [
        ("fifty-spike-star", vector::shapes::polygon(4.0, 4.0, 40.0, 40.0, 50, 0.2), ellipse.clone()),
        ("hole-touching-outer", touching_hole, ellipse.clone()),
        ("shared-edge", square.clone(), vector::shapes::rect(40.0, 8.0, 16.0, 32.0)),
        ("corner-touch", square.clone(), vector::shapes::rect(40.0, 40.0, 16.0, 16.0)),
        ("tangent-circle", ellipse.clone(), vector::shapes::ellipse(40.0, 8.0, 16.0, 32.0)),
        ("coincident", ellipse.clone(), ellipse.clone()),
        ("coincident-reversed", ellipse.clone(), ops::reverse(&ellipse).unwrap()),
        ("evenodd-bow", Path { fill_rule: FillRule::EvenOdd, ..bow.clone() }, ellipse.clone()),
        ("nonzero-bow", bow, ellipse.clone()),
        ("evenodd-hole", hole, ellipse.clone()),
        ("nonzero-hole", reversed_hole, ellipse.clone()),
        ("zero-area-spike", spike, ellipse.clone()),
        ("star", star, ellipse.clone()),
        ("empty", Path::default(), ellipse.clone()),
        ("contained", square, vector::shapes::ellipse(16.0, 16.0, 16.0, 16.0)),
        ("open-fill", Path::new(vec![Subpath::polyline(&[(8.0, 8.0), (40.0, 8.0), (40.0, 40.0)])]), ellipse),
    ];
    let ratchet: Vec<serde_json::Value> = serde_json::from_str(include_str!("ratchet/geometry.json")).unwrap();
    let mut failed = Vec::new();
    for (name, a, mut b) in cases {
        b.fill_rule = a.fill_rule; // Simple B contours have identical evenodd/nonzero fills.
        for (op, component) in [
            (ops::BoolOp::Union, PathOp::Combine),
            (ops::BoolOp::Difference, PathOp::Subtract),
            (ops::BoolOp::Intersect, PathOp::Intersect),
            (ops::BoolOp::Xor, PathOp::Exclude),
        ] {
            let mut oracle = a.clone();
            if oracle.subpaths.is_empty() {
                oracle.subpaths.push(Subpath::default());
            }
            oracle.subpaths.extend(b.subpaths.iter().enumerate().map(|(i, s)| s.clone().with_op(if i == 0 { component } else { PathOp::Join })));
            let expected = vector::fill_rasterizer(&oracle.transform(&Affine::scale(4.0)), 0.001).render(Rect::new(0, 0, 256, 256));
            let result = ops::boolean(&a, &b, op);
            let pass = result.as_ref().is_ok_and(|p| {
                metrics::is_finite(p) && metrics::self_intersections(p).is_empty() && {
                    let actual = vector::fill_rasterizer(&p.transform(&Affine::scale(4.0)), 0.001).render(Rect::new(0, 0, 256, 256));
                    metrics::max_abs_diff(&expected, &actual).is_ok_and(|d| d <= 2.0 / 255.0)
                }
            });
            let key = format!("{name}/{op:?}");
            let listed = ratchet.iter().any(|v| v["case"] == key && v["reason"].as_str().is_some_and(|r| !r.is_empty()));
            assert!(!(pass && listed), "passing case still ratcheted: {key}");
            if !pass {
                failed.push(key.clone());
                assert!(listed, "new geometry failure: {key}; {result:?}");
            }
        }
    }
    assert!(failed.len() * 100 <= 64 * 5, "less than 95% pass");
    assert_eq!(failed.len(), ratchet.len(), "unknown or duplicate ratchet entries");
}
