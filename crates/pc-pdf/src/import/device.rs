//! hayro Device → editable paths, glyph curves, native clips/groups and raster image layers.
//! Callback/group/mask mapping adapted from VectorCraft pdf/import.rs/import_mask.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092. Copyright (c) 2026 ArtCraft
//! Team and the VectorCraft contributors. MIT OR Apache-2.0. pc-vector remains the only path rasterizer.
use super::{ImportError, Result, cancelled, geometry, paint, preflight::groups, transfer, warn};
use hayro_interpret::{
    CacheKey, ClipPath, Device, DrawMode, DrawProps, Image, ImageData, ImageDrawProps, MaskType, Paint, SoftMask,
    font::{Glyph, GlyphRun},
    pattern::Pattern,
};
use kurbo::{Affine, BezPath, Point};
use photocraft_doc::{
    BlendMode, Fill, GroupIsolation, Layer, LayerContent, LayerMask, Path, PixelFormat, Rect, ShapeLayer, Surface, VectorMask,
    vector_design::exchange::{ImportOptions, VectorWarning},
};
use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::atomic::AtomicBool,
};
type MaskKey = (u128, bool, [u32; 4], Option<[u8; 256]>);
/// Glyph slice address, CTM and per-glyph text transforms identify one hayro glyph run.
type GlyphRunKey = (usize, [u64; 6], Vec<[u64; 6]>);
#[derive(Default)]
struct Budget {
    objects: usize,
    segments: usize,
    bytes: u64,
    callbacks: usize,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Root,
    Clip,
    Transparency,
}
struct Frame {
    kind: FrameKind,
    marked: bool,
    isolation: GroupIsolation,
    children: Vec<Layer>,
    clip: Option<Path>,
    opacity: f32,
    blend: BlendMode,
    mask: Option<LayerMask>,
}
impl Frame {
    fn root() -> Self {
        Self {
            kind: FrameKind::Root,
            marked: false,
            isolation: GroupIsolation::Auto,
            children: vec![],
            clip: None,
            opacity: 1.0,
            blend: BlendMode::Normal,
            mask: None,
        }
    }
}
/// Layers a Fill glyph run appended to the current frame. A Stroke run of the same glyphs that
/// follows immediately (text render modes 2 and 6) is interleaved glyph by glyph, matching
/// per-glyph fill-then-stroke painting.
struct FillRun {
    glyphs: GlyphRunKey,
    frames: usize,
    start: usize,
    counts: Vec<usize>,
}
pub(super) struct Builder<'a> {
    options: &'a ImportOptions,
    cancel: &'a AtomicBool,
    pdf: &'a hayro_syntax::Pdf,
    transfers: Rc<OnceCell<transfer::Transfers>>,
    canvas: Rect,
    budget: Rc<RefCell<Budget>>,
    frames: Vec<Frame>,
    groups: Rc<groups::Table>,
    markers: Vec<Option<usize>>,
    pattern_cell: bool,
    blend: BlendMode,
    mask: Option<LayerMask>,
    masks: HashMap<MaskKey, LayerMask>,
    fill_run: Option<FillRun>,
    nested: usize,
    error: Option<ImportError>,
    pub warnings: Vec<VectorWarning>,
}
impl<'a> Builder<'a> {
    pub fn new(options: &'a ImportOptions, cancel: &'a AtomicBool, pdf: &'a hayro_syntax::Pdf, canvas: Rect, groups: Rc<groups::Table>) -> Self {
        Self {
            options,
            cancel,
            pdf,
            transfers: Rc::default(),
            canvas,
            budget: Rc::default(),
            frames: vec![Frame::root()],
            groups,
            markers: vec![],
            pattern_cell: false,
            blend: BlendMode::Normal,
            mask: None,
            masks: HashMap::new(),
            fill_run: None,
            nested: 0,
            error: None,
            warnings: vec![],
        }
    }
    fn child(&self, canvas: Rect) -> Result<Self> {
        if self.nested + self.frames.len() >= self.options.limits.depth.min(photocraft_doc::MAX_GROUP_DEPTH).saturating_sub(2) {
            return Err(ImportError::Limit("nested mask/pattern/glyph"));
        }
        let mut b = Self::new(self.options, self.cancel, self.pdf, canvas, self.groups.clone());
        b.budget = self.budget.clone();
        b.transfers = self.transfers.clone();
        b.nested = self.nested + 1;
        b.pattern_cell = self.pattern_cell;
        Ok(b)
    }
    pub fn begin_page(&mut self) {
        self.frames = vec![Frame::root()];
        self.markers.clear();
        self.fill_run = None;
        self.mask = None;
        self.blend = BlendMode::Normal;
    }
    pub fn end_page(&mut self) -> Result<Vec<Layer>> {
        if let Some(e) = self.error.take() {
            return Err(e);
        }
        cancelled(self.cancel)?;
        if !self.markers.is_empty() {
            return Err(ImportError::Invalid("unbalanced PDF markers".into()));
        }
        while self.frames.len() > 1 {
            self.pop()?;
        }
        Ok(self.frames.pop().map(|f| f.children).unwrap_or_default())
    }
    fn run(&mut self, f: impl FnOnce(&mut Self) -> Result<()>) {
        self.fill_run = None;
        if self.error.is_none()
            && let Err(e) = cancelled(self.cancel).and_then(|()| f(self))
        {
            self.error = Some(e);
        }
    }
    fn segments(&mut self, n: usize) -> Result<()> {
        let mut b = self.budget.borrow_mut();
        b.segments = b.segments.saturating_add(n);
        if b.segments > self.options.limits.segments {
            return Err(ImportError::Limit("expanded segment"));
        }
        cancelled(self.cancel)
    }
    fn charge(&mut self, rect: Rect) -> Result<()> {
        if rect.size().area() > self.options.limits.pixels {
            return Err(ImportError::Limit("cache pixel"));
        }
        let tile = i64::from(photocraft_geom::TILE_SIZE);
        let tiles = if rect.is_empty() {
            0
        } else {
            (i64::from(rect.x1 - 1).div_euclid(tile) - i64::from(rect.x0).div_euclid(tile) + 1) as u64
                * (i64::from(rect.y1 - 1).div_euclid(tile) - i64::from(rect.y0).div_euclid(tile) + 1) as u64
        };
        let mut b = self.budget.borrow_mut();
        b.bytes = b.bytes.saturating_add(tiles.saturating_mul(tile as u64 * tile as u64 * 16));
        if b.bytes > self.options.limits.decompressed_bytes {
            return Err(ImportError::Limit("cache byte"));
        }
        cancelled(self.cancel)
    }
    fn path(&mut self, p: &BezPath, m: Affine, rule: hayro_interpret::FillRule) -> Result<Path> {
        self.segments(p.elements().len())?;
        let path = geometry::native(p, m, rule)?;
        self.flatten_budget(&path)?;
        Ok(path)
    }
    fn flatten_budget(&mut self, path: &Path) -> Result<()> {
        let mut cost = 0usize;
        for s in &path.subpaths {
            for p in s.segments() {
                let [a, b, c, d] = p;
                let second = ((a.x - 2.0 * b.x + c.x).hypot(a.y - 2.0 * b.y + c.y)).max((b.x - 2.0 * c.x + d.x).hypot(b.y - 2.0 * c.y + d.y));
                let size = (b.x - a.x).hypot(b.y - a.y) + (c.x - b.x).hypot(c.y - b.y) + (d.x - c.x).hypot(d.y - c.y);
                let tolerance = 0.01_f64.min(size * 1e-3).max(1e-4);
                cost = cost.saturating_add(((0.75 * second / tolerance).sqrt().ceil() as usize).clamp(1, 1 << 16));
            }
        }
        self.segments(cost)
    }
    fn push(&mut self, mut layer: Layer, masked: bool) -> Result<()> {
        {
            let mut b = self.budget.borrow_mut();
            b.objects += 1;
            if b.objects > self.options.limits.objects {
                return Err(ImportError::Limit("expanded object"));
            }
        }
        layer.blend = self.blend;
        if masked {
            layer.mask = self.mask.clone();
        }
        self.frames.last_mut().ok_or_else(|| ImportError::Invalid("missing drawing frame".into()))?.children.push(layer);
        Ok(())
    }
    fn frame(&mut self, frame: Frame) -> Result<()> {
        if self.frames.len() + self.nested >= self.options.limits.depth.min(photocraft_doc::MAX_GROUP_DEPTH).saturating_sub(1) {
            return Err(ImportError::Limit("group depth"));
        }
        self.frames.push(frame);
        Ok(())
    }
    fn pop(&mut self) -> Result<()> {
        if self.frames.len() <= 1 {
            return Err(ImportError::Invalid("unbalanced drawing stack".into()));
        }
        if self.markers.iter().flatten().any(|&index| index + 1 >= self.frames.len()) {
            return Err(ImportError::Invalid("PDF marker crossed drawing frame".into()));
        }
        let Some(frame) = self.frames.pop() else {
            return Err(ImportError::Invalid("drawing frame".into()));
        };
        let mut group = Layer::group(if frame.clip.is_some() { "PDF clip" } else { "PDF transparency" }, frame.children);
        if let Some(mut p) = frame.clip {
            if p.is_empty() {
                p.inverted = true;
            }
            group.vector.clip = Some(p.clone());
            group.vector_mask = Some(VectorMask::new(p));
        }
        if let LayerContent::Group(g) = &mut group.content {
            g.isolation = frame.isolation;
        }
        group.opacity = frame.opacity;
        group.blend = frame.blend;
        group.mask = frame.mask;
        {
            let mut b = self.budget.borrow_mut();
            b.objects += 1;
            if b.objects > self.options.limits.objects {
                return Err(ImportError::Limit("expanded object"));
            }
        }
        self.frames.last_mut().ok_or_else(|| ImportError::Invalid("missing parent frame".into()))?.children.push(group);
        Ok(())
    }
    fn soft_mask(&mut self, mask: &SoftMask<'_>) -> Result<LayerMask> {
        // hayro hashes only source ID/transform. Mode, background and transfer also affect
        // the result. The transfer's 8-bit table selects the exact function evaluated below.
        let background = mask.background_color().to_rgba().components();
        let transfer = mask.transfer_function().map(|f| {
            let mut table: [u8; 256] = std::array::from_fn(|i| i as u8);
            f.apply_to(&mut table);
            table
        });
        let key = (mask.cache_key(), matches!(mask.mask_type(), MaskType::Alpha), background.map(f32::to_bits), transfer);
        if let Some(m) = self.masks.get(&key) {
            return Ok(m.clone());
        }
        let exact = match &transfer {
            Some(table) => {
                if self.transfers.get().is_none() {
                    let _ = self.transfers.set(transfer::Transfers::scan(self.pdf, self.cancel)?);
                }
                self.transfers.get().and_then(|t| t.find(mask, table)).cloned()
            }
            None => None,
        };
        let mut b = self.child(self.canvas)?;
        mask.interpret(&mut b);
        let children = b.end_page()?;
        self.warnings.extend(b.warnings);
        let mut group = Layer::group("PDF mask source", children);
        if matches!(mask.mask_type(), MaskType::Luminosity) {
            {
                let mut budget = self.budget.borrow_mut();
                budget.objects = budget.objects.saturating_add(2);
                if budget.objects > self.options.limits.objects {
                    return Err(ImportError::Limit("mask background object"));
                }
            }
            // BC is an initial backdrop: backdrop-dependent child blends must
            // see it during interpretation of the compositing tree.
            let background =
                Layer::new("PDF mask background", LayerContent::Fill(Fill::Solid(photocraft_doc::Color::rgb(background[0], background[1], background[2]))));
            group = Layer::group("PDF luminosity mask", vec![background, group]);
        }
        self.charge(self.canvas)?;
        let mut out = LayerMask::hide_all();
        out.surface = Surface::new(PixelFormat::new(photocraft_doc::ColorMode::Grayscale, photocraft_doc::SampleType::F32, false));
        for band in geometry::bands(self.canvas) {
            cancelled(self.cancel)?;
            let rendered = photocraft_compose::render_layer(&group, band);
            let values: Vec<f32> = rendered
                .px
                .iter()
                .map(|p| {
                    let v = match mask.mask_type() {
                        MaskType::Alpha => p[3],
                        MaskType::Luminosity => {
                            let rgb = [0, 1, 2].map(|i| p[i] * p[3] + background[i] * (1.0 - p[3]));
                            0.3 * rgb[0] + 0.59 * rgb[1] + 0.11 * rgb[2]
                        }
                    };
                    match (&exact, &transfer) {
                        (Some(f), _) => transfer::apply(f, v),
                        (None, Some(table)) => f32::from(table[(v.clamp(0.0, 1.0) * 255.0 + 0.5) as usize]) / 255.0,
                        (None, None) => v,
                    }
                    .clamp(0.0, 1.0)
                })
                .collect();
            out.surface.write_region(band, &values);
        }
        self.masks.insert(key, out.clone());
        warn(&mut self.warnings, "soft-mask", None, "The PDF soft mask was converted to a pixel mask; its original vector objects are not editable.");
        Ok(out)
    }
    fn draw(&mut self, p: &BezPath, m: Affine, paint: &Paint<'_>, mode: &DrawMode, name: &str) -> Result<()> {
        if let DrawMode::FillAndStroke(rule, stroke) = mode {
            self.draw(p, m, paint, &DrawMode::Fill(*rule), name)?;
            return self.draw(p, m, paint, &DrawMode::Stroke(stroke.clone()), name);
        }
        geometry::transform(m)?;
        let mut shape = ShapeLayer::default();
        let name = if matches!(paint, Paint::Pattern(_)) { "PDF shading" } else { name };
        let solid = paint::solid(paint);
        let placeholder = solid.clone().unwrap_or(Fill::Solid(photocraft_doc::Color::BLACK));
        match mode {
            DrawMode::FillAndStroke(..) | DrawMode::Invisible => return Ok(()),
            DrawMode::Fill(rule) => {
                shape.path = self.path(p, m, *rule)?;
                shape.fill = Some(placeholder);
            }
            DrawMode::Stroke(props) => {
                let scale = if props.line_width == 0.0 && props.dash_array.is_empty() { Some(1.0) } else { paint::isotropic(m).filter(|s| *s > 1e-12) };
                if let Some(scale) = scale {
                    shape.path = self.path(p, m, hayro_interpret::FillRule::NonZero)?;
                    shape.stroke = Some(paint::stroke(props, placeholder, scale)?);
                } else {
                    if props.line_width == 0.0 {
                        return Err(ImportError::Unsupported("nonuniform dashed hairlines"));
                    }
                    let local = self.path(p, Affine::IDENTITY, hayro_interpret::FillRule::NonZero)?;
                    let style = paint::stroke(props, placeholder.clone(), 1.0)?;
                    let lines = photocraft_vector::flatten_path(&local, 0.01);
                    self.segments(lines.iter().map(|l| l.pts.len()).sum())?;
                    let length: f64 = lines.iter().flat_map(|l| l.pts.windows(2)).map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1)).sum();
                    let period: f64 = style.dashes.iter().map(|v| f64::from(*v) * f64::from(style.width)).sum();
                    if period > 0.0 && length / period * style.dashes.len() as f64 > self.options.limits.segments as f64 / 16.0 {
                        return Err(ImportError::Limit("dash expansion"));
                    }
                    let polys = photocraft_vector::stroke_polygons(&lines, &photocraft_vector::stroke_style(&style), 0.01);
                    self.segments(polys.iter().map(Vec::len).sum())?;
                    let mut path = Path::new(
                        polys
                            .iter()
                            .enumerate()
                            .map(|(i, p)| {
                                photocraft_doc::Subpath::polygon(p).with_op(if i == 0 { photocraft_doc::PathOp::Combine } else { photocraft_doc::PathOp::Join })
                            })
                            .collect(),
                    );
                    path = path.transform(&photocraft_doc::Affine { m: m.as_coeffs() });
                    geometry::validate(&path)?;
                    shape.path = path;
                    shape.fill = Some(placeholder);
                    warn(&mut self.warnings, "stroke-outline", Some(name), "A nonuniformly transformed stroke was converted to editable outlines.");
                }
            }
        }
        if shape.path.is_empty() {
            return Ok(());
        }
        if let Some(stroke) = &shape.stroke {
            let period: f64 = stroke.dashes.iter().map(|d| f64::from(*d) * f64::from(stroke.width)).sum();
            let length: f64 = shape
                .path
                .subpaths
                .iter()
                .flat_map(|s| s.segments())
                .map(|p| p.windows(2).map(|q| (q[1].x - q[0].x).hypot(q[1].y - q[0].y)).sum::<f64>())
                .sum();
            if period > 0.0 && length / period * stroke.dashes.len() as f64 > self.options.limits.segments as f64 / 16.0 {
                return Err(ImportError::Limit("dash expansion"));
            }
        }
        // Estimate/control cache extent before the renderer compiles or allocates it.
        let b = shape.path.control_bounds().unwrap_or((0.0, 0.0, 0.0, 0.0));
        let pad = shape.stroke.as_ref().map_or(0.0, |s| f64::from(s.width) * f64::from(s.miter_limit) + 2.0);
        let extent = geometry::rect(&[Point::new(b.0 - pad, b.1 - pad), Point::new(b.2 + pad, b.3 + pad)])?.intersect(&self.canvas);
        self.charge(extent)?;
        let compiled = photocraft_vector::CompiledShape::new(&shape, photocraft_vector::DEFAULT_TOLERANCE, self.canvas);
        let rect = compiled.bounds().unwrap_or(Rect::EMPTY).intersect(&self.canvas);
        if solid.is_some() {
            shape.cache = Some(photocraft_vector::render_shape(&shape, PixelFormat::RGBA32F, self.canvas));
            self.push(Layer::new(name, LayerContent::Shape(shape)), true)?;
            return Ok(());
        }
        let coverage_shape = shape.clone();
        let mut surface = Surface::new(PixelFormat::RGBA32F);
        let (segments, bytes) = {
            let b = self.budget.borrow();
            (self.options.limits.segments.saturating_sub(b.segments) / 3, self.options.limits.decompressed_bytes.saturating_sub(b.bytes))
        };
        let mesh = paint::mesh(paint, segments, bytes)?;
        self.budget.borrow_mut().bytes += mesh.len() as u64 * 4096;
        self.segments(mesh.len().saturating_mul(3))?;
        if !mesh.is_empty() && rect.size().area().saturating_mul(mesh.len() as u64) > 64_000_000 {
            return Err(ImportError::Limit("mesh sample work"));
        }
        let tile = if let Paint::Pattern(p) = paint
            && let Pattern::Tiling(t) = p.as_ref()
        {
            if !t.x_step.is_finite() || !t.y_step.is_finite() || t.x_step.abs() < 1e-6 || t.y_step.abs() < 1e-6 {
                return Err(ImportError::Limit("pattern step"));
            }
            geometry::transform(t.matrix)?;
            if t.matrix.determinant().abs() < 1e-15 {
                return Err(ImportError::Invalid("singular pattern transform".into()));
            }
            let local = geometry::rect(&[Point::new(t.bbox.x0, t.bbox.y0), Point::new(t.bbox.x1, t.bbox.y1)])?;
            if t.bbox.width() / f64::from(t.x_step.abs()) > 16.0 || t.bbox.height() / f64::from(t.y_step.abs()) > 16.0 {
                return Err(ImportError::Limit("overlapping pattern tiles"));
            }
            let mut child = self.child(local)?;
            child.pattern_cell = true;
            t.interpret(&mut child, Affine::IDENTITY, matches!(mode, DrawMode::Stroke(_))).ok_or_else(|| ImportError::Invalid("pattern content".into()))?;
            let children = child.end_page()?;
            self.warnings.extend(child.warnings);
            self.charge(local)?;
            Some((photocraft_compose::render_layer(&Layer::group("PDF pattern cell", children), local), t))
        } else {
            None
        };
        for band in geometry::bands(rect) {
            cancelled(self.cancel)?;
            let coverage = compiled.render_rgba(band);
            let mut px = Vec::with_capacity(coverage.len() * 4);
            for (y, row) in coverage.chunks_exact(band.width() as usize).enumerate() {
                for (x, c) in row.iter().enumerate() {
                    let point = Point::new(f64::from(band.x0) + x as f64 + 0.5, f64::from(band.y0) + y as f64 + 0.5);
                    let mut rgba = if let Some((image, t)) = &tile {
                        let p = t.matrix.inverse() * point;
                        let sx = f64::from(t.x_step.abs());
                        let sy = f64::from(t.y_step.abs());
                        let ix = ((p.x - t.bbox.x0) / sx).floor();
                        let iy = ((p.y - t.bbox.y0) / sy).floor();
                        let mut out = [0.0_f32; 4];
                        for a in 0..=(t.bbox.width() / sx).ceil() as i32 {
                            for b in 0..=(t.bbox.height() / sy).ceil() as i32 {
                                let q = Point::new(p.x - (ix - f64::from(a)) * sx, p.y - (iy - f64::from(b)) * sy);
                                if t.bbox.contains(q) {
                                    let c = image.get(q.x.floor() as i32, q.y.floor() as i32);
                                    let alpha = c[3] + out[3] * (1.0 - c[3]);
                                    if alpha > 0.0 {
                                        for i in 0..3 {
                                            out[i] = (c[i] * c[3] + out[i] * out[3] * (1.0 - c[3])) / alpha;
                                        }
                                    }
                                    out[3] = alpha;
                                }
                            }
                        }
                        out
                    } else {
                        paint::evaluate(paint, point, &mesh)?
                    };
                    rgba[3] *= c[3];
                    px.extend(rgba);
                }
            }
            surface.write_region(band, &px);
        }
        let mut layer = if let Some(fill) = paint::mirror(paint, &coverage_shape.path)? {
            shape.fill = shape.fill.as_ref().map(|_| fill.clone());
            if let Some(s) = &mut shape.stroke {
                s.paint = fill;
            }
            shape.cache = Some(surface);
            warn(
                &mut self.warnings,
                "gradient-projection",
                Some(name),
                "PDF shading retains an editable gradient and path; editing may simplify its geometry or colour function.",
            );
            Layer::new(name, LayerContent::Shape(shape))
        } else {
            warn(
                &mut self.warnings,
                "raster-fallback",
                Some(name),
                "PDF mesh, function shading or tiling paint was converted to a pixel layer; vector editing of the paint is unavailable.",
            );
            Layer::new(name, LayerContent::Raster(surface))
        };
        if let Paint::Pattern(p) = paint
            && let Pattern::Shading(s) = p.as_ref()
            && let Some(cp) = &s.shading.clip_path
        {
            let path = self.path(cp, Affine::IDENTITY, hayro_interpret::FillRule::NonZero)?;
            layer.vector_mask = Some(VectorMask::new(path));
        }
        self.push(layer, true)
    }
    fn image(&mut self, image: Image<'_, '_>, m: Affine) -> Result<()> {
        geometry::transform(m)?;
        let (w, h) = (image.width(), image.height());
        if u64::from(w) * u64::from(h) > self.options.limits.pixels || w == 0 || h == 0 {
            return Err(ImportError::Limit("image pixel"));
        }
        self.charge(Rect::from_xywh(0, 0, w, h))?;
        let mut decoded = None;
        match image {
            Image::Raster(r) => r.with_rgba(
                |rgb, alpha| {
                    let (w, h, sf) = (rgb.width(), rgb.height(), rgb.scale_factors());
                    let a = alpha.map(|a| match image::GrayImage::from_raw(a.width, a.height, a.data) {
                        Some(g) => image::imageops::resize(&g, w, h, image::imageops::FilterType::Triangle).into_raw(),
                        None => vec![0; (w as usize) * (h as usize)],
                    });
                    let rgba: Vec<[u8; 4]> = match rgb {
                        ImageData::Rgb(d) => d
                            .data
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .enumerate()
                            .map(|(i, c)| [c[0], c[1], c[2], a.as_ref().and_then(|a| a.get(i)).copied().unwrap_or(255)])
                            .collect(),
                        ImageData::Luma(d) => {
                            d.data.iter().enumerate().map(|(i, c)| [*c, *c, *c, a.as_ref().and_then(|a| a.get(i)).copied().unwrap_or(255)]).collect()
                        }
                    };
                    decoded = Some((rgba, w, h, sf));
                },
                None,
            ),
            Image::Stencil(s) => s.with_stencil(
                |luma, p| {
                    let c = paint::sample(p, Point::ZERO).unwrap_or([0.0, 0.0, 0.0, 1.0]);
                    decoded = Some((
                        luma.data.iter().map(|a| [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, (*a as f32 * c[3]) as u8]).collect(),
                        luma.width,
                        luma.height,
                        luma.scale_factors,
                    ));
                },
                None,
            ),
        }
        let Some((rgba, w, h, sf)) = decoded else {
            warn(&mut self.warnings, "image-decode", None, "An image could not be decoded and was omitted.");
            return Ok(());
        };
        // hayro supplies a transform in decoded-image pixels (including its y flip).
        let m = m * Affine::scale_non_uniform(f64::from(sf.0), f64::from(sf.1));
        geometry::transform(m)?;
        if m.determinant().abs() < 1e-15 {
            return Ok(());
        }
        let rect = geometry::rect(&[Point::ZERO, Point::new(w.into(), 0.0), Point::new(w.into(), h.into()), Point::new(0.0, h.into())].map(|p| m * p))?
            .intersect(&self.canvas);
        self.charge(rect)?;
        if rgba.len() != (w as usize).checked_mul(h as usize).ok_or(ImportError::Limit("image pixel"))? {
            return Err(ImportError::Invalid("decoded image length does not match dimensions".into()));
        }
        let inverse = m.inverse();
        let mut surface = Surface::new(PixelFormat::RGBA32F);
        for band in geometry::bands(rect) {
            cancelled(self.cancel)?;
            for y in band.y0..band.y1 {
                for x in band.x0..band.x1 {
                    let p = inverse * Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
                    if p.x >= 0.0 && p.y >= 0.0 && p.x < f64::from(w) && p.y < f64::from(h) {
                        let c = rgba[p.y as usize * w as usize + p.x as usize].map(|c| f32::from(c) / 255.0);
                        surface.write_pixel(x, y, &c);
                    }
                }
            }
        }
        warn(
            &mut self.warnings,
            "image-placement",
            Some("PDF image"),
            "Embedded image placement was baked to editable pixel layers; interpolation may differ from a PDF viewer.",
        );
        self.push(Layer::new("PDF image", LayerContent::Raster(surface)), true)
    }
    /// Applies one draw call's soft mask and blend mode to the layers it pushes.
    fn props(&mut self, mask: Option<&SoftMask<'_>>, blend: hayro_interpret::BlendMode) -> Result<()> {
        self.mask = mask.map(|m| self.soft_mask(m)).transpose()?;
        self.blend = paint::blend(blend);
        Ok(())
    }
    fn children(&self) -> usize {
        self.frames.last().map_or(0, |f| f.children.len())
    }
    fn glyph<'p>(&mut self, g: &Glyph<'p>, m: Affine, gm: Affine, paint: &Paint<'p>, draw: &DrawMode) -> Result<()> {
        match g {
            Glyph::Outline(o) => {
                if matches!(draw, DrawMode::Stroke(_)) {
                    let mut outline = o.outline();
                    outline.apply_affine(gm);
                    self.draw(&outline, m, paint, draw, "PDF glyph")?;
                } else {
                    self.draw(&o.outline(), m * gm, paint, draw, "PDF glyph")?;
                }
                warn(
                    &mut self.warnings,
                    "glyph-outlines",
                    Some("PDF glyph"),
                    "PDF glyphs were converted to editable curves using their embedded font or bundled standard substitute; text editing is unavailable.",
                );
            }
            Glyph::Type3(t) => {
                let mut child = self.child(self.canvas)?;
                t.interpret(&mut child, m, gm, paint);
                let layers = child.end_page()?;
                self.warnings.extend(child.warnings);
                self.push(Layer::group("PDF Type 3 glyph", layers), true)?;
                warn(&mut self.warnings, "type3-glyph", Some("PDF Type 3 glyph"), "Type 3 glyph artwork was converted to an editable group.");
            }
        }
        Ok(())
    }
    fn glyph_run<'p>(&mut self, run: &GlyphRun<'_, 'p>, props: &DrawProps<'p>, mode: &DrawMode, fill: Option<FillRun>) -> Result<()> {
        if matches!(mode, DrawMode::Invisible) {
            return Ok(());
        }
        self.props(props.soft_mask.as_ref(), props.blend_mode)?;
        let m = props.transform;
        let bits = |a: Affine| a.as_coeffs().map(f64::to_bits);
        let glyphs = (run.glyphs().as_ptr().addr(), bits(m), run.glyphs().iter().map(|g| bits(g.transform())).collect());
        let (frames, start) = (self.frames.len(), self.children());
        let mut counts = Vec::with_capacity(run.glyphs().len());
        for g in run.glyphs() {
            let before = self.children();
            self.glyph(g, m, g.transform(), &props.paint, mode)?;
            counts.push(self.children() - before);
        }
        match mode {
            DrawMode::Fill(_) => self.fill_run = Some(FillRun { glyphs, frames, start, counts }),
            DrawMode::Stroke(_) => {
                let Some(fill) = fill.filter(|f| f.glyphs == glyphs && f.frames == frames && f.start + f.counts.iter().sum::<usize>() == start) else {
                    return Ok(());
                };
                let children = &mut self.frames.last_mut().ok_or_else(|| ImportError::Invalid("missing drawing frame".into()))?.children;
                let mut strokes = children.split_off(start).into_iter();
                let mut fills = children.split_off(fill.start).into_iter();
                for (f, s) in fill.counts.into_iter().zip(counts) {
                    children.extend(fills.by_ref().take(f));
                    children.extend(strokes.by_ref().take(s));
                }
            }
            DrawMode::FillAndStroke(..) | DrawMode::Invisible => {}
        }
        Ok(())
    }
}
impl<'pdf> Device<'pdf> for Builder<'_> {
    fn draw_path(&mut self, p: &BezPath, props: DrawProps<'pdf>, mode: &DrawMode) {
        self.run(|b| {
            b.props(props.soft_mask.as_ref(), props.blend_mode)?;
            b.draw(p, props.transform, &props.paint, mode, "PDF path")
        });
    }
    fn push_clip_path(&mut self, c: &ClipPath) {
        self.run(|b| {
            let p = b.path(&c.path, Affine::IDENTITY, c.fill)?;
            b.frame(Frame { kind: FrameKind::Clip, clip: Some(p), blend: BlendMode::PassThrough, ..Frame::root() })
        });
    }
    fn pop_clip(&mut self) {
        self.run(|b| {
            if b.frames.last().is_none_or(|f| f.kind != FrameKind::Clip) {
                return Err(ImportError::Invalid("PDF clip stack".into()));
            }
            b.pop()
        });
    }
    fn push_transparency_group(&mut self, opacity: f32, m: Option<SoftMask<'pdf>>, blend: hayro_interpret::BlendMode) {
        self.run(|b| {
            if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                return Err(ImportError::Invalid("group opacity".into()));
            }
            let mask = m.as_ref().map(|m| b.soft_mask(m)).transpose()?;
            b.frame(Frame { kind: FrameKind::Transparency, opacity, blend: paint::blend(blend), mask, ..Frame::root() })
        });
    }
    fn pop_transparency_group(&mut self) {
        self.run(|b| {
            if b.frames.last().is_none_or(|f| f.kind != FrameKind::Transparency) {
                return Err(ImportError::Invalid("PDF transparency stack".into()));
            }
            b.pop()
        });
    }
    fn begin_marked_content(&mut self, tag: &[u8], _: Option<i32>) {
        self.run(|b| {
            {
                let mut budget = b.budget.borrow_mut();
                budget.callbacks = budget.callbacks.saturating_add(1);
                if budget.callbacks > b.options.limits.segments {
                    return Err(ImportError::Limit("marked content callbacks"));
                }
            }
            if b.markers.len() + b.nested >= b.options.limits.depth.min(64) {
                return Err(ImportError::Limit("marked content depth"));
            }
            let mut owner = None;
            if tag.starts_with(groups::PREFIX) {
                let isolated = *b.groups.get(tag).ok_or_else(|| ImportError::Invalid("unknown PDF group marker".into()))?;
                let index = b.frames.len().checked_sub(2).ok_or_else(|| ImportError::Invalid("PDF marker without Form frame".into()))?;
                if b.frames.last().is_none_or(|f| f.kind != FrameKind::Clip)
                    || b.frames[index].kind != FrameKind::Transparency
                    || b.frames[index].marked
                    || !b.frames[index + 1].children.is_empty()
                {
                    return Err(ImportError::Invalid("PDF marker does not own a transparency frame".into()));
                }
                if !isolated && b.pattern_cell {
                    return Err(ImportError::Unsupported("nonisolated group inside rasterized pattern"));
                }
                b.frames[index].isolation = if isolated { GroupIsolation::Isolated } else { GroupIsolation::NonIsolated };
                b.frames[index].marked = true;
                owner = Some(index);
            }
            b.markers.push(owner);
            Ok(())
        });
    }
    fn end_marked_content(&mut self) {
        self.run(|b| {
            let owner = b.markers.pop().ok_or_else(|| ImportError::Invalid("unbalanced PDF marker callback".into()))?;
            if let Some(index) = owner
                && (b.frames.len() != index + 2 || b.frames[index + 1].kind != FrameKind::Clip)
            {
                return Err(ImportError::Invalid("PDF marker crossed Form boundary".into()));
            }
            Ok(())
        });
    }
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'pdf>, props: DrawProps<'pdf>, mode: &DrawMode) {
        let fill = self.fill_run.take();
        self.run(|b| b.glyph_run(run, &props, mode, fill));
    }
    fn draw_image(&mut self, i: Image<'pdf, '_>, props: ImageDrawProps<'pdf>) {
        self.run(|b| {
            b.props(props.soft_mask.as_ref(), props.blend_mode)?;
            b.image(i, props.transform)
        });
    }
}
