//! Package 09 display integration. These source-path bridges compile the owned modules
//! without competing edits to package 00's crate registries. 00 may replace bridges with
//! public crate modules; the app already exercises the same source files here.
use eframe::wgpu;
pub use photocraft_compose::{Buffer, cmyk_space, effects, fill_frame, gradient_fill, masks, pattern, render_tiled};
pub use photocraft_gpu::{ChunkOut, Compositor, Stats, Unsupported};
pub use photocraft_vector::{CompiledShape, DEFAULT_TOLERANCE};
#[path = "../../../pc-compose/src/scaled_view.rs"]
pub mod cpu;
#[path = "../../../pc-gpu/src/scaled_view.rs"]
pub mod gpu;
#[path = "../../../pc-vector/src/view.rs"]
pub mod vector;

pub fn render(doc: &photocraft_doc::Document, rect: photocraft_geom::Rect) -> Buffer {
    photocraft_compose::render(doc, rect)
}

/// Same legacy providers in CPU fallback and GPU projection. Enhanced geometry/appearance
/// providers install at this boundary after 08/12/15/18–20; absence is an error.
pub fn project(
    doc: &photocraft_doc::Document,
    layer: &photocraft_doc::Layer,
    scale: f64,
    clip: photocraft_geom::Rect,
) -> Result<photocraft_raster::Surface, cpu::ViewError> {
    if let Some(provider) = PROJECTOR.get() {
        return provider(doc, layer, scale, clip);
    }
    if layer.vector.appearance.is_some() || !layer.vector.corners.is_empty() {
        return Err(cpu::ViewError("enhanced display projector required".into()));
    }
    match &layer.content {
        photocraft_doc::LayerContent::Shape(s) => {
            vector::render_shape_view(s, doc.pixel_format(), cpu::scaled_rect(doc.bounds(), scale)?, clip, scale).map_err(|e| cpu::ViewError(e.into()))
        }
        photocraft_doc::LayerContent::Text(t) => {
            if !doc.resolution_dpi.is_finite() || doc.resolution_dpi <= 0.0 || t.transform.m.iter().any(|v| !v.is_finite()) {
                return Err(cpu::ViewError("invalid type view transform or DPI".into()));
            }
            let mut engine = photocraft_text::shared().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            // Native pc-text::view has the visible-band implementation. Until 00 registers
            // it, use the existing public renderer only when its entire ink buffer is bounded.
            // The UI cannot compile the glyph module directly: skrifa belongs to pc-text.
            let transform = photocraft_geom::Affine::scale(scale).mul(&t.transform);
            let limit = f64::from(i32::MAX / 4);
            if transform.m.iter().any(|v| !v.is_finite() || v.abs() > limit) {
                return Err(cpu::ViewError("type view transform exceeds coordinate budget".into()));
            }
            let layout = engine.layout(t, doc.resolution_dpi);
            let warp = photocraft_text::render::layout_warp(&layout, t.warp.as_ref());
            let ink = photocraft_text::render::ink_rect_warped(&layout, &transform, warp.as_ref());
            if [ink.x0, ink.y0, ink.x1, ink.y1].iter().any(|v| f64::from(*v).abs() > limit) {
                return Err(cpu::ViewError("type view ink exceeds coordinate budget".into()));
            }
            if ink.intersect(&clip).is_empty() {
                return Ok(photocraft_raster::Surface::new(doc.pixel_format()));
            }
            if u64::from(ink.width()) * u64::from(ink.height()) > cpu::MAX_VIEW_PIXELS {
                return Err(cpu::ViewError("native visible-band type module registration required".into()));
            }
            let rendered = photocraft_text::render::rasterize_warped(&layout, &transform, doc.pixel_format(), t.antialias, warp.as_ref());
            let mut surface = photocraft_raster::Surface::new(rendered.surface.format());
            let rect = ink.intersect(&clip);
            surface.write_region(rect, &rendered.surface.read_region(rect));
            surface.prune();
            Ok(surface)
        }
        _ => Err(cpu::ViewError("expected display geometry".into())),
    }
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
    Some(changed.inflate(reach(old).max(reach(new))).intersect(&new.bounds()))
}

static PROJECTOR: std::sync::OnceLock<cpu::Projector> = std::sync::OnceLock::new();
/// Install 08's display paint provider once during app startup. Same boundary for fallback.
pub fn install_projector(provider: cpu::Projector) -> Result<(), cpu::Projector> {
    PROJECTOR.set(provider)
}
