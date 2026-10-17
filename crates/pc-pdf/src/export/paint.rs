//! Gradient mapping adapted from VectorCraft pdf/export.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Native frame/ramp conventions follow Local Image SVG export and pc-vector #1321.
use super::*;
use photocraft_doc::{Color, GradientStyle};
use photocraft_export_common::gradient::Gradient;
pub(super) use photocraft_export_common::gradient::supported;
pub(super) fn color(c: &Color) -> krilla::color::rgb::Color {
    let rgb = c.to_rgb().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    krilla::color::rgb::Color::new(rgb[0], rgb[1], rgb[2])
}
pub(super) fn convert(doc: &Document, fill: &Fill, path: &Path, context: &ExportContext<'_>) -> Result<(krilla::paint::Paint, f32)> {
    if let Fill::Solid(c) = fill {
        return Ok((color(c).into(), c.alpha));
    }
    let gradient = Gradient::new(doc, fill, path)?;
    let mut output = Vec::new();
    for &(u, side) in &gradient.samples {
        check_cancel(context)?;
        // PDF stitching bounds must be strictly increasing. Keep native left/right
        // limiting colors, but separate the discontinuity by one representable ULP.
        let offset = match side {
            -1 => u.next_down().max(0.0),
            1 => u.next_up().min(1.0),
            _ => u,
        };
        let rgba = gradient.color(u, side);
        output.push(krilla::paint::Stop { offset: norm(offset), color: color(&Color::rgb(rgba[0], rgba[1], rgba[2])).into(), opacity: norm(rgba[3]) });
    }
    output.sort_by(|a, b| a.offset.get().total_cmp(&b.offset.get()));
    output.dedup_by(|a, b| a.offset == b.offset);
    let paint = if gradient.style == GradientStyle::Radial {
        let (cx, cy) = gradient.center;
        krilla::paint::RadialGradient {
            fx: cx as f32,
            fy: cy as f32,
            fr: 0.0,
            cx: cx as f32,
            cy: cy as f32,
            cr: gradient.radius as f32,
            transform: Default::default(),
            spread_method: Default::default(),
            stops: output,
            anti_alias: false,
        }
        .into()
    } else {
        let [x1, y1, x2, y2] = gradient.axis.map(|v| v as f32);
        krilla::paint::LinearGradient { x1, y1, x2, y2, transform: Default::default(), spread_method: Default::default(), stops: output, anti_alias: false }
            .into()
    };
    Ok((paint, 1.0))
}
