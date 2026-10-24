//! Automatic Upright analysed off the UI thread. Paste, Paste from Previous, Sync, Auto Sync and
//! presets give each target the copied Upright mode without a transform (a transform belongs to
//! the photo whose lines it was analysed from), and commit at once. Their targets are then
//! analysed here, each from its own preview-level source, on a background thread within the
//! heavy-job permits; until that lands, renders solve the same transform from the same source, so
//! nothing shifts when it is stored. The result is one undo step, merged into the step that asked
//! for it while that is still the latest.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use lightcraft_catalog::{Op, PhotoId};
use lightcraft_develop::{DevelopSettings, Geometry};
use lightcraft_geom::Homography;
use lightcraft_pipeline::SourceInfo;
use serde_json::{Value, json};

use super::{CommandSpec, always, cmd};
use crate::Session;
use crate::media::{SourceLevel, SourceRef};

/// Undo groups of Upright analyses (the high bit keeps them apart from the app's import groups).
const GROUP_BIT: u64 = 1 << 63;

/// One background analysis of a batch of photos.
#[derive(Debug, Default)]
pub struct UprightSolve {
    pub total: usize,
    pub done: AtomicUsize,
    pub cancel: AtomicBool,
    finished: AtomicBool,
    /// Undo group shared with the step that queued it, and that step's label.
    group: u64,
    label: String,
    library_generation: u64,
    /// (photo, [`analysis_key`] of the settings analysed, transform)
    solved: Mutex<Vec<(PhotoId, u64, Homography)>>,
}

impl UprightSolve {
    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    fn finish(&self) {
        self.finished.store(true, Ordering::Release);
    }

    pub fn json(&self) -> Value {
        json!({
            "total": self.total,
            "done": self.done.load(Ordering::Relaxed),
            "running": !self.finished(),
            "cancelled": self.cancel.load(Ordering::Relaxed),
        })
    }
}

/// The session's Upright analyses: photos queued by the command running now, and the analyses
/// started for earlier commands that are not applied yet.
#[derive(Debug, Default)]
pub struct UprightQueue {
    pending: Vec<PhotoId>,
    /// Undo steps there were when the first photo was queued (the command's own step comes after).
    undo_before: usize,
    jobs: Vec<Arc<UprightSolve>>,
}

impl UprightQueue {
    /// Analyses not yet applied, oldest first.
    pub fn jobs(&self) -> &[Arc<UprightSolve>] {
        &self.jobs
    }

    pub fn busy(&self) -> bool {
        self.jobs.iter().any(|j| !j.finished())
    }
}

/// What the analysis of a photo depends on in its settings: the mode, and everything that shapes
/// the lens-corrected, oriented image its lines are read from.
fn analysis_key(s: &DevelopSettings) -> u64 {
    DevelopSettings {
        geometry: Geometry { upright: s.geometry.upright, ..Default::default() },
        optics: s.optics,
        lens_profile: s.lens_profile.clone(),
        lens_db: s.lens_db.clone(),
        orientation: s.orientation,
        raw: s.raw,
        disabled_sections: s.disabled_sections.clone(),
        ..Default::default()
    }
    .hash64()
}

/// A photo to analyse: its preview-level source, header facts and settings, and their
/// [`analysis_key`].
struct Target {
    id: PhotoId,
    source: SourceRef,
    info: SourceInfo,
    settings: Arc<DevelopSettings>,
    key: u64,
}

fn run(targets: Vec<(u64, Target)>, state: &UprightSolve, permits: &lightcraft_sysmem::HeavyJobs) {
    super::previews::for_each_bounded("lc-upright", targets, permits, &|| state.cancel.load(Ordering::Relaxed), &|_, t| {
        match crate::media::upright_from(&t.source, &t.info, &t.settings, t.settings.geometry.upright) {
            Ok(h) => state.solved.lock().unwrap_or_else(PoisonError::into_inner).push((t.id, t.key, h)),
            Err(e) => log::warn!("Upright analysis of photo {}: {e}; its renders solve it", t.id.0),
        }
        state.done.fetch_add(1, Ordering::Relaxed);
    });
    state.finish();
}

impl Session {
    /// Analyse photo `id`'s automatic Upright in the background once the running command has
    /// committed `settings` (a mode without a transform).
    pub(crate) fn queue_upright(&mut self, id: PhotoId, settings: &DevelopSettings) {
        if !settings.upright_unsolved() || self.upright.pending.contains(&id) {
            return;
        }
        if self.upright.pending.is_empty() {
            self.upright.undo_before = self.undo.len();
        }
        self.upright.pending.push(id);
    }

    /// Start analysing the photos the finished command queued (still without a transform).
    pub(crate) fn start_upright(&mut self) {
        let ids = std::mem::take(&mut self.upright.pending);
        if ids.is_empty() {
            return;
        }
        let mut targets = Vec::new();
        for id in ids {
            let Some(p) = self.catalog.photo(id).cloned() else { continue };
            if !p.develop.upright_unsolved() {
                continue;
            }
            let cost = super::previews::preview_cost(p.width, p.height, SourceLevel::Preview.max_edge());
            let target = Target {
                id,
                source: self.media.source_ref(&p, SourceLevel::Preview),
                info: crate::media::job_info(&p, &p.develop),
                key: analysis_key(&p.develop),
                settings: p.develop.clone(),
            };
            targets.push((cost, target));
        }
        if targets.is_empty() {
            return;
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let group = GROUP_BIT | NEXT.fetch_add(1, Ordering::Relaxed);
        // the command's own step (the paste or sync) and the analysis then undo as one
        let own_step = self.undo.len() > self.upright.undo_before;
        let label = match self.undo.last_mut() {
            Some(e) if own_step && e.group.is_none() => {
                e.group = Some(group);
                e.label.clone()
            }
            _ => "Upright".to_string(),
        };
        let state = Arc::new(UprightSolve { total: targets.len(), group, label, library_generation: self.library_generation, ..Default::default() });
        self.upright.jobs.push(state.clone());
        #[cfg(not(target_arch = "wasm32"))]
        {
            let st = state.clone();
            let spawned = std::thread::Builder::new().name("lc-upright".into()).spawn(move || {
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
                run(targets, &st, lightcraft_sysmem::heavy_jobs())
            });
            if let Err(e) = spawned {
                log::warn!("Upright analysis could not start ({e}); renders solve it");
                state.finish();
            }
        }
        #[cfg(target_arch = "wasm32")]
        run(targets, &state, lightcraft_sysmem::heavy_jobs());
    }

    /// Store finished Upright analyses: each batch is one undo step on top of the photos'
    /// settings as they are now, skipping a photo whose Upright mode or optics changed meanwhile
    /// (or that got a transform another way). Waits while a slider drag or brush stroke is in
    /// progress. Call it from the frame loop; returns the photos changed.
    pub fn upright_poll(&mut self) -> usize {
        // photo ids of another library mean other photos
        for j in self.upright.jobs.iter().filter(|j| j.library_generation != self.library_generation) {
            j.cancel.store(true, Ordering::Relaxed);
        }
        if self.interaction.is_some() || !self.upright.jobs.iter().any(|j| j.finished()) {
            return 0;
        }
        let (done, running): (Vec<_>, Vec<_>) = std::mem::take(&mut self.upright.jobs).into_iter().partition(|j| j.finished());
        self.upright.jobs = running;
        let mut changed = 0;
        for job in done {
            if job.cancel.load(Ordering::Relaxed) || job.library_generation != self.library_generation {
                continue;
            }
            let solved = std::mem::take(&mut *job.solved.lock().unwrap_or_else(PoisonError::into_inner));
            let ops: Vec<Op> = solved
                .into_iter()
                .filter_map(|(id, key, h)| {
                    let d = self.develop_of(id)?;
                    if !d.upright_unsolved() || analysis_key(&d) != key {
                        return None;
                    }
                    let mut settings = (*d).clone();
                    settings.geometry.upright_transform = Some(h);
                    self.develop_op(id, settings, "Upright")
                })
                .collect();
            if ops.is_empty() {
                continue;
            }
            let n = ops.len();
            match self.execute_grouped("develop.uprightAnalysed", job.group, &job.label, |s| {
                s.commit("Upright", Op::Batch { ops })?;
                Ok(json!({"changed": n}))
            }) {
                Ok(_) => changed += n,
                Err(e) => log::warn!("Upright analysis of {n} photos not stored: {e}"),
            }
        }
        changed
    }

    /// Stop the Upright analyses not stored yet (their photos keep the mode; renders solve it).
    pub fn cancel_upright(&mut self) -> usize {
        for j in &self.upright.jobs {
            j.cancel.store(true, Ordering::Relaxed);
        }
        self.upright.jobs.len()
    }
}

fn progress(s: &Session) -> Value {
    let jobs = s.upright.jobs();
    json!({
        "total": jobs.iter().map(|j| j.total).sum::<usize>(),
        "done": jobs.iter().map(|j| j.done.load(Ordering::Relaxed)).sum::<usize>(),
        "running": s.upright.busy(),
        "jobs": jobs.iter().map(|j| j.json()).collect::<Vec<_>>(),
    })
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "develop.uprightProgress", "Upright Analysis Progress", [], None, "{} — the background Upright analyses after Paste, Sync, Auto Sync or a preset → {total, done, running, jobs}", always, |s, _| Ok(progress(s))),
        cmd!(
            "develop.cancelUpright",
            "Stop Upright Analysis",
            [],
            None,
            "{} — stop the background Upright analyses not stored yet (their photos keep the mode, solved by each render) → {cancelled}",
            always,
            |s, _| { Ok(json!({"cancelled": s.cancel_upright()})) }
        ),
    ]
}
