//! Directed lattice edges, with foreground on the right in document (y-down) coordinates.
use crate::vc::BinaryImage;
use kurbo::{BezPath, Point};
use std::collections::BTreeMap;
pub fn contours(mask: &BinaryImage, budget: &mut usize, cancel: &impl Fn() -> bool) -> Result<Vec<Vec<Point>>, crate::Error> {
    type V = (i32, i32);
    let mut edges: BTreeMap<V, Vec<V>> = BTreeMap::new();
    for y in 0..mask.height as i32 {
        if cancel() {
            return Err(crate::Error::Cancelled);
        }
        for x in 0..mask.width as i32 {
            if !mask.at(x, y) {
                continue;
            }
            for (outside, a, b) in [
                (!mask.at(x, y - 1), (x, y), (x + 1, y)),
                (!mask.at(x + 1, y), (x + 1, y), (x + 1, y + 1)),
                (!mask.at(x, y + 1), (x + 1, y + 1), (x, y + 1)),
                (!mask.at(x - 1, y), (x, y + 1), (x, y)),
            ] {
                if outside {
                    if *budget == 0 {
                        return Err(crate::Error::BoundaryBudget);
                    }
                    *budget -= 1;
                    edges.entry(a).or_default().push(b);
                }
            }
        }
    }
    let mut result = Vec::new();
    while let Some((&start, _)) = edges.first_key_value() {
        if cancel() {
            return Err(crate::Error::Cancelled);
        }
        let mut p = start;
        let mut previous = (start.0 - 1, start.1);
        let mut points = Vec::new();
        loop {
            points.push(Point::new(p.0 as f64, p.1 as f64));
            let Some(nexts) = edges.get_mut(&p) else {
                break;
            };
            // At a diagonal contact turn right, keeping the two components separate.
            let direction = (p.0 - previous.0, p.1 - previous.1);
            let idx = nexts
                .iter()
                .enumerate()
                .max_by_key(|(_, q)| {
                    let v = (q.0 - p.0, q.1 - p.1);
                    (direction.0 * v.1 - direction.1 * v.0, direction.0 * v.0 + direction.1 * v.1)
                })
                .map_or(0, |(i, _)| i);
            let next = nexts.remove(idx);
            if nexts.is_empty() {
                edges.remove(&p);
            }
            previous = p;
            p = next;
            if p == start {
                break;
            }
        }
        if points.len() >= 3 {
            result.push(points);
        }
    }
    Ok(result)
}
pub fn polygon(points: &[Point]) -> BezPath {
    let mut b = BezPath::new();
    if let Some(&p) = points.first() {
        b.move_to(p);
        for &p in &points[1..] {
            b.line_to(p);
        }
        b.close_path();
    }
    b
}
pub fn remove_collinear(points: &[Point]) -> Vec<Point> {
    let n = points.len();
    (0..n)
        .filter_map(|i| {
            let a = points[i] - points[(i + n - 1) % n];
            let b = points[(i + 1) % n] - points[i];
            (a.cross(b).abs() > 1e-9 || a.dot(b) < 0.).then_some(points[i])
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_allocation_is_bounded_and_cancellable() {
        let mut mask = BinaryImage::new_w_h(1, 1);
        mask.set_pixel(0, 0, true);
        assert!(matches!(contours(&mask, &mut 3, &|| false), Err(crate::Error::BoundaryBudget)));
        assert!(matches!(contours(&mask, &mut 4, &|| true), Err(crate::Error::Cancelled)));
        let result = contours(&mask, &mut 4, &|| false);
        assert!(result.is_ok_and(|loops| loops.len() == 1 && loops[0].len() == 4));
    }
}
