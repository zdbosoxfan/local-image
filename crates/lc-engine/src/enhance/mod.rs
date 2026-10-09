//! local-image: AI in Develop — **AI Remove** (Lightroom's Remove tool with generative AI) and
//! **AI Denoise** (Lightroom's Denoise), both non-destructive.
//!
//! * Generated pixels are stored, not baked: [`store`] keeps them beside the catalog under content
//!   keys, and the develop settings refer to them (an AI spot's patch, the photo's Denoise result),
//!   so undo, history, versions and copies of the settings work as for any other edit.
//! * AI Remove asks the host for the inpainting ([`AiHost`]): the app passes in Compositing's AI
//!   Remove engines (FLUX.2 Klein, Qwen through ComfyUI), so `lc-*` crates depend on neither
//!   ComfyUI nor the editor. The patch is composited by `lightcraft_pipeline::patches` at the
//!   retouching stage.
//! * AI Denoise runs the RawNIND UtNet2 model on the CPU (`li_seg::denoise`, pure Rust); its
//!   result replaces the photo's demosaiced source when rendering ([`denoise`]), blended by the
//!   Denoise amount.
//!
//! Both are slow, so they run as background [`Job`]s with progress and cancel; the session picks
//! up finished jobs and applies them as one undo step each.

pub mod denoise;
pub mod store;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use lightcraft_catalog::PhotoId;
use lightcraft_geom::Point;
use serde::Serialize;

// ------------------------------------------------------------------------------ host

/// An AI Remove engine the host offers.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveEngine {
    /// Stable key (`klein`, `qwen-int8`, `qwen-bf16`).
    pub key: String,
    pub label: String,
    /// Why it can't run now (the AI engine isn't running, a model is missing…); `None`: ready.
    pub problem: Option<String>,
}

/// What an AI Remove engine gets: 8-bit sRGB pixels of the area around the removal and the mask
/// (255 = remove), both `width × height`.
#[derive(Clone, Debug)]
pub struct RemoveRequest {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<[u8; 3]>,
    pub mask: Vec<u8>,
    pub engine: String,
    pub seed: u64,
}

/// What it returns: the repaired pixels and where they replace the original (coverage 0..255),
/// both the request's size.
#[derive(Clone, Debug)]
pub struct RemoveResult {
    pub rgb: Vec<[u8; 3]>,
    pub alpha: Vec<u8>,
}

/// A model download the host runs.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub running: bool,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}

/// AI services the app provides to Develop. Everything here may be called from worker threads.
pub trait AiHost: Send + Sync {
    /// The AI Remove engines, in menu order.
    fn remove_engines(&self) -> Vec<RemoveEngine>;
    /// Repair the masked area (blocking; poll `ctl` for cancellation and report progress on it).
    fn remove(&self, req: &RemoveRequest, ctl: &JobCtl) -> Result<RemoveResult, String>;
    /// Start downloading a local model (`li_seg::MODELS` id, e.g. the AI Denoise model) into
    /// the models folder the session uses ([`crate::Session::quick_seg_dir`]).
    fn start_model_download(&self, id: &str) -> Result<(), String>;
    /// The last or running download of model `id`.
    fn model_download(&self, id: &str) -> Option<Download>;
    fn cancel_model_download(&self, id: &str);
}

// ------------------------------------------------------------------------------ jobs

/// Progress and cancellation of a running job (shared with its worker).
#[derive(Debug, Default)]
pub struct JobCtl {
    progress: AtomicU32,
    cancel: AtomicBool,
    message: Mutex<String>,
}

impl JobCtl {
    pub fn new() -> Arc<JobCtl> {
        Arc::new(JobCtl::default())
    }
    /// Report `frac` (0..1) done, doing `msg`.
    pub fn set(&self, frac: f32, msg: &str) {
        self.progress.store(frac.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        if let Ok(mut m) = self.message.try_lock()
            && *m != msg
        {
            msg.clone_into(&mut m);
        }
    }
    pub fn progress(&self) -> f32 {
        f32::from_bits(self.progress.load(Ordering::Relaxed))
    }
    pub fn message(&self) -> String {
        self.message.lock().map(|m| m.clone()).unwrap_or_default()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    /// `Err("cancelled")` once cancelled (for `?` in workers).
    pub fn check(&self) -> Result<(), String> {
        if self.cancelled() { Err(CANCELLED.into()) } else { Ok(()) }
    }
}

/// The error of a cancelled job.
pub const CANCELLED: &str = "cancelled";

/// Where an AI removal is and how it is blended, as painted (normalized transformed coordinates,
/// like spots).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStroke {
    /// Brush dabs (`size` is their radius as a fraction of the long edge).
    pub points: Vec<Point>,
    /// A lasso outline instead of dabs (closed; empty for a brush stroke).
    pub polygon: Vec<Point>,
    pub size: f64,
    /// 0..100: the soft edge of the brush.
    pub feather: f64,
    /// 0..100.
    pub opacity: f64,
    /// The develop layer (mask id) whose area was removed, when removed from a mask.
    pub mask: Option<u32>,
}

/// A finished AI removal: the stroke and its stored patch.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveDone {
    pub stroke: AiStroke,
    /// The patch's store key.
    pub key: String,
    /// The photo's content hash it was made from.
    pub source: String,
    /// Where the patch goes: `[x0, y0, x1, y1]`, normalized transformed coordinates.
    pub rect: [f64; 4],
    pub engine: String,
    pub seed: u64,
    /// Fingerprint of the photo's geometry (orientation, lens, perspective) when it was made.
    pub geometry: String,
}

/// What a job produced.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Remove(RemoveDone),
    Denoise(denoise::DenoiseDone),
}

/// What a job does.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobKind {
    /// A new AI removal (its stroke, for the canvas to show while it runs).
    Remove { stroke: AiStroke },
    /// A new variation of AI spot `spot`.
    Regenerate { spot: usize },
    Denoise,
}

/// A background AI job.
#[derive(Debug)]
pub struct Job {
    pub id: u64,
    pub photo: PhotoId,
    pub kind: JobKind,
    pub label: String,
    pub ctl: Arc<JobCtl>,
    result: Mutex<Option<Result<Outcome, String>>>,
    finished: AtomicBool,
}

impl Job {
    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    fn finish(&self, r: Result<Outcome, String>) {
        *self.result.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(r);
        self.finished.store(true, Ordering::SeqCst);
    }

    /// The job as `enhance.jobs` reports it.
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "photo": self.photo.0,
            "job": self.kind,
            "label": self.label,
            "progress": self.ctl.progress(),
            "message": self.ctl.message(),
            "running": !self.finished(),
            "cancelled": self.ctl.cancelled(),
        })
    }
}

/// The work of a job, run on a worker thread.
pub type Work = Box<dyn FnOnce(&JobCtl) -> Result<Outcome, String> + Send>;

/// The session's AI state: the host's services, a denoiser override, the jobs.
#[derive(Default)]
pub struct Enhance {
    /// Set by the app (Compositing's AI engines and model downloads). `None`: AI Remove is
    /// unavailable (CLI, web, tests without a mock).
    pub host: Option<Arc<dyn AiHost>>,
    /// Runs AI Denoise instead of the installed model (tests, tools).
    pub denoiser: Option<denoise::DenoiseFn>,
    jobs: Vec<Arc<Job>>,
    next: u64,
}

impl std::fmt::Debug for Enhance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Enhance").field("host", &self.host.is_some()).field("jobs", &self.jobs.len()).finish()
    }
}

impl Enhance {
    /// Start `work` as a job: on a worker thread, or (with `wait`, and always on the web) right
    /// here. A panic in the work is the job's error.
    pub fn spawn(&mut self, photo: PhotoId, kind: JobKind, label: &str, wait: bool, work: Work) -> Arc<Job> {
        self.next += 1;
        let job = Arc::new(Job {
            id: self.next,
            photo,
            kind,
            label: label.to_owned(),
            ctl: JobCtl::new(),
            result: Mutex::new(None),
            finished: AtomicBool::new(false),
        });
        self.jobs.push(job.clone());
        let run = {
            let job = job.clone();
            move || {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&job.ctl)))
                    .unwrap_or_else(|_| Err("the AI job failed unexpectedly".to_string()));
                let r = if job.ctl.cancelled() && r.is_err() { Err(CANCELLED.to_string()) } else { r };
                job.finish(r);
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        if !wait {
            let name = format!("ai-job-{}", job.id);
            if let Err(e) = std::thread::Builder::new().name(name).spawn(run) {
                job.finish(Err(format!("could not start the job: {e}")));
            }
            return job;
        }
        let _ = wait;
        run();
        job
    }

    /// Jobs not yet picked up (running or finished), oldest first.
    pub fn jobs(&self) -> &[Arc<Job>] {
        &self.jobs
    }

    pub fn job(&self, id: u64) -> Option<&Arc<Job>> {
        self.jobs.iter().find(|j| j.id == id)
    }

    /// Running jobs of `photo`.
    pub fn running_for(&self, photo: PhotoId) -> impl Iterator<Item = &Arc<Job>> {
        self.jobs.iter().filter(move |j| j.photo == photo && !j.finished())
    }

    pub fn busy(&self) -> bool {
        self.jobs.iter().any(|j| !j.finished())
    }

    /// Ask job `id` (every job with `None`) to stop. Returns how many were asked.
    pub fn cancel(&self, id: Option<u64>) -> usize {
        let mut n = 0;
        for j in self.jobs.iter().filter(|j| id.is_none_or(|i| i == j.id) && !j.finished()) {
            j.ctl.cancel();
            n += 1;
        }
        n
    }

    /// Remove and return the finished jobs with their results.
    pub fn take_finished(&mut self) -> Vec<(Arc<Job>, Result<Outcome, String>)> {
        let (done, running): (Vec<_>, Vec<_>) = std::mem::take(&mut self.jobs).into_iter().partition(|j| j.finished());
        self.jobs = running;
        done.into_iter()
            .map(|j| {
                let r = j.result.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take().unwrap_or_else(|| Err("no result".into()));
                (j, r)
            })
            .collect()
    }
}

/// Serializes the tests that switch the process-wide store's folder.
#[cfg(test)]
pub(crate) fn tests_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: Mutex<()> = Mutex::new(());
    L.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done() -> Outcome {
        Outcome::Denoise(denoise::DenoiseDone { key: "k".into(), source: "s".into(), model: "m".into() })
    }

    #[test]
    fn jobs_run_report_and_are_picked_up_once() {
        let mut e = Enhance::default();
        let j = e.spawn(PhotoId(7), JobKind::Denoise, "AI Denoise", true, Box::new(|ctl| {
            ctl.set(0.5, "half");
            Ok(done())
        }));
        assert!(j.finished());
        assert_eq!((j.ctl.progress(), j.ctl.message()), (0.5, "half".to_string()));
        assert_eq!(j.json()["job"]["kind"], "denoise");
        let got = e.take_finished();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, Ok(done()));
        assert!(e.take_finished().is_empty() && e.jobs().is_empty());
    }

    #[test]
    fn background_jobs_cancel_and_panics_become_errors() {
        let mut e = Enhance::default();
        let j = e.spawn(PhotoId(1), JobKind::Denoise, "x", false, Box::new(|ctl| {
            while !ctl.cancelled() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            ctl.check().map(|_| done())
        }));
        assert!(e.busy() && e.running_for(PhotoId(1)).count() == 1);
        assert_eq!(e.cancel(Some(j.id)), 1);
        while !j.finished() {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let p = e.spawn(PhotoId(2), JobKind::Denoise, "y", true, Box::new(|_| panic!("boom")));
        assert!(p.finished());
        let got = e.take_finished();
        assert_eq!(got[0].1, Err(CANCELLED.to_string()));
        assert_eq!(got[1].1, Err("the AI job failed unexpectedly".to_string()));
    }
}
