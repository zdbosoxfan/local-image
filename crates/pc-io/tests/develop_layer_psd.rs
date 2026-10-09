//! local-image: Develop layers in PSD (`photocraft_io::develop_layer_map`): our record round-trips,
//! a raw source carries Camera Raw (`crs:`) settings, other sources a stand-in Camera Raw Filter,
//! and a PSD with only the `crs:` settings opens as a Develop layer.

use std::sync::Arc;

use photocraft_codecs::{ChannelLayout, EncodeOptions, Format, Image};
use photocraft_doc::{DevelopLink, Document, LayerContent, SmartObject, SmartSource};
use photocraft_io::develop_layer_map::{MARKER, settings_from_open};
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
    s.mixer.blue.sat = -20.0;
    // beyond what Camera Raw understands: only our record keeps it
    s.profile.id = "lc.vivid".into();
    DevelopLink { settings: serde_json::to_value(&s).unwrap(), photo }
}

fn settings(v: &Value) -> lightcraft_develop::DevelopSettings {
    serde_json::from_value(v.clone()).unwrap()
}

/// A Develop layer of `bytes` (as `develop.openPhoto` builds one), named `name`.
fn develop_layer(doc: &mut Document, name: &str, file: &str, bytes: &[u8], link: DevelopLink) {
    let r = photocraft_io::raw::develop_with_settings(file, bytes, &link.settings).unwrap();
    let cache = r.document.layers[0].surface().cloned();
    let mut sm = SmartObject::new(SmartSource::Embedded { file_name: file.into(), bytes: Arc::new(bytes.to_vec()) }, photocraft_geom::Affine::IDENTITY, cache);
    sm.develop = Some(link);
    let mut l = r.document.layers[0].clone();
    l.id = photocraft_doc::LayerId(1000 + doc.layers.len() as u64);
    l.name = name.into();
    l.content = LayerContent::Smart(sm);
    doc.layers.push(l);
}

fn develop_doc(file: &str, bytes: &[u8], link: DevelopLink) -> Document {
    let mut doc = photocraft_io::raw::develop_with_settings(file, bytes, &link.settings).unwrap().document;
    doc.layers.clear();
    develop_layer(&mut doc, "Develop", file, bytes, link);
    doc
}

fn save_load(doc: &Document) -> Document {
    let e = export(doc, "psd", &ExportOptions::default()).unwrap();
    import("t.psd", &e.bytes).unwrap().document
}

fn smart<'a>(doc: &'a Document, name: &str) -> &'a SmartObject {
    let l = doc.walk().into_iter().map(|(_, _, l)| l).find(|l| l.name == name).unwrap();
    match &l.content {
        LayerContent::Smart(sm) => sm,
        c => panic!("{name} is {}", c.kind_name()),
    }
}

fn smart_mut<'a>(doc: &'a mut Document, name: &str) -> &'a mut SmartObject {
    let id = doc.walk().into_iter().map(|(_, _, l)| l).find(|l| l.name == name).unwrap().id;
    match &mut doc.layer_mut(id).unwrap().content {
        LayerContent::Smart(sm) => sm,
        _ => panic!("not a smart object"),
    }
}

fn source_uuid(sm: &SmartObject) -> String {
    match &sm.source {
        SmartSource::Linked { path } => path.clone(),
        SmartSource::Embedded { .. } => panic!("PSD import keeps sources as uuids"),
    }
}

#[test]
fn raw_develop_layers_round_trip_with_camera_raw_settings() {
    let raw = dng();
    let l = link(0.5, Some(7));
    let doc = develop_doc("shot.dng", &raw, l.clone());
    let back = save_load(&doc);
    let sm = smart(&back, "Develop");
    assert_eq!(sm.develop, Some(l.clone()), "our record wins, nothing lost");
    assert!(sm.smart_filters.is_empty(), "a raw source needs no stand-in filter");
    // the source is embedded, with the Camera Raw settings for Photoshop beside it
    let uuid = source_uuid(sm);
    assert_eq!(photocraft_io::linked::find_linked_file(&back.metadata, &uuid).map(|f| f.bytes), Some(raw.clone()));
    let open = photocraft_io::linked::find_open_descriptor(&back.metadata, &uuid).expect("Camera Raw settings");
    let cr = settings(&settings_from_open(&open, true).unwrap());
    assert_eq!((cr.light.exposure, cr.light.contrast, cr.color.vibrance, cr.mixer.blue.sat), (0.5, -12.0, 15.0, -20.0));
    assert_eq!(cr.profile.id, lightcraft_develop::DevelopSettings::default().profile.id, "the crs: subset has no profile");
    // saving again keeps it (a fixed point)
    let again = save_load(&back);
    assert_eq!(smart(&again, "Develop").develop, Some(l));
    assert_eq!(source_uuid(smart(&again, "Develop")), uuid);
}

#[test]
fn a_redeveloped_layer_writes_its_new_settings() {
    let raw = dng();
    let mut back = save_load(&develop_doc("shot.dng", &raw, link(0.5, Some(7))));
    let old_uuid = source_uuid(smart(&back, "Develop"));
    let newer = link(-1.25, Some(7));
    smart_mut(&mut back, "Develop").develop = Some(newer.clone());
    let again = save_load(&back);
    let sm = smart(&again, "Develop");
    assert_eq!(sm.develop, Some(newer));
    let uuid = source_uuid(sm);
    assert_ne!(uuid, old_uuid, "Camera Raw keeps settings per embedded file");
    let open = photocraft_io::linked::find_open_descriptor(&again.metadata, &uuid).unwrap();
    assert_eq!(settings(&settings_from_open(&open, true).unwrap()).light.exposure, -1.25);
    // the old version's file is gone
    assert!(photocraft_io::linked::find_linked_file(&again.metadata, &old_uuid).is_none());
}

#[test]
fn two_settings_on_one_raw_get_two_files() {
    let raw = dng();
    let mut doc = develop_doc("shot.dng", &raw, link(0.5, Some(7)));
    develop_layer(&mut doc, "Copy", "shot.dng", &raw, link(1.0, Some(7)));
    let back = save_load(&doc);
    let (a, b) = (smart(&back, "Develop"), smart(&back, "Copy"));
    assert_eq!(settings(&a.develop.as_ref().unwrap().settings).light.exposure, 0.5);
    assert_eq!(settings(&b.develop.as_ref().unwrap().settings).light.exposure, 1.0);
    assert_ne!(source_uuid(a), source_uuid(b));
}

#[test]
fn a_psd_with_only_camera_raw_settings_opens_as_a_develop_layer() {
    let raw = dng();
    let mut back = save_load(&develop_doc("shot.dng", &raw, link(0.75, Some(7))));
    // as another application would leave it: no record of ours, only the Camera Raw settings
    let sm = smart_mut(&mut back, "Develop");
    let data = sm.psd_raw.as_ref().unwrap().to_vec();
    sm.psd_raw = Some(Arc::new(photocraft_io::develop_layer_map::sold_with_record(data, None)));
    sm.develop = None;
    let again = save_load(&back);
    let link = smart(&again, "Develop").develop.clone().expect("a Develop layer");
    assert_eq!(link.photo, None, "no Library photo to follow");
    let s = settings(&link.settings);
    assert_eq!((s.light.exposure, s.light.contrast, s.color.vibrance, s.mixer.blue.sat), (0.75, -12.0, 15.0, -20.0));
}

#[test]
fn other_sources_carry_a_stand_in_camera_raw_filter() {
    let bytes = png();
    let l = link(0.5, Some(9));
    let mut doc = develop_doc("photo.png", &bytes, l.clone());
    let blur = photocraft_doc::SmartFilter {
        command: "filter.blur.gaussianBlur".into(),
        params: serde_json::json!({"radius": 2.0}),
        blend: photocraft_color::BlendMode::Normal,
        opacity: 1.0,
        visible: true,
    };
    smart_mut(&mut doc, "Develop").smart_filters.push(blur);
    let back = save_load(&doc);
    let sm = smart(&back, "Develop");
    // Photoshop sees a Camera Raw Filter under the layer's own filters
    let placed = photocraft_io::smart_map::parse_sold(sm.psd_raw.as_deref().unwrap()).unwrap();
    let stack = placed.stack.unwrap();
    assert_eq!(stack.filters.len(), 2);
    assert_eq!(stack.filters[0].command, photocraft_io::develop_filter::COMMAND);
    assert!(stack.filters[0].params.get(MARKER).is_some());
    assert!((stack.filters[0].params["light"]["exposure"].as_f64().unwrap() - 0.5).abs() < 1e-6);
    // we read it back as the Develop layer, its own filters intact
    assert_eq!(sm.develop, Some(l.clone()));
    assert_eq!(sm.smart_filters.len(), 1);
    assert_eq!(sm.smart_filters[0].command, "filter.blur.gaussianBlur");
    assert!(photocraft_io::linked::find_open_descriptor(&back.metadata, &source_uuid(sm)).is_none());
    // and again
    assert_eq!(smart(&save_load(&back), "Develop").develop, Some(l));
}

#[test]
fn a_stand_in_edited_in_photoshop_applies_over_ours() {
    let bytes = png();
    let mut doc = develop_doc("photo.png", &bytes, link(0.5, Some(9)));
    smart_mut(&mut doc, "Develop").smart_filters.clear();
    let back = save_load(&doc);
    // Photoshop changes the filter's exposure: rewrite the stand-in as it would read then
    let sm = smart(&back, "Develop");
    let mut placed = photocraft_io::smart_map::parse_sold(sm.psd_raw.as_deref().unwrap()).unwrap();
    let mut stack = placed.stack.take().unwrap();
    let mut item = photocraft_io::smart_map::item_for_filter(&stack.filters[0]).unwrap();
    let fltr = item.items.iter_mut().find(|(k, _)| k.is("Fltr")).unwrap();
    if let photocraft_psd::descriptor::Value::Descriptor(d) = &mut fltr.1 {
        for (k, v) in &mut d.items {
            if k.is("Ex12") {
                *v = photocraft_psd::descriptor::Value::Double(-0.5);
            }
        }
    }
    stack.filters[0] = photocraft_io::smart_map::filter_from_item(&item);
    let s = settings(&stack.filters[0].params[MARKER]["settings"]);
    assert_eq!(s.light.exposure, -0.5, "Photoshop's value");
    assert_eq!(s.profile.id, "lc.vivid", "ours where Photoshop has no setting");
}
