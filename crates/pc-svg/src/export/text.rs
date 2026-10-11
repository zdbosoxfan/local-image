//! Text/outlines use only pc-text's shaped layout and public outlines API.
//! Writer organization adapted from VectorCraft svg/export.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
use super::*;
use photocraft_text::render::PathEl;

impl Writer<'_, '_, '_> {
    pub(super) fn text(&mut self, t: &photocraft_doc::TextLayer) -> Result<()> {
        let layout = self.cx.text.layout(t, self.doc.resolution_dpi);
        let warp = photocraft_text::render::layout_warp(&layout, t.warp.as_ref());
        if warp.is_some() && !layout.decorations.is_empty() {
            return Err(invalid("warped decorations require raster fallback"));
        }
        let curves = self.opts.text_as_curves
            || warp.is_some()
            || layout.vertical
            || t.text.chars().any(|c| c == '\u{fffe}' || c == '\u{ffff}' || c < ' ' && !matches!(c, '\t' | '\n' | '\r'))
            || layout.decorations.iter().any(|d| layout.styles[d.style as usize].color.alpha != 1.0)
            || !layout.decorations.is_empty() && layout.styles.iter().any(|s| s.color != layout.styles[0].color)
            || layout.styles.iter().any(|s| {
                s.faux_italic
                    || s.horizontal_scale != 1.0
                    || s.vertical_scale != 1.0
                    || !s.features.is_empty()
                    || s.kerning == photocraft_doc::text::Kerning::Optical
                    || s.kern != 0.0
                    || s.caps != photocraft_doc::text::Caps::Normal
                    || !s.variations.is_empty()
            });
        if curves {
            if !self.opts.text_as_curves {
                self.warn("text outlines", "Text with unsupported SVG styles was converted to curves.");
            }
            // Native pc-text accumulates all glyphs/decorations of one colour together,
            // in first-colour order. Keep that union when translucent strokes overlap.
            let mut colors = Vec::new();
            for style in &layout.styles {
                if !colors.contains(&style.color) {
                    colors.push(style.color);
                }
            }
            for color in colors {
                let mut part = layout.clone();
                part.glyphs.retain(|g| layout.styles[g.style as usize].color == color);
                let mut data = String::new();
                for glyph in photocraft_text::render::outlines(&part, &t.transform, warp.as_ref()) {
                    check_cancel(self.cx)?;
                    for el in glyph {
                        let point = |p: [f64; 2]| -> Result<String> {
                            if p.iter().any(|n| !n.is_finite() || n.abs() > 1_000_000.0) {
                                return Err(invalid("text outline exceeds coordinate bounds"));
                            }
                            Ok(format!("{} {}", self.n(p[0]), self.n(p[1])))
                        };
                        let command = match el {
                            PathEl::MoveTo(p) => format!("M{}", point(p)?),
                            PathEl::LineTo(p) => format!("L{}", point(p)?),
                            PathEl::QuadTo(c, p) => format!("Q{} {}", point(c)?, point(p)?),
                            PathEl::CurveTo(a, b, p) => format!("C{} {} {}", point(a)?, point(b)?, point(p)?),
                            PathEl::Close => "Z".into(),
                        };
                        if data.len().saturating_add(command.len()).saturating_add(self.body.len()).saturating_add(self.defs.len()) > self.opts.max_bytes {
                            return Err(ExportError::Budget("text outline bytes"));
                        }
                        data.push_str(&command);
                    }
                }
                for d in &layout.decorations {
                    if layout.styles[d.style as usize].color != color {
                        continue;
                    }
                    let points = [(d.x0, d.y0), (d.x1, d.y0), (d.x1, d.y1), (d.x0, d.y1)]
                        .map(|(x, y)| t.transform.apply(photocraft_geom::Point::new(f64::from(x), f64::from(y))));
                    let command = format!(
                        "M{} {}L{} {}L{} {}L{} {}Z",
                        self.n(points[0].x),
                        self.n(points[0].y),
                        self.n(points[1].x),
                        self.n(points[1].y),
                        self.n(points[2].x),
                        self.n(points[2].y),
                        self.n(points[3].x),
                        self.n(points[3].y)
                    );
                    if data.len().saturating_add(command.len()).saturating_add(self.body.len()).saturating_add(self.defs.len()) > self.opts.max_bytes {
                        return Err(ExportError::Budget("text outline bytes"));
                    }
                    data.push_str(&command);
                }
                if !data.is_empty() {
                    self.line(&format!("<path d=\"{data}\" fill=\"{}\" fill-opacity=\"{}\"/>", paint::color(&color), self.n(f64::from(color.alpha))))?;
                }
            }
            return Ok(());
        } else {
            self.warn("fonts", "Editable SVG text uses named fonts; choose Text as curves for portable glyph geometry.");
            let matrix = t.transform.m.iter().map(|n| self.n(*n)).collect::<Vec<_>>().join(" ");
            self.line(&format!("<text xml:space=\"preserve\" transform=\"matrix({matrix})\">"))?;
            for cluster in &layout.clusters {
                if cluster.range.is_empty() {
                    continue;
                }
                let Some(line) = layout.lines.get(cluster.line) else {
                    return Err(invalid("text cluster line is missing"));
                };
                let mut start = 0;
                let mut style = None;
                for run in t.char_runs() {
                    let end = start + run.len;
                    if cluster.range.start >= start && cluster.range.start < end {
                        style = Some(run.style);
                        break;
                    }
                    start = end;
                }
                let st = style.ok_or_else(|| invalid("text cluster style is missing"))?;
                let text = t.text.get(cluster.range.clone()).ok_or_else(|| invalid("text cluster range is invalid"))?;
                let family = if st.font_family.is_empty() { "Inter" } else { &st.font_family };
                let italic = if st.italic { "italic" } else { "normal" };
                self.line(&format!("<tspan x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" font-style=\"{italic}\" fill=\"{}\" fill-opacity=\"{}\">{}</tspan>",self.n(f64::from(cluster.x)),self.n(f64::from(line.baseline-st.baseline_shift_pt*layout.px_per_pt)),escape(family),self.n(f64::from(st.size_pt*layout.px_per_pt)),st.weight,paint::color(&st.color),self.n(f64::from(st.color.alpha)),escape(text)))?;
            }
            self.line("</text>")?;
        }
        let matrix = t.transform.m.iter().map(|n| self.n(*n)).collect::<Vec<_>>().join(" ");
        for d in &layout.decorations {
            let style = layout.styles.get(d.style as usize).ok_or_else(|| invalid("decoration style missing"))?;
            self.line(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" transform=\"matrix({matrix})\" fill=\"{}\" fill-opacity=\"{}\"/>",
                self.n(f64::from(d.x0)),
                self.n(f64::from(d.y0)),
                self.n(f64::from(d.x1 - d.x0)),
                self.n(f64::from(d.y1 - d.y0)),
                paint::color(&style.color),
                self.n(f64::from(style.color.alpha))
            ))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    #[test]
    fn dsfix_warped_decorations_fail_before_curve_output() {
        let mut doc = Document::new("warped text", photocraft_doc::Size::new(128, 128), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
        let text = photocraft_doc::TextLayer {
            text: "H".into(),
            size_pt: 24.,
            warp: Some(photocraft_doc::text::TextWarp { style: "warpArc".into(), value: 25., ..Default::default() }),
            runs: vec![photocraft_doc::text::TextRun {
                len: 1,
                style: photocraft_doc::text::CharStyle { size_pt: 24., underline: true, ..Default::default() },
            }],
            ..Default::default()
        };
        doc.layers.push(Layer::new("type", LayerContent::Text(text.clone())));
        for text_as_curves in [false, true] {
            let opts = ExportOptions { text_as_curves, ..Default::default() };
            let mut engine = photocraft_text::TextEngine::new();
            let mut cx = ExportContext { cancelled: &|| false, projection: None, text: &mut engine };
            let mut writer = Writer {
                doc: &doc,
                opts: &opts,
                cx: &mut cx,
                rect: doc.bounds(),
                defs: String::new(),
                body: String::new(),
                resources: BTreeMap::new(),
                warnings: BTreeMap::new(),
                next: 0,
                label: None,
            };
            let error = writer.text(&text).unwrap_err();
            assert!(error.to_string().contains("warped decorations"), "{error}");
            assert!(writer.body.is_empty());
            let output = export(&doc, &ExportScope::Document, &opts).unwrap();
            assert!(String::from_utf8(output.bytes).unwrap().contains("data:image/png;base64,"));
        }
    }
}
