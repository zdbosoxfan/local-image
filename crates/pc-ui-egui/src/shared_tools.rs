//! One handler per tool family for both personas. Package 00 installs this adapter in
//! its serial router; existing Pixel entrypoints already reuse the Pen/shape core.
//! No gesture state lives in egui's cloning temp-data store.
use crate::{
    PhotocraftApp,
    canvas::{ToolEvent, ViewXform},
    vector_packages::{appearance, corners, drawing, layout, path_text, pen, pencil},
};
use egui::{Modifiers, Rect};
use photocraft_doc::{Color, Document, Fill, LayerId, ShapeStroke};
use photocraft_engine::{
    EngineError, Result, Session,
    shared_tools::{self as registry, Handler},
    toolsets::Persona,
};
use photocraft_geom::Point;

pub type PointerHandler = fn(&mut State, &mut PhotocraftApp, ToolEvent, Modifiers) -> Result<()>;

/// The function address is independent of persona, including aliases. This is also
/// the integration guard against adding a separate reduced Pixel implementation.
pub fn handler(id: &str, persona: Persona) -> Option<PointerHandler> {
    Some(match registry::resolve(id, persona)? {
        Handler::Pen => pen_event,
        Handler::Shape => shape_event,
        Handler::Node => node_event,
        Handler::VectorPencil => pencil_event,
        Handler::Corner => corner_event,
        Handler::VectorPaint => paint_event,
        Handler::PathText => path_text_event,
        Handler::VectorCrop | Handler::Transform => layout_event,
        _ => existing_event,
    })
}

#[derive(Default)]
pub struct State {
    pub pen: pen::State,
    pub drawing: drawing::State,
    pub pencil: pencil::State,
    pub corners: corners::State,
    pub paint: appearance::State,
    pub path_text: path_text::State,
    pub shape_mode: bool,
    pub layout: layout::State,
    pub snapping: photocraft_engine::vector_packages::snapping::query::Settings,
    pub snap_result: Option<photocraft_engine::vector_packages::snapping::query::SnapResult>,
    /// Host supplies edited-object/work-path exclusions before pointer capture.
    pub snap_exclusions: photocraft_engine::vector_packages::snapping::query::Exclusions,
    /// Physical display scale for pointer-only hosts; raw egui input refreshes it automatically.
    pub pixels_per_point: Option<f64>,
    snap_cache: Option<photocraft_engine::vector_packages::snapping::query::Cache>,
    id: Option<&'static str>,
    persona: Option<Persona>,
    capture: Option<Capture>,
    text_start: Option<[f64; 2]>,
    last: Option<(Point, Modifiers)>,
    mapping: Option<ViewXform>,
}
struct Capture {
    document: photocraft_doc::DocId,
    revision: u64,
    active: Option<LayerId>,
    selected: Vec<LayerId>,
}
impl Capture {
    fn new(s: &Session) -> Option<Self> {
        let st = s.active()?;
        Some(Self { document: st.doc.id, revision: st.revision, active: st.active_layer, selected: st.selected_layers() })
    }
    fn current(&self, s: &Session) -> bool {
        s.active().is_some_and(|st| {
            st.doc.id == self.document
                && st.revision == self.revision
                && st.active_layer == self.active
                && if st.selected_layers.is_empty() { self.selected.as_slice() == st.active_layer.as_slice() } else { self.selected == st.selected_layers }
        })
    }
}
impl State {
    pub fn cancel(&mut self) {
        self.pen.cancel();
        self.drawing.cancel();
        self.pencil.cancel();
        self.corners.cancel();
        self.paint.cancel();
        self.path_text.cancel();
        self.layout.cancel();
        self.snap_cache = None;
        self.snap_result = None;
        self.capture = None;
        self.text_start = None;
        self.last = None;
        self.mapping = None;
    }
    /// Abandon shared and existing Node capture together at the host lifecycle boundary.
    pub fn deactivate(&mut self, app: &mut PhotocraftApp) {
        self.cancel();
        crate::direct_select::cancel(app);
        app.ui.pen = None;
    }
    pub fn activate(&mut self, app: &mut PhotocraftApp, id: &str, persona: Persona) -> bool {
        let id = photocraft_engine::toolsets::canonical_tool(id).filter(|id| registry::resolve(id, persona).is_some());
        if self.id != id || self.persona != Some(persona) {
            self.deactivate(app);
            self.id = id;
            self.persona = Some(persona);
            self.shape_mode = persona == Persona::Vector;
            if let Some(parameters) = id.and_then(drawing::parameters_for) {
                self.drawing.parameters = parameters;
            }
        }
        if self.capture.as_ref().is_some_and(|c| !c.current(&app.session)) {
            self.deactivate(app);
        }
        id.is_some()
    }
    pub fn pointer(&mut self, app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> Result<bool> {
        let Some((id, persona)) = self.id.zip(self.persona) else { return Ok(false) };
        let Some(run) = handler(id, persona) else { return Ok(false) };
        if self.capture.as_ref().is_some_and(|c| !c.current(&app.session)) {
            self.deactivate(app);
            return Ok(true);
        }
        let at = point(ev);
        if !at.x.is_finite() || !at.y.is_finite() || at.x.abs() > 1e6 || at.y.abs() > 1e6 {
            self.deactivate(app);
            return Err(EngineError::Other("tool pointer must be finite and bounded".into()));
        }
        if matches!(ev, ToolEvent::Down { .. }) && self.capture.is_none() {
            self.capture = Capture::new(&app.session);
            if let Some(st) = app.session.active() {
                use photocraft_engine::vector_packages::snapping::query::{Cache, installed_geometry};
                let cache = Cache::build(&st.doc, st.revision, self.snap_exclusions.clone(), installed_geometry());
                match cache {
                    Ok(cache) => self.snap_cache = Some(cache),
                    Err(e) => {
                        self.deactivate(app);
                        return Err(e);
                    }
                }
            }
        }
        let result = run(self, app, ev, mods);
        if result.is_err() {
            self.deactivate(app);
        } else {
            self.last = Some((at, mods));
            if matches!(ev, ToolEvent::Up { .. }) && self.pen.gesture.is_none() && self.path_text.draft.is_none() {
                self.capture = None;
                self.mapping = None;
                self.snap_cache = None;
                self.snap_result = None;
            }
        }
        result.map(|()| true)
    }
    /// Common raw-event adapter, with capture outside the canvas and current modifiers.
    /// Returns ownership, allowing 00 to suppress the old dispatch exactly once.
    pub fn input(&mut self, app: &mut PhotocraftApp, ui: &mut egui::Ui, id: &str, persona: Persona, xf: &ViewXform, rect: Rect) -> bool {
        if !self.activate(app, id, persona) {
            return false;
        }
        let ctx = ui.ctx();
        self.pixels_per_point = Some(f64::from(ctx.pixels_per_point()));
        if !app.ui.dialogs.is_empty() || !ctx.input(|i| i.focused) {
            self.deactivate(app);
            return true;
        }
        if !ctx.text_edit_focused() && ctx.input_mut(|i| i.consume_key(i.modifiers, egui::Key::Escape)) {
            self.deactivate(app);
            return true;
        }
        match registry::handler(id) {
            Some(Handler::Node) => {
                crate::direct_select::frame(app, ctx);
                return crate::direct_select::canvas_input(app, ctx, app.session.active_index().unwrap_or(0), xf, rect);
            }
            Some(Handler::PathText) => {
                let result = path_text::input(
                    &mut self.path_text,
                    &mut app.session,
                    ctx,
                    rect,
                    |p| {
                        let p = xf.to_doc(p);
                        Point::new(p[0], p[1])
                    },
                    true,
                    6. / f64::from(xf.zoom).max(0.01),
                    &Default::default(),
                );
                if let Err(e) = result {
                    self.deactivate(app);
                    app.ui.status = e.to_string();
                    app.ui.status_error = true;
                }
                return true;
            }
            // These established app paths already share an implementation. Keep canvas
            // pan/zoom, live raster paint, type IME and Move capture in their existing host.
            Some(Handler::Text) if !matches!(self.id, Some("ArtText" | "FrameText")) => return false,
            Some(
                Handler::Move
                | Handler::PathSelection
                | Handler::CustomShape
                | Handler::Crop
                | Handler::Eyedropper
                | Handler::Hand
                | Handler::Zoom
                | Handler::RasterPencil
                | Handler::RasterGradient
                | Handler::Raster
                | Handler::Feature(_),
            ) => return false,
            _ => {}
        }
        // Borrow the event slice. Clone neither the egui event list nor gesture/document state.
        // Dispatch outside ctx.input to avoid locking egui during an app command.
        let count = ctx.input(|i| i.events.len());
        for index in 0..count {
            let event = ctx.input(|i| i.events.get(index).cloned());
            let event = match event {
                Some(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers }) => {
                    if pressed && !rect.contains(pos) || !pressed && self.capture.is_none() {
                        continue;
                    }
                    if pressed && self.mapping.is_none() {
                        self.mapping = Some(*xf);
                    }
                    let at = self.mapping.as_ref().unwrap_or(xf).to_doc(pos);
                    Some((if pressed { ToolEvent::Down { x: at[0], y: at[1], pressure: 1. } } else { ToolEvent::Up { x: at[0], y: at[1] } }, modifiers))
                }
                Some(egui::Event::PointerMoved(pos)) if self.capture.is_some() || self.pen.gesture.is_some() => {
                    let at = self.mapping.as_ref().unwrap_or(xf).to_doc(pos);
                    Some((ToolEvent::Move { x: at[0], y: at[1], pressure: 1. }, ctx.input(|i| i.modifiers)))
                }
                Some(egui::Event::PointerGone | egui::Event::WindowFocused(false)) => {
                    self.deactivate(app);
                    None
                }
                _ => None,
            };
            if let Some((ev, mods)) = event
                && let Err(e) = self.pointer(app, ev, mods)
            {
                app.ui.status = e.to_string();
                app.ui.status_error = true;
            }
        }
        if let Some((at, previous)) = self.last {
            let mods = ctx.input(|i| i.modifiers);
            if previous != mods
                && self.capture.is_some()
                && let Err(e) = self.pointer(app, ToolEvent::Move { x: at.x, y: at.y, pressure: 1. }, mods)
            {
                app.ui.status = e.to_string();
                app.ui.status_error = true;
            }
        }
        if self.pen.gesture.is_some() && !ctx.text_edit_focused() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, egui::Key::Enter)) {
            if let Err(e) = self.pen.enter(&mut app.session) {
                app.ui.status = e.to_string();
                app.ui.status_error = true;
            }
            self.capture = None;
            self.mapping = None;
            self.snap_cache = None;
            self.snap_result = None;
        }
        true
    }
    /// Borrowed detached preview: render it with a transient key; do not clone it per frame.
    pub fn preview(&self) -> Option<(&Document, u64)> {
        self.pen
            .preview
            .as_ref()
            .map(|doc| (doc, preview_key(self.pen.generation, 1)))
            .or_else(|| self.paint.preview.as_ref().map(|doc| (doc, preview_key(self.paint.generation, 2))))
            .or_else(|| self.path_text.preview.as_ref().map(|doc| (doc, preview_key(self.path_text.generation, 3))))
            .or_else(|| self.pencil.preview().map(|p| (p.document.as_ref(), preview_key(self.pencil.preview_generation(), 4))))
            .or_else(|| self.layout.preview.as_deref().map(|doc| (doc, preview_key(self.layout.generation, 5))))
            .or_else(|| self.corners.preview_document().map(|doc| (doc, preview_key(self.corners.preview_generation(), 6))))
    }
    pub fn shape_preview(&self) -> Option<(&photocraft_doc::Layer, u64)> {
        self.drawing.preview_layer.as_ref().map(|layer| (layer, preview_key(self.drawing.generation, 7)))
    }
    pub fn context(&mut self, app: &mut PhotocraftApp, ui: &mut egui::Ui) {
        match registry::handler(self.id.unwrap_or("")) {
            Some(Handler::Pen) => pen::context(ui, &mut self.pen),
            Some(Handler::VectorPencil) => pencil::context_bar(ui, &mut self.pencil),
            Some(Handler::Corner) => self.corners.context_bar(ui, &mut app.session),
            Some(Handler::Node) => crate::direct_select::context_ui(app, ui),
            Some(Handler::Shape) => {
                let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
                drawing::controls(ui, &mut app.session, &mut self.drawing, &ids, str::to_owned);
            }
            Some(Handler::VectorPaint) => {
                let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
                if let Err(e) = appearance::panel(ui, &mut self.paint, &mut app.session, &ids) {
                    app.ui.status = e.to_string();
                    app.ui.status_error = true;
                }
            }
            Some(Handler::PathText) => {
                if let Err(e) = path_text::context(ui, &mut self.path_text, &mut app.session) {
                    app.ui.status = e.to_string();
                    app.ui.status_error = true;
                }
            }
            _ => {}
        }
    }
    pub fn overlay(&self, painter: &egui::Painter, xf: &ViewXform, app: &PhotocraftApp) {
        let screen = |p: Point| xf.to_screen(p.x as f32, p.y as f32);
        pen::paint_overlay(&self.pen, painter, screen);
        if registry::handler(self.id.unwrap_or("")) == Some(Handler::Shape) {
            drawing::paint_mapped(painter, &self.drawing, screen, &app.session, app.session.active().and_then(|s| s.active_layer));
        }
        path_text::paint_overlay(&self.path_text, painter, screen);
        if let Some(snap) = &self.snap_result {
            crate::vector_packages::snapping::paint(painter, &snap.guides, |p| screen(Point::new(p[0], p[1])), |v| format!("{v:.1}"));
        }
    }
}
fn preview_key(generation: u64, family: u64) -> u64 {
    generation | (1 << 58) | (family << 59)
}
fn point(ev: ToolEvent) -> Point {
    match ev {
        ToolEvent::Down { x, y, .. } | ToolEvent::Move { x, y, .. } | ToolEvent::Up { x, y } => Point::new(x, y),
    }
}
pub fn paints(app: &PhotocraftApp) -> (Option<Fill>, Option<ShapeStroke>) {
    let fg = app.session.tools.foreground;
    let bg = app.session.tools.background;
    (
        app.ui.tool_options.shape_fill.then(|| Fill::Solid(Color::rgba(fg[0], fg[1], fg[2], fg[3]))),
        (app.ui.tool_options.stroke_width > 0.).then(|| ShapeStroke {
            width: app.ui.tool_options.stroke_width,
            paint: Fill::Solid(Color::rgba(bg[0], bg[1], bg[2], bg[3])),
            ..Default::default()
        }),
    )
}
fn foreground_stroke(app: &PhotocraftApp, width: f32) -> ShapeStroke {
    let fg = app.session.tools.foreground;
    ShapeStroke { width: width.max(1.), paint: Fill::Solid(Color::rgba(fg[0], fg[1], fg[2], fg[3])), ..Default::default() }
}
pub fn shape_paints(app: &PhotocraftApp, parameters: &photocraft_doc::vector_design::drawing::ShapeParameters) -> (Option<Fill>, Option<ShapeStroke>) {
    if photocraft_engine::vector_packages::drawing::geometry::stroke_only(parameters) {
        let width = if matches!(parameters, photocraft_doc::vector_design::drawing::ShapeParameters::Line { .. }) {
            app.ui.tool_options.line_weight
        } else {
            app.ui.tool_options.stroke_width
        };
        (None, Some(foreground_stroke(app, width)))
    } else {
        paints(app)
    }
}
pub fn pen_paints(app: &PhotocraftApp, mode: photocraft_engine::vector_packages::pen::Mode) -> (Option<Fill>, Option<ShapeStroke>) {
    if mode == photocraft_engine::vector_packages::pen::Mode::Line {
        (None, Some(foreground_stroke(app, app.ui.tool_options.stroke_width)))
    } else {
        paints(app)
    }
}
pub fn pen_pointer(
    state: &mut pen::State,
    s: &mut Session,
    ev: ToolEvent,
    mods: Modifiers,
    shape_mode: bool,
    mask: bool,
    zoom: f64,
    paint: (Option<Fill>, Option<ShapeStroke>),
) -> Result<()> {
    let at = point(ev);
    let tolerance = 6. / zoom.max(0.01);
    let result = match ev {
        ToolEvent::Down { .. } => {
            let target = pen::destination(s, at, tolerance, shape_mode, mask, state.options.add_to_selected)?;
            state.down(s, target, at, tolerance, mods, paint.0, paint.1)
        }
        ToolEvent::Move { .. } => state.motion(s, at, mods),
        ToolEvent::Up { .. } => state.motion(s, at, mods).and_then(|()| state.up(s)),
    };
    if result.is_err() {
        state.cancel();
    }
    result
}
fn pen_event(state: &mut State, app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> Result<()> {
    let paint = pen_paints(app, state.pen.options.mode);
    let zoom = f64::from(app.current_zoom());
    let origin = state.pen.gesture.as_ref().map(|g| g.constraint_origin());
    let at = origin.map_or(point(ev), |origin| photocraft_engine::vector_packages::pen::constrain(origin, point(ev), mods.shift));
    let at = snap_point(state, at, mods, zoom, origin.map(|p| (p, std::f64::consts::FRAC_PI_4, 0.)))?;
    let ev = with_point(ev, at);
    pen_pointer(&mut state.pen, &mut app.session, ev, mods, state.shape_mode, app.ui.vector_mask_target, zoom * state.pixels_per_point.unwrap_or(1.), paint)
}
fn shape_event(state: &mut State, app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> Result<()> {
    let ev = match ev {
        ToolEvent::Down { .. } => drawing::Event::Down(point(ev)),
        ToolEvent::Move { .. } => drawing::Event::Move(point(ev)),
        ToolEvent::Up { .. } => drawing::Event::Up(point(ev)),
    };
    let selected = app.session.active().and_then(|s| s.active_layer);
    let zoom = f64::from(app.current_zoom());
    if matches!(ev, drawing::Event::Down(_)) {
        state.drawing.paint = Some(shape_paints(app, &state.drawing.parameters));
    }
    let cache = state.snap_cache.as_ref();
    let settings = &state.snapping;
    let view = photocraft_engine::vector_packages::snapping::query::View { zoom, pixels_per_point: state.pixels_per_point.unwrap_or(1.) };
    let latest = std::cell::RefCell::new(None);
    let line = matches!(
        state.drawing.parameters,
        photocraft_doc::vector_design::drawing::ShapeParameters::Line { .. } | photocraft_doc::vector_design::drawing::ShapeParameters::Arrow { .. }
    );
    drawing::pointer_checked(&mut app.session, &mut state.drawing, ev, mods, selected, true, 7. / zoom.max(0.01), |p, start, shift| {
        if let Some(cache) = cache {
            let angle = shift.then_some(photocraft_engine::vector_packages::snapping::query::Angle {
                origin: [start.x, start.y],
                step: if line { std::f64::consts::FRAC_PI_4 } else { std::f64::consts::FRAC_PI_2 },
                offset: if line { 0. } else { std::f64::consts::FRAC_PI_4 },
            });
            let snap = query_snap(cache, [p.x, p.y], settings, view, angle, mods.command || mods.ctrl)?;
            let at = Point::new(snap.point[0], snap.point[1]);
            *latest.borrow_mut() = Some(snap);
            Ok(at)
        } else {
            Ok(p)
        }
    })?;
    state.snap_result = latest.into_inner();
    Ok(())
}
fn with_point(ev: ToolEvent, p: Point) -> ToolEvent {
    match ev {
        ToolEvent::Down { pressure, .. } => ToolEvent::Down { x: p.x, y: p.y, pressure },
        ToolEvent::Move { pressure, .. } => ToolEvent::Move { x: p.x, y: p.y, pressure },
        ToolEvent::Up { .. } => ToolEvent::Up { x: p.x, y: p.y },
    }
}
fn snap_point(state: &mut State, p: Point, mods: Modifiers, zoom: f64, construction: Option<(Point, f64, f64)>) -> Result<Point> {
    use photocraft_engine::vector_packages::snapping::query::{Angle, View};
    let Some(cache) = &state.snap_cache else { return Ok(p) };
    let angle = mods.shift.then(|| construction.map(|(origin, step, offset)| Angle { origin: [origin.x, origin.y], step, offset })).flatten();
    let snap = query_snap(
        cache,
        [p.x, p.y],
        &state.snapping,
        View { zoom, pixels_per_point: state.pixels_per_point.unwrap_or(1.) },
        angle,
        mods.command || mods.ctrl,
    )?;
    let at = Point::new(snap.point[0], snap.point[1]);
    state.snap_result = Some(snap);
    Ok(at)
}
fn query_snap(
    cache: &photocraft_engine::vector_packages::snapping::query::Cache,
    point: [f64; 2],
    settings: &photocraft_engine::vector_packages::snapping::query::Settings,
    view: photocraft_engine::vector_packages::snapping::query::View,
    angle: Option<photocraft_engine::vector_packages::snapping::query::Angle>,
    bypass: bool,
) -> Result<photocraft_engine::vector_packages::snapping::query::SnapResult> {
    if angle.is_some() {
        // Manual Shift construction is mandatory even if automatic angle guides are off.
        let mut constrained = settings.clone();
        constrained.angle = true;
        cache.point(point, &constrained, view, angle, bypass)
    } else {
        cache.point(point, settings, view, angle, bypass)
    }
}
fn layout_event(state: &mut State, app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> Result<()> {
    let view = photocraft_engine::vector_packages::snapping::query::View {
        zoom: f64::from(app.current_zoom()),
        pixels_per_point: state.pixels_per_point.unwrap_or(1.),
    };
    let tool = if state.id == Some("VectorCrop") { layout::Tool::Crop } else { layout::Tool::PointTransform };
    state.layout.switch(tool);
    state.layout.synchronize(&app.session, true)?;
    let at = point(ev);
    match ev {
        ToolEvent::Down { .. } => state.layout.down(&app.session, at, 6. / view.zoom.max(0.01), view).map(|_| ()),
        ToolEvent::Move { .. } => state.layout.motion(&app.session, at, mods, view),
        ToolEvent::Up { .. } => state.layout.up(&mut app.session, at, mods, view),
    }
}
fn pencil_event(state: &mut State, app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> Result<()> {
    let zoom = app.current_zoom();
    state.pencil.pointer(&mut app.session, ev, mods, zoom)
}
fn node_event(_state: &mut State, app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> Result<()> {
    crate::direct_select::pointer(app, ev, mods);
    Ok(())
}
fn corner_event(state: &mut State, app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> Result<()> {
    let zoom = f64::from(app.current_zoom());
    state.corners.active = true;
    state.corners.layer = app.session.active().and_then(|s| s.active_layer);
    state.corners.pointer(&mut app.session, ev, mods, zoom);
    if let Some(e) = &state.corners.error {
        return Err(EngineError::Other(e.clone()));
    }
    Ok(())
}
fn paint_event(state: &mut State, app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> Result<()> {
    state.paint.options.tool = if state.id == Some("Transparency") { appearance::Tool::Transparency } else { appearance::Tool::Fill };
    let at = point(ev);
    let zoom = f64::from(app.current_zoom());
    match ev {
        ToolEvent::Down { .. } => {
            state.paint.begin_handle(&app.session, &app.session.active().map(|s| s.selected_layers()).unwrap_or_default(), at, 6. / zoom.max(0.01))
        }
        ToolEvent::Move { .. } => state.paint.motion_handle(&app.session, at, mods.shift),
        ToolEvent::Up { .. } => state.paint.motion_handle(&app.session, at, mods.shift).and_then(|()| state.paint.release(&mut app.session)),
    }
}
fn path_text_event(state: &mut State, app: &mut PhotocraftApp, ev: ToolEvent, _mods: Modifiers) -> Result<()> {
    let at = point(ev);
    let zoom = f64::from(app.current_zoom());
    match ev {
        ToolEvent::Down { .. } => state.path_text.down(&app.session, at, 6. / zoom.max(0.01), Default::default()).map(|_| ()),
        ToolEvent::Move { .. } => state.path_text.motion(&app.session, at),
        ToolEvent::Up { .. } => state.path_text.motion(&app.session, at).and_then(|()| state.path_text.up(&mut app.session)),
    }
}
fn existing_event(state: &mut State, app: &mut PhotocraftApp, mut ev: ToolEvent, mods: Modifiers) -> Result<()> {
    // Existing Move/Path/Type/Crop/Hand/Zoom/raster paths are already persona-neutral.
    // Artistic/Frame defaults use the same Type handler and editing session.
    let id = state.id.unwrap_or("");
    let tool = crate::Tool::from_name(id).or_else(|| matches!(id, "ArtText" | "FrameText").then_some(crate::Tool::Type));
    let Some(tool) = tool else {
        return Err(EngineError::Other(format!("{id} requires its feature provider")));
    };
    if tool.is_type() {
        if let ToolEvent::Down { x, y, .. } = ev {
            state.text_start = Some([x, y]);
        }
        if let ToolEvent::Up { x: _, y: _ } = ev
            && let Some(start) = state.text_start
            && id == "ArtText"
        {
            ev = ToolEvent::Up { x: start[0], y: start[1] };
        }
    }
    let old = app.ui.tool;
    app.ui.tool = tool;
    crate::canvas::tool_event(app, ev, mods);
    app.ui.tool = old;
    Ok(())
}
pub fn commit_legacy_pen(app: &mut PhotocraftApp, path: photocraft_doc::Path) -> Result<()> {
    let at = path.subpaths.first().and_then(|s| s.knots.first()).map_or(Point::new(0., 0.), |k| k.anchor);
    let target = pen::destination(
        &app.session,
        at,
        6. / f64::from(app.current_zoom()).max(0.01),
        app.ui.tool_options.vector_mode == "shape",
        app.ui.vector_mask_target,
        false,
    )?;
    let st = app.session.active().ok_or(EngineError::NoDocument)?;
    let (fill, stroke) = paints(app);
    let commit = photocraft_engine::vector_packages::pen::Commit { document: st.doc.id.0, revision: st.revision, target, path, fill, stroke };
    app.session.execute("path.penCommit", serde_json::to_value(commit).map_err(|e| EngineError::Other(e.to_string()))?).map(|_| ())
}
#[cfg(test)]
mod tests;
