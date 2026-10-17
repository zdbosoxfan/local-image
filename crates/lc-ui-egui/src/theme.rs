//! Design tokens (colours, sizes, fonts). Values measured from black-box observation of the
//! reference app's dark theme; everything is our own code.

use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId};

pub use li_ui_kit::library::{
    BUTTON_FONT_SIZE, CONTROL_FONT_SIZE, DEVELOP_INSET, DROPDOWN_ARROW_SPACE, FONT_SEMIBOLD, PANEL_CONTENT_MARGIN, PREFERENCES_BODY_WIDTH,
};

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

impl From<li_ui_kit::theme::Tokens> for Tokens {
    fn from(t: li_ui_kit::theme::Tokens) -> Self {
        Self {
            dark: t.library.dark,
            selection: t.library.selection,
            selection_text: t.library.selection_text,
            chrome: t.chrome,
            canvas: t.canvas,
            grid_bg: t.library.grid_bg,
            cell: t.library.cell,
            cell_selected: t.library.cell_selected,
            divider: t.library.divider,
            inset: t.library.inset,
            field: t.field,
            field_border: t.field_border,
            button: t.library.button,
            button_border: t.library.button_border,
            hover: t.hover,
            pressed: t.pressed,
            tool_active: t.library.tool_active,
            text: t.text,
            text_label: t.library.text_label,
            text_dim: t.text_dim,
            text_disabled: t.library.text_disabled,
            icon: t.icon,
            track: t.library.track,
            thumb: t.library.thumb,
            thumb_hover: t.library.thumb_hover,
            accent: t.accent,
            star: t.library.star,
            pick: t.library.pick,
            reject: t.library.reject,
            caution: t.library.caution,
            mask_overlay: t.library.mask_overlay,
            overlay_bg: t.library.overlay_bg,
            overlay_fg: t.library.overlay_fg,
            overlay_reject: t.library.overlay_reject,
            top_bar_h: t.library.metrics.top_bar_h,
            bottom_bar_h: t.library.metrics.bottom_bar_h,
            panel_w: t.library.metrics.panel_w,
            strip_w: t.library.metrics.strip_w,
            slider_row_h: t.library.metrics.slider_row_h,
            section_h: t.library.metrics.section_h,
            film_h: t.library.metrics.film_h,
            panel_inset_left: t.library.metrics.panel_inset_left,
            panel_inset_right: t.library.metrics.panel_inset_right,
        }
    }
}
impl From<Tokens> for li_ui_kit::theme::Tokens {
    fn from(old: Tokens) -> Self {
        let mut t = li_ui_kit::library::standalone();
        t.library.dark = old.dark;
        t.library.selection = old.selection;
        t.library.selection_text = old.selection_text;
        t.chrome = old.chrome;
        t.canvas = old.canvas;
        t.library.grid_bg = old.grid_bg;
        t.library.cell = old.cell;
        t.library.cell_selected = old.cell_selected;
        t.library.divider = old.divider;
        t.library.inset = old.inset;
        t.field = old.field;
        t.field_border = old.field_border;
        t.library.button = old.button;
        t.library.button_border = old.button_border;
        t.hover = old.hover;
        t.pressed = old.pressed;
        t.library.tool_active = old.tool_active;
        t.text = old.text;
        t.library.text_label = old.text_label;
        t.text_dim = old.text_dim;
        t.library.text_disabled = old.text_disabled;
        t.icon = old.icon;
        t.library.track = old.track;
        t.library.thumb = old.thumb;
        t.library.thumb_hover = old.thumb_hover;
        t.accent = old.accent;
        t.library.star = old.star;
        t.library.pick = old.pick;
        t.library.reject = old.reject;
        t.library.caution = old.caution;
        t.library.mask_overlay = old.mask_overlay;
        t.library.overlay_bg = old.overlay_bg;
        t.library.overlay_fg = old.overlay_fg;
        t.library.overlay_reject = old.overlay_reject;
        t.library.metrics.top_bar_h = old.top_bar_h;
        t.library.metrics.bottom_bar_h = old.bottom_bar_h;
        t.library.metrics.panel_w = old.panel_w;
        t.library.metrics.strip_w = old.strip_w;
        t.library.metrics.slider_row_h = old.slider_row_h;
        t.library.metrics.section_h = old.section_h;
        t.library.metrics.film_h = old.film_h;
        t.library.metrics.panel_inset_left = old.panel_inset_left;
        t.library.metrics.panel_inset_right = old.panel_inset_right;
        t
    }
}
impl Default for Tokens {
    fn default() -> Self {
        li_ui_kit::library::standalone().into()
    }
}
impl Tokens {
    pub fn get(ctx: &egui::Context) -> Self {
        li_ui_kit::library::get(ctx).into()
    }
    pub fn curve_color(&self, channel: &str) -> Color32 {
        li_ui_kit::library::curve_color(&(*self).into(), channel)
    }
    pub fn font(&self, size: f32) -> FontId {
        FontId::proportional(size)
    }
    pub fn semibold(&self, size: f32) -> FontId {
        li_ui_kit::fonts::semibold(size)
    }
}
pub use li_ui_kit::library::{apply, readable_text, text_on};
pub fn apply_tokens(ctx: &egui::Context, t: Tokens) {
    li_ui_kit::library::apply_tokens(ctx, t.into());
}
pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions(lightcraft_engine::CRAFT_FONTS));
}

/// Inter (bundled) for Latin text, egui's default fonts, then the craft-fonts CJK faces as the last
/// fallback of every family — the active language's own script first, so shared Han characters keep
/// that language's forms. Without craft-fonts (`craft` empty) CJK text has no glyphs and shows as
/// boxes.
pub fn font_definitions(craft: &'static [lightcraft_engine::CraftFont]) -> FontDefinitions {
    let mut fonts = li_ui_kit::fonts::definitions(li_ui_kit::fonts::Profile::Library);
    // Craft-fonts faces in preference order for a family drawn in `style`.
    let fallback = |style: &str| {
        lightcraft_engine::fonts::cjk_fallback(craft, li_i18n::current().script(), style).into_iter().map(craft_font_name).collect::<Vec<String>>()
    };
    let (regular, bold) = (fallback("Regular"), fallback("Bold"));
    for name in regular.iter().chain(bold.iter()) {
        if let Some(font) = craft.iter().find(|font| &craft_font_name(font) == name) {
            fonts.font_data.insert(name.clone(), Arc::new(FontData::from_static(font.bytes)));
        }
    }
    fonts.families.entry(FontFamily::Proportional).or_default().extend(regular);
    fonts.families.entry(FontFamily::Name(FONT_SEMIBOLD.into())).or_default().extend(bold);
    fonts.families.entry(FontFamily::Monospace).or_default().extend(fallback("Regular"));
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

/// Host palette from the single shared theme authority.
pub fn host_palette(kind: li_ui_kit::theme::ThemeKind) -> Tokens {
    li_ui_kit::library::for_kind(kind).into()
}
