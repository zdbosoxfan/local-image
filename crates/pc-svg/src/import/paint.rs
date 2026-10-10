//! SVG paint mapping. Native editable mirrors use Fill; normalized SVG paints also
//! prepare a faithful cache through pc-vector coverage (no second geometry rasterizer).
//! Adapted from VectorCraft crates/svg/src/import.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
use super::{geometry::affine, warn};
use photocraft_doc::{Affine, Color, Fill, GradientStyle, ShapeStroke, vector_design::exchange::VectorWarning};
use photocraft_geom::Point;

pub(super) fn mirror(p: &usvg::Paint, transform: Affine, opacity: f32, bounds: (f64, f64, f64, f64), warnings: &mut Vec<VectorWarning>, id: &str) -> Fill {
    let (base, style, a, b) = match p {
        usvg::Paint::Color(c) => {
            let mut c = Color::rgb(f32::from(c.red) / 255.0, f32::from(c.green) / 255.0, f32::from(c.blue) / 255.0);
            c.alpha = opacity;
            return Fill::Solid(c);
        }
        usvg::Paint::LinearGradient(g) => {
            (&**g as &usvg::BaseGradient, GradientStyle::Linear, Point::new(g.x1().into(), g.y1().into()), Point::new(g.x2().into(), g.y2().into()))
        }
        usvg::Paint::RadialGradient(g) => (
            &**g as &usvg::BaseGradient,
            GradientStyle::Radial,
            Point::new(g.cx().into(), g.cy().into()),
            Point::new(f64::from(g.cx() + g.r().get()), g.cy().into()),
        ),
        usvg::Paint::Pattern(_) => {
            warn(warnings, "pattern", id, "Pattern paint could not be preserved.");
            return Fill::Solid(Color::TRANSPARENT);
        }
    };
    let m = transform.mul(&affine(base.transform()));
    let (a, b) = (m.apply(a), m.apply(b));
    let (w, h) = ((bounds.2 - bounds.0).max(1.0), (bounds.3 - bounds.1).max(1.0));
    let angle = -(b.y - a.y).atan2(b.x - a.x);
    let chord = (w / angle.cos().abs().max(1e-6)).min(h / angle.sin().abs().max(1e-6)).max(1.0);
    let (center, denom) = if style == GradientStyle::Linear {
        (Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0), chord)
    } else {
        (a, ((angle.cos() * w).powi(2) + (angle.sin() * h).powi(2)).sqrt() / 2.0)
    };
    let stops = base
        .stops()
        .iter()
        .map(|s| {
            let c = s.color();
            let mut c = Color::rgb(f32::from(c.red) / 255.0, f32::from(c.green) / 255.0, f32::from(c.blue) / 255.0);
            c.alpha = s.opacity().get() * opacity;
            (s.offset().get(), c)
        })
        .collect();
    // Fill has no SVG affine/spread/focal geometry. Report the mirror's limitation even
    // when a prepared cache matches the original; editing must not silently lose fidelity.
    warn(warnings, "gradient-projection", id, "SVG gradients remain editable, but some SVG geometry may be simplified.");
    Fill::Gradient {
        stops,
        angle: angle.to_degrees() as f32,
        scale: ((b.x - a.x).hypot(b.y - a.y) / denom.max(1e-6)) as f32,
        style,
        reverse: false,
        opacity_stops: vec![],
        midpoints: vec![],
        offset: (((center.x - (bounds.0 + bounds.2) / 2.0) / w) as f32, ((center.y - (bounds.1 + bounds.3) / 2.0) / h) as f32),
        dither: false,
        align: true,
    }
}

pub(super) fn stroke(s: &usvg::Stroke, transform: Affine, bounds: (f64, f64, f64, f64), warnings: &mut Vec<VectorWarning>, id: &str) -> ShapeStroke {
    let width = s.width().get();
    let scale = transform.determinant().abs().sqrt() as f32;
    ShapeStroke {
        width: width * scale,
        paint: mirror(s.paint(), transform, 1.0, bounds, warnings, id),
        opacity: s.opacity().get(),
        cap: match s.linecap() {
            usvg::LineCap::Butt => photocraft_doc::LineCap::Butt,
            usvg::LineCap::Round => photocraft_doc::LineCap::Round,
            usvg::LineCap::Square => photocraft_doc::LineCap::Square,
        },
        join: match s.linejoin() {
            usvg::LineJoin::Round => photocraft_doc::LineJoin::Round,
            usvg::LineJoin::Bevel => photocraft_doc::LineJoin::Bevel,
            _ => photocraft_doc::LineJoin::Miter,
        },
        miter_limit: s.miterlimit().get(),
        dashes: s.dasharray().unwrap_or(&[]).iter().map(|d| d / width).collect(),
        dash_offset: s.dashoffset() / width,
        ..Default::default()
    }
}

pub(super) fn sample(p: &usvg::Paint, transform: Affine, point: Point) -> [f32; 4] {
    let (base, mut t) = match p {
        usvg::Paint::Color(c) => return [f32::from(c.red) / 255.0, f32::from(c.green) / 255.0, f32::from(c.blue) / 255.0, 1.0],
        usvg::Paint::Pattern(_) => return [0.0; 4],
        usvg::Paint::LinearGradient(g) => {
            let Some(inv) = transform.mul(&affine(g.transform())).inverse() else { return [0.0; 4] };
            let p = inv.apply(point);
            let (dx, dy) = (f64::from(g.x2() - g.x1()), f64::from(g.y2() - g.y1()));
            (&**g as &usvg::BaseGradient, ((p.x - f64::from(g.x1())) * dx + (p.y - f64::from(g.y1())) * dy) / (dx * dx + dy * dy).max(1e-12))
        }
        usvg::Paint::RadialGradient(g) => {
            let Some(inv) = transform.mul(&affine(g.transform())).inverse() else { return [0.0; 4] };
            let p = inv.apply(point);
            let (cx, cy, r) = (f64::from(g.cx()), f64::from(g.cy()), f64::from(g.r().get()));
            let (mut fx, mut fy) = (f64::from(g.fx()), f64::from(g.fy()));
            let distance = (fx - cx).hypot(fy - cy);
            if distance >= r {
                let k = r * 0.999_999 / distance;
                fx = cx + (fx - cx) * k;
                fy = cy + (fy - cy) * k;
            }
            let (dx, dy) = (p.x - fx, p.y - fy);
            let aa = dx * dx + dy * dy;
            let bb = 2.0 * ((fx - cx) * dx + (fy - cy) * dy);
            let cc = (fx - cx).powi(2) + (fy - cy).powi(2) - r * r;
            let root = if aa < 1e-18 { f64::INFINITY } else { (-bb + (bb * bb - 4.0 * aa * cc).max(0.0).sqrt()) / (2.0 * aa) };
            (&**g as &usvg::BaseGradient, 1.0 / root)
        }
    };
    t = match base.spread_method() {
        usvg::SpreadMethod::Pad => t.clamp(0.0, 1.0),
        usvg::SpreadMethod::Repeat => t.rem_euclid(1.0),
        usvg::SpreadMethod::Reflect => 1.0 - (t.rem_euclid(2.0) - 1.0).abs(),
    };
    let rgba = |s: &usvg::Stop| {
        let c = s.color();
        [f32::from(c.red) / 255.0, f32::from(c.green) / 255.0, f32::from(c.blue) / 255.0, s.opacity().get()]
    };
    let stops = base.stops();
    let Some(first) = stops.first() else { return [0.0; 4] };
    if t <= f64::from(first.offset().get()) {
        return rgba(first);
    }
    for pair in stops.windows(2) {
        if t < f64::from(pair[1].offset().get()) {
            let a = rgba(&pair[0]);
            let b = rgba(&pair[1]);
            let k = ((t as f32 - pair[0].offset().get()) / (pair[1].offset().get() - pair[0].offset().get()).max(1e-12)).clamp(0.0, 1.0);
            // SVG/resvg interpolates straight sRGB colour and stop opacity
            // separately, then premultiplies for compositing.
            return std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k);
        }
    }
    stops.last().map(rgba).unwrap_or([0.0; 4])
}
