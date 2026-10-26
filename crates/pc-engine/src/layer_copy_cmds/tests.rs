use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, LayerContent, LayerId, MAX_GROUP_DEPTH, Pattern};
use photocraft_geom::Rect;
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::Session;

/// Document 0 (`w`×`h`, `depth`) with a red 10×10 square at (4, 6) on layer "paint" above the
/// Background, and document 1 (60×50, 8-bit RGB). Document 0 is active.
fn two_docs(w: u32, h: u32, depth: u32) -> (Session, LayerId) {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": w, "height": h, "depth": depth})).unwrap();
    let paint = LayerId(s.execute("layer.new.layer", json!({"name": "paint"})).unwrap()["layer"].as_u64().unwrap());
    s.execute("select.rect", json!({"x": 4, "y": 6, "width": 10, "height": 10})).unwrap();
    s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
    s.execute("select.deselect", json!({})).unwrap();
    s.execute("file.new", json!({"width": 60, "height": 50})).unwrap();
    s.set_active(0);
    (s, paint)
}

fn doc(s: &Session, i: usize) -> &Document {
    &s.documents()[i].doc
}

fn copied(r: &Value) -> LayerId {
    LayerId(r["layers"][0].as_u64().unwrap())
}

fn bounds(s: &Session, i: usize, id: LayerId) -> Rect {
    crate::layer_multi_cmds::layer_bounds(doc(s, i).layer(id).unwrap()).unwrap()
}

#[test]
fn copies_the_selected_layers_above_the_destinations_active_layer_in_one_step() {
    let (mut s, paint) = two_docs(40, 30, 8);
    let source_before = doc(&s, 0).clone();
    let r = s.execute("layer.copyToDocument", json!({"document": 1})).unwrap();
    assert_eq!(r["document"], 1);
    assert_eq!(s.active_index(), Some(1), "the destination becomes the active document");
    let d = doc(&s, 1);
    let id = copied(&r);
    assert_ne!(id, paint, "a copy has its own id");
    assert_eq!(d.layers.len(), 2);
    assert_eq!(d.layers[1].id, id, "above the Background, the destination's active layer");
    assert_eq!(d.layers[1].name, "paint");
    assert_eq!(s.active().unwrap().active_layer, Some(id));
    assert_eq!(bounds(&s, 1, id), Rect::new(4, 6, 14, 16), "same canvas position by default");
    assert_eq!(d.layers[1].surface().unwrap().read_region(Rect::new(5, 7, 6, 8)), vec![1.0, 0.0, 0.0, 1.0]);
    assert_eq!(*doc(&s, 0), source_before, "the source is untouched");
    // One history step in the destination.
    assert!(s.undo());
    assert_eq!(doc(&s, 1).layers.len(), 1);
}

#[test]
fn converts_to_the_destinations_depth_and_colour_mode() {
    for depth in [8, 16, 32] {
        let (mut s, _) = two_docs(40, 30, depth);
        s.set_active(1);
        s.execute("image.mode.grayscale", json!({})).unwrap();
        s.execute("image.mode.bits16", json!({})).unwrap();
        s.set_active(0);
        let r = s.execute("layer.copyToDocument", json!({"document": 1})).unwrap();
        let id = copied(&r);
        let surf = doc(&s, 1).layer(id).unwrap().surface().unwrap();
        assert_eq!(surf.format(), PixelFormat::new(ColorMode::Grayscale, SampleType::U16, true), "from {depth}-bit RGB");
        let px = surf.read_region(Rect::new(5, 7, 6, 8));
        assert!(px[0] > 0.1 && px[0] < 0.9 && px[1] == 1.0, "red as a mid grey, opaque: {px:?}");
    }
}

#[test]
fn places_the_copies_centred_on_the_canvas_on_a_point_or_offset() {
    let (mut s, paint) = two_docs(40, 30, 8);
    let place = |s: &mut Session, p: Value| {
        s.set_active(0);
        let mut p = p;
        p["document"] = json!(1);
        p["layers"] = json!([paint.0]);
        let r = s.execute("layer.copyToDocument", p).unwrap();
        bounds(s, 1, copied(&r))
    };
    assert_eq!(place(&mut s, json!({"center": true})), Rect::new(25, 20, 35, 30));
    assert_eq!(place(&mut s, json!({"at": [10.0, 40.0]})), Rect::new(5, 35, 15, 45));
    assert_eq!(place(&mut s, json!({"offset": [-4, 20]})), Rect::new(0, 26, 10, 36));
    assert_eq!(doc(&s, 1).layers.len(), 4);
}

#[test]
fn a_copied_background_is_an_ordinary_layer_and_several_layers_keep_their_order() {
    let (mut s, paint) = two_docs(40, 30, 8);
    let bg = doc(&s, 0).layers[0].id;
    let r = s.execute("layer.copyToDocument", json!({"document": 1, "layers": [paint.0, bg]})).unwrap();
    let d = doc(&s, 1);
    let names: Vec<&str> = d.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Background", "Layer 1", "paint"]);
    assert!(!d.layers[1].locks.transparency && !d.layers[1].locks.position);
    assert_eq!(r["layers"].as_array().unwrap().len(), 2);
    assert_eq!(s.active().unwrap().selected_layers().len(), 2, "the copies are selected");
}

#[test]
fn the_sources_patterns_come_along_once() {
    let (mut s, _) = two_docs(40, 30, 8);
    let mut tile = Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::U8, true));
    tile.fill_rect(Rect::new(0, 0, 2, 2), &[0.0, 1.0, 0.0, 1.0]);
    let pat = Pattern::new("dots", tile, 2, 2);
    s.edit("add pattern", |d, _| {
        d.patterns.push(pat.clone());
        Ok(())
    })
    .unwrap();
    s.execute("layer.copyToDocument", json!({"document": 1})).unwrap();
    assert_eq!(doc(&s, 1).patterns.iter().filter(|p| p.id == pat.id).count(), 1);
    s.set_active(0);
    s.execute("layer.copyToDocument", json!({"document": 1})).unwrap();
    assert_eq!(doc(&s, 1).patterns.iter().filter(|p| p.id == pat.id).count(), 1, "not twice");
}

#[test]
fn bad_params_and_states_fail_without_changing_anything() {
    let (mut s, _) = two_docs(40, 30, 8);
    let cmd = "layer.copyToDocument";
    for p in [
        json!({}),
        json!({"document": 0}),
        json!({"document": 7}),
        json!({"document": -1}),
        json!({"document": "1"}),
        json!({"document": 1, "source": 9}),
        json!({"document": 1, "layers": [999_999]}),
        json!({"document": 1, "layers": "all"}),
        json!({"document": 1, "layers": []}),
        json!({"document": 1, "at": [1.0]}),
        json!({"document": 1, "at": [f64::MAX, 0.0]}),
        json!({"document": 1, "offset": ["a", 2]}),
        Value::Null,
    ] {
        assert!(s.execute(cmd, p.clone()).is_err(), "{p}");
        assert_eq!(s.active_index(), Some(0), "{p}: the source stays active");
        assert_eq!(doc(&s, 1).layers.len(), 1, "{p}");
    }
    // An Indexed destination has no layers to add to.
    s.set_active(1);
    s.edit("indexed", |d, _| {
        d.mode = ColorMode::Indexed;
        Ok(())
    })
    .unwrap();
    s.set_active(0);
    assert!(s.execute(cmd, json!({"document": 1})).is_err());
    // A single open document: nothing to copy to.
    s.close(1);
    assert!(!s.is_enabled(cmd));
}

#[test]
fn a_copied_group_cannot_nest_the_destination_past_the_group_depth_limit() {
    let (mut s, paint) = two_docs(40, 30, 8);
    // A group in the source, and a destination whose active layer sits at the deepest legal level.
    s.execute("layer.groupLayers", json!({"layer": paint.0})).unwrap();
    let group = s.active().unwrap().active_layer.unwrap();
    s.set_active(1);
    let mut inner = None;
    s.edit("nest", |doc, active| {
        let mut l = Layer::raster("deep", doc.pixel_format());
        inner = Some(l.id);
        for _ in 0..MAX_GROUP_DEPTH {
            l = Layer::group("g", vec![l]);
        }
        doc.layers.push(l);
        *active = inner;
        Ok(())
    })
    .unwrap();
    s.set_active(0);
    let before = doc(&s, 1).clone();
    let e = s.execute("layer.copyToDocument", json!({"document": 1, "layers": [group.0]})).unwrap_err();
    assert!(e.to_string().contains("deeper than"), "{e}");
    assert_eq!(*doc(&s, 1), before, "nothing was copied");
    assert_eq!(s.active_index(), Some(0), "the source stays active");
}

/// Red 20×20 rectangles with drop shadows at x = `xs`, y = 20, on a new transparent
/// `width`×`height` 8-bit document, grouped when there are several. Returns the shape ids.
fn shapes_doc(s: &mut Session, width: u32, height: u32, xs: &[i64]) -> Vec<LayerId> {
    s.execute("file.new", json!({"width": width, "height": height, "background": "transparent"})).unwrap();
    let mut ids = Vec::new();
    for x in xs {
        s.execute("shape.create", json!({"kind": "rect", "rect": [x, 20, 20, 20], "fill": "#ff0000"})).unwrap();
        s.execute("layer.layerStyle.dropShadow", json!({"size": 4, "distance": 3})).unwrap();
        ids.push(s.active().unwrap().active_layer.unwrap());
    }
    if ids.len() > 1 {
        s.active_mut().unwrap().selected_layers = ids.clone();
        s.execute("layer.groupLayers", json!({})).unwrap();
    }
    ids
}

/// Every shape of the active document has a non-empty cache rendered for its canvas and depth.
fn assert_shapes_render_for_the_canvas(s: &Session, count: usize, what: &str) {
    let st = s.active().unwrap();
    let shapes: Vec<_> =
        st.doc.walk().into_iter().filter_map(|(_, _, layer)| if let LayerContent::Shape(shape) = &layer.content { Some(shape) } else { None }).collect();
    assert_eq!(shapes.len(), count, "{what}");
    for shape in shapes {
        let cache = shape.cache.as_ref().unwrap_or_else(|| panic!("{what}: no cache"));
        assert_eq!(cache.format().sample, st.doc.depth, "{what}");
        assert!(!cache.content_bounds().is_empty(), "{what}: the cache is clipped to the source's canvas");
        let fresh = photocraft_vector::render_shape(shape, st.doc.pixel_format(), st.doc.bounds());
        assert!(*cache == fresh, "{what}: the cache differs from a fresh render on the destination");
    }
}

#[test]
fn a_shape_group_centred_on_a_larger_document_renders_on_its_canvas() {
    for depth in [8, 16] {
        let mut s = Session::new();
        shapes_doc(&mut s, 100, 100, &[10, 50]);
        s.execute("file.new", json!({"width": 800, "height": 600, "depth": depth})).unwrap();
        s.execute("layer.copyToDocument", json!({"source": 0, "document": 1, "center": true})).unwrap();
        let st = s.active().unwrap();
        let group = st.doc.layer(st.active_layer.unwrap()).unwrap();
        assert!(!crate::transform_cmds::transform_bounds(&st.doc, group).is_empty(), "{depth}-bit: Free Transform has bounds");
        assert_eq!(group.children().unwrap().len(), 2);
        for child in group.children().unwrap() {
            let LayerContent::Shape(shape) = &child.content else { panic!("shape") };
            assert!(shape.cache.as_ref().unwrap().content_bounds().x0 > 100, "{depth}-bit: beyond the source's canvas");
        }
        assert_shapes_render_for_the_canvas(&s, 2, &format!("{depth}-bit"));
        let before = s.active().unwrap().doc.clone();
        s.execute("layer.translate", json!({"dx": 10, "dy": 10})).unwrap();
        assert_ne!(s.active().unwrap().doc, before);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
    }
}

#[test]
fn a_shape_kept_in_place_on_a_larger_document_is_not_cut_at_the_sources_edge() {
    let mut s = Session::new();
    // 90..110 across: the source's canvas ends at 100.
    shapes_doc(&mut s, 100, 100, &[90]);
    s.execute("file.new", json!({"width": 800, "height": 600, "depth": 16})).unwrap();
    let r = s.execute("layer.copyToDocument", json!({"source": 0, "document": 1})).unwrap();
    assert_eq!(r["offset"], json!([0, 0]));
    assert_shapes_render_for_the_canvas(&s, 1, "in place");
    let st = s.active().unwrap();
    let LayerContent::Shape(shape) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("shape") };
    assert!(shape.cache.as_ref().unwrap().content_bounds().x1 > 100, "the part past the source's edge shows");
}

#[test]
fn moved_type_renders_at_the_destinations_depth() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 100, "height": 100, "background": "transparent"})).unwrap();
    s.execute("type.create", json!({"x": 10, "y": 30, "text": "Hello", "size": 12, "color": "#000000"})).unwrap();
    s.execute("file.new", json!({"width": 800, "height": 600, "depth": 16})).unwrap();
    s.execute("layer.copyToDocument", json!({"source": 0, "document": 1, "center": true})).unwrap();
    let st = s.active().unwrap();
    let LayerContent::Text(t) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("type") };
    let cache = t.cache.as_ref().unwrap();
    assert_eq!(cache.format().sample, st.doc.depth, "re-rendered for the 16-bit destination");
    assert!(cache.content_bounds().x0 > 100, "centred beyond the source's canvas");
}

#[test]
fn shapes_copied_to_a_smaller_document_are_placed_by_their_source_bounds() {
    for x in [90, 700] {
        for grouped in [false, true] {
            for missing_cache in [false, true] {
                for depth in [8, 16] {
                    for at in [None, Some([40, 60])] {
                        let what = format!("x={x}, grouped={grouped}, cache missing={missing_cache}, depth={depth}, at={at:?}");
                        let mut s = Session::new();
                        let xs: &[i64] = if grouped { &[x, x + 30] } else { &[x] };
                        let ids = shapes_doc(&mut s, 800, 600, xs);
                        if missing_cache {
                            s.edit("drop cached shapes", |doc, _| {
                                for id in &ids {
                                    let LayerContent::Shape(shape) = &mut doc.layer_mut(*id).unwrap().content else { panic!("shape") };
                                    shape.cache = None;
                                }
                                Ok(())
                            })
                            .unwrap();
                        }
                        s.execute("file.new", json!({"width": 100, "height": 100, "depth": depth, "background": "transparent"})).unwrap();
                        let before = s.active().unwrap().doc.clone();
                        let mut params = json!({"source": 0, "document": 1, "center": true});
                        if let Some(at) = at {
                            params["center"] = json!(false);
                            params["at"] = json!(at);
                        }
                        let result = s.execute("layer.copyToDocument", params).unwrap();
                        let target = at.unwrap_or([50, 50]);
                        let dx = target[0] - (x + if grouped { 25 } else { 10 });
                        let dy = target[1] - 30;
                        assert_eq!(result["offset"], json!([dx, dy]), "{what}");
                        assert_shapes_render_for_the_canvas(&s, xs.len(), &what);
                        let after = s.active().unwrap().doc.clone();
                        assert!(s.undo());
                        assert_eq!(s.active().unwrap().doc, before, "{what}");
                        assert!(s.redo());
                        assert_eq!(s.active().unwrap().doc, after, "{what}");
                    }
                }
            }
        }
    }
}
