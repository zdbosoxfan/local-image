//! File › Save asks before replacing the document's own file when another app changed it after
//! the document was opened from it or last saved to it (upstream PhotoCraft fbcb70b3, #6596).
//! Replace writes the document over it anyway; Cancel leaves the file and the document as they
//! are, and File › Save As keeps both.
//!
//! The prompt is an ordinary dialog in the UI state: `ui.dialog.confirm` replaces the file and
//! `ui.dialog.cancel` keeps it.

use photocraft_doc::DocId;
use serde_json::{Map, Value, json};

use crate::state::DialogKind;
use crate::{FileStamp, PhotocraftApp};

const MARK: &str = "__diskChanged";

pub fn owns(fields: &Map<String, Value>) -> bool {
    fields.contains_key(MARK)
}

/// Note how `doc`'s own file at `path` looks now that the document read or wrote it.
pub(crate) fn remember_stamp(app: &mut PhotocraftApp, doc: DocId, path: &str) {
    match stamp(app, path) {
        Some(now) => app.disk_stamps.insert(doc, (path.to_string(), now)),
        None => app.disk_stamps.remove(&doc),
    };
}

/// After `doc` wrote `path`: a save over its own file is this app's change, not another app's.
pub(crate) fn wrote(app: &mut PhotocraftApp, doc: DocId, path: &str, adopted: bool) {
    if adopted || app.disk_stamps.get(&doc).is_some_and(|(own, _)| own == path) {
        remember_stamp(app, doc, path);
    }
}

fn stamp(app: &PhotocraftApp, path: &str) -> Option<FileStamp> {
    app.services.file_stamp.as_ref().and_then(|stamp| stamp(path))
}

/// Another app changed `doc`'s own file at `path` since the document was opened from it or last
/// saved to it. A file that is gone, or one never stamped, counts as unchanged.
pub(crate) fn changed_on_disk(app: &PhotocraftApp, doc: DocId, path: &str) -> bool {
    let Some((own, then)) = app.disk_stamps.get(&doc) else { return false };
    own == path && stamp(app, path).is_some_and(|now| now != *then)
}

/// File › Save of the active document to its own file: opens the prompt instead when another app
/// changed that file; `None` saves as usual.
pub(crate) fn intercept_save(app: &mut PhotocraftApp) -> Option<Result<Value, String>> {
    let st = app.session.active()?;
    let path = st.path.clone().filter(|p| photocraft_engine::file_cmds::saves_in_place(p))?;
    let doc = st.doc.id;
    if !changed_on_disk(app, doc, &path) {
        return None;
    }
    if let Some(d) = app.ui.dialogs.iter().find(|d| owns(&d.fields)) {
        return Some(Ok(json!({"dialog": d.id, "confirmationPending": true})));
    }
    let name = crate::file_open::display_name(&path);
    let message = li_i18n::fmt(tl!("{name} was changed by another app after it was opened or saved here."), &[("name", name.as_str())]);
    let fields = json!({MARK: true, "__label": "Save", "message": message, "document": doc.0, "path": path});
    let id = crate::dialogs::open_document_dialog(app, DialogKind::Command, fields.as_object().cloned().unwrap_or_default());
    Some(Ok(json!({"dialog": id, "confirmationPending": true})))
}

pub fn body(ui: &mut egui::Ui, fields: &Map<String, Value>) {
    ui.label(fields.get("message").and_then(Value::as_str).unwrap_or_default());
    ui.add_space(6.0);
    ui.label(tl!("Replace it with this document, or cancel and use File › Save As to keep both."));
}

/// Replace: the save the prompt held back. The dialog's document target guarantees the document
/// is still the active one.
pub fn confirm(app: &mut PhotocraftApp, fields: &Map<String, Value>) -> Result<Value, String> {
    let doc = fields.get("document").and_then(Value::as_u64).map(DocId).ok_or(tl!("No document"))?;
    if app.session.active().map(|st| st.doc.id) != Some(doc) {
        return Err(tl!("No document").into());
    }
    let path = fields.get("path").and_then(Value::as_str).ok_or(tl!("No document"))?.to_string();
    app.save_as(Some(path)).map(|(p, w)| crate::save_worker::response(app, p, w))
}

#[cfg(test)]
mod tests;
