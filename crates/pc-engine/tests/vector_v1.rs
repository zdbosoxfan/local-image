//! Tests adapted from VectorCraft crates/engine/src/{tests_pathops,tests_outlinestroke}.rs
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-NOTICE.
//! V1 commands through the same journal/undo/format channel used by UI and automation.
use photocraft_doc::{Fill, LayerContent, Path, PathOp, Subpath};
use photocraft_engine::Session;
use photocraft_pathops as ops;
use serde_json::{Value, json};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width":192,"height":192,"background":"transparent"})).unwrap();
    s
}
fn shape(s: &mut Session, rect: [f64; 4], color: &str) -> u64 {
    s.execute("shape.create", json!({"kind":"rect","rect":rect,"fill":color})).unwrap()["layer"].as_u64().unwrap()
}
fn path(s: &Session, id: u64) -> Path {
    match &s.active().unwrap().doc.layer(photocraft_doc::LayerId(id)).unwrap().content {
        LayerContent::Shape(sh) => sh.path.clone(),
        _ => panic!("shape missing"),
    }
}
fn result_ids(v: &Value) -> Vec<u64> {
    v["layers"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect()
}
#[test]
fn union_preserves_bottom_appearance_is_one_step_and_journaled() {
    let mut s = session();
    let a = shape(&mut s, [10.0, 10.0, 100.0, 100.0], "#ff0000");
    let b = shape(&mut s, [60.0, 60.0, 100.0, 100.0], "#0000ff");
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.entries().len();
    let r = s.execute("path.boolean", json!({"layers":[b,a],"op":"add"})).unwrap();
    assert_eq!(result_ids(&r), vec![a]);
    assert_eq!(s.active().unwrap().doc.layer_count(), before.layer_count() - 1);
    assert!((ops::area(&path(&s, a)).unwrap() - 17500.0).abs() < 0.1);
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(photocraft_doc::LayerId(a)).unwrap().content else { panic!() };
    assert!(matches!(sh.fill,Some(Fill::Solid(c)) if c.to_rgb()==[1.0,0.0,0.0]));
    assert!(sh.live.is_none() && sh.psd_raw.is_none());
    assert!(sh.cache.is_some());
    assert_eq!(s.active().unwrap().history.entries().len(), history + 1);
    assert_eq!(s.journal.last().unwrap().0, "path.boolean");
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.active().unwrap().doc.as_ref(), before.as_ref());
    s.execute("edit.redo", json!({})).unwrap();
    assert_eq!(s.active().unwrap().doc.layer_count(), before.layer_count() - 1);
}
#[test]
fn divide_returns_three_layers_and_undo_restores_inputs() {
    let mut s = session();
    let a = shape(&mut s, [10.0, 10.0, 100.0, 100.0], "#ff0000");
    let b = shape(&mut s, [60.0, 60.0, 100.0, 100.0], "#0000ff");
    let before = s.active().unwrap().doc.clone();
    let ids = result_ids(&s.execute("path.boolean", json!({"layers":[a,b],"op":"divide"})).unwrap());
    assert_eq!(ids.len(), 3);
    assert!((ids.iter().map(|id| ops::area(&path(&s, *id)).unwrap()).sum::<f64>() - 17500.0).abs() < 0.1);
    assert_eq!(s.active().unwrap().selected_layers.len(), 3);
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.active().unwrap().doc.as_ref(), before.as_ref());
}
#[test]
fn live_compound_finish_and_hole_psd_roundtrip() {
    let mut s = session();
    let a = shape(&mut s, [10.0, 10.0, 100.0, 100.0], "#ff0000");
    let b = shape(&mut s, [35.0, 35.0, 50.0, 50.0], "#0000ff");
    s.execute("path.boolean", json!({"layers":[a,b],"op":"subtract","keep_compound":true})).unwrap();
    assert_eq!(path(&s, a).subpaths[1].op, PathOp::Subtract);
    s.execute("path.finishCompound", json!({"layers":[a]})).unwrap();
    assert_eq!(path(&s, a).subpaths[1].op, PathOp::Join);
    let doc = &s.active().unwrap().doc;
    let out = photocraft_io::export(doc, "result.psd", &Default::default()).unwrap();
    let back = photocraft_io::import("result.psd", &out.bytes).unwrap().document;
    let original = path(&s, a);
    let shape = back
        .layers
        .iter()
        .find_map(|l| match &l.content {
            LayerContent::Shape(s) => Some(s),
            _ => None,
        })
        .unwrap();
    assert!((ops::area(&shape.path).unwrap() - 7500.0).abs() < 0.1);
    let a = photocraft_vector::path_coverage(&original, doc.bounds());
    let b = photocraft_vector::path_coverage(&shape.path, doc.bounds());
    assert!(a.iter().zip(&b).all(|(a, b)| (a - b).abs() <= 1.0 / 255.0));
}
#[test]
fn all_edit_operations_use_transactions() {
    for (cmd, options) in [
        ("path.offset", json!({"delta":5,"join":"round","miter":4})),
        ("path.simplify", json!({"tolerance":0.5})),
        ("path.smooth", json!({"amount":0.5})),
        ("path.reverse", json!({})),
        ("path.splitAt", json!({"subpath":0,"segment":0,"t":0.5})),
    ] {
        let mut s = session();
        let id = shape(&mut s, [10.0, 10.0, 100.0, 100.0], "#ff0000");
        let before = s.active().unwrap().doc.clone();
        let mut params = options;
        params["layers"] = json!([id]);
        let r = s.execute(cmd, params).unwrap();
        assert!(!result_ids(&r).is_empty());
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(s.active().unwrap().doc.as_ref(), before.as_ref(), "{cmd}");
    }
}
#[test]
fn every_pathfinder_mode_runs_and_keeps_history() {
    for op in ["trim", "merge", "crop", "outline", "minusBack"] {
        let mut s = session();
        let a = shape(&mut s, [10.0, 10.0, 100.0, 100.0], "#ff0000");
        let b = shape(&mut s, [60.0, 60.0, 100.0, 100.0], "#ff0000");
        let before = s.active().unwrap().doc.clone();
        let r = s.execute("path.pathfinder", json!({"layers":[b,a],"op":op})).unwrap();
        assert!(!result_ids(&r).is_empty(), "{op}");
        let ids = result_ids(&r);
        if op != "outline" {
            let expected = match op {
                "trim" | "merge" => 17500.0,
                "crop" => 2500.0,
                _ => 7500.0,
            };
            assert!((ids.iter().map(|id| ops::area(&path(&s, *id)).unwrap()).sum::<f64>() - expected).abs() < 0.1, "{op}");
        }
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(s.active().unwrap().doc.as_ref(), before.as_ref());
    }
}
#[test]
fn outline_stroke_converts_paint_and_join_connects_open_ends() {
    let mut s = session();
    let id = shape(&mut s, [10.0, 10.0, 100.0, 100.0], "#ff0000");
    s.execute("shape.edit", json!({"layer":id,"stroke":{"width":8,"color":"#0000ff","opacity":50,"join":"round","dashes":[2,1]}})).unwrap();
    let before = photocraft_compose::flatten(&s.active().unwrap().doc);
    let ids = result_ids(&s.execute("path.outlineStroke", json!({"layers":[id]})).unwrap());
    assert_eq!(ids.len(), 2);
    let after = photocraft_compose::flatten(&s.active().unwrap().doc);
    let worst = before.px.iter().zip(&after.px).flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs())).fold(0.0, f32::max);
    assert!(worst <= 1.0 / 255.0, "expanded paint difference {worst}");
    let layer = s.active().unwrap().doc.layer(photocraft_doc::LayerId(*ids.last().unwrap())).unwrap();
    let LayerContent::Shape(sh) = &layer.content else { panic!() };
    assert!(sh.stroke.is_none());
    assert!(matches!(sh.fill,Some(Fill::Solid(c)) if c.to_rgb()==[0.0,0.0,1.0]));
    assert!((layer.opacity - 0.5).abs() < 1e-6);
    let create = |s: &mut Session, pts: &[(f64, f64)]| {
        s.execute("shape.create",json!({"kind":"path","path":photocraft_engine::vector_cmds::path_json(&Path::new(vec![Subpath::polyline(pts)]))})).unwrap()["layer"].as_u64().unwrap()
    };
    let a = create(&mut s, &[(10.0, 10.0), (50.0, 10.0)]);
    let b = create(&mut s, &[(50.0, 10.0), (90.0, 10.0)]);
    s.execute("path.join", json!({"layers":[a,b],"tolerance":0.01})).unwrap();
    assert_eq!(path(&s, a).subpaths.len(), 1);
    assert_eq!(path(&s, a).subpaths[0].knots.len(), 3);
}
#[test]
fn invalid_input_is_atomic_and_does_not_journal() {
    let mut s = session();
    let a = shape(&mut s, [10.0, 10.0, 100.0, 100.0], "#ff0000");
    let before = s.active().unwrap().doc.clone();
    let journal = s.journal.len();
    for (cmd, p) in [
        ("path.boolean", json!({"layers":[a,a],"op":"add"})),
        ("path.offset", json!({"layers":[a],"join":"invalid"})),
        ("path.simplify", json!({"layers":[a],"tolerance":-1})),
        ("path.splitAt", json!({"layers":[a],"segment":9999})),
        ("path.outlineStroke", json!({"layers":[a]})),
    ] {
        assert!(s.execute(cmd, p).is_err());
        assert_eq!(s.active().unwrap().doc.as_ref(), before.as_ref());
        assert_eq!(s.journal.len(), journal);
    }
}
#[test]
fn locked_and_nonshape_inputs_leave_document_history_and_journal_unchanged() {
    let mut s = session();
    let id = shape(&mut s, [10.0, 10.0, 100.0, 100.0], "#ff0000");
    s.edit("Lock shape", |doc, _| {
        doc.layer_mut(photocraft_doc::LayerId(id)).unwrap().locks.all = true;
        Ok(())
    })
    .unwrap();
    let raster = s.active().unwrap().doc.layers.iter().find(|l| matches!(l.content, LayerContent::Raster(_))).unwrap().id.0;
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.entries().len();
    let journal = s.journal.len();
    for input in [id, raster, u64::MAX] {
        for command in [
            "path.boolean",
            "path.pathfinder",
            "path.outlineStroke",
            "path.offset",
            "path.simplify",
            "path.smooth",
            "path.reverse",
            "path.join",
            "path.splitAt",
            "path.finishCompound",
        ] {
            assert!(s.execute(command, json!({"layers":[input]})).is_err(), "{command}");
            assert_eq!(s.active().unwrap().doc.as_ref(), before.as_ref(), "{command}");
            assert_eq!(s.active().unwrap().history.entries().len(), history, "{command}");
            assert_eq!(s.journal.len(), journal, "{command}");
        }
    }
}
#[test]
fn v1_bundle_with_shapes_masks_and_paths_loads() {
    let mut s = session();
    let id = shape(&mut s, [10.0, 10.0, 100.0, 100.0], "#ff0000");
    let p = photocraft_engine::vector_cmds::path_json(&path(&s, id));
    s.execute("path.set", json!({"name":"Saved","path":p})).unwrap();
    let raster = s.active().unwrap().doc.layers.iter().find(|l| matches!(l.content, LayerContent::Raster(_))).unwrap().id.0;
    s.execute("layer.vectorMask.add", json!({"layer":raster,"path":p})).unwrap();
    let bytes = photocraft_format::save_to_bytes(&s.active().unwrap().doc, &Default::default()).unwrap();
    assert_eq!(photocraft_format::read_manifest(&bytes).unwrap().format_version, 1);
    let back = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(back.paths.len(), 1);
    assert!(back.layer(photocraft_doc::LayerId(raster)).unwrap().vector_mask.is_some());
    assert!(matches!(back.layer(photocraft_doc::LayerId(id)).unwrap().content, LayerContent::Shape(_)));
}

#[test]
fn upstream_dashed_line_outline_has_one_contour_per_dash() {
    let mut s = session();
    let p = Path::new(vec![Subpath::polyline(&[(0.0, 50.0), (100.0, 50.0)])]);
    let id = s
        .execute(
            "shape.create",
            json!({"kind":"path","path":photocraft_engine::vector_cmds::path_json(&p),"fill":null,"stroke":{"width":4,"dashes":[2.5,2.5]}}),
        )
        .unwrap()["layer"]
        .as_u64()
        .unwrap();
    s.execute("path.outlineStroke", json!({"layers":[id]})).unwrap();
    let p = path(&s, id);
    assert_eq!(p.subpaths.len(), 5);
    assert!((ops::area(&p).unwrap() - 200.0).abs() < 0.5);
    assert_eq!(p.bounds(), Some((0.0, 48.0, 90.0, 52.0)));
}

#[test]
fn empty_operand_live_compound_keeps_its_set_semantics() {
    for op in ["subtract", "intersect"] {
        let mut s = session();
        let empty = s.execute("shape.create", json!({"kind":"path","path":{"subpaths":[]}})).unwrap()["layer"].as_u64().unwrap();
        let a = shape(&mut s, [0.0, 0.0, 100.0, 100.0], "#ff0000");
        s.execute("path.boolean", json!({"layers":[empty,a],"op":op,"keep_compound":true})).unwrap();
        assert!(ops::finish_compound(&path(&s, empty), None).unwrap().is_empty());
    }
}

#[test]
fn checked_in_v1_fixture_with_missing_defaults_loads() {
    let bytes = include_bytes!("fixtures/vector-v1.pcraft");
    assert_eq!(photocraft_format::read_manifest(bytes).unwrap().format_version, 1);
    let doc = photocraft_format::load_from_bytes(bytes).unwrap();
    assert_eq!(doc.paths.len(), 1);
    assert_eq!(doc.paths[0].name, "Saved");
    let sh = doc
        .layers
        .iter()
        .find_map(|l| match &l.content {
            LayerContent::Shape(s) => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!(sh.path.fill_rule, photocraft_doc::FillRule::NonZero);
    assert!(!sh.path.inverted);
    assert_eq!(sh.path.subpaths[0].op, PathOp::Combine);
    assert!(sh.path.subpaths[0].knots.iter().all(|k| !k.smooth));
    assert!((ops::area(&sh.path).unwrap() - 10000.0).abs() < 0.1);
    assert!(doc.layers.iter().any(|l| l.vector_mask.is_some()));
}
