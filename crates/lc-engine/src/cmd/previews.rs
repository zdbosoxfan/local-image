//! Build previews ahead of time: each photo's grid thumbnail and its loupe view (standard size,
//! or 1:1), rendered into the memory + disk preview cache so browsing and the loupe are instant.
//! Runs on a background thread (the app keeps working; progress / cancel by command), or inline
//! with `wait` (CLI, MCP, tests).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd, str_param};
use crate::media::{RenderJob, THUMB_SIZES};
use crate::{Result, Session};

/// The long edge of a standard-sized preview.
pub const STANDARD_EDGE: usize = 2048;

/// A preview build in progress.
#[derive(Debug, Default)]
pub struct PreviewBuild {
    pub total: usize,
    pub done: AtomicUsize,
    pub failed: AtomicUsize,
    pub cancel: AtomicBool,
    pub finished: AtomicBool,
    /// Damaged smart previews rebuilt (Build Smart Previews only).
    pub repaired: AtomicUsize,
    /// What is built ("" = previews; "smart previews" — Build Smart Previews runs the same way).
    pub what: &'static str,
    /// Why the whole build failed (e.g. the smart previews folder isn't there).
    pub error: std::sync::Mutex<Option<String>>,
}

impl PreviewBuild {
    pub fn json(&self) -> Value {
        json!({
            "total": self.total,
            "done": self.done.load(Ordering::Relaxed),
            "failed": self.failed.load(Ordering::Relaxed),
            "running": !self.finished.load(Ordering::Relaxed),
            "pausedOnBattery": !self.finished.load(Ordering::Relaxed) && lightcraft_sysmem::power::conserving(),
            "cancelled": self.cancel.load(Ordering::Relaxed),
            "what": if self.what.is_empty() { "previews" } else { self.what },
            "repaired": self.repaired.load(Ordering::Relaxed),
            "error": self.error(),
        })
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

/// The jobs that build `id`'s previews: the largest grid thumbnail, then the view render at
/// `edge` (`None` = 1:1, the photo's full size).
fn jobs_for(s: &mut Session, id: lightcraft_catalog::PhotoId, edge: Option<usize>) -> Vec<RenderJob> {
    let Some(p) = s.catalog.photo(id) else { return Vec::new() };
    let full = p.width.max(p.height) as usize;
    let e = edge.unwrap_or(full).min(full.max(1)).max(1);
    let mut v = Vec::new();
    v.extend(s.thumb_job(id, THUMB_SIZES[THUMB_SIZES.len() - 1]));
    v.extend(s.loupe_job(id, e, e, true));
    v
}

/// Run `work(position, item)` for each item on a bounded group of threads (threading rules 3, 5,
/// 6, 9): as many workers as the heavy-job permits allow (`permits.max_jobs()`, never more than
/// items), each item admitted with a permit for its estimated `cost` bytes, so the free memory
/// decides how many run at once. Stops taking items once `cancelled` says so; items already
/// started finish. Workers are named OS threads in the Utility class (one worker runs inline on
/// the calling thread); rayon is only used inside `work`.
pub(crate) fn for_each_bounded<T: Send>(
    name: &str,
    items: Vec<(u64, T)>,
    permits: &lightcraft_sysmem::HeavyJobs,
    cancelled: &(dyn Fn() -> bool + Sync),
    work: &(dyn Fn(usize, T) + Sync),
) {
    let workers = if cfg!(target_arch = "wasm32") { 1 } else { permits.max_jobs().clamp(1, items.len().max(1)) };
    let queue = std::sync::Mutex::new(items.into_iter().enumerate());
    let run = || {
        loop {
            if !lightcraft_sysmem::power::wait_background(cancelled) {
                break;
            }
            let Some((i, (cost, item))) = queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner).next() else { break };
            let Some(_permit) = permits.admit(cost, cancelled) else { break };
            work(i, item);
        }
    };
    if workers == 1 {
        return run();
    }
    std::thread::scope(|sc| {
        let mut started = 0;
        for n in 0..workers {
            let spawned = std::thread::Builder::new().name(format!("{name}-{n}")).spawn_scoped(sc, || {
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
                run()
            });
            match spawned {
                Ok(_) => started += 1,
                Err(e) => log::warn!("{name}: could not start a worker ({e})"),
            }
        }
        if started == 0 {
            run();
        }
    });
}

/// Working memory of one photo's preview build at long edge `edge` (rule 6): its raw data
/// (2 B/px of the original) plus a render at that size ([`crate::export::render_cost`]).
fn preview_cost(width: u32, height: u32, edge: usize) -> u64 {
    let (w, h) = (width as usize, height as usize);
    let long = w.max(h).max(1);
    let scale = (edge as f64 / long as f64).min(1.0);
    let at = ((w as f64 * scale).round() as usize, (h as f64 * scale).round() as usize);
    let raw = if w == 0 || h == 0 { 0 } else { w as u64 * h as u64 * 2 };
    raw + crate::export::render_cost(at, at, lightcraft_pipeline::OutputDepth::U8)
}

/// Build each photo's previews, several photos at a time ([`for_each_bounded`]); the loupe renders
/// share the GPU one at a time ([`crate::export::pipeline::run_render_job`]). Progress and failures
/// go to `state`; visible-grid thumbnails (the Library's own job pool) never wait for these
/// permits.
fn run_all(jobs: Vec<(u64, Vec<RenderJob>)>, state: &PreviewBuild, permits: &lightcraft_sysmem::HeavyJobs) {
    for_each_bounded("lc-build-previews", jobs, permits, &|| state.cancel.load(Ordering::Relaxed), &|_, photo_jobs| {
        let ok = photo_jobs.into_iter().all(|j| crate::export::pipeline::run_render_job(j).rendered.is_ok());
        if !ok {
            state.failed.fetch_add(1, Ordering::Relaxed);
        }
        state.done.fetch_add(1, Ordering::Relaxed);
    });
    state.finished.store(true, Ordering::Relaxed);
}

fn build(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "library.buildPreviews";
    let edge = match str_param(p, "size").unwrap_or("standard") {
        "embedded" => Some(0),
        "standard" => Some(p.get("edge").and_then(Value::as_u64).map_or(STANDARD_EDGE, |e| e.clamp(256, 8192) as usize)),
        "full" | "1:1" => None,
        other => return Err(bad(C, format!("unknown size `{other}` (standard|full)"))),
    };
    if s.preview_build.as_ref().is_some_and(|b| !b.finished.load(Ordering::Relaxed)) {
        return Err(bad(C, "a preview build is already running"));
    }
    // explicit ids, else the selection, else everything in view
    let ids = if p.get("ids").is_some() || p.get("id").is_some() || !s.selection.ids.is_empty() { s.targets(p) } else { s.visible_cloned() };
    let jobs: Vec<(u64, Vec<RenderJob>)> = ids
        .iter()
        .map(|id| {
            let cost = s.catalog.photo(*id).map_or(0, |p| preview_cost(p.width, p.height, edge.unwrap_or(usize::MAX)));
            (cost, if edge == Some(0) { s.thumb_job(*id, THUMB_SIZES[THUMB_SIZES.len() - 1]).into_iter().collect() } else { jobs_for(s, *id, edge) })
        })
        .filter(|(_, j)| !j.is_empty())
        .collect();
    let state = Arc::new(PreviewBuild { total: jobs.len(), ..Default::default() });
    s.preview_build = Some(state.clone());
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false) || cfg!(target_arch = "wasm32");
    if wait {
        run_all(jobs, &state, lightcraft_sysmem::heavy_jobs());
        return Ok(state.json());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let st = state.clone();
        std::thread::Builder::new()
            .name("lc-build-previews".into())
            .spawn(move || {
                // A batch with a progress bar (threading rule 7).
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
                run_all(jobs, &st, lightcraft_sysmem::heavy_jobs())
            })
            .map_err(|e| bad(C, format!("could not start: {e}")))?;
    }
    Ok(state.json())
}

/// One photo's smart preview to build or discard: (photo, its file in the smart previews folder,
/// how to load the original at preview size, the estimated working memory of building it).
#[cfg(not(target_arch = "wasm32"))]
type SmartJob = (lightcraft_catalog::PhotoId, std::path::PathBuf, Option<crate::media::SourceRef>, u64);

/// What Build / Discard Smart Previews did.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct SmartCounts {
    /// Proxies there now (complete ones kept, missing or damaged ones written).
    built: usize,
    /// Of those, damaged ones (cut short by a crash or a full drive) that were rebuilt.
    repaired: usize,
    removed: usize,
    /// `[[id, why]]`
    failed: Vec<Value>,
}

/// What building one smart preview did.
#[cfg(not(target_arch = "wasm32"))]
enum SmartOutcome {
    /// A complete proxy was already there, or one was written (`repaired`: over a damaged one).
    Built {
        repaired: bool,
    },
    Failed(Value),
}

/// The file-system half of Build / Discard Smart Previews (runs on a worker thread in the app).
/// `custom`: the folder was chosen (on another drive, perhaps): it must be there, it is never
/// recreated. With `state`, progress and cancel. Builds run several at a time within the
/// heavy-job `permits` ([`for_each_bounded`]); the counts and failures come out in job order.
#[cfg(not(target_arch = "wasm32"))]
fn smart_run(
    dir: &std::path::Path,
    custom: bool,
    discard: bool,
    jobs: Vec<SmartJob>,
    state: Option<&PreviewBuild>,
    permits: &lightcraft_sysmem::HeavyJobs,
) -> std::result::Result<SmartCounts, String> {
    let mut n = SmartCounts::default();
    let cancelled = || state.is_some_and(|st| st.cancel.load(Ordering::Relaxed));
    if discard {
        for (_, path, _, _) in jobs {
            if cancelled() {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                n.removed += 1;
            }
            if let Some(st) = state {
                st.done.fetch_add(1, Ordering::Relaxed);
            }
        }
        return Ok(n);
    }
    // a chosen folder on another drive must be there: never recreate it on this one
    if custom {
        crate::smart::check_writable(dir)?;
    } else {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let outcomes = std::sync::Mutex::new(Vec::new());
    outcomes.lock().unwrap_or_else(std::sync::PoisonError::into_inner).resize_with(jobs.len(), || None);
    let items = jobs.into_iter().map(|(id, path, source, cost)| (cost, (id, path, source))).collect();
    for_each_bounded("lc-smart-previews", items, permits, &cancelled, &|i, (id, path, source)| {
        // a complete proxy is kept; a missing or damaged one (cut short by a crash or a full
        // drive) is (re)built
        let damaged = path.exists();
        let outcome = if damaged && crate::smart::is_valid(&path) {
            SmartOutcome::Built { repaired: false }
        } else {
            // atomic: a failed write leaves no partial proxy that would pass for a built one;
            // and synced (issue #134): built so the photo can be edited while its original
            // is offline, when it is the only copy — the sync is small next to the decode
            let r = source
                .ok_or_else(|| "nothing to build from".to_string())
                .and_then(|s| s.load_source())
                .and_then(|src| crate::smart::encode_source(&src))
                .and_then(|b| li_fs::write_atomic(&path, &b).map_err(|e| format!("{}: {e}", path.display())));
            match r {
                Ok(()) => {
                    if damaged && let Some(st) = state {
                        st.repaired.fetch_add(1, Ordering::Relaxed);
                    }
                    SmartOutcome::Built { repaired: damaged }
                }
                Err(e) => {
                    if let Some(st) = state {
                        st.failed.fetch_add(1, Ordering::Relaxed);
                    }
                    SmartOutcome::Failed(json!([id.0, e]))
                }
            }
        };
        if let Some(st) = state {
            st.done.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(slot) = outcomes.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_mut(i) {
            *slot = Some(outcome);
        }
    });
    for outcome in outcomes.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner).into_iter().flatten() {
        match outcome {
            SmartOutcome::Built { repaired } => {
                n.built += 1;
                n.repaired += usize::from(repaired);
            }
            SmartOutcome::Failed(why) => n.failed.push(why),
        }
    }
    Ok(n)
}

/// Smart previews: build (from the original, at preview size) or discard the proxies that keep
/// photos editable while their originals are offline. With `background` (the app) the files are
/// read and written on a worker thread; progress like Build Previews (`library.previewProgress`).
#[cfg(not(target_arch = "wasm32"))]
fn smart(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "library.smartPreviews";
    let dir = s.media.smart_dir.clone().ok_or_else(|| bad(C, "smart previews need a library on disk"))?;
    let discard = p.get("discard").and_then(Value::as_bool).unwrap_or(false);
    let background = p.get("background").and_then(Value::as_bool).unwrap_or(false);
    if background && s.preview_build.as_ref().is_some_and(|b| !b.finished.load(Ordering::Relaxed)) {
        return Err(bad(C, "a preview build is already running"));
    }
    let ids = if p.get("ids").is_some() || !s.selection.ids.is_empty() { s.targets(p) } else { s.visible_cloned() };
    let mut jobs: Vec<SmartJob> = Vec::new();
    for id in ids {
        let Some(ph) = s.catalog.photo(id).cloned() else { continue };
        if !matches!(ph.source, lightcraft_catalog::Source::File { .. }) {
            continue;
        }
        let path = dir.join(crate::smart::file_name(&ph));
        let source = (!discard).then(|| s.media.source_ref(&ph, crate::media::SourceLevel::Preview));
        let cost = preview_cost(ph.width, ph.height, crate::media::SourceLevel::Preview.max_edge());
        jobs.push((id, path, source, cost));
    }
    let custom = s.smart_previews_dir.is_some();
    if background {
        let what = if discard { "discarding smart previews" } else { "smart previews" };
        let state = Arc::new(PreviewBuild { total: jobs.len(), what, ..Default::default() });
        s.preview_build = Some(state.clone());
        let st = state.clone();
        std::thread::Builder::new()
            .name("lc-smart-previews".into())
            .spawn(move || {
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
                if let Err(e) = smart_run(&dir, custom, discard, jobs, Some(&st), lightcraft_sysmem::heavy_jobs()) {
                    log::warn!("{C}: {e}");
                    *st.error.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(e);
                }
                st.finished.store(true, Ordering::Relaxed);
            })
            .map_err(|e| bad(C, format!("could not start: {e}")))?;
        return Ok(state.json());
    }
    let r = smart_run(&dir, custom, discard, jobs, None, lightcraft_sysmem::heavy_jobs()).map_err(|e| bad(C, e))?;
    Ok(json!({"built": r.built, "repaired": r.repaired, "removed": r.removed, "failed": r.failed}))
}

/// The smart previews folder: where it is, whether it is custom, what it holds, whether it can be
/// used now (a chosen folder on an unplugged drive is `available: false`).
#[cfg(not(target_arch = "wasm32"))]
fn location_json(s: &Session) -> Value {
    let dir = s.media.smart_dir.clone();
    let default = s.library.as_ref().filter(|l| l.on_disk).map(|l| crate::smart::dir(&l.dir));
    let (count, bytes) = dir.as_deref().map_or((0, 0), crate::smart::stats);
    json!({
        "path": dir.as_ref().map(|d| d.to_string_lossy()),
        "default": default.map(|d| d.to_string_lossy().to_string()),
        "custom": s.smart_previews_dir.is_some(),
        "available": dir.as_ref().is_some_and(|d| d.is_dir()) || s.smart_previews_dir.is_none(),
        "count": count,
        "bytes": bytes,
    })
}

/// Show or change where smart previews are kept (per library).
#[cfg(not(target_arch = "wasm32"))]
fn smart_location(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "library.smartPreviewsLocation";
    let current = s.media.smart_dir.clone().ok_or_else(|| bad(C, "smart previews need a library on disk"))?;
    let reset = p.get("reset").and_then(Value::as_bool).unwrap_or(false);
    let chosen = str_param(p, "path").filter(|x| !x.trim().is_empty());
    if !reset && chosen.is_none() {
        return Ok(location_json(s));
    }
    let lib_dir = s.library.as_ref().filter(|l| l.on_disk).map(|l| l.dir.clone()).ok_or_else(|| bad(C, "smart previews need a library on disk"))?;
    let default = crate::smart::dir(&lib_dir);
    let (new_dir, custom) = match chosen {
        Some(x) if !reset => {
            let d = std::path::PathBuf::from(x.trim());
            if !d.is_absolute() {
                return Err(bad(C, "path must be an absolute folder path"));
            }
            (d.clone(), (d != default).then_some(d))
        }
        _ => (default, None),
    };
    if new_dir == current || current.canonicalize().ok().is_some_and(|path| new_dir.canonicalize().ok() == Some(path)) {
        return Ok(location_json(s));
    }
    // the new place must work before anything moves; there is no fallback to another drive
    crate::smart::check_writable(&new_dir).map_err(|e| bad(C, e))?;
    let (held, _) = crate::smart::stats(&current);
    let existing = match str_param(p, "existing") {
        Some(x) => Some(crate::smart::Existing::parse(x).ok_or_else(|| bad(C, "existing must be move, leave or discard"))?),
        None if held == 0 => Some(crate::smart::Existing::Leave),
        None => None,
    };
    let Some(existing) = existing else {
        return Err(bad(C, format!("{held} smart preview(s) are in {}: pass existing = move, leave or discard", current.display())));
    };
    let (moved, failed) = crate::smart::migrate(&current, &new_dir, existing);
    s.smart_previews_dir = custom;
    s.media.smart_dir = Some(new_dir);
    s.save_prefs()?;
    let mut r = location_json(s);
    r["handled"] = json!(moved);
    r["failed"] = json!(failed);
    Ok(r)
}

/// Whether photo `id` has a smart preview.
pub fn has_smart_preview(s: &Session, id: lightcraft_catalog::PhotoId) -> bool {
    match (&s.media.smart_dir, s.catalog.photo(id)) {
        (Some(dir), Some(p)) => crate::smart::is_valid(&dir.join(crate::smart::file_name(p))),
        _ => false,
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        #[cfg(not(target_arch = "wasm32"))]
        cmd!(
            "library.smartPreviews",
            "Build Smart Previews",
            [],
            None,
            "{ids?, discard?: bool, background?: bool} — build (or discard) the smart previews of the selected photos (else all in view): compact proxies in the library that keep photos editable and exportable (at proxy size) while their originals are offline; a damaged proxy (cut short) is rebuilt → {built, repaired, removed, failed}; with background (the app's menu) the files are read and written on a worker thread → {total, done, failed, repaired, running, what} like library.buildPreviews (progress: library.previewProgress, stop: library.cancelPreviews)",
            always,
            smart
        ),
        #[cfg(not(target_arch = "wasm32"))]
        cmd!(
            "library.smartPreviewsLocation",
            "Smart Previews Location",
            [],
            None,
            "{path?: absolute folder, reset?: bool, existing?: move|leave|discard} — without path/reset: where this library keeps its smart previews → {path, default, custom, available, count, bytes}. With path (or reset = back to `Smart Previews` in the library): use that folder instead (must exist or have an existing parent, and accept writes; never falls back to another drive). Smart previews already in the old folder need `existing`: move them, leave them (build again), or discard them → also {handled, failed}",
            always,
            smart_location
        ),
        cmd!(query "photo.smartPreview", "Smart Preview Status", [], None, "{id?} → {smartPreview: bool, originalOnline: bool}", always, |s, p| {
            let id = s.targets(p).first().copied().ok_or_else(|| bad("photo.smartPreview", "no photo"))?;
            let online = match s.catalog.photo(id).map(|p| p.source.clone()) {
                Some(lightcraft_catalog::Source::File { path }) => std::path::Path::new(&path).exists(),
                _ => true,
            };
            Ok(json!({"smartPreview": has_smart_preview(s, id), "originalOnline": online}))
        }),
        cmd!(
            "library.buildPreviews",
            "Build Previews",
            [],
            None,
            "{size?: standard (2048 px, or `edge`) | full (1:1), ids?, wait?: bool} — render the grid thumbnail and loupe view of the selected photos (else all in view) into the preview cache, in the background unless `wait` → {total, done, failed, running}",
            always,
            build
        ),
        cmd!(query "library.previewProgress", "Preview Build Progress", [], None, "{} → {total, done, failed, repaired, running, cancelled, what, error} | null", always, |s, _| {
            Ok(s.preview_build.as_ref().map_or(Value::Null, |b| b.json()))
        }),
        cmd!("library.cancelPreviews", "Cancel Preview Build", [], None, "{}", always, |s, _| {
            if let Some(b) = &s.preview_build {
                b.cancel.store(true, Ordering::Relaxed);
            }
            Ok(s.preview_build.as_ref().map_or(Value::Null, |b| b.json()))
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_sysmem::{HeavyJobs, MemoryNow};
    use std::sync::Mutex;

    fn permits(max_jobs: usize, available: u64) -> HeavyJobs {
        HeavyJobs::new(max_jobs, 64 << 30, move || MemoryNow { available, rss: None })
    }

    /// Runs `n` items that each take a moment; returns (positions seen, most running at once).
    fn bounded_run(n: usize, jobs: &HeavyJobs, cost: u64, cancel_after: Option<usize>) -> (Vec<usize>, usize) {
        let seen = Mutex::new(Vec::new());
        let (running, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let cancel = AtomicBool::new(false);
        let items = (0..n).map(|i| (cost, i)).collect();
        for_each_bounded("test-bounded", items, jobs, &|| cancel.load(Ordering::SeqCst), &|pos, item| {
            assert_eq!(pos, item);
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(5));
            running.fetch_sub(1, Ordering::SeqCst);
            let mut s = seen.lock().unwrap();
            s.push(item);
            if cancel_after.is_some_and(|c| s.len() >= c) {
                cancel.store(true, Ordering::SeqCst);
            }
        });
        let mut seen = seen.into_inner().unwrap();
        seen.sort_unstable();
        (seen, peak.into_inner())
    }

    #[test]
    fn bounded_batches_run_every_item_once_within_the_permits() {
        let gb = 1u64 << 30;
        // plenty of memory: up to the job slots at once, every item exactly once
        let jobs = permits(3, 64 * gb);
        let (seen, peak) = bounded_run(24, &jobs, gb, None);
        assert_eq!(seen, (0..24).collect::<Vec<_>>());
        assert!((1..=3).contains(&peak), "{peak}");
        assert_eq!(jobs.in_flight(), (0, 0));
        // memory for one item only (3 GB headroom, 2 GB each): one at a time, still all done
        let jobs = permits(3, 4 * gb);
        let (seen, peak) = bounded_run(8, &jobs, 2 * gb, None);
        assert_eq!(seen.len(), 8);
        assert_eq!(peak, 1);
        // cancel: no new items after it; the ones started finish
        let jobs = permits(2, 64 * gb);
        let (seen, _) = bounded_run(40, &jobs, gb, Some(3));
        assert!((3..=4).contains(&seen.len()), "{}", seen.len());
        assert_eq!(jobs.in_flight(), (0, 0));
    }

    #[test]
    fn preview_cost_grows_with_the_size_built() {
        let standard = preview_cost(6000, 4000, STANDARD_EDGE);
        let full = preview_cost(6000, 4000, usize::MAX);
        assert!(standard < full);
        assert!(full > 1 << 30, "a 1:1 preview of 24 MP plans for over 1 GB");
        assert!(preview_cost(1000, 800, STANDARD_EDGE) < standard);
    }
}
