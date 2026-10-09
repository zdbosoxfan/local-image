//! Adapter for VectorCraft geom/path.rs and pathops operations to pc-doc paths.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0
//! Upstream d522c1d7be4035bd4f4a84cd6ebfca44f5155092; see licenses/vectorcraft-NOTICE.

use crate::{BoolOp, PathOpsError, PathfinderOp, geom, kernel};
use kurbo::Shape as _;
use photocraft_doc::{FillRule, Knot, Path, PathOp, ShapeStroke, StrokeAlign, Subpath};
use photocraft_geom::Point;

type Result<T> = std::result::Result<T, PathOpsError>;
const NZ: geom::FillRule = geom::FillRule::NonZero;

pub fn validate(path: &Path) -> Result<()> {
    if path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        Err(PathOpsError::NonFinite)
    } else {
        Ok(())
    }
}

fn rule(r: FillRule) -> geom::FillRule {
    match r {
        FillRule::NonZero => NZ,
        FillRule::EvenOdd => geom::FillRule::EvenOdd,
    }
}

/// Lossless coordinate/handle adapter. Compound operations are resolved by `finish_compound`.
pub fn to_geometry(path: &Path) -> geom::PathData {
    geom::PathData::new(
        path.subpaths
            .iter()
            .map(|s| {
                geom::SubPath::new(
                    s.knots
                        .iter()
                        .map(|k| geom::Anchor {
                            p: (k.anchor.x, k.anchor.y).into(),
                            h_in: (k.in_ctrl.x, k.in_ctrl.y).into(),
                            h_out: (k.out_ctrl.x, k.out_ctrl.y).into(),
                            kind: if k.smooth { geom::AnchorKind::Smooth } else { geom::AnchorKind::Corner },
                        })
                        .collect(),
                    s.closed,
                )
            })
            .collect(),
    )
}

/// A normalized winding region: Join keeps holes in their owning PSD component.
/// Independent Combine subpaths would fill the holes in Local Image's PSD rasterizer.
pub fn from_geometry(path: &geom::PathData) -> Path {
    let pt = |p: kurbo::Point| Point::new(p.x, p.y);
    Path::new(
        path.subpaths
            .iter()
            .enumerate()
            .map(|(i, s)| Subpath {
                closed: s.closed,
                op: if i == 0 { PathOp::Combine } else { PathOp::Join },
                knots: s
                    .anchors
                    .iter()
                    .map(|a| Knot { anchor: pt(a.p), in_ctrl: pt(a.h_in), out_ctrl: pt(a.h_out), smooth: a.kind == geom::AnchorKind::Smooth })
                    .collect(),
            })
            .collect(),
    )
}

/// Bake PSD's ordered component operations. Inversion is bounded by `clip` when supplied.
pub fn finish_compound(path: &Path, clip: Option<kurbo::Rect>) -> Result<Path> {
    validate(path)?;
    let raw = to_geometry(path);
    let mut out = geom::PathData::default();
    for c in path.components() {
        let part = geom::PathData::new(raw.subpaths[c.clone()].to_vec());
        let part = kernel::try_normalize(&part, rule(path.fill_rule))?;
        let op = match path.effective_op(c.start) {
            PathOp::Combine | PathOp::Join => BoolOp::Union,
            PathOp::Subtract => BoolOp::Difference,
            PathOp::Intersect => BoolOp::Intersect,
            PathOp::Exclude => BoolOp::Xor,
        };
        out = kernel::try_boolean(&out, NZ, &part, NZ, op, crate::DEFAULT_PRECISION)?;
    }
    if path.inverted {
        let rect = clip.ok_or(PathOpsError::NeedsClip)?;
        if ![rect.x0, rect.y0, rect.x1, rect.y1].iter().all(|v| v.is_finite()) {
            return Err(PathOpsError::NonFinite);
        }
        let frame = geom::PathData::from_bezpath(&rect.to_path(1e-6));
        out = kernel::try_boolean(&frame, NZ, &out, NZ, BoolOp::Difference, crate::DEFAULT_PRECISION)?;
    }
    Ok(from_geometry(&out))
}

pub fn boolean(a: &Path, b: &Path, op: BoolOp) -> Result<Path> {
    let a = to_geometry(&finish_compound(a, None)?);
    let b = to_geometry(&finish_compound(b, None)?);
    Ok(from_geometry(&kernel::try_boolean(&a, NZ, &b, NZ, op, crate::DEFAULT_PRECISION)?))
}

pub fn area(path: &Path) -> Result<f64> {
    let p = finish_compound(path, None)?;
    Ok(to_geometry(&p).subpaths.iter().map(geom::SubPath::signed_area).sum::<f64>().abs())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub path: Path,
    pub key: u64,
}
impl Shape {
    pub fn new(path: Path, key: u64) -> Self {
        Self { path, key }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub path: Path,
    pub sources: Vec<usize>,
}
impl Region {
    pub fn contains(&self, point: Point) -> bool {
        contains(&self.path, point).unwrap_or(false)
    }
    pub fn top(&self) -> Option<usize> {
        self.sources.last().copied()
    }
}

fn shapes(input: &[Shape], filled: bool) -> Result<Vec<kernel::Shape>> {
    input
        .iter()
        .map(|s| {
            validate(&s.path)?;
            let p = if filled {
                finish_compound(&s.path, None)?
            } else {
                // Shape builder uses open edges without implicitly closing them.
                let closed = Path { subpaths: s.path.subpaths.iter().filter(|p| p.closed).cloned().collect(), ..s.path.clone() };
                let mut p = finish_compound(&closed, None)?;
                p.subpaths.extend(s.path.subpaths.iter().filter(|p| !p.closed).cloned());
                p
            };
            Ok(kernel::Shape::new(to_geometry(&p), NZ, s.key))
        })
        .collect()
}
pub fn pathfinder(op: PathfinderOp, input: &[Shape]) -> Result<Vec<Shape>> {
    Ok(kernel::pathfinder(op, &shapes(input, true)?).into_iter().map(|s| Shape::new(from_geometry(&s.path), s.key)).collect())
}
pub fn regions(input: &[Shape]) -> Result<Vec<Region>> {
    Ok(kernel::regions(&shapes(input, true)?).into_iter().map(|r| Region { path: from_geometry(&r.path), sources: r.sources }).collect())
}
#[derive(Clone, Debug, PartialEq)]
pub struct BuilderArrangement {
    pub regions: Vec<Region>,
    pub lines: Vec<Shape>,
    pub edges: Vec<Shape>,
}
pub fn shape_builder(input: &[Shape], edges: bool) -> Result<BuilderArrangement> {
    let a = kernel::shape_builder(&shapes(input, false)?, edges);
    Ok(BuilderArrangement {
        regions: a.regions.into_iter().map(|r| Region { path: from_geometry(&r.path), sources: r.sources }).collect(),
        lines: a.lines.into_iter().map(|s| Shape::new(from_geometry(&s.path), s.key)).collect(),
        edges: a.edges.into_iter().map(|s| Shape::new(from_geometry(&s.path), s.key)).collect(),
    })
}
pub fn merge_regions(input: &[Shape], faces: &[&Region]) -> Result<Path> {
    let shapes = shapes(input, false)?;
    let regions: Vec<kernel::Region> = faces.iter().map(|r| kernel::Region { path: to_geometry(&r.path), sources: r.sources.clone() }).collect();
    Ok(from_geometry(&kernel::FaceMerger::new(&shapes).merge(&regions.iter().collect::<Vec<_>>())))
}

pub fn offset_path(path: &Path, delta: f64, join: crate::Join, miter: f64) -> Result<Path> {
    validate(path)?;
    if !delta.is_finite() || !miter.is_finite() || miter < 1.0 {
        return Err(PathOpsError::InvalidOption);
    }
    let closed = Path { subpaths: path.subpaths.iter().filter(|s| s.closed).cloned().collect(), ..path.clone() };
    let mut p = finish_compound(&closed, None)?;
    p.subpaths.extend(path.subpaths.iter().filter(|s| !s.closed).cloned());
    Ok(from_geometry(&kernel::offset_path(&to_geometry(&p), delta, join, miter)))
}

/// Native outline geometry uses pc-vector's exact stroke polygons (including PSD dash units).
/// This keeps existing stroke coverage unchanged when a stroke becomes an editable filled path.
pub fn outline_stroke(path: &Path, stroke: &ShapeStroke, tolerance: f64) -> Result<Path> {
    validate(path)?;
    if !stroke.width.is_finite()
        || stroke.width <= 0.0
        || !stroke.miter_limit.is_finite()
        || stroke.miter_limit < 1.0
        || !tolerance.is_finite()
        || tolerance <= 0.0
        || !stroke.dash_offset.is_finite()
        || stroke.dashes.iter().any(|d| !d.is_finite() || *d < 0.0)
    {
        return Err(PathOpsError::InvalidOption);
    }
    let align = if path.subpaths.iter().all(|s| s.closed) { stroke.align } else { StrokeAlign::Center };
    let w = f64::from(stroke.width);
    let style = photocraft_vector::StrokeStyle {
        width: if align == StrokeAlign::Center { w } else { 2.0 * w },
        cap: stroke.cap,
        join: stroke.join,
        miter_limit: f64::from(stroke.miter_limit),
        dashes: stroke.dashes.iter().map(|d| f64::from(*d) * w).collect(),
        dash_offset: f64::from(stroke.dash_offset) * w,
    };
    let polygons = photocraft_vector::stroke_polygons(
        &photocraft_vector::flatten_path(path, (tolerance * 4.0).min(style.width.max(0.01) * 0.1).max(1e-3)),
        &style,
        tolerance,
    );
    let raw = Path::new(polygons.iter().enumerate().map(|(i, p)| Subpath::polygon(p).with_op(if i == 0 { PathOp::Combine } else { PathOp::Join })).collect());
    // Native tessellation contains small contours at dash/cap intersections. The
    // general Pathfinder precision drops those as slivers, changing stroke coverage.
    // Keep cleanup well below the caller's rasterization tolerance for expansion.
    let precision = (tolerance * 1e-3).clamp(1e-9, 1e-5);
    let out = kernel::try_boolean(&to_geometry(&raw), NZ, &geom::PathData::default(), NZ, BoolOp::Union, precision)?;
    if align == StrokeAlign::Center {
        return Ok(from_geometry(&out));
    }
    // The stroke band is finite: clipping by an inverted fill swaps intersection
    // and difference, without requiring an arbitrary finite frame for that fill.
    let mut mask = path.clone();
    mask.inverted = false;
    let fill = to_geometry(&finish_compound(&mask, None)?);
    let op = if (align == StrokeAlign::Inside) != path.inverted { BoolOp::Intersect } else { BoolOp::Difference };
    Ok(from_geometry(&kernel::try_boolean(&out, NZ, &fill, NZ, op, precision)?))
}

fn edit(path: &Path, f: impl FnOnce(&geom::PathData) -> geom::PathData) -> Result<Path> {
    validate(path)?;
    let mut out = from_geometry(&f(&to_geometry(path)));
    out.fill_rule = path.fill_rule;
    out.inverted = path.inverted;
    for (s, old) in out.subpaths.iter_mut().zip(&path.subpaths) {
        s.op = old.op;
    }
    Ok(out)
}
pub fn simplify(path: &Path, tolerance: f64) -> Result<Path> {
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(PathOpsError::InvalidOption);
    }
    edit(path, |p| kernel::simplify(p, tolerance))
}
pub fn smooth(path: &Path, amount: f64) -> Result<Path> {
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err(PathOpsError::InvalidOption);
    }
    edit(path, |p| kernel::smooth(p, amount))
}
pub fn reverse(path: &Path) -> Result<Path> {
    edit(path, |p| {
        let mut p = p.clone();
        p.reverse();
        p
    })
}
pub fn join(paths: &[Path], tolerance: f64) -> Result<Path> {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(PathOpsError::InvalidOption);
    }
    let mut out = Path::default();
    let mut open = Vec::new();
    for p in paths {
        validate(p)?;
        if p.inverted {
            return Err(PathOpsError::NeedsClip);
        }
        let closed = Path { subpaths: p.subpaths.iter().filter(|s| s.closed).cloned().collect(), ..p.clone() };
        // Each original closed input is its own PSD component, regardless of orientation.
        out.subpaths.extend(finish_compound(&closed, None)?.subpaths);
        open.push(to_geometry(&Path::new(p.subpaths.iter().filter(|s| !s.closed).cloned().collect())));
    }
    let joined = kernel::join(&open, tolerance);
    out.subpaths.extend(from_geometry(&joined).subpaths.into_iter().map(|s| s.with_op(PathOp::Combine)));
    Ok(out)
}
/// Split a segment at t and cut at that point, preserving cubic handles exactly.
pub fn split_at(path: &Path, subpath: usize, segment: usize, t: f64) -> Result<Vec<Path>> {
    validate(path)?;
    if !t.is_finite() || !(0.0..=1.0).contains(&t) {
        return Err(PathOpsError::InvalidOption);
    }
    let mut raw = to_geometry(path);
    let s = raw.subpaths.get_mut(subpath).ok_or(PathOpsError::InvalidOption)?;
    if segment >= s.segment_count() {
        return Err(PathOpsError::InvalidOption);
    }
    let index = if t == 0.0 {
        segment
    } else if t == 1.0 {
        (segment + 1) % s.anchors.len()
    } else {
        s.insert_anchor(segment, t)
    };
    let pieces = s.cut_at(&std::collections::BTreeSet::from([index]));
    let mut out: Vec<Path> = pieces.into_iter().map(|s| from_geometry(&geom::PathData::single(s))).collect();
    for (i, s) in path.subpaths.iter().enumerate() {
        if i != subpath {
            out.push(Path { subpaths: vec![s.clone()], ..path.clone() });
        }
    }
    Ok(out)
}
pub fn contains(path: &Path, point: Point) -> Result<bool> {
    validate(path)?;
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(PathOpsError::NonFinite);
    }
    // Preserve unbounded inversion for hit tests; geometry output requires clipping.
    let mut p = path.clone();
    p.inverted = false;
    let p = finish_compound(&p, None)?;
    let inside = to_geometry(&p).to_bezpath().winding((point.x, point.y).into()) != 0;
    Ok(inside != path.inverted)
}
pub fn nearest(path: &Path, point: Point) -> Result<Option<(usize, usize, f64, Point, f64)>> {
    validate(path)?;
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(PathOpsError::NonFinite);
    }
    Ok(to_geometry(path).nearest((point.x, point.y).into()).map(|(s, i, t, p, d)| (s, i, t, Point::new(p.x, p.y), d)))
}

/// Fit sampled points to cubic curves with a maximum fitting error.
pub fn fit_path(points: &[Point], tolerance: f64) -> Result<Path> {
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(PathOpsError::InvalidOption);
    }
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(PathOpsError::NonFinite);
    }
    if points.len() < 2 {
        return Ok(Path::default());
    }
    let pts: Vec<kurbo::Point> = points.iter().map(|p| (p.x, p.y).into()).collect();
    let t0 = kernel::fit::unit(pts[1] - pts[0]).unwrap_or(kurbo::Vec2::new(1.0, 0.0));
    let t1 = kernel::fit::unit(pts[pts.len() - 1] - pts[pts.len() - 2]).unwrap_or(t0);
    let mut bp = kurbo::BezPath::new();
    bp.move_to(pts[0]);
    for c in kernel::fit::fit_cubics(&pts, t0, t1, tolerance) {
        bp.curve_to(c.p1, c.p2, c.p3);
    }
    Ok(from_geometry(&geom::PathData::from_bezpath(&bp)))
}
