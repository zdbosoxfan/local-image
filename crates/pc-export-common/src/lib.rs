//! Format-independent halves of the SVG (pc-svg) and PDF (pc-pdf) writers, which adapt
//! VectorCraft `crates/{svg,pdf}/src/export.rs` at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Licensed under MIT OR Apache-2.0; see licenses/vectorcraft-*.
//! Each writer converts [`Error`] into its own error type, which owns the user-facing text.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use photocraft_doc::vector_design::appearance::AppearanceItem;
use photocraft_doc::vector_design::exchange::ExportScope;
use photocraft_doc::{BlendMode, Document, Layer, LayerContent, LayerId, Path, ShapeLayer};
use photocraft_geom::Rect;
use std::collections::BTreeSet;

mod clipping;
pub mod gradient;
pub mod validate;
pub use clipping::{Clipping, clipping_source_eligible};

#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Cancelled,
    Budget(&'static str),
}
pub type Result<T> = std::result::Result<T, Error>;

pub fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
pub fn check_cancel(cancelled: &dyn Fn() -> bool) -> Result<()> {
    if cancelled() { Err(Error::Cancelled) } else { Ok(()) }
}
pub fn check_pixels(rect: Rect, max_pixels: u64) -> Result<()> {
    if rect.is_empty() {
        return Err(invalid("empty export scope"));
    }
    if rect.size().area() > max_pixels {
        return Err(Error::Budget("pixels"));
    }
    Ok(())
}

/// Blend modes both writers express natively (`mix-blend-mode` and `/BM`).
pub fn native_blend(mode: BlendMode) -> bool {
    matches!(
        mode,
        BlendMode::Normal
            | BlendMode::PassThrough
            | BlendMode::Multiply
            | BlendMode::Screen
            | BlendMode::Overlay
            | BlendMode::Darken
            | BlendMode::Lighten
            | BlendMode::ColorDodge
            | BlendMode::ColorBurn
            | BlendMode::HardLight
            | BlendMode::Difference
            | BlendMode::Exclusion
            | BlendMode::Hue
            | BlendMode::Saturation
            | BlendMode::Color
            | BlendMode::Luminosity
    )
}

pub fn partial_backdrop_dependency(doc: &Document, scope: &ExportScope) -> bool {
    let ids = match scope {
        ExportScope::Document => return false,
        ExportScope::Selection(ids) => ids.clone(),
        ExportScope::Artboard(id) => vec![*id],
    };
    fn selected(layer: &Layer, ids: &[LayerId]) -> bool {
        ids.contains(&layer.id) || layer.children().is_some_and(|kids| kids.iter().any(|l| selected(l, ids)))
    }
    fn walk(layers: &[Layer], ids: &[LayerId], inside: bool) -> bool {
        layers.iter().any(|layer| {
            let included = inside || ids.contains(&layer.id);
            matches!(&layer.content,LayerContent::Group(g) if g.isolation==photocraft_doc::GroupIsolation::NonIsolated && (included||selected(layer,ids)))
                || layer.children().is_some_and(|kids| walk(kids, ids, included))
        })
    }
    walk(&doc.layers, &ids, false)
}
/// Validates every requested id before pruning, including hidden objects. Selecting both
/// a group and its descendant includes the group only once, in document paint order.
pub fn scoped_document(doc: &Document, scope: &ExportScope) -> Result<Document> {
    let targets = match scope {
        ExportScope::Document => &[][..],
        ExportScope::Selection(ids) => ids.as_slice(),
        ExportScope::Artboard(id) => std::slice::from_ref(id),
    };
    fn selected_descendant(layer: &Layer, targets: &[LayerId]) -> bool {
        targets.contains(&layer.id) || layer.children().is_some_and(|children| children.iter().any(|l| selected_descendant(l, targets)))
    }
    fn check_clipped(layers: &[Layer], targets: &[LayerId], inherited: bool) -> bool {
        let mut i = 0;
        while i < layers.len() {
            let end = clipping_end(layers, i);
            let selected = inherited || targets.contains(&layers[i].id);
            if !selected && layers[i + 1..end].iter().any(|l| l.visible && selected_descendant(l, targets)) {
                return false;
            }
            for layer in &layers[i..end] {
                if let Some(children) = layer.children()
                    && !check_clipped(children, targets, inherited || targets.contains(&layer.id))
                {
                    return false;
                }
            }
            i = end;
        }
        true
    }
    if !targets.is_empty() && !check_clipped(&doc.layers, targets, false) {
        return Err(invalid("selected clipped artwork requires its clipping base; include the base or export its enclosing group"));
    }

    let mut compatible = doc.clone();
    compatible.normalize_legacy_groups();
    let doc = &compatible;
    let ids = match scope {
        ExportScope::Document => return Ok(doc.clone()),
        ExportScope::Selection(ids) if !ids.is_empty() => ids.clone(),
        ExportScope::Selection(_) => return Err(invalid("selection is empty")),
        ExportScope::Artboard(id) => {
            if doc.layer(*id).and_then(Layer::artboard).is_none() {
                return Err(invalid("artboard is missing"));
            }
            vec![*id]
        }
    };
    let mut selected = BTreeSet::new();
    for id in ids {
        if doc.layer(id).is_none() {
            return Err(invalid(format!("layer {} is missing", id.0)));
        }
        if !selected.insert(id.0) {
            return Err(invalid("duplicate export target"));
        }
    }
    fn prune(layers: &[Layer], selected: &BTreeSet<u64>) -> Vec<Layer> {
        layers
            .iter()
            .filter_map(|layer| {
                if selected.contains(&layer.id.0) {
                    return Some(layer.clone());
                }
                if let LayerContent::Group(group) = &layer.content {
                    let children = prune(&group.children, selected);
                    if !children.is_empty() {
                        let mut layer = layer.clone();
                        if let LayerContent::Group(g) = &mut layer.content {
                            g.children = children;
                        }
                        return Some(layer);
                    }
                }
                None
            })
            .collect()
    }
    let mut scene = doc.clone();
    scene.layers = prune(&doc.layers, &selected);
    if scene.walk().iter().any(|(_, _, l)| matches!(&l.content, LayerContent::Group(g) if g.isolation == photocraft_doc::GroupIsolation::NonIsolated)) {
        return Err(invalid("nonisolated groups require Document export to retain their actual backdrop"));
    }
    Ok(scene)
}

pub fn painted_shapes(l: &Layer, shape: &ShapeLayer) -> Vec<ShapeLayer> {
    if let Some(a) = &l.vector.appearance {
        a.items
            .iter()
            .filter_map(|item| {
                let mut s = ShapeLayer { path: shape.path.clone(), ..Default::default() };
                match item {
                    AppearanceItem::Fill(f) if f.visible => s.fill = Some(f.paint.clone()),
                    AppearanceItem::Stroke(st) if st.visible => s.stroke = Some(st.base.clone()),
                    _ => return None,
                }
                Some(s)
            })
            .collect()
    } else {
        vec![shape.clone()]
    }
}

// Sibling clipping is source-atop, not an independent clip-path on each child.
pub fn clipping_end(layers: &[Layer], start: usize) -> usize {
    let mut end = start + 1;
    while !layers[start].clipped && end < layers.len() && layers[end].clipped {
        end += 1;
    }
    end
}
pub fn clipping_base_independent(layer: &Layer) -> bool {
    matches!(layer.content, LayerContent::Shape(_) | LayerContent::Raster(_) | LayerContent::Fill(_) | LayerContent::Text(_))
        && native_blend(layer.blend)
        && photocraft_compose::text_gamma(layer) == 1.0
        && !photocraft_compose::effects::has_effects(layer)
        && layer.excluded_channels == 0
        && layer.blend_if.is_default()
}

// A local raster fallback may absorb descendant dependencies only when this
// group's content starts from its own transparent backdrop. Its outer operator
// must remain representable; effects/Blend If/channel restrictions can depend
// on the enclosing backdrop and are deliberately not absorbed here.
pub fn closed_isolated_group(layer: &Layer) -> bool {
    let LayerContent::Group(group) = &layer.content else {
        return false;
    };
    let isolated = group.isolation == photocraft_doc::GroupIsolation::Isolated
        || group.isolation == photocraft_doc::GroupIsolation::Auto && (layer.blend != BlendMode::PassThrough || layer.fill_opacity < 1.0);
    isolated
        && group.artboard.is_none()
        && !layer.clipped
        && native_blend(layer.blend)
        && layer.excluded_channels == 0
        && layer.blend_if.is_default()
        && !photocraft_compose::effects::has_effects(layer)
}

pub fn clipping_child_native(layer: &Layer) -> bool {
    if !layer.visible {
        return true;
    }
    if !native_blend(layer.blend)
        || photocraft_compose::effects::has_effects(layer)
        || layer.excluded_channels != 0
        || !layer.blend_if.is_default()
        || photocraft_compose::text_gamma(layer) != 1.0
    {
        return false;
    }
    match &layer.content {
        LayerContent::Adjustment(_) => false,
        LayerContent::Group(group) => {
            group.artboard.is_none() && group.isolation != photocraft_doc::GroupIsolation::NonIsolated && group.children.iter().all(clipping_child_native)
        }
        _ => true,
    }
}
pub fn isolated_group(name: &str, children: Vec<Layer>) -> Layer {
    let mut layer = Layer::group(name, children);
    layer.blend = BlendMode::Normal;
    if let LayerContent::Group(group) = &mut layer.content {
        group.isolation = photocraft_doc::GroupIsolation::Isolated;
    }
    layer
}

// combined_mask's Gaussian uses three box passes per axis. Sampling a cropped
// output directly would clamp intermediate blur values at the crop edge. Keep
// exactly the full support of all passes, without allocating the full canvas.
pub fn soft_mask_sampling_canvas(layer: &Layer, output: Rect, canvas: Rect) -> Result<Rect> {
    if !photocraft_compose::masks::has_feather(layer) {
        return Ok(output);
    }
    let support = |feather: f32| -> Result<i32> {
        let sigma = photocraft_compose::masks::feather_sigma(feather);
        if sigma <= 0.0 {
            return Ok(0);
        }
        // Keep the same floating-point expression and odd-width rounding as
        // masks::gaussian; pixel and vector masks blur separately then multiply.
        let ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
        let radius = (((ideal.floor() as usize) | 1).max(1) - 1) / 2;
        radius.checked_mul(3).and_then(|v| i32::try_from(v).ok()).ok_or(Error::Budget("mask halo"))
    };
    let pixel = layer.mask.as_ref().filter(|m| m.enabled).map_or(Ok(0), |m| support(m.feather))?;
    let vector = layer.vector_mask.as_ref().filter(|m| m.enabled).map_or(Ok(0), |m| support(m.feather))?;
    let halo = pixel.max(vector);
    let grow = |value: i32, delta: i32| value.checked_add(delta).ok_or(Error::Budget("mask halo coordinates"));
    Ok(Rect::new(grow(output.x0, -halo)?, grow(output.y0, -halo)?, grow(output.x1, halo)?, grow(output.y1, halo)?).intersect(&canvas))
}
pub fn mask_allocation_bytes(pixels: u64, bytes_per_pixel: u64) -> Result<usize> {
    pixels
        .checked_mul(bytes_per_pixel)
        .and_then(|bytes| usize::try_from(bytes).ok())
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or(Error::Budget("mask working bytes"))
}

pub fn effect_margin(layer: &Layer) -> i32 {
    if photocraft_compose::effects::has_effects(layer) { photocraft_compose::effects::margin(layer) } else { 0 }
}
pub fn normalized_fill(path: &Path, rect: Rect) -> Result<Path> {
    if !path.inverted && path.components().len() <= 1 {
        return Ok(path.clone());
    }
    photocraft_pathops::finish_compound(path, Some(kurbo::Rect::new(rect.x0 as f64, rect.y0 as f64, rect.x1 as f64, rect.y1 as f64)))
        .map_err(|e| invalid(e.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod mask_halo_tests {
    use super::*;
    use photocraft_doc::Fill;
    #[test]
    fn three_pass_support_uses_max_enabled_radius_and_canvas_boundary() {
        let mut layer = Layer::new("mask", LayerContent::Fill(Fill::Solid(photocraft_doc::Color::WHITE)));
        let mut pixel = photocraft_doc::LayerMask::reveal_all();
        pixel.feather = 2.;
        layer.mask = Some(pixel);
        let mut vector = photocraft_doc::VectorMask::new(rectangle());
        vector.feather = 1.;
        layer.vector_mask = Some(vector);
        let output = Rect::new(19, 39, 90, 82);
        let canvas = Rect::new(0, 0, 128, 128);
        assert_eq!(soft_mask_sampling_canvas(&layer, output, canvas).unwrap(), Rect::new(13, 33, 96, 88));
        layer.vector_mask.as_mut().unwrap().feather = 3.;
        assert_eq!(soft_mask_sampling_canvas(&layer, output, canvas).unwrap(), Rect::new(10, 30, 99, 91));
        layer.vector_mask.as_mut().unwrap().enabled = false;
        layer.mask.as_mut().unwrap().feather = 1.;
        assert_eq!(soft_mask_sampling_canvas(&layer, Rect::new(-2, -1, 4, 3), Rect::new(0, 0, 5, 5)).unwrap(), Rect::new(0, 0, 5, 5));
        layer.mask.as_mut().unwrap().enabled = false;
        assert_eq!(soft_mask_sampling_canvas(&layer, output, canvas).unwrap(), output);
        layer.mask.as_mut().unwrap().enabled = true;
        layer.mask.as_mut().unwrap().feather = f32::NAN;
        assert_eq!(soft_mask_sampling_canvas(&layer, output, canvas).unwrap(), output);
    }
    fn rectangle() -> Path {
        Path::new(vec![photocraft_doc::Subpath::polygon(&[(0., 0.), (10., 0.), (10., 10.), (0., 10.)])])
    }
    #[test]
    fn mask_byte_and_coordinate_overflow_refuse_before_sampling() {
        assert!(mask_allocation_bytes(u64::MAX, 8).is_err());
        assert!(mask_allocation_bytes(isize::MAX as u64 + 1, 1).is_err());
        let mut layer = Layer::new("mask", LayerContent::Fill(Fill::Solid(photocraft_doc::Color::WHITE)));
        let mut mask = photocraft_doc::LayerMask::reveal_all();
        mask.feather = 1.;
        layer.mask = Some(mask);
        assert!(soft_mask_sampling_canvas(&layer, Rect::new(i32::MIN, 0, 0, 10), Rect::new(0, 0, 20, 20)).is_err());
    }
}
