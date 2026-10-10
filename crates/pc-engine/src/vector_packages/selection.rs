//! Vector selection, adapted from VectorCraft engine/cmd/{select,wand}.rs and
//! tools/xform/wand.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Selection is session view state. No document edits, raster caches, revision increments,
//! journal entries or history steps are produced by this module.

use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
use photocraft_doc::{Color, DocId, Document, Fill, Layer, LayerContent, LayerId, LiveShape, Path};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const LIMIT: f64 = 1e7;
const MAX_LOOP: usize = 16_384;
fn error(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
fn bounded(v: f64, max: f64) -> Result<()> {
    if !v.is_finite() || v.abs() > max {
        return Err(error("selection input must be finite and bounded"));
    }
    Ok(())
}
fn point(p: [f64; 2]) -> Result<Point> {
    bounded(p[0], LIMIT)?;
    bounded(p[1], LIMIT)?;
    Ok(Point::new(p[0], p[1]))
}

/// False limits queries to the active artboard, including its nested contents.
/// Without artboards, it limits queries to siblings of the active object. At the root this
/// excludes group contents; inside a group it selects its immediate children. True
/// descends every eligible group. Isolation always limits both modes to that container.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub edit_all_layers: bool,
    pub select_hidden: bool,
    pub active: Option<LayerId>,
    pub isolation: Option<LayerId>,
}
impl Default for Scope {
    fn default() -> Self {
        Self { edit_all_layers: true, select_hidden: false, active: None, isolation: None }
    }
}
/// Package-owned per-document view memory, for DocState's additive coordinator bridge.
/// It is neither document persistence nor an undo state.
#[derive(Clone, Debug, Default)]
pub struct Memory {
    pub previous: Vec<LayerId>,
}

/// Package 00 installs the authoritative session scope (37) and per-document selection
/// memory. UI/menu/MCP share these hooks; no global selection history is kept here.
pub struct Hooks {
    pub scope: fn(&Session) -> Result<Scope>,
    pub previous: fn(&Session) -> Vec<LayerId>,
    pub remember: fn(&mut Session, Vec<LayerId>),
}
static HOOKS: OnceLock<Hooks> = OnceLock::new();
pub fn install_hooks(hooks: Hooks) -> std::result::Result<(), Hooks> {
    HOOKS.set(hooks)
}
/// Read-only providers: 08 may project the authoritative appearance, and 38 supplies
/// the style link. Missing style support is an error, never a match on absent links.
#[derive(Clone, Copy, Default)]
pub struct Readers {
    pub appearance: Option<fn(&Layer) -> Result<Appearance>>,
    pub graphic_style: Option<fn(&Layer) -> Result<Option<String>>>,
    pub rotation: Option<fn(&Layer) -> Result<Option<f64>>>,
    pub symbol_instance: Option<fn(&Layer) -> bool>,
}
static READERS: OnceLock<Readers> = OnceLock::new();
pub fn install_readers(readers: Readers) -> std::result::Result<(), Readers> {
    READERS.set(readers)
}
pub fn installed_readers() -> Readers {
    READERS.get().copied().unwrap_or_default()
}
/// Resolve the same authoritative scope for detached UI previews and command commits.
pub fn resolve_scope(session: &Session, fallback: &Scope) -> Result<Scope> {
    if let Some(h) = HOOKS.get() { (h.scope)(session) } else { Ok(fallback.clone()) }
}
pub fn session_scope(session: &Session) -> Result<Scope> {
    resolve_scope(session, &Scope { active: session.active().and_then(|st| st.active_layer), ..Default::default() })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Replace,
    Add,
    Subtract,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Compare {
    Less,
    LessEqual,
    #[default]
    Equal,
    GreaterEqual,
    Greater,
}
impl Compare {
    fn matches(self, a: f64, b: f64) -> bool {
        match self {
            Self::Less => a < b,
            Self::LessEqual => a <= b,
            Self::Equal => (a - b).abs() <= 1e-9,
            Self::GreaterEqual => a >= b,
            Self::Greater => a > b,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Same {
    FillColor,
    StrokeColor,
    FillAndStroke,
    StrokeWeight,
    Opacity,
    BlendMode,
    Appearance,
    AppearanceAttribute,
    GraphicStyle,
    ShapeType,
    Width,
    Height,
    Rotation,
    Name,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Object {
    AllOnSameLayers,
    OpenPaths,
    StrayPoints,
    ClippingMasks,
    TextObjects,
    Groups,
    SymbolInstances,
    Unfilled,
    Stroked,
    Unstroked,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Wand {
    pub fill_color: bool,
    /// 100 times Euclidean Oklab distance; no RGB-byte rounding.
    pub fill_tolerance: f64,
    pub stroke_color: bool,
    pub stroke_tolerance: f64,
    pub stroke_weight: bool,
    /// Document pixels, consistent with geometry; UI may convert design units.
    pub weight_tolerance: f64,
    pub opacity: bool,
    /// Percentage points, 0..100.
    pub opacity_tolerance: f64,
    pub blend_mode: bool,
}
impl Default for Wand {
    fn default() -> Self {
        Self {
            fill_color: true,
            fill_tolerance: 5.,
            stroke_color: false,
            stroke_tolerance: 5.,
            stroke_weight: false,
            weight_tolerance: 1.,
            opacity: false,
            opacity_tolerance: 5.,
            blend_mode: false,
        }
    }
}
impl Wand {
    pub fn validate(&self) -> Result<()> {
        for (v, max) in [(self.fill_tolerance, 200.), (self.stroke_tolerance, 200.), (self.weight_tolerance, LIMIT), (self.opacity_tolerance, 100.)] {
            bounded(v, max)?;
            if v < 0. {
                return Err(error("selection tolerance must be nonnegative"));
            }
        }
        if !(self.fill_color || self.stroke_color || self.stroke_weight || self.opacity || self.blend_mode) {
            return Err(error("enable at least one Magic Wand attribute"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Query {
    Same {
        criterion: Same,
        reference: LayerId,
        #[serde(default)]
        compare: Compare,
        #[serde(default)]
        value: Option<f64>,
        #[serde(default)]
        item: Option<usize>,
    },
    Object {
        criterion: Object,
    },
    Wand {
        reference: LayerId,
        #[serde(default)]
        settings: Wand,
    },
    Lasso {
        points: Vec<[f64; 2]>,
        #[serde(default)]
        nodes: bool,
    },
    Group {
        leaf: LayerId,
        #[serde(default)]
        previous: Option<LayerId>,
    },
    All,
    Deselect,
    Inverse,
    Next {
        above: bool,
    },
    Artboard {
        board: LayerId,
    },
    Reselect {
        ids: Vec<LayerId>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub document: DocId,
    pub revision: u64,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub mode: Mode,
    pub query: Query,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nodes {
    pub layer: LayerId,
    pub anchors: Vec<[usize; 2]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    pub document: DocId,
    pub revision: u64,
    pub scope: Scope,
    pub base: Vec<LayerId>,
    pub layers: Vec<LayerId>,
    /// Node selections are returned to 03's existing DirectSelection state, never saved
    /// into a second engine node-selection store. Object layer selection stays intact.
    pub nodes: Option<Vec<Nodes>>,
    pub bounds: Vec<(LayerId, [f64; 4])>,
}
struct Entry<'a> {
    layer: &'a Layer,
    parents: Vec<LayerId>,
    locked: bool,
    hidden: bool,
}
struct Index<'a> {
    entries: Vec<Entry<'a>>,
    by_id: HashMap<LayerId, usize>,
    scope: Scope,
    parent: Option<LayerId>,
    board: Option<LayerId>,
}
impl<'a> Index<'a> {
    fn new(doc: &'a Document, scope: &Scope) -> Result<Self> {
        fn walk<'a>(layers: &'a [Layer], parents: &mut Vec<LayerId>, locked: bool, hidden: bool, out: &mut Vec<Entry<'a>>) {
            for l in layers {
                let locked = locked || l.locks.all || l.locks.pixels;
                let hidden = hidden || !l.visible;
                out.push(Entry { layer: l, parents: parents.clone(), locked, hidden });
                if let Some(children) = l.children() {
                    parents.push(l.id);
                    walk(children, parents, locked, hidden, out);
                    parents.pop();
                }
            }
        }
        let mut entries = Vec::new();
        walk(&doc.layers, &mut Vec::new(), false, false, &mut entries);
        let by_id: HashMap<_, _> = entries.iter().enumerate().map(|(i, e)| (e.layer.id, i)).collect();
        if by_id.len() != entries.len() {
            return Err(error("duplicate document layer IDs"));
        }
        if let Some(id) = scope.isolation {
            let e = by_id.get(&id).map(|i| &entries[*i]).ok_or(EngineError::NoLayer(id))?;
            if !e.layer.is_group() {
                return Err(error("isolation must name a group"));
            }
        }
        let parent = match scope.active {
            Some(id) => by_id.get(&id).map(|i| entries[*i].parents.last().copied()).ok_or(EngineError::NoLayer(id))?,
            None => scope.isolation,
        };
        let board = scope.active.and_then(|id| by_id.get(&id)).and_then(|i| {
            let e = &entries[*i];
            std::iter::once(e.layer.id)
                .chain(e.parents.iter().rev().copied())
                .find(|id| by_id.get(id).is_some_and(|i| matches!(&entries[*i].layer.content,LayerContent::Group(g) if g.artboard.is_some())))
        });
        Ok(Self { entries, by_id, scope: scope.clone(), parent, board })
    }
    fn entry(&self, id: LayerId) -> Result<&Entry<'a>> {
        self.by_id.get(&id).map(|i| &self.entries[*i]).ok_or(EngineError::NoLayer(id))
    }
    fn allowed(&self, e: &Entry<'_>, descend: bool) -> bool {
        let in_layers = self.scope.edit_all_layers
            || self.board.map_or_else(
                || {
                    if descend { self.parent.is_none_or(|id| e.layer.id == id || e.parents.contains(&id)) } else { e.parents.last().copied() == self.parent }
                },
                |board| e.parents.contains(&board),
            );
        !e.locked && (self.scope.select_hidden || !e.hidden) && self.scope.isolation.is_none_or(|id| e.parents.contains(&id)) && in_layers
    }
    fn reference(&self, id: LayerId) -> Result<&Layer> {
        let e = self.entry(id)?;
        if !self.allowed(e, false) {
            return Err(error("selection reference is outside editable scope"));
        }
        Ok(e.layer)
    }
}
fn path(layer: &Layer) -> Option<&Path> {
    match &layer.content {
        LayerContent::Shape(s) => Some(&s.path),
        _ => layer.vector_mask.as_ref().map(|m| &m.path),
    }
}
fn appearance(layer: &Layer, readers: Readers) -> Result<Appearance> {
    if let Some(read) = readers.appearance {
        return read(layer);
    }
    if let Some(a) = &layer.vector.appearance {
        return Ok(a.clone());
    }
    let mut a = Appearance::default();
    match &layer.content {
        LayerContent::Shape(s) => {
            if let Some(p) = &s.fill {
                a.items.push(AppearanceItem::Fill(FillItem::new(p.clone())));
            }
            if let Some(s) = &s.stroke {
                a.items.push(AppearanceItem::Stroke(StrokeItem { base: s.clone(), ..Default::default() }));
            }
        }
        LayerContent::Text(t) => a.items.push(AppearanceItem::Fill(FillItem::new(Fill::Solid(t.color)))),
        LayerContent::Fill(f) => a.items.push(AppearanceItem::Fill(FillItem::new(f.clone()))),
        _ => {}
    }
    Ok(a)
}
fn fill(a: &Appearance) -> Option<&Fill> {
    a.items.iter().rev().find_map(|i| match i {
        AppearanceItem::Fill(f) if f.visible => Some(&f.paint),
        _ => None,
    })
}
fn stroke(a: &Appearance) -> Option<&photocraft_doc::ShapeStroke> {
    a.items.iter().rev().find_map(|i| match i {
        AppearanceItem::Stroke(s) if s.visible => Some(&s.base),
        _ => None,
    })
}
fn weight(a: &Appearance) -> f64 {
    stroke(a).map_or(0., |s| f64::from(s.width))
}
/// Oklab matrices from Björn Ottosson's published definition, using Color::to_rgb's
/// encoded sRGB followed by IEC sRGB decoding. Alpha is a separate appearance attribute.
pub fn delta_e_ok(a: Color, b: Color) -> Result<f64> {
    fn lab(c: Color) -> Result<[f64; 3]> {
        if c.c.iter().chain(std::iter::once(&c.alpha)).any(|v| !v.is_finite()) {
            return Err(error("invalid paint colour"));
        }
        let rgb = c.to_rgb().map(f64::from);
        let [r, g, b] = rgb.map(|v| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) });
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        Ok([
            0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        ])
    }
    let (a, b) = (lab(a)?, lab(b)?);
    Ok(100. * a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt())
}
fn similar(a: Option<&Fill>, b: Option<&Fill>, tolerance: f64) -> Result<bool> {
    match (a, b) {
        (Some(Fill::Solid(a)), Some(Fill::Solid(b))) => Ok(delta_e_ok(*a, *b)? <= tolerance),
        _ => Ok(a == b),
    }
}
fn bounds(l: &Layer) -> Result<Option<[f64; 4]>> {
    if let Some(p) = path(l) {
        return photocraft_pathops::bounds(p).map(|b| b.map(|(x0, y0, x1, y1)| [x0, y0, x1, y1])).map_err(|e| error(e.to_string()));
    }
    if let Some(children) = l.children() {
        let mut b: Option<[f64; 4]> = None;
        for c in children {
            if let Some(r) = bounds(c)? {
                b = Some(b.map_or(r, |b| [b[0].min(r[0]), b[1].min(r[1]), b[2].max(r[2]), b[3].max(r[3])]));
            }
        }
        return Ok(b);
    }
    Ok(l.surface().map(|s| {
        let r = s.content_bounds();
        [f64::from(r.x0), f64::from(r.y0), f64::from(r.x1), f64::from(r.y1)]
    }))
}
fn rotation(l: &Layer, readers: Readers) -> Result<Option<f64>> {
    if let Some(read) = readers.rotation {
        return read(l);
    }
    Ok(l.vector.live_shape.as_ref().map(|s| s.angle.to_degrees()))
}
fn shape_kind(l: &Layer) -> Result<Option<String>> {
    if let Some(s) = &l.vector.live_shape {
        return Ok(serde_json::to_value(&s.parameters).map_err(|e| error(e.to_string()))?["kind"].as_str().map(str::to_owned));
    }
    Ok(match &l.content {
        LayerContent::Shape(s) => s.live.as_ref().map(|s| {
            match s {
                LiveShape::Rect { .. } => "rect",
                LiveShape::Ellipse { .. } => "ellipse",
                LiveShape::Polygon { star_ratio, .. } => {
                    if *star_ratio < 1. {
                        "star"
                    } else {
                        "polygon"
                    }
                }
                LiveShape::Line { .. } => "line",
            }
            .to_owned()
        }),
        _ => None,
    })
}
fn same(a: &Layer, b: &Layer, criterion: Same, compare: Compare, value: Option<f64>, item: Option<usize>, readers: Readers) -> Result<bool> {
    let aa = appearance(a, readers)?;
    let ba = appearance(b, readers)?;
    Ok(match criterion {
        Same::FillColor => fill(&aa) == fill(&ba),
        Same::StrokeColor => stroke(&aa).map(|s| &s.paint) == stroke(&ba).map(|s| &s.paint),
        Same::FillAndStroke => fill(&aa) == fill(&ba) && stroke(&aa).map(|s| &s.paint) == stroke(&ba).map(|s| &s.paint),
        Same::StrokeWeight => compare.matches(weight(&ba), value.unwrap_or(weight(&aa))),
        Same::Opacity => compare.matches(f64::from(b.opacity), value.unwrap_or(f64::from(a.opacity))),
        Same::BlendMode => a.blend == b.blend,
        Same::Appearance => aa == ba && a.opacity == b.opacity && a.blend == b.blend && a.effects == b.effects,
        Same::AppearanceAttribute => {
            let i = item.unwrap_or_else(|| aa.items.len().saturating_sub(1));
            let attribute = aa.items.get(i).ok_or_else(|| error("appearance attribute index is missing"))?;
            ba.items.contains(attribute)
        }
        Same::GraphicStyle => {
            let read = readers.graphic_style.ok_or_else(|| error("Graphic Style requires package 38 reader"))?;
            let style = read(a)?.ok_or_else(|| error("reference has no graphic style link"))?;
            read(b)?.is_some_and(|b| b == style)
        }
        Same::ShapeType => shape_kind(a)?.is_some_and(|a| shape_kind(b).ok().flatten().is_some_and(|b| a == b)),
        Same::Width | Same::Height => {
            let axis = usize::from(criterion == Same::Height);
            let ab = bounds(a)?.ok_or_else(|| error("reference has no geometry bounds"))?;
            bounds(b)?.is_some_and(|b| compare.matches(b[axis + 2] - b[axis], value.unwrap_or(ab[axis + 2] - ab[axis])))
        }
        Same::Rotation => {
            let r = value.or(rotation(a, readers)?).ok_or_else(|| error("Rotation requires live shape or transform reader"))?;
            rotation(b, readers)?.is_some_and(|b| compare.matches(b, r))
        }
        Same::Name => a.name == b.name,
    })
}
fn wand(a: &Layer, b: &Layer, w: &Wand, readers: Readers) -> Result<bool> {
    let aa = appearance(a, readers)?;
    let ba = appearance(b, readers)?;
    Ok((!w.fill_color || similar(fill(&aa), fill(&ba), w.fill_tolerance)?)
        && (!w.stroke_color || similar(stroke(&aa).map(|s| &s.paint), stroke(&ba).map(|s| &s.paint), w.stroke_tolerance)?)
        && (!w.stroke_weight || (weight(&aa) - weight(&ba)).abs() <= w.weight_tolerance)
        && (!w.opacity || f64::from((a.opacity - b.opacity).abs()) * 100. <= w.opacity_tolerance)
        && (!w.blend_mode || a.blend == b.blend))
}
fn paintable(l: &Layer, readers: Readers) -> bool {
    matches!(l.content, LayerContent::Shape(_) | LayerContent::Text(_)) || readers.symbol_instance.is_some_and(|read| read(l))
}

/// Even-odd lasso enclosure, including its boundary. Adapted from VC's wand tool.
pub fn enclosed(poly: &[Point], p: Point) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    for (a, b) in edges(poly) {
        if distance_segment(p, a, b) <= 1e-9 {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}
fn edges(p: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    p.iter().copied().zip(p.iter().copied().cycle().skip(1)).take(p.len())
}
fn distance_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let sq = dx * dx + dy * dy;
    let t = if sq == 0. { 0. } else { ((p.x - a.x) * dx + (p.y - a.y) * dy) / sq }.clamp(0., 1.);
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}
fn intersects(a: Point, b: Point, c: Point, d: Point) -> bool {
    let cross = |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let (a1, a2, b1, b2) = (cross(a, b, c), cross(a, b, d), cross(c, d, a), cross(c, d, b));
    if a1 * a2 < 0. && b1 * b2 < 0. {
        return true;
    }
    [(a, c, d), (b, c, d), (c, a, b), (d, a, b)].iter().any(|(p, a, b)| distance_segment(*p, *a, *b) <= 1e-9)
}
fn touches(l: &Layer, poly: &[Point], readers: Readers) -> Result<bool> {
    if let Some(p) = path(l) {
        let a = appearance(l, readers)?;
        // V1 contains preserves compound holes, fill rule, and PSD component order.
        if fill(&a).is_some() {
            for q in poly {
                if photocraft_pathops::contains(p, *q).map_err(|e| error(e.to_string()))? {
                    return Ok(true);
                }
            }
        }
        let width = weight(&a) * 0.5;
        for sp in photocraft_pathops::flatten_path(p, 0.025) {
            let mut points: Vec<_> = sp.pts.iter().map(|(x, y)| Point::new(*x, *y)).collect();
            if sp.closed
                && let Some(first) = points.first().copied()
            {
                points.push(first);
            }
            if points.iter().any(|q| enclosed(poly, *q)) {
                return Ok(true);
            }
            for segment in points.windows(2) {
                for (a, b) in edges(poly) {
                    if intersects(segment[0], segment[1], a, b)
                        || (width > 0.
                            && [
                                distance_segment(a, segment[0], segment[1]),
                                distance_segment(b, segment[0], segment[1]),
                                distance_segment(segment[0], a, b),
                                distance_segment(segment[1], a, b),
                            ]
                            .into_iter()
                            .any(|d| d <= width))
                    {
                        return Ok(true);
                    }
                }
            }
        }
        return Ok(false);
    }
    if let Some(b) = bounds(l)? {
        let rect = [Point::new(b[0], b[1]), Point::new(b[2], b[1]), Point::new(b[2], b[3]), Point::new(b[0], b[3])];
        return Ok(rect.iter().any(|p| enclosed(poly, *p))
            || poly.iter().any(|p| enclosed(&rect, *p))
            || edges(&rect).any(|(a, b)| edges(poly).any(|(c, d)| intersects(a, b, c, d))));
    }
    Ok(false)
}
/// Leaf hit query; groups are ascended by Query::Group. Does not change selection.
pub fn hit(doc: &Document, scope: &Scope, p: [f64; 2], tolerance: f64, readers: Readers) -> Result<Option<LayerId>> {
    let q = point(p)?;
    bounded(tolerance, LIMIT)?;
    if tolerance < 0. {
        return Err(error("negative hit tolerance"));
    }
    let index = Index::new(doc, scope)?;
    for e in index.entries.iter().rev().filter(|e| index.allowed(e, true) && paintable(e.layer, readers)) {
        let a = appearance(e.layer, readers)?;
        if let Some(p) = path(e.layer) {
            let inside = fill(&a).is_some() && photocraft_pathops::contains(p, q).map_err(|e| error(e.to_string()))?;
            let nearest = photocraft_pathops::nearest(p, q).map_err(|e| error(e.to_string()))?;
            if inside || nearest.is_some_and(|(_, _, _, _, d)| d <= tolerance + weight(&a) * 0.5) {
                return Ok(Some(e.layer.id));
            }
        } else if bounds(e.layer)?.is_some_and(|b| q.x >= b[0] - tolerance && q.x <= b[2] + tolerance && q.y >= b[1] - tolerance && q.y <= b[3] + tolerance) {
            return Ok(Some(e.layer.id));
        }
    }
    Ok(None)
}

/// Detached query; capture includes source selection so concurrent selection changes
/// (which deliberately do not bump revision) invalidate publication as well.
pub fn preview(doc: &Document, revision: u64, base: &[LayerId], request: &Request, readers: Readers) -> Result<Preview> {
    if request.document != doc.id || request.revision != revision {
        return Err(error("stale vector selection gesture"));
    }
    let index = Index::new(doc, &request.scope)?;
    let mut hits = Vec::new();
    let mut node_hits = None;
    match &request.query {
        Query::Same { criterion, reference, compare, value, item } => {
            if let Some(v) = value {
                bounded(*v, LIMIT)?;
            }
            let reference = index.reference(*reference)?;
            for e in index.entries.iter().filter(|e| {
                index.allowed(e, false)
                    && (paintable(e.layer, readers)
                        || e.layer.is_group()
                            && matches!(
                                criterion,
                                Same::Name
                                    | Same::Width
                                    | Same::Height
                                    | Same::Opacity
                                    | Same::BlendMode
                                    | Same::Appearance
                                    | Same::AppearanceAttribute
                                    | Same::GraphicStyle
                            ))
            }) {
                if same(reference, e.layer, *criterion, *compare, *value, *item, readers)? {
                    hits.push(e.layer.id);
                }
            }
        }
        Query::Wand { reference, settings } => {
            settings.validate()?;
            let reference = index.reference(*reference)?;
            for e in index.entries.iter().filter(|e| index.allowed(e, false) && paintable(e.layer, readers)) {
                if wand(reference, e.layer, settings, readers)? {
                    hits.push(e.layer.id);
                }
            }
        }
        Query::Object { criterion } => {
            if *criterion == Object::SymbolInstances && readers.symbol_instance.is_none() {
                return Err(error("Symbol Instances requires package 15 reader"));
            }
            let parents: HashSet<_> = base.iter().map(|id| index.entry(*id).map(|e| e.parents.last().copied())).collect::<Result<_>>()?;
            for e in index.entries.iter().filter(|e| index.allowed(e, false)) {
                let l = e.layer;
                let a = appearance(l, readers)?;
                let yes = match criterion {
                    Object::AllOnSameLayers => paintable(l, readers) && parents.contains(&e.parents.last().copied()),
                    Object::OpenPaths => path(l).is_some_and(|p| p.subpaths.iter().any(|s| !s.closed && s.knots.len() >= 2)),
                    Object::StrayPoints => path(l).is_some_and(|p| p.subpaths.iter().any(|s| s.knots.len() == 1)),
                    Object::ClippingMasks => l.vector.clip.is_some() || l.vector_mask.as_ref().is_some_and(|m| m.enabled),
                    Object::TextObjects => matches!(l.content, LayerContent::Text(_)),
                    Object::Groups => l.is_group(),
                    Object::SymbolInstances => (readers.symbol_instance.ok_or_else(|| error("Symbol Instances requires package 15 reader"))?)(l),
                    Object::Unfilled => paintable(l, readers) && fill(&a).is_none(),
                    Object::Stroked => paintable(l, readers) && stroke(&a).is_some(),
                    Object::Unstroked => paintable(l, readers) && stroke(&a).is_none(),
                };
                if yes {
                    hits.push(l.id);
                }
            }
        }
        Query::Lasso { points, nodes } => {
            if !(3..=MAX_LOOP).contains(&points.len()) {
                return Err(error("lasso requires 3..16384 points"));
            }
            let poly = points.iter().map(|p| point(*p)).collect::<Result<Vec<_>>>()?;
            if *nodes {
                let mut selected = Vec::new();
                for id in base {
                    let e = index.entry(*id)?;
                    if !index.allowed(e, false) {
                        continue;
                    }
                    if let Some(p) = path(e.layer) {
                        let anchors = p
                            .subpaths
                            .iter()
                            .enumerate()
                            .flat_map(|(s, sp)| {
                                sp.knots.iter().enumerate().filter_map({
                                    let poly = &poly;
                                    move |(k, n)| enclosed(poly, n.anchor).then_some([s, k])
                                })
                            })
                            .collect();
                        selected.push(Nodes { layer: *id, anchors });
                    }
                }
                node_hits = Some(selected);
            } else {
                for e in index.entries.iter().filter(|e| index.allowed(e, false) && paintable(e.layer, readers)) {
                    if touches(e.layer, &poly, readers)? {
                        hits.push(e.layer.id);
                    }
                }
            }
        }
        Query::Group { leaf, previous } => {
            let e = index.entry(*leaf)?;
            if !index.allowed(e, true) || !paintable(e.layer, readers) {
                return Err(error("group selection leaf is outside editable scope"));
            }
            let chain: Vec<_> = std::iter::once(*leaf)
                .chain(
                    e.parents
                        .iter()
                        .rev()
                        .copied()
                        .take_while(|id| Some(*id) != request.scope.isolation && index.entry(*id).is_ok_and(|e| index.allowed(e, true))),
                )
                .collect();
            let next = previous.and_then(|id| chain.iter().position(|i| *i == id)).map_or(0, |i| (i + 1).min(chain.len() - 1));
            hits.push(chain[next]);
        }
        Query::All | Query::Inverse => {
            let set: HashSet<_> = base.iter().copied().collect();
            hits.extend(
                index
                    .entries
                    .iter()
                    .filter(|e| {
                        index.allowed(e, false) && paintable(e.layer, readers) && (!matches!(request.query, Query::Inverse) || !set.contains(&e.layer.id))
                    })
                    .map(|e| e.layer.id),
            );
        }
        Query::Deselect => {}
        Query::Next { above } => {
            let list: Vec<_> = index.entries.iter().filter(|e| index.allowed(e, false) && paintable(e.layer, readers)).map(|e| e.layer.id).collect();
            let position = base.iter().filter_map(|id| list.iter().position(|i| i == id));
            let p = if *above { position.max().and_then(|i| i.checked_add(1)) } else { position.min().and_then(|i| i.checked_sub(1)) };
            if let Some(id) = p.and_then(|i| list.get(i)) {
                hits.push(*id);
            } else {
                hits.extend(base.iter().copied());
            }
        }
        Query::Artboard { board } => {
            let e = index.entry(*board)?;
            let LayerContent::Group(g) = &e.layer.content else {
                return Err(error("active artboard must be a group"));
            };
            let board = g.artboard.as_ref().ok_or_else(|| error("active group is not an artboard"))?;
            let rect = board.rect;
            for e in index.entries.iter().filter(|e| index.allowed(e, false) && paintable(e.layer, readers)) {
                if bounds(e.layer)?
                    .is_some_and(|b| b[2] >= f64::from(rect.x0) && b[0] <= f64::from(rect.x1) && b[3] >= f64::from(rect.y0) && b[1] <= f64::from(rect.y1))
                {
                    hits.push(e.layer.id);
                }
            }
        }
        Query::Reselect { ids } => {
            for id in ids {
                let e = index.entry(*id)?;
                if index.allowed(e, false) {
                    hits.push(*id);
                }
            }
        }
    }
    let set: HashSet<_> = hits.iter().copied().collect();
    let mut wanted: HashSet<_> = match request.mode {
        Mode::Replace => set,
        Mode::Add => base.iter().copied().chain(hits).collect(),
        Mode::Subtract => base.iter().copied().filter(|id| !set.contains(id)).collect(),
    };
    if node_hits.is_some() {
        wanted = base.iter().copied().collect();
    }
    let layers: Vec<_> = index.entries.iter().filter(|e| wanted.contains(&e.layer.id)).map(|e| e.layer.id).collect();
    let mut prepared = Vec::new();
    for id in &layers {
        let e = index.entry(*id)?;
        if !index.allowed(e, matches!(request.query, Query::Group { .. })) {
            return Err(error("selection target is outside editable scope"));
        }
        if let Some(b) = bounds(e.layer)? {
            for v in b {
                bounded(v, LIMIT)?;
            }
            prepared.push((*id, b));
        }
    }
    Ok(Preview { document: doc.id, revision, scope: request.scope.clone(), base: base.to_vec(), layers, nodes: node_hits, bounds: prepared })
}
/// Validate the entire target set before publishing any session field. No Session::edit:
/// caches and overlays are detached and content pixels never need regeneration.
pub fn apply(session: &mut Session, p: &Preview) -> Result<()> {
    let st = session.active().ok_or(EngineError::NoDocument)?;
    if HOOKS.get().is_some_and(|h| !matches!((h.scope)(session),Ok(scope) if scope==p.scope)) {
        return Err(error("stale vector selection scope"));
    }
    if st.doc.id != p.document || st.revision != p.revision || st.selected_layers() != p.base {
        return Err(error("stale vector selection gesture"));
    }
    let index = Index::new(&st.doc, &p.scope)?;
    let mut unique = HashSet::new();
    for id in &p.layers {
        let e = index.entry(*id)?;
        if !unique.insert(*id) || !index.allowed(e, true) {
            return Err(error("invalid vector selection target set"));
        }
    }
    if let Some(nodes) = &p.nodes {
        let mut seen = HashSet::new();
        for n in nodes {
            let e = index.entry(n.layer)?;
            if !seen.insert(n.layer) || !index.allowed(e, false) || !p.base.contains(&n.layer) {
                return Err(error("invalid node selection target"));
            }
            let path = path(e.layer).ok_or_else(|| error("node selection requires a curve"))?;
            for [s, k] in &n.anchors {
                if path.subpaths.get(*s).and_then(|sp| sp.knots.get(*k)).is_none() {
                    return Err(error("selected node is missing"));
                }
            }
        }
        return Ok(());
    }
    let before = st.selected_layers();
    let st = session.active_mut().ok_or(EngineError::NoDocument)?;
    st.selected_layers = p.layers.clone();
    st.active_layer = p.layers.last().copied();
    st.layer_anchor = st.active_layer;
    if before != p.layers
        && !before.is_empty()
        && let Some(h) = HOOKS.get()
    {
        (h.remember)(session, before);
    }
    Ok(())
}
pub fn run(session: &mut Session, request: &Request) -> Result<Preview> {
    let mut r = request.clone();
    if let Some(h) = HOOKS.get() {
        r.scope = (h.scope)(session)?;
        if matches!(r.query, Query::Reselect { .. }) {
            r.query = Query::Reselect { ids: (h.previous)(session) };
        }
    }
    let st = session.active().ok_or(EngineError::NoDocument)?;
    let p = preview(&st.doc, st.revision, &st.selected_layers(), &r, READERS.get().copied().unwrap_or_default())?;
    apply(session, &p)?;
    Ok(p)
}
fn command(session: &mut Session, value: &Value) -> Result<Value> {
    let request: Request = serde_json::from_value(value.clone()).map_err(|e| error(format!("vector.select: {e}")))?;
    Ok(json!(run(session, &request)?))
}
fn query_command(session: &mut Session, value: &Value) -> Result<Value> {
    let mut request: Request = serde_json::from_value(value.clone()).map_err(|e| error(format!("vector.select.query: {e}")))?;
    if let Some(h) = HOOKS.get() {
        request.scope = (h.scope)(session)?;
        if matches!(request.query, Query::Reselect { .. }) {
            request.query = Query::Reselect { ids: (h.previous)(session) };
        }
    }
    let st = session.active().ok_or(EngineError::NoDocument)?;
    Ok(json!(preview(&st.doc, st.revision, &st.selected_layers(), &request, READERS.get().copied().unwrap_or_default())?))
}
fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}
pub fn specs() -> Vec<CommandSpec> {
    [("vector.select", command as fn(&mut Session, &Value) -> Result<Value>), ("vector.select.query", query_command)]
        .into_iter()
        .map(|(id, run)| CommandSpec {
            id,
            label: "Vector Selection",
            menu: &[],
            shortcut: None,
            params: "{document,revision,scope?:Scope,mode?:replace|add|subtract,query:Query} → Preview",
            enabled: has_doc,
            run,
            journal: false,
        })
        .collect()
}
#[cfg(test)]
#[path = "selection/gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "selection/tests.rs"]
mod tests;
