use super::*;
use photocraft_doc::{Color, ColorMode, Fill, Layer, SampleType, ShapeLayer, Size, Subpath};
use serde_json::json;

fn document() -> Document {
    Document::new("Snapping", Size::new(400, 300), ColorMode::Rgb, SampleType::U8)
}
fn shape(doc: &mut Document, rect: [f64; 4]) -> LayerId {
    let path = Path::new(vec![Subpath::polygon(&[(rect[0], rect[1]), (rect[2], rect[1]), (rect[2], rect[3]), (rect[0], rect[3])])]);
    let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    crate::vector_cmds::refresh_shape(doc, &mut sh);
    let layer = Layer::new("Target", LayerContent::Shape(sh));
    let id = layer.id;
    doc.layers.push(layer);
    id
}
fn only(kind: Kind) -> Settings {
    Settings {
        enabled: true,
        nodes: kind == Kind::Node,
        curves: kind == Kind::Curve,
        bounds: kind == Kind::Bounds,
        guides: kind == Kind::Guide,
        grid: kind == Kind::Grid,
        pixel: kind == Kind::Pixel,
        artboard: kind == Kind::Artboard,
        document: kind == Kind::Document,
        alignment: kind == Kind::Alignment,
        spacing: kind == Kind::Spacing,
        angle: kind == Kind::Angle,
        ..Default::default()
    }
}
#[test]
fn snapping_guides_grid_nodes_pixels_at_zoom_and_hidpi() {
    let mut doc = document();
    doc.guides.vertical.push(120.0);
    shape(&mut doc, [80.0, 50.0, 100.0, 90.0]);
    let cache = Cache::build(&doc, 7, Exclusions::default(), None).unwrap();
    for zoom in [0.25, 1.0, 4.0, 8.0] {
        for ppp in [1.0, 2.0] {
            let view = View { zoom, pixels_per_point: ppp };
            let tol = view.tolerance(8.0).unwrap();
            let guide = cache.point([120.0 + tol * 0.75, 33.0], &only(Kind::Guide), view, None, false).unwrap();
            assert_eq!(guide.point, [120.0, 33.0]);
            assert!(cache.point([120.0 + tol * 1.01, 33.0], &only(Kind::Guide), view, None, false).unwrap().guides.is_empty());
            let node = cache.point([100.0 + tol * 0.5, 90.0], &only(Kind::Node), view, None, false).unwrap();
            assert_eq!(node.point, [100.0, 90.0]);
            let grid = Settings { grid_step: 20.0, grid_origin: [3.0, -2.0], ..only(Kind::Grid) };
            assert_eq!(cache.point([23.0 + tol.min(1.0) * 0.5, 38.0], &grid, view, None, false).unwrap().point, [23.0, 38.0]);
            assert_eq!(cache.point([40.1, 49.9], &only(Kind::Pixel), view, None, false).unwrap().point, [40.0, 50.0]);
        }
    }
}
#[test]
fn snapping_tie_priority_and_reordering_are_stable() {
    let mut doc = document();
    let a = shape(&mut doc, [40.0, 50.0, 60.0, 80.0]);
    let b = shape(&mut doc, [80.0, 50.0, 100.0, 80.0]);
    let settings = only(Kind::Node);
    let query = |d: &Document| {
        Cache::build(d, 0, Exclusions::default(), None)
            .unwrap()
            .point([70.0, 50.0], &Settings { tolerance_px: 20.0, ..settings.clone() }, View::default(), None, false)
            .unwrap()
    };
    let result = query(&doc);
    doc.layers.reverse();
    assert_eq!(query(&doc), result);
    assert_eq!(result.guides[0].hit.source, Source::Layer(a.min(b)));
    doc.guides.vertical.push(60.0);
    let cache = Cache::build(&doc, 0, Exclusions::default(), None).unwrap();
    let result = cache.point([61.0, 50.0], &Settings { document: false, ..Default::default() }, View::default(), None, false).unwrap();
    assert_eq!(result.guides[0].hit.kind, Kind::Guide);
    // Guides win rectangle ties too, regardless of which anchor is visited first.
    let result = cache.rect([59.0, 120.0, 79.0, 140.0], &Settings { document: false, alignment: false, ..Default::default() }, View::default(), false).unwrap();
    assert_eq!(result.delta[0], 1.0);
    assert_eq!(result.guides[0].hit.kind, Kind::Guide);
}
#[test]
fn snapping_excludes_descendants_hidden_ancestors_masks_and_paths() {
    let mut doc = document();
    let id = shape(&mut doc, [10.0, 20.0, 30.0, 40.0]);
    let layer = doc.layers.remove(0);
    let group = Layer::group("Moving", vec![layer]);
    let group_id = group.id;
    doc.layers.push(group);
    let exclude = Exclusions { layers: vec![group_id], ..Default::default() };
    let cache = Cache::build(&doc, 0, exclude, None).unwrap();
    assert!(cache.point([10.1, 20.1], &only(Kind::Node), View::default(), None, false).unwrap().guides.is_empty());
    doc.layers[0].visible = false;
    let cache = Cache::build(&doc, 0, Exclusions::default(), None).unwrap();
    assert!(cache.nodes.is_empty());
    assert!(Cache::build(&doc, 0, Exclusions { layers: vec![id, id], ..Default::default() }, None).is_err());
    let path = Path::new(vec![Subpath::polyline(&[(10.0, 20.0), (30.0, 40.0)])]);
    doc.work_path = Some(path.clone());
    doc.paths.push(photocraft_doc::NamedPath { name: "Saved".into(), path, psd_raw: None });
    let cache = Cache::build(&doc, 0, Exclusions { work_path: true, paths: vec!["Saved".into()], ..Default::default() }, None).unwrap();
    assert!(cache.nodes.is_empty());
}
#[test]
fn snapping_cached_targets_never_follow_preview_or_revision() {
    let mut doc = document();
    let id = shape(&mut doc, [10.0, 20.0, 30.0, 40.0]);
    let cache = Cache::build(&doc, 4, Exclusions::default(), None).unwrap();
    let count = cache.target_count();
    doc.layer_mut(id).unwrap().visible = false;
    assert!(cache.is_current(doc.id, 4));
    assert!(!cache.is_current(doc.id, 5));
    assert!(!cache.is_current(DocId(999), 4));
    for x in 10..20 {
        let result = cache.point([f64::from(x) + 0.1, 20.0], &only(Kind::Node), View::default(), None, false).unwrap();
        assert_eq!(cache.target_count(), count);
        assert_eq!(result.guides.is_empty(), x >= 18);
    }
}
// A quadratic parabola y=x²/100 represented as a cubic. At x=50, the normal is
// (-1,1); its analytically known closest point is (50,25). This callback checks
// adapter plumbing; V1's solver remains a separately named integration gate.
fn parabola_nearest(_: &Path, p: Point) -> Result<Option<Nearest>> {
    assert!((p.x - 49.0).abs() < 1e-8 && (p.y - 26.0).abs() < 1e-8);
    Ok(Some((0, 0, 0.5, Point::new(50.0, 25.0), 2.0_f64.sqrt())))
}
fn parabola_bounds(_: &Path) -> Result<Option<Bounds>> {
    Ok(Some((0.0, 0.0, 100.0, 100.0)))
}
#[test]
fn snapping_nearest_curve_is_atomic_and_uses_v1_adapter_at_hidpi() {
    let mut doc = document();
    let path = Path::new(vec![Subpath {
        knots: vec![
            photocraft_doc::Knot::smooth(Point::new(0.0, 0.0), Point::new(0.0, 0.0), Point::new(100.0 / 3.0, 0.0)),
            photocraft_doc::Knot::smooth(Point::new(100.0, 100.0), Point::new(200.0 / 3.0, 100.0 / 3.0), Point::new(100.0, 100.0)),
        ],
        closed: false,
        op: Default::default(),
    }]);
    doc.layers.push(Layer::new("Parabola", LayerContent::Shape(ShapeLayer { path, ..Default::default() })));
    doc.guides.vertical.push(47.0); // farther guide must not pull the nearest point off the curve
    let cache = Cache::build(&doc, 0, Exclusions::default(), Some(Geometry { nearest: parabola_nearest, bounds: parabola_bounds })).unwrap();
    for (zoom, ppp) in [(1.0, 1.0), (2.0, 2.0)] {
        let result = cache.point([49.0, 26.0], &Settings { guides: true, ..only(Kind::Curve) }, View { zoom, pixels_per_point: ppp }, None, false).unwrap();
        assert_eq!(result.point, [50.0, 25.0]);
        assert_eq!(result.guides.len(), 1);
        assert_eq!(result.guides[0].hit.t, Some(0.5));
        assert_eq!(result.guides[0].hit.kind, Kind::Curve);
    }
    let cache = Cache::build(&doc, 0, Exclusions::default(), None).unwrap();
    assert!(cache.point([49.0, 26.0], &only(Kind::Curve), View::default(), None, false).is_err());
}
#[test]
fn snapping_equal_gap_spacing_for_multi_object_union_and_both_axes() {
    for axis in 0..2 {
        let swap = |mut r: [f64; 4]| {
            if axis == 1 {
                r.swap(0, 1);
                r.swap(2, 3);
            }
            r
        };
        let mut doc = document();
        shape(&mut doc, swap([10.0, 30.0, 30.0, 60.0]));
        shape(&mut doc, swap([90.0, 30.0, 110.0, 60.0]));
        let cache = Cache::build(&doc, 0, Exclusions::default(), None).unwrap();
        // The union of two moving rectangles is 20px wide. Equal gaps are (60-20)/2=20.
        let result = cache.rect(swap([52.0, 32.0, 72.0, 58.0]), &only(Kind::Spacing), View::default(), false).unwrap();
        assert_eq!(result.delta[axis], -2.0);
        assert_eq!(result.guides.len(), 2);
        assert!(result.guides.iter().all(|g| g.value == Some(20.0)));
        // Repeat the existing 60px gap after the pair.
        let result = cache.rect(swap([168.0, 32.0, 188.0, 58.0]), &only(Kind::Spacing), View::default(), false).unwrap();
        assert_eq!(result.delta[axis], 2.0);
        assert!(result.guides.iter().all(|g| g.value == Some(60.0)));
        assert!(cache.rect(swap([52.0, 100.0, 72.0, 120.0]), &only(Kind::Spacing), View::default(), false).unwrap().guides.is_empty());
    }
}
#[test]
fn snapping_angle_projection_and_bypass_are_exact() {
    let cache = Cache::build(&document(), 0, Exclusions::default(), None).unwrap();
    let angle = Angle { origin: [100.0, 100.0], step: std::f64::consts::FRAC_PI_4, offset: 0.0 };
    let result = cache.point([161.0, 162.0], &only(Kind::Angle), View::default(), Some(angle), false).unwrap();
    assert!((result.point[0] - 161.5).abs() < 1e-10 && (result.point[1] - 161.5).abs() < 1e-10);
    let settings = Settings { pixel: true, grid: true, ..only(Kind::Angle) };
    assert_eq!(cache.point([161.1, 162.2], &settings, View::default(), Some(angle), true).unwrap(), SnapResult::unchanged([161.1, 162.2]));
}
#[test]
fn snapping_artboard_and_alignment_and_final_guide_spans() {
    let mut doc = document();
    let mut board = Layer::group("Board", vec![]);
    if let LayerContent::Group(g) = &mut board.content {
        g.artboard = Some(photocraft_doc::Artboard::new(photocraft_geom::Rect::new(60, 80, 200, 220)));
    }
    doc.layers.push(board);
    doc.guides.vertical.push(120.0);
    doc.guides.horizontal.push(80.0);
    shape(&mut doc, [20.0, 20.0, 40.0, 40.0]);
    let cache = Cache::build(&doc, 0, Exclusions::default(), None).unwrap();
    assert_eq!(cache.point([61.0, 100.0], &only(Kind::Artboard), View::default(), None, false).unwrap().point, [60.0, 100.0]);
    assert_eq!(cache.point([31.0, 100.0], &only(Kind::Alignment), View::default(), None, false).unwrap().point, [30.0, 100.0]);
    let result = cache.point([119.0, 81.0], &only(Kind::Guide), View::default(), None, false).unwrap();
    assert_eq!(result.point, [120.0, 80.0]);
    assert!(result.guides.iter().all(|g| g.a[0] == g.b[0] || g.a[1] == g.b[1]));
}
#[test]
fn snapping_invalid_inputs_fail_and_old_settings_load() {
    let settings: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(settings, Settings::default());
    let cache = Cache::build(&document(), 0, Exclusions::default(), None).unwrap();
    for bad in [f64::NAN, f64::INFINITY, -f64::INFINITY, 1e8] {
        assert!(cache.point([bad, 0.0], &settings, View::default(), None, false).is_err());
        assert!(cache.rect([0.0, 0.0, bad, 10.0], &settings, View::default(), false).is_err());
        assert!(View { zoom: bad, ..Default::default() }.tolerance(8.0).is_err());
    }
    assert!(cache.point([0.0, 0.0], &Settings { grid_step: 0.0, ..settings.clone() }, View::default(), None, false).is_err());
    assert!(cache.rect([2.0, 0.0, 1.0, 1.0], &settings, View::default(), false).is_err());
    let mut doc = document();
    let mut board = Layer::group("Oversized board", vec![]);
    if let LayerContent::Group(g) = &mut board.content {
        g.artboard = Some(photocraft_doc::Artboard::new(photocraft_geom::Rect::new(0, 0, 100_000_000, 20)));
    }
    doc.layers.push(board);
    assert!(Cache::build(&doc, 0, Exclusions::default(), None).is_err(), "out-of-range artboard spans must not reach guide painting");
}
#[test]
fn snapping_command_is_nonjournaled_and_stale_queries_are_rejected() {
    let mut s = crate::Session::new();
    s.add_document(document(), None);
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    let result = s.execute("vector.snap.query", json!({"point":[0.1,0.1],"settings":{"pixel":true}})).unwrap();
    assert_eq!(result["result"]["point"], json!([0.0, 0.0]));
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, rev);
    assert!(s.journal.is_empty());
    assert!(!s.undo());
    for p in [
        json!({"point":[1.0,1.0],"revision":rev+1}),
        json!({"point":[1.0,1.0],"rect":[0.0,0.0,1.0,1.0]}),
        json!({"point":[1.0,1.0],"settings":{"unknown":true}}),
        json!({"point":[1.0,1.0],"exclude":{"layers":[999]}}),
    ] {
        assert!(s.execute("vector.snap.query", p).is_err());
    }
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
}

#[test]
fn snapping_grid_randomized_independent_neighbor_oracle_256_cases() {
    let cache = Cache::build(&document(), 0, Exclusions::default(), None).unwrap();
    let settings = Settings { grid_step: 7.0, grid_origin: [3.0, -2.0], tolerance_px: 64.0, ..only(Kind::Grid) };
    let mut seed = 0x9a22_1371_u64;
    for _ in 0..256 {
        let mut p = [0.0; 2];
        for v in &mut p {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            *v = ((seed >> 32) % 100_000) as f64 / 100.0 - 500.0;
        }
        let result = cache.point(p, &settings, View::default(), None, false).unwrap();
        for axis in 0..2 {
            // Enumerate a finite lattice independently, rather than duplicating rounding.
            let expected =
                (-100..=100).map(|i| settings.grid_origin[axis] + 7.0 * f64::from(i)).min_by(|a, b| (a - p[axis]).abs().total_cmp(&(b - p[axis]).abs()));
            let expected = expected.unwrap();
            assert_eq!((result.point[axis] - p[axis]).abs(), (expected - p[axis]).abs());
            assert!((-100..=100).any(|i| settings.grid_origin[axis] + 7.0 * f64::from(i) == result.point[axis]));
            assert!((result.point[axis] - p[axis]).abs() <= 3.5);
            assert!(result.guides[axis].a[axis] == result.point[axis] && result.guides[axis].b[axis] == result.point[axis]);
        }
    }
}
#[test]
fn snapping_guides_ties_ignore_guide_iteration_order() {
    let mut doc = document();
    doc.guides.vertical = vec![110.0, 90.0];
    let cache = Cache::build(&doc, 0, Exclusions::default(), None).unwrap();
    let settings = Settings { tolerance_px: 20.0, ..only(Kind::Guide) };
    let result = cache.point([100.0, 20.0], &settings, View::default(), None, false).unwrap();
    doc.guides.vertical.reverse();
    let cache = Cache::build(&doc, 0, Exclusions::default(), None).unwrap();
    assert_eq!(result.point, cache.point([100.0, 20.0], &settings, View::default(), None, false).unwrap().point);
    assert_eq!(result.point, [90.0, 20.0]);
}

#[test]
fn snapping_masks_and_cache_budget_are_checked() {
    let mut doc = document();
    let mut layer = Layer::raster("Masked", doc.pixel_format());
    let id = layer.id;
    layer.vector_mask = Some(photocraft_doc::VectorMask::new(Path::new(vec![Subpath::polyline(&[(71.0, 81.0), (91.0, 81.0)])])));
    doc.layers.push(layer);
    let cache = Cache::build(&doc, 0, Exclusions::default(), None).unwrap();
    let result = cache.point([72.0, 82.0], &only(Kind::Node), View::default(), None, false).unwrap();
    assert_eq!(result.point, [71.0, 81.0]);
    assert_eq!(result.guides[0].hit.source, Source::Mask(id));
    let cache = Cache::build(&doc, 0, Exclusions { layers: vec![id], ..Default::default() }, None).unwrap();
    assert!(cache.nodes.is_empty());
    doc.guides.vertical = vec![1.0; MAX_TARGETS + 1];
    assert!(Cache::build(&doc, 0, Exclusions::default(), None).is_err());
}

#[test]
fn review_additional_curve_snap_culls_distant_cached_control_bounds() {
    thread_local! { static CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    fn nearest(_: &Path, _: photocraft_geom::Point) -> crate::Result<Option<Nearest>> {
        CALLS.with(|count| count.set(count.get() + 1));
        Ok(None)
    }
    let mut doc = document();
    for i in 0..100 {
        shape(&mut doc, [i as f64 * 100., 100., i as f64 * 100. + 10., 110.]);
    }
    let cache = Cache::build(&doc, 0, Exclusions::default(), Some(Geometry { nearest, bounds: |_| Ok(None) })).unwrap();
    cache.point([5., 105.], &only(Kind::Curve), View::default(), None, false).unwrap();
    assert_eq!(CALLS.with(std::cell::Cell::get), 1);
}

#[test]
fn dsfix_v204_curve_nearest_skips_distant_bounds_but_keeps_tolerance_edge() {
    thread_local! { static CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    fn nearest(path: &Path, _: Point) -> Result<Option<Nearest>> {
        CALLS.with(|n| n.set(n.get() + 1));
        Ok(Some((0, 0, 0., path.subpaths[0].knots[0].anchor, 0.)))
    }
    fn bounds(path: &Path) -> Result<Option<Bounds>> {
        Ok(path.control_bounds())
    }
    let mut doc = document();
    for i in 0..100 {
        let x = f64::from(i) * 100.;
        shape(&mut doc, [x, 50., x + 10., 60.]);
    }
    let cache = Cache::build(&doc, 0, Exclusions::default(), Some(Geometry { nearest, bounds })).unwrap();
    let settings = Settings { tolerance_px: 8., ..only(Kind::Curve) };
    let result = cache.point([-8., 50.], &settings, View::default(), None, false).unwrap();
    assert_eq!(result.point, [0., 50.]);
    assert_eq!(CALLS.with(|n| n.get()), 1);
    cache.point([-8.01, 50.], &settings, View::default(), None, false).unwrap();
    assert_eq!(CALLS.with(|n| n.get()), 1);
}
