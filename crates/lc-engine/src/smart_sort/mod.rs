//! Smart Sort's model boundary and prepared inputs. Catalog commands never expose tract types;
//! mocks and future model families implement the same small embedding interface.

pub mod bursts;
pub mod classify;
pub mod examples;
pub mod faces_store;
pub mod people;
pub use people::{FaceTagger, PeopleEngine};
#[doc(hidden)]
pub mod mock;
pub mod plan;
mod presets;
pub mod sessions;
pub mod store;
pub mod tagsets;
pub mod tokens;

use crate::{RenderJob, Session};
use lightcraft_catalog::{MediaKind, PhotoId, Source};
use lightcraft_raster::Rgba8;
use std::sync::Arc;

pub use plan::FolderDef;
pub use presets::{Category, Sensitivity, SmartSortPrefs, SortPreset, builtin_presets};
pub use store::Store;

pub const DEFAULT_MODEL: &str = "clip-b32-laion";

pub trait Tagger: Send + Sync {
    fn model_id(&self) -> &str;
    fn dim(&self) -> usize;
    fn embed_image(&self, img: &Rgba8) -> Result<Vec<f32>, String>;
    fn embed_text(&self, text: &str) -> Result<Vec<f32>, String>;
}

pub struct ClipTagger {
    pub model: Arc<li_seg::Clip>,
    pub crops: li_seg::Crops,
}

impl Tagger for ClipTagger {
    fn model_id(&self) -> &str {
        DEFAULT_MODEL
    }
    fn dim(&self) -> usize {
        self.model.dim()
    }
    fn embed_image(&self, img: &Rgba8) -> Result<Vec<f32>, String> {
        let rgba: Vec<u8> = img.data.iter().flatten().copied().collect();
        self.model.embed_image(&rgba, img.width, img.height, self.crops).map_err(|e| format!("{e:#}"))
    }
    fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
        self.model.embed_text(text).map_err(|e| format!("{e:#}"))
    }
}

/// Text directions are reused while the dialog is open. Sensitivity changes and corrections
/// need only the cached image/text vectors, rather than repeating dozens of text-model runs.
pub struct CachedTagger {
    inner: Arc<dyn Tagger>,
    text: std::sync::Mutex<std::collections::BTreeMap<String, Vec<f32>>>,
}
impl CachedTagger {
    pub fn new(inner: Arc<dyn Tagger>) -> Self {
        Self { inner, text: Default::default() }
    }
}
impl Tagger for CachedTagger {
    fn model_id(&self) -> &str {
        self.inner.model_id()
    }
    fn dim(&self) -> usize {
        self.inner.dim()
    }
    fn embed_image(&self, img: &Rgba8) -> Result<Vec<f32>, String> {
        self.inner.embed_image(img)
    }
    fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
        if let Some(v) = self.text.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(text).cloned() {
            return Ok(v);
        }
        let vector = self.inner.embed_text(text)?;
        let mut cache = self.text.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(text.to_string(), vector.clone());
        Ok(vector)
    }
}

#[derive(Default)]
pub struct SmartSort {
    pub tagger: Option<Arc<dyn Tagger>>,
    pub store: Store,
    pub prefs: SmartSortPrefs,
    pub tag_sets: Vec<tagsets::TagSet>,
    pub people: PeopleEngine,
}

impl SmartSort {
    pub fn tagger(&mut self, models_dir: Option<&std::path::Path>) -> Result<Arc<dyn Tagger>, String> {
        if self.tagger.is_none() {
            let dir = models_dir.ok_or("Smart Sort model folder is not configured")?;
            let model = li_seg::shared_clip(dir).ok_or("Smart Sort needs its complete, verified CLIP model bundle")?;
            self.tagger = Some(Arc::new(CachedTagger::new(Arc::new(ClipTagger { model, crops: li_seg::Crops::Three }))));
        }
        self.tagger.clone().ok_or("Smart Sort model unavailable".into())
    }
}

/// Jobs are built on the session thread and can then run on a worker. Unedited raws prefer their
/// oriented embedded preview; render jobs use full-frame developed settings and smart previews.
pub struct InputJob {
    pub id: PhotoId,
    pub key: String,
    embedded: Option<(String, crate::media::PreviewLoader)>,
    render: RenderJob,
    fallback: Option<RenderJob>,
}

impl InputJob {
    pub fn run(self) -> Result<Rgba8, String> {
        if let Some((path, loader)) = self.embedded
            && let Some(img) = loader(&path, 1024)
        {
            return Ok(img);
        }
        match self.render.run().rendered {
            Ok(r) => Ok(r.image),
            Err(e) => match self.fallback.and_then(|j| j.run().rendered.ok()) {
                Some(r) => Ok(r.image),
                None => Err(e),
            },
        }
    }
}

pub fn prepare_inputs(session: &mut Session, ids: &[PhotoId]) -> Vec<InputJob> {
    ids.iter()
        .filter_map(|id| {
            let p = session.catalog.photo(*id)?.clone();
            if p.kind == MediaKind::Video {
                return None;
            }
            let key = crate::media::content_key(&p);
            let embedded = match (&p.source, &session.media.preview_loader) {
                (Source::File { path }, Some(loader)) if p.kind == MediaKind::Raw && crate::import::has_import_look(&p) => {
                    Some((path.clone(), loader.clone()))
                }
                _ => None,
            };
            let render = session.preview_job(*id, 1024, 1024, false, &p.develop)?;
            let fallback = session.thumb_job(*id, 512);
            Some(InputJob { id: *id, key, embedded, render, fallback })
        })
        .collect()
}

#[cfg(test)]
mod tests;

#[derive(Default, serde::Serialize)]
pub struct Analysis {
    pub analysed: usize,
    pub skipped: usize,
    pub videos: usize,
    pub failed: Vec<(PhotoId, String)>,
    pub cancelled: bool,
}

/// Bounded batches keep memory small. Each batch is saved before the next; cancellation keeps
/// all completed embeddings and a later run skips them. At most half the CPU cores decode/run.
pub fn analyze(s: &mut Session, ids: &[PhotoId], cancel: &std::sync::atomic::AtomicBool, progress: &dyn Fn(usize, usize)) -> crate::Result<Analysis> {
    use rayon::prelude::*;
    use std::sync::atomic::Ordering;
    let tagger = s.smart.tagger(s.quick_seg_dir.as_deref()).map_err(crate::EngineError::Other)?;
    let model = tagger.model_id().to_string();
    s.smart.store.ensure(&model, tagger.dim()).map_err(crate::EngineError::Other)?;
    let mut result = Analysis::default();
    let mut keys = std::collections::BTreeSet::new();
    let mut todo = Vec::new();
    for id in ids {
        let Some(p) = s.catalog.photo(*id) else {
            result.failed.push((*id, "unknown photo".into()));
            continue;
        };
        if p.kind == MediaKind::Video {
            result.videos += 1;
            result.skipped += 1;
            continue;
        }
        let key = crate::media::content_key(p);
        if s.smart.store.get(&model, &key).is_some() || !keys.insert(key) {
            result.skipped += 1;
            continue;
        }
        todo.push(*id);
    }
    let threads = std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1));
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().map_err(|e| crate::EngineError::Other(e.to_string()))?;
    for batch in todo.chunks(32) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let inputs = prepare_inputs(s, batch);
        let prepared: std::collections::BTreeSet<_> = inputs.iter().map(|j| j.id).collect();
        for id in batch {
            if !prepared.contains(id) {
                result.failed.push((*id, "could not prepare photo".into()));
            }
        }
        let rows: Vec<_> = pool.install(|| {
            inputs
                .into_par_iter()
                .map(|input| {
                    let (id, key) = (input.id, input.key.clone());
                    if cancel.load(Ordering::Relaxed) {
                        return (id, key, None);
                    }
                    let v = input.run().and_then(|img| tagger.embed_image(&img));
                    (id, key, Some(v))
                })
                .collect()
        });
        for (id, key, v) in rows {
            let Some(v) = v else {
                continue;
            };
            match v.and_then(|v| s.smart.store.insert(&model, key, v)) {
                Ok(()) => result.analysed += 1,
                Err(e) => result.failed.push((id, e)),
            }
        }
        s.smart.store.save().map_err(|e| crate::EngineError::Other(format!("Smart Sort cache save: {e}")))?;
        progress(result.analysed + result.skipped + result.failed.len(), ids.len());
    }
    // A prior save may have failed after embeddings were cached in memory. Even an entirely
    // skipped retry (or cancellation) must flush that pending batch.
    s.smart.store.save().map_err(|e| crate::EngineError::Other(format!("Smart Sort cache save: {e}")))?;
    result.cancelled = cancel.load(Ordering::Relaxed);
    Ok(result)
}

/// Run prepared jobs without borrowing a Session. The UI applies each result and periodically
/// flushes the cache, preserving completed work even when the dialog is cancelled.
pub fn run_inputs(
    inputs: Vec<InputJob>,
    tagger: Arc<dyn Tagger>,
    cancel: &std::sync::atomic::AtomicBool,
    result: &(dyn Fn(PhotoId, String, Result<Vec<f32>, String>) + Send + Sync),
) -> Result<(), String> {
    use rayon::prelude::*;
    let threads = std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1));
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().map_err(|e| e.to_string())?;
    pool.install(|| {
        inputs.into_par_iter().for_each(|input| {
            if !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                let (id, key) = (input.id, input.key.clone());
                result(id, key, input.run().and_then(|img| tagger.embed_image(&img)));
            }
        })
    });
    Ok(())
}
