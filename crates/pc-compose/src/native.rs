//! Mode-native flattening of plain CMYK layer stacks.
//!
//! The compositor works in display RGB, so flattening a CMYK document through it reads every ink
//! value through the profile and separates it again: even a lone Background comes back with
//! different ink. When every visible layer is a plain raster layer in Normal mode (no masks,
//! effects or clipping), Normal mixes ink values linearly, as Photoshop does in CMYK, so the
//! composite is worked out on the stored values and an opaque layer keeps them exactly.

use photocraft_color::{ColorMode, PixelFormat, blend::BlendMode};
use photocraft_doc::{Document, LayerContent};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

/// The composite of `doc` over `rect` in `fmt`, worked out in ink values, or `None` when the
/// document isn't CMYK or a visible layer needs the full compositor. `paper` flattens onto white
/// paper (no ink) and makes the result opaque.
pub(crate) fn flatten_cmyk(doc: &Document, rect: Rect, fmt: PixelFormat, paper: bool) -> Option<Surface> {
    if doc.mode != ColorMode::Cmyk || fmt.mode != ColorMode::Cmyk || doc.layers.iter().any(|l| l.clipped) {
        return None;
    }
    let mut layers = Vec::new();
    for layer in doc.layers.iter().filter(|l| l.visible) {
        let LayerContent::Raster(surface) = &layer.content else { return None };
        if layer.blend != BlendMode::Normal
            || layer.mask.as_ref().is_some_and(|m| m.enabled)
            || layer.vector_mask.as_ref().is_some_and(|m| m.enabled)
            || crate::effects::has_effects(layer)
            || layer.excluded_channels != 0
            || !layer.blend_if.is_default()
            || layer.video.is_some()
            || surface.format().mode != ColorMode::Cmyk
        {
            return None;
        }
        layers.push((surface, (layer.opacity * layer.fill_opacity).clamp(0., 1.)));
    }
    let mut out = Surface::new(fmt);
    let mut y = rect.y0;
    let rows = crate::band_rows_for(rect.width(), 0);
    while y < rect.y1 && !rect.is_empty() {
        let band = Rect::new(rect.x0, y, rect.x1, y.saturating_add(rows).min(rect.y1));
        let count = (band.width() as usize).checked_mul(band.height() as usize)?;
        let mut accum = vec![[0.; 5]; count];
        for &(surface, opacity) in &layers {
            if opacity <= 0. || (surface.content_bounds().intersect(&band).is_empty() && surface.default_pixel().iter().all(|v| *v == 0.)) {
                continue;
            }
            let sf = surface.format();
            let pixels = surface.read_region(band);
            for (d, s) in accum.iter_mut().zip(pixels.chunks_exact(sf.channels())) {
                let alpha = if sf.alpha { s[4] } else { 1. } * opacity;
                if alpha <= 0. {
                    continue;
                }
                let keep = d[4] * (1. - alpha);
                let result_alpha = alpha + keep;
                for c in 0..4 {
                    d[c] = if alpha >= 1. { s[c] } else { (s[c] * alpha + d[c] * keep) / result_alpha };
                }
                d[4] = result_alpha;
            }
        }
        let mut pixels = Vec::with_capacity(count.checked_mul(fmt.channels())?);
        for p in accum {
            let factor = if paper { p[4] } else { 1. };
            pixels.extend(p[..4].iter().map(|v| v * factor));
            if fmt.alpha {
                pixels.push(if paper { 1. } else { p[4] });
            }
        }
        out.write_region(band, &pixels);
        y = band.y1;
    }
    out.prune();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::SampleType;
    use photocraft_doc::Layer;
    use photocraft_geom::Size;
    #[test]
    fn normal_cmyk_layers_preserve_inks_and_mix_coverage_without_rgb() {
        for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let mut doc = Document::new("ink", Size::new(2, 1), ColorMode::Cmyk, depth);
            let fmt = doc.pixel_format();
            let mut lower = Layer::raster("lower", fmt);
            lower.surface_mut().unwrap().fill_rect(doc.bounds(), &[0.1, 0.2, 0.3, 0.8, 1.]);
            let original = lower.surface().unwrap().pixel(0, 0);
            doc.layers.push(lower);
            assert_eq!(crate::flatten_to_surface(&doc, fmt, None).pixel(0, 0), original);
            let mut upper = Layer::raster("upper", fmt);
            upper.surface_mut().unwrap().fill_rect(doc.bounds(), &[0.8, 0.4, 0.1, 0.2, 1.]);
            upper.opacity = 0.5;
            let top = upper.surface().unwrap().pixel(0, 0);
            doc.layers.push(upper);
            let actual = crate::flatten_to_surface(&doc, fmt, Some([1.; 3])).pixel(0, 0);
            for c in 0..4 {
                assert!((actual[c] - (original[c] + top[c]) * 0.5).abs() <= 1. / 255.);
            }
            doc.layers[1].blend = BlendMode::Multiply;
            assert!(flatten_cmyk(&doc, doc.bounds(), fmt, false).is_none());
        }
    }
}
