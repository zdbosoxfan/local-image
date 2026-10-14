//! Camera Raw PSD interchange descriptor and validation.
//! Processing is provided by `photocraft_io::develop_filter`, shared with Library Develop.

use serde::{Deserialize, Serialize};

/// Most points a Camera Raw point curve accepts from commands and the control channel.
pub const MAX_CURVE_POINTS: usize = 16;

/// Checks a point curve from untrusted input: empty (linear) or 2..=16 finite points in 0..=255
/// with inputs increasing by at least one level. Deserialization stays lenient on purpose:
/// earlier editors saved curves this rejects; the shared Develop spline sorts and
/// de-duplicates them, so stored Smart Filters keep rendering.
pub fn validate_curve(points: &[[f32; 2]]) -> Result<(), String> {
    if points.len() == 1 {
        return Err("a nonempty curve needs at least two points".into());
    }
    if points.len() > MAX_CURVE_POINTS {
        return Err(format!("at most {MAX_CURVE_POINTS} curve points"));
    }
    if !points.iter().flatten().all(|v| v.is_finite() && (0.0..=255.0).contains(v)) {
        return Err("curve coordinates must be finite and in 0..=255".into());
    }
    if points.windows(2).any(|w| w[1][0] - w[0][0] < 1.0) {
        return Err("curve inputs must increase by at least one level".into());
    }
    Ok(())
}

/// One colour grading wheel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Wheel {
    /// Degrees 0..360.
    pub hue: f32,
    /// 0..=100.
    pub sat: f32,
    /// −100..=100.
    pub lum: f32,
}

impl Wheel {
    fn neutral(&self) -> bool {
        self.sat == 0.0 && self.lum == 0.0
    }
}

/// Camera Raw settings in Adobe Camera Raw's slider units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CameraRaw {
    // Basic
    pub temperature: f32,
    pub tint: f32,
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub texture: f32,
    pub clarity: f32,
    pub dehaze: f32,
    pub vibrance: f32,
    pub saturation: f32,
    // Tone curve
    pub curve_highlights: f32,
    pub curve_lights: f32,
    pub curve_darks: f32,
    pub curve_shadows: f32,
    /// Region splits (percent).
    pub curve_splits: [f32; 3],
    /// Point curves as `[input, output]` in 0..=255 (empty = linear).
    pub point_curve: Vec<[f32; 2]>,
    pub point_curve_red: Vec<[f32; 2]>,
    pub point_curve_green: Vec<[f32; 2]>,
    pub point_curve_blue: Vec<[f32; 2]>,
    // Color mixer: red, orange, yellow, green, aqua, blue, purple, magenta.
    pub hsl_hue: [f32; 8],
    pub hsl_sat: [f32; 8],
    pub hsl_lum: [f32; 8],
    // Color grading
    pub grade_shadows: Wheel,
    pub grade_midtones: Wheel,
    pub grade_highlights: Wheel,
    pub grade_global: Wheel,
    pub grade_blending: f32,
    pub grade_balance: f32,
    // Detail
    pub sharpen_amount: f32,
    pub sharpen_radius: f32,
    pub sharpen_detail: f32,
    pub sharpen_masking: f32,
    pub noise_luminance: f32,
    pub noise_luminance_detail: f32,
    pub noise_color: f32,
    pub noise_color_detail: f32,
    // Effects
    pub grain_amount: f32,
    pub grain_size: f32,
    pub grain_roughness: f32,
    pub vignette_amount: f32,
    pub vignette_midpoint: f32,
    pub vignette_roundness: f32,
    pub vignette_feather: f32,
    pub vignette_highlights: f32,
    /// `highlightPriority | colorPriority | paintOverlay`.
    pub vignette_style: String,
    pub seed: u32,
    /// Pixel-size multiplier for previews on downsampled proxies (radii in px scale by it).
    pub pixel_scale: f32,
}

impl Default for CameraRaw {
    fn default() -> Self {
        CameraRaw {
            temperature: 0.0,
            tint: 0.0,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            texture: 0.0,
            clarity: 0.0,
            dehaze: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            curve_highlights: 0.0,
            curve_lights: 0.0,
            curve_darks: 0.0,
            curve_shadows: 0.0,
            curve_splits: [25.0, 50.0, 75.0],
            point_curve: Vec::new(),
            point_curve_red: Vec::new(),
            point_curve_green: Vec::new(),
            point_curve_blue: Vec::new(),
            hsl_hue: [0.0; 8],
            hsl_sat: [0.0; 8],
            hsl_lum: [0.0; 8],
            grade_shadows: Wheel::default(),
            grade_midtones: Wheel::default(),
            grade_highlights: Wheel::default(),
            grade_global: Wheel::default(),
            grade_blending: 50.0,
            grade_balance: 0.0,
            sharpen_amount: 0.0,
            sharpen_radius: 1.0,
            sharpen_detail: 25.0,
            sharpen_masking: 0.0,
            noise_luminance: 0.0,
            noise_luminance_detail: 50.0,
            noise_color: 0.0,
            noise_color_detail: 50.0,
            grain_amount: 0.0,
            grain_size: 25.0,
            grain_roughness: 50.0,
            vignette_amount: 0.0,
            vignette_midpoint: 50.0,
            vignette_roundness: 0.0,
            vignette_feather: 50.0,
            vignette_highlights: 0.0,
            vignette_style: "highlightPriority".into(),
            seed: 0,
            pixel_scale: 1.0,
        }
    }
}

/// Hue band centres of the Color Mixer (degrees).
pub const HSL_BANDS: [f32; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];

impl CameraRaw {
    /// Rejects malformed point curves in new settings (see [`validate_curve`]).
    pub fn validate(&self) -> Result<(), String> {
        for (name, curve) in [
            ("pointCurve", &self.point_curve),
            ("pointCurveRed", &self.point_curve_red),
            ("pointCurveGreen", &self.point_curve_green),
            ("pointCurveBlue", &self.point_curve_blue),
        ] {
            validate_curve(curve).map_err(|e| format!("{name}: {e}"))?;
        }
        Ok(())
    }
    /// [`validate`](Self::validate) for an edit of `stored` settings: a curve left as stored is
    /// accepted, so editing another control of an older Smart Filter still works.
    pub fn validate_changes(&self, stored: &CameraRaw) -> Result<(), String> {
        for (name, curve, kept) in [
            ("pointCurve", &self.point_curve, &stored.point_curve),
            ("pointCurveRed", &self.point_curve_red, &stored.point_curve_red),
            ("pointCurveGreen", &self.point_curve_green, &stored.point_curve_green),
            ("pointCurveBlue", &self.point_curve_blue, &stored.point_curve_blue),
        ] {
            if curve != kept {
                validate_curve(curve).map_err(|e| format!("{name}: {e}"))?;
            }
        }
        Ok(())
    }
    fn wb_neutral(&self) -> bool {
        self.temperature == 0.0 && self.tint == 0.0 && self.exposure == 0.0
    }
    fn tone_neutral(&self) -> bool {
        [self.contrast, self.highlights, self.shadows, self.whites, self.blacks, self.texture, self.clarity].iter().all(|v| *v == 0.0)
    }
    fn hsl_neutral(&self) -> bool {
        self.hsl_hue.iter().chain(&self.hsl_sat).chain(&self.hsl_lum).all(|v| *v == 0.0)
    }
    fn grade_neutral(&self) -> bool {
        self.grade_shadows.neutral() && self.grade_midtones.neutral() && self.grade_highlights.neutral() && self.grade_global.neutral()
    }
    fn curves_neutral(&self) -> bool {
        [self.curve_highlights, self.curve_lights, self.curve_darks, self.curve_shadows].iter().all(|v| *v == 0.0)
            && [&self.point_curve, &self.point_curve_red, &self.point_curve_green, &self.point_curve_blue].iter().all(|c| is_linear(c))
    }
    /// True when every control is neutral (the filter is an identity).
    pub fn is_identity(&self) -> bool {
        self.wb_neutral()
            && self.tone_neutral()
            && self.dehaze == 0.0
            && self.vibrance == 0.0
            && self.saturation == 0.0
            && self.curves_neutral()
            && self.hsl_neutral()
            && self.grade_neutral()
            && self.sharpen_amount == 0.0
            && self.noise_luminance == 0.0
            && self.noise_color == 0.0
            && self.grain_amount == 0.0
            && self.vignette_amount == 0.0
    }
}

fn is_linear(c: &[[f32; 2]]) -> bool {
    c.len() < 2 || c.iter().all(|p| (p[0] - p[1]).abs() < 1e-3)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_curves_are_validated_but_saved_curves_still_load_and_render() {
        for field in ["pointCurve", "pointCurveRed", "pointCurveGreen", "pointCurveBlue"] {
            for points in [
                serde_json::json!([[128, 128]]),
                serde_json::json!([[255, 255], [0, 0]]),
                serde_json::json!([[0, 0], [0, 255]]),
                serde_json::json!([[0, -1], [255, 255]]),
                serde_json::json!([[0, 0], [256, 255]]),
                serde_json::json!([[0, 0], [1e30, 255]]),
                serde_json::json!((0..17).map(|i| [i * 15, i * 15]).collect::<Vec<_>>()),
            ] {
                let p: CameraRaw = serde_json::from_value(serde_json::json!({field: points})).unwrap();
                assert!(p.validate().is_err(), "{field}: {points}");
            }
            for points in
                [serde_json::json!([]), serde_json::json!([[0, 0], [255, 255]]), serde_json::json!((0..16).map(|i| [i * 17, i * 17]).collect::<Vec<_>>())]
            {
                let p: CameraRaw = serde_json::from_value(serde_json::json!({field: points})).unwrap();
                assert!(p.validate().is_ok(), "{field}: {points}");
            }
        }
        assert_eq!(serde_json::from_str::<CameraRaw>("{}").unwrap(), CameraRaw::default());
        // An older editor could stack two points on one input and exceed 16 points; such a
        // saved Smart Filter must still develop instead of silently disappearing.
        let mut legacy: Vec<[f32; 2]> = (0..20).map(|i| [i as f32 * 12.0, i as f32 * 12.0]).collect();
        legacy.insert(5, [60.0, 200.0]);
        let p: CameraRaw = serde_json::from_value(serde_json::json!({"pointCurve": legacy})).unwrap();
        assert!(p.validate().is_err());
        let edited = CameraRaw { exposure: 1.0, ..p.clone() };
        assert!(edited.validate_changes(&p).is_ok(), "an unchanged stored curve stays editable");
        let broken = CameraRaw { point_curve: vec![[10.0, 10.0]], ..p.clone() };
        assert!(broken.validate_changes(&p).is_err(), "a changed curve is validated");
    }
}

#[cfg(test)]
mod j04_tests {
    use super::*;
    #[test]
    fn j04_descriptor_defaults_and_legacy_metadata_still_load() {
        let value = serde_json::json!({"exposure":0.5,"pixelScale":0.25,"seed":42,"__acrDescriptor":{"old":true}});
        let descriptor: CameraRaw = serde_json::from_value(value).unwrap();
        assert_eq!(descriptor.exposure, 0.5);
        assert_eq!(descriptor.pixel_scale, 0.25);
        assert_eq!(descriptor.seed, 42);
        let saved = serde_json::to_value(&descriptor).unwrap();
        assert_eq!(serde_json::from_value::<CameraRaw>(saved).unwrap(), descriptor);
        assert_eq!(serde_json::from_str::<CameraRaw>("{}").unwrap(), CameraRaw::default());
    }
}
