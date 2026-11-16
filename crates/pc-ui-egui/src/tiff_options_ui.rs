//! TIFF Options: what File › Save As and Save a Copy ask before writing a TIFF, as Photoshop's
//! TIFF Options dialog does: Image Compression (None, LZW, ZIP), Save Image Pyramid, Save
//! Transparency (offered when the document has no Background layer) and, for a document a TIFF
//! stores with its layers, "Discard Layers and Save a Copy". Photoshop's JPEG compression, Pixel
//! Order and Byte Order aren't offered: the TIFF encoder writes interleaved samples in the
//! machine's byte order without JPEG.
//!
//! With "Ask Before Saving Layered TIFF Files" on (Preferences › File Handling), a layered save
//! also notes that layers increase the file size; "Don't show again" clears the preference.
//!
//! The save is parked in [`Prompt`] while the modal is up (egui is immediate-mode) and written
//! once answered. Cancel drops the save. The compression and pyramid answer is where the next
//! TIFF save of the session starts.

use std::sync::Arc;

use photocraft_codecs::TiffCompression;
use photocraft_doc::Document;

use crate::{ExportSettings, PhotocraftApp};

/// A TIFF Options answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TiffChoice {
    pub compression: TiffCompression,
    /// Save Transparency; `None` for the default (see [`ExportSettings::tiff_transparency`]).
    pub transparency: Option<bool>,
    /// Save Image Pyramid.
    pub pyramid: bool,
    /// `Some(false)`: "Discard Layers and Save a Copy". `None` keeps what the save would do.
    pub layers: Option<bool>,
}

impl TiffChoice {
    /// `settings` writing this choice.
    pub fn apply(self, settings: ExportSettings) -> ExportSettings {
        ExportSettings {
            tiff_compression: self.compression,
            tiff_transparency: self.transparency,
            tiff_pyramid: self.pyramid,
            tiff_layers: self.layers.unwrap_or(settings.tiff_layers),
            ..settings
        }
    }
}

/// What the last TIFF Options answer of the session chose, where the next prompt starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Remembered {
    pub compression: TiffCompression,
    pub pyramid: bool,
}

/// A save waiting on the TIFF Options answer.
pub struct Prompt {
    pub doc: photocraft_doc::DocId,
    pub next_file: Option<std::path::PathBuf>,
    pub return_to_library: bool,
    /// Where the document goes.
    pub path: String,
    pub compression: TiffCompression,
    pub pyramid: bool,
    /// Save Transparency, offered when [`Self::has_transparency`].
    pub transparency: bool,
    /// The document has no Background layer, so its composite can be transparent.
    pub has_transparency: bool,
    /// The TIFF would carry the document's layers: "Discard Layers and Save a Copy" is offered.
    pub layered: bool,
    /// "Discard Layers and Save a Copy".
    pub discard_layers: bool,
    /// Show the layered-file note ("Ask Before Saving Layered TIFF Files").
    pub layer_note: bool,
    /// Clears `ask_before_saving_layered_tiff` when the save goes ahead.
    pub dont_ask_again: bool,
    /// Save a Copy: the document as it was when asked, written without taking the path or
    /// marking it saved.
    pub copy: Option<Arc<Document>>,
}

/// `true` when `path` is a TIFF name (`.tif` / `.tiff`).
pub fn is_tiff_path(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && (ext.eq_ignore_ascii_case("tif") || ext.eq_ignore_ascii_case("tiff")))
}

/// Whether saving the active document to `path` asks for TIFF Options: any TIFF.
pub fn wants_prompt(app: &PhotocraftApp, path: &str) -> bool {
    is_tiff_path(path) && app.session.active().is_some()
}

/// Parks the save of the active document to `path` behind the prompt, or of `copy` for Save a
/// Copy.
pub fn park(app: &mut PhotocraftApp, path: String, copy: Option<Arc<Document>>) {
    let Some(doc) = copy.as_deref().or_else(|| app.session.active().map(|st| &*st.doc)) else { return };
    let (id, layered) = (doc.id, photocraft_io::tiff_layers::would_write_layers(doc));
    let has_transparency = !doc.layers.first().is_some_and(|l| crate::doc_props_ui::is_background(doc, l));
    let layer_note = layered && app.session.prefs().file_handling.ask_before_saving_layered_tiff;
    let Remembered { compression, pyramid } = app.tiff_remembered;
    app.tiff_options = Some(Prompt {
        doc: id,
        next_file: None,
        return_to_library: false,
        path,
        compression,
        pyramid,
        // Our TIFF writes no alpha or spot channels, so an unset Save Transparency keeps it.
        transparency: true,
        has_transparency,
        layered,
        discard_layers: false,
        layer_note,
        dont_ask_again: false,
        copy,
    });
}

/// The answer the prompt gives as it stands.
pub fn choice(p: &Prompt) -> TiffChoice {
    TiffChoice {
        compression: p.compression,
        transparency: p.has_transparency.then_some(p.transparency),
        pyramid: p.pyramid,
        layers: p.layered.then_some(!p.discard_layers),
    }
}

fn finish(app: &mut PhotocraftApp, p: Prompt) -> Result<(), String> {
    app.tiff_remembered = Remembered { compression: p.compression, pyramid: p.pyramid };
    let dont_ask = p.layer_note && p.dont_ask_again && !p.discard_layers;
    let settings = choice(&p).apply(ExportSettings::default());
    if let Some(doc) = &p.copy {
        crate::view_cmds::write_copy(app, doc, p.path, &settings)?;
        if dont_ask {
            app.session.edit_prefs(|prefs| prefs.file_handling.ask_before_saving_layered_tiff = false);
        }
        return Ok(());
    }
    let active = app.session.active().map(|st| st.doc.id);
    let i = app.session.documents().iter().position(|st| st.doc.id == p.doc).ok_or("document was closed")?;
    app.session.set_active(i);
    let saves_before = app.library_saves.len();
    let result = app.write_document(p.path, &settings).map(|_| ());
    if result.is_ok()
        && let Some(pending) = app.pending_save.as_mut().filter(|pending| pending.doc == p.doc)
    {
        pending.return_to_library = p.return_to_library;
        pending.tiff_dont_ask = dont_ask;
        if active == Some(p.doc) {
            pending.next_file = p.next_file;
        }
        if let Some(index) = active.and_then(|id| app.session.documents().iter().position(|state| state.doc.id == id)) {
            app.session.set_active(index);
            app.sync_views();
        }
        return result;
    }
    if result.is_ok() && dont_ask {
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
    let modal = egui::Modal::new(egui::Id::new("tiff-options")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.label(egui::RichText::new(tl!("TIFF Options")).font(crate::theme::semibold(15.0)));
        ui.add_space(4.0);
        crate::widgets::hairline(ui);
        ui.add_space(8.0);
        crate::widgets::section_label(ui, tl!("Image Compression"));
        ui.radio_value(&mut p.compression, TiffCompression::None, tl!("None"));
        ui.radio_value(&mut p.compression, TiffCompression::Lzw, tl!("LZW"));
        ui.radio_value(&mut p.compression, TiffCompression::Deflate, tl!("ZIP"));
        ui.add_space(8.0);
        crate::widgets::checkbox(ui, &mut p.pyramid, tl!("Save Image Pyramid"));
        if p.has_transparency {
            crate::widgets::checkbox(ui, &mut p.transparency, tl!("Save Transparency"));
        }
        if p.layered {
            ui.add_space(8.0);
            crate::widgets::section_label(ui, tl!("Layers"));
            crate::widgets::checkbox(ui, &mut p.discard_layers, tl!("Discard Layers and Save a Copy"));
            if p.layer_note && !p.discard_layers {
                ui.add_space(4.0);
                ui.label(tl!("Including layers will increase file size."));
                crate::widgets::checkbox(ui, &mut p.dont_ask_again, tl!("Don't show again"));
            }
        }
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ok = crate::widgets::primary_button(ui, tl!("OK"), 84.0).clicked();
            cancel = crate::widgets::secondary_button(ui, tl!("Cancel"), 84.0).clicked();
        });
    });
    cancel |= modal.should_close();
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

    #[test]
    fn apply_sets_the_export_settings() {
        let base = ExportSettings::default();
        let s = TiffChoice { compression: TiffCompression::Lzw, transparency: Some(false), pyramid: true, layers: Some(false) }.apply(base.clone());
        assert_eq!((s.tiff_compression, s.tiff_transparency, s.tiff_pyramid, s.tiff_layers), (TiffCompression::Lzw, Some(false), true, false));
        // No layers answer keeps what the save would do.
        assert!(TiffChoice::default().apply(base).tiff_layers);
    }
}
