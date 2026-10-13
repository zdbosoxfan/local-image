//! Package 02 live-shape input/preview/context hooks; registered by packages 00/01.
//! Modifier/capture behavior adapted from VectorCraft crates/tools/src/shape.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092. Copyright (c) 2026 ArtCraft Team
//! and the VectorCraft contributors. MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
use egui::{Color32, Pos2, Sense, Stroke};
use photocraft_doc::vector_design::drawing::{CornerKind, DesignShape, ShapeParameters};
use photocraft_doc::{DesignUnits, Layer, LayerContent, LayerId};
use photocraft_engine::vector_packages::drawing::{
    self as engine, Capture,
    geometry::{self, HandleKind},
};
use photocraft_engine::{EngineError, Result, Session};
use photocraft_geom::Point;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub origin: Pos2,
    pub scale: f64,
}
impl View {
    pub fn document(self, p: Pos2) -> Point {
        Point::new(f64::from(p.x - self.origin.x) / self.scale, f64::from(p.y - self.origin.y) / self.scale)
    }
    pub fn screen(self, p: Point) -> Pos2 {
        Pos2::new(self.origin.x + (p.x * self.scale) as f32, self.origin.y + (p.y * self.scale) as f32)
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Gesture {
    capture: Capture,
    targets: Vec<LayerId>,
    above: Option<LayerId>,
    start: Point,
    original: DesignShape,
    handle: Option<HandleKind>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct State {
    pub parameters: ShapeParameters,
    pub angle: f64,
    #[serde(skip)]
    pub paint: Option<(Option<photocraft_doc::Fill>, Option<photocraft_doc::ShapeStroke>)>,
    #[serde(skip)]
    pub generation: u64,
    #[serde(skip)]
    pub preview: Option<DesignShape>,
    #[serde(skip)]
    pub preview_layer: Option<Layer>,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    gesture: Option<Gesture>,
    #[serde(skip)]
    numeric: Option<(Capture, LayerId, DesignShape)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            parameters: ShapeParameters::Rect { radii: [0.; 4], kinds: [CornerKind::Round; 4] },
            angle: 0.,
            paint: None,
            generation: 0,
            preview: None,
            preview_layer: None,
            error: None,
            gesture: None,
            numeric: None,
        }
    }
}
impl State {
    pub fn is_active(&self) -> bool {
        self.gesture.is_some() || self.numeric.is_some()
    }
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.numeric = None;
        self.preview = None;
        self.preview_layer = None;
        self.error = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn begin_new(&mut self, s: &Session, start: Point, above: Option<LayerId>) -> Result<()> {
        self.cancel();
        let capture = Capture::new(s)?;
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        if let Some(id) = above {
            if doc.layer(id).is_none() {
                return Err(EngineError::NoLayer(id));
            }
            let locks = doc.effective_locks(id);
            if locks.all || locks.pixels {
                return Err(EngineError::Other("creation target is locked".into()));
            }
        }
        let original = DesignShape { rect: [start.x, start.y, 1., 1.], angle: self.angle, parameters: self.parameters.clone() };
        self.gesture = Some(Gesture { capture, targets: Vec::new(), above, start, original, handle: None });
        Ok(())
    }
    pub fn begin_handle(&mut self, s: &Session, id: LayerId, handle: HandleKind) -> Result<()> {
        self.cancel();
        let capture = Capture::new(s)?;
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        let edits = photocraft_engine::vector_api::snapshot_shapes(doc, &[id])?;
        let original = edits[0].vector.live_shape.clone().ok_or_else(|| EngineError::Other("select a live design shape".into()))?;
        if geometry::generate(&original).map_err(|e| EngineError::Other(e.to_string()))? != edits[0].shape.path {
            return Err(EngineError::Other("live parameters no longer match the base path".into()));
        }
        self.gesture = Some(Gesture { capture, targets: vec![id], above: None, start: Point::new(0., 0.), original, handle: Some(handle) });
        Ok(())
    }
    /// Snap provider 07 supplies a point satisfying the current Shift constraint. Sample modifiers each frame.
    pub fn update(&mut self, s: &Session, p: Point, mods: egui::Modifiers, snap: impl Fn(Point, Point, bool) -> Point) -> Result<()> {
        self.update_checked(s, p, mods, |p, start, shift| Ok(snap(p, start, shift)))
    }
    /// Fallible provider queries abort before a preview or commit can be published.
    pub fn update_checked(&mut self, s: &Session, p: Point, mods: egui::Modifiers, snap: impl Fn(Point, Point, bool) -> Result<Point>) -> Result<()> {
        let Some(g) = self.gesture.as_ref() else { return Ok(()) };
        g.capture.check(s)?;
        let candidate = if let Some(handle) = g.handle {
            geometry::drag_handle(&g.original, handle, snap(p, p, false)?).map_err(|e| EngineError::Other(e.to_string()))?
        } else {
            let q = constrained_endpoint(&g.original, g.start, p, mods.shift);
            design_between(&g.original, g.start, snap(q, g.start, mods.shift)?, mods.alt)
        };
        if g.handle.is_none() && (candidate.rect[2] < 1e-6 || candidate.rect[3] < 1e-6 || (candidate.rect[2] <= 1e-6 && candidate.rect[3] <= 1e-6)) {
            // A modifier-only event can arrive before the first motion. Keep capture,
            // but remove any old geometry so a zero-size release cannot commit it.
            self.preview = None;
            self.preview_layer = None;
            self.generation = self.generation.wrapping_add(1);
            return Ok(());
        }
        let targets = g.targets.clone();
        self.prepare_preview(s, candidate, &targets)
    }
    fn prepare_preview(&mut self, s: &Session, candidate: DesignShape, targets: &[LayerId]) -> Result<()> {
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        let mut layer = if let Some(id) = targets.first() {
            doc.layer(*id).ok_or(EngineError::NoLayer(*id))?.clone()
        } else if let Some(layer) = &self.preview_layer {
            layer.clone()
        } else {
            Layer::new(
                crate::i18n::t(engine::layer_name(&candidate.parameters)),
                LayerContent::Shape(if geometry::stroke_only(&candidate.parameters) {
                    photocraft_doc::ShapeLayer { stroke: Some(Default::default()), ..Default::default() }
                } else {
                    photocraft_doc::ShapeLayer { fill: Some(photocraft_doc::Fill::Solid(photocraft_doc::Color::BLACK)), ..Default::default() }
                }),
            )
        };
        if targets.is_empty() {
            layer.name = crate::i18n::t(engine::layer_name(&candidate.parameters)).into();
        }
        let LayerContent::Shape(sh) = &layer.content else {
            return Err(EngineError::Other("shape preview target required".into()));
        };
        if layer.vector.appearance.is_some() || !layer.vector.corners.is_empty() {
            return Err(EngineError::Other("live shape preview requires appearance/corner projection".into()));
        }
        let mut base = sh.clone();
        if targets.is_empty()
            && let Some((fill, stroke)) = &self.paint
        {
            base.fill = fill.clone();
            base.stroke = stroke.clone();
        }
        let shape = engine::prepare(doc, &candidate, base)?;
        layer.content = LayerContent::Shape(shape);
        layer.vector.live_shape = Some(candidate.clone());
        self.preview = Some(candidate);
        self.preview_layer = Some(layer);
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    pub fn commit(&mut self, s: &mut Session) -> Result<()> {
        let g = self.gesture.take();
        let preview = self.preview.take();
        let paint = self.preview_layer.take().and_then(|layer| if let LayerContent::Shape(sh) = layer.content { Some((sh.fill, sh.stroke)) } else { None });
        self.numeric = None;
        let (Some(g), Some(d)) = (g, preview) else { return Ok(()) };
        g.capture.check(s)?;
        let base = json!({"document":g.capture.document.0,"revision":g.capture.revision});
        let mut params = base;
        if g.targets.is_empty() {
            params["name"] = json!(crate::i18n::t(engine::layer_name(&d.parameters)));
            params["design"] = json!(d);
            params["above"] = json!(g.above.map(|id| id.0));
            if let Some((fill, stroke)) = paint {
                params["fill"] = json!(fill);
                params["stroke"] = json!(stroke);
            }
            s.execute("shape.design.create", params)?;
        } else {
            params["edits"] = json!(g.targets.iter().map(|id| json!({"layer":id.0,"design":d})).collect::<Vec<_>>());
            s.execute("shape.design.set", params)?;
        }
        Ok(())
    }
    /// Detached document for the real compositor/preview plane; never replaces the active document.
    pub fn preview_document(&self, s: &Session) -> Result<photocraft_doc::Document> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let mut doc = (*st.doc).clone();
        if let Some(g) = &self.gesture {
            g.capture.check(s)?;
            if let Some(l) = &self.preview_layer {
                if let Some(id) = g.targets.first() {
                    *doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))? = l.clone();
                } else {
                    doc.insert_above(g.above, l.clone());
                }
            }
        } else if let Some((capture, id, _)) = &self.numeric {
            capture.check(s)?;
            if let Some(l) = &self.preview_layer {
                *doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))? = l.clone();
            }
        }
        Ok(doc)
    }
    fn record(&mut self, result: Result<()>) {
        if let Err(e) = result {
            self.preview = None;
            self.preview_layer = None;
            self.numeric = None;
            self.error = Some(e.to_string());
        } else {
            self.error = None;
        }
    }
}
/// One geometry implementation for legacy and design-shape frontends in either persona.
/// Snapping runs after constraining; its provider must preserve the current construction ray.
pub fn drag_design(original: &DesignShape, start: Point, p: Point, mods: egui::Modifiers, snap: impl Fn(Point, Point, bool) -> Point) -> DesignShape {
    let q = constrained_endpoint(original, start, p, mods.shift);
    design_between(original, start, snap(q, start, mods.shift), mods.alt)
}
fn constrained_endpoint(original: &DesignShape, start: Point, p: Point, shift: bool) -> Point {
    let mut q = p;
    let (dx, dy) = (p.x - start.x, p.y - start.y);
    if shift {
        if matches!(original.parameters, ShapeParameters::Line { .. } | ShapeParameters::Arrow { .. }) {
            let a = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
            let length = dx.hypot(dy);
            q = Point::new(start.x + length * a.cos(), start.y + length * a.sin());
        } else {
            let d = dx.abs().max(dy.abs());
            q = Point::new(start.x + if dx < 0. { -d } else { d }, start.y + if dy < 0. { -d } else { d });
        }
    }
    q
}
fn design_between(original: &DesignShape, start: Point, q: Point, centered: bool) -> DesignShape {
    let a = if centered { Point::new(2. * start.x - q.x, 2. * start.y - q.y) } else { start };
    let mut d = original.clone();
    d.rect = [a.x.min(q.x), a.y.min(q.y), (a.x - q.x).abs(), (a.y - q.y).abs()];
    fit_endpoints(&mut d, a, q);
    d
}
fn fit_endpoints(d: &mut DesignShape, a: Point, b: Point) {
    match &mut d.parameters {
        ShapeParameters::Line { from, to } | ShapeParameters::Arrow { from, to, .. } => {
            *from = [a.x, a.y];
            *to = [b.x, b.y];
            d.rect[2] = d.rect[2].max(1e-6);
            d.rect[3] = d.rect[3].max(1e-6);
        }
        ShapeParameters::Spiral { center, radius, .. } => {
            *center = [a.x, a.y];
            *radius = (a.x - b.x).hypot(a.y - b.y);
        }
        ShapeParameters::Callout { tail, .. } => *tail = [d.rect[0] + d.rect[2] * 0.5, d.rect[1] + d.rect[3] * 1.3],
        ShapeParameters::Rect { radii, .. } => {
            for r in radii {
                *r = r.min(d.rect[2].min(d.rect[3]) / 2.);
            }
        }
        _ => {}
    }
}
/// Independent canvas hook. Full-app routing calls it inside the ordinary canvas response.
pub fn canvas(
    ui: &mut egui::Ui,
    s: &mut Session,
    state: &mut State,
    rect: egui::Rect,
    view: View,
    selected: Option<LayerId>,
    enabled: bool,
    snap: impl Fn(Point, Point, bool) -> Point,
) {
    if !enabled || !ui.input(|i| i.focused) || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.cancel();
        return;
    }
    let response = ui.interact(rect, ui.id().with("live-shapes-canvas"), Sense::click_and_drag());
    if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
        if response.contains_pointer() && ui.input(|i| i.pointer.primary_pressed()) {
            pointer(s, state, Event::Down(view.document(p)), ui.input(|i| i.modifiers), selected, enabled, 7. / view.scale, &snap);
        }
        if state.gesture.is_some() && (ui.input(|i| i.pointer.primary_down()) || ui.input(|i| i.pointer.primary_released())) {
            let event = if ui.input(|i| i.pointer.primary_released()) { Event::Up(view.document(p)) } else { Event::Move(view.document(p)) };
            pointer(s, state, event, ui.input(|i| i.modifiers), selected, enabled, 7. / view.scale, &snap);
        }
    }
    paint(ui.painter(), state, view, s, selected);
}
pub fn paint(painter: &egui::Painter, state: &State, view: View, s: &Session, selected: Option<LayerId>) {
    paint_mapped(painter, state, |p| view.screen(p), s, selected);
}
/// Borrow the prepared preview geometry and use the host transform (including view flips).
pub fn paint_mapped(painter: &egui::Painter, state: &State, screen: impl Fn(Point) -> Pos2, s: &Session, selected: Option<LayerId>) {
    if let Some(layer) = &state.preview_layer
        && let LayerContent::Shape(shape) = &layer.content
    {
        for sp in &shape.path.subpaths {
            for q in sp.segments() {
                let mut pts = Vec::new();
                for i in 0..=24 {
                    let t = i as f64 / 24.;
                    let u = 1. - t;
                    pts.push(screen(Point::new(
                        u * u * u * q[0].x + 3. * u * u * t * q[1].x + 3. * u * t * t * q[2].x + t * t * t * q[3].x,
                        u * u * u * q[0].y + 3. * u * u * t * q[1].y + 3. * u * t * t * q[2].y + t * t * t * q[3].y,
                    )));
                }
                painter.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::from_rgb(220, 40, 40))));
            }
        }
    }
    let design = state.preview.as_ref().or_else(|| selected.and_then(|id| s.active()?.doc.layer(id)?.vector.live_shape.as_ref()));
    if let Some(d) = design
        && let Ok(handles) = geometry::handles(d)
    {
        for h in handles {
            painter.circle_filled(screen(h.position), 4., Color32::from_rgb(220, 40, 40));
        }
    }
}
/// Unit conversion at the UI boundary; geometry storage stays in document pixels.
pub fn unit_scale(unit: DesignUnits, dpi: f64) -> Result<f64> {
    if !dpi.is_finite() || !(1.0..=100_000.0).contains(&dpi) {
        return Err(EngineError::Other("invalid document DPI".into()));
    }
    Ok(match unit {
        DesignUnits::Pixels => 1.,
        DesignUnits::Points => dpi / 72.,
        DesignUnits::Millimeters => dpi / 25.4,
        DesignUnits::Inches => dpi,
    })
}
/// Numeric fields cover every persisted parameter; changes preview until drag release/Enter/focus loss.
pub fn controls(ui: &mut egui::Ui, s: &mut Session, state: &mut State, selected: &[LayerId], translate: impl Fn(&str) -> String) {
    if !ui.input(|i| i.focused) || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.cancel();
        return;
    }
    if selected.is_empty() {
        return;
    }
    if ui.button(translate("Convert to Curves")).clicked() {
        let result = (|| {
            state.commit(s)?;
            let c = Capture::new(s)?;
            s.execute(
                "shape.design.convert",
                json!({"document":c.document.0,"revision":c.revision,"layers":selected.iter().map(|id|id.0).collect::<Vec<_>>()}),
            )?;
            Ok(())
        })();
        state.record(result);
    }
    let id = selected[0];
    if state.numeric.as_ref().is_some_and(|(_, target, _)| *target != id) {
        state.cancel();
    }
    let original = state.numeric.as_ref().map(|(_, _, d)| d.clone()).or_else(|| s.active()?.doc.layer(id)?.vector.live_shape.clone());
    let Some(original) = original else {
        return;
    };
    let Some(st) = s.active() else {
        return;
    };
    if state.numeric.is_none() {
        let consistent = st
            .doc
            .layer(id)
            .is_some_and(|layer| matches!(&layer.content, LayerContent::Shape(shape) if geometry::generate(&original).is_ok_and(|path| path == shape.path)));
        if !consistent {
            state.error = Some("live parameters no longer match the base path".into());
            return;
        }
    }
    let scale = match unit_scale(st.doc.design_units, f64::from(st.doc.resolution_dpi)) {
        Ok(v) => v,
        Err(e) => {
            state.error = Some(e.to_string());
            return;
        }
    };
    let mut value = json!(original);
    // Document serde omits a zero angle, but the context bar must still expose rotation.
    value["angle"] = json!(original.angle);
    let (changed, finished) = numeric_fields(ui, &mut value, "", "", scale, &translate);
    if changed {
        let result = (|| {
            let candidate: DesignShape = serde_json::from_value(value).map_err(|e| EngineError::Other(e.to_string()))?;
            let capture = state.numeric.as_ref().map(|v| v.0).unwrap_or(Capture::new(s)?);
            capture.check(s)?;
            state.prepare_preview(s, candidate.clone(), &[id])?;
            state.numeric = Some((capture, id, candidate));
            Ok(())
        })();
        state.record(result);
    }
    if finished && let Some((capture, id, d)) = state.numeric.take() {
        state.preview = None;
        state.preview_layer = None;
        let result =
            s.execute("shape.design.set", json!({"document":capture.document.0,"revision":capture.revision,"edits":[{"layer":id.0,"design":d}]})).map(|_| ());
        state.record(result);
    }
    if let Some(error) = &state.error {
        ui.label(error);
    }
}
fn numeric_fields(ui: &mut egui::Ui, value: &mut Value, name: &str, label: &str, scale: f64, tr: &impl Fn(&str) -> String) -> (bool, bool) {
    let mut changed = false;
    let mut finished = false;
    match value {
        Value::Object(map) => {
            for (name, value) in map {
                let (c, f) = numeric_fields(ui, value, name, field_label(name), scale, tr);
                changed |= c;
                finished |= f;
            }
        }
        Value::Array(array) => {
            for (i, value) in array.iter_mut().enumerate() {
                ui.push_id((name, i), |ui| {
                    let (c, f) = numeric_fields(ui, value, name, component_label(name, i).unwrap_or(label), scale, tr);
                    changed |= c;
                    finished |= f;
                });
            }
        }
        Value::Number(number) => {
            let Some(mut n) = number.as_f64() else { return (false, false) };
            let integer = number.is_u64();
            let unit = if is_length(name) { scale } else { 1. };
            n /= unit;
            ui.horizontal(|ui| {
                let label = ui.label(tr(label));
                let response = ui.add(egui::DragValue::new(&mut n).speed(if integer { 1. } else { 0.1 })).labelled_by(label.id);
                if response.changed() {
                    let v = n * unit;
                    if v.is_finite() {
                        *value = if integer { json!(v.max(0.).round() as u64) } else { json!(v) };
                        changed = true;
                    }
                }
                finished |= response.drag_stopped() || response.lost_focus() || response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            });
        }
        Value::Bool(b) => {
            let r = ui.checkbox(b, tr(label));
            changed = r.changed();
            finished = changed;
        }
        Value::String(kind) if name == "kinds" => {
            ui.label(tr(label));
            egui::ComboBox::from_id_salt(ui.id().with(name)).selected_text(tr(kind)).show_ui(ui, |ui| {
                for v in ["Round", "Inverted", "Chamfer", "Straight"] {
                    let r = ui.selectable_value(kind, v.into(), tr(v));
                    changed |= r.changed();
                    finished |= r.changed();
                }
            });
        }
        _ => {}
    }
    (changed, finished)
}
fn field_label(name: &str) -> &str {
    match name {
        "angle" => "Rotation (radians)",
        "sides" => "Sides",
        "curved" | "curve" => "Curvature",
        "corner" => "Corner size",
        "points" => "Points",
        "inner" => "Inner radius ratio",
        "secondary" => "Secondary radius ratio",
        "top" => "Vertex position",
        "top_left" => "Top left",
        "top_right" => "Top right",
        "hole" => "Hole radius ratio",
        "start" => "Start angle (radians)",
        "end" => "End angle (radians)",
        "head_width" => "Head width",
        "head_length" => "Head length",
        "shaft" => "Shaft width",
        "double" => "Double arrow",
        "ellipse" => "Ellipse body",
        "radius" => "Radius",
        "teeth" => "Teeth",
        "depth" => "Depth",
        "bend" => "Bend",
        "offset" => "Offset",
        "bubbles" => "Bubbles",
        "decay" => "Decay",
        "turns" => "Turns",
        "rows" => "Rows",
        "cols" => "Columns",
        "rings" => "Rings",
        "spokes" => "Spokes",
        _ => name,
    }
}
fn component_label(name: &str, i: usize) -> Option<&'static str> {
    let labels: &[&str] = match name {
        "rect" => &["X", "Y", "Width", "Height"],
        "radii" => &["Top left radius", "Top right radius", "Bottom right radius", "Bottom left radius"],
        "kinds" => &["Top left corner", "Top right corner", "Bottom right corner", "Bottom left corner"],
        "from" => &["Start X", "Start Y"],
        "to" => &["End X", "End Y"],
        "tail" => &["Tail X", "Tail Y"],
        "center" => &["Center X", "Center Y"],
        _ => &[],
    };
    labels.get(i).copied()
}
fn is_length(name: &str) -> bool {
    matches!(name, "rect" | "radii" | "corner" | "radius" | "from" | "to" | "tail" | "center" | "head_width" | "head_length" | "shaft")
}
#[cfg(test)]
#[path = "drawing/tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Down(Point),
    Move(Point),
    Up(Point),
    Cancel,
}
/// Full-app captured-pointer router. 00 maps canvas::ToolEvent to these events before legacy shape routing.
/// `handle_tolerance` is in document pixels (normally 7 / zoom).
pub fn pointer(
    s: &mut Session,
    state: &mut State,
    event: Event,
    mods: egui::Modifiers,
    selected: Option<LayerId>,
    enabled: bool,
    handle_tolerance: f64,
    snap: impl Fn(Point, Point, bool) -> Point,
) -> bool {
    let result = pointer_checked(s, state, event, mods, selected, enabled, handle_tolerance, |p, start, shift| Ok(snap(p, start, shift)));
    state.record(result.map(|_| ()));
    enabled
}
/// Shared pointer implementation; errors from the snap provider cannot become raw commits.
pub fn pointer_checked(
    s: &mut Session,
    state: &mut State,
    event: Event,
    mods: egui::Modifiers,
    selected: Option<LayerId>,
    enabled: bool,
    handle_tolerance: f64,
    snap: impl Fn(Point, Point, bool) -> Result<Point>,
) -> Result<bool> {
    if !enabled {
        state.cancel();
        return Ok(false);
    }
    let result = (|| match event {
        Event::Cancel => {
            state.cancel();
            Ok(())
        }
        Event::Down(p) => {
            let hit = selected
                .and_then(|id| s.active()?.doc.layer(id)?.vector.live_shape.as_ref())
                .and_then(|d| geometry::handles(d).ok())
                .and_then(|handles| handles.into_iter().find(|h| (h.position.x - p.x).hypot(h.position.y - p.y) <= handle_tolerance));
            if let (Some(id), Some(hit)) = (selected, hit) { state.begin_handle(s, id, hit.kind) } else { state.begin_new(s, snap(p, p, false)?, selected) }
        }
        Event::Move(p) => state.update_checked(s, p, mods, &snap),
        Event::Up(p) => {
            state.update_checked(s, p, mods, &snap)?;
            state.commit(s)
        }
    })();
    if result.is_err() {
        state.cancel();
    }
    result.map(|()| true)
}
/// Existing persona/tool registry 01 selects these defaults; no new registry is created here.
pub fn parameters_for(id: &str) -> Option<ShapeParameters> {
    use ShapeParameters::*;
    let id = if id.eq_ignore_ascii_case("EllipseShape") { "ellipse" } else { id };
    const IDS: &[&str] = &[
        "rectangle",
        "roundedRectangle",
        "ellipse",
        "polygon",
        "star",
        "doubleStar",
        "squareStar",
        "triangle",
        "trapezoid",
        "diamond",
        "donut",
        "pie",
        "segment",
        "arrow",
        "callout",
        "ellipseCallout",
        "cog",
        "heart",
        "tear",
        "crescent",
        "cloud",
        "spiral",
        "arc",
        "line",
        "rectGrid",
        "polarGrid",
    ];
    let id = IDS.iter().copied().find(|name| name.eq_ignore_ascii_case(id))?;
    Some(match id {
        "rectangle" => Rect { radii: [0.; 4], kinds: [CornerKind::Round; 4] },
        "roundedRectangle" => Rect { radii: [12.; 4], kinds: [CornerKind::Round; 4] },
        "ellipse" => Ellipse,
        "polygon" => Polygon { sides: 6, curved: 0., corner: 0. },
        "star" => Star { points: 5, inner: 0.5, corner: 0. },
        "doubleStar" => DoubleStar { points: 5, inner: 0.3, secondary: 0.7, corner: 0. },
        "squareStar" => SquareStar { points: 4, inner: 0.4, corner: 0. },
        "triangle" => Triangle { top: 0.5 },
        "trapezoid" => Trapezoid { top_left: 0.2, top_right: 0.8 },
        "diamond" => Diamond { top: 0.5 },
        "donut" => Donut { hole: 0.5 },
        "pie" => Pie { start: 0., end: std::f64::consts::PI * 1.5 },
        "segment" => Segment { start: 0., end: std::f64::consts::PI, inner: 0. },
        "arrow" => Arrow { from: [0., 0.], to: [100., 0.], head_width: 24., head_length: 20., shaft: 10., double: false },
        "callout" => Callout { ellipse: false, tail: [50., 130.], radius: 0. },
        "ellipseCallout" => Callout { ellipse: true, tail: [50., 130.], radius: 0. },
        "cog" => Cog { teeth: 12, inner: 0.7, hole: 0.2, curve: 0. },
        "heart" => Heart { depth: 0.5 },
        "tear" => Tear { bend: 0. },
        "crescent" => Crescent { offset: 1. },
        "cloud" => Cloud { bubbles: 6 },
        "spiral" => Spiral { center: [0., 0.], radius: 50., decay: 0.8, turns: 3. },
        "arc" => Arc { start: 0., end: std::f64::consts::PI },
        "line" => Line { from: [0., 0.], to: [100., 100.] },
        "rectGrid" => RectGrid { rows: 3, cols: 3 },
        "polarGrid" => PolarGrid { rings: 3, spokes: 8 },
        _ => return None,
    })
}
