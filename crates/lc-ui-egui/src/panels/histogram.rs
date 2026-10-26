//! Develop histogram and cached photo metadata.

use egui::{Align2, pos2};
use lightcraft_catalog::PhotoId;

use crate::{LightcraftApp, render::Slot, theme::Tokens, widgets::register};

pub(super) fn show(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let t = Tokens::get(ui.ctx());
    // Borrow cached bins: no image-sized work or per-frame histogram/smoothing allocation.
    let hist = app
        .renderer
        .textures
        .get(&Slot::Main)
        .filter(|t| t.photo == id)
        .and_then(|t| t.histogram.as_ref())
        .or_else(|| app.renderer.textures.get(&Slot::Thumb(id)).and_then(|t| t.histogram.as_ref()));
    let view = hist.map(|h| li_ui_kit::histogram::Histogram::u32([&h.r, &h.g, &h.b], Some(&h.luma)));
    let resp =
        li_ui_kit::histogram::library(ui, view, hist.map_or((0.0, 0.0), |h| h.clipping()), app.ui.show_clipping, li_i18n::library::t("Histogram"));
    let r = resp.rect;
    register(ui.ctx(), "histogram", r);
    if resp.clicked() {
        app.ui.show_clipping = !app.ui.show_clipping;
    }
    let p = ui.painter();
    // EXIF strip
    if let Some(ph) = app.session.catalog.photo(id) {
        let m = &ph.meta;
        let items = [
            m.iso.map(|i| format!("ISO {i}")).unwrap_or_default(),
            m.focal_mm.map(|f| format!("{f:.0}mm")).unwrap_or_default(),
            m.aperture.map(|a| format!("f/{a:.1}")).unwrap_or_default(),
            m.shutter.clone(),
        ];
        let y = r.bottom() - 13.0;
        for (i, s) in items.iter().enumerate() {
            let x = r.left() + 18.0 + (r.width() - 36.0) * i as f32 / 3.0;
            let align = match i {
                0 => Align2::LEFT_CENTER,
                3 => Align2::RIGHT_CENTER,
                _ => Align2::CENTER_CENTER,
            };
            p.text(pos2(x, y), align, s, t.font(12.5), t.text_label);
        }
    }
}
