//! File dialogs that never block the event loop (upstream PhotoCraft b97a72ac, #673, #574).
//!
//! A command that needs a file asks with [`PhotocraftApp::ask_file`] (or one of its wrappers) and
//! hands over what to do with the answer. The platform shell shows the dialog through
//! [`Services::file_dialog`](crate::Services::file_dialog) and returns at once, answering later
//! through the [`FileDialogReply`] from any thread; [`PhotocraftApp::poll_file_dialog`], every
//! frame, continues the action once the answer is in (see [`li_ui_kit::file_dialog`]). UI code
//! without the app at hand (the AI settings and model rows) asks with [`ask_later`].
//!
//! A service that answers before returning (tests, a dialog that couldn't be shown) continues the
//! action straight away, so the command returns its own result. One dialog at a time: asking
//! while one is open is refused.

use std::cell::RefCell;

use photocraft_doc::DocId;
use serde_json::{Value, json};

use li_ui_kit::file_dialog::{Asked, ShowFileDialog};
pub use li_ui_kit::file_dialog::{CANCELLED, FileDialogAnswer, FileDialogReply, FileDialogSpec, Pick, answering};

use crate::PhotocraftApp;
use crate::file_open::display_name;

/// What a file dialog asks the user for. Each request has its answer kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDialogRequest {
    /// Documents to open: several for File › Open, one for Place. Answered with
    /// [`FileDialogAnswer::Paths`] (or [`FileDialogAnswer::File`] by a host that reads the file).
    Open { multiple: bool },
    /// One file a command reads (a script, notes, presets): answered with
    /// [`FileDialogAnswer::File`].
    Read,
    /// Where to write, starting from `suggested` (a file name, or a full path whose folder the
    /// dialog opens in). Answered with [`FileDialogAnswer::SaveTo`].
    Save { suggested: String },
    /// An existing folder: answered with [`FileDialogAnswer::Paths`] holding it.
    Folder,
    /// A dialog with its own title and filters (the AI, Generate and Swatches pickers), answered
    /// as its [`Pick`] says.
    Described(FileDialogSpec),
}

/// Show a file dialog and return at once, answering through the reply when the user is done.
pub type FileDialogFn = ShowFileDialog<FileDialogRequest>;

/// What to do with the answer (`None`: cancelled).
type Then = Box<dyn FnOnce(&mut PhotocraftApp, Option<FileDialogAnswer>) -> Result<Value, String>>;

/// What to do with a dialog's answer, for [`ask_later`].
type ThenLater = Box<dyn FnOnce(&mut PhotocraftApp, FileDialogAnswer) -> Result<Value, String>>;

/// The file dialog in progress.
pub(crate) type Pending = li_ui_kit::file_dialog::Pending<Then>;

const UNEXPECTED: &str = "the file dialog gave an unexpected answer";

thread_local! {
    /// The dialog asked for by [`ask_later`], shown by the next [`PhotocraftApp::poll_file_dialog`].
    static LATER: RefCell<Option<(FileDialogRequest, ThenLater)>> = const { RefCell::new(None) };
}

/// [`PhotocraftApp::ask_file`] for UI code without the app at hand: the dialog opens at the end of
/// the frame and `then` continues from its answer like any other. A second request before then
/// replaces the first.
pub(crate) fn ask_later(
    ctx: &egui::Context,
    request: FileDialogRequest,
    then: impl FnOnce(&mut PhotocraftApp, FileDialogAnswer) -> Result<Value, String> + 'static,
) {
    LATER.with(|later| *later.borrow_mut() = Some((request, Box::new(then))));
    ctx.request_repaint();
}

/// Whether `r` is the result of a command that is waiting on the file dialog it opened.
pub(crate) fn waiting(r: &Result<Value, String>) -> bool {
    r.as_ref().is_ok_and(|v| v.get("fileDialog").is_some())
}

impl PhotocraftApp {
    /// Whether a file dialog is open.
    pub fn file_dialog_open(&self) -> bool {
        self.file_dialog.is_some()
    }

    /// Show a file dialog; `then` continues with the user's choice (see the module docs). Returns
    /// `then`'s result when the answer came at once, else `{"fileDialog": "open" | "save" |
    /// "folder"}`. `Err` ("cancelled") without a dialog service or when cancelled at once, and
    /// while another dialog is open.
    pub(crate) fn ask_file(
        &mut self,
        request: FileDialogRequest,
        then: impl FnOnce(&mut Self, FileDialogAnswer) -> Result<Value, String> + 'static,
    ) -> Result<Value, String> {
        if self.file_dialog.is_some() {
            return Err("a file dialog is already open".into());
        }
        let Some(show) = self.services.file_dialog.as_mut() else { return Err(CANCELLED.into()) };
        let kind = match &request {
            FileDialogRequest::Open { .. } | FileDialogRequest::Read => "open",
            FileDialogRequest::Save { .. } => "save",
            FileDialogRequest::Folder => "folder",
            FileDialogRequest::Described(spec) => match spec.pick {
                Pick::File | Pick::Files => "open",
                Pick::Save => "save",
                Pick::Folder => "folder",
            },
        };
        let then: Then = Box::new(move |app, answer| answer.map_or_else(|| Err(CANCELLED.into()), |a| then(app, a)));
        match li_ui_kit::file_dialog::ask(show, request, then) {
            Asked::Answered(then, a) => then(self, a),
            Asked::Waiting(pending) => {
                self.file_dialog = Some(pending);
                Ok(json!({ "fileDialog": kind }))
            }
        }
    }

    /// Ask where to save: `then` gets the chosen path.
    pub(crate) fn pick_save(&mut self, suggested: &str, then: impl FnOnce(&mut Self, String) -> Result<Value, String> + 'static) -> Result<Value, String> {
        self.ask_file(FileDialogRequest::Save { suggested: suggested.to_string() }, move |app, answer| match answer {
            FileDialogAnswer::SaveTo(path) => then(app, path),
            _ => Err(UNEXPECTED.into()),
        })
    }

    /// Ask for a file a command reads (a script, notes, presets): `then` gets its name (the full
    /// path on the desktop) and bytes. A file that can't be read fails with "<file name>: <why>".
    pub(crate) fn pick_file_bytes(&mut self, then: impl FnOnce(&mut Self, String, Vec<u8>) -> Result<Value, String> + 'static) -> Result<Value, String> {
        self.ask_file(FileDialogRequest::Read, move |app, answer| {
            let (name, bytes) = read_picked(answer)?;
            then(app, name, bytes)
        })
    }

    /// Ask for an existing folder: `then` gets its path.
    pub(crate) fn pick_folder(&mut self, then: impl FnOnce(&mut Self, String) -> Result<Value, String> + 'static) -> Result<Value, String> {
        self.ask_file(FileDialogRequest::Folder, move |app, answer| match answer {
            FileDialogAnswer::Paths(paths) => then(app, paths.into_iter().next().ok_or(CANCELLED)?),
            _ => Err(UNEXPECTED.into()),
        })
    }

    /// File › Open: opens every chosen file, reporting each failure (see [`Self::open_paths`]).
    /// Cancelling opens nothing, and isn't an error.
    pub fn open_dialog_file(&mut self) -> Result<Value, String> {
        let r = self.ask_file(FileDialogRequest::Open { multiple: true }, |app, answer| {
            match answer {
                FileDialogAnswer::Paths(paths) => {
                    app.open_paths(&paths);
                }
                FileDialogAnswer::File(path, bytes) => {
                    if let Err(e) = bytes.and_then(|bytes| app.open_file(&path, &bytes)) {
                        app.open_failed(&display_name(&path), &e);
                    }
                }
                FileDialogAnswer::SaveTo(_) => return Err(UNEXPECTED.into()),
            }
            Ok(Value::Null)
        });
        match r {
            Err(e) if e == CANCELLED => Ok(Value::Null),
            r => r,
        }
    }

    /// `next` with `r`, a command's result: at once, or, when the command is waiting on its file
    /// dialog, with the result of the action that continues from the answer (which `next`'s then
    /// replaces).
    pub(crate) fn after_file_dialog(
        &mut self,
        r: Result<Value, String>,
        next: impl FnOnce(&mut Self, Result<Value, String>) -> Result<Value, String> + 'static,
    ) -> Result<Value, String> {
        let Some(p) = self.file_dialog.as_mut().filter(|_| waiting(&r)) else { return next(self, r) };
        let then = std::mem::replace(&mut p.then, Box::new(|_, _| Ok(Value::Null)));
        p.then = Box::new(move |app, answer| {
            let r = then(app, answer);
            next(app, r)
        });
        r
    }

    /// The document `id` becomes active again: an answer applies to the document it was asked
    /// for, even when another one was activated while the dialog was open. `Err` once it's closed.
    pub(crate) fn refocus(&mut self, id: DocId) -> Result<(), String> {
        let i = self.session.documents().iter().position(|d| d.doc.id == id).ok_or("no document")?;
        self.session.set_active(i);
        Ok(())
    }

    /// The active document's id, for [`Self::refocus`].
    pub(crate) fn active_doc_id(&self) -> Result<DocId, String> {
        Ok(self.session.active().ok_or("no document")?.doc.id)
    }

    /// Called every frame: shows the dialog [`ask_later`] asked for, and continues the action
    /// that asked once the dialog is answered. Its errors go to the status bar.
    pub fn poll_file_dialog(&mut self, ctx: &egui::Context) {
        if let Some((request, then)) = LATER.with(|later| later.borrow_mut().take()) {
            let r = self.ask_file(request, then);
            self.report_file_dialog_error(r);
        }
        let Some(p) = self.file_dialog.as_ref() else { return };
        let Some(answer) = p.poll(ctx) else { return };
        let Some(p) = self.file_dialog.take() else { return };
        let r = (p.then)(self, answer);
        self.report_file_dialog_error(r);
    }

    fn report_file_dialog_error(&mut self, r: Result<Value, String>) {
        if let Err(e) = r
            && e != CANCELLED
        {
            self.ui.status = e;
            self.ui.status_error = true;
        }
    }
}

/// The name and bytes of the single file an open dialog answered with.
pub(crate) fn read_picked(answer: FileDialogAnswer) -> Result<(String, Vec<u8>), String> {
    match answer {
        FileDialogAnswer::File(name, bytes) => bytes.map(|b| (name.clone(), b)).map_err(|e| format!("{}: {e}", display_name(&name))),
        FileDialogAnswer::Paths(_) | FileDialogAnswer::SaveTo(_) => Err(UNEXPECTED.into()),
    }
}

#[cfg(test)]
#[path = "file_dialog/tests.rs"]
mod tests;
