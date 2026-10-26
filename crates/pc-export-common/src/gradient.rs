//! Native gradient geometry and ramp are read from pc-vector::Paint, including #1321
//! pixel-corner sampling. Writers serialize the frame and stop samples below.
use super::*;
use photocraft_doc::{Fill, GradientMethod, GradientStyle};
use photocraft_vector::Paint;
use photocraft_vector::gradient_method::{MAX_EXPANDED_STOPS, refine_samples, sample_limits};

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
        if let Fill::Gradient { stops, opacity_stops, midpoints, .. } = fill
            && stops.len().saturating_add(opacity_stops.len()).saturating_add(midpoints.len()) > MAX_EXPANDED_STOPS
        {
            return Err(invalid("gradient has too many source stops; simplify the ramp"));
        }
        let bounds = path.control_bounds().unwrap_or((0.0, 0.0, 1.0, 1.0));
        let r = doc.bounds();
        let paint = Paint::from_fill(fill, bounds, (r.x0 as f64, r.y0 as f64, r.x1 as f64, r.y1 as f64));
        let Paint::Gradient { method, stops, midpoints, opacity_stops, style, layout, reverse, frame, .. } = paint else {
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
            if method == GradientMethod::Perceptual && positions.len().saturating_mul(16) > MAX_EXPANDED_STOPS {
                return Err(invalid("gradient alpha needs too many samples; simplify the ramp"));
            }
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
        if method == GradientMethod::Perceptual && samples.len() > MAX_EXPANDED_STOPS {
            return Err(invalid("gradient needs too many samples; simplify the ramp"));
        }
        let ramp = Paint::Gradient {
            method,
            stops,
            midpoints,
            opacity_stops,
            style: GradientStyle::Linear,
            layout: (0.0, 1.0, (0.0, 0.0)),
            reverse,
            dither: false,
            frame: (0.0, 0.0, 1.0, 1.0),
        };
        let mut gradient = Self { style, center: (cx, cy), radius: len / 2.0, axis, samples, ramp };
        if method == GradientMethod::Perceptual {
            gradient.samples = refine_samples(std::mem::take(&mut gradient.samples), |u, side| gradient.color(u, side)).map_err(invalid)?;
        }
        Ok(gradient)
    }
    /// Straight RGBA at a sample from [`Self::samples`].
    pub fn color(&self, u: f32, side: i8) -> [f32; 4] {
        if side != 0
            && let Paint::Gradient { method: GradientMethod::Perceptual, stops, midpoints, opacity_stops, reverse, .. } = &self.ramp
        {
            let position = stops.iter().map(|s| s.0).chain(opacity_stops.iter().map(|s| s.0)).find(|t| {
                let t = if *reverse { 1.0 - t } else { *t };
                if self.style == GradientStyle::Reflected { (1.0 - t) / 2.0 == u || (1.0 + t) / 2.0 == u } else { t == u }
            });
            if let Some(t) = position {
                let (left, right) = sample_limits(GradientMethod::Perceptual, stops, midpoints, opacity_stops, t);
                let increasing = (self.style != GradientStyle::Reflected || u >= 0.5) != *reverse;
                return if (side < 0) == increasing { left } else { right };
            }
        }
        let sample = (u + f32::from(side) * 0.000001).clamp(0.0, 1.0);
        let t = if self.style == GradientStyle::Reflected { (2.0 * sample - 1.0).abs() } else { sample };
        self.ramp.at(f64::from(t) + 0.5, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::{Color, ColorMode, SampleType, Size};

    #[test]
    fn classic_export_retains_large_alpha_and_hard_stop_ramps() {
        let doc = Document::new("Gradient", Size::new(64, 32), ColorMode::Rgb, SampleType::U8);
        let path = photocraft_vector::shapes::rect(0.0, 0.0, 64.0, 32.0);
        let mut alpha = Fill::gradient(
            (0..65).map(|i| (i as f32 / 64.0, Color::rgba(0.0, 0.0, 0.0, i as f32 / 64.0))).collect(),
            0.0, 1.0, GradientStyle::Reflected, false,
        );
        if let Fill::Gradient { opacity_stops, midpoints, .. } = &mut alpha {
            *opacity_stops = (0..65).map(|i| (i as f32 / 64.0, 1.0 - i as f32 / 64.0)).collect();
            *midpoints = vec![0.25; 64];
        }
        let hard = Fill::gradient(
            (0..1100).flat_map(|i| [(i as f32 / 1099.0, Color::BLACK), (i as f32 / 1099.0, Color::WHITE)]).collect(),
            0.0, 1.0, GradientStyle::Reflected, false,
        );
        for mut fill in [alpha, hard] {
            let gradient = Gradient::new(&doc, &fill, &path).expect("Classic ramps retain their pre-Perceptual export acceptance");
            assert!(gradient.samples.len() > MAX_EXPANDED_STOPS);
            assert!(gradient.samples.iter().all(|(t, side)| gradient.color(*t, *side).iter().all(|v| v.is_finite())));
            if let Fill::Gradient { method, .. } = &mut fill {
                *method = GradientMethod::Perceptual;
            }
            assert!(Gradient::new(&doc, &fill, &path).is_err(), "Perceptual expansion stays bounded");
        }
    }

    #[test]
    fn perceptual_export_keeps_hard_stops_and_bounds_every_smooth_span() {
        let doc = Document::new("Gradient", Size::new(64, 32), ColorMode::Rgb, SampleType::U8);
        let path = photocraft_vector::shapes::rect(0.0, 0.0, 64.0, 32.0);
        for style in [GradientStyle::Linear, GradientStyle::Radial, GradientStyle::Reflected] {
            for reverse in [false, true] {
                let mut fill = Fill::gradient(
                    vec![(0.0, Color::BLACK), (0.5, Color::rgb(0.0, 0.0, 1.0)), (0.5, Color::rgb(1.0, 1.0, 0.0)), (1.0, Color::WHITE)],
                    0.0,
                    1.0,
                    style,
                    reverse,
                );
                if let Fill::Gradient { method, .. } = &mut fill {
                    *method = GradientMethod::Perceptual;
                }
                let gradient = Gradient::new(&doc, &fill, &path).unwrap();
                let hard: &[f32] = if style == GradientStyle::Reflected { &[0.25, 0.75] } else { &[0.5] };
                for t in hard {
                    assert!(gradient.samples.contains(&(*t, -1)) && gradient.samples.contains(&(*t, 1)));
                    let (a, b) = (gradient.color(*t, -1), gradient.color(*t, 1));
                    assert!((a[0] - b[0]).abs() > 0.9, "hard stop at {t}: {a:?} {b:?}");
                }
                for pair in gradient.samples.windows(2).filter(|p| p[0].0 < p[1].0) {
                    let (a, b) = (gradient.color(pair[0].0, pair[0].1), gradient.color(pair[1].0, pair[1].1));
                    for u in [0.125, 0.375, 0.625, 0.875] {
                        let actual = gradient.color(pair[0].0 + (pair[1].0 - pair[0].0) * u, 0);
                        for ch in 0..4 {
                            assert!((actual[ch] - (a[ch] + (b[ch] - a[ch]) * u)).abs() < 0.4 / 255.0, "{style:?} reverse={reverse}");
                        }
                    }
                }
            }
        }
        let fill = Fill::gradient(vec![(0.0, Color::BLACK); MAX_EXPANDED_STOPS + 1], 0.0, 1.0, GradientStyle::Linear, false);
        assert!(Gradient::new(&doc, &fill, &path).is_err());
    }
}
