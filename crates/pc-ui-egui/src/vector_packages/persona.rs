//! Persona shell on the existing toolset store. Original Local Image code; UX follows the
//! owner decisions in vector-persona-spec §6 and Affinity's documented persona switcher.
//! Feature tools register independently: ids never alias raster Pencil/Gradient/Type.

#[cfg(test)]
use egui::Key;
use egui::{Context, Modifiers};
use photocraft_doc::{DocId, LayerContent, LayerId};
use photocraft_engine::toolsets::{Persona, VECTOR_TOOLS, supports};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::state::Tool;

/// Each feature supplies these exact entrypoints to 00's serial router. A missing hook is
/// unavailable; it must not fall through to a raster tool's input handler.
pub struct FeatureHooks {
    pub pointer: fn(&mut PhotocraftApp, crate::canvas::ToolEvent, Modifiers) -> bool,
    pub context: fn(&mut PhotocraftApp, &mut egui::Ui) -> bool,
    pub overlay: fn(&PhotocraftApp, &egui::Painter, &crate::canvas::ViewXform),
    pub cancel: fn(&mut PhotocraftApp),
}

#[derive(Clone)]
struct Observed {
    persona: Persona,
    tool: Tool,
    pixel_mode: String,
    gesture: Option<GestureTarget>,
}

/// Captured before a gesture or a delayed operation. Revision, document and the entire target
/// set must still match at release. Feature packages can reuse this for detached previews.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GestureTarget {
    pub doc: DocId,
    pub revision: u64,
    pub active: Option<LayerId>,
    pub layers: Vec<LayerId>,
}

impl GestureTarget {
    pub fn capture(app: &PhotocraftApp) -> Option<Self> {
        let st = app.session.active()?;
        Some(Self { doc: st.doc.id, revision: st.revision, active: st.active_layer, layers: st.selected_layers() })
    }

    pub fn is_current(&self, app: &PhotocraftApp) -> bool {
        app.session
            .active()
            .is_some_and(|st| st.doc.id == self.doc && st.revision == self.revision && st.active_layer == self.active && st.selected_layers() == self.layers)
    }
}

/// 00 stores this as a serde-skipped field in UiState and invokes before the shared canvas
/// router, including automation. Unlike a frame observer, it protects a same-frame release.
#[derive(Clone, Debug, Default)]
pub struct PointerGuard {
    target: Option<GestureTarget>,
    tool: Option<Tool>,
}

impl PointerGuard {
    /// True consumes invalid/stale input without executing any engine edit.
    pub fn intercept(&mut self, app: &mut PhotocraftApp, ev: crate::canvas::ToolEvent) -> bool {
        use crate::canvas::ToolEvent;
        if matches!(ev, ToolEvent::Down { .. }) && app.ui.tool == Tool::Pen && app.ui.pen.is_none() {
            self.clear();
        }
        let (x, y, pressure) = match ev {
            ToolEvent::Down { x, y, pressure } | ToolEvent::Move { x, y, pressure } => (x, y, Some(pressure)),
            ToolEvent::Up { x, y } => (x, y, None),
        };
        let invalid =
            !x.is_finite() || !y.is_finite() || x.abs() > 1e9 || y.abs() > 1e9 || pressure.is_some_and(|p| !p.is_finite() || !(0.0..=1.0).contains(&p));
        let stale = self.target.as_ref().is_some_and(|target| !target.is_current(app)) || self.tool.is_some_and(|tool| tool != app.ui.tool);
        if invalid || stale {
            self.target = None;
            self.tool = None;
            app.ui.pen = None;
            app.ui.direct_selection.drag = None;
            app.drag = None;
            app.live_stroke = None;
            app.move_preview = None;
            app.move_fast = None;
            app.ui.status = tl!("The gesture was cancelled; try again").into();
            return true;
        }
        // Pen retains a draft between clicks. The established Move and raster
        // handlers capture after their own selection/layer setup; do not capture
        // their pre-press selection here and invalidate that setup on the next move.
        if matches!(ev, ToolEvent::Down { .. }) && app.ui.tool == Tool::Pen && (self.target.is_none() || app.ui.pen.is_none()) {
            self.target = GestureTarget::capture(app);
            self.tool = Some(app.ui.tool);
        }
        if matches!(ev, ToolEvent::Up { .. }) && app.ui.tool != Tool::Pen {
            self.target = None;
            self.tool = None;
        }
        false
    }

    pub fn clear(&mut self) {
        self.target = None;
        self.tool = None;
    }
}

/// Add this capture to the existing rasterization dialog at open; 00 calls the paired check
/// before its existing rasterize-and-paint confirmation. All metadata stays out of UI text.
pub fn capture_rasterize_prompt(app: &PhotocraftApp, fields: &mut serde_json::Map<String, Value>) -> Result<(), String> {
    let target = GestureTarget::capture(app).ok_or_else(|| tl!("No document").to_string())?;
    fields.insert(
        "__personaTarget".into(),
        json!({"doc":target.doc.0,"revision":target.revision,"active":target.active.map(|id|id.0),
        "layers":target.layers.iter().map(|id|id.0).collect::<Vec<_>>() }),
    );
    Ok(())
}

pub fn validate_rasterize_prompt(app: &PhotocraftApp, fields: &serde_json::Map<String, Value>) -> Result<(), String> {
    let current = GestureTarget::capture(app).ok_or_else(|| tl!("No document").to_string())?;
    let expected = json!({"doc":current.doc.0,"revision":current.revision,"active":current.active.map(|id|id.0),
        "layers":current.layers.iter().map(|id|id.0).collect::<Vec<_>>() });
    if fields.get("__personaTarget") != Some(&expected) {
        return Err(tl!("The document or selection changed; try again").into());
    }
    Ok(())
}

/// Drop detached previews and pointer capture; switching personas never commits a gesture.
pub fn cancel(app: &mut PhotocraftApp, ctx: &Context) {
    crate::vector_integration::cancel(app);
    app.ui.persona_pointer.clear();
    for view in &mut app.ui.views {
        view.rotation_drag = None;
    }
    for window in &mut app.ui.windows {
        window.view.rotation_drag = None;
    }
    app.ui.pen = None;
    app.ui.direct_selection.drag = None;
    app.live_stroke = None;
    crate::move_input::cancel(app, ctx);
    crate::magnetic_lasso_ui::cancel(app);
    crate::perspective_crop_ui::cancel();
    app.ui.polygon.clear();
    app.ui.polygon_mode.clear();
    app.crop.drag = None;
    app.gradient = Default::default();
    app.brush_resize = None;
    app.alt_sampling = false;
    ctx.data_mut(|d| {
        d.remove::<(egui::Id, egui::Rect)>(egui::Id::new("tool-flyout"));
        d.remove::<egui::Id>(egui::Id::new("tool-flyout").with("held"));
    });
}

fn memory_key() -> egui::Id {
    egui::Id::new("vector-persona-observed")
}

/// Called before chrome/canvas each frame (00 also calls this when chrome is hidden). The
/// owned toolbar and context bar use it already. Automation changes use the same prefs.
pub fn frame(app: &mut PhotocraftApp, ctx: &Context) {
    if let Some(document) = app.ui.photo_admission.take()
        && app.session.active().is_some_and(|st| st.doc.id == document)
    {
        cancel(app, ctx);
        app.session.edit_prefs(|prefs| {
            prefs.toolbar.persona = Persona::Pixel;
            prefs.toolbar.remember_tool(Persona::Pixel, "Move");
        });
        app.ui.tool = Tool::Move;
    }
    sync(app, ctx);
}

/// After successful photo admission/focus, prepare Pixel/Move for that document only.
/// The frame rechecks the identity before cancelling a gesture or changing persona.
/// Hosts must pass the intended photo's DocId, never infer it after an asynchronous wait.
pub fn photo_admitted(app: &mut PhotocraftApp, document: DocId) -> bool {
    if !app.session.active().is_some_and(|st| st.doc.id == document) {
        return false;
    }
    app.ui.photo_admission = Some(document);
    true
}

pub(crate) fn contains_photo(doc: &photocraft_doc::Document) -> bool {
    doc.walk().iter().any(|(_, _, layer)| matches!(layer.content, LayerContent::Raster(_)) || crate::develop_layer::is_develop_layer(layer))
}

pub fn select(app: &mut PhotocraftApp, ctx: &Context, persona: Persona) {
    app.ui.photo_admission = None;
    sync(app, ctx);
    let previous = app.session.prefs().toolbar.persona;
    if persona == previous {
        return;
    }
    app.session.edit_prefs(|pr| pr.toolbar.persona = persona);
    sync(app, ctx);
}

pub fn select_tool(app: &mut PhotocraftApp, ctx: &Context, tool: Tool) {
    app.ui.photo_admission = None;
    if app.ui.tool == tool {
        return;
    }
    cancel(app, ctx);
    app.ui.persona_pointer.clear();
    app.ui.tool = tool;
    sync(app, ctx);
}

fn sync(app: &mut PhotocraftApp, ctx: &Context) {
    let persona = app.session.prefs().toolbar.persona;
    let mut seen = ctx.data(|d| d.get_temp::<Observed>(memory_key()));
    let tool_changed = seen.as_ref().is_some_and(|s| s.persona == persona && s.tool != app.ui.tool);
    if seen.as_ref().is_some_and(|s| s.persona == persona && s.tool == app.ui.tool) {
        let saved = app.session.prefs().toolbar.last_tool(persona);
        if let Some(tool) = Tool::from_name(saved).filter(|t| supports(&format!("{t:?}"), persona)) {
            app.ui.tool = tool;
        }
    }
    let changed = seen.as_ref().is_some_and(|s| s.persona != persona || (s.tool != app.ui.tool && s.gesture.is_some()));
    let stale = seen.as_ref().and_then(|s| s.gesture.as_ref()).is_some_and(|g| !g.is_current(app));
    if changed || stale {
        cancel(app, ctx);
    }
    if seen.as_ref().is_none_or(|s| s.persona != persona) {
        let mut pixel_mode = seen.as_ref().map_or_else(|| app.ui.tool_options.vector_mode.clone(), |s| s.pixel_mode.clone());
        if let Some(old) = &seen
            && old.persona == Persona::Pixel
        {
            pixel_mode = app.ui.tool_options.vector_mode.clone();
        }
        let saved = app.session.prefs().toolbar.last_tool(persona);
        let restored = Tool::from_name(saved).filter(|t| supports(&format!("{t:?}"), persona));
        if seen.is_some() {
            app.ui.tool = restored.unwrap_or_else(|| if supports(&format!("{:?}", app.ui.tool), persona) { app.ui.tool } else { Tool::Move });
        }
        if persona == Persona::Vector {
            app.ui.tool_options.vector_mode = "shape".into();
        } else {
            app.ui.tool_options.vector_mode = pixel_mode.clone();
        }
        seen = Some(Observed { persona, tool: app.ui.tool, pixel_mode, gesture: None });
    }
    // A tool reached by a shortcut/menu remains reachable. If it belongs exclusively to the
    // other family, select that persona instead of injecting Brush into the Vector toolbar.
    if !supports(&format!("{:?}", app.ui.tool), persona) {
        let other = if persona == Persona::Pixel { Persona::Vector } else { Persona::Pixel };
        if supports(&format!("{:?}", app.ui.tool), other) {
            let tool = format!("{:?}", app.ui.tool);
            app.session.edit_prefs(|pr| {
                pr.toolbar.remember_tool(other, &tool);
                pr.toolbar.persona = other;
            });
            sync(app, ctx);
            return;
        }
    }
    let tool = format!("{:?}", app.ui.tool);
    let saved = app.session.prefs().toolbar.last_tool(persona);
    let reserved = supports(saved, persona) && Tool::from_name(saved).is_none();
    if saved != tool && (!reserved || tool_changed) {
        app.session.edit_prefs(|pr| {
            pr.toolbar.remember_tool(persona, &tool);
        });
    }
    if let Some(mut seen) = seen {
        seen.tool = app.ui.tool;
        if app.ui.pen.is_some() {
            if seen.gesture.is_none() {
                seen.gesture = GestureTarget::capture(app);
            }
        } else {
            seen.gesture = None;
        }
        ctx.data_mut(|d| d.insert_temp(memory_key(), seen));
    }
}

/// Labelled segmented switcher at the fixed left edge of the Compositing options row.
pub fn switcher(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let active = app.session.prefs().toolbar.persona;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for (persona, label, tip) in [(Persona::Pixel, tl!("Pixel"), tl!("Pixel persona")), (Persona::Vector, tl!("Vector"), tl!("Vector persona"))] {
            let r = ui.add(egui::Button::selectable(active == persona, label));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
            if r.on_hover_text(tip).clicked() {
                select(app, ui.ctx(), persona);
            }
        }
    });
}

/// Stable vector slots, resolved through the shell registry after 00 adds the reserved Tool
/// variants. Existing tools work now; no absent tool is silently mapped to a raster tool.
pub fn vector_slots() -> Vec<(usize, Vec<Tool>)> {
    let registered: Vec<_> = VECTOR_TOOLS.iter().map(|entry| entry.id).filter(|id| Tool::from_name(id).is_some()).collect();
    photocraft_engine::vector_toolbar_layout::groups(&registered, None)
        .into_iter()
        .map(|group| (group.slot, group.tools.into_iter().filter_map(Tool::from_name).collect()))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geometry {
    Add,
    Subtract,
    Intersect,
    Xor,
    Divide,
    Trim,
    Merge,
    Crop,
    Outline,
    MinusBack,
}

impl Geometry {
    pub const ALL: [Self; 10] =
        [Self::Add, Self::Subtract, Self::Intersect, Self::Xor, Self::Divide, Self::Trim, Self::Merge, Self::Crop, Self::Outline, Self::MinusBack];
    pub fn label(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
            Self::Xor => "Xor",
            Self::Divide => "Divide",
            Self::Trim => "Trim",
            Self::Merge => "Merge",
            Self::Crop => "Crop",
            Self::Outline => "Outline",
            Self::MinusBack => "Minus Back",
        }
    }
    pub fn command(self) -> &'static str {
        match self {
            Self::Add | Self::Subtract | Self::Intersect | Self::Xor | Self::Divide => "path.boolean",
            _ => "path.pathfinder",
        }
    }
    pub fn op(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Subtract => "subtract",
            Self::Intersect => "intersect",
            Self::Xor => "xor",
            Self::Divide => "divide",
            Self::Trim => "trim",
            Self::Merge => "merge",
            Self::Crop => "crop",
            Self::Outline => "outline",
            Self::MinusBack => "minusBack",
        }
    }
}

#[derive(Clone)]
pub struct GeometryRequest {
    pub target: GestureTarget,
    pub op: Geometry,
    pub compound: bool,
}

impl GeometryRequest {
    pub fn capture(app: &PhotocraftApp, op: Geometry, compound: bool) -> Result<Self, String> {
        let target = GestureTarget::capture(app).ok_or_else(|| tl!("No document").to_string())?;
        if target.layers.len() < 2 {
            return Err(tl!("Select at least two editable shape layers").into());
        }
        let request = Self { target, op, compound };
        request.validate(app)?;
        Ok(request)
    }
    pub fn validate(&self, app: &PhotocraftApp) -> Result<(), String> {
        if !self.target.is_current(app) {
            return Err(tl!("The document or selection changed; try again").into());
        }
        let st = app.session.active().ok_or_else(|| tl!("No document").to_string())?;
        for id in &self.target.layers {
            if st.doc.effective_locks(*id).all || !st.doc.layer(*id).is_some_and(|l| matches!(l.content, LayerContent::Shape(_))) {
                return Err(tl!("Select at least two editable shape layers").into());
            }
        }
        if self.compound && (self.op.command() != "path.boolean" || self.op == Geometry::Divide) {
            return Err(tl!("This operation cannot create a compound").into());
        }
        Ok(())
    }
    pub fn params(&self) -> Value {
        json!({"layers": self.target.layers.iter().map(|id| id.0).collect::<Vec<_>>(), "op": self.op.op(), "keep_compound": self.compound})
    }
    pub fn execute(self, app: &mut PhotocraftApp) -> Result<Value, String> {
        self.validate(app)?;
        app.run(self.op.command(), self.params())
    }
}

/// Basic operations plus the Illustrator Pathfinder flyout. Alt is sampled on the actual click.
pub fn geometry_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let compound = ui.input(|i| i.modifiers.alt);
    for op in &Geometry::ALL[..5] {
        geometry_button(app, ui, *op, compound);
    }
    ui.menu_button(tl!("Pathfinder"), |ui| {
        for op in &Geometry::ALL[5..] {
            geometry_button(app, ui, *op, false);
        }
    });
    let enabled = app.session.is_enabled("file.svg.place");
    if ui.add_enabled(enabled, egui::Button::new(tl!("Place SVG…")).small()).on_hover_text(tl!("Place an editable SVG above the selected layer")).clicked() {
        let ctx = ui.ctx().clone();
        let _ = crate::menus::invoke(app, &ctx, "file.placeEmbedded", json!({}));
    }
}

fn geometry_button(app: &mut PhotocraftApp, ui: &mut egui::Ui, op: Geometry, compound: bool) {
    type Cached = (std::sync::Weak<photocraft_doc::Document>, u64, Option<GestureTarget>, bool, Result<GeometryRequest, String>, Option<String>);
    let key = ui.id().with(("geometry-button-eligibility", op.label()));
    let source = app.session.active().map(|st| std::sync::Arc::downgrade(&st.doc)).unwrap_or_default();
    let target = GestureTarget::capture(app);
    let prefs = app.session.prefs.rev();
    let cached: Option<Cached> = ui.data(|d| d.get_temp(key));
    let (request, reason) = if let Some((previous, revision, captured, alt, request, reason)) = cached
        && previous.ptr_eq(&source)
        && revision == prefs
        && captured == target
        && alt == compound
    {
        (request, reason)
    } else {
        let request = GeometryRequest::capture(app, op, compound);
        let reason = match &request {
            Ok(r) => app.session.disabled_reason_with(op.command(), &r.params()).map(|reason| {
                if photocraft_engine::commands::find(op.command()).is_none() { tl!("Geometry is not available yet").to_string() } else { reason }
            }),
            Err(reason) => Some(reason.clone()),
        };
        ui.data_mut(|d| d.insert_temp(key, (source, prefs, target, compound, request.clone(), reason.clone())));
        (request, reason)
    };
    let r = ui.add_enabled(reason.is_none(), egui::Button::new(tl!(op.label())).small());
    let clicked = r.on_hover_text(reason.unwrap_or_else(|| format!("{} ({})", tl!("Geometry"), tl!(op.label())))).clicked();
    if clicked
        && let Ok(request) = request
        && let Err(error) = request.execute(app)
    {
        app.ui.status = error;
        app.ui.status_error = true;
    }
}

#[cfg(test)]
#[path = "persona_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "persona_gpu_tests.rs"]
mod gpu_tests;
