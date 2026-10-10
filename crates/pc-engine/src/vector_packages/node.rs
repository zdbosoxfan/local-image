//! Node editing transactions on detached snapshots. Gesture guards are checked before render
//! work; every target is validated, and topology plus live-corner provenance publish together.
//! Node conversion/deletion follows VectorCraft tools/direct.rs and pathops/edit.rs,
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092, MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.

use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{Document, LayerContent, LayerId, Path};
use photocraft_geom::{Affine, Point};
use photocraft_vector::edit::{self, Handle, Origins, PointKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Target {
    pub name: String,
    pub layer: Option<u64>,
    pub anchors: Vec<[usize; 2]>,
}
impl Default for Target {
    fn default() -> Self {
        Self { name: "work".into(), layer: None, anchors: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Operation {
    Move {
        delta: [f64; 2],
    },
    Handle {
        knot: [usize; 2],
        handle: String,
        to: [f64; 2],
        #[serde(default)]
        independent: bool,
    },
    Bend {
        knot: [usize; 2],
        t: f64,
        delta: [f64; 2],
    },
    Convert {
        knot: [usize; 2],
        out: Option<[f64; 2]>,
    },
    Insert {
        knot: [usize; 2],
        t: f64,
    },
    Delete,
    Sharp,
    Smooth,
    Smart,
    Break,
    Join {
        tolerance: f64,
    },
    Close,
    Reverse,
    Align {
        axis: String,
        value: f64,
    },
    Transform {
        matrix: [f64; 6],
    },
}

/// Package 00 installs thin adapters to V1's actual join/split_at exports. There is no second
/// join/split implementation here. An absent provider is an error, never successful geometry.
pub type Nearest = Option<(usize, usize, f64, Point, f64)>;
pub struct Geometry {
    pub nearest: fn(&Path, Point) -> std::result::Result<Nearest, String>,
    pub join: fn(&[Path], f64) -> std::result::Result<Path, String>,
    pub split: fn(&Path, usize, usize, f64) -> std::result::Result<Vec<Path>, String>,
}
static GEOMETRY: OnceLock<Geometry> = OnceLock::new();
pub fn install_geometry(geometry: Geometry) -> std::result::Result<(), Geometry> {
    GEOMETRY.set(geometry)
}

/// V1 refines segment hit testing when integrated, preserving handle/anchor precedence.
pub fn hit(path: &Path, p: Point, tolerance: f64, handles: &[([usize; 2], Handle)]) -> Option<edit::Hit> {
    let legacy = edit::hit(path, p, tolerance, handles);
    if matches!(legacy, Some(edit::Hit::Anchor(_) | edit::Hit::Handle(..))) {
        return legacy;
    }
    if let Some(geometry) = GEOMETRY.get() {
        return (geometry.nearest)(path, p).ok().flatten().filter(|(_, _, _, _, d)| *d <= tolerance).map(|(s, k, t, _, _)| edit::Hit::Segment([s, k], t));
    }
    legacy
}

/// 00 adapts the shared 06/08 feature projection/paint renderer here. The same callback
/// prepares detached previews and commits, preserving metadata-backed corners/appearance.
pub type RenderCache = fn(&Document, &photocraft_doc::ShapeLayer, &photocraft_doc::VectorData) -> Result<photocraft_doc::Surface>;
static RENDER_CACHE: OnceLock<RenderCache> = OnceLock::new();
pub fn install_renderer(render: RenderCache) -> std::result::Result<(), RenderCache> {
    RENDER_CACHE.set(render)
}

fn error(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
fn finite(values: &[f64]) -> Result<()> {
    if values.iter().any(|v| !v.is_finite() || v.abs() > edit::MAX_COORDINATE) {
        return Err(error("node input must be finite and bounded"));
    }
    Ok(())
}

fn work_name(name: &str) -> bool {
    name.is_empty() || name.eq_ignore_ascii_case("work") || name == "Work Path"
}

pub fn path<'a>(doc: &'a Document, target: &Target) -> Result<&'a Path> {
    if target.name == "layer" {
        let id = LayerId(target.layer.ok_or_else(|| error("layer target requires an explicit id"))?);
        let l = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
        let locks = doc.effective_locks(id);
        if locks.all || locks.pixels {
            return Err(error("node target is locked"));
        }
        if let LayerContent::Shape(sh) = &l.content {
            return Ok(&sh.path);
        }
        return l.vector_mask.as_ref().map(|vm| &vm.path).ok_or_else(|| error("layer has no editable curve"));
    }
    if target.layer.is_some() {
        return Err(error("layer id requires name=layer"));
    }
    if work_name(&target.name) {
        return doc.work_path.as_ref().ok_or_else(|| error("no work path"));
    }
    doc.paths.iter().find(|p| p.name == target.name).map(|p| &p.path).ok_or_else(|| error("saved path is missing"))
}

/// Pure preview evaluator, shared by the UI and command transaction. No Session mutation.
pub fn evaluate(base: &Path, anchors: &[[usize; 2]], op: &Operation) -> Result<(Path, Origins)> {
    edit::validate(base).map_err(error)?;
    for [s, k] in anchors {
        if base.subpaths.get(*s).and_then(|sp| sp.knots.get(*k)).is_none() {
            return Err(error("selected node is missing"));
        }
    }
    let mut p = base.clone();
    let mut map = edit::origins(base);
    let geometry_error = |e: String| error(e);
    match op {
        Operation::Move { delta } => {
            finite(delta)?;
            edit::move_anchors(&mut p, anchors, *delta).map_err(geometry_error)?;
        }
        Operation::Handle { knot, handle, to, independent } => {
            finite(to)?;
            let h = Handle::parse(handle).ok_or_else(|| error("handle must be in or out"))?;
            edit::move_handle(&mut p, *knot, h, Point::new(to[0], to[1]), *independent).map_err(geometry_error)?;
        }
        Operation::Bend { knot, t, delta } => {
            finite(delta)?;
            if !t.is_finite() || !(0.0..=1.0).contains(t) {
                return Err(error("bend t must be in 0..1"));
            }
            edit::bend_segment(&mut p, *knot, *t, *delta).map_err(geometry_error)?;
        }
        Operation::Convert { knot, out } => {
            if let Some(out) = out {
                finite(out)?;
            }
            edit::convert_point(&mut p, *knot, out.map(|p| Point::new(p[0], p[1]))).map_err(geometry_error)?;
        }
        Operation::Insert { knot, t } => {
            map = edit::insert(&mut p, *knot, *t).map_err(geometry_error)?;
        }
        Operation::Delete => {
            map = edit::delete(&mut p, anchors).map_err(geometry_error)?;
        }
        Operation::Sharp | Operation::Smooth | Operation::Smart => {
            let kind = match op {
                Operation::Sharp => PointKind::Sharp,
                Operation::Smooth => PointKind::Smooth,
                _ => PointKind::Smart,
            };
            edit::set_kind(&mut p, anchors, kind).map_err(geometry_error)?;
        }
        Operation::Reverse => {
            map = edit::reverse_nodes(&mut p, anchors).map_err(geometry_error)?;
        }
        Operation::Close => edit::close_nodes(&mut p, anchors).map_err(geometry_error)?,
        Operation::Align { axis, value } => {
            finite(&[*value])?;
            if axis != "x" && axis != "y" {
                return Err(error("align axis must be x or y"));
            }
            for r @ [s, k] in anchors.iter().copied().collect::<HashSet<_>>() {
                let a = p.subpaths[s].knots[k].anchor;
                let d = if axis == "x" { [value - a.x, 0.0] } else { [0.0, value - a.y] };
                edit::move_anchors(&mut p, &[r], d).map_err(geometry_error)?;
            }
        }
        Operation::Transform { matrix } => {
            finite(matrix)?;
            edit::transform_nodes(&mut p, anchors, &Affine { m: *matrix }).map_err(geometry_error)?;
        }
        Operation::Break => {
            let geometry = GEOMETRY.get().ok_or_else(|| error("Break requires the V1 split_at provider (package 00)"))?;
            if anchors.is_empty() {
                return Err(error("break requires selected nodes"));
            }
            let mut selected = anchors.to_vec();
            selected.sort_unstable();
            selected.dedup();
            let mut changed = false;
            for old in selected {
                let position = map.iter().enumerate().find_map(|(s, sp)| {
                    sp.iter().enumerate().find_map(|(k, origin)| {
                        let path = &p.subpaths[s];
                        (*origin == Some(old) && (path.closed || (k > 0 && k + 1 < path.knots.len()))).then_some((s, k))
                    })
                });
                let Some((s, k)) = position else { continue };
                let sp = &p.subpaths[s];
                let old_map = &map[s];
                let single = Path { subpaths: vec![sp.clone()], ..p.clone() };
                let parts = (geometry.split)(&single, 0, k, 0.0).map_err(error)?;
                let pieces: Vec<_> = parts
                    .into_iter()
                    .flat_map(|p| p.subpaths)
                    .map(|mut piece| {
                        piece.op = sp.op;
                        piece
                    })
                    .collect();
                let expected: Origins = if sp.closed {
                    vec![(0..=sp.knots.len()).map(|i| old_map[(k + i) % sp.knots.len()]).collect()]
                } else {
                    vec![old_map[..=k].to_vec(), old_map[k..].to_vec()]
                };
                if pieces.len() != expected.len() || pieces.iter().zip(&expected).any(|(p, m)| p.knots.len() != m.len()) {
                    return Err(error("V1 split provenance does not match its documented cut contract"));
                }
                p.subpaths.splice(s..=s, pieces);
                map.splice(s..=s, expected);
                changed = true;
            }
            if !changed {
                return Err(error("selected open endpoints are already broken"));
            }
        }
        Operation::Join { .. } => return Err(error("join requires the multi-target transaction")),
    }
    edit::validate(&p).map_err(error)?;
    Ok((p, map))
}

fn provenance(base: &Path, output: &Path) -> Result<Origins> {
    output
        .subpaths
        .iter()
        .map(|sp| {
            sp.knots
                .iter()
                .map(|k| {
                    let candidates: Vec<_> = base
                        .subpaths
                        .iter()
                        .enumerate()
                        .flat_map(|(s, sp)| sp.knots.iter().enumerate().map(move |(i, old)| ([s, i], old)))
                        .filter(|(_, old)| old.anchor == k.anchor)
                        .collect();
                    match candidates.as_slice() {
                        [] => Ok(None),
                        [(r, _)] => Ok(Some(*r)),
                        // A join merges coincident endpoint knots. Prefer an unchanged knot, then
                        // its reversed form, then the first source endpoint (receiving paint order).
                        // That endpoint's corner wins deterministically; the other endpoint ceases
                        // to exist, exactly as its duplicate node index does.
                        _ => Ok(candidates
                            .iter()
                            .find(|(_, old)| **old == *k || (old.in_ctrl == k.out_ctrl && old.out_ctrl == k.in_ctrl))
                            .or_else(|| candidates.first())
                            .map(|(r, _)| *r)),
                    }
                })
                .collect()
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct Prepared {
    pub doc: Document,
    pub targets: Vec<Target>,
}

/// Validate the entire target set before changing any geometry or preparing render caches.
pub fn prepare(doc: &Document, targets: &[Target], op: &Operation, convert_live: bool) -> Result<Prepared> {
    if targets.is_empty() {
        return Err(error("select at least one curve"));
    }
    let shape_ids: Vec<_> =
        targets.iter().filter_map(|t| t.layer.map(LayerId)).filter(|id| doc.layer(*id).is_some_and(|l| matches!(l.content, LayerContent::Shape(_)))).collect();
    if !shape_ids.is_empty() {
        crate::vector_api::snapshot_shapes(doc, &shape_ids)?;
    }
    let mut seen = HashSet::new();
    for target in targets {
        if !seen.insert((if work_name(&target.name) { "work" } else { target.name.as_str() }, target.layer)) {
            return Err(error("duplicate node target"));
        }
        let base = path(doc, target)?;
        edit::validate(base).map_err(error)?;
        if target.anchors.iter().any(|[s, k]| base.subpaths.get(*s).and_then(|sp| sp.knots.get(*k)).is_none()) {
            return Err(error("selected node is missing"));
        }
        if let Some(l) = target.layer.and_then(|id| doc.layer(LayerId(id))) {
            if (matches!(&l.content, LayerContent::Shape(sh) if sh.live.is_some()) || l.vector.live_shape.is_some()) && !convert_live {
                return Err(error("Convert to curves before editing this live shape"));
            }
            if l.vector.appearance.is_some() && RENDER_CACHE.get().is_none() {
                return Err(error("node editing appearance requires package 08 renderer"));
            }
            for c in &l.vector.corners {
                if base.subpaths.get(c.subpath).and_then(|sp| sp.knots.get(c.knot)).is_none()
                    || !c.corner.radius.is_finite()
                    || c.corner.radius < 0.0
                    || c.corner.radius > edit::MAX_COORDINATE
                {
                    return Err(error("invalid corner reference"));
                }
            }
        }
    }
    let mut shown = doc.clone();
    let mut remapped = Vec::new();
    if let Operation::Join { tolerance } = op {
        finite(&[*tolerance])?;
        if *tolerance < 0.0 {
            return Err(error("join requires nonnegative tolerance"));
        }
        let geometry = GEOMETRY.get().ok_or_else(|| error("Join requires the V1 join provider (package 00)"))?;
        // Join into the first curve, retaining its paint. Consumed shape layers are removed
        // in the same transaction; a vector mask donor keeps its underlying pixel layer.
        let bases: Vec<_> = targets.iter().map(|t| path(doc, t).cloned()).collect::<Result<_>>()?;
        let joined = (geometry.join)(&bases, *tolerance).map_err(error)?;
        edit::validate(&joined).map_err(error)?;
        let mut combined = Path::default();
        let mut corners = Vec::new();
        for (target, base) in targets.iter().zip(&bases) {
            let offset = combined.subpaths.len();
            if let Some(l) = target.layer.and_then(|id| doc.layer(LayerId(id))) {
                corners.extend(l.vector.corners.iter().map(|c| photocraft_doc::vector_design::drawing::KnotCorner { subpath: c.subpath + offset, ..*c }));
            }
            combined.subpaths.extend(base.subpaths.clone());
        }
        let joined_map = provenance(&combined, &joined)?;
        let joined_corners: Vec<_> = joined_map
            .iter()
            .enumerate()
            .flat_map(|(s, sp)| {
                sp.iter().enumerate().filter_map({
                    let corners = &corners;
                    move |(k, old)| {
                        corners.iter().find(|c| *old == Some([c.subpath, c.knot])).map(|c| photocraft_doc::vector_design::drawing::KnotCorner {
                            subpath: s,
                            knot: k,
                            corner: c.corner,
                        })
                    }
                })
            })
            .collect();
        for (i, target) in targets.iter().enumerate() {
            let p = if i == 0 { joined.clone() } else { Path::default() };
            publish(&mut shown, target, p, &Vec::new())?;
            if let Some(l) = target.layer.and_then(|id| shown.layer_mut(LayerId(id))) {
                l.vector.corners = if i == 0 { joined_corners.clone() } else { Vec::new() };
            }
            if i > 0 && target.layer.is_some_and(|id| doc.layer(LayerId(id)).is_some_and(|l| matches!(l.content, LayerContent::Shape(_)))) {
                let id = LayerId(target.layer.ok_or_else(|| error("missing join donor"))?);
                shown.remove(id).ok_or(EngineError::NoLayer(id))?;
            } else {
                remapped.push(Target { anchors: Vec::new(), ..target.clone() });
            }
        }
    } else {
        // Geometry first, caches second: an invalid last target does no expensive raster work.
        let evaluated = targets.iter().map(|t| evaluate(path(doc, t)?, &t.anchors, op)).collect::<Result<Vec<_>>>()?;
        for (target, (path, map)) in targets.iter().zip(evaluated) {
            let anchors = map
                .iter()
                .enumerate()
                .flat_map(|(s, sp)| sp.iter().enumerate().filter_map(move |(k, old)| old.filter(|old| target.anchors.contains(old)).map(|_| [s, k])))
                .collect();
            publish(&mut shown, target, path, &map)?;
            remapped.push(Target { anchors, ..target.clone() });
        }
    }
    for target in targets {
        if let Some(id) = target.layer {
            let rendered = if let Some(l) = shown.layer(LayerId(id))
                && let LayerContent::Shape(sh) = &l.content
            {
                let cache = if let Some(render) = RENDER_CACHE.get() {
                    render(&shown, sh, &l.vector)?
                } else {
                    photocraft_vector::render_shape(sh, shown.pixel_format(), shown.bounds())
                };
                Some(cache)
            } else {
                None
            };
            if let Some(cache) = rendered
                && let Some(l) = shown.layer_mut(LayerId(id))
                && let LayerContent::Shape(sh) = &mut l.content
            {
                sh.cache = Some(cache);
            }
        }
    }
    Ok(Prepared { doc: shown, targets: remapped })
}

fn publish(doc: &mut Document, target: &Target, p: Path, map: &Origins) -> Result<()> {
    if target.name == "layer" {
        let id = LayerId(target.layer.ok_or_else(|| error("missing layer"))?);
        let l = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
        if let LayerContent::Shape(sh) = &mut l.content {
            sh.path = p;
            sh.live = None;
            sh.psd_raw = None;
        } else {
            l.vector_mask.as_mut().ok_or_else(|| error("missing mask"))?.path = p;
        }
        let corners = l.vector.corners.clone();
        l.vector.corners = map
            .iter()
            .enumerate()
            .flat_map(|(s, sp)| {
                sp.iter().enumerate().flat_map({
                    let corners = &corners;
                    move |(k, old)| {
                        corners.iter().filter_map(move |c| {
                            (*old == Some([c.subpath, c.knot])).then_some(photocraft_doc::vector_design::drawing::KnotCorner {
                                subpath: s,
                                knot: k,
                                corner: c.corner,
                            })
                        })
                    }
                })
            })
            .collect();
        l.vector.live_shape = None;
    } else {
        *crate::vector_cmds::path_mut(doc, &target.name, None)? = p;
    }
    Ok(())
}

pub fn guarded(s: &Session, p: &Value) -> Result<()> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    for (key, expected) in [("document", st.doc.id.0), ("revision", st.revision)] {
        if let Some(value) = p.get(key)
            && value.as_u64() != Some(expected)
        {
            return Err(error("node gesture is stale"));
        }
    }
    Ok(())
}

pub(crate) fn apply(s: &mut Session, p: &Value, label: &str, op: Operation) -> Result<Value> {
    guarded(s, p)?;
    let targets: Vec<Target> =
        serde_json::from_value(p.get("targets").cloned().ok_or_else(|| error("missing node targets"))?).map_err(|e| error(e.to_string()))?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let prepared = prepare(&st.doc, &targets, &op, p.get("convertLive").and_then(Value::as_bool).unwrap_or(false))?;
    let result = json!({"targets": prepared.targets});
    let shape_only = !matches!(op, Operation::Join { .. })
        && targets.iter().all(|t| t.layer.is_some_and(|id| prepared.doc.layer(LayerId(id)).is_some_and(|l| matches!(l.content, LayerContent::Shape(_)))));
    if shape_only {
        let edits = targets
            .iter()
            .map(|t| {
                let id = LayerId(t.layer.ok_or_else(|| error("missing prepared shape id"))?);
                let l = prepared.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
                let LayerContent::Shape(shape) = &l.content else { return Err(error("missing prepared shape")) };
                Ok(crate::vector_api::ShapeEdit { layer: id, shape: shape.clone(), vector: l.vector.clone() })
            })
            .collect::<Result<Vec<_>>>()?;
        crate::vector_api::apply_shape_edits(s, label, edits)?;
    } else {
        // The shared shape-only boundary cannot publish work/saved paths, masks or consumed
        // join donors. Keep these heterogeneous targets in one prepared document transaction.
        s.edit(label, move |doc, _| {
            *doc = prepared.doc;
            Ok(())
        })?;
    }
    Ok(result)
}
fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let op = serde_json::from_value(p.get("operation").cloned().ok_or_else(|| error("missing node operation"))?).map_err(|e| error(e.to_string()))?;
    apply(s, p, "Edit Nodes", op)
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "path.node.edit",
        label: "Edit Nodes",
        menu: &[],
        shortcut: None,
        params: "{targets:[{name,layer?,anchors:[[s,k]]}],operation:{type,...},document?:id,revision?:u64,convertLive?:bool}",
        enabled: crate::vector_cmds::has_doc,
        run,
        journal: true,
    }]
}
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_gpu;
