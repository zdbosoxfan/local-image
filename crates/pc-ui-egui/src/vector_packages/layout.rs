//! Original V8 layout widgets and captured tools. Router registration lives outside this
//! module; interaction tests compile the same source.
use egui::{Context, Key, Modifiers, Pos2, Ui};
use photocraft_doc::vector_design::structure::DesignUnits;
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_engine::vector_packages::{
    constraints::arithmetic::{self as math, Box2},
    layout as engine,
    snapping::query::{Cache, Exclusions, Settings, View},
};
use photocraft_engine::{EngineError, Result, Session};
use photocraft_geom::{Affine, Point};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    None,
    Artboard,
    PointTransform,
    Crop,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerMap {
    pub origin: Pos2,
    pub document_origin: Point,
    pub dx: Point,
    pub dy: Point,
    pub view: View,
}
impl PointerMap {
    pub fn point(self, p: Pos2) -> Point {
        let delta = p - self.origin;
        Point::new(
            self.document_origin.x + self.dx.x * f64::from(delta.x) + self.dy.x * f64::from(delta.y),
            self.document_origin.y + self.dx.y * f64::from(delta.x) + self.dy.y * f64::from(delta.y),
        )
    }
}
#[derive(Clone)]
struct Drag {
    request: engine::Commit,
    start: Point,
    last: Point,
    original: Box2,
    handle: Option<usize>,
    pivot: Point,
    node: Point,
    tool: Tool,
    snap: Cache,
}
impl std::fmt::Debug for Drag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drag").field("request", &self.request).field("tool", &self.tool).finish_non_exhaustive()
    }
}
impl PartialEq for Drag {
    fn eq(&self, other: &Self) -> bool {
        self.request == other.request && self.start == other.start && self.last == other.last && self.tool == other.tool
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub tool: Tool,
    pub open: bool,
    pub pointer_map: Option<PointerMap>,
    pub display_cache: Option<(u64, u64, u64, Arc<Document>, Arc<Document>)>,
    pub anchor: usize,
    pub pivot: Option<Point>,
    pub key: Option<LayerId>,
    pub spacing: f64,
    pub automatic_spacing: bool,
    pub values: [f64; 6],
    pub preview: Option<Arc<Document>>,
    pub generation: u64,
    pub snapping: Settings,
    pub pixels_per_point: f64,
    pub host_tool: Option<&'static str>,
    drag: Option<Drag>,
    panel: Option<engine::Capture>,
    panel_bounds: Box2,
    panel_units: DesignUnits,
    synchronized: Option<(u64, u64, Vec<LayerId>)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            tool: Tool::None,
            open: false,
            pointer_map: None,
            display_cache: None,
            anchor: 4,
            pivot: None,
            key: None,
            spacing: 10.,
            automatic_spacing: false,
            values: [0.; 6],
            preview: None,
            generation: 0,
            snapping: Settings::default(),
            pixels_per_point: 1.,
            host_tool: None,
            drag: None,
            panel: None,
            panel_bounds: [0., 0., 1., 1.],
            panel_units: DesignUnits::Pixels,
            synchronized: None,
        }
    }
}
pub fn selected(s: &Session) -> Vec<LayerId> {
    s.active().map(|s| s.selected_layers()).unwrap_or_default()
}
pub fn anchor_point(b: Box2, index: usize) -> Point {
    Point::new(b[0] + b[2] * (index % 3) as f64 / 2., b[1] + b[3] * (index / 3) as f64 / 2.)
}
fn path_nodes(doc: &Document, ids: &[LayerId]) -> Vec<Point> {
    fn collect(layer: &photocraft_doc::Layer, points: &mut Vec<Point>) {
        match &layer.content {
            LayerContent::Shape(s) => points.extend(s.path.subpaths.iter().flat_map(|s| &s.knots).map(|k| k.anchor)),
            LayerContent::Group(g) => {
                for child in &g.children {
                    collect(child, points);
                }
            }
            _ => {}
        }
    }
    let mut points = Vec::new();
    for id in ids {
        if let Some(layer) = doc.layer(*id) {
            collect(layer, &mut points);
        }
    }
    points
}
fn crop_box(doc: &Document, ids: &[LayerId]) -> Result<Box2> {
    if ids.len() == 1
        && let Some(p) = doc.layer(ids[0]).and_then(|l| l.vector.clip.as_ref())
        && let Some((x, y, x1, y1)) = p.control_bounds()
    {
        return math::validate_rect([x, y, x1 - x, y1 - y]);
    }
    engine::selection_bounds(doc, ids)
}
fn panel_operation(doc: &Document, layers: &[LayerId], matrix: Affine) -> Result<engine::Operation> {
    if let [id] = layers
        && let Some(layer) = doc.layer(*id)
        && layer.artboard().is_some()
    {
        math::parent_basis(matrix)?;
        let b = engine::bounds(layer)?;
        let p = matrix.apply(Point::new(b[0], b[1]));
        return Ok(engine::Operation::Resize { rect: [p.x, p.y, b[2] * matrix.m[0], b[3] * matrix.m[3]], basis: Affine::IDENTITY });
    }
    Ok(engine::Operation::Transform { matrix })
}
impl State {
    pub fn is_fresh(&self, s: &Session) -> bool {
        let Some(st) = s.active() else { return false };
        let captured = self.drag.as_ref().map(|d| &d.request.capture).or(self.panel.as_ref());
        captured.is_some_and(|c| c.document == st.doc.id.0 && c.revision == st.revision)
    }
    pub fn captured(&self) -> bool {
        self.drag.is_some() || self.panel.is_some()
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.pointer_map = None;
        self.panel = None;
        self.preview = None;
        self.synchronized = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn switch(&mut self, tool: Tool) {
        if tool != self.tool {
            self.cancel();
            self.tool = tool;
        }
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) -> Result<()> {
        if !enabled {
            self.cancel();
            self.tool = Tool::None;
            return Ok(());
        }
        let Some(st) = s.active() else {
            self.cancel();
            return Ok(());
        };
        let ids = selected(s);
        let key = (st.doc.id.0, st.revision, ids.clone());
        let stale = self.drag.as_ref().is_some_and(|d| {
            d.request.capture.document != st.doc.id.0
                || d.request.capture.revision != st.revision
                || (d.tool != Tool::Artboard && d.request.capture.layers != ids)
        }) || self.panel.as_ref().is_some_and(|c| c.document != st.doc.id.0 || c.revision != st.revision || c.layers != ids);
        if stale {
            self.cancel();
        }
        if self.key.is_some_and(|id| !ids.contains(&id)) {
            self.key = None;
        }
        if self.captured() || self.synchronized.as_ref() == Some(&key) {
            return Ok(());
        }
        if let Ok(b) = engine::selection_bounds(&st.doc, &ids) {
            let p = anchor_point(b, self.anchor);
            let dpi = f64::from(st.doc.resolution_dpi);
            self.values = [
                engine::from_pixels(p.x, st.doc.design_units, dpi)?,
                engine::from_pixels(p.y, st.doc.design_units, dpi)?,
                engine::from_pixels(b[2], st.doc.design_units, dpi)?,
                engine::from_pixels(b[3], st.doc.design_units, dpi)?,
                0.,
                0.,
            ];
            self.panel_bounds = b;
            self.panel_units = st.doc.design_units;
        }
        self.synchronized = Some(key);
        Ok(())
    }
    fn show(&mut self, s: &Session, request: &engine::Commit) -> Result<()> {
        let doc = engine::preview(s, request)?;
        if self.preview.as_deref() != Some(&doc) {
            self.preview = Some(Arc::new(doc));
            self.generation = self.generation.wrapping_add(1);
        }
        Ok(())
    }
    pub fn down(&mut self, s: &Session, p: Point, tolerance: f64, view: View) -> Result<bool> {
        self.synchronize(s, true)?;
        if self.tool == Tool::None {
            return Ok(false);
        }
        self.cancel();
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let ids = if self.tool == Tool::Artboard {
            // Hit an existing artboard's handle or interior; empty pasteboard draws a new one.
            st.doc
                .artboards()
                .iter()
                .rev()
                .find(|(_, _, a)| {
                    let b = engine::box_from_rect(a.rect);
                    p.x >= b[0] - tolerance && p.x <= b[0] + b[2] + tolerance && p.y >= b[1] - tolerance && p.y <= b[1] + b[3] + tolerance
                })
                .map(|(id, _, _)| vec![*id])
                .unwrap_or_default()
        } else {
            selected(s)
        };
        let capture = if ids.is_empty() && self.tool == Tool::Artboard {
            engine::Capture { document: st.doc.id.0, revision: st.revision, layers: vec![] }
        } else {
            engine::Capture::begin(s, ids.clone())?
        };
        let original = if ids.is_empty() {
            [p.x, p.y, 1., 1.]
        } else if self.tool == Tool::Crop {
            crop_box(&st.doc, &ids)?
        } else {
            engine::selection_bounds(&st.doc, &ids)?
        };
        let handle = (0..9)
            .filter(|i| *i != 4)
            .filter_map(|i| {
                let q = anchor_point(original, i);
                let distance = (q.x - p.x).hypot(q.y - p.y);
                (distance <= tolerance).then_some((i, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        let pivot = self.pivot.unwrap_or_else(|| anchor_point(original, self.anchor));
        let node = if self.tool == Tool::PointTransform {
            let node = path_nodes(&st.doc, &ids)
                .into_iter()
                .map(|q| (q, (q.x - p.x).hypot(q.y - p.y)))
                .filter(|(_, d)| *d <= tolerance)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(q, _)| q);
            let Some(node) = node else {
                self.pivot = Some(p);
                return Ok(true);
            };
            node
        } else {
            p
        };
        let operation = match self.tool {
            Tool::Artboard if ids.is_empty() => engine::Operation::Artboard { rect: original },
            Tool::Artboard => engine::Operation::Resize { rect: original, basis: Affine::IDENTITY },
            Tool::Crop => engine::Operation::Crop { rect: Some(original) },
            _ => engine::Operation::Transform { matrix: Affine::IDENTITY },
        };
        let snap = Cache::build(
            &st.doc,
            st.revision,
            Exclusions { layers: ids, ..Default::default() },
            photocraft_engine::vector_packages::snapping::query::installed_geometry(),
        )?;
        view.tolerance(self.snapping.tolerance_px)?;
        self.drag = Some(Drag { request: engine::Commit { capture, operation }, start: p, last: p, original, handle, pivot, node, tool: self.tool, snap });
        Ok(true)
    }
    pub fn motion(&mut self, s: &Session, p: Point, mods: Modifiers, view: View) -> Result<()> {
        self.synchronize(s, true)?;
        let Some(d) = &self.drag else { return Ok(()) };
        if p == d.start && p == d.last {
            return Ok(());
        }
        let snap = d.snap.point([p.x, p.y], &self.snapping, view, None, mods.ctrl || mods.command || mods.mac_cmd)?;
        let point = Point::new(snap.point[0], snap.point[1]);
        let operation = match d.tool {
            Tool::PointTransform => engine::Operation::Transform { matrix: engine::point_transform(d.pivot, d.node, point, mods.shift, mods.alt)? },
            Tool::Artboard if d.request.capture.layers.is_empty() => engine::Operation::Artboard { rect: drag_rect(d.start, point, mods.shift, mods.alt)? },
            Tool::Artboard => engine::Operation::Resize { rect: handle_rect(d.original, d.handle, d.start, point, mods)?, basis: Affine::IDENTITY },
            Tool::Crop if d.handle.is_none() => engine::Operation::ContentMove { matrix: Affine::translate(point.x - d.start.x, point.y - d.start.y) },
            Tool::Crop => engine::Operation::Crop { rect: Some(handle_rect(d.original, d.handle, d.start, point, mods)?) },
            Tool::None => return Ok(()),
        };
        let mut request = d.request.clone();
        request.operation = operation;
        self.show(s, &request)?;
        if let Some(d) = &mut self.drag {
            d.request = request;
            d.last = p;
        }
        Ok(())
    }
    pub fn up(&mut self, s: &mut Session, p: Point, mods: Modifiers, view: View) -> Result<()> {
        if self.drag.is_none() {
            return Ok(());
        }
        let moved = self.drag.as_ref().is_some_and(|d| d.start != p);
        let result = if moved {
            self.motion(s, p, mods, view).and_then(|_| {
                let request = self.drag.as_ref().ok_or_else(|| math::error("layout capture lost"))?.request.clone();
                s.execute("vector.layout.apply", json!(request)).map(|_| ())
            })
        } else {
            Ok(())
        };
        self.cancel();
        result
    }
    pub fn panel_preview(&mut self, s: &Session) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if self.panel.is_none() {
            self.panel = Some(engine::Capture::begin(s, selected(s))?);
        }
        let b = self.panel_bounds;
        let units = self.panel_units;
        let dpi = f64::from(st.doc.resolution_dpi);
        let pivot = self.pivot.unwrap_or_else(|| anchor_point(b, self.anchor));
        let [x, y, w, h, r, shear] = self.values;
        let position = anchor_point(b, self.anchor);
        let matrix = engine::anchored(
            pivot,
            Point::new(engine::to_pixels(x, units, dpi)? - position.x, engine::to_pixels(y, units, dpi)? - position.y),
            [engine::to_pixels(w, units, dpi)? / b[2], engine::to_pixels(h, units, dpi)? / b[3]],
            r,
            shear,
        )?;
        let capture = self.panel.clone().ok_or_else(|| math::error("no transform capture"))?;
        let operation = panel_operation(&st.doc, &capture.layers, matrix)?;
        let request = engine::Commit { capture, operation };
        self.show(s, &request)
    }
    pub fn panel_commit(&mut self, s: &mut Session) -> Result<()> {
        let Some(capture) = self.panel.clone() else { return Ok(()) };
        self.panel_preview(s)?;
        // Publication repeats the matrix from captured values through the journaled command.
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let b = self.panel_bounds;
        let dpi = f64::from(st.doc.resolution_dpi);
        let q = anchor_point(b, self.anchor);
        let pivot = self.pivot.unwrap_or(q);
        let v = self.values;
        let u = self.panel_units;
        let matrix = engine::anchored(
            pivot,
            Point::new(engine::to_pixels(v[0], u, dpi)? - q.x, engine::to_pixels(v[1], u, dpi)? - q.y),
            [engine::to_pixels(v[2], u, dpi)? / b[2], engine::to_pixels(v[3], u, dpi)? / b[3]],
            v[4],
            v[5],
        )?;
        let operation = panel_operation(&st.doc, &capture.layers, matrix)?;
        let result = s.execute("vector.layout.apply", json!(engine::Commit { capture, operation })).map(|_| ());
        self.cancel();
        result
    }
}
fn drag_rect(a: Point, b: Point, square: bool, center: bool) -> Result<Box2> {
    let mut dx = b.x - a.x;
    let mut dy = b.y - a.y;
    if square {
        let side = dx.abs().max(dy.abs());
        dx = dx.signum() * side;
        dy = dy.signum() * side;
    }
    let r = if center { [a.x - dx.abs(), a.y - dy.abs(), dx.abs() * 2., dy.abs() * 2.] } else { [a.x.min(a.x + dx), a.y.min(a.y + dy), dx.abs(), dy.abs()] };
    math::validate_rect(r)
}
fn handle_rect(b: Box2, handle: Option<usize>, start: Point, p: Point, mods: Modifiers) -> Result<Box2> {
    let Some(index) = handle else { return math::validate_rect([b[0] + p.x - start.x, b[1] + p.y - start.y, b[2], b[3]]) };
    let opposite = anchor_point(b, 8 - index);
    let mut x0 = b[0];
    let mut y0 = b[1];
    let mut x1 = b[0] + b[2];
    let mut y1 = b[1] + b[3];
    if index % 3 == 0 {
        x0 = p.x.min(opposite.x - math::MIN_SIZE);
    } else if index % 3 == 2 {
        x1 = p.x.max(opposite.x + math::MIN_SIZE);
    }
    if index / 3 == 0 {
        y0 = p.y.min(opposite.y - math::MIN_SIZE);
    } else if index / 3 == 2 {
        y1 = p.y.max(opposite.y + math::MIN_SIZE);
    }
    if mods.alt {
        let c = anchor_point(b, 4);
        if index % 3 != 1 {
            let half = (p.x - c.x).abs().max(math::MIN_SIZE / 2.);
            x0 = c.x - half;
            x1 = c.x + half;
        }
        if index / 3 != 1 {
            let half = (p.y - c.y).abs().max(math::MIN_SIZE / 2.);
            y0 = c.y - half;
            y1 = c.y + half;
        }
    }
    if mods.shift {
        let ratio = b[2] / b[3];
        let w = x1 - x0;
        let h = y1 - y0;
        let (w, h) = if w / h > ratio { (w, w / ratio) } else { (h * ratio, h) };
        let anchor = if mods.alt { anchor_point(b, 4) } else { opposite };
        let ix = if mods.alt { 1 } else { (8 - index) % 3 };
        let iy = if mods.alt { 1 } else { (8 - index) / 3 };
        x0 = anchor.x - w * ix as f64 / 2.;
        y0 = anchor.y - h * iy as f64 / 2.;
        x1 = x0 + w;
        y1 = y0 + h;
    }
    math::validate_rect([x0, y0, x1 - x0, y1 - y0])
}
/// Widget host bridge: true pointer presses are captured before egui's drag threshold;
/// releases outside the original rectangle still finish through the engine.
pub fn input(state: &mut State, s: &mut Session, ctx: &Context, rect: egui::Rect, to_doc: impl Fn(Pos2) -> Point, view: View, enabled: bool) -> Result<()> {
    state.synchronize(s, enabled)?;
    let (pressed, released, down, pos, mods, cancel) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.primary_down(),
            i.pointer.latest_pos(),
            i.modifiers,
            !i.focused || i.key_pressed(Key::Escape) || i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false) | egui::Event::PointerGone)),
        )
    });
    if cancel {
        state.cancel();
        return Ok(());
    }
    if let Some(pos) = pos {
        let p = state.pointer_map.map_or_else(|| to_doc(pos), |map| map.point(pos));
        let view = state.pointer_map.map_or(view, |map| map.view);
        if pressed && rect.contains(pos) {
            state.down(s, p, view.tolerance(6.)?, view)?;
            if state.captured() {
                let x = to_doc(pos + egui::vec2(1., 0.));
                let y = to_doc(pos + egui::vec2(0., 1.));
                state.pointer_map =
                    Some(PointerMap { origin: pos, document_origin: p, dx: Point::new(x.x - p.x, x.y - p.y), dy: Point::new(y.x - p.x, y.y - p.y), view });
            }
        }
        if released {
            state.up(s, p, mods, view)?;
        } else if down && state.drag.is_some() && !pressed {
            state.motion(s, p, mods, view)?;
        }
    }
    if !down && !released && state.drag.is_some() {
        state.cancel();
    }
    Ok(())
}
const ANCHORS: [&str; 9] = [
    "Anchor top left",
    "Anchor top center",
    "Anchor top right",
    "Anchor middle left",
    "Anchor center",
    "Anchor middle right",
    "Anchor bottom left",
    "Anchor bottom center",
    "Anchor bottom right",
];
/// English-source lookup matches existing catalog convention. Every string is in i18n/16.tsv.
pub fn panel(ui: &mut Ui, state: &mut State, s: &mut Session, text: impl Fn(&str) -> String) -> Result<()> {
    panel_section(ui, state, s, None, text)
}

pub fn panel_section(ui: &mut Ui, state: &mut State, s: &mut Session, section: Option<crate::dock::Tab>, text: impl Fn(&str) -> String) -> Result<()> {
    state.synchronize(s, true)?;
    if section.is_none() || section == Some(crate::dock::Tab::Transform) {
        ui.horizontal_wrapped(|ui| {
            for (tool, label) in [
                (Tool::None, "Transform"),
                (Tool::Artboard, "Vector Artboard tool"),
                (Tool::PointTransform, "Point Transform tool"),
                (Tool::Crop, "Vector Crop tool"),
            ] {
                if ui.selectable_label(state.tool == tool, text(label)).clicked() {
                    state.switch(tool);
                }
            }
        });
    }
    let ids = selected(s);
    let editable = s.active().is_some_and(|st| engine::preflight(&st.doc, &ids).is_ok());
    let mut result = Ok(());
    ui.add_enabled_ui(editable, |ui| {
        if section.is_none() || section == Some(crate::dock::Tab::Transform) {
            egui::Grid::new("layout anchors").show(ui, |ui| {
                for (i, label) in ANCHORS.iter().enumerate() {
                    if ui.selectable_label(state.anchor == i, text(label)).clicked() {
                        state.cancel();
                        state.anchor = i;
                        state.pivot = None;
                    }
                    if i % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
            if let Err(e) = state.synchronize(s, true) {
                result = Err(e);
            }
            let mut changed = false;
            let mut commit = false;
            for (i, label) in ["X", "Y", "Width", "Height", "Rotation change (degrees)", "Shear change (degrees)"].iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    let label = ui.label(text(label));
                    let range = if i == 5 {
                        -88.99..=88.99
                    } else if i == 2 || i == 3 {
                        math::MIN_SIZE..=math::LIMIT
                    } else {
                        -math::LIMIT..=math::LIMIT
                    };
                    let field_width = ui.spacing().interact_size.x * 3.0;
                    let response = crate::widgets::value_field_with_precision(ui, &mut state.values[i], range, "", field_width, 2).labelled_by(label.id);
                    changed |= response.changed();
                    commit |= response.drag_stopped() || (response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                });
            }
            if changed {
                result = state.panel_preview(s);
            }
            if commit && result.is_ok() {
                result = state.panel_commit(s);
            }
            ui.horizontal(|ui| {
                if crate::widgets::secondary_button(ui, &text("Apply transform"), 0.0).clicked() {
                    result = state.panel_commit(s);
                }
                if crate::widgets::secondary_button(ui, &text("Cancel transform"), 0.0).clicked() {
                    state.cancel();
                }
            });
        }
        if section.is_none() || section == Some(crate::dock::Tab::Align) {
            crate::widgets::hairline(ui);
            ui.horizontal_wrapped(|ui| {
                ui.label(text("Key object"));
                if ui.selectable_label(state.key.is_none(), text("Selection bounds")).clicked() {
                    state.key = None;
                }
                if let Some(st) = s.active() {
                    for id in &ids {
                        if let Some(l) = st.doc.layer(*id)
                            && ui.selectable_label(state.key == Some(*id), &l.name).clicked()
                        {
                            state.key = Some(*id);
                        }
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (edge, label) in [
                    ("left", "Align left"),
                    ("centerX", "Align horizontal centers"),
                    ("right", "Align right"),
                    ("top", "Align top"),
                    ("centerY", "Align vertical centers"),
                    ("bottom", "Align bottom"),
                ] {
                    if crate::widgets::secondary_button(ui, &text(label), 0.0).clicked() {
                        result = run_layout(s, state, "vector.layout.align", json!({"edge":edge}));
                    }
                }
            });
            crate::widgets::checkbox(ui, &mut state.automatic_spacing, &text("Automatic spacing"));
            ui.horizontal_wrapped(|ui| {
                let label = ui.label(text("Spacing"));
                let field_width = ui.spacing().interact_size.x * 3.0;
                crate::widgets::value_field_with_precision(ui, &mut state.spacing, -math::LIMIT..=math::LIMIT, "", field_width, 2).labelled_by(label.id);
                for (axis, label) in [("x", "Distribute horizontal spacing"), ("y", "Distribute vertical spacing")] {
                    if crate::widgets::secondary_button(ui, &text(label), 0.0).clicked() {
                        let mut params = json!({"axis":axis});
                        if !state.automatic_spacing {
                            match s
                                .active()
                                .ok_or(EngineError::NoDocument)
                                .and_then(|st| engine::to_pixels(state.spacing, st.doc.design_units, f64::from(st.doc.resolution_dpi)))
                            {
                                Ok(gap) => params["spacing"] = json!(gap),
                                Err(e) => {
                                    result = Err(e);
                                    continue;
                                }
                            }
                        }
                        result = run_layout(s, state, "vector.layout.spacing", params);
                    }
                }
            });
        }
        if section.is_none() || section == Some(crate::dock::Tab::Layout) {
            crate::widgets::hairline(ui);
            let mut c = s.active().and_then(|st| ids.first().and_then(|id| st.doc.layer(*id))).and_then(|l| l.vector.constraints).unwrap_or_default();
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for (label, value) in [
                    ("Pin left", &mut c.left),
                    ("Pin right", &mut c.right),
                    ("Pin top", &mut c.top),
                    ("Pin bottom", &mut c.bottom),
                    ("Fixed width", &mut c.fixed_width),
                    ("Fixed height", &mut c.fixed_height),
                ] {
                    changed |= crate::widgets::checkbox(ui, value, &text(label)).changed();
                }
            });
            if changed {
                result = engine::Capture::begin(s, ids.clone())
                    .and_then(|capture| s.execute("vector.constraints.set", json!({"capture":capture,"constraints":c})).map(|_| ()));
                state.cancel();
            }
            if crate::widgets::secondary_button(ui, &text("Clear vector crop"), 0.0).clicked() {
                result = engine::Capture::begin(s, ids.clone()).and_then(|capture| {
                    s.execute("vector.layout.apply", json!(engine::Commit { capture, operation: engine::Operation::Crop { rect: None } })).map(|_| ())
                });
                state.cancel();
            }
        }
    });
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        state.cancel();
    }
    result
}
fn run_layout(s: &mut Session, state: &mut State, command: &str, mut params: serde_json::Value) -> Result<()> {
    state.cancel();
    let capture = engine::Capture::begin(s, selected(s))?;
    params["capture"] = json!(capture);
    if let Some(key) = state.key {
        params["key"] = json!(key.0);
    }
    s.execute(command, params).map(|_| ())
}
pub fn overlay(state: &State, s: &Session, painter: &egui::Painter, to_screen: impl Fn(Point) -> Pos2) {
    let doc = state.preview.as_deref().or_else(|| s.active().map(|st| st.doc.as_ref()));
    let Some(doc) = doc else { return };
    let ids = selected(s);
    let bounds = if let Some(d) = &state.drag {
        match d.request.operation {
            engine::Operation::Crop { rect: Some(b) } | engine::Operation::Resize { rect: b, .. } | engine::Operation::Artboard { rect: b } => Ok(b),
            _ => engine::selection_bounds(doc, &ids),
        }
    } else {
        crop_box(doc, &ids)
    };
    if state.tool == Tool::None {
        return;
    }
    if let Ok(b) = bounds {
        let color = egui::Color32::from_rgb(70, 160, 255);
        let points = [0, 2, 8, 6, 0].map(|i| to_screen(anchor_point(b, i)));
        for pair in points.windows(2) {
            painter.line_segment([pair[0], pair[1]], egui::Stroke::new(1., color));
        }
        if state.tool == Tool::PointTransform {
            for node in path_nodes(doc, &ids) {
                painter.circle_filled(to_screen(node), 3., color);
            }
        } else {
            for i in [0, 1, 2, 3, 5, 6, 7, 8] {
                painter.circle_filled(to_screen(anchor_point(b, i)), 3., color);
            }
        }
        if state.tool == Tool::PointTransform {
            painter.circle_stroke(to_screen(state.pivot.unwrap_or_else(|| anchor_point(b, state.anchor))), 5., egui::Stroke::new(1., color));
        }
    }
}
