//! Lightroom-style widgets: sliders with gradient tracks and hollow thumbs, section headers,
//! flyout rows, bordered buttons. Every interactive widget registers an automation id + rect
//! ([`register`]) so the control channel can find and click it by name.

use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use lightcraft_develop::{ControlSpec, Track};

use crate::icons::{Icon, paint};
use crate::theme::Tokens;
use li_ui_kit::widget_style::text_button_size;

mod parametric;
pub(crate) use parametric::parametric_drag;

/// egui clears the press origin on a release batched into the same frame.
pub(crate) fn primary_press(ui: &Ui, response: &Response) -> Option<Pos2> {
    ui.input(|input| {
        input.pointer.press_origin().or_else(|| {
            input.events.iter().rev().find_map(|event| match event {
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } => Some(*pos),
                _ => None,
            })
        })
    })
    .or(response.interact_pointer_pos())
}

/// Claim the original press even when a complete gesture arrives in one input frame.
pub(crate) fn primary_pressed(ui: &Ui, response: &Response) -> bool {
    ui.is_enabled()
        && ui.memory(|memory| memory.is_above_modal_layer(response.layer_id))
        && ui.input(|input| input.pointer.primary_pressed())
        && primary_press(ui, response).is_some_and(|point| response.rect.contains(point) && ui.ctx().layer_id_at(point) == Some(response.layer_id))
}

// automation registry

pub use li_ui_kit::widgets::{Registry, color_swatch, register, take_registry};

/// Standard Develop content inset, used by Optics and its lens database rows.
pub fn develop_body(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE.inner_margin(crate::theme::DEVELOP_INSET).show(ui, add);
}

/// The shared, keyboard-accessible checkbox with a stable automation id. `label` is translated
/// by the caller; egui supplies the check mark, focus and disabled treatment from the theme.
pub fn checkbox(ui: &mut Ui, id: &str, checked: &mut bool, label: &str) -> Response {
    let response = li_ui_kit::widgets::checkbox(ui, checked, li_i18n::library::t(label), li_ui_kit::widgets::Profile::Library);
    register(ui.ctx(), id, response.rect);
    // Preserve both jobs' public automation IDs with one widget implementation.
    if !id.starts_with("check:") {
        register(ui.ctx(), format!("check:{id}"), response.rect);
    }
    response
}

/// Wrapping secondary text inside a Develop section, using the theme's Small type style.
pub fn panel_note(ui: &mut Ui, text: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(egui::RichText::new(text).small().color(t.text_dim)).wrap())
}

// preview-only raws

/// The decoder's reason why a raw is shown from its embedded preview, shortened for the UI
/// ("Nikon Huffman-compressed NEF (no clean-room …)" → "Nikon Huffman-compressed NEF").
pub fn preview_only_variant(reason: &str) -> &str {
    reason.split(" (").next().unwrap_or(reason).trim()
}

/// What a preview-only raw means for the user (see `Photo::preview_only`).
pub fn preview_only_explanation(reason: &str) -> String {
    format!(
        "Local Image can't decode this raw variant yet ({}). You're editing the camera's embedded JPEG preview, \
         which already includes the camera's picture style (e.g. Monochrome) and white balance.",
        preview_only_variant(reason)
    )
}

/// A panel notice for a raw shown from its embedded preview (Edit, Info): an amber info icon,
/// "Preview only" and the explanation. Registered as `notice:previewOnly:{key}`.
pub fn preview_only_notice(ui: &mut Ui, key: &str, reason: &str) {
    let t = Tokens::get(ui.ctx());
    let r = egui::Frame::NONE
        .fill(t.canvas)
        .stroke(Stroke::new(1.0, t.caution.gamma_multiply(0.45)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin { left: 10, right: 10, top: 8, bottom: 9 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                paint(ui.painter(), ir, Icon::Info, t.caution);
                ui.label(egui::RichText::new(li_i18n::library::t("Preview only")).font(t.semibold(12.5)).color(t.text));
            });
            ui.add_space(2.0);
            ui.label(egui::RichText::new(preview_only_explanation(reason)).size(11.5).color(t.text_label));
        })
        .response;
    let r = r.on_hover_text(li_i18n::tr_format!("Decoder: {reason}", reason = reason));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "Preview only: editing the camera's embedded JPEG"));
    register(ui.ctx(), format!("notice:previewOnly:{key}"), r.rect);
}

// colour helpers

pub fn hex(s: &str) -> Color32 {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0x808080);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// Band colours for the 8 colour-mixer bands (swatches and tracks).
pub const BAND_COLORS: [&str; 8] = ["#aa0000", "#bb6600", "#aaaa00", "#009900", "#00aaaa", "#0044cc", "#7722aa", "#aa0077"];

fn track_stops(track: &Track) -> Option<Vec<Color32>> {
    let mut stops: Vec<Color32> = match track {
        Track::Plain | Track::Centered => return None,
        Track::Temp => ["#193961", "#47566a", "#60666d", "#70705b", "#737339", "#75742a"].iter().map(|h| hex(h)).collect(),
        Track::Tint => ["#246123", "#2c552c", "#394739", "#4a3d4c", "#6b336b", "#872c87"].iter().map(|h| hex(h)).collect(),
        Track::Gradient { from, to } => vec![hex(from), hex(to)],
        Track::Hue { band } => {
            let i = *band as usize;
            let prev = hex(BAND_COLORS[(i + 7) % 8]);
            let next = hex(BAND_COLORS[(i + 1) % 8]);
            let me = hex(BAND_COLORS[i]);
            vec![
                prev.gamma_multiply(0.7),
                lerp(prev, me, 0.5).gamma_multiply(0.7),
                me.gamma_multiply(0.7),
                lerp(me, next, 0.5).gamma_multiply(0.7),
                next.gamma_multiply(0.7),
            ]
        }
        Track::Sat { band } => vec![hex("#454343"), lerp(hex("#454343"), hex(BAND_COLORS[*band as usize]), 0.6), hex(BAND_COLORS[*band as usize])],
        Track::Lum { band } => {
            let c = hex(BAND_COLORS[*band as usize]);
            vec![c.gamma_multiply(0.2), c.gamma_multiply(0.55), lerp(c, Color32::WHITE, 0.35), lerp(c, Color32::WHITE, 0.6)]
        }
        Track::Rainbow => ["#aa0000", "#aaaa00", "#009900", "#00aaaa", "#0044cc", "#aa0077", "#aa0000"].iter().map(|h| hex(h)).collect(),
    };
    // The inherited painter interpolated RGB as opaque even for gamma-multiplied
    // stops. Preserve those displayed stop colors when passing the shared mesh.
    for color in &mut stops {
        *color = Color32::from_rgb(color.r(), color.g(), color.b());
    }
    Some(stops)
}

/// Result of a slider interaction this frame.
#[derive(Default, Debug, Clone, Copy)]
pub struct SliderOut {
    pub value: Option<f64>,
    pub drag_started: bool,
    pub drag_stopped: bool,
    pub reset: bool,
}

/// A Lightroom slider row (label + value above a track with a hollow ring thumb).
/// Double-click the label or thumb to reset. Shift-drag = fine adjustment.
pub fn slider(ui: &mut Ui, spec: &ControlSpec, value: f64, enabled: bool, label_override: Option<&str>) -> SliderOut {
    slider_inner(ui, spec, value, enabled, label_override, None)
}

/// Explicit document/target identity for a numeric edit; ordinary UI drags keep their IDs.
pub fn numeric_owner(app: &crate::LightcraftApp, target: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::new((app.session.library_identity(), app.session.library_generation, app.session.active()))
        // Auto Sync commits through the current selected target list. Its policy and
        // ordered IDs are captured too; even a same-active-photo change cancels.
        .with((app.session.auto_sync, &app.session.selection.ids))
        .with((app.session.active_mask, app.session.active_spot, app.ui.eye))
        .with(format!("{:?}/{:?}/{:?}", app.ui.view, app.ui.right, app.ui.dialog.as_ref().map(std::mem::discriminant)))
        .with(target)
}

pub fn numeric_slider(ui: &mut Ui, spec: &ControlSpec, value: f64, enabled: bool, label_override: Option<&str>, owner: egui::Id) -> SliderOut {
    slider_inner(ui, spec, value, enabled, label_override, Some(owner))
}

#[derive(Clone)]
struct NumericDraft {
    text: String,
    owner: egui::Id,
    base: f64,
    frame: u64,
}

fn slider_inner(ui: &mut Ui, spec: &ControlSpec, value: f64, enabled: bool, label_override: Option<&str>, owner: Option<egui::Id>) -> SliderOut {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (row, _) = ui.allocate_exact_size(vec2(w, t.slider_row_h), Sense::hover());
    let pad_l = 24.0;
    let pad_r = 22.0;
    let label_rect = Rect::from_min_size(pos2(row.left() + pad_l, row.top() + 4.0), vec2(w - pad_l - pad_r, 18.0));
    let track_rect = Rect::from_min_max(pos2(row.left() + pad_l, row.top() + 22.0), pos2(row.right() - pad_r, row.top() + 40.0));
    let id = ui.id().with(spec.id);
    let edit_id = id.with("numeric-edit");
    let frame = ui.ctx().cumulative_frame_nr();
    let mut draft = ui.data(|d| d.get_temp::<NumericDraft>(edit_id));
    let admissible = enabled && ui.is_enabled() && ui.memory(|m| m.is_above_modal_layer(ui.layer_id()));
    if draft.as_ref().is_some_and(|d| Some(d.owner) != owner || d.base != value || d.frame.saturating_add(1) < frame || !admissible) {
        ui.data_mut(|d| d.remove::<NumericDraft>(edit_id));
        ui.memory_mut(|m| m.surrender_focus(edit_id));
        draft = None;
    }
    let editing = draft.is_some();
    let resp = ui.interact(track_rect.expand2(vec2(8.0, 2.0)), id, if enabled && !editing { Sense::click_and_drag() } else { Sense::hover() });
    // screen readers: a slider named after its control, with its value
    let label_text = li_i18n::library::t(label_override.unwrap_or(spec.label)).to_string();
    resp.widget_info(|| egui::WidgetInfo::slider(enabled, value, label_text.clone()));
    let value_rect = Rect::from_min_max(pos2(label_rect.right() - 72., label_rect.top()), label_rect.max);
    let label_hit = Rect::from_min_max(label_rect.min, pos2(value_rect.left() - 4., label_rect.bottom()));
    let label_resp = ui.interact(label_hit, id.with("label"), if enabled { Sense::click() } else { Sense::hover() });
    register(ui.ctx(), format!("slider:{}", spec.id), track_rect);
    let mut out = SliderOut::default();
    let scale = li_ui_kit::slider::Scale { min: spec.min, max: spec.max, step: Some(spec.step) };
    let mut v = value;
    if enabled && !editing && (resp.double_clicked() || label_resp.double_clicked()) {
        out.reset = true;
        out.value = Some(spec.default);
        v = spec.default;
    } else if admissible && !editing {
        let result = li_ui_kit::slider::interact(
            ui,
            &resp,
            track_rect,
            value,
            li_ui_kit::slider::Interaction {
                scale,
                fine_factor: Some(0.1),
                epsilon: 1e-12,
                hover_nudge: Some(row),
                nudge_unit: (spec.id == "wb.temp").then_some(50.0),
            },
        );
        out.value = result.value;
        out.drag_started = result.drag_started;
        out.drag_stopped = result.drag_stopped;
        v = result.value.unwrap_or(value);
    }
    // paint
    let hovered = resp.hovered() || resp.dragged();
    let text_c = if enabled { t.text_label } else { t.text_disabled };
    let p = ui.painter();
    p.text(label_rect.left_center(), Align2::LEFT_CENTER, li_i18n::library::t(label_override.unwrap_or(spec.label)), t.font(12.5), text_c);
    let shown = if spec.id == "wb.temp" { format!("{v:.0}") } else { spec.format(v).replace("+0.00", "0").replace("-0.00", "0") };
    let shown = if shown == "+0" || shown == "-0" { "0".to_string() } else { shown };
    if let Some(draft) = &mut draft {
        // Consume confirmation before TextEdit handles it: Escape must never become
        // indistinguishable from focus loss (which deliberately commits a valid draft).
        let focused = ui.memory(|m| m.has_focus(edit_id));
        let escape = focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let enter = focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let response = ui.put(
            value_rect,
            egui::TextEdit::singleline(&mut draft.text)
                .id(edit_id)
                .font(t.font(12.5))
                .horizontal_align(egui::Align::RIGHT)
                // Keep ownership through egui's begin-pass Escape handling so the
                // focused field can cancel; otherwise Escape looks like valid blur.
                .event_filter(egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, escape: true, ..Default::default() }),
        );
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::text_edit(admissible, &draft.text, &draft.text, "");
            info.label = Some(label_text.clone());
            info
        });
        register(ui.ctx(), format!("value:{}", spec.id), response.rect);
        if escape || response.lost_focus() || enter || !response.has_focus() {
            if !escape
                && (enter || response.lost_focus())
                && admissible
                && let Ok(number) = draft.text.trim().parse::<f64>()
                && number.is_finite()
                && number >= spec.min
                && number <= spec.max
            {
                out.value = Some(number);
                out.drag_started = true;
                out.drag_stopped = true;
            }
            ui.data_mut(|d| d.remove::<NumericDraft>(edit_id));
            response.surrender_focus();
        } else {
            draft.frame = frame;
            ui.data_mut(|d| d.insert_temp(edit_id, draft.clone()));
        }
    } else {
        p.text(label_rect.right_center(), Align2::RIGHT_CENTER, shown, t.font(12.5), text_c);
        let response = ui.interact(value_rect, id.with("value"), if admissible && owner.is_some() { Sense::click() } else { Sense::hover() });
        register(ui.ctx(), format!("value:{}", spec.id), response.rect);
        if response.clicked()
            && let Some(owner) = owner
        {
            // Start empty: each typed number is an intentional replacement. Invalid,
            // nonfinite and out-of-range intermediates never touch the document.
            ui.data_mut(|d| d.insert_temp(edit_id, NumericDraft { text: String::new(), owner, base: value, frame }));
            ui.memory_mut(|m| m.request_focus(edit_id));
        }
    }
    li_ui_kit::slider::paint_hollow(
        ui,
        track_rect,
        track_stops(&spec.track).as_deref(),
        scale.position(track_rect, v),
        enabled,
        hovered,
        li_ui_kit::slider::HollowStyle {
            track: t.track,
            background: t.chrome,
            thumb: t.thumb,
            thumb_hover: t.thumb_hover,
            disabled: t.text_disabled,
        },
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    out
}

/// `value` moved by `steps` keyboard nudges: one nudge is about 1/200 of the range, a whole
/// number of the control's steps (exposure 0.05, most sliders 1, temperature 50 K).
pub fn nudged(spec: &ControlSpec, value: f64, steps: f64) -> f64 {
    li_ui_kit::slider::Scale { min: spec.min, max: spec.max, step: Some(spec.step) }.nudged(value, steps, (spec.id == "wb.temp").then_some(50.0))
}

/// Collapsible section header ("› Light"). Returns the response (click toggles).
pub fn section_header(ui: &mut Ui, id: &str, title: &str, open: bool, enabled: Option<bool>) -> (Response, Option<bool>) {
    li_ui_kit::widgets::library_header(ui, id, li_i18n::library::t(title), open, enabled, |ui, rect, glyph, color| {
        use li_ui_kit::widgets::HeaderGlyph;
        paint(
            ui.painter(),
            rect,
            match glyph {
                HeaderGlyph::Open => Icon::ChevronDown,
                HeaderGlyph::Closed => Icon::ChevronRight,
                HeaderGlyph::Eye => Icon::Eye,
            },
            color,
        );
    })
}

/// Shared settings checkbox, translated and sized by the installed theme. Used in Preferences
/// and other Library controls; callers retain their stable automation registration.
/// Thin divider between groups.
pub fn divider(ui: &mut Ui) {
    li_ui_kit::widgets::separator(ui, li_ui_kit::widgets::Profile::Library, None);
}

/// A flyout row inside a section (Curve, Color Mixer, Color Grading…): inset dark box with icon tile.
pub fn flyout_row(ui: &mut Ui, id: &str, title: &str, icon: Icon, open: bool) -> Response {
    let title = li_i18n::library::t(title);
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (outer, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    let r = Rect::from_min_max(pos2(outer.left() + 16.0, outer.top() + 4.0), pos2(outer.right() - 16.0, outer.bottom() - 4.0));
    let resp = ui.interact(r, ui.id().with(("flyout", id)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, title));
    register(ui.ctx(), format!("flyout:{id}"), r);
    let p = ui.painter();
    p.rect_filled(r, 4.0, if resp.hovered() { t.hover.gamma_multiply(0.8) } else { t.inset });
    let tile = Rect::from_min_size(pos2(r.left() + 4.0, r.top() + 4.0), vec2(r.height() - 8.0, r.height() - 8.0));
    p.rect_filled(tile, 3.0, t.chrome.gamma_multiply(1.15));
    paint(p, tile.shrink(3.0), icon, t.text_label);
    p.text(pos2(tile.right() + 10.0, r.center().y), Align2::LEFT_CENTER, title, t.semibold(13.0), t.text_label);
    paint(
        p,
        Rect::from_center_size(pos2(r.right() - 14.0, r.center().y), vec2(12.0, 12.0)),
        if open { Icon::ChevronDown } else { Icon::TriangleLeft },
        t.text_label,
    );
    resp
}

/// A small bordered text button, retaining translation and automation IDs.
pub fn text_button(ui: &mut Ui, id: &str, label: &str, active: bool) -> Response {
    let label = if id.starts_with("labelSet-") { label } else { li_i18n::library::t(label) };
    let response = li_ui_kit::widgets::selection(ui, label, active, li_ui_kit::widgets::SelectionStyle::Text, None);
    register(ui.ctx(), format!("button:{id}"), response.rect);
    response
}

/// A segmented control: `labels` share the available width equally, `per_row` per row. Each segment is
/// addressable as `button:{id}-{key}` (keys parallel to labels). Returns the clicked index.
pub fn segmented(ui: &mut Ui, id: &str, items: &[(&str, &str)], active: Option<usize>, per_row: usize) -> Option<usize> {
    li_ui_kit::widgets::segmented(ui, id, items, active, per_row, li_i18n::library::t)
}

/// An icon-only button. `active` draws the selected background (tool strip).
pub fn icon_button(ui: &mut Ui, id: &str, icon: Icon, size: egui::Vec2, active: bool, enabled: bool, tooltip: &str) -> Response {
    icon_button_inner(ui, &format!("icon:{id}"), icon, None, size, active, enabled, tooltip)
}

/// [`icon_button`] registered under a full automation id (e.g. `button:layerNew`) instead of
/// `icon:{id}`: for icon buttons that replaced text buttons and keep their old ids.
pub fn icon_button_as(ui: &mut Ui, automation_id: &str, icon: Icon, size: egui::Vec2, tooltip: &str) -> Response {
    icon_button_inner(ui, automation_id, icon, None, size, false, true, tooltip)
}

pub fn tool_button(ui: &mut Ui, id: &str, icon: Icon, size: egui::Vec2, active: bool, enabled: bool, tooltip: &str) -> Response {
    let tool = id.strip_prefix("removeMode-").unwrap_or(id);
    let name = crate::icons::tool_name(if tool.starts_with("tat-") { "target" } else { tool });
    icon_button_inner(ui, &format!("icon:{id}"), icon, name, size, active, enabled, tooltip)
}

fn icon_button_inner(
    ui: &mut Ui,
    automation_id: &str,
    icon: Icon,
    tool: Option<&str>,
    size: egui::Vec2,
    active: bool,
    enabled: bool,
    tooltip: &str,
) -> Response {
    let tooltip = li_i18n::library::t(tooltip);
    let response = li_ui_kit::widgets::icon_button(ui, size, active, enabled, tooltip, li_ui_kit::widgets::Profile::Library, |ui, r, c| {
        let glyph = (size.x.min(size.y) * 0.62).min(22.0);
        let glyph = if tool.is_some() { glyph.max(20.0) } else { glyph };
        let rect = Rect::from_center_size(r.center(), vec2(glyph, glyph));
        if let Some(name) = tool {
            crate::icons::paint_tool(ui, rect, name, icon, c, enabled);
        } else {
            paint(ui.painter(), rect, icon, c);
        }
    });
    register(ui.ctx(), automation_id, response.rect);
    if !tooltip.is_empty() { response.on_hover_text(tooltip) } else { response }
}

// disclosure rows

/// A compact disclosure row inside a panel: a chevron and a semibold title that open and close
/// what follows (the Develop layer list, a layer's whole-image tools). [`section_header`] is the
/// full-width 52 pt section header; this is the one-line version. `width` is the row's width;
/// a long title is cut short with "…". `secondary` draws the title dimmed (a folded group that is
/// not the panel's main content). Registered under the full automation id `id`; screen readers
/// get a collapsing header with `title` and its open state. Click toggles (the caller flips it).
pub fn disclosure(ui: &mut Ui, id: &str, title: &str, open: bool, width: f32, secondary: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(width.max(24.0), DISCLOSURE_H), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, title));
    register(ui.ctx(), id, r);
    let c = if resp.hovered() || resp.has_focus() {
        t.text
    } else if secondary {
        t.text_dim
    } else {
        t.text_label
    };
    let p = ui.painter();
    paint(
        p,
        Rect::from_center_size(pos2(r.left() + 7.0, r.center().y), vec2(11.0, 11.0)),
        if open { Icon::ChevronDown } else { Icon::ChevronRight },
        c,
    );
    let mut job = egui::text::LayoutJob::simple_singleline(title.to_string(), t.semibold(12.5), c);
    job.wrap = egui::text::TextWrapping::truncate_at_width((r.width() - DISCLOSURE_INDENT).max(0.0));
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos2(r.left() + DISCLOSURE_INDENT, r.center().y - galley.size().y / 2.0), galley, c);
    resp
}

/// Height of a [`disclosure`] row.
pub const DISCLOSURE_H: f32 = 24.0;
/// Where a [`disclosure`] row's title starts; rows it opens indent their content by this much.
pub const DISCLOSURE_INDENT: f32 = 18.0;

// colour samples

/// Corner radius of a [`color_swatch`] tile (one less than the 4 pt buttons, which it sits beside).
pub const SWATCH_RADIUS: f32 = li_ui_kit::widget_style::SWATCH_RADIUS;

/// A range readout: `label` (small, `text_label`) over a full-width strip of colour samples with
/// the selected window(s) outlined, as Skin Tone shows its hue, saturation and lightness ranges.
/// `sample(f)` is the colour at `f` in 0..1 (sampled at each step's centre); `windows` are
/// `(lo, hi)` fractions in 0..1 (two windows for a range that wraps, like hue). The outline is the
/// overlay pair (`overlay_bg` outside, `overlay_fg` inside) so it shows on any colour. The strip
/// is registered under the full automation id `id` and named "`label` range".
pub fn range_strip(ui: &mut Ui, id: &str, label: &str, sample: impl Fn(f32) -> Color32, windows: &[(f32, f32)]) -> Response {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(label).font(t.font(11.0)).color(t.text_label));
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), RANGE_STRIP_H), Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("{label} range")));
    register(ui.ctx(), id, r);
    let p = ui.painter();
    for i in 0..RANGE_STRIP_STEPS {
        let (a, b) = (i as f32 / RANGE_STRIP_STEPS as f32, (i + 1) as f32 / RANGE_STRIP_STEPS as f32);
        let step = Rect::from_min_max(pos2(r.left() + r.width() * a, r.top()), pos2(r.left() + r.width() * b, r.bottom()));
        p.rect_filled(step, 0.0, sample((a + b) * 0.5));
    }
    for &(lo, hi) in windows {
        let (lo, hi) = (lo.clamp(0.0, 1.0), hi.clamp(0.0, 1.0));
        if hi > lo {
            let w = Rect::from_min_max(pos2(r.left() + r.width() * lo, r.top()), pos2(r.left() + r.width() * hi, r.bottom()));
            p.rect_stroke(w, 0.0, Stroke::new(3.0, t.overlay_bg), StrokeKind::Inside);
            p.rect_stroke(w.shrink(1.0), 0.0, Stroke::new(1.0, t.overlay_fg), StrokeKind::Inside);
        }
    }
    resp
}

/// Height of a [`range_strip`]'s strip.
pub const RANGE_STRIP_H: f32 = 14.0;
const RANGE_STRIP_STEPS: usize = 64;

// badges over the photo

/// A badge over photo pixels: the canvas's active-layer pill, a picker's cursor badge, a preset's
/// name while its preview shows. `overlay_fg` text and/or icon on an `overlay_bg` scrim, which
/// read the same over any photo and in light or dark chrome (the Library grid's badges use the
/// same pair). `at` is the badge's top-left; text longer than `max_width` is cut short with "…".
/// Paints only (the canvas under it keeps the pointer); returns the badge's rect.
pub fn image_badge(painter: &egui::Painter, at: Pos2, icon: Option<Icon>, text: Option<&str>, max_width: f32) -> Rect {
    let t = Tokens::get(painter.ctx());
    // an icon sits 3 pt from the edge; text keeps BADGE_PAD on its open sides
    let icon_w = if icon.is_some() { BADGE_H - 6.0 } else { 0.0 };
    let lead = if icon.is_some() { 3.0 + icon_w + if text.is_some() { 4.0 } else { 3.0 } } else { BADGE_PAD };
    let max_width = max_width.max(BADGE_H);
    let galley = text.map(|text| {
        let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), t.font(12.0), t.overlay_fg);
        job.wrap = egui::text::TextWrapping::truncate_at_width((max_width - lead - BADGE_PAD).max(8.0));
        painter.layout_job(job)
    });
    let width = galley.as_ref().map_or(lead, |g| lead + g.size().x + BADGE_PAD);
    let r = Rect::from_min_size(at, vec2(width.min(max_width), BADGE_H));
    painter.rect_filled(r, BADGE_RADIUS, t.overlay_bg.gamma_multiply(BADGE_SCRIM));
    if let Some(icon) = icon {
        paint(painter, Rect::from_min_size(pos2(r.left() + 3.0, r.top() + 3.0), vec2(icon_w, icon_w)), icon, t.overlay_fg);
    }
    if let Some(g) = galley {
        painter.with_clip_rect(r).galley(pos2(r.left() + lead, r.center().y - g.size().y / 2.0), g, t.overlay_fg);
    }
    r
}

/// Height and corner radius of an [`image_badge`].
pub const BADGE_H: f32 = 24.0;
pub const BADGE_RADIUS: f32 = 6.0;
const BADGE_PAD: f32 = 10.0;
/// Opacity of an [`image_badge`]'s scrim over the photo.
const BADGE_SCRIM: f32 = 0.85;

/// Star rating row (5 stars); returns a new rating when clicked (clicking the current rating clears).
pub fn stars(ui: &mut Ui, id: &str, rating: u8, size: f32) -> Option<u8> {
    let t = Tokens::get(ui.ctx());
    let mut out = None;
    let hover_n = {
        let mut h = None;
        for i in 0..5u8 {
            let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
            register(ui.ctx(), format!("star:{id}:{}", i + 1), r);
            if resp.hovered() {
                h = Some(i + 1);
            }
            if resp.clicked() {
                out = Some(if rating == i + 1 { 0 } else { i + 1 });
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "", t.font(1.0), t.star);
            let filled = i < rating;
            paint(ui.painter(), r.shrink(size * 0.12), if filled { Icon::StarFilled } else { Icon::Star }, if filled { t.star } else { t.icon });
        }
        h
    };
    let _ = hover_n;
    out
}

/// A borderless dropdown button: `text` followed by a painted chevron (no font glyph needed).
pub fn dropdown(ui: &mut Ui, id: &str, text: &str, font: egui::FontId, color: Color32) -> Response {
    let response = li_ui_kit::widgets::choice_button(ui, text, font, color, |ui, rect, c| paint(ui.painter(), rect, Icon::ChevronDown, c));
    register(ui.ctx(), format!("dropdown:{id}"), response.rect);
    response
}

/// A labelled, width-bounded profile choice, shared by camera and lens profile pickers.
/// Rows register both the combo and each choice for headless and accessibility tests.
pub fn profile_choice(ui: &mut Ui, id: &str, label: &str, selected: &str, choices: &[(String, String, String)]) -> Option<String> {
    let t = Tokens::get(ui.ctx());
    let mut picked = None;
    egui::Frame::NONE.inner_margin(egui::Margin { left: t.panel_inset_left, right: t.panel_inset_right, top: 0, bottom: 0 }).show(ui, |ui| {
        ui.label(egui::RichText::new(li_i18n::library::t(label)).color(t.text_dim));
        let name = choices
            .iter()
            .find(|(key, _, _)| key == selected)
            .map(|(_, name, _)| name.as_str())
            .unwrap_or_else(|| li_i18n::library::t("Missing Profile"));
        let r = egui::ComboBox::from_id_salt(id).selected_text(name).width(ui.available_width()).truncate().show_ui(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            let mut group = String::new();
            for (key, name, section) in choices {
                if group != *section {
                    if !group.is_empty() {
                        ui.separator();
                    }
                    ui.label(egui::RichText::new(li_i18n::library::t(section)).color(t.text_dim));
                    group = section.clone();
                }
                let r = ui.selectable_label(key == selected, name);
                register(ui.ctx(), format!("choice:{id}:{key}"), r.rect);
                if r.clicked() {
                    picked = Some(key.clone());
                }
            }
        });
        r.response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, format!("{}: {name}", li_i18n::library::t(label))));
        register(ui.ctx(), format!("combo:{id}"), r.response.rect);
    });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nudges_are_a_sensible_step() {
        let spec = |id, min, max, step| ControlSpec {
            id,
            label: "",
            section: lightcraft_develop::controls::Section::Light,
            min,
            max,
            default: 0.0,
            step,
            decimals: 2,
            track: Track::Plain,
        };
        assert!((nudged(&spec("light.exposure", -5.0, 5.0, 0.01), 0.0, 1.0) - 0.05).abs() < 1e-9);
        assert_eq!(nudged(&spec("light.contrast", -100.0, 100.0, 1.0), 10.0, -5.0), 5.0);
        assert_eq!(nudged(&spec("wb.temp", 2000.0, 50000.0, 1.0), 6500.0, 1.0), 6550.0);
        assert_eq!(nudged(&spec("light.contrast", -100.0, 100.0, 1.0), 99.0, 5.0), 100.0, "clamped");
    }
}

#[cfg(test)]
mod access_tests {
    use super::*;

    #[test]
    fn colorprofiles_shared_picker_names_its_camera_field() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::theme::install_fonts(&ctx);
        let mut names = Vec::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.set_width(250.);
                let choices = vec![(String::new(), "Automatic".into(), "Camera Profiles".into())];
                profile_choice(ui, "testProfile", "Camera Profile", "", &choices);
            });
            out.textures_delta.clear();
            if let Some(update) = out.platform_output.accesskit_update.take() {
                names = update.nodes.iter().map(|(_, n)| n.label().unwrap_or_default().to_string()).collect();
            }
        }
        assert!(names.iter().any(|n| n == "Camera Profile: Automatic"), "{names:?}");
    }

    /// Screen readers see the custom widgets: sliders by control name with their value, buttons
    /// by label or tooltip.
    #[test]
    fn custom_widgets_describe_themselves() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.enable_accesskit();
        let spec = lightcraft_develop::controls::find("light.exposure").unwrap();
        let mut found = Vec::new();
        for _ in 0..3 {
            let out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let _ = slider(ui, spec, 0.5, true, None);
                let _ = text_button(ui, "auto", "Auto", false);
                let _ = icon_button(ui, "trash", Icon::Trash, vec2(24.0, 24.0), false, true, "Delete mask");
            });
            let mut out = out;
            out.textures_delta.clear();
            if let Some(update) = out.platform_output.accesskit_update.take() {
                found = update
                    .nodes
                    .iter()
                    .map(|(_, n)| (format!("{:?}", n.role()), n.label().unwrap_or_default().to_string(), n.numeric_value()))
                    .collect();
            }
        }
        let has = |role: &str, label: &str| found.iter().any(|(r, l, _)| r == role && l == label);
        assert!(has("Slider", "Exposure"), "{found:?}");
        assert!(found.iter().any(|(r, l, v)| r == "Slider" && l == "Exposure" && *v == Some(0.5)), "the slider's value: {found:?}");
        assert!(has("Button", "Auto"), "{found:?}");
        assert!(has("Button", "Delete mask"), "{found:?}");
    }
    /// The design-system widgets added for Develop's new UI describe themselves and keep their
    /// automation ids: colour samples, range strips, disclosure rows, `icon_button_as`.
    #[test]
    fn design_system_widgets_describe_themselves_and_register() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.enable_accesskit();
        let mut found = Vec::new();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let _ = color_swatch(ui, "pointColor:0", Color32::RED, vec2(22.0, 22.0), Some(true), "Point color sample 1");
                let _ = color_swatch(ui, "skinToneSwatch", Color32::from_rgb(220, 180, 160), vec2(32.0, 32.0), None, "Skin reference");
                let _ = range_strip(ui, "skinToneRange:Hue", "Hue", |f| Color32::from_gray((f * 255.0) as u8), &[(0.2, 0.4)]);
                let _ = disclosure(ui, "layerGlobalGroup", "5 whole-image tools", false, 200.0, true);
                let _ = icon_button_as(ui, "button:layerNew", Icon::Plus, vec2(24.0, 24.0), "New layer");
                ids = take_registry(ui.ctx()).into_iter().map(|(id, _)| id).collect();
            });
            out.textures_delta.clear();
            if let Some(update) = out.platform_output.accesskit_update.take() {
                found = update.nodes.iter().map(|(_, n)| (format!("{:?}", n.role()), n.label().unwrap_or_default().to_string())).collect();
            }
        }
        let has = |role: &str, label: &str| found.iter().any(|(r, l)| r == role && l == label);
        assert!(has("Button", "Point color sample 1"), "{found:?}");
        assert!(found.iter().any(|(_, l)| l == "Skin reference"), "{found:?}");
        assert!(found.iter().any(|(_, l)| l == "Hue range"), "{found:?}");
        assert!(found.iter().any(|(_, l)| l == "5 whole-image tools"), "{found:?}");
        assert!(has("Button", "New layer"), "{found:?}");
        for id in ["pointColor:0", "skinToneSwatch", "skinToneRange:Hue", "layerGlobalGroup", "button:layerNew"] {
            assert!(ids.iter().any(|i| i == id), "{id} registered: {ids:?}");
        }
    }

    #[test]
    fn library_navigation_numeric_editor_exposes_control_name_and_text() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.enable_accesskit();
        let spec = lightcraft_develop::controls::find("light.exposure").unwrap();
        let mut frame = 0;
        let mut value_rect = Rect::NOTHING;
        let mut seen = false;
        let mut run = |events: Vec<egui::Event>| {
            frame += 1;
            let raw = crate::headless::HeadlessView::raw_input(vec2(640., 480.), 1., frame as f64 / 60., events);
            let mut out = ctx.run_ui(raw, |ui| {
                let result = numeric_slider(ui, spec, 0.5, true, None, egui::Id::new("accessibility-owner"));
                assert!(result.value.is_none(), "typing alone must not commit");
                value_rect = take_registry(ui.ctx()).into_iter().find(|(name, _)| name == "value:light.exposure").unwrap().1;
            });
            if let Some(update) = out.platform_output.accesskit_update.take() {
                seen |= update.nodes.iter().any(|(id, n)| {
                    format!("{:?}", n.role()) == "TextInput" && n.label() == Some("Exposure") && n.value() == Some("1.25") && update.focus == *id
                });
            }
            out.textures_delta.clear();
            value_rect
        };
        run(vec![]);
        let rect = run(vec![]);
        let pos = rect.center();
        run(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
        ]);
        run(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
        run(vec![]);
        run(vec![egui::Event::Text("1.25".into())]);
        assert!(seen, "focused numeric TextInput must expose its control name and current draft to accessibility");
    }
}

/// Inset body for Develop sections and optional tool actions, using the shared panel metrics.
pub fn panel_content<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::NONE.inner_margin(crate::theme::PANEL_CONTENT_MARGIN).show(ui, body).inner
}

/// A Preferences choice menu, using the shared dropdown chrome and the installed theme.
/// Callers supply catalog/preset rows and keep their legacy `combo:` automation aliases.
pub fn choice_menu(ui: &mut Ui, id: &str, selected: &str, body: impl FnOnce(&mut Ui)) -> Response {
    let t = Tokens::get(ui.ctx());
    let response = dropdown(ui, id, selected, t.font(crate::theme::CONTROL_FONT_SIZE), t.text_label);
    egui::Popup::menu(&response).show(body);
    response
}

/// The shared Library modal shell, themed by the installed tokens. Unlike a dimmed Window,
/// egui Modal owns backdrop input and keeps custom controls clickable. Used by Preferences.
pub fn modal<R>(ctx: &egui::Context, id: &str, body: impl FnOnce(&mut Ui) -> R) -> egui::ModalResponse<R> {
    let area = egui::Modal::default_area(egui::Id::new(id)).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, Tokens::get(ctx).top_bar_h));
    egui::Modal::new(egui::Id::new(id)).area(area).show(ctx, body)
}

/// Preflight notice for on-device CPU AI. Shared by mask and batch analysis controls.
/// Returns whether the model fits the interactive CPU time limit; warnings use theme tokens.
pub fn cpu_estimate(ui: &mut egui::Ui, estimate: li_seg::estimate::Estimate) -> bool {
    let t = Tokens::get(ui.ctx());
    let color = if estimate.warning() { t.caution } else { t.text_dim };
    ui.label(egui::RichText::new(estimate.label()).color(color));
    estimate.allowed()
}

/// Total CPU estimate for cancellable background AI batches (Smart Sort and indexing).
/// The per-model inference guard still applies; a large batch shows a warning and can run.
pub fn cpu_batch_estimate(ui: &mut egui::Ui, estimate: li_seg::estimate::Estimate) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(estimate.batch_label()).color(if estimate.warning() { t.caution } else { t.text_dim }));
}

/// A reusable choice dropdown: themed shell and menu rows, translated accessible labels.
/// Import, backup preferences and proof sheet options use `dropdown:{id}` and `button:{id}-{i}`.
pub fn choice<T: PartialEq + Clone>(ui: &mut Ui, id: &str, value: &mut T, options: &[(T, &str)]) -> bool {
    let t = Tokens::get(ui.ctx());
    let label = options.iter().find(|(v, _)| *v == *value).map_or("Choose…", |(_, l)| *l);
    let response = dropdown(ui, id, li_i18n::library::t(label), t.font(12.), t.text);
    let mut changed = false;
    egui::Popup::menu(&response).show(|ui| {
        for (i, (v, label)) in options.iter().enumerate() {
            if text_button(ui, &format!("{id}-{i}"), label, *value == *v).clicked() {
                changed = *value != *v;
                *value = v.clone();
                ui.close();
            }
        }
    });
    changed
}

/// A labelled, width-adaptive text field for dialogs and settings. Registers `field:{id}`.
/// Used for PDF captions/destinations; retains the native theme, keyboard and screen-reader behavior.
pub fn text_field(ui: &mut Ui, id: &str, value: &mut String, label: &str) -> Response {
    // TextEdit's desired width excludes its frame padding. Bound the complete field so an
    // auto-sized window doesn't grow on every frame, moving targets between down and up.
    let padding = ui.spacing().button_padding;
    let width = (ui.available_width().min(ui.spacing().text_edit_width) - padding.x * 2.).max(0.);
    let response = ui.add(egui::TextEdit::singleline(value).margin(padding).desired_width(width).hint_text(li_i18n::library::t(label)));
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::text_edit(ui.is_enabled(), value.as_str(), value.as_str(), "");
        info.label = Some(li_i18n::library::t(label).to_string());
        info
    });
    register(ui.ctx(), format!("field:{id}"), response.rect);
    response
}

/// A text field with a labelled action, stacked when the row cannot fit. Import backup folders
/// and proof-sheet output use this so a narrow panel never clips its Choose button.
pub fn field_with_action(ui: &mut Ui, id: &str, value: &mut String, label: &str, action: &str, enabled: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let action_width = text_button_size(ui.painter().layout_no_wrap(li_i18n::library::t(action).to_string(), t.semibold(11.5), t.text).size().x).x;
    let fits = ui.available_width() >= ui.spacing().text_edit_width + action_width + ui.spacing().item_spacing.x;
    let mut clicked = false;
    if !fits {
        text_field(ui, id, value, label);
    }
    ui.horizontal(|ui| {
        if fits {
            text_field(ui, id, value, label);
        }
        clicked = ui
            .add_enabled_ui(enabled, |ui| text_button(ui, id, action, false))
            .inner
            .on_disabled_hover_text(li_i18n::library::t("Folder picker is not available here"))
            .clicked();
    });
    clicked
}
