//! darktable basecurve.c maker/camera presets and curve_tools.c monotone Hermite.
//! GPL-3.0-or-later, 733bd69f32cac7ff5e41025115942772add1f088.
//! Upstream polynomial is exact; above 90% display luminance we join a C1 exponential
//! shoulder instead of upstream's exponential extrapolation followed by clipping.
use crate::{base_curve_data::PRESETS, tone::CameraTone};

/// SQL-style preset patterns (% and _) used by darktable, ASCII case-insensitive.
fn matches(pattern: &str, value: &str) -> bool {
    let (p, v) = (pattern.to_ascii_uppercase().into_bytes(), value.to_ascii_uppercase().into_bytes());
    let mut row = vec![false; v.len() + 1];
    row[0] = true;
    for c in p {
        let mut next = vec![false; v.len() + 1];
        if c == b'%' {
            next[0] = row[0];
        }
        for j in 1..=v.len() {
            next[j] = if c == b'%' { row[j] || next[j - 1] } else { row[j - 1] && (c == b'_' || c == v[j - 1]) };
        }
        row = next;
    }
    row[v.len()]
}

pub fn camera(make: &str, model: &str) -> Option<CameraTone> {
    let find = |specific| {
        PRESETS.iter().position(|(m, c, _)| {
            !m.is_empty()
                && (c.is_empty() != specific)
                && make.to_ascii_uppercase().starts_with(&m.to_ascii_uppercase())
                && (c.is_empty() || matches(c, model))
        })
    };
    CameraTone::from_preset(
        find(true).or_else(|| find(false)).unwrap_or_else(|| PRESETS.iter().position(|(m, c, _)| m.is_empty() && c.is_empty()).unwrap_or(0)),
    )
}

pub fn points(index: usize) -> Option<&'static [[f32; 2]]> {
    PRESETS.get(index).map(|p| p.2)
}

/// Upstream monotone_hermite_set + catmull_rom_val, with their f32 operation order.
pub fn hermite(points: &[[f32; 2]], x: f32) -> f32 {
    let n = points.len();
    if n < 2 {
        return x;
    }
    let mut delta = vec![0.0; n];
    let mut m = vec![0.0; n + 1];
    for i in 0..n - 1 {
        delta[i] = (points[i + 1][1] - points[i][1]) / (points[i + 1][0] - points[i][0]);
    }
    delta[n - 1] = delta[n - 2];
    m[0] = delta[0];
    m[n - 1] = delta[n - 1];
    for i in 1..n - 1 {
        m[i] = (delta[i - 1] + delta[i]) * 0.5;
    }
    for i in 0..n {
        if delta[i].abs() < 2.0 * f32::MIN_POSITIVE {
            m[i] = 0.0;
            m[i + 1] = 0.0;
        } else {
            let a = m[i] / delta[i];
            let b = m[i + 1] / delta[i];
            let tau = a * a + b * b;
            if tau > 9.0 {
                m[i] = 3.0 * a * delta[i] / tau.sqrt();
                m[i + 1] = 3.0 * b * delta[i] / tau.sqrt();
            }
        }
    }
    let i = (0..n - 2).find(|i| x < points[i + 1][0]).unwrap_or(n - 2);
    let h = points[i + 1][0] - points[i][0];
    let dx = (x - points[i][0]) / h;
    let dx2 = dx * dx;
    let dx3 = dx * dx2;
    let h00 = 2.0 * dx3 - 3.0 * dx2 + 1.0;
    let h10 = dx3 - 2.0 * dx2 + dx;
    let h01 = -2.0 * dx3 + 3.0 * dx2;
    let h11 = dx3 - dx2;
    h00 * points[i][1] + h10 * h * m[i] + h01 * points[i + 1][1] + h11 * h * m[i + 1]
}

pub fn eval(index: usize, x: f32) -> f32 {
    let Some(p) = points(index) else {
        return crate::tone2::adobe(x);
    };
    // Precompute the shoulder join once per preset, not per LUT entry.
    static JOINS: std::sync::OnceLock<Vec<(f32, f32)>> = std::sync::OnceLock::new();
    let joins = JOINS.get_or_init(|| {
        PRESETS
            .iter()
            .map(|(_, _, p)| {
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..32 {
                    let mid = (lo + hi) * 0.5;
                    if hermite(p, mid) < 0.9 {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let at = (lo + hi) * 0.5;
                let slope = ((hermite(p, at + 1e-4) - hermite(p, at - 1e-4)) / 2e-4).max(1e-6);
                (at, slope)
            })
            .collect()
    });
    let (at, slope) = joins[index];
    if x <= at { hermite(p, x).max(0.0) } else { 1.0 - 0.1 * (-(x - at) * slope / 0.1).exp() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_presets_match_extracted_darktable_hermite() {
        let mut worst = 0.0f32;
        for row in include_str!("../tests/fixtures/basecurve.csv").lines() {
            let v: Vec<&str> = row.split(',').collect();
            let index: usize = v[0].parse().unwrap();
            let x: f32 = v[1].parse().unwrap();
            let expected: f32 = v[2].parse().unwrap();
            worst = worst.max((hermite(points(index).unwrap(), x) - expected).abs());
        }
        assert!(worst < 8e-7, "darktable C spline absolute error: {worst}");
    }
    #[test]
    fn selection_and_shoulders() {
        assert!(matches("%D____%", "NIKON D7500"));
        assert!(!matches("%D____%", "NIKON D750"));
        assert!(!matches("EOS 5D Mark%", "EOS 6D"));
        assert_ne!(camera("NIKON CORPORATION", "NIKON D750"), camera("NIKON CORPORATION", "unknown"));
        for i in 0..PRESETS.len() {
            let a = eval(i, 1.0);
            let b = eval(i, 2.0);
            assert!(a <= b && b <= 1.0 && a > 0.9, "preset {i}: {a}, {b}");
        }
    }
}
