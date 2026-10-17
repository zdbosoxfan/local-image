//! Display/export and serde bridge to the additive live-paint data variant.
//! The bridge fails explicitly rather than guessing. It never
//! stores paint in names, caches, a global map or an unrelated live-group variant.
use super::{Graph, LivePaintSpec, Result, error};
use photocraft_doc::vector_design::exchange::ImportedScene;
use photocraft_doc::{Document, Layer, LayerContent, LayerId};

pub fn read_spec(layer: &Layer) -> Result<Option<LivePaintSpec>> {
    let Some(live) = &layer.vector.live else { return Ok(None) };
    let mut value = serde_json::to_value(live).map_err(error)?;
    if value["kind"] != "live_paint" {
        return Ok(None);
    }
    value.as_object_mut().ok_or_else(|| error("invalid live paint metadata"))?.remove("kind");
    let spec: LivePaintSpec = serde_json::from_value(value).map_err(error)?;
    spec.validate()?;
    Ok(Some(spec))
}
pub fn write_spec(layer: &mut Layer, spec: &LivePaintSpec) -> Result<()> {
    spec.validate()?;
    let mut value = serde_json::to_value(spec).map_err(error)?;
    value.as_object_mut().ok_or_else(|| error("invalid live paint metadata"))?.insert("kind".into(), "live_paint".into());
    layer.vector.live = Some(serde_json::from_value(value).map_err(|e| error(format!("00 LiveGroup::LivePaint data integration required: {e}")))?);
    Ok(())
}
pub fn project_document(source: &Document) -> Result<Document> {
    let mut out = source.clone();
    let mut next = crate::blend::max_document_id(source);
    let mut count = 0usize;
    fn walk(layers: &mut [Layer], depth: usize, next: &mut u64, count: &mut usize) -> Result<()> {
        if depth > 16 {
            return Err(error("live paint hierarchy exceeds depth budget"));
        }
        for layer in layers {
            *count += 1;
            if *count > 16384 {
                return Err(error("live paint projection layer budget exceeded"));
            }
            if let Some(spec) = read_spec(layer)? {
                let LayerContent::Group(group) = &mut layer.content else { return Err(error("live paint metadata requires a group")) };
                group.children = Graph::new(&group.children, spec.gap)?.expand(&group.children, &spec)?;
                layer.vector.live = None;
            }
            if layer.id.0 == 0 {
                *next = next.checked_add(1).ok_or_else(|| error("projection ID overflow"))?;
                layer.id = LayerId(*next);
            }
            if let Some(children) = layer.children_mut() {
                walk(children, depth + 1, next, count)?;
            }
        }
        Ok(())
    }
    walk(&mut out.layers, 0, &mut next, &mut count)?;
    // Established bounds-limited two-pass appearance cache preparation.
    crate::blend::prepare_caches(&mut out).map_err(error)?;
    Ok(out)
}
pub fn project_scene(scene: &ImportedScene, format: photocraft_color::PixelFormat, patterns: &[photocraft_doc::Pattern]) -> Result<ImportedScene> {
    let mut document = Document::new("Live Paint projection", scene.size, format.mode, format.sample);
    document.id = photocraft_doc::DocId(0);
    document.layers = scene.layers.clone();
    document.symbols = scene.symbols.clone();
    document.patterns = patterns.to_vec();
    document.resolution_dpi = scene.resolution_dpi;
    document.design_units = scene.units;
    let mut out = scene.clone();
    out.layers = project_document(&document)?.layers;
    Ok(out)
}
