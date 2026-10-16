//! Model loading, decoding and embedding run off the UI thread. Results are applied to the
//! session on each frame; closing the dialog cancels work without discarding its cache. The task
//! carries one `JobCtl` (`step` = photos done / total, message = the photo id in progress) that
//! the dialog and the Library's task panel read and cancel.
use crate::{LightcraftApp, state::Dialog};
use lightcraft_catalog::{MediaKind, PhotoId};
use lightcraft_engine::smart_sort::{self, Tagger};
use lightcraft_sysmem::JobCtl;
use serde_json::{Value, json};
use std::sync::{Arc, mpsc::Receiver};

type Row = (PhotoId, String, Result<(Vec<f32>, f32), String>);
type SaveBatch = lightcraft_engine::smart_sort::store::SaveBatch;
#[cfg(not(target_arch = "wasm32"))]
type EventSender = std::sync::mpsc::SyncSender<Event>;
#[cfg(target_arch = "wasm32")]
type EventSender = std::sync::mpsc::Sender<Event>;
enum Event {
    Model(Arc<dyn Tagger>),
    Rows(Vec<Row>, Option<std::sync::mpsc::SyncSender<Option<SaveBatch>>>),
    Saved(SaveBatch, std::io::Result<()>),
    Done(Result<(), String>),
}

fn flush(sender: &EventSender, batch: SaveBatch) -> Result<(), String> {
    let result = batch.write();
    let failure = result.as_ref().err().map(|error| format!("Smart Sort cache save: {error}"));
    sender.send(Event::Saved(batch, result)).map_err(|_| "Analysis closed".to_string())?;
    failure.map_or(Ok(()), Err)
}

/// Only the OS coordinator waits for UI admission/persistence. Rayon workers finish their
/// bounded batch first; no channel wait or filesystem write occupies a compute-pool worker.
fn deliver(sender: &EventSender, rows: Vec<Row>) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (saved, request) = std::sync::mpsc::sync_channel(1);
        sender.send(Event::Rows(rows, Some(saved))).map_err(|_| "Analysis closed".to_string())?;
        if let Some(batch) = request.recv().map_err(|_| "Analysis closed".to_string())? {
            flush(sender, batch)?;
        }
    }
    #[cfg(target_arch = "wasm32")]
    sender.send(Event::Rows(rows, None)).map_err(|_| "Analysis closed".to_string())?;
    Ok(())
}
pub struct SmartSortTask {
    /// Cancel and progress: `step` = photos done / total, message = the photo id in progress.
    pub job: JobCtl,
    pub total: usize,
    pub skipped: usize,
    pub videos: usize,
    model: String,
    rx: Receiver<Event>,
}
impl SmartSortTask {
    pub fn status(&self) -> Value {
        let status = self.job.status();
        json!({"done":status.step.map_or(0, |s| s.0),"total":self.total,"current":status.message,"skipped":self.skipped,"videos":self.videos,"cancelled":self.job.is_cancelled()})
    }

    /// Stop the analysis after the photos in flight; what was analysed stays cached.
    pub fn cancel(&self) {
        self.job.cancel();
    }
}
pub fn start(app: &mut LightcraftApp, ids: &[PhotoId]) -> Result<(), String> {
    if app.smart_sort.is_some() {
        return Err(li_i18n::library::t("Analysis is already running").to_string());
    }
    let (model, dim) = app.session.smart.selected();
    let injected = app.session.smart.tagger.clone();
    app.session.smart.store.ensure(&model, dim)?;
    let mut keys = std::collections::BTreeSet::new();
    let mut todo = Vec::new();
    let mut skipped = 0;
    let mut videos = 0;
    for id in ids {
        if let Some(p) = app.session.catalog.photo(*id) {
            if p.kind == MediaKind::Video {
                videos += 1;
                continue;
            }
            let key = lightcraft_engine::media::content_key(p);
            if (app.session.smart.store.get(&model, &key).is_some() && app.session.smart.store.sharpness(&key).is_some()) || !keys.insert(key) {
                skipped += 1;
            } else {
                todo.push(*id);
            }
        }
    }
    let inputs = smart_sort::prepare_inputs(&mut app.session, &todo);
    let prepared: std::collections::BTreeSet<_> = inputs.iter().map(|j| j.id).collect();
    let missing: Vec<_> = todo.into_iter().filter(|id| !prepared.contains(id)).collect();
    let total = inputs.len() + missing.len();
    let dir = app.session.quick_seg_dir.clone();
    let job = JobCtl::new();
    job.set_step(0, u32::try_from(total).unwrap_or(u32::MAX));
    #[cfg(not(target_arch = "wasm32"))]
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    #[cfg(target_arch = "wasm32")]
    let (tx, rx) = std::sync::mpsc::channel();
    let pending_save = app.session.smart.store.pending_save();
    let worker = job.clone();
    let prepare_error = li_i18n::library::t("Could not prepare photo").to_string();
    let work = move || {
        let run = || -> Result<(), String> {
            if let Some(batch) = pending_save {
                flush(&tx, batch)?;
            }
            let tagger = injected.map(Ok).unwrap_or_else(|| {
                let mut smart = smart_sort::SmartSort::default();
                smart.tagger(dir.as_deref())
            })?;
            tx.send(Event::Model(tagger.clone())).map_err(|_| "Analysis closed".to_string())?;
            for batch in missing.chunks(32) {
                let rows = batch
                    .iter()
                    .map(|id| {
                        worker.add_step(1);
                        (*id, String::new(), Err(prepare_error.clone()))
                    })
                    .collect();
                deliver(&tx, rows)?;
            }
            let mut inputs = inputs.into_iter();
            loop {
                if worker.is_cancelled() {
                    break;
                }
                let batch: Vec<_> = inputs.by_ref().take(32).collect();
                if batch.is_empty() {
                    break;
                }
                let rows = std::sync::Mutex::new(Vec::new());
                smart_sort::run_inputs(batch, tagger.clone(), worker.cancel_flag(), &|id, key, value| {
                    worker.add_step(1);
                    worker.set_message(id.0.to_string());
                    rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((id, key, value));
                })?;
                deliver(&tx, rows.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner))?;
            }
            Ok(())
        };
        let result = run();
        let _ = tx.send(Event::Done(result));
    };

    let faces = app.session.smart.prefs.faces_enabled;
    // Finish tagging setup before starting faces, so setup errors leave no worker running.
    if faces {
        crate::people_task::start(app, ids, None)?;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Err(e) = std::thread::Builder::new().name("smart-sort".into()).spawn(move || {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Background);
        work()
    }) {
        if faces {
            app.people_task = None;
        } // Drop cancels the worker we just started.
        return Err(e.to_string());
    }
    #[cfg(target_arch = "wasm32")]
    work();
    app.smart_sort = Some(SmartSortTask { job, total, skipped, videos, model, rx });
    Ok(())
}

pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(task) = app.smart_sort.take() else { return };
    let mut finished = false;
    let mut succeeded = true;
    let mut changed = false;
    loop {
        match task.rx.try_recv() {
            Ok(Event::Model(tagger)) => {
                app.session.smart.tagger = Some(tagger);
            }
            Ok(Event::Rows(rows, save)) => {
                for (id, key, value) in rows {
                    let value = value.and_then(|(v, sharp)| {
                        app.session.smart.store.set_sharpness(key.clone(), sharp);
                        app.session.smart.store.insert(&task.model, key, v)
                    });
                    if let Some(Dialog::SmartSort { state }) = &mut app.ui.dialog {
                        match value {
                            Ok(()) => state.analysed += 1,
                            Err(e) => state.failed.push((id, e)),
                        }
                    }
                }
                if let Some(save) = save {
                    // One request slot, and the coordinator is already waiting for this reply.
                    // No filesystem operation or join runs on the UI thread.
                    let _ = save.send(app.session.smart.store.pending_save());
                }
                #[cfg(target_arch = "wasm32")]
                if let Err(error) = app.session.smart.store.save() {
                    app.toast_error(ctx, error.to_string());
                }
                changed = true;
                break;
            }
            Ok(Event::Saved(batch, result)) => match result {
                Ok(()) => app.session.smart.store.acknowledge_save(&batch),
                Err(error) => app.session.smart.store.save_failed(&batch, &error),
            },
            Ok(Event::Done(result)) => {
                if let Err(e) = result {
                    succeeded = false;
                    if let Some(Dialog::SmartSort { state }) = &mut app.ui.dialog {
                        state.error = e.clone();
                    }
                    app.toast_error(ctx, e);
                }
                finished = true;
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                succeeded = false;
                finished = true;
                break;
            }
        }
    }
    if changed || finished {
        ctx.request_repaint();
    }
    if finished {
        if let Some(Dialog::SmartSort { state }) = &mut app.ui.dialog {
            state.skipped = task.skipped;
            state.videos = task.videos;
            state.dirty = app.session.smart.tagger.is_some();
            if succeeded && !task.job.is_cancelled() && app.people_task.is_none() {
                state.step = 1;
            }
        } else {
            app.session.smart.tagger = None;
        }
    } else {
        app.smart_sort = Some(task);
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

impl Drop for SmartSortTask {
    fn drop(&mut self) {
        self.job.cancel();
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod persistence_tests {
    use super::*;
    #[test]
    fn ui_poll_only_hands_off_pending_bytes_and_write_keeps_original_library_path() {
        let root = std::env::temp_dir().join(format!(
            "smart-ui-save-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let first = root.join("first");
        let second = root.join("second");
        let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
        app.session.smart.store = smart_sort::Store::new(Some(&first));
        app.session.smart.store.ensure("test", 2).unwrap();
        let (sender, rx) = std::sync::mpsc::sync_channel(1);
        app.smart_sort = Some(SmartSortTask { job: JobCtl::new(), total: 1, skipped: 0, videos: 0, model: "test".into(), rx });
        let (reply, pending) = std::sync::mpsc::sync_channel(1);
        sender.send(Event::Rows(vec![(PhotoId(1), "key".into(), Ok((vec![1.0, 0.0], 4.0)))], Some(reply))).unwrap();
        poll(&mut app, &egui::Context::default());
        let batch = pending.try_recv().unwrap().unwrap();
        assert!(!first.exists(), "poll must perform no filesystem writes");
        assert!(app.session.smart.store.pending_save().is_some(), "not clean before worker acknowledgement");
        app.smart_sort = None;
        app.session.smart.store = smart_sort::Store::new(Some(&second));
        std::thread::spawn(move || batch.write()).join().unwrap().unwrap();
        assert!(!second.exists(), "late completion cannot write into the replacement library");
        let mut loaded = smart_sort::Store::new(Some(&first));
        loaded.ensure("test", 2).unwrap();
        assert_eq!(loaded.get("test", "key"), Some(&[1.0, 0.0][..]));
        assert_eq!(loaded.sharpness("key"), Some(4.0));
        std::fs::remove_dir_all(root).unwrap();
    }
}
