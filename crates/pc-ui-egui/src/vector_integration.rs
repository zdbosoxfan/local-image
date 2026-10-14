//! Production vector routing. Feature-owned algorithms share the normal app canvas.
use crate::{
    PhotocraftApp,
    canvas::{ToolEvent, ViewXform},
    state::Tool,
};
use photocraft_doc::{Document, LayerContent};
use photocraft_engine::{
    shared_tools::{Handler, handler},
    toolsets::Persona,
};
use photocraft_geom::Point;
use std::sync::Arc;
pub mod views;

#[derive(Default)]
pub struct State {
    pub document_palette: bool,
    settings_revision: Option<u64>,
    settings_defaults: std::collections::BTreeMap<&'static str, serde_json::Value>,
    settings_saved: std::collections::BTreeMap<&'static str, Option<serde_json::Value>>,
    views: views::State,
    ase: Option<std::sync::mpsc::Receiver<AseResult>>,
    owned_preview: Option<(u64, u64, Arc<Document>)>,
    owner: Option<(Tool, Persona, crate::Module)>,
    // Derived compatibility view for the Window menu; membership is owned by DockLayout.
    windows: std::collections::BTreeSet<String>,
    swatches: crate::swatches_doc::State,
    recolor: crate::recolor::State,
    preview: Option<(u64, u64, Arc<Document>)>,
    projection: Option<(u64, u64, u64, Arc<Document>, Arc<Document>)>,
    damage: Option<(u64, (u64, u64), (u64, u64), photocraft_geom::Rect)>,
}
pub(crate) fn retain_documents(app: &mut PhotocraftApp) {
    let live = app.session.documents().iter().map(|st| st.doc.id).collect();
    app.vector_runtime.views.retain(&live);
    if app.vector_runtime.projection.as_ref().is_some_and(|(id, _, _, _, _)| !live.contains(&photocraft_doc::DocId(*id))) {
        app.vector_runtime.projection = None;
    }
    if app.vector_runtime.owned_preview.as_ref().is_some_and(|(id, _, _)| !live.contains(&photocraft_doc::DocId(*id))) {
        app.vector_runtime.owned_preview = None;
    }
    if app.vector_runtime.preview.as_ref().is_some_and(|(id, _, _)| !live.contains(&photocraft_doc::DocId(*id))) {
        app.vector_runtime.preview = None;
    }
}
fn report(app: &mut PhotocraftApp, result: impl ToString) {
    crate::command_feedback::report(app, result.to_string());
}
pub fn cancel(app: &mut PhotocraftApp) {
    let mut shared = std::mem::take(&mut app.shared_tools);
    shared.deactivate(app);
    app.shared_tools = shared;
    app.ui.vector_blend.cancel();
    app.constructive_state.cancel();
    app.width_state.cancel();
    app.vector_brush.cancel();
    app.envelope_state.cancel();
    app.vector_transform_tools.cancel();
    app.clip_compound_state.cancel();
    app.blob_eraser.cancel();
    app.mesh_state.cancel();
    app.ui.frame_compose.cancel();
    app.ui.fill_stroke_proxy.cancel();
    app.session.vector_insertion = None;
    app.vector_runtime.preview = None;
    app.vector_runtime.swatches.cancel();
    app.vector_runtime.recolor.cancel();
    app.vector_runtime.owned_preview = None;
}
fn constructive(tool: Tool) -> Option<crate::constructive::Tool> {
    use crate::constructive::Tool as C;
    Some(match tool {
        Tool::ShapeBuilder => C::Builder,
        Tool::Knife => C::Knife,
        Tool::Scissors => C::Scissors,
        Tool::LineCut => C::LineCut,
        Tool::Contour => C::Contour,
        Tool::Smooth => C::Smooth,
        Tool::PathEraser => C::Eraser,
        Tool::Join => C::Join,
        _ => return None,
    })
}
fn transform(tool: Tool) -> Option<crate::vector_transform_tools::Kind> {
    use crate::vector_transform_tools::Kind as K;
    Some(match tool {
        Tool::Rotate => K::Rotate,
        Tool::Reflect => K::Reflect,
        Tool::Scale => K::Scale,
        Tool::Shear => K::Shear,
        Tool::Reshape => K::Reshape,
        Tool::FreeTransform => K::FreeTransform,
        _ => return None,
    })
}
pub fn frame(app: &mut PhotocraftApp, ctx: &egui::Context) {
    preferences(app, ctx);
    crate::toolsets_ui::persona::frame(app, ctx);
    if crate::isolation::frame(app, ctx) {
        cancel(app);
    }
    let persona = app.session.prefs().toolbar.persona;
    let tool = app.ui.tool;
    let owner = (tool, persona, app.current_module);
    if app.vector_runtime.owner != Some(owner) {
        cancel(app);
        app.vector_runtime.owner = Some(owner);
        use crate::anchor_tools::Tool as A;
        crate::anchor_tools::activate(
            app,
            ctx,
            match tool {
                Tool::AddAnchor => Some(A::AddAnchor),
                Tool::DeleteAnchor => Some(A::DeleteAnchor),
                Tool::AnchorPoint => Some(A::AnchorPoint),
                Tool::Curvature => Some(A::Curvature),
                _ => None,
            },
        );
        use crate::distort_tools::Tool as D;
        crate::distort_tools::activate(
            app,
            ctx,
            match tool {
                Tool::VectorWarp => Some(D::Warp),
                Tool::VectorTwirl => Some(D::Twirl),
                Tool::VectorPucker => Some(D::Pucker),
                Tool::VectorBloat => Some(D::Bloat),
                Tool::Scallop => Some(D::Scallop),
                Tool::Crystallize => Some(D::Crystallize),
                Tool::Wrinkle => Some(D::Wrinkle),
                Tool::VectorPuppetWarp => Some(D::PuppetWarp),
                _ => None,
            },
        );
        use crate::vector_packages::selection;
        let selected_tool = match tool {
            Tool::GroupSelection => Some(selection::Tool::Group),
            Tool::VectorMagicWand => Some(selection::Tool::Wand),
            Tool::VectorLasso => Some(selection::Tool::Lasso),
            _ => None,
        };
        if let Some(selected_tool) = selected_tool {
            let scope = photocraft_engine::vector_packages::selection::session_scope(&app.session).unwrap_or_default();
            selection::activate(app, ctx, selected_tool, false, scope);
        }
        crate::vector_eyedropper::activate(
            app,
            ctx,
            match tool {
                Tool::StylePicker => Some(crate::vector_eyedropper::Tool::StylePicker),
                _ => None,
            },
        );
        if matches!(tool, Tool::Measure | Tool::Area) {
            crate::measure::activate(app, ctx, if tool == Tool::Measure { crate::measure::Mode::Measure } else { crate::measure::Mode::Area });
        }
    }
    let mut shared = std::mem::take(&mut app.shared_tools);
    shared.activate(app, &format!("{tool:?}"), persona);
    shared.pen.options = app.ui.tool_options.pen_options.clone();
    shared.snapping = app.ui.vector_snapping.clone();
    shared.pixels_per_point = Some(f64::from(ctx.pixels_per_point()));
    shared.shape_mode = app.ui.tool_options.vector_mode == "shape";
    shared.snapping.enabled = app.ui.extras.snap;
    app.shared_tools = shared;
    let ids = app.session.active().map(|st| st.selected_layers()).unwrap_or_default();
    let focused = ctx.input(|i| i.focused);
    let escape = crate::shortcuts::canvas_keyboard_available(ctx) && ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if !focused {
        let mut shared = std::mem::take(&mut app.shared_tools);
        shared.suspend(app);
        app.shared_tools = shared;
        app.ui.persona_pointer.clear();
    }
    crate::vector_packages::layout_app_hooks::synchronize(app, ctx);
    poll_ase(app);
    crate::vector_packages::selection::frame(app, ctx);
    crate::direct_select::frame(app, ctx);
    crate::anchor_tools::frame(app, ctx);
    crate::distort_tools::frame(app, ctx);
    crate::vector_eyedropper::frame(app, ctx);
    crate::measure::frame(app, ctx);
    app.constructive_state.set_tool(constructive(tool));
    app.constructive_state.set_targets(ids.clone());
    app.constructive_state.frame(&app.session, ctx, constructive(tool).is_some());
    app.width_state.lifecycle(&app.session, &ids, tool == Tool::Width, focused, escape);
    app.vector_brush.frame(&app.session, ctx, tool == Tool::VectorBrush);
    app.blob_eraser.mode = if tool == Tool::BlobBrush {
        photocraft_engine::vector_packages::blob_eraser::Mode::Blob
    } else {
        photocraft_engine::vector_packages::blob_eraser::Mode::Eraser
    };
    app.blob_eraser.frame(&app.session, ctx, matches!(tool, Tool::BlobBrush | Tool::VectorEraser));
    app.envelope_state.set_target(ids.first().copied());
    app.envelope_state.lifecycle(&app.session, tool == Tool::Envelope, focused, escape);
    if !app.vector_transform_tools.active()
        && let Some(kind) = transform(tool)
    {
        app.vector_transform_tools.kind = kind;
    }
    let transform_enabled = transform(tool).is_some() || app.vector_transform_tools.active();
    app.vector_transform_tools.lifecycle(&app.session, &ids, transform_enabled, focused, escape);
    app.mesh_state.lifecycle(&app.session, &ids, tool == Tool::Mesh, focused, escape);
    app.clip_compound_state.frame(&app.session, persona == Persona::Vector, focused, escape);
    app.ui.fill_stroke_proxy.synchronize(&app.session, &ids, persona == Persona::Vector);
    app.ui.fill_stroke_proxy.selected_stop = app.shared_tools.paint.options.selected_stop.map(|stop| (app.shared_tools.paint.options.item, stop));
    let mut live = crate::live_paint::state(ctx);
    live.set_tool(match tool {
        Tool::LivePaintBucket => Some(crate::live_paint::Tool::Bucket),
        Tool::LivePaintSelection => Some(crate::live_paint::Tool::Selection),
        Tool::VectorFloodFill => Some(crate::live_paint::Tool::Flood),
        _ => None,
    });
    live.set_targets(ids);
    live.paint = crate::shared_tools::paints(app).0;
    live.frame(&app.session, ctx, live.tool.is_some());
    crate::live_paint::store(ctx, live);
    crate::repeat::app_hooks::frame(app, ctx);
    crate::path_effects::app_hooks::frame(app, ctx);

    app.svg_import.background = app.background_jobs;
    app.affinity_import.background = app.background_jobs;
    app.svg_import.synchronize_context(&app.session, tool, app.current_module, focused && !escape);
    app.affinity_import.synchronize_context(&app.session, tool, app.current_module, focused && !escape);
    let mut blend = std::mem::take(&mut app.ui.vector_blend);
    crate::blend::app_hooks::synchronize(&mut blend, app, ctx);
    if let Err(e) = crate::blend::app_hooks::window(&mut blend, app, ctx) {
        report(app, e);
    }
    app.ui.vector_blend = blend;
    if app.ui.dialogs.is_empty() {
        match app.library_vectorize.take_launch(&app.session) {
            Ok(Some(_)) => app.trace_dialog.open(&mut app.session, &app.ui.trace_preferences),
            Ok(None) => {}
            Err(e) => crate::notices::error(app, e),
        }
    }
    if app.ui.view.panels_visible() {
        crate::vector_packages::layout_app_hooks::window(app, ctx);
    }
    windows(app, ctx);
    crate::graphic_styles::frame(app, ctx);
    crate::object_path::app_hooks::dialogs(app, ctx);
    crate::direct_select::conversion_dialog(app, ctx);
    app.vector_transform_tools.dialogs(ctx, &mut app.session);
    crate::pdf_import::show(app, ctx);
    crate::vector_packages::pdf_export::frame(app, ctx);
}

/// Options use the shell-owned, defaulted preferences map. Only persistent fields
/// cross this boundary; captures, previews and caches stay in the app runtime.
fn preferences(app: &mut PhotocraftApp, ctx: &egui::Context) {
    fn restore<T: serde::de::DeserializeOwned + serde::Serialize>(
        runtime: &mut State,
        prefs: &photocraft_engine::prefs::Preferences,
        key: &'static str,
        target: &mut T,
    ) {
        if let Ok(value) = serde_json::to_value(&*target) {
            runtime.settings_defaults.entry(key).or_insert(value);
        }
        let stored = prefs.dialogs.get(key).cloned();
        if runtime.settings_saved.get(key) != Some(&stored)
            && let Some(value) = stored.as_ref().or_else(|| runtime.settings_defaults.get(key))
            && let Ok(value) = serde_json::from_value(value.clone())
        {
            *target = value;
        }
        runtime.settings_saved.insert(key, stored);
    }
    fn save<T: serde::Serialize>(values: &mut Vec<(&'static str, serde_json::Value)>, key: &'static str, value: &T) {
        if let Ok(value) = serde_json::to_value(value) {
            values.push((key, value));
        }
    }
    let mut selection = crate::vector_packages::selection::bridge(ctx);
    if app.vector_runtime.settings_revision != Some(app.session.prefs.rev()) {
        let prefs = app.session.prefs();
        restore(&mut app.vector_runtime, prefs, "vector.tools.pencil", &mut app.shared_tools.pencil.options);
        restore(&mut app.vector_runtime, prefs, "vector.tools.width", &mut app.width_state.options);
        restore(&mut app.vector_runtime, prefs, "vector.tools.appearance", &mut app.shared_tools.paint.options);
        restore(&mut app.vector_runtime, prefs, "vector.tools.transform", &mut app.vector_transform_tools.preferences);
        restore(&mut app.vector_runtime, prefs, "vector.tools.drawingMode", &mut app.clip_compound_state.settings);
        restore(&mut app.vector_runtime, prefs, "vector.tools.mesh", &mut app.mesh_state.options);
        restore(&mut app.vector_runtime, prefs, "vector.tools.brush", &mut app.vector_brush.options);
        restore(&mut app.vector_runtime, prefs, "vector.tools.brushPreset", &mut app.vector_brush.preset);
        restore(&mut app.vector_runtime, prefs, "vector.tools.brushRope", &mut app.vector_brush.rope_length);
        restore(&mut app.vector_runtime, prefs, "vector.tools.blobEraser", &mut app.blob_eraser.options);
        restore(&mut app.vector_runtime, prefs, "vector.tools.blobEraserRope", &mut app.blob_eraser.rope_length);
        restore(&mut app.vector_runtime, prefs, "vector.tools.selection", &mut selection.state.settings);
        crate::vector_packages::selection::save_bridge(ctx, selection.clone());
        app.vector_runtime.settings_revision = Some(app.session.prefs.rev());
    }
    let mut values = Vec::new();
    save(&mut values, "vector.tools.pencil", &app.shared_tools.pencil.options);
    save(&mut values, "vector.tools.width", &app.width_state.options);
    save(&mut values, "vector.tools.appearance", &app.shared_tools.paint.options);
    save(&mut values, "vector.tools.transform", &app.vector_transform_tools.preferences);
    save(&mut values, "vector.tools.drawingMode", &app.clip_compound_state.settings);
    save(&mut values, "vector.tools.mesh", &app.mesh_state.options);
    save(&mut values, "vector.tools.brush", &app.vector_brush.options);
    save(&mut values, "vector.tools.brushPreset", &app.vector_brush.preset);
    save(&mut values, "vector.tools.brushRope", &app.vector_brush.rope_length);
    save(&mut values, "vector.tools.blobEraser", &app.blob_eraser.options);
    save(&mut values, "vector.tools.blobEraserRope", &app.blob_eraser.rope_length);
    save(&mut values, "vector.tools.selection", &selection.state.settings);
    if let Some(st) = app.session.active_mut() {
        st.vector_selection_scope.select_hidden = selection.state.settings.select_hidden;
    }
    values.retain(|(key, value)| app.session.prefs().dialogs.get(*key) != Some(value));
    if !values.is_empty() {
        app.session.prefs.edit(|prefs| {
            for (key, value) in values {
                prefs.dialogs.insert(key.into(), value);
            }
        });
    }
    for key in app.vector_runtime.settings_defaults.keys() {
        app.vector_runtime.settings_saved.insert(key, app.session.prefs().dialogs.get(*key).cloned());
    }
    app.vector_runtime.settings_revision = Some(app.session.prefs.rev());
}

fn isolation_canvas(app: &mut PhotocraftApp, ui: &egui::Ui, response: &egui::Response, xf: &ViewXform, _rect: egui::Rect) -> bool {
    let enabled = app.session.prefs().toolbar.persona == Persona::Vector
        && app.ui.dialogs.is_empty()
        && matches!(app.ui.tool, Tool::Move | Tool::PathSelection | Tool::GroupSelection | Tool::DirectSelection);
    if !enabled || !ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::PointerButton { button: egui::PointerButton::Primary, .. }))) {
        return false;
    }
    let mut bridge = crate::isolation::bridge(ui.ctx());
    let hit = app.session.active().and_then(|st| {
        let point = ui.input(|i| i.pointer.interact_pos())?;
        let q = xf.to_doc(point);
        let point = Point::new(q[0], q[1]);
        let scope = photocraft_engine::vector_packages::isolation::Scope { document: st.doc.id, target: None };
        let tolerance = 6. / f64::from(xf.zoom).max(0.01);
        let hit = if bridge.views.mode == crate::view_modes::Mode::Outline {
            photocraft_engine::vector_integration::project_document(&st.doc)
                .and_then(|projected| {
                    crate::view_modes::prepare(
                        &st.doc,
                        &bridge.views,
                        scope,
                        photocraft_geom::Affine::IDENTITY,
                        st.doc.bounds(),
                        &views::SharedGeometry(&projected),
                    )
                    .and_then(|frame| frame.hit(&st.doc, scope, point, tolerance))
                })
                .ok()
                .flatten()
        } else {
            photocraft_engine::vector_packages::selection::hit(
                &st.doc,
                &Default::default(),
                [point.x, point.y],
                tolerance,
                photocraft_engine::vector_packages::selection::installed_readers(),
            )
            .ok()
            .flatten()
        }?;
        // Enter the nearest eligible container. During isolation, descendants may
        // still enter a nested container while unrelated objects remain outside hits.
        let path = st.doc.path_of(hit)?;
        (1..=path.len())
            .rev()
            .filter_map(|n| st.doc.layer_at(&path[..n]))
            .find(|layer| {
                layer.is_group()
                    || matches!(&layer.content, LayerContent::Symbol(_))
                    || matches!(&layer.content,LayerContent::Shape(s) if s.path.subpaths.len()>1)
            })
            .map(|layer| layer.id)
            .or(Some(hit))
    });
    let result = crate::isolation::canvas(ui, response, &mut app.session, &mut bridge.isolation, hit, enabled);
    crate::isolation::store_session(&mut app.session, ui.ctx(), bridge);
    match result {
        Ok(true) => {
            cancel(app);
            true
        }
        Ok(false) => false,
        Err(e) => {
            report(app, e);
            true
        }
    }
}

/// The common automation pointer route, before legacy paint/snapping consumers.
pub fn pointer(app: &mut PhotocraftApp, event: ToolEvent, mods: egui::Modifiers) -> bool {
    if crate::vector_packages::layout_app_hooks::event(app, event, mods) {
        return true;
    }
    let tool = app.ui.tool;
    if tool == Tool::VectorBrush
        && matches!(event, ToolEvent::Down { .. })
        && let Err(e) = photocraft_engine::vector_integration::capture_insertion(&mut app.session, app.clip_compound_state.settings.mode)
    {
        report(app, e);
        return true;
    }
    let zoom = f64::from(app.current_zoom());
    match crate::blend::app_hooks::event(&mut app.ui.vector_blend, &mut app.session, event, mods, zoom) {
        Ok(true) => return true,
        Err(e) => {
            report(app, e);
            return true;
        }
        _ => {}
    }
    if constructive(tool).is_some() {
        let zoom = f64::from(app.current_zoom());
        return app.constructive_state.pointer(&mut app.session, event, mods, zoom);
    }
    if tool == Tool::VectorBrush {
        if let Err(e) = app.vector_brush.pointer(&mut app.session, event) {
            report(app, e);
        }
        return true;
    }
    if matches!(tool, Tool::BlobBrush | Tool::VectorEraser) {
        let color = app.session.tools.foreground;
        if let Err(e) = app.blob_eraser.pointer_with_color(&mut app.session, event, mods, color) {
            report(app, e);
        }
        return true;
    }
    let id = format!("{tool:?}");
    if matches!(
        handler(&id),
        Some(
            Handler::Pen
                | Handler::Shape
                | Handler::VectorPencil
                | Handler::Corner
                | Handler::VectorPaint
                | Handler::PathText
                | Handler::VectorCrop
                | Handler::Transform
                | Handler::Node
        )
    ) {
        let persona = app.session.prefs().toolbar.persona;
        let mut state = std::mem::take(&mut app.shared_tools);
        state.activate(app, &id, persona);
        let result = state.pointer(app, event, mods);
        app.shared_tools = state;
        match result {
            Ok(consumed) => consumed,
            Err(e) => {
                report(app, e);
                true
            }
        }
    } else {
        false
    }
}

fn canvas_scrollbar_at(app: &PhotocraftApp, idx: usize, xf: &ViewXform, rect: egui::Rect, pos: egui::Pos2) -> bool {
    let size = app.session.documents()[idx].doc.size;
    let view = crate::state::View { zoom: xf.zoom, center: xf.center, rotation: xf.rotation, doc_size: [size.width, size.height], ..Default::default() };
    let (h, v) = crate::scrollbars::visible_bar_rects(rect, &view, xf.flip);
    h.is_some_and(|r| r.contains(pos)) || v.is_some_and(|r| r.contains(pos))
}

/// Raw pointer ownership is decided before normal Move/paint handling, including release.
pub fn input(app: &mut PhotocraftApp, ui: &mut egui::Ui, idx: usize, response: &egui::Response, xf: &ViewXform, rect: egui::Rect) -> bool {
    if app.session.active_index() != Some(idx) {
        return false;
    }
    let drawing =
        app.shared_tools.drawing.is_active() || crate::crop_ui::active(app) || app.drag.as_ref().is_some_and(|drag| crate::hold_keys::repositions(drag.tool));
    if crate::hold_keys::for_frame(app, ui.ctx(), drawing).is_some() {
        return false;
    }
    let down = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::PointerButton { pressed: true, .. })));
    if down
        && ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|pos| ui.ctx().layer_id_at(pos) != Some(response.layer_id) || canvas_scrollbar_at(app, idx, xf, rect, pos))
    {
        return false;
    }
    if isolation_canvas(app, ui, response, xf, rect) {
        return true;
    }
    let ctx = ui.ctx().clone();
    if crate::pdf_import::canvas_input(app, response, xf) {
        return true;
    }
    let map = |p: egui::Pos2| {
        let q = xf.to_doc(p);
        egui::pos2(q[0] as f32, q[1] as f32)
    };
    if app.svg_import.pending() {
        if let Err(e) = crate::vector_packages::svg_import::input(&mut app.svg_import, &mut app.session, &ctx, rect, map, true) {
            report(app, e);
        }
        return true;
    }
    if app.affinity_import.pending() {
        if let Err(e) = crate::vector_packages::affinity::input(&mut app.affinity_import, &mut app.session, &ctx, rect, map, true) {
            report(app, e);
        }
        return true;
    }
    if matches!(app.ui.tool, Tool::AddAnchor | Tool::DeleteAnchor | Tool::AnchorPoint | Tool::Curvature) {
        let shown = crate::direct_select::anchor_tool_handles(app);
        if crate::anchor_tools::route(app, ui, idx, xf, rect, &shown) {
            return true;
        }
    }
    let distort_start = crate::gpu_canvas::now_ms();
    let distorted = crate::distort_tools::route(app, ui, idx, xf, rect);
    app.perf.spans.insert("vector-distort", crate::gpu_canvas::now_ms() - distort_start);
    if distorted {
        return true;
    }
    if crate::repeat::app_hooks::route(app, ui, idx, xf, rect) {
        return true;
    }
    let tool = app.ui.tool;
    if tool == Tool::DirectSelection {
        let selection = crate::vector_packages::selection::bridge(&ctx);
        if selection.node_mode && selection.tool.is_some() {
            // The selection router publishes node lasso results after painting.
            // It owns these pointer events; do not also begin a Node marquee.
            return true;
        }
    }
    if tool == Tool::PathSelection && app.shared_tools.layout.tool != crate::vector_packages::layout::Tool::None {
        let count = ctx.input(|i| i.events.len());
        for index in 0..count {
            match ctx.input(|i| i.events.get(index).cloned()) {
                Some(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers })
                    if rect.contains(pos) || app.shared_tools.layout.captured() =>
                {
                    let p = xf.to_doc(pos);
                    pointer(app, if pressed { ToolEvent::Down { x: p[0], y: p[1], pressure: 1. } } else { ToolEvent::Up { x: p[0], y: p[1] } }, modifiers);
                }
                Some(egui::Event::PointerMoved(pos)) if app.shared_tools.layout.captured() => {
                    let p = xf.to_doc(pos);
                    pointer(app, ToolEvent::Move { x: p[0], y: p[1], pressure: 1. }, ctx.input(|i| i.modifiers));
                }
                _ => {}
            }
        }
        return true;
    }
    let frame_transform =
        app.session.active().and_then(|st| st.active_layer.and_then(|id| st.doc.layer(id))).filter(|layer| layer.vector.on_path.is_none()).and_then(|layer| {
            match &layer.content {
                LayerContent::Text(text) if matches!(text.shape, photocraft_doc::text::TextShape::Box { .. }) => text.transform.inverse(),
                _ => None,
            }
        });
    let frame_enabled = tool.is_type() && frame_transform.is_some();
    let frame_owned = app.ui.frame_compose.dragging || app.ui.frame_compose.draft.is_some();
    if let Some(inverse) = frame_transform {
        let result = crate::frame_compose::input(
            &mut app.ui.frame_compose,
            &mut app.session,
            &ctx,
            rect,
            |p| {
                let q = xf.to_doc(p);
                let q = inverse.apply(Point::new(q[0], q[1]));
                (q.x as f32, q.y as f32)
            },
            frame_enabled,
            &crate::frame_compose::NativeMetadata,
        );
        if let Err(e) = result {
            report(app, e);
        }
    } else {
        app.ui.frame_compose.synchronize(&app.session, false);
    }
    if frame_owned || app.ui.frame_compose.dragging {
        return true;
    }
    let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
    let point = |p: egui::Pos2| {
        let q = xf.to_doc(p);
        Point::new(q[0], q[1])
    };
    let tolerance = 6. / (f64::from(xf.zoom) * f64::from(ctx.pixels_per_point())).max(0.01);
    if constructive(tool).is_some() {
        app.constructive_state.canvas_input(&mut app.session, response, xf);
        return true;
    }
    if tool == Tool::Width {
        app.width_state.input(&mut app.session, &ctx, rect, point, &ids, true, tolerance);
        return true;
    }
    if tool == Tool::VectorBrush {
        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
            && response.contains_pointer()
            && let Err(e) = photocraft_engine::vector_integration::capture_insertion(&mut app.session, app.clip_compound_state.settings.mode)
        {
            report(app, e);
            return true;
        }
        if let Err(e) = crate::vector_brush::canvas_input_with_pressure(&mut app.vector_brush, &mut app.session, response, xf, app.stylus.pressure()) {
            report(app, e);
        }
        if ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            app.session.vector_insertion = None;
        }
        return true;
    }
    if matches!(tool, Tool::BlobBrush | Tool::VectorEraser) {
        if let Err(e) = crate::blob_eraser::canvas_input(&mut app.blob_eraser, &mut app.session, response, xf) {
            report(app, e);
        }
        return true;
    }
    if tool == Tool::Envelope {
        app.envelope_state.canvas_input(&mut app.session, response, xf);
        return true;
    }
    if transform(tool).is_some() {
        app.vector_transform_tools.input(
            &mut app.session,
            &ctx,
            rect,
            point,
            &ids,
            true,
            photocraft_engine::vector_packages::snapping::query::View { zoom: f64::from(xf.zoom), pixels_per_point: f64::from(ctx.pixels_per_point()) },
        );
        return true;
    }
    if tool == Tool::Mesh {
        if let Err(e) = app.mesh_state.input(
            &mut app.session,
            &ids,
            &ctx,
            rect,
            |p| {
                let q = xf.to_doc(p);
                [q[0], q[1]]
            },
            tolerance,
            true,
        ) {
            report(app, e);
        }
        return true;
    }
    let mut live = crate::live_paint::state(&ctx);
    let consumed = live.canvas_input(&mut app.session, response, xf);
    crate::live_paint::store(&ctx, live);
    if consumed {
        return true;
    }
    if matches!(tool, Tool::GroupSelection | Tool::VectorMagicWand | Tool::VectorLasso) {
        return true;
    }
    if matches!(tool, Tool::Measure | Tool::Area | Tool::StylePicker) {
        return true;
    }
    let mut shared = std::mem::take(&mut app.shared_tools);
    let consumed = shared.input(app, ui, &format!("{tool:?}"), app.session.prefs().toolbar.persona, xf, rect);
    app.shared_tools = shared;
    consumed
}

pub fn after_paint(app: &mut PhotocraftApp, ui: &mut egui::Ui, idx: usize, xf: &ViewXform, rect: egui::Rect) {
    crate::vector_packages::selection::route(app, ui, idx, xf, rect);
    crate::measure::route(app, ui, idx, xf, rect);
    if app.ui.tool == Tool::StylePicker {
        let scope = photocraft_engine::vector_packages::selection::session_scope(&app.session).unwrap_or_default();
        let readers = photocraft_engine::vector_packages::selection::installed_readers();
        let tolerance = 6. / f64::from(xf.zoom).max(0.01);
        let hit_scope = scope.clone();
        crate::vector_eyedropper::route(
            app,
            ui,
            idx,
            xf,
            rect,
            move |doc, p| {
                Ok(photocraft_engine::vector_packages::selection::style_hit(doc, &hit_scope, [p.x, p.y], tolerance, readers)?
                    .map(|layer| crate::vector_eyedropper::Hit { layer, byte: None, stroke: false }))
            },
            move |doc, a, b| {
                let r = photocraft_engine::vector_packages::selection::Request {
                    document: doc.id,
                    revision: 0,
                    scope: scope.clone(),
                    mode: photocraft_engine::vector_packages::selection::Mode::Replace,
                    query: photocraft_engine::vector_packages::selection::Query::All,
                };
                let preview = photocraft_engine::vector_packages::selection::preview(doc, 0, &[], &r, readers)?;
                Ok(preview
                    .layers
                    .into_iter()
                    .filter(|id| {
                        doc.layer(*id)
                            .and_then(|l| if let LayerContent::Shape(s) = &l.content { s.path.control_bounds() } else { None })
                            .is_some_and(|r| r.0 >= a.x.min(b.x) && r.1 >= a.y.min(b.y) && r.2 <= a.x.max(b.x) && r.3 <= a.y.max(b.y))
                    })
                    .collect())
            },
        );
    }
    if app.ui.tool.is_type()
        && let Some(layer) = app.session.active().and_then(|st| st.active_layer.and_then(|id| st.doc.layer(id)))
        && let LayerContent::Text(t) = &layer.content
        && layer.vector.on_path.is_none()
    {
        let transform = t.transform;
        crate::frame_compose::overlay(ui, &app.ui.frame_compose, &app.session, |x, y| {
            let p = transform.apply(Point::new(f64::from(x), f64::from(y)));
            xf.to_screen(p.x as f32, p.y as f32)
        });
    }
    let painter = ui.painter_at(rect);
    crate::blend::app_hooks::paint(&app.ui.vector_blend, app, &painter, xf);
    crate::repeat::app_hooks::overlay(app, ui, xf);
    app.shared_tools.overlay(&painter, xf, app);
    app.constructive_state.draw_overlay(&painter, &|p| xf.to_screen(p.x as f32, p.y as f32), f64::from(xf.zoom));
    let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
    if app.ui.tool == Tool::Width {
        app.width_state.paint_handles(&app.session, &ids, &painter, |p| xf.to_screen(p.x as f32, p.y as f32));
    }
    if app.ui.tool == Tool::Envelope {
        app.envelope_state.draw_overlay(&app.session, &painter, xf);
    }
    if transform(app.ui.tool).is_some() {
        app.vector_transform_tools.paint(&app.session, &ids, &painter, |p| xf.to_screen(p.x as f32, p.y as f32));
    }
    if app.ui.tool == Tool::Mesh {
        let _ = app.mesh_state.overlay(&app.session, &ids, &painter, |p| xf.to_screen(p[0] as f32, p[1] as f32));
    }
    crate::live_paint::state(ui.ctx()).draw_overlay(&painter, &|p| xf.to_screen(p.x as f32, p.y as f32), f64::from(xf.zoom));
}

pub fn context(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> bool {
    if app.session.prefs().toolbar.persona == Persona::Vector {
        let mut bridge = crate::isolation::bridge(ui.ctx());
        let result = crate::isolation::bar(ui, &mut app.session, &mut bridge.isolation, |text| li_i18n::t(text).to_string());
        crate::isolation::store_session(&mut app.session, ui.ctx(), bridge);
        match result {
            Ok(true) => cancel(app),
            Err(e) => report(app, e),
            _ => {}
        }
    }
    let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
    let tool = app.ui.tool;
    if tool.is_type()
        && app.session.active().is_some_and(|st| {
            st.active_layer.and_then(|id| st.doc.layer(id)).is_some_and(|layer| {
                layer.vector.on_path.is_none()
                    && matches!(&layer.content, photocraft_doc::LayerContent::Text(text) if matches!(text.shape, photocraft_doc::text::TextShape::Box { .. }))
            })
        })
    {
        ui.menu_button(li_i18n::t("Frame text"), |ui| {
            if let Err(e) = crate::frame_compose::panel(ui, &mut app.ui.frame_compose, &mut app.session, &crate::frame_compose::NativeMetadata) {
                report(app, e);
            }
        });
    }
    if constructive(tool).is_some() {
        app.constructive_state.context_bar(ui, &mut app.session);
        return true;
    }
    match tool {
        Tool::DirectSelection => crate::direct_select::context_ui(app, ui),
        Tool::AddAnchor | Tool::DeleteAnchor | Tool::AnchorPoint | Tool::Curvature => {
            let mut bridge = crate::anchor_tools::bridge(ui.ctx());
            crate::anchor_tools::context(ui, &mut bridge.state);
            crate::anchor_tools::save_bridge(ui.ctx(), bridge);
        }
        Tool::VectorWarp
        | Tool::VectorTwirl
        | Tool::VectorPucker
        | Tool::VectorBloat
        | Tool::Scallop
        | Tool::Crystallize
        | Tool::Wrinkle
        | Tool::VectorPuppetWarp => crate::distort_tools::app_context(app, ui),
        Tool::Measure | Tool::Area => crate::measure::app_context(app, ui),
        Tool::VectorMagicWand => crate::vector_packages::selection::app_context(app, ui),
        Tool::Blend => crate::blend::app_hooks::context(&mut app.ui.vector_blend, ui),
        Tool::Width => app.width_state.panel(ui, &mut app.session, &ids),
        Tool::VectorBrush => crate::vector_brush::context_bar(ui, &mut app.vector_brush),
        Tool::BlobBrush | Tool::VectorEraser => crate::blob_eraser::context_bar(ui, &mut app.blob_eraser),
        Tool::Envelope => app.envelope_state.context_bar(ui, &mut app.session),
        Tool::Rotate | Tool::Reflect | Tool::Scale | Tool::Shear | Tool::Reshape | Tool::FreeTransform => app.vector_transform_tools.context_bar(ui),
        Tool::Mesh => {
            if let Err(e) = app.mesh_state.context_bar(ui, &mut app.session, &ids).and_then(|_| app.mesh_state.point_controls(ui, &mut app.session, &[])) {
                report(app, e);
            }
        }
        Tool::StylePicker => crate::vector_eyedropper::context_bar(app, ui),
        Tool::LivePaintBucket | Tool::LivePaintSelection | Tool::VectorFloodFill => {
            let mut state = crate::live_paint::state(ui.ctx());
            state.context_bar(ui, &mut app.session);
            crate::live_paint::store(ui.ctx(), state);
        }
        _ => {
            if matches!(
                handler(&format!("{tool:?}")),
                Some(Handler::Pen | Handler::Shape | Handler::VectorPencil | Handler::Corner | Handler::VectorPaint | Handler::PathText | Handler::Node)
            ) {
                let mut state = std::mem::take(&mut app.shared_tools);
                state.context(app, ui);
                app.shared_tools = state;
            } else {
                return false;
            }
        }
    }
    true
}

pub fn display(app: &mut PhotocraftApp, ctx: &egui::Context, idx: usize, mut source: Arc<Document>, mut key: u64) -> (Arc<Document>, u64) {
    if app.session.active_index() == Some(idx) {
        if let Some((doc, k)) = app.shared_tools.preview() {
            if app.vector_runtime.preview.as_ref().is_none_or(|(id, old, _)| *id != doc.id.0 || *old != k) {
                app.vector_runtime.preview = Some((doc.id.0, k, Arc::new(doc.clone())));
            }
            if let Some((_, k, doc)) = &app.vector_runtime.preview {
                source = doc.clone();
                key = *k;
            }
        } else if let Some((layer, k)) = app.shared_tools.shape_preview() {
            if app.vector_runtime.preview.as_ref().is_none_or(|(id, old, _)| *id != source.id.0 || *old != k) {
                let mut doc = (*source).clone();
                if let Some(target) = doc.layer_mut(layer.id) {
                    *target = layer.clone();
                } else {
                    doc.layers.push(layer.clone());
                }
                app.vector_runtime.preview = Some((doc.id.0, k, Arc::new(doc)));
            }
            if let Some((_, k, doc)) = &app.vector_runtime.preview {
                source = doc.clone();
                key = *k;
            }
        } else {
            app.vector_runtime.preview = None;
        }
        if let Some(shown) = [
            app.constructive_state.display_document(&app.session),
            app.vector_brush.display_doc(&app.session),
            app.blob_eraser.display_doc(&app.session),
            app.envelope_state.display_document(&app.session),
            crate::direct_select::display_doc(app, idx),
            crate::anchor_tools::display_doc(app, idx, ctx),
            crate::distort_tools::display_doc(app, idx, ctx),
            crate::vector_eyedropper::display_doc(app, idx, ctx),
            crate::pdf_import::display_doc(app, idx),
            crate::object_path::app_hooks::display(app, ctx, idx),
            crate::graphic_styles::display_doc(app, idx, ctx),
            crate::live_paint::state(ctx).display_document(&app.session),
            crate::path_effects::app_hooks::display(app, ctx, idx),
            crate::vector_packages::layout_app_hooks::preview(app, idx),
        ]
        .into_iter()
        .flatten()
        .next()
        {
            source = shown.0;
            key = shown.1;
        }
        let repeat = crate::repeat::app_hooks::bridge(ctx);
        if let Some(preview) = repeat.state.preview {
            source = preview;
            key = (1 << 45) | repeat.state.generation;
        }
        if let Some(preview) = &app.clip_compound_state.preview {
            key = app.clip_compound_state.generation | (1 << 45);
            source = owned_preview(&mut app.vector_runtime, preview, key);
        }
        if let Some(preview) = &app.ui.fill_stroke_proxy.preview {
            key = (1 << 53) | app.ui.fill_stroke_proxy.generation;
            source = owned_preview(&mut app.vector_runtime, preview, key);
        }
        if let Some(preview) = &app.ui.frame_compose.preview {
            key = (1 << 58) | app.ui.frame_compose.generation;
            source = owned_preview(&mut app.vector_runtime, preview, key);
        }
        if let Some(preview) = &app.ui.vector_blend.preview {
            source = preview.clone();
            key = (1 << 47) | app.ui.vector_blend.generation;
        }
        if let Some(preview) = &app.width_state.preview {
            key = (1 << 56) | app.width_state.generation;
            source = owned_preview(&mut app.vector_runtime, preview, key);
        }
        if let Some(preview) = &app.vector_transform_tools.preview {
            key = (1 << 59) | app.vector_transform_tools.generation;
            source = owned_preview(&mut app.vector_runtime, preview, key);
        }
        if let Some(preview) = &app.mesh_state.preview {
            key = (1 << 57) | app.mesh_state.generation;
            source = owned_preview(&mut app.vector_runtime, preview, key);
        }
        if let Some(preview) = &app.svg_import.preview {
            source = preview.clone();
            key = (1 << 54) | app.svg_import.generation;
        }
        for (preview, generation, tag) in [
            (app.vector_runtime.swatches.preview.as_ref(), app.vector_runtime.swatches.generation, 51),
            (app.vector_runtime.recolor.preview.as_ref(), app.vector_runtime.recolor.generation, 52),
        ] {
            if let Some(preview) = preview {
                key = (1 << tag) | generation;
                let cached = &mut app.vector_runtime.owned_preview;
                if cached.as_ref().is_none_or(|(id, k, _)| *id != preview.document.id.0 || *k != key) {
                    *cached = Some((preview.document.id.0, key, Arc::new(preview.document.clone())));
                }
                if let Some((_, _, doc)) = cached {
                    source = doc.clone();
                }
            }
        }
        if let Some(preview) = &app.affinity_import.preview {
            source = preview.clone();
            key = (1 << 60) | app.affinity_import.generation;
        }
    }
    if app.session.active_index() == Some(idx) && app.session.vector_insertion.is_some() && key != 0 {
        match photocraft_engine::vector_integration::insertion_preview(&app.session, &source) {
            Ok(document) => source = owned_preview(&mut app.vector_runtime, &document, key ^ (1 << 46)),
            Err(e) => report(app, e),
        }
    }
    let revision = app.session.documents()[idx].revision;
    if let Some((id, rev, k, original, projected)) = &app.vector_runtime.projection
        && *id == source.id.0
        && *rev == revision
        && *k == key
        && Arc::ptr_eq(original, &source)
    {
        let projected = projected.clone();
        return views::display(app, idx, source.clone(), projected, key).unwrap_or_else(|e| {
            report(app, e);
            (source, key)
        });
    }
    match photocraft_engine::vector_integration::project_document(&source) {
        Ok(doc) => {
            let projected = Arc::new(doc);
            app.vector_runtime.damage = app.vector_runtime.projection.as_ref().and_then(|(id, rev, old_key, _, old)| {
                crate::gpu_canvas::scaled_view::damage(old, &projected).map(|rect| (*id, (*rev, *old_key), (revision, key), rect))
            });
            app.vector_runtime.projection = Some((source.id.0, revision, key, source.clone(), projected.clone()));
            views::display(app, idx, source.clone(), projected, key).unwrap_or_else(|e| {
                report(app, e);
                (source, key)
            })
        }
        Err(e) => {
            report(app, e);
            (source, key)
        }
    }
}

pub fn damage(app: &PhotocraftApp, document: photocraft_doc::DocId, seen: (u64, u64), now: (u64, u64)) -> Option<photocraft_geom::Rect> {
    app.vector_runtime.damage.as_ref().filter(|(id, from, to, _)| *id == document.0 && *from == seen && *to == now).map(|(_, _, _, r)| *r)
}

pub fn keys(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    crate::vector_packages::selection::frame(app, ctx);
    crate::direct_select::frame(app, ctx);
    if crate::vector_packages::path_text_app_hooks::keys(app, ctx) {
        return true;
    }
    let mut shared = std::mem::take(&mut app.shared_tools);
    let shared_keys = shared.keys(app, ctx);
    app.shared_tools = shared;
    if shared_keys {
        return true;
    }
    if crate::shortcut_dispatch::Focus::of(ctx) == crate::shortcut_dispatch::Focus::None {
        let remove = matches!(app.ui.tool, Tool::Width | Tool::VectorPuppetWarp)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete) || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace));
        if remove {
            let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
            if app.ui.tool == Tool::Width {
                if let Err(e) = app.width_state.delete_selected(&mut app.session, &ids) {
                    report(app, e);
                }
            } else {
                let mut state = crate::distort_tools::bridge(ctx);
                state.state.delete_pins();
                crate::distort_tools::save_bridge(ctx, state);
            }
            return true;
        }
    }
    // Inline canvas text is not an egui TextEdit. Its letters belong to the type editor.
    if app.ui.text_edit.is_some() {
        return false;
    }
    if crate::object_path::app_hooks::frame(app, ctx) {
        return true;
    }
    if app.session.prefs().toolbar.persona != Persona::Vector {
        return false;
    }
    if crate::anchor_tools::keys(app, ctx) {
        return true;
    }
    if crate::shortcut_dispatch::dispatch_pressed(app, ctx, crate::shortcut_dispatch::Focus::of(ctx), false) {
        return true;
    }
    if crate::shortcut_dispatch::Focus::of(ctx) != crate::shortcut_dispatch::Focus::None {
        return false;
    }
    let preset = app.session.prefs().toolbar.vector_keyboard.preset;
    for (id, key) in &app.session.prefs().shortcut_overrides(false).clone() {
        if let Some(tool) = id.strip_prefix("tools.select.").and_then(Tool::from_name)
            && photocraft_engine::toolsets::supports(&format!("{tool:?}"), Persona::Vector)
            && crate::keymaps::global_conflict(key).is_none()
            && li_keys::input::parse(key).is_some_and(|sc| li_keys::input::consume(ctx, &sc))
        {
            crate::toolsets_ui::persona::select_tool(app, ctx, tool);
            return true;
        }
    }
    let registered = photocraft_engine::toolsets::tools_for(Persona::Vector);
    for id in &registered {
        if let Some(tool) = Tool::from_name(id)
            && crate::shortcuts::tool_shortcut(&app.session, tool)
                .as_deref()
                .and_then(li_keys::input::parse)
                .is_some_and(|sc| li_keys::input::consume(ctx, &sc))
        {
            let current = format!("{:?}", app.ui.tool);
            let key = crate::shortcuts::tool_shortcut(&app.session, tool).unwrap_or_default();
            let tool = photocraft_engine::vector_toolbar_layout::tool_for_key_after(Persona::Vector, preset, &key, &registered, &current)
                .and_then(Tool::from_name)
                .unwrap_or(tool);
            crate::toolsets_ui::persona::select_tool(app, ctx, tool);
            return true;
        }
    }
    false
}

#[cfg(test)]
pub(crate) fn open_panels_for_test(app: &PhotocraftApp) -> &std::collections::BTreeSet<String> {
    &app.vector_runtime.windows
}

pub fn set_panel_open(app: &mut PhotocraftApp, name: &str, open: bool) {
    let Some(tab) = crate::dock::Tab::from_label(name).filter(|tab| tab.vector_panel() || *tab == crate::dock::Tab::Symbols) else { return };
    if open {
        crate::dock::reveal_feature(app, tab);
    } else if !app.ui.dock.hidden_tabs.contains(&tab) {
        app.ui.dock.hidden_tabs.push(tab);
    }
    sync_panel_checks(app);
}
fn open_panel(app: &mut PhotocraftApp, name: &str) {
    set_panel_open(app, name, true);
}
pub fn menu(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, _params: &serde_json::Value) -> Option<Result<serde_json::Value, String>> {
    if let Some(name) = id.strip_prefix("window.vector.") {
        let visible = crate::dock::Tab::from_label(name).is_some_and(|tab| crate::dock::feature_visible(app, tab));
        set_panel_open(app, name, !visible);
        return Some(Ok(serde_json::json!({"handled":true})));
    }
    let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
    if app.session.prefs().toolbar.persona == Persona::Vector {
        if id == "edit.copy" {
            crate::object_path::app_hooks::copy(app, ctx);
            return Some(Ok(serde_json::json!({"handled":true})));
        }
        if let Some(command) = id.strip_prefix("object.paste") {
            let command = if command == "Front" {
                "pasteFront"
            } else if command == "Back" {
                "pasteBack"
            } else {
                return None;
            };
            return Some(
                crate::object_path::app_hooks::menu_command(app, ctx, command).map(|()| serde_json::json!({"handled":true})).map_err(|e| e.to_string()),
            );
        }
        if let Some(action) = id.strip_prefix("vector.paint.") {
            use crate::fill_stroke_proxy::{Action, ActivePaint};
            let action = match action {
                "swap" => Some(Action::Swap),
                "default" => Some(Action::Default),
                "none" => Some(Action::None),
                "active" => {
                    app.ui.fill_stroke_proxy.active =
                        if app.ui.fill_stroke_proxy.active == ActivePaint::Fill { ActivePaint::Stroke } else { ActivePaint::Fill };
                    None
                }
                _ => return None,
            };
            let result = if let Some(action) = action { app.ui.fill_stroke_proxy.apply(&mut app.session, &ids, &action) } else { Ok(()) };
            return Some(result.map(|()| serde_json::json!({"handled":true})).map_err(|e| e.to_string()));
        }
        if id == "vector.drawMode.cycle" {
            use photocraft_engine::vector_packages::clip_compound::DrawMode;
            app.clip_compound_state.cancel();
            app.clip_compound_state.settings.mode = match app.clip_compound_state.settings.mode {
                DrawMode::Normal => DrawMode::Behind,
                DrawMode::Behind if crate::clip_compound::inside_enabled(&app.session) => DrawMode::Inside,
                _ => DrawMode::Normal,
            };
            return Some(Ok(serde_json::json!({"handled":true})));
        }
    }
    if let Some(kind) = id.strip_prefix("ui.vector.transform.") {
        use crate::vector_transform_tools::Kind;
        app.vector_transform_tools.kind = match kind {
            "move" => Kind::Move,
            "rotate" => Kind::Rotate,
            "reflect" => Kind::Reflect,
            "scale" => Kind::Scale,
            "shear" => Kind::Shear,
            "each" => Kind::Each,
            _ => return None,
        };
        return Some(app.vector_transform_tools.open_dialog(&app.session, &ids).map(|()| serde_json::json!({"handled":true})).map_err(|e| e.to_string()));
    }
    if let Some(command) = id.strip_prefix("ui.vector.path.").or_else(|| (id == "object.path.join").then_some("join")) {
        return Some(crate::object_path::app_hooks::menu_command(app, ctx, command).map(|()| serde_json::json!({"handled":true})).map_err(|e| e.to_string()));
    }
    if let Some(index) = id.strip_prefix("ui.vector.effect.").and_then(|n| n.parse::<usize>().ok()) {
        let mut bridge = crate::path_effects::app_hooks::bridge(ctx);
        let result = photocraft_engine::vector_packages::path_effects::access().and_then(|access| {
            let effect = photocraft_doc::vector_design::effects::EffectKind::presets()
                .into_iter()
                .nth(index)
                .ok_or_else(|| photocraft_engine::EngineError::Other("unknown path effect".into()))?;
            bridge.state.open_add(&app.session, &ids, effect, crate::path_effects::EffectTarget::Layer, access)
        });
        crate::path_effects::app_hooks::store(ctx, bridge);
        return Some(result.map(|()| serde_json::json!({"handled":true})).map_err(|e| e.to_string()));
    }
    if let Some(query) =
        id.strip_prefix("ui.vector.select.").or_else(|| (id == "select.deselect" && app.session.prefs().toolbar.persona == Persona::Vector).then_some("none"))
    {
        use photocraft_engine::vector_packages::selection::Query;
        let mut b = crate::vector_packages::selection::bridge(ctx);
        let chosen = match query {
            "all" => Some(Query::All),
            "none" => Some(Query::Deselect),
            "inverse" => Some(Query::Inverse),
            "above" => Some(Query::Next { above: true }),
            "below" => Some(Query::Next { above: false }),
            "reselect" => Some(Query::Reselect { ids: Vec::new() }),
            _ if query.starts_with("same.") => {
                query[5..].parse::<usize>().ok().and_then(|i| crate::vector_packages::selection::SAME.get(i)).and_then(|(criterion, _)| {
                    ids.first().map(|reference| Query::Same {
                        criterion: *criterion,
                        reference: *reference,
                        compare: b.state.settings.compare,
                        value: b.state.settings.value,
                        item: b.state.settings.item,
                    })
                })
            }
            _ if query.starts_with("object.") => query[7..]
                .parse::<usize>()
                .ok()
                .and_then(|i| crate::vector_packages::selection::OBJECT.get(i))
                .map(|(criterion, _)| Query::Object { criterion: *criterion }),
            _ => None,
        };
        let result = chosen.ok_or_else(|| photocraft_engine::EngineError::Other("Select a reference object".into())).and_then(|query| {
            let scope = photocraft_engine::vector_packages::selection::session_scope(&app.session)?;
            crate::vector_packages::selection::execute(&mut app.session, &mut b.state, scope, query)
        });
        if result.is_ok()
            && let Some(st) = app.session.active()
        {
            crate::vector_packages::selection::clear_nodes(&mut app.ui.direct_selection, st.doc.id);
        }
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("vector-selection-32"), b));
        return Some(result.map(|()| serde_json::json!({"handled":true})).map_err(|e| e.to_string()));
    }
    let result = match id {
        "ui.vector.layout" => {
            open_panel(app, "Transform");
            Ok(())
        }
        "ui.vector.blend" => {
            app.ui.vector_blend.open = true;
            Ok(())
        }
        "ui.vector.repeat" => {
            crate::repeat::app_hooks::open(ctx);
            Ok(())
        }
        "ui.vector.recolor" => photocraft_engine::vector_packages::colours::access()
            .and_then(|access| app.vector_runtime.recolor.open(&app.session, access, &ids))
            .map_err(|e| e.to_string()),
        "ui.vectorize" | "layer.vectorize" => {
            app.trace_dialog.open(&mut app.session, &app.ui.trace_preferences);
            Ok(())
        }
        "vector.transform.again" => photocraft_engine::vector_packages::transform_tools::again_request(
            &app.session,
            &app.session.active().map(|s| s.selected_layers()).unwrap_or_default(),
        )
        .and_then(|r| app.session.execute("vector.transform.apply", serde_json::json!(r)).map(|_| ()))
        .map_err(|e| e.to_string()),
        "vector.view.outline" => {
            let mut b = crate::isolation::bridge(ctx);
            b.views.toggle_outline();
            crate::isolation::store_session(&mut app.session, ctx, b);
            Ok(())
        }
        "vector.snap.smartGuides" => {
            let enabled = !app.shared_tools.snapping.alignment;
            app.shared_tools.snapping.alignment = enabled;
            app.shared_tools.snapping.spacing = enabled;
            app.ui.view.show.smart_guides = enabled;
            app.ui.vector_snapping = app.shared_tools.snapping.clone();
            Ok(())
        }
        "vector.object.lock" | "vector.object.hide" => {
            let target = crate::toolsets_ui::persona::GestureTarget::capture(app);
            target.ok_or_else(|| "Select objects".to_string()).and_then(|target| {
                if !target.is_current(app) {
                    return Err("The document or selection changed; try again".into());
                }
                let lock = id == "vector.object.lock";
                app.session
                    .edit(if lock { "Lock Objects" } else { "Hide Objects" }, move |doc, _| {
                        for id in target.layers {
                            let layer = doc.layer_mut(id).ok_or(photocraft_engine::EngineError::NoLayer(id))?;
                            if lock {
                                layer.locks.all = true;
                            } else {
                                layer.visible = false;
                            }
                        }
                        Ok(())
                    })
                    .map_err(|e| e.to_string())
            })
        }
        "object.clippingMask.make" => {
            crate::clip_compound::execute(&mut app.session, &mut app.clip_compound_state, crate::clip_compound::Command::MakeClip).map_err(|e| e.to_string())
        }
        "object.clippingMask.release" => {
            crate::clip_compound::execute(&mut app.session, &mut app.clip_compound_state, crate::clip_compound::Command::ReleaseClip).map_err(|e| e.to_string())
        }
        "object.compoundPath.release" => {
            crate::clip_compound::execute(&mut app.session, &mut app.clip_compound_state, crate::clip_compound::Command::ReleaseCompound)
                .map_err(|e| e.to_string())
        }
        "object.compoundPath.make" => {
            crate::clip_compound::execute(&mut app.session, &mut app.clip_compound_state, crate::clip_compound::Command::MakeCompound)
                .map_err(|e| e.to_string())
        }
        _ => return None,
    };
    Some(result.map(|()| serde_json::json!({"handled":true})))
}
pub fn menus(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if app.session.prefs().toolbar.persona != Persona::Vector {
        return;
    }
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button(li_i18n::t("Vector"), |ui| {
            ui.menu_button(li_i18n::t("View"), |ui| {
                let mut b = crate::isolation::bridge(ui.ctx());
                crate::view_modes::menu(ui, &mut b.views, |s| li_i18n::t(s).into());
                crate::isolation::store_session(&mut app.session, ui.ctx(), b);
            });
            for name in ["Appearance", "Graphic Styles", "Frame Composition", "Mesh", "Snapping Manager"] {
                if ui.button(li_i18n::t(name)).clicked() {
                    open_panel(app, name);
                    ui.close();
                }
            }
            if ui.button(li_i18n::t("Vectorize / Image Trace")).clicked() {
                app.trace_dialog.open(&mut app.session, &app.ui.trace_preferences);
                ui.close();
            }
            if ui.button(li_i18n::t("Export SVG…")).clicked() {
                let r = crate::vector_packages::svg_export::open_dialog(app);
                if let Err(e) = r {
                    report(app, e);
                }
                ui.close();
            }
            if ui.button(li_i18n::t("Export PDF…")).clicked() {
                let r = crate::vector_packages::pdf_export::open_dialog(app);
                if let Err(e) = r {
                    report(app, e);
                }
                ui.close();
            }
        });
    });
}
pub(crate) fn keyboard_preset(ui: &mut egui::Ui, preset: &mut photocraft_engine::vector_toolbar_layout::Preset) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(li_i18n::t("Vector Keyboard Preset"));
        egui::ComboBox::from_id_salt("vector-keyboard-preset").selected_text(format!("{preset:?}")).show_ui(ui, |ui| {
            for choice in [photocraft_engine::vector_toolbar_layout::Preset::Illustrator, photocraft_engine::vector_toolbar_layout::Preset::Affinity] {
                changed |= ui.selectable_value(preset, choice, format!("{choice:?}")).changed();
            }
        });
    });
    changed
}

fn sync_panel_checks(app: &mut PhotocraftApp) {
    app.vector_runtime.windows = crate::dock::Tab::ALL
        .into_iter()
        .filter(|tab| tab.vector_panel() && crate::dock::feature_visible(app, *tab))
        .map(|tab| tab.label().to_owned())
        .collect();
}

/// Same content and detached preview lifecycle, whether docked or torn off.
pub(crate) fn dock_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui, tab: crate::dock::Tab) {
    let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
    match tab.label() {
        "Appearance" => {
            let mut state = std::mem::take(&mut app.shared_tools.paint);
            if let Err(e) = crate::vector_packages::appearance::panel(ui, &mut state, &mut app.session, &ids) {
                report(app, e);
            }
            app.shared_tools.paint = state;
            crate::path_effects::app_hooks::panel(app, ui);
        }
        "SVG Import" => crate::vector_packages::svg_import::controls(ui, &mut app.svg_import, &mut app.session),
        "Affinity Import" => crate::vector_packages::affinity::controls(ui, &mut app.affinity_import, &mut app.session),
        "Document Palette" => palette(app, ui),
        "Graphic Styles" => crate::graphic_styles::panel(app, ui),
        "Frame Composition" => {
            if let Err(e) = crate::frame_compose::panel(ui, &mut app.ui.frame_compose, &mut app.session, &crate::frame_compose::NativeMetadata) {
                report(app, e);
            }
        }
        "Mesh" => {
            if let Err(e) = app.mesh_state.context_bar(ui, &mut app.session, &ids).and_then(|_| app.mesh_state.point_controls(ui, &mut app.session, &[])) {
                report(app, e);
            }
        }
        "Snapping Manager" => {
            let mut manager = crate::vector_packages::snapping::Manager { settings: app.shared_tools.snapping.clone() };
            crate::vector_packages::snapping::manager(ui, &mut manager, |s| li_i18n::t(s).into());
            app.shared_tools.snapping = manager.settings.clone();
            app.ui.vector_snapping = manager.settings;
        }
        "Transform" | "Align" | "Layout" => crate::vector_packages::layout_app_hooks::dock_panel(app, ui, tab),
        "Pathfinder" => {
            ui.horizontal_wrapped(|ui| crate::toolsets_ui::persona::geometry_bar(app, ui));
        }
        "Brushes" => crate::vector_brush::context_bar(ui, &mut app.vector_brush),
        _ => {}
    }
}

fn windows(app: &mut PhotocraftApp, ctx: &egui::Context) {
    sync_panel_checks(app);
    crate::vector_packages::svg_export::frame(app, ctx);
    let mut recolor = std::mem::take(&mut app.vector_runtime.recolor);
    match photocraft_engine::vector_packages::colours::access().and_then(|access| crate::recolor::dialog(ctx, &app.session, access, &mut recolor)) {
        Ok(Some(plan)) => {
            if let Err(e) = app.session.execute("recolor.apply", serde_json::json!(plan)) {
                report(app, e);
            }
        }
        Err(e) => report(app, e),
        _ => {}
    }
    app.vector_runtime.recolor = recolor;
    let mut prefs = std::mem::take(&mut app.ui.trace_preferences);
    crate::trace_dialog::show(ctx, &mut app.trace_dialog, &mut app.session, &mut prefs, &[]);
    app.ui.trace_preferences = prefs;
}

/// Shared menu/catalog entries, with captured feature adapters above.
/// Move's Auto-Select uses vector geometry/scope, including empty raster caches.
/// The resulting object selection is the same engine path as Select and vector tools.
pub(crate) fn pick_object(app: &mut PhotocraftApp, point: [f64; 2], add: bool, group: bool) {
    use photocraft_engine::vector_packages::selection::{self as engine, Mode, Query, Request};
    let result = (|| {
        let st = app.session.active().ok_or(photocraft_engine::EngineError::NoDocument)?;
        let scope = engine::session_scope(&app.session)?;
        let hit = engine::hit(&st.doc, &scope, point, 6. / f64::from(app.current_zoom()).max(0.01), engine::installed_readers())?;
        let query = match hit {
            Some(leaf) if group => Query::Group { leaf, previous: None },
            Some(id) => Query::Reselect { ids: vec![id] },
            None => Query::Deselect,
        };
        let request = Request { document: st.doc.id, revision: st.revision, scope, mode: if add { Mode::Add } else { Mode::Replace }, query };
        let preview = engine::preview(&st.doc, st.revision, &st.selected_layers(), &request, engine::installed_readers())?;
        engine::apply(&mut app.session, &preview)
    })();
    if let Err(e) = result {
        report(app, e);
    }
}

/// Preconditions shared by Object main-menu rows and the canvas object menu.
/// Query-only: disabled clicks never call a command or create a preview.
pub fn disabled_reason(app: &PhotocraftApp, id: &str) -> Option<String> {
    use photocraft_engine::vector_packages::{clip_compound, transform_tools};
    let vector_layer_action = app.session.prefs().toolbar.persona == Persona::Vector
        && (matches!(id, "layer.groupLayers" | "layer.ungroupLayers" | "type.convertToShape") || id.starts_with("layer.arrange."));
    if !handles(id) && !vector_layer_action && !matches!(id, "path.outlineStroke" | "vector.transform.again") {
        return None;
    }
    let st = match app.session.active() {
        Some(st) => st,
        None => return Some(tl!("Open a photo first.").into()),
    };
    let object = id.starts_with("ui.vector.transform.")
        || id.starts_with("ui.vector.path.")
        || id.starts_with("ui.vector.effect.")
        || id.starts_with("vector.object.")
        || id.starts_with("object.clippingMask.")
        || id.starts_with("object.compoundPath.")
        || matches!(id, "object.path.join" | "vector.transform.again")
        || (app.session.prefs().toolbar.persona == Persona::Vector
            && (matches!(id, "layer.groupLayers" | "layer.ungroupLayers" | "type.convertToShape") || id.starts_with("layer.arrange.")));
    if object && app.session.prefs().toolbar.persona != Persona::Vector {
        return Some(tl!("Switch to the Vector persona first.").into());
    }
    let ids = st.selected_layers();
    if id == "path.outlineStroke" {
        if ids.is_empty() || !ids.iter().all(|id| st.doc.layer(*id).is_some_and(|l| matches!(&l.content, LayerContent::Shape(shape) if shape.stroke.is_some())))
        {
            return Some(tl!("Select a shape with a stroke first.").into());
        }
        return app.session.disabled_reason(id).map(|raw| crate::command_feedback::message(&raw));
    }
    if matches!(id, "layer.vectorize" | "ui.vectorize") {
        return app.session.disabled_reason("layer.vectorize").map(|raw| crate::command_feedback::message(&raw));
    }
    if object && ids.is_empty() {
        return Some(tl!("Select a vector object first.").into());
    }
    if object
        && ids.iter().any(|id| {
            let locks = st.doc.effective_locks(*id);
            locks.all || locks.pixels || locks.position
        })
    {
        return Some(tl!("This layer is locked.").into());
    }
    if id.starts_with("ui.vector.path.") || id == "object.path.join" {
        if !matches!(id, "ui.vector.path.cleanup" | "ui.vector.path.expand" | "ui.vector.path.expandAppearance")
            && !ids.iter().all(|id| st.doc.layer(*id).is_some_and(|l| matches!(l.content, LayerContent::Shape(_))))
        {
            return Some(tl!("Select a shape first.").into());
        }
        if let Err(e) = photocraft_engine::vector_packages::path_commands::Capture::begin(&app.session, &ids) {
            return Some(crate::command_feedback::message(&e.to_string()));
        }
    }
    if matches!(id, "ui.vector.path.expand" | "ui.vector.path.expandAppearance") {
        use photocraft_engine::vector_packages::{expand, path_commands};
        return match path_commands::Capture::begin(&app.session, &ids) {
            Ok(capture) => {
                expand::disabled_reason(&app.session, &expand::Request { capture, options: Default::default(), appearance: id.ends_with("expandAppearance") })
            }
            Err(e) => Some(crate::command_feedback::message(&e.to_string())),
        };
    }
    if id.starts_with("ui.vector.effect.")
        && !ids.iter().all(|id| st.doc.layer(*id).is_some_and(|l| matches!(l.content, LayerContent::Shape(_) | LayerContent::Text(_))))
    {
        return Some(tl!("Select a shape or text object first.").into());
    }
    if id.starts_with("ui.vector.transform.") {
        return transform_tools::preflight(&st.doc, &ids).err().map(|e| crate::command_feedback::message(&e.to_string()));
    }
    if id == "vector.transform.again" {
        return transform_tools::again_request(&app.session, &ids).err().map(|e| crate::command_feedback::message(&e.to_string()));
    }
    if id.starts_with("object.clippingMask.") || id.starts_with("object.compoundPath.") {
        if let Err(e) = clip_compound::Capture::selected(&app.session) {
            return Some(crate::command_feedback::message(&e.to_string()));
        }
        let layer = ids.first().and_then(|id| st.doc.layer(*id));
        let why = match id {
            "object.clippingMask.make" if ids.len() < 2 => Some(tl!("Select a clipping path and at least one content object.")),
            "object.clippingMask.make"
                if !ids.last().and_then(|id| st.doc.layer(*id)).is_some_and(|l| matches!(l.content, LayerContent::Shape(_) | LayerContent::Text(_))) =>
            {
                Some(tl!("Select a shape or text object above the objects to clip."))
            }
            "object.clippingMask.release" if ids.len() != 1 || !ids.first().is_some_and(|id| clip_compound::clip_parent(&st.doc, *id).is_some()) => {
                Some(tl!("Select a clipping group first."))
            }
            "object.compoundPath.make"
                if ids.len() < 2 || !ids.iter().all(|id| st.doc.layer(*id).is_some_and(|l| matches!(l.content, LayerContent::Shape(_)))) =>
            {
                Some(tl!("Select shapes to make a compound path."))
            }
            "object.compoundPath.release"
                if ids.len() != 1
                    || !layer.is_some_and(|l| matches!(&l.content, LayerContent::Shape(sh) if sh.path.subpaths.len() >= 2 && !sh.path.inverted)) =>
            {
                Some(tl!("Select a compound path first."))
            }
            _ => None,
        };
        if let Some(why) = why {
            return Some(why.into());
        }
        if id.ends_with(".make") {
            let paths: Vec<_> = ids.iter().filter_map(|id| st.doc.path_of(*id)).collect();
            if paths.first().is_some_and(|first| paths.iter().any(|p| p[..p.len() - 1] != first[..first.len() - 1])) {
                return Some(tl!("Select objects in the same group first.").into());
            }
        }
    }
    if id.starts_with("vector.object.")
        && !ids.iter().all(|id| {
            st.doc
                .layer(*id)
                .is_some_and(|l| matches!(l.content, LayerContent::Shape(_) | LayerContent::Text(_) | LayerContent::Group(_) | LayerContent::Symbol(_)))
        })
    {
        return Some(tl!("Select a vector object first.").into());
    }
    if id == "object.path.join"
        && !ids.iter().any(|id| {
            st.doc
                .layer(*id)
                .is_some_and(|l| matches!(&l.content, LayerContent::Shape(sh) if sh.path.subpaths.iter().any(|sp| !sp.closed && sp.knots.len() >= 2)))
        })
    {
        return Some(tl!("Select an open path first.").into());
    }
    None
}

pub fn menu_items(app: &PhotocraftApp) -> Vec<li_commands::menu::MenuItem> {
    let mut rows: Vec<(&str, &str, Vec<&str>)> = vec![
        ("path.outlineStroke", "Outline Stroke", vec!["Layer", "Geometry"]),
        ("layer.vectorize", "Vectorize / Image Trace", vec!["Layer"]),
        ("ui.vector.layout", "Transform and Layout", vec!["Window"]),
        ("ui.vector.recolor", "Recolor Artwork…", vec!["Edit", "Edit Colors"]),
        ("ui.vector.blend", "Blend Options", vec!["Object", "Blend"]),
        ("ui.vector.repeat", "Repeat Options", vec!["Object", "Repeat"]),
        ("object.path.join", "Join", vec!["Object", "Path"]),
        ("ui.vector.path.addAnchors", "Add Anchor Points", vec!["Object", "Path"]),
        ("ui.vector.path.removeAnchors", "Remove Anchor Points", vec!["Object", "Path"]),
        ("ui.vector.path.divideBelow", "Divide Objects Below", vec!["Object", "Path"]),
        ("ui.vector.path.average", "Average…", vec!["Object", "Path"]),
        ("ui.vector.path.simplify", "Simplify…", vec!["Object", "Path"]),
        ("ui.vector.path.grid", "Split Into Grid…", vec!["Object", "Path"]),
        ("ui.vector.path.cleanup", "Clean Up…", vec!["Object", "Path"]),
        ("ui.vector.path.outline", "Outline Stroke", vec!["Object", "Path"]),
        ("ui.vector.path.offset", "Offset Path…", vec!["Object", "Path"]),
        ("ui.vector.path.expand", "Expand…", vec!["Object"]),
        ("ui.vector.path.expandAppearance", "Expand Appearance", vec!["Object"]),
        ("vector.transform.again", "Transform Again", vec!["Object", "Transform"]),
        ("vector.object.lock", "Lock Objects", vec!["Object"]),
        ("vector.object.hide", "Hide Objects", vec!["Object"]),
        ("vector.object.unlockAll", "Unlock All", vec!["Object"]),
        ("vector.object.showAll", "Show All", vec!["Object"]),
        ("object.clippingMask.make", "Make Clipping Mask", vec!["Object", "Clipping Mask"]),
        ("object.clippingMask.release", "Release Clipping Mask", vec!["Object", "Clipping Mask"]),
        ("object.compoundPath.make", "Make Compound Path", vec!["Object", "Compound Path"]),
        ("object.compoundPath.release", "Release Compound Path", vec!["Object", "Compound Path"]),
        ("ui.vector.view.vector", "Vector View", vec!["View"]),
        ("vector.view.outline", "Outline", vec!["View"]),
        ("ui.vector.view.xray", "X-ray", vec!["View"]),
        ("ui.vector.view.pixels", "Pixel Preview", vec!["View"]),
        ("vector.snap.smartGuides", "Smart Guides", vec!["View"]),
        ("ui.vector.select.all", "All Vector Objects", vec!["Select"]),
        ("ui.vector.select.none", "Deselect Vector Objects", vec!["Select"]),
        ("ui.vector.select.inverse", "Inverse Vector Selection", vec!["Select"]),
        ("ui.vector.select.above", "Next Object Above", vec!["Select"]),
        ("ui.vector.select.below", "Next Object Below", vec!["Select"]),
        ("ui.vector.select.reselect", "Reselect Vector Objects", vec!["Select"]),
    ];
    let vector = app.session.prefs().toolbar.persona == Persona::Vector;
    rows.retain(|(_, _, path)| vector || matches!(path[0], "File" | "Layer" | "Window"));
    let item = |id: String, label: String, path: Vec<String>| li_commands::menu::MenuItem {
        checked: if let Some(name) = id.strip_prefix("window.vector.") {
            Some(app.vector_runtime.windows.contains(name))
        } else {
            crate::menus::checked(app, &id)
        },
        enabled: disabled_reason(app, &id).is_none(),
        tooltip: disabled_reason(app, &id),
        id,
        label,
        path,
        shortcut: None,
        color: None,
    };
    let mut out: Vec<_> = rows
        .into_iter()
        .map(|(id, label, path)| {
            let mut row = item(id.into(), label.into(), path.into_iter().map(Into::into).collect());
            if matches!(id, "vector.object.unlockAll" | "vector.object.showAll") {
                row.enabled = app.session.is_enabled(id);
                row.shortcut = crate::shortcut_dispatch::binding_strings(app).into_iter().find(|(command, _)| command == id).map(|(_, key)| key);
                if !row.enabled && row.tooltip.is_none() {
                    // Every disabled row explains itself (U029).
                    row.tooltip = Some(match app.session.disabled_reason(id) {
                        Some(raw) => crate::command_feedback::message(&raw),
                        None if app.session.active().is_none() => tl!("Open a photo first.").into(),
                        None if id == "vector.object.unlockAll" => tl!("Nothing in this document is locked.").into(),
                        None => tl!("Nothing in this document is hidden.").into(),
                    });
                }
            }
            row
        })
        .collect();
    for name in ["Appearance", "Graphic Styles", "Frame Composition", "Mesh", "Snapping Manager", "Document Palette"] {
        out.push(item(format!("window.vector.{name}"), name.into(), vec!["Window".into()]));
    }
    if vector {
        for (kind, label) in
            [("move", "Move…"), ("rotate", "Rotate…"), ("reflect", "Reflect…"), ("scale", "Scale…"), ("shear", "Shear…"), ("each", "Transform Each…")]
        {
            out.push(item(format!("ui.vector.transform.{kind}"), label.into(), vec!["Object".into(), "Transform".into()]));
        }
        for (index, (_, label)) in crate::vector_packages::selection::SAME.iter().enumerate() {
            out.push(item(format!("ui.vector.select.same.{index}"), (*label).into(), vec!["Select".into(), "Same".into()]));
        }
        for (index, (_, label)) in crate::vector_packages::selection::OBJECT.iter().enumerate() {
            out.push(item(format!("ui.vector.select.object.{index}"), (*label).into(), vec!["Select".into(), "Object".into()]));
        }
        for (index, effect) in photocraft_doc::vector_design::effects::EffectKind::presets().into_iter().enumerate() {
            out.push(item(
                format!("ui.vector.effect.{index}"),
                effect.label().into(),
                vec![
                    "Effect".into(),
                    if index < 5 {
                        "Distort & Transform"
                    } else if index < 7 {
                        "Stylize"
                    } else {
                        "Other Path Effects"
                    }
                    .into(),
                ],
            ));
        }
    }
    out
}
pub fn handles(id: &str) -> bool {
    id.starts_with("vector.paint.")
        || id == "vector.drawMode.cycle"
        || matches!(id, "object.pasteFront" | "object.pasteBack")
        || id.starts_with("ui.vector.")
        || id.starts_with("window.vector.")
        || id.starts_with("object.clippingMask.")
        || id.starts_with("object.compoundPath.")
        || matches!(
            id,
            "object.path.join" | "layer.vectorize" | "vector.object.lock" | "vector.object.hide" | "vector.view.outline" | "vector.snap.smartGuides"
        )
}

fn owned_preview(runtime: &mut State, doc: &Document, key: u64) -> Arc<Document> {
    if runtime.owned_preview.as_ref().is_none_or(|(id, k, _)| *id != doc.id.0 || *k != key) {
        runtime.owned_preview = Some((doc.id.0, key, Arc::new(doc.clone())));
    }
    runtime.owned_preview.as_ref().map_or_else(|| Arc::new(doc.clone()), |(_, _, doc)| doc.clone())
}
/// Native document palette alongside the unchanged application swatches.
pub fn palette(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // The legacy ASE worker remains outside this task's function ownership.
    let _ = begin_ase;
    crate::panels::swatches_panel::panel(app, ui);
}

type AseResult = (
    photocraft_engine::vector_packages::colours::Capture,
    Result<(photocraft_doc::vector_design::swatches::SwatchBook, Vec<photocraft_doc::vector_design::exchange::VectorWarning>), String>,
);
fn begin_ase(app: &mut PhotocraftApp, capture: photocraft_engine::vector_packages::colours::Capture) {
    let (tx, rx) = std::sync::mpsc::channel();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(pick) = &mut app.services.pick_open_paths {
        let Some(path) = pick().and_then(|paths| paths.into_iter().next()) else { return };
        std::thread::spawn(move || {
            use std::io::Read;
            let limits = photocraft_doc::vector_design::exchange::ImportLimits::default();
            let result = (|| {
                let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                if file.metadata().map_err(|e| e.to_string())?.len() > limits.bytes as u64 {
                    return Err("ASE byte budget exceeded".into());
                }
                let mut bytes = Vec::new();
                file.take(limits.bytes as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                photocraft_doc::vector_design::swatches::import_ase(&bytes, limits)
            })();
            let _ = tx.send((capture, result));
        });
        app.vector_runtime.ase = Some(rx);
        return;
    }
    let Some((_, bytes)) = app.services.pick_open.as_mut().and_then(|pick| pick()) else { return };
    let result = bytes.and_then(|bytes| photocraft_doc::vector_design::swatches::import_ase(&bytes, Default::default()));
    let _ = tx.send((capture, result));
    app.vector_runtime.ase = Some(rx);
}
fn poll_ase(app: &mut PhotocraftApp) {
    let Some(result) = app.vector_runtime.ase.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
    app.vector_runtime.ase = None;
    let (capture, result) = result;
    match result {
        Ok((palette, warnings)) => {
            let request = photocraft_engine::vector_packages::colours::Request {
                capture,
                action: photocraft_engine::vector_packages::colours::Action::Import { palette, replace: false },
            };
            match app.session.execute("swatch.document.edit", serde_json::json!(request)) {
                Err(e) => report(app, e),
                Ok(_) => {
                    if !warnings.is_empty() {
                        app.ui.status = warnings.into_iter().map(|w| w.message).collect::<Vec<_>>().join("; ");
                    }
                }
            }
        }
        Err(e) => report(app, e),
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    #[allow(clippy::unwrap_used)]
    fn review_additional_vector_preferences_reset_and_unrelated_edits() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        preferences(&mut app, &ctx);
        let initial = app.width_state.options.clone();
        app.width_state.options.item = Some(2);
        app.session.prefs.edit(|p| {
            p.dialogs.insert("unrelated".into(), serde_json::json!(1));
        });
        preferences(&mut app, &ctx);
        assert_ne!(app.width_state.options, initial);
        app.session.prefs.edit(|p| p.reset(Some("dialogs"))).unwrap();
        preferences(&mut app, &ctx);
        assert_eq!(app.width_state.options, initial);
        crate::dock::select(&mut app, crate::dock::Tab::Symbols);
        assert_eq!(crate::dock::active_tab(&app, crate::dock::Place::Docked(crate::dock::Group::Layers)), Some(crate::dock::Tab::Symbols));
    }
}

#[cfg(test)]
#[path = "vector_integration/review_tests.rs"]
mod bugfix_vector_tests;
