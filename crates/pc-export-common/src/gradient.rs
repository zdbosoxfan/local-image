//! Native gradient geometry and ramp are read from pc-vector::Paint, including #1321
//! pixel-corner sampling. Writers serialize the frame and stop samples below.
use super::*;
use photocraft_doc::{Fill, GradientStyle};
use photocraft_vector::Paint;

pub fn supported(f: &Fill) -> bool {
    match f {
        Fill::Solid(_) => true,
        Fill::Gradient { style, dither, .. } => !*dither && matches!(style, GradientStyle::Linear | GradientStyle::Reflected | GradientStyle::Radial),
        Fill::Pattern { .. } => false,
    }
}

pub struct Gradient {
    pub style: GradientStyle,
    /// Radial centre and radius.
    pub center: (f64, f64),
    pub radius: f64,
    /// Linear axis `[x1, y1, x2, y2]`, translated by half a pixel on both axes (pixel
    /// centres). The component perpendicular to the gradient does not affect paint.
    pub axis: [f64; 4],
    /// Ascending ramp offsets; side -1/1 is the left/right limit of a hard stop.
    pub samples: Vec<(f32, i8)>,
    ramp: Paint,
}

impl Gradient {
    pub fn new(doc: &Document, fill: &Fill, path: &Path) -> Result<Self> {
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
        let cx = frame.0 + w / 2.0 + f64::from(offset.0) * w;
        let cy = frame.1 + h / 2.0 + f64::from(offset.1) * h;
        let (lx, ly) = (cx + 0.5, cy + 0.5);
        let axis = [lx - c * chord / 2.0, ly + s * chord / 2.0, lx + c * chord / 2.0, ly - s * chord / 2.0];
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
        // Native colour-alpha times independent opacity is quadratic. SVG and PDF stops
        // interpolate straight channels, so only that product needs subdivisions.
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
        Ok(Self { style, center: (cx, cy), radius: len / 2.0, axis, samples, ramp })
    }
    /// Straight RGBA at a sample from [`Self::samples`].
    pub fn color(&self, u: f32, side: i8) -> [f32; 4] {
        let sample = (u + f32::from(side) * 0.000001).clamp(0.0, 1.0);
        let t = if self.style == GradientStyle::Reflected { (2.0 * sample - 1.0).abs() } else { sample };
        self.ramp.at(f64::from(t) + 0.5, 0.5)
    }
}
