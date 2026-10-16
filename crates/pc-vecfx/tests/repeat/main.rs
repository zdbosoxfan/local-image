use photocraft_doc::{Affine, Color, Fill, Layer, LayerContent, LayerId, Path, ShapeLayer, Subpath, vector_design::live::LiveGroup};
use photocraft_geom::Point;
use photocraft_vecfx::repeat::*;

fn b() -> Bounds {
    Bounds::new([2., 3.], [12., 9.]).unwrap()
}
fn close(a: Point, b: Point) {
    assert!((a.x - b.x).hypot(a.y - b.y) < 1e-8, "{a:?} != {b:?}");
}
fn source(id: u64) -> Layer {
    let mut l = Layer::new(
        "source",
        LayerContent::Shape(ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(2., 3.), (12., 3.), (12., 9.), (2., 9.)])]),
            fill: Some(Fill::Solid(Color::BLACK)),
            ..Default::default()
        }),
    );
    l.id = LayerId(id);
    l
}
fn group(id: u64, children: Vec<Layer>, spec: RepeatSpec) -> Layer {
    let mut l = Layer::group("repeat", children);
    l.id = LayerId(id);
    l.vector.live = Some(LiveGroup::Repeat(spec));
    l
}
#[test]
fn repeat19_radial_has_exact_quarter_turn_oracle_and_n_fold_symmetry() {
    let spec = RepeatSpec::Radial { count: 4, center: [5., 7.], angle: 0. };
    let transforms = transforms(&spec, b()).unwrap();
    let p = Point::new(9., 10.);
    for (a, expected) in transforms.iter().zip([(9., 10.), (2., 11.), (1., 4.), (8., 3.)]) {
        close(a.apply(p), Point::new(expected.0, expected.1));
        assert!((a.determinant() - 1.).abs() < 1e-12);
    }
    for n in [1, 2, 3, 7, 31, 1000] {
        let transforms = photocraft_vecfx::repeat::transforms(&RepeatSpec::Radial { count: n, center: [5., 7.], angle: 0.37 }, b()).unwrap();
        let turn = Affine::translate(5., 7.).mul(&Affine::rotate(std::f64::consts::TAU / f64::from(n))).mul(&Affine::translate(-5., -7.));
        for i in 0..n as usize {
            close(turn.apply(transforms[i].apply(p)), transforms[(i + 1) % n as usize].apply(p));
        }
    }
}
#[test]
fn repeat19_grid_exact_count_spacing_and_paint_order() {
    let transforms = transforms(&RepeatSpec::Grid { rows: 2, cols: 3, spacing: [4., 2.] }, b()).unwrap();
    assert_eq!(transforms.len(), 6);
    let origins: Vec<_> = transforms.iter().map(|a| a.apply(Point::new(2., 3.))).collect();
    assert_eq!(origins, [(2., 3.), (16., 3.), (30., 3.), (2., 11.), (16., 11.), (30., 11.)].map(|(x, y)| Point::new(x, y)));
    // Independent bounds edge subtraction, including valid overlapping tiles.
    let transforms = photocraft_vecfx::repeat::transforms(&RepeatSpec::Grid { rows: 1, cols: 2, spacing: [-2., 0.] }, b()).unwrap();
    assert_eq!(transforms[1].apply(Point::new(2., 3.)).x - 12., -2.);
}
#[test]
fn repeat19_mirror_is_involution_fixes_axis_and_preserves_distances() {
    for angle in [0., 0.3, std::f64::consts::FRAC_PI_2, -2.1] {
        let transforms = transforms(&RepeatSpec::Mirror { origin: [5., 7.], angle }, b()).unwrap();
        assert_eq!(transforms[0], Affine::IDENTITY);
        let a = transforms[1];
        let axis = Point::new(5. + 13. * angle.cos(), 7. + 13. * angle.sin());
        close(a.apply(axis), axis);
        for p in [Point::new(-20., 31.), Point::new(5., 7.), Point::new(12., 9.)] {
            close(a.apply(a.apply(p)), p);
            let q = a.apply(p);
            let normal = Point::new(-angle.sin(), angle.cos());
            assert!(((p.x - 5.) * normal.x + (p.y - 7.) * normal.y + (q.x - 5.) * normal.x + (q.y - 7.) * normal.y).abs() < 1e-8);
            assert!(((p.x - 5.).hypot(p.y - 7.) - (q.x - 5.).hypot(q.y - 7.)).abs() < 1e-8);
        }
        assert!((a.determinant() + 1.).abs() < 1e-12);
    }
}
#[test]
fn repeat19_refuses_counts_overflow_nonfinite_and_out_of_bounds() {
    for spec in [
        RepeatSpec::Radial { count: 0, center: [0., 0.], angle: 0. },
        RepeatSpec::Radial { count: 1001, center: [0., 0.], angle: 0. },
        RepeatSpec::Grid { rows: u32::MAX, cols: u32::MAX, spacing: [0., 0.] },
        RepeatSpec::Grid { rows: 0, cols: 1, spacing: [0., 0.] },
        RepeatSpec::Grid { rows: 40, cols: 30, spacing: [0., 0.] },
        RepeatSpec::Grid { rows: 1, cols: 2, spacing: [-10., 0.] },
        RepeatSpec::Mirror { origin: [f64::NAN, 0.], angle: 0. },
        RepeatSpec::Mirror { origin: [0., 0.], angle: f64::INFINITY },
        RepeatSpec::Grid { rows: 1, cols: 2, spacing: [f64::NEG_INFINITY, 0.] },
        RepeatSpec::Radial { count: 3, center: [1e20, 0.], angle: 0. },
        RepeatSpec::Grid { rows: 1, cols: 1000, spacing: [MAX_COORDINATE, 0.] },
    ] {
        assert!(transforms(&spec, b()).is_err(), "accepted {spec:?}");
    }
    assert_eq!(transforms(&RepeatSpec::Grid { rows: 20, cols: 50, spacing: [0., 0.] }, b()).unwrap().len(), 1000);
}
#[test]
fn repeat19_projection_is_deterministic_retains_sources_and_instance_clipping() {
    let mut top = source(2);
    top.clipped = true;
    top.opacity = 0.4;
    let input = vec![group(3, vec![source(1), top], RepeatSpec::Grid { rows: 2, cols: 3, spacing: [4., 2.] })];
    let before = input.clone();
    let a = expand_layers(&input).unwrap();
    let z = expand_layers(&input).unwrap();
    assert_eq!(a, z);
    assert_eq!(input, before);
    let LayerContent::Group(g) = &a.layers[0].content else { panic!() };
    assert!(a.layers[0].vector.live.is_none());
    assert_eq!(g.children.len(), 6);
    let mut ids = std::collections::BTreeSet::new();
    for (i, instance) in g.children.iter().enumerate() {
        let children = instance.children().unwrap();
        assert_eq!(children.len(), 2);
        assert!(children[1].clipped);
        assert_eq!(children[1].opacity, 0.4);
        for child in children {
            assert!(ids.insert(child.id));
            assert_eq!(a.sources[&child.id.0], LayerId(if child.clipped { 2 } else { 1 }));
        }
        assert_eq!(a.transforms[&children[0].id.0].m[4], (i % 3) as f64 * 14.);
    }
}
#[test]
fn repeat19_nested_repeats_compose_absolute_transforms_once() {
    let inner = group(2, vec![source(1)], RepeatSpec::Mirror { origin: [20., 0.], angle: std::f64::consts::FRAC_PI_2 });
    let outer = group(3, vec![inner], RepeatSpec::Grid { rows: 2, cols: 1, spacing: [0., 2.] });
    let ex = expand_layers(&[outer]).unwrap();
    fn points(layers: &[Layer], ex: &Expansion, out: &mut Vec<Point>) {
        for l in layers {
            if let LayerContent::Shape(s) = &l.content {
                out.push(ex.transforms[&l.id.0].apply(s.path.subpaths[0].knots[0].anchor));
            }
            if let Some(c) = l.children() {
                points(c, ex, out);
            }
        }
    }
    let mut output = Vec::new();
    points(&ex.layers, &ex, &mut output);
    for (p, q) in output.into_iter().zip([(2., 3.), (38., 3.), (2., 11.), (38., 11.)]) {
        close(p, Point::new(q.0, q.1));
    }
}
#[test]
fn repeat19_projection_refuses_exponential_growth_duplicate_ids_and_bad_geometry() {
    let mut l = source(1);
    for i in 2..8 {
        l = group(i, vec![l], RepeatSpec::Radial { count: 10, center: [0., 0.], angle: 0. });
    }
    assert!(expand_layers(&[l]).is_err());
    assert!(expand_layers(&[source(1), source(1)]).is_err());
    let mut s = source(1);
    if let LayerContent::Shape(sh) = &mut s.content {
        sh.path.subpaths[0].knots[0].anchor.x = f64::NAN;
    }
    assert!(expand_layers(&[s]).is_err());
    assert!(expand_layers(&[group(u64::MAX, vec![source(1)], RepeatSpec::Mirror { origin: [0., 0.], angle: 0. })]).is_err());
}
#[test]
fn repeat19_grid_uses_analytic_curve_bounds_instead_of_control_handles() {
    let mut s = source(1);
    if let LayerContent::Shape(sh) = &mut s.content {
        sh.path = Path::new(vec![Subpath::polygon(&[(0., 0.), (10., 0.)])]);
        let sp = &mut sh.path.subpaths[0];
        sp.closed = false;
        sp.knots[0].out_ctrl = Point::new(0., 100.);
        sp.knots[1].in_ctrl = Point::new(10., 100.);
    }
    let bounds = source_bounds(&[s]).unwrap();
    assert_eq!(bounds.height(), 75.); // cubic y(t)=300t(1-t), maximum at t=1/2.
    let a = transforms(&RepeatSpec::Grid { rows: 2, cols: 1, spacing: [1., 5.] }, bounds).unwrap();
    assert_eq!(a[1].m[5], 80.);
}
#[test]
fn repeat19_exchange_scene_uses_same_projection_without_engine_dependency() {
    let input = vec![group(2, vec![source(1)], RepeatSpec::Radial { count: 4, center: [0., 0.], angle: 0. })];
    let scene = photocraft_doc::vector_design::exchange::ImportedScene {
        size: photocraft_doc::Size::new(32, 32),
        resolution_dpi: 72.,
        units: photocraft_doc::DesignUnits::Pixels,
        layers: input.clone(),
        symbols: vec![],
        warnings: vec![],
    };
    assert_eq!(expand_scene(&scene).unwrap(), expand_layers(&input).unwrap());
}

#[test]
fn review_repeat_unused_pitch_and_large_valid_reflection() {
    let bounds = Bounds::new([999_990., 0.], [1_000_000., 10.]).unwrap();
    assert_eq!(transforms(&RepeatSpec::Grid { rows: 1, cols: 1, spacing: [-10., -10.] }, bounds).unwrap().len(), 1);
    let matrix = transforms(&RepeatSpec::Mirror { origin: [999_995., 5.], angle: std::f64::consts::FRAC_PI_2 }, bounds).unwrap();
    assert_eq!(matrix.len(), 2);
    for transform in matrix {
        let p = transform.apply(Point::new(999_990., 0.));
        assert!(p.x.abs() <= MAX_COORDINATE && p.y.abs() <= MAX_COORDINATE);
    }
}
