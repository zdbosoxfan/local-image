//! Authoritative bottom-to-top shape paints, using the existing exact-area rasterizer.
//! Stack semantics informed by VectorCraft crates/doc/src/appearance.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! None retains legacy fill/stroke; Some(empty) paints nothing. No silent paint fallback.

use crate::{DEFAULT_TOLERANCE, Rasterizer, fill_rasterizer};
use photocraft_color::{BlendMode, PixelFormat, blend::composite};
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
use photocraft_doc::{Fill, Path, Pattern, ShapeLayer, StrokeAlign};
use photocraft_geom::{Affine, Rect, TILE_SIZE};
use photocraft_raster::Surface;

mod geometry;
mod paint;
pub use paint::{PaintLimitations, check_svg_paint_limits};

pub const MAX_ITEMS: usize = 128;
pub const MAX_STOPS: usize = 4096;
pub const MAX_COORDINATE: f64 = 1_000_000.0;

/// Width/brush providers supply filled document-space outlines. No dependency back to pathops.
/// A projected outline includes stroke alignment, dashes and arrows, applied once by its provider.
pub trait StrokeProjection {
    fn outline(&self, path: &Path, stroke: &StrokeItem) -> Result<Path, String>;
}
pub struct NoProjection;
impl StrokeProjection for NoProjection {
    fn outline(&self, _: &Path, _: &StrokeItem) -> Result<Path, String> {
        Err("width profile / brush paint requires the package 11 / 17 outline projection".into())
    }
}

pub fn legacy_appearance(shape: &ShapeLayer) -> Appearance {
    Appearance {
        items: shape
            .fill
            .iter()
            .cloned()
            .map(|p| AppearanceItem::Fill(FillItem::new(p)))
            .chain(shape.stroke.iter().cloned().map(|base| AppearanceItem::Stroke(StrokeItem { base, ..Default::default() })))
            .collect(),
    }
}

fn number(v: f64, lo: f64, hi: f64) -> bool {
    v.is_finite() && (lo..=hi).contains(&v)
}
fn validate_paint(p: &Fill) -> Result<(), String> {
    let color = |c: &photocraft_color::Color| c.c.iter().all(|v| number(f64::from(*v), -65504., 65504.)) && number(f64::from(c.alpha), 0., 1.);
    let valid = match p {
        Fill::Solid(c) => color(c),
        Fill::Gradient { stops, angle, scale, opacity_stops, midpoints, offset, .. } => {
            stops.len() <= MAX_STOPS
                && opacity_stops.len() <= MAX_STOPS
                && midpoints.len() <= MAX_STOPS
                && stops.iter().all(|(t, c)| number(f64::from(*t), 0., 1.) && color(c))
                && opacity_stops.iter().all(|(t, a)| number(f64::from(*t), 0., 1.) && number(f64::from(*a), 0., 1.))
                && midpoints.iter().all(|v| number(f64::from(*v), 0., 1.))
                && number(f64::from(*angle), -360000., 360000.)
                && number(f64::from(*scale), 0.001, 10000.)
                && number(f64::from(offset.0), -10000., 10000.)
                && number(f64::from(offset.1), -10000., 10000.)
        }
        Fill::Pattern { name, id, scale, angle, phase, .. } => {
            name.len() <= 4096
                && id.len() <= 4096
                && number(f64::from(*scale), 0.001, 10000.)
                && number(f64::from(*angle), -360000., 360000.)
                && number(f64::from(phase.0), -MAX_COORDINATE, MAX_COORDINATE)
                && number(f64::from(phase.1), -MAX_COORDINATE, MAX_COORDINATE)
        }
    };
    if valid { Ok(()) } else { Err("appearance paint numbers must be finite and bounded".into()) }
}

pub fn validate(appearance: &Appearance) -> Result<(), String> {
    if appearance.items.len() > MAX_ITEMS {
        return Err("appearance exceeds the item budget".into());
    }
    for item in &appearance.items {
        match item {
            AppearanceItem::Fill(f) => {
                validate_paint(&f.paint)?;
                if !number(f64::from(f.opacity), 0., 1.) || f.blend == BlendMode::PassThrough {
                    return Err("fill opacity / blend is invalid".into());
                }
            }
            AppearanceItem::Mesh(m) => {
                crate::mesh_paint::Mesh::from(&m.paint).validate()?;
                if !number(f64::from(m.opacity), 0., 1.) || m.blend == BlendMode::PassThrough {
                    return Err("mesh opacity / blend is invalid".into());
                }
            }
            AppearanceItem::Stroke(s) => {
                validate_paint(&s.base.paint)?;
                if !number(f64::from(s.base.width), 0., 5000.)
                    || !number(f64::from(s.base.opacity), 0., 1.)
                    || !number(f64::from(s.base.miter_limit), 1., 1000.)
                    || s.base.dashes.len() > 1024
                    || s.base.dashes.iter().any(|v| !number(f64::from(*v), 0., 10000.))
                    || !number(f64::from(s.base.dash_offset), -MAX_COORDINATE, MAX_COORDINATE)
                    || s.arrow_scale.iter().any(|v| !number(*v, 0.001, 1000.))
                    || s.blend == BlendMode::PassThrough
                {
                    return Err("stroke numbers / blend must be finite and bounded".into());
                }
                if let Some(p) = &s.profile
                    && (p.points.len() > MAX_STOPS
                        || p.points.windows(2).any(|w| w[0].t >= w[1].t)
                        || p.points.iter().any(|p| !number(p.t, 0., 1.) || !number(p.left, 0., 5000.) || !number(p.right, 0., 5000.)))
                {
                    return Err("width profile must have ordered finite bounded points".into());
                }
                if s.brush.as_ref().is_some_and(|b| b.id.is_empty() || b.id.len() > 4096) {
                    return Err("invalid brush reference".into());
                }
            }
        }
    }
    Ok(())
}

/// PSD mirror: the topmost visible representable paint. Merged cache remains authoritative.
/// Pattern item opacity cannot be stored in legacy Fill, so a translucent pattern is skipped.
pub fn mirror_legacy(shape: &mut ShapeLayer, appearance: &Appearance) {
    shape.fill = None;
    shape.stroke = None;
    for item in &appearance.items {
        match item {
            AppearanceItem::Fill(f) if f.visible => {
                let mut p = f.paint.clone();
                match &mut p {
                    Fill::Solid(c) => c.alpha *= f.opacity,
                    Fill::Gradient { stops, .. } => stops.iter_mut().for_each(|(_, c)| c.alpha *= f.opacity),
                    Fill::Pattern { .. } if f.opacity != 1. => continue,
                    Fill::Pattern { .. } => {}
                }
                shape.fill = Some(p);
            }
            AppearanceItem::Stroke(s) if s.visible => shape.stroke = Some(s.base.clone()),
            _ => {}
        }
    }
}

/// Apply scale-with-object exactly once when geometry is baked. Relative dash lengths stay
/// relative to width. For anisotropic transforms use the area scale, as VectorCraft does.
pub fn scale_with_object(a: &mut Appearance, transform: Affine) -> Result<(), String> {
    let k = transform.determinant().abs().sqrt();
    if transform.m.iter().any(|v| !v.is_finite()) || !number(k, 0.001, 1000.) {
        return Err("stroke transform scale must be finite and bounded".into());
    }
    let mut next = a.clone();
    for i in &mut next.items {
        if let AppearanceItem::Mesh(mesh) = i {
            let projected = crate::mesh_paint::Mesh::from(&mesh.paint).transform(transform)?;
            mesh.paint = (&projected).into();
        }
        if let AppearanceItem::Stroke(s) = i
            && s.scale_with_object
        {
            s.base.width *= k as f32;
            if let Some(p) = &mut s.profile {
                for v in &mut p.points {
                    v.left *= k;
                    v.right *= k;
                }
            }
        }
    }
    validate(&next)?;
    *a = next;
    Ok(())
}

struct Item<'a> {
    coverage: Rasterizer,
    align: Option<(Rasterizer, bool)>,
    paint: paint::Sampler<'a>,
    opacity: f32,
    blend: BlendMode,
    bounds: Rect,
}

pub struct CompiledAppearance<'a> {
    items: Vec<Item<'a>>,
    bounds: Rect,
}
impl<'a> CompiledAppearance<'a> {
    pub fn new(shape: &ShapeLayer, appearance: Option<&Appearance>, patterns: &'a [Pattern], canvas: Rect) -> Result<Self, String> {
        Self::with_projection(shape, appearance, patterns, canvas, &NoProjection)
    }
    pub fn with_projection(
        shape: &ShapeLayer,
        appearance: Option<&Appearance>,
        patterns: &'a [Pattern],
        canvas: Rect,
        projection: &dyn StrokeProjection,
    ) -> Result<Self, String> {
        let legacy;
        let a = match appearance {
            Some(a) => a,
            None => {
                legacy = legacy_appearance(shape);
                &legacy
            }
        };
        validate(a)?;
        let source_points = geometry::validate_path(&shape.path)?;
        if source_points.saturating_mul(a.items.len()) > 1_048_576 {
            return Err("appearance stack exceeds the geometry budget".into());
        }
        let frame = shape.path.control_bounds().unwrap_or((0., 0., 0., 0.));
        let area = fill_rasterizer(&shape.path, DEFAULT_TOLERANCE);
        let mut items = Vec::new();
        let mut bounds = Rect::EMPTY;
        let mut geometry_points = 0;
        for i in &a.items {
            let (coverage, align, paint, opacity, blend) = match i {
                AppearanceItem::Fill(f) if f.visible && f.opacity > 0. => {
                    geometry_points += source_points;
                    (area.clone(), None, paint::Sampler::new(&f.paint, frame, canvas, patterns, None)?, f.opacity, f.blend)
                }
                AppearanceItem::Mesh(m) if m.visible && m.opacity > 0. => {
                    geometry_points += source_points;
                    let mesh = crate::mesh_paint::CompiledMesh::new(&crate::mesh_paint::Mesh::from(&m.paint), &shape.path, canvas)?;
                    (area.clone(), None, paint::Sampler::Mesh(Box::new(mesh)), m.opacity, m.blend)
                }
                AppearanceItem::Stroke(s)
                    if s.visible
                        && s.base.opacity > 0.
                        && (s.base.width > 0. || s.profile.as_ref().is_some_and(|p| p.points.iter().any(|p| p.left > 0. || p.right > 0.))) =>
                {
                    let projected = s.profile.is_some() || s.brush.is_some();
                    let (coverage, points) = if projected {
                        let p = projection.outline(&shape.path, s)?;
                        let points = geometry::validate_path(&p)?;
                        (fill_rasterizer(&p, DEFAULT_TOLERANCE), points)
                    } else {
                        geometry::stroke(&shape.path, s)?
                    };
                    geometry_points += points;
                    let align = (!projected && shape.path.subpaths.iter().all(|p| p.closed) && s.base.align != StrokeAlign::Center)
                        .then(|| (area.clone(), s.base.align == StrokeAlign::Inside));
                    (coverage, align, paint::Sampler::new(&s.base.paint, frame, canvas, patterns, Some((&shape.path, s)))?, s.base.opacity, s.blend)
                }
                _ => continue,
            };
            if geometry_points > 1_048_576 {
                return Err("appearance compiled geometry exceeds the point budget".into());
            }
            let b = if shape.path.inverted && matches!(i, AppearanceItem::Fill(_) | AppearanceItem::Mesh(_)) {
                canvas
            } else {
                coverage.pixel_bounds().unwrap_or(Rect::EMPTY).intersect(&canvas)
            };
            if b.is_empty() {
                continue;
            }
            bounds = if bounds.is_empty() { b } else { bounds.union(&b) };
            items.push(Item { coverage, align, paint, opacity, blend, bounds: b });
        }
        Ok(Self { items, bounds })
    }
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    /// Conservative tile-storage bound, including padding at both rectangle edges.
    pub fn cache_bytes(&self, format: PixelFormat) -> u128 {
        let b = self.bounds;
        if b.is_empty() {
            return 0;
        }
        let nx = (b.x1 - 1).div_euclid(TILE_SIZE) - b.x0.div_euclid(TILE_SIZE) + 1;
        let ny = (b.y1 - 1).div_euclid(TILE_SIZE) - b.y0.div_euclid(TILE_SIZE) + 1;
        nx as u128 * ny as u128 * TILE_SIZE as u128 * TILE_SIZE as u128 * format.bytes_per_pixel() as u128
    }
    /// Bounded row-major straight RGBA. Surface rendering calls this on tile-sized regions.
    pub fn render_rgba(&self, rect: Rect) -> Result<Vec<[f32; 4]>, String> {
        let n = u64::from(rect.width()) * u64::from(rect.height());
        if n > 1_048_576 {
            return Err("appearance region exceeds the pixel buffer budget".into());
        }
        let mut out = vec![[0.; 4]; n as usize];
        if n == 0 {
            return Ok(out);
        }
        let w = rect.width() as usize;
        for item in &self.items {
            if item.bounds.intersect(&rect).is_empty() {
                continue;
            }
            let mut cov = item.coverage.render(rect);
            if let Some((area, inside)) = &item.align {
                for (c, f) in cov.iter_mut().zip(area.render(rect)) {
                    *c *= if *inside { f } else { 1. - f };
                }
            }
            for (i, (o, c)) in out.iter_mut().zip(cov).enumerate() {
                if c <= 0. {
                    continue;
                }
                let x = rect.x0 + (i % w) as i32;
                let y = rect.y0 + (i / w) as i32;
                let mut p = item.paint.at(f64::from(x) + 0.5, f64::from(y) + 0.5);
                p[3] *= c;
                let mut opacity = item.opacity;
                if item.blend == BlendMode::Dissolve {
                    let alpha = p[3] * opacity;
                    p[3] = if photocraft_color::dither_noise(x, y) < alpha { 1. } else { 0. };
                    opacity = 1.;
                }
                *o = composite(item.blend, *o, p, opacity);
            }
        }
        Ok(out)
    }
    pub fn render(&self, format: PixelFormat, clip: Rect) -> Result<Surface, String> {
        if self.cache_bytes(format) > 512 << 20 {
            return Err("appearance cache exceeds the 512 MiB tile-storage budget".into());
        }
        let mut out = Surface::new(format);
        let area = self.bounds.intersect(&clip);
        if area.is_empty() {
            return Ok(out);
        }
        let channels = format.channels();
        for y in (area.y0..area.y1).step_by(TILE_SIZE as usize) {
            for x in (area.x0..area.x1).step_by(TILE_SIZE as usize) {
                let r = Rect::new(x, y, x.saturating_add(TILE_SIZE).min(area.x1), y.saturating_add(TILE_SIZE).min(area.y1));
                let rgba = self.render_rgba(r)?;
                if rgba.iter().all(|p| p[3] == 0.) {
                    continue;
                }
                let mut values = vec![0.; rgba.len() * channels];
                for (p, v) in rgba.iter().zip(values.chunks_exact_mut(channels)) {
                    photocraft_raster::from_rgba_into(&format, *p, v);
                }
                out.write_region(r, &values);
            }
        }
        Ok(out)
    }
}

pub fn render_shape(shape: &ShapeLayer, appearance: Option<&Appearance>, patterns: &[Pattern], format: PixelFormat, canvas: Rect) -> Result<Surface, String> {
    CompiledAppearance::new(shape, appearance, patterns, canvas)?.render(format, canvas)
}

#[cfg(test)]
mod tests;
