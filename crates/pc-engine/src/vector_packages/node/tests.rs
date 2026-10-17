//! Gesture replay semantics adapted from VectorCraft crates/tools/src/direct.rs tests,
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092, MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Synthetic geometry and native/GPU oracles are original Local Image tests.

use super::*;
use photocraft_doc::vector_design::drawing::{Corner, CornerKind, KnotCorner};
use photocraft_doc::{ColorMode, Fill, Layer, SampleType, ShapeLayer, Size, Subpath};
use std::sync::Arc;

fn setup() -> (Session, Vec<Target>) {
    let mut doc = Document::new("Node replay", Size::new(200, 200), ColorMode::Rgb, SampleType::U8);
    let mut targets = Vec::new();
    for x in [20.0, 100.0] {
        let shape = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 20.0), (x + 40.0, 20.0), (x + 40.0, 60.0), (x, 60.0)])]),
            fill: Some(Fill::Solid(photocraft_doc::Color::BLACK)),
            ..Default::default()
        };
        let mut l = Layer::new("Curve", LayerContent::Shape(shape));
        l.vector.corners.push(KnotCorner { subpath: 0, knot: 2, corner: Corner { radius: 4.0, kind: CornerKind::Round } });
        targets.push(Target { name: "layer".into(), layer: Some(l.id.0), anchors: vec![[0, 2]] });
        doc.layers.push(l);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, targets)
}
fn command(s: &mut Session, targets: &[Target], op: Operation) -> Result<Value> {
    let st = s.active().unwrap();
    s.execute("path.node.edit", json!({"document":st.doc.id.0,"revision":st.revision,"targets":targets,"operation":op}))
}
fn native(s: &Session) -> Vec<u8> {
    photocraft_format::save_to_bytes(&s.active().unwrap().doc, &Default::default()).unwrap()
}
#[test]
fn node_batch_preview_atomic_commit_native_undo_and_journal() {
    let (mut s, t) = setup();
    let before = native(&s);
    let doc = s.active().unwrap().doc.clone();
    let shown = prepare(&doc, &t, &Operation::Move { delta: [10.0, 5.0] }, false).unwrap();
    assert_eq!(native(&s), before);
    assert!(s.journal.is_empty());
    command(&mut s, &t, Operation::Move { delta: [10.0, 5.0] }).unwrap();
    assert_eq!(*s.active().unwrap().doc, shown.doc);
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert!(s.undo());
    assert_eq!(native(&s), before);
}
#[test]
fn node_topology_remaps_corners_and_selection_in_same_undo() {
    let (mut s, mut t) = setup();
    t.truncate(1);
    let before = native(&s);
    command(&mut s, &t, Operation::Insert { knot: [0, 0], t: 0.5 }).unwrap();
    let id = LayerId(t[0].layer.unwrap());
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().vector.corners[0].knot, 3);
    t[0].anchors = vec![[0, 1]];
    command(&mut s, &t, Operation::Delete).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().vector.corners[0].knot, 2);
    command(&mut s, &t, Operation::Reverse).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().vector.corners[0].knot, 1);
    for _ in 0..3 {
        assert!(s.undo());
    }
    assert_eq!(native(&s), before);
}

#[test]
fn node_vector_mask_corner_remap_conversion_and_native_undo() {
    let (mut s, mut t) = setup();
    t.truncate(1);
    let id = LayerId(t[0].layer.unwrap());
    s.edit("mask fixture", |doc, _| {
        let l = doc.layer_mut(id).unwrap();
        let LayerContent::Shape(sh) = &l.content else { panic!("shape") };
        l.vector_mask = Some(photocraft_doc::VectorMask::new(sh.path.clone()));
        l.content = LayerContent::Fill(Fill::Solid(photocraft_doc::Color::BLACK));
        l.vector.live_shape = Some(photocraft_doc::vector_design::drawing::DesignShape {
            rect: [20.0, 20.0, 40.0, 40.0],
            angle: 0.0,
            parameters: photocraft_doc::vector_design::drawing::ShapeParameters::Rect { radii: [0.0; 4], kinds: [CornerKind::Round; 4] },
        });
        Ok(())
    })
    .unwrap();
    let before = native(&s);
    let journal = s.journal.len();
    assert!(command(&mut s, &t, Operation::Insert { knot: [0, 0], t: 0.5 }).is_err());
    assert_eq!(native(&s), before);
    s.execute("path.node.edit", json!({"targets":t,"operation":{"type":"insert","knot":[0,0],"t":0.5},"convertLive":true})).unwrap();
    let l = s.active().unwrap().doc.layer(id).unwrap();
    assert_eq!(l.vector.corners[0].knot, 3);
    assert!(l.vector.live_shape.is_none());
    t[0].anchors = vec![[0, 1]];
    command(&mut s, &t, Operation::Delete).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().vector.corners[0].knot, 2);
    assert_eq!(s.journal.len(), journal + 2);
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(native(&s), before);
}
#[test]
fn node_invalid_last_target_stale_lock_and_overflow_are_atomic() {
    let (mut s, t) = setup();
    let before = native(&s);
    let revision = s.active().unwrap().revision;
    let mut invalid = t.clone();
    invalid[1].anchors = vec![[0, usize::MAX]];
    assert!(command(&mut s, &invalid, Operation::Move { delta: [1.0, 2.0] }).is_err());
    let mut duplicate = t.clone();
    duplicate.push(t[0].clone());
    assert!(command(&mut s, &duplicate, Operation::Delete).is_err());
    assert!(command(&mut s, &t, Operation::Move { delta: [1e6, 0.0] }).is_err());
    assert!(s.execute("path.node.edit", json!({"revision":revision+1,"targets":t,"operation":{"type":"delete"}})).is_err());
    assert_eq!(native(&s), before);
    assert!(s.journal.is_empty());
    assert!(!s.undo());
    s.active_mut().unwrap().doc = Arc::new({
        let mut doc = (*s.active().unwrap().doc).clone();
        doc.layer_mut(LayerId(t[1].layer.unwrap())).unwrap().locks.all = true;
        doc
    });
    assert!(command(&mut s, &t, Operation::Sharp).is_err());
    assert!(s.journal.is_empty());
    assert!(!s.undo());
}
#[test]
fn node_live_shape_requires_explicit_conversion_and_undo_restores_it() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width":100,"height":100})).unwrap();
    let id = s.execute("shape.create", json!({"kind":"rect","rect":[10,10,50,50]})).unwrap()["layer"].as_u64().unwrap();
    let targets = vec![Target { name: "layer".into(), layer: Some(id), anchors: vec![[0, 0]] }];
    let before = native(&s);
    assert!(command(&mut s, &targets, Operation::Move { delta: [5.0, 0.0] }).is_err());
    assert_eq!(native(&s), before);
    s.execute("path.node.edit", json!({"targets":targets,"operation":{"type":"move","delta":[5,0]},"convertLive":true})).unwrap();
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(LayerId(id)).unwrap().content else { panic!("shape") };
    assert!(sh.live.is_none());
    assert_eq!(sh.path.subpaths[0].knots[0].anchor.x, 15.0);
    assert!(s.undo());
    assert_eq!(native(&s), before);
}
#[test]
fn node_vc_replay_smooth_handles_segment_and_alignment() {
    let (mut s, mut t) = setup();
    t.truncate(1);
    t[0].anchors = vec![[0, 0], [0, 1], [0, 2], [0, 3]];
    command(&mut s, &t, Operation::Smart).unwrap();
    command(&mut s, &t, Operation::Handle { knot: [0, 1], handle: "out".into(), to: [72.0, 24.0], independent: false }).unwrap();
    let p = path(&s.active().unwrap().doc, &t[0]).unwrap();
    let k = p.subpaths[0].knots[1];
    let (a, b) = (k.in_ctrl, k.out_ctrl);
    assert!(((a.x - k.anchor.x) * (b.y - k.anchor.y) - (a.y - k.anchor.y) * (b.x - k.anchor.x)).abs() < 1e-9);
    command(&mut s, &t, Operation::Align { axis: "y".into(), value: 50.0 }).unwrap();
    assert!(path(&s.active().unwrap().doc, &t[0]).unwrap().subpaths[0].knots.iter().all(|k| k.anchor.y == 50.0));
}
#[test]
fn node_random_sequences_256_finite_corner_valid_and_native_undo() {
    for seed in 0..256_u64 {
        let (mut s, mut t) = setup();
        t.truncate(1);
        let before = native(&s);
        let mut rng = seed + 1;
        for step in 0..16 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let len = path(&s.active().unwrap().doc, &t[0]).unwrap().subpaths[0].knots.len();
            let k = (rng as usize) % len;
            t[0].anchors = vec![[0, k]];
            let op = match step % 8 {
                0 => Operation::Move { delta: [(rng % 7) as f64 - 3.0, 2.0] },
                1 => Operation::Smart,
                2 => Operation::Insert { knot: [0, k.min(len - 2)], t: 0.37 },
                3 => Operation::Smooth,
                4 => Operation::Delete,
                5 => Operation::Reverse,
                6 => Operation::Sharp,
                _ => Operation::Transform { matrix: [1.01, 0.0, 0.0, 0.99, 1.0, -1.0] },
            };
            command(&mut s, &t, op).unwrap();
            let p = path(&s.active().unwrap().doc, &t[0]).unwrap();
            edit::validate(p).unwrap();
            let l = s.active().unwrap().doc.layer(LayerId(t[0].layer.unwrap())).unwrap();
            assert!(l.vector.corners.iter().all(|c| p.subpaths.get(c.subpath).and_then(|sp| sp.knots.get(c.knot)).is_some()));
            for k in p.subpaths.iter().flat_map(|sp| &sp.knots).filter(|k| k.smooth) {
                let a = Point::new(k.in_ctrl.x - k.anchor.x, k.in_ctrl.y - k.anchor.y);
                let b = Point::new(k.out_ctrl.x - k.anchor.x, k.out_ctrl.y - k.anchor.y);
                assert!((a.x * b.y - a.y * b.x).abs() < 1e-5);
            }
        }
        for _ in 0..16 {
            assert!(s.undo());
        }
        assert_eq!(native(&s), before, "seed {seed}");
    }
}
#[test]
fn node_topology_failure_is_an_error_without_edit() {
    let (mut s, t) = setup();
    let before = native(&s);
    if GEOMETRY.get().is_none() {
        assert!(command(&mut s, &t, Operation::Join { tolerance: 0.1 }).is_err());
        assert!(command(&mut s, &t[..1], Operation::Break).is_err());
    } else {
        assert!(command(&mut s, &t, Operation::Join { tolerance: -1.0 }).is_err());
        let mut empty = t[..1].to_vec();
        empty[0].anchors.clear();
        assert!(command(&mut s, &empty, Operation::Break).is_err());
    }
    assert_eq!(native(&s), before);
    assert!(s.journal.is_empty());
}

#[test]
fn node_v1_break_join_close_corner_replay_coverage_and_native_undo() {
    let (mut s, mut t) = setup();
    assert!(GEOMETRY.get().is_some(), "required V1 provider must be installed");
    t.truncate(1);
    t[0].anchors = vec![[0, 0], [0, 2]];
    let before = native(&s);
    let original = path(&s.active().unwrap().doc, &t[0]).unwrap().clone();
    let rect = photocraft_geom::Rect::new(0, 0, 200, 200);
    let coverage = photocraft_vector::path_coverage(&original, rect);
    command(&mut s, &t, Operation::Break).unwrap();
    let p = path(&s.active().unwrap().doc, &t[0]).unwrap();
    assert_eq!(p.subpaths.len(), 2);
    assert!(p.subpaths.iter().all(|sp| !sp.closed));
    let l = s.active().unwrap().doc.layer(LayerId(t[0].layer.unwrap())).unwrap();
    assert_eq!(l.vector.corners.len(), 2);
    t[0].anchors = vec![[0, 0]];
    command(&mut s, &t, Operation::Join { tolerance: 0.1 }).unwrap();
    command(&mut s, &t, Operation::Close).unwrap();
    let p = path(&s.active().unwrap().doc, &t[0]).unwrap();
    assert_eq!(p.subpaths.len(), 1);
    assert!(p.subpaths[0].closed);
    assert_eq!(photocraft_vector::path_coverage(p, rect), coverage);
    let l = s.active().unwrap().doc.layer(LayerId(t[0].layer.unwrap())).unwrap();
    assert!(l.vector.corners.iter().all(|c| p.subpaths.get(c.subpath).and_then(|sp| sp.knots.get(c.knot)).is_some()));
    for _ in 0..3 {
        assert!(s.undo());
    }
    assert_eq!(native(&s), before);
}

#[test]
fn node_v1_join_consumes_shape_layers_remaps_corners_and_native_undo() {
    let (mut s, t) = setup();
    assert!(GEOMETRY.get().is_some(), "required V1 provider must be installed");
    s.edit("open fixtures", |doc, _| {
        for (index, target) in t.iter().enumerate() {
            let layer = doc.layer_mut(LayerId(target.layer.unwrap())).unwrap();
            let LayerContent::Shape(sh) = &mut layer.content else { panic!("shape") };
            sh.path.subpaths[0].closed = false;
            // Join the facing endpoints without a connector crossing either square.
            if index == 0 {
                sh.path.subpaths[0].knots.rotate_left(3);
                layer.vector.corners[0].knot = 3;
            } else {
                sh.path.subpaths[0].knots.reverse();
                layer.vector.corners[0].knot = 1;
            }
        }
        Ok(())
    })
    .unwrap();
    let before = native(&s);
    let journal = s.journal.len();
    command(&mut s, &t, Operation::Join { tolerance: 0.1 }).unwrap();
    let doc = &s.active().unwrap().doc;
    assert_eq!(doc.layers.len(), 1);
    let l = doc.layer(LayerId(t[0].layer.unwrap())).unwrap();
    let LayerContent::Shape(sh) = &l.content else { panic!("shape") };
    assert_eq!(sh.path.subpaths[0].knots.len(), 8);
    assert_eq!(l.vector.corners.len(), 2);
    assert!(l.vector.corners.iter().all(|c| sh.path.subpaths.get(c.subpath).and_then(|sp| sp.knots.get(c.knot)).is_some()));
    assert_eq!(s.journal.len(), journal + 1);
    assert!(s.undo());
    assert_eq!(native(&s), before);
}

#[test]
fn review_regression_node_provenance_matches_signed_zero_and_prefers_unchanged_coincident_knots() {
    let first = photocraft_doc::Knot::corner(-0., 5.);
    let second = photocraft_doc::Knot::smooth(Point::new(0., 5.), Point::new(1., 4.), Point::new(1., 6.));
    let base = Path::new(vec![photocraft_doc::Subpath { knots: vec![first, second], ..Default::default() }]);
    let output = Path::new(vec![photocraft_doc::Subpath { knots: vec![second, first, photocraft_doc::Knot::corner(9., 9.)], ..Default::default() }]);
    assert_eq!(provenance(&base, &output).unwrap(), vec![vec![Some([0, 1]), Some([0, 0]), None]]);
}

#[test]
fn dsfix_v137_indexed_provenance_keeps_coincident_and_signed_zero_rules() {
    let mut base = Path::new(vec![Subpath::polyline(&[(-0., 0.), (10., 0.), (0., 0.)])]);
    base.subpaths[0].knots[2].in_ctrl = Point::new(2., 3.);
    base.subpaths[0].knots[2].out_ctrl = Point::new(4., 5.);
    let mut reversed = base.subpaths[0].knots[2];
    std::mem::swap(&mut reversed.in_ctrl, &mut reversed.out_ctrl);
    let output = Path::new(vec![Subpath {
        knots: vec![base.subpaths[0].knots[2], reversed, photocraft_doc::Knot::corner(0., 0.), photocraft_doc::Knot::corner(99., 99.)],
        closed: false,
        op: Default::default(),
    }]);
    assert_eq!(provenance(&base, &output).unwrap(), vec![vec![Some([0, 2]), Some([0, 2]), Some([0, 0]), None]]);
    let large = Path::new(vec![Subpath::polyline(&(0..16_384).map(|i| (f64::from(i), 0.)).collect::<Vec<_>>())]);
    let origins = provenance(&large, &large).unwrap();
    assert_eq!(origins[0].len(), 16_384);
    assert_eq!(origins[0][16_383], Some([0, 16_383]));
}
