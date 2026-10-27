//! Native paint mirrors. Affinity gradient geometry outside Fill's model produces a warning.
use super::scene::State;
use crate::{
    Error,
    paint::{self, Paint},
};
use photocraft_doc::{Affine, Color, Fill, GradientStyle, ShapeStroke, effects::Gradient};

pub(super) fn color(c: paint::Color) -> Color {
    let (r, g, b, a) = match c {
        paint::Color::Rgb { r, g, b, a } => (r, g, b, a),
        paint::Color::Gray { v, a } => (v, v, v, a),
        paint::Color::Cmyk { c, m, y, k, a } => ((1. - c) * (1. - k), (1. - m) * (1. - k), (1. - y) * (1. - k), a),
        paint::Color::Lab { l, a, b, alpha } => {
            // Preserve this import format's historical white and matrix coefficients;
            // shared color science supplies the f64 Lab inverse and sRGB transfer.
            let xyz = li_color::cms_math::lab_to_xyz([l, a, b], [0.96422, 1.0, 0.82521]);
            let adapted =
                li_color::Mat3([[0.9555766, -0.0230393, 0.0631636], [-0.0282895, 1.0099416, 0.0210077], [0.0122982, -0.020483, 1.3299098]]).apply(xyz);
            let [r, g, b] = li_color::Mat3([[3.2404542, -1.5371385, -0.4985314], [-0.969266, 1.8760108, 0.041556], [0.0556434, -0.2040259, 1.0572252]])
                .apply(adapted)
                .map(li_color::transfer::linear_to_srgb_f64);
            (r, g, b, alpha)
        }
    };
    let mut out = Color::rgb(r.clamp(0., 1.) as f32, g.clamp(0., 1.) as f32, b.clamp(0., 1.) as f32);
    out.alpha = a.clamp(0., 1.) as f32;
    out
}
pub(super) fn fill(state: &mut State<'_, '_>, p: &Paint, m: Affine, bounds: (f64, f64, f64, f64), name: &str) -> Result<Option<Fill>, Error> {
    match p {
        Paint::None => Ok(None),
        Paint::Solid(c) => {
            if matches!(c, paint::Color::Cmyk { .. }) {
                state.warn("colour-profile", name, "CMYK colour was converted without the source profile.");
            }
            Ok(Some(Fill::Solid(color(*c))))
        }
        Paint::Gradient(g) => {
            let (Gradient { stops, opacity_stops, style, angle, scale, reverse, align, offset }, midpoints) = gradient(state, g, m, bounds, name)?;
            Ok(Some(Fill::Gradient { stops, opacity_stops, style, angle, scale, reverse, align, offset, midpoints, dither: false }))
        }
    }
}
pub(super) fn gradient(
    state: &mut State<'_, '_>,
    g: &paint::Gradient,
    m: Affine,
    bounds: (f64, f64, f64, f64),
    name: &str,
) -> Result<(Gradient, Vec<f32>), Error> {
    if g.stops.iter().any(|stop| matches!(stop.color, paint::Color::Cmyk { .. })) {
        state.warn("colour-profile", name, "CMYK gradient colours were converted without the source profile.");
    }
    let [a, b, c, d, e, f] = g.transform.0;
    let transform = m.mul(&Affine { m: [a, b, c, d, e, f] });
    state.affine(transform)?;
    let start = transform.apply(photocraft_geom::Point::new(0., 0.));
    let end = transform.apply(photocraft_geom::Point::new(1., 0.));
    let angle = -(end.y - start.y).atan2(end.x - start.x);
    let (w, h) = ((bounds.2 - bounds.0).max(1.), (bounds.3 - bounds.1).max(1.));
    let style = if g.kind == paint::GradientKind::Linear { GradientStyle::Linear } else { GradientStyle::Radial };
    let denom = if style == GradientStyle::Linear {
        (w / angle.cos().abs().max(1e-6)).min(h / angle.sin().abs().max(1e-6))
    } else {
        ((angle.cos() * w).powi(2) + (angle.sin() * h).powi(2)).sqrt() / 2.
    };
    let center = if style == GradientStyle::Linear { photocraft_geom::Point::new((start.x + end.x) / 2., (start.y + end.y) / 2.) } else { start };
    let mut stops = g.stops.clone();
    stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    state.warn("gradient-projection", name, "Gradient stops and midpoint remain editable; arbitrary affine geometry is simplified to native Fill geometry.");
    if g.kind == paint::GradientKind::Conical {
        state.warn("conical-gradient", name, "Conical gradient imported as radial.");
    }
    Ok((
        Gradient {
            stops: stops.iter().map(|s| (s.offset as f32, color(s.color))).collect(),
            angle: angle.to_degrees() as f32,
            scale: ((end.x - start.x).hypot(end.y - start.y) / denom.max(1e-6)) as f32,
            style,
            reverse: false,
            opacity_stops: vec![],
            offset: (((center.x - (bounds.0 + bounds.2) / 2.) / w) as f32, ((center.y - (bounds.1 + bounds.3) / 2.) / h) as f32),
            align: true,
        },
        stops.iter().take(stops.len().saturating_sub(1)).map(|s| s.half_point() as f32).collect(),
    ))
}

pub(super) fn stroke(state: &mut State<'_, '_>, s: &paint::Stroke, m: Affine, bounds: (f64, f64, f64, f64), name: &str) -> Result<Option<ShapeStroke>, Error> {
    let Some(paint) = fill(state, &s.paint, m, bounds, name)? else { return Ok(None) };
    let width = s.width * m.determinant().abs().sqrt();
    if !width.is_finite() || !(0.001..=5000.).contains(&width) {
        return Err(Error::Limit("stroke width"));
    }
    let (dashes, phase) = s.dash.as_ref().map_or((vec![], 0.), |(v, p)| (v.iter().map(|d| (d / s.width) as f32).collect(), (p / s.width) as f32));
    Ok(Some(ShapeStroke {
        paint,
        width: width as f32,
        cap: match s.cap {
            paint::Cap::Butt => photocraft_doc::LineCap::Butt,
            paint::Cap::Round => photocraft_doc::LineCap::Round,
            paint::Cap::Square => photocraft_doc::LineCap::Square,
        },
        join: match s.join {
            paint::Join::Miter => photocraft_doc::LineJoin::Miter,
            paint::Join::Round => photocraft_doc::LineJoin::Round,
            paint::Join::Bevel => photocraft_doc::LineJoin::Bevel,
        },
        align: match s.align {
            paint::Align::Center => photocraft_doc::StrokeAlign::Center,
            paint::Align::Inside => photocraft_doc::StrokeAlign::Inside,
            paint::Align::Outside => photocraft_doc::StrokeAlign::Outside,
        },
        miter_limit: s.miter_limit as f32,
        dashes,
        dash_offset: phase,
        ..Default::default()
    }))
}
