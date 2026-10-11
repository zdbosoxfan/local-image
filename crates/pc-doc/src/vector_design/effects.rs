//! Editable path-effect data informed by VectorCraft effects/{distort,stylize,live}.rs
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Geometry and validation live in pc-vecfx. Appearance indices are bottom-to-top.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectTarget {
    #[default]
    Layer,
    Item {
        index: usize,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Points {
    #[default]
    Smooth,
    Corner,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    #[default]
    Rectangle,
    RoundedRectangle,
    Ellipse,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectKind {
    Roughen {
        size: f64,
        relative: bool,
        detail: f64,
        points: Points,
        seed: u64,
    },
    ZigZag {
        size: f64,
        relative: bool,
        ridges: u32,
        points: Points,
    },
    PuckerBloat {
        amount: f64,
    },
    Twist {
        angle: f64,
    },
    /// Percent scale; document-pixel move; positive degrees counterclockwise in y-down space.
    /// Zero copies transforms the source; n copies retains the source and adds n duplicates.
    Transform {
        scale: [f64; 2],
        move_by: [f64; 2],
        angle: f64,
        reflect: [bool; 2],
        copies: u32,
    },
    RoundCorners {
        radius: f64,
    },
    OffsetPath {
        distance: f64,
        join: OffsetJoin,
        miter: f64,
    },
    /// TL, TR, BR, BL in normalized bounding-box space; a collapsed axis uses a one-pixel frame.
    FreeDistort {
        corners: [[f64; 2]; 4],
    },
    /// Extra total width/height around the current geometry bounds, in document pixels.
    ConvertToShape {
        shape: ShapeKind,
        extra: [f64; 2],
        radius: f64,
    },
}
impl EffectKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Roughen { .. } => "Roughen",
            Self::ZigZag { .. } => "Zig Zag",
            Self::PuckerBloat { .. } => "Pucker & Bloat",
            Self::Twist { .. } => "Twist",
            Self::Transform { .. } => "Transform",
            Self::RoundCorners { .. } => "Round Corners",
            Self::OffsetPath { .. } => "Offset Path",
            Self::FreeDistort { .. } => "Free Distort",
            Self::ConvertToShape { .. } => "Convert to Shape",
        }
    }
    pub fn presets() -> Vec<Self> {
        vec![
            Self::Roughen { size: 5., relative: true, detail: 10., points: Points::Smooth, seed: 0 },
            Self::ZigZag { size: 10., relative: false, ridges: 4, points: Points::Corner },
            Self::PuckerBloat { amount: 0. },
            Self::Twist { angle: 0. },
            Self::Transform { scale: [100.; 2], move_by: [0.; 2], angle: 0., reflect: [false; 2], copies: 0 },
            Self::RoundCorners { radius: 10. },
            Self::OffsetPath { distance: 5., join: OffsetJoin::Miter, miter: 4. },
            Self::FreeDistort { corners: [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] },
            Self::ConvertToShape { shape: ShapeKind::Rectangle, extra: [18.; 2], radius: 9. },
        ]
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathEffect {
    #[serde(default, skip_serializing_if = "is_layer")]
    pub target: EffectTarget,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    pub effect: EffectKind,
}
impl PathEffect {
    pub fn new(effect: EffectKind) -> Self {
        Self { target: EffectTarget::Layer, enabled: true, effect }
    }
}
fn yes() -> bool {
    true
}
fn is_true(v: &bool) -> bool {
    *v
}
fn is_layer(v: &EffectTarget) -> bool {
    *v == EffectTarget::Layer
}

/// Appearance owner calls this in the same edit that reorders/deletes paint items.
/// Deleted items lose their effects rather than silently targeting a different paint.
pub fn remap_items(effects: &mut Vec<PathEffect>, old_to_new: &[Option<usize>]) {
    effects.retain_mut(|e| match &mut e.target {
        EffectTarget::Layer => true,
        EffectTarget::Item { index } => match old_to_new.get(*index).copied().flatten() {
            Some(next) => {
                *index = next;
                true
            }
            None => false,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_effects45_data_defaults_roundtrip_and_remap() {
        let e: PathEffect = serde_json::from_str(r#"{"effect":{"kind":"twist","angle":0.0}}"#).unwrap();
        assert!(e.enabled);
        assert_eq!(e.target, EffectTarget::Layer);
        for kind in EffectKind::presets() {
            let e = PathEffect::new(kind);
            assert_eq!(serde_json::from_str::<PathEffect>(&serde_json::to_string(&e).unwrap()).unwrap(), e);
        }
        let mut effects =
            vec![e.clone(), PathEffect { target: EffectTarget::Item { index: 0 }, ..e.clone() }, PathEffect { target: EffectTarget::Item { index: 1 }, ..e }];
        remap_items(&mut effects, &[Some(2), None]);
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[1].target, EffectTarget::Item { index: 2 });
    }
}
