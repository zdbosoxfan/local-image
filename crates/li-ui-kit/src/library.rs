//! Library profile of the canonical token union; standalone defaults retain LightCraft's look.
use crate::theme::{LibraryRoles, Tokens};
use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, Visuals};

pub fn get(ctx: &egui::Context) -> Tokens {
    ctx.data(|d| d.get_temp::<Tokens>(egui::Id::NULL)).unwrap_or_else(standalone)
}
pub fn standalone() -> Tokens {
    let mut t = Tokens::for_kind(crate::theme::ThemeKind::Studio);
    t.library = LibraryRoles::default();
    t.chrome = Color32::from_rgb(0x2d, 0x2d, 0x2d);
    t.canvas = Color32::from_rgb(0x1c, 0x1c, 0x1c);
    t.field = Color32::from_rgb(0x22, 0x22, 0x22);
    t.field_border = Color32::from_rgb(0x3c, 0x3c, 0x3c);
    t.hover = Color32::from_rgb(0x3a, 0x3a, 0x3a);
    t.pressed = Color32::from_rgb(0x3f, 0x3f, 0x3f);
    t.text = Color32::from_rgb(0xe2, 0xe2, 0xe2);
    t.text_dim = Color32::from_rgb(0x8e, 0x8e, 0x8e);
    t.icon = Color32::from_rgb(0x9a, 0x9a, 0x9a);
    t.accent = Color32::from_rgb(0x01, 0x65, 0xdd);
    t
}
/// Derive Library-specific aliases once when constructing a palette.
pub(crate) fn map_roles(t: &mut Tokens) {
    let palette = *t;
    let text = readable_text(palette.text, palette.pressed);
    t.library.dark = palette.dark();
    t.library.selection = palette.pressed;
    t.library.selection_text = text_on(palette.pressed);
    t.library.grid_bg = palette.canvas;
    t.library.cell = palette.card;
    t.library.cell_selected = palette.hover;
    t.library.divider = palette.separator;
    t.library.inset = palette.dock;
    t.library.button = palette.card;
    t.library.button_border = palette.card_border;
    t.library.tool_active = palette.pressed;
    t.library.text_label = text;
    t.library.text_disabled = palette.text_faint;
    t.library.track = palette.text_dim;
    t.library.thumb = palette.icon;
    t.library.thumb_hover = text;
    t.library.star = text;
    t.library.pick = text;
    t.library.reject = readable_text(palette.danger, palette.chrome);
    t.library.caution = readable_text(palette.warning, palette.chrome);
    // Image badges keep their existing amber when the light chrome warning darkens.
    t.library.overlay_caution =
        if palette.kind == crate::theme::ThemeKind::StudioLight { readable_text(Color32::from_rgb(190, 130, 20), palette.chrome) } else { t.library.caution };
}
/// The hosted Library profile retains metrics and all pre-existing contrast adjustments.
pub fn for_kind(kind: crate::theme::ThemeKind) -> Tokens {
    let mut t = Tokens::for_kind(kind);
    let palette = t;
    let text = readable_text(palette.text, palette.pressed);
    t.chrome = palette.chrome;
    t.canvas = palette.canvas;
    t.field = palette.field;
    t.field_border = palette.field_border;
    t.hover = palette.hover;
    t.pressed = palette.pressed;
    t.text_dim = readable_text(palette.text_dim, palette.chrome);
    t.icon = palette.icon;
    t.accent = palette.accent;
    t.text = text;
    t
}
pub fn apply(ctx: &egui::Context) {
    apply_tokens(ctx, standalone());
}

/// Apply a host palette while preserving Library layout metrics and font ownership.
pub fn apply_tokens(ctx: &egui::Context, t: Tokens) {
    crate::icons::install(ctx);
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, t));
    let mut v = if t.library.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.chrome;
    v.window_fill = t.chrome;
    v.extreme_bg_color = t.field;
    v.faint_bg_color = t.library.inset;
    v.window_stroke = Stroke::new(1.0, t.library.button_border);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = t.library.selection;
    v.selection.stroke = Stroke::new(1.0, t.library.selection_text);
    v.override_text_color = Some(t.library.text_label);
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(140) };
    v.window_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 30, spread: 0, color: Color32::from_black_alpha(160) };
    let w = &mut v.widgets;
    for (wv, fill) in
        [(&mut w.noninteractive, t.chrome), (&mut w.inactive, t.library.button), (&mut w.hovered, t.hover), (&mut w.active, t.pressed), (&mut w.open, t.hover)]
    {
        wv.bg_fill = fill;
        wv.weak_bg_fill = fill;
        wv.corner_radius = CornerRadius::same(4);
        wv.fg_stroke = Stroke::new(1.0, t.library.text_label);
    }
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.library.divider);
    w.inactive.bg_stroke = Stroke::new(1.0, t.library.button_border);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.0));
        s.text_styles.insert(egui::TextStyle::Heading, FontId::new(16.0, FontFamily::Name(FONT_SEMIBOLD.into())));
        s.animation_time = 0.08;
    });
}

fn luminance(color: Color32) -> f32 {
    li_color::luminance_709([color.r(), color.g(), color.b()].map(|v| li_color::transfer::srgb_to_linear(f32::from(v) / 255.0)))
}
/// Choose a readable foreground on an opaque UI fill, not a document/image colour.
pub fn text_on(background: Color32) -> Color32 {
    if luminance(background) > 0.179 { Color32::BLACK } else { Color32::WHITE }
}
/// Retain a semantic text hue when legible, otherwise move it toward the stronger
/// black/white foreground. Called only when constructing a host palette.
pub fn readable_text(foreground: Color32, background: Color32) -> Color32 {
    let back = luminance(background);
    let target = text_on(background);
    for step in 0..=16 {
        let mix = |a: u8, b: u8| ((u16::from(a) * (16 - step) + u16::from(b) * step) / 16) as u8;
        let candidate = Color32::from_rgb(mix(foreground.r(), target.r()), mix(foreground.g(), target.g()), mix(foreground.b(), target.b()));
        let front = luminance(candidate);
        if (front.max(back) + 0.05) / (front.min(back) + 0.05) >= 4.5 {
            return candidate;
        }
    }
    target
}

pub fn curve_color(t: &Tokens, channel: &str) -> Color32 {
    let color = match channel {
        "red" => Color32::from_rgb(0xdd, 0x33, 0x33),
        "green" => Color32::from_rgb(0x33, 0xbb, 0x55),
        "blue" => Color32::from_rgb(0x33, 0x77, 0xee),
        _ => t.text,
    };
    readable_text(color, t.library.grid_bg)
}

/// Standard Develop section content inset.
pub const PANEL_CONTENT_MARGIN: egui::Margin = egui::Margin { left: 24, right: 22, top: 4, bottom: 4 };

/// Existing Library Preferences body width, shared by standalone and hosted rows.
pub const PREFERENCES_BODY_WIDTH: f32 = 560.0;
pub const CONTROL_FONT_SIZE: f32 = 13.0;
pub const BUTTON_FONT_SIZE: f32 = 11.5;
pub const DROPDOWN_ARROW_SPACE: f32 = 20.0;

pub const FONT_SEMIBOLD: &str = "semibold";
/// Shared inset for controls in a Develop section, matching the inherited panel grammar.
pub const DEVELOP_INSET: egui::Margin = PANEL_CONTENT_MARGIN;
