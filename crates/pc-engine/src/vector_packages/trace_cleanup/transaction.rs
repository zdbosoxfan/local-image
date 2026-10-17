//! A preview owns detached prepared layers. Cache work completes before the single edit.
use super::{Output, Stats};
use crate::{EngineError, Result, Session};
use photocraft_doc::{Color, DocId, Fill, Layer, LayerContent, LayerId, ShapeLayer};
use photocraft_geom::Affine;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct Capture {
    document: DocId,
    revision: u64,
    targets: Vec<LayerId>,
}
impl Capture {
    pub fn check(&self, session: &Session) -> Result<()> {
        let state = session.active().ok_or(EngineError::NoDocument)?;
        if state.doc.id != self.document || state.revision != self.revision {
            return Err(EngineError::Other("trace cleanup request is stale".into()));
        }
        let mut seen = HashSet::new();
        if self.targets.is_empty() || self.targets.len() > 64 {
            return Err(EngineError::Other("trace source is required".into()));
        }
        for &id in &self.targets {
            if !seen.insert(id) {
                return Err(EngineError::Other("duplicate trace source".into()));
            }
            let layer = state.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
            let locks = state.doc.effective_locks(id);
            if locks.all || locks.pixels {
                return Err(EngineError::Other("trace source is locked".into()));
            }
            if !matches!(layer.content, LayerContent::Raster(_) | LayerContent::Smart(_)) {
                return Err(EngineError::Other("trace requires a pixel layer or smart object".into()));
            }
        }
        Ok(())
    }
}
pub fn capture(session: &Session, targets: &[LayerId]) -> Result<Capture> {
    let state = session.active().ok_or(EngineError::NoDocument)?;
    let capture = Capture { document: state.doc.id, revision: state.revision, targets: targets.to_vec() };
    capture.check(session)?;
    Ok(capture)
}
#[derive(Clone, Debug)]
pub struct Prepared {
    capture: Capture,
    children: Vec<Layer>,
    pub stats: Stats,
}
/// Geometry in Output is already at source scale. The caller supplies only the ROI origin.
/// All paths are preflighted before any render cache is allocated.
pub fn prepare(session: &Session, capture: &Capture, output: &Output, origin: [i32; 2], cancel: impl Fn() -> bool) -> Result<Prepared> {
    capture.check(session)?;
    let state = session.active().ok_or(EngineError::NoDocument)?;
    let trace = &output.trace;
    if trace.width == 0
        || trace.height == 0
        || u64::from(trace.width) * u64::from(trace.height) > pc_trace::PIXEL_BUDGET as u64
        || origin.iter().any(|v| v.unsigned_abs() > 1_000_000)
        || trace.layers.len() > 64
    {
        return Err(EngineError::Other("trace output dimensions or origin exceed bounds".into()));
    }
    let mut nodes = 0usize;
    for layer in &trace.layers {
        crate::vector_api::validate_path(&layer.path)?;
        if layer.path.inverted || layer.path.subpaths.iter().any(|s| !s.closed || s.knots.len() < 2) {
            return Err(EngineError::Other("cleanup fill paths must be closed and noninverted".into()));
        }
        for s in &layer.path.subpaths {
            nodes = nodes.saturating_add(s.knots.len());
        }
        if layer
            .path
            .subpaths
            .iter()
            .flat_map(|s| &s.knots)
            .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
            .any(|p| p.x.abs() > 1_000_000. || p.y.abs() > 1_000_000.)
        {
            return Err(EngineError::Other("trace output coordinates exceed bounds".into()));
        }
    }
    if nodes > 1_000_000 || nodes != trace.nodes {
        return Err(EngineError::Other("trace output node count is invalid".into()));
    }
    let transform = Affine::translate(f64::from(origin[0]), f64::from(origin[1]));
    let mut children = Vec::with_capacity(trace.layers.len());
    for layer in &trace.layers {
        if cancel() {
            return Err(EngineError::Other("trace cancelled".into()));
        }
        let [r, g, b, a] = layer.color;
        let mut shape = ShapeLayer {
            path: layer.path.transform(&transform),
            fill: Some(Fill::Solid(Color::rgba(f32::from(r) / 255., f32::from(g) / 255., f32::from(b) / 255., f32::from(a) / 255.))),
            ..Default::default()
        };
        // pc-vector's CompiledShape bounds-clips and sparsely allocates affected tiles.
        shape.cache = Some(photocraft_vector::render_shape(&shape, state.doc.pixel_format(), state.doc.bounds()));
        children.push(Layer::new(format!("#{r:02X}{g:02X}{b:02X}"), LayerContent::Shape(shape)));
    }
    if cancel() {
        return Err(EngineError::Other("trace cancelled".into()));
    }
    Ok(Prepared { capture: capture.clone(), children, stats: output.stats.clone() })
}
/// Called inside the journaled apply command. Preview drops Prepared.
pub fn commit(session: &mut Session, prepared: Prepared, group_name: &str, cancel: impl Fn() -> bool) -> Result<LayerId> {
    prepared.capture.check(session)?;
    if cancel() {
        return Err(EngineError::Other("trace cancelled".into()));
    }
    let above = prepared.capture.targets.first().copied();
    let group = Layer::group(group_name, prepared.children);
    session.edit("Vectorize", move |doc, active| {
        let id = doc.insert_above(above, group);
        *active = Some(id);
        Ok(id)
    })
}

#[cfg(test)]
mod tests;
