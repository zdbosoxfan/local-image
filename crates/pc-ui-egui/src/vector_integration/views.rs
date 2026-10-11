//! Session-only view paint policy and exact device overlays on the normal canvas.
use super::*;
use crate::view_modes::{self, Geometry};
use photocraft_doc::{DocId, Layer, Path};
use photocraft_engine::{EngineError, Result};
use photocraft_geom::{Affine, Rect};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct State {
    documents: HashMap<DocId, Policy>,
    overlays: HashMap<(egui::Id, DocId), Overlay>,
}
impl State {
    pub(crate) fn retain(&mut self, open: &[DocId]) {
        self.documents.retain(|id, _| open.contains(id));
        self.overlays.retain(|(_, id), _| open.contains(id));
    }
}
pub(crate) fn retain(app: &mut PhotocraftApp, open: &[DocId]) {
    app.vector_runtime.views.retain(open);
}
struct Policy {
    original: Arc<Document>,
    projected: Arc<Document>,
    cache: view_modes::FrameCache,
    frame: Option<Arc<view_modes::Frame>>,
    base: Arc<Document>,
}
#[derive(Default)]
struct Overlay {
    cache: view_modes::FrameCache,
    frame: Option<Arc<view_modes::Frame>>,
    texture: Option<egui::TextureHandle>,
}
pub(super) struct SharedGeometry<'a>(pub &'a Document);
impl Geometry for SharedGeometry<'_> {
    fn paths(&self, _: &Document, source: &Layer) -> Result<Vec<Path>> {
        let shown = self.0.layer(source.id).ok_or(EngineError::NoLayer(source.id))?;
        fn collect(doc: &Document, layer: &Layer, out: &mut Vec<Path>) -> Result<()> {
            out.extend(view_modes::NativeGeometry.paths(doc, layer)?);
            if let Some(children) = layer.children() {
                for child in children {
                    collect(doc, child, out)?;
                }
            }
            Ok(())
        }
        if source.vector.live.is_some() || matches!(source.content, LayerContent::Symbol(_)) {
            let mut out = Vec::new();
            collect(self.0, shown, &mut out)?;
            Ok(out)
        } else {
            view_modes::NativeGeometry.paths(self.0, shown)
        }
    }
    fn xray_layer(&self, _: &Document, source: &Layer) -> Result<Layer> {
        fn fade(doc: &Document, layer: &Layer) -> Result<Layer> {
            let mut out = view_modes::NativeGeometry.xray_layer(doc, layer)?;
            if let Some(children) = out.children_mut() {
                for child in children {
                    *child = fade(doc, child)?;
                }
            }
            Ok(out)
        }
        let shown = self.0.layer(source.id).ok_or(EngineError::NoLayer(source.id))?;
        fade(self.0, shown)
    }
}
fn policy(app: &PhotocraftApp, idx: usize) -> (view_modes::State, photocraft_engine::vector_packages::isolation::Scope) {
    let st = &app.session.documents()[idx];
    (
        st.vector_view.clone(),
        st.vector_isolation.scope(&st.doc).unwrap_or(photocraft_engine::vector_packages::isolation::Scope { document: st.doc.id, target: None }),
    )
}
pub(super) fn display(app: &mut PhotocraftApp, idx: usize, original: Arc<Document>, projected: Arc<Document>, key: u64) -> Result<(Arc<Document>, u64)> {
    let (state, scope) = policy(app, idx);
    if let Some(gpu) = &app.gpu {
        gpu.set_pixel_preview(original.id.0, state.mode == view_modes::Mode::PixelPreview);
    }
    let rev = app.session.documents()[idx].revision;
    let entry = app.vector_runtime.views.documents.entry(original.id).or_insert_with(|| Policy {
        original: original.clone(),
        projected: projected.clone(),
        cache: Default::default(),
        frame: None,
        base: projected.clone(),
    });
    entry.original = original.clone();
    entry.projected = projected.clone();
    // Ordinary previews retain the existing key and shared raster tiles. A view
    // policy key is needed only when paint or isolation actually changes the view.
    if state.mode == view_modes::Mode::Vector && state.layer_outlines.is_empty() && scope.target.is_none() {
        entry.base = projected.clone();
        entry.frame = None;
        return Ok((projected, key));
    }
    let frame = entry.cache.prepare(&original, rev, key, &state, scope, Affine::IDENTITY, original.bounds(), &SharedGeometry(&projected))?;
    if entry.frame.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &frame)) {
        let base = match &frame.base {
            Some(base) => photocraft_engine::vector_integration::project_document(base)?,
            None => {
                let mut blank = (*original).clone();
                blank.layers.clear();
                blank
            }
        };
        entry.base = Arc::new(base);
        entry.frame = Some(frame.clone());
    }
    Ok((entry.base.clone(), key ^ frame.view_key.rotate_left(13)))
}
pub fn pixel_preview(app: &PhotocraftApp, idx: usize) -> bool {
    app.session.documents()[idx].vector_view.mode == view_modes::Mode::PixelPreview
}
pub fn overlay(app: &mut PhotocraftApp, ctx: &egui::Context, idx: usize, xf: &ViewXform, painter: &egui::Painter) -> Result<()> {
    let (state, scope) = policy(app, idx);
    let st = &app.session.documents()[idx];
    if matches!(state.mode, view_modes::Mode::Vector | view_modes::Mode::PixelPreview) && state.layer_outlines.is_empty() && scope.target.is_none() {
        if let Some(entry) = app.vector_runtime.views.overlays.get_mut(&(ctx.viewport_id().0, st.doc.id)) {
            entry.texture = None;
            entry.frame = None;
        }
        return Ok(());
    }
    let Some(policy) = app.vector_runtime.views.documents.get(&st.doc.id) else { return Ok(()) };
    let ppp = f64::from(ctx.pixels_per_point());
    let origin = xf.to_screen(0., 0.);
    let scale = f64::from(xf.zoom) * ppp;
    let transform = Affine { m: [if xf.flip { -scale } else { scale }, 0., 0., scale, f64::from(origin.x) * ppp, f64::from(origin.y) * ppp] };
    let rect = xf.rect;
    let clip = Rect::new(
        (f64::from(rect.left()) * ppp).round() as i32,
        (f64::from(rect.top()) * ppp).round() as i32,
        (f64::from(rect.right()) * ppp).round() as i32,
        (f64::from(rect.bottom()) * ppp).round() as i32,
    );
    let entry = app.vector_runtime.views.overlays.entry((ctx.viewport_id().0, st.doc.id)).or_default();
    let frame = entry.cache.prepare(
        &policy.original,
        st.revision,
        policy.frame.as_ref().map_or(0, |f| f.view_key),
        &state,
        scope,
        transform,
        clip,
        &SharedGeometry(&policy.projected),
    )?;
    if frame.outlines.is_empty() {
        entry.texture = None;
        entry.frame = Some(frame);
        return Ok(());
    }
    if entry.frame.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &frame)) {
        let buffer = frame.overlay_buffer(clip)?;
        let pixels = buffer.px.iter().map(|p| egui::Color32::from_black_alpha((p[3].clamp(0., 1.) * 255.).round() as u8)).collect();
        let image = egui::ColorImage::new([clip.width() as usize, clip.height() as usize], pixels);
        match &mut entry.texture {
            Some(texture) => texture.set(image, egui::TextureOptions::NEAREST),
            None => entry.texture = Some(ctx.load_texture("vector-device-outline", image, egui::TextureOptions::NEAREST)),
        }
        entry.frame = Some(frame);
    }
    if let Some(texture) = &entry.texture {
        let rect = egui::Rect::from_min_max(
            egui::pos2(clip.x0 as f32 / ppp as f32, clip.y0 as f32 / ppp as f32),
            egui::pos2(clip.x1 as f32 / ppp as f32, clip.y1 as f32 / ppp as f32),
        );
        painter.image(texture.id(), rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)), egui::Color32::WHITE);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bugfix_develop_closed_document_releases_view_policy() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", serde_json::json!({"width":32,"height":24})).unwrap();
        let doc = app.session.active().unwrap().doc.clone();
        let weak = Arc::downgrade(&doc);
        app.canvases.insert(
            (doc.id, 0),
            crate::canvas::CanvasCache { revision: 0, texture: None, scale: 1., preview_key: 0, on_gpu: false, tex_revision: 0, tex_preview_key: 0 },
        );
        display(&mut app, 0, doc.clone(), doc.clone(), 0).unwrap();
        drop(doc);
        app.run("file.close", serde_json::json!({})).unwrap();
        crate::canvas::retain_gpu_documents(&mut app);
        assert!(app.vector_runtime.views.documents.is_empty());
        assert!(app.canvases.is_empty());
        assert!(weak.upgrade().is_none());
    }
}
