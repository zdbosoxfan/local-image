//! Move owns the press until release. egui's click/drag threshold and changing widget IDs must
//! not determine the lifetime of an editing gesture.

use egui::{Context, Pos2, Response};
use photocraft_doc::DocId;

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, ViewXform};
use crate::state::Tool;

#[derive(Clone, Copy)]
struct Capture {
    doc: DocId,
    origin: Pos2,
    start: [f64; 2],
    camera: crate::view_camera::Camera,
}

fn key() -> egui::Id {
    egui::Id::new("pc-move-capture")
}

pub(crate) fn captured(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<Capture>(key()).is_some())
}

pub(crate) fn cancel(app: &mut PhotocraftApp, ctx: &Context) {
    ctx.data_mut(|d| d.remove::<Capture>(key()));
    app.drag = None;
    app.move_preview = None;
    app.move_fast = None;
    app.guide_drag = None;
    app.prefs_rt.snap = None;
    app.prefs_rt.snap_lines.clear();
    crate::move_mods::abandon(app);
}

/// Run before painting, so this frame's pixels use this frame's input. A captured pointer can
/// cross panels, leave the viewport, or survive a relayout without losing its source origin.
pub(crate) fn canvas_input(app: &mut PhotocraftApp, response: &Response, xf: &ViewXform) -> bool {
    let ctx = &response.ctx;
    let mut capture = ctx.data(|d| d.get_temp::<Capture>(key()));
    let (pressed, released, down, pos, mods, focused) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.primary_down(),
            i.pointer.latest_pos().or(i.pointer.interact_pos()),
            i.modifiers,
            i.focused && !i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))),
        )
    });
    if let Some(c) = capture {
        if app.session.active().is_none_or(|s| s.doc.id != c.doc) || app.ui.tool != Tool::Move || !focused || !app.ui.dialogs.is_empty() {
            cancel(app, ctx);
            return true;
        }
    } else {
        if app.ui.tool != Tool::Move || crate::hold_keys::held_tool(app, ctx).is_some() || !app.ui.dialogs.is_empty() || !pressed {
            return false;
        }
        let Some(p) = ctx
            .input(|i| {
                i.pointer.press_origin().or_else(|| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } => Some(*pos),
                        _ => None,
                    })
                })
            })
            .filter(|p| xf.rect.contains(*p) && ctx.layer_id_at(*p) == Some(response.layer_id))
        else {
            return false;
        };
        let Some(doc) = app.session.active().map(|s| s.doc.id) else { return false };
        let start = xf.to_doc(p);
        let c = Capture { doc, origin: p, start, camera: xf.camera() };
        capture = Some(c);
        ctx.data_mut(|d| d.insert_temp(key(), c));
        if app.ui.transform.is_none() {
            crate::canvas::begin_transform_controls_at(app, ctx, xf, p);
        }
        let press_mods = ctx
            .input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed: true, modifiers, .. } => Some(*modifiers),
                    _ => None,
                })
            })
            .unwrap_or(mods);
        crate::canvas::tool_event(app, ToolEvent::Down { x: start[0], y: start[1], pressure: 1.0 }, press_mods);
    }
    let Some(c) = capture else { return false };
    let at = pos.map(|p| {
        let d = c.camera.document_delta(p - c.origin);
        [c.start[0] + f64::from(d.x), c.start[1] + f64::from(d.y)]
    });
    if released {
        let [x, y] = at.unwrap_or_else(|| app.drag.as_ref().and_then(|d| d.points.last()).map_or(c.start, |p| [p[0], p[1]]));
        crate::canvas::tool_event(app, ToolEvent::Up { x, y }, mods);
        ctx.data_mut(|d| d.remove::<Capture>(key()));
        app.move_fast = None;
    } else if down {
        if let Some([x, y]) = at.filter(|p| *p != c.start || app.drag.as_ref().is_some_and(|d| d.points.len() > 1)) {
            crate::canvas::tool_event(app, ToolEvent::Move { x, y, pressure: 1.0 }, mods);
        }
    } else {
        cancel(app, ctx);
    }
    true
}

#[cfg(test)]
mod tests;
