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

// Drawing semantics adapted from this crate's render.rs. Both paths use the existing
// skrifa outlines and exact-area Coverage; no new shaping or vector rasterizer.
use super::layout::{GlyphOrient, PlacedGlyph, TextLayout};
use super::raster::{Coverage, LineSink, Pen, Xform, rect_to};
use super::render::{FAUX_BOLD_RADIUS, FAUX_ITALIC_DEG};
use super::warp::Warp;
use photocraft_color::Color;
use photocraft_doc::text::AntiAlias;
use skrifa::instance::{LocationRef, NormalizedCoord, Size as FontSize};
use skrifa::outline::DrawSettings;
use skrifa::{GlyphId, MetadataProvider};
/// Bends text-space lines with a [`Warp`], then maps them through `post` into `inner`.
struct WarpSink<'a, S: LineSink> {
    inner: &'a mut S,
    warp: &'a Warp,
    post: Xform,
}

impl<S: LineSink> LineSink for WarpSink<'_, S> {
    fn line(&mut self, p0: (f64, f64), p1: (f64, f64)) {
        let len = ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt();
        let n = ((len / self.warp.max_segment()).ceil() as usize).clamp(1, 256);
        let map = |t: f64| {
            let (x, y) = self.warp.apply(p0.0 + (p1.0 - p0.0) * t, p0.1 + (p1.1 - p0.1) * t);
            self.post.apply(x, y)
        };
        let mut a = map(0.0);
        for i in 1..=n {
            let b = map(i as f64 / n as f64);
            self.inner.line(a, b);
            a = b;
        }
    }
}

/// Draws every glyph and decoration of `layout` whose style colour is `color` (or all of them
/// when `color` is `None`) into `sink`, through `transform` (text space → sink space), bending
/// the outlines with `warp` first (in text space).
fn draw(layout: &TextLayout, transform: &Xform, sink: &mut impl LineSink, only_color: Option<&Color>, warp: Option<&Warp>) {
    for g in &layout.glyphs {
        let st = &layout.styles[g.style as usize];
        if only_color.is_some_and(|c| c != &st.color) {
            continue;
        }
        let face = &layout.faces[g.face as usize];
        let Ok(font) = skrifa::FontRef::from_index(face.font.data.as_ref(), face.font.index) else {
            continue;
        };
        let Some(outline) = font.outline_glyphs().get(GlyphId::new(g.id)) else {
            continue;
        };
        let coords: Vec<NormalizedCoord> = face.coords.iter().map(|&c| NormalizedCoord::from_bits(c)).collect();
        let glyph = glyph_xform(layout, g, face.skew_deg);
        let bold = st.faux_bold || face.embolden;
        let r = (face.size_px * FAUX_BOLD_RADIUS) as f64;
        let offsets: &[(f64, f64)] =
            if bold { &[(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (0.7, 0.7), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7)] } else { &[(0.0, 0.0)] };
        for &(ox, oy) in offsets {
            let local = Xform([1.0, 0.0, 0.0, 1.0, ox * r, oy * r]).mul(&glyph);
            let settings = DrawSettings::unhinted(FontSize::new(face.size_px), LocationRef::new(&coords));
            match warp {
                Some(w) => {
                    let mut ws = WarpSink { inner: &mut *sink, warp: w, post: *transform };
                    let mut pen = Pen::new(&mut ws, local);
                    if outline.draw(settings, &mut pen).is_ok() {
                        skrifa::outline::OutlinePen::close(&mut pen);
                    }
                }
                None => {
                    let mut pen = Pen::new(&mut *sink, transform.mul(&local));
                    if outline.draw(settings, &mut pen).is_ok() {
                        skrifa::outline::OutlinePen::close(&mut pen);
                    }
                }
            }
        }
    }
    for d in &layout.decorations {
        let st = &layout.styles[d.style as usize];
        if only_color.is_some_and(|c| c != &st.color) {
            continue;
        }
        match warp {
            Some(w) => {
                let mut ws = WarpSink { inner: &mut *sink, warp: w, post: *transform };
                rect_to(&mut ws, &Xform::IDENTITY, d.x0 as f64, d.y0 as f64, d.x1 as f64, d.y1 as f64);
            }
            None => rect_to(sink, transform, d.x0 as f64, d.y0 as f64, d.x1 as f64, d.y1 as f64),
        }
    }
}

/// Font units (scaled to px, y up) → text space for a placed glyph: scales, faux italic,
/// baseline shift, and the vertical-type orientation.
fn glyph_xform(layout: &TextLayout, g: &PlacedGlyph, face_skew_deg: f32) -> Xform {
    let Some(st) = layout.styles.get(g.style as usize) else { return Xform([1.0, 0.0, 0.0, -1.0, g.x as f64, g.y as f64]) };
    let hs = if st.horizontal_scale > 0.0 { st.horizontal_scale } else { 1.0 } as f64;
    let vs = if st.vertical_scale > 0.0 { st.vertical_scale } else { 1.0 } as f64;
    let skew_deg = if st.faux_italic { FAUX_ITALIC_DEG } else { 0.0 } + face_skew_deg;
    let skew = (skew_deg as f64).to_radians().tan() * vs;
    let shift = (st.baseline_shift_pt * layout.px_per_pt) as f64;
    let (x, y) = (g.x as f64, g.y as f64);
    match g.orient {
        GlyphOrient::Horizontal => Xform([hs, 0.0, skew, -vs, x, y - shift]),
        // Baseline shift moves upright glyphs in vertical type to the right.
        GlyphOrient::Upright => Xform([hs, 0.0, skew, -vs, x + shift, y]),
        // 90° clockwise: the glyph's up direction (and its baseline shift) points right.
        GlyphOrient::Rotated => Xform([0.0, 1.0, -1.0, 0.0, x, y]).mul(&Xform([hs, 0.0, skew, -vs, 0.0, -shift])),
    }
}

/// Colour components in `format`'s model (without alpha).
fn color_in(format: &PixelFormat, c: &Color) -> Vec<f32> {
    let n = format.mode.color_channels();
    if c.mode == format.mode {
        c.c[..n].to_vec()
    } else {
        let [r, g, b] = c.to_rgb();
        let mut v = photocraft_raster::from_rgba(&PixelFormat { alpha: false, ..*format }, [r, g, b, 1.0]);
        v.truncate(n);
        v
    }
}

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
    let key = |c: Color| (c.mode as u8, c.c.map(f32::to_bits), c.alpha.to_bits());
    let mut colors = Vec::new();
    let mut slots = std::collections::HashMap::new();
    for style in &layout.styles {
        if let std::collections::hash_map::Entry::Vacant(entry) = slots.entry(key(style.color)) {
            entry.insert(colors.len());
            colors.push(style.color);
        }
    }
    let mut groups = vec![Vec::new(); colors.len()];
    for glyph in &layout.glyphs {
        groups[slots[&key(layout.styles[glyph.style as usize].color)]].push(*glyph);
    }
    let mut part = layout.clone();
    let mut projected = Vec::new();
    let mut total = 0usize;
    for (color, glyphs) in colors.into_iter().zip(groups) {
        part.glyphs = glyphs;
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
}
