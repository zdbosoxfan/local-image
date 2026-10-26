//! Editable Affinity text conversion and bounded layout.
use super::{cancelled, paint, scene::State};
use crate::{Error, model};
use photocraft_doc::{Affine, Layer, LayerContent, PixelFormat};
use photocraft_doc::{
    TextLayer,
    text::{Caps, CharStyle, FontFeature, ParagraphRun, ParagraphStyle, TextAlign, TextRun, TextShape},
};
impl State<'_, '_> {
    pub(super) fn text(&mut self, t: &model::Text, m: Affine, name: &str) -> Result<Layer, Error> {
        cancelled(Some(self.cancel))?;
        let text_bytes = t.runs.iter().fold(0u64, |total, run| total.saturating_add(run.text.len() as u64));
        // Reserve a conservative allowance for glyph/layout work before the text engine allocates.
        self.bytes = self.bytes.saturating_add(text_bytes.saturating_mul(256));
        if self.bytes > self.options.limits.decompressed_bytes {
            return Err(Error::Limit("text layout bytes"));
        }
        let mut text = TextLayer::default();
        let pt = 72. / f64::from(self.options.dpi);
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
            text.runs.push(TextRun {
                len: r.text.len(),
                style: CharStyle {
                    font_family: r.family.clone(),
                    postscript_name: Some(r.postscript.clone()),
                    weight: r.weight.clamp(100, 900) as u16,
                    italic: r.italic,
                    size_pt: (r.size * pt) as f32,
                    tracking: (r.tracking * 1000.) as f32,
                    leading_pt: r.leading.map(|v| (v * pt) as f32),
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
        text.paragraphs = vec![ParagraphRun {
            len: text.text.len(),
            style: ParagraphStyle {
                align: match t.align {
                    model::Align::Left => TextAlign::Left,
                    model::Align::Center => TextAlign::Center,
                    model::Align::Right => TextAlign::Right,
                    model::Align::Justify => TextAlign::JustifyLeft,
                },
                ..Default::default()
            },
        }];
        if self.fonts.is_none() && self.bundled.is_none() {
            self.bundled = Some(photocraft_text::TextEngine::new());
        }
        let fonts = self.fonts.as_deref_mut().or(self.bundled.as_mut()).ok_or(Error::Unsupported("text engine"))?;
        let missing = text.runs.iter().any(|r| !fonts.fonts.has_family(&r.style.font_family));
        let layout = fonts.layout(&text, self.options.dpi);
        let r = photocraft_text::render::ink_rect(&layout, &text.transform);
        self.charge(r)?;
        if missing {
            self.warn("font-substitution", name, "Unavailable font substituted; text remains editable.");
        }
        let fonts = self.fonts.as_deref_mut().or(self.bundled.as_mut()).ok_or(Error::Unsupported("text engine"))?;
        fonts.render_layer(&mut text, self.options.dpi, PixelFormat::RGBA32F);
        cancelled(Some(self.cancel))?;
        self.supported += 1;
        Ok(Layer::new(name, LayerContent::Text(text)))
    }
}
