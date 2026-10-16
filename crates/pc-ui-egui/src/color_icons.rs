//! Colour tool icons in this shell's theme: [`li_ui_kit::color_icons`] tinted with the current
//! module's icon token. Everything else (registry, rasterising, the colour/monochrome switch) is
//! used from `li_ui_kit` directly.
pub fn image(ui: &egui::Ui, name: &str, size: f32, disabled: bool) -> Option<egui::Image<'static>> {
    li_ui_kit::color_icons::image_with(ui, name, size, disabled, crate::theme::Tokens::get(ui.ctx()).icon, ui.visuals().dark_mode)
}
