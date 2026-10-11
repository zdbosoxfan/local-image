//! Vector layout/keymaps adapted from VectorCraft tools/catalog.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Local Image IDs, compact groups, persona isolation and availability filtering are local.
use crate::toolsets::{Persona, ToolSet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Preset {
    #[default]
    Illustrator,
    Affinity,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub preset: Preset,
}

/// Group indices are stable, independent of package registration order.
pub const GROUPS: &[&[&str]] = &[
    &["Move"],
    &["DirectSelection", "GroupSelection", "PointTransform", "PathSelection"],
    &["VectorMagicWand", "VectorLasso"],
    &["Artboard", "VectorCrop", "Slice", "SliceSelect", "Crop"],
    &["Pen", "Curvature", "AddAnchor", "DeleteAnchor", "AnchorPoint"],
    &["VectorPencil", "Smooth", "PathEraser", "Join"],
    &["VectorBrush", "BlobBrush"],
    &["Corner", "Contour"],
    &[
        "Rectangle",
        "RoundedRectangle",
        "EllipseShape",
        "Polygon",
        "Star",
        "Triangle",
        "Diamond",
        "Trapezoid",
        "DoubleStar",
        "SquareStar",
        "Arrow",
        "Donut",
        "Pie",
        "Segment",
        "Crescent",
        "Cog",
        "Cloud",
        "Callout",
        "CalloutEllipse",
        "Tear",
        "Heart",
        "CustomShape",
    ],
    &["Line", "Arc", "Spiral", "RectGrid", "PolarGrid"],
    &["ArtText", "FrameText", "TextOnPath", "VerticalType", "Type"],
    &["Rotate", "Reflect"],
    &["Scale", "Shear", "Reshape"],
    &["Width", "VectorWarp", "VectorTwirl", "VectorPucker", "VectorBloat", "Scallop", "Crystallize", "Wrinkle"],
    &["FreeTransform", "VectorPuppetWarp"],
    &["ShapeBuilder", "LivePaintBucket", "VectorFloodFill", "LivePaintSelection"],
    &["VectorEraser", "Scissors", "Knife", "LineCut"],
    &["VectorFill", "Mesh", "Transparency"],
    &["Eyedropper", "StylePicker", "Measure", "Area"],
    &["Blend", "Repeat", "Envelope"],
    &["Hand"],
    &["Zoom"],
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group<'a> {
    pub slot: usize,
    pub tools: Vec<&'a str>,
}
/// Only executable registrations supplied by 01 are eligible. Unknown new IDs get their own
/// tail slot so a future package cannot disappear from All Tools. Duplicates are suppressed.
pub fn groups<'a>(registered: &[&'a str], selected: Option<&ToolSet>) -> Vec<Group<'a>> {
    let mut seen = std::collections::BTreeSet::new();
    let eligible = |id: &&str| selected.is_none_or(|s| s.tools.iter().any(|t| t == *id));
    let mut out = Vec::new();
    for (slot, group) in GROUPS.iter().enumerate() {
        let tools: Vec<_> = group.iter().filter_map(|id| registered.iter().copied().find(|r| r == id)).filter(eligible).filter(|id| seen.insert(*id)).collect();
        if !tools.is_empty() {
            out.push(Group { slot: 1000 + slot, tools });
        }
    }
    for id in registered.iter().copied().filter(eligible) {
        if seen.insert(id) {
            out.push(Group { slot: 1000 + out.len() + GROUPS.len(), tools: vec![id] });
        }
    }
    out
}
const ESSENTIALS: &[&str] = &[
    "Move",
    "DirectSelection",
    "Pen",
    "Curvature",
    "VectorPencil",
    "Corner",
    "ArtText",
    "FrameText",
    "ShapeBuilder",
    "VectorFill",
    "Transparency",
    "Eyedropper",
    "Hand",
    "Zoom",
];
const ILLUSTRATION: &[&str] = &[
    "VectorBrush",
    "BlobBrush",
    "Width",
    "VectorWarp",
    "VectorTwirl",
    "VectorPucker",
    "VectorBloat",
    "Scallop",
    "Crystallize",
    "Wrinkle",
    "Knife",
    "VectorEraser",
    "Blend",
    "Contour",
    "Mesh",
];
const LAYOUT: &[&str] = &["Move", "DirectSelection", "Artboard", "ArtText", "FrameText", "TextOnPath", "VerticalType", "Type", "Measure", "Hand", "Zoom"];
const PHOTOGRAPHER: &[&str] = &[
    "Move",
    "DirectSelection",
    "Pen",
    "Rectangle",
    "EllipseShape",
    "RoundedRectangle",
    "Star",
    "Arrow",
    "Callout",
    "ArtText",
    "TextOnPath",
    "Corner",
    "Eyedropper",
    "Hand",
    "Zoom",
];
/// Paired Photographer retains its established order; others follow the compact toolbar.
pub fn builtins(registered: &[&str]) -> Vec<ToolSet> {
    let all: Vec<_> = groups(registered, None).into_iter().flat_map(|g| g.tools).collect();
    [
        ("vector.allTools", "All Tools"),
        ("vector.photographer", "Photographer"),
        ("vector.essentials", "Essentials"),
        ("vector.illustration", "Illustration"),
        ("vector.layout", "Layout & Type"),
    ]
    .into_iter()
    .map(|(id, name)| {
        let tools = if id == "vector.photographer" {
            PHOTOGRAPHER.iter().copied().filter(|t| registered.contains(t)).collect()
        } else {
            all.iter()
                .copied()
                .filter(|t| match id {
                    "vector.allTools" => true,
                    "vector.essentials" => ESSENTIALS.contains(t) || GROUPS[8].contains(t),
                    "vector.illustration" => ESSENTIALS.contains(t) || GROUPS[8].contains(t) || ILLUSTRATION.contains(t),
                    _ => LAYOUT.contains(t) || GROUPS[8].contains(t),
                })
                .collect::<Vec<_>>()
        };
        ToolSet { id: id.into(), name: name.into(), persona: Persona::Vector, tools: tools.into_iter().map(str::to_string).collect() }
    })
    .collect()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: &'static str,
    pub target: &'static str,
}
const fn b(key: &'static str, target: &'static str) -> Binding {
    Binding { key, target }
}
pub const ILLUSTRATOR: &[Binding] = &[
    b("V", "Move"),
    b("A", "DirectSelection"),
    b("Y", "VectorMagicWand"),
    b("Q", "VectorLasso"),
    b("Shift+O", "Artboard"),
    b("Shift+K", "Slice"),
    b("P", "Pen"),
    b("Shift+~", "Curvature"),
    b("+", "AddAnchor"),
    b("-", "DeleteAnchor"),
    b("Shift+C", "AnchorPoint"),
    b("N", "VectorPencil"),
    b("B", "VectorBrush"),
    b("Shift+B", "BlobBrush"),
    b("M", "Rectangle"),
    b("L", "EllipseShape"),
    b("\\", "Line"),
    b("T", "ArtText"),
    b("R", "Rotate"),
    b("O", "Reflect"),
    b("S", "Scale"),
    b("Shift+W", "Width"),
    b("Shift+R", "VectorWarp"),
    b("E", "FreeTransform"),
    b("Shift+M", "ShapeBuilder"),
    b("K", "LivePaintBucket"),
    b("Shift+L", "LivePaintSelection"),
    b("Shift+E", "VectorEraser"),
    b("C", "Scissors"),
    b("G", "VectorFill"),
    b("U", "Mesh"),
    b("I", "Eyedropper"),
    b("W", "Blend"),
    b("H", "Hand"),
    b("Z", "Zoom"),
];
pub const AFFINITY: &[Binding] = &[
    b("V", "Move"),
    b("A", "DirectSelection"),
    b("F", "PointTransform"),
    b("P", "Pen"),
    b("N", "VectorPencil"),
    b("B", "VectorBrush"),
    b("C", "Corner"),
    b("O", "Contour"),
    b("M", "Rectangle"),
    b("T", "ArtText"),
    b("W", "Width"),
    b("S", "ShapeBuilder"),
    b("R", "VectorFloodFill"),
    b("K", "Knife"),
    b("G", "VectorFill"),
    b("Y", "Transparency"),
    b("I", "Eyedropper"),
    b("H", "Hand"),
    b("Z", "Zoom"),
];
pub const PROXY_KEYS: &[Binding] = &[
    b("X", "vector.paint.active"),
    b("Shift+X", "vector.paint.swap"),
    b("D", "vector.paint.default"),
    b("/", "vector.paint.none"),
    b("Shift+D", "vector.drawMode.cycle"),
];
/// These override *all* owners of their chord, including channel targeting outside the menu.
pub const ACCELERATORS: &[Binding] = &[
    b("Cmd+D", "vector.transform.again"),
    b("Cmd+J", "object.path.join"),
    b("Cmd+Y", "vector.view.outline"),
    b("Cmd+U", "vector.snap.smartGuides"),
    b("Cmd+B", "object.pasteBack"),
    b("Cmd+T", "window.panel.character"),
    b("Cmd+Shift+O", "type.convertToShape"),
    b("Cmd+F", "object.pasteFront"),
    b("Cmd+2", "vector.object.lock"),
    b("Cmd+3", "vector.object.hide"),
    b("Cmd+7", "object.clippingMask.make"),
    b("Cmd+8", "object.compoundPath.make"),
];
pub fn tool_bindings(preset: Preset) -> &'static [Binding] {
    match preset {
        Preset::Illustrator => ILLUSTRATOR,
        Preset::Affinity => AFFINITY,
    }
}
pub fn tool_for_key<'a>(persona: Persona, preset: Preset, key: &str, registered: &[&'a str]) -> Option<&'a str> {
    if persona != Persona::Vector {
        return None;
    }
    let target = tool_bindings(preset).iter().find(|b| b.key == key)?.target;
    registered.iter().copied().find(|id| *id == target)
}
/// Affinity M cycles the registered rectangle/rounded rectangle/ellipse family.
/// One chord remains owned by one binding; unavailable family members are skipped.
pub fn tool_for_key_after<'a>(persona: Persona, preset: Preset, key: &str, registered: &[&'a str], current: &str) -> Option<&'a str> {
    if persona == Persona::Vector && preset == Preset::Affinity && key == "M" {
        let family: Vec<_> =
            ["Rectangle", "RoundedRectangle", "EllipseShape"].into_iter().filter_map(|id| registered.iter().copied().find(|r| *r == id)).collect();
        if family.is_empty() {
            return None;
        }
        let next = family.iter().position(|id| *id == current).map_or(0, |i| (i + 1) % family.len());
        return family.get(next).copied();
    }
    tool_for_key(persona, preset, key, registered)
}
/// Apply after constructing the existing Pixel table and before sorting/dispatch. Pixel returns
/// byte-identical input, including user overrides. The shell uses the same table for labels.
pub fn command_bindings(persona: Persona, pixel: &[(String, String)]) -> Vec<(String, String)> {
    if persona == Persona::Pixel {
        return pixel.to_vec();
    }
    let overrides: Vec<_> = ACCELERATORS.iter().chain(PROXY_KEYS).collect();
    let mut out: Vec<_> = pixel.iter().filter(|(id, key)| !overrides.iter().any(|b| b.key == key || b.target == id)).cloned().collect();
    out.extend(overrides.into_iter().map(|b| (b.target.into(), b.key.into())));
    out
}
#[cfg(test)]
#[path = "vector_toolbar_layout/tests.rs"]
mod tests;

/// Compiled 46 toolbar slot. Paint edits reuse 08's journaled shape.appearance.set; this
/// read-only command exposes keymap data to the shell/automation without another prefs store.
pub fn specs() -> Vec<crate::commands::CommandSpec> {
    vec![crate::commands::CommandSpec {
        id: "vector.toolbar.keymap",
        label: "Vector Keyboard Preset",
        menu: &[],
        shortcut: None,
        params: "{preset?: illustrator|affinity}",
        enabled: crate::commands::always,
        run: |_, v| {
            let settings: Settings = serde_json::from_value(v.clone()).map_err(|e| crate::EngineError::Other(e.to_string()))?;
            Ok(
                serde_json::json!({"preset":settings.preset,"tools":tool_bindings(settings.preset).iter().map(|b|serde_json::json!({"key":b.key,"tool":b.target})).collect::<Vec<_>>() }),
            )
        },
        journal: false,
    }]
}
