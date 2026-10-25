//! One device and recovery policy for the window, compositor and Develop compute.
#![forbid(unsafe_code)]
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]
#![cfg_attr(test, expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed"))]

pub mod backend;
pub mod device;
pub mod health;
pub mod memory;
pub mod startup;
pub mod wgsl;
pub use device::{SharedDevice, device_limits, install_window, reserve_window};
pub use health::{DeviceHealth, Fault};

// Serialize tests that configure the process marker with tests that create native devices.
#[cfg(test)]
pub(crate) static TEST_INIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
