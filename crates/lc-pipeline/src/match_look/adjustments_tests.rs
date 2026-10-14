use super::*;
use serde_json::{Value, json};

const TOOLS: [&str; 19] = [
    "exposure",
    "whiteBalance",
    "contrast",
    "highlights",
    "shadows",
    "whites",
    "blacks",
    "lumaCurve",
    "saturation",
    "vibrance",
    "colorEditor",
    "blackWhite",
    "masterBalance",
    "shadowsBalance",
    "midtonesBalance",
    "highlightsBalance",
    "redCurve",
    "greenCurve",
    "blueCurve",
];
fn adjustments(default: bool, exception: Option<&str>) -> Adjustments {
    let mut value = json!({});
    for name in TOOLS {
        value[name] = json!(if exception == Some(name) { !default } else { default });
    }
    serde_json::from_value(value).unwrap()
}
fn tool(settings: &DevelopSettings, name: &str) -> Value {
    match name {
        "exposure" => json!(settings.light.exposure),
        "whiteBalance" => json!(settings.wb),
        "contrast" => json!(settings.light.contrast),
        "highlights" => json!(settings.light.highlights),
        "shadows" => json!(settings.light.shadows),
        "whites" => json!(settings.light.whites),
        "blacks" => json!(settings.light.blacks),
        "lumaCurve" => {
            let mut curve = settings.curve.clone();
            curve.red.clear();
            curve.green.clear();
            curve.blue.clear();
            json!(curve)
        }
        "saturation" => json!(settings.color.saturation),
        "vibrance" => json!(settings.color.vibrance),
        "colorEditor" => json!(settings.mixer),
        "blackWhite" => json!(settings.treatment),
        "masterBalance" => json!(settings.grading.global),
        "shadowsBalance" => json!(settings.grading.shadows),
        "midtonesBalance" => json!(settings.grading.midtones),
        "highlightsBalance" => json!(settings.grading.highlights),
        "redCurve" => json!(settings.curve.red),
        "greenCurve" => json!(settings.curve.green),
        "blueCurve" => json!(settings.curve.blue),
        _ => unreachable!(),
    }
}
fn edited() -> DevelopSettings {
    let mut d = DevelopSettings::default();
    d.wb = lightcraft_develop::WhiteBalance { mode: WbMode::Custom, temp: 8200.0, tint: 8.0 };
    d.light.exposure = 0.7;
    d.light.contrast = 17.0;
    d.light.highlights = -18.0;
    d.light.shadows = 15.0;
    d.light.whites = 7.0;
    d.light.blacks = -9.0;
    d.curve.darks = -5.0;
    d.curve.lights = 8.0;
    d.color.saturation = -25.0;
    d.color.vibrance = 11.0;
    d.mixer.orange.hue = -12.0;
    d.mixer.orange.sat = -20.0;
    d.mixer.blue.lum = 9.0;
    d.grading.global = lightcraft_develop::Wheel { hue: 210.0, sat: 9.0, lum: 3.0 };
    d.grading.shadows = lightcraft_develop::Wheel { hue: 190.0, sat: 10.0, lum: -2.0 };
    d.grading.midtones = lightcraft_develop::Wheel { hue: 35.0, sat: 7.0, lum: 2.0 };
    d.grading.highlights = lightcraft_develop::Wheel { hue: 55.0, sat: 8.0, lum: 1.0 };
    d.curve.red = vec![Point::new(0.0, 0.0), Point::new(0.5, 0.55), Point::new(1.0, 1.0)];
    d.curve.green = vec![Point::new(0.0, 0.0), Point::new(0.5, 0.47), Point::new(1.0, 1.0)];
    d.curve.blue = vec![Point::new(0.0, 0.0), Point::new(0.5, 0.57), Point::new(1.0, 1.0)];
    d
}
fn source() -> Arc<Rgb32f> {
    Arc::new(Rgb32f::from_fn(32, 24, |x, y| {
        let l = 0.04 + 0.4 * x as f32 / 31.0;
        match y / 6 {
            0 => [l; 3],
            1 => [l * 1.3, l * 0.7, l * 0.5],
            2 => [l * 0.55, l * 0.9, l * 1.2],
            _ => [l * 0.65, l, l * 0.7],
        }
    }))
}
fn relative(before: &DevelopSettings, after: &DevelopSettings) -> DevelopSettings {
    let mut d = DevelopSettings::default();
    d.treatment = after.treatment;
    if after.masks.len() > before.masks.len() {
        let mask = after.masks.last().unwrap();
        d.light = mask.tools.light.unwrap_or_default();
        d.color = mask.tools.color.unwrap_or_default();
        d.mixer = mask.tools.mixer.unwrap_or_default();
        d.grading = mask.tools.grading.unwrap_or_default();
        d.curve = mask.tools.curve.clone().unwrap_or_default();
    }
    d
}

#[test]
fn individual_adjustment_schema_defaults_and_legacy_locks() {
    let old: Options = serde_json::from_value(json!({"strength":75})).unwrap();
    assert_eq!(old.adjustments, Adjustments::default());
    let partial: Options = serde_json::from_value(json!({"adjustments":{"saturation":false}})).unwrap();
    assert!(!partial.adjustments.saturation && partial.adjustments.vibrance && partial.adjustments.master_balance);
    let held = Options { keep_exposure: true, keep_white_balance: true, ..old };
    assert!(!held.exposure_enabled() && !held.white_balance_enabled());
    assert!(!parameters(held).iter().any(|p| matches!(p, Parameter::Temperature | Parameter::Slider("light.exposure" | "wb.tint", _))));
}

#[test]
fn individual_exclusions_survive_amount_and_shared_copy_at_every_strength() {
    let mut before = DevelopSettings::default();
    before.wb.mode = WbMode::Shade;
    before.color.saturation = 13.0;
    before.grading.balance = 19.0;
    before.grading.blending = 62.0;
    before.masks.push(Mask { id: 7, name: "Existing".into(), ..Default::default() });
    let mut full = edited();
    full.treatment = Treatment::Bw;
    full.grading.balance = -30.0;
    full.grading.blending = 21.0;
    for excluded in TOOLS {
        for strength in [0.0, 37.0, 100.0, 125.0] {
            for consistent_batch in [false, true] {
                let opts = Options { adjustments: adjustments(true, Some(excluded)), strength, consistent_batch, ..Default::default() };
                let faded = amount_settings(&before, &full, opts);
                assert_eq!(tool(&faded, excluded), tool(&before, excluded), "global {excluded} at {strength}, consistent={consistent_batch}");
                assert_eq!(faded.grading.balance, before.grading.balance);
                assert_eq!(faded.grading.blending, before.grading.blending);
                let mut copied = before.clone();
                copy_color_controls(&mut copied, &full, opts);
                if !matches!(excluded, "exposure" | "whiteBalance" | "contrast" | "highlights" | "shadows" | "whites" | "blacks" | "lumaCurve") {
                    assert_eq!(tool(&copied, excluded), tool(&before, excluded), "shared copy {excluded}");
                }
                let all = Options { new_layer: true, ..Default::default() };
                let layered = layer_settings(&before, &full, all, (6500.0, 0.0));
                let layer_opts = Options { new_layer: true, ..opts };
                let faded = amount_settings(&before, &layered, layer_opts);
                assert_eq!(faded.masks[0], before.masks[0]);
                assert_eq!(faded.masks.len(), before.masks.len() + 1);
                let neutral = DevelopSettings::default();
                if excluded == "blackWhite" {
                    assert_eq!(faded.treatment, before.treatment);
                } else if excluded == "whiteBalance" {
                    assert_eq!((faded.masks.last().unwrap().adjust.temp, faded.masks.last().unwrap().adjust.tint), (0.0, 0.0));
                } else {
                    assert_eq!(tool(&relative(&before, &faded), excluded), tool(&neutral, excluded), "relative layer {excluded} at {strength}");
                }
            }
        }
    }
}

#[test]
fn individual_exclusions_hold_through_real_independent_and_locked_fits() {
    let src = source();
    let info = SourceInfo::default();
    let mut base = DevelopSettings::default();
    base.wb.mode = WbMode::Shade;
    base.color.saturation = 12.0;
    base.grading.global.hue = 45.0;
    base.grading.global.sat = 3.0;
    base.grading.balance = 12.0;
    base.grading.blending = 65.0;
    base.masks.push(Mask { id: 3, name: "Prior mask".into(), ..Default::default() });
    let desired = edited();
    let refs = [analyze(&src, &info, &desired)];
    let bw_refs = [analyze(&src, &info, &DevelopSettings { treatment: Treatment::Bw, ..desired.clone() })];
    let mut changes = 0;
    for new_layer in [false, true] {
        for (index, excluded) in TOOLS.into_iter().enumerate() {
            for consistent_batch in [false, true] {
                let opts = Options {
                    adjustments: adjustments(true, Some(excluded)),
                    new_layer,
                    consistent_batch,
                    strength: if index % 2 == 0 { 37.0 } else { 100.0 },
                    ..Default::default()
                };
                let references = if excluded == "blackWhite" { &bw_refs } else { &refs };
                let result = if consistent_batch {
                    solve_batch_locked_color_with_faces(&src, &info, &base, references, opts, &[], &desired, &|| false)
                } else {
                    solve_batch_with_faces(&src, &info, &base, references, opts, &[], &|| false)
                }
                .unwrap();
                changes += usize::from(result.settings != base);
                assert_eq!(result.settings.masks[0], base.masks[0]);
                for settings in [&result.settings, &result.full_settings] {
                    if !new_layer || excluded == "blackWhite" || excluded == "whiteBalance" {
                        assert_eq!(tool(settings, excluded), tool(&base, excluded), "fit {excluded}, layer={new_layer}, shared={consistent_batch}");
                    }
                    if new_layer && settings.masks.len() > base.masks.len() {
                        if excluded == "whiteBalance" {
                            let mask = settings.masks.last().unwrap();
                            assert_eq!((mask.adjust.temp, mask.adjust.tint), (0.0, 0.0));
                        } else if excluded != "blackWhite" {
                            assert_eq!(
                                tool(&relative(&base, settings), excluded),
                                tool(&DevelopSettings::default(), excluded),
                                "relative fitted {excluded}"
                            );
                        }
                    }
                }
                assert_eq!(amount_settings(&base, &result.full_settings, opts), result.settings, "re-amount drift for {excluded}");
                for group in ["normalize", "light", "color", "tone"] {
                    let Some(reset) = reset_group(&base, &result.full_settings, opts, group) else {
                        assert!(new_layer && result.full_settings.masks.len() == base.masks.len());
                        continue;
                    };
                    if !new_layer || excluded == "blackWhite" || excluded == "whiteBalance" {
                        assert_eq!(tool(&reset, excluded), tool(&base, excluded), "reset {group} changed excluded {excluded}");
                    }
                    if new_layer && excluded == "whiteBalance" {
                        let mask = reset.masks.last().unwrap();
                        assert_eq!((mask.adjust.temp, mask.adjust.tint), (0.0, 0.0));
                    } else if new_layer && excluded != "blackWhite" {
                        assert_eq!(
                            tool(&relative(&base, &reset), excluded),
                            tool(&DevelopSettings::default(), excluded),
                            "reset {group} changed excluded relative {excluded}"
                        );
                    }
                }
            }
        }
    }
    assert!(changes > 60, "fixtures must exercise real edits, not no-op preservation: {changes}");
}

#[test]
fn master_balance_is_a_real_color_solver_and_all_excluded_is_noop() {
    let src = source();
    let info = SourceInfo::default();
    let base = DevelopSettings::default();
    let mut desired = base.clone();
    desired.grading.global.hue = 215.0;
    desired.grading.global.sat = 15.0;
    let refs = [analyze(&src, &info, &desired)];
    let opts = Options { adjustments: adjustments(false, Some("masterBalance")), ..Default::default() };
    let result = solve(&src, &info, &base, &refs, opts, &|| false).unwrap();
    assert_eq!(result.settings.treatment, Treatment::Color);
    assert!(result.settings.grading.global.sat > 1.0, "Master must fit a color reference");
    assert!(result.after_error < result.before_error * 0.6, "Master-only error {} -> {}", result.before_error, result.after_error);
    for name in TOOLS.into_iter().filter(|name| *name != "masterBalance") {
        assert_eq!(tool(&result.settings, name), tool(&base, name), "Master-only changed {name}");
    }
    for new_layer in [false, true] {
        let none = Options { adjustments: adjustments(false, None), new_layer, consistent_batch: true, ..Default::default() };
        let result = solve_batch_locked_color_with_faces(&src, &info, &base, &refs, none, &[], &desired, &|| false).unwrap();
        assert_eq!(result.settings, base);
        assert_eq!(result.full_settings, base);
    }
}

#[test]
fn managed_layer_placement_preserves_later_layers_and_selects_owned_tools() {
    let mut before = DevelopSettings::default();
    before.masks.push(Mask { id: 1, name: "Below".into(), ..Default::default() });
    let mut above = Mask { id: 9, name: "Later user layer".into(), ..Default::default() };
    above.tools.color = Some(lightcraft_develop::ColorAdj { saturation: 77.0, vibrance: -17.0 });
    above.tools.grading = Some(edited().grading);
    before.masks.push(above.clone());
    let opts = Options { new_layer: true, consistent_batch: true, layer_placement: Some(LayerPlacement { index: 1, id: 5 }), ..Default::default() };
    let full = layer_settings(&before, &edited(), opts, (6500.0, 0.0));
    assert_eq!(full.masks.iter().map(|mask| mask.id).collect::<Vec<_>>(), vec![1, 5, 9]);
    assert_eq!(full.masks[2], above);
    assert_eq!(shared_color_settings(&before, &full, opts).color, edited().color);
    let faded = amount_settings(&before, &full, Options { strength: 50.0, ..opts });
    assert_eq!(faded.masks[2], above);
    assert_eq!(faded.masks[1].tools.color.unwrap().saturation, edited().color.saturation * 0.5);
    let reset = reset_group(&before, &full, opts, "tone").unwrap();
    assert_eq!(reset.masks[2], above);
    assert!(reset.masks[1].tools.grading.is_none());
    let replaced = layer_settings(&full, &DevelopSettings::default(), opts, (6500.0, 0.0));
    assert_eq!(replaced.masks.iter().map(|mask| mask.id).collect::<Vec<_>>(), vec![1, 5, 9]);
    assert_eq!(replaced.masks[2], above);
    let loaded: Options = serde_json::from_value(serde_json::to_value(opts).unwrap()).unwrap();
    assert_eq!(loaded.layer_placement, None, "catalog ownership must not be accepted from serialized presets");
}
