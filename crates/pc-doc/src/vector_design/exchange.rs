//! Shared file-adapter contract for SVG, PDF and Affinity packages. No filesystem, renderer,
//! fonts or UI live here. Readers consume caller-provided bytes and return a detached scene;
//! engine commands publish it once. Warnings are aggregated by the caller for one UI notice.
use crate::{DesignUnits, Layer, LayerId, Size, SymbolDef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorWarning {
    pub feature: String,
    pub object: Option<String>,
    pub message: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportLimits {
    pub bytes: usize,
    pub pixels: u64,
    pub objects: usize,
    pub depth: usize,
    pub segments: usize,
    pub decompressed_bytes: u64,
}
impl Default for ImportLimits {
    fn default() -> Self {
        Self { bytes: 64 << 20, pixels: 100_000_000, objects: 100_000, depth: crate::MAX_GROUP_DEPTH, segments: 8_000_000, decompressed_bytes: 512 << 20 }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ImportOptions {
    pub dpi: f32,
    pub limits: ImportLimits,
    pub pages: Vec<usize>,
}
impl Default for ImportOptions {
    fn default() -> Self {
        Self { dpi: 72.0, limits: ImportLimits::default(), pages: Vec::new() }
    }
}
/// Bottom-to-top ordinary layer tree. Unresolved linked resources must be warnings/errors,
/// never fetched implicitly. Page selection is zero-based; an empty list means the first page.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedScene {
    pub size: Size,
    pub resolution_dpi: f32,
    pub units: DesignUnits,
    pub layers: Vec<Layer>,
    pub symbols: Vec<SymbolDef>,
    pub warnings: Vec<VectorWarning>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportScope {
    Document,
    Selection(Vec<LayerId>),
    Artboard(LayerId),
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExportedVector {
    pub bytes: Vec<u8>,
    pub warnings: Vec<VectorWarning>,
}
