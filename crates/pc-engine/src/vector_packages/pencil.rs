//! Vector Pencil and Sculpt; raster paint.pencil is untouched.
//! Adapted from VectorCraft tools/src/draw2/{gesture.rs,../draw2.rs} and
//! engine/src/cmd/draw2.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! The package bridge compiles the same owned helper until 00 registers its pc-vector
//! module. Final re-export and V1 fitter hook: docs/wip/vector-tasks/integration/05.md.

use crate::{EngineError, Result, Session, commands::CommandSpec, vector_api};
use fitting::{FitOptions, Sample, Stabilization};
use photocraft_doc::{DocId, Document, Fill, Layer, LayerContent, LayerId, Path, ShapeLayer, ShapeStroke};
use photocraft_geom::Point;
pub use photocraft_vector::pencil_fit as fitting;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

pub const MAX_CACHE_PIXELS: u64 = 16_777_216;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub tolerance: f64,
    pub smoothness: f64,
    pub stabilizer: StabilizerKind,
    pub rope_length: f64,
    pub window_samples: usize,
    pub sculpt: bool,
    pub edit_within: f64,
    pub close_within: f64,
    pub fill: bool,
    pub stroke_width: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StabilizerKind {
    #[default]
    None,
    Rope,
    Window,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            tolerance: 1.5,
            smoothness: 0.5,
            stabilizer: StabilizerKind::None,
            rope_length: 12.0,
            window_samples: 8,
            sculpt: false,
            edit_within: 12.0,
            close_within: 8.0,
            fill: false,
            stroke_width: 2.0,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        FitOptions { tolerance: self.tolerance, smoothness: self.smoothness, closed: false }.validate().map_err(EngineError::Other)?;
        Stabilization::Rope { length: self.rope_length }.validate().map_err(EngineError::Other)?;
        Stabilization::Window { samples: self.window_samples }.validate().map_err(EngineError::Other)?;
        if [self.edit_within, self.close_within].iter().any(|v| !v.is_finite() || !(0.0..=1000.0).contains(v))
            || !self.stroke_width.is_finite()
            || !(0.1..=1000.0).contains(&self.stroke_width)
        {
            return Err(EngineError::Other("invalid pencil distances or width".into()));
        }
        Ok(())
    }
    pub fn stabilization(&self) -> Stabilization {
        match self.stabilizer {
            StabilizerKind::None => Stabilization::None,
            StabilizerKind::Rope => Stabilization::Rope { length: self.rope_length },
            StabilizerKind::Window => Stabilization::Window { samples: self.window_samples },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    Create { above: Option<LayerId> },
    Continue { layer: LayerId, subpath: usize, at_start: bool },
    Sculpt { layer: LayerId, subpath: usize, first: usize, last: usize },
    SculptRange { layer: LayerId, subpath: usize, from: CurvePosition, to: CurvePosition },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurvePosition {
    pub segment: usize,
    pub t: f64,
}
impl CurvePosition {
    pub fn tuple(self) -> (usize, f64) {
        (self.segment, self.t)
    }
}
impl Operation {
    pub fn target(&self) -> Option<LayerId> {
        match *self {
            Self::Create { .. } => None,
            Self::Continue { layer, .. } | Self::Sculpt { layer, .. } | Self::SculptRange { layer, .. } => Some(layer),
        }
    }
}
/// Identity and targets captured on press, also explicit automation parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub document: DocId,
    pub revision: u64,
    pub operation: Operation,
    pub new_layer: LayerId,
    pub options: Options,
    pub color: [f32; 4],
}
impl Capture {
    pub fn begin(session: &Session, operation: Operation, options: Options) -> Result<Self> {
        let st = session.active().ok_or(EngineError::NoDocument)?;
        let capture = Self { document: st.doc.id, revision: st.revision, operation, new_layer: LayerId::fresh(), options, color: session.tools.foreground };
        capture.validate(session)?;
        Ok(capture)
    }
    pub fn validate(&self, session: &Session) -> Result<()> {
        self.options.validate()?;
        let st = session.active().ok_or(EngineError::NoDocument)?;
        if st.doc.id != self.document || st.revision != self.revision {
            return Err(EngineError::Other("pencil gesture is stale".into()));
        }
        if self.color.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
            return Err(EngineError::Other("invalid pencil color".into()));
        }
        match self.operation {
            Operation::Create { above } => {
                if self.new_layer.0 == 0 || self.new_layer.0 > i64::MAX as u64 || st.doc.layer(self.new_layer).is_some() {
                    return Err(EngineError::Other("pencil layer id is already used".into()));
                }
                if let Some(id) = above {
                    let path = st.doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
                    // A locked pixel sibling allows vector creation; a locked parent does not.
                    for n in 1..path.len() {
                        if st.doc.layer_at(&path[..n]).is_some_and(|l| l.locks.all) {
                            return Err(EngineError::Other("pencil insertion parent is locked".into()));
                        }
                    }
                }
            }
            _ => {
                if let Some(id) = self.operation.target() {
                    let edits = vector_api::snapshot_shapes(&st.doc, &[id])?;
                    if edits[0].vector.appearance.is_some() {
                        return Err(EngineError::Other("pencil edits need the integrated appearance renderer".into()));
                    }
                    let path = &edits[0].shape.path;
                    fitting::validate_path(path).map_err(EngineError::Other)?;
                    match self.operation {
                        Operation::Continue { subpath, .. } if path.subpaths.get(subpath).is_none_or(|s| s.closed || s.knots.len() < 2) => {
                            return Err(EngineError::Other("pencil continuation requires an open path".into()));
                        }
                        Operation::Sculpt { subpath, first, last, .. }
                            if path
                                .subpaths
                                .get(subpath)
                                .is_none_or(|s| first == last || first >= s.knots.len() || last >= s.knots.len() || (!s.closed && first > last)) =>
                        {
                            return Err(EngineError::Other("invalid sculpt interval".into()));
                        }
                        Operation::SculptRange { subpath, from, to, .. } => {
                            let segments = path.subpaths.get(subpath).ok_or_else(|| EngineError::Other("sculpt subpath missing".into()))?.segments().len();
                            for p in [from, to] {
                                if p.segment >= segments || !p.t.is_finite() || !(0.0..=1.0).contains(&p.t) {
                                    return Err(EngineError::Other("invalid sculpt curve position".into()));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}
/// Detached preview, including pressure samples for WidthProfile's consumer.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub capture: Capture,
    pub document: Arc<Document>,
    pub pressure: Vec<Sample>,
    layer: Layer,
    path: Path,
}
impl Prepared {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn layer(&self) -> &Layer {
        &self.layer
    }
    pub fn params(&self) -> Value {
        json!({"capture": self.capture, "path": self.path})
    }
    pub fn commit(self, session: &mut Session) -> Result<Value> {
        self.capture.validate(session)?;
        session.execute("path.pencil.commit", self.params())
    }
}
pub fn prepare(session: &Session, capture: &Capture, samples: &[Sample], closed: bool) -> Result<Prepared> {
    prepare_with(session, capture, samples, closed, fitting::fit)
}
/// Inject V1's fit_path through fitting::fit_with, without a pc-vector -> pc-pathops cycle.
pub fn prepare_with(
    session: &Session,
    capture: &Capture,
    samples: &[Sample],
    closed: bool,
    fitter: impl Fn(&[Sample], FitOptions) -> fitting::FitResult<Path>,
) -> Result<Prepared> {
    capture.validate(session)?;
    if !(2..=fitting::MAX_SAMPLES).contains(&samples.len()) {
        return Err(EngineError::Other("pencil needs 2..8192 samples".into()));
    }
    for sample in samples {
        sample.validate().map_err(EngineError::Other)?;
    }
    let st = session.active().ok_or(EngineError::NoDocument)?;
    let mut samples = samples.to_vec();
    let mut close_extension = false;
    let edits = capture.operation.target().map(|id| vector_api::snapshot_shapes(&st.doc, &[id])).transpose()?;
    if let Some(edits) = &edits {
        match capture.operation {
            Operation::Continue { subpath, at_start, .. } => {
                let sp = edits[0]
                    .shape
                    .path
                    .subpaths
                    .get(subpath)
                    .filter(|s| !s.closed && s.knots.len() >= 2)
                    .ok_or_else(|| EngineError::Other("pencil continuation requires an open path".into()))?;
                let (start, opposite) = if at_start {
                    (sp.knots[0].anchor, sp.knots[sp.knots.len() - 1].anchor)
                } else {
                    (sp.knots[sp.knots.len() - 1].anchor, sp.knots[0].anchor)
                };
                samples[0].point = start;
                let n = samples.len() - 1;
                if fitting::distance(samples[n].point, opposite) <= capture.options.close_within {
                    samples[n].point = opposite;
                    close_extension = true;
                }
            }
            Operation::Sculpt { subpath, first, last, .. } => {
                let sp = edits[0].shape.path.subpaths.get(subpath).ok_or_else(|| EngineError::Other("sculpt subpath missing".into()))?;
                samples[0].point = sp.knots.get(first).ok_or_else(|| EngineError::Other("sculpt start missing".into()))?.anchor;
                let n = samples.len() - 1;
                samples[n].point = sp.knots.get(last).ok_or_else(|| EngineError::Other("sculpt end missing".into()))?.anchor;
            }
            Operation::SculptRange { subpath, from, to, .. } => {
                let (base, first, last) = fitting::sculpt_boundaries(&edits[0].shape.path, subpath, from.tuple(), to.tuple()).map_err(EngineError::Other)?;
                samples[0].point = base.subpaths[subpath].knots[first].anchor;
                let n = samples.len() - 1;
                samples[n].point = base.subpaths[subpath].knots[last].anchor;
            }
            _ => {}
        }
    }
    let options = FitOptions {
        tolerance: capture.options.tolerance,
        smoothness: capture.options.smoothness,
        closed: matches!(capture.operation, Operation::Create { .. }) && closed,
    };
    let fitted = fitter(&samples, options).map_err(EngineError::Other)?;
    let path = match capture.operation {
        Operation::Create { .. } => fitted,
        Operation::Continue { subpath, at_start, .. } => {
            let base = &edits.as_ref().ok_or_else(|| EngineError::Other("missing captured pencil shape".into()))?[0].shape.path;
            fitting::continue_path(base, subpath, at_start, &fitted, closed || close_extension).map_err(EngineError::Other)?
        }
        Operation::Sculpt { subpath, first, last, .. } => {
            let base = &edits.as_ref().ok_or_else(|| EngineError::Other("missing captured pencil shape".into()))?[0].shape.path;
            fitting::sculpt(base, subpath, first, last, &fitted).map_err(EngineError::Other)?
        }
        Operation::SculptRange { subpath, from, to, .. } => {
            let base = &edits.as_ref().ok_or_else(|| EngineError::Other("missing captured pencil shape".into()))?[0].shape.path;
            let (base, first, last) = fitting::sculpt_boundaries(base, subpath, from.tuple(), to.tuple()).map_err(EngineError::Other)?;
            fitting::sculpt(&base, subpath, first, last, &fitted).map_err(EngineError::Other)?
        }
    };
    let mut prepared = prepare_path(session, capture, path)?;
    prepared.pressure = samples;
    Ok(prepared)
}
fn prepare_path(session: &Session, capture: &Capture, path: Path) -> Result<Prepared> {
    capture.validate(session)?;
    fitting::validate_path(&path).map_err(EngineError::Other)?;
    if path.is_empty() || path.subpaths.iter().any(|s| s.knots.len() < 2) {
        return Err(EngineError::Other("pencil cannot commit empty geometry".into()));
    }
    let st = session.active().ok_or(EngineError::NoDocument)?;
    let mut layer = if let Some(id) = capture.operation.target() {
        st.doc.layer(id).cloned().ok_or(EngineError::NoLayer(id))?
    } else {
        let color = photocraft_doc::Color::rgba(capture.color[0], capture.color[1], capture.color[2], capture.color[3]);
        let shape = ShapeLayer {
            fill: capture.options.fill.then_some(Fill::Solid(color)),
            stroke: Some(ShapeStroke { width: capture.options.stroke_width, paint: Fill::Solid(color), ..Default::default() }),
            ..Default::default()
        };
        let mut layer = Layer::new("Pencil", LayerContent::Shape(shape));
        layer.id = capture.new_layer;
        layer
    };
    let LayerContent::Shape(shape) = &mut layer.content else {
        return Err(EngineError::Other("pencil target is not a shape".into()));
    };
    shape.path = path.clone();
    shape.live = None;
    shape.psd_raw = None;
    layer.vector.live_shape = None;
    layer.vector.corners.clear();
    if let Some((x0, y0, x1, y1)) = path.control_bounds() {
        let pad = shape.stroke.as_ref().map_or(1.0, |s| f64::from(s.width) * f64::from(s.miter_limit.max(1.0)) + 2.0);
        if !pad.is_finite() || pad > 100_000.0 {
            return Err(EngineError::Other("pencil stroke bounds are invalid".into()));
        }
        let bounds = photocraft_geom::Rect::new((x0 - pad).floor() as i32, (y0 - pad).floor() as i32, (x1 + pad).ceil() as i32, (y1 + pad).ceil() as i32)
            .intersect(&st.doc.bounds());
        if bounds.size().area() > MAX_CACHE_PIXELS {
            return Err(EngineError::Other("pencil render cache exceeds its pixel budget".into()));
        }
    }
    crate::vector_cmds::refresh_shape(&st.doc, shape);
    let mut document = (*st.doc).clone();
    match capture.operation {
        Operation::Create { above } => {
            document.insert_above(above, layer.clone());
        }
        _ => {
            let id = layer.id;
            *document.layer_mut(id).ok_or(EngineError::NoLayer(id))? = layer.clone();
        }
    }
    Ok(Prepared { capture: capture.clone(), layer, path, document: Arc::new(document), pressure: Vec::new() })
}
fn commit_command(session: &mut Session, params: &Value) -> Result<Value> {
    let capture: Capture =
        serde_json::from_value(params.get("capture").cloned().ok_or_else(|| crate::vector_cmds::bad("path.pencil.commit", "capture required"))?)
            .map_err(|e| crate::vector_cmds::bad("path.pencil.commit", e.to_string()))?;
    capture.validate(session)?;
    let raw_path = params.get("path").ok_or_else(|| crate::vector_cmds::bad("path.pencil.commit", "path required"))?;
    let subs = raw_path
        .get("subpaths")
        .and_then(Value::as_array)
        .filter(|s| s.len() <= fitting::MAX_SAMPLES)
        .ok_or_else(|| crate::vector_cmds::bad("path.pencil.commit", "bounded subpaths required"))?;
    let count = subs
        .iter()
        .try_fold(0usize, |n, s| s.get("knots").and_then(Value::as_array).and_then(|a| n.checked_add(a.len())))
        .filter(|&n| n <= fitting::MAX_SAMPLES)
        .ok_or_else(|| crate::vector_cmds::bad("path.pencil.commit", "pencil node budget exceeded"))?;
    if count < 2 {
        return Err(crate::vector_cmds::bad("path.pencil.commit", "pencil path is empty"));
    }
    let path: Path = serde_json::from_value(raw_path.clone()).map_err(|e| crate::vector_cmds::bad("path.pencil.commit", e.to_string()))?;
    validate_operation_path(session, &capture, &path)?;
    let prepared = prepare_path(session, &capture, path)?;
    let id = prepared.layer.id;
    let label = if matches!(capture.operation, Operation::Sculpt { .. } | Operation::SculptRange { .. }) { "Sculpt Path" } else { "Vector Pencil" };
    match capture.operation {
        Operation::Create { above } => {
            photocraft_doc::ensure_ids_above(id.0);
            crate::vector_integration::insert_layer(session, above, prepared.layer, label)?;
        }
        _ => {
            let LayerContent::Shape(shape) = prepared.layer.content else {
                return Err(EngineError::Other("pencil cache missing".into()));
            };
            vector_api::apply_shape_edits(session, label, vec![vector_api::ShapeEdit { layer: id, shape, vector: prepared.layer.vector }])?;
        }
    }
    Ok(json!({"layer":id.0}))
}

/// Commit accepts detached geometry, but cannot use Sculpt/Continue as a backdoor to change
/// another component or the anchors outside the captured interval.
fn validate_operation_path(session: &Session, capture: &Capture, path: &Path) -> Result<()> {
    let Some(id) = capture.operation.target() else {
        return Ok(());
    };
    let edits = vector_api::snapshot_shapes(&session.active().ok_or(EngineError::NoDocument)?.doc, &[id])?;
    let mut resolved = capture.clone();
    let expanded;
    let base = if let Operation::SculptRange { layer, subpath, from, to } = capture.operation {
        let (base, first, last) = fitting::sculpt_boundaries(&edits[0].shape.path, subpath, from.tuple(), to.tuple()).map_err(EngineError::Other)?;
        expanded = base;
        resolved.operation = Operation::Sculpt { layer, subpath, first, last };
        &expanded
    } else {
        &edits[0].shape.path
    };
    let capture = &resolved;
    let si = match capture.operation {
        Operation::Continue { subpath, .. } | Operation::Sculpt { subpath, .. } => subpath,
        _ => return Ok(()),
    };
    let fail = || EngineError::Other("pencil geometry changed outside the captured interval".into());
    if base.fill_rule != path.fill_rule
        || base.inverted != path.inverted
        || base.subpaths.len() != path.subpaths.len()
        || base.subpaths.iter().zip(&path.subpaths).enumerate().any(|(i, (a, b))| i != si && a != b)
    {
        return Err(fail());
    }
    let old = &base.subpaths[si];
    let new = &path.subpaths[si];
    if old.op != new.op {
        return Err(fail());
    }
    match capture.operation {
        Operation::Sculpt { first, last, .. } => {
            if first == last || old.closed != new.closed {
                return Err(fail());
            }
            let outside = if first < last { old.knots.len() - (last - first + 1) } else { first - last - 1 };
            if new.knots.len() < outside + 2 {
                return Err(fail());
            }
            let run = if first < last {
                let suffix = old.knots.len() - last - 1;
                new.knots.get(first..new.knots.len() - suffix).ok_or_else(fail)?
            } else {
                &new.knots[..new.knots.len() - outside]
            };
            let replacement = Path::new(vec![photocraft_doc::Subpath { knots: run.to_vec(), ..Default::default() }]);
            let rebuilt = fitting::sculpt(base, si, first, last, &replacement).map_err(EngineError::Other)?;
            if &rebuilt != path {
                return Err(fail());
            }
        }
        Operation::Continue { at_start, .. } => {
            if new.knots.len() < old.knots.len() {
                return Err(fail());
            }
            let retained = new.knots.iter().enumerate().any(|(start, n)| {
                n.anchor == old.knots[0].anchor
                    && (new.closed || if at_start { start + old.knots.len() == new.knots.len() } else { start == 0 })
                    && old.knots.iter().enumerate().all(|(i, k)| {
                        let Some(n) = new.knots.get(if new.closed { (start + i) % new.knots.len() } else { start + i }) else {
                            return false;
                        };
                        let join = if at_start { i == 0 } else { i + 1 == old.knots.len() };
                        n.anchor == k.anchor
                            && if join {
                                if at_start { n.out_ctrl == k.out_ctrl } else { n.in_ctrl == k.in_ctrl }
                            } else if new.closed && i == 0 {
                                n.out_ctrl == k.out_ctrl
                            } else if new.closed && i + 1 == old.knots.len() {
                                n.in_ctrl == k.in_ctrl
                            } else {
                                n == k
                            }
                    })
            });
            if !retained {
                return Err(fail());
            }
        }
        _ => {}
    }
    Ok(())
}

fn preview_command(session: &mut Session, params: &Value) -> Result<Value> {
    let capture: Capture =
        serde_json::from_value(params.get("capture").cloned().ok_or_else(|| crate::vector_cmds::bad("path.pencil.preview", "capture required"))?)
            .map_err(|e| crate::vector_cmds::bad("path.pencil.preview", e.to_string()))?;
    capture.validate(session)?;
    let raw = params
        .get("samples")
        .and_then(Value::as_array)
        .filter(|a| a.len() <= fitting::MAX_SAMPLES)
        .ok_or_else(|| crate::vector_cmds::bad("path.pencil.preview", "bounded samples array required"))?;
    let samples: Vec<_> = raw
        .iter()
        .map(|v| {
            let a = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| crate::vector_cmds::bad("path.pencil.preview", "samples are [x,y,pressure]"))?;
            let number = |i: usize| a[i].as_f64().ok_or_else(|| crate::vector_cmds::bad("path.pencil.preview", "sample must be numeric"));
            Ok(Sample { point: Point::new(number(0)?, number(1)?), pressure: number(2)? })
        })
        .collect::<Result<_>>()?;
    let preview = prepare(session, &capture, &samples, params.get("closed").and_then(Value::as_bool).unwrap_or(false))?;
    Ok(preview.params())
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "path.pencil.preview",
            label: "Preview Vector Pencil",
            menu: &[],
            shortcut: None,
            params: "{capture:{document,revision,operation,new_layer,options,color},samples:[[x,y,pressure],…],closed?:bool}",
            enabled: crate::vector_cmds::has_doc,
            run: preview_command,
            journal: false,
        },
        CommandSpec {
            id: "path.pencil.commit",
            label: "Vector Pencil",
            menu: &[],
            shortcut: None,
            params: "{capture:{document,revision,operation,new_layer,options,color},path:serde Path}",
            enabled: crate::vector_cmds::has_doc,
            run: commit_command,
            journal: true,
        },
    ]
}
#[cfg(test)]
#[path = "pencil/gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "pencil/tests.rs"]
mod tests;
