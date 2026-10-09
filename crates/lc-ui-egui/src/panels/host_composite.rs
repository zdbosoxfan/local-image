//! local-image: a photo **being edited in Compositing**, shown as it is there. When Compositing
//! has a document open for a Library photo (a photo taken there from Develop, with layers added)
//! and its work isn't saved yet, the host renders the document's composite and hands it over
//! ([`set`]); the Library's loupe and grid then show that image instead of the photo's own render,
//! marked "Edited in Compositing · unsaved", with a button back to the document
//! ([`LightcraftApp::host_composite_open`], which the host takes). Develop keeps showing the
//! photo itself: its settings are what Develop edits. Saving the document in Compositing adds it
//! to the Library as its own photo, and the host drops the stand-in ([`retain`]).

use egui::{Color32, Rect, Sense, TextureHandle, pos2, vec2};
use lightcraft_catalog::PhotoId;

use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::register;

/// The composite of a Compositing document, shown for a Library photo.
pub struct HostComposite {
    pub tex: TextureHandle,
    /// Pixels (width, height).
    pub size: [usize; 2],
    /// What the badge says ("Edited in Compositing · unsaved").
    pub label: String,
    /// The host's version of the image (document and revision): unchanged images aren't re-sent.
    pub key: u64,
}

/// Shows `rgba` (sRGB, `w`×`h`) for photo `id` in the Library until [`retain`] drops it.
pub fn set(app: &mut LightcraftApp, ctx: &egui::Context, id: PhotoId, key: u64, w: usize, h: usize, rgba: &[u8], label: impl Into<String>) {
    let img = egui::ColorImage::from_rgba_unmultiplied([w.max(1), h.max(1)], rgba);
    let tex = ctx.load_texture(format!("host-composite-{}", id.0), img, egui::TextureOptions::LINEAR);
    app.host_composites.insert(id, HostComposite { tex, size: [w.max(1), h.max(1)], label: label.into(), key });
    ctx.request_repaint();
}

/// The host's key of the image shown for `id`, if any.
pub fn key(app: &LightcraftApp, id: PhotoId) -> Option<u64> {
    app.host_composites.get(&id).map(|c| c.key)
}

/// Keeps the stand-ins of `keep` and drops the others (saved or closed documents).
pub fn retain(app: &mut LightcraftApp, keep: &[PhotoId]) {
    app.host_composites.retain(|id, _| keep.contains(id));
}

/// Draws photo `id`'s composite in the loupe (`area`), with its badge and the button back to
/// Compositing. `None` when Compositing isn't editing the photo; otherwise the button's rect,
/// which the caller makes clickable with [`button`] once the loupe's own interaction exists
/// (so the button is on top of it).
pub fn loupe(app: &LightcraftApp, ui: &egui::Ui, id: PhotoId, area: Rect) -> Option<Rect> {
    let c = app.host_composites.get(&id)?;
    let t = Tokens::get(ui.ctx());
    let ppp = ui.ctx().pixels_per_point();
    let aspect = c.size[0] as f32 / c.size[1] as f32;
    let r = super::detail::fit_rect(area, aspect, crate::state::Zoom::Fit, c.size, ppp, (0.0, 0.0));
    let p = ui.painter_at(area);
    p.image(c.tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    // the badge and the way back, over the image's top-left corner
    let g = p.layout_no_wrap(c.label.clone(), t.semibold(12.0), Color32::WHITE);
    let badge = Rect::from_min_size(r.left_top() + vec2(8.0, 8.0), g.size() + vec2(16.0, 8.0));
    p.rect_filled(badge, 4.0, Color32::from_rgba_unmultiplied(40, 90, 170, 220));
    p.galley(badge.min + vec2(8.0, 4.0), g, Color32::WHITE);
    let bg = p.layout_no_wrap(crate::i18n::tr("Open in Compositing").to_string(), t.font(12.0), Color32::WHITE);
    let button = Rect::from_min_size(pos2(badge.right() + 6.0, badge.top()), bg.size() + vec2(16.0, 8.0));
    register(ui.ctx(), "hostComposite:open", button);
    Some(button)
}

/// The button back to Compositing at `rect` (from [`loupe`]); `true` when clicked.
pub fn button(ui: &egui::Ui, id: PhotoId, rect: Rect) -> bool {
    let t = Tokens::get(ui.ctx());
    let resp = ui.interact(rect, egui::Id::new(("host-composite-open", id.0)), Sense::click());
    let p = ui.painter();
    let g = p.layout_no_wrap(crate::i18n::tr("Open in Compositing").to_string(), t.font(12.0), Color32::WHITE);
    p.rect_filled(rect, 4.0, if resp.hovered() { Color32::from_black_alpha(220) } else { Color32::from_black_alpha(160) });
    p.galley(rect.min + vec2(8.0, 4.0), g, Color32::WHITE);
    resp.on_hover_text(crate::i18n::tr("Back to the document in Compositing (its layers are kept)")).clicked()
}

/// Photo `id`'s composite for a grid cell: (texture, size).
pub fn thumb(app: &LightcraftApp, id: PhotoId) -> Option<(egui::TextureId, [usize; 2])> {
    app.host_composites.get(&id).map(|c| (c.tex.id(), c.size))
}

/// The grid cell's mark: a small "Compositing" tag in the thumbnail's corner.
pub fn grid_badge(ui: &egui::Ui, img: Rect) {
    let t = Tokens::get(ui.ctx());
    let p = ui.painter();
    let g = p.layout_no_wrap(crate::i18n::tr("Compositing · unsaved").to_string(), t.semibold(9.0), Color32::WHITE);
    let r = Rect::from_min_size(img.left_top() + vec2(4.0, 4.0), g.size() + vec2(8.0, 4.0));
    p.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(40, 90, 170, 220));
    p.galley(r.min + vec2(4.0, 2.0), g, Color32::WHITE);
}
