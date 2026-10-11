#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Fill, SampleType, Size};
use photocraft_geom::{Affine, Rect};
use photocraft_vector as vector;

fn fixture(paths: Vec<Path>) -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Constructive", Size::new(160, 160), ColorMode::Rgb, SampleType::F32);
    let mut ids = Vec::new();
    for (i, path) in paths.into_iter().enumerate() {
        let mut shape = ShapeLayer {
            path,
            fill: Some(Fill::Solid(if i == 0 { Color::rgba(0.8, 0.1, 0.2, 0.7) } else { Color::rgba(0.1, 0.2, 0.8, 0.9) })),
            ..Default::default()
        };
        crate::vector_cmds::refresh_shape(&doc, &mut shape);
        let layer = Layer::new(format!("Source {i}"), LayerContent::Shape(shape));
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    let mut session = Session::new();
    session.add_document(doc, None);
    (session, ids)
}
fn rectangle() -> Path {
    vector::shapes::rect(20., 20., 100., 100.)
}
fn circles() -> Vec<Path> {
    vec![vector::shapes::ellipse(16., 24., 80., 80.), vector::shapes::ellipse(56., 24., 80., 80.)]
}
fn path(doc: &Document, id: LayerId) -> &Path {
    let LayerContent::Shape(sh) = &doc.layer(id).unwrap().content else { panic!("not shape") };
    &sh.path
}
/// Independent exact polynomial Green integral, no geometry-kernel area/normalization.
fn area(path: &Path) -> f64 {
    path.subpaths
        .iter()
        .flat_map(Subpath::segments)
        .map(|s| {
            let coeff = |axis: fn(Point) -> f64| {
                let p = s.map(axis);
                [p[0], 3. * (p[1] - p[0]), 3. * (p[2] - 2. * p[1] + p[0]), p[3] - 3. * p[2] + 3. * p[1] - p[0]]
            };
            let (x, y) = (coeff(|p| p.x), coeff(|p| p.y));
            let mut a = 0.;
            for i in 0..4 {
                for j in 1..4 {
                    a += (x[i] * y[j] - y[i] * x[j]) * j as f64 / (i + j) as f64;
                }
            }
            a / 2.
        })
        .sum::<f64>()
        .abs()
}
fn parity(a: &Path, b: &Path) {
    let rect = Rect::new(0, 0, 640, 640);
    let xf = Affine::scale(4.);
    let a = vector::fill_rasterizer(&a.transform(&xf), 0.01).render(rect);
    let b = vector::fill_rasterizer(&b.transform(&xf), 0.01).render(rect);
    let max = a.iter().zip(b).map(|(a, b)| (a - b).abs()).fold(0., f32::max);
    assert!(max <= 2. / 255., "coverage oracle max={max}");
}
fn topology(path: &Path) {
    validate_path(path).unwrap();
    for sp in &path.subpaths {
        let p = vector::flatten_subpath(sp, 0.01).pts;
        let n = p.len().saturating_sub(usize::from(!sp.closed));
        let cross = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        for i in 0..n {
            for j in i + 2..n {
                if sp.closed && i == 0 && j + 1 == n {
                    continue;
                }
                let (a, b, c, d) = (p[i], p[(i + 1) % p.len()], p[j], p[(j + 1) % p.len()]);
                assert!(!(cross(a, b, c) * cross(a, b, d) < -1e-12 && cross(c, d, a) * cross(c, d, b) < -1e-12), "crossing {i}/{j}");
            }
        }
    }
}
#[test]
fn constructive_cpu_builder_three_faces_partial_merge_and_union_coverage() {
    let input = circles();
    let (mut s, ids) = fixture(input.clone());
    let g = Gesture::begin(&s, &ids.iter().rev().copied().collect::<Vec<_>>()).unwrap();
    assert_eq!(g.ids(), ids);
    let map = g.arrangement().unwrap();
    assert_eq!(map.regions.len(), 3);
    let left = g.face_at(Point::new(32., 64.)).unwrap().unwrap();
    let middle = g.face_at(Point::new(76., 64.)).unwrap().unwrap();
    let prepared = g.preview(&Action::Builder { faces: vec![left, middle], remove: false, style: ids[0] }).unwrap();
    parity(prepared.output_paths()[0], &input[0]);
    let action = Action::Builder { faces: (0..3).collect(), remove: false, style: ids[1] };
    let prepared = g.preview(&action).unwrap();
    let mut oracle = input[0].clone();
    oracle.subpaths.extend(input[1].subpaths.clone());
    parity(prepared.output_paths()[0], &oracle);
    assert!((map.regions.iter().map(|r| area(&r.path)).sum::<f64>() - area(prepared.output_paths()[0])).abs() < 0.1);
    let radius: f64 = 40.;
    let separation: f64 = 40.;
    let lens = 2. * radius * radius * (separation / (2. * radius)).acos() - 0.5 * separation * (4. * radius * radius - separation * separation).sqrt();
    assert!((area(prepared.output_paths()[0]) - (2. * std::f64::consts::PI * radius * radius - lens)).abs() < 3.);
    let before = s.active().unwrap().doc.clone();
    g.commit(&mut s, action).unwrap();
    assert_eq!(s.active().unwrap().doc.layers.len(), 1);
    assert_eq!(s.active().unwrap().doc.layers[0].id, ids[1]);
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layers[0].content else { panic!() };
    let LayerContent::Shape(expected) = &before.layer(ids[1]).unwrap().content else { panic!() };
    assert_eq!(sh.fill, expected.fill);
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
}
#[test]
fn constructive_cpu_builder_remove_face_and_entire_selection() {
    let input = circles();
    let (mut s, ids) = fixture(input.clone());
    let g = Gesture::begin(&s, &ids).unwrap();
    let middle = g.face_at(Point::new(76., 64.)).unwrap().unwrap();
    let prepared = g.preview(&Action::Builder { faces: vec![middle], remove: true, style: ids[0] }).unwrap();
    assert_eq!(prepared.output_paths().len(), 2);
    let mut actual = Path::default();
    for p in prepared.output_paths() {
        actual.subpaths.extend(p.subpaths.clone());
    }
    let mut oracle = input[0].clone();
    oracle.subpaths.extend(input[1].subpaths.clone().into_iter().map(|p| p.with_op(photocraft_doc::PathOp::Exclude)));
    parity(&actual, &oracle);
    g.commit(&mut s, Action::Builder { faces: (0..3).collect(), remove: true, style: ids[0] }).unwrap();
    assert!(s.active().unwrap().doc.layers.is_empty());
    assert!(s.active().unwrap().selected_layers.is_empty());
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
}
#[test]
fn constructive_cpu_knife_area_through_nodes_tangent_and_partial() {
    for (stroke, count) in
        [(vec![[70., 0.], [70., 140.]], 2), (vec![[0., 0.], [140., 140.]], 2), (vec![[0., 20.], [140., 20.]], 0), (vec![[70., 0.], [70., 60.]], 0)]
    {
        let (mut s, ids) = fixture(vec![rectangle()]);
        let before = s.active().unwrap().doc.clone();
        let g = Gesture::begin(&s, &ids).unwrap();
        let action = Action::Knife { points: stroke };
        let result = g.preview(&action).unwrap();
        assert_eq!(result.output_paths().len(), count);
        if count > 0 {
            assert!((result.output_paths().iter().map(|p| area(p)).sum::<f64>() - 10000.).abs() < 0.01);
            let mut union = Path::default();
            for p in result.output_paths() {
                assert!(p.subpaths.iter().all(|s| s.closed));
                topology(p);
                union.subpaths.extend(p.subpaths.clone());
            }
            parity(&union, &rectangle());
        }
        g.commit(&mut s, action).unwrap();
        if count == 0 {
            assert_eq!(s.active().unwrap().doc, before);
            assert!(s.journal.is_empty());
            assert!(!s.undo());
        } else {
            assert_eq!(s.journal.len(), 1);
            assert!(s.undo());
            assert_eq!(s.active().unwrap().doc, before);
            assert!(!s.undo());
        }
    }
}
#[test]
fn constructive_cpu_line_cut_extends_short_axis_and_keeps_holes() {
    let mut ring = rectangle();
    ring.subpaths.extend(vector::shapes::rect(45., 45., 50., 50.).subpaths.into_iter().map(|p| p.with_op(photocraft_doc::PathOp::Subtract)));
    let (s, ids) = fixture(vec![ring.clone()]);
    let g = Gesture::begin(&s, &ids).unwrap();
    let result = g.preview(&Action::LineCut { from: [70., 60.], to: [70., 80.] }).unwrap();
    assert_eq!(result.output_paths().len(), 2);
    assert!((result.output_paths().iter().map(|p| area(p)).sum::<f64>() - 7500.).abs() < 0.01);
    let mut union = Path::default();
    for p in result.output_paths() {
        union.subpaths.extend(p.subpaths.clone());
    }
    parity(&union, &ring);
}
#[test]
fn constructive_cpu_scissors_preserves_cubic_length_and_unrelated_contours() {
    let mut curve = Path::new(vec![Subpath::polyline(&[(10., 80.), (140., 80.)])]);
    curve.subpaths[0].knots[0].out_ctrl = Point::new(30., 10.);
    curve.subpaths[0].knots[1].in_ctrl = Point::new(120., 10.);
    let original = curve.clone();
    curve.subpaths.extend(vector::shapes::rect(20., 110., 30., 30.).subpaths);
    let (mut s, ids) = fixture(vec![curve]);
    let g = Gesture::begin(&s, &ids).unwrap();
    let action = Action::Scissors { layer: ids[0], subpath: 0, segment: 0, t: 0.5 };
    let result = g.preview(&action).unwrap();
    assert_eq!(result.output_paths().len(), 2);
    let a = &result.output_paths()[0].subpaths[0];
    let b = &result.output_paths()[1].subpaths[0];
    assert_eq!(a.knots.last().unwrap().anchor, b.knots[0].anchor);
    let len = |sp: Subpath| ops::to_geometry(&Path::new(vec![sp])).length();
    assert!((len(a.clone()) + len(b.clone()) - len(original.subpaths[0].clone())).abs() < 1e-4);
    assert_eq!(result.output_paths()[0].subpaths.len(), 2);
    g.commit(&mut s, action).unwrap();
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
}
#[test]
fn constructive_cpu_contour_round_distance_oracle_and_inset_vanish() {
    let source = vector::shapes::rect(40., 40., 60., 60.);
    let (s, ids) = fixture(vec![source]);
    let g = Gesture::begin(&s, &ids).unwrap();
    for delta in [-8., 8.] {
        let result = g.preview(&Action::Contour { delta, join: JoinKind::Round, miter: 4. }).unwrap();
        let p = result.output_paths()[0];
        topology(p);
        let expected = if delta > 0. { 3600. + 240. * delta + std::f64::consts::PI * delta * delta } else { (60. + 2. * delta).powi(2) };
        assert!((area(p) - expected).abs() < 0.2);
        // Exact Euclidean distance to the analytic rectangle is an independent EDT limit.
        // 8x centre samples avoid the rasterizer's antialias threshold and agree off the boundary.
        let coverage = vector::fill_rasterizer(&p.transform(&Affine::scale(8.)), 0.01).render(Rect::new(0, 0, 1280, 1280));
        let mut compared = 0;
        for y in 240..880 {
            for x in 240..880 {
                let q = Point::new((x as f64 + 0.5) / 8., (y as f64 + 0.5) / 8.);
                let dx = (40. - q.x).max(q.x - 100.).max(0.);
                let dy = (40. - q.y).max(q.y - 100.).max(0.);
                let signed = if dx > 0. || dy > 0. {
                    dx.hypot(dy)
                } else {
                    -[(q.x - 40.), (100. - q.x), (q.y - 40.), (100. - q.y)].into_iter().fold(f64::INFINITY, f64::min)
                };
                if (signed - delta).abs() > 0.05 {
                    assert_eq!(coverage[y * 1280 + x] > 0.5, signed < delta, "{q:?} delta={delta}");
                    compared += 1;
                }
            }
        }
        assert!(compared > 400_000);
    }
    let vanished = g.preview(&Action::Contour { delta: -40., join: JoinKind::Round, miter: 4. }).unwrap();
    assert!(vanished.output_paths().is_empty());
    assert!(vanished.document().unwrap().layers.is_empty());
}
#[test]
fn constructive_cpu_smooth_local_topology_and_open_endpoints() {
    let path = Path::new(vec![Subpath::polyline(&[(10., 70.), (40., 75.), (70., 65.), (100., 75.), (140., 70.)])]);
    let (s, ids) = fixture(vec![path.clone()]);
    let g = Gesture::begin(&s, &ids).unwrap();
    let result = g.preview(&Action::Smooth { points: vec![[65., 55.], [75., 85.]], radius: 10., amount: 0.7 }).unwrap();
    let p = result.output_paths()[0];
    topology(p);
    assert_eq!(p.subpaths[0].knots[0], path.subpaths[0].knots[0]);
    assert_eq!(p.subpaths[0].knots[4], path.subpaths[0].knots[4]);
    assert_eq!(p.subpaths[0].knots[1], path.subpaths[0].knots[1]);
    assert_ne!(p.subpaths[0].knots[2], path.subpaths[0].knots[2]);
}
#[test]
fn constructive_cpu_eraser_narrow_interval_and_closed_outline() {
    for closed in [false, true] {
        let source = if closed { rectangle() } else { Path::new(vec![Subpath::polyline(&[(10., 70.), (140., 70.)])]) };
        let (mut s, ids) = fixture(vec![source]);
        let g = Gesture::begin(&s, &ids).unwrap();
        let action = Action::Erase { points: if closed { vec![[64.23, 0.], [64.23, 40.]] } else { vec![[64.23, 50.], [64.23, 90.]] }, width: 0.5 };
        let result = g.preview(&action).unwrap();
        assert!(result.changed());
        assert_eq!(result.output_paths().len(), if closed { 1 } else { 2 });
        for p in result.output_paths() {
            topology(p);
            assert!(p.subpaths.iter().all(|s| !s.closed));
            for sp in &p.subpaths {
                for k in &sp.knots {
                    assert!((k.anchor.x - 64.23).abs() >= 0.249 - 1e-3 || k.anchor.y < 49.75 || k.anchor.y > 90.25);
                }
            }
        }
        g.commit(&mut s, action).unwrap();
        assert_eq!(s.journal.len(), 1);
        assert!(s.undo());
    }
}
#[test]
fn constructive_cpu_join_scrub_preserves_unrelated_paths_and_bottom_style() {
    let a = Path::new(vec![Subpath::polyline(&[(10., 70.), (60., 70.)])]);
    let b = Path::new(vec![Subpath::polyline(&[(65., 70.), (140., 70.)])]);
    let (mut s, ids) = fixture(vec![a, b, vector::shapes::rect(10., 110., 20., 20.)]);
    let before = s.active().unwrap().doc.clone();
    let g = Gesture::begin(&s, &ids).unwrap();
    g.commit(&mut s, Action::Join { points: vec![[58., 67.], [67., 73.]], tolerance: 2. }).unwrap();
    assert_eq!(s.active().unwrap().doc.layers.len(), 2);
    assert_eq!(path(&s.active().unwrap().doc, ids[0]).subpaths[0].knots.len(), 4);
    assert_eq!(s.active().unwrap().doc.layer(ids[2]), before.layer(ids[2]));
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn constructive_cpu_invalid_options_targets_staleness_and_preview_are_atomic() {
    let (mut s, ids) = fixture(circles());
    let g = Gesture::begin(&s, &ids).unwrap();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    for action in [
        Action::Contour { delta: f64::NAN, join: JoinKind::Miter, miter: 4. },
        Action::Contour { delta: 100_001., join: JoinKind::Miter, miter: 4. },
        Action::Builder { faces: vec![999], remove: false, style: ids[0] },
        Action::Builder { faces: vec![0, 0], remove: true, style: ids[0] },
        Action::Scissors { layer: ids[0], subpath: 0, segment: usize::MAX, t: 0.5 },
        Action::Erase { points: vec![[f64::INFINITY, 0.]], width: 1. },
    ] {
        assert!(g.preview(&action).is_err());
        assert_eq!(s.active().unwrap().doc, before);
        assert_eq!(s.active().unwrap().revision, revision);
        assert!(s.journal.is_empty());
    }
    assert!(Gesture::begin(&s, &[ids[0], ids[0]]).is_err());
    assert!(Gesture::begin(&s, &[ids[0], LayerId(u64::MAX)]).is_err());
    let action = Action::Contour { delta: 3., join: JoinKind::Round, miter: 4. };
    s.execute("path.constructive.preview", serde_json::to_value(g.request(action.clone())).unwrap()).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert_eq!(s.active().unwrap().revision, revision);
    s.active_mut().unwrap().revision += 1;
    assert!(g.commit(&mut s, action).is_err());
    assert!(s.journal.is_empty());
    assert!(!s.undo());
}

#[test]
fn constructive_paint_order_scissors_coincident_hit_uses_top_source() {
    let (mut s, ids) = fixture(vec![rectangle(), rectangle()]);
    let before = s.active().unwrap().doc.clone();
    let g = Gesture::begin(&s, &ids).unwrap();
    let (layer, subpath, segment, t, _) = g.nearest(Point::new(70., 20.), 6.).unwrap().unwrap();
    assert_eq!(layer, ids[1]);
    g.commit(&mut s, Action::Scissors { layer, subpath, segment, t }).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(ids[0]), before.layer(ids[0]));
    assert!(path(&s.active().unwrap().doc, ids[1]).subpaths.iter().all(|sp| !sp.closed));
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn constructive_cpu_appearance_style_and_inherited_pixel_lock() {
    use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem};
    let (mut s, ids) = fixture(vec![rectangle()]);
    let mut doc = (*s.active().unwrap().doc).clone();
    let appearance = Appearance { items: vec![AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgba(0.2, 0.5, 0.7, 0.4))))] };
    doc.layer_mut(ids[0]).unwrap().vector.appearance = Some(appearance.clone());
    s.active_mut().unwrap().doc = Arc::new(doc);
    let g = Gesture::begin(&s, &ids).unwrap();
    let preview = g.preview(&Action::Contour { delta: 5., join: JoinKind::Round, miter: 4. }).unwrap().document().unwrap();
    assert_eq!(preview.layer(ids[0]).unwrap().vector.appearance, Some(appearance));
    let mut doc = (*s.active().unwrap().doc).clone();
    doc.layer_mut(ids[0]).unwrap().locks.pixels = true;
    s.active_mut().unwrap().doc = Arc::new(doc);
    assert!(g.commit(&mut s, Action::Contour { delta: 5., join: JoinKind::Round, miter: 4. }).is_err());
    assert!(s.journal.is_empty());
    assert!(!s.undo());
}

#[test]
fn constructive_cpu_builder_merge_removes_internal_open_divider() {
    let line = Path::new(vec![Subpath::polyline(&[(70., 0.), (70., 140.)])]);
    let (s, ids) = fixture(vec![rectangle(), line]);
    let g = Gesture::begin(&s, &ids).unwrap();
    assert_eq!(g.arrangement().unwrap().regions.len(), 2);
    let result = g.preview(&Action::Builder { faces: vec![0, 1], remove: false, style: ids[0] }).unwrap();
    assert_eq!(result.output_paths().len(), 3);
    parity(result.output_paths()[0], &rectangle());
    for p in result.output_paths().iter().skip(1) {
        assert!(p.subpaths.iter().flat_map(|s| &s.knots).all(|k| k.anchor.y <= 20.0001 || k.anchor.y >= 119.9999));
    }
}

#[test]
fn constructive_cpu_scissors_keeps_subtractive_component_and_zero_smooth_is_noop() {
    let mut ring = rectangle();
    ring.subpaths.extend(vector::shapes::rect(45., 45., 50., 50.).subpaths.into_iter().map(|sp| sp.with_op(photocraft_doc::PathOp::Subtract)));
    let (mut s, ids) = fixture(vec![ring.clone()]);
    let g = Gesture::begin(&s, &ids).unwrap();
    let result = g.preview(&Action::Scissors { layer: ids[0], subpath: 1, segment: 0, t: 0.5 }).unwrap();
    assert_eq!(result.output_paths()[0].subpaths[1].op, photocraft_doc::PathOp::Subtract);
    parity(result.output_paths()[0], &ring);
    g.commit(&mut s, Action::Smooth { points: vec![[70., 70.]], radius: 100., amount: 0. }).unwrap();
    assert!(s.journal.is_empty());
    assert!(!s.undo());
}

#[test]
fn review_regression_constructive_scissors_all_pieces_retain_fill_rule_inversion_and_component_op() {
    let mut p = Path::new(vec![Subpath::polyline(&[(20., 40.), (70., 40.), (120., 40.)]).with_op(photocraft_doc::PathOp::Subtract)]);
    p.fill_rule = photocraft_doc::FillRule::EvenOdd;
    p.inverted = true;
    let (s, ids) = fixture(vec![p.clone()]);
    let g = Gesture::begin(&s, &ids).unwrap();
    let prepared = g.preview(&Action::Scissors { layer: ids[0], subpath: 0, segment: 0, t: 0.5 }).unwrap();
    assert_eq!(prepared.output_paths().len(), 2);
    for piece in prepared.output_paths() {
        assert_eq!(piece.fill_rule, p.fill_rule);
        assert_eq!(piece.inverted, p.inverted);
        assert!(piece.subpaths.iter().all(|sp| sp.op == photocraft_doc::PathOp::Subtract));
    }
}
#[test]
fn review_regression_constructive_erase_single_path_pieces_retain_fill_rule_and_inversion() {
    let mut p = Path::new(vec![Subpath::polyline(&[(20., 40.), (70., 40.), (120., 40.)])]);
    p.fill_rule = photocraft_doc::FillRule::EvenOdd;
    p.inverted = true;
    let pieces = erase(&p, &vector::shapes::rect(55., 30., 30., 20.)).unwrap().unwrap();
    assert_eq!(pieces.len(), 2);
    for piece in pieces {
        assert_eq!(piece.fill_rule, p.fill_rule);
        assert_eq!(piece.inverted, p.inverted);
    }
}

#[test]
fn dsfix_v122_v123_split_and_erase_preserve_fill_semantics() {
    let mut source = Path::new(vec![Subpath::polyline(&[(10., 70.), (140., 70.)])]);
    source.fill_rule = photocraft_doc::FillRule::EvenOdd;
    source.inverted = true;
    let (s, ids) = fixture(vec![source.clone()]);
    let g = Gesture::begin(&s, &ids).unwrap();
    let split = g.preview(&Action::Scissors { layer: ids[0], subpath: 0, segment: 0, t: 0.5 }).unwrap();
    assert_eq!(split.output_paths().len(), 2);
    for p in split.output_paths() {
        assert_eq!(p.fill_rule, source.fill_rule);
        assert_eq!(p.inverted, source.inverted);
    }
    let brush = vector::shapes::rect(60., 50., 5., 40.);
    let erased = erase(&source, &brush).unwrap().unwrap();
    assert_eq!(erased.len(), 2);
    for p in erased {
        assert_eq!(p.fill_rule, source.fill_rule);
        assert_eq!(p.inverted, source.inverted);
    }
}
