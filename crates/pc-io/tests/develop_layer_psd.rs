//! local-image: Develop layers in PSD. The layer is written as a plain smart object whose develop
//! is the existing Camera Raw Filter (`filter.develop`) carrying our private record; saving then
//! loading gives the same `DevelopLink`, raw and non-raw sources alike.

use std::sync::Arc;

use photocraft_codecs::{ChannelLayout, EncodeOptions, Format, Image};
use photocraft_doc::{DevelopLink, Document, LayerContent, SmartFilter, SmartObject, SmartSource};
use photocraft_io::smart_map::{DEVELOP_LAYER_MARKER, filter_from_item, item_for_filter, parse_sold};
use photocraft_io::{ExportOptions, export, import};
use photocraft_raw::testgen::{DngSpec, mosaic, scene};
use serde_json::Value;

fn dng() -> Vec<u8> {
    let (w, h) = (40, 24);
    let mut spec = DngSpec::cfa(w, h, mosaic(&scene(w, h), w, [0, 1, 1, 2], 0, 65535));
    spec.as_shot_neutral = Some([0.6, 1.0, 0.8]);
    spec.build()
}

fn png() -> Vec<u8> {
    let px: Vec<u8> = (0..32 * 20).flat_map(|i| [(i % 32 * 8) as u8, (i / 32 * 12) as u8, 128]).collect();
    let img = Image::from_u8(32, 20, ChannelLayout::Rgb, px).unwrap();
    photocraft_codecs::encode(&img, Format::Png, &EncodeOptions::default()).unwrap()
}

fn link(exposure: f64, photo: Option<u64>) -> DevelopLink {
    let mut s = lightcraft_develop::DevelopSettings::default();
    s.light.exposure = exposure;
    s.light.contrast = -12.0;
    s.color.vibrance = 15.0;
    // beyond what Camera Raw understands: only our record keeps it
    s.profile.id = "lc.vivid".into();
    DevelopLink { settings: serde_json::to_value(&s).unwrap(), photo }
}

fn settings(v: &Value) -> lightcraft_develop::DevelopSettings {
    serde_json::from_value(v.clone()).unwrap()
}

/// A document whose only layer is a Develop layer of `bytes` (as `develop.openPhoto` builds one).
fn develop_doc(file: &str, bytes: &[u8], link: DevelopLink) -> Document {
    let mut doc = photocraft_io::raw::develop_with_settings(file, bytes, &link.settings).unwrap().document;
    let cache = doc.layers[0].surface().cloned();
    let mut sm = SmartObject::new(SmartSource::Embedded { file_name: file.into(), bytes: Arc::new(bytes.to_vec()) }, photocraft_geom::Affine::IDENTITY, cache);
    sm.develop = Some(link);
    doc.layers[0].name = "Develop".into();
    doc.layers[0].content = LayerContent::Smart(sm);
    doc
}

fn save_load(doc: &Document) -> Document {
    let e = export(doc, "psd", &ExportOptions::default()).unwrap();
    import("t.psd", &e.bytes).unwrap().document
}

fn smart(doc: &Document) -> &SmartObject {
    match &doc.layers[0].content {
        LayerContent::Smart(sm) => sm,
        c => panic!("the layer is {}", c.kind_name()),
    }
}

fn smart_mut(doc: &mut Document) -> &mut SmartObject {
    match &mut doc.layers[0].content {
        LayerContent::Smart(sm) => sm,
        _ => panic!("not a smart object"),
    }
}

#[test]
fn develop_layers_round_trip_through_psd() {
    for (file, bytes) in [("shot.dng", dng()), ("photo.png", png())] {
        let l = link(0.5, Some(7));
        let back = save_load(&develop_doc(file, &bytes, l.clone()));
        let sm = smart(&back);
        assert_eq!(sm.develop, Some(l.clone()), "{file}: the same link");
        assert!(sm.smart_filters.is_empty(), "{file}: the develop filter becomes the link again");
        // the source is embedded as for any smart object
        let SmartSource::Linked { path } = &sm.source else { panic!("PSD import keeps sources as uuids") };
        assert_eq!(photocraft_io::linked::find_linked_file(&back.metadata, path).map(|f| f.bytes), Some(bytes.clone()), "{file}");
        // Photoshop sees a Camera Raw Filter with the settings it understands
        let stack = parse_sold(sm.psd_raw.as_deref().unwrap()).unwrap().stack.unwrap();
        assert_eq!(stack.filters.len(), 1);
        assert_eq!(stack.filters[0].command, photocraft_io::develop_filter::COMMAND);
        let item = item_for_filter(&stack.filters[0]).unwrap();
        assert!(item.get("Fltr").is_some(), "{file}: Photoshop's Camera Raw descriptor");
        // saving again keeps it; a changed develop is written
        let mut again = save_load(&back);
        assert_eq!(smart(&again).develop, Some(l));
        let newer = link(-1.25, Some(7));
        smart_mut(&mut again).develop = Some(newer.clone());
        assert_eq!(smart(&save_load(&again)).develop, Some(newer), "{file}");
    }
}

#[test]
fn the_develop_stays_under_the_layers_own_filters() {
    let l = link(0.5, Some(9));
    let mut doc = develop_doc("photo.png", &png(), l.clone());
    let blur = SmartFilter {
        command: "filter.blur.gaussianBlur".into(),
        params: serde_json::json!({"radius": 2.0}),
        blend: photocraft_color::BlendMode::Normal,
        opacity: 1.0,
        visible: true,
    };
    smart_mut(&mut doc).smart_filters.push(blur);
    let back = save_load(&doc);
    let sm = smart(&back);
    let stack = parse_sold(sm.psd_raw.as_deref().unwrap()).unwrap().stack.unwrap();
    assert_eq!(stack.filters.iter().map(|f| f.command.as_str()).collect::<Vec<_>>(), [photocraft_io::develop_filter::COMMAND, "filter.blur.gaussianBlur"]);
    assert_eq!(sm.develop, Some(l));
    assert_eq!(sm.smart_filters.len(), 1);
    assert_eq!(sm.smart_filters[0].command, "filter.blur.gaussianBlur");
}

#[test]
fn photoshops_edits_to_the_filter_apply_over_ours() {
    let back = save_load(&develop_doc("photo.png", &png(), link(0.5, Some(9))));
    let stack = parse_sold(smart(&back).psd_raw.as_deref().unwrap()).unwrap().stack.unwrap();
    // Photoshop changes the filter's exposure
    let mut item = item_for_filter(&stack.filters[0]).unwrap();
    let fltr = item.items.iter_mut().find(|(k, _)| k.is("Fltr")).unwrap();
    if let photocraft_psd::descriptor::Value::Descriptor(d) = &mut fltr.1 {
        for (k, v) in &mut d.items {
            if k.is("Ex12") {
                *v = photocraft_psd::descriptor::Value::Double(-0.5);
            }
        }
    }
    let f = filter_from_item(&item);
    let s = settings(&f.params[DEVELOP_LAYER_MARKER]["settings"]);
    assert_eq!(s.light.exposure, -0.5, "Photoshop's value");
    assert_eq!(s.profile.id, "lc.vivid", "ours where Photoshop has no setting");
    assert_eq!(f.params[DEVELOP_LAYER_MARKER]["photo"], 9);
}
