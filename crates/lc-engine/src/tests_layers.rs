//! Develop layers: `mask.setTools`, `mask.setOpacity`, `mask.resetTools`, a layer's Point Color,
//! and copy / paste / presets / Auto Sync carrying layer tools.

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
    assert_eq!(history_labels(&s).last().map(String::as_str), Some("Exposure"));
    // a partial merges with what's there (like a preset), clamped through the control specs
    s.execute("mask.setTools", &json!({"tools": {"light": {"contrast": 500}, "curve": {"master": [{"x": 0.0, "y": 0.1}, {"x": 1.0, "y": 1.0}]}}}))
        .unwrap();
    let m = &active_dev(&s).masks[0];
    let l = m.tools.light.unwrap();
    assert_eq!((l.exposure, l.contrast), (0.7, 100.0));
    assert_eq!(m.tools.curve.as_ref().unwrap().master.len(), 2);
    assert_eq!(m.tools.used_keys(), ["light", "curve"]);
    assert_eq!(history_labels(&s).last().map(String::as_str), Some("Layer Adjustment"));
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
    // Auto Sync carries a layer tool change
    let ids = [vis[0].0, vis[1].0];
    s.execute("library.select", &json!({"ids": ids})).unwrap();
    s.selection.active = Some(PhotoId(ids[0]));
    s.active_mask = Some(src[0].id);
    s.execute("develop.autoSync", &json!({"on": true})).unwrap();
    s.execute("mask.setTools", &json!({"values": {"effects.dehaze": 25}})).unwrap();
    assert_eq!(s.develop_of(PhotoId(ids[1])).unwrap().masks[0].tools.effects.unwrap().dehaze, 25.0);
}
