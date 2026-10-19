//! Colour, blend and axial/radial shading mapping inspired by VectorCraft pdf/import.rs
//! and import_shading.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
use super::{ImportError, Result};
use hayro_interpret::{Paint, StrokeProps, pattern::Pattern, shading::ShadingType};
use kurbo::{Affine, Point};
use photocraft_doc::{BlendMode, Color, Fill, ShapeStroke};

pub(super) fn blend(b: hayro_interpret::BlendMode) -> BlendMode {
    use hayro_interpret::BlendMode as H;
    match b {
        H::Normal => BlendMode::Normal,
        H::Multiply => BlendMode::Multiply,
        H::Screen => BlendMode::Screen,
        H::Overlay => BlendMode::Overlay,
        H::Darken => BlendMode::Darken,
        H::Lighten => BlendMode::Lighten,
        H::ColorDodge => BlendMode::ColorDodge,
        H::ColorBurn => BlendMode::ColorBurn,
        H::HardLight => BlendMode::HardLight,
        H::SoftLight => BlendMode::SoftLight,
        H::Difference => BlendMode::Difference,
        H::Exclusion => BlendMode::Exclusion,
        H::Hue => BlendMode::Hue,
        H::Saturation => BlendMode::Saturation,
        H::Color => BlendMode::Color,
        H::Luminosity => BlendMode::Luminosity,
    }
}
pub(super) fn color(rgba: [f32; 4]) -> Color {
    let mut c = Color::rgb(rgba[0], rgba[1], rgba[2]);
    c.alpha = rgba[3];
    c
}
pub(super) fn solid(p: &Paint<'_>) -> Option<Fill> {
    if let Paint::Color(c) = p { Some(Fill::Solid(color(c.to_rgba().components()))) } else { None }
}
pub(super) fn stroke(p: &StrokeProps, fill: Fill, scale: f64) -> Result<ShapeStroke> {
    let width = if p.line_width == 0.0 { 1.0 / scale.max(1e-9) } else { f64::from(p.line_width) };
    if !width.is_finite()
        || width < 0.0
        || width * scale > 100_000.0
        || !p.miter_limit.is_finite()
        || !(1.0..=100_000.0).contains(&p.miter_limit)
        || !p.dash_offset.is_finite()
        || p.dash_array.iter().any(|n| !n.is_finite() || *n < 0.0)
    {
        return Err(ImportError::Invalid("invalid stroke".into()));
    }
    let dashes: Vec<f32> = p.dash_array.iter().map(|n| (f64::from(*n) / width) as f32).collect();
    let dash_offset = (f64::from(p.dash_offset) / width) as f32;
    if dashes.iter().chain([&dash_offset]).any(|n| !n.is_finite()) {
        return Err(ImportError::Invalid("normalized stroke dash exceeds bounds".into()));
    }
    Ok(ShapeStroke {
        width: (width * scale) as f32,
        paint: fill,
        cap: match p.line_cap {
            kurbo::Cap::Butt => photocraft_doc::LineCap::Butt,
            kurbo::Cap::Round => photocraft_doc::LineCap::Round,
            kurbo::Cap::Square => photocraft_doc::LineCap::Square,
        },
        join: match p.line_join {
            kurbo::Join::Miter => photocraft_doc::LineJoin::Miter,
            kurbo::Join::Round => photocraft_doc::LineJoin::Round,
            kurbo::Join::Bevel => photocraft_doc::LineJoin::Bevel,
        },
        miter_limit: p.miter_limit,
        dashes,
        dash_offset,
        ..Default::default()
    })
}
pub(super) fn isotropic(m: Affine) -> Option<f64> {
    let [a, b, c, d, _, _] = m.as_coeffs();
    let (s, t) = (a.hypot(b), c.hypot(d));
    if (s - t).abs() < 1e-7 * s.max(1.0) && (a * c + b * d).abs() < 1e-7 * s * s { Some(s) } else { None }
}
/// Paint evaluation is independent of pc-vector coverage. No second rasterizer is used.
/// Unsupported meshes fail explicitly instead of silently being painted black.
pub(super) fn sample(p: &Paint<'_>, point: Point) -> Result<[f32; 4]> {
    evaluate(p, point, &[])
}
pub(super) fn evaluate(p: &Paint<'_>, point: Point, triangles: &[hayro_interpret::shading::Triangle]) -> Result<[f32; 4]> {
    let Paint::Pattern(pattern) = p else { return Ok(if let Paint::Color(c) = p { c.to_rgba().components() } else { [0.0; 4] }) };
    let Pattern::Shading(s) = pattern.as_ref() else { return Err(ImportError::Unsupported("tiling patterns")) };
    if !s.matrix.determinant().is_finite() || s.matrix.determinant().abs() < 1e-15 {
        return Ok([0.0; 4]);
    }
    let p = s.matrix.inverse() * point;
    let (function, input) = match s.shading.shading_type.as_ref() {
        ShadingType::RadialAxial { coords, domain, function, extend, axial } => {
            let v = coords.map(f64::from);
            let mut t = if *axial {
                let (dx, dy) = (v[2] - v[0], v[3] - v[1]);
                ((p.x - v[0]) * dx + (p.y - v[1]) * dy) / (dx * dx + dy * dy)
            } else {
                let (dx, dy, dr) = (v[3] - v[0], v[4] - v[1], v[5] - v[2]);
                let (px, py) = (p.x - v[0], p.y - v[1]);
                let (a, b, c) = (dx * dx + dy * dy - dr * dr, -2.0 * (px * dx + py * dy + v[2] * dr), px * px + py * py - v[2] * v[2]);
                let roots = if a.abs() < 1e-15 {
                    vec![-c / b]
                } else {
                    let disc = b * b - 4.0 * a * c;
                    if disc < 0.0 {
                        return Ok([0.0; 4]);
                    }
                    vec![(-b - disc.sqrt()) / (2.0 * a), (-b + disc.sqrt()) / (2.0 * a)]
                };
                let Some(t) = roots
                    .into_iter()
                    .filter(|t| t.is_finite() && v[2] + t * dr >= 0.0 && (*t >= 0.0 || extend[0]) && (*t <= 1.0 || extend[1]))
                    .max_by(f64::total_cmp)
                else {
                    return Ok([0.0; 4]);
                };
                t
            };
            if !t.is_finite() || (t < 0.0 && !extend[0]) || (t > 1.0 && !extend[1]) {
                return Ok([0.0; 4]);
            }
            t = t.clamp(0.0, 1.0);
            (function, vec![domain[0] + (domain[1] - domain[0]) * t as f32])
        }
        ShadingType::FunctionBased { domain, matrix, function } => {
            let p = matrix.inverse() * p;
            if p.x < f64::from(domain[0]) || p.x > f64::from(domain[1]) || p.y < f64::from(domain[2]) || p.y > f64::from(domain[3]) {
                return Ok([0.0; 4]);
            }
            (function, vec![p.x as f32, p.y as f32])
        }
        ShadingType::Dummy => return Ok([0.0; 4]),
        ShadingType::TriangleMesh { function, .. } | ShadingType::CoonsPatchMesh { function, .. } | ShadingType::TensorProductPatchMesh { function, .. } => {
            let values = triangles.iter().rev().find(|t| t.contains_point(p)).map(|t| t.interpolate(p));
            let Some(values) = values else {
                return Ok([0.0; 4]);
            };
            let values = if let Some(f) = function { f.eval(&values).ok_or_else(|| ImportError::Invalid("mesh colour function".into()))? } else { values };
            let mut c = s.shading.color_space.to_rgba(&values, s.opacity);
            if let Some(tf) = &s.transfer_function {
                c = tf.apply(&c);
            }
            return finite_rgba(c.components());
        }
    };
    let input = input.into_iter().collect();
    let values = function.eval(&input).ok_or_else(|| ImportError::Invalid("shading function".into()))?;
    let mut color = s.shading.color_space.to_rgba(&values, s.opacity);
    if let Some(tf) = &s.transfer_function {
        color = tf.apply(&color);
    }
    finite_rgba(color.components())
}
fn finite_rgba(rgba: [f32; 4]) -> Result<[f32; 4]> {
    if rgba.iter().any(|n| !n.is_finite()) {
        return Err(ImportError::Invalid("nonfinite shading output".into()));
    }
    Ok(rgba)
}
pub(super) fn mirror(p: &Paint<'_>, path: &photocraft_doc::Path) -> Result<Option<Fill>> {
    let Paint::Pattern(p) = p else { return Ok(solid(p)) };
    let Pattern::Shading(s) = p.as_ref() else { return Ok(None) };
    let ShadingType::RadialAxial { coords, axial, .. } = s.shading.shading_type.as_ref() else { return Ok(None) };
    let c = coords.map(f64::from);
    let (a, b) = if *axial {
        (s.matrix * Point::new(c[0], c[1]), s.matrix * Point::new(c[2], c[3]))
    } else {
        (s.matrix * Point::new(c[3], c[4]), s.matrix * Point::new(c[3] + c[5], c[4]))
    };
    let bounds = path.control_bounds().unwrap_or((0.0, 0.0, 1.0, 1.0));
    let (w, h) = ((bounds.2 - bounds.0).max(1.0), (bounds.3 - bounds.1).max(1.0));
    let angle = -(b.y - a.y).atan2(b.x - a.x);
    let length = if *axial {
        (w / angle.cos().abs().max(1e-6)).min(h / angle.sin().abs().max(1e-6))
    } else {
        ((angle.cos() * w).powi(2) + (angle.sin() * h).powi(2)).sqrt() / 2.0
    };
    let mut stops = Vec::new();
    for i in 0..=64 {
        let t = i as f32 / 64.0;
        let local = if *axial {
            Point::new(c[0] + (c[2] - c[0]) * f64::from(t), c[1] + (c[3] - c[1]) * f64::from(t))
        } else {
            Point::new(c[0] + (c[3] - c[0]) * f64::from(t) + c[2] + (c[5] - c[2]) * f64::from(t), c[1] + (c[4] - c[1]) * f64::from(t))
        };
        let paint = Paint::Pattern(Box::new(Pattern::Shading(s.clone())));
        stops.push((t, color(sample(&paint, s.matrix * local)?)));
    }
    let center = if *axial { Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0) } else { a };
    Ok(Some(Fill::Gradient {
        stops,
        angle: angle.to_degrees() as f32,
        scale: ((b - a).hypot() / length.max(1e-6)) as f32,
        style: if *axial { photocraft_doc::GradientStyle::Linear } else { photocraft_doc::GradientStyle::Radial },
        reverse: false,
        opacity_stops: vec![],
        midpoints: vec![],
        offset: (((center.x - (bounds.0 + bounds.2) / 2.0) / w) as f32, ((center.y - (bounds.1 + bounds.3) / 2.0) / h) as f32),
        dither: false,
        align: true,
    }))
}

pub(super) fn mesh(p: &Paint<'_>, limit: usize, bytes: u64) -> Result<Vec<hayro_interpret::shading::Triangle>> {
    let Paint::Pattern(p) = p else { return Ok(vec![]) };
    let Pattern::Shading(s) = p.as_ref() else { return Ok(vec![]) };
    let limit = limit.min((bytes / 4096).min(usize::MAX as u64) as usize);
    let mut triangles = Vec::new();
    match s.shading.shading_type.as_ref() {
        ShadingType::TriangleMesh { triangles: ts, .. } => {
            if ts.len() > limit {
                return Err(ImportError::Limit("mesh triangle"));
            }
            triangles = ts.clone();
        }
        ShadingType::CoonsPatchMesh { patches, .. } => {
            if patches.len() > limit / 722 {
                return Err(ImportError::Limit("mesh triangle"));
            }
            for p in patches {
                p.to_triangles(&mut triangles);
            }
        }
        ShadingType::TensorProductPatchMesh { patches, .. } => {
            if patches.len() > limit / 722 {
                return Err(ImportError::Limit("mesh triangle"));
            }
            for p in patches {
                p.to_triangles(&mut triangles);
            }
        }
        _ => {}
    }
    Ok(triangles)
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn review_additional_pdf_rejects_nonfinite_normalized_dash_and_mesh_color() {
        let props = StrokeProps { line_width: f32::MIN_POSITIVE / 4., dash_array: vec![1.].into(), ..Default::default() };
        assert!(stroke(&props, Fill::Solid(Color::BLACK), 1.).is_err());
        assert!(finite_rgba([0., 0., f32::NAN, 1.]).is_err());
        assert!(finite_rgba([0., 0., 0., 1.]).is_ok());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    #[test]
    fn dsfix_denormal_width_cannot_produce_infinite_dash_values() {
        let mut props = StrokeProps { line_width: 1e-40, ..Default::default() };
        props.dash_array.push(1.);
        assert!(stroke(&props, Fill::Solid(Color::BLACK), 1.).is_err());
        props.line_width = 2.;
        let normal = stroke(&props, Fill::Solid(Color::BLACK), 1.).unwrap();
        assert_eq!(normal.dashes, vec![0.5]);
        props.line_width = 0.;
        assert!(stroke(&props, Fill::Solid(Color::BLACK), 1.).unwrap().dashes.iter().all(|n| n.is_finite()));
    }
}
