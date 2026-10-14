//! V8 symbols and constraints. Definitions own ordinary layers, in paint order. Symbol
//! instances are the `Symbol` content variant.
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

/// Optional drawing scale, where factor=100 represents 1:100.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DrawingScale {
    pub factor: f64,
}
impl Default for DrawingScale {
    fn default() -> Self {
        Self { factor: 1.0 }
    }
}
impl DrawingScale {
    pub fn validate(self) -> Result<(), String> {
        if self.factor.is_finite() && (1e-6..=1e6).contains(&self.factor) { Ok(()) } else { Err("drawing scale must be between 0.000001 and 1000000".into()) }
    }
}
