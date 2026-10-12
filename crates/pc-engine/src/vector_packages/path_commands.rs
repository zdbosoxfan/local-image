//! Object › Path transactions, informed by VectorCraft cmd/{path,pathops}.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-NOTICE. Geometry is delegated to V1, never duplicated.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{Document, Layer, LayerContent, LayerId, Path};
use photocraft_pathops as ops;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn bad(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
pub const MAX_TARGETS: usize = 256;
pub const MAX_POINTS: usize = 65_536;
pub const MAX_CACHE_BYTES: u128 = 512 << 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub layers: Vec<LayerId>,
}
impl Capture {
    pub fn begin(s: &Session, layers: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let c = Self { document: st.doc.id.0, revision: st.revision, layers: layers.to_vec() };
        c.check(s)?;
        Ok(c)
    }
    /// Front/back without selected objects inserts at the document root's ends.
    pub fn begin_paste(s: &Session, layers: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let c = Self { document: st.doc.id.0, revision: st.revision, layers: layers.to_vec() };
        c.check_paste(s)?;
        Ok(c)
    }
    fn check_identity(&self, s: &Session) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if st.doc.id.0 != self.document || st.revision != self.revision {
            return Err(bad("Object command is stale; start again"));
        }
        Ok(())
    }
    pub fn check(&self, s: &Session) -> Result<()> {
        self.check_identity(s)?;
        validate_targets(&s.active().ok_or(EngineError::NoDocument)?.doc, &self.layers)
    }
    pub(crate) fn check_paste(&self, s: &Session) -> Result<()> {
        if self.layers.is_empty() { self.check_identity(s) } else { self.check(s) }
    }
}
pub(crate) fn validate_targets(doc: &Document, ids: &[LayerId]) -> Result<()> {
    if ids.is_empty() || ids.len() > MAX_TARGETS {
        return Err(bad("select 1–256 objects"));
    }
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    fn subtree(doc: &Document, layer: &Layer) -> Result<()> {
        let locks = doc.effective_locks(layer.id);
        if locks.all || locks.pixels || locks.position {
            return Err(bad("Object command target is locked"));
        }
        if let Some(children) = layer.children() {
            for child in children {
                subtree(doc, child)?;
            }
        }
        Ok(())
    }
    for id in ids {
        if !seen.insert(*id) {
            return Err(bad("duplicate Object command target"));
        }
        let p = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
        if paths.iter().any(|old: &Vec<usize>| p.starts_with(old) || old.starts_with(&p)) {
            return Err(bad("select roots without their descendants"));
        }
        paths.push(p);
        subtree(doc, doc.layer(*id).ok_or(EngineError::NoLayer(*id))?)?;
    }
    Ok(())
}
pub(crate) fn validate_path(path: &Path) -> Result<()> {
    ops::validate(path).map_err(|e| bad(e.to_string()))?;
    let points: Vec<_> = path.subpaths.iter().flat_map(|s| &s.knots).collect();
    if points.len() > MAX_POINTS || points.iter().flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).any(|p| p.x.abs() > 1_000_000. || p.y.abs() > 1_000_000.) {
        return Err(bad("path exceeds the finite coordinate/point budget"));
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Horizontal,
    Vertical,
    #[default]
    Both,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchors {
    pub layer: LayerId,
    pub points: Vec<[usize; 2]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Join {
        #[serde(default = "join_tolerance")]
        tolerance: f64,
    },
    Average {
        axis: Axis,
        anchors: Vec<Anchors>,
    },
    Simplify {
        precision: f64,
        corner_angle: f64,
    },
    AddAnchors,
    RemoveAnchors {
        anchors: Vec<Anchors>,
    },
    CleanUp {
        stray_points: bool,
        unpainted: bool,
        empty_text: bool,
    },
    DivideBelow,
    Grid {
        rows: usize,
        columns: usize,
        row_gutter: f64,
        column_gutter: f64,
        guides: bool,
    },
}
fn join_tolerance() -> f64 {
    1e-6
}
impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Join { .. } => "Join",
            Self::Average { .. } => "Average",
            Self::Simplify { .. } => "Simplify",
            Self::AddAnchors => "Add Anchor Points",
            Self::RemoveAnchors { .. } => "Remove Anchor Points",
            Self::CleanUp { .. } => "Clean Up",
            Self::DivideBelow => "Divide Objects Below",
            Self::Grid { .. } => "Split Into Grid",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capture: Capture,
    pub action: Action,
}
#[derive(Clone, Debug)]
pub struct Prepared {
    pub document: Document,
    pub layers: Vec<LayerId>,
    pub original_points: usize,
    pub new_points: usize,
}
fn number(v: f64, lo: f64, hi: f64) -> Result<()> {
    if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err(bad("numeric input must be finite and within range")) }
}
fn shape(layer: &Layer) -> Result<&photocraft_doc::ShapeLayer> {
    match &layer.content {
        LayerContent::Shape(s) => Ok(s),
        _ => Err(bad("Path command requires shape layers")),
    }
}
fn count(doc: &Document, ids: &[LayerId]) -> usize {
    fn layer_points(layer: &Layer) -> usize {
        match &layer.content {
            LayerContent::Shape(s) => s.path.subpaths.iter().map(|p| p.knots.len()).sum(),
            LayerContent::Group(g) => g.children.iter().map(layer_points).sum(),
            _ => 0,
        }
    }
    ids.iter().filter_map(|id| doc.layer(*id)).map(layer_points).sum()
}
fn selected_anchors(doc: &Document, capture: &Capture, selection: &[Anchors]) -> Result<Vec<(LayerId, usize, usize)>> {
    let mut seen = BTreeSet::new();
    let mut layers = BTreeSet::new();
    for a in selection {
        if !capture.layers.contains(&a.layer) || !layers.insert(a.layer) {
            return Err(bad("invalid anchor target"));
        }
        let sh = shape(doc.layer(a.layer).ok_or(EngineError::NoLayer(a.layer))?)?;
        for &[sp, knot] in &a.points {
            if sh.path.subpaths.get(sp).and_then(|s| s.knots.get(knot)).is_none() || !seen.insert((a.layer, sp, knot)) {
                return Err(bad("invalid or duplicate selected anchor"));
            }
        }
    }
    if seen.is_empty() || seen.len() > MAX_POINTS {
        return Err(bad("select anchors within the point budget"));
    }
    Ok(seen.into_iter().collect())
}
fn map_geometry(path: &Path, f: impl FnOnce(&mut ops::geom::PathData) -> Result<()>) -> Result<Path> {
    validate_path(path)?;
    let mut data = ops::to_geometry(path);
    f(&mut data)?;
    let mut out = ops::from_geometry(&data);
    out.fill_rule = path.fill_rule;
    out.inverted = path.inverted;
    for (sp, old) in out.subpaths.iter_mut().zip(&path.subpaths) {
        sp.op = old.op;
    }
    validate_path(&out)?;
    Ok(out)
}
fn replace_path(doc: &mut Document, id: LayerId, path: Path) -> Result<()> {
    validate_path(&path)?;
    let layer = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
    let LayerContent::Shape(sh) = &mut layer.content else {
        return Err(bad("shape disappeared"));
    };
    sh.path = path;
    sh.live = None;
    sh.psd_raw = None;
    layer.vector.live_shape = None;
    layer.vector.corners.clear();
    super::expand::clear_geometry_blocks(layer);
    Ok(())
}
/// Original geometry is always read afresh. A slider update never simplifies its last preview.
pub fn prepare(s: &Session, request: &Request) -> Result<Prepared> {
    request.capture.check(s)?;
    let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let mut doc = (**source).clone();
    let mut ids = request.capture.layers.clone();
    let original_points = count(source, &ids);
    if !matches!(request.action, Action::CleanUp { .. }) {
        for id in &ids {
            let l = doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))?;
            // Direct-selection coordinates address the original knots, not the extra knots
            // introduced by rounded-corner projection. Bake corners after those edits.
            let anchor_edit = matches!(request.action, Action::Average { .. } | Action::RemoveAnchors { .. });
            let mut base = l.clone();
            if anchor_edit {
                base.vector.corners.clear();
            }
            let projected = super::expand::project_shape_base(&base)?;
            l.content = LayerContent::Shape(projected);
            l.vector.live_shape = None;
            if !anchor_edit {
                l.vector.corners.clear();
            }
            super::expand::clear_geometry_blocks(l);
        }
    }
    match &request.action {
        Action::Join { tolerance } => {
            number(*tolerance, 0., 1000.)?;
            // Merging layers is only meaningful within one parent and one compatible paint.
            let first = doc.layer(ids[0]).ok_or(EngineError::NoLayer(ids[0]))?;
            let first_shape = shape(first)?;
            let parent = doc.path_of(ids[0]).ok_or(EngineError::NoLayer(ids[0]))?;
            for id in &ids {
                let l = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
                let sh = shape(l)?;
                let p = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
                if p[..p.len() - 1] != parent[..parent.len() - 1]
                    || sh.fill != first_shape.fill
                    || sh.stroke != first_shape.stroke
                    || l.vector.appearance != first.vector.appearance
                    || (ids.len() > 1
                        && (l.opacity != first.opacity
                            || l.fill_opacity != first.fill_opacity
                            || l.blend != first.blend
                            || l.blend_if != first.blend_if
                            || l.excluded_channels != first.excluded_channels
                            || l.clipped
                            || l.mask.is_some()
                            || l.vector_mask.is_some()
                            || photocraft_compose::effects::has_effects(l)
                            || l.effects.psd_raw.is_some()))
                {
                    return Err(bad("Join needs sibling paths with compatible paint and transparency"));
                }
            }
            let paths = ids.iter().map(|id| shape(doc.layer(*id).ok_or(EngineError::NoLayer(*id))?).map(|s| s.path.clone())).collect::<Result<Vec<_>>>()?;
            if paths.iter().flat_map(|p| &p.subpaths).filter(|sp| !sp.closed && sp.knots.len() >= 2).count() == 0 {
                return Err(bad("select open paths to join"));
            }
            replace_path(&mut doc, ids[0], ops::join(&paths, *tolerance).map_err(|e| bad(e.to_string()))?)?;
            for id in &ids[1..] {
                doc.remove(*id);
            }
            ids.truncate(1);
        }
        Action::Average { axis, anchors } => {
            let selected = selected_anchors(&doc, &request.capture, anchors)?;
            let n = selected.len() as f64;
            let mut centroid = [0., 0.];
            for &(id, sp, k) in &selected {
                let a = shape(doc.layer(id).ok_or(EngineError::NoLayer(id))?)?.path.subpaths[sp].knots[k].anchor;
                centroid[0] += a.x / n;
                centroid[1] += a.y / n;
            }
            for &(id, sp, k) in &selected {
                let l = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
                let LayerContent::Shape(sh) = &mut l.content else {
                    return Err(bad("shape disappeared"));
                };
                let a = &mut sh.path.subpaths[sp].knots[k];
                let dx = if *axis == Axis::Horizontal { 0. } else { centroid[0] - a.anchor.x };
                let dy = if *axis == Axis::Vertical { 0. } else { centroid[1] - a.anchor.y };
                for p in [&mut a.anchor, &mut a.in_ctrl, &mut a.out_ctrl] {
                    p.x += dx;
                    p.y += dy;
                }
            }
        }
        Action::Simplify { precision, corner_angle } => {
            number(*precision, 0., 100.)?;
            number(*corner_angle, 0., 180.)?;
            let opts = ops::SimplifyOptions { tolerance: precision_tolerance(*precision), corner_angle_deg: *corner_angle, straight_lines: false };
            for id in &ids {
                let path = shape(doc.layer(*id).ok_or(EngineError::NoLayer(*id))?)?.path.clone();
                let path = map_geometry(&path, |p| {
                    *p = ops::kernel::simplify_with(p, &opts);
                    Ok(())
                })?;
                replace_path(&mut doc, *id, path)?;
            }
        }
        Action::AddAnchors => {
            for id in &ids {
                let path = &shape(doc.layer(*id).ok_or(EngineError::NoLayer(*id))?)?.path;
                let out = map_geometry(path, |p| {
                    *p = ops::kernel::add_anchor_points(p);
                    Ok(())
                })?;
                replace_path(&mut doc, *id, out)?;
            }
        }
        Action::RemoveAnchors { anchors } => {
            let selected = selected_anchors(&doc, &request.capture, anchors)?;
            for id in &ids {
                let path = &shape(doc.layer(*id).ok_or(EngineError::NoLayer(*id))?)?.path;
                let out = map_geometry(path, |p| {
                    for &(_, sp, k) in selected.iter().rev().filter(|(layer, _, _)| layer == id) {
                        if !ops::kernel::remove_anchor(&mut p.subpaths[sp], k) {
                            return Err(bad("anchor disappeared"));
                        }
                    }
                    Ok(())
                })?;
                let corners = doc
                    .layer(*id)
                    .ok_or(EngineError::NoLayer(*id))?
                    .vector
                    .corners
                    .iter()
                    .filter_map(|c| {
                        if selected.contains(&(*id, c.subpath, c.knot)) {
                            return None;
                        }
                        let removed = selected.iter().filter(|(layer, sp, k)| layer == id && *sp == c.subpath && *k < c.knot).count();
                        Some(photocraft_doc::vector_design::drawing::KnotCorner { knot: c.knot - removed, ..*c })
                    })
                    .collect();
                replace_path(&mut doc, *id, out)?;
                doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))?.vector.corners = corners;
            }
        }
        Action::CleanUp { stray_points, unpainted, empty_text } => {
            let removed: Vec<_> = ids
                .iter()
                .copied()
                .filter(|id| {
                    doc.layer(*id).is_some_and(|l| match &l.content {
                        LayerContent::Shape(sh) => {
                            (*stray_points && !sh.path.inverted && sh.path.subpaths.iter().map(|p| p.knots.len()).sum::<usize>() <= 1)
                                || (*unpainted && l.vector.appearance.as_ref().map_or(sh.fill.is_none() && sh.stroke.is_none(), |a| a.items.is_empty()))
                        }
                        LayerContent::Text(t) => *empty_text && t.text.trim().is_empty(),
                        _ => false,
                    })
                })
                .collect();
            for id in &removed {
                doc.remove(*id);
            }
            ids.retain(|id| !removed.contains(id));
        }
        Action::Grid { rows, columns, row_gutter, column_gutter, guides } => {
            if *rows == 0 || *columns == 0 || rows.checked_mul(*columns).and_then(|n| n.checked_mul(ids.len())).is_none_or(|n| n > 4096) {
                return Err(bad("grid must contain 1–4096 cells"));
            }
            number(*row_gutter, 0., 1_000_000.)?;
            number(*column_gutter, 0., 1_000_000.)?;
            for id in &ids {
                let template = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?.clone();
                let (x0, y0, x1, y1) =
                    ops::bounds(&shape(&template)?.path).map_err(|e| bad(e.to_string()))?.ok_or_else(|| bad("grid needs nonempty geometry"))?;
                let w = (x1 - x0 - column_gutter * (*columns - 1) as f64) / *columns as f64;
                let h = (y1 - y0 - row_gutter * (*rows - 1) as f64) / *rows as f64;
                if w <= 0. || h <= 0. {
                    return Err(bad("gutters leave no positive grid cells"));
                }
                let mut children = Vec::new();
                for row in 0..*rows {
                    for column in 0..*columns {
                        let x = x0 + column as f64 * (w + column_gutter);
                        let y = y0 + row as f64 * (h + row_gutter);
                        let path = Path::new(vec![photocraft_doc::Subpath::polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])]);
                        children.push(paint_child(&template, path)?);
                        if *guides {
                            doc.guides.horizontal.extend([y as f32, (y + h) as f32]);
                            doc.guides.vertical.extend([x as f32, (x + w) as f32]);
                        }
                    }
                }
                let l = doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))?;
                make_group(l, children);
            }
            doc.guides.horizontal.sort_by(f32::total_cmp);
            doc.guides.horizontal.dedup();
            doc.guides.vertical.sort_by(f32::total_cmp);
            doc.guides.vertical.dedup();
        }
        Action::DivideBelow => {
            if ids.len() != 1 {
                return Err(bad("select exactly one cutter"));
            }
            let cutter = ids[0];
            let p = doc.path_of(cutter).ok_or(EngineError::NoLayer(cutter))?;
            // Document paint order includes ordinary groups below the cutter, as in the
            // upstream command. Live ancestors require their owner's projection first.
            let below: Vec<_> = doc
                .walk()
                .into_iter()
                .filter(|(at, _, l)| {
                    *at < p
                        && l.visible
                        && (1..at.len()).all(|n| doc.layer_at(&at[..n]).is_some_and(|parent| parent.visible))
                        && matches!(&l.content, LayerContent::Shape(sh) if !sh.path.subpaths.is_empty() && sh.path.subpaths.iter().all(|sp| sp.closed))
                })
                .map(|(_, _, l)| l.id)
                .collect();
            for id in &below {
                let at = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
                for n in 1..at.len() {
                    if doc.layer_at(&at[..n]).is_some_and(|l| l.vector.live.is_some()) {
                        return Err(bad("Divide requires expanding live objects below first"));
                    }
                }
            }
            if below.is_empty() {
                return Err(bad("no shapes below the cutter"));
            }
            validate_targets(&doc, &below)?;
            let cutter_path = shape(doc.layer(cutter).ok_or(EngineError::NoLayer(cutter))?)?.path.clone();
            let mut changed = Vec::new();
            for id in below {
                let mut template = doc.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
                template.content = LayerContent::Shape(super::expand::project_shape_base(&template)?);
                template.vector.corners.clear();
                template.vector.live_shape = None;
                let path = &shape(&template)?.path;
                let a = ops::boolean(path, &cutter_path, ops::BoolOp::Intersect).map_err(|e| bad(e.to_string()))?;
                let b = ops::boolean(path, &cutter_path, ops::BoolOp::Difference).map_err(|e| bad(e.to_string()))?;
                if a.is_empty() || b.is_empty() {
                    continue;
                }
                let children = vec![paint_child(&template, b)?, paint_child(&template, a)?];
                make_group(doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?, children);
                changed.push(id);
            }
            doc.remove(cutter);
            ids = changed;
        }
    }
    let mut bytes = 0;
    for id in &ids {
        let mut l = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?.clone();
        if matches!(request.action, Action::Average { .. } | Action::RemoveAnchors { .. }) && !l.vector.corners.is_empty() {
            l.content = LayerContent::Shape(super::expand::project_shape_base(&l)?);
            l.vector.corners.clear();
        }
        super::expand::refresh_tree(&doc, &mut l, &mut bytes)?;
        *doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))? = l;
    }
    if doc.max_group_depth() > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(bad("Object command exceeds group depth"));
    }
    let new_points = count(&doc, &ids);
    Ok(Prepared { document: doc, layers: ids, original_points, new_points })
}
pub fn precision_tolerance(precision: f64) -> f64 {
    0.01 + (100. - precision) / 100. * 4.99
}
fn paint_child(template: &Layer, path: Path) -> Result<Layer> {
    let mut sh = shape(template)?.clone();
    sh.path = path;
    sh.live = None;
    sh.psd_raw = None;
    sh.cache = None;
    let mut layer = Layer::new(template.name.clone(), LayerContent::Shape(sh));
    layer.vector.appearance = template.vector.appearance.clone();
    Ok(layer)
}
fn make_group(layer: &mut Layer, children: Vec<Layer>) {
    layer.content = LayerContent::Group(photocraft_doc::Group { children, expanded: true, artboard: None, isolation: photocraft_doc::GroupIsolation::Auto });
    layer.vector.appearance = None;
    layer.vector.live_shape = None;
    layer.vector.corners.clear();
    super::expand::clear_geometry_blocks(layer);
}
pub(crate) fn publish(s: &mut Session, capture: &Capture, label: &str, prepared: Prepared) -> Result<Value> {
    // Path/Expand preparation still requires nonempty targets; paste may capture root insertion.
    capture.check_paste(s)?;
    let ids = prepared.layers;
    let active_id = ids.first().copied();
    s.edit(label, move |doc, active| {
        *doc = prepared.document;
        *active = active_id;
        Ok(())
    })?;
    if let Some(st) = s.active_mut() {
        st.selected_layers = ids.clone();
        st.layer_anchor = ids.first().copied();
        st.history.set_current_layers(photocraft_ops::LayerTarget { active: st.active_layer, selected: ids.clone() });
    }
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()}))
}
fn request(v: &Value) -> Result<Request> {
    serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))
}
fn commit(s: &mut Session, v: &Value) -> Result<Value> {
    let r = request(v)?;
    let p = prepare(s, &r)?;
    publish(s, &r.capture, r.action.label(), p)
}
fn preview(s: &mut Session, v: &Value) -> Result<Value> {
    let p = prepare(s, &request(v)?)?;
    Ok(json!({"original_points":p.original_points,"new_points":p.new_points,"detached":true}))
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "object.path.commit",
            label: "Object Path",
            menu: &[],
            shortcut: None,
            params: "{capture:{document,revision,layers},action:Action}",
            enabled: crate::vector_cmds::has_doc,
            run: commit,
            journal: true,
        },
        CommandSpec {
            id: "object.path.preview",
            label: "Preview Object Path",
            menu: &[],
            shortcut: None,
            params: "{capture,action}",
            enabled: crate::vector_cmds::has_doc,
            run: preview,
            journal: false,
        },
    ]
}
#[path = "path_commands/clipboard.rs"]
pub mod clipboard;
#[cfg(test)]
mod tests;
