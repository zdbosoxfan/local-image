use super::*;
use photocraft_doc::vector_design::drawing::{CornerKind, DesignShape, ShapeParameters as P};
use photocraft_doc::{Effect, Effects, LiveShape, ShapeLayer, VectorData};
pub fn similarity(a: Affine) -> bool {
    let [x, y, u, v, _, _] = a.m;
    let scale = x.hypot(y).max(u.hypot(v));
    (x * u + y * v).abs() <= 1e-10 * scale * scale && (x.hypot(y) - u.hypot(v)).abs() <= 1e-10 * scale
}
pub fn live(sh: &mut ShapeLayer, vector: &mut VectorData, a: Affine, scale_corners: bool) -> Result<()> {
    if !similarity(a) {
        sh.live = None;
        vector.live_shape = None;
        return Ok(());
    }
    let k = a.m[0].hypot(a.m[1]);
    if scale_corners {
        for corner in &mut vector.corners {
            corner.corner.radius *= k;
        }
    }
    if let Some(LiveShape::Line { from, to, weight }) = &sh.live {
        let p = a.apply(Point::new(from[0], from[1]));
        let q = a.apply(Point::new(to[0], to[1]));
        sh.live = Some(LiveShape::Line { from: [p.x, p.y], to: [q.x, q.y], weight: *weight * k });
        return Ok(());
    }
    let source = vector.live_shape.clone().or_else(|| {
        sh.live.as_ref().map(|l| match l {
            LiveShape::Rect { rect, radii } => DesignShape { rect: *rect, angle: 0., parameters: P::Rect { radii: *radii, kinds: [CornerKind::Round; 4] } },
            LiveShape::Ellipse { rect } => DesignShape { rect: *rect, angle: 0., parameters: P::Ellipse },
            LiveShape::Polygon { rect, sides, star_ratio } => DesignShape {
                rect: *rect,
                angle: 0.,
                parameters: if *star_ratio == 1. {
                    P::Polygon { sides: *sides, curved: 0., corner: 0. }
                } else {
                    P::Star { points: *sides, inner: *star_ratio, corner: 0. }
                },
            },
            LiveShape::Line { .. } => DesignShape { rect: [0., 0., 1., 1.], angle: 0., parameters: P::Line { from: [0., 0.], to: [1., 1.] } },
        })
    });
    let Some(mut d) = source else {
        return Ok(());
    };
    let [x, y, w, h] = d.rect;
    let center = Point::new(x + w / 2., y + h / 2.);
    let c = a.apply(center);
    let flip = a.determinant() < 0.;
    let mirror_y = matches!(d.parameters, P::Crescent { .. } | P::PolarGrid { .. });
    let direction = a.m[1].atan2(a.m[0]);
    d.angle = if flip { direction - d.angle + if mirror_y { 0. } else { std::f64::consts::PI } } else { direction + d.angle };
    d.angle = (d.angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
    d.rect = [c.x - w * k / 2., c.y - h * k / 2., w * k, h * k];
    let local = |p: &mut [f64; 2]| {
        let dx = (p[0] - center.x) * k;
        let dy = (p[1] - center.y) * k;
        *p = [c.x + if flip && !mirror_y { -dx } else { dx }, c.y + if flip && mirror_y { -dy } else { dy }];
    };
    let corners = if scale_corners { k } else { 1. };
    match &mut d.parameters {
        P::Rect { radii, kinds } => {
            for r in radii.iter_mut() {
                *r *= corners;
            }
            if flip {
                radii.swap(0, 1);
                radii.swap(2, 3);
                kinds.swap(0, 1);
                kinds.swap(2, 3);
            }
        }
        P::Polygon { curved, corner, .. } => {
            *corner *= corners;
            if flip {
                *curved = -*curved;
            }
        }
        P::Star { corner, .. } | P::DoubleStar { corner, .. } | P::SquareStar { corner, .. } => *corner *= corners,
        P::Triangle { top } => {
            if flip {
                *top = 1. - *top;
            }
        }
        P::Trapezoid { top_left, top_right } => {
            if flip {
                let left = *top_left;
                *top_left = 1. - *top_right;
                *top_right = 1. - left;
            }
        }
        P::Pie { start, end } | P::Arc { start, end } | P::Segment { start, end, .. } => {
            if flip {
                *start = std::f64::consts::PI - *start;
                *end = std::f64::consts::PI - *end;
            }
        }
        P::Line { from, to } => {
            local(from);
            local(to);
        }
        P::Arrow { from, to, head_width, head_length, shaft, .. } => {
            local(from);
            local(to);
            *head_width *= k;
            *head_length *= k;
            *shaft *= k;
        }
        P::Callout { tail, radius, .. } => {
            local(tail);
            *radius *= corners;
        }
        P::Cog { .. } => {
            if flip {
                // The tooth's unequal radial phases have no handedness parameter;
                // rotating an elliptical cog cannot express its mirrored phase.
                sh.live = None;
                vector.live_shape = None;
                return Ok(());
            }
        }
        P::Tear { bend } => {
            if flip {
                *bend = -*bend;
            }
        }
        P::Spiral { center, radius, .. } => {
            if flip {
                sh.live = None;
                vector.live_shape = None;
                return Ok(());
            }
            local(center);
            *radius *= k;
        }
        _ => {}
    }
    super::super::drawing::geometry::validate(&d).map_err(|e| error(e.to_string()))?;
    // Without corner scaling the generator, rather than scaled rounded handles, is authoritative.
    if !scale_corners {
        sh.path = super::super::drawing::geometry::generate(&d).map_err(|e| error(e.to_string()))?;
    }
    sh.live = None;
    vector.live_shape = Some(d);
    Ok(())
}
pub fn effects(effects: &mut Effects, k: f64) -> Result<()> {
    let k = k as f32;
    let mul = |v: &mut f32| -> Result<()> {
        *v *= k;
        if !v.is_finite() || v.abs() > 1e6 {
            return Err(error("scaled effect exceeds bounds"));
        }
        Ok(())
    };
    for e in &mut effects.items {
        match e {
            Effect::DropShadow(s) | Effect::InnerShadow(s) => {
                mul(&mut s.distance)?;
                mul(&mut s.size)?;
            }
            Effect::OuterGlow(g) | Effect::InnerGlow(g) => mul(&mut g.size)?,
            Effect::Stroke(s) => mul(&mut s.size)?,
            Effect::Satin(s) => {
                mul(&mut s.distance)?;
                mul(&mut s.size)?;
            }
            Effect::BevelEmboss(b) => {
                mul(&mut b.size)?;
                mul(&mut b.soften)?;
            }
            _ => {}
        }
    }
    effects.psd_raw = None;
    Ok(())
}
/// Pattern paint uses its existing similarity/phase contract. General affine paint frames
/// cannot be represented by Fill::Pattern; reject instead of silently approximating them.
pub fn paint(fill: &mut Fill, a: Option<Affine>, transform: bool, old: Option<(f64, f64, f64, f64)>, path: &Path) -> Result<()> {
    let Fill::Pattern { scale, angle, phase, link, .. } = fill else {
        return Ok(());
    };
    let old_origin = if *link { old.map(|b| Point::new(b.0, b.1)).unwrap_or_default() } else { Point::new(0., 0.) };
    let new_origin = if *link { path.control_bounds().map(|b| Point::new(b.0, b.1)).unwrap_or_default() } else { Point::new(0., 0.) };
    let p = Point::new(old_origin.x + f64::from(phase.0), old_origin.y + f64::from(phase.1));
    let p = if transform {
        let a = a.ok_or_else(|| error("Turn Transform Patterns off to keep the pattern fixed while distorting the shape"))?;
        if !similarity(a) || a.determinant() < 0. {
            return Err(error("Pattern transforms currently support movement, rotation and uniform scale"));
        }
        *scale *= a.m[0].hypot(a.m[1]) as f32;
        *angle -= a.m[1].atan2(a.m[0]).to_degrees() as f32;
        a.apply(p)
    } else {
        p
    };
    *phase = ((p.x - new_origin.x) as f32, (p.y - new_origin.y) as f32);
    if !scale.is_finite() || *scale <= 0. || *scale > 10000. || !angle.is_finite() || !phase.0.is_finite() || !phase.1.is_finite() {
        return Err(error("transformed pattern exceeds bounds"));
    }
    Ok(())
}
