//! Normalized tree to ordinary layers, with bounded native geometry/text caches.
//! Adapted from VectorCraft crates/svg/src/import.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
use super::{ImportError, Result, cancelled, geometry, paint, text::Slots, warn};
use photocraft_doc::{
    Affine, BlendMode, DesignUnits, Layer, LayerContent, LayerMask, Path, PixelFormat, Rect, ShapeLayer, Size, Surface, VectorMask,
    vector_design::exchange::{ImportOptions, ImportedScene, VectorWarning},
};
use photocraft_geom::Point;
use std::{io::Cursor, sync::atomic::AtomicBool};

pub(super) fn convert(
    tree: &usvg::Tree,
    xml: &roxmltree::Document<'_>,
    slots: Slots,
    options: &ImportOptions,
    fonts: Option<&mut photocraft_text::TextEngine>,
    cancel: &AtomicBool,
    warnings: Vec<VectorWarning>,
) -> Result<ImportedScene> {
    let w = f64::from(tree.size().width()).ceil();
    let h = f64::from(tree.size().height()).ceil();
    if w > geometry::MAX_COORD || h > geometry::MAX_COORD || w * h > options.limits.pixels as f64 {
        return Err(ImportError::Limit("viewport pixel"));
    }
    let size = Size { width: w as u32, height: h as u32 };
    let units = match xml.root_element().attribute("width").unwrap_or("").trim().chars().rev().take(2).collect::<String>().as_str() {
        "mm" | "mc" => DesignUnits::Millimeters,
        "tp" | "cp" => DesignUnits::Points,
        "ni" => DesignUnits::Inches,
        _ => DesignUnits::Pixels,
    };
    let mut state = State { options, canvas: Rect::from_size(size), fonts, bundled: None, cancel, slots, warnings, objects: 0, segments: 0, bytes: 0, xml };
    let root = tree.root();
    let m = geometry::affine(root.transform());
    let mut layers = state.children(root, m, 0)?;
    if root.opacity().get() != 1.0
        || root.blend_mode() != usvg::BlendMode::Normal
        || root.should_isolate()
        || root.clip_path().is_some()
        || root.mask().is_some()
    {
        let mut layer = Layer::group(state.name(root.id(), "SVG"), layers);
        layer.opacity = root.opacity().get();
        layer.blend = blend(root.blend_mode());
        if root.blend_mode() == usvg::BlendMode::Normal && !root.should_isolate() {
            layer.blend = BlendMode::PassThrough;
        }
        if let Some(clip) = root.clip_path() {
            let mut path = state.clip(clip, m, 1)?;
            // Native empty vector masks reveal all; an empty SVG clip hides all.
            if path.is_empty() {
                path.inverted = true;
            }
            layer.vector.clip = Some(path.clone());
            layer.vector_mask = Some(VectorMask::new(path));
        }
        if let Some(mask) = root.mask() {
            layer.mask = Some(state.mask(mask, m, 1)?);
            warn(&mut state.warnings, "mask", root.id(), "The SVG mask was converted to a pixel mask; its original vector objects are not retained.");
        }
        layers = vec![layer];
    }
    for warning in &mut state.warnings {
        if let Some(id) = &warning.object
            && let Some(label) = xml
                .descendants()
                .find(|n| n.attribute("id") == Some(id.as_str()))
                .and_then(|n| n.attribute(("http://www.inkscape.org/namespaces/inkscape", "label")).or_else(|| n.attribute("data-name")))
        {
            warning.object = Some(label.into());
        }
    }
    Ok(ImportedScene { size, resolution_dpi: options.dpi, units, layers, symbols: vec![], warnings: state.warnings })
}
struct State<'a, 'b, 'c> {
    options: &'a ImportOptions,
    canvas: Rect,
    fonts: Option<&'b mut photocraft_text::TextEngine>,
    bundled: Option<photocraft_text::TextEngine>,
    cancel: &'a AtomicBool,
    slots: Slots,
    warnings: Vec<VectorWarning>,
    objects: usize,
    segments: usize,
    bytes: u64,
    xml: &'c roxmltree::Document<'c>,
}
impl State<'_, '_, '_> {
    fn charge(&mut self, rect: Rect) -> Result<()> {
        let pixels = rect.size().area();
        if pixels > self.options.limits.pixels {
            return Err(ImportError::Limit("cache pixel"));
        }
        // Surfaces allocate complete tiles, even for a one-pixel-high image. Charge
        // that footprint so a very wide/thin viewport cannot evade the byte limit.
        let tile = i64::from(photocraft_geom::TILE_SIZE);
        let tiles = if rect.is_empty() {
            0
        } else {
            let columns = i64::from(rect.x1 - 1).div_euclid(tile) - i64::from(rect.x0).div_euclid(tile) + 1;
            let rows = i64::from(rect.y1 - 1).div_euclid(tile) - i64::from(rect.y0).div_euclid(tile) + 1;
            (columns as u64).saturating_mul(rows as u64)
        };
        self.bytes = self.bytes.saturating_add(tiles.saturating_mul(tile as u64).saturating_mul(tile as u64).saturating_mul(16));
        if self.bytes > self.options.limits.decompressed_bytes {
            return Err(ImportError::Limit("cache byte"));
        }
        cancelled(self.cancel)
    }
    fn path(&mut self, data: &usvg::tiny_skia_path::Path, transform: Affine, rule: usvg::FillRule) -> Result<Path> {
        self.segments = self.segments.saturating_add(data.segments().count());
        if self.segments > self.options.limits.segments {
            return Err(ImportError::Limit("expanded segment"));
        }
        geometry::path(data, transform, rule)
    }
    fn name(&self, id: &str, default: &str) -> String {
        self.xml
            .descendants()
            .find(|n| !id.is_empty() && n.attribute("id") == Some(id))
            .and_then(|n| n.attribute(("http://www.inkscape.org/namespaces/inkscape", "label")).or_else(|| n.attribute("data-name")))
            .unwrap_or(if id.is_empty() { default } else { id })
            .to_string()
    }
    fn children(&mut self, group: &usvg::Group, transform: Affine, depth: usize) -> Result<Vec<Layer>> {
        if depth >= self.options.limits.depth.min(photocraft_doc::MAX_GROUP_DEPTH).saturating_sub(1) {
            return Err(ImportError::Limit("normalized group depth"));
        }
        let mut out = Vec::new();
        for n in group.children() {
            cancelled(self.cancel)?;
            self.objects += 1;
            if self.objects > self.options.limits.objects {
                return Err(ImportError::Limit("expanded object"));
            }
            match n {
                usvg::Node::Group(g) => {
                    let m = geometry::bounded(transform.mul(&geometry::affine(g.transform())))?;
                    let mut children = self.children(g, m, depth + 1)?;
                    // usvg wraps <image> placement in a group and strips the leaf's ID.
                    // Carry its source label to the editable image itself as well.
                    if children.len() == 1 && matches!(children[0].content, LayerContent::Raster(_)) && !g.id().is_empty() {
                        children[0].name = self.name(g.id(), "Image");
                    }
                    let mut l = Layer::group(self.name(g.id(), "Group"), children);
                    l.opacity = g.opacity().get();
                    l.blend = blend(g.blend_mode());
                    if g.blend_mode() == usvg::BlendMode::Normal && !g.should_isolate() {
                        l.blend = BlendMode::PassThrough;
                    }
                    if let Some(clip) = g.clip_path() {
                        let mut p = self.clip(clip, m, depth + 1)?;
                        if p.is_empty() {
                            p.inverted = true;
                        }
                        l.vector.clip = Some(p.clone());
                        l.vector_mask = Some(VectorMask::new(p));
                    }
                    if let Some(mask) = g.mask() {
                        l.mask = Some(self.mask(mask, m, depth + 1)?);
                        warn(&mut self.warnings, "mask", g.id(), "The SVG mask was converted to a pixel mask; its original vector objects are not retained.");
                    }
                    out.push(l);
                }
                usvg::Node::Path(p) => {
                    if !p.is_visible() {
                        continue;
                    }
                    if let Some(i) = self.slots.find(p) {
                        let mut t = self.slots.texts[i].clone();
                        let name = self.name(&self.slots.names[i], "Text");
                        t.transform = geometry::bounded(transform.mul(&t.transform))?;
                        if self.fonts.is_none() && self.bundled.is_none() {
                            self.bundled = Some(photocraft_text::TextEngine::new());
                        }
                        let fonts = self.fonts.as_deref_mut().or(self.bundled.as_mut()).ok_or_else(|| ImportError::Invalid("missing text engine".into()))?;
                        for run in t.char_runs() {
                            if !fonts.fonts.has_family(&run.style.font_family) {
                                warn(&mut self.warnings, "font-substitution", &name, "An unavailable font was substituted. Text remains editable.");
                            }
                        }
                        let layout = fonts.layout(&t, self.options.dpi);
                        // Check transformed logical and glyph ink extents before pc-text allocates.
                        if let Some(b) = layout.bounds() {
                            rect_of(
                                &[
                                    Point::new(b[0].into(), b[1].into()),
                                    Point::new(b[2].into(), b[1].into()),
                                    Point::new(b[2].into(), b[3].into()),
                                    Point::new(b[0].into(), b[3].into()),
                                ]
                                .map(|p| t.transform.apply(p)),
                            )?;
                        }
                        let r = photocraft_text::render::ink_rect(&layout, &t.transform);
                        self.charge(r)?;
                        let fonts = self.fonts.as_deref_mut().or(self.bundled.as_mut()).ok_or_else(|| ImportError::Invalid("missing text engine".into()))?;
                        fonts.render_layer(&mut t, self.options.dpi, PixelFormat::RGBA32F);
                        out.push(Layer::new(name, LayerContent::Text(t)));
                    } else {
                        out.push(self.shape(p, transform)?);
                    }
                }
                usvg::Node::Image(i) => {
                    if i.is_visible()
                        && let Some(l) = self.image(i, transform)?
                    {
                        out.push(l);
                    }
                }
                usvg::Node::Text(_) => return Err(ImportError::Invalid("unexpected usvg text feature; use XML placeholders".into())),
            }
        }
        Ok(out)
    }
    fn shape(&mut self, p: &usvg::Path, m: Affine) -> Result<Layer> {
        let rule = p.fill().map_or(usvg::FillRule::NonZero, |f| f.rule());
        let path = self.path(p.data(), m, rule)?;
        let b = path.control_bounds().unwrap_or((0.0, 0.0, 0.0, 0.0));
        let fill = p.fill().map(|f| paint::mirror(f.paint(), m, f.opacity().get(), b, &mut self.warnings, p.id()));
        let stroke = p.stroke().map(|s| paint::stroke(s, m, b, &mut self.warnings, p.id()));
        // Stroke in source space before transforming, preserving skew/nonuniform scaling.
        let stroke_path = if let Some(s) = p.stroke() {
            let local = self.path(p.data(), Affine::IDENTITY, usvg::FillRule::NonZero)?;
            let style = photocraft_vector::stroke_style(&paint::stroke(s, Affine::IDENTITY, b, &mut Vec::new(), p.id()));
            // Keep all native stroke polygons in one nonzero component, as pc-vector's
            // stroker does. A boolean cleanup is unnecessary for coverage and can lose
            // the small contours of a hairline or dashed stroke.
            let tolerance = photocraft_vector::DEFAULT_TOLERANCE;
            let lines = photocraft_vector::flatten_path(&local, (tolerance * 4.0).min(style.width.max(0.01) * 0.1).max(1e-3));
            let length: f64 = lines
                .iter()
                .map(|line| {
                    line.pts.windows(2).map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1)).sum::<f64>()
                        + if line.closed { line.pts.first().zip(line.pts.last()).map_or(0.0, |(a, b)| (a.0 - b.0).hypot(a.1 - b.1)) } else { 0.0 }
                })
                .sum();
            let period: f64 = style.dashes.iter().sum();
            if period > 0.0 && length / period * style.dashes.len() as f64 > self.options.limits.segments as f64 / 16.0 {
                return Err(ImportError::Limit("stroke dash expansion"));
            }
            let polygons = photocraft_vector::stroke_polygons(&lines, &style, tolerance);
            self.segments = self.segments.saturating_add(polygons.iter().map(Vec::len).sum::<usize>());
            if self.segments > self.options.limits.segments {
                return Err(ImportError::Limit("stroke segment"));
            }
            Some(
                Path::new(
                    polygons
                        .iter()
                        .enumerate()
                        .map(|(i, polygon)| {
                            photocraft_doc::Subpath::polygon(polygon).with_op(if i == 0 {
                                photocraft_doc::PathOp::Combine
                            } else {
                                photocraft_doc::PathOp::Join
                            })
                        })
                        .collect(),
                )
                .transform(&m),
            )
        } else {
            None
        };
        let mut bounds = path.control_bounds();
        if let Some(sp) = &stroke_path
            && let Some(b) = sp.control_bounds()
        {
            bounds = Some(bounds.map_or(b, |a| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))));
        }
        let b = bounds.unwrap_or((0.0, 0.0, 0.0, 0.0));
        let rect = rect_of(&[Point::new(b.0, b.1), Point::new(b.2, b.3)])?.intersect(&self.canvas);
        self.charge(rect)?;
        let mut cache = Surface::new(PixelFormat::RGBA32F);
        for band in bands(rect) {
            cancelled(self.cancel)?;
            let fc = if p.fill().is_some() { photocraft_vector::path_coverage(&path, band) } else { vec![0.0; band.size().area() as usize] };
            let sc = stroke_path.as_ref().map(|sp| photocraft_vector::path_coverage(sp, band));
            let mut pixels = Vec::with_capacity(fc.len() * 4);
            for (i, &coverage) in fc.iter().enumerate() {
                let point =
                    Point::new(f64::from(band.x0) + (i % band.width() as usize) as f64 + 0.5, f64::from(band.y0) + (i / band.width() as usize) as f64 + 0.5);
                let mut f = p.fill().map_or([0.0; 4], |f| {
                    let mut c = paint::sample(f.paint(), m, point);
                    c[3] *= f.opacity().get() * coverage;
                    c
                });
                let mut s = p.stroke().map_or([0.0; 4], |s| {
                    let mut c = paint::sample(s.paint(), m, point);
                    c[3] *= s.opacity().get() * sc.as_ref().map_or(0.0, |v| v[i]);
                    c
                });
                if p.paint_order() == usvg::PaintOrder::StrokeAndFill {
                    std::mem::swap(&mut f, &mut s);
                }
                pixels.extend(over(s, f));
            }
            cache.write_region(band, &pixels);
        }
        cache.prune();
        if p.paint_order() == usvg::PaintOrder::StrokeAndFill {
            warn(&mut self.warnings, "paint-order", p.id(), "The stroke draws beneath the fill. Editing this shape may change that order.");
        }
        Ok(Layer::new(self.name(p.id(), "Path"), LayerContent::Shape(ShapeLayer { path, fill, stroke, cache: Some(cache), ..Default::default() })))
    }
    fn clip(&mut self, c: &usvg::ClipPath, m: Affine, depth: usize) -> Result<Path> {
        if depth >= self.options.limits.depth {
            return Err(ImportError::Limit("clip depth"));
        }
        let original = m;
        let m = m.mul(&geometry::affine(c.transform()));
        let mut paths = Vec::new();
        self.clip_children(c.root(), m, depth, &mut paths)?;
        let mut result = Path::default();
        for path in paths {
            result = if result.is_empty() {
                path
            } else {
                photocraft_pathops::boolean(&result, &path, photocraft_pathops::BoolOp::Union).map_err(|e| ImportError::Invalid(e.to_string()))?
            };
        }
        if let Some(next) = c.clip_path() {
            let next = self.clip(next, original, depth + 1)?;
            result = photocraft_pathops::boolean(&result, &next, photocraft_pathops::BoolOp::Intersect).map_err(|e| ImportError::Invalid(e.to_string()))?;
        }
        Ok(result)
    }
    fn clip_children(&mut self, g: &usvg::Group, m: Affine, depth: usize, out: &mut Vec<Path>) -> Result<()> {
        if depth >= self.options.limits.depth {
            return Err(ImportError::Limit("clip depth"));
        }
        for n in g.children() {
            cancelled(self.cancel)?;
            match n {
                usvg::Node::Path(p) if p.is_visible() => out.push(self.path(p.data(), m, p.fill().map_or(usvg::FillRule::NonZero, |f| f.rule()))?),
                usvg::Node::Group(g) => {
                    let mut nested = Vec::new();
                    let m = m.mul(&geometry::affine(g.transform()));
                    self.clip_children(g, m, depth + 1, &mut nested)?;
                    if let Some(cp) = g.clip_path() {
                        let clip = self.clip(cp, m, depth + 1)?;
                        for path in nested {
                            out.push(
                                photocraft_pathops::boolean(&path, &clip, photocraft_pathops::BoolOp::Intersect)
                                    .map_err(|e| ImportError::Invalid(e.to_string()))?,
                            );
                        }
                    } else {
                        out.extend(nested);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn mask(&mut self, mask: &usvg::Mask, m: Affine, depth: usize) -> Result<LayerMask> {
        let children = self.children(mask.root(), m.mul(&geometry::affine(mask.root().transform())), depth)?;
        let l = Layer::group("Mask", children);
        let mr = mask.rect();
        let points = [
            Point::new(mr.x().into(), mr.y().into()),
            Point::new(mr.right().into(), mr.y().into()),
            Point::new(mr.right().into(), mr.bottom().into()),
            Point::new(mr.x().into(), mr.bottom().into()),
        ]
        .map(|p| m.apply(p));
        let rect = rect_of(&points)?.intersect(&self.canvas);
        self.charge(rect)?;
        let next = mask.mask().map(|next| self.mask(next, m, depth + 1)).transpose()?;
        let region = Path::new(vec![photocraft_doc::Subpath::polygon(&points.map(|p| (p.x, p.y)))]);
        let mut result = LayerMask::hide_all();
        for band in bands(rect) {
            cancelled(self.cancel)?;
            let frame = photocraft_compose::render_layer(&l, band);
            let region_coverage = photocraft_vector::path_coverage(&region, band);
            let data: Vec<_> = frame
                .px
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let mut v = p[3] * region_coverage[i];
                    if mask.kind() == usvg::MaskType::Luminance {
                        v *= 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
                    }
                    if let Some(next) = &next {
                        v *= next.value(band.x0 + (i % band.width() as usize) as i32, band.y0 + (i / band.width() as usize) as i32);
                    }
                    v
                })
                .collect();
            result.surface.write_region(band, &data);
        }
        Ok(result)
    }
    fn image(&mut self, i: &usvg::Image, m: Affine) -> Result<Option<Layer>> {
        let data = match i.kind() {
            usvg::ImageKind::PNG(d) | usvg::ImageKind::JPEG(d) | usvg::ImageKind::WEBP(d) => d,
            _ => {
                warn(&mut self.warnings, "embedded-format", i.id(), "Unsupported embedded image was omitted.");
                return Ok(None);
            }
        };
        let reader = image::ImageReader::new(Cursor::new(data.as_slice())).with_guessed_format().map_err(|e| ImportError::Invalid(e.to_string()))?;
        let (w, h) = reader.into_dimensions().map_err(|e| ImportError::Invalid(e.to_string()))?;
        if u64::from(w) * u64::from(h) > self.options.limits.pixels {
            return Err(ImportError::Limit("decoded image pixel"));
        }
        self.charge(Rect::from_xywh(0, 0, w, h))?;
        let image = image::load_from_memory(data).map_err(|e| ImportError::Invalid(e.to_string()))?.to_rgba8();
        let m = m.mul(&Affine { m: [f64::from(i.size().width()) / f64::from(w), 0.0, 0.0, f64::from(i.size().height()) / f64::from(h), 0.0, 0.0] });
        geometry::bounded(m)?;
        let Some(inv) = m.inverse() else { return Ok(None) };
        let points = [Point::new(0.0, 0.0), Point::new(w.into(), 0.0), Point::new(w.into(), h.into()), Point::new(0.0, h.into())].map(|p| m.apply(p));
        let rect = rect_of(&points)?.intersect(&self.canvas);
        self.charge(rect)?;
        let mut surface = Surface::new(PixelFormat::RGBA32F);
        for band in bands(rect) {
            cancelled(self.cancel)?;
            for y in band.y0..band.y1 {
                for x in band.x0..band.x1 {
                    let p = inv.apply(Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5));
                    if p.x >= 0.0 && p.y >= 0.0 && p.x < f64::from(w) && p.y < f64::from(h) {
                        let rgba = image.get_pixel(p.x as u32, p.y as u32).0.map(|v| f32::from(v) / 255.0);
                        surface.write_pixel(x, y, &rgba);
                    }
                }
            }
        }
        warn(&mut self.warnings, "image-transform", i.id(), "Embedded image placement was converted to raster pixels; smooth SVG resampling may differ.");
        Ok(Some(Layer::new(self.name(i.id(), "Image"), LayerContent::Raster(surface))))
    }
}
fn rect_of(points: &[Point]) -> Result<Rect> {
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > geometry::MAX_COORD) {
        return Err(ImportError::Limit("render coordinate"));
    }
    let (x0, y0, x1, y1) = points
        .iter()
        .fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |b, p| (b.0.min(p.x), b.1.min(p.y), b.2.max(p.x), b.3.max(p.y)));
    Ok(Rect { x0: x0.floor() as i32, y0: y0.floor() as i32, x1: x1.ceil() as i32, y1: y1.ceil() as i32 })
}
fn bands(rect: Rect) -> impl Iterator<Item = Rect> {
    (rect.y0..rect.y1).step_by(32).map(move |y| Rect { x0: rect.x0, y0: y, x1: rect.x1, y1: (y + 32).min(rect.y1) })
}
fn over(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let alpha = a[3] + b[3] * (1.0 - a[3]);
    if alpha <= 0.0 {
        return [0.0; 4];
    }
    [
        (a[0] * a[3] + b[0] * b[3] * (1.0 - a[3])) / alpha,
        (a[1] * a[3] + b[1] * b[3] * (1.0 - a[3])) / alpha,
        (a[2] * a[3] + b[2] * b[3] * (1.0 - a[3])) / alpha,
        alpha,
    ]
}
fn blend(b: usvg::BlendMode) -> BlendMode {
    match b {
        usvg::BlendMode::Normal => BlendMode::Normal,
        usvg::BlendMode::Multiply => BlendMode::Multiply,
        usvg::BlendMode::Screen => BlendMode::Screen,
        usvg::BlendMode::Overlay => BlendMode::Overlay,
        usvg::BlendMode::Darken => BlendMode::Darken,
        usvg::BlendMode::Lighten => BlendMode::Lighten,
        usvg::BlendMode::ColorDodge => BlendMode::ColorDodge,
        usvg::BlendMode::ColorBurn => BlendMode::ColorBurn,
        usvg::BlendMode::HardLight => BlendMode::HardLight,
        usvg::BlendMode::SoftLight => BlendMode::SoftLight,
        usvg::BlendMode::Difference => BlendMode::Difference,
        usvg::BlendMode::Exclusion => BlendMode::Exclusion,
        usvg::BlendMode::Hue => BlendMode::Hue,
        usvg::BlendMode::Saturation => BlendMode::Saturation,
        usvg::BlendMode::Color => BlendMode::Color,
        usvg::BlendMode::Luminosity => BlendMode::Luminosity,
    }
}
