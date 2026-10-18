use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 32, "height": 32})).unwrap();
    s
}

#[test]
fn bugfix_save_o046_bad_enum_keeps_valid_nested_preferences() {
    let mut s = session();
    let r = s.load_prefs_json(r##"{"interface":{"theme":"retired-theme","uiFontSize":"large"},"general":{"beepWhenDone":true},"colorSettings":{"workingRgb":"removed-profile","blendTextGamma":1.25}}"##);
    assert!(r.is_err(), "report the repaired field");
    assert!(s.prefs().general.beep_when_done);
    assert_eq!(s.prefs().interface.theme, Preferences::default().interface.theme);
    assert_eq!(s.prefs().interface.ui_font_size, UiFontSize::Large);
    assert_eq!(s.color.settings.blend_text_gamma, 1.25);
    let value = s.prefs_to_json();
    let mut reloaded = session();
    reloaded.load_prefs_json(&value).unwrap();
    assert!(reloaded.prefs().general.beep_when_done);
}

#[test]
fn defaults_match_photoshop() {
    let p = Preferences::default();
    assert_eq!(p.performance.history_states, 50);
    assert_eq!(p.guides_grid_and_slices.subdivisions, 4);
    assert_eq!(p.guides_grid_and_slices.guide_color, "#4affff");
    assert_eq!(p.transparency_and_gamut.colors(), [[255, 255, 255], [204, 204, 204]]);
    assert_eq!(p.transparency_and_gamut.square(), Some(8.0));
    assert_eq!(p.cursors.painting, PaintingCursor::NormalTip);
    // Every dialog section is a key of the JSON form.
    let v = p.to_json();
    for (id, _) in SECTIONS {
        assert!(v.get(id).is_some_and(Value::is_object), "{id}");
    }
}

#[test]
fn get_set_reset_by_path() {
    let mut s = session();
    assert_eq!(s.execute("prefs.get", json!({"path": "performance.historyStates"})).unwrap(), json!(50));
    s.execute("prefs.set", json!({"path": "performance.historyStates", "value": 3})).unwrap();
    assert_eq!(s.prefs().performance.history_states, 3);
    // Applied to open documents: only three undo steps remain.
    for i in 0..6 {
        s.execute("layer.new.layer", json!({"name": format!("L{i}")})).unwrap();
    }
    let mut n = 0;
    while s.undo() {
        n += 1;
    }
    assert_eq!(n, 3);
    // New documents too.
    s.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
    assert_eq!(s.active().unwrap().history.max_states, 3);
    s.execute("prefs.reset", json!({"path": "performance"})).unwrap();
    assert_eq!(s.prefs().performance.history_states, 50);
    assert_eq!(s.active().unwrap().history.max_states, 50);
}

#[test]
fn reset_one_colour_setting() {
    let mut s = session();
    s.execute("prefs.set", json!({"values": {"colorSettings.bpc": false, "colorSettings.workingRgb": "display-p3"}})).unwrap();
    assert_eq!(s.execute("prefs.reset", json!({"path": "colorSettings.bpc"})).unwrap(), json!(true));
    assert!(s.color.settings.bpc);
    assert_eq!(s.color.settings.working_rgb, "display-p3", "other colour settings are untouched");
    s.execute("prefs.reset", json!({"path": "colorSettings.workingRgb"})).unwrap();
    assert_eq!(s.color.settings, crate::color_cmds::ColorSettings::default());
    assert!(s.execute("prefs.reset", json!({"path": "colorSettings.notAField"})).is_err());
}

#[test]
fn set_validates_and_is_all_or_nothing() {
    let mut s = session();
    for (path, value) in [
        ("cursors.painting", json!("huge")),
        ("performance.historyStates", json!(0)),
        ("performance.historyStates", json!("ten")),
        ("guidesGridAndSlices.guideColor", json!("cyan")),
        ("nope.nothing", json!(1)),
        ("general.notAField", json!(true)),
        ("shortcuts.edit.undo", json!("Cmd+Banana+Q")),
    ] {
        assert!(s.execute("prefs.set", json!({"path": path, "value": value})).is_err(), "{path}");
    }
    let r = s.execute("prefs.set", json!({"values": {"cursors.painting": "precise", "performance.historyStates": -4}}));
    assert!(r.is_err());
    assert_eq!(s.prefs().cursors.painting, PaintingCursor::NormalTip, "nothing applied");
    let r = s.execute("prefs.set", json!({"values": {"cursors.painting": "precise", "unitsAndRulers.rulers": "cm"}})).unwrap();
    assert_eq!(r["cursors.painting"], "precise");
    assert_eq!(s.prefs().units_and_rulers.rulers, Unit::Centimeters);
}

#[test]
fn whole_section_set_merges_keys() {
    let mut s = session();
    s.execute("prefs.set", json!({"path": "transparencyAndGamut", "value": {"gridSize": "large", "gridColors": "custom", "customDark": "#336699"}})).unwrap();
    let t = &s.prefs().transparency_and_gamut;
    assert_eq!(t.square(), Some(16.0));
    assert_eq!(t.colors()[1], [0x33, 0x66, 0x99]);
    assert_eq!(t.gamut_warning_color, "#808080", "untouched keys keep their values");
}

#[test]
fn json_round_trip_tolerates_unknown_and_missing_keys() {
    let mut s = session();
    s.execute("prefs.set", json!({"values": {"interface.theme": "studioLight", "fileHandling.autosaveMinutes": 5, "shortcuts.edit.fill": "Cmd+Shift+F"}}))
        .unwrap();
    s.execute("edit.colorSettings", json!({"workingRgb": "display-p3", "policyRgb": "convert"})).unwrap();
    let text = s.prefs_to_json();
    let mut t = Session::new();
    t.load_prefs_json(&text).unwrap();
    assert_eq!(t.prefs(), s.prefs());
    assert_eq!(t.color.settings.working_rgb, "display-p3");
    assert_eq!(t.color.settings.policy_rgb, crate::color_cmds::Policy::Convert);
    // Older/newer files: unknown keys ignored, missing ones default.
    let mut u = Session::new();
    u.load_prefs_json(r#"{"general": {"beepWhenDone": true, "futureThing": 1}, "someNewSection": {}}"#).unwrap();
    assert!(u.prefs().general.beep_when_done);
    assert_eq!(u.prefs().performance.history_states, 50);
    assert!(u.load_prefs_json("not json").is_err());
}

#[test]
fn units_convert_both_ways() {
    for u in [Unit::Pixels, Unit::Inches, Unit::Centimeters, Unit::Millimeters, Unit::Points, Unit::Picas, Unit::Percent] {
        let v = u.from_px(450.0, 300.0, 900.0, 72.0);
        assert!((u.to_px(v, 300.0, 900.0, 72.0) - 450.0).abs() < 1e-9, "{u:?}");
    }
    assert_eq!(Unit::Inches.from_px(600.0, 300.0, 0.0, 72.0), 2.0);
    assert_eq!(Unit::Points.from_px(300.0, 300.0, 0.0, 72.0), 72.0);
    assert_eq!(Unit::Percent.from_px(450.0, 300.0, 900.0, 72.0), 50.0);
    let r = UnitsAndRulers { rulers: Unit::Centimeters, ..Default::default() };
    assert_eq!(r.format(300.0, 300.0, 0.0), "2.54");
}

#[test]
fn shortcuts_normalise_and_detect_conflicts() {
    assert_eq!(normalize_shortcut("shift+cmd+n").as_deref(), Some("Cmd+Shift+N"));
    assert_eq!(normalize_shortcut("Alt+Cmd+f7").as_deref(), Some("Cmd+Alt+F7"));
    assert_eq!(normalize_shortcut("Cmd++").as_deref(), Some("Cmd++"));
    assert_eq!(normalize_shortcut("Cmd+Shift"), None);
    assert_eq!(normalize_shortcut("Cmd+A+B"), None);
    let c = conflicts([("a", "Cmd+J"), ("b", "cmd+j"), ("c", "Cmd+K")]);
    assert_eq!(c, vec![("Cmd+J".to_string(), vec!["a".to_string(), "b".to_string()])]);
}

#[test]
fn keyboard_shortcuts_reassign_and_reset() {
    let mut s = session();
    let undo = crate::commands::find("edit.undo").unwrap().shortcut;
    assert_eq!(s.prefs().shortcut("edit.undo", undo), Some("Cmd+Z"));
    // Global Undo cannot be stolen by a workspace profile. Retain and report the
    // user's conflicting assignment, but ignore it in the effective key set.
    let r = s.execute("edit.keyboardShortcuts", json!({"set": {"edit.fill": "Cmd+Z"}})).unwrap();
    assert_eq!(s.prefs().shortcut_overrides(false)["edit.fill"], "Cmd+Z");
    assert_eq!(s.prefs().shortcut("edit.fill", None), None);
    assert_eq!(s.prefs().shortcut("edit.undo", undo), Some("Cmd+Z"));
    assert!(!s.prefs().shortcut_overrides(false).contains_key("edit.undo"), "Global conflict must not disable Undo in the profile");
    assert_eq!(r["conflicts"][0]["global"], true);
    assert_eq!(r["conflicts"][0]["ignored"], true);
    // Reviewing the assignment still reports its Global conflict.
    let r = s.execute("edit.keyboardShortcuts", json!({"set": {"edit.undo": "Cmd+Z"}, "removeConflicts": false})).unwrap();
    assert_eq!(r["conflicts"][0]["shortcut"], "Cmd+Z");
    assert!(s.execute("edit.keyboardShortcuts", json!({"set": {"no.such.command": "F9"}})).is_err());
    s.execute("edit.keyboardShortcuts", json!({"reset": true})).unwrap();
    assert!(s.prefs().shortcut_overrides(false).is_empty());
    assert_eq!(s.prefs().shortcut("edit.undo", undo), Some("Cmd+Z"));
    // Listing with a filter.
    let r = s.execute("edit.keyboardShortcuts", json!({"filter": "gaussian"})).unwrap();
    assert!(r["commands"].as_array().unwrap().iter().any(|c| c["id"] == "filter.blur.gaussianBlur"));
}

/// #249: temporary tools (held keys) and the fill / colour keys are bindable like commands.
#[test]
fn temporary_tools_and_fill_keys_are_rebindable() {
    let mut s = session();
    let r = s.execute("edit.keyboardShortcuts", json!({"filter": "tools.temporary"})).unwrap();
    let ids: Vec<&str> = r["commands"].as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
    assert!(ids.contains(&"tools.temporary.hand") && ids.contains(&"tools.temporary.zoomIn") && ids.contains(&"tools.temporary.zoomOut"), "{ids:?}");
    assert!(r["commands"].as_array().unwrap().iter().all(|c| c["hold"] == true));
    // Rebind the temporary zoom; an unknown id still fails.
    s.execute("edit.keyboardShortcuts", json!({"set": {"tools.temporary.zoomIn": "Ctrl+Space"}})).unwrap();
    assert_eq!(s.prefs().shortcut("tools.temporary.zoomIn", Some("Cmd+Space")), Some("Ctrl+Space"));
    assert!(s.execute("edit.keyboardShortcuts", json!({"set": {"tools.temporary.unknown": "Space"}})).is_err());
    // Taking a temporary tool's key for a command removes it from the temporary tool.
    s.execute("edit.keyboardShortcuts", json!({"set": {"edit.fillForeground": "Space"}})).unwrap();
    assert_eq!(s.prefs().shortcut("tools.temporary.hand", Some("Space")), None);
    assert_eq!(s.prefs().shortcut("edit.fillForeground", Some("Alt+Backspace")), Some("Space"));
    // D / X and the fill keys are listed with their Photoshop defaults.
    let r = s.execute("edit.keyboardShortcuts", json!({"list": true})).unwrap();
    let def = |id: &str| r["commands"].as_array().unwrap().iter().find(|c| c["id"] == id).map(|c| c["default"].clone());
    assert_eq!(def("tools.swapColors"), Some(json!("X")));
    assert_eq!(def("tools.defaultColors"), Some(json!("D")));
    assert_eq!(def("edit.fillBackground"), Some(json!("Cmd+Backspace")));
    // Bad values fail.
    assert!(s.execute("edit.keyboardShortcuts", json!({"set": {"tools.temporary.hand": 7}})).is_err());
}

#[test]
fn menus_and_toolbar_customisation_persist() {
    let mut s = session();
    s.execute("edit.menus", json!({"hide": ["edit.fade"], "color": {"edit.fill": "red"}})).unwrap();
    assert_eq!(s.prefs().menus.hidden, vec!["edit.fade".to_string()]);
    assert_eq!(s.prefs().menus.colors["edit.fill"], "red");
    assert!(s.execute("edit.menus", json!({"color": {"edit.fill": "plaid"}})).is_err());
    s.execute("edit.menus", json!({"show": ["edit.fade"], "color": {"edit.fill": "none"}})).unwrap();
    assert!(s.prefs().menus.hidden.is_empty() && s.prefs().menus.colors.is_empty());
    s.execute("edit.toolbar", json!({"hidden": ["Sponge"]})).unwrap();
    let back: Preferences = serde_json::from_str(&s.prefs_to_json()).unwrap();
    // Hiding a tool edits a custom copy of the active (built-in) set.
    let set = back.toolbar.active();
    assert_eq!(set.name, "My Tools");
    assert!(!set.tools.iter().any(|t| t == "Sponge") && set.tools.iter().any(|t| t == "Dodge"));
}

#[test]
fn menus_refuse_to_hide_essential_items() {
    let mut s = session();
    for id in ["edit.menus", "edit.keyboardShortcuts", "edit.preferences.general", "file.save", "file.saveAs", "file.exit"] {
        assert!(s.execute("edit.menus", json!({"hide": ["edit.fade", id]})).is_err(), "{id}");
        assert!(s.prefs().menus.hidden.is_empty(), "{id}: a refused call changes nothing");
    }
    // A preferences file that lists them anyway still shows them.
    let menus = MenuCustomization { hidden: vec!["file.exit".into(), "edit.fade".into()], ..Default::default() };
    assert!(!menus.is_hidden("file.exit") && menus.is_hidden("edit.fade"));
}

#[test]
fn preference_sections_are_commands() {
    let mut s = Session::new();
    for (id, title) in SECTIONS {
        let result = s.execute(&format!("edit.preferences.{id}"), json!({}));
        if id == "integrations" {
            // The hidden storage section remains, but its obsolete UI alias was retired.
            assert!(matches!(result, Err(crate::EngineError::UnknownCommand(_))));
            continue;
        }
        let r = result.unwrap();
        assert_eq!(r["section"], id);
        assert_eq!(r["title"], title);
        assert!(r["values"].is_object(), "{id}");
    }
}

#[test]
fn choice_and_range_tables_cover_enum_fields() {
    let p = Preferences::default().to_json();
    for (id, _) in SECTIONS {
        for (k, v) in p[id].as_object().unwrap() {
            let path = format!("{id}.{k}");
            if let Some(c) = choices(&path) {
                assert!(c.contains(&v.as_str().unwrap()), "{path}");
            }
            if let Some((lo, hi)) = range(&path) {
                let x = v.as_f64().unwrap();
                assert!(x >= lo && x <= hi, "{path} default {x} outside {lo}..{hi}");
            }
            if is_color(&path) {
                assert!(parse_hex(v.as_str().unwrap()).is_some(), "{path}");
            }
        }
    }
}

#[test]
fn right_click_with_painting_tools_pref() {
    let mut s = session();
    assert_eq!(s.prefs().tools.right_click_with_painting_tools, RightClickPaint::BrushPicker, "Photoshop: the Brush Preset picker");
    assert_eq!(s.execute("prefs.get", json!({"path": "tools.rightClickWithPaintingTools"})).unwrap(), json!("brushPicker"));
    assert_eq!(choices("tools.rightClickWithPaintingTools"), Some(RightClickPaint::NAMES));
    s.execute("prefs.set", json!({"path": "tools.rightClickWithPaintingTools", "value": "erase"})).unwrap();
    assert_eq!(s.prefs().tools.right_click_with_painting_tools, RightClickPaint::Erase);
    for bad in [json!("smudge"), json!(1), json!(null), json!(["erase"])] {
        assert!(s.execute("prefs.set", json!({"path": "tools.rightClickWithPaintingTools", "value": bad})).is_err());
    }
    assert_eq!(s.prefs().tools.right_click_with_painting_tools, RightClickPaint::Erase, "rejected values change nothing");
    // Preferences saved before the option existed load with the default.
    let old: Tools = serde_json::from_value(json!({"showTooltips": false})).unwrap();
    assert_eq!(old.right_click_with_painting_tools, RightClickPaint::BrushPicker);
}

#[test]
fn gpu_backend_round_trips_and_validates() {
    let mut s = session();
    assert_eq!(s.prefs().performance.gpu_backend, GpuBackend::Auto);
    assert_eq!(s.execute("prefs.get", json!({"path": "performance.gpuBackend"})).unwrap(), json!("auto"));
    s.execute("prefs.set", json!({"path": "performance.gpuBackend", "value": "dx12"})).unwrap();
    assert_eq!(s.prefs().performance.gpu_backend, GpuBackend::Dx12);
    // Unknown names and wrong types are errors, and leave the value alone.
    assert!(s.execute("prefs.set", json!({"path": "performance.gpuBackend", "value": "directx"})).is_err());
    assert!(s.execute("prefs.set", json!({"path": "performance.gpuBackend", "value": 3})).is_err());
    assert_eq!(s.prefs().performance.gpu_backend, GpuBackend::Dx12);
    // Persisted and restored with the rest of the preferences.
    let text = s.prefs_to_json();
    let mut t = Session::new();
    t.load_prefs_json(&text).unwrap();
    assert_eq!(t.prefs().performance.gpu_backend, GpuBackend::Dx12);
    // A file without the setting loads as `auto`.
    let mut u = Session::new();
    u.load_prefs_json(r#"{"performance": {}}"#).unwrap();
    assert_eq!(u.prefs().performance.gpu_backend, GpuBackend::Auto);
    assert_eq!(choices("performance.gpuBackend"), Some(GpuBackend::NAMES));
    for n in GpuBackend::NAMES {
        assert_eq!(GpuBackend::parse(n).map(GpuBackend::name), Some(*n));
    }
}

#[test]
fn pen_pressure_curve_pref() {
    let mut s = session();
    assert_eq!(s.prefs().tools.pen_pressure_curve, vec![[0.0, 0.0], [1.0, 1.0]], "linear: pressure unchanged");
    s.execute("prefs.set", json!({"path": "tools.penPressureCurve", "value": [[0.0, 0.0], [0.3, 0.55], [1.0, 1.0]]})).unwrap();
    assert_eq!(photocraft_paint::pressure::preset_of(&s.prefs().tools.pen_pressure_curve), Some("soft"));
    assert!(s.execute("prefs.set", json!({"path": "tools.penPressureCurve", "value": "firm"})).is_err());
    // Preferences saved before the option existed load with the linear curve.
    let old: Tools = serde_json::from_value(json!({"showTooltips": false})).unwrap();
    assert_eq!(old.pen_pressure_curve, photocraft_paint::pressure::LINEAR.to_vec());
}

#[test]
fn tool_icon_preferences_default_validate_and_round_trip() {
    let old: Preferences = serde_json::from_value(json!({"interface": {"theme": "studio"}})).unwrap();
    assert_eq!(old.interface.tool_icons, "monochrome", "fresh default is Monochrome");
    let mut s = session();
    s.execute("prefs.set", json!({"path": "interface.toolIcons", "value": "colour"})).unwrap();
    assert!(s.execute("prefs.set", json!({"path": "interface.toolIcons", "value": "invalid"})).is_err());
    let mut restored = Session::new();
    restored.load_prefs_json(&s.prefs_to_json()).unwrap();
    assert_eq!(restored.prefs().interface.tool_icons, "colour");
    restored.execute("prefs.reset", json!({"path": "interface.toolIcons"})).unwrap();
    assert_eq!(restored.prefs().interface.tool_icons, "monochrome", "reset returns to the default");
}

#[test]
fn ds_finding_toolbar_batch_keeps_hidden_and_order() {
    let mut s = Session::new();
    s.execute("edit.toolbar", json!({"hidden":["Move"],"order":["Brush","Eraser"]})).unwrap();
    let tools = s.prefs().toolbar.active().tools;
    assert_eq!(&tools[..2], &["Brush", "Eraser"]);
    assert!(!tools.iter().any(|t| t == "Move"));
}

#[test]
fn ds_finding_repeated_toolbar_edits_and_unique_bindings() {
    let mut s = Session::new();
    s.execute("edit.toolbar", json!({"order":["Brush","Eraser"],"hidden":["Move"]})).unwrap();
    s.execute("edit.toolbar", json!({"hidden":["Brush"]})).unwrap();
    assert!(!s.prefs().toolbar.active().tools.iter().any(|t| t == "Brush"));
    assert_eq!(s.prefs().toolbar.active().tools[0], "Eraser");
    let bindings: Vec<_> = super::bindable().collect();
    assert_eq!(bindings.len(), bindings.iter().map(|b| b.0).collect::<std::collections::BTreeSet<_>>().len());
    assert_eq!(bindings.iter().find(|b| b.0 == "tools.select.Move").unwrap().1, Some("V"));
    assert!(super::conflicts([("same", "V"), ("same", "V")]).is_empty());
}

#[test]
fn review_regression_vector_toolbar_defaults_are_addressable_by_preferences_api() {
    let mut s = session();
    for (key, value) in [
        ("toolbar.persona", json!("vector")),
        ("toolbar.vectorSet", json!("vector.illustration")),
        ("toolbar.pixelTool", json!("Pen")),
        ("toolbar.vectorTool", json!("Rectangle")),
    ] {
        assert!(!s.execute("prefs.get", json!({"path":key})).unwrap().is_null(), "{key}");
        s.execute("prefs.set", json!({"path":key,"value":value})).unwrap();
        assert_eq!(s.execute("prefs.get", json!({"path":key})).unwrap(), value);
    }
}
#[test]
fn review_regression_edit_toolbar_uses_and_resets_current_persona_set() {
    let mut s = session();
    let pixel = s.prefs().toolbar.active_set.clone();
    s.edit_prefs(|p| p.toolbar.persona = crate::toolsets::Persona::Vector);
    s.execute("edit.toolbar", json!({"hidden":["Rectangle"],"order":["Pen","EllipseShape"]})).unwrap();
    assert_eq!(s.prefs().toolbar.active_set, pixel);
    assert_eq!(s.prefs().toolbar.active().persona, crate::toolsets::Persona::Vector);
    assert_eq!(s.prefs().toolbar.active().tools[0], "Pen");
    assert!(!s.prefs().toolbar.active().tools.iter().any(|id| id == "Rectangle"));
    assert!(s.prefs().toolbar.active().tools.iter().all(|id| crate::toolsets::supports(id, crate::toolsets::Persona::Vector)));
    s.execute("edit.toolbar", json!({"reset":true})).unwrap();
    assert_eq!(s.prefs().toolbar.vector_set, crate::toolsets::Persona::Vector.default_set());
    assert_eq!(s.prefs().toolbar.active_set, pixel);
}

#[test]
fn dsfix_v029_default_persona_preferences_are_addressable() {
    let mut s = session();
    for (path, value) in
        [("toolbar.persona", json!("pixel")), ("toolbar.vectorSet", json!("")), ("toolbar.pixelTool", json!("")), ("toolbar.vectorTool", json!(""))]
    {
        assert_eq!(s.execute("prefs.get", json!({"path":path})).unwrap(), value);
        s.execute("prefs.set", json!({"path":path,"value":value})).unwrap();
    }
    s.execute("prefs.set", json!({"path":"toolbar.persona","value":"vector"})).unwrap();
    assert_eq!(s.prefs().toolbar.persona, crate::toolsets::Persona::Vector);
}

#[test]
fn dsfix_v030_v031_toolbar_edit_and_reset_follow_vector_persona() {
    use crate::toolsets::Persona;
    let mut s = session();
    s.edit_prefs(|pr| {
        pr.toolbar.active_set = "photographer".into();
        pr.toolbar.persona = Persona::Vector;
        pr.toolbar.vector_set = "vector.illustration".into();
    });
    s.execute("edit.toolbar", json!({"hidden":["Pen"],"order":["DirectSelection","VectorPencil"]})).unwrap();
    let toolbar = &s.prefs().toolbar;
    assert_eq!(toolbar.active_set, "photographer");
    let custom = toolbar.active();
    assert_eq!(custom.persona, Persona::Vector);
    assert_eq!(custom.id, toolbar.vector_set);
    assert!(!custom.tools.iter().any(|t| t == "Pen"));
    assert_eq!(&custom.tools[..2], &["DirectSelection", "VectorPencil"]);
    assert!(custom.tools.iter().all(|t| crate::toolsets::supports(t, Persona::Vector)));
    s.execute("edit.toolbar", json!({"reset":true})).unwrap();
    assert_eq!(s.prefs().toolbar.vector_set, "vector.allTools");
    assert_eq!(s.prefs().toolbar.active_set, "photographer");
}

#[test]
fn named_shortcut_keys_are_canonical_and_unknown_keys_are_refused() {
    for (input, expected) in [
        ("ctrl+pageup", "Ctrl+PageUp"),
        ("Ctrl+Pagedown", "Ctrl+PageDown"),
        ("Shift+Opencurlybracket", "Shift+OpenCurlyBracket"),
        ("Shift+CloseCurlyBracket", "Shift+CloseCurlyBracket"),
        ("Cmd+ArrowUp", "Cmd+Up"),
        ("Alt+NumpadEnter", "Alt+Enter"),
        ("Ctrl+KeyQ", "Ctrl+Q"),
        ("f35", "F35"),
    ] {
        assert_eq!(normalize_shortcut(input).as_deref(), Some(expected));
    }
    for input in ["Cmd+Banana", "Ctrl+F36", "Ctrl+Numpad42", "Ctrl+A+B"] {
        assert!(normalize_shortcut(input).is_none(), "{input}");
    }
    let mut s = session();
    assert!(s.execute("prefs.set", json!({"path":"shortcuts.edit.search","value":"Cmd+Banana"})).is_err());
}

#[test]
fn keyed_preference_reset_removes_only_the_requested_override() {
    let mut s = session();
    s.execute("prefs.set", json!({"values":{"shortcuts.edit.undo":"Alt+PageUp","shortcuts.edit.redo":"Alt+PageDown","menus.colors.edit.undo":"red","menus.colors.edit.redo":"blue"}})).unwrap();
    for path in ["shortcuts.edit.undo", "menus.colors.edit.undo"] {
        assert_eq!(s.execute("prefs.reset", json!({"path":path})).unwrap(), Value::Null);
        assert_eq!(s.execute("prefs.reset", json!({"path":path})).unwrap(), Value::Null);
    }
    assert!(!s.prefs().shortcut_overrides(false).contains_key("edit.undo"));
    assert_eq!(s.prefs().shortcut("edit.undo", Some("Cmd+Z")), Some("Cmd+Z"));
    assert_eq!(s.prefs().shortcut_overrides(false)["edit.redo"], "Alt+PageDown");
    assert!(!s.prefs().menus.colors.contains_key("edit.undo"));
    assert_eq!(s.prefs().menus.colors["edit.redo"], "blue");
    assert!(s.execute("prefs.reset", json!({"path":"no.such.preference"})).is_err());
    assert!(s.execute("prefs.reset", json!({"path":"shortcuts."})).is_err());
}

#[test]
fn shortcut_reset_assignment_and_profile_validation_are_atomic() {
    let mut s = session();
    s.load_prefs_json(
        r#"{"shortcutProfiles":{"editor":{"active":"Mine","custom":{"Mine":{"parent":"Photoshop","overrides":{"edit.search":"Alt+F35","edit.undo":"","future.command":"Alt+FutureKey"}}}}}}"#,
    )
    .unwrap();
    let before = s.prefs().shortcut_overrides(false).clone();
    assert_eq!(before.len(), 3);
    assert!(s.execute("edit.keyboardShortcuts", json!({"reset":true,"set":"not an object"})).is_err());
    assert_eq!(s.prefs().shortcut_overrides(false), &before);
    assert!(
        s.execute("edit.keyboardShortcuts", json!({"reset":true,"allowUnknown":true,"set":{"edit.search":"Alt+PageUp","edit.redo":"Cmd+Banana"}})).is_err()
    );
    assert_eq!(s.prefs().shortcut_overrides(false), &before);
    let path = "shortcutProfiles.editor.custom.Mine.overrides";
    for values in [json!({"edit.search":"Cmd+Banana"}), json!({"edit.search":"Alt+PageUp","edit.redo":"Cmd+Banana"})] {
        assert!(s.execute("prefs.set", json!({"path":path,"value":values})).is_err());
        assert_eq!(s.prefs().shortcut_overrides(false), &before);
    }
    let mut edited = before.clone();
    edited.insert("edit.search".into(), "Alt+Pageup".into());
    s.execute("prefs.set", json!({"path":path,"value":edited})).unwrap();
    assert_eq!(s.prefs().shortcut_overrides(false)["edit.search"], "Alt+PageUp");
    assert_eq!(s.prefs().shortcut_overrides(false)["future.command"], "Alt+FutureKey", "a newer build's key name survives unchanged");
    assert_eq!(s.prefs().shortcut_overrides(false)["edit.undo"], "");
}

#[test]
fn tool_switch_defaults_use_shift_but_explicit_saved_false_is_preserved() {
    let mut s = session();
    assert!(s.prefs().tools.use_shift_key_for_tool_switch);
    s.load_prefs_json(r#"{"tools":{"useShiftKeyForToolSwitch":false}}"#).unwrap();
    assert!(!s.prefs().tools.use_shift_key_for_tool_switch);
    let saved = s.prefs_to_json();
    let mut reopened = session();
    reopened.load_prefs_json(&saved).unwrap();
    assert!(!reopened.prefs().tools.use_shift_key_for_tool_switch);
}

/// Photoshop's defaults: R is Rotate View; AI Remove has no key of its own (it sits in the
/// healing / remove group on J, so it must never take R back). No two tool groups may share a
/// default key, or one of them is unreachable.
#[test]
fn tool_group_default_keys_are_unique_and_r_is_rotate_view() {
    let mut seen = std::collections::BTreeMap::new();
    for (id, _, key) in TOOL_SHORTCUTS {
        if let Some(key) = key {
            assert!(seen.insert(normalize_shortcut(key).unwrap(), *id).is_none(), "{id} shares {key} with {:?}", seen.get(*key));
        }
    }
    assert_eq!(seen.get("R"), Some(&"tools.select.RotateView"));
    assert_eq!(seen.get("J"), Some(&"tools.select.SpotHealing"));
    assert_eq!(TOOL_SHORTCUTS.iter().find(|t| t.0 == "tools.select.AiRemove").unwrap().2, None);
    let mut s = session();
    let r = s.execute("edit.keyboardShortcuts", json!({})).unwrap();
    let tool_conflicts: Vec<_> = r["conflicts"].as_array().unwrap().iter().filter(|c| c["commands"].to_string().contains("tools.select.")).collect();
    assert!(tool_conflicts.is_empty(), "default tool keys conflict: {tool_conflicts:?}");
}

#[test]
fn dsfix_shortcut_path_keeps_profile_only_bindings() {
    for library in [false, true] {
        let mut prefs = Preferences::default();
        let id = if library { "library::view.detail" } else { "tools.select.Brush" };
        let profile_only = if library { "library::view.grid" } else { "tools.select.Move" };
        let ws = prefs.shortcut_profiles.workspace_mut(library);
        ws.duplicate("Mine").unwrap();
        ws.custom.get_mut("Mine").unwrap().overrides.insert(profile_only.into(), "F8".into());
        prefs.set(&format!("shortcuts.{id}"), json!("F7")).unwrap();
        assert_eq!(prefs.shortcut_overrides(library).get(profile_only).map(String::as_str), Some("F8"));
        assert_eq!(prefs.shortcut_overrides(library).get(id).map(String::as_str), Some("F7"));
        prefs.reset(Some(&format!("shortcuts.{id}"))).unwrap();
        assert!(!prefs.shortcut_overrides(library).contains_key(id));
        assert_eq!(prefs.shortcut_overrides(library).get(profile_only).map(String::as_str), Some("F8"));
    }
}
