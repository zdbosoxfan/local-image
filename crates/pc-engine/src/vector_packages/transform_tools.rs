//! Reference-point transform transactions adapted from VectorCraft's tools/xform/transform.rs,
//! tools/xform/free.rs, engine/cmd/{object,menucmds,path}.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
//! Previews are detached; commits preflight all targets and publish one Session::edit.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::appearance::{AppearanceItem, StrokeItem};
use photocraft_doc::{Document, Fill, Layer, LayerContent, LayerId, Path};
use photocraft_geom::{Affine, Point};
use photocraft_pathops::geom::Affine as KurboAffine;
use photocraft_vector::{
    appearance::{self, CompiledAppearance, StrokeProjection},
    projective::{self, Map},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
mod conversion;
mod metadata;
pub use conversion::{ConversionChoice, EnvelopeRequest, EnvelopeRouter, conversion_targets};

pub const MAX_TARGETS: usize = 1024;
pub const MAX_CACHE_BYTES: u128 = 512 << 20;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub scale_strokes_effects: bool,
    pub scale_corners: bool,
    pub objects: bool,
    pub patterns: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self { scale_strokes_effects: false, scale_corners: true, objects: true, patterns: true }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub layers: Vec<LayerId>,
}
impl Capture {
    pub fn begin(s: &Session, ids: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        preflight(&st.doc, ids)?;
        Ok(Self { document: st.doc.id.0, revision: st.revision, layers: ids.to_vec() })
    }
    pub fn check(&self, s: &Session) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if st.doc.id.0 != self.document || st.revision != self.revision {
            return Err(error("transform gesture is stale"));
        }
        preflight(&st.doc, &self.layers)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Operation {
    Affine {
        matrix: [f64; 6],
    },
    Distort {
        source: [f64; 4],
        quad: [[f64; 2]; 4],
        perspective: bool,
        #[serde(default = "tolerance")]
        tolerance: f64,
    },
    Each(Each),
    Reshape {
        nodes: Vec<Node>,
        delta: [f64; 2],
        radius: f64,
    },
}
fn tolerance() -> f64 {
    0.25
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub layer: LayerId,
    pub subpath: usize,
    pub knot: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Each {
    pub scale: [f64; 2],
    pub movement: [f64; 2],
    pub rotation: f64,
    pub reflect: [bool; 2],
    pub reference: usize,
    pub random: bool,
    pub seed: u64,
}
impl Default for Each {
    fn default() -> Self {
        Self { scale: [1.; 2], movement: [0.; 2], rotation: 0., reflect: [false; 2], reference: 4, random: false, seed: 0x9E37_79B9_7F4A_7C15 }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub capture: Capture,
    pub operation: Operation,
    #[serde(default)]
    pub options: Options,
    #[serde(default)]
    pub copy: bool,
    #[serde(default)]
    pub conversion: ConversionChoice,
}
pub(crate) fn error(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}
pub fn finite(v: f64, limit: f64) -> Result<()> {
    if !v.is_finite() || v.abs() > limit {
        return Err(error("transform input must be finite and bounded"));
    }
    Ok(())
}
pub fn validate_matrix(m: [f64; 6]) -> Result<Affine> {
    for (i, v) in m.into_iter().enumerate() {
        finite(v, if i < 4 { 1000. } else { 1e7 })?;
    }
    let a = Affine { m };
    let x = m[0].hypot(m[1]);
    let y = m[2].hypot(m[3]);
    if x < 1e-4 || y < 1e-4 || a.determinant().abs() < 1e-8 {
        return Err(error("transform must be invertible"));
    }
    Ok(a)
}
pub fn about(origin: Point, matrix: KurboAffine) -> Result<[f64; 6]> {
    finite(origin.x, 1e7)?;
    finite(origin.y, 1e7)?;
    let a = KurboAffine::translate((origin.x, origin.y)) * matrix * KurboAffine::translate((-origin.x, -origin.y));
    let m = a.as_coeffs();
    validate_matrix(m)?;
    Ok(m)
}
pub fn rotate(origin: Point, degrees: f64) -> Result<[f64; 6]> {
    finite(degrees, 36000.)?;
    about(origin, KurboAffine::rotate(-degrees.to_radians()))
}
pub fn reflect(origin: Point, degrees: f64) -> Result<[f64; 6]> {
    finite(degrees, 36000.)?;
    let t = -degrees.to_radians();
    about(origin, KurboAffine::rotate(t) * KurboAffine::scale_non_uniform(1., -1.) * KurboAffine::rotate(-t))
}
pub fn scale(origin: Point, x: f64, y: f64) -> Result<[f64; 6]> {
    about(origin, KurboAffine::scale_non_uniform(x, y))
}
pub fn shear(origin: Point, degrees: f64, vertical: bool) -> Result<[f64; 6]> {
    finite(degrees, 88.99)?;
    let t = -degrees.to_radians().tan();
    about(origin, KurboAffine::new(if vertical { [1., t, 0., 1., 0., 0.] } else { [1., 0., t, 1., 0., 0.] }))
}
/// Illustrator angles are positive counter-clockwise; document pixels have a downward Y axis.
pub fn move_by(distance: f64, degrees: f64) -> Result<[f64; 6]> {
    finite(distance, 1e7)?;
    finite(degrees, 36000.)?;
    let t = degrees.to_radians();
    let m = [1., 0., 0., 1., distance * t.cos(), -distance * t.sin()];
    validate_matrix(m)?;
    Ok(m)
}
pub fn preflight(doc: &Document, ids: &[LayerId]) -> Result<()> {
    if ids.is_empty() || ids.len() > MAX_TARGETS {
        return Err(error("transform needs 1..1024 targets"));
    }
    let set: HashSet<_> = ids.iter().copied().collect();
    if set.len() != ids.len() {
        return Err(error("duplicate transform target"));
    }
    let mut count = 0;
    fn check(doc: &Document, l: &Layer, count: &mut usize) -> Result<()> {
        *count += 1;
        if *count > MAX_TARGETS {
            return Err(error("transform exceeds 1024 objects"));
        }
        let locks = doc.effective_locks(l.id);
        if locks.all || locks.pixels || locks.position {
            return Err(error("transform target is locked"));
        }
        if l.vector.live.is_some() {
            return Err(error("expand live groups before transforming"));
        }
        match &l.content {
            LayerContent::Shape(sh) => {
                validate_path(&sh.path)?;
                photocraft_pathops::validate(&sh.path).map_err(|e| error(e.to_string()))?;
            }
            LayerContent::Group(g) => {
                for c in &g.children {
                    check(doc, c, count)?;
                }
                if g.artboard.is_some() {
                    return Err(error("use the Artboard tool for artboards"));
                }
            }
            LayerContent::Text(_) | LayerContent::Symbol(_) => {}
            _ => return Err(error("select vector layers for these transform tools")),
        }
        Ok(())
    }
    for id in ids {
        let path = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
        if (1..path.len()).any(|n| doc.layer_at(&path[..n]).is_some_and(|l| set.contains(&l.id))) {
            return Err(error("transform selection contains both parent and child"));
        }
        check(doc, doc.layer(*id).ok_or(EngineError::NoLayer(*id))?, &mut count)?;
    }
    Ok(())
}
pub fn bounds(doc: &Document, ids: &[LayerId]) -> Result<[f64; 4]> {
    super::layout::selection_bounds(doc, ids)
}
struct Projection;
impl StrokeProjection for Projection {
    fn outline(&self, path: &Path, stroke: &StrokeItem) -> std::result::Result<Path, String> {
        if stroke.brush.is_some() {
            photocraft_vecfx::brush::outline(path, stroke).map_err(|e| e.to_string())
        } else {
            photocraft_vecfx::width::outline(path, stroke)
        }
    }
}
fn validate_path(p: &Path) -> Result<()> {
    crate::vector_api::validate_path(p)?;
    if p.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > projective::MAX_NODES {
        return Err(error("transform path exceeds node budget"));
    }
    for p in p.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]) {
        finite(p.x, 1e6)?;
        finite(p.y, 1e6)?;
    }
    Ok(())
}
fn mapping(operation: &Operation) -> Result<Option<Map>> {
    match operation {
        Operation::Distort { source, quad, perspective, tolerance } => {
            if !tolerance.is_finite() || !(1e-6..=0.25).contains(tolerance) {
                return Err(error("distortion tolerance must be 0.000001..0.25 px"));
            }
            let q = quad.map(|p| Point::new(p[0], p[1]));
            Ok(Some(if *perspective { Map::perspective(*source, q) } else { Map::bilinear(*source, q) }.map_err(|e| error(e.to_string()))?))
        }
        _ => Ok(None),
    }
}
fn transform_path(path: &Path, a: Option<Affine>, map: Option<Map>, tol: f64) -> Result<Path> {
    let out = if let Some(a) = a {
        path.transform(&a)
    } else if let Some(map) = map {
        projective::map_path(path, map, tol).map_err(|e| error(e.to_string()))?.path
    } else {
        path.clone()
    };
    validate_path(&out)?;
    Ok(out)
}
fn reshape(path: &Path, id: LayerId, nodes: &[Node], delta: [f64; 2], radius: f64) -> Result<Path> {
    finite(radius, 1e6)?;
    if radius <= 0. {
        return Err(error("reshape radius must be positive"));
    }
    for v in delta {
        finite(v, 1e6)?;
    }
    if nodes.is_empty() || nodes.len() > projective::MAX_NODES {
        return Err(error("select nodes to reshape"));
    }
    let selected: HashSet<_> = nodes.iter().filter(|n| n.layer == id).map(|n| (n.subpath, n.knot)).collect();
    let mut origins = Vec::new();
    for &(si, ki) in &selected {
        let p = path.subpaths.get(si).and_then(|s| s.knots.get(ki)).ok_or_else(|| error("reshape node is missing"))?.anchor;
        origins.push((si, p));
    }
    let mut out = path.clone();
    for (si, s) in out.subpaths.iter_mut().enumerate() {
        for (ki, k) in s.knots.iter_mut().enumerate() {
            let weight = if selected.contains(&(si, ki)) {
                1.
            } else {
                origins
                    .iter()
                    .filter(|(sub, _)| *sub == si)
                    .map(|(_, p)| {
                        let q = ((k.anchor.x - p.x).hypot(k.anchor.y - p.y) / radius).min(1.);
                        (1. - q * q).powi(2)
                    })
                    .fold(0., f64::max)
            };
            let a = Affine::translate(delta[0] * weight, delta[1] * weight);
            *k = k.transform(&a);
        }
    }
    validate_path(&out)?;
    Ok(out)
}
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}
fn each_matrix(l: &Layer, each: &Each, rng: &mut Rng) -> Result<Affine> {
    for v in each.scale {
        finite(v, 1000.)?;
        if v.abs() < 1e-4 {
            return Err(error("Transform Each scale must be nonzero"));
        }
    }
    for v in each.movement {
        finite(v, 1e7)?;
    }
    finite(each.rotation, 36000.)?;
    if each.reference > 8 {
        return Err(error("Transform Each reference must be 0..8"));
    }
    let [x, y, w, h] = super::layout::bounds(l)?;
    let o = Point::new(x + w * (each.reference % 3) as f64 / 2., y + h * (each.reference / 3) as f64 / 2.);
    let mut s = each.scale;
    let mut d = each.movement;
    let mut r = each.rotation;
    if each.random {
        for v in &mut s {
            *v = 1. + (*v - 1.) * rng.next();
        }
        for v in &mut d {
            *v *= rng.next() * 2. - 1.;
        }
        r *= rng.next() * 2. - 1.;
    }
    if each.reflect[0] {
        s[1] = -s[1];
    }
    if each.reflect[1] {
        s[0] = -s[0];
    }
    let m = about(o, KurboAffine::rotate(-r.to_radians()) * KurboAffine::scale_non_uniform(s[0], s[1]))?;
    let mut a = Affine { m };
    a.m[4] += d[0];
    a.m[5] += d[1];
    validate_matrix(a.m)
}
fn distortion_scale(operation: &Operation) -> f64 {
    if let Operation::Distort { source, quad, .. } = operation {
        let area = (0..4).map(|i| quad[i][0] * quad[(i + 1) % 4][1] - quad[i][1] * quad[(i + 1) % 4][0]).sum::<f64>().abs() * 0.5;
        (area / (source[2] * source[3])).sqrt()
    } else {
        1.
    }
}
// Existing type/symbol providers rasterize during their affine transform. Bound the
// entire set before calling them; shape plans are charged later before shape rendering.
fn auxiliary_bytes(doc: &Document, layer: &Layer, a: Affine) -> Result<u128> {
    fn frame(rect: [f64; 4], a: Affine, bytes_per_pixel: usize) -> Result<u128> {
        let [x, y, w, h] = rect;
        let points = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)].map(|(x, y)| a.apply(Point::new(x, y)));
        for p in points {
            finite(p.x, 1e7)?;
            finite(p.y, 1e7)?;
        }
        let x0 = (points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min) - 1.).floor() as i64;
        let y0 = (points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min) - 1.).floor() as i64;
        let x1 = (points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max) + 1.).ceil() as i64;
        let y1 = (points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max) + 1.).ceil() as i64;
        let tile = i64::from(photocraft_geom::TILE_SIZE);
        Ok(((x1 - 1).div_euclid(tile) - x0.div_euclid(tile) + 1) as u128
            * ((y1 - 1).div_euclid(tile) - y0.div_euclid(tile) + 1) as u128
            * tile as u128
            * tile as u128
            * bytes_per_pixel as u128)
    }
    let mut bytes = 0;
    match &layer.content {
        LayerContent::Text(_) | LayerContent::Symbol(_) => {
            let format = layer.surface().map_or(doc.pixel_format(), |s| s.format());
            bytes += frame(super::layout::bounds(layer)?, a, format.bytes_per_pixel())?;
            if let Some(mask) = &layer.mask {
                let r = mask.surface.content_bounds();
                bytes += frame(
                    [f64::from(r.x0), f64::from(r.y0), f64::from(r.width()), f64::from(r.height())],
                    if mask.linked { a } else { Affine::IDENTITY },
                    mask.surface.format().bytes_per_pixel(),
                )?;
            }
        }
        LayerContent::Group(g) => {
            return g.children.iter().try_fold(0_u128, |sum, child| Ok(sum + auxiliary_bytes(doc, child, a)?));
        }
        _ => {}
    }
    // A conservative allowance for provider scratch buffers and antialias bounds.
    Ok(bytes * 2)
}
fn transform_layer(doc: &Document, mut l: Layer, c: &Commit, a: Option<Affine>, map: Option<Map>, tol: f64) -> Result<Layer> {
    if let LayerContent::Group(g) = &mut l.content {
        for child in &mut g.children {
            *child = transform_layer(doc, child.clone(), c, a, map, tol)?;
        }
        if c.options.objects {
            if let Some(clip) = &l.vector.clip {
                l.vector.clip = Some(transform_path(clip, a, map, tol)?);
            }
            if let Some(mask) = &mut l.vector_mask
                && mask.linked
            {
                mask.path = transform_path(&mask.path, a, map, tol)?;
            }
            if l.mask.as_ref().is_some_and(|m| m.linked) {
                return Err(error("Unlink the pixel mask before transforming this shape or group"));
            }
            if c.options.scale_strokes_effects {
                metadata::effects(&mut l.effects, a.map_or_else(|| distortion_scale(&c.operation), |a| a.determinant().abs().sqrt()))?;
            }
        }
        return Ok(l);
    }
    if !matches!(l.content, LayerContent::Shape(_)) {
        if map.is_some() || matches!(c.operation, Operation::Reshape { .. }) {
            l = conversion::convert(doc, l, c.conversion)?;
            return transform_layer(doc, l, c, a, map, tol);
        }
        if let Some(a) = a {
            if c.options.objects {
                l = super::layout::transformed(doc, &l, a)?;
                if c.options.scale_strokes_effects {
                    metadata::effects(&mut l.effects, a.determinant().abs().sqrt())?;
                }
            }
            return Ok(l);
        }
    }
    if l.vector.on_path.is_some() {
        return Err(error("convert attached path text before changing its path"));
    }
    let LayerContent::Shape(sh) = &mut l.content else {
        return Err(error("transform requires vector geometry"));
    };
    let old_paint_geometry = super::corners::project_shape(sh, &l.vector.corners)?.path;
    let old_bounds = old_paint_geometry.control_bounds();
    let k = a.map_or_else(|| distortion_scale(&c.operation), |a| a.determinant().abs().sqrt());
    if c.options.objects {
        if let Operation::Reshape { nodes, delta, radius } = &c.operation {
            sh.path = reshape(&sh.path, l.id, nodes, *delta, *radius)?;
            l.vector.corners.clear();
            l.vector.live_shape = None;
            sh.live = None;
        } else {
            let similarity = a.is_some_and(metadata::similarity);
            let bake_corners_after = !similarity && a.is_some() && !c.options.scale_corners && !l.vector.corners.is_empty();
            if !similarity && !l.vector.corners.is_empty() && !bake_corners_after {
                sh.path = super::corners::project_shape(sh, &l.vector.corners)?.path;
                l.vector.corners.clear();
            }
            sh.path = transform_path(&sh.path, a, map, tol)?;
            if bake_corners_after {
                // Preserve absolute corner radii on the transformed base before baking.
                sh.path = super::corners::project_shape(sh, &l.vector.corners)?.path;
                l.vector.corners.clear();
            }
            if let Some(a) = a {
                metadata::live(sh, &mut l.vector, a, c.options.scale_corners)?;
            } else {
                sh.live = None;
                l.vector.live_shape = None;
                l.vector.corners.clear();
            }
        }
        if let Some(clip) = &l.vector.clip {
            l.vector.clip = Some(transform_path(clip, a, map, tol)?);
        }
        if let Some(mask) = &mut l.vector_mask
            && mask.linked
        {
            mask.path = transform_path(&mask.path, a, map, tol)?;
        }
        if l.mask.as_ref().is_some_and(|m| m.linked) {
            return Err(error("Unlink the pixel mask before transforming this shape or group"));
        }
    }
    if c.options.scale_strokes_effects && c.options.objects {
        if let Some(s) = &mut sh.stroke {
            s.width *= k as f32;
        }
        metadata::effects(&mut l.effects, k)?;
    }
    let paint_geometry = super::corners::project_shape(sh, &l.vector.corners)?.path;
    if let Some(ap) = &mut l.vector.appearance {
        for item in &mut ap.items {
            match item {
                AppearanceItem::Stroke(s) => {
                    if c.options.scale_strokes_effects && c.options.objects {
                        s.base.width *= k as f32;
                        if let Some(profile) = &mut s.profile {
                            for point in &mut profile.points {
                                point.left *= k;
                                point.right *= k;
                            }
                        }
                    }
                    metadata::paint(&mut s.base.paint, a, c.options.patterns, old_bounds, &paint_geometry)?;
                }
                AppearanceItem::Fill(f) => metadata::paint(&mut f.paint, a, c.options.patterns, old_bounds, &paint_geometry)?,
            }
        }
        appearance::validate(ap).map_err(error)?;
    }
    if let Some(f) = &mut sh.fill {
        metadata::paint(f, a, c.options.patterns, old_bounds, &paint_geometry)?;
    }
    if let Some(s) = &mut sh.stroke {
        metadata::paint(&mut s.paint, a, c.options.patterns, old_bounds, &paint_geometry)?;
    }
    sh.psd_raw = None;
    l.fill_cache = None;
    validate_path(&sh.path)?;
    Ok(l)
}
pub struct Prepared {
    pub document: Document,
    pub targets: Vec<LayerId>,
    capture: Capture,
}
fn reid(l: &mut Layer) {
    l.id = LayerId::fresh();
    l.psd_id = None;
    l.psd_blocks.clear();
    if let LayerContent::Group(g) = &mut l.content {
        for child in &mut g.children {
            reid(child);
        }
    }
}
/// Builds all geometry, then all bounded render plans, then caches. Nothing is published.
pub fn prepare(s: &Session, c: &Commit) -> Result<Prepared> {
    c.capture.check(s)?;
    if !c.options.objects && !c.options.patterns {
        return Err(error("enable Transform Objects or Patterns"));
    }
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let map = mapping(&c.operation)?;
    let tol = match c.operation {
        Operation::Distort { tolerance, .. } => tolerance,
        _ => 0.25,
    };
    let a = match c.operation {
        Operation::Affine { matrix } => Some(validate_matrix(matrix)?),
        _ => None,
    };
    if let Operation::Reshape { nodes, .. } = &c.operation {
        let seen: HashSet<_> = nodes.iter().collect();
        if seen.len() != nodes.len() {
            return Err(error("duplicate reshape node"));
        }
        for node in nodes {
            let l = doc.layer(node.layer).ok_or(EngineError::NoLayer(node.layer))?;
            if !c
                .capture
                .layers
                .iter()
                .any(|id| *id == node.layer || doc.path_of(node.layer).zip(doc.path_of(*id)).is_some_and(|(p, root)| p.starts_with(&root)))
            {
                return Err(error("reshape node is outside captured targets"));
            }
            let LayerContent::Shape(sh) = &l.content else {
                return Err(error("reshape requires shape nodes"));
            };
            if sh.path.subpaths.get(node.subpath).and_then(|s| s.knots.get(node.knot)).is_none() {
                return Err(error("reshape node is missing"));
            }
        }
    }
    let mut rng = Rng(match &c.operation {
        Operation::Each(e) => e.seed.max(1),
        _ => 1,
    });
    let mut plans = Vec::new();
    let mut bytes = 0;
    for id in &c.capture.layers {
        let layer = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
        let a = if let Operation::Each(e) = &c.operation { Some(each_matrix(layer, e, &mut rng)?) } else { a };
        if c.options.objects
            && let Some(a) = a
        {
            bytes += auxiliary_bytes(doc, layer, a)?;
            if bytes > MAX_CACHE_BYTES {
                return Err(error("transform caches exceed 512 MiB"));
            }
        }
        plans.push((*id, layer, a));
    }
    let mut replacements = Vec::new();
    for (id, layer, a) in plans {
        replacements.push((id, transform_layer(doc, layer.clone(), c, a, map, tol)?));
    }
    fn cache(doc: &Document, l: &mut Layer, bytes: &mut u128, render: bool) -> Result<()> {
        match &mut l.content {
            LayerContent::Shape(sh) => {
                let projected = super::corners::project_shape(sh, &l.vector.corners)?;
                let compiled =
                    CompiledAppearance::with_projection(&projected, l.vector.appearance.as_ref(), &doc.patterns, doc.bounds(), &Projection).map_err(error)?;
                if !render {
                    *bytes += compiled.cache_bytes(doc.pixel_format());
                    if *bytes > MAX_CACHE_BYTES {
                        return Err(error("transform caches exceed 512 MiB"));
                    }
                } else {
                    sh.cache = Some(compiled.render(doc.pixel_format(), doc.bounds()).map_err(error)?);
                    if let Some(ap) = &l.vector.appearance {
                        appearance::mirror_legacy(sh, ap);
                    }
                }
            }
            LayerContent::Group(g) => {
                for child in &mut g.children {
                    cache(doc, child, bytes, render)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for (_, l) in &mut replacements {
        cache(doc, l, &mut bytes, false)?;
    }
    for (_, l) in &mut replacements {
        cache(doc, l, &mut bytes, true)?;
    }
    let mut detached = (**doc).clone();
    let mut targets = Vec::new();
    for (id, mut l) in replacements {
        if c.copy {
            reid(&mut l);
            targets.push(l.id);
            detached.insert_above(Some(id), l);
        } else {
            targets.push(id);
            *detached.layer_mut(id).ok_or(EngineError::NoLayer(id))? = l;
        }
    }
    Ok(Prepared { document: detached, targets, capture: c.capture.clone() })
}
fn request(v: &Value) -> Result<Commit> {
    serde_json::from_value(v.clone()).map_err(|e| error(format!("invalid transform request: {e}")))
}
fn apply(s: &mut Session, v: &Value) -> Result<Value> {
    let c = request(v)?;
    let prepared = prepare(s, &c)?;
    prepared.capture.check(s)?;
    let ids = prepared.targets;
    let active = ids.last().copied();
    s.edit("Transform Vector Objects", move |doc, selection| {
        *doc = prepared.document;
        *selection = active;
        Ok(())
    })?;
    if let Some(st) = s.active_mut() {
        st.selected_layers = ids.clone();
    }
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()}))
}
fn preview(s: &mut Session, v: &Value) -> Result<Value> {
    let p = prepare(s, &request(v)?)?;
    Ok(json!({"detached":true,"layers":p.targets.iter().map(|id|id.0).collect::<Vec<_>>()}))
}
pub fn commit(s: &mut Session, c: &Commit) -> Result<Vec<LayerId>> {
    let value = s.execute("vector.transform.apply", serde_json::to_value(c).map_err(|e| error(e.to_string()))?)?;
    serde_json::from_value(value["layers"].clone()).map_err(|e| error(e.to_string()))
}
/// Bridge for 00: document-scoped vector replay; Pixel's edit.transform state is untouched.
/// Replays the last successful vector command's operation/options/copy on the current targets.
pub fn again_request(s: &Session, ids: &[LayerId]) -> Result<Commit> {
    let capture = Capture::begin(s, ids)?;
    let mut previous = s
        .journal
        .iter()
        .rev()
        .filter(|(id, _)| id == "vector.transform.apply")
        .filter_map(|(_, v)| request(v).ok())
        .find(|c| c.capture.document == capture.document)
        .ok_or_else(|| error("no previous vector transform"))?;
    if let Operation::Reshape { nodes, .. } = &mut previous.operation {
        if previous.capture.layers.len() != capture.layers.len() {
            return Err(error("repeat Reshape needs the same number of target objects"));
        }
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        for node in nodes {
            let node_path = doc.path_of(node.layer).ok_or(EngineError::NoLayer(node.layer))?;
            let (index, old_path) = previous
                .capture
                .layers
                .iter()
                .enumerate()
                .find_map(|(index, root)| doc.path_of(*root).filter(|p| node_path.starts_with(p)).map(|p| (index, p)))
                .ok_or_else(|| error("repeat Reshape source is missing"))?;
            let mut new_path = doc.path_of(capture.layers[index]).ok_or(EngineError::NoLayer(capture.layers[index]))?;
            new_path.extend_from_slice(&node_path[old_path.len()..]);
            node.layer = doc.layer_at(&new_path).ok_or_else(|| error("repeat Reshape target structure changed"))?.id;
        }
    }
    previous.capture = capture;
    Ok(previous)
}
fn again(s: &mut Session, v: &Value) -> Result<Value> {
    let ids: Vec<LayerId> = serde_json::from_value(v.get("layers").cloned().unwrap_or_else(|| json!(s.active().map(|st| st.selected_layers()))))
        .map_err(|e| error(e.to_string()))?;
    let c = again_request(s, &ids)?;
    // The outer command journals repeat; the previous apply remains the repeat source.
    apply(s, &serde_json::to_value(c).map_err(|e| error(e.to_string()))?)
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    [
        ("vector.transform.apply", "Transform Vector Objects", true, apply as fn(&mut Session, &Value) -> Result<Value>),
        ("vector.transform.preview", "Preview Vector Transform", false, preview),
        ("vector.transform.again", "Transform Again", true, again),
    ]
    .into_iter()
    .map(|(id, label, journal, run)| CommandSpec {
        id,
        label,
        journal,
        run,
        menu: &[],
        shortcut: None,
        params: "Commit {capture,operation,options,copy,conversion}; again {layers?}",
        enabled: crate::vector_cmds::has_doc,
    })
    .collect()
}
#[cfg(test)]
#[path = "transform_tools/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transform_tools/gpu_tests.rs"]
mod gpu_tests;
