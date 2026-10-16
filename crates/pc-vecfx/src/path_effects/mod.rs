//! Live path effects adapted from VectorCraft effects/{distort,stylize,util}.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Ordinary paths are transient. Copies remain distinct paint operations, preserving opacity.
use kurbo::{CubicBez, ParamCurve, Point, Rect};
pub use photocraft_doc::vector_design::effects::*;
use photocraft_doc::{
    Knot, Path, ShapeLayer,
    vector_design::appearance::{Appearance, AppearanceItem},
};
mod distort;
mod mapping;
pub mod render;
pub use photocraft_vector::live_corners as corners;

pub const MAX_NODES: usize = 65_536;
pub const MAX_EFFECTS: usize = 64;
pub const MAX_PAINT_PATHS: usize = 4096;
pub const MAX_COORDINATE: f64 = 1_000_000.;
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("path effect numbers must be finite and bounded")]
    Invalid,
    #[error("path effects exceed the geometry or work budget")]
    Budget,
    #[error("path effect references an absent appearance item")]
    Target,
    #[error("path effect geometry: {0}")]
    Geometry(String),
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub dpi: f64,
    pub tolerance: f64,
}
impl Default for Options {
    fn default() -> Self {
        Self { dpi: 72., tolerance: 0.01 }
    }
}
impl Options {
    fn validate(self) -> Result<()> {
        if number(self.dpi, 1., 10000.) && number(self.tolerance, 0.00001, 1.) { Ok(()) } else { Err(Error::Invalid) }
    }
}
fn number(v: f64, lo: f64, hi: f64) -> bool {
    v.is_finite() && (lo..=hi).contains(&v)
}
pub fn validate_effect(e: &EffectKind) -> Result<()> {
    let valid = match e {
        EffectKind::Roughen { size, detail, .. } => number(*size, 0., 10000.) && number(*detail, 0., 100.),
        EffectKind::ZigZag { size, ridges, .. } => number(*size, -10000., 10000.) && *ridges <= 100,
        EffectKind::PuckerBloat { amount } => number(*amount, -200., 200.),
        EffectKind::Twist { angle } => number(*angle, -3600., 3600.),
        EffectKind::Transform { scale, move_by, angle, copies, .. } => {
            scale.iter().all(|v| number(*v, 0.01, 10000.))
                && move_by.iter().all(|v| number(*v, -100000., 100000.))
                && number(*angle, -3600., 3600.)
                && *copies <= 1000
        }
        EffectKind::RoundCorners { radius } => number(*radius, 0., 100000.),
        EffectKind::OffsetPath { distance, miter, .. } => number(*distance, -100000., 100000.) && number(*miter, 1., 1000.),
        EffectKind::FreeDistort { corners } => corners.iter().flatten().all(|v| number(*v, -16., 16.)),
        EffectKind::ConvertToShape { extra, radius, .. } => extra.iter().all(|v| number(*v, -100000., 100000.)) && number(*radius, 0., 100000.),
    };
    if valid { Ok(()) } else { Err(Error::Invalid) }
}
pub fn validate_path(p: &Path) -> Result<()> {
    if p.subpaths.len() > MAX_NODES || node_count(p) > MAX_NODES {
        return Err(Error::Budget);
    }
    if p.subpaths
        .iter()
        .flat_map(|s| &s.knots)
        .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
        .any(|p| !number(p.x, -MAX_COORDINATE, MAX_COORDINATE) || !number(p.y, -MAX_COORDINATE, MAX_COORDINATE))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
pub fn validate_stack(effects: &[PathEffect], items: usize) -> Result<()> {
    if effects.len() > MAX_EFFECTS {
        return Err(Error::Budget);
    }
    for e in effects {
        validate_effect(&e.effect)?;
        if matches!(e.target, EffectTarget::Item { index } if index >= items) {
            return Err(Error::Target);
        }
    }
    Ok(())
}
fn node_count(p: &Path) -> usize {
    p.subpaths.iter().map(|s| s.knots.len()).sum()
}
fn bounds<'a>(paths: impl Iterator<Item = &'a Path>) -> Result<Option<Rect>> {
    let mut out: Option<Rect> = None;
    for p in paths {
        validate_path(p)?;
        if let Some((x0, y0, x1, y1)) = photocraft_pathops::bounds(p).map_err(|e| Error::Geometry(e.to_string()))? {
            let r = Rect::new(x0, y0, x1, y1);
            out = Some(out.map_or(r, |o| o.union(r)));
        }
    }
    Ok(out)
}
fn cubic(a: &Knot, b: &Knot) -> CubicBez {
    let p = |p: photocraft_geom::Point| Point::new(p.x, p.y);
    let (p0, p3) = (p(a.anchor), p(b.anchor));
    if a.out_ctrl == a.anchor && b.in_ctrl == b.anchor {
        CubicBez::new(p0, p0.lerp(p3, 1. / 3.), p0.lerp(p3, 2. / 3.), p3)
    } else {
        CubicBez::new(p0, p(a.out_ctrl), p(b.in_ctrl), p3)
    }
}
fn doc_point(p: Point) -> photocraft_geom::Point {
    photocraft_geom::Point::new(p.x, p.y)
}
fn affine_path(path: &Path, a: kurbo::Affine) -> Result<Path> {
    let mut out = path.clone();
    for p in out.subpaths.iter_mut().flat_map(|s| &mut s.knots).flat_map(|k| [&mut k.anchor, &mut k.in_ctrl, &mut k.out_ctrl]) {
        *p = doc_point(a * Point::new(p.x, p.y));
    }
    validate_path(&out)?;
    Ok(out)
}
fn single(path: &Path, e: &EffectKind, b: Rect, options: Options) -> Result<Path> {
    match e {
        EffectKind::Roughen { .. } | EffectKind::ZigZag { .. } | EffectKind::PuckerBloat { .. } | EffectKind::Twist { .. } | EffectKind::FreeDistort { .. } => {
            distort::apply(path, e, b, options)
        }
        EffectKind::RoundCorners { radius } => {
            if *radius == 0. {
                return Ok(path.clone());
            }
            use photocraft_doc::vector_design::drawing::{Corner, CornerKind, KnotCorner};
            let metadata = corners::handles(path)
                .map_err(|e| Error::Geometry(e.to_string()))?
                .into_iter()
                .map(|h| KnotCorner { subpath: h.key.subpath, knot: h.key.knot, corner: Corner { radius: *radius, kind: CornerKind::Round } })
                .collect::<Vec<_>>();
            Ok(corners::evaluate(path, &metadata).map_err(|e| Error::Geometry(e.to_string()))?.path)
        }
        EffectKind::OffsetPath { distance, join, miter } => {
            let join = match join {
                OffsetJoin::Miter => photocraft_pathops::Join::Miter,
                OffsetJoin::Round => photocraft_pathops::Join::Round,
                OffsetJoin::Bevel => photocraft_pathops::Join::Bevel,
            };
            photocraft_pathops::offset_path(path, *distance, join, *miter).map_err(|e| Error::Geometry(e.to_string()))
        }
        EffectKind::ConvertToShape { shape, extra, radius } => {
            let w = (b.width() + extra[0]).max(0.);
            let h = (b.height() + extra[1]).max(0.);
            let x = b.center().x - w / 2.;
            let y = b.center().y - h / 2.;
            Ok(match shape {
                ShapeKind::Rectangle => photocraft_vector::shapes::rect(x, y, w, h),
                ShapeKind::RoundedRectangle => photocraft_vector::shapes::rounded_rect(x, y, w, h, [*radius; 4]),
                ShapeKind::Ellipse => photocraft_vector::shapes::ellipse(x, y, w, h),
            })
        }
        EffectKind::Transform { .. } => affine_path(path, distort::transform(e, b)?),
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PaintPath {
    pub path: Path,
    pub item: AppearanceItem,
    pub source_item: usize,
}
/// Every layer effect precedes paint. Item effects change only that item's geometry.
/// Layer Transform copies retain complete bottom-to-top appearance order for each duplicate.
pub fn project(shape: &ShapeLayer, appearance: Option<&Appearance>, effects: &[PathEffect], options: Options) -> Result<Vec<PaintPath>> {
    options.validate()?;
    validate_path(&shape.path)?;
    let a = appearance.cloned().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(shape));
    photocraft_vector::appearance::validate(&a).map_err(Error::Geometry)?;
    validate_stack(effects, a.items.len())?;
    let mut scene: Vec<_> = a.items.into_iter().enumerate().map(|(source_item, item)| PaintPath { path: shape.path.clone(), item, source_item }).collect();
    budget(&scene)?;
    for effect in effects.iter().filter(|e| e.enabled) {
        let selected = |p: &PaintPath| effect.target == EffectTarget::Layer || effect.target == EffectTarget::Item { index: p.source_item };
        let Some(b) = bounds(scene.iter().filter(|p| selected(p)).map(|p| &p.path))? else { continue };
        if let EffectKind::Transform { copies, .. } = &effect.effect
            && *copies > 0
        {
            let count = scene.iter().filter(|p| selected(p)).count();
            if scene.len().saturating_add(count.saturating_mul(*copies as usize)) > MAX_PAINT_PATHS {
                return Err(Error::Budget);
            }
            let a = distort::transform(&effect.effect, b)?;
            if effect.target == EffectTarget::Layer {
                let mut cur = scene.clone();
                for _ in 0..*copies {
                    for p in &mut cur {
                        p.path = affine_path(&p.path, a)?;
                    }
                    scene.extend(cur.iter().cloned());
                    budget(&scene)?;
                }
            } else {
                let mut next = Vec::new();
                for p in scene {
                    let is_selected = selected(&p);
                    let mut cur = p.clone();
                    next.push(p);
                    if is_selected {
                        for _ in 0..*copies {
                            cur.path = affine_path(&cur.path, a)?;
                            next.push(cur.clone());
                        }
                    }
                }
                scene = next;
            }
        } else {
            for i in 0..scene.len() {
                if selected(&scene[i]) {
                    scene[i].path = single(&scene[i].path, &effect.effect, b, options)?;
                    budget(&scene)?;
                }
            }
        }
        budget(&scene)?;
    }
    Ok(scene)
}
fn budget(scene: &[PaintPath]) -> Result<()> {
    if scene.len() > MAX_PAINT_PATHS || scene.iter().map(|p| node_count(&p.path)).sum::<usize>() > MAX_NODES {
        return Err(Error::Budget);
    }
    for p in scene {
        validate_path(&p.path)?;
    }
    Ok(())
}
/// Paint-independent convenience for geometry consumers; duplicates stay separate paths.
pub fn evaluate(path: &Path, effects: &[PathEffect], options: Options) -> Result<Vec<Path>> {
    let shape = ShapeLayer { path: path.clone(), fill: Some(photocraft_doc::Fill::Solid(photocraft_doc::Color::BLACK)), ..Default::default() };
    Ok(project(&shape, None, effects, options)?.into_iter().map(|p| p.path).collect())
}
pub fn export_warning(effects: &[PathEffect]) -> Option<photocraft_doc::vector_design::exchange::VectorWarning> {
    effects.iter().any(|e| e.enabled).then(|| photocraft_doc::vector_design::exchange::VectorWarning {
        feature: "path_effects".into(),
        object: None,
        message: "Live path effects were expanded to ordinary paths; effect parameters are no longer editable.".into(),
    })
}
#[cfg(test)]
#[path = "../../tests/path_effects/geometry.rs"]
mod tests;
