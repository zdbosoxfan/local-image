//! Coons transfinite interpolation and outline partition adapted from VectorCraft
//! crates/doc/src/live.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0
//! Four-corner curves are exact. General outlines use bounded flattening and arc-length
//! boundary samples rather than upstream's fixed 65-point resampling.
use super::*;
use kurbo::Shape;
#[derive(Clone, Debug)]
enum Edge {
    Cubic(CubicBez),
    Polyline { points: Vec<Point>, lengths: Vec<f64> },
}
impl Edge {
    fn polyline(points: Vec<Point>) -> Result<Self> {
        let mut lengths = vec![0.0];
        for w in points.windows(2) {
            lengths.push(lengths.last().copied().ok_or(Error::TopObject)? + w[0].distance(w[1]));
        }
        if points.len() < 2 || lengths.last().is_none_or(|v| *v < 1e-12) {
            return Err(Error::TopObject);
        }
        Ok(Self::Polyline { points, lengths })
    }
    fn eval(&self, t: f64) -> Point {
        match self {
            Self::Cubic(c) => c.eval(t),
            Self::Polyline { points, lengths } => {
                let s = t * lengths[lengths.len() - 1];
                let i = lengths.partition_point(|v| *v <= s).saturating_sub(1).min(points.len() - 2);
                let d = lengths[i + 1] - lengths[i];
                points[i].lerp(points[i + 1], if d > 0.0 { (s - lengths[i]) / d } else { 0.0 })
            }
        }
    }
}
#[derive(Clone, Debug)]
pub(super) struct Coons {
    sides: [Edge; 4],
}
fn reverse(c: CubicBez) -> CubicBez {
    CubicBez::new(c.p3, c.p2, c.p1, c.p0)
}
fn flatten(c: CubicBez, depth: usize, out: &mut Vec<Point>, accuracy: f64) -> Result<()> {
    if out.len() >= 16384 || depth > 24 {
        return Err(Error::Budget);
    }
    let chord = c.p3 - c.p0;
    let length = chord.hypot();
    let flat = if length > 0.0 {
        (c.p1 - c.p0).cross(chord).abs().max((c.p2 - c.p0).cross(chord).abs()) / length <= accuracy
            && [c.p1, c.p2].iter().all(|p| {
                let t = (*p - c.p0).dot(chord) / chord.hypot2();
                (0.0..=1.0).contains(&t)
            })
    } else {
        c.p0.distance(c.p1).max(c.p0.distance(c.p2)) <= accuracy
    };
    if flat {
        if out.last().is_none_or(|p| p.distance(c.p3) > 1e-12) {
            out.push(c.p3);
        }
        return Ok(());
    }
    let (a, b) = c.subdivide();
    flatten(a, depth + 1, out, accuracy)?;
    flatten(b, depth + 1, out, accuracy)
}
impl Coons {
    pub fn new(path: &Path, accuracy: f64) -> Result<Self> {
        let raw = photocraft_pathops::to_geometry(path);
        let index = raw
            .subpaths
            .iter()
            .enumerate()
            .filter(|(_, s)| s.closed && s.anchors.len() >= 3)
            .max_by(|(_, a), (_, b)| a.signed_area().abs().total_cmp(&b.signed_area().abs()))
            .map(|(i, _)| i)
            .ok_or(Error::TopObject)?;
        let mut k = path.subpaths[index].knots.clone();
        if raw.subpaths[index].signed_area().abs() < 1e-12 {
            return Err(Error::TopObject);
        }
        if raw.subpaths[index].signed_area() < 0.0 {
            k.reverse();
            for a in &mut k {
                std::mem::swap(&mut a.in_ctrl, &mut a.out_ctrl);
            }
        }
        let mut boundary = kurbo::BezPath::new();
        raw.subpaths[index].to_bezpath_into(&mut boundary);
        let bounds = boundary.bounding_box();
        let tl = Point::new(bounds.x0, bounds.y0);
        if k.len() == 4 {
            let cross = |a: photocraft_geom::Point, b: photocraft_geom::Point, c: photocraft_geom::Point| (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
            if (0..4).all(|i| cross(k[i].anchor, k[(i + 1) % 4].anchor, k[(i + 2) % 4].anchor) > 1e-12) {
                let start = (0..4)
                    .min_by(|a, b| {
                        Point::new(k[*a].anchor.x, k[*a].anchor.y)
                            .distance_squared(tl)
                            .total_cmp(&Point::new(k[*b].anchor.x, k[*b].anchor.y).distance_squared(tl))
                    })
                    .ok_or(Error::TopObject)?;
                k.rotate_left(start);
                return Ok(Self {
                    sides: [
                        Edge::Cubic(cubic(&k[0], &k[1])),
                        Edge::Cubic(cubic(&k[1], &k[2])),
                        Edge::Cubic(reverse(cubic(&k[2], &k[3]))),
                        Edge::Cubic(reverse(cubic(&k[3], &k[0]))),
                    ],
                });
            }
        }
        let mut poly = vec![Point::new(k[0].anchor.x, k[0].anchor.y)];
        for i in 0..k.len() {
            flatten(cubic(&k[i], &k[(i + 1) % k.len()]), 0, &mut poly, accuracy)?;
        }
        if poly.first().zip(poly.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-12) {
            poly.pop();
        }
        if poly.len() < 4 {
            return Err(Error::TopObject);
        }
        let nearest = |p: Point| (0..poly.len()).min_by(|a, b| poly[*a].distance_squared(p).total_cmp(&poly[*b].distance_squared(p))).ok_or(Error::TopObject);
        let corners =
            [nearest(tl)?, nearest(Point::new(bounds.x1, bounds.y0))?, nearest(Point::new(bounds.x1, bounds.y1))?, nearest(Point::new(bounds.x0, bounds.y1))?];
        let n = poly.len();
        let tl = corners[0];
        let offsets = corners.map(|c| (c + n - tl) % n);
        if !(offsets[0] < offsets[1] && offsets[1] < offsets[2] && offsets[2] < offsets[3]) {
            return Err(Error::TopObject);
        }
        let arc = |a: usize, b: usize| {
            let count = (b + n - a) % n;
            (0..=count).map(|i| poly[(a + i) % n]).collect::<Vec<_>>()
        };
        let mut bottom = arc(corners[2], corners[3]);
        bottom.reverse();
        let mut left = arc(corners[3], corners[0]);
        left.reverse();
        Ok(Self {
            sides: [Edge::polyline(arc(corners[0], corners[1]))?, Edge::polyline(arc(corners[1], corners[2]))?, Edge::polyline(bottom)?, Edge::polyline(left)?],
        })
    }
    pub fn eval(&self, u: f64, v: f64) -> Point {
        let [top, right, bottom, left] = &self.sides;
        let p = (1.0 - v) * top.eval(u).to_vec2() + v * bottom.eval(u).to_vec2() + (1.0 - u) * left.eval(v).to_vec2() + u * right.eval(v).to_vec2()
            - ((1.0 - u) * (1.0 - v) * top.eval(0.0).to_vec2()
                + u * (1.0 - v) * top.eval(1.0).to_vec2()
                + (1.0 - u) * v * bottom.eval(0.0).to_vec2()
                + u * v * bottom.eval(1.0).to_vec2());
        p.to_point()
    }
}
