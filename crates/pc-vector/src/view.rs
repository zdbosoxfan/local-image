//! Detached display-space shape projection. Uses the existing exact-area renderer.
use photocraft_doc::{Fill, ShapeLayer};
use photocraft_geom::{Affine, Rect};

/// Project a legacy shape with one uniform positive scale. The caller supplies a global
/// canvas for paint layout and a separate clip for bounds-limited tile rasterization.
pub fn project_shape(shape: &ShapeLayer, scale: f64) -> Result<ShapeLayer, &'static str> {
    if !scale.is_finite() || !(1.0..=64.0).contains(&scale) {
        return Err("display scale must be finite and within 1..=64");
    }
    if shape.path.subpaths.iter().flat_map(|s| &s.knots).any(|k| {
        [k.anchor, k.in_ctrl, k.out_ctrl].iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) * scale > f64::from(i32::MAX / 4))
    }) {
        return Err("invalid display geometry");
    }
    let mut out = shape.clone();
    out.path = shape.path.transform(&Affine::scale(scale));
    out.live = None;
    out.cache = None;
    out.psd_raw = None;
    if let Some(stroke) = &mut out.stroke {
        let width = f64::from(stroke.width) * scale;
        let limit = f64::from(i32::MAX / 4);
        if !width.is_finite()
            || width < 0.0
            || !stroke.miter_limit.is_finite()
            || width * f64::from(stroke.miter_limit.max(1.0)) > limit
            || !stroke.dash_offset.is_finite()
            || f64::from(stroke.dash_offset).abs() * width > limit
            || stroke.dashes.iter().any(|d| !d.is_finite() || *d < 0.0 || f64::from(*d) * width > limit)
        {
            return Err("invalid display stroke");
        }
        stroke.width *= scale as f32;
        scale_paint(&mut stroke.paint, scale);
    }
    if let Some(fill) = &mut out.fill {
        scale_paint(fill, scale);
    }
    Ok(out)
}

/// Pattern samples stay in document space. Gradient frames follow projected geometry.
pub fn scale_paint(fill: &mut Fill, scale: f64) {
    if let Fill::Pattern { scale: size, phase, .. } = fill {
        *size *= scale as f32;
        phase.0 *= scale as f32;
        phase.1 *= scale as f32;
    }
}

pub fn render_shape_view(
    shape: &ShapeLayer,
    format: photocraft_color::PixelFormat,
    canvas: Rect,
    clip: Rect,
    scale: f64,
) -> Result<photocraft_raster::Surface, &'static str> {
    if u64::from(clip.width()) * u64::from(clip.height()) > 16_777_216 {
        return Err("shape view exceeds visible raster budget");
    }
    if shape.fill.as_ref().is_some_and(|f| matches!(f, Fill::Pattern { .. })) || shape.stroke.as_ref().is_some_and(|s| matches!(s.paint, Fill::Pattern { .. }))
    {
        return Err("pattern shape requires enhanced paint projection");
    }
    let projected = project_shape(shape, scale)?;
    Ok(super::CompiledShape::new(&projected, super::DEFAULT_TOLERANCE, canvas).render(format, clip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::{Color, Path, ShapeStroke, Subpath};
    #[test]
    fn scaled_hairline_800_percent_has_eight_full_columns() {
        let sh = ShapeLayer {
            path: Path::new(vec![Subpath::polyline(&[(4.5, 1.0), (4.5, 8.0)])]),
            stroke: Some(ShapeStroke { width: 1.0, ..Default::default() }),
            ..Default::default()
        };
        let out = render_shape_view(&sh, photocraft_color::PixelFormat::RGBA8, Rect::new(0, 0, 80, 80), Rect::new(0, 0, 80, 80), 8.0).unwrap();
        for x in 0..80 {
            assert_eq!(out.pixel(x, 32)[3], if (32..40).contains(&x) { 1.0 } else { 0.0 }, "column {x}");
        }
        assert!(sh.cache.is_none());
    }
    #[test]
    fn scaled_shape_tiles_share_global_geometry_and_paint_origin() {
        let sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(1.25, 1.75), (15.5, 2.0), (11.5, 15.5)])]),
            fill: Some(Fill::Solid(Color::BLACK)),
            ..Default::default()
        };
        let canvas = Rect::new(0, 0, 128, 128);
        let all = render_shape_view(&sh, photocraft_color::PixelFormat::RGBA8, canvas, canvas, 8.0).unwrap();
        for y in [0, 64] {
            for x in [0, 64] {
                let r = Rect::from_xywh(x, y, 64, 64);
                let t = render_shape_view(&sh, photocraft_color::PixelFormat::RGBA8, canvas, r, 8.0).unwrap();
                assert_eq!(all.read_region(r), t.read_region(r));
            }
        }
    }
    #[test]
    fn scaled_shape_rejects_nonfinite_scale_and_geometry() {
        for scale in [f64::NAN, f64::INFINITY, 0.0, 65.0] {
            assert!(project_shape(&ShapeLayer::default(), scale).is_err());
        }
        let mut sh = ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(1.0, 1.0), (2.0, 3.0)])]), ..Default::default() };
        sh.path.subpaths[0].knots[0].anchor.x = f64::NAN;
        assert!(project_shape(&sh, 2.0).is_err());
        assert!(project_shape(&ShapeLayer { stroke: Some(ShapeStroke { width: f32::MAX, ..Default::default() }), ..Default::default() }, 64.0).is_err());
        let huge = Rect::new(0, 0, 6000, 4000);
        assert!(render_shape_view(&ShapeLayer::default(), photocraft_color::PixelFormat::RGBA8, huge, huge, 1.0).is_err());
    }
}
