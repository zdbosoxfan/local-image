//! Optional frame composer on native pc-text advances and glyph runs.
//!
//! No alternate shaper/renderer is used. The disabled route is the native layout exactly.
//! Bidi, vertical, tabs, discretionary/dictionary hyphenation and complex-script breaking
//! remain native until a cluster-safe adapter for those cases is audited. Integration owns
//! persisting Options beside TextLayer and selecting this hook during cache refresh.
use std::ops::Range;

use crate::{TextEngine, layout::TextLayout};
use photocraft_doc::{
    TextLayer,
    text::{Orientation, ParagraphRun, ParagraphStyle, TextAlign, TextRun, TextShape},
};

pub mod breaker;

/// Cluster advance adapter for the pinned DesignCraft breaker; not a new glyph stack.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub ch: char,
    pub adv: f64,
    pub space: f64,
    pub size: f64,
    pub hyphen_width: f64,
    pub locked_advance: bool,
    pub ideographic_space_elastic: bool,
    pub no_break: bool,
    pub break_after: Option<bool>,
    pub cjk_hang: f64,
    pub korean: bool,
}
impl Glyph {
    pub fn is_space(&self) -> bool {
        matches!(self.ch, ' ' | '\u{3000}' | '\t')
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    pub enabled: bool,
    pub balanced: bool,
    pub optical_margins: bool,
    /// Text-space pixels, positive downward. Zero spacing disables snapping.
    pub grid_spacing: f32,
    pub grid_origin: f32,
}
impl Options {
    pub fn validate(self) -> Result<(), Error> {
        if !self.grid_spacing.is_finite()
            || !(0.0..=4096.0).contains(&self.grid_spacing)
            || !self.grid_origin.is_finite()
            || self.grid_origin.abs() > 1_048_576.0
        {
            return Err(Error("invalid baseline grid"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Native,
    Composed,
    NativeComplexText,
}
#[derive(Clone, Debug)]
pub struct Composition {
    pub layout: TextLayout,
    /// First byte not placed, including a word wider than the frame. Text remains intact.
    pub overflow: Option<usize>,
    pub route: Route,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(pub &'static str);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}

/// Input guard shared by UI preflight and composer. Bounds precede all shaping/allocation.
pub fn validate(t: &TextLayer, dpi: f32) -> Result<(), Error> {
    if t.text.len() > 65_536 || t.char_runs().len() > 256 || t.paragraph_runs().len() > 256 {
        return Err(Error("frame text exceeds the text/run budget"));
    }
    if !dpi.is_finite()
        || !(1.0..=9600.0).contains(&dpi)
        || !t.transform.m.iter().all(|v| v.is_finite() && v.abs() <= 1_048_576.0)
        || t.transform.inverse().is_none()
    {
        return Err(Error("invalid frame resolution or transform"));
    }
    if let TextShape::Box { x, y, width, height } = t.shape
        && (![x, y, width, height].iter().all(|v| v.is_finite() && v.abs() <= 1_048_576.0) || width < 1.0 || height < 1.0)
    {
        return Err(Error("invalid frame bounds"));
    }
    for r in t.char_runs() {
        let s = r.style;
        if !s.size_pt.is_finite()
            || !(0.1..=4096.0).contains(&s.size_pt)
            || ![s.tracking, s.kern, s.baseline_shift_pt].iter().all(|v| v.is_finite() && v.abs() <= 4096.0)
            || ![s.horizontal_scale, s.vertical_scale].iter().all(|v| v.is_finite() && (0.01..=100.0).contains(v))
            || s.leading_pt.is_some_and(|v| !v.is_finite() || !(0.1..=4096.0).contains(&v))
            || s.variations.iter().any(|v| !v.value.is_finite())
            || !s.color.c.iter().all(|v| v.is_finite())
            || !s.color.alpha.is_finite()
        {
            return Err(Error("invalid frame character style"));
        }
    }
    for r in t.paragraph_runs() {
        let s = r.style;
        if ![s.first_line_indent_pt, s.start_indent_pt, s.end_indent_pt, s.space_before_pt, s.space_after_pt].iter().all(|v| v.is_finite() && v.abs() <= 4096.0)
            || !s.auto_leading.is_finite()
            || !(0.1..=10.0).contains(&s.auto_leading)
        {
            return Err(Error("invalid frame paragraph style"));
        }
    }
    Ok(())
}

fn remaining(t: &TextLayer, layout: &TextLayout) -> Option<usize> {
    let end = layout.lines.last().map_or(0, |l| l.range.end);
    t.text.get(end..).filter(|s| !s.chars().all(char::is_whitespace)).map(|_| end)
}
fn complex(t: &TextLayer) -> bool {
    t.orientation != Orientation::Horizontal
        || t.warp.is_some()
        || t.paragraph_runs().iter().any(|r| r.style.hyphenate || r.style.direction == photocraft_doc::text::TextDirection::Rtl)
        || t.text.chars().any(|c| {
            matches!(c, '\t' | '\u{ad}' | '\u{3}' | '\u{200b}' | '\u{2060}')
                || matches!(c as u32, 0x590..=0x8ff | 0xe00..=0x109f | 0x1780..=0x17ff | 0x2028..=0x202e | 0x2066..=0x2069)
        })
}
fn paragraph_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if c == '\r' || c == '\n' {
            out.push(start..at);
            start = at + 1;
            if c == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                chars.next();
                start += 1;
            }
        }
    }
    out.push(start..text.len());
    out
}
fn paragraph_style(t: &TextLayer, at: usize) -> ParagraphStyle {
    let mut end = 0;
    let runs = t.paragraph_runs();
    runs.iter()
        .find(|r| {
            end += r.len;
            at < end
        })
        .or(runs.last())
        .map(|r| r.style.clone())
        .unwrap_or_default()
}
/// A detached slice preserving the original resolved native run styles.
fn slice(t: &TextLayer, range: Range<usize>, ps: &ParagraphStyle) -> TextLayer {
    let mut part = t.clone();
    part.cache = None;
    part.psd_raw = None;
    part.text = t.text[range.clone()].into();
    part.runs.clear();
    let mut at = 0;
    for r in t.char_runs() {
        let end = at + r.len;
        let lo = at.max(range.start);
        let hi = end.min(range.end);
        if hi > lo || (range.is_empty() && at <= range.start && range.start <= end && part.runs.is_empty()) {
            part.runs.push(TextRun { len: hi.saturating_sub(lo), style: r.style });
        }
        at = end;
    }
    let mut p = ps.clone();
    p.align = TextAlign::Left;
    p.start_indent_pt = 0.0;
    p.end_indent_pt = 0.0;
    p.first_line_indent_pt = 0.0;
    p.space_before_pt = 0.0;
    p.space_after_pt = 0.0;
    part.paragraphs = vec![ParagraphRun { len: part.text.len(), style: p }];
    part.shape = TextShape::Point;
    part
}

/// Compose using native cluster advances, then shape chosen lines with the same engine.
/// Native re-shaping preserves ligatures, run/font fallback, decorations and caret clusters.
pub fn compose(engine: &mut TextEngine, t: &TextLayer, dpi: f32, options: Options) -> Result<Composition, Error> {
    options.validate()?;
    validate(t, dpi)?;
    let TextShape::Box { x, y, width, height } = t.shape else {
        return Ok(Composition { layout: engine.layout(t, dpi), overflow: None, route: Route::Native });
    };
    if !options.enabled || complex(t) {
        let layout = engine.layout(t, dpi);
        return Ok(Composition { overflow: remaining(t, &layout), layout, route: if options.enabled { Route::NativeComplexText } else { Route::Native } });
    }
    let k = dpi / 72.0;
    let mut out = TextLayout { px_per_pt: k, ..Default::default() };
    let mut baseline: Option<f32> = None;
    let mut pending_space = 0.0;
    let mut overflow = None;
    let mut exhausted = false;
    for (pi, range) in paragraph_ranges(&t.text).into_iter().enumerate() {
        let ps = paragraph_style(t, range.start);
        let part = slice(t, range.clone(), &ps);
        let shaped = engine.layout(&part, dpi);
        if shaped.lines.len() != 1 || shaped.clusters.iter().any(|c| c.rtl) {
            let layout = engine.layout(t, dpi);
            return Ok(Composition { overflow: remaining(t, &layout), layout, route: Route::NativeComplexText });
        }
        // Sort logical clusters; never split a grapheme or native shaping cluster.
        let mut clusters = shaped.clusters.clone();
        clusters.sort_by_key(|c| c.range.start);
        if clusters.len() > 4096 {
            return Err(Error("frame paragraph exceeds the cluster budget"));
        }
        let mut run_end = 0;
        let languages: Vec<_> = part
            .char_runs()
            .into_iter()
            .map(|r| {
                run_end += r.len;
                let korean = r.style.language.as_deref().and_then(|s| s.split('-').next()).is_some_and(|s| s.eq_ignore_ascii_case("ko"));
                (run_end, korean)
            })
            .collect();
        let glyphs: Vec<Glyph> = clusters
            .iter()
            .enumerate()
            .map(|(ci, c)| {
                let ch = part.text[c.range.clone()].chars().next().unwrap_or(' ');
                Glyph {
                    ch,
                    adv: f64::from(c.advance.max(0.0)),
                    space: f64::from(c.advance.max(0.0)),
                    size: 0.0,
                    hyphen_width: 0.0,
                    locked_advance: true,
                    ideographic_space_elastic: false,
                    no_break: ch == '\u{a0}' || c.advance <= 0.0,
                    break_after: clusters.get(ci + 1).filter(|c| c.advance <= 0.0).map(|_| false),
                    cjk_hang: 0.0,
                    korean: languages.iter().find(|(end, _)| c.range.start < *end).is_some_and(|(_, korean)| *korean),
                }
            })
            .collect();
        if glyphs.iter().any(|g| !g.adv.is_finite()) {
            return Err(Error("invalid native cluster advance"));
        }
        let measure =
            |li: usize| f64::from((width - (ps.start_indent_pt + ps.end_indent_pt + if li == 0 { ps.first_line_indent_pt } else { 0.0 }) * k).max(1.0));
        let spacing = breaker::Spacing { justify: ps.align.is_justified(), optical: options.optical_margins, ..Default::default() };
        let hyphens = vec![false; glyphs.len()];
        let breaks = if options.balanced {
            breaker::balanced(&glyphs, &hyphens, &spacing, &measure)
        } else {
            breaker::knuth_plass(&glyphs, &hyphens, &spacing, &measure)
        };
        if baseline.is_some() {
            pending_space += ps.space_before_pt * k;
        }
        for (li, b) in breaks.iter().enumerate() {
            let start = clusters.get(b.start).map_or(part.text.len(), |c| c.range.start);
            let end = if b.end > b.start { clusters[b.end - 1].range.end } else { start };
            let mut line_part = slice(&part, start..end, &ps);
            // Native box first-baseline metric (ascender of 'd'), retained exactly.
            line_part.shape = TextShape::Box { x: 0.0, y: 0.0, width: 1_048_576.0, height: 1_048_576.0 };
            let mut line = engine.layout(&line_part, dpi);
            if line.lines.len() != 1 {
                return Err(Error("native frame line exceeds the measure budget"));
            }
            let Some(info) = line.lines.first().cloned() else { continue };
            let leading = line.styles.iter().map(|s| s.leading_pt.map_or(ps.auto_leading * s.size_pt * k, |l| l * k)).fold(0.0, f32::max);
            let mut by = baseline.map_or(y + info.baseline, |v| v + leading + pending_space);
            if options.grid_spacing > 0.0 {
                by = options.grid_origin + ((by - options.grid_origin) / options.grid_spacing).ceil() * options.grid_spacing;
            }
            if by + info.descent > y + height + 0.5 {
                overflow.get_or_insert(range.start + start);
                exhausted = true;
                break;
            }
            let last = li + 1 == breaks.len();
            let target = measure(li) as f32;
            let origin = x + (ps.start_indent_pt + if li == 0 { ps.first_line_indent_pt } else { 0.0 }) * k;
            let natural = position_line(&mut line, &line_part.text, target, ps.align, last, options.optical_margins);
            if natural > target + 0.5 && overflow.is_none() {
                overflow = Some(range.start + start);
            }
            let face_base = out.faces.len() as u32;
            let style_base = out.styles.len() as u32;
            let line_base = out.lines.len();
            let dy = by - info.baseline;
            for g in &mut line.glyphs {
                g.x += origin;
                g.y += dy;
                g.face += face_base;
                g.style += style_base;
            }
            for d in &mut line.decorations {
                d.x0 += origin;
                d.x1 += origin;
                d.y0 += dy;
                d.y1 += dy;
                d.style += style_base;
            }
            for c in &mut line.clusters {
                c.x += origin;
                c.range.start += range.start + start;
                c.range.end += range.start + start;
                c.line += line_base;
            }
            for l in &mut line.lines {
                l.x0 += origin;
                l.x1 += origin;
                l.baseline += dy;
                l.range.start += range.start + start;
                l.range.end += range.start + start;
                l.paragraph = pi;
            }
            out.faces.extend(line.faces);
            out.styles.extend(line.styles);
            out.glyphs.extend(line.glyphs);
            out.decorations.extend(line.decorations);
            out.clusters.extend(line.clusters);
            out.lines.extend(line.lines);
            baseline = Some(by);
            pending_space = 0.0;
        }
        if exhausted {
            break;
        }
        pending_space += ps.space_after_pt * k;
    }
    Ok(Composition { layout: out, overflow, route: Route::Composed })
}

fn position_line(line: &mut TextLayout, text: &str, target: f32, align: TextAlign, last: bool, optical: bool) -> f32 {
    let Some(info) = line.lines.first() else { return 0.0 };
    let mut width = info.x1 - info.x0;
    let left_hang = if optical {
        line.clusters.first().and_then(|c| text[c.range.clone()].chars().next().map(|ch| breaker::hang(ch).0 as f32 * c.advance)).unwrap_or(0.0)
    } else {
        0.0
    };
    let right_hang = if optical {
        line.clusters.last().and_then(|c| text[c.range.clone()].chars().next_back().map(|ch| breaker::hang(ch).1 as f32 * c.advance)).unwrap_or(0.0)
    } else {
        0.0
    };
    let spaces: Vec<_> = line.clusters.iter().enumerate().filter(|(_, c)| text.get(c.range.clone()) == Some(" ")).map(|(i, c)| (i, c.x, c.advance)).collect();
    let justify = align.is_justified() && (!last || align == TextAlign::JustifyAll);
    let total_space: f32 = spaces.iter().map(|s| s.2).sum();
    let ratio = if justify && total_space > 0.0 { ((target - width + left_hang + right_hang) / total_space).clamp(-0.2, 0.33) } else { 0.0 };
    let shift_at = |x: f32| spaces.iter().filter(|(_, sx, a)| x >= sx + a - 1e-3).map(|s| s.2 * ratio).sum::<f32>();
    for g in &mut line.glyphs {
        g.x += shift_at(g.x);
    }
    for d in &mut line.decorations {
        d.x0 += shift_at(d.x0);
        d.x1 += shift_at(d.x1);
    }
    for (i, c) in line.clusters.iter_mut().enumerate() {
        c.x += shift_at(c.x);
        if spaces.iter().any(|s| s.0 == i) {
            c.advance *= 1.0 + ratio;
        }
    }
    width += total_space * ratio;
    let slack = target - width + left_hang + right_hang;
    let dx = -left_hang
        + match align {
            TextAlign::Center | TextAlign::JustifyCenter if !justify => slack * 0.5,
            TextAlign::Right | TextAlign::JustifyRight if !justify => slack,
            _ => 0.0,
        };
    for g in &mut line.glyphs {
        g.x += dx;
    }
    for d in &mut line.decorations {
        d.x0 += dx;
        d.x1 += dx;
    }
    for c in &mut line.clusters {
        c.x += dx;
    }
    if let Some(info) = line.lines.first_mut() {
        info.x0 += dx;
        info.x1 = info.x0 + width;
    }
    width - left_hang - right_hang
}

/// Use the existing rasterizer with a bounded allocation, then retain document pixels only.
/// Huge transformed ink is rejected before raster allocation, including sparse offscreen text.
pub fn render_bounded(
    composition: &Composition,
    t: &TextLayer,
    format: photocraft_color::PixelFormat,
    clip: photocraft_geom::Rect,
) -> Result<crate::render::Rendered, Error> {
    if let Some([x0, y0, x1, y1]) = composition.layout.bounds() {
        let points = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(x, y)| t.transform.apply(photocraft_geom::Point::new(f64::from(x), f64::from(y))));
        let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        if (max_x - min_x) * (max_y - min_y) > 16_777_216.0 {
            return Err(Error("frame cache exceeds the pixel budget"));
        }
    }
    let warp = crate::render::layout_warp(&composition.layout, t.warp.as_ref());
    let ink = crate::render::ink_rect_warped(&composition.layout, &t.transform, warp.as_ref());
    if u64::from(ink.width()) * u64::from(ink.height()) > 16_777_216 {
        return Err(Error("frame cache exceeds the pixel budget"));
    }
    let rendered = crate::render::rasterize_warped(&composition.layout, &t.transform, format, t.antialias, warp.as_ref());
    let rect = rendered.rect.intersect(&clip);
    let mut surface = photocraft_raster::Surface::new(rendered.surface.format());
    if !rect.is_empty() {
        surface.write_region(rect, &rendered.surface.read_region(rect));
        surface.prune();
    }
    Ok(crate::render::Rendered { surface, rect })
}
