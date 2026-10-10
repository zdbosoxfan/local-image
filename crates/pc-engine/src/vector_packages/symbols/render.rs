//! Bounds-limited instance caches through pc-vector/pc-text and the ordinary compositor.
use super::*;
use photocraft_algo::transform::{Homography, Interp, warp_surface};
use photocraft_doc::{Locks, ShapeLayer};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

const MAX_CACHE_PIXELS: u64 = 16_000_000;
fn budget(rect: Rect) -> Result<()> {
    if u64::from(rect.width()) * u64::from(rect.height()) > MAX_CACHE_PIXELS { Err(bad("symbol cache pixel budget exceeded")) } else { Ok(()) }
}
fn mapped_bounds(rect: Rect, a: Affine) -> Result<Rect> {
    // The existing warp reads its source region before producing the destination.
    budget(rect)?;
    let corners = [(rect.x0, rect.y0), (rect.x1, rect.y0), (rect.x0, rect.y1), (rect.x1, rect.y1)]
        .map(|(x, y)| a.apply(photocraft_geom::Point::new(f64::from(x), f64::from(y))));
    if corners.iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > validation::MAX_COORDINATE || p.y.abs() > validation::MAX_COORDINATE) {
        return Err(bad("expanded symbol coordinates exceed bounds"));
    }
    let min_x = corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let min_y = corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_x = corners.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let max_y = corners.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    let rect = Rect::new(min_x.floor() as i32, min_y.floor() as i32, max_x.ceil() as i32, max_y.ceil() as i32);
    budget(rect)?;
    Ok(rect)
}
fn shape_valid(sh: &ShapeLayer) -> Result<()> {
    for knot in sh.path.subpaths.iter().flat_map(|p| &p.knots) {
        for p in [knot.anchor, knot.in_ctrl, knot.out_ctrl] {
            if !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > validation::MAX_COORDINATE || p.y.abs() > validation::MAX_COORDINATE {
                return Err(bad("symbol path must be finite and bounded"));
            }
        }
    }
    if let Some(st) = &sh.stroke {
        let numbers = [st.width, st.miter_limit, st.dash_offset];
        if numbers.iter().chain(&st.dashes).any(|n| !n.is_finite() || n.abs() > 10_000.) || st.width < 0. {
            return Err(bad("symbol stroke must be finite and bounded"));
        }
    }
    Ok(())
}
fn composite_surface(doc: &Document, layers: Vec<Layer>, bounds: Rect) -> Result<Surface> {
    budget(bounds)?;
    let mut projected = doc.clone();
    projected.layers = layers;
    let fmt = doc.pixel_format();
    let n = fmt.channels();
    let mut cache = Surface::new(fmt);
    photocraft_compose::render_bands(&projected, bounds, 256, |band| -> Result<()> {
        let mut pixels = vec![0.; band.px.len() * n];
        for (p, out) in band.px.iter().zip(pixels.chunks_exact_mut(n)) {
            photocraft_raster::from_rgba_into(&fmt, *p, out);
        }
        cache.write_region(band.rect, &pixels);
        Ok(())
    })?;
    cache.prune();
    Ok(cache)
}
/// Applies the shared data projection exactly once. Package 09 may substitute its scaled
/// geometry/appearance provider for this legacy preparation step, using the same Expansion.
pub fn prepare_expansion(doc: &Document, mut ex: expansion::Expansion) -> Result<Vec<Layer>> {
    validation::validate(&[], &ex.layers).map_err(|e| bad(e.to_string()))?;
    for a in ex.transforms.values() {
        validation::affine(*a).map_err(|e| bad(e.to_string()))?;
    }
    fn prepare(doc: &Document, layer: &mut Layer, transforms: &std::collections::BTreeMap<u64, Affine>) -> Result<()> {
        let a = *transforms.get(&layer.id.0).ok_or_else(|| bad("projection transform missing"))?;
        if layer.vector.appearance.is_some()
            || !layer.vector.corners.is_empty()
            || layer.vector.on_path.is_some()
            || layer.vector.live.is_some()
            || layer.vector.clip.is_some()
        {
            return Err(bad("Rendering this symbol's advanced vector content is not available in this build"));
        }
        if let LayerContent::Group(g) = &mut layer.content {
            for child in &mut g.children {
                prepare(doc, child, transforms)?;
            }
            if let Some(artboard) = &mut g.artboard {
                artboard.rect = mapped_bounds(artboard.rect, a)?;
            }
            // Parent masks/effects are transformed without re-applying a to children.
        } else {
            // Legacy Fill has no affine field. Preserve its complete canvas-relative paint by
            // projecting bare content to pixels before warping; enclosing masks/fx stay outside.
            // The shared data projection still retains editable Fill + affine for file/scaled
            // providers. Detached legacy transformed fills become ordinary raster layers.
            if a != Affine::IDENTITY && matches!(layer.content, LayerContent::Fill(_)) {
                let bare = Layer::new("Fill projection", layer.content.clone());
                let surface = composite_surface(doc, vec![bare], doc.bounds())?;
                layer.content = LayerContent::Raster(surface);
                layer.fill_cache = None;
            }
            if let LayerContent::Shape(sh) = &layer.content {
                shape_valid(sh)?;
            }
            if a != Affine::IDENTITY {
                layer.vector.live_shape = None;
                if let Some(surface) = layer.surface() {
                    mapped_bounds(surface.content_bounds(), a)?;
                }
                let locks = layer.locks;
                layer.locks = Locks::default();
                let [a0, b, c, d, e, f] = a.m;
                let h = Homography([a0, c, e, b, d, f, 0., 0., 1.]);
                // Keep linked masks out of transform_layer; all masks below are projected once.
                let mask = layer.mask.take();
                let vector_mask = layer.vector_mask.take();
                crate::transform_cmds::transform_layer(None, Locks::default(), layer, &h, Some(a), Interp::Bicubic)?;
                layer.mask = mask;
                layer.vector_mask = vector_mask;
                layer.locks = locks;
            }
            match &mut layer.content {
                LayerContent::Shape(sh) => {
                    shape_valid(sh)?;
                    let compiled = photocraft_vector::CompiledShape::new(sh, photocraft_vector::DEFAULT_TOLERANCE, doc.bounds());
                    let bounds = compiled.bounds().unwrap_or(Rect::EMPTY).intersect(&doc.bounds());
                    budget(bounds)?;
                    sh.cache = Some(compiled.render(doc.pixel_format(), bounds));
                }
                LayerContent::Text(t) => {
                    let mut eng = photocraft_text::shared().lock().unwrap_or_else(|e| e.into_inner());
                    let layout = eng.layout(t, doc.resolution_dpi);
                    let warp = photocraft_text::render::layout_warp(&layout, t.warp.as_ref());
                    let bounds = photocraft_text::render::ink_rect_warped(&layout, &t.transform, warp.as_ref());
                    mapped_bounds(bounds, Affine::IDENTITY)?;
                    t.cache = Some(photocraft_text::render::rasterize_warped(&layout, &t.transform, doc.pixel_format(), t.antialias, warp.as_ref()).surface);
                    t.psd_raw = Some(std::sync::Arc::new(photocraft_text::psd::build_tysh(t, doc.resolution_dpi, layout.bounds())));
                }
                LayerContent::Smart(_) => {
                    crate::smart_cmds::refresh_layer(doc, layer)?;
                }
                _ => {}
            }
        }
        if a != Affine::IDENTITY {
            let [a0, b, c, d, e, f] = a.m;
            let h = Homography([a0, c, e, b, d, f, 0., 0., 1.]);
            if let Some(m) = &mut layer.mask {
                mapped_bounds(m.surface.content_bounds(), a)?;
                m.surface = crate::transform_cmds::warp_gray(&m.surface, &h, Interp::Bicubic);
            }
            if let Some(vm) = &mut layer.vector_mask {
                vm.path = vm.path.transform(&a);
            }
            if let Some(clip) = &mut layer.vector.clip {
                *clip = clip.transform(&a);
            }
            if let Some((x, y)) = layer.effects.reference {
                let p = a.apply(photocraft_geom::Point::new(x, y));
                layer.effects.reference = Some((p.x, p.y));
            }
            if let Some(fc) = &mut layer.fill_cache {
                mapped_bounds(fc.surface.content_bounds(), a)?;
                fc.surface = warp_surface(&fc.surface, fc.surface.content_bounds(), &h, Interp::Bicubic);
            }
        }
        Ok(())
    }
    for l in &mut ex.layers {
        prepare(doc, l, &ex.transforms)?;
    }
    Ok(ex.layers)
}
/// Cache contains master art only. The enclosing instance layer's masks/effects/blend are
/// applied once by pc-compose/pc-gpu. Hidden instances are refreshed too for later visibility.
pub fn render_instance(doc: &Document, instance: &SymbolInstance) -> Result<Surface> {
    let temporary = Layer::new("Symbol projection", LayerContent::Symbol(instance.clone()));
    let ex = expansion::expand(doc, std::slice::from_ref(&temporary)).map_err(|e| bad(e.to_string()))?;
    let mut projected = doc.clone();
    projected.layers = prepare_expansion(doc, ex)?;
    let bounds = projected
        .layers
        .iter()
        .map(|l| photocraft_compose::composite_bounds(l, doc.bounds()).unwrap_or(doc.bounds()))
        .fold(Rect::EMPTY, |a, b| a.union(&b))
        .intersect(&doc.bounds());
    composite_surface(doc, projected.layers, bounds)
}

/// Legacy Free Transform's detached cached preview. Bounds/affine validation runs before
/// mutation; a later semantic refresh can replace these pixels without losing the fallback.
pub fn transform_cached_instance(instance: &mut SymbolInstance, a: Affine) -> Result<()> {
    validation::affine(a).map_err(|e| bad(e.to_string()))?;
    let transform = a.mul(&instance.transform);
    validation::affine(transform).map_err(|e| bad(e.to_string()))?;
    let cache = if let Some(cache) = &instance.cache {
        mapped_bounds(cache.content_bounds(), a)?;
        let [a0, b, c, d, e, f] = a.m;
        let h = Homography([a0, c, e, b, d, f, 0., 0., 1.]);
        Some(warp_surface(cache, cache.content_bounds(), &h, Interp::Bicubic))
    } else {
        None
    };
    instance.transform = transform;
    instance.cache = cache;
    Ok(())
}
/// A release/canvas operation refreshes only symbol dependents in its target subtree.
pub fn refresh_layer_symbols(doc: &Document, layer: &mut Layer) -> Result<()> {
    match &mut layer.content {
        LayerContent::Symbol(s) => s.cache = Some(render_instance(doc, s)?),
        LayerContent::Group(g) => {
            for c in &mut g.children {
                refresh_layer_symbols(doc, c)?;
            }
        }
        _ => {}
    }
    Ok(())
}
