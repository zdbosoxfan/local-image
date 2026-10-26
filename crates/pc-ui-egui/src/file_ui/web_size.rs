//! Save for Web's Image Size, as in Photoshop: Width and Height in pixels, linked in proportion,
//! Percent and the resampling method.
//!
//! The fields are `file.export.saveForWebLegacy` parameters, so the control channel drives them
//! with `ui.dialog.set`:
//! - `width`, `height`: the exported size in pixels.
//! - `percent`: a scale of the document, which the engine applies before `width` and `height`.
//! - `resample`: any [`ResampleChoice`] key, `bicubic` by default; remembered with the encoding
//!   choices.
//! - `__constrain`: links the two sides in proportion. True at each opening and never sent to the
//!   engine.
//!
//! A side edit sets both sides and removes `percent`; a Percent edit sets both sides from it.
//! Each opening starts at the document's full size. The document itself is never resized.

use li_raster::resample::ResampleChoice;

use super::*;

/// Largest side Save for Web asks Image Size for: the cap `image.imageSize` enforces.
const MAX_PX: f64 = 300_000.0;

/// A pixel size and the percent scale of the side it was set from.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ExportSize {
    width: f64,
    height: f64,
    scale: f64,
}

/// Each side scaled and rounded on its own, never below 1 px.
fn scaled_size(w: f64, h: f64, scale: f64) -> (f64, f64) {
    ((w * scale).round().max(1.0), (h * scale).round().max(1.0))
}

fn finite_px(v: f64) -> Result<f64, String> {
    if !v.is_finite() || !(1.0..=MAX_PX).contains(&v) {
        return Err(format!("export size must be within 1..={MAX_PX} pixels"));
    }
    Ok(v.round())
}

/// Percent scale applied to both sides. `scale` is a percent in 1..=1000.
fn resize_from_scale(src_w: f64, src_h: f64, scale: f64) -> Result<ExportSize, String> {
    if !scale.is_finite() || !(1.0..=1000.0).contains(&scale) {
        return Err("scale must be within 1..=1000 percent".into());
    }
    let (width, height) = scaled_size(src_w.max(1.0), src_h.max(1.0), scale / 100.0);
    if width > MAX_PX || height > MAX_PX {
        return Err(format!("export size must be within 1..={MAX_PX} pixels"));
    }
    Ok(ExportSize { width, height, scale })
}

/// An edited side. With `lock`, the other side keeps the source aspect; otherwise it stays.
fn resize_from_side(src_w: f64, src_h: f64, width: f64, height: f64, lock: bool, edited_width: bool) -> Result<ExportSize, String> {
    let src_w = src_w.max(1.0);
    let src_h = src_h.max(1.0);
    let (width, height) = if edited_width {
        let width = finite_px(width)?;
        let height = if lock { finite_px(src_h * width / src_w)? } else { finite_px(height)? };
        (width, height)
    } else {
        let height = finite_px(height)?;
        let width = if lock { finite_px(src_w * height / src_h)? } else { finite_px(width)? };
        (width, height)
    };
    let scale = if edited_width { width / src_w * 100.0 } else { height / src_h * 100.0 };
    Ok(ExportSize { width, height, scale })
}

/// The exported size in pixels, by the precedence `web_cmds::web_document` applies: `percent`,
/// then `width` and `height` together, then one side in proportion, then the full size.
pub(super) fn web_dimensions(doc: &photocraft_doc::Document, p: &Value) -> Result<(f64, f64), String> {
    let (w0, h0) = (f64::from(doc.size.width).max(1.0), f64::from(doc.size.height).max(1.0));
    let positive = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite() && *v > 0.0);
    let (w, h) = match (positive("percent"), positive("width"), positive("height")) {
        (Some(pct), _, _) => (w0 * pct / 100.0, h0 * pct / 100.0),
        (_, Some(w), Some(h)) => (w, h),
        (_, Some(w), None) => (w, h0 * w / w0),
        (_, None, Some(h)) => (w0 * h / h0, h),
        _ => (w0, h0),
    };
    if !w.is_finite() || !h.is_finite() || w > MAX_PX || h > MAX_PX {
        return Err("The export size is over the 300,000 px per-side limit. Choose a smaller Width, Height or Percent.".into());
    }
    Ok((w.round().max(1.0), h.round().max(1.0)))
}

/// The method a preview proxy is resampled with: the export's own when the proxy is at full
/// size, so the preview and the size estimate show it; bilinear for a reduced proxy.
pub(super) fn proxy_resample(p: &Value, scale: f64) -> &str {
    if scale >= 1.0 { p.get("resample").and_then(Value::as_str).unwrap_or("bicubic") } else { "bilinear" }
}

/// A size row in a two-column grid: the label, then a field showing `shown` and named by the
/// label. Returns the typed value when it changed, compared with the value the widget was given,
/// so a field that only clamps does not count as an edit.
fn size_field(ui: &mut egui::Ui, label: &str, shown: f64, range: std::ops::RangeInclusive<f32>, unit: &str, decimals: u32) -> Option<f64> {
    let t = Tokens::get(ui.ctx());
    let passed = (shown as f32).clamp(*range.start(), *range.end());
    let mut v = passed;
    let label = ui.label(egui::RichText::new(label).color(t.text_dim));
    crate::widgets::value_field_prec(ui, &mut v, range, unit, 90.0, decimals).labelled_by(label.id);
    ui.end_row();
    (v != passed).then_some(f64::from(v))
}

/// Image Size: Width and Height linked in proportion (on at each opening), Percent and the
/// resampling method, from the shared list Export As and Library export offer.
pub(super) fn web_size(ui: &mut egui::Ui, f: &mut Map<String, Value>, doc: &photocraft_doc::Document) {
    let t = Tokens::get(ui.ctx());
    let (w0, h0) = (f64::from(doc.size.width).max(1.0), f64::from(doc.size.height).max(1.0));
    let (w, h) = web_dimensions(doc, &params(f)).unwrap_or((w0, h0));
    let lock = b(f, "__constrain", true);
    let pct = n(f, "percent", w / w0 * 100.0);
    let write = |f: &mut Map<String, Value>, size: ExportSize| {
        f.insert("width".into(), json!(size.width));
        f.insert("height".into(), json!(size.height));
    };
    let max = MAX_PX as f32;
    let (width, height, percent) = egui::Grid::new("web-size")
        .num_columns(2)
        .show(ui, |ui| {
            (
                size_field(ui, tl!("Width"), w, 1.0..=max, "px", 0),
                size_field(ui, tl!("Height"), h, 1.0..=max, "px", 0),
                size_field(ui, tl!("Percent"), pct, 1.0..=1000.0, "%", 2),
            )
        })
        .inner;
    let side = match (width, height) {
        (Some(v), _) => resize_from_side(w0, h0, v.round(), h, lock, true).ok(),
        (_, Some(v)) => resize_from_side(w0, h0, w, v.round(), lock, false).ok(),
        _ => None,
    };
    if let Some(size) = side {
        f.remove("percent");
        write(f, size);
    } else if let Some(v) = percent
        && let Ok(size) = resize_from_scale(w0, h0, v)
    {
        f.insert("percent".into(), json!(size.scale));
        write(f, size);
    }
    let mut linked = lock;
    if crate::widgets::checkbox(ui, &mut linked, tl!("Constrain proportions")).changed() {
        f.insert("__constrain".into(), json!(linked));
        // Linked again, the height follows the width in the document's proportions.
        if linked && let Ok(size) = resize_from_side(w0, h0, w, h, true, true) {
            f.remove("percent");
            write(f, size);
        }
    }
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Resample")).color(t.text_dim));
        let current = f.get("resample").and_then(Value::as_str).and_then(ResampleChoice::from_key).unwrap_or(ResampleChoice::Bicubic);
        let mut kernel = current.key().to_owned();
        let kernels: Vec<(String, &str, &str)> = ResampleChoice::ALL.iter().map(|c| (c.key().to_owned(), c.label(), c.hint())).collect();
        if crate::widgets::dropdown_with_tooltips(ui, "web-resample", &mut kernel, &kernels, 170.0) {
            f.insert("resample".into(), json!(kernel));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_and_percent_edits_keep_the_aspect_only_when_linked() {
        let linked = resize_from_scale(400.0, 200.0, 50.0).unwrap();
        assert_eq!((linked.width, linked.height, linked.scale), (200.0, 100.0, 50.0));
        let from_width = resize_from_side(400.0, 200.0, 100.0, 200.0, true, true).unwrap();
        assert_eq!((from_width.width, from_width.height), (100.0, 50.0));
        let from_height = resize_from_side(400.0, 200.0, 100.0, 40.0, true, false).unwrap();
        assert_eq!((from_height.width, from_height.height), (80.0, 40.0));
        let free = resize_from_side(400.0, 200.0, 100.0, 80.0, false, true).unwrap();
        assert_eq!((free.width, free.height), (100.0, 80.0));
        assert!(resize_from_scale(400.0, 200.0, 0.0).is_err());
        assert!(resize_from_side(400.0, 200.0, 0.0, 10.0, false, true).is_err());
        assert!(resize_from_side(10.0, 10.0, 400_000.0, 10.0, false, true).is_err());
    }

    /// The dialog's size and the engine's export size come from two functions; they must agree.
    #[test]
    fn web_dimensions_follow_the_engine_precedence() {
        let doc = photocraft_doc::Document::with_background(
            "x",
            photocraft_doc::Size::new(40, 20),
            photocraft_doc::ColorMode::Rgb,
            photocraft_doc::SampleType::U8,
            photocraft_doc::Color::WHITE,
        );
        for (p, want) in [
            (json!({"percent": 50, "width": 30, "height": 30}), (20.0, 10.0)),
            (json!({"width": 30, "height": 12}), (30.0, 12.0)),
            (json!({"width": 30}), (30.0, 15.0)),
            (json!({"height": 5}), (10.0, 5.0)),
            (json!({"percent": 0, "width": 8}), (8.0, 4.0)),
            (json!({}), (40.0, 20.0)),
        ] {
            assert_eq!(web_dimensions(&doc, &p), Ok(want), "{p}");
            let (wd, _, _) = web_cmds::web_document(&doc, &p, &WebSettings::default()).unwrap();
            assert_eq!((f64::from(wd.size.width), f64::from(wd.size.height)), want, "engine, {p}");
        }
        assert!(web_dimensions(&doc, &json!({"width": 400_000})).is_err());
        assert!(web_dimensions(&doc, &json!({"percent": 1_000_000})).is_err());
    }
}
