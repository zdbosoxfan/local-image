//! Editable abutting trace partition. Geometry edits invalidate its coverage policy.
use crate::{Layer, LayerContent};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mosaic {
    pub geometry: u64,
}
/// Stable geometry fingerprint independent of native LayerId remapping and paint edits.
pub fn fingerprint(layer: &Layer) -> Option<u64> {
    let LayerContent::Group(group) = &layer.content else { return None };
    if group.artboard.is_some() || group.children.is_empty() || group.children.len() > 100_000 {
        return None;
    }
    let mut hash = 0xcbf29ce484222325u64;
    let mut put = |v: u64| {
        for b in v.to_le_bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    };
    put(group.children.len() as u64);
    for child in &group.children {
        let LayerContent::Shape(shape) = &child.content else { return None };
        if shape.stroke.is_some()
            || !matches!(shape.fill, Some(crate::Fill::Solid(_)))
            || child.clipped
            || child.blend != crate::BlendMode::Normal
            || child.opacity != 1.0
            || child.fill_opacity != 1.0
            || child.blend_if != crate::BlendIf::default()
            || child.excluded_channels != 0
            || child.mask.is_some()
            || child.vector_mask.is_some()
            || !child.effects.items.is_empty()
            || !child.vector.is_empty()
        {
            return None;
        }
        put(shape.path.fill_rule as u64);
        put(u64::from(shape.path.inverted));
        put(shape.path.subpaths.len() as u64);
        for sub in &shape.path.subpaths {
            if !sub.closed {
                return None;
            }
            put(sub.knots.len() as u64);
            put(sub.op as u64);
            for knot in &sub.knots {
                for point in [knot.anchor, knot.in_ctrl, knot.out_ctrl] {
                    if !point.x.is_finite() || !point.y.is_finite() {
                        return None;
                    }
                    put(point.x.to_bits());
                    put(point.y.to_bits());
                }
            }
        }
    }
    Some(hash)
}
pub fn active(layer: &Layer) -> bool {
    layer.vector.mosaic.is_some_and(|m| fingerprint(layer) == Some(m.geometry))
}
/// Trace owns the proof that these independent paths form a nonoverlapping partition.
pub fn mark(layer: &mut Layer) -> Result<(), &'static str> {
    let geometry = fingerprint(layer).ok_or("mosaic requires plain abutting filled shapes")?;
    layer.vector.mosaic = Some(Mosaic { geometry });
    layer.blend = crate::BlendMode::Normal;
    Ok(())
}
