//! Lens profile corrections from the lensfun database: distortion, transverse chromatic aberration
//! (TCA) and vignetting, as the inverse map the geometry resample needs (a point of the corrected
//! image → where it is in the source) plus the vignetting gain at a source point.
//!
//! The models and their normalisation follow lensfun (`libs/lensfun/mod-coord.cpp`,
//! `mod-subpix.cpp`, `mod-color.cpp`, `modifier.cpp`; LGPL-3.0-or-later), as ported to Rust by the
//! `lensfun` crate, whose `Modifier` (correction mode) the engine's tests compare against. The
//! engine (`lightcraft_engine::lens_db`) looks the lens up, interpolates the calibration for the
//! shot and rescales it to the image (lensfun's `rescale_*`); this module only evaluates the
//! result, so the pipeline needs no database. See `docs/PORTS.md`.
//!
//! In correction mode every model is a closed-form polynomial (no Newton inverse):
//!
//! * distortion: `Rd = Ru·(1 + k1·Ru²)` (poly3, rescaled), `Ru·(1 + k1·Ru² + k2·Ru⁴)` (poly5),
//!   `Ru·(a·Ru³ + b·Ru² + c·Ru + 1)` (ptlens, rescaled) — corrected radius → source radius;
//! * TCA, applied after distortion around the same centre: red/blue scaled by `kr`/`kb`
//!   (linear), or by `b·r² + c·r + v` (poly3);
//! * vignetting (pa): the source is divided by `1 + k1·r² + k2·r⁴ + k3·r⁶` at its own position.
//!
//! Coordinates: lensfun measures pixels from pixel centres (`x_lf = x − 0.5` in this pipeline's
//! continuous coordinates) and normalises by `hypot(36, 24) / crop / hypot(w, h) / focal`, so a
//! correction is the same at any resolution.

use lightcraft_geom::Real;

/// Distortion model, rescaled to the image (lensfun's `rescale_distortion`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Distortion {
    #[default]
    None,
    Poly3 {
        k1: f64,
    },
    Poly5 {
        k1: f64,
        k2: f64,
    },
    Ptlens {
        a: f64,
        b: f64,
        c: f64,
    },
}

impl Distortion {
    /// Includes calibrated models whose coefficients leave every position unchanged.
    pub fn is_identity(&self) -> bool {
        match *self {
            Self::None => true,
            Self::Poly3 { k1 } => k1 == 0.0,
            Self::Poly5 { k1, k2 } => k1 == 0.0 && k2 == 0.0,
            Self::Ptlens { a, b, c } => a == 0.0 && b == 0.0 && c == 0.0,
        }
    }
}

/// Transverse chromatic aberration model, rescaled to the image (lensfun's `rescale_tca`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Tca {
    #[default]
    None,
    Linear {
        kr: f64,
        kb: f64,
    },
    /// `[v, c, b]` per channel: scale `b·r² + c·r + v`.
    Poly3 {
        red: [f64; 3],
        blue: [f64; 3],
    },
}

impl Tca {
    pub fn is_identity(&self) -> bool {
        match *self {
            Self::None => true,
            Self::Linear { kr, kb } => kr == 1.0 && kb == 1.0,
            Self::Poly3 { red, blue } => red == [1.0, 0.0, 0.0] && blue == [1.0, 0.0, 0.0],
        }
    }
}

/// A lens correction for one shot, independent of the image's resolution.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LensCorrection {
    /// `hypot(36, 24) / crop / real focal`: lensfun units per image diagonal.
    pub diag_norm: f64,
    /// Optical centre offset as lensfun stores it: fractions of half the short side, in the
    /// oriented image (x right, y down).
    pub center: [f64; 2],
    pub distortion: Distortion,
    pub tca: Tca,
    /// Vignetting (pa model) `k1, k2, k3`, rescaled (lensfun's `rescale_vignetting`).
    pub vignetting: Option<[f64; 3]>,
}

impl LensCorrection {
    /// Whether the correction moves pixels (distortion or TCA).
    pub fn moves_pixels(&self) -> bool {
        !self.distortion.is_identity() || !self.tca.is_identity()
    }

    /// Whether the colour planes land on different source positions.
    pub fn per_channel(&self) -> bool {
        !self.tca.is_identity()
    }

    pub fn is_identity(&self) -> bool {
        !self.moves_pixels() && self.vignetting.is_none_or(|k| k == [0.0; 3])
    }

    /// The correction laid on an image of `w × h` pixels.
    pub fn on(&self, w: f64, h: f64) -> LensMap {
        let ns = self.diag_norm / w.hypot(h).max(1e-9);
        let (wm, hm) = ((w - 1.0).max(1.0), (h - 1.0).max(1.0));
        let half_short = wm.min(hm) / 2.0;
        // lensfun pixel coordinates of the centre, then this pipeline's (+0.5)
        let cx = wm / 2.0 + half_short * self.center[0] + 0.5;
        let cy = hm / 2.0 + half_short * self.center[1] + 0.5;
        LensMap { c: *self, ns, cx, cy }
    }
}

/// A [`LensCorrection`] on an image of known size ([`LensCorrection::on`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensMap {
    pub c: LensCorrection,
    /// Normalised units per pixel.
    pub ns: f64,
    /// Optical centre (pixels, continuous coordinates).
    pub cx: f64,
    pub cy: f64,
}

impl LensMap {
    /// Source position (pixels) of corrected position `(x, y)` for colour plane `ch`
    /// (0 = R, 1 = G, 2 = B), with the distortion scaled by `dist` and the TCA by `tca`
    /// (1 = as calibrated, 0 = none).
    pub fn to_source(&self, x: f64, y: f64, ch: usize, dist: f64, tca: f64) -> (f64, f64) {
        self.to_source_real(x, y, ch, dist, tca)
    }

    /// [`Self::to_source`] for any [`Real`] (interval bounds of a block of pixels).
    pub fn to_source_real<T: Real>(&self, x: T, y: T, ch: usize, dist: f64, tca: f64) -> (T, T) {
        let (u, v) = ((x - self.cx) * self.ns, (y - self.cy) * self.ns);
        let ru2 = u * u + v * v;
        let f = match self.c.distortion {
            Distortion::None => None,
            Distortion::Poly3 { k1 } => Some(ru2 * (k1 * dist) + 1.0),
            Distortion::Poly5 { k1, k2 } => Some(ru2 * (k1 * dist) + ru2 * ru2 * (k2 * dist) + 1.0),
            Distortion::Ptlens { a, b, c } => {
                let r = ru2.sqrt_nonneg();
                Some(ru2 * r * (a * dist) + ru2 * (b * dist) + r * (c * dist) + 1.0)
            }
        };
        let (du, dv) = match f {
            Some(f) => (u * f, v * f),
            None => (u, v),
        };
        let (du, dv) = match (self.c.tca, ch) {
            (_, 1) | (Tca::None, _) => (du, dv),
            (Tca::Linear { kr, kb }, ch) => {
                let s = 1.0 + ((if ch == 0 { kr } else { kb }) - 1.0) * tca;
                (du * s, dv * s)
            }
            (Tca::Poly3 { red, blue }, ch) => {
                let [v0, c, b] = if ch == 0 { red } else { blue };
                let r2 = du * du + dv * dv;
                let s = if c == 0.0 { r2 * b + v0 } else { r2 * b + r2.sqrt_nonneg() * c + v0 };
                let s = (s - 1.0) * tca + 1.0;
                (du * s, dv * s)
            }
        };
        (du / self.ns + self.cx, dv / self.ns + self.cy)
    }

    /// Vignetting correction gain (≥ 1 for a lens that darkens its corners) at source position
    /// `(x, y)`, the correction scaled by `amount` in stops (1 = as calibrated).
    pub fn gain(&self, x: f64, y: f64, amount: f64) -> f64 {
        let Some([k1, k2, k3]) = self.c.vignetting else { return 1.0 };
        let (u, v) = ((x - self.cx) * self.ns, (y - self.cy) * self.ns);
        let r2 = u * u + v * v;
        let g = 1.0 + k1 * r2 + k2 * r2 * r2 + k3 * r2 * r2 * r2;
        if g <= 1e-6 {
            return 1.0;
        }
        (1.0 / g).powf(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn barrel() -> LensCorrection {
        LensCorrection {
            diag_norm: 36f64.hypot(24.0) / 1.0 / 24.0,
            center: [0.0, 0.0],
            distortion: Distortion::Poly3 { k1: -0.03 },
            tca: Tca::Linear { kr: 1.0004, kb: 0.9996 },
            vignetting: Some([-0.4, 0.1, -0.02]),
        }
    }

    #[test]
    fn identity_and_centre() {
        let id = LensCorrection { diag_norm: 2.0, ..Default::default() }.on(600.0, 400.0);
        assert!(LensCorrection::default().is_identity());
        for (x, y) in [(0.0, 0.0), (123.4, 87.0), (600.0, 400.0)] {
            for ch in 0..3 {
                let (sx, sy) = id.to_source(x, y, ch, 1.0, 1.0);
                assert!((sx - x).abs() < 1e-9 && (sy - y).abs() < 1e-9);
            }
            assert_eq!(id.gain(x, y, 1.0), 1.0);
        }
        let m = barrel().on(600.0, 400.0);
        assert_eq!((m.cx, m.cy), (300.0, 200.0));
        let (x, y) = m.to_source(300.0, 200.0, 0, 1.0, 1.0);
        assert!((x - 300.0).abs() < 1e-12 && (y - 200.0).abs() < 1e-12, "the centre stays put");
    }

    #[test]
    fn corrections_move_and_brighten_corners() {
        let m = barrel().on(600.0, 400.0);
        // barrel distortion (k1 < 0): the corrected corner comes from further in
        let (sx, sy) = m.to_source(0.0, 0.0, 1, 1.0, 1.0);
        assert!(sx > 0.0 && sy > 0.0, "{sx} {sy}");
        // half the distortion lands half way (to first order)
        let (hx, _) = m.to_source(0.0, 0.0, 1, 0.5, 1.0);
        assert!((hx - sx / 2.0).abs() < 1e-9);
        // TCA: red slightly outward, blue inward of green
        let (rx, _) = m.to_source(0.0, 0.0, 0, 1.0, 1.0);
        let (bx, _) = m.to_source(0.0, 0.0, 2, 1.0, 1.0);
        assert!(rx < sx && bx > sx, "{rx} {sx} {bx}");
        assert_eq!(m.to_source(0.0, 0.0, 0, 1.0, 0.0), m.to_source(0.0, 0.0, 1, 1.0, 1.0));
        // vignetting: corners brightened, centre unchanged, amount in stops
        assert_eq!(m.gain(300.0, 200.0, 1.0), 1.0);
        let g = m.gain(0.0, 0.0, 1.0);
        assert!(g > 1.05, "{g}");
        assert!((m.gain(0.0, 0.0, 0.5) - g.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn independent_of_resolution() {
        let c = barrel();
        let (a, b) = (c.on(6000.0, 4000.0), c.on(1500.0, 1000.0));
        // the same relative point maps to the same relative point (pixel-centre offsets aside)
        let (ax, ay) = a.to_source(600.0, 400.0, 2, 1.0, 1.0);
        let (bx, by) = b.to_source(150.0, 100.0, 2, 1.0, 1.0);
        assert!((ax / 6000.0 - bx / 1500.0).abs() < 2e-4 && (ay / 4000.0 - by / 1000.0).abs() < 2e-4);
        assert!((a.gain(600.0, 400.0, 1.0) - b.gain(150.0, 100.0, 1.0)).abs() < 1e-3);
    }

    #[test]
    fn every_model_runs() {
        for d in [Distortion::Poly5 { k1: -0.02, k2: 0.004 }, Distortion::Ptlens { a: 0.01, b: -0.03, c: 0.002 }] {
            for t in [Tca::Poly3 { red: [1.0003, 0.0, 0.0001], blue: [0.9997, 0.0001, -0.0001] }, Tca::None] {
                let m = LensCorrection { distortion: d, tca: t, ..barrel() }.on(300.0, 200.0);
                let p = m.to_source(10.0, 20.0, 0, 1.0, 1.0);
                assert!(p.0.is_finite() && p.1.is_finite() && (p.0 - 10.0).abs() < 20.0);
            }
        }
    }
}
