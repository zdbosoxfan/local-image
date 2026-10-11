#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, PathOp, Subpath};
use photocraft_geom::Affine;

pub(super) fn sources() -> Vec<Layer> {
    [
        photocraft_vector::shapes::ellipse(16., 24., 80., 80.),
        photocraft_vector::shapes::ellipse(56., 24., 80., 80.),
        Path::new(vec![Subpath::polyline(&[(0., 64.), (150., 64.)])]),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, path)| {
        let mut layer = Layer::new(
            "Source",
            LayerContent::Shape(ShapeLayer { path, fill: if i < 2 { Some(Fill::Solid(Color::BLACK)) } else { None }, ..Default::default() }),
        );
        layer.id = LayerId(i as u64 + 1);
        layer
    })
    .collect()
}
#[test]
fn live_paint42_six_regions_area_and_independent_coverage_oracle() {
    let sources = sources();
    let graph = Graph::new(&sources, Gap::None).unwrap();
    assert_eq!(graph.faces.len(), 6);
    assert_eq!(graph.faces.iter().map(|f| &f.signature).collect::<HashSet<_>>().len(), 6);
    let faces = graph.touched(&[Point::new(32., 40.), Point::new(76., 40.)]).unwrap();
    assert_eq!(faces.len(), 2);
    let path = graph.merge(&faces).unwrap();
    let analytic = std::f64::consts::PI * 40. * 40. / 2.;
    assert!((ops::area(&path).unwrap() - analytic).abs() < 1.);
    for i in &faces {
        assert!((ops::area(&graph.faces[*i].region.path).unwrap() - graph.faces[*i].area).abs() < 1e-7);
    }
    // The original circle intersected with an upper half-plane through PSD PathOp
    // coverage is independent of the sweep and FaceMerger.
    let LayerContent::Shape(shape) = &sources[0].content else { panic!() };
    let mut oracle = shape.path.clone();
    let mut upper = photocraft_vector::shapes::rect(0., 0., 160., 64.).subpaths.remove(0);
    upper.op = PathOp::Intersect;
    oracle.subpaths.push(upper);
    let rect = photocraft_geom::Rect::new(0, 0, 160, 128);
    let expected = photocraft_vector::path_coverage(&oracle, rect);
    let actual = photocraft_vector::path_coverage(&path, rect);
    let max = expected.iter().zip(&actual).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
    assert!(max <= 1. / 255., "coverage max={max}");
}
#[test]
fn live_paint42_source_move_preserves_face_and_edge_paint_signatures() {
    let mut sources = sources();
    let old = Graph::new(&sources, Gap::None).unwrap();
    let i = old.face_at(Point::new(76., 40.)).unwrap().unwrap();
    let mut spec = LivePaintSpec::default();
    spec.set_face(old.faces[i].signature.clone(), Some(Fill::Solid(Color::rgb(1., 0., 0.)))).unwrap();
    let edge = old.edge_at(Point::new(70., 64.), 1.).unwrap().unwrap();
    spec.set_edge(old.edges[edge].signature.clone(), Some(ShapeStroke { paint: Fill::Solid(Color::rgb(0., 0., 1.)), width: 2., ..Default::default() }))
        .unwrap();
    if let LayerContent::Shape(shape) = &mut sources[1].content {
        shape.path = shape.path.transform(&Affine::translate(5., 0.));
    }
    let new = Graph::new(&sources, Gap::None).unwrap();
    assert_eq!(new.faces.len(), 6);
    let j = new.face_at(Point::new(76., 40.)).unwrap().unwrap();
    assert_eq!(old.faces[i].signature, new.faces[j].signature);
    assert_eq!(new.face_paint(j, &sources, &spec).unwrap(), Some(Fill::Solid(Color::rgb(1., 0., 0.))));
    assert!((new.faces[j].area - old.faces[i].area).abs() > 10.);
    let r = 40f64;
    let d = 45f64;
    let lens = 2. * r * r * (d / (2. * r)).acos() - 0.5 * d * (4. * r * r - d * d).sqrt();
    assert!((new.faces[j].area - lens / 2.).abs() < 1., "{} vs {}", new.faces[j].area, lens / 2.);
    let k = new.edge_at(Point::new(76., 64.), 1.).unwrap().unwrap();
    assert_eq!(new.edge_paint(k, &sources, &spec).unwrap().unwrap().width, 2.);
}
#[test]
fn live_paint42_small_gap_closes_three_pixels_without_editing_source() {
    let mut l = Layer::new(
        "Open",
        LayerContent::Shape(ShapeLayer {
            path: Path::new(vec![Subpath::polyline(&[(20., 20.), (80., 20.), (80., 80.), (20., 80.), (20., 23.)])]),
            ..Default::default()
        }),
    );
    l.id = LayerId(1);
    let before = l.clone();
    assert!(Graph::new(&[l.clone()], Gap::None).unwrap().faces.is_empty());
    let g = Graph::new(&[l.clone()], Gap::Small).unwrap();
    assert_eq!(g.faces.len(), 1);
    assert!((g.faces[0].area - 3600.).abs() < 1e-6);
    assert_eq!(l, before);
    assert!(g.edges.iter().all(|e| e.source == 1));
    if let LayerContent::Shape(shape) = &mut l.content {
        shape.path.subpaths[0].knots.last_mut().unwrap().anchor.y = 25.;
        shape.path.subpaths[0].knots.last_mut().unwrap().in_ctrl.y = 25.;
        shape.path.subpaths[0].knots.last_mut().unwrap().out_ctrl.y = 25.;
    }
    assert!(Graph::new(&[l.clone()], Gap::Small).unwrap().faces.is_empty());
    assert_eq!(Graph::new(&[l], Gap::Medium).unwrap().faces.len(), 1);
}
#[test]
fn live_paint42_visible_boundaries_merge_only_connected_matching_paints() {
    let sources = sources();
    let graph = Graph::new(&sources, Gap::None).unwrap();
    let seed = BTreeSet::from([graph.face_at(Point::new(32., 40.)).unwrap().unwrap()]);
    let all = graph.visible_faces(&seed, &sources).unwrap();
    assert_eq!(all.len(), 6);
    let union = graph.merge(&all).unwrap();
    let LayerContent::Shape(a) = &sources[0].content else { panic!() };
    let LayerContent::Shape(b) = &sources[1].content else { panic!() };
    let actual = ops::area(&union).unwrap();
    let expected = ops::area(&ops::boolean(&a.path, &b.path, ops::BoolOp::Union).unwrap()).unwrap();
    assert!((actual - expected).abs() < 1e-5, "merged area {actual} vs boolean union {expected}");
}
#[test]
fn live_paint42_sparse_pointer_frames_hit_subpixel_regions() {
    let mut sources = Vec::new();
    for (i, path) in [
        photocraft_vector::shapes::rect(0., 0., 100., 20.),
        Path::new(vec![Subpath::polyline(&[(50., -10.), (50., 30.)])]),
        Path::new(vec![Subpath::polyline(&[(50.01, -10.), (50.01, 30.)])]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut l = Layer::new("Thin", LayerContent::Shape(ShapeLayer { path, ..Default::default() }));
        l.id = LayerId(i as u64 + 1);
        sources.push(l);
    }
    let g = Graph::new(&sources, Gap::None).unwrap();
    assert_eq!(g.faces.len(), 3);
    let hits = g.touched(&[Point::new(10., 10.), Point::new(90., 10.)]).unwrap();
    assert_eq!(hits.len(), 3, "hits={hits:?}, faces={:?}", g.faces.iter().map(|f| (f.area, f.bounds)).collect::<Vec<_>>());
}
#[test]
fn live_paint42_expand_and_spec_serde_preserve_geometry_paint_and_defaults() {
    let sources = sources();
    let g = Graph::new(&sources, Gap::None).unwrap();
    let mut spec: LivePaintSpec = serde_json::from_str("{}").unwrap();
    assert_eq!(spec, LivePaintSpec::default());
    let i = g.face_at(Point::new(32., 40.)).unwrap().unwrap();
    spec.set_face(g.faces[i].signature.clone(), Some(Fill::Solid(Color::rgb(0.2, 0.7, 0.4)))).unwrap();
    let bytes = serde_json::to_vec(&spec).unwrap();
    let loaded: LivePaintSpec = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(spec, loaded);
    let a = g.expand(&sources, &spec).unwrap();
    let b = g.expand(&sources, &loaded).unwrap();
    assert_eq!(a, b);
    assert!(a.iter().all(|l| l.vector.live.is_none() && matches!(l.content, LayerContent::Shape(_))));
}
#[test]
fn live_paint42_rejects_nonfinite_duplicate_and_budget_inputs() {
    let sources = sources();
    let g = Graph::new(&sources, Gap::None).unwrap();
    assert!(g.face_at(Point::new(f64::NAN, 0.)).is_err());
    assert!(g.edge_at(Point::new(0., 0.), f64::INFINITY).is_err());
    assert!(g.touched(&[]).is_err());
    assert!(g.merge(&BTreeSet::from([1000])).is_err());
    assert!(Graph::new(&[sources[0].clone(), sources[0].clone()], Gap::Small).is_err());
    assert!(validate_fill(&Fill::Solid(Color::rgb(f32::NAN, 0., 0.))).is_err());
    let mut bad = sources.clone();
    if let LayerContent::Shape(s) = &mut bad[0].content {
        s.path.subpaths[0].knots[0].anchor.x = f64::INFINITY;
    }
    assert!(Graph::new(&bad, Gap::None).is_err());
}

#[test]
fn live_paint42_native_id_remap_keeps_source_slot_signatures() {
    let a = sources();
    let graph = Graph::new(&a, Gap::None).unwrap();
    let mut b = a.clone();
    for (i, l) in b.iter_mut().enumerate() {
        l.id = LayerId(100 + i as u64);
    }
    let loaded = Graph::new(&b, Gap::None).unwrap();
    assert_eq!(graph.faces.iter().map(|f| &f.signature).collect::<Vec<_>>(), loaded.faces.iter().map(|f| &f.signature).collect::<Vec<_>>());
    assert_eq!(graph.edges.iter().map(|e| &e.signature).collect::<Vec<_>>(), loaded.edges.iter().map(|e| &e.signature).collect::<Vec<_>>());
    assert!(loaded.edges.iter().all(|e| e.source >= 100));
}

#[test]
fn live_paint42_closed_ring_includes_unfilled_counter_face() {
    let outer = photocraft_vector::shapes::ellipse(10., 10., 100., 100.);
    let inner = photocraft_vector::shapes::ellipse(40., 40., 40., 40.);
    let path = ops::boolean(&outer, &inner, ops::BoolOp::Difference).unwrap();
    let mut l = Layer::new("Ring", LayerContent::Shape(ShapeLayer { path, fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() }));
    l.id = LayerId(1);
    let graph = Graph::new(&[l.clone()], Gap::None).unwrap();
    assert_eq!(graph.faces.len(), 2);
    let i = graph.face_at(Point::new(60., 60.)).unwrap().unwrap();
    assert!(graph.faces[i].region.sources.is_empty());
    assert!(graph.face_paint(i, &[l.clone()], &Default::default()).unwrap().is_none());
    assert!((graph.faces[i].area - ops::area(&inner).unwrap()).abs() < 1e-6);
    let mut spec = LivePaintSpec::default();
    spec.set_face(graph.faces[i].signature.clone(), Some(Fill::Solid(Color::WHITE))).unwrap();
    assert_eq!(graph.expand(&[l], &spec).unwrap().len(), 2);
}
