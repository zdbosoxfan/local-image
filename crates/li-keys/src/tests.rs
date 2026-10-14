use super::*;

#[test]
fn j20_profile_json_golden_and_editing() {
    let mut profiles = ShortcutProfiles::default();
    profiles.set_override("edit.copy", Some("Cmd+F19"));
    profiles.set_override("library::tool.brush", Some(""));
    profiles.set_override("library::unknown", Some("Alt+FutureKey"));
    let golden = serde_json::json!({
        "library": { "active": "Custom (Lightroom)", "custom": { "Custom (Lightroom)": {
            "parent": "Lightroom", "overrides": { "library::tool.brush": "", "library::unknown": "Alt+FutureKey" }
        }}},
        "editor": { "active": "Custom (Photoshop)", "custom": { "Custom (Photoshop)": {
            "parent": "Photoshop", "overrides": { "edit.copy": "Cmd+F19" }
        }}}
    });
    assert_eq!(serde_json::to_value(&profiles).unwrap(), golden);
    assert_eq!(profiles.overrides(false)["edit.copy"], "Cmd+F19");
    let mut restored: ShortcutProfiles = serde_json::from_value(golden).unwrap();
    restored.validate(&profiles).unwrap();
    restored.set_override("library::unknown", None);
    assert!(!restored.overrides(true).contains_key("library::unknown"));
    restored.library.duplicate("My bindings").unwrap();
    restored.library.rename("Renamed").unwrap();
    restored.library.delete_active();
    assert_eq!(restored.library.active, "Lightroom");
    assert!(restored.overrides(true).is_empty(), "a built-in has no bindings of its own");
    restored.set_override("library::tool.brush", None);
    assert_eq!(restored.library.active, "Lightroom", "returning to a default never creates a profile");
    restored.clear_overrides();
    assert!(restored.overrides(false).is_empty());
    assert_eq!(restored.editor.active, "Custom (Photoshop)");
    let defaults: ShortcutProfiles = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults, ShortcutProfiles::default());
}

#[test]
fn j20_profile_tables_golden() {
    assert_eq!(
        keymaps::CAPTURE_ONE,
        &[("tool.brush", "B"), ("tool.erase", "E"), ("tool.linear", "G"), ("tool.radial", "T"), ("view.maskOverlay", "M"), ("view.lightsOut", "")]
    );
    for (id, key) in keymaps::CAPTURE_ONE {
        assert_eq!(keymaps::library_default("Capture One", id, true, Some("old")), (!key.is_empty()).then_some(*key));
        assert_eq!(keymaps::library_default("Capture One", id, false, Some("old")), Some("old"));
        assert_eq!(keymaps::library_default("Lightroom", id, true, Some("old")), Some("old"));
    }
    assert_eq!(keymaps::library_default("Capture One", "library::alias/tool.linear/L", true, None), Some("L"));
    for id in ["view.detail", "view.gridToggle", "alias/album.toggleTarget/B"] {
        assert_eq!(keymaps::library_default("Capture One", id, true, Some("old")), None);
    }
    for (id, key) in keymaps::AFFINITY {
        assert_eq!(keymaps::editor_default("Affinity Photo", id, false, Some("old")), Some(*key));
        assert_eq!(keymaps::editor_default("Photoshop", id, false, Some("old")), Some("old"));
    }
    assert_eq!(keymaps::AFFINITY.len(), 29);
    assert_eq!(keymaps::editor_default("Affinity Photo", "tools.temporary.hand", true, Some("Space")), Some("Space"));
    assert_eq!(keymaps::editor_default("Affinity Photo", "unsupported", false, Some("B")), None);
}

#[test]
fn j20_conflicts_cover_event_overlap_and_platform_modifiers() {
    for (a, b) in [("Cmd+=", "Cmd++"), ("Shift+[", "Shift+{"), ("Shift+;", "Shift+:"), ("Delete", "Backspace")] {
        let conflicts = conflicts([("first", a), ("second", b), ("first", a)]);
        assert_eq!(conflicts.len(), 1, "{a} {b}");
        assert_eq!(conflicts[0].1.len(), 2);
    }
    let connected = conflicts([("plain", ":"), ("shift-base", "Shift+;"), ("shift-symbol", "Shift+:")]);
    assert_eq!(connected.len(), 1);
    assert_eq!(connected[0].1.len(), 3);
    assert!(conflicts([("first", "Cmd+Z"), ("second", "Cmd+Shift+Z")]).is_empty());
    assert_eq!(global_shortcut_owner("control+z"), if Platform::CURRENT == Platform::Ctrl { Some("edit.undo") } else { None });
    let a = Chord::parse("Cmd+H").unwrap();
    let b = Chord::parse("Ctrl+Cmd+H").unwrap();
    assert!(!a.overlaps_on(&b, Platform::Mac));
    assert!(a.overlaps_on(&b, Platform::Ctrl));
    assert!(conflicts([("one", "B"), ("one", "b")]).is_empty());
}

#[cfg(feature = "egui")]
mod egui_tests {
    use super::*;
    use crate::{input, legacy_tests as old};
    use egui::{Key, KeyboardShortcut, Modifiers};

    fn mods(bits: u8) -> Modifiers {
        Modifiers { command: bits & 1 != 0, ctrl: bits & 2 != 0, alt: bits & 4 != 0, shift: bits & 8 != 0, mac_cmd: false }
    }
    #[test]
    fn j20_parser_golden_against_all_three_copies_and_engine() {
        let mut compared = [0; 4];
        for key in Key::ALL {
            for bits in 0..16 {
                let sc = KeyboardShortcut::new(mods(bits), *key);
                let mut chord = String::new();
                for (bit, name) in [(1, "Cmd"), (2, "Ctrl"), (4, "Alt"), (8, "Shift")] {
                    if bits & bit != 0 {
                        chord.push_str(name);
                        chord.push('+');
                    }
                }
                chord.push_str(key.name());
                assert_eq!(normalize_shortcut(&chord), old::engine::normalize(&chord), "{chord}");
                compared[0] += 1;
                assert_eq!(input::parse(&chord), Some(sc), "{chord}");
                assert_eq!(input::parse(&chord), old::editor::parse(&chord));
                compared[1] += 1;
                if let Some((m, k)) = old::library::parse(&chord) {
                    assert_eq!(input::identity(m, k), input::identity(sc.modifiers, sc.logical_key));
                    compared[2] += 1;
                }
                let native = old::native::Chord::parse(&chord).unwrap();
                let shared = Chord::parse(&chord).unwrap();
                // Native formerly kept arbitrary spellings; portable canonical inputs must match.
                if native.key_event(true).is_some() {
                    assert_eq!(shared.key_event_on(true, Platform::Mac), native.key_event(true));
                    compared[3] += 1;
                }
            }
        }
        for (alias, expected) in [
            ("command+option+shift+keyz", "Cmd+Alt+Shift+Z"),
            ("⌃+⌘+numpadenter", "Cmd+Ctrl+Enter"),
            ("super+control+ArrowLeft", "Cmd+Ctrl+Left"),
            ("Cmd++", "Cmd++"),
            ("+", "+"),
            ("Cmd+Quote", "Cmd+'"),
            ("Shift+~", "Shift+Backtick"),
        ] {
            assert_eq!(normalize_shortcut(alias).as_deref(), Some(expected));
        }
        for bad in ["", "Cmd", "Alt", "Shift+Cmd", "Cmd+Unknown", "Cmd+A+B", "A++", "F36"] {
            assert!(Chord::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(Chord::parse("Ctrl+Cmd+M").unwrap().portable(), "Ctrl+Cmd+M");
        assert_eq!(Chord::parse("Cmd+`").unwrap().portable(), "Cmd+`");
        assert_eq!(Chord::parse("Cmd+Backtick").unwrap().key, "`");
        assert_eq!(normalize_shortcut("Cmd+`").as_deref(), Some("Cmd+Backtick"));
        assert!(Chord::parse("F24").unwrap().native_ok());
        assert!(!Chord::parse("F25").unwrap().native_ok());
        eprintln!("J20 parser parity counts engine/editor/library/native: {compared:?}; keys={}", Key::ALL.len());
    }

    #[test]
    fn j20_matching_golden_and_exact_mac_control() {
        let mut old_positive = 0;
        let mut new_positive = 0;
        for key in Key::ALL {
            for bits in 0..16 {
                let sc = KeyboardShortcut::new(mods(bits), *key);
                for actual_key in Key::ALL {
                    for actual_bits in 0..16 {
                        let actual = mods(actual_bits);
                        let legacy_library = old::library::key_matches(sc.modifiers, *key, actual, *actual_key);
                        let legacy_editor = old::editor::key_matches(&sc, *actual_key, actual);
                        let shared = input::key_matches(&sc, *actual_key, actual);
                        if legacy_library || legacy_editor {
                            old_positive += 1;
                        }
                        if shared {
                            new_positive += 1;
                        }
                        // Library's exact matches always survive; Editor's shifted layout matches
                        // survive after rejecting extra Ctrl/Command on Mac and folding them off Mac.
                        assert!(!legacy_library || shared, "lost Library {sc:?} {actual_key:?} {actual:?}");
                        let same_mods = input::identity(sc.modifiers, *key).0 == input::identity(actual, *key).0;
                        if same_mods {
                            assert_eq!(
                                old::editor::key_matches(
                                    &KeyboardShortcut::new(input::identity(sc.modifiers, *key).0, *key),
                                    *actual_key,
                                    input::identity(actual, *key).0
                                ),
                                shared,
                                "{sc:?} {actual_key:?} {actual:?}"
                            );
                        }
                    }
                }
            }
        }
        for platform in [Platform::Mac, Platform::Ctrl] {
            let sc = input::parse("Cmd+H").unwrap();
            let ctrl_cmd = Modifiers { command: true, ctrl: true, mac_cmd: platform == Platform::Mac, ..Modifiers::NONE };
            assert_eq!(input::key_matches_on(&sc, Key::H, ctrl_cmd, platform), platform == Platform::Ctrl);
            let sc = input::parse("Ctrl+Cmd+H").unwrap();
            assert!(!input::key_matches_on(&sc, Key::H, Modifiers::COMMAND, Platform::Mac));
            assert!(input::key_matches_on(&sc, Key::H, ctrl_cmd, platform));
        }
        eprintln!("J20 event matrix: old union positives={old_positive}, shared positives={new_positive}");
    }

    #[test]
    fn j20_core_and_egui_overlap_parity() {
        let keys = ["=", "+", ";", ":", "[", "{", "]", "}", "/", "?", "\\", "|", "1", "!", "Delete", "Backspace", "'", ",", "B", "F35"];
        let mut chords = Vec::new();
        for key in keys {
            for prefix in ["", "Shift+", "Cmd+", "Ctrl+", "Ctrl+Cmd+", "Cmd+Shift+", "Alt+"] {
                chords.push(Chord::parse(&format!("{prefix}{key}")).unwrap());
            }
        }
        for a in &chords {
            for b in &chords {
                for platform in [Platform::Mac, Platform::Ctrl] {
                    assert_eq!(a.overlaps_on(b, platform), input::overlaps_on(&a.shortcut().unwrap(), &b.shortcut().unwrap(), platform), "{a:?} {b:?}");
                }
            }
        }
        let mut bindings = vec![("custom", "Cmd++"), ("default", "Cmd+="), ("other", "Cmd+Shift+Z")];
        input::retain_first(&mut bindings, |b| b.1);
        assert_eq!(bindings, [("custom", "Cmd++"), ("other", "Cmd+Shift+Z")]);
    }
}

#[cfg(feature = "egui")]
#[test]
fn j20_dispatch_preserves_order_unmatched_keys_and_releases() {
    use egui::{Key, Modifiers};
    let ctx = egui::Context::default();
    let key = |key, pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
    let mut output =
        ctx.run_ui(egui::RawInput { events: vec![key(Key::C, true), key(Key::B, true), key(Key::A, true), key(Key::B, false)], ..Default::default() }, |ui| {
            let bindings = [("a", input::parse("A").unwrap()), ("b", input::parse("B").unwrap())];
            assert_eq!(input::take_pressed(ui.ctx(), &bindings), Some(("b", Key::B)));
            assert_eq!(input::take_pressed(ui.ctx(), &bindings), Some(("a", Key::A)));
            assert_eq!(input::take_pressed(ui.ctx(), &bindings), None);
            assert_eq!(ui.input(|i| i.events.clone()), vec![key(Key::C, true), key(Key::B, false)]);
        });
    output.textures_delta.clear();
}

#[cfg(feature = "egui")]
#[test]
fn j20_capture_platform_mapping_preserves_mac_control_command() {
    use egui::{Key, Modifiers};
    let physical = Modifiers { ctrl: true, command: true, mac_cmd: true, alt: true, shift: true };
    assert_eq!(input::shortcut_text_on(Key::M, physical, Platform::Mac).as_deref(), Some("Cmd+Ctrl+Alt+Shift+M"));
    assert_eq!(input::shortcut_text_on(Key::M, physical, Platform::Ctrl).as_deref(), Some("Cmd+Alt+Shift+M"));
    assert_eq!(input::shortcut_text_on(Key::Delete, Modifiers::NONE, Platform::Mac).as_deref(), Some("Delete"));
    assert!(input::shortcut_text_on(Key::ControlLeft, physical, Platform::Mac).is_none());
    assert!(input::is_function_key(Key::F35));
    assert!(!input::is_function_key(Key::F));
}
