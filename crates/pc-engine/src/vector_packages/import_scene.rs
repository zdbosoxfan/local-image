use crate::{EngineError, Result};
use photocraft_doc::{
    Affine, ColorMode, Document, Layer, LayerContent, SampleType,
    vector_design::exchange::{ImportedScene, VectorWarning},
};

/// File adapters all return the shared scene; Open wraps it as a new native document.
pub fn document(scene: ImportedScene, name: &str) -> (Document, Vec<VectorWarning>) {
    let mut doc = Document::new(name, scene.size, ColorMode::Rgb, SampleType::F32);
    doc.resolution_dpi = scene.resolution_dpi;
    doc.design_units = scene.units;
    doc.layers = scene.layers;
    doc.symbols = scene.symbols;
    (doc, scene.warnings)
}

pub(super) fn translate(layer: &mut Layer, offset: [i32; 2], format: &str) -> Result<()> {
    let affine = Affine::translate(f64::from(offset[0]), f64::from(offset[1]));
    if let Some(m) = &mut layer.vector_mask {
        m.path = m.path.transform(&affine);
        check_path(&m.path, format)?;
    }
    if let Some(p) = &mut layer.vector.clip {
        *p = p.transform(&affine);
        check_path(p, format)?;
    }
    if let Some(m) = &mut layer.mask {
        m.surface = m.surface.translated(offset[0], offset[1]);
    }
    match &mut layer.content {
        LayerContent::Shape(shape) => {
            shape.path = shape.path.transform(&affine);
            check_path(&shape.path, format)?;
            shape.cache = shape.cache.as_ref().map(|s| s.translated(offset[0], offset[1]));
        }
        LayerContent::Text(text) => {
            text.transform = affine.mul(&text.transform);
            if text.transform.m.iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.0) {
                return Err(EngineError::Other(format!("{format} text placement exceeds coordinate bounds")));
            }
            text.cache = text.cache.as_ref().map(|s| s.translated(offset[0], offset[1]));
        }
        LayerContent::Raster(surface) => {
            *surface = surface.translated(offset[0], offset[1]);
        }
        LayerContent::Group(group) => {
            for child in &mut group.children {
                translate(child, offset, format)?;
            }
        }
        _ => return Err(EngineError::Other(format!("unexpected content in {format} scene"))),
    }
    Ok(())
}
pub(super) fn check_path(path: &photocraft_doc::Path, format: &str) -> Result<()> {
    crate::vector_api::validate_path(path)?;
    if path.control_bounds().is_some_and(|b| [b.0, b.1, b.2, b.3].iter().any(|v| v.abs() > 1_000_000.0)) {
        return Err(EngineError::Other(format!("{format} placement exceeds coordinate bounds")));
    }
    Ok(())
}
