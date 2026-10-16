//! Catalog safety controls shared by Preferences and its headless click tests.
use crate::{LightcraftApp, widgets};
use lightcraft_engine::library_safety::BackupFrequency;
use serde_json::json;
pub fn settings(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    ui.label(li_i18n::library::t("Catalog backup on quit"));
    if let Some(error) = &app.session.backup_preferences.last_error {
        let t = crate::theme::Tokens::get(ui.ctx());
        ui.label(egui::RichText::new(format!("{}: {error}", li_i18n::library::t("Last catalog backup failed"))).color(t.caution));
    }
    let mut prefs = app.session.backup_preferences.clone();
    let mut changed = false;
    ui.add_enabled_ui(app.session.library.as_ref().is_some_and(|l| l.on_disk), |ui| {
        ui.horizontal_wrapped(|ui| {
            changed |= widgets::choice(
                ui,
                "backupFrequency",
                &mut prefs.frequency,
                &[
                    (BackupFrequency::Never, "Never"),
                    (BackupFrequency::EveryQuit, "Every quit"),
                    (BackupFrequency::Daily, "Daily"),
                    (BackupFrequency::Weekly, "Weekly"),
                ],
            );
            ui.label(li_i18n::library::t("Keep"));
            changed |= widgets::choice(ui, "backupKeep", &mut prefs.keep, &[(1, "1"), (3, "3"), (5, "5"), (10, "10"), (20, "20")]);
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(if prefs.folder.is_empty() { li_i18n::library::t("Library Backups folder") } else { &prefs.folder });
            ui.add_enabled_ui(app.services.has_folder_picker(), |ui| {
                if widgets::text_button(ui, "backupFolder", "Choose…", false).clicked()
                    && let Some(folder) = app.services.choose_folder("Choose Catalog Backup Folder")
                {
                    prefs.folder = folder;
                    changed = true;
                }
            })
            .response
            .on_disabled_hover_text(li_i18n::library::t("Folder picker is not available here"));
            if widgets::text_button(ui, "backupDefault", "Library", false).clicked() {
                prefs.folder.clear();
                changed = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (id, label, command) in
                [("verifyLibrary", "Verify / Optimize library", "library.verifyOptimize"), ("backupNow", "Back up now", "library.backupNow")]
            {
                if widgets::text_button(ui, id, label, false).clicked() {
                    match app.run(command, json!({})) {
                        Ok(_) => app.toast(ui.ctx(), li_i18n::library::t("Library safety check completed").to_string()),
                        Err(e) => app.toast_error(ui.ctx(), e),
                    }
                }
            }
        });
    })
    .response
    .on_disabled_hover_text(li_i18n::library::t("Open a library on disk first"));
    if changed {
        let value = serde_json::to_value(prefs).unwrap_or_default();
        if let Err(e) = app.run("library.backupPreferences", value) {
            app.toast_error(ui.ctx(), e);
        }
    }
    ui.label(li_i18n::library::t("Catalog backups contain edits and metadata. Back up original photos separately."));
}
