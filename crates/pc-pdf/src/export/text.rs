//! Positioned-font/outline organization adapted from VectorCraft pdf/export.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Only pc-text shapes. Its current public layout lacks per-glyph cluster ranges;
//! embed only layouts whose one-glyph-per-cluster correspondence is provable.
//! Other layouts use its authoritative outlines and report the loss of text semantics.
use super::*;
use photocraft_text::layout::TextLayout;
use photocraft_text::render::{OutlineVisitError, OutlineVisitLimits, PaintedOutlineEvent, PathEl};
use std::ops::Range;
impl Writer<'_, '_, '_> {
    pub(super) fn text(&mut self, s: &mut Surface<'_>, t: &photocraft_doc::TextLayer) -> Result<()> {
        if t.text.chars().take(100_001).count() > 100_000 {
            return Err(ExportError::Budget("text characters"));
        }
        check_cancel(self.cx)?;
        let layout = self.cx.text.layout(t, self.doc.resolution_dpi);
        if layout.glyphs.len() > 100_000 {
            return Err(ExportError::Budget("text glyphs"));
        }
        let warp = photocraft_text::render::layout_warp(&layout, t.warp.as_ref());
        let ranges = exact_ranges(&layout, self.cx)?;
        let fonts = layout
            .faces
            .iter()
            .map(|f| {
                if f.coords.iter().any(|v| *v != 0) || !subset_allowed(f.font.data.as_ref(), f.font.index) {
                    return None;
                }
                let (data, _) = f.font.data.clone().into_raw_parts();
                krilla::text::Font::new(data.into(), f.font.index)
            })
            .collect::<Option<Vec<_>>>();
        if self.opts.text_as_curves || warp.is_some() || layout.vertical || fonts.is_none() || ranges.is_none() {
            if !self.opts.text_as_curves {
                self.warn("text outlines", "Text without an embeddable font or exact public glyph-to-Unicode mapping was converted to pc-text outlines.");
            }
            self.outlines(s, t, &layout, warp.as_ref())?;
            return Ok(());
        }
        let fonts = fonts.ok_or_else(|| invalid("font preflight disappeared"))?;
        let ranges = ranges.ok_or_else(|| invalid("glyph mapping disappeared"))?;
        s.push_transform(&affine(t.transform));
        let result = (|| {
            for (g, range) in layout.glyphs.iter().zip(ranges) {
                check_cancel(self.cx)?;
                let f = layout.faces.get(g.face as usize).ok_or_else(|| invalid("glyph face missing"))?;
                let style = layout.styles.get(g.style as usize).ok_or_else(|| invalid("glyph style missing"))?;
                let hs = style.horizontal_scale.max(0.000001);
                let vs = style.vertical_scale.max(0.000001);
                let angle = if style.faux_italic { 12.0 } else { 0.0 } + f.skew_deg;
                let skew = angle.to_radians().tan() * vs;
                let y = g.y - style.baseline_shift_pt * layout.px_per_pt;
                s.push_transform(&Transform::from_row(hs, 0.0, -skew, vs, g.x, y));
                s.set_stroke(None);
                s.set_fill(Some(krilla::paint::Fill { paint: paint::color(&style.color).into(), opacity: norm(style.color.alpha), rule: Default::default() }));
                let glyph = krilla::text::KrillaGlyph::new(krilla::text::GlyphId::new(g.id), 0.0, 0.0, 0.0, 0.0, range, None);
                s.draw_glyphs(krilla::geom::Point::from_xy(0.0, 0.0), &[glyph], fonts[g.face as usize].clone(), &t.text, f.size_px, false);
                s.pop();
            }
            self.decorations(s, &layout, photocraft_geom::Affine::IDENTITY)
        })();
        s.pop();
        result
    }
    fn outlines(&mut self, s: &mut Surface<'_>, t: &photocraft_doc::TextLayer, layout: &TextLayout, warp: Option<&photocraft_text::warp::Warp>) -> Result<()> {
        let mut color = None;
        let mut builder = PathBuilder::new();
        let flush = |builder: &mut PathBuilder, color: photocraft_doc::Color, surface: &mut Surface<'_>| {
            if let Some(path) = std::mem::replace(builder, PathBuilder::new()).finish() {
                surface.set_stroke(None);
                surface.set_fill(Some(krilla::paint::Fill { paint: paint::color(&color).into(), opacity: norm(color.alpha), rule: Default::default() }));
                surface.draw_path(&path);
            }
        };
        let limits = OutlineVisitLimits {
            max_glyphs: 100_000,
            max_segments: 1_000_000.min(self.opts.max_bytes / std::mem::size_of::<PathEl>()),
            max_scratch_bytes: self.opts.max_bytes.min(16 << 20),
        };
        photocraft_text::render::visit_painted_outlines(layout, &t.transform, warp, limits, self.cx.cancelled, |event| {
            match event {
                PaintedOutlineEvent::Paint(next) => {
                    if color != Some(next) {
                        if let Some(previous) = color {
                            flush(&mut builder, previous, s);
                        }
                        color = Some(next);
                    }
                }
                PaintedOutlineEvent::Element(element) => {
                    let point = |p: [f64; 2]| -> Result<[f32; 2]> {
                        if p.iter().any(|n| !n.is_finite() || n.abs() > 1_000_000.0) {
                            return Err(invalid("text outline exceeds bounds"));
                        }
                        Ok([p[0] as f32, p[1] as f32])
                    };
                    match element {
                        PathEl::MoveTo(p) => {
                            let p = point(p)?;
                            builder.move_to(p[0], p[1]);
                        }
                        PathEl::LineTo(p) => {
                            let p = point(p)?;
                            builder.line_to(p[0], p[1]);
                        }
                        PathEl::QuadTo(c, p) => {
                            let c = point(c)?;
                            let p = point(p)?;
                            builder.quad_to(c[0], c[1], p[0], p[1]);
                        }
                        PathEl::CurveTo(a, z, p) => {
                            let a = point(a)?;
                            let z = point(z)?;
                            let p = point(p)?;
                            builder.cubic_to(a[0], a[1], z[0], z[1], p[0], p[1]);
                        }
                        PathEl::Close => builder.close(),
                    }
                }
            }
            Ok(())
        })
        .map_err(|error| match error {
            OutlineVisitError::Cancelled => ExportError::Cancelled,
            OutlineVisitError::Budget(kind) => ExportError::Budget(kind),
            OutlineVisitError::Visitor(error) => error,
            OutlineVisitError::InvalidOutline => invalid("invalid text outline"),
        })?;
        if let Some(color) = color {
            flush(&mut builder, color, s);
        }
        Ok(())
    }
    fn decorations(&mut self, s: &mut Surface<'_>, layout: &TextLayout, transform: photocraft_geom::Affine) -> Result<()> {
        for d in &layout.decorations {
            let st = layout.styles.get(d.style as usize).ok_or_else(|| invalid("decoration style missing"))?;
            let points =
                [(d.x0, d.y0), (d.x1, d.y0), (d.x1, d.y1), (d.x0, d.y1)].map(|(x, y)| transform.apply(photocraft_geom::Point::new(f64::from(x), f64::from(y))));
            let path = Path::new(vec![photocraft_doc::Subpath::polygon(&points.map(|p| (p.x, p.y)))]);
            draw_solid(s, &path, st.color, 1.0)?;
        }
        Ok(())
    }
}
fn affine(a: photocraft_geom::Affine) -> Transform {
    let m = a.m;
    Transform::from_row(m[0] as f32, m[1] as f32, m[2] as f32, m[3] as f32, m[4] as f32, m[5] as f32)
}
/// Avoid inventing Unicode for substitutions, marks or ligature continuations.
fn exact_ranges(l: &TextLayout, cx: &ExportContext<'_>) -> Result<Option<Vec<Range<usize>>>> {
    if l.glyphs.len() != l.clusters.len() || l.vertical {
        return Ok(None);
    }
    // Exact public positions only, indexed once to avoid quadratic work on long text.
    // Ambiguous or adjusted positions conservatively use authoritative outlines.
    let mut clusters = BTreeMap::new();
    for c in &l.clusters {
        check_cancel(cx)?;
        let Some(line) = l.lines.get(c.line) else { return Ok(None) };
        if clusters.insert((line.baseline.to_bits(), c.x.to_bits()), c.range.clone()).is_some() {
            return Ok(None);
        }
    }
    let mut ranges = Vec::with_capacity(l.glyphs.len());
    for g in &l.glyphs {
        check_cancel(cx)?;
        let Some(range) = clusters.remove(&(g.y.to_bits(), g.x.to_bits())) else { return Ok(None) };
        ranges.push(range);
    }
    Ok(Some(ranges))
}
/// OS/2 fsType bit restrictions (OpenType specification). TTC offsets are absolute.
/// Malformed font tables refuse embedding and use existing outlines, never a guessed licence.
fn subset_allowed(data: &[u8], index: u32) -> bool {
    let u16_at = |i: usize| data.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let u32_at = |i: usize| data.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let offset = if data.starts_with(b"ttcf") {
        let Some(count) = u32_at(8) else {
            return false;
        };
        if index >= count {
            return false;
        }
        let Some(offset) = u32_at(12 + index as usize * 4) else {
            return false;
        };
        offset as usize
    } else {
        if index != 0 {
            return false;
        }
        0
    };
    let Some(count) = u16_at(offset + 4) else {
        return false;
    };
    for n in 0..usize::from(count) {
        let entry = offset + 12 + n * 16;
        if data.get(entry..entry + 4) == Some(b"OS/2") {
            let Some(table) = u32_at(entry + 8) else {
                return false;
            };
            let Some(bits) = u16_at(table as usize + 8) else {
                return false;
            };
            return bits & (0x0002 | 0x0100 | 0x0200) == 0;
        }
    }
    false
}
