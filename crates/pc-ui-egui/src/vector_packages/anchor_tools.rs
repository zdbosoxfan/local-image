//! Pen flyout input, adapted from VectorCraft tools/src/draw2/{anchor,curvature}.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Notices: licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Router/tool/icon wiring belongs to 00/01/46; see docs/wip/vector-tasks/integration/33.md.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};
use photocraft_doc::{Document, Fill, LayerContent, ShapeStroke};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::{anchor_tools as engine, node, snapping::query as snap},
};
use photocraft_geom::Point;
use photocraft_vector::{
    curvature_fit,
    edit::{Handle, Hit},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    #[default]
    AddAnchor,
    DeleteAnchor,
    AnchorPoint,
    Curvature,
}
impl Tool {
    pub fn id(self) -> &'static str {
        match self {
            Self::AddAnchor => "addAnchor",
            Self::DeleteAnchor => "deleteAnchor",
            Self::AnchorPoint => "anchorPoint",
            Self::Curvature => "curvature",
        }
    }
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub tool: Tool,
    #[serde(default = "yes")]
    pub rubber_band: bool,
    pub snapping: snap::Settings,
}
impl Default for Options {
    fn default() -> Self {
        Self { tool: Tool::AddAnchor, rubber_band: true, snapping: snap::Settings::default() }
    }
}
/// Only handles displayed by 03 may be hit. Target indices refer to the captured candidate list.
#[derive(Clone, Copy, Debug)]
pub struct ShownHandle {
    pub target: usize,
    pub knot: [usize; 2],
    pub handle: Handle,
}
/// Copy into 03's conversion modal before opening it. Confirmation replays this immutable
/// press after checking both document and revision; current selection is never substituted.
#[derive(Clone)]
pub struct PendingConversion {
    pub document: u64,
    pub revision: u64,
    pub tool: Tool,
    pub targets: Vec<node::Target>,
    pub shown: Vec<ShownHandle>,
    pub point: Point,
    pub tolerance: f64,
    pub modifiers: Modifiers,
    pub view: snap::View,
    pub fill: Option<Fill>,
    pub stroke: Option<ShapeStroke>,
}
#[derive(Clone, Debug)]
struct AnchorDrag {
    gesture: engine::AnchorGesture,
    start: Point,
    kind: Hit,
    original_handle: Option<Point>,
    moved: bool,
    tool: Tool,
}
#[derive(Clone, Debug)]
struct PointDrag {
    index: usize,
    start: Point,
    close: bool,
    moved: bool,
    toggled: bool,
}
#[derive(Clone, Debug)]
struct Curvature {
    request: engine::CurvatureCommit,
    current: Option<usize>,
    drag: Option<PointDrag>,
    /// Editing a pre-existing curve finishes on release; drawing stays detached until Esc/Enter.
    editing: bool,
    changed: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub options: Options,
    #[serde(skip)]
    anchor: Option<AnchorDrag>,
    #[serde(skip)]
    curvature: Option<Curvature>,
    #[serde(skip)]
    pub preview: Option<Arc<Document>>,
    #[serde(skip)]
    pub generation: u64,
    #[serde(skip)]
    pub hover: Option<Point>,
    #[serde(skip)]
    pub guides: Vec<snap::Guide>,
    #[serde(skip)]
    snap_cache: Option<snap::Cache>,
    /// 00 routes this to 03's conversion modal. No target is converted before confirmation.
    #[serde(skip)]
    pub needs_conversion: bool,
    #[serde(skip)]
    pub convert_live: bool,
    #[serde(skip)]
    pub last_targets: Vec<node::Target>,
    #[serde(skip)]
    pub pending_conversion: Option<PendingConversion>,
}
fn error(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}
fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
impl State {
    pub fn captured(&self) -> bool {
        self.anchor.is_some() || self.curvature.as_ref().is_some_and(|c| c.drag.is_some())
    }
    pub fn active(&self) -> bool {
        self.anchor.is_some() || self.curvature.is_some()
    }
    pub fn cancel(&mut self) {
        self.anchor = None;
        self.curvature = None;
        self.preview = None;
        self.hover = None;
        self.guides.clear();
        self.snap_cache = None;
        self.needs_conversion = false;
        self.convert_live = false;
        self.last_targets.clear();
        self.pending_conversion = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) {
        let stale = self.anchor.as_ref().is_some_and(|a| a.tool != self.options.tool || a.gesture.capture.validate(s).is_err())
            || self.curvature.as_ref().is_some_and(|c| {
                self.options.tool != Tool::Curvature || photocraft_engine::vector_packages::pen::check_fresh(s, c.request.document, c.request.revision).is_err()
            });
        let stale =
            stale || self.pending_conversion.as_ref().is_some_and(|p| photocraft_engine::vector_packages::pen::check_fresh(s, p.document, p.revision).is_err());
        if !enabled || stale {
            self.cancel();
        }
    }
    pub fn display_doc(&self, s: &Session) -> Option<(Arc<Document>, u64)> {
        if let Some(a) = &self.anchor {
            a.gesture.capture.validate(s).ok()?;
        }
        if let Some(c) = &self.curvature {
            photocraft_engine::vector_packages::pen::check_fresh(s, c.request.document, c.request.revision).ok()?;
        }
        Some((self.preview.clone()?, self.generation | (1 << 61)))
    }
    fn update_preview(&mut self, s: &Session) -> Result<()> {
        self.preview = if let Some(a) = &self.anchor {
            Some(Arc::new(a.gesture.preview(s)?))
        } else if let Some(c) = &self.curvature {
            if c.changed { Some(Arc::new(engine::preview_curvature(s, &c.request)?)) } else { None }
        } else {
            None
        };
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    fn snapped(&mut self, s: &Session, point: Point, view: snap::View, exclude: snap::Exclusions, origin: Option<Point>, shift: bool) -> Result<Point> {
        engine::valid_point(point)?;
        let point = if shift {
            if let Some(origin) = origin {
                let x = point.x - origin.x;
                let y = point.y - origin.y;
                let step = std::f64::consts::FRAC_PI_4;
                let theta = (y.atan2(x) / step).round() * step;
                let len = x.hypot(y);
                Point::new(origin.x + len * theta.cos(), origin.y + len * theta.sin())
            } else {
                point
            }
        } else {
            point
        };
        self.options.snapping.validate()?;
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if self.snap_cache.as_ref().is_none_or(|c| !c.is_current(st.doc.id, st.revision) || c.exclusions() != &exclude) {
            self.snap_cache = Some(snap::Cache::build(&st.doc, st.revision, exclude, snap::installed_geometry())?);
        }
        let angle = if shift { origin.map(|p| snap::Angle { origin: [p.x, p.y], step: std::f64::consts::FRAC_PI_4, offset: 0. }) } else { None };
        let result =
            self.snap_cache.as_ref().ok_or_else(|| error("missing snap capture"))?.point([point.x, point.y], &self.options.snapping, view, angle, false)?;
        self.guides = result.guides;
        Ok(Point::new(result.point[0], result.point[1]))
    }
    fn attempt(&mut self, r: Result<()>) -> Result<()> {
        if let Err(e) = r {
            let conversion = e.to_string().contains("Convert to curves");
            self.cancel();
            self.needs_conversion = conversion;
            Err(e)
        } else {
            Ok(())
        }
    }
    pub fn down(
        &mut self,
        s: &Session,
        targets: Vec<node::Target>,
        shown: &[ShownHandle],
        point: Point,
        tolerance: f64,
        mods: Modifiers,
        view: snap::View,
        fill: Option<Fill>,
        stroke: Option<ShapeStroke>,
    ) -> Result<()> {
        self.synchronize(s, true);
        let pending = s.active().map(|st| PendingConversion {
            document: st.doc.id.0,
            revision: st.revision,
            tool: self.options.tool,
            targets: targets.clone(),
            shown: shown.to_vec(),
            point,
            tolerance,
            modifiers: mods,
            view,
            fill: fill.clone(),
            stroke: stroke.clone(),
        });
        let r = self.down_inner(s, targets, shown, point, tolerance, mods, view, fill, stroke);
        let conversion = r.as_ref().err().is_some_and(|e| e.to_string().contains("Convert to curves"));
        let result = self.attempt(r);
        if conversion {
            self.pending_conversion = pending;
        }
        result
    }
    /// Called only after 03's modal confirms conversion. Restore its saved pending record
    /// first if modal frame deactivation has cancelled this state.
    pub fn confirm_conversion(&mut self, s: &Session) -> Result<()> {
        let pending = self.pending_conversion.take().ok_or_else(|| error("missing conversion press"))?;
        if let Err(e) = photocraft_engine::vector_packages::pen::check_fresh(s, pending.document, pending.revision) {
            self.cancel();
            return Err(e);
        }
        self.options.tool = pending.tool;
        self.needs_conversion = false;
        self.convert_live = true;
        self.down(s, pending.targets, &pending.shown, pending.point, pending.tolerance, pending.modifiers, pending.view, pending.fill, pending.stroke)
    }
    fn down_inner(
        &mut self,
        s: &Session,
        targets: Vec<node::Target>,
        shown: &[ShownHandle],
        point: Point,
        tolerance: f64,
        mods: Modifiers,
        view: snap::View,
        fill: Option<Fill>,
        stroke: Option<ShapeStroke>,
    ) -> Result<()> {
        engine::valid_point(point)?;
        view.tolerance(5.)?;
        if !tolerance.is_finite() || !(0.0..=1e6).contains(&tolerance) {
            return Err(error("invalid hit tolerance"));
        }
        if self.options.tool == Tool::Curvature {
            return self.curvature_down(s, targets, point, tolerance, mods, view, fill, stroke);
        }
        if targets.is_empty() {
            return Ok(());
        }
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        let tool = if mods.alt {
            match self.options.tool {
                Tool::AddAnchor => Tool::DeleteAnchor,
                Tool::DeleteAnchor => Tool::AddAnchor,
                t => t,
            }
        } else {
            self.options.tool
        };
        let picked = match tool {
            Tool::AddAnchor => engine::segment_under(doc, &targets, point, tolerance)?.map(|h| (h.target, Hit::Segment(h.knot, h.t))),
            Tool::DeleteAnchor => engine::node_under(doc, &targets, point, tolerance)?.map(|h| (h.target, Hit::Anchor(h.knot))),
            _ => {
                let mut found = None;
                // Handle precedence across the complete selected set, then nodes, then segments.
                for (i, t) in targets.iter().enumerate() {
                    let handles: Vec<_> = shown.iter().filter(|h| h.target == i).map(|h| (h.knot, h.handle)).collect();
                    if let Some(hit @ Hit::Handle(..)) = node::hit(node::path(doc, t)?, point, tolerance, &handles) {
                        found = Some((i, hit));
                        break;
                    }
                }
                if found.is_none() {
                    found = engine::node_under(doc, &targets, point, tolerance)?.map(|h| (h.target, Hit::Anchor(h.knot)));
                }
                if found.is_none() {
                    found = engine::segment_under(doc, &targets, point, tolerance)?.map(|h| (h.target, Hit::Segment(h.knot, h.t)));
                }
                found
            }
        };
        let Some((target, kind)) = picked else {
            return Ok(());
        };
        let mut capture = engine::Capture::begin(s, targets, self.convert_live)?;
        let operation = match (tool, kind) {
            (Tool::AddAnchor, Hit::Segment(knot, t)) => node::Operation::Insert { knot, t },
            (Tool::DeleteAnchor, Hit::Anchor(knot)) => {
                capture.targets[target].anchors = vec![knot];
                node::Operation::Delete
            }
            (_, Hit::Anchor(knot)) => node::Operation::Convert { knot, out: None },
            (_, Hit::Handle(knot, h)) => node::Operation::Handle { knot, handle: h.name().into(), to: [point.x, point.y], independent: true },
            (_, Hit::Segment(knot, t)) => node::Operation::Bend { knot, t, delta: [0., 0.] },
        };
        let original_handle = if let Hit::Handle([si, ki], h) = kind {
            let k = node::path(doc, &capture.targets[target])?.subpaths[si].knots[ki];
            Some(if h == Handle::In { k.in_ctrl } else { k.out_ctrl })
        } else {
            None
        };
        self.anchor = Some(AnchorDrag {
            gesture: engine::AnchorGesture { capture, target, operation },
            start: point,
            kind,
            original_handle,
            moved: false,
            tool: self.options.tool,
        });
        if tool != Tool::AnchorPoint {
            self.update_preview(s)?;
        }
        Ok(())
    }
    fn curvature_down(
        &mut self,
        s: &Session,
        targets: Vec<node::Target>,
        point: Point,
        tolerance: f64,
        mods: Modifiers,
        view: snap::View,
        fill: Option<Fill>,
        stroke: Option<ShapeStroke>,
    ) -> Result<()> {
        // A retained point selection is not a retained gesture. Recapture the current
        // complete target set at the next press, or leave the former curve if deselected.
        let previous = self.curvature.as_ref().filter(|c| c.editing && !c.changed).and_then(|c| match &c.request.destination {
            engine::Destination::Curve { target, .. } => Some((target.name.clone(), target.layer)),
            _ => None,
        });
        if let Some(c) = self.curvature.as_ref().filter(|c| c.editing && !c.changed)
            && let engine::Destination::Curve { target, subpath } = &c.request.destination
        {
            let st = s.active().ok_or(EngineError::NoDocument)?;
            if let Some(hit) = engine::node_under(&st.doc, &targets, point, tolerance)?
                && (targets[hit.target] != *target || hit.knot[0] != *subpath)
            {
                self.cancel();
            }
        }
        if self.curvature.is_some()
            && let Some((name, layer)) = previous
        {
            if let Some(target) = targets.iter().find(|t| t.name == name && t.layer == layer).cloned() {
                let capture = engine::Capture::begin(s, targets.clone(), self.convert_live)?;
                let c = self.curvature.as_mut().ok_or_else(|| error("missing Curvature selection"))?;
                c.request.document = capture.document;
                c.request.revision = capture.revision;
                c.request.targets = capture.targets;
                c.request.convert_live = capture.convert_live;
                if let engine::Destination::Curve { target: captured, .. } = &mut c.request.destination {
                    *captured = target;
                }
            } else {
                self.cancel();
            }
        }
        if self.curvature.is_none() {
            let st = s.active().ok_or(EngineError::NoDocument)?;
            let hit = if targets.is_empty() { None } else { engine::node_under(&st.doc, &targets, point, tolerance)? };
            let (destination, points, closed, editing) = if let Some(h) = hit {
                engine::Capture::begin(s, targets.clone(), self.convert_live)?;
                let sp = &node::path(&st.doc, &targets[h.target])?.subpaths[h.knot[0]];
                (
                    engine::Destination::Curve { target: targets[h.target].clone(), subpath: h.knot[0] },
                    sp.knots
                        .iter()
                        .enumerate()
                        .map(|(i, k)| engine::CurvaturePoint { at: [k.anchor.x, k.anchor.y], corner: !k.smooth, origin: Some(i) })
                        .collect(),
                    sp.closed,
                    true,
                )
            } else {
                (engine::Destination::NewShape { above: st.active_layer }, Vec::new(), false, false)
            };
            self.curvature = Some(Curvature {
                request: engine::CurvatureCommit {
                    document: st.doc.id.0,
                    revision: st.revision,
                    destination,
                    targets: if editing { targets } else { Vec::new() },
                    points,
                    closed,
                    convert_live: self.convert_live,
                    fill,
                    stroke,
                },
                current: None,
                drag: None,
                editing,
                changed: false,
            });
        }
        let c = self.curvature.as_mut().ok_or_else(|| error("missing Curvature capture"))?;
        let hit = c.request.points.iter().position(|p| distance(Point::new(p.at[0], p.at[1]), point) <= tolerance);
        if let Some(index) = hit {
            if mods.alt {
                c.request.points[index].corner = !c.request.points[index].corner;
                c.changed = true;
            }
            let close = index == 0 && !mods.alt && !c.editing && !c.request.closed && c.request.points.len() >= 3;
            c.current = Some(index);
            c.drag = Some(PointDrag { index, start: point, close, moved: false, toggled: mods.alt });
        } else if !c.request.closed {
            let exclude = exclusions(&c.request.destination);
            let origin = c.request.points.last().map(|p| Point::new(p.at[0], p.at[1]));
            let at = self.snapped(s, point, view, exclude, origin, mods.shift)?;
            let c = self.curvature.as_mut().ok_or_else(|| error("missing Curvature capture"))?;
            if c.request.points.len() >= curvature_fit::MAX_POINTS {
                return Err(error("Curvature exceeds the point budget"));
            }
            let index = c.request.points.len();
            c.request.points.push(engine::CurvaturePoint { at: [at.x, at.y], corner: mods.alt, origin: None });
            c.changed = true;
            c.current = Some(index);
            c.drag = Some(PointDrag { index, start: point, close: false, moved: false, toggled: mods.alt });
        }
        self.hover = None;
        self.update_preview(s)
    }
    pub fn motion(&mut self, s: &Session, point: Point, mods: Modifiers, view: snap::View) -> Result<()> {
        self.synchronize(s, true);
        let r = self.motion_inner(s, point, mods, view);
        self.attempt(r)
    }
    fn motion_inner(&mut self, s: &Session, point: Point, mods: Modifiers, view: snap::View) -> Result<()> {
        engine::valid_point(point)?;
        let threshold = view.tolerance(2.)?;
        self.hover = Some(point);
        if let Some(a) = &self.anchor {
            if a.tool != Tool::AnchorPoint || distance(a.start, point) < threshold && !a.moved {
                return Ok(());
            }
            let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
            let target = &a.gesture.capture.targets[a.gesture.target];
            let (knot, origin) = match a.kind {
                Hit::Anchor(knot) | Hit::Handle(knot, _) => {
                    let k = node::path(doc, target)?.subpaths[knot[0]].knots[knot[1]];
                    (knot, Some(k.anchor))
                }
                Hit::Segment(knot, _) => (knot, None),
            };
            let exclude = snap::Exclusions {
                layers: target.layer.into_iter().map(photocraft_doc::LayerId).collect(),
                paths: if target.layer.is_none() { vec![target.name.clone()] } else { Vec::new() },
                work_path: target.layer.is_none(),
            };
            let kind = a.kind;
            let start = a.start;
            let handle = a.original_handle;
            let destination = if let Some(h) = handle { Point::new(h.x + point.x - start.x, h.y + point.y - start.y) } else { point };
            let snapped = self.snapped(s, destination, view, exclude, origin, mods.shift)?;
            let a = self.anchor.as_mut().ok_or_else(|| error("missing anchor drag"))?;
            a.moved = true;
            a.gesture.operation = match kind {
                Hit::Anchor(_) => node::Operation::Convert { knot, out: Some([snapped.x, snapped.y]) },
                Hit::Handle(_, h) => node::Operation::Handle { knot, handle: h.name().into(), to: [snapped.x, snapped.y], independent: true },
                Hit::Segment(_, t) => node::Operation::Bend { knot, t, delta: [point.x - start.x, point.y - start.y] },
            };
            self.update_preview(s)?;
        } else if let Some(c) = &self.curvature {
            if let Some(d) = &c.drag {
                if distance(d.start, point) < threshold && !d.moved {
                    return Ok(());
                }
                let exclude = exclusions(&c.request.destination);
                let index = d.index;
                let at = self.snapped(s, point, view, exclude, None, false)?;
                let c = self.curvature.as_mut().ok_or_else(|| error("missing Curvature drag"))?;
                c.request.points[index].at = [at.x, at.y];
                c.changed = true;
                if let Some(d) = &mut c.drag {
                    d.moved = true;
                    d.close = false;
                }
                self.hover = None;
                self.update_preview(s)?;
            } else {
                let exclude = exclusions(&c.request.destination);
                let origin = c.request.points.last().map(|p| Point::new(p.at[0], p.at[1]));
                self.hover = Some(self.snapped(s, point, view, exclude, origin, mods.shift)?);
            }
        }
        Ok(())
    }
    pub fn up(&mut self, s: &mut Session, double_clicked: bool) -> Result<()> {
        self.synchronize(s, true);
        let r = self.up_inner(s, double_clicked);
        self.attempt(r)
    }
    fn up_inner(&mut self, s: &mut Session, double_clicked: bool) -> Result<()> {
        if let Some(a) = self.anchor.take() {
            let click_no_op = a.tool == Tool::AnchorPoint
                && !a.moved
                && match a.kind {
                    Hit::Anchor([si, ki]) => {
                        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
                        let k = node::path(doc, &a.gesture.capture.targets[a.gesture.target])?.subpaths[si].knots[ki];
                        !k.smooth && k.in_ctrl == k.anchor && k.out_ctrl == k.anchor
                    }
                    _ => true,
                };
            let targets = if !click_no_op { a.gesture.commit(s)? } else { Vec::new() };
            self.cancel();
            self.last_targets = targets;
            return Ok(());
        }
        if let Some(c) = &mut self.curvature
            && let Some(d) = c.drag.take()
        {
            if double_clicked && !d.moved && !d.toggled {
                c.request.points[d.index].corner = !c.request.points[d.index].corner;
                c.changed = true;
            }
            if d.close && !double_clicked {
                c.request.closed = true;
                c.changed = true;
            }
            let finish = c.editing || c.request.closed;
            // A click on an existing point selects it; do not journal an unchanged refit.
            if finish && c.changed {
                self.finish(s)?;
            } else if finish {
                self.preview = None;
            } else {
                self.update_preview(s)?;
            }
        }
        self.guides.clear();
        Ok(())
    }
    pub fn finish(&mut self, s: &mut Session) -> Result<()> {
        let Some(mut c) = self.curvature.take() else {
            return Ok(());
        };
        self.cancel();
        if c.changed && (c.editing || c.request.points.len() >= 2) {
            self.last_targets = engine::commit_curvature(s, c.request.clone())?;
            // Keep the selected point after a finished edit so Delete or another drag is
            // a fresh gesture. Drawing still ends on Esc/Enter/close. Origins now name
            // the committed knots, never the indices from the preceding revision.
            if c.editing
                && let Some(index) = c.current
                && let engine::Destination::Curve { target, subpath } = &mut c.request.destination
            {
                let st = s.active().ok_or(EngineError::NoDocument)?;
                let updated = self
                    .last_targets
                    .iter_mut()
                    .find(|t| t.name == target.name && t.layer == target.layer)
                    .ok_or_else(|| error("missing committed Curvature selection"))?;
                updated.anchors = vec![[*subpath, index]];
                *target = updated.clone();
                c.request.document = st.doc.id.0;
                c.request.revision = st.revision;
                c.request.targets = self.last_targets.clone();
                for (i, point) in c.request.points.iter_mut().enumerate() {
                    point.origin = Some(i);
                }
                c.changed = false;
                self.curvature = Some(c);
            }
        }
        Ok(())
    }
    pub fn delete_point(&mut self, s: &mut Session) -> Result<()> {
        let Some(c) = &mut self.curvature else {
            return Ok(());
        };
        if c.drag.is_some() {
            return Ok(());
        }
        if let Some(i) = c.current.take() {
            c.request.points.remove(i);
            c.changed = true;
        }
        if c.request.points.len() < 3 {
            c.request.closed = false;
        }
        if c.request.points.is_empty() && !c.editing {
            self.cancel();
            return Ok(());
        }
        let editing = c.editing;
        let r = if editing { self.finish(s) } else { self.update_preview(s) };
        self.attempt(r)
    }
    pub fn curvature_path(&self) -> Option<photocraft_doc::Path> {
        self.curvature.as_ref()?.request.path().ok()
    }
    pub fn rubber_band(&self) -> Option<photocraft_doc::Path> {
        let c = self.curvature.as_ref()?;
        if !self.options.rubber_band || c.request.closed || c.drag.is_some() {
            return None;
        }
        let p = self.hover?;
        let mut points = c.request.points.clone();
        points.push(engine::CurvaturePoint { at: [p.x, p.y], corner: false, origin: None });
        curvature_fit::fit(&points.iter().map(|p| (Point::new(p.at[0], p.at[1]), p.corner)).collect::<Vec<_>>(), false).ok()
    }
}
fn exclusions(d: &engine::Destination) -> snap::Exclusions {
    match d {
        engine::Destination::NewShape { .. } => Default::default(),
        engine::Destination::Curve { target, .. } => snap::Exclusions {
            layers: target.layer.into_iter().map(photocraft_doc::LayerId).collect(),
            paths: if target.layer.is_none() { vec![target.name.clone()] } else { Vec::new() },
            work_path: target.layer.is_none(),
        },
    }
}

/// Uses 03's selection model plus selected shape layers, front first. Vector-mask routing
/// passes an explicit PathRef::Layer; saved paths can be supplied directly to input.
pub fn selected_targets(s: &Session, selection: &crate::direct_select::DirectSelection, vector_mask: bool) -> Vec<node::Target> {
    use crate::direct_select::PathRef;
    let Some(st) = s.active() else {
        return Vec::new();
    };
    let mut refs = if vector_mask {
        selection.target.into_iter().collect::<Vec<_>>()
    } else {
        st.selected_layers()
            .into_iter()
            .rev()
            .filter(|id| st.doc.layer(*id).is_some_and(|l| matches!(l.content, LayerContent::Shape(_))))
            .map(|id| PathRef::Layer(id.0))
            .collect()
    };
    if selection.target == Some(PathRef::Work) && st.doc.work_path.is_some() && !refs.contains(&PathRef::Work) {
        refs.push(PathRef::Work);
    }
    if refs.is_empty() && st.doc.work_path.is_some() {
        refs.push(PathRef::Work);
    }
    refs.into_iter()
        .map(|r| node::Target {
            name: if r == PathRef::Work { "work" } else { "layer" }.into(),
            layer: if let PathRef::Layer(id) = r { Some(id) } else { None },
            anchors: if selection.target == Some(r) {
                selection.anchors.clone()
            } else {
                selection.additional.iter().find(|a| a.target == r).map_or_else(Vec::new, |a| a.anchors.clone())
            },
        })
        .collect()
}
pub fn context(ui: &mut egui::Ui, state: &mut State) {
    let old = state.options.tool;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.options.tool, Tool::AddAnchor, tl!("Add Anchor Point"));
        ui.selectable_value(&mut state.options.tool, Tool::DeleteAnchor, tl!("Delete Anchor Point"));
        ui.selectable_value(&mut state.options.tool, Tool::AnchorPoint, tl!("Anchor Point"));
        ui.selectable_value(&mut state.options.tool, Tool::Curvature, tl!("Curvature"));
        if state.options.tool == Tool::Curvature {
            ui.checkbox(&mut state.options.rubber_band, tl!("Rubber band"));
        }
    });
    if old != state.options.tool {
        state.cancel();
    }
}
/// Raw egui events preserve captured release outside the canvas. 00 may instead call
/// down/motion/up after its ViewXform. Document/tool/persona/focus changes cancel the capture.
pub fn input(
    state: &mut State,
    s: &mut Session,
    ctx: &egui::Context,
    canvas: Rect,
    to_doc: impl Fn(Pos2) -> Point,
    targets: Vec<node::Target>,
    shown: &[ShownHandle],
    enabled: bool,
    tolerance: f64,
    view: snap::View,
    fill: Option<Fill>,
    stroke: Option<ShapeStroke>,
) -> Result<()> {
    state.synchronize(s, enabled);
    if !enabled {
        return Ok(());
    }
    let (events, mut mods, focused, double) =
        ctx.input(|i| (i.events.clone(), i.modifiers, i.focused, i.pointer.button_double_clicked(PointerButton::Primary)));
    if !focused || events.iter().any(|e| matches!(e, Event::WindowFocused(false))) {
        state.cancel();
        return Ok(());
    }
    for event in events {
        match event {
            Event::Key { key, pressed: true, modifiers, .. } if shortcut(key, modifiers, ctx.egui_wants_keyboard_input()).is_some() => {
                if let Some(tool) = shortcut(key, modifiers, false) {
                    state.cancel();
                    state.options.tool = tool;
                }
            }
            Event::ModifiersChanged(m) => mods = m,
            Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers } => {
                mods = modifiers;
                if pressed && canvas.contains(pos) {
                    state.down(s, targets.clone(), shown, to_doc(pos), tolerance, mods, view, fill.clone(), stroke.clone())?;
                } else if !pressed && state.captured() {
                    state.motion(s, to_doc(pos), mods, view)?;
                    state.up(s, double)?;
                }
            }
            Event::PointerMoved(pos) if canvas.contains(pos) || state.captured() => state.motion(s, to_doc(pos), mods, view)?,
            Event::PointerGone => state.cancel(),
            Event::Key { key: Key::Escape, pressed: true, .. } => {
                if state.captured() {
                    state.cancel();
                } else if state.curvature.is_some() {
                    state.finish(s)?;
                } else {
                    state.cancel();
                }
            }
            Event::Key { key: Key::Enter, pressed: true, .. } if !ctx.egui_wants_keyboard_input() => {
                if state.captured() {
                    state.cancel();
                } else {
                    state.finish(s)?;
                }
            }
            Event::Key { key: Key::Delete | Key::Backspace, pressed: true, .. } if !ctx.egui_wants_keyboard_input() => state.delete_point(s)?,
            _ => {}
        }
    }
    Ok(())
}
/// 46 uses this mapping only in the Vector persona, after keyboard-focus routing.
pub fn shortcut(key: Key, mods: Modifiers, wants_keyboard: bool) -> Option<Tool> {
    if wants_keyboard || mods.command || mods.ctrl || mods.alt {
        return None;
    }
    match key {
        Key::Plus => Some(Tool::AddAnchor),
        Key::Equals if mods.shift => Some(Tool::AddAnchor),
        Key::Minus if !mods.shift => Some(Tool::DeleteAnchor),
        Key::C if mods.shift => Some(Tool::AnchorPoint),
        Key::Backtick if mods.shift => Some(Tool::Curvature),
        _ => None,
    }
}
/// Only controls/rubber band are egui strokes; filled preview pixels use display_doc and the
/// normal app compositor. Thus release compares actual cached pixels, not an overlay.
pub fn paint_overlay(state: &State, painter: &egui::Painter, to_screen: impl Fn(Point) -> Pos2) {
    let ink = egui::Color32::from_rgb(48, 180, 255);
    if let Some(path) = state.curvature_path() {
        for sp in path.subpaths {
            for k in sp.knots {
                painter.circle_filled(to_screen(k.anchor), 3., ink);
            }
        }
    }
    if let Some(path) = state.rubber_band() {
        for sp in path.subpaths {
            for seg in sp.segments() {
                let points = (0..=32)
                    .map(|i| {
                        let t = i as f64 / 32.;
                        let u = 1. - t;
                        to_screen(Point::new(
                            u * u * u * seg[0].x + 3. * u * u * t * seg[1].x + 3. * u * t * t * seg[2].x + t * t * t * seg[3].x,
                            u * u * u * seg[0].y + 3. * u * u * t * seg[1].y + 3. * u * t * t * seg[2].y + t * t * t * seg[3].y,
                        ))
                    })
                    .collect();
                painter.add(egui::Shape::line(points, egui::Stroke::new(1., ink)));
            }
        }
    }
    for g in &state.guides {
        painter.line_segment([to_screen(Point::new(g.a[0], g.a[1])), to_screen(Point::new(g.b[0], g.b[1]))], egui::Stroke::new(1., egui::Color32::MAGENTA));
    }
}

/// Active Curvature owns its point-editing keys before generic Edit menu shortcuts.
pub fn keys(app: &mut crate::PhotocraftApp, ctx: &egui::Context) -> bool {
    if ctx.egui_wants_keyboard_input() {
        return false;
    }
    frame(app, ctx);
    let mut b = bridge(ctx);
    if b.tool != Some(Tool::Curvature) || !b.state.active() {
        return false;
    }
    let key = [Key::Delete, Key::Backspace, Key::Enter, Key::Escape].into_iter().find(|&key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)));
    let Some(key) = key else { return false };
    let result = match key {
        Key::Delete | Key::Backspace => b.state.delete_point(&mut app.session),
        _ if b.state.captured() => {
            b.state.cancel();
            Ok(())
        }
        _ => b.state.finish(&mut app.session),
    };
    if let Err(e) = result {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
    save_bridge(ctx, b);
    true
}

/// Transient app bridge. 00 hooks route/display_doc/frame without expanding persisted UI state.
#[derive(Clone, Default)]
pub struct AppBridge {
    pub tool: Option<Tool>,
    pub state: State,
    pub routed: bool,
    owner: Option<crate::state::Tool>,
    persona: Option<photocraft_engine::toolsets::Persona>,
    module: Option<crate::Module>,
}
fn bridge_key() -> egui::Id {
    egui::Id::new("vector-anchor-tools-33")
}
pub fn bridge(ctx: &egui::Context) -> AppBridge {
    ctx.data_mut(|d| d.get_temp::<AppBridge>(bridge_key()).unwrap_or_default())
}
pub fn save_bridge(ctx: &egui::Context, bridge: AppBridge) {
    ctx.data_mut(|d| d.insert_temp(bridge_key(), bridge));
}
pub fn activate(app: &crate::PhotocraftApp, ctx: &egui::Context, tool: Option<Tool>) {
    let mut b = bridge(ctx);
    b.state.cancel();
    b.tool = tool;
    if let Some(tool) = tool {
        b.state.options.tool = tool;
    }
    b.owner = Some(app.ui.tool);
    b.persona = Some(app.session.prefs().toolbar.persona);
    b.module = Some(app.current_module);
    ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
}
pub fn frame(app: &crate::PhotocraftApp, ctx: &egui::Context) {
    let mut b = bridge(ctx);
    if b.owner.is_some_and(|t| t != app.ui.tool)
        || b.persona.is_some_and(|p| p != app.session.prefs().toolbar.persona)
        || b.module.is_some_and(|m| m != app.current_module)
    {
        b.tool = None;
    }
    let enabled = b.tool.is_some() && app.ui.dialogs.is_empty() && ctx.input(|i| i.focused);
    b.state.synchronize(&app.session, enabled);
    ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
}
/// Put before ordinary canvas tool input and draw, passing 03's displayed handles. A true
/// result consumes the tool route. An error becomes app status; it never publishes a preview.
pub fn route(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, idx: usize, xf: &crate::canvas::ViewXform, rect: Rect, shown: &[ShownHandle]) -> bool {
    if app.session.active_index() != Some(idx) {
        return false;
    }
    let ctx = ui.ctx().clone();
    frame(app, &ctx);
    let mut b = bridge(&ctx);
    if b.tool.is_none() || !app.ui.dialogs.is_empty() {
        return false;
    }
    b.routed = true;
    b.state.options.snapping.enabled = app.ui.extras.snap;
    let view = snap::View { zoom: f64::from(xf.zoom), pixels_per_point: f64::from(ctx.pixels_per_point()) };
    let targets = selected_targets(&app.session, &app.ui.direct_selection, false);
    let tolerance = match view.tolerance(6.) {
        Ok(t) => t,
        Err(e) => {
            app.ui.status = e.to_string();
            app.ui.status_error = true;
            return true;
        }
    };
    let result = input(
        &mut b.state,
        &mut app.session,
        &ctx,
        rect,
        |p| {
            let q = xf.to_doc(p);
            Point::new(q[0], q[1])
        },
        targets,
        shown,
        true,
        tolerance,
        view,
        None,
        Some(ShapeStroke::default()),
    );
    if let Err(e) = result {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
    for target in std::mem::take(&mut b.state.last_targets) {
        let reference = if target.name == "layer" { target.layer.map(crate::direct_select::PathRef::Layer) } else { Some(crate::direct_select::PathRef::Work) };
        if app.ui.direct_selection.target == reference {
            app.ui.direct_selection.anchors = target.anchors;
        } else if let Some(selected) = app.ui.direct_selection.additional.iter_mut().find(|s| Some(s.target) == reference) {
            selected.anchors = target.anchors;
        }
    }
    if app.ui.view.extras {
        paint_overlay(&b.state, &ui.painter_at(rect), |p| xf.to_screen(p.x as f32, p.y as f32));
    }
    if b.state.active() {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
    true
}
pub fn display_doc(app: &crate::PhotocraftApp, idx: usize, ctx: &egui::Context) -> Option<(Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) {
        return None;
    }
    let b = bridge(ctx);
    if b.tool.is_none() || b.owner != Some(app.ui.tool) || b.persona != Some(app.session.prefs().toolbar.persona) || b.module != Some(app.current_module) {
        return None;
    }
    b.state.display_doc(&app.session)
}

#[cfg(test)]
#[path = "anchor_tools/tests.rs"]
mod tests;
