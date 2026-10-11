//! Region painting on V1's fallible planar arrangement. Local Image, GPL-3.0-or-later.
//! Face/edge initialization and paint projection follow VectorCraft buildcmds.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092, MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
pub mod gaps;
mod projection;
pub mod signatures;
pub use projection::{project_document, project_scene, read_spec, write_spec};

use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
use photocraft_doc::{Fill, Layer, LayerContent, LayerId, Path, ShapeLayer, ShapeStroke};
use photocraft_geom::Point;
use photocraft_pathops::{
    self as ops,
    geom::{ParamCurve, Shape as _},
};
pub mod data;
pub use data::{Boundary, EdgePaint, FacePaint, Gap, LivePaintSpec, Signature};
use std::collections::{BTreeSet, HashSet};

pub const MAX_SOURCES: usize = 128;
pub const MAX_SEGMENTS: usize = 4096;
pub const MAX_REGIONS: usize = 2048;
pub const MAX_POINTS: usize = 4096;
pub const MAX_COORD: f64 = 1_000_000.;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn error(e: impl ToString) -> Error {
    Error(e.to_string())
}
pub fn point(p: Point) -> Result<()> {
    if [p.x, p.y].iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD) { Ok(()) } else { Err(error("paint coordinates must be finite and bounded")) }
}

impl LivePaintSpec {
    pub fn validate(&self) -> Result<()> {
        if self.faces.len() > MAX_REGIONS * 2 || self.edges.len() > MAX_SEGMENTS * 2 {
            return Err(error("live paint record budget exceeded"));
        }
        let mut seen = HashSet::new();
        for f in &self.faces {
            f.signature.validate()?;
            if !seen.insert(&f.signature) {
                return Err(error("duplicate face paint signature"));
            }
            if let Some(p) = &f.paint {
                validate_fill(p)?;
            }
        }
        seen.clear();
        for e in &self.edges {
            e.signature.validate()?;
            if !seen.insert(&e.signature) {
                return Err(error("duplicate edge paint signature"));
            }
            if let Some(s) = &e.stroke {
                photocraft_vector::appearance::validate(&Appearance {
                    items: vec![AppearanceItem::Stroke(StrokeItem { base: s.clone(), ..Default::default() })],
                })
                .map_err(error)?;
            }
        }
        Ok(())
    }
    pub fn set_face(&mut self, signature: Signature, paint: Option<Fill>) -> Result<()> {
        if let Some(p) = &paint {
            validate_fill(p)?;
        }
        let mut candidate = self.clone();
        if let Some(record) = candidate.faces.iter_mut().find(|f| f.signature == signature) {
            record.paint = paint;
        } else {
            candidate.faces.push(FacePaint { signature, paint });
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn set_edge(&mut self, signature: Signature, stroke: Option<ShapeStroke>) -> Result<()> {
        let mut candidate = self.clone();
        if let Some(record) = candidate.edges.iter_mut().find(|e| e.signature == signature) {
            record.stroke = stroke;
        } else {
            candidate.edges.push(EdgePaint { signature, stroke });
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
}
pub fn validate_fill(paint: &Fill) -> Result<()> {
    photocraft_vector::appearance::validate(&Appearance { items: vec![AppearanceItem::Fill(FillItem::new(paint.clone()))] }).map_err(error)
}
pub fn fill(layer: &Layer) -> Option<Fill> {
    let LayerContent::Shape(shape) = &layer.content else { return None };
    if let Some(a) = &layer.vector.appearance {
        a.items.iter().rev().find_map(|i| match i {
            AppearanceItem::Fill(f) if f.visible && f.opacity > 0. => {
                let mut shape = ShapeLayer::default();
                photocraft_vector::appearance::mirror_legacy(&mut shape, &Appearance { items: vec![i.clone()] });
                shape.fill
            }
            _ => None,
        })
    } else {
        shape.fill.clone()
    }
}
pub fn stroke(layer: &Layer) -> Option<ShapeStroke> {
    let LayerContent::Shape(shape) = &layer.content else { return None };
    layer.vector.appearance.as_ref().map_or_else(
        || shape.stroke.clone(),
        |a| {
            a.items.iter().rev().find_map(|i| match i {
                AppearanceItem::Stroke(s) if s.visible => Some(s.base.clone()),
                _ => None,
            })
        },
    )
}
#[derive(Clone, Debug)]
pub struct Face {
    pub region: ops::Region,
    pub signature: Signature,
    pub area: f64,
    bez: ops::geom::BezPath,
    bounds: ops::geom::Rect,
}
#[derive(Clone, Debug)]
pub struct Edge {
    pub path: Path,
    pub source: u64,
    pub signature: Signature,
}
#[derive(Clone, Debug)]
pub struct Graph {
    pub faces: Vec<Face>,
    pub edges: Vec<Edge>,
    pub inputs: Vec<ops::Shape>,
}
impl Graph {
    pub fn new(sources: &[Layer], gap: Gap) -> Result<Self> {
        if sources.is_empty() || sources.len() > MAX_SOURCES {
            return Err(error("region paint requires 1 to 128 curves"));
        }
        let mut ids = HashSet::new();
        let mut segments = 0;
        let mut inputs = Vec::new();
        for (slot, layer) in sources.iter().enumerate() {
            if !ids.insert(layer.id) || layer.id.0 == 0 {
                return Err(error("duplicate or zero source ID"));
            }
            let LayerContent::Shape(shape) = &layer.content else { return Err(error("live paint sources must be curves")) };
            if layer.vector.live.is_some()
                || !layer.vector.corners.is_empty()
                || layer.vector.clip.is_some()
                || layer.vector_mask.is_some()
                || layer.mask.is_some()
                || layer.clipped
            {
                return Err(error("project live geometry, corners, clipping and masks before region painting"));
            }
            if !layer.opacity.is_finite()
                || !(0. ..=1.).contains(&layer.opacity)
                || !layer.fill_opacity.is_finite()
                || !(0. ..=1.).contains(&layer.fill_opacity)
            {
                return Err(error("invalid source opacity"));
            }
            ops::validate(&shape.path).map_err(error)?;
            if shape.path.inverted {
                return Err(error("inverted paint source requires finite clip projection"));
            }
            for knot in shape.path.subpaths.iter().flat_map(|s| &s.knots) {
                for p in [knot.anchor, knot.in_ctrl, knot.out_ctrl] {
                    point(p)?;
                }
            }
            segments += shape.path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
            if segments > MAX_SEGMENTS {
                return Err(error("live paint geometry budget exceeded"));
            }
            photocraft_vector::appearance::validate(
                &layer.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(shape)),
            )
            .map_err(error)?;
            // Source slots are stable within the persisted group children; native ID remapping
            // and duplicating a group do not change its paint signatures.
            inputs.push(ops::Shape::new(shape.path.clone(), slot as u64 + 1));
        }
        inputs = gaps::close(&inputs, gap)?;
        let arrangement = if inputs.iter().flat_map(|s| &s.path.subpaths).any(|s| !s.closed) {
            ops::shape_builder(&inputs, true).map_err(error)?
        } else {
            // Shape Builder's closed-only mode intentionally excludes unfilled holes.
            // Use V1's existing full Live Paint map for those inputs. Its legacy helper
            // returns empty on sweep failure: explicitly reject that outcome, so it
            // can never become a successful destructive empty edit.
            let native = inputs
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let path = ops::finish_compound(&s.path, None).map_err(error)?;
                    Ok(ops::kernel::Shape::new(ops::to_geometry(&path), ops::geom::FillRule::NonZero, i as u64))
                })
                .collect::<Result<Vec<_>>>()?;
            let (faces, edges) = ops::kernel::live_paint(&native);
            if faces.is_empty() && edges.is_empty() {
                return Err(error("closed Live Paint planar sweep produced no usable map"));
            }
            ops::BuilderArrangement {
                regions: faces.into_iter().map(|f| ops::Region { path: ops::from_geometry(&f.path), sources: f.sources }).collect(),
                edges: edges.into_iter().map(|e| ops::Shape::new(ops::from_geometry(&e.path), e.key)).collect(),
                lines: Vec::new(),
            }
        };
        if arrangement.regions.len() > MAX_REGIONS || arrangement.edges.len() + arrangement.lines.len() > MAX_SEGMENTS * 2 {
            return Err(error("live paint arrangement budget exceeded"));
        }
        let mut faces = Vec::new();
        let mut seen = HashSet::new();
        for region in arrangement.regions {
            let signature = signatures::face(&region, &inputs)?;
            if !seen.insert(signature.clone()) {
                return Err(error("ambiguous region signature; simplify the source topology"));
            }
            let bez = ops::to_geometry(&region.path).to_bezpath();
            let bounds = bez.bounding_box();
            let area = ops::area(&region.path).map_err(error)?;
            faces.push(Face { region, signature, area, bez, bounds });
        }
        let mut paths = arrangement.edges;
        paths.extend(arrangement.lines);
        let mut edges = Vec::new();
        // Stable paint records exclude invisible gap bridges.
        for edge in paths.into_iter().filter(|e| e.key < sources.len() as u64) {
            let signature = signatures::edge(&edge.path, inputs[edge.key as usize].key, &inputs)?;
            edges.push(Edge { path: edge.path, source: sources[edge.key as usize].id.0, signature });
        }
        signatures::disambiguate_edges(&mut edges, &inputs)?;
        if faces.is_empty() && edges.is_empty() {
            return Err(error("paths enclose no paintable regions or edges"));
        }
        Ok(Self { faces, edges, inputs })
    }
    pub fn face_at(&self, p: Point) -> Result<Option<usize>> {
        point(p)?;
        let q = ops::geom::Point::new(p.x, p.y);
        Ok(self.faces.iter().position(|f| f.bounds.contains(q) && f.bez.winding(q) != 0))
    }
    pub fn edge_at(&self, p: Point, tolerance: f64) -> Result<Option<usize>> {
        point(p)?;
        if !tolerance.is_finite() || !(0. ..=1000.).contains(&tolerance) {
            return Err(error("invalid edge tolerance"));
        }
        let mut best = None;
        for (i, e) in self.edges.iter().enumerate() {
            if let Some((_, _, _, _, d)) = ops::nearest(&e.path, p).map_err(error)?
                && d <= tolerance
                && best.is_none_or(|(_, old)| d < old)
            {
                best = Some((i, d));
            }
        }
        Ok(best.map(|(i, _)| i))
    }
    /// Intersect the entire pointer segment with each face. Narrow regions are not lost
    /// between pointer frames, as they are with pixel-spaced gesture sampling.
    pub fn touched(&self, points: &[Point]) -> Result<BTreeSet<usize>> {
        if points.is_empty() || points.len() > MAX_POINTS {
            return Err(error("invalid paint gesture point count"));
        }
        for p in points {
            point(*p)?;
        }
        let mut touched = BTreeSet::new();
        for p in points {
            if let Some(i) = self.face_at(*p)? {
                touched.insert(i);
            }
        }
        for pair in points.windows(2) {
            if pair[0] == pair[1] {
                continue;
            }
            let line = kurbo::Line::new((pair[0].x, pair[0].y), (pair[1].x, pair[1].y));
            for (i, f) in self.faces.iter().enumerate() {
                if touched.contains(&i)
                    || pair[0].x.min(pair[1].x) > f.bounds.x1
                    || pair[0].x.max(pair[1].x) < f.bounds.x0
                    || pair[0].y.min(pair[1].y) > f.bounds.y1
                    || pair[0].y.max(pair[1].y) < f.bounds.y0
                {
                    continue;
                }
                let mut cuts = vec![0., 1.];
                for seg in f.bez.segments() {
                    cuts.extend(seg.intersect_line(line).iter().map(|hit| hit.line_t.clamp(0., 1.)));
                }
                cuts.sort_by(f64::total_cmp);
                if cuts.windows(2).any(|w| w[1] - w[0] > 1e-12 && f.bez.winding(line.eval((w[0] + w[1]) * 0.5)) != 0) {
                    touched.insert(i);
                }
            }
        }
        Ok(touched)
    }
    pub fn merge(&self, faces: &BTreeSet<usize>) -> Result<Path> {
        if faces.is_empty() {
            return Err(error("no faces selected"));
        }
        let regions = faces.iter().map(|i| self.faces.get(*i).map(|f| &f.region).ok_or_else(|| error("invalid face index"))).collect::<Result<Vec<_>>>()?;
        // A cutting line splits one filled coverage cell into multiple planar faces.
        // If all pieces of each selected coverage cell are selected, use V1's exact
        // closed arrangement. Reuniting the split cubic pieces through the open-path
        // fallback can refit their boundaries at Pathfinder's 0.01px precision.
        let complete_cells = regions.iter().all(|r| !r.sources.is_empty())
            && self.faces.iter().enumerate().all(|(i, f)| !regions.iter().any(|r| r.sources == f.region.sources) || faces.contains(&i));
        if complete_cells {
            let closed = self
                .inputs
                .iter()
                .map(|s| {
                    let mut path = s.path.clone();
                    path.subpaths.retain(|p| p.closed);
                    ops::Shape::new(path, s.key)
                })
                .collect::<Vec<_>>();
            return ops::merge_regions(&closed, &regions).map_err(error);
        }
        ops::merge_regions(&self.inputs, &regions).map_err(error)
    }
    pub fn face_paint(&self, i: usize, sources: &[Layer], spec: &LivePaintSpec) -> Result<Option<Fill>> {
        let f = self.faces.get(i).ok_or_else(|| error("invalid face index"))?;
        if let Some(record) = spec.faces.iter().find(|r| r.signature == f.signature) {
            return Ok(record.paint.clone());
        }
        Ok(f.region.sources.iter().rev().filter_map(|i| sources.get(*i)).find_map(fill))
    }
    pub fn edge_paint(&self, i: usize, sources: &[Layer], spec: &LivePaintSpec) -> Result<Option<ShapeStroke>> {
        let e = self.edges.get(i).ok_or_else(|| error("invalid edge index"))?;
        if let Some(record) = spec.edges.iter().find(|r| r.signature == e.signature) {
            return Ok(record.stroke.clone());
        }
        Ok(sources.iter().find(|l| l.id.0 == e.source).and_then(stroke))
    }
    pub fn visible_faces(&self, seeds: &BTreeSet<usize>, sources: &[Layer]) -> Result<BTreeSet<usize>> {
        let spec = LivePaintSpec::default();
        let paints = (0..self.faces.len()).map(|i| self.face_paint(i, sources, &spec)).collect::<Result<Vec<_>>>()?;
        let mut found = seeds.clone();
        let mut pending = seeds.iter().copied().collect::<Vec<_>>();
        while let Some(i) = pending.pop() {
            let a = self.faces.get(i).ok_or_else(|| error("invalid face index"))?;
            for (j, b) in self.faces.iter().enumerate() {
                if found.contains(&j) || paints[i] != paints[j] || a.bounds.inflate(1e-4, 1e-4).intersect(b.bounds).is_zero_area() {
                    continue;
                }
                let adjacent = a.bez.segments().any(|seg| {
                    let q = seg.eval(0.5);
                    ops::to_geometry(&b.region.path).nearest(q).is_some_and(|(_, _, _, _, d)| d < 1e-4)
                });
                if adjacent {
                    found.insert(j);
                    pending.push(j);
                }
            }
        }
        Ok(found)
    }
    /// Plain fill/edge paths, the only representation consumed by display and exporters.
    pub fn expand(&self, sources: &[Layer], spec: &LivePaintSpec) -> Result<Vec<Layer>> {
        spec.validate()?;
        let mut layers = Vec::new();
        for (i, f) in self.faces.iter().enumerate() {
            if let Some(paint) = self.face_paint(i, sources, spec)? {
                let mut l = Layer::new("Face", LayerContent::Shape(ShapeLayer { path: f.region.path.clone(), fill: Some(paint), ..Default::default() }));
                l.id = LayerId(0);
                layers.push(l);
            }
        }
        for (i, e) in self.edges.iter().enumerate() {
            if let Some(stroke) = self.edge_paint(i, sources, spec)?.filter(|s| s.width > 0. && s.opacity > 0.) {
                let mut l = Layer::new("Edge", LayerContent::Shape(ShapeLayer { path: e.path.clone(), stroke: Some(stroke), ..Default::default() }));
                l.id = LayerId(0);
                layers.push(l);
            }
        }
        Ok(layers)
    }
}

#[cfg(test)]
#[path = "../../tests/live_paint/suite.rs"]
mod tests;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    #[test]
    fn dsfix_gesture_crosses_faces_even_with_horizontal_zero_height_bounds() {
        let layers: Vec<_> = [0., 30., 1000.]
            .into_iter()
            .enumerate()
            .map(|(i, x)| {
                let mut layer = Layer::new(
                    "face",
                    LayerContent::Shape(ShapeLayer {
                        path: photocraft_vector::shapes::rect(x, 0., 10., 10.),
                        fill: Some(Fill::Solid(photocraft_doc::Color::BLACK)),
                        ..Default::default()
                    }),
                );
                layer.id = LayerId(i as u64 + 1);
                layer
            })
            .collect();
        let graph = Graph::new(&layers, Gap::None).unwrap();
        let touched = graph.touched(&[Point::new(-5., 5.), Point::new(45., 5.)]).unwrap();
        assert_eq!(touched.len(), 2);
        let vertical = graph.touched(&[Point::new(5., -5.), Point::new(5., 15.)]).unwrap();
        assert_eq!(vertical.len(), 1);
    }
}
