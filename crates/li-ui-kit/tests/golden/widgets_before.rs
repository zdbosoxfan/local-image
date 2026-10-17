//! Frozen earlier painter/control implementations; parity oracle only.
#![allow(dead_code, unused_imports)]
pub mod editor {
    use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
    #[allow(unused_imports)]
    use li_ui_kit::{fonts, theme};
    use theme::Tokens;
    pub fn surface(ui: &Ui, rect: Rect, fill: Color32, raised: bool) {
        let t = Tokens::get(ui.ctx());
        let p = ui.painter();
        p.rect_filled(rect, t.radius_sm, fill);
        if t.bevel {
            let (hi, lo) = if raised { (Color32::WHITE, Color32::from_gray(64)) } else { (Color32::from_gray(64), Color32::WHITE) };
            p.line_segment([rect.left_bottom(), rect.left_top()], Stroke::new(1.0, hi));
            p.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, hi));
            p.line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, lo));
            p.line_segment([rect.right_bottom(), rect.left_bottom()], Stroke::new(1.0, lo));
        }
    }
    pub fn pill_tab_translated(ui: &mut Ui, label: &str, selected: bool) -> Response {
        let t = Tokens::get(ui.ctx());
        let font = fonts::medium(12.5);
        let galley = ui.painter().layout_no_wrap(label.to_owned(), font, t.text);
        let size = vec2(galley.size().x + theme::PILL_TAB_PADDING, theme::PILL_TAB_HEIGHT);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        // the label is painted, not a widget: tell accessibility (and kittest) what the tab says
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, ui.is_enabled(), selected, label));
        if selected {
            surface(ui, rect, t.hover, true);
            if !t.bevel {
                ui.painter().rect_stroke(rect, t.radius_sm, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
            }
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, t.radius_sm, t.hover.gamma_multiply(0.6));
        }
        let color = if selected { t.text } else { t.text_dim };
        ui.painter().galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, color);
        resp
    }
    fn button_layout(ui: &Ui, label: &str, min_width: f32, fg: Color32) -> (std::sync::Arc<egui::Galley>, Vec2) {
        let galley = ui.painter().layout_no_wrap(label.to_owned(), fonts::medium(13.0), fg);
        let h = if Tokens::get(ui.ctx()).pro { 28.0 } else { 30.0 };
        let size = vec2((galley.size().x + 28.0).max(min_width), h);
        (galley, size)
    }
    fn button_impl(ui: &mut Ui, label: &str, min_width: f32, bg: Color32, fg: Color32, primary: bool) -> Response {
        let t = Tokens::get(ui.ctx());
        let (galley, size) = button_layout(ui, label, min_width, fg);
        let h = size.y;
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        // Painted text: name the button for accessibility (and so tests and agents can find it).
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label));
        if resp.has_focus() {
            // Keyboard focus (Tab); clicks don't focus egui buttons.
            let r = if t.pro { h / 2.0 } else { t.radius_sm } + 2.0;
            ui.painter().rect_stroke(rect.expand(2.0), r, Stroke::new(2.0, t.accent), StrokeKind::Outside);
        }
        if t.pro {
            // Spectrum buttons: fully rounded; primary = filled accent, secondary = outline.
            let down = resp.is_pointer_button_down_on();
            let r = h / 2.0;
            if primary {
                let fill = if down {
                    bg.gamma_multiply(0.8)
                } else if resp.hovered() {
                    bg.gamma_multiply(0.9)
                } else {
                    bg
                };
                ui.painter().rect_filled(rect, r, fill);
            } else {
                if resp.hovered() || down {
                    ui.painter().rect_filled(rect, r, t.hover);
                }
                ui.painter().rect_stroke(rect, r, Stroke::new(1.5, if resp.hovered() { t.text } else { t.text_dim }), StrokeKind::Inside);
            }
            ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
            return resp;
        }
        let fill = if resp.is_pointer_button_down_on() {
            bg.gamma_multiply(0.85)
        } else if resp.hovered() {
            if primary { bg.gamma_multiply(0.93) } else { t.hover }
        } else {
            bg
        };
        surface(ui, rect, fill, !resp.is_pointer_button_down_on());
        if !t.bevel && !primary {
            ui.painter().rect_stroke(rect, t.radius_sm, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
        }
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
        resp
    }
    pub fn primary_button(ui: &mut Ui, label: &str, min_width: f32) -> Response {
        let t = Tokens::get(ui.ctx());
        button_impl(ui, label, min_width, t.primary_bg, t.primary_text, true)
    }
    pub fn secondary_button(ui: &mut Ui, label: &str, min_width: f32) -> Response {
        let t = Tokens::get(ui.ctx());
        button_impl(ui, label, min_width, t.field, t.text, false)
    }
    pub fn checker(painter: &egui::Painter, rect: Rect, cell: f32) {
        painter.rect_filled(rect, 0.0, Color32::from_gray(250));
        let nx = (rect.width() / cell).ceil() as i32;
        let ny = (rect.height() / cell).ceil() as i32;
        for j in 0..ny {
            for i in 0..nx {
                if (i + j) % 2 == 1 {
                    let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * cell, rect.top() + j as f32 * cell), Vec2::splat(cell)).intersect(rect);
                    painter.rect_filled(r, 0.0, Color32::from_gray(214));
                }
            }
        }
    }
    pub fn color_tile(ui: &Ui, rect: Rect, color: Color32) {
        let t = Tokens::get(ui.ctx());
        let p = ui.painter();
        if color.a() < 255 {
            checker(&p.with_clip_rect(rect.intersect(p.clip_rect())), rect, (rect.height() / 3.0).clamp(3.0, 6.0));
        }
        p.rect_filled(rect, t.radius_sm, color);
        p.rect_stroke(rect, t.radius_sm, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
    }
    pub fn checkbox(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
        let t = Tokens::get(ui.ctx());
        let mut resp = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (rect, resp) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::click());
                let p = ui.painter();
                if *on {
                    p.rect_filled(rect, 2.0, t.accent);
                    let a = rect.left_center() + vec2(3.0, 0.5);
                    let b = rect.center_bottom() + vec2(-1.0, -3.5);
                    let c = rect.right_top() + vec2(-3.0, 3.5);
                    p.line_segment([a, b], Stroke::new(1.8, Color32::WHITE));
                    p.line_segment([b, c], Stroke::new(1.8, Color32::WHITE));
                } else {
                    p.rect_filled(rect, 2.0, t.field);
                    p.rect_stroke(rect, 2.0, Stroke::new(1.5, if resp.hovered() { t.text_dim } else { t.text_faint }), StrokeKind::Inside);
                }
                let l = ui.add(egui::Label::new(egui::RichText::new(label).color(t.text_dim)).sense(Sense::click()));
                resp.union(l)
            })
            .inner;
        if resp.clicked() {
            *on = !*on;
            resp.mark_changed();
        }
        resp
    }
    pub fn section_label_translated(ui: &mut Ui, text: &str) {
        let t = Tokens::get(ui.ctx());
        ui.label(egui::RichText::new(text).font(fonts::medium(11.5)).color(t.text_faint));
    }
    pub fn hairline(ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, t.separator));
    }
    pub fn vline(ui: &mut Ui, height: f32) {
        let t = Tokens::get(ui.ctx());
        let (r, _) = ui.allocate_exact_size(vec2(9.0, height), Sense::hover());
        ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0, t.separator));
    }
    pub fn chevron_icon(ui: &Ui, rect: Rect, visuals: &egui::style::WidgetVisuals, _open: bool) {
        let c = rect.center();
        let s = 3.2;
        let stroke = Stroke::new(1.3, visuals.fg_stroke.color.gamma_multiply(0.8));
        ui.painter().line_segment([c + vec2(-s, -s * 0.5), c + vec2(0.0, s * 0.5)], stroke);
        ui.painter().line_segment([c + vec2(0.0, s * 0.5), c + vec2(s, -s * 0.5)], stroke);
    }
}
pub mod library {
    use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
    #[allow(unused_imports)]
    use li_ui_kit::{fonts, theme};
    const SWATCH_RADIUS: f32 = 3.0;
    use li_ui_kit::library_icons::Icon;
    fn register(_: &egui::Context, _: impl Into<String>, _: Rect) {}
    fn paint(p: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
        let side = rect.width().min(rect.height());
        let ppp = p.ctx().pixels_per_point();
        let px = (side * ppp).round().clamp(1.0, 512.0) as u32;
        if let Some(tex) =
            li_ui_kit::icons::texture(p.ctx(), li_ui_kit::library_icons::svg(icon), li_ui_kit::icons::StrokeStyle::Interface, px, Color32::WHITE, false)
        {
            let r = Rect::from_center_size(rect.center(), Vec2::splat(px as f32 / ppp));
            p.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), color);
        }
    }
    fn text_button_size(label_width: f32) -> egui::Vec2 {
        vec2((label_width + 18.0).max(37.0), 24.0)
    }
    pub fn text_button(ui: &mut Ui, id: &str, label: &str, active: bool) -> Response {
        let t = li_ui_kit::library::get(ui.ctx());
        let text = if ui.is_enabled() { t.text } else { t.library.text_disabled };
        let font = fonts::semibold(11.5);
        let galley = ui.painter().layout_no_wrap(label.to_string(), font, text);
        let size = text_button_size(galley.size().x);
        let (r, resp) = ui.allocate_exact_size(size, Sense::click());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), active, label));
        register(ui.ctx(), format!("button:{id}"), r);
        let fill = if active {
            t.pressed
        } else if resp.hovered() || resp.has_focus() {
            t.hover
        } else {
            t.library.button
        };
        let p = ui.painter();
        p.rect(r, CornerRadius::same(4), fill, Stroke::new(1.0, t.library.button_border), StrokeKind::Inside);
        p.galley(r.center() - galley.size() / 2.0, galley, text);
        resp
    }
    pub fn segmented(ui: &mut Ui, id: &str, items: &[(&str, &str)], active: Option<usize>, per_row: usize) -> Option<usize> {
        let t = li_ui_kit::library::get(ui.ctx());
        let per_row = per_row.max(1);
        let gap = 4.0;
        let w = ui.available_width();
        let seg_w = ((w - gap * (per_row as f32 - 1.0)) / per_row as f32).max(24.0);
        let mut clicked = None;
        for (row_i, row) in items.chunks(per_row).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (j, (label, key)) in row.iter().enumerate() {
                    let i = row_i * per_row + j;
                    let (r, resp) = ui.allocate_exact_size(vec2(seg_w, 24.0), Sense::click());
                    register(ui.ctx(), format!("button:{id}-{key}"), r);
                    let fill = if active == Some(i) {
                        t.pressed
                    } else if resp.hovered() {
                        t.hover
                    } else {
                        t.library.button
                    };
                    let p = ui.painter();
                    p.rect(r, CornerRadius::same(4), fill, Stroke::new(1.0, t.library.button_border), StrokeKind::Inside);
                    p.text(r.center(), Align2::CENTER_CENTER, label, fonts::semibold(11.5), t.text);
                    if resp.clicked() {
                        clicked = Some(i);
                    }
                }
            });
        }
        clicked
    }
    pub fn divider(ui: &mut Ui) {
        let t = li_ui_kit::library::get(ui.ctx());
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, t.library.divider);
    }
    pub fn color_swatch(ui: &mut Ui, id: &str, color: Color32, size: Vec2, selected: Option<bool>, label: &str) -> Response {
        let t = li_ui_kit::library::get(ui.ctx());
        let (r, resp) = ui.allocate_exact_size(size, if selected.is_some() { Sense::click() } else { Sense::hover() });
        resp.widget_info(|| match selected {
            Some(on) => egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), on, label),
            None => egui::WidgetInfo::labeled(egui::WidgetType::Other, true, label),
        });
        register(ui.ctx(), id, r);
        let p = ui.painter();
        let tile = if selected.is_some() { r.shrink(2.0) } else { r };
        p.rect_filled(tile, SWATCH_RADIUS, color);
        p.rect_stroke(tile, SWATCH_RADIUS, Stroke::new(1.0, t.library.button_border), StrokeKind::Inside);
        match selected {
            Some(true) => {
                p.rect_stroke(r, SWATCH_RADIUS + 1.0, Stroke::new(1.5, t.text), StrokeKind::Inside);
            }
            Some(false) if resp.hovered() || resp.has_focus() => {
                p.rect_stroke(r, SWATCH_RADIUS + 1.0, Stroke::new(1.0, t.text_dim), StrokeKind::Inside);
            }
            _ => {}
        }
        resp
    }
    pub fn dropdown(ui: &mut Ui, id: &str, text: &str, font: egui::FontId, color: Color32) -> Response {
        let t = li_ui_kit::library::get(ui.ctx());
        let color = if ui.is_enabled() { color } else { t.library.text_disabled };
        let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
        job.wrap = egui::text::TextWrapping::truncate_at_width((ui.available_width() - li_ui_kit::library::DROPDOWN_ARROW_SPACE).max(0.0));
        let galley = ui.painter().layout_job(job);
        let size = vec2(
            (galley.size().x + li_ui_kit::library::DROPDOWN_ARROW_SPACE).min(ui.available_width()),
            galley.size().y.max(li_ui_kit::library::DROPDOWN_ARROW_SPACE),
        );
        let (r, resp) = ui.allocate_exact_size(size, Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), text));
        register(ui.ctx(), format!("dropdown:{id}"), r);
        let resp = if galley.elided { resp.on_hover_text(text) } else { resp };
        let c = if resp.hovered() || resp.has_focus() { t.text } else { color };
        ui.painter().galley(pos2(r.left(), r.center().y - galley.size().y / 2.0), galley, c);
        paint(ui.painter(), Rect::from_center_size(pos2(r.right() - 7.0, r.center().y + 1.0), vec2(12.0, 12.0)), Icon::ChevronDown, c);
        resp
    }
}
