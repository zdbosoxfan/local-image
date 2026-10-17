use photocraft_engine::artboard_cmds;
use photocraft_engine::doc::{ArtboardBackground, ColorMode, Document, Layer, LayerId, SampleType, Size};
use photocraft_engine::{EngineError, Session};
use serde_json::json;

fn test_doc(w: u32, h: u32) -> Document {
    Document::new("test", Size::new(w, h), ColorMode::Rgb, SampleType::U8)
}

fn test_session(w: u32, h: u32) -> Session {
    let mut s = Session::new();
    s.add_document(test_doc(w, h), None);
    s
}

#[test]
fn new_artboard_creates_top_level_group() {
    let mut s = test_session(640, 480);
    let res = s.execute("layer.new.artboard", json!({"rect": [10, 20, 300, 150], "name": "Hero"})).unwrap();

    let id = LayerId(res["layer"].as_u64().unwrap());
    let st = s.active().unwrap();
    let doc = st.doc.as_ref();

    let boards = doc.artboards();
    assert_eq!(boards.len(), 1);
    let rect = boards[0].2.rect;
    assert_eq!(rect.x0, 10);
    assert_eq!(rect.y0, 20);
    assert_eq!(rect.width(), 300);
    assert_eq!(rect.height(), 150);

    let layer = doc.layer(id).unwrap();
    assert!(layer.is_group());
    assert!(layer.artboard().is_some());
    assert_eq!(layer.name, "Hero");
}

#[test]
fn new_artboard_defaults_to_canvas_size_at_origin() {
    let mut s = test_session(200, 100);
    s.execute("layer.new.artboard", json!({})).unwrap();

    let doc = s.active().unwrap().doc.as_ref();
    let boards = doc.artboards();
    assert_eq!(boards.len(), 1);
    let rect = boards[0].2.rect;
    assert_eq!((rect.x0, rect.y0, rect.width(), rect.height()), (0, 0, 200, 100));
}

#[test]
fn new_artboard_preset_sets_size() {
    let mut s = test_session(50, 50);
    s.execute("layer.new.artboard", json!({"preset": "A4"})).unwrap();

    let doc = s.active().unwrap().doc.as_ref();
    let boards = doc.artboards();
    assert_eq!(boards.len(), 1);
    let rect = boards[0].2.rect;
    assert_eq!(rect.width(), 595);
    assert_eq!(rect.height(), 842);
    assert_eq!(doc.size.width, 595);
    assert_eq!(doc.size.height, 842);
}

#[test]
fn new_artboard_sets_background() {
    let mut s = test_session(20, 20);
    s.execute("layer.new.artboard", json!({"background": "black"})).unwrap();

    let doc = s.active().unwrap().doc.as_ref();
    let boards = doc.artboards();
    assert_eq!(boards.len(), 1);
    let bg = &boards[0].2.background;
    assert!(matches!(bg, &ArtboardBackground::Black));
}

#[test]
fn artboard_from_layers_wraps_selected_layers() {
    let mut doc = test_doc(300, 300);
    let g1 = Layer::group("G1", vec![]);
    let g2 = Layer::group("G2", vec![]);
    let id1 = doc.insert_above(None, g1);
    let id2 = doc.insert_above(None, g2);

    let mut s = Session::new();
    s.add_document(doc, None);
    {
        let st = s.active_mut().unwrap();
        st.active_layer = Some(id1);
        st.selected_layers = vec![id1, id2];
        st.layer_anchor = Some(id1);
    }

    let res = s.execute("layer.new.artboardFromLayers", json!({"name": "Merged"})).unwrap();
    let gid = LayerId(res["layer"].as_u64().unwrap());

    let doc = s.active().unwrap().doc.as_ref();
    assert_eq!(doc.artboards().len(), 1);

    let layer = doc.layer(gid).unwrap();
    assert!(layer.is_group());
    assert!(layer.artboard().is_some());
    assert_eq!(layer.name, "Merged");

    let top_ids: Vec<LayerId> = doc.layers.iter().map(|l| l.id).collect();
    assert!(!top_ids.contains(&id1));
    assert!(!top_ids.contains(&id2));
    assert!(top_ids.contains(&gid));
}

#[test]
fn set_props_resizes_and_clears_preset() {
    let mut s = test_session(100, 100);
    let res = s.execute("layer.new.artboard", json!({"preset": "A4"})).unwrap();
    let id = LayerId(res["layer"].as_u64().unwrap());

    s.execute("layer.artboard.set", json!({"layer": id.0, "width": 200, "height": 100})).unwrap();

    let doc = s.active().unwrap().doc.as_ref();
    let boards = doc.artboards();
    assert_eq!(boards.len(), 1);
    let board = &boards[0].2;
    assert_eq!(board.rect.width(), 200);
    assert_eq!(board.rect.height(), 100);
    assert!(board.preset.is_empty());
}

#[test]
fn set_props_rejects_non_artboard_layer() {
    let mut doc = test_doc(100, 100);
    let group = Layer::group("G", vec![]);
    let plain_id = doc.insert_above(None, group);

    let mut s = Session::new();
    s.add_document(doc, None);

    // Create an artboard and make it active so the precondition passes.
    let res = s.execute("layer.new.artboard", json!({"rect": [0, 0, 50, 50]})).unwrap();
    let artboard_id = LayerId(res["layer"].as_u64().unwrap());

    // Try to set properties on a plain (non-artboard) layer.
    let err = s.execute("layer.artboard.set", json!({"layer": plain_id.0, "width": 20, "height": 20})).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));

    // Ensure the artboard still exists and is unchanged.
    let doc = s.active().unwrap().doc.as_ref();
    let boards = doc.artboards();
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].0, artboard_id);
}

#[test]
fn artboard_document_extracts_correct_size() {
    let mut s = test_session(300, 300);
    let res = s.execute("layer.new.artboard", json!({"rect": [25, 50, 200, 100], "name": "Board"})).unwrap();
    let id = LayerId(res["layer"].as_u64().unwrap());

    let st = s.active().unwrap();
    let doc = st.doc.as_ref();
    let one = artboard_cmds::artboard_document(doc, id).unwrap();

    assert_eq!(one.size.width, 200);
    assert_eq!(one.size.height, 100);
    assert_eq!(one.name, "Board");
    assert_eq!(one.layers.len(), 1);
}

#[test]
fn set_props_preset_updates_size_and_name() {
    let mut s = test_session(100, 100);
    let res = s.execute("layer.new.artboard", json!({})).unwrap();
    let id = LayerId(res["layer"].as_u64().unwrap());

    s.execute("layer.artboard.set", json!({"layer": id.0, "preset": "Letter", "name": "Paper"})).unwrap();

    let doc = s.active().unwrap().doc.as_ref();
    let boards = doc.artboards();
    assert_eq!(boards.len(), 1);
    let board = &boards[0].2;
    assert_eq!(board.rect.width(), 612);
    assert_eq!(board.rect.height(), 792);
    assert_eq!(board.preset, "Letter");

    let layer = doc.layer(id).unwrap();
    assert_eq!(layer.name, "Paper");
}
