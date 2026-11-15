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
}

impl CloneFrom {
    /// Source pixels of `rect` in the native channels of the painted surface `surf`.
    pub(super) fn sample(&self, pre: &Document, target: Option<LayerId>, surf: &Surface, which: SampleLayers, rect: Rect, cmd: &str) -> Result<Region> {
        let doc = self.other.as_deref().unwrap_or(pre);
        let fmt = surf.format();
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
        convert_region(region, from, fmt, self.transform.as_deref(), cmd)
    }
}

/// `region` (in `from`'s channels) in `to`'s channels, through `transform` when the two
/// documents' profiles differ. Alpha is coverage and is carried over unconverted.
fn convert_region(region: Region, from: PixelFormat, to: PixelFormat, transform: Option<&Transform>, cmd: &str) -> Result<Region> {
    // A malformed document can hold a surface whose model disagrees with its ICC profile: check
    // the strides before the transform indexes them or its fixed scratch buffer.
    if transform.is_some_and(|t| t.inputs() != from.mode.color_channels() || t.outputs() != to.mode.color_channels() || t.outputs() > 16) {
        return Err(bad(cmd, "clone source color channels do not match the document profiles"));
    }
    if transform.is_none() && from.mode == to.mode && from.alpha == to.alpha {
        return Ok(region);
    }
    let (from_colors, colors) = (from.mode.color_channels(), to.mode.color_channels());
    let mut out = Region::new(region.rect, to.channels());
    for (src, dst) in region.data.chunks_exact(from.channels()).zip(out.data.chunks_exact_mut(to.channels())) {
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
    Ok(CloneFrom { other: external.then(|| st.doc.clone()), layer, transform })
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
