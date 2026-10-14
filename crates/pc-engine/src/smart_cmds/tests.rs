use super::*;

const DEPTHS: [u64; 3] = [8, 16, 32];
const W: i32 = 64;
const H: i32 = 48;

fn session(depth: u64) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": W, "height": H, "depth": depth})).unwrap();
    s.execute("layer.new.layer", json!({})).unwrap();
    s
}

/// Paints a textured block on the active layer, partly outside the canvas (top-left), so
/// conversion has to keep off-canvas pixels and crop/translate correctly.
fn paint(s: &mut Session) {
    s.edit("setup", |doc, active| {
        let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
        let fmt = surf.format();
        let r = Rect::new(-6, 5, 40, 38);
        let mut data = Vec::new();
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                let v = ((x * 7 + y * 13) % 23) as f32 / 22.0;
                let a = if (x + y) % 5 == 0 { 0.5 } else { 1.0 };
                data.extend(photocraft_raster::from_rgba(&fmt, [v, 1.0 - v, ((x * y) % 7) as f32 / 6.0, a]));
            }
        }
        surf.write_region(r, &data);
        Ok(())
    })
    .unwrap();
}

fn flat(s: &Session) -> Vec<[f32; 4]> {
    photocraft_compose::flatten(&s.active().unwrap().doc).px
}

fn max_diff(a: &[[f32; 4]], b: &[[f32; 4]]) -> f32 {
    a.iter().zip(b).flat_map(|(p, q)| (0..4).map(move |c| (p[c] - q[c]).abs())).fold(0.0, f32::max)
}

fn mean_diff(a: &[[f32; 4]], b: &[[f32; 4]]) -> f32 {
    let sum: f32 = a.iter().zip(b).map(|(p, q)| (0..4).map(|c| (p[c] - q[c]).abs()).sum::<f32>()).sum();
    sum / (a.len() * 4) as f32
}

fn active_smart(s: &Session) -> SmartObject {
    let d = s.active().unwrap();
    match &d.doc.layer(d.active_layer.unwrap()).unwrap().content {
        LayerContent::Smart(sm) => sm.clone(),
        other => panic!("not smart: {}", other.kind_name()),
    }
}

fn convert(s: &mut Session) -> u64 {
    s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap()["layer"].as_u64().unwrap()
}

#[test]
fn conversion_is_pixel_identical_at_every_depth() {
    for depth in DEPTHS {
        let mut s = session(depth);
        paint(&mut s);
        let id = s.active().unwrap().active_layer.unwrap();
        s.edit("opacity", |doc, _| {
            doc.layer_mut(id).unwrap().opacity = 0.6;
            Ok(())
        })
        .unwrap();
        let before = flat(&s);
        convert(&mut s);
        assert_eq!(max_diff(&flat(&s), &before), 0.0, "depth {depth}");
        let d = s.active().unwrap();
        let l = d.doc.layer(d.active_layer.unwrap()).unwrap();
        assert!((l.opacity - 0.6).abs() < 1e-6, "opacity stays on the smart layer");
        let sm = active_smart(&s);
        let SmartSource::Embedded { file_name, bytes } = &sm.source else { panic!() };
        assert!(file_name.ends_with(".pcraft"));
        // The nested document is cropped to the content, including the off-canvas part.
        let inner = decode_source(file_name, bytes).unwrap();
        assert_eq!((inner.size.width, inner.size.height), (46, 33));
        assert_eq!(inner.depth, d.doc.depth);
        assert_eq!(sm.transform, Affine::translate(-6.0, 5.0));
        // Re-rendering from the source reproduces the conversion cache exactly.
        let rendered = render(&d.doc, &sm).unwrap().unwrap();
        let b = Rect::new(-6, 5, 40, 38);
        assert_eq!(rendered.read_region(b), sm.cache.as_ref().unwrap().read_region(b), "depth {depth}");
        s.undo();
        assert_eq!(flat(&s), before);
    }
}

#[test]
fn masked_layer_and_group_convert_within_rounding() {
    let mut s = session(8);
    paint(&mut s);
    s.edit("mask", |doc, active| {
        let l = doc.layer_mut(active.unwrap()).unwrap();
        let mut m = LayerMask::reveal_all();
        m.surface.fill_rect(Rect::new(0, 0, 20, 20), &[0.3]);
        l.mask = Some(m);
        Ok(())
    })
    .unwrap();
    let before = flat(&s);
    convert(&mut s);
    assert!(max_diff(&flat(&s), &before) <= 1.0 / 255.0 + 1e-6);
    // A group converts too (its children move into the nested document).
    s.execute("layer.new.layer", json!({})).unwrap();
    paint(&mut s);
    s.execute("layer.groupLayers", json!({})).unwrap();
    let before = flat(&s);
    convert(&mut s);
    assert!(max_diff(&flat(&s), &before) <= 1.0 / 255.0 + 1e-6);
}

#[test]
fn empty_layer_cannot_be_converted() {
    let mut s = session(8);
    assert!(s.execute("layer.smartObjects.convertToSmartObject", json!({})).is_err());
}

#[test]
fn smart_filters_are_re_editable() {
    for depth in [8, 16] {
        let mut s = session(depth);
        paint(&mut s);
        let original = flat(&s);
        s.execute("filter.convertForSmartFilters", json!({})).unwrap();
        assert!(!s.is_enabled("filter.convertForSmartFilters"), "already smart");
        s.execute("filter.blur.gaussianBlur", json!({"radius": 2})).unwrap();
        let r2 = flat(&s);
        assert!(max_diff(&r2, &original) > 0.05);
        let sm = active_smart(&s);
        assert_eq!(sm.smart_filters.len(), 1);
        let SmartSource::Embedded { bytes, .. } = &sm.source else { panic!() };
        // Change the radius: the result comes from the source, not from the blurred pixels.
        s.execute("layer.smartFilter.setParams", json!({"index": 0, "params": {"radius": 6}})).unwrap();
        let r6 = flat(&s);
        assert!(max_diff(&r6, &r2) > 0.01);
        s.execute("layer.smartFilter.setParams", json!({"params": {"radius": 2}})).unwrap();
        assert!(max_diff(&flat(&s), &r2) < 1e-6, "same params, same pixels");
        // Hiding the filter shows the original exactly.
        s.execute("layer.smartFilter.setVisible", json!({"index": 0})).unwrap();
        assert_eq!(flat(&s), original);
        assert!(!active_smart(&s).smart_filters[0].visible);
        s.undo();
        assert!(max_diff(&flat(&s), &r2) < 1e-6);
        s.redo();
        assert_eq!(flat(&s), original);
        // All of that decoded the contents at most once (the conversion seeded the cache).
        assert!(source_decode_count(bytes, s.active().unwrap().doc.pixel_format()) <= 1);
    }
}

#[test]
fn filter_stack_order_blend_disable_and_clear() {
    let mut s = session(8);
    paint(&mut s);
    let original = flat(&s);
    convert(&mut s);
    s.execute("filter.other.offset", json!({"horizontal": 5, "undefinedAreas": "transparent"})).unwrap();
    s.execute("filter.blur.gaussianBlur", json!({"radius": 3})).unwrap();
    let ab = flat(&s);
    s.execute("layer.smartFilter.move", json!({"index": 1, "to": 0})).unwrap();
    assert_eq!(active_smart(&s).smart_filters[0].command, "filter.blur.gaussianBlur");
    assert!(max_diff(&flat(&s), &ab) > 1e-3, "order matters");
    s.undo();
    assert_eq!(flat(&s), ab);
    // 0 % opacity on both filters = original.
    s.execute("layer.smartFilter.blendingOptions", json!({"index": 0, "opacity": 0})).unwrap();
    s.execute("layer.smartFilter.blendingOptions", json!({"index": 1, "opacity": 0, "blend": "multiply"})).unwrap();
    assert!(max_diff(&flat(&s), &original) < 1e-6);
    assert_eq!(active_smart(&s).smart_filters[1].blend, BlendMode::Multiply);
    assert!(s.execute("layer.smartFilter.blendingOptions", json!({"blend": "nope"})).is_err());
    s.undo();
    s.undo();
    s.execute("layer.smartFilter.disableSmartFilters", json!({})).unwrap();
    assert_eq!(flat(&s), original);
    assert!(!active_smart(&s).filters_enabled);
    s.execute("layer.smartFilter.disableSmartFilters", json!({})).unwrap();
    assert_eq!(flat(&s), ab);
    s.execute("layer.smartFilter.delete", json!({"index": 0})).unwrap();
    assert_eq!(active_smart(&s).smart_filters.len(), 1);
    assert!(s.execute("layer.smartFilter.delete", json!({"index": 5})).is_err());
    s.execute("layer.smartFilter.clearSmartFilters", json!({})).unwrap();
    assert_eq!(flat(&s), original);
    assert!(!s.is_enabled("layer.smartFilter.clearSmartFilters"));
}

/// Issue #466: an explicit `layer` target is checked instead of the active layer, which stays
/// active; without one the commands still follow the active layer (menus).
#[test]
fn smart_filter_commands_check_an_explicit_layer_target() {
    let mut s = session(8);
    paint(&mut s);
    let so = convert(&mut s);
    s.execute("filter.sharpen.smartSharpen", json!({"amount": 65, "radius": 1})).unwrap();
    let curves = s.execute("layer.newAdjustmentLayer.curves", json!({"points": [[0, 0], [255, 255]]})).unwrap()["layer"].as_u64().unwrap();
    let active = |s: &Session| s.active().unwrap().active_layer.unwrap().0;
    assert_eq!(active(&s), curves);
    let filter = |s: &Session| match &s.active().unwrap().doc.layer(LayerId(so)).unwrap().content {
        LayerContent::Smart(sm) => sm.smart_filters[0].clone(),
        other => panic!("not smart: {}", other.kind_name()),
    };
    // No target: the active Curves layer decides, as the menu shows.
    assert!(!s.is_enabled("layer.smartFilter.setParams"));
    assert!(matches!(s.execute("layer.smartFilter.setParams", json!({"params": {"amount": 80}})), Err(EngineError::Disabled(..))));
    let r = s.execute("layer.smartFilter.setParams", json!({"layer": so, "index": 0, "params": {"amount": 80}})).unwrap();
    assert_eq!(r, json!({"layer": so}));
    assert_eq!(filter(&s).params["amount"], json!(80));
    s.execute("layer.smartFilter.setVisible", json!({"layer": so, "visible": false})).unwrap();
    assert!(!filter(&s).visible);
    assert_eq!(active(&s), curves, "the active layer stays");
    // A target that isn't a smart object with filters is still refused, and the active layer stays.
    assert!(matches!(s.execute("layer.smartFilter.setParams", json!({"layer": curves, "params": {}})), Err(EngineError::Disabled(..))));
    assert!(s.execute("layer.smartFilter.setParams", json!({"layer": 999_999, "params": {}})).is_err());
    assert_eq!(active(&s), curves);
}

#[test]
fn selection_becomes_the_filter_mask() {
    let mut s = session(8);
    paint(&mut s);
    let original = flat(&s);
    convert(&mut s);
    s.execute("select.rect", json!({"x": 0, "y": 0, "width": 16, "height": 48})).unwrap();
    s.execute("filter.blur.gaussianBlur", json!({"radius": 3})).unwrap();
    assert!(active_smart(&s).filter_mask.is_some());
    let px = flat(&s);
    let at = |v: &Vec<[f32; 4]>, x: i32, y: i32| v[(y * W + x) as usize];
    assert_eq!(at(&px, 30, 20), at(&original, 30, 20), "outside the mask: unfiltered");
    assert_ne!(at(&px, 6, 20), at(&original, 6, 20), "inside the mask: filtered");
    s.execute("layer.smartFilter.disableFilterMask", json!({})).unwrap();
    assert_ne!(at(&flat(&s), 30, 20), at(&original, 30, 20));
    s.execute("layer.smartFilter.deleteFilterMask", json!({})).unwrap();
    assert!(active_smart(&s).filter_mask.is_none());
    assert!(!s.is_enabled("layer.smartFilter.deleteFilterMask"));
}

/// Scales `id` to 10 % around the origin and back, returning the composite.
fn shrink_and_grow(s: &mut Session, id: u64) -> Vec<[f32; 4]> {
    s.execute("edit.transform", json!({"layer": id, "matrix": [0.1, 0, 0, 0.1, 0, 0]})).unwrap();
    s.execute("edit.transform", json!({"layer": id, "matrix": [10, 0, 0, 10, 0, 0]})).unwrap();
    flat(s)
}

#[test]
fn transforms_re_render_from_source_losslessly() {
    let mut raster = session(8);
    paint(&mut raster);
    let original = flat(&raster);
    let rid = raster.active().unwrap().active_layer.unwrap().0;
    let raster_result = shrink_and_grow(&mut raster, rid);

    let mut smart = session(8);
    paint(&mut smart);
    let sid = convert(&mut smart);
    // Scaled down, the smart object really is small...
    smart.execute("edit.transform", json!({"layer": sid, "matrix": [0.1, 0, 0, 0.1, 0, 0]})).unwrap();
    let small = active_smart(&smart).cache.unwrap().content_bounds();
    assert!(small.width() <= 7, "{small:?}");
    // ...and scaled back up it is the original again.
    smart.execute("edit.transform", json!({"layer": sid, "matrix": [10, 0, 0, 10, 0, 0]})).unwrap();
    let smart_result = flat(&smart);
    let (es, er) = (mean_diff(&smart_result, &original), mean_diff(&raster_result, &original));
    assert!(es < 1e-6, "smart error {es}");
    assert!(er > 0.02, "raster error {er}");
    // A non-integer move re-renders with bicubic; moving back is exact again.
    smart.execute("edit.transform", json!({"layer": sid, "matrix": [1, 0, 0, 1, 0.5, 0.25]})).unwrap();
    assert!(max_diff(&flat(&smart), &original) > 0.01);
    smart.execute("edit.transform", json!({"layer": sid, "matrix": [1, 0, 0, 1, -0.5, -0.25]})).unwrap();
    assert!(max_diff(&flat(&smart), &original) < 1e-6);
    // Whole-pixel moves (Move tool) shift without re-rendering.
    smart.execute("layer.translate", json!({"dx": 3, "dy": -2})).unwrap();
    assert_eq!(active_smart(&smart).transform, Affine::translate(-3.0, 3.0));
    // Perspective keeps the full projective placement (and stays a smart object).
    smart.execute("edit.transform", json!({"layer": sid, "quad": [[0, 0], [10, 0], [12, 10], [-2, 10]]})).unwrap();
    assert!(active_smart(&smart).perspective.is_some());
}

#[test]
fn filters_follow_transforms() {
    let mut s = session(16);
    paint(&mut s);
    let id = convert(&mut s);
    s.execute("filter.blur.gaussianBlur", json!({"radius": 2})).unwrap();
    let blurred = flat(&s);
    s.execute("edit.transform", json!({"layer": id, "matrix": [2, 0, 0, 2, 0, 0]})).unwrap();
    assert_eq!(active_smart(&s).smart_filters.len(), 1);
    s.execute("edit.transform", json!({"layer": id, "matrix": [0.5, 0, 0, 0.5, 0, 0]})).unwrap();
    assert!(max_diff(&flat(&s), &blurred) < 1e-6, "filter re-applied to the re-rendered source");
}

#[test]
fn edit_contents_updates_the_parent() {
    let mut s = session(8);
    paint(&mut s);
    let id = convert(&mut s);
    let before = flat(&s);
    let r = s.execute("layer.smartObjects.editContents", json!({})).unwrap();
    let child = r["document"].as_u64().unwrap() as usize;
    assert_eq!(s.active_index(), Some(child));
    assert!(s.is_enabled("layer.smartObjects.saveContents"));
    // Paint the nested document's layer solid red.
    s.edit("paint", |doc, _| {
        let l = &mut doc.layers[0];
        let b = Rect::new(0, 0, 46, 33);
        l.surface_mut().unwrap().fill_rect(b, &[1.0, 0.0, 0.0, 1.0]);
        Ok(())
    })
    .unwrap();
    s.execute("layer.smartObjects.saveContents", json!({})).unwrap();
    assert!(!s.active().unwrap().is_dirty());
    s.set_active(0);
    let after = flat(&s);
    assert_eq!(after[(20 * W + 10) as usize], [1.0, 0.0, 0.0, 1.0]);
    assert_ne!(after, before);
    s.undo();
    assert_eq!(flat(&s), before, "the update is one undoable step in the parent");
    s.redo();
    // Closing an edited contents document also commits it.
    s.set_active(child);
    s.edit("paint2", |doc, _| {
        doc.layers[0].surface_mut().unwrap().fill_rect(Rect::new(0, 0, 46, 33), &[0.0, 0.0, 1.0, 1.0]);
        Ok(())
    })
    .unwrap();
    s.execute("file.close", json!({})).unwrap();
    assert!(s.smart_links.is_empty());
    s.set_active(0);
    assert_eq!(flat(&s)[(20 * W + 10) as usize], [0.0, 0.0, 1.0, 1.0]);
    let d = s.active().unwrap();
    assert!(matches!(d.doc.layer(LayerId(id)).unwrap().content, LayerContent::Smart(_)));
}

#[test]
fn rasterize_and_via_copy() {
    let mut s = session(8);
    paint(&mut s);
    convert(&mut s);
    s.execute("filter.blur.gaussianBlur", json!({"radius": 2})).unwrap();
    let blurred = flat(&s);
    // Via copy: an independent smart object above.
    let copy = s.execute("layer.smartObjects.newSmartObjectViaCopy", json!({})).unwrap()["layer"].as_u64().unwrap();
    assert_eq!(s.active().unwrap().active_layer, Some(LayerId(copy)));
    s.execute("layer.smartFilter.setParams", json!({"params": {"radius": 8}})).unwrap();
    let d = s.active().unwrap();
    let n_smart = d.doc.walk().iter().filter(|(_, _, l)| matches!(l.content, LayerContent::Smart(_))).count();
    assert_eq!(n_smart, 2);
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("layer.smartObjects.rasterize", json!({})).unwrap();
    let d = s.active().unwrap();
    assert!(matches!(d.doc.layer(d.active_layer.unwrap()).unwrap().content, LayerContent::Raster(_)));
    assert!(max_diff(&flat(&s), &blurred) <= 1.0 / 255.0 + 1e-6);
    assert!(!s.is_enabled("layer.smartObjects.rasterize"));
}

#[test]
fn pcraft_round_trip_keeps_live_smart_objects() {
    for depth in [8, 16] {
        let mut s = session(depth);
        paint(&mut s);
        convert(&mut s);
        s.execute("filter.blur.gaussianBlur", json!({"radius": 2})).unwrap();
        s.execute("select.rect", json!({"x": 0, "y": 0, "width": 30, "height": 30})).unwrap();
        s.execute("filter.noise.addNoise", json!({"amount": 20, "seed": 3})).unwrap();
        s.execute("layer.smartFilter.blendingOptions", json!({"opacity": 0.5, "blend": "screen"})).unwrap();
        let doc = s.active().unwrap().doc.clone();
        let bytes = photocraft_format::save_to_bytes(&doc, &Default::default()).unwrap();
        let back = photocraft_format::load_from_bytes(&bytes).unwrap();
        let id = s.active().unwrap().active_layer.unwrap();
        let (a, b) = (smart(&doc, id).unwrap(), smart(&back, id).unwrap());
        assert_eq!(a, b, "depth {depth}");
        assert_eq!(b.smart_filters.len(), 2);
        assert!(b.filter_mask.is_some());
        // Still live after loading: re-editing works and matches a fresh render.
        let mut s2 = Session::new();
        s2.add_document(back, None);
        s2.select_layer(id).unwrap();
        s2.execute("layer.smartFilter.setVisible", json!({"index": 1, "visible": false})).unwrap();
        s.execute("layer.smartFilter.setVisible", json!({"index": 1, "visible": false})).unwrap();
        assert_eq!(flat(&s2), flat(&s));
    }
}

#[test]
fn psd_placed_layer_renders_from_lnk2_data() {
    // A PNG embedded the way Photoshop stores placed files: a global lnk2 block keyed by uuid.
    let mut inner = Document::with_background(
        "in",
        Size::new(10, 8),
        photocraft_color::ColorMode::Rgb,
        photocraft_color::SampleType::U8,
        photocraft_color::Color::rgba(0.0, 1.0, 0.0, 1.0),
    );
    inner.layers[0].surface_mut().unwrap().fill_rect(Rect::new(0, 0, 5, 8), &[1.0, 0.0, 0.0, 1.0]);
    let png = photocraft_io::export(&inner, "png", &Default::default()).unwrap().bytes;
    let lnk2 = photocraft_io::linked::encode_linked_file(&photocraft_io::linked::LinkedFile { uuid: "uuid-1".into(), file_name: "in.png".into(), bytes: png });
    let mut s = session(8);
    s.edit("place", |doc, active| {
        doc.metadata.psd_global_blocks.push((*b"8BIM", *b"lnk2", Arc::new(lnk2)));
        // Photoshop's rendering (deliberately different, to see the re-render happen).
        let mut cache = Surface::new(doc.pixel_format());
        cache.fill_rect(Rect::new(20, 10, 40, 26), &[0.0, 0.0, 1.0, 1.0]);
        let mut sm = SmartObject::new(SmartSource::Linked { path: "uuid-1".into() }, Affine { m: [2.0, 0.0, 0.0, 2.0, 20.0, 10.0] }, Some(cache));
        sm.psd_raw = Some(Arc::new(vec![1, 2, 3]));
        let mut l = Layer::new("placed", LayerContent::Smart(sm));
        l.psd_blocks.push((*b"SoLd", Arc::new(vec![1, 2, 3])));
        *active = Some(doc.insert_above(*active, l));
        Ok(())
    })
    .unwrap();
    // Nothing re-renders until something changes (PSD round trips stay byte-stable).
    assert_eq!(active_smart(&s).cache.unwrap().rgba(25, 12), [0.0, 0.0, 1.0, 1.0]);
    s.execute("filter.other.offset", json!({"horizontal": 0})).unwrap();
    let c = active_smart(&s).cache.unwrap();
    assert_eq!(c.rgba(23, 15), [1.0, 0.0, 0.0, 1.0], "left half red, from the source");
    assert_eq!(c.rgba(37, 15), [0.0, 1.0, 0.0, 1.0]);
    assert!(active_smart(&s).psd_raw.is_some(), "filters alone keep the placed-layer block");
    // Embedding resolves the lnk2 data.
    assert!(s.is_enabled("layer.smartObjects.convertToEmbedded"));
    s.execute("layer.smartObjects.convertToEmbedded", json!({})).unwrap();
    assert!(matches!(active_smart(&s).source, SmartSource::Embedded { ref file_name, .. } if file_name == "in.png"));
}

#[test]
fn export_replace_and_linked_contents() {
    let dir = std::env::temp_dir().join(format!("pcraft-smart-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = session(8);
    paint(&mut s);
    convert(&mut s);
    let out = dir.join("contents.pcraft");
    s.execute("layer.smartObjects.exportContents", json!({"path": out.to_str().unwrap()})).unwrap();
    assert!(photocraft_format::is_pcraft(&std::fs::read(&out).unwrap()));
    // Replace with a flat PNG.
    let png_doc = Document::with_background(
        "p",
        Size::new(4, 4),
        photocraft_color::ColorMode::Rgb,
        photocraft_color::SampleType::U8,
        photocraft_color::Color::rgba(1.0, 1.0, 0.0, 1.0),
    );
    let png = dir.join("yellow.png");
    std::fs::write(&png, photocraft_io::export(&png_doc, "png", &Default::default()).unwrap().bytes).unwrap();
    s.execute("layer.smartObjects.replaceContents", json!({"path": png.to_str().unwrap()})).unwrap();
    let sm = active_smart(&s);
    assert_eq!(sm.cache.as_ref().unwrap().content_bounds(), Rect::new(-6, 5, -2, 9), "transform kept");
    assert!(s.execute("layer.smartObjects.replaceContents", json!({})).is_err());
    // Linked: write out, edit the file, update.
    let linked = dir.join("linked.png");
    s.execute("layer.smartObjects.convertToLinked", json!({"path": linked.to_str().unwrap()})).unwrap();
    assert!(matches!(active_smart(&s).source, SmartSource::Linked { .. }));
    let mut red = png_doc.clone();
    let rb = red.bounds();
    red.layers[0].surface_mut().unwrap().fill_rect(rb, &[1.0, 0.0, 0.0, 1.0]);
    std::fs::write(&linked, photocraft_io::export(&red, "png", &Default::default()).unwrap().bytes).unwrap();
    s.execute("layer.smartObjects.updateAllModifiedContent", json!({})).unwrap();
    assert_eq!(active_smart(&s).cache.unwrap().rgba(-5, 6), [1.0, 0.0, 0.0, 1.0]);
    s.execute("layer.smartObjects.convertToEmbedded", json!({})).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    // Missing linked file: re-rendering fails cleanly and changes nothing.
    s.execute("layer.smartObjects.relinkToFile", json!({"path": dir.join("gone.png").to_str().unwrap()})).unwrap_err();
    assert!(matches!(active_smart(&s).source, SmartSource::Embedded { .. }));
}

#[test]
fn commands_are_registered_with_catalog_ids() {
    for id in [
        "layer.smartObjects.convertToSmartObject",
        "filter.convertForSmartFilters",
        "layer.smartObjects.newSmartObjectViaCopy",
        "layer.smartObjects.rasterize",
        "layer.smartObjects.editContents",
        "layer.smartObjects.replaceContents",
        "layer.smartObjects.exportContents",
        "layer.smartObjects.updateModifiedContent",
        "layer.smartObjects.updateAllModifiedContent",
        "layer.smartObjects.convertToEmbedded",
        "layer.smartObjects.convertToLinked",
        "layer.smartObjects.relinkToFile",
        "layer.smartFilter.disableSmartFilters",
        "layer.smartFilter.clearSmartFilters",
        "layer.smartFilter.blendingOptions",
        "layer.smartFilter.deleteFilterMask",
        "layer.smartFilter.disableFilterMask",
    ] {
        let spec = crate::commands::find(id).unwrap_or_else(|| panic!("{id}"));
        assert!(!spec.menu.is_empty(), "{id}");
    }
    let s = Session::new();
    assert!(!s.is_enabled("layer.smartObjects.convertToSmartObject"));
    assert!(!s.is_enabled("layer.smartObjects.editContents"));
}

#[test]
fn smart_filter_blur_repeats_the_canvas_edge_like_a_layer_filter() {
    for depth in [8, 32] {
        let fill_canvas = |s: &mut Session| {
            s.edit("setup", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                let fmt = surf.format();
                let r = Rect::new(0, 0, W, H);
                let mut data = Vec::new();
                for y in r.y0..r.y1 {
                    for x in r.x0..r.x1 {
                        let v = if (x / 8 + y / 8) % 2 == 0 { 0.2 } else { 0.9 };
                        data.extend(photocraft_raster::from_rgba(&fmt, [v, 0.5, 1.0 - v, 1.0]));
                    }
                }
                surf.write_region(r, &data);
                Ok(())
            })
            .unwrap();
        };
        let mut layer = session(depth);
        fill_canvas(&mut layer);
        layer.execute("filter.blur.gaussianBlur", json!({"radius": 4})).unwrap();
        let mut smart = session(depth);
        fill_canvas(&mut smart);
        convert(&mut smart);
        smart.execute("filter.blur.gaussianBlur", json!({"radius": 4})).unwrap();
        let (a, b) = (flat(&layer), flat(&smart));
        // The corner stays opaque (no transparency fades in from outside the canvas).
        assert!((b[0][3] - 1.0).abs() < 1e-3, "corner alpha {}", b[0][3]);
        assert!(max_diff(&a, &b) < 2.0 / 255.0, "smart re-render matches the layer filter ({depth}-bit)");
    }
}

/// A distorted smart object keeps its fourth corner: through Edit Contents → Save (which
/// re-renders from the source) the placement and the rendered corners don't move.
#[test]
fn distort_survives_edit_contents() {
    let mut s = session(8);
    paint(&mut s);
    let id = convert(&mut s);
    let r = active_smart(&s).cache.unwrap().content_bounds();
    let (x0, y0, x1, y1) = (r.x0, r.y0, r.x1, r.y1);
    // Distort: only the bottom-right corner moves (in by 12, up by 8): not a parallelogram.
    let quad = json!([[x0, y0], [x1, y0], [x1 - 12, y1 - 8], [x0, y1]]);
    s.execute("edit.transform", json!({"layer": id, "rect": [x0, y0, x1, y1], "quad": quad})).unwrap();
    let placed = active_smart(&s);
    assert!(placed.perspective.is_some(), "Distort keeps a projective placement");
    let before = flat(&s);
    s.execute("layer.smartObjects.editContents", json!({})).unwrap();
    s.execute("layer.smartObjects.saveContents", json!({})).unwrap();
    s.set_active(0);
    assert_eq!(active_smart(&s).perspective, placed.perspective, "the placement survives the save");
    assert!(mean_diff(&before, &flat(&s)) < 0.01, "the re-render matches the distorted look");
    // The bottom-right of the original frame is now empty: the corner really moved in.
    let sm = active_smart(&s);
    let c = sm.cache.as_ref().unwrap();
    assert_eq!(c.rgba(x1 - 2, y1 - 2)[3], 0.0, "no pixels where the corner used to be");
}

/// Warp handles on a distorted smart object sit on its four distorted corners (the warp is mapped
/// through the full projective placement, not its affine approximation), and warping it keeps the
/// Distort placement.
#[test]
fn warp_on_a_distorted_smart_object_follows_the_corners() {
    let mut s = session(8);
    paint(&mut s);
    let id = convert(&mut s);
    let r = active_smart(&s).cache.unwrap().content_bounds();
    let (x0, y0, x1, y1) = (f64::from(r.x0), f64::from(r.y0), f64::from(r.x1), f64::from(r.y1));
    let quad = [[x0, y0], [x1, y0], [x1 - 12.0, y1 - 8.0], [x0, y1]];
    s.execute("edit.transform", json!({"layer": id, "rect": [x0, y0, x1, y1], "quad": quad})).unwrap();
    let mut sm = active_smart(&s);
    let h = placement(&sm);
    let inv = h.inverse().unwrap();
    let src: Vec<(f64, f64)> = quad.iter().map(|p| inv.apply(p[0], p[1])).collect();
    let b = src.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)]);
    sm.warp = Some(photocraft_geom::warp::Warp::custom(photocraft_geom::warp::BezierMesh::identity(b, 1, 1), b));
    let layer = Layer::new("w", LayerContent::Smart(sm));
    let doc_warp = crate::warp_cmds::smart_warp_doc_space(&layer).unwrap();
    let pts = doc_warp.mesh.unwrap().points;
    for (q, i) in quad.iter().zip([0usize, 3, 15, 12]) {
        let p = pts[i];
        assert!((p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6, "corner {i}: {p:?} vs {q:?}");
    }
    // Warping the distorted smart object works and keeps its projective placement.
    let before = active_smart(&s).perspective;
    assert!(before.is_some());
    s.execute("edit.transform.warp", json!({"style": "arc", "bend": 30})).unwrap();
    let after = active_smart(&s);
    assert!(after.warp.is_some());
    assert_eq!(after.perspective, before);
}

fn convert_to_layers(s: &mut Session) -> Value {
    s.execute("layer.smartObjects.convertToLayers", json!({})).unwrap()
}

fn active_layer(s: &Session) -> Layer {
    let d = s.active().unwrap();
    d.doc.layer(d.active_layer.unwrap()).unwrap().clone()
}

#[test]
fn convert_to_layers_round_trips_a_layer_at_every_depth() {
    for depth in DEPTHS {
        let mut s = session(depth);
        paint(&mut s);
        let id = s.active().unwrap().active_layer.unwrap();
        s.edit("props", |doc, _| {
            let l = doc.layer_mut(id).unwrap();
            l.name = "Sky".into();
            l.opacity = 0.6;
            l.blend = BlendMode::Multiply;
            Ok(())
        })
        .unwrap();
        let before = flat(&s);
        convert(&mut s);
        let smart = flat(&s);
        let r = convert_to_layers(&mut s);
        assert_eq!(r["group"], false, "one layer replaces the smart object");
        let l = active_layer(&s);
        assert_eq!(l.id.0, r["layer"].as_u64().unwrap());
        assert!(matches!(&l.content, LayerContent::Raster(px) if px.format() == s.active().unwrap().doc.pixel_format()), "depth {depth}");
        assert_eq!((l.name.as_str(), l.blend), ("Sky", BlendMode::Multiply));
        assert!((l.opacity - 0.6).abs() < 1e-6);
        assert_eq!(max_diff(&flat(&s), &before), 0.0, "depth {depth}: whole-pixel placement is exact");
        s.undo();
        assert!(matches!(active_layer(&s).content, LayerContent::Smart(_)), "one undoable step");
        assert_eq!(flat(&s), smart);
    }
}

#[test]
fn convert_to_layers_unpacks_several_layers_into_a_group_with_the_smart_layers_properties() {
    let mut s = session(8);
    paint(&mut s);
    let id = convert(&mut s);
    s.execute("layer.setProps", json!({"layer": id, "name": "Placed", "opacity": 0.5})).unwrap();
    // A second layer inside the contents (Edit Contents, then save).
    let child = s.execute("layer.smartObjects.editContents", json!({})).unwrap()["document"].as_u64().unwrap() as usize;
    s.execute("layer.new.layer", json!({"name": "Top"})).unwrap();
    s.edit("paint", |doc, active| {
        doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap().fill_rect(Rect::new(2, 2, 8, 8), &[0.0, 0.0, 1.0, 1.0]);
        Ok(())
    })
    .unwrap();
    s.execute("layer.smartObjects.saveContents", json!({})).unwrap();
    s.execute("file.close", json!({})).unwrap();
    assert_ne!(s.active_index(), Some(child));
    s.execute("filter.blur.gaussianBlur", json!({"radius": 2})).unwrap();
    s.execute("filter.blur.gaussianBlur", json!({"radius": 1})).unwrap();
    // Without the filters, the smart object looks exactly like the unpacked group.
    s.execute("layer.smartFilter.disableSmartFilters", json!({})).unwrap();
    let before = flat(&s);
    let r = convert_to_layers(&mut s);
    assert_eq!((r["group"].as_bool(), r["discardedSmartFilters"].as_u64()), (Some(true), Some(2)));
    let g = active_layer(&s);
    assert_eq!((g.name.as_str(), g.blend), ("Placed", BlendMode::Normal), "named after the smart object, isolated");
    assert!((g.opacity - 0.5).abs() < 1e-6);
    let names: Vec<_> = g.children().unwrap().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names.last(), Some(&"Top"));
    assert_eq!(names.len(), 2);
    assert!(max_diff(&flat(&s), &before) <= 1.0 / 255.0 + 1e-6);
    assert!(!s.is_enabled("layer.smartObjects.convertToLayers"), "a group isn't a smart object");
}

#[test]
fn convert_to_layers_keeps_a_smart_layer_mask_on_a_group() {
    let mut s = session(16);
    paint(&mut s);
    convert(&mut s);
    s.execute("layer.layerMask.hideAll", json!({})).unwrap();
    let before = flat(&s);
    assert!(convert_to_layers(&mut s)["group"].as_bool().unwrap(), "the mask can't fold into the layer");
    let g = active_layer(&s);
    assert!(g.mask.is_some() && g.children().unwrap()[0].mask.is_none());
    assert_eq!(max_diff(&flat(&s), &before), 0.0);
}

#[test]
fn convert_to_layers_applies_the_placement() {
    let mut s = session(8);
    paint(&mut s);
    let id = convert(&mut s);
    s.execute("edit.transform", json!({"layer": id, "matrix": [0.5, 0, 0, 0.5, 10.25, 3]})).unwrap();
    let before = flat(&s);
    convert_to_layers(&mut s);
    let l = active_layer(&s);
    assert!(matches!(l.content, LayerContent::Raster(_)));
    assert!(mean_diff(&flat(&s), &before) < 2e-3, "{}", mean_diff(&flat(&s), &before));
    // Distort keeps the projective placement for pixels.
    let mut s = session(8);
    paint(&mut s);
    let id = convert(&mut s);
    s.execute("edit.transform", json!({"layer": id, "quad": [[0, 0], [30, 0], [36, 30], [-4, 30]]})).unwrap();
    let before = flat(&s);
    convert_to_layers(&mut s);
    assert!(mean_diff(&flat(&s), &before) < 2e-3);
}

#[test]
fn convert_to_layers_rejects_what_it_cannot_unpack() {
    let mut s = session(8);
    assert!(!s.is_enabled("layer.smartObjects.convertToLayers"));
    assert!(s.execute("layer.smartObjects.convertToLayers", json!({})).is_err());
    paint(&mut s);
    let raster = s.active().unwrap().active_layer.unwrap().0;
    let id = convert(&mut s);
    // Bad targets fail without panicking: a pixel layer, a missing layer.
    for p in [json!({"layer": raster}), json!({"layer": u64::MAX})] {
        assert!(s.execute("layer.smartObjects.convertToLayers", p).is_err());
    }
    // A warp can't be applied to the unpacked layers: an error, and the document is unchanged.
    s.execute("edit.transform.warp", json!({"layer": id, "style": "arc", "bend": 30})).unwrap();
    let steps = s.active().unwrap().history.entries().len();
    assert!(s.execute("layer.smartObjects.convertToLayers", json!({})).is_err());
    assert_eq!(s.active().unwrap().history.entries().len(), steps);
}

#[test]
fn convert_to_layers_conforms_the_contents_to_the_document_mode_and_depth() {
    let mut s = session(8);
    paint(&mut s);
    convert(&mut s);
    // The contents stay 8-bit RGB while the document becomes 16-bit grayscale.
    s.execute("image.mode.bits16", json!({})).unwrap();
    s.execute("image.mode.grayscale", json!({})).unwrap();
    convert_to_layers(&mut s);
    let fmt = s.active().unwrap().doc.pixel_format();
    assert!(matches!(&active_layer(&s).content, LayerContent::Raster(px) if px.format().mode == fmt.mode && px.format().sample == fmt.sample));
}

/// A developed photo's source taken straight from its one plain layer is exactly what flattening
/// it gives, at the document's depth; at another depth it still flattens.
#[test]
fn developed_source_shortcut_matches_the_flatten() {
    use lightcraft_raw::testgen::{DngSpec, mosaic, scene};
    let (w, h) = (40, 24);
    let mut spec = DngSpec::cfa(w, h, mosaic(&scene(w, h), w, [0, 1, 1, 2], 0, 65535));
    spec.as_shot_neutral = Some([0.6, 1.0, 0.8]);
    let bytes = spec.build();
    let settings = json!({ "light": { "exposure": 0.4 } });
    let doc = crate::develop_layer_cmds::developed_doc("a.dng", &bytes, &settings).unwrap();
    let fmt = doc.pixel_format();
    let fast = plain_layer_surface(&doc, fmt).expect("a developed photo is one plain layer");
    let slow =
        buffer_to_surface(&photocraft_compose::flatten(&doc), fmt, &crate::color_cmds::composite_profile(&doc), &crate::color_cmds::document_profile(&doc))
            .unwrap();
    assert_eq!(fast.read_region(doc.bounds()), slow.read_region(doc.bounds()));
    let other = PixelFormat { sample: photocraft_color::SampleType::U8, ..fmt };
    assert!(plain_layer_surface(&doc, other).is_none(), "another depth converts through the flatten");
    let mut faded = doc.clone();
    faded.layers[0].opacity = 0.5;
    assert!(plain_layer_surface(&faded, fmt).is_none());
}

#[test]
fn smart_resource_nested_symbols_styles_swatches_survive_cold_source_and_contents_edit() {
    use crate::vector_packages::{colours, symbols};
    use photocraft_doc::vector_design::{
        styles::{StyleLink, starter_library},
        swatches::*,
    };
    use photocraft_doc::{Color, ColorMode, Fill, Group, ShapeLayer, SymbolDef, SymbolId, SymbolInstance};
    let mut doc = Document::new("Resource closure", Size::new(80, 60), ColorMode::Rgb, photocraft_color::SampleType::U8);
    let shape = |doc: &Document, x: f64, y: f64| {
        let mut sh =
            ShapeLayer { path: photocraft_vector::shapes::rect(x, y, 12., 10.), fill: Some(Fill::Solid(Color::rgb(1., 0., 0.))), ..Default::default() };
        crate::vector_cmds::refresh_shape(doc, &mut sh);
        Layer::new("Badge", LayerContent::Shape(sh))
    };
    let mut master = shape(&doc, 2., 3.);
    let styles = starter_library();
    master.vector.graphic_style =
        Some(StyleLink { style: styles[0].id, applied: styles[0].attributes.clone(), character_runs: vec![], paragraph_runs: vec![] });
    let master_layer = master.id;
    doc.graphic_styles = styles;
    doc.symbols = vec![
        SymbolDef { id: SymbolId(7), name: "Leaf".into(), layers: vec![master] },
        SymbolDef {
            id: SymbolId(9),
            name: "Nested".into(),
            layers: vec![Layer::new(
                "Leaf instance",
                LayerContent::Symbol(SymbolInstance { symbol: SymbolId(7), transform: Affine::translate(8., 4.), cache: None }),
            )],
        },
        SymbolDef { id: SymbolId(15), name: "Unrelated".into(), layers: vec![shape(&doc, 0., 0.)] },
    ];
    let root = shape(&doc, 5., 40.);
    let root_id = root.id;
    let outside = shape(&doc, 70., 50.);
    let outside_id = outside.id;
    let instance =
        Layer::new("Nested instance", LayerContent::Symbol(SymbolInstance { symbol: SymbolId(9), transform: Affine::translate(30., 8.), cache: None }));
    let mut group = Layer::new(
        "Artwork",
        LayerContent::Group(Group { children: vec![root, instance], expanded: true, artboard: None, isolation: photocraft_doc::GroupIsolation::Auto }),
    );
    symbols::refresh_layer_symbols(&doc, &mut group).unwrap();
    let group_id = group.id;
    doc.layers = vec![group, outside];
    doc.swatches = SwatchBook {
        swatches: vec![
            Swatch {
                id: SwatchId(1),
                name: "Brand red".into(),
                paint: Fill::Solid(Color::rgb(1., 0., 0.)),
                global: true,
                spot: false,
                group: Some(2),
                stops: vec![],
            },
            Swatch { id: SwatchId(3), name: "Unused".into(), paint: Fill::Solid(Color::WHITE), global: false, spot: false, group: None, stops: vec![] },
        ],
        groups: vec![SwatchGroup { id: 2, name: "Brand".into() }, SwatchGroup { id: 4, name: "Unused group".into() }],
        links: vec![(None, root_id), (Some(SymbolId(7)), master_layer), (None, outside_id)]
            .into_iter()
            .map(|(symbol, layer)| PaintLink {
                target: PaintTarget { symbol, layer, paint: PaintSlot::Fill, stop: None },
                reference: SwatchRef { swatch: SwatchId(1), tint: 100. },
            })
            .collect(),
    };
    let before = photocraft_compose::flatten(&doc).px;
    let mut session = Session::new();
    session.add_document(doc, None);
    session.execute("layer.smartObjects.convertToSmartObject", json!({"layer":group_id.0})).unwrap();
    assert_eq!(flat(&session), before);
    let smart_layer = session.active().unwrap().active_layer.unwrap();
    let sm = active_smart(&session);
    let SmartSource::Embedded { file_name, bytes } = &sm.source else { panic!("embedded contents expected") };
    let mut contents = decode_source(file_name, bytes).unwrap();
    assert_eq!(contents.symbols.iter().map(|master| master.name.as_str()).collect::<Vec<_>>(), ["Leaf", "Nested"]);
    assert_eq!(contents.graphic_styles.len(), 1);
    assert_eq!(contents.swatches.links.len(), 2);
    assert_eq!(contents.swatches.swatches.len(), 1);
    assert_eq!(contents.swatches.groups.len(), 1);
    assert_eq!(contents.size, Size::new(80, 60), "symbol paint keeps its original canvas frame");
    photocraft_doc::vector_design::validation::document(&contents).unwrap();
    let projected = crate::vector_integration::project_document(&contents).unwrap();
    let expected = photocraft_compose::flatten(&projected).px;
    contents.name.push_str(" cold decode");
    let cold = encode_source(&contents).unwrap();
    let fresh = source_image("contents.pcraft", &cold, contents.pixel_format()).unwrap();
    let mut actual = vec![[0.; 4]; 80 * 60];
    fresh.surface.read_rgba_into(contents.bounds(), &mut actual);
    assert!(max_diff(&actual, &expected) <= 1. / 255. + 1e-6);
    // Reopen the outer native document and verify its editable nested source again.
    let saved = photocraft_format::save_to_bytes(&session.active().unwrap().doc, &Default::default()).unwrap();
    let reopened = photocraft_format::load_from_bytes(&saved).unwrap();
    assert_eq!(photocraft_compose::flatten(&reopened).px, before);
    session.execute("layer.smartObjects.editContents", json!({"layer":smart_layer.0})).unwrap();
    let mut swatch = session.active().unwrap().doc.swatches.swatches[0].clone();
    swatch.paint = Fill::Solid(Color::rgb(0., 1., 0.));
    let request = colours::Request { capture: colours::Capture::begin(&session).unwrap(), action: colours::Action::Edit { swatch } };
    session.execute("swatch.document.edit", serde_json::to_value(request).unwrap()).unwrap();
    session.execute("layer.smartObjects.saveContents", json!({})).unwrap();
    session.set_active(0);
    let after = flat(&session);
    assert_eq!(after[43 * 80 + 8], [0., 1., 0., 1.]);
    assert_eq!(after[18 * 80 + 44], [0., 1., 0., 1.]);
    assert_eq!(after[55 * 80 + 74], [1., 0., 0., 1.], "unrelated outer art is unchanged");
    assert!(session.undo());
    assert_eq!(flat(&session), before, "contents update is one undoable step");
    assert!(session.undo());
    assert!(matches!(session.active().unwrap().doc.layer(group_id).unwrap().content, LayerContent::Group(_)));
}

#[test]
fn smart_resource_canvas_gradient_pattern_and_live_geometry_reproject_identically() {
    use photocraft_doc::{Color, ColorMode, Fill, Pattern, ShapeLayer};
    let mut doc = Document::new("Canvas paint", Size::new(120, 80), ColorMode::Rgb, photocraft_color::SampleType::U8);
    let mut tile = Surface::new(PixelFormat::RGBA8);
    tile.fill_rect(Rect::from_xywh(0, 0, 2, 2), &[0., 0., 1., 1.]);
    doc.patterns.push(Pattern::new("Tile", tile, 2, 2));
    let mut gradient = Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 1., photocraft_doc::GradientStyle::Linear, false);
    if let Fill::Gradient { align, .. } = &mut gradient {
        *align = false;
    }
    for fill in [gradient, Fill::Pattern { name: "Tile".into(), id: doc.patterns[0].id.clone(), scale: 1., angle: 0., link: false, phase: (1., 1.) }] {
        let sh = ShapeLayer { path: photocraft_vector::shapes::rect(40., 20., 30., 25.), fill: Some(fill), ..Default::default() };
        let mut layer = Layer::new("Canvas-relative vector", LayerContent::Shape(sh));
        layer.vector.live_shape = Some(photocraft_doc::vector_design::drawing::DesignShape {
            rect: [40., 20., 30., 25.],
            angle: 0.,
            parameters: photocraft_doc::vector_design::drawing::ShapeParameters::Rect { radii: [0.; 4], kinds: [Default::default(); 4] },
        });
        layer.vector.clip = Some(photocraft_vector::shapes::rect(42., 22., 24., 20.));
        doc.layers = vec![layer];
        let before = photocraft_compose::flatten(&crate::vector_integration::project_document(&doc).unwrap()).px;
        let out = layer_to_smart(&doc, &doc.layers[0]).unwrap();
        let LayerContent::Smart(sm) = out.content else { panic!("smart expected") };
        let SmartSource::Embedded { file_name, bytes } = sm.source else { panic!("embedded expected") };
        let mut contents = decode_source(&file_name, &bytes).unwrap();
        assert_eq!(contents.size, doc.size);
        assert_eq!(contents.patterns, doc.patterns);
        assert_eq!(contents.layers[0].vector.live_shape, doc.layers[0].vector.live_shape);
        assert_eq!(contents.layers[0].vector.clip, doc.layers[0].vector.clip);
        let projected = crate::vector_integration::project_document(&contents).unwrap();
        assert!(max_diff(&photocraft_compose::flatten(&projected).px, &before) <= 1. / 255. + 1e-6);
        contents.name.push_str(" independent cold source");
        let bytes = encode_source(&contents).unwrap();
        let cold = source_image("cold.pcraft", &bytes, contents.pixel_format()).unwrap();
        let mut pixels = vec![[0.; 4]; 120 * 80];
        cold.surface.read_rgba_into(contents.bounds(), &mut pixels);
        assert!(max_diff(&pixels, &before) <= 1. / 255. + 1e-6);
    }
}

#[test]
fn smart_resource_raster_pattern_reference_survives_cold_crop_boundary() {
    let mut doc = Document::new("Pattern reference", Size::new(80, 60), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
    let mut pixels = Surface::new(PixelFormat::RGBA8);
    pixels.write_region(Rect::from_xywh(0, 0, 3, 1), &[1., 0., 0., 1., 0., 1., 0., 1., 0., 0., 1., 1.]);
    let pattern = photocraft_doc::Pattern::new("RGB tile", pixels, 3, 1);
    let mut layer = Layer::raster("Paint", PixelFormat::RGBA8);
    layer.surface_mut().unwrap().fill_rect(Rect::from_xywh(17, 13, 20, 18), &[0.7, 0.7, 0.7, 1.]);
    layer.effects.reference = Some((7., 11.));
    layer.effects.items.push(photocraft_doc::Effect::PatternOverlay {
        common: photocraft_doc::FxCommon::new(BlendMode::Normal, 1.),
        name: pattern.name.clone(),
        id: pattern.id.clone(),
        scale: 1.,
        angle: 0.,
        link: true,
        phase: (1., 0.),
    });
    doc.patterns.push(pattern);
    doc.layers.push(layer);
    let before = photocraft_compose::flatten(&doc).px;
    let layer = layer_to_smart(&doc, &doc.layers[0]).unwrap();
    let LayerContent::Smart(sm) = layer.content else { panic!("expected smart") };
    let SmartSource::Embedded { file_name, bytes } = sm.source else { panic!("expected embedded") };
    let mut inner = decode_source(&file_name, &bytes).unwrap();
    inner.name.push_str(" cold");
    let bytes = encode_source(&inner).unwrap();
    let image = source_image("cold.pcraft", &bytes, inner.pixel_format()).unwrap();
    let mut after = vec![[0.; 4]; 80 * 60];
    image.surface.read_rgba_into(doc.bounds(), &mut after);
    assert!(max_diff(&before, &after) <= 1. / 255. + 1e-6);
}

#[test]
fn smart_resource_nested_psd_linked_source_remains_editable() {
    use photocraft_doc::{Color, ColorMode, SampleType, SymbolDef, SymbolId, SymbolInstance};
    let inner = Document::with_background("Embedded source", Size::new(10, 8), ColorMode::Rgb, SampleType::U8, Color::rgb(0., 1., 0.));
    let png = photocraft_io::export(&inner, "png", &Default::default()).unwrap().bytes;
    let linked = photocraft_io::linked::encode_linked_file(&photocraft_io::linked::LinkedFile {
        uuid: "master-source".into(),
        file_name: "source.png".into(),
        bytes: png.clone(),
    });
    let mut doc = Document::new("Linked master", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
    doc.metadata.psd_global_blocks.push((*b"8BIM", *b"lnk2", Arc::new(linked)));
    let smart = Layer::new(
        "Linked pixels",
        LayerContent::Smart(SmartObject::new(
            SmartSource::Linked { path: "master-source".into() },
            Affine::IDENTITY,
            Some(inner.layers[0].surface().unwrap().clone()),
        )),
    );
    doc.symbols.push(SymbolDef { id: SymbolId(1), name: "Master".into(), layers: vec![smart] });
    let mut instance = Layer::new("Instance", LayerContent::Symbol(SymbolInstance { symbol: SymbolId(1), transform: Affine::translate(5., 6.), cache: None }));
    crate::vector_packages::symbols::refresh_layer_symbols(&doc, &mut instance).unwrap();
    doc.layers.push(instance);
    let converted = layer_to_smart(&doc, &doc.layers[0]).unwrap();
    let LayerContent::Smart(sm) = converted.content else { panic!("expected smart") };
    let SmartSource::Embedded { file_name, bytes } = sm.source else { panic!("expected embedded") };
    let contents = decode_source(&file_name, &bytes).unwrap();
    let LayerContent::Smart(nested) = &contents.symbols[0].layers[0].content else { panic!("expected nested smart") };
    let (name, bytes) = source_bytes(&contents.metadata, &nested.source).unwrap();
    assert_eq!(name, "source.png");
    assert_eq!(bytes.as_ref(), &png);
    assert_eq!(decode_source(&name, &bytes).unwrap().size, Size::new(10, 8));
}
