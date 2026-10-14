//! Adobe perspective LCP evaluation in the single geometry resample. User imports
//! remain separate from redistributed lensfun data. Equations follow RawTherapee
//! `rtengine/lcp.cc` (Adobe Camera Model); unlike that implementation, vignette
//! compensation uses the exact reciprocal polynomial rather than a Taylor truncation.
use lightcraft_geom::{Orientation, Real};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LcpModel {
    pub focal: [f64; 2],
    pub center: [f64; 2],
    pub radial: [f64; 3],
    pub tangential: [f64; 2],
    pub scale: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LcpCorrection {
    pub distortion: Option<LcpModel>,
    pub chromatic: Option<[LcpModel; 3]>,
    pub vignette: Option<LcpModel>,
    /// The decoder has already applied this orientation to the source pixels.
    pub exif_orientation: Orientation,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LcpMap {
    pub correction: LcpCorrection,
    pub orientation: Orientation,
    pub source_w: f64,
    pub source_h: f64,
}

impl LcpModel {
    fn map<T: Real>(&self, x: T, y: T, w: f64, h: f64, scale: bool) -> (T, T) {
        let cx = self.center[0] * w;
        let cy = self.center[1] * h;
        let fx = self.focal[0] * w.max(h);
        let fy = self.focal[1] * w.max(h);
        let dx = (x - cx) / fx;
        let dy = (y - cy) / fy;
        let r2 = dx * dx + dy * dy;
        let f = ((r2 * self.radial[2] + self.radial[1]) * r2 + self.radial[0]) * r2 + 1.0;
        let common = f + (dy * self.tangential[0] + dx * self.tangential[1]) * 2.0;
        let s = if scale { self.scale } else { 1.0 };
        ((dx * common + r2 * self.tangential[1]) * (fx * s) + cx, (dy * common + r2 * self.tangential[0]) * (fy * s) + cy)
    }
    fn moves(&self) -> bool {
        self.radial != [0.0; 3] || self.tangential != [0.0; 2]
    }
    fn valid(&self) -> bool {
        self.focal.iter().all(|x| x.is_finite() && (0.001..=1000.0).contains(x))
            && self.center.iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            && self.radial.iter().chain(&self.tangential).all(|x| x.is_finite() && x.abs() <= 100.0)
            && self.scale.is_finite()
            && (0.1..=10.0).contains(&self.scale)
    }
}
impl LcpCorrection {
    pub fn valid(&self) -> bool {
        self.distortion.iter().chain(self.vignette.iter()).chain(self.chromatic.iter().flatten()).all(LcpModel::valid)
    }
    pub fn geometric_model(&self) -> Option<LcpModel> {
        self.chromatic.map(|m| m[1]).or(self.distortion)
    }
    pub fn moves_pixels(&self) -> bool {
        self.geometric_model().is_some_and(|m| m.moves())
            || self.chromatic.is_some_and(|m| {
                m[0].moves()
                    || m[2].moves()
                    || m[0].scale != 1.0
                    || m[2].scale != 1.0
                    || m[0].focal != m[1].focal
                    || m[2].focal != m[1].focal
                    || m[0].center != m[1].center
                    || m[2].center != m[1].center
            })
    }
    pub fn per_channel(&self) -> bool {
        self.chromatic.is_some_and(|m| m[0] != m[1] || m[1] != m[2])
    }
    pub fn has_gain(&self) -> bool {
        self.vignette.is_some_and(|m| m.radial != [0.0; 3])
    }
}

/// Generic orientation preserves interval geometry bounds as well as point mapping.
fn orient<T: Real>(o: Orientation, mut x: T, mut y: T, mut w: f64, mut h: f64) -> (T, T) {
    let (flip, turns) = o.to_parts();
    if flip {
        x = x * -1.0 + w;
    }
    for _ in 0..turns {
        (x, y) = (y * -1.0 + h, x);
        std::mem::swap(&mut w, &mut h);
    }
    (x, y)
}
impl LcpMap {
    fn calibration_dimensions(&self) -> (f64, f64) {
        if self.correction.exif_orientation.swaps_axes() { (self.source_h, self.source_w) } else { (self.source_w, self.source_h) }
    }
    fn map_to_calibration<T: Real>(&self, x: T, y: T) -> (T, T) {
        let (ow, oh) = if self.orientation.swaps_axes() { (self.source_h, self.source_w) } else { (self.source_w, self.source_h) };
        let (x, y) = orient(self.orientation.inverse(), x, y, ow, oh);
        orient(self.correction.exif_orientation.inverse(), x, y, self.source_w, self.source_h)
    }
    fn map_from_calibration<T: Real>(&self, x: T, y: T) -> (T, T) {
        let (w, h) = self.calibration_dimensions();
        let (x, y) = orient(self.correction.exif_orientation, x, y, w, h);
        orient(self.orientation, x, y, self.source_w, self.source_h)
    }
    pub fn to_source_real<T: Real>(&self, x: T, y: T, ch: usize, distortion: f64) -> (T, T) {
        let (mut qx, mut qy) = self.map_to_calibration(x, y);
        let (w, h) = self.calibration_dimensions();
        if let Some(m) = self.correction.geometric_model() {
            let (sx, sy) = m.map(qx, qy, w, h, false);
            (qx, qy) = (qx + (sx - qx) * distortion, qy + (sy - qy) * distortion);
        }
        if ch != 1
            && let Some(m) = self.correction.chromatic
        {
            // Green model is the reference coordinate frame. Red and blue encode
            // relative displacement, applied after green distortion exactly once.
            let g = m[1];
            let c = m[ch];
            let dx = (qx - g.center[0] * w) / (g.focal[0] * w.max(h));
            let dy = (qy - g.center[1] * h) / (g.focal[1] * w.max(h));
            let px = dx * (c.focal[0] * w.max(h)) + c.center[0] * w;
            let py = dy * (c.focal[1] * w.max(h)) + c.center[1] * h;
            (qx, qy) = c.map(px, py, w, h, true);
        }
        self.map_from_calibration(qx, qy)
    }
    pub fn gain(&self, x: f64, y: f64, strength: f64) -> f64 {
        let Some(m) = self.correction.vignette else { return 1.0 };
        let (x, y) = self.map_to_calibration(x, y);
        let (w, h) = self.calibration_dimensions();
        let dx = (x - m.center[0] * w) / (m.focal[0] * w.max(h));
        let dy = (y - m.center[1] * h) / (m.focal[1] * w.max(h));
        let r2 = dx * dx + dy * dy;
        let attenuation = ((m.radial[2] * r2 + m.radial[1]) * r2 + m.radial[0]) * r2 + 1.0;
        // Corrupt extrapolated polynomials must not inject NaNs or extreme gains.
        let gain = if attenuation.is_finite() && attenuation > 0.0 { attenuation.recip().clamp(0.0625, 16.0) } else { 1.0 };
        (1.0 + (gain - 1.0) * strength).clamp(0.0, 32.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_develop::DevelopSettings;
    use lightcraft_geom::Point;
    fn model() -> LcpModel {
        LcpModel { focal: [0.8, 1.2], center: [0.4, 0.6], radial: [-0.1, 0.01, 0.0], tangential: [0.001, -0.002], scale: 1.0 }
    }
    #[test]
    fn lcp_pipeline_matches_adobe_equation_and_exif_rotation() {
        let m = model();
        let map = LcpMap {
            correction: LcpCorrection { distortion: Some(m), ..Default::default() },
            orientation: Orientation::Normal,
            source_w: 600.0,
            source_h: 400.0,
        };
        let (x, y) = map.to_source_real(500.0, 100.0, 1, 1.0);
        let dx = (500.0 - 240.0) / 480.0;
        let dy = (100.0 - 240.0) / 720.0;
        let r2 = dx * dx + dy * dy;
        let f = 1.0 - 0.1 * r2 + 0.01 * r2 * r2;
        assert!((x - (240.0 + 480.0 * (dx * f + 2.0 * 0.001 * dx * dy - 0.002 * (r2 + 2.0 * dx * dx)))).abs() < 1e-10);
        assert!((y - (240.0 + 720.0 * (dy * f + 0.001 * (r2 + 2.0 * dy * dy) - 2.0 * 0.002 * dx * dy))).abs() < 1e-10);
        for exif in (1..=8).map(Orientation::from_exif) {
            let (sw, sh) = if exif.swaps_axes() { (400.0, 600.0) } else { (600.0, 400.0) };
            for user in (1..=8).map(Orientation::from_exif) {
                let oriented =
                    LcpMap { correction: LcpCorrection { exif_orientation: exif, ..map.correction }, orientation: user, source_w: sw, source_h: sh };
                let (px, py) = exif.map(500.0, 100.0, 600.0, 400.0);
                let (px, py) = user.map(px, py, sw, sh);
                let actual = oriented.to_source_real(px, py, 1, 1.0);
                let expected = exif.map(x, y, 600.0, 400.0);
                let expected = user.map(expected.0, expected.1, sw, sh);
                assert!((actual.0 - expected.0).abs() < 1e-10 && (actual.1 - expected.1).abs() < 1e-10);
            }
        }
    }
    #[test]
    fn lcp_pipeline_frame_applies_profile_once_and_has_cpu_fallback() -> Result<(), &'static str> {
        let c = LcpCorrection { distortion: Some(model()), vignette: Some(LcpModel { radial: [-0.2, 0.0, 0.0], ..model() }), ..Default::default() };
        let mut settings = DevelopSettings::default();
        settings.optics.lens_profile = true;
        let f = crate::geometry::Frame::with_lcp(600, 400, &settings, false, None, None, Some(&c));
        let warp = f.warp.as_ref().ok_or("LCP warp absent")?;
        assert!(!f.gpu_samplable());
        assert!(warp.to_source(Point::new(500.0, 100.0), 1).dist(Point::new(500.0, 100.0)) > 1.0);
        assert!(warp.gain(Point::new(500.0, 100.0)) > 1.0);
        settings.optics.lens_profile = false;
        assert!(crate::geometry::Frame::with_lcp(600, 400, &settings, false, None, None, Some(&c)).warp.is_none());
        Ok(())
    }
    #[test]
    fn lcp_pipeline_ca_is_relative_and_vignette_uses_exact_reciprocal() -> Result<(), &'static str> {
        let base = LcpModel { focal: [1.0; 2], center: [0.5; 2], radial: [0.0; 3], tangential: [0.0; 2], scale: 1.0 };
        let c = LcpCorrection {
            chromatic: Some([LcpModel { scale: 1.002, ..base }, base, LcpModel { scale: 0.999, ..base }]),
            vignette: Some(LcpModel { radial: [-0.2, 0.01, 0.0], ..base }),
            ..Default::default()
        };
        let m = LcpMap { correction: c, orientation: Orientation::Normal, source_w: 600.0, source_h: 400.0 };
        let r = m.to_source_real(500.0, 100.0, 0, 1.0);
        let g = m.to_source_real(500.0, 100.0, 1, 1.0);
        let b = m.to_source_real(500.0, 100.0, 2, 1.0);
        assert_eq!(g, (500.0, 100.0));
        assert!((r.0 - 500.4).abs() < 1e-12 && (r.1 - 99.8).abs() < 1e-12);
        assert!((b.0 - 499.8).abs() < 1e-12 && (b.1 - 100.1).abs() < 1e-12);
        let r2: f64 = (200.0 * 200.0 + 100.0 * 100.0) / (600.0 * 600.0);
        assert!((m.gain(500.0, 100.0, 1.0) - 1.0 / (1.0 - 0.2 * r2 + 0.01 * r2 * r2)).abs() < 1e-12);
        assert_eq!(m.gain(500.0, 100.0, 0.0), 1.0);
        let mut settings = DevelopSettings::default();
        settings.optics.lens_profile = true;
        let disabled = crate::geometry::Frame::with_lcp(600, 400, &settings, false, None, None, Some(&c));
        assert!(!disabled.warp.as_ref().ok_or("vignette warp absent")?.per_channel());
        settings.optics.remove_ca = true;
        let enabled = crate::geometry::Frame::with_lcp(600, 400, &settings, false, None, None, Some(&c));
        let warp = enabled.warp.as_ref().ok_or("LCP CA warp absent")?;
        assert!(warp.per_channel());
        assert!((warp.to_source(Point::new(500.0, 100.0), 0).x - 500.4).abs() < 1e-12);
        Ok(())
    }
}
