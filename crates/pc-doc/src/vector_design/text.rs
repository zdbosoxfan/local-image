//! V6 path text data, following SVG 2 textPath's path/offset/side model.
//! Offsets here are fractions of total arc length; importers convert absolute SVG lengths.
//! Text stays owned by the existing TextLayer; its transform maps this text-space path.
use crate::Path;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathText {
    pub path: Path,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub start: f64,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub end: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    #[serde(default, skip_serializing_if = "is_baseline")]
    pub align: PathTextAlign,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathTextAlign {
    #[default]
    Baseline,
    Ascender,
    Center,
    Descender,
}
fn one() -> f64 {
    1.0
}
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}
fn is_one(v: &f64) -> bool {
    *v == 1.0
}
fn is_false(v: &bool) -> bool {
    !*v
}
fn is_baseline(v: &PathTextAlign) -> bool {
    *v == PathTextAlign::Baseline
}
