//! The global pen pressure curve (Preferences › Tools › Pen pressure curve), applied to stylus
//! pressure before it reaches brush dynamics, as Krita's global tablet pressure curve does.
//!
//! The curve is a list of points in `0..=1` (input pressure → output pressure) joined by a
//! monotone cubic (F. N. Fritsch, R. E. Carlson, *Monotone Piecewise Cubic Interpolation*, SIAM
//! J. Numer. Anal. 1980), so it never overshoots between points. The linear curve is the exact
//! identity.

/// The identity curve.
pub const LINEAR: &[[f32; 2]] = &[[0.0, 0.0], [1.0, 1.0]];
/// A light touch reaches more of the range.
pub const SOFT: &[[f32; 2]] = &[[0.0, 0.0], [0.3, 0.55], [1.0, 1.0]];
/// It takes more force to reach the same pressure.
pub const FIRM: &[[f32; 2]] = &[[0.0, 0.0], [0.6, 0.35], [1.0, 1.0]];

/// The presets, by id.
pub const PRESETS: [(&str, &[[f32; 2]]); 3] = [("soft", SOFT), ("linear", LINEAR), ("firm", FIRM)];

/// Most points a curve keeps.
pub const MAX_POINTS: usize = 16;

/// The preset `points` equal, if any.
pub fn preset_of(points: &[[f32; 2]]) -> Option<&'static str> {
    let p = sanitize(points);
    PRESETS
        .iter()
        .find(|(_, q)| p.len() == q.len() && p.iter().zip(q.iter()).all(|(a, b)| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4))
        .map(|(id, _)| *id)
}

/// A usable curve from any input: finite points clamped to `0..=1`, sorted by input, inputs at
/// least 0.01 apart, at most [`MAX_POINTS`], with points at input 0 and 1. Fewer than two points
/// give [`LINEAR`].
pub fn sanitize(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut p: Vec<[f32; 2]> = points.iter().filter(|q| q[0].is_finite() && q[1].is_finite()).map(|q| [q[0].clamp(0.0, 1.0), q[1].clamp(0.0, 1.0)]).collect();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]));
    p.dedup_by(|b, a| (b[0] - a[0]).abs() < 0.01);
    p.truncate(MAX_POINTS);
    if p.is_empty() {
        return LINEAR.to_vec();
    }
    if p[0][0] > 0.0 {
        p.insert(0, [0.0, p[0][1]]);
    }
    if let Some(&last) = p.last()
        && last[0] < 1.0
    {
        p.push([1.0, last[1]]);
    }
    if p.len() < 2 { LINEAR.to_vec() } else { p }
}

const LUT: usize = 1024;

/// An evaluated curve (a lookup table).
#[derive(Clone, Debug, PartialEq)]
pub struct PressureCurve {
    points: Vec<[f32; 2]>,
    lut: Vec<f32>,
    identity: bool,
}

impl Default for PressureCurve {
    fn default() -> Self {
        Self::new(LINEAR)
    }
}

impl PressureCurve {
    pub fn new(points: &[[f32; 2]]) -> Self {
        let pts = sanitize(points);
        let identity = pts.len() == 2 && pts[0] == [0.0, 0.0] && pts[1] == [1.0, 1.0];
        let lut = if identity { Vec::new() } else { (0..=LUT).map(|i| eval(&pts, i as f32 / LUT as f32)).collect() };
        PressureCurve { points: pts, lut, identity }
    }

    /// The (sanitized) points.
    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }

    pub fn is_identity(&self) -> bool {
        self.identity
    }

    /// Output pressure for input `p` (clamped to `0..=1`; NaN reads as full pressure).
    pub fn map(&self, p: f32) -> f32 {
        let p = if p.is_nan() { 1.0 } else { p.clamp(0.0, 1.0) };
        if self.identity {
            return p;
        }
        let x = p * LUT as f32;
        let i = (x.floor() as usize).min(LUT - 1);
        let f = x - i as f32;
        (self.lut[i] + (self.lut[i + 1] - self.lut[i]) * f).clamp(0.0, 1.0)
    }
}

/// Fritsch–Carlson monotone cubic Hermite interpolation of sorted points at `x`.
pub fn eval(pts: &[[f32; 2]], x: f32) -> f32 {
    let n = pts.len();
    if n == 0 {
        return x;
    }
    if n == 1 || x <= pts[0][0] {
        return pts[0][1];
    }
    if x >= pts[n - 1][0] {
        return pts[n - 1][1];
    }
    // Secant slopes and Fritsch–Carlson tangents.
    let d: Vec<f32> = (0..n - 1).map(|k| (pts[k + 1][1] - pts[k][1]) / (pts[k + 1][0] - pts[k][0]).max(1e-6)).collect();
    let mut m = vec![0.0f32; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for k in 1..n - 1 {
        m[k] = if d[k - 1] * d[k] <= 0.0 { 0.0 } else { 0.5 * (d[k - 1] + d[k]) };
    }
    for k in 0..n - 1 {
        if d[k] == 0.0 {
            m[k] = 0.0;
            m[k + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[k] / d[k], m[k + 1] / d[k]);
        let s = a * a + b * b;
        if s > 9.0 {
            let t = 3.0 / s.sqrt();
            m[k] = t * a * d[k];
            m[k + 1] = t * b * d[k];
        }
    }
    let k = pts.windows(2).position(|w| x < w[1][0]).unwrap_or(n - 2);
    let (x0, x1) = (pts[k][0], pts[k + 1][0]);
    let h = (x1 - x0).max(1e-6);
    let t = (x - x0) / h;
    let (t2, t3) = (t * t, t * t * t);
    let (h00, h10, h01, h11) = (2.0 * t3 - 3.0 * t2 + 1.0, t3 - 2.0 * t2 + t, -2.0 * t3 + 3.0 * t2, t3 - t2);
    (h00 * pts[k][1] + h10 * h * m[k] + h01 * pts[k + 1][1] + h11 * h * m[k + 1]).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_is_the_exact_identity() {
        let c = PressureCurve::new(LINEAR);
        assert!(c.is_identity());
        for i in 0..=100 {
            let p = i as f32 / 100.0;
            assert_eq!(c.map(p).to_bits(), p.to_bits());
        }
        assert_eq!(c.map(f32::NAN), 1.0);
        assert_eq!(c.map(7.0), 1.0);
    }

    #[test]
    fn presets_bend_the_right_way_and_stay_monotonic() {
        let (soft, firm) = (PressureCurve::new(SOFT), PressureCurve::new(FIRM));
        assert!(soft.map(0.3) > 0.5 && firm.map(0.6) < 0.4);
        for c in [&soft, &firm] {
            let mut last = -1.0;
            for i in 0..=200 {
                let v = c.map(i as f32 / 200.0);
                assert!(v >= last - 1e-6, "not monotonic at {i}");
                last = v;
            }
            assert!(c.map(0.0) < 1e-6 && (c.map(1.0) - 1.0).abs() < 1e-6);
        }
        assert_eq!(preset_of(SOFT), Some("soft"));
        assert_eq!(preset_of(&[[0.0, 0.0], [1.0, 1.0]]), Some("linear"));
        assert_eq!(preset_of(&[[0.0, 0.1], [1.0, 1.0]]), None);
    }

    #[test]
    fn hostile_points_are_sanitized() {
        assert_eq!(sanitize(&[]), LINEAR.to_vec());
        assert_eq!(sanitize(&[[f32::NAN, 0.5]]), LINEAR.to_vec());
        let p = sanitize(&[[0.8, 2.0], [0.2, -1.0], [0.205, 0.5]]);
        assert_eq!(p, vec![[0.0, 0.0], [0.2, 0.0], [0.8, 1.0], [1.0, 1.0]]);
        let many: Vec<[f32; 2]> = (0..40).map(|i| [i as f32 / 39.0, 0.5]).collect();
        assert!(sanitize(&many).len() <= MAX_POINTS + 2);
        // Through its points, never overshooting.
        let c = PressureCurve::new(&[[0.0, 0.0], [0.5, 0.9], [0.55, 0.9], [1.0, 1.0]]);
        assert!((c.map(0.5) - 0.9).abs() < 1e-3);
        assert!((0..=100).all(|i| c.map(i as f32 / 100.0) <= 1.0));
    }
}
