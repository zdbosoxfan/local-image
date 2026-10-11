use lightcraft_develop::*;
use serde_json::{Value, json};
use std::sync::mpsc;
use std::time::Duration;

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn default_settings_are_neutral_identity_and_omit_default_sections() {
    let d = DevelopSettings::default();
    assert!(d.is_unedited());
    assert_eq!(d.version, 1);
    assert!(d.geometry.is_identity());

    let v = d.to_json();
    for key in ["negative", "raw", "lens_db", "tone_eq", "color_cal", "skin_tone"] {
        assert!(v.get(key).is_none(), "{key} should be left out at defaults");
    }

    let full = d.to_json_full();
    for key in ["negative", "raw", "lens_db", "tone_eq", "color_cal", "skin_tone"] {
        assert!(full.get(key).is_some(), "{key} missing from full JSON");
    }
    assert!(full["effects"].get("structure").is_some());
    assert!(full["effects"].get("clarity_mode").is_some());
    assert!(full["curve"].get("mode").is_some());
    assert!(full.get("look").is_some());
    assert!(full.get("look_options").is_some());
}

#[test]
fn to_json_roundtrip_default_and_edited() {
    let mut s = DevelopSettings::default();
    assert!(controls::set(&mut s, "light.exposure", 1.25));

    let mask: Mask = serde_json::from_value(json!({
        "id": 1,
        "components": [{
            "op": "add",
            "invert": false,
            "shape": {
                "kind": "radial",
                "center": {"x": 0.5, "y": 0.5},
                "rx": 0.2,
                "ry": 0.1,
                "angle": 10.0,
                "feather": 50.0,
                "invert": false
            }
        }]
    }))
    .unwrap();
    s.masks.push(mask);

    let v = s.to_json();
    let back = DevelopSettings::from_json(&v).unwrap();
    assert_eq!(back, s);
    assert_ne!(back.hash64(), DevelopSettings::default().hash64());
}

#[test]
fn missing_fields_use_defaults_and_unknown_fields_are_ignored() {
    let s = DevelopSettings::from_json(&json!({
        "light": {"exposure": 0.5},
        "futureThing": 3,
        "effects": {"clarity": 10, "madeUp": true}
    }))
    .unwrap();

    assert_eq!(s.light.exposure, 0.5);
    assert_eq!(s.light.contrast, 0.0);
    assert_eq!(s.grading.blending, 50.0);
    assert_eq!(s.effects.clarity, 10.0);
    assert_eq!(DevelopSettings::from_json(&json!({})).unwrap(), DevelopSettings::default());
}

#[test]
fn old_partial_data_loads_with_toolset_sections_disabled_and_omitted() {
    let old = json!({
        "version": 1,
        "light": {"exposure": 0.3},
        "optics": {"lens_profile": true},
        "disabled_sections": []
    });

    let s = DevelopSettings::from_json(&old).unwrap();
    assert_eq!(s.negative, Negative::default());
    assert_eq!(s.raw, RawProcessing::default());
    assert_eq!(s.tone_eq, ToneEq::default());
    assert_eq!(s.color_cal, ColorCal::default());
    assert_eq!(s.lens_db, LensDb::default());
    assert!(!s.negative.enabled && !s.raw.capture.enabled && !s.tone_eq.enabled && !s.color_cal.enabled && !s.lens_db.enabled);

    let v = s.to_json();
    for key in ["negative", "raw", "lens_db", "tone_eq", "color_cal"] {
        assert!(v.get(key).is_none(), "{key} written for old settings");
        assert!(s.to_json_full().get(key).is_some(), "{key} missing from full JSON");
    }
}

#[test]
fn hash_is_deterministic_and_edit_sensitive() {
    let a = DevelopSettings::default();
    let b = DevelopSettings::default();
    assert_eq!(a.hash64(), b.hash64());

    let from_json = DevelopSettings::from_json(&json!({"light": {"exposure": 0.25}})).unwrap();
    let mut edited = DevelopSettings::default();
    controls::set(&mut edited, "light.exposure", 0.25);
    assert_eq!(from_json.hash64(), edited.hash64());

    controls::set(&mut edited, "light.exposure", 0.0);
    assert_eq!(edited.hash64(), a.hash64());

    controls::set(&mut edited, "light.exposure", 0.1);
    assert_ne!(edited.hash64(), a.hash64());
}

#[test]
fn is_unedited_ignores_only_white_balance() {
    let mut s = DevelopSettings::default();
    s.wb.temp = 3200.0;
    s.wb.tint = 40.0;
    assert!(s.is_unedited());

    s.light.shadows = 1.0;
    assert!(!s.is_unedited());
}

#[test]
fn for_raw_applies_named_defaults_and_roundtrips() {
    let a = DevelopSettings::for_raw(5200.0, 7.0);
    let b = DevelopSettings::for_raw(5200.0, 7.0);
    assert_eq!(a, b);
    assert_eq!(a.wb.temp, 5200.0);
    assert_eq!(a.wb.tint, 7.0);
    assert_eq!(a.detail.sharpen_amount, 40.0);
    assert_eq!(a.detail.nr_color, 25.0);
    assert!(!a.is_unedited());
    assert_eq!(DevelopSettings::from_json(&a.to_json()).unwrap(), a);
}

#[test]
fn serde_enums_roundtrip_and_from_id() {
    assert_eq!(Look::from_id("sigMoId"), Some(Look::Sigmoid));
    assert_eq!(Look::from_id("Camera"), Some(Look::Camera));
    assert_eq!(Look::from_id("bogus"), None);

    assert_eq!(serde_json::to_value(Look::Camera).unwrap(), json!("camera"));
    assert_eq!(serde_json::to_value(WbMode::Fluorescent).unwrap(), json!("fluorescent"));
    assert_eq!(serde_json::to_value(FilmStock::Bw).unwrap(), json!("bw"));
    assert_eq!(WbMode::Daylight.preset(), Some((5500.0, 10.0)));
    assert_eq!(WbMode::Tungsten.preset(), Some((2850.0, 0.0)));
}

#[test]
fn deep_merge_objects_and_replaces_arrays() {
    let mut base = json!({"x": {"a": 1, "b": 2}, "y": [1]});
    presets::deep_merge(&mut base, &json!({"x": {"b": 3}, "y": [2, 3]}));
    assert_eq!(base, json!({"x": {"a": 1, "b": 3}, "y": [2, 3]}));
}

#[test]
fn extract_groups_is_subset_and_strips_photo_bound_ai_spots() {
    let s = DevelopSettings::from_json(&json!({
        "light": {"exposure": 0.8},
        "crop": {"flip_h": true},
        "spots": [
            {"mode": "remove"},
            {
                "mode": "ai",
                "patch": {
                    "key": "k",
                    "source": "src",
                    "rect": [0.0, 0.0, 1.0, 1.0],
                    "engine": "klein",
                    "seed": 1,
                    "geometry": "g"
                }
            }
        ]
    }))
    .unwrap();

    let light = extract_groups(&s, &[SettingsGroup::Light]);
    assert_eq!(light["light"]["exposure"], json!(0.8));
    assert!(light.get("crop").is_none());
    assert!(light.get("spots").is_none());

    let spots = extract_groups(&s, &[SettingsGroup::Spots]);
    let arr = spots["spots"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["mode"], json!("remove"));
}

#[test]
fn apply_partial_interpolates_and_clamps() {
    let base = DevelopSettings::default();
    let partial = json!({"light": {"exposure": 1.0, "contrast": 40}});

    let half = apply_partial(&base, &partial, 0.5);
    assert!((half.light.exposure - 0.5).abs() < 1e-9);
    assert!((half.light.contrast - 20.0).abs() < 1e-9);

    let more = apply_partial(&base, &partial, 2.0);
    assert!((more.light.contrast - 80.0).abs() < 1e-9);
    assert_eq!(more.light.exposure, 2.0);

    let lots = apply_partial(&base, &json!({"light": {"exposure": 4.0}}), 2.0);
    assert_eq!(lots.light.exposure, 5.0);
}

#[test]
fn apply_partial_merges_nested_objects() {
    let mut base = DevelopSettings::default();
    controls::set(&mut base, "light.exposure", 0.5);
    controls::set(&mut base, "light.contrast", 10.0);

    let partial = json!({"light": {"contrast": 20.0}});
    let out = apply_partial(&base, &partial, 1.0);
    assert_eq!(out.light.exposure, 0.5);
    assert_eq!(out.light.contrast, 20.0);
}

#[test]
fn apply_partial_strict_errors_on_invalid_types() {
    let base = DevelopSettings::default();
    assert!(presets::apply_partial_strict(&base, &json!({"light": {"exposure": "lots"}})).is_err());
}

#[test]
fn apply_partial_recovers_from_malformed_patch_by_keeping_original() {
    let mut base = DevelopSettings::default();
    controls::set(&mut base, "light.exposure", 0.75);

    let out = apply_partial(&base, &json!({"light": {"exposure": "lots"}}), 1.0);
    assert_eq!(out, base);
}

#[test]
fn preset_serde_roundtrip_and_apply_subset() {
    let mut source = DevelopSettings::default();
    controls::set(&mut source, "light.exposure", 0.8);
    controls::set(&mut source, "effects.dehaze", 15.0);
    source.wb.temp = 5000.0;

    let p = Preset::from_settings("p", "P", "User", &source, &[SettingsGroup::Light, SettingsGroup::Effects, SettingsGroup::WhiteBalance]);
    let v = serde_json::to_value(&p).unwrap();
    let p2: Preset = serde_json::from_value(v).unwrap();
    assert_eq!(p, p2);

    let out = p2.apply(&DevelopSettings::default(), 1.0);
    assert_eq!(out.light.exposure, 0.8);
    assert_eq!(out.effects.dehaze, 15.0);
    assert_eq!(out.wb.temp, 5000.0);
    assert_eq!(out.light.contrast, 0.0);
}

#[test]
fn preset_default_copy_excludes_crop_masks_and_spots() {
    let source = DevelopSettings::from_json(&json!({
        "light": {"exposure": 1.0},
        "crop": {"flip_h": true},
        "masks": [{"id": 1, "name": "M", "components": []}],
        "spots": [{"mode": "remove"}]
    }))
    .unwrap();

    let p = Preset::from_settings("p", "P", "User", &source, &SettingsGroup::default_copy());
    assert!(p.settings.get("crop").is_none());
    assert!(p.settings.get("masks").is_none());
    assert!(p.settings.get("spots").is_none());
    assert_eq!(p.settings["light"]["exposure"], json!(1.0));
}

#[test]
fn preset_apply_masks_scales_amount_and_skips_existing_same_mask() {
    let source = DevelopSettings::from_json(&json!({
        "masks": [{
            "id": 1,
            "name": "L",
            "components": [{
                "op": "add",
                "invert": false,
                "shape": {
                    "kind": "radial",
                    "center": {"x": 0.5, "y": 0.5},
                    "rx": 0.2,
                    "ry": 0.1,
                    "angle": 0.0,
                    "feather": 50.0,
                    "invert": false
                }
            }],
            "adjust": {"exposure": 1.0, "amount": 100.0}
        }]
    }))
    .unwrap();

    let p = Preset::from_settings("p", "P", "User", &source, &[SettingsGroup::Masks]);

    let half = p.apply(&DevelopSettings::default(), 0.5);
    assert_eq!(half.masks.len(), 1);
    assert_eq!(half.masks[0].id, 1);
    assert_eq!(half.masks[0].adjust.exposure, 1.0);
    assert_eq!(half.masks[0].adjust.amount, 50.0);

    let again = p.apply(&half, 0.5);
    assert_eq!(again.masks.len(), 1, "same preset mask must not be duplicated");
    assert_eq!(again.masks[0].id, 1);
}

#[test]
fn applying_spot_partial_preserves_existing_ai_spots() {
    let target = DevelopSettings::from_json(&json!({
        "spots": [{
            "mode": "ai",
            "patch": {
                "key": "k",
                "source": "src",
                "rect": [0.0, 0.0, 1.0, 1.0],
                "engine": "klein",
                "seed": 1,
                "geometry": "g"
            }
        }]
    }))
    .unwrap();

    let partial = json!({"spots": [{"mode": "remove"}]});
    let out = apply_partial(&target, &partial, 1.0);
    assert_eq!(out.spots.len(), 2);
    assert!(out.spots[0].is_ai());
    assert_eq!(out.spots[1].mode, SpotMode::Remove);
}

#[test]
fn mask_old_fields_are_omitted_when_default() {
    let old = json!({
        "masks": [{
            "id": 3,
            "name": "Sky",
            "visible": true,
            "invert": false,
            "components": [],
            "adjust": {"exposure": -0.5, "saturation": 20.0, "amount": 80.0}
        }]
    });

    let s = DevelopSettings::from_json(&old).unwrap();
    let v = s.to_json();
    assert!(v["masks"][0].get("opacity").is_none());
    assert!(v["masks"][0].get("tools").is_none());
    assert_eq!(DevelopSettings::from_json(&v).unwrap(), s);
}

#[test]
fn mask_shape_variants_roundtrip() {
    let s = DevelopSettings::from_json(&json!({
        "masks": [
            {
                "id": 1,
                "components": [{
                    "op": "add",
                    "invert": false,
                    "shape": {
                        "kind": "radial",
                        "center": {"x": 0.5, "y": 0.5},
                        "rx": 0.2,
                        "ry": 0.1,
                        "angle": 10.0,
                        "feather": 50.0,
                        "invert": false
                    }
                }]
            },
            {
                "id": 2,
                "components": [{
                    "op": "subtract",
                    "invert": false,
                    "shape": {
                        "kind": "linear",
                        "start": {"x": 0.0, "y": 0.0},
                        "end": {"x": 1.0, "y": 1.0}
                    }
                }]
            }
        ]
    }))
    .unwrap();

    let v = serde_json::to_value(&s).unwrap();
    let back: DevelopSettings = serde_json::from_value(v).unwrap();
    assert_eq!(s, back);
    assert_eq!(s.masks.len(), 2);
    assert_eq!(s.masks[0].components[0].op, MaskOp::Add);
    assert_eq!(s.masks[1].components[0].op, MaskOp::Subtract);
}

#[test]
fn controls_set_clamps_values_and_rejects_unknown_ids() {
    let mut s = DevelopSettings::default();

    assert!(controls::set(&mut s, "light.exposure", 999.0));
    assert_eq!(s.light.exposure, 5.0);
    assert!(!controls::set(&mut s, "light.notARealSlider", 1.0));
    assert_eq!(controls::get(&s, "light.exposure"), Some(5.0));

    assert!(controls::set(&mut s, "toneEq.ev8", 5.0));
    assert_eq!(s.tone_eq.ev8, 2.0);
}

#[test]
fn section_toggles_and_reset_preserve_section_enabled_state() {
    let mut s = DevelopSettings::default();

    assert!(s.section_enabled("effects"));
    s.set_section_enabled("effects", false);
    s.set_section_enabled("effects", false);
    assert!(!s.section_enabled("effects"));
    assert_eq!(s.disabled_sections.len(), 1);

    s.set_section_enabled("effects", true);
    assert!(s.section_enabled("effects"));
    assert!(s.disabled_sections.is_empty());

    s.negative.enabled = true;
    s.negative.d_max = 1.6;
    s.reset_section(Section::Negative);
    assert_eq!(s.negative, Negative { enabled: true, ..Negative::default() });

    s.raw.capture.enabled = true;
    s.raw.demosaic = Demosaic::Rcd;
    s.reset_section(Section::Raw);
    assert!(s.raw.capture.enabled);
    assert_eq!(s.raw.demosaic, Demosaic::Auto);
}

#[test]
fn layer_tools_are_sparse_merge_and_reject_image_controls() {
    let t = LayerTools::default()
        .merged(
            (5000.0, 4.0),
            &json!({
                "light": {"exposure": 9.0},
                "mixer": {"blue": {"sat": -40}}
            }),
        )
        .unwrap();

    assert_eq!(t.light.unwrap().exposure, 5.0);
    assert_eq!(t.mixer.unwrap().blue.sat, -40.0);
    assert_eq!(t.used_keys(), vec!["light", "mixer"]);

    let t = t.merged((5000.0, 4.0), &json!({"light": {"contrast": 20}, "mixer": null})).unwrap();
    assert_eq!(t.light.unwrap().exposure, 5.0);
    assert_eq!(t.light.unwrap().contrast, 20.0);
    assert!(t.mixer.is_none());

    assert!(t.merged((5000.0, 4.0), &json!({"optics": {"distortion": 10}})).is_err());
    assert!(t.merged((5000.0, 4.0), &json!({"light": {"exposure": "lots"}})).is_err());
}

#[test]
fn layer_tools_view_and_set_controls_by_id() {
    let t = LayerTools::default();
    let view = t.view((5000.0, 4.0));
    assert_eq!(view.wb.temp, 5000.0);
    assert_eq!(view.wb.tint, 4.0);
    assert_eq!(view.wb.mode, WbMode::Custom);
    assert!(t.used_keys().is_empty());

    let set = t.set_controls((5000.0, 4.0), &[("wb.tint".into(), 12.0)]).unwrap();
    assert_eq!(set.wb.unwrap().temp, 5000.0);
    assert_eq!(set.wb.unwrap().tint, 12.0);

    assert_eq!(layer_key_of_control("bw.red"), Some("bw_mix"));
    assert_eq!(layer_key_of_control("calibration.redHue"), None);

    let t_point = LayerTools { point_colors: Some(vec![PointColor::default()]), ..Default::default() };
    let p = t_point.set_controls((6500.0, 0.0), &[("pointColor.0.hueShift".into(), 30.0), ("effects.clarity".into(), 15.0)]).unwrap();
    assert_eq!(p.point_colors.unwrap()[0].hue_shift, 30.0);
    assert_eq!(p.effects.unwrap().clarity, 15.0);

    assert!(t_point.set_controls((6500.0, 0.0), &[("optics.distortion".into(), 1.0)]).is_err());
}

#[test]
fn malformed_inputs_return_errors_not_panics() {
    let cases = [
        Value::Null,
        json!([]),
        json!({"light": {"exposure": "lots"}}),
        json!({"masks": [{"components": [{"shape": {"kind": "bogus"}}]}]}),
        json!({"orientation": "sideways"}),
    ];

    for case in &cases {
        let _ = DevelopSettings::from_json(case);
        let _ = DevelopSettings::default().merged(case);
        let _ = apply_partial(&DevelopSettings::default(), case, 1.0);
    }
}

#[test]
fn settings_are_send_sync_and_threads_finish_within_timeout() {
    assert_send_sync::<DevelopSettings>();
    assert_send_sync::<Preset>();

    let base = DevelopSettings::default();
    let mut expected_settings = base.clone();
    controls::set(&mut expected_settings, "light.exposure", 0.25);
    let expected_hash = expected_settings.hash64();

    let (tx, rx) = mpsc::channel();
    for _ in 0..4 {
        let s = base.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut local = s;
            controls::set(&mut local, "light.exposure", 0.25);
            tx.send((local.hash64(), local.light.exposure)).unwrap();
        });
    }
    drop(tx);

    let mut results = Vec::new();
    while let Ok(result) = rx.recv_timeout(Duration::from_secs(1)) {
        results.push(result);
        if results.len() == 4 {
            break;
        }
    }

    assert_eq!(results.len(), 4);
    for (hash, exposure) in results {
        assert_eq!(hash, expected_hash);
        assert!((exposure - 0.25).abs() < 1e-9);
    }
}
