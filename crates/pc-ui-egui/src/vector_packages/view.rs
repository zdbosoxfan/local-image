//! Display integration: re-exports of the scaled-view projection modules.
pub use photocraft_compose::scaled_view as cpu;
pub use photocraft_compose::{Buffer, cmyk_space, effects, fill_frame, gradient_fill, masks, pattern, render_tiled};
pub use photocraft_gpu::scaled_view as gpu;
pub use photocraft_gpu::{ChunkOut, Compositor, Stats, Unsupported};
pub use photocraft_vector::view as vector;
pub use photocraft_vector::{CompiledShape, DEFAULT_TOLERANCE};

pub fn render(doc: &photocraft_doc::Document, rect: photocraft_geom::Rect) -> Buffer {
    photocraft_compose::render(doc, rect)
}

/// Same providers in CPU fallback and GPU projection.
pub fn project(
    doc: &photocraft_doc::Document,
    layer: &photocraft_doc::Layer,
    scale: f64,
    clip: photocraft_geom::Rect,
) -> Result<photocraft_raster::Surface, cpu::ViewError> {
    photocraft_engine::vector_integration::project_view(doc, layer, scale, clip)
}

/// Conservative stack damage from detached source changes. Uses established compositor
/// bounds, including clipped bases and ancestor effect reach; topology changes invalidate all.
pub fn source_change_bounds(layer: &photocraft_doc::Layer, canvas: photocraft_geom::Rect) -> Option<photocraft_geom::Rect> {
    use photocraft_doc::LayerContent;
    let mut bounds = photocraft_compose::change_bounds(layer, canvas)?;
    match &layer.content {
        LayerContent::Shape(shape) if shape.cache.is_none() => {
            if layer.vector.appearance.is_some() || !layer.vector.corners.is_empty() {
                return None;
            }
            // Detached geometry can deliberately have no persisted 1x cache. Its paint
            // still changes the scaled view; compiling bounds never allocates a raster.
            let shape = vector::project_shape(shape, 1.0).ok()?;
            let own = CompiledShape::new(&shape, DEFAULT_TOLERANCE, canvas).bounds().unwrap_or(photocraft_geom::Rect::EMPTY);
            let margin = if effects::has_effects(layer) { effects::margin(layer) } else { 0 };
            bounds = bounds.union(&own.inflate(margin).intersect(&canvas));
        }
        LayerContent::Text(text) if text.cache.is_none() => return None,
        LayerContent::Group(group) if group.artboard.is_none() => {
            for child in group.children.iter().filter(|l| l.visible) {
                bounds = bounds.union(&source_change_bounds(child, canvas)?);
            }
        }
        _ => {}
    }
    Some(bounds)
}

pub fn damage(old: &photocraft_doc::Document, new: &photocraft_doc::Document) -> Option<photocraft_geom::Rect> {
    use photocraft_doc::LayerContent;
    use photocraft_geom::Rect;
    if old.id != new.id
        || old.size != new.size
        || old.mode != new.mode
        || old.depth != new.depth
        || old.resolution_dpi != new.resolution_dpi
        || old.icc_profile != new.icc_profile
        || old.global_light != new.global_light
    {
        return None;
    }
    let a = old.walk();
    let b = new.walk();
    if a.len() != b.len() || a.iter().zip(&b).any(|(a, b)| a.2.id != b.2.id || a.0 != b.0 || a.1 != b.1) {
        return None;
    }
    let reach = |doc: &photocraft_doc::Document| {
        doc.walk().iter().map(|(_, _, l)| if effects::has_effects(l) { effects::margin(l) } else { 0 }).fold(0i32, i32::saturating_add)
    };
    let mut changed = Rect::EMPTY;
    for ((_, _, a), (_, _, b)) in a.iter().zip(&b) {
        let mut aa = (*a).clone();
        let mut bb = (*b).clone();
        if let LayerContent::Group(g) = &mut aa.content {
            g.children.clear();
        }
        if let LayerContent::Group(g) = &mut bb.content {
            g.children.clear();
        }
        if aa == bb && cpu::dependency_key(old, a, 0) == cpu::dependency_key(new, b, 0) {
            continue;
        }
        changed = changed.union(&source_change_bounds(a, old.bounds())?).union(&source_change_bounds(b, new.bounds())?);
        // Changes above a shape may be clipped to that shape; bounds alone can undercount a
        // fill/adjustment with a canvas-wide source, which already returns None above.
    }
    Some(if changed.is_empty() { changed } else { changed.inflate(reach(old).max(reach(new))).intersect(&new.bounds()) })
}

#[cfg(test)]
mod damage_regression {
    #[test]
    fn unchanged_effect_document_has_empty_damage() {
        use photocraft_doc::{ColorMode, Document, Layer, SampleType};
        let mut doc = Document::new("Unchanged", photocraft_geom::Size::new(64, 64), ColorMode::Rgb, SampleType::U8);
        let mut layer = Layer::raster("Effects", doc.pixel_format());
        layer.effects.items.push(photocraft_doc::effects::Effect::default_drop_shadow());
        doc.layers.push(layer);
        assert_eq!(super::damage(&doc, &doc.clone()), Some(photocraft_geom::Rect::EMPTY));
    }
}
