//! One native document save at a time: immutable encoding, staged bytes, guarded publication.
use crate::{ExportSettings, PhotocraftApp, PreparedSave, develop_layer, notices};
use photocraft_doc::{DocId, Document};
use photocraft_engine::{
    EngineError,
    jobs::{JobEvent, JobId, JobOutcome, Started},
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

struct Alive(Arc<AtomicBool>);
impl Drop for Alive {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
struct Saved {
    snapshot: Arc<Document>,
    warnings: Vec<String>,
    unchanged: bool,
}
pub(crate) struct Pending {
    pub doc: DocId,
    pub return_to_library: bool,
    pub next_file: Option<PathBuf>,
    pub tiff_dont_ask: bool,
    job: JobId,
    path: String,
    preserves: bool,
    origin: Option<develop_layer::LibraryOrigin>,
    photo: Option<u64>,
    source_reset: Option<develop_layer::LibrarySaveSource>,
    alive: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    terminal: bool,
    receiver: Receiver<Saved>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn pending(app: &PhotocraftApp, doc: DocId) -> bool {
    app.pending_save.as_ref().is_some_and(|p| p.doc == doc)
}
pub(crate) fn enabled(app: &PhotocraftApp) -> bool {
    app.background_jobs && !app.automation_input && !cfg!(target_arch = "wasm32") && app.services.prepare_save.is_some()
}
pub(crate) fn preserves(path: &str, settings: &ExportSettings) -> bool {
    photocraft_engine::file_cmds::saves_in_place(path) || crate::tiff_options_ui::is_tiff_path(path) && settings.tiff_layers
}

pub(crate) fn begin(app: &mut PhotocraftApp, path: String, settings: &ExportSettings) -> Result<(String, Vec<String>), String> {
    if app.pending_save.is_some() {
        return Err("A save is already running; wait for it to finish or finish cancelling".into());
    }
    let prepare = app.services.prepare_save.clone().ok_or("background save service unavailable")?;
    let state = app.session.active().ok_or("no document")?;
    let (doc, revision, before_path, snapshot) = (state.doc.id, state.revision, state.path.clone(), state.doc.clone());
    let preserves = preserves(&path, settings);
    let mut origin = develop_layer::active_origin(app);
    let mut photo = develop_layer::active_photo(app);
    let source_reset = source_reset(app, &path, origin.as_ref())?;
    if let Some(reset) = &source_reset {
        origin = Some(reset.origin.clone());
        photo = Some(reset.origin.photo);
    }
    let worker_reset = source_reset.clone();
    let apply_reset = source_reset.clone();
    let sources = app.library_save_sources.clone();
    let params = json!({"path":path,"document":doc.0});
    if let Some(gate) = app.session.authorize {
        gate("file.saveAs", &params).map_err(|e| e.to_string())?;
    }
    let (tx, receiver) = mpsc::sync_channel(1);
    let alive = Arc::new(AtomicBool::new(true));
    let guard = Alive(alive.clone());
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let apply_cancel = cancel.clone();
    let worker_path = path.clone();
    let saved_path = path.clone();
    let settings = settings.clone();
    let started = app
        .session
        .start_job(
            "file.saveAs",
            params.clone(),
            "Saving document",
            false,
            move |ctx| {
                let _guard = guard;
                let cancelled = || ctx.is_cancelled() || worker_cancel.load(Ordering::Relaxed);
                let progress = |p| ctx.report(p, if p < 0.8 { "Encoding document" } else { "Writing temporary save" });
                let interrupt = lightcraft_sysmem::jobs::Interrupt::new(&cancelled, &progress);
                ctx.check()?;
                let snapshot = reset_snapshot(snapshot, worker_reset.as_ref());
                let prepared = prepare(snapshot, &worker_path, &settings, &interrupt)
                    .map_err(|e| if cancelled() { EngineError::Cancelled } else { EngineError::Other(e) })?;
                if cancelled() {
                    return Err(EngineError::Cancelled);
                }
                Ok(prepared)
            },
            move |session, prepared: PreparedSave| {
                if apply_cancel.load(Ordering::Relaxed) {
                    return Err(EngineError::Cancelled);
                }
                if let Some(gate) = session.authorize {
                    gate("file.saveAs", &params)?;
                }
                let index = session.documents().iter().position(|s| s.doc.id == doc).ok_or_else(|| EngineError::Other("Save document was closed".into()))?;
                if session.documents()[index].path != before_path {
                    return Err(EngineError::Other("Document save path changed; save again".into()));
                }
                check_source_reset(&sources, apply_reset.as_ref()).map_err(EngineError::Other)?;
                // Prepare against today's session/history, never an old whole-session capture.
                // The own job has already been removed by finish_job before this callback.
                let preservation = prepared.original.as_ref().map(|bytes| session.prepare_source_overwrite(&saved_path, bytes.clone())).transpose()?;
                (prepared.write.commit)().map_err(EngineError::Other)?;
                if let Some(plan) = preservation {
                    plan.publish(session);
                }
                acknowledge_reset(session, apply_reset.as_ref());
                let index = session.documents().iter().position(|s| s.doc.id == doc).ok_or_else(|| EngineError::Other("Saved document disappeared".into()))?;
                let current = &session.documents()[index];
                let content_saved =
                    current.revision == revision && photocraft_engine::group_view_cmds::matches_saved_snapshot(&current.doc, &prepared.snapshot);
                let unchanged = content_saved && !photocraft_engine::float_cmds::floating(current).is_some_and(|floating| floating.offset != (0, 0));
                if preserves {
                    let viewed = session.active_index();
                    session.set_active(index);
                    if let Some(state) = session.active_mut() {
                        state.path = Some(saved_path.clone());
                        state.saved_revision = if content_saved || state.revision != revision { revision } else { 0 };
                    }
                    if let Some(viewed) = viewed {
                        session.set_active(viewed);
                    }
                }
                let warnings = prepared.warnings;
                let result = json!({"path":saved_path,"warnings":warnings,"document":doc.0,"snapshotRevision":revision,"currentSnapshot":unchanged});
                // A closed UI receiver does not undo an already committed file.
                let _ = tx.send(Saved { snapshot: prepared.snapshot, warnings, unchanged });
                Ok(result)
            },
        )
        .map_err(|e| e.to_string())?;
    let Started::Job(job) = started else { return Err("background save did not start a worker".into()) };
    app.pending_save = Some(Pending {
        doc,
        job,
        path: path.clone(),
        preserves,
        origin,
        photo,
        source_reset,
        alive,
        cancel,
        terminal: false,
        receiver,
        return_to_library: false,
        next_file: None,
        tiff_dont_ask: false,
    });
    app.jobs.last_started = Some(job);
    app.ui.status = crate::i18n::fmt(tl!("Saving {name}…"), &[("name", &crate::file_open::display_name(&path))]);
    app.ui.status_error = false;
    Ok((path, Vec::new()))
}

pub(crate) fn tick(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(p) = app.pending_save.as_ref() else { return };
    if p.terminal && !p.alive.load(Ordering::Acquire) {
        if app.ui.status == tl!("Finishing save cancellation…") {
            app.ui.status = tl!("Save cancelled").into();
        }
        app.pending_save = None;
        return;
    }
    if !p.terminal && !app.session.documents().iter().any(|state| state.doc.id == p.doc) {
        app.session.cancel_job(p.job);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(30));
}

pub(crate) fn on_event(app: &mut PhotocraftApp, event: &JobEvent) -> bool {
    if !app.pending_save.as_ref().is_some_and(|p| p.job == event.id) {
        return false;
    }
    let Some(mut pending) = app.pending_save.take() else { return false };
    let succeeded = matches!(event.outcome, JobOutcome::Done(_));
    crate::discard_ui::save_finished(app, pending.doc, succeeded);
    match &event.outcome {
        JobOutcome::Done(_) => match pending.receiver.try_recv() {
            Ok(saved) => {
                crate::prefs_ui::refresh_recovery(app);
                complete(app, &pending, saved);
            }
            Err(_) => notices::error(app, "File was saved, but completion details were unavailable".to_string()),
        },
        JobOutcome::Failed(error) => {
            notices::post(app, tl!("Save failed"), vec![pending.path.clone(), error.clone()], true, None);
            app.ui.status = crate::i18n::fmt(tl!("Save failed: {error}"), &[("error", error)]);
            app.ui.status_error = true;
        }
        JobOutcome::Cancelled => {
            pending.cancel.store(true, Ordering::Relaxed);
            app.ui.status = if pending.alive.load(Ordering::Acquire) { tl!("Finishing save cancellation…") } else { tl!("Save cancelled") }.into();
            app.ui.status_error = false;
        }
    }
    // Some encoders have indivisible phases. Keep the save slot reserved until the actual
    // worker has returned, even though the engine has acknowledged its cancellation.
    if pending.alive.load(Ordering::Acquire) {
        pending.terminal = true;
        app.pending_save = Some(pending);
    }
    true
}
fn complete(app: &mut PhotocraftApp, pending: &Pending, mut saved: Saved) {
    let viewed = app.session.active().map(|state| state.doc.id);
    if pending.tiff_dont_ask {
        app.session.edit_prefs(|prefs| prefs.file_handling.ask_before_saving_layered_tiff = false);
    }
    let mut detail = if pending.preserves { format!("Saved {}", pending.path) } else { format!("Saved a copy to {}", pending.path) };
    if pending.preserves
        && let Some(index) = app.session.documents().iter().position(|state| state.doc.id == pending.doc)
    {
        app.session.set_active(index);
        photocraft_engine::automate_cmds::fire_event(&mut app.session, "saveDocument");
        if app.session.documents().iter().any(|state| state.doc.id == pending.doc) && app.session.file_menu.image_assets.contains(&pending.doc) {
            // Configured scripts/assets are completion work; assets use the bytes' snapshot,
            // not edits made during encoding or by the Save Document callback.
            let dir = photocraft_engine::web_cmds::assets_dir(&pending.path);
            let (files, errors) = photocraft_engine::web_cmds::generate_assets(&saved.snapshot, &dir);
            detail = format!("Saved {}; {} image assets in {dir}", pending.path, files.len());
            saved.warnings.extend(errors.into_iter().map(|error| format!("Image asset: {error}")));
        }
        if app.session.active().is_some_and(|state| state.doc.id == pending.doc)
            && let Some(index) = viewed.and_then(|id| app.session.documents().iter().position(|state| state.doc.id == id))
        {
            app.session.set_active(index);
        }
    }
    if !saved.unchanged {
        saved.warnings.push(tl!("Saved the captured snapshot; newer edits remain unsaved").into());
    }
    app.ui.status = detail.clone();
    app.ui.status_error = false;
    let notice = notices::file_result(app, "Saved", &pending.path, &saved.warnings);
    notices::complete_file_result(app, Some(notice), format!("Saved {}", crate::file_open::display_name(&pending.path)), detail, false);
    let still_current = !has_live_input(app)
        && app.session.active().is_some_and(|state| state.doc.id == pending.doc)
        && app.session.documents().iter().find(|state| state.doc.id == pending.doc).is_some_and(|state| {
            photocraft_engine::group_view_cmds::matches_saved_snapshot(&state.doc, &saved.snapshot)
                && !photocraft_engine::float_cmds::floating(state).is_some_and(|floating| floating.offset != (0, 0))
        });
    if app.host_modes {
        app.library_saves.push(develop_layer::LibrarySave {
            notice: Some(notice),
            origin: pending.origin.clone(),
            path: pending.path.clone(),
            photo: pending.photo,
            show: pending.return_to_library && still_current && saved.unchanged,
            overwritten: false,
            source_reset: pending.source_reset.clone(),
        });
    }
    if still_current
        && saved.unchanged
        && let Some(path) = &pending.next_file
    {
        crate::filmstrip_ui::saved_and_next(app, path);
    }
    app.sync_views();
}

pub(crate) fn response(app: &PhotocraftApp, path: String, warnings: Vec<String>) -> serde_json::Value {
    match app.pending_save.as_ref().filter(|pending| pending.path == path) {
        Some(pending) => json!({"path":path,"warnings":warnings,"job":pending.job.0,"pending":true}),
        None => json!({"path":path,"warnings":warnings}),
    }
}

pub(crate) fn owns_job(app: &PhotocraftApp, job: JobId) -> bool {
    app.pending_save.as_ref().is_some_and(|pending| pending.job == job)
}

pub(crate) fn has_live_input(app: &PhotocraftApp) -> bool {
    app.drag.is_some()
        || app.live_stroke.is_some()
        || app.move_preview.is_some()
        || app.patch_preview.is_some()
        || app.transform_preview.is_some()
        || app.adjust_preview.is_some()
        || app.style_preview.is_some()
        || app.ui.transform.is_some()
        || app.ui.pen.is_some()
        || app.ui.direct_selection.drag.is_some()
}

// Only a captured, qualified source can reset Develop. A derivative save never does.
pub(crate) fn source_reset(
    app: &PhotocraftApp,
    path: &str,
    origin: Option<&develop_layer::LibraryOrigin>,
) -> Result<Option<develop_layer::LibrarySaveSource>, String> {
    if !app.host_modes {
        return Ok(None);
    }
    let matched = app.session.active().and_then(|state| {
        state
            .doc
            .walk()
            .into_iter()
            .filter_map(|(_, _, layer)| develop_layer::layer_origin(layer))
            .find(|origin| develop_layer::same_file_path(path, &origin.path))
    });
    let Some(origin) = origin.filter(|origin| develop_layer::same_file_path(path, &origin.path)).or(matched.as_ref()) else { return Ok(None) };
    let reset = app
        .library_save_sources
        .lock()
        .map_err(|_| "Library save state unavailable".to_string())?
        .iter()
        .find(|source| &source.origin == origin)
        .cloned()
        .map(Some)
        .ok_or_else(|| "The source Library is unavailable or changed; reopen it before replacing its photo".to_string())?;
    check_source_reset(&app.library_save_sources, reset.as_ref())?;
    Ok(reset)
}
pub(crate) fn check_source_reset(sources: &develop_layer::LibrarySaveSources, reset: Option<&develop_layer::LibrarySaveSource>) -> Result<(), String> {
    if let Some(reset) = reset
        && (!sources.lock().map_err(|_| "Library save state unavailable".to_string())?.contains(reset)
            || reset.source_paths.iter().any(|path| std::fs::canonicalize(path).ok().as_ref() != Some(&reset.canonical_path)))
    {
        return Err("Library source or Develop settings changed during saving; save again to keep the newer edits".into());
    }
    Ok(())
}
fn reset_defaults() -> serde_json::Value {
    json!(photocraft_io::develop_filter::DevelopSettings::default())
}
pub(crate) fn reset_snapshot(mut doc: Arc<Document>, reset: Option<&develop_layer::LibrarySaveSource>) -> Arc<Document> {
    if let Some(reset) = reset
        && let Some(library) = &reset.origin.library
    {
        let defaults = reset_defaults();
        for (photo, _) in &reset.settings {
            doc = photocraft_engine::develop_layer_cmds::acknowledge_library_snapshot(&doc, library, *photo, &reset.origin.path, &defaults);
        }
    }
    doc
}
pub(crate) fn acknowledge_reset(session: &mut photocraft_engine::Session, reset: Option<&develop_layer::LibrarySaveSource>) {
    if let Some(reset) = reset
        && let Some(library) = &reset.origin.library
    {
        let defaults = reset_defaults();
        for (photo, _) in &reset.settings {
            session.acknowledge_library_overwrite(library, *photo, &reset.origin.path, &defaults);
        }
    }
}

#[cfg(test)]
#[path = "save_worker_tests.rs"]
mod tests;
