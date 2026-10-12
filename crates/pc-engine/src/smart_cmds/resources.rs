//! Native resources needed by editable smart-object contents.
use super::*;
use photocraft_doc::vector_design::{symbol_validation, validation};
use std::collections::{HashMap, HashSet};

fn walk<'a>(layers: &'a [Layer], out: &mut Vec<&'a Layer>) {
    for layer in layers {
        out.push(layer);
        if let Some(children) = layer.children() {
            walk(children, out);
        }
    }
}

/// The destination is a new document, so IDs can stay unchanged. Preserving both
/// the symbol and layer namespaces also preserves global-swatch targets.
pub(super) fn inherit(source: &Document, sub: &mut Document) -> Result<()> {
    symbol_validation::document(source).map_err(|error| other(error.to_string()))?;
    let masters: HashMap<_, _> = source.symbols.iter().map(|master| (master.id, master)).collect();
    let mut roots = Vec::new();
    walk(&sub.layers, &mut roots);
    let mut pending = roots
        .iter()
        .filter_map(|layer| match &layer.content {
            LayerContent::Symbol(instance) => Some(instance.symbol),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut needed = HashSet::new();
    while let Some(id) = pending.pop() {
        if !needed.insert(id) {
            continue;
        }
        let master = masters.get(&id).ok_or_else(|| other("missing smart-object symbol master"))?;
        let mut layers = Vec::new();
        walk(&master.layers, &mut layers);
        pending.extend(layers.into_iter().filter_map(|layer| match &layer.content {
            LayerContent::Symbol(instance) => Some(instance.symbol),
            _ => None,
        }));
    }
    sub.symbols = source.symbols.iter().filter(|master| needed.contains(&master.id)).cloned().collect();
    let mut targets = roots.iter().map(|layer| (None, layer.id)).collect::<HashSet<_>>();
    let mut styles = roots.iter().filter_map(|layer| layer.vector.graphic_style.as_ref().map(|link| link.style)).collect::<HashSet<_>>();
    for master in &sub.symbols {
        let mut layers = Vec::new();
        walk(&master.layers, &mut layers);
        for layer in layers {
            targets.insert((Some(master.id), layer.id));
            if let Some(link) = &layer.vector.graphic_style {
                styles.insert(link.style);
            }
        }
    }
    sub.graphic_styles = source.graphic_styles.iter().filter(|style| styles.contains(&style.id)).cloned().collect();
    sub.swatches.links = source.swatches.links.iter().filter(|link| targets.contains(&(link.target.symbol, link.target.layer))).cloned().collect();
    let mut swatches = sub.swatches.links.iter().map(|link| link.reference.swatch).collect::<HashSet<_>>();
    loop {
        let before = swatches.len();
        for swatch in &source.swatches.swatches {
            if swatches.contains(&swatch.id) {
                swatches.extend(swatch.stops.iter().map(|stop| stop.reference.swatch));
            }
        }
        if swatches.len() == before {
            break;
        }
    }
    sub.swatches.swatches = source.swatches.swatches.iter().filter(|swatch| swatches.contains(&swatch.id)).cloned().collect();
    let groups = sub.swatches.swatches.iter().filter_map(|swatch| swatch.group).collect::<HashSet<_>>();
    sub.swatches.groups = source.swatches.groups.iter().filter(|group| groups.contains(&group.id)).cloned().collect();
    // Pattern lookup can use ID or name from fills, appearances and effect textures.
    // Named type styles also retain their shared PSD style-sheet context.
    sub.patterns = source.patterns.clone();
    sub.text_styles = source.text_styles.clone();
    sub.color_table = source.color_table.clone();
    sub.duotone = source.duotone.clone();
    sub.drawing_scale = source.drawing_scale;
    sub.design_units = source.design_units;
    symbol_validation::document(sub).map_err(|error| other(error.to_string()))?;
    validation::document(sub).map_err(other)?;
    Ok(())
}

/// Editable vector/type content and document-relative paints use the original
/// canvas as their coordinate frame. Keep that frame rather than rebasing only
/// their cached pixels. Ordinary raster contents retain the compact crop path.
pub(super) fn keep_canvas(layer: &Layer) -> bool {
    if layer.effects.reference.is_some()
        || !layer.vector.is_empty()
        || matches!(layer.content, LayerContent::Shape(_) | LayerContent::Text(_) | LayerContent::Symbol(_) | LayerContent::Fill(_))
    {
        return true;
    }
    if layer.effects.items.iter().any(|effect| match effect {
        photocraft_doc::Effect::GradientOverlay { gradient, .. } => !gradient.align,
        photocraft_doc::Effect::PatternOverlay { link, .. } => !link,
        photocraft_doc::Effect::BevelEmboss(bevel) => bevel.texture.as_ref().is_some_and(|texture| !texture.link),
        photocraft_doc::Effect::Stroke(stroke) => paint_uses_canvas(&stroke.paint),
        photocraft_doc::Effect::InnerGlow(glow) | photocraft_doc::Effect::OuterGlow(glow) => paint_uses_canvas(&glow.paint),
        _ => false,
    }) {
        return true;
    }
    layer.children().is_some_and(|children| children.iter().any(keep_canvas))
}

fn paint_uses_canvas(paint: &photocraft_doc::FxPaint) -> bool {
    match paint {
        photocraft_doc::FxPaint::Gradient(gradient) => !gradient.align,
        photocraft_doc::FxPaint::Pattern { .. } => true,
        _ => false,
    }
}
