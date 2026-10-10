//! Package 29: correlate a Library open with exactly one package 28 dialog launch.
//! File reading/decoding, tabs, progress, cancellation and errors use the existing shell.
use photocraft_doc::{DocId, LayerContent, LayerId};
use photocraft_engine::{
    Session,
    jobs::{JobEvent, JobId, JobOutcome, OPEN_JOB},
};

use crate::PhotocraftApp;

/// Validated after loading, before package 28 captures its detached preview target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub photo: u64,
    pub document: DocId,
    pub revision: u64,
    pub layer: LayerId,
}
impl Launch {
    pub fn check(&self, session: &Session) -> Result<(), String> {
        let st = session.active().ok_or_else(|| tl!("The loaded photo changed; try Vectorize again").to_string())?;
        if st.doc.id != self.document
            || st.revision != self.revision
            || st.active_layer != Some(self.layer)
            || st.selected_layers() != [self.layer]
            || !st.doc.layer(self.layer).is_some_and(|l| matches!(l.content, LayerContent::Raster(_)))
        {
            return Err(tl!("The loaded photo changed; try Vectorize again").into());
        }
        Ok(())
    }
}

struct Opening {
    photo: u64,
    job: JobId,
    previous: Vec<DocId>,
}

/// Transient shell state, not a preference or a second tracing implementation.
#[derive(Default)]
pub struct State {
    opening: Option<Opening>,
    ready: Option<Launch>,
    library_documents: Vec<DocId>,
}

impl State {
    pub fn pending(&self) -> bool {
        self.opening.is_some() || self.ready.is_some()
    }

    /// The shell's return-to-Library route must keep its browsing state for these
    /// documents, including after dialog Cancel. Do not auto-select the file's photo
    /// or replace the remembered Library view with a grid. No saved preferences.
    pub fn preserves_library_state(&self, session: &Session) -> bool {
        session.active().is_some_and(|st| self.library_documents.contains(&st.doc.id))
    }

    /// Start through the ordinary bounded file reader and photo decoder. The host
    /// must switch modules without opening the active Library photo a second time.
    pub fn open_path(&mut self, app: &mut PhotocraftApp, photo: u64, path: &str) -> Result<(), String> {
        self.begin(app, photo, |app| app.open_path(path).map(|_| ()))
    }

    /// Already-read bytes use the same decode route (also usable by offscreen tests).
    pub fn open_file(&mut self, app: &mut PhotocraftApp, photo: u64, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.begin(app, photo, |app| app.open_file(path, bytes).map(|_| ()))
    }

    fn begin(&mut self, app: &mut PhotocraftApp, photo: u64, open: impl FnOnce(&mut PhotocraftApp) -> Result<(), String>) -> Result<(), String> {
        if self.pending() {
            return Err(tl!("A photo is already opening for Vectorize").into());
        }
        self.library_documents.retain(|id| app.session.documents().iter().any(|st| st.doc.id == *id));
        let previous: Vec<_> = app.session.documents().iter().map(|st| st.doc.id).collect();
        let jobs: Vec<_> = app.jobs.opens.iter().map(|tab| tab.job).collect();
        open(app)?;
        if let Some(tab) = app.jobs.opens.iter().find(|tab| !jobs.contains(&tab.job)) {
            self.opening = Some(Opening { photo, job: tab.job, previous });
        } else {
            if let Some(st) = app.session.active().filter(|st| !previous.contains(&st.doc.id)) {
                self.library_documents.push(st.doc.id);
            }
            self.ready = Some(capture(&app.session, photo, &previous)?);
        }
        Ok(())
    }

    /// Observe every event in the existing poll batch BEFORE jobs_ui handles ANY
    /// of them. Do not poll the Session a second time. The event's document index
    /// is resolved before tab moves/open-document scripts can change it. Launch is consumed
    /// AFTER normal open completion, which also checks for those intervening edits.
    pub fn on_job_event(&mut self, app: &PhotocraftApp, event: &JobEvent) -> Result<bool, String> {
        if event.command != OPEN_JOB || !self.opening.as_ref().is_some_and(|opening| opening.job == event.id) {
            return Ok(false);
        }
        let Some(opening) = self.opening.take() else { return Ok(false) };
        if let JobOutcome::Done(value) = &event.outcome {
            let index = value.get("document").and_then(|v| v.as_u64()).and_then(|i| usize::try_from(i).ok());
            let st = index.and_then(|i| app.session.documents().get(i)).ok_or_else(|| tl!("The open job returned no photo document").to_string())?;
            if !opening.previous.contains(&st.doc.id) {
                self.library_documents.push(st.doc.id);
            }
            // A decoder success must create a document, never retarget old work.
            self.ready = Some(capture_document(st, opening.photo, &opening.previous)?);
        }
        // Failed/cancelled opens keep the Library and all existing documents as-is.
        // The ordinary jobs UI is the sole owner of their progress/error notices.
        Ok(true)
    }

    /// Exactly once. Stale targets are discarded instead of opening on another layer.
    /// On success call trace_dialog::State::open(session, prefs) immediately.
    pub fn take_launch(&mut self, session: &Session) -> Result<Option<Launch>, String> {
        let Some(launch) = self.ready.take() else { return Ok(None) };
        launch.check(session)?;
        Ok(Some(launch))
    }

    /// Closing the opening tab, leaving the module, or explicit cancellation drops
    /// only the launch and cancels the existing job. A loaded photo is retained.
    pub fn cancel(&mut self, app: &mut PhotocraftApp) {
        if let Some(opening) = self.opening.take() {
            crate::jobs_ui::cancel(app, opening.job);
        }
        self.ready = None;
    }

    pub fn opening_job(&self) -> Option<JobId> {
        self.opening.as_ref().map(|opening| opening.job)
    }
}

fn capture(session: &Session, photo: u64, previous: &[DocId]) -> Result<Launch, String> {
    let st = session.active().ok_or_else(|| tl!("The open job returned no photo document").to_string())?;
    capture_document(st, photo, previous)
}

fn capture_document(st: &photocraft_engine::DocState, photo: u64, previous: &[DocId]) -> Result<Launch, String> {
    if previous.contains(&st.doc.id) {
        return Err(tl!("The open job returned no photo document").into());
    }
    let layer = st.active_layer.filter(|id| st.doc.layer(*id).is_some_and(|l| matches!(l.content, LayerContent::Raster(_))));
    let Some(layer) = layer.filter(|layer| st.selected_layers() == [*layer]) else {
        return Err(tl!("Vectorize requires a raster photo layer").into());
    };
    Ok(Launch { photo, document: st.doc.id, revision: st.revision, layer })
}

#[cfg(test)]
#[path = "library_vectorize/tests.rs"]
mod tests;
