//! Package 41 transaction oracles, using owned CommandSpecs until 00 registers the slot.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Fill, SampleType, ShapeLayer, Size, Subpath};
fn session() -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Distort", Size::new(256, 180), ColorMode::Rgb, SampleType::U8);
    let mut ids = Vec::new();
    for x in [20., 150.] {
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 30.), (x + 70., 30.), (x + 70., 130.), (x, 130.)])]),
            fill: Some(Fill::Solid(Color::rgb(0.1, 0.4, 0.8))),
            ..Default::default()
        };
        crate::vector_cmds::refresh_shape(&doc, &mut sh);
        let l = Layer::new("Curve", LayerContent::Shape(sh));
        ids.push(l.id);
        doc.layers.push(l);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    s.select_layer(ids[0]).unwrap();
    (s, ids)
}
fn run(s: &mut Session, id: &str, v: Value) -> Result<Value> {
    let spec = specs().into_iter().find(|c| c.id == id).unwrap();
    (spec.run)(s, &v)
}
#[test]
fn distort41_engine_atomic_preview_one_undo_and_stale_commit_rejection() {
    let (mut s, ids) = session();
    let before = s.active().unwrap().doc.clone();
    let mut g = LiquifyGesture::begin(&s, ids, liquify::Options { kind: liquify::Kind::Bloat, ..Default::default() }, 0).unwrap();
    let history = s.active().unwrap().history.past_len();
    let journal = s.journal.len();
    let preview = g.push(&s, liquify::Sample::new(85., 80., 1.)).unwrap();
    assert_ne!(preview.document, before);
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), journal);
    let expected = preview.document.clone();
    let value = serde_json::to_value(&g.request).unwrap();
    run(&mut s, "object.liquify", value.clone()).unwrap();
    assert_eq!(s.active().unwrap().doc, expected);
    assert_eq!(s.active().unwrap().history.past_len(), history + 1);
    assert!(run(&mut s, "object.liquify", value.clone()).is_err());
    assert_eq!(s.active().unwrap().doc, expected);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(run(&mut s, "object.liquify", value).is_err(), "undo advances revision");
}
#[test]
fn distort41_engine_full_target_preflight_locks_duplicates_and_missing_layers() {
    let (mut s, ids) = session();
    s.edit("Lock", |doc, _| {
        doc.layer_mut(ids[1]).unwrap().locks.position = true;
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    assert!(Capture::begin(&s, ids.clone()).is_err());
    assert!(Capture::begin(&s, vec![ids[0], ids[0]]).is_err());
    assert!(Capture::begin(&s, vec![ids[0], LayerId(u64::MAX)]).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().history.past_len(), history);
    s.select_layer(ids[0]).unwrap();
    assert_eq!(target_layers(&s).unwrap(), vec![ids[0]]);
    let st = s.active_mut().unwrap();
    st.active_layer = None;
    st.selected_layers.clear();
    assert_eq!(target_layers(&s).unwrap(), vec![ids[0]]);
}
#[test]
fn distort41_engine_automatic_targets_all_under_stroke_and_noop_preserves_document() {
    let (mut s, ids) = session();
    let st = s.active_mut().unwrap();
    st.active_layer = None;
    st.selected_layers.clear();
    assert_eq!(target_layers(&s).unwrap(), ids);
    let mut g =
        LiquifyGesture::begin(&s, target_layers(&s).unwrap(), liquify::Options { width: 70., height: 70., simplify_on: false, ..Default::default() }, 9)
            .unwrap();
    g.push(&s, liquify::Sample::new(90., 80., 1.)).unwrap();
    g.push(&s, liquify::Sample::new(220., 80., 1.)).unwrap();
    let p = prepare_liquify(&s, &g.request).unwrap();
    assert_eq!(p.edits.len(), 2);
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    g.request.options.intensity = 0.;
    let value = serde_json::to_value(&g.request).unwrap();
    assert!(run(&mut s, "object.liquify", value).unwrap()["layers"].as_array().unwrap().is_empty());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().history.past_len(), history);
}
#[test]
fn distort41_engine_puppet_preview_release_translation_undo_and_queries_do_not_edit() {
    let (mut s, ids) = session();
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    let g = PuppetGesture::begin(&s, ids.clone(), Default::default()).unwrap();
    let mut pins = g.rig.automatic_pins();
    for p in &mut pins {
        p.at = [p.rest[0] + 5., p.rest[1] + 7.];
    }
    let preview = g.preview(&s, &pins).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    let r = PuppetRequest { capture: g.capture.clone(), pins, options: Default::default() };
    run(&mut s, "object.puppetWarp.preview", serde_json::to_value(&r).unwrap()).unwrap();
    assert_eq!(s.active().unwrap().history.past_len(), history);
    run(&mut s, "object.puppetWarp", serde_json::to_value(&r).unwrap()).unwrap();
    assert_eq!(s.active().unwrap().doc, preview.document);
    assert_eq!(s.active().unwrap().history.past_len(), history + 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    let metadata = specs();
    assert!(metadata.iter().filter(|c| c.id.ends_with("preview") || c.id.ends_with("info")).all(|c| !c.journal));
    assert!(metadata.iter().filter(|c| !c.id.ends_with("preview") && !c.id.ends_with("info")).all(|c| c.journal));
}
#[test]
fn distort41_engine_live_shape_conversion_is_explicit_and_atomic() {
    let (mut s, ids) = session();
    s.edit("Live", |doc, _| {
        let l = doc.layer_mut(ids[0]).unwrap();
        let LayerContent::Shape(sh) = &mut l.content else { panic!() };
        sh.live = Some(photocraft_doc::LiveShape::Rect { rect: [20., 30., 70., 100.], radii: [0.; 4] });
        Ok(())
    })
    .unwrap();
    assert!(Capture::begin(&s, vec![ids[0]]).unwrap_err().to_string().contains("Convert"));
    let st = s.active().unwrap();
    let c = Capture { document: st.doc.id.0, revision: st.revision, layers: vec![ids[0]], automatic: false };
    let before = st.doc.clone();
    let history = st.history.past_len();
    let preview = prepare_conversion(&s, &c).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    run(&mut s, "object.distort.convert", serde_json::to_value(&c).unwrap()).unwrap();
    assert_eq!(*s.active().unwrap().doc, preview);
    assert_eq!(s.active().unwrap().history.past_len(), history + 1);
    Capture::begin(&s, vec![ids[0]]).unwrap();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn distort41_engine_appearance_and_corners_use_shared_projection_and_clear_geometry_only() {
    let (mut s, ids) = session();
    s.edit("Metadata", |doc, _| {
        let l = doc.layer_mut(ids[0]).unwrap();
        let LayerContent::Shape(sh) = &l.content else { panic!() };
        l.vector.appearance = Some(photocraft_vector::appearance::legacy_appearance(sh));
        l.vector.corners.push(photocraft_doc::vector_design::drawing::KnotCorner {
            subpath: 0,
            knot: 0,
            corner: photocraft_doc::vector_design::drawing::Corner { radius: 5., kind: Default::default() },
        });
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let mut g = LiquifyGesture::begin(&s, vec![ids[0]], liquify::Options { kind: liquify::Kind::Bloat, ..Default::default() }, 0).unwrap();
    let p = g.push(&s, liquify::Sample::new(85., 80., 1.)).unwrap();
    assert!(!p.edits[0].vector.appearance.as_ref().unwrap().items.is_empty());
    assert!(p.edits[0].vector.corners.is_empty());
    assert!(p.edits[0].shape.cache.is_some());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn distort41_engine_automatic_live_shapes_only_prompt_when_reached_by_brush() {
    let (mut s, ids) = session();
    s.edit("Live", |doc, _| {
        let LayerContent::Shape(sh) = &mut doc.layer_mut(ids[1]).unwrap().content else { panic!() };
        sh.live = Some(photocraft_doc::LiveShape::Rect { rect: [150., 30., 70., 100.], radii: [0.; 4] });
        Ok(())
    })
    .unwrap();
    let st = s.active_mut().unwrap();
    st.active_layer = None;
    st.selected_layers.clear();
    let before = st.doc.clone();
    let options = liquify::Options { kind: liquify::Kind::Bloat, width: 70., height: 70., ..Default::default() };
    let mut g = LiquifyGesture::begin(&s, target_layers(&s).unwrap(), options, 0).unwrap();
    let p = g.push(&s, liquify::Sample::new(85., 80., 1.)).unwrap();
    assert_eq!(p.edits.len(), 1);
    assert_eq!(conversion_targets(&s, [85., 80.], options), Vec::<LayerId>::new());
    assert!(g.push(&s, liquify::Sample::new(400., 80., 1.)).unwrap_err().to_string().contains("Convert"));
    assert!(conversion_targets(&s, [400., 80.], options).is_empty());
    assert_eq!(g.conversion_layers, vec![ids[1]], "the swept brush still names the reached live shape");
    assert_eq!(s.active().unwrap().doc, before);
}

#[test]
fn distort41_engine_oversized_appearance_caches_fail_before_publication() {
    let (mut s, ids) = session();
    s.edit("Large vector canvas", |doc, _| {
        doc.size = Size::new(16_000, 16_000);
        doc.depth = SampleType::F32;
        for id in &ids {
            let LayerContent::Shape(sh) = &mut doc.layer_mut(*id).unwrap().content else { panic!() };
            sh.path = Path::new(vec![Subpath::polygon(&[(0., 0.), (15_000., 0.), (15_000., 15_000.), (0., 15_000.)])]);
            sh.cache = None;
        }
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    let g = PuppetGesture::begin(&s, ids, Default::default()).unwrap();
    let mut pins = g.rig.automatic_pins();
    for p in &mut pins {
        p.at[0] += 1.;
    }
    let request = PuppetRequest { capture: g.capture, options: g.rig.options, pins };
    let e = run(&mut s, "object.puppetWarp", serde_json::to_value(request).unwrap()).unwrap_err();
    assert!(e.to_string().contains("256 MiB"), "{e}");
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().history.past_len(), history);
}

#[test]
fn distort41_engine_group_conversion_preflights_locked_children_atomically() {
    let (mut s, ids) = session();
    let mut group_id = ids[0];
    s.edit("Grouped live shapes", |doc, _| {
        let mut children = std::mem::take(&mut doc.layers);
        for l in &mut children {
            let LayerContent::Shape(sh) = &mut l.content else { panic!() };
            sh.live = Some(photocraft_doc::LiveShape::Rect { rect: [20., 30., 70., 100.], radii: [0.; 4] });
        }
        children[1].locks.position = true;
        let group = Layer::new(
            "Group",
            LayerContent::Group(photocraft_doc::Group { children, expanded: true, artboard: None, isolation: photocraft_doc::GroupIsolation::Auto }),
        );
        group_id = group.id;
        doc.layers.push(group);
        Ok(())
    })
    .unwrap();
    let st = s.active().unwrap();
    let c = Capture { document: st.doc.id.0, revision: st.revision, layers: vec![group_id], automatic: false };
    let before = st.doc.clone();
    let history = st.history.past_len();
    assert!(run(&mut s, "object.distort.convert", serde_json::to_value(c).unwrap()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().history.past_len(), history);
}
