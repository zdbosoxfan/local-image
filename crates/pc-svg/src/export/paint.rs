//! Gradient defs / stacked paint adaptation of VectorCraft svg/export.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Native gradient geometry and ramp are read from pc-vector::Paint, including #1321
//! pixel-corner sampling. SVG interpolates at pixel centres; the half-pixel shift below
//! expresses that existing convention without changing the shared renderer.
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
pub(super) fn color(c: &Color) -> String {
    let rgb = c.to_rgb().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}
impl Writer<'_, '_, '_> {
    pub(super) fn paint(&mut self, fill: &Fill, path: &Path, attr: &str, opacity: f32) -> Result<String> {
        if let Fill::Solid(c) = fill {
            return Ok(format!(" {attr}=\"{}\" {attr}-opacity=\"{}\"", color(c), self.n(f64::from(c.alpha * opacity))));
        }
        if !supported(fill) {
            return Err(invalid("unsupported vector paint reached writer"));
        }
        let bounds = path.control_bounds().unwrap_or((0.0, 0.0, 1.0, 1.0));
        let r = self.doc.bounds();
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
        let (tag, geom) = if style == GradientStyle::Radial {
            ("radialGradient", format!(" cx=\"{}\" cy=\"{}\" r=\"{}\"", self.n(cx), self.n(cy), self.n(len / 2.0)))
        } else {
            // Translate both axes by half a pixel. The component perpendicular to
            // the gradient does not affect paint; keeping it prevents native endpoint
            // floor-snapping from drifting after import/export.
            cx += 0.5;
            cy += 0.5;
            let snapped = |v: f64| {
                let grid = (v - 0.5).round() + 0.5;
                if (v - grid).abs() < 1e-4 { grid } else { v }
            };
            (
                "linearGradient",
                format!(
                    " x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"",
                    self.n(snapped(cx - c * chord / 2.0)),
                    self.n(snapped(cy + s * chord / 2.0)),
                    self.n(snapped(cx + c * chord / 2.0)),
                    self.n(snapped(cy - s * chord / 2.0))
                ),
            )
        };
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
        // Native colour-alpha times independent opacity is quadratic. SVG stops
        // interpolate straight RGBA, so only that product needs subdivisions.
        // An imported SVG already has one alpha ramp and must retain its stop count.
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
        let mut markup = String::new();
        for (u, side) in samples {
            check_cancel(self.cx)?;
            let sample = (u + f32::from(side) * 0.000001).clamp(0.0, 1.0);
            let t = if style == GradientStyle::Reflected { (2.0 * sample - 1.0).abs() } else { sample };
            let rgba = ramp.at(f64::from(t) + 0.5, 0.5);
            // RGB percentages retain sub-byte colours (hex would accumulate quantisation).
            markup.push_str(&format!(
                "<stop offset=\"{}\" stop-color=\"rgb({}%,{}%,{}%)\" stop-opacity=\"{}\"/>",
                number(f64::from(u), 9),
                number(f64::from(rgba[0]) * 100.0, 6),
                number(f64::from(rgba[1]) * 100.0, 6),
                number(f64::from(rgba[2]) * 100.0, 6),
                number(f64::from(rgba[3]), 9)
            ));
        }
        let key = format!("{tag}{geom}{markup}");
        let id = if let Some(id) = self.resources.get(&key) {
            id.clone()
        } else {
            let id = self.id("gradient");
            if self.defs.len() + markup.len() + self.body.len() > self.opts.max_bytes {
                return Err(ExportError::Budget("gradient bytes"));
            }
            self.defs.push_str(&format!("<{tag} id=\"{id}\" gradientUnits=\"userSpaceOnUse\" color-interpolation=\"sRGB\"{geom}>{markup}</{tag}>"));
            self.resources.insert(key, id.clone());
            id
        };
        Ok(format!(" {attr}=\"url(#{id})\" {attr}-opacity=\"{}\"", self.n(f64::from(opacity))))
    }
}
