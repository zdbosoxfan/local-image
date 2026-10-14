//! Paint/appearance interpolation adapted from VectorCraft doc/blend.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Oklab follows Björn Ottosson's published 2021 matrices (MIT):
//! https://bottosson.github.io/posts/oklab/. Transfer functions come from pc-color.
use super::{Result, error};
use li_color::convert::{linear_to_srgb, srgb_to_linear};
use photocraft_color::Color;
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, WidthPoint, WidthProfile};
use photocraft_doc::{Fill, Layer, LayerContent, ShapeStroke};

pub fn oklab(color: Color) -> [f64; 3] {
    let rgb = color.to_rgb().map(|v| f64::from(srgb_to_linear(v)));
    li_color::srgb_perceptual::linear_srgb_to_oklab_f64(rgb)
}
fn rgb([l, a, b]: [f64; 3], alpha: f32) -> Color {
    let [r, g, b] = li_color::srgb_perceptual::oklab_to_linear_srgb_f64([l, a, b]);
    Color::rgba(linear_to_srgb(r as f32), linear_to_srgb(g as f32), linear_to_srgb(b as f32), alpha)
}
fn mix(a: f32, b: f32, t: f64) -> f32 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * t) as f32
}
pub fn color(a: Color, b: Color, t: f64) -> Color {
    if t == 0. {
        return a;
    }
    if t == 1. {
        return b;
    }
    if a == b {
        return a;
    }
    let (a_lab, b_lab) = (oklab(a), oklab(b));
    rgb(std::array::from_fn(|i| a_lab[i] + (b_lab[i] - a_lab[i]) * t), mix(a.alpha, b.alpha, t))
}
fn sample(stops: &[(f32, Color)], midpoints: &[f32], t: f32) -> Color {
    let mut stops = stops.to_vec();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some(first) = stops.first() else { return Color::TRANSPARENT };
    if t <= first.0 {
        return first.1;
    }
    for (i, w) in stops.windows(2).enumerate() {
        if t <= w[1].0 {
            let f = ((t - w[0].0) / (w[1].0 - w[0].0).max(1e-9)).clamp(0., 1.);
            let mid = midpoints.get(i).copied().unwrap_or(0.5).clamp(0.001, 0.999);
            let u = if f <= mid { 0.5 * f / mid } else { 0.5 + 0.5 * (f - mid) / (1. - mid) };
            return color(w[0].1, w[1].1, f64::from(u));
        }
    }
    stops.last().map_or(Color::TRANSPARENT, |s| s.1)
}
fn sample_alpha(stops: &[(f32, f32)], t: f32) -> f32 {
    if stops.is_empty() {
        return 1.;
    }
    let s: Vec<_> = stops.iter().map(|(t, a)| (*t, Color::rgba(0., 0., 0., *a))).collect();
    sample(&s, &[], t).alpha
}
fn solid_like(c: Color, p: &Fill) -> Fill {
    let mut out = p.clone();
    if let Fill::Gradient { stops, opacity_stops, .. } = &mut out {
        for (_, x) in stops {
            *x = c;
        }
        opacity_stops.clear();
    }
    out
}
pub fn fill(a: &Fill, b: &Fill, t: f64) -> Fill {
    if t == 0. {
        return a.clone();
    }
    if t == 1. {
        return b.clone();
    }
    match (a, b) {
        (Fill::Solid(x), Fill::Solid(y)) => Fill::Solid(color(*x, *y, t)),
        (Fill::Solid(c), Fill::Gradient { .. }) => fill(&solid_like(*c, b), b, t),
        (Fill::Gradient { .. }, Fill::Solid(c)) => fill(a, &solid_like(*c, a), t),
        (
            Fill::Gradient { stops: sa, midpoints: ma, opacity_stops: oa, angle: aa, scale: za, offset: fa, style: ka, .. },
            Fill::Gradient { stops: sb, midpoints: mb, opacity_stops: ob, angle: ab, scale: zb, offset: fb, style: kb, .. },
        ) if ka == kb => {
            let mut out = if t < 0.5 { a.clone() } else { b.clone() };
            if let Fill::Gradient { stops, midpoints, opacity_stops, angle, scale, offset, .. } = &mut out {
                if sa.len() == sb.len() {
                    *stops = sa.iter().zip(sb).map(|(a, b)| (mix(a.0, b.0, t), color(a.1, b.1, t))).collect();
                    *midpoints =
                        (0..sa.len().saturating_sub(1)).map(|i| mix(ma.get(i).copied().unwrap_or(0.5), mb.get(i).copied().unwrap_or(0.5), t)).collect();
                } else {
                    let mut positions: Vec<_> = sa.iter().chain(sb).map(|s| s.0).collect();
                    for (s, m) in [(sa, ma), (sb, mb)] {
                        for (i, w) in s.windows(2).enumerate() {
                            if let Some(mid) = m.get(i) {
                                positions.push(w[0].0 + (w[1].0 - w[0].0) * mid);
                            }
                        }
                    }
                    positions.sort_by(f32::total_cmp);
                    positions.dedup();
                    *stops = positions.iter().map(|p| (*p, color(sample(sa, ma, *p), sample(sb, mb, *p), t))).collect();
                    midpoints.clear();
                }
                let mut positions: Vec<_> = oa.iter().chain(ob).map(|s| s.0).collect();
                positions.sort_by(f32::total_cmp);
                positions.dedup();
                *opacity_stops = positions.iter().map(|p| (*p, mix(sample_alpha(oa, *p), sample_alpha(ob, *p), t))).collect();
                *angle = mix(*aa, *ab, t);
                *scale = mix(*za, *zb, t);
                *offset = (mix(fa.0, fb.0, t), mix(fa.1, fb.1, t));
            }
            out
        }
        _ => {
            if t < 0.5 {
                a.clone()
            } else {
                b.clone()
            }
        }
    }
}
fn profile(a: Option<&WidthProfile>, b: Option<&WidthProfile>, wa: f32, wb: f32, t: f64) -> Option<WidthProfile> {
    if a.is_none() && b.is_none() {
        return None;
    }
    let mut positions = vec![0., 1.];
    for p in a.into_iter().chain(b) {
        positions.extend(p.points.iter().map(|p| p.t));
    }
    positions.sort_by(f64::total_cmp);
    positions.dedup();
    let at = |p: Option<&WidthProfile>, w: f32, u: f64| -> [f64; 2] {
        let Some(p) = p.filter(|p| !p.points.is_empty()) else { return [f64::from(w) / 2.; 2] };
        let i = p.points.partition_point(|p| p.t <= u).saturating_sub(1);
        let x = p.points[i];
        let y = p.points.get(i + 1).copied().unwrap_or(x);
        let v = if y.t > x.t { ((u - x.t) / (y.t - x.t)).clamp(0., 1.) } else { 0. };
        [x.left + (y.left - x.left) * v, x.right + (y.right - x.right) * v]
    };
    Some(WidthProfile {
        points: positions
            .into_iter()
            .map(|u| {
                let (x, y) = (at(a, wa, u), at(b, wb, u));
                WidthPoint { t: u, left: x[0] + (y[0] - x[0]) * t, right: x[1] + (y[1] - x[1]) * t }
            })
            .collect(),
    })
}
fn stroke(a: &ShapeStroke, b: &ShapeStroke, t: f64) -> ShapeStroke {
    let mut out = if t < 0.5 { a.clone() } else { b.clone() };
    out.paint = fill(&a.paint, &b.paint, t);
    out.width = mix(a.width, b.width, t);
    out.opacity = mix(a.opacity, b.opacity, t);
    out.miter_limit = mix(a.miter_limit, b.miter_limit, t);
    out.dash_offset = mix(a.dash_offset, b.dash_offset, t);
    // Repeat odd patterns twice; empty/solid closes the other pattern's gaps.
    let even = |s: &[f32]| if s.len() % 2 == 1 { s.repeat(2) } else { s.to_vec() };
    let (mut da, mut db) = (even(&a.dashes), even(&b.dashes));
    let solid = |p: &[f32]| p.chunks(2).flat_map(|w| [w.iter().sum(), 0.]).collect::<Vec<_>>();
    if da.is_empty() {
        da = solid(&db);
    }
    if db.is_empty() {
        db = solid(&da);
    }
    let n = da.len().max(db.len());
    out.dashes = (0..n).map(|i| mix(da[i % da.len()], db[i % db.len()], t)).collect();
    out
}
fn stack(layer: &Layer) -> Appearance {
    layer.vector.appearance.clone().unwrap_or_else(|| match &layer.content {
        LayerContent::Shape(s) => photocraft_vector::appearance::legacy_appearance(s),
        _ => Appearance::default(),
    })
}
pub fn appearance(a: &Layer, b: &Layer, t: f64) -> Result<Appearance> {
    let (a, b) = (stack(a), stack(b));
    let (near, far) = if t < 0.5 { (&a, &b) } else { (&b, &a) };
    let mut out = Appearance { items: (0..a.items.len().max(b.items.len())).filter_map(|i| near.items.get(i).or_else(|| far.items.get(i)).cloned()).collect() };
    for (i, item) in out.items.iter_mut().enumerate() {
        match (item, a.items.get(i), b.items.get(i)) {
            (AppearanceItem::Mesh(o), Some(AppearanceItem::Mesh(x)), Some(AppearanceItem::Mesh(y))) => {
                if x.paint.rows != y.paint.rows || x.paint.cols != y.paint.cols || x.paint.points.len() != y.paint.points.len() {
                    return Err(error("blending gradient meshes requires matching topology"));
                }
                for ((p, x), y) in o.paint.points.iter_mut().zip(&x.paint.points).zip(&y.paint.points) {
                    p.position = std::array::from_fn(|i| x.position[i] + (y.position[i] - x.position[i]) * t);
                    p.handles = std::array::from_fn(|h| std::array::from_fn(|i| x.handles[h][i] + (y.handles[h][i] - x.handles[h][i]) * t));
                    p.rgba = std::array::from_fn(|i| mix(x.rgba[i], y.rgba[i], t));
                    p.opacity = mix(x.opacity, y.opacity, t);
                }
                o.opacity = mix(if x.visible { x.opacity } else { 0. }, if y.visible { y.opacity } else { 0. }, t);
                o.visible = true;
            }
            (AppearanceItem::Mesh(o), x, y) => {
                let opacity = |item: Option<&AppearanceItem>| match item {
                    Some(AppearanceItem::Mesh(m)) if m.visible => m.opacity,
                    _ => 0.,
                };
                o.opacity = mix(opacity(x), opacity(y), t);
                o.visible = true;
            }
            (AppearanceItem::Fill(o), Some(AppearanceItem::Fill(x)), Some(AppearanceItem::Fill(y))) => {
                o.paint = fill(&x.paint, &y.paint, t);
                o.opacity = mix(if x.visible { x.opacity } else { 0. }, if y.visible { y.opacity } else { 0. }, t);
                o.visible = true;
            }
            (AppearanceItem::Stroke(o), Some(AppearanceItem::Stroke(x)), Some(AppearanceItem::Stroke(y))) => {
                o.base = stroke(&x.base, &y.base, t);
                o.base.opacity = mix(if x.visible { x.base.opacity } else { 0. }, if y.visible { y.base.opacity } else { 0. }, t);
                o.visible = true;
                o.profile = profile(x.profile.as_ref(), y.profile.as_ref(), x.base.width, y.base.width, t);
                o.arrow_scale = std::array::from_fn(|i| x.arrow_scale[i] + (y.arrow_scale[i] - x.arrow_scale[i]) * t);
            }
            (AppearanceItem::Fill(o), x, y) => {
                let opacity = |item: Option<&AppearanceItem>| match item {
                    Some(AppearanceItem::Fill(f)) if f.visible => f.opacity,
                    _ => 0.,
                };
                o.opacity = mix(opacity(x), opacity(y), t);
                o.visible = true;
            }
            (AppearanceItem::Stroke(o), x, y) => {
                let mut missing = o.base.clone();
                missing.width = 0.;
                missing.opacity = 0.;
                let x = match x {
                    Some(AppearanceItem::Stroke(x)) => &x.base,
                    _ => &missing,
                };
                let y = match y {
                    Some(AppearanceItem::Stroke(y)) => &y.base,
                    _ => &missing,
                };
                o.base = stroke(x, y, t);
            }
        }
    }
    photocraft_vector::appearance::validate(&out).map_err(error)?;
    Ok(out)
}
pub fn distance(a: &Layer, b: &Layer) -> f64 {
    fn colors(layer: &Layer) -> Vec<[f64; 3]> {
        let mut colors = Vec::new();
        for item in stack(layer).items {
            let p = match item {
                AppearanceItem::Mesh(mesh) => {
                    colors.extend(mesh.paint.points.iter().map(|p| oklab(photocraft_doc::Color::rgba(p.rgba[0], p.rgba[1], p.rgba[2], p.rgba[3]))));
                    continue;
                }
                AppearanceItem::Fill(f) => f.paint,
                AppearanceItem::Stroke(s) => s.base.paint,
            };
            match p {
                Fill::Solid(c) => colors.push(oklab(c)),
                Fill::Gradient { stops, .. } => colors.extend(stops.into_iter().map(|s| oklab(s.1))),
                _ => {}
            }
        }
        colors
    }
    // Groups compare recursively, in stacking order, using the same perceptual metric.
    let mut delta = 0f64;
    let (ca, cb) = (colors(a), colors(b));
    for i in 0..ca.len().max(cb.len()) {
        if let (Some(a), Some(b)) = (ca.get(i.min(ca.len().saturating_sub(1))), cb.get(i.min(cb.len().saturating_sub(1)))) {
            delta = delta.max(a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt());
        } else {
            delta = delta.max(1.);
        }
    }
    if let (LayerContent::Group(a), LayerContent::Group(b)) = (&a.content, &b.content) {
        for (a, b) in a.children.iter().zip(&b.children) {
            delta = delta.max(distance(a, b));
        }
    }
    delta
}
