//! The editor's theme: the shared design system (`li_ui_kit::theme` and fonts, used through this
//! module by the editor's panels) plus the editor-specific CJK font sources and UI font sizes.
pub use li_ui_kit::fonts::{medium, mono, semibold, size_ui_font};
pub use li_ui_kit::theme::*;
use photocraft_engine::prefs::UiFontSize;
pub fn install_fonts(ctx: &egui::Context) {
    install_fonts_with(ctx, crate::cjk_fonts::Sources::system());
}
pub fn install_fonts_with(ctx: &egui::Context, cjk: crate::cjk_fonts::Sources) {
    li_ui_kit::fonts::install_editor(ctx);
    crate::cjk_fonts::install_with(ctx, cjk);
}
fn shared_size(size: UiFontSize) -> li_ui_kit::fonts::FontSize {
    use li_ui_kit::fonts::FontSize;
    match size {
        UiFontSize::Tiny => FontSize::Tiny,
        UiFontSize::Small => FontSize::Small,
        UiFontSize::Medium => FontSize::Medium,
        UiFontSize::Large => FontSize::Large,
    }
}
pub(crate) fn set_ui_font_size(ctx: &egui::Context, size: UiFontSize) {
    li_ui_kit::fonts::set_ui_font_size(ctx, shared_size(size));
}
#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, FontFamily, FontId, TextStyle};
    fn font_scale(size: UiFontSize) -> f32 {
        li_ui_kit::fonts::font_scale(shared_size(size))
    }
    /// Upstream 9cdcfbb1 (#1467): the desktop app installs fonts and loads the saved UI Font Size
    /// in its first frame, while egui's default fonts are still active. Any size but Small then
    /// panicked on the next pass ("FontFamily::Name("medium") is not bound to any fonts") because
    /// the plugin rescaled a copy of the defaults and that replaced the queued Inter stack.
    #[test]
    fn saved_ui_font_size_survives_first_frame_install() {
        let ctx = egui::Context::default();
        let medium_bound = |ctx: &egui::Context| ctx.fonts(|f| f.families().contains(&FontFamily::Name("medium".into())));
        ctx.run_ui(Default::default(), |ui| {
            assert!(!medium_bound(ui.ctx()), "egui's defaults are active in the first frame");
            install_fonts(ui.ctx());
            apply(ui.ctx(), ThemeKind::Pro);
            set_ui_font_size(ui.ctx(), UiFontSize::Medium);
        })
        .textures_delta
        .clear();
        let mut width = 0.0;
        ctx.run_ui(Default::default(), |ui| {
            assert!(medium_bound(ui.ctx()), "the installed stack reaches the second pass");
            width = ui.painter().layout_no_wrap("Local Image".into(), medium(13.0), Color32::WHITE).size().x;
        })
        .textures_delta
        .clear();
        assert!(width > 0.0);
        // The saved size applies one pass later, from the installed stack.
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        let mut scaled = 0.0;
        ctx.run_ui(Default::default(), |ui| {
            assert!(medium_bound(ui.ctx()));
            scaled = ui.painter().layout_no_wrap("Local Image".into(), medium(13.0), Color32::WHITE).size().x;
        })
        .textures_delta
        .clear();
        assert!((scaled - width * font_scale(UiFontSize::Medium)).abs() < 1.0, "{scaled} vs {width} × 14/12");
        // Re-installing (a language change) keeps the size and the named families.
        ctx.run_ui(Default::default(), |ui| install_fonts(ui.ctx())).textures_delta.clear();
        for _ in 0..2 {
            ctx.run_ui(Default::default(), |ui| {
                assert!(medium_bound(ui.ctx()));
                let w = ui.painter().layout_no_wrap("Local Image".into(), medium(13.0), Color32::WHITE).size().x;
                assert!((w - scaled).abs() < 1.0, "{w} vs {scaled}");
            })
            .textures_delta
            .clear();
        }
    }

    fn text_sizes(ctx: &egui::Context) -> Vec<egui::Vec2> {
        let mut sizes = Vec::new();
        // The plugin queues changed fonts at the end of a pass; egui applies them next pass.
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        ctx.run_ui(Default::default(), |ui| {
            // Include custom painter fonts and explicit RichText sizes, not just text styles.
            for font in [FontId::proportional(11.5), medium(12.0), semibold(15.0), mono(12.0), TextStyle::Body.resolve(ui.style())] {
                sizes.push(ui.painter().layout_no_wrap("Interface 123".into(), font, Color32::WHITE).size());
            }
            sizes.push(ui.label(egui::RichText::new("Interface 123").size(13.0)).rect.size());
        })
        .textures_delta
        .clear();
        sizes
    }

    #[test]
    fn ui_font_sizes_scale_shaping_and_row_height_without_accumulating() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        apply(&ctx, ThemeKind::Pro);
        let original = text_sizes(&ctx);
        let definitions = ctx.fonts(|f| f.definitions().clone());
        for size in [UiFontSize::Tiny, UiFontSize::Small, UiFontSize::Medium, UiFontSize::Large, UiFontSize::Tiny, UiFontSize::Small] {
            set_ui_font_size(&ctx, size);
            let measured = text_sizes(&ctx);
            for (before, after) in original.iter().zip(&measured) {
                let expected = *before * font_scale(size);
                assert!((after.x - expected.x).abs() < 1.0, "{size:?}: width {after:?} vs {expected:?}");
                // egui rounds ascent/descent and the final row height to pixels separately.
                assert!((after.y - expected.y).abs() < 1.1, "{size:?}: height {after:?} vs {expected:?}");
            }
            set_ui_font_size(&ctx, size);
            assert_eq!(text_sizes(&ctx), measured, "setting the same size twice is stable");
        }
        assert_eq!(ctx.fonts(|f| f.definitions().clone()), definitions, "Small restores original font tweaks exactly");
        assert_eq!(ctx.zoom_factor(), 1.0);
    }
}
