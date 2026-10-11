//! Reusable graphic appearance and sampling masks. Data only; original Local Image code.
//! Behaviour follows Illustrator Graphic Styles and Affinity Style Picker. Stable IDs survive
//! rename; a link records the last applied attributes so local edits cannot be overwritten.
use super::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
use crate::{
    BlendIf, BlendMode, Color, Effect, Fill,
    text::{AntiAlias, CharStyle, Orientation, ParagraphStyle, TextShape, TextWarp},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StyleId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Attributes {
    pub fill: bool,
    pub stroke: bool,
    pub opacity: bool,
    pub effects: bool,
    pub character: bool,
    pub paragraph: bool,
    pub object: bool,
}
impl Attributes {
    pub const ALL: Self = Self { fill: true, stroke: true, opacity: true, effects: true, character: true, paragraph: true, object: true };
    pub const NONE: Self = Self { fill: false, stroke: false, opacity: false, effects: false, character: false, paragraph: false, object: false };
    pub fn intersection(self, other: Self) -> Self {
        Self {
            fill: self.fill && other.fill,
            stroke: self.stroke && other.stroke,
            opacity: self.opacity && other.opacity,
            effects: self.effects && other.effects,
            character: self.character && other.character,
            paragraph: self.paragraph && other.paragraph,
            object: self.object && other.object,
        }
    }
}
impl Default for Attributes {
    fn default() -> Self {
        Self::ALL
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerEffects {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Effect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<(f64, f64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub psd_raw: Option<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextObject {
    pub shape: TextShape,
    pub orientation: Orientation,
    pub antialias: AntiAlias,
    pub warp: Option<TextWarp>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectAttributes {
    pub fill_opacity: f32,
    pub clipped: bool,
    pub blend_if: BlendIf,
    pub excluded_channels: u32,
    pub text: Option<TextObject>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// None for an object without vector paint; Some(empty) intentionally removes paint.
    pub appearance: Option<Appearance>,
    pub effects: LayerEffects,
    pub opacity: f32,
    pub blend: BlendMode,
    pub character: Option<CharStyle>,
    pub paragraph: Option<ParagraphStyle>,
    pub object: ObjectAttributes,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphicStyle {
    pub id: StyleId,
    pub name: String,
    pub attributes: Sample,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StyleLink {
    pub style: StyleId,
    pub applied: Sample,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub character_runs: Vec<crate::text::TextRun>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paragraph_runs: Vec<crate::text::ParagraphRun>,
}

/// Original starter artwork: editable paint stacks, no imported brand assets.
pub fn starter_library() -> Vec<GraphicStyle> {
    [
        ("Ocean Ink", Color::rgb(0.05, 0.32, 0.48), Color::rgb(0.02, 0.12, 0.2)),
        ("Citrus Paper", Color::rgb(0.98, 0.76, 0.2), Color::rgb(0.35, 0.2, 0.06)),
        ("Plum Outline", Color::rgb(0.75, 0.58, 0.78), Color::rgb(0.3, 0.12, 0.35)),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (name, fill, stroke))| GraphicStyle {
        id: StyleId(i as u64 + 1),
        name: name.into(),
        attributes: Sample {
            appearance: Some(Appearance {
                items: vec![
                    AppearanceItem::Fill(FillItem::new(Fill::Solid(fill))),
                    AppearanceItem::Stroke(StrokeItem {
                        base: crate::ShapeStroke { width: 2., paint: Fill::Solid(stroke), ..Default::default() },
                        ..Default::default()
                    }),
                ],
            }),
            effects: LayerEffects::default(),
            opacity: 1.,
            blend: BlendMode::Normal,
            character: None,
            paragraph: None,
            object: ObjectAttributes { fill_opacity: 1., clipped: false, blend_if: BlendIf::default(), excluded_channels: 0, text: None },
        },
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn styles38_data_old_options_and_starter_roundtrip() {
        assert_eq!(serde_json::from_str::<Attributes>("{}").unwrap(), Attributes::ALL);
        let library = starter_library();
        assert_eq!(library.len(), 3);
        let decoded: Vec<GraphicStyle> = serde_json::from_slice(&serde_json::to_vec(&library).unwrap()).unwrap();
        assert_eq!(decoded, library);
        assert_eq!(Attributes::ALL.intersection(Attributes::NONE), Attributes::NONE);
    }
}
