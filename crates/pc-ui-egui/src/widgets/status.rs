use super::*;

/// What a [`status_chip`] says about its subject, which picks its colour role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Good,
    Warning,
    Danger,
}

/// A small status label: a shape icon and text in the tone's colour on a field-coloured pill, so
/// the status reads by shape as well as colour. `label` is the accessible name. Used by Set Up
/// Models (each model's setup state) and the Generate model picker ("Not tested", "Re-check").
pub fn status_chip(ui: &mut Ui, icon: &str, label: &str, tone: Tone) -> Response {
    let t = Tokens::get(ui.ctx());
    let ink = match tone {
        Tone::Neutral => t.text_dim,
        Tone::Good => t.accent,
        Tone::Warning => t.warning,
        Tone::Danger => t.danger,
    };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), theme::medium(11.0), ink);
    let height = 18.0;
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + height + 8.0, height), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, height / 2.0, t.field);
        let icon_rect = Rect::from_min_size(rect.min + vec2(3.0, 0.0), Vec2::splat(height));
        crate::icons::paint(ui, icon_rect, icon, 12.0, ink);
        ui.painter().galley(pos2(icon_rect.right(), rect.center().y - galley.size().y / 2.0), galley, ink);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
    response
}

