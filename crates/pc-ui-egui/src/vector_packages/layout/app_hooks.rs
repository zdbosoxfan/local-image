//! Production full-app hook implementation for 00's serial registration lane. Tests use
//! these exact functions, with normal canvas pointer capture and GpuCanvas rendering.
use super::layout::{PointerMap, State, Tool as LayoutTool};
use crate::{
    PhotocraftApp,
    canvas::{ToolEvent, ViewXform},
    state::Tool,
};
use egui::{Context, Key};
use photocraft_doc::Document;
use photocraft_engine::vector_packages::layout as engine;
use photocraft_geom::Point;
use std::sync::Arc;
fn report(app: &mut PhotocraftApp, result: photocraft_engine::Result<()>) {
    if let Err(e) = result {
        app.shared_tools.layout.cancel();
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
}
pub fn synchronize(app: &mut PhotocraftApp, ctx: &Context) {
    let current = app.ui.tool.label();
    if app.shared_tools.layout.host_tool.is_some_and(|previous| previous != current) || !app.ui.dialogs.is_empty() {
        app.shared_tools.layout.cancel();
    }
    app.shared_tools.layout.host_tool = Some(current);
    app.shared_tools.layout.pixels_per_point = f64::from(ctx.pixels_per_point());
    let enabled = app.ui.tool == Tool::PathSelection && app.ui.dialogs.is_empty();
    // Transform panel fields work with ordinary selections even when no dedicated tool
    // is active. Switching away cancels a captured tool, never ordinary Move state.
    if !enabled && app.shared_tools.layout.tool != LayoutTool::None {
        app.shared_tools.layout.cancel();
        app.shared_tools.layout.tool = LayoutTool::None;
    }
    let result = app.shared_tools.layout.synchronize(&app.session, true);
    report(app, result);
    let cancel = ctx.input(|i| {
        !i.focused || i.key_pressed(Key::Escape) || i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false) | egui::Event::PointerGone))
    });
    if cancel {
        app.shared_tools.layout.cancel();
    } else if enabled
        && let Some(position) = ctx.input(|i| i.pointer.latest_pos())
        && let Some(xf) = ViewXform::active(app)
    {
        let raw = xf.to_doc(position);
        let p = app.shared_tools.layout.pointer_map.map_or(Point::new(raw[0], raw[1]), |map| map.point(position));
        let mods = ctx.input(|i| i.modifiers);
        let view = photocraft_engine::vector_packages::snapping::query::View { zoom: f64::from(xf.zoom), pixels_per_point: f64::from(ctx.pixels_per_point()) };
        // Recompute current modifiers on stationary captured frames too.
        let result = app.shared_tools.layout.motion(&app.session, p, mods, app.shared_tools.layout.pointer_map.map_or(view, |map| map.view));
        report(app, result);
    }
}
pub fn event(app: &mut PhotocraftApp, event: ToolEvent, mods: egui::Modifiers) -> bool {
    if app.ui.tool != Tool::PathSelection || app.shared_tools.layout.tool == LayoutTool::None {
        return false;
    }
    let view = photocraft_engine::vector_packages::snapping::query::View {
        zoom: f64::from(app.current_zoom()),
        pixels_per_point: app.shared_tools.layout.pixels_per_point,
    };
    let xf = ViewXform::active(app);
    let point =
        |state: &State, x: f64, y: f64| state.pointer_map.zip(xf.as_ref()).map_or(Point::new(x, y), |(map, xf)| map.point(xf.to_screen(x as f32, y as f32)));
    let result = match event {
        ToolEvent::Down { x, y, .. } => {
            let p = Point::new(x, y);
            let result = view.tolerance(6.).and_then(|tolerance| app.shared_tools.layout.down(&app.session, p, tolerance, view)).map(|_| ());
            if app.shared_tools.layout.captured()
                && let Some(xf) = &xf
            {
                let origin = xf.to_screen(x as f32, y as f32);
                let dx = xf.to_doc(origin + egui::vec2(1., 0.));
                let dy = xf.to_doc(origin + egui::vec2(0., 1.));
                app.shared_tools.layout.pointer_map =
                    Some(PointerMap { origin, document_origin: p, dx: Point::new(dx[0] - p.x, dx[1] - p.y), dy: Point::new(dy[0] - p.x, dy[1] - p.y), view });
            }
            result
        }
        ToolEvent::Move { x, y, .. } => {
            let p = point(&app.shared_tools.layout, x, y);
            let view = app.shared_tools.layout.pointer_map.map_or(view, |map| map.view);
            app.shared_tools.layout.motion(&app.session, p, mods, view)
        }
        ToolEvent::Up { x, y } => {
            let p = point(&app.shared_tools.layout, x, y);
            let view = app.shared_tools.layout.pointer_map.map_or(view, |map| map.view);
            app.shared_tools.layout.up(&mut app.session, p, mods, view)
        }
    };
    report(app, result);
    true
}
pub fn preview(app: &PhotocraftApp, idx: usize) -> Option<(Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) {
        return None;
    }
    let state = &app.shared_tools.layout;
    if !state.is_fresh(&app.session) {
        return None;
    }
    let st = app.session.active()?;
    let doc = state.preview.as_ref()?;
    if doc.id != st.doc.id {
        return None;
    }
    Some((doc.clone(), (1 << 46) | state.generation))
}
/// Wrap the result of the existing display_doc function, so Move/gradient/effect previews
/// retain their normal precedence. Clipping is projected identically for GPU and CPU fallback.
pub fn project_display(app: &mut PhotocraftApp, source: Arc<Document>, key: u64) -> (Arc<Document>, u64) {
    let revision = app.session.documents().iter().find(|d| d.doc.id == source.id).map_or(0, |d| d.revision);
    if let Some((id, rev, hash, original, projected)) = &app.shared_tools.layout.display_cache
        && *id == source.id.0
        && *rev == revision
        && *hash == key
        && Arc::ptr_eq(original, &source)
    {
        return (projected.clone(), key);
    }
    if !source.walk().iter().any(|(_, _, l)| l.vector.clip.is_some()) {
        app.shared_tools.layout.display_cache = Some((source.id.0, revision, key, source.clone(), source.clone()));
        return (source, key);
    }
    match engine::project_clips(&source) {
        Ok(doc) => {
            let doc = Arc::new(doc);
            app.shared_tools.layout.display_cache = Some((source.id.0, revision, key, source, doc.clone()));
            (doc, key)
        }
        Err(e) => {
            report(app, Err(e));
            (source, key)
        }
    }
}
pub fn overlay(app: &PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    super::layout::overlay(&app.shared_tools.layout, &app.session, painter, |p| xf.to_screen(p.x as f32, p.y as f32));
}
/// Preserve configured legacy physical units; the default pixel preference takes the
/// active document's design units. Design points always use the exact 72 pt/in definition.
pub fn ruler_units(app: &PhotocraftApp) -> (photocraft_engine::prefs::Unit, f64) {
    use photocraft_doc::vector_design::structure::DesignUnits;
    use photocraft_engine::prefs::Unit;
    let prefs = &app.session.prefs().units_and_rulers;
    let legacy = (prefs.rulers, prefs.point_size.per_inch());
    if prefs.rulers != Unit::Pixels {
        return legacy;
    }
    match app.session.active().map(|s| s.doc.design_units) {
        Some(DesignUnits::Points) => (Unit::Points, 72.),
        Some(DesignUnits::Millimeters) => (Unit::Millimeters, 72.),
        Some(DesignUnits::Inches) => (Unit::Inches, 72.),
        _ => legacy,
    }
}
/// Package 00 places this button in the existing context/properties UI; it opens the
/// same panel used by keyboard fields and dedicated tool selection.
pub fn context(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if crate::widgets::secondary_button(ui, crate::i18n::t("Transform and Layout"), 0.0).clicked() {
        app.shared_tools.layout.open = true;
    }
}
/// Legacy open requests reveal the persisted Transform tab; no independent floating window.
pub fn window(app: &mut PhotocraftApp, _ctx: &Context) {
    if app.shared_tools.layout.open {
        app.shared_tools.layout.open = false;
        crate::dock::reveal_feature(app, crate::dock::Tab::Transform);
    }
}

pub fn dock_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui, tab: crate::dock::Tab) {
    let mut state: State = std::mem::take(&mut app.shared_tools.layout);
    let before = state.tool;
    let result = super::layout::panel_section(ui, &mut state, &mut app.session, Some(tab), |s| crate::i18n::t(s).to_string());
    if state.tool != before && state.tool != LayoutTool::None {
        app.ui.tool = Tool::PathSelection;
    }
    state.host_tool = Some(app.ui.tool.label());
    app.shared_tools.layout = state;
    report(app, result);
}
