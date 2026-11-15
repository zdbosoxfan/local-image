//! Where a clone stroke reads its pixels: the layer and document picked with the Clone Source
//! slot, resolved once per stroke. Another document's colours are converted to the painted
//! document's profile.

use std::sync::Arc;

use photocraft_cms::{Transform, TransformOptions};
use photocraft_color::ColorMode;

use super::*;

#[derive(Clone, Default)]
pub(super) struct CloneFrom {
    /// The other open document the source was picked in.
    other: Option<Arc<Document>>,
    /// The picked layer that Current Layer sampling reads instead of the paint target.
    layer: Option<LayerId>,
    transform: Option<Arc<Transform>>,
    /// The stroke paints layer pixels, not a mask or channel.
    pixels: bool,
}

impl CloneFrom {
    /// The channels the source pixels come in: the painted surface's, with a coverage channel for
    /// layer pixels even when that surface stores no alpha. Masks and channels keep their own.
    pub(super) fn sample_format(&self, surf: &Surface) -> PixelFormat {
        let fmt = surf.format();
        if self.pixels { PixelFormat::new(fmt.mode, fmt.sample, true) } else { fmt }
    }

    /// Source pixels of `rect` in [`CloneFrom::sample_format`] of the painted surface `surf`.
    pub(super) fn sample(&self, pre: &Document, target: Option<LayerId>, surf: &Surface, which: SampleLayers, rect: Rect, cmd: &str) -> Result<Region> {
        let doc = self.other.as_deref().unwrap_or(pre);
        let fmt = self.sample_format(surf);
        if self.other.is_some() && fmt.mode != pre.mode {
            return Err(bad(cmd, "the destination layer color model does not match its document"));
        }
        let (region, from) = if self.other.is_some() && which != SampleLayers::Current {
            let from = PixelFormat::new(ColorMode::Rgb, SampleType::F32, true);
            (composite_region(doc, self.layer, which, rect, from), from)
        } else {
            let sampled = match self.layer {
                Some(id) => doc.layer(id).and_then(|l| l.surface()).ok_or_else(|| bad(cmd, "the clone source layer no longer has pixels"))?,
                None => surf,
            };
            if self.other.is_some() && sampled.format().mode != doc.mode {
                return Err(bad(cmd, "the source layer color model does not match its document; choose All Layers"));
            }
            (sample(doc, target, sampled, which, rect), sampled.format())
        };
        convert_region(region, from, fmt, self.transform.as_deref(), self.pixels.then(|| doc.bounds()), cmd)
    }
}

/// Opaque native pixels with a coverage channel added, colours untouched.
pub(super) fn opaque_alpha(region: Region, fmt: PixelFormat) -> Region {
    if fmt.alpha {
        return region;
    }
    let n = fmt.channels();
    let mut out = Region::new(region.rect, n + 1);
    for (src, dst) in region.data.chunks_exact(n).zip(out.data.chunks_exact_mut(n + 1)) {
        dst[..n].copy_from_slice(src);
        dst[n] = 1.0;
    }
    out
}

/// Zero the part of `region` outside `bounds`.
fn clip_canvas(region: &mut Region, bounds: Rect) {
    let n = region.ch;
    let row_values = region.width() * n;
    if row_values == 0 {
        return;
    }
    let (x0, x1) = (i64::from(region.rect.x0), i64::from(region.rect.x1));
    let left = (i64::from(bounds.x0).clamp(x0, x1) - x0) as usize * n;
    let right = (i64::from(bounds.x1).clamp(x0, x1) - x0) as usize * n;
    for (i, row) in region.data.chunks_exact_mut(row_values).enumerate() {
        let y = i64::from(region.rect.y0) + i as i64;
        if y < i64::from(bounds.y0) || y >= i64::from(bounds.y1) {
            row.fill(0.0);
        } else {
            row[..left].fill(0.0);
            row[right..].fill(0.0);
        }
    }
}

/// `region` (in `from`'s channels) in `to`'s channels, through `transform` when the two
/// documents' profiles differ. Alpha is coverage and is carried over unconverted. With a `canvas`
/// (the source document's bounds), pixels outside it are no sample: a surface without alpha is
/// opaque inside the canvas, black included, and reads as its default colour outside it.
fn convert_region(mut region: Region, from: PixelFormat, to: PixelFormat, transform: Option<&Transform>, canvas: Option<Rect>, cmd: &str) -> Result<Region> {
    // A malformed document can hold a surface whose model disagrees with its ICC profile: check
    // the strides before the transform indexes them or its fixed scratch buffer.
    if transform.is_some_and(|t| t.inputs() != from.mode.color_channels() || t.outputs() != to.mode.color_channels() || t.outputs() > 16) {
        return Err(bad(cmd, "clone source color channels do not match the document profiles"));
    }
    let canvas = canvas.filter(|bounds| region.rect.intersect(bounds) != region.rect);
    if transform.is_none() && from.mode == to.mode && from.alpha == to.alpha {
        if let Some(bounds) = canvas {
            clip_canvas(&mut region, bounds);
        }
        return Ok(region);
    }
    let (from_colors, colors) = (from.mode.color_channels(), to.mode.color_channels());
    let mut out = Region::new(region.rect, to.channels());
    let width = region.width();
    for (i, (src, dst)) in region.data.chunks_exact(from.channels()).zip(out.data.chunks_exact_mut(to.channels())).enumerate() {
        if let Some(bounds) = canvas {
            let (x, y) = (i64::from(region.rect.x0) + (i % width) as i64, i64::from(region.rect.y0) + (i / width) as i64);
            if x < i64::from(bounds.x0) || y < i64::from(bounds.y0) || x >= i64::from(bounds.x1) || y >= i64::from(bounds.y1) {
                continue;
            }
        }
        if let Some(transform) = transform {
            let mut converted = [0.0f32; 16];
            transform.eval(&src[..from_colors], &mut converted);
            dst[..colors].copy_from_slice(&converted[..colors]);
        } else if from.mode == to.mode {
            dst[..colors].copy_from_slice(&src[..colors]);
        } else {
            dst.copy_from_slice(&photocraft_raster::convert_pixel(&from, &to, src));
            continue;
        }
        if to.alpha {
            dst[colors] = if from.alpha { src[from_colors] } else { 1.0 };
        }
    }
    Ok(out)
}

/// The source of a clone stroke with params `p`. A stroke that names its own `source` or `offset`
/// samples the painted document; one that follows the slot samples the document and layer the
/// slot was picked on, and fails when either is gone.
pub(super) fn resolve(s: &Session, p: &Value, cmd: &str) -> Result<CloneFrom> {
    let picked = crate::presets::clone_source::uses_slot(p);
    let slot = s.presets.clone.active();
    if !targets_pixels(p) {
        // A mask or channel has no counterpart in another document: refuse instead of cloning
        // the painted document's own mask or channel.
        if picked && slot.document.is_some_and(|id| s.active().is_none_or(|st| st.doc.id != id)) {
            return Err(bad(cmd, "cross-document cloning requires a pixel-layer destination"));
        }
        return Ok(CloneFrom::default());
    }
    let which = sample_layers(p, cmd)?;
    let active = s.active().ok_or(EngineError::NoDocument)?;
    let doc_id = picked.then_some(slot.document).flatten().unwrap_or(active.doc.id);
    let st = s.documents().iter().find(|st| st.doc.id == doc_id).ok_or_else(|| bad(cmd, "the clone source document was closed; pick a new source"))?;
    let external = st.doc.id != active.doc.id;
    let mut layer = None;
    if which == SampleLayers::Current || (external && which == SampleLayers::CurrentAndBelow) {
        let id = match p.get("sourceLayer") {
            Some(value) => Some(LayerId(value.as_u64().ok_or_else(|| bad(cmd, "sourceLayer must be a layer id"))?)),
            None if picked => slot.layer.map(LayerId),
            None => None,
        }
        .or(if external { st.active_layer } else { None });
        if let Some(id) = id {
            let source = st.doc.layer(id).ok_or_else(|| bad(cmd, "the clone source layer no longer exists; pick a new source"))?;
            if source.surface().is_some() || which != SampleLayers::Current {
                layer = Some(id);
            } else if external {
                return Err(bad(cmd, "the clone source layer has no pixels; choose All Layers or pick a pixel layer"));
            }
        } else if external && which == SampleLayers::Current {
            return Err(bad(cmd, "the clone source has no pixel layer"));
        }
    }
    let transform = if external { color_transform(s, &st.doc, &active.doc, which, cmd)? } else { None };
    Ok(CloneFrom { other: external.then(|| st.doc.clone()), layer, transform, pixels: true })
}

/// The conversion from the sampled pixels' profile (the layer's document profile, or the
/// compositor's RGB for several layers) to the painted document's; `None` when they are the same.
fn color_transform(s: &Session, doc: &Document, target: &Document, which: SampleLayers, cmd: &str) -> Result<Option<Arc<Transform>>> {
    let current = which == SampleLayers::Current;
    let source = if current { crate::color_cmds::document_profile(doc) } else { crate::color_cmds::composite_profile(doc) };
    let destination = crate::color_cmds::document_profile(target);
    let inputs = if current { doc.mode.color_channels() } else { 3 };
    if source.channels() != inputs || destination.channels() != target.mode.color_channels() {
        return Err(bad(cmd, "cross-document cloning does not support this color model"));
    }
    if source.content_hash() == destination.content_hash() {
        return Ok(None);
    }
    let opts = TransformOptions { intent: s.color.settings.intent(), bpc: s.color.settings.bpc, ..Default::default() };
    photocraft_cms::cached(&source, &destination, opts).map(Some).map_err(|e| bad(cmd, format!("cannot convert clone source colors: {e}")))
}

/// [`apply_coverage`] for clone paint that carries coverage (see [`CloneFrom::sample_format`])
/// onto a surface that may store no alpha. Such a surface is composited in 64 px blocks through
/// an alpha-bearing float copy, so the temporary stays small and nothing is quantized before
/// it is blended.
pub(super) fn apply_clone_coverage(
    target: &mut Surface,
    rect: Rect,
    cov: &[f32],
    opacity: f32,
    selection: Option<&Surface>,
    lock: bool,
    paint: &Region,
    mode: BlendMode,
) -> Rect {
    let fmt = target.format();
    let n = fmt.channels();
    if fmt.alpha || paint.ch == n || rect.is_empty() {
        return apply_coverage(target, rect, cov, opacity, selection, lock, paint, mode);
    }
    let full_width = rect.width() as usize;
    let mut y = rect.y0;
    while y < rect.y1 {
        let y1 = y.saturating_add(64).min(rect.y1);
        let mut x = rect.x0;
        while x < rect.x1 {
            let x1 = x.saturating_add(64).min(rect.x1);
            let block = Rect::new(x, y, x1, y1);
            let width = block.width() as usize;
            let mut block_cov = vec![0.0; width * block.height() as usize];
            for (row, values) in block_cov.chunks_exact_mut(width).enumerate() {
                let offset = (y.abs_diff(rect.y0) as usize + row) * full_width + x.abs_diff(rect.x0) as usize;
                if let Some(source) = cov.get(offset..offset + width) {
                    values.copy_from_slice(source);
                }
            }
            // Skip blocks the stroke or the source does not reach, by alpha and never by colour:
            // an all-zero black source pixel inside the canvas is opaque paint.
            let covered = block_cov.iter().any(|value| *value > 0.0);
            let sampled = covered && (y..y1).any(|py| (x..x1).any(|px| paint.px(px, py).get(n).is_some_and(|alpha| *alpha > 0.0)));
            if sampled {
                let mut patch = Surface::new(PixelFormat::new(fmt.mode, SampleType::F32, true));
                opaque_alpha(Region::read(target, block), fmt).write(&mut patch);
                apply_coverage(&mut patch, block, &block_cov, opacity, selection, lock, paint, mode);
                let native: Vec<f32> = patch.read_region(block).chunks_exact(n + 1).flat_map(|px| px[..n].iter().copied()).collect();
                target.write_region(block, &native);
            }
            x = x1;
        }
        y = y1;
    }
    rect
}
