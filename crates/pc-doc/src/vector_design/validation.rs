//! Native vector metadata validation, independent of renderers and the engine.
use super::{appearance::AppearanceItem, swatches::PaintSlot};
use crate::{Document, Fill, LayerContent};
use std::collections::HashSet;

pub fn document(doc: &Document) -> Result<(), String> {
    if let Some(scale) = doc.drawing_scale {
        scale.validate()?;
    }
    doc.swatches.validate()?;
    if doc.graphic_styles.len() > 1024 {
        return Err("graphic style budget exceeded".into());
    }
    let mut styles = HashSet::new();
    let mut names = HashSet::new();
    for style in &doc.graphic_styles {
        if style.id.0 == 0
            || !styles.insert(style.id)
            || style.name.trim().is_empty()
            || style.name.len() > 4096
            || style.name.chars().any(char::is_control)
            || style.name.trim() != style.name
            || !names.insert(style.name.as_str())
        {
            return Err("invalid or duplicate graphic style".into());
        }
        sample(&style.attributes)?;
        if !style.attributes.opacity.is_finite() || !(0.0..=1.0).contains(&style.attributes.opacity) {
            return Err("invalid graphic style opacity".into());
        }
    }
    let mut layers = doc.walk().into_iter().map(|(_, _, layer)| layer).collect::<Vec<_>>();
    fn append<'a>(roots: &'a [crate::Layer], out: &mut Vec<&'a crate::Layer>) {
        for layer in roots {
            out.push(layer);
            if let Some(children) = layer.children() {
                append(children, out);
            }
        }
    }
    for master in &doc.symbols {
        append(&master.layers, &mut layers);
    }
    for layer in layers {
        if let Some(scale) = layer.artboard().and_then(|a| a.drawing_scale) {
            scale.validate()?;
        }
        if layer.vector.graphic_style.as_ref().is_some_and(|link| !styles.contains(&link.style)) {
            return Err("dangling graphic style link".into());
        }
        if let Some(link) = &layer.vector.graphic_style {
            sample(&link.applied)?;
        }
        if let Some(appearance) = &layer.vector.appearance {
            validate_appearance(appearance)?;
        }
        let value = serde_json::to_value(&layer.vector).map_err(|e| e.to_string())?;
        if serde_json::from_value::<super::VectorData>(value.clone()).map_err(|e| e.to_string())? != layer.vector {
            return Err("non-finite vector metadata".into());
        }
        numbers(&value, "", 0)?;
        if let Some(path) = &layer.vector.on_path
            && (!(0.0..=1.0).contains(&path.start) || !(0.0..=1.0).contains(&path.end))
        {
            return Err("invalid path-text brackets".into());
        }
        if let Some(frame) = layer.vector.frame_compose
            && (!frame.grid_spacing.is_finite()
                || !(0.0..=4096.0).contains(&frame.grid_spacing)
                || !frame.grid_origin.is_finite()
                || frame.grid_origin.abs() > 1_048_576.0)
        {
            return Err("invalid frame composition grid".into());
        }
    }
    for link in &doc.swatches.links {
        let layer = if let Some(symbol) = link.target.symbol {
            let master = doc.symbols.iter().find(|s| s.id == symbol).ok_or("missing swatch symbol")?;
            fn find(layers: &[crate::Layer], id: crate::LayerId) -> Option<&crate::Layer> {
                layers.iter().find_map(|l| {
                    if l.id == id {
                        Some(l)
                    } else if let LayerContent::Group(g) = &l.content {
                        find(&g.children, id)
                    } else {
                        None
                    }
                })
            }
            find(&master.layers, link.target.layer)
        } else {
            doc.layer(link.target.layer)
        }
        .ok_or("missing swatch layer")?;
        let paint = match link.target.paint {
            PaintSlot::Fill => match &layer.content {
                LayerContent::Shape(s) => s.fill.as_ref(),
                LayerContent::Fill(f) => Some(f),
                _ => None,
            },
            PaintSlot::Stroke => match &layer.content {
                LayerContent::Shape(s) => s.stroke.as_ref().map(|s| &s.paint),
                _ => None,
            },
            PaintSlot::Appearance(index) => layer.vector.appearance.as_ref().and_then(|a| a.items.get(index)).and_then(|i| match i {
                AppearanceItem::Fill(f) => Some(&f.paint),
                AppearanceItem::Stroke(s) => Some(&s.base.paint),
                AppearanceItem::Mesh(_) => None,
            }),
        }
        .ok_or("invalid swatch paint target")?;
        match (link.target.stop, paint) {
            (None, Fill::Solid(_)) => {}
            (Some(index), Fill::Gradient { stops, .. }) if index < stops.len() => {}
            _ => return Err("invalid swatch paint stop".into()),
        }
    }
    Ok(())
}

fn numbers(value: &serde_json::Value, key: &str, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("vector metadata nesting budget exceeded".into());
    }
    match value {
        serde_json::Value::Number(n)
            if !matches!(
                key,
                "id" | "symbol" | "style" | "seed" | "geometry" | "swatch" | "style_sheet" | "excluded_channels" | "members" | "layer" | "source" | "target"
            ) =>
        {
            let number = n.as_f64().ok_or("invalid vector number")?;
            if !number.is_finite() || number.abs() > 1_048_576. {
                return Err("vector metadata numbers exceed bounds".into());
            }
            if matches!(key, "opacity" | "fill_opacity" | "alpha") && !(0.0..=1.0).contains(&number) {
                return Err("vector opacity outside 0..1".into());
            }
        }
        serde_json::Value::String(s) if s.len() > 4096 => return Err("vector metadata string budget exceeded".into()),
        serde_json::Value::Array(a) => {
            if a.len() > 8_000_000 {
                return Err("vector metadata array budget exceeded".into());
            }
            for value in a {
                numbers(value, key, depth + 1)?;
            }
        }
        serde_json::Value::Object(o) => {
            for (key, value) in o {
                if key != "psd_raw" {
                    numbers(value, key, depth + 1)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
fn sample(sample: &super::styles::Sample) -> Result<(), String> {
    if let Some(a) = &sample.appearance {
        validate_appearance(a)?;
    }
    if sample.effects.items.len() > 64 || sample.effects.psd_raw.as_ref().is_some_and(|raw| raw.len() > 1 << 20) {
        return Err("graphic style effects budget exceeded".into());
    }
    let value = serde_json::to_value(sample).map_err(|e| e.to_string())?;
    if serde_json::from_value::<super::styles::Sample>(value.clone()).map_err(|e| e.to_string())? != *sample {
        return Err("non-finite graphic style".into());
    }
    numbers(&value, "", 0)
}
fn validate_appearance(a: &super::appearance::Appearance) -> Result<(), String> {
    if a.items.len() > 64 {
        return Err("appearance item budget exceeded".into());
    }
    for item in &a.items {
        match item {
            AppearanceItem::Mesh(mesh) => {
                let m = &mesh.paint;
                if !(1..=50).contains(&m.rows) || !(1..=50).contains(&m.cols) || m.points.len() != (m.rows + 1) * (m.cols + 1) {
                    return Err("invalid gradient mesh dimensions".into());
                }
                for p in &m.points {
                    if p.rgba.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                        || p.handles.iter().any(|h| (0..2).any(|axis| !h[axis].is_finite() || (p.position[axis] + h[axis]).abs() > 1_000_000.))
                    {
                        return Err("invalid gradient mesh colour or handle".into());
                    }
                }
            }
            AppearanceItem::Stroke(s) if s.base.width < 0. || s.base.width > 5000. || s.base.dashes.len() > 65_536 => {
                return Err("invalid appearance stroke".into());
            }
            _ => {}
        }
    }
    Ok(())
}
