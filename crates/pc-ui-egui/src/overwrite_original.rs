//! Destructive Library-source save, parked behind a modal like TIFF Options.
//! The preference uses the existing serde-defaulted dialog preference map.

use crate::{PhotocraftApp, develop_layer};
use photocraft_doc::DocId;
use serde_json::json;

pub const SAVE_ID: &str = "file.saveOverOriginal";
pub const RETURN_ID: &str = "file.saveOverOriginalAndReturnToLibrary";
const PREFERENCE: &str = "file.saveOverOriginal";

pub struct Prompt {
    pub origin: develop_layer::LibraryOrigin,
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
    let origin = develop_layer::origin_of(&state.doc).ok_or("no Library original")?;
    let prompt = Prompt { origin, doc, path, photo, show_library, dont_ask_again: false };
    let skip = app.session.prefs().dialogs.get(PREFERENCE).and_then(|v| v.get("dontAskAgain")).and_then(serde_json::Value::as_bool).unwrap_or(false);
    // Photoshop originals lose their layers: always show this warning, even with Don't Ask Again.
    let layered_source = std::path::Path::new(&prompt.path).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("psd") || ext.eq_ignore_ascii_case("psb"));
    let editable = state.doc.walk().iter().any(|(_, _, layer)| layer.generation.is_some());
    if skip && !layered_source && !editable {
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
    if develop_layer::origin_of(&state.doc).as_ref() != Some(&prompt.origin) {
        return Err("the document's original changed; try again".into());
    }
    let doc = state.doc.clone();
    let quality = app.session.prefs().export.jpeg_quality.clamp(1, 100) as u8;
    let prepared = app.services.overwrite_original.as_mut().ok_or("no overwrite writer configured")?(&doc, &prompt.path, quality)?;
    let preserved = app.session.prepare_source_overwrite(&prompt.path, prepared.original).map_err(|error| error.to_string())?;
    let warnings = (prepared.commit)()?;
    preserved.publish(&mut app.session);
    crate::prefs_ui::refresh_recovery(app);
    // Keep the layered document and its history/path. A subsequent normal Save can still
    // preserve the layers as PSD; only this flattened revision has been saved to the source.
    if prompt.dont_ask_again {
        app.session.edit_prefs(|p| {
            p.dialogs.insert(PREFERENCE.into(), json!({"dontAskAgain": true}));
        });
    }
    app.library_saves.push(develop_layer::LibrarySave {
        origin: Some(prompt.origin.clone()),
        path: prompt.path.clone(),
        photo: Some(prompt.photo),
        show: prompt.show_library,
        overwritten: true,
    });
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
        if std::path::Path::new(&p.path).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("psd") || ext.eq_ignore_ascii_case("psb")) {
            ui.label(tl!("This replaces the original Photoshop document with flattened pixels. Its layers will be lost."));
        }
        ui.label(tl!("The replacement contains flattened pixels. Keep a layered project to retain masks, smart objects and AI generation settings; your open document remains editable."));
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
    fn overwrite_preservation_refreshes_recovery_without_an_edit_revision() {
        use photocraft_doc::{LayerContent, SmartSource};
        use photocraft_format::autosave::{Autosaver, list_recovery, recover};
        let dir = std::env::temp_dir().join(format!("li-overwrite-recovery-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("photo.png");
        image::RgbImage::from_pixel(4, 3, image::Rgb([35u8, 80, 120])).save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let recovery_dir = dir.join("recovery");
        let saver = Arc::new(Mutex::new(Some(Autosaver::new(&recovery_dir, "source"))));
        let sink = saver.clone();
        let requests = Arc::new(Mutex::new(0));
        let count = requests.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                autosave: Some(Box::new(move |doc, revision, path| {
                    *count.lock().unwrap() += 1;
                    sink.lock().unwrap().as_ref().unwrap().request(doc.clone(), revision, path.map(str::to_owned), Default::default());
                    Ok(())
                })),
                overwrite_original: Some(Box::new(|_, path, _| {
                    let original = Arc::new(std::fs::read(path).unwrap());
                    let path = path.to_owned();
                    Ok(crate::PreparedOverwrite {
                        original,
                        commit: Box::new(move || {
                            image::RgbImage::from_pixel(4, 3, image::Rgb([200u8, 20, 10])).save(path).map_err(|e| e.to_string())?;
                            Ok(vec![])
                        }),
                    })
                })),
                ..Default::default()
            },
        );
        app.host_modes = true;
        develop_layer::open_photo_in(&mut app, "catalog-a", 7, &path.to_string_lossy(), &json!({}), "photo.png").unwrap();
        app.run("layer.new.layer", json!({"name":"retouch"})).unwrap();
        app.session.edit_prefs(|p| p.file_handling.autosave = true);
        crate::prefs_ui::refresh_recovery(&mut app);
        assert_eq!(*requests.lock().unwrap(), 1);
        // Complete the old recovery copy, then enqueue another old snapshot in the same
        // writer before overwrite to exercise serial/coalesced native requests.
        assert!(saver.lock().unwrap().take().unwrap().flush().unwrap().is_ok());
        let entries = list_recovery(&recovery_dir);
        assert_eq!(entries.len(), 1);
        let before = recover(&entries[0]).unwrap();
        assert!(matches!(&before.layers[0].content, LayerContent::Smart(smart) if matches!(&smart.source,SmartSource::Linked{..})));
        let writer = Autosaver::new(&recovery_dir, "source");
        let state = app.session.active().unwrap();
        let (revision, saved_revision, id) = (state.revision, state.saved_revision, state.doc.id);
        writer.request(state.doc.clone(), revision, None, Default::default());
        *saver.lock().unwrap() = Some(writer);
        request(&mut app, id, false).unwrap();
        let prompt = app.overwrite_prompt.take().unwrap();
        save(&mut app, prompt).unwrap();
        app.session.acknowledge_library_overwrite("catalog-a", 7, &path.to_string_lossy(), &json!({}));
        crate::prefs_ui::refresh_recovery(&mut app);
        assert_eq!(*requests.lock().unwrap(), 3, "both freeze and acknowledgement queue changed snapshots");
        let state = app.session.active().unwrap();
        assert_eq!((state.revision, state.saved_revision), (revision, saved_revision));
        assert!(state.is_dirty());
        assert!(saver.lock().unwrap().take().unwrap().flush().unwrap().is_ok());
        let entries = list_recovery(&recovery_dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].info.revision, revision);
        let restored = recover(&entries[0]).unwrap();
        let LayerContent::Smart(smart) = &restored.layers[0].content else { panic!("restored source") };
        assert!(matches!(&smart.source,SmartSource::Embedded{bytes,..} if bytes.as_ref()==&original));
        assert_eq!(smart.develop.as_ref().unwrap().library_baseline.as_deref(), Some(&json!({})));
        crate::prefs_ui::refresh_recovery(&mut app);
        assert_eq!(*requests.lock().unwrap(), 3, "unchanged snapshot is not saved repeatedly");
        assert_ne!(std::fs::read(path).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }

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
                        let original = Arc::new(std::fs::read(path).unwrap());
                        let path = path.to_string();
                        let sink = sink.clone();
                        Ok(crate::PreparedOverwrite {
                            original,
                            commit: Box::new(move || {
                                sink.lock().unwrap().push((path, q));
                                Ok(vec![])
                            }),
                        })
                    })),
                    ..Default::default()
                },
            );
            app.host_modes = true;
            develop_layer::open_photo(&mut app, 7, &path.to_string_lossy(), &json!({}), "photo.png").unwrap();
            app.run("layer.new.layer", json!({"name":"unsaved edit"})).unwrap();
            let saved_revision = app.session.active().unwrap().saved_revision;
            assert_ne!(saved_revision, app.session.active().unwrap().revision);
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
            assert_eq!(h.state().session.active().unwrap().saved_revision, saved_revision);
            assert_ne!(h.state().session.active().unwrap().saved_revision, h.state().session.active().unwrap().revision);
            let save = &h.state().library_saves[0];
            assert_eq!(save.show, command == RETURN_ID);
            assert!(save.overwritten);
            assert_eq!(saves.lock().unwrap()[0], (path.to_string_lossy().into(), 71));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
