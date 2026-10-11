//! Adapted from VectorCraft crates/brush/src/calli.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092. MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
use super::track::{Track, normal};
use super::{Calligraphic, Error, MAX_VERTICES, Result, native, normalize, widths};
use photocraft_doc::vector_design::appearance::StrokeItem;
use photocraft_doc::{Path, PathOp};
use photocraft_pathops::geom::{Point, Vec2};

fn support(c: &Calligraphic, d: Vec2, half: f64) -> Vec2 {
    let angle = c.angle.to_radians();
    let u = Vec2::new(angle.cos(), -angle.sin());
    let v = normal(u);
    let a = half * c.size;
    let b = a * c.roundness;
    let (du, dv) = (u.dot(d), v.dot(d));
    let den = (a * du).hypot(b * dv);
    if den <= 1e-12 { Vec2::ZERO } else { (u * (a * a * du) + v * (b * b * dv)) / den }
}
fn rot(base: Vec2, angle: f64) -> Vec2 {
    base * angle.cos() + normal(base) * angle.sin()
}
pub(super) fn calligraphic(c: &Calligraphic, tracks: &[Track], stroke: &StrokeItem) -> Result<Vec<Path>> {
    let mut subs = Vec::new();
    let mut count = 0;
    for t in tracks {
        let mut samples: Vec<(Point, Vec2, f64)> = Vec::new();
        for i in 0..=t.segments() {
            let p = t.verts[i];
            let pos = t.cum[i] / t.len();
            if t.corner[i] {
                let (a, b) = t.in_out(i);
                let turn = a.cross(b).atan2(a.dot(b));
                let steps = (turn.abs() / 10f64.to_radians()).ceil().max(1.) as usize;
                for k in 0..=steps {
                    samples.push((p, rot(a, turn * k as f64 / steps as f64), pos));
                }
            } else {
                samples.push((p, t.tangents[i], pos));
            }
            if i < t.segments()
                && let Some(profile) = &stroke.profile
            {
                for q in &profile.points {
                    let s = q.t * t.len();
                    if s > t.cum[i] && s < t.cum[i + 1] {
                        let (p, tg) = t.frame(s);
                        samples.push((p, tg, q.t));
                    }
                }
            }
        }
        count += samples.len() * 2 + 32;
        if count > MAX_VERTICES {
            return Err(Error::Budget);
        }
        let left: Vec<_> = samples.iter().map(|(p, tg, s)| *p + support(c, normal(*tg), widths(stroke, *s).0)).collect();
        let right: Vec<_> = samples.iter().map(|(p, tg, s)| *p + support(c, -normal(*tg), widths(stroke, *s).1)).collect();
        if t.closed {
            subs.push(native(&left, true));
            let mut r = right;
            r.reverse();
            let mut hole = native(&r, true);
            hole.op = PathOp::Join;
            subs.push(hole);
        } else {
            let mut poly = left;
            let cap = |p: Point, tg: Vec2, s: f64, from: f64, to: f64, out: &mut Vec<Point>| {
                let (l, r) = widths(stroke, s);
                for k in 1..16 {
                    let angle = from + (to - from) * k as f64 / 16.;
                    let half = if angle.sin() >= 0. { l } else { r };
                    out.push(p + support(c, rot(tg, angle), half));
                }
            };
            let half = std::f64::consts::FRAC_PI_2;
            let (pe, te, se) = samples[samples.len() - 1];
            cap(pe, te, se, half, -half, &mut poly);
            poly.extend(right.into_iter().rev());
            let (ps, ts, ss) = samples[0];
            cap(ps, ts, ss, -half, -3. * half, &mut poly);
            subs.push(native(&poly, true));
        }
    }
    let path = normalize(Path::new(subs))?;
    if path.is_empty() {
        return Err(Error::Degenerate);
    }
    Ok(vec![path])
}
