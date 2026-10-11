use super::*;
use geometry::{CornerError, CornerKey, evaluate, handles};
use photocraft_doc::vector_design::drawing::{Corner, CornerKind};
use photocraft_doc::{Color, ColorMode, Fill, Knot, Path, SampleType, Size, Subpath};
use photocraft_geom::Point;

pub(super) fn square() -> Path {
    Path::new(vec![Subpath::polygon(&[(8.0, 8.0), (108.0, 8.0), (108.0, 108.0), (8.0, 108.0)])])
}
pub(super) fn metadata(kind: CornerKind, radius: f64, count: usize) -> Vec<KnotCorner> {
    (0..count).map(|knot| KnotCorner { subpath: 0, knot, corner: Corner { radius, kind } }).collect()
}
pub(super) fn session(path: Path) -> (Session, LayerId) {
    let mut s = Session::new();
    let mut d = Document::new("Corners", Size::new(128, 128), ColorMode::Rgb, SampleType::F32);
    let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(Color::rgba(0.2, 0.5, 0.8, 0.7))), ..Default::default() };
    crate::vector_cmds::refresh_shape(&d, &mut sh);
    let l = Layer::new("Path", LayerContent::Shape(sh));
    let id = l.id;
    d.layers.push(l);
    s.add_document(d, None);
    (s, id)
}
/// Independent Green's-theorem oracle: exact polynomial integral of each cubic, not flattened
/// samples nor the implementation's arc formula. Translation improves cancellation at large xy.
fn area(path: &Path) -> f64 {
    let origin = path.subpaths[0].knots[0].anchor;
    path.subpaths
        .iter()
        .flat_map(Subpath::segments)
        .map(|seg| {
            let coefficients = |axis: fn(Point) -> f64| {
                let p: Vec<_> = seg.iter().map(|p| axis(*p) - axis(origin)).collect();
                [p[0], 3.0 * (p[1] - p[0]), 3.0 * (p[2] - 2.0 * p[1] + p[0]), p[3] - 3.0 * p[2] + 3.0 * p[1] - p[0]]
            };
            let x = coefficients(|p| p.x);
            let y = coefficients(|p| p.y);
            let mut integral = 0.0;
            for i in 0..4 {
                for j in 1..4 {
                    integral += (x[i] * y[j] - y[i] * x[j]) * j as f64 / (i + j) as f64;
                }
            }
            integral * 0.5
        })
        .sum::<f64>()
        .abs()
}
/// Different oracle: dense parameter sampling and proper segment intersections; exercises curves
/// without using the evaluator's adaptive tables or safety predicate.
fn assert_simple(path: &Path) {
    for sp in &path.subpaths {
        let mut p = Vec::new();
        for [a, b, c, d] in sp.segments() {
            for i in 0..32 {
                let t = i as f64 / 32.0;
                let u = 1.0 - t;
                p.push(Point::new(
                    u * u * u * a.x + 3.0 * u * u * t * b.x + 3.0 * u * t * t * c.x + t * t * t * d.x,
                    u * u * u * a.y + 3.0 * u * u * t * b.y + 3.0 * u * t * t * c.y + t * t * t * d.y,
                ));
            }
        }
        if !sp.closed {
            p.push(sp.knots.last().unwrap().anchor);
        }
        let n = if sp.closed { p.len() } else { p.len() - 1 };
        let side = |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        for i in 0..n {
            for j in i + 2..n {
                if sp.closed && i == 0 && j + 1 == n {
                    continue;
                }
                let (a, b, c, d) = (p[i], p[(i + 1) % p.len()], p[j], p[(j + 1) % p.len()]);
                assert!(!(side(a, b, c) * side(a, b, d) < -1e-16 && side(c, d, a) * side(c, d, b) < -1e-16), "crossing edges {i} {j}");
            }
        }
    }
}
#[test]
fn corners_square_area_monotone_clamped_and_kinds() {
    for kind in [CornerKind::Round, CornerKind::Chamfer, CornerKind::Inverted, CornerKind::Straight] {
        let mut previous = 10000.0;
        for r in [0.0, 1.0, 10.0, 20.0, 49.0, 50.0, 80.0, 1e6] {
            let projected = evaluate(&square(), &metadata(kind, r, 4)).unwrap();
            let rr = r.min(50.0);
            let expected = match kind {
                CornerKind::Round => 10000.0 - (4.0 - std::f64::consts::PI) * rr * rr,
                CornerKind::Chamfer => 10000.0 - 2.0 * rr * rr,
                CornerKind::Inverted => 10000.0 - std::f64::consts::PI * rr * rr,
                CornerKind::Straight => 10000.0,
            };
            let actual = area(&projected.path);
            assert!((actual - expected).abs() <= 2e-8 * rr * rr + 1e-8, "{kind:?} r={r} expected={expected} actual={actual}");
            assert!(actual <= previous + 1e-7);
            previous = actual;
            assert!(projected.corners.iter().all(|c| c.corner.radius == rr));
            assert_simple(&projected.path);
        }
    }
}
#[test]
fn corners_curved_adjacency_preserves_untouched_cubics_and_is_simple() {
    let mut p = square();
    p.subpaths[0].knots[0].out_ctrl = Point::new(35.0, 28.0);
    p.subpaths[0].knots[1].in_ctrl = Point::new(85.0, 28.0);
    p.subpaths[0].knots[1].out_ctrl = Point::new(88.0, 35.0);
    p.subpaths[0].knots[2].in_ctrl = Point::new(88.0, 85.0);
    let base = p.clone();
    for kind in [CornerKind::Round, CornerKind::Inverted, CornerKind::Chamfer] {
        for r in [1.0, 10.0, 30.0, 1000.0] {
            let out = evaluate(&p, &metadata(kind, r, 4)).unwrap();
            assert_simple(&out.path);
            assert_eq!(p, base);
            assert!(out.corners.iter().all(|c| c.corner.radius <= r));
        }
    }
    let zero = evaluate(&p, &metadata(CornerKind::Round, 0.0, 4)).unwrap();
    assert_eq!(zero.path, p);
    let one = evaluate(&p, &metadata(CornerKind::Round, 5.0, 1)).unwrap();
    assert!(one.path.subpaths[0].segments().contains(&p.subpaths[0].segments()[1]));
}
#[test]
fn corners_validation_topology_sources_and_open_endpoints() {
    let p = square();
    let hs = handles(&p).unwrap();
    assert_eq!(hs.len(), 4);
    assert_eq!(hs[0].key, CornerKey { subpath: 0, knot: 0 });
    assert!((hs[0].max_radius - 50.0).abs() < 1e-9);
    let mut open = p.clone();
    open.subpaths[0].closed = false;
    assert_eq!(handles(&open).unwrap().len(), 2);
    assert_eq!(evaluate(&open, &metadata(CornerKind::Round, 1.0, 1)), Err(CornerError::NotCorner));
    let mut smooth = p.clone();
    smooth.subpaths[0].knots[1].smooth = true;
    assert_eq!(handles(&smooth).unwrap().len(), 3);
    let mut m = metadata(CornerKind::Round, 2.0, 1);
    m.push(m[0]);
    assert_eq!(evaluate(&p, &m), Err(CornerError::Duplicate));
    for r in [f64::NAN, f64::INFINITY] {
        assert_eq!(evaluate(&p, &metadata(CornerKind::Round, r, 1)), Err(CornerError::NonFinite));
    }
    assert_eq!(evaluate(&p, &metadata(CornerKind::Round, 1e7, 1)), Err(CornerError::Limit));
    let mut m = metadata(CornerKind::Round, 2.0, 1);
    m[0].subpath = 9;
    assert_eq!(evaluate(&p, &m), Err(CornerError::InvalidIndex));
    let bow = Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (100.0, 100.0), (0.0, 100.0), (100.0, 0.0)])]);
    assert_eq!(evaluate(&bow, &metadata(CornerKind::Round, 5.0, 4)), Err(CornerError::SelfIntersection));
    let mut compound = p.clone();
    compound.subpaths.push(Subpath::polygon(&[(30.0, 30.0), (30.0, 70.0), (70.0, 70.0), (70.0, 30.0)]).with_op(photocraft_doc::PathOp::Join));
    compound.fill_rule = photocraft_doc::FillRule::EvenOdd;
    compound.inverted = true;
    let projected = evaluate(&compound, &metadata(CornerKind::Round, 10.0, 4)).unwrap();
    assert_eq!(projected.path.subpaths[1], compound.subpaths[1]);
    assert_eq!(projected.path.fill_rule, compound.fill_rule);
    assert!(projected.path.inverted);
    for (sp, from) in projected.path.subpaths.iter().zip(projected.sources) {
        assert_eq!(sp.knots.len(), from.len());
        assert!(from.iter().all(|k| compound.subpaths[k.subpath].knots.get(k.knot).is_some()));
    }
    let point = Path::new(vec![Subpath { knots: vec![Knot::corner(0.0, 0.0)], ..Default::default() }]);
    assert!(handles(&point).unwrap().is_empty());
    let collinear =
        Path::new(vec![Subpath { closed: false, knots: vec![Knot::corner(0.0, 0.0), Knot::corner(20.0, 0.0), Knot::corner(40.0, 0.0)], ..Default::default() }]);
    assert!(handles(&collinear).unwrap().is_empty());
    let folded = Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (80.0, 0.0), (20.0, 0.0), (20.0, 80.0), (0.0, 80.0)])]);
    let corner = KnotCorner { subpath: 0, knot: 3, corner: Corner { radius: 5.0, kind: CornerKind::Round } };
    assert_eq!(evaluate(&folded, &[corner]), Err(CornerError::SelfIntersection));
}
#[test]
fn corners_preview_cancel_commit_save_load_and_single_undo() {
    let (mut s, id) = session(square());
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    let mut gesture = CornerGesture::begin(&s, &[id]).unwrap();
    gesture.set(id, metadata(CornerKind::Round, 10.0, 4)).unwrap();
    let preview = gesture.preview_document().unwrap();
    assert_ne!(preview, before.as_ref().clone());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    let params = gesture.params();
    s.execute("path.corner.preview", params).unwrap();
    assert!(s.journal.is_empty());
    assert_eq!(s.active().unwrap().revision, rev);
    drop(gesture.clone());
    assert_eq!(s.active().unwrap().doc, before);
    gesture.commit(&mut s).unwrap();
    let after = s.active().unwrap().doc.clone();
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().revision, rev + 1);
    let LayerContent::Shape(shape) = &after.layer(id).unwrap().content else { panic!("shape") };
    assert_eq!(shape.path, square());
    let cache = shape.cache.as_ref().unwrap();
    assert!(photocraft_geom::Rect::new(7, 7, 109, 109).contains_rect(&cache.content_bounds()));
    assert_eq!(cache.tile_count(), 1);
    let saved = photocraft_format::save_to_bytes(&after, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&saved).unwrap();
    let l = loaded.layers.last().unwrap();
    assert_eq!(l.vector.corners, metadata(CornerKind::Round, 10.0, 4));
    let projected = project_document(&loaded).unwrap();
    assert_eq!(projected.layers.last().unwrap().content, preview.layers.last().unwrap().content);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, after);
}
#[test]
fn corners_atomic_target_set_and_stale_document_revision() {
    let (mut s, id) = session(square());
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    let g = CornerGesture::begin(&s, &[id]).unwrap();
    let mut p = g.params();
    p["edits"][0]["corners"] = json!(metadata(CornerKind::Round, 5.0, 4));
    for invalid in [json!({"layer":9999999999u64,"corners":[]}), json!({"layer":id.0,"corners":[]})] {
        let mut bad = p.clone();
        bad["edits"].as_array_mut().unwrap().push(invalid);
        assert!(s.execute("path.corner.set", bad).is_err());
        assert_eq!(s.active().unwrap().doc, before);
        assert!(s.journal.is_empty());
        assert!(!s.undo());
    }
    s.active_mut().unwrap().revision += 1;
    assert!(!g.is_current(&s));
    assert!(g.clone().commit(&mut s).is_err());
    s.active_mut().unwrap().revision = rev;
    let (mut other, _) = session(square());
    assert!(g.commit(&mut other).is_err());
    let mut locked = (*before).clone();
    locked.layers[0].locks.pixels = true;
    s.active_mut().unwrap().doc = Arc::new(locked);
    assert!(s.execute("path.corner.set", p).is_err());
    assert!(s.journal.is_empty());
    assert!(!s.undo());
}

#[test]
fn corners_seeded_256_rectangles_against_analytic_area_and_monotone_curve_radii() {
    let mut seed = 0x06c0_1234u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 32) as u32) as f64 / u32::MAX as f64
    };
    for _ in 0..256 {
        let w = 20.0 + next() * 180.0;
        let h = 20.0 + next() * 180.0;
        let r = next() * w.min(h);
        let mut p = Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)])]);
        let angle = next() * std::f64::consts::TAU;
        let (sin, cos) = angle.sin_cos();
        p = p.transform(&photocraft_geom::Affine { m: [cos, sin, -sin, cos, -200.0 + next() * 400.0, -200.0 + next() * 400.0] });
        let rr = r.min(w.min(h) * 0.5);
        let result = evaluate(&p, &metadata(CornerKind::Round, r, 4)).unwrap();
        let expected = w * h - (4.0 - std::f64::consts::PI) * rr * rr;
        assert!((area(&result.path) - expected).abs() < 2e-8 * rr * rr + 1e-8);
        assert_eq!(result, evaluate(&p, &metadata(CornerKind::Round, r, 4)).unwrap());
    }
    let mut curve = square();
    curve.subpaths[0].knots[0].out_ctrl = Point::new(40.0, 35.0);
    curve.subpaths[0].knots[1].in_ctrl = Point::new(80.0, 35.0);
    for kind in [CornerKind::Round, CornerKind::Inverted, CornerKind::Chamfer] {
        let mut last = vec![0.0; 4];
        for radius in [1.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 50.0, 100.0, 1000.0] {
            let result = evaluate(&curve, &metadata(kind, radius, 4)).unwrap();
            for (c, previous) in result.corners.iter().zip(&mut last) {
                assert!(c.corner.radius + 1e-9 >= *previous);
                *previous = c.corner.radius;
            }
        }
    }
}
#[test]
fn corners_legacy_document_without_metadata_still_loads() {
    let doc = photocraft_format::load_from_bytes(include_bytes!("../../../../pc-format/tests/fixtures/vector-v1.pcraft")).unwrap();
    assert!(doc.layers.iter().all(|l| l.vector.corners.is_empty()));
    assert_eq!(project_document(&doc).unwrap(), doc);
}
#[test]
fn corners_node_remapping_keeps_base_indices_and_removes_deleted_knots() {
    let corners = metadata(CornerKind::Round, 10.0, 4);
    let map = vec![vec![Some(CornerKey { subpath: 1, knot: 2 }), None, Some(CornerKey { subpath: 0, knot: 0 }), Some(CornerKey { subpath: 1, knot: 0 })]];
    let remapped = geometry::remap_metadata(&corners, &map).unwrap();
    assert_eq!(remapped.iter().map(|c| (c.subpath, c.knot)).collect::<Vec<_>>(), [(0, 0), (1, 0), (1, 2)]);
    assert!(remapped.iter().all(|c| c.corner == corners[0].corner));
    assert_eq!(geometry::remap_metadata(&corners, &[]), Err(CornerError::InvalidIndex));
    let duplicate = vec![vec![Some(CornerKey { subpath: 0, knot: 0 }); 4]];
    assert_eq!(geometry::remap_metadata(&corners, &duplicate), Err(CornerError::Duplicate));
}
#[test]
fn corners_curved_round_is_circular_and_tangent_to_both_trimmed_edges() {
    let mut p = square();
    p.subpaths[0].knots[0].out_ctrl = Point::new(35.0, 25.0);
    p.subpaths[0].knots[1].in_ctrl = Point::new(85.0, 25.0);
    p.subpaths[0].knots[1].out_ctrl = Point::new(92.0, 35.0);
    p.subpaths[0].knots[2].in_ctrl = Point::new(92.0, 85.0);
    for kind in [CornerKind::Round, CornerKind::Inverted] {
        let target = KnotCorner { subpath: 0, knot: 1, corner: Corner { radius: 10.0, kind } };
        let out = evaluate(&p, &[target]).unwrap();
        let segs = out.path.subpaths[0].segments();
        let from = &out.sources[0];
        let indices: Vec<_> = (0..segs.len()).filter(|i| from[*i] == CornerKey { subpath: 0, knot: 1 } && from[(*i + 1) % from.len()] == from[*i]).collect();
        assert!(indices.len() > 1);
        let first = segs[indices[0]];
        let last = segs[*indices.last().unwrap()];
        let (center, radius) = if kind == CornerKind::Round {
            let v = Point::new(first[1].x - first[0].x, first[1].y - first[0].y);
            let len = v.x.hypot(v.y);
            let radius = out.corners[0].corner.radius;
            (Point::new(first[0].x - v.y / len * radius, first[0].y + v.x / len * radius), radius)
        } else {
            let center = p.subpaths[0].knots[1].anchor;
            (center, (first[0].x - center.x).hypot(first[0].y - center.y))
        };
        for i in &indices {
            let [a, b, c, d] = segs[*i];
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let u = 1.0 - t;
                let x = u * u * u * a.x + 3.0 * u * u * t * b.x + 3.0 * u * t * t * c.x + t * t * t * d.x;
                let y = u * u * u * a.y + 3.0 * u * u * t * b.y + 3.0 * u * t * t * c.y + t * t * t * d.y;
                assert!(((x - center.x).hypot(y - center.y) - radius).abs() < radius * 5e-8 + 1e-7);
            }
        }
        if kind == CornerKind::Round {
            let prev = segs[indices[0] - 1];
            let next = segs[indices.last().unwrap() + 1];
            let parallel = |a: Point, b: Point, c: Point, d: Point| {
                let x = Point::new(b.x - a.x, b.y - a.y);
                let y = Point::new(d.x - c.x, d.y - c.y);
                assert!((x.x * y.y - x.y * y.x).abs() / (x.x.hypot(x.y) * y.x.hypot(y.y)) < 1e-6);
            };
            parallel(prev[2], prev[3], first[0], first[1]);
            parallel(last[2], last[3], next[0], next[1]);
        }
    }
}

#[test]
fn corners_caches_allocate_only_shape_tiles_on_a_6000x4000_document() {
    let (mut s, id) = session(square());
    let mut large = (*s.active().unwrap().doc).clone();
    large.size = Size::new(6000, 4000);
    s.active_mut().unwrap().doc = Arc::new(large);
    let mut g = CornerGesture::begin(&s, &[id]).unwrap();
    g.set(id, metadata(CornerKind::Round, 20.0, 4)).unwrap();
    g.commit(&mut s).unwrap();
    let LayerContent::Shape(shape) = &s.active().unwrap().doc.layer(id).unwrap().content else { panic!("shape") };
    let cache = shape.cache.as_ref().unwrap();
    assert_eq!(cache.tile_count(), 1);
    assert!(cache.content_bounds().size().area() <= 10404);
    assert_eq!(cache.sample_channel(5000, 3000, 3), 0.0);
    assert!(cache.sample_channel(64, 64, 3) > 0.69);
}
