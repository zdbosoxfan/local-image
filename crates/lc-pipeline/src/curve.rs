//! Parametric tone curve shared by rendering and direct manipulation of its graph.

use li_color::spline::Lut1;
use lightcraft_develop::ToneCurve;

pub const PARAM_AMOUNTS: [&str; 4] = ["curve.shadows", "curve.darks", "curve.lights", "curve.highlights"];
const SAMPLES: usize = 1024;

#[derive(Clone, Debug)]
pub struct ParametricCurve {
    lut: Lut1,
    pub splits: [f64; 3],
    pub amounts: [f64; 4],
}

impl ParametricCurve {
    pub fn new(curve: &ToneCurve) -> Self {
        let mut lower = 0.0_f32;
        let raw_splits = [curve.split_shadows, curve.split_mid, curve.split_highlights];
        let splits = std::array::from_fn::<_, 3, _>(|i| {
            let split = raw_splits[i];
            let value = if split.is_nan() { [0.25_f32, 0.5, 0.75][i].max(lower) } else { (split / 100.0).clamp(f64::from(lower), 1.0) as f32 };
            lower = value;
            value
        });
        let amounts = [curve.shadows, curve.darks, curve.lights, curve.highlights]
            .map(|amount| if amount.is_nan() { 0.0 } else { amount.clamp(-100.0, 100.0) });
        let boundaries = [0.0, splits[0], splits[1], splits[2], 1.0];
        let mut lut = Lut1::from_fn(SAMPLES, |x| {
            let mut delta = 0.0;
            for (i, amount) in amounts.iter().enumerate() {
                if *amount == 0.0 {
                    continue;
                }
                let (a, b) = (boundaries[i], boundaries[i + 1]);
                let (center, half) = ((a + b) / 2.0, (b - a) * 0.75 + 0.05);
                let t = ((x - center) / half).clamp(-1.0, 1.0);
                let window = 0.5 + 0.5 * (t * std::f32::consts::PI).cos();
                delta += (*amount / 100.0) as f32 * 0.22 * window;
            }
            (x + delta * 4.0 * x * (1.0 - x)).clamp(0.0, 1.0)
        });
        for i in 1..SAMPLES {
            lut.v[i] = lut.v[i].max(lut.v[i - 1]);
        }
        Self { lut, splits: splits.map(f64::from), amounts }
    }

    pub fn to_lut(&self) -> Lut1 {
        self.lut.clone()
    }

    pub fn at(&self, x: f64) -> f64 {
        f64::from(self.lut.eval(if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) as f32 }))
    }

    pub fn region_at(&self, x: f64) -> usize {
        let x = if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) };
        self.splits.iter().position(|split| x <= *split).unwrap_or(3)
    }

    /// Local change in output per slider unit, before the monotone envelope and master curve.
    pub fn level_per_unit(&self, region: usize, x: f64) -> f64 {
        let boundaries = [0.0, self.splits[0], self.splits[1], self.splits[2], 1.0];
        let Some((&a, &b)) = boundaries.get(region).zip(boundaries.get(region.saturating_add(1))) else { return 0.0 };
        if !x.is_finite() {
            return 0.0;
        }
        let x = x.clamp(0.0, 1.0);
        let t = ((x - (a + b) / 2.0) / ((b - a) * 0.75 + 0.05)).clamp(-1.0, 1.0);
        let window = 0.5 + 0.5 * (t * std::f64::consts::PI).cos();
        0.22 / 100.0 * window * 4.0 * x * (1.0 - x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_finite_tables_match_the_previous_renderer() {
        for splits in [[25.0, 50.0, 75.0], [8.0, 37.0, 92.0], [0.0, 0.0, 100.0]] {
            for amounts in [[0.0; 4], [90.0, -80.0, -70.0, 60.0], [-100.0, 30.0, 70.0, 100.0]] {
                let c = ToneCurve {
                    split_shadows: splits[0],
                    split_mid: splits[1],
                    split_highlights: splits[2],
                    shadows: amounts[0],
                    darks: amounts[1],
                    lights: amounts[2],
                    highlights: amounts[3],
                    ..Default::default()
                };
                const N: usize = 1024;
                let (s1, s2, s3) = ((c.split_shadows / 100.0) as f32, (c.split_mid / 100.0) as f32, (c.split_highlights / 100.0) as f32);
                let regions = [(0.0, s1, c.shadows), (s1, s2, c.darks), (s2, s3, c.lights), (s3, 1.0, c.highlights)];
                let mut base = Lut1::from_fn(N, |x| {
                    let mut d = 0.0;
                    for (a, b, amt) in regions {
                        if amt == 0.0 {
                            continue;
                        }
                        let (ctr, half) = ((a + b) / 2.0, (b - a) * 0.75 + 0.05);
                        let t = ((x - ctr) / half).clamp(-1.0, 1.0);
                        let win = 0.5 + 0.5 * (t * std::f32::consts::PI).cos();
                        d += (amt / 100.0) as f32 * 0.22 * win;
                    }
                    (x + d * 4.0 * x * (1.0 - x)).clamp(0.0, 1.0)
                });
                // keep monotone
                for i in 1..N {
                    base.v[i] = base.v[i].max(base.v[i - 1]);
                }
                assert_eq!(ParametricCurve::new(&c).to_lut().v, base.v);
            }
        }
    }

    #[test]
    fn hostile_splits_and_amounts_stay_finite_and_monotone() {
        for splits in [[80.0, 20.0, 30.0], [f64::NAN, f64::INFINITY, f64::NEG_INFINITY], [50.0; 3]] {
            let c = ToneCurve {
                split_shadows: splits[0],
                split_mid: splits[1],
                split_highlights: splits[2],
                shadows: f64::NAN,
                darks: -100.0,
                lights: f64::INFINITY,
                highlights: f64::NEG_INFINITY,
                ..Default::default()
            };
            let model = ParametricCurve::new(&c);
            let table = model.to_lut();
            assert!(table.v.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
            assert!(table.v.windows(2).all(|w| w[0] <= w[1]));
            assert!(model.splits.windows(2).all(|w| w[0] <= w[1]));
            for x in [f64::NAN, f64::NEG_INFINITY, 0.0, 0.51, 1.0, f64::INFINITY] {
                assert!(model.at(x).is_finite());
                assert!(model.region_at(x) < 4);
                assert!(model.level_per_unit(usize::MAX, x).is_finite());
            }
        }
    }
}
