#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Size, SmartObject, SmartSource, Subpath};
use photocraft_raster::Surface;
use std::sync::Arc;

fn pixels() -> Session {
    let mut doc = Document::new("Trace fixture", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
    let mut layer = Layer::raster("Logo", doc.pixel_format());
    let surface = layer.surface_mut().unwrap();
    surface.fill_rect(Rect::from_xywh(2, 2, 8, 8), &[1., 0., 0., 1.]);
    surface.fill_rect(Rect::from_xywh(18, 3, 9, 6), &[0., 0., 1., 1.]);
    doc.layers.push(layer);
    let mut s = Session::new();
    s.add_document(doc, None);
    s
}
fn pixel_params() -> Value {
    json!({"preset":"pixel_art","max_colors":2})
}
fn synthetic(s: &Session, path: Path) -> Preview {
    Preview {
        capture: capture(s, None).unwrap(),
        trace: Trace {
            width: 32,
            height: 24,
            nodes: path.subpaths.iter().map(|s| s.knots.len()).sum(),
            layers: vec![pc_trace::ColorPath { color: [0, 0, 0, 255], path }],
        },
        scale: 1.,
        scale_xy: [1., 1.],
        bounds: [0, 0, 32, 24],
        source_rgba: vec![],
        result_rgba: vec![],
        elapsed_ms: 0.,
    }
}
#[test]
fn trace28_two_colour_apply_cache_pixels_single_undo_and_journal() {
    let mut s = pixels();
    let before = s.active().unwrap().doc.clone();
    let out = s.execute("layer.vectorize", json!({"params":pixel_params()})).unwrap();
    assert_eq!(out["colors"], 2);
    assert_eq!(out["shapes"], 2);
    assert_eq!(s.journal.len(), 1);
    let group = s.active().unwrap().doc.layer(LayerId(out["group"].as_u64().unwrap())).unwrap();
    for layer in group.children().unwrap() {
        let LayerContent::Shape(shape) = &layer.content else { panic!("shape") };
        assert!(shape.cache.is_some());
        assert!(shape.cache.as_ref().unwrap().content_bounds().width() <= 10);
    }
    // Independent pixel oracle: exact two rectangles and their original RGB colours.
    let rendered = photocraft_compose::render_layer(group, before.bounds());
    for y in 0..24 {
        for x in 0..32 {
            let expected = if (2..10).contains(&x) && (2..10).contains(&y) {
                [1., 0., 0., 1.]
            } else if (18..27).contains(&x) && (3..9).contains(&y) {
                [0., 0., 1., 1.]
            } else {
                [0.; 4]
            };
            assert_eq!(rendered.get(x, y), expected, "{x},{y}");
        }
    }
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
}
#[test]
fn trace28_expand_connected_islands_preserves_counter_and_coverage() {
    let mut s = pixels();
    let outer = Subpath::polygon(&[(2., 2.), (12., 2.), (12., 12.), (2., 12.)]);
    let hole = Subpath::polygon(&[(4., 4.), (4., 8.), (8., 8.), (8., 4.)]).with_op(photocraft_doc::PathOp::Join);
    let island = Subpath::polygon(&[(20., 2.), (24., 2.), (24., 6.), (20., 6.)]).with_op(photocraft_doc::PathOp::Join);
    let path = Path::new(vec![outer, hole, island]);
    let p = synthetic(&s, path.clone());
    let prepared = prepare(&s.active().unwrap().doc, &p, &Output { expand: true, ..Default::default() }, &JobCtx::new()).unwrap();
    assert_eq!(prepared.children.len(), 2, "hole belongs to its enclosing island");
    let result = publish(&mut s, &p.capture, prepared).unwrap();
    let group = s.active().unwrap().doc.layer(LayerId(result["group"].as_u64().unwrap())).unwrap();
    let actual = photocraft_compose::render_layer(group, Rect::from_xywh(0, 0, 32, 24));
    let coverage = photocraft_vector::path_coverage(&path, actual.rect);
    assert!(coverage.iter().zip(&actual.px).all(|(mask, px)| (mask - px[3]).abs() < 1e-6));
    assert_eq!(actual.get(5, 5)[3], 0.);
    assert_eq!(actual.get(3, 3)[3], 1.);
    assert_eq!(actual.get(22, 3)[3], 1.);
    assert!((actual.px.iter().map(|p| p[3]).sum::<f32>() - 100.).abs() < 1e-6);
}
#[test]
fn trace28_work_path_saved_name_work_path_and_undo() {
    let mut s = pixels();
    let before = s.active().unwrap().doc.clone();
    let out = s.execute("layer.vectorize", json!({"params":{"preset":"black_white"},"output":{"work_path":true}})).unwrap();
    let after = &s.active().unwrap().doc;
    assert_eq!(after.layers.len(), before.layers.len());
    assert_eq!(after.paths.len(), 1);
    assert_eq!(after.paths[0].name, out["path"].as_str().unwrap());
    assert_eq!(after.work_path.as_ref(), Some(&after.paths[0].path));
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.execute("layer.vectorize", json!({"params":pixel_params(),"output":{"work_path":true}})).is_err());
}
#[test]
fn trace28_preview_latest_request_wins_cancel_and_editable_snapshot() {
    let mut s = pixels();
    let before = s.active().unwrap().doc.clone();
    let started = s.start("layer.vectorize.preview", json!({"scale":0.5,"params":pixel_params()})).unwrap();
    let crate::jobs::Started::Job(old) = started else { panic!("worker") };
    let crate::jobs::Started::Job(new) = s.start("layer.vectorize.preview", json!({"scale":0.75,"params":pixel_params()})).unwrap() else { panic!("worker") };
    assert_eq!(s.jobs_with_recent().into_iter().find(|j| j.id == old).unwrap().state, "cancelled");
    assert!(s.wait_job(old).is_err());
    assert!(s.job(new).unwrap().document.is_none());
    assert!(s.is_enabled("layer.setProps"));
    s.execute("layer.setProps", json!({"name":"Edited"})).unwrap();
    let out = s.wait_job(new).unwrap();
    assert_eq!(out["trace"]["width"], 24);
    assert_eq!(out["capture"]["revision"], 1);
    assert_eq!(s.active().unwrap().doc.layers.len(), before.layers.len());
    assert_eq!(s.journal.len(), 1, "preview jobs never journal");
}
#[test]
fn trace28_cancel_apply_and_stale_revision_targets_document_never_publish() {
    let mut s = pixels();
    let before = s.active().unwrap().doc.clone();
    let request = capture(&s, None).unwrap();
    let crate::jobs::Started::Job(id) = s.start("layer.vectorize", json!({"params":pixel_params()})).unwrap() else { panic!("worker") };
    assert!(s.cancel_job(id));
    assert!(s.wait_job(id).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    let path = photocraft_vector::shapes::rect(1., 1., 4., 4.);
    let p = synthetic(&s, path);
    let prepared = prepare(&before, &p, &Output::default(), &JobCtx::new()).unwrap();
    s.edit("Concurrent edit", |doc, _| {
        doc.name = "New name".into();
        Ok(())
    })
    .unwrap();
    let edited = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    assert!(publish(&mut s, &request, prepared).is_err());
    assert_eq!(s.active().unwrap().doc, edited);
    assert_eq!(s.active().unwrap().revision, rev);
    assert!(s.execute("layer.vectorize", json!({"layer":request.layer.0,"capture":request,"params":pixel_params()})).is_err());
    let fresh = capture(&s, None).unwrap();
    s.active_mut().unwrap().selected_layers.push(LayerId(u64::MAX));
    assert!(check(&s, &fresh).is_err());
    s.add_document((*before).clone(), None);
    assert!(check(&s, &fresh).is_err());
}
#[test]
fn trace28_cached_smart_selection_odd_preview_coordinates_and_alpha() {
    let mut s = pixels();
    let st = s.active_mut().unwrap();
    let doc = Arc::make_mut(&mut st.doc);
    let pixels = Surface::with_default(PixelFormat::RGBA8, &[0., 0., 0., 1.]);
    let layer = Layer::new(
        "Cached smart",
        LayerContent::Smart(SmartObject::new(SmartSource::Linked { path: "/missing-trace28.png".into() }, Affine::IDENTITY, Some(pixels))),
    );
    let id = layer.id;
    doc.layers.push(layer);
    st.active_layer = Some(id);
    st.selected_layers = vec![id];
    let mut selection = Surface::new(PixelFormat::GRAY8);
    selection.fill_rect(Rect::from_xywh(7, 9, 5, 7), &[1.]);
    doc.selection = Some(selection);
    let capture = capture(&s, None).unwrap();
    let params = Params::for_preset(Preset::PixelArt);
    let p = trace(&s.active().unwrap().doc, &capture, &params, &Output::default(), 0.5, &JobCtx::new()).unwrap();
    assert_eq!(p.bounds, [7, 9, 5, 7]);
    assert_eq!(p.scale_xy, [0.6, 4. / 7.]);
    let prepared = prepare(&s.active().unwrap().doc, &p, &Output::default(), &JobCtx::new()).unwrap();
    let LayerContent::Shape(shape) = &prepared.children[0].content else { panic!("shape") };
    let bounds = shape.path.control_bounds().unwrap();
    for (actual, expected) in [bounds.0, bounds.1, bounds.2, bounds.3].into_iter().zip([7., 9., 12., 16.]) {
        assert!((actual - expected).abs() < 1e-6, "actual bounds {bounds:?}");
    }
    let area: f32 = photocraft_vector::path_coverage(&shape.path, s.active().unwrap().doc.bounds()).iter().sum();
    assert!((area - 35.).abs() < 1e-5);
}
#[test]
fn trace28_palette_merge_remove_and_fixed_document_swatches() {
    let s = pixels();
    let c = capture(&s, None).unwrap();
    let params = Params::for_preset(Preset::PixelArt);
    let output = Output { palette_edits: vec![PaletteEdit { from: [255, 0, 0], to: Some([0, 0, 255]) }], ..Default::default() };
    let p = trace(&s.active().unwrap().doc, &c, &params, &output, 1., &JobCtx::new()).unwrap();
    assert_eq!(p.trace.layers.len(), 1);
    assert_eq!(p.trace.layers[0].color, [0, 0, 255, 255]);
    assert_eq!(&p.result_rgba[(3 * 32 + 3) * 4..(3 * 32 + 3) * 4 + 4], &[0, 0, 255, 255]);
    let remove = Output { palette_edits: vec![PaletteEdit { from: [255, 0, 0], to: None }], ..Default::default() };
    let p = trace(&s.active().unwrap().doc, &c, &params, &remove, 1., &JobCtx::new()).unwrap();
    assert_eq!(p.trace.layers.len(), 1);
    let mut params = params;
    params.palette = vec![[0, 255, 0]];
    let p = trace(&s.active().unwrap().doc, &c, &params, &Output::default(), 1., &JobCtx::new()).unwrap();
    assert_eq!(p.trace.layers[0].color, [0, 255, 0, 255]);
}
#[test]
fn trace28_invalid_empty_nonpixel_and_locked_destination_no_mutation() {
    let mut s = pixels();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    for params in [
        json!({"scale":0}),
        json!({"scale":1e300}),
        json!({"output":{"expand":"yes"}}),
        json!({"params":{"detail":-1}}),
        json!({"params":{"speckle":16777217}}),
    ] {
        assert!(s.execute("layer.vectorize.preview", params).is_err());
        assert_eq!(s.active().unwrap().doc, before);
        assert_eq!(s.active().unwrap().revision, revision);
    }
    let empty = synthetic(&s, Path::default());
    let ctx = JobCtx::new();
    let prepared = prepare(&before, &empty, &Output::default(), &ctx).unwrap();
    assert!(publish(&mut s, &empty.capture, prepared).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    let mut doc = (*before).clone();
    let child = doc.layers.remove(0);
    let mut group = Layer::group("Locked", vec![child]);
    group.locks.all = true;
    doc.layers.push(group);
    s.active_mut().unwrap().doc = Arc::new(doc);
    assert!(capture(&s, None).is_err());
    s.active_mut().unwrap().active_layer = Some(s.active().unwrap().doc.layers[0].id);
    assert!(!s.is_enabled("layer.vectorize"));
    assert!(s.disabled_reason("layer.vectorize").unwrap().contains("pixel layer"));
}

#[test]
fn trace28_typed_preview_pixels_bypass_json_history_and_cancelled_mailbox() {
    let mut s = pixels();
    let capture = capture(&s, None).unwrap();
    let params = Params::for_preset(Preset::PixelArt);
    let (started, old_rx) = start_preview(&mut s, &capture, &params, &Output::default(), 0.5).unwrap();
    let crate::jobs::Started::Job(old) = started else { panic!("worker") };
    let (started, new_rx) = start_preview(&mut s, &capture, &params, &Output::default(), 1.).unwrap();
    let crate::jobs::Started::Job(new) = started else { panic!("worker") };
    drop(old_rx);
    assert!(s.wait_job(old).is_err());
    let summary = s.wait_job(new).unwrap();
    assert!(summary.get("source_rgba").is_none());
    assert!(summary.get("result_rgba").is_none());
    assert!(summary.get("trace").is_none());
    let result = new_rx.recv().unwrap();
    assert_eq!(result.source_rgba.len(), 32 * 24 * 4);
    assert_eq!(result.result_rgba.len(), 32 * 24 * 4);
    assert_eq!(result.scale, 1.);
    assert_eq!(result.capture, capture);
    assert!(s.journal.is_empty());
    assert!(!s.undo());
    // Invalid requests also remove the registered sink.
    assert!(start_preview(&mut s, &capture, &params, &Output::default(), 0.).is_err());
}

#[test]
fn trace28_selection_clip_survives_ignoring_transparent_source_alpha() {
    let mut s = pixels();
    let st = s.active_mut().unwrap();
    let doc = Arc::make_mut(&mut st.doc);
    // Transparent black pixels, selected as a 3x3 ring. Ignore source alpha should
    // reveal the black ring, never its unselected centre or bounding rectangle.
    let mut selection = Surface::new(PixelFormat::GRAY8);
    selection.fill_rect(Rect::from_xywh(12, 12, 3, 3), &[1.]);
    selection.write_pixel(13, 13, &[0.]);
    doc.selection = Some(selection);
    let mut params = Params::for_preset(Preset::PixelArt);
    params.use_alpha = false;
    let c = capture(&s, None).unwrap();
    let p = trace(&s.active().unwrap().doc, &c, &params, &Output::default(), 1., &JobCtx::new()).unwrap();
    let prepared = prepare(&s.active().unwrap().doc, &p, &Output::default(), &JobCtx::new()).unwrap();
    let group = Layer::group("Preview", prepared.children);
    let image = photocraft_compose::render_layer(&group, s.active().unwrap().doc.bounds());
    assert_eq!(image.get(13, 13)[3], 0.);
    assert_eq!(image.get(12, 12)[3], 1.);
    assert_eq!(image.get(11, 12)[3], 0.);
    assert!((image.px.iter().map(|p| p[3]).sum::<f32>() - 8.).abs() < 1e-6);
    assert!(p.source_rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0), "before view retains original transparent pixels");
}
