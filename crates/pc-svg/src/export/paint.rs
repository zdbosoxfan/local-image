//! Gradient defs / stacked paint adaptation of VectorCraft svg/export.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Native gradient geometry and ramp are read from pc-vector::Paint, including #1321
//! pixel-corner sampling. SVG interpolates at pixel centres; the half-pixel shift below
//! expresses that existing convention without changing the shared renderer.
use super::*;
use photocraft_doc::{Color, GradientStyle};
use photocraft_export_common::gradient::Gradient;
pub(super) use photocraft_export_common::gradient::supported;

pub(super) fn color(c: &Color) -> String {
    let rgb = c.to_rgb().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}
impl Writer<'_, '_, '_> {
    pub(super) fn paint(&mut self, fill: &Fill, path: &Path, attr: &str, opacity: f32) -> Result<String> {
        if let Fill::Solid(c) = fill {
            return Ok(format!(" {attr}=\"{}\" {attr}-opacity=\"{}\"", color(c), self.n(f64::from(c.alpha * opacity))));
        }
        let gradient = Gradient::new(self.doc, fill, path)?;
        let (tag, geom) = if gradient.style == GradientStyle::Radial {
            let (cx, cy) = gradient.center;
            ("radialGradient", format!(" cx=\"{}\" cy=\"{}\" r=\"{}\"", self.n(cx), self.n(cy), self.n(gradient.radius)))
        } else {
            // The half-pixel axis keeps native endpoint floor-snapping from drifting
            // after import/export.
            let snapped = |v: f64| {
                let grid = (v - 0.5).round() + 0.5;
                if (v - grid).abs() < 1e-4 { grid } else { v }
            };
            let [x1, y1, x2, y2] = gradient.axis.map(snapped);
            ("linearGradient", format!(" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"", self.n(x1), self.n(y1), self.n(x2), self.n(y2)))
        };
        let mut markup = String::new();
        for &(u, side) in &gradient.samples {
            check_cancel(self.cx)?;
            let rgba = gradient.color(u, side);
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
