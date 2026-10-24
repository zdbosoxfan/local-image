//! AI Remove in Develop: stored patches, undoable background jobs, progress and cancellation.
//! The host supplies the inpainting engines; retouching composites patches in scene-linear RGB.

pub(crate) mod cmds;
pub mod denoise;
pub mod remove;
pub mod session;
pub mod store;

pub use session::{PatchState, Polled, for_render, patch_state};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use lightcraft_catalog::PhotoId;
use lightcraft_geom::Point;
/// Every AI job reports and is cancelled through the app's one job handle.
use lightcraft_sysmem::JobCtl;
use serde::Serialize;

// host

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

/// What an AI Remove engine gets: the area around the removal as encoded sRGB (0–1, float, no
/// 8-bit step) and the mask (255 = remove), both `width × height`.
#[derive(Clone, Debug)]
pub struct RemoveRequest {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<[f32; 3]>,
    pub mask: Vec<u8>,
    pub engine: String,
    pub seed: u64,
}

/// What it returns: the repaired pixels (encoded sRGB, as the request) and where they replace the
/// original (coverage 0..255), both the request's size.
#[derive(Clone, Debug)]
pub struct RemoveResult {
    pub rgb: Vec<[f32; 3]>,
    pub alpha: Vec<u8>,
}

/// Restoration of an already developed sRGB image for export.
#[derive(Clone, Debug)]
pub struct UpscaleRequest {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<[u8; 4]>,
    pub target: (usize, usize),
    pub method: String,
}

/// AI services the app provides to Develop. Everything here may be called from worker threads.
pub trait AiHost: Send + Sync {
    /// Upscale export pixels. Hosts without restoration retain their existing behaviour.
    fn upscale(&self, _req: &UpscaleRequest, _ctl: &JobCtl) -> Result<Vec<[u8; 4]>, String> {
        Err("AI enlargement is not available in this host".into())
    }
    /// The AI Remove engines, in menu order.
    fn remove_engines(&self) -> Vec<RemoveEngine>;
    /// Repair the masked area (blocking; poll `ctl` for cancellation and report progress on it).
    fn remove(&self, req: &RemoveRequest, ctl: &JobCtl) -> Result<RemoveResult, String>;
}

// jobs

/// The error of a cancelled job, as Develop's string errors carry it: the text of the shared
/// [`lightcraft_sysmem::Cancelled`] (`job.check()?` in a `Result<_, String>` worker yields it).
pub const CANCELLED: &str = "Cancelled";

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
    Denoise(denoise::DenoiseDone),
    DenoiseExport(String),
    Remove(RemoveDone),
}

/// What a job does.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobKind {
    Denoise,
    DenoiseExport,
    /// A new AI removal (its stroke, for the canvas to show while it runs).
    Remove {
        stroke: AiStroke,
    },
    /// A new variation of AI spot `spot`.
    Regenerate {
        spot: usize,
    },
}

/// A background AI job.
#[derive(Debug)]
pub struct Job {
    pub id: u64,
    pub photo: PhotoId,
    pub kind: JobKind,
    pub label: String,
    pub ctl: JobCtl,
    pub(crate) target_patch: Option<String>,
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
            "progress": self.ctl.fraction().unwrap_or(0.0),
            "message": self.ctl.status().message,
            "running": !self.finished(),
            "cancelled": self.ctl.is_cancelled(),
        })
    }
}

/// The work of a job, run on a worker thread.
pub type Work = Box<dyn FnOnce(&JobCtl) -> Result<Outcome, String> + Send>;

/// The session's AI state: the host's services and jobs.
#[derive(Default)]
pub struct Enhance {
    /// Optional test/host override, used on worker threads only.
    pub denoiser: Option<denoise::DenoiseFn>,
    /// Set by the app (Compositing's AI engines and model downloads). `None`: AI Remove is
    /// unavailable (CLI, web, tests without a mock).
    pub host: Option<Arc<dyn AiHost>>,
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
        self.spawn_target(photo, kind, label, wait, work, None)
    }

    pub(crate) fn spawn_target(
        &mut self,
        photo: PhotoId,
        kind: JobKind,
        label: &str,
        wait: bool,
        work: Work,
        target_patch: Option<String>,
    ) -> Arc<Job> {
        self.next += 1;
        let job = Arc::new(Job {
            id: self.next,
            photo,
            kind,
            label: label.to_owned(),
            ctl: JobCtl::new(),
            target_patch,
            result: Mutex::new(None),
            finished: AtomicBool::new(false),
        });
        self.jobs.push(job.clone());
        let run = {
            let job = job.clone();
            move || {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&job.ctl)))
                    .unwrap_or_else(|_| Err("the AI job failed unexpectedly".to_string()));
                let r = if job.ctl.is_cancelled() { Err(CANCELLED.to_string()) } else { r };
                job.finish(r);
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        if !wait {
            let name = format!("ai-job-{}", job.id);
            // An AI job with a progress bar; `wait` runs inline on the caller.
            let run = move || {
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
                run()
            };
            if let Err(e) = std::thread::Builder::new().name(name).spawn(run) {
                job.finish(Err(format!("could not start the job: {e}")));
            }
            return job;
        }
        let _ = wait;
        run();
        job
    }

    /// Discard all work when its Library closes or is replaced.
    pub fn clear(&mut self) {
        self.discard();
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

    /// A library's jobs cannot cross into a new catalog, even if photo IDs or files match.
    pub fn discard(&mut self) {
        for job in self.jobs.drain(..) {
            job.ctl.cancel();
        }
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

impl crate::Session {
    /// Where this session's AI results are kept: its library's `Patches` folder, `None` without
    /// a library on disk (results then stay in memory).
    pub fn ai_store_root(&self) -> Option<std::path::PathBuf> {
        self.library.as_ref().filter(|l| l.on_disk).map(|l| store::root_for_library(&l.dir))
    }
}

/// The content hash a photo's AI patches are keyed by.
pub fn source_hash(p: &lightcraft_catalog::Photo) -> String {
    lightcraft_preview::Hasher128::new().str(&crate::media::content_key(p)).finish().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done() -> Outcome {
        Outcome::Remove(RemoveDone {
            stroke: AiStroke { points: vec![], polygon: vec![], size: 0.01, feather: 50.0, opacity: 100.0, mask: None },
            key: "k".into(),
            source: "s".into(),
            rect: [0.0; 4],
            engine: "m".into(),
            seed: 1,
            geometry: "g".into(),
        })
    }

    #[test]
    fn bugfix_save_o047_cancel_during_saving_discards_successful_outcome() {
        let mut e = Enhance::default();
        let job = e.spawn(
            PhotoId(1),
            JobKind::DenoiseExport,
            "Saving",
            true,
            Box::new(|ctl| {
                ctl.report(0.99, "Saving");
                ctl.cancel();
                Ok(Outcome::DenoiseExport("already-written.tif".into()))
            }),
        );
        assert!(job.finished());
        assert_eq!(e.take_finished()[0].1, Err(CANCELLED.into()));
    }

    #[test]
    fn jobs_run_report_and_are_picked_up_once() {
        let mut e = Enhance::default();
        let j = e.spawn(
            PhotoId(7),
            JobKind::Regenerate { spot: 0 },
            "Regenerate",
            true,
            Box::new(|ctl| {
                ctl.report(0.5, "half");
                Ok(done())
            }),
        );
        assert!(j.finished());
        assert_eq!((j.ctl.fraction().unwrap_or(0.0), j.ctl.status().message), (0.5, "half".to_string()));
        assert_eq!(j.json()["job"]["kind"], "regenerate");
        let got = e.take_finished();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, Ok(done()));
        assert!(e.take_finished().is_empty() && e.jobs().is_empty());
    }

    #[test]
    fn background_jobs_cancel_and_panics_become_errors() {
        let mut e = Enhance::default();
        let j = e.spawn(
            PhotoId(1),
            JobKind::Regenerate { spot: 0 },
            "x",
            false,
            Box::new(|ctl| {
                while !ctl.is_cancelled() {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                ctl.check().map(|_| done()).map_err(String::from)
            }),
        );
        assert!(e.busy() && e.running_for(PhotoId(1)).count() == 1);
        assert_eq!(e.cancel(Some(j.id)), 1);
        while !j.finished() {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let p = e.spawn(PhotoId(2), JobKind::Regenerate { spot: 0 }, "y", true, Box::new(|_| panic!("boom")));
        assert!(p.finished());
        let got = e.take_finished();
        assert_eq!(got[0].1, Err(CANCELLED.to_string()));
        assert_eq!(got[1].1, Err("the AI job failed unexpectedly".to_string()));
    }
}
