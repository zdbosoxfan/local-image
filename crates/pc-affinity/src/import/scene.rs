//! Affinity model -> ordinary native layers. All caches are bounds-limited and detached.
use super::{cancelled, paint};
use crate::{Error, model};
use photocraft_doc::{
    Affine, BlendMode, DesignUnits, FillRule, Knot, Layer, LayerContent, Path, PathOp, PixelFormat, Rect, ShapeLayer, Size, Subpath, Surface, VectorMask,
    vector_design::{
        appearance::{Appearance, AppearanceItem, FillItem, StrokeItem},
        exchange::{ImportOptions, ImportedScene, VectorWarning},
    },
};
use photocraft_geom::Point;
use std::{borrow::Cow, io::Cursor, sync::atomic::AtomicBool};

enum RasterPixels<'a> {
    Rgba8(Cow<'a, [u8]>),
    Rgba32F(Vec<f32>),
}

pub(super) struct State<'a, 'b> {
    pub(super) options: &'a ImportOptions,
    pub(super) fonts: Option<&'b mut photocraft_text::TextEngine>,
    pub(super) bundled: Option<photocraft_text::TextEngine>,
    pub(super) cancel: &'a AtomicBool,
    pub(super) canvas: Rect,
    pub(super) bytes: u64,
    pub(super) pixels: u64,
    pub(super) segments: usize,
    pub(super) objects: usize,
    pub(super) warnings: Vec<VectorWarning>,
    pub(super) supported: usize,
    pub(super) unsupported: bool,
    pub(super) cmyk: std::collections::HashMap<usize, photocraft_cms::Transform>,
}
impl<'a, 'b> State<'a, 'b> {
    pub(super) fn new(options: &'a ImportOptions, fonts: Option<&'b mut photocraft_text::TextEngine>, cancel: &'a AtomicBool, size: Size, bytes: u64) -> Self {
        Self {
            options,
            fonts,
            bundled: None,
            cancel,
            canvas: Rect::from_size(size),
            bytes,
            pixels: 0,
            segments: 0,
            objects: 0,
            warnings: vec![],
            supported: 0,
            unsupported: false,
            cmyk: std::collections::HashMap::new(),
        }
    }
    pub(super) fn warn(&mut self, feature: &str, name: &str, message: &str) {
        if !self.warnings.iter().any(|w| w.feature == feature && w.object.as_deref() == Some(name)) {
            self.warnings.push(VectorWarning { feature: feature.into(), object: Some(name.into()), message: message.into() });
        }
    }
    pub(super) fn charge(&mut self, r: Rect) -> Result<(), Error> {
        cancelled(Some(self.cancel))?;
        if r.is_empty() {
            return Ok(());
        }
        self.pixels = self.pixels.saturating_add(r.size().area());
        if self.pixels > self.options.limits.pixels {
            return Err(Error::Limit("prepared pixels"));
        }
        let nx = (i64::from(r.x1) - 1).div_euclid(256) - i64::from(r.x0).div_euclid(256) + 1;
        let ny = (i64::from(r.y1) - 1).div_euclid(256) - i64::from(r.y0).div_euclid(256) + 1;
        self.bytes = self.bytes.saturating_add((nx as u64).saturating_mul(ny as u64).saturating_mul(256 * 256 * 16));
        if self.bytes > self.options.limits.decompressed_bytes {
            return Err(Error::Limit("prepared cache bytes"));
        }
        Ok(())
    }
    pub(super) fn charge_layers(&mut self, count: usize) -> Result<(), Error> {
        self.objects = self.objects.saturating_add(count);
        self.bytes = self.bytes.saturating_add((count as u64).saturating_mul(std::mem::size_of::<Layer>() as u64 * 2));
        if self.objects > self.options.limits.objects || self.bytes > self.options.limits.decompressed_bytes {
            return Err(Error::Limit("expanded native layers"));
        }
        Ok(())
    }
    pub(super) fn affine(&self, m: Affine) -> Result<(), Error> {
        if m.m.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.) { Ok(()) } else { Err(Error::Limit("transform coordinates")) }
    }
    pub(super) fn path(&mut self, p: &model::Path, m: Affine, even: bool) -> Result<Path, Error> {
        let n = p.subpaths.iter().map(|p| p.segments.len() + 1).sum::<usize>();
        self.segments = self.segments.saturating_add(n);
        if self.segments > self.options.limits.segments {
            return Err(Error::Limit("native segments"));
        }
        let mut subpaths = Vec::new();
        for s in &p.subpaths {
            let point = |p: model::Point| m.apply(Point::new(p.x, p.y));
            let start = point(s.start);
            let mut knots = vec![Knot::corner(start.x, start.y)];
            for [a, b, e] in &s.segments {
                let (a, b, e) = (point(*a), point(*b), point(*e));
                if let Some(last) = knots.last_mut() {
                    last.out_ctrl = a;
                }
                knots.push(Knot { anchor: e, in_ctrl: b, out_ctrl: e, smooth: false });
            }
            // Affinity repeats the first anchor on closed paths; preserve its incoming handle.
            if s.closed
                && knots.len() > 1
                && knots.last().is_some_and(|k| k.anchor == start)
                && let Some(last) = knots.pop()
            {
                knots[0].in_ctrl = last.in_ctrl;
            }
            if knots
                .iter()
                .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
                .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1_000_000. || p.y.abs() > 1_000_000.)
            {
                return Err(Error::Limit("path coordinates"));
            }
            subpaths.push(Subpath { closed: s.closed, knots, op: if subpaths.is_empty() { PathOp::Combine } else { PathOp::Join } });
        }
        let mut path = Path::new(subpaths);
        path.fill_rule = if even { FillRule::EvenOdd } else { FillRule::NonZero };
        Ok(path)
    }
    pub(super) fn shape(&mut self, path: Path, fills: &[crate::paint::Paint], strokes: &[crate::paint::Stroke], m: Affine, name: &str) -> Result<Layer, Error> {
        let b = path.control_bounds().unwrap_or((0., 0., 0., 0.));
        let mut items = Vec::new();
        for s in strokes.iter().filter(|s| s.behind) {
            if let Some(base) = paint::stroke(self, s, m, b, name)? {
                items.push(AppearanceItem::Stroke(StrokeItem { base, ..Default::default() }));
            }
        }
        for f in fills {
            if let Some(paint) = paint::fill(self, f, m, b, name)? {
                items.push(AppearanceItem::Fill(FillItem::new(paint)));
            }
        }
        for s in strokes.iter().filter(|s| !s.behind) {
            if let Some(base) = paint::stroke(self, s, m, b, name)? {
                items.push(AppearanceItem::Stroke(StrokeItem { base, ..Default::default() }));
            }
        }
        let appearance = Appearance { items };
        let mut shape = ShapeLayer { path, ..Default::default() };
        photocraft_vector::appearance::mirror_legacy(&mut shape, &appearance);
        let compiled = photocraft_vector::appearance::CompiledAppearance::new(&shape, Some(&appearance), &[], self.canvas)
            .map_err(|_| Error::Limit("native appearance geometry"))?;
        self.charge(compiled.bounds())?;
        let mut surface = Surface::new(PixelFormat::RGBA32F);
        for r in tiles(compiled.bounds()) {
            cancelled(Some(self.cancel))?;
            let rgba = compiled.render_rgba(r).map_err(|_| Error::Limit("appearance region"))?;
            let data: Vec<f32> = rgba.into_iter().flatten().collect();
            surface.write_region(r, &data);
        }
        shape.cache = Some(surface);
        let stack = (appearance != photocraft_vector::appearance::legacy_appearance(&shape)).then_some(appearance);
        let mut layer = Layer::new(name, LayerContent::Shape(shape));
        // Ordinary paints retain the established path-edit route; explicit paint order uses the appearance stack.
        layer.vector.appearance = stack;
        self.supported += 1;
        Ok(layer)
    }
    pub(super) fn image(&mut self, image: &model::Image, m: Affine) -> Result<Surface, Error> {
        let count = u64::from(image.width) * u64::from(image.height);
        if count == 0 || count > crate::raster::MAX_PIXELS || count > self.options.limits.pixels {
            return Err(Error::Limit("image pixels"));
        }
        self.bytes = self.bytes.saturating_add(count.saturating_mul(16));
        if self.bytes > self.options.limits.decompressed_bytes {
            return Err(Error::Limit("decoded image bytes"));
        }
        cancelled(Some(self.cancel))?;
        let raw = match &image.pixels {
            model::Pixels::Rgba8(v) => {
                if v.len() as u64 != count * 4 {
                    return Err(Error::Malformed("RGBA length"));
                }
                RasterPixels::Rgba8(Cow::Borrowed(v))
            }
            model::Pixels::Cmyka8(v) => {
                if v.len() as u64 != count * 5 {
                    return Err(Error::Malformed("CMYK length"));
                }
                let key = image.profile.as_ref().map_or(0, |p| std::sync::Arc::as_ptr(p) as usize);
                if !self.cmyk.contains_key(&key) {
                    self.bytes = self.bytes.saturating_add(8 << 20).saturating_add(image.profile.as_ref().map_or(0, |p| (p.len() as u64).saturating_mul(8)));
                    if self.bytes > self.options.limits.decompressed_bytes {
                        return Err(Error::Limit("CMYK transform bytes"));
                    }
                    let profile = if let Some(bytes) = &image.profile {
                        let profile = photocraft_cms::Profile::parse(bytes).map_err(|_| Error::Malformed("invalid CMYK profile"))?;
                        if profile.color_space == photocraft_cms::ColorSpace::Cmyk {
                            profile
                        } else {
                            self.warn(
                                "colour-profile",
                                "Affinity document",
                                "The embedded profile is not CMYK; CMYK pixels use the built-in coated CMYK profile.",
                            );
                            photocraft_cms::Builtin::CoatedCmyk.profile().clone()
                        }
                    } else {
                        self.warn(
                            "colour-profile",
                            "Affinity document",
                            "CMYK pixels use the built-in coated CMYK profile because no source profile is embedded.",
                        );
                        photocraft_cms::Builtin::CoatedCmyk.profile().clone()
                    };
                    let transform =
                        photocraft_cms::Transform::new(&profile, photocraft_cms::Builtin::Srgb.profile(), photocraft_cms::Intent::RelativeColorimetric, true)
                            .map_err(|_| Error::Malformed("CMYK source transform"))?;
                    self.cmyk.insert(key, transform);
                }
                let transform = self.cmyk.get(&key).ok_or(Error::Malformed("CMYK transform"))?;
                let mut out = vec![0f32; count as usize * 4];
                for (index, (input, output)) in v.as_chunks::<5>().0.iter().zip(out.as_chunks_mut::<4>().0.iter_mut()).enumerate() {
                    if index % 65536 == 0 {
                        cancelled(Some(self.cancel))?;
                    }
                    let mut rgb = [0.; 3];
                    transform.eval(&[input[0], input[1], input[2], input[3]].map(|v| f32::from(v) / 255.), &mut rgb);
                    if rgb.iter().any(|v| !v.is_finite()) {
                        return Err(Error::Malformed("nonfinite CMYK conversion"));
                    }
                    output[..3].copy_from_slice(&rgb.map(|v| v.clamp(0., 1.)));
                    output[3] = f32::from(input[4]) / 255.;
                }
                RasterPixels::Rgba32F(out)
            }
            model::Pixels::Encoded(v) => {
                let reader = || image::ImageReader::new(Cursor::new(v)).with_guessed_format().map_err(|_| Error::Malformed("image format"));
                let mut r = reader()?;
                if !matches!(r.format(), Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)) {
                    return Err(Error::Unsupported("image codec"));
                }
                if reader()?.into_dimensions().map_err(|_| Error::Malformed("image dimensions"))? != (image.width, image.height) {
                    return Err(Error::Malformed("image dimensions differ from bitmap"));
                }
                let mut lim = image::Limits::default();
                lim.max_image_width = Some(image.width);
                lim.max_image_height = Some(image.height);
                lim.max_alloc = Some(self.options.limits.decompressed_bytes.min(count * 16));
                r.limits(lim);
                let pixels = r
                    .decode()
                    .map_err(|e| if matches!(e, image::ImageError::Limits(_)) { Error::Limit("image decoder") } else { Error::Malformed("image pixels") })?
                    .into_rgba8()
                    .into_raw();
                RasterPixels::Rgba8(Cow::Owned(pixels))
            }
        };
        let transform = m.mul(&affine(image.transform));
        self.affine(transform)?;
        let inv = transform.inverse().ok_or(Error::Malformed("singular image transform"))?;
        let points =
            [Point::new(0., 0.), Point::new(image.width.into(), 0.), Point::new(0., image.height.into()), Point::new(image.width.into(), image.height.into())]
                .map(|p| transform.apply(p));
        let r = rect_of(&points)?.intersect(&self.canvas);
        self.charge(r)?;
        let mut surface = Surface::new(PixelFormat::RGBA32F);
        for r in tiles(r) {
            cancelled(Some(self.cancel))?;
            let mut out = Vec::with_capacity(r.size().area() as usize * 4);
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    let p = inv.apply(Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5));
                    if p.x < 0. || p.y < 0. || p.x >= f64::from(image.width) || p.y >= f64::from(image.height) {
                        out.extend([0.; 4]);
                        continue;
                    }
                    let i = ((p.y.floor() as usize) * image.width as usize + p.x.floor() as usize) * 4;
                    match &raw {
                        RasterPixels::Rgba8(pixels) => out.extend(pixels[i..i + 4].iter().map(|v| f32::from(*v) / 255.)),
                        RasterPixels::Rgba32F(pixels) => out.extend_from_slice(&pixels[i..i + 4]),
                    }
                }
            }
            surface.write_region(r, &out);
        }
        self.supported += 1;
        Ok(surface)
    }
    fn nodes(
        &mut self,
        nodes: &[model::Node],
        m: Affine,
        depth: usize,
        native: &model::Document,
        spread: &model::Spread,
        masters: &mut std::collections::HashSet<usize>,
    ) -> Result<Vec<Layer>, Error> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }
        if depth > self.options.limits.depth.min(photocraft_doc::MAX_GROUP_DEPTH) {
            return Err(Error::Limit("native group depth"));
        }
        let mut out = Vec::new();
        for n in nodes
            .iter()
            .filter(|n| matches!(n.kind, model::Kind::MasterInstance { .. }))
            .chain(nodes.iter().filter(|n| !matches!(n.kind, model::Kind::MasterInstance { .. })))
        {
            cancelled(Some(self.cancel))?;
            self.charge_layers(1)?;
            let name = if n.name.is_empty() { "Affinity object" } else { &n.name };
            let children = self.nodes(&n.children, m, depth + 1, native, spread, masters)?;
            let mut layer = match &n.kind {
                model::Kind::Group | model::Kind::Layer => Layer::group(name, children),
                model::Kind::Shape { path, fills, strokes, even_odd } | model::Kind::Artboard { path, background: fills, strokes, even_odd, .. } => {
                    let path = self.path(path, m, *even_odd)?;
                    if children.is_empty() && matches!(n.kind, model::Kind::Shape { .. }) {
                        self.shape(path, fills, strokes, m, name)?
                    } else {
                        let behind: Vec<_> = strokes.iter().filter(|s| s.behind).cloned().collect();
                        let front: Vec<_> = strokes.iter().filter(|s| !s.behind).cloned().collect();
                        self.charge_layers(2 + usize::from(!front.is_empty()))?;
                        let mut parts = vec![self.shape(path.clone(), fills, &behind, m, name)?];
                        let mut content = Layer::group("Clipped content", children);
                        content.vector.clip = Some(path.clone());
                        content.vector_mask = Some(VectorMask::new(path.clone()));
                        parts.push(content);
                        if !front.is_empty() {
                            parts.push(self.shape(path, &[], &front, m, "Affinity stroke")?);
                        }
                        let mut group = Layer::group(name, parts);
                        if let model::Kind::Artboard { rect, .. } = n.kind {
                            let r = rect_of(&[m.apply(Point::new(rect.x0, rect.y0)), m.apply(Point::new(rect.x1, rect.y1))])?;
                            if let LayerContent::Group(g) = &mut group.content {
                                g.artboard = Some(photocraft_doc::Artboard {
                                    drawing_scale: None,
                                    rect: r,
                                    background: photocraft_doc::ArtboardBackground::Transparent,
                                    preset: String::new(),
                                });
                            }
                        }
                        group
                    }
                }
                model::Kind::Table(t) => self.table(t, m, name)?,
                model::Kind::MasterInstance { master, page, master_page, count } => {
                    if !masters.insert(*master) {
                        return Err(Error::Malformed("cyclic master pages"));
                    }
                    let source = native.masters.get(*master).ok_or(Error::Malformed("missing master"))?;
                    let mut parts = Vec::new();
                    for offset in 0..*count {
                        let target = spread.pages.get(page.saturating_add(offset)).copied().or_else(|| (*page == 0 && offset == 0).then_some(spread.bounds));
                        let from = source
                            .pages
                            .get(master_page.saturating_add(offset))
                            .copied()
                            .or_else(|| (*master_page == 0 && offset == 0).then_some(source.bounds));
                        let (Some(target), Some(from)) = (target, from) else {
                            self.warn("master-page", name, "Master page range does not match the spread; unmatched pages omitted.");
                            continue;
                        };
                        let at = m.mul(&Affine::translate(target.x0 - from.x0, target.y0 - from.y0));
                        let nodes = self.nodes(&source.nodes, at, depth + 1, native, source, masters)?;
                        let clip =
                            self.path(&model::Path { subpaths: vec![crate::shapes::rectangle(target.x0, target.y0, target.x1, target.y1)] }, m, false)?;
                        self.charge_layers(1)?;
                        let mut page = Layer::group("Master page", nodes);
                        page.vector_mask = Some(VectorMask::new(clip.clone()));
                        page.vector.clip = Some(clip);
                        parts.push(page);
                    }
                    masters.remove(master);
                    Layer::group(name, parts)
                }
                model::Kind::Text(t) => {
                    if !children.is_empty() {
                        self.warn("text-children", name, "Children of text are retained in a group.");
                        self.charge_layers(1)?;
                        let text = self.text(t, m, name)?;
                        let mut v = vec![text];
                        v.extend(children);
                        Layer::group(name, v)
                    } else {
                        self.text(t, m, name)?
                    }
                }
                model::Kind::Image(i) => {
                    let image = Layer::new(name, LayerContent::Raster(self.image(i, m)?));
                    if children.is_empty() {
                        image
                    } else {
                        self.charge_layers(1)?;
                        let mut v = vec![image];
                        v.extend(children);
                        Layer::group(name, v)
                    }
                }
                model::Kind::Unsupported => {
                    self.unsupported = true;
                    self.warn("unsupported-record", name, &format!("Unsupported {} record; available children retained.", n.class));
                    Layer::group(name, children)
                }
            };
            self.effects(&n.effects, &mut layer, m, name)?;
            if let Some(p) = &n.mask {
                let mut path = self.path(p, m, false)?;
                if path.is_empty() {
                    path.inverted = true;
                }
                layer.vector.clip = Some(path.clone());
                layer.vector_mask = Some(VectorMask::new(path));
            }
            if let Some(i) = &n.pixel_mask {
                let rgba = self.image(i, m)?;
                // A mask is auxiliary data, not supported visible artwork.
                self.supported = self.supported.saturating_sub(1);
                let mut mask = photocraft_doc::LayerMask::hide_all();
                let r = rgba.content_bounds();
                for r in tiles(r) {
                    cancelled(Some(self.cancel))?;
                    let data: Vec<f32> = rgba.read_region(r).as_chunks::<4>().0.iter().map(|v| v[0] * v[3]).collect();
                    mask.surface.write_region(r, &data);
                }
                layer.mask = Some(mask);
            }
            layer.visible = n.visible;
            layer.locks.all = n.locked;
            if !n.opacity.is_finite() {
                return Err(Error::Malformed("layer opacity"));
            }
            layer.opacity = n.opacity as f32;
            layer.blend = blend(n.blend, self, name);
            out.push(layer);
        }
        Ok(out)
    }
}
pub(super) fn convert(
    native: model::Document,
    options: &ImportOptions,
    fonts: Option<&mut photocraft_text::TextEngine>,
    cancel: &AtomicBool,
    bytes: u64,
) -> Result<ImportedScene, Error> {
    let indices = if options.pages.is_empty() { vec![0] } else { options.pages.clone() };
    let mut unique = std::collections::HashSet::new();
    let scale = f64::from(options.dpi) / native.dpi;
    let mut selected = Vec::new();
    let mut width = 0.;
    let mut height = 0.0f64;
    for i in indices {
        if !unique.insert(i) {
            return Err(Error::Malformed("duplicate page index"));
        }
        let s = native.spreads.get(i).ok_or(Error::Malformed("page index"))?;
        let b = s.bounds;
        let (w, h) = ((b.x1 - b.x0) * scale, (b.y1 - b.y0) * scale);
        if ![b.x0, b.y0, w, h].iter().all(|v| v.is_finite()) || w <= 0. || h <= 0. {
            return Err(Error::Malformed("page dimensions"));
        }
        let m = Affine { m: [scale, 0., 0., scale, width - b.x0 * scale, -b.y0 * scale] };
        selected.push((s, m, w, h));
        width += w;
        height = height.max(h);
    }
    if width > 1_000_000. || height > 1_000_000. || width.ceil() * height.ceil() > options.limits.pixels as f64 {
        return Err(Error::Limit("canvas pixels"));
    }
    let size = Size::new(width.ceil() as u32, height.ceil() as u32);
    let mut state = State::new(options, fonts, cancel, size, bytes);
    let mut layers = Vec::new();
    for (s, m, w, h) in selected {
        state.affine(m)?;
        let mut nodes = state.nodes(&s.nodes, m, 1, &native, s, &mut std::collections::HashSet::new())?;
        state.charge_layers(1)?;
        if !s.transparent {
            state.charge_layers(1)?;
            let path = Path::new(vec![Subpath::polygon(&[
                (m.m[4] + s.bounds.x0 * scale, 0.),
                (m.m[4] + s.bounds.x0 * scale + w, 0.),
                (m.m[4] + s.bounds.x0 * scale + w, h),
                (m.m[4] + s.bounds.x0 * scale, h),
            ])]);
            nodes.insert(
                0,
                state.shape(
                    path,
                    &[crate::paint::Paint::Solid(crate::paint::Color::Rgb { r: 1., g: 1., b: 1., a: 1. })],
                    &[],
                    Affine::IDENTITY,
                    "Page background",
                )?,
            );
            // The paper background must not hide a wholly unsupported artwork import.
            state.supported = state.supported.saturating_sub(1);
        }
        // Each spread retains its original page clipping after placement into a larger canvas.
        let x = m.m[4] + s.bounds.x0 * scale;
        let clip = Path::new(vec![Subpath::polygon(&[(x, 0.), (x + w, 0.), (x + w, h), (x, h)])]);
        let mut page = Layer::group("Affinity page", nodes);
        page.vector.clip = Some(clip.clone());
        page.vector_mask = Some(VectorMask::new(clip));
        layers.push(page);
    }
    fn tree_depth(layers: &[Layer], depth: usize) -> usize {
        layers.iter().map(|l| if let LayerContent::Group(g) = &l.content { tree_depth(&g.children, depth + 1) } else { depth }).max().unwrap_or(depth)
    }
    if tree_depth(&layers, 0) > options.limits.depth.min(photocraft_doc::MAX_GROUP_DEPTH) {
        return Err(Error::Limit("expanded group depth"));
    }
    if state.unsupported && state.supported == 0 {
        return Err(Error::Unsupported("no supported artwork"));
    }
    for message in native.warnings {
        if !state.warnings.iter().any(|w| w.message == message) {
            state.warnings.push(VectorWarning { feature: "affinity-record".into(), object: None, message });
        }
    }
    Ok(ImportedScene { size, resolution_dpi: options.dpi, units: DesignUnits::Pixels, layers, symbols: vec![], warnings: state.warnings })
}
fn affine(m: model::Affine) -> Affine {
    Affine { m: m.0 }
}
fn rect_of(points: &[Point]) -> Result<Rect, Error> {
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1_000_000. || p.y.abs() > 1_000_000.) {
        return Err(Error::Limit("pixel bounds"));
    }
    let x0 = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min).floor();
    let x1 = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max).ceil();
    let y0 = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min).floor();
    let y1 = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max).ceil();
    Ok(Rect::new(x0 as i32, y0 as i32, x1 as i32, y1 as i32))
}
fn tiles(r: Rect) -> Vec<Rect> {
    let mut out = Vec::new();
    for y in (r.y0..r.y1).step_by(256) {
        for x in (r.x0..r.x1).step_by(256) {
            out.push(Rect::new(x, y, (x + 256).min(r.x1), (y + 256).min(r.y1)));
        }
    }
    out
}
pub(super) fn blend(b: model::Blend, s: &mut State<'_, '_>, name: &str) -> BlendMode {
    use model::Blend as B;
    match b {
        B::Normal => BlendMode::Normal,
        B::PassThrough => BlendMode::PassThrough,
        B::Darken => BlendMode::Darken,
        B::DarkerColor => BlendMode::DarkerColor,
        B::Multiply => BlendMode::Multiply,
        B::ColorBurn => BlendMode::ColorBurn,
        B::Lighten => BlendMode::Lighten,
        B::LighterColor => BlendMode::LighterColor,
        B::Screen => BlendMode::Screen,
        B::ColorDodge => BlendMode::ColorDodge,
        B::Add => BlendMode::LinearDodge,
        B::Overlay => BlendMode::Overlay,
        B::SoftLight => BlendMode::SoftLight,
        B::HardLight => BlendMode::HardLight,
        B::VividLight => BlendMode::VividLight,
        B::PinLight => BlendMode::PinLight,
        B::LinearLight => BlendMode::LinearLight,
        B::HardMix => BlendMode::HardMix,
        B::Difference => BlendMode::Difference,
        B::Exclusion => BlendMode::Exclusion,
        B::Subtract => BlendMode::Subtract,
        B::Hue => BlendMode::Hue,
        B::Saturation => BlendMode::Saturation,
        B::Luminosity => BlendMode::Luminosity,
        B::Color => BlendMode::Color,
        _ => {
            s.warn("blend-mode", name, "Unsupported blend mode imported as Normal.");
            BlendMode::Normal
        }
    }
}

pub(super) fn bounds(layer: &Layer) -> Rect {
    if let Some(surface) = layer.surface() {
        return surface.content_bounds();
    }
    layer.children().unwrap_or_default().iter().map(bounds).fold(Rect::EMPTY, |a, b| a.union(&b))
}
