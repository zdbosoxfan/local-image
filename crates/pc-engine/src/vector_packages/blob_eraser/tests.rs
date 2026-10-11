#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{ColorMode, Knot, PathOp, SampleType, ShapeLayer, ShapeStroke, Size, Subpath, TextLayer};
use photocraft_geom::Point;
pub(super) fn session() -> Session {
    let mut s = Session::new();
    s.add_document(Document::new("Blob40", Size::new(160, 160), ColorMode::Rgb, SampleType::F32), None);
    s
}
fn sample(x: f64, y: f64) -> sweep::Sample {
    sweep::Sample { point: Point::new(x, y), pressure: 1. }
}
pub(super) fn request(s: &Session, mode: Mode, points: &[(f64, f64)], size: f64) -> Request {
    Request {
        capture: Capture::begin(s, mode, Options { tip: Tip { size, ..Default::default() }, ..Default::default() }).unwrap(),
        samples: points.iter().map(|&(x, y)| sample(x, y)).collect(),
        rectangle: false,
    }
}
pub(super) fn insert(s: &mut Session, path: Path, fill: bool) -> LayerId {
    let mut sh = ShapeLayer {
        path,
        fill: fill.then_some(Fill::Solid(Color::BLACK)),
        stroke: (!fill).then_some(ShapeStroke { width: 2., ..Default::default() }),
        ..Default::default()
    };
    crate::vector_cmds::refresh_shape(&s.active().unwrap().doc, &mut sh);
    let l = Layer::new("Shape", LayerContent::Shape(sh));
    let id = l.id;
    s.active_mut().unwrap().doc = Arc::new({
        let mut d = (*s.active().unwrap().doc).clone();
        d.layers.push(l);
        d
    });
    id
}
fn paths(doc: &Document) -> Vec<&Path> {
    doc.walk().into_iter().filter_map(|(_, _, l)| if let LayerContent::Shape(sh) = &l.content { Some(&sh.path) } else { None }).collect()
}
fn area(doc: &Document) -> f64 {
    paths(doc).into_iter().map(|p| ops::area(p).unwrap()).sum()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < b.abs() * 0.005 + 1e-5, "{a} != {b}");
}
#[test]
fn blob40_straight_area_fill_uses_stroke_color_merge_and_different_color() {
    let mut s = session();
    s.tools.foreground = [0.7, 0.2, 0.1, 1.];
    let before = s.active().unwrap().doc.clone();
    let p = prepare(&s, &request(&s, Mode::Blob, &[(20., 40.), (100., 40.)], 10.)).unwrap();
    near(area(&p.document), 800. + std::f64::consts::PI * 25.);
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
    assert!(s.journal.is_empty());
    p.commit(&mut s).unwrap();
    assert_eq!(s.journal.len(), 1);
    assert_eq!(paths(&s.active().unwrap().doc).len(), 1);
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layers[0].content else { panic!() };
    assert_eq!(sh.fill, Some(paint([0.7, 0.2, 0.1, 1.])));
    assert!(sh.stroke.is_none());
    prepare(&s, &request(&s, Mode::Blob, &[(50., 40.), (130., 40.)], 10.)).unwrap().commit(&mut s).unwrap();
    assert_eq!(paths(&s.active().unwrap().doc).len(), 1);
    near(area(&s.active().unwrap().doc), 1100. + std::f64::consts::PI * 25.);
    s.tools.foreground = [0.1, 0.2, 0.7, 1.];
    prepare(&s, &request(&s, Mode::Blob, &[(50., 40.), (100., 40.)], 10.)).unwrap().commit(&mut s).unwrap();
    assert_eq!(paths(&s.active().unwrap().doc).len(), 2);
    for _ in 0..3 {
        assert!(s.undo());
    }
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
}
#[test]
fn blob40_merge_only_selection_keep_selected_and_pressure() {
    let mut s = session();
    let mut r = request(&s, Mode::Blob, &[(30., 40.), (100., 40.)], 12.);
    r.capture.options.keep_selected = true;
    prepare(&s, &r).unwrap().commit(&mut s).unwrap();
    let first = s.active().unwrap().active_layer.unwrap();
    let mut r = request(&s, Mode::Blob, &[(30., 100.), (100., 100.)], 12.);
    r.capture.options.keep_selected = true;
    prepare(&s, &r).unwrap().commit(&mut s).unwrap();
    let second = s.active().unwrap().active_layer.unwrap();
    let mut r = request(&s, Mode::Blob, &[(60., 20.), (60., 120.)], 12.);
    r.capture.targets = vec![second];
    r.capture.options.merge_only_selection = true;
    r.capture.options.keep_selected = true;
    prepare(&s, &r).unwrap().commit(&mut s).unwrap();
    assert_eq!(paths(&s.active().unwrap().doc).len(), 2);
    assert!(s.active().unwrap().doc.layer(first).is_some());
    assert_eq!(s.active().unwrap().selected_layers(), vec![second]);
    let mut p = session();
    let mut r = request(&p, Mode::Blob, &[(20., 40.), (100., 40.)], 20.);
    r.capture.options.tip.pressure = 1.;
    for a in &mut r.samples {
        a.pressure = 0.5;
    }
    let half = prepare(&p, &r).unwrap();
    near(area(&half.document), 800. + std::f64::consts::PI * 25.);
    half.commit(&mut p).unwrap();
    assert_eq!(p.journal.len(), 1);
    let p = session();
    let mut r = request(&p, Mode::Blob, &[(20., 40.), (20., 40.), (100., 40.)], 20.);
    r.capture.options.tip.pressure = 1.;
    r.samples[1].pressure = 0.;
    r.samples[2].pressure = 0.;
    let dab = prepare(&p, &r).unwrap();
    near(area(&dab.document), std::f64::consts::PI * 100.);
}
#[test]
fn blob40_eraser_rectangle_split_area_live_conversion_preview_and_undo() {
    let mut s = session();
    let id = insert(&mut s, photocraft_vector::shapes::rect(20., 20., 100., 100.), true);
    s.select_layer(id).unwrap();
    let before = s.active().unwrap().doc.clone();
    let p = prepare(&s, &request(&s, Mode::Eraser, &[(70., 0.), (70., 150.)], 10.)).unwrap();
    assert_eq!(paths(&p.document).len(), 2);
    near(area(&p.document), 9000.);
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    let expected = photocraft_compose::flatten(&p.document);
    p.commit(&mut s).unwrap();
    assert_eq!(s.journal.len(), 1);
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc), expected);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    let mut r = request(&s, Mode::Eraser, &[(60., 10.), (80., 130.)], 10.);
    r.rectangle = true;
    let p = prepare(&s, &r).unwrap();
    near(area(&p.document), 8000.);
    assert_eq!(paths(&p.document).len(), 2);
    p.commit(&mut s).unwrap();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    s.active_mut().unwrap().doc = Arc::new({
        let mut d = (*before).clone();
        let LayerContent::Shape(sh) = &mut d.layer_mut(id).unwrap().content else { panic!() };
        sh.live = Some(photocraft_doc::LiveShape::Ellipse { rect: [20., 20., 100., 100.] });
        d
    });
    let p = prepare(&s, &request(&s, Mode::Eraser, &[(70., 0.), (70., 150.)], 10.)).unwrap();
    for (_, _, l) in p.document.walk() {
        if let LayerContent::Shape(sh) = &l.content {
            assert!(sh.live.is_none());
            assert!(l.vector.live_shape.is_none());
        }
    }
}
#[test]
fn blob40_compound_hole_preserved_and_independent_coverage_difference() {
    let mut s = session();
    let mut path = photocraft_vector::shapes::rect(10., 10., 120., 120.);
    let mut hole = Subpath::polygon(&[(50., 50.), (50., 90.), (90., 90.), (90., 50.)]);
    hole.op = PathOp::Join;
    path.subpaths.push(hole);
    insert(&mut s, path.clone(), true);
    let p = prepare(&s, &request(&s, Mode::Eraser, &[(10., 25.), (140., 25.)], 10.)).unwrap();
    near(area(&p.document), 14400. - 1600. - 1200.);
    assert_eq!(paths(&p.document).len(), 2);
    let mask = photocraft_vector::path_coverage(&path, p.document.bounds());
    let erase = photocraft_vector::path_coverage(&p.region, p.document.bounds());
    let result = photocraft_compose::flatten(&p.document);
    // Source and sweep are rasterized independently of geometric subtraction.
    for ((a, b), out) in mask.iter().zip(erase).zip(&result.px) {
        assert!((out[3] - a * (1. - b)).abs() <= 1. / 255.);
    }
    assert_eq!(result.get(70, 70)[3], 0.);
}
#[test]
fn blob40_open_line_cuts_narrow_crossing_and_fully_contained_path() {
    let mut s = session();
    insert(&mut s, Path::new(vec![Subpath::polyline(&[(20., 70.), (120., 70.)])]), false);
    let p = prepare(&s, &request(&s, Mode::Eraser, &[(70., 0.), (70., 150.)], 10.)).unwrap();
    let path_list = paths(&p.document);
    assert_eq!(path_list.len(), 2);
    let spans: Vec<_> = path_list
        .iter()
        .map(|p| {
            let k = &p.subpaths[0].knots;
            (k[0].anchor.x, k[k.len() - 1].anchor.x)
        })
        .collect();
    assert!(spans.contains(&(20., 65.)));
    assert!(spans.contains(&(75., 120.)));
    let mut r = request(&s, Mode::Eraser, &[(66.013, 60.), (66.033, 80.)], 1.);
    r.rectangle = true;
    let p = prepare(&s, &r).unwrap();
    assert_eq!(paths(&p.document).len(), 2);
    let mut r = request(&s, Mode::Eraser, &[(0., 0.), (150., 150.)], 10.);
    r.rectangle = true;
    let p = prepare(&s, &r).unwrap();
    assert!(p.changed);
    assert!(paths(&p.document).is_empty());
    p.commit(&mut s).unwrap();
    assert!(s.active().unwrap().doc.layers.is_empty());
}
#[test]
fn blob40_open_cubic_cut_retains_outside_geometry() {
    let mut s = session();
    let a = Point::new(20., 70.);
    let b = Point::new(120., 70.);
    let path = Path::new(vec![Subpath {
        knots: vec![
            Knot { anchor: a, in_ctrl: a, out_ctrl: Point::new(40., 20.), smooth: false },
            Knot { anchor: b, in_ctrl: Point::new(100., 120.), out_ctrl: b, smooth: false },
        ],
        ..Default::default()
    }]);
    insert(&mut s, path.clone(), false);
    let mut r = request(&s, Mode::Eraser, &[(65., 0.), (75., 160.)], 10.);
    r.rectangle = true;
    let p = prepare(&s, &r).unwrap();
    assert_eq!(paths(&p.document).len(), 2);
    // Exact subsegments still lie on the original cubic, checked by V1 nearest.
    for path_out in paths(&p.document) {
        for poly in photocraft_vector::flatten_path(path_out, 0.1) {
            for (x, y) in poly.pts {
                let nearest = ops::nearest(&path, Point::new(x, y)).unwrap().unwrap();
                assert!(nearest.4 < 0.2);
            }
        }
    }
}
#[test]
fn blob40_stale_locked_whole_set_exclusions_noops_and_legacy_options() {
    assert_eq!(serde_json::from_str::<Options>("{}").unwrap(), Options::default());
    let mut s = session();
    let id = insert(&mut s, photocraft_vector::shapes::rect(20., 20., 100., 100.), true);
    let r = request(&s, Mode::Eraser, &[(70., 0.), (70., 150.)], 10.);
    let before = s.active().unwrap().doc.clone();
    s.active_mut().unwrap().revision += 1;
    assert!(prepare(&s, &r).is_err());
    s.active_mut().unwrap().revision -= 1;
    s.active_mut().unwrap().doc = Arc::new({
        let mut d = (*before).clone();
        d.layer_mut(id).unwrap().locks.all = true;
        d
    });
    assert!(prepare(&s, &r).is_err());
    assert!(s.journal.is_empty());
    let all = request(&s, Mode::Eraser, &[(70., 0.), (70., 150.)], 10.);
    assert!(all.capture.targets.is_empty());
    let p = prepare(&s, &all).unwrap();
    assert!(!p.changed);
    p.commit(&mut s).unwrap();
    assert!(s.journal.is_empty());
    s.active_mut().unwrap().doc = Arc::new({
        let mut d = (*before).clone();
        d.layers.push(Layer::new("Text", LayerContent::Text(TextLayer::default())));
        d
    });
    assert!(Capture::begin(&s, Mode::Eraser, Options::default()).unwrap().excluded);
    let mut invalid = r.clone();
    invalid.capture.targets.push(LayerId(999999));
    assert!(prepare(&s, &invalid).is_err());
    let mut invalid = r.clone();
    invalid.capture.options.tip.size = f64::NAN;
    assert!(prepare(&s, &invalid).is_err());
    assert_eq!(s.active().unwrap().revision, r.capture.revision);
    assert!(s.journal.is_empty());
}
