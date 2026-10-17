//! One cancel + progress primitive for every long operation in the app.
//!
//! Two views of the same thing:
//!
//! * [`JobCtl`] — the **owned** handle a job and its UI share (clone it; every clone is the same
//!   job). The UI calls [`JobCtl::cancel`] and reads [`JobCtl::fraction`] / [`JobCtl::status`];
//!   the worker checks [`JobCtl::is_cancelled`] / [`JobCtl::check`] and reports with
//!   [`JobCtl::set_fraction`], [`JobCtl::set_step`], [`JobCtl::set_stage`] and
//!   [`JobCtl::set_message`]. The stage type `S` is the job's own (e.g. `li_ai::Stage`); plain jobs
//!   use the default `()`.
//! * [`Interrupt`] — the **borrowed**, `Copy` view that pixel algorithms take. Hot loops call
//!   [`Interrupt::cancelled`] / [`Interrupt::check`] once per tile, row band or iteration and may
//!   report [`Interrupt::progress`]. Get one from a job with [`JobCtl::interrupt`] (lock-free: two
//!   atomics, no callbacks), from a bare `&AtomicBool` with [`Interrupt::flag`], or from callbacks
//!   with [`Interrupt::new`]. [`Interrupt::NONE`] never cancels, for synchronous callers.
//!
//! APIs that take a plain `&AtomicBool` cancel flag (`lightcraft-segment` model downloads, `li-seg`) are fed with
//! [`JobCtl::cancel_flag`], so one cancel reaches every layer of a job.
//!
//! Every stopped operation ends with the one [`Cancelled`] error, so `err.is::<Cancelled>()` /
//! `err.chain().any(..)` recognises a cancel whichever layer noticed it.
//!
//! Progress is a fraction in `0.0..=1.0` (or unknown); [`JobStatus`] adds the stage, an optional
//! `step / max` count and a message for the readout. All updates are thread-safe; the cancel flag
//! and fraction are atomics (no lock in hot loops), the status is behind a mutex that survives a
//! panicking worker.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// The error of an operation that stopped because it was cancelled. Shown as "Cancelled".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// For workers whose errors are plain strings (Develop's AI jobs, export): `job.check()?`.
impl From<Cancelled> for String {
    fn from(c: Cancelled) -> String {
        c.to_string()
    }
}

/// Bit pattern of the stored fraction meaning "unknown" (a NaN, never produced by a store).
const UNKNOWN: u32 = u32::MAX;

fn load_fraction(bits: &AtomicU32) -> Option<f32> {
    let f = f32::from_bits(bits.load(Ordering::Relaxed));
    f.is_finite().then_some(f)
}

/// Store `fraction` clamped to `0.0..=1.0`; NaN and infinities are ignored.
fn store_fraction(bits: &AtomicU32, fraction: f32) {
    if fraction.is_finite() {
        bits.store(fraction.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
}

/// What a job is doing, for a progress readout. `S` is the job's stage type.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JobStatus<S> {
    pub stage: S,
    /// Step `value / max` (sampling steps, photos done…), when the job counts.
    pub step: Option<(u32, u32)>,
    pub message: String,
}

impl<S> JobStatus<S> {
    /// `step` as a fraction (0..1) when known.
    pub fn fraction(&self) -> Option<f32> {
        self.step.filter(|s| s.1 > 0).map(|(v, m)| v as f32 / m as f32)
    }
}

struct Shared<S> {
    cancel: AtomicBool,
    fraction: AtomicU32,
    status: Mutex<JobStatus<S>>,
}

/// Cancellation in, progress out: the handle a long job and its UI share. Cloning is cheap and
/// every clone controls the same job. See the [module docs](self).
pub struct JobCtl<S = ()> {
    shared: Arc<Shared<S>>,
}

impl<S> Clone for JobCtl<S> {
    fn clone(&self) -> Self {
        Self { shared: Arc::clone(&self.shared) }
    }
}

impl<S: Default> Default for JobCtl<S> {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared { cancel: AtomicBool::new(false), fraction: AtomicU32::new(UNKNOWN), status: Mutex::new(JobStatus::default()) }),
        }
    }
}

impl<S> std::fmt::Debug for JobCtl<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobCtl").field("cancelled", &self.is_cancelled()).field("fraction", &self.fraction()).finish()
    }
}

impl JobCtl {
    /// A job without stages, not cancelled, progress unknown. (Jobs with a stage type use
    /// `JobCtl::<MyStage>::default()`.)
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S> JobCtl<S> {
    /// Ask the job to stop. Workers see it at their next check; it cannot be undone.
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::SeqCst);
    }

    /// Has cancellation been requested?
    pub fn is_cancelled(&self) -> bool {
        self.shared.cancel.load(Ordering::SeqCst)
    }

    /// `Err(Cancelled)` once cancelled, for `?` in workers.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() { Err(Cancelled) } else { Ok(()) }
    }

    /// The cancel flag itself, for APIs that take `&AtomicBool` (`lightcraft-segment` model downloads, `li-seg`).
    /// Only read it; cancel through [`JobCtl::cancel`].
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.shared.cancel
    }

    /// The borrowed view algorithms take: cancel checks and fraction reports go straight to this
    /// job's atomics (no callbacks, no lock).
    pub fn interrupt(&self) -> Interrupt<'_> {
        Interrupt { source: Source::Atomics { cancel: &self.shared.cancel, fraction: Some(&self.shared.fraction) } }
    }

    /// Overall progress in `0.0..=1.0`, `None` while unknown.
    pub fn fraction(&self) -> Option<f32> {
        load_fraction(&self.shared.fraction)
    }

    /// Report overall progress (`0.0..=1.0`; clamped, NaN ignored).
    pub fn set_fraction(&self, fraction: f32) {
        store_fraction(&self.shared.fraction, fraction);
    }

    /// Report overall progress that never goes backwards: a lower value than already reported is
    /// ignored (several threads reporting parts of one job; the editor's job runner). Clamped,
    /// NaN ignored.
    pub fn advance(&self, fraction: f32) {
        if fraction.is_finite() {
            // Non-negative floats order like their bits, and "unknown" (`u32::MAX`) is replaced
            // by the first report.
            let bits = fraction.clamp(0.0, 1.0).to_bits();
            let _ = self.shared.fraction.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| (old == UNKNOWN || bits > old).then_some(bits));
        }
    }

    /// Report progress with a short status, the way long jobs do between and inside stages: the
    /// fraction never goes backwards ([`JobCtl::advance`]) and an empty `message` keeps the
    /// previous one.
    pub fn report(&self, fraction: f32, message: &str) {
        self.advance(fraction);
        if !message.is_empty() {
            let mut status = self.lock();
            if status.message != message {
                message.clone_into(&mut status.message);
            }
        }
    }

    /// Run `f` with an algorithm-level [`Interrupt`] whose progress maps onto `lo..hi` of this
    /// job (shown with `message`) and whose cancellation is this job's.
    pub fn stage<R>(&self, lo: f32, hi: f32, message: &str, f: impl FnOnce(&Interrupt) -> R) -> R
    where
        S: Send,
    {
        self.report(lo, message);
        let cancel = || self.is_cancelled();
        let progress = |p: f32| self.report(lo + (hi - lo) * p, "");
        f(&Interrupt::new(&cancel, &progress))
    }

    /// Report step `value / max`; also sets the fraction (unknown when `max` is 0).
    pub fn set_step(&self, value: u32, max: u32) {
        let mut status = self.lock();
        status.step = Some((value, max));
        match status.fraction() {
            Some(f) => store_fraction(&self.shared.fraction, f),
            None => self.shared.fraction.store(UNKNOWN, Ordering::Relaxed),
        }
    }

    /// Count `done` more steps of the current `step` (one more photo finished), safe from several
    /// workers at once; also sets the fraction. Without a step yet, counts from 0 of an unknown
    /// total.
    pub fn add_step(&self, done: u32) {
        let mut status = self.lock();
        let (value, max) = status.step.unwrap_or((0, 0));
        status.step = Some((value.saturating_add(done), max));
        match status.fraction() {
            Some(f) => store_fraction(&self.shared.fraction, f),
            None => self.shared.fraction.store(UNKNOWN, Ordering::Relaxed),
        }
    }

    /// Replace the message shown with the progress.
    pub fn set_message(&self, message: impl Into<String>) {
        self.lock().message = message.into();
    }

    /// Enter `stage`. A different stage starts its own count: step and fraction become unknown.
    pub fn set_stage(&self, stage: S)
    where
        S: PartialEq,
    {
        let mut status = self.lock();
        if status.stage != stage {
            status.step = None;
            self.shared.fraction.store(UNKNOWN, Ordering::Relaxed);
        }
        status.stage = stage;
    }

    /// A snapshot of stage, step and message.
    pub fn status(&self) -> JobStatus<S>
    where
        S: Clone,
    {
        self.lock().clone()
    }

    /// The status survives a worker that panicked while holding the lock.
    fn lock(&self) -> MutexGuard<'_, JobStatus<S>> {
        self.shared.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Cancellation check plus progress sink for one long operation, borrowed for the duration of a
/// call. Cheap to copy; `Sync`, so rayon workers can check it while filtering tiles in parallel.
#[derive(Clone, Copy)]
pub struct Interrupt<'a> {
    source: Source<'a>,
}

#[derive(Clone, Copy)]
enum Source<'a> {
    Callbacks { cancel: &'a (dyn Fn() -> bool + Sync), progress: &'a (dyn Fn(f32) + Sync) },
    Atomics { cancel: &'a AtomicBool, fraction: Option<&'a AtomicU32> },
}

fn never() -> bool {
    false
}

fn ignore(_: f32) {}

impl std::fmt::Debug for Interrupt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interrupt").field("cancelled", &self.cancelled()).finish()
    }
}

impl Default for Interrupt<'_> {
    fn default() -> Self {
        Interrupt::NONE
    }
}

impl<'a> Interrupt<'a> {
    /// Never cancelled; progress is ignored.
    pub const NONE: Interrupt<'static> = Interrupt { source: Source::Callbacks { cancel: &never, progress: &ignore } };

    /// An interrupt from a cancellation check and a progress sink (`0.0..=1.0`).
    pub fn new(cancel: &'a (dyn Fn() -> bool + Sync), progress: &'a (dyn Fn(f32) + Sync)) -> Self {
        Self { source: Source::Callbacks { cancel, progress } }
    }

    /// Only a cancellation check (progress is ignored).
    pub fn cancel_only(cancel: &'a (dyn Fn() -> bool + Sync)) -> Self {
        Self { source: Source::Callbacks { cancel, progress: &ignore } }
    }

    /// Only a cancel flag (progress is ignored), for callers that hold a plain `&AtomicBool`.
    pub fn flag(cancel: &'a AtomicBool) -> Self {
        Self { source: Source::Atomics { cancel, fraction: None } }
    }

    /// Has the operation been cancelled? Algorithms stop as soon as they see `true`.
    pub fn cancelled(&self) -> bool {
        match self.source {
            Source::Callbacks { cancel, .. } => cancel(),
            Source::Atomics { cancel, .. } => cancel.load(Ordering::SeqCst),
        }
    }

    /// Report progress (`0.0..=1.0`; values outside are clamped, NaN is ignored).
    pub fn progress(&self, fraction: f32) {
        if !fraction.is_finite() {
            return;
        }
        match self.source {
            Source::Callbacks { progress, .. } => progress(fraction.clamp(0.0, 1.0)),
            Source::Atomics { fraction: Some(bits), .. } => store_fraction(bits, fraction),
            Source::Atomics { fraction: None, .. } => {}
        }
    }

    /// `Err(Cancelled)` once cancelled, for `?` in loops.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.cancelled() { Err(Cancelled) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobctl_clones_share_cancel_and_progress() {
        let job = JobCtl::new();
        let worker = job.clone();
        assert_eq!(job.fraction(), None);
        assert_eq!(worker.check(), Ok(()));
        worker.set_fraction(0.25);
        assert_eq!(job.fraction(), Some(0.25));
        worker.set_fraction(f32::NAN);
        assert_eq!(job.fraction(), Some(0.25));
        worker.set_fraction(7.0);
        assert_eq!(job.fraction(), Some(1.0));
        job.cancel();
        assert!(worker.is_cancelled());
        assert!(worker.cancel_flag().load(Ordering::Relaxed));
        assert_eq!(worker.check(), Err(Cancelled));
        assert_eq!(Cancelled.to_string(), "Cancelled");
    }

    #[test]
    fn jobctl_advance_never_goes_backwards() {
        let job = JobCtl::new();
        job.advance(f32::NAN);
        assert_eq!(job.fraction(), None);
        job.advance(0.0);
        assert_eq!(job.fraction(), Some(0.0), "the first report replaces unknown");
        let views = [job.clone(), job.clone(), job.clone(), job.clone()];
        std::thread::scope(|s| {
            for (i, v) in views.iter().enumerate() {
                s.spawn(move || {
                    for k in 0..=100 {
                        v.advance((k as f32 / 100.0) * (i + 1) as f32 / 4.0);
                    }
                });
            }
        });
        assert_eq!(job.fraction(), Some(1.0));
        job.advance(0.5);
        assert_eq!(job.fraction(), Some(1.0));
        job.set_fraction(0.25);
        assert_eq!(job.fraction(), Some(0.25), "set_fraction still moves freely");
    }

    #[test]
    fn jobctl_report_and_stage_map_progress_into_their_range() {
        let job = JobCtl::new();
        job.report(0.5, "Decoding");
        job.report(0.25, "");
        assert_eq!((job.fraction(), job.status().message.as_str()), (Some(0.5), "Decoding"));
        let r = job.stage(0.5, 1.0, "Filtering", |i| {
            i.progress(0.5);
            i.check()
        });
        assert_eq!(r, Ok(()));
        assert_eq!((job.fraction(), job.status().message.as_str()), (Some(0.75), "Filtering"));
        job.cancel();
        assert_eq!(job.stage(0.0, 1.0, "", |i| i.check()), Err(Cancelled));
        let text: Result<(), String> = (|| Ok(job.check()?))();
        assert_eq!(text, Err("Cancelled".to_string()));
    }

    #[test]
    fn jobctl_add_step_counts_from_parallel_workers() {
        let job = JobCtl::new();
        job.set_step(0, 400);
        std::thread::scope(|s| {
            for _ in 0..4 {
                let job = &job;
                s.spawn(move || (0..100).for_each(|_| job.add_step(1)));
            }
        });
        assert_eq!(job.status().step, Some((400, 400)));
        assert_eq!(job.fraction(), Some(1.0));
        let fresh = JobCtl::new();
        fresh.add_step(2);
        assert_eq!((fresh.status().step, fresh.fraction()), (Some((2, 0)), None));
    }

    #[test]
    fn jobctl_stage_step_and_message() {
        #[derive(Clone, Debug, Default, PartialEq)]
        enum Stage {
            #[default]
            Loading,
            Running,
        }
        let job = JobCtl::<Stage>::default();
        job.set_step(3, 4);
        assert_eq!(job.fraction(), Some(0.75));
        assert_eq!(job.status().fraction(), Some(0.75));
        job.set_stage(Stage::Loading);
        assert_eq!(job.status().step, Some((3, 4)), "same stage keeps its count");
        job.set_stage(Stage::Running);
        job.set_message("photo 1");
        assert_eq!(job.status(), JobStatus { stage: Stage::Running, step: None, message: "photo 1".into() });
        assert_eq!(job.fraction(), None, "a new stage starts with unknown progress");
        job.set_step(1, 0);
        assert_eq!(job.fraction(), None);
    }

    #[test]
    fn jobctl_interrupt_view_reaches_the_job() {
        let job = JobCtl::new();
        let view = job.interrupt();
        let views = [view; 4];
        std::thread::scope(|s| {
            for (i, v) in views.iter().enumerate() {
                s.spawn(move || v.progress(i as f32 / 4.0));
            }
        });
        assert!(job.fraction().is_some());
        view.progress(-1.0);
        assert_eq!(job.fraction(), Some(0.0));
        assert!(!view.cancelled());
        job.cancel();
        assert!(view.cancelled());
        assert_eq!(view.check(), Err(Cancelled));

        let flag = AtomicBool::new(false);
        let flagged = Interrupt::flag(&flag);
        flagged.progress(0.5);
        assert!(!flagged.cancelled());
        flag.store(true, Ordering::SeqCst);
        assert_eq!(flagged.check(), Err(Cancelled));
    }

    #[test]
    fn jobctl_status_survives_a_panicking_worker() {
        let job = JobCtl::new();
        let worker = job.clone();
        let _ = std::thread::spawn(move || {
            let _guard = worker.lock();
            std::panic::resume_unwind(Box::new("worker failed"));
        })
        .join();
        job.set_message("still readable");
        assert_eq!(job.status().message, "still readable");
    }

    #[test]
    fn interrupt_none_and_callbacks() {
        assert!(!Interrupt::NONE.cancelled());
        Interrupt::NONE.progress(0.5);
        assert_eq!(Interrupt::default().check(), Ok(()));
        let flag = AtomicBool::new(false);
        let last = AtomicU32::new(0);
        let cancel = || flag.load(Ordering::Relaxed);
        let progress = |f: f32| last.store(f.to_bits(), Ordering::Relaxed);
        let i = Interrupt::new(&cancel, &progress);
        i.progress(2.0);
        assert_eq!(f32::from_bits(last.load(Ordering::Relaxed)), 1.0);
        flag.store(true, Ordering::Relaxed);
        assert_eq!(i.check(), Err(Cancelled));
        assert!(Interrupt::cancel_only(&cancel).cancelled());
    }
}

#[cfg(test)]
mod interrupt_tests {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use super::*;

    #[test]
    fn none_never_cancels() {
        assert!(!Interrupt::NONE.cancelled());
        Interrupt::NONE.progress(0.5);
        assert_eq!(Interrupt::NONE.check(), Ok(()));
    }

    #[test]
    fn reports_cancel_and_clamped_progress() {
        let flag = AtomicBool::new(false);
        let last = AtomicU32::new(0);
        let cancel = || flag.load(Ordering::Relaxed);
        let progress = |f: f32| last.store(f.to_bits(), Ordering::Relaxed);
        let i = Interrupt::new(&cancel, &progress);
        i.progress(2.0);
        assert_eq!(f32::from_bits(last.load(Ordering::Relaxed)), 1.0);
        i.progress(f32::NAN);
        assert_eq!(f32::from_bits(last.load(Ordering::Relaxed)), 1.0);
        assert!(!i.cancelled());
        flag.store(true, Ordering::Relaxed);
        assert!(i.cancelled());
        assert_eq!(i.check(), Err(Cancelled));
    }
}
