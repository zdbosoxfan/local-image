//! Editable Affinity text conversion and bounded layout.
use super::{cancelled, paint, scene::State};
use crate::{Error, model};
use photocraft_doc::{Affine, Layer, LayerContent, PixelFormat};
use photocraft_doc::{
    TextLayer,
    text::{Caps, CharStyle, FontFeature, ParagraphRun, ParagraphStyle, TextAlign, TextRun, TextShape},
};
use photocraft_geom::Point;
impl State<'_, '_> {
    pub(super) fn text(&mut self, t: &model::Text, m: Affine, name: &str) -> Result<Layer, Error> {
        let (text, layout) = self.layout_text(t, m, name)?;
        self.render_text(t, text, layout, name)
    }

    fn layout_text(&mut self, t: &model::Text, m: Affine, name: &str) -> Result<(TextLayer, photocraft_text::TextLayout), Error> {
        cancelled(Some(self.cancel))?;
        let text_bytes = t.runs.iter().fold(0u64, |total, run| total.saturating_add(run.text.len() as u64));
        // Reserve a conservative allowance for glyph/layout work before the text engine allocates.
        self.bytes = self.bytes.saturating_add(text_bytes.saturating_mul(256)).saturating_add(metadata_bytes(&t.runs).saturating_mul(8));
        if self.bytes > self.options.limits.decompressed_bytes {
            return Err(Error::Limit("text layout bytes"));
        }
        let mut text = TextLayer::default();
        let type_ratio = if t.frame.is_some() { t.text_transform.scale() / t.transform.scale().max(1e-9) } else { 1. };
        if !type_ratio.is_finite() || !(0.0001..=10000.).contains(&type_ratio) {
            return Err(Error::Limit("text scale"));
        }
        if t.frame.is_some()
            && (t.transform.0[1].abs() > 1e-6 || t.transform.0[2].abs() > 1e-6 || (t.transform.0[0].abs() - t.transform.0[3].abs()).abs() > 1e-6)
        {
            self.warn("text-transform", name, "Frame type scaling under a rotated or nonuniform transform is approximated; text remains editable.");
        }
        let pt = 72. / f64::from(self.options.dpi) * type_ratio;
        for r in &t.runs {
            if !r.size.is_finite()
                || !(0.01..=100_000.).contains(&r.size)
                || !r.tracking.is_finite()
                || r.tracking.abs() > 1000.
                || r.leading.is_some_and(|v| !v.is_finite() || v.abs() > 100_000.)
            {
                return Err(Error::Limit("text attributes"));
            }
            let c = match r.fill {
                crate::paint::Paint::None => {
                    let mut color = photocraft_doc::Color::BLACK;
                    color.alpha = 0.;
                    color
                }
                crate::paint::Paint::Solid(c) => {
                    if matches!(c, crate::paint::Color::Cmyk { .. }) {
                        self.warn("colour-profile", name, "CMYK text colour was converted without the source profile.");
                    }
                    paint::color(c)
                }
                _ => {
                    self.warn("text-paint", name, "Text paint imported as solid black.");
                    photocraft_doc::Color::BLACK
                }
            };
            text.text.push_str(&r.text);
            text.paragraphs.push(ParagraphRun {
                len: r.text.len(),
                style: ParagraphStyle {
                    align: match r.paragraph.align {
                        model::Align::Left => TextAlign::Left,
                        model::Align::Center => TextAlign::Center,
                        model::Align::Right => TextAlign::Right,
                        model::Align::Justify => TextAlign::JustifyLeft,
                    },
                    start_indent_pt: (r.paragraph.left * pt) as f32,
                    end_indent_pt: (r.paragraph.right * pt) as f32,
                    first_line_indent_pt: ((r.paragraph.first - r.paragraph.left) * pt) as f32,
                    space_before_pt: (r.paragraph.space_before * pt) as f32,
                    space_after_pt: (r.paragraph.space_after * pt) as f32,
                    auto_leading: r.paragraph.auto_leading as f32,
                    ..Default::default()
                },
            });
            text.runs.push(TextRun {
                len: r.text.len(),
                style: CharStyle {
                    font_family: r.family.clone(),
                    postscript_name: Some(r.postscript.clone()),
                    weight: r.weight.clamp(100, 900) as u16,
                    italic: r.italic,
                    size_pt: (r.size * pt) as f32,
                    tracking: (r.tracking * 1000.) as f32,
                    underline: r.underline,
                    strikethrough: r.strikethrough,
                    horizontal_scale: r.h_scale as f32,
                    leading_pt: r.leading.or(r.paragraph.leading).map(|v| (v * pt) as f32),
                    color: c,
                    caps: if r.all_caps { Caps::AllCaps } else { Caps::Normal },
                    features: r.features.iter().map(|s| FontFeature { tag: s.trim_start_matches('-').into(), value: u16::from(!s.starts_with('-')) }).collect(),
                    ..Default::default()
                },
            });
        }
        // Font sizes are in node pixels; the node transform applies the DPI conversion once.
        let anchor = t.frame.map_or(t.anchor, |r| model::Point { x: r.x0, y: r.y0 });
        text.transform = m.mul(&Affine { m: t.transform.0 }).mul(&Affine::translate(anchor.x, anchor.y));
        self.affine(text.transform)?;
        if let Some(b) = t.frame {
            let (w, h) = (b.x1 - b.x0, b.y1 - b.y0);
            if !w.is_finite() || !h.is_finite() || w <= 0. || h <= 0. || w > 1_000_000. || h > 1_000_000. {
                return Err(Error::Limit("text frame"));
            }
            text.shape = TextShape::Box { x: 0., y: 0., width: w as f32, height: h as f32 };
        }
        if let Some(r) = text.runs.first() {
            text.font_family = r.style.font_family.clone();
            text.size_pt = r.style.size_pt;
            text.color = r.style.color;
        }
        if self.fonts.is_none() && self.bundled.is_none() {
            self.bundled = Some(photocraft_text::TextEngine::new());
        }
        let fonts = self.fonts.as_deref_mut().or(self.bundled.as_mut()).ok_or(Error::Unsupported("text engine"))?;
        let missing = text.runs.iter().any(|r| !fonts.fonts.has_family(&r.style.font_family));
        let layout = fonts.layout(&text, self.options.dpi);
        if missing {
            self.warn("font-substitution", name, "Unavailable font substituted; text remains editable.");
        }
        Ok((text, layout))
    }

    fn render_text(&mut self, t: &model::Text, mut text: TextLayer, layout: photocraft_text::TextLayout, name: &str) -> Result<Layer, Error> {
        self.charge(photocraft_text::render::ink_rect(&layout, &text.transform))?;
        text.cache = Some(photocraft_text::render::rasterize(&layout, &text.transform, PixelFormat::RGBA32F, text.antialias).surface);
        cancelled(Some(self.cancel))?;
        self.supported += 1;
        let transform = text.transform;
        let mut layer = Layer::new(name, LayerContent::Text(text));
        let outlined: Vec<_> = t.runs.iter().filter_map(|r| r.stroke.as_ref()).collect();
        if outlined.is_empty() {
            return Ok(layer);
        }
        if outlined.len() == t.runs.len()
            && outlined.iter().all(|s| *s == outlined[0] && s.dash.is_none())
            && t.runs.iter().all(|r| matches!(r.fill, crate::paint::Paint::Solid(color) if color.alpha() == 1.))
        {
            let stroke = outlined[0];
            let b = super::scene::bounds(&layer);
            let Some(native) = paint::stroke(self, stroke, transform, (f64::from(b.x0), f64::from(b.y0), f64::from(b.x1), f64::from(b.y1)), name)? else {
                return Ok(layer);
            };
            if native.width > 512. {
                return self.outlined_text(t, &layout, transform, layer, name);
            }
            let fxpaint = match native.paint {
                photocraft_doc::Fill::Solid(color) => photocraft_doc::effects::FxPaint::Color(color),
                _ => {
                    self.warn("text-outline-paint", name, "Gradient text outline retained as companion geometry.");
                    return self.outlined_text(t, &layout, transform, layer, name);
                }
            };
            layer.effects.enabled = true;
            layer.effects.items.push(photocraft_doc::effects::Effect::Stroke(photocraft_doc::effects::StrokeFx {
                common: photocraft_doc::effects::FxCommon::new(photocraft_doc::BlendMode::Normal, 1.),
                size: native.width,
                position: match stroke.align {
                    crate::paint::Align::Inside => photocraft_doc::effects::StrokePosition::Inside,
                    crate::paint::Align::Outside => photocraft_doc::effects::StrokePosition::Outside,
                    crate::paint::Align::Center => photocraft_doc::effects::StrokePosition::Center,
                },
                paint: fxpaint,
            }));
            return Ok(layer);
        }
        self.outlined_text(t, &layout, transform, layer, name)
    }
}

impl State<'_, '_> {
    fn outlined_text(&mut self, t: &model::Text, layout: &photocraft_text::TextLayout, transform: Affine, layer: Layer, name: &str) -> Result<Layer, Error> {
        use photocraft_text::render::{OutlineEvent, OutlineVisitError, OutlineVisitLimits, PathEl, visit_outlines};
        let mut paths = vec![model::Path::default(); t.runs.len()];
        let mut run = None;
        let mut current: Option<model::SubPath> = None;
        let limits = OutlineVisitLimits {
            max_glyphs: 1 << 20,
            max_segments: self.options.limits.segments.saturating_sub(self.segments),
            max_scratch_bytes: self.options.limits.decompressed_bytes.saturating_sub(self.bytes).min(32 << 20) as usize,
        };
        let mut bytes = self.bytes;
        visit_outlines(layout, &transform, None, limits, &|| cancelled(Some(self.cancel)).is_err(), |event| -> Result<(), Error> {
            let finish = |current: &mut Option<model::SubPath>, run: Option<usize>, paths: &mut Vec<model::Path>| {
                if let (Some(path), Some(run)) = (current.take(), run) {
                    paths[run].subpaths.push(path);
                }
            };
            match event {
                OutlineEvent::Glyph { index, .. } => {
                    finish(&mut current, run, &mut paths);
                    let style = layout.glyphs[index].style as usize;
                    run = t.runs.get(style).and_then(|r| r.stroke.as_ref()).map(|_| style);
                }
                OutlineEvent::Element(element) if run.is_some() => {
                    bytes = bytes.saturating_add(std::mem::size_of::<model::SubPath>() as u64 + 2 * std::mem::size_of::<[model::Point; 3]>() as u64);
                    if bytes > self.options.limits.decompressed_bytes {
                        return Err(Error::Limit("text outline bytes"));
                    }
                    let point = |[x, y]: [f64; 2]| model::Point { x, y };
                    match element {
                        PathEl::MoveTo(p) => {
                            finish(&mut current, run, &mut paths);
                            current = Some(model::SubPath { start: point(p), segments: vec![], closed: false });
                        }
                        PathEl::Close => {
                            if let Some(p) = &mut current {
                                p.closed = true;
                            }
                            finish(&mut current, run, &mut paths);
                        }
                        PathEl::LineTo(end) => {
                            if let Some(p) = &mut current {
                                let from = p.segments.last().map_or(p.start, |s| s[2]);
                                p.segments.push([from, point(end), point(end)]);
                            }
                        }
                        PathEl::CurveTo(a, b, end) => {
                            if let Some(p) = &mut current {
                                p.segments.push([point(a), point(b), point(end)]);
                            }
                        }
                        PathEl::QuadTo(c, end) => {
                            if let Some(p) = &mut current {
                                let from = p.segments.last().map_or(p.start, |s| s[2]);
                                let a = model::Point { x: from.x + (c[0] - from.x) * 2. / 3., y: from.y + (c[1] - from.y) * 2. / 3. };
                                let b = model::Point { x: end[0] + (c[0] - end[0]) * 2. / 3., y: end[1] + (c[1] - end[1]) * 2. / 3. };
                                p.segments.push([a, b, point(end)]);
                            }
                        }
                    }
                }
                OutlineEvent::Element(_) => {}
            }
            Ok(())
        })
        .map_err(|e| match e {
            OutlineVisitError::Cancelled => Error::Cancelled,
            OutlineVisitError::Budget(_) => Error::Limit("text outline work"),
            OutlineVisitError::Visitor(e) => e,
            OutlineVisitError::InvalidOutline => Error::Malformed("glyph outline"),
        })?;
        self.bytes = bytes;
        let mut parts = vec![layer];
        for (path, run) in paths.into_iter().zip(&t.runs) {
            if let Some(stroke) = &run.stroke {
                if path.subpaths.is_empty() {
                    continue;
                }
                self.charge_layers(1)?;
                let native = self.path(&path, Affine::IDENTITY, false)?;
                // The glyph visitor already placed the path; only paint/width retain the type transform.
                parts.push(self.shape(native, &[], std::slice::from_ref(stroke), transform, "Text outline")?);
            }
        }
        self.charge_layers(1)?;
        Ok(Layer::group(name, parts))
    }

    pub(super) fn table(&mut self, table: &model::Table, m: Affine, name: &str) -> Result<Layer, Error> {
        let columns = table.ncols();
        self.charge_layers(table.cells.len().saturating_mul(2).saturating_add(table.vertical.len()).saturating_add(table.horizontal.len()))?;
        let mut parts = Vec::new();
        for (index, cell) in table.cells.iter().enumerate() {
            if cell.merged_left {
                continue;
            }
            let (column, row) = (index % columns, index / columns);
            let mut end = column + 1;
            while end < columns && table.cells.get(row * columns + end).is_some_and(|c| c.merged_left) {
                end += 1;
            }
            let [left, top, right, bottom] = cell.inset;
            let bounds =
                model::Rect { x0: table.columns[column] + left, y0: table.rows[row] + top, x1: table.columns[end] - right, y1: table.rows[row + 1] - bottom };
            if bounds.x1 <= bounds.x0 || bounds.y1 <= bounds.y0 {
                self.warn("table-insets", name, "Cell insets leave no text area; cell text omitted.");
                continue;
            }
            let clone_bytes = metadata_bytes(&cell.runs).saturating_add(cell.runs.iter().map(|r| r.text.len() as u64).sum::<u64>()).saturating_mul(2);
            self.bytes = self.bytes.saturating_add(clone_bytes);
            if self.bytes > self.options.limits.decompressed_bytes {
                return Err(Error::Limit("table text bytes"));
            }
            let source = model::Text {
                runs: cell.runs.clone(),
                align: cell.runs.first().map_or(model::Align::Left, |r| r.paragraph.align),
                anchor: model::Point { x: bounds.x0, y: bounds.y0 },
                frame: Some(model::Rect { y1: bounds.y0 + 1_000_000., ..bounds }),
                transform: table.transform,
                text_transform: table.text_transform,
            };
            let (mut text, layout) = self.layout_text(&source, m, "Table cell")?;
            if let Some(last) = layout.lines.last() {
                let height = bounds.y1 - bounds.y0;
                let baseline = f64::from(last.baseline);
                let frame_height = height.max(baseline + f64::from(last.descent) + 0.05);
                let share = match cell.valign {
                    model::VAlign::Top => 0.,
                    model::VAlign::Center => 0.5,
                    model::VAlign::Bottom => 1.,
                };
                let shift = (height - baseline) * share;
                text.transform = text.transform.mul(&Affine::translate(0., shift));
                text.shape = TextShape::Box { x: 0., y: 0., width: (bounds.x1 - bounds.x0) as f32, height: frame_height as f32 };
                self.affine(text.transform)?;
            }
            parts.push(self.render_text(&source, text, layout, "Table cell")?);
        }
        let at = m.mul(&Affine { m: table.transform.0 });
        for (edges, vertical) in [(&table.vertical, true), (&table.horizontal, false)] {
            for (index, stroke) in edges.iter().enumerate() {
                let Some(stroke) = stroke else { continue };
                let (a, b) = if vertical {
                    let row = index / (columns + 1);
                    let column = index % (columns + 1);
                    (Point::new(table.columns[column], table.rows[row]), Point::new(table.columns[column], table.rows[row + 1]))
                } else {
                    let row = index / columns;
                    let column = index % columns;
                    (Point::new(table.columns[column], table.rows[row]), Point::new(table.columns[column + 1], table.rows[row]))
                };
                let path = photocraft_doc::Path::new(vec![photocraft_doc::Subpath {
                    knots: vec![photocraft_doc::Knot::corner(a.x, a.y), photocraft_doc::Knot::corner(b.x, b.y)],
                    closed: false,
                    op: photocraft_doc::PathOp::Combine,
                }])
                .transform(&at);
                parts.push(self.shape(path, &[], std::slice::from_ref(stroke), m, "Table grid")?);
            }
        }
        Ok(Layer::group(name, parts))
    }
}

fn metadata_bytes(runs: &[model::TextRun]) -> u64 {
    runs.iter().fold(0u64, |total, run| {
        let stops = |paint: &crate::paint::Paint| match paint {
            crate::paint::Paint::Gradient(g) => g.stops.len() as u64 * std::mem::size_of::<crate::paint::Stop>() as u64,
            _ => 0,
        };
        let outline = run.stroke.as_ref().map_or(0, |stroke| stops(&stroke.paint) + stroke.dash.as_ref().map_or(0, |(d, _)| d.len() as u64 * 8));
        total
            .saturating_add(std::mem::size_of::<model::TextRun>() as u64 * 2)
            .saturating_add(run.family.len() as u64 + run.postscript.len() as u64)
            .saturating_add(run.features.iter().map(|s| (s.len() + std::mem::size_of::<String>()) as u64).sum::<u64>())
            .saturating_add(stops(&run.fill))
            .saturating_add(outline)
    })
}
