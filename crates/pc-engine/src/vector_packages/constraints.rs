//! Original Affinity-style constraints on ordinary group/artboard children. No symbol
//! validation or shared document types are changed. Coordinates remain document pixels.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::structure::Constraints;
use photocraft_doc::{Document, Layer, LayerContent, LayerId};
use serde_json::{Value, json};
pub mod arithmetic;
use super::layout;
use arithmetic::{Box2, error};

pub fn has_constraints(layer: &Layer) -> bool {
    layer.vector.constraints.is_some() || matches!(&layer.content,LayerContent::Group(g) if g.children.iter().any(has_constraints))
}
/// Resize in document-axis parent space. Unconstrained artboard children stay the same
/// size unless scaling is requested explicitly; constrained children always follow pins.
/// Groups have no persisted local transform; rotated/sheared parent bases are refused by
/// layout::Operation::Resize. Rotated child paths receive the ordinary affine mapping.
pub fn prepare_resize(doc: &Document, id: LayerId, old: Box2, new: Box2, scale_unconstrained: bool) -> Result<Layer> {
    layout::preflight(doc, &[id])?;
    arithmetic::validate_rect(old)?;
    arithmetic::validate_rect(new)?;
    let layer = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    layout::check_pixel_sum(resize_pixels(layer, old, new, scale_unconstrained)?)?;
    resize_layer(doc, layer, old, new, scale_unconstrained)
}
fn child_target(child: &Layer, old: Box2, new: Box2, scale: bool) -> Result<(Box2, Box2)> {
    let b = layout::bounds(child)?;
    let c = child.vector.constraints.unwrap_or_else(|| {
        if scale { Constraints::default() } else { Constraints { left: true, top: true, fixed_width: true, fixed_height: true, ..Default::default() } }
    });
    Ok((b, arithmetic::resize(old, new, b, c)?))
}
fn resize_pixels(layer: &Layer, old: Box2, new: Box2, scale: bool) -> Result<u64> {
    let LayerContent::Group(group) = &layer.content else {
        return Err(error("constraints resize requires a group or artboard"));
    };
    let mut pixels = layout::transform_pixels(layer, arithmetic::mapping(old, new)?, true)?;
    for child in &group.children {
        let (b, target) = child_target(child, old, new, scale)?;
        pixels = pixels.saturating_add(if child.is_group() {
            resize_pixels(child, b, target, true)?
        } else {
            layout::transform_pixels(child, arithmetic::mapping(b, target)?, false)?
        });
        layout::check_pixel_sum(pixels)?;
    }
    Ok(pixels)
}
fn resize_layer(doc: &Document, layer: &Layer, old: Box2, new: Box2, scale_unconstrained: bool) -> Result<Layer> {
    let LayerContent::Group(group) = &layer.content else {
        return Err(error("constraints resize requires a group or artboard"));
    };
    let a = arithmetic::mapping(old, new)?;
    let mut out = layout::transformed_parent(doc, layer, a)?;
    let mut children = Vec::new();
    let mut pixels = layout::prepared_pixels(&out)?;
    for child in &group.children {
        let (b, target) = child_target(child, old, new, scale_unconstrained)?;
        let prepared =
            if child.is_group() { resize_layer(doc, child, b, target, true)? } else { layout::transformed(doc, child, arithmetic::mapping(b, target)?)? };
        pixels = pixels.saturating_add(layout::prepared_pixels(&prepared)?);
        layout::check_pixel_sum(pixels)?;
        children.push(prepared);
    }
    if let LayerContent::Group(g) = &mut out.content {
        g.children = children;
    }
    if out.artboard().is_none() {
        let represented = layout::bounds(&out)?;
        if represented.iter().zip(new).any(|(a, b)| (a - b).abs() > 1e-6) {
            return Err(error("this group resize needs an explicit container frame; use an artboard"));
        }
    }
    Ok(out)
}
fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let capture: layout::Capture =
        serde_json::from_value(p.get("capture").cloned().ok_or_else(|| error("constraints require capture"))?).map_err(|e| error(e.to_string()))?;
    capture.check(s)?;
    let raw = p.get("constraints").ok_or_else(|| error("constraints object or null required"))?;
    if !raw.is_null() && !raw.is_object() {
        return Err(error("constraints object or null required"));
    }
    if let Some(map) = raw.as_object() {
        for (key, value) in map {
            if !matches!(key.as_str(), "left" | "right" | "top" | "bottom" | "fixed_width" | "fixed_height") || !value.is_boolean() {
                return Err(error("unknown or non-boolean constraint"));
            }
        }
    }
    let constraints: Option<Constraints> = serde_json::from_value(raw.clone()).map_err(|e| error(e.to_string()))?;
    s.edit("Set Constraints", |doc, _| {
        for id in &capture.layers {
            doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))?.vector.constraints = constraints;
        }
        Ok(())
    })?;
    Ok(json!({"layers":capture.layers,"constraints":constraints}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "vector.constraints.set",
        label: "Set Constraints",
        menu: &[],
        shortcut: None,
        params: "{capture:{document,revision,layers},constraints:{left?,right?,top?,bottom?,fixed_width?,fixed_height?}|null}",
        enabled: crate::vector_cmds::has_doc,
        journal: true,
        run: set,
    }]
}
#[cfg(test)]
mod tests;
