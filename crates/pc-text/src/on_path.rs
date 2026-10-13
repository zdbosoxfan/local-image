//! Cluster placement adapted from VectorCraft `crates/text/src/layout.rs` (on_path),
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Upstream license: MIT OR Apache-2.0; adaptation/glue: GPL-3.0-or-later.
//! SPDX-License-Identifier: GPL-3.0-or-later
//! See licenses/vectorcraft-*. Geometry is supplied by V1; shaping stays in pc-text.
//! SVG 2 method=align uses the advance midpoint's tangent. All glyphs in a cluster
//! share a rigid map, preserving marks, ligatures, bidi order and vertical alternates.

use super::{TextEngine, layout, raster, render};
use layout::TextLayout;
use photocraft_color::{Color, PixelFormat};
use photocraft_doc::{
    TextLayer,
    text::{AntiAlias, TextAlign, TextShape},
    vector_design::text::{PathText, PathTextAlign},
};
use photocraft_geom::{Affine, Point, Rect};
use photocraft_raster::Surface;
use render::PathEl;
use std::ops::Range;

/// No geometry dependency is introduced in the text crate.
pub trait Curve {
    fn length(&self) -> f64;
    fn closed(&self) -> bool;
    /// Position and unit tangent; closed curves wrap, open curves clamp.
    fn sample(&self, distance: f64) -> (Point, Point);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub &'static str);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct Cluster {
    pub source: Range<usize>,
    pub source_index: usize,
    pub advance: f64,
    /// Distance travelled within the bracket span (before midpoint placement).
    pub distance: f64,
    /// Midpoint on the curve, before baseline alignment.
    pub anchor: Point,
    pub angle: f64,
    /// Existing text-space positions/outlines -> path text space.
    pub transform: Affine,
    pub visible: bool,
    pub rtl: bool,
}

#[derive(Clone, Debug)]
pub struct PathLayout {
    pub shaped: TextLayout,
    pub clusters: Vec<Cluster>,
    /// One cluster index for each existing glyph; no glyph is re-shaped or re-ordered.
    pub glyph_clusters: Vec<usize>,
    pub advance: f64,
    pub used: f64,
    pub available: f64,
    pub overflow: bool,
    pub start: Point,
    pub end: Point,
}
impl PathLayout {
    /// Byte caret nearest to a point in path text space. Logical bidi edges come
    /// from pc-text clusters; only their geometry is projected.
    pub fn hit_test(&self, point: Point) -> usize {
        self.hit_test_distance(point).0
    }
    /// Distance to the projected logical cluster boxes, in text-space pixels.
    pub fn hit_test_distance(&self, point: Point) -> (usize, f64) {
        self.clusters
            .iter()
            .filter(|c| c.visible)
            .filter_map(|c| {
                let source = self.shaped.clusters.get(c.source_index)?;
                let line = self.shaped.lines.get(source.line)?;
                let p = c.transform.inverse()?.apply(point);
                let (x, y) = self.shaped.to_line(p.x as f32, p.y as f32);
                let distance = f64::from((source.x - x).max(0.).max(x - source.x - source.advance)).powi(2)
                    + f64::from((line.baseline - line.ascent - y).max(0.).max(y - line.baseline - line.descent)).powi(2);
                let before = x < source.x + source.advance * 0.5;
                let caret = if before != c.rtl { c.source.start } else { c.source.end };
                Some((distance, caret))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or((0, f64::INFINITY), |(distance, caret)| (caret, distance.sqrt()))
    }
    pub fn caret_segment(&self, byte: usize) -> Option<[Point; 2]> {
        let c =
            self.clusters.iter().find(|c| c.visible && c.source.start == byte).or_else(|| self.clusters.iter().find(|c| c.visible && c.source.end == byte))?;
        Some(self.shaped.caret_segment(byte).map(|(x, y)| c.transform.apply(Point::new(f64::from(x), f64::from(y)))))
    }
}

fn finite(v: f64) -> bool {
    v.is_finite() && v.abs() <= 1_000_000.0
}

/// Project an already shaped layout. Equal brackets mean an empty span; on a
/// closed curve start > end crosses the seam once. Flip traverses the same span
/// from its end, rotating the type by pi rather than mirroring glyph outlines.
pub fn place(shaped: TextLayout, curve: &impl Curve, options: &PathText, align: TextAlign) -> Result<PathLayout, Error> {
    let length = curve.length();
    if !finite(length) || length <= 1e-9 || ![options.start, options.end].into_iter().all(|v| v.is_finite() && (0.0..=1.0).contains(&v)) {
        return Err(Error("invalid path length or bracket offset"));
    }
    if !curve.closed() && options.start > options.end {
        return Err(Error("open path brackets are reversed"));
    }
    if shaped.glyphs.len() > 65_536 || shaped.clusters.len() > 65_536 || shaped.lines.len() > 4096 {
        return Err(Error("path text exceeds the glyph budget"));
    }
    let span = if options.start > options.end { 1.0 - options.start + options.end } else { options.end - options.start };
    let available = span * length;
    let mut by_line = vec![Vec::new(); shaped.lines.len()];
    for (index, c) in shaped.clusters.iter().enumerate() {
        if !finite(f64::from(c.x)) || !finite(f64::from(c.advance)) || c.advance < 0.0 {
            return Err(Error("invalid cluster metrics"));
        }
        by_line.get_mut(c.line).ok_or(Error("cluster has no line"))?.push(index);
    }
    for indices in &mut by_line {
        indices.sort_by(|&a, &b| shaped.clusters[a].x.total_cmp(&shaped.clusters[b].x));
    }
    let widths: Vec<f64> = shaped
        .lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            // In f64, like the cluster distances below: summing the f32 metrics in f32 rounds
            // differently, which left the last cluster of a right-aligned line a few ulps past
            // the span and hid it as overflow.
            let end = by_line[i].iter().map(|&c| f64::from(shaped.clusters[c].x) + f64::from(shaped.clusters[c].advance)).fold(f64::from(l.x1), f64::max);
            end - f64::from(l.x0)
        })
        .collect();
    if widths.iter().any(|w| !finite(*w) || *w < 0.0) {
        return Err(Error("invalid shaped line extent"));
    }
    let advance: f64 = widths.iter().sum();
    if !finite(advance) {
        return Err(Error("path text advance exceeds the limit"));
    }
    let gap = (available - advance).max(0.0);
    let offset = match align {
        TextAlign::Center | TextAlign::JustifyCenter => gap * 0.5,
        TextAlign::Right | TextAlign::JustifyRight => gap,
        _ => 0.0,
    };
    let from = options.start * length;
    let sign = if options.flip { -1.0 } else { 1.0 };
    let distance = |d| if options.flip { from + available - d } else { from + d };
    let sample = |d: f64| -> Result<(Point, f64), Error> {
        let (p, t) = curve.sample(distance(d));
        if ![p.x, p.y, t.x, t.y].into_iter().all(finite) || (t.x.hypot(t.y) - 1.0).abs() > 1e-3 {
            return Err(Error("invalid curve sample"));
        }
        Ok((p, (t.y * sign).atan2(t.x * sign)))
    };
    let mut clusters = Vec::with_capacity(shaped.clusters.len());
    let mut source_to_projection = vec![0; shaped.clusters.len()];
    let mut line_start = 0.0;
    for (li, line) in shaped.lines.iter().enumerate() {
        if ![line.baseline, line.ascent, line.descent, line.x0, line.x1].into_iter().all(|v| finite(f64::from(v))) {
            return Err(Error("invalid line metrics"));
        }
        let rise = match options.align {
            PathTextAlign::Baseline => 0.0,
            PathTextAlign::Ascender => f64::from(line.ascent),
            PathTextAlign::Center => f64::from(line.ascent - line.descent) * 0.5,
            PathTextAlign::Descender => -f64::from(line.descent),
        };
        for &source_index in &by_line[li] {
            let c = &shaped.clusters[source_index];
            if !finite(f64::from(c.x)) || !finite(f64::from(c.advance)) || c.advance < 0.0 {
                return Err(Error("invalid cluster metrics"));
            }
            let d = offset + line_start + (f64::from(c.x) - f64::from(line.x0));
            let width = f64::from(c.advance);
            let (anchor, angle) = sample(d + width * 0.5)?;
            let to_line = if shaped.vertical { Affine::rotate(-std::f64::consts::FRAC_PI_2) } else { Affine::IDENTITY };
            let transform = Affine::translate(anchor.x, anchor.y)
                .mul(&Affine::rotate(angle))
                .mul(&Affine::translate(-f64::from(c.x) - width * 0.5, rise - f64::from(line.baseline)))
                .mul(&to_line);
            source_to_projection[source_index] = clusters.len();
            clusters.push(Cluster {
                source: c.range.clone(),
                source_index,
                advance: width,
                distance: d,
                anchor,
                angle,
                transform,
                visible: d >= -1e-6 && d + width <= available + 1e-6,
                rtl: c.rtl,
            });
        }
        line_start += widths[li];
    }
    let mut glyph_clusters = Vec::with_capacity(shaped.glyphs.len());
    for g in &shaped.glyphs {
        if !finite(f64::from(g.x)) || !finite(f64::from(g.y)) || shaped.faces.get(g.face as usize).is_none() || shaped.styles.get(g.style as usize).is_none() {
            return Err(Error("invalid shaped glyph"));
        }
        let source_index = g.source_cluster;
        if source_index >= shaped.clusters.len() {
            return Err(Error("glyph has no source cluster"));
        }
        let index = source_to_projection[source_index];
        glyph_clusters.push(index);
    }
    let used = clusters.iter().filter(|c| c.visible).map(|c| c.distance + c.advance - offset).fold(0.0_f64, f64::max);
    let overflow = clusters.iter().any(|c| !c.visible);
    Ok(PathLayout {
        shaped,
        clusters,
        glyph_clusters,
        advance,
        used,
        available,
        overflow,
        start: sample(if options.flip { available } else { 0.0 })?.0,
        end: sample(if options.flip { 0.0 } else { available })?.0,
    })
}

/// Model validation shared by cache, conversion and exchange adapters.
pub fn validate_text(t: &TextLayer) -> Result<(), Error> {
    if t.text.len() > 262_144 || t.runs.len() > 65_536 || t.paragraphs.len() > 65_536 {
        return Err(Error("path text exceeds the text budget"));
    }
    if !t.transform.m.into_iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.) || t.transform.inverse().is_none() {
        return Err(Error("invalid type transform"));
    }
    if t.warp.as_ref().is_some_and(|w| w.style != "warpNone") {
        return Err(Error("path text and Warp Text cannot be combined"));
    }
    for st in t.char_runs().iter().map(|r| &r.style) {
        if st.features.len() > 256 || st.variations.len() > 64 || st.font_family.len() > 4096 {
            return Err(Error("path text style exceeds the font option budget"));
        }
        let values = [st.size_pt, st.tracking, st.baseline_shift_pt, st.horizontal_scale, st.vertical_scale, st.kern, st.color.alpha];
        if values
            .into_iter()
            .chain(st.leading_pt)
            .chain(st.color.c)
            .chain(st.variations.iter().map(|v| v.value))
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
            || !(0.1..=4096.).contains(&st.size_pt)
            || !(0.001..=1000.).contains(&st.horizontal_scale)
            || !(0.001..=1000.).contains(&st.vertical_scale)
        {
            return Err(Error("invalid path text character style"));
        }
    }
    for st in t.paragraph_runs().iter().map(|r| &r.style) {
        if [st.first_line_indent_pt, st.start_indent_pt, st.end_indent_pt, st.space_before_pt, st.space_after_pt, st.auto_leading]
            .into_iter()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
        {
            return Err(Error("invalid path text paragraph style"));
        }
    }
    Ok(())
}

/// Path text has one continuous flow; paragraph shaping remains pc-text's own.
pub fn layout(engine: &mut TextEngine, layer: &TextLayer, curve: &impl Curve, options: &PathText, dpi: f32) -> Result<PathLayout, Error> {
    if !dpi.is_finite() || !(1.0..=9600.0).contains(&dpi) {
        return Err(Error("invalid document DPI"));
    }
    validate_text(layer)?;
    let mut source = layer.clone();
    source.shape = TextShape::Point;
    let align = source.paragraph_runs().first().map_or(TextAlign::Left, |r| r.style.align);
    place(engine.layout(&source, dpi), curve, options, align)
}

#[derive(Clone, Debug)]
pub struct PaintedOutline {
    pub color: Color,
    pub elements: Vec<PathEl>,
}

/// The shared projection for caches and conversion/export. Uses pc-text's glyph
/// outline service, including existing faux italic, scaling and baseline shift.
pub fn outlines(layout: &PathLayout, transform: &Affine) -> Vec<PaintedOutline> {
    let mut out = Vec::new();
    let mut one = layout.shaped.clone();
    one.glyphs.clear();
    one.decorations.clear();
    for (g, &ci) in layout.shaped.glyphs.iter().zip(&layout.glyph_clusters) {
        let Some(c) = layout.clusters.get(ci).filter(|c| c.visible) else { continue };
        one.glyphs.clear();
        one.glyphs.push(*g);
        let post = transform.mul(&c.transform);
        let st = &one.styles[g.style as usize];
        let face = &one.faces[g.face as usize];
        let bold = st.faux_bold || face.embolden;
        let radius = f64::from(face.size_px * render::FAUX_BOLD_RADIUS);
        let offsets: &[(f64, f64)] =
            if bold { &[(-1., 0.), (1., 0.), (0., -1.), (0., 1.), (0.7, 0.7), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7)] } else { &[(0., 0.)] };
        // Existing rasterizer dilates in text space, before the post-transform.
        for &(x, y) in offsets {
            let map = post.mul(&Affine::translate(x * radius, y * radius));
            for elements in render::outlines(&one, &map, None) {
                out.push(PaintedOutline { color: st.color, elements });
            }
        }
    }
    // Decorations follow the same cluster maps; split at each cluster boundary.
    for d in &layout.shaped.decorations {
        let (x0, y0) = layout.shaped.to_line(d.x0, d.y0);
        let (x1, y1) = layout.shaped.to_line(d.x1, d.y1);
        let shift = if layout.shaped.vertical { 0. } else { layout.shaped.styles[d.style as usize].baseline_shift_pt * layout.shaped.px_per_pt };
        let center = (y0 + y1) * 0.5 + shift;
        let line_index =
            layout.shaped.lines.iter().enumerate().min_by(|a, b| (a.1.baseline - center).abs().total_cmp(&(b.1.baseline - center).abs())).map(|(i, _)| i);
        for (source, c) in layout.clusters.iter().filter(|c| c.visible).filter_map(|c| layout.shaped.clusters.get(c.source_index).map(|s| (s, c))) {
            let (a, b) = (x0.min(x1).max(source.x), x0.max(x1).min(source.x + source.advance));
            if b <= a || line_index != Some(source.line) {
                continue;
            }
            let post = transform.mul(&c.transform);
            let mut elements = Vec::new();
            for (i, (x, y)) in [(a, y0), (b, y0), (b, y1), (a, y1)].into_iter().enumerate() {
                let (x, y) = layout.shaped.to_text(x, y);
                let p = post.apply(Point::new(f64::from(x), f64::from(y)));
                elements.push(if i == 0 { PathEl::MoveTo([p.x, p.y]) } else { PathEl::LineTo([p.x, p.y]) });
            }
            elements.push(PathEl::Close);
            out.push(PaintedOutline { color: layout.shaped.styles[d.style as usize].color, elements });
        }
    }
    out
}

fn line(sink: &mut impl raster::LineSink, a: Point, b: Point, budget: &mut usize) -> Result<(), Error> {
    if *budget == 0 {
        return Err(Error("path text exceeds the flattened segment budget"));
    }
    *budget -= 1;
    sink.line((a.x, a.y), (b.x, b.y));
    Ok(())
}
fn flatten(sink: &mut impl raster::LineSink, p: [Point; 4], depth: u8, budget: &mut usize) -> Result<(), Error> {
    let chord = (p[3].x - p[0].x).hypot(p[3].y - p[0].y);
    let polygon = (p[1].x - p[0].x).hypot(p[1].y - p[0].y) + (p[2].x - p[1].x).hypot(p[2].y - p[1].y) + (p[3].x - p[2].x).hypot(p[3].y - p[2].y);
    // Bound both deviation and control-polygon excess (cusps/collinear reversals).
    let deviation = |q: Point| {
        if chord > 1e-12 { ((p[3].x - p[0].x) * (p[0].y - q.y) - (p[0].x - q.x) * (p[3].y - p[0].y)).abs() / chord } else { (q.x - p[0].x).hypot(q.y - p[0].y) }
    };
    if depth >= 18 || (deviation(p[1]).max(deviation(p[2])) <= 0.02 && polygon - chord <= 0.02) {
        return line(sink, p[0], p[3], budget);
    }
    let mid = |a: Point, b: Point| Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
    let (a, b, c) = (mid(p[0], p[1]), mid(p[1], p[2]), mid(p[2], p[3]));
    let (d, e) = (mid(a, b), mid(b, c));
    let f = mid(d, e);
    flatten(sink, [p[0], a, d, f], depth + 1, budget)?;
    flatten(sink, [f, e, c, p[3]], depth + 1, budget)
}
fn draw(outlines: &[PaintedOutline], color: Option<Color>, sink: &mut impl raster::LineSink, offset: Affine, budget: &mut usize) -> Result<(), Error> {
    for outline in outlines.iter().filter(|o| color.is_none_or(|c| o.color == c)) {
        let map = |[x, y]: [f64; 2]| offset.apply(Point::new(x, y));
        let mut start = Point::new(0., 0.);
        let mut last = start;
        for el in &outline.elements {
            match *el {
                PathEl::MoveTo(p) => {
                    start = map(p);
                    last = start;
                }
                PathEl::LineTo(p) => {
                    let p = map(p);
                    line(sink, last, p, budget)?;
                    last = p;
                }
                PathEl::QuadTo(c, p) => {
                    let (c, p) = (map(c), map(p));
                    let a = Point::new(last.x + (c.x - last.x) * 2. / 3., last.y + (c.y - last.y) * 2. / 3.);
                    let b = Point::new(p.x + (c.x - p.x) * 2. / 3., p.y + (c.y - p.y) * 2. / 3.);
                    flatten(sink, [last, a, b, p], 0, budget)?;
                    last = p;
                }
                PathEl::CurveTo(a, b, p) => {
                    let p = map(p);
                    flatten(sink, [last, map(a), map(b), p], 0, budget)?;
                    last = p;
                }
                PathEl::Close => {
                    line(sink, last, start, budget)?;
                    last = start;
                }
            }
        }
    }
    Ok(())
}

/// Validate projected outline controls before cache/conversion/export work.
pub fn validated_outlines(layout: &PathLayout, transform: &Affine) -> Result<Vec<PaintedOutline>, Error> {
    if !transform.m.into_iter().all(finite) {
        return Err(Error("invalid text transform"));
    }
    let paths = outlines(layout, transform);
    let mut elements = 0usize;
    for path in &paths {
        elements = elements.saturating_add(path.elements.len());
        for el in &path.elements {
            let coords: &[[f64; 2]] = match el {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => std::slice::from_ref(p),
                PathEl::QuadTo(a, b) => &[*a, *b],
                PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
                PathEl::Close => &[],
            };
            if coords.iter().flatten().any(|v| !finite(*v)) {
                return Err(Error("path text outline exceeds the coordinate limit"));
            }
        }
    }
    if elements > 262_144 {
        return Err(Error("path text outline exceeds the segment budget"));
    }
    Ok(paths)
}

/// Bounds-limited text cache, clipped before allocating coverage or pixels.
pub fn rasterize(layout: &PathLayout, transform: &Affine, format: PixelFormat, antialias: AntiAlias, clip: Rect) -> Result<render::Rendered, Error> {
    let paths = validated_outlines(layout, transform)?;
    let mut colors = Vec::new();
    for path in &paths {
        if !colors.contains(&path.color) {
            colors.push(path.color);
        }
    }
    if colors.len() > 256 {
        return Err(Error("path text exceeds the paint budget"));
    }
    let mut bounds = raster::Bounds::default();
    draw(&paths, None, &mut bounds, Affine::IDENTITY, &mut 1_048_576)?;
    let format = PixelFormat { alpha: true, ..format };
    let mut surface = Surface::new(format);
    let Some([x0, y0, x1, y1]) = bounds.rect else {
        return Ok(render::Rendered { surface, rect: Rect::EMPTY });
    };
    if ![x0, y0, x1, y1].into_iter().all(finite) {
        return Err(Error("path text bounds exceed the limit"));
    }
    let rect = Rect::new(x0.floor() as i32 - 1, y0.floor() as i32 - 1, x1.ceil() as i32 + 1, y1.ceil() as i32 + 1).intersect(&clip);
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    if w == 0 || h == 0 {
        return Ok(render::Rendered { surface, rect: Rect::EMPTY });
    }
    if w as u64 * h as u64 > 16_777_216 {
        return Err(Error("path text cache exceeds the pixel budget"));
    }
    let n = format.mode.color_channels();
    let stride = n + 1;
    let mut pixels = vec![0.0_f32; w * h * stride];
    let mut budget = 1_048_576;
    for run in paths.chunk_by(|a, b| a.color == b.color) {
        let color = run[0].color;
        let mut coverage = raster::Coverage::new(w, h);
        draw(run, Some(color), &mut coverage, Affine::translate(-f64::from(rect.x0), -f64::from(rect.y0)), &mut budget)?;
        let [r, g, b] = color.to_rgb();
        let comps = if color.mode == format.mode {
            color.c[..n].to_vec()
        } else {
            photocraft_raster::from_rgba(&PixelFormat { alpha: false, ..format }, [r, g, b, 1.])
        };
        for (px, cov) in pixels.chunks_exact_mut(stride).zip(coverage.finish()) {
            let cov = if antialias == AntiAlias::None { if cov >= 0.5 { 1.0 } else { 0.0 } } else { cov };
            let a = cov * color.alpha.clamp(0., 1.);
            for (v, c) in px[..n].iter_mut().zip(&comps) {
                *v = *c * a + *v * (1. - a);
            }
            px[n] = a + px[n] * (1. - a);
        }
    }
    for px in pixels.chunks_exact_mut(stride) {
        let a = px[n];
        if a > 0. {
            for v in &mut px[..n] {
                *v = (*v / a).clamp(0., 1.);
            }
        }
    }
    surface.write_region(rect, &pixels);
    surface.prune();
    Ok(render::Rendered { surface, rect })
}
