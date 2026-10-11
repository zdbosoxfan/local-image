//! Live group data informed by VectorCraft crates/doc/src/live.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Keys are the group's bottom-to-top children, avoiding unstable LayerId references.
use crate::Path;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq)]
pub enum LiveGroup {
    Blend(BlendSpec),
    Repeat(RepeatSpec),
    Envelope(EnvelopeSpec),
    LivePaint(LivePaintSpec),
}

pub mod paint_data;
pub use paint_data::LivePaintSpec;

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LiveGroupWire {
    Blend(BlendSpec),
    Repeat { spec: RepeatSpec },
    Envelope { spec: EnvelopeSpec },
    LivePaint(LivePaintSpec),
}
impl Serialize for LiveGroup {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Blend(spec) => LiveGroupWire::Blend(spec.clone()),
            Self::Repeat(spec) => LiveGroupWire::Repeat { spec: spec.clone() },
            Self::Envelope(spec) => LiveGroupWire::Envelope { spec: spec.clone() },
            Self::LivePaint(spec) => LiveGroupWire::LivePaint(spec.clone()),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for LiveGroup {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ReadWire {
            Current(LiveGroupWire),
            LegacyEnvelope(EnvelopeSpec),
            LegacyRepeat(RepeatSpec),
        }
        Ok(match ReadWire::deserialize(deserializer)? {
            ReadWire::Current(LiveGroupWire::Blend(spec)) => Self::Blend(spec),
            ReadWire::Current(LiveGroupWire::Repeat { spec }) => Self::Repeat(spec),
            ReadWire::Current(LiveGroupWire::Envelope { spec }) => Self::Envelope(spec),
            ReadWire::Current(LiveGroupWire::LivePaint(spec)) => Self::LivePaint(spec),
            ReadWire::LegacyEnvelope(spec) => Self::Envelope(spec),
            ReadWire::LegacyRepeat(spec) => Self::Repeat(spec),
        })
    }
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

#[cfg(test)]
mod integration_tests {
    use super::*;
    #[test]
    fn live_group_tags_roundtrip_and_read_legacy_flat_specs() {
        let specs = [
            LiveGroup::Repeat(RepeatSpec::Radial { count: 9, center: [1., 2.], angle: 0.3 }),
            LiveGroup::Repeat(RepeatSpec::Grid { rows: 3, cols: 4, spacing: [2., 3.] }),
            LiveGroup::Repeat(RepeatSpec::Mirror { origin: [4., 5.], angle: 0.7 }),
            LiveGroup::Envelope(EnvelopeSpec::Warp { style: "arc".into(), bend: 0.2, horizontal: 0.1, vertical: -0.1 }),
            LiveGroup::Envelope(EnvelopeSpec::Mesh { rows: 2, cols: 2, points: vec![[0., 0.], [1., 0.], [0., 1.], [1., 1.]] }),
            LiveGroup::Envelope(EnvelopeSpec::TopObject { path: crate::Path::default() }),
            LiveGroup::Blend(BlendSpec::default()),
            LiveGroup::LivePaint(LivePaintSpec::default()),
        ];
        for spec in specs {
            let value = serde_json::to_value(&spec).unwrap();
            assert_eq!(serde_json::from_value::<LiveGroup>(value.clone()).unwrap(), spec);
            if matches!(spec, LiveGroup::Repeat(_) | LiveGroup::Envelope(_)) {
                assert_eq!(serde_json::from_value::<LiveGroup>(value["spec"].clone()).unwrap(), spec);
            }
        }
    }
}
