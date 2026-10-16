//! VectorCraft effects/distort.rs and util.rs, d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
//! Native path operations, explicit errors and global budgets are Local Image adaptations.
use super::*;
use kurbo::{Affine, ParamCurveArclen, Vec2};
fn noise(seed: u64, a: u64, b: u64, c: u64) -> f64 {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ a.wrapping_mul(0xBF58_476D_1CE4_E5B9) ^ b.wrapping_mul(0x94D0_49BB_1331_11EB) ^ c;
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64 * 2. - 1.
}
fn normal(c: CubicBez, t: f64) -> Vec2 {
    let mut d = c.eval((t + 1e-4).min(1.)) - c.eval((t - 1e-4).max(0.));
    if d.hypot() < 1e-12 {
        d = c.p3 - c.p0;
    }
    if d.hypot() < 1e-12 { Vec2::ZERO } else { Vec2::new(d.y, -d.x) / d.hypot() }
}
fn points_knots(pts: &[Point], closed: bool, style: Points) -> Vec<Knot> {
    let n = pts.len();
    pts.iter()
        .enumerate()
        .map(|(i, &p)| {
            if n < 3 || style == Points::Corner {
                return Knot::corner(p.x, p.y);
            }
            let prev = if i == 0 { if closed { pts[n - 1] } else { p } } else { pts[i - 1] };
            let next = if i + 1 == n { if closed { pts[0] } else { p } } else { pts[i + 1] };
            let t = (next - prev) / 6.;
            Knot::smooth(doc_point(p), doc_point(p - t), doc_point(p + t))
        })
        .collect()
}
fn resample(path: &Path, b: Rect, e: &EffectKind, options: Options) -> Result<Path> {
    let (size, relative, points) = match e {
        EffectKind::Roughen { size, relative, points, .. } | EffectKind::ZigZag { size, relative, points, .. } => (*size, *relative, *points),
        _ => return Err(Error::Invalid),
    };
    // Identity must preserve handles, smooth flags, holes and component operations exactly.
    if size == 0. {
        return Ok(path.clone());
    }
    let amp = if relative { size / 100. * (b.width() + b.height()) / 2. } else { size };
    let mut out = path.clone();
    let mut nodes: usize = 0;
    for (si, sp) in out.subpaths.iter_mut().enumerate() {
        let src = &path.subpaths[si];
        let n = src.knots.len();
        let segs = if src.closed { n } else { n.saturating_sub(1) };
        let mut pts = Vec::new();
        for i in 0..segs {
            let c = cubic(&src.knots[i], &src.knots[(i + 1) % n]);
            let k = match e {
                EffectKind::Roughen { detail, .. } => {
                    if *detail == 0. {
                        1
                    } else {
                        (c.arclen(0.01) * detail / options.dpi).ceil().max(1.) as usize
                    }
                }
                EffectKind::ZigZag { ridges, .. } => *ridges as usize + 1,
                _ => return Err(Error::Invalid),
            };
            if k > MAX_NODES || nodes.saturating_add(pts.len()).saturating_add(k) > MAX_NODES {
                return Err(Error::Budget);
            }
            for j in 0..k {
                let t = j as f64 / k as f64;
                let mut p = c.eval(t);
                match e {
                    EffectKind::Roughen { seed, .. } => {
                        p.x += amp * noise(*seed, si as u64, pts.len() as u64, 1);
                        p.y += amp * noise(*seed, si as u64, pts.len() as u64, 2);
                    }
                    EffectKind::ZigZag { .. } => {
                        let mut norm = normal(c, t);
                        if j == 0 && (i > 0 || src.closed) {
                            let prev = (i + segs - 1) % segs;
                            let sum = norm + normal(cubic(&src.knots[prev], &src.knots[(prev + 1) % n]), 1.);
                            if sum.hypot() > 1e-9 {
                                norm = sum / sum.hypot();
                            }
                        }
                        p += norm * amp * if pts.len().is_multiple_of(2) { 1. } else { -1. };
                    }
                    _ => return Err(Error::Invalid),
                }
                pts.push(p);
            }
        }
        if !src.closed
            && let Some(last) = src.knots.last()
        {
            let mut p = Point::new(last.anchor.x, last.anchor.y);
            match e {
                EffectKind::Roughen { seed, .. } => {
                    p.x += amp * noise(*seed, si as u64, pts.len() as u64, 1);
                    p.y += amp * noise(*seed, si as u64, pts.len() as u64, 2);
                }
                EffectKind::ZigZag { .. } if segs > 0 => {
                    p += normal(cubic(&src.knots[n - 2], &src.knots[n - 1]), 1.) * amp * if pts.len().is_multiple_of(2) { 1. } else { -1. };
                }
                _ => {}
            }
            pts.push(p);
        }
        if pts.is_empty() {
            pts.extend(src.knots.iter().map(|k| Point::new(k.anchor.x, k.anchor.y)));
        }
        nodes += pts.len();
        if nodes > MAX_NODES {
            return Err(Error::Budget);
        }
        sp.knots = points_knots(&pts, src.closed, points);
    }
    validate_path(&out)?;
    Ok(out)
}
pub(super) fn transform(e: &EffectKind, b: Rect) -> Result<Affine> {
    let EffectKind::Transform { scale, move_by, angle, reflect, .. } = e else { return Err(Error::Invalid) };
    let c = b.center().to_vec2();
    Ok(Affine::translate(c + Vec2::new(move_by[0], move_by[1]))
        * Affine::rotate(-angle.to_radians())
        * Affine::scale_non_uniform(scale[0] / 100. * if reflect[0] { -1. } else { 1. }, scale[1] / 100. * if reflect[1] { -1. } else { 1. })
        * Affine::translate(-c))
}
pub(super) fn apply(path: &Path, e: &EffectKind, b: Rect, options: Options) -> Result<Path> {
    match e {
        EffectKind::Roughen { .. } | EffectKind::ZigZag { .. } => resample(path, b, e, options),
        EffectKind::PuckerBloat { amount } => {
            if *amount == 0. {
                return Ok(path.clone());
            }
            let a = *amount / 200.;
            let center = b.center();
            let mut out = path.clone();
            for (si, sp) in out.subpaths.iter_mut().enumerate() {
                let src = &path.subpaths[si];
                let n = src.knots.len();
                let segs = if src.closed { n } else { n.saturating_sub(1) };
                for i in 0..segs {
                    let c = cubic(&src.knots[i], &src.knots[(i + 1) % n]);
                    sp.knots[i].out_ctrl = doc_point(c.p1);
                    sp.knots[(i + 1) % n].in_ctrl = doc_point(c.p2);
                }
                for k in &mut sp.knots {
                    let p = Point::new(k.anchor.x, k.anchor.y);
                    k.anchor = doc_point(p.lerp(center, a));
                    k.in_ctrl = doc_point(Point::new(k.in_ctrl.x, k.in_ctrl.y).lerp(center, -a));
                    k.out_ctrl = doc_point(Point::new(k.out_ctrl.x, k.out_ctrl.y).lerp(center, -a));
                    k.smooth = false;
                }
            }
            validate_path(&out)?;
            Ok(out)
        }
        EffectKind::Twist { angle } => {
            if *angle == 0. {
                return Ok(path.clone());
            }
            let center = b.center();
            let radius = (b.width().hypot(b.height()) / 2.).max(1e-9);
            let angle = -angle.to_radians();
            mapping::map(path, options.tolerance, |p| {
                let d = p - center;
                let a = angle * (1. - d.hypot() / radius).max(0.);
                let (s, c) = a.sin_cos();
                center + Vec2::new(d.x * c - d.y * s, d.x * s + d.y * c)
            })
        }
        EffectKind::FreeDistort { corners: c } => {
            if *c == [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] {
                return Ok(path.clone());
            }
            let (w, h) = (if b.width() < 1e-9 { 1. } else { b.width() }, if b.height() < 1e-9 { 1. } else { b.height() });
            let (x, y) = (b.center().x - w / 2., b.center().y - h / 2.);
            let corners = c.map(|c| Point::new(x + c[0] * w, y + c[1] * h));
            mapping::map(path, options.tolerance, |p| {
                let u = (p.x - x) / w;
                let v = (p.y - y) / h;
                corners[0].lerp(corners[1], u).lerp(corners[3].lerp(corners[2], u), v)
            })
        }
        _ => Err(Error::Invalid),
    }
}
