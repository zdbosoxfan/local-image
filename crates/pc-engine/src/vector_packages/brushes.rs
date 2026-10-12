//! Vector Brush command and gesture boundary. Geometry is adapted from pinned VectorCraft;
//! the bridge below compiles the owned source until package 00 adds the crate dependency.
//! See docs/wip/vector-tasks/integration/17.md. No shared registries are replaced.
use crate::vector_packages::pencil::fitting::{self, FitOptions, Sample};
use crate::{EngineError, Result, Session, commands::CommandSpec, vector_api};
use geometry::{Brush, Resource};
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
use photocraft_doc::{DocId, Document, Fill, Layer, LayerContent, LayerId, ShapeLayer, ShapeStroke};
use photocraft_geom::Rect;
pub use photocraft_vecfx::brush as geometry;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

pub const MAX_CACHE_PIXELS: u64 = 16_777_216;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub brush: Brush,
    pub seed: u64,
    pub width: f32,
    pub pressure: bool,
    pub tolerance: f64,
}
impl Default for Options {
    fn default() -> Self {
        Self { brush: Brush::Calligraphic(Default::default()), seed: 0, width: 8., pressure: true, tolerance: 1. }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        self.brush.validate().map_err(error)?;
        if !self.width.is_finite() || !(0.1..=1000.).contains(&self.width) || !self.tolerance.is_finite() || !(0.05..=100.).contains(&self.tolerance) {
            return Err(EngineError::Other("invalid brush width or tolerance".into()));
        }
        Ok(())
    }
}
fn error(e: geometry::Error) -> EngineError {
    EngineError::Other(e.to_string())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub document: DocId,
    pub revision: u64,
    pub above: Option<LayerId>,
    pub layer: LayerId,
    pub options: Options,
    pub color: [f32; 4],
}
impl Capture {
    pub fn begin(session: &Session, options: Options) -> Result<Self> {
        let st = session.active().ok_or(EngineError::NoDocument)?;
        let capture =
            Self { document: st.doc.id, revision: st.revision, above: st.active_layer, layer: LayerId::fresh(), options, color: session.tools.foreground };
        capture.validate(session)?;
        Ok(capture)
    }
    pub fn validate(&self, session: &Session) -> Result<()> {
        self.options.validate()?;
        let st = session.active().ok_or(EngineError::NoDocument)?;
        if st.doc.id != self.document || st.revision != self.revision {
            return Err(EngineError::Other("brush gesture is stale".into()));
        }
        if self.layer.0 == 0
            || self.layer.0 > i64::MAX as u64
            || st.doc.layer(self.layer).is_some()
            || self.color.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(EngineError::Other("invalid brush layer or color".into()));
        }
        if let Some(id) = self.above {
            let path = st.doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
            for n in 1..path.len() {
                if st.doc.layer_at(&path[..n]).is_some_and(|l| l.locks.all) {
                    return Err(EngineError::Other("brush insertion parent is locked".into()));
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Prepared {
    pub capture: Capture,
    pub document: Arc<Document>,
    pub stroke: StrokeItem,
    layer: Layer,
    samples: Vec<Sample>,
}
impl Prepared {
    pub fn layer(&self) -> &Layer {
        &self.layer
    }
    pub fn commit(self, session: &mut Session) -> Result<Value> {
        self.capture.validate(session)?;
        let samples: Vec<_> = self.samples.iter().map(|s| json!({"x":s.point.x,"y":s.point.y,"pressure":s.pressure})).collect();
        session.execute("vector.brush.commit", json!({"capture":self.capture,"samples":samples}))
    }
}
pub fn prepare(session: &Session, capture: &Capture, samples: &[Sample]) -> Result<Prepared> {
    capture.validate(session)?;
    if !(2..=geometry::MAX_INPUT).contains(&samples.len()) {
        return Err(EngineError::Other("brush needs 2..8192 samples".into()));
    }
    for sample in samples {
        sample.validate().map_err(EngineError::Other)?;
    }
    // Use the established pencil fitter/stylus contract and V1 optimal fitter.
    let path = fitting::fit_with(samples, FitOptions { tolerance: capture.options.tolerance, smoothness: 0.5, closed: false }, |points, tol| {
        photocraft_pathops::fit_path(points, tol).map_err(|e| e.to_string())
    })
    .map_err(EngineError::Other)?;
    let paint = Fill::Solid(photocraft_doc::Color::rgba(capture.color[0], capture.color[1], capture.color[2], capture.color[3]));
    let mut stroke = StrokeItem {
        base: ShapeStroke { width: capture.options.width, paint, ..Default::default() },
        brush: Some(Resource { brush: capture.options.brush.clone(), seed: capture.options.seed }.reference().map_err(error)?),
        ..Default::default()
    };
    if capture.options.pressure {
        let samples: Vec<_> = samples.iter().map(|s| (s.point, s.pressure)).collect();
        stroke.profile = Some(geometry::pressure_profile(&samples, f64::from(capture.options.width)).map_err(error)?);
    }
    let st = session.active().ok_or(EngineError::NoDocument)?;
    let mut layer = Layer::new("Vector Brush", LayerContent::Shape(ShapeLayer { path, stroke: Some(stroke.base.clone()), ..Default::default() }));
    layer.id = capture.layer;
    layer.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Stroke(stroke.clone())] });
    refresh(&st.doc, &mut layer)?;
    let mut document = (*st.doc).clone();
    document.insert_above(capture.above, layer.clone());
    Ok(Prepared { capture: capture.clone(), document: Arc::new(document), stroke, layer, samples: samples.to_vec() })
}
/// Bake supported paints into ordinary shape layers in bottom-to-top paint order.
/// Brush coverage matches BrushProjection. Paint-coordinate bakes and missing sibling
/// projection providers are explicit errors rather than discarded metadata.
pub fn projection(layer: &Layer) -> Result<Vec<Layer>> {
    let LayerContent::Shape(shape) = &layer.content else {
        return Err(EngineError::Other("brush requires shape layers".into()));
    };
    if layer.vector.appearance.as_ref().is_some_and(|a| a.items.len() > 256) {
        return Err(error(geometry::Error::Budget));
    }
    let appearance = layer.vector.appearance.clone().unwrap_or_else(|| {
        let mut items = Vec::new();
        if let Some(p) = &shape.fill {
            items.push(AppearanceItem::Fill(FillItem::new(p.clone())));
        }
        if let Some(s) = &shape.stroke {
            items.push(AppearanceItem::Stroke(StrokeItem { base: s.clone(), ..Default::default() }));
        }
        Appearance { items }
    });
    if layer.vector.clip.is_some() || !layer.vector.corners.is_empty() || layer.vector.live_shape.is_some() {
        return Err(EngineError::Other("brush projection requires the integrated corner/clip renderer".into()));
    }
    let mut out = Vec::new();
    let mut nodes = 0;
    for item in appearance.items {
        let (paths, paint, opacity, blend) = match item {
            AppearanceItem::Fill(f) if f.visible => (vec![shape.path.clone()], f.paint, f.opacity, f.blend),
            AppearanceItem::Stroke(s) if s.visible => {
                if s.arrow_scale.iter().any(|v| !v.is_finite() || !(0.0..=1000.0).contains(v)) {
                    return Err(EngineError::Other("invalid brush arrow scale".into()));
                }
                if s.start_arrow.is_some() || s.end_arrow.is_some() || s.dash_align || s.gradient_mode != Default::default() {
                    return Err(EngineError::Other("brush stroke needs the integrated arrow/gradient renderer".into()));
                }
                let paths = if s.brush.is_some() {
                    vec![geometry::outline(&shape.path, &s).map_err(error)?]
                } else if s.profile.is_some() {
                    return Err(EngineError::Other("unbrushed variable width requires package 11".into()));
                } else {
                    vec![photocraft_pathops::outline_stroke(&shape.path, &s.base, 0.05).map_err(|e| EngineError::Other(e.to_string()))?]
                };
                (paths, s.base.paint, s.base.opacity, s.blend)
            }
            _ => continue,
        };
        let Fill::Solid(color) = &paint else {
            return Err(EngineError::Other("expanding brush gradient/pattern paint requires a paint-coordinate bake".into()));
        };
        if !opacity.is_finite()
            || !(0.0..=1.0).contains(&opacity)
            || !color.alpha.is_finite()
            || !(0.0..=1.0).contains(&color.alpha)
            || color.c.iter().any(|v| !v.is_finite() || v.abs() > 10000.)
        {
            return Err(EngineError::Other("invalid brush paint".into()));
        }
        for path in paths {
            nodes += path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
            if out.len() >= geometry::MAX_PIECES || nodes > geometry::MAX_VERTICES {
                return Err(error(geometry::Error::Budget));
            }
            let mut l = Layer::new("Brush geometry", LayerContent::Shape(ShapeLayer { path, fill: Some(paint.clone()), ..Default::default() }));
            l.opacity = opacity;
            l.blend = blend;
            out.push(l);
        }
    }
    Ok(out)
}
fn cache_bounds(doc: &Document, layers: &[Layer]) -> Result<Rect> {
    let mut bounds = Rect::EMPTY;
    let mut area = 0u64;
    for layer in layers {
        let LayerContent::Shape(shape) = &layer.content else {
            continue;
        };
        if let Some((x0, y0, x1, y1)) = shape.path.control_bounds() {
            let b = Rect::new((x0 - 1.).floor() as i32, (y0 - 1.).floor() as i32, (x1 + 1.).ceil() as i32, (y1 + 1.).ceil() as i32).intersect(&doc.bounds());
            if !b.is_empty() {
                area = area.saturating_add(b.size().area());
                bounds = if bounds.is_empty() { b } else { Rect::new(bounds.x0.min(b.x0), bounds.y0.min(b.y0), bounds.x1.max(b.x1), bounds.y1.max(b.y1)) };
            }
        }
    }
    if area > MAX_CACHE_PIXELS || bounds.size().area() > MAX_CACHE_PIXELS {
        return Err(EngineError::Other("brush render cache exceeds pixel budget".into()));
    }
    Ok(bounds)
}
/// Actual package 08 hook; pc-vector never depends back on pathops/vecfx.
pub struct BrushProjection;
impl photocraft_vector::appearance::StrokeProjection for BrushProjection {
    fn outline(&self, path: &photocraft_doc::Path, stroke: &StrokeItem) -> std::result::Result<photocraft_doc::Path, String> {
        if stroke.start_arrow.is_some() || stroke.end_arrow.is_some() || stroke.dash_align {
            return Err("brushed arrows/dash alignment require the integrated width projection".into());
        }
        if stroke.brush.is_none() {
            return Err("unbrushed width profile requires the package 11 projection".into());
        }
        geometry::outline(path, stroke).map_err(|e| e.to_string())
    }
}
pub fn refresh(doc: &Document, layer: &mut Layer) -> Result<()> {
    if let Some(appearance) = &layer.vector.appearance {
        for item in &appearance.items {
            if let AppearanceItem::Stroke(stroke) = item
                && let Some(reference) = &stroke.brush
            {
                Resource::resolve(reference).map_err(error)?;
                geometry::validate_stroke(&stroke.base).map_err(error)?;
                if let Some(profile) = &stroke.profile {
                    geometry::validate_profile(profile).map_err(error)?;
                }
            }
        }
    }
    if layer.vector.clip.is_some() || !layer.vector.corners.is_empty() || layer.vector.live_shape.is_some() {
        return Err(EngineError::Other("brush refresh requires integrated corner/clip projection".into()));
    }
    let LayerContent::Shape(shape) = &mut layer.content else {
        return Err(EngineError::Other("brush requires shape".into()));
    };
    let compiled = photocraft_vector::appearance::CompiledAppearance::with_projection(
        shape,
        layer.vector.appearance.as_ref(),
        &doc.patterns,
        doc.bounds(),
        &BrushProjection,
    )
    .map_err(EngineError::Other)?;
    if compiled.cache_bytes(doc.pixel_format()) > MAX_CACHE_PIXELS as u128 * doc.pixel_format().bytes_per_pixel() as u128 {
        return Err(EngineError::Other("brush render cache exceeds pixel budget".into()));
    }
    shape.cache = Some(compiled.render(doc.pixel_format(), doc.bounds()).map_err(EngineError::Other)?);
    if let Some(appearance) = &layer.vector.appearance {
        photocraft_vector::appearance::mirror_legacy(shape, appearance);
    }
    shape.psd_raw = None;
    shape.live = None;
    Ok(())
}
fn request_samples(params: &Value) -> Result<Vec<Sample>> {
    let raw = params
        .get("samples")
        .and_then(Value::as_array)
        .filter(|v| (2..=geometry::MAX_INPUT).contains(&v.len()))
        .ok_or_else(|| EngineError::Other("bounded brush samples required".into()))?;
    let samples: Vec<_> = raw
        .iter()
        .map(|v| {
            let get = |k| v.get(k).and_then(Value::as_f64).ok_or_else(|| EngineError::Other("brush sample number missing".into()));
            Ok(Sample { point: photocraft_geom::Point::new(get("x")?, get("y")?), pressure: get("pressure")? })
        })
        .collect::<Result<_>>()?;
    Ok(samples)
}
fn preview_command(session: &mut Session, params: &Value) -> Result<Value> {
    let capture: Capture = parse(params, "capture")?;
    let prepared = prepare(session, &capture, &request_samples(params)?)?;
    let LayerContent::Shape(shape) = &prepared.layer.content else {
        return Err(EngineError::Other("brush shape missing".into()));
    };
    Ok(json!({"detached":true,"layer":capture.layer.0,"path":shape.path,"stroke":prepared.stroke}))
}
fn commit_command(session: &mut Session, params: &Value) -> Result<Value> {
    let capture: Capture = parse(params, "capture")?;
    capture.validate(session)?;
    let samples = request_samples(params)?;
    let prepared = prepare(session, &capture, &samples)?;
    let id = capture.layer;
    photocraft_doc::ensure_ids_above(id.0);
    crate::vector_integration::insert_layer(session, capture.above, prepared.layer, "Vector Brush")?;
    Ok(json!({"layer":id.0}))
}
fn parse<T: serde::de::DeserializeOwned>(params: &Value, key: &str) -> Result<T> {
    serde_json::from_value(params.get(key).cloned().ok_or_else(|| EngineError::Other(format!("brush {key} required")))?)
        .map_err(|e| EngineError::Other(e.to_string()))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetCapture {
    pub document: DocId,
    pub revision: u64,
    pub layers: Vec<LayerId>,
}
impl TargetCapture {
    pub fn begin(session: &Session, layers: Vec<LayerId>) -> Result<Self> {
        let st = session.active().ok_or(EngineError::NoDocument)?;
        let c = Self { document: st.doc.id, revision: st.revision, layers };
        c.validate(session)?;
        Ok(c)
    }
    pub fn validate(&self, session: &Session) -> Result<()> {
        let st = session.active().ok_or(EngineError::NoDocument)?;
        if self.document != st.doc.id || self.revision != st.revision {
            return Err(EngineError::Other("brush targets are stale".into()));
        }
        if self.layers.len() > 64 {
            return Err(error(geometry::Error::Budget));
        }
        vector_api::snapshot_shapes(&st.doc, &self.layers)?;
        Ok(())
    }
}
fn apply_command(session: &mut Session, params: &Value) -> Result<Value> {
    let c: TargetCapture = parse(params, "capture")?;
    c.validate(session)?;
    let stroke: StrokeItem = parse(params, "stroke")?;
    if stroke.brush.is_none() {
        return Err(error(geometry::Error::Reference));
    }
    let doc = &session.active().ok_or(EngineError::NoDocument)?.doc;
    let mut edits = vector_api::snapshot_shapes(doc, &c.layers)?;
    let mut cache_bytes = 0usize;
    for edit in &mut edits {
        let mut l = doc.layer(edit.layer).cloned().ok_or(EngineError::NoLayer(edit.layer))?;
        let mut appearance = l.vector.appearance.clone().unwrap_or_else(|| Appearance {
            items: {
                let mut items = Vec::new();
                if let Some(fill) = &edit.shape.fill {
                    items.push(AppearanceItem::Fill(FillItem::new(fill.clone())));
                }
                if let Some(base) = &edit.shape.stroke {
                    items.push(AppearanceItem::Stroke(StrokeItem { base: base.clone(), ..Default::default() }));
                }
                items
            },
        });
        if let Some(AppearanceItem::Stroke(s)) = appearance.items.iter_mut().rev().find(|i| matches!(i, AppearanceItem::Stroke(_))) {
            *s = stroke.clone();
        } else {
            appearance.items.push(AppearanceItem::Stroke(stroke.clone()));
        }
        l.vector.appearance = Some(appearance);
        if let LayerContent::Shape(sh) = &mut l.content {
            sh.stroke = Some(stroke.base.clone());
        }
        refresh(doc, &mut l)?;
        let LayerContent::Shape(sh) = l.content else {
            return Err(EngineError::Other("brush shape missing".into()));
        };
        cache_bytes += sh.cache.as_ref().map_or(0, |s| s.tiles().map(|(_, t)| t.bytes().len()).sum::<usize>());
        if cache_bytes > 512 << 20 {
            return Err(EngineError::Other("brush target caches exceed 512 MiB".into()));
        }
        edit.shape = sh;
        edit.vector = l.vector;
    }
    vector_api::apply_shape_edits(session, "Apply Vector Brush", edits)?;
    Ok(json!({"layers":c.layers}))
}
fn expand_command(session: &mut Session, params: &Value) -> Result<Value> {
    let c: TargetCapture = parse(params, "capture")?;
    c.validate(session)?;
    let doc = &session.active().ok_or(EngineError::NoDocument)?.doc;
    let mut replacements = Vec::new();
    let mut cache_bytes = 0usize;
    for id in &c.layers {
        let layer = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
        if !layer.vector.appearance.as_ref().is_some_and(|a| a.items.iter().any(|i| matches!(i,AppearanceItem::Stroke(s) if s.brush.is_some()))) {
            return Err(EngineError::Other("expand requires a brushed stroke on every target".into()));
        }
        let mut children = projection(layer)?;
        cache_bounds(doc, &children)?;
        for child in &mut children {
            if let LayerContent::Shape(sh) = &mut child.content {
                crate::vector_cmds::refresh_shape(doc, sh);
                cache_bytes += sh.cache.as_ref().map_or(0, |s| s.tiles().map(|(_, t)| t.bytes().len()).sum::<usize>());
                if cache_bytes > 512 << 20 {
                    return Err(EngineError::Other("expanded brush caches exceed 512 MiB".into()));
                }
            }
        }
        let mut group = layer.clone();
        group.content =
            LayerContent::Group(photocraft_doc::Group { children, expanded: true, artboard: None, isolation: photocraft_doc::GroupIsolation::Auto });
        group.vector.appearance = None;
        replacements.push(group);
    }
    session.edit("Expand Vector Brush", move |doc, _| {
        for l in replacements {
            let id = l.id;
            *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = l;
        }
        Ok(())
    })?;
    Ok(json!({"layers":c.layers}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    let mut specs: Vec<_> = [
        ("vector.brush.commit", "Vector Brush", commit_command as fn(&mut Session, &Value) -> Result<Value>, "{capture,samples:[{x,y,pressure}]}"),
        ("vector.brush.apply", "Apply Vector Brush", apply_command, "{capture:{document,revision,layers},stroke:StrokeItem}"),
        ("vector.brush.expand", "Expand Vector Brush", expand_command, "{capture:{document,revision,layers}}"),
    ]
    .into_iter()
    .map(|(id, label, run, params)| CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: crate::vector_cmds::has_doc, run, journal: true })
    .collect();
    specs.push(CommandSpec {
        id: "vector.brush.preview",
        label: "Vector Brush",
        menu: &[],
        shortcut: None,
        params: "{capture,samples:[{x,y,pressure}]}",
        enabled: crate::vector_cmds::has_doc,
        run: preview_command,
        journal: false,
    });
    specs
}
#[cfg(test)]
#[path = "brushes/gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "brushes/tests.rs"]
mod tests;
