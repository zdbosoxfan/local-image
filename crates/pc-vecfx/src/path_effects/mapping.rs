//! Bounded nonlinear cubic interpolation (original Local Image adapter).
//! Mapping formulas follow the pinned VC distort.rs. Subdivision and error guards
//! preserve explicit closedness, PSD operations and source knots; no silent fallback.
use super::*;
const MAX_WORK: usize = 1_000_000;
fn fit(c: CubicBez, map: &impl Fn(Point) -> Point, tol: f64, depth: usize, work: &mut usize, out: &mut Vec<CubicBez>) -> Result<()> {
    *work += 40;
    if *work > MAX_WORK || depth > 20 || out.len() >= MAX_NODES {
        return Err(Error::Budget);
    }
    let p0 = map(c.p0);
    let p3 = map(c.p3);
    let q1 = map(c.eval(1. / 3.));
    let q2 = map(c.eval(2. / 3.));
    let v1 = q1.to_vec2() * 27. - p0.to_vec2() * 8. - p3.to_vec2();
    let v2 = q2.to_vec2() * 27. - p0.to_vec2() - p3.to_vec2() * 8.;
    let fitted = CubicBez::new(p0, ((v1 * 2. - v2) / 18.).to_point(), ((v2 * 2. - v1) / 18.).to_point(), p3);
    if [fitted.p0, fitted.p1, fitted.p2, fitted.p3]
        .iter()
        .any(|p| !number(p.x, -MAX_COORDINATE, MAX_COORDINATE) || !number(p.y, -MAX_COORDINATE, MAX_COORDINATE))
    {
        return Err(Error::Invalid);
    }
    let mut error: f64 = 0.;
    for i in 1..32 {
        let t = i as f64 / 32.;
        let distance = map(c.eval(t)).distance(fitted.eval(t));
        if !distance.is_finite() {
            return Err(Error::Invalid);
        }
        error = error.max(distance);
    }
    if error <= tol * 0.5 {
        out.push(fitted);
        return Ok(());
    }
    let (a, b) = c.subdivide();
    fit(a, map, tol, depth + 1, work, out)?;
    fit(b, map, tol, depth + 1, work, out)
}
pub(super) fn map(path: &Path, tol: f64, f: impl Fn(Point) -> Point) -> Result<Path> {
    let mut out = path.clone();
    let mut total = 0;
    let mut work = 0;
    for sp in &mut out.subpaths {
        let src = sp.knots.clone();
        let n = src.len();
        let Some(first) = src.first() else { continue };
        let point = doc_point(f(Point::new(first.anchor.x, first.anchor.y)));
        let mut knots = vec![Knot { anchor: point, in_ctrl: point, out_ctrl: point, smooth: first.smooth }];
        let segs = if sp.closed { n } else { n.saturating_sub(1) };
        for i in 0..segs {
            let mut curves = Vec::new();
            fit(cubic(&src[i], &src[(i + 1) % n]), &f, tol, 0, &mut work, &mut curves)?;
            let len = curves.len();
            for (j, c) in curves.into_iter().enumerate() {
                if let Some(k) = knots.last_mut() {
                    k.out_ctrl = doc_point(c.p1);
                }
                if sp.closed && i + 1 == segs && j + 1 == len {
                    knots[0].in_ctrl = doc_point(c.p2);
                } else {
                    let p = doc_point(c.p3);
                    knots.push(Knot { anchor: p, in_ctrl: doc_point(c.p2), out_ctrl: p, smooth: if j + 1 == len { src[(i + 1) % n].smooth } else { true } });
                }
                if total + knots.len() > MAX_NODES {
                    return Err(Error::Budget);
                }
            }
        }
        total += knots.len();
        sp.knots = knots;
    }
    validate_path(&out)?;
    Ok(out)
}
