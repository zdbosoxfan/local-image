use super::*;

#[test]
fn profile_json_golden_and_editing() {
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
fn profile_tables_golden() {
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
fn conflicts_cover_event_overlap_and_platform_modifiers() {
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
    use crate::input;
    use egui::{Key, KeyboardShortcut, Modifiers};

    fn mods(bits: u8) -> Modifiers {
        Modifiers { command: bits & 1 != 0, ctrl: bits & 2 != 0, alt: bits & 4 != 0, shift: bits & 8 != 0, mac_cmd: false }
    }

    /// egui key names that the canonical spelling writes as the character itself. Snapshot of the
    /// frozen pre-merge engine normalizer, which every other key name passed through unchanged.
    const SYMBOL_NAMES: &[(&str, &str)] = &[
        ("Colon", ":"),
        ("Comma", ","),
        ("Minus", "-"),
        ("Period", "."),
        ("Plus", "+"),
        ("Equals", "="),
        ("Semicolon", ";"),
        ("OpenBracket", "["),
        ("CloseBracket", "]"),
        ("Backslash", "\\"),
        ("Slash", "/"),
        ("Pipe", "|"),
        ("Questionmark", "?"),
        ("Exclamationmark", "!"),
        ("Quote", "'"),
    ];

    #[test]
    fn parser_golden_for_every_key_and_modifier_set() {
        for key in Key::ALL {
            let canonical_key = SYMBOL_NAMES.iter().find(|(name, _)| *name == key.name()).map_or(key.name(), |(_, symbol)| symbol);
            for bits in 0..16 {
                let sc = KeyboardShortcut::new(mods(bits), *key);
                let mut chord = String::new();
                for (bit, name) in [(1, "Cmd"), (2, "Ctrl"), (4, "Alt"), (8, "Shift")] {
                    if bits & bit != 0 {
                        chord.push_str(name);
                        chord.push('+');
                    }
                }
                let prefix = chord.clone();
                chord.push_str(key.name());
                assert_eq!(normalize_shortcut(&chord), Some(format!("{prefix}{canonical_key}")), "{chord}");
                assert_eq!(input::parse(&chord), Some(sc), "{chord}");
                // The event a native menu item delivers for the chord on macOS (physical ⌘ = command).
                let m = sc.modifiers;
                let mac_modifiers = Modifiers { alt: m.alt, ctrl: m.ctrl, shift: m.shift, mac_cmd: m.command, command: m.command };
                let event = egui::Event::Key { key: *key, physical_key: Some(*key), pressed: true, repeat: false, modifiers: mac_modifiers };
                assert_eq!(Chord::parse(&chord).unwrap().key_event_on(true, Platform::Mac), Some(event), "{chord}");
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
    }

    /// Matches between different keys, or the same key with a different Shift state, when the
    /// other modifiers agree: `(binding, key press)`. Snapshot of the frozen pre-merge
    /// Editor matcher's layout aliases (Photoshop-style), which the shared matcher keeps.
    const LAYOUT_ALIASES: &[(&str, &str)] = &[
        ("Backspace", "Delete"),
        ("Shift+Backspace", "Shift+Delete"),
        ("Delete", "Backspace"),
        ("Shift+Delete", "Shift+Backspace"),
        ("Colon", "Shift+Colon"),
        ("Plus", "Shift+Plus"),
        ("Equals", "Plus"),
        ("Equals", "Shift+Plus"),
        ("Shift+Equals", "Shift+Plus"),
        ("Shift+Semicolon", "Shift+Colon"),
        ("Shift+OpenBracket", "Shift+OpenCurlyBracket"),
        ("Shift+CloseBracket", "Shift+CloseCurlyBracket"),
        ("OpenCurlyBracket", "Shift+OpenCurlyBracket"),
        ("CloseCurlyBracket", "Shift+CloseCurlyBracket"),
        ("Shift+Backslash", "Shift+Pipe"),
        ("Shift+Slash", "Shift+Questionmark"),
        ("Pipe", "Shift+Pipe"),
        ("Questionmark", "Shift+Questionmark"),
        ("Exclamationmark", "Shift+Exclamationmark"),
        ("Shift+1", "Shift+Exclamationmark"),
    ];

    #[test]
    fn matching_golden_and_exact_mac_control() {
        for key in Key::ALL {
            for bits in 0..16 {
                let sc = KeyboardShortcut::new(mods(bits), *key);
                for actual_key in Key::ALL {
                    for actual_bits in 0..16 {
                        let actual = mods(actual_bits);
                        // An identical chord identity (Delete = Backspace, Ctrl folded into Cmd off
                        // the Mac) always matches.
                        if input::identity(sc.modifiers, *key) == input::identity(actual, *actual_key) {
                            assert!(input::key_matches(&sc, *actual_key, actual), "lost exact match {sc:?} {actual_key:?} {actual:?}");
                        }
                    }
                }
            }
        }
        let spell = |shift: bool, key: Key| format!("{}{}", if shift { "Shift+" } else { "" }, key.name());
        for platform in [Platform::Mac, Platform::Ctrl] {
            // Cmd/Ctrl/Alt held identically on both sides; Shift and the key vary.
            for other in 0..8 {
                for key in Key::ALL {
                    for pattern_shift in [false, true] {
                        let sc = KeyboardShortcut::new(Modifiers { shift: pattern_shift, ..mods(other) }, *key);
                        for actual_key in Key::ALL {
                            for actual_shift in [false, true] {
                                let actual = Modifiers { shift: actual_shift, ..mods(other) };
                                let (binding, press) = (spell(pattern_shift, *key), spell(actual_shift, *actual_key));
                                let expected =
                                    (key == actual_key && pattern_shift == actual_shift) || LAYOUT_ALIASES.contains(&(binding.as_str(), press.as_str()));
                                assert_eq!(
                                    input::key_matches_on(&sc, *actual_key, actual, platform),
                                    expected,
                                    "{platform:?} {sc:?} {actual_key:?} {actual:?}"
                                );
                            }
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
    }

    #[test]
    fn core_and_egui_overlap_parity() {
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
fn dispatch_preserves_order_unmatched_keys_and_releases() {
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
fn capture_platform_mapping_preserves_mac_control_command() {
    use egui::{Key, Modifiers};
    let physical = Modifiers { ctrl: true, command: true, mac_cmd: true, alt: true, shift: true };
    assert_eq!(input::shortcut_text_on(Key::M, physical, Platform::Mac).as_deref(), Some("Cmd+Ctrl+Alt+Shift+M"));
    assert_eq!(input::shortcut_text_on(Key::M, physical, Platform::Ctrl).as_deref(), Some("Cmd+Alt+Shift+M"));
    assert_eq!(input::shortcut_text_on(Key::Delete, Modifiers::NONE, Platform::Mac).as_deref(), Some("Delete"));
    assert!(input::shortcut_text_on(Key::ControlLeft, physical, Platform::Mac).is_none());
    assert!(input::is_function_key(Key::F35));
    assert!(!input::is_function_key(Key::F));
}
