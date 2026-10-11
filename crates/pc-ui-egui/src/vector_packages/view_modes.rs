//! Session-only vector views. Original GPL-3.0-or-later code, following Illustrator
//! viewing-artwork and Affinity GetStarted/view. X-ray fill alpha is a Local Image choice:
//! official Affinity documentation specifies reduced opacity, but no numeric value.
use egui::{Color32, Context, Key, Modifiers, Painter, Response, Stroke, Ui};
use photocraft_doc::{Document, Knot, Layer, LayerContent, LayerId, Path, Subpath};
use photocraft_engine::{
    EngineError, Result,
    vector_packages::{
        corners,
        isolation::{DIM_ALPHA, Scope},
        path_text, symbols,
    },
};
use photocraft_geom::{Affine, Point, Rect};
use photocraft_vector::outline_view::Outline;

pub const XRAY_FILL_ALPHA: f32 = 0.20;
fn error(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
pub use photocraft_engine::vector_packages::isolation::{ViewMode as Mode, ViewState as State};
/// Call BEFORE Pixel's proof-colors accelerator. 46 supplies persona/keymap enablement;
/// text editors and modal dialogs pass enabled=false so typing cannot change canvas modes.
pub fn shortcuts(ctx: &Context, state: &mut State, enabled: bool) -> bool {
    if !enabled || ctx.egui_wants_keyboard_input() {
        return false;
    }
    let pixel = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::ALT, Key::Y));
    if pixel {
        state.toggle_pixels();
        return true;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Y)) {
        state.toggle_outline();
        return true;
    }
    false
}
/// Return true to consume the eye click BEFORE ordinary visibility handling.
pub fn layer_eye(ui: &Ui, response: &Response, state: &mut State, doc: &Document, id: LayerId) -> Result<bool> {
    if response.clicked() && ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
        state.toggle_layer(doc, id)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
pub fn menu(ui: &mut Ui, state: &mut State, text: impl Fn(&str) -> String) {
    for (mode, label) in [(Mode::Vector, "Vector View"), (Mode::Outline, "Outline"), (Mode::Xray, "X-ray"), (Mode::PixelPreview, "Pixel Preview")] {
        if ui.selectable_label(state.mode == mode, text(label)).clicked() {
            state.set_mode(mode);
            ui.close();
        }
    }
}
/// Provider uses the same feature projection as 08/09/export. Every returned path is in
/// document coordinates. Raster boxes are added here; glyphs come exclusively from pc-text.
pub trait Geometry {
    fn paths(&self, doc: &Document, layer: &Layer) -> Result<Vec<Path>>;
    /// Feature providers override this for live/symbol geometry and reuse 08's fill paints.
    fn xray_layer(&self, doc: &Document, layer: &Layer) -> Result<Layer> {
        native_xray(doc, layer)
    }
}
fn native_xray(doc: &Document, layer: &Layer) -> Result<Layer> {
    let mut shown = layer.clone();
    shown.effects = Default::default();
    if layer.vector.live.is_some() {
        return Err(error("live X-ray fills require the shared feature projection"));
    }
    if let LayerContent::Symbol(_) = &layer.content {
        let expanded = symbols::expansion::expand(doc, std::slice::from_ref(layer)).map_err(|e| error(e.to_string()))?;
        let mut expanded = symbols::prepare_expansion(doc, expanded)?;
        let mut root = expanded.pop().ok_or_else(|| error("symbol X-ray projection is empty"))?;
        fn fade(doc: &Document, layer: &mut Layer) -> Result<()> {
            *layer = native_xray(doc, layer)?;
            if let Some(children) = layer.children_mut() {
                for child in children {
                    fade(doc, child)?;
                }
            }
            Ok(())
        }
        fade(doc, &mut root)?;
        return Ok(root);
    }
    if let LayerContent::Text(text) = &mut shown.content {
        if let Some(path) = &shown.vector.on_path {
            path_text::refresh(doc, text, path)?;
        } else {
            let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine unavailable"))?;
            let layout = photocraft_engine::vector_integration::text_layout(&mut engine, doc, text, &layer.vector)?;
            let warp = photocraft_text::render::layout_warp(&layout, text.warp.as_ref());
            let bounds = photocraft_text::render::ink_rect_warped(&layout, &text.transform, warp.as_ref());
            if u64::from(bounds.width()) * u64::from(bounds.height()) > 16_777_216 {
                return Err(error("X-ray text exceeds visible-band budget"));
            }
            text.cache = Some(photocraft_text::render::rasterize_warped(&layout, &text.transform, doc.pixel_format(), text.antialias, warp.as_ref()).surface);
        }
    }
    if let LayerContent::Shape(shape) = &layer.content {
        let mut projected = corners::project_shape(shape, &layer.vector.corners)?;
        projected.stroke = None;
        let mut appearance = layer.vector.appearance.clone();
        if let Some(a) = &mut appearance {
            a.items.retain(|i| matches!(i, photocraft_doc::vector_design::appearance::AppearanceItem::Fill(_)));
        }
        let compiled = photocraft_vector::appearance::CompiledAppearance::new(&projected, appearance.as_ref(), &doc.patterns, doc.bounds()).map_err(error)?;
        if compiled.cache_bytes(doc.pixel_format()) > 64 << 20 {
            return Err(error("X-ray fill cache exceeds budget"));
        }
        projected.cache = Some(compiled.render(doc.pixel_format(), doc.bounds()).map_err(error)?);
        shown.vector.corners.clear();
        shown.vector.appearance = appearance;
        shown.content = LayerContent::Shape(projected);
    }
    if !shown.is_group() {
        shown.opacity *= XRAY_FILL_ALPHA;
    }
    Ok(shown)
}
pub struct NativeGeometry;
impl Geometry for NativeGeometry {
    fn paths(&self, doc: &Document, layer: &Layer) -> Result<Vec<Path>> {
        if layer.vector.live.is_some() {
            return Err(error("live outline geometry requires the shared feature projection"));
        }
        let mut paths = match &layer.content {
            LayerContent::Shape(shape) => vec![corners::project_shape(shape, &layer.vector.corners)?.path],
            LayerContent::Text(text) => {
                if !doc.resolution_dpi.is_finite() || !(1.0..=100_000.).contains(&doc.resolution_dpi) {
                    return Err(error("invalid outline text DPI"));
                }
                let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine unavailable"))?;
                let glyphs = if let Some(path) = &layer.vector.on_path {
                    let layout = path_text::project(&mut engine, doc, text, path)?;
                    path_text::on_path::validated_outlines(&layout, &text.transform)
                        .map_err(|e| error(e.to_string()))?
                        .into_iter()
                        .map(|p| p.elements)
                        .collect()
                } else {
                    let layout = photocraft_engine::vector_integration::text_layout(&mut engine, doc, text, &layer.vector)?;
                    let warp = photocraft_text::render::layout_warp(&layout, text.warp.as_ref());
                    photocraft_text::render::outlines(&layout, &text.transform, warp.as_ref())
                };
                glyphs.into_iter().map(glyph_path).collect()
            }
            LayerContent::Raster(s) => vec![photocraft_vector::outline_view::raster_box(s.content_bounds())],
            LayerContent::Smart(s) => vec![photocraft_vector::outline_view::raster_box(s.cache.as_ref().map_or(Rect::EMPTY, |s| s.content_bounds()))],
            LayerContent::Symbol(_) => {
                let expanded = symbols::expansion::expand(doc, std::slice::from_ref(layer)).map_err(|e| error(e.to_string()))?;
                let mut paths = Vec::new();
                for (_, _, l) in walk(&expanded.layers) {
                    if l.is_group() {
                        continue;
                    }
                    let transform = expanded.transforms.get(&l.id.0).copied().unwrap_or(Affine::IDENTITY);
                    paths.extend(self.paths(doc, l)?.into_iter().map(|p| p.transform(&transform)));
                }
                paths
            }
            LayerContent::Group(_) => Vec::new(),
            LayerContent::Fill(_) => vec![photocraft_vector::outline_view::raster_box(doc.bounds())],
            LayerContent::Adjustment(_) => Vec::new(),
        };
        if let Some(clip) = &layer.vector.clip {
            paths.push(clip.clone());
        }
        if let Some(mask) = &layer.vector_mask
            && mask.enabled
        {
            paths.push(mask.path.clone());
        }
        Ok(paths)
    }
}
fn walk(layers: &[Layer]) -> Vec<(usize, usize, &Layer)> {
    fn add<'a>(layers: &'a [Layer], depth: usize, out: &mut Vec<(usize, usize, &'a Layer)>) {
        for (i, l) in layers.iter().enumerate() {
            out.push((depth, i, l));
            if let Some(c) = l.children() {
                add(c, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    add(layers, 0, &mut out);
    out
}
/// Cubic conversion of pc-text's existing glyph outlines, including quadratic fonts.
fn glyph_path(elements: Vec<photocraft_text::render::PathEl>) -> Path {
    use photocraft_text::render::PathEl;
    let mut parts = Vec::new();
    let mut sub = Subpath::default();
    let pt = |p: [f64; 2]| Point::new(p[0], p[1]);
    for e in elements {
        match e {
            PathEl::MoveTo(p) => {
                if !sub.knots.is_empty() {
                    parts.push(std::mem::take(&mut sub));
                }
                sub.knots.push(Knot::corner(p[0], p[1]));
            }
            PathEl::LineTo(p) => sub.knots.push(Knot::corner(p[0], p[1])),
            PathEl::CurveTo(a, b, p) => {
                if let Some(last) = sub.knots.last_mut() {
                    last.out_ctrl = pt(a);
                }
                let mut k = Knot::corner(p[0], p[1]);
                k.in_ctrl = pt(b);
                sub.knots.push(k);
            }
            PathEl::QuadTo(c, p) => {
                if let Some(last) = sub.knots.last_mut() {
                    last.out_ctrl = Point::new(last.anchor.x + (c[0] - last.anchor.x) * 2. / 3., last.anchor.y + (c[1] - last.anchor.y) * 2. / 3.);
                }
                let mut k = Knot::corner(p[0], p[1]);
                k.in_ctrl = Point::new(p[0] + (c[0] - p[0]) * 2. / 3., p[1] + (c[1] - p[1]) * 2. / 3.);
                sub.knots.push(k);
            }
            PathEl::Close => {
                sub.closed = true; // Fonts may explicitly repeat the start anchor with closing controls.
                if sub.knots.len() > 1
                    && sub.knots.first().map(|k| k.anchor) == sub.knots.last().map(|k| k.anchor)
                    && let Some(last) = sub.knots.pop()
                    && let Some(first) = sub.knots.first_mut()
                {
                    first.in_ctrl = last.in_ctrl;
                }
                parts.push(std::mem::take(&mut sub));
            }
        }
    }
    if !sub.knots.is_empty() {
        parts.push(sub);
    }
    Path::new(parts)
}
/// An overlay sits above the entire base composite, so hidden paths remain exposed even
/// when another layer's fill overlaps them. Coordinates and width are device pixels.
pub struct Entry {
    pub layer: LayerId,
    pub geometry: Vec<Outline>,
    pub alpha: f32,
}
pub struct Frame {
    pub base: Option<Document>,
    pub outlines: Vec<Entry>,
    pub pixel_preview: bool,
    pub view_key: u64,
}
/// Prepare paint policy before 09's scaled projection. The base remains in document space;
/// overlays are already in exact device space, never bucket-scaled or persisted.
pub fn prepare(doc: &Document, state: &State, scope: Scope, document_to_device: Affine, device_clip: Rect, geometry: &dyn Geometry) -> Result<Frame> {
    if scope.document != doc.id || state.document().is_some_and(|id| id != doc.id) {
        return Err(error("view document changed"));
    }
    let mut base = if state.mode == Mode::Outline { None } else { Some(doc.clone()) };
    let mut outlines = Vec::new();
    let mut knots = 0usize;
    for (path, _, layer) in doc.walk() {
        if (1..=path.len()).any(|n| doc.layer_at(&path[..n]).is_none_or(|l| !l.visible))
            || (1..path.len()).any(|n| doc.layer_at(&path[..n]).is_some_and(|l| l.vector.live.is_some() || matches!(l.content, LayerContent::Symbol(_))))
        {
            continue;
        }
        if state.outlined(doc, layer.id) {
            let paths = geometry.paths(doc, layer)?;
            knots = knots.saturating_add(paths.iter().flat_map(|p| &p.subpaths).map(|s| s.knots.len()).sum::<usize>());
            if knots > 1_048_576 {
                return Err(error("outline scene exceeds knot budget"));
            }
            let compiled = paths.iter().map(|p| Outline::new(p, document_to_device, device_clip).map_err(error)).collect::<Result<Vec<_>>>()?;
            outlines.push(Entry { layer: layer.id, geometry: compiled, alpha: if scope.dimmed(doc, layer.id) { DIM_ALPHA } else { 1. } });
            if let Some(base) = &mut base {
                let shown = base.layer_mut(layer.id).ok_or(EngineError::NoLayer(layer.id))?;
                if state.mode != Mode::Xray || state.layer_overridden(doc, layer.id) {
                    shown.opacity = 0.;
                } else {
                    *shown = geometry.xray_layer(doc, layer)?;
                }
            }
        }
    }
    if let Some(base) = &mut base {
        // Fade maximal outside roots only, so nested groups do not multiply the dim factor.
        let outside: Vec<_> = doc
            .walk()
            .into_iter()
            .filter(|(p, _, l)| scope.dimmed(doc, l.id) && (p.len() == 1 || doc.layer_at(&p[..p.len() - 1]).is_none_or(|parent| !scope.dimmed(doc, parent.id))))
            .map(|(_, _, l)| l.id)
            .collect();
        for id in outside {
            if let Some(l) = base.layer_mut(id) {
                l.opacity *= DIM_ALPHA;
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    state.generation.hash(&mut h);
    (state.mode as u8).hash(&mut h);
    scope.target.hash_key().hash(&mut h);
    let mut layers: Vec<_> = state.layer_outlines.iter().map(|id| id.0).collect();
    layers.sort_unstable();
    layers.hash(&mut h);
    Ok(Frame { base, outlines, pixel_preview: state.mode == Mode::PixelPreview, view_key: h.finish() })
}
// Stable scope fingerprint without depending on document serialization.
trait TargetKey {
    fn hash_key(self) -> (u64, u8);
}
impl TargetKey for Option<photocraft_engine::vector_packages::isolation::Target> {
    fn hash_key(self) -> (u64, u8) {
        use photocraft_engine::vector_packages::isolation::Target;
        match self {
            None => (0, 0),
            Some(Target::Group { layer }) => (layer.0, 1),
            Some(Target::Compound { layer }) => (layer.0, 2),
            Some(Target::Symbol { layer, .. }) => (layer.0, 3),
        }
    }
}
impl Frame {
    pub fn hit(&self, doc: &Document, scope: Scope, point: Point, tolerance: f64) -> Result<Option<LayerId>> {
        for e in self.outlines.iter().rev() {
            if scope.selectable(doc, e.layer)
                && e.geometry.iter().map(|g| g.hit(point, tolerance).map_err(error)).collect::<Result<Vec<_>>>()?.iter().any(|h| *h)
            {
                return Ok(Some(e.layer));
            }
        }
        Ok(None)
    }
    /// Exact sampling of the established 1x CPU cache. The renderer uses nearest document
    /// pixels for Pixel Preview even on HiDPI, without rebuilding document caches.
    pub fn pixel_buffer(&self, doc: &Document, device_rect: Rect, document_to_device: Affine) -> Result<photocraft_compose::Buffer> {
        if u64::from(device_rect.width()) * u64::from(device_rect.height()) > 16_777_216 {
            return Err(error("pixel viewport exceeds budget"));
        }
        let inverse = document_to_device.inverse().filter(|a| a.m.iter().all(|v| v.is_finite())).ok_or_else(|| error("invalid pixel preview transform"))?;
        // Visible source rectangle only; do not flatten a potentially huge whole document.
        let corners = [(device_rect.x0, device_rect.y0), (device_rect.x1, device_rect.y0), (device_rect.x0, device_rect.y1), (device_rect.x1, device_rect.y1)]
            .map(|(x, y)| inverse.apply(Point::new(f64::from(x), f64::from(y))));
        if corners.iter().any(|p| p.x.abs().max(p.y.abs()) > 100_000_000.) {
            return Err(error("pixel preview coordinates exceed budget"));
        }
        let r = Rect::new(
            corners.iter().map(|p| p.x.floor() as i32).min().unwrap_or(0),
            corners.iter().map(|p| p.y.floor() as i32).min().unwrap_or(0),
            corners.iter().map(|p| p.x.ceil() as i32).max().unwrap_or(0),
            corners.iter().map(|p| p.y.ceil() as i32).max().unwrap_or(0),
        );
        if u64::from(r.width()) * u64::from(r.height()) > 16_777_216 {
            return Err(error("pixel source viewport exceeds budget"));
        }
        let source = photocraft_compose::render(self.base.as_ref().unwrap_or(doc), r);
        let mut output = photocraft_compose::Buffer::transparent(device_rect);
        for y in device_rect.y0..device_rect.y1 {
            for x in device_rect.x0..device_rect.x1 {
                let p = inverse.apply(Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5));
                let (sx, sy) = (p.x.floor() as i32, p.y.floor() as i32);
                if r.contains(sx, sy) {
                    output.px[((y - device_rect.y0) as usize) * device_rect.width() as usize + (x - device_rect.x0) as usize] =
                        source.px[((sy - r.y0) as usize) * r.width() as usize + (sx - r.x0) as usize];
                }
            }
        }
        Ok(output)
    }
    /// The callback maps DEVICE coordinates to egui points, including canvas translation.
    /// A one-device-pixel line therefore uses 1 / pixels_per_point logical points.
    pub fn overlay_buffer(&self, clip: Rect) -> Result<photocraft_compose::Buffer> {
        if u64::from(clip.width()) * u64::from(clip.height()) > 16_777_216 {
            return Err(error("outline viewport exceeds budget"));
        }
        let mut out = photocraft_compose::Buffer::transparent(clip);
        for e in &self.outlines {
            for g in &e.geometry {
                let bounds = g.bounds().unwrap_or(Rect::EMPTY).intersect(&clip);
                let pixels = g.render(photocraft_color::PixelFormat::RGBA8, bounds).map_err(error)?;
                for y in bounds.y0..bounds.y1 {
                    for x in bounds.x0..bounds.x1 {
                        let a = pixels.pixel(x, y)[3] * e.alpha;
                        let p = &mut out.px[((y - clip.y0) as usize) * clip.width() as usize + (x - clip.x0) as usize];
                        p[3] = a + p[3] * (1. - a);
                    }
                }
            }
        }
        Ok(out)
    }
    pub fn paint(&self, painter: &Painter, pixels_per_point: f32, to_screen: impl Fn(Point) -> egui::Pos2) -> Result<()> {
        if !pixels_per_point.is_finite() || !(0.1..=16.).contains(&pixels_per_point) {
            return Err(error("invalid outline display density"));
        }
        for e in &self.outlines {
            for g in &e.geometry {
                for poly in photocraft_vector::flatten_path(&g.path, photocraft_vector::DEFAULT_TOLERANCE) {
                    let points = poly.pts.into_iter().map(|(x, y)| to_screen(Point::new(x, y))).collect::<Vec<_>>();
                    let stroke = Stroke::new(1. / pixels_per_point, Color32::from_black_alpha((255. * e.alpha).round() as u8));
                    if poly.closed {
                        painter.add(egui::Shape::closed_line(points, stroke));
                    } else {
                        painter.add(egui::Shape::line(points, stroke));
                    }
                }
            }
        }
        Ok(())
    }
}
/// 09 supplies all shared projection dependency generations (fonts, masters, paints).
/// Source identity also keys detached previews that deliberately retain the document revision.
#[derive(Default)]
pub struct FrameCache {
    source: Option<std::sync::Arc<Document>>,
    key: Option<Vec<u64>>,
    frame: Option<std::sync::Arc<Frame>>,
}
impl FrameCache {
    pub fn clear(&mut self) {
        self.key = None;
        self.source = None;
        self.frame = None;
    }
    pub fn prepare(
        &mut self,
        doc: &std::sync::Arc<Document>,
        revision: u64,
        dependency_generation: u64,
        state: &State,
        scope: Scope,
        transform: Affine,
        clip: Rect,
        geometry: &dyn Geometry,
    ) -> Result<std::sync::Arc<Frame>> {
        let (root, kind) = scope.target.hash_key();
        let mut key = vec![
            doc.id.0,
            revision,
            dependency_generation,
            std::sync::Arc::as_ptr(doc) as usize as u64,
            state.generation,
            state.mode as u64,
            root,
            u64::from(kind),
        ];
        key.extend(transform.m.map(f64::to_bits));
        key.extend([clip.x0, clip.y0, clip.x1, clip.y1].map(|i| i as u64));
        let mut ids: Vec<_> = state.layer_outlines.iter().map(|id| id.0).collect();
        ids.sort_unstable();
        key.extend(ids);
        if self.key.as_ref() == Some(&key)
            && let Some(frame) = &self.frame
        {
            return Ok(frame.clone());
        }
        let frame = std::sync::Arc::new(prepare(doc, state, scope, transform, clip, geometry)?);
        self.source = Some(doc.clone());
        self.key = Some(key);
        self.frame = Some(frame.clone());
        Ok(frame)
    }
}
#[cfg(test)]
#[path = "view_modes/app_gpu.rs"]
mod app_gpu;
#[cfg(test)]
#[path = "view_modes/tests.rs"]
mod tests;
