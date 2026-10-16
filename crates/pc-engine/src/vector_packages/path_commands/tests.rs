#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Fill, SampleType, ShapeLayer, Size, Subpath, TextLayer};
use std::sync::Arc;
pub(crate) fn fixture(paths: Vec<Path>) -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Object paths 36", Size::new(180, 140), ColorMode::Rgb, SampleType::F32);
    let mut ids = Vec::new();
    for path in paths {
        let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
        crate::vector_cmds::refresh_shape(&doc, &mut sh);
        let l = Layer::new("Path", LayerContent::Shape(sh));
        ids.push(l.id);
        doc.layers.push(l);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, ids)
}
fn req(s: &Session, ids: &[LayerId], action: Action) -> Request {
    Request { capture: Capture::begin(s, ids).unwrap(), action }
}
fn call(s: &mut Session, ids: &[LayerId], action: Action) {
    let r = req(s, ids, action);
    s.execute("object.path.commit", json!(r)).unwrap();
}
fn path(s: &Session, id: LayerId) -> Path {
    shape(s.active().unwrap().doc.layer(id).unwrap()).unwrap().path.clone()
}
#[test]
fn object36_join_coincident_noncoincident_close_straight_and_one_undo() {
    for gap in [0., 7.] {
        let (mut s, ids) =
            fixture(vec![Path::new(vec![Subpath::polyline(&[(10., 10.), (30., 10.)])]), Path::new(vec![Subpath::polyline(&[(50., 20.), (30. + gap, 10.)])])]);
        let before = s.active().unwrap().doc.clone();
        call(&mut s, &ids, Action::Join { tolerance: 1e-6 });
        let p = path(&s, ids[0]);
        assert_eq!(p.subpaths.len(), 1);
        let sp = &p.subpaths[0];
        assert_eq!(sp.knots.len(), if gap == 0. { 3 } else { 4 });
        assert!(!sp.closed);
        if gap > 0. {
            let bridge = sp.segments()[1];
            assert_eq!(bridge[0], bridge[1]);
            assert_eq!(bridge[2], bridge[3]);
        }
        assert!(s.active().unwrap().doc.layer(ids[1]).is_none());
        assert_eq!(s.journal.len(), 1);
        assert_eq!(s.active().unwrap().history.past_len(), 1);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
    }
    let (mut s, ids) = fixture(vec![Path::new(vec![Subpath::polyline(&[(10., 10.), (30., 10.), (10., 10.)])])]);
    call(&mut s, &ids, Action::Join { tolerance: 1e-6 });
    let p = path(&s, ids[0]);
    assert!(p.subpaths[0].closed);
    assert_eq!(p.subpaths[0].knots.len(), 2);
}
#[test]
fn object36_average_global_centroid_axes_and_handles() {
    for axis in [Axis::Horizontal, Axis::Vertical, Axis::Both] {
        let (mut s, ids) =
            fixture(vec![Path::new(vec![Subpath::polyline(&[(10., 20.), (30., 40.)])]), Path::new(vec![Subpath::polyline(&[(50., 60.), (70., 80.)])])]);
        let anchors = ids.iter().map(|id| Anchors { layer: *id, points: vec![[0, 0], [0, 1]] }).collect();
        call(&mut s, &ids, Action::Average { axis, anchors });
        for id in &ids {
            for k in &path(&s, *id).subpaths[0].knots {
                if axis != Axis::Horizontal {
                    assert_eq!(k.anchor.x, 40.);
                }
                if axis != Axis::Vertical {
                    assert_eq!(k.anchor.y, 50.);
                }
                assert_eq!(k.anchor, k.in_ctrl);
                assert_eq!(k.anchor, k.out_ctrl);
            }
        }
    }
}
fn circle() -> Path {
    Path::new(vec![Subpath::polygon(
        &(0..100)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 100.;
                (70. + 40. * a.cos(), 70. + 40. * a.sin())
            })
            .collect::<Vec<_>>(),
    )])
}
fn line_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let d = (b.0 - a.0, b.1 - a.1);
    let l = d.0 * d.0 + d.1 * d.1;
    let t = if l == 0. { 0. } else { ((p.0 - a.0) * d.0 + (p.1 - a.1) * d.1) / l }.clamp(0., 1.);
    (p.0 - a.0 - t * d.0).hypot(p.1 - a.1 - t * d.1)
}
fn distance(p: (f64, f64), lines: &[ops::Polyline]) -> f64 {
    lines.iter().flat_map(|l| l.pts.windows(2).map(move |w| line_distance(p, w[0], w[1]))).fold(f64::INFINITY, f64::min)
}
#[test]
fn object36_simplify_circle_independent_bidirectional_hausdorff_preview_and_counts() {
    let original = circle();
    let (mut s, ids) = fixture(vec![original.clone()]);
    let before = s.active().unwrap().doc.clone();
    let r = req(&s, &ids, Action::Simplify { precision: 80., corner_angle: 30. });
    let prepared = prepare(&s, &r).unwrap();
    assert_eq!(prepared.original_points, 100);
    assert!(prepared.new_points <= 8, "{}", prepared.new_points);
    let simplified = &shape(prepared.document.layer(ids[0]).unwrap()).unwrap().path;
    let a = ops::flatten_path(&original, 0.002);
    let b = ops::flatten_path(simplified, 0.002);
    let max = a.iter().flat_map(|l| &l.pts).map(|p| distance(*p, &b)).chain(b.iter().flat_map(|l| &l.pts).map(|p| distance(*p, &a))).fold(0., f64::max);
    assert!(max <= precision_tolerance(80.) + 0.004, "Hausdorff={max}");
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    assert!(s.journal.is_empty());
    s.execute("object.path.commit", json!(r)).unwrap();
    assert_eq!(*s.active().unwrap().doc, prepared.document);
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn object36_add_midpoints_remove_selected_restores_curve_and_compound_ops() {
    let original = photocraft_vector::shapes::ellipse(20., 20., 80., 60.);
    let (mut s, ids) = fixture(vec![original.clone()]);
    call(&mut s, &ids, Action::AddAnchors);
    let added = path(&s, ids[0]);
    assert_eq!(added.subpaths[0].knots.len(), 8);
    let anchors = vec![Anchors { layer: ids[0], points: vec![[0, 1], [0, 3], [0, 5], [0, 7]] }];
    call(&mut s, &ids, Action::RemoveAnchors { anchors });
    let out = path(&s, ids[0]);
    assert_eq!(out.subpaths[0].knots.len(), 4);
    let a = ops::flatten_path(&original, 0.001);
    let b = ops::flatten_path(&out, 0.001);
    let max = a.iter().flat_map(|l| &l.pts).map(|p| distance(*p, &b)).fold(0., f64::max);
    assert!(max < 0.02, "curve refit error {max}");
    let compound = Path {
        subpaths: vec![
            Subpath::polygon(&[(0., 0.), (80., 0.), (80., 80.), (0., 80.)]),
            Subpath::polygon(&[(20., 20.), (20., 60.), (60., 60.), (60., 20.)]).with_op(photocraft_doc::PathOp::Join),
        ],
        fill_rule: photocraft_doc::FillRule::EvenOdd,
        inverted: false,
    };
    let (mut s, ids) = fixture(vec![compound.clone()]);
    call(&mut s, &ids, Action::AddAnchors);
    let out = path(&s, ids[0]);
    assert_eq!(out.fill_rule, compound.fill_rule);
    assert_eq!(out.subpaths[1].op, photocraft_doc::PathOp::Join);
    assert!((ops::area(&out).unwrap() - 4800.).abs() < 1e-6);
}
#[test]
fn object36_cleanup_exact_seeded_strays_unpainted_empty_text_and_one_undo() {
    let (mut s, mut ids) = fixture(vec![
        Path::new(vec![Subpath::polyline(&[(1., 1.)])]),
        photocraft_vector::shapes::rect(10., 10., 10., 10.),
        photocraft_vector::shapes::rect(30., 10., 10., 10.),
    ]);
    let mut doc = (*s.active().unwrap().doc).clone();
    if let LayerContent::Shape(sh) = &mut doc.layer_mut(ids[1]).unwrap().content {
        sh.fill = None;
    }
    let empty = Layer::new("Empty", LayerContent::Text(TextLayer::default()));
    ids.push(empty.id);
    doc.layers.push(empty);
    let nonempty = Layer::new("Nonempty text", LayerContent::Text(TextLayer { text: "A".into(), ..Default::default() }));
    let keep = nonempty.id;
    ids.push(keep);
    doc.layers.push(nonempty);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let before = s.active().unwrap().doc.clone();
    call(&mut s, &ids, Action::CleanUp { stray_points: true, unpainted: true, empty_text: true });
    let after = &s.active().unwrap().doc;
    assert_eq!(after.layers.len(), 2);
    assert!(after.layer(ids[2]).is_some());
    assert!(after.layer(keep).is_some());
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn object36_grid_exact_six_cells_ten_gutters_and_guides() {
    let (mut s, ids) = fixture(vec![photocraft_vector::shapes::rect(10., 20., 110., 80.)]);
    call(&mut s, &ids, Action::Grid { rows: 3, columns: 2, row_gutter: 10., column_gutter: 10., guides: true });
    let doc = &s.active().unwrap().doc;
    let children = doc.layer(ids[0]).unwrap().children().unwrap();
    assert_eq!(children.len(), 6);
    for (i, l) in children.iter().enumerate() {
        let x = 10. + (i % 2) as f64 * 60.;
        let y = 20. + (i / 2) as f64 * 30.;
        assert_eq!(ops::bounds(&shape(l).unwrap().path).unwrap(), Some((x, y, x + 50., y + 20.)));
    }
    assert_eq!(doc.guides.vertical, vec![10., 60., 70., 120.]);
    assert_eq!(doc.guides.horizontal, vec![20., 40., 50., 70., 80., 100.]);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
}
#[test]
fn object36_divide_below_conserves_area_and_consumes_only_cutter() {
    let (mut s, ids) = fixture(vec![photocraft_vector::shapes::rect(10., 10., 100., 80.), photocraft_vector::shapes::rect(50., 0., 20., 100.)]);
    call(&mut s, &ids[1..], Action::DivideBelow);
    let doc = &s.active().unwrap().doc;
    assert!(doc.layer(ids[1]).is_none());
    let children = doc.layer(ids[0]).unwrap().children().unwrap();
    assert_eq!(children.len(), 2);
    let a = &shape(&children[0]).unwrap().path;
    let b = &shape(&children[1]).unwrap().path;
    assert!((ops::area(a).unwrap() + ops::area(b).unwrap() - 8000.).abs() < 1e-5);
    assert!(ops::area(&ops::boolean(a, b, ops::BoolOp::Intersect).unwrap()).unwrap() < 1e-5);
    assert_eq!(s.journal.len(), 1);
}
#[test]
fn object36_invalid_last_target_locked_stale_numeric_and_anchor_set_are_atomic() {
    let (mut s, ids) = fixture(vec![circle(), circle()]);
    let before = s.active().unwrap().doc.clone();
    let base = req(&s, &ids, Action::AddAnchors);
    let mut invalid = base.clone();
    invalid.capture.layers[1] = LayerId(u64::MAX);
    assert!(s.execute("object.path.commit", json!(invalid)).is_err());
    let mut invalid = base.clone();
    invalid.capture.layers[1] = ids[0];
    assert!(s.execute("object.path.commit", json!(invalid)).is_err());
    let mut invalid = base.clone();
    invalid.capture.revision += 1;
    assert!(s.execute("object.path.commit", json!(invalid)).is_err());
    for action in [
        Action::Simplify { precision: 101., corner_angle: 30. },
        Action::Grid { rows: 0, columns: 2, row_gutter: 0., column_gutter: 0., guides: false },
        Action::RemoveAnchors { anchors: vec![Anchors { layer: ids[1], points: vec![[0, 1000]] }] },
    ] {
        let r = req(&s, &ids, action);
        assert!(s.execute("object.path.commit", json!(r)).is_err());
    }
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    assert!(s.journal.is_empty());
    assert_eq!(s.active().unwrap().history.past_len(), 0);
    let mut doc = (*before).clone();
    doc.layer_mut(ids[1]).unwrap().locks.pixels = true;
    s.active_mut().unwrap().doc = Arc::new(doc);
    assert!(s.execute("object.path.commit", json!(base)).is_err());
    assert!(s.journal.is_empty());
}
#[test]
fn object36_vector_clipboard_front_back_order_coordinates_native_and_undo() {
    let (mut s, ids) = fixture(vec![photocraft_vector::shapes::rect(10., 10., 20., 20.), photocraft_vector::shapes::rect(40., 10., 20., 20.)]);
    let before = s.active().unwrap().doc.clone();
    let clip = clipboard::Clipboard::copy(&s, &ids).unwrap();
    for front in [true, false] {
        let capture = Capture::begin(&s, &ids).unwrap();
        let out = clip.paste(&mut s, &capture, front).unwrap();
        let pasted: Vec<_> = out["layers"].as_array().unwrap().iter().map(|v| LayerId(v.as_u64().unwrap())).collect();
        let doc = &s.active().unwrap().doc;
        let order: Vec<_> = doc.layers.iter().map(|l| l.id).collect();
        assert_eq!(order, if front { vec![ids[0], ids[1], pasted[0], pasted[1]] } else { vec![pasted[0], pasted[1], ids[0], ids[1]] });
        for (original, copy) in ids.iter().zip(pasted) {
            assert_eq!(path(&s, *original), path(&s, copy));
        }
        let native = photocraft_format::save_to_bytes(doc, &Default::default()).unwrap();
        assert_eq!(photocraft_format::load_from_bytes(&native).unwrap(), **doc);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
    }
}

#[test]
fn object36_vector_clipboard_preserves_group_masks_effects_and_native_metadata() {
    let (mut s, ids) = fixture(vec![photocraft_vector::shapes::rect(10., 10., 30., 20.)]);
    let mut doc = (*s.active().unwrap().doc).clone();
    let mut child = doc.layers.remove(0);
    child.effects.enabled = true;
    child.effects.reference = Some((14., 18.));
    child.effects.items.push(photocraft_doc::effects::Effect::ColorOverlay {
        common: photocraft_doc::FxCommon::new(photocraft_color::BlendMode::Normal, 0.7),
        color: photocraft_doc::Color::rgb(0.2, 0.4, 0.6),
    });
    child.mask = Some(photocraft_doc::LayerMask::reveal_all());
    child.vector_mask = Some(photocraft_doc::VectorMask::new(photocraft_vector::shapes::rect(10., 10., 15., 20.)));
    let group = Layer::group("Vector group", vec![child]);
    let id = group.id;
    doc.layers = vec![group];
    s.active_mut().unwrap().doc = Arc::new(doc);
    let before = s.active().unwrap().doc.clone();
    let clip = clipboard::Clipboard::copy(&s, &[id]).unwrap();
    let capture = Capture::begin(&s, &[id]).unwrap();
    let out = clip.paste(&mut s, &capture, true).unwrap();
    let copy = LayerId(out["layers"][0].as_u64().unwrap());
    let original = s.active().unwrap().doc.layer(id).unwrap();
    let pasted = s.active().unwrap().doc.layer(copy).unwrap();
    assert_ne!(pasted.id, id);
    let a = &original.children().unwrap()[0];
    let b = &pasted.children().unwrap()[0];
    assert_ne!(a.id, b.id);
    assert_eq!(a.mask, b.mask);
    assert_eq!(a.vector_mask, b.vector_mask);
    assert_eq!(a.vector, b.vector);
    assert_eq!(a.effects, b.effects);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.active().unwrap().doc.layer(ids[0]).is_some());
}

#[test]
fn object36_selected_anchors_use_base_indices_before_live_corner_projection() {
    use photocraft_doc::vector_design::drawing::{Corner, CornerKind, KnotCorner};
    let (mut s, ids) = fixture(vec![photocraft_vector::shapes::rect(10., 10., 40., 40.), Path::new(vec![Subpath::polyline(&[(70., 80.), (100., 100.)])])]);
    let mut doc = (*s.active().unwrap().doc).clone();
    doc.layer_mut(ids[0]).unwrap().vector.corners = vec![KnotCorner { subpath: 0, knot: 0, corner: Corner { radius: 5., kind: CornerKind::Round } }];
    s.active_mut().unwrap().doc = Arc::new(doc);
    let r = req(
        &s,
        &ids,
        Action::Average { axis: Axis::Both, anchors: vec![Anchors { layer: ids[0], points: vec![[0, 2]] }, Anchors { layer: ids[1], points: vec![[0, 0]] }] },
    );
    let p = prepare(&s, &r).unwrap();
    // The base rectangle's knot 2 is (50,50): centroid with (70,80) is (60,65).
    // Rounding knot 0 inserts extra nodes, which must never redirect this selection.
    let path = &shape(p.document.layer(ids[0]).unwrap()).unwrap().path;
    assert!(path.subpaths[0].knots.iter().any(|k| k.anchor.x == 60. && k.anchor.y == 65.));
    let path = &shape(p.document.layer(ids[1]).unwrap()).unwrap().path;
    assert_eq!((path.subpaths[0].knots[0].anchor.x, path.subpaths[0].knots[0].anchor.y), (60., 65.));
    assert!(p.document.layer(ids[0]).unwrap().vector.corners.is_empty());
    assert!(s.journal.is_empty());
}

#[test]
fn object36_vector_clipboard_symbol_resources_use_owner_transfer_without_collisions() {
    let (mut source, ids) = fixture(vec![photocraft_vector::shapes::rect(10., 10., 20., 20.)]);
    let c = Capture::begin(&source, &ids).unwrap();
    let out = source.execute("symbol.create", json!({"document":c.document,"revision":c.revision,"layers":ids,"name":"Source"})).unwrap();
    let original_id = LayerId(out["layer"].as_u64().unwrap());
    let clip = clipboard::Clipboard::copy(&source, &[original_id]).unwrap();
    let (mut destination, ids) = fixture(vec![photocraft_vector::shapes::rect(40., 40., 40., 40.)]);
    let c = Capture::begin(&destination, &ids).unwrap();
    let out = destination.execute("symbol.create", json!({"document":c.document,"revision":c.revision,"layers":ids,"name":"Destination"})).unwrap();
    let target = LayerId(out["layer"].as_u64().unwrap());
    let before = destination.active().unwrap().doc.clone();
    let c = Capture::begin(&destination, &[target]).unwrap();
    let out = clip.paste(&mut destination, &c, true).unwrap();
    let copy_id = LayerId(out["layers"][0].as_u64().unwrap());
    let doc = &destination.active().unwrap().doc;
    assert_eq!(doc.symbols.len(), before.symbols.len() + 1);
    let LayerContent::Symbol(copy) = &doc.layer(copy_id).unwrap().content else { panic!("live instance retained") };
    let LayerContent::Symbol(old) = &doc.layer(target).unwrap().content else { panic!("destination instance retained") };
    assert_ne!(copy.symbol, old.symbol);
    let master = doc.symbols.iter().find(|d| d.id == copy.symbol).unwrap();
    let LayerContent::Shape(sh) = &master.layers[0].content else { panic!("plain source master") };
    assert_eq!(ops::bounds(&sh.path).unwrap().unwrap(), (10., 10., 30., 30.));
    assert!(destination.undo());
    assert_eq!(destination.active().unwrap().doc, before);
}

#[test]
fn object36_vector_paste_without_selection_uses_root_ends_and_empty_document() {
    let (mut s, ids) = fixture(vec![photocraft_vector::shapes::rect(10., 10., 20., 20.)]);
    let clip = clipboard::Clipboard::copy(&s, &ids).unwrap();
    for front in [false, true] {
        let before = s.active().unwrap().doc.clone();
        let c = Capture::begin_paste(&s, &[]).unwrap();
        let out = clip.paste(&mut s, &c, front).unwrap();
        let id = LayerId(out["layers"][0].as_u64().unwrap());
        let order: Vec<_> = s.active().unwrap().doc.layers.iter().map(|l| l.id).collect();
        assert_eq!(order, if front { vec![ids[0], id] } else { vec![id, ids[0]] });
        assert_eq!(path(&s, id), path(&s, ids[0]));
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
    }
    let mut doc = (*s.active().unwrap().doc).clone();
    doc.layers.clear();
    s.active_mut().unwrap().doc = Arc::new(doc);
    let c = Capture::begin_paste(&s, &[]).unwrap();
    clip.paste(&mut s, &c, true).unwrap();
    assert_eq!(s.active().unwrap().doc.layers.len(), 1);
    assert!(s.undo());
    assert!(s.active().unwrap().doc.layers.is_empty());
    assert!(s.execute("object.pasteBack", json!({"capture":c,"clipboard":clip})).is_err(), "undo made the capture stale");
}
