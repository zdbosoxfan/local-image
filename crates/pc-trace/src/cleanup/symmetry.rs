//! Mask reflection is the authorization for geometric symmetrization, never bbox similarity.
use super::{Stats, check};
use crate::{Error, Trace};
use photocraft_doc::{Path, Subpath};
use photocraft_geom::Point;

#[derive(Clone, Copy)]
pub(super) struct Axis {
    pub vertical: bool,
    pub position: f64,
}
impl Axis {
    fn reflect(self, p: Point) -> Point {
        if self.vertical { Point::new(2. * self.position - p.x, p.y) } else { Point::new(p.x, 2. * self.position - p.y) }
    }
}
pub(super) fn axes(trace: &Trace, layer: usize, pixels: &[u8], scale: u32, stats: &mut Stats, cancel: &impl Fn() -> bool) -> Result<Vec<Axis>, Error> {
    let (w, h) = ((trace.width * scale) as usize, (trace.height * scale) as usize);
    let color = trace.layers[layer].color;
    let mut mask = vec![false; w * h];
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    let mut count = 0;
    for y in 0..h {
        check(cancel)?;
        for x in 0..w {
            let p = &pixels[(y * w + x) * 4..][..4];
            if p[3] >= 128 && p[..3] == color[..3] {
                mask[y * w + x] = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
                count += 1;
            }
        }
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for vertical in [true, false] {
        let mut intersection = 0;
        for y in y0..=y1 {
            check(cancel)?;
            for x in x0..=x1 {
                let (xx, yy) = if vertical { (x0 + x1 - x, y) } else { (x, y0 + y1 - y) };
                intersection += usize::from(mask[y * w + x] && mask[yy * w + xx]);
            }
        }
        let iou = intersection as f64 / (2 * count - intersection) as f64;
        stats.symmetry_iou.push(iou);
        if iou > 0.98 {
            let position = if vertical { (x0 + x1 + 1) as f64 } else { (y0 + y1 + 1) as f64 } / (2. * f64::from(scale));
            result.push(Axis { vertical, position });
        }
    }
    Ok(result)
}

/// Pair complete rings, including disjoint mirrored components and opposite-wound counters.
/// Require a reciprocal bijection and matching handles, then average both sides simultaneously.
pub(super) fn symmetrize(path: &Path, axis: Axis, tolerance: f64) -> Option<Path> {
    let rings = &path.subpaths;
    // Bound the reciprocal-pair search independently of T1's boundary budget.
    if rings.iter().map(|s| s.knots.len()).sum::<usize>() > 1024 {
        return None;
    }
    let joined_component = rings.iter().skip(1).all(|s| s.op == photocraft_doc::PathOp::Join);

    let mut out = path.clone();
    let mut partners = Vec::new();
    for (si, s) in rings.iter().enumerate() {
        let n = s.knots.len();
        if !s.closed || n == 0 {
            return None;
        }
        let mut best = None;
        for (ti, t) in rings.iter().enumerate().filter(|(_, t)| t.closed && t.knots.len() == n && (t.op == s.op || joined_component)) {
            for offset in 0..n {
                let error = s
                    .knots
                    .iter()
                    .enumerate()
                    .map(|(i, k)| {
                        let other = &t.knots[(offset + n - i) % n];
                        distance(axis.reflect(k.anchor), other.anchor)
                            .max(distance(axis.reflect(k.out_ctrl), other.in_ctrl))
                            .max(distance(axis.reflect(k.in_ctrl), other.out_ctrl))
                    })
                    .fold(0., f64::max);
                if error <= tolerance * 2. && best.is_none_or(|(_, _, e)| error < e) {
                    best = Some((ti, offset, error));
                }
            }
        }
        let (ti, offset, _) = best?;
        partners.push((si, ti, offset));
    }
    for &(si, ti, offset) in &partners {
        if partners.get(ti).is_none_or(|&(_, back, off)| back != si || off != offset) {
            return None;
        }
        let s: &Subpath = &rings[si];
        let n = s.knots.len();
        for (i, k) in s.knots.iter().enumerate() {
            let other = rings[ti].knots[(offset + n - i) % n];
            let target = &mut out.subpaths[si].knots[i];
            target.anchor = mid(k.anchor, axis.reflect(other.anchor));
            target.in_ctrl = mid(k.in_ctrl, axis.reflect(other.out_ctrl));
            target.out_ctrl = mid(k.out_ctrl, axis.reflect(other.in_ctrl));
        }
    }
    (out != *path).then_some(out)
}
fn mid(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.)
}
fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
