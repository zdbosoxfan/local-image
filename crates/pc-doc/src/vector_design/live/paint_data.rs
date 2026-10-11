//! Data-only Live Paint schema, GPL-3.0-or-later. Include this exact source in pc-doc
//! Native schema shared by the evaluator; no renderer, engine or UI dependencies.
use crate::{Fill, ShapeStroke};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundary {
    pub source: u64,
    pub subpath: usize,
    pub segment: usize,
    pub forward: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<u64>,
    pub boundary: Vec<Boundary>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub branch: u32,
}
fn is_zero(v: &u32) -> bool {
    *v == 0
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gap {
    #[default]
    None,
    Small,
    Medium,
    Large,
}
impl Gap {
    pub fn distance(self) -> f64 {
        match self {
            Self::None => 0.,
            Self::Small => 4.,
            Self::Medium => 8.,
            Self::Large => 16.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacePaint {
    pub signature: Signature,
    pub paint: Option<Fill>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgePaint {
    pub signature: Signature,
    pub stroke: Option<ShapeStroke>,
}
/// Serde schema requested from 00. Ordinary group children remain the editable sources.
/// Paint records whose signatures disappear are retained, allowing Undo/topology restoration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LivePaintSpec {
    #[serde(default, skip_serializing_if = "Gap::is_none")]
    pub gap: Gap,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FacePaint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgePaint>,
}
impl Gap {
    fn is_none(&self) -> bool {
        *self == Self::None
    }
}
