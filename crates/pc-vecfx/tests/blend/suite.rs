//! Synthetic, independent geometry/colour oracles. See SOURCES.md.
use photocraft_doc::vector_design::live::{BlendOrientation, BlendSpacing, BlendSpec, LiveGroup};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, Path, PathOp, SampleType, ShapeLayer, Size, Subpath};
use photocraft_geom::{Point, Rect};
use photocraft_vecfx::blend::{self, correspondence::PathPair, spine::Spine};
fn circle(x: f64, r: f64, color: Color) -> Layer {
    Layer::new(
        "Key",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::ellipse(x - r, 32. - r, 2. * r, 2. * r),
            fill: Some(Fill::Solid(color)),
            ..Default::default()
        }),
    )
}
fn path(layer: &Layer) -> &Path {
    let LayerContent::Shape(s) = &layer.content else { panic!() };
    &s.path
}
fn spec(n: u32) -> BlendSpec {
    BlendSpec { spacing: BlendSpacing::Steps(n), ..Default::default() }
}
#[test]
fn blend18_two_circles_five_steps_exact_radius_and_perceptual_colour() {
    let keys = vec![circle(20., 5., Color::BLACK), circle(100., 17., Color::WHITE)];
    let out = blend::evaluate(&keys, &spec(5)).unwrap();
    assert_eq!(out.len(), 7);
    for (i, l) in out.iter().enumerate() {
        let t = i as f64 / 6.;
        let r = 5. + 12. * t;
        let (x0, y0, x1, y1) = photocraft_pathops::bounds(path(l)).unwrap().unwrap();
        assert!((x1 - x0 - 2. * r).abs() < 1e-9 && (y1 - y0 - 2. * r).abs() < 1e-9);
        assert!(((x0 + x1) / 2. - (20. + 80. * t)).abs() < 1e-9);
        let LayerContent::Shape(s) = &l.content else { panic!() };
        let Some(Fill::Solid(c)) = s.fill else { panic!() };
        // Independent neutral-axis Oklab identity: L=t => linear sRGB=t^3.
        let expected = if t.powi(3) <= 0.0031308 { 12.92 * t.powi(3) } else { 1.055 * t.powf(1.25) - 0.055 };
        for v in c.to_rgb() {
            assert!((f64::from(v) - expected).abs() < 2e-6, "{i}: {c:?} vs {expected}");
        }
        let area = photocraft_pathops::area(path(l)).unwrap();
        assert!((area / (std::f64::consts::PI * r * r) - 1.).abs() < 0.0003);
    }
}
#[test]
fn blend18_zero_steps_is_exact_keys_and_many_keys_keep_order() {
    let keys = vec![circle(20., 5., Color::BLACK), circle(60., 9., Color::WHITE), circle(100., 17., Color::BLACK)];
    assert_eq!(blend::evaluate(&keys, &spec(0)).unwrap(), keys);
    let out = blend::evaluate(&keys, &spec(2)).unwrap();
    assert_eq!(out.len(), 7);
    assert_eq!(out[0], keys[0]);
    assert_eq!(out[3], keys[1]);
    assert_eq!(out[6], keys[2]);
}
#[test]
fn blend18_winding_and_start_rotation_preserve_square_area() {
    let a = photocraft_vector::shapes::rect(0., 0., 10., 10.);
    for rotation in 0..4 {
        let mut b = photocraft_vector::shapes::rect(100., 0., 10., 10.);
        b.subpaths[0].knots.rotate_left(rotation);
        b = photocraft_pathops::reverse(&b).unwrap();
        let pair = PathPair::new(&a, &b).unwrap();
        for t in [0.25, 0.5, 0.75] {
            let mid = pair.at(t).unwrap();
            assert!((photocraft_pathops::area(&mid).unwrap() - 100.).abs() < 1e-9);
        }
        assert_eq!(pair.at(0.).unwrap(), a);
        assert_eq!(pair.at(1.).unwrap(), b);
    }
}
#[test]
fn blend18_explicit_start_nodes_and_open_endpoints_override_automatic_match() {
    let a = photocraft_vector::shapes::rect(0., 0., 10., 10.);
    let b = photocraft_vector::shapes::rect(100., 0., 10., 10.);
    let mid = PathPair::with_starts(&a, &b, (Some(0), Some(2))).unwrap().at(0.5).unwrap();
    assert!(photocraft_pathops::area(&mid).unwrap().abs() < 1e-9);
    let line = |x| Path::new(vec![Subpath::polyline(&[(x, 0.), (x + 10., 0.)])]);
    let mid = PathPair::with_starts(&line(0.), &line(100.), (None, Some(1))).unwrap().at(0.5).unwrap();
    assert_eq!(mid.subpaths[0].knots[0].anchor, Point::new(55., 0.));
    assert!(PathPair::with_starts(&a, &b, (Some(99), None)).is_err());
}
#[test]
fn blend18_different_counts_resample_without_changing_key_geometry() {
    let a = photocraft_vector::shapes::rect(0., 0., 10., 10.);
    let b = photocraft_vector::shapes::rect(100., 0., 10., 10.);
    let mut g = photocraft_pathops::to_geometry(&b);
    g.subpaths[0].insert_anchor(0, 0.5);
    let b = photocraft_pathops::from_geometry(&g);
    let mid = PathPair::new(&a, &b).unwrap().at(0.5).unwrap();
    assert_eq!(mid.subpaths[0].knots.len(), 5);
    assert!((photocraft_pathops::area(&mid).unwrap() - 100.).abs() < 1e-9);
}
fn ring(x: f64, swap: bool) -> Path {
    let outer = photocraft_vector::shapes::rect(x, 0., 20., 20.).subpaths.remove(0);
    let inner = photocraft_pathops::reverse(&photocraft_vector::shapes::rect(x + 5., 5., 10., 10.)).unwrap().subpaths.remove(0);
    let mut subs = if swap { vec![inner, outer] } else { vec![outer, inner] };
    subs[1].op = PathOp::Join;
    Path::new(subs)
}
#[test]
fn blend18_compound_holes_match_by_role_when_contours_are_reordered() {
    let a = ring(0., false);
    let mut b = ring(100., true);
    b.subpaths[0].knots.rotate_left(1);
    b.subpaths[1].knots.rotate_left(2);
    let mid = PathPair::new(&a, &b).unwrap().at(0.5).unwrap();
    assert_eq!(mid.subpaths[1].op, PathOp::Join);
    let pixels = photocraft_vector::path_coverage(&mid, Rect::new(50, 0, 70, 20));
    assert_eq!(pixels[10 * 20 + 10], 0.);
    assert_eq!(pixels[2 * 20 + 2], 1.);
    assert!((pixels.iter().sum::<f32>() - 300.).abs() < 1e-4);
}
#[test]
fn blend18_missing_hole_shrinks_continuously() {
    let a = ring(0., false);
    let b = photocraft_vector::shapes::rect(100., 0., 20., 20.);
    let mid = PathPair::new(&a, &b).unwrap().at(0.5).unwrap();
    let pixels = photocraft_vector::path_coverage(&mid, Rect::new(50, 0, 70, 20));
    assert!((pixels.iter().sum::<f32>() - 375.).abs() < 1e-4);
}
#[test]
fn blend18_spine_length_distance_spacing_and_tangent_orientation() {
    let spine = Path::new(vec![Subpath::polyline(&[(20., 20.), (50., 60.), (80., 20.)])]);
    let rail = Spine::new(&spine).unwrap();
    assert_eq!(rail.length(), 100.);
    let (p, angle) = rail.at(0.25).unwrap();
    assert_eq!(p, Point::new(35., 40.));
    assert!((angle - 4f64.atan2(3.)).abs() < 1e-12);
    let keys = vec![circle(20., 5., Color::BLACK), circle(80., 5., Color::WHITE)];
    let out = blend::evaluate(&keys, &BlendSpec { spacing: BlendSpacing::Distance(25.), spine: Some(spine), ..Default::default() }).unwrap();
    assert_eq!(out.len(), 5);
    assert!((blend::center(&out[2]).unwrap().y - 60.).abs() < 1e-9);
    let key = Layer::new("Rectangle", LayerContent::Shape(ShapeLayer { path: photocraft_vector::shapes::rect(15., 15., 10., 4.), ..Default::default() }));
    let mut next = key.clone();
    if let LayerContent::Shape(s) = &mut next.content {
        s.path = s.path.transform(&photocraft_geom::Affine::translate(0., 80.));
    }
    let out = blend::evaluate(&[key, next], &BlendSpec { spacing: BlendSpacing::Steps(1), orientation: BlendOrientation::AlignToPath, ..Default::default() })
        .unwrap();
    let b = photocraft_pathops::bounds(path(&out[1])).unwrap().unwrap();
    assert!((b.2 - b.0 - 4.).abs() < 1e-9);
    assert!((b.3 - b.1 - 10.).abs() < 1e-9);
}
#[test]
fn blend18_curved_spine_uses_curve_length_and_places_symmetrically() {
    let path = Path::new(vec![Subpath {
        closed: false,
        op: PathOp::Combine,
        knots: vec![
            photocraft_doc::Knot::smooth(Point::new(0., 0.), Point::new(0., 0.), Point::new(0., 100.)),
            photocraft_doc::Knot::smooth(Point::new(100., 0.), Point::new(100., 100.), Point::new(100., 0.)),
        ],
    }]);
    let rail = Spine::new(&path).unwrap();
    // This symmetric cubic has an exact arc length of 200.
    assert!((rail.length() - 200.).abs() < 1e-5);
    let (p, angle) = rail.at(0.5).unwrap();
    assert!((p.x - 50.).abs() < 0.002 && (p.y - 75.).abs() < 0.002);
    assert!(angle.abs() < 0.006);
}
#[test]
fn blend18_malformed_parameters_and_output_budgets_are_errors() {
    let keys = vec![circle(20., 5., Color::BLACK), circle(100., 17., Color::WHITE)];
    for spacing in [
        BlendSpacing::Steps(u32::MAX),
        BlendSpacing::Distance(f64::NAN),
        BlendSpacing::Distance(f64::INFINITY),
        BlendSpacing::Distance(0.),
        BlendSpacing::Distance(-10.),
    ] {
        assert!(blend::evaluate(&keys, &BlendSpec { spacing, ..Default::default() }).is_err());
    }
    let mut bad = keys.clone();
    if let LayerContent::Shape(s) = &mut bad[0].content {
        s.path.subpaths[0].knots[0].anchor.x = f64::NAN;
    }
    assert!(blend::evaluate(&bad, &spec(5)).is_err());
    let keys = (0..32).map(|i| circle(i as f64, 1., Color::BLACK)).collect::<Vec<_>>();
    assert!(blend::evaluate(&keys, &spec(1000)).is_err());
    let spine = Path::new(vec![Subpath::polyline(&[(0., 0.), (0., 0.)])]);
    assert!(Spine::new(&spine).is_err());
    assert!(PathPair::new(path(&keys[0]), path(&keys[1])).unwrap().at(f64::NAN).is_err());
}
#[test]
fn blend18_group_children_grow_and_projection_is_deterministic_lazy_and_bounded() {
    let keys =
        vec![Layer::group("A", vec![circle(20., 5., Color::BLACK), circle(40., 3., Color::WHITE)]), Layer::group("B", vec![circle(100., 9., Color::WHITE)])];
    let out = blend::evaluate(&keys, &spec(1)).unwrap();
    assert_eq!(out[1].children().unwrap().len(), 2);
    let second = out[1].children().unwrap()[1].clone();
    let b = photocraft_pathops::bounds(path(&second)).unwrap().unwrap();
    assert!((b.2 - b.0 - 3.).abs() < 1e-9);
    let mut doc = Document::new("Blend", Size::new(128, 64), ColorMode::Rgb, SampleType::U8);
    let mut group = Layer::group("Blend", keys);
    group.vector.live = Some(LiveGroup::Blend(spec(2)));
    doc.layers.push(group);
    let reserved = blend::max_document_id(&doc) + 100;
    let mut master = circle(20., 3., Color::BLACK);
    master.id = photocraft_doc::LayerId(reserved + 10);
    doc.symbols.push(photocraft_doc::vector_design::structure::SymbolDef {
        id: photocraft_doc::vector_design::structure::SymbolId(reserved),
        name: "Reserved master".into(),
        layers: vec![master],
    });
    let a = blend::project_document(&doc).unwrap();
    let b = blend::project_document(&doc).unwrap();
    assert_eq!(a.layers, b.layers);
    assert!(a.layers[0].children().unwrap()[1].id.0 > reserved + 10);
    assert_eq!(a.symbols, doc.symbols);
    assert!(doc.layers[0].vector.live.is_some());
    let ids = a.walk().iter().map(|(_, _, l)| l.id).collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), a.walk().len());
    for (_, _, layer) in a.walk() {
        if let Some(s) = layer.surface() {
            assert!(s.content_bounds().width() <= 128 && s.content_bounds().height() <= 64);
        }
    }
    let before = path(&a.layers[0].children().unwrap()[1].children().unwrap()[0]).clone();
    if let LayerContent::Shape(sh) = &mut doc.layers[0].children_mut().unwrap()[0].children_mut().unwrap()[0].content {
        sh.path = photocraft_vector::shapes::ellipse(10., 22., 20., 20.);
    }
    let updated = blend::project_document(&doc).unwrap();
    assert_ne!(path(&updated.layers[0].children().unwrap()[1].children().unwrap()[0]), &before);
}
#[test]
fn blend18_old_specs_load_and_gradient_stop_correspondence_preserves_alpha() {
    let old: BlendSpec = serde_json::from_str("{}").unwrap();
    assert_eq!(old, BlendSpec::default());
    let keys = [circle(20., 5., Color::BLACK), circle(100., 17., Color::WHITE)];
    assert_eq!(blend::step_count(&keys[0], &keys[1], BlendSpacing::SmoothColor, 80.).unwrap(), 128);
    assert_eq!(blend::evaluate(&keys, &old).unwrap().len(), 130);
    let a = Fill::gradient(vec![(0., Color::BLACK), (1., Color::BLACK)], 0., 100., photocraft_doc::GradientStyle::Linear, false);
    let b = Fill::gradient(vec![(0., Color::WHITE), (0.5, Color::WHITE), (1., Color::WHITE)], 90., 200., photocraft_doc::GradientStyle::Linear, false);
    let Fill::Gradient { stops, angle, scale, .. } = blend::paint::fill(&a, &b, 0.5) else { panic!() };
    assert_eq!(stops.len(), 3);
    assert_eq!(angle, 45.);
    assert_eq!(scale, 150.);
    for (_, c) in stops {
        assert!((c.c[0] - 0.38857284).abs() < 1e-6);
        assert_eq!(c.alpha, 1.);
    }
    let c = blend::paint::color(Color::rgba(0., 0., 0., 0.25), Color::rgba(1., 1., 1., 0.75), 0.5);
    assert_eq!(c.alpha, 0.5);
}

#[test]
fn blend18_appearance_unmatched_paints_and_strokes_interpolate_continuously() {
    use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem, WidthPoint, WidthProfile};
    let mut a = circle(20., 5., Color::BLACK);
    let mut b = circle(100., 17., Color::WHITE);
    a.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::BLACK)))] });
    let mut extra = FillItem::new(Fill::Solid(Color::rgba(1., 0., 0., 0.5)));
    extra.opacity = 0.8;
    b.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::WHITE))), AppearanceItem::Fill(extra)] });
    for t in [0.25, 0.75] {
        let out = blend::paint::appearance(&a, &b, t).unwrap();
        let AppearanceItem::Fill(extra) = &out.items[1] else { panic!() };
        let Fill::Solid(color) = extra.paint else { panic!() };
        assert!((f64::from(extra.opacity * color.alpha) - 0.4 * t).abs() < 1e-7);
    }
    let mut x = StrokeItem::default();
    x.base.width = 2.;
    x.base.opacity = 0.4;
    x.base.dashes = vec![2., 2.];
    x.profile = Some(WidthProfile { points: vec![WidthPoint { t: 0., left: 1., right: 3. }, WidthPoint { t: 1., left: 1., right: 3. }] });
    let mut y = x.clone();
    y.base.width = 6.;
    y.base.opacity = 0.8;
    y.base.dashes = vec![6., 2.];
    y.base.dash_offset = 8.;
    for point in &mut y.profile.as_mut().unwrap().points {
        point.left = 3.;
        point.right = 5.;
    }
    a.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Stroke(x)] });
    b.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Stroke(y)] });
    let out = blend::paint::appearance(&a, &b, 0.5).unwrap();
    let AppearanceItem::Stroke(s) = &out.items[0] else { panic!() };
    assert_eq!(s.base.width, 4.);
    assert!((s.base.opacity - 0.6).abs() < 1e-7);
    assert_eq!(s.base.dashes, vec![4., 2.]);
    assert_eq!(s.base.dash_offset, 4.);
    for point in &s.profile.as_ref().unwrap().points {
        assert_eq!([point.left, point.right], [2., 4.]);
    }
}

#[test]
fn blend18_scene_projection_is_deterministic_and_preserves_exchange_metadata() {
    use photocraft_doc::vector_design::{
        exchange::ImportedScene,
        structure::{SymbolDef, SymbolId},
    };
    let mut a = circle(20., 5., Color::BLACK);
    let mut b = circle(100., 17., Color::WHITE);
    a.id = photocraft_doc::LayerId(1);
    b.id = photocraft_doc::LayerId(2);
    let mut group = Layer::group("Blend", vec![a, b]);
    group.id = photocraft_doc::LayerId(3);
    group.vector.live = Some(LiveGroup::Blend(spec(1)));
    let mut master = circle(20., 3., Color::BLACK);
    master.id = photocraft_doc::LayerId(9);
    let scene = ImportedScene {
        size: Size::new(128, 64),
        resolution_dpi: 144.,
        units: photocraft_doc::DesignUnits::Millimeters,
        layers: vec![group],
        symbols: vec![SymbolDef { id: SymbolId(10), name: "Master".into(), layers: vec![master] }],
        warnings: vec![],
    };
    let format = photocraft_color::PixelFormat::new(ColorMode::Rgb, SampleType::U8, true);
    let a = blend::project_scene(&scene, format, &[]).unwrap();
    let b = blend::project_scene(&scene, format, &[]).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.layers[0].children().unwrap()[1].id, photocraft_doc::LayerId(11));
    assert_eq!((a.size, a.resolution_dpi, a.units), (scene.size, scene.resolution_dpi, scene.units));
    assert_eq!(a.symbols, scene.symbols);
    assert_eq!(a.warnings, scene.warnings);
    assert!(scene.layers[0].vector.live.is_some());
}
