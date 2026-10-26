//! Design system: themes, colour tokens, radii, typography.
//!
//! - **Pro Medium** (default): neutral medium-grey Photoshop-style chrome.
//! - **Studio**: near-black surfaces, rounded cards, Inter + JetBrains Mono, soft violet
//!   accent. Modelled on the look of modern pro editors (such as Photoshop 2025).
//! - **Studio Light**: the same system on light surfaces.
//! - **Classic**: a deliberately Windows-2000-era look (grey bevels, square corners, navy selection)
//!   for people who prefer it.
//!
//! Widgets read [`Tokens::get`] instead of hard-coding colours, so every theme applies everywhere.

use egui::{Color32, CornerRadius, FontId, Stroke, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeKind {
    /// Photoshop-style Spectrum dark: flat charcoal panels, tab strips, blue accents (darkest brightness).
    Pro,
    /// Photoshop's default (second) interface brightness: #535353 panels, #282828 canvas (default).
    #[default]
    ProMedium,
    Studio,
    StudioLight,
    Classic,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 5] = [ThemeKind::Pro, ThemeKind::ProMedium, ThemeKind::Studio, ThemeKind::StudioLight, ThemeKind::Classic];
    pub fn label(self) -> &'static str {
        match self {
            ThemeKind::Pro => "Pro (Dark)",
            ThemeKind::ProMedium => "Pro (Medium Gray)",
            ThemeKind::Studio => "Studio (Dark)",
            ThemeKind::StudioLight => "Studio (Light)",
            ThemeKind::Classic => "Classic",
        }
    }
    /// The canonical name: `ui.set {theme}` accepts it and the Window › Theme commands are
    /// `window.theme.<id>`.
    pub fn id(self) -> &'static str {
        match self {
            ThemeKind::Pro => "pro",
            ThemeKind::ProMedium => "proMedium",
            ThemeKind::Studio => "studio",
            ThemeKind::StudioLight => "studioLight",
            ThemeKind::Classic => "classic",
        }
    }
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|k| *k == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().replace([' ', '_', '-', '(', ')'], "").as_str() {
            "pro" | "prodark" | "photoshop" | "dark" => Some(ThemeKind::Pro),
            "promedium" | "promediumgray" | "medium" | "mediumgray" => Some(ThemeKind::ProMedium),
            "studio" | "studiodark" => Some(ThemeKind::Studio),
            "studiolight" | "light" => Some(ThemeKind::StudioLight),
            "classic" | "win2000" | "retro" => Some(ThemeKind::Classic),
            _ => None,
        }
    }
}

/// Colour and shape tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// Library/Develop roles and layout; one theme authority serves both profiles.
    pub library: LibraryRoles,
    pub kind: ThemeKind,
    /// Window chrome (title bar, toolbars).
    pub chrome: Color32,
    /// Canvas surround.
    pub canvas: Color32,
    /// Dot colour of the canvas grid pattern.
    pub canvas_dot: Color32,
    /// Dock background (behind cards).
    pub dock: Color32,
    /// Cards / panels.
    pub card: Color32,
    pub card_border: Color32,
    /// Inputs, fields, dropdowns.
    pub field: Color32,
    pub field_border: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub icon: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_border: Color32,
    pub accent_text: Color32,
    pub separator: Color32,
    pub shadow: Color32,
    pub primary_bg: Color32,
    pub primary_text: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub radius_sm: f32,
    pub radius: f32,
    pub radius_lg: f32,
    /// Classic theme draws 3D bevels instead of flat fills.
    pub bevel: bool,
    /// Pro (Photoshop-grammar) layout: tab strips, flat panels, checkboxes, pill buttons.
    pub pro: bool,
    /// Panel tab-strip background (Pro).
    pub tab_strip: Color32,
    /// Selected list row (layers, history).
    pub row_selected: Color32,
    /// Scope plot background (histogram, vectorscope, clipping diagnostics).
    pub histogram_bg: Color32,
    /// Channel intensity of scope outlines and hue markers.
    pub histogram_level: u8,
    /// The custom title bar's Close button while hovered (Windows' red), and its glyph.
    pub caption_close: Color32,
    pub caption_close_text: Color32,
}

impl Tokens {
    pub fn for_kind(kind: ThemeKind) -> Self {
        let mut tokens = match kind {
            // Sampled from Photoshop 2026's default brightness (raw display values).
            ThemeKind::ProMedium => Tokens {
                library: LibraryRoles::default(),
                kind,
                chrome: Color32::from_rgb(83, 83, 83),
                dock: Color32::from_rgb(66, 66, 66),
                card: Color32::from_rgb(83, 83, 83),
                card_border: Color32::from_rgb(66, 66, 66),
                field: Color32::from_rgb(69, 69, 69),
                field_border: Color32::from_rgb(104, 104, 104),
                hover: Color32::from_rgb(98, 98, 98),
                pressed: Color32::from_rgb(112, 112, 112),
                text: Color32::from_rgb(238, 238, 238),
                text_dim: Color32::from_rgb(212, 212, 212),
                text_faint: Color32::from_rgb(160, 160, 160),
                icon: Color32::from_rgb(226, 226, 226),
                accent_soft: Color32::from_rgb(110, 110, 110),
                separator: Color32::from_rgb(62, 62, 62),
                tab_strip: Color32::from_rgb(74, 74, 74),
                row_selected: Color32::from_rgb(107, 107, 107),
                ..Tokens::for_kind(ThemeKind::Pro)
            },
            ThemeKind::Pro => Tokens {
                library: LibraryRoles::default(),
                kind,
                chrome: Color32::from_rgb(50, 50, 50),
                canvas: Color32::from_rgb(40, 40, 40),
                canvas_dot: Color32::from_rgb(40, 40, 40),
                dock: Color32::from_rgb(30, 30, 30),
                card: Color32::from_rgb(50, 50, 50),
                card_border: Color32::from_rgb(30, 30, 30),
                field: Color32::from_rgb(36, 36, 36),
                field_border: Color32::from_rgb(74, 74, 74),
                hover: Color32::from_rgb(66, 66, 66),
                pressed: Color32::from_rgb(78, 78, 78),
                text: Color32::from_rgb(222, 222, 222),
                text_dim: Color32::from_rgb(178, 178, 178),
                text_faint: Color32::from_rgb(128, 128, 128),
                icon: Color32::from_rgb(200, 200, 200),
                accent: Color32::from_rgb(55, 142, 240),
                accent_soft: Color32::from_rgb(78, 78, 78),
                accent_border: Color32::from_rgb(55, 142, 240),
                accent_text: Color32::WHITE,
                separator: Color32::from_rgb(30, 30, 30),
                shadow: Color32::from_black_alpha(150),
                primary_bg: Color32::from_rgb(55, 142, 240),
                primary_text: Color32::WHITE,
                danger: Color32::from_rgb(236, 91, 98),
                warning: Color32::from_rgb(232, 176, 70),
                radius_sm: 3.0,
                radius: 4.0,
                radius_lg: 6.0,
                bevel: false,
                pro: true,
                tab_strip: Color32::from_rgb(38, 38, 38),
                row_selected: Color32::from_rgb(82, 82, 82),
                histogram_bg: Color32::from_rgb(40, 40, 40),
                histogram_level: 225,
                caption_close: Color32::from_rgb(196, 43, 28),
                caption_close_text: Color32::WHITE,
            },
            ThemeKind::Studio => Tokens {
                library: LibraryRoles::default(),
                kind,
                chrome: Color32::from_rgb(20, 20, 21),
                canvas: Color32::from_rgb(14, 14, 15),
                canvas_dot: Color32::from_rgb(46, 46, 50),
                dock: Color32::from_rgb(17, 17, 18),
                card: Color32::from_rgb(26, 26, 28),
                card_border: Color32::from_rgb(40, 40, 44),
                field: Color32::from_rgb(35, 35, 38),
                field_border: Color32::from_rgb(52, 52, 57),
                hover: Color32::from_rgb(44, 44, 48),
                pressed: Color32::from_rgb(56, 56, 62),
                text: Color32::from_rgb(236, 236, 240),
                text_dim: Color32::from_rgb(150, 150, 158),
                text_faint: Color32::from_rgb(96, 96, 104),
                icon: Color32::from_rgb(196, 196, 204),
                accent: Color32::from_rgb(139, 124, 246),
                accent_soft: Color32::from_rgba_unmultiplied(139, 124, 246, 46),
                accent_border: Color32::from_rgba_unmultiplied(160, 148, 255, 110),
                accent_text: Color32::from_rgb(214, 208, 255),
                separator: Color32::from_rgb(38, 38, 42),
                shadow: Color32::from_black_alpha(140),
                primary_bg: Color32::from_rgb(246, 246, 248),
                primary_text: Color32::from_rgb(12, 12, 14),
                danger: Color32::from_rgb(240, 96, 96),
                warning: Color32::from_rgb(240, 190, 90),
                radius_sm: 6.0,
                radius: 8.0,
                radius_lg: 12.0,
                bevel: false,
                pro: false,
                tab_strip: Color32::TRANSPARENT,
                row_selected: Color32::TRANSPARENT,
                histogram_bg: Color32::from_rgb(14, 14, 15),
                histogram_level: 225,
                caption_close: Color32::from_rgb(196, 43, 28),
                caption_close_text: Color32::WHITE,
            },
            ThemeKind::StudioLight => Tokens {
                library: LibraryRoles::default(),
                kind,
                chrome: Color32::from_rgb(246, 246, 248),
                canvas: Color32::from_rgb(226, 226, 230),
                canvas_dot: Color32::from_rgb(200, 200, 206),
                dock: Color32::from_rgb(240, 240, 243),
                card: Color32::from_rgb(252, 252, 253),
                card_border: Color32::from_rgb(222, 222, 228),
                field: Color32::from_rgb(242, 242, 245),
                field_border: Color32::from_rgb(214, 214, 220),
                hover: Color32::from_rgb(232, 232, 237),
                pressed: Color32::from_rgb(220, 220, 226),
                text: Color32::from_rgb(24, 24, 28),
                text_dim: Color32::from_rgb(96, 96, 106),
                text_faint: Color32::from_rgb(150, 150, 160),
                icon: Color32::from_rgb(60, 60, 68),
                accent: Color32::from_rgb(108, 92, 231),
                accent_soft: Color32::from_rgba_unmultiplied(108, 92, 231, 36),
                accent_border: Color32::from_rgba_unmultiplied(108, 92, 231, 120),
                accent_text: Color32::from_rgb(80, 64, 200),
                separator: Color32::from_rgb(226, 226, 232),
                shadow: Color32::from_black_alpha(50),
                primary_bg: Color32::from_rgb(20, 20, 24),
                primary_text: Color32::from_rgb(250, 250, 252),
                danger: Color32::from_rgb(210, 60, 60),
                warning: Color32::from_rgb(190, 130, 20),
                radius_sm: 6.0,
                radius: 8.0,
                radius_lg: 12.0,
                bevel: false,
                pro: false,
                tab_strip: Color32::TRANSPARENT,
                row_selected: Color32::TRANSPARENT,
                histogram_bg: Color32::from_gray(40),
                histogram_level: 240,
                caption_close: Color32::from_rgb(196, 43, 28),
                caption_close_text: Color32::WHITE,
            },
            ThemeKind::Classic => Tokens {
                library: LibraryRoles::default(),
                kind,
                chrome: Color32::from_rgb(212, 208, 200),
                canvas: Color32::from_rgb(128, 128, 128),
                canvas_dot: Color32::from_rgb(128, 128, 128),
                dock: Color32::from_rgb(212, 208, 200),
                card: Color32::from_rgb(212, 208, 200),
                card_border: Color32::from_rgb(128, 128, 128),
                field: Color32::WHITE,
                field_border: Color32::from_rgb(128, 128, 128),
                hover: Color32::from_rgb(226, 222, 214),
                pressed: Color32::from_rgb(190, 186, 178),
                text: Color32::BLACK,
                text_dim: Color32::from_rgb(64, 64, 64),
                text_faint: Color32::from_rgb(128, 128, 128),
                icon: Color32::BLACK,
                accent: Color32::from_rgb(10, 36, 106),
                accent_soft: Color32::from_rgb(10, 36, 106),
                accent_border: Color32::from_rgb(10, 36, 106),
                accent_text: Color32::WHITE,
                separator: Color32::from_rgb(128, 128, 128),
                shadow: Color32::from_black_alpha(0),
                primary_bg: Color32::from_rgb(212, 208, 200),
                primary_text: Color32::BLACK,
                danger: Color32::from_rgb(160, 0, 0),
                warning: Color32::from_rgb(128, 96, 0),
                radius_sm: 0.0,
                radius: 0.0,
                radius_lg: 0.0,
                bevel: true,
                pro: false,
                tab_strip: Color32::from_rgb(212, 208, 200),
                row_selected: Color32::from_rgb(10, 36, 106),
                histogram_bg: Color32::from_gray(40),
                histogram_level: 240,
                caption_close: Color32::from_rgb(196, 43, 28),
                caption_close_text: Color32::WHITE,
            },
        };
        crate::library::map_roles(&mut tokens);
        tokens
    }

    /// Tokens for the active theme (stored in egui's context data by [`apply`]).
    pub fn get(ctx: &egui::Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(egui::Id::new("photocraft-theme"))).unwrap_or_else(|| Tokens::for_kind(ThemeKind::Studio))
    }

    /// Thin RGB outlines; the filled bands use the same hues with subdued coverage.
    pub fn histogram_color(&self, mask: u8) -> Color32 {
        let v = self.histogram_level;
        Color32::from_rgb(if mask & 1 != 0 { v } else { 0 }, if mask & 2 != 0 { v } else { 0 }, if mask & 4 != 0 { v } else { 0 })
    }

    pub fn histogram_fill(&self, mask: u8) -> Color32 {
        self.histogram_color(mask).gamma_multiply(0.3)
    }

    /// Library scopes preserve the inherited display hues, independent of chrome/accent.
    pub fn histogram_library_color(&self, channel: usize) -> Color32 {
        match channel {
            0 => Color32::from_rgb(0xdf, 0x39, 0x39),
            1 => Color32::from_rgb(0x44, 0xb0, 0x72),
            _ => Color32::from_rgb(0x3b, 0x6f, 0xe0),
        }
    }
    pub fn histogram_library_luma_fill(&self) -> Color32 {
        Color32::from_gray(120).gamma_multiply(0.35)
    }
    pub fn histogram_library_neutral(&self) -> Color32 {
        Color32::from_gray(200)
    }
    pub fn histogram_clip_background(&self) -> Color32 {
        Color32::from_gray(60)
    }
    pub fn histogram_clip_color(&self, clipped: bool) -> Color32 {
        if clipped { Color32::WHITE } else { Color32::from_gray(130) }
    }

    /// Analysis colours are semantic hues, independent of the application accent palette.
    pub fn scope_hue(&self, h: f32, s: f32) -> Color32 {
        let v = f32::from(self.histogram_level) / 255.0;
        let f = |offset: f32| {
            let k = (offset + h * 6.0).rem_euclid(6.0);
            (v * (1.0 - s * k.min(4.0 - k).clamp(0.0, 1.0)) * 255.0) as u8
        };
        Color32::from_rgb(f(5.0), f(3.0), f(1.0))
    }

    pub fn histogram_background(&self) -> Color32 {
        self.histogram_bg
    }

    pub fn dark(&self) -> bool {
        matches!(self.kind, ThemeKind::Studio | ThemeKind::Pro | ThemeKind::ProMedium)
    }
}

/// Apply a theme to egui's global style and publish its tokens.
pub fn apply(ctx: &egui::Context, kind: ThemeKind) {
    crate::icons::install(ctx);
    let t = Tokens::for_kind(kind);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("photocraft-theme"), t));
    let mut v = if t.dark() { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.chrome;
    v.window_fill = t.card;
    v.window_stroke = Stroke::new(1.0, t.card_border);
    v.extreme_bg_color = t.field;
    v.faint_bg_color = t.card;
    v.code_bg_color = t.field;
    v.override_text_color = Some(t.text);
    v.hyperlink_color = t.accent;
    v.warn_fg_color = t.warning;
    v.error_fg_color = t.danger;
    v.window_corner_radius = CornerRadius::same(t.radius_lg as u8);
    v.menu_corner_radius = CornerRadius::same(t.radius as u8);
    v.window_shadow = egui::Shadow { offset: [0, 10], blur: 32, spread: 0, color: t.shadow };
    v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: t.shadow };
    v.selection.bg_fill = if t.bevel || t.pro { t.accent } else { t.accent_soft };
    v.selection.stroke = Stroke::new(1.0, t.accent_text);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.striped = false;
    let r = CornerRadius::same(t.radius_sm as u8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.card;
    w.noninteractive.weak_bg_fill = t.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.separator);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text_dim);
    w.noninteractive.corner_radius = r;
    for (wv, bg, stroke) in [
        (&mut w.inactive, t.field, t.field_border),
        (&mut w.hovered, t.hover, t.field_border),
        (&mut w.active, t.pressed, t.accent_border),
        (&mut w.open, t.hover, t.field_border),
    ] {
        wv.bg_fill = bg;
        wv.weak_bg_fill = bg;
        wv.bg_stroke = if t.bevel { Stroke::new(1.0, Color32::from_gray(64)) } else { Stroke::new(1.0, stroke) };
        wv.fg_stroke = Stroke::new(1.0, t.text);
        wv.corner_radius = r;
        wv.expansion = 0.0;
    }
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(10.5)),
            (TextStyle::Body, FontId::proportional(if t.pro { 12.0 } else { 12.5 })),
            (TextStyle::Button, FontId::proportional(if t.pro { 12.0 } else { 12.5 })),
            (TextStyle::Heading, crate::fonts::semibold(15.0)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.interact_size = egui::vec2(24.0, 24.0);
        s.spacing.slider_width = 150.0;
        s.spacing.combo_width = 120.0;
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.window_margin = egui::Margin::same(16);
        s.spacing.icon_width = 14.0;
        s.visuals.indent_has_left_vline = false;
        s.interaction.tooltip_delay = TOOLTIP_DELAY;
        // Thin overlay scrollbars that appear on hover (Photoshop/macOS style).
        s.spacing.scroll = if t.bevel { egui::style::ScrollStyle::solid() } else { egui::style::ScrollStyle::thin() };
        s.spacing.tooltip_width = 280.0;
    });
}

/// Seconds the pointer rests on a control before its tooltip shows.
pub const TOOLTIP_DELAY: f32 = 0.35;

/// Vertical gap between stacked control rows in panels (Properties fields, the Layers panel's
/// Opacity and Fill rows); docks zero egui's item spacing, so rows add this themselves.
pub const ROW_GAP: f32 = 4.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_names_parse() {
        assert_eq!(ThemeKind::from_name("Classic"), Some(ThemeKind::Classic));
        assert_eq!(ThemeKind::from_name("studio (light)"), Some(ThemeKind::StudioLight));
        assert_eq!(ThemeKind::from_name("dark"), Some(ThemeKind::Pro));
        assert_eq!(ThemeKind::from_name("studio"), Some(ThemeKind::Studio));
        assert_eq!(ThemeKind::from_name("Pro (Medium Gray)"), Some(ThemeKind::ProMedium));
        let m = Tokens::for_kind(ThemeKind::ProMedium);
        assert!(m.pro && m.dark() && m.kind == ThemeKind::ProMedium && m.card == Color32::from_rgb(83, 83, 83));
        assert_eq!(ThemeKind::from_name("neon"), None);
    }

    #[test]
    fn studio_text_contrast_is_high() {
        let t = Tokens::for_kind(ThemeKind::Studio);
        let p = Tokens::for_kind(ThemeKind::Pro);
        assert!(p.pro && !t.pro);
        let lum = |c: Color32| 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
        assert!(lum(t.text) - lum(t.card) > 180.0);
        assert!(lum(t.text_dim) - lum(t.card) > 90.0);
    }
}

/// Development feature: live design-token overrides.
///
/// Set `PHOTOCRAFT_THEME_FILE=/path/tokens.json` in a debug build; the file is polled and applied
/// on change, so colours, radii and sizes can be tuned without recompiling. Keys are `Tokens` field
/// names; values are `"#rrggbb"`, `"#rrggbbaa"` or numbers. Compiled out of release builds.
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
pub mod live {
    use super::Tokens;
    use egui::Color32;
    use std::time::SystemTime;

    #[derive(Default)]
    pub struct LiveTokens {
        path: Option<std::path::PathBuf>,
        stamp: Option<SystemTime>,
        last_check: f64,
    }

    impl LiveTokens {
        pub fn from_env() -> Self {
            Self { path: std::env::var_os("PHOTOCRAFT_THEME_FILE").map(Into::into), ..Default::default() }
        }

        /// Re-apply overrides if the file changed. Returns true when tokens were updated.
        pub fn poll(&mut self, ctx: &egui::Context, kind: super::ThemeKind) -> bool {
            let Some(path) = &self.path else { return false };
            let now = ctx.input(|i| i.time);
            if now - self.last_check < 0.4 {
                ctx.request_repaint_after(std::time::Duration::from_millis(400));
                return false;
            }
            self.last_check = now;
            ctx.request_repaint_after(std::time::Duration::from_millis(400));
            let Ok(meta) = std::fs::metadata(path) else { return false };
            let stamp = meta.modified().ok();
            if stamp == self.stamp {
                return false;
            }
            self.stamp = stamp;
            let Ok(text) = std::fs::read_to_string(path) else { return false };
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) => {
                    super::apply(ctx, kind);
                    let mut t = Tokens::get(ctx);
                    let unknown = apply_overrides(&mut t, &v);
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("photocraft-theme"), t));
                    if !unknown.is_empty() {
                        log::warn!("unknown token keys: {unknown:?}");
                    }
                    true
                }
                Err(e) => {
                    log::warn!("theme file: {e}");
                    false
                }
            }
        }
    }

    fn color(v: &serde_json::Value) -> Option<Color32> {
        let s = v.as_str()?.trim_start_matches('#');
        let b = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            6 => Some(Color32::from_rgb(b(0)?, b(2)?, b(4)?)),
            8 => Some(Color32::from_rgba_unmultiplied(b(0)?, b(2)?, b(4)?, b(6)?)),
            _ => None,
        }
    }

    /// Apply JSON overrides onto tokens; returns unknown keys.
    pub fn apply_overrides(t: &mut Tokens, v: &serde_json::Value) -> Vec<String> {
        let mut unknown = Vec::new();
        let Some(obj) = v.as_object() else { return unknown };
        for (k, val) in obj {
            macro_rules! c {
                ($($f:ident),*) => {
                    match k.as_str() {
                        $(stringify!($f) => { if let Some(c) = color(val) { t.$f = c; } })*
                        "radius_sm" => { if let Some(x) = val.as_f64() { t.radius_sm = x as f32; } }
                        "radius" => { if let Some(x) = val.as_f64() { t.radius = x as f32; } }
                        "radius_lg" => { if let Some(x) = val.as_f64() { t.radius_lg = x as f32; } }
                        _ => unknown.push(k.clone()),
                    }
                };
            }
            c!(
                caption_close,
                caption_close_text,
                chrome,
                canvas,
                canvas_dot,
                dock,
                card,
                card_border,
                field,
                field_border,
                hover,
                pressed,
                text,
                text_dim,
                text_faint,
                icon,
                accent,
                accent_soft,
                accent_border,
                accent_text,
                separator,
                shadow,
                primary_bg,
                primary_text,
                danger,
                warning,
                tab_strip,
                row_selected
            );
        }
        unknown
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn overrides_apply_and_report_unknown() {
            let mut t = Tokens::for_kind(super::super::ThemeKind::Pro);
            let unknown = apply_overrides(&mut t, &serde_json::json!({"card": "#102030", "radius": 9, "nope": 1}));
            assert_eq!(t.card, Color32::from_rgb(16, 32, 48));
            assert_eq!(t.radius, 9.0);
            assert_eq!(unknown, vec!["nope".to_string()]);
        }
    }
}

/// Inherited pill-tab metrics, shared by dock tabs and the module picker.
pub const PILL_TAB_FONT_SIZE: f32 = 12.5;
pub const PREFERENCES_TAB_HEIGHT: f32 = 22.0;
pub const PREFERENCES_TAB_INSET: f32 = 8.0;
pub const PREFERENCES_TAB_TEXT_MARGIN: f32 = 12.0;
pub const PILL_TAB_PADDING: f32 = 20.0;
pub const PILL_TAB_HEIGHT: f32 = 24.0;

/// Library-only semantic roles; shared colours live directly on [`Tokens`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LibraryRoles {
    pub dark: bool,
    pub selection: Color32,
    pub selection_text: Color32,
    pub grid_bg: Color32,
    pub cell: Color32,
    pub cell_selected: Color32,
    pub divider: Color32,
    pub inset: Color32,
    pub button: Color32,
    pub button_border: Color32,
    pub tool_active: Color32,
    pub text_label: Color32,
    pub text_disabled: Color32,
    pub track: Color32,
    pub thumb: Color32,
    pub thumb_hover: Color32,
    pub star: Color32,
    pub pick: Color32,
    pub reject: Color32,
    pub caution: Color32,
    pub mask_overlay: Color32,
    pub overlay_bg: Color32,
    pub overlay_fg: Color32,
    pub overlay_reject: Color32,
    /// Unselected Upright guides, paired with an overlay_bg outline over photo pixels.
    pub overlay_guide: Color32,
    pub metrics: Metrics,
}
/// Inherited Library/Develop layout in points, independent of the active palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub top_bar_h: f32,
    pub bottom_bar_h: f32,
    pub panel_w: f32,
    pub strip_w: f32,
    pub slider_row_h: f32,
    pub section_h: f32,
    pub film_h: f32,
    pub panel_inset_left: i8,
    pub panel_inset_right: i8,
}
impl Default for Metrics {
    fn default() -> Self {
        Self {
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
impl Default for LibraryRoles {
    fn default() -> Self {
        Self {
            dark: true,
            selection: Color32::from_rgb(0x01, 0x65, 0xdd).gamma_multiply(0.6),
            selection_text: Color32::from_rgb(0x01, 0x65, 0xdd),
            grid_bg: Color32::from_rgb(0x0f, 0x0f, 0x0f),
            cell: Color32::from_rgb(0x1c, 0x1c, 0x1c),
            cell_selected: Color32::from_rgb(0x2d, 0x2d, 0x2d),
            divider: Color32::from_rgb(0x1c, 0x1c, 0x1c),
            inset: Color32::from_rgb(0x23, 0x23, 0x23),
            button: Color32::from_rgb(0x24, 0x24, 0x24),
            button_border: Color32::from_rgb(0x4c, 0x4c, 0x4c),
            tool_active: Color32::from_rgb(0x3f, 0x3f, 0x3f),
            text_label: Color32::from_rgb(0xbc, 0xbc, 0xbc),
            text_disabled: Color32::from_rgb(0x5c, 0x5c, 0x5c),
            track: Color32::from_rgb(0x5a, 0x5a, 0x5a),
            thumb: Color32::from_rgb(0xa0, 0xa0, 0xa0),
            thumb_hover: Color32::from_rgb(0xe0, 0xe0, 0xe0),
            star: Color32::from_rgb(0xd8, 0xd8, 0xd8),
            pick: Color32::from_rgb(0xf0, 0xf0, 0xf0),
            reject: Color32::from_rgb(0xe0, 0x4a, 0x4a),
            caution: Color32::from_rgb(0xe3, 0xa8, 0x3c),
            mask_overlay: Color32::from_rgba_unmultiplied(0xe0, 0x20, 0x30, 110),
            overlay_bg: Color32::from_gray(26),
            overlay_fg: Color32::from_gray(235),
            overlay_reject: Color32::from_rgb(255, 120, 120),
            overlay_guide: Color32::from_rgb(255, 196, 40),
            metrics: Metrics::default(),
        }
    }
}

/// Inherited editor slider and Classic bevel roles, shared by reusable controls.
#[derive(Clone, Copy, Debug)]
pub struct SliderColors {
    pub knob: Color32,
    pub knob_border: Color32,
    pub knob_shadow: Color32,
    pub bevel_highlight: Color32,
    pub bevel_shadow: Color32,
}

impl Tokens {
    pub fn slider_colors(self) -> SliderColors {
        SliderColors {
            knob: if self.pro { Color32::from_gray(236) } else { Color32::WHITE },
            knob_border: Color32::from_gray(40),
            knob_shadow: Color32::from_black_alpha(90),
            bevel_highlight: Color32::WHITE,
            bevel_shadow: Color32::from_gray(64),
        }
    }
}
