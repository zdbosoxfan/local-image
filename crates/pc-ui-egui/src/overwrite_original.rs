//! Destructive Library-source save, parked behind a modal like TIFF Options.
//! The preference uses the existing serde-defaulted dialog preference map.

use crate::{PhotocraftApp, develop_layer};
use photocraft_doc::DocId;
use serde_json::json;

pub const SAVE_ID: &str = "file.saveOverOriginal";
pub const RETURN_ID: &str = "file.saveOverOriginalAndReturnToLibrary";
const PREFERENCE: &str = "file.saveOverOriginal";

pub struct Prompt {
    pub doc: DocId,
    pub path: String,
    pub photo: u64,
    pub show_library: bool,
    pub dont_ask_again: bool,
}

/// Restrict destructive saves to formats whose writers preserve the source's identity.
pub fn disabled_reason(app: &PhotocraftApp, doc: DocId) -> Option<&'static str> {
    let Some(state) = app.session.documents().iter().find(|d| d.doc.id == doc) else { return Some("document was closed") };
    let Some((_, path)) = develop_layer::original_of(&state.doc) else { return Some(tl!("This document has no Library original")) };
    let ext = std::path::Path::new(&path).extension().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if [
        "dng", "cr2", "cr3", "nef", "nrw", "arw", "raf", "orf", "rw2", "rwl", "raw", "pef", "srw", "x3f", "3fr", "iiq", "erf", "mos", "srf", "sr2", "kdc",
        "dcr", "mef", "mrw", "fff",
    ]
    .contains(&ext.as_str())
    {
        return Some(tl!("Raw files can't be overwritten; save a copy instead"));
    }
    if !matches!(ext.as_str(), "psd" | "psb")
        && !matches!(
            photocraft_codecs::from_extension(&ext),
            Some(photocraft_codecs::Format::Jpeg | photocraft_codecs::Format::Png | photocraft_codecs::Format::Tiff | photocraft_codecs::Format::WebP)
        )
    {
        return Some(tl!("This format can't be overwritten; save a copy instead"));
    }
    if !app.host_modes || app.services.overwrite_original.is_none() {
        return Some(tl!("Overwrite requires a recoverable system Trash"));
    }
    None
}

pub fn request(app: &mut PhotocraftApp, doc: DocId, show_library: bool) -> Result<(), String> {
    if let Some(reason) = disabled_reason(app, doc) {
        return Err(reason.into());
    }
    let state = app.session.documents().iter().find(|d| d.doc.id == doc).ok_or("document was closed")?;
    let (photo, path) = develop_layer::original_of(&state.doc).ok_or("no Library original")?;
    let prompt = Prompt { doc, path, photo, show_library, dont_ask_again: false };
    let skip = app.session.prefs().dialogs.get(PREFERENCE).and_then(|v| v.get("dontAskAgain")).and_then(serde_json::Value::as_bool).unwrap_or(false);
    if skip {
        save(app, prompt)
    } else {
        app.overwrite_prompt = Some(prompt);
        Ok(())
    }
}

fn save(app: &mut PhotocraftApp, prompt: Prompt) -> Result<(), String> {
    if let Some(reason) = disabled_reason(app, prompt.doc) {
        return Err(reason.into());
    }
    let state = app.session.documents().iter().find(|d| d.doc.id == prompt.doc).ok_or("document was closed")?;
    if develop_layer::original_of(&state.doc) != Some((prompt.photo, prompt.path.clone())) {
        return Err("the document's original changed; try again".into());
    }
    let doc = state.doc.clone();
    let revision = state.revision;
    let quality = app.session.prefs().export.jpeg_quality.clamp(1, 100) as u8;
    let warnings = app.services.overwrite_original.as_mut().ok_or("no overwrite writer configured")?(&doc, &prompt.path, quality)?;
    // Keep the layered document and its history/path. A subsequent normal Save can still
    // preserve the layers as PSD; only this flattened revision has been saved to the source.
    let active = app.session.active_index();
    if let Some(index) = app.session.documents().iter().position(|d| d.doc.id == prompt.doc) {
        app.session.set_active(index);
        if let Some(state) = app.session.active_mut() {
            state.saved_revision = revision;
        }
        if let Some(index) = active {
            app.session.set_active(index);
        }
    }
    if prompt.dont_ask_again {
        app.session.edit_prefs(|p| {
            p.dialogs.insert(PREFERENCE.into(), json!({"dontAskAgain": true}));
        });
    }
    app.library_saves.push(develop_layer::LibrarySave { path: prompt.path.clone(), photo: Some(prompt.photo), show: prompt.show_library, overwritten: true });
    app.ui.status = format!("Overwrote {}; previous version kept in Trash", prompt.path);
    app.ui.status_error = false;
    crate::notices::io_warnings(app, &app.ui.status.clone(), &warnings);
    Ok(())
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(p) = &mut app.overwrite_prompt else { return };
    let name = std::path::Path::new(&p.path).file_name().unwrap_or_default().to_string_lossy();
    let (mut confirm, mut cancel) = (false, false);
    let modal = egui::Modal::new(egui::Id::new("overwrite-original-confirm")).show(ctx, |ui| {
        ui.set_max_width(440.0);
        ui.heading(format!("Overwrite {name}?"));
        ui.label(tl!("This replaces the original file. The previous version will be kept in Trash so you can recover it."));
        ui.label(&p.path);
        let checkbox = ui.checkbox(&mut p.dont_ask_again, tl!("Don't ask again"));
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("overwriteDontAsk"), checkbox.rect));
        ui.horizontal(|ui| {
            let r = ui.button(tl!("Cancel"));
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("overwriteCancel"), r.rect));
            cancel = r.clicked();
            let r = ui.button(tl!("Overwrite Original"));
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("overwriteConfirm"), r.rect));
            confirm = r.clicked();
        });
    });
    cancel |= modal.should_close();
    if cancel {
        app.overwrite_prompt = None;
    } else if confirm {
        let Some(prompt) = app.overwrite_prompt.take() else { return };
        if let Err(e) = save(app, prompt) {
            app.ui.status = format!("Couldn't overwrite the original: {e}");
            app.ui.status_error = true;
            crate::notices::post(app, app.ui.status.clone(), vec![], true, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use std::sync::{Arc, Mutex};

    #[test]
    fn overwrite_original_file_menu_clicks_confirm_cancel_and_return() {
        for command in [SAVE_ID, RETURN_ID] {
            let dir = std::env::temp_dir().join(format!("li-overwrite-menu-{}-{}", std::process::id(), command));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("photo.png");
            image::RgbImage::new(4, 3).save(&path).unwrap();
            let saves: Arc<Mutex<Vec<(String, u8)>>> = Arc::default();
            let sink = saves.clone();
            let mut app = PhotocraftApp::new(
                photocraft_engine::Session::new(),
                crate::Services {
                    overwrite_original: Some(Box::new(move |_, path, q| {
                        sink.lock().unwrap().push((path.into(), q));
                        Ok(vec![])
                    })),
                    ..Default::default()
                },
            );
            app.host_modes = true;
            develop_layer::open_photo(&mut app, 7, &path.to_string_lossy(), &json!({}), "photo.png").unwrap();
            app.session.edit_prefs(|p| p.export.jpeg_quality = 71);
            let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_ui_state(
                |ui, app| {
                    crate::menus::menu_bar(app, ui);
                    show(app, ui.ctx());
                },
                app,
            );
            PhotocraftApp::setup_context(&h.ctx, Default::default());
            let label = if command == SAVE_ID { "Save Over Original…" } else { "Save Over Original and Return to Library…" };
            for confirm in [false, true] {
                h.run_steps(3);
                h.get_by_label("File").click();
                h.run_steps(3);
                h.get_by_label(label).click();
                h.run_steps(3);
                assert!(h.state().overwrite_prompt.is_some());
                if confirm {
                    h.get_by_label("Overwrite Original").click();
                } else {
                    h.get_by_label("Cancel").click();
                }
                h.run_steps(3);
                assert!(h.state().overwrite_prompt.is_none());
                assert_eq!(saves.lock().unwrap().len(), usize::from(confirm));
            }
            let save = &h.state().library_saves[0];
            assert_eq!(save.show, command == RETURN_ID);
            assert!(save.overwritten);
            assert_eq!(saves.lock().unwrap()[0], (path.to_string_lossy().into(), 71));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
