//! Shared vector-persona data. Geometry stays in `LayerContent::Shape` and paint/layout
//! implementations stay outside pc-doc. Optional additions are grouped on `Layer::vector`
//! to avoid changing every legacy Knot/LiveShape/TextShape literal and exhaustive match.
//! See docs/specs/vector-architecture.md for activation and format-version rules.

pub mod appearance;
pub mod drawing;
pub mod exchange;
pub mod live;
pub mod structure;
pub mod swatches;
pub mod text;

use crate::Path;
use appearance::Appearance;
use drawing::{DesignShape, KnotCorner};
use live::LiveGroup;
use serde::{Deserialize, Serialize};
use structure::Constraints;
use text::PathText;

/// Additive editable metadata, with geometry expressed in document pixels (text in text space).
/// Empty on every old document. Feature packages must update the legacy rendered cache when
/// activating a field; merely storing metadata does not implement its rendering.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VectorData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corners: Vec<KnotCorner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_shape: Option<DesignShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<Appearance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_path: Option<PathText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<LiveGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Constraints>,
    /// Non-destructive Vector Crop, in document space; combined with the existing masks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<Path>,
}

impl VectorData {
    pub fn is_empty(&self) -> bool {
        self.corners.is_empty()
            && self.live_shape.is_none()
            && self.appearance.is_none()
            && self.on_path.is_none()
            && self.live.is_none()
            && self.constraints.is_none()
            && self.clip.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_defaults_are_additive() {
        let old: VectorData = serde_json::from_str("{}").unwrap();
        assert!(old.is_empty());
        assert_eq!(serde_json::to_string(&old).unwrap(), "{}");
    }
}

// Package 15 owned provider source; relocate to pc-doc after the serial merge if desired.
#[path = "../../pc-engine/src/vector_packages/symbols/validation.rs"]
pub mod symbol_validation;
#[path = "../../pc-engine/src/vector_packages/symbols/expansion.rs"]
pub mod symbol_expansion;

pub mod styles;

pub mod effects;
