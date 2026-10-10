//! V8 symbols and constraints. Definitions own ordinary layers, in paint order. V8 adds the
//! Symbol content variant and FORMAT_VERSION 2 together; no instance variant is written yet.
//! Constraints follow Affinity's edge pins and fixed-size choices in parent coordinates.
use crate::{Affine, Layer, Surface};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u64);
#[derive(Clone, Debug, PartialEq)]
pub struct SymbolDef {
    pub id: SymbolId,
    pub name: String,
    pub layers: Vec<Layer>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SymbolInstance {
    pub symbol: SymbolId,
    pub transform: Affine,
    pub cache: Option<Surface>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {
    #[serde(default, skip_serializing_if = "is_false")]
    pub left: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub right: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub top: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bottom: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub fixed_width: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub fixed_height: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesignUnits {
    #[default]
    Pixels,
    Points,
    Millimeters,
    Inches,
}
impl DesignUnits {
    pub fn is_pixels(&self) -> bool {
        *self == Self::Pixels
    }
}
fn is_false(v: &bool) -> bool {
    !*v
}
