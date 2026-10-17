//! PSD advanced-vector fallback and the shared editability-loss boundary.
//!
//! Original Local Image code, following vector-architecture §7 and the November 2019
//! Adobe PSD specification's layer pixels / additional layer information. Existing vector
//! blocks are still written by vector_map's unchanged-model guards, not another encoder.
//! A private versioned marker tells our importer that the stored pixels are authoritative;
//! it contains no editable appearance, text, symbol or live-feature payload.

use std::collections::BTreeMap;

use photocraft_doc::vector_design::appearance::{AppearanceItem, StrokeGradientMode};
use photocraft_doc::vector_design::exchange::{ExportedVector, VectorWarning};
use photocraft_doc::vector_design::live::LiveGroup;
use photocraft_doc::{BlendMode, Document, Layer, LayerContent, ShapeLayer};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::{IoError, PsdExportOptions};

/// One stable entry per feature, independent of layer count and export traversal order.
/// SVG/PDF consumers can add their own losses using the same VectorWarning boundary.
#[derive(Clone, Debug, Default)]
pub struct LossCollector {
    losses: BTreeMap<String, VectorWarning>,
}

impl LossCollector {
    pub fn push(&mut self, warning: VectorWarning) {
        match self.losses.entry(warning.feature.clone()) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(warning);
            }
            std::collections::btree_map::Entry::Occupied(mut e) => {
                // A document-level loss must not appear to affect only its first object.
                if e.get().object != warning.object {
                    e.get_mut().object = None;
                }
                if warning.message < e.get().message {
                    e.get_mut().message = warning.message;
                }
            }
        }
    }

    pub fn extend(&mut self, warnings: impl IntoIterator<Item = VectorWarning>) {
        for warning in warnings {
            self.push(warning);
        }
    }

    pub fn warnings(&self) -> Vec<VectorWarning> {
        self.losses.values().cloned().collect()
    }

    /// One IO notice line containing every distinct loss. Empty collectors are silent.
    pub fn notice(&self) -> Option<String> {
        self.notice_for("PSD")
    }

    pub fn notice_for(&self, format: &str) -> Option<String> {
        self.notice_with(format, &str::to_owned)
    }

    /// UI supplies its existing catalog translator. Stable format/feature keys stay unchanged.
    pub fn notice_with(&self, format: &str, translate: &dyn Fn(&str) -> String) -> Option<String> {
        (!self.losses.is_empty()).then(|| {
            let labels = self.losses.values().map(|w| translate(w.message.trim_end_matches('.'))).collect::<Vec<_>>().join("; ");
            translate("{format} keeps rendered pixels; editability lost: {losses}.").replace("{format}", format).replace("{losses}", &labels)
        })
    }

    fn loss(&mut self, feature: &str, label: &str) {
        self.push(VectorWarning { feature: feature.into(), object: None, message: label.into() });
    }

    /// Includes hidden items/objects: their appearance settings also lose editability.
    pub fn layer(&mut self, layer: &Layer) {
        if matches!(layer.content, LayerContent::Shape(_)) && layer.vector_mask.is_some() {
            self.loss("independent-shape-vector-mask", "independent shape vector masks");
        }
        let v = &layer.vector;
        if let Some(a) = &v.appearance {
            self.loss("appearance-stack", "appearance stacks");
            let fills = a.items.iter().filter(|i| matches!(i, AppearanceItem::Fill(_))).count();
            if fills > 1 {
                self.loss("multiple-fills", "multiple fills");
            }
            if a.items.len() - fills > 1 {
                self.loss("multiple-strokes", "multiple strokes");
            }
            for i in &a.items {
                let (visible, blend) = match i {
                    AppearanceItem::Mesh(m) => {
                        self.loss("gradient-mesh", "gradient meshes");
                        (m.visible, m.blend)
                    }
                    AppearanceItem::Fill(f) => {
                        if f.opacity != 1.0 {
                            self.loss("paint-opacity", "per-paint opacity");
                        }
                        (f.visible, f.blend)
                    }
                    AppearanceItem::Stroke(s) => {
                        if s.base.opacity != 1.0 {
                            self.loss("paint-opacity", "per-paint opacity");
                        }
                        if s.profile.is_some() {
                            self.loss("width-profile", "width profiles");
                        }
                        if s.brush.is_some() {
                            self.loss("vector-brush", "vector brushes");
                        }
                        if s.start_arrow.is_some() || s.end_arrow.is_some() {
                            self.loss("arrowheads", "arrowheads");
                        }
                        if s.dash_align {
                            self.loss("dash-alignment", "dash alignment");
                        }
                        if s.scale_with_object {
                            self.loss("stroke-scaling", "stroke scaling with objects");
                        }
                        if s.gradient_mode != StrokeGradientMode::Object {
                            self.loss("stroke-gradient", "along/across stroke gradients");
                        }
                        (s.visible, s.blend)
                    }
                };
                if !visible {
                    self.loss("paint-visibility", "per-paint visibility");
                }
                if blend != BlendMode::Normal {
                    self.loss("paint-blend", "per-paint blend modes");
                }
            }
        }
        if v.mosaic.is_some() {
            self.loss("cutout-coverage", "seamless cutout coverage");
        }
        if v.frame_compose.is_some_and(|frame| frame.enabled) {
            self.loss("frame-composition", "frame composition");
        }
        if v.on_path.is_some() {
            self.loss("path-text", "text on a path");
        }
        if matches!(layer.content, LayerContent::Shape(_)) && needs_cache(layer) && layer.vector_mask.is_some() {
            self.loss("shape-vector-mask", "separate shape vector masks");
        }
        if v.live_shape.is_some() {
            self.loss("design-shape", "design shape parameters");
        }
        if !v.corners.is_empty() {
            self.loss("live-corners", "live corners");
        }
        if v.constraints.is_some() {
            self.loss("constraints", "layout constraints");
        }
        if v.clip.is_some() {
            self.loss("vector-crop", "vector crop controls");
        }
        match &v.live {
            Some(LiveGroup::Blend(_)) => self.loss("live-blend", "live blends"),
            Some(LiveGroup::Repeat(_)) => self.loss("live-repeat", "live repeats"),
            Some(LiveGroup::Envelope(_)) => self.loss("live-envelope", "live envelopes"),
            Some(LiveGroup::LivePaint(_)) => self.loss("live-paint", "live paint groups"),
            None => {}
        }
        if matches!(layer.content, LayerContent::Symbol(_)) {
            self.loss("symbols", "symbol definitions and instances");
        }
        if let LayerContent::Group(g) = &layer.content {
            for child in &g.children {
                self.layer(child);
            }
        }
    }

    pub fn document(doc: &Document) -> Self {
        let mut out = Self::default();
        for layer in &doc.layers {
            out.layer(layer);
        }
        if !doc.symbols.is_empty() {
            out.loss("symbols", "symbol definitions and instances");
            for master in &doc.symbols {
                for layer in &master.layers {
                    out.layer(layer);
                }
            }
        }
        out
    }
}

/// Common feature projection, also used by display and other adapters. Live groups have no
/// cache in the shared model. Their provider returns ordinary detached layers or a merged
/// raster with the original ID and compositing envelope; it must clear evaluated metadata.
pub trait Projection {
    fn layer(&self, document: &Document, layer: &Layer) -> Result<Option<Layer>, String>;
}

/// Validate/prep a detached export. Cached shape/type/symbol pixels are never re-rendered
/// from their legacy proxy. Missing live/crop providers or caches are errors, not empty output.
pub fn prepare(doc: &Document, projection: Option<&dyn Projection>) -> Result<Document, IoError> {
    if !doc.resolution_dpi.is_finite() || !(1.0..=9600.0).contains(&doc.resolution_dpi) {
        return Err(IoError::Unsupported("PSD resolution must be finite and between 1 and 9600 DPI".into()));
    }
    fn walk(source: &Document, layer: &Layer, projection: Option<&dyn Projection>, depth: usize) -> Result<Layer, IoError> {
        if depth > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(IoError::Unsupported("PSD projection exceeds the layer depth limit".into()));
        }
        let mut out = layer.clone();
        if photocraft_doc::vector_design::mosaic::active(layer) {
            let bounds = photocraft_compose::composite_bounds(layer, source.bounds()).unwrap_or(source.bounds()).intersect(&source.bounds());
            let surface = photocraft_compose::mosaic::surface(layer, bounds, 1.0, source.pixel_format())
                .ok_or_else(|| IoError::Unsupported("cutout coverage exceeds PSD raster budget".into()))?;
            out.content = LayerContent::Raster(surface);
            out.vector.mosaic = None;
        }
        if layer.vector.live.is_some() || layer.vector.clip.is_some() {
            out = projection
                .ok_or_else(|| IoError::Unsupported("PSD live/crop export requires the common feature projection (18–20 / vector crop)".into()))?
                .layer(source, layer)
                .map_err(IoError::Unsupported)?
                .ok_or_else(|| IoError::Unsupported("common projection did not evaluate the requested PSD fallback".into()))?;
            if out.id != layer.id || out.vector.live.is_some() || out.vector.clip.is_some() {
                return Err(IoError::Unsupported("common projection must retain the layer ID and clear evaluated live/crop metadata".into()));
            }
            if out.visible != layer.visible
                || out.opacity != layer.opacity
                || out.fill_opacity != layer.fill_opacity
                || out.blend != layer.blend
                || out.clipped != layer.clipped
                || out.vector_mask != layer.vector_mask
                || out.mask != layer.mask
                || out.effects != layer.effects
            {
                return Err(IoError::Unsupported("common projection must preserve the layer compositing envelope".into()));
            }
        }
        if let Some(a) = &out.vector.appearance {
            photocraft_vector::appearance::validate(a).map_err(IoError::Unsupported)?;
        }
        if needs_cache(&out) {
            let valid_path = |path: &photocraft_doc::Path| {
                path.subpaths
                    .iter()
                    .flat_map(|s| &s.knots)
                    .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
                    .all(|p| [p.x, p.y].into_iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.))
            };
            if let LayerContent::Shape(shape) = &out.content
                && !valid_path(&shape.path)
            {
                return Err(IoError::Unsupported("PSD fallback path must be finite and bounded".into()));
            }
            if let Some(text) = &out.vector.on_path
                && (!valid_path(&text.path) || ![text.start, text.end].into_iter().all(|v| v.is_finite() && (0.0..=1.0).contains(&v)))
            {
                return Err(IoError::Unsupported("PSD fallback path text must be finite and bounded".into()));
            }
            if let Some(mask) = &out.vector_mask
                && (!valid_path(&mask.path)
                    || !mask.density.is_finite()
                    || !(0.0..=1.0).contains(&mask.density)
                    || !mask.feather.is_finite()
                    || !(0.0..=1000.0).contains(&mask.feather))
            {
                return Err(IoError::Unsupported("PSD fallback vector mask must be finite and bounded".into()));
            }
        }
        if out.surface().is_none()
            && out.vector_mask.is_some()
            && out.vector.appearance.is_none()
            && out.vector.live_shape.is_none()
            && out.vector.corners.is_empty()
            && let LayerContent::Shape(shape) = &mut out.content
        {
            shape.cache = Some(photocraft_vector::render_shape(shape, source.pixel_format(), source.bounds()));
        }
        if needs_cache(&out) && out.surface().is_none() {
            return Err(IoError::Unsupported(format!("layer \"{}\": advanced PSD fallback requires its rendered cache", out.name)));
        }
        if let LayerContent::Group(g) = &mut out.content {
            g.children = g.children.iter().map(|l| walk(source, l, projection, depth + 1)).collect::<Result<_, _>>()?;
        }
        Ok(out)
    }
    let mut out = doc.clone();
    out.layers = doc.layers.iter().map(|l| walk(doc, l, projection, 0)).collect::<Result<_, _>>()?;
    Ok(out)
}

/// Lower adapter boundary used by guarded engine exports and the normal PSD/PSB save route.
/// Structured losses use VectorWarning, never an engine/UI dependency.
pub fn export(doc: &Document, options: &PsdExportOptions, projection: Option<&dyn Projection>) -> Result<ExportedVector, IoError> {
    let prepared = prepare(doc, projection)?;
    let losses = LossCollector::document(doc);
    let (file, io_warnings) = crate::document_to_psd_with(&prepared, options);
    file.header.validate()?;
    let mut warnings = losses;
    // The IO exporter already includes the same aggregated vector notice. Do not re-add it.
    let vector_notice = LossCollector::document(&prepared).notice();
    for message in io_warnings {
        if Some(&message) != vector_notice.as_ref() {
            warnings.push(VectorWarning { feature: format!("psd-io:{message}"), object: None, message });
        }
    }
    Ok(ExportedVector { bytes: file.to_bytes()?, warnings: warnings.warnings() })
}

pub(crate) fn needs_cache(layer: &Layer) -> bool {
    matches!(layer.content, LayerContent::Symbol(_))
        || matches!(layer.content, LayerContent::Shape(_))
            && (layer.vector_mask.is_some() || layer.vector.appearance.is_some() || layer.vector.live_shape.is_some() || !layer.vector.corners.is_empty())
        || matches!(layer.content, LayerContent::Text(_)) && (layer.vector.on_path.is_some() || layer.vector.frame_compose.is_some_and(|frame| frame.enabled))
}

/// Deterministic topmost visible supported fill/stroke using the single shared mirror rule.
pub fn shape_mirror(layer: &Layer, shape: &ShapeLayer) -> ShapeLayer {
    let mut out = shape.clone();
    if let Some(a) = &layer.vector.appearance {
        photocraft_vector::appearance::mirror_legacy(&mut out, a);
    }
    if layer.vector.live_shape.is_some() || !layer.vector.corners.is_empty() {
        out.live = None;
    }
    out
}

/// Bounds-limited source pixels, including an explicit empty surface for an empty stack.
pub(crate) fn bounded_cache(layer: &Layer, canvas: Rect) -> Option<Surface> {
    let surface = layer.surface()?;
    let bounds = surface.content_bounds();
    let clip = bounds.intersect(&canvas);
    let mut cache = if canvas.contains_rect(&bounds) {
        surface.clone()
    } else if clip.is_empty() {
        Surface::new(surface.format())
    } else {
        Surface::from_interleaved(surface.format(), clip, &surface.to_interleaved(clip))
    };
    if matches!(layer.content, LayerContent::Shape(_)) && layer.vector_mask.is_some() && !clip.is_empty() {
        // PSD has one vector-mask slot, used by the supported shape proxy. Bake a separate
        // user vector mask through the same native mask renderer, keeping the pixel mask
        // independent so it is still applied exactly once on import.
        let mut mask_layer = layer.clone();
        mask_layer.mask = None;
        if let Some(mask) = photocraft_compose::masks::combined_mask(&mask_layer, canvas) {
            let mut format = cache.format();
            format.alpha = true;
            let source = cache.convert(format);
            cache = Surface::new(format);
            let mut pixels = Vec::new();
            let mut weights = Vec::new();
            for y in clip.y0..clip.y1 {
                let row = Rect::new(clip.x0, y, clip.x1, y + 1);
                source.read_region_into(row, &mut pixels);
                mask.read_region_into(row, &mut weights);
                for (p, weight) in pixels.chunks_exact_mut(format.channels()).zip(&weights) {
                    p[format.channels() - 1] *= weight;
                }
                cache.write_region(row, &pixels);
            }
            cache.prune();
        }
    }
    Some(cache)
}

pub const CACHE_KEY: [u8; 4] = *b"liVF";
const CACHE_VERSION: &[u8] = b"Local Image vector cache\0\0\0\0\x01\0";

/// Pixel compositing behavior survives editability loss. This is not native editable data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CacheKind {
    Raster = 0,
    Shape = 1,
    PathText = 2,
}

pub fn cache_kind(data: &[u8]) -> Option<CacheKind> {
    if data.len() != CACHE_VERSION.len() + 50 || !data.starts_with(CACHE_VERSION) {
        return None;
    }
    let kind = match data[CACHE_VERSION.len()] {
        0 => Some(CacheKind::Raster),
        1 => Some(CacheKind::Shape),
        2 => Some(CacheKind::PathText),
        _ => None,
    }?;
    let flags = data[CACHE_VERSION.len() + 1];
    let frame = relative_frame(data);
    (flags <= u8::from(kind == CacheKind::Shape)
        && frame.iter().all(|v| v.unsigned_abs() <= 2_000_000)
        && frame[2] >= frame[0]
        && frame[3] >= frame[1]
        && (kind == CacheKind::Shape || frame == [0; 4]))
        .then_some(kind)
}

fn relative_frame(data: &[u8]) -> [i32; 4] {
    std::array::from_fn(|k| {
        let i = CACHE_VERSION.len() + 34 + k * 4;
        i32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
    })
}

/// Shape effect gradient/pattern frame, relative to cache origin so normal Move stays exact.
pub fn cache_paint_bounds(layer: &Layer) -> Option<Rect> {
    let (_, data) = layer.psd_blocks.iter().find(|(k, _)| k == &CACHE_KEY)?;
    if cache_kind(data) != Some(CacheKind::Shape) {
        return None;
    }
    let f = relative_frame(data);
    if f == [0; 4] {
        return None;
    }
    let b = layer.surface()?.content_bounds();
    Some(Rect::new(b.x0.checked_add(f[0])?, b.y0.checked_add(f[1])?, b.x0.checked_add(f[2])?, b.y0.checked_add(f[3])?))
}

/// Recognition is strict/versioned; an unrelated or truncated block never changes import.
pub fn is_cache_block(data: &[u8]) -> bool {
    cache_kind(data).is_some()
}

/// Shape proxy vmsk/vsms is already represented by the cache; ordinary user vector masks
/// on cached type/symbol layers remain separate and must still be imported/applied once.
pub fn cache_has_shape_proxy(data: &[u8]) -> bool {
    cache_kind(data) == Some(CacheKind::Shape) && data[CACHE_VERSION.len() + 1] == 1
}

fn cache_digest(surface: &Surface) -> blake3::Hash {
    let mut h = blake3::Hasher::new();
    let r = surface.content_bounds();
    for v in [r.x0, r.y0, r.x1, r.y1] {
        h.update(&v.to_be_bytes());
    }
    h.update(format!("{:?}", surface.format()).as_bytes());
    // Hash sparse content a row at a time, avoiding a full cache-sized allocation.
    for y in r.y0..r.y1 {
        h.update(&surface.to_interleaved(Rect::new(r.x0, y, r.x1, y + 1)));
    }
    h.finalize()
}

pub(crate) fn cache_block(surface: &Surface, kind: CacheKind, shape_proxy: bool, paint_bounds: Option<Rect>) -> Vec<u8> {
    let mut data = CACHE_VERSION.to_vec();
    data.extend_from_slice(&[kind as u8, u8::from(kind == CacheKind::Shape && shape_proxy)]);
    data.extend_from_slice(cache_digest(surface).as_bytes());
    let bounds = surface.content_bounds();
    let frame = paint_bounds
        .filter(|b| kind == CacheKind::Shape && !b.is_empty())
        .map_or([0; 4], |b| [b.x0 - bounds.x0, b.y0 - bounds.y0, b.x1 - bounds.x0, b.y1 - bounds.y0]);
    for v in frame {
        data.extend_from_slice(&v.to_be_bytes());
    }
    data
}

/// Raster pixel edits invalidate editable proxies, not their existing pixel blending rules.
pub(crate) fn refreshed_cache_block(layer: &Layer) -> Option<Vec<u8>> {
    let (_, data) = layer.psd_blocks.iter().find(|(k, _)| k == &CACHE_KEY)?;
    Some(cache_block(layer.surface()?, cache_kind(data)?, false, cache_paint_bounds(layer)))
}

/// A reopened raster fallback keeps its proxy blocks only while its pixels are unchanged.
pub fn cache_matches(layer: &Layer) -> bool {
    layer.psd_blocks.iter().find(|(k, _)| k == &CACHE_KEY).is_some_and(|(_, d)| {
        is_cache_block(d) && layer.surface().is_some_and(|s| d[CACHE_VERSION.len() + 2..CACHE_VERSION.len() + 34] == cache_digest(s).as_bytes()[..])
    })
}

pub(crate) fn proxy_key(key: &[u8; 4]) -> bool {
    matches!(key, b"vmsk" | b"vsms" | b"vscg" | b"vstk" | b"vogk" | b"SoCo" | b"GdFl" | b"PtFl")
}

#[cfg(test)]
#[path = "../tests/vector_fallback/data.rs"]
mod tests;
