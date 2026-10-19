//! File dialogs that never block the event loop, shared by the editor and the Library (upstream
//! PhotoCraft b97a72ac, #673, #574).
//!
//! A UI asks with [`ask`], handing over the host's dialog service, its request and whatever
//! continues once the user has chosen. The host shows the dialog and returns at once, answering
//! later through the [`FileDialogReply`] from any thread; the UI polls its [`Pending`] dialog every
//! frame and continues once the answer is in. The window keeps drawing and answering the
//! compositor meanwhile. A dialog run to completion inside the frame froze the window (Linux
//! compositors offered to kill it) and, on macOS, ran AppKit's modal loop inside winit's event
//! handler, which aborts the app when another event arrives.
//!
//! A service that answers before returning (tests, a dialog that couldn't be shown) is answered
//! at once, so callers keep their synchronous fakes.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};

/// The user's choice (a cancelled dialog answers `None`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDialogAnswer {
    /// The chosen files or folder.
    Paths(Vec<String>),
    /// The chosen file's name (its full path on the desktop) and contents, or why it couldn't be
    /// read (reported like any other open failure).
    File(String, Result<Vec<u8>, String>),
    /// The path to save to.
    SaveTo(String),
}

impl FileDialogAnswer {
    /// The first chosen path, whatever the kind of answer.
    pub fn into_path(self) -> Option<String> {
        match self {
            Self::Paths(paths) => paths.into_iter().next(),
            Self::File(path, _) | Self::SaveTo(path) => Some(path),
        }
    }

    /// Every chosen path.
    pub fn into_paths(self) -> Vec<String> {
        match self {
            Self::Paths(paths) => paths,
            Self::File(path, _) | Self::SaveTo(path) => vec![path],
        }
    }
}

/// What a described dialog asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pick {
    /// One existing file: answered with [`FileDialogAnswer::Paths`] holding it.
    #[default]
    File,
    /// Existing files: [`FileDialogAnswer::Paths`].
    Files,
    /// An existing folder: [`FileDialogAnswer::Paths`] holding it.
    Folder,
    /// Where to write: [`FileDialogAnswer::SaveTo`].
    Save,
}

/// A dialog whose title, filters and default name the caller chooses (the Library's, the AI and
/// Swatches pickers). Filter extensions match in any letter case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileDialogSpec {
    pub pick: Pick,
    pub title: Option<String>,
    /// (filter name, extensions without the dot).
    pub filters: Vec<(String, Vec<String>)>,
    /// The name a save dialog suggests.
    pub file_name: Option<String>,
}

impl FileDialogSpec {
    pub fn new(pick: Pick) -> Self {
        Self { pick, ..Default::default() }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn filter(mut self, name: impl Into<String>, exts: &[&str]) -> Self {
        self.filters.push((name.into(), exts.iter().map(|e| (*e).to_owned()).collect()));
        self
    }

    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }
}

/// Hands a dialog's answer to the app and wakes it, from any thread. Dropping it unanswered (a
/// dialog that failed to show) counts as Cancel.
pub struct FileDialogReply {
    tx: Sender<Option<FileDialogAnswer>>,
    /// The context to wake, known once the app has polled (see [`Pending::poll`]).
    wake: Arc<Mutex<Option<egui::Context>>>,
}

impl FileDialogReply {
    pub fn send(self, answer: Option<FileDialogAnswer>) {
        // Nothing is waiting any more when the app has quit meanwhile.
        let _ = self.tx.send(answer);
        if let Some(ctx) = self.wake.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
            ctx.request_repaint();
        }
    }
}

/// Show the dialog `Q` asks for and return at once, answering through the reply when the user is
/// done.
pub type ShowFileDialog<Q> = Box<dyn FnMut(Q, FileDialogReply)>;

/// The error of a cancelled dialog: not reported, since backing out is the user's choice.
pub const CANCELLED: &str = "cancelled";

/// A file dialog in progress and `T`, what continues once it is answered.
pub struct Pending<T> {
    answer: Receiver<Option<FileDialogAnswer>>,
    wake: Arc<Mutex<Option<egui::Context>>>,
    pub then: T,
}

/// Where [`ask`] left a dialog.
pub enum Asked<T> {
    /// The service answered at once (`None`: cancelled, or it couldn't show the dialog).
    Answered(T, Option<FileDialogAnswer>),
    /// The user is still choosing.
    Waiting(Pending<T>),
}

/// Show the dialog for `request` through `show`, keeping `then` for the answer.
pub fn ask<Q, T>(show: &mut ShowFileDialog<Q>, request: Q, then: T) -> Asked<T> {
    let (tx, answer) = mpsc::channel();
    let wake = Arc::new(Mutex::new(None));
    show(request, FileDialogReply { tx, wake: wake.clone() });
    match answer.try_recv() {
        Ok(a) => Asked::Answered(then, a),
        Err(TryRecvError::Disconnected) => Asked::Answered(then, None),
        Err(TryRecvError::Empty) => Asked::Waiting(Pending { answer, wake, then }),
    }
}

impl<T> Pending<T> {
    /// The answer once the user is done (`Some(None)`: cancelled), `None` while the dialog is
    /// open. An answer that arrives later wakes `ctx`.
    pub fn poll(&self, ctx: &egui::Context) -> Option<Option<FileDialogAnswer>> {
        // Set before looking, so an answer that arrives after the look wakes the app.
        self.wake.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert_with(|| ctx.clone());
        match self.answer.try_recv() {
            Err(TryRecvError::Empty) => None,
            Ok(answer) => Some(answer),
            // The reply was dropped unanswered: the dialog couldn't be shown.
            Err(TryRecvError::Disconnected) => Some(None),
        }
    }
}

/// Test helper: a dialog service that answers every request at once with `answer(request)`
/// (`None`: Cancel).
#[doc(hidden)]
pub fn answering<Q: 'static>(mut answer: impl FnMut(Q) -> Option<FileDialogAnswer> + 'static) -> ShowFileDialog<Q> {
    Box::new(move |request, reply| reply.send(answer(request)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_from_another_thread_wakes_the_app_and_continues() {
        let held = std::rc::Rc::new(std::cell::RefCell::new(None));
        let keep = held.clone();
        let mut show: ShowFileDialog<&str> = Box::new(move |_, reply| *keep.borrow_mut() = Some(reply));
        let Asked::Waiting(pending) = ask(&mut show, "folder", 7) else { panic!("answered before the user chose") };
        let ctx = egui::Context::default();
        assert_eq!(pending.poll(&ctx), None);
        let reply = held.borrow_mut().take().unwrap();
        std::thread::spawn(move || reply.send(Some(FileDialogAnswer::Paths(vec!["/a".into()])))).join().unwrap();
        assert!(ctx.has_requested_repaint());
        assert_eq!(pending.poll(&ctx), Some(Some(FileDialogAnswer::Paths(vec!["/a".into()]))));
        assert_eq!(pending.then, 7);
    }

    #[test]
    fn a_reply_dropped_unanswered_counts_as_cancel() {
        let mut dropped: ShowFileDialog<()> = Box::new(|_, reply| drop(reply));
        assert!(matches!(ask(&mut dropped, (), ()), Asked::Answered((), None)));
        let held = std::rc::Rc::new(std::cell::RefCell::new(None));
        let keep = held.clone();
        let mut show: ShowFileDialog<()> = Box::new(move |_, reply| *keep.borrow_mut() = Some(reply));
        let Asked::Waiting(pending) = ask(&mut show, (), ()) else { panic!() };
        held.borrow_mut().take();
        assert_eq!(pending.poll(&egui::Context::default()), Some(None));
    }

    #[test]
    fn an_immediate_answer_continues_at_once() {
        let mut show = answering(|q: u8| Some(FileDialogAnswer::SaveTo(format!("/{q}.tif"))));
        assert!(matches!(ask(&mut show, 3, ()), Asked::Answered((), Some(FileDialogAnswer::SaveTo(p))) if p == "/3.tif"));
    }
}
