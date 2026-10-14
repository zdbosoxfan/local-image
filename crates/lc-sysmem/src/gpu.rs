//! Shared GPU pressure, fed by ComfyUI on a worker. Remote VRAM never budgets this device.
//! Budgets are byte fractions, including the host RAM limit on unified-memory machines.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub local: bool,
    pub busy: bool,
    pub total: u64,
    pub free: u64,
    pub unified: bool,
}
#[derive(Default)]
struct State {
    sample: Option<(Sample, Instant)>,
    idle_epoch: u64,
    ram_free: u64,
}
static STATE: Mutex<State> = Mutex::new(State { sample: None, idle_epoch: 0, ram_free: 0 });
static WAKER: Mutex<Option<std::sync::Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);

/// Notify the host immediately when a local reservation starts/ends; no GPU or UI work here.
pub fn set_waker(waker: std::sync::Arc<dyn Fn() + Send + Sync>) {
    *WAKER.lock().unwrap_or_else(PoisonError::into_inner) = Some(waker);
}
fn wake() {
    let waker = WAKER.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if let Some(waker) = waker {
        waker();
    }
}
static JOBS: AtomicUsize = AtomicUsize::new(0);

/// Only publish complete queue + memory observations; a failed probe expires naturally.
pub fn publish(sample: Sample) {
    let ram_free = if sample.unified { crate::ram_available() } else { u64::MAX };
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    state.ram_free = ram_free;
    let was_busy = state.sample.is_some_and(|(s, _)| s.local && s.busy);
    if sample.local && !sample.busy && sample.free > sample.total / 4 && was_busy {
        state.idle_epoch = state.idle_epoch.wrapping_add(1);
    }
    state.sample = Some((sample, Instant::now()));
}
/// Forget telemetry on endpoint changes or unsupported (CPU-only) servers.
pub fn clear_sample() {
    STATE.lock().unwrap_or_else(PoisonError::into_inner).sample = None;
}
pub fn idle_epoch() -> u64 {
    STATE.lock().unwrap_or_else(PoisonError::into_inner).idle_epoch
}
pub fn sample() -> Option<Sample> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner).sample.filter(|(_, at)| at.elapsed() < Duration::from_secs(10)).map(|(s, _)| s)
}
pub fn pressured() -> bool {
    if JOBS.load(Ordering::Acquire) > 0 {
        return true;
    }
    let state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    state.sample.is_some_and(|(sample, at)| at.elapsed() < Duration::from_secs(10) && pressure(sample, state.ram_free))
}
fn pressure(sample: Sample, ram_free: u64) -> bool {
    let free = if sample.unified { sample.free.min(ram_free) } else { sample.free };
    sample.local && (sample.busy || free < sample.total / 8)
}
/// Each app GPU consumer gets at most a quarter of currently free memory, leaving room for
/// the other consumer, display and transient allocations. Unknown memory retains the old cap.
pub fn cache_budget(requested: u64) -> u64 {
    let Some(mut s) = sample().filter(|s| s.local) else { return if pressured() { 0 } else { requested } };
    s.busy |= JOBS.load(Ordering::Acquire) > 0;
    let ram_free = STATE.lock().unwrap_or_else(PoisonError::into_inner).ram_free;
    budget(requested, s, ram_free)
}
pub fn budget(requested: u64, s: Sample, ram_free: u64) -> u64 {
    if !s.local {
        return requested;
    }
    let free = if s.unified { s.free.min(ram_free) } else { s.free };
    let headroom = s.total / 10;
    requested.min(free.saturating_sub(headroom) / if s.busy { 16 } else { 4 })
}
/// Holds local pressure from before submission until the prompt reaches a terminal state.
/// Network servers use their own VRAM and never hold a local reservation.
pub struct AiJob(bool);
impl AiJob {
    pub fn new(local: bool) -> Self {
        if local {
            JOBS.fetch_add(1, Ordering::AcqRel);
            wake();
        }
        Self(local)
    }
}
impl Drop for AiJob {
    fn drop(&mut self) {
        if self.0 {
            JOBS.fetch_sub(1, Ordering::AcqRel);
            wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_budget_remote_busy_and_unified() {
        let s = Sample { local: true, busy: false, total: 16 << 30, free: 8 << 30, unified: false };
        assert!(budget(4 << 30, s, 1 << 30) < 2 << 30);
        assert!(budget(4 << 30, Sample { busy: true, ..s }, 1 << 30) < budget(4 << 30, s, 1 << 30));
        assert_eq!(budget(4 << 30, Sample { local: false, ..s }, 1), 4 << 30);
        assert_eq!(budget(4 << 30, Sample { unified: true, ..s }, 1 << 30), 0);
        assert!(pressure(Sample { unified: true, ..s }, 1 << 30), "low host RAM closes unified-memory admission");
        assert!(!pressure(s, 1), "discrete VRAM is independent of host RAM");
        assert!(!pressure(Sample { local: false, unified: true, ..s }, 1));
    }
}
