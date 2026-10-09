//! Independent comparison metrics (CIELab D65, CIEDE2000, lightness order, halos).
//! CIEDE2000 follows Sharma/Wu/Dalal (2005); formula tests use their published pairs.
use lightcraft_raster::{Plane, Rgb32f};
use std::f64::consts::PI;
pub fn lab(c: [f32; 3]) -> [f64; 3] {
    let xyz = lightcraft_color::REC2020.to_xyz().apply(c.map(f64::from));
    let f = |t: f64| if t > 216. / 24389. { t.cbrt() } else { (24389. / 27. * t + 16.) / 116. };
    let x = f(xyz[0] / 0.95047);
    let y = f(xyz[1]);
    let z = f(xyz[2] / 1.08883);
    [116. * y - 16., 500. * (x - y), 200. * (y - z)]
}
pub fn delta_e(a: [f64; 3], b: [f64; 3]) -> f64 {
    let [l1, a1, b1] = a;
    let [l2, a2, b2] = b;
    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();
    let c = (c1 + c2) / 2.;
    let g = 0.5 * (1. - (c.powi(7) / (c.powi(7) + 25f64.powi(7))).sqrt());
    let ap1 = (1. + g) * a1;
    let ap2 = (1. + g) * a2;
    let cp1 = (ap1 * ap1 + b1 * b1).sqrt();
    let cp2 = (ap2 * ap2 + b2 * b2).sqrt();
    let hp1 = if cp1 == 0. { 0. } else { b1.atan2(ap1).rem_euclid(2. * PI) };
    let hp2 = if cp2 == 0. { 0. } else { b2.atan2(ap2).rem_euclid(2. * PI) };
    let dl = l2 - l1;
    let dc = cp2 - cp1;
    let mut dh = hp2 - hp1;
    if cp1 * cp2 == 0. {
        dh = 0.;
    } else if dh > PI {
        dh -= 2. * PI;
    } else if dh < -PI {
        dh += 2. * PI;
    }
    let dh = 2. * (cp1 * cp2).sqrt() * (dh / 2.).sin();
    let lm = (l1 + l2) / 2.;
    let cm = (cp1 + cp2) / 2.;
    let hm = if cp1 * cp2 == 0. {
        hp1 + hp2
    } else if (hp1 - hp2).abs() <= PI {
        (hp1 + hp2) / 2.
    } else if hp1 + hp2 < 2. * PI {
        (hp1 + hp2 + 2. * PI) / 2.
    } else {
        (hp1 + hp2 - 2. * PI) / 2.
    };
    let t = 1. - 0.17 * (hm - 30f64.to_radians()).cos() + 0.24 * (2. * hm).cos() + 0.32 * (3. * hm + 6f64.to_radians()).cos()
        - 0.20 * (4. * hm - 63f64.to_radians()).cos();
    let sl = 1. + 0.015 * (lm - 50.).powi(2) / (20. + (lm - 50.).powi(2)).sqrt();
    let sc = 1. + 0.045 * cm;
    let sh = 1. + 0.015 * cm * t;
    let dt = 30f64.to_radians() * (-((hm.to_degrees() - 275.) / 25.).powi(2)).exp();
    let rt = -2. * (cm.powi(7) / (cm.powi(7) + 25f64.powi(7))).sqrt() * (2. * dt).sin();
    ((dl / sl).powi(2) + (dc / sc).powi(2) + (dh / sh).powi(2) + rt * (dc / sc) * (dh / sh)).max(0.).sqrt()
}
/// CIELab hue difference after normalizing both scene luminances to 18% (isolates colour
/// drift from the intended tone movement). Achromatic pairs have no measurable hue.
pub fn hue_shift(a: [f32; 3], b: [f32; 3]) -> f64 {
    let norm = |c: [f32; 3]| {
        let y = lightcraft_color::luminance_2020(c).max(1e-7);
        lab(c.map(|v| v * 0.18 / y))
    };
    let a = norm(a);
    let b = norm(b);
    if a[1] * a[1] + a[2] * a[2] < 1e-4 || b[1] * b[1] + b[2] * b[2] < 1e-4 {
        return 0.;
    }
    ((b[2].atan2(b[1]) - a[2].atan2(a[1]) + PI).rem_euclid(2. * PI) - PI).abs().to_degrees()
}
pub fn p95(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.;
    }
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * 0.95).ceil() as usize]
}
/// Lightness-order error: deterministic spatial sample (at most 256), all sample pairs.
pub fn lightness_order(a: &Rgb32f, b: &Rgb32f) -> f64 {
    let n = a.len().min(256);
    let mut wrong = 0usize;
    let mut pairs = 0usize;
    for i in 0..n {
        for j in i + 1..n {
            let (i, j) = (i * a.len() / n, j * a.len() / n);
            let x = crate::primary::log_light(a.data[i]) - crate::primary::log_light(a.data[j]);
            let y = crate::primary::log_light(b.data[i]) - crate::primary::log_light(b.data[j]);
            if x.abs() > 1e-5 {
                pairs += 1;
                wrong += usize::from(x * y < 0.);
            }
        }
    }
    wrong as f64 / pairs.max(1) as f64
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Halo {
    pub overshoot: f64,
    pub undershoot: f64,
    pub width: f64,
    pub energy: f64,
}
/// Error against ideal two plateaus on a neutral step: far-field changes are intentional.
/// Width measures connected 10%-peak support around each side's peak; energy is |EV|·px.
pub fn halo(before: &Plane, after: &Plane, y: usize, edge: usize) -> Halo {
    let w = before.width;
    let radius = (w / 4).max(1);
    let gain: Vec<_> = (0..w).map(|x| (after.get(x, y) - before.get(x, y)) as f64).collect();
    let left = gain[..(edge / 2).max(1)].iter().sum::<f64>() / (edge / 2).max(1) as f64;
    let start = (edge + (w - edge) / 2).min(w - 1);
    let right = gain[start..].iter().sum::<f64>() / (w - start) as f64;
    let errors: Vec<_> = gain.iter().enumerate().map(|(x, &g)| g - if x < edge { left } else { right }).collect();
    let slice = &errors[edge.saturating_sub(radius)..(edge + radius).min(w)];
    let over = slice.iter().copied().fold(0., f64::max);
    let under = -slice.iter().copied().fold(0., f64::min);
    let peak = over.max(under);
    let width = if peak < 1e-6 {
        0
    } else {
        let split = edge.min(errors.len());
        let support = |part: &[f64]| {
            let Some((p, _)) = part.iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())) else {
                return 0;
            };
            if part[p].abs() < 0.1 * peak {
                return 0;
            }
            let mut a = p;
            let mut b = p + 1;
            while a > 0 && part[a - 1].abs() >= 0.1 * peak {
                a -= 1;
            }
            while b < part.len() && part[b].abs() >= 0.1 * peak {
                b += 1;
            }
            b - a
        };
        support(&errors[..split]) + support(&errors[split..])
    };
    Halo { overshoot: over, undershoot: under, width: width as f64, energy: slice.iter().map(|v| v.abs()).sum() }
}
#[cfg(test)]
mod tests {
    #[test]
    fn sharma_published_pairs() {
        let cases = [
            ([50., 2.6772, -79.7751], [50., 0., -82.7485], 2.0425),
            ([50., 3.1571, -77.2803], [50., 0., -82.7485], 2.8615),
            ([50., 2.8361, -74.0200], [50., 0., -82.7485], 3.4412),
            ([50., 0., 0.], [50., -1., 2.], 2.3669),
        ];
        for (a, b, expected) in cases {
            assert!((super::delta_e(a, b) - expected).abs() < 0.00005);
            assert!((super::delta_e(a, b) - super::delta_e(b, a)).abs() < 1e-12);
        }
    }
}
