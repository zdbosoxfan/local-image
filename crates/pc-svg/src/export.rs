//! Stable SVG writer adapted from VectorCraft `crates/svg/src/export.rs` at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Licensed under MIT OR Apache-2.0; see licenses/vectorcraft-*.
//! Geometry stays in document pixels. Physical root dimensions use the document DPI;
//! viewBox retains the scoped document-space origin. No resource URLs are emitted.

use base64::Engine as _;
use image::ImageEncoder as _;
use photocraft_doc::vector_design::appearance::{AppearanceItem, StrokeGradientMode};
use photocraft_doc::vector_design::exchange::{ExportScope, ExportedVector, VectorWarning};
use photocraft_doc::{BlendMode, DesignUnits, Document, Fill, FillRule, Layer, LayerContent, LayerId, Path, ShapeLayer, ShapeStroke};
use photocraft_export_common::validate::Preflight;
use photocraft_export_common::{
    Clipping, clipping_base_independent, clipping_end, clipping_source_eligible, closed_isolated_group, effect_margin, mask_allocation_bytes, normalized_fill,
    painted_shapes, partial_backdrop_dependency, soft_mask_sampling_canvas,
};
use photocraft_geom::Rect;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[path = "export/paint.rs"]
mod paint;
#[path = "export/text.rs"]
mod text;

pub type Result<T> = std::result::Result<T, ExportError>;
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("invalid SVG export: {0}")]
    Invalid(String),
    #[error("SVG export cancelled")]
    Cancelled,
    #[error("SVG export needs the shared {0} projection provider")]
    Projection(&'static str),
    #[error("SVG export exceeds its {0} budget")]
    Budget(&'static str),
}

/// Additive preferences. Precision is decimal places, 0..=9, and never silently clamped.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportOptions {
    pub text_as_curves: bool,
    pub precision: u8,
    pub minify: bool,
    pub max_pixels: u64,
    pub max_bytes: usize,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self { text_as_curves: false, precision: 6, minify: false, max_pixels: 16_777_216, max_bytes: 64 << 20 }
    }
}

/// Shared enhanced projection returns a replacement ordinary layer, retaining the source
/// layer's compositing envelope (opacity/masks/effects/artboard). The returned tree must
/// clear evaluated metadata. The adapter never evaluates corners/live/path-text itself.
/// Callers use the SAME projection as display; returning None is an explicit dependency error.
pub trait Projection {
    fn layer(&self, doc: &Document, layer: &Layer) -> Result<Option<Layer>>;
}

pub struct ExportContext<'a> {
    pub cancelled: &'a dyn Fn() -> bool,
    pub projection: Option<&'a dyn Projection>,
    /// Process-owned font database supplied by the caller, never created per layer.
    pub text: &'a mut photocraft_text::TextEngine,
}

/// Detached export: no document/cache/history mutation, file I/O, or system-font scanning.
pub fn export(doc: &Document, scope: &ExportScope, opts: &ExportOptions) -> Result<ExportedVector> {
    let mut text = photocraft_text::TextEngine::new();
    export_with(doc, scope, opts, &mut ExportContext { cancelled: &|| false, projection: None, text: &mut text })
}

pub fn export_with(doc: &Document, scope: &ExportScope, opts: &ExportOptions, cx: &mut ExportContext<'_>) -> Result<ExportedVector> {
    check_cancel(cx)?;
    validate(doc, opts)?;
    let mut scene = scoped_document(doc, scope)?;
    project(&mut scene.layers, doc, cx, 0)?;
    validate(&scene, opts)?;
    let rect = scope_rect(&scene, scope, cx)?;
    check_pixels(rect, opts)?;
    let mut w = Writer {
        isolated_raster: BTreeSet::new(),
        clipping_regions: BTreeMap::new(),
        doc: &scene,
        opts,
        cx,
        rect,
        defs: String::new(),
        body: String::new(),
        resources: BTreeMap::new(),
        warnings: BTreeMap::new(),
        next: 0,
        label: None,
    };
    let mut flatten = w.scan(&scene.layers);
    if scene.mode == photocraft_doc::ColorMode::Lab {
        w.warn("Lab compositing", "Lab-space compositing was embedded as PNG because SVG blends in RGB.");
        flatten = true;
    }
    if flatten {
        if partial_backdrop_dependency(doc, scope) {
            return Err(invalid("this partial nonisolated scope requires rasterization with its enclosing backdrop; export the document instead"));
        }
        w.warn("compositing", "The scoped composition was embedded as PNG to preserve adjustments, clipping or backdrop-dependent blending.");
        w.raster(&scene.layers, rect)?;
    } else {
        w.layers(&scene.layers, 0)?;
    }
    check_cancel(w.cx)?;
    let (unit, factor) = match scene.design_units {
        DesignUnits::Pixels => ("", 1.0),
        DesignUnits::Points => ("pt", 72.0 / f64::from(scene.resolution_dpi)),
        DesignUnits::Millimeters => ("mm", 25.4 / f64::from(scene.resolution_dpi)),
        DesignUnits::Inches => ("in", 1.0 / f64::from(scene.resolution_dpi)),
    };
    let nl = if opts.minify { "" } else { "\n" };
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}{unit}\" height=\"{}{unit}\" viewBox=\"{} {} {} {}\">{nl}",
        number(f64::from(rect.width()) * factor, opts.precision),
        number(f64::from(rect.height()) * factor, opts.precision),
        rect.x0,
        rect.y0,
        rect.width(),
        rect.height()
    );
    if !w.defs.is_empty() {
        out.push_str(&format!("<defs>{nl}{}</defs>{nl}", w.defs));
    }
    out.push_str(&w.body);
    out.push_str("</svg>");
    out.push_str(nl);
    if number(f64::from(rect.width()) * factor, opts.precision) == "0" || number(f64::from(rect.height()) * factor, opts.precision) == "0" {
        return Err(invalid("precision rounds physical size to zero; use more decimal places"));
    }
    if out.len() > opts.max_bytes {
        return Err(ExportError::Budget("output bytes"));
    }
    Ok(ExportedVector { bytes: out.into_bytes(), warnings: w.warnings.into_values().collect() })
}

// Photoshop Soft Light differs from the PDF/CSS low-backdrop polynomial.
// Keep that and nonstandard modes in the compositor fallback.
fn native_blend(mode: BlendMode) -> Option<&'static str> {
    Some(match mode {
        BlendMode::Normal | BlendMode::PassThrough => "normal",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color-dodge",
        BlendMode::ColorBurn => "color-burn",
        BlendMode::HardLight => "hard-light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
        _ => return None,
    })
}

impl From<photocraft_export_common::Error> for ExportError {
    fn from(error: photocraft_export_common::Error) -> Self {
        match error {
            photocraft_export_common::Error::Invalid(message) => Self::Invalid(message),
            photocraft_export_common::Error::Cancelled => Self::Cancelled,
            photocraft_export_common::Error::Budget(budget) => Self::Budget(budget),
        }
    }
}
fn invalid(message: impl Into<String>) -> ExportError {
    ExportError::Invalid(message.into())
}
fn check_cancel(cx: &ExportContext<'_>) -> Result<()> {
    if (cx.cancelled)() { Err(ExportError::Cancelled) } else { Ok(()) }
}
fn check_pixels(rect: Rect, opts: &ExportOptions) -> Result<()> {
    Ok(photocraft_export_common::check_pixels(rect, opts.max_pixels)?)
}
/// Bounded numeric/tree preflight precedes cloning, normalization, shaping and raster work.
fn validate(doc: &Document, opts: &ExportOptions) -> Result<()> {
    if opts.precision > 9 {
        return Err(invalid("invalid export options"));
    }
    Ok(photocraft_export_common::validate::document(doc, opts.max_pixels, opts.max_bytes, Preflight::Svg)?)
}

/// Validates every requested id before pruning, including hidden objects. Selecting both
/// a group and its descendant includes the group only once, in document paint order.
pub fn scoped_document(doc: &Document, scope: &ExportScope) -> Result<Document> {
    Ok(photocraft_export_common::scoped_document(doc, scope)?)
}

fn needs_paint_projection(layer: &Layer) -> bool {
    let pattern = |f: &Fill| matches!(f, Fill::Pattern { .. });
    if let Some(a) = &layer.vector.appearance {
        return a.items.iter().any(|i| match i {
            AppearanceItem::Fill(f) => f.visible && pattern(&f.paint),
            AppearanceItem::Mesh(_) => false,
            AppearanceItem::Stroke(s) => {
                s.visible
                    && (pattern(&s.base.paint)
                        || s.profile.is_some()
                        || s.brush.is_some()
                        || s.start_arrow.is_some()
                        || s.end_arrow.is_some()
                        || s.dash_align
                        || s.gradient_mode != StrokeGradientMode::Object)
            }
        });
    }
    match &layer.content {
        LayerContent::Shape(s) => s.fill.as_ref().is_some_and(pattern) || s.stroke.as_ref().is_some_and(|s| pattern(&s.paint)),
        LayerContent::Fill(f) => pattern(f),
        _ => false,
    }
}

fn project(layers: &mut [Layer], original: &Document, cx: &ExportContext<'_>, depth: usize) -> Result<()> {
    if depth > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(ExportError::Budget("projection depth"));
    }
    for layer in layers {
        check_cancel(cx)?;
        if !layer.visible {
            continue;
        }
        let feature = if !layer.vector.corners.is_empty() {
            Some("corners")
        } else if layer.vector.on_path.is_some() {
            Some("path text")
        } else if layer.vector.live.is_some() {
            Some("live geometry")
        } else if needs_paint_projection(layer) {
            Some("paint raster")
        } else {
            None
        };
        if let Some(feature) = feature {
            *layer = cx.projection.ok_or(ExportError::Projection(feature))?.layer(original, layer)?.ok_or(ExportError::Projection(feature))?;
            if !layer.vector.corners.is_empty() || layer.vector.on_path.is_some() || layer.vector.live.is_some() || needs_paint_projection(layer) {
                return Err(invalid("projection retained unevaluated metadata"));
            }
        }
        if let LayerContent::Group(g) = &mut layer.content {
            project(&mut g.children, original, cx, depth + 1)?;
        }
    }
    Ok(())
}

fn scope_rect(doc: &Document, scope: &ExportScope, cx: &mut ExportContext<'_>) -> Result<Rect> {
    match scope {
        ExportScope::Document => Ok(doc.bounds()),
        ExportScope::Artboard(id) => doc.layer(*id).and_then(Layer::artboard).map(|a| a.rect).ok_or_else(|| invalid("artboard disappeared")),
        ExportScope::Selection(_) => layer_bounds(&doc.layers, doc, cx, false),
    }
}

// Selection describes current semantic ink; raster crops must also retain authoritative
// enhanced caches and complete artboard compositing regions. Do not let conservative
// cache bounds change the selection viewBox or physical dimensions.
fn layer_bounds(layers: &[Layer], doc: &Document, cx: &mut ExportContext<'_>, raster_crop: bool) -> Result<Rect> {
    let mut rect = Rect::EMPTY;
    for l in layers.iter().filter(|l| l.visible) {
        check_cancel(cx)?;
        let b = if raster_crop && !photocraft_compose::transparent_outside(l) {
            doc.bounds()
        } else {
            match &l.content {
                LayerContent::Group(g) => {
                    let b = layer_bounds(&g.children, doc, cx, raster_crop)?;
                    g.artboard.as_ref().map_or(b, |a| if raster_crop { a.rect } else { b.intersect(&a.rect) })
                }
                LayerContent::Shape(s) if raster_crop && l.vector.appearance.is_some() => s.cache.as_ref().map_or(doc.bounds(), |cache| cache.content_bounds()),
                LayerContent::Shape(s) => {
                    // Bounds of ALL visible appearance items, not the PSD paint mirror.
                    let mut b = Rect::EMPTY;
                    for shape in painted_shapes(l, s) {
                        let r = if shape.stroke.as_ref().is_some_and(|s| s.width > 0.0) {
                            photocraft_vector::CompiledShape::new(&shape, 0.01, doc.bounds()).bounds()
                        } else if shape.fill.is_some() {
                            let path = normalized_fill(&shape.path, doc.bounds())?;
                            photocraft_pathops::bounds(&path)
                                .map_err(|e| invalid(e.to_string()))?
                                .map(|r| Rect::new(r.0.floor() as i32, r.1.floor() as i32, r.2.ceil() as i32, r.3.ceil() as i32))
                        } else {
                            None
                        };
                        if let Some(r) = r {
                            b = b.union(&r);
                        }
                    }
                    b
                }
                LayerContent::Text(t) => {
                    let layout = cx.text.layout(t, doc.resolution_dpi);
                    let warp = photocraft_text::render::layout_warp(&layout, t.warp.as_ref());
                    photocraft_text::render::ink_rect_warped(&layout, &t.transform, warp.as_ref())
                }
                LayerContent::Fill(_) | LayerContent::Adjustment(_) => doc.bounds(),
                _ => l.surface().map_or(Rect::EMPTY, |s| s.content_bounds()),
            }
        };
        let margin = effect_margin(l);
        let b = if !b.is_empty() && margin > 0 {
            Rect::new(b.x0.saturating_sub(margin), b.y0.saturating_sub(margin), b.x1.saturating_add(margin), b.y1.saturating_add(margin))
        } else {
            b
        };
        rect = rect.union(&b);
    }
    Ok(rect)
}

/// Bound compressed allocation and observe cancellation during encoder writes.
struct PngSink<'a> {
    bytes: Vec<u8>,
    cancelled: &'a dyn Fn() -> bool,
    limit: usize,
    exceeded: bool,
}
impl std::io::Write for PngSink<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if (self.cancelled)() {
            return Err(std::io::Error::other("SVG export cancelled"));
        }
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other("SVG embedded PNG exceeds output budget"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// A pass-through group paints its children straight onto the backdrop and then fades /
// masks / clips that result against the same backdrop. When every visible descendant
// composites with plain Normal (no blend modes, adjustments, effects, Blend If, channel
// limits or text gamma), that equals an isolated group with the same opacity, mask and
// clip, so it stays native (e.g. a Make Clipping Mask group keeps its `<clipPath>`).
fn pass_through_matches_isolated(children: &[Layer]) -> bool {
    children.iter().filter(|l| l.visible).all(|l| {
        l.excluded_channels == 0
            && l.blend_if.is_default()
            && !photocraft_compose::effects::has_effects(l)
            && photocraft_compose::text_gamma(l) == 1.0
            && match &l.content {
                LayerContent::Adjustment(_) => false,
                LayerContent::Group(g) => {
                    g.isolation != photocraft_doc::GroupIsolation::NonIsolated
                        && match l.blend {
                            BlendMode::Normal => true,
                            BlendMode::PassThrough
                                if g.isolation == photocraft_doc::GroupIsolation::Isolated
                                    || (g.isolation == photocraft_doc::GroupIsolation::Auto && l.fill_opacity < 1.0) =>
                            {
                                true
                            }
                            BlendMode::PassThrough => pass_through_matches_isolated(&g.children),
                            _ => false,
                        }
                }
                _ => l.blend == BlendMode::Normal,
            }
    })
}

struct Writer<'a, 'b, 'c> {
    isolated_raster: BTreeSet<LayerId>,
    clipping_regions: BTreeMap<LayerId, Rect>,
    doc: &'a Document,
    opts: &'a ExportOptions,
    cx: &'b mut ExportContext<'c>,
    rect: Rect,
    defs: String,
    body: String,
    resources: BTreeMap<String, String>,
    warnings: BTreeMap<String, VectorWarning>,
    next: usize,
    label: Option<(String, String)>,
}
impl Writer<'_, '_, '_> {
    fn n(&self, v: f64) -> String {
        number(v, self.opts.precision)
    }
    fn id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }
    fn line(&mut self, value: &str) -> Result<()> {
        check_cancel(self.cx)?;
        if self.body.len() + self.defs.len() + value.len() > self.opts.max_bytes {
            return Err(ExportError::Budget("output bytes"));
        }
        self.body.push_str(value);
        if !self.opts.minify {
            self.body.push('\n');
        }
        Ok(())
    }
    fn warn(&mut self, feature: &str, message: &str) {
        self.warnings.entry(feature.into()).or_insert_with(|| VectorWarning { feature: feature.into(), object: None, message: message.into() });
    }
    fn scan(&mut self, layers: &[Layer]) -> bool {
        let mut flatten = false;
        let mut i = 0;
        while i < layers.len() {
            let end = clipping_end(layers, i);
            let base = &layers[i];
            if base.visible && base.opacity > 0.0 {
                let active = layers[i + 1..end].iter().any(|l| l.visible && l.opacity > 0.0);
                if !active || !clipping_base_independent(base) {
                    flatten |= active || self.scan_plain(std::slice::from_ref(base));
                }
            }
            i = end;
        }
        flatten
    }
    fn scan_plain(&mut self, layers: &[Layer]) -> bool {
        let mut flatten = false;
        for layer in layers.iter().filter(|l| l.visible) {
            if native_blend(layer.blend).is_none()
                || !matches!(layer.blend, BlendMode::Normal | BlendMode::PassThrough)
                    && (photocraft_compose::effects::has_effects(layer) || layer.artboard().is_some())
                || layer.excluded_channels != 0
                || layer.blend_if != Default::default()
            {
                flatten = true;
            }
            if layer.mask.as_ref().is_some_and(|m| m.enabled) {
                self.warn("raster masks", "Raster masks were embedded as PNG.");
            }
            if layer.effects.enabled && layer.effects.items.iter().any(|e| e.enabled()) {
                self.warn("effects", "Layer effects were embedded as PNG.");
            }
            self.raster_reason(layer);
            match &layer.content {
                LayerContent::Group(g) => {
                    flatten |= g.isolation == photocraft_doc::GroupIsolation::NonIsolated;
                    if g.isolation == photocraft_doc::GroupIsolation::Auto
                        && layer.blend == BlendMode::PassThrough
                        && !closed_isolated_group(layer)
                        && (layer.opacity != 1.0
                            || layer.mask.is_some()
                            || layer.vector_mask.as_ref().is_some_and(|m| m.enabled)
                            || layer.vector.clip.is_some()
                            || layer.effects.items.iter().any(|e| e.enabled()))
                        && !pass_through_matches_isolated(&g.children)
                    {
                        flatten = true;
                    }
                    let inside = self.scan(&g.children);
                    if inside && closed_isolated_group(layer) {
                        self.isolated_raster.insert(layer.id);
                        self.warn(
                            "isolated composition",
                            "Unsupported compositing was embedded only within its closed isolated group; outside artwork remains native.",
                        );
                    } else {
                        flatten |= inside;
                    }
                }
                LayerContent::Adjustment(_) => {
                    self.warn("adjustments", "Adjustments were embedded in the scoped PNG composition.");
                    flatten = true;
                }
                LayerContent::Raster(_) | LayerContent::Smart(_) => {
                    self.warn("images", "Pixel and smart-object layers were embedded as PNG and remain raster images.")
                }
                _ => {}
            }
        }
        flatten
    }

    fn clipping_source(&mut self, base: &Layer, clipped: &[Layer]) -> Result<Option<Layer>> {
        if !clipping_source_eligible(self.doc, base, clipped) {
            return Ok(None);
        }
        let region = layer_bounds(std::slice::from_ref(base), self.doc, self.cx, true)?.intersect(&self.rect);
        let mut clipping = Clipping {
            doc: self.doc,
            rect: self.rect,
            max_pixels: self.opts.max_pixels,
            max_bytes: self.opts.max_bytes,
            cancelled: self.cx.cancelled,
            text: self.cx.text,
            regions: &mut self.clipping_regions,
            warnings: &mut self.warnings,
        };
        Ok(clipping.source(base, clipped, region)?)
    }
    fn layers(&mut self, layers: &[Layer], depth: usize) -> Result<()> {
        if depth > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(ExportError::Budget("depth"));
        }
        let mut i = 0;
        while i < layers.len() {
            check_cancel(self.cx)?;
            let end = clipping_end(layers, i);
            let base = &layers[i];
            let clipped = &layers[i + 1..end];
            if base.visible && base.opacity > 0.0 {
                if clipped.iter().any(|l| l.visible && l.opacity > 0.0) {
                    if let Some(source) = self.clipping_source(base, clipped)? {
                        self.layers_plain(std::slice::from_ref(&source), depth)?;
                    } else {
                        self.warn("clipping chain", "This isolated clipping chain was embedded as an image; unrelated artwork remains vector.");
                        let rect = layer_bounds(std::slice::from_ref(base), self.doc, self.cx, true)?.intersect(&self.rect);
                        if !rect.is_empty() {
                            let mut source = layers[i..end].to_vec();
                            source[0].blend = BlendMode::Normal;
                            let blend = native_blend(base.blend).ok_or_else(|| invalid("unsupported clipping base blend"))?;
                            self.line(&format!("<g style=\"mix-blend-mode:{blend}\">"))?;
                            self.raster(&source, rect)?;
                            self.line("</g>")?;
                        }
                    }
                } else {
                    self.layers_plain(std::slice::from_ref(base), depth)?;
                }
            }
            i = end;
        }
        Ok(())
    }
    fn layers_plain(&mut self, layers: &[Layer], depth: usize) -> Result<()> {
        if depth > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(ExportError::Budget("depth"));
        }
        for layer in layers.iter().filter(|l| l.visible && l.opacity > 0.0) {
            check_cancel(self.cx)?;
            if self.isolated_raster.contains(&layer.id) || self.raster_reason(layer) {
                let rect = layer_bounds(std::slice::from_ref(layer), self.doc, self.cx, true)?.intersect(&self.rect);
                if !rect.is_empty() {
                    // Encode source pixels/coverage once; the exported blend sees the
                    // actual backdrop instead of being baked against transparency.
                    let mut source = layer.clone();
                    source.blend = BlendMode::Normal;
                    let blend = native_blend(layer.blend).ok_or_else(|| invalid("unsupported blend reached writer"))?;
                    self.line(&format!("<g style=\"mix-blend-mode:{blend}\">"))?;
                    self.raster(std::slice::from_ref(&source), rect)?;
                    self.line("</g>")?;
                }
                continue;
            }
            let mut attrs = String::new();
            let blend = native_blend(layer.blend).ok_or_else(|| invalid("unsupported blend reached writer"))?;
            let isolated = matches!(&layer.content, LayerContent::Group(g) if g.isolation == photocraft_doc::GroupIsolation::Isolated || g.isolation == photocraft_doc::GroupIsolation::Auto && (layer.blend != BlendMode::PassThrough || layer.fill_opacity < 1.0));
            if blend != "normal" || isolated {
                attrs.push_str(&format!(" style=\"mix-blend-mode:{blend}{}\"", if isolated { ";isolation:isolate" } else { "" }));
            }
            if layer.opacity != 1.0 {
                attrs.push_str(&format!(" opacity=\"{}\"", self.n(f64::from(layer.opacity))));
            }
            let id = self.id("layer");
            // A single ordinary paint is a named path, rather than a synthetic group.
            // The public importer can then preserve the tree on subsequent exports.
            if let LayerContent::Shape(shape) = &layer.content
                && layer.vector.appearance.is_none()
                && layer.vector.clip.is_none()
                && layer.vector_mask.is_none()
                && !layer.mask.as_ref().is_some_and(|m| m.enabled)
                && layer.opacity == 1.0
                && layer.fill_opacity == 1.0
                && matches!(layer.blend, BlendMode::Normal | BlendMode::PassThrough)
                && (shape.fill.is_some() != shape.stroke.is_some())
            {
                self.label = Some((id, layer.name.clone()));
                self.shape(layer, shape)?;
                self.label = None;
                continue;
            }
            let soft_mask = layer.mask.as_ref().is_some_and(|m| m.enabled)
                || layer.vector_mask.as_ref().is_some_and(|m| m.enabled && (m.density != 1.0 || m.feather != 0.0 || m.path.is_empty() && m.path.inverted));
            if soft_mask {
                let mask = self.soft_mask(layer)?;
                attrs.push_str(&format!(" mask=\"url(#{mask})\""));
            }
            let mut clips = Vec::new();
            if let Some(p) = &layer.vector.clip {
                clips.push(p.clone());
            }
            if let Some(m) = &layer.vector_mask
                && !soft_mask
                && m.enabled
                && !m.path.is_empty()
            {
                clips.push(m.path.clone());
            }
            if let Some(a) = layer.artboard() {
                clips.push(Path::new(vec![photocraft_doc::Subpath::polygon(&[
                    (a.rect.x0 as f64, a.rect.y0 as f64),
                    (a.rect.x1 as f64, a.rect.y0 as f64),
                    (a.rect.x1 as f64, a.rect.y1 as f64),
                    (a.rect.x0 as f64, a.rect.y1 as f64),
                ])]));
            }
            if !clips.is_empty() {
                let mut compound = Path::default();
                let mut seen = BTreeSet::new();
                for clip in clips {
                    let mut p = normalized_fill(&clip, self.rect)?;
                    if p.is_empty() {
                        compound = p;
                        break;
                    }
                    let key = format!("{}:{}", rule(p.fill_rule), path_data(&p, self.opts.precision));
                    if !seen.insert(key) {
                        continue;
                    }
                    // Normalize each clip's fill rule before combining its geometry.
                    p = photocraft_pathops::finish_compound(&p, None).map_err(|e| invalid(e.to_string()))?;
                    if let Some(first) = p.subpaths.first_mut() {
                        first.op = if compound.is_empty() { photocraft_doc::PathOp::Combine } else { photocraft_doc::PathOp::Intersect };
                    }
                    compound.subpaths.extend(p.subpaths);
                }
                let clip = self.clip(&compound)?;
                attrs.push_str(&format!(" clip-path=\"url(#{clip})\""));
            }
            self.line(&format!("<g id=\"{id}\" data-name=\"{}\"{attrs}><title>{}</title>", escape(&layer.name), escape(&layer.name)))?;
            if layer.fill_opacity != 1.0 {
                self.line(&format!("<g opacity=\"{}\">", self.n(f64::from(layer.fill_opacity))))?;
            }
            match &layer.content {
                LayerContent::Group(g) => {
                    if let Some(a) = &g.artboard {
                        let color = match a.background {
                            photocraft_doc::ArtboardBackground::White => Some(photocraft_doc::Color::WHITE),
                            photocraft_doc::ArtboardBackground::Black => Some(photocraft_doc::Color::BLACK),
                            photocraft_doc::ArtboardBackground::Custom(c) => Some(c),
                            photocraft_doc::ArtboardBackground::Transparent => None,
                        };
                        if let Some(c) = color {
                            self.line(&format!(
                                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" fill-opacity=\"{}\"/>",
                                a.rect.x0,
                                a.rect.y0,
                                a.rect.width(),
                                a.rect.height(),
                                paint::color(&c),
                                self.n(f64::from(c.alpha))
                            ))?;
                        }
                    }
                    self.layers(&g.children, depth + 1)?;
                }
                LayerContent::Shape(s) => self.shape(layer, s)?,
                LayerContent::Text(t) => self.text(t)?,
                LayerContent::Fill(f) => {
                    let r = self.doc.bounds();
                    let p = Path {
                        subpaths: vec![photocraft_doc::Subpath::polygon(&[
                            (r.x0 as f64, r.y0 as f64),
                            (r.x1 as f64, r.y0 as f64),
                            (r.x1 as f64, r.y1 as f64),
                            (r.x0 as f64, r.y1 as f64),
                        ])],
                        ..Default::default()
                    };
                    self.path_fill(&p, f, 1.0)?;
                }
                _ => return Err(invalid("unhandled layer content")),
            }
            if layer.fill_opacity != 1.0 {
                self.line("</g>")?;
            }
            self.line("</g>")?;
        }
        Ok(())
    }
    fn raster_reason(&mut self, l: &Layer) -> bool {
        let mask = l.mask.as_ref().is_some_and(|m| m.enabled)
            || l.vector_mask.as_ref().is_some_and(|m| m.enabled && (m.density != 1.0 || m.feather != 0.0 || m.path.is_empty() && m.path.inverted));
        if mask {
            self.warn("masks", "Soft alpha masks were embedded as PNG; their artwork remains native where supported.");
        }
        let mut fallback =
            l.effects.enabled && l.effects.items.iter().any(|e| e.enabled()) || matches!(l.content, LayerContent::Raster(_) | LayerContent::Smart(_));
        let supported = |f: &Fill| paint::supported(f);
        match &l.content {
            LayerContent::Shape(s) => {
                if let Some(a) = &l.vector.appearance {
                    for item in &a.items {
                        match item {
                            AppearanceItem::Mesh(m) if m.visible => {
                                fallback = true;
                                self.warn("gradient-mesh", "Gradient meshes were embedded as PNG using the shared renderer.");
                            }
                            AppearanceItem::Fill(f) if f.visible => fallback |= !supported(&f.paint) || f.blend != BlendMode::Normal,
                            AppearanceItem::Stroke(s) if s.visible => {
                                fallback |= !supported(&s.base.paint)
                                    || s.blend != BlendMode::Normal
                                    || s.profile.is_some()
                                    || s.brush.is_some()
                                    || s.start_arrow.is_some()
                                    || s.end_arrow.is_some()
                                    || s.dash_align
                                    || s.gradient_mode != StrokeGradientMode::Object
                            }
                            _ => {}
                        }
                    }
                    if fallback {
                        self.warn("appearance", "Unsupported appearance items were embedded as PNG using the shared renderer.");
                    }
                } else {
                    fallback |= s.fill.as_ref().is_some_and(|f| !supported(f)) || s.stroke.as_ref().is_some_and(|s| !supported(&s.paint));
                }
                if fallback {
                    self.warn("paint", "Unsupported paints were embedded as PNG.");
                }
            }
            LayerContent::Text(t) => {
                let layout = self.cx.text.layout(t, self.doc.resolution_dpi);
                if t.antialias == photocraft_doc::text::AntiAlias::None
                    || layout.styles.iter().any(|s| s.faux_bold)
                    || layout.faces.iter().any(|f| f.embolden)
                    || photocraft_text::render::layout_warp(&layout, t.warp.as_ref()).is_some()
                {
                    self.warn("text rendering", "Text styling without an exact portable outline was embedded as PNG.");
                    fallback = true;
                }
            }
            LayerContent::Fill(f) if !supported(f) => {
                self.warn("paint", "Unsupported paints were embedded as PNG.");
                fallback = true;
            }
            _ => {}
        }
        fallback
    }
    fn raster(&mut self, layers: &[Layer], rect: Rect) -> Result<()> {
        check_pixels(rect, self.opts)?;
        let mut doc = self.doc.clone();
        doc.layers = layers.to_vec();
        // Refresh ordinary geometry only; projection supplies authoritative enhanced caches.
        prepare_caches(&mut doc, self.cx, self.opts, rect)?;
        check_cancel(self.cx)?;
        let mut rgba = Vec::new();
        photocraft_compose::render_bands(&doc, rect, 256, |band| {
            check_cancel(self.cx)?;
            rgba.extend_from_slice(&band.to_rgba8().pixels);
            check_cancel(self.cx)
        })?;
        check_cancel(self.cx)?;
        let mut sink = PngSink { bytes: Vec::new(), cancelled: self.cx.cancelled, limit: self.opts.max_bytes, exceeded: false };
        image::codecs::png::PngEncoder::new(&mut sink).write_image(&rgba, rect.width(), rect.height(), image::ExtendedColorType::Rgba8).map_err(|e| {
            if (self.cx.cancelled)() {
                ExportError::Cancelled
            } else if sink.exceeded {
                ExportError::Budget("embedded PNG bytes")
            } else {
                invalid(e.to_string())
            }
        })?;
        check_cancel(self.cx)?;
        let png = sink.bytes;
        if png.len().saturating_mul(4).div_ceil(3) > self.opts.max_bytes {
            return Err(ExportError::Budget("embedded PNG bytes"));
        }
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        self.line(&format!(
            "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" xlink:href=\"data:image/png;base64,{data}\"/>",
            rect.x0,
            rect.y0,
            rect.width(),
            rect.height()
        ))
    }
    fn soft_mask(&mut self, layer: &Layer) -> Result<String> {
        let rect = self.clipping_regions.get(&layer.id).copied().unwrap_or(self.rect);
        check_pixels(rect, self.opts)?;
        let sampling = soft_mask_sampling_canvas(layer, rect, self.doc.bounds())?;
        if !sampling.is_empty() {
            check_pixels(sampling, self.opts)?;
            // Two float blur planes are the largest contiguous allocations in
            // combined_mask; the existing pixel budget bounds their work.
            mask_allocation_bytes(sampling.size().area(), 2 * std::mem::size_of::<f32>() as u64)?;
        }
        let output_bytes = mask_allocation_bytes(rect.size().area(), 4)?;
        check_cancel(self.cx)?;
        let combined = photocraft_compose::masks::combined_mask(layer, sampling);
        check_cancel(self.cx)?;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(output_bytes).map_err(|_| ExportError::Budget("mask output allocation"))?;
        for y in rect.y0..rect.y1 {
            check_cancel(self.cx)?;
            for x in rect.x0..rect.x1 {
                let value =
                    combined.as_ref().map_or_else(|| layer.mask.as_ref().filter(|m| m.enabled).map_or(1.0, |m| m.value(x, y)), |m| m.sample_channel(x, y, 0));
                if !value.is_finite() {
                    return Err(invalid("nonfinite mask sample"));
                }
                pixels.extend([255, 255, 255, (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8]);
            }
        }
        let mut sink = PngSink { bytes: Vec::new(), cancelled: self.cx.cancelled, limit: self.opts.max_bytes, exceeded: false };
        image::codecs::png::PngEncoder::new(&mut sink).write_image(&pixels, rect.width(), rect.height(), image::ExtendedColorType::Rgba8).map_err(|e| {
            if (self.cx.cancelled)() {
                ExportError::Cancelled
            } else if sink.exceeded {
                ExportError::Budget("mask PNG bytes")
            } else {
                invalid(e.to_string())
            }
        })?;
        check_cancel(self.cx)?;
        if sink.bytes.len().saturating_mul(4).div_ceil(3) > self.opts.max_bytes {
            return Err(ExportError::Budget("mask PNG bytes"));
        }
        let data = base64::engine::general_purpose::STANDARD.encode(sink.bytes);
        let id = self.id("alpha");
        let definition = format!(
            "<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" style=\"mask-type:alpha\"><image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" xlink:href=\"data:image/png;base64,{data}\"/></mask>",
            rect.x0,
            rect.y0,
            rect.width(),
            rect.height(),
            rect.x0,
            rect.y0,
            rect.width(),
            rect.height()
        );
        if self.body.len() + self.defs.len() + definition.len() > self.opts.max_bytes {
            return Err(ExportError::Budget("output bytes"));
        }
        self.defs.push_str(&definition);
        Ok(id)
    }
    fn clip(&mut self, path: &Path) -> Result<String> {
        let p = normalized_fill(path, self.rect)?;
        let data = path_data(&p, self.opts.precision);
        let key = format!("clip:{}:{data}", rule(p.fill_rule));
        if let Some(id) = self.resources.get(&key) {
            return Ok(id.clone());
        }
        let id = self.id("clip");
        self.defs
            .push_str(&format!("<clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{data}\" clip-rule=\"{}\"/></clipPath>", rule(p.fill_rule)));
        self.resources.insert(key, id.clone());
        Ok(id)
    }
    fn shape(&mut self, layer: &Layer, s: &ShapeLayer) -> Result<()> {
        if let Some(a) = &layer.vector.appearance {
            for item in &a.items {
                match item {
                    AppearanceItem::Fill(f) if f.visible => self.path_fill(&s.path, &f.paint, f.opacity)?,
                    AppearanceItem::Stroke(st) if st.visible => self.path_stroke(&s.path, &st.base)?,
                    _ => {}
                }
            }
        } else {
            if let Some(f) = &s.fill {
                self.path_fill(&s.path, f, 1.0)?;
            }
            if let Some(st) = &s.stroke {
                self.path_stroke(&s.path, st)?;
            }
        }
        Ok(())
    }
    fn path_element(&mut self, attrs: &str) -> Result<()> {
        if let Some((id, name)) = self.label.take() {
            let name = escape(&name);
            self.line(&format!("<path id=\"{id}\" data-name=\"{name}\"{attrs}><title>{name}</title></path>"))
        } else {
            self.line(&format!("<path{attrs}/>"))
        }
    }
    fn path_fill(&mut self, path: &Path, fill: &Fill, opacity: f32) -> Result<()> {
        let p = normalized_fill(path, self.rect)?;
        let attrs = self.paint(fill, path, "fill", opacity)?;
        self.path_element(&format!(" d=\"{}\" fill-rule=\"{}\"{attrs}", path_data(&p, self.opts.precision), rule(p.fill_rule)))
    }
    fn path_stroke(&mut self, path: &Path, st: &ShapeStroke) -> Result<()> {
        if st.width == 0.0 || st.opacity == 0.0 {
            return Ok(());
        }
        if st.align != photocraft_doc::StrokeAlign::Center && path.subpaths.iter().all(|s| s.closed) {
            let p = photocraft_pathops::outline_stroke(path, st, 0.01).map_err(|e| invalid(e.to_string()))?;
            // Paint stays anchored to the source path's control bounds.
            let attrs = self.paint(&st.paint, path, "fill", st.opacity)?;
            return self.path_element(&format!(" d=\"{}\" fill-rule=\"{}\"{attrs}", path_data(&p, self.opts.precision), rule(p.fill_rule)));
        }
        let paint = self.paint(&st.paint, path, "stroke", st.opacity)?;
        let cap = match st.cap {
            photocraft_doc::LineCap::Butt => "butt",
            photocraft_doc::LineCap::Round => "round",
            photocraft_doc::LineCap::Square => "square",
        };
        let join = match st.join {
            photocraft_doc::LineJoin::Miter => "miter",
            photocraft_doc::LineJoin::Round => "round",
            photocraft_doc::LineJoin::Bevel => "bevel",
        };
        let mut dash = String::new();
        if st.dashes.iter().any(|d| *d > 0.0) {
            let values = st.dashes.iter().map(|d| self.n(f64::from(*d) * f64::from(st.width))).collect::<Vec<_>>().join(" ");
            dash = format!(" stroke-dasharray=\"{values}\" stroke-dashoffset=\"{}\"", self.n(f64::from(st.dash_offset) * f64::from(st.width)));
        }
        self.path_element(&format!(
            " d=\"{}\" fill=\"none\"{paint} stroke-width=\"{}\" stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\" stroke-miterlimit=\"{}\"{dash}",
            path_data(path, self.opts.precision),
            self.n(f64::from(st.width)),
            self.n(f64::from(st.miter_limit))
        ))
    }
}

/// Shared compositor caches for the detached raster fallback, bounded by the scoped region.
fn prepare_caches(doc: &mut Document, cx: &mut ExportContext<'_>, opts: &ExportOptions, export_rect: Rect) -> Result<()> {
    let space = photocraft_compose::cmyk_space(doc);
    let format = doc.pixel_format();
    let canvas = doc.bounds();
    let dpi = doc.resolution_dpi;
    fn padding(layers: &[Layer]) -> i32 {
        layers
            .iter()
            .map(|l| {
                effect_margin(l).saturating_add(match &l.content {
                    LayerContent::Group(g) => padding(&g.children),
                    _ => 0,
                })
            })
            .max()
            .unwrap_or(0)
    }
    let pad = padding(&doc.layers);
    let region = Rect::new(
        export_rect.x0.saturating_sub(pad),
        export_rect.y0.saturating_sub(pad),
        export_rect.x1.saturating_add(pad),
        export_rect.y1.saturating_add(pad),
    );
    check_pixels(region, opts)?;
    fn walk(
        layers: &mut [Layer],
        format: photocraft_doc::PixelFormat,
        canvas: Rect,
        region: Rect,
        dpi: f32,
        opts: &ExportOptions,
        cx: &mut ExportContext<'_>,
    ) -> Result<()> {
        for l in layers {
            check_cancel(cx)?;
            if let Some(clip) = l.vector.clip.take() {
                let clip = normalized_fill(&clip, region)?;
                let coverage = photocraft_vector::path_coverage(&clip, region);
                let previous = photocraft_compose::masks::combined_mask(l, region);
                let data = coverage
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let x = region.x0 + (i % region.width() as usize) as i32;
                        let y = region.y0 + (i / region.width() as usize) as i32;
                        *v * previous.as_ref().map_or_else(|| l.mask.as_ref().map_or(1.0, |m| m.value(x, y)), |s| s.sample_channel(x, y, 0))
                    })
                    .collect::<Vec<_>>();
                let mut surface = photocraft_doc::Surface::new(photocraft_doc::PixelFormat::new(
                    photocraft_doc::ColorMode::Grayscale,
                    photocraft_doc::SampleType::F32,
                    false,
                ));
                surface.write_region(region, &data);
                l.mask = Some(photocraft_doc::LayerMask { surface, enabled: true, linked: true, density: 1.0, feather: 0.0 });
                l.vector_mask = None;
            }
            match &mut l.content {
                LayerContent::Shape(s) if l.vector.appearance.is_none() => {
                    s.cache = Some(photocraft_vector::CompiledShape::new(s, 0.01, canvas).render(format, region))
                }
                LayerContent::Shape(s) if s.cache.is_none() => return Err(ExportError::Projection("appearance raster")),
                LayerContent::Text(t) => {
                    let layout = cx.text.layout(t, dpi);
                    let ink = photocraft_text::render::ink_rect(&layout, &t.transform);
                    if !ink.is_empty() {
                        check_pixels(ink, opts)?;
                    }
                    cx.text.render_layer(t, dpi, format);
                }
                LayerContent::Group(g) => walk(&mut g.children, format, canvas, region, dpi, opts, cx)?,
                _ => {}
            }
        }
        Ok(())
    }
    photocraft_compose::with_cmyk_space(space.as_ref(), || walk(&mut doc.layers, format, canvas, region, dpi, opts, cx))
}
fn rule(rule: FillRule) -> &'static str {
    match rule {
        FillRule::NonZero => "nonzero",
        FillRule::EvenOdd => "evenodd",
    }
}
/// Closed last-to-first curves are emitted before Z. Open endpoints' unused handles
/// never create a closing curve. Z (join) is distinct from an explicit final L (caps).
fn path_data(path: &Path, precision: u8) -> String {
    let mut out = String::new();
    let pt = |p: photocraft_geom::Point| format!("{} {}", number(p.x, precision), number(p.y, precision));
    for s in &path.subpaths {
        let Some(first) = s.knots.first() else { continue };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!("M{}", pt(first.anchor)));
        let segment = |a: &photocraft_doc::Knot, b: &photocraft_doc::Knot| {
            if a.out_ctrl == a.anchor && b.in_ctrl == b.anchor {
                format!(" L{}", pt(b.anchor))
            } else {
                format!(" C{} {} {}", pt(a.out_ctrl), pt(b.in_ctrl), pt(b.anchor))
            }
        };
        for pair in s.knots.windows(2) {
            out.push_str(&segment(&pair[0], &pair[1]));
        }
        if s.closed {
            if let Some(last) = s.knots.last()
                && (last.out_ctrl != last.anchor || first.in_ctrl != first.anchor)
            {
                out.push_str(&segment(last, first));
            }
            out.push_str(" Z");
        }
    }
    out
}
fn number(v: f64, precision: u8) -> String {
    let mut s = format!("{v:.p$}", p = usize::from(precision));
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    s
}
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if c == '\u{fffe}' || c == '\u{ffff}' || c < ' ' => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    out
}

#[test]
fn dsfix_isolated_child_does_not_force_outer_passthrough_raster() {
    let mut child =
        photocraft_export_common::isolated_group("isolated", vec![Layer::new("multiply", LayerContent::Fill(Fill::Solid(photocraft_doc::Color::WHITE)))]);
    child.children_mut().unwrap()[0].blend = BlendMode::Multiply;
    child.blend = BlendMode::PassThrough;
    assert!(pass_through_matches_isolated(&[child.clone()]));
    let LayerContent::Group(g) = &mut child.content else { panic!() };
    g.isolation = photocraft_doc::GroupIsolation::Auto;
    assert!(!pass_through_matches_isolated(&[child.clone()]));
    child.fill_opacity = 0.5;
    assert!(pass_through_matches_isolated(&[child]));
}
#[test]
fn native_blend_matches_shared_native_set() {
    for mode in BlendMode::LAYER_MODES.into_iter().chain([BlendMode::PassThrough]) {
        assert_eq!(native_blend(mode).is_some(), photocraft_export_common::native_blend(mode), "{mode:?}");
    }
}
