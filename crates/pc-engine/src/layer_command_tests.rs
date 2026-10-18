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
