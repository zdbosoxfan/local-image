//! Rasterizes a [`TextLayout`] into a document-space [`Surface`] of any pixel format.

use photocraft_color::{Color, PixelFormat};
use photocraft_doc::text::AntiAlias;
use photocraft_geom::{Affine, Rect};
use photocraft_raster::Surface;
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::DrawSettings;
use skrifa::{GlyphId, MetadataProvider};

use crate::layout::{GlyphOrient, PlacedGlyph, TextLayout};
use crate::raster::{Bounds, Coverage, LineSink, Pen, Xform, rect_to};
use crate::warp::Warp;

/// Faux-italic slant in degrees (Photoshop-like).
pub const FAUX_ITALIC_DEG: f32 = 12.0;
/// Faux-bold dilation radius as a fraction of the font size.
pub const FAUX_BOLD_RADIUS: f32 = 0.018;
/// Largest raster we produce (pixels), as a guard against absurd sizes.
const MAX_PIXELS: u64 = 256 * 1024 * 1024;

/// Rendered text: pixels in document space plus the covered rectangle.
pub struct Rendered {
    pub surface: Surface,
    pub rect: Rect,
}

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
            let settings = DrawSettings::unhinted(Size::new(face.size_px), LocationRef::new(&coords));
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

/// The warp to apply to `layout` (None when `warp` is absent, `warpNone` or flat).
pub fn layout_warp(layout: &TextLayout, warp: Option<&photocraft_doc::text::TextWarp>) -> Option<Warp> {
    Warp::new(warp?, layout.bounds()?)
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

/// Document-space bounds of the drawn text (integer pixel rectangle).
pub fn ink_rect(layout: &TextLayout, transform: &Affine) -> Rect {
    ink_rect_warped(layout, transform, None)
}

fn rect_from_bounds([x0, y0, x1, y1]: [f64; 4]) -> Rect {
    if ![x0, y0, x1, y1].into_iter().all(f64::is_finite) {
        return Rect::EMPTY;
    }

    let min = f64::from(i32::MIN);
    let max = f64::from(i32::MAX);
    let (x0, y0, x1, y1) =
        ((x0.floor() - 1.0).clamp(min, max), (y0.floor() - 1.0).clamp(min, max), (x1.ceil() + 1.0).clamp(min, max), (y1.ceil() + 1.0).clamp(min, max));
    if x1 <= x0 || y1 <= y0 || x1 - x0 > MAX_PIXELS as f64 || y1 - y0 > MAX_PIXELS as f64 {
        return Rect::EMPTY;
    }

    Rect::new(x0 as i32, y0 as i32, x1 as i32, y1 as i32)
}

/// [`ink_rect`] of warped text.
pub fn ink_rect_warped(layout: &TextLayout, transform: &Affine, warp: Option<&Warp>) -> Rect {
    let mut b = Bounds::default();
    draw(layout, &Xform(transform.m), &mut b, None, warp);
    b.rect.map(rect_from_bounds).unwrap_or(Rect::EMPTY)
}

/// Rasterizes `layout` through `transform` (text space → document pixels). The result always
/// has an alpha channel; colour is written in `format`'s colour model and sample depth.
pub fn rasterize(layout: &TextLayout, transform: &Affine, format: PixelFormat, antialias: AntiAlias) -> Rendered {
    rasterize_warped(layout, transform, format, antialias, None)
}

/// Only adjacent paints may share a coverage mask. Nonadjacent equal colours must
/// remain above the intervening glyphs, including translucent and negative-tracking runs.
pub fn paint_runs(layout: &TextLayout) -> Vec<(Color, Vec<PlacedGlyph>, Vec<crate::layout::DecorationRect>)> {
    let mut runs: Vec<(Color, Vec<PlacedGlyph>, Vec<crate::layout::DecorationRect>)> = Vec::new();
    for glyph in &layout.glyphs {
        let color = layout.styles[glyph.style as usize].color;
        if runs.last().is_none_or(|(previous, _, _)| *previous != color) {
            runs.push((color, Vec::new(), Vec::new()));
        }
        if let Some(run) = runs.last_mut() {
            run.1.push(*glyph);
        }
    }
    for decoration in &layout.decorations {
        let color = layout.styles[decoration.style as usize].color;
        if runs.last().is_none_or(|(previous, _, _)| *previous != color) {
            runs.push((color, Vec::new(), Vec::new()));
        }
        if let Some(run) = runs.last_mut() {
            run.2.push(*decoration);
        }
    }
    runs
}

/// [`rasterize`] with the glyph outlines bent by `warp` (Type › Warp Text).
pub fn rasterize_warped(layout: &TextLayout, transform: &Affine, format: PixelFormat, antialias: AntiAlias, warp: Option<&Warp>) -> Rendered {
    let format = PixelFormat { alpha: true, ..format };
    let rect = ink_rect_warped(layout, transform, warp);
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    let mut surface = Surface::new(format);
    if w == 0 || h == 0 || (w as u64) * (h as u64) > MAX_PIXELS {
        return Rendered { surface, rect: Rect::new(0, 0, 0, 0) };
    }
    let n = format.mode.color_channels();
    let stride = n + 1;
    // Premultiplied accumulation.
    let mut acc = vec![0.0f32; w * h * stride];
    let xf = Xform([1.0, 0.0, 0.0, 1.0, -rect.x0 as f64, -rect.y0 as f64]).mul(&Xform(transform.m));
    let mut part = layout.clone();
    for (color, glyphs, decorations) in paint_runs(layout) {
        let c = &color;
        part.glyphs = glyphs;
        part.decorations = decorations;
        let mut cov = Coverage::new(w, h);
        draw(&part, &xf, &mut cov, Some(c), warp);
        let cov = cov.finish();
        let comps = color_in(&format, c);
        let a = c.alpha.clamp(0.0, 1.0);
        for (i, &cv) in cov.iter().enumerate() {
            let cv = if antialias == AntiAlias::None { if cv >= 0.5 { 1.0 } else { 0.0 } } else { cv };
            let s = cv * a;
            if s <= 0.0 {
                continue;
            }
            let px = &mut acc[i * stride..(i + 1) * stride];
            let keep = 1.0 - s;
            for j in 0..n {
                px[j] = comps[j] * s + px[j] * keep;
            }
            px[n] = s + px[n] * keep;
        }
    }
    // Unpremultiply.
    for px in acc.chunks_exact_mut(stride) {
        let a = px[n];
        if a > 0.0 {
            for v in &mut px[..n] {
                *v = (*v / a).clamp(0.0, 1.0);
            }
        }
    }
    surface.write_region(rect, &acc);
    surface.prune();
    Rendered { surface, rect }
}

/// One element of a glyph outline in document space (see [`outlines`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathEl {
    MoveTo([f64; 2]),
    LineTo([f64; 2]),
    QuadTo([f64; 2], [f64; 2]),
    CurveTo([f64; 2], [f64; 2], [f64; 2]),
    Close,
}

/// Records outline commands through a text→document map (and an optional warp).
struct Recorder<'a> {
    glyph: Xform,
    post: Xform,
    warp: Option<&'a Warp>,
    out: Vec<PathEl>,
}

impl Recorder<'_> {
    fn map(&self, x: f32, y: f32) -> [f64; 2] {
        let (tx, ty) = self.glyph.apply(f64::from(x), f64::from(y));
        let (tx, ty) = match self.warp {
            Some(w) => w.apply(tx, ty),
            None => (tx, ty),
        };
        let (dx, dy) = self.post.apply(tx, ty);
        [dx, dy]
    }
}

impl skrifa::outline::OutlinePen for Recorder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.out.push(PathEl::MoveTo(p));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.out.push(PathEl::LineTo(p));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.map(cx0, cy0), self.map(x, y));
        self.out.push(PathEl::QuadTo(c, p));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (a, b, p) = (self.map(cx0, cy0), self.map(cx1, cy1), self.map(x, y));
        self.out.push(PathEl::CurveTo(a, b, p));
    }
    fn close(&mut self) {
        self.out.push(PathEl::Close);
    }
}

/// Limits for the incremental outline visitor. Scratch includes the drawer's
/// reported temporary bytes and this glyph's normalized variation coordinates.
#[derive(Clone, Copy, Debug)]
pub struct OutlineVisitLimits {
    pub max_glyphs: usize,
    pub max_segments: usize,
    pub max_scratch_bytes: usize,
}
/// A glyph boundary followed by zero or more document-space path elements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutlineEvent {
    Glyph { index: usize, color: Color },
    Element(PathEl),
}
#[derive(Debug)]
pub enum OutlineVisitError<E> {
    Cancelled,
    Budget(&'static str),
    Visitor(E),
    InvalidOutline,
}

struct VisitingPen<'a, E, V> {
    glyph: Xform,
    post: Xform,
    warp: Option<&'a Warp>,
    cancelled: &'a dyn Fn() -> bool,
    visit: &'a mut V,
    segments: &'a mut usize,
    limit: usize,
    error: Option<OutlineVisitError<E>>,
}
impl<E, V: FnMut(OutlineEvent) -> Result<(), E>> VisitingPen<'_, E, V> {
    fn active(&mut self) -> bool {
        if self.error.is_some() {
            return false;
        }
        if (self.cancelled)() {
            self.error = Some(OutlineVisitError::Cancelled);
            return false;
        }
        if *self.segments >= self.limit {
            self.error = Some(OutlineVisitError::Budget("text outline segments"));
            return false;
        }
        true
    }
    fn map(&self, x: f32, y: f32) -> [f64; 2] {
        let (x, y) = self.glyph.apply(f64::from(x), f64::from(y));
        let (x, y) = self.warp.map_or((x, y), |warp| warp.apply(x, y));
        let (x, y) = self.post.apply(x, y);
        [x, y]
    }
    fn emit(&mut self, element: PathEl) {
        *self.segments += 1;
        if let Err(error) = (self.visit)(OutlineEvent::Element(element)) {
            self.error = Some(OutlineVisitError::Visitor(error));
        }
    }
}
impl<E, V: FnMut(OutlineEvent) -> Result<(), E>> skrifa::outline::OutlinePen for VisitingPen<'_, E, V> {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.active() {
            let p = self.map(x, y);
            self.emit(PathEl::MoveTo(p));
        }
    }
    fn line_to(&mut self, x: f32, y: f32) {
        if self.active() {
            let p = self.map(x, y);
            self.emit(PathEl::LineTo(p));
        }
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        if self.active() {
            let (c, p) = (self.map(cx, cy), self.map(x, y));
            self.emit(PathEl::QuadTo(c, p));
        }
    }
    fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
        if self.active() {
            let (a, b, p) = (self.map(ax, ay), self.map(bx, by), self.map(x, y));
            self.emit(PathEl::CurveTo(a, b, p));
        }
    }
    fn close(&mut self) {
        if self.active() {
            self.emit(PathEl::Close);
        }
    }
}

/// Visit outlines without building a per-glyph or full-layout path collection.
/// Glyph admission precedes traversal; cancellation and the cumulative segment
/// bound are checked before each emitted command. Per-glyph scratch is checked
/// before allocation and supplied explicitly to the drawer. No layout is cloned.
///
/// The pinned skrifa pen is infallible: after a visitor error, limit or cancel,
/// its current glyph draw finishes internally, but no further coordinates are
/// mapped, callbacks invoked or path elements retained, and no next glyph starts.
/// The caller must discard its temporary output on error. Existing collecting
/// [`outlines`] / [`painted_outlines`] APIs retain their behavior.
pub fn visit_outlines<E>(
    layout: &TextLayout,
    transform: &Affine,
    warp: Option<&Warp>,
    limits: OutlineVisitLimits,
    cancelled: &dyn Fn() -> bool,
    mut visit: impl FnMut(OutlineEvent) -> Result<(), E>,
) -> Result<(), OutlineVisitError<E>> {
    if cancelled() {
        return Err(OutlineVisitError::Cancelled);
    }
    if layout.glyphs.len() > limits.max_glyphs {
        return Err(OutlineVisitError::Budget("text glyphs"));
    }
    let mut segments = 0;
    for (index, glyph) in layout.glyphs.iter().enumerate() {
        if cancelled() {
            return Err(OutlineVisitError::Cancelled);
        }
        let face = layout.faces.get(glyph.face as usize).ok_or(OutlineVisitError::InvalidOutline)?;
        let color = layout.styles.get(glyph.style as usize).ok_or(OutlineVisitError::InvalidOutline)?.color;
        visit(OutlineEvent::Glyph { index, color }).map_err(OutlineVisitError::Visitor)?;
        let font = skrifa::FontRef::from_index(face.font.data.as_ref(), face.font.index).map_err(|_| OutlineVisitError::InvalidOutline)?;
        let Some(outline) = font.outline_glyphs().get(GlyphId::new(glyph.id)) else {
            continue;
        };
        let scratch_bytes = outline.draw_memory_size(skrifa::outline::Hinting::None);
        let coordinate_bytes =
            face.coords.len().checked_mul(std::mem::size_of::<NormalizedCoord>()).ok_or(OutlineVisitError::Budget("text outline scratch"))?;
        if scratch_bytes.checked_add(coordinate_bytes).is_none_or(|bytes| bytes > limits.max_scratch_bytes) {
            return Err(OutlineVisitError::Budget("text outline scratch"));
        }
        if cancelled() {
            return Err(OutlineVisitError::Cancelled);
        }
        let mut coords = Vec::new();
        coords.try_reserve_exact(face.coords.len()).map_err(|_| OutlineVisitError::Budget("text outline scratch"))?;
        coords.extend(face.coords.iter().map(|&c| NormalizedCoord::from_bits(c)));
        let mut scratch = Vec::new();
        scratch.try_reserve_exact(scratch_bytes).map_err(|_| OutlineVisitError::Budget("text outline scratch"))?;
        scratch.resize(scratch_bytes, 0u8);
        let mut pen = VisitingPen {
            glyph: glyph_xform(layout, glyph, face.skew_deg),
            post: Xform(transform.m),
            warp,
            cancelled,
            visit: &mut visit,
            segments: &mut segments,
            limit: limits.max_segments,
            error: None,
        };
        let settings = DrawSettings::unhinted(Size::new(face.size_px), LocationRef::new(&coords)).with_memory(Some(&mut scratch));
        let result = outline.draw(settings, &mut pen);
        if let Some(error) = pen.error {
            return Err(error);
        }
        result.map_err(|_| OutlineVisitError::InvalidOutline)?;
    }
    Ok(())
}

/// Glyph outlines of `layout` in document space (through `transform`, bent by `warp`), one
/// element list per glyph, for Type › Create Work Path and Convert to Shape. Faux bold is not
/// applied (Photoshop also converts the regular outline); faux italic and scaling are. Under a
/// warp, control points are mapped directly, which is exact for straight segments and a close
/// approximation for curves.
pub fn outlines(layout: &TextLayout, transform: &Affine, warp: Option<&Warp>) -> Vec<Vec<PathEl>> {
    painted_outlines(layout, transform, warp).into_iter().map(|(_, outline)| outline).collect()
}
/// Preserve source glyph paints without relying on output count after missing outlines.
pub fn painted_outlines(layout: &TextLayout, transform: &Affine, warp: Option<&Warp>) -> Vec<(photocraft_doc::Color, Vec<PathEl>)> {
    let mut glyphs = Vec::new();
    for g in &layout.glyphs {
        let face = &layout.faces[g.face as usize];
        let Ok(font) = skrifa::FontRef::from_index(face.font.data.as_ref(), face.font.index) else {
            continue;
        };
        let Some(outline) = font.outline_glyphs().get(GlyphId::new(g.id)) else {
            continue;
        };
        let coords: Vec<NormalizedCoord> = face.coords.iter().map(|&c| NormalizedCoord::from_bits(c)).collect();
        let glyph = glyph_xform(layout, g, face.skew_deg);
        let mut rec = Recorder { glyph, post: Xform(transform.m), warp, out: Vec::new() };
        let settings = DrawSettings::unhinted(Size::new(face.size_px), LocationRef::new(&coords));
        if outline.draw(settings, &mut rec).is_ok() && !rec.out.is_empty() {
            glyphs.push((layout.styles[g.style as usize].color, rec.out));
        }
    }
    glyphs
}

#[cfg(test)]
mod tests {
    use super::{MAX_PIXELS, rect_from_bounds};
    use photocraft_geom::Rect;

    #[test]
    fn bounds_are_clipped_without_overflow_and_oversized_rectangles_rejected() {
        let min = f64::from(i32::MIN);
        let max = f64::from(i32::MAX);
        let cases = [
            ([min, 0.0, min + 8.0, 8.0], Rect::new(i32::MIN, -1, i32::MIN + 9, 9)),
            ([max - 8.0, 0.0, max, 8.0], Rect::new(i32::MAX - 9, -1, i32::MAX, 9)),
            ([-f64::MAX, 0.0, f64::MAX, 8.0], Rect::EMPTY),
            ([0.0, 0.0, MAX_PIXELS as f64, 8.0], Rect::EMPTY),
            ([0.25, -3.25, 10.1, 8.8], Rect::new(-1, -5, 12, 10)),
        ];

        for (bounds, expected) in cases {
            let first = rect_from_bounds(bounds);
            assert_eq!(first, expected, "bounds: {bounds:?}");
            assert_eq!(rect_from_bounds(bounds), first, "bounds: {bounds:?}");
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod outline_visit_tests {
    use super::*;
    use photocraft_doc::{
        TextLayer,
        text::{CharStyle, TextRun, TextWarp},
    };
    use std::cell::Cell;
    fn fixture(value: &str) -> (TextLayer, TextLayout) {
        let text = TextLayer { text: value.into(), font_family: "Inter".into(), size_pt: 32., ..Default::default() };
        let layout = crate::TextEngine::new().layout(&text, 96.);
        (text, layout)
    }
    fn limits() -> OutlineVisitLimits {
        OutlineVisitLimits { max_glyphs: 100, max_segments: 10_000, max_scratch_bytes: 1 << 20 }
    }
    #[test]
    fn visitor_matches_existing_painted_outlines_with_transform_and_warp() {
        for warped in [false, true] {
            let (mut text, _) = fixture("BO fi");
            text.transform = Affine::translate(14., 80.).mul(&Affine::rotate(0.17));
            text.runs = vec![
                TextRun {
                    len: 2,
                    style: CharStyle { font_family: "Inter".into(), size_pt: 32., color: Color::rgba(0.8, 0.2, 0.1, 0.6), ..Default::default() },
                },
                TextRun {
                    len: 3,
                    style: CharStyle { font_family: "Inter".into(), size_pt: 32., color: Color::rgb(0.1, 0.4, 0.8), faux_italic: true, ..Default::default() },
                },
            ];
            if warped {
                text.warp = Some(TextWarp { style: "warpArc".into(), value: 18., ..Default::default() });
            }
            let layout = crate::TextEngine::new().layout(&text, 96.);
            let warp = layout_warp(&layout, text.warp.as_ref());
            let expected = painted_outlines(&layout, &text.transform, warp.as_ref());
            let mut actual = Vec::<(Color, Vec<PathEl>)>::new();
            visit_outlines(&layout, &text.transform, warp.as_ref(), limits(), &|| false, |event| {
                match event {
                    OutlineEvent::Glyph { color, .. } => actual.push((color, Vec::new())),
                    OutlineEvent::Element(element) => actual.last_mut().unwrap().1.push(element),
                }
                Ok::<(), ()>(())
            })
            .unwrap();
            actual.retain(|(_, path)| !path.is_empty());
            assert!(!actual.is_empty());
            assert_eq!(actual, expected);
        }
    }
    #[test]
    fn segment_budget_stops_retention_and_never_starts_remaining_glyphs() {
        let (text, layout) = fixture("BBBBBBBB");
        assert!(layout.glyphs.len() > 1);
        let mut bounds = limits();
        bounds.max_segments = 3;
        let mut started = 0;
        let mut retained = Vec::with_capacity(3);
        let result = visit_outlines(&layout, &text.transform, None, bounds, &|| false, |event| {
            match event {
                OutlineEvent::Glyph { .. } => started += 1,
                OutlineEvent::Element(element) => retained.push(element),
            }
            Ok::<(), ()>(())
        });
        assert!(matches!(result, Err(OutlineVisitError::Budget("text outline segments"))));
        assert_eq!(started, 1);
        assert_eq!(retained.len(), 3);
        assert_eq!(retained.capacity(), 3);
    }
    #[test]
    fn cancellation_and_visitor_error_stop_during_first_glyph() {
        let (text, layout) = fixture("BBBBBBBB");
        let mut callbacks = 0;
        let result = visit_outlines(&layout, &text.transform, None, limits(), &|| true, |_| {
            callbacks += 1;
            Ok::<(), ()>(())
        });
        assert!(matches!(result, Err(OutlineVisitError::Cancelled)));
        assert_eq!(callbacks, 0);
        let elements = Cell::new(0);
        let mut started = 0;
        let mut retained = Vec::with_capacity(2);
        let result = visit_outlines(&layout, &text.transform, None, limits(), &|| elements.get() >= 2, |event| {
            match event {
                OutlineEvent::Glyph { .. } => started += 1,
                OutlineEvent::Element(element) => {
                    elements.set(elements.get() + 1);
                    retained.push(element);
                }
            }
            Ok::<(), ()>(())
        });
        assert!(matches!(result, Err(OutlineVisitError::Cancelled)));
        assert_eq!(started, 1);
        assert_eq!(retained.len(), 2);
        assert_eq!(retained.capacity(), 2);
        let mut started = 0;
        let mut elements = 0;
        let result = visit_outlines(&layout, &text.transform, None, limits(), &|| false, |event| {
            match event {
                OutlineEvent::Glyph { .. } => started += 1,
                OutlineEvent::Element(_) => {
                    elements += 1;
                    return Err("stop");
                }
            }
            Ok(())
        });
        assert!(matches!(result, Err(OutlineVisitError::Visitor("stop"))));
        assert_eq!(started, 1);
        assert_eq!(elements, 1);
    }
    #[test]
    fn glyph_and_scratch_budgets_refuse_before_path_allocation() {
        let (text, layout) = fixture("BBBBBBBB");
        let mut bounds = limits();
        bounds.max_glyphs = 1;
        let mut callbacks = 0;
        let result = visit_outlines(&layout, &text.transform, None, bounds, &|| false, |_| {
            callbacks += 1;
            Ok::<(), ()>(())
        });
        assert!(matches!(result, Err(OutlineVisitError::Budget("text glyphs"))));
        assert_eq!(callbacks, 0);
        let glyph = &layout.glyphs[0];
        let face = &layout.faces[glyph.face as usize];
        let font = skrifa::FontRef::from_index(face.font.data.as_ref(), face.font.index).unwrap();
        let outline = font.outline_glyphs().get(GlyphId::new(glyph.id)).unwrap();
        assert!(outline.draw_memory_size(skrifa::outline::Hinting::None) > 0, "fixture needs drawer scratch");
        bounds = limits();
        bounds.max_scratch_bytes = 0;
        let mut started = 0;
        let mut elements = 0;
        let result = visit_outlines(&layout, &text.transform, None, bounds, &|| false, |event| {
            match event {
                OutlineEvent::Glyph { .. } => started += 1,
                OutlineEvent::Element(_) => elements += 1,
            }
            Ok::<(), ()>(())
        });
        assert!(matches!(result, Err(OutlineVisitError::Budget("text outline scratch"))));
        assert_eq!(started, 1);
        assert_eq!(elements, 0);
    }
}
