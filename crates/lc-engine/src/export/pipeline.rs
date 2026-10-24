use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use lightcraft_sysmem::{HeavyJobs, Permit, ThreadClass};

use super::{Exported, PreparedExport};

/// Batch renders that would use the GPU take this one at a time: concurrent full-size GPU
/// renders could exhaust video memory. Device errors use bounded recovery; local
/// ComfyUI pressure temporarily selects CPU rendering. Their decoding runs in parallel outside it. Interactive renders (loupe, grid)
/// never take it.
pub static GPU_GATE: Mutex<()> = Mutex::new(());
/// AI enlargement (CPU inference or the ComfyUI GPU queue) in an export: one at a time.
pub static AI_GATE: Mutex<()> = Mutex::new(());

/// How many stage threads a batch uses and how far ahead of the writer it may run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    /// Render threads (each develops one photo at a time).
    pub renderers: usize,
    /// Encode threads.
    pub encoders: usize,
    /// Photos admitted but not yet written, at most.
    pub window: usize,
}

impl Shape {
    /// One photo at a time, render → encode → write: the old serial loop (for tests and the
    /// "concurrent exports: 1" choice).
    pub fn serial() -> Shape {
        Shape { renderers: 1, encoders: 1, window: 1 }
    }

    /// From the heavy-job slots (`max_jobs`, memory still limits admission) and the compute
    /// threads `C`: one render thread per slot, one encode thread more (encoders are mostly
    /// single-threaded per image) within `C`, never more threads than photos; the window lets
    /// every stage hold a photo plus one waiting between stages.
    pub fn auto(items: usize, max_jobs: usize, threads: usize) -> Shape {
        let items = items.max(1);
        let renderers = max_jobs.clamp(1, items);
        let encoders = (max_jobs + 1).min(threads).clamp(1, items);
        Shape { renderers, encoders, window: renderers + encoders + 2 }
    }
}

/// One photo out of the pipeline: its position, its file (or error) and its permit (released
/// once written).
pub(crate) type Output<'p> = (usize, Result<Exported, String>, Permit<'p>);

/// How far the writer got, so the feeder never runs more than [`Shape::window`] photos ahead.
#[derive(Default)]
pub(crate) struct Flow {
    written: Mutex<usize>,
    cv: Condvar,
}

impl Flow {
    /// Positions before `n` are done.
    pub(crate) fn written(&self, n: usize) {
        *self.written.lock().unwrap_or_else(PoisonError::into_inner) = n;
        self.cv.notify_all();
    }

    /// Wake the feeder (the batch is ending).
    pub(crate) fn wake(&self) {
        self.cv.notify_all();
    }

    /// Wait until position `seq` is within `window` of the writer; false once `stopped`.
    fn admit(&self, seq: usize, window: usize, stopped: &dyn Fn() -> bool) -> bool {
        let mut w = self.written.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if stopped() {
                return false;
            }
            if seq < *w + window.max(1) {
                return true;
            }
            w = self.cv.wait_timeout(w, Duration::from_millis(50)).unwrap_or_else(PoisonError::into_inner).0;
        }
    }
}

/// Run a batch's render job: decode first, then develop. When the develop may use the GPU it
/// takes [`GPU_GATE`] (the decode before it does not), so batch items share the GPU one at a
/// time while their decoding and the CPU work around it overlap.
pub fn run_render_job(mut job: crate::media::RenderJob) -> crate::media::RenderResult {
    // the conditions under which `develop` keeps the render off the GPU; those renders, and
    // all of them with no usable GPU (or GPU off), run side by side on the CPU
    let gpu = job.cpu_only().is_none() && lightcraft_gpu::available();
    if !gpu {
        return job.run();
    }
    if !matches!(job.source, crate::media::SourceRef::Loaded(_)) {
        match crate::guard::catch("Preview render", || job.source.load_source()) {
            Ok(Ok(src)) => job.source = crate::media::SourceRef::Loaded(Box::new(src)),
            // the job reports the failure as it always has
            _ => return job.run(),
        }
    }
    let _gpu = GPU_GATE.lock().unwrap_or_else(PoisonError::into_inner);
    job.run()
}

/// A receiver shared by a stage's threads (dropped with the last of them, so the stage before
/// sees the channel close).
type Shared<T> = Arc<Mutex<Receiver<T>>>;

fn next<T>(rx: &Shared<T>) -> Option<T> {
    rx.lock().unwrap_or_else(PoisonError::into_inner).recv().ok()
}

fn named<'s, F: FnOnce() + Send + 's>(sc: &'s std::thread::Scope<'s, '_>, name: String, f: F) {
    let started = std::thread::Builder::new().name(name).spawn_scoped(sc, move || {
        lightcraft_sysmem::set_current_thread_class(ThreadClass::Utility);
        f()
    });
    if let Err(e) = started {
        log::warn!("export: could not start a pipeline thread ({e})");
    }
}

/// Start the feeder, render and encode threads for `work` (position, photo) in scope `sc`;
/// returns the channel the finished photos arrive on (in any order, each with its position).
/// Every stage stops at its next item once `stopped` says so; dropping the receiver also ends
/// them.
pub(crate) fn spawn<'s, 'e: 's>(
    sc: &'s std::thread::Scope<'s, 'e>,
    work: Vec<(usize, PreparedExport)>,
    shape: Shape,
    permits: &'e HeavyJobs,
    flow: &'e Flow,
    stopped: &'e (dyn Fn() -> bool + Sync),
    ctl: &'e lightcraft_sysmem::JobCtl,
) -> Receiver<Output<'e>> {
    type Admitted<'p> = (usize, PreparedExport, Permit<'p>);
    type Developed<'p> = (usize, Result<(String, super::Developed), String>, Permit<'p>);
    let (adm_tx, adm_rx): (SyncSender<Admitted<'e>>, _) = sync_channel(1);
    let (dev_tx, dev_rx): (SyncSender<Developed<'e>>, _) = sync_channel(1);
    let (out_tx, out_rx) = sync_channel::<Output<'e>>(1);

    // feeder: admit in order, within the window, each with a permit sized from its pixels
    named(sc, "export-feed".into(), move || {
        for (seq, item) in work {
            if !flow.admit(seq, shape.window, stopped) {
                break;
            }
            let Some(permit) = permits.admit(item.estimated_bytes(), stopped) else { break };
            if adm_tx.send((seq, item, permit)).is_err() {
                break;
            }
        }
    });
    let adm_rx: Shared<Admitted<'e>> = Arc::new(Mutex::new(adm_rx));
    for n in 0..shape.renderers.max(1) {
        let (rx, tx) = (adm_rx.clone(), dev_tx.clone());
        named(sc, format!("export-render-{n}"), move || {
            while let Some((seq, item, mut permit)) = next(&rx) {
                if stopped() {
                    continue; // dropped: its permit is released
                }
                let r = crate::guard::catch("Export", || item.develop(ctl)).and_then(|r| r);
                // the render's working memory is free now; the result waits for an encoder
                permit.finish_heavy(r.as_ref().map_or(0, |(_, d)| d.reserved_bytes()));
                if tx.send((seq, r, permit)).is_err() {
                    break;
                }
            }
        });
    }
    drop((adm_rx, dev_tx));
    let dev_rx: Shared<Developed<'e>> = Arc::new(Mutex::new(dev_rx));
    for n in 0..shape.encoders.max(1) {
        let (rx, tx) = (dev_rx.clone(), out_tx.clone());
        named(sc, format!("export-encode-{n}"), move || {
            while let Some((seq, r, mut permit)) = next(&rx) {
                if stopped() {
                    continue;
                }
                let r = r.and_then(|(name, d)| crate::guard::catch("Export", || super::encode_developed(name, d, ctl)).and_then(|r| r));
                permit.shrink(r.as_ref().map_or(0, |e| (e.bytes.len() + e.sidecars.iter().map(|(_, b)| b.len()).sum::<usize>()) as u64));
                if tx.send((seq, r, permit)).is_err() {
                    break;
                }
            }
        });
    }
    out_rx
}
