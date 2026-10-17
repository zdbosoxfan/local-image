//! Supplies the actual scaled-document provider. No duplicate renderer,
//! source module registration, private-module access or identity fallback is used here.
use super::super::{EngineError, Result};
use photocraft_doc::Document;
pub type Projector = fn(&Document, f64) -> Result<Document>;
static PROJECTOR: std::sync::OnceLock<Projector> = std::sync::OnceLock::new();
pub fn install(projector: Projector) -> Result<()> {
    PROJECTOR.set(projector).map_err(|_| EngineError::Other("09 scaled test provider already installed".into()))
}
pub fn project(doc: &Document, scale: f64) -> Result<Document> {
    PROJECTOR.get().ok_or_else(|| EngineError::Other("09 must install its actual scaled-document provider for the 45 GPU gate".into()))?(doc, scale)
}
