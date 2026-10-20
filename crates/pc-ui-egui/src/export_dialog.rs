//! File › Export › Export As… (and Quick Export as PNG): format, quality, transparency and scale,
//! with a preview and an estimated file size. The estimate encodes a small proxy and scales by
//! pixel count, so the dialog stays instant on 36 MP documents. A many-layer or very large
//! document is reduced and encoded on a worker thread instead of inside the UI frame.

use std::sync::{Arc, Mutex};

use li_raster::resample::ResampleChoice;
use photocraft_doc::Document;
use serde_json::{Map, Value, json};

use crate::state::DialogKind;
use crate::theme::Tokens;
use crate::{ExportSettings, PhotocraftApp};

const FORMATS: [(&str, &str); 7] = [("png", "PNG"), ("jpg", "JPG"), ("webp", "WebP"), ("tif", "TIFF"), ("tga", "TGA"), ("svg", "SVG"), ("pdf", "PDF")];
const PREFS: &str = "file.exportAs";
const KEYS: [&str; 7] = ["format", "quality", "lossless", "transparency", "scale", "metadata", "resample"];
/// The default resample choice: plain Bicubic, as Export As always used.
const DEFAULT_RESAMPLE: ResampleChoice = ResampleChoice::Bicubic;

pub fn open(app: &mut PhotocraftApp) -> Result<u64, String> {
    open_with_format(app, None)
}
pub(crate) fn open_format(app: &mut PhotocraftApp, format: &str) -> Result<u64, String> {
    open_with_format(app, Some(format))
}
fn open_with_format(app: &mut PhotocraftApp, format: Option<&str>) -> Result<u64, String> {
    if app.session.active().is_none() {
        return Err("no document".into());
    }
    // A second entry point must not replace the worker that owns publication.
    if pending(app) {
        return Err(tl!("An export is already running; finish or cancel it first").into());
    }
    complete(app);
    let st = app.session.active().ok_or("no document")?;
    let mut f = Map::new();
    f.insert("__export".into(), json!(true));
    f.insert("__label".into(), json!("Export As"));
    f.insert("format".into(), json!("png"));
    f.insert("quality".into(), json!(85));
    f.insert("lossless".into(), json!(false));
    f.insert("transparency".into(), json!(true));
    f.insert("scale".into(), json!(100));
    f.insert("metadata".into(), json!("none"));
    f.insert("resample".into(), json!(DEFAULT_RESAMPLE.key()));
    if let Some(saved) = app.session.prefs().dialogs.get(PREFS).and_then(Value::as_object) {
        for key in KEYS {
            if let Some(value) = saved.get(key) {
                f.insert(key.into(), value.clone());
            }
        }
    }
    if let Some(format) = format {
        f.insert("format".into(), json!(format));
    }
    if !FORMATS.iter().any(|(format, _)| *format == s_fmt(&f)) {
        f.insert("format".into(), json!("png"));
    }
    f.insert("__document".into(), json!(st.doc.id.0));
    f.insert("__revision".into(), json!(st.revision));
    f.insert("__stem".into(), json!(basename(st.path.as_deref().unwrap_or(&st.doc.name))));
    f.insert("__w".into(), json!(st.doc.size.width));
    f.insert("__h".into(), json!(st.doc.size.height));
    app.ui.svg_export.options =
        app.session.prefs().dialogs.get("vector.svg.exportOptions").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    app.ui.pdf_export.options =
        app.session.prefs().dialogs.get("vector.pdf.exportOptions").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    app.ui.svg_export.open(&app.session).map_err(|e| e.to_string())?;
    app.ui.pdf_export.open(&app.session).map_err(|e| e.to_string())?;
    let svg_scope = app.session.prefs().dialogs.get("vector.svg.exportScope").and_then(Value::as_str).unwrap_or("document");
    app.ui.svg_export.restore_scope(svg_scope);
    let pdf_scope = app.session.prefs().dialogs.get("vector.pdf.exportScope").and_then(Value::as_str).unwrap_or("document");
    app.ui.pdf_export.restore_scope(pdf_scope);
    app.ui.svg_export.embedded = true;
    app.ui.pdf_export.embedded = true;
    Ok(app.ui.open_dialog(DialogKind::Command, f))
}
fn basename(name: &str) -> String {
    // Both separators are accepted because documents can originate on another OS.
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    std::path::Path::new(file).file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty()).unwrap_or("drawing").to_owned()
}
pub(crate) type Remembered = Map<String, Value>;
fn remembered(app: &PhotocraftApp, f: &Map<String, Value>) -> Remembered {
    let saved: Map<String, Value> = KEYS.iter().filter_map(|key| f.get(*key).map(|v| ((*key).into(), v.clone()))).collect();
    let mut values = Map::from_iter([(PREFS.into(), Value::Object(saved))]);
    match s_fmt(f).as_str() {
        "svg" => {
            if let Ok(options) = serde_json::to_value(&app.ui.svg_export.options) {
                values.insert("vector.svg.exportOptions".into(), options);
            }
            if let Some(scope) = app.ui.svg_export.scope_name() {
                values.insert("vector.svg.exportScope".into(), json!(scope));
            }
        }
        "pdf" => {
            // Layer IDs belong to one document, never to global export defaults.
            let mut options = app.ui.pdf_export.options.clone();
            options.artboards.clear();
            if let Ok(options) = serde_json::to_value(options) {
                values.insert("vector.pdf.exportOptions".into(), options);
            }
            if let Some(scope) = app.ui.pdf_export.scope_name() {
                values.insert("vector.pdf.exportScope".into(), json!(scope));
            }
        }
        _ => {}
    }
    values
}
pub(crate) fn publish_preferences(app: &mut PhotocraftApp, values: Remembered) {
    app.session.edit_prefs(|prefs| prefs.dialogs.extend(values));
}
pub(crate) fn pending(app: &PhotocraftApp) -> bool {
    app.ui.svg_export.is_pending() || app.ui.pdf_export.is_pending()
}
pub(crate) fn worker_frame(ctx: &egui::Context) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("export-worker-frame"), frame));
}
pub(crate) fn settings_hidden(app: &PhotocraftApp, ctx: &egui::Context) -> bool {
    pending(app) || ctx.data(|data| data.get_temp::<u64>(egui::Id::new("export-worker-frame"))) == Some(ctx.cumulative_frame_nr())
}
pub(crate) fn complete(app: &mut PhotocraftApp) {
    app.ui.dialogs.retain(|d| !d.fields.contains_key("__export"));
    app.ui.svg_export.cancel();
    app.ui.pdf_export.cancel();
}
pub(crate) fn cancel(app: &mut PhotocraftApp) {
    complete(app);
}
fn capture_current(app: &PhotocraftApp, f: &Map<String, Value>) -> Result<(), String> {
    if let Some(document) = f.get("__document").and_then(Value::as_u64) {
        let active = app.session.active().ok_or(tl!("Export document is closed; reopen Export As"))?;
        if active.doc.id.0 != document || Some(active.revision) != f.get("__revision").and_then(Value::as_u64) {
            return Err(tl!("Export document changed; reopen Export As").into());
        }
    }
    Ok(())
}

/// Layer › Export As…: the same dialog for just the active layer (trimmed to its pixels).
pub fn open_layer(app: &mut PhotocraftApp, layer: photocraft_doc::LayerId) -> Result<u64, String> {
    let st = app.session.active().ok_or("no document")?;
    let ldoc = photocraft_engine::layer_menu_cmds::layer_document(&st.doc, layer).map_err(|e| e.to_string())?;
    let id = open(app)?;
    if let Some(d) = app.ui.dialogs.iter_mut().find(|d| d.id == id) {
        d.fields.insert("__layer".into(), json!(layer.0));
        d.fields.insert("__stem".into(), json!(basename(&ldoc.name)));
        if matches!(s_fmt(&d.fields).as_str(), "svg" | "pdf") {
            d.fields.insert("format".into(), json!("png"));
        }
        d.fields.insert("__label".into(), json!(format!("Export As: {}", ldoc.name)));
        d.fields.insert("__w".into(), json!(ldoc.size.width));
        d.fields.insert("__h".into(), json!(ldoc.size.height));
    }
    Ok(id)
}

/// Layer › Quick Export as PNG.
pub fn quick_export_layer_png(app: &mut PhotocraftApp, layer: photocraft_doc::LayerId) -> Result<Value, String> {
    let mut f = Map::new();
    f.insert("format".into(), json!("png"));
    f.insert("transparency".into(), json!(true));
    f.insert("scale".into(), json!(100));
    f.insert("__layer".into(), json!(layer.0));
    confirm(app, &f)
}

/// The document a dialog exports: the whole image, or one layer (`__layer`).
fn source_document_from(doc: Arc<Document>, f: &Map<String, Value>) -> Result<Arc<Document>, String> {
    match f.get("__layer").and_then(Value::as_u64) {
        Some(id) => photocraft_engine::layer_menu_cmds::layer_document(&doc, photocraft_doc::LayerId(id)).map(Arc::new).map_err(|e| e.to_string()),
        None => Ok(doc),
    }
}

fn source_document(app: &PhotocraftApp, f: &Map<String, Value>) -> Result<Arc<Document>, String> {
    source_document_from(app.session.active().ok_or("no document")?.doc.clone(), f)
}

fn s(f: &Map<String, Value>, k: &str) -> String {
    f.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}
fn n(f: &Map<String, Value>, k: &str, d: f64) -> f64 {
    f.get(k).and_then(Value::as_f64).unwrap_or(d)
}

/// The dialog's resample choice (the list Library export shows too; unknown → Bicubic).
fn resample(f: &Map<String, Value>) -> ResampleChoice {
    f.get("resample").and_then(Value::as_str).and_then(ResampleChoice::from_key).unwrap_or(DEFAULT_RESAMPLE)
}

/// The document as exported: scaled (engine Image Size with the chosen kernel; proxies bilinear)
/// and flattened over white when transparency is off or the format has no alpha.
fn export_document(doc: &Document, f: &Map<String, Value>, max_side: Option<u32>) -> Result<Document, String> {
    let mut s = photocraft_engine::Session::new();
    s.add_document(doc.clone(), None);
    let scale = n(f, "scale", 100.0) / 100.0;
    let mut w = (doc.size.width as f64 * scale).round().max(1.0);
    if let Some(m) = max_side {
        let long = doc.size.width.max(doc.size.height) as f64 * scale;
        if long > m as f64 {
            w = (w * m as f64 / long).round().max(1.0);
        }
    }
    if (w - doc.size.width as f64).abs() >= 1.0 {
        let kernel = if max_side.is_some() { ResampleChoice::Bilinear } else { resample(f) };
        s.execute("image.imageSize", json!({"width": w, "resample": kernel.key()})).map_err(|e| e.to_string())?;
    }
    let fmt = s_fmt(f);
    if !f.get("transparency").and_then(Value::as_bool).unwrap_or(true) || fmt == "jpg" {
        s.execute("layer.flattenImage", json!({})).map_err(|e| e.to_string())?;
    }
    s.active().map(|d| (*d.doc).clone()).ok_or_else(|| "export failed".into())
}

fn s_fmt(f: &Map<String, Value>) -> String {
    let v = s(f, "format");
    if v.is_empty() { "png".into() } else { v }
}

fn lossless(f: &Map<String, Value>) -> bool {
    f.get("lossless").and_then(Value::as_bool).unwrap_or(false)
}

fn settings(f: &Map<String, Value>) -> ExportSettings {
    let fmt = s_fmt(f);
    let quality = n(f, "quality", 85.0).clamp(1.0, 100.0) as u8;
    ExportSettings {
        jpeg_quality: (fmt == "jpg").then_some(quality),
        webp_lossless: fmt != "webp" || lossless(f),
        webp_quality: (fmt == "webp" && !lossless(f)).then_some(quality),
        xmp_all: s(f, "metadata") == "all",
        ..Default::default()
    }
}

/// The preview and the size estimate share one reduced document: resizing every layer twice is
/// what froze the dialog on large layered files.
fn preview_proxy(doc: &Document, f: &Map<String, Value>) -> Result<Document, String> {
    export_document(doc, f, Some(512))
}

/// File size at full scale from the encoded size of the ≤512 px `proxy`, by pixel count.
fn scaled_estimate(bytes: usize, doc: &Document, proxy: &Document, f: &Map<String, Value>) -> u64 {
    let scale = n(f, "scale", 100.0) / 100.0;
    let full = doc.size.width as f64 * scale * doc.size.height as f64 * scale;
    let small = (proxy.size.width as f64 * proxy.size.height as f64).max(1.0);
    (bytes as f64 * full / small) as u64
}

fn estimate(export: Option<&crate::ExportFn>, doc: &Document, proxy: &Document, f: &Map<String, Value>) -> Option<u64> {
    let export = export?;
    let (bytes, _) = export(proxy, &format!("estimate.{}", s_fmt(f)), &settings(f)).ok()?;
    Some(scaled_estimate(bytes.len(), doc, proxy, f))
}

/// What the dialog shows for one set of options: the thumbnail's pixels and the size estimate.
struct PreviewPixels {
    estimate: Option<u64>,
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

fn preview_pixels(proxy: &Document, estimate: Option<u64>) -> PreviewPixels {
    let img = photocraft_compose::thumbnail(proxy, 360);
    PreviewPixels { estimate, width: img.width as usize, height: img.height as usize, rgba: img.pixels }
}

fn preview_sync(app: &PhotocraftApp, doc: &Document, f: &Map<String, Value>) -> Result<PreviewPixels, String> {
    let proxy = preview_proxy(doc, f)?;
    Ok(preview_pixels(&proxy, estimate(app.services.export.as_ref(), doc, &proxy, f)))
}

#[derive(Clone)]
struct CachedPreview {
    signature: String,
    estimate: Option<u64>,
    /// `None` when the preview could not be made.
    texture: Option<Arc<egui::TextureHandle>>,
}

fn cache_preview(ui: &mut egui::Ui, key: egui::Id, signature: String, result: Result<PreviewPixels, String>) -> CachedPreview {
    let preview = match result {
        Ok(pixels) => {
            let image = egui::ColorImage::from_rgba_unmultiplied([pixels.width, pixels.height], &pixels.rgba);
            let texture = Arc::new(ui.ctx().load_texture("export-preview", image, egui::TextureOptions::LINEAR));
            CachedPreview { signature, estimate: pixels.estimate, texture: Some(texture) }
        }
        Err(error) => {
            log::warn!("Export As preview failed: {error}");
            CachedPreview { signature, estimate: None, texture: None }
        }
    };
    ui.data_mut(|d| d.insert_temp(key, preview.clone()));
    preview
}

#[derive(Clone)]
struct PendingPreview {
    key: egui::Id,
    signature: String,
    result: Arc<Mutex<Option<Result<PreviewPixels, String>>>>,
}

/// Many layers or many pixels: reducing the document is too slow for a UI frame.
fn background_preview_needed(doc: &Document) -> bool {
    doc.walk().len() > 32 || u64::from(doc.size.width) * u64::from(doc.size.height) > 16_000_000
}

/// The result of the document's running preview job once it is done (`None` while it runs or
/// has just been started). One job per document, so dragging a slider doesn't start a worker per
/// tick; a finished job for options that have since changed is dropped and a new one starts.
fn poll_or_start_preview(
    ui: &mut egui::Ui,
    key: egui::Id,
    signature: &str,
    doc: Arc<Document>,
    fields: &Map<String, Value>,
    export: Option<crate::ExportFn>,
) -> Option<Result<PreviewPixels, String>> {
    let pending_key = egui::Id::new(("export-preview-pending", doc.id.0));
    let pending: Option<PendingPreview> = ui.data(|d| d.get_temp(pending_key));
    if let Some(pending) = pending {
        let result = pending.result.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()?;
        ui.data_mut(|d| d.remove::<PendingPreview>(pending_key));
        if pending.key == key && pending.signature == signature {
            return Some(result);
        }
    }
    let result = Arc::new(Mutex::new(None));
    let shared = result.clone();
    let ctx = ui.ctx().clone();
    let fields = fields.clone();
    let started = std::thread::Builder::new().name("export-preview".into()).spawn(move || {
        let prepared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let source = source_document_from(doc.clone(), &fields).unwrap_or(doc);
            let proxy = preview_proxy(&source, &fields)?;
            let estimate = estimate(export.as_ref(), &source, &proxy, &fields);
            Ok::<_, String>(preview_pixels(&proxy, estimate))
        }))
        .unwrap_or_else(|_| Err("preview worker panicked".into()));
        *shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(prepared);
        ctx.request_repaint();
    });
    match started {
        Ok(_) => {
            ui.data_mut(|d| d.insert_temp(pending_key, PendingPreview { key, signature: signature.to_string(), result }));
            None
        }
        Err(error) => Some(Err(format!("could not start preview worker: {error}"))),
    }
}

pub fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let Some(doc) = app.session.active().map(|s| s.doc.clone()) else { return };
    ui.horizontal(|ui| {
        ui.label(tl!("Format"));
        let mut fmt = s_fmt(f);
        let formats = if f.contains_key("__layer") { &FORMATS[..5] } else { &FORMATS[..] };
        let opts: Vec<(String, &str)> = formats.iter().map(|(key, label)| ((*key).to_owned(), *label)).collect();
        if crate::widgets::dropdown(ui, "export-format", &mut fmt, &opts, 160.0) {
            f.insert("format".into(), json!(fmt));
        }
    });
    ui.weak(format!("{}.{}", s(f, "__stem"), s_fmt(f)));
    if let Err(error) = capture_current(app, f) {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    match s_fmt(f).as_str() {
        "svg" => {
            egui::ScrollArea::vertical().max_height((ui.ctx().content_rect().height() - 200.0).max(100.0)).show(ui, |ui| {
                crate::vector_packages::svg_export::settings(ui, &mut app.ui.svg_export, |text| li_i18n::t(text).to_owned());
            });
            return;
        }
        "pdf" => {
            egui::ScrollArea::vertical().max_height((ui.ctx().content_rect().height() - 200.0).max(100.0)).show(ui, |ui| {
                crate::vector_packages::pdf_export::settings(ui, &mut app.ui.pdf_export, |text| li_i18n::t(text).to_owned());
            });
            return;
        }
        _ => {}
    }
    ui.horizontal_top(|ui| {
        // Left: settings.
        ui.vertical(|ui| {
            ui.set_width(220.0);
            ui.label(egui::RichText::new(tl!("File Settings")).font(crate::theme::semibold(12.0)).color(t.text));
            let fmt = s_fmt(f);
            if fmt == "webp" {
                let mut ll = lossless(f);
                crate::widgets::checkbox(ui, &mut ll, tl!("Lossless"));
                f.insert("lossless".into(), json!(ll));
            }
            if fmt == "jpg" || (fmt == "webp" && !lossless(f)) {
                let mut q = n(f, "quality", 85.0) as f32;
                crate::widgets::slider_row(ui, tl!("Quality"), &mut q, 1.0..=100.0, "%", None);
                f.insert("quality".into(), json!(q.round()));
            }
            if fmt != "jpg" {
                let mut tr = f.get("transparency").and_then(Value::as_bool).unwrap_or(true);
                crate::widgets::checkbox(ui, &mut tr, tl!("Transparency"));
                f.insert("transparency".into(), json!(tr));
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tl!("Metadata")).color(t.text_dim));
                let mut m = s(f, "metadata");
                let opts: Vec<(String, &str)> = vec![("none".into(), tl!("None")), ("all".into(), tl!("All"))];
                if crate::widgets::dropdown(ui, "export-metadata", &mut m, &opts, 130.0) {
                    f.insert("metadata".into(), json!(m));
                }
            });
            ui.add_space(8.0);
            ui.label(egui::RichText::new(tl!("Image Size")).font(crate::theme::semibold(12.0)).color(t.text));
            let mut sc = n(f, "scale", 100.0) as f32;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tl!("Scale")).color(t.text_dim));
                crate::widgets::value_field(ui, &mut sc, 1.0..=1000.0, "%", 70.0);
            });
            f.insert("scale".into(), json!(sc.round()));
            let (bw, bh) = (n(f, "__w", f64::from(doc.size.width)) as f32, n(f, "__h", f64::from(doc.size.height)) as f32);
            let (w, h) = ((bw * sc / 100.0).round(), (bh * sc / 100.0).round());
            ui.label(egui::RichText::new(format!("{w} × {h} px")).color(t.text_dim).size(11.5));
            ui.label(egui::RichText::new(tl!("Resample")).color(t.text_dim));
            let mut kernel = resample(f).key().to_owned();
            let kernels: Vec<(String, &str, &str)> = ResampleChoice::ALL.iter().map(|c| (c.key().to_owned(), c.label(), c.hint())).collect();
            if crate::widgets::dropdown_with_tooltips(ui, "export-resample", &mut kernel, &kernels, 210.0) {
                f.insert("resample".into(), json!(kernel));
            }
        });
        ui.add_space(12.0);
        // Right: preview + estimated size.
        ui.vertical(|ui| {
            let layer = f.get("__layer").and_then(Value::as_u64);
            let key = egui::Id::new(("export-preview", doc.id.0, app.session.active().map_or(0, |s| s.revision), layer));
            let sig = format!(
                "{}{}{}{}{}{}",
                s_fmt(f),
                n(f, "quality", 85.0),
                lossless(f),
                f.get("transparency").map(|v| v.to_string()).unwrap_or_default(),
                n(f, "scale", 100.0),
                s(f, "metadata")
            );
            let cached: Option<CachedPreview> = ui.data(|d| d.get_temp(key));
            let preview = match cached.filter(|cached| cached.signature == sig) {
                Some(cached) => Some(cached),
                None => {
                    let result = if background_preview_needed(&doc) {
                        poll_or_start_preview(ui, key, &sig, doc.clone(), f, app.services.export.clone())
                    } else {
                        let source = source_document(app, f).unwrap_or_else(|_| doc.clone());
                        Some(preview_sync(app, &source, f))
                    };
                    result.map(|result| cache_preview(ui, key, sig, result))
                }
            };
            let side = 300.0;
            let (r, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            crate::widgets::checker(ui.painter(), r, 8.0);
            if let Some(tex) = preview.as_ref().and_then(|preview| preview.texture.as_ref()) {
                let ts = tex.size_vec2();
                let k = (side / ts.x.max(ts.y)).min(1.0e3);
                let ir = egui::Rect::from_center_size(r.center(), ts * k);
                ui.painter().image(tex.id(), ir, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            } else if preview.is_none() {
                ui.put(egui::Rect::from_center_size(r.center(), egui::vec2(24.0, 24.0)), egui::Spinner::new());
            } else {
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, tl!("Preview"), egui::FontId::proportional(12.0), t.text_dim);
            }
            ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0, t.separator), egui::StrokeKind::Outside);
            let est = preview.as_ref().and_then(|preview| preview.estimate).map_or("—".to_string(), |b| crate::sizing::human_bytes(b as f64));
            ui.label(egui::RichText::new(format!("{}  ≈ {est}", s_fmt(f).to_uppercase())).color(t.text_dim).size(11.5));
        });
    });
}

/// Export with the dialog's settings: choose a path, render, encode, write.
pub fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    if pending(app) {
        return Err(tl!("An export is already running; finish or cancel it first").into());
    }
    capture_current(app, f)?;
    let preferences = f.contains_key("__export").then(|| remembered(app, f));
    let doc = source_document(app, f)?;
    let stem = f.get("__stem").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| basename(&doc.name));
    let ext = s_fmt(f);
    if !FORMATS.iter().any(|(format, _)| *format == ext) {
        return Err(tl!("Unsupported export format").into());
    }
    let suggested = format!("{stem}.{ext}");
    if matches!(ext.as_str(), "svg" | "pdf") {
        if f.contains_key("__layer") {
            return Err(tl!("Use document Export As for SVG/PDF selection export").into());
        }
        return if ext == "svg" {
            crate::vector_packages::svg_export::begin_embedded(app, &suggested, preferences)
        } else {
            crate::vector_packages::pdf_export::begin_embedded(app, &suggested, preferences)
        };
    }
    let f = f.clone();
    app.pick_save(&suggested, move |app, path| {
        capture_current(app, &f)?;
        let out = export_document(&doc, &f, None)?;
        let export = app.services.export.as_ref().ok_or("no exporter configured")?;
        let (bytes, warnings) = export(&out, &path, &settings(&f)).map_err(|error| format!("Could not export {path}: {error}"))?;
        let write = app.services.write.as_mut().ok_or("no writer configured")?;
        write(&path, &bytes).map_err(|error| format!("Could not export {path}: {error}"))?;
        if let Some(preferences) = preferences {
            publish_preferences(app, preferences);
        }
        crate::notices::file_result(app, tl!("Exported"), &path, &warnings);
        Ok(json!({"path": path, "bytes": bytes.len(), "warnings": warnings}))
    })
}

/// File › Export › Quick Export as PNG: the format, quality, metadata, colour space and location
/// from File › Export › Export Preferences (engine `file.export.quickExport`). On the web (no
/// file system) it falls back to a PNG download through the export service.
pub fn quick_export_png(app: &mut PhotocraftApp) -> Result<Value, String> {
    if !cfg!(target_arch = "wasm32") {
        let prefs = app.session.prefs().export.clone();
        let fmt = serde_json::to_value(prefs.quick_export_format).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "png".into());
        let same = serde_json::to_value(prefs.quick_export_location).ok().is_some_and(|v| v == "sameFolder");
        let saved = app.session.active().is_some_and(|d| d.path.is_some());
        if same && saved {
            return quick_export(app, json!({}));
        }
        let st = app.session.active().ok_or("no document")?;
        let stem = st.doc.name.rsplit_once('.').map_or(st.doc.name.as_str(), |(a, _)| a);
        let (suggested, doc) = (format!("{stem}.{fmt}"), st.doc.id);
        return app.pick_save(&suggested, move |app, path| {
            app.refocus(doc)?;
            quick_export(app, json!({"path": path}))
        });
    }
    let mut f = Map::new();
    f.insert("format".into(), json!("png"));
    f.insert("transparency".into(), json!(true));
    f.insert("scale".into(), json!(100));
    confirm(app, &f)
}

/// Runs Quick Export with `p` (an explicit `path`, or none for the document's folder).
fn quick_export(app: &mut PhotocraftApp, p: Value) -> Result<Value, String> {
    let r = app.run("file.export.quickExport", p)?;
    app.ui.status = format!("Exported {}", r["path"].as_str().unwrap_or_default());
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layered_preview_runs_outside_the_ui_frame() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 32, "height": 16})).unwrap();
        for _ in 0..33 {
            app.run("layer.new.layer", json!({})).unwrap();
        }
        let doc = app.session.active().unwrap().doc.clone();
        assert!(background_preview_needed(&doc));
        let fields = Map::from_iter([("format".into(), json!("png")), ("scale".into(), json!(100))]);
        let ctx = egui::Context::default();
        let key = egui::Id::new("layered-preview-test");
        let mut result = None;
        let poll = |result: &mut Option<Result<PreviewPixels, String>>| {
            let mut output = ctx.run_ui(Default::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    *result = poll_or_start_preview(ui, key, "png100", doc.clone(), &fields, Some(Arc::new(|_, _, _| Ok((vec![0; 123], vec![])))));
                });
            });
            output.textures_delta.clear();
        };
        poll(&mut result);
        assert!(result.is_none(), "the first UI frame must only start the worker");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            poll(&mut result);
            if let Some(preview) = result.take() {
                let preview = preview.unwrap();
                assert_eq!((preview.width, preview.height), (32, 16));
                assert_eq!(preview.rgba.len(), 32 * 16 * 4);
                assert_eq!(preview.estimate, Some(123), "the worker must use the configured exporter");
                break;
            }
            assert!(std::time::Instant::now() < deadline, "background preview did not complete");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn export_document_scales_and_flattens() {
        let doc = Document::with_background(
            "x",
            photocraft_doc::Size::new(200, 100),
            photocraft_doc::ColorMode::Rgb,
            photocraft_doc::SampleType::U8,
            photocraft_doc::Color::WHITE,
        );
        let mut f = Map::new();
        f.insert("format".into(), json!("jpg"));
        f.insert("scale".into(), json!(50));
        let out = export_document(&doc, &f, None).unwrap();
        assert_eq!((out.size.width, out.size.height), (100, 50));
        assert_eq!(out.layers.len(), 1);
        let proxy = export_document(&doc, &f, Some(40)).unwrap();
        assert_eq!(proxy.size.width, 40);
        assert_eq!(settings(&f).jpeg_quality, Some(85));
    }

    #[test]
    fn webp_settings_follow_the_lossless_switch() {
        let mut f = Map::new();
        f.insert("format".into(), json!("webp"));
        f.insert("quality".into(), json!(70));
        let s = settings(&f);
        assert!(!s.webp_lossless, "Export As writes lossy WebP unless asked");
        assert_eq!(s.webp_quality, Some(70));
        assert_eq!(s.jpeg_quality, None);
        f.insert("lossless".into(), json!(true));
        let s = settings(&f);
        assert!(s.webp_lossless);
        assert_eq!(s.webp_quality, None);
        f.insert("format".into(), json!("png"));
        assert!(settings(&f).webp_lossless, "other formats leave the WebP default alone");
    }

    /// Export As offers the shared kernel list; each choice exports exactly what Image Size gives
    /// with that `resample`, and no choice (old prefs, Quick Export) stays plain Bicubic.
    #[test]
    fn export_as_uses_the_chosen_shared_kernel() {
        let mut doc = Document::with_background(
            "x",
            photocraft_doc::Size::new(61, 37),
            photocraft_doc::ColorMode::Rgb,
            photocraft_doc::SampleType::F32,
            photocraft_doc::Color::WHITE,
        );
        let surface = doc.layers[0].surface_mut().unwrap();
        for y in 0..37 {
            for x in 0..61 {
                let v = ((x * 7 + y * 13) % 17) as f32 / 16.0;
                surface.write_pixel(x, y, &[v, x as f32 / 61.0, if (x / 5 + y / 3) % 2 == 0 { 0.9 } else { 0.1 }, 1.0]);
            }
        }
        let pixels = |d: &Document| d.layers[0].surface().unwrap().read_region(photocraft_doc::Rect::new(0, 0, d.size.width as i32, d.size.height as i32));
        let image_size = |resample: &str, w: u32| {
            let mut s = photocraft_engine::Session::new();
            s.add_document(doc.clone(), None);
            s.execute("image.imageSize", json!({"width": w, "resample": resample})).unwrap();
            pixels(&s.active().unwrap().doc)
        };
        let mut f = Map::new();
        f.insert("format".into(), json!("png"));
        for (scale, w) in [(40, 24), (250, 153)] {
            f.insert("scale".into(), json!(scale));
            f.remove("resample");
            assert_eq!(pixels(&export_document(&doc, &f, None).unwrap()), image_size("bicubic", w), "default stays Bicubic");
            let mut looks = Vec::new();
            for c in ResampleChoice::ALL {
                f.insert("resample".into(), json!(c.key()));
                let out = pixels(&export_document(&doc, &f, None).unwrap());
                assert_eq!(out, image_size(c.key(), w), "{c:?} at {scale}%");
                looks.push(out);
            }
            looks.dedup();
            assert!(looks.len() >= 5, "the kernels give different results");
        }
        assert!(KEYS.contains(&"resample"), "the choice is remembered with the other Export As settings");
    }
}

#[cfg(test)]
#[path = "export_dialog/unified_tests.rs"]
mod unified_tests;
