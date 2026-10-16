//! Gradient mapping adapted from VectorCraft pdf/export.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Native frame/ramp conventions follow Local Image SVG export and pc-vector #1321.
use super::*;
use photocraft_doc::{Color, GradientStyle};
use photocraft_vector::Paint;
pub(super) fn supported(f: &Fill) -> bool {
    match f {
        Fill::Solid(_) => true,
        Fill::Gradient { style, dither, .. } => !*dither && matches!(style, GradientStyle::Linear | GradientStyle::Reflected | GradientStyle::Radial),
        Fill::Pattern { .. } => false,
    }
}
pub(super) fn color(c: &Color) -> krilla::color::rgb::Color {
    let rgb = c.to_rgb().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    krilla::color::rgb::Color::new(rgb[0], rgb[1], rgb[2])
}
pub(super) fn convert(doc: &Document, fill: &Fill, path: &Path, context: &ExportContext<'_>) -> Result<(krilla::paint::Paint, f32)> {
    if let Fill::Solid(c) = fill {
        return Ok((color(c).into(), c.alpha));
    }
    if !supported(fill) {
        return Err(invalid("unsupported vector paint reached writer"));
    }
    let bounds = path.control_bounds().unwrap_or((0.0, 0.0, 1.0, 1.0));
    let r = doc.bounds();
    let paint = Paint::from_fill(fill, bounds, (r.x0 as f64, r.y0 as f64, r.x1 as f64, r.y1 as f64));
    let Paint::Gradient { stops, midpoints, opacity_stops, style, layout, reverse, frame, .. } = paint else {
        return Err(invalid("gradient expected"));
    };
    let (angle, scale, offset) = layout;
    let (w, h) = ((frame.2 - frame.0).max(1.0), (frame.3 - frame.1).max(1.0));
    let (s, c) = f64::from(angle).to_radians().sin_cos();
    let scale = f64::from(scale.max(1e-3));
    let chord = (w / c.abs().max(1e-6)).min(h / s.abs().max(1e-6)).max(1.0) * scale;
    let len = ((c * w).powi(2) + (s * h).powi(2)).sqrt().max(1.0) * scale;
    let mut cx = frame.0 + w / 2.0 + f64::from(offset.0) * w;
    let mut cy = frame.1 + h / 2.0 + f64::from(offset.1) * h;
    // Include every ramp breakpoint plus bounded subdivisions for the product of
    // colour alpha and independent opacity (quadratic between breakpoints).
    let mut positions = vec![0.0_f32, 1.0];
    positions.extend(stops.iter().map(|s| s.0));
    positions.extend(opacity_stops.iter().map(|s| s.0));
    for (i, pair) in stops.windows(2).enumerate() {
        let midpoint = midpoints.get(i).copied().unwrap_or(0.5).clamp(0.05, 0.95);
        if (midpoint - 0.5).abs() >= 1e-6 {
            positions.push(pair[0].0 + (pair[1].0 - pair[0].0) * midpoint);
        }
    }
    if reverse {
        for p in &mut positions {
            *p = 1.0 - *p;
        }
    }
    if style == GradientStyle::Reflected {
        positions = positions.into_iter().flat_map(|p| [(1.0 - p) / 2.0, (1.0 + p) / 2.0]).collect();
    }
    positions.sort_by(f32::total_cmp);
    positions.dedup();
    let mut samples = positions.clone();
    // Native colour-alpha times independent opacity is quadratic. PDF shading
    // stops interpolate straight channels, so that product needs subdivisions.
    if !opacity_stops.is_empty() && stops.windows(2).any(|p| p[0].1[3] != p[1].1[3]) {
        for pair in positions.windows(2) {
            for i in 1..16 {
                samples.push(pair[0] + (pair[1] - pair[0]) * i as f32 / 16.0);
            }
        }
    }
    samples.sort_by(f32::total_cmp);
    let mut hard = stops.windows(2).filter(|p| p[0].0 == p[1].0).map(|p| if reverse { 1.0 - p[0].0 } else { p[0].0 }).collect::<Vec<_>>();
    hard.extend(opacity_stops.windows(2).filter(|p| p[0].0 == p[1].0).map(|p| if reverse { 1.0 - p[0].0 } else { p[0].0 }));
    if style == GradientStyle::Reflected {
        hard = hard.into_iter().flat_map(|p| [(1.0 - p) / 2.0, (1.0 + p) / 2.0]).collect();
    }
    let mut samples = samples.into_iter().filter(|u| !hard.contains(u)).map(|u| (u, 0_i8)).collect::<Vec<_>>();
    for u in hard {
        samples.push((u, -1));
        samples.push((u, 1));
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    samples.dedup();
    let ramp = Paint::Gradient {
        stops,
        midpoints,
        opacity_stops,
        style: GradientStyle::Linear,
        layout: (0.0, 1.0, (0.0, 0.0)),
        reverse,
        dither: false,
        frame: (0.0, 0.0, 1.0, 1.0),
    };

    let mut output = Vec::new();
    for (u, side) in samples {
        check_cancel(context)?;
        let sample = (u + f32::from(side) * 0.000001).clamp(0.0, 1.0);
        // PDF stitching bounds must be strictly increasing. Keep native left/right
        // limiting colors, but separate the discontinuity by one representable ULP.
        let offset = match side {
            -1 => u.next_down().max(0.0),
            1 => u.next_up().min(1.0),
            _ => u,
        };
        let t = if style == GradientStyle::Reflected { (2.0 * sample - 1.0).abs() } else { sample };
        let rgba = ramp.at(f64::from(t) + 0.5, 0.5);
        output.push(krilla::paint::Stop { offset: norm(offset), color: color(&Color::rgb(rgba[0], rgba[1], rgba[2])).into(), opacity: norm(rgba[3]) });
    }
    output.sort_by(|a, b| a.offset.get().total_cmp(&b.offset.get()));
    output.dedup_by(|a, b| a.offset == b.offset);
    let paint = if style == GradientStyle::Radial {
        krilla::paint::RadialGradient {
            fx: cx as f32,
            fy: cy as f32,
            fr: 0.0,
            cx: cx as f32,
            cy: cy as f32,
            cr: (len / 2.0) as f32,
            transform: Default::default(),
            spread_method: Default::default(),
            stops: output,
            anti_alias: false,
        }
        .into()
    } else {
        cx += 0.5;
        cy += 0.5;
        krilla::paint::LinearGradient {
            x1: (cx - c * chord / 2.0) as f32,
            y1: (cy + s * chord / 2.0) as f32,
            x2: (cx + c * chord / 2.0) as f32,
            y2: (cy - s * chord / 2.0) as f32,
            transform: Default::default(),
            spread_method: Default::default(),
            stops: output,
            anti_alias: false,
        }
        .into()
    };
    Ok((paint, 1.0))
}
