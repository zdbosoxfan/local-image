//! Bounded kurbo fit_to_cubic driver. Original Local Image adapter.
//! kurbo 0.13.1's fitting accuracy is numerical, not a rigorous interval guarantee;
//! we additionally check both directions against dense samples before accepting.
use super::*;
use kurbo::{CurveFitSample, ParamCurveFit, ParamCurveNearest, Vec2, fit_to_cubic};
use photocraft_doc::Knot;
use photocraft_geom::Point as DocPoint;
use std::{cell::Cell, ops::Range};
const MAX_WORK: usize = 1_000_000;
struct Mapped<'a> {
    source: CubicBez,
    map: &'a Map,
    failed: Cell<bool>,
    work: Cell<usize>,
}
impl Mapped<'_> {
    fn eval(&self, t: f64) -> Point {
        self.work.set(self.work.get().saturating_add(1));
        if self.work.get() > MAX_WORK {
            self.failed.set(true);
            return Point::ZERO;
        }
        match self.map.point(self.source.eval(t)) {
            Ok(p) => p,
            Err(_) => {
                self.failed.set(true);
                Point::ZERO
            }
        }
    }
}
impl ParamCurveFit for Mapped<'_> {
    fn sample_pt_tangent(&self, t: f64, _sign: f64) -> CurveFitSample {
        let (p, tangent) = self.sample_pt_deriv(t);
        CurveFitSample { p, tangent }
    }
    fn sample_pt_deriv(&self, t: f64) -> (Point, Vec2) {
        let lo = (t - 1e-6).max(0.0);
        let hi = (t + 1e-6).min(1.0);
        (self.eval(t), (self.eval(hi) - self.eval(lo)) / (hi - lo))
    }
    fn break_cusp(&self, _range: Range<f64>) -> Option<f64> {
        None
    }
}
fn distance_poly(p: Point, points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|w| {
            let d = w[1] - w[0];
            let t = if d.hypot2() > 0.0 { ((p - w[0]).dot(d) / d.hypot2()).clamp(0.0, 1.0) } else { 0.0 };
            p.distance(w[0] + d * t)
        })
        .fold(f64::INFINITY, f64::min)
}
fn fit(source: &Mapped<'_>, range: Range<f64>, tol: f64, depth: usize, budget: &mut usize, out: &mut Vec<CubicBez>) -> Result<()> {
    if source.failed.get() || source.work.get() > MAX_WORK || depth > 16 || *budget == 0 {
        return Err(Error::Budget);
    }
    if let Some((c, _)) = fit_to_cubic(source, range.clone(), tol * 0.25) {
        let samples: Vec<_> = (0..=64).map(|i| source.eval(range.start + (range.end - range.start) * i as f64 / 64.0)).collect();
        if source.failed.get() {
            return Err(Error::Coordinate);
        }
        let finite = [c.p0, c.p1, c.p2, c.p3].into_iter().all(valid_point);
        let forward = finite && samples.iter().all(|p| c.nearest(*p, tol * 0.125).distance_sq <= tol * tol * 0.25);
        let reverse = (0..=64).all(|i| distance_poly(c.eval(i as f64 / 64.0), &samples) <= tol * 0.5);
        if finite && forward && reverse {
            *budget -= 1;
            out.push(c);
            return Ok(());
        }
    }
    let mid = (range.start + range.end) * 0.5;
    fit(source, range.start..mid, tol, depth + 1, budget, out)?;
    fit(source, mid..range.end, tol, depth + 1, budget, out)
}
pub(super) fn path(path: &Path, map: &Map, tol: f64, budget: &mut usize, work: &mut usize) -> Result<Path> {
    let pt = |p: Point| DocPoint::new(p.x, p.y);
    let mut out = path.clone();
    if let Some(a) = map.affine() {
        for k in out.subpaths.iter_mut().flat_map(|s| &mut s.knots) {
            if *budget == 0 {
                return Err(Error::Budget);
            }
            *budget -= 1;
            for p in [&mut k.anchor, &mut k.in_ctrl, &mut k.out_ctrl] {
                let q = a * Point::new(p.x, p.y);
                if !valid_point(q) {
                    return Err(Error::Coordinate);
                }
                *p = pt(q);
            }
        }
        return Ok(out);
    }
    for sp in &mut out.subpaths {
        let originals = sp.knots.clone();
        let Some(first) = originals.first() else { continue };
        if *budget == 0 {
            return Err(Error::Budget);
        }
        *budget -= 1;
        let anchor = pt(map.point(Point::new(first.anchor.x, first.anchor.y))?);
        let mut knots = vec![Knot { anchor, in_ctrl: anchor, out_ctrl: anchor, smooth: first.smooth }];
        let count = if sp.closed { originals.len() } else { originals.len().saturating_sub(1) };
        for i in 0..count {
            let next = (i + 1) % originals.len();
            let mapped = Mapped { source: cubic(&originals[i], &originals[next]), map, failed: Cell::new(false), work: Cell::new(*work) };
            let mut curves = Vec::new();
            fit(&mapped, 0.0..1.0, tol, 0, budget, &mut curves)?;
            *work = mapped.work.get();
            let n = curves.len();
            for (j, c) in curves.into_iter().enumerate() {
                if let Some(last) = knots.last_mut() {
                    last.out_ctrl = pt(c.p1);
                }
                if sp.closed && i + 1 == count && j + 1 == n {
                    knots[0].in_ctrl = pt(c.p2);
                } else {
                    knots.push(Knot {
                        anchor: pt(c.p3),
                        in_ctrl: pt(c.p2),
                        out_ctrl: pt(c.p3),
                        smooth: if j + 1 == n { originals[next].smooth } else { true },
                    });
                }
            }
        }
        sp.knots = knots;
    }
    Ok(out)
}
