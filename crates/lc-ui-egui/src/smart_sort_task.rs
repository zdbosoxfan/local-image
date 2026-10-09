//! Model loading, decoding and embedding run off the UI thread. Results are applied to the
//! session on each frame; closing the dialog cancels work without discarding its cache.
use crate::{LightcraftApp, i18n::tr, state::Dialog};
use lightcraft_catalog::{MediaKind, PhotoId};
use lightcraft_engine::smart_sort::{self, Tagger};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, channel},
};

enum Event {
    Model(Arc<dyn Tagger>),
    Row(PhotoId, String, Result<Vec<f32>, String>),
    Done(Result<(), String>),
}
pub struct SmartSortTask {
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<Mutex<(usize, String)>>,
    pub total: usize,
    pub skipped: usize,
    pub videos: usize,
    model: String,
    rx: Receiver<Event>,
}
impl SmartSortTask {
    pub fn status(&self) -> Value {
        let (done, current) = self.progress.lock().map(|p| p.clone()).unwrap_or_default();
        json!({"done":done,"total":self.total,"current":current,"skipped":self.skipped,"videos":self.videos,"cancelled":self.cancel.load(Ordering::Relaxed)})
    }
}
pub fn start(app: &mut LightcraftApp, ids: &[PhotoId]) -> Result<(), String> {
    if app.smart_sort.is_some() {
        return Err(tr("Analysis is already running").to_string());
    }
    let injected = app.session.smart.tagger.clone();
    let (model, dim) = injected.as_ref().map_or((smart_sort::DEFAULT_MODEL.into(), 512), |t| (t.model_id().to_string(), t.dim()));
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
            if app.session.smart.store.get(&model, &key).is_some() || !keys.insert(key) {
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
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(Mutex::new((0, String::new())));
    let (tx, rx) = channel();
    let (c, p) = (cancel.clone(), progress.clone());
    let prepare_error = tr("Could not prepare photo").to_string();
    let work = move || {
        let tagger = injected.map(Ok).unwrap_or_else(|| smart_sort::SmartSort::default().tagger(dir.as_deref()));
        let result = tagger.and_then(|tagger| {
            let _ = tx.send(Event::Model(tagger.clone()));
            for id in missing {
                let _ = tx.send(Event::Row(id, String::new(), Err(prepare_error.clone())));
            }
            smart_sort::run_inputs(inputs, tagger, &c, &|id, key, value| {
                if let Ok(mut progress) = p.lock() {
                    progress.0 += 1;
                    progress.1 = format!("{}", id.0);
                }
                let _ = tx.send(Event::Row(id, key, value));
            })
        });
        let _ = tx.send(Event::Done(result));
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name("smart-sort".into()).spawn(work).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    work();
    app.smart_sort = Some(SmartSortTask { cancel, progress, total, skipped, videos, model, rx });
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
            Ok(Event::Row(id, key, value)) => {
                let value = value.and_then(|v| app.session.smart.store.insert(&task.model, key, v));
                if let Some(Dialog::SmartSort { state }) = &mut app.ui.dialog {
                    match value {
                        Ok(()) => state.analysed += 1,
                        Err(e) => state.failed.push((id, e)),
                    }
                }
                changed = true;
            }
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
        if let Err(e) = app.session.smart.store.save() {
            app.toast_error(ctx, e.to_string());
        }
    }
    if finished {
        if let Some(Dialog::SmartSort { state }) = &mut app.ui.dialog {
            state.skipped = task.skipped;
            state.videos = task.videos;
            state.dirty = app.session.smart.tagger.is_some();
            if succeeded && !task.cancel.load(Ordering::Relaxed) {
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
        self.cancel.store(true, Ordering::Relaxed);
    }
}
