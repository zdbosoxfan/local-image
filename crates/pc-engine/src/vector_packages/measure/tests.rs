use super::*;
use photocraft_doc::{ColorMode, FillRule, Knot, Layer, PathOp, SampleType, ShapeLayer, Size};
use photocraft_geom::Point;
fn doc() -> Document {
    let mut d = Document::new("Measure fixture", Size::new(400, 300), ColorMode::Rgb, SampleType::U8);
    d.resolution_dpi = 300.;
    d
}
fn units() -> Units {
    Units { unit: DesignUnits::Pixels, dpi: 300., scale: DrawingScale::default() }
}
fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::new(vec![Subpath::polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])])
}
fn add(d: &mut Document, p: Path) -> LayerId {
    let l = Layer::new("Object", LayerContent::Shape(ShapeLayer { path: p, ..Default::default() }));
    let id = l.id;
    d.layers.push(l);
    id
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-7 * b.abs().max(1.), "{a} != {b}");
}
#[test]
fn measure44_rectangle_exact_units_scale_sum_and_read_only_bytes() {
    let mut d = doc();
    let a = add(&mut d, rectangle(20., 20., 100., 50.));
    let b = add(&mut d, rectangle(30., 30., 100., 50.));
    d.layer_mut(a).unwrap().locks.all = true; // Locked artwork remains measurable.
    let original = d.clone();
    let before = photocraft_format::save_to_bytes(&d, &Default::default()).unwrap();
    let cache = AreaCache::build(&d, 7).unwrap();
    let px = cache.read(&d, 7, &[a], Some((a, [60., 20.])), PerimeterMode::CuspedSegments, units()).unwrap();
    assert_eq!(px.area, 5000.);
    assert_eq!(px.including_holes, 5000.);
    assert_eq!(px.perimeter, 300.);
    assert_eq!(px.segment, Some(100.));
    let mm = cache.read(&d, 7, &[a], None, PerimeterMode::FullEdges, Units { unit: DesignUnits::Millimeters, ..units() }).unwrap();
    close(mm.area, 5000. * 25.4 * 25.4 / 90_000.);
    for (u, factor) in [(DesignUnits::Pixels, 1.), (DesignUnits::Points, 72. / 300.), (DesignUnits::Millimeters, 25.4 / 300.), (DesignUnits::Inches, 1. / 300.)]
    {
        let r = distance([0., 0.], [30., -40.], Units { unit: u, ..units() }).unwrap();
        close(r.distance, 50. * factor);
        close(r.dx, 30. * factor);
        close(r.dy, -40. * factor);
        close(r.angle, 53.13010235415598);
    }
    let scaled = Units { scale: DrawingScale { factor: 100. }, ..units() };
    assert_eq!(distance([10., 10.], [40., 50.], scaled).unwrap().distance, 5000.);
    let sum = cache.read(&d, 7, &[a, b], None, PerimeterMode::FullEdges, scaled).unwrap();
    assert_eq!(sum.area, 100_000_000.);
    assert_eq!(sum.perimeter, 60_000.); // Task specifies sum even for overlapping objects.
    assert_eq!(d, original);
    assert_eq!(before, photocraft_format::save_to_bytes(&d, &Default::default()).unwrap());
}
#[test]
fn measure44_ring_holes_boolean_perimeter_and_open_curve() {
    let mut ring = rectangle(10., 10., 100., 100.);
    ring.subpaths.push(rectangle(35., 35., 50., 50.).subpaths.remove(0).with_op(PathOp::Subtract));
    let a = path_area(&ring).unwrap();
    assert_eq!(a.area, 7500.);
    assert_eq!(a.including_holes, 10_000.);
    close(a.perimeter, 600.);
    // Same hole with even-odd Join; independent shoelace oracle for the square ring.
    ring.fill_rule = FillRule::EvenOdd;
    ring.subpaths[1].op = PathOp::Join;
    assert_eq!(path_area(&ring).unwrap().area, 7500.);
    let open = Path::new(vec![Subpath { closed: false, knots: vec![Knot::corner(0., 0.), Knot::corner(30., 40.)], op: PathOp::Combine }]);
    let a = path_area(&open).unwrap();
    assert_eq!(a.area, 0.);
    close(a.perimeter, 50.);
    let triangle =
        Path::new(vec![Subpath { closed: false, knots: vec![Knot::corner(0., 0.), Knot::corner(30., 0.), Knot::corner(30., 40.)], op: PathOp::Combine }]);
    let area = path_area(&triangle).unwrap();
    assert_eq!(area.area, 600.);
    close(area.perimeter, 70.);
    assert!(segment_length(&open, 0, 99, PerimeterMode::CuspedSegments).is_err());
}
#[test]
fn measure44_live_circle_exact_circumference_and_stale_metadata_ignored() {
    let mut d = doc();
    let live = LiveShape::Ellipse { rect: [20., 20., 100., 100.] };
    let p = photocraft_vector::shapes::live_path(&live);
    let id = add(&mut d, p);
    let LayerContent::Shape(sh) = &mut d.layer_mut(id).unwrap().content else { panic!() };
    sh.live = Some(live);
    let c = AreaCache::build(&d, 0).unwrap();
    let a = c.read(&d, 0, &[id], Some((id, [70., 20.])), PerimeterMode::CuspedSegments, units()).unwrap();
    assert!((a.perimeter / (100. * std::f64::consts::PI) - 1.).abs() < 1e-6);
    close(a.area, 2500. * std::f64::consts::PI);
    close(a.segment.unwrap(), a.perimeter);
    let LayerContent::Shape(sh) = &mut d.layer_mut(id).unwrap().content else { panic!() };
    sh.path = rectangle(20., 20., 100., 50.);
    let c = AreaCache::build(&d, 1).unwrap();
    assert_eq!(c.read(&d, 1, &[id], None, PerimeterMode::FullEdges, units()).unwrap().perimeter, 300.);
}
#[test]
fn measure44_snapped_nodes_curve_guide_page_and_shift_constraint() {
    let mut d = doc();
    let id = add(&mut d, rectangle(20., 20., 100., 50.));
    d.guides.vertical.push(175.);
    let settings = Settings { curves: true, alignment: false, spacing: false, bounds: false, ..Default::default() };
    let g = Gesture::begin(&d, 0, [19., 19.], &settings, View::default(), false).unwrap();
    assert_eq!(g.start, [20., 20.]);
    let end = g.endpoint([121., 71.], &settings, View::default(), false, false).unwrap();
    assert_eq!(end.point, [120., 70.]);
    close(distance(g.start, end.point, units()).unwrap().distance, 12500_f64.sqrt());
    let edge = g.endpoint([60., 21.], &settings, View::default(), false, false).unwrap().point;
    close(edge[0], 60.);
    close(edge[1], 20.);
    assert_eq!(g.endpoint([176., 90.], &settings, View::default(), false, false).unwrap().point, [175., 90.]);
    assert_eq!(g.endpoint([399., 90.], &settings, View::default(), false, false).unwrap().point, [400., 90.]);
    for bypass in [false, true] {
        let r = g.endpoint([83., 65.], &Settings { enabled: false, ..settings.clone() }, View::default(), true, bypass).unwrap();
        close(r.point[0] - 20., r.point[1] - 20.);
        close(distance(g.start, r.point, units()).unwrap().distance, 63_f64.hypot(45.));
    }
    assert!(g.capture.layers.contains(&id));
    assert!(g.capture.check(&d, 1).is_err());
    d.layers.clear();
    assert!(g.capture.check(&d, 0).is_err());
}
#[test]
fn measure44_cusped_run_and_full_edges_independent_lengths() {
    let mut path = Path::new(vec![Subpath {
        closed: false,
        op: PathOp::Combine,
        knots: vec![Knot::corner(0., 0.), Knot::corner(30., 40.), Knot::corner(60., 80.), Knot::corner(60., 100.)],
    }]);
    path.subpaths[0].knots[1].smooth = true;
    close(segment_length(&path, 0, 1, PerimeterMode::CuspedSegments).unwrap(), 100.);
    close(segment_length(&path, 0, 2, PerimeterMode::CuspedSegments).unwrap(), 20.);
    close(segment_length(&path, 0, 1, PerimeterMode::FullEdges).unwrap(), 120.);
    // Cubic x=30t, y=0 is exactly 30 px despite its asymmetric handles.
    let p = Path::new(vec![Subpath {
        closed: false,
        op: PathOp::Combine,
        knots: vec![
            Knot::smooth(Point::new(0., 0.), Point::new(0., 0.), Point::new(10., 0.)),
            Knot::smooth(Point::new(30., 0.), Point::new(20., 0.), Point::new(30., 0.)),
        ],
    }]);
    close(path_area(&p).unwrap().perimeter, 30.);
}
#[test]
fn measure44_validation_hover_order_and_group_selection_deduplicated() {
    let mut d = doc();
    let id = add(&mut d, rectangle(20., 20., 100., 50.));
    let top = add(&mut d, rectangle(20., 20., 20., 20.));
    let c = AreaCache::build(&d, 0).unwrap();
    assert_eq!(c.hover([25., 25.], 0.).unwrap(), Some(top));
    assert_eq!(c.hover([100., 100.], 0.).unwrap(), None);
    assert!(Capture::new(&d, 0, vec![id, id]).is_err());
    assert!(c.read(&d, 1, &[id], None, PerimeterMode::FullEdges, units()).is_err());
    for v in [f64::NAN, f64::INFINITY, 1e20] {
        assert!(distance([v, 0.], [0., 0.], units()).is_err());
        assert!(DrawingScale { factor: v }.validate().is_err());
    }
    assert!(DrawingScale::assign(0., 1., DesignUnits::Pixels, 300.).is_err());
    assert!(distance([0., 0.], [1., 1.], Units { dpi: 0., ..units() }).is_err());
    let child = d.layers.remove(0);
    let group = Layer::group("Group", vec![child]);
    let parent = group.id;
    d.layers.push(group);
    let c = AreaCache::build(&d, 1).unwrap();
    assert_eq!(c.read(&d, 1, &[parent, id], None, PerimeterMode::FullEdges, units()).unwrap().area, 5000.);
}
#[test]
fn measure44_scale_assignment_metadata_transaction_and_query_contract() {
    assert_eq!(serde_json::from_str::<DrawingScale>("{}").unwrap(), DrawingScale::default());
    close(DrawingScale::assign(300., 2540., DesignUnits::Millimeters, 300.).unwrap().factor, 100.);
    let mut s = Session::new();
    s.add_document(doc(), None);
    let st = s.active().unwrap();
    let capture = Capture::new(&st.doc, st.revision, vec![]).unwrap();
    let before = st.doc.clone();
    let rev = st.revision;
    for spec in specs() {
        assert_eq!(spec.journal, spec.id == "vector.measure.setScale");
        if spec.id.ends_with("distance") {
            let r = (spec.run)(&mut s, &json!({"capture":capture,"a":[0,0],"b":[30,40]})).unwrap();
            assert_eq!(r["distance"], 50.);
        }
    }
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, rev);
    assert!(s.journal.is_empty());
    let edit = ScaleEdit { capture, artboard: None, scale: DrawingScale { factor: 100. } };
    assert!(edit.commit(&mut s, |_, _, _| Err(error("provider failure"))).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, rev);
    // Simulate 00's metadata setter using an existing data field, then check atomic history.
    edit.commit(&mut s, |d, _, scale| {
        d.name = format!("scale {}", scale.factor);
        Ok(())
    })
    .unwrap();
    assert_eq!(s.active().unwrap().revision, rev + 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(edit.commit(&mut s, |_, _, _| Ok(())).is_err());
}

#[test]
fn review_regression_measure44_distance_does_not_use_area_selection_object_budget() {
    let mut d = doc();
    for _ in 0..MAX_OBJECTS + 1 {
        d.layers.push(Layer::group("Unmeasurable empty group", Vec::new()));
    }
    let settings = Settings { enabled: false, ..Default::default() };
    let view = View { zoom: 1., pixels_per_point: 1. };
    let g = Gesture::begin(&d, 1, [10., 20.], &settings, view, true).unwrap();
    assert_eq!(g.endpoint([30., 40.], &settings, view, false, true).unwrap().point, [30., 40.]);
    g.capture.check(&d, 1).unwrap();
    assert!(g.capture.check(&d, 2).is_err());
}
