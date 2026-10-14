//! Device health (#243): notices a lost wgpu device or uncaptured wgpu errors so callers stop
//! issuing GPU work and fall back to the CPU compositor instead of panicking.
//!
//! A device loss requires a replacement device. Retry/backoff lives on the same health object
//! as strike/decay; it never makes a lost device usable again. An uncaptured error (a validation or internal error, out of memory)
//! is not on its own: it is logged and counted, the operation that raised it is redone on the
//! CPU by its caller (compare [`DeviceHealth::errors`] before and after), and only errors in
//! [`STRIKE_LIMIT`] separate frames (see [`DeviceHealth::tick`]) without [`DECAY_FRAMES`] clean
//! frames in between mark the device unusable.
//!
//! wgpu's default uncaptured-error handler panics ("Buffer 'pc_compose_uniforms' is invalid"
//! after the OS killed a huge submission), and `Device::poll` panics on a lost device whatever
//! the handler. [`DeviceHealth::watch`] replaces the handler with one that records the fault,
//! registers the device-lost callback, and [`DeviceHealth::wait`] polls only while the device is
//! healthy (catching the poll's panic as a last resort). Every GPU entry point checks
//! [`DeviceHealth::fault`] first.
//!
//! Builds for the web too: the callbacks only touch atomics and a mutex.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use web_time::{Duration, Instant};

/// Why the GPU can't be used any more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The device was lost (driver reset, the OS killed a submission, `Device::destroy`).
    Lost(String),
    /// An uncaptured wgpu error (out of memory, validation, internal): the GPU output can't be
    /// trusted, and on most drivers the device is about to be lost anyway.
    Error(String),
}

impl Fault {
    /// Whether the device itself was lost (rather than an error reported).
    pub fn is_lost(&self) -> bool {
        matches!(self, Fault::Lost(_))
    }

    /// The driver's or wgpu's message.
    pub fn detail(&self) -> &str {
        match self {
            Fault::Lost(s) | Fault::Error(s) => s,
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fault::Lost(s) if s.is_empty() => f.write_str("GPU device lost"),
            Fault::Lost(s) => write!(f, "GPU device lost: {s}"),
            Fault::Error(s) => write!(f, "GPU error: {s}"),
        }
    }
}

/// Frames with uncaptured errors (without [`DECAY_FRAMES`] clean frames in between) after which
/// the device counts as unusable ([`Fault::Error`]).
pub const STRIKE_LIMIT: u64 = 3;
/// Clean frames after which earlier error frames are forgotten.
pub const DECAY_FRAMES: u64 = 600;

#[derive(Clone, Default)]
struct Retry {
    until: Option<Instant>,
    failures: u32,
    epoch: u64,
}

#[derive(Default)]
struct Inner {
    faulted: AtomicBool,
    fault: Mutex<Option<Fault>>,
    errors: AtomicU64,
    /// Frame counter advanced by [`DeviceHealth::tick`].
    /// (current frame, strikes since decay, last error frame + 1).
    strike_state: Mutex<(u64, u64, u64)>,
    last_error: Mutex<Option<String>>,
    retry: Mutex<Retry>,
}

/// Shared health flag of one wgpu device. Cheap to clone; every clone sees the same state.
#[derive(Clone, Default)]
pub struct DeviceHealth(Arc<Inner>);

impl fmt::Debug for DeviceHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceHealth").field("fault", &self.fault()).field("errors", &self.errors()).finish()
    }
}

impl DeviceHealth {
    /// A healthy flag not attached to a device (tests, CPU-only callers).
    pub fn new() -> Self {
        Self::default()
    }

    /// Watch `device`: its device-lost callback and uncaptured errors mark the returned flag
    /// instead of panicking. Replaces any handler set before (wgpu keeps one of each).
    pub fn watch(device: &wgpu::Device) -> Self {
        let health = Self::new();
        health.attach(device);
        health
    }

    /// Attach this health (including replacement/backoff history) to a newly created device.
    /// Call once per device. Late callbacks from its predecessor affect only that predecessor.
    pub fn attach(&self, device: &wgpu::Device) {
        let health = self;
        let lost = health.clone();
        // Called by wgpu with its own locks held: only record, never call back into wgpu.
        device.set_device_lost_callback(move |reason, message| {
            let detail = match (reason, message.is_empty()) {
                (wgpu::DeviceLostReason::Destroyed, true) => "device destroyed".to_string(),
                (_, true) => "lost by the driver".to_string(),
                (_, false) => message,
            };
            log::error!("wgpu device lost ({reason:?}): {detail}");
            lost.mark(Fault::Lost(detail));
        });
        let errors = health.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| errors.uncaptured(&e)));
    }

    /// Record an uncaptured wgpu error. Errors explicitly naming a lost/destroyed device can
    /// precede its loss callback (it fires on the next poll): those mark the device lost.
    /// Other errors are strikes for the current frame (see [`STRIKE_LIMIT`]).
    pub fn uncaptured(&self, e: &wgpu::Error) {
        self.error_text(&e.to_string());
    }

    fn error_text(&self, text: &str) {
        self.0.errors.fetch_add(1, Ordering::Relaxed);
        log::error!("uncaptured wgpu error: {text}");
        let line = first_line(text);
        *self.0.last_error.lock().unwrap_or_else(PoisonError::into_inner) = Some(line.clone());
        let lower = text.to_ascii_lowercase();
        let device_loss = lower.lines().map(str::trim).any(|line| {
            // A destroyed texture/buffer is recoverable even when the error context names
            // Device::create_bind_group. Match the diagnostic's subject, not separate words.
            let state = line.strip_prefix("parent device ").or_else(|| line.strip_prefix("device "));
            state.is_some_and(|state| {
                ["is lost", "was lost", "has been lost", "is destroyed", "was destroyed", "has been destroyed"].iter().any(|p| state.starts_with(p))
            })
        });
        if device_loss {
            self.mark(Fault::Lost(line));
            return;
        }
        // One strike per frame, however many errors the frame raised (an invalid object makes
        // every later use of it an error too).
        let mut state = self.0.strike_state.lock().unwrap_or_else(PoisonError::into_inner);
        let frame = state.0.saturating_add(1);
        if state.2 != frame {
            state.2 = frame;
            state.1 = state.1.saturating_add(1);
            let strikes = state.1;
            drop(state);
            self.schedule_retry(Instant::now());
            if strikes >= STRIKE_LIMIT {
                self.mark(Fault::Error(format!("{line} (errors in {strikes} frames)")));
            } else {
                log::warn!("GPU error {strikes} of {STRIKE_LIMIT}; the affected refresh is redone on the CPU");
            }
        }
    }

    /// Advance a UI frame; strike recording and decay share one state transition.
    pub fn tick(&self) {
        let mut state = self.0.strike_state.lock().unwrap_or_else(PoisonError::into_inner);
        state.0 = state.0.saturating_add(1);
        if state.2 != 0 && state.0.saturating_add(1).saturating_sub(state.2) >= DECAY_FRAMES {
            state.1 = 0;
            state.2 = 0;
        }
    }

    /// Frames with errors not yet forgotten (see [`STRIKE_LIMIT`]).
    pub fn strikes(&self) -> u64 {
        self.0.strike_state.lock().unwrap_or_else(PoisonError::into_inner).1
    }

    /// The last uncaptured error's first line.
    pub fn last_error(&self) -> Option<String> {
        self.0.last_error.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Mark the device unusable. The first fault is kept; a later device loss upgrades an error.
    pub fn mark(&self, fault: Fault) {
        let mut slot = self.0.fault.lock().unwrap_or_else(PoisonError::into_inner);
        let replace = match slot.as_ref() {
            None => true,
            Some(old) => !old.is_lost() && fault.is_lost(),
        };
        if replace {
            *slot = Some(fault);
        }
        self.0.faulted.store(true, Ordering::Release);
        drop(slot);
        self.schedule_retry(Instant::now());
    }

    fn schedule_retry(&self, now: Instant) {
        let mut retry = self.0.retry.lock().unwrap_or_else(PoisonError::into_inner);
        if retry.until.is_none() {
            retry.failures = retry.failures.saturating_add(1);
            retry.until = Some(now + Duration::from_secs((2u64 << retry.failures.min(5)).min(60)));
        }
    }

    /// Start a compute operation with the current idle-VRAM observation generation. A new
    /// idle observation can shorten backoff. Never call this to relabel an already paused job.
    pub fn begin_operation(&self, idle_epoch: u64) {
        let mut retry = self.0.retry.lock().unwrap_or_else(PoisonError::into_inner);
        if retry.until.is_none() {
            retry.epoch = idle_epoch;
        }
    }

    /// A captured operation error uses the same one-strike-per-frame policy as uncaptured
    /// errors. Only this operation's output is discarded; repeated faults close the device.
    pub fn operation_failed(&self, reason: &str, idle_epoch: u64) {
        self.begin_operation(idle_epoch);
        self.error_text(reason);
    }

    /// A clean operation resets the retry delay; strike decay still uses DECAY_FRAMES.
    pub fn operation_succeeded(&self) {
        if self.is_ok() {
            *self.0.retry.lock().unwrap_or_else(PoisonError::into_inner) = Retry::default();
        }
    }

    /// Compute admission (displaying a CPU fallback on a healthy device remains permitted).
    pub fn can_render(&self) -> bool {
        self.is_ok() && self.0.retry.lock().unwrap_or_else(PoisonError::into_inner).until.is_none()
    }
    pub fn needs_retry(&self) -> bool {
        !self.is_ok() || self.0.retry.lock().unwrap_or_else(PoisonError::into_inner).until.is_some()
    }
    /// None = only a time-based retry; Some = also retry after a new idle-VRAM observation.
    pub fn retry_ready(&self, now: Instant, idle_epoch: Option<u64>) -> bool {
        let retry = self.0.retry.lock().unwrap_or_else(PoisonError::into_inner);
        retry.until.is_none_or(|until| now >= until || idle_epoch.is_some_and(|epoch| epoch != retry.epoch))
    }
    /// Resume a recoverable operation on the same device. A lost/faulted device stays closed
    /// even when its timer expires; its owner must create a replacement instead.
    pub fn resume_retry(&self, now: Instant, idle_epoch: Option<u64>) -> bool {
        if self.is_ok() && self.retry_ready(now, idle_epoch) {
            self.0.retry.lock().unwrap_or_else(PoisonError::into_inner).until = None;
            true
        } else {
            false
        }
    }
    /// Health for a replacement device. Keeps repeated-failure backoff, resets per-device
    /// strikes and faults, and never shares callback state with the predecessor.
    pub fn replacement(&self) -> Self {
        let new = Self::new();
        let mut retry = self.0.retry.lock().unwrap_or_else(PoisonError::into_inner).clone();
        retry.until = None;
        *new.0.retry.lock().unwrap_or_else(PoisonError::into_inner) = retry;
        new
    }
    /// Test fault injection can request an immediate retry, but cannot revive a lost device.
    #[doc(hidden)]
    pub fn retry_now(&self) {
        self.0.retry.lock().unwrap_or_else(PoisonError::into_inner).until = Some(Instant::now());
        self.resume_retry(Instant::now(), None);
    }

    /// Whether the device may still be used.
    pub fn is_ok(&self) -> bool {
        !self.0.faulted.load(Ordering::Acquire)
    }

    /// Why the device can't be used, if it can't.
    pub fn fault(&self) -> Option<Fault> {
        if self.is_ok() {
            return None;
        }
        self.0.fault.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Uncaptured wgpu errors seen so far. A caller compares it before and after a piece of GPU
    /// work to tell whether that work's output can be trusted.
    pub fn errors(&self) -> u64 {
        self.0.errors.load(Ordering::Relaxed)
    }

    /// Block until `index` (or all submitted work) finished, unless the device is unusable.
    /// Returns whether the device is still healthy afterwards. `Device::poll` panics on a lost
    /// device, so a panic is caught and recorded as a loss (native only; the web has no blocking
    /// wait).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn wait(&self, device: &wgpu::Device, index: Option<wgpu::SubmissionIndex>) -> bool {
        if !self.is_ok() {
            return false;
        }
        let poll = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| device.poll(wgpu::PollType::Wait { submission_index: index, timeout: None })));
        match poll {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => log::warn!("wgpu poll: {e}"),
            Err(_) => self.mark(Fault::Lost("the device stopped responding".into())),
        }
        self.is_ok()
    }

    /// Non-blocking poll (reclaims staging memory), skipped once the device is unusable.
    pub fn poll(&self, device: &wgpu::Device) {
        if !self.is_ok() {
            return;
        }
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| device.poll(wgpu::PollType::Poll))).is_err() {
            self.mark(Fault::Lost("the device stopped responding".into()));
        }
    }
}

fn first_line(s: &str) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut out: String = line.chars().take(200).collect();
    if line.chars().count() > 200 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_fault_wins_but_loss_upgrades_an_error() {
        let h = DeviceHealth::new();
        assert!(h.is_ok());
        assert_eq!(h.fault(), None);
        h.mark(Fault::Error("out of memory".into()));
        assert!(!h.is_ok());
        let c = h.clone();
        c.mark(Fault::Error("second".into()));
        assert_eq!(h.fault(), Some(Fault::Error("out of memory".into())));
        c.mark(Fault::Lost("reset".into()));
        assert_eq!(h.fault(), Some(Fault::Lost("reset".into())));
        c.mark(Fault::Lost("later".into()));
        assert_eq!(h.fault().map(|f| f.to_string()).as_deref(), Some("GPU device lost: reset"));
    }

    #[test]
    fn single_errors_are_strikes_not_faults() {
        let h = DeviceHealth::new();
        // Many errors in one frame are one strike.
        for _ in 0..5 {
            h.error_text("Validation Error: Buffer 'x' is invalid");
        }
        assert!(h.is_ok());
        assert_eq!((h.errors(), h.strikes()), (5, 1));
        assert_eq!(h.last_error().as_deref(), Some("Validation Error: Buffer 'x' is invalid"));
        // Clean frames forget it.
        for _ in 0..DECAY_FRAMES {
            h.tick();
        }
        assert_eq!(h.strikes(), 0);
        // Errors in STRIKE_LIMIT separate frames are a fault.
        for i in 0..STRIKE_LIMIT {
            assert!(h.is_ok(), "fault after {i} strikes");
            h.error_text("Validation Error: bad bind group");
            h.tick();
        }
        assert!(matches!(h.fault(), Some(Fault::Error(ref m)) if m.contains("bad bind group")), "{:?}", h.fault());
        // A lost-device error is final at once.
        let l = DeviceHealth::new();
        l.error_text("Parent device is lost");
        assert!(l.fault().is_some_and(|f| f.is_lost()));
    }

    #[test]
    fn validation_resource_names_do_not_count_as_device_loss() {
        let h = DeviceHealth::new();
        h.error_text("Validation Error: Texture 'lost panorama' is invalid");
        h.error_text("Validation Error: Buffer 'destroyed' is invalid");
        h.error_text("Validation Error\nCaused by:\n  In Device::create_bind_group\n    Texture 'panorama' is destroyed");
        h.error_text("Validation Error\nCaused by:\n  Buffer 'device is lost' is invalid");
        assert!(h.is_ok());
        assert_eq!(h.strikes(), 1);
        h.error_text("Validation Error\nCaused by:\n  Parent device is lost");
        assert!(h.fault().is_some_and(|f| f.is_lost()));
    }

    #[test]
    fn fault_text() {
        assert_eq!(Fault::Lost(String::new()).to_string(), "GPU device lost");
        assert_eq!(Fault::Error("x".into()).to_string(), "GPU error: x");
        assert_eq!(first_line("\n  Buffer 'pc' is invalid\nmore"), "Buffer 'pc' is invalid");
        assert_eq!(first_line(&"a".repeat(300)).chars().count(), 201);
    }
}

#[cfg(test)]
#[test]
fn ds_finding_error_and_decay_are_serialized() {
    for _ in 0..100 {
        let h = DeviceHealth::new();
        h.error_text("validation");
        for _ in 0..DECAY_FRAMES - 1 {
            h.tick();
        }
        std::thread::scope(|scope| {
            scope.spawn(|| h.tick());
            scope.spawn(|| h.error_text("validation"));
        });
        assert!((1..=2).contains(&h.strikes()));
        h.tick();
        h.error_text("validation");
        h.tick();
        h.error_text("validation");
        assert!(!h.is_ok());
        assert_eq!(h.errors(), 4);
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    #[test]
    fn device_health_backoff_idle_epoch_strikes_and_decay() {
        let h = DeviceHealth::new();
        let now = Instant::now();
        h.operation_failed("out of device memory", 3);
        assert!(h.is_ok(), "one operation failure is one strike, not device loss");
        assert!(!h.can_render());
        assert!(!h.retry_ready(now, Some(3)));
        assert!(h.retry_ready(now + Duration::from_secs(5), Some(3)));
        assert!(h.resume_retry(now, Some(4)), "fresh idle VRAM permits early retry");
        assert!(h.can_render());
        assert_eq!(h.strikes(), 1);
        h.operation_succeeded();
        for _ in 0..DECAY_FRAMES {
            h.tick();
        }
        assert_eq!(h.strikes(), 0);
    }
    #[test]
    fn lost_device_stays_closed_and_late_callback_cannot_fault_replacement() {
        let h = DeviceHealth::new();
        h.begin_operation(8);
        h.mark(Fault::Lost("driver reset".into()));
        let after = Instant::now() + Duration::from_secs(61);
        assert!(h.retry_ready(after, Some(8)));
        assert!(!h.resume_retry(after, Some(8)));
        let replacement = h.replacement();
        assert!(replacement.can_render());
        h.mark(Fault::Lost("late callback".into()));
        assert!(replacement.can_render());
        replacement.operation_failed("OOM again", 8);
        assert!(!replacement.retry_ready(Instant::now() + Duration::from_secs(7), Some(8)), "second consecutive failure waits 8 seconds");
    }
}
