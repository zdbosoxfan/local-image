//! Background rendering: engine [`RenderJob`]s run on a [`JobPool`]; results become textures.
//!
//! Each visible thing that needs pixels is a *slot* (a grid thumbnail, the loupe, the "before"
//! image…). Requests are deduplicated per slot (a newer request replaces a queued older one) and
//! prioritised (loupe first, then on-screen thumbnails, then prefetch); thumbnails scrolled far
//! out of view are dropped from the queue. Thumbnail jobs hit the engine's preview cache (memory +
//! disk) before rendering. On wasm the jobs run inline, one per frame, unless the host installs a
//! [`RenderOffload`] (the browser build's Web Workers): then queued jobs are handed to it as it has
//! room, and its results are collected every frame.
//!
//! Stand-ins ([`QuickJob`]s, once per photo and settings): [`Slot::Preview`] holds what the loupe
//! shows until [`Slot::Main`] has the photo's render (cached view render, embedded camera JPEG or
//! a thumbnail render); [`Slot::ThumbQuick`] holds a raw's embedded preview in the grid until its
//! rendered thumbnail arrives (a cached thumbnail found by the quick job becomes the
//! [`Slot::Thumb`] texture directly).

use std::collections::HashMap;

use li_raster::Histogram;
use lightcraft_catalog::PhotoId;
use lightcraft_engine::Session;
use lightcraft_engine::media::{QuickJob, QuickSource, RenderJob, RenderResult};
use lightcraft_engine::pipeline::StageCache;
use lightcraft_pipeline::OutputSpace;
use lightcraft_preview::JobPool;
use photocraft_engine::display_color::{CanvasDisplay, rendered_display, view_space};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Bound interactive work independently of zoom, monitor resolution and the settled preview
/// preference. In particular, a 100%/200% view must never decode a full RAW for each drag step.
pub(crate) const DRAFT_EDGE: usize = 1280;

pub(crate) fn view_edge(display_edge: f32, max_edge: f32, interacting: bool) -> usize {
    let (scale, cap) = if interacting { (0.6, max_edge.min(DRAFT_EDGE as f32)) } else { (1.0, max_edge) };
    (display_edge * scale).min(cap).max(8.0) as usize
}

fn interactive_slot(slot: Slot) -> bool {
    matches!(slot, Slot::Main | Slot::Before | Slot::Hover | Slot::Second)
}

/// Slots showing a photo large enough to judge its colour: rendered in the monitor's gamut
/// ([`Renderer::set_display`]), as are the neighbours prefetched for them and the loupe stand-in
/// taken from their cached render. The loupe's 1:1 view is drawn over its render, so it is in the
/// same space. Thumbnails stay sRGB.
fn view_slot(slot: Slot) -> bool {
    interactive_slot(slot) || matches!(slot, Slot::Compare(_) | Slot::Detail)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Thumb(PhotoId),
    /// A thumbnail's stand-in (embedded preview of an unedited raw).
    ThumbQuick(PhotoId),
    Main,
    /// The loupe zoomed in past what `Main` holds: the whole frame again at the zoom's own scale
    /// (at most the photo's native pixels), drawn over `Main` while it is current. Never drafted:
    /// drags show `Main`.
    Detail,
    /// The loupe's stand-in until `Main` has the photo.
    Preview,
    Before,
    Compare(u8),
    /// Background preparation of a neighbouring photo (no texture: its decoded source and view
    /// render are cached by the engine). 0 = next, 1 = previous.
    Prefetch(u8),
    /// A variant thumbnail (profile / preset browsers), by its job key ([`Session::variant_job`]).
    /// Kept in a bounded LRU ([`VARIANT_TEXTURES`]).
    Variant(u64),
    /// The loupe while hovering a preset or profile: the photo with the hovered look (nothing is
    /// committed).
    Hover,
    /// An import candidate's thumbnail in the import review dialog (by index).
    Import(u32),
    /// The second window's view of the active photo.
    Second,
}

pub struct Tex {
    pub key: u64,
    pub photo: PhotoId,
    pub tex: egui::TextureHandle,
    pub size: [usize; 2],
    pub histogram: Option<Histogram>,
    pub ms: f64,
    /// Set for stand-ins: where the image came from.
    pub quick: Option<QuickSource>,
    /// CPU copy of the pixels (only with [`Renderer::keep_pixels`]; for headless screenshots).
    pub pixels: Option<std::sync::Arc<egui::ColorImage>>,
}

/// Runs render jobs outside this thread's job pool (the browser build: Web Workers, each with its
/// own wasm instance). The [`Renderer`] keeps the queue (priorities, per-slot de-duplication) and
/// hands jobs over one at a time.
pub trait RenderOffload {
    /// Start `job` if an executor is free; otherwise give it back (`Some`: it stays queued).
    fn try_start(&mut self, slot: Slot, job: RenderJob) -> Option<RenderJob>;
    /// Finished jobs since the last call: (slot, result, run time in ms).
    fn finished(&mut self) -> Vec<(Slot, RenderResult, f64)>;
    /// Whether these executors take the loupe's full-size renders ([`Slot::Detail`]; they need a
    /// GPU, see [`Renderer::full_size_on_gpu`]).
    fn full_size(&self) -> bool {
        false
    }
}

/// Existing native preview workers do the display conversion too. Browser inline/offload
/// collection never changes the UI thread's priority.
fn display_worker_priority() {
    #[cfg(not(target_arch = "wasm32"))]
    lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
}

struct DisplayResult {
    result: RenderResult,
    color: Option<Arc<egui::ColorImage>>,
    display_key: u64,
}

/// `result` with its image scaled down to fit a texture of at most `max_side` pixels a side
/// (a stand-in from an old cache can be larger than the GPU takes).
fn fit_texture(mut result: RenderResult, max_side: usize) -> RenderResult {
    if let Ok(r) = &mut result.rendered
        && r.image.width.max(r.image.height) > max_side
    {
        let f = r.image.map(|p| p.map(|v| f32::from(v) / 255.0));
        r.image = f.fit(max_side, max_side, li_raster::resample::Filter::Mitchell).map(|p| p.map(li_raster::quantize_u8));
    }
    result
}

/// How `slot`'s texture of `size` is sampled. The loupe's 1:1 render shows its pixels as they are
/// when magnified (past 100% each one a square of screen pixels, as Lightroom and Capture One show
/// them), unless it had to be capped at the largest texture and is resampled anyway.
fn texture_options(slot: Slot, size: [usize; 2], max_side: usize) -> egui::TextureOptions {
    if slot == Slot::Detail && size[0].max(size[1]) < max_side {
        egui::TextureOptions { magnification: egui::TextureFilter::Nearest, ..egui::TextureOptions::LINEAR }
    } else {
        egui::TextureOptions::LINEAR
    }
}

/// The result's texture pixels: its 8-bit image (encoded in `display`'s source space) converted to
/// the monitor, unless it is a soft proof already encoded for it. `display_key` is the key of the
/// monitor setting the conversion belongs to.
fn display_result(result: RenderResult, display: Option<&CanvasDisplay>, display_key: u64) -> DisplayResult {
    let color = result.rendered.as_ref().ok().map(|r| {
        let img = &r.image;
        let bytes = img.as_bytes();
        let bytes = if let Some(d) = display.filter(|d| !d.is_identity() && result.proof_monitor != Some(d.monitor.content_hash())) {
            d.rgb8_to_display(img.width, &bytes)
        } else {
            bytes
        };
        Arc::new(egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &bytes))
    });
    DisplayResult { result, color, display_key }
}

struct Queued {
    slot: Slot,
    priority: u32,
    seq: u64,
    job: RenderJob,
}

/// Variant thumbnails kept as textures (LRU).
pub const VARIANT_TEXTURES: usize = 96;
/// Priority of variant thumbnail jobs: below on-screen grid thumbnails and the loupe.
const VARIANT_PRIORITY: u32 = 6;

pub struct Renderer {
    #[cfg(test)]
    pub(crate) thumb_jobs_built: usize,
    request_ids: HashMap<Slot, u64>,
    /// Identity of the source the interactive slot wants. Late results may advance an edit,
    /// but must never put another photo (or pre-reload content) back on screen.
    view_sources: HashMap<Slot, (PhotoId, Option<lightcraft_preview::Hash128>)>,
    /// Request id of the pixels each interactive view (loupe, before, hover) shows: a draft that
    /// finishes after a newer request was made still replaces older pixels, so a long slider drag
    /// keeps updating the loupe instead of waiting for the drag to pause.
    shown_ids: HashMap<Slot, u64>,
    /// Requests made before this id belong to a library or preview cache that is gone.
    epoch: u64,
    preview_generation: Option<(std::sync::Weak<lightcraft_preview::PreviewCache>, u64)>,
    /// Last thumbnail inputs; weak identities do not retain photo histories or force deep clones.
    /// (photo, size bucket, job key, whether the request went through an embedded stand-in)
    thumb_inputs: HashMap<PhotoId, (std::sync::Weak<lightcraft_catalog::Photo>, usize, u64, bool)>,
    pool: JobPool<Slot, DisplayResult>,
    display: Option<Arc<CanvasDisplay>>,
    /// On a monitor wider than sRGB: the space views ([`view_slot`]) render in, and how their
    /// pixels reach the monitor.
    view: Option<(OutputSpace, Arc<CanvasDisplay>)>,
    /// Jobs run elsewhere (see [`RenderOffload`]); `queue` holds the ones not started yet.
    offload: Option<Box<dyn RenderOffload>>,
    queue: Vec<Queued>,
    seq: u64,
    /// Slot → (key, priority) of the request in flight (queued or running).
    pending: HashMap<Slot, (u64, u32)>,
    /// Slot → stop flag of its admitted render ([`Renderer::request_admitted`]) in flight.
    admitted: HashMap<Slot, Arc<AtomicBool>>,
    pub textures: HashMap<Slot, Tex>,
    pub last_main_ms: f64,
    /// The largest texture side the GPU takes (from egui, updated every poll).
    max_texture_side: usize,
    /// Jobs finished since start (for inspect/perf).
    pub completed: u64,
    /// Keep a CPU copy of every texture so the UI can be rasterized headlessly (set when the app
    /// is driven by the control channel).
    pub keep_pixels: bool,
    /// Per-view intermediate results (loupe and "before"), so slider drags redo only what changed.
    stages: HashMap<Slot, Arc<StageCache>>,
    /// Slot → key of the last quick job requested for it (each is tried once).
    quick_tried: HashMap<Slot, u64>,
    /// Jobs that failed (unreadable or missing file…), by slot and job key, with the error: not
    /// requested again until the key changes (an edit, another size) — the grid asks every frame.
    failed: HashMap<Slot, (u64, String, u64)>,
    /// The catalog revision seen at the last poll: a failure is retried once the catalog changes
    /// (a relinked or re-imported file).
    catalog_rev: u64,
    /// Prefetch slot → key of the last job submitted (each runs once).
    prefetched: HashMap<Slot, u64>,
    /// Variant key → frame it was last asked for (LRU of [`Slot::Variant`] textures).
    variant_used: HashMap<u64, u64>,
    /// Frames polled so far.
    frame: u64,
    /// Since when nothing has been pending, and whether the GPU pool was trimmed since.
    #[cfg(not(target_arch = "wasm32"))]
    idle: Option<(std::time::Instant, bool)>,
}

/// After this long without renders the GPU renderer's pool of recycled buffers is freed.
#[cfg(not(target_arch = "wasm32"))]
const IDLE_TRIM: std::time::Duration = std::time::Duration::from_secs(3);

impl Default for Renderer {
    fn default() -> Self {
        // from the compute profile: one worker per compute thread,
        // 2..=6; these visible-grid and loupe renders never take a batch permit
        let threads =
            if cfg!(target_arch = "wasm32") { 0 } else { JobPool::<Slot, RenderResult>::threads_for(lightcraft_sysmem::profile().threads).min(6) };
        Self::with_worker_threads(threads)
    }
}

impl Renderer {
    /// Explicit worker count for hosts and bounded validation runs (one to six on native).
    /// Browser hosts continue to use inline jobs or their installed worker offload.
    pub fn with_worker_threads(threads: usize) -> Self {
        let threads = if cfg!(target_arch = "wasm32") { 0 } else { threads.clamp(1, 6) };
        Renderer {
            #[cfg(test)]
            thumb_jobs_built: 0,
            request_ids: HashMap::new(),
            view_sources: HashMap::new(),
            shown_ids: HashMap::new(),
            epoch: 0,
            preview_generation: None,
            pool: JobPool::new(threads),
            display: None,
            view: None,
            thumb_inputs: HashMap::new(),
            offload: None,
            queue: Vec::new(),
            seq: 0,
            pending: HashMap::new(),
            admitted: HashMap::new(),
            textures: HashMap::new(),
            last_main_ms: 0.0,
            max_texture_side: usize::MAX,
            completed: 0,
            keep_pixels: false,
            stages: HashMap::new(),
            quick_tried: HashMap::new(),
            failed: HashMap::new(),
            catalog_rev: 0,
            prefetched: HashMap::new(),
            variant_used: HashMap::new(),
            frame: 0,
            #[cfg(not(target_arch = "wasm32"))]
            idle: None,
        }
    }
}

impl Renderer {
    /// The active shared monitor profile, carried with a proof render to avoid
    /// clipping through sRGB and applying monitor conversion twice.
    pub fn proof_monitor(&self) -> Option<lightcraft_pipeline::ProofProfile> {
        self.display.as_ref().map(|d| lightcraft_pipeline::ProofProfile::monitor(d.monitor.clone()))
    }

    /// Change monitor transform and invalidate display textures, retaining processing stage caches.
    /// New uploads run on the existing bounded workers; old in-flight monitor results are discarded.
    /// Views render in the narrowest built-in space holding the monitor's gamut, so what the
    /// monitor can show is gamut-mapped exactly as an export to that space instead of being
    /// clipped to sRGB first.
    pub fn set_display(&mut self, display: Arc<CanvasDisplay>) {
        if self.display.as_ref().is_some_and(|d| d.key == display.key) {
            return;
        }
        let stages = std::mem::take(&mut self.stages);
        self.forget_all();
        self.stages = stages;
        self.view = match view_space(&display.monitor) {
            OutputSpace::Srgb => None,
            space => match rendered_display(space, display.monitor.clone()) {
                Ok(view) => Some((space, view)),
                Err(e) => {
                    log::warn!("views stay sRGB: {e}");
                    None
                }
            },
        };
        self.display = Some(display);
    }

    /// How the pixels of `slot`'s renders reach the monitor.
    fn display_for(&self, slot: Slot) -> Option<Arc<CanvasDisplay>> {
        match &self.view {
            Some((_, view)) if view_slot(slot) => Some(view.clone()),
            _ => self.display.clone(),
        }
    }

    fn display_key(&self) -> u64 {
        self.display.as_ref().map_or(0, |d| d.key)
    }

    /// Would asking for this thumbnail again (at `priority`) change nothing?
    pub fn thumb_current(&self, photo: &Arc<lightcraft_catalog::Photo>, bucket: usize, priority: u32) -> bool {
        let Some(&(ref old, size, key, quick)) = self.thumb_inputs.get(&photo.id) else { return false };
        if old.as_ptr() != Arc::as_ptr(photo) || size != bucket {
            return false;
        }
        let slot = Slot::Thumb(photo.id);
        if quick && !self.textures.contains_key(&slot) {
            // Stand-in path (`grid::request_thumb`): the embedded preview is still on its way, or
            // the real thumbnail is requested at the background priority whatever the caller's.
            return self.pending.get(&Slot::ThumbQuick(photo.id)).is_some_and(|p| p.0 == key)
                || !self.request_needed(slot, key, crate::panels::grid::BACKGROUND_THUMB_PRIORITY);
        }
        !self.request_needed(slot, key, priority)
    }

    /// Remember the inputs of the thumbnail just requested (`quick`: through a stand-in).
    pub fn remember_thumb(&mut self, photo: &Arc<lightcraft_catalog::Photo>, bucket: usize, key: u64, quick: bool) {
        if self.thumb_inputs.len() >= 1024 && !self.thumb_inputs.contains_key(&photo.id) {
            self.thumb_inputs.retain(|_, (p, _, _, _)| p.strong_count() > 0);
            if self.thumb_inputs.len() >= 1024
                && let Some(id) = self.thumb_inputs.keys().next().copied()
            {
                self.thumb_inputs.remove(&id);
            }
        }
        self.thumb_inputs.insert(photo.id, (Arc::downgrade(photo), bucket, key, quick));
    }

    fn request_needed(&self, slot: Slot, key: u64, priority: u32) -> bool {
        // A matching old texture must not mask a different newer request (e.g. undo).
        if let Some(&(k, p)) = self.pending.get(&slot) {
            return k != key || (p != priority && self.is_queued(slot));
        }
        if self.textures.get(&slot).is_some_and(|t| t.key == key) || self.failed.get(&slot).is_some_and(|f| f.0 == key && f.2 == self.catalog_rev) {
            return false;
        }
        true
    }
    /// Run jobs through `offload` from now on (instead of the job pool / inline).
    pub fn set_offload(&mut self, offload: Box<dyn RenderOffload>) {
        self.offload = Some(offload);
    }

    fn is_queued(&self, slot: Slot) -> bool {
        self.queue.iter().any(|q| q.slot == slot) || self.pool.is_queued(slot)
    }

    /// Is the slot's current texture (or pending request) already for `key`?
    /// Key of what `slot` shows or is rendering next (the pending request wins).
    pub fn wanted(&self, slot: Slot) -> Option<u64> {
        self.pending.get(&slot).map(|p| p.0).or_else(|| self.textures.get(&slot).map(|t| t.key))
    }

    pub fn is_current(&self, slot: Slot, key: u64) -> bool {
        self.wanted(slot) == Some(key)
    }

    /// Request a render for `slot` (no-op if already current or pending at the same priority).
    pub fn request(&mut self, slot: Slot, job: RenderJob, priority: u32) {
        self.submit(slot, job, priority, None);
    }

    /// Like [`Self::request`], for a full-size render (the loupe's 1:1 view): before it decodes
    /// it waits for a heavy-job permit of `cost` bytes, so it never stacks with exports and
    /// preview builds, and while it waits a newer request for its slot (or [`Self::cancel`])
    /// stops it. Its develop takes the GPU in turn with theirs
    /// ([`lightcraft_engine::export::pipeline::run_render_job`]).
    pub fn request_admitted(&mut self, slot: Slot, job: RenderJob, priority: u32, cost: u64) {
        self.submit(slot, job, priority, Some(cost));
    }

    /// `job` as `slot` renders it: a view slot's in the monitor's space on a wide-gamut monitor
    /// ([`Self::set_display`]), with that space's key, which is the key its texture carries. (A
    /// soft proof already renders straight to the monitor.)
    pub fn for_slot(&self, slot: Slot, job: RenderJob) -> RenderJob {
        match &self.view {
            Some((space, _)) if view_slot(slot) && job.request.proof.is_none() => job.with_space(*space),
            _ => job,
        }
    }

    /// Whether the loupe's full-size renders may run: on the GPU only. On the CPU one needs
    /// several GB and seconds per edit.
    pub fn full_size_on_gpu(&self) -> bool {
        match &self.offload {
            Some(off) => off.full_size(),
            None => lightcraft_engine::gpu::ready() && lightcraft_engine::gpu::available(),
        }
    }

    /// Drop `slot`'s request in flight: a queued job is dropped, an admitted one still waiting for
    /// its permit gives up, and a late result is ignored.
    pub fn cancel(&mut self, slot: Slot) {
        if let Some(stop) = self.admitted.remove(&slot) {
            stop.store(true, Ordering::Relaxed);
        }
        if self.pending.remove(&slot).is_some() {
            self.request_ids.remove(&slot);
            self.pool.reprioritize(|s, p| (*s != slot).then_some(p));
            self.queue.retain(|q| q.slot != slot);
        }
    }

    fn submit(&mut self, slot: Slot, job: RenderJob, priority: u32, admit: Option<u64>) {
        let mut job = self.for_slot(slot, job);
        if !self.request_needed(slot, job.key, priority) {
            return;
        }
        if let Some(stop) = self.admitted.remove(&slot) {
            stop.store(true, Ordering::Relaxed);
        }
        self.pending.insert(slot, (job.key, priority));
        job.request_id = lightcraft_preview::next_tick();
        self.request_ids.insert(slot, job.request_id);
        if interactive_slot(slot) {
            self.view_sources.insert(slot, (job.photo, job.source_key));
        }
        // (an offload keeps its own per-view stage caches; the flag tells it to)
        let job = if interactive_slot(slot) { job.with_stages(self.stages.entry(slot).or_default().clone()) } else { job };
        if self.offload.is_some() {
            self.seq += 1;
            self.queue.retain(|q| q.slot != slot);
            self.queue.push(Queued { slot, priority, seq: self.seq, job });
            self.dispatch(); // an idle worker starts now rather than next frame
            return;
        }
        let key = job.key;
        let (display, display_key) = (self.display_for(slot), self.display_key());
        let max_side = self.max_texture_side;
        let background = matches!(slot, Slot::Thumb(_) | Slot::ThumbQuick(_) | Slot::Prefetch(_));
        let admit = admit.map(|cost| {
            let stop = Arc::new(AtomicBool::new(false));
            self.admitted.insert(slot, stop.clone());
            (cost, stop)
        });
        self.pool.submit(
            slot,
            key,
            priority,
            Box::new(move || {
                display_worker_priority();
                let _permit = match &admit {
                    Some((cost, stop)) => match lightcraft_sysmem::heavy_jobs().admit(*cost, &|| stop.load(Ordering::Relaxed)) {
                        Some(permit) => Some(permit),
                        None => return DisplayResult { result: job.cancelled(), color: None, display_key },
                    },
                    None => None,
                };
                let result = if background {
                    lightcraft_engine::memory::in_background(|| job.run())
                } else if admit.is_some() {
                    // full size: on the GPU one at a time with exports and preview builds
                    lightcraft_engine::export::pipeline::run_render_job(job)
                } else {
                    job.run()
                };
                display_result(fit_texture(result, max_side), display.as_deref(), display_key)
            }),
        );
    }

    /// Request a stand-in for `slot` (once per job key).
    pub fn request_quick(&mut self, slot: Slot, job: QuickJob, priority: u32) {
        if self.quick_tried.get(&slot) == Some(&job.key) {
            return;
        }
        // the loupe's stand-in: its cached render in the view space, shown as the view will be
        let (mut job, view) = match &self.view {
            Some((space, view)) if slot == Slot::Preview => (job.with_space(*space), Some(view.clone())),
            _ => (job, None),
        };
        self.quick_tried.insert(slot, job.key);
        self.pending.insert(slot, (job.key, priority));
        job.request_id = lightcraft_preview::next_tick();
        self.request_ids.insert(slot, job.request_id);
        let key = job.key;
        let (display, display_key) = (self.display_for(slot), self.display_key());
        let max_side = self.max_texture_side;
        self.pool.submit(
            slot,
            key,
            priority,
            Box::new(move || {
                display_worker_priority();
                let result = fit_texture(job.run(), max_side);
                let display = view.filter(|_| result.quick == Some(QuickSource::Cached)).or(display);
                display_result(result, display.as_deref(), display_key)
            }),
        );
    }

    /// Prepare a photo in the background (a [`Slot::Prefetch`] job runs once per key; a newer one
    /// replaces it while queued). Memory stays bounded by the engine's source caches. Its view
    /// render is cached in the views' space, so the loupe's stand-in for it is the view itself.
    pub fn prefetch(&mut self, slot: Slot, job: RenderJob, priority: u32) {
        let mut job = match &self.view {
            Some((space, _)) => job.with_space(*space),
            None => job,
        };
        if self.prefetched.get(&slot) == Some(&job.key) {
            return;
        }
        self.prefetched.insert(slot, job.key);
        self.pending.insert(slot, (job.key, priority));
        job.request_id = lightcraft_preview::next_tick();
        self.request_ids.insert(slot, job.request_id);
        let key = job.key;
        let display_key = self.display_key();
        self.pool.submit(
            slot,
            key,
            priority,
            Box::new(move || {
                display_worker_priority();
                DisplayResult { result: lightcraft_engine::memory::in_background(|| job.run()), color: None, display_key }
            }),
        );
    }

    /// Is a request for `slot` queued or running?
    /// Drop every texture and cached stage (another library was opened: photo ids changed meaning).
    pub fn forget_all(&mut self) {
        for (_, stop) in self.admitted.drain() {
            stop.store(true, Ordering::Relaxed);
        }
        self.request_ids.clear();
        self.view_sources.clear();
        self.shown_ids.clear();
        self.epoch = lightcraft_preview::next_tick();
        self.preview_generation = None;
        self.pool.reprioritize(|_, _| None);
        self.thumb_inputs.clear();
        self.textures.clear();
        self.stages.clear();
        self.quick_tried.clear();
        self.failed.clear();
        self.prefetched.clear();
        self.pending.clear();
        self.queue.clear();
    }

    pub fn is_pending(&self, slot: Slot) -> bool {
        self.pending.contains_key(&slot)
    }

    /// The texture to show for a grid/filmstrip thumbnail: the rendered one, else its stand-in.
    pub fn thumb(&self, id: PhotoId) -> Option<&Tex> {
        self.textures.get(&Slot::Thumb(id)).or_else(|| self.textures.get(&Slot::ThumbQuick(id)))
    }

    pub fn queued(&self) -> usize {
        self.pool.queued() + self.queue.len()
    }

    /// Hand queued jobs to the offload (best first) while it has room.
    fn dispatch(&mut self) {
        let Some(off) = self.offload.as_mut() else { return };
        while let Some(i) = self.queue.iter().enumerate().max_by_key(|(_, q)| (q.priority, q.seq)).map(|(i, _)| i) {
            let q = self.queue.swap_remove(i);
            if let Some(job) = off.try_start(q.slot, q.job) {
                self.queue.push(Queued { job, ..q });
                break;
            }
        }
    }

    /// Requests queued or running.
    pub fn in_flight(&self) -> usize {
        self.pending.len()
    }

    /// Why the last render for `slot` failed (until a render with another key succeeds).
    pub fn failure(&self, slot: Slot) -> Option<&str> {
        self.failed.get(&slot).map(|f| f.1.as_str())
    }

    /// The slots with a render pending (diagnostics: `ui.inspect`).
    pub fn pending_slots(&self) -> Vec<String> {
        self.pending.keys().map(|s| format!("{s:?}")).collect()
    }

    /// Thumbnail textures currently loaded.
    /// A variant thumbnail ([`Slot::Variant`]): its texture once rendered; until then the job is
    /// queued at low priority (below on-screen grid thumbnails). Call it every frame the variant
    /// is visible: variants not asked for during the last frames leave the queue, and textures
    /// beyond [`VARIANT_TEXTURES`] are evicted least recently used first.
    pub fn variant(&mut self, job: RenderJob) -> Option<&Tex> {
        let key = job.key;
        let slot = Slot::Variant(key);
        self.variant_used.insert(key, self.frame);
        if !self.textures.contains_key(&slot) {
            self.request(slot, job, VARIANT_PRIORITY);
        }
        self.textures.get(&slot)
    }

    /// Variant thumbnail textures currently loaded.
    pub fn variant_textures(&self) -> usize {
        self.textures.keys().filter(|s| matches!(s, Slot::Variant(_))).count()
    }

    /// Drop variant jobs nobody asked for in the last frames, and the least recently used
    /// variant textures beyond the budget.
    fn evict_variants(&mut self) {
        let now = self.frame;
        let stale = |used: Option<&u64>| used.is_none_or(|f| now.saturating_sub(*f) > 2);
        let used = &self.variant_used;
        let dropped = self.pool.reprioritize(|s, p| match s {
            Slot::Variant(k) if stale(used.get(k)) => None,
            _ => Some(p),
        });
        for s in dropped {
            self.pending.remove(&s);
            self.request_ids.remove(&s);
        }
        let request_ids = &mut self.request_ids;
        let pending = &mut self.pending;
        self.queue.retain(|q| match q.slot {
            Slot::Variant(k) if stale(used.get(&k)) => {
                pending.remove(&q.slot);
                request_ids.remove(&q.slot);
                false
            }
            _ => true,
        });
        let n = self.variant_textures();
        if n > VARIANT_TEXTURES {
            let mut have: Vec<(u64, u64)> = self
                .textures
                .keys()
                .filter_map(|s| if let Slot::Variant(k) = s { Some((self.variant_used.get(k).copied().unwrap_or(0), *k)) } else { None })
                .collect();
            have.sort_unstable();
            for (_, k) in have.into_iter().take(n - VARIANT_TEXTURES) {
                self.textures.remove(&Slot::Variant(k));
                self.variant_used.remove(&k);
            }
        }
        if self.variant_used.len() > 4 * VARIANT_TEXTURES {
            let textures = &self.textures;
            let pending = &self.pending;
            self.variant_used.retain(|k, _| textures.contains_key(&Slot::Variant(*k)) || pending.contains_key(&Slot::Variant(*k)));
        }
    }

    pub fn thumb_textures(&self) -> usize {
        self.textures.keys().filter(|s| matches!(s, Slot::Thumb(_) | Slot::ThumbQuick(_))).count()
    }

    /// Collect finished jobs into textures. Returns true if anything changed.
    pub fn poll(&mut self, ctx: &egui::Context, session: &mut Session) -> bool {
        if lightcraft_sysmem::gpu::pressured() || lightcraft_engine::gpu::recovering() {
            for stages in self.stages.values() {
                lightcraft_engine::gpu::trim_stages(stages);
            }
            lightcraft_engine::gpu::trim_pool(0);
        }
        let cache = &session.media.rendered;
        let generation = cache.generation();
        if self.preview_generation.as_ref().is_some_and(|(old, g)| old.as_ptr() != Arc::as_ptr(cache) || *g != generation) {
            self.forget_all();
        }
        self.preview_generation = Some((Arc::downgrade(cache), generation));
        self.catalog_rev = session.catalog.revision;
        self.max_texture_side = ctx.input(|i| i.max_texture_side).max(1);
        self.frame += 1;
        // wasm: run one job per frame on this thread, timed with the host clock
        #[cfg(target_arch = "wasm32")]
        let inline_ms = {
            let t0 = crate::now_ms();
            self.pool.run_inline(1);
            crate::now_ms() - t0
        };
        #[cfg(not(target_arch = "wasm32"))]
        let inline_ms = 0.0;
        while let Some(failure) = self.pool.try_recv_failure() {
            if self.pending.get(&failure.slot).is_some_and(|(key, _)| *key == failure.key) {
                self.pending.remove(&failure.slot);
                self.failed.insert(failure.slot, (failure.key, failure.message, self.catalog_rev));
            }
        }
        let mut finished = Vec::new();
        while let Some(done) = self.pool.try_recv() {
            finished.push((done.slot, done.result, if done.ms > 0.0 { done.ms } else { inline_ms }));
        }
        let offloaded = self.offload.as_mut().map(|off| off.finished()).unwrap_or_default();
        let (display_key, max_side) = (self.display_key(), self.max_texture_side);
        finished.extend(
            offloaded
                .into_iter()
                .map(|(slot, result, ms)| (slot, display_result(fit_texture(result, max_side), self.display_for(slot).as_deref(), display_key), ms)),
        );
        self.dispatch();
        let mut changed = false;
        for (mut slot, done, ms) in finished {
            if done.display_key != self.display_key() {
                continue;
            }
            let DisplayResult { result: r, color, .. } = done;
            self.completed += 1;
            // Ignore obsolete pixels, failures and decoded sources, even for the same render key;
            // an interactive view still takes a superseded draft that is newer than what it shows.
            let current = self.request_ids.get(&slot) == Some(&r.request_id);
            if !current {
                let newer_draft = interactive_slot(slot)
                    && self.request_ids.contains_key(&slot)
                    && self.view_sources.get(&slot) == Some(&(r.photo, r.source_key))
                    && r.request_id >= self.epoch
                    && self.shown_ids.get(&slot).is_none_or(|shown| r.request_id > *shown)
                    && r.rendered.is_ok();
                if !newer_draft {
                    continue;
                }
            } else {
                self.request_ids.remove(&slot);
                self.admitted.remove(&slot);
            }
            // Even an advancing superseded preview can supply the decoded source. Otherwise a
            // continuous drag that starts cold keeps decoding until the user finally releases.
            session.accept(&r);
            if interactive_slot(slot) {
                self.shown_ids.insert(slot, r.request_id);
            }
            if current && self.pending.get(&slot).is_some_and(|p| p.0 == r.key) {
                self.pending.remove(&slot);
            }
            let rendered = match r.rendered {
                Ok(x) => {
                    self.failed.remove(&slot);
                    x
                }
                Err(e) => {
                    if !matches!(slot, Slot::Prefetch(_)) {
                        log::warn!("render {slot:?}: {e}");
                        self.failed.insert(slot, (r.key, e, self.catalog_rev));
                    }
                    continue;
                }
            };
            if matches!(slot, Slot::Prefetch(_)) {
                continue;
            }
            if let Slot::ThumbQuick(id) = slot {
                if self.textures.contains_key(&Slot::Thumb(id)) {
                    continue; // the real thumbnail won the race
                }
                if r.quick == Some(QuickSource::Cached) {
                    // the photo's own cached thumbnail: final
                    slot = Slot::Thumb(id);
                }
            }
            if let Slot::Thumb(id) = slot {
                self.textures.remove(&Slot::ThumbQuick(id));
            }
            let img = &rendered.image;
            let Some(color) = color else { continue };
            let pixels = self.keep_pixels.then(|| color.clone());
            let name = format!("{slot:?}");
            let options = texture_options(slot, [img.width, img.height], self.max_texture_side);
            match self.textures.get_mut(&slot) {
                Some(t) => {
                    t.tex.set(color, options);
                    t.pixels = pixels;
                    t.key = r.key;
                    t.photo = r.photo;
                    t.size = [img.width, img.height];
                    t.histogram = Some(rendered.histogram);
                    t.ms = ms;
                    t.quick = r.quick;
                }
                None => {
                    let tex = ctx.load_texture(name, color, options);
                    self.textures.insert(
                        slot,
                        Tex {
                            key: r.key,
                            photo: r.photo,
                            tex,
                            size: [img.width, img.height],
                            histogram: Some(rendered.histogram),
                            ms,
                            quick: r.quick,
                            pixels,
                        },
                    );
                }
            }
            if slot == Slot::Main {
                self.last_main_ms = ms;
            }
            changed = true;
        }
        self.evict_variants();
        #[cfg(not(target_arch = "wasm32"))]
        self.trim_when_idle(ctx);
        if !self.pending.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        changed
    }

    /// Free the GPU renderer's recycled buffers once nothing has rendered for [`IDLE_TRIM`] (they
    /// are reallocated by the next render; while working, renders reuse them).
    #[cfg(not(target_arch = "wasm32"))]
    fn trim_when_idle(&mut self, ctx: &egui::Context) {
        if !self.pending.is_empty() {
            self.idle = None;
            return;
        }
        let now = std::time::Instant::now();
        let (since, trimmed) = *self.idle.get_or_insert((now, false));
        if trimmed {
            return;
        }
        let waited = now.duration_since(since);
        if waited >= IDLE_TRIM {
            lightcraft_engine::gpu::trim_pool(0);
            lightcraft_engine::memory::release();
            self.idle = Some((since, true));
        } else {
            ctx.request_repaint_after(IDLE_TRIM - waited);
        }
    }

    /// What the renderer holds: per-view stage caches (CPU images and GPU buffers) and textures.
    pub fn memory(&self) -> serde_json::Value {
        let cpu: usize = self.stages.values().map(|s| s.bytes()).sum();
        let gpu: usize = self.stages.values().map(|s| lightcraft_engine::gpu::stage_bytes(s)).sum();
        let tex: usize = self.textures.values().map(|t| t.size[0] * t.size[1] * 4).sum();
        let copies: usize = self.textures.values().filter_map(|t| t.pixels.as_ref()).map(|p| p.pixels.len() * 4).sum();
        serde_json::json!({
            "stageCaches": {"count": self.stages.len(), "cpuBytes": cpu, "gpuBytes": gpu},
            "textures": {"count": self.textures.len(), "bytes": tex, "cpuCopyBytes": copies},
        })
    }

    /// CPU copies of the current textures by id (see [`Self::keep_pixels`]).
    pub fn cpu_textures(&self) -> HashMap<egui::TextureId, li_testkit::softpaint::CpuTexture> {
        self.textures.values().filter_map(|t| Some((t.tex.id(), li_testkit::softpaint::CpuTexture::linear(t.pixels.clone()?)))).collect()
    }

    /// Drop the import review thumbnails (a new review, or the dialog closed).
    pub fn forget_imports(&mut self) {
        self.pool.reprioritize(|s, p| if matches!(s, Slot::Import(_)) { None } else { Some(p) });
        self.queue.retain(|q| !matches!(q.slot, Slot::Import(_)));
        self.pending.retain(|s, _| !matches!(s, Slot::Import(_)));
        self.request_ids.retain(|s, _| !matches!(s, Slot::Import(_)));
        self.textures.retain(|s, _| !matches!(s, Slot::Import(_)));
        self.quick_tried.retain(|s, _| !matches!(s, Slot::Import(_)));
    }

    /// Drop thumbnails that aren't in `keep` (bounded memory for huge libraries), and queued
    /// thumbnail jobs for photos that scrolled out of `keep`.
    pub fn evict_thumbs(&mut self, keep: &std::collections::HashSet<PhotoId>, max: usize) {
        self.thumb_inputs.retain(|id, _| keep.contains(id));
        let dropped = self.pool.reprioritize(|s, p| match s {
            Slot::Thumb(id) | Slot::ThumbQuick(id) if !keep.contains(id) && p <= 11 => None,
            _ => Some(p),
        });
        for s in dropped {
            self.pending.remove(&s);
            self.request_ids.remove(&s);
            self.quick_tried.remove(&s);
        }
        let request_ids = &mut self.request_ids;
        let pending = &mut self.pending;
        self.queue.retain(|q| match q.slot {
            Slot::Thumb(id) if !keep.contains(&id) && q.priority <= 10 => {
                pending.remove(&q.slot);
                request_ids.remove(&q.slot);
                false
            }
            _ => true,
        });
        let thumbs = self.thumb_textures();
        if thumbs <= max {
            return;
        }
        self.textures.retain(|s, _| !matches!(s, Slot::Thumb(id) | Slot::ThumbQuick(id) if !keep.contains(id)));
        self.quick_tried.retain(|s, _| !matches!(s, Slot::ThumbQuick(id) if !keep.contains(id)));
    }
}

#[cfg(test)]
mod thumbnail_tests;
