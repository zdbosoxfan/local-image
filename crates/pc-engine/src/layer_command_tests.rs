use crate::Session;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 20, "height": 20})).unwrap();
    s
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
