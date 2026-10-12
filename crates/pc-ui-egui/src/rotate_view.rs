//! View-only rotation. Each capture is stored in its own document/window camera, never history.
use super::ViewXform;
use crate::{
    PhotocraftApp,
    state::{Tool, View},
};
use egui::{Key, Modifiers, PointerButton, Pos2, Response, Vec2, vec2};
use photocraft_doc::DocId;
use serde_json::{Value, json};

/// A transient drag must still belong to the same document and viewport at every update.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    document: DocId,
    viewport: egui::ViewportId,
    initial: f32,
    last: f32,
    total: f32,
    start: Pos2,
    radial: bool,
}

/// Fit a rotated document extent without silently resetting its angle.
pub fn fit_zoom(size: Vec2, available: Vec2, rotation: f32) -> f32 {
    let (sin, cos) = crate::view_camera::normalized_angle(rotation).sin_cos();
    let extent = vec2(cos.abs() * size.x + sin.abs() * size.y, sin.abs() * size.x + cos.abs() * size.y);
    (available.x / extent.x.max(1.0)).min(available.y / extent.y.max(1.0))
}

pub fn pan(view: &mut View, xf: &ViewXform, delta: Vec2) {
    let d = xf.camera().document_delta(delta);
    view.center[0] -= d.x;
    view.center[1] -= d.y;
}

/// Claim navigation before vector, move, painting or raw-pointer handlers see this gesture.
/// Read all Context/Response state before mutating the capture; no nested Context locks.
pub fn input(ctx: &egui::Context, response: &Response, view: &mut View, document: DocId, tool: Tool, enabled: bool) -> bool {
    let viewport = ctx.viewport_id();
    let (focused, space, middle, down, pressed, origin) = ctx.input(|i| {
        (i.focused, i.key_down(Key::Space), i.pointer.middle_down(), i.pointer.primary_down(), i.pointer.primary_pressed(), i.pointer.press_origin())
    });
    let pointer = response.interact_pointer_pos();
    // Capture the press itself; a later egui drag threshold must not restart a cancelled
    // press or transfer a held pointer into another document.
    let started = pressed && response.is_pointer_button_down_on();
    let stopped = response.drag_stopped_by(PointerButton::Primary);
    let allowed =
        enabled && tool == Tool::RotateView && focused && !space && !middle && !egui::Popup::is_any_open(ctx) && ctx.memory(|m| m.top_modal_layer().is_none());
    if !allowed || view.rotation_drag.as_ref().is_some_and(|d| d.document != document || d.viewport != viewport) {
        view.rotation_drag = None;
    }
    if !allowed {
        return false;
    }
    if response.hovered() || view.rotation_drag.is_some() {
        ctx.set_cursor_icon(if view.rotation_drag.is_some() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
    }
    if view.rotation_drag.is_some() && crate::shortcuts::canvas_keyboard_available(ctx) && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        if let Some(drag) = view.rotation_drag.take() {
            view.rotation = drag.initial;
        }
        ctx.request_repaint();
        return true;
    }
    if started
        && view.rotation_drag.is_none()
        && let Some(start) = origin.filter(|p| response.rect.contains(*p)).or(pointer)
    {
        let d = start - response.rect.center();
        view.rotation_drag = Some(Drag { document, viewport, initial: view.rotation, last: d.y.atan2(d.x), total: 0.0, start, radial: d.length_sq() >= 64.0 });
        view.fit_pending = false;
    }
    if !down && !stopped {
        view.rotation_drag = None;
    }
    if let Some(drag) = &mut view.rotation_drag {
        if let Some(pointer) = pointer {
            if drag.radial {
                let d = pointer - response.rect.center();
                if d.length_sq() >= 1.0 {
                    let angle = d.y.atan2(d.x);
                    drag.total += crate::view_camera::normalized_angle(angle - drag.last);
                    drag.last = angle;
                }
            } else {
                drag.total = (pointer.x - drag.start.x) / 150.0;
            }
            view.rotation = crate::view_camera::normalized_angle(drag.initial + drag.total);
            ctx.request_repaint();
        }
        if stopped || !down {
            view.rotation_drag = None;
        }
    }
    true
}

fn set_view_angle(view: &mut View, degrees: f64) -> Result<Value, String> {
    if !degrees.is_finite() || degrees.abs() > 1e6 {
        return Err(tl!("View angle must be a finite number of degrees.").into());
    }
    view.rotation = crate::view_camera::normalized_angle(degrees.to_radians() as f32);
    view.rotation_drag = None;
    view.fit_pending = false;
    Ok(json!({"degrees":view.rotation.to_degrees()}))
}

pub fn set_angle(app: &mut PhotocraftApp, degrees: f64) -> Result<Value, String> {
    let index = app.session.active_index().ok_or("no document")?;
    let view = app.ui.views.get_mut(index).ok_or("no document view")?;
    set_view_angle(view, degrees)
}

/// Run before global job/AI Escape handlers in the main viewport.
pub fn handle_keys(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let document = app.session.active().map(|s| s.doc.id);
    let viewport = ctx.viewport_id();
    let focused = ctx.input(|i| i.focused);
    let blocked =
        crate::restoration_ui::is_open(app) || !app.ui.dialogs.is_empty() || egui::Popup::is_any_open(ctx) || ctx.memory(|m| m.top_modal_layer().is_some());
    for view in &mut app.ui.views {
        if view
            .rotation_drag
            .as_ref()
            .is_some_and(|d| d.viewport == viewport && (Some(d.document) != document || app.ui.tool != Tool::RotateView || !focused || blocked))
        {
            view.rotation_drag = None;
        }
    }
    let Some(index) = app.session.active_index() else { return };
    let owns = app.ui.views.get(index).and_then(|v| v.rotation_drag.as_ref()).is_some_and(|d| Some(d.document) == document && d.viewport == viewport);
    if owns && crate::shortcuts::canvas_keyboard_available(ctx) && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        if let Some(drag) = app.ui.views[index].rotation_drag.take() {
            app.ui.views[index].rotation = drag.initial;
        }
        ctx.request_repaint();
    }
}

/// Shared toolbar hook; hidden panels do not alter any camera or capture state.
pub fn options(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(index) = app.session.active_index() else { return };
    if let Some(view) = app.ui.views.get_mut(index) {
        options_for_view(view, ui);
    }
}

/// Extra document windows edit their own camera, never the main tab's View.
pub fn options_for_view(view: &mut View, ui: &mut egui::Ui) {
    let mut degrees = view.rotation.to_degrees();
    let label = ui.label(tl!("View angle"));
    if ui.add(egui::DragValue::new(&mut degrees).range(-180.0..=180.0).speed(1.0).suffix("°")).labelled_by(label.id).changed() {
        let _ = set_view_angle(view, f64::from(degrees));
    }
    if ui.button(tl!("Reset View")).clicked() {
        let _ = set_view_angle(view, 0.0);
    }
}

/// Resize feedback follows the same rotated/reflected axes as its document-space handle.
pub fn resize_cursor(icon: egui::CursorIcon, xf: &ViewXform) -> egui::CursorIcon {
    use egui::CursorIcon::{ResizeHorizontal, ResizeNeSw, ResizeNwSe, ResizeVertical};
    let direction = match icon {
        ResizeHorizontal => vec2(1.0, 0.0),
        ResizeVertical => vec2(0.0, 1.0),
        ResizeNwSe => vec2(1.0, 1.0),
        ResizeNeSw => vec2(1.0, -1.0),
        _ => return icon,
    };
    let d = xf.camera().screen_delta(direction);
    match ((d.y.atan2(d.x) / std::f32::consts::FRAC_PI_4).round() as i32).rem_euclid(4) {
        0 => ResizeHorizontal,
        1 => ResizeNwSe,
        2 => ResizeVertical,
        _ => ResizeNeSw,
    }
}

/// Rotated CPU fallback shadow, evaluated in document coordinates with screen-down light.
/// The mesh is bounded by the viewport and capped independently of document pixel size.
pub fn cpu_shadow(painter: &egui::Painter, xf: &ViewXform, document: egui::Rect) {
    let camera = xf.camera();
    let rect = camera.screen_bounds(document).expand(64.0).intersect(painter.clip_rect());
    if !rect.is_positive() {
        return;
    }
    let (cols, rows) = (((rect.width() / 4.0).ceil() as usize).clamp(1, 256), ((rect.height() / 4.0).ceil() as usize).clamp(1, 256));
    let erf = |x: f32| {
        let a = x.abs();
        let t = 1.0 + (0.278393 + (0.230389 + 0.078108 * a * a) * a) * a;
        x.signum() * (1.0 - 1.0 / t.powi(4))
    };
    let blur = |point: Pos2, sigma: f32| {
        let d = camera.to_document(point);
        let k = std::f32::consts::FRAC_1_SQRT_2 * xf.zoom / sigma;
        let x = (erf((d.x - document.left()) * k) - erf((d.x - document.right()) * k)) * 0.5;
        let y = (erf((d.y - document.top()) * k) - erf((d.y - document.bottom()) * k)) * 0.5;
        x * y
    };
    let mut mesh = egui::Mesh::default();
    for y in 0..=rows {
        for x in 0..=cols {
            let p = rect.min + rect.size() * vec2(x as f32 / cols as f32, y as f32 / rows as f32);
            let a = 0.40 * blur(p - vec2(0.0, 7.0), 16.0);
            let b = 0.30 * blur(p - vec2(0.0, 0.75), 1.25);
            let alpha = ((1.0 - (1.0 - a) * (1.0 - b)).clamp(0.0, 1.0) * 255.0).round() as u8;
            mesh.colored_vertex(p, egui::Color32::from_black_alpha(alpha));
        }
    }
    for y in 0..rows {
        for x in 0..cols {
            let i = (y * (cols + 1) + x) as u32;
            mesh.add_triangle(i, i + 1, i + cols as u32 + 2);
            mesh.add_triangle(i, i + cols as u32 + 2, i + cols as u32 + 1);
        }
    }
    painter.add(mesh);
}

#[cfg(test)]
#[path = "rotate_view/tests.rs"]
mod tests;
