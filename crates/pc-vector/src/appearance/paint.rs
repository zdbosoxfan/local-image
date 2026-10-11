//! Pattern placement adapted from the PhotoCraft fork crates/pc-compose/src/pattern.rs
//! at c18d18cf613422fe08d31001e1f803cf8ef7dd68 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the PhotoCraft contributors.
//! SVG limit checks follow https://www.w3.org/TR/SVG2/pservers.html; the native Fill model
//! stores rotation/uniform scale/phase and pad spread, not an arbitrary SVG paint transform.
use super::geometry::Metric;
use crate::Paint;
use photocraft_doc::vector_design::appearance::{StrokeGradientMode, StrokeItem};
use photocraft_doc::{Fill, GradientStyle, Path, Pattern};
use photocraft_geom::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaintLimitations {
    /// SVG [a,b,c,d,e,f] mapping. Native paints can represent a similarity transform only.
    pub transform: Option<[f64; 6]>,
    /// True for repeat or reflect spread. Native gradients currently use pad.
    pub non_pad_spread: bool,
}
/// File adapters use this capability check before native projection; they own raster fallback.
pub fn check_svg_paint_limits(limits: PaintLimitations) -> Vec<&'static str> {
    let mut warnings = Vec::new();
    if limits.transform.is_some_and(|m| {
        m.iter().any(|v| !v.is_finite())
            || (m[0] * m[2] + m[1] * m[3]).abs() > 1e-8
            || (m[0].hypot(m[1]) - m[2].hypot(m[3])).abs() > 1e-8
            || m[0] * m[3] - m[1] * m[2] <= 0.
    }) {
        warnings.push("SVG paint skew, reflection or nonuniform scale requires raster fallback");
    }
    if limits.non_pad_spread {
        warnings.push("SVG repeat / reflect gradient spread requires raster fallback");
    }
    warnings
}

pub(super) enum Sampler<'a> {
    Mesh(Box<crate::mesh_paint::CompiledMesh>),
    Object(Paint),
    Pattern { tile: &'a Pattern, origin: (f64, f64), cs: (f64, f64), inverse: f64 },
    Stroke { ramp: Paint, metric: Metric, mode: StrokeGradientMode, dither: bool },
}
impl<'a> Sampler<'a> {
    pub fn new(p: &Fill, bounds: (f64, f64, f64, f64), canvas: Rect, patterns: &'a [Pattern], stroke: Option<(&Path, &StrokeItem)>) -> Result<Self, String> {
        let cb = (f64::from(canvas.x0), f64::from(canvas.y0), f64::from(canvas.x1), f64::from(canvas.y1));
        if let Fill::Pattern { name, id, scale, angle, link, phase } = p {
            let tile = photocraft_doc::pattern::find(patterns, id, name).ok_or_else(|| format!("missing appearance pattern: {name} ({id})"))?;
            if tile.is_empty() || tile.width > i32::MAX as u32 || tile.height > i32::MAX as u32 {
                return Err("appearance pattern has invalid dimensions".into());
            }
            let base = if *link { (bounds.0.floor(), bounds.1.floor()) } else { (0., 0.) };
            let a = f64::from(*angle).to_radians();
            return Ok(Self::Pattern {
                tile,
                origin: (base.0 + f64::from(phase.0), base.1 + f64::from(phase.1)),
                cs: (a.cos(), a.sin()),
                inverse: 1. / f64::from(*scale),
            });
        }
        if let Some((path, s)) = stroke
            && s.gradient_mode != StrokeGradientMode::Object
            && let Fill::Gradient { dither, .. } = p
        {
            let mut ramp = p.clone();
            if let Fill::Gradient { angle, style, dither, align, .. } = &mut ramp {
                *angle = 0.;
                *style = GradientStyle::Linear;
                *dither = false;
                *align = true;
            }
            return Ok(Self::Stroke {
                ramp: Paint::from_fill(&ramp, (0., 0., 1024., 1024.), cb),
                metric: Metric::new(path, s)?,
                mode: s.gradient_mode,
                dither: *dither,
            });
        }
        Ok(Self::Object(Paint::from_fill(p, bounds, cb)))
    }
    pub fn at(&self, x: f64, y: f64) -> [f32; 4] {
        match self {
            Self::Object(p) => p.at(x, y),
            Self::Mesh(mesh) => mesh.at(x, y),
            Self::Stroke { ramp, metric, mode, dither } => {
                let (along, across) = metric.sample(x, y);
                let t = if *mode == StrokeGradientMode::Along { along } else { across };
                let mut c = ramp.at(t.clamp(0., 1.) * 1024. + 0.5, 512.);
                if *dither {
                    let n = (photocraft_color::dither_noise(x.floor() as i32, y.floor() as i32) - 0.5) / 255.;
                    for v in &mut c[..3] {
                        *v = (*v + n).clamp(0., 1.);
                    }
                }
                c
            }
            Self::Pattern { tile, origin, cs: (c, s), inverse } => {
                let (dx, dy) = (x - origin.0, y - origin.1);
                let (u, v) = ((dx * c - dy * s) * inverse - 0.5, (dx * s + dy * c) * inverse - 0.5);
                // Reduce in floating point before the integer cast, avoiding saturation at tiny scales.
                let (u, v) = (u.rem_euclid(f64::from(tile.width)), v.rem_euclid(f64::from(tile.height)));
                let (fx, fy) = ((u - u.floor()) as f32, (v - v.floor()) as f32);
                let (x0, y0) = (u.floor() as i32, v.floor() as i32);
                let mut p = [0.; 4];
                for (dy, wy) in [(0, 1. - fy), (1, fy)] {
                    for (dx, wx) in [(0, 1. - fx), (1, fx)] {
                        let q = tile.surface.rgba((x0 + dx).rem_euclid(tile.width as i32), (y0 + dy).rem_euclid(tile.height as i32));
                        let w = wx * wy;
                        for k in 0..3 {
                            p[k] += q[k] * q[3] * w;
                        }
                        p[3] += q[3] * w;
                    }
                }
                if p[3] > 0. {
                    for k in 0..3 {
                        p[k] /= p[3];
                    }
                }
                p
            }
        }
    }
}
