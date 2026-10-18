//! Downsampled proxy documents: every pixel surface scaled down by an integer factor `k`, so a
//! composite of the proxy approximates the document's composite at 1/k resolution for ~1/k² of
//! the work. Used for interactive previews and for thumbnails of large documents (navigator,
//! channel thumbnails, histograms, file previews), where a full-resolution composite of a 200 MP
//! document would cost seconds and gigabytes.

use photocraft_doc::{Document, Layer, LayerContent, Size};
use photocraft_geom::{Affine, Rect};
use photocraft_raster::Surface;

/// Nearest-neighbour downsample of a surface by integer factor `k` (document coordinates / k).
pub fn downsample(s: &Surface, k: u32) -> Surface {
    let mut out = Surface::with_default(s.format(), &s.default_pixel());
    let b = s.content_bounds();
    if b.is_empty() || k <= 1 {
        return if k <= 1 { s.clone() } else { out };
    }
    // Surfaces span at most ±2^30 px; larger factors reduce everything to one pixel anyway.
    let k = k.min(1 << 20) as i32;
    let bpp = s.format().bytes_per_pixel();
    let (x0, x1) = (b.x0.div_euclid(k), b.x1.div_euclid(k) + i32::from(b.x1.rem_euclid(k) != 0));
    let (y0, y1) = (b.y0.div_euclid(k), b.y1.div_euclid(k) + i32::from(b.y1.rem_euclid(k) != 0));
    let w = x1.saturating_sub(x0).max(0) as usize;
    let mut row = vec![0u8; w * bpp];
    for oy in y0..y1 {
        let src = s.to_interleaved(Rect::new(x0.saturating_mul(k), oy.saturating_mul(k), x1.saturating_mul(k), oy.saturating_mul(k).saturating_add(1)));
        for (i, px) in row.chunks_exact_mut(bpp).enumerate() {
            // Upstream 2842973d: saturated edges (a surface at the i32 limits) can make `src`
            // shorter than expected.
            if let Some(from) = (i * k as usize * bpp).checked_add(bpp).and_then(|end| src.get(i * k as usize * bpp..end)) {
                px.copy_from_slice(from);
            }
        }
        out.write_interleaved(Rect::new(x0, oy, x1, oy + 1), &row);
    }
    out
}

fn shrink_layer(l: &mut Layer, k: u32) {
    if let Some(m) = &mut l.mask {
        m.surface = downsample(&m.surface, k);
        // Feather is a document-pixel distance, just like the mask geometry.
        m.feather = if m.feather.is_finite() { m.feather.clamp(0.0, 1000.0) / k as f32 } else { 0.0 };
    }
    if let Some(fc) = &mut l.fill_cache {
        fc.surface = downsample(&fc.surface, k);
    }
    match &mut l.content {
        LayerContent::Raster(s) => *s = downsample(s, k),
        LayerContent::Group(g) => {
            if let Some(ab) = &mut g.artboard {
                let divisor = i64::from(k);
                // Pixel x samples source x*k: both half-open edges round up, including negatives.
                let edge = |v: i32| {
                    let v = i64::from(v);
                    (v.div_euclid(divisor) + i64::from(v.rem_euclid(divisor) != 0)) as i32
                };
                ab.rect = Rect::new(edge(ab.rect.x0), edge(ab.rect.y0), edge(ab.rect.x1), edge(ab.rect.y1));
            }
            for c in &mut g.children {
                shrink_layer(c, k);
            }
        }
        LayerContent::Text(t) => t.cache = t.cache.as_ref().map(|s| downsample(s, k)),
        LayerContent::Shape(sh) => sh.cache = sh.cache.as_ref().map(|s| downsample(s, k)),
        LayerContent::Smart(so) => {
            so.cache = so.cache.as_ref().map(|s| downsample(s, k));
            if let Some(mask) = &mut so.filter_mask {
                mask.surface = downsample(&mask.surface, k);
            }
            // The placement shrinks too: a preview that adds a smart filter re-renders the source
            // through it, which left in full-size coordinates came out `k` times too large.
            let scale = 1.0 / f64::from(k);
            so.transform = Affine::scale(scale).mul(&so.transform);
            // Scaling the document side of the projective map scales its first two rows.
            if let Some(perspective) = &mut so.perspective {
                for value in perspective.iter_mut().take(6) {
                    *value *= scale;
                }
            }
        }
        LayerContent::Symbol(s) => s.cache = s.cache.as_ref().map(|s| downsample(s, k)),
        LayerContent::Adjustment(_) | LayerContent::Fill(_) => {}
    }
}

/// A copy of `doc` scaled down by `k` (same layer ids, so adjustments can be swapped in). The
/// selection scales with it, so a filter previewed on the proxy stays inside the (feathered)
/// selection exactly as the full-size result does.
pub fn proxy_document(doc: &Document, k: u32) -> Document {
    let mut p = doc.clone();
    if k <= 1 {
        return p;
    }
    p.size = Size::new(doc.size.width.div_ceil(k), doc.size.height.div_ceil(k));
    for l in &mut p.layers {
        shrink_layer(l, k);
    }
    // Spot channels are printed under Multichannel composites.
    for c in &mut p.channels {
        c.surface = downsample(&c.surface, k);
    }
    p.selection = p.selection.as_ref().map(|s| downsample(s, k));
    p
}

/// Whether a proxy composite is a faithful reduction of `doc`: layer effects (whose sizes are in
/// document pixels) and vector masks (paths in document coordinates) would not scale with it.
pub fn proxy_faithful(doc: &Document) -> bool {
    doc.walk().iter().all(|(_, _, l)| !crate::effects::has_effects(l) && l.vector_mask.as_ref().is_none_or(|v| !v.enabled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::{Color, ColorMode, PixelFormat, SampleType};

    #[test]
    fn downsample_picks_every_kth_pixel() {
        let mut s = Surface::new(PixelFormat::RGBA8);
        s.fill_rect(Rect::new(0, 0, 8, 8), &[1.0, 0.0, 0.0, 1.0]);
        s.write_pixel(4, 4, &[0.0, 0.0, 1.0, 1.0]);
        let d = downsample(&s, 4);
        assert_eq!(d.pixel(0, 0), vec![1.0, 0.0, 0.0, 1.0]);
        assert_eq!(d.pixel(1, 1), vec![0.0, 0.0, 1.0, 1.0]);
        assert_eq!(d.pixel(2, 2)[3], 0.0);
    }

    #[test]
    fn downsample_handles_negative_coordinates() {
        let mut s = Surface::new(PixelFormat::RGBA8);
        s.fill_rect(Rect::new(-8, -8, 0, 0), &[0.0, 1.0, 0.0, 1.0]);
        let d = downsample(&s, 4);
        assert_eq!(d.pixel(-1, -1), vec![0.0, 1.0, 0.0, 1.0]);
        assert_eq!(d.pixel(0, 0)[3], 0.0);
    }

    #[test]
    fn proxy_keeps_structure_and_ids() {
        let doc = Document::with_background("d", Size::new(4000, 3000), ColorMode::Rgb, SampleType::U8, Color::WHITE);
        let p = proxy_document(&doc, 4);
        assert_eq!((p.size.width, p.size.height), (1000, 750));
        assert_eq!(p.layers[0].id, doc.layers[0].id);
        assert_eq!(p.layers[0].surface().unwrap().pixel(999, 749), vec![1.0; 4]);
        assert!(proxy_faithful(&doc));
    }

    /// A smart object re-rendered on the proxy (a smart filter preview, such as Curves) goes through
    /// its placement, which must shrink with the canvas, cache and filter mask.
    #[test]
    fn proxy_scales_smart_object_placement() {
        use photocraft_doc::{LayerMask, SmartObject, SmartSource};
        let mut doc = Document::new("smart", Size::new(64, 64), ColorMode::Rgb, SampleType::U8);
        let mut cache = Surface::new(doc.pixel_format());
        cache.fill_rect(Rect::new(16, 16, 32, 48), &[0.0, 0.0, 0.0, 1.0]);
        let source = SmartSource::Embedded { file_name: "s.png".into(), bytes: std::sync::Arc::new(Vec::new()) };
        let mut sm = SmartObject::new(source, Affine::translate(16.0, 16.0).mul(&Affine::scale(2.0)), Some(cache));
        let mut mask = LayerMask::hide_all();
        mask.surface.fill_rect(Rect::new(16, 16, 32, 32), &[1.0]);
        sm.filter_mask = Some(mask);
        sm.perspective = Some([2.0, 0.0, 16.0, 0.0, 2.0, 16.0, 0.0, 0.0, 1.0]);
        doc.layers.push(Layer::new("smart", LayerContent::Smart(sm)));
        let p = proxy_document(&doc, 4);
        let LayerContent::Smart(sm) = &p.layers[0].content else { panic!("a smart object") };
        assert_eq!(sm.transform, Affine { m: [0.5, 0.0, 0.0, 0.5, 4.0, 4.0] });
        assert_eq!(sm.perspective, Some([0.5, 0.0, 4.0, 0.0, 0.5, 4.0, 0.0, 0.0, 1.0]));
        assert_eq!(sm.cache.as_ref().unwrap().content_bounds(), Rect::new(4, 4, 8, 12));
        let mask = &sm.filter_mask.as_ref().unwrap().surface;
        assert_eq!((mask.pixel(4, 4)[0], mask.pixel(8, 8)[0]), (1.0, 0.0));
        // The document keeps its full-size placement.
        let LayerContent::Smart(orig) = &doc.layers[0].content else { panic!("a smart object") };
        assert_eq!(orig.transform.m[4], 16.0);
    }

    #[test]
    fn proxy_scales_the_selection() {
        let mut doc = Document::with_background("d", Size::new(64, 64), ColorMode::Rgb, SampleType::U8, Color::WHITE);
        let mut sel = Surface::new(PixelFormat::GRAY8);
        sel.fill_rect(Rect::new(0, 0, 32, 64), &[1.0]);
        doc.selection = Some(sel);
        let p = proxy_document(&doc, 4);
        let s = p.selection.as_ref().expect("selection kept");
        assert_eq!((s.pixel(7, 15)[0], s.pixel(8, 0)[0]), (1.0, 0.0));
        assert!(proxy_document(&Document::new("n", Size::new(8, 8), ColorMode::Rgb, SampleType::U8), 4).selection.is_none());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod feather_proxy_regression {
    use super::*;
    use photocraft_doc::{Color, ColorMode, LayerMask, SampleType};

    #[test]
    fn proxy_mask_feather_keeps_its_source_pixel_scale() {
        let mut doc = Document::with_background("feather", Size::new(512, 512), ColorMode::Rgb, SampleType::U8, Color::WHITE);
        let mut mask = LayerMask::hide_all();
        mask.surface.fill_rect(Rect::new(0, 0, 256, 512), &[1.0]);
        mask.feather = 16.0;
        doc.layers[0].mask = Some(mask);
        let original = crate::masks::combined_mask(&doc.layers[0], doc.bounds()).unwrap();
        for k in [2, 4, 8] {
            assert!(proxy_faithful(&doc));
            let proxy = proxy_document(&doc, k);
            assert_eq!(proxy.layers[0].mask.as_ref().unwrap().feather, 16.0 / k as f32);
            let reduced = crate::masks::combined_mask(&proxy.layers[0], proxy.bounds()).unwrap();
            for x in [192, 208, 224, 288, 304, 320] {
                let expected = original.sample_channel(x, 256, 0);
                let got = reduced.sample_channel(x / k as i32, 256 / k as i32, 0);
                assert!((got - expected).abs() < 0.04, "k={k} x={x}: {got} versus {expected}");
            }
        }
        assert_eq!(doc.layers[0].mask.as_ref().unwrap().feather, 16.0);
    }
}
