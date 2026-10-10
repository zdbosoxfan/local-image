//! Appearance data adapted to Local Image's Fill/ShapeStroke model, informed by VectorCraft
//! crates/doc/src/appearance.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Paint order is bottom to top. An explicit empty stack means no paint; None means legacy paint.

use crate::{BlendMode, Fill, ShapeStroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<AppearanceItem>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppearanceItem {
    Fill(FillItem),
    Stroke(StrokeItem),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FillItem {
    pub paint: Fill,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f32,
    #[serde(default = "normal", skip_serializing_if = "is_normal")]
    pub blend: BlendMode,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub visible: bool,
}
impl FillItem {
    pub fn new(paint: Fill) -> Self {
        Self { paint, opacity: 1.0, blend: BlendMode::Normal, visible: true }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrokeItem {
    pub base: ShapeStroke,
    #[serde(skip_serializing_if = "is_normal")]
    pub blend: BlendMode,
    #[serde(skip_serializing_if = "is_true")]
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<WidthProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brush: Option<BrushRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_arrow: Option<Arrowhead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_arrow: Option<Arrowhead>,
    #[serde(skip_serializing_if = "is_arrow_scale")]
    pub arrow_scale: [f64; 2],
    #[serde(skip_serializing_if = "is_false")]
    pub dash_align: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub scale_with_object: bool,
    #[serde(skip_serializing_if = "is_object_gradient")]
    pub gradient_mode: StrokeGradientMode,
}
impl Default for StrokeItem {
    fn default() -> Self {
        Self {
            base: ShapeStroke::default(),
            blend: BlendMode::Normal,
            visible: true,
            profile: None,
            brush: None,
            start_arrow: None,
            end_arrow: None,
            arrow_scale: [1.0; 2],
            dash_align: false,
            scale_with_object: false,
            gradient_mode: StrokeGradientMode::Object,
        }
    }
}
/// t is normalized arc length, left/right are nonnegative distances in document pixels.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WidthProfile {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<WidthPoint>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WidthPoint {
    pub t: f64,
    pub left: f64,
    pub right: f64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrushRef {
    pub id: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arrowhead {
    Triangle,
    TriangleOpen,
    Circle,
    CircleOpen,
    Square,
    SquareOpen,
    Bar,
    Diamond,
    Arrow,
    ArrowOpen,
    Barbed,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeGradientMode {
    #[default]
    Object,
    Along,
    Across,
}

fn normal() -> BlendMode {
    BlendMode::Normal
}
fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}
fn is_one(x: &f32) -> bool {
    *x == 1.0
}
fn is_normal(x: &BlendMode) -> bool {
    *x == BlendMode::Normal
}
fn is_true(x: &bool) -> bool {
    *x
}
fn is_false(x: &bool) -> bool {
    !*x
}
fn is_arrow_scale(x: &[f64; 2]) -> bool {
    *x == [1.0; 2]
}
fn is_object_gradient(x: &StrokeGradientMode) -> bool {
    *x == StrokeGradientMode::Object
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_appearance_missing_options_use_editable_defaults() {
        let s: StrokeItem = serde_json::from_str("{}").unwrap();
        assert_eq!(s, StrokeItem::default());
        let f: FillItem = serde_json::from_value(serde_json::json!({"paint": Fill::Solid(crate::Color::BLACK)})).unwrap();
        assert!(f.visible);
        assert_eq!(f.opacity, 1.0);
        let a = Appearance { items: vec![AppearanceItem::Fill(f), AppearanceItem::Stroke(s)] };
        assert_eq!(a, serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap());
    }
}
