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
        ui.painter().galley(pos2(rect.right() - NAV_INSET - tail.size().x, rect.center().y - tail.size().y / 2.0), tail, t.text_faint);
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label));
    if elided { response.on_hover_text(label) } else { response }
}
