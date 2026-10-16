use super::*;
use photocraft_doc::Fill;
use photocraft_doc::vector_design::exchange::VectorWarning;
use std::collections::BTreeMap;

/// Writer state read and recorded by a native clipping-chain decomposition.
pub struct Clipping<'a> {
    pub doc: &'a Document,
    pub rect: Rect,
    pub max_pixels: u64,
    pub max_bytes: usize,
    pub cancelled: &'a dyn Fn() -> bool,
    pub text: &'a mut photocraft_text::TextEngine,
    pub regions: &'a mut BTreeMap<LayerId, Rect>,
    pub warnings: &'a mut BTreeMap<String, VectorWarning>,
}

/// False when the chain needs the bounded raster fallback; checked before any bounds work.
pub fn clipping_source_eligible(doc: &Document, base: &Layer, clipped: &[Layer]) -> bool {
    // The row-wise opaque RGB decomposition does not install document ICC
    // conversion. Other document/source modes use the existing renderer's
    // scoped conversion in the bounded chain fallback.
    if doc.mode != photocraft_doc::ColorMode::Rgb
        || matches!(&base.content, LayerContent::Raster(surface) if surface.format().mode != photocraft_doc::ColorMode::Rgb)
    {
        return false;
    }
    // A valid imported fill cache can deliberately differ from the paint
    // definition. Preserve its authoritative source in the local fallback.
    if let LayerContent::Fill(fill) = &base.content
        && base.fill_cache.as_ref().is_some_and(|cache| cache.fill == *fill)
    {
        return false;
    }
    clipped.iter().all(clipping_child_native)
}

impl Clipping<'_> {
    fn warn(&mut self, feature: &str, message: &str) {
        self.warnings.entry(feature.into()).or_insert_with(|| VectorWarning { feature: feature.into(), object: None, message: message.into() });
    }
    fn text_geometry(&mut self, text: &photocraft_doc::TextLayer) -> Result<Option<(photocraft_doc::Color, Path)>> {
        use photocraft_text::render::PathEl;
        if text.antialias == photocraft_doc::text::AntiAlias::None || text.warp.is_some() {
            return Ok(None);
        }
        let layout = self.text.layout(text, self.doc.resolution_dpi);
        if layout.glyphs.len() > 100_000 {
            return Err(Error::Budget("text glyphs"));
        }
        let paints = photocraft_text::render::paint_runs(&layout);
        let Some(color) = paints.first().map(|(color, _, _)| *color) else { return Ok(Some((photocraft_doc::Color::TRANSPARENT, Path::default()))) };
        if paints.iter().any(|(paint, _, _)| *paint != color)
            || layout.styles.iter().any(|style| style.faux_bold)
            || layout.faces.iter().any(|face| face.embolden)
        {
            return Ok(None);
        }
        let mut geometry = kurbo::BezPath::new();
        let mut segments = 0usize;
        let point = |p: [f64; 2]| -> Result<kurbo::Point> {
            if p.iter().any(|value| !value.is_finite() || value.abs() > 1_000_000.) {
                return Err(invalid("text clipping outline exceeds coordinate bounds"));
            }
            Ok((p[0], p[1]).into())
        };
        let limits =
            photocraft_text::render::OutlineVisitLimits { max_glyphs: 100_000, max_segments: 1_000_000, max_scratch_bytes: self.max_bytes.min(16 << 20) };
        photocraft_text::render::visit_outlines(&layout, &text.transform, None, limits, self.cancelled, |event| {
            let photocraft_text::render::OutlineEvent::Element(element) = event else {
                return Ok(());
            };
            // The visitor checks cancellation and the segment budget before
            // delivery; no eager full-layout outline collection is created.
            segments += 1;
            match element {
                PathEl::MoveTo(p) => geometry.move_to(point(p)?),
                PathEl::LineTo(p) => geometry.line_to(point(p)?),
                PathEl::QuadTo(c, p) => geometry.quad_to(point(c)?, point(p)?),
                PathEl::CurveTo(a, b, p) => geometry.curve_to(point(a)?, point(b)?, point(p)?),
                PathEl::Close => geometry.close_path(),
            }
            Ok(())
        })
        .map_err(|error| match error {
            photocraft_text::render::OutlineVisitError::Cancelled => Error::Cancelled,
            photocraft_text::render::OutlineVisitError::Budget(kind) => Error::Budget(kind),
            photocraft_text::render::OutlineVisitError::Visitor(error) => error,
            photocraft_text::render::OutlineVisitError::InvalidOutline => invalid("invalid text clipping outline"),
        })?;
        for decoration in &layout.decorations {
            check_cancel(self.cancelled)?;
            segments += 5;
            if segments > 1_000_000 {
                return Err(Error::Budget("text outline segments"));
            }
            let mut points = [(decoration.x0, decoration.y0), (decoration.x1, decoration.y0), (decoration.x1, decoration.y1), (decoration.x0, decoration.y1)]
                .into_iter()
                .map(|(x, y)| text.transform.apply(photocraft_geom::Point::new(f64::from(x), f64::from(y))));
            if let Some(p) = points.next() {
                geometry.move_to(point([p.x, p.y])?);
            }
            for p in points {
                geometry.line_to(point([p.x, p.y])?);
            }
            geometry.close_path();
        }
        // The adapter keeps all contours in one nonzero-winding component,
        // preserving counters/holes and the same-colored glyph coverage union.
        let path = photocraft_pathops::from_geometry(&photocraft_pathops::geom::PathData::from_bezpath(&geometry));
        Ok(Some((color, path)))
    }
    /// Rewrites an eligible chain as isolated native groups; `region` is the writer's
    /// bounds of the base, recorded for both returned groups.
    pub fn source(&mut self, base: &Layer, clipped: &[Layer], region: Rect) -> Result<Option<Layer>> {
        let (mut body, stroke) = match &base.content {
            LayerContent::Text(text) if base.vector.appearance.is_none() => {
                let Some((color, geometry)) = self.text_geometry(text)? else {
                    return Ok(None);
                };
                if geometry.is_empty() {
                    return Ok(Some(isolated_group("Empty text clipping base", Vec::new())));
                }
                let mut opaque = color;
                opaque.alpha = 1.0;
                let mut body = isolated_group("Text clipping colors", vec![Layer::new("Text base color", LayerContent::Fill(Fill::Solid(opaque)))]);
                body.opacity = color.alpha;
                body.vector_mask = Some(photocraft_doc::VectorMask::new(geometry));
                self.warn(
                    "text clipping outlines",
                    "Text clipping coverage was represented by positioned glyph outlines; the native source text is unchanged.",
                );
                (body, None)
            }

            LayerContent::Shape(shape) if base.vector.appearance.is_none() => {
                let Some(Fill::Solid(color)) = &shape.fill else {
                    if shape.fill.is_none() && shape.stroke.is_some() {
                        // The compositor clips to the fill, then paints the stroke.
                        return Ok(Some(base.clone()));
                    }
                    return Ok(None);
                };
                if shape.stroke.as_ref().is_some_and(|s| !gradient::supported(&s.paint)) {
                    return Ok(None);
                }
                let mut opaque = *color;
                opaque.alpha = 1.0;
                let mut body = isolated_group("Clipping colors", vec![Layer::new("Base color", LayerContent::Fill(Fill::Solid(opaque)))]);
                body.opacity = color.alpha;
                let geometry = normalized_fill(&shape.path, self.rect)?;
                if geometry.is_empty() {
                    // Compound fill operations can cancel while the original
                    // subpaths still have a visible centered stroke.
                    if shape.stroke.is_some() {
                        let mut stroke_only = base.clone();
                        if let LayerContent::Shape(shape) = &mut stroke_only.content {
                            shape.fill = None;
                            shape.cache = None;
                        }
                        return Ok(Some(stroke_only));
                    }
                    return Ok(Some(isolated_group("Empty clipping base", Vec::new())));
                }
                body.vector_mask = Some(photocraft_doc::VectorMask::new(geometry));
                let stroke = shape.stroke.as_ref().map(|stroke| {
                    Layer::new(
                        "Base stroke",
                        LayerContent::Shape(ShapeLayer { path: shape.path.clone(), fill: None, stroke: Some(stroke.clone()), ..Default::default() }),
                    )
                });
                (body, stroke)
            }
            LayerContent::Fill(Fill::Solid(color)) => {
                let mut opaque = *color;
                opaque.alpha = 1.0;
                let mut body = isolated_group("Clipping colors", vec![Layer::new("Base color", LayerContent::Fill(Fill::Solid(opaque)))]);
                body.opacity = color.alpha;
                (body, None)
            }
            LayerContent::Raster(surface) => {
                let rect = photocraft_compose::composite_bounds(base, self.doc.bounds()).unwrap_or(self.doc.bounds()).intersect(&self.rect);
                if rect.is_empty() {
                    return Ok(Some(isolated_group("Empty clipping base", Vec::new())));
                }
                check_pixels(rect, self.max_pixels)?;
                let mut color = photocraft_doc::Surface::new(photocraft_doc::PixelFormat::RGBA32F);
                let mut alpha = photocraft_doc::Surface::new(photocraft_doc::PixelFormat::new(
                    photocraft_doc::ColorMode::Grayscale,
                    photocraft_doc::SampleType::F32,
                    false,
                ));
                for y in rect.y0..rect.y1 {
                    check_cancel(self.cancelled)?;
                    let row = Rect::new(rect.x0, y, rect.x1, y + 1);
                    let pixels = photocraft_compose::surface_to_buffer(surface, row);
                    let colors: Vec<f32> = pixels.px.iter().flat_map(|p| [p[0], p[1], p[2], 1.0]).collect();
                    let alphas: Vec<f32> = pixels.px.iter().map(|p| p[3]).collect();
                    if colors.iter().chain(&alphas).any(|v| !v.is_finite()) {
                        return Err(invalid("nonfinite clipping base sample"));
                    }
                    color.write_region(row, &colors);
                    alpha.write_region(row, &alphas);
                }
                let mut body = isolated_group("Clipping colors", vec![Layer::new("Opaque raster base", LayerContent::Raster(color))]);
                let mut mask = photocraft_doc::LayerMask::hide_all();
                mask.surface = alpha;
                body.mask = Some(mask);
                (body, None)
            }
            _ => return Ok(None),
        };
        if let LayerContent::Group(group) = &mut body.content {
            for layer in clipped.iter().filter(|l| l.visible && l.opacity > 0.0) {
                check_cancel(self.cancelled)?;
                let mut layer = layer.clone();
                layer.clipped = false;
                // composite_atop renders ordinary groups as isolated content.
                if let LayerContent::Group(group) = &mut layer.content {
                    group.isolation = photocraft_doc::GroupIsolation::Isolated;
                }
                group.children.push(layer);
            }
        }
        if !region.is_empty() {
            self.regions.insert(body.id, region);
        }
        let mut children = vec![body];
        if let Some(stroke) = stroke {
            children.push(stroke);
        }
        let mut outer = isolated_group(&base.name, children);
        outer.blend = base.blend;
        outer.opacity = base.opacity * base.fill_opacity;
        outer.mask = base.mask.clone();
        outer.vector_mask = base.vector_mask.clone();
        outer.vector.clip = base.vector.clip.clone();
        if !region.is_empty() {
            self.regions.insert(outer.id, region);
        }
        Ok(Some(outer))
    }
}
