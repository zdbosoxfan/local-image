use crate::Session;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 20, "height": 20})).unwrap();
    s
}

#[test]
fn locked_text_rejects_edits_without_history() {
    let mut s = session();
    s.execute("type.create", json!({"x": 5, "y": 5, "text": "a", "size": 12})).unwrap();
    s.execute("layer.setProps", json!({"locked": true})).unwrap();
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    assert!(s.execute("type.edit", json!({"text": "b"})).unwrap_err().to_string().contains("locked"));
    assert_eq!(s.active().unwrap().history.past_len(), history);
    assert!(std::sync::Arc::ptr_eq(&before, &s.active().unwrap().doc));
    s.execute("layer.setProps", json!({"locked": false})).unwrap();
    s.execute("type.edit", json!({"text": "b"})).unwrap();
    let st = s.active().unwrap();
    let photocraft_doc::LayerContent::Text(t) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("expected text") };
    assert_eq!(t.text, "b");
}

#[test]
fn all_lock_blocks_mask_painting_but_pixel_lock_permits_it() {
    let mut s = session();
    s.execute("layer.new.layer", json!({})).unwrap();
    let id = s.active().unwrap().active_layer.unwrap();
    s.edit("Mask", |doc, _| {
        doc.layer_mut(id).unwrap().mask = Some(photocraft_doc::LayerMask::reveal_all());
        Ok(())
    })
    .unwrap();
    let stroke = json!({"points": [[5, 5]], "size": 4, "hardness": 1.0, "color": "#000000", "target": "mask"});
    s.execute("layer.setProps", json!({"locks": {"all": true}})).unwrap();
    let history = s.active().unwrap().history.past_len();
    assert!(s.execute("paint.stroke", stroke.clone()).unwrap_err().to_string().contains("locked"));
    assert_eq!(s.active().unwrap().history.past_len(), history);
    let mask_value = |s: &Session| s.active().unwrap().doc.layer(id).unwrap().mask.as_ref().unwrap().surface.pixel(5, 5)[0];
    assert!(mask_value(&s) > 0.99);
    s.execute("layer.setProps", json!({"locks": {"all": false, "pixels": true}})).unwrap();
    s.execute("paint.stroke", stroke).unwrap();
    assert!(mask_value(&s) < 0.01);
    s.execute("edit.undo", json!({})).unwrap();
    assert!(mask_value(&s) > 0.99);
}

#[test]
fn quick_mask_invert_includes_unstored_black() {
    for depth in [8, 16, 32] {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 20, "height": 20, "depth": depth})).unwrap();
        s.execute("select.rect", json!({"x": 5, "y": 5, "width": 10, "height": 10})).unwrap();
        s.execute("select.editInQuickMaskMode", json!({"on": true})).unwrap();
        s.execute("image.adjustments.invert", json!({"target": "quickMask"})).unwrap();
        let st = s.active().unwrap();
        let mask = &st.doc.quick_mask.as_ref().unwrap().surface;
        assert!(mask.sample_channel(8, 8, 0) < 0.01);
        assert!(mask.sample_channel(1, 1, 0) > 0.99);
        s.execute("select.editInQuickMaskMode", json!({"on": false})).unwrap();
        let selection = s.active().unwrap().doc.selection.as_ref().unwrap();
        assert!(selection.sample_channel(8, 8, 0) < 0.01);
        assert!(selection.sample_channel(1, 1, 0) > 0.99);
        s.execute("select.deselect", json!({})).unwrap();
        s.execute("channel.new", json!({"fill": "black"})).unwrap();
        s.execute("image.adjustments.invert", json!({"target": {"channel": 0}})).unwrap();
        assert!(s.active().unwrap().doc.channels[0].surface.sample_channel(1, 1, 0) > 0.99);
        s.undo();
        assert!(s.active().unwrap().doc.channels[0].surface.sample_channel(1, 1, 0) < 0.01);
    }
}

#[test]
fn copied_styles_keep_blend_if_channels_and_group_blending() {
    let mut s = session();
    let a = s.execute("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
    let b = s.execute("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
    s.execute(
        "layer.layerStyle.blendingOptions",
        json!({"layer": a, "blendIf": [{"channel": "gray", "underlying": [0, 128]}, {"channel": "red", "thisLayer": [10, 200]}]}),
    )
    .unwrap();
    s.execute("layer.setProps", json!({"layer": a, "channels": [true, false, true]})).unwrap();
    s.execute("layer.layerStyle.copyLayerStyle", json!({"layer": a})).unwrap();
    s.execute("layer.layerStyle.pasteLayerStyle", json!({"layer": b})).unwrap();
    let doc = &s.active().unwrap().doc;
    let source = doc.layer(photocraft_doc::LayerId(a)).unwrap().clone();
    let pasted = doc.layer(photocraft_doc::LayerId(b)).unwrap();
    assert_eq!(pasted.blend_if, source.blend_if);
    assert_eq!(pasted.excluded_channels, source.excluded_channels);
    s.execute("file.new", json!({"width": 20, "height": 20, "mode": "cmyk"})).unwrap();
    s.execute("layer.layerStyle.pasteLayerStyle", json!({})).unwrap();
    let st = s.active().unwrap();
    let pasted = st.doc.layer(st.active_layer.unwrap()).unwrap();
    assert_eq!(pasted.blend_if.get(0), source.blend_if.get(0));
    assert_eq!(pasted.blend_if.ranges.len(), 1);
    assert_eq!(pasted.excluded_channels, 0);
    let pixel = s.execute("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.new.group", json!({})).unwrap();
    s.execute("layer.layerStyle.copyLayerStyle", json!({})).unwrap();
    s.execute("layer.layerStyle.pasteLayerStyle", json!({"layer": pixel})).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(photocraft_doc::LayerId(pixel)).unwrap().blend, photocraft_color::BlendMode::Normal);
}

#[test]
fn pixel_lock_clears_and_holds_off_transparency_lock() {
    let mut s = session();
    s.execute("layer.new.layer", json!({})).unwrap();
    let locks = |s: &Session| {
        let st = s.active().unwrap();
        st.doc.layer(st.active_layer.unwrap()).unwrap().locks
    };
    for command in ["layer.setProps", "layer.lockLayers"] {
        for params in [json!({"pixels": false}), json!({"transparency": true}), json!({"pixels": true}), json!({"transparency": true})] {
            let p = if command == "layer.setProps" { json!({"locks": params}) } else { params };
            s.execute(command, p).unwrap();
        }
        assert!(locks(&s).pixels && !locks(&s).transparency);
    }
}

#[test]
fn background_becomes_layer_zero_when_blending_changes() {
    for params in [json!({"opacity": 0.5}), json!({"fill": 0.5}), json!({"blend": "Multiply"})] {
        let mut s = session();
        s.execute("layer.setProps", params).unwrap();
        let st = s.active().unwrap();
        let l = st.doc.layer(st.active_layer.unwrap()).unwrap();
        assert_eq!(l.name, "Layer 0");
        assert!(!l.locks.position && !l.locks.transparency);
    }
}

#[test]
fn object_selection_explains_empty_active_layers() {
    let mut s = session();
    s.execute("layer.new.layer", json!({})).unwrap();
    for params in [json!({"rect": [0, 0, 20, 20]}), json!({"point": [5, 5]})] {
        let error = s.execute("select.object", params).unwrap_err().to_string();
        assert!(error.contains("active layer is empty") && error.contains("sampleAllLayers"), "{error}");
        assert!(s.active().unwrap().doc.selection.is_none());
    }
}

#[test]
fn refining_a_mask_preserves_metadata_and_explicit_target() {
    let mut s = session();
    let id = s.active().unwrap().active_layer.unwrap();
    s.edit("Mask", |doc, _| {
        let mut mask = photocraft_doc::LayerMask::hide_all();
        mask.surface.fill_rect(photocraft_geom::Rect::new(0, 0, 10, 20), &[1.0]);
        mask.density = 0.75;
        mask.feather = 3.5;
        mask.linked = false;
        mask.enabled = false;
        doc.layer_mut(id).unwrap().mask = Some(mask);
        Ok(())
    })
    .unwrap();
    s.execute("select.rect", json!({"x": 10, "y": 0, "width": 10, "height": 20})).unwrap();
    s.execute("select.refineEdge", json!({"target": "mask", "output": "layerMask", "radius": 0})).unwrap();
    let l = s.active().unwrap().doc.layer(id).unwrap();
    let m = l.mask.as_ref().unwrap();
    assert_eq!((m.density, m.feather, m.linked, m.enabled), (0.75, 3.5, false, false));
    assert!(m.surface.sample_channel(2, 10, 0) > 0.9);
    assert!(m.surface.sample_channel(18, 10, 0) < 0.1);
    assert_eq!(l.name, "Layer 0");
}

#[test]
fn fill_keys_recolor_editable_text_and_respect_locks() {
    let mut s = session();
    s.tools.foreground = [1.0, 0.0, 0.0, 1.0];
    s.tools.background = [0.0, 0.0, 1.0, 1.0];
    s.execute("type.create", json!({"x": 1, "y": 5, "text": "Hello", "size": 4, "color": "#000000"})).unwrap();
    let colors = |s: &Session| {
        let st = s.active().unwrap();
        let photocraft_doc::LayerContent::Text(t) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("expected editable text") };
        t.char_runs().iter().map(|r| r.style.color.to_rgb()).collect::<Vec<_>>()
    };
    s.execute("edit.fillForeground", json!({})).unwrap();
    assert_eq!(colors(&s), [[1.0, 0.0, 0.0]]);
    s.execute("edit.fillBackgroundPreserve", json!({})).unwrap();
    assert_eq!(colors(&s), [[0.0, 0.0, 1.0]]);
    s.undo();
    assert_eq!(colors(&s), [[1.0, 0.0, 0.0]]);
    s.execute("layer.lockLayers", json!({"pixels": true})).unwrap();
    assert!(s.execute("edit.fillBackground", json!({})).unwrap_err().to_string().contains("locked"));
    assert_eq!(colors(&s), [[1.0, 0.0, 0.0]]);
}

#[test]
fn multiple_layers_align_to_active_selection() {
    let mut s = session();
    let mut ids = Vec::new();
    for x in [0, 8] {
        let id = s.execute("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
        s.execute("select.rect", json!({"x": x, "y": 0, "width": 4, "height": 4})).unwrap();
        s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
        ids.push(id);
    }
    s.execute("select.rect", json!({"x": 15, "y": 0, "width": 5, "height": 10})).unwrap();
    s.execute("layer.select", json!({"layer": ids[0]})).unwrap();
    s.execute("layer.select", json!({"layer": ids[1], "mode": "add"})).unwrap();
    s.execute("layer.align.leftEdges", json!({})).unwrap();
    for id in ids {
        assert_eq!(s.active().unwrap().doc.layer(photocraft_doc::LayerId(id)).unwrap().surface().unwrap().content_bounds().x0, 15);
    }
}

#[test]
fn curves_on_masks_use_gray_levels_in_lab_and_cmyk_documents() {
    for mode in ["lab", "cmyk"] {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 20, "height": 20, "mode": mode})).unwrap();
        s.execute("layer.layerMask.revealAll", json!({})).unwrap();
        s.edit("Mask", |doc, active| {
            let bounds = doc.bounds();
            doc.layer_mut(active.unwrap()).unwrap().mask.as_mut().unwrap().surface.fill_rect(bounds, &[0.25]);
            Ok(())
        })
        .unwrap();
        s.execute("image.adjustments.curves", json!({"target": "mask", "points": [[0, 255], [255, 0]]})).unwrap();
        let st = s.active().unwrap();
        let mask = &st.doc.layer(st.active_layer.unwrap()).unwrap().mask.as_ref().unwrap().surface;
        assert!((mask.sample_channel(5, 5, 0) - 0.75).abs() < 0.02);
    }
}

#[test]
fn opacity_and_fill_reject_out_of_range_values_without_mutation() {
    let mut s = session();
    s.execute("layer.new.layer", json!({})).unwrap();
    s.execute("layer.setProps", json!({"opacity": 0.55, "fill": 0.4})).unwrap();
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    for key in ["opacity", "fill"] {
        for value in [json!(55), json!(-0.1), json!("55"), json!(null)] {
            let error = s.execute("layer.setProps", json!({key: value})).unwrap_err().to_string();
            assert!(error.contains("0..1"), "{error}");
            assert!(std::sync::Arc::ptr_eq(&before, &s.active().unwrap().doc));
            assert_eq!(s.active().unwrap().history.past_len(), history);
        }
    }
}

#[test]
fn fibers_fills_an_empty_layer_within_the_selection() {
    let mut s = session();
    s.execute("layer.new.layer", json!({})).unwrap();
    s.execute("select.rect", json!({"x": 2, "y": 3, "width": 12, "height": 10})).unwrap();
    s.execute("filter.render.fibers", json!({"seed": 1})).unwrap();
    let st = s.active().unwrap();
    let surf = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap();
    assert_eq!(surf.content_bounds(), photocraft_geom::Rect::new(2, 3, 14, 13));
    assert!(surf.rgba(5, 5)[3] > 0.99);
    assert_eq!(surf.rgba(0, 0)[3], 0.0);
}

#[test]
fn default_grayscale_conversion_matches_dot_gain_twenty_percent() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 4, "height": 1, "background": "white"})).unwrap();
    s.edit("Paint", |doc, _| {
        let surf = doc.layers[0].surface_mut().unwrap();
        for (x, c) in [[64.0, 64.0, 64.0], [128.0, 128.0, 128.0], [192.0, 192.0, 192.0], [255.0, 0.0, 0.0]].into_iter().enumerate() {
            surf.fill_rect(photocraft_geom::Rect::new(x as i32, 0, x as i32 + 1, 1), &[c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, 1.0]);
        }
        Ok(())
    })
    .unwrap();
    s.execute("image.mode.grayscale", json!({})).unwrap();
    let surf = s.active().unwrap().doc.layers[0].surface().unwrap();
    for (x, want) in [46.0, 105.0, 177.0, 107.0].into_iter().enumerate() {
        let got = surf.pixel(x as i32, 0)[0] * 255.0;
        assert!((got - want).abs() <= 1.5, "{x}: {got} vs {want}");
    }
}
