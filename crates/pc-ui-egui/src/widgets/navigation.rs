//! Full-width navigation for master-detail windows.

use super::*;

const NAV_HEIGHT: f32 = 26.0;
const NAV_INSET: f32 = 8.0;
const NAV_ICON_BOX: f32 = 16.0;
const NAV_ICON_GAP: f32 = 6.0;

/// A neutral selected row with optional icon tint and trailing text. Text is already translated.
/// Labels elide with a tooltip, selection is exposed to AccessKit, and keyboard focus is visible.
pub fn nav_row(ui: &mut Ui, icon: &str, icon_tint: Option<Color32>, label: &str, trailing: &str, selected: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let ink = if selected { t.text } else { t.text_dim };
    let tail = ui.painter().layout_no_wrap(trailing.to_owned(), theme::medium(11.0), t.text_faint);
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), theme::medium(12.5), ink);
    let icon_width = if icon.is_empty() { 0.0 } else { NAV_ICON_BOX + NAV_ICON_GAP };
    let trailing_width = if trailing.is_empty() { 0.0 } else { tail.size().x + NAV_INSET };
    job.wrap = egui::text::TextWrapping::truncate_at_width((ui.available_width() - 2.0 * NAV_INSET - icon_width - trailing_width).max(0.0));
    let galley = ui.painter().layout_job(job);
    let elided = galley.elided;
    let height = NAV_HEIGHT.max(galley.size().y + 6.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if response.clicked() {
        response.request_focus();
    }
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, t.radius_sm, if selected { t.row_selected } else { t.hover });
    }
    if response.has_focus() {
        ui.painter().rect_stroke(rect.shrink(0.5), t.radius_sm, Stroke::new(1.0, t.accent_border), StrokeKind::Inside);
    }
    let x = rect.left() + NAV_INSET;
    if !icon.is_empty() {
        crate::icons::paint(
            ui,
            Rect::from_center_size(pos2(x + NAV_ICON_BOX / 2.0, rect.center().y), Vec2::splat(NAV_ICON_BOX)),
            icon,
            14.0,
            icon_tint.unwrap_or(ink),
        );
    }
    ui.painter().galley(pos2(x + icon_width, rect.center().y - galley.size().y / 2.0), galley, ink);
    if !trailing.is_empty() {
        let tail_ink = if selected || response.hovered() || response.has_focus() || !t.dark() { t.text_secondary_on_fill } else { t.text_faint };
        ui.painter().galley_with_override_text_color(pos2(rect.right() - NAV_INSET - tail.size().x, rect.center().y - tail.size().y / 2.0), tail, tail_ink);
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label));
    if elided { response.on_hover_text(label) } else { response }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::{NodeT, Queryable};

    #[test]
    fn clicking_a_navigation_row_takes_keyboard_focus() {
        let mut first = true;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(300.0, 140.0)).build_ui_state(
            move |ui, selected: &mut bool| {
                if first {
                    crate::PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Pro);
                    first = false;
                    return;
                }
                nav_row(ui, "", None, "First model", "", !*selected);
                if nav_row(ui, "", None, "Second model", "", *selected).clicked() {
                    *selected = true;
                }
            },
            false,
        );
        h.run_steps(3);
        h.get_by_label("First model").click();
        h.run_steps(3);
        assert!(h.get_by_label("First model").accesskit_node().is_focused());
        h.key_press(egui::Key::Tab);
        h.run_steps(3);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(*h.state(), "keyboard activation reaches the next row");
    }
}
