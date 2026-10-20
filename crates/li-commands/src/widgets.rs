//! Shared command rows using the caller's egui theme, spacing and monochrome icon painter.
//! Shells translate labels and format shortcuts before calling these functions.

use egui::{Color32, Rect, Response, Ui};

/// Reserve a themed icon gutter without changing the label's accessible name.
pub fn menu_row_label(ui: &Ui, label: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple_singleline(label.into(), egui::TextStyle::Button.resolve(ui.style()), ui.visuals().text_color());
    if let Some(section) = job.sections.first_mut() {
        section.leading_space = ui.spacing().icon_width + ui.spacing().icon_spacing;
    }
    job
}

pub fn menu_row_button<'a>(ui: &Ui, label: &str) -> egui::Button<'a> {
    egui::Button::new(menu_row_label(ui, label))
}

/// Keep checked state accessible; let the shell use its established shared icon painter.
pub fn menu_row_check(ui: &Ui, response: &Response, label: &str, checked: Option<bool>, paint_check: impl FnOnce(&Ui, Rect, f32, Color32)) {
    if let Some(on) = checked {
        response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, response.enabled(), on, label));
        if on {
            let width = ui.spacing().icon_width;
            let center = response.rect.left_center() + egui::vec2(ui.spacing().button_padding.x + width / 2.0, 0.0);
            let rect = Rect::from_center_size(center, egui::Vec2::splat(width));
            paint_check(ui, rect, width, ui.style().interact(response).fg_stroke.color);
        }
    }
}

/// Render one menu command. Shells can add shortcut text, a saved tint or truncation to the
/// shared button first. Keyboard navigation and invocation stay with the owning menu surface.
pub fn command_menu_item(
    ui: &mut Ui,
    button: egui::Button<'_>,
    label: &str,
    enabled: bool,
    checked: Option<bool>,
    paint_check: impl FnOnce(&Ui, Rect, f32, Color32),
) -> Response {
    let response = ui.add_enabled(enabled, button);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label));
    menu_row_check(ui, &response, label, checked, paint_check);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn shared_menu_row_clicks_and_disabled_accessible_checks() {
        for dark in [false, true] {
            let mut h = Harness::builder().build_ui_state(
                |ui, clicks: &mut usize| {
                    let button = menu_row_button(ui, "Available").shortcut_text("Ctrl+A");
                    if command_menu_item(ui, button, "Available", true, Some(true), |_, _, _, _| {}).clicked() {
                        *clicks += 1;
                    }
                    let button = menu_row_button(ui, "Unavailable");
                    if command_menu_item(ui, button, "Unavailable", false, Some(false), |_, _, _, _| {}).clicked() {
                        *clicks += 100;
                    }
                },
                0,
            );
            h.ctx.set_visuals(if dark { egui::Visuals::dark() } else { egui::Visuals::light() });
            h.run();
            h.get_by_label("Available").click();
            h.run();
            assert_eq!(*h.state(), 1);
            h.get_by_label("Unavailable").click();
            h.run();
            assert_eq!(*h.state(), 1);
        }
    }
}
