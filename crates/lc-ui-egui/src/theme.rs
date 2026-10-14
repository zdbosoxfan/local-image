//! Design tokens (colours, sizes, fonts). Values measured from black-box observation of the
//! reference app's dark theme (see plan/lightroom/10-observed-ui.md); everything is our own code.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};

/// Standard Develop section content inset.
pub const PANEL_CONTENT_MARGIN: egui::Margin = egui::Margin { left: 24, right: 22, top: 4, bottom: 4 };

/// Existing Library Preferences body width, shared by standalone and hosted rows.
pub const PREFERENCES_BODY_WIDTH: f32 = 560.0;
pub const CONTROL_FONT_SIZE: f32 = 13.0;
pub const BUTTON_FONT_SIZE: f32 = 11.5;
pub const DROPDOWN_ARROW_SPACE: f32 = 20.0;

pub const FONT_SEMIBOLD: &str = "semibold";
/// Shared inset for controls in a Develop section, matching the inherited panel grammar.
pub const DEVELOP_INSET: egui::Margin = egui::Margin { left: 24, right: 22, top: 4, bottom: 4 };

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// Visual defaults for the palette; supplied by a host without persisting another theme.
    pub dark: bool,
    pub selection: Color32,
    pub selection_text: Color32,
    /// Top bar, side panels, bottom bar, tool strip.
    pub chrome: Color32,
    /// Photo canvas (detail view) and filmstrip.
    pub canvas: Color32,
    /// Grid background behind the cells.
    pub grid_bg: Color32,
    pub cell: Color32,
    pub cell_selected: Color32,
    pub divider: Color32,
    pub inset: Color32,
    pub field: Color32,
    pub field_border: Color32,
    pub button: Color32,
    pub button_border: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub tool_active: Color32,
    pub text: Color32,
    pub text_label: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    /// Icon glyphs in the chrome.
    pub icon: Color32,
    pub track: Color32,
    pub thumb: Color32,
    pub thumb_hover: Color32,
    pub accent: Color32,
    pub star: Color32,
    pub pick: Color32,
    pub reject: Color32,
    /// Cautionary notices (e.g. a raw shown from its embedded preview): a muted amber.
    pub caution: Color32,
    pub mask_overlay: Color32,
    /// Image badges sit over arbitrary pixels, independently of light/dark chrome.
    pub overlay_bg: Color32,
    pub overlay_fg: Color32,
    pub overlay_reject: Color32,
    // metrics (points)
    pub top_bar_h: f32,
    pub bottom_bar_h: f32,
    pub panel_w: f32,
    pub strip_w: f32,
    pub slider_row_h: f32,
    pub section_h: f32,
    pub film_h: f32,
    /// Develop content inset, shared by profile choices.
    pub panel_inset_left: i8,
    pub panel_inset_right: i8,
}

impl Default for Tokens {
    fn default() -> Self {
        Tokens {
            dark: true,
            selection: Color32::from_rgb(0x01, 0x65, 0xdd).gamma_multiply(0.6),
            selection_text: Color32::from_rgb(0x01, 0x65, 0xdd),
            chrome: Color32::from_rgb(0x2d, 0x2d, 0x2d),
            canvas: Color32::from_rgb(0x1c, 0x1c, 0x1c),
            grid_bg: Color32::from_rgb(0x0f, 0x0f, 0x0f),
            cell: Color32::from_rgb(0x1c, 0x1c, 0x1c),
            cell_selected: Color32::from_rgb(0x2d, 0x2d, 0x2d),
            divider: Color32::from_rgb(0x1c, 0x1c, 0x1c),
            inset: Color32::from_rgb(0x23, 0x23, 0x23),
            field: Color32::from_rgb(0x22, 0x22, 0x22),
            field_border: Color32::from_rgb(0x3c, 0x3c, 0x3c),
            button: Color32::from_rgb(0x24, 0x24, 0x24),
            button_border: Color32::from_rgb(0x4c, 0x4c, 0x4c),
            hover: Color32::from_rgb(0x3a, 0x3a, 0x3a),
            pressed: Color32::from_rgb(0x3f, 0x3f, 0x3f),
            tool_active: Color32::from_rgb(0x3f, 0x3f, 0x3f),
            text: Color32::from_rgb(0xe2, 0xe2, 0xe2),
            text_label: Color32::from_rgb(0xbc, 0xbc, 0xbc),
            text_dim: Color32::from_rgb(0x8e, 0x8e, 0x8e),
            text_disabled: Color32::from_rgb(0x5c, 0x5c, 0x5c),
            icon: Color32::from_rgb(0x9a, 0x9a, 0x9a),
            track: Color32::from_rgb(0x5a, 0x5a, 0x5a),
            thumb: Color32::from_rgb(0xa0, 0xa0, 0xa0),
            thumb_hover: Color32::from_rgb(0xe0, 0xe0, 0xe0),
            accent: Color32::from_rgb(0x01, 0x65, 0xdd),
            star: Color32::from_rgb(0xd8, 0xd8, 0xd8),
            pick: Color32::from_rgb(0xf0, 0xf0, 0xf0),
            reject: Color32::from_rgb(0xe0, 0x4a, 0x4a),
            caution: Color32::from_rgb(0xe3, 0xa8, 0x3c),
            mask_overlay: Color32::from_rgba_unmultiplied(0xe0, 0x20, 0x30, 110),
            overlay_bg: Color32::from_gray(26),
            overlay_fg: Color32::from_gray(235),
            overlay_reject: Color32::from_rgb(255, 120, 120),
            top_bar_h: 42.0,
            bottom_bar_h: 48.0,
            panel_w: 270.0,
            strip_w: 48.0,
            slider_row_h: 45.0,
            section_h: 57.0,
            film_h: 142.0,
            panel_inset_left: 24,
            panel_inset_right: 22,
        }
    }
}

impl Tokens {
    pub fn get(ctx: &egui::Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(egui::Id::NULL)).unwrap_or_default()
    }
    /// Curve channels retain their hue with an AA-readable stroke on the current plot fill.
    /// Derive this from host tokens so light palettes never inherit dark-theme grey strokes.
    pub fn curve_color(&self, channel: &str) -> Color32 {
        let color = match channel {
            "red" => Color32::from_rgb(0xdd, 0x33, 0x33),
            "green" => Color32::from_rgb(0x33, 0xbb, 0x55),
            "blue" => Color32::from_rgb(0x33, 0x77, 0xee),
            _ => self.text,
        };
        readable_text(color, self.grid_bg)
    }
    pub fn font(&self, size: f32) -> FontId {
        FontId::proportional(size)
    }
    pub fn semibold(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Name(FONT_SEMIBOLD.into()))
    }
}

pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions(lightcraft_engine::CRAFT_FONTS));
}

/// Inter (bundled) for Latin text, egui's default fonts, then the craft-fonts CJK faces as the last
/// fallback of every family — the active language's own script first, so shared Han characters keep
/// that language's forms. Without craft-fonts (`craft` empty) CJK text has no glyphs and shows as
/// boxes.
pub fn font_definitions(craft: &'static [lightcraft_engine::CraftFont]) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("Inter".into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Inter-Regular.ttf"))));
    fonts.font_data.insert("Inter-SemiBold".into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"))));
    fonts
        .font_data
        .insert("JetBrainsMono".into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"))));
    // Craft-fonts faces in preference order for a family drawn in `style`.
    let fallback = |style: &str| {
        lightcraft_engine::fonts::cjk_fallback(craft, crate::i18n::language().script(), style)
            .into_iter()
            .map(craft_font_name)
            .collect::<Vec<String>>()
    };
    let (regular, bold) = (fallback("Regular"), fallback("Bold"));
    for name in regular.iter().chain(bold.iter()) {
        if let Some(font) = craft.iter().find(|font| &craft_font_name(font) == name) {
            fonts.font_data.insert(name.clone(), Arc::new(FontData::from_static(font.bytes)));
        }
    }
    let mut defaults: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    // Inter/default egui fonts omit the small disclosure triangles used in labels.
    // Reuse the bundled mono face only after those fonts, preserving Latin metrics.
    defaults.push("JetBrainsMono".into());
    let mut prop = vec!["Inter".to_string()];
    prop.extend(defaults.iter().cloned());
    prop.extend(regular);
    fonts.families.insert(FontFamily::Proportional, prop);
    let mut semi = vec!["Inter-SemiBold".to_string()];
    semi.extend(defaults);
    semi.extend(bold);
    fonts.families.insert(FontFamily::Name(FONT_SEMIBOLD.into()), semi);
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.push("JetBrainsMono".into());
    mono.extend(fallback("Regular"));
    fonts
}

/// The embedded font families for the About box: Inter, plus the craft-fonts families when built
/// with them.
pub fn font_credits() -> String {
    let mut families = vec!["Inter"];
    for f in lightcraft_engine::CRAFT_FONTS {
        if !families.contains(&f.family) {
            families.push(f.family);
        }
    }
    families.join(" / ")
}

fn craft_font_name(f: &lightcraft_engine::CraftFont) -> String {
    format!("craft-fonts {} {}", f.family, f.style)
}

pub fn apply(ctx: &egui::Context) {
    apply_tokens(ctx, Tokens::default());
}

/// Apply a host palette while preserving Library layout metrics and font ownership.
pub fn apply_tokens(ctx: &egui::Context, t: Tokens) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, t));
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.chrome;
    v.window_fill = t.chrome;
    v.extreme_bg_color = t.field;
    v.faint_bg_color = t.inset;
    v.window_stroke = Stroke::new(1.0, t.button_border);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = t.selection;
    v.selection.stroke = Stroke::new(1.0, t.selection_text);
    v.override_text_color = Some(t.text_label);
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(140) };
    v.window_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 30, spread: 0, color: Color32::from_black_alpha(160) };
    let w = &mut v.widgets;
    for (wv, fill) in [
        (&mut w.noninteractive, t.chrome),
        (&mut w.inactive, t.button),
        (&mut w.hovered, t.hover),
        (&mut w.active, t.pressed),
        (&mut w.open, t.hover),
    ] {
        wv.bg_fill = fill;
        wv.weak_bg_fill = fill;
        wv.corner_radius = CornerRadius::same(4);
        wv.fg_stroke = Stroke::new(1.0, t.text_label);
    }
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.divider);
    w.inactive.bg_stroke = Stroke::new(1.0, t.button_border);
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
