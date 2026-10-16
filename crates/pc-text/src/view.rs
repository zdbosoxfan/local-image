//! Type display projection through the existing shaper and glyph rasterizer.
//! Drawing semantics adapted from pc-text/src/render.rs in Local Image's PhotoCraft
//! import c18d18cf613422fe08d31001e1f803cf8ef7dd68 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the PhotoCraft contributors.
use photocraft_color::PixelFormat;
use photocraft_doc::TextLayer;
use photocraft_geom::{Affine, Rect};
use photocraft_raster::Surface;

/// Retain text-space layout/DPI and apply display scale after the document transform.
/// Glyph raster buffers are restricted to visible tile bands, including warp and faux styles.
pub fn render_text_view(
    engine: &mut super::TextEngine,
    layer: &TextLayer,
    dpi: f32,
    format: PixelFormat,
    clip: Rect,
    scale: f64,
) -> Result<Surface, &'static str> {
    if !scale.is_finite() || !(1.0..=64.0).contains(&scale) || !dpi.is_finite() || dpi <= 0.0 || layer.transform.m.iter().any(|v| !v.is_finite()) {
        return Err("invalid type view transform or DPI");
    }
    let transform = Affine::scale(scale).mul(&layer.transform);
    let limit = f64::from(i32::MAX / 4);
    if transform.m.iter().any(|v| !v.is_finite() || v.abs() > limit) {
        return Err("type view transform exceeds coordinate budget");
    }
    let layout = engine.layout(layer, dpi);
    render_layout_view(&layout, layer, format, clip, scale)
}

/// Render an already composed native layout in bounded device-space tiles.
pub fn render_layout_view(layout: &super::TextLayout, layer: &TextLayer, format: PixelFormat, clip: Rect, scale: f64) -> Result<Surface, &'static str> {
    let limit = f64::from(i32::MAX / 4);
    if !scale.is_finite() || !(1.0..=64.0).contains(&scale) {
        return Err("invalid type view scale");
    }
    let transform = Affine::scale(scale).mul(&layer.transform);
    let warp = super::render::layout_warp(layout, layer.warp.as_ref());
    let ink = super::render::ink_rect_warped(layout, &transform, warp.as_ref());
    if [ink.x0, ink.y0, ink.x1, ink.y1].iter().any(|v| f64::from(*v).abs() > limit) {
        return Err("type view ink exceeds coordinate budget");
    }
    if ink.intersect(&clip).is_empty() {
        return Ok(Surface::new(PixelFormat { alpha: true, ..format }));
    }
    let area = ink.intersect(&clip);
    if u64::from(area.width()) * u64::from(area.height()) > 16_777_216 {
        return Err("type view exceeds visible raster budget");
    }
    let format = PixelFormat { alpha: true, ..format };
    let projected = project_lines(layout, &transform, warp.as_ref())?;
    let mut out = Surface::new(format);
    for y in (area.y0..area.y1).step_by(256) {
        for x in (area.x0..area.x1).step_by(256) {
            let rect = Rect::new(x, y, (x + 256).min(area.x1), (y + 256).min(area.y1));
            let pixels = rasterize_tile(&projected, format, layer.antialias, rect);
            out.write_region(rect, &pixels);
        }
    }
    out.prune();
    Ok(out)
}

// Drawing is shared with this crate's render.rs: the existing skrifa outlines and
// exact-area Coverage; no new shaping or vector rasterizer.
use super::layout::TextLayout;
use super::raster::{Coverage, LineSink, Xform};
use super::render::{color_in, draw};
use super::warp::Warp;
use photocraft_color::Color;
use photocraft_doc::text::AntiAlias;
#[derive(Clone, Copy)]
struct ProjectedLine {
    a: (f64, f64),
    b: (f64, f64),
}
#[derive(Default)]
struct LineCache {
    lines: Vec<ProjectedLine>,
    exceeded: bool,
}
impl LineSink for LineCache {
    fn line(&mut self, a: (f64, f64), b: (f64, f64)) {
        if self.lines.len() >= 1_048_576 {
            self.exceeded = true;
            return;
        }
        self.lines.push(ProjectedLine { a, b });
    }
}
struct ProjectedColor {
    color: Color,
    lines: Vec<ProjectedLine>,
}
fn project_lines(layout: &TextLayout, transform: &Affine, warp: Option<&Warp>) -> Result<Vec<ProjectedColor>, &'static str> {
    let groups = super::render::paint_runs(layout);
    let mut part = layout.clone();
    let mut projected = Vec::new();
    let mut total = 0usize;
    for (color, glyphs, decorations) in groups {
        part.glyphs = glyphs;
        part.decorations = decorations;
        let mut cache = LineCache::default();
        draw(&part, &Xform(transform.m), &mut cache, Some(&color), warp);
        total += cache.lines.len();
        if cache.exceeded || total > 1_048_576 {
            return Err("type view exceeds projected outline budget");
        }
        projected.push(ProjectedColor { color, lines: cache.lines });
    }
    Ok(projected)
}
fn rasterize_tile(projected: &[ProjectedColor], format: PixelFormat, antialias: AntiAlias, rect: Rect) -> Vec<f32> {
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    let n = format.mode.color_channels();
    let stride = n + 1;
    let mut acc = vec![0.0; w * h * stride];
    for group in projected {
        let c = group.color;
        let mut cov = Coverage::new(w, h);
        for line in &group.lines {
            if line.a.1.max(line.b.1) < f64::from(rect.y0) || line.a.1.min(line.b.1) > f64::from(rect.y1) {
                continue;
            }
            let local = |p: (f64, f64)| (p.0 - f64::from(rect.x0), p.1 - f64::from(rect.y0));
            cov.line(local(line.a), local(line.b));
        }
        let cov = cov.finish();
        let comps = color_in(&format, &c);
        let alpha = c.alpha.clamp(0.0, 1.0);
        for (i, cv) in cov.into_iter().enumerate() {
            let cv = if antialias == AntiAlias::None { if cv >= 0.5 { 1.0 } else { 0.0 } } else { cv };
            let a = cv * alpha;
            if a <= 0.0 {
                continue;
            }
            let px = &mut acc[i * stride..(i + 1) * stride];
            for j in 0..n {
                px[j] = comps[j] * a + px[j] * (1.0 - a);
            }
            px[n] = a + px[n] * (1.0 - a);
        }
    }
    for px in acc.chunks_exact_mut(stride) {
        let a = px[n];
        if a > 0.0 {
            for v in &mut px[..n] {
                *v = (*v / a).clamp(0.0, 1.0);
            }
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaled_type_matches_shaped_document_with_post_transform_and_preserves_source() {
        let mut engine = super::super::TextEngine::new();
        let text = TextLayer { text: "ABC".into(), font_family: "Inter".into(), size_pt: 12.0, transform: Affine::translate(4.0, 20.0), ..Default::default() };
        let before = text.clone();
        let clip = Rect::new(0, 0, 512, 256);
        let actual = render_text_view(&mut engine, &text, 72.0, PixelFormat::RGBA8, clip, 4.0).unwrap();
        let mut expected = text.clone();
        expected.transform = Affine::scale(4.0).mul(&text.transform);
        let (_, rendered) = engine.render(&expected, 72.0, PixelFormat::RGBA8);
        assert_eq!(actual.read_region(clip), rendered.surface.read_region(clip));
        assert!(actual.tile_count() > 0);
        assert_eq!(text, before);
        let mut invalid = text.clone();
        invalid.transform = Affine::scale(f64::MAX);
        assert!(render_text_view(&mut engine, &invalid, 72.0, PixelFormat::RGBA8, clip, 4.0).is_err());
    }
    #[test]
    fn bugfix_vector_overlapping_native_text_preserves_run_paint_order() {
        use photocraft_doc::{
            Color,
            text::{AntiAlias, TextRun},
        };
        for alpha in [1., 0.5] {
            let mut engine = super::super::TextEngine::new();
            let mut layer =
                TextLayer { text: "HHH".into(), size_pt: 24., transform: Affine::translate(0., 30.), antialias: AntiAlias::None, ..Default::default() };
            let initial = engine.layout(&layer, 72.);
            let advance = initial.glyphs[1].x - initial.glyphs[0].x;
            let mut red = initial.styles[0].clone();
            red.color = Color::rgba(1., 0., 0., alpha);
            red.tracking = -advance / red.size_pt * 1000.;
            let mut blue = red.clone();
            blue.color = Color::rgba(0., 0., 1., alpha);
            layer.runs = vec![TextRun { len: 1, style: red.clone() }, TextRun { len: 1, style: blue }, TextRun { len: 1, style: red }];
            let layout = engine.layout(&layer, 72.);
            assert!(layout.glyphs.iter().all(|g| (g.x - layout.glyphs[0].x).abs() < 0.05));
            let clip = Rect::new(0, 0, 64, 64);
            let view = render_layout_view(&layout, &layer, PixelFormat::RGBA32F, clip, 1.).unwrap();
            let native = super::super::render::rasterize(&layout, &layer.transform, PixelFormat::RGBA32F, layer.antialias).surface;
            struct Line;
            impl super::super::on_path::Curve for Line {
                fn length(&self) -> f64 {
                    200.
                }
                fn closed(&self) -> bool {
                    false
                }
                fn sample(&self, s: f64) -> (photocraft_geom::Point, photocraft_geom::Point) {
                    (photocraft_geom::Point::new(s, 30.), photocraft_geom::Point::new(1., 0.))
                }
            }
            let options =
                photocraft_doc::vector_design::text::PathText { path: Default::default(), start: 0., end: 1., flip: false, align: Default::default() };
            let on_path = super::super::on_path::place(layout.clone(), &Line, &options, photocraft_doc::text::TextAlign::Left).unwrap();
            let on_path = super::super::on_path::rasterize(&on_path, &Affine::IDENTITY, PixelFormat::RGBA32F, layer.antialias, clip).unwrap().surface;
            for surface in [view, native, on_path] {
                let pixels = surface.read_region(clip);
                let solid: Vec<_> = pixels.as_chunks::<4>().0.iter().filter(|p| p[3] > if alpha == 1. { 0.999 } else { 0.87 }).collect();
                assert!(!solid.is_empty());
                for p in solid {
                    assert!((p[0] - if alpha == 1. { 1. } else { 5. / 7. }).abs() < 0.001, "{p:?}");
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    #[test]
    fn dsfix_overlapping_glyphs_composite_in_layout_order_and_cull_distant_items() {
        let mut engine = super::super::TextEngine::new();
        let text = TextLayer { text: "HHH".into(), size_pt: 24., ..Default::default() };
        let mut layout = engine.layout(&text, 72.);
        layout.styles[0].color = Color::rgb(1., 0., 0.);
        let mut blue = layout.styles[0].clone();
        blue.color = Color::rgb(0., 0., 1.);
        layout.styles.push(blue);
        let x = layout.glyphs[0].x;
        for (i, glyph) in layout.glyphs.iter_mut().enumerate() {
            glyph.x = x;
            glyph.style = u32::from(i == 1);
        }
        let transform = Affine::translate(0., 30.);
        let projected = project_lines(&layout, &transform, None).unwrap();
        let far = Rect::new(256, 0, 512, 256);
        assert!(projected.iter().flat_map(|group| &group.lines).all(|line| line.a.0.max(line.b.0) < f64::from(far.x0)));
        let layer = TextLayer { transform, ..text };
        // The current view rejects the complete clipped ink bounds before it
        // projects outlines; no font opening/flattening occurs per tile.
        assert!(render_layout_view(&layout, &layer, PixelFormat::RGBA32F, far, 1.).unwrap().content_bounds().is_empty());
        let pixels = rasterize_tile(&projected, PixelFormat::RGBA32F, AntiAlias::None, Rect::new(0, 0, 64, 64));
        let solid: Vec<_> = pixels.as_chunks::<4>().0.iter().filter(|p| p[3] > 0.999).collect();
        assert!(!solid.is_empty());
        assert!(solid.iter().all(|p| p[0] > 0.999 && p[2] < 0.001), "the final red glyph must cover the middle blue glyph");
    }
}
