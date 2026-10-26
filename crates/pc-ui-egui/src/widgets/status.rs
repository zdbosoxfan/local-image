//! Compact labels and status icons shared by model setup, pickers and preview badges.

use super::*;

const CHIP_HEIGHT: f32 = 16.0;
const CHIP_FONT: f32 = 10.5;
const CHIP_PADDING_X: f32 = 5.0;
const CHIP_PADDING_Y: f32 = 1.0;
const STATUS_ICON_BOX: f32 = 16.0;
const STATUS_ICON_GLYPH: f32 = 12.0;

/// Semantic colour of a status, always drawn from the active theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Good,
    Warning,
    Danger,
}

impl Tone {
    pub fn ink(self, t: &Tokens) -> Color32 {
        match self {
            Self::Neutral => t.text_dim,
            Self::Good => t.accent,
            Self::Warning => t.warning,
            Self::Danger => t.danger,
        }
    }
}

/// An accessible status glyph. `label` includes the outcome, such as "Create passed".
pub fn status_icon(ui: &mut Ui, icon: &str, label: &str, tone: Tone) -> Response {
    let t = Tokens::get(ui.ctx());
    let height = ui.text_style_height(&egui::TextStyle::Body).max(STATUS_ICON_BOX);
    let (rect, response) = ui.allocate_exact_size(vec2(STATUS_ICON_BOX, height), Sense::hover());
    crate::icons::paint(ui, rect, icon, STATUS_ICON_GLYPH, tone.ink(&t));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), label));
    response
}

/// A compact status pill. Text is already translated; shape and words also convey the state.
pub fn status_chip(ui: &mut Ui, icon: &str, label: &str, tone: Tone) -> Response {
    let t = Tokens::get(ui.ctx());
    chip(ui, icon, label, t.field, tone.ink(&t))
}

/// A plain capability tag, using the same metrics and font as [`status_chip`].
pub fn tag_chip(ui: &mut Ui, label: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    chip(ui, "", label, t.field, Tone::Neutral.ink(&t))
}

/// A badge over image pixels: overlay roles keep it readable in either chrome theme.
pub fn overlay_chip(ui: &mut Ui, label: &str, tone: Tone) -> Response {
    let t = Tokens::get(ui.ctx());
    let ink = if tone == Tone::Neutral { t.library.overlay_fg } else { t.library.overlay_fg.lerp_to_gamma(tone.ink(&t), 0.3) };
    chip(ui, "", label, t.library.overlay_bg, ink)
}

fn chip(ui: &mut Ui, icon: &str, label: &str, fill: Color32, ink: Color32) -> Response {
    let icon_width = if icon.is_empty() { 0.0 } else { STATUS_ICON_BOX };
    let width = (ui.available_width() - 2.0 * CHIP_PADDING_X - icon_width).max(0.0);
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), theme::medium(CHIP_FONT), ink);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    let galley = ui.painter().layout_job(job);
    let elided = galley.elided;
    let height = CHIP_HEIGHT.max(galley.size().y + 2.0 * CHIP_PADDING_Y);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + icon_width + 2.0 * CHIP_PADDING_X, height), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, height / 2.0, fill);
        if !icon.is_empty() {
            let icon_rect = Rect::from_min_size(pos2(rect.left() + CHIP_PADDING_X, rect.top()), vec2(icon_width, height));
            crate::icons::paint(ui, icon_rect, icon, STATUS_ICON_GLYPH, ink);
        }
        ui.painter().galley(pos2(rect.left() + CHIP_PADDING_X + icon_width, rect.center().y - galley.size().y / 2.0), galley, ink);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), label));
    if elided { response.on_hover_text(label) } else { response }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn chips_share_metrics_and_expand_for_large_fonts_without_overflow() {
        let mut first = true;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(300.0, 180.0)).build_ui(move |ui| {
            if first {
                crate::PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Pro);
                first = false;
                return;
            }
            ui.horizontal(|ui| {
                status_chip(ui, "check", "Verified", Tone::Good);
                tag_chip(ui, "Create");
                overlay_chip(ui, "Installed", Tone::Good);
            });
        });
        h.run_steps(4);
        let small = h.get_by_label("Verified").rect().height();
        for label in ["Create", "Installed"] {
            assert!((h.get_by_label(label).rect().height() - small).abs() < 0.1);
        }
        li_ui_kit::fonts::set_ui_font_size(&h.ctx, li_ui_kit::fonts::FontSize::Large);
        h.run_steps(6);
        assert!(h.get_by_label("Verified").rect().height() > small);
        for label in ["Verified", "Create", "Installed"] {
            assert!(h.ctx.content_rect().contains_rect(h.get_by_label(label).rect()));
        }
    }
}
