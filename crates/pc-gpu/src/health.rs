//! Device health (#243): notices a lost wgpu device or uncaptured wgpu errors so callers stop
//! issuing GPU work and fall back to the CPU compositor instead of panicking.
//!
//! A device loss is final. An uncaptured error (a validation or internal error, out of memory)
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

#[derive(Default)]
struct Inner {
    faulted: AtomicBool,
    fault: Mutex<Option<Fault>>,
    errors: AtomicU64,
    /// Frame counter advanced by [`DeviceHealth::tick`].
    frame: AtomicU64,
    /// Frames with errors since the last decay, and the last such frame (+1; 0 = none).
    strikes: AtomicU64,
    strike_frame: AtomicU64,
    last_error: Mutex<Option<String>>,
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
        health
    }

    /// Record an uncaptured wgpu error. Errors about lost or destroyed objects follow a device
    /// loss whose callback may not have run yet (it fires on the next poll): those mark the
    /// device lost. Any other error is a strike for the current frame (see [`STRIKE_LIMIT`]).
    pub fn uncaptured(&self, e: &wgpu::Error) {
        self.error_text(&e.to_string());
    }

    fn error_text(&self, text: &str) {
        self.0.errors.fetch_add(1, Ordering::Relaxed);
        log::error!("uncaptured wgpu error: {text}");
        let line = first_line(text);
        *self.0.last_error.lock().unwrap_or_else(PoisonError::into_inner) = Some(line.clone());
        let lower = text.to_ascii_lowercase();
        if lower.contains("lost") || lower.contains("destroyed") {
            self.mark(Fault::Lost(line));
            return;
        }
        // One strike per frame, however many errors the frame raised (an invalid object makes
        // every later use of it an error too).
        let frame = self.0.frame.load(Ordering::Acquire) + 1;
        if self.0.strike_frame.swap(frame, Ordering::AcqRel) != frame {
            let strikes = self.0.strikes.fetch_add(1, Ordering::AcqRel) + 1;
            if strikes >= STRIKE_LIMIT {
                self.mark(Fault::Error(format!("{line} (errors in {strikes} frames)")));
            } else {
                log::warn!("GPU error {strikes} of {STRIKE_LIMIT}; the affected refresh is redone on the CPU");
            }
        }
    }

    /// Advance the frame counter (once per UI frame): errors in later frames are separate
    /// strikes, and [`DECAY_FRAMES`] clean frames forget earlier ones.
    pub fn tick(&self) {
        let frame = self.0.frame.fetch_add(1, Ordering::AcqRel) + 1;
        let last = self.0.strike_frame.load(Ordering::Acquire);
        if last != 0 && frame.saturating_sub(last) >= DECAY_FRAMES {
            self.0.strikes.store(0, Ordering::Release);
            self.0.strike_frame.store(0, Ordering::Release);
        }
    }

    /// Frames with errors not yet forgotten (see [`STRIKE_LIMIT`]).
    pub fn strikes(&self) -> u64 {
        self.0.strikes.load(Ordering::Acquire)
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
    fn fault_text() {
        assert_eq!(Fault::Lost(String::new()).to_string(), "GPU device lost");
        assert_eq!(Fault::Error("x".into()).to_string(), "GPU error: x");
        assert_eq!(first_line("\n  Buffer 'pc' is invalid\nmore"), "Buffer 'pc' is invalid");
        assert_eq!(first_line(&"a".repeat(300)).chars().count(), 201);
    }
}
