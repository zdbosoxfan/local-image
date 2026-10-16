//! Selected replays adapted from VectorCraft crates/tools/src/pen.rs,
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! MIT OR Apache-2.0; Local Image independent oracles/fixtures are GPL-3.0-or-later.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{
    Color, ColorMode, Knot, SampleType, Size, Subpath,
    vector_design::drawing::{Corner, CornerKind, KnotCorner},
};
use photocraft_geom::Point;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}
fn session() -> Session {
    let mut s = Session::new();
    s.add_document(Document::new("Pen", Size::new(128, 128), ColorMode::Rgb, SampleType::U8), None);
    s
}
fn begin(s: &Session, mode: Mode) -> Gesture {
    Gesture::begin(s, Target::NewShape { above: None }, mode, p(20., 20.), 4., Some(Fill::Solid(Color::BLACK)), None).unwrap()
}
fn click(g: &mut Gesture, at: Point) -> Outcome {
    g.press(at, Modifiers::default(), 4.).unwrap();
    g.release().unwrap()
}
fn publish(s: &mut Session, c: &Commit) {
    s.execute("path.penCommit", serde_json::to_value(c).unwrap()).unwrap();
}
fn path(s: &Session) -> &Path {
    let st = s.active().unwrap();
    let LayerContent::Shape(sh) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("shape") };
    &sh.path
}

#[test]
fn pen_replay_click_click_drag_close_is_one_edit_and_cache_matches_preview() {
    let mut s = session();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let history = s.active().unwrap().history.entries().len();
    let mut g = begin(&s, Mode::Pen);
    g.release().unwrap();
    click(&mut g, p(100., 20.));
    g.press(p(80., 90.), Modifiers::default(), 4.).unwrap();
    g.motion(p(50., 90.), Modifiers::default()).unwrap();
    g.release().unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    g.press(p(20., 20.), Modifiers::default(), 4.).unwrap();
    let preview = preview_document(&s, g.request()).unwrap();
    let Outcome::Finished(c) = g.release().unwrap() else { panic!("close") };
    let sp = &c.path.subpaths[0];
    assert!(sp.closed);
    assert_eq!(sp.knots.len(), 3);
    assert_eq!(sp.knots.iter().filter(|k| k.smooth).count(), 1);
    // Independent cubic midpoint and raster interior oracles.
    let seg = sp.segments()[1];
    let midpoint = p((seg[0].x + 3. * seg[1].x + 3. * seg[2].x + seg[3].x) / 8., (seg[0].y + 3. * seg[1].y + 3. * seg[2].y + seg[3].y) / 8.);
    assert_eq!(midpoint, p(101.25, 55.));
    let raster = photocraft_compose::flatten(&preview);
    let sample = |x: usize, y: usize| raster.px[y * 128 + x][3];
    assert!(sample(50, 40) > 0.99);
    assert_eq!(sample(10, 10), 0.);
    publish(&mut s, &c);
    assert_eq!(path(&s), &c.path);
    assert_eq!(s.active().unwrap().revision, revision + 1);
    assert_eq!(s.active().unwrap().history.entries().len(), history + 1);
    assert_eq!(s.journal.len(), 1);
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc), raster);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
}

#[test]
fn pen_alt_mid_drag_freezes_incoming_and_shift_constrains_each_frame() {
    let s = session();
    let mut g = begin(&s, Mode::Pen);
    g.release().unwrap();
    g.press(p(60., 60.), Modifiers::default(), 4.).unwrap();
    g.motion(p(70., 60.), Modifiers::default()).unwrap();
    g.motion(p(80., 80.), Modifiers { alt: true, shift: false }).unwrap();
    let k = g.path().subpaths[0].knots[1];
    assert_eq!(k.in_ctrl, p(50., 60.));
    assert_eq!(k.out_ctrl, p(80., 80.));
    assert!(!k.smooth);
    g.motion(p(90., 68.), Modifiers { alt: true, shift: true }).unwrap();
    let k = g.path().subpaths[0].knots[1];
    assert_eq!(k.in_ctrl, p(50., 60.));
    assert_eq!(k.out_ctrl.y, 60.);
    g.release().unwrap();
    g.press(p(100., 72.), Modifiers { shift: true, alt: false }, 4.).unwrap();
    assert_eq!(g.path().subpaths[0].knots[2].anchor.y, 60.);
}

#[test]
fn pen_close_drag_alt_preserves_first_outgoing_and_close_click_preserves_handles() {
    let s = session();
    let mut g = begin(&s, Mode::Pen);
    g.motion(p(30., 20.), Modifiers::default()).unwrap();
    g.release().unwrap();
    click(&mut g, p(80., 20.));
    click(&mut g, p(80., 80.));
    let first = g.path().subpaths[0].knots[0];
    g.press(p(21., 21.), Modifiers::default(), 4.).unwrap();
    g.motion(p(21., 21.), Modifiers::default()).unwrap();
    assert_eq!(g.path().subpaths[0].knots[0], first);
    g.motion(p(20., 40.), Modifiers { alt: true, shift: false }).unwrap();
    let k = g.path().subpaths[0].knots[0];
    assert_eq!(k.out_ctrl, first.out_ctrl);
    assert_eq!(k.in_ctrl, p(20., 0.));
    assert!(!k.smooth);
    assert!(matches!(g.release().unwrap(), Outcome::Finished(_)));
}

#[test]
fn pen_modes_smart_flows_polygon_stays_sharp_line_self_terminates() {
    let s = session();
    for mode in [Mode::Smart, Mode::Polygon, Mode::Line] {
        let mut g = begin(&s, mode);
        g.release().unwrap();
        g.press(p(80., 20.), Modifiers::default(), 4.).unwrap();
        g.motion(p(90., 40.), Modifiers::default()).unwrap();
        let out = g.release().unwrap();
        if mode == Mode::Line {
            assert!(matches!(out, Outcome::Finished(_)));
        } else {
            click(&mut g, p(80., 80.));
        }
        let sp = &g.path().subpaths[0];
        if mode == Mode::Smart {
            let k = sp.knots[1]; // Dragged node remains manual, auto nodes still flow.
            assert!(k.smooth);
            assert_eq!(k.in_ctrl, p(70., 0.));
            assert!(sp.knots[2].smooth);
        } else {
            assert!(sp.knots.iter().all(|k| !k.smooth && k.in_ctrl == k.anchor && k.out_ctrl == k.anchor));
        }
    }
    let mut g = begin(&s, Mode::Smart);
    g.release().unwrap();
    click(&mut g, p(80., 20.));
    click(&mut g, p(80., 80.));
    let k = g.path().subpaths[0].knots[1];
    assert_eq!(k.in_ctrl, p(70., 10.));
    assert_eq!(k.out_ctrl, p(90., 30.));
    assert_eq!((k.out_ctrl.x - k.anchor.x) * (k.in_ctrl.y - k.anchor.y) - (k.out_ctrl.y - k.anchor.y) * (k.in_ctrl.x - k.anchor.x), 0.);
}

fn existing(s: &mut Session) -> LayerId {
    let mut sh =
        ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(30., 30.), (60., 30.)])]), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    sh.path.subpaths[0].knots[1] = Knot::smooth(p(60., 30.), p(50., 30.), p(70., 30.));
    let mut l = Layer::new("Existing", LayerContent::Shape(sh));
    l.vector.corners.push(KnotCorner { subpath: 0, knot: 0, corner: Corner { radius: 2., kind: CornerKind::Round } });
    let id = l.id;
    s.edit("fixture", |doc, active| {
        doc.layers.push(l);
        *active = Some(id);
        Ok(())
    })
    .unwrap();
    id
}

#[test]
fn pen_continue_both_ends_preserves_endpoint_and_remaps_metadata_undo_is_exact() {
    for (at, next, first) in [(p(60., 30.), p(100., 50.), false), (p(30., 30.), p(10., 50.), true)] {
        let mut s = session();
        let id = existing(&mut s);
        let before = s.active().unwrap().doc.clone();
        let original = path(&s).clone();
        let mut g = Gesture::begin(&s, Target::Shape { layer: id }, Mode::Smart, at, 4., None, None).unwrap();
        assert_eq!(g.path(), &original);
        g.release().unwrap();
        click(&mut g, next);
        let c = g.finish().unwrap();
        publish(&mut s, &c);
        let old = &original.subpaths[0].knots;
        let new = &path(&s).subpaths[0].knots;
        assert_eq!(if first { &new[1..] } else { &new[..2] }, old);
        assert_eq!(s.active().unwrap().doc.layer(id).unwrap().vector.corners[0].knot, usize::from(first));
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
        if first {
            let mut g = Gesture::begin(&s, Target::Shape { layer: id }, Mode::Smart, at, 4., None, None).unwrap();
            g.release().unwrap();
            click(&mut g, next);
            g.press(p(60., 30.), Modifiers::default(), 4.).unwrap();
            g.motion(p(70., 40.), Modifiers::default()).unwrap();
            let Outcome::Finished(c) = g.release().unwrap() else { panic!("closing drag finishes") };
            publish(&mut s, &c);
            let l = s.active().unwrap().doc.layer(id).unwrap();
            assert_eq!(l.vector.corners[0].knot, 1);
            assert_eq!(path(&s).subpaths[0].knots[1], original.subpaths[0].knots[0]);
            assert!(s.undo());
            assert_eq!(s.active().unwrap().doc, before);
        }
    }
}

#[test]
fn pen_selected_curve_addition_preserves_existing_subpaths_and_style() {
    let mut s = session();
    let id = existing(&mut s);
    let original = path(&s).clone();
    let mut g = Gesture::begin(&s, Target::Shape { layer: id }, Mode::Polygon, p(20., 80.), 4., Some(Fill::Solid(Color::WHITE)), None).unwrap();
    g.release().unwrap();
    click(&mut g, p(70., 80.));
    publish(&mut s, &g.finish().unwrap());
    assert_eq!(path(&s).subpaths.len(), 2);
    assert_eq!(path(&s).subpaths[0], original.subpaths[0]);
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(id).unwrap().content else { panic!("shape") };
    assert_eq!(sh.fill, Some(Fill::Solid(Color::BLACK)));
}

#[test]
fn pen_failures_are_atomic_stale_wrong_document_locks_missing_and_invalid_numbers() {
    let mut s = session();
    let mut g = begin(&s, Mode::Pen);
    g.release().unwrap();
    click(&mut g, p(100., 20.));
    let c = g.finish().unwrap();
    for bad in [
        Commit { revision: c.revision + 1, ..c.clone() },
        Commit { document: c.document + 1, ..c.clone() },
        Commit { target: Target::Shape { layer: LayerId(u64::MAX) }, ..c.clone() },
        Commit { stroke: Some(ShapeStroke { width: 5001., ..Default::default() }), ..c.clone() },
    ] {
        let before = s.active().unwrap().doc.clone();
        assert!(s.execute("path.penCommit", serde_json::to_value(bad).unwrap()).is_err());
        assert_eq!(s.active().unwrap().doc, before);
        assert!(s.journal.is_empty());
        assert!(!s.undo());
    }
    for n in [f64::NAN, f64::INFINITY, MAX_COORDINATE + 1.] {
        assert!(g.motion(p(n, 0.), Modifiers::default()).is_err());
        let mut path = c.path.clone();
        path.subpaths[0].knots[0].anchor.x = n;
        assert!(validate(&path).is_err());
    }
    let id = existing(&mut s);
    let mut doc = (*s.active().unwrap().doc).clone();
    let child = doc.layers.remove(0);
    let mut parent = Layer::group("Locked", vec![child]);
    parent.locks.all = true;
    doc.layers.push(parent);
    s.active_mut().unwrap().doc = std::sync::Arc::new(doc);
    assert!(Gesture::begin(&s, Target::Shape { layer: id }, Mode::Pen, p(60., 30.), 4., None, None).is_err());
    assert!(Gesture::begin(&s, Target::NewShape { above: Some(id) }, Mode::Pen, p(60., 30.), 4., None, None).is_err());
}

#[test]
fn pen_rubber_band_is_detached_and_random_replays_remain_finite() {
    let s = session();
    for seed in 0..256u64 {
        let mut rng = seed + 1;
        let mut g = begin(&s, Mode::Pen);
        g.release().unwrap();
        for _ in 0..16 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let at = p(30. + ((rng >> 32) % 80) as f64, 30. + ((rng >> 40) % 80) as f64);
            g.press(at, Modifiers { alt: rng & 1 != 0, shift: rng & 2 != 0 }, 0.).unwrap();
            g.motion(p(at.x + 3., at.y + 7.), Modifiers { alt: rng & 4 != 0, shift: rng & 8 != 0 }).unwrap();
            g.release().unwrap();
            validate(g.path()).unwrap();
        }
        let before = g.path().clone();
        g.motion(p(110., 100.), Modifiers::default()).unwrap();
        assert!(g.rubber_band(Modifiers::default()).is_some());
        assert_eq!(g.path(), &before);
    }
}

#[test]
fn pen_work_and_vector_mask_destinations_are_guarded_and_undoable() {
    for mask in [false, true] {
        let mut s = session();
        let target = if mask {
            let mut l = Layer::raster("Mask", s.active().unwrap().doc.pixel_format());
            l.vector_mask = Some(photocraft_doc::VectorMask::new(Path::default()));
            let id = l.id;
            s.edit("fixture", |doc, active| {
                doc.layers.push(l);
                *active = Some(id);
                Ok(())
            })
            .unwrap();
            Target::VectorMask { layer: id }
        } else {
            Target::Work
        };
        let before = s.active().unwrap().doc.clone();
        let mut g = Gesture::begin(&s, target.clone(), Mode::Pen, p(10., 10.), 4., None, None).unwrap();
        g.release().unwrap();
        click(&mut g, p(60., 10.));
        let c = g.finish().unwrap();
        let preview = preview_document(&s, &c).unwrap();
        assert_eq!(s.active().unwrap().doc, before);
        publish(&mut s, &c);
        assert_eq!(*s.active().unwrap().doc, preview);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
        if let Target::VectorMask { layer } = target {
            s.edit("Lock", |doc, _| {
                doc.layer_mut(layer).unwrap().locks.pixels = true;
                Ok(())
            })
            .unwrap();
            let mut c = c;
            c.revision = s.active().unwrap().revision;
            let before = s.active().unwrap().doc.clone();
            let journal = s.journal.len();
            let revision = s.active().unwrap().revision;
            assert!(s.execute("path.penCommit", serde_json::to_value(c).unwrap()).is_err());
            assert_eq!(s.active().unwrap().doc, before);
            assert_eq!(s.active().unwrap().revision, revision);
            assert_eq!(s.journal.len(), journal);
        }
    }
}

#[test]
fn pen_bounds_budget_paint_and_lone_node_rejections() {
    let mut s = session();
    let mut g = begin(&s, Mode::Pen);
    assert!(g.finish().is_err());
    g.release().unwrap();
    click(&mut g, p(100., 20.));
    let mut c = g.finish().unwrap();
    let preview = preview_document(&s, &c).unwrap();
    let LayerContent::Shape(sh) = &preview.layers[0].content else { panic!("shape") };
    let bounds = sh.cache.as_ref().unwrap().content_bounds();
    assert!(bounds.width() < 128 && bounds.height() < 128);
    c.stroke = Some(ShapeStroke { width: f32::NAN, ..Default::default() });
    assert!(preview_document(&s, &c).is_err());
    c.stroke = None;
    c.path.subpaths[0].knots.resize(MAX_KNOTS + 1, Knot::corner(30., 30.));
    assert!(s.execute("path.penCommit", serde_json::to_value(&c).unwrap()).is_err());
    assert!(s.journal.is_empty());
    assert!(Gesture::begin(&s, Target::Work, Mode::Pen, p(1., 1.), f64::NAN, None, None).is_err());
}

#[path = "gpu.rs"]
mod gpu;

#[test]
fn pen_addition_keeps_distinct_metadata_on_equal_duplicate_knots() {
    let mut s = session();
    let id = existing(&mut s);
    s.edit("duplicate fixture", |doc, _| {
        let l = doc.layer_mut(id).unwrap();
        let LayerContent::Shape(sh) = &mut l.content else { panic!("shape") };
        let duplicate = sh.path.subpaths[0].knots[0];
        sh.path.subpaths[0].knots.insert(1, duplicate);
        l.vector.corners[0].knot = 1;
        Ok(())
    })
    .unwrap();
    let mut g = Gesture::begin(&s, Target::Shape { layer: id }, Mode::Pen, p(20., 80.), 4., None, None).unwrap();
    g.release().unwrap();
    click(&mut g, p(80., 80.));
    publish(&mut s, &g.finish().unwrap());
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().vector.corners[0].knot, 1);
}

#[test]
fn pen_line_drag_self_terminates_with_shift_and_no_handles() {
    let s = session();
    let mut g = begin(&s, Mode::Line);
    g.motion(p(70., 31.), Modifiers { shift: true, alt: false }).unwrap();
    let Outcome::Finished(c) = g.release().unwrap() else { panic!("line drag completes on release") };
    let sp = &c.path.subpaths[0];
    assert_eq!(sp.knots.len(), 2);
    assert!(!sp.closed);
    assert_eq!(sp.knots[1].anchor.y, 20.);
    assert!(sp.knots.iter().all(|k| !k.smooth && k.in_ctrl == k.anchor && k.out_ctrl == k.anchor));
}

#[test]
fn review_followup_pen_new_subpath_cannot_exceed_captured_knot_budget() {
    let mut s = session();
    let path = Path::new(vec![Subpath::polyline(&vec![(1., 1.); MAX_KNOTS])]);
    let layer = Layer::new("Full path", LayerContent::Shape(ShapeLayer { path, ..Default::default() }));
    let id = layer.id;
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).layers.push(layer);
    let before = s.active().unwrap().doc.clone();
    assert!(Gesture::begin(&s, Target::Shape { layer: id }, Mode::Pen, p(80., 80.), 1., None, None).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn review_followup_pen_zero_length_line_release_invalidates_preview_generation() {
    let s = session();
    let mut g = begin(&s, Mode::Line);
    g.release().unwrap();
    g.press(p(30., 30.), Modifiers::default(), 0.).unwrap();
    g.motion(p(20., 20.), Modifiers::default()).unwrap();
    let generation = g.generation();
    assert!(matches!(g.release().unwrap(), Outcome::Preview));
    assert_eq!(g.path().subpaths[0].knots.len(), 1);
    assert!(g.generation() > generation);
}

#[test]
fn dsfix_v187_begin_new_subpath_checks_knot_budget_but_allows_endpoint() {
    let mut s = session();
    let mut sp = Subpath::polyline(&[(10., 10.), (20., 10.)]);
    sp.knots.resize(MAX_KNOTS, Knot::corner(20., 10.));
    let original = Path::new(vec![sp]);
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).work_path = Some(original.clone());
    assert!(Gesture::begin(&s, Target::Work, Mode::Pen, p(90., 90.), 0., None, None).is_err());
    assert_eq!(s.active().unwrap().doc.work_path.as_ref(), Some(&original));
    assert!(Gesture::begin(&s, Target::Work, Mode::Pen, p(20., 10.), 0., None, None).is_ok());
}

#[test]
fn dsfix_v188_zero_length_line_release_changes_generation() {
    let s = session();
    let mut g = begin(&s, Mode::Line);
    g.motion(p(40., 20.), Modifiers::default()).unwrap();
    g.motion(p(20., 20.), Modifiers::default()).unwrap();
    assert_eq!(g.path().subpaths[0].knots.len(), 2);
    let generation = g.generation();
    assert_eq!(g.release().unwrap(), Outcome::Preview);
    assert_eq!(g.path().subpaths[0].knots.len(), 1);
    assert_eq!(g.generation(), generation.wrapping_add(1));
}
