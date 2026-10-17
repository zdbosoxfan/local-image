#[test]
fn colour_preferences_take_a_typed_hex_value() {
    // #668: the canvas colour (and every colour preference) showed its hex but couldn't take one.
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let mut h = egui_kittest::Harness::new_ui_state(|ui, rgb: &mut [u8; 3]| super::hex_field(ui, "interface.canvasCustomColor", rgb), [0x28, 0x28, 0x28]);
    h.run();
    assert_eq!(h.get_by_role(Role::TextInput).value().as_deref(), Some("#282828"));
    h.get_by_role(Role::TextInput).click();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    for ch in "#80".chars() {
        h.event(egui::Event::Text(ch.to_string()));
        h.run_steps(1);
    }
    // A partial value stays as typed and leaves the colour alone.
    assert_eq!(h.get_by_role(Role::TextInput).value().as_deref(), Some("#80"));
    assert_eq!(*h.state(), [0x28, 0x28, 0x28]);
    for ch in "8080".chars() {
        h.event(egui::Event::Text(ch.to_string()));
        h.run_steps(1);
    }
    assert_eq!(*h.state(), [0x80, 0x80, 0x80]);
}

#[test]
fn rendering_mode_display_respects_explicit_mode() {
    let explicit = serde_json::json!({"renderingMode": "gpu", "gpuBackend": "cpu"});
    assert_eq!(super::rendering_mode_value(explicit.as_object().unwrap()), "gpu");
    let cpu = serde_json::json!({"renderingMode": "cpu", "gpuBackend": "auto"});
    assert_eq!(super::rendering_mode_value(cpu.as_object().unwrap()), "cpu");
    let automatic = serde_json::json!({"gpuBackend": "cpu"});
    assert_eq!(super::rendering_mode_value(automatic.as_object().unwrap()), "auto");
}
use super::*;
use std::sync::{Arc, Mutex};

/// Every generated preference label, section title and choice label has an entry in each
/// language that claims complete menus (Japanese, Traditional Chinese, ...).
#[test]
fn preference_labels_are_translated() {
    let session = photocraft_engine::Session::new();
    let v: Value = serde_json::from_str(&session.prefs_to_json()).expect("prefs json");
    for lang in li_i18n::Lang::ALL.into_iter().filter(|l| l.complete_menus()) {
        let mut missing = Vec::new();
        for (sec, title) in SECTIONS {
            if !li_i18n::has(lang, title) {
                missing.push(title.to_string());
            }
            let Some(obj) = v.get(sec).and_then(Value::as_object) else { continue };
            for k in obj.keys() {
                let mut labels = vec![humanize(k)];
                labels.extend(prefs::choices(&format!("{sec}.{k}")).into_iter().flatten().map(|c| choice_label(c)));
                if sec == "general" && k == "checkForUpdatesAtStartup" {
                    // This new string is deferred to the pending translation list.
                    // Keep every existing caption and all choice labels under the strict gate.
                    let caption = humanize(k);
                    let pending: &[&str] = include!("../i18n_pending_vector.rs");
                    assert!(pending.contains(&caption.as_str()), "new update preference must be registered for translation");
                    labels.retain(|label| *label != caption);
                }
                missing.extend(labels.into_iter().filter(|l| !li_i18n::has(lang, l)));
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "{}: untranslated preference labels: {missing:#?}", lang.code());
    }
}

fn app_with_store() -> (PhotocraftApp, Arc<Mutex<Option<String>>>) {
    app_with_saved(None)
}

fn app_with_saved(text: Option<String>) -> (PhotocraftApp, Arc<Mutex<Option<String>>>) {
    let store: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(text));
    (app_on(&store, Arc::default()), store)
}

/// An app whose preferences live in `store`; writes fail while `fail` is set.
fn app_on(store: &Arc<Mutex<Option<String>>>, fail: Arc<std::sync::atomic::AtomicBool>) -> PhotocraftApp {
    let (a, b) = (store.clone(), store.clone());
    let services = crate::Services {
        load_prefs: Some(Box::new(move || a.lock().unwrap().clone())),
        save_prefs: Some(Box::new(move |s: &str| {
            if fail.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("storage is full".into());
            }
            *b.lock().unwrap() = Some(s.to_string());
            Ok(())
        })),
        ..Default::default()
    };
    PhotocraftApp::new(photocraft_engine::Session::new(), services)
}

fn stored(store: &Arc<Mutex<Option<String>>>) -> Value {
    serde_json::from_str(store.lock().unwrap().as_deref().unwrap_or("null")).unwrap()
}

#[test]
fn transient_initial_read_failure_rebases_local_edits_and_next_save() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let original = r#"{"interface":{"theme":"studio"},"shortcutProfiles":{"editor":{"active":"Mine","custom":{"Mine":{"parent":"Photoshop","overrides":{"file.new":"Ctrl+Alt+N","file.open":"Ctrl+Shift+O"}}}}},"performance":{"historyStates":12},"future":{"keep":"original"}}"#;
    let store = Arc::new(Mutex::new(Some(original.to_string())));
    let failed = Arc::new(AtomicBool::new(true));
    let writes = Arc::new(AtomicUsize::new(0));
    let (read, write, read_failure, write_count) = (store.clone(), store.clone(), failed.clone(), writes.clone());
    let mut app = PhotocraftApp::new(
        photocraft_engine::Session::new(),
        crate::Services {
            load_prefs_checked: Some(Box::new(move || {
                if read_failure.load(Ordering::Relaxed) {
                    return Err(crate::PrefsLoadError::Unreadable("temporarily locked".into()));
                }
                Ok(read.lock().unwrap().clone())
            })),
            save_prefs: Some(Box::new(move |text| {
                write_count.fetch_add(1, Ordering::Relaxed);
                *write.lock().unwrap() = Some(text.into());
                Ok(())
            })),
            ..Default::default()
        },
    );
    app.run("prefs.set", json!({"values":{"interface.theme":"classic"}})).unwrap();
    assert_eq!(persist(&mut app, 100.), Some(SAVE_RETRY_S));
    assert_eq!(writes.load(Ordering::Relaxed), 0, "unreadable source must never reach the writer");
    assert_eq!(store.lock().unwrap().as_deref(), Some(original));
    failed.store(false, Ordering::Relaxed);
    assert_eq!(persist(&mut app, 102.), Some(0.));
    let saved = stored(&store);
    assert_eq!(saved["interface"]["theme"], "classic");
    assert_eq!(saved["shortcutProfiles"]["editor"]["custom"]["Mine"]["overrides"], json!({"file.new":"Ctrl+Alt+N","file.open":"Ctrl+Shift+O"}));
    assert_eq!(saved["performance"]["historyStates"], 12);
    assert_eq!(saved["future"]["keep"], "original");
    assert_eq!(app.session.prefs_value()["shortcutProfiles"]["editor"], saved["shortcutProfiles"]["editor"], "live prefs must be rebased too");
    assert_eq!(app.session.prefs_value()["performance"]["historyStates"], 12);
    assert_eq!(app.prefs_rt.saved_value, Some(app.session.prefs_value()));
    // A later ordinary save must not write old default shortcuts back over the recovered file.
    app.run("prefs.set", json!({"values":{"performance.historyStates":32}})).unwrap();
    assert_eq!(persist(&mut app, 103.), None);
    let next = stored(&store);
    assert_eq!(next["shortcutProfiles"], saved["shortcutProfiles"]);
    assert_eq!(next["performance"]["historyStates"], 32);
    assert_eq!(next["future"], saved["future"]);
    assert_eq!(writes.load(Ordering::Relaxed), 2);
}

#[test]
fn checked_preferences_keep_three_way_merge_and_persistent_recovery_location() {
    let store = Arc::new(Mutex::new(Some(r#"{"interface":{"theme":"studio"},"future":{"value":1}}"#.to_string())));
    let read = store.clone();
    let write = store.clone();
    let mut app = PhotocraftApp::new(
        photocraft_engine::Session::new(),
        crate::Services {
            load_prefs: Some(Box::new(|| Some("{broken legacy loader".into()))),
            load_prefs_checked: Some(Box::new(move || Ok(read.lock().unwrap().clone()))),
            save_prefs: Some(Box::new(move |text| {
                *write.lock().unwrap() = Some(text.into());
                Ok(())
            })),
            prefs_recovery: Some(Box::new(|| {
                Some(crate::PrefsRecoveryNotice {
                    backup_path: "/settings/preference-backups/original.json".into(),
                    reason: "Invalid fields used defaults".into(),
                })
            })),
            ..Default::default()
        },
    );
    assert_eq!(app.session.prefs().interface.theme, Theme::Studio);
    app.run("prefs.set", json!({"values":{"interface.theme":"classic"}})).unwrap();
    *store.lock().unwrap() = Some(r#"{"interface":{"theme":"studio"},"future":{"value":2,"otherWindow":true}}"#.into());
    assert_eq!(persist(&mut app, 100.), None);
    let saved = stored(&store);
    assert_eq!(saved["interface"]["theme"], "classic");
    assert_eq!(saved["future"], json!({"value":2,"otherWindow":true}));
    app.ui.status = "a later action replaced the status text".into();
    show_prefs_recovery(&mut app);
    let notices: Vec<_> = app.ui.notices.iter().filter(|notice| notice.title == "Original preferences preserved").collect();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].lines.iter().any(|line| line.contains("/settings/preference-backups/original.json")));
    assert!(notices[0].dismiss_pref.is_some());
}

#[test]
fn merge_keeps_changes_made_elsewhere() {
    let base = json!({"a": {"x": 1, "y": 1}, "list": [1], "gone": 1, "kept": 1});
    // Here: a.x changed, `gone` removed, `new` added.
    let ours = json!({"a": {"x": 2, "y": 1}, "list": [1], "kept": 1, "new": 1});
    // There: a.y, `list` and `kept` changed, and a key this version doesn't know.
    let mut theirs = json!({"a": {"x": 1, "y": 3}, "list": [1, 2], "gone": 1, "kept": 5, "future": true});
    merge_changes(&base, &ours, &mut theirs);
    assert_eq!(theirs, json!({"a": {"x": 2, "y": 3}, "list": [1, 2], "kept": 5, "new": 1, "future": true}));
    // Arrays are leaves: a list changed here replaces the stored one whole.
    merge_changes(&json!({"list": [1]}), &json!({"list": [1, 3]}), &mut theirs);
    assert_eq!(theirs["list"], json!([1, 3]));
    // Nothing changed here: storage is left as it is, whatever it holds.
    let mut odd = json!({"x": [7]});
    merge_changes(&ours, &ours, &mut odd);
    assert_eq!(odd, json!({"x": [7]}));
    // A value that changed type here replaces the stored one.
    merge_changes(&json!({"a": 1}), &json!({"a": {"b": 2}}), &mut theirs);
    assert_eq!(theirs["a"], json!({"b": 2}));
}

#[test]
fn saving_keeps_preferences_another_instance_changed() {
    let store = Arc::new(Mutex::new(None));
    let ctx = egui::Context::default();
    // Two browser tabs (or app windows) on the same storage.
    let (mut a, mut b) = (app_on(&store, Arc::default()), app_on(&store, Arc::default()));
    tick(&mut a, &ctx);
    tick(&mut b, &ctx);
    a.run("prefs.set", json!({"values": {"interface.theme": "studio"}})).unwrap();
    tick(&mut a, &ctx);
    assert_eq!(stored(&store)["interface"]["theme"], "studio");
    // The other one, never reloaded, saves an unrelated change: the theme stays.
    b.run("prefs.set", json!({"values": {"performance.historyStates": 12}})).unwrap();
    tick(&mut b, &ctx);
    let v = stored(&store);
    assert_eq!((v["interface"]["theme"].as_str(), v["performance"]["historyStates"].as_u64()), (Some("studio"), Some(12)));
    // And the first one's next save keeps the second one's change.
    a.run("prefs.set", json!({"values": {"interface.showTooltips": false}})).unwrap();
    tick(&mut a, &ctx);
    let v = stored(&store);
    assert_eq!(v["performance"]["historyStates"], 12);
    assert_eq!(v["interface"]["showTooltips"], false);
    // A value changed in both: the latest save wins.
    b.run("prefs.set", json!({"values": {"interface.theme": "classic"}})).unwrap();
    tick(&mut b, &ctx);
    assert_eq!(stored(&store)["interface"]["theme"], "classic");
    // Unreadable storage is overwritten with this instance's preferences.
    *store.lock().unwrap() = Some("not json".into());
    a.run("prefs.set", json!({"values": {"performance.historyStates": 30}})).unwrap();
    tick(&mut a, &ctx);
    let (mut c, _) = app_with_saved(store.lock().unwrap().clone());
    tick(&mut c, &ctx);
    assert_eq!(c.session.prefs().performance.history_states, 30);
    assert_eq!(c.session.prefs().interface.theme, Theme::Studio);
}

#[test]
fn failed_preference_saves_are_retried() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let store = Arc::new(Mutex::new(None));
    let fail = Arc::new(AtomicBool::new(true));
    let mut app = app_on(&store, fail.clone());
    tick(&mut app, &egui::Context::default());
    app.run("prefs.set", json!({"values": {"interface.theme": "studio"}})).unwrap();
    // The write fails: reported, and retried after a backoff rather than marked saved.
    assert_eq!(persist(&mut app, 100.0), Some(SAVE_RETRY_S));
    assert!(app.ui.status.starts_with("Couldn't save preferences: storage is full") && app.ui.status_error);
    assert!(store.lock().unwrap().is_none());
    assert_eq!(persist(&mut app, 101.0), Some(1.0), "not due yet");
    // Storage works again: the pending change is written without another edit.
    fail.store(false, Ordering::Relaxed);
    assert_eq!(persist(&mut app, 102.0), None);
    assert_eq!(stored(&store)["interface"]["theme"], "studio");
    assert_eq!(app.ui.status, "Preferences saved");
    assert_eq!(persist(&mut app, 103.0), None, "nothing left to save");

    // A persistent failure backs off up to a cap and raises a notice once.
    fail.store(true, Ordering::Relaxed);
    app.run("prefs.set", json!({"values": {"interface.theme": "classic"}})).unwrap();
    let mut now = 200.0;
    let mut delays = Vec::new();
    for _ in 0..6 {
        let wait = persist(&mut app, now).unwrap();
        delays.push(wait);
        now += wait;
    }
    assert_eq!(delays, [2.0, 4.0, 8.0, 16.0, SAVE_RETRY_MAX_S, SAVE_RETRY_MAX_S]);
    assert_eq!(app.ui.notices.iter().filter(|n| n.error && n.title == "Preferences can't be saved").count(), 1);
    // A newer change retries sooner than the backoff, but not on every frame.
    app.run("prefs.set", json!({"values": {"interface.theme": "pro"}})).unwrap();
    assert_eq!(persist(&mut app, now - 29.0), Some(1.0));
    fail.store(false, Ordering::Relaxed);
    assert_eq!(persist(&mut app, now - 28.0), None);
    assert_eq!(stored(&store)["interface"]["theme"], "pro");
    assert!(app.ui.notices.is_empty(), "the notice goes once preferences are saved");
}

#[test]
fn auto_scale_detects_4k_and_preserves_larger_system_dpi() {
    use prefs::UiScale::Auto;
    for size in [vec2(3840.0, 2160.0), vec2(4096.0, 2160.0), vec2(2160.0, 3840.0)] {
        assert_eq!(display_scale(Auto, Some(1.0), Some(size)), 2.0);
        for dpi in [1.25, 1.5, 2.0] {
            assert_eq!(display_scale(Auto, Some(dpi), Some(size)), 2.0);
        }
    }
    assert_eq!(display_scale(Auto, Some(3.0), Some(vec2(3840.0, 2160.0))), 3.0);
    for size in [vec2(1920.0, 1080.0), vec2(2560.0, 1440.0), vec2(3840.0, 1080.0)] {
        assert_eq!(display_scale(Auto, Some(1.0), Some(size)), 1.0);
    }
    assert_eq!(display_scale(Auto, None, None), 1.0);
    assert_eq!(display_scale(Auto, Some(f32::NAN), Some(vec2(f32::INFINITY, 2160.0))), 1.0);
}

#[test]
fn scale_preferences_and_monitor_changes_apply_live() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    {
        let mut step = |physical: egui::Vec2, native: f32, expected: f32| {
            let mut input = egui::RawInput::default();
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.native_pixels_per_point = Some(native);
            viewport.monitor_size = Some(physical / (native * ctx.zoom_factor()));
            ctx.run_ui(input, |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
            // Scale changes take effect on the following pass.
            let mut input = egui::RawInput::default();
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.native_pixels_per_point = Some(native);
            viewport.monitor_size = Some(physical / (native * ctx.zoom_factor()));
            ctx.run_ui(input, |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
            assert!((ctx.pixels_per_point() - expected).abs() < 1e-4);
        };
        for _ in 0..4 {
            step(vec2(3840.0, 2160.0), 1.0, 2.0);
        }
        step(vec2(1920.0, 1080.0), 1.0, 1.0);
        step(vec2(3840.0, 2160.0), 1.5, 2.0);
    }
    for (pref, expected) in [("200", 2.0), ("125", 1.25), ("150", 1.5), ("100", 1.0), ("auto", 1.5)] {
        app.run("prefs.set", json!({"values": {"interface.uiScale": pref}})).unwrap();
        let mut input = egui::RawInput::default();
        input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().native_pixels_per_point = Some(1.5);
        ctx.run_ui(input.clone(), |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
        ctx.run_ui(input, |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
        assert!((ctx.pixels_per_point() - expected).abs() < 1e-4);
    }
}

#[test]
fn ui_font_size_commands_follow_themes_and_dpi_and_reset_live() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, ThemeKind::Pro);
    let mut input = egui::RawInput::default();
    input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().native_pixels_per_point = Some(1.5);
    for (theme, scale, expected) in
        [("pro", "100", 1.0), ("studio", "auto", 1.5), ("studioLight", "200", 2.0), ("proMedium", "100", 1.0), ("classic", "auto", 1.5)]
    {
        app.run("prefs.set", json!({"values": {"interface.uiFontSize": "large", "interface.theme": theme, "interface.uiScale": scale}})).unwrap();
        for _ in 0..3 {
            ctx.run_ui(input.clone(), |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
        }
        assert!((ctx.pixels_per_point() - expected).abs() < 1e-4);
        assert_eq!(ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
    }
    app.run("prefs.reset", json!({"path": "interface.uiFontSize"})).unwrap();
    for _ in 0..2 {
        ctx.run_ui(input.clone(), |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
    }
    assert_eq!(ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 1.0);
    assert!((ctx.pixels_per_point() - 1.5).abs() < 1e-4, "resetting font size keeps UI Scale");
}

#[test]
fn recent_files_survive_a_restart_and_honour_the_count() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    app.push_recent("/work/a.psd");
    app.push_recent("/work/b.png");
    tick(&mut app, &ctx);
    let saved = store.lock().unwrap().clone().unwrap();
    // A new app instance (a restart) gets the list back, newest first.
    let (mut app2, _) = app_with_saved(Some(saved));
    tick(&mut app2, &ctx);
    assert_eq!(app2.ui.recent_files, vec!["/work/b.png".to_string(), "/work/a.psd".to_string()]);
    // Lowering "Recent File List Contains" shortens the menu at once; 0 turns it off.
    app2.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 1}})).unwrap();
    tick(&mut app2, &ctx);
    assert_eq!(app2.ui.recent_files, vec!["/work/b.png".to_string()]);
    app2.push_recent("/work/c.tif");
    assert_eq!(app2.session.prefs().file_handling.recent_files, vec!["/work/c.tif".to_string()]);
    app2.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 0}})).unwrap();
    tick(&mut app2, &ctx);
    app2.push_recent("/work/d.tif");
    assert!(app2.ui.recent_files.is_empty());
    // Clearing the list in the Preferences dialog (or by an agent) reaches the menu.
    app2.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 20, "fileHandling.recentFiles": ["/x.psd"]}})).unwrap();
    tick(&mut app2, &ctx);
    assert_eq!(app2.ui.recent_files, vec!["/x.psd".to_string()]);
    // A hostile count from a hand-edited preferences file is capped, not trusted.
    app2.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 100}})).unwrap();
    app2.session.prefs.edit(|p| p.file_handling.recent_file_count = u32::MAX);
    assert_eq!(app2.recent_cap(), 100);
}

#[test]
fn show_tooltips_preferences_turn_tooltips_off() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    assert_eq!(ctx.global_style().interaction.tooltip_delay, crate::theme::TOOLTIP_DELAY);
    for path in ["interface.showTooltips", "tools.showTooltips"] {
        app.run("prefs.set", json!({"values": {path: false}})).unwrap();
        tick(&mut app, &ctx);
        assert!(ctx.global_style().interaction.tooltip_delay.is_infinite(), "{path} off hides tooltips");
        app.run("prefs.set", json!({"values": {path: true}})).unwrap();
        tick(&mut app, &ctx);
        assert_eq!(ctx.global_style().interaction.tooltip_delay, crate::theme::TOOLTIP_DELAY);
    }
}

#[test]
fn unimplemented_preferences_are_hidden_from_the_dialog() {
    let values = prefs::Preferences::default().to_json();
    assert!(has_visible_fields(&values, "general"));
    assert!(has_visible_fields(&values, "fileHandling"));
    // Every setting of these sections is still unimplemented.
    for section in ["type", "enhancedControls", "rawDefaults", "integrations", "scratchDisks"] {
        assert!(!has_visible_fields(&values, section), "{section}");
    }
    assert!(prefs::is_hidden("rawDefaults.applyAutoTone"));
    assert!(!prefs::is_hidden("general.autoShowHomeScreen"));
    assert!(!prefs::is_hidden("interface.uiScale"));
    assert!(!prefs::is_hidden("interface.uiFontSize"));
    // Hidden values still round-trip through the dialog untouched.
    let (mut app, _) = app_with_store();
    app.run("prefs.set", json!({"values": {"type.smartQuotes": false}})).unwrap();
    let id = open_preferences(&mut app, "type");
    let fields = app.ui.dialogs.iter().find(|d| d.id == id).map(|d| d.fields.clone()).unwrap();
    confirm(&mut app, &fields).unwrap();
    assert!(!app.session.prefs().type_.smart_quotes);
}

#[test]
fn preferences_persist_through_services_and_theme_follows() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    app.run("prefs.set", json!({"values": {"interface.theme": "studioLight", "performance.historyStates": 12}})).unwrap();
    tick(&mut app, &ctx);
    assert_eq!(app.ui.theme, ThemeKind::StudioLight);
    let saved = store.lock().unwrap().clone().unwrap();
    assert!(saved.contains("\"historyStates\": 12"));
    // A new app instance loads them.
    let (mut app2, store2) = app_with_saved(Some(saved));
    tick(&mut app2, &ctx);
    assert_eq!(app2.session.prefs().performance.history_states, 12);
    assert_eq!(app2.ui.theme, ThemeKind::StudioLight);
    // Picking a theme from the Window menu becomes the preference.
    crate::menus::invoke(&mut app2, &ctx, "window.theme.classic", json!({})).unwrap();
    tick(&mut app2, &ctx);
    assert_eq!(app2.session.prefs().interface.theme, Theme::Classic);
    assert!(store2.lock().unwrap().as_ref().unwrap().contains("classic"));
}

#[test]
fn brush_preset_store_attaches_when_loaded_and_persists() {
    use photocraft_engine::preset_store::{MemBackend, open};
    let mem = MemBackend::default();
    let ctx = egui::Context::default();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services { preset_store: Some(rx), ..Default::default() });
    // Still loading: presets made now are kept and written once the store arrives.
    tick(&mut app, &ctx);
    app.run("brush.presets.save", json!({"name": "Early"})).unwrap();
    assert!(mem.files.lock().unwrap().is_empty());
    tx.send(open(Box::new(mem.clone()))).unwrap();
    tick(&mut app, &ctx);
    assert!(app.services.preset_store.is_none() && app.session.preset_store.is_some());
    app.run("brush.presets.save", json!({"name": "Late"})).unwrap();
    let mut s2 = photocraft_engine::Session::new();
    assert!(s2.attach_preset_store(open(Box::new(mem))).is_empty());
    for n in ["Early", "Late"] {
        assert!(s2.tools.presets.iter().any(|p| p.name == n), "{n}");
    }
}

#[test]
fn ai_preferences_real_click_apply_updates_the_single_root() {
    use egui_kittest::{Harness, kittest::Queryable};
    let initial = serde_json::to_string(&prefs::Preferences::default()).unwrap();
    let (app, store) = app_with_saved(Some(initial));
    let read = store.clone();
    let write = store.clone();
    let backend = li_ai::settings::StoreBackend {
        load: Arc::new(move || {
            Ok(read.lock().unwrap().as_ref().and_then(|text| serde_json::from_str::<Value>(text).ok()).and_then(|root| root.get("ai").cloned()))
        }),
        save: Arc::new(move |ai| {
            let mut saved = write.lock().unwrap();
            let mut root: Value = serde_json::from_str(saved.as_ref().unwrap()).unwrap();
            root["ai"] = ai;
            *saved = Some(serde_json::to_string(&root).unwrap());
            Ok(())
        }),
    };
    li_ai::settings::with_store_backend(backend, || {
        let mut app = app;
        let id = open_preferences(&mut app, "ai.connection");
        let mut harness = Harness::builder().with_size(vec2(1100.0, 850.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if !super::design_system_shots::fonts_ready(ui) {
                    return;
                }
                crate::dialogs::show(app, ui.ctx());
            },
            app,
        );
        harness.run_steps(4);
        harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Use a remote ComfyUI server").click();
        harness.run();
        assert_eq!(harness.state().ui.dialogs.len(), 1);
        assert_eq!(harness.state().ui.dialogs[0].id, id);
        harness.get_by_label("Apply").click();
        harness.run();
        let root: Value = serde_json::from_str(store.lock().unwrap().as_ref().unwrap()).unwrap();
        assert_eq!(root["ai"]["setup_mode"], "remote");
        assert!(root.get("library").is_some());
        assert!(root.get("general").is_some());
        assert_eq!(harness.state().ui.dialogs.len(), 1);
        harness.get_by_label("Cancel").click();
        harness.run();
        assert!(harness.state().ui.dialogs.is_empty());
    });
}

#[test]
fn preferences_dialog_round_trip() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    let r = crate::menus::invoke(&mut app, &ctx, "edit.preferences.cursors", json!({})).unwrap();
    let id = r["dialog"].as_u64().unwrap();
    let d = app.ui.dialogs.iter().find(|d| d.id == id).unwrap().clone();
    assert_eq!(d.fields["section"], "cursors");
    let mut values = d.fields["values"].clone();
    values["cursors"]["painting"] = json!("fullSizeTip");
    values["unitsAndRulers"]["rulers"] = json!("inches");
    app.ui.dialog_mut(id).unwrap().fields.insert("values".into(), values);
    crate::dialogs::confirm(&mut app, id).unwrap();
    assert_eq!(app.session.prefs().cursors.painting, prefs::PaintingCursor::FullSizeTip);
    assert_eq!(app.session.prefs().units_and_rulers.rulers, prefs::Unit::Inches);
    // Invalid values are rejected and nothing changes.
    let id = open_preferences(&mut app, "performance");
    let mut values = app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields["values"].clone();
    values["performance"]["historyStates"] = json!(0);
    app.ui.dialog_mut(id).unwrap().fields.insert("values".into(), values);
    assert!(crate::dialogs::confirm(&mut app, id).is_err());
    assert_eq!(app.session.prefs().performance.history_states, 50);
}

/// local-image: Performance › Processing shows the automatic values and stores the optional
/// overrides (thread count, background priority, memory share); out-of-range values set by path
/// are refused.
#[test]
fn processing_overrides_show_auto_values_and_round_trip() {
    use egui_kittest::{Harness, kittest::Queryable};

    let (mut app, _store) = app_with_store();
    assert_eq!(app.session.prefs().performance.system, prefs::SystemResources::default());
    let id = open_preferences(&mut app, "performance");
    let mut h = Harness::builder().with_size(vec2(1280.0, 900.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(4);
    let processing = h.get_by_label(PROCESSING_THREADS).rect();
    h.hover_at(egui::pos2(processing.center().x, h.ctx.content_rect().center().y));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -800.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(4);
    h.get_by_label("Keep full speed on battery").click();
    h.run_steps(2);
    assert_eq!(h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap().fields["values"]["performance"]["system"]["fullSpeedOnBattery"], true);
    let auto = format!("Automatic ({})", lightcraft_sysmem::profile().threads_auto);
    assert!(h.query_by_label(PROCESSING_THREADS).is_some());
    // The dropdown shows its choice as the combo box's value (a label in some egui versions).
    assert!(h.query_by_value(&auto).is_some() || h.query_by_label(&auto).is_some(), "the automatic thread count is shown");
    assert!(h.query_by_label_contains("physical cores").is_some(), "the detected hardware is shown");
    let values = h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap();
    values["performance"]["system"] = json!({"threads": 3, "backgroundPriority": "normal", "memoryPercent": 60});
    h.run_steps(2);
    let app = h.state_mut();
    crate::dialogs::confirm(app, id).unwrap();
    let c = &app.session.prefs().performance.system;
    assert_eq!((c.threads, c.background_priority, c.memory_percent), (3, prefs::BackgroundPriority::Normal, 60));
    assert!(app.run("prefs.set", json!({"values": {"performance.system.memoryPercent": 95}})).is_err());
    assert!(app.run("prefs.set", json!({"values": {"performance.system.backgroundPriority": "idle"}})).is_err());
    app.run("prefs.set", json!({"values": {"performance.system.threads": 0}})).unwrap();
    assert_eq!((app.session.prefs().performance.system.threads, app.session.prefs().performance.system.memory_percent), (0, 60));
}

#[test]
fn failed_ai_save_keeps_core_host_history_and_preferences_drafts_atomic() {
    use egui_kittest::{Harness, kittest::Queryable};
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct HostDraft(Rc<Cell<bool>>);
    impl HostPreferencesPages for HostDraft {
        fn pages(&self) -> &'static [(&'static str, &'static str)] {
            &[("library.test", "Test")]
        }
        fn values(&self) -> Value {
            json!({"enabled": self.0.get()})
        }
        fn body(&mut self, _: &mut egui::Ui, _: &str, _: &mut Value) {}
        fn prepare(&self, draft: &Value) -> Result<Box<dyn FnOnce() + '_>, String> {
            let enabled = draft["enabled"].as_bool().ok_or("Invalid host draft")?;
            let state = self.0.clone();
            Ok(Box::new(move || state.set(enabled)))
        }
    }
    fn edit(h: &mut Harness<'_, PhotocraftApp>, id: u64) {
        h.run_steps(4);
        h.get_by_role_and_label(egui::accesskit::Role::PasswordInput, "OpenAI:").focus();
        h.run_steps(1);
        h.get_by_role_and_label(egui::accesskit::Role::PasswordInput, "OpenAI:").type_text("synthetic-key-never-sent");
        h.run_steps(2);
        let fields = &mut h.state_mut().ui.dialog_mut(id).unwrap().fields;
        fields.get_mut("values").unwrap()["interface"]["theme"] = json!("studioLight");
        fields.get_mut("values").unwrap()["performance"]["historyStates"] = json!(1);
        fields.insert("hostValues".into(), json!({"enabled":true}));
        h.run_steps(2);
    }
    for button in ["Apply", "OK"] {
        let failure = Arc::new(AtomicBool::new(true));
        let saved = Arc::new(Mutex::new(json!({"auto_start_comfy":false})));
        let load = saved.clone();
        let write = saved.clone();
        let fail = failure.clone();
        li_ai::settings::with_store_backend(
            li_ai::settings::StoreBackend {
                load: Arc::new(move || Ok(Some(load.lock().unwrap().clone()))),
                save: Arc::new(move |value| {
                    if fail.load(Ordering::SeqCst) {
                        return Err("injected AI preferences write failure".into());
                    }
                    *write.lock().unwrap() = value;
                    Ok(())
                }),
            },
            || {
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
                app.run("prefs.set", json!({"values":{"interface.language":"en"}})).unwrap();
                app.run("file.new", json!({"width":4,"height":4})).unwrap();
                for color in ["#ff0000", "#00ff00", "#0000ff", "#ff00ff"] {
                    app.run("edit.fill", json!({"color":color})).unwrap();
                }
                let host = Rc::new(Cell::new(false));
                app.host_preferences = Some(Box::new(HostDraft(host.clone())));
                let before = app.session.prefs_value();
                let history = app.session.active().unwrap().history.past_len();
                assert!(history > 1);
                let pixels = app.session.active().unwrap().doc.clone();
                let mut h = Harness::builder().with_size(vec2(1200., 900.)).build_ui_state(|ui, app| crate::dialogs::show(app, ui.ctx()), app);
                PhotocraftApp::setup_context(&h.ctx, Default::default());
                let id = open_preferences(h.state_mut(), "ai.providers");
                edit(&mut h, id);
                h.get_by_label(button).click();
                h.run_steps(3);
                assert_eq!(h.state().session.prefs_value(), before, "{button} failure must not publish core values");
                assert_eq!(h.state().session.active().unwrap().history.past_len(), history, "validation cannot trim undo history");
                assert_eq!(*h.state().session.active().unwrap().doc, *pixels);
                assert!(!host.get(), "host values remain unchanged");
                assert!(h.state().ui.dialogs.iter().any(|dialog| dialog.id == id), "failed OK retains its draft too");
                assert_eq!(
                    h.state().ui.dialogs.iter().find(|dialog| dialog.id == id).unwrap().fields.get("__error").and_then(Value::as_str),
                    Some("injected AI preferences write failure"),
                    "{button} must reach the checked writer after validating a new provider key"
                );
                assert!(h.query_by_label_contains("injected AI preferences write failure").is_some());
                h.get_by_label("Cancel").click();
                h.run_steps(2);
                assert_eq!(h.state().session.prefs_value(), before);
                assert!(!host.get());
                // Retry from a fresh draft with a functioning writer commits all portions.
                let id = open_preferences(h.state_mut(), "ai.providers");
                edit(&mut h, id);
                failure.store(false, Ordering::SeqCst);
                h.get_by_label(button).click();
                h.run_steps(3);
                assert_eq!(h.state().session.prefs().interface.theme, prefs::Theme::StudioLight);
                assert_eq!(h.state().session.prefs().performance.history_states, 1);
                assert!(h.state().session.active().unwrap().history.past_len() <= 1);
                assert!(host.get());
                assert_eq!(h.state().session.prefs().ai.extra_string("openai_api_key").as_deref(), Some("synthetic-key-never-sent"));
                assert_eq!(
                    h.state().session.prefs().ai.extra_string("openai_api_key"),
                    saved.lock().unwrap().get("openai_api_key").and_then(Value::as_str).map(str::to_string)
                );
                if button == "Apply" {
                    crate::dialogs::cancel(h.state_mut(), id);
                }
            },
        );
    }
}

#[test]
fn preferences_apply_button_saves_without_closing_and_cancel_keeps_applied_values() {
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    let (mut app, store) = app_with_store();
    app.run("prefs.set", json!({"values": {"interface.language": "en", "type.smartQuotes": false}})).unwrap();
    let id = open_preferences(&mut app, "interface");
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(4);
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled());

    let values = h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap();
    values["interface"]["theme"] = json!("studioLight");
    values["interface"]["uiFontSize"] = json!("large");
    values["performance"]["historyStates"] = json!(12);
    h.run_steps(2);
    assert!(!h.get_by_label("Apply").accesskit_node().is_disabled());
    h.get_by_label("Apply").click();
    h.run_steps(4);

    assert_eq!(h.state().session.prefs().performance.history_states, 12);
    assert_eq!(h.state().ui.theme, ThemeKind::StudioLight);
    assert_eq!(h.ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
    assert!(!h.state().session.prefs().type_.smart_quotes, "hidden settings round-trip unchanged");
    let d = h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap();
    assert_eq!(d.fields["section"], "interface");
    assert_eq!(d.fields["values"]["performance"]["historyStates"], 12);
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled());
    let saved: Value = serde_json::from_str(store.lock().unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(saved["performance"]["historyStates"], 12);
    assert_eq!(saved["interface"]["theme"], "studioLight");
    assert_eq!(saved["interface"]["uiFontSize"], "large");

    // Repeated Apply starts from the validated values, not the dialog's original snapshot.
    h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["performance"]["historyStates"] = json!(22);
    h.run_steps(2);
    h.get_by_label("Apply").click();
    h.run_steps(4);
    assert_eq!(h.state().session.prefs().performance.history_states, 22);
    h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["performance"]["historyStates"] = json!(33);
    h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["interface"]["uiFontSize"] = json!("tiny");
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    assert!(h.state().ui.dialogs.iter().all(|d| d.id != id));
    assert_eq!(h.state().session.prefs().performance.history_states, 22);
    assert_eq!(h.ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
    let saved = store.lock().unwrap().clone().unwrap();
    let (mut restarted, _) = app_with_saved(Some(saved));
    tick(&mut restarted, &egui::Context::default());
    assert_eq!(restarted.session.prefs().performance.history_states, 22);
    assert_eq!(restarted.ui.theme, ThemeKind::StudioLight);
    assert_eq!(restarted.session.prefs().interface.ui_font_size, prefs::UiFontSize::Large);
}

#[test]
fn preferences_confirm_after_apply_commits_later_edits() {
    let (mut app, _) = app_with_store();
    let id = open_preferences(&mut app, "cursors");
    app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["cursors"]["painting"] = json!("fullSizeTip");
    apply(&mut app, id).unwrap();
    app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["unitsAndRulers"]["rulers"] = json!("inches");
    crate::dialogs::confirm(&mut app, id).unwrap();
    assert!(app.ui.dialogs.iter().all(|d| d.id != id));
    assert_eq!(app.session.prefs().cursors.painting, prefs::PaintingCursor::FullSizeTip);
    assert_eq!(app.session.prefs().units_and_rulers.rulers, prefs::Unit::Inches);
}

#[test]
fn preferences_apply_rejects_invalid_drafts_and_keeps_them_open() {
    let (mut app, _) = app_with_store();
    let before = app.session.prefs().to_json();
    for invalid in [json!(0), json!("invalid"), Value::Null] {
        let id = open_preferences(&mut app, "performance");
        app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["performance"]["historyStates"] = invalid;
        let draft = app.ui.dialog_mut(id).unwrap().fields.clone();
        let error = apply(&mut app, id).unwrap_err();
        assert_eq!(app.session.prefs().to_json(), before);
        let mut retained = app.ui.dialog_mut(id).unwrap().fields.clone();
        assert_eq!(retained.remove("__error"), Some(json!(error)), "a failed Apply adds its error without changing the draft");
        assert_eq!(retained, draft, "every editable value and existing dialog field remains intact");
        app.ui.close_dialog(id);
    }
    let id = open_preferences(&mut app, "interface");
    app.ui.dialog_mut(id).unwrap().fields.remove("values");
    assert!(apply(&mut app, id).is_err());
    assert!(app.ui.dialog_mut(id).is_some());
    assert_eq!(app.session.prefs().to_json(), before);
}

#[test]
fn apply_rejects_missing_and_non_preferences_dialogs() {
    let (mut app, _) = app_with_store();
    let before = app.session.prefs().to_json();
    assert!(apply(&mut app, u64::MAX).is_err());
    let id = open_shortcuts(&mut app, 0);
    assert!(apply(&mut app, id).is_err());
    assert!(app.ui.dialog_mut(id).is_some());
    let fields = json!({"__prefsui": "prefs", "values": preference_values(app.session.prefs())}).as_object().unwrap().clone();
    let id = app.ui.open_dialog(DialogKind::About, fields);
    assert!(apply(&mut app, id).is_err());
    assert!(app.ui.dialog_mut(id).is_some());
    assert_eq!(app.session.prefs().to_json(), before);
}

/// Upstream 57be0f58 (#956): a non-map `overrides` was read as "no overrides" and wiped every
/// custom shortcut.
#[test]
fn shortcut_dialog_rejects_malformed_overrides() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    app.run("edit.keyboardShortcuts", json!({"set": {"file.new": "Ctrl+Shift+N"}})).unwrap();
    let before = app.session.prefs().shortcut_profiles.clone();
    assert!(!before.editor.custom.is_empty());
    for bad in [json!("bogus"), json!(["file.new"]), json!({"file.new": 5})] {
        let id = crate::menus::invoke(&mut app, &ctx, "edit.keyboardShortcuts", json!({})).unwrap()["dialog"].as_u64().unwrap();
        app.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), bad.clone());
        assert!(crate::dialogs::confirm(&mut app, id).is_err(), "{bad} accepted");
        assert_eq!(app.session.prefs().shortcut_profiles, before, "{bad} changed the shortcuts");
    }
    // Drawing the open dialog keeps the malformed value for OK to reject, instead of
    // replacing it with the empty map it shows.
    let id = crate::menus::invoke(&mut app, &ctx, "edit.keyboardShortcuts", json!({})).unwrap()["dialog"].as_u64().unwrap();
    app.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!("bogus"));
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(4);
    assert_eq!(h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap().fields["overrides"], json!("bogus"));
    assert!(crate::dialogs::confirm(h.state_mut(), id).is_err());
    assert_eq!(h.state().session.prefs().shortcut_profiles, before);
}

#[test]
fn shortcut_dialog_moves_shortcuts_and_shell_dispatch_follows() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    let id = crate::menus::invoke(&mut app, &ctx, "edit.keyboardShortcuts", json!({})).unwrap()["dialog"].as_u64().unwrap();
    // ⌘O (File › Open, a shell command) given to Layer › New › Layer.
    app.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!({"layer.new.layer": "Cmd+O"}));
    crate::dialogs::confirm(&mut app, id).unwrap();
    assert_eq!(effective_shortcut(&app, "layer.new.layer", None).as_deref(), Some("Cmd+O"));
    assert_eq!(effective_shortcut(&app, "file.open", Some("Cmd+O")), None, "taken from File › Open");
    // Menu items show the effective shortcut.
    let items = crate::menus::menu_items(&app);
    assert!(items.iter().any(|i| i.id == "layer.new.layer" && i.shortcut.as_deref() == Some("Cmd+O")));
    assert!(items.iter().any(|i| i.id == "file.open" && i.shortcut.is_none()));
    assert_eq!(shortcut_text(egui::Key::K, egui::Modifiers { command: true, shift: true, ..Default::default() }).as_deref(), Some("Cmd+Shift+K"));
}

#[test]
fn hidden_menu_items_disappear() {
    let (mut app, _) = app_with_store();
    app.run("edit.menus", json!({"hide": ["edit.fade"]})).unwrap();
    assert!(!crate::menus::menu_items(&app).iter().any(|i| i.id == "edit.fade"));
}

#[test]
fn autosave_runs_for_dirty_documents() {
    type Saved = Arc<Mutex<Vec<(u64, u64)>>>;
    let saved: Saved = Arc::default();
    let s2 = saved.clone();
    let services = crate::Services {
        autosave: Some(Box::new(move |doc: &Arc<photocraft_doc::Document>, rev: u64, _path: Option<&str>| {
            s2.lock().unwrap().push((doc.id.0, rev));
            Ok(())
        })),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    let ctx = egui::Context::default();
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    tick(&mut app, &ctx);
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert!(saved.lock().unwrap().is_empty(), "clean documents are not autosaved");
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert_eq!(saved.lock().unwrap().len(), 1);
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert_eq!(saved.lock().unwrap().len(), 1, "unchanged since the last autosave");

    // Loaded copies can have the same persisted identity and dirty revision.
    let original = app.session.active().unwrap().doc.clone();
    let original_id = original.id;
    app.session.add_document(original.as_ref().clone(), None);
    app.run("edit.fill", json!({"color": "#0000ff"})).unwrap();
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    {
        let saved = saved.lock().unwrap();
        assert_eq!(saved.len(), 2, "both dirty copies need independent autosaves");
        assert_eq!(saved[0].0, original_id.0);
        assert_ne!(saved[0].0, saved[1].0, "recovery ownership must be distinct");
        assert_eq!(saved[0].1, saved[1].1, "the revisions must match to reproduce suppression");
    }
    app.run("prefs.set", json!({"path": "fileHandling.autosave", "value": false})).unwrap();
    app.run("edit.fill", json!({"color": "#00ff00"})).unwrap();
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert_eq!(saved.lock().unwrap().len(), 2, "autosave off");
}

#[test]
fn recovered_documents_adopt_their_entries_until_saved_or_closed() {
    type Log = Arc<Mutex<Vec<String>>>;
    let log: Log = Arc::default();
    let (l1, l2, l3) = (log.clone(), log.clone(), log.clone());
    use photocraft_doc::{Color, ColorMode, Document, SampleType, Size};
    let doc = || Document::with_background("R", Size::new(4, 4), ColorMode::Rgb, SampleType::U8, Color::WHITE);
    let services = crate::Services {
        autosave: Some(Box::new(move |d: &Arc<Document>, _: u64, _: Option<&str>| {
            l1.lock().unwrap().push(format!("save {}", d.id.0));
            Ok(())
        })),
        discard_autosave: Some(Box::new(move |id: u64| l2.lock().unwrap().push(format!("discard {id}")))),
        recover: Some(Box::new(move || ["a", "b"].map(|key| crate::Recovered { key: key.into(), path: None, doc: doc() }).into())),
        adopt_autosave: Some(Box::new(move |id: u64, key: &str| l3.lock().unwrap().push(format!("adopt {id} {key}")))),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    let ctx = egui::Context::default();
    let ids: Vec<u64> = app.session.documents().iter().map(|d| d.doc.id.0).collect();
    assert!(app.session.documents().iter().all(|d| d.is_dirty()), "recovered documents are unsaved");
    let take = || std::mem::take(&mut *log.lock().unwrap());
    assert_eq!(take(), [format!("adopt {} a", ids[0]), format!("adopt {} b", ids[1])]);
    // Their entries are current: nothing to autosave until they change.
    tick(&mut app, &ctx);
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert!(take().is_empty());
    // Closing one drops its entry; editing the other autosaves over its own.
    app.run("file.close", json!({"document": 0})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert_eq!(take(), [format!("discard {}", ids[0]), format!("save {}", ids[1])]);
}

#[test]
fn edit_dialogs_open() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
    app.sync_views();
    let d = crate::menus::invoke(&mut app, &ctx, "edit.colorSettings", json!({})).unwrap()["dialog"].as_u64().unwrap();
    assert_eq!(app.ui.dialogs.iter().find(|x| x.id == d).unwrap().fields["workingRgb"], "srgb");
    let note = app.ui.dialogs.iter().find(|x| x.id == d).unwrap().fields["__note"].as_str().unwrap().to_string();
    assert!(note.starts_with("Monitor profile in use: sRGB") && note.contains("fallback"), "{note}");
    assert!(crate::menus::invoke(&mut app, &ctx, "edit.fade", json!({})).is_err());
    app.run("select.rect", json!({"x": 4, "y": 4, "width": 8, "height": 8})).unwrap();
    let d = crate::menus::invoke(&mut app, &ctx, "edit.contentAwareFill", json!({})).unwrap()["dialog"].as_u64().unwrap();
    assert_eq!(app.ui.dialogs.iter().find(|x| x.id == d).unwrap().fields["__preview"], true);
    crate::dialogs::confirm(&mut app, d).unwrap();
    let d = crate::menus::invoke(&mut app, &ctx, "edit.contentAwareScale", json!({})).unwrap()["dialog"].as_u64().unwrap();
    assert_eq!(app.ui.dialogs.iter().find(|x| x.id == d).unwrap().fields["width"], 8);
    let d = crate::menus::invoke(&mut app, &ctx, "edit.presets.presetManager", json!({})).unwrap()["dialog"].as_u64().unwrap();
    assert!(crate::dialogs::confirm(&mut app, d).is_ok());
}

#[cfg(test)]
mod prefs_repair_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn repaired_preferences_are_written_back_once() {
        let stored = Arc::new(Mutex::new(Some(r#"{"interface":{"theme":"gone"},"general":{"beepWhenDone":true}}"#.to_string())));
        let read = stored.clone();
        let write = stored.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                load_prefs: Some(Box::new(move || read.lock().unwrap().clone())),
                save_prefs: Some(Box::new(move |text| {
                    *write.lock().unwrap() = Some(text.into());
                    Ok(())
                })),
                ..Default::default()
            },
        );
        load(&mut app);
        assert!(app.session.prefs().general.beep_when_done);
        assert!(persist(&mut app, 0.0).is_none());
        let text = stored.lock().unwrap().clone().unwrap();
        let mut session = photocraft_engine::Session::new();
        session.load_prefs_json(&text).unwrap();
        assert!(session.prefs().general.beep_when_done);
    }
}

#[cfg(test)]
mod tablet_compatibility_tests {
    use super::*;
    #[test]
    fn tablet_compatibility_dropdown_clicks_auto_on_off() {
        use egui_kittest::{Harness, kittest::Queryable};
        let values = json!({"tabletCompatibilityMode":"auto"}).as_object().unwrap().clone();
        let mut h = Harness::builder().with_size(egui::vec2(900.0, 200.0)).build_ui_state(
            |ui, values: &mut Map<String, Value>| {
                section_fields(ui, "tools", values, &["tabletCompatibilityMode".into()], li_i18n::Lang::En);
            },
            values,
        );
        h.run_steps(2);
        h.get_by_value("Auto").click();
        h.run_steps(2);
        h.get_by_label("On").click();
        h.run_steps(2);
        assert_eq!(h.state()["tabletCompatibilityMode"], "on");
        h.get_by_value("On").click();
        h.run_steps(2);
        h.get_by_label("Off").click();
        h.run_steps(2);
        assert_eq!(h.state()["tabletCompatibilityMode"], "off");
    }
}

#[cfg(test)]
mod display_info_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn display_info_is_read_only_and_visible_after_graphics_interaction() {
        let info = crate::gpu_canvas::GpuInfo {
            output_depth: 10,
            output_note: "native Wayland; panel depth is controlled by the compositor".into(),
            ..Default::default()
        };
        let lines = json!(info.lines());
        let mut h = Harness::builder().with_size(egui::vec2(400.0, 600.0)).build_ui_state(
            move |ui, values: &mut Map<String, Value>| {
                if super::design_system_shots::fonts_ready(ui) {
                    gpu_status_rows(ui, Some(&lines), None, values);
                }
            },
            Map::new(),
        );
        h.run_steps(2);
        h.get_by_label_contains("Display output: 10-bit client surface");
        h.get_by_label("Advanced").click();
        h.run_steps(2);
        h.get_by_value("Auto").click();
        h.run_steps(2);
        h.get_by_label("Vulkan").click();
        h.run_steps(2);
        assert_eq!(h.state()["gpuBackend"], "vulkan");
        assert!(!h.state().contains_key("outputDepth"));
        h.get_by_label_contains("Display output: 10-bit client surface");
    }
}

#[cfg(test)]
mod shifted_symbol_conflict_tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{By, Queryable},
    };

    #[test]
    fn capture_widget_transfers_shifted_symbol_conflict() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let id = open_shortcuts(&mut app, 0);
        let fields = app.ui.dialog_mut(id).unwrap().fields.clone();
        app.ui.dialogs.clear();
        let mut h = Harness::builder().with_size(vec2(900., 760.)).with_max_steps(64).build_ui_state(
            |ui, (app, fields): &mut (PhotocraftApp, Map<String, Value>)| {
                let ready = egui::Id::new("j20-fonts-ready");
                if !ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
                    crate::theme::install_fonts(ui.ctx());
                    crate::theme::apply(ui.ctx(), crate::theme::ThemeKind::Pro);
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    ui.ctx().request_repaint();
                    return;
                }
                shortcuts_body(app, ui, fields);
            },
            (app, fields),
        );
        let items: Vec<HostShortcutItem> = vec![
            ("library::tool.brush".into(), "Brush".into(), vec!["Library".into(), "Tools".into()], Some("Shift+[".into())),
            ("library::view.maskOverlay".into(), "Mask overlay".into(), vec!["Library".into(), "View".into()], Some("O".into())),
        ];
        h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("host-shortcut-catalog"), items));
        h.run_steps(4);
        h.get(By::new().role(egui::accesskit::Role::ComboBox).value("Editor")).click();
        h.run();
        h.get_by_label("Library + Develop").click();
        h.run();
        let pos = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("shortcut-capture", "library::view.maskOverlay")))).unwrap().center();
        h.event(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
        for pressed in [true, false] {
            h.event(egui::Event::Key { key: egui::Key::OpenCurlyBracket, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::SHIFT });
            h.run_steps(2);
        }
        assert_eq!(h.state().1["overrides"]["library::tool.brush"], "", "the shared matcher detects Shift+[ / Shift+{{ overlap");
        assert_eq!(h.state().1["overrides"]["library::view.maskOverlay"], "Shift+OpenCurlyBracket");
        assert!(h.state().1["message"].as_str().unwrap().contains("already in use"));
        let (app, fields) = h.state_mut();
        confirm(app, fields).unwrap();
        let profiles = &app.session.prefs().shortcut_profiles;
        assert_eq!(profiles.library.custom[&profiles.library.active].overrides["library::tool.brush"], "");
        assert_eq!(profiles.editor.active, "Photoshop");
    }
}
