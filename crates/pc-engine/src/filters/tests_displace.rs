use super::*;
use photocraft_doc::{Document, LayerId, SmartObject};
use photocraft_geom::Rect;

const DISPLACE: &str = "filter.distort.displace";

fn session(depth: u64) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width":32,"height":24,"depth":depth})).unwrap();
    s.execute("layer.new.layer", json!({})).unwrap();
    s.edit("pattern", |doc, active| {
        let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
        for y in 0..24 {
            for x in 0..32 {
                surf.write_pixel(x, y, &[(x % 7) as f32 / 6.0, (y % 5) as f32 / 4.0, 0.0, 1.0]);
            }
        }
        Ok(())
    })
    .unwrap();
    s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
    s
}

fn smart(doc: &Document) -> (LayerId, &SmartObject) {
    doc.walk()
        .into_iter()
        .find_map(|(_, _, l)| match &l.content {
            LayerContent::Smart(sm) => Some((l.id, sm)),
            _ => None,
        })
        .unwrap()
}

fn pixels(doc: &Document) -> Vec<f32> {
    smart(doc).1.cache.as_ref().unwrap().read_region(doc.bounds())
}

fn embedded() -> Value {
    let map = algo::Image { rect: Rect::new(0, 0, 2, 2), ch: 1, data: vec![1.0; 4] };
    photocraft_io::smart_map::displace_embed_map(&json!({"horizontal":10,"vertical":0,"fit":"tile","undefinedAreas":"wrap"}), &map).unwrap()
}

#[test]
fn displace_on_a_smart_object_embeds_its_map() {
    let mut s = session(8);
    let before = pixels(&s.active().unwrap().doc);
    s.execute("file.new", json!({"width":2,"height":2,"background":"white"})).unwrap();
    s.set_active(0);
    let history = s.active().unwrap().history.past_len();
    s.execute(DISPLACE, json!({"mapDocument":1,"horizontal":10,"vertical":0,"undefinedAreas":"wrap"})).unwrap();
    let doc = s.active().unwrap().doc.clone();
    let after = pixels(&doc);
    assert_ne!(before, after);
    assert_eq!(s.active().unwrap().history.past_len(), history + 1);
    let params = &smart(&doc).1.smart_filters[0].params;
    assert!(photocraft_io::smart_map::displace_embedded_map(params).is_some());
    assert!(params.get("mapDocument").is_none());
    s.undo();
    assert_eq!(pixels(&s.active().unwrap().doc), before);
    s.redo();
    assert_eq!(pixels(&s.active().unwrap().doc), after);
    s.execute("file.new", json!({"width":2,"height":2,"background":"black"})).unwrap();
    s.set_active(0);
    let mut replacement = smart(&s.active().unwrap().doc).1.smart_filters[0].params.clone();
    replacement["mapDocument"] = json!(2);
    s.execute("layer.smartFilter.setParams", json!({"params":replacement})).unwrap();
    assert_ne!(pixels(&s.active().unwrap().doc), after);
    s.execute("layer.smartFilter.setParams", json!({"params":{"mapDocument":1}})).unwrap();
    assert_eq!(pixels(&s.active().unwrap().doc), after);
    s.execute("file.close", json!({"document":2,"discard":true})).unwrap();
    s.set_active(1);
    s.execute("file.close", json!({"discard":true})).unwrap();
    s.set_active(0);
    for (visible, expected) in [(false, &before), (true, &after)] {
        s.execute("layer.smartFilter.setVisible", json!({"visible":visible})).unwrap();
        assert_eq!(&pixels(&s.active().unwrap().doc), expected);
    }
}

#[test]
fn displace_psd_filter_renders_with_its_embedded_map() {
    for depth in [8, 16, 32] {
        for (fit, edges) in [("stretch", "repeat"), ("tile", "wrap")] {
            let mut s = session(depth);
            let before = pixels(&s.active().unwrap().doc);
            let mut params = embedded();
            params["fit"] = json!(fit);
            params["undefinedAreas"] = json!(edges);
            s.execute(DISPLACE, params).unwrap();
            let after = pixels(&s.active().unwrap().doc);
            assert_ne!(after, before);
            let bytes = photocraft_io::export(&s.active().unwrap().doc, "psd", &Default::default()).unwrap().bytes;
            let imported = photocraft_io::import("map.psd", &bytes).unwrap().document;
            assert_eq!(smart(&imported).1.smart_filters[0].command, DISPLACE);
            let mut reopened = Session::new();
            reopened.add_document(imported, None);
            let mut edit = smart(&reopened.active().unwrap().doc).1.smart_filters[0].params.clone();
            edit["mapPath"] = json!("");
            edit["mapDocument"] = json!(-1);
            edit["mapLayer"] = Value::Null;
            reopened.execute("layer.smartFilter.setParams", json!({"layer":smart(&reopened.active().unwrap().doc).0.0,"params":edit})).unwrap();
            let doc = &reopened.active().unwrap().doc;
            let actual = pixels(doc);
            // PSD's source cache may quantize by one 8-bit code value.
            assert!(after.iter().zip(&actual).all(|(a, b)| (a - b).abs() <= 2.0 / 255.0), "depth {depth}");
            let exported = photocraft_io::export(doc, "psd", &Default::default()).unwrap();
            let back = photocraft_io::import("again.psd", &exported.bytes).unwrap().document;
            assert_eq!(smart(&back).1.smart_filters[0].command, DISPLACE);
            let native = photocraft_format::save_to_bytes(doc, &Default::default()).unwrap();
            let mut native = photocraft_format::load_from_bytes(&native).unwrap();
            let id = smart(&native).0;
            crate::smart_cmds::refresh(&mut native, id).unwrap();
            assert_eq!(pixels(&native), actual);
            let id = smart(&reopened.active().unwrap().doc).0.0;
            reopened.execute("layer.smartFilter.setParams", json!({"layer":id,"params":{"horizontal":-10}})).unwrap();
            assert_ne!(pixels(&reopened.active().unwrap().doc), actual);
            let out = photocraft_io::export(&reopened.active().unwrap().doc, "psd", &Default::default()).unwrap();
            let back = photocraft_io::import("scaled.psd", &out.bytes).unwrap().document;
            let filter = &smart(&back).1.smart_filters[0];
            assert_eq!(filter.command, DISPLACE);
            assert_eq!(filter.params["horizontal"].as_f64(), Some(-10.0));
            assert!(photocraft_io::smart_map::displace_embedded_map(&filter.params).is_some());
        }
    }
}

#[test]
fn invalid_and_oversized_map_requests_leave_document_and_history_unchanged() {
    let mut s = session(8);
    let huge = Document::new("large map", photocraft_geom::Size::new((1 << 24) + 1, 1), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
    s.add_document(huge, None);
    s.set_active(0);
    for params in [json!({"mapDocument":1}), json!({"__displacePsd":"not hex"}), json!({"mapDocument":-1,"mapPath":""})] {
        let before = s.active().unwrap().doc.clone();
        let history = s.active().unwrap().history.past_len();
        let journal = s.journal.clone();
        assert!(s.execute(DISPLACE, params).is_err());
        assert!(std::sync::Arc::ptr_eq(&before, &s.active().unwrap().doc));
        assert_eq!(s.active().unwrap().history.past_len(), history);
        assert_eq!(s.journal, journal);
    }
}
