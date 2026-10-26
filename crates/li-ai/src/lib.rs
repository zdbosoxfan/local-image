//! Local Image's AI services, independent of the editor: a ComfyUI client, the model catalog and
//! workflow graphs carried over from Local Image 0.7, verified model downloads, the generated-image
//! library and ComfyUI setup helpers. Everything here is blocking and meant for worker threads.
#![expect(clippy::allow_attributes_without_reason, clippy::let_underscore_must_use, clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed")]
#![cfg_attr(test, expect(clippy::unused_result_ok, reason = "baseline 2026-10-25: unreviewed"))]

pub mod arch;
pub mod browser;
pub mod builders;
pub mod catalog;
pub mod cloud;
pub mod comfy;
pub mod custom;
pub mod discovery;
pub mod download;
pub mod enhance;
pub mod family;
pub mod imaging;
pub mod inpaint;
pub mod inventory;
pub mod library;
pub mod mock;
pub mod mock_hub;
pub mod ops;
pub mod presets;
pub mod refine;
pub mod resize;
pub mod resources;
pub mod restoration;
pub mod settings;
pub mod setup;
pub mod tls;
pub mod trash;
pub mod updates;
pub mod workflows;

pub use catalog::ModelId;
pub use comfy::{ComfyClient, JobControl, Progress, Stage};
pub use ops::{Ai, GenerateMode, GenerateRequest, RefineStep, RemoveEngine};
pub use settings::AiSettings;

/// The AI service as configured: `LOCAL_IMAGE_COMFY` (`host:port`, for tests and demos) or the
/// ComfyUI address in `config.json`.
pub fn service() -> Ai {
    if let Some(h) = SERVICE_OVERRIDE.lock().ok().and_then(|g| g.clone()) {
        return Ai::new(h);
    }
    match std::env::var("LOCAL_IMAGE_COMFY") {
        Ok(h) if !h.trim().is_empty() => Ai::new(h.trim()),
        _ => Ai::new(AiSettings::load().host()),
    }
}

static SERVICE_OVERRIDE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Points [`service`] at another ComfyUI (`host:port`) for this process, e.g. the mock server in
/// tests or `--mock-ai`; `None` restores the configured one.
pub fn set_service_override(host: Option<String>) {
    if let Ok(mut g) = SERVICE_OVERRIDE.lock() {
        *g = host;
    }
}
