//! Original V8 layout commands and detached geometry preparation. Existing Move/Align
//! commands remain authoritative for their legacy behavior. This package adds precision
//! layout with whole-target validation and captured document/revision guards.
use super::constraints::arithmetic::{self as math, Box2, error};
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::structure::DesignUnits;
use photocraft_doc::{Artboard, ArtboardBackground, BlendMode, Document, Group, Layer, LayerContent, LayerId, Path, VectorMask};
use photocraft_geom::{Affine, Point, Rect, Size};
use photocraft_vector::appearance::{self, CompiledAppearance};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

pub const MAX_TARGETS: usize = 128;
const MAX_PIXELS: u64 = 16_777_216;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub layers: Vec<LayerId>,
}
impl Capture {
    pub fn begin(s: &Session, layers: Vec<LayerId>) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        preflight(&st.doc, &layers)?;
        Ok(Self { document: st.doc.id.0, revision: st.revision, layers })
    }
    pub fn check(&self, s: &Session) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if self.document != st.doc.id.0 || self.revision != st.revision {
            return Err(error("layout gesture is stale; start again"));
        }
        preflight(&st.doc, &self.layers)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Operation {
    Transform {
        matrix: Affine,
    },
    Crop {
        rect: Option<Box2>,
    },
    ContentMove {
        matrix: Affine,
    },
    Resize {
        rect: Box2,
        #[serde(default)]
        basis: Affine,
    },
    Artboard {
        rect: Box2,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub capture: Capture,
    pub operation: Operation,
}
pub fn validate_geometry(path: &Path) -> Result<()> {
    crate::vector_api::validate_path(path)?;
    if path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > 65_536 {
        return Err(error("layout path exceeds knot budget"));
    }
    for p in path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]) {
        math::finite(p.x)?;
        math::finite(p.y)?;
    }
    Ok(())
}
pub fn preflight(doc: &Document, ids: &[LayerId]) -> Result<()> {
    if ids.is_empty() || ids.len() > MAX_TARGETS {
        return Err(error("layout requires 1 to 128 targets"));
    }
    let mut seen = HashSet::new();
    let mut count = 0;
    let mut total_pixels = 0u64;
    for id in ids {
        if !seen.insert(*id) {
            return Err(error("duplicate layout target"));
        }
        let path = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
        if path.iter().enumerate().any(|(i, _)| i + 1 < path.len() && doc.layer_at(&path[..=i]).is_some_and(|l| ids.contains(&l.id))) {
            return Err(error("select either a group or its child, not both"));
        }
        for (p, _, layer) in doc.walk() {
            if p.starts_with(&path) {
                count += 1;
                let locks = doc.effective_locks(layer.id);
                if locks.all || locks.position || locks.pixels {
                    return Err(error("layout target or descendant is locked"));
                }
                validate_layer(layer)?;
                if !layer.is_group() {
                    let b = bounds(layer)?;
                    let geometry = (b[2].ceil() * b[3].ceil()) as u64;
                    let cache = layer.surface().map_or(0, |s| rect_pixels(s.content_bounds()));
                    total_pixels = total_pixels.saturating_add(geometry.max(cache));
                }
                if let Some(mask) = &layer.mask {
                    total_pixels =
                        total_pixels.saturating_add(u64::from(mask.surface.content_bounds().width()) * u64::from(mask.surface.content_bounds().height()));
                }
                if total_pixels > MAX_PIXELS * 2 {
                    return Err(error("layout target caches exceed aggregate pixel budget"));
                }
            }
        }
    }
    if count > 512 {
        return Err(error("layout descendant budget exceeded"));
    }
    Ok(())
}
fn validate_layer(layer: &Layer) -> Result<()> {
    if layer.vector.live.is_some() {
        return Err(error("live group layout requires its geometry provider"));
    }
    if let LayerContent::Shape(s) = &layer.content {
        validate_geometry(&s.path)?;
    }
    for path in layer.vector.clip.iter().chain(layer.vector_mask.iter().map(|v| &v.path)) {
        validate_geometry(path)?;
    }
    if let Some(s) = layer.surface() {
        pixel_budget(s.content_bounds())?;
    }
    Ok(())
}
fn rect_pixels(r: Rect) -> u64 {
    u64::from(r.width()) * u64::from(r.height())
}
fn pixel_budget(r: Rect) -> Result<()> {
    if rect_pixels(r) > MAX_PIXELS { Err(error("layout cache exceeds 16 megapixel budget")) } else { Ok(()) }
}
pub fn box_from_rect(r: Rect) -> Box2 {
    [f64::from(r.x0), f64::from(r.y0), f64::from(r.width()), f64::from(r.height())]
}
pub fn integer_rect(r: Box2) -> Result<Rect> {
    math::validate_rect(r)?;
    if r[2] > 300_000. || r[3] > 300_000. {
        return Err(error("artboard dimensions must not exceed 300000 pixels"));
    }
    let out = Rect::new(r[0].round() as i32, r[1].round() as i32, (r[0] + r[2]).round() as i32, (r[1] + r[3]).round() as i32);
    if out.is_empty() {
        return Err(error("artboard rounds to an empty rectangle"));
    }
    Ok(out)
}
pub fn bounds(layer: &Layer) -> Result<Box2> {
    if let Some(a) = layer.artboard() {
        return Ok(box_from_rect(a.rect));
    }
    let b = match &layer.content {
        LayerContent::Shape(s) => photocraft_pathops::bounds(&s.path).map_err(|e| error(e.to_string()))?,
        LayerContent::Group(g) => {
            let boxes = g.children.iter().map(bounds).collect::<Result<Vec<_>>>()?;
            boxes.into_iter().map(|b| (b[0], b[1], b[0] + b[2], b[1] + b[3])).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
        }
        _ => layer.surface().map(|s| box_from_rect(s.content_bounds())).filter(|b| b[2] > 0. && b[3] > 0.).map(|b| (b[0], b[1], b[0] + b[2], b[1] + b[3])),
    }
    .ok_or_else(|| error("layout target has no geometry"))?;
    // Straight lines are legitimate transform targets, with a minimal orthogonal extent.
    math::validate_rect([b.0, b.1, (b.2 - b.0).max(math::MIN_SIZE), (b.3 - b.1).max(math::MIN_SIZE)])
}
pub fn selection_bounds(doc: &Document, ids: &[LayerId]) -> Result<Box2> {
    preflight(doc, ids)?;
    let boxes = ids.iter().map(|id| bounds(doc.layer(*id).ok_or(EngineError::NoLayer(*id))?)).collect::<Result<Vec<_>>>()?;
    let b = boxes
        .into_iter()
        .reduce(|a, b| {
            let x = a[0].min(b[0]);
            let y = a[1].min(b[1]);
            [x, y, (a[0] + a[2]).max(b[0] + b[2]) - x, (a[1] + a[3]).max(b[1] + b[3]) - y]
        })
        .ok_or_else(|| error("empty layout selection"))?;
    math::validate_rect(b)
}
/// Transform about a document-space pivot, applying size/shear, then rotation, then movement.
pub fn anchored(pivot: Point, delta: Point, scale: [f64; 2], rotation: f64, shear: f64) -> Result<Affine> {
    for v in [pivot.x, pivot.y, delta.x, delta.y, scale[0], scale[1], rotation, shear] {
        math::finite(v)?;
    }
    if shear.abs() >= 89. {
        return Err(error("shear must be less than 89 degrees"));
    }
    let size = Affine { m: [scale[0], 0., shear.to_radians().tan() * scale[1], scale[1], 0., 0.] };
    let a = Affine::translate(pivot.x + delta.x, pivot.y + delta.y)
        .mul(&Affine::rotate(rotation.to_radians()))
        .mul(&size)
        .mul(&Affine::translate(-pivot.x, -pivot.y));
    math::validate_affine(a)?;
    Ok(a)
}
/// Independent node-to-pointer similarity: pivot stays fixed and the dragged node lands at
/// the endpoint. Shift quantizes rotation to 15 degrees; Alt chooses rotation without scale.
pub fn point_transform(pivot: Point, from: Point, to: Point, shift: bool, rotate_only: bool) -> Result<Affine> {
    let a = [from.x - pivot.x, from.y - pivot.y];
    let b = [to.x - pivot.x, to.y - pivot.y];
    let length = a[0].hypot(a[1]);
    if length < math::MIN_SIZE {
        return Err(error("drag a node away from the transform origin"));
    }
    let mut degrees = (b[1].atan2(b[0]) - a[1].atan2(a[0])).to_degrees();
    if shift {
        degrees = (degrees / 15.).round() * 15.;
    }
    let scale = if rotate_only { 1. } else { b[0].hypot(b[1]) / length };
    anchored(pivot, Point::new(0., 0.), [scale; 2], degrees, 0.)
}
fn update_metadata(layer: &mut Layer, original: &Layer, a: Affine) -> Result<()> {
    // Always derive clips from the captured source. Shared transform-layer integration
    // may also transform clips; replacing from source makes this exactly once.
    layer.vector.clip = original.vector.clip.as_ref().map(|clip| clip.transform(&a));
    if let Some(clip) = &layer.vector.clip {
        validate_geometry(clip)?;
    }
    let k = a.determinant().abs().sqrt();
    if !layer.vector.corners.is_empty() {
        let dot = a.m[0] * a.m[2] + a.m[1] * a.m[3];
        let sx = a.m[0].hypot(a.m[1]);
        let sy = a.m[2].hypot(a.m[3]);
        if dot.abs() > 1e-8 || (sx - sy).abs() > 1e-8 {
            return Err(error("anisotropic live-corner layout requires conversion to curves"));
        }
        for corner in &mut layer.vector.corners {
            corner.corner.radius *= k;
        }
    }
    // The baked base path remains authoritative when the parametric design cannot express
    // the transform. No knot indices change, so live corners retain their identity.
    layer.vector.live_shape = None;
    if let Some(appearance) = &mut layer.vector.appearance {
        appearance::scale_with_object(appearance, a).map_err(error)?;
    }
    if let LayerContent::Group(g) = &mut layer.content {
        if let Some(board) = &mut g.artboard {
            if a.m[1].abs() > 1e-12 || a.m[2].abs() > 1e-12 || a.m[0] <= 0. || a.m[3] <= 0. {
                return Err(error("artboards support axis-aligned transforms"));
            }
            let r = box_from_rect(board.rect);
            let p = a.apply(Point::new(r[0], r[1]));
            board.rect = integer_rect([p.x, p.y, r[2] * a.m[0], r[3] * a.m[3]])?;
            board.preset.clear();
        }
        if let LayerContent::Group(source) = &original.content {
            for (child, original_child) in g.children.iter_mut().zip(&source.children) {
                update_metadata(child, original_child, a)?;
            }
        }
    }
    Ok(())
}
fn refresh(doc: &Document, l: &mut Layer) -> Result<()> {
    match &mut l.content {
        LayerContent::Shape(shape) => {
            validate_geometry(&shape.path)?;
            let projected = super::corners::project_shape(shape, &l.vector.corners)?;
            let compiled = CompiledAppearance::new(&projected, l.vector.appearance.as_ref(), &doc.patterns, doc.bounds()).map_err(error)?;
            pixel_budget(compiled.bounds().intersect(&doc.bounds()))?;
            if compiled.cache_bytes(doc.pixel_format()) > 512 << 20 {
                return Err(error("layout appearance cache exceeds byte budget"));
            }
            shape.cache = Some(compiled.render(doc.pixel_format(), doc.bounds()).map_err(error)?);
            if let Some(a) = &l.vector.appearance {
                appearance::mirror_legacy(shape, a);
            }
        }
        LayerContent::Text(t) => {
            if let Some(path) = &l.vector.on_path {
                super::path_text::refresh(doc, t, path)?;
            } else {
                crate::type_cmds::refresh(doc, t);
            }
            if let Some(s) = &t.cache {
                pixel_budget(s.content_bounds())?;
            }
        }
        LayerContent::Smart(_) => {
            crate::smart_cmds::refresh_layer(doc, l)?;
        }
        LayerContent::Group(g) => {
            let mut pixels = 0u64;
            for c in &mut g.children {
                refresh(doc, c)?;
                pixels = pixels.saturating_add(prepared_pixels(c)?);
                check_pixel_sum(pixels)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn prepared_pixels(layer: &Layer) -> Result<u64> {
    let mut pixels = if let LayerContent::Group(g) = &layer.content {
        g.children.iter().try_fold(0u64, |sum, l| -> Result<u64> { Ok(sum.saturating_add(prepared_pixels(l)?)) })?
    } else {
        let b = bounds(layer)?;
        ((b[2].ceil() * b[3].ceil()) as u64).max(layer.surface().map_or(0, |s| rect_pixels(s.content_bounds())))
    };
    if let Some(mask) = &layer.mask {
        pixels = pixels.saturating_add(rect_pixels(mask.surface.content_bounds()));
    }
    check_pixel_sum(pixels)?;
    Ok(pixels)
}
pub fn transformed(doc: &Document, layer: &Layer, a: Affine) -> Result<Layer> {
    transformed_impl(doc, layer, a, false)
}
/// Resize a parent's frame/masks without rendering children that receive their own constraints.
pub fn transformed_parent(doc: &Document, layer: &Layer, a: Affine) -> Result<Layer> {
    transformed_impl(doc, layer, a, true)
}
/// Bound the entire detached target set before any raster warp/cache allocation.
pub(crate) fn check_pixel_sum(pixels: u64) -> Result<()> {
    if pixels > MAX_PIXELS * 2 { Err(error("transformed targets exceed aggregate cache budget")) } else { Ok(()) }
}
pub(crate) fn transform_pixels(layer: &Layer, a: Affine, frame_only: bool) -> Result<u64> {
    math::validate_affine(a)?;
    fn extent(b: Box2, a: Affine) -> Result<u64> {
        let points = [(b[0], b[1]), (b[0] + b[2], b[1]), (b[0], b[1] + b[3]), (b[0] + b[2], b[1] + b[3])].map(|(x, y)| a.apply(Point::new(x, y)));
        for p in points {
            math::finite(p.x)?;
            math::finite(p.y)?;
        }
        let lo_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let hi_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let lo_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let hi_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        Ok(((hi_x - lo_x).ceil() * (hi_y - lo_y).ceil()) as u64)
    }
    let geometry = extent(bounds(layer)?, a)?;
    let mut pixels = if layer.is_group() { 0 } else { geometry };
    if pixels > MAX_PIXELS {
        return Err(error("transformed leaf exceeds cache budget"));
    }
    if let Some(mask) = &layer.mask {
        let r = mask.surface.content_bounds();
        pixel_budget(r)?;
        let n = extent(box_from_rect(r), if mask.linked { a } else { Affine::IDENTITY })?;
        if n > MAX_PIXELS {
            return Err(error("transformed mask exceeds cache budget"));
        }
        pixels = pixels.saturating_add(n);
    }
    if !frame_only && let LayerContent::Group(g) = &layer.content {
        for child in &g.children {
            pixels = pixels.saturating_add(transform_pixels(child, a, false)?);
        }
    }
    check_pixel_sum(pixels)?;
    Ok(pixels)
}
fn transformed_impl(doc: &Document, layer: &Layer, a: Affine, frame_only: bool) -> Result<Layer> {
    transform_pixels(layer, a, frame_only)?;
    let mut l = layer.clone();
    if frame_only && let LayerContent::Group(g) = &mut l.content {
        g.children.clear();
    }
    let [a0, b0, c0, d0, e0, f0] = a.m;
    let h = photocraft_algo::transform::Homography([a0, c0, e0, b0, d0, f0, 0., 0., 1.]);
    crate::transform_cmds::transform_layer(None, Default::default(), &mut l, &h, Some(a), photocraft_algo::transform::Interp::Bilinear)?;
    update_metadata(&mut l, layer, a)?;
    refresh(doc, &mut l)?;
    Ok(l)
}
pub fn crop_path(rect: Box2) -> Result<Path> {
    math::validate_rect(rect)?;
    if rect[2] * rect[3] > MAX_PIXELS as f64 {
        return Err(error("crop rectangle exceeds mask pixel budget"));
    }
    Ok(photocraft_vector::shapes::rect(rect[0], rect[1], rect[2], rect[3]))
}
fn grow(doc: &mut Document, r: Rect) {
    doc.size = Size::new(doc.size.width.max(r.x1.clamp(0, 300_000) as u32), doc.size.height.max(r.y1.clamp(0, 300_000) as u32));
}
/// All operations prepare a detached document; callers may use it for display. Nothing is
/// journaled or published until commit, which repeats freshness and target validation.
pub fn preview(s: &Session, request: &Commit) -> Result<Document> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if request.capture.document != st.doc.id.0 || request.capture.revision != st.revision {
        return Err(error("layout gesture is stale; start again"));
    }
    let mut doc = (*st.doc).clone();
    if let Operation::Artboard { rect } = request.operation {
        if !request.capture.layers.is_empty() {
            return Err(error("new artboard cannot have targets"));
        }
        let r = integer_rect(rect)?;
        let mut board = Layer::group("Artboard", vec![]);
        if let LayerContent::Group(g) = &mut board.content {
            g.artboard = Some(Artboard { rect: r, background: ArtboardBackground::Transparent, preset: String::new() });
        }
        doc.layers.push(board);
        grow(&mut doc, r);
        return Ok(doc);
    }
    request.capture.check(s)?;
    let mut prepared = Vec::new();
    let mut actual_pixels = 0u64;
    match &request.operation {
        Operation::Transform { matrix } | Operation::ContentMove { matrix } => {
            let pixels = request.capture.layers.iter().try_fold(0u64, |sum, id| -> Result<u64> {
                Ok(sum.saturating_add(transform_pixels(doc.layer(*id).ok_or(EngineError::NoLayer(*id))?, *matrix, false)?))
            })?;
            check_pixel_sum(pixels)?;
            for id in &request.capture.layers {
                let original = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
                let mut out = transformed(&doc, original, *matrix)?;
                if matches!(request.operation, Operation::ContentMove { .. }) {
                    out.vector.clip = original.vector.clip.clone();
                }
                actual_pixels = actual_pixels.saturating_add(prepared_pixels(&out)?);
                check_pixel_sum(actual_pixels)?;
                prepared.push(out);
            }
        }
        Operation::Crop { rect } => {
            let path = rect.map(crop_path).transpose()?;
            for id in &request.capture.layers {
                let mut l = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?.clone();
                l.vector.clip = path.clone();
                prepared.push(l);
            }
        }
        Operation::Resize { rect, basis } => {
            math::parent_basis(*basis)?;
            if request.capture.layers.len() != 1 {
                return Err(error("resize requires one parent"));
            }
            let id = request.capture.layers[0];
            let l = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
            let old = bounds(l)?;
            let target = if l.artboard().is_some() {
                let r = integer_rect(*rect)?;
                grow(&mut doc, r);
                box_from_rect(r)
            } else {
                *rect
            };
            prepared.push(super::constraints::prepare_resize(&doc, id, old, target, true)?);
        }
        Operation::Artboard { .. } => {}
    }
    for l in prepared {
        let id = l.id;
        *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = l;
    }
    Ok(doc)
}
pub fn commit(s: &mut Session, request: &Commit) -> Result<()> {
    let prepared = preview(s, request)?;
    if !matches!(request.operation, Operation::Artboard { .. }) {
        request.capture.check(s)?;
    }
    let ids = request.capture.layers.clone();
    let is_new = matches!(request.operation, Operation::Artboard { .. });
    let created = s.edit("Vector Layout", move |doc, active| {
        doc.size = prepared.size;
        if is_new {
            let board = prepared.layers.last().ok_or_else(|| error("missing prepared artboard"))?.clone();
            let id = doc.insert_above(None, board);
            *active = Some(id);
            return Ok(Some(id));
        } else {
            for id in ids {
                *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = prepared.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
            }
        }
        Ok(None)
    })?;
    if let Some(id) = created {
        crate::layer_multi_cmds::set_selection(s, vec![id], Some(id), Some(id))?;
    }
    Ok(())
}
fn apply(s: &mut Session, p: &Value) -> Result<Value> {
    let request: Commit = serde_json::from_value(p.clone()).map_err(|e| error(e.to_string()))?;
    commit(s, &request)?;
    Ok(json!({"applied":true}))
}
/// Non-destructive crop projection for the normal CPU/GPU/scaled tree. The wrapper adds a
/// separate vector clip after the original layer's mask, preserving mask density/feather.
/// Synthetic child IDs are deterministic and outside all IDs in this detached document.
/// This is a display/export projection only; never persist the wrapper tree.
pub fn project_clips(source: &Document) -> Result<Document> {
    let mut doc = source.clone();
    let mut next = source.walk().iter().map(|(_, _, l)| l.id.0).max().unwrap_or(0);
    fn layers(input: &mut [Layer], next: &mut u64) -> Result<()> {
        for l in input {
            if let LayerContent::Group(g) = &mut l.content {
                layers(&mut g.children, next)?;
            }
            let Some(clip) = l.vector.clip.take() else { continue };
            validate_geometry(&clip)?;
            *next = next.checked_add(1).ok_or_else(|| error("projection layer id overflow"))?;
            let mut child = l.clone();
            child.id = LayerId(*next);
            child.clipped = false;
            child.blend = if child.is_group() && child.blend == BlendMode::PassThrough { BlendMode::PassThrough } else { BlendMode::Normal };
            child.opacity = 1.;
            child.fill_opacity = 1.;
            child.effects = Default::default();
            child.blend_if = Default::default();
            l.content = LayerContent::Group(Group { children: vec![child], expanded: true, artboard: None });
            if l.blend == BlendMode::PassThrough {
                l.blend = BlendMode::Normal;
            }
            l.mask = None;
            l.vector_mask = Some(VectorMask::new(clip));
            // Feature metadata belongs to the content child, not its clip wrapper.
            l.vector = Default::default();
        }
        Ok(())
    }
    layers(&mut doc.layers, &mut next)?;
    Ok(doc)
}
/// Conversion uses exact definitions: 72 points and 25.4 millimeters per inch.
pub fn to_pixels(value: f64, unit: DesignUnits, dpi: f64) -> Result<f64> {
    math::finite(value)?;
    if !dpi.is_finite() || !(1.0..=30_000.0).contains(&dpi) {
        return Err(error("DPI must be finite and between 1 and 30000"));
    }
    math::finite(
        value
            * match unit {
                DesignUnits::Pixels => 1.,
                DesignUnits::Points => dpi / 72.,
                DesignUnits::Millimeters => dpi / 25.4,
                DesignUnits::Inches => dpi,
            },
    )
}
pub fn from_pixels(value: f64, unit: DesignUnits, dpi: f64) -> Result<f64> {
    math::finite(value)?;
    Ok(value / to_pixels(1., unit, dpi)?)
}
pub fn preset_units(category: &str) -> DesignUnits {
    match category {
        "Print" => DesignUnits::Millimeters,
        "Web" | "Mobile" | "Film & Video" | "Art & Illustration" => DesignUnits::Points,
        _ => DesignUnits::Pixels,
    }
}
/// Additive file.new hook for 00: geometry parameters remain pixels, while this field
/// records design ruler units. Old automation/settings with no category default to px.
pub fn new_document_units(p: &Value) -> Result<DesignUnits> {
    p.get("designUnits")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| error(e.to_string()))
        .map(|units| units.unwrap_or_else(|| preset_units(p.get("category").and_then(Value::as_str).unwrap_or("Photo"))))
}
fn new_document(s: &mut Session, p: &Value) -> Result<Value> {
    let category = p.get("category").and_then(Value::as_str).ok_or_else(|| error("new design document requires category"))?;
    let units = p.get("units").cloned().map(serde_json::from_value).transpose().map_err(|e| error(e.to_string()))?.unwrap_or_else(|| preset_units(category));
    let dpi = p.get("dpi").and_then(Value::as_f64).unwrap_or(72.);
    let w = to_pixels(p.get("width").and_then(Value::as_f64).ok_or_else(|| error("width required"))?, units, dpi)?.round();
    let h = to_pixels(p.get("height").and_then(Value::as_f64).ok_or_else(|| error("height required"))?, units, dpi)?.round();
    if !(1.0..=300_000.).contains(&w) || !(1.0..=300_000.).contains(&h) || w * h > MAX_PIXELS as f64 {
        return Err(error("new design document exceeds size budget"));
    }
    let mut doc = Document::new(
        p.get("name").and_then(Value::as_str).unwrap_or("Design"),
        Size::new(w as u32, h as u32),
        photocraft_doc::ColorMode::Rgb,
        photocraft_doc::SampleType::U8,
    );
    doc.design_units = units;
    doc.resolution_dpi = dpi as f32;
    s.add_document(doc, None);
    Ok(json!({"units":units,"width":w,"height":h}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "vector.layout.apply",
            label: "Vector Layout",
            menu: &[],
            shortcut: None,
            params: "{capture:{document,revision,layers},operation:{kind:transform|crop|resize|artboard,…}}",
            enabled: crate::vector_cmds::has_doc,
            journal: true,
            run: apply,
        },
        CommandSpec {
            id: "vector.layout.align",
            label: "Align to Key Object",
            menu: &[],
            shortcut: None,
            params: "{capture,edge:left|centerX|right|top|centerY|bottom,key?}",
            enabled: crate::vector_cmds::has_doc,
            journal: true,
            run: crate::align_cmds::vector_align,
        },
        CommandSpec {
            id: "vector.layout.spacing",
            label: "Distribute Spacing",
            menu: &[],
            shortcut: None,
            params: "{capture,axis:x|y,spacing?,key?}",
            enabled: crate::vector_cmds::has_doc,
            journal: true,
            run: crate::align_cmds::vector_spacing,
        },
        CommandSpec {
            id: "vector.layout.newDocument",
            label: "New Design Document",
            menu: &[],
            shortcut: None,
            params: "{category,width,height,dpi?,units?,name?}",
            enabled: |_| Ok(()),
            journal: true,
            run: new_document,
        },
    ]
}
#[cfg(test)]
#[path = "layout/tests.rs"]
mod tests;
