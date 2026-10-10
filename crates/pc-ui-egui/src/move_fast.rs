//! Cached GPU planes for Move. Normal compositing is associative: contiguous stationary and
//! moving runs can be composited once, then translated and blended at viewport resolution.
//! Backdrop-dependent blends, clipping and adjustments keep the reference composite preview.

use photocraft_color::{BlendMode, ColorMode};
use photocraft_doc::{DocId, Document, Layer, LayerContent, LayerId};
use photocraft_geom::{Rect, Size};

use crate::PhotocraftApp;
use crate::gpu_canvas::MovePlane;
use crate::state::Tool;

pub(crate) struct FastMove {
    source: std::sync::Arc<Document>,
    ids: Vec<LayerId>,
    planes: Vec<MovePlane>,
}

fn independent(l: &Layer, moving: bool, canvas: Rect) -> bool {
    if !l.visible {
        return true;
    }
    let group = matches!(l.content, LayerContent::Group(_));
    if l.clipped
        || l.excluded_channels != 0
        || !l.blend_if.is_default()
        || (l.effects.enabled && !l.effects.items.is_empty())
        || (l.blend != BlendMode::Normal
            && !(group && l.blend == BlendMode::PassThrough && l.opacity == 1.0 && l.fill_opacity == 1.0 && l.mask.is_none() && l.vector_mask.is_none()))
        || (moving && (l.mask.as_ref().is_some_and(|m| m.enabled && !m.linked) || l.vector_mask.as_ref().is_some_and(|m| m.enabled && !m.linked)))
    {
        return false;
    }
    match &l.content {
        LayerContent::Adjustment(_) => false,
        LayerContent::Fill(_) => !moving,
        LayerContent::Group(g) => g.artboard.is_none() && g.children.iter().all(|c| independent(c, moving, canvas)),
        // A clipped shape cache must be regenerated to bring its off-canvas geometry into view.
        LayerContent::Shape(_) if moving => l.surface().is_some_and(|s| {
            let b = photocraft_compose::bounds::content_bounds(s);
            b.x0 > canvas.x0 && b.y0 > canvas.y0 && b.x1 < canvas.x1 && b.y1 < canvas.y1
        }),
        _ => true,
    }
}

impl FastMove {
    pub(crate) fn keys(&self) -> impl Iterator<Item = u64> + '_ {
        self.planes.iter().map(|p| p.key)
    }
    pub(crate) fn showing(&self) -> bool {
        !self.planes.is_empty()
    }
}

fn build(app: &mut PhotocraftApp, doc: &Document, ids: &[LayerId]) -> Option<Vec<MovePlane>> {
    let gpu = app.gpu.clone()?;
    if doc.mode != ColorMode::Rgb
        || app.session.color.canvas_display(doc).ok()?.encode_srgb
        || ids.iter().any(|id| !doc.layers.iter().any(|l| l.id == *id) || doc.effective_locks(*id).position || doc.effective_locks(*id).all)
        || !doc.layers.iter().all(|l| independent(l, ids.contains(&l.id), doc.bounds()))
    {
        return None;
    }
    let mut runs: Vec<(bool, Vec<Layer>)> = Vec::new();
    for l in doc.layers.iter().filter(|l| l.visible) {
        let moving = ids.contains(&l.id);
        if let Some((_, layers)) = runs.last_mut().filter(|(m, _)| *m == moving) {
            layers.push(l.clone());
        } else {
            runs.push((moving, vec![l.clone()]));
        }
    }
    if runs.len() > 16 {
        return None;
    }
    let mut planes = Vec::new();
    for (moving, layers) in runs {
        let mut plane = doc.clone();
        plane.id = DocId::fresh();
        plane.selection = None;
        plane.layers = layers;
        let mut origin = [0, 0];
        if moving {
            // Include pixels beyond the canvas: moving them into view must not reveal a hole.
            let all = Rect::new(-10_000_000, -10_000_000, 10_000_000, 10_000_000);
            let b = plane.layers.iter().try_fold(Rect::EMPTY, |a, l| Some(a.union(&photocraft_compose::change_bounds(l, all)?)))?;
            origin = [b.x0.min(0), b.y0.min(0)];
            let size = Size::new((b.x1.max(doc.bounds().x1) - origin[0]) as u32, (b.y1.max(doc.bounds().y1) - origin[1]) as u32);
            if size.area() > doc.size.area().saturating_mul(4).max(1_000_000) {
                return None;
            }
            if origin != [0, 0] {
                let roots: Vec<_> = plane.layers.iter().map(|l| l.id).collect();
                plane = photocraft_engine::layer_multi_cmds::moved(&plane, &roots, -origin[0], -origin[1]).ok()?;
            }
            plane.size = size;
        }
        let refreshed = gpu.refresh(plane.id.0, &plane, None, None);
        if !refreshed.kind.starts_with("gpu-") {
            return None;
        }
        planes.push(MovePlane { key: plane.id.0, size: [plane.size.width, plane.size.height], origin, moving, offset: [0, 0] });
    }
    Some(planes)
}

/// Current planes and offset; after the first frame this does no pixel work or texture upload.
pub(crate) fn planes(app: &mut PhotocraftApp, idx: usize) -> Option<Vec<MovePlane>> {
    if app.session.active_index() != Some(idx)
        || app.ui.tool != Tool::Move
        || app.gpu.is_none()
        || app.ui.transform.is_some()
        || app.drag.as_ref().is_some_and(|d| d.tool != Tool::Move)
        || app.session.documents().get(idx)?.doc.selection.is_some()
    {
        return None;
    }
    let offset = crate::move_ui::drag_offset(app);
    let st = app.session.documents().get(idx)?;
    let (doc, ids) = crate::move_mods::copy_source(app)
        .unwrap_or_else(|| (st.doc.clone(), photocraft_engine::layer_multi_cmds::move_targets(&st.doc, &st.selected_layers())));
    // Prepare while Move is idle. Selecting the same layer changes the session revision but
    // leaves the immutable document intact, so a press can reuse the ready textures.
    let fresh = app.move_fast.as_ref().is_some_and(|f| std::sync::Arc::ptr_eq(&f.source, &doc) && f.ids == ids);
    if !fresh {
        let t = crate::gpu_canvas::now_ms();
        let planes = build(app, &doc, &ids).unwrap_or_default();
        app.move_fast = Some(FastMove { source: doc, ids, planes });
        app.perf.span("move cache", crate::gpu_canvas::now_ms() - t);
    }
    let offset = offset?;
    let f = app.move_fast.as_ref()?;
    if !f.showing() {
        return None;
    }
    let gpu = app.gpu.as_ref()?;
    if f.planes.iter().any(|p| !gpu.has(p.key, p.size)) {
        app.move_fast = None;
        return None;
    }
    let mut planes = f.planes.clone();
    for p in &mut planes {
        if p.moving {
            p.offset = [offset.0, offset.1];
        }
    }
    Some(planes)
}
