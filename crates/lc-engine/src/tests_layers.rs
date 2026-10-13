//! Develop layers: `mask.setTools`, `mask.setOpacity`, `mask.resetTools`, a layer's Point Color,
//! and copy / paste / presets carrying layer tools (Auto Sync never carries layers).

use lightcraft_catalog::PhotoId;
use serde_json::json;

use crate::Session;

fn active_dev(s: &Session) -> lightcraft_develop::DevelopSettings {
    (*s.develop_of(s.active().unwrap()).unwrap()).clone()
}

fn with_layer() -> Session {
    let mut s = Session::with_demo();
    s.execute("mask.add", &json!({"kind": "radial", "center": [0.5, 0.5]})).unwrap();
    s
}

fn history_labels(s: &Session) -> Vec<String> {
    let id = s.active().unwrap();
    s.catalog.photo(id).unwrap().history.iter().map(|h| h.label.clone()).collect()
}

#[test]
fn set_tools_merges_values_and_partials() {
    let mut s = with_layer();
    let r = s.execute("mask.setTools", &json!({"values": {"light.exposure": 0.7}})).unwrap();
    assert_eq!(r["tools"]["light"]["exposure"], json!(0.7));
    assert_eq!(history_labels(&s).last().map(String::as_str), Some("Exposure – Radial Gradient 1"));
    // a partial merges with what's there (like a preset), clamped through the control specs
    s.execute("mask.setTools", &json!({"tools": {"light": {"contrast": 500}, "curve": {"master": [{"x": 0.0, "y": 0.1}, {"x": 1.0, "y": 1.0}]}}}))
        .unwrap();
    let m = &active_dev(&s).masks[0];
    let l = m.tools.light.unwrap();
    assert_eq!((l.exposure, l.contrast), (0.7, 100.0));
    assert_eq!(m.tools.curve.as_ref().unwrap().master.len(), 2);
    assert_eq!(m.tools.used_keys(), ["light", "curve"]);
    assert_eq!(history_labels(&s).last().map(String::as_str), Some("Layer Adjustment – Radial Gradient 1"));
    // null drops a section; indexed and colour controls work
    s.execute("mask.setTools", &json!({"tools": {"curve": null}, "values": {"mixer.blue.sat": -30, "grading.shadows.sat": 40}})).unwrap();
    let t = active_dev(&s).masks[0].tools.clone();
    assert!(t.curve.is_none());
    assert_eq!((t.mixer.unwrap().blue.sat, t.grading.unwrap().shadows.sat), (-30.0, 40.0));
    // white balance starts from the photo's
    s.execute("mask.setTools", &json!({"values": {"wb.tint": 15}})).unwrap();
    let wb = active_dev(&s).masks[0].tools.wb.unwrap();
    let id = s.active().unwrap();
    let info = s.source_info(id);
    assert_eq!((wb.temp, wb.tint), (lightcraft_pipeline::local::effective_wb(&info, &active_dev(&s)).0, 15.0));
    // refused: image-level tools, unknown controls, nothing to set, no such mask
    assert!(s.execute("mask.setTools", &json!({"tools": {"crop": {}}})).is_err());
    assert!(s.execute("mask.setTools", &json!({"values": {"optics.distortion": 5}})).is_err());
    assert!(s.execute("mask.setTools", &json!({"values": {"light.nope": 5}})).is_err());
    assert!(s.execute("mask.setTools", &json!({})).is_err());
    assert!(s.execute("mask.setTools", &json!({"id": 99, "values": {"light.exposure": 1}})).is_err());
    // one undo step each
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(active_dev(&s).masks[0].tools.wb.is_none());
}

#[test]
fn opacity_and_reset() {
    let mut s = with_layer();
    s.execute("mask.setOpacity", &json!({"value": 140})).unwrap();
    assert_eq!(active_dev(&s).masks[0].opacity, 100.0);
    s.execute("mask.setOpacity", &json!({"value": 35})).unwrap();
    assert_eq!(active_dev(&s).masks[0].opacity, 35.0);
    s.execute("mask.adjust", &json!({"values": {"exposure": 1.0, "amount": 70}})).unwrap();
    s.execute("mask.setTools", &json!({"values": {"effects.clarity": 30, "light.exposure": 0.5}})).unwrap();
    s.execute("mask.resetTools", &json!({"section": "effects"})).unwrap();
    let m = active_dev(&s).masks[0].clone();
    assert_eq!(m.tools.used_keys(), ["light"]);
    assert!(s.execute("mask.resetTools", &json!({"section": "optics"})).is_err());
    s.execute("mask.resetTools", &json!({})).unwrap();
    let m = active_dev(&s).masks[0].clone();
    assert!(m.tools.is_empty());
    assert_eq!((m.opacity, m.adjust.exposure, m.adjust.amount), (100.0, 0.0, 70.0));
    assert_eq!(history_labels(&s).last().map(String::as_str), Some("Reset Layer"));
    // the photo renders (on the CPU) with layer tools
    s.execute("mask.setTools", &json!({"values": {"curve.darks": 40, "light.contrast": 20}})).unwrap();
    let id = s.active().unwrap();
    assert!(s.render_now(id, 160, 160).is_ok());
}

#[test]
fn point_color_on_a_layer() {
    let mut s = with_layer();
    let mid = s.active_mask.unwrap();
    let r = s.execute("pointColor.pick", &json!({"x": 0.5, "y": 0.5, "mask": mid})).unwrap();
    assert_eq!(r["index"], json!(0));
    let d = active_dev(&s);
    assert!(d.point_colors.is_empty(), "the photo's own samples are untouched");
    assert_eq!(d.masks[0].tools.point_colors.as_ref().map(Vec::len), Some(1));
    s.execute("mask.setTools", &json!({"values": {"pointColor.0.hueShift": 40}})).unwrap();
    assert_eq!(active_dev(&s).masks[0].tools.point_colors.as_ref().unwrap()[0].hue_shift, 40.0);
    s.execute("pointColor.delete", &json!({"index": 0, "mask": mid})).unwrap();
    assert_eq!(active_dev(&s).masks[0].tools.point_colors.as_ref().map(Vec::len), Some(0));
    assert!(s.execute("pointColor.pick", &json!({"x": 0.5, "y": 0.5, "mask": 77})).is_err());
}

#[test]
fn copy_paste_presets_and_auto_sync_carry_layers() {
    let mut s = with_layer();
    s.execute("mask.setTools", &json!({"values": {"grading.midtones.sat": 30, "light.exposure": -0.4}})).unwrap();
    s.execute("mask.setOpacity", &json!({"value": 60})).unwrap();
    let src = active_dev(&s).masks.clone();
    let vis = s.visible_cloned();
    // copy with the Masking group, paste onto two others
    s.execute("develop.copy", &json!({"groups": ["masks"]})).unwrap();
    s.execute("library.select", &json!({"ids": [vis[1].0, vis[2].0]})).unwrap();
    s.execute("develop.paste", &json!({})).unwrap();
    assert_eq!(s.develop_of(vis[2]).unwrap().masks, src);
    // a preset with masks
    s.execute("library.select", &json!({"ids": [vis[0].0]})).unwrap();
    let p = s.execute("preset.create", &json!({"name": "Layered", "groups": ["masks"]})).unwrap()["id"].as_str().unwrap().to_string();
    s.execute("library.select", &json!({"ids": [vis[3].0]})).unwrap();
    s.execute("preset.apply", &json!({"id": p, "amount": 100})).unwrap();
    let m = &s.develop_of(vis[3]).unwrap().masks[0];
    assert_eq!((&m.tools, m.opacity), (&src[0].tools, 60.0));
    // Auto Sync never carries a layer tool change: layers sync only through Sync / Copy, so the
    // other photo keeps its own layers (TEST-CHANGED: used to expect the change to sync)
    let ids = [vis[0].0, vis[1].0];
    s.execute("library.select", &json!({"ids": ids})).unwrap();
    s.selection.active = Some(PhotoId(ids[0]));
    s.active_mask = Some(src[0].id);
    s.execute("develop.autoSync", &json!({"on": true})).unwrap();
    let other = s.develop_of(PhotoId(ids[1])).unwrap().masks.clone();
    s.execute("mask.setTools", &json!({"values": {"effects.dehaze": 25}})).unwrap();
    assert_eq!(s.develop_of(PhotoId(ids[0])).unwrap().masks[0].tools.effects.unwrap().dehaze, 25.0);
    assert_eq!(s.develop_of(PhotoId(ids[1])).unwrap().masks, other);
}

/// Data-loss regression: with Auto Sync on, a layer / mask edit on the active photo used to
/// replace the whole layer stack of every other selected photo (`json_delta` takes arrays whole).
#[test]
fn auto_sync_keeps_each_photos_own_layers() {
    let mut s = Session::with_demo();
    let vis = s.visible_cloned();
    let (a, b) = (vis[0], vis[1]);
    // B has two layers of its own
    s.execute("library.select", &json!({"ids": [b.0], "active": b.0})).unwrap();
    s.execute("mask.add", &json!({"kind": "radial", "center": [0.2, 0.2]})).unwrap();
    s.execute("mask.setTools", &json!({"values": {"light.exposure": -0.8}})).unwrap();
    s.execute("mask.add", &json!({"kind": "radial", "center": [0.8, 0.8]})).unwrap();
    let b_masks = s.develop_of(b).unwrap().masks.clone();
    assert_eq!(b_masks.len(), 2);
    // A has one layer; both are selected with A active and Auto Sync on
    s.execute("library.select", &json!({"ids": [a.0], "active": a.0})).unwrap();
    s.execute("mask.add", &json!({"kind": "radial", "center": [0.5, 0.5]})).unwrap();
    s.execute("library.select", &json!({"ids": [a.0, b.0], "active": a.0})).unwrap();
    assert_eq!(s.active(), Some(a));
    s.active_mask = Some(s.develop_of(a).unwrap().masks[0].id);
    s.execute("develop.autoSync", &json!({"on": true})).unwrap();
    // a global slider still syncs, and B keeps its layers
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.6})).unwrap();
    assert_eq!(s.develop_of(b).unwrap().light.exposure, 0.6, "global settings still auto sync");
    assert_eq!(s.develop_of(b).unwrap().masks, b_masks);
    // layer edits on A: tools, opacity, a new layer, a deleted layer
    s.execute("mask.setTools", &json!({"values": {"effects.dehaze": 25}})).unwrap();
    s.execute("mask.setOpacity", &json!({"value": 40})).unwrap();
    s.execute("mask.add", &json!({"kind": "radial", "center": [0.3, 0.7]})).unwrap();
    assert_eq!(s.develop_of(a).unwrap().masks.len(), 2);
    assert_eq!(s.develop_of(b).unwrap().masks, b_masks, "B keeps its own layers");
    let first = s.develop_of(a).unwrap().masks[0].id;
    s.execute("mask.delete", &json!({"id": first})).unwrap();
    assert_eq!(s.develop_of(a).unwrap().masks.len(), 1);
    assert_eq!(s.develop_of(b).unwrap().masks, b_masks, "B keeps its own layers");
    // and a global change after the layer edits still reaches B
    s.execute("develop.set", &json!({"control": "light.contrast", "value": 15})).unwrap();
    assert_eq!(s.develop_of(b).unwrap().light.contrast, 15.0);
    assert_eq!(s.develop_of(b).unwrap().masks, b_masks);
}

#[test]
fn develop_a_layer_wb_curve_target_and_undo_selection() {
    let mut s = Session::with_demo();
    s.execute("mask.add", &json!({"kind":"full"})).unwrap();
    s.execute("mask.rename", &json!({"name":"Face"})).unwrap();
    let global = active_dev(&s);
    s.execute("develop.wbPick", &json!({"x":0.4,"y":0.5,"mask":1})).unwrap();
    assert_eq!(active_dev(&s).wb, global.wb);
    assert!(active_dev(&s).masks[0].tools.wb.is_some());
    assert_eq!(history_labels(&s).last().unwrap(), "White Balance – Face");
    s.execute("curve.applyPreset", &json!({"name":"Medium Contrast","mask":1})).unwrap();
    assert_eq!(active_dev(&s).curve, global.curve);
    assert!(active_dev(&s).masks[0].tools.curve.is_some());
    assert!(s.execute("curve.applyPreset", &json!({"name":"Medium Contrast","mask":99})).is_err());
    assert!(s.execute("develop.wbPick", &json!({"x":0.4,"y":0.5,"mask":99})).is_err());
    while !active_dev(&s).masks.is_empty() {
        s.undo_step().unwrap();
    }
    assert_eq!(s.active_mask, None);
    s.redo_step().unwrap();
    assert_eq!(s.active_mask, None); // selection itself stays UI state
    s.active_mask = Some(99);
    s.resolve_active_mask();
    assert_eq!(s.active_mask, None);
}

#[test]
fn develop_a_sync_copy_layers_whole_and_strip_ai_caches() {
    let mut s = Session::with_demo();
    let a = s.active().unwrap();
    let b = PhotoId(2);
    s.execute("mask.add", &json!({"kind":"full"})).unwrap();
    s.execute("mask.rename", &json!({"name":"Sky"})).unwrap();
    s.execute("mask.setTools", &json!({"values":{"light.exposure":1.0}})).unwrap();
    s.selection = crate::Selection { ids: vec![a, b], active: Some(a) };
    s.execute("develop.sync", &json!({"groups":["masks"]})).unwrap();
    assert_eq!(s.develop_of(b).unwrap().masks, active_dev(&s).masks);
    assert_eq!(s.catalog.photo(b).unwrap().history.last().unwrap().label, "Sync Settings");
    s.execute("develop.copy", &json!({"groups":["light"]})).unwrap();
    s.selection = crate::Selection::single(b);
    let masks = active_dev(&s).masks.clone();
    s.execute("develop.paste", &json!({})).unwrap();
    assert_eq!(active_dev(&s).masks, masks);
    s.selection = crate::Selection::single(a);
    s.execute("mask.add", &json!({"kind":"sky"})).unwrap();
    let mut settings = active_dev(&s);
    settings.masks[1].components[0].shape =
        lightcraft_develop::MaskShape::Sky { seg: Some(lightcraft_develop::SegMask::from_logits(2, &[1.0, -1.0, 1.0, -1.0])) };
    s.set_develop(a, settings, "AI fixture").unwrap();
    s.execute("develop.copy", &json!({"groups":["masks"]})).unwrap();
    assert!(s.clipboard.as_ref().unwrap()["masks"][1]["components"][0]["shape"].get("seg").is_none());
}
