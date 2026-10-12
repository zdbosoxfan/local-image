//! V3 gestures adapted from VectorCraft crates/tools/src/direct.rs, pinned at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092, MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//!
//! Direct Selection tool (A, #790), and the Pen's ⌘/Ctrl (Direct Selection) and ⌥/Alt (Convert
//! Point) modifiers, as in Photoshop. It edits the paths the canvas shows: the targeted vector
//! mask or the active shape layer's path, and the work path.
//!
//! * Click an anchor to select it (⇧ adds or removes, ⌥ selects its whole subpath), drag on empty
//!   canvas to marquee-select anchors, click empty canvas to deselect.
//! * Drag selected anchors to move them, a direction handle to turn it (a smooth point keeps its
//!   handles collinear; ⌥ breaks them apart), or a segment to bend it (straight segments move).
//! * Pen + ⌥: click an anchor to make it a corner, drag from it to pull out smooth handles, drag a
//!   handle to move it alone.
//!
//! A drag previews with `photocraft_vector::edit` and commits once on release through the engine
//! (`path.moveAnchors`, `path.moveHandle`, `path.bendSegment`, `path.convertPoint`): one history
//! step per drag, and the same commands drive it over MCP and the control channel.

use egui::{Color32, Modifiers, Pos2};
use photocraft_doc::vector::Path;
use photocraft_geom::Point;
use photocraft_vector::edit::{self, Handle, Hit};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::canvas::ToolEvent;
use crate::state::Tool;
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_engine::vector_packages::node::{self, Operation, Target};
use std::sync::Arc;

/// A path Direct Selection edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathRef {
    Work,
    /// A layer's shape path, or its vector mask.
    Layer(u64),
}

/// Direct Selection state: which anchors are selected and the drag in progress.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DirectSelection {
    /// The path whose anchors are selected.
    pub target: Option<PathRef>,
    /// Selected anchors as `[subpath, knot]`.
    pub anchors: Vec<[usize; 2]>,
    /// A clicked segment (`[subpath, knot]` it leaves): its two handles show.
    pub segment: Option<[usize; 2]>,
    #[serde(skip)]
    pub drag: Option<Drag>,
    /// Selections on other curves; old single-curve UI settings still deserialize.
    pub additional: Vec<NodeSelection>,
    pub transform_mode: bool,
    #[serde(skip)]
    pub document: Option<u64>,
    #[serde(skip)]
    pub preview_doc: Option<(Arc<Document>, u64)>,
    #[serde(skip)]
    pub pending: Option<Pending>,
    #[serde(skip)]
    pub last_click: Option<(f64, [f64; 2])>,
    #[serde(skip)]
    pub pixels_per_point: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeSelection {
    pub target: PathRef,
    pub anchors: Vec<[usize; 2]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub tool: Tool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub capture: Capture,
    pub targets: Vec<Target>,
    pub operation: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Drag {
    /// Editing `target`: `base` is its path at the press, `preview` the edit so far.
    Edit { target: PathRef, base: Path, edit: Edit, start: [f64; 2], end: [f64; 2], preview: Option<Path>, targets: Vec<Target>, capture: Capture },
    /// Marquee-selecting anchors (⇧ adds to the selection).
    Marquee { start: [f64; 2], end: [f64; 2], add: bool, capture: Capture, paths: Vec<(PathRef, Path)> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Anchors(Vec<[usize; 2]>),
    /// A handle, where it was at the press, and whether it moves alone (⌥).
    Handle([usize; 2], Handle, Point, bool),
    Segment([usize; 2], f64),
    /// Pen + ⌥ on an anchor: a click makes a corner, a drag pulls out smooth handles.
    Convert([usize; 2]),
    Transform {
        origin: [f64; 2],
        from: [f64; 2],
        rotate: bool,
    },
}

impl Edit {
    /// The edit for a drag from `start` to `end`, as a command and its params (without target).
    fn command(&self, start: [f64; 2], end: [f64; 2]) -> (&'static str, Value) {
        let d = [end[0] - start[0], end[1] - start[1]];
        match self {
            Edit::Anchors(a) => ("path.moveAnchors", json!({"anchors": a, "move": d})),
            Edit::Handle([s, k], h, from, independent) => {
                ("path.moveHandle", json!({"subpath": s, "knot": k, "handle": h.name(), "to": [from.x + d[0], from.y + d[1]], "independent": independent}))
            }
            Edit::Segment([s, k], t) => ("path.bendSegment", json!({"subpath": s, "knot": k, "t": t, "move": d})),
            Edit::Convert([s, k]) => ("path.convertPoint", json!({"subpath": s, "knot": k, "out": end})),
            Edit::Transform { .. } => ("path.node.edit", json!({"operation": self.operation(start, end)})),
        }
    }
}

/// Is Direct Selection what the pointer does now: its own tool, or the Pen with ⌘/Ctrl held.
fn direct(tool: Tool, mods: Modifiers) -> bool {
    tool == Tool::DirectSelection || (tool == Tool::Pen && mods.command)
}

/// Does the canvas draw Direct Selection's overlay instead of the plain path outlines?
pub fn shows(app: &PhotocraftApp, mods: Modifiers) -> bool {
    direct(app.ui.tool, mods) || app.ui.direct_selection.drag.is_some()
}

/// The paths Direct Selection edits, front first: the targeted vector mask or the active shape
/// layer's path, then the work path.
fn visit_candidates<'a>(app: &'a PhotocraftApp, mut visit: impl FnMut(PathRef, &'a Path)) {
    let Some(st) = app.session.active() else { return };
    if let Some((id, p)) = crate::vector_ui::targeted_vector_mask_ref(app) {
        visit(PathRef::Layer(id), p);
    } else {
        let selected = if st.selected_layers.is_empty() { st.active_layer.as_slice() } else { st.selected_layers.as_slice() };
        for id in selected.iter().rev() {
            if let Some(l) = st.doc.layer(*id)
                && let LayerContent::Shape(sh) = &l.content
            {
                visit(PathRef::Layer(id.0), &sh.path);
            }
        }
    }
    if let Some(path) = &st.doc.work_path {
        visit(PathRef::Work, path);
    }
}
fn candidates(app: &PhotocraftApp) -> Vec<(PathRef, Path)> {
    let mut paths = Vec::new();
    visit_candidates(app, |r, p| paths.push((r, p.clone())));
    paths
}
fn borrowed_path(doc: &Document, r: PathRef) -> Option<&Path> {
    match r {
        PathRef::Work => doc.work_path.as_ref(),
        PathRef::Layer(id) => {
            let l = doc.layer(photocraft_doc::LayerId(id))?;
            if let LayerContent::Shape(sh) = &l.content { Some(&sh.path) } else { l.vector_mask.as_ref().map(|m| &m.path) }
        }
    }
}

impl DirectSelection {
    fn anchors_for(&self, target: PathRef) -> Vec<[usize; 2]> {
        self.anchors_slice(target).to_vec()
    }
    fn anchors_slice(&self, target: PathRef) -> &[[usize; 2]] {
        if self.target == Some(target) {
            return &self.anchors;
        }
        self.additional.iter().find(|s| s.target == target).map_or(&[], |s| s.anchors.as_slice())
    }
    fn select(&mut self, target: PathRef, anchors: Vec<[usize; 2]>, add: bool) {
        if !add {
            self.additional.clear();
            self.anchors.clear();
            self.target = None;
        }
        if self.target == Some(target) {
            self.anchors = anchors;
        } else if self.target.is_none() {
            self.target = Some(target);
            self.anchors = anchors;
        } else if let Some(s) = self.additional.iter_mut().find(|s| s.target == target) {
            s.anchors = anchors;
        } else {
            self.additional.push(NodeSelection { target, anchors });
        }
    }
    fn targets(&self) -> Vec<Target> {
        self.target
            .into_iter()
            .map(|target| NodeSelection { target, anchors: self.anchors.clone() })
            .chain(self.additional.clone())
            .filter(|s| !s.anchors.is_empty())
            .map(|s| target(s.target, s.anchors))
            .collect()
    }
}
fn target(path: PathRef, anchors: Vec<[usize; 2]>) -> Target {
    match path {
        PathRef::Work => Target { anchors, ..Default::default() },
        PathRef::Layer(id) => Target { name: "layer".into(), layer: Some(id), anchors },
    }
}
fn capture(app: &PhotocraftApp) -> Option<Capture> {
    let st = app.session.active()?;
    Some(Capture { document: st.doc.id.0, revision: st.revision, tool: app.ui.tool })
}
fn current(app: &PhotocraftApp, c: &Capture) -> bool {
    capture(app).is_some_and(|now| now == *c)
}
/// Package 00 installs package 07's read-only query adapter. The query excludes the selected
/// nodes in targets and returns document-space geometry; previews and releases use it alike.
pub type SnapQuery = fn(&Document, &[Target], Point, f64) -> Result<Point, String>;
static SNAP_QUERY: std::sync::OnceLock<SnapQuery> = std::sync::OnceLock::new();
pub fn install_snapping(query: SnapQuery) -> Result<(), SnapQuery> {
    SNAP_QUERY.set(query)
}
fn pick_tolerance(app: &PhotocraftApp) -> f64 {
    // Canvas input and overlays use logical points. DPI scales both together;
    // only document zoom converts the six-point hit radius to document units.
    6.0 / f64::from(app.current_zoom().max(0.01))
}
fn snapped_end(app: &PhotocraftApp, base: &Path, edit: &Edit, targets: &[Target], start: [f64; 2], end: [f64; 2], mods: Modifiers) -> [f64; 2] {
    if mods.shift || mods.command || !app.ui.extras.snap {
        return end;
    }
    let Some(query) = SNAP_QUERY.get() else { return end };
    let grab = match edit {
        Edit::Anchors(anchors) => anchors
            .iter()
            .filter_map(|[s, k]| base.subpaths.get(*s)?.knots.get(*k).map(|kn| kn.anchor))
            .min_by(|a, b| (a.x - start[0]).hypot(a.y - start[1]).total_cmp(&(b.x - start[0]).hypot(b.y - start[1]))),
        Edit::Handle(_, _, from, _) => Some(*from),
        _ => None,
    };
    let Some(grab) = grab else { return end };
    let Some(st) = app.session.active() else { return end };
    let p = Point::new(grab.x + end[0] - start[0], grab.y + end[1] - start[1]);
    match query(&st.doc, targets, p, pick_tolerance(app)) {
        Ok(to) if to.x.is_finite() && to.y.is_finite() => [start[0] + to.x - grab.x, start[1] + to.y - grab.y],
        _ => end,
    }
}
fn bounded_end(start: [f64; 2], mut end: [f64; 2], shift: bool) -> [f64; 2] {
    if shift {
        let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
        let angle = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
        let length = dx.hypot(dy);
        end = [start[0] + angle.cos() * length, start[1] + angle.sin() * length];
    }
    end
}
fn adjusted_end(base: &Path, edit: &Edit, start: [f64; 2], end: [f64; 2], mods: Modifiers) -> [f64; 2] {
    if !mods.shift {
        return end;
    }
    match edit {
        Edit::Handle([s, k], _, from, _) => {
            if let Some(kn) = base.subpaths.get(*s).and_then(|sp| sp.knots.get(*k)) {
                let to = [from.x + end[0] - start[0], from.y + end[1] - start[1]];
                let to = bounded_end([kn.anchor.x, kn.anchor.y], to, true);
                [start[0] + to[0] - from.x, start[1] + to[1] - from.y]
            } else {
                end
            }
        }
        Edit::Transform { origin, from, rotate: false } => {
            let ratio = |i: usize| if (from[i] - origin[i]).abs() > 1e-9 { (end[i] - origin[i]) / (from[i] - origin[i]) } else { 1.0 };
            let (x, y) = (ratio(0), ratio(1));
            let scale = if x.abs() > y.abs() { x } else { y };
            [origin[0] + scale * (from[0] - origin[0]), origin[1] + scale * (from[1] - origin[1])]
        }
        Edit::Segment(_, _) | Edit::Convert(_) => end,
        _ => bounded_end(start, end, true),
    }
}
impl Edit {
    fn operation(&self, start: [f64; 2], end: [f64; 2]) -> Operation {
        let delta = [end[0] - start[0], end[1] - start[1]];
        match self {
            Edit::Anchors(_) => Operation::Move { delta },
            Edit::Handle(knot, handle, from, independent) => {
                Operation::Handle { knot: *knot, handle: handle.name().into(), to: [from.x + delta[0], from.y + delta[1]], independent: *independent }
            }
            Edit::Segment(knot, t) => Operation::Bend { knot: *knot, t: *t, delta },
            Edit::Convert(knot) => Operation::Convert { knot: *knot, out: Some(end) },
            Edit::Transform { origin, from, rotate } => {
                let [ox, oy] = *origin;
                let (a, b, c, d) = if *rotate {
                    let angle = (end[1] - oy).atan2(end[0] - ox) - (from[1] - oy).atan2(from[0] - ox);
                    (angle.cos(), angle.sin(), -angle.sin(), angle.cos())
                } else {
                    let scale = |e: f64, f: f64, o: f64| if (f - o).abs() > 1e-9 { (e - o) / (f - o) } else { 1.0 };
                    (scale(end[0], from[0], ox), 0.0, 0.0, scale(end[1], from[1], oy))
                };
                Operation::Transform { matrix: [a, b, c, d, ox - a * ox - c * oy, oy - b * ox - d * oy] }
            }
        }
    }
}

fn retracted(k: &photocraft_doc::Knot, h: Handle) -> bool {
    let c = if h == Handle::In { k.in_ctrl } else { k.out_ctrl };
    (c.x - k.anchor.x).abs() < 1e-9 && (c.y - k.anchor.y).abs() < 1e-9
}

/// The handles shown on `path`: both of every selected anchor and the two of a clicked segment
/// (retracted handles sit on their anchor and don't show).
fn shown_handles(ds: &DirectSelection, path: &Path) -> Vec<([usize; 2], Handle)> {
    let mut out = Vec::new();
    visit_handles(&ds.anchors, ds.segment, path, |r, h| out.push((r, h)));
    out
}
fn visit_handles(anchors: &[[usize; 2]], segment: Option<[usize; 2]>, path: &Path, mut visit: impl FnMut([usize; 2], Handle)) {
    let mut visible = |r: [usize; 2], h| {
        if path.subpaths.get(r[0]).and_then(|s| s.knots.get(r[1])).is_some_and(|k| !retracted(k, h)) {
            visit(r, h);
        }
    };
    for &r in anchors {
        visible(r, Handle::In);
        visible(r, Handle::Out);
    }
    if let Some([s, k]) = segment
        && let Some(sp) = path.subpaths.get(s)
    {
        let n = sp.knots.len();
        let next = k.checked_add(1).filter(|&j| j < n).or((sp.closed && n > 1).then_some(0));
        visible([s, k], Handle::Out);
        if let Some(j) = next {
            visible([s, j], Handle::In);
        }
    }
}

/// Canvas pointer events for Direct Selection and the Pen's ⌘ / ⌥ modes; false when the event is
/// not theirs.
pub fn pointer(app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> bool {
    let tool = app.ui.tool;
    let p = match ev {
        ToolEvent::Down { x, y, .. } | ToolEvent::Move { x, y, .. } | ToolEvent::Up { x, y } => [x, y],
    };
    if p.iter().any(|v| !v.is_finite() || v.abs() > edit::MAX_COORDINATE) {
        cancel(app);
        return tool == Tool::DirectSelection;
    }
    if let Some(c) = app.ui.direct_selection.drag.as_ref().map(|d| match d {
        Drag::Edit { capture, .. } | Drag::Marquee { capture, .. } => capture.clone(),
    }) && !current(app, &c)
    {
        cancel(app);
        return true;
    }
    match ev {
        ToolEvent::Down { x, y, .. } => {
            if app.ui.direct_selection.pending.is_some() {
                return true;
            }
            let pen_idle = app.ui.pen.is_none();
            let convert = tool == Tool::Pen && mods.alt && !mods.command && pen_idle;
            if !(direct(tool, mods) || convert) {
                return false;
            }
            if tool == Tool::Pen && !pen_idle {
                crate::vector_ui::pen_commit(app, false);
            }
            let document = app.session.active().map(|s| s.doc.id.0);
            if app.ui.direct_selection.document != document {
                app.ui.direct_selection = DirectSelection {
                    document,
                    pixels_per_point: app.ui.direct_selection.pixels_per_point,
                    transform_mode: app.ui.direct_selection.transform_mode,
                    ..Default::default()
                };
            }
            press(app, [x, y], mods, convert)
        }
        ToolEvent::Move { x, y, .. } => {
            let Some(mut drag) = app.ui.direct_selection.drag.take() else { return tool == Tool::DirectSelection };
            match &mut drag {
                Drag::Edit { base, edit, start, end, preview, targets, .. } => {
                    let previous = edit.operation(*start, *end);
                    let snapped = snapped_end(app, base, edit, targets, *start, [x, y], mods);
                    *end = adjusted_end(base, edit, *start, snapped, mods);
                    if let Edit::Handle(_, _, _, independent) = edit {
                        *independent = mods.alt;
                    }
                    let op = edit.operation(*start, *end);
                    if previous == op && app.ui.direct_selection.preview_doc.is_some() {
                        app.ui.direct_selection.drag = Some(drag);
                        return true;
                    }
                    *preview = None;
                    if let Some(st) = app.session.active() {
                        match node::prepare(&st.doc, targets, &op, true) {
                            Ok(shown) => {
                                if let Some(t) = targets.first() {
                                    *preview = node::path(&shown.doc, t).ok().cloned();
                                }
                                let next = app.ui.direct_selection.preview_doc.as_ref().map_or(1, |(_, k)| k.wrapping_add(1));
                                app.ui.direct_selection.preview_doc = Some((Arc::new(shown.doc), next | (1 << 55)));
                            }
                            Err(_) => {
                                *preview = None;
                                app.ui.direct_selection.preview_doc = None;
                            }
                        }
                    }
                }
                Drag::Marquee { end, add, .. } => {
                    *end = [x, y];
                    *add = mods.shift;
                }
            }
            app.ui.direct_selection.drag = Some(drag);
            true
        }
        ToolEvent::Up { x, y } => {
            app.ui.direct_selection.preview_doc = None;
            match app.ui.direct_selection.drag.take() {
                Some(Drag::Edit { target, base, mut edit, start, targets, capture, .. }) => {
                    let end = snapped_end(app, &base, &edit, &targets, start, [x, y], mods);
                    let end = adjusted_end(&base, &edit, start, end, mods);
                    if let Edit::Handle(_, _, _, independent) = &mut edit {
                        *independent = mods.alt;
                    }
                    release(app, target, &edit, start, end, targets, capture);
                    true
                }
                Some(Drag::Marquee { start, paths, .. }) => {
                    marquee(app, paths, start, [x, y], mods.shift);
                    true
                }
                None => tool == Tool::DirectSelection,
            }
        }
    }
}

/// A press: pick what's under the pointer and start a drag. False lets the Pen handle a ⌥-press
/// that hit nothing.
fn press(app: &mut PhotocraftApp, p: [f64; 2], mods: Modifiers, convert: bool) -> bool {
    let tol = pick_tolerance(app);
    let ds = &app.ui.direct_selection;
    let found = candidates(app).into_iter().find_map(|(r, path)| {
        let local = DirectSelection {
            target: Some(r),
            anchors: ds.anchors_for(r),
            segment: if ds.target == Some(r) { ds.segment } else { None },
            ..Default::default()
        };
        let handles = shown_handles(&local, &path);
        node::hit(&path, Point::new(p[0], p[1]), tol, &handles).map(|h| (r, path, h))
    });
    let Some(capture) = capture(app) else { return true };
    if !convert
        && app.ui.direct_selection.transform_mode
        && let Some(bounds) = selection_bounds(app)
    {
        let corners = [[bounds[0], bounds[1]], [bounds[2], bounds[1]], [bounds[2], bounds[3]], [bounds[0], bounds[3]]];
        if let Some((i, from)) = corners.into_iter().enumerate().find(|(_, q)| (q[0] - p[0]).hypot(q[1] - p[1]) < tol) {
            let targets = app.ui.direct_selection.targets();
            if let Some(t) = targets.first()
                && let Some(st) = app.session.active()
                && let Ok(base) = node::path(&st.doc, t)
            {
                let origin = if mods.alt { [(bounds[0] + bounds[2]) * 0.5, (bounds[1] + bounds[3]) * 0.5] } else { corners[(i + 2) % 4] };
                app.ui.direct_selection.drag = Some(Drag::Edit {
                    target: app.ui.direct_selection.target.unwrap_or(PathRef::Work),
                    base: base.clone(),
                    edit: Edit::Transform { origin, from, rotate: mods.command },
                    start: p,
                    end: p,
                    preview: None,
                    targets,
                    capture,
                });
                return true;
            }
        }
    }
    let paths = candidates(app);
    let ds = &mut app.ui.direct_selection;
    let Some((target, base, hit)) = found else {
        if convert {
            return false;
        }
        ds.drag = Some(Drag::Marquee { start: p, end: p, add: mods.shift, capture, paths });
        return true;
    };
    if ds.target != Some(target) {
        let selected = ds.anchors_for(target);
        if mods.shift || !selected.is_empty() {
            ds.additional.retain(|s| s.target != target);
            if let Some(old) = ds.target {
                ds.additional.push(NodeSelection { target: old, anchors: ds.anchors.clone() });
            }
            ds.target = Some(target);
            ds.anchors = selected;
        } else {
            ds.select(target, Vec::new(), false);
        }
    }
    let mut anchors = ds.anchors_for(target);
    let edit = match hit {
        Hit::Handle(k, h) => {
            let kn = base.subpaths.get(k[0]).and_then(|s| s.knots.get(k[1]));
            let from = kn.map_or(Point::new(p[0], p[1]), |kn| if h == Handle::In { kn.in_ctrl } else { kn.out_ctrl });
            Edit::Handle(k, h, from, mods.alt)
        }
        Hit::Anchor(k) if convert => Edit::Convert(k),
        // Pen + ⌥ on a segment does nothing.
        Hit::Segment(..) if convert => return true,
        Hit::Anchor(k) => {
            ds.segment = None;
            if mods.alt {
                anchors = edit::subpath_anchors(&base, k[0]);
            } else if mods.shift {
                // ⇧-click toggles the anchor; a click that deselects drags nothing.
                if let Some(i) = anchors.iter().position(|a| *a == k) {
                    anchors.remove(i);
                    ds.select(target, anchors, true);
                    return true;
                }
                anchors.push(k);
            } else if !anchors.contains(&k) {
                anchors = vec![k];
                ds.additional.clear();
            }
            Edit::Anchors(anchors.clone())
        }
        Hit::Segment(k, _) if mods.alt => {
            ds.segment = None;
            anchors = edit::subpath_anchors(&base, k[0]);
            Edit::Anchors(anchors.clone())
        }
        Hit::Segment(k, t) => {
            let next = base.subpaths[k[0]].knots.len();
            if !mods.shift {
                anchors.clear();
            }
            for a in [k, [k[0], (k[1] + 1) % next]] {
                if !anchors.contains(&a) {
                    anchors.push(a);
                }
            }
            if !mods.shift {
                ds.additional.clear();
            }
            ds.segment = Some(k);
            Edit::Segment(k, t)
        }
    };
    ds.select(target, anchors, true);
    let targets = if matches!(edit, Edit::Anchors(_)) { ds.targets() } else { vec![self::target(target, ds.anchors_for(target))] };
    ds.drag = Some(Drag::Edit { target, base, edit, start: p, end: p, preview: None, targets, capture });
    true
}

/// End of an edit drag: commit it as one command. A click (no movement) commits nothing, except
/// Pen + ⌥ on an anchor, which makes it a corner.
fn release(app: &mut PhotocraftApp, _target: PathRef, edit: &Edit, start: [f64; 2], end: [f64; 2], targets: Vec<Target>, capture: Capture) {
    let moved = (end[0] - start[0]).hypot(end[1] - start[1]) * f64::from(app.current_zoom()) >= 1.0;
    if moved {
        app.ui.direct_selection.last_click = None;
    }
    let (id, mut params) = match edit {
        Edit::Convert([s, k]) if !moved => ("path.convertPoint", json!({"subpath": s, "knot": k})),
        _ if !moved => return,
        e => e.command(start, end),
    };
    params["targets"] = json!(targets);
    params["document"] = json!(capture.document);
    params["revision"] = json!(capture.revision);
    let op = match edit {
        Edit::Convert(knot) if !moved => Operation::Convert { knot: *knot, out: None },
        _ => edit.operation(start, end),
    };
    if live_targets(app, &targets) {
        app.ui.direct_selection.pending = Some(Pending { capture, targets, operation: json!(op) });
        return;
    }
    run(app, id, params);
}
fn live_targets(app: &PhotocraftApp, targets: &[Target]) -> bool {
    targets.iter().any(|t| {
        app.session
            .active()
            .and_then(|st| t.layer.and_then(|id| st.doc.layer(LayerId(id))))
            .is_some_and(|l| l.vector.live_shape.is_some() || matches!(&l.content, LayerContent::Shape(sh) if sh.live.is_some()))
    })
}
fn run(app: &mut PhotocraftApp, id: &str, params: Value) {
    match app.run(id, params) {
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        Ok(result) => {
            if let Ok(targets) = serde_json::from_value::<Vec<Target>>(result["targets"].clone()) {
                app.ui.direct_selection.additional.clear();
                app.ui.direct_selection.target = None;
                app.ui.direct_selection.anchors.clear();
                for t in targets {
                    let path = t.layer.map_or(PathRef::Work, PathRef::Layer);
                    app.ui.direct_selection.select(path, t.anchors, true);
                }
            }
        }
    }
}
fn marquee(app: &mut PhotocraftApp, paths: Vec<(PathRef, Path)>, start: [f64; 2], end: [f64; 2], add: bool) {
    let hits: Vec<_> = paths.into_iter().map(|(r, p)| (r, edit::anchors_in(&p, [start[0], start[1], end[0], end[1]]))).filter(|(_, a)| !a.is_empty()).collect();
    let ds = &mut app.ui.direct_selection;
    if !add {
        ds.target = None;
        ds.anchors.clear();
        ds.additional.clear();
    }
    for (r, mut anchors) in hits {
        for old in ds.anchors_for(r) {
            if !anchors.contains(&old) {
                anchors.push(old);
            }
        }
        ds.select(r, anchors, true);
    }
    ds.segment = None;
}

pub fn cancel(app: &mut PhotocraftApp) {
    let ds = &mut app.ui.direct_selection;
    ds.drag = None;
    ds.preview_doc = None;
    ds.pending = None;
    ds.last_click = None;
}

/// Package 00 calls before shortcut dispatch, and each frame even when the pointer is still.
pub fn frame(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    app.ui.direct_selection.pixels_per_point = ctx.pixels_per_point();
    let document = app.session.active().map(|s| s.doc.id.0);
    if document != app.ui.direct_selection.document {
        app.ui.direct_selection = DirectSelection {
            document,
            transform_mode: app.ui.direct_selection.transform_mode,
            pixels_per_point: ctx.pixels_per_point(),
            ..Default::default()
        };
    }
    let c = app.ui.direct_selection.drag.as_ref().map(|d| match d {
        Drag::Edit { capture, .. } | Drag::Marquee { capture, .. } => capture,
    });
    if c.is_some_and(|c| !current(app, c)) || app.ui.direct_selection.pending.as_ref().is_some_and(|p| !current(app, &p.capture)) {
        cancel(app);
    }
    if !ctx.input(|i| i.focused) {
        app.ui.direct_selection.drag = None;
    }
    if app.ui.tool != Tool::DirectSelection && app.ui.direct_selection.drag.is_none() {
        return false;
    }
    if ctx.text_edit_focused() {
        return false;
    }
    if ctx.input_mut(|i| i.consume_key(i.modifiers, egui::Key::Escape)) {
        cancel(app);
        return true;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, egui::Key::Delete) || i.consume_key(Modifiers::NONE, egui::Key::Backspace)) {
        action(app, Operation::Delete);
        return true;
    }
    false
}

/// Raw pointer capture for package 00's canvas router. A press begins immediately, moves and
/// release keep working outside the widget. Return true to suppress legacy tool_event dispatch.
pub fn canvas_input(app: &mut PhotocraftApp, ctx: &egui::Context, idx: usize, xf: &crate::canvas::ViewXform, rect: egui::Rect) -> bool {
    if app.session.active_index() != Some(idx) {
        return false;
    }
    let modal_frame = ctx.data(|d| d.get_temp::<u64>(egui::Id::new("node-modal-frame")));
    if app.ui.direct_selection.pending.is_some() || modal_frame == Some(ctx.cumulative_frame_nr()) {
        return true;
    }
    if app.ui.tool != Tool::DirectSelection && app.ui.direct_selection.drag.is_none() {
        return false;
    }
    if !ctx.input(|i| i.focused) {
        cancel(app);
        return true;
    }
    let mods = ctx.input(|i| i.modifiers);
    for event in ctx.input(|i| i.events.clone()) {
        match event {
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } if rect.contains(pos) => {
                let p = xf.to_doc(pos);
                let time = ctx.input(|i| i.time);
                let double = app
                    .ui
                    .direct_selection
                    .last_click
                    .is_some_and(|(t, q)| time - t <= 0.3 && (p[0] - q[0]).hypot(p[1] - q[1]) * f64::from(app.current_zoom()) <= 6.0);
                if double {
                    cancel(app);
                    double_click(app, p);
                    app.ui.direct_selection.last_click = None;
                } else {
                    pointer(app, ToolEvent::Down { x: p[0], y: p[1], pressure: 1.0 }, mods);
                    app.ui.direct_selection.last_click = Some((time, p));
                }
            }
            egui::Event::PointerMoved(pos) if app.ui.direct_selection.drag.is_some() => {
                let p = xf.to_doc(pos);
                pointer(app, ToolEvent::Move { x: p[0], y: p[1], pressure: 1.0 }, mods);
            }
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, .. } if app.ui.direct_selection.drag.is_some() => {
                let p = xf.to_doc(pos);
                pointer(app, ToolEvent::Up { x: p[0], y: p[1] }, mods);
            }
            _ => {}
        }
    }
    // Modifier-only frames must recompute the detached preview too.
    if app.ui.direct_selection.drag.is_some()
        && let Some(pos) = ctx.input(|i| i.pointer.latest_pos())
    {
        let p = xf.to_doc(pos);
        pointer(app, ToolEvent::Move { x: p[0], y: p[1], pressure: 1.0 }, mods);
    }
    true
}

pub fn double_click(app: &mut PhotocraftApp, p: [f64; 2]) {
    let tol = pick_tolerance(app);
    if let Some((r, knot, t)) = candidates(app).into_iter().find_map(|(r, path)| match node::hit(&path, Point::new(p[0], p[1]), tol, &[]) {
        Some(Hit::Segment(k, t)) => Some((r, k, t)),
        _ => None,
    }) {
        app.ui.direct_selection.select(r, Vec::new(), false);
        perform(app, vec![target(r, Vec::new())], Operation::Insert { knot, t });
    }
}
pub fn action(app: &mut PhotocraftApp, operation: Operation) {
    let targets = app.ui.direct_selection.targets();
    if !targets.is_empty() {
        perform(app, targets, operation);
    }
}
fn perform(app: &mut PhotocraftApp, targets: Vec<Target>, op: Operation) {
    let Some(capture) = capture(app) else { return };
    if live_targets(app, &targets) {
        app.ui.direct_selection.pending = Some(Pending { capture, targets, operation: json!(op) });
    } else {
        run(app, "path.node.edit", json!({"document":capture.document,"revision":capture.revision,"targets":targets,"operation":op}));
    }
}

pub fn selection_bounds(app: &PhotocraftApp) -> Option<[f64; 4]> {
    let st = app.session.active()?;
    let mut bounds: Option<[f64; 4]> = None;
    let ds = &app.ui.direct_selection;
    for (r, anchors) in ds.target.into_iter().map(|r| (r, ds.anchors_slice(r))).chain(ds.additional.iter().map(|s| (s.target, s.anchors.as_slice()))) {
        if anchors.is_empty() {
            continue;
        }
        let p = node::path(&st.doc, &target(r, Vec::new())).ok()?;
        for &[s, k] in anchors {
            let a = p.subpaths.get(s)?.knots.get(k)?.anchor;
            bounds = Some(bounds.map_or([a.x, a.y, a.x, a.y], |b| [b[0].min(a.x), b[1].min(a.y), b[2].max(a.x), b[3].max(a.y)]));
        }
    }
    bounds
}

/// Package 00 inserts this before other canvas display_doc providers. The returned document
/// has bounds-limited caches for changed shapes and never enters history or the journal.
pub fn display_doc(app: &PhotocraftApp, idx: usize) -> Option<(Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) {
        return None;
    }
    let Drag::Edit { capture, .. } = app.ui.direct_selection.drag.as_ref()? else { return None };
    if !current(app, capture) {
        return None;
    }
    app.ui.direct_selection.preview_doc.clone()
}

/// Context controls and the explicit live conversion prompt, owned entirely by package 03.
pub fn context_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        for (label, operation) in [
            (tl!("Sharp"), Operation::Sharp),
            (tl!("Smooth"), Operation::Smooth),
            (tl!("Smart"), Operation::Smart),
            (tl!("Break Curve"), Operation::Break),
            (tl!("Join Curves"), Operation::Join { tolerance: 0.1 }),
            (tl!("Close Curve"), Operation::Close),
            (tl!("Reverse Curve"), Operation::Reverse),
        ] {
            if ui.button(label).clicked() {
                action(app, operation);
            }
        }
        ui.checkbox(&mut app.ui.direct_selection.transform_mode, tl!("Transform selected nodes"))
            .on_hover_text(tl!("Drag a corner to scale; Ctrl or Command rotates; Shift preserves proportions; Alt uses the center."));
        if ui.button(tl!("Align nodes horizontally")).clicked()
            && let Some(b) = selection_bounds(app)
        {
            action(app, Operation::Align { axis: "y".into(), value: (b[1] + b[3]) * 0.5 });
        }
        if ui.button(tl!("Align nodes vertically")).clicked()
            && let Some(b) = selection_bounds(app)
        {
            action(app, Operation::Align { axis: "x".into(), value: (b[0] + b[2]) * 0.5 });
        }
    });

    conversion_dialog(app, ui.ctx());
}

/// Paths with their anchors (selected ones filled), the shown handles, the drag's preview in
/// place of its path, and the marquee.
pub fn draw_overlay(app: &PhotocraftApp, painter: &egui::Painter, to_scr: &dyn Fn([f64; 2]) -> Pos2, accent: Color32) {
    let ds = &app.ui.direct_selection;
    visit_candidates(app, |r, path| {
        let path = match &ds.drag {
            Some(Drag::Edit { target, preview: Some(p), .. }) if *target == r => p,
            _ => ds.preview_doc.as_ref().and_then(|(doc, _)| borrowed_path(doc, r)).unwrap_or(path),
        };
        crate::vector_ui::draw_outline(painter, path, to_scr, accent);
        let anchors = ds.anchors_slice(r);
        let segment = if ds.target == Some(r) { ds.segment } else { None };
        let ours = !anchors.is_empty() || segment.is_some();
        visit_handles(anchors, segment, path, |k, h| {
            if let Some(kn) = path.subpaths.get(k[0]).and_then(|s| s.knots.get(k[1])) {
                let c = if h == Handle::In { kn.in_ctrl } else { kn.out_ctrl };
                crate::vector_ui::draw_handle(painter, to_scr([kn.anchor.x, kn.anchor.y]), to_scr([c.x, c.y]), accent);
            }
        });
        for (s, sp) in path.subpaths.iter().enumerate() {
            for (k, kn) in sp.knots.iter().enumerate() {
                crate::vector_ui::draw_anchor(painter, to_scr([kn.anchor.x, kn.anchor.y]), ours && anchors.contains(&[s, k]), accent);
            }
        }
    });
    if ds.transform_mode
        && let Some(b) = selection_bounds(app)
    {
        let points = [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]].map(to_scr);
        painter.add(egui::Shape::closed_line(points.to_vec(), egui::Stroke::new(1.0, accent)));
        for p in points {
            crate::vector_ui::draw_anchor(painter, p, false, accent);
        }
    }
    if let Some(Drag::Marquee { start, end, .. }) = &ds.drag {
        let pts = [*start, [end[0], start[1]], *end, [start[0], end[1]]].map(to_scr);
        crate::tool_feedback::draw_ants(painter, &pts, true);
    }
}

#[cfg(test)]
#[path = "direct_select_tests.rs"]
mod tests;

/// Anchor tools hit exactly the direction handles currently shown by Direct Selection.
pub fn anchor_tool_handles(app: &PhotocraftApp) -> Vec<crate::anchor_tools::ShownHandle> {
    let Some(st) = app.session.active() else { return Vec::new() };
    let selection = &app.ui.direct_selection;
    let targets = crate::anchor_tools::selected_targets(&app.session, selection, false);
    let mut out = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        let path_ref = target.layer.map_or(PathRef::Work, PathRef::Layer);
        let Some(path) = borrowed_path(&st.doc, path_ref) else { continue };
        let ds = DirectSelection {
            anchors: target.anchors.clone(),
            segment: (selection.target == Some(path_ref)).then_some(selection.segment).flatten(),
            ..Default::default()
        };
        out.extend(shown_handles(&ds, path).into_iter().map(|(knot, handle)| crate::anchor_tools::ShownHandle { target: index, knot, handle }));
    }
    out
}

/// Draw independently of options-bar ownership and widget keyboard focus.
pub fn conversion_dialog(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if ctx.data(|d| d.get_temp::<u64>(egui::Id::new("node-modal-frame"))) == Some(ctx.cumulative_frame_nr()) {
        return;
    }
    if let Some(pending) = app.ui.direct_selection.pending.clone() {
        let frame = ctx.cumulative_frame_nr();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("node-modal-frame"), frame));
        egui::Modal::new(egui::Id::new("node-live-conversion")).show(ctx, |ui| {
            ui.label(tl!("Convert the live shape to curves to edit its nodes?"));
            if ui.button(tl!("Convert to curves")).clicked() {
                app.ui.direct_selection.pending = None;
                if current(app, &pending.capture) { run(app,"path.node.edit",json!({"targets":pending.targets,"operation":pending.operation,"convertLive":true,"document":pending.capture.document,"revision":pending.capture.revision})); }
            }
            if ui.button(tl!("Cancel")).clicked() { cancel(app); }
        });
    }
}
