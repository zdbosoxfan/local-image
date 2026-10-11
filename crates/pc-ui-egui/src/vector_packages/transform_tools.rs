//! Reference-point tools and dialogs adapted from VectorCraft tools/xform/{transform,free}.rs
//! and tools/extra.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
//! The host supplies document/view coordinates, selected IDs, and the ordinary preview plane.
use crate::i18n::t;
use egui::{Color32, Event, Key, Modifiers, Painter, PointerButton, Pos2, Rect, Stroke, Ui};
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_engine::{
    Session,
    vector_packages::{
        snapping::query as snap,
        transform_tools::{self as engine, Capture, Commit, ConversionChoice, Each, EnvelopeRequest, Node, Operation, Options},
    },
};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    #[default]
    Rotate,
    Reflect,
    Scale,
    Shear,
    Reshape,
    FreeTransform,
    Move,
    Each,
}
impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Rotate => "rotate",
            Self::Reflect => "reflect",
            Self::Scale => "scale",
            Self::Shear => "shear",
            Self::Reshape => "reshape",
            Self::FreeTransform => "freeTransform",
            Self::Move => "move",
            Self::Each => "transformEach",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Rotate => "Rotate",
            Self::Reflect => "Reflect",
            Self::Scale => "Scale",
            Self::Shear => "Shear",
            Self::Reshape => "Reshape",
            Self::FreeTransform => "Free Transform",
            Self::Move => "Move",
            Self::Each => "Transform Each",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FreeMode {
    #[default]
    Free,
    Perspective,
    Distort,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub options: Options,
    pub free_mode: FreeMode,
    pub reshape_radius: f64,
    pub snapping: snap::Settings,
}
impl Default for Preferences {
    fn default() -> Self {
        Self { options: Options::default(), free_mode: FreeMode::Free, reshape_radius: 100., snapping: snap::Settings::default() }
    }
}
#[derive(Clone, Copy)]
enum DragOp {
    Reference,
    Move,
    Scale(usize),
    Rotate,
    Shear(usize),
    Distort(usize, FreeMode),
    Reshape,
}
struct Drag {
    request: Commit,
    start: Point,
    pick: Point,
    origin: Point,
    bounds: [f64; 4],
    op: DragOp,
    moved: bool,
    alt_press: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MoveMode {
    #[default]
    Relative,
    Position,
    Distance,
}
struct Dialog {
    capture: Capture,
    kind: Kind,
    origin: Point,
    fields: [String; 5],
    preview: bool,
    vertical: bool,
    proportional: bool,
    reference: usize,
    random: bool,
    seed: String,
    move_mode: MoveMode,
}
pub struct State {
    pub kind: Kind,
    pub preferences: Preferences,
    pub origin: Option<Point>,
    pub preview: Option<Document>,
    pub generation: u64,
    pub error: Option<String>,
    pub selected_nodes: Vec<Node>,
    /// Take this captured request and route it to package 20; no destructive edit has occurred.
    pub envelope_request: Option<EnvelopeRequest>,
    pub guides: Vec<snap::Guide>,
    drag: Option<Drag>,
    dialog: Option<Dialog>,
    pending: Option<Commit>,
    targets: Vec<LayerId>,
    document: Option<u64>,
    last_kind: Kind,
    snap_cache: Option<snap::Cache>,
    drag_snap: Option<snap::Cache>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            kind: Kind::Rotate,
            preferences: Preferences::default(),
            origin: None,
            preview: None,
            generation: 0,
            error: None,
            selected_nodes: Vec::new(),
            envelope_request: None,
            guides: Vec::new(),
            drag: None,
            dialog: None,
            pending: None,
            targets: Vec::new(),
            document: None,
            last_kind: Kind::Rotate,
            snap_cache: None,
            drag_snap: None,
        }
    }
}
fn pt(p: [f64; 2]) -> Point {
    Point::new(p[0], p[1])
}
fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
fn corners([x, y, w, h]: [f64; 4]) -> [Point; 4] {
    [Point::new(x, y), Point::new(x + w, y), Point::new(x + w, y + h), Point::new(x, y + h)]
}
fn handles(b: [f64; 4]) -> [Point; 8] {
    let q = corners(b);
    quad_handles(q)
}
fn quad_handles(q: [Point; 4]) -> [Point; 8] {
    [
        q[0],
        q[1],
        q[2],
        q[3],
        Point::new((q[0].x + q[1].x) / 2., (q[0].y + q[1].y) / 2.),
        Point::new((q[1].x + q[2].x) / 2., (q[1].y + q[2].y) / 2.),
        Point::new((q[2].x + q[3].x) / 2., (q[2].y + q[3].y) / 2.),
        Point::new((q[3].x + q[0].x) / 2., (q[3].y + q[0].y) / 2.),
    ]
}
fn opposite(i: usize) -> usize {
    if i < 4 { (i + 2) % 4 } else { 4 + (i - 4 + 2) % 4 }
}
impl State {
    pub fn active(&self) -> bool {
        self.drag.is_some() || self.dialog.is_some() || self.pending.is_some() || self.envelope_request.is_some()
    }
    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.dialog = None;
        self.pending = None;
        self.preview = None;
        self.envelope_request = None;
        self.drag_snap = None;
        self.guides.clear();
        self.generation = self.generation.wrapping_add(1) | (1 << 59);
    }
    pub fn lifecycle(&mut self, s: &Session, ids: &[LayerId], enabled: bool, focused: bool, escape: bool) {
        let doc = s.active().map(|st| st.doc.id.0);
        if self.targets != ids || self.document != doc {
            self.cancel();
            self.origin = None;
            self.selected_nodes.clear();
            self.targets = ids.to_vec();
            self.document = doc;
            self.snap_cache = None;
        }
        let stale = self.drag.as_ref().is_some_and(|d| d.request.capture.check(s).is_err())
            || self.dialog.as_ref().is_some_and(|d| d.capture.check(s).is_err())
            || self.pending.as_ref().is_some_and(|p| p.capture.check(s).is_err())
            || self.envelope_request.as_ref().is_some_and(|p| p.transform.capture.check(s).is_err());
        if !enabled || escape || stale || self.kind != self.last_kind {
            self.cancel();
        }
        if !focused && self.drag.is_some() {
            self.drag = None;
            self.preview = None;
            self.drag_snap = None;
            self.guides.clear();
        }
        self.last_kind = self.kind;
    }
    fn bounds(&self, s: &Session, ids: &[LayerId]) -> Result<[f64; 4], String> {
        engine::bounds(&s.active().ok_or("no document")?.doc, ids).map_err(|e| e.to_string())
    }
    fn reference(&self, s: &Session, ids: &[LayerId]) -> Result<Point, String> {
        if let Some(p) = self.origin {
            return Ok(p);
        }
        let [x, y, w, h] = self.bounds(s, ids)?;
        Ok(Point::new(x + w / 2., y + h / 2.))
    }
    fn snap(&mut self, s: &Session, p: Point, view: snap::View, drag: bool) -> Result<Point, String> {
        let st = s.active().ok_or("no document")?;
        let cache = if drag { &mut self.drag_snap } else { &mut self.snap_cache };
        if cache.as_ref().is_none_or(|c| !c.is_current(st.doc.id, st.revision)) {
            *cache = Some(
                snap::Cache::build(
                    &st.doc,
                    st.revision,
                    snap::Exclusions { layers: if drag { self.targets.clone() } else { vec![] }, ..Default::default() },
                    snap::installed_geometry(),
                )
                .map_err(|e| e.to_string())?,
            );
        }
        let result =
            cache.as_ref().ok_or("snap cache unavailable")?.point([p.x, p.y], &self.preferences.snapping, view, None, false).map_err(|e| e.to_string())?;
        self.guides = result.guides;
        Ok(pt(result.point))
    }
    fn down(&mut self, s: &Session, ids: &[LayerId], p: Point, mods: Modifiers, view: snap::View) -> Result<(), String> {
        let capture = Capture::begin(s, ids).map_err(|e| e.to_string())?;
        let bounds = self.bounds(s, ids)?;
        let origin = self.reference(s, ids)?;
        let pick = self.snap(s, p, view, false)?;
        let tol = view.tolerance(8.).map_err(|e| e.to_string())?;
        let op = match self.kind {
            Kind::FreeTransform => {
                let hs = handles(bounds);
                let hit =
                    hs.iter().enumerate().map(|(i, h)| (i, distance(*h, p))).filter(|(_, d)| *d <= tol).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i);
                if let Some(i) = hit {
                    let mode = if mods.command && mods.alt && mods.shift {
                        FreeMode::Distort
                    } else if mods.command && i < 4 {
                        FreeMode::Perspective
                    } else {
                        self.preferences.free_mode
                    };
                    if mode != FreeMode::Free {
                        DragOp::Distort(i, mode)
                    } else if mods.command && i >= 4 {
                        DragOp::Shear(i)
                    } else {
                        DragOp::Scale(i)
                    }
                } else {
                    let [x, y, w, h] = bounds;
                    if p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h { DragOp::Move } else { DragOp::Rotate }
                }
            }
            Kind::Move => DragOp::Move,
            Kind::Reshape => {
                let doc = &s.active().ok_or("no document")?.doc;
                let mut best = None;
                for (layer, _, l) in doc.walk() {
                    let _ = layer;
                    if !ids.iter().any(|id| *id == l.id || doc.path_of(l.id).zip(doc.path_of(*id)).is_some_and(|(p, root)| p.starts_with(&root))) {
                        continue;
                    }
                    if let LayerContent::Shape(sh) = &l.content {
                        for (si, sub) in sh.path.subpaths.iter().enumerate() {
                            for (ki, k) in sub.knots.iter().enumerate() {
                                let d = distance(k.anchor, p);
                                if d <= tol && best.is_none_or(|(_, old)| d < old) {
                                    best = Some((Node { layer: l.id, subpath: si, knot: ki }, d));
                                }
                            }
                        }
                    }
                }
                let (node, _) = best.ok_or("click a selected path node to reshape")?;
                if mods.shift {
                    if let Some(i) = self.selected_nodes.iter().position(|n| *n == node) {
                        self.selected_nodes.remove(i);
                    } else {
                        self.selected_nodes.push(node);
                    }
                } else if !self.selected_nodes.contains(&node) {
                    self.selected_nodes = vec![node];
                }
                DragOp::Reshape
            }
            _ => DragOp::Reference,
        };
        self.drag = Some(Drag {
            request: Commit {
                capture,
                operation: Operation::Affine { matrix: [1., 0., 0., 1., 0., 0.] },
                options: self.preferences.options.clone(),
                copy: false,
                conversion: ConversionChoice::Ask,
            },
            start: p,
            pick,
            origin,
            bounds,
            op,
            moved: false,
            alt_press: mods.alt,
        });
        Ok(())
    }
    fn update(&mut self, s: &Session, p: Point, mods: Modifiers, view: snap::View) -> Result<(), String> {
        if self.drag.is_none() {
            return Ok(());
        }
        let p = self.snap(s, p, view, true)?;
        let d = self.drag.as_mut().ok_or("drag unavailable")?;
        if !d.moved && distance(d.start, p) < view.tolerance(3.).map_err(|e| e.to_string())? {
            return Ok(());
        }
        d.moved = true;
        let delta = [p.x - d.start.x, p.y - d.start.y];
        let axis_delta = if mods.shift { if delta[0].abs() >= delta[1].abs() { [delta[0], 0.] } else { [0., delta[1]] } } else { delta };
        let affine = |m| Operation::Affine { matrix: m };
        let operation = match d.op {
            DragOp::Move => affine([1., 0., 0., 1., axis_delta[0], axis_delta[1]]),
            DragOp::Reshape => Operation::Reshape { nodes: self.selected_nodes.clone(), delta: axis_delta, radius: self.preferences.reshape_radius },
            DragOp::Distort(i, mode) => {
                let mut q = corners(d.bounds);
                if i < 4 {
                    if mode == FreeMode::Perspective {
                        if delta[0].abs() >= delta[1].abs() {
                            q[i].x += delta[0];
                            q[[1, 0, 3, 2][i]].x -= delta[0];
                        } else {
                            q[i].y += delta[1];
                            q[[3, 2, 1, 0][i]].y -= delta[1];
                        }
                    } else {
                        q[i] = Point::new(q[i].x + axis_delta[0], q[i].y + axis_delta[1]);
                    }
                } else {
                    let (a, b) = match i {
                        4 => (0, 1),
                        5 => (1, 2),
                        6 => (2, 3),
                        _ => (3, 0),
                    };
                    for j in [a, b] {
                        q[j].x += axis_delta[0];
                        q[j].y += axis_delta[1];
                    }
                }
                Operation::Distort { source: d.bounds, quad: q.map(|p| [p.x, p.y]), perspective: mode == FreeMode::Perspective, tolerance: 0.25 }
            }
            DragOp::Rotate => affine(drag_matrix(Kind::Rotate, d.origin, d.pick, p, mods.shift, d.bounds)?),
            DragOp::Scale(i) => {
                let pivot = if mods.alt { Point::new(d.bounds[0] + d.bounds[2] / 2., d.bounds[1] + d.bounds[3] / 2.) } else { handles(d.bounds)[opposite(i)] };
                let mut m = drag_matrix(Kind::Scale, pivot, handles(d.bounds)[i], p, mods.shift, d.bounds)?;
                if i >= 4 && !mods.shift {
                    if i == 4 || i == 6 {
                        m = engine::scale(pivot, 1., m[3]).map_err(|e| e.to_string())?;
                    } else {
                        m = engine::scale(pivot, m[0], 1.).map_err(|e| e.to_string())?;
                    }
                }
                affine(m)
            }
            DragOp::Shear(i) => {
                let pivot = handles(d.bounds)[opposite(i)];
                let lever = if i == 4 || i == 6 { d.start.y - pivot.y } else { d.start.x - pivot.x };
                if lever.abs() < 1e-6 {
                    return Err("shear handle has no lever".into());
                }
                let k = if i == 4 || i == 6 { delta[0] / lever } else { delta[1] / lever };
                affine(engine::shear(pivot, -k.atan().to_degrees(), i == 5 || i == 7).map_err(|e| e.to_string())?)
            }
            DragOp::Reference => affine(drag_matrix(self.kind, d.origin, d.pick, p, mods.shift, d.bounds)?),
        };
        d.request.operation = operation;
        d.request.copy = mods.alt;
        d.request.options = self.preferences.options.clone();
        if matches!(d.request.operation, Operation::Distort { .. })
            && !engine::conversion_targets(&s.active().ok_or("no document")?.doc, &d.request.capture.layers).is_empty()
        {
            self.preview = None;
            return Ok(());
        }
        self.preview = Some(engine::prepare(s, &d.request).map_err(|e| e.to_string())?.document);
        self.generation = self.generation.wrapping_add(1) | (1 << 59);
        Ok(())
    }
    fn release(&mut self, s: &mut Session, ids: &[LayerId], mods: Modifiers) -> Result<(), String> {
        let Some(mut d) = self.drag.take() else {
            return Ok(());
        };
        self.preview = None;
        self.drag_snap = None;
        self.generation = self.generation.wrapping_add(1) | (1 << 59);
        if !d.moved {
            if matches!(d.op, DragOp::Reference) {
                self.origin = Some(d.pick);
                if d.alt_press {
                    self.open_dialog(s, ids)?;
                }
            }
            return Ok(());
        }
        d.request.copy = mods.alt;
        if matches!(d.request.operation, Operation::Distort { .. }) && !engine::conversion_targets(&s.active().ok_or("no document")?.doc, ids).is_empty() {
            self.pending = Some(d.request);
            return Ok(());
        }
        self.publish(s, &d.request)
    }
    fn publish(&mut self, s: &mut Session, c: &Commit) -> Result<(), String> {
        let ids = engine::commit(s, c).map_err(|e| e.to_string())?;
        self.targets = ids;
        self.document = s.active().map(|st| st.doc.id.0);
        self.snap_cache = None;
        self.drag = None;
        self.dialog = None;
        self.pending = None;
        self.preview = None;
        Ok(())
    }
    fn fail(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.cancel();
            self.error = Some(e);
        }
    }
    pub fn open_dialog(&mut self, s: &Session, ids: &[LayerId]) -> Result<(), String> {
        let capture = Capture::begin(s, ids).map_err(|e| e.to_string())?;
        let origin = self.reference(s, ids)?;
        let fields = if self.kind == Kind::Scale || self.kind == Kind::Each {
            ["100", "100", "0", "0", "0"]
        } else if self.kind == Kind::Reflect {
            ["90", "0", "0", "0", "0"]
        } else {
            ["0", "0", "0", "0", "0"]
        }
        .map(str::to_string);
        self.targets = ids.to_vec();
        self.document = s.active().map(|st| st.doc.id.0);
        self.last_kind = self.kind;
        self.drag = None;
        self.preview = None;
        self.dialog = Some(Dialog {
            capture,
            kind: self.kind,
            origin,
            fields,
            preview: true,
            vertical: false,
            proportional: false,
            reference: 4,
            random: false,
            seed: "1".into(),
            move_mode: MoveMode::Relative,
        });
        Ok(())
    }
    /// Call once per frame. Move's existing Enter route may call open_dialog directly.
    pub fn input(
        &mut self,
        s: &mut Session,
        ctx: &egui::Context,
        canvas: Rect,
        to_doc: impl Fn(Pos2) -> Point,
        ids: &[LayerId],
        enabled: bool,
        view: snap::View,
    ) {
        // This query reads the context too; keep it outside the input read lock.
        let wants = ctx.egui_wants_keyboard_input();
        let (focused, escape, events, mods) = ctx.input(|i| (i.focused, i.key_pressed(Key::Escape), i.events.clone(), i.modifiers));
        let escape = escape && crate::shortcut_dispatch::Focus::of(ctx) == crate::shortcut_dispatch::Focus::None;
        self.lifecycle(s, ids, enabled, focused, escape);
        if !enabled || !focused || escape {
            return;
        }
        let result = (|| {
            if !wants && ctx.input(|i| i.key_pressed(Key::D)) && (mods.ctrl || mods.command) {
                if mods.alt && mods.shift {
                    self.kind = Kind::Each;
                    self.last_kind = Kind::Each;
                    self.open_dialog(s, ids)?;
                } else {
                    let c = engine::again_request(s, ids).map_err(|e| e.to_string())?;
                    self.publish(s, &c)?;
                }
                return Ok(());
            }
            if !wants && self.pending.is_none() && self.envelope_request.is_none() && ctx.input(|i| i.key_pressed(Key::Enter)) {
                self.open_dialog(s, ids)?;
                return Ok(());
            }
            if !wants && !self.active() {
                let next = ctx.input(|i| {
                    if i.key_pressed(Key::R) {
                        Some(Kind::Rotate)
                    } else if i.key_pressed(Key::O) {
                        Some(Kind::Reflect)
                    } else if i.key_pressed(Key::S) {
                        Some(Kind::Scale)
                    } else if i.key_pressed(Key::E) {
                        Some(Kind::FreeTransform)
                    } else {
                        None
                    }
                });
                if let Some(next) = next {
                    self.kind = next;
                    self.last_kind = next;
                }
            }
            if self.dialog.is_some() || self.pending.is_some() || self.envelope_request.is_some() {
                return Ok(());
            }
            for event in events {
                match event {
                    Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers } if canvas.contains(pos) => {
                        self.down(s, ids, to_doc(pos), modifiers, view)?
                    }
                    Event::PointerMoved(pos) if self.drag.is_some() => self.update(s, to_doc(pos), mods, view)?,
                    Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers } if self.drag.is_some() => {
                        if self.drag.as_ref().is_some_and(|d| d.moved) {
                            self.update(s, to_doc(pos), modifiers, view)?;
                        }
                        self.release(s, ids, modifiers)?;
                    }
                    Event::PointerGone if self.drag.is_some() => self.cancel(),
                    _ => {}
                }
            }
            Ok(())
        })();
        self.fail(result);
    }
    pub fn context_bar(&mut self, ui: &mut Ui) {
        if self.kind == Kind::FreeTransform {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.preferences.free_mode, FreeMode::Free, t("Free Transform"));
                ui.selectable_value(&mut self.preferences.free_mode, FreeMode::Perspective, t("Perspective Distort"));
                ui.selectable_value(&mut self.preferences.free_mode, FreeMode::Distort, t("Free Distort"));
            });
        }
        if self.kind == Kind::Reshape {
            ui.label(t("Falloff radius"));
            ui.add(egui::DragValue::new(&mut self.preferences.reshape_radius).range(0.1..=1e6));
        }
    }
    pub fn paint(&self, s: &Session, ids: &[LayerId], painter: &Painter, to_view: impl Fn(Point) -> Pos2) {
        let cyan = Color32::from_rgb(30, 205, 230);
        let stroke = Stroke::new(1., cyan);
        if let Ok(o) = self.reference(s, ids) {
            let p = to_view(o);
            painter.circle_stroke(p, 5., stroke);
            painter.line_segment([p - egui::vec2(8., 0.), p + egui::vec2(8., 0.)], stroke);
            painter.line_segment([p - egui::vec2(0., 8.), p + egui::vec2(0., 8.)], stroke);
        }
        if self.kind == Kind::FreeTransform
            && let Ok(b) = self.bounds(s, ids)
        {
            let q =
                if let Some(Drag { request: Commit { operation: Operation::Distort { quad, .. }, .. }, .. }) = &self.drag { quad.map(pt) } else { corners(b) };
            for i in 0..4 {
                painter.line_segment([to_view(q[i]), to_view(q[(i + 1) % 4])], stroke);
            }
            for p in quad_handles(q) {
                painter.rect_filled(Rect::from_center_size(to_view(p), egui::vec2(6., 6.)), 0., cyan);
            }
        }
        for guide in &self.guides {
            painter.line_segment([to_view(pt(guide.a)), to_view(pt(guide.b))], stroke);
        }
        if self.kind == Kind::Reshape
            && let Some(st) = s.active()
        {
            for (_, _, l) in st.doc.walk() {
                if let LayerContent::Shape(sh) = &l.content {
                    for (si, sub) in sh.path.subpaths.iter().enumerate() {
                        for (ki, k) in sub.knots.iter().enumerate() {
                            if st
                                .doc
                                .path_of(l.id)
                                .is_some_and(|path| (1..=path.len()).any(|n| st.doc.layer_at(&path[..n]).is_some_and(|ancestor| ids.contains(&ancestor.id))))
                            {
                                let selected = self.selected_nodes.contains(&Node { layer: l.id, subpath: si, knot: ki });
                                painter.circle_filled(to_view(k.anchor), 3., if selected { cyan } else { Color32::WHITE });
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Pinned VectorCraft reference-point drag formulas; Shift quantizes to 45°/uniform.
pub fn drag_matrix(kind: Kind, o: Point, start: Point, p: Point, shift: bool, b: [f64; 4]) -> Result<[f64; 6], String> {
    let result = match kind {
        Kind::Rotate => {
            let mut a = (p.y - o.y).atan2(p.x - o.x) - (start.y - o.y).atan2(start.x - o.x);
            if shift {
                a = (a / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
            }
            engine::rotate(o, -a.to_degrees())
        }
        Kind::Reflect => {
            let mut a = (p.y - o.y).atan2(p.x - o.x);
            if shift {
                a = (a / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
            }
            engine::reflect(o, -a.to_degrees())
        }
        Kind::Scale => {
            let (x, y) = (start.x - o.x, start.y - o.y);
            let (u, v) = (p.x - o.x, p.y - o.y);
            let (mut sx, mut sy) = (if x.abs() > 1e-6 { u / x } else { 1. }, if y.abs() > 1e-6 { v / y } else { 1. });
            if shift {
                let sq = x * x + y * y;
                let k = if sq > 1e-12 { (u * x + v * y) / sq } else { 1. };
                sx = k;
                sy = k;
            }
            let clamp = |v: f64| if v.abs() < 1e-4 { 1e-4f64.copysign(v) } else { v };
            engine::scale(o, clamp(sx), clamp(sy))
        }
        Kind::Shear => {
            let (x, y) = (p.x - start.x, p.y - start.y);
            let horizontal = x.abs() >= y.abs();
            let lever = if horizontal { start.y - o.y } else { start.x - o.x };
            let fallback = if horizontal { b[3] } else { b[2] } / 2.;
            let lever = if lever.abs() > 1e-3 { lever } else { fallback.max(1.) };
            let k = if horizontal { x / lever } else { y / lever };
            engine::shear(o, -k.atan().to_degrees(), !horizontal)
        }
        _ => return Err("tool has no reference-point transform".into()),
    };
    result.map_err(|e| e.to_string())
}
#[path = "transform_tools/dialogs.rs"]
mod dialogs;
#[cfg(test)]
#[path = "transform_tools/tests.rs"]
mod tests;
