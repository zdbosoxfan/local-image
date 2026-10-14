//! One font stack builder and scaling plugin; locale/CJK source policy remains in each UI adapter.
use egui::{FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontSize {
    Tiny,
    #[default]
    Small,
    Medium,
    Large,
}
#[derive(Clone, Copy, Debug)]
pub enum Profile {
    Editor,
    Library,
}
/// Existing stacks intentionally retain their fallback order and metrics.
pub fn definitions(profile: Profile) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "Inter", photocraft_text::fonts::INTER_REGULAR);
    add(&mut fonts, "Inter-Medium", photocraft_text::fonts::INTER_MEDIUM);
    add(&mut fonts, "Inter-SemiBold", photocraft_text::fonts::INTER_SEMIBOLD);
    add(&mut fonts, "JetBrainsMono", photocraft_text::fonts::JETBRAINS_MONO_REGULAR);
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "Inter".to_owned());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "JetBrainsMono".to_owned());
    // Low-priority symbol coverage (e.g. ▸/▾) for labels using proportional/named fonts.
    fonts.families.entry(FontFamily::Proportional).or_default().push("JetBrainsMono".to_owned());
    // Named weights fall back to the default stack for missing glyphs.
    let fallback: Vec<String> = fonts.families[&FontFamily::Proportional].clone();
    for (fam, primary) in [("medium", "Inter-Medium"), ("semibold", "Inter-SemiBold")] {
        let mut stack = vec![primary.to_owned()];
        stack.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(fam.into()), stack);
    }
    if matches!(profile, Profile::Library) {
        fonts.font_data.remove("Inter-Medium");
        fonts.families.remove(&FontFamily::Name("medium".into()));
        let mono = fonts.families.entry(FontFamily::Monospace).or_default();
        mono.retain(|name| name != "JetBrainsMono");
        mono.push("JetBrainsMono".into());
        let semi = fonts.families.get_mut(&FontFamily::Name("semibold".into())).unwrap();
        semi.retain(|name| name != "Inter");
    }
    fonts
}
/// Apply registered faces at the saved size, retaining the deferred first-frame fix (#1467).
pub fn install_editor(ctx: &egui::Context) {
    let mut fonts = definitions(Profile::Editor);
    let size = ui_font_size(ctx);
    for (name, data) in &mut fonts.font_data {
        size_ui_font(ctx, name, Arc::make_mut(data));
    }
    ctx.set_fonts(fonts);
    if ctx
        .with_plugin::<FontSizePlugin, _>(|plugin| {
            plugin.applied = size;
            plugin.defer_after_install = true;
        })
        .is_none()
    {
        ctx.add_plugin(FontSizePlugin { applied: size, defer_after_install: true });
    }
}
/// Has [`install_editor`]'s stack reached the active fonts? Before that (the first pass, when
/// the desktop app installs fonts and loads preferences in the same frame) the active fonts
/// are egui's defaults, and rescaling a copy of those would overwrite the queued stack.
fn ui_fonts_active(ctx: &egui::Context) -> bool {
    ctx.fonts(|f| f.definitions().families.contains_key(&FontFamily::Name("medium".into())))
}

fn ui_fonts_id() -> egui::Id {
    egui::Id::new("photocraft-ui-fonts")
}

fn ui_font_size(ctx: &egui::Context) -> FontSize {
    ctx.data(|d| d.get_temp::<FontSize>(ui_fonts_id())).unwrap_or_default()
}

pub fn font_scale(size: FontSize) -> f32 {
    match size {
        FontSize::Tiny => 10.0 / 12.0,
        FontSize::Small => 1.0,
        FontSize::Medium => 14.0 / 12.0,
        FontSize::Large => 16.0 / 12.0,
    }
}

/// Register the original multiplier before sizing a face, including lazy CJK fallbacks.
pub fn size_ui_font(ctx: &egui::Context, name: &str, data: &mut FontData) {
    ctx.data_mut(|d| d.insert_temp(ui_fonts_id().with(name), data.tweak.scale));
    data.tweak.scale *= font_scale(ui_font_size(ctx));
}

/// Apply Interface › UI Font Size independently of display/canvas zoom. Scaling the registered
/// faces covers explicit RichText and painter font sizes as well as egui's text styles. egui
/// 0.36 uses these tweaks in shaping and row metrics; the layout tests below guard that contract.
pub fn set_ui_font_size(ctx: &egui::Context, size: FontSize) {
    if ui_font_size(ctx) == size {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(ui_fonts_id(), size));
    ctx.request_repaint();
}

/// Font access is valid after the first pass, including when preferences load before it.
/// Keep only the original multipliers, not a second copy of large system font files.
struct FontSizePlugin {
    applied: FontSize,
    /// `set_fonts` takes effect next pass: skip one hook so the rescale starts from that stack
    /// and not from the one it replaces.
    defer_after_install: bool,
}

impl egui::Plugin for FontSizePlugin {
    fn debug_name(&self) -> &'static str {
        "photocraft-ui-font-size"
    }

    fn output_hook(&mut self, ctx: &egui::Context, _output: &mut egui::FullOutput) {
        let size = ui_font_size(ctx);
        let deferred = std::mem::take(&mut self.defer_after_install);
        if size == self.applied {
            return;
        }
        if deferred || !ui_fonts_active(ctx) {
            // Cloning the active definitions now would discard the queued stack (the desktop
            // app installs fonts and loads the saved size in its first frame).
            ctx.request_repaint();
            return;
        }
        // Start from the current stack so lazily registered fallback faces are retained.
        let mut fonts = ctx.fonts(|f| f.definitions().clone());
        for (name, data) in &mut fonts.font_data {
            let id = ui_fonts_id().with(name);
            let original = ctx.data(|d| d.get_temp::<f32>(id)).unwrap_or(data.tweak.scale / font_scale(self.applied));
            ctx.data_mut(|d| d.insert_temp(id, original));
            Arc::make_mut(data).tweak.scale = original * font_scale(size);
        }
        ctx.set_fonts(fonts);
        self.applied = size;
        ctx.request_repaint();
    }
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}
