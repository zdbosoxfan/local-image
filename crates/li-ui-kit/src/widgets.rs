//! Shared widget backend with the inherited Editor and Library visual profiles.
//! Callers translate labels and retain domain state; geometry, painting and interaction live here.
use crate::theme::{self, Tokens};
use crate::widget_style as m;
use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

/// Existing application profiles; a profile selects presentation, never persisted document data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Editor,
    Library,
}
impl Profile {
    pub fn tokens(self, ctx: &egui::Context) -> Tokens {
        match self {
            Self::Editor => Tokens::get(ctx),
            Self::Library => crate::library::get(ctx),
        }
    }
}

#[derive(Clone, Default)]
pub struct Registry(pub Vec<(String, Rect)>);
pub fn register(ctx: &egui::Context, id: impl Into<String>, rect: Rect) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Registry>(egui::Id::new("lc-registry")).0.push((id.into(), rect)));
}
pub fn take_registry(ctx: &egui::Context) -> Vec<(String, Rect)> {
    ctx.data_mut(|d| std::mem::take(&mut d.get_temp_mut_or_default::<Registry>(egui::Id::new("lc-registry")).0))
}
pub fn surface(ui: &Ui, rect: Rect, fill: Color32, raised: bool) {
    let t = Tokens::get(ui.ctx());
    let p = ui.painter();
    p.rect_filled(rect, t.radius_sm, fill);
    if t.bevel {
        let c = t.slider_colors();
        let (hi, lo) = if raised { (c.bevel_highlight, c.bevel_shadow) } else { (c.bevel_shadow, c.bevel_highlight) };
        p.line_segment([rect.left_bottom(), rect.left_top()], Stroke::new(m::STROKE, hi));
        p.line_segment([rect.left_top(), rect.right_top()], Stroke::new(m::STROKE, hi));
        p.line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(m::STROKE, lo));
        p.line_segment([rect.right_bottom(), rect.left_bottom()], Stroke::new(m::STROKE, lo));
    }
}

pub fn button_layout(ui: &Ui, label: &str, min_width: f32, fg: Color32) -> (std::sync::Arc<egui::Galley>, Vec2) {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), crate::fonts::medium(m::EDITOR_BUTTON_FONT), fg);
    let h = if Tokens::get(ui.ctx()).pro { m::PRO_BUTTON_HEIGHT } else { m::EDITOR_BUTTON_HEIGHT };
    let size = vec2((galley.size().x + m::EDITOR_BUTTON_PADDING).max(min_width), h);
    (galley, size)
}

fn editor_button(ui: &mut Ui, label: &str, min_width: f32, bg: Color32, fg: Color32, primary: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let (galley, size) = button_layout(ui, label, min_width, fg);
    let h = size.y;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    // Painted text: name the button for accessibility (and so tests and agents can find it).
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label));
    if resp.has_focus() {
        // Keyboard focus (Tab); clicks don't focus egui buttons.
        let r = if t.pro { h / 2.0 } else { t.radius_sm } + m::FOCUS_OUTSET;
        ui.painter().rect_stroke(rect.expand(m::FOCUS_OUTSET), r, Stroke::new(m::FOCUS_STROKE, t.accent), StrokeKind::Outside);
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
            ui.painter().rect_stroke(rect, r, Stroke::new(m::PRO_OUTLINE_STROKE, if resp.hovered() { t.text } else { t.text_dim }), StrokeKind::Inside);
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
        ui.painter().rect_stroke(rect, t.radius_sm, Stroke::new(m::STROKE, t.field_border), StrokeKind::Inside);
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    resp
}

pub fn primary_button(ui: &mut Ui, label: &str, min_width: f32) -> Response {
    let t = Tokens::get(ui.ctx());
    editor_button(ui, label, min_width, t.primary_bg, t.primary_text, true)
}

pub fn secondary_button(ui: &mut Ui, label: &str, min_width: f32) -> Response {
    let t = Tokens::get(ui.ctx());
    editor_button(ui, label, min_width, t.field, t.text, false)
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

pub fn chevron_icon(ui: &Ui, rect: Rect, visuals: &egui::style::WidgetVisuals, _open: bool) {
    let c = rect.center();
    let s = m::CHEVRON_HALF_WIDTH;
    let stroke = Stroke::new(m::CHEVRON_STROKE, visuals.fg_stroke.color.gamma_multiply(0.8));
    ui.painter().line_segment([c + vec2(-s, -s * 0.5), c + vec2(0.0, s * 0.5)], stroke);
    ui.painter().line_segment([c + vec2(0.0, s * 0.5), c + vec2(s, -s * 0.5)], stroke);
}

pub fn disabled_reason_tooltip(response: Response, reason: &str) -> Response {
    response.on_disabled_hover_ui(|ui| {
        let room = ui.ctx().content_rect().width() - ui.spacing().window_margin.sum().x;
        ui.set_max_width(ui.spacing().tooltip_width.min(room.max(ui.spacing().interact_size.x)));
        ui.add(egui::Label::new(reason).wrap());
    })
}

/// Labelled selection controls share interaction/accessibility while retaining visual grammar.
#[derive(Clone, Copy)]
pub enum SelectionStyle {
    Pill,
    Segment,
    Text,
}
pub fn selection(ui: &mut Ui, label: &str, selected: bool, style: SelectionStyle, size: Option<Vec2>) -> Response {
    let profile = if matches!(style, SelectionStyle::Pill) { Profile::Editor } else { Profile::Library };
    let t = profile.tokens(ui.ctx());
    let text = if profile == Profile::Library && !ui.is_enabled() { t.library.text_disabled } else { t.text };
    let font = if profile == Profile::Editor { crate::fonts::medium(theme::PILL_TAB_FONT_SIZE) } else { crate::fonts::semibold(m::LABEL_FONT) };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, text);
    let size = size.unwrap_or_else(|| match style {
        SelectionStyle::Pill => vec2(galley.size().x + theme::PILL_TAB_PADDING, theme::PILL_TAB_HEIGHT),
        _ => m::text_button_size(galley.size().x),
    });
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| {
        egui::WidgetInfo::selected(
            if matches!(style, SelectionStyle::Pill) { egui::WidgetType::SelectableLabel } else { egui::WidgetType::Button },
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let p = ui.painter();
    if matches!(style, SelectionStyle::Pill) {
        if selected {
            surface(ui, rect, t.hover, true);
            if !t.bevel {
                p.rect_stroke(rect, t.radius_sm, Stroke::new(m::STROKE, t.field_border), StrokeKind::Inside);
            }
        } else if resp.hovered() || resp.has_focus() {
            p.rect_filled(rect, t.radius_sm, t.hover.gamma_multiply(0.6));
        }
        p.galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, if selected { t.text } else { t.text_dim });
    } else {
        let fill = if selected {
            t.pressed
        } else if resp.hovered() || resp.has_focus() {
            t.hover
        } else {
            t.library.button
        };
        if resp.has_focus() {
            p.rect_stroke(rect.expand(m::FOCUS_OUTSET), m::LIBRARY_RADIUS + m::FOCUS_OUTSET, Stroke::new(m::FOCUS_STROKE, t.accent), StrokeKind::Outside);
        }
        p.rect(rect, m::LIBRARY_RADIUS, fill, Stroke::new(m::STROKE, t.library.button_border), StrokeKind::Inside);
        p.galley(rect.center() - galley.size() / 2.0, galley, text);
    }
    resp
}

/// Shared transparency-aware tile; the colour is image/document content.
pub fn color_tile(ui: &Ui, rect: Rect, color: Color32, profile: Profile) {
    let t = profile.tokens(ui.ctx());
    let p = ui.painter();
    if color.a() < 255 {
        checker(&p.with_clip_rect(rect.intersect(p.clip_rect())), rect, (rect.height() / 3.0).clamp(m::CHECKER_MIN_CELL, m::CHECKER_MAX_CELL));
    }
    let (radius, border) = match profile {
        Profile::Editor => (t.radius_sm, t.field_border),
        Profile::Library => (m::SWATCH_RADIUS, t.library.button_border),
    };
    p.rect_filled(rect, radius, color);
    p.rect_stroke(rect, radius, Stroke::new(m::STROKE, border), StrokeKind::Inside);
}

pub fn separator(ui: &mut Ui, profile: Profile, vertical_height: Option<f32>) {
    let t = profile.tokens(ui.ctx());
    let (r, _) =
        ui.allocate_exact_size(vertical_height.map_or_else(|| vec2(ui.available_width(), m::STROKE), |h| vec2(m::VERTICAL_SEPARATOR_WIDTH, h)), Sense::hover());
    if vertical_height.is_some() {
        ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(m::STROKE, t.separator));
    } else if profile == Profile::Library {
        ui.painter().rect_filled(r, 0.0, t.library.divider);
    } else {
        ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(m::STROKE, t.separator));
    }
}

/// Shared heading allocation/name; the Library facade adds its disclosure/enable glyphs.
pub fn heading(ui: &mut Ui, title: &str, profile: Profile, open: bool) -> Response {
    let t = profile.tokens(ui.ctx());
    if profile == Profile::Editor {
        let ink = ui.visuals().weak_text_color.unwrap_or(t.text_faint);
        return ui.label(egui::RichText::new(title).font(crate::fonts::medium(m::LABEL_FONT)).color(ink));
    }
    let (r, response) = ui.allocate_exact_size(vec2(ui.available_width(), m::HEADER_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, ui.is_enabled(), open, title));
    if response.hovered() {
        ui.painter().rect_filled(r, 0.0, t.chrome.gamma_multiply(1.06));
    }
    ui.painter().text(pos2(r.left() + m::HEADER_TEXT_INSET, r.center().y), Align2::LEFT_CENTER, title, crate::fonts::semibold(m::HEADER_FONT), t.text);
    response
}

/// Dropdown shell used by grouped choices. The popup uses egui's viewport fitting/focus.
pub fn choice_menu<R>(ui: &mut Ui, id: &str, selected: &str, width: f32, body: impl FnOnce(&mut Ui) -> R) {
    egui::ComboBox::from_id_salt(id).selected_text(selected).width(width).height(m::DROPDOWN_HEIGHT).icon(chevron_icon).show_ui(ui, body);
}

/// Borderless Library shell with elision and a caller-supplied sourced chevron.
pub fn choice_button(ui: &mut Ui, text: &str, font: egui::FontId, color: Color32, paint_arrow: impl FnOnce(&Ui, Rect, Color32)) -> Response {
    let t = Profile::Library.tokens(ui.ctx());
    let color = if ui.is_enabled() { color } else { t.library.text_disabled };
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width((ui.available_width() - crate::library::DROPDOWN_ARROW_SPACE).max(0.0));
    let galley = ui.painter().layout_job(job);
    let size =
        vec2((galley.size().x + crate::library::DROPDOWN_ARROW_SPACE).min(ui.available_width()), galley.size().y.max(crate::library::DROPDOWN_ARROW_SPACE));
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), text));
    let resp = if galley.elided { resp.on_hover_text(text) } else { resp };
    let c = if resp.hovered() || resp.has_focus() { t.text } else { color };
    ui.painter().galley(pos2(r.left(), r.center().y - galley.size().y / 2.0), galley, c);
    paint_arrow(ui, Rect::from_center_size(r.right_center() + m::DROPDOWN_ARROW_OFFSET, Vec2::splat(m::DROPDOWN_ARROW_SIZE)), c);
    resp
}

/// Icon button allocation/state/colour. The facade supplies its shared SVG/tool paint adapter.
#[allow(clippy::too_many_arguments)]
pub fn icon_button(ui: &mut Ui, size: Vec2, selected: bool, enabled: bool, label: &str, profile: Profile, paint: impl FnOnce(&Ui, Rect, Color32)) -> Response {
    let t = profile.tokens(ui.ctx());
    let enabled = enabled && ui.is_enabled();
    let (r, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label));
    let lit = (resp.hovered() || resp.has_focus()) && enabled;
    let color = match profile {
        Profile::Library => {
            let sq = Rect::from_center_size(r.center(), vec2(m::ICON_CHROME_MAX_SIZE.min(size.x), m::ICON_CHROME_MAX_SIZE.min(size.y)));
            if selected {
                ui.painter().rect_filled(sq, m::LIBRARY_RADIUS, t.library.tool_active);
            } else if lit {
                ui.painter().rect_filled(sq, m::LIBRARY_RADIUS, t.hover.gamma_multiply(0.7));
            }
            if !enabled {
                t.library.text_disabled
            } else if selected {
                t.library.pick
            } else if lit {
                t.text
            } else {
                t.icon
            }
        }
        Profile::Editor => {
            if lit {
                ui.painter().rect_filled(r, t.radius_sm, t.hover);
            }
            if !enabled {
                t.text_faint.gamma_multiply(0.6)
            } else if lit {
                t.text
            } else {
                t.icon
            }
        }
    };
    paint(ui, r, color);
    resp
}

/// Shared checkbox policy, with native Library and inherited Spectrum presentations.
pub fn checkbox(ui: &mut Ui, on: &mut bool, label: &str, profile: Profile) -> Response {
    if profile == Profile::Library {
        return ui.add(egui::Checkbox::new(on, label));
    }
    let t = profile.tokens(ui.ctx());
    let mut resp = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = m::CHECKBOX_GAP;
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(m::CHECKBOX_SIZE), Sense::click());
            let p = ui.painter();
            if *on {
                p.rect_filled(rect, m::CHECKBOX_RADIUS, t.accent);
                let a = rect.left_center() + m::CHECKBOX_CHECK_OFFSETS[0];
                let b = rect.center_bottom() + m::CHECKBOX_CHECK_OFFSETS[1];
                let c = rect.right_top() + m::CHECKBOX_CHECK_OFFSETS[2];
                p.line_segment([a, b], Stroke::new(m::CHECKBOX_CHECK_STROKE, t.slider_colors().bevel_highlight));
                p.line_segment([b, c], Stroke::new(m::CHECKBOX_CHECK_STROKE, t.slider_colors().bevel_highlight));
            } else {
                p.rect_filled(rect, m::CHECKBOX_RADIUS, t.field);
                p.rect_stroke(
                    rect,
                    m::CHECKBOX_RADIUS,
                    Stroke::new(m::CHECKBOX_OUTLINE, if resp.hovered() || resp.has_focus() { t.text_dim } else { t.text_faint }),
                    StrokeKind::Inside,
                );
            }
            // The text belongs to the checkbox, so expose one accessible control for the
            // complete hit target. Adding a Label widget here would also announce a separate
            // node with the same name (and make automation by label ambiguous).
            let (text_pos, galley, l) =
                egui::Label::new(egui::RichText::new(label).color(t.text_dim)).sense(Sense::click() - Sense::FOCUSABLE).layout_in_ui(ui);
            if ui.is_rect_visible(l.rect) {
                let response_color = ui.style().interact(&l).text_color();
                let underline = if l.has_focus() || l.highlighted() { Stroke::new(1.0, response_color) } else { Stroke::NONE };
                ui.painter().add(egui::epaint::TextShape::new(text_pos, galley, response_color).with_underline(underline));
            }
            resp.union(l)
        })
        .inner;
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, label));
    resp
}

/// Shared Library segment layout; translation stays in the frontend without a temporary Vec.
pub fn segmented(ui: &mut Ui, id: &str, items: &[(&str, &str)], active: Option<usize>, per_row: usize, translate: impl Fn(&str) -> &str) -> Option<usize> {
    let per_row = per_row.max(1);
    let seg_w = ((ui.available_width() - m::SEGMENT_GAP * (per_row as f32 - 1.0)) / per_row as f32).max(m::SEGMENT_MIN_WIDTH);
    let mut clicked = None;
    for (row_i, row) in items.chunks(per_row).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = m::SEGMENT_GAP;
            for (j, (label, key)) in row.iter().enumerate() {
                let i = row_i * per_row + j;
                let response = selection(ui, translate(label), active == Some(i), SelectionStyle::Segment, Some(vec2(seg_w, theme::PILL_TAB_HEIGHT)));
                register(ui.ctx(), format!("button:{id}-{key}"), response.rect);
                if response.clicked() {
                    clicked = Some(i);
                }
            }
        });
    }
    clicked
}

/// Selection outline and accessibility around the common transparency-aware tile.
pub fn color_swatch(ui: &mut Ui, id: &str, color: Color32, size: Vec2, selected: Option<bool>, label: &str) -> Response {
    let t = Profile::Library.tokens(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(size, if selected.is_some() { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| match selected {
        Some(on) => egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), on, label),
        None => egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), label),
    });
    register(ui.ctx(), id, r);
    color_tile(ui, if selected.is_some() { r.shrink(m::FOCUS_OUTSET) } else { r }, color, Profile::Library);
    match selected {
        Some(true) => {
            ui.painter().rect_stroke(r, m::LIBRARY_RADIUS, Stroke::new(m::PRO_OUTLINE_STROKE, t.text), StrokeKind::Inside);
        }
        Some(false) if resp.hovered() || resp.has_focus() => {
            ui.painter().rect_stroke(r, m::LIBRARY_RADIUS, Stroke::new(m::STROKE, t.text_dim), StrokeKind::Inside);
        }
        _ => {}
    }
    resp
}

#[derive(Clone, Copy)]
pub enum HeaderGlyph {
    Open,
    Closed,
    Eye,
}
/// Library header's reusable disclosure/enable controls; facade supplies sourced icon paint.
pub fn library_header(
    ui: &mut Ui,
    id: &str,
    title: &str,
    open: bool,
    enabled: Option<bool>,
    paint: impl Fn(&Ui, Rect, HeaderGlyph, Color32),
) -> (Response, Option<bool>) {
    let t = Profile::Library.tokens(ui.ctx());
    let resp = heading(ui, title, Profile::Library, open);
    let r = resp.rect;
    register(ui.ctx(), format!("section:{id}"), r);
    let chev = Rect::from_center_size(pos2(r.left() + m::HEADER_CHEVRON_INSET, r.center().y), Vec2::splat(m::HEADER_CHEVRON_SIZE));
    paint(ui, chev, if open { HeaderGlyph::Open } else { HeaderGlyph::Closed }, t.library.text_label);
    let mut toggled = None;
    if let Some(on) = enabled
        && (resp.hovered() || resp.has_focus() || !on)
    {
        let eye = Rect::from_center_size(pos2(r.right() - m::HEADER_EYE_INSET, r.center().y), Vec2::splat(m::HEADER_EYE_SIZE));
        let er = ui.interact(eye, ui.id().with(("eye", id)), Sense::click());
        er.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), on, title));
        register(ui.ctx(), format!("sectionEye:{id}"), eye);
        paint(ui, eye, HeaderGlyph::Eye, if on { t.icon } else { t.library.text_disabled });
        if !on {
            ui.painter().line_segment([eye.left_bottom(), eye.right_top()], Stroke::new(m::PRO_OUTLINE_STROKE, t.library.text_disabled));
        }
        if er.clicked() {
            toggled = Some(!on);
        }
    }
    (resp, toggled)
}
