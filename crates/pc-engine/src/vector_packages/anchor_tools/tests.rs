//! GPL-3.0-or-later synthetic fixtures and independent native/analytic geometry oracles.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{
    Color, ColorMode, Knot, Layer, SampleType, ShapeLayer, Size, Subpath,
    vector_design::drawing::{Corner, CornerKind, KnotCorner},
};
use std::sync::Arc;
fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}
fn doc() -> Document {
    Document::new("Anchor oracles", Size::new(256, 256), ColorMode::Rgb, SampleType::U8)
}
fn session(path: Path) -> (Session, LayerId) {
    let mut doc = doc();
    let mut sh = ShapeLayer { path, stroke: Some(ShapeStroke { width: 2., ..Default::default() }), ..Default::default() };
    crate::vector_cmds::refresh_shape(&doc, &mut sh);
    let l = Layer::new("Curve", LayerContent::Shape(sh));
    let id = l.id;
    doc.layers.push(l);
    let mut s = Session::new();
    s.add_document(doc, None);
    s.select_layer(id).unwrap();
    (s, id)
}
fn target(id: LayerId) -> node::Target {
    node::Target { name: "layer".into(), layer: Some(id.0), anchors: Vec::new() }
}
fn curve() -> Path {
    let mut sp = Subpath::polyline(&[(20., 80.), (230., 100.)]);
    sp.knots[0].out_ctrl = p(40., -30.);
    sp.knots[1].in_ctrl = p(190., 230.);
    Path::new(vec![sp])
}
fn eval(s: [Point; 4], t: f64) -> Point {
    let u = 1. - t;
    p(
        u * u * u * s[0].x + 3. * u * u * t * s[1].x + 3. * u * t * t * s[2].x + t * t * t * s[3].x,
        u * u * u * s[0].y + 3. * u * u * t * s[1].y + 3. * u * t * t * s[2].y + t * t * t * s[3].y,
    )
}
fn circle() -> Path {
    let step = std::f64::consts::TAU / 8.;
    let h = 4. / 3. * (step / 4.).tan() * 100.;
    let knots = (0..8)
        .map(|i| {
            let theta = i as f64 * step;
            let a = p(128. + 100. * theta.cos(), 128. + 100. * theta.sin());
            let d = p(-theta.sin() * h, theta.cos() * h);
            Knot::smooth(a, p(a.x - d.x, a.y - d.y), p(a.x + d.x, a.y + d.y))
        })
        .collect();
    Path::new(vec![Subpath { knots, closed: true, ..Default::default() }])
}
#[test]
fn anchor33_insert_oracle_every_sample_is_unchanged_and_one_undo() {
    let original = curve();
    let cubic = original.subpaths[0].segments()[0];
    for t in [0.001, 0.12, 0.37, 0.5, 0.92, 0.999] {
        let (s, id) = session(original.clone());
        let g =
            AnchorGesture { capture: Capture::begin(&s, vec![target(id)], false).unwrap(), target: 0, operation: node::Operation::Insert { knot: [0, 0], t } };
        let preview = g.preview(&s).unwrap();
        let LayerContent::Shape(sh) = &preview.layer(id).unwrap().content else { panic!() };
        let segments = sh.path.subpaths[0].segments();
        for i in 0..=1000 {
            let u = i as f64 / 1000.;
            let sample = if u <= t { eval(segments[0], u / t) } else { eval(segments[1], (u - t) / (1. - t)) };
            assert!((sample.x - eval(cubic, u).x).hypot(sample.y - eval(cubic, u).y) < 1e-6);
        }
    }
    let (mut s, id) = session(original);
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    let g =
        AnchorGesture { capture: Capture::begin(&s, vec![target(id)], false).unwrap(), target: 0, operation: node::Operation::Insert { knot: [0, 0], t: 0.5 } };
    let preview = g.preview(&s).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    g.commit(&mut s).unwrap();
    assert_eq!(*s.active().unwrap().doc, preview);
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().history.past_len(), history + 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn anchor33_delete_eight_node_circle_hausdorff_below_half_pixel() {
    let original = circle();
    for k in 0..8 {
        let (out, _) = node::evaluate(&original, &[[0, k]], &node::Operation::Delete).unwrap();
        assert_eq!(out.subpaths[0].knots.len(), 7);
        let mut max: f64 = 0.;
        for (a, b) in [(&original, &out), (&out, &original)] {
            for seg in a.subpaths[0].segments() {
                for i in 0..=256 {
                    let q = eval(seg, i as f64 / 256.);
                    let distance = photocraft_pathops::nearest(b, q).unwrap().unwrap().4;
                    max = max.max(distance);
                }
            }
        }
        assert!(max <= 0.5, "removed {k}: Hausdorff {max}");
    }
}
#[test]
fn anchor33_random_topology_keeps_finite_paths_and_corner_identities() {
    let (mut s, id) = session(circle());
    let l = Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap();
    l.vector.corners = (0..8).map(|k| KnotCorner { subpath: 0, knot: k, corner: Corner { radius: k as f64 / 10., kind: CornerKind::Round } }).collect();
    let mut shown = (*s.active().unwrap().doc).clone();
    let mut seed = 0x331234_u64;
    for i in 0..256 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let old = shown.layer(id).unwrap();
        let LayerContent::Shape(sh) = &old.content else { panic!() };
        let n = sh.path.subpaths[0].knots.len();
        let k = (seed as usize) % n;
        let mut t = target(id);
        let op = if i % 2 == 0 || n < 5 {
            node::Operation::Insert { knot: [0, k], t: 0.2 + (seed % 600) as f64 / 1000. }
        } else {
            t.anchors = vec![[0, k]];
            node::Operation::Delete
        };
        let next = node::prepare(&shown, &[t], &op, false).unwrap().doc;
        let l = next.layer(id).unwrap();
        let LayerContent::Shape(sh) = &l.content else { panic!() };
        edit::validate(&sh.path).unwrap();
        for c in &l.vector.corners {
            let old_c = old.vector.corners.iter().find(|o| o.corner == c.corner).unwrap();
            let LayerContent::Shape(before) = &old.content else { panic!() };
            assert_eq!(sh.path.subpaths[c.subpath].knots[c.knot].anchor, before.path.subpaths[old_c.subpath].knots[old_c.knot].anchor);
        }
        shown = next;
    }
}
#[test]
fn anchor33_convert_drag_handle_break_and_bend_oracles() {
    let base = Path::new(vec![Subpath::polyline(&[(20., 20.), (100., 50.), (200., 20.)])]);
    let (s, id) = session(base);
    let capture = Capture::begin(&s, vec![target(id)], false).unwrap();
    let g = AnchorGesture { capture, target: 0, operation: node::Operation::Convert { knot: [0, 1], out: Some([120., 80.]) } };
    let shown = g.preview(&s).unwrap();
    let LayerContent::Shape(sh) = &shown.layer(id).unwrap().content else { panic!() };
    let k = sh.path.subpaths[0].knots[1];
    assert!(k.smooth);
    assert_eq!(k.in_ctrl, p(80., 20.));
    assert_eq!(k.out_ctrl, p(120., 80.));
    let (out, _) = node::evaluate(&sh.path, &[], &node::Operation::Handle { knot: [0, 1], handle: "in".into(), to: [70., 40.], independent: true }).unwrap();
    assert!(!out.subpaths[0].knots[1].smooth);
    assert_eq!(out.subpaths[0].knots[1].out_ctrl, k.out_ctrl);
    let before = eval(out.subpaths[0].segments()[0], 0.5);
    let (out, _) = node::evaluate(&out, &[], &node::Operation::Bend { knot: [0, 0], t: 0.5, delta: [0., 12.] }).unwrap();
    let after = eval(out.subpaths[0].segments()[0], 0.5);
    assert!(distance(before, p(after.x, after.y - 12.)) < 1e-9);
}
fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
#[test]
fn anchor33_selected_hover_stale_and_invalid_last_target_are_atomic() {
    let (mut s, id) = session(curve());
    let ts = vec![target(id)];
    let q = eval(curve().subpaths[0].segments()[0], 0.37);
    let hit = segment_under(&s.active().unwrap().doc, &ts, q, 1.).unwrap().unwrap();
    assert!((hit.t - 0.37).abs() < 1e-5);
    assert_eq!(node_under(&s.active().unwrap().doc, &ts, p(20., 80.), 1.).unwrap().unwrap().knot, [0, 0]);
    assert!(segment_under(&s.active().unwrap().doc, &ts, p(20., 80.), 1.).unwrap().is_none());
    assert!(segment_under(&s.active().unwrap().doc, &ts, p(f64::NAN, 0.), 1.).is_err());
    let g = AnchorGesture { capture: Capture::begin(&s, ts, false).unwrap(), target: 0, operation: node::Operation::Insert { knot: [0, 0], t: 0.5 } };
    s.edit("Unrelated", |d, _| {
        d.name = "Changed".into();
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    assert!(g.commit(&mut s).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert!(Capture::begin(&s, vec![target(id), target(LayerId(u64::MAX))], false).is_err());
}
fn curvature_request(s: &Session, destination: Destination) -> CurvatureCommit {
    let st = s.active().unwrap();
    CurvatureCommit {
        document: st.doc.id.0,
        revision: st.revision,
        destination,
        targets: Vec::new(),
        points: vec![(20., 50.), (80., 100.), (140., 40.), (220., 150.)]
            .into_iter()
            .map(|(x, y)| CurvaturePoint { at: [x, y], corner: false, origin: None })
            .collect(),
        closed: false,
        convert_live: false,
        fill: Some(Fill::Solid(Color::BLACK)),
        stroke: Some(Default::default()),
    }
}
#[test]
fn anchor33_curvature_new_preview_commit_and_existing_corner_remap() {
    let (mut s, id) = session(Path::new(vec![Subpath::polyline(&[(20., 50.), (100., 120.), (220., 50.)])]));
    let before = s.active().unwrap().doc.clone();
    let c = curvature_request(&s, Destination::NewShape { above: Some(id) });
    let preview = preview_curvature(&s, &c).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    let pixels = photocraft_compose::flatten(&preview);
    commit_curvature(&mut s, c).unwrap();
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc), pixels);
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    let l = Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap();
    l.vector.corners = vec![KnotCorner { subpath: 0, knot: 1, corner: Corner { radius: 2., kind: CornerKind::Round } }];
    let mut c = curvature_request(&s, Destination::Curve { target: target(id), subpath: 0 });
    if let Destination::Curve { target, .. } = &mut c.destination {
        target.anchors = vec![[0, 1]];
    }
    c.points[0].origin = Some(0);
    c.points[2].origin = Some(1);
    c.points[2].corner = true;
    let preview = preview_curvature(&s, &c).unwrap();
    assert_eq!(preview.layer(id).unwrap().vector.corners[0].knot, 2);
    // Exercise the module's CommandSpec transaction directly. This proves history atomicity,
    // not global journal registration.
    let history = s.active().unwrap().history.past_len();
    let result = (specs()[0].run)(&mut s, &serde_json::to_value(&c).unwrap()).unwrap();
    assert_eq!(result["targets"][0]["anchors"], json!([[0, 2]]));
    assert_eq!(*s.active().unwrap().doc, preview);
    assert_eq!(s.active().unwrap().history.past_len(), history + 1);
    let before = s.active().unwrap().doc.clone();
    c.revision = s.active().unwrap().revision;
    c.points[2].origin = Some(0);
    assert!(preview_curvature(&s, &c).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn anchor33_live_conversion_locked_targets_and_bad_numbers_reject_without_publication() {
    let (mut s, id) = session(curve());
    let l = Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap();
    let LayerContent::Shape(sh) = &mut l.content else { panic!() };
    sh.live = Some(photocraft_doc::LiveShape::Ellipse { rect: [20., 20., 200., 100.] });
    let before = s.active().unwrap().doc.clone();
    assert!(Capture::begin(&s, vec![target(id)], false).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    let g =
        AnchorGesture { capture: Capture::begin(&s, vec![target(id)], true).unwrap(), target: 0, operation: node::Operation::Insert { knot: [0, 0], t: 0.5 } };
    let shown = g.preview(&s).unwrap();
    let LayerContent::Shape(sh) = &shown.layer(id).unwrap().content else { panic!() };
    assert!(sh.live.is_none());
    g.commit(&mut s).unwrap();
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap().locks.pixels = true;
    assert!(Capture::begin(&s, vec![target(id)], true).is_err());
    let before = s.active().unwrap().doc.clone();
    let mut c = curvature_request(&s, Destination::NewShape { above: Some(id) });
    c.points[0].at[0] = f64::NAN;
    assert!(preview_curvature(&s, &c).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn anchor33_curvature_delete_last_subpath_remaps_later_corner_refs_and_saved_paths() {
    let mut base = curve();
    base.subpaths.push(Subpath::polyline(&[(30., 10.), (100., 60.), (200., 10.)]));
    let (mut s, id) = session(base.clone());
    let l = Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap();
    l.vector.corners = vec![KnotCorner { subpath: 1, knot: 1, corner: Corner { radius: 2., kind: CornerKind::Round } }];
    let mut c = curvature_request(&s, Destination::Curve { target: target(id), subpath: 0 });
    c.points.clear();
    let preview = preview_curvature(&s, &c).unwrap();
    let l = preview.layer(id).unwrap();
    assert_eq!(l.vector.corners[0].subpath, 0);
    let LayerContent::Shape(sh) = &l.content else { panic!() };
    assert_eq!(sh.path.subpaths.len(), 1);
    assert_eq!(sh.path.subpaths[0], base.subpaths[1]);
    Arc::make_mut(&mut s.active_mut().unwrap().doc).work_path = Some(base.clone());
    c.destination = Destination::Curve { target: node::Target::default(), subpath: 0 };
    let shown = preview_curvature(&s, &c).unwrap();
    assert_eq!(shown.work_path.unwrap().subpaths.len(), 1);
    Arc::make_mut(&mut s.active_mut().unwrap().doc).paths.push(photocraft_doc::NamedPath { name: "Saved".into(), path: base, psd_raw: None });
    c.destination = Destination::Curve { target: node::Target { name: "Saved".into(), ..Default::default() }, subpath: 0 };
    let shown = preview_curvature(&s, &c).unwrap();
    assert_eq!(shown.paths[0].path.subpaths.len(), 1);
    assert!(s.journal.is_empty());
}

#[test]
fn anchor33_curvature_captured_selection_validates_last_target_and_remaps_only_edited_curve() {
    let (mut s, id) = session(curve());
    let mut edited = target(id);
    edited.anchors = vec![[0, 1]];
    let mut c = curvature_request(&s, Destination::Curve { target: edited.clone(), subpath: 0 });
    c.points[0].origin = Some(0);
    c.points[3].origin = Some(1);
    c.targets = vec![edited, node::Target { anchors: vec![[0, 0]], ..Default::default() }];
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let history = s.active().unwrap().history.past_len();
    assert!(preview_curvature(&s, &c).is_err()); // Missing work path is the last captured target.
    assert!((specs()[0].run)(&mut s, &serde_json::to_value(&c).unwrap()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, revision);
    assert_eq!(s.active().unwrap().history.past_len(), history);
    assert!(s.journal.is_empty());
    Arc::make_mut(&mut s.active_mut().unwrap().doc).work_path = Some(curve());
    let preview = preview_curvature(&s, &c).unwrap();
    let result = (specs()[0].run)(&mut s, &serde_json::to_value(&c).unwrap()).unwrap();
    assert_eq!(result["targets"][0]["anchors"], json!([[0, 3]]));
    assert_eq!(result["targets"][1]["anchors"], json!([[0, 0]]));
    assert_eq!(*s.active().unwrap().doc, preview);
}
