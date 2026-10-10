//! Visible display tiles backed by a detached, sparse projection of the normal layer tree.
//! No document cache or session is mutated. Tile coordinates have one document-global origin.
use photocraft_doc::{DocId, Document, Effect, Fill, FillCache, FxPaint, Layer, LayerContent, LayerId, LayerMask};
use photocraft_geom::{Affine, Rect, Size};
use photocraft_raster::Surface;
use std::collections::HashMap;

pub const MAX_VIEW_PIXELS: u64 = 16_777_216;
pub const VIEW_TILE: i32 = 256;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewError(pub String);
impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ViewError {}

#[derive(Clone, Debug)]
pub struct ViewRequest {
    pub document: DocId,
    pub revision: u64,
    pub view_key: u64,
    pub logical_zoom: f64,
    pub pixels_per_point: f64,
    pub visible: Rect,
    pub display_generation: u64,
    /// Feature providers fold relevant font/master/pattern/appearance generations per layer.
    pub dependencies: HashMap<LayerId, u64>,
}
impl ViewRequest {
    pub fn scale(&self) -> Result<f64, ViewError> {
        if !self.logical_zoom.is_finite() || !self.pixels_per_point.is_finite() || self.logical_zoom <= 0.0 || self.pixels_per_point <= 0.0 {
            return Err(ViewError("invalid zoom or physical pixel scale".into()));
        }
        let desired = (self.logical_zoom * self.pixels_per_point).max(1.0);
        if desired > 64.0 {
            return Err(ViewError("display scale exceeds 64x budget".into()));
        }
        // Quarter-octave buckets never undersample; exact zoom remains with the canvas sampler.
        Ok(2.0f64.powf((desired.log2() * 4.0).ceil() / 4.0))
    }
}

#[derive(Clone)]
pub struct ViewTiles {
    pub scale: f64,
    pub rect: Rect,
    pub tiles: Vec<super::Buffer>,
}
impl ViewTiles {
    pub fn buffer(&self) -> super::Buffer {
        let mut b = super::Buffer::transparent(self.rect);
        for t in &self.tiles {
            for y in t.rect.y0..t.rect.y1 {
                let start = ((y - self.rect.y0) as usize * self.rect.width() as usize) + (t.rect.x0 - self.rect.x0) as usize;
                let src = (y - t.rect.y0) as usize * t.rect.width() as usize;
                b.px[start..start + t.rect.width() as usize].copy_from_slice(&t.px[src..src + t.rect.width() as usize]);
            }
        }
        b
    }
}

pub type Projector = fn(&Document, &Layer, f64, Rect) -> Result<Surface, ViewError>;

pub fn scaled_rect(rect: Rect, scale: f64) -> Result<Rect, ViewError> {
    if !scale.is_finite() || !(1.0..=64.0).contains(&scale) {
        return Err(ViewError("invalid display scale".into()));
    }
    if rect.is_empty() {
        return Ok(Rect::EMPTY);
    }
    let a = [f64::from(rect.x0) * scale, f64::from(rect.y0) * scale, f64::from(rect.x1) * scale, f64::from(rect.y1) * scale];
    if a.iter().any(|v| v.abs() > f64::from(i32::MAX / 4)) {
        return Err(ViewError("scaled coordinates exceed budget".into()));
    }
    Ok(Rect::new(a[0].floor() as i32, a[1].floor() as i32, a[2].ceil() as i32, a[3].ceil() as i32))
}

pub fn tile_rects(rect: Rect, side: i32) -> Result<Vec<Rect>, ViewError> {
    if side <= 0 || u64::from(rect.width()) * u64::from(rect.height()) > MAX_VIEW_PIXELS {
        return Err(ViewError("visible tile budget exceeded".into()));
    }
    if rect.is_empty() {
        return Ok(Vec::new());
    }
    let mut v = Vec::new();
    for y in (rect.y0.div_euclid(side)..=(rect.y1 - 1).div_euclid(side)).map(|i| i * side) {
        for x in (rect.x0.div_euclid(side)..=(rect.x1 - 1).div_euclid(side)).map(|i| i * side) {
            v.push(Rect::from_xywh(x, y, side as u32, side as u32).intersect(&rect));
        }
    }
    Ok(v)
}

/// Nearest document sampling matches the existing high-zoom raster canvas. Only the
/// requested apron is allocated, including defaults and pixels outside the document.
pub fn sample_surface(source: &Surface, scale: f64, clip: Rect) -> Result<Surface, ViewError> {
    if !scale.is_finite() || !(1.0..=64.0).contains(&scale) || u64::from(clip.width()) * u64::from(clip.height()) > MAX_VIEW_PIXELS {
        return Err(ViewError("surface projection exceeds budget".into()));
    }
    let mut out = Surface::with_default(source.format(), &source.default_pixel());
    let region = scaled_rect(source.content_bounds(), scale)?.intersect(&clip);
    let channels = source.channels();
    let mut row = vec![0.0; region.width() as usize * channels];
    for y in region.y0..region.y1 {
        let sy = ((f64::from(y) + 0.5) / scale).floor() as i32;
        for (i, p) in row.chunks_exact_mut(channels).enumerate() {
            let sx = ((f64::from(region.x0) + i as f64 + 0.5) / scale).floor() as i32;
            for (c, v) in p.iter_mut().enumerate() {
                *v = source.sample_channel(sx, sy, c);
            }
        }
        out.write_region(Rect::new(region.x0, y, region.x1, y + 1), &row);
    }
    out.prune();
    Ok(out)
}

fn paint(fill: &mut Fill, s: f32) {
    if let Fill::Pattern { scale, phase, .. } = fill {
        *scale *= s;
        phase.0 *= s;
        phase.1 *= s;
    }
}
fn fx_paint(p: &mut FxPaint, s: f32) {
    if let FxPaint::Pattern { scale, .. } = p {
        *scale *= s;
    }
}
fn scale_effects(l: &mut Layer, s: f32) -> Result<(), ViewError> {
    for e in &mut l.effects.items {
        match e {
            Effect::DropShadow(a) | Effect::InnerShadow(a) => {
                a.distance *= s;
                a.size *= s;
            }
            Effect::OuterGlow(a) | Effect::InnerGlow(a) => {
                a.size *= s;
                fx_paint(&mut a.paint, s);
            }
            Effect::Stroke(a) => {
                a.size *= s;
                fx_paint(&mut a.paint, s);
            }
            Effect::Satin(a) => {
                a.distance *= s;
                a.size *= s;
            }
            Effect::BevelEmboss(a) => {
                a.size *= s;
                a.soften *= s;
                if let Some(t) = &mut a.texture {
                    t.scale *= s;
                }
            }
            Effect::PatternOverlay { scale, phase, .. } => {
                *scale *= s;
                phase.0 *= s;
                phase.1 *= s;
            }
            _ => {}
        }
    }
    if let Some((x, y)) = &mut l.effects.reference {
        *x *= f64::from(s);
        *y *= f64::from(s);
    }
    // The established effect renderer caps reach at 512 device pixels. Refuse a projection
    // that would silently truncate an effect until package 08 supplies an uncapped provider.
    if super::effects::has_effects(l) && super::effects::margin(l) >= super::effects::MAX_REACH as i32 + 2 {
        return Err(ViewError("scaled effects exceed compositor reach".into()));
    }
    Ok(())
}

fn fill_surface(doc: &Document, source: &Layer, fill: &Fill, scale: f64, clip: Rect) -> Result<Surface, ViewError> {
    let format = doc.pixel_format();
    let frame = scaled_rect(super::fill_frame(source, doc.bounds()), scale)?;
    let pixels = match fill {
        Fill::Solid(color) => {
            let rgb = color.to_rgb();
            vec![[rgb[0], rgb[1], rgb[2], color.alpha]; clip.width() as usize * clip.height() as usize]
        }
        Fill::Gradient { .. } => super::gradient_fill::render(fill, clip, frame),
        Fill::Pattern { id, name, scale, phase, angle, link } => {
            let p = photocraft_doc::pattern::find(&doc.patterns, id, name).ok_or_else(|| ViewError("missing display pattern".into()))?;
            if u64::from(p.width) * u64::from(p.height) > MAX_VIEW_PIXELS {
                return Err(ViewError("pattern exceeds display budget".into()));
            }
            let tile = super::pattern::Tile::new(p).ok_or_else(|| ViewError("empty display pattern".into()))?;
            super::pattern::render(&tile, &super::pattern::Placement::new(frame, *link, *phase, *scale, *angle), clip)
        }
    };
    let mut out = Surface::new(format);
    let mut data = Vec::with_capacity(pixels.len() * format.channels());
    for p in pixels {
        data.extend(photocraft_raster::from_rgba(&format, p));
    }
    out.write_region(clip, &data);
    out.prune();
    Ok(out)
}

/// Only the patterns referenced by this layer participate in its projection identity.
pub fn dependency_key(doc: &Document, layer: &Layer, supplied: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    supplied.hash(&mut h);
    if super::effects::has_effects(layer) {
        format!("{:?}", doc.global_light).hash(&mut h);
    }
    if matches!(layer.content, LayerContent::Text(_)) {
        doc.resolution_dpi.to_bits().hash(&mut h);
    }
    doc.icc_profile.as_ref().map(std::sync::Arc::as_ptr).hash(&mut h);
    let mut reference = |id: &str, name: &str| {
        if let Some(p) = photocraft_doc::pattern::find(&doc.patterns, id, name) {
            p.id.hash(&mut h);
            p.name.hash(&mut h);
            p.width.hash(&mut h);
            p.height.hash(&mut h);
            p.surface.revision().hash(&mut h);
        } else {
            id.hash(&mut h);
            name.hash(&mut h);
        }
    };
    let mut fill = |f: &Fill| {
        if let Fill::Pattern { id, name, .. } = f {
            reference(id, name);
        }
    };
    match &layer.content {
        LayerContent::Fill(f) => fill(f),
        LayerContent::Shape(s) => {
            if let Some(f) = &s.fill {
                fill(f);
            }
            if let Some(st) = &s.stroke {
                fill(&st.paint);
            }
        }
        _ => {}
    }
    for e in &layer.effects.items {
        let paint = match e {
            Effect::OuterGlow(g) | Effect::InnerGlow(g) => Some(&g.paint),
            Effect::Stroke(s) => Some(&s.paint),
            _ => None,
        };
        if let Some(FxPaint::Pattern { id, name, .. }) = paint {
            reference(id, name);
        }
        match e {
            Effect::PatternOverlay { id, name, .. } => reference(id, name),
            Effect::BevelEmboss(b) => {
                if let Some(t) = &b.texture {
                    reference(&t.id, &t.name);
                }
            }
            _ => {}
        }
    }
    h.finish()
}

/// Default legacy projection; type requires the pc-text provider in the app (no dependency
/// from the compositor back into a second text engine).
pub fn legacy_projector(doc: &Document, l: &Layer, scale: f64, clip: Rect) -> Result<Surface, ViewError> {
    match &l.content {
        LayerContent::Shape(sh) => {
            if l.vector.appearance.is_some() || !l.vector.corners.is_empty() {
                return Err(ViewError("enhanced shape projection provider required".into()));
            }
            if sh.path.subpaths.iter().flat_map(|p| &p.knots).any(|k| {
                [k.anchor, k.in_ctrl, k.out_ctrl]
                    .iter()
                    .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) * scale > f64::from(i32::MAX / 4))
            }) {
                return Err(ViewError("invalid display geometry".into()));
            }
            if let Some(st) = &sh.stroke {
                let width = f64::from(st.width) * scale;
                let limit = f64::from(i32::MAX / 4);
                if !width.is_finite()
                    || width < 0.0
                    || !st.miter_limit.is_finite()
                    || width * f64::from(st.miter_limit.max(1.0)) > limit
                    || !st.dash_offset.is_finite()
                    || f64::from(st.dash_offset).abs() * width > limit
                    || st.dashes.iter().any(|d| !d.is_finite() || *d < 0.0 || f64::from(*d) * width > limit)
                {
                    return Err(ViewError("invalid display stroke".into()));
                }
            }
            if sh.fill.as_ref().is_some_and(|f| matches!(f, Fill::Pattern { .. }))
                || sh.stroke.as_ref().is_some_and(|s| matches!(s.paint, Fill::Pattern { .. }))
            {
                return Err(ViewError("pattern shape requires enhanced paint projection".into()));
            }
            let mut sh = sh.clone();
            sh.path = sh.path.transform(&Affine::scale(scale));
            sh.cache = None;
            if let Some(st) = &mut sh.stroke {
                st.width *= scale as f32;
                paint(&mut st.paint, scale as f32);
            }
            if let Some(f) = &mut sh.fill {
                paint(f, scale as f32);
            }
            Ok(photocraft_vector::CompiledShape::new(&sh, photocraft_vector::DEFAULT_TOLERANCE, scaled_rect(doc.bounds(), scale)?)
                .render(doc.pixel_format(), clip))
        }
        LayerContent::Text(_) => Err(ViewError("type projection provider required".into())),
        _ => Err(ViewError("projection provider called for non-vector content".into())),
    }
}

#[derive(Default)]
pub struct ProjectionCache {
    entries: HashMap<LayerId, Entry>,
    context: Option<(DocId, Size, photocraft_color::ColorMode, photocraft_color::SampleType)>,
    pub rebuilds: u64,
    pub hits: u64,
}
struct Entry {
    source: Layer,
    scale: u64,
    clip: Rect,
    generation: u64,
    layer: Layer,
}
impl ProjectionCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.context = None;
    }
    pub fn project(&mut self, doc: &Document, request: &ViewRequest, projector: Projector) -> Result<Document, ViewError> {
        if doc.id != request.document {
            return Err(ViewError("stale document view request".into()));
        }
        self.project_at(doc, request.visible, request.scale()?, &request.dependencies, projector)
    }
    pub fn project_at(&mut self, doc: &Document, visible: Rect, scale: f64, deps: &HashMap<LayerId, u64>, projector: Projector) -> Result<Document, ViewError> {
        let context = (doc.id, doc.size, doc.mode, doc.depth);
        if self.context != Some(context) {
            self.entries.clear();
            self.context = Some(context);
        }
        let canvas = scaled_rect(doc.bounds(), scale)?;
        let visible = scaled_rect(visible.intersect(&doc.bounds()), scale)?;
        let _ = tile_rects(visible, VIEW_TILE)?;
        fn reach(layers: &[Layer], scale: f64) -> i32 {
            layers
                .iter()
                .filter(|l| l.visible)
                .map(|l| {
                    let effects = if super::effects::has_effects(l) { f64::from(super::effects::margin(l)) * scale } else { 0.0 };
                    let feather =
                        l.mask.as_ref().map_or(0.0, |m| f64::from(m.feather.max(0.0))) + l.vector_mask.as_ref().map_or(0.0, |m| f64::from(m.feather.max(0.0)));
                    let own = (effects.ceil() as i32).saturating_add((feather * scale * 3.0).ceil() as i32);
                    let children = match &l.content {
                        LayerContent::Group(g) => reach(&g.children, scale),
                        _ => 0,
                    };
                    own.saturating_add(children)
                })
                .max()
                .unwrap_or(0)
        }
        let padding = reach(&doc.layers, scale).saturating_add(2);
        // Padding follows the longest ancestor stack. It is bounded before allocation.
        let clip = visible.inflate(padding);
        if u64::from(clip.width()) * u64::from(clip.height()) > MAX_VIEW_PIXELS {
            return Err(ViewError("effect/mask apron exceeds view budget".into()));
        }
        let mut out = doc.clone();
        out.size = Size::new(canvas.width(), canvas.height());
        out.selection = None;
        out.quick_mask = None;
        out.paths.clear();
        out.work_path = None;
        out.layers =
            photocraft_color::convert::with_cmyk_space(super::cmyk_space(doc).as_ref(), || self.layers(doc, &doc.layers, scale, clip, deps, projector))?;
        for c in &mut out.channels {
            c.surface = sample_surface(&c.surface, scale, clip)?;
        }
        self.entries.retain(|id, _| doc.layer(*id).is_some());
        Ok(out)
    }
    fn layers(
        &mut self,
        doc: &Document,
        layers: &[Layer],
        scale: f64,
        clip: Rect,
        deps: &HashMap<LayerId, u64>,
        projector: Projector,
    ) -> Result<Vec<Layer>, ViewError> {
        layers
            .iter()
            .enumerate()
            .map(|(index, source)| {
                if !source.visible {
                    return Ok(source.clone());
                }
                let generation = dependency_key(doc, source, deps.get(&source.id).copied().unwrap_or(0));
                if let Some(e) = self.entries.get(&source.id)
                    && e.source == *source
                    && e.scale == scale.to_bits()
                    && e.clip == clip
                    && e.generation == generation
                {
                    self.hits += 1;
                    return Ok(e.layer.clone());
                }
                let mut l = source.clone();
                if let LayerContent::Shape(sh) = &source.content
                    && let Some(st) = &sh.stroke
                    && let Some((x0, y0, x1, y1)) = sh.path.control_bounds()
                    && (super::effects::has_effects(source) || layers[index + 1..].iter().take_while(|l| l.clipped).any(|l| l.visible))
                {
                    // The established shape splitter renders its whole compiled bounds for
                    // clipped bases/effects. Until 08 adds a visible-region split API, never
                    // let that downstream path allocate a document-size magnified buffer.
                    let reach = f64::from(st.width) * f64::from(st.miter_limit.max(1.0)) * scale + 4.0;
                    let pixels = ((x1 - x0) * scale + 2.0 * reach) * ((y1 - y0) * scale + 2.0 * reach);
                    if !pixels.is_finite() || pixels > MAX_VIEW_PIXELS as f64 {
                        return Err(ViewError("large stroked shape requires bounded shape-split integration".into()));
                    }
                }
                if let Some(m) = &mut l.mask {
                    m.surface = sample_surface(&m.surface, scale, clip)?;
                    m.feather *= scale as f32;
                }
                if let Some(m) = &mut l.vector_mask {
                    m.path = m.path.transform(&Affine::scale(scale));
                    m.feather *= scale as f32;
                }
                if l.vector.clip.is_some() || l.vector.live.is_some() || l.vector.on_path.is_some() {
                    return Err(ViewError("feature projection provider required".into()));
                }
                scale_effects(&mut l, scale as f32)?;
                if let Some(c) = &mut l.fill_cache {
                    c.surface = sample_surface(&c.surface, scale, clip)?;
                    paint(&mut c.fill, scale as f32);
                }
                l.content = match &source.content {
                    LayerContent::Raster(s) => LayerContent::Raster(sample_surface(s, scale, clip)?),
                    LayerContent::Smart(s) => {
                        let mut s = s.clone();
                        s.cache = s.cache.as_ref().map(|s| sample_surface(s, scale, clip)).transpose()?;
                        LayerContent::Smart(s)
                    }
                    LayerContent::Text(t) => {
                        let mut t = t.clone();
                        t.cache = Some(projector(doc, source, scale, clip)?);
                        t.transform = Affine::scale(scale).mul(&t.transform);
                        LayerContent::Text(t)
                    }
                    LayerContent::Shape(sh) => {
                        let mut sh = sh.clone();
                        sh.cache = Some(projector(doc, source, scale, clip)?);
                        sh.path = sh.path.transform(&Affine::scale(scale));
                        sh.live = None;
                        if let Some(st) = &mut sh.stroke {
                            st.width *= scale as f32;
                            paint(&mut st.paint, scale as f32);
                        }
                        if let Some(f) = &mut sh.fill {
                            paint(f, scale as f32);
                        }
                        LayerContent::Shape(sh)
                    }
                    LayerContent::Fill(f) => {
                        let mut f = f.clone();
                        paint(&mut f, scale as f32);
                        if l.fill_cache.as_ref().is_none_or(|c| c.fill != f) {
                            l.fill_cache = Some(FillCache { fill: f.clone(), surface: fill_surface(doc, source, &f, scale, clip)? });
                        }
                        LayerContent::Fill(f)
                    }
                    LayerContent::Group(g) => {
                        let mut g = g.clone();
                        g.children = self.layers(doc, &g.children, scale, clip, deps, projector)?;
                        if let Some(a) = &mut g.artboard {
                            a.rect = scaled_rect(a.rect, scale)?;
                        }
                        LayerContent::Group(g)
                    }
                    LayerContent::Adjustment(a) => LayerContent::Adjustment(a.clone()),
                };
                // Bake masks over the bounded apron once. Retaining the full vector path here
                // would make the GPU mask planner allocate a document-size scaled surface.
                if let Some(surface) = super::masks::combined_mask(&l, clip) {
                    let linked = l.mask.as_ref().is_none_or(|m| m.linked) && l.vector_mask.as_ref().is_none_or(|m| m.linked);
                    l.mask = Some(LayerMask { surface, enabled: true, linked, density: 1.0, feather: 0.0 });
                    l.vector_mask = None;
                }
                self.rebuilds += 1;
                // Groups contain child state and are rewalked so child generations always propagate.
                if !matches!(source.content, LayerContent::Group(_)) {
                    self.entries.insert(source.id, Entry { source: source.clone(), scale: scale.to_bits(), clip, generation, layer: l.clone() });
                }
                Ok(l)
            })
            .collect()
    }
}

pub fn flatten_scaled(doc: &Document, visible: Rect, scale: f64) -> Result<ViewTiles, ViewError> {
    flatten_scaled_with(doc, visible, scale, legacy_projector)
}
pub fn flatten_scaled_with(doc: &Document, visible: Rect, scale: f64, projector: Projector) -> Result<ViewTiles, ViewError> {
    let projected = ProjectionCache::default().project_at(doc, visible, scale, &HashMap::new(), projector)?;
    let rect = scaled_rect(visible.intersect(&doc.bounds()), scale)?;
    let tiles = tile_rects(rect, VIEW_TILE)?.into_iter().map(|r| super::render(&projected, r)).collect();
    Ok(ViewTiles { scale, rect, tiles })
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::{Color, ColorMode, Path, SampleType, ShapeLayer, Subpath, VectorMask};
    fn document() -> Document {
        let mut doc = Document::new("scaled", Size::new(80, 60), ColorMode::Rgb, SampleType::U8);
        let path = Path::new(vec![Subpath::polygon(&[(7.25, 8.5), (60.0, 8.5), (60.0, 48.0), (7.25, 48.0)])]);
        doc.layers.push(Layer::new("base", LayerContent::Shape(ShapeLayer { path, fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() })));
        let mut raster = Layer::raster("clipped raster", doc.pixel_format());
        raster.clipped = true;
        raster.surface_mut().unwrap().fill_rect(Rect::new(0, 0, 80, 60), &[1.0, 0.0, 0.0, 1.0]);
        raster.vector_mask = Some(VectorMask::new(Path::new(vec![Subpath::polygon(&[(20.0, 0.0), (80.0, 0.0), (80.0, 60.0), (20.0, 60.0)])])));
        doc.layers.push(raster);
        doc
    }
    #[test]
    fn scaled_mixed_clipping_has_independent_analytic_pixels_and_does_not_change_document() {
        let doc = document();
        let before = doc.clone();
        for s in [2.0, 4.0, 8.0] {
            let all = flatten_scaled(&doc, doc.bounds(), s).unwrap().buffer();
            let p = |x: f64, y: f64| all.get((x * s) as i32, (y * s) as i32);
            assert_eq!(p(12.0, 20.0), [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(p(30.0, 20.0), [1.0, 0.0, 0.0, 1.0]);
            assert_eq!(p(65.0, 20.0)[3], 0.0);
            assert_eq!(p(10.0, 4.0)[3], 0.0);
            assert_eq!(doc, before);
        }
    }
    #[test]
    fn scaled_tiles_equal_untiled_mixed_stack_with_shadow_padding() {
        let mut doc = document();
        doc.layers[0].effects.enabled = true;
        doc.layers[0].effects.items.push(Effect::default_drop_shadow());
        let projected = ProjectionCache::default().project_at(&doc, doc.bounds(), 4.0, &HashMap::new(), legacy_projector).unwrap();
        let rect = scaled_rect(doc.bounds(), 4.0).unwrap();
        let full = super::super::render_tiled(&projected, rect, 1000);
        let tiled = flatten_scaled(&doc, doc.bounds(), 4.0).unwrap().buffer();
        for (a, b) in full.px.iter().zip(&tiled.px) {
            for c in 0..4 {
                assert!((a[c] * if c == 3 { 1.0 } else { a[3] } - b[c] * if c == 3 { 1.0 } else { b[3] }).abs() <= 1e-6);
            }
        }
        assert!(full.get(245, 100)[3] > 0.0, "shadow extends outside shape");
    }
    #[test]
    fn scaled_projection_rebuilds_only_changed_layer_and_dependency_generation() {
        let mut doc = document();
        let mut cache = ProjectionCache::default();
        let mut deps = HashMap::new();
        let rect = doc.bounds();
        cache.project_at(&doc, rect, 2.0, &deps, legacy_projector).unwrap();
        let n = cache.rebuilds;
        cache.project_at(&doc, rect, 2.0, &deps, legacy_projector).unwrap();
        assert_eq!(cache.rebuilds, n);
        doc.layers[1].opacity = 0.5;
        cache.project_at(&doc, rect, 2.0, &deps, legacy_projector).unwrap();
        assert_eq!(cache.rebuilds, n + 1);
        deps.insert(doc.layers[0].id, 3);
        cache.project_at(&doc, rect, 2.0, &deps, legacy_projector).unwrap();
        assert_eq!(cache.rebuilds, n + 2);
    }
    #[test]
    fn scaled_pattern_changes_rebuild_only_referencing_layer() {
        use photocraft_doc::pattern::Pattern;
        let mut doc = document();
        for name in ["used", "unrelated"] {
            let mut surface = Surface::new(doc.pixel_format());
            surface.fill_rect(Rect::new(0, 0, 2, 2), &[1.0, 0.0, 0.0, 1.0]);
            doc.patterns.push(Pattern::new(name, surface, 2, 2));
        }
        doc.layers.push(Layer::new(
            "pattern",
            LayerContent::Fill(Fill::Pattern { id: doc.patterns[0].id.clone(), name: "used".into(), scale: 1.0, phase: (0.0, 0.0), angle: 0.0, link: true }),
        ));
        let mut cache = ProjectionCache::default();
        let rect = doc.bounds();
        let deps = HashMap::new();
        cache.project_at(&doc, rect, 2.0, &deps, legacy_projector).unwrap();
        let n = cache.rebuilds;
        doc.patterns[1].surface.write_pixel(0, 0, &[0.0, 1.0, 0.0, 1.0]);
        cache.project_at(&doc, rect, 2.0, &deps, legacy_projector).unwrap();
        assert_eq!(cache.rebuilds, n, "unreferenced pattern preserves every projection");
        doc.patterns[0].surface.write_pixel(0, 0, &[0.0, 0.0, 1.0, 1.0]);
        let out = cache.project_at(&doc, rect, 2.0, &deps, legacy_projector).unwrap();
        assert_eq!(cache.rebuilds, n + 1);
        let LayerContent::Fill(_) = out.layers[2].content else { panic!("pattern fill") };
        let surface = &out.layers[2].fill_cache.as_ref().unwrap().surface;
        // At 2x, device pixel center 0.5 maps to pattern 0.25: bilinear wrapping
        // gives the changed blue texel weight 0.75², with red in its three neighbours.
        for (a, b) in surface.pixel(0, 0).iter().zip([0.4375, 0.0, 0.5625, 1.0]) {
            assert!((a - b).abs() <= 1.0 / 255.0);
        }
    }
    #[test]
    fn scaled_large_document_vector_mask_is_limited_to_visible_apron() {
        let mut doc = Document::new("large masked view", Size::new(6000, 4000), ColorMode::Rgb, SampleType::U8);
        let mut layer = Layer::raster("default red", doc.pixel_format());
        layer.content = LayerContent::Raster(Surface::with_default(doc.pixel_format(), &[1.0, 0.0, 0.0, 1.0]));
        layer.vector_mask = Some(VectorMask::new(Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (6000.0, 0.0), (6000.0, 4000.0), (0.0, 4000.0)])])));
        doc.layers.push(layer);
        let visible = Rect::new(3000, 2000, 3010, 2010);
        let projected = ProjectionCache::default().project_at(&doc, visible, 8.0, &HashMap::new(), legacy_projector).unwrap();
        let mask = projected.layers[0].mask.as_ref().unwrap();
        assert!(mask.surface.tile_count() < 8);
        assert!(projected.layers[0].vector_mask.is_none());
        let buffer = super::super::render(&projected, scaled_rect(visible, 8.0).unwrap());
        assert!(buffer.px.iter().all(|p| *p == [1.0, 0.0, 0.0, 1.0]));
        assert!(doc.layers[0].vector_mask.is_some());
    }

    #[test]
    fn scaled_large_stroked_shape_refuses_unbounded_legacy_split() {
        let mut doc = document();
        let LayerContent::Shape(sh) = &mut doc.layers[0].content else { panic!("shape") };
        sh.path = Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (6000.0, 0.0), (6000.0, 4000.0), (0.0, 4000.0)])]);
        sh.stroke = Some(photocraft_doc::ShapeStroke { width: 1.0, ..Default::default() });
        let result = flatten_scaled(&doc, Rect::new(10, 10, 20, 20), 8.0);
        assert!(result.is_err(), "a tiny viewport must not authorize whole-path split allocation");
    }

    #[test]
    fn scaled_masked_gradient_keeps_document_frame_when_visible_rect_moves() {
        let mut doc = Document::new("gradient pan", Size::new(120, 80), ColorMode::Rgb, SampleType::U8);
        let mut l = Layer::new(
            "gradient",
            LayerContent::Fill(Fill::gradient(vec![(0.0, Color::BLACK), (1.0, Color::WHITE)], 0.0, 1.0, photocraft_doc::GradientStyle::Linear, false)),
        );
        l.vector_mask = Some(VectorMask::new(Path::new(vec![Subpath::polygon(&[(10.0, 10.0), (110.0, 10.0), (110.0, 70.0), (10.0, 70.0)])])));
        doc.layers.push(l);
        let full = flatten_scaled(&doc, doc.bounds(), 4.0).unwrap().buffer();
        for visible in [Rect::new(20, 20, 50, 60), Rect::new(50, 20, 90, 60)] {
            let part = flatten_scaled(&doc, visible, 4.0).unwrap().buffer();
            for y in part.rect.y0..part.rect.y1 {
                for x in part.rect.x0..part.rect.x1 {
                    let a = full.get(x, y);
                    let b = part.get(x, y);
                    for c in 0..4 {
                        assert!((a[c] - b[c]).abs() < 1e-6, "{x},{y}: {a:?} {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn scaled_hidpi_bucket_never_undersamples_and_validates_requests() {
        let doc = document();
        let mut r = ViewRequest {
            document: doc.id,
            revision: 1,
            view_key: 2,
            logical_zoom: 1.1,
            pixels_per_point: 2.0,
            visible: doc.bounds(),
            display_generation: 1,
            dependencies: HashMap::new(),
        };
        assert!(r.scale().unwrap() >= 2.2);
        assert!(r.scale().unwrap() < 2.2 * 2.0f64.powf(0.25));
        r.pixels_per_point = f64::NAN;
        assert!(r.scale().is_err());
        assert!(scaled_rect(Rect::new(0, 0, i32::MAX, 1), 8.0).is_err());
        assert!(tile_rects(Rect::new(0, 0, 100000, 100000), 256).is_err());
        let mut oversized = document();
        let mut mask = photocraft_doc::LayerMask::reveal_all();
        mask.feather = f32::MAX;
        oversized.layers[0].mask = Some(mask);
        oversized.layers[0].effects.enabled = true;
        oversized.layers[0].effects.items.push(Effect::default_drop_shadow());
        assert!(flatten_scaled(&oversized, Rect::new(0, 0, 1, 1), 8.0).is_err(), "excessive padding must fail before allocation or integer overflow");
    }
    #[test]
    fn scaled_raster_sampling_preserves_hdr_defaults_and_negative_pixels() {
        let mut source =
            Surface::with_default(photocraft_color::PixelFormat { sample: SampleType::F32, ..photocraft_color::PixelFormat::RGBA8 }, &[2.0, 0.0, 0.0, 0.5]);
        source.write_pixel(-1, -1, &[0.0, 3.0, 0.0, 1.0]);
        let scaled = sample_surface(&source, 4.0, Rect::new(-8, -8, 8, 8)).unwrap();
        assert_eq!(scaled.pixel(-2, -2), vec![0.0, 3.0, 0.0, 1.0]);
        assert_eq!(scaled.pixel(1, 1), vec![2.0, 0.0, 0.0, 0.5]);
    }
}
