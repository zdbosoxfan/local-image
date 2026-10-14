//! Cooperative cancellation and progress for long operations (background jobs, #210).
//!
//! Heavy algorithms take an [`Interrupt`] and check [`Interrupt::cancelled`] once per tile, row
//! band or iteration, returning early (typically `None`) when it is set. They may also report
//! [`Interrupt::progress`] in `0.0..=1.0`. [`Interrupt::NONE`] never cancels and ignores progress,
//! so synchronous callers pass it and get the old behaviour.
//!
//! The callbacks are `Sync`, so rayon workers can check them while filtering tiles in parallel.
//!
//! These are the app's one job primitive, `lightcraft_sysmem::jobs` (duplicate-systems audit D14),
//! re-exported here so pixel code keeps its imports. A job's owned handle is
//! [`lightcraft_sysmem::JobCtl`]; `JobCtl::interrupt()` gives the view to pass in here.

pub use lightcraft_sysmem::jobs::{Cancelled, Interrupt};

#[cfg(test)]
mod tests {
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
