//! Tone equalizer: exposure by luminance zone. Nine sliders set an exposure change (±2 EV) for
//! the zones −8 … 0 EV (relative to white); a smooth curve through them (a least-squares fit of
//! eight Gaussians) gives the change at any luminance. Which zone a pixel is in is read from a
//! *mask* — the luminance smoothed by an edge-aware (guided) filter — so whole objects move
//! together and local contrast is kept, unlike a tone curve.
//!
//! Ported from darktable's `src/iop/toneequal.c` (`build_interpolation_matrix`, `pseudo_solve`
//! of `gaussian_elimination.h`, `compute_correction_lut`, `pixel_correction`; the Euclidean-norm
//! luminance of `src/common/luminance_mask.h`; GPL-3.0-or-later, Aurélien Pierre), see
//! `docs/PORTS.md`. The mask uses the upstream linear-luminance EIGF, with quantized
//! guidance and geometric blending; exposure/contrast compensation precede filtering.
//! The correction table is interpolated for continuous slider response.

use lightcraft_raster::{Plane, Rgb32f};

/// Number of zone sliders (−8 … 0 EV).
pub const ZONES: usize = 9;
/// Darkest and brightest zone (EV relative to white).
pub const MIN_EV: f32 = -8.0;
pub const MAX_EV: f32 = 0.0;
/// Centres of the eight Gaussians (darktable's `centers_ops`).
const CENTERS_OPS: [f32; 8] = [-56.0 / 7.0, -48.0 / 7.0, -40.0 / 7.0, -32.0 / 7.0, -24.0 / 7.0, -16.0 / 7.0, -8.0 / 7.0, 0.0];
/// Zone centres (darktable's `centers_params`).
const CENTERS_ZONES: [f32; ZONES] = [-8.0, -7.0, -6.0, -5.0, -4.0, -3.0, -2.0, -1.0, 0.0];
/// Contrast compensation fulcrum (darktable's `CONTRAST_FULCRUM`, as EV).
pub const FULCRUM_EV: f32 = -4.0;
/// Entries of the correction table per EV.
const LUT_PER_EV: usize = 256;

/// darktable's curve smoothing slider (−2.33 … 1.67, 0 by default) → Gaussian σ (EV).
pub fn smoothing_sigma(slider: f64) -> f32 {
    std::f32::consts::SQRT_2.powf(1.0 + slider as f32)
}

/// The fitted correction: gain (linear) by mask exposure (EV).
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    factors: [f32; 8],
    sigma: f32,
    lut: Vec<f32>,
}

/// Solve the least-squares problem `A x ≈ y` (`A`: 9 × 8) by its normal equations, Gaussian
/// elimination with partial pivoting. `None` when singular.
fn least_squares(a: &[[f64; 8]; ZONES], y: &[f64; ZONES]) -> Option<[f64; 8]> {
    let mut m = [[0f64; 9]; 8];
    for i in 0..8 {
        for j in 0..8 {
            m[i][j] = (0..ZONES).map(|k| a[k][i] * a[k][j]).sum();
        }
        m[i][8] = (0..ZONES).map(|k| a[k][i] * y[k]).sum();
    }
    for col in 0..8 {
        let piv = (col..8).max_by(|&p, &q| m[p][col].abs().total_cmp(&m[q][col].abs()))?;
        if m[piv][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, piv);
        for r in 0..8 {
            if r != col {
                let f = m[r][col] / m[col][col];
                for c in col..9 {
                    m[r][c] -= f * m[col][c];
                }
            }
        }
    }
    let x: [f64; 8] = std::array::from_fn(|i| m[i][8] / m[i][i]);
    x.iter().all(|v| v.is_finite()).then_some(x)
}

impl Curve {
    /// Shared correction table for native renderers (the CPU uses the same interpolation).
    pub fn lut(&self) -> &[f32] {
        &self.lut
    }
    /// The curve through `zones` (EV changes, clamped to ±2) with Gaussians of `sigma` EV.
    /// `None` when every zone is 0 (nothing to do) or the fit is unstable.
    pub fn new(zones: &[f64; ZONES], sigma: f32) -> Option<Curve> {
        if zones.iter().all(|z| *z == 0.0) || sigma.is_nan() || sigma <= 0.0 {
            return None;
        }
        let denom = 2.0 * (sigma as f64).powi(2);
        let a: [[f64; 8]; ZONES] =
            std::array::from_fn(|i| std::array::from_fn(|j| (-((CENTERS_ZONES[i] - CENTERS_OPS[j]) as f64).powi(2) / denom).exp()));
        let y: [f64; ZONES] = std::array::from_fn(|i| zones[i].clamp(-2.0, 2.0).exp2());
        let x = least_squares(&a, &y)?;
        let factors = x.map(|v| v as f32);
        let n = (MAX_EV - MIN_EV) as usize * LUT_PER_EV;
        let mut c = Curve { factors, sigma, lut: Vec::new() };
        c.lut = (0..=n).map(|j| c.exact(MIN_EV + j as f32 / LUT_PER_EV as f32)).collect();
        Some(c)
    }

    /// The gain at `ev` from the Gaussians (darktable's `pixel_correction`), clamped to ¼…4.
    fn exact(&self, ev: f32) -> f32 {
        let ev = ev.clamp(MIN_EV, MAX_EV);
        let d = 2.0 * self.sigma * self.sigma;
        let r: f32 = CENTERS_OPS.iter().zip(&self.factors).map(|(c, f)| (-(ev - c) * (ev - c) / d).exp() * f).sum();
        r.clamp(0.25, 4.0)
    }

    /// The gain (linear) for a pixel whose mask reads `ev` (EV relative to white).
    #[inline]
    pub fn gain(&self, ev: f32) -> f32 {
        let t = (ev.clamp(MIN_EV, MAX_EV) - MIN_EV) * LUT_PER_EV as f32;
        let i = (t as usize).min(self.lut.len() - 2);
        let f = t - i as f32;
        self.lut[i] + (self.lut[i + 1] - self.lut[i]) * f
    }

    /// The fitted change (EV) at each zone centre (for drawing the curve).
    pub fn at_zones(&self) -> [f32; ZONES] {
        CENTERS_ZONES.map(|z| self.exact(z).log2())
    }
}

/// The luminance the mask is built from: log2 of the RGB Euclidean norm (darktable's default
/// estimator), floored at −16 EV.
#[inline]
pub fn log_norm(c: [f32; 3]) -> f32 {
    (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt().max(1.0 / 65536.0).log2()
}

/// The (pre-exposure) mask of `img`: [`log_norm`] smoothed by the fast guided filter with
/// Gaussian windows of `sigma` px and edge epsilon `eps` (EV²).
pub fn mask_plane(img: &Rgb32f, sigma: f32, eps: f32) -> Plane {
    mask_plane_adjusted(img, sigma, eps, 0.0, 0.0)
}

/// Upstream linear luminance_mask contrast/exposure, followed by faithful EIGF.
pub fn mask_plane_adjusted(img: &Rgb32f, sigma: f32, eps: f32, exposure: f32, contrast: f32) -> Plane {
    let gain = exposure.exp2();
    let slope = contrast.exp2();
    let lum = img.map(|c| {
        let n = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() * gain;
        ((n - 2f32.powi(-4)) * slope + 2f32.powi(-4)).max(2f32.powi(-16))
    });
    let mut p = crate::eigf::Params::new(sigma, eps);
    p.iterations = 2;
    p.quantization = 1.0;
    p.geometric = true;
    crate::eigf::filter(&lum, p).map(|v| v.max(2f32.powi(-16)).log2())
}

/// Exposure and contrast compensation of the mask (both EV; darktable's "mask exposure /
/// contrast compensation"), so its zones can be spread over the sliders.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaskAdjust {
    pub exposure: f32,
    pub contrast: f32,
}

impl MaskAdjust {
    /// The zone (EV) of a pixel whose pre-exposure mask is `m`, at photo exposure `ev`.
    #[inline]
    pub fn zone_ev(&self, m: f32, ev: f32) -> f32 {
        let v = m + ev + self.exposure;
        if self.contrast == 0.0 { v } else { FULCRUM_EV + (v - FULCRUM_EV) * self.contrast.exp2() }
    }
}

/// Mask preview: the zone as a grey level, posterised to the nine zones (black = −8 EV and
/// below, white = 0 EV and above).
pub fn preview_grey(zone_ev: f32) -> f32 {
    ((zone_ev.clamp(MIN_EV, MAX_EV) - MIN_EV).round() / (MAX_EV - MIN_EV)).clamp(0.0, 1.0)
}

/// Whether the tone equalizer of `s` changes anything.
pub fn active(s: &lightcraft_develop::DevelopSettings) -> bool {
    let t = &s.tone_eq;
    t.enabled && s.section_enabled("toneEq") && t.zones().iter().any(|z| *z != 0.0)
}

/// Whether the tool is on (its mask is computed, e.g. for the mask preview, even while every
/// zone is 0).
pub fn mask_wanted(s: &lightcraft_develop::DevelopSettings) -> bool {
    s.tone_eq.enabled && s.section_enabled("toneEq")
}

/// The fitted curve and mask compensation of `s` (`None` when the tool changes nothing).
pub fn of(s: &lightcraft_develop::DevelopSettings) -> Option<(Curve, MaskAdjust)> {
    if !active(s) {
        return None;
    }
    let t = &s.tone_eq;
    let curve = Curve::new(&t.zones(), smoothing_sigma(t.smoothing.clamp(-2.33, 1.67)))?;
    Some((curve, MaskAdjust { exposure: -s.light.exposure as f32, contrast: 0.0 }))
}

/// The mask's guided-filter radius (Gaussian σ, output px) and edge epsilon (EV²) for `s` at
/// `px_per_long` output pixels per long edge: the mask size is a diameter in % of the long edge;
/// Mask Edges 50 is the highlights/shadows base's epsilon, each 25 more halves it.
pub fn mask_sigma_eps(s: &lightcraft_develop::DevelopSettings, px_per_long: f64) -> (f32, f32) {
    let t = &s.tone_eq;
    let sigma = (t.size.clamp(0.1, 50.0) / 100.0 * px_per_long / 4.0) as f32;
    let eps = crate::local::BASE_EPS * 2f32.powf(((50.0 - t.refine.clamp(0.0, 100.0)) / 25.0) as f32);
    (sigma.max(0.5), eps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_zones_do_nothing() {
        assert!(Curve::new(&[0.0; ZONES], smoothing_sigma(0.0)).is_none());
    }

    #[test]
    fn a_zone_lifts_its_own_luminance_most() {
        let mut z = [0.0; ZONES];
        z[4] = 1.0; // −4 EV +1 stop
        let c = Curve::new(&z, smoothing_sigma(0.0)).unwrap();
        let at = c.at_zones();
        assert!((at[4] - 1.0).abs() < 0.25, "{at:?}");
        assert!(at[0].abs() < 0.2 && at[8].abs() < 0.2, "{at:?}");
        assert!((c.gain(-4.0) - c.exact(-4.0)).abs() < 1e-3);
        assert!(c.gain(-4.0) > c.gain(-1.5) && c.gain(-4.0) > c.gain(-6.5));
        // outside −8 … 0 EV the ends hold
        assert_eq!(c.gain(3.0), c.gain(0.0));
        assert_eq!(c.gain(-12.0), c.gain(-8.0));
    }

    #[test]
    fn uniform_zones_are_an_exposure_change_and_clamped() {
        let c = Curve::new(&[0.5; ZONES], smoothing_sigma(0.0)).unwrap();
        for ev in [-7.5f32, -4.2, -0.3] {
            assert!((c.gain(ev).log2() - 0.5).abs() < 0.08, "{ev}: {}", c.gain(ev).log2());
        }
        let c = Curve::new(&[9.0; ZONES], smoothing_sigma(0.0)).unwrap();
        assert!(c.gain(-4.0) <= 4.0);
    }

    #[test]
    fn the_mask_keeps_edges_and_smooths_texture() {
        // left half dark and textured, right half bright: the mask follows the edge
        let img = Rgb32f::from_fn(120, 40, |x, y| {
            let v = if x < 60 { 0.01 * (1.0 + 0.5 * (((x + y) % 2) as f32 - 0.5)) } else { 0.5 };
            [v, v, v]
        });
        let m = mask_plane(&img, 6.0, 0.1);
        let (dark, bright) = (m.get(20, 20), m.get(100, 20));
        assert!(bright - dark > 4.0, "{dark} {bright}");
        // Geometric EIGF deliberately retains some fine contrast; attenuation is measured
        // against the input instead of requiring the old log filter's near-flat result.
        let original = (0.0125f32 / 0.0075).log2();
        assert!((m.get(20, 20) - m.get(21, 20)).abs() < original * 0.75);
        let a = MaskAdjust { exposure: 1.0, contrast: 0.0 };
        assert_eq!(a.zone_ev(-5.0, 0.5), -3.5);
        let b = MaskAdjust { exposure: 0.0, contrast: 1.0 };
        assert_eq!(b.zone_ev(-5.0, 0.0), -6.0);
        assert_eq!(preview_grey(-8.0), 0.0);
        assert_eq!(preview_grey(0.0), 1.0);
        assert_eq!(preview_grey(-4.2), 0.5);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod refvec_tests {
    use super::*;
    #[test]
    fn upstream_solve_and_correction_vectors() {
        let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toneeq/solve.f64")).unwrap();
        let values: Vec<_> = bytes.as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect();
        assert_eq!(values.len(), 3 * (72 + 9 + 8));
        let mut correction = Vec::new();
        for (mode, chunk) in values.as_chunks::<89>().0.iter().enumerate() {
            let a: [[f64; 8]; 9] = std::array::from_fn(|r| std::array::from_fn(|c| chunk[r * 8 + c]));
            let y: [f64; 9] = chunk[72..81].try_into().unwrap();
            let x = least_squares(&a, &y).unwrap();
            let max = x.iter().zip(&chunk[81..]).map(|(a, b)| (a - b).abs()).fold(0.0f64, f64::max);
            eprintln!("toneeq solve {mode}: max abs {max:.12}");
            assert!(max < 2e-8);
            let sigma = [0.7, std::f32::consts::SQRT_2, 2.0][mode];
            let curve = Curve { factors: x.map(|v| v as f32), sigma, lut: Vec::new() };
            for i in 0..=512 {
                correction.push(curve.exact(-8.0 + i as f32 / 64.0));
            }
        }
        crate::test_vectors::compare("toneeq/gain.f32", &correction, 2e-6);
        // Also measure full curve construction with our higher-precision interpolation matrix.
        let expected = crate::test_vectors::read("toneeq/gain.f32");
        let zones = [0.2, -0.1, 0.3, 0.5, 0.8, 0.4, -0.3, -0.2, 0.1];
        for (mode, sigma) in [0.7, std::f32::consts::SQRT_2, 2.0].into_iter().enumerate() {
            let curve = Curve::new(&zones, sigma).unwrap();
            let max = (0..=512).map(|i| (curve.gain(-8.0 + i as f32 / 64.0) - expected[mode * 513 + i]).abs()).fold(0.0f32, f32::max);
            eprintln!("toneeq full curve {mode}: max abs {max:.9}");
            assert!(max < 6e-5);
        }
    }
}
