//! Shared vector providers. Installed once for UI, automation, and headless sessions.
use crate::{EngineError, Result, vector_packages as packages};
use photocraft_doc::{Document, LayerContent, ShapeLayer, VectorData};
use std::sync::Once;

fn error(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}

pub fn initialize() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        #[cfg(test)]
        let _ = packages::path_effects::gpu::scaled::install(|doc, scale| {
            photocraft_compose::scaled_view::ProjectionCache::default()
                .project_at(doc, doc.bounds(), scale, &Default::default(), project_view)
                .map_err(|e| error(e.to_string()))
        });
        let _ = packages::isolation::install_compound_inserter(|session, capture, parent, layer| {
            let state = packages::isolation::authoritative(session).unwrap_or_default();
            capture.check(session, &state)?;
            let c = packages::clip_compound::Capture::begin(session, &[parent])?;
            let id = layer.id;
            let prepared = packages::clip_compound::prepare_insert(session, &c, packages::clip_compound::DrawMode::Inside, Some(parent), layer)?;
            packages::clip_compound::publish(session, prepared, "Create Object")?;
            Ok(id)
        });
        let _ = packages::node::install_geometry(packages::node::Geometry {
            nearest: |path, point| photocraft_pathops::nearest(path, point).map_err(|e| e.to_string()),
            join: |paths, tolerance| photocraft_pathops::join(paths, tolerance).map_err(|e| e.to_string()),
            split: |path, subpath, segment, t| photocraft_pathops::split_at(path, subpath, segment, t).map_err(|e| e.to_string()),
        });
        let _ = packages::snapping::query::install_geometry(packages::snapping::query::Geometry {
            nearest: |path, point| photocraft_pathops::nearest(path, point).map_err(|e| error(e.to_string())),
            bounds: |path| photocraft_pathops::bounds(path).map_err(|e| error(e.to_string())),
        });
        let _ = packages::mesh::install_view_hooks(packages::mesh::ViewHooks {
            project: |doc, scale| {
                photocraft_compose::scaled_view::ProjectionCache::default()
                    .project_at(doc, doc.bounds(), scale, &Default::default(), project_view)
                    .map_err(|e| error(e.to_string()))
            },
        });
        let _ = packages::node::install_renderer(render_shape);
        let _ = packages::anchor_tools::install_renderer(render_shape);
        let _ = packages::styles::install_hooks(packages::styles::Hooks {
            read: |doc| {
                Ok(packages::styles::Store {
                    graphic_styles: doc.graphic_styles.clone(),
                    links: doc.walk().into_iter().filter_map(|(_, _, l)| l.vector.graphic_style.clone().map(|link| (l.id, link))).collect(),
                })
            },
            write: |doc, store| {
                doc.graphic_styles = store.graphic_styles.clone();
                for (_, _, layer) in doc.walk() {
                    if store.links.contains_key(&layer.id) && !store.graphic_styles.iter().any(|s| Some(s.id) == store.links.get(&layer.id).map(|l| l.style)) {
                        return Err(error("dangling graphic style link"));
                    }
                }
                let ids: Vec<_> = doc.walk().into_iter().map(|(_, _, l)| l.id).collect();
                for id in ids {
                    if let Some(layer) = doc.layer_mut(id) {
                        layer.vector.graphic_style = store.links.get(&id).cloned();
                    }
                }
                Ok(())
            },
        });
        let _ = packages::colours::install_access(packages::colours::Access {
            read: |doc| Ok(doc.swatches.clone()),
            write: |doc, book| {
                book.validate().map_err(error)?;
                doc.swatches = book;
                Ok(())
            },
        });
        let _ = packages::path_effects::install_access(packages::path_effects::Access {
            read: |layer| Ok(layer.vector.effects.clone()),
            write: |layer, effects| {
                layer.vector.effects = effects;
                Ok(())
            },
        });
        let _ = packages::selection::install_hooks(packages::selection::Hooks {
            scope: |session| {
                let st = session.active().ok_or(EngineError::NoDocument)?;
                Ok(packages::selection::Scope {
                    active: st.active_layer,
                    isolation: st.vector_isolation.current().map(|target| target.layer()),
                    ..st.vector_selection_scope.clone()
                })
            },
            previous: |session| session.active().map(|st| st.vector_selection.previous.clone()).unwrap_or_default(),
            remember: |session, previous| {
                if let Some(st) = session.active_mut() {
                    st.vector_selection.previous = previous;
                }
            },
        });
        let _ = packages::selection::install_readers(packages::selection::Readers {
            appearance: Some(|layer| match &layer.content {
                LayerContent::Shape(shape) => Ok(layer.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(shape))),
                _ => Ok(layer.vector.appearance.clone().unwrap_or_default()),
            }),
            graphic_style: Some(|layer| packages::styles::style_key(layer, layer.vector.graphic_style.as_ref())),
            rotation: Some(|layer| {
                Ok(match &layer.content {
                    LayerContent::Text(text) => Some(text.transform.m[1].atan2(text.transform.m[0])),
                    LayerContent::Symbol(instance) => Some(instance.transform.m[1].atan2(instance.transform.m[0])),
                    LayerContent::Shape(_) => layer.vector.live_shape.as_ref().map(|live| live.angle),
                    _ => None,
                })
            }),
            symbol_instance: Some(|layer| matches!(layer.content, LayerContent::Symbol(_))),
        });
        let _ = packages::isolation::install_hooks(packages::isolation::Hooks {
            exit_all: |session| {
                for st in &mut session.docs {
                    st.vector_isolation = Default::default();
                }
            },
            read: |session| session.active().map(|st| st.vector_isolation.clone()).unwrap_or_default(),
            write: |session, state| {
                if let Some(st) = session.active_mut() {
                    st.vector_isolation = state;
                }
            },
        });
        let _ = packages::isolation::install_view_hooks(packages::isolation::ViewHooks {
            read: |session| session.active().map(|st| st.vector_view.clone()).unwrap_or_default(),
            write: |session, state| {
                if let Some(st) = session.active_mut() {
                    st.vector_view = state;
                }
            },
        });
    });
}

pub fn render_shape(doc: &Document, source: &ShapeLayer, vector: &VectorData) -> Result<photocraft_doc::Surface> {
    let mut shape = source.clone();
    if let Some(live) = &vector.live_shape {
        shape.path = photocraft_vector::design_shapes::generate(live).map_err(|e| error(e.to_string()))?;
    }
    if !vector.corners.is_empty() {
        shape = packages::corners::project_shape(&shape, &vector.corners)?;
    }
    let plan = photocraft_vector::appearance::CompiledAppearance::with_projection(
        &shape,
        vector.appearance.as_ref(),
        &doc.patterns,
        doc.bounds(),
        &packages::styles::Projection,
    )
    .map_err(error)?;
    if plan.cache_bytes(doc.pixel_format()) > 512 << 20 {
        return Err(error("vector cache exceeds 512 MiB"));
    }
    plan.render(doc.pixel_format(), doc.bounds()).map_err(error)
}

/// Drop links whose layers or paint slots were removed in this same edit.
pub fn maintain_metadata(doc: &mut Document) -> Result<()> {
    packages::styles::prune_links(doc)?;
    let links = doc.swatches.links.clone();
    doc.swatches.links = links.into_iter().filter(|link| packages::colours::color(doc, &link.target).is_ok()).collect();
    Ok(())
}

#[cfg(test)]
#[test]
#[allow(clippy::unwrap_used)]
fn bugfix_save_o031_locked_swatch_links_survive_unlock_and_prune_deleted_targets() {
    initialize();
    use photocraft_doc::vector_design::swatches::*;
    let mut doc = Document::new("links", photocraft_doc::Size::new(32, 24), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
    let mut layer = photocraft_doc::Layer::new(
        "shape",
        LayerContent::Shape(ShapeLayer { fill: Some(photocraft_doc::Fill::Solid(photocraft_doc::Color::BLACK)), ..Default::default() }),
    );
    let target = PaintTarget { symbol: None, layer: layer.id, paint: PaintSlot::Fill, stop: None };
    let link = PaintLink { target: target.clone(), reference: SwatchRef { swatch: SwatchId(1), tint: 100. } };
    layer.locks.all = true;
    doc.layers.push(layer);
    doc.swatches.links.push(link.clone());
    maintain_metadata(&mut doc).unwrap();
    assert_eq!(doc.swatches.links, vec![link]);
    doc.layers[0].locks.all = false;
    assert!(packages::colours::validate_target(&doc, &target).is_ok());
    doc.layers.clear();
    maintain_metadata(&mut doc).unwrap();
    assert!(doc.swatches.links.is_empty());
}

/// One detached projection traversal for canvas, exports and expanded objects.
/// Sources remain native and editable; evaluated metadata is cleared only on this copy.
pub fn project_document(source: &Document) -> Result<Document> {
    project_document_cancellable(source, &|| false, true)
}

fn project_document_cancellable(source: &Document, cancelled: &dyn Fn() -> bool, refresh_type: bool) -> Result<Document> {
    use photocraft_doc::vector_design::live::LiveGroup;
    fn ids(layer: &photocraft_doc::Layer, out: &mut std::collections::HashSet<photocraft_doc::LayerId>) {
        out.insert(layer.id);
        if let Some(children) = layer.children() {
            for child in children {
                ids(child, out);
            }
        }
    }
    fn remap_generated(layer: &mut photocraft_doc::Layer, original: &std::collections::HashSet<photocraft_doc::LayerId>, next: &mut u64) -> Result<()> {
        if !original.contains(&layer.id) {
            *next = next.checked_add(1).ok_or_else(|| error("vector projection ID overflow"))?;
            layer.id = photocraft_doc::LayerId(*next);
        }
        if let Some(children) = layer.children_mut() {
            for child in children {
                remap_generated(child, original, next)?;
            }
        }
        Ok(())
    }
    fn project(source: &Document, layer: &mut photocraft_doc::Layer, next: &mut u64, cancelled: &dyn Fn() -> bool, refresh_type: bool) -> Result<()> {
        if cancelled() {
            return Err(EngineError::Cancelled);
        }
        if let Some(children) = layer.children_mut() {
            for child in children {
                project(source, child, next, cancelled, refresh_type)?;
            }
        }
        if matches!(layer.content, LayerContent::Symbol(_)) {
            let ex = packages::symbols::expansion::expand(source, std::slice::from_ref(layer)).map_err(|e| error(e.to_string()))?;
            let expanded =
                if refresh_type { packages::symbols::prepare_expansion(source, ex)? } else { packages::symbols::prepare_expansion_for_export(source, ex)? };
            *layer = expanded.into_iter().next().ok_or_else(|| error("empty symbol projection"))?;
        }
        if layer.vector.live.is_some() {
            let mut original = std::collections::HashSet::new();
            ids(layer, &mut original);
            let mut isolated = source.clone();
            isolated.layers = vec![layer.clone()];
            let projected = match layer.vector.live {
                Some(LiveGroup::Blend(_)) => photocraft_vecfx::blend::project_document(&isolated).map_err(|e| error(e.to_string()))?,
                Some(LiveGroup::Repeat(_)) => packages::repeat::project_document(&isolated)?,
                Some(LiveGroup::Envelope(_)) => packages::envelope::project_document(&isolated)?,
                Some(LiveGroup::LivePaint(_)) => photocraft_vecfx::live_paint::project_document(&isolated).map_err(|e| error(e.to_string()))?,
                None => isolated,
            };
            *layer = projected.layers.into_iter().next().ok_or_else(|| error("empty live group projection"))?;
            // Isolated evaluators reserve only their subtree's IDs. Reserve against the
            // entire source and preceding projections before the GPU indexes by layer ID.
            remap_generated(layer, &original, next)?;
        }
        if !layer.vector.effects.is_empty() {
            let scene = packages::path_effects::project(layer, &layer.vector.effects, source.resolution_dpi)?;
            *layer = packages::path_effects::projected_layer(layer, &scene, source)?;
        }
        if let LayerContent::Shape(shape) = &mut layer.content {
            if let Some(live) = &layer.vector.live_shape {
                shape.path = photocraft_vector::design_shapes::generate(live).map_err(|e| error(e.to_string()))?;
            }
            if !layer.vector.corners.is_empty() {
                *shape = packages::corners::project_shape(shape, &layer.vector.corners)?;
            }
            if !layer.vector.is_empty() || shape.cache.is_none() {
                shape.cache = Some(render_shape(source, shape, &VectorData { appearance: layer.vector.appearance.clone(), ..Default::default() })?);
            }
            layer.vector.live_shape = None;
            layer.vector.corners.clear();
            shape.live = None;
        }
        if let LayerContent::Text(text) = &mut layer.content
            && refresh_type
            && (layer.vector.on_path.is_some() || frame_options(&layer.vector).enabled)
        {
            refresh_text(source, text, &layer.vector)?;
        }
        if cancelled() {
            return Err(EngineError::Cancelled);
        }
        Ok(())
    }
    if cancelled() {
        return Err(EngineError::Cancelled);
    }
    let mut out = source.clone();
    let mut next = photocraft_vecfx::blend::max_document_id(source);
    for layer in &mut out.layers {
        project(source, layer, &mut next, cancelled, refresh_type)?;
    }
    let out = packages::layout::project_clips(&out)?;
    if cancelled() {
        return Err(EngineError::Cancelled);
    }
    Ok(out)
}

/// PSD/TIFF fallback evaluates the same native features as the canvas. Only the detached
/// layer's content is baked; the original compositing envelope is serialized once.
struct LayeredSaveProjection;
impl photocraft_io::vector_map::fallback::Projection for LayeredSaveProjection {
    fn layer(&self, document: &Document, layer: &photocraft_doc::Layer) -> std::result::Result<Option<photocraft_doc::Layer>, String> {
        save_layer_projection(document, layer, project_document)
    }
}

fn save_layer_projection(
    document: &Document,
    layer: &photocraft_doc::Layer,
    project: impl FnOnce(&Document) -> Result<Document>,
) -> std::result::Result<Option<photocraft_doc::Layer>, String> {
    let mut isolated = document.clone();
    let mut content = layer.clone();
    content.visible = true;
    content.opacity = 1.;
    content.fill_opacity = 1.;
    content.blend = photocraft_doc::BlendMode::Normal;
    content.clipped = false;
    content.mask = None;
    content.vector_mask = None;
    content.effects = Default::default();
    content.blend_if = Default::default();
    isolated.layers = vec![content];
    let projected = project(&isolated).map_err(|e| e.to_string())?;
    let mut out = layer.clone();
    out.content = LayerContent::Raster(photocraft_compose::flatten_to_surface(&projected, document.pixel_format(), None));
    out.vector = Default::default();
    out.psd_blocks.retain(|(key, _)| ![b"vmsk", b"vsms", b"vogk", b"vstk", b"vscg", b"SoCo", b"GdFl", b"PtFl"].contains(&key));
    Ok(Some(out))
}

#[path = "vector_integration/save_worker.rs"]
mod save_worker;
pub use save_worker::export_document_background;

/// Normal application and headless layered-save bridge, with original editability losses.
pub fn export_document(
    source: &Document,
    name: &str,
    options: &photocraft_io::ExportOptions,
) -> std::result::Result<photocraft_io::ExportResult, photocraft_io::IoError> {
    let ext = std::path::Path::new(name).extension().and_then(|s| s.to_str()).unwrap_or(name).to_ascii_lowercase();
    if matches!(ext.as_str(), "pcraft" | "psd" | "psb") || ext == "tif" && options.tiff_layers || ext == "tiff" && options.tiff_layers {
        return photocraft_io::export_with_projection(source, name, options, Some(&LayeredSaveProjection));
    }
    let projected = project_document(source).map_err(|e| photocraft_io::IoError::Unsupported(e.to_string()))?;
    photocraft_io::export(&projected, name, options)
}

/// Native editable vector imports share the file worker's cancellation and publication.
pub fn read_vector_file(name: &str, bytes: &[u8], cancel: &std::sync::atomic::AtomicBool) -> Option<Result<photocraft_io::ImportResult>> {
    let result = if photocraft_affinity::is_affinity(bytes) {
        packages::affinity::read_open(bytes, 72., cancel).map(|scene| packages::affinity::document(scene, name))
    } else if photocraft_svg::import::recognizes(bytes) {
        photocraft_svg::import_with(bytes, &Default::default(), None, cancel).map(|scene| packages::svg_import::document(scene, name)).map_err(|e| match e {
            photocraft_svg::ImportError::Cancelled => EngineError::Cancelled,
            e => EngineError::Other(e.to_string()),
        })
    } else {
        return None;
    };
    Some(result.map(|(document, warnings)| photocraft_io::ImportResult { document, warnings: warnings.into_iter().map(|w| w.message).collect() }))
}
pub fn is_native_vector_file(bytes: &[u8]) -> bool {
    photocraft_affinity::is_affinity(bytes) || photocraft_svg::import::recognizes(bytes)
}

/// All native text mutations use the layer's retained layout metadata.
pub fn frame_options(vector: &VectorData) -> photocraft_text::frame_compose::Options {
    let m = vector.frame_compose.unwrap_or_default();
    photocraft_text::frame_compose::Options {
        enabled: m.enabled,
        balanced: m.balanced,
        optical_margins: m.optical_margins,
        grid_spacing: m.grid_spacing,
        grid_origin: m.grid_origin,
    }
}
pub fn text_layout(
    engine: &mut photocraft_text::TextEngine,
    doc: &Document,
    text: &photocraft_doc::TextLayer,
    vector: &VectorData,
) -> Result<photocraft_text::TextLayout> {
    photocraft_text::frame_compose::compose(engine, text, doc.resolution_dpi, frame_options(vector)).map(|c| c.layout).map_err(|e| error(e.to_string()))
}
pub fn refresh_text(doc: &Document, text: &mut photocraft_doc::TextLayer, vector: &VectorData) -> Result<()> {
    if let Some(path) = &vector.on_path {
        packages::path_text::refresh(doc, text, path)?;
        return Ok(());
    }
    if !frame_options(vector).enabled {
        crate::type_cmds::refresh(doc, text);
        return Ok(());
    }
    let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine unavailable"))?;
    let composition =
        photocraft_text::frame_compose::compose(&mut engine, text, doc.resolution_dpi, frame_options(vector)).map_err(|e| error(e.to_string()))?;
    let rendered = photocraft_text::frame_compose::render_bounded(&composition, text, doc.pixel_format(), doc.bounds()).map_err(|e| error(e.to_string()))?;
    text.cache = Some(rendered.surface);
    text.psd_raw = Some(std::sync::Arc::new(photocraft_text::psd::build_tysh(text, doc.resolution_dpi, composition.layout.bounds())));
    Ok(())
}

/// Detached worker projection with a private font/layout snapshot and cooperative checks.
pub fn project_for_export_with(
    source: &Document,
    text: &mut photocraft_text::TextEngine,
    cancelled: &dyn Fn() -> bool,
) -> Result<(Document, Vec<photocraft_doc::vector_design::exchange::VectorWarning>)> {
    use photocraft_doc::vector_design::appearance::{AppearanceItem, StrokeGradientMode};
    use photocraft_doc::{Fill, Layer, LayerContent};
    fn rec(
        doc: &Document,
        layer: &mut Layer,
        warnings: &mut Vec<photocraft_doc::vector_design::exchange::VectorWarning>,
        engine: &mut photocraft_text::TextEngine,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<()> {
        if cancelled() {
            return Err(EngineError::Cancelled);
        }
        if let Some(children) = layer.children_mut() {
            for child in children {
                rec(doc, child, warnings, engine, cancelled)?;
            }
        }
        if photocraft_doc::vector_design::mosaic::active(layer) {
            let surface = photocraft_compose::mosaic::surface(
                layer,
                photocraft_compose::composite_bounds(layer, doc.bounds()).unwrap_or(doc.bounds()).intersect(&doc.bounds()),
                1.0,
                doc.pixel_format(),
            )
            .ok_or_else(|| error("invalid mosaic projection"))?;
            layer.content = LayerContent::Raster(surface);
            layer.vector.mosaic = None;
            warnings.push(photocraft_doc::vector_design::exchange::VectorWarning {
                feature: "cutout coverage".into(),
                object: Some(layer.name.clone()),
                message: "Abutting coverage was embedded as an image to preserve seamless rendering.".into(),
            });
        }
        if let LayerContent::Text(text) = &layer.content {
            if let Some(path) = &layer.vector.on_path {
                let layout = packages::path_text::project(engine, doc, text, path)?;
                let paints = photocraft_text::on_path::validated_outlines(&layout, &text.transform).map_err(|e| error(e.to_string()))?;
                let children = packages::path_text::curves::paths(&paints)
                    .into_iter()
                    .map(|(color, path)| {
                        Layer::new("Text curves", LayerContent::Shape(ShapeLayer { path, fill: Some(Fill::Solid(color)), ..Default::default() }))
                    })
                    .collect();
                layer.content = Layer::group("Text curves", children).content;
                layer.vector.on_path = None;
            } else if frame_options(&layer.vector).enabled {
                let layout = text_layout(engine, doc, text, &layer.vector)?;
                let warp = photocraft_text::render::layout_warp(&layout, text.warp.as_ref());
                let paints = photocraft_text::render::painted_outlines(&layout, &text.transform, warp.as_ref())
                    .into_iter()
                    .map(|(color, elements)| photocraft_text::on_path::PaintedOutline { color, elements })
                    .collect::<Vec<_>>();
                let children = packages::path_text::curves::paths(&paints)
                    .into_iter()
                    .map(|(color, path)| {
                        Layer::new("Composed text curves", LayerContent::Shape(ShapeLayer { path, fill: Some(Fill::Solid(color)), ..Default::default() }))
                    })
                    .collect();
                layer.content = Layer::group("Composed text curves", children).content;
                layer.vector.frame_compose = None;
            }
        }
        let advanced = layer.vector.appearance.as_ref().is_some_and(|a| {
            a.items.iter().any(|item| match item {
                AppearanceItem::Fill(f) => matches!(f.paint, Fill::Pattern { .. }),
                AppearanceItem::Stroke(s) => {
                    matches!(s.base.paint, Fill::Pattern { .. })
                        || s.brush.is_some()
                        || s.profile.is_some()
                        || s.start_arrow.is_some()
                        || s.end_arrow.is_some()
                        || s.dash_align
                        || s.gradient_mode != StrokeGradientMode::Object
                }
                AppearanceItem::Mesh(_) => false,
            })
        }) || matches!(&layer.content, LayerContent::Shape(s) if s.fill.as_ref().is_some_and(|f|matches!(f, Fill::Pattern{..})) || s.stroke.as_ref().is_some_and(|s|matches!(s.paint,Fill::Pattern{..})));
        if advanced && let LayerContent::Shape(shape) = &layer.content {
            let surface = render_shape(doc, shape, &layer.vector)?;
            layer.content = LayerContent::Raster(surface);
            layer.vector.appearance = None;
            warnings.push(photocraft_doc::vector_design::exchange::VectorWarning {
                feature: "advanced paint".into(),
                object: Some(layer.name.clone()),
                message: "Advanced paint was embedded as an image to preserve its rendering.".into(),
            });
        }
        if cancelled() {
            return Err(EngineError::Cancelled);
        }
        Ok(())
    }
    let mut doc = project_document_cancellable(source, cancelled, false)?;
    let snapshot = doc.clone();
    let mut warnings = Vec::new();
    for layer in &mut doc.layers {
        rec(&snapshot, layer, &mut warnings, text, cancelled)?;
    }
    Ok((doc, warnings))
}

pub fn project_view(
    doc: &photocraft_doc::Document,
    layer: &photocraft_doc::Layer,
    scale: f64,
    clip: photocraft_geom::Rect,
) -> std::result::Result<photocraft_raster::Surface, photocraft_compose::scaled_view::ViewError> {
    let error = |e: String| photocraft_compose::scaled_view::ViewError(e);
    if u64::from(clip.width()) * u64::from(clip.height()) > photocraft_compose::scaled_view::MAX_VIEW_PIXELS {
        return Err(error("display clip exceeds raster budget".into()));
    }
    match &layer.content {
        photocraft_doc::LayerContent::Shape(source) => {
            let mut shape = source.clone();
            if let Some(live) = &layer.vector.live_shape {
                shape.path = photocraft_vector::design_shapes::generate(live).map_err(|e| error(e.to_string()))?;
            }
            shape = packages::corners::project_shape(&shape, &layer.vector.corners).map_err(|e| error(e.to_string()))?;
            let mut appearance = layer.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(&shape));
            // A view scale always scales device coverage, independently of the edit preference.
            for item in &mut appearance.items {
                match item {
                    photocraft_doc::vector_design::appearance::AppearanceItem::Fill(fill) => photocraft_vector::view::scale_paint(&mut fill.paint, scale),
                    photocraft_doc::vector_design::appearance::AppearanceItem::Stroke(stroke) => {
                        stroke.scale_with_object = true;
                        photocraft_vector::view::scale_paint(&mut stroke.base.paint, scale);
                    }
                    photocraft_doc::vector_design::appearance::AppearanceItem::Mesh(_) => {}
                }
            }
            photocraft_vector::appearance::scale_with_object(&mut appearance, photocraft_geom::Affine::scale(scale)).map_err(error)?;
            shape = photocraft_vector::view::project_shape(&shape, scale).map_err(|e| error(e.into()))?;
            let canvas = photocraft_compose::scaled_view::scaled_rect(doc.bounds(), scale)?;
            photocraft_vector::appearance::CompiledAppearance::with_projection(&shape, Some(&appearance), &doc.patterns, canvas, &packages::styles::Projection)
                .and_then(|plan| plan.render(doc.pixel_format(), clip))
                .map_err(error)
        }
        photocraft_doc::LayerContent::Text(text) => {
            let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine unavailable".into()))?;
            if let Some(path) = &layer.vector.on_path {
                let layout = packages::path_text::project(&mut engine, doc, text, path).map_err(|e| error(e.to_string()))?;
                let transform = photocraft_geom::Affine::scale(scale).mul(&text.transform);
                photocraft_text::on_path::rasterize(&layout, &transform, doc.pixel_format(), text.antialias, clip)
                    .map(|r| r.surface)
                    .map_err(|e| error(e.to_string()))
            } else if frame_options(&layer.vector).enabled {
                let composition = photocraft_text::frame_compose::compose(&mut engine, text, doc.resolution_dpi, frame_options(&layer.vector))
                    .map_err(|e| error(e.to_string()))?;
                photocraft_text::view::render_layout_view(&composition.layout, text, doc.pixel_format(), clip, scale).map_err(|e| error(e.into()))
            } else {
                photocraft_text::view::render_text_view(&mut engine, text, doc.resolution_dpi, doc.pixel_format(), clip, scale).map_err(|e| error(e.into()))
            }
        }
        photocraft_doc::LayerContent::Symbol(_) => {
            let mut bare = layer.clone();
            bare.opacity = 1.;
            bare.fill_opacity = 1.;
            bare.visible = true;
            bare.mask = None;
            bare.vector_mask = None;
            bare.effects = Default::default();
            bare.blend = photocraft_doc::BlendMode::Normal;
            bare.clipped = false;
            let mut isolated = doc.clone();
            isolated.layers = vec![bare];
            let projected = project_document(&isolated).map_err(|e| error(e.to_string()))?;
            let visible = photocraft_geom::Rect::new(
                (f64::from(clip.x0) / scale).floor() as i32,
                (f64::from(clip.y0) / scale).floor() as i32,
                (f64::from(clip.x1) / scale).ceil() as i32,
                (f64::from(clip.y1) / scale).ceil() as i32,
            );
            let tiles = photocraft_compose::scaled_view::flatten_scaled_with(&projected, visible, scale, project_view)?;
            let mut surface = photocraft_raster::Surface::new(doc.pixel_format());
            for tile in tiles.tiles {
                let n = doc.pixel_format().channels();
                let mut pixels = vec![0.; tile.px.len() * n];
                for (p, out) in tile.px.into_iter().zip(pixels.chunks_exact_mut(n)) {
                    photocraft_raster::from_rgba_into(&doc.pixel_format(), p, out);
                }
                surface.write_region(tile.rect, &pixels);
            }
            surface.prune();
            Ok(surface)
        }
        _ => Err(error("expected display geometry".into())),
    }
}

#[derive(Clone)]
pub struct Insertion {
    scope: packages::isolation::State,
    capture: packages::isolation::Capture,
    mode: packages::clip_compound::DrawMode,
    target: Option<photocraft_doc::LayerId>,
    selected: Vec<photocraft_doc::LayerId>,
    container: photocraft_doc::LayerId,
}
pub fn capture_insertion(session: &mut crate::Session, mode: packages::clip_compound::DrawMode) -> Result<()> {
    let scope = packages::isolation::authoritative(session).unwrap_or_default();
    let capture = packages::isolation::Capture::new(session, &scope)?;
    let st = session.active().ok_or(crate::EngineError::NoDocument)?;
    if mode == packages::clip_compound::DrawMode::Inside && st.selected_layers().len() != 1 {
        return Err(error("Draw Inside needs one selected object"));
    }
    session.vector_insertion =
        Some(Insertion { scope, capture, mode, target: st.active_layer, selected: st.selected_layers(), container: photocraft_doc::LayerId::fresh() });
    Ok(())
}
/// Every creator publishes prepared native geometry through the captured insertion policy.
pub fn insert_layer(
    session: &mut crate::Session,
    above: Option<photocraft_doc::LayerId>,
    mut layer: photocraft_doc::Layer,
    label: &str,
) -> Result<photocraft_doc::LayerId> {
    let scope = packages::isolation::authoritative(session).unwrap_or_default();
    let capture = packages::isolation::Capture::new(session, &scope)?;
    let (mode, target) = if let Some(insertion) = &session.vector_insertion {
        insertion.capture.check(session, &scope)?;
        if insertion.scope != scope {
            return Err(error("Insertion scope changed; start again"));
        }
        let st = session.active().ok_or(EngineError::NoDocument)?;
        if st.active_layer != insertion.target || st.selected_layers() != insertion.selected {
            return Err(error("Drawing selection changed; start again"));
        }
        (insertion.mode, insertion.target)
    } else {
        (packages::clip_compound::DrawMode::Normal, above)
    };
    let id = layer.id;
    if mode != packages::clip_compound::DrawMode::Normal {
        let c = packages::clip_compound::Capture::begin(session, &target.into_iter().collect::<Vec<_>>())?;
        let mut prepared = packages::clip_compound::prepare_insert(session, &c, mode, target, layer)?;
        stable_insertion_container(session, &mut prepared, target);
        packages::clip_compound::publish(session, prepared, label)?;
        return Ok(id);
    }
    if let Some(packages::isolation::Target::Compound { layer: parent }) = scope.current() {
        let capture = packages::clip_compound::Capture::begin(session, &[parent])?;
        let mut prepared = packages::clip_compound::prepare_insert(session, &capture, packages::clip_compound::DrawMode::Inside, Some(parent), layer)?;
        stable_insertion_container(session, &mut prepared, Some(parent));
        packages::clip_compound::publish(session, prepared, label)?;
        return Ok(id);
    }
    if scope.current().is_some() {
        let st = session.active().ok_or(crate::EngineError::NoDocument)?;
        let insertion = scope.insertion_target(&st.doc)?;
        if insertion.from_document != photocraft_geom::Affine::IDENTITY {
            let a = insertion.from_document;
            let [a0, b, c, d, e, f] = a.m;
            let h = photocraft_algo::transform::Homography([a0, c, e, b, d, f, 0., 0., 1.]);
            crate::transform_cmds::transform_layer(None, Default::default(), &mut layer, &h, Some(a), photocraft_algo::transform::Interp::Bicubic)?;
            crate::transform_cmds::refresh_text(&st.doc, &mut layer)?;
        }
        let above = isolation_anchor(&st.doc, &scope, above);
        return packages::isolation::insert_prepared(session, &scope, capture, above, layer);
    }
    session.edit(label, move |doc, active| {
        doc.insert_above(above, layer);
        *active = Some(id);
        Ok(id)
    })
}

fn stable_insertion_container(session: &crate::Session, prepared: &mut packages::clip_compound::Prepared, target: Option<photocraft_doc::LayerId>) {
    let (Some(policy), Some(source), Some(target)) = (&session.vector_insertion, session.active(), target) else {
        return;
    };
    let created = prepared.document.walk().into_iter().find_map(|(_, _, layer)| {
        (source.doc.layer(layer.id).is_none() && layer.children().is_some_and(|children| children.iter().any(|child| child.id == target))).then_some(layer.id)
    });
    if let Some(id) = created
        && let Some(layer) = prepared.document.layer_mut(id)
    {
        layer.id = policy.container;
        for selected in &mut prepared.selection {
            if *selected == id {
                *selected = policy.container;
            }
        }
    }
}

fn isolation_anchor(doc: &Document, scope: &packages::isolation::State, above: Option<photocraft_doc::LayerId>) -> Option<photocraft_doc::LayerId> {
    let packages::isolation::Target::Group { layer: parent } = scope.current()? else {
        return None;
    };
    let parent_path = doc.path_of(parent)?;
    let anchor_path = doc.path_of(above?)?;
    if !anchor_path.starts_with(&parent_path) || anchor_path.len() <= parent_path.len() {
        return None;
    }
    doc.layer_at(&anchor_path[..parent_path.len() + 1]).map(|l| l.id)
}
/// The detached drawing preview follows the same captured destination as release.
pub fn insertion_preview(session: &crate::Session, preview: &Document) -> Result<Document> {
    let Some(policy) = &session.vector_insertion else {
        return Ok(preview.clone());
    };
    let source = &session.active().ok_or(EngineError::NoDocument)?.doc;
    let scope = packages::isolation::authoritative(session).unwrap_or_default();
    policy.capture.check(session, &scope)?;
    if policy.scope != scope {
        return Err(error("Insertion scope changed; start again"));
    }
    let created = preview.layers.iter().filter(|layer| source.layer(layer.id).is_none()).collect::<Vec<_>>();
    if created.is_empty() || policy.mode == packages::clip_compound::DrawMode::Normal && scope.current().is_none() {
        return Ok(preview.clone());
    }
    if created.len() != 1 {
        return Err(error("Drawing preview requires one new object"));
    }
    let mut layer = created[0].clone();
    if policy.mode != packages::clip_compound::DrawMode::Normal {
        let capture = packages::clip_compound::Capture::begin(session, &policy.target.into_iter().collect::<Vec<_>>())?;
        let mut prepared = packages::clip_compound::prepare_insert(session, &capture, policy.mode, policy.target, layer)?;
        stable_insertion_container(session, &mut prepared, policy.target);
        return Ok(prepared.document);
    }
    let target = scope.insertion_target(source)?;
    if target.from_document != photocraft_geom::Affine::IDENTITY {
        let a = target.from_document;
        let [a0, b, c, d, e, f] = a.m;
        let h = photocraft_algo::transform::Homography([a0, c, e, b, d, f, 0., 0., 1.]);
        crate::transform_cmds::transform_layer(None, Default::default(), &mut layer, &h, Some(a), photocraft_algo::transform::Interp::Bicubic)?;
        crate::transform_cmds::refresh_text(source, &mut layer)?;
    }
    let mut document = preview.clone();
    document.layers.retain(|l| l.id != layer.id);
    match scope.current() {
        Some(packages::isolation::Target::Group { layer: parent }) => {
            let anchor = isolation_anchor(source, &scope, policy.target);
            let children = document.layer_mut(parent).and_then(photocraft_doc::Layer::children_mut).ok_or(EngineError::NoLayer(parent))?;
            let index = anchor.and_then(|id| children.iter().position(|l| l.id == id)).map_or(children.len(), |i| i + 1);
            children.insert(index, layer);
        }
        Some(packages::isolation::Target::Compound { layer: parent }) => {
            let capture = packages::clip_compound::Capture::begin(session, &[parent])?;
            let mut prepared = packages::clip_compound::prepare_insert(session, &capture, packages::clip_compound::DrawMode::Inside, Some(parent), layer)?;
            stable_insertion_container(session, &mut prepared, Some(parent));
            return Ok(prepared.document);
        }
        Some(packages::isolation::Target::Symbol { master, .. }) => {
            document.symbols.iter_mut().find(|s| s.id == master).ok_or_else(|| error("Missing isolated symbol"))?.layers.push(layer);
        }
        None => {}
    }
    Ok(document)
}

#[cfg(test)]
mod integration_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use photocraft_doc::{Color, Fill, Layer, Size, Subpath};
    fn object(name: &str) -> Layer {
        Layer::new(
            name,
            LayerContent::Shape(ShapeLayer {
                path: photocraft_doc::Path::new(vec![Subpath::polygon(&[(1., 1.), (8., 1.), (8., 8.), (1., 8.)])]),
                fill: Some(Fill::Solid(Color::BLACK)),
                ..Default::default()
            }),
        )
    }
    fn session() -> crate::Session {
        let mut s = crate::Session::new();
        let mut doc = Document::new("Insertion", Size::new(32, 32), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
        doc.layers.push(object("Target"));
        s.add_document(doc, None);
        s
    }
    #[test]
    fn integration_projected_live_groups_reserve_global_ids_and_are_stable() {
        use photocraft_doc::vector_design::live::{BlendSpacing, BlendSpec, LiveGroup, RepeatSpec};
        let mut doc = Document::new("Projection IDs", Size::new(32, 32), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
        for i in 0..2 {
            let mut blend = Layer::group("Blend", vec![object("A"), object("B")]);
            blend.vector.live = Some(LiveGroup::Blend(BlendSpec { spacing: BlendSpacing::Steps(2), ..Default::default() }));
            doc.layers.push(blend);
            let mut repeat = Layer::group("Repeat", vec![object("Source")]);
            repeat.vector.live = Some(LiveGroup::Repeat(RepeatSpec::Grid { rows: 1, cols: 2, spacing: [10., 0.] }));
            doc.layers.push(repeat);
            doc.layers.push(Layer::raster(format!("Unrelated {i}"), doc.pixel_format()));
        }
        let before = doc.clone();
        let projected = project_document(&doc).unwrap();
        assert_eq!(doc, before);
        let ids: Vec<_> = projected.walk().into_iter().map(|(_, _, l)| l.id).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
        assert!(ids.len() > doc.walk().len());
        let again = project_document(&doc).unwrap();
        assert_eq!(ids, again.walk().into_iter().map(|(_, _, l)| l.id).collect::<Vec<_>>());
        for layer in &doc.layers {
            assert_eq!(projected.layer(layer.id).unwrap().name, layer.name);
        }
    }
    #[test]
    fn integration_shared_insertion_draw_modes_preview_publication_and_undo() {
        for mode in [packages::clip_compound::DrawMode::Behind, packages::clip_compound::DrawMode::Inside] {
            let mut s = session();
            let before = s.active().unwrap().doc.clone();
            let target = before.layers[0].id;
            s.active_mut().unwrap().active_layer = Some(target);
            capture_insertion(&mut s, mode).unwrap();
            let layer = object("Created");
            let id = layer.id;
            let mut draft = (*before).clone();
            draft.layers.push(layer.clone());
            let preview = insertion_preview(&s, &draft).unwrap();
            assert_eq!(s.active().unwrap().doc, before);
            insert_layer(&mut s, Some(target), layer, "Draw").unwrap();
            assert_eq!(*s.active().unwrap().doc, preview);
            assert!(s.active().unwrap().doc.layer(id).is_some());
            assert!(s.undo());
            assert_eq!(s.active().unwrap().doc, before);
        }
    }
    #[test]
    fn integration_shared_insertion_isolation_uses_parent_and_rejects_stale_selection() {
        let mut s = session();
        let child = s.active().unwrap().doc.layers[0].clone();
        let group = Layer::group("Isolated", vec![child]);
        let parent = group.id;
        std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).layers = vec![group];
        let mut scope = packages::isolation::State::default();
        scope.synchronize(&s, true);
        let capture = packages::isolation::Capture::new(&s, &scope).unwrap();
        scope.enter(&mut s, capture, parent).unwrap();
        packages::isolation::publish_view_state(&mut s, &scope);
        capture_insertion(&mut s, packages::clip_compound::DrawMode::Normal).unwrap();
        let mut layer = object("Child");
        if let LayerContent::Shape(shape) = &mut layer.content {
            crate::vector_cmds::refresh_shape(&s.active().unwrap().doc, shape);
        }
        let id = layer.id;
        insert_layer(&mut s, None, layer, "Draw").unwrap();
        assert_eq!(s.active().unwrap().doc.layers.len(), 1);
        assert!(s.active().unwrap().doc.layer(parent).unwrap().children().unwrap().iter().any(|l| l.id == id));
        capture_insertion(&mut s, packages::clip_compound::DrawMode::Normal).unwrap();
        s.active_mut().unwrap().active_layer = None;
        let before = s.active().unwrap().doc.clone();
        assert!(insert_layer(&mut s, None, object("Stale"), "Draw").is_err());
        assert_eq!(s.active().unwrap().doc, before);
    }
}
