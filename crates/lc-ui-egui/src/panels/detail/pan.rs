use super::{LightcraftApp, Rect};

pub(super) fn middle_pan(app: &mut LightcraftApp, ui: &egui::Ui, response: &egui::Response, canvas: Rect, image: Rect) -> bool {
    if !ui.is_enabled() {
        return false;
    }
    // Pending gradient tools also act on release; the pan owns that frame.
    if response.drag_stopped_by(egui::PointerButton::Middle) {
        return true;
    }
    if !response.dragged_by(egui::PointerButton::Middle) {
        return false;
    }
    let Some(origin) = ui.input(|input| input.pointer.press_origin()) else { return false };
    let navigator = app.widgets.iter().rev().find(|(name, _)| name == "canvas:navigator").map(|(_, rect)| *rect);
    if !response.rect.contains(origin)
        || ui.ctx().layer_id_at(origin) != Some(response.layer_id)
        || navigator.is_some_and(|rect| rect.expand(5.0).contains(origin))
    {
        return false;
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    if image.width() > canvas.width() + 1.0 || image.height() > canvas.height() + 1.0 {
        let delta = response.drag_delta();
        app.ui.pan.0 = (app.ui.pan.0 - delta.x / image.width()).clamp(0.0, 1.0);
        app.ui.pan.1 = (app.ui.pan.1 - delta.y / image.height()).clamp(0.0, 1.0);
    }
    true
}
