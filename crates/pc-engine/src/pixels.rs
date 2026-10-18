//! Destructive pixel operations on layers (Image → Adjustments, flips, fills).

use rayon::prelude::*;

use photocraft_compose::{Buffer, adjust};
use photocraft_doc::{Adjustment, Document, Layer, LayerContent};
use photocraft_geom::Rect;
use photocraft_raster::{Surface, to_rgba};

/// Apply an adjustment destructively to a surface, weighted by an optional selection.
/// Applies `adj` to a surface (any colour model and depth, via straight RGBA) through the
/// selection; `mode` is the document's, for the tone transfer (e.g. Exposure in Grayscale).
pub fn adjust_surface(s: &mut Surface, adj: &Adjustment, selection: Option<&Surface>, mode: photocraft_color::ColorMode) {
    let r = s.content_bounds();
    if r.is_empty() {
        return;
    }
    let out = adjusted(s, r, adj, selection, mode);
    s.write_region(r, &out);
}

/// Applies `adj` to a layer mask through the selection. Untouched mask pixels count (they read as
/// the mask's default value), and without a selection the default changes too, since a mask
/// reaches past the canvas: Invert turns a reveal-all mask into a hide-all one (#780).
pub fn adjust_mask(s: &mut Surface, adj: &Adjustment, selection: Option<&Surface>) {
    let mode = photocraft_color::ColorMode::Grayscale;
    if let Some(sel) = selection {
        let r = sel.content_bounds();
        if !r.is_empty() {
            let out = adjusted(s, r, adj, Some(sel), mode);
            s.write_region(r, &out);
            s.prune();
        }
        return;
    }
    let fmt = s.format();
    let unit = Rect::from_xywh(0, 0, 1, 1);
    let mut out = Surface::with_default(fmt, &adjusted(&Surface::with_default(fmt, &s.default_pixel()), unit, adj, None, mode));
    let r = s.tile_bounds();
    if !r.is_empty() {
        out.write_region(r, &adjusted(s, r, adj, None, mode));
    }
    out.prune();
    *s = out;
}

/// `s`'s pixels over `r` (untouched ones included) with `adj` applied through the selection, encoded.
fn adjusted(s: &Surface, r: Rect, adj: &Adjustment, selection: Option<&Surface>, mode: photocraft_color::ColorMode) -> Vec<f32> {
    lightcraft_sysmem::power::compute(|| {
        let fmt = s.format();
        let n = fmt.channels();
        let raw = s.read_region(r);
        let mut buf = Buffer { rect: r, px: raw.par_chunks_exact(n).map(|p| to_rgba(&fmt, p)).collect() };
        let orig = buf.clone();
        // 32-bit documents get the float behaviour of an adjustment layer there (Levels doesn't clip);
        // integer depths keep the unrounded, clipped curves. Threshold and Posterize split on the
        // surface's whole samples, so they always get its depth, as an adjustment layer does.
        let samples_split = matches!(adj, Adjustment::Threshold { .. } | Adjustment::Posterize { .. });
        let depth = (fmt.sample == photocraft_color::SampleType::F32 || samples_split).then_some(fmt.sample);
        adjust::apply_depth(adj, &mut buf, adjust::Transfer::for_document(mode, fmt.sample), depth);
        let w = r.width() as usize;
        let mut out = vec![0.0; raw.len()];
        out.par_chunks_exact_mut(n).enumerate().for_each(|(i, dst)| {
            let (a, o) = (&buf.px[i], &orig.px[i]);
            let k = selection.map_or(1.0, |sel| sel.sample_channel(r.x0 + (i % w) as i32, r.y0 + (i / w) as i32, 0));
            let mixed: [f32; 4] = std::array::from_fn(|c| o[c] + (a[c] - o[c]) * k);
            let mut enc = [0.0f32; 8];
            photocraft_raster::from_rgba_into(&fmt, mixed, &mut enc);
            dst.copy_from_slice(&enc[..n]);
        });
        out
    })
}
/// Fill (respecting selection coverage) with a straight RGBA colour.
pub fn fill_surface(s: &mut Surface, area: Rect, color: [f32; 4], selection: Option<&Surface>, lock_transparency: bool) -> crate::Result<()> {
    let fmt = s.format();
    let n = fmt.channels();
    let mut region = crate::allocation::read_region(s, area, "editing pixels")?;
    let w = area.width() as usize;
    for (i, px) in region.chunks_exact_mut(n).enumerate() {
        let x = area.x0 + (i % w) as i32;
        let y = area.y0 + (i / w) as i32;
        let k = selection.map_or(1.0, |sel| sel.sample_channel(x, y, 0));
        if k <= 0.0 {
            continue;
        }
        let d = to_rgba(&fmt, px);
        let sa = color[3] * k;
        let oa = sa + d[3] * (1.0 - sa);
        let mut o = [0.0f32; 4];
        if oa > 0.0 {
            for c in 0..3 {
                o[c] = (color[c] * sa + d[c] * d[3] * (1.0 - sa)) / oa;
            }
        }
        o[3] = if lock_transparency { d[3] } else { oa };
        let mut enc = [0.0f32; 8];
        photocraft_raster::from_rgba_into(&fmt, o, &mut enc);
        px.copy_from_slice(&enc[..n]);
    }
    s.try_write_region(area, &region).map_err(|e| crate::allocation::error("editing pixels", &e.to_string()))?;
    Ok(())
}

/// Clear pixels (make transparent) within the selection.
pub fn clear_surface(s: &mut Surface, area: Rect, selection: Option<&Surface>) -> crate::Result<()> {
    let fmt = s.format();
    let n = fmt.channels();
    if !fmt.alpha {
        return Ok(());
    }
    let mut region = crate::allocation::read_region(s, area, "editing pixels")?;
    let w = area.width() as usize;
    for (i, px) in region.chunks_exact_mut(n).enumerate() {
        let k = selection.map_or(1.0, |sel| sel.sample_channel(area.x0 + (i % w) as i32, area.y0 + (i / w) as i32, 0));
        px[n - 1] *= 1.0 - k;
        // Fully cleared pixels become the empty pixel, so bounds and tile pruning see them as gone.
        if px[n - 1] <= 0.0 {
            px.fill(0.0);
        }
    }
    s.try_write_region(area, &region).map_err(|e| crate::allocation::error("editing pixels", &e.to_string()))?;
    s.prune();
    Ok(())
}

/// Remap a surface through a pixel-coordinate mapping (used for flips/rotations).
pub fn remap_surface(s: &Surface, map: impl Fn(i32, i32) -> (i32, i32)) -> Surface {
    let r = s.content_bounds();
    let mut out = Surface::with_default(s.format(), &s.default_pixel());
    let n = s.channels();
    let raw = s.read_region(r);
    let w = r.width() as usize;
    for (i, px) in raw.chunks_exact(n).enumerate() {
        let (x, y) = (r.x0 + (i % w) as i32, r.y0 + (i / w) as i32);
        let (nx, ny) = map(x, y);
        out.write_pixel(nx, ny, px);
    }
    out
}

/// Merge `upper` onto `lower` producing a raster layer (Layer → Merge Down).
pub fn merge_down(doc: &Document, lower: &Layer, upper: &Layer) -> Layer {
    let doc_bounds = doc.bounds();
    let format = doc.pixel_format();
    let area = [lower, upper].iter().map(|l| l.surface().map(|s| s.content_bounds()).unwrap_or(doc_bounds)).fold(Rect::EMPTY, |a, b| a.union(&b));
    let stack = vec![lower.clone(), upper.clone()];
    let mut tmp = Document::new("merge", doc_bounds.size(), format.mode, format.sample);
    tmp.layers = stack;
    tmp.icc_profile = doc.icc_profile.clone();
    let s = photocraft_compose::composite_to_surface(&tmp, area, format, None);
    let mut merged = Layer::new(lower.name.clone(), LayerContent::Raster(s));
    merged.id = lower.id;
    merged.blend = lower.blend;
    merged.opacity = 1.0;
    merged
}

#[cfg(test)]
mod parallel_tests {
    use super::*;
    #[test]
    fn destructive_adjustment_byte_identical_across_threads_and_serial_reference() {
        use photocraft_color::{ColorMode, PixelFormat, SampleType};
        for sample in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let fmt = PixelFormat::new(ColorMode::Rgb, sample, true);
            let r = Rect::from_xywh(-7, -3, 73, 59);
            let raw: Vec<f32> = (0..73 * 59).flat_map(|i| [((i * 19) % 256) as f32 / 255.0, 0.3, 0.7, (i % 13) as f32 / 12.0]).collect();
            let mut surface = Surface::new(fmt);
            surface.write_region(r, &raw);
            let mut selection = Surface::new(PixelFormat::new(ColorMode::Grayscale, SampleType::F32, false));
            selection.write_region(r, &(0..73 * 59).map(|i| (i % 17) as f32 / 16.0).collect::<Vec<_>>());
            let adj = Adjustment::Invert;
            let original: Vec<_> = surface.read_region(r).as_chunks::<4>().0.iter().map(|p| to_rgba(&fmt, p)).collect();
            let mut buf = Buffer { rect: r, px: original.clone() };
            let depth = (fmt.sample == SampleType::F32).then_some(fmt.sample);
            adjust::apply_depth(&adj, &mut buf, adjust::Transfer::for_document(ColorMode::Rgb, sample), depth);
            let mut reference = Vec::new();
            for (i, (a, o)) in buf.px.iter().zip(&original).enumerate() {
                let k = selection.sample_channel(r.x0 + (i % 73) as i32, r.y0 + (i / 73) as i32, 0);
                reference.extend(photocraft_raster::from_rgba(&fmt, std::array::from_fn(|c| o[c] + (a[c] - o[c]) * k)));
            }
            for threads in [1, 4] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
                let out = pool.install(|| adjusted(&surface, r, &adj, Some(&selection), ColorMode::Rgb));
                assert_eq!(out.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), reference.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
            }
        }
    }
}

#[cfg(test)]
#[test]
fn merging_normal_cmyk_layers_keeps_the_original_ink_separations() {
    use photocraft_color::{ColorMode, SampleType};
    let doc = Document::new("inks", photocraft_geom::Size::new(2, 1), ColorMode::Cmyk, SampleType::U16);
    let mut lower = Layer::raster("lower", doc.pixel_format());
    lower.surface_mut().unwrap().fill_rect(doc.bounds(), &[0.2, 0.4, 0.1, 0.8, 1.]);
    let mut upper = Layer::raster("upper", doc.pixel_format());
    upper.surface_mut().unwrap().write_pixel(1, 0, &[0.7, 0.2, 0.3, 0.6, 1.]);
    let merged = merge_down(&doc, &lower, &upper);
    assert_eq!(merged.surface().unwrap().pixel(0, 0), lower.surface().unwrap().pixel(0, 0));
    assert_eq!(merged.surface().unwrap().pixel(1, 0), upper.surface().unwrap().pixel(1, 0));
}
