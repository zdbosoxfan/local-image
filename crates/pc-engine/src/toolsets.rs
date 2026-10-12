//! Tool sets: named layouts of the Compositing toolbar (which tools show). Five read-only
//! templates belong to each persona; the user's own live in
//! `toolbar.sets` of the preferences. Commands `toolset.list / select / save / rename / delete /
//! duplicate / reset` drive them, so the toolbar, the menus, the dialog and automation agree.
//!
//! Tools are named as in `ui.set {tool}` (`Move`, `RectMarquee`, …). A set belongs to a
//! [`Persona`]. Pixel ids and tool order remain compatible with old settings.
//! The toolbar keeps the shell's group structure (the flyouts) and shows, per group, the tools
//! the set lists; a tool a set leaves out is still reachable by its shortcut (the toolbar then
//! shows the active tool in its place while it is active).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::prefs::ToolbarCustomization;
use crate::{EngineError, Result, Session};

/// Command bodies report plain messages; [`wrap`] turns them into bad-parameter errors.
type R<T> = std::result::Result<T, String>;

/// Which editing persona a set belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Persona {
    /// The raster editor.
    #[default]
    Pixel,
    Vector,
}

impl Persona {
    pub fn is_pixel(&self) -> bool {
        *self == Self::Pixel
    }

    pub fn default_set(self) -> &'static str {
        match self {
            Self::Pixel => DEFAULT_ID,
            Self::Vector => "vector.allTools",
        }
    }
}

/// A named list of tools.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolSet {
    pub id: String,
    pub name: String,
    pub persona: Persona,
    /// Tool names in display order; flyout tools keep the toolbar’s group structure.
    pub tools: Vec<String>,
}

/// The id of All Tools, the default set.
pub const DEFAULT_ID: &str = "allTools";

/// Frozen Pixel tools, in the toolbar's canonical order (legacy API/migration oracle).
pub const ALL_TOOLS: &[&str] = &[
    "Move",
    "RectMarquee",
    "EllipseMarquee",
    "Lasso",
    "PolygonLasso",
    "MagneticLasso",
    "ObjectSelection",
    "QuickSelection",
    "MagicWand",
    "Crop",
    "PerspectiveCrop",
    "Slice",
    "SliceSelect",
    "Eyedropper",
    "Ruler",
    "Note",
    "Count",
    "SpotHealing",
    "Healing",
    "Patch",
    "ContentAwareMove",
    "RedEye",
    "Brush",
    "Pencil",
    "MixerBrush",
    "CloneStamp",
    "HistoryBrush",
    "Eraser",
    "BackgroundEraser",
    "MagicEraser",
    "Gradient",
    "PaintBucket",
    "Blur",
    "Sharpen",
    "Smudge",
    "Dodge",
    "Burn",
    "Sponge",
    "Pen",
    "Type",
    "VerticalType",
    "PathSelection",
    "DirectSelection",
    "Rectangle",
    "EllipseShape",
    "Triangle",
    "Polygon",
    "Line",
    "CustomShape",
    "AiRemove",
    "AiCutout",
    "Hand",
    "RotateView",
    "Zoom",
];

const PHOTOGRAPHER: &[&str] = &[
    "Move",
    "RectMarquee",
    "EllipseMarquee",
    "Lasso",
    "ObjectSelection",
    "QuickSelection",
    "Crop",
    "PerspectiveCrop",
    "Eyedropper",
    "SpotHealing",
    "Healing",
    "Patch",
    "ContentAwareMove",
    "RedEye",
    "CloneStamp",
    "MagicEraser",
    "AiRemove",
    "Brush",
    "Eraser",
    "Gradient",
    "Dodge",
    "Burn",
    "Sponge",
    "Blur",
    "Sharpen",
    "Type",
    "Hand",
    "RotateView",
    "Zoom",
];

const ESSENTIALS: &[&str] = &[
    "Move",
    "RectMarquee",
    "Lasso",
    "ObjectSelection",
    "QuickSelection",
    "Crop",
    "Eyedropper",
    "SpotHealing",
    "Brush",
    "CloneStamp",
    "Eraser",
    "Gradient",
    "Pen",
    "Type",
    "Rectangle",
    "Hand",
    "RotateView",
    "Zoom",
];

const RETOUCHING: &[&str] = &[
    "Move",
    "Lasso",
    "QuickSelection",
    "Eyedropper",
    "SpotHealing",
    "Healing",
    "Patch",
    "ContentAwareMove",
    "RedEye",
    "Brush",
    "CloneStamp",
    "HistoryBrush",
    "Eraser",
    "Blur",
    "Sharpen",
    "Smudge",
    "Dodge",
    "Burn",
    "Sponge",
    "Hand",
    "RotateView",
    "Zoom",
];

// Quick Selection can use Subject Assist; the other retouch tools use pixel algorithms.
const AI: &[&str] = &["Move", "ObjectSelection", "QuickSelection", "Crop", "AiRemove", "AiCutout", "Hand", "RotateView", "Zoom"];

/// Registration supplied to the shell and feature packages. `package` owns the feature hook,
/// not the tool id. Missing hooks must stay disabled rather than route to a raster namesake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VectorTool {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub package: Option<u8>,
    /// Stable flyout family; adding tools never changes Pixel's slot identities.
    pub slot: u8,
}

macro_rules! vector_tools {
    ($(($id:literal, $label:literal, $icon:literal, $package:expr, $slot:literal)),* $(,)?) => {
        pub const VECTOR_TOOLS: &[VectorTool] = &[$(VectorTool { id: $id, label: $label, icon: $icon, package: $package, slot: $slot }),*];
    };
}

// Original Local Image registry, following vector-persona-spec §7. Icons alias the existing
// colour/mono icon system; no upstream brand assets or tool implementation is copied.
vector_tools![
    ("Move", "Move Tool", "move", None, 0),
    ("DirectSelection", "Direct Selection Tool", "direct-select", Some(3), 1),
    ("PathSelection", "Path Selection Tool", "mouse-pointer-2", None, 1),
    ("PointTransform", "Point Transform Tool", "vector-point-transform", Some(16), 2),
    ("Pen", "Pen Tool", "pen-tool", Some(4), 3),
    ("VectorPencil", "Vector Pencil Tool", "vector-pencil", Some(5), 4),
    ("VectorBrush", "Vector Brush Tool", "vector-brush", Some(17), 4),
    ("Smooth", "Smooth Tool", "vector-smooth", Some(10), 5),
    ("PathEraser", "Path Eraser Tool", "vector-path-eraser", Some(10), 5),
    ("Join", "Join Tool", "vector-join", Some(10), 5),
    ("Corner", "Corner Tool", "vector-corner", Some(6), 6),
    ("Rectangle", "Rectangle Tool", "rectangle-horizontal", Some(2), 7),
    ("RoundedRectangle", "Rounded Rectangle Tool", "vector-rounded-rectangle", Some(2), 7),
    ("EllipseShape", "Ellipse Tool", "circle", Some(2), 7),
    ("Triangle", "Triangle Tool", "shape-triangle", Some(2), 7),
    ("Polygon", "Polygon Tool", "shape-polygon", Some(2), 7),
    ("Star", "Star Tool", "vector-star", Some(2), 7),
    ("DoubleStar", "Double Star Tool", "vector-double-star", Some(2), 7),
    ("SquareStar", "Square Star Tool", "vector-square-star", Some(2), 7),
    ("Trapezoid", "Trapezoid Tool", "vector-trapezoid", Some(2), 7),
    ("Diamond", "Diamond Tool", "vector-diamond", Some(2), 7),
    ("Donut", "Donut Tool", "vector-donut", Some(2), 7),
    ("Pie", "Pie Tool", "vector-pie", Some(2), 7),
    ("Segment", "Segment Tool", "vector-segment", Some(2), 7),
    ("Arrow", "Arrow Tool", "vector-arrow", Some(2), 7),
    ("Callout", "Callout Tool", "vector-callout", Some(2), 7),
    ("Cog", "Cog Tool", "vector-cog", Some(2), 7),
    ("Heart", "Heart Tool", "vector-heart", Some(2), 7),
    ("Tear", "Tear Tool", "vector-tear", Some(2), 7),
    ("Crescent", "Crescent Tool", "vector-crescent", Some(2), 7),
    ("Cloud", "Cloud Tool", "vector-cloud", Some(2), 7),
    ("Spiral", "Spiral Tool", "vector-spiral", Some(2), 7),
    ("Arc", "Arc Tool", "vector-arc", Some(2), 7),
    ("Line", "Line Tool", "slash", Some(2), 7),
    ("RectGrid", "Rectangular Grid Tool", "vector-rect-grid", Some(2), 7),
    ("PolarGrid", "Polar Grid Tool", "vector-polar-grid", Some(2), 7),
    ("CustomShape", "Custom Shape Tool", "star", None, 7),
    ("VectorFill", "Vector Fill Tool", "blend", Some(8), 8),
    ("Transparency", "Transparency Tool", "vector-transparency", Some(8), 8),
    ("ShapeBuilder", "Shape Builder Tool", "vector-shape-builder", Some(10), 9),
    ("Contour", "Contour Tool", "vector-contour", Some(10), 10),
    ("Knife", "Knife Tool", "vector-knife", Some(10), 11),
    ("Scissors", "Scissors Tool", "vector-scissors", Some(10), 11),
    ("LineCut", "Line Cut Tool", "vector-line-cut", Some(10), 11),
    ("Width", "Width Tool", "vector-width", Some(11), 12),
    ("ArtText", "Artistic Text Tool", "type", Some(12), 13),
    ("FrameText", "Frame Text Tool", "vector-frame-text", Some(12), 13),
    ("TextOnPath", "Text on Path Tool", "vector-path-text", Some(12), 13),
    ("Type", "Horizontal Type Tool", "type", None, 13),
    ("VerticalType", "Vertical Type Tool", "type", None, 13),
    ("Blend", "Blend Tool", "vector-blend", Some(18), 14),
    ("Repeat", "Repeat Tool", "vector-repeat", Some(19), 14),
    ("Envelope", "Envelope Tool", "vector-envelope", Some(20), 14),
    ("VectorCrop", "Vector Crop Tool", "vector-crop", Some(16), 15),
    ("Artboard", "Artboard Tool", "vector-artboard", Some(16), 16),
    ("Crop", "Crop Tool", "crop", None, 17),
    ("Eyedropper", "Eyedropper Tool", "pipette", None, 18),
    ("Hand", "Hand Tool", "hand", None, 19),
    ("RotateView", "Rotate View Tool", "vector-rotate", None, 19),
    ("Zoom", "Zoom Tool", "zoom-in", None, 20),
    ("GroupSelection", "Group Selection Tool", "vector-group-select", Some(32), 1),
    ("VectorMagicWand", "Vector Magic Wand Tool", "wand-sparkles", Some(32), 2),
    ("VectorLasso", "Vector Lasso Tool", "vector-lasso", Some(32), 2),
    ("Curvature", "Curvature Tool", "vector-curvature", Some(33), 3),
    ("AddAnchor", "Add Anchor Tool", "vector-anchor-add", Some(33), 3),
    ("DeleteAnchor", "Delete Anchor Tool", "vector-anchor-delete", Some(33), 3),
    ("AnchorPoint", "Anchor Point Tool", "vector-anchor-point", Some(33), 3),
    ("Rotate", "Rotate Tool", "vector-rotate", Some(34), 2),
    ("Reflect", "Reflect Tool", "vector-reflect", Some(34), 2),
    ("Scale", "Scale Tool", "vector-scale", Some(34), 2),
    ("Shear", "Shear Tool", "vector-shear", Some(34), 2),
    ("Reshape", "Reshape Tool", "vector-reshape", Some(34), 2),
    ("FreeTransform", "Free Transform Tool", "vector-free-transform", Some(34), 2),
    ("StylePicker", "Style Picker Tool", "vector-style-picker", Some(38), 18),
    ("BlobBrush", "Blob Brush Tool", "vector-blob-brush", Some(40), 4),
    ("VectorEraser", "Vector Eraser Tool", "vector-eraser", Some(40), 11),
    ("VectorWarp", "Warp Tool", "forward-warp", Some(41), 12),
    ("VectorTwirl", "Twirl Tool", "twirl-cw", Some(41), 12),
    ("VectorPucker", "Pucker Tool", "pucker", Some(41), 12),
    ("VectorBloat", "Bloat Tool", "bloat", Some(41), 12),
    ("Scallop", "Scallop Tool", "vector-scallop", Some(41), 12),
    ("Crystallize", "Crystallize Tool", "vector-crystallize", Some(41), 12),
    ("Wrinkle", "Wrinkle Tool", "vector-wrinkle", Some(41), 12),
    ("VectorPuppetWarp", "Puppet Warp Tool", "vector-puppet", Some(41), 12),
    ("LivePaintBucket", "Live Paint Bucket Tool", "vector-live-paint", Some(42), 9),
    ("VectorFloodFill", "Vector Flood Fill Tool", "vector-flood-fill", Some(42), 9),
    ("LivePaintSelection", "Live Paint Selection Tool", "vector-live-select", Some(42), 9),
    ("Mesh", "Mesh Tool", "vector-mesh", Some(43), 8),
    ("Measure", "Measure Tool", "ruler", Some(44), 18),
    ("Area", "Area Tool", "vector-area", Some(44), 18),
];

/// The legacy Pixel snapshot remains ALL_TOOLS. Vector All Tools is generated from its registry.
pub fn tools_for(persona: Persona) -> Vec<&'static str> {
    match persona {
        Persona::Pixel => ALL_TOOLS.iter().chain(crate::shared_tools::SHARED_ADDITIONS).copied().collect(),
        Persona::Vector => VECTOR_TOOLS.iter().map(|t| t.id).collect(),
    }
}

pub fn supports(tool: &str, persona: Persona) -> bool {
    match persona {
        Persona::Pixel => ALL_TOOLS.iter().any(|id| id.eq_ignore_ascii_case(tool)) || crate::shared_tools::shared_in_pixel(tool),
        Persona::Vector => VECTOR_TOOLS.iter().any(|entry| entry.id.eq_ignore_ascii_case(tool)),
    }
}

pub fn canonical_tool(tool: &str) -> Option<&'static str> {
    ALL_TOOLS.iter().copied().chain(VECTOR_TOOLS.iter().map(|t| t.id)).find(|id| id.eq_ignore_ascii_case(tool))
}

fn built(id: &str, name: &str, tools: &[&str]) -> ToolSet {
    ToolSet { id: id.into(), name: name.into(), persona: Persona::Pixel, tools: tools.iter().map(|t| t.to_string()).collect() }
}

/// The built-in sets, All Tools first.
pub fn builtins() -> Vec<ToolSet> {
    vec![
        built(DEFAULT_ID, "All Tools", ALL_TOOLS),
        built("photographer", "Photographer", PHOTOGRAPHER),
        built("essentials", "Essentials", ESSENTIALS),
        built("retouching", "Retouching", RETOUCHING),
        built("ai", "AI", AI),
    ]
}

pub fn builtins_for(persona: Persona) -> Vec<ToolSet> {
    if persona == Persona::Pixel {
        return builtins();
    }
    crate::vector_toolbar_layout::builtins(&tools_for(persona))
}

/// Longest set name.
pub const MAX_NAME: usize = 40;

impl ToolbarCustomization {
    pub fn is_builtin(id: &str) -> bool {
        matches!(
            id,
            "allTools"
                | "photographer"
                | "essentials"
                | "retouching"
                | "ai"
                | "vector.allTools"
                | "vector.photographer"
                | "vector.essentials"
                | "vector.illustration"
                | "vector.layout"
        )
    }

    /// Every set in the active persona (the original Pixel API keeps its five templates).
    pub fn all_sets(&self) -> Vec<ToolSet> {
        self.sets_for(self.persona)
    }

    /// Complete registry, for explicit ids and cross-persona copies.
    pub fn catalog_sets(&self) -> Vec<ToolSet> {
        let mut v = builtins();
        v.extend(builtins_for(Persona::Vector));
        v.extend(self.sets.iter().cloned());
        v
    }

    /// The set in use (All Tools when the saved id no longer exists).
    pub fn active(&self) -> ToolSet {
        let sets = self.sets_for(self.persona);
        let saved = match self.persona {
            Persona::Pixel => &self.active_set,
            Persona::Vector => &self.vector_set,
        };
        let want = if saved.is_empty() { self.persona.default_set() } else { saved.as_str() };
        sets.iter().find(|s| s.id == want).or_else(|| sets.first()).cloned().unwrap_or_default()
    }

    pub fn sets_for(&self, persona: Persona) -> Vec<ToolSet> {
        let mut sets = builtins_for(persona);
        sets.extend(self.sets.iter().filter(|s| s.persona == persona).cloned());
        sets
    }

    pub fn select_set(&mut self, set: &ToolSet) {
        self.persona = set.persona;
        match set.persona {
            Persona::Pixel => self.active_set = set.id.clone(),
            Persona::Vector => self.vector_set = set.id.clone(),
        }
    }

    pub fn last_tool(&self, persona: Persona) -> &str {
        match persona {
            Persona::Pixel => &self.pixel_tool,
            Persona::Vector => &self.vector_tool,
        }
    }

    pub fn remember_tool(&mut self, persona: Persona, tool: &str) -> bool {
        let Some(tool) = canonical_tool(tool).filter(|t| supports(t, persona)) else { return false };
        let saved = match persona {
            Persona::Pixel => &mut self.pixel_tool,
            Persona::Vector => &mut self.vector_tool,
        };
        if saved == tool {
            return false;
        }
        *saved = tool.into();
        true
    }

    /// Preferences saved before tool sets had a hidden list and an order: they become the custom
    /// set "My Tools" (made active), and the old fields are cleared. Nothing changes for a
    /// default toolbar.
    pub fn migrate(&mut self) {
        if self.hidden.is_empty() && self.order.is_empty() {
            return;
        }
        {
            let active = self.active();
            let known = |t: &str| ALL_TOOLS.iter().any(|a| a.eq_ignore_ascii_case(t));
            let mut tools: Vec<String> = Vec::new();
            for t in self.order.iter().filter(|t| known(t)) {
                let canon = ALL_TOOLS.iter().find(|a| a.eq_ignore_ascii_case(t)).map(|a| a.to_string()).unwrap_or_default();
                if !tools.contains(&canon) {
                    tools.push(canon);
                }
            }
            for t in &active.tools {
                if !tools.contains(t) {
                    tools.push(t.clone());
                }
            }
            let hidden: Vec<String> = self.hidden.iter().map(|h| h.to_ascii_lowercase()).collect();
            tools.retain(|t| !hidden.contains(&t.to_ascii_lowercase()));
            if tools != active.tools {
                if let Some(set) = self.sets.iter_mut().find(|s| s.id == active.id) {
                    set.tools = tools;
                } else {
                    let id = self.next_id();
                    self.sets.push(ToolSet { id: id.clone(), name: "My Tools".into(), persona: active.persona, tools });
                    self.active_set = id;
                }
            }
        }
        self.hidden.clear();
        self.order.clear();
    }

    fn next_id(&self) -> String {
        (1u32..).map(|n| format!("custom-{n}")).find(|id| !self.sets.iter().any(|s| &s.id == id)).unwrap_or_default()
    }

    /// A free name: `base`, else `base 2`, `base 3` …
    pub fn unique_name(&self, base: &str) -> String {
        let taken = |n: &str| self.sets_for(self.persona).iter().any(|s| s.name.eq_ignore_ascii_case(n));
        let base: String = base.trim().chars().take(MAX_NAME).collect();
        let base = if base.is_empty() { "My Tools".to_string() } else { base };
        if !taken(&base) {
            return base;
        }
        (2u32..)
            .map(|n| {
                let suffix = format!(" {n}");
                let stem: String = base.chars().take(MAX_NAME - suffix.len()).collect();
                format!("{stem}{suffix}")
            })
            .find(|n| !taken(n))
            .unwrap_or(base)
    }
}

fn str_param<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k).and_then(Value::as_str)
}

fn describe(c: &ToolbarCustomization) -> Value {
    let sets: Vec<Value> = c
        .all_sets()
        .iter()
        .map(|s| json!({"id": s.id, "name": s.name, "persona": s.persona, "builtin": ToolbarCustomization::is_builtin(&s.id), "tools": s.tools}))
        .collect();
    json!({"active": c.active().id, "persona": c.persona, "sets": sets})
}

/// A set by id, or by name (case-insensitive).
fn find(c: &ToolbarCustomization, p: &Value) -> R<ToolSet> {
    let sets = c.catalog_sets();
    let hit = match (str_param(p, "id"), str_param(p, "name")) {
        (Some(id), _) => sets.iter().find(|s| s.id == id).or_else(|| sets.iter().find(|s| s.persona == c.persona && s.name.eq_ignore_ascii_case(id))),
        (None, Some(n)) => sets.iter().find(|s| s.persona == c.persona && s.name.eq_ignore_ascii_case(n)),
        _ => return Err("toolset needs an `id` (or a `name`)".into()),
    };
    hit.cloned().ok_or_else(|| "no such tool set".into())
}

fn clean_tools(p: &Value, persona: Persona) -> R<Option<Vec<String>>> {
    let Some(value) = p.get("tools") else { return Ok(None) };
    let arr = value.as_array().ok_or("`tools` must be an array of tool names")?;
    let mut out: Vec<String> = Vec::new();
    for value in arr {
        let t = value.as_str().ok_or("every tool must be a string")?;
        let canon = canonical_tool(t).ok_or_else(|| format!("unknown tool `{t}`"))?;
        if !supports(canon, persona) {
            return Err(format!("tool `{t}` does not belong to {persona:?}"));
        }
        if !out.iter().any(|o| o == canon) {
            out.push(canon.to_string());
        }
    }
    Ok(Some(out))
}

fn check_name(c: &ToolbarCustomization, persona: Persona, name: &str, except: Option<&str>) -> R<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("a tool set needs a name".into());
    }
    if n.chars().count() > MAX_NAME {
        return Err(format!("a tool set name is at most {MAX_NAME} characters"));
    }
    if c.sets_for(persona).iter().any(|s| s.name.eq_ignore_ascii_case(n) && Some(s.id.as_str()) != except) {
        return Err(format!("a tool set named \"{n}\" already exists"));
    }
    Ok(n.to_string())
}

fn list_impl(s: &mut Session, _: &Value) -> R<Value> {
    Ok(describe(&s.prefs().toolbar))
}

fn select_impl(s: &mut Session, p: &Value) -> R<Value> {
    let set = find(&s.prefs().toolbar, p)?;
    s.edit_prefs(|pr| pr.toolbar.select_set(&set));
    Ok(describe(&s.prefs().toolbar))
}

/// `toolset.save`: create a set (`name`, `tools`, or `from` an existing set), or update a custom
/// one (`id`, with `tools` and/or `name`). Built-ins are read-only: saving changes to one is a
/// new set (omit `id`, give `from`).
fn save_impl(s: &mut Session, p: &Value) -> R<Value> {
    let c = s.prefs().toolbar.clone();
    if let Some(id) = str_param(p, "id") {
        if ToolbarCustomization::is_builtin(id) {
            return Err("built-in tool sets are read-only: save a copy (omit `id`, give `from`)".into());
        }
        let set = c.sets.iter().find(|x| x.id == id).ok_or("no such tool set")?;
        let tools = clean_tools(p, set.persona)?;
        let name = match str_param(p, "name") {
            Some(n) => Some(check_name(&c, set.persona, n, Some(id))?),
            None => None,
        };
        if !c.sets.iter().any(|x| x.id == id) {
            return Err("no such tool set".into());
        }
        s.edit_prefs(|pr| {
            if let Some(set) = pr.toolbar.sets.iter_mut().find(|x| x.id == id) {
                if let Some(t) = tools {
                    set.tools = t;
                }
                if let Some(n) = name {
                    set.name = n;
                }
            }
        });
        return Ok(describe(&s.prefs().toolbar));
    }
    let from = match str_param(p, "from") {
        Some(f) => Some(find(&c, &json!({"id": f}))?),
        None => None,
    };
    let persona = match p.get("persona") {
        Some(value) => serde_json::from_value::<Persona>(value.clone()).map_err(|_| "persona must be pixel or vector")?,
        None => from.as_ref().map_or(c.persona, |f| f.persona),
    };
    if from.as_ref().is_some_and(|f| f.persona != persona) {
        return Err("a copy must keep its source persona".into());
    }
    let tools = clean_tools(p, persona)?;
    let tools = tools.or_else(|| from.as_ref().map(|f| f.tools.clone())).ok_or_else(|| "toolset.save needs `tools` or `from`".to_string())?;
    let name = match str_param(p, "name") {
        Some(n) => check_name(&c, persona, n, None)?,
        None => {
            let mut family = c.clone();
            family.persona = persona;
            family.unique_name(&from.as_ref().map_or("My Tools".to_string(), |f| format!("{} copy", f.name)))
        }
    };
    let id = c.next_id();
    s.edit_prefs(|pr| {
        let set = ToolSet { id: id.clone(), name, persona, tools };
        pr.toolbar.select_set(&set);
        pr.toolbar.sets.push(set);
    });
    let mut out = describe(&s.prefs().toolbar);
    out["created"] = json!(id);
    Ok(out)
}

fn duplicate_impl(s: &mut Session, p: &Value) -> R<Value> {
    let set = find(&s.prefs().toolbar, p)?;
    let mut q = json!({"from": set.id});
    if let Some(n) = str_param(p, "newName") {
        q["name"] = json!(n);
    }
    save_impl(s, &q)
}

fn rename_impl(s: &mut Session, p: &Value) -> R<Value> {
    let set = find(&s.prefs().toolbar, p)?;
    if ToolbarCustomization::is_builtin(&set.id) {
        return Err("built-in tool sets cannot be renamed".into());
    }
    let new = str_param(p, "newName").or_else(|| str_param(p, "to")).ok_or_else(|| "toolset.rename needs `newName`".to_string())?;
    save_impl(s, &json!({"id": set.id, "name": new}))
}

fn delete_impl(s: &mut Session, p: &Value) -> R<Value> {
    let set = find(&s.prefs().toolbar, p)?;
    if ToolbarCustomization::is_builtin(&set.id) {
        return Err("built-in tool sets cannot be deleted".into());
    }
    s.edit_prefs(|pr| {
        pr.toolbar.sets.retain(|x| x.id != set.id);
        if pr.toolbar.active_set == set.id {
            pr.toolbar.active_set = DEFAULT_ID.into();
        }
        if pr.toolbar.vector_set == set.id {
            pr.toolbar.vector_set = Persona::Vector.default_set().into();
        }
    });
    Ok(describe(&s.prefs().toolbar))
}

/// `toolset.reset`: select a built-in template (All Tools by default); custom sets stay.
fn reset_impl(s: &mut Session, p: &Value) -> R<Value> {
    let set = if p.get("id").is_some() || p.get("name").is_some() { find(&s.prefs().toolbar, p)? } else { builtins_for(s.prefs().toolbar.persona).remove(0) };
    if !ToolbarCustomization::is_builtin(&set.id) {
        return Err("reset needs a built-in tool set".into());
    }
    s.edit_prefs(|pr| pr.toolbar.select_set(&set));
    Ok(describe(&s.prefs().toolbar))
}

macro_rules! wrap {
    ($name:ident, $imp:ident, $id:literal) => {
        fn $name(s: &mut Session, p: &Value) -> Result<Value> {
            $imp(s, p).map_err(|msg| EngineError::BadParams { cmd: $id.into(), msg })
        }
    };
}
wrap!(list, list_impl, "toolset.list");
wrap!(select, select_impl, "toolset.select");
wrap!(save, save_impl, "toolset.save");
wrap!(duplicate, duplicate_impl, "toolset.duplicate");
wrap!(rename, rename_impl, "toolset.rename");
wrap!(delete, delete_impl, "toolset.delete");
wrap!(reset, reset_impl, "toolset.reset");

fn select_persona(s: &mut Session, p: &Value) -> Result<Value> {
    let persona: Persona = serde_json::from_value(p.get("persona").cloned().unwrap_or(Value::Null))
        .map_err(|_| EngineError::BadParams { cmd: "persona.select".into(), msg: "persona must be pixel or vector".into() })?;
    s.edit_prefs(|pr| pr.toolbar.persona = persona);
    Ok(describe(&s.prefs().toolbar))
}

fn select_pixel(s: &mut Session, _: &Value) -> Result<Value> {
    select_persona(s, &json!({"persona":"pixel"}))
}
fn select_vector(s: &mut Session, _: &Value) -> Result<Value> {
    select_persona(s, &json!({"persona":"vector"}))
}

macro_rules! spec {
    ($id:literal, $label:literal, $params:literal, $run:expr, $journal:expr) => {
        CommandSpec { id: $id, label: $label, menu: &[], shortcut: None, params: $params, enabled: always, run: $run, journal: $journal }
    };
}

fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

/// The `toolset.*` command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "view.persona.pixel",
            label: "Pixel persona",
            menu: &[],
            shortcut: Some("Cmd+Shift+1"),
            params: "{}",
            enabled: always,
            run: select_pixel,
            journal: false,
        },
        CommandSpec {
            id: "view.persona.vector",
            label: "Vector persona",
            menu: &[],
            shortcut: Some("Cmd+Shift+2"),
            params: "{}",
            enabled: always,
            run: select_vector,
            journal: false,
        },
        spec!("persona.select", "Select Persona", r##"{"persona":"pixel|vector"}"##, select_persona, false),
        spec!("toolset.list", "List Tool Sets", r##"{} → {"active":id,"sets":[{id,name,persona,builtin,tools}]}"##, list, false),
        spec!("toolset.select", "Select Tool Set", r##"{"id":"allTools|photographer|essentials|retouching|ai|custom-N"} (or "name")"##, select, true),
        spec!(
            "toolset.save",
            "Save Tool Set",
            r##"{"id":"custom-N"?,"name":str?,"tools":["<tool>",…]?,"from":"<set id>"?} creates a set (selected) when `id` is omitted; built-ins are read-only"##,
            save,
            true
        ),
        spec!("toolset.duplicate", "Duplicate Tool Set", r##"{"id":"<set>","newName":str?}"##, duplicate, true),
        spec!("toolset.rename", "Rename Tool Set", r##"{"id":"custom-N","newName":str}"##, rename, true),
        spec!("toolset.delete", "Delete Tool Set", r##"{"id":"custom-N"} (custom sets only)"##, delete, true),
        spec!("toolset.reset", "Reset Built-in Tool Set", r##"{"id":"<built-in>"?} (defaults to All Tools; custom sets stay)"##, reset, true),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session() -> Session {
        Session::new()
    }

    #[test]
    fn persona_pixel_snapshot_and_vector_photographer_contract() {
        let fixture: Value = serde_json::from_str(include_str!("../tests/fixtures/persona/pixel-toolbar.json")).unwrap();
        assert_eq!(json!(ALL_TOOLS), fixture["allTools"]);
        assert_eq!(json!(builtins()[1].tools), fixture["photographer"]);
        let vector = builtins_for(Persona::Vector);
        assert_eq!(
            vector[1].tools,
            [
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
                "Zoom"
            ]
        );
        let mut names = std::collections::BTreeSet::new();
        for tool in VECTOR_TOOLS {
            assert!(names.insert(tool.id));
            assert!(!tool.label.is_empty() && !tool.icon.is_empty());
        }
        for set in vector {
            assert!(set.tools.iter().all(|t| supports(t, Persona::Vector)), "{}", set.id);
        }
        assert!(supports("VectorPencil", Persona::Vector) && !supports("Pencil", Persona::Vector));
        assert!(supports("VectorFill", Persona::Vector) && !supports("Gradient", Persona::Vector));
        assert_ne!(canonical_tool("ArtText"), canonical_tool("FrameText"));
    }

    #[test]
    fn persona_settings_remember_each_family_and_custom_ids_across_restart() {
        let mut s = session();
        let pixel = s.execute("toolset.save", json!({"name":"Mine","from":"retouching"})).unwrap()["created"].clone();
        s.edit_prefs(|pr| {
            assert!(pr.toolbar.remember_tool(Persona::Pixel, "Brush"));
        });
        s.execute("persona.select", json!({"persona":"vector"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, "vector.allTools");
        let vector = s.execute("toolset.save", json!({"name":"Mine","from":"vector.photographer"})).unwrap()["created"].clone();
        assert_ne!(pixel, vector);
        s.edit_prefs(|pr| {
            assert!(pr.toolbar.remember_tool(Persona::Vector, "Pen"));
        });
        s.execute("toolset.rename", json!({"id":vector,"newName":"Logos"})).unwrap();
        let mut restored = session();
        restored.load_prefs_json(&s.prefs_to_json()).unwrap();
        assert_eq!(restored.prefs().toolbar, s.prefs().toolbar);
        restored.execute("persona.select", json!({"persona":"pixel"})).unwrap();
        assert_eq!(json!(restored.prefs().toolbar.active().id), pixel);
        assert_eq!(restored.prefs().toolbar.pixel_tool, "Brush");
        restored.execute("persona.select", json!({"persona":"vector"})).unwrap();
        assert_eq!(restored.prefs().toolbar.active().name, "Logos");
        assert_eq!(restored.prefs().toolbar.vector_tool, "Pen");
        restored.execute("toolset.delete", json!({"id":vector})).unwrap();
        assert_eq!(restored.prefs().toolbar.active().id, "vector.allTools");
        let mut final_session = session();
        final_session.load_prefs_json(&restored.prefs_to_json()).unwrap();
        assert_eq!(final_session.prefs().toolbar.sets.len(), 1);
        assert_eq!(json!(final_session.prefs().toolbar.sets[0].id), pixel);
    }

    #[test]
    fn persona_invalid_tools_and_cross_family_copy_are_atomic() {
        let mut s = session();
        s.execute("persona.select", json!({"persona":"vector"})).unwrap();
        let original = s.prefs().toolbar.clone();
        for p in [
            json!({"name":"Bad","tools":["Brush"]}),
            json!({"name":"Bad","from":"photographer","persona":"vector"}),
            json!({"name":"Bad","tools":["Pen"],"persona":"unknown"}),
        ] {
            assert!(s.execute("toolset.save", p).is_err());
            assert_eq!(s.prefs().toolbar, original);
        }
        assert!(s.execute("persona.select", json!({"persona":false})).is_err());
        assert_eq!(s.prefs().toolbar, original);
        let mut memory = original.clone();
        assert!(!memory.remember_tool(Persona::Vector, "Pencil"));
        assert!(!memory.remember_tool(Persona::Pixel, "VectorBrush"));
        assert_eq!(memory, original);
    }

    #[test]
    fn persona_old_prefs_load_pixel_and_family_mismatches_fall_back() {
        let mut s = session();
        s.load_prefs_json(r#"{"toolbar":{"activeSet":"photographer"}}"#).unwrap();
        assert_eq!(s.prefs().toolbar.persona, Persona::Pixel);
        assert_eq!(s.prefs().toolbar.active().id, "photographer");
        s.load_prefs_json(r#"{"toolbar":{"persona":"vector","vectorSet":"photographer","activeSet":"retouching"}}"#).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, "vector.allTools");
        s.execute("toolset.select", json!({"name":"Photographer"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, "vector.photographer");
        s.execute("persona.select", json!({"persona":"pixel"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, "retouching");
    }

    #[test]
    fn persona_vector_templates_are_readonly_and_reset_preserves_pixel_customization() {
        let mut s = session();
        let pixel = s.execute("toolset.save", json!({"name":"Pixels","tools":["Brush","Move"]})).unwrap()["created"].clone();
        for id in ["vector.allTools", "vector.photographer", "vector.essentials", "vector.illustration", "vector.layout"] {
            let before = s.prefs().toolbar.clone();
            assert!(s.execute("toolset.delete", json!({"id":id})).is_err());
            assert!(s.execute("toolset.rename", json!({"id":id,"newName":"Changed"})).is_err());
            assert!(s.execute("toolset.save", json!({"id":id,"tools":["Pen"]})).is_err());
            assert_eq!(s.prefs().toolbar, before);
        }
        s.execute("toolset.select", json!({"id":"vector.illustration"})).unwrap();
        s.execute("toolset.reset", json!({})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, "vector.allTools");
        s.execute("persona.select", json!({"persona":"pixel"})).unwrap();
        assert_eq!(json!(s.prefs().toolbar.active().id), pixel);
    }

    #[test]
    fn built_in_sets_use_known_tools_and_all_tools_is_the_default() {
        for b in builtins() {
            assert!(!b.tools.is_empty());
            for t in &b.tools {
                assert!(ALL_TOOLS.contains(&t.as_str()), "{} lists unknown {t}", b.id);
            }
        }
        let s = session();
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
        assert_eq!(builtins().iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["allTools", "photographer", "essentials", "retouching", "ai"]);
        assert_eq!(builtins()[0].tools.len(), ALL_TOOLS.len());
    }

    #[test]
    fn create_rename_duplicate_select_and_delete_custom_sets() {
        let mut s = session();
        let r = s.execute("toolset.save", json!({"name": "Mine", "tools": ["move", "Brush"]})).unwrap();
        let id = r["created"].as_str().unwrap().to_string();
        assert_eq!(r["active"], id);
        assert_eq!(s.prefs().toolbar.active().tools, ["Move", "Brush"]);
        assert!(s.execute("toolset.save", json!({"name": "mine", "tools": ["Move"]})).is_err(), "names are unique");
        assert!(s.execute("toolset.save", json!({"name": "X", "tools": ["Nope"]})).is_err());
        s.execute("toolset.rename", json!({"id": id, "newName": "Faces"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().name, "Faces");
        let d = s.execute("toolset.duplicate", json!({"id": "retouching"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().name, "Retouching copy");
        assert_eq!(s.prefs().toolbar.active().tools.len(), RETOUCHING.len());
        s.execute("toolset.select", json!({"id": "ai"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, "ai");
        let copy = d["created"].as_str().unwrap().to_string();
        s.execute("toolset.delete", json!({"id": copy})).unwrap();
        assert_eq!(s.prefs().toolbar.sets.len(), 1);
        s.execute("toolset.select", json!({"id": id})).unwrap();
        s.execute("toolset.delete", json!({"id": id})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID, "deleting the active set falls back to All Tools");
    }

    #[test]
    fn built_ins_are_read_only() {
        let mut s = session();
        for id in ["allTools", "photographer", "essentials", "retouching", "ai"] {
            assert!(s.execute("toolset.delete", json!({"id": id})).is_err(), "{id}");
            assert!(s.execute("toolset.rename", json!({"id": id, "newName": "Z"})).is_err(), "{id}");
            assert!(s.execute("toolset.save", json!({"id": id, "tools": ["Move"]})).is_err(), "{id}");
        }
        assert_eq!(s.prefs().toolbar.sets.len(), 0);
    }

    #[test]
    fn old_hidden_and_order_become_my_tools() {
        let mut s = session();
        s.load_prefs_json(r#"{"toolbar": {"hidden": ["Sponge", "burn"], "order": ["Brush"]}}"#).unwrap();
        let t = &s.prefs().toolbar;
        assert!(t.hidden.is_empty() && t.order.is_empty());
        let a = t.active();
        assert_eq!(a.name, "My Tools");
        assert_eq!(a.tools.first().map(String::as_str), Some("Brush"));
        assert!(!a.tools.iter().any(|x| x == "Sponge" || x == "Burn"));
        assert_eq!(a.tools.len(), ALL_TOOLS.len() - 2);
        // a default toolbar stays on All Tools
        let mut s = session();
        s.load_prefs_json(r#"{"toolbar": {}}"#).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
        assert!(s.prefs().toolbar.sets.is_empty());
        // and preferences with no toolbar key at all
        s.load_prefs_json(r#"{}"#).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
    }

    #[test]
    fn toolsets_reject_malformed_tools_without_mutation_and_accept_an_empty_layout() {
        let mut s = session();
        let original = s.prefs().toolbar.clone();
        for tools in [json!(true), json!("Move"), json!(["Move", 1]), json!([null]), json!(["Move", "Unknown"])] {
            assert!(s.execute("toolset.save", json!({"name": "Bad", "tools": tools})).is_err());
            assert_eq!(s.prefs().toolbar, original);
        }
        s.execute("toolset.save", json!({"name": "Empty", "tools": []})).unwrap();
        assert!(s.prefs().toolbar.active().tools.is_empty());
    }

    #[test]
    fn toolsets_long_names_duplicate_and_reset_keeps_custom_sets() {
        let mut s = session();
        let name = "a".repeat(MAX_NAME);
        let first = s.execute("toolset.save", json!({"name": name, "from": "photographer"})).unwrap();
        for _ in 0..3 {
            s.execute("toolset.duplicate", json!({"id": first["created"]})).unwrap();
        }
        assert_eq!(s.prefs().toolbar.sets.len(), 4);
        assert!(s.prefs().toolbar.sets.iter().all(|t| t.name.chars().count() <= MAX_NAME));
        s.execute("toolset.reset", json!({"id": "photographer"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, "photographer");
        s.execute("edit.toolbar", json!({"reset": true})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
        assert_eq!(s.prefs().toolbar.sets.len(), 4);
    }

    #[test]
    fn toolsets_default_legacy_order_and_prefs_set_migration() {
        let mut s = session();
        s.load_prefs_json(&json!({"toolbar": {"order": ALL_TOOLS}}).to_string()).unwrap();
        assert!(s.prefs().toolbar.sets.is_empty());
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
        s.execute("prefs.set", json!({"values": {"toolbar.hidden": ["Brush"]}})).unwrap();
        assert_eq!(s.prefs().toolbar.active().name, "My Tools");
        assert!(!s.prefs().toolbar.active().tools.iter().any(|t| t == "Brush"));
        s.execute("edit.toolbar", json!({"order": ["Zoom", "Move"]})).unwrap();
        assert_eq!(&s.prefs().toolbar.active().tools[..2], &["Zoom", "Move"]);
    }

    #[test]
    fn sets_survive_a_prefs_round_trip() {
        let mut s = session();
        s.execute("toolset.save", json!({"name": "Mine", "tools": ["Move", "Zoom"]})).unwrap();
        let json = s.prefs_to_json();
        let mut t = session();
        t.load_prefs_json(&json).unwrap();
        assert_eq!(t.prefs().toolbar.sets, s.prefs().toolbar.sets);
        assert_eq!(t.prefs().toolbar.active().name, "Mine");
        assert_eq!(t.prefs().toolbar.active().persona, Persona::Pixel);
    }
}
