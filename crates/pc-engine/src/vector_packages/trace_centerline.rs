//! Detached centerline stroke-output bridge for the trace job/dialog.
//! The existing layer.vectorize command family is reused; no duplicate commands.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use pc_trace::centerline::CenterlineTrace;
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, StrokeItem};
use photocraft_doc::{Document, Layer, LayerContent, LayerId, ShapeLayer};
use photocraft_geom::Affine;
use photocraft_vector::appearance::{CompiledAppearance, StrokeProjection};
use serde::{Deserialize, Serialize};

fn error(value: impl ToString) -> EngineError {
    EngineError::Other(value.to_string())
}

/// Capture before starting a worker. Image-space positions and widths both use
/// 1/scale when previewing a reduced trace. Origin is the selection's document offset.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub source: LayerId,
    pub active: Option<LayerId>,
    pub targets: Vec<LayerId>,
    pub origin: [f64; 2],
    pub scale: f64,
}
pub fn capture(session: &Session, source: LayerId, origin: [f64; 2], scale: f64) -> Result<Capture> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let c = Capture { document: s.doc.id.0, revision: s.revision, source, active: s.active_layer, targets: s.selected_layers.clone(), origin, scale };
    check(session, &c)?;
    Ok(c)
}
/// Recheck the complete captured target set, ancestor locks and insertion depth.
pub fn check(session: &Session, c: &Capture) -> Result<()> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    if s.doc.id.0 != c.document || s.revision != c.revision || s.active_layer != c.active || s.selected_layers != c.targets {
        return Err(error("centreline trace is stale; start again"));
    }
    if !c.scale.is_finite() || !(0.01..=1.).contains(&c.scale) || c.origin.iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.) {
        return Err(error("centreline origin/scale must be finite and bounded"));
    }
    if c.targets.len() > 8192 {
        return Err(error("centreline target set exceeds budget"));
    }
    let source = s.doc.layer(c.source).ok_or(EngineError::NoLayer(c.source))?;
    if !matches!(source.content, LayerContent::Raster(_) | LayerContent::Smart(_)) {
        return Err(error("centreline requires a pixel layer or smart object"));
    }
    let mut seen = std::collections::HashSet::new();
    for &id in &c.targets {
        if !seen.insert(id) {
            return Err(error("duplicate centreline target"));
        }
    }
    seen.insert(c.source);
    for id in seen {
        s.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
        let locks = s.doc.effective_locks(id);
        if locks.all || locks.pixels {
            return Err(error("centreline target is locked"));
        }
    }
    let path = s.doc.path_of(c.source).ok_or(EngineError::NoLayer(c.source))?;
    // insert_above places alongside the source; the new group adds one level.
    if path.len() + 1 > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(error("centreline output exceeds group depth"));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Prepared {
    capture: Capture,
    group: Layer,
}
impl Prepared {
    pub fn group(&self) -> &Layer {
        &self.group
    }
}

/// Admission precedes all cache work. Width projection is supplied by the caller;
/// NoProjection returns a real error for varying widths, never a uniform fallback.
pub fn prepare(session: &Session, c: &Capture, trace: &CenterlineTrace, projection: &dyn StrokeProjection, cancel: impl Fn() -> bool) -> Result<Prepared> {
    check(session, c)?;
    let poll = || if cancel() { Err(error("centreline preparation cancelled")) } else { Ok(()) };
    poll()?;
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let n = u64::from(trace.width) * u64::from(trace.height);
    if trace.width == 0 || trace.height == 0 || n > pc_trace::PIXEL_BUDGET as u64 || trace.strokes.len() > 8192 {
        return Err(error("centreline output exceeds image/stroke budget"));
    }
    let mut nodes = 0usize;
    let mut profile_points = 0usize;
    let mut descriptors = Vec::with_capacity(trace.strokes.len());
    let transform = Affine::translate(c.origin[0], c.origin[1]).mul(&Affine::scale(1. / c.scale));
    for stroke in &trace.strokes {
        poll()?;
        if stroke.path.inverted || stroke.path.subpaths.len() != 1 || stroke.path.subpaths[0].closed || stroke.path.subpaths[0].knots.len() < 2 {
            return Err(error("centreline output must be an open, uninverted graph run"));
        }
        nodes = nodes.checked_add(stroke.path.subpaths[0].knots.len()).ok_or_else(|| error("centreline node budget"))?;
        if nodes > 1_048_576 {
            return Err(error("centreline output exceeds node budget"));
        }
        crate::vector_api::validate_path(&stroke.path)?;
        if let Some(p) = &stroke.profile {
            profile_points = profile_points.checked_add(p.points.len()).ok_or_else(|| error("centreline width profile budget"))?;
            if p.points.len() < 2 || p.points.len() > 4096 || profile_points > 1_048_576 {
                return Err(error("centreline output exceeds width profile budget"));
            }
            if p.points.first().is_none_or(|p| p.t != 0.) || p.points.last().is_none_or(|p| p.t != 1.) {
                return Err(error("centreline width profile must span 0..1"));
            }
        }
        let mut base = stroke.stroke.clone();
        base.width /= c.scale as f32;
        if !base.width.is_finite() || !(0.01..=5000.).contains(&base.width) {
            return Err(error("centreline stroke width must be finite and positive"));
        }
        let mut profile = stroke.profile.clone();
        if let Some(p) = &mut profile {
            for p in &mut p.points {
                p.left /= c.scale;
                p.right /= c.scale;
            }
        }
        let appearance = Appearance { items: vec![AppearanceItem::Stroke(StrokeItem { base: base.clone(), profile, ..Default::default() })] };
        photocraft_vector::appearance::validate(&appearance).map_err(error)?;
        let shape = ShapeLayer { path: stroke.path.transform(&transform), fill: None, stroke: Some(base), ..Default::default() };
        descriptors.push((shape, appearance));
    }
    if nodes != trace.nodes {
        return Err(error("centreline node count does not match output"));
    }
    let mut compiled = Vec::with_capacity(descriptors.len());
    let mut bytes = 0u128;
    for (shape, appearance) in &descriptors {
        poll()?;
        let paint = CompiledAppearance::with_projection(shape, Some(appearance), &s.doc.patterns, s.doc.bounds(), projection).map_err(error)?;
        bytes += paint.cache_bytes(s.doc.pixel_format());
        if bytes > 512 << 20 {
            return Err(error("centreline caches exceed 512 MiB budget"));
        }
        compiled.push(paint);
    }
    let mut caches = Vec::with_capacity(compiled.len());
    for paint in compiled {
        poll()?;
        caches.push(paint.render(s.doc.pixel_format(), s.doc.bounds()).map_err(error)?);
    }
    let mut children = Vec::with_capacity(descriptors.len());
    for ((mut shape, appearance), cache) in descriptors.into_iter().zip(caches) {
        shape.cache = Some(cache);
        let mut layer = Layer::new("Centreline", LayerContent::Shape(shape));
        layer.vector.appearance = Some(appearance);
        children.push(layer);
    }
    poll()?;
    let source = s.doc.layer(c.source).ok_or(EngineError::NoLayer(c.source))?;
    Ok(Prepared { capture: c.clone(), group: Layer::group(format!("Vectorized – {}", source.name), children) })
}

/// Detached preview uses precisely the same prepared layers that publication inserts.
pub fn preview(session: &Session, prepared: &Prepared) -> Result<Document> {
    check(session, &prepared.capture)?;
    let mut doc = (*session.active().ok_or(EngineError::NoDocument)?.doc).clone();
    doc.insert_above(Some(prepared.capture.source), prepared.group.clone());
    Ok(doc)
}
pub fn publish(session: &mut Session, prepared: Prepared) -> Result<LayerId> {
    check(session, &prepared.capture)?;
    if let LayerContent::Group(g) = &prepared.group.content
        && g.children.is_empty()
    {
        return Err(error("centreline trace has no strokes"));
    }
    session.edit("Vectorize", move |doc, active| {
        let id = doc.insert_above(Some(prepared.capture.source), prepared.group);
        *active = Some(id);
        Ok(id)
    })
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    Vec::new()
}

#[cfg(test)]
mod gpu;
#[cfg(test)]
mod tests;
