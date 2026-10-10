//! Ported from Potrace 1.16 © Peter Selinger: optimal polygon, vertex adjustment,
//! alphamax corner analysis, and area/tangency constrained opticurve.
// Copyright (C) 2001-2019 Peter Selinger.
// This file is part of Potrace. It is free software and it is covered
// by the GNU General Public License. See licenses/potrace-COPYING for details.
// SPDX-License-Identifier: GPL-2.0-or-later
// Source: potrace-1.16/src/trace.c. Pixel decomposition is supplied by boundary.rs.
use kurbo::{BezPath, Point, Vec2};
fn cyclic(a: usize, b: usize, c: usize) -> bool {
    if a <= c { a <= b && b < c } else { a <= b || b < c }
}
fn cross(a: Vec2, b: Vec2) -> f64 {
    a.cross(b)
}
fn para(a: Point, b: Point, c: Point) -> f64 {
    cross(b - a, c - a)
}
fn cprod(a: Point, b: Point, c: Point, d: Point) -> f64 {
    cross(b - a, d - c)
}
fn interval(t: f64, a: Point, b: Point) -> Point {
    a + (b - a) * t
}
fn sign(x: f64) -> f64 {
    if x > 0. {
        1.
    } else if x < 0. {
        -1.
    } else {
        0.
    }
}
fn denom(a: Point, b: Point) -> f64 {
    let d = b - a;
    sign(d.x) * d.x + sign(d.y) * d.y
}
#[derive(Clone, Copy, Default)]
struct Sums {
    x: f64,
    y: f64,
    x2: f64,
    xy: f64,
    y2: f64,
}
struct Data<'a> {
    p: &'a [Point],
    s: Vec<Sums>,
}
impl<'a> Data<'a> {
    fn new(p: &'a [Point]) -> Self {
        let mut s = vec![Sums::default()];
        for q in p {
            let d = *q - p[0];
            let t = s[s.len() - 1];
            s.push(Sums { x: t.x + d.x, y: t.y + d.y, x2: t.x2 + d.x * d.x, xy: t.xy + d.x * d.y, y2: t.y2 + d.y * d.y });
        }
        Self { p, s }
    }
    fn sum(&self, i: usize, j: usize) -> (Sums, f64) {
        let n = self.p.len();
        let r = j / n;
        let j = j % n;
        let a = self.s[i];
        let b = self.s[j + 1];
        let t = self.s[n];
        let r = r as f64;
        (
            Sums { x: b.x - a.x + r * t.x, y: b.y - a.y + r * t.y, x2: b.x2 - a.x2 + r * t.x2, xy: b.xy - a.xy + r * t.xy, y2: b.y2 - a.y2 + r * t.y2 },
            j as f64 + 1. - i as f64 + r * n as f64,
        )
    }
    fn penalty(&self, i: usize, j: usize) -> f64 {
        let (s, k) = self.sum(i, j);
        let a = self.p[i];
        let b = self.p[j % self.p.len()];
        let mid = interval(0.5, a, b) - self.p[0];
        let d = b - a;
        let (ex, ey) = (-d.y, d.x);
        let x = (s.x2 - 2. * s.x * mid.x) / k + mid.x * mid.x;
        let y = (s.xy - s.x * mid.y - s.y * mid.x) / k + mid.x * mid.y;
        let z = (s.y2 - 2. * s.y * mid.y) / k + mid.y * mid.y;
        (ex * ex * x + 2. * ex * ey * y + ey * ey * z).max(0.).sqrt()
    }
    fn slope(&self, i: usize, j: usize) -> (Point, Vec2) {
        let (s, k) = self.sum(i, j);
        let ctr = Point::new(s.x / k, s.y / k);
        let mut a = (s.x2 - s.x * s.x / k) / k;
        let b = (s.xy - s.x * s.y / k) / k;
        let mut c = (s.y2 - s.y * s.y / k) / k;
        let lambda = (a + c + ((a - c).powi(2) + 4. * b * b).sqrt()) / 2.;
        a -= lambda;
        c -= lambda;
        let d = if a.abs() >= c.abs() { Vec2::new(-b, a) } else { Vec2::new(-c, b) };
        let l = d.hypot();
        (ctr, if l == 0. { Vec2::ZERO } else { d / l })
    }
}
/// Fast straight-subpath constraints (Potrace calc_lon, including next-corner skipping).
fn longest(p: &[Point]) -> Vec<usize> {
    let n = p.len();
    let mut nc = vec![0; n];
    let mut k = 0;
    for i in (0..n).rev() {
        if p[i].x != p[k].x && p[i].y != p[k].y {
            k = i + 1;
        }
        nc[i] = k;
    }
    let mut piv = vec![0; n];
    for i in (0..n).rev() {
        let mut ct = [0; 4];
        let d = p[(i + 1) % n] - p[i];
        ct[((3. + 3. * d.x + d.y) / 2.) as usize] += 1;
        let mut con = [Vec2::ZERO; 2];
        let mut k = nc[i];
        let mut k1 = i;
        let mut all_directions = false;
        for _ in 0..=n {
            let d = p[k] - p[k1];
            ct[((3. + 3. * sign(d.x) + sign(d.y)) / 2.) as usize] += 1;
            if ct.iter().all(|&v| v != 0) {
                piv[i] = k1;
                all_directions = true;
                break;
            }
            let cur = p[k] - p[i];
            if cross(con[0], cur) < 0. || cross(con[1], cur) > 0. {
                break;
            }
            if cur.x.abs() > 1. || cur.y.abs() > 1. {
                let off = cur
                    + Vec2::new(
                        if cur.y >= 0. && (cur.y > 0. || cur.x < 0.) { 1. } else { -1. },
                        if cur.x <= 0. && (cur.x < 0. || cur.y < 0.) { 1. } else { -1. },
                    );
                if cross(con[0], off) >= 0. {
                    con[0] = off;
                }
                let off = cur
                    + Vec2::new(
                        if cur.y <= 0. && (cur.y < 0. || cur.x < 0.) { 1. } else { -1. },
                        if cur.x >= 0. && (cur.x > 0. || cur.y < 0.) { 1. } else { -1. },
                    );
                if cross(con[1], off) <= 0. {
                    con[1] = off;
                }
            }
            k1 = k;
            k = nc[k1];
            if !cyclic(k, i, k1) {
                break;
            }
        }
        if !all_directions {
            let d = p[k] - p[k1];
            let dk = Vec2::new(sign(d.x), sign(d.y));
            let cur = p[k1] - p[i];
            let (a, b, c, d) = (cross(con[0], cur), cross(con[0], dk), cross(con[1], cur), cross(con[1], dk));
            let mut j = 10_000_000.;
            if b < 0. {
                j = (a / -b).floor();
            }
            if d > 0. {
                j = j.min((-c / d).floor());
            }
            piv[i] = (k1 as i64 + j as i64).rem_euclid(n as i64) as usize;
        }
    }
    let mut lon = vec![0; n];
    let mut j = piv[n - 1];
    lon[n - 1] = j;
    for i in (0..n - 1).rev() {
        if cyclic(i + 1, piv[i], j) {
            j = piv[i];
        }
        lon[i] = j;
    }
    for i in (0..n).rev() {
        if !cyclic((i + 1) % n, j, lon[i]) {
            break;
        }
        lon[i] = j;
    }
    lon
}
fn best_polygon(d: &Data<'_>) -> Vec<usize> {
    let n = d.p.len();
    let lon = longest(d.p);
    let mut clip0 = vec![0; n];
    let mut clip1 = vec![0; n + 1];
    for i in 0..n {
        let mut c = (lon[(i + n - 1) % n] + n - 1) % n;
        if c == i {
            c = (i + 1) % n;
        }
        clip0[i] = if c < i { n } else { c };
    }
    let mut j = 1;
    for (i, &c) in clip0.iter().enumerate() {
        while j <= c && j <= n {
            clip1[j] = i;
            j += 1;
        }
    }
    let mut seg0 = vec![0];
    let mut i = 0;
    for _ in 0..n {
        if i >= n {
            break;
        }
        let next = clip0[i];
        if next <= i {
            return (0..n).collect();
        }
        i = next;
        seg0.push(i);
    }
    let m = seg0.len() - 1;
    let mut seg1 = vec![0; m + 1];
    let mut i = n;
    for j in (1..=m).rev() {
        seg1[j] = i;
        i = clip1[i];
    }
    let mut pen = vec![0.; n + 1];
    let mut prev = vec![0; n + 1];
    for j in 1..=m {
        for i in seg1[j]..=seg0[j] {
            let mut best = f64::INFINITY;
            for k in (clip1[i]..=seg0[j - 1]).rev() {
                let value = d.penalty(k, i) + pen[k];
                if value < best {
                    prev[i] = k;
                    best = value;
                }
            }
            pen[i] = best;
        }
    }
    let mut po = vec![0; m];
    let mut i = n;
    for j in (0..m).rev() {
        i = prev[i];
        po[j] = i;
    }
    po
}
type Quad = [[f64; 3]; 3];
fn quad(q: Quad, p: Point) -> f64 {
    let v = [p.x, p.y, 1.];
    let mut sum = 0.;
    for i in 0..3 {
        for j in 0..3 {
            sum += v[i] * q[i][j] * v[j];
        }
    }
    sum
}
fn adjust(d: &Data<'_>, po: &[usize]) -> Vec<Point> {
    let m = po.len();
    let n = d.p.len();
    let mut qs = vec![[[0.; 3]; 3]; m];
    for i in 0..m {
        let j = (po[(i + 1) % m] + n - po[i]) % n + po[i];
        let (c, v) = d.slope(po[i], j);
        let norm = v.hypot2();
        if norm != 0. {
            let v = [v.y, -v.x, v.x * c.y - v.y * c.x];
            for a in 0..3 {
                for b in 0..3 {
                    qs[i][a][b] = v[a] * v[b] / norm;
                }
            }
        }
    }
    let mut out = Vec::new();
    for i in 0..m {
        let s = d.p[po[i]] - d.p[0];
        let s = Point::new(s.x, s.y);
        let mut q = [[0.; 3]; 3];
        for a in 0..3 {
            for b in 0..3 {
                q[a][b] = qs[(i + m - 1) % m][a][b] + qs[i][a][b];
            }
        }
        let mut w = s;
        for _ in 0..3 {
            let det = q[0][0] * q[1][1] - q[0][1] * q[1][0];
            if det.abs() > 1e-15 {
                w = Point::new((-q[0][2] * q[1][1] + q[1][2] * q[0][1]) / det, (q[0][2] * q[1][0] - q[1][2] * q[0][0]) / det);
                break;
            }
            let (x, y) = if q[0][0] > q[1][1] {
                (-q[0][1], q[0][0])
            } else if q[1][1] != 0. {
                (-q[1][1], q[1][0])
            } else {
                (1., 0.)
            };
            let v = [x, y, -y * s.y - x * s.x];
            let norm = x * x + y * y;
            for a in 0..3 {
                for b in 0..3 {
                    q[a][b] += v[a] * v[b] / norm;
                }
            }
        }
        if (w.x - s.x).abs() > 0.5 || (w.y - s.y).abs() > 0.5 {
            let mut best = quad(q, s);
            w = s;
            let mut candidate = |p: Point| {
                let v = quad(q, p);
                if v < best && (p.x - s.x).abs() <= 0.500000001 && (p.y - s.y).abs() <= 0.500000001 {
                    best = v;
                    w = p;
                }
            };
            for z in [-0.5, 0.5] {
                if q[0][0] != 0. {
                    let y = s.y + z;
                    candidate(Point::new(-(q[0][1] * y + q[0][2]) / q[0][0], y));
                }
                if q[1][1] != 0. {
                    let x = s.x + z;
                    candidate(Point::new(x, -(q[1][0] * x + q[1][2]) / q[1][1]));
                }
            }
            for x in [-0.5, 0.5] {
                for y in [-0.5, 0.5] {
                    candidate(Point::new(s.x + x, s.y + y));
                }
            }
        }
        out.push(w + (d.p[0] - Point::ORIGIN));
    }
    out
}
#[derive(Clone, Copy)]
struct Segment {
    corner: bool,
    c: [Point; 3],
    vertex: Point,
    alpha: f64,
}
fn smooth(v: &[Point], alphamax: f64) -> Vec<Segment> {
    let m = v.len();
    let mut out = vec![Segment { corner: false, c: [Point::ORIGIN; 3], vertex: Point::ORIGIN, alpha: 0. }; m];
    for i in 0..m {
        let j = (i + 1) % m;
        let k = (i + 2) % m;
        let end = interval(0.5, v[k], v[j]);
        let denom = denom(v[i], v[k]);
        let mut a = if denom != 0. {
            let dd = (para(v[i], v[j], v[k]) / denom).abs();
            if dd > 1. { (1. - 1. / dd) / 0.75 } else { 0. }
        } else {
            4. / 3.
        };
        let corner = a >= alphamax;
        let c = if corner {
            [Point::ORIGIN, v[j], end]
        } else {
            a = a.clamp(0.55, 1.);
            [interval(0.5 + 0.5 * a, v[i], v[j]), interval(0.5 + 0.5 * a, v[k], v[j]), end]
        };
        out[j] = Segment { corner, c, vertex: v[j], alpha: a };
    }
    out
}
fn bezier(t: f64, p: [Point; 4]) -> Point {
    let s = 1. - t;
    Point::new(
        s * s * s * p[0].x + 3. * s * s * t * p[1].x + 3. * s * t * t * p[2].x + t * t * t * p[3].x,
        s * s * s * p[0].y + 3. * s * s * t * p[1].y + 3. * s * t * t * p[2].y + t * t * t * p[3].y,
    )
}
fn tangent(p: [Point; 4], q0: Point, q1: Point) -> Option<f64> {
    let a = cprod(p[0], p[1], q0, q1);
    let b = cprod(p[1], p[2], q0, q1);
    let c = cprod(p[2], p[3], q0, q1);
    let (a, b, c) = (a - 2. * b + c, -2. * a + 2. * b, a);
    let d = b * b - 4. * a * c;
    if a == 0. || d < 0. {
        return None;
    }
    for t in [(-b + d.sqrt()) / (2. * a), (-b - d.sqrt()) / (2. * a)] {
        if (0.0..=1.).contains(&t) {
            return Some(t);
        }
    }
    None
}
#[derive(Clone, Copy)]
struct Opt {
    pen: f64,
    c: [Point; 2],
    s: f64,
    alpha: f64,
}
fn opti_penalty(curve: &[Segment], i: usize, j: usize, tol: f64, convc: &[f64], area: &[f64]) -> Option<Opt> {
    let m = curve.len();
    if i == j {
        return None;
    }
    let i1 = (i + 1) % m;
    let conv = convc[i1];
    if conv == 0. {
        return None;
    }
    let d = curve[i].vertex.distance(curve[i1].vertex);
    let mut k = i1;
    while k != j {
        let k1 = (k + 1) % m;
        let k2 = (k + 2) % m;
        if convc[k1] != conv
            || sign(cprod(curve[i].vertex, curve[i1].vertex, curve[k1].vertex, curve[k2].vertex)) != conv
            || (curve[i1].vertex - curve[i].vertex).dot(curve[k2].vertex - curve[k1].vertex) < d * curve[k1].vertex.distance(curve[k2].vertex) * -0.999847695156
        {
            return None;
        }
        k = k1;
    }
    let p0 = curve[i].c[2];
    let p1 = curve[i1].vertex;
    let p2 = curve[j].vertex;
    let p3 = curve[j].c[2];
    let mut a = area[j] - area[i] - para(curve[0].vertex, p0, p3) / 2.;
    if i >= j {
        a += area[m];
    }
    let (a1, a2, a3) = (para(p0, p1, p2), para(p0, p1, p3), para(p0, p2, p3));
    let a4 = a1 + a3 - a2;
    if a2 == a1 || a3 == a4 {
        return None;
    }
    let t = a3 / (a3 - a4);
    let s = a2 / (a2 - a1);
    let aa = a2 * t / 2.;
    if aa == 0. {
        return None;
    }
    let discriminant = 4. - a / aa / 0.3;
    if discriminant < 0. {
        return None;
    }
    let alpha = 2. - discriminant.sqrt();
    let c = [interval(t * alpha, p0, p1), interval(s * alpha, p3, p2)];
    let p = [p0, c[0], c[1], p3];
    let mut pen = 0.;
    let mut k = i1;
    while k != j {
        let k1 = (k + 1) % m;
        let a = curve[k].vertex;
        let b = curve[k1].vertex;
        let t = tangent(p, a, b)?;
        let pt = bezier(t, p);
        let d = a.distance(b);
        if d == 0. {
            return None;
        }
        let d1 = para(a, b, pt) / d;
        if d1.abs() > tol || (b - a).dot(pt - a) < 0. || (a - b).dot(pt - b) < 0. {
            return None;
        }
        pen += d1 * d1;
        k = k1;
    }
    let mut k = i;
    while k != j {
        let k1 = (k + 1) % m;
        let a = curve[k].c[2];
        let b = curve[k1].c[2];
        let pt = bezier(tangent(p, a, b)?, p);
        let d = a.distance(b);
        if d == 0. {
            return None;
        }
        let mut d1 = para(a, b, pt) / d;
        let mut d2 = para(a, b, curve[k1].vertex) / d * 0.75 * curve[k1].alpha;
        if d2 < 0. {
            d1 = -d1;
            d2 = -d2;
        }
        if d1 < d2 - tol {
            return None;
        }
        if d1 < d2 {
            pen += (d1 - d2).powi(2);
        }
        k = k1;
    }
    Some(Opt { pen, c, s, alpha })
}
fn optimize(curve: Vec<Segment>, tol: f64) -> Vec<Segment> {
    let m = curve.len();
    let mut conv = vec![0.; m];
    for i in 0..m {
        if !curve[i].corner {
            conv[i] = sign(para(curve[(i + m - 1) % m].vertex, curve[i].vertex, curve[(i + 1) % m].vertex));
        }
    }
    let mut area = vec![0.; m + 1];
    for i in 0..m {
        let j = (i + 1) % m;
        area[i + 1] = area[i];
        if !curve[j].corner {
            let alpha = curve[j].alpha;
            area[i + 1] += 0.3 * alpha * (4. - alpha) * para(curve[i].c[2], curve[j].vertex, curve[j].c[2]) / 2.
                + para(curve[0].vertex, curve[i].c[2], curve[j].c[2]) / 2.;
        }
    }
    let mut prev = vec![0; m + 1];
    let mut pen = vec![0.; m + 1];
    let mut len = vec![0; m + 1];
    let mut opts = vec![None; m + 1];
    for j in 1..=m {
        prev[j] = j - 1;
        pen[j] = pen[j - 1];
        len[j] = len[j - 1] + 1;
        for i in (0..j.saturating_sub(1)).rev() {
            let Some(o) = opti_penalty(&curve, i, j % m, tol, &conv, &area) else {
                break;
            };
            if len[j] > len[i] + 1 || (len[j] == len[i] + 1 && pen[j] > pen[i] + o.pen) {
                prev[j] = i;
                pen[j] = pen[i] + o.pen;
                len[j] = len[i] + 1;
                opts[j] = Some(o);
            }
        }
    }
    let mut result = Vec::new();
    let mut j = m;
    while j > 0 {
        let old = curve[j % m];
        if prev[j] == j - 1 {
            result.push(old);
        } else if let Some(o) = opts[j] {
            result.push(Segment { corner: false, c: [o.c[0], o.c[1], old.c[2]], vertex: interval(o.s, old.c[2], old.vertex), alpha: o.alpha });
        }
        j = prev[j];
    }
    result.reverse();
    result
}
pub fn fit(points: &[Point], alphamax: f64, tolerance: f64) -> BezPath {
    if points.len() < 3 {
        return BezPath::new();
    }
    let d = Data::new(points);
    let po = best_polygon(&d);
    if po.len() < 3 {
        return crate::boundary::polygon(points);
    }
    let vertices = adjust(&d, &po);
    let c = smooth(&vertices, alphamax);
    let c = if tolerance > 0. { optimize(c, tolerance) } else { c };
    let mut out = BezPath::new();
    if let Some(last) = c.last() {
        out.move_to(last.c[2]);
        for s in c {
            if s.corner {
                out.line_to(s.c[1]);
                out.line_to(s.c[2]);
            } else {
                out.curve_to(s.c[0], s.c[1], s.c[2]);
            }
        }
        out.close_path();
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_c_numeric_stage_fixtures() -> Result<(), Box<dyn std::error::Error>> {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!("../tests/golden/potrace-stages.json"))?;
        for fixture in fixtures.as_array().ok_or("fixtures")? {
            let points: Vec<[f64; 2]> = serde_json::from_value(fixture["points"].clone())?;
            let points: Vec<Point> = points.into_iter().map(|p| Point::new(p[0], p[1])).collect();
            let d = Data::new(&points);
            let po = best_polygon(&d);
            assert_eq!(serde_json::to_value(&po)?, fixture["polygon"], "{} polygon", fixture["name"]);
            let out = optimize(smooth(&adjust(&d, &po), 1.), 0.2);
            let reference = fixture["segments"].as_array().ok_or("segments")?;
            assert_eq!(out.len(), reference.len(), "{} segment count", fixture["name"]);
            for (a, b) in out.iter().zip(reference) {
                assert_eq!(a.corner, b["corner"].as_bool().ok_or("corner")?);
                let reference: Vec<[f64; 2]> = serde_json::from_value(b["points"].clone())?;
                let corner = a.corner;
                for (i, (a, b)) in a.c.iter().zip(reference).enumerate() {
                    if corner && i == 0 {
                        continue;
                    }
                    assert!(a.distance(Point::new(b[0], b[1])) < 1e-8, "{} control mismatch {a:?} {b:?}", fixture["name"]);
                }
            }
        }
        Ok(())
    }
}
