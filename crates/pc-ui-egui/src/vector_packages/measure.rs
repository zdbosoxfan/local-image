//! Transient Measure/Area interaction, following VectorCraft xform/measure.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-LICENSE-{MIT,APACHE} and vectorcraft-NOTICE.
//! Painter overlays never enter the document, raster cache, export or settings model.
use egui::{Color32, Pos2, Rect, Response, Stroke, Ui};
use photocraft_doc::vector_design::structure::DesignUnits;
use photocraft_doc::{DocId, Document, LayerId};
use photocraft_engine::{
    EngineError, Result,
    vector_packages::{
        measure::{self as engine, Area, AreaCache, Capture, Distance, DrawingScale, Gesture, PerimeterMode, ScaleEdit, Units},
        snapping::query::{Settings, View},
    },
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Measure,
    Area,
}
#[derive(Clone, Copy)]
pub struct Mapping {
    pub origin: Pos2,
    pub document_origin: [f64; 2],
    pub zoom: f64,
    pub flip: bool,
    pub rotation: f32,
}
impl Mapping {
    fn camera(self) -> crate::view_camera::Camera {
        crate::view_camera::Camera::new(Rect::from_center_size(self.origin, egui::Vec2::ZERO), self.zoom as f32, [0.0, 0.0], self.flip, self.rotation)
    }
    fn point(self, p: Pos2) -> [f64; 2] {
        let d = self.camera().to_document(p);
        [self.document_origin[0] + f64::from(d.x), self.document_origin[1] + f64::from(d.y)]
    }
    pub fn screen(self, p: [f64; 2]) -> Pos2 {
        self.camera().to_screen(egui::pos2((p[0] - self.document_origin[0]) as f32, (p[1] - self.document_origin[1]) as f32))
    }
}
#[derive(Clone)]
struct Drag {
    gesture: Arc<Gesture>,
    mapping: Mapping,
    view: View,
    last: Pos2,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub document: DocId,
    pub a: [f64; 2],
    pub b: [f64; 2],
}
#[derive(Clone)]
pub struct State {
    drag: Option<Drag>,
    completed: Option<Line>,
    pub line: Option<Line>,
    pub distance: Option<Distance>,
    pub area: Option<Area>,
    pub hovered: Option<LayerId>,
    pub unit: Option<DesignUnits>,
    pub perimeter_mode: PerimeterMode,
    area_cache: Option<Arc<AreaCache>>,
    mode: Mode,
    pub scale_factor: f64,
    pub real_length: f64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            drag: None,
            completed: None,
            line: None,
            distance: None,
            area: None,
            hovered: None,
            unit: None,
            perimeter_mode: PerimeterMode::default(),
            area_cache: None,
            mode: Mode::Measure,
            scale_factor: 1.,
            real_length: 1.,
        }
    }
}
pub struct Input<'a> {
    pub doc: &'a Document,
    pub revision: u64,
    pub selected: &'a [LayerId],
    pub mode: Mode,
    pub mapping: Mapping,
    pub view: View,
    pub settings: &'a Settings,
    pub scale: DrawingScale,
    pub cancel: bool,
}
impl State {
    pub fn captured(&self) -> bool {
        self.drag.is_some()
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.line = self.completed.clone();
        self.distance = None;
    }
    pub fn clear(&mut self) {
        self.cancel();
        self.line = None;
        self.completed = None;
        self.distance = None;
        self.area = None;
        self.hovered = None;
        self.area_cache = None;
    }
    /// Call on every frame, including modifier-only frames and release outside canvas.
    pub fn input(&mut self, response: &Response, input: Input<'_>) -> Result<()> {
        let result = self.update(response, input);
        if result.is_err() {
            self.cancel();
            self.area = None;
            self.hovered = None;
        }
        result
    }
    fn update(&mut self, response: &Response, input: Input<'_>) -> Result<()> {
        let (pressed, released, down, pos, mods, escape, focused) = response.ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.primary_down(),
                i.pointer.latest_pos().or(i.pointer.interact_pos()),
                i.modifiers,
                i.key_pressed(egui::Key::Escape),
                i.focused && !i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))),
            )
        });
        input.view.tolerance(input.settings.tolerance_px)?;
        View { zoom: input.mapping.zoom, pixels_per_point: input.view.pixels_per_point }.tolerance(input.settings.tolerance_px)?;
        if !input.mapping.origin.is_finite() || input.mapping.document_origin.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
            return Err(EngineError::Other("invalid measurement canvas transform".into()));
        }
        let mut units = Units::document(input.doc, input.scale);
        units.unit = self.unit.unwrap_or(units.unit);
        units.length(1.)?;
        if self.line.as_ref().is_some_and(|l| l.document != input.doc.id) {
            self.clear();
        }
        if self.mode != input.mode {
            self.cancel();
            self.area = None;
            self.hovered = None;
            self.mode = input.mode;
        }
        let stale = self.drag.as_ref().is_some_and(|d| d.gesture.capture.document != input.doc.id || d.gesture.capture.revision != input.revision);
        if input.cancel || escape || !focused || stale {
            self.cancel();
            self.area = None;
            self.hovered = None;
            self.distance = self.line.as_ref().map(|l| engine::distance(l.a, l.b, units)).transpose()?;
            return Ok(());
        }
        match input.mode {
            Mode::Measure => {
                let bypass = mods.ctrl || mods.command || mods.mac_cmd;
                if pressed && self.drag.is_none() {
                    let origin = response
                        .ctx
                        .input(|i| i.pointer.press_origin())
                        .filter(|p| response.rect.contains(*p) && response.ctx.layer_id_at(*p) == Some(response.layer_id));
                    if let Some(origin) = origin {
                        let gesture = Arc::new(Gesture::begin(input.doc, input.revision, input.mapping.point(origin), input.settings, input.view, bypass)?);
                        self.line = Some(Line { document: input.doc.id, a: gesture.start, b: gesture.start });
                        self.drag = Some(Drag { gesture, mapping: input.mapping, view: input.view, last: origin });
                    }
                }
                if let Some(drag) = &mut self.drag {
                    if !down && !released {
                        self.cancel();
                    } else {
                        if let Some(p) = pos {
                            drag.last = p;
                        }
                        let end = drag.gesture.endpoint(drag.mapping.point(drag.last), input.settings, drag.view, mods.shift, bypass)?;
                        self.line = Some(Line { document: input.doc.id, a: drag.gesture.start, b: end.point });
                        if released {
                            drag.gesture.capture.check(input.doc, input.revision)?;
                            self.completed = self.line.clone();
                            self.drag = None;
                        }
                    }
                }
                self.distance = self.line.as_ref().map(|l| engine::distance(l.a, l.b, units)).transpose()?;
            }
            Mode::Area => {
                if self.area_cache.as_ref().is_none_or(|c| !c.current(input.doc, input.revision)) {
                    self.area_cache = Some(Arc::new(AreaCache::build(input.doc, input.revision)?));
                }
                let cache = self.area_cache.as_ref().ok_or_else(|| EngineError::Other("missing area cache".into()))?;
                let pointer =
                    pos.filter(|p| response.rect.contains(*p) && response.ctx.layer_id_at(*p) == Some(response.layer_id)).map(|p| input.mapping.point(p));
                self.hovered = pointer.map(|p| cache.hover(p, input.view.tolerance(6.)?)).transpose()?.flatten();
                let hover = self.hovered.zip(pointer);
                let ids = if input.selected.is_empty() { self.hovered.into_iter().collect::<Vec<_>>() } else { input.selected.to_vec() };
                self.area = if ids.is_empty() { None } else { Some(cache.read(input.doc, input.revision, &ids, hover, self.perimeter_mode, units)?) };
            }
        }
        Ok(())
    }
}
pub fn unit_suffix(u: DesignUnits) -> &'static str {
    match u {
        DesignUnits::Pixels => "px",
        DesignUnits::Points => "pt",
        DesignUnits::Millimeters => "mm",
        DesignUnits::Inches => "in",
    }
}
fn distance_text(d: Distance, u: DesignUnits, text: &impl Fn(&str) -> String) -> String {
    let unit = unit_suffix(u);
    format!("{}: {:.4} {unit}\n{}: {:.2}°\nΔX: {:.4} {unit}   ΔY: {:.4} {unit}", text("Distance"), d.distance, text("Angle"), d.angle, d.dx, d.dy)
}
fn area_text(a: Area, u: DesignUnits, text: &impl Fn(&str) -> String) -> String {
    let unit = unit_suffix(u);
    let mut s = format!(
        "{}: {:.4} {unit}²\n{}: {:.4} {unit}²\n{}: {:.4} {unit}",
        text("Area"),
        a.area,
        text("Including holes"),
        a.including_holes,
        text("Perimeter"),
        a.perimeter
    );
    if let Some(v) = a.segment {
        s.push_str(&format!("\n{}: {v:.4} {unit}", text("Segment")));
    }
    s
}
/// The caller's ordinary English-source catalogue function localizes every label.
/// Return a prepared request; 00 persists it via ScaleEdit::commit and its setter.
#[allow(clippy::too_many_arguments)]
pub fn options(
    ui: &mut Ui,
    state: &mut State,
    doc: &Document,
    revision: u64,
    artboard: Option<LayerId>,
    scale: DrawingScale,
    text: impl Fn(&str) -> String,
) -> Result<Option<ScaleEdit>> {
    let mut request = None;
    let unit = state.unit.unwrap_or(doc.design_units);
    ui.horizontal(|ui| {
        ui.label(text("Measurement units"));
        for (u, label) in
            [(DesignUnits::Pixels, "Pixels"), (DesignUnits::Points, "Points"), (DesignUnits::Millimeters, "Millimeters"), (DesignUnits::Inches, "Inches")]
        {
            if ui.selectable_label(unit == u, text(label)).clicked() {
                state.unit = Some(u);
            }
        }
        if state.mode == Mode::Area {
            for (m, label) in [(PerimeterMode::FullEdges, "Full edges"), (PerimeterMode::CuspedSegments, "Cusped segments")] {
                if ui.selectable_label(state.perimeter_mode == m, text(label)).clicked() {
                    state.perimeter_mode = m;
                }
            }
        }
    });
    if let Some(d) = state.distance.filter(|_| state.mode == Mode::Measure) {
        ui.label(distance_text(d, unit, &text));
    }
    if let Some(a) = state.area.filter(|_| state.mode == Mode::Area) {
        ui.label(area_text(a, unit, &text));
    }
    ui.horizontal(|ui| {
        ui.label(format!("{} 1:{}", text("Drawing scale"), scale.factor));
        let label = ui.label(text("Scale factor"));
        ui.add(egui::DragValue::new(&mut state.scale_factor).range(1e-6..=1e6)).labelled_by(label.id);
        if ui.button(text("Set drawing scale")).clicked() {
            request = Some(Ok(DrawingScale { factor: state.scale_factor }));
        }
    });
    ui.horizontal(|ui| {
        let label = ui.label(text("Real length"));
        ui.add(egui::DragValue::new(&mut state.real_length).range(1e-9..=1e7)).labelled_by(label.id);
        if ui
            .add_enabled(state.completed.as_ref().is_some_and(|l| l.document == doc.id) && !state.captured(), egui::Button::new(text("Assign drawing scale")))
            .clicked()
        {
            request = state
                .completed
                .as_ref()
                .map(|l| DrawingScale::assign((l.b[0] - l.a[0]).hypot(l.b[1] - l.a[1]), state.real_length, unit, f64::from(doc.resolution_dpi)));
        }
    });
    request
        .map(|r| {
            let scale = r?;
            scale.validate()?;
            Ok(ScaleEdit { capture: Capture::new(doc, revision, artboard.into_iter().collect())?, artboard, scale })
        })
        .transpose()
}
/// UI overlay only. Use the current display transform to paint a captured document line.
pub fn paint(painter: &egui::Painter, state: &State, mapping: Mapping, unit: DesignUnits, text: impl Fn(&str) -> String) {
    let color = Color32::from_rgb(0, 210, 230);
    if state.mode == Mode::Measure
        && let (Some(line), Some(d)) = (&state.line, state.distance)
    {
        let a = mapping.screen(line.a);
        let b = mapping.screen(line.b);
        painter.line_segment([a, b], Stroke::new(1., color));
        painter.circle_stroke(a, 3., Stroke::new(1., color));
        painter.circle_stroke(b, 3., Stroke::new(1., color));
        painter.text(b + egui::vec2(12., 12.), egui::Align2::LEFT_TOP, distance_text(d, unit, &text), egui::FontId::proportional(12.), color);
    }
    if state.mode == Mode::Area
        && let Some(a) = state.area
    {
        painter.text(
            painter.clip_rect().left_top() + egui::vec2(12., 12.),
            egui::Align2::LEFT_TOP,
            area_text(a, unit, &text),
            egui::FontId::proportional(12.),
            color,
        );
    }
}
/// Production adapter awaiting 00's canvas callsite; context storage avoids new app fields.
#[derive(Clone)]
pub struct Bridge {
    pub state: State,
    pub mode: Option<Mode>,
    pub scale: DrawingScale,
    pub settings: Settings,
    owner: Option<crate::state::Tool>,
    persona: Option<photocraft_engine::toolsets::Persona>,
    module: Option<crate::Module>,
}
impl Default for Bridge {
    fn default() -> Self {
        Self {
            state: State::default(),
            mode: None,
            scale: DrawingScale::default(),
            settings: Settings { curves: true, ..Default::default() },
            owner: None,
            persona: None,
            module: None,
        }
    }
}
fn key() -> egui::Id {
    egui::Id::new("vector-measure-44")
}
pub fn bridge(ctx: &egui::Context) -> Bridge {
    ctx.data(|d| d.get_temp(key())).unwrap_or_default()
}
pub fn activate(app: &crate::PhotocraftApp, ctx: &egui::Context, mode: Mode) {
    let mut b = bridge(ctx);
    b.state.cancel();
    b.mode = Some(mode);
    b.owner = Some(app.ui.tool);
    b.persona = Some(app.session.prefs().toolbar.persona);
    b.module = Some(app.current_module);
    ctx.data_mut(|d| d.insert_temp(key(), b));
}
pub fn set_scale(ctx: &egui::Context, scale: DrawingScale) -> Result<()> {
    scale.validate()?;
    let mut b = bridge(ctx);
    b.scale = scale;
    ctx.data_mut(|d| d.insert_temp(key(), b));
    Ok(())
}
/// Before canvas dispatch. Tool/persona/module changes cancel previews and deactivate.
pub fn frame(app: &crate::PhotocraftApp, ctx: &egui::Context) {
    let mut b = bridge(ctx);
    if b.owner.is_some_and(|t| t != app.ui.tool)
        || b.persona.is_some_and(|p| p != app.session.prefs().toolbar.persona)
        || b.module.is_some_and(|m| m != app.current_module)
    {
        b.state.cancel();
        b.mode = None;
    }
    let keyboard = crate::shortcuts::canvas_keyboard_available(ctx);
    if ctx.input(|i| (keyboard && i.key_pressed(egui::Key::Escape)) || !i.focused) || !app.ui.dialogs.is_empty() {
        b.state.cancel();
        b.state.area = None;
    }
    ctx.data_mut(|d| d.insert_temp(key(), b));
}
pub fn route(app: &mut crate::PhotocraftApp, ui: &mut Ui, idx: usize, xf: &crate::canvas::ViewXform, rect: egui::Rect) -> bool {
    if app.session.active_index() != Some(idx) {
        return false;
    }
    let ctx = ui.ctx().clone();
    frame(app, &ctx);
    let mut b = bridge(&ctx);
    let Some(mode) = b.mode else {
        return false;
    };
    let Some(st) = app.session.active() else {
        return false;
    };
    let p = xf.to_doc(rect.min);
    let mapping = Mapping { origin: rect.min, document_origin: p, zoom: f64::from(xf.zoom), flip: xf.flip, rotation: xf.rotation };
    let response = ui.interact(rect, egui::Id::new(("measure44-canvas", st.doc.id.0)), egui::Sense::click_and_drag());
    if let Err(e) = b.state.input(
        &response,
        Input {
            doc: &st.doc,
            revision: st.revision,
            selected: &st.selected_layers(),
            mode,
            mapping,
            view: View { zoom: mapping.zoom, pixels_per_point: f64::from(ctx.pixels_per_point()) },
            settings: &b.settings,
            scale: b.scale,
            cancel: !app.ui.dialogs.is_empty(),
        },
    ) {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    } else {
        let unit = b.state.unit.unwrap_or(st.doc.design_units);
        paint(&ui.painter().with_clip_rect(rect), &b.state, mapping, unit, |s| crate::i18n::t(s).to_owned());
    }
    ctx.data_mut(|d| d.insert_temp(key(), b));
    true
}
#[cfg(test)]
#[path = "measure/tests.rs"]
mod tests;

pub fn app_context(app: &mut crate::PhotocraftApp, ui: &mut Ui) {
    let Some(st) = app.session.active() else { return };
    let mut b = bridge(ui.ctx());
    let scale = DrawingScale { factor: st.doc.drawing_scale.unwrap_or_default().factor };
    let request = options(ui, &mut b.state, &st.doc, st.revision, None, scale, |s| crate::i18n::t(s).into());
    match request.and_then(|request| request.map(|request| app.session.execute("vector.measure.setScale", serde_json::json!(request))).transpose()) {
        Ok(_) => {}
        Err(e) => {
            app.ui.status = e.to_string();
            app.ui.status_error = true;
        }
    }
    b.scale = app.session.active().map(|st| DrawingScale { factor: st.doc.drawing_scale.unwrap_or_default().factor }).unwrap_or_default();
    ui.ctx().data_mut(|d| d.insert_temp(key(), b));
}
