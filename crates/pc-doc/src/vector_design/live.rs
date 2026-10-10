//! Live group data informed by VectorCraft crates/doc/src/live.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Keys are the group's bottom-to-top children, avoiding unstable LayerId references.
use crate::Path;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveGroup {
    Blend(BlendSpec),
    Repeat(RepeatSpec),
    Envelope(EnvelopeSpec),
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlendSpec {
    #[serde(default, skip_serializing_if = "is_default_spacing")]
    pub spacing: BlendSpacing,
    #[serde(default, skip_serializing_if = "is_default_orientation")]
    pub orientation: BlendOrientation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spine: Option<Path>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum BlendSpacing {
    #[default]
    SmoothColor,
    Steps(u32),
    Distance(f64),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendOrientation {
    #[default]
    AlignToPage,
    AlignToPath,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepeatSpec {
    Radial { count: u32, center: [f64; 2], angle: f64 },
    Grid { rows: u32, cols: u32, spacing: [f64; 2] },
    Mirror { origin: [f64; 2], angle: f64 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EnvelopeSpec {
    Warp {
        style: String,
        bend: f64,
        horizontal: f64,
        vertical: f64,
    },
    /// Row-major destination mesh, normalized relative to the original content bounds.
    Mesh {
        rows: u32,
        cols: u32,
        points: Vec<[f64; 2]>,
    },
    TopObject {
        path: Path,
    },
}
fn is_default_spacing(v: &BlendSpacing) -> bool {
    *v == BlendSpacing::SmoothColor
}
fn is_default_orientation(v: &BlendOrientation) -> bool {
    *v == BlendOrientation::AlignToPage
}
