//! Blob Brush / area Eraser transactions adapted from VectorCraft
//! crates/engine/src/cmd/draw2.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Sampling/fidelity use the Pencil fitter; pressure profiles and appearance projection use the brush APIs.
use super::pencil::fitting::{self, FitOptions, Sample};
use crate::{EngineError, Result, Session, commands::CommandSpec, vector_api};
use photocraft_doc::{BlendMode, Color, Document, Fill, Layer, LayerContent, LayerId, Path};
use photocraft_pathops::{
    self as ops, BoolOp,
    geom::{ParamCurve, Shape as _},
};
pub use photocraft_vecfx::sweep::{self, Tip};
use photocraft_vector::appearance::{self, CompiledAppearance};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

pub const MAX_TARGETS: usize = 1024;
const MAX_OUTPUTS: usize = 1024;
const MAX_CACHE_BYTES: u128 = 512 << 20;
pub const EXCLUDED_TOOLTIP: &str = "Vector Eraser does not erase text or symbol instances. Convert or detach them first.";
fn error(e: impl ToString) -> EngineError {
    EngineError::Other(e.to_string())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Blob,
    Eraser,
}
impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Blob => "Blob Brush",
            Self::Eraser => "Vector Eraser",
        }
    }
    fn command(self) -> &'static str {
        match self {
            Self::Blob => "path.blob",
            Self::Eraser => "path.eraseRegion",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub tip: Tip,
    pub tolerance: f64,
    pub fidelity: f64,
    pub keep_selected: bool,
    pub merge_only_selection: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self { tip: Tip::default(), tolerance: 1., fidelity: 0.5, keep_selected: false, merge_only_selection: false }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        self.tip.validate().map_err(error)?;
        FitOptions { tolerance: self.tolerance, smoothness: self.fidelity, closed: false }.validate().map_err(error)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub targets: Vec<LayerId>,
    pub selected: Vec<LayerId>,
    pub above: Option<LayerId>,
    pub new_layer: LayerId,
    pub color: [f32; 4],
    pub mode: Mode,
    pub options: Options,
    pub excluded: bool,
}
fn paint(color: [f32; 4]) -> Fill {
    Fill::Solid(Color::rgba(color[0], color[1], color[2], color[3]))
}
fn parent(doc: &Document, id: Option<LayerId>) -> Result<Vec<usize>> {
    let Some(id) = id else {
        return Ok(Vec::new());
    };
    let mut path = doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
    path.pop();
    Ok(path)
}
fn mergeable(doc: &Document, layer: &Layer, color: [f32; 4], above: Option<LayerId>) -> Result<bool> {
    let LayerContent::Shape(sh) = &layer.content else {
        return Ok(false);
    };
    let expected = photocraft_doc::ShapeLayer { fill: Some(paint(color)), ..Default::default() };
    Ok(parent(doc, Some(layer.id))? == parent(doc, above)?
        && layer.opacity == 1.
        && layer.fill_opacity == 1.
        && layer.blend == BlendMode::Normal
        && !layer.clipped
        && layer.mask.is_none()
        && layer.vector_mask.is_none()
        && layer.vector.clip.is_none()
        && layer.effects == photocraft_doc::Effects { enabled: layer.effects.enabled, ..Default::default() }
        && layer.excluded_channels == 0
        && layer.blend_if.is_default()
        && layer.vector.appearance.clone().unwrap_or_else(|| appearance::legacy_appearance(sh)) == appearance::legacy_appearance(&expected)
        && sh.path.subpaths.iter().all(|sp| sp.closed))
}
impl Capture {
    pub fn begin(s: &Session, mode: Mode, options: Options) -> Result<Self> {
        Self::begin_with_color(s, mode, options, s.tools.foreground)
    }
    /// The app passes its current stroke proxy colour here, before the gesture starts.
    pub fn begin_with_color(s: &Session, mode: Mode, options: Options, color: [f32; 4]) -> Result<Self> {
        options.validate()?;
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let selected = st.selected_layers();
        let selected_paths = selected.iter().map(|id| st.doc.path_of(*id).ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
        let limited = match mode {
            Mode::Blob => options.merge_only_selection,
            Mode::Eraser => !selected.is_empty(),
        };
        let mut targets = Vec::new();
        let mut excluded = false;
        for (path, _, layer) in st.doc.walk() {
            if limited && !selected_paths.iter().any(|p| path.starts_with(p)) {
                continue;
            }
            if matches!(layer.content, LayerContent::Text(_) | LayerContent::Symbol(_)) {
                excluded = true;
            }
            if !matches!(layer.content, LayerContent::Shape(_)) {
                continue;
            }
            let locks = st.doc.effective_locks(layer.id);
            if !limited && (locks.all || locks.pixels) {
                continue;
            }
            if mode == Mode::Blob && !mergeable(&st.doc, layer, color, st.active_layer)? {
                continue;
            }
            targets.push(layer.id);
        }
        let capture = Self {
            document: st.doc.id.0,
            revision: st.revision,
            targets,
            selected,
            above: st.active_layer,
            new_layer: LayerId::fresh(),
            color,
            mode,
            options,
            excluded,
        };
        capture.validate(s)?;
        Ok(capture)
    }
    pub fn validate(&self, s: &Session) -> Result<()> {
        self.options.validate()?;
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if st.doc.id.0 != self.document || st.revision != self.revision {
            return Err(error("Blob Brush / Eraser gesture is stale"));
        }
        if self.targets.len() > MAX_TARGETS || self.selected.len() > MAX_TARGETS {
            return Err(error("Blob Brush / Eraser target budget exceeded"));
        }
        if self.color.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
            return Err(error("invalid stroke colour"));
        }
        if !self.targets.is_empty() {
            vector_api::snapshot_shapes(&st.doc, &self.targets)?;
        }
        let mut knots = 0;
        for id in &self.targets {
            let layer = st.doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
            let LayerContent::Shape(sh) = &layer.content else {
                return Err(error("shape required"));
            };
            sweep::validate_path(&sh.path).map_err(error)?;
            knots += sh.path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
            if knots > sweep::MAX_KNOTS {
                return Err(error("captured target geometry budget exceeded"));
            }
            if self.mode == Mode::Blob && !mergeable(&st.doc, layer, self.color, self.above)? {
                return Err(error("blob merge target has a different appearance or parent"));
            }
        }
        if self.mode == Mode::Blob {
            if self.new_layer.0 == 0 || self.new_layer.0 > i64::MAX as u64 || st.doc.layer(self.new_layer).is_some() {
                return Err(error("invalid blob layer ID"));
            }
            let par = parent(&st.doc, self.above)?;
            for n in 1..=par.len() {
                if st.doc.layer_at(&par[..n]).is_some_and(|l| l.locks.all) {
                    return Err(error("blob insertion parent is locked"));
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capture: Capture,
    pub samples: Vec<sweep::Sample>,
    /// Sampled each frame; true means an Alt rectangle from first to last pointer.
    #[serde(default)]
    pub rectangle: bool,
}
#[derive(Clone, Debug)]
pub struct Prepared {
    pub document: Arc<Document>,
    pub region: Path,
    pub layers: Vec<LayerId>,
    pub changed: bool,
    request: Request,
}
impl Prepared {
    pub fn capture(&self) -> &Capture {
        &self.request.capture
    }
    pub fn commit(self, s: &mut Session) -> Result<Value> {
        self.request.capture.validate(s)?;
        if !self.changed {
            return Ok(json!({"layers":[],"changed":false}));
        }
        s.execute(self.request.capture.mode.command(), serde_json::to_value(&self.request).map_err(error)?)
    }
}
/// Fit with the corner-preserving smoothing and V1, then interpolate the
/// pressure profile by normalized length, retaining its pressure breakpoints.
fn samples(request: &Request) -> Result<Vec<sweep::Sample>> {
    if request.samples.is_empty() || request.samples.len() > sweep::MAX_SAMPLES {
        return Err(error("gesture needs 1..1024 samples"));
    }
    for s in &request.samples {
        s.validate().map_err(error)?;
    }
    if request.samples.len() < 2 || request.rectangle {
        return Ok(request.samples.clone());
    }
    let mut raw = request.samples.clone();
    raw.dedup_by(|a, b| a.point == b.point && a.pressure == b.pressure);
    if raw.iter().all(|s| s.point == raw[0].point) {
        return Ok(raw);
    }
    let fitted = fitting::fit_with(
        &raw.iter().map(|s| Sample { point: s.point, pressure: s.pressure }).collect::<Vec<_>>(),
        FitOptions { tolerance: request.capture.options.tolerance, smoothness: request.capture.options.fidelity, closed: false },
        |p, t| {
            let mut path = ops::fit_path(p, t).map_err(|e| e.to_string())?;
            // V1's kurbo refit can introduce roundoff at an endpoint (e.g. a
            // vertical run starting at y=0). Restore the exact endpoints that
            // Pencil requires, while rejecting any material endpoint displacement.
            if let Some(sp) = path.subpaths.first_mut() {
                for (i, point) in [(0, p[0]), (sp.knots.len().saturating_sub(1), p[p.len() - 1])] {
                    if let Some(k) = sp.knots.get_mut(i) {
                        let dx = point.x - k.anchor.x;
                        let dy = point.y - k.anchor.y;
                        if dx.hypot(dy) > 1e-7 {
                            return Err("V1 fitter displaced a run endpoint".into());
                        }
                        k.anchor = point;
                        k.in_ctrl.x += dx;
                        k.in_ctrl.y += dy;
                        k.out_ctrl.x += dx;
                        k.out_ctrl.y += dy;
                    }
                }
            }
            Ok(path)
        },
    )
    .map_err(error)?;
    let poly = photocraft_vector::flatten_path(&fitted, 0.05).into_iter().next().ok_or_else(|| error("missing fitted stroke"))?;
    if poly.pts.len() > sweep::MAX_SAMPLES {
        return Err(error("fitted sweep sample budget exceeded"));
    }
    let profile = photocraft_vecfx::brush::pressure_profile(&raw.iter().map(|s| (s.point, s.pressure)).collect::<Vec<_>>(), 1.).map_err(error)?;
    let mut length = vec![0.];
    for p in poly.pts.windows(2) {
        length.push(length[length.len() - 1] + (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1));
    }
    let total = *length.last().ok_or_else(|| error("empty fitted stroke"))?;
    if total <= 1e-9 {
        return Err(error("degenerate fitted stroke"));
    }
    let mut positions: Vec<_> = length.iter().map(|n| n / total).chain(profile.points.iter().map(|p| p.t)).collect();
    positions.sort_by(f64::total_cmp);
    positions.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    if positions.len() > sweep::MAX_SAMPLES {
        return Err(error("pressure sweep sample budget exceeded"));
    }
    Ok(positions
        .into_iter()
        .map(|t| {
            let i = length.partition_point(|n| *n < t * total).clamp(1, length.len() - 1);
            let u = (t * total - length[i - 1]) / (length[i] - length[i - 1]);
            let j = profile.points.partition_point(|p| p.t < t).clamp(1, profile.points.len() - 1);
            let (a, b) = (profile.points[j - 1], profile.points[j]);
            let pressure = 2. * (a.left + (b.left - a.left) * (t - a.t) / (b.t - a.t));
            let (a, b) = (poly.pts[i - 1], poly.pts[i]);
            sweep::Sample { point: photocraft_geom::Point::new(a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u), pressure: pressure.clamp(0., 1.) }
        })
        .collect())
}
fn normalize(doc: &Document, path: &Path) -> Result<Path> {
    let b = doc.bounds();
    ops::finish_compound(path, Some(ops::geom::Rect::new(b.x0 as f64, b.y0 as f64, b.x1 as f64, b.y1 as f64))).map_err(error)
}
/// Connected outers keep their holes. Adapted from VC draw2::components; independent
/// Combine contours would incorrectly fill holes in the native PSD component model.
fn components(path: &Path) -> Vec<Path> {
    let raw = ops::to_geometry(path);
    let subs: Vec<_> = raw.subpaths.iter().filter(|s| s.closed && s.anchors.len() >= 2).collect();
    let bps: Vec<_> = subs.iter().map(|s| ops::geom::PathData::single((*s).clone()).to_bezpath()).collect();
    let points: Vec<_> = subs.iter().map(|s| s.segment(0).eval(0.5)).collect();
    let inside = |j: usize, i: usize| bps[j].winding(points[i]) != 0;
    let depth: Vec<_> = (0..subs.len()).map(|i| (0..subs.len()).filter(|&j| i != j && inside(j, i)).count()).collect();
    let outer: Vec<_> = (0..subs.len()).filter(|&i| depth[i].is_multiple_of(2)).collect();
    let mut groups: Vec<_> = outer.iter().map(|&i| vec![subs[i].clone()]).collect();
    for h in (0..subs.len()).filter(|&i| !depth[i].is_multiple_of(2)) {
        if let Some((k, _)) = outer
            .iter()
            .enumerate()
            .filter(|(_, o)| depth[**o] + 1 == depth[h] && inside(**o, h))
            .min_by(|a, b| subs[*a.1].signed_area().abs().total_cmp(&subs[*b.1].signed_area().abs()))
        {
            groups[k].push(subs[h].clone());
        }
    }
    groups.into_iter().map(|g| ops::from_geometry(&ops::geom::PathData::new(g))).collect()
}
/// Use V1 intersection parameters and retain exact cubic subsegments, including fully
/// contained open curves. No fixed event/parameter sampling can miss a narrow crossing.
fn erase_open(path: &Path, region: &Path) -> Result<(bool, Vec<Path>)> {
    let mut changed = false;
    let mut out = Vec::new();
    let outline = ops::to_geometry(region).to_bezpath();
    for sp in &path.subpaths {
        let source = Path::new(vec![sp.clone()]);
        let arrangement = ops::shape_builder(&[ops::Shape::new(source.clone(), 0), ops::Shape::new(region.clone(), 1)], true).map_err(error)?;
        let mut raw = ops::to_geometry(&source);
        for piece in arrangement.lines.iter().chain(&arrangement.edges).filter(|p| p.key == 0) {
            for cut in &ops::to_geometry(&piece.path).subpaths {
                let n = cut.segment_count();
                if n > 0
                    && outline.winding(cut.segment(n / 2).eval(0.5)) != 0
                    && let Some(next) = ops::kernel::cut_out(&raw, cut, false, 0.001)
                {
                    changed = true;
                    raw = next;
                }
            }
        }
        out.extend(raw.subpaths.into_iter().map(|s| ops::from_geometry(&ops::geom::PathData::single(s))));
    }
    Ok((changed, out))
}
struct Projection;
impl appearance::StrokeProjection for Projection {
    fn outline(&self, path: &Path, stroke: &photocraft_doc::vector_design::appearance::StrokeItem) -> std::result::Result<Path, String> {
        if stroke.brush.is_some() {
            appearance::StrokeProjection::outline(&super::brushes::BrushProjection, path, stroke)
        } else {
            appearance::StrokeProjection::outline(&super::width::geometry::Projection, path, stroke)
        }
    }
}
fn rendered(doc: &Document, layer: &mut Layer) -> Result<()> {
    let LayerContent::Shape(sh) = &mut layer.content else {
        return Err(error("shape output required"));
    };
    sweep::validate_path(&sh.path).map_err(error)?;
    sh.live = None;
    sh.psd_raw = None;
    sh.cache = None;
    layer.vector.corners.clear();
    layer.vector.live_shape = None;
    layer.psd_blocks.retain(|(k, _)| ![b"vmsk", b"vsms", b"vogk", b"vstk", b"vscg", b"SoCo", b"GdFl", b"PtFl"].contains(&k));
    if let Some(a) = &layer.vector.appearance {
        appearance::mirror_legacy(sh, a);
    }
    let compiled = CompiledAppearance::with_projection(sh, layer.vector.appearance.as_ref(), &doc.patterns, doc.bounds(), &Projection).map_err(error)?;
    if compiled.cache_bytes(doc.pixel_format()) > MAX_CACHE_BYTES {
        return Err(error("Blob Brush / Eraser cache budget exceeded"));
    }
    sh.cache = Some(compiled.render(doc.pixel_format(), doc.bounds()).map_err(error)?);
    Ok(())
}
pub fn prepare(s: &Session, request: &Request) -> Result<Prepared> {
    request.capture.validate(s)?;
    if request.rectangle && request.capture.mode != Mode::Eraser {
        return Err(error("rectangle erase is only valid for Vector Eraser"));
    }
    let samples = samples(request)?;
    let mut region = if request.rectangle {
        sweep::rectangle(samples[0].point, samples[samples.len() - 1].point)
    } else {
        sweep::outline(&samples, request.capture.options.tip)
    }
    .map_err(error)?;
    // The brush arc-length profile keeps the last pressure at each position. Earlier
    // stationary dabs remain part of a swept area, even if the pointer then moves
    // away at zero pressure; preserve those without inventing a taper on that move.
    if !request.rectangle {
        for pair in request.samples.windows(2).filter(|p| p[0].point == p[1].point && p[0].pressure != p[1].pressure) {
            let dab = sweep::outline(pair, request.capture.options.tip).map_err(error)?;
            if !dab.is_empty() {
                region = ops::boolean(&region, &dab, BoolOp::Union).map_err(error)?;
            }
        }
        sweep::validate_path(&region).map_err(error)?;
    }
    let base = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let c = &request.capture;
    let mut outputs: Vec<(Option<LayerId>, Layer)> = Vec::new();
    let mut remove = Vec::new();
    if !region.is_empty() {
        let mut merged = region.clone();
        for id in &c.targets {
            let layer = base.layer(*id).ok_or(EngineError::NoLayer(*id))?;
            let LayerContent::Shape(sh) = &layer.content else {
                return Err(error("shape target required"));
            };
            let sh = super::corners::project_shape(sh, &layer.vector.corners)?;
            if c.mode == Mode::Blob {
                let path = normalize(base, &sh.path)?;
                if ops::area(&ops::boolean(&path, &merged, BoolOp::Intersect).map_err(error)?).map_err(error)? <= 1e-10 {
                    continue;
                }
                merged = ops::boolean(&merged, &path, BoolOp::Union).map_err(error)?;
                remove.push(*id);
            } else {
                let mut closed = sh.path.clone();
                closed.subpaths.retain(|sp| sp.closed);
                let mut open = sh.path.clone();
                open.subpaths.retain(|sp| !sp.closed);
                open.inverted = false;
                let mut pieces = Vec::new();
                let mut changed = false;
                if !closed.is_empty() || closed.inverted {
                    let path = normalize(base, &closed)?;
                    changed = ops::area(&ops::boolean(&path, &region, BoolOp::Intersect).map_err(error)?).map_err(error)? > 1e-10;
                    let difference = if changed { ops::boolean(&path, &region, BoolOp::Difference).map_err(error)? } else { path };
                    pieces.extend(components(&difference));
                }
                if !open.is_empty() {
                    let (cut, out) = erase_open(&open, &region)?;
                    changed |= cut;
                    pieces.extend(out);
                }
                if !changed {
                    continue;
                }
                remove.push(*id);
                for (i, path) in pieces.into_iter().enumerate() {
                    let mut output = layer.clone();
                    if i > 0 {
                        output.id = LayerId::fresh();
                    }
                    let LayerContent::Shape(sh) = &mut output.content else {
                        return Err(error("shape output required"));
                    };
                    sh.path = path;
                    outputs.push((Some(*id), output));
                }
            }
        }
        if c.mode == Mode::Blob {
            // Imported overlapping same-appearance shapes may precede the first hit in
            // paint order. Resolve the whole connected merge component, independent of order.
            loop {
                let count = remove.len();
                for id in c.targets.iter().filter(|id| !remove.contains(id)).copied().collect::<Vec<_>>() {
                    let layer = base.layer(id).ok_or(EngineError::NoLayer(id))?;
                    let LayerContent::Shape(sh) = &layer.content else {
                        return Err(error("shape target required"));
                    };
                    let sh = super::corners::project_shape(sh, &layer.vector.corners)?;
                    let path = normalize(base, &sh.path)?;
                    if ops::area(&ops::boolean(&path, &merged, BoolOp::Intersect).map_err(error)?).map_err(error)? > 1e-10 {
                        merged = ops::boolean(&merged, &path, BoolOp::Union).map_err(error)?;
                        remove.push(id);
                    }
                }
                if count == remove.len() {
                    break;
                }
            }
            remove.sort_by_key(|id| base.path_of(*id));
            let keep = remove.last().copied();
            let mut layer = if let Some(id) = keep {
                base.layer(id).cloned().ok_or(EngineError::NoLayer(id))?
            } else {
                let mut l = Layer::new("Blob Brush", LayerContent::Shape(photocraft_doc::ShapeLayer { fill: Some(paint(c.color)), ..Default::default() }));
                l.id = c.new_layer;
                l
            };
            let LayerContent::Shape(sh) = &mut layer.content else {
                return Err(error("shape output required"));
            };
            sh.path = merged;
            outputs.push((keep.or(c.above), layer));
        }
    }
    if outputs.len() > MAX_OUTPUTS {
        return Err(error("eraser output budget exceeded"));
    }
    // Preflight total cache size before allocating any output cache.
    let mut bytes = 0;
    let mut knots = 0;
    for (_, layer) in &outputs {
        if let LayerContent::Shape(sh) = &layer.content {
            sweep::validate_path(&sh.path).map_err(error)?;
            knots += sh.path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
            if knots > sweep::MAX_KNOTS {
                return Err(error("eraser total output geometry budget exceeded"));
            }
            let compiled =
                CompiledAppearance::with_projection(sh, layer.vector.appearance.as_ref(), &base.patterns, base.bounds(), &Projection).map_err(error)?;
            bytes += compiled.cache_bytes(base.pixel_format());
            if bytes > MAX_CACHE_BYTES {
                return Err(error("Blob Brush / Eraser total cache budget exceeded"));
            }
        }
    }
    for (_, layer) in &mut outputs {
        rendered(base, layer)?;
    }
    let changed = !remove.is_empty() || !outputs.is_empty();
    let mut document = (**base).clone();
    let mut last = std::collections::BTreeMap::new();
    let mut retained = BTreeSet::new();
    let mut layers = Vec::new();
    for (anchor, layer) in outputs {
        if Some(layer.id) == anchor {
            retained.insert(layer.id);
            *document.layer_mut(layer.id).ok_or(EngineError::NoLayer(layer.id))? = layer.clone();
        } else {
            document.insert_above(anchor.map(|a| last.get(&a).copied().unwrap_or(a)), layer.clone());
        }
        if let Some(anchor) = anchor {
            last.insert(anchor, layer.id);
        }
        layers.push(layer.id);
    }
    for id in remove {
        if !retained.contains(&id) {
            document.remove(id).ok_or(EngineError::NoLayer(id))?;
        }
    }
    Ok(Prepared { document: Arc::new(document), region, layers, changed, request: request.clone() })
}
fn decode(p: &Value) -> Result<Request> {
    for name in ["targets", "selected"] {
        if p.get("capture").and_then(|c| c.get(name)).and_then(Value::as_array).is_none_or(|a| a.len() > MAX_TARGETS) {
            return Err(error("bounded target arrays required"));
        }
    }
    if p.get("samples").and_then(Value::as_array).is_none_or(|a| a.is_empty() || a.len() > sweep::MAX_SAMPLES) {
        return Err(error("bounded samples array required"));
    }
    serde_json::from_value(p.clone()).map_err(error)
}
fn apply(s: &mut Session, p: &Value) -> Result<Value> {
    let request = decode(p)?;
    let prepared = prepare(s, &request)?;
    request.capture.validate(s)?;
    if !prepared.changed {
        return Ok(json!({"layers":[],"changed":false}));
    }
    let ids = prepared.layers;
    let selected = if request.capture.mode == Mode::Eraser || request.capture.options.keep_selected {
        ids.clone()
    } else {
        request.capture.selected.iter().copied().filter(|id| prepared.document.layer(*id).is_some() && !ids.contains(id)).collect()
    };
    let active = if request.capture.mode == Mode::Blob && !request.capture.options.keep_selected {
        request.capture.above.filter(|id| prepared.document.layer(*id).is_some() && !ids.contains(id))
    } else {
        ids.first().copied()
    };
    let doc = prepared.document;
    if request.capture.mode == Mode::Blob {
        photocraft_doc::ensure_ids_above(request.capture.new_layer.0);
    }
    s.edit(request.capture.mode.label(), move |d, a| {
        *d = (*doc).clone();
        *a = active;
        Ok(())
    })?;
    if let Some(st) = s.active_mut() {
        st.selected_layers = selected.clone();
        st.layer_anchor = selected.first().copied();
        st.history.set_current_layers(photocraft_ops::LayerTarget { active: st.active_layer, selected });
    }
    Ok(json!({"layers":ids,"changed":true}))
}
fn blob(s: &mut Session, p: &Value) -> Result<Value> {
    if decode(p)?.capture.mode != Mode::Blob {
        return Err(error("blob capture required"));
    }
    apply(s, p)
}
fn erase(s: &mut Session, p: &Value) -> Result<Value> {
    if decode(p)?.capture.mode != Mode::Eraser {
        return Err(error("eraser capture required"));
    }
    apply(s, p)
}
fn preview(s: &mut Session, p: &Value) -> Result<Value> {
    let prepared = prepare(s, &decode(p)?)?;
    Ok(json!({"region":prepared.region,"layers":prepared.layers,"changed":prepared.changed,"detached":true}))
}
pub fn specs() -> Vec<CommandSpec> {
    [
        ("path.blob", "Blob Brush", blob as fn(&mut Session, &Value) -> Result<Value>, true),
        ("path.eraseRegion", "Vector Eraser", erase, true),
        ("path.blobEraser.preview", "Preview Blob Brush / Eraser", preview, false),
    ]
    .into_iter()
    .map(|(id, label, run, journal)| CommandSpec {
        id,
        label,
        run,
        journal,
        menu: &[],
        shortcut: None,
        params: "{capture:{document,revision,targets,selected,above,new_layer,color,mode,options,excluded},samples:[{point:{x,y},pressure}],rectangle?:bool}",
        enabled: crate::vector_cmds::has_doc,
    })
    .collect()
}
#[cfg(test)]
mod gpu_tests;
#[cfg(test)]
mod tests;
