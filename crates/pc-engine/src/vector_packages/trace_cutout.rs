//! Package 27 cutout evaluation and atomic publication adapter.
//! Package 28 owns the existing public vectorize commands and dialog routing.
//! Preview stays detached; prepared caches are published by one Session::edit.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use pc_trace::{Image, Params, Trace};
use photocraft_doc::{Color, DocId, Document, Fill, Layer, LayerContent, LayerId, ShapeLayer};
use photocraft_geom::{Affine, Point};
use std::sync::Arc;

/// Capture this at gesture begin, before reading pixels or starting a job.
#[derive(Clone)]
pub struct Capture {
    document: Arc<Document>,
    revision: u64,
    target: LayerId,
}
fn validate_target(doc: &Document, target: LayerId) -> Result<()> {
    let layer = doc.layer(target).ok_or(EngineError::NoLayer(target))?;
    let locks = doc.effective_locks(target);
    if locks.all || locks.pixels {
        return Err(EngineError::Other("trace target is locked".into()));
    }
    if !matches!(layer.content, LayerContent::Raster(_) | LayerContent::Smart(_)) {
        return Err(EngineError::Other("trace requires a pixel layer or smart object".into()));
    }
    Ok(())
}
impl Capture {
    pub fn begin(session: &Session, target: LayerId) -> Result<Self> {
        let state = session.active().ok_or(EngineError::NoDocument)?;
        validate_target(&state.doc, target)?;
        Ok(Self { document: state.doc.clone(), revision: state.revision, target })
    }
    pub fn document(&self) -> DocId {
        self.document.id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn target(&self) -> LayerId {
        self.target
    }
    /// Read only source snapshot for package 28's pixel/selection extraction.
    pub fn snapshot(&self) -> &Document {
        &self.document
    }
    fn current(&self, session: &Session) -> Result<()> {
        let state = session.active().ok_or(EngineError::NoDocument)?;
        if state.doc.id != self.document.id || state.revision != self.revision {
            return Err(EngineError::Other("trace request is stale".into()));
        }
        validate_target(&state.doc, self.target)
    }
    /// Build bounds-limited caches on a detached captured document. The caller
    /// supplies the selected source bounds' origin; trace coordinates are local.
    pub fn prepare(self, trace: Trace, origin: Point, cancel: impl Fn() -> bool) -> Result<Prepared> {
        if !origin.x.is_finite() || !origin.y.is_finite() {
            return Err(EngineError::Other("trace origin must be finite".into()));
        }
        if trace.width == 0 || trace.height == 0 || u64::from(trace.width) * u64::from(trace.height) > pc_trace::PIXEL_BUDGET as u64 {
            return Err(EngineError::Other("trace dimensions exceed budget".into()));
        }
        let bounds = self.document.bounds();
        if origin.x < f64::from(bounds.x0)
            || origin.y < f64::from(bounds.y0)
            || origin.x + f64::from(trace.width) > f64::from(bounds.x1)
            || origin.y + f64::from(trace.height) > f64::from(bounds.y1)
        {
            return Err(EngineError::Other("trace source bounds lie outside the document".into()));
        }
        let node_count: usize = trace.layers.iter().flat_map(|l| &l.path.subpaths).map(|s| s.knots.len()).sum();
        if trace.layers.len() > 64 || node_count > pc_trace::cutout::EDGE_BUDGET.saturating_mul(2) {
            return Err(EngineError::Other("trace geometry exceeds budget".into()));
        }
        let mut children = Vec::new();
        for color in trace.layers {
            if cancel() {
                return Err(EngineError::Other("trace cancelled".into()));
            }
            crate::vector_api::validate_path(&color.path)?;
            if color.path.subpaths.iter().any(|s| !s.closed || s.knots.len() < 2) {
                return Err(EngineError::Other("cutout fill contours must be closed".into()));
            }
            if let Some((x0, y0, x1, y1)) = color.path.control_bounds()
                && (x0 < -64. || y0 < -64. || x1 > f64::from(trace.width) + 64. || y1 > f64::from(trace.height) + 64.)
            {
                return Err(EngineError::Other("trace coordinates exceed budget".into()));
            }
            let [r, g, b, a] = color.color;
            let mut shape = ShapeLayer {
                path: color.path.transform(&Affine::translate(origin.x, origin.y)),
                fill: Some(Fill::Solid(Color::rgba(f32::from(r) / 255., f32::from(g) / 255., f32::from(b) / 255., f32::from(a) / 255.))),
                ..Default::default()
            };
            crate::vector_api::validate_path(&shape.path)?;
            shape.cache = Some(photocraft_vector::render_shape(&shape, self.document.pixel_format(), bounds));
            children.push(Layer::new(format!("#{r:02X}{g:02X}{b:02X}"), LayerContent::Shape(shape)));
        }
        if cancel() {
            return Err(EngineError::Other("trace cancelled".into()));
        }
        let source = self.document.layer(self.target).ok_or(EngineError::NoLayer(self.target))?;
        let mut group = Layer::group(format!("Vectorized – {}", source.name), children);
        photocraft_doc::vector_design::mosaic::mark(&mut group).map_err(|e| EngineError::Other(e.into()))?;
        Ok(Prepared { capture: self, group })
    }
}
/// Prepared output is deliberately opaque: callers cannot change geometry or
/// drop its caches between validation and publication.
pub struct Prepared {
    capture: Capture,
    group: Layer,
}
impl Prepared {
    /// Package 28 calls this from its journaled vectorize command completion.
    pub fn commit(self, session: &mut Session) -> Result<LayerId> {
        self.capture.current(session)?;
        let target = self.capture.target;
        session.edit("Vectorize", move |doc, active| {
            validate_target(doc, target)?;
            let id = doc.insert_above(Some(target), self.group);
            *active = Some(id);
            Ok(id)
        })
    }
}
/// Background preview/job evaluator. No session access, journal or caches.
pub fn evaluate(image: Image<'_>, params: &Params, cancel: impl Fn() -> bool) -> Result<Trace> {
    pc_trace::cutout::trace_with_cancel(image, params, cancel).map_err(|e| EngineError::Other(e.to_string()))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
#[cfg(test)]
#[path = "trace_cutout/tests.rs"]
mod tests;
