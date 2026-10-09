//! local-image: the **Layers** list of a layered document (PSD/PSB, layered TIFF, `.pcraft`) in
//! Library (Info) and Develop (Edit), read-only: each layer's thumbnail, name, visibility, blend
//! mode and opacity; Develop layers are marked with the Library photo they follow (click it to go
//! to that photo).
//!
//! Develop works on the document's merged image (that is what the Library decodes), so the list
//! says so and offers **Open in Compositing** to work on the layers: the document opens in this
//! window's editor through [`crate::Services::open_with`] (no app named = Local Image's own), and
//! coming back to Develop from there asks how to develop a layered document (the composite live,
//! a merged copy, or just switch), as any switch from Compositing does.
//!
//! The layers come from the host ([`crate::Services::doc_layers`]): reading a layered document
//! takes the editor's PSD and document readers, which this crate doesn't link. They are read on
//! a worker thread, once per version of the file (its size and content hash).

use std::sync::Arc;

use egui::{Align2, Rect, Sense, TextureHandle, pos2, vec2};
use lightcraft_catalog::PhotoId;
use serde_json::json;

use crate::LightcraftApp;
use crate::icons::{Icon, paint};
use crate::theme::Tokens;
use crate::widgets::{divider, register, section_header, text_button};

/// Reads the layers of the document at a path (host service; runs on a worker thread).
pub type DocLayersFn = Arc<dyn Fn(&str) -> Result<DocLayers, String> + Send + Sync>;

/// File extensions of documents that can have layers.
pub const EXTENSIONS: [&str; 5] = ["psd", "psb", "tif", "tiff", "pcraft"];

/// Section id (open/closed state).
const SECTION: &str = "docLayers";

/// A layered document's layers, top first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocLayers {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<DocLayer>,
    /// Layers left out of `layers` (very large documents are listed in part).
    pub more: usize,
}

impl DocLayers {
    /// Worth listing: more than one layer, or a Develop layer.
    pub fn is_layered(&self) -> bool {
        self.layers.len() + self.more > 1 || self.layers.iter().any(|l| l.kind == LayerKind::Develop)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Pixel,
    Group,
    Adjustment,
    Fill,
    Text,
    Shape,
    Smart,
    /// A Develop layer (a smart object rendered through the develop engine).
    Develop,
}

impl LayerKind {
    fn label(self) -> &'static str {
        match self {
            LayerKind::Pixel => "Pixel layer",
            LayerKind::Group => "Group",
            LayerKind::Adjustment => "Adjustment layer",
            LayerKind::Fill => "Fill layer",
            LayerKind::Text => "Text layer",
            LayerKind::Shape => "Shape layer",
            LayerKind::Smart => "Smart Object",
            LayerKind::Develop => "Develop layer",
        }
    }
}

/// One layer of a [`DocLayers`].
#[derive(Clone, Debug, PartialEq)]
pub struct DocLayer {
    pub name: String,
    /// Nesting depth (0 = top level; a group's layers are one deeper).
    pub depth: usize,
    pub visible: bool,
    /// 0..1.
    pub opacity: f32,
    /// Blend mode, as Photoshop names it ("Normal", "Multiply"…).
    pub blend: String,
    pub kind: LayerKind,
    /// A Develop layer's Library photo (`None`: it keeps its settings without following one).
    pub follows: Option<u64>,
    /// A small thumbnail (sRGB).
    pub thumb: Option<lightcraft_raster::Rgba8>,
}

/// Could the file at `path` be a layered document?
pub fn layered_candidate(path: &str) -> bool {
    std::path::Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Photo `id`'s file and the cache key of its current version, when it can be layered.
fn source(app: &LightcraftApp, id: PhotoId) -> Option<(String, String)> {
    let p = app.session.catalog.photo(id)?;
    let lightcraft_catalog::Source::File { path } = &p.source else { return None };
    layered_candidate(path).then(|| (path.clone(), format!("{path}\n{}\n{}", p.file_size, p.content_hash.as_deref().unwrap_or(""))))
}

/// The layers of photo `id`'s document: `None` when it isn't a layered document (or there is no
/// reader), `Some(None)` while it is being read.
pub fn layers_of(app: &LightcraftApp, ui: &egui::Ui, id: PhotoId) -> Option<Option<Result<DocLayers, String>>> {
    let read = app.services.doc_layers.clone()?;
    let (_, key) = source(app, id)?;
    Some(crate::panels::left::fs_cached(ui, "doc-layers", &key, f64::INFINITY, move |k| read(k.split('\n').next().unwrap_or(k))))
}

/// Opens photo `id`'s document in Compositing; the Library picks up what is saved there when
/// you come back.
pub fn open_in_compositing(app: &mut LightcraftApp, ctx: &egui::Context, id: PhotoId) {
    let Some((path, _)) = source(app, id) else { return };
    if !app.ui.external_edits.contains(&id.0) {
        app.ui.external_edits.push(id.0);
    }
    match app.services.open_with.as_mut() {
        Some(f) => {
            if let Err(e) = f(&path, "") {
                app.toast_error(ctx, format!("Couldn't open the editor: {e}"));
            }
        }
        None => app.toast(ctx, crate::i18n::tr("Compositing isn't available here")),
    }
}

/// The Layers section for photo `id` (nothing for a photo that isn't a layered document).
pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    // a Camera Raw Filter session edits one layer's pixels: no document to list
    if app.host_session.is_some() {
        return;
    }
    let Some(state) = layers_of(app, ui, id) else { return };
    let doc = match state {
        Some(Ok(d)) if d.is_layered() => d,
        // still reading, unreadable, or a single layer: nothing to show
        _ => return,
    };
    let open = app.ui.section_open(SECTION);
    let (resp, _) = section_header(ui, SECTION, "Layers", open, None);
    if resp.clicked() {
        app.ui.toggle_section(SECTION);
    }
    if open {
        body(app, ui, id, &doc);
    }
    divider(ui);
}

fn body(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId, doc: &DocLayers) {
    let t = Tokens::get(ui.ctx());
    let key = source(app, id).map(|(_, k)| k).unwrap_or_default();
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 10 }).show(ui, |ui| {
        ui.label(
            egui::RichText::new(crate::i18n::tr("Develop edits the merged image of this document. Open it in Compositing to work on its layers."))
                .size(11.5)
                .color(t.text_dim),
        );
        ui.add_space(6.0);
        for (i, l) in doc.layers.iter().enumerate() {
            row(app, ui, &key, i, l);
        }
        if doc.more > 0 {
            ui.label(egui::RichText::new(crate::i18n::tr_format!("{n} more layers", n = doc.more)).size(11.5).color(t.text_dim));
        }
        ui.add_space(8.0);
        if text_button(ui, "openInCompositing", "Open in Compositing", false).clicked() {
            let ctx = ui.ctx().clone();
            open_in_compositing(app, &ctx, id);
        }
    });
}

/// A cached texture for layer `i`'s thumbnail of the document version `key`.
fn thumb_texture(ctx: &egui::Context, key: &str, i: usize, img: &lightcraft_raster::Rgba8) -> TextureHandle {
    let tid = egui::Id::new(("doc-layer-thumb", key, i));
    if let Some(t) = ctx.data(|d| d.get_temp::<TextureHandle>(tid)) {
        return t;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.as_bytes());
    let tex = ctx.load_texture(format!("doc-layer-{i}"), color, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(tid, tex.clone()));
    tex
}

fn row(app: &mut LightcraftApp, ui: &mut egui::Ui, key: &str, i: usize, l: &DocLayer) {
    let t = Tokens::get(ui.ctx());
    let develop = l.kind == LayerKind::Develop;
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), if develop { 62.0 } else { 46.0 }), Sense::hover());
    register(ui.ctx(), format!("docLayers:row:{i}"), r);
    let p = ui.painter().clone();
    let x0 = r.left() + (l.depth.min(6) as f32) * 14.0;
    // visibility (shown, not switched: the document is edited in Compositing)
    let eye = Rect::from_center_size(pos2(x0 + 9.0, r.top() + 23.0), vec2(16.0, 16.0));
    paint(&p, eye, if l.visible { Icon::Eye } else { Icon::EyeOff }, if l.visible { t.icon } else { t.text_disabled });
    // thumbnail
    let tile = Rect::from_min_size(pos2(x0 + 24.0, r.top() + 5.0), vec2(36.0, 36.0));
    p.rect_filled(tile, 2.0, t.inset);
    match &l.thumb {
        Some(img) if img.width > 0 && img.height > 0 => {
            let tex = thumb_texture(ui.ctx(), key, i, img);
            let s = (tile.width() / img.width as f32).min(tile.height() / img.height as f32);
            let fit = Rect::from_center_size(tile.center(), vec2(img.width as f32 * s, img.height as f32 * s));
            p.image(tex.id(), fit, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        _ if l.kind == LayerKind::Group => paint(&p, tile.shrink(8.0), Icon::Stack, t.text_label),
        _ => {}
    }
    // name, then kind · blend · opacity
    let text_x = tile.right() + 10.0;
    let dim = if l.visible { t.text } else { t.text_dim };
    p.text(pos2(text_x, r.top() + 14.0), Align2::LEFT_CENTER, &l.name, t.font(13.0), dim);
    let mut detail = vec![crate::i18n::tr(l.kind.label()).to_string()];
    if l.blend != "Normal" && l.blend != "Pass Through" {
        detail.push(crate::i18n::tr(&l.blend).to_string());
    }
    if l.opacity < 0.995 {
        detail.push(format!("{:.0}%", l.opacity * 100.0));
    }
    let detail_color = if develop { t.accent } else { t.text_dim };
    p.text(pos2(text_x, r.top() + 32.0), Align2::LEFT_CENTER, detail.join(" · "), t.font(11.0), detail_color);
    // a Develop layer's photo: click to go to it
    if develop {
        let label = match l.follows.map(|id| (id, app.session.catalog.photo(PhotoId(id)).map(|p| p.file_name.clone()))) {
            Some((_, Some(name))) => crate::i18n::tr_format!("Follows {name}", name = name),
            Some((_, None)) => crate::i18n::tr("Its photo isn't in this library").to_string(),
            None => crate::i18n::tr("Keeps its own settings").to_string(),
        };
        let galley = p.layout_no_wrap(label, t.font(11.0), t.accent);
        let link = Rect::from_min_size(pos2(text_x, r.top() + 42.0), galley.size() + vec2(2.0, 4.0));
        let follows = l.follows.filter(|id| app.session.catalog.photo(PhotoId(*id)).is_some());
        let resp = ui.interact(link, ui.id().with(("doc-layer-follow", i)), if follows.is_some() { Sense::click() } else { Sense::hover() });
        register(ui.ctx(), format!("docLayers:follow:{i}"), link);
        let color = if resp.hovered() && follows.is_some() { t.text } else { t.accent };
        p.galley_with_override_text_color(link.min, galley, color);
        if let Some(photo) = follows
            && resp.clicked()
        {
            let _ = app.run("library.select", json!({"ids": [photo], "active": photo}));
        }
    }
}
