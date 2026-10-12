//! Group Selection, vector Magic Wand and object/node Lasso. Gesture/modifier semantics
//! adapted from VectorCraft tools/xform/wand.rs and tools/select.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! The caller supplies the existing session and 03 DirectSelection; previews are detached.

use crate::direct_select::{DirectSelection, NodeSelection, PathRef};
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, Stroke, Ui};
use photocraft_doc::{DocId, LayerId};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::selection::{self as engine, Compare, Mode, Nodes, Object, Preview, Query, Readers, Request, Same, Scope, Wand},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Group,
    Wand,
    Lasso,
    Node,
}
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub origin: Pos2,
    pub document_origin: [f64; 2],
    pub zoom: f64,
    pub flip: bool,
    pub rotation: f32,
}
impl View {
    fn camera(self) -> crate::view_camera::Camera {
        crate::view_camera::Camera::new(Rect::from_center_size(self.origin, egui::Vec2::ZERO), self.zoom as f32, [0.0, 0.0], self.flip, self.rotation)
    }
    fn point(self, p: Pos2) -> [f64; 2] {
        let d = self.camera().to_document(p);
        [self.document_origin[0] + f64::from(d.x), self.document_origin[1] + f64::from(d.y)]
    }
    fn screen(self, p: [f64; 2]) -> Pos2 {
        self.camera().to_screen(egui::pos2((p[0] - self.document_origin[0]) as f32, (p[1] - self.document_origin[1]) as f32))
    }
    fn valid(self) -> bool {
        self.rotation.is_finite()
            && self.zoom.is_finite()
            && (1e-6..=1e6).contains(&self.zoom)
            && self.origin.x.is_finite()
            && self.origin.y.is_finite()
            && self.document_origin.iter().all(|v| v.is_finite() && v.abs() <= 1e7)
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub wand: Wand,
    pub select_hidden: bool,
    pub compare: Compare,
    pub value: Option<f64>,
    pub item: Option<usize>,
}
#[derive(Clone)]
struct Capture {
    request: Request,
    base: Vec<LayerId>,
    node_base: Vec<Nodes>,
    points: Vec<[f64; 2]>,
    tool: Tool,
    node_mode: bool,
    view: View,
}
#[derive(Clone, Default)]
pub struct State {
    pub settings: Settings,
    capture: Option<Capture>,
    pub preview: Option<Preview>,
    pub selection: Option<Preview>,
    pub error: Option<String>,
    group: Option<(DocId, u64, LayerId, LayerId)>,
    /// Per-document view memory. The coordinator should use engine Hooks to share this
    /// with non-canvas menu/MCP selection changes, rather than a second selection store.
    pub previous: HashMap<DocId, Vec<LayerId>>,
}
impl State {
    pub fn captured(&self) -> bool {
        self.capture.is_some()
    }
    pub fn cancel(&mut self) {
        self.capture = None;
        self.preview = None;
    }
}
fn mode(mods: Modifiers, group: bool) -> Mode {
    if mods.alt && !group {
        Mode::Subtract
    } else if mods.shift {
        Mode::Add
    } else {
        Mode::Replace
    }
}
fn node_base(d: &DirectSelection) -> Vec<Nodes> {
    d.target
        .iter()
        .map(|t| (*t, &d.anchors))
        .chain(d.additional.iter().map(|n| (n.target, &n.anchors)))
        .filter_map(|(t, a)| match t {
            PathRef::Layer(id) => Some(Nodes { layer: LayerId(id), anchors: a.clone() }),
            PathRef::Work => None,
        })
        .collect()
}
fn same_node_base(base: &[Nodes], d: &DirectSelection) -> bool {
    let mut current = d.target.iter().map(|t| (*t, &d.anchors)).chain(d.additional.iter().map(|n| (n.target, &n.anchors))).filter_map(|(t, a)| match t {
        PathRef::Layer(id) => Some((LayerId(id), a)),
        PathRef::Work => None,
    });
    base.iter().all(|b| current.next().is_some_and(|(id, anchors)| b.layer == id && b.anchors == *anchors)) && current.next().is_none()
}
/// Use after object-selection publication in shared menu/shortcut adapters as well.
pub fn clear_nodes(d: &mut DirectSelection, document: DocId) {
    *d = DirectSelection { document: Some(document.0), pixels_per_point: d.pixels_per_point, transform_mode: d.transform_mode, ..Default::default() };
}
fn publish_nodes(d: &mut DirectSelection, hits: Vec<Nodes>, base: &[Nodes], mode: Mode, doc: DocId) {
    let mut output = Vec::new();
    for mut hit in hits {
        let old = base.iter().find(|n| n.layer == hit.layer).map(|n| n.anchors.as_slice()).unwrap_or_default();
        match mode {
            Mode::Replace => {}
            Mode::Add => hit.anchors.extend(old.iter().copied()),
            Mode::Subtract => {
                let removed: std::collections::HashSet<_> = hit.anchors.iter().copied().collect();
                hit.anchors = old.iter().copied().filter(|a| !removed.contains(a)).collect();
            }
        }
        hit.anchors.sort_unstable();
        hit.anchors.dedup();
        if !hit.anchors.is_empty() {
            output.push(hit);
        }
    }
    d.target = output.first().map(|n| PathRef::Layer(n.layer.0));
    d.anchors = output.first().map(|n| n.anchors.clone()).unwrap_or_default();
    d.additional = output.into_iter().skip(1).map(|n| NodeSelection { target: PathRef::Layer(n.layer.0), anchors: n.anchors }).collect();
    d.segment = None;
    d.drag = None;
    d.document = Some(doc.0);
    d.preview_doc = None;
}
fn evaluate(session: &Session, c: &mut Capture, mods: Modifiers, readers: Readers) -> Result<Option<Preview>> {
    c.request.mode = mode(mods, c.tool == Tool::Group);
    if c.tool == Tool::Lasso {
        if c.points.len() < 3 {
            return Ok(None);
        }
        c.request.query = Query::Lasso { points: c.points.clone(), nodes: c.node_mode };
    }
    let st = session.active().ok_or(EngineError::NoDocument)?;
    if st.selected_layers() != c.base {
        return Err(EngineError::Other("stale vector selection gesture".into()));
    }
    engine::preview(&st.doc, st.revision, &c.base, &c.request, readers).map(Some)
}
/// Call once per active canvas frame before the old tool dispatch. True consumes this
/// frame's object selection events; the router still paints the canvas. Node+Alt routes
/// here temporarily. Set node_mode for Q while the Node tool is the lasso's origin.
#[allow(clippy::too_many_arguments)]
pub fn canvas(
    ui: &mut Ui,
    session: &mut Session,
    state: &mut State,
    direct: &mut DirectSelection,
    rect: Rect,
    view: View,
    tool: Tool,
    node_mode: bool,
    scope: Scope,
    readers: Readers,
    cancel: bool,
) -> bool {
    let scope = match engine::resolve_scope(session, &scope) {
        Ok(scope) => scope,
        Err(e) => {
            state.error = Some(e.to_string());
            state.cancel();
            return tool != Tool::Node;
        }
    };
    let (events, mods, focused, escape) = ui.input(|i| (i.events.clone(), i.modifiers, i.focused, i.key_pressed(egui::Key::Escape)));
    let effective = if tool == Tool::Node && mods.alt { Tool::Group } else { tool };
    let stale = state.capture.as_ref().is_some_and(|c| {
        c.tool != effective
            || c.node_mode != node_mode
            || (c.node_mode && !same_node_base(&c.node_base, direct))
            || session.active().is_none_or(|st| st.doc.id != c.request.document || st.revision != c.request.revision || st.selected_layers() != c.base)
            || c.request.scope != scope
    });
    if cancel || !focused || escape || stale || !view.valid() {
        if stale {
            state.error = Some("stale vector selection gesture".into());
        }
        state.cancel();
        return effective != Tool::Node;
    }
    if effective == Tool::Node {
        return false;
    }
    let result = (|| -> Result<()> {
        for event in events {
            match event {
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers }
                    if rect.contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id()) =>
                {
                    let st = session.active().ok_or(EngineError::NoDocument)?;
                    let base = st.selected_layers();
                    let p = view.point(pos);
                    let query = if effective == Tool::Lasso {
                        Query::Deselect
                    } else {
                        let hit = engine::hit(&st.doc, &scope, p, 6. / view.zoom, readers)?;
                        match (effective, hit) {
                            (Tool::Group, Some(leaf)) => {
                                let previous = state
                                    .group
                                    .filter(|(doc, rev, old, selected)| *doc == st.doc.id && *rev == st.revision && *old == leaf && base.contains(selected))
                                    .map(|(_, _, _, id)| id);
                                Query::Group { leaf, previous }
                            }
                            (Tool::Wand, Some(reference)) => Query::Wand { reference, settings: state.settings.wand.clone() },
                            (_, None) if modifiers.shift || (modifiers.alt && effective != Tool::Group) => {
                                state.cancel();
                                continue;
                            }
                            _ => Query::Deselect,
                        }
                    };
                    state.error = None;
                    state.capture = Some(Capture {
                        request: Request {
                            document: st.doc.id,
                            revision: st.revision,
                            scope: scope.clone(),
                            mode: mode(modifiers, effective == Tool::Group),
                            query,
                        },
                        base,
                        node_base: node_base(direct),
                        points: vec![p],
                        tool: effective,
                        node_mode,
                        view,
                    });
                }
                Event::PointerMoved(pos) => {
                    if let Some(c) = &mut state.capture
                        && c.tool == Tool::Lasso
                    {
                        let p = c.view.point(pos);
                        if c.points.last().is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) * c.view.zoom >= 0.5) {
                            if c.points.len() >= 16_384 {
                                return Err(EngineError::Other("lasso point budget exceeded".into()));
                            }
                            c.points.push(p);
                        }
                    }
                }
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers } => {
                    if let Some(mut c) = state.capture.take() {
                        if c.tool == Tool::Lasso {
                            let p = c.view.point(pos);
                            if c.points.last() != Some(&p) {
                                c.points.push(p);
                            }
                        }
                        if let Some(p) = evaluate(session, &mut c, modifiers, readers)? {
                            let group_target = if matches!(c.request.query, Query::Group { .. }) && c.request.mode == Mode::Add {
                                // The merged selection's last layer may be an unrelated object.
                                // Keep the ancestor selected by this click for the next ascent.
                                let mut r = c.request.clone();
                                r.mode = Mode::Replace;
                                let st = session.active().ok_or(EngineError::NoDocument)?;
                                engine::preview(&st.doc, st.revision, &c.base, &r, readers)?.layers.first().copied()
                            } else {
                                p.layers.last().copied()
                            };
                            // preflight the capture, then one non-journaled session command.
                            engine::apply(session, &Preview { nodes: Some(Vec::new()), ..p.clone() })?;
                            let value = session.execute("vector.select", serde_json::to_value(&c.request).map_err(|e| EngineError::Other(e.to_string()))?)?;
                            let applied: Preview = serde_json::from_value(value).map_err(|e| EngineError::Other(e.to_string()))?;
                            ui.ctx().request_repaint();
                            if let Some(nodes) = applied.nodes {
                                publish_nodes(direct, nodes, &c.node_base, c.request.mode, c.request.document);
                            } else {
                                // Switching from node to object selection must not leave
                                // old anchors as a subsequent Node Delete target.
                                clear_nodes(direct, c.request.document);
                                state.selection = Some(applied.clone());
                                if !c.base.is_empty() && c.base != applied.layers {
                                    state.previous.insert(c.request.document, c.base);
                                }
                                state.group = match c.request.query {
                                    Query::Group { leaf, .. } => group_target.map(|id| (c.request.document, c.request.revision, leaf, id)),
                                    _ => None,
                                };
                            }
                        }
                        state.preview = None;
                    }
                }
                _ => {}
            }
        }
        if let Some(c) = &mut state.capture {
            state.preview = evaluate(session, c, mods, readers)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        state.error = Some(e.to_string());
        state.cancel();
    }
    paint(ui, state, view, rect);
    true
}
/// Overlay-only rendering; uses prepared bounds and never invalidates raster planes.
pub fn paint(ui: &Ui, state: &State, view: View, rect: Rect) {
    let painter = ui.painter_at(rect);
    let color = Color32::from_rgb(70, 145, 255);
    if let Some(p) = state.preview.as_ref().or(state.selection.as_ref()) {
        for (_, b) in &p.bounds {
            painter.rect_stroke(Rect::from_two_pos(view.screen([b[0], b[1]]), view.screen([b[2], b[3]])), 0., Stroke::new(1., color), egui::StrokeKind::Middle);
        }
    }
    if let Some(c) = &state.capture
        && c.tool == Tool::Lasso
        && c.points.len() > 1
    {
        let mut p: Vec<_> = c.points.iter().map(|p| c.view.screen(*p)).collect();
        p.push(p[0]);
        painter.add(egui::Shape::line(p, Stroke::new(1., color)));
    }
}
pub const SAME: &[(Same, &str)] = &[
    (Same::FillColor, "Fill Color"),
    (Same::StrokeColor, "Stroke Color"),
    (Same::FillAndStroke, "Fill & Stroke"),
    (Same::StrokeWeight, "Stroke Weight"),
    (Same::Opacity, "Opacity"),
    (Same::BlendMode, "Blend Mode"),
    (Same::Appearance, "Appearance"),
    (Same::AppearanceAttribute, "Appearance Attribute"),
    (Same::GraphicStyle, "Graphic Style"),
    (Same::ShapeType, "Shape Type"),
    (Same::Width, "Width"),
    (Same::Height, "Height"),
    (Same::Rotation, "Rotation"),
    (Same::Name, "Name"),
];
pub const OBJECT: &[(Object, &str)] = &[
    (Object::AllOnSameLayers, "All on Same Layers"),
    (Object::OpenPaths, "Open Paths"),
    (Object::StrayPoints, "Stray Points"),
    (Object::ClippingMasks, "Clipping Masks"),
    (Object::TextObjects, "Text Objects"),
    (Object::Groups, "Groups"),
    (Object::SymbolInstances, "Symbol Instances"),
    (Object::Unfilled, "Unfilled"),
    (Object::Stroked, "Stroked"),
    (Object::Unstroked, "Unstroked"),
];
pub fn execute(session: &mut Session, state: &mut State, scope: Scope, query: Query) -> Result<()> {
    let st = session.active().ok_or(EngineError::NoDocument)?;
    let before = st.selected_layers();
    let doc = st.doc.id;
    let request = Request { document: doc, revision: st.revision, scope, mode: Mode::Replace, query };
    let value = session.execute("vector.select", serde_json::to_value(request).map_err(|e| EngineError::Other(e.to_string()))?)?;
    state.selection = Some(serde_json::from_value(value).map_err(|e| EngineError::Other(e.to_string()))?);
    state.cancel();
    state.group = None;
    state.error = None;
    if !before.is_empty() && session.active().is_some_and(|s| s.selected_layers() != before) {
        state.previous.insert(doc, before);
    }
    Ok(())
}
/// Select-menu fragment. True means an object command succeeded; the caller clears
/// 03's transient nodes with clear_nodes(). Translate with the existing catalog lookup.
pub fn menu(ui: &mut Ui, session: &mut Session, state: &mut State, mut scope: Scope, text: impl Fn(&str) -> String) -> bool {
    let mut chosen = None;
    ui.checkbox(&mut state.settings.select_hidden, text("Select Hidden Objects"));
    scope.select_hidden = state.settings.select_hidden;
    ui.menu_button(text("Same"), |ui| {
        ui.horizontal(|ui| {
            ui.label(text("Comparison"));
            for (c, label) in [(Compare::Less, "<"), (Compare::LessEqual, "≤"), (Compare::Equal, "="), (Compare::GreaterEqual, "≥"), (Compare::Greater, ">")]
            {
                ui.selectable_value(&mut state.settings.compare, c, label);
            }
        });
        if let Some(reference) = session.active().and_then(|s| s.active_layer) {
            for (criterion, label) in SAME {
                if ui.button(text(label)).clicked() {
                    chosen = Some(Query::Same {
                        criterion: *criterion,
                        reference,
                        compare: state.settings.compare,
                        value: state.settings.value,
                        item: state.settings.item,
                    });
                    ui.close();
                }
            }
        }
    });
    ui.menu_button(text("Object"), |ui| {
        for (criterion, label) in OBJECT {
            if ui.button(text(label)).clicked() {
                chosen = Some(Query::Object { criterion: *criterion });
                ui.close();
            }
        }
    });
    for (q, label) in [
        (Query::All, "All Vector Objects"),
        (Query::Deselect, "Deselect Vector Objects"),
        (Query::Inverse, "Inverse Vector Selection"),
        (Query::Next { above: true }, "Next Object Above"),
        (Query::Next { above: false }, "Next Object Below"),
    ] {
        if ui.button(text(label)).clicked() {
            chosen = Some(q);
            ui.close();
        }
    }
    if let Some(st) = session.active() {
        if ui.button(text("Reselect Vector Objects")).clicked() {
            chosen = Some(Query::Reselect { ids: state.previous.get(&st.doc.id).cloned().unwrap_or_default() });
            ui.close();
        }
        let board = st.active_layer.and_then(|id| st.doc.path_of(id)).and_then(|p| {
            (1..=p.len())
                .rev()
                .filter_map(|n| st.doc.layer_at(&p[..n]))
                .find(|l| matches!(&l.content,photocraft_doc::LayerContent::Group(g) if g.artboard.is_some()))
                .map(|l| l.id)
        });
        if ui.add_enabled(board.is_some(), egui::Button::new(text("All on Active Artboard"))).clicked()
            && let Some(board) = board
        {
            chosen = Some(Query::Artboard { board });
            ui.close();
        }
    }
    if let Some(q) = chosen {
        match execute(session, state, scope, q) {
            Ok(()) => return true,
            Err(e) => state.error = Some(e.to_string()),
        }
    }
    false
}
pub fn wand_options(ui: &mut Ui, settings: &mut Settings, text: impl Fn(&str) -> String) {
    let w = &mut settings.wand;
    for (on, value, max, label) in [
        (&mut w.fill_color, &mut w.fill_tolerance, 200., "Fill Color ΔE"),
        (&mut w.stroke_color, &mut w.stroke_tolerance, 200., "Stroke Color ΔE"),
        (&mut w.stroke_weight, &mut w.weight_tolerance, 1e7, "Stroke Weight Tolerance"),
        (&mut w.opacity, &mut w.opacity_tolerance, 100., "Opacity Tolerance (%)"),
    ] {
        ui.horizontal(|ui| {
            ui.checkbox(on, text(label));
            let label = ui.label(text(match label {
                "Fill Color ΔE" => "Fill Tolerance ΔE",
                "Stroke Color ΔE" => "Stroke Tolerance ΔE",
                "Stroke Weight Tolerance" => "Weight Tolerance",
                _ => "Opacity Tolerance",
            }));
            ui.add_enabled(*on, egui::DragValue::new(value).range(0.0..=max).speed(0.1)).labelled_by(label.id);
        });
    }
    ui.checkbox(&mut w.blend_mode, text("Match Blend Mode"));
}
#[cfg(test)]
#[path = "selection/tests.rs"]
mod tests;

/// Canvas-router bridge owned here; 00 calls route() after the GPU image is painted and
/// before legacy pointer dispatch. Toolbar/keymap integration calls activate() for the
/// three new IDs. Transients live in egui memory, separate from persisted Settings.
#[derive(Clone, Default)]
pub struct Bridge {
    pub state: State,
    pub tool: Option<Tool>,
    pub node_mode: bool,
    pub scope: Scope,
    owner: Option<crate::state::Tool>,
    persona: Option<photocraft_engine::toolsets::Persona>,
    module: Option<crate::Module>,
}
fn bridge_key() -> egui::Id {
    egui::Id::new("vector-selection-32")
}
pub fn bridge(ctx: &egui::Context) -> Bridge {
    ctx.data(|d| d.get_temp(bridge_key())).unwrap_or_default()
}
pub fn save_bridge(ctx: &egui::Context, state: Bridge) {
    ctx.data_mut(|d| d.insert_temp(bridge_key(), state));
}
pub fn activate(app: &crate::PhotocraftApp, ctx: &egui::Context, tool: Tool, node_mode: bool, scope: Scope) {
    let mut b = bridge(ctx);
    b.state.cancel();
    b.tool = Some(tool);
    b.node_mode = node_mode;
    b.scope = Scope { select_hidden: b.state.settings.select_hidden, ..scope };
    b.owner = Some(app.ui.tool);
    b.persona = Some(app.session.prefs().toolbar.persona);
    b.module = Some(app.current_module);
    ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
}
pub fn frame(app: &crate::PhotocraftApp, ctx: &egui::Context) {
    let mut b = bridge(ctx);
    if crate::shortcuts::canvas_keyboard_available(ctx) && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        b.state.cancel();
    }
    if b.owner.is_some_and(|t| t != app.ui.tool)
        || b.persona.is_some_and(|p| p != app.session.prefs().toolbar.persona)
        || b.module.is_some_and(|m| m != app.current_module)
    {
        b.state.cancel();
        b.tool = None;
    } else if !ctx.input(|i| i.focused) || !app.ui.dialogs.is_empty() {
        b.state.cancel();
    }
    ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
}
pub fn route(app: &mut crate::PhotocraftApp, ui: &mut Ui, idx: usize, xf: &crate::canvas::ViewXform, rect: Rect) -> bool {
    if app.session.active_index() != Some(idx) {
        return false;
    }
    let start = crate::gpu_canvas::now_ms();
    let ctx = ui.ctx().clone();
    frame(app, &ctx);
    let mut b = bridge(&ctx);
    let tool = b
        .tool
        .filter(|_| b.owner == Some(app.ui.tool))
        .or_else(|| (app.ui.tool == crate::state::Tool::DirectSelection && ui.input(|i| i.modifiers.alt)).then_some(Tool::Node));
    if tool.is_none() && app.session.prefs().toolbar.persona != photocraft_engine::toolsets::Persona::Vector {
        b.state.cancel();
        b.tool = None;
        ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
        return false;
    };
    match engine::resolve_scope(&app.session, &b.scope) {
        Ok(scope) => b.scope = scope,
        Err(e) => {
            b.state.cancel();
            app.ui.status = e.to_string();
            app.ui.status_error = true;
            ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
            return true;
        }
    }
    let origin = xf.to_screen(0., 0.);
    let view = View { origin, document_origin: [0., 0.], zoom: f64::from(xf.zoom), flip: xf.flip, rotation: xf.rotation };
    if let Some(st) = app.session.active()
        && b.state
            .selection
            .as_ref()
            .is_none_or(|p| p.document != st.doc.id || p.revision != st.revision || p.scope != b.scope || p.layers != st.selected_layers())
    {
        let base = st.selected_layers();
        let r =
            Request { document: st.doc.id, revision: st.revision, scope: b.scope.clone(), mode: Mode::Replace, query: Query::Reselect { ids: base.clone() } };
        b.state.selection = engine::preview(&st.doc, st.revision, &base, &r, engine::installed_readers()).ok();
    }
    let Some(tool) = tool else {
        // Menu/MCP selection also has overlays under ordinary vector tools.
        b.state.cancel();
        b.tool = None;
        if view.valid() && !matches!(app.ui.tool, crate::state::Tool::Hand | crate::state::Tool::Zoom) && app.ui.view.extras {
            paint(ui, &b.state, view, rect);
        }
        ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
        app.perf.span("vector-selection", crate::gpu_canvas::now_ms() - start);
        return false;
    };
    let consumed = canvas(
        ui,
        &mut app.session,
        &mut b.state,
        &mut app.ui.direct_selection,
        rect,
        view,
        tool,
        b.node_mode,
        b.scope.clone(),
        engine::installed_readers(),
        !app.ui.dialogs.is_empty(),
    );
    if let Some(e) = &b.state.error {
        app.ui.status = e.clone();
        app.ui.status_error = true;
    }
    ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
    app.perf.span("vector-selection", crate::gpu_canvas::now_ms() - start);
    consumed
}
#[cfg(test)]
#[path = "selection/app_gpu.rs"]
mod app_gpu;

pub fn app_context(app: &mut crate::PhotocraftApp, ui: &mut Ui) {
    let mut b = bridge(ui.ctx());
    wand_options(ui, &mut b.state.settings, |s| crate::i18n::t(s).into());
    b.scope.select_hidden = b.state.settings.select_hidden;
    if let Some(st) = app.session.active_mut() {
        st.vector_selection_scope.select_hidden = b.state.settings.select_hidden;
    }
    ui.ctx().data_mut(|d| d.insert_temp(bridge_key(), b));
}
