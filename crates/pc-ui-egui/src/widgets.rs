//! Custom widgets for the Photocraft look: cards with pill tabs, thin sliders with round knobs,
//! monospace value fields with dimmed units, toggle switches, primary/secondary buttons.

use egui::{Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::theme::{self, Tokens};
use li_commands::widgets::menu_row_button;
pub use li_ui_kit::widgets::{checker, chevron_icon, disabled_reason_tooltip, surface};

/// An accent insertion line on one edge of `r` while a drag hovers it (vertical: on its left or,
/// `after`, right edge; else on its top or bottom).
pub fn drop_line(ui: &Ui, r: Rect, after: bool, vertical: bool, t: &Tokens) {
    let s = Stroke::new(2.0, t.accent);
    if vertical {
        let x = if after { r.right() } else { r.left() };
        ui.painter().line_segment([pos2(x, r.top() + 2.0), pos2(x, r.bottom() - 2.0)], s);
    } else {
        let y = if after { r.bottom() } else { r.top() };
        ui.painter().line_segment([pos2(r.left() + 4.0, y), pos2(r.right() - 4.0, y)], s);
    }
}

/// A dock card: rounded container with a header of pill tabs and optional trailing actions.
/// Returns the index of the selected tab.
pub fn card(ui: &mut Ui, id: &str, tabs: &[&str], selected: &mut usize, body: impl FnOnce(&mut Ui, usize)) {
    let _ = card_ex(ui, id, tabs, selected, false, body);
}

/// What happened on a card's tab strip this frame (see [`card_ex`]).
pub struct CardResponse {
    /// The tab strip background: drag to move the group, double-click to collapse it.
    pub strip: Response,
    /// The panel menu button (hamburger in Pro, ellipsis in Studio).
    pub menu: Response,
    /// A tab was double-clicked (Photoshop collapses the group).
    pub tab_double_clicked: bool,
    /// Rects of the tabs on the strip, `(tab index, rect)`; tabs that don't fit are in the
    /// chevron menu instead (#151).
    pub tabs: Vec<(usize, Rect)>,
    /// The » overflow button, when some tabs didn't fit.
    pub chevron: Option<Rect>,
    /// A tab started being dragged this frame (the dock tears it off).
    pub tab_drag_started: Option<usize>,
}

/// [`card`] that can be collapsed to its tab strip and reports strip and menu interactions.
pub fn card_ex(ui: &mut Ui, id: &str, tabs: &[&str], selected: &mut usize, collapsed: bool, body: impl FnOnce(&mut Ui, usize)) -> CardResponse {
    let t = Tokens::get(ui.ctx());
    if t.pro {
        return pro_panel(ui, id, tabs, selected, collapsed, body);
    }
    let frame = egui::Frame::NONE
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.card_border))
        .corner_radius(CornerRadius::same(t.radius as u8))
        .inner_margin(egui::Margin { left: 8, right: 8, top: 6, bottom: if collapsed { 6 } else { 10 } });
    let out = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Registered before the tabs so they keep their clicks; drags fall through to it.
            let strip_rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 22.0));
            let strip = ui.interact(strip_rect, ui.id().with((id, "strip")), Sense::click_and_drag());
            // Pill tabs left of the menu button; they elide or overflow into a chevron (#151).
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
            let tip = li_i18n::fmt(tl!("{name} options"), &[("name", tl!(tabs.get(*selected).copied().unwrap_or(id)))]);
            let menu_rect = Rect::from_min_max(pos2(row.right() - 22.0, row.top() + 1.0), pos2(row.right(), row.bottom() - 1.0));
            let menu = crate::icons::button(&mut ui.new_child(egui::UiBuilder::new().max_rect(menu_rect)), "ellipsis", 22.0, false, &tip);
            let area = Rect::from_min_max(row.min, pos2((menu_rect.left() - 4.0).max(row.left()), row.bottom()));
            let tabs_out = crate::tab_strip::pill_tabs(ui, ui.id().with((id, "tabs")), area, tabs, selected);
            if !collapsed {
                ui.add_space(6.0);
                body(ui, *selected);
            }
            CardResponse {
                strip,
                menu,
                tab_double_clicked: tabs_out.double_clicked,
                tabs: tabs_out.tabs,
                chevron: tabs_out.chevron,
                tab_drag_started: tabs_out.drag_started,
            }
        })
        .inner;
    ui.add_space(6.0);
    out
}

/// Photoshop-grammar panel group: dark tab strip with flat tabs, flat body, hamburger menu.
fn pro_panel(ui: &mut Ui, id: &str, tabs: &[&str], selected: &mut usize, collapsed: bool, body: impl FnOnce(&mut Ui, usize)) -> CardResponse {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width();
    // Tab strip. Its background senses drags (move the group) and double-clicks (collapse);
    // the tabs, registered after it, keep their clicks.
    let (strip, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
    let strip_resp = ui.interact(strip, ui.id().with((id, "strip")), Sense::click_and_drag());
    let rounding = if collapsed { CornerRadius::same(3) } else { CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 } };
    ui.painter().rect_filled(strip, rounding, t.tab_strip);
    // Panel menu (hamburger); the tabs stay left of it, eliding or overflowing (#151).
    let menu = Rect::from_center_size(pos2(strip.right() - 14.0, strip.center().y), vec2(20.0, 18.0));
    let tabs_out = crate::tab_strip::pro_tabs(ui, ui.id().with((id, "tabs")), strip, menu.left(), tabs, selected, collapsed);
    let mresp = ui.interact(menu, ui.id().with((id, "menu")), Sense::click());
    let c = if mresp.hovered() { t.text } else { t.text_faint };
    for k in 0..3 {
        let y = menu.center().y - 3.5 + k as f32 * 3.5;
        ui.painter().line_segment([pos2(menu.center().x - 5.0, y), pos2(menu.center().x + 5.0, y)], Stroke::new(1.0, c));
    }
    // Body.
    if !collapsed {
        egui::Frame::NONE
            .fill(t.card)
            .corner_radius(CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 })
            .inner_margin(egui::Margin { left: 8, right: 8, top: 8, bottom: 8 })
            .show(ui, |ui| {
                ui.set_width(width - 16.0);
                body(ui, *selected);
            });
    }
    ui.add_space(2.0);
    CardResponse {
        strip: strip_resp,
        menu: mresp,
        tab_double_clicked: tabs_out.double_clicked,
        tabs: tabs_out.tabs,
        chevron: tabs_out.chevron,
        tab_drag_started: tabs_out.drag_started,
    }
}

pub fn pill_tab(ui: &mut Ui, label: &str, selected: bool) -> Response {
    pill_tab_translated(ui, tl!(label), selected)
}

/// [`pill_tab`] for a label already in the UI language (see [`slider_row_translated`]).
pub fn pill_tab_translated(ui: &mut Ui, label: &str, selected: bool) -> Response {
    li_ui_kit::widgets::selection(ui, label, selected, li_ui_kit::widgets::SelectionStyle::Pill, None)
}

/// Monospace numeric field with a dimmed unit suffix, Photoshop style. Drag to scrub.
pub fn value_field(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str, width: f32) -> Response {
    let decimals = if range.end() - range.start() <= 10.0 { 2 } else { 1 };
    value_field_with_precision(ui, value, range, suffix, width, decimals)
}

/// A [`value_field`] showing up to `decimals` decimal places (trailing zeros dropped) instead of
/// the one or two it picks from the range: a crop ratio of 1.414 : 1 must not read 1.4.
pub fn value_field_prec(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str, width: f32, decimals: u32) -> Response {
    value_field_with_precision(ui, value, range, suffix, width, decimals as usize)
}

/// A value field whose precision is independent of the backend's range (e.g. ComfyUI guidance).
pub fn value_field_with_precision<T: egui::emath::Numeric>(
    ui: &mut Ui,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    suffix: &str,
    width: f32,
    decimals: usize,
) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
    surface(ui, rect, t.field, false);
    if !t.bevel {
        ui.painter().rect_stroke(rect, t.radius_sm, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
    }
    let suffix_w = if suffix.is_empty() { 0.0 } else { 16.0 };
    let field = Rect::from_min_max(rect.min + vec2(4.0, 2.0), rect.max - vec2(4.0 + suffix_w, 2.0));
    // Small ranges (gamma 0.01–9.99, 0–1 centres) need two decimals and a finer drag, like Photoshop.
    // new_child (not scope_builder): a scope would move the parent cursor back to the child rect.
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
    let resp = {
        let ui = &mut child;
        {
            ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
            ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            ui.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
            ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;
            ui.style_mut().visuals.widgets.hovered.weak_bg_fill = Color32::TRANSPARENT;
            ui.style_mut().override_font_id = Some(theme::mono(12.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());
                ui.allocate_ui_with_layout(field.size(), layout, |ui| number_edit(ui, value, range, decimals)).inner
            })
            .inner
        }
    };
    if !suffix.is_empty() {
        ui.painter().text(pos2(rect.right() - 6.0, rect.center().y), Align2::RIGHT_CENTER, suffix, theme::mono(11.0), t.text_faint);
    }
    resp
}

/// Longest text, in characters, a numeric field takes in one input event or holds while editing.
const NUMERIC_INPUT_LIMIT: usize = 128;

/// The number in a [`value_field`]. A typed number applies as it's typed; arithmetic waits for
/// Enter, Tab or click-away, because per keystroke `5/2` would land first and a caller that
/// rounds it would cut `5/2*2` short. `changed()` means a new value, not just a keystroke.
fn number_edit<T: egui::emath::Numeric>(ui: &mut Ui, value: &mut T, range: std::ops::RangeInclusive<T>, decimals: usize) -> Response {
    let (id, ctx) = (ui.next_auto_id(), ui.ctx().clone());
    // Mirror DragValue's focus check before it builds its TextEdit: an oversized event must never
    // reach text layout. Registering interest also covers focus arriving by Tab this frame.
    let editing = ui.is_enabled()
        && ui.memory_mut(|m| {
            m.interested_in_focus(id, ui.layer_id());
            m.has_focus(id)
        });
    if editing {
        ui.input_mut(|input| {
            input.events.retain(|event| {
                let text = match event {
                    egui::Event::Text(text) | egui::Event::Paste(text) | egui::Event::Ime(egui::ImeEvent::Commit(text)) => text,
                    egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => text,
                    _ => return true,
                };
                text.chars().nth(NUMERIC_INPUT_LIMIT).is_none()
            });
        });
    }
    let held = id.with("arithmetic");
    let math = ui.memory(|m| m.has_focus(id)) && ui.data(|d| d.get_temp(held)).unwrap_or(false);
    ui.data_mut(|d| d.insert_temp(held, math));
    let before = *value;
    let mut resp = ui.add(
        egui::DragValue::new(value)
            .range(range.start().to_f64()..=range.end().to_f64())
            // Only edits are clamped: showing a value past the range, such as a type layer scaled
            // to 2400 pt in a 1296 pt field, must not change it.
            .clamp_existing_to_range(false)
            .speed(if decimals >= 2 { 0.01 } else { 0.5 })
            .custom_formatter(move |v, _| match decimals {
                0 => format!("{v:.0}"),
                1 => fmt_num(v),
                2 => fmt_num2(v),
                _ => fmt_num_prec(v, decimals as u32),
            })
            .update_while_editing(!math)
            // Focus is read when parsing, not above: Tab hands it on inside `ui.add`.
            .custom_parser(move |s| {
                // Short events can accumulate: reject before the parser normalizes the text.
                if s.chars().nth(NUMERIC_INPUT_LIMIT).is_some() {
                    return None;
                }
                let v = parse_num(s);
                if ctx.memory(|m| m.has_focus(id)) && plain(s).is_none() {
                    // Still typing arithmetic: hold it, and stop applying keystrokes once it parses.
                    ctx.data_mut(|d| d.insert_temp(held, v.is_some()));
                    return None;
                }
                v
            }),
    );
    resp.flags.set(egui::response::Flags::CHANGED, *value != before);
    resp
}

/// Thin-track slider with a round knob. `gradient` paints the track (e.g. hue spectrum).
pub fn slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, gradient: Option<&[Color32]>) -> Response {
    li_ui_kit::slider::editor(ui, value, li_ui_kit::slider::Scale { min: f64::from(*range.start()), max: f64::from(*range.end()), step: None }, gradient)
}

/// Labelled slider row: `Label ........ [value field]` above a full-width thin slider.
pub fn slider_row(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str, gradient: Option<&[Color32]>) -> Response {
    slider_row_translated(ui, tl!(label), value, range, suffix, gradient)
}

/// [`slider_row`] for a label that is already in the UI language (e.g. a contextual translation): it is shown as is,
/// never looked up again, so a translation that happens to equal another English message keeps its own text.
pub fn slider_row_translated(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    gradient: Option<&[Color32]>,
) -> Response {
    let t = Tokens::get(ui.ctx());
    let mut changed_resp = None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let field = value_field(ui, value, range.clone(), suffix, 74.0);
            field.widget_info(|| {
                let mut info = egui::WidgetInfo::drag_value(ui.is_enabled(), f64::from(*value));
                info.label = Some(li_i18n::fmt(tl!("{name} value"), &[("name", label)]));
                info
            });
            changed_resp = Some(field);
        });
    });
    let s = slider(ui, value, range, gradient);
    let accessible = li_i18n::fmt(tl!("{name} slider"), &[("name", label)]);
    s.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), f64::from(*value), &accessible));
    let mut r = s.clone();
    if let Some(v) = changed_resp
        && v.changed()
    {
        r.mark_changed();
    }
    ui.add_space(4.0);
    r
}

/// iOS-style toggle switch.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    if t.pro {
        return checkbox(ui, on, label);
    }
    let mut resp = ui
        .horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 17.0), Sense::click());
            let how_on = ui.ctx().animate_bool(resp.id, *on);
            let bg = if *on { t.accent } else { t.field_border };
            if t.bevel {
                surface(ui, rect, if *on { t.accent } else { t.field }, false);
            } else {
                ui.painter().rect_filled(rect, 8.5, bg);
            }
            let x = egui::lerp((rect.left() + 8.5)..=(rect.right() - 8.5), how_on);
            ui.painter().circle_filled(pos2(x, rect.center().y), 6.5, Color32::WHITE);
            ui.label(egui::RichText::new(tl!(label)).color(if *on { t.text } else { t.text_dim }));
            resp
        })
        .inner;
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp
}

/// Big white primary button.
pub fn primary_button(ui: &mut Ui, label: &str, min_width: f32) -> Response {
    li_ui_kit::widgets::primary_button(ui, tl!(label), min_width)
}

pub fn secondary_button(ui: &mut Ui, label: &str, min_width: f32) -> Response {
    li_ui_kit::widgets::secondary_button(ui, tl!(label), min_width)
}

/// What a dialog button does, which decides where the platform's button order puts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonRole {
    /// The default answer (OK, Save, Yes), drawn as the primary button.
    Default,
    /// Another answer that closes the dialog (Don't Save, No).
    Alternate,
    /// Closes the dialog without acting.
    Cancel,
    /// Acts but keeps the dialog open (Apply).
    Apply,
}

impl ButtonRole {
    /// Position from the left in the platform's order. Windows and Linux put the default action
    /// first (OK Cancel Apply, Yes No Cancel); macOS puts it last, in the corner, with Cancel beside
    /// it and the other answers further left (Don't Save, Cancel, Save).
    fn slot(self, mac: bool) -> u8 {
        match (self, mac) {
            (Self::Default, false) | (Self::Alternate, true) => 0,
            (Self::Alternate, false) | (Self::Cancel, true) => 1,
            (Self::Cancel, false) | (Self::Apply, true) => 2,
            (Self::Apply, false) | (Self::Default, true) => 3,
        }
    }
}

/// One button of a [`dialog_buttons`] row.
#[derive(Clone, Copy)]
pub struct DialogButton<'a> {
    pub role: ButtonRole,
    pub label: &'a str,
    pub min_width: f32,
    pub enabled: bool,
    /// Take keyboard focus this frame (see [`DialogButton::focus`]).
    pub focus: bool,
}

impl<'a> DialogButton<'a> {
    pub fn new(role: ButtonRole, label: &'a str, min_width: f32) -> Self {
        Self { role, label, min_width, enabled: true, focus: false }
    }

    pub fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    /// Asks for keyboard focus for this button: from there the arrow keys move between the row's
    /// buttons and Enter or Space presses the focused one.
    pub fn focus(self, focus: bool) -> Self {
        Self { focus, ..self }
    }
}

/// A dialog's button row at the cursor, in the platform's order (see [`ButtonRole`]): every modal
/// draws its buttons through this, so they all agree. Inside a right-to-left row it sits at the
/// right edge. The buttons are laid out left to right, so Tab walks them in reading order.
/// Returns the role of the button clicked this frame.
pub fn dialog_buttons(ui: &mut Ui, buttons: &[DialogButton]) -> Option<ButtonRole> {
    let mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
    let gap = ui.spacing().item_spacing.x;
    let size = buttons.iter().fold(Vec2::ZERO, |acc, b| {
        let s = button_size(ui, b.label, b.min_width);
        vec2(acc.x + s.x, acc.y.max(s.y))
    }) + vec2(gap * buttons.len().saturating_sub(1) as f32, 0.0);
    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        let mut hit = None;
        for slot in 0..4 {
            for b in buttons.iter().filter(|b| b.role.slot(mac) == slot) {
                let r = ui
                    .add_enabled_ui(b.enabled, |ui| {
                        if b.role == ButtonRole::Default { primary_button(ui, b.label, b.min_width) } else { secondary_button(ui, b.label, b.min_width) }
                    })
                    .inner;
                ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("dialog-button-role", b.role as u8)), r.rect));
                if b.focus && b.enabled {
                    r.request_focus();
                }
                if r.clicked() {
                    hit = Some(b.role);
                }
            }
        }
        hit
    })
    .inner
}

/// The size of a primary or secondary button: its label plus padding, at least `min_width` wide.
fn button_size(ui: &Ui, label: &str, min_width: f32) -> Vec2 {
    button_layout(ui, label, min_width, Tokens::get(ui.ctx()).text).1
}

fn button_layout(ui: &Ui, label: &str, min_width: f32, fg: Color32) -> (std::sync::Arc<egui::Galley>, Vec2) {
    li_ui_kit::widgets::button_layout(ui, tl!(label), min_width, fg)
}

/// Cached RGB data; drawing work is bounded by the 256 display bins, never the image size.
pub fn rgb_histogram(ui: &mut Ui, histogram: &li_raster::histogram::RgbHistogram, height: f32) -> egui::Response {
    li_ui_kit::histogram::editor(ui, li_ui_kit::histogram::Histogram::u64(&histogram.channels, Some(&histogram.luma)), height, tl!("Histogram"))
}

/// Small caps section label.
pub fn section_label(ui: &mut Ui, text: &str) {
    section_label_translated(ui, tl!(text));
}

/// [`section_label`] for text already in the UI language (see [`slider_row_translated`]).
pub fn section_label_translated(ui: &mut Ui, text: &str) {
    li_ui_kit::widgets::heading(ui, text, li_ui_kit::widgets::Profile::Editor, false);
}

/// Hairline separator.
pub fn hairline(ui: &mut Ui) {
    li_ui_kit::widgets::separator(ui, li_ui_kit::widgets::Profile::Editor, None);
}

/// Vertical hairline for horizontal layouts.
pub fn vline(ui: &mut Ui, height: f32) {
    li_ui_kit::widgets::separator(ui, li_ui_kit::widgets::Profile::Editor, Some(height));
}

/// Hue spectrum stops for colour sliders.
pub fn hue_stops() -> Vec<Color32> {
    (0..=12)
        .map(|i| {
            let h = i as f32 / 12.0;
            let rgb = egui::ecolor::Hsva::new(h, 0.85, 1.0, 1.0).to_srgb();
            Color32::from_rgb(rgb[0], rgb[1], rgb[2])
        })
        .collect()
}

/// A compact labelled dropdown in the studio style.
pub fn dropdown<T: PartialEq + Clone>(ui: &mut Ui, id: &str, current: &mut T, options: &[(T, &str)], width: f32) -> bool {
    let label = options.iter().find(|(v, _)| v == current).map(|(_, l)| tl!(l)).unwrap_or("—");
    let mut changed = false;
    dropdown_menu(ui, id, label, width, |ui| {
        for (v, l) in options {
            if ui.selectable_label(v == current, tl!(l)).clicked() {
                *current = v.clone();
                changed = true;
            }
        }
    });
    changed
}

/// The same dropdown shell for grouped choices with icons, hints and trailing actions.
pub fn dropdown_menu<R>(ui: &mut Ui, id: &str, selected: &str, width: f32, contents: impl FnOnce(&mut Ui) -> R) {
    li_ui_kit::widgets::choice_menu(ui, id, selected, width, contents);
}

/// The body of a right-click menu: as tall as its items up to the part of the window that can be
/// seen (clear of a taskbar a too-tall window runs under, #315), scrolling past that, so long
/// context menus (a layer's, the canvas tools') stay reachable on small windows instead of running
/// off the bottom. egui moves a popup up to keep it in the window, so a menu opened low on the
/// screen first shifts up and only scrolls when it is taller than the window.
pub fn menu_scroll<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    // The popup frame's margin and stroke, and a small gap to the window's edges.
    let frame = ui.spacing().menu_margin.sum().y + 2.0 + 2.0 * MENU_EDGE;
    let room = (crate::work_area::visible_rect(ui.ctx()).height() - frame).max(MENU_MIN_HEIGHT);
    // A popup's Ui is only as tall as the popup was last frame (400 pt on the first), so ask for
    // the whole room: the area still shrinks to its rows when they need less.
    egui::ScrollArea::vertical().max_height(room).min_scrolled_height(room).show(ui, add_contents).inner
}

/// Gap kept between a context menu and the window's edges, and the shortest it gets.
const MENU_EDGE: f32 = 4.0;
const MENU_MIN_HEIGHT: f32 = 120.0;

/// The colour picker popup of a colour swatch: a click on `swatch` toggles it, a click outside
/// closes it. While open, its left edge stays where it first showed (at the swatch, or further
/// left when the window edge needs it), below the swatch or above it as room allows. The picker's
/// width follows its value readouts, and placing it anew every frame moved it under the pointer,
/// flipping it from side to side near the right edge of the window (#534).
pub fn swatch_popup(swatch: &Response) -> egui::Popup<'static> {
    // Room for the readouts to widen after the picker opened.
    const SLACK: f32 = 32.0;
    let popup = egui::Popup::from_toggle_button_response(swatch).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let ctx = &swatch.ctx;
    let key = popup.get_id().with("left");
    if !popup.is_open() {
        ctx.data_mut(|d| d.remove::<f32>(key));
        return popup;
    }
    // Its width is known from the frame after it opened (egui sizes it unseen first).
    let left = ctx.data(|d| d.get_temp::<f32>(key)).or_else(|| {
        let width = popup.get_expected_size()?.x;
        let screen = ctx.content_rect();
        let left = swatch.rect.left().min(screen.right() - width - SLACK).max(screen.left());
        ctx.data_mut(|d| d.insert_temp(key, left));
        Some(left)
    });
    let Some(left) = left else { return popup };
    popup
        .anchor(Rect::from_x_y_ranges(left..=left, swatch.rect.y_range()))
        .align(egui::RectAlign::BOTTOM_START)
        .align_alternatives(&[egui::RectAlign::TOP_START])
}

pub fn dropdown_with_tooltips<T: PartialEq + Clone>(ui: &mut Ui, id: &str, current: &mut T, options: &[(T, &str, &str)], width: f32) -> bool {
    let label = options.iter().find(|(v, _, _)| v == current).map(|(_, l, _)| tl!(l)).unwrap_or("—");
    let mut changed = false;
    let response = egui::ComboBox::from_id_salt(id).selected_text(label).width(width).height(420.0).icon(chevron_icon).show_ui(ui, |ui| {
        for (v, l, tip) in options {
            if ui.selectable_label(v == current, tl!(l)).on_hover_text(tl!(tip)).clicked() {
                *current = v.clone();
                changed = true;
            }
        }
    });
    if let Some((_, _, tip)) = options.iter().find(|(v, _, _)| v == current) {
        let _ = response.response.on_hover_text(tl!(tip));
    }
    changed
}

/// A compact square icon button for panel footers and headers (the Swatches panel's New Group,
/// New Swatch, Delete and ☰): the same chrome as [`crate::icons::button`] (transparent until
/// hovered or focused by Tab) with an accessible `label` separate from its tooltip `tip`, and a
/// disabled state that explains itself: with `disabled = Some(why)` it takes no clicks or focus,
/// its icon fades to `text_faint`, `why` becomes the tooltip and screen readers see it disabled.
/// `label` and `tip` are shown as given (translate them first).
pub fn icon_action(ui: &mut Ui, icon: &str, box_size: f32, label: &str, tip: &str, disabled: Option<&str>) -> Response {
    li_ui_kit::widgets::icon_button(ui, Vec2::splat(box_size), false, disabled.is_none(), label, li_ui_kit::widgets::Profile::Editor, |ui, rect, tint| {
        crate::icons::paint(ui, rect, icon, (box_size * ICON_ACTION_GLYPH).round(), tint);
    })
    .on_hover_text(disabled.unwrap_or(tip))
}

/// Glyph size of an [`icon_action`] relative to its box (16 pt in the 24 pt footer box).
const ICON_ACTION_GLYPH: f32 = 2.0 / 3.0;

/// A colour sample tile: a swatch, the Fill/Stroke or Foreground/Background target, any place a
/// colour value is shown as a colour. Paints `color` over the transparency [`checker`] (so a
/// translucent colour reads as one) in `rect`, rounded `radius_sm` and outlined in `field_border`
/// so it holds its edge on the panel in light and dark themes. Paints only: wrap it in the
/// control that owns the click (a button, a chip) and give that control the accessible name.
/// The colour is document data, not a token.
pub fn color_tile(ui: &Ui, rect: Rect, color: Color32) {
    li_ui_kit::widgets::color_tile(ui, rect, color, li_ui_kit::widgets::Profile::Editor);
}

/// Named colour swatch opening the ordinary Color Picker in a shared popup.
/// Used by Symbols master editing; preserves HDR/channel values until the user changes paint.
pub fn color_control(ui: &mut Ui, color: &mut photocraft_doc::Color, label: &str) -> bool {
    use serde_json::{Map, Value, json};
    let rgb = color.to_rgb();
    let response = ui
        .horizontal(|ui| {
            let side = ui.spacing().interact_size.y;
            let (tile, tile_response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
            let tile_label = li_i18n::fmt(tl!("{label} colour swatch"), &[("label", label)]);
            tile_response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tile_label.as_str()));
            color_tile(
                ui,
                tile,
                Color32::from_rgba_unmultiplied(
                    (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (color.alpha.clamp(0.0, 1.0) * 255.0) as u8,
                ),
            );
            secondary_button(ui, label, 0.0).union(tile_response)
        })
        .inner;
    let key = response.id.with("color-fields");
    if response.clicked() {
        let hex = crate::color_picker_ui::hex(rgb);
        let fields: Map<String, Value> = serde_json::from_value(json!({"color":hex,"__orig":hex})).unwrap_or_default();
        ui.data_mut(|d| d.insert_temp(key, fields));
    }
    let mut changed = false;
    swatch_popup(&response).show(|ui| {
        let mut fields = ui.data(|d| d.get_temp::<Map<String, Value>>(key)).unwrap_or_default();
        let before = fields.get("color").cloned();
        crate::color_picker_ui::body(ui, &mut fields);
        if fields.get("color") != before.as_ref()
            && let Some(rgb) = fields.get("color").and_then(Value::as_str).and_then(crate::color_picker_ui::parse_hex)
        {
            let alpha = color.alpha;
            *color = photocraft_doc::Color::rgb(rgb[0], rgb[1], rgb[2]);
            color.alpha = alpha;
            changed = true;
        }
        let mut alpha = color.alpha;
        if slider_row_translated(ui, tl!("Opacity"), &mut alpha, 0.0..=1.0, "", None).changed() {
            color.alpha = alpha;
            changed = true;
        }
        ui.data_mut(|d| d.insert_temp(key, fields));
    });
    changed
}

/// Spectrum-style checkbox (blue when checked).
pub fn checkbox(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    li_ui_kit::widgets::checkbox(ui, on, tl!(label), li_ui_kit::widgets::Profile::Editor)
}

/// Photoshop-style numbers: "100", "12.5" (never "100.0").
pub fn fmt_num(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{r:.1}") }
}

/// Rounds `v` to `decimals` places and drops trailing zeros, so a value is never shown with more
/// precision than it has: `1.4140` at three places is `1.414`, `16.0` is `16`.
pub fn fmt_num_prec(v: f64, decimals: u32) -> String {
    let s = format!("{v:.*}", decimals as usize);
    let s = if decimals == 0 { s } else { s.trim_end_matches('0').trim_end_matches('.').to_string() };
    if s == "-0" { "0".to_string() } else { s }
}

/// Two-decimal variant of [`fmt_num`] for small ranges: `1.05`, `0.78`, `2`.
pub fn fmt_num2(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    format!("{r:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Parses a typed number or simple arithmetic such as `1280*2` or `20*2+5-2` (`+ - * /`,
/// `*` and `/` first). Like egui's own parser it ignores whitespace and reads `−` as `-`.
/// `None` for anything else, including a division by zero.
pub fn parse_num(text: &str) -> Option<f64> {
    let s = clean(text);
    s.parse().ok().or_else(|| sum(&s)).filter(|v: &f64| v.is_finite())
}

/// A plain typed number, no arithmetic.
fn plain(text: &str) -> Option<f64> {
    clean(text).parse().ok()
}

fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).map(|c| if c == '−' { '-' } else { c }).collect()
}

/// `a+b-c…`: a `+` or `-` right after an operand splits terms; anywhere else it is a sign.
fn sum(s: &str) -> Option<f64> {
    let (mut total, mut sign, mut start, mut prev) = (0.0, 1.0, 0, ' ');
    for (i, c) in s.char_indices() {
        if matches!(c, '+' | '-') && i > start && !matches!(prev, '*' | '/' | 'e' | 'E') {
            total += sign * product(s.get(start..i)?)?;
            sign = if c == '-' { -1.0 } else { 1.0 };
            start = i + 1;
        }
        prev = c;
    }
    Some(total + sign * product(s.get(start..)?)?)
}

/// `a*b/c…`, left to right.
fn product(s: &str) -> Option<f64> {
    let mut factors = s.split(['*', '/']).map(str::parse::<f64>);
    let mut acc = factors.next()?.ok()?;
    for (op, x) in s.matches(['*', '/']).zip(factors) {
        acc = if op == "*" { acc * x.ok()? } else { acc / x.ok()? };
    }
    Some(acc)
}

#[cfg(test)]
mod tests {
    #[test]
    fn status_chips_name_themselves_and_keep_one_height() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut h = Harness::builder().with_size(egui::vec2(360.0, 80.0)).build_ui_state(
            |ui, ready: &mut bool| {
                if *ready {
                    ui.horizontal(|ui| {
                        super::status_chip(ui, "circle-dashed", "Not tested", super::Tone::Neutral);
                        super::status_chip(ui, "triangle-alert", "Needs setup", super::Tone::Warning);
                        super::status_chip(ui, "x", "Failed", super::Tone::Danger);
                    });
                }
            },
            false,
        );
        crate::PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        *h.state_mut() = true;
        h.run();
        let (a, b, c) = (h.get_by_label("Not tested").rect(), h.get_by_label("Needs setup").rect(), h.get_by_label("Failed").rect());
        assert!(a.height() == b.height() && b.height() == c.height(), "{a:?} {b:?} {c:?}");
        assert!(c.width() < a.width(), "the pill fits its text");
        assert!(a.right() <= b.left() && b.right() <= c.left(), "chips sit side by side");
    }

    #[test]
    fn shared_checkbox_has_one_accessible_control_for_box_and_text() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        for profile in [li_ui_kit::widgets::Profile::Editor, li_ui_kit::widgets::Profile::Library] {
            let mut h = Harness::builder().with_size(egui::vec2(360.0, 120.0)).build_ui_state(
                move |ui, state: &mut (bool, bool)| {
                    if state.0 {
                        li_ui_kit::widgets::checkbox(ui, &mut state.1, "Sample All Layers", profile);
                        ui.add_enabled_ui(false, |ui| {
                            li_ui_kit::widgets::checkbox(ui, &mut false, "Unavailable", profile);
                        });
                        let _ = ui.button("Next");
                    }
                },
                (false, false),
            );
            crate::PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
            h.state_mut().0 = true;
            h.run();
            let control = h.get_by_label("Sample All Layers");
            assert_eq!(control.accesskit_node().role(), egui::accesskit::Role::CheckBox);
            assert_eq!(control.accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
            assert!(h.get_by_label("Unavailable").accesskit_node().is_disabled());
            let rect = control.rect();
            // Both the box and the end of the text activate the same labelled control.
            for (at, checked) in [(egui::pos2(rect.left() + 7.0, rect.center().y), true), (egui::pos2(rect.right() - 3.0, rect.center().y), false)] {
                h.hover_at(at);
                h.run_steps(1);
                h.drag_at(at);
                h.run_steps(1);
                h.drop_at(at);
                h.run_steps(2);
                assert_eq!(h.state().1, checked);
                assert_eq!(
                    h.get_by_label("Sample All Layers").accesskit_node().toggled(),
                    Some(if checked { egui::accesskit::Toggled::True } else { egui::accesskit::Toggled::False })
                );
            }
            h.get_by_label("Sample All Layers").focus();
            h.run_steps(1);
            h.key_press(egui::Key::Space);
            h.run_steps(2);
            assert!(h.state().1, "keyboard activation uses the checkbox's accessible id");
            h.key_press(egui::Key::Tab);
            h.run_steps(2);
            assert!(h.get_by_label("Next").accesskit_node().is_focused(), "the checkbox text must not create a second Tab stop");
        }
    }

    /// A right-aligned OK / Cancel / Apply row as `os` draws it: labels left to right, and the
    /// row's right edge with the window's.
    fn button_row(os: egui::os::OperatingSystem) -> (Vec<String>, f32, f32) {
        use super::{ButtonRole, DialogButton};
        use egui_kittest::{Harness, kittest::Queryable};
        // Drawn from the second frame, once the theme's fonts are bound.
        let mut h = Harness::builder().with_size(egui::vec2(500.0, 80.0)).build_ui_state(
            |ui, ready: &mut bool| {
                if *ready {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let row = [
                            DialogButton::new(ButtonRole::Default, "OK", 84.0),
                            DialogButton::new(ButtonRole::Cancel, "Cancel", 84.0),
                            DialogButton::new(ButtonRole::Apply, "Apply", 84.0),
                        ];
                        super::dialog_buttons(ui, &row);
                    });
                }
            },
            false,
        );
        crate::PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.ctx.set_os(os);
        *h.state_mut() = true;
        h.run();
        let mut drawn: Vec<(f32, f32, String)> =
            ["OK", "Cancel", "Apply"].iter().map(|l| (h.get_by_label(l).rect().left(), h.get_by_label(l).rect().right(), l.to_string())).collect();
        drawn.sort_by(|a, b| a.0.total_cmp(&b.0));
        let right = drawn.last().map_or(0.0, |d| d.1);
        (drawn.into_iter().map(|d| d.2).collect(), right, h.ctx.content_rect().right())
    }

    #[test]
    fn dialog_buttons_follow_the_platform_order() {
        use egui::os::OperatingSystem as Os;
        for (os, want) in [(Os::Windows, ["OK", "Cancel", "Apply"]), (Os::Nix, ["OK", "Cancel", "Apply"]), (Os::Mac, ["Cancel", "Apply", "OK"])] {
            let (order, right, edge) = button_row(os);
            assert_eq!(order, want, "{os:?}");
            assert!(edge - right < 20.0, "{os:?}: the row hugs the right edge ({right} of {edge})");
        }
    }

    #[test]
    fn two_decimal_numbers_trim_zeros() {
        assert_eq!(super::fmt_num2(1.05), "1.05");
        assert_eq!(super::fmt_num2(0.78), "0.78");
        assert_eq!(super::fmt_num2(0.5), "0.5");
        assert_eq!(super::fmt_num2(2.0), "2");
    }

    #[test]
    fn typed_arithmetic_evaluates() {
        use super::parse_num;
        for (text, want) in [
            ("1280*2", 2560.0),
            ("658 * 1.5", 987.0),
            ("48/3", 16.0),
            ("20*2+5-2", 43.0),
            ("2+3*4", 14.0),
            ("10/4*2", 5.0),
            ("-5+3", -2.0),
            ("5--3", 8.0),
            ("2*-3+1", -5.0),
            ("−4", -4.0),
            ("1 234", 1234.0),
            ("1e3/2", 500.0),
        ] {
            assert_eq!(parse_num(text), Some(want), "{text}");
        }
        for text in ["", "abc", "5+", "*2", "4/0", "0/0", "1+*2", "(2+3)", "1e400"] {
            assert_eq!(parse_num(text), None, "{text}");
        }
    }

    /// Types `text` into a value field holding `start`, then presses `key`. Returns the value an OK
    /// button that also fires on Enter saw, as dialogs read it, else the field's value. `round`
    /// makes the caller round the value every frame, as pixel fields do.
    fn type_and_press(start: f32, text: &str, key: egui::Key, round: bool) -> f32 {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut h = Harness::builder().with_size(egui::vec2(300.0, 100.0)).build_ui_state(
            move |ui, (v, ok): &mut (f32, Option<f32>)| {
                super::value_field(ui, v, 0.0..=300000.0, "px", 90.0);
                if round {
                    *v = v.round();
                }
                if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    *ok = Some(*v);
                }
            },
            (start, None),
        );
        h.get_by_role(egui::accesskit::Role::SpinButton).click();
        h.run();
        for c in text.chars() {
            h.event(egui::Event::Text(c.to_string()));
            h.run();
        }
        h.key_press(key);
        h.run();
        let (v, ok) = *h.state();
        ok.unwrap_or(v)
    }

    /// Plain digits apply as they're typed; arithmetic doesn't report a change until it's committed.
    #[test]
    fn value_field_holds_arithmetic_until_committed() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut h = Harness::builder().with_size(egui::vec2(300.0, 100.0)).build_ui_state(
            |ui, (v, changes): &mut (f32, u32)| {
                if super::value_field(ui, v, 0.0..=100.0, "%", 90.0).changed() {
                    *changes += 1;
                }
            },
            (100.0, 0),
        );
        h.get_by_role(egui::accesskit::Role::SpinButton).click();
        h.run();
        for (c, want) in [('5', (5.0, 1)), ('0', (50.0, 2)), ('/', (50.0, 2)), ('2', (50.0, 2))] {
            h.event(egui::Event::Text(c.to_string()));
            h.run();
            assert_eq!(*h.state(), want, "after {c}");
        }
        h.key_press(egui::Key::Enter);
        h.run();
        assert_eq!(*h.state(), (25.0, 3));
    }

    #[test]
    fn value_field_applies_typed_arithmetic() {
        use egui::Key::{Enter, Tab};
        assert_eq!(type_and_press(500.0, "1280*2", Tab, false), 2560.0);
        assert_eq!(type_and_press(500.0, "1280*2", Enter, false), 2560.0);
        // `5/2` alone would round to 3 under the caller; the whole expression still applies.
        assert_eq!(type_and_press(1.0, "5/2*2", Tab, true), 5.0);
        assert_eq!(type_and_press(1.0, "1280/3*2", Enter, true), 853.0);
    }

    /// The footer icon button names itself apart from its tooltip, and a disabled one is
    /// disabled for screen readers, ignores clicks and explains why in its tooltip.
    #[test]
    fn icon_action_names_itself_and_disables_with_a_reason() {
        use egui_kittest::{Harness, kittest::NodeT as _, kittest::Queryable};
        let mut h = Harness::builder().with_size(egui::vec2(200.0, 80.0)).build_ui_state(
            |ui, (on, clicks): &mut (bool, u32)| {
                ui.horizontal(|ui| {
                    if super::icon_action(ui, "plus", 24.0, "New Swatch", "New Swatch from the current colour", None).clicked() {
                        *clicks += 1;
                    }
                    let why = (!*on).then_some("Select a swatch first");
                    if super::icon_action(ui, "trash", 24.0, "Delete Swatch", "Delete Swatch (Delete)", why).clicked() {
                        *clicks += 10;
                    }
                });
            },
            (false, 0),
        );
        h.run();
        assert!(!h.get_by_label("New Swatch").accesskit_node().is_disabled());
        assert!(h.get_by_label("Delete Swatch").accesskit_node().is_disabled());
        h.get_by_label("Delete Swatch").click();
        h.run();
        assert_eq!(h.state().1, 0, "a disabled button takes no click");
        h.get_by_label("Delete Swatch").hover();
        h.run_steps(30);
        assert!(h.query_by_label("Select a swatch first").is_some(), "the reason is the tooltip");
        h.get_by_label("New Swatch").click();
        h.run();
        assert_eq!(h.state().1, 1);
        h.state_mut().0 = true;
        h.run();
        assert!(!h.get_by_label("Delete Swatch").accesskit_node().is_disabled());
        h.get_by_label("Delete Swatch").click();
        h.run();
        assert_eq!(h.state().1, 11);
    }

    #[test]
    fn numbers_drop_trailing_zero() {
        assert_eq!(super::fmt_num(100.0), "100");
        assert_eq!(super::fmt_num(12.46), "12.5");
        assert_eq!(super::fmt_num(-3.0), "-3");
    }
}

#[cfg(test)]
#[test]
fn ds2_value_field_displays_requested_four_decimal_precision() {
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(300., 100.)).build_ui_state(
        move |ui, state: &mut (f64, Rect)| {
            if !initialized {
                crate::PhotocraftApp::setup_context(ui.ctx(), Default::default());
                initialized = true;
                return;
            }
            state.1 = value_field_with_precision(ui, &mut state.0, 0.0..=1.0, "", 140., 4).rect;
        },
        (0.1234, Rect::NOTHING),
    );
    h.run_steps(4);
    assert!(h.output().shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="0.1234")));
    let pos = h.state().1.center();
    h.hover_at(pos);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
        h.step();
    }
    h.run_steps(2);
    h.event(egui::Event::Key { key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.event(egui::Event::Text("0.5678".into()));
    h.step();
    h.event(egui::Event::Key { key: egui::Key::Enter, physical_key: Some(egui::Key::Enter), pressed: true, repeat: false, modifiers: Default::default() });
    h.run_steps(2);
    assert!((h.state().0 - 0.5678).abs() < 1e-8);
}

/// Paints a menu row's check mark with the editor's shared monochrome `check` icon.
pub fn menu_row_check(ui: &Ui, response: &Response, label: &str, checked: Option<bool>) {
    li_commands::widgets::menu_row_check(ui, response, label, checked, |ui, rect, width, color| {
        crate::icons::paint(ui, rect, "check", width, color);
    });
}

/// A canvas/panel command row. Uses egui's themed menu button and the disabled-hover
/// API so unavailable commands remain discoverable without focus or activation.
/// The caller supplies a translated label with its active-profile shortcut.
pub fn command_menu_item(ui: &mut Ui, label: &str, shortcut: Option<&str>, checked: Option<bool>, reason: Option<&str>) -> egui::Response {
    egui::containers::menu::MenuConfig::find(ui).style.apply(ui.style_mut());
    let mut button = menu_row_button(ui, label).truncate();
    if let Some(shortcut) = shortcut {
        button = button.shortcut_text(shortcut);
    }
    let response = li_commands::widgets::command_menu_item(ui, button, label, reason.is_none(), checked, |ui, rect, width, color| {
        crate::icons::paint(ui, rect, "check", width, color);
    });
    if let Some(reason) = reason { disabled_reason_tooltip(response, reason) } else { response }
}

#[test]
fn dsfix_color_tile_click_opens_picker() {
    use egui_kittest::{Harness, kittest::Queryable as _};
    let mut h = Harness::builder().with_size(egui::vec2(800., 700.)).build_ui_state(
        |ui, color: &mut photocraft_doc::Color| {
            if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                color_control(ui, color, "Paint");
            }
        },
        photocraft_doc::Color::WHITE,
    );
    crate::PhotocraftApp::setup_context(&h.ctx, Default::default());
    h.run_steps(3);
    assert!(h.query_by_label("Paint").is_some(), "the original label button remains available");
    h.get_by_label("Paint colour swatch").click();
    h.run();
    assert!(h.query_by_label("Hex colour").is_some(), "the color tile opens the ordinary picker");
}

/// The app's module picker, placed first in the title bar's right-to-left controls in every
/// module/persona. Measures the shared pill tabs before reversing layout so it never eats the
/// title/drag gap. Host automation rectangles remain `host-mode-rect` + module digit.
pub fn module_switcher(ui: &mut Ui, current: crate::Module) -> Option<crate::Module> {
    let labels = crate::Module::ALL.map(|m| li_i18n::tr(li_i18n::current(), m.label()));
    let t = Tokens::get(ui.ctx());
    let width = labels
        .iter()
        .map(|label| {
            ui.painter().layout_no_wrap(label.to_string(), crate::theme::medium(crate::theme::PILL_TAB_FONT_SIZE), t.text).size().x
                + crate::theme::PILL_TAB_PADDING
        })
        .sum::<f32>()
        + (labels.len() - 1) as f32 * ui.spacing().item_spacing.x;
    let mut picked = None;
    ui.allocate_ui_with_layout(vec2(width, crate::theme::PILL_TAB_HEIGHT), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        for (module, label) in crate::Module::ALL.into_iter().zip(labels) {
            let r = pill_tab_translated(ui, label, module == current);
            ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("host-mode-rect", module.digit())), r.rect));
            let key = crate::shortcuts::pretty(&format!("Cmd+Alt+{}", module.digit()));
            if r.on_hover_text(format!("{} ({key})", li_i18n::tr(li_i18n::current(), module.tip()))).clicked() && module != current {
                picked = Some(module);
            }
        }
    });
    picked
}

/// The shared Preferences navigation tab: themed, accessible and elided, with keyboard focus.
/// Used by both editor and hosted Library/Develop pages in the one Preferences window.
pub fn preferences_tab(ui: &mut egui::Ui, title: &str, selected: bool, empty: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), crate::theme::PREFERENCES_TAB_HEIGHT), Sense::click());
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, t.radius_sm, if selected { t.row_selected } else { t.hover });
    }
    let color = if selected {
        t.text
    } else if empty {
        t.text_faint
    } else {
        t.text_dim
    };
    let mut job = egui::text::LayoutJob::simple_singleline(title.to_owned(), crate::theme::medium(crate::theme::PILL_TAB_FONT_SIZE), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - crate::theme::PREFERENCES_TAB_TEXT_MARGIN);
    let galley = ui.painter().layout_job(job);
    let elided = galley.elided;
    ui.painter().galley(rect.left_center() + vec2(crate::theme::PREFERENCES_TAB_INSET, -galley.size().y / 2.0), galley, color);
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, title));
    let response = if elided { response.on_hover_text(title) } else { response };
    let seen = ui.id().with(("prefs-nav-seen", title));
    if selected && !ui.is_sizing_pass() && !ui.data(|d| d.get_temp::<bool>(seen).unwrap_or(false)) {
        response.scroll_to_me(Some(egui::Align::Center));
        ui.data_mut(|d| d.insert_temp(seen, true));
    } else if !selected {
        ui.data_mut(|d| d.remove::<bool>(seen));
    }
    response
}

/// Preflight notice for on-device CPU AI. Shared by Restore and selection/model controls.
/// Returns whether the model fits the interactive CPU time limit; warnings use theme tokens.
pub fn cpu_estimate(ui: &mut Ui, estimate: li_seg::estimate::Estimate) -> bool {
    let t = Tokens::get(ui.ctx());
    let color = if estimate.warning() { t.warning } else { t.text_dim };
    ui.label(egui::RichText::new(estimate.label()).color(color));
    estimate.allowed()
}

/// Total CPU estimate for cancellable background AI batches (Smart Sort and indexing).
/// The per-model inference guard still applies; a large batch shows a warning and can run.
pub fn cpu_batch_estimate(ui: &mut egui::Ui, estimate: li_seg::estimate::Estimate) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(estimate.batch_label()).color(if estimate.warning() { t.warning } else { t.text_dim }));
}

mod status;
pub use status::{Tone, status_chip};

#[cfg(test)]
#[path = "widgets/numeric_input_tests.rs"]
mod numeric_input_tests;
