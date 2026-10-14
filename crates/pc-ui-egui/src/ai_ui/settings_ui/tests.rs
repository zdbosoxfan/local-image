use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

fn harness(section: &str, size: egui::Vec2, scale: f32, theme: crate::theme::ThemeKind) -> Harness<'static, PhotocraftApp> {
    harness_with_models(section, size, scale, theme, None)
}

fn harness_with_models(
    section: &str,
    size: egui::Vec2,
    scale: f32,
    theme: crate::theme::ThemeKind,
    models: Option<std::path::PathBuf>,
) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    crate::prefs_ui::open_preferences(&mut app, section);
    let mut settings = AiSettings::default();
    settings.setup_mode = "remote".into();
    settings.comfy_host = "192.168.0.100".into();
    dialogs_mut(|d| {
        d.settings = Some(settings.clone());
        d.applied_settings = Some(settings);
        d.model_search.clear();
        d.installed_only = Some(false);
    });
    TEST_STATUS.with(|s| *s.borrow_mut() = Some(EngineStatus::default()));
    let mut initialized = false;
    let mut h = Harness::builder().with_size(size).with_pixels_per_point(scale).build_ui_state(
        move |ui, app| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), theme);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            if let Some(models) = &models {
                // The real status worker uses the process store; this offscreen
                // fixture has a scoped store. Keep its test root stable per frame.
                let mut settings = AiSettings::default();
                settings.model_directory = models.to_string_lossy().into_owned();
                li_ai::settings::publish(&settings);
            }
            crate::dialogs::show(app, ui.ctx());
            super::super::windows(app, ui.ctx());
        },
        app,
    );
    h.run_steps(5);
    h
}

#[test]
fn ai_preferences_function_labels_hide_model_details_until_manage() {
    let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness("ai.tools", vec2(1100.0, 850.0), 1.0, crate::theme::ThemeKind::ProMedium);
    h.get_by_label("Download Subject Selection");
    h.get_by_label("Download Sky Selection");
    h.get_by_label("Download AI Remove");
    assert!(h.query_by_label("Fast · BRIA RMBG-1.4").is_none());
    h.get_by_label("Manage Subject Selection").click();
    h.run_steps(4);
    h.get_by_label("Fast · BRIA RMBG-1.4");
    h.get_by_label("Manage AI Remove").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Manage AI Remove").click();
    h.run_steps(4);
    h.get_by_label("LaMa · On-device");
    reset();
    TEST_STATUS.with(|s| *s.borrow_mut() = None);
}

#[test]
fn ai_preferences_old_model_cleanup_confirms_cancels_and_preserves_active_files() {
    let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    struct Restore(AiSettings);
    impl Drop for Restore {
        fn drop(&mut self) {
            li_ai::settings::publish(&self.0);
        }
    }
    let _restore = Restore(AiSettings::load());
    let dir = std::env::temp_dir().join(format!("li-ai-cleanup-ui-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("segmentation")).unwrap();
    std::fs::write(dir.join("segmentation/u2net.onnx"), b"retired subject model").unwrap();
    std::fs::write(dir.join("segmentation/midas_v21_small_256.onnx"), b"retired depth model").unwrap();
    for file in li_seg::Group::Tagging.official().files() {
        std::fs::write(li_seg::companion_path(&dir, file.file), b"active clip file").unwrap();
    }
    let mut settings = AiSettings::default();
    settings.model_directory = dir.to_string_lossy().into_owned();
    let value = serde_json::to_value(&settings).unwrap();
    let backend = li_ai::settings::StoreBackend {
        load: std::sync::Arc::new(move || Ok(Some(value.clone()))),
        save: std::sync::Arc::new(|_| panic!("cleanup must not change preferences")),
    };
    li_ai::settings::with_store_backend(backend, || {
        li_ai::settings::publish(&settings);
        let mut h = harness_with_models("ai.tools", vec2(1100.0, 850.0), 1.0, crate::theme::ThemeKind::StudioLight, Some(dir.clone()));
        dialogs_mut(|d| {
            d.settings.as_mut().unwrap().model_directory = dir.to_string_lossy().into_owned();
            d.applied_settings = d.settings.clone();
        });
        h.run_steps(3);
        h.get_by_label("Storage and cleanup").scroll_to_me();
        h.run_steps(4);
        h.get_by_label("Storage and cleanup").click();
        h.run_steps(4);
        h.get_by_label_contains("Remove old models (").scroll_to_me();
        h.run_steps(4);
        h.get_by_label_contains("Remove old models (").click();
        h.run_steps(3);
        h.get_by_label("Remove the old models?");
        assert_eq!(li_seg::legacy_installed(&dir).len(), 2, "opening confirmation cannot delete files");
        if !li_ai::trash::available() {
            h.get_by_label_contains("The files are deleted permanently.");
        }
        // The removal modal's Cancel is above the Preferences footer's Cancel.
        h.query_all_by_label("Cancel").min_by(|a, b| a.rect().bottom().total_cmp(&b.rect().bottom())).unwrap().click();
        h.run_steps(3);
        assert!(dialogs_mut(|d| d.remove.is_none()));
        assert_eq!(li_seg::legacy_installed(&dir).len(), 2, "Cancel preserves both files");
        assert_eq!(h.state().ui.dialogs.len(), 1, "Preferences remains open");
        h.get_by_label_contains("Remove old models (").click();
        h.run_steps(3);
        h.get_by_label(if li_ai::trash::available() { "Move to Trash" } else { "Delete" }).click();
        let start = std::time::Instant::now();
        while !li_seg::legacy_installed(&dir).is_empty() && start.elapsed() < std::time::Duration::from_secs(10) {
            h.run_steps(1);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        h.run_steps(3);
        assert!(li_seg::legacy_installed(&dir).is_empty());
        for file in li_seg::Group::Tagging.official().files() {
            assert_eq!(std::fs::read(li_seg::companion_path(&dir, file.file)).unwrap(), b"active clip file");
        }
        assert!(h.query_by_label_contains("Remove old models (").is_none());
        reset();
        TEST_STATUS.with(|s| *s.borrow_mut() = None);
    });
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ai_preferences_cancel_discards_connection_and_provider_drafts() {
    let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness(CONNECTION, vec2(1100.0, 850.0), 1.0, crate::theme::ThemeKind::Studio);
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled());
    dialogs_mut(|d| {
        let draft = d.settings.as_mut().unwrap();
        draft.comfy_host = "192.168.0.57".into();
        draft.set_extra_string("hf_token", "test-draft-only");
    });
    h.run_steps(2);
    assert!(!h.get_by_label("Apply").accesskit_node().is_disabled());
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.state().ui.dialogs.is_empty());
    assert!(!changed());
    assert!(dialogs_mut(|d| d.settings.is_none() && d.applied_settings.is_none()));
    TEST_STATUS.with(|s| *s.borrow_mut() = None);
}

#[test]
fn ai_preferences_families_start_folded_and_browser_escape_preserves_preferences() {
    let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness("ai.models", vec2(1100.0, 850.0), 1.0, crate::theme::ThemeKind::StudioLight);
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Installed only");
    assert!(h.query_by_label_contains("Fast drafts and reference editing").is_none());
    h.get_by_label_contains("FLUX   ").click();
    h.run_steps(4);
    h.get_by_label_contains("Fast drafts and reference editing");
    h.get_by_label("Browse Models…").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Browse Models…").click();
    h.run_steps(4);
    assert!(crate::model_browser::is_open());
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!crate::model_browser::is_open());
    assert_eq!(h.state().ui.dialogs.len(), 1);
    reset();
    TEST_STATUS.with(|s| *s.borrow_mut() = None);
}

#[test]
fn ai_preferences_pages_fit_compact_and_normal_sizes_across_themes() {
    let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for theme in crate::theme::ThemeKind::ALL {
        for (size, scale) in [(vec2(760.0, 600.0), 1.5), (vec2(1280.0, 850.0), 1.0)] {
            for (section, _) in PAGES {
                let h = harness(section, size, scale, theme);
                for button in ["Cancel", "Apply", "OK"] {
                    assert!(
                        h.ctx.content_rect().contains_rect(h.get_by_label(button).rect()),
                        "{theme:?} {section} {scale}: {button} {:?} in {:?}",
                        h.get_by_label(button).rect(),
                        h.ctx.content_rect()
                    );
                }
                if section == CONNECTION {
                    assert!(h.ctx.content_rect().contains_rect(h.get_by_label("Find on LAN").rect()));
                }
                reset();
            }
        }
    }
    TEST_STATUS.with(|s| *s.borrow_mut() = None);
}

#[test]
fn ai_preferences_family_search_and_installed_filter_preserve_known_inventory() {
    let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let preset = catalog::presets_for(ModelId::Klein4B).next().unwrap();
    let mut st = EngineStatus::default();
    st.presets.insert(preset.id(), Ok(()));
    let found = families(" klein ", true, &st);
    assert!(found.get("FLUX").unwrap().iter().any(|(_, p)| p.id() == preset.id()));
    assert!(families("no-such-model-fixture", false, &st).is_empty());
    assert_eq!(family_name("FLUX.2 Klein"), "FLUX");
    assert_eq!(family_name("Auto-detect"), "Other installed models");
}

#[test]
fn ai_preferences_function_download_cancel_targets_the_bundle_job() {
    let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = super::super::tests::temp_dir("function-download-cancel");
    let cleanup = dir.clone();
    let spec = li_seg::Group::Faces.official();
    let control = JobControl::default();
    let observed = control.clone();
    let downloads = BTreeMap::from([(format!("seg:{}", spec.id), Download { total: 1000, done: 250, ctl: control, ..Default::default() })]);
    let mut h = Harness::builder().with_size(vec2(700.0, 300.0)).build_ui(move |ui| {
        if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("semibold".into()))) {
            return;
        }
        tool_row(ui, &Tokens::get(ui.ctx()), &downloads, &dir, "Face Recognition", "Group people in your library.", "This computer · CPU", spec, false);
    });
    PhotocraftApp::setup_context(&h.ctx, Default::default());
    h.run_steps(3);
    h.get_by_label("Cancel Face Recognition download").click();
    h.run_steps(2);
    assert!(observed.is_cancelled());
    std::fs::remove_dir_all(cleanup).unwrap();
}
