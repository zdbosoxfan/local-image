//! Local Image's AI services, independent of the editor: a ComfyUI client, the model catalog and
//! workflow graphs carried over from Local Image 0.7, verified model downloads, the generated-image
//! library and ComfyUI setup helpers. Everything here is blocking and meant for worker threads.

pub mod catalog;
pub mod comfy;
pub mod download;
pub mod imaging;
pub mod library;
pub mod mock;
pub mod ops;
pub mod settings;
pub mod setup;
pub mod workflows;

pub use catalog::ModelId;
pub use comfy::{Cancelled, ComfyClient, JobControl, Progress, Stage};
pub use ops::{Ai, GenerateMode, GenerateRequest, RemoveEngine};
pub use settings::AiSettings;
