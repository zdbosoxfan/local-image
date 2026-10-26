//! Document → PDF through krilla 0.8.2. Tree/paint organization adapted from
//! VectorCraft crates/pdf/src/export.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! MIT OR Apache-2.0; see licenses/vectorcraft-*. Scope/preflight follows the
//! existing Local Image SVG boundary. Geometry and glyphs come from pc-vector/pc-text.
use krilla::{
    geom::{PathBuilder, Transform},
    surface::Surface,
};
use photocraft_doc::vector_design::appearance::{AppearanceItem, StrokeGradientMode};
use photocraft_doc::vector_design::exchange::{ExportScope, ExportedVector, VectorWarning};
use photocraft_doc::{BlendMode, Document, Fill, FillRule, Layer, LayerContent, LayerId, Path, ShapeLayer, ShapeStroke};
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
#[path = "export/xmp.rs"]
mod xmp;
pub use xmp::FileInfo;
pub type Result<T> = std::result::Result<T, ExportError>;
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("invalid PDF export: {0}")]
    Invalid(String),
    #[error("PDF export cancelled")]
    Cancelled,
    #[error("PDF export needs the shared {0} projection provider")]
    Projection(&'static str),
    #[error("PDF export exceeds its {0} budget")]
    Budget(&'static str),
    #[error("PDF serialization failed: {0}")]
    Write(String),
}
/// Empty artboards means the scoped document is one page. With Document scope,
/// artboards selects pages in the supplied order; every ID is validated first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportOptions {
    pub text_as_curves: bool,
    pub artboards: Vec<LayerId>,
    pub compress: bool,
    pub max_pixels: u64,
    pub max_bytes: usize,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self { text_as_curves: false, artboards: Vec::new(), compress: true, max_pixels: 16_777_216, max_bytes: 64 << 20 }
    }
}
/// Same detached ordinary-layer projection used by display. Evaluated metadata must
/// be cleared; the adapter does not implement another live geometry evaluator.
pub trait Projection {
    fn layer(&self, doc: &Document, layer: &Layer) -> Result<Option<Layer>>;
}
pub struct ExportContext<'a> {
    pub cancelled: &'a dyn Fn() -> bool,
    pub projection: Option<&'a dyn Projection>,
    pub text: &'a mut photocraft_text::TextEngine,
}
pub fn export(doc: &Document, scope: &ExportScope, opts: &ExportOptions) -> Result<ExportedVector> {
    let mut text = photocraft_text::TextEngine::new();
    export_with(doc, scope, opts, &mut ExportContext { cancelled: &|| false, projection: None, text: &mut text })
}
pub fn export_with(doc: &Document, scope: &ExportScope, opts: &ExportOptions, cx: &mut ExportContext<'_>) -> Result<ExportedVector> {
    export_with_info(doc, scope, opts, cx, &FileInfo::default())
}

/// Export with the editor's selected File Info fields, excluding camera and location metadata.
pub fn export_with_info(doc: &Document, scope: &ExportScope, opts: &ExportOptions, cx: &mut ExportContext<'_>, info: &FileInfo) -> Result<ExportedVector> {
    check_cancel(cx)?;
    validate(doc, opts)?;
    let scopes = page_scopes(doc, scope, opts)?;
    let mut pdf = krilla::Document::new_with(krilla::SerializeSettings {
        compress_content_streams: opts.compress,
        configuration: krilla::configure::ConfigurationBuilder::new()
            .with_version(krilla::configure::PdfVersion::Pdf17)
            .finish()
            .map_err(|e| ExportError::Write(format!("{e:?}")))?,
        ..Default::default()
    });
    pdf.set_metadata(xmp::metadata(&doc.name, info, opts.max_bytes)?);
    let mut warnings = BTreeMap::new();
    for scope in scopes {
        check_cancel(cx)?;
        let mut scene = scoped_document(doc, &scope)?;
        project(&mut scene.layers, doc, cx, 0)?;
        validate(&scene, opts)?;
        let rect = scope_rect(&scene, &scope, cx)?;
        check_pixels(rect, opts)?;
        let factor = 72.0 / scene.resolution_dpi;
        let page_settings =
            krilla::page::PageSettings::from_wh(rect.width() as f32 * factor, rect.height() as f32 * factor).ok_or_else(|| invalid("page size is invalid"))?;
        let mut page = pdf.start_page_with(page_settings);
        let mut surface = page.surface();
        // krilla has a top-left y-down surface and inserts PDF's y flip itself.
        surface.push_transform(&Transform::from_row(factor, 0.0, 0.0, factor, -rect.x0 as f32 * factor, -rect.y0 as f32 * factor));
        let mut w = Writer { isolated_raster: BTreeSet::new(), clipping_regions: BTreeMap::new(), doc: &scene, opts, cx, rect, warnings: &mut warnings };
        let result = if w.scan(&scene.layers) || scene.mode == photocraft_doc::ColorMode::Lab {
            if partial_backdrop_dependency(doc, &scope) {
                Err(invalid("this partial nonisolated scope requires rasterization with its enclosing backdrop; export the document instead"))
            } else {
                w.warn("compositing", "The scoped composition was embedded as an image to preserve unsupported compositing.");
                w.raster(&mut surface, &scene.layers, rect)
            }
        } else {
            w.layers(&mut surface, &scene.layers, 0)
        };
        surface.pop();
        surface.finish();
        page.finish();
        result?;
    }
    check_cancel(cx)?;
    let bytes = pdf.finish().map_err(|e| ExportError::Write(format!("{e:?}")))?;
    check_cancel(cx)?;
    let bytes = xmp::apply(bytes, info, opts.max_bytes)?;
    check_cancel(cx)?;
    if bytes.len() > opts.max_bytes {
        return Err(ExportError::Budget("output bytes"));
    }
    Ok(ExportedVector { bytes, warnings: warnings.into_values().collect() })
}
/// Target-set preflight, shared by the engine and exporter. No shaping or raster work.
pub fn page_scopes(doc: &Document, scope: &ExportScope, opts: &ExportOptions) -> Result<Vec<ExportScope>> {
    if opts.artboards.len() > 1024 {
        return Err(ExportError::Budget("pages"));
    }
    if opts.artboards.is_empty() {
        scoped_document(doc, scope)?;
        return Ok(vec![scope.clone()]);
    }
    if *scope != ExportScope::Document {
        return Err(invalid("page list requires Document scope"));
    }
    let mut ids = BTreeSet::new();
    for id in &opts.artboards {
        if !ids.insert(id.0) {
            return Err(invalid("duplicate artboard page"));
        }
        if doc.layer(*id).and_then(Layer::artboard).is_none() {
            return Err(invalid("artboard page is missing"));
        }
    }
    Ok(opts.artboards.iter().copied().map(ExportScope::Artboard).collect())
}
// The compositor uses Photoshop's Soft Light variant, not PDF's low-tone
// polynomial. Unsupported/nonstandard modes remain whole-composition fallbacks.
fn native_blend(mode: BlendMode) -> Option<krilla::blend::BlendMode> {
    use krilla::blend::BlendMode as B;
    Some(match mode {
        BlendMode::Normal | BlendMode::PassThrough => B::Normal,
        BlendMode::Multiply => B::Multiply,
        BlendMode::Screen => B::Screen,
        BlendMode::Overlay => B::Overlay,
        BlendMode::Darken => B::Darken,
        BlendMode::Lighten => B::Lighten,
        BlendMode::ColorDodge => B::ColorDodge,
        BlendMode::ColorBurn => B::ColorBurn,
        BlendMode::HardLight => B::HardLight,
        BlendMode::Difference => B::Difference,
        BlendMode::Exclusion => B::Exclusion,
        BlendMode::Hue => B::Hue,
        BlendMode::Saturation => B::Saturation,
        BlendMode::Color => B::Color,
        BlendMode::Luminosity => B::Luminosity,
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
    Ok(photocraft_export_common::validate::document(doc, opts.max_pixels, opts.max_bytes, Preflight::Pdf)?)
}

/// Validates every requested id before pruning, including hidden objects. Selecting both
/// a group and its descendant includes the group only once, in document paint order.
pub fn scoped_document(doc: &Document, scope: &ExportScope) -> Result<Document> {
    Ok(photocraft_export_common::scoped_document(doc, scope)?)
}

fn needs_paint_projection(layer: &Layer) -> bool {
    if let Some(a) = &layer.vector.appearance {
        return a.items.iter().any(|i| match i {
            AppearanceItem::Fill(_) | AppearanceItem::Mesh(_) => false,
            AppearanceItem::Stroke(s) => {
                s.visible
                    && (s.profile.is_some()
                        || s.brush.is_some()
                        || s.start_arrow.is_some()
                        || s.end_arrow.is_some()
                        || s.dash_align
                        || s.gradient_mode != StrokeGradientMode::Object)
            }
        });
    }
    false
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
        } else if matches!(layer.content, LayerContent::Symbol(_)) {
            Some("symbols")
        } else if needs_paint_projection(layer) {
            Some("paint raster")
        } else {
            None
        };
        if let Some(feature) = feature {
            *layer = cx.projection.ok_or(ExportError::Projection(feature))?.layer(original, layer)?.ok_or(ExportError::Projection(feature))?;
            if !layer.vector.corners.is_empty()
                || layer.vector.on_path.is_some()
                || layer.vector.live.is_some()
                || matches!(layer.content, LayerContent::Symbol(_))
                || needs_paint_projection(layer)
            {
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
        ExportScope::Selection(_) => layer_bounds(&doc.layers, doc, cx),
    }
}

fn layer_bounds(layers: &[Layer], doc: &Document, cx: &mut ExportContext<'_>) -> Result<Rect> {
    let mut rect = Rect::EMPTY;
    for l in layers.iter().filter(|l| l.visible) {
        check_cancel(cx)?;
        let b = match &l.content {
            LayerContent::Group(g) => {
                let b = layer_bounds(&g.children, doc, cx)?;
                g.artboard.as_ref().map_or(b, |a| a.rect)
            }
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
        patterns: &[photocraft_doc::Pattern],
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
                LayerContent::Shape(s)
                    if l.vector.appearance.is_none()
                        && !s.fill.as_ref().is_some_and(|f| matches!(f, Fill::Pattern { .. }))
                        && !s.stroke.as_ref().is_some_and(|s| matches!(s.paint, Fill::Pattern { .. })) =>
                {
                    s.cache = Some(photocraft_vector::CompiledShape::new(s, 0.01, canvas).render(format, region))
                }
                LayerContent::Shape(s) => {
                    s.cache = Some(
                        photocraft_vector::appearance::CompiledAppearance::new(s, l.vector.appearance.as_ref(), patterns, canvas)
                            .map_err(invalid)?
                            .render(format, region)
                            .map_err(invalid)?,
                    );
                }
                LayerContent::Fill(f) if matches!(f, Fill::Pattern { .. }) => {
                    let shape = ShapeLayer { path: rect_path(canvas), fill: Some(f.clone()), ..Default::default() };
                    let surface = photocraft_vector::appearance::CompiledAppearance::new(&shape, None, patterns, canvas)
                        .map_err(invalid)?
                        .render(format, region)
                        .map_err(invalid)?;
                    l.fill_cache = Some(photocraft_doc::FillCache { fill: f.clone(), surface });
                }
                LayerContent::Text(t) => {
                    let layout = cx.text.layout(t, dpi);
                    let ink = photocraft_text::render::ink_rect(&layout, &t.transform);
                    if !ink.is_empty() {
                        check_pixels(ink, opts)?;
                    }
                    cx.text.render_layer(t, dpi, format);
                }
                LayerContent::Group(g) => walk(&mut g.children, format, canvas, region, dpi, patterns, opts, cx)?,
                _ => {}
            }
        }
        Ok(())
    }
    photocraft_compose::with_cmyk_space(space.as_ref(), || walk(&mut doc.layers, format, canvas, region, dpi, &doc.patterns, opts, cx))
}

struct Writer<'a, 'b, 'c> {
    isolated_raster: BTreeSet<LayerId>,
    clipping_regions: BTreeMap<LayerId, Rect>,
    doc: &'a Document,
    opts: &'a ExportOptions,
    cx: &'b mut ExportContext<'c>,
    rect: Rect,
    warnings: &'b mut BTreeMap<String, VectorWarning>,
}
impl Writer<'_, '_, '_> {
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
        layers.iter().filter(|l| l.visible).any(|l| {
            let unsupported = native_blend(l.blend).is_none()
                || !matches!(l.blend, BlendMode::Normal | BlendMode::PassThrough) && (photocraft_compose::effects::has_effects(l) || l.artboard().is_some())
                || l.excluded_channels != 0
                || l.blend_if != Default::default();
            match &l.content {
                LayerContent::Adjustment(_) => true,
                LayerContent::Group(g) => {
                    let inside = self.scan(&g.children);
                    let local = inside && closed_isolated_group(l);
                    if local {
                        self.isolated_raster.insert(l.id);
                        self.warn(
                            "isolated composition",
                            "Unsupported compositing was embedded only within its closed isolated group; outside artwork remains native.",
                        );
                    }
                    inside && !local
                        || g.isolation == photocraft_doc::GroupIsolation::NonIsolated
                        || unsupported
                        || g.isolation == photocraft_doc::GroupIsolation::Auto
                            && l.blend == BlendMode::PassThrough
                            && !closed_isolated_group(l)
                            && (l.opacity != 1.0
                                || l.mask.as_ref().is_some_and(|m| m.enabled)
                                || l.vector_mask.as_ref().is_some_and(|m| m.enabled)
                                || l.vector.clip.is_some()
                                || photocraft_compose::effects::has_effects(l))
                            && !pass_through_matches_isolated(&g.children)
                }
                _ => unsupported,
            }
        })
    }
    fn fallback(&mut self, l: &Layer) -> bool {
        let mut fallback = l.effects.enabled && l.effects.items.iter().any(|e| e.enabled());
        match &l.content {
            LayerContent::Raster(_) | LayerContent::Smart(_) => fallback = true,
            LayerContent::Shape(s) => {
                if let Some(a) = &l.vector.appearance {
                    for i in &a.items {
                        fallback |= match i {
                            AppearanceItem::Mesh(m) => {
                                if m.visible {
                                    self.warn("gradient_mesh", "Gradient meshes were embedded as an image using the shared renderer.");
                                }
                                m.visible
                            }
                            AppearanceItem::Fill(f) => f.visible && (f.blend != BlendMode::Normal || !paint::supported(&f.paint)),
                            AppearanceItem::Stroke(s) => s.visible && (s.blend != BlendMode::Normal || !paint::supported(&s.base.paint)),
                        };
                    }
                } else {
                    fallback |= s.fill.as_ref().is_some_and(|f| !paint::supported(f)) || s.stroke.as_ref().is_some_and(|s| !paint::supported(&s.paint));
                }
            }
            LayerContent::Fill(f) => fallback |= !paint::supported(f),
            LayerContent::Text(t) => {
                // Faux-bold's dilation has no exact public outline in pc-text.
                let layout = self.cx.text.layout(t, self.doc.resolution_dpi);
                fallback |= layout.faces.iter().any(|f| f.embolden)
                    || layout.styles.iter().any(|s| s.faux_bold)
                    || t.antialias == photocraft_doc::text::AntiAlias::None;
                fallback |= t.warp.is_some() && !layout.decorations.is_empty();
            }
            _ => {}
        }
        if fallback {
            self.warn("raster fallback", "Pixel layers, effects or unsupported paint/text styling were embedded using the shared compositor.");
        }
        fallback
    }

    fn clipping_source(&mut self, base: &Layer, clipped: &[Layer]) -> Result<Option<Layer>> {
        if !clipping_source_eligible(self.doc, base, clipped) {
            return Ok(None);
        }
        let region = layer_bounds(std::slice::from_ref(base), self.doc, self.cx)?.intersect(&self.rect);
        let mut clipping = Clipping {
            doc: self.doc,
            rect: self.rect,
            max_pixels: self.opts.max_pixels,
            max_bytes: self.opts.max_bytes,
            cancelled: self.cx.cancelled,
            text: self.cx.text,
            regions: &mut self.clipping_regions,
            warnings: self.warnings,
        };
        Ok(clipping.source(base, clipped, region)?)
    }
    fn layers(&mut self, s: &mut Surface<'_>, layers: &[Layer], depth: usize) -> Result<()> {
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
                        self.layers_plain(s, std::slice::from_ref(&source), depth)?;
                    } else {
                        self.warn("clipping chain", "This isolated clipping chain was embedded as an image; unrelated artwork remains vector.");
                        let rect = layer_bounds(std::slice::from_ref(base), self.doc, self.cx)?.intersect(&self.rect);
                        if !rect.is_empty() {
                            let mut source = layers[i..end].to_vec();
                            source[0].blend = BlendMode::Normal;
                            s.push_blend_mode(native_blend(base.blend).ok_or_else(|| invalid("unsupported clipping base blend"))?);
                            let result = self.raster(s, &source, rect);
                            s.pop();
                            result?;
                        }
                    }
                } else {
                    self.layers_plain(s, std::slice::from_ref(base), depth)?;
                }
            }
            i = end;
        }
        Ok(())
    }
    fn layers_plain(&mut self, s: &mut Surface<'_>, layers: &[Layer], depth: usize) -> Result<()> {
        if depth > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(ExportError::Budget("depth"));
        }
        for l in layers.iter().filter(|l| l.visible && l.opacity > 0.0) {
            check_cancel(self.cx)?;
            if self.isolated_raster.contains(&l.id) || self.fallback(l) {
                let rect = layer_bounds(std::slice::from_ref(l), self.doc, self.cx)?.intersect(&self.rect);
                if !rect.is_empty() {
                    let mut source = l.clone();
                    source.blend = BlendMode::Normal;
                    s.push_blend_mode(native_blend(l.blend).ok_or_else(|| invalid("unsupported blend reached writer"))?);
                    let result = self.raster(s, std::slice::from_ref(&source), rect);
                    s.pop();
                    result?;
                }
                continue;
            }
            // Balance every krilla push even on cancellation/error; its Drop checks this.
            let mut pushes = 1;
            s.push_blend_mode(native_blend(l.blend).ok_or_else(|| invalid("unsupported blend reached writer"))?);
            s.push_opacity(norm(l.opacity));
            pushes += 1;
            let isolate = !matches!(l.blend, BlendMode::Normal | BlendMode::PassThrough)
                || matches!(&l.content, LayerContent::Group(g) if g.isolation == photocraft_doc::GroupIsolation::Isolated || g.isolation == photocraft_doc::GroupIsolation::Auto && (l.blend != BlendMode::PassThrough || l.fill_opacity < 1.0));
            let result = (|| {
                if let Some(p) = &l.vector.clip {
                    let clip = normalized_fill(p, self.rect)?;
                    if let Some(p) = pdf_path(&clip)? {
                        s.push_clip_path(&p, &rule(clip.fill_rule));
                        pushes += 1;
                    } else {
                        return Ok(());
                    }
                }
                if let Some(a) = l.artboard()
                    && let Some(p) = pdf_path(&rect_path(a.rect))?
                {
                    s.push_clip_path(&p, &krilla::paint::FillRule::NonZero);
                    pushes += 1;
                }
                let has_pixel = l.mask.as_ref().is_some_and(|m| m.enabled);
                let vector = l.vector_mask.as_ref().filter(|m| m.enabled);
                if has_pixel || vector.is_some_and(|m| m.feather > 0.0) {
                    let mask = self.soft_mask(s, l)?;
                    s.push_mask(mask);
                    pushes += 1;
                } else if let Some(m) = vector {
                    if m.path.is_empty() {
                        s.push_opacity(norm(if m.path.inverted { 1.0 - m.density } else { 1.0 }));
                        pushes += 1;
                    } else {
                        let p = normalized_fill(&m.path, self.rect)?;
                        if m.density == 1.0 {
                            if let Some(path) = pdf_path(&p)? {
                                s.push_clip_path(&path, &rule(p.fill_rule));
                                pushes += 1;
                            } else {
                                return Ok(());
                            }
                        } else {
                            let mut builder = s.stream_builder();
                            let mut ms = builder.surface();
                            draw_solid(&mut ms, &rect_path(self.rect), photocraft_doc::Color::WHITE, 1.0 - m.density)?;
                            draw_solid(&mut ms, &p, photocraft_doc::Color::WHITE, 1.0)?;
                            ms.finish();
                            let mask = krilla::mask::Mask::new(builder.finish(), krilla::mask::MaskType::Alpha);
                            s.push_mask(mask);
                            pushes += 1;
                        }
                    }
                }
                // Apply geometry/masks to the completed group object, rather
                // than independently clipping every color-chain operation.
                if isolate {
                    s.push_isolated();
                    pushes += 1;
                }
                s.push_opacity(norm(l.fill_opacity));
                pushes += 1;
                match &l.content {
                    LayerContent::Group(g) => {
                        if let Some(a) = &g.artboard {
                            let c = match a.background {
                                photocraft_doc::ArtboardBackground::White => Some(photocraft_doc::Color::WHITE),
                                photocraft_doc::ArtboardBackground::Black => Some(photocraft_doc::Color::BLACK),
                                photocraft_doc::ArtboardBackground::Custom(c) => Some(c),
                                photocraft_doc::ArtboardBackground::Transparent => None,
                            };
                            if let Some(c) = c {
                                draw_solid(s, &rect_path(a.rect), c, 1.0)?;
                            }
                        }
                        self.layers(s, &g.children, depth + 1)
                    }
                    LayerContent::Shape(sh) => self.shape(s, l, sh),
                    LayerContent::Text(t) => self.text(s, t),
                    LayerContent::Fill(f) => self.path_fill(s, &rect_path(self.doc.bounds()), f, 1.0),
                    _ => Err(invalid("unhandled projected content")),
                }
            })();
            for _ in 0..pushes {
                s.pop();
            }
            result?;
        }
        Ok(())
    }
    fn shape(&mut self, s: &mut Surface<'_>, l: &Layer, shape: &ShapeLayer) -> Result<()> {
        if let Some(a) = &l.vector.appearance {
            for i in &a.items {
                check_cancel(self.cx)?;
                match i {
                    AppearanceItem::Fill(f) if f.visible => self.path_fill(s, &shape.path, &f.paint, f.opacity)?,
                    AppearanceItem::Stroke(st) if st.visible => self.path_stroke(s, &shape.path, &st.base)?,
                    _ => {}
                }
            }
        } else {
            if let Some(f) = &shape.fill {
                self.path_fill(s, &shape.path, f, 1.0)?;
            }
            if let Some(st) = &shape.stroke {
                self.path_stroke(s, &shape.path, st)?;
            }
        }
        Ok(())
    }
    fn path_fill(&mut self, s: &mut Surface<'_>, path: &Path, fill: &Fill, opacity: f32) -> Result<()> {
        let p = normalized_fill(path, self.rect)?;
        let Some(pdf) = pdf_path(&p)? else {
            return Ok(());
        };
        let (paint, alpha) = paint::convert(self.doc, fill, path, self.cx)?;
        s.set_stroke(None);
        s.set_fill(Some(krilla::paint::Fill { paint, opacity: norm(alpha * opacity), rule: rule(p.fill_rule) }));
        s.draw_path(&pdf);
        Ok(())
    }
    fn path_stroke(&mut self, s: &mut Surface<'_>, path: &Path, st: &ShapeStroke) -> Result<()> {
        if st.width == 0.0 || st.opacity == 0.0 {
            return Ok(());
        }
        if st.align != photocraft_doc::StrokeAlign::Center && path.subpaths.iter().all(|s| s.closed) {
            let p = photocraft_pathops::outline_stroke(path, st, 0.01).map_err(|e| invalid(e.to_string()))?;
            let Some(pdf) = pdf_path(&p)? else {
                return Ok(());
            };
            let (paint, alpha) = paint::convert(self.doc, &st.paint, path, self.cx)?;
            s.set_stroke(None);
            s.set_fill(Some(krilla::paint::Fill { paint, opacity: norm(alpha * st.opacity), rule: rule(p.fill_rule) }));
            s.draw_path(&pdf);
            return Ok(());
        }
        let Some(p) = pdf_path(path)? else {
            return Ok(());
        };
        let (paint, alpha) = paint::convert(self.doc, &st.paint, path, self.cx)?;
        let line_cap = match st.cap {
            photocraft_doc::LineCap::Butt => krilla::paint::LineCap::Butt,
            photocraft_doc::LineCap::Round => krilla::paint::LineCap::Round,
            photocraft_doc::LineCap::Square => krilla::paint::LineCap::Square,
        };
        let line_join = match st.join {
            photocraft_doc::LineJoin::Miter => krilla::paint::LineJoin::Miter,
            photocraft_doc::LineJoin::Round => krilla::paint::LineJoin::Round,
            photocraft_doc::LineJoin::Bevel => krilla::paint::LineJoin::Bevel,
        };
        let dash = st
            .dashes
            .iter()
            .any(|v| *v > 0.0)
            .then(|| krilla::paint::StrokeDash { array: st.dashes.iter().map(|v| v * st.width).collect(), offset: st.dash_offset * st.width });
        s.set_fill(None);
        s.set_stroke(Some(krilla::paint::Stroke {
            paint,
            width: st.width,
            miter_limit: st.miter_limit,
            line_cap,
            line_join,
            opacity: norm(alpha * st.opacity),
            dash,
        }));
        s.draw_path(&p);
        Ok(())
    }
    fn raster(&mut self, s: &mut Surface<'_>, layers: &[Layer], rect: Rect) -> Result<()> {
        check_pixels(rect, self.opts)?;
        let mut doc = self.doc.clone();
        doc.layers = layers.to_vec();
        prepare_caches(&mut doc, self.cx, self.opts, rect)?;
        check_cancel(self.cx)?;
        let mut rgba = Vec::new();
        photocraft_compose::render_bands(&doc, rect, 256, |band| {
            check_cancel(self.cx)?;
            if band.px.iter().flatten().any(|v| !v.is_finite()) {
                return Err(invalid("nonfinite raster sample"));
            }
            rgba.extend_from_slice(&band.to_rgba8().pixels);
            check_cancel(self.cx)
        })?;
        check_cancel(self.cx)?;
        draw_image(s, rgba, rect)
    }
    fn soft_mask(&mut self, s: &mut Surface<'_>, l: &Layer) -> Result<krilla::mask::Mask> {
        self.warn("soft masks", "Feathered and pixel masks were embedded as soft mask images; their artwork stays vector.");
        let rect = self.clipping_regions.get(&l.id).copied().unwrap_or(self.rect);
        check_pixels(rect, self.opts)?;
        let sampling = soft_mask_sampling_canvas(l, rect, self.doc.bounds())?;
        if !sampling.is_empty() {
            check_pixels(sampling, self.opts)?;
            // Two float blur planes are the largest contiguous allocations in
            // combined_mask; the existing pixel budget bounds their work.
            mask_allocation_bytes(sampling.size().area(), 2 * std::mem::size_of::<f32>() as u64)?;
        }
        let output_bytes = mask_allocation_bytes(rect.size().area(), 4)?;
        check_cancel(self.cx)?;
        let combined = photocraft_compose::masks::combined_mask(l, sampling);
        check_cancel(self.cx)?;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(output_bytes).map_err(|_| ExportError::Budget("mask output allocation"))?;
        for y in rect.y0..rect.y1 {
            check_cancel(self.cx)?;
            for x in rect.x0..rect.x1 {
                let value =
                    combined.as_ref().map_or_else(|| l.mask.as_ref().filter(|m| m.enabled).map_or(1.0, |m| m.value(x, y)), |m| m.sample_channel(x, y, 0));
                if !value.is_finite() {
                    return Err(invalid("nonfinite mask sample"));
                }
                pixels.extend([255, 255, 255, (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8]);
            }
        }
        let mut builder = s.stream_builder();
        let mut ms = builder.surface();
        draw_image(&mut ms, pixels, rect)?;
        ms.finish();
        Ok(krilla::mask::Mask::new(builder.finish(), krilla::mask::MaskType::Alpha))
    }
}
fn norm(v: f32) -> krilla::num::NormalizedF32 {
    krilla::num::NormalizedF32::new(v.clamp(0.0, 1.0)).unwrap_or(krilla::num::NormalizedF32::ZERO)
}
fn rule(r: FillRule) -> krilla::paint::FillRule {
    match r {
        FillRule::NonZero => krilla::paint::FillRule::NonZero,
        FillRule::EvenOdd => krilla::paint::FillRule::EvenOdd,
    }
}
fn rect_path(r: Rect) -> Path {
    Path::new(vec![photocraft_doc::Subpath::polygon(&[
        (r.x0 as f64, r.y0 as f64),
        (r.x1 as f64, r.y0 as f64),
        (r.x1 as f64, r.y1 as f64),
        (r.x0 as f64, r.y1 as f64),
    ])])
}
fn pdf_path(p: &Path) -> Result<Option<krilla::geom::Path>> {
    let mut b = PathBuilder::new();
    for sub in &p.subpaths {
        let Some(first) = sub.knots.first() else {
            continue;
        };
        b.move_to(first.anchor.x as f32, first.anchor.y as f32);
        let mut segment = |a: &photocraft_doc::Knot, z: &photocraft_doc::Knot| {
            if a.out_ctrl == a.anchor && z.in_ctrl == z.anchor {
                b.line_to(z.anchor.x as f32, z.anchor.y as f32);
            } else {
                b.cubic_to(a.out_ctrl.x as f32, a.out_ctrl.y as f32, z.in_ctrl.x as f32, z.in_ctrl.y as f32, z.anchor.x as f32, z.anchor.y as f32);
            }
        };
        for pair in sub.knots.windows(2) {
            segment(&pair[0], &pair[1]);
        }
        if sub.closed {
            if let Some(last) = sub.knots.last()
                && (last.out_ctrl != last.anchor || first.in_ctrl != first.anchor)
            {
                segment(last, first);
            }
            b.close();
        }
    }
    Ok(b.finish())
}
fn draw_solid(s: &mut Surface<'_>, p: &Path, c: photocraft_doc::Color, opacity: f32) -> Result<()> {
    if let Some(p) = pdf_path(p)? {
        s.set_stroke(None);
        s.set_fill(Some(krilla::paint::Fill { paint: paint::color(&c).into(), opacity: norm(c.alpha * opacity), rule: Default::default() }));
        s.draw_path(&p);
    }
    Ok(())
}
fn draw_image(s: &mut Surface<'_>, pixels: Vec<u8>, rect: Rect) -> Result<()> {
    let size = krilla::geom::Size::from_wh(rect.width() as f32, rect.height() as f32).ok_or_else(|| invalid("image size is invalid"))?;
    s.push_transform(&Transform::from_translate(rect.x0 as f32, rect.y0 as f32));
    s.draw_image(krilla::image::Image::from_rgba8(pixels, rect.width(), rect.height()), size);
    s.pop();
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    #[test]
    fn dsfix_selected_empty_artboard_exports_its_background() {
        let mut doc = Document::new("board", photocraft_geom::Size::new(100, 100), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
        let mut layer = Layer::group("board", vec![]);
        if let LayerContent::Group(g) = &mut layer.content {
            let mut board = photocraft_doc::Artboard::new(Rect::new(10, 20, 50, 60));
            board.background = photocraft_doc::ArtboardBackground::White;
            g.artboard = Some(board);
        }
        let id = layer.id;
        doc.layers.push(layer);
        let bytes = export(&doc, &ExportScope::Selection(vec![id]), &ExportOptions::default()).unwrap().bytes;
        let pdf = hayro_syntax::Pdf::new(bytes).unwrap();
        assert_eq!(pdf.pages()[0].base_dimensions(), (40., 40.));
    }
    #[test]
    fn dsfix_raster_text_bounds_include_warped_ink() {
        let mut doc = Document::new("warped", photocraft_geom::Size::new(256, 256), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
        let text = photocraft_doc::TextLayer {
            text: "HHHH".into(),
            size_pt: 24.,
            transform: photocraft_doc::Affine::translate(100., 100.),
            warp: Some(photocraft_doc::text::TextWarp { style: "warpArc".into(), value: 70., ..Default::default() }),
            ..Default::default()
        };
        let mut engine = photocraft_text::TextEngine::new();
        let layout = engine.layout(&text, doc.resolution_dpi);
        let warp = photocraft_text::render::layout_warp(&layout, text.warp.as_ref());
        let expected = photocraft_text::render::ink_rect_warped(&layout, &text.transform, warp.as_ref());
        assert_ne!(expected, photocraft_text::render::ink_rect(&layout, &text.transform));
        doc.layers.push(Layer::new("type", LayerContent::Text(text)));
        let mut cx = ExportContext { cancelled: &|| false, projection: None, text: &mut engine };
        assert_eq!(layer_bounds(&doc.layers, &doc, &mut cx).unwrap(), expected);
    }
    #[test]
    fn dsfix_raster_layer_embeds_only_its_content_bounds() {
        use hayro_syntax::object::{Name, Object};
        let mut doc = Document::new("bounded image", photocraft_geom::Size::new(1000, 1000), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
        let mut pixels = photocraft_doc::Surface::new(doc.pixel_format());
        pixels.write_region(Rect::new(40, 60, 48, 64), &vec![1.; 8 * 4 * 4]);
        doc.layers.push(Layer::new("small image", LayerContent::Raster(pixels)));
        let bytes = export(&doc, &ExportScope::Document, &ExportOptions::default()).unwrap().bytes;
        let pdf = hayro_syntax::Pdf::new(bytes).unwrap();
        let sizes: Vec<_> = pdf
            .objects()
            .into_iter()
            .filter_map(|o| {
                if let Object::Stream(s) = o {
                    (s.dict().get::<Name<'_>>(b"Subtype").is_some_and(|n| n.as_ref() == b"Image"))
                        .then(|| (s.dict().get::<u32>(b"Width").unwrap(), s.dict().get::<u32>(b"Height").unwrap()))
                } else {
                    None
                }
            })
            .collect();
        assert!(!sizes.is_empty());
        assert!(sizes.iter().all(|s| *s == (8, 4)), "{sizes:?}");
    }
}

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
                            BlendMode::PassThrough => pass_through_matches_isolated(&g.children),
                            _ => false,
                        }
                }
                _ => l.blend == BlendMode::Normal,
            }
    })
}
#[test]
fn native_blend_matches_shared_native_set() {
    for mode in BlendMode::LAYER_MODES.into_iter().chain([BlendMode::PassThrough]) {
        assert_eq!(native_blend(mode).is_some(), photocraft_export_common::native_blend(mode), "{mode:?}");
    }
}
