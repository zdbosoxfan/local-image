//! Snapshot-only document encoding with private font/layout state.
use super::*;
use lightcraft_sysmem::jobs::Interrupt;
use std::cell::RefCell;

fn project(source: &Document, engine: &mut photocraft_text::TextEngine, cancel: &dyn Fn() -> bool) -> Result<Document> {
    fn refresh(source: &Document, layers: &mut [photocraft_doc::Layer], engine: &mut photocraft_text::TextEngine, cancel: &dyn Fn() -> bool) -> Result<()> {
        for layer in layers {
            if cancel() {
                return Err(EngineError::Cancelled);
            }
            if let Some(children) = layer.children_mut() {
                refresh(source, children, engine, cancel)?;
            }
            if let LayerContent::Text(text) = &mut layer.content {
                if let Some(path) = &layer.vector.on_path {
                    let layout = packages::path_text::project(engine, source, text, path)?;
                    text.cache = Some(
                        photocraft_text::on_path::rasterize(&layout, &text.transform, source.pixel_format(), text.antialias, source.bounds())
                            .map_err(|e| error(e.to_string()))?
                            .surface,
                    );
                    text.psd_raw = None;
                } else if frame_options(&layer.vector).enabled {
                    let composition = photocraft_text::frame_compose::compose(engine, text, source.resolution_dpi, frame_options(&layer.vector))
                        .map_err(|e| error(e.to_string()))?;
                    text.cache = Some(
                        photocraft_text::frame_compose::render_bounded(&composition, text, source.pixel_format(), source.bounds())
                            .map_err(|e| error(e.to_string()))?
                            .surface,
                    );
                    text.psd_raw = Some(std::sync::Arc::new(photocraft_text::psd::build_tysh(text, source.resolution_dpi, composition.layout.bounds())));
                } else if text.cache.is_none() {
                    let (layout, rendered) = engine.render(text, source.resolution_dpi, source.pixel_format());
                    text.cache = Some(rendered.surface);
                    text.psd_raw = Some(std::sync::Arc::new(photocraft_text::psd::build_tysh(text, source.resolution_dpi, layout.bounds())));
                }
            }
        }
        Ok(())
    }
    let mut output = project_document_cancellable(source, cancel, false)?;
    refresh(source, &mut output.layers, engine, cancel)?;
    if cancel() {
        return Err(EngineError::Cancelled);
    }
    Ok(output)
}
struct Projection<'a> {
    text: RefCell<photocraft_text::TextEngine>,
    cancel: &'a dyn Fn() -> bool,
}
impl photocraft_io::vector_map::fallback::Projection for Projection<'_> {
    fn layer(&self, doc: &Document, layer: &photocraft_doc::Layer) -> std::result::Result<Option<photocraft_doc::Layer>, String> {
        let mut engine = self.text.try_borrow_mut().map_err(|_| "recursive save projection".to_string())?;
        save_layer_projection(doc, layer, |source| project(source, &mut engine, self.cancel))
    }
}

/// Encode on a worker. Projection cooperates with cancellation; indivisible codec calls
/// may finish their current phase, but callers must discard cancelled output before staging.
pub fn export_document_background(
    source: &Document,
    name: &str,
    options: &photocraft_io::ExportOptions,
    interrupt: &Interrupt<'_>,
) -> std::result::Result<photocraft_io::ExportResult, photocraft_io::IoError> {
    use photocraft_io::IoError;
    let cancel = || interrupt.cancelled();
    let map = |e: EngineError| if cancel() { IoError::Cancelled } else { IoError::Unsupported(e.to_string()) };
    if cancel() {
        return Err(IoError::Cancelled);
    }
    let mut text = packages::vector_export_job::font_snapshot(&cancel).map_err(map)?;
    interrupt.progress(0.05);
    let ext = std::path::Path::new(name).extension().and_then(|s| s.to_str()).unwrap_or(name).to_ascii_lowercase();
    let result = if matches!(ext.as_str(), "pcraft" | "psd" | "psb") || matches!(ext.as_str(), "tif" | "tiff") && options.tiff_layers {
        let projection = Projection { text: RefCell::new(text), cancel: &cancel };
        photocraft_io::export_with_projection(source, name, options, Some(&projection))
    } else {
        let projected = project(source, &mut text, &cancel).map_err(map)?;
        interrupt.progress(0.25);
        photocraft_io::export(&projected, name, options)
    };
    if cancel() {
        return Err(IoError::Cancelled);
    }
    interrupt.progress(1.0);
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use photocraft_doc::{ColorMode, Layer, SampleType, Size, TextLayer};
    #[test]
    fn background_save_encoder_preserves_flat_pixels_layered_fallback_and_original_text() {
        initialize();
        let mut doc = Document::new("worker type", Size::new(192, 128), ColorMode::Rgb, SampleType::U8);
        let mut layer = Layer::new(
            "path type",
            LayerContent::Text(TextLayer { text: "Snapshot type".into(), font_family: "Inter".into(), size_pt: 16., ..Default::default() }),
        );
        layer.vector.on_path = Some(photocraft_doc::vector_design::text::PathText {
            path: photocraft_vector::shapes::rect(12., 26., 155., 60.),
            start: 0.,
            end: 1.,
            flip: false,
            align: photocraft_doc::vector_design::text::PathTextAlign::Baseline,
        });
        if let LayerContent::Text(text) = &mut layer.content {
            refresh_text(&doc, text, &layer.vector).unwrap();
        }
        doc.layers.push(layer);
        let before = doc.clone();
        for name in ["snapshot.png", "snapshot.psd", "snapshot.tif", "snapshot.pcraft"] {
            let opts = photocraft_io::ExportOptions::default();
            let expected = export_document(&doc, name, &opts).unwrap();
            let actual = export_document_background(&doc, name, &opts, &Interrupt::NONE).unwrap();
            assert_eq!(actual.warnings, expected.warnings, "{name}");
            let expected = photocraft_io::import(name, &expected.bytes).unwrap().document;
            let actual = photocraft_io::import(name, &actual.bytes).unwrap().document;
            assert_eq!(photocraft_compose::flatten(&actual).to_rgba8().pixels, photocraft_compose::flatten(&expected).to_rgba8().pixels, "{name}");
            assert_eq!(actual.layers.len(), expected.layers.len());
            assert!(doc == before, "background export changed source state");
        }
    }
    #[test]
    fn background_save_encoder_cancel_is_observed_before_encoding() {
        initialize();
        let doc = Document::new("cancel", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
        assert!(matches!(
            export_document_background(&doc, "cancel.psd", &Default::default(), &Interrupt::cancel_only(&|| true)),
            Err(photocraft_io::IoError::Cancelled)
        ));
    }
}
