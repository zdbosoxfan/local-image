//! The Library's file dialogs, which never block the event loop (see [`li_ui_kit::file_dialog`]).
//!
//! A command or button asks with [`LightcraftApp::ask_file`] and hands over what to do with the
//! answer; the host shows the dialog through [`Services::file_dialog`](crate::Services::file_dialog)
//! and [`LightcraftApp::poll_file_dialog`], every frame, continues once the user has chosen. A
//! widget that only fills in a field asks with [`LightcraftApp::choose_folder`] (or
//! [`LightcraftApp::ask_into`]) and takes the choice with [`LightcraftApp::take_chosen`] when it
//! next draws. A service that answers at once (tests) continues at once. One dialog at a time.

use li_ui_kit::file_dialog::{Asked, ShowFileDialog};
pub use li_ui_kit::file_dialog::{FileDialogAnswer, FileDialogReply, answering};
use serde_json::{Value, json};

use crate::LightcraftApp;

/// What a Library file dialog asks for. The host chooses the title and file types of each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDialogRequest {
    /// Photos (Add Photos, Locate Missing File, an Export watermark graphic): answered with
    /// [`FileDialogAnswer::Paths`].
    Photos,
    /// Presets and profiles to import (`.lcpreset`, `.xmp`, `.lrtemplate`, `.zip`, `.dng`,
    /// Luminar `.lmp` / `.mplumpack`, `.cube`).
    Presets,
    /// A GPS track log (`.gpx`; Photo ▸ Auto-Tag from Tracklog…).
    Tracklog,
    /// Point-curve preset files to import (`.lccurve`).
    CurvePresets,
    /// Where to save exported presets (`.lcpreset`), suggesting `name`: answered with
    /// [`FileDialogAnswer::SaveTo`].
    SavePreset { name: String },
    /// Where to save exported point-curve presets (`.lccurve`), suggesting `name`.
    SaveCurvePresets { name: String },
    /// Where to save a denoised copy (DNG or TIFF), suggesting `name`.
    SaveDenoised { name: String },
    /// A folder; `title` (untranslated) says what for.
    Folder { title: Option<String> },
}

impl FileDialogRequest {
    pub fn folder(title: &str) -> Self {
        Self::Folder { title: Some(title.to_owned()) }
    }
}

/// Show a Library file dialog and return at once, answering through the reply when the user is
/// done.
pub type FileDialogFn = ShowFileDialog<FileDialogRequest>;

type Then = Box<dyn FnOnce(&mut LightcraftApp, FileDialogAnswer) -> Result<Value, String>>;

/// The file dialog in progress and what continues from its answer.
pub(crate) type Pending = li_ui_kit::file_dialog::Pending<Then>;

impl LightcraftApp {
    /// Whether a file dialog is open.
    pub fn file_dialog_open(&self) -> bool {
        self.file_dialog.is_some()
    }

    /// Show a file dialog; `then` continues with the user's choice. Returns `then`'s result when
    /// the answer came at once, `null` when cancelled at once or without a dialog service, else
    /// `{"fileDialog": "open" | "save" | "folder"}`. `Err` while another dialog is open.
    pub fn ask_file(
        &mut self,
        request: FileDialogRequest,
        then: impl FnOnce(&mut Self, FileDialogAnswer) -> Result<Value, String> + 'static,
    ) -> Result<Value, String> {
        if self.file_dialog.is_some() {
            return Err("a file dialog is already open".into());
        }
        let Some(show) = self.services.file_dialog.as_mut() else { return Ok(Value::Null) };
        let kind = match request {
            FileDialogRequest::Folder { .. } => "folder",
            FileDialogRequest::SavePreset { .. } | FileDialogRequest::SaveCurvePresets { .. } | FileDialogRequest::SaveDenoised { .. } => "save",
            FileDialogRequest::Photos | FileDialogRequest::Presets | FileDialogRequest::Tracklog | FileDialogRequest::CurvePresets => "open",
        };
        match li_ui_kit::file_dialog::ask(show, request, Box::new(then) as Then) {
            Asked::Answered(then, Some(answer)) => then(self, answer),
            Asked::Answered(_, None) => Ok(Value::Null),
            Asked::Waiting(pending) => {
                self.file_dialog = Some(pending);
                Ok(json!({ "fileDialog": kind }))
            }
        }
    }

    /// `next` with `r`, a command's result: at once, or, when the command is waiting on its file
    /// dialog, with the result of the action that continues from the answer (nothing follows a
    /// cancelled dialog).
    pub(crate) fn after_file_dialog(
        &mut self,
        r: Result<Value, String>,
        next: impl FnOnce(&mut Self, Result<Value, String>) -> Result<Value, String> + 'static,
    ) -> Result<Value, String> {
        let waiting = r.as_ref().is_ok_and(|v| v.get("fileDialog").is_some());
        let Some(p) = self.file_dialog.as_mut().filter(|_| waiting) else { return next(self, r) };
        let then = std::mem::replace(&mut p.then, Box::new(|_, _| Ok(Value::Null)));
        p.then = Box::new(move |app, answer| {
            let r = then(app, answer);
            next(app, r)
        });
        r
    }

    /// Ask `request` for the widget `key`, which takes the first chosen path with
    /// [`Self::take_chosen`].
    pub fn ask_into(&mut self, request: FileDialogRequest, key: impl Into<String>) {
        let key = key.into();
        if let Err(e) = self.ask_file(request, move |app, answer| {
            app.chosen = answer.into_path().map(|path| (key, path));
            Ok(Value::Null)
        }) {
            self.ui.status = e;
        }
    }

    /// Ask for a folder (`title`, untranslated, says what for) for the widget `key`.
    pub fn choose_folder(&mut self, title: &str, key: impl Into<String>) {
        self.ask_into(FileDialogRequest::folder(title), key);
    }

    /// The path chosen for the widget `key`, once.
    pub fn take_chosen(&mut self, key: &str) -> Option<String> {
        if self.chosen.as_ref().is_some_and(|(k, _)| k == key) { self.chosen.take().map(|(_, path)| path) } else { None }
    }

    /// Called every frame: continues the action that asked once the dialog is answered. Its
    /// errors are shown as a toast.
    pub fn poll_file_dialog(&mut self, ctx: &egui::Context) {
        let Some(p) = self.file_dialog.as_ref() else { return };
        let Some(answer) = p.poll(ctx) else { return };
        let Some(p) = self.file_dialog.take() else { return };
        // A widget waiting on the choice draws again.
        ctx.request_repaint();
        if let Some(answer) = answer
            && let Err(e) = (p.then)(self, answer)
        {
            self.toast_error(ctx, e);
        }
    }
}

/// Test helper: a dialog service that chooses `path` in every dialog.
#[cfg(test)]
pub(crate) fn choosing(path: impl Into<String>) -> FileDialogFn {
    let path = path.into();
    answering(move |request| {
        Some(match request {
            FileDialogRequest::SavePreset { .. } | FileDialogRequest::SaveCurvePresets { .. } | FileDialogRequest::SaveDenoised { .. } => {
                FileDialogAnswer::SaveTo(path.clone())
            }
            _ => FileDialogAnswer::Paths(vec![path.clone()]),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Open = Rc<RefCell<Vec<(FileDialogRequest, FileDialogReply)>>>;

    /// An app whose file dialogs stay open until the test answers them.
    fn app() -> (LightcraftApp, Open) {
        let open: Open = Rc::default();
        let held = open.clone();
        let services =
            crate::Services { file_dialog: Some(Box::new(move |request, reply| held.borrow_mut().push((request, reply)))), ..Default::default() };
        (LightcraftApp::new(lightcraft_engine::Session::new(), services), open)
    }

    fn answer(open: &Open, answer: Option<FileDialogAnswer>) {
        let Some((_, reply)) = open.borrow_mut().pop() else { panic!("no dialog is open") };
        std::thread::spawn(move || reply.send(answer)).join().ok();
    }

    #[test]
    fn a_command_waits_for_its_dialog_and_continues_with_the_choice() {
        let (mut app, open) = app();
        let ctx = egui::Context::default();
        let dir = std::env::temp_dir().join(format!("lc-file-dialog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        assert_eq!(app.run("file.addFolder", json!({})), Ok(json!({"fileDialog": "folder"})));
        assert!(matches!(open.borrow().as_slice(), [(FileDialogRequest::Folder { title: Some(t) }, _)] if t == "Choose Import Source"));
        // Frames go on while the user is in the dialog, and a second dialog is refused.
        app.poll_file_dialog(&ctx);
        assert!(app.file_dialog_open() && app.scan.is_none());
        assert_eq!(app.run("file.addPhotos", json!({})), Err("a file dialog is already open".into()));
        answer(&open, Some(FileDialogAnswer::Paths(vec![dir.to_string_lossy().into_owned()])));
        app.poll_file_dialog(&ctx);
        assert!(!app.file_dialog_open());
        assert!(app.scan.is_some(), "the import review scans the chosen folder");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_widget_takes_the_folder_chosen_for_it_once() {
        let (mut app, open) = app();
        let ctx = egui::Context::default();
        app.choose_folder("Choose Export Folder", "export");
        app.poll_file_dialog(&ctx);
        assert_eq!(app.take_chosen("export"), None);
        answer(&open, Some(FileDialogAnswer::Paths(vec!["/exports".into()])));
        app.poll_file_dialog(&ctx);
        assert_eq!(app.take_chosen("import"), None);
        assert_eq!(app.take_chosen("export").as_deref(), Some("/exports"));
        assert_eq!(app.take_chosen("export"), None);
    }

    #[test]
    fn cancelling_continues_nothing() {
        let (mut app, open) = app();
        let ctx = egui::Context::default();
        app.choose_folder("Choose Export Folder", "export");
        answer(&open, None);
        app.poll_file_dialog(&ctx);
        assert!(!app.file_dialog_open());
        assert_eq!(app.take_chosen("export"), None);
    }
}
