//! XML text placeholders retain draw order, group effects and <use> instances without fontdb.
//! Adapted from VectorCraft crates/svg/src/import/text.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
use super::{ImportError, Result, css::Styles, geometry::MAX_COORD, warn};
use photocraft_doc::{
    Affine, Color, TextLayer,
    text::{CharStyle, ParagraphRun, ParagraphStyle, TextAlign, TextRun},
    vector_design::exchange::{ImportOptions, VectorWarning},
};
use std::{
    collections::HashSet,
    fmt::Write,
    str::FromStr,
    sync::{Arc, atomic::AtomicBool},
};
#[path = "text_opacity.rs"]
mod opacity;
pub(super) use opacity::OutlineNode;

pub(super) struct Slots {
    pub prefix: String,
    pub texts: Vec<TextLayer>,
    pub names: Vec<String>,
    pub markers: Vec<[f32; 4]>,
    pub outlined: Vec<Option<Arc<Vec<OutlineNode>>>>,
}
impl Slots {
    pub fn find(&self, p: &usvg::Path) -> Option<usize> {
        if let Some(i) = p.id().strip_prefix(&self.prefix).and_then(|i| i.parse::<usize>().ok()) {
            return (i < self.texts.len()).then_some(i);
        }
        if !p.id().is_empty() {
            return None;
        }
        // usvg removes IDs inside use. Bounds, colour and an interior open-line
        // fingerprint identify cloned slots without treating a plain rectangle as text.
        let usvg::Paint::Color(c) = p.fill()?.paint() else { return None };
        let i = (usize::from(c.red) << 16) | (usize::from(c.green) << 8) | usize::from(c.blue);
        let b = p.data().bounds();
        let marker = self.markers.get(i)?;
        let near = |a: f32, b: f32| (a - b).abs() <= 1e-3 * b.abs().max(1.0);
        let segments: Vec<_> = p.data().segments().take(8).collect();
        let fingerprint = matches!(segments.as_slice(), [_,_,_,_,_,
            usvg::tiny_skia_path::PathSegment::MoveTo(a),
            usvg::tiny_skia_path::PathSegment::LineTo(b)]
            if near(a.x,marker[0]+0.125) && near(a.y,marker[1]+0.25)
                && near(b.x,marker[0]+0.1875) && near(b.y,marker[1]+0.28125));
        (i < self.texts.len()
            && p.stroke().is_none()
            && fingerprint
            && near(b.x(), marker[0])
            && near(b.y(), marker[1])
            && near(b.width(), marker[2])
            && near(b.height(), marker[3]))
        .then_some(i)
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;")
}
/// usvg 0.45 processes stylesheet !important before inline !important. Scope
/// source rules to source elements, leaving synthetic paths outside the cascade.
/// The same locked simplecss parser used by usvg preserves supported selectors.
fn protect_markers(source: String, slots: &Slots, limit: usize) -> Result<String> {
    if slots.texts.is_empty() {
        return Ok(source);
    }
    let xml = roxmltree::Document::parse(&source).map_err(|e| ImportError::Invalid(e.to_string()))?;
    // Collect names once instead of repeatedly searching attacker-controlled
    // prefixes. Include decoded CSS text, where XML entities can spell a name.
    let mut names = HashSet::new();
    for n in xml.descendants().filter(|n| n.is_element()) {
        names.extend(n.attributes().map(|a| a.name()));
        if n.tag_name().name() == "style" {
            names.extend(n.text().unwrap_or("").split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))));
        }
    }
    let guard = (0usize..)
        .map(|i| format!("data-local-image-svg-source-{i}"))
        .find(|name| !names.contains(name.as_str()))
        .ok_or(ImportError::Limit("text marker namespace"))?;
    let mut edits = Vec::new();
    for n in xml.descendants().filter(|n| n.is_element()) {
        if n.attribute("id").is_some_and(|id| id.starts_with(&slots.prefix)) {
            continue;
        }
        let range = n.range();
        let raw = &source[range.clone()];
        let mut quote = None;
        let end = raw
            .char_indices()
            .find_map(|(i, c)| {
                if matches!(c, '\'' | '"') {
                    if quote == Some(c) {
                        quote = None;
                    } else if quote.is_none() {
                        quote = Some(c);
                    }
                }
                (c == '>' && quote.is_none()).then_some(i)
            })
            .ok_or_else(|| ImportError::Invalid("missing element delimiter".into()))?;
        let at = range.start + if raw[..end].ends_with('/') { end - 1 } else { end };
        edits.push((at..at, format!(" {guard}=\"\"")));
        if n.tag_name().name() == "style" {
            let Some(first) = n.first_child() else { continue };
            let Some(last) = n.last_child() else { continue };
            let sheet = simplecss::StyleSheet::parse(n.text().unwrap_or(""));
            let mut scoped = String::new();
            for rule in &sheet.rules {
                let _ = write!(scoped, "{}[{guard}]{{", rule.selector);
                for d in &rule.declarations {
                    let _ = write!(scoped, "{}:{}{};", d.name, d.value, if d.important { "!important" } else { "" });
                }
                scoped.push('}');
            }
            edits.push((first.range().start..last.range().end, escape(&scoped)));
        }
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = String::new();
    let mut copied = 0;
    for (range, replacement) in edits {
        out.push_str(&source[copied..range.start]);
        out.push_str(&replacement);
        copied = range.end;
        if out.len() > limit {
            return Err(ImportError::Limit("prepared XML byte"));
        }
    }
    out.push_str(&source[copied..]);
    if out.len() > limit {
        return Err(ImportError::Limit("prepared XML byte"));
    }
    Ok(out)
}
fn length(value: &str, dpi: f32, em: f64) -> Option<f64> {
    let l = svgtypes::Length::from_str(value.trim()).ok()?;
    use svgtypes::LengthUnit as U;
    let n = l.number
        * match l.unit {
            U::None | U::Px => 1.0,
            U::Pt => f64::from(dpi) / 72.0,
            U::Pc => f64::from(dpi) / 6.0,
            U::In => f64::from(dpi),
            U::Cm => f64::from(dpi) / 2.54,
            U::Mm => f64::from(dpi) / 25.4,
            U::Em => em,
            U::Ex => em / 2.0,
            U::Percent => em / 100.0,
        };
    (n.is_finite() && n.abs() <= MAX_COORD).then_some(n)
}
fn value(css: &Styles, n: roxmltree::Node<'_, '_>, key: &str, default: &str) -> String {
    css.prop(n, key).unwrap_or_else(|| default.into())
}
fn style(css: &Styles, n: roxmltree::Node<'_, '_>, dpi: f32) -> CharStyle {
    let mut em = 12.0;
    let mut ancestors: Vec<_> = n.ancestors().filter(|n| n.is_element()).collect();
    ancestors.reverse();
    for n in ancestors {
        if let Some(v) = css.own(n, "font-size") {
            em = length(&v, dpi, em).unwrap_or(em);
        }
    }
    let paint = value(css, n, "fill", "black");
    let paint = if paint == "currentColor" { value(css, n, "color", "black") } else { paint };
    let c = svgtypes::Color::from_str(&paint).unwrap_or(svgtypes::Color { red: 0, green: 0, blue: 0, alpha: 255 });
    let mut color = Color::rgb(f32::from(c.red) / 255.0, f32::from(c.green) / 255.0, f32::from(c.blue) / 255.0);
    color.alpha = f32::from(c.alpha) / 255.0 * value(css, n, "fill-opacity", "1").parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(1.0).clamp(0.0, 1.0);
    if paint == "none" {
        color.alpha = 0.0;
    }
    CharStyle {
        font_family: value(css, n, "font-family", "Inter").split(',').next().unwrap_or("Inter").trim().trim_matches(['\'', '"']).to_string(),
        size_pt: (em * 72.0 / f64::from(dpi)).max(0.01) as f32,
        weight: match value(css, n, "font-weight", "400").as_str() {
            "bold" => 700,
            s => s.parse::<u16>().unwrap_or(400).clamp(100, 900),
        },
        italic: matches!(value(css, n, "font-style", "normal").as_str(), "italic" | "oblique"),
        tracking: length(&value(css, n, "letter-spacing", "0"), dpi, em).unwrap_or(0.0) as f32 / em.max(0.01) as f32 * 1000.0,
        color,
        ..Default::default()
    }
}
fn node_opacity(css: &Styles, node: roxmltree::Node<'_, '_>) -> f32 {
    let Some(value) = css.own(node, "opacity") else { return 1.0 };
    let value = if value == "inherit" { css.prop(node, "opacity").unwrap_or_else(|| "1".into()) } else { value };
    let (value, factor) = value.strip_suffix('%').map_or((value.as_str(), 1.0), |v| (v, 0.01));
    value.parse::<f32>().ok().filter(|v| v.is_finite()).map_or(1.0, |v| (v * factor).clamp(0.0, 1.0))
}
/// Text-content elements have layout semantics; metadata and non-rendered subtrees do not.
fn content_style(css: &Styles, root: roxmltree::Node<'_, '_>, parent: roxmltree::Node<'_, '_>, dpi: f32) -> Option<(CharStyle, Vec<opacity::GroupRef>)> {
    let mut alpha = 1.0;
    let mut groups = Vec::new();
    for node in parent.ancestors().take_while(|node| *node != root) {
        if !node.is_element()
            || node.tag_name().namespace() != Some("http://www.w3.org/2000/svg")
            || !matches!(node.tag_name().name(), "tspan" | "textPath" | "a" | "tref")
            || css.own(node, "display").as_deref() == Some("none")
        {
            return None;
        }
        let opacity = node_opacity(css, node);
        if opacity == 0.0 {
            alpha = 0.0;
        }
        if opacity != 1.0 {
            groups.push(opacity::GroupRef { id: node.id(), opacity });
        }
    }
    groups.reverse();
    let mut style = style(css, parent, dpi);
    if matches!(value(css, parent, "visibility", "visible").as_str(), "hidden" | "collapse") {
        alpha = 0.0;
    }
    style.color.alpha *= alpha;
    Some((style, groups))
}

pub(super) fn prepare(
    source: &str,
    xml: &roxmltree::Document<'_>,
    options: &ImportOptions,
    mut fonts: Option<&mut photocraft_text::TextEngine>,
    warnings: &mut Vec<VectorWarning>,
    cancel: &AtomicBool,
) -> Result<(String, Slots)> {
    let css = Styles::new(xml);
    let used: HashSet<usize> = xml
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .filter_map(|id| id.strip_prefix("local-image-svg-text-")?.split('-').next()?.parse().ok())
        .collect();
    let namespace = (0usize..).find(|i| !used.contains(i)).ok_or(ImportError::Limit("text marker namespace"))?;
    let prefix = format!("local-image-svg-text-{namespace}-");
    let mut slots = Slots { prefix, texts: vec![], names: vec![], markers: vec![], outlined: vec![] };
    let mut bundled = None;
    let mut out = String::new();
    let mut copied = 0;
    let mut characters = 0usize;
    let mut outline_segments = 0usize;
    for n in xml.descendants().filter(|n| n.is_element() && n.tag_name().name() == "text" && n.tag_name().namespace() == Some("http://www.w3.org/2000/svg")) {
        super::cancelled(cancel)?;
        let range = n.range();
        if range.start < copied {
            continue;
        }
        // Bound glyph-layout allocation before pc-text shapes any run. Count
        // source characters conservatively, including collapsed whitespace.
        for t in n.descendants().filter(|n| n.is_text()) {
            let remaining = options.limits.objects.saturating_sub(characters);
            characters = characters.saturating_add(t.text().unwrap_or("").chars().take(remaining.saturating_add(1)).count());
            if characters > options.limits.objects {
                return Err(ImportError::Limit("text character"));
            }
        }
        let name = n.attribute("id").unwrap_or("Text");
        let st = style(&css, n, options.dpi);
        let mut text = TextLayer::default();
        let mut paint_sources = Vec::new();
        let preserve = n.ancestors().any(|n| n.attribute(("http://www.w3.org/XML/1998/namespace", "space")) == Some("preserve"));
        let mut was_space = true;
        for t in n.descendants().filter(|n| n.is_text()) {
            let parent = t.parent().unwrap_or(n);
            let Some((s, groups)) = content_style(&css, n, parent, options.dpi) else { continue };
            let mut chars = String::new();
            for c in t.text().unwrap_or("").chars() {
                if !preserve && matches!(c, '\r' | '\n') {
                    continue;
                }
                let c = if !preserve && c == '\t' { ' ' } else { c };
                if !preserve && c == ' ' && was_space {
                    continue;
                }
                was_space = c == ' ';
                chars.push(c);
            }
            if !chars.is_empty() {
                paint_sources.push(groups);
                text.runs.push(TextRun { len: chars.len(), style: s });
                text.text.push_str(&chars);
            }
        }
        if !preserve && text.text.ends_with(' ') {
            text.text.pop();
            if let Some(last) = text.runs.last_mut() {
                last.len = last.len.saturating_sub(1);
            }
        }
        text.sync_summary();
        let mut first = |key| -> Result<f64> {
            let Some(v) = n.attribute(key) else {
                return Ok(0.0);
            };
            let items: Vec<_> = v.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).collect();
            if items.len() > 1 {
                warn(warnings, "text-position-lists", name, "Per-character SVG positioning is approximated by editable text runs.");
            }
            if v.contains('%') {
                warn(warnings, "text-position-percent", name, "Percentage SVG text positions were approximated; adjust the editable text position.");
            }
            items
                .first()
                .and_then(|v| length(v, options.dpi, f64::from(st.size_pt) * f64::from(options.dpi) / 72.0))
                .ok_or_else(|| ImportError::Invalid("invalid text position".into()))
        };
        let x = first("x")? + first("dx")?;
        let y = first("y")? + first("dy")?;
        text.transform = Affine::translate(x, y);
        let align = match value(&css, n, "text-anchor", "start").as_str() {
            "middle" => TextAlign::Center,
            "end" => TextAlign::Right,
            _ => TextAlign::Left,
        };
        text.paragraphs.push(ParagraphRun { len: text.text.len(), style: ParagraphStyle { align, ..Default::default() } });
        if fonts.is_none() && bundled.is_none() {
            bundled = Some(photocraft_text::TextEngine::new());
        }
        let engine = fonts.as_deref_mut().or(bundled.as_mut()).ok_or_else(|| ImportError::Invalid("missing text engine".into()))?;
        let layout = engine.layout(&text, options.dpi);
        let outlined = if paint_sources.iter().flatten().any(|group| group.opacity > 0.0 && group.opacity < 1.0) {
            for style in &layout.styles {
                if !engine.fonts.has_family(&style.font_family) {
                    warn(warnings, "font-substitution", name, "An unavailable font was substituted before text was converted to outlines.");
                }
            }
            let nodes = opacity::build(&layout, &text.transform, &paint_sources, options, cancel, &mut outline_segments)?;
            warn(warnings, "text-opacity-outlines", name, "Nested text opacity was preserved as grouped outlines; this text is no longer editable.");
            Some(Arc::new(nodes))
        } else {
            None
        };
        let rect = photocraft_text::render::ink_rect(&layout, &text.transform);
        let marker = [rect.x0 as f32, rect.y0 as f32, rect.width().max(1) as f32, rect.height().max(1) as f32];
        if marker.iter().any(|v| !v.is_finite() || f64::from(*v).abs() > MAX_COORD) || rect.size().area() > options.limits.pixels {
            return Err(ImportError::Limit("text placeholder bounds"));
        }
        if n.descendants().any(|n| n.tag_name().name() == "textPath") {
            warn(warnings, "text-path", name, "Text on a path was imported as ordinary editable text.");
        }
        if n.has_attribute("rotate")
            || n.descendants().filter(|n| n.is_element()).skip(1).any(|n| ["x", "y", "dx", "dy", "rotate"].iter().any(|k| n.has_attribute(*k)))
        {
            warn(warnings, "text-position-lists", name, "Per-character SVG positioning is approximated by editable text runs.");
        }
        if value(&css, n, "fill", "black").starts_with("url(") || value(&css, n, "stroke", "none") != "none" {
            warn(warnings, "text-paint", name, "Text gradients and strokes were simplified to solid editable text.");
        }
        let raw = &source[range.clone()];
        let qname = raw.trim_start_matches('<').split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("text");
        let q = qname.strip_suffix("text").unwrap_or("");
        out.push_str(&source[copied..range.start]);
        let _ = write!(out, "<{q}g");
        for key in ["id", "transform", "systemLanguage", "requiredFeatures", "requiredExtensions"] {
            if let Some(v) = n.attribute(key) {
                let _ = write!(out, " {key}=\"{}\"", escape(v));
            }
        }
        let mut group_style = String::new();
        for (key, default) in [
            ("opacity", "1"),
            ("clip-path", "none"),
            ("mask", "none"),
            ("filter", "none"),
            ("mix-blend-mode", "normal"),
            ("isolation", "auto"),
            ("display", "inline"),
        ] {
            let _ = write!(group_style, "{key}:{};", css.own(n, key).as_deref().unwrap_or(default));
        }
        // Visibility was resolved per text run, including visible descendants of a hidden root.
        group_style.push_str("visibility:visible;");
        let index = slots.texts.len();
        let _ = write!(
            out,
            " style=\"{}\"><{q}path id=\"{}{index}\" d=\"M{} {}h{}v{}h-{}Z M{} {}l0.0625 0.03125\" style=\"fill:#{index:06x}!important;stroke:none!important;opacity:1!important;display:inline!important;visibility:visible!important;marker:none!important;clip-path:none!important;mask:none!important;filter:none!important\"/></{q}g>",
            escape(&group_style),
            slots.prefix,
            marker[0],
            marker[1],
            marker[2],
            marker[3],
            marker[2],
            marker[0] + 0.125,
            marker[1] + 0.25
        );
        copied = range.end;
        slots.outlined.push(outlined);
        slots.texts.push(text);
        slots.names.push(name.to_string());
        slots.markers.push(marker);
    }
    out.push_str(&source[copied..]);
    let prepared_limit = usize::try_from(options.limits.decompressed_bytes).unwrap_or(usize::MAX);
    if out.len() > prepared_limit {
        return Err(ImportError::Limit("prepared XML byte"));
    }
    Ok((protect_markers(out, &slots, prepared_limit)?, slots))
}
