//! Destructive pixel operations on layers (Image → Adjustments, flips, fills).

use rayon::prelude::*;

use photocraft_compose::{Buffer, adjust};
use photocraft_doc::{Adjustment, Document, Layer, LayerContent};
use photocraft_geom::{Rect, TILE_SIZE, TileCoord};
use photocraft_raster::{Surface, to_rgba};

/// The least [`max_unstored_bytes`] allows. Two distant tiles make a surface's content bounds a
/// grid of millions of tiles that hold nothing; a dense layer stores its whole grid, so the budget
/// never limits one, however large.
const MIN_UNSTORED_BYTES: u64 = 1 << 30;

/// Most bytes of tiles an adjustment may materialize beyond the ones a surface stores: a quarter
/// of the memory budget, so the empty mask of a large panorama can still be adjusted through a
/// selection, and at least [`MIN_UNSTORED_BYTES`].
fn max_unstored_bytes() -> u64 {
    (lightcraft_sysmem::profile().memory_budget() / 4).max(MIN_UNSTORED_BYTES)
}

/// Tiles in the grid that covers `r`.
fn grid_tiles(r: Rect) -> u64 {
    if r.is_empty() {
        return 0;
    }
    let (a, b) = (TileCoord::containing(r.x0, r.y0), TileCoord::containing(r.x1 - 1, r.y1 - 1));
    (u64::from(a.tx.abs_diff(b.tx)) + 1) * (u64::from(a.ty.abs_diff(b.ty)) + 1)
}

/// Whether adjusting every tile of the grid over `r` would materialize more than
/// [`max_unstored_bytes`] beyond the `stored` tiles, in format `fmt`.
fn too_sparse(r: Rect, stored: usize, fmt: &photocraft_color::PixelFormat) -> bool {
    let tile_bytes = (TILE_SIZE as u64).pow(2) * fmt.bytes_per_pixel() as u64;
    grid_tiles(r).saturating_sub(stored as u64).saturating_mul(tile_bytes) > max_unstored_bytes()
}

/// The coordinates of `s`'s stored tiles that overlap `r`.
fn stored_tiles(s: &Surface, r: Rect) -> Vec<TileCoord> {
    s.tiles().map(|(c, _)| *c).filter(|c| !c.rect().intersect(&r).is_empty()).collect()
}

const TOO_SPARSE: &str =
    "the adjustment would have to fill too large an empty area between the pixels this layer holds; delete its far-off pixels and try again";

/// Apply an adjustment destructively to a surface, weighted by an optional selection.
/// Applies `adj` to a surface (any colour model and depth, via straight RGBA) through the
/// selection; `mode` is the document's, for the tone transfer (e.g. Exposure in Grayscale).
/// Works tile by tile in parallel, so a large layer needs one tile's buffers per thread rather
/// than four full-layer copies, and the result is the same as one pass over the whole region.
pub fn adjust_surface(s: &mut Surface, adj: &Adjustment, selection: Option<&Surface>, mode: photocraft_color::ColorMode) -> Result<(), String> {
    let r = s.content_bounds();
    if r.is_empty() {
        return Ok(());
    }
    let fmt = s.format();
    // Past the budget a transparent default needs only the stored tiles adjusted (adjustments
    // leave transparent pixels alone, so the gaps stay as they are); an opaque default would
    // change everywhere in that grid, so the adjustment is refused rather than left half applied.
    let stored = stored_tiles(s, r);
    let transparent = fmt.alpha && s.default_pixel().last().is_some_and(|a| *a <= 0.0);
    let over = too_sparse(r, stored.len(), &fmt);
    if over && !transparent {
        return Err(TOO_SPARSE.into());
    }
    let taken: Vec<_> = if over { stored.into_iter().flat_map(|c| s.take_tiles(c.rect())).collect() } else { s.take_tiles(r) };
    let done: Vec<_> = lightcraft_sysmem::power::compute(|| {
        taken
            .into_par_iter()
            .flat_map_iter(|(tc, tile)| {
                // A one-tile surface, so `adjusted` reads and writes this tile only.
                let mut one = Surface::new(fmt);
                one.put_tiles([(tc, tile)]);
                let tr = tc.rect().intersect(&r);
                let out = adjusted(&one, tr, adj, selection, mode);
                one.write_region(tr, &out);
                one.take_tiles(tc.rect())
            })
            .collect()
    });
    s.put_tiles(done);
    Ok(())
}

/// Applies `adj` to a layer mask through the selection. Untouched mask pixels count (they read as
/// the mask's default value), and without a selection the default changes too, since a mask
/// reaches past the canvas: Invert turns a reveal-all mask into a hide-all one (#780).
pub fn adjust_mask(s: &mut Surface, adj: &Adjustment, selection: Option<&Surface>) -> Result<(), String> {
    let mode = photocraft_color::ColorMode::Grayscale;
    if let Some(sel) = selection {
        let r = sel.content_bounds();
        // Tile by tile, and only where the selection stores something (it is unselected, so
        // unchanged, elsewhere): its content bounds can span a grid of mostly nothing.
        let tiles: Vec<Rect> = if sel.default_pixel().iter().all(|v| *v == 0.0) {
            let tiles = stored_tiles(sel, r);
            let missing = tiles.iter().filter(|c| s.tile(**c).is_none()).count() as u64;
            let tile_bytes = (TILE_SIZE as u64).pow(2) * s.format().bytes_per_pixel() as u64;
            if missing.saturating_mul(tile_bytes) > max_unstored_bytes() {
                return Err(TOO_SPARSE.into());
            }
            tiles.into_iter().map(|c| c.rect().intersect(&r)).collect()
        } else {
            // Unselected pixels count too, so every tile of the span is adjusted, one at a time
            // (not one read of the whole span), and the span is bounded by the tiles that writes
            // beyond the ones the mask stores.
            if too_sparse(r, stored_tiles(s, r).len(), &s.format()) {
                return Err(TOO_SPARSE.into());
            }
            r.tiles().map(|c| c.rect().intersect(&r)).collect()
        };
        for t in &tiles {
            let out = adjusted(s, *t, adj, Some(sel), mode);
            s.write_region(*t, &out);
        }
        if !tiles.is_empty() {
            s.prune();
        }
        return Ok(());
    }
    let fmt = s.format();
    let unit = Rect::from_xywh(0, 0, 1, 1);
    let mut out = Surface::with_default(fmt, &adjusted(&Surface::with_default(fmt, &s.default_pixel()), unit, adj, None, mode));
    // Everything the mask doesn't store reads as its default, which `out` already holds adjusted.
    // So only the stored tiles are adjusted, not the (possibly huge) rectangle around them.
    for r in s.tiles().map(|(c, _)| c.rect()) {
        out.write_region(r, &adjusted(s, r, adj, None, mode));
    }
    out.prune();
    *s = out;
    Ok(())
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
///
/// Refuses, leaving everything unchanged, when the combined bounds exceed the render pixel limit
/// (for example layers moved far apart), since the composite would otherwise come back empty.
pub fn merge_down(doc: &Document, lower: &Layer, upper: &Layer) -> crate::Result<Layer> {
    let doc_bounds = doc.bounds();
    let format = doc.pixel_format();
    let area = [lower, upper].iter().map(|l| l.surface().map(|s| s.content_bounds()).unwrap_or(doc_bounds)).fold(Rect::EMPTY, |a, b| a.union(&b));
    let (w, h) = (u64::from(area.width()), u64::from(area.height()));
    if w.saturating_mul(h) > photocraft_compose::MAX_RENDER_PIXELS {
        let detail = format!("the merged layer would be {w} x {h} px, more than the {} px limit", photocraft_compose::MAX_RENDER_PIXELS);
        return Err(crate::allocation::error("merging down", &detail));
    }
    let stack = vec![lower.clone(), upper.clone()];
    let mut tmp = Document::new("merge", doc_bounds.size(), format.mode, format.sample);
    tmp.layers = stack;
    tmp.icc_profile = doc.icc_profile.clone();
    let s = photocraft_compose::composite_to_surface(&tmp, area, format, None);
    let mut merged = Layer::new(lower.name.clone(), LayerContent::Raster(s));
    merged.id = lower.id;
    merged.blend = lower.blend;
    merged.opacity = 1.0;
    Ok(merged)
}

#[cfg(test)]
mod tests {
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

    use photocraft_color::{ColorMode, PixelFormat, SampleType};
    use serde_json::json;

    /// The single pass over the content bounds that the tiled `adjust_surface` replaced.
    fn one_pass(s: &Surface, adj: &Adjustment, selection: Option<&Surface>, mode: ColorMode) -> Surface {
        let mut s = s.clone();
        let r = s.content_bounds();
        if !r.is_empty() {
            let out = adjusted(&s, r, adj, selection, mode);
            s.write_region(r, &out);
        }
        s
    }

    /// Two patches over a 3×3 tile grid, not tile-aligned, with partial alpha; the bottom-left tiles
    /// inside the content bounds are never allocated.
    fn layer(fmt: PixelFormat) -> Surface {
        let mut s = Surface::new(fmt);
        for r in [Rect::new(200, 240, 300, 300), Rect::new(530, 250, 560, 530)] {
            let mut data = Vec::new();
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    let a = if fmt.alpha { 0.3 + 0.7 * ((x + y) % 7) as f32 / 6.0 } else { 1.0 };
                    data.extend(photocraft_raster::from_rgba(&fmt, [x as f32 / 600.0, y as f32 / 540.0, 0.5 + 0.4 * (x as f32 * 0.05).sin(), a]));
                }
            }
            s.write_region(r, &data);
        }
        assert!(!s.has_tiles_in(Rect::new(200, 512, 256, 530)), "a hole inside the content bounds");
        s
    }

    #[test]
    fn tiled_adjustment_matches_one_pass_over_the_content() {
        let adjustments = [
            ("invert", json!({})),
            ("levels", json!({"inBlack": 20, "gamma": 1.3, "inWhite": 230})),
            ("curves", json!({"points": [[0, 10], [128, 160], [255, 240]]})),
            ("hueSaturation", json!({"hue": 40, "saturation": 20})),
            ("brightnessContrast", json!({"brightness": 30, "contrast": 20})),
            ("exposure", json!({"exposure": 0.8})),
            ("posterize", json!({"levels": 5})),
            ("threshold", json!({"level": 120})),
            ("colorBalance", json!({"midtones": [20, -10, 5]})),
            ("blackWhite", json!({"tint": true})),
            ("photoFilter", json!({"density": 50})),
            ("channelMixer", json!({"red": [80, 30, -10, 5]})),
            ("vibrance", json!({"vibrance": 40})),
            ("selectiveColor", json!({"reds": [10, -20, 30, 5]})),
            // Dithered kinds vary by position, so they show a tile's pixels kept their coordinates.
            ("gradientMap", json!({"stops": [[0, "#102030"], [1, "#f0e0a0"]], "dither": true})),
            ("colorLookup", json!({"lut": "warm", "dither": true})),
        ];
        let mut formats = Vec::new();
        for mode in [ColorMode::Rgb, ColorMode::Cmyk, ColorMode::Lab, ColorMode::Grayscale] {
            for sample in [SampleType::U8, SampleType::U16, SampleType::F32] {
                formats.push(PixelFormat::new(mode, sample, true));
            }
        }
        formats.push(PixelFormat::new(ColorMode::Rgb, SampleType::U8, false));
        formats.push(PixelFormat::new(ColorMode::Grayscale, SampleType::U16, false));
        // A soft selection that ends inside the content.
        let mut soft = Surface::new(PixelFormat::GRAY8);
        let sr = Rect::new(0, 0, 500, 400);
        soft.write_region(sr, &(0..sr.height()).flat_map(|_| (0..sr.width()).map(|x| x as f32 / 500.0)).collect::<Vec<_>>());
        for fmt in formats {
            let s = layer(fmt);
            for (kind, p) in &adjustments {
                let adj = crate::adjust_params::from_params(kind, p, None, fmt.mode).unwrap_or_else(|e| panic!("{kind}: {e}"));
                for sel in [None, Some(&soft)] {
                    let want = one_pass(&s, &adj, sel, fmt.mode);
                    let mut got = s.clone();
                    adjust_surface(&mut got, &adj, sel, fmt.mode).unwrap();
                    assert!(want != s, "{kind} {fmt:?}: the adjustment changes the layer");
                    assert!(got == want, "{kind} {fmt:?} selection {}: tiled result differs", sel.is_some());
                }
            }
        }
    }

    #[test]
    fn an_empty_layer_is_left_alone() {
        let mut s = Surface::new(PixelFormat::RGBA8);
        adjust_surface(&mut s, &Adjustment::Invert, None, ColorMode::Rgb).unwrap();
        assert_eq!(s.tile_count(), 0);
    }

    /// Two small blocks two billion pixels apart: the content bounds are a grid of 6·10¹³ tiles
    /// that holds nothing, which no adjustment may materialize.
    fn distant_blocks(fmt: PixelFormat, default: f32) -> Surface {
        let mut s = Surface::with_default(fmt, &vec![default; fmt.channels()]);
        s.fill_rect(Rect::new(0, 0, 10, 10), &vec![0.25; fmt.channels()]);
        s.fill_rect(Rect::new(2_000_000_000, 2_000_000_000, 2_000_000_010, 2_000_000_010), &vec![0.25; fmt.channels()]);
        s
    }

    #[test]
    fn adjusting_a_layer_of_distant_blocks_visits_only_its_tiles() {
        let mut s = distant_blocks(PixelFormat::RGBA8, 0.0);
        adjust_surface(&mut s, &Adjustment::Invert, None, ColorMode::Rgb).unwrap();
        let p = s.read_region(Rect::new(2_000_000_005, 2_000_000_005, 2_000_000_006, 2_000_000_006));
        // Straight colour inverted, alpha untouched.
        assert!((p[0] - 0.75).abs() < 0.01 && (p[3] - 0.25).abs() < 0.01, "{p:?}");
        assert_eq!(s.tile_count(), 2);
    }

    #[test]
    fn adjusting_a_mask_of_distant_blocks_visits_only_its_tiles() {
        let mut m = distant_blocks(PixelFormat::GRAY8, 1.0);
        adjust_mask(&mut m, &Adjustment::Invert, None).unwrap();
        assert!(m.default_pixel()[0] < 0.01, "the default is inverted: {:?}", m.default_pixel());
        let p = m.read_region(Rect::new(2_000_000_005, 2_000_000_005, 2_000_000_006, 2_000_000_006));
        assert!((p[0] - 0.75).abs() < 0.01, "{p:?}");
        // Through a selection of its own distant tiles, too.
        let mut sel = Surface::new(PixelFormat::GRAY8);
        sel.fill_rect(Rect::new(2_000_000_000, 2_000_000_000, 2_000_000_010, 2_000_000_010), &[1.0]);
        adjust_mask(&mut m, &Adjustment::Invert, Some(&sel)).unwrap();
        let p = m.read_region(Rect::new(2_000_000_005, 2_000_000_005, 2_000_000_006, 2_000_000_006));
        assert!((p[0] - 0.25).abs() < 0.01, "{p:?}");
    }

    #[test]
    fn an_opaque_layer_of_distant_blocks_is_refused_not_half_adjusted() {
        let mut s = distant_blocks(PixelFormat::RGBA8, 1.0);
        let before = s.clone();
        assert!(adjust_surface(&mut s, &Adjustment::Invert, None, ColorMode::Rgb).is_err());
        assert!(s == before, "a refused adjustment leaves the layer as it was");
    }

    #[test]
    fn a_selection_with_a_nonzero_default_over_distant_blocks_is_refused() {
        let mut m = Surface::with_default(PixelFormat::GRAY8, &[0.5]);
        let sel = distant_blocks(PixelFormat::GRAY8, 1.0);
        assert!(adjust_mask(&mut m, &Adjustment::Invert, Some(&sel)).is_err());
    }

    #[test]
    fn a_large_shared_selection_cannot_materialize_an_unstored_mask() {
        let mut mask = Surface::with_default(PixelFormat::GRAY8, &[1.0]);
        let before = mask.clone();
        let mut selection = Surface::new(PixelFormat::GRAY8);
        let columns = 1000;
        let tile_bytes = (TILE_SIZE as u64).pow(2) * mask.format().bytes_per_pixel() as u64;
        let rows = max_unstored_bytes() / (columns * tile_bytes) + 1;
        let height = i32::try_from(rows * TILE_SIZE as u64).unwrap();
        selection.fill_rect(Rect::new(0, 0, columns as i32 * TILE_SIZE, height), &[1.0]);
        assert!(adjust_mask(&mut mask, &Adjustment::Invert, Some(&selection)).is_err());
        assert!(mask == before, "refusal leaves every mask pixel and its default unchanged");
    }

    #[test]
    fn adjusting_a_stored_mask_through_a_selection_keeps_unselected_pixels() {
        let mut mask = Surface::with_default(PixelFormat::GRAY8, &[1.0]);
        mask.fill_rect(Rect::new(0, 0, 512, 256), &[0.25]);
        let mut selection = Surface::new(PixelFormat::GRAY8);
        selection.fill_rect(Rect::new(0, 0, 256, 256), &[1.0]);
        adjust_mask(&mut mask, &Adjustment::Invert, Some(&selection)).unwrap();
        assert!((mask.sample_channel(128, 128, 0) - 0.75).abs() < 0.01);
        assert!((mask.sample_channel(384, 128, 0) - 0.25).abs() < 0.01);
        assert_eq!(mask.sample_channel(768, 128, 0), 1.0);
    }

    /// Only the tiles a surface lacks count against the budget: a dense 16-bit layer of 26
    /// gigapixels (210 GB) is not too sparse, though the same box with nothing stored is. Fully
    /// covered tiles share one, so the layer costs one tile here.
    #[test]
    fn a_large_dense_layer_is_never_too_sparse() {
        let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U16, true);
        let mut s = Surface::new(fmt);
        let r = Rect::new(0, 0, 256_000, 102_400);
        s.fill_rect(r, &[0.25, 0.5, 0.75, 1.0]);
        assert!(!too_sparse(r, stored_tiles(&s, r).len(), &fmt));
        assert!(too_sparse(r, 0, &fmt));
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
    let merged = merge_down(&doc, &lower, &upper).unwrap();
    assert_eq!(merged.surface().unwrap().pixel(0, 0), lower.surface().unwrap().pixel(0, 0));
    assert_eq!(merged.surface().unwrap().pixel(1, 0), upper.surface().unwrap().pixel(1, 0));
}
