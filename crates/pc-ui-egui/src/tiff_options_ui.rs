//! TIFF Options: the prompt File › Save As shows before writing a layered TIFF, as Photoshop
//! does when "Ask Before Saving Layered TIFF Files" is on in Preferences › File Handling.
//!
//! The save is parked in [`Prompt`] while the modal is up (egui is immediate-mode) and written
//! once answered: with the layers (Photoshop layer data in the file), or flattened ("Discard
//! Layers and Save a Copy"). "Don't show again" clears the preference. Cancel drops the save.

use crate::{ExportSettings, PhotocraftApp};

/// A save waiting on the TIFF Options answer.
pub struct Prompt {
    pub doc: photocraft_doc::DocId,
    pub next_file: Option<std::path::PathBuf>,
    pub return_to_library: bool,
    /// Where the document goes.
    pub path: String,
    /// "Discard Layers and Save a Copy".
    pub discard_layers: bool,
    /// Clears `ask_before_saving_layered_tiff` when the save goes ahead.
    pub dont_ask_again: bool,
}

/// `true` when `path` is a TIFF name (`.tif` / `.tiff`).
pub fn is_tiff_path(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && (ext.eq_ignore_ascii_case("tif") || ext.eq_ignore_ascii_case("tiff")))
}

/// Whether saving the active document to `path` should go through the prompt: a TIFF that
/// would carry layer data, with the preference on.
pub fn wants_prompt(app: &PhotocraftApp, path: &str) -> bool {
    if !is_tiff_path(path) || !app.session.prefs().file_handling.ask_before_saving_layered_tiff {
        return false;
    }
    app.session.active().is_some_and(|st| photocraft_io::tiff_layers::would_write_layers(&st.doc))
}

/// Parks the save behind the prompt.
pub fn park(app: &mut PhotocraftApp, path: String) {
    let Some(st) = app.session.active() else { return };
    app.tiff_options = Some(Prompt { doc: st.doc.id, next_file: None, return_to_library: false, path, discard_layers: false, dont_ask_again: false });
}

fn finish(app: &mut PhotocraftApp, p: Prompt) -> Result<(), String> {
    let active = app.session.active().map(|st| st.doc.id);
    let i = app.session.documents().iter().position(|st| st.doc.id == p.doc).ok_or("document was closed")?;
    app.session.set_active(i);
    let settings = ExportSettings { tiff_layers: !p.discard_layers, ..Default::default() };
    let saves_before = app.library_saves.len();
    let result = app.write_document(p.path, &settings).map(|_| ());
    if result.is_ok()
        && let Some(pending) = app.pending_save.as_mut().filter(|pending| pending.doc == p.doc)
    {
        pending.return_to_library = p.return_to_library;
        pending.tiff_dont_ask = p.dont_ask_again;
        if active == Some(p.doc) {
            pending.next_file = p.next_file;
        }
        if let Some(index) = active.and_then(|id| app.session.documents().iter().position(|state| state.doc.id == id)) {
            app.session.set_active(index);
            app.sync_views();
        }
        return result;
    }
    if result.is_ok() && p.dont_ask_again {
        app.session.edit_prefs(|prefs| prefs.file_handling.ask_before_saving_layered_tiff = false);
    }
    if let Some(i) = active.and_then(|id| app.session.documents().iter().position(|st| st.doc.id == id)) {
        app.session.set_active(i);
        app.sync_views();
    }
    if result.is_ok()
        && p.return_to_library
        && let Some(save) = app.library_saves.get_mut(saves_before)
    {
        save.show = true;
    }
    if result.is_ok()
        && active == Some(p.doc)
        && let Some(file) = p.next_file
    {
        crate::filmstrip_ui::saved_and_next(app, &file);
    }
    result
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(p) = &mut app.tiff_options else { return };
    let (mut ok, mut cancel) = (false, false);
    let (mut discard, mut dont_ask) = (p.discard_layers, p.dont_ask_again);
    let modal = egui::Modal::new(egui::Id::new("tiff-options")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.label(egui::RichText::new(tl!("TIFF Options")).font(crate::theme::semibold(15.0)));
        ui.add_space(4.0);
        crate::widgets::hairline(ui);
        ui.add_space(8.0);
        ui.label(tl!("Including layers will increase file size."));
        ui.add_space(8.0);
        crate::widgets::checkbox(ui, &mut discard, tl!("Discard Layers and Save a Copy"));
        crate::widgets::checkbox(ui, &mut dont_ask, tl!("Don't show again"));
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ok = crate::widgets::primary_button(ui, tl!("OK"), 84.0).clicked();
            cancel = crate::widgets::secondary_button(ui, tl!("Cancel"), 84.0).clicked();
        });
    });
    cancel |= modal.should_close();
    p.discard_layers = discard;
    p.dont_ask_again = dont_ask;
    if ok || ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        let Some(p) = app.tiff_options.take() else { return };
        if let Err(e) = finish(app, p) {
            app.ui.status = li_i18n::fmt(tl!("Save failed: {error}"), &[("error", &e)]);
            app.ui.status_error = true;
        }
    } else if cancel {
        app.tiff_options = None;
        app.ui.status = tl!("Save cancelled").into();
        app.ui.status_error = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiff_names() {
        assert!(is_tiff_path("a.tif"));
        assert!(is_tiff_path("dir/sub/a.TIFF"));
        assert!(is_tiff_path("C:\\x\\y.tiff"));
        assert!(!is_tiff_path("a.tif.psd"));
        assert!(!is_tiff_path(".tif"));
        assert!(!is_tiff_path("tif"));
        assert!(!is_tiff_path("a.png"));
    }
}
