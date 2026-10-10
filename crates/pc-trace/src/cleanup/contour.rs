//! Marching squares with linear edge interpolation, high-valued pixels 4-connected.
//! Algorithm semantics follow scikit-image's official find_contours documentation:
//! https://scikit-image.org/docs/stable/api/skimage.measure.html#skimage.measure.find_contours
//! Original Rust implementation; no scikit-image source is copied.
use super::check;
use crate::Error;
use kurbo::Point;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Edge {
    x: usize,
    y: usize,
    vertical: bool,
}

pub(super) fn contours(field: &[f64], w: usize, h: usize, budget: &mut usize, cancel: &impl Fn() -> bool) -> Result<Vec<Vec<Point>>, Error> {
    let mut links: BTreeMap<Edge, (Edge, Point)> = BTreeMap::new();
    let get = |x: usize, y: usize| if x == 0 || y == 0 || x > w || y > h { 0. } else { field[(y - 1) * w + x - 1] };
    let level = 0.5 + 1e-9;
    for y in 0..=h {
        check(cancel)?;
        for x in 0..=w {
            let values = [get(x, y), get(x + 1, y), get(x + 1, y + 1), get(x, y + 1)];
            let case = values.iter().enumerate().fold(0u8, |n, (i, &v)| n | (u8::from(v >= level) << i));
            let pairs: &[(usize, usize)] = match case {
                0 | 15 => &[],
                1 | 14 => &[(0, 3)],
                2 | 13 => &[(0, 1)],
                3 | 12 => &[(1, 3)],
                4 | 11 => &[(1, 2)],
                5 => &[(0, 3), (1, 2)],
                6 | 9 => &[(0, 2)],
                7 | 8 => &[(2, 3)],
                10 => &[(0, 1), (2, 3)],
                _ => &[],
            };
            let vertices = [
                Point::new(x as f64 - 0.5, y as f64 - 0.5),
                Point::new(x as f64 + 0.5, y as f64 - 0.5),
                Point::new(x as f64 + 0.5, y as f64 + 0.5),
                Point::new(x as f64 - 0.5, y as f64 + 0.5),
            ];
            let edges =
                [Edge { x, y, vertical: false }, Edge { x: x + 1, y, vertical: true }, Edge { x, y: y + 1, vertical: false }, Edge { x, y, vertical: true }];
            let ends = [(0, 1), (1, 2), (3, 2), (0, 3)];
            let point = |edge: usize| {
                let (a, b) = ends[edge];
                let t = ((level - values[a]) / (values[b] - values[a])).clamp(0., 1.);
                vertices[a].lerp(vertices[b], t)
            };
            for &(a, b) in pairs {
                if *budget == 0 {
                    return Err(Error::BoundaryBudget);
                }
                *budget -= 1;
                let (p, q) = (point(a), point(b));
                let mid = p.lerp(q, 0.5);
                let high = vertices
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| values[*i] >= level)
                    .min_by(|(_, p), (_, q)| p.distance(mid).total_cmp(&q.distance(mid)))
                    .map(|(_, p)| *p)
                    .ok_or(Error::BoundaryBudget)?;
                let (from, to, start) = if (q - p).cross(high - mid) > 0. { (edges[a], edges[b], p) } else { (edges[b], edges[a], q) };
                if links.insert(from, (to, start)).is_some() {
                    return Err(Error::BoundaryBudget);
                }
            }
        }
    }
    let mut out = Vec::new();
    while let Some((&start, _)) = links.first_key_value() {
        check(cancel)?;
        let mut edge = start;
        let mut ring = Vec::new();
        loop {
            if ring.len() % 4096 == 0 {
                check(cancel)?;
            }
            let (next, p) = links.remove(&edge).ok_or(Error::BoundaryBudget)?;
            if ring.last() != Some(&p) {
                ring.push(p);
            }
            edge = next;
            if edge == start {
                break;
            }
        }
        if ring.len() >= 3 {
            out.push(ring);
        }
    }
    Ok(out)
}
