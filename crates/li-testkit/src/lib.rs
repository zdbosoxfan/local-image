//! Local Image's one shared UI test toolkit.
//!
//! - [`softpaint`]: a deterministic CPU rasteriser of egui's tessellated output (the Library's
//!   headless screenshots, agents and CLI use it at runtime too).
//! - `kittest` feature: [`SoftpaintRenderer`], an `egui_kittest` renderer built on [`softpaint`],
//!   so the editor's and the Library's kittest screenshots render the same pixels on any machine,
//!   with no GPU or window.
//! - `gpu` feature: [`gpu`], the one strict real-GPU test prelude (Vulkan, or Metal on macOS;
//!   never a software adapter) shared by the GPU parity gates.

pub mod softpaint;

#[cfg(feature = "kittest")]
mod kittest;
#[cfg(feature = "kittest")]
pub use kittest::SoftpaintRenderer;

#[cfg(feature = "gpu")]
pub mod gpu;

#[cfg(test)]
mod golden_tests;
