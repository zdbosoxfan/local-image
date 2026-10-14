//! Package 28: captured trace requests and fully prepared, atomic output.
//! T1 tracing stays in pc-trace. No preview changes document/history/journal state.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use lightcraft_sysmem::JobCtl;
pub use pc_trace::cutout::Layering;
pub use pc_trace::{Mode, Params, Preset, Trace};
use photocraft_color::Color;
use photocraft_doc::{Document, Fill, Layer, LayerContent, LayerId, NamedPath, Path, ShapeLayer};
use photocraft_geom::{Affine, Rect};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub layer: LayerId,
    pub active: Option<LayerId>,
    pub targets: Vec<LayerId>,
}
/// Removing a paint removes its output paths; merging replaces its paint, preserving
/// stacking order. These edits are reapplied to both preview and full-resolution output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteEdit {
    pub from: [u8; 3],
    pub to: Option<[u8; 3]>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Output {
    pub expand: bool,
    pub work_path: bool,
    pub palette_edits: Vec<PaletteEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<pc_trace::cleanup::Options>,
}
impl Output {
    pub fn validate(&self, params: &Params) -> Result<()> {
        if let Some(cleanup) = &self.cleanup {
            cleanup.validate().map_err(error)?;
        }
        if self.work_path && params.preset != Preset::BlackWhite {
            return Err(error("Make Work Path requires Black & White"));
        }
        if self.palette_edits.len() > 64 {
            return Err(error("palette edits exceed the 64 colour limit"));
        }
        if self.work_path && !self.palette_edits.is_empty() {
            return Err(error("Make Work Path cannot edit the palette"));
        }
        Ok(())
    }
}
pub fn error(message: impl ToString) -> EngineError {
    EngineError::Other(message.to_string())
}
pub fn capture(session: &Session, layer: Option<LayerId>) -> Result<Capture> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let request = Capture {
        document: s.doc.id.0,
        revision: s.revision,
        layer: layer.or(s.active_layer).ok_or_else(|| error("select a pixel layer or smart object"))?,
        active: s.active_layer,
        targets: s.selected_layers.clone(),
    };
    check(session, &request)?;
    Ok(request)
}
pub fn check(session: &Session, request: &Capture) -> Result<()> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    if s.doc.id.0 != request.document || s.revision != request.revision {
        return Err(error("Trace source changed; reopen Vectorize"));
    }
    if s.active_layer != request.active || s.selected_layers != request.targets {
        return Err(error("Trace targets changed; reopen Vectorize"));
    }
    let mut seen = std::collections::HashSet::new();
    for &id in &request.targets {
        if !seen.insert(id) {
            return Err(error("duplicate trace target"));
        }
        s.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    }
    let layer = s.doc.layer(request.layer).ok_or(EngineError::NoLayer(request.layer))?;
    if !matches!(layer.content, LayerContent::Raster(_) | LayerContent::Smart(_)) {
        return Err(error("Vectorize requires a pixel layer or smart object"));
    }
    // Reading a locked background is supported. Inserting into a locked ancestor is not.
    let mut path = s.doc.path_of(request.layer).ok_or(EngineError::NoLayer(request.layer))?;
    path.pop();
    for n in 1..=path.len() {
        if s.doc.layer_at(&path[..n]).is_some_and(|l| s.doc.effective_locks(l.id).all) {
            return Err(error("Trace destination group is locked"));
        }
    }
    source_bounds(&s.doc)?;
    Ok(())
}
pub fn source_bounds(doc: &Document) -> Result<Rect> {
    let bounds = doc
        .selection
        .as_ref()
        .map_or(doc.bounds(), |s| if s.default_pixel().first().is_some_and(|v| *v > 0.) { doc.bounds() } else { s.content_bounds().intersect(&doc.bounds()) });
    if bounds.is_empty() {
        return Err(error("selection is empty"));
    }
    if u64::from(bounds.width()) * u64::from(bounds.height()) > pc_trace::PIXEL_BUDGET as u64 {
        return Err(error(format!("image exceeds the {} pixel tracing budget", pc_trace::PIXEL_BUDGET)));
    }
    Ok(bounds)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preview {
    pub capture: Capture,
    pub trace: Trace,
    pub scale: f64,
    /// Exact x/y scales matter for rounded, odd-sized previews.
    pub scale_xy: [f64; 2],
    pub bounds: [i32; 4],
    pub source_rgba: Vec<u8>,
    pub result_rgba: Vec<u8>,
    pub elapsed_ms: f64,
    #[serde(default)]
    pub cutout: bool,
    #[serde(default)]
    pub cleanup_stats: Option<pc_trace::cleanup::Stats>,
}
// A typed mailbox keeps preview bitmaps out of JobInfo's JSON history. Session::start
// still owns cancellation, latest keys, worker lifetime and completion events.
type PreviewSender = std::sync::mpsc::Sender<Preview>;
static MAILBOXES: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<u64, PreviewSender>>> = std::sync::LazyLock::new(Default::default);
static NEXT_MAILBOX: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
pub(crate) fn take_mailbox(id: u64) -> Option<PreviewSender> {
    MAILBOXES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&id)
}
/// Start the ordinary non-journaled T1 preview command with typed pixel delivery.
/// The caller drops the receiver when superseding/cancelling a request.
pub fn start_preview(
    session: &mut Session,
    capture: &Capture,
    params: &Params,
    output: &Output,
    scale: f64,
) -> Result<(crate::jobs::Started, std::sync::mpsc::Receiver<Preview>)> {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = NEXT_MAILBOX.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    MAILBOXES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(id, tx);
    let result = session.start(
        "layer.vectorize.preview",
        json!({"layer":capture.layer.0,"capture":capture,"params":params,"output":output,"scale":scale,"preview_mailbox":id}),
    );
    // Validation can fail before the command consumes its mailbox. Never leak a sender.
    let _ = take_mailbox(id);
    result.map(|started| (started, rx))
}
/// Pixel extraction and smart rendering happen on the job worker, against the captured doc.
pub fn trace(doc: &Document, request: &Capture, params: &Params, output: &Output, scale: f64, ctx: &JobCtl) -> Result<Preview> {
    trace_impl(doc, request, params, output, scale, ctx, true)
}
pub(crate) fn trace_geometry(doc: &Document, request: &Capture, params: &Params, output: &Output, scale: f64, ctx: &JobCtl) -> Result<Preview> {
    trace_impl(doc, request, params, output, scale, ctx, false)
}
fn trace_impl(doc: &Document, request: &Capture, params: &Params, output: &Output, scale: f64, ctx: &JobCtl, render_pixels: bool) -> Result<Preview> {
    params.validate().map_err(error)?;
    output.validate(params)?;
    if !scale.is_finite() || !(0.01..=1.0).contains(&scale) {
        return Err(error("preview scale must be finite and 0.01..1"));
    }
    // speckle has no bound in T1; the UI/output boundary bounds it before work.
    if params.speckle > pc_trace::PIXEL_BUDGET {
        return Err(error("noise exceeds the pixel budget"));
    }
    let started = std::time::Instant::now();
    let bounds = source_bounds(doc)?;
    let (width, height) = (bounds.width(), bounds.height());
    ctx.check()?;
    ctx.report(0.05, "Reading image");
    let layer = doc.layer(request.layer).ok_or(EngineError::NoLayer(request.layer))?;
    let rendered = if let LayerContent::Smart(sm) = &layer.content { crate::smart_cmds::render(doc, sm)? } else { None };
    let surface = rendered.as_ref().or_else(|| layer.surface()).ok_or_else(|| error("smart object has no renderable pixels"))?;
    let fmt = surface.format();
    let pixels = surface.read_region(bounds);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    let mut source_rgba = render_pixels.then(|| Vec::with_capacity(width as usize * height as usize * 4));
    for (i, pixel) in pixels.chunks_exact(fmt.channels()).enumerate() {
        if i % width as usize == 0 {
            ctx.check()?;
        }
        let mut c = [0.; 4];
        c[..fmt.mode.color_channels()].copy_from_slice(&pixel[..fmt.mode.color_channels()]);
        let alpha = if fmt.alpha { pixel[fmt.mode.color_channels()] } else { 1. };
        let mut color = Color { mode: fmt.mode, c, alpha }.to_rgba8();
        let selection_alpha = if let Some(selection) = &doc.selection {
            let x = bounds.x0 + (i % width as usize) as i32;
            let y = bounds.y0 + (i / width as usize) as i32;
            selection.sample_channel(x, y, 0).clamp(0., 1.)
        } else {
            1.
        };
        if let Some(source) = &mut source_rgba {
            let mut before = color;
            before[3] = (f32::from(before[3]) * selection_alpha).round() as u8;
            source.extend(before);
        }
        // Ignoring source alpha must not remove the active selection's clip.
        if doc.selection.is_some() && !params.use_alpha {
            color[3] = 255;
        }
        color[3] = (f32::from(color[3]) * selection_alpha).round() as u8;
        rgba.extend(color);
    }
    let (tw, th) = ((f64::from(width) * scale).round().max(1.) as u32, (f64::from(height) * scale).round().max(1.) as u32);
    if (tw, th) != (width, height) {
        rgba = resize_rgba(rgba, width, height, tw, th)?;
        if let Some(source) = source_rgba.take() {
            source_rgba = Some(resize_rgba(source, width, height, tw, th)?);
        }
    }
    ctx.check()?;
    ctx.report(0.15, "Tracing regions");
    let mut trace_params = params.clone();
    if doc.selection.is_some() {
        trace_params.use_alpha = true;
    }
    let image = pc_trace::Image { width: tw, height: th, rgba: &rgba };
    let mut cleanup_stats = None;
    let mut trace = if let Some(options) = &output.cleanup {
        let cleaned = pc_trace::cleanup::trace_with_cancel(image, &trace_params, options, || ctx.is_cancelled()).map_err(error)?;
        cleanup_stats = Some(cleaned.stats);
        cleaned.trace
    } else {
        pc_trace::trace_with_cancel(image, &trace_params, || ctx.is_cancelled()).map_err(error)?
    };
    for edit in &output.palette_edits {
        trace.layers.retain_mut(|l| {
            if l.color[..3] != edit.from {
                return true;
            }
            if let Some(to) = edit.to {
                l.color[..3].copy_from_slice(&to);
                true
            } else {
                false
            }
        });
    }
    for edit in &output.palette_edits {
        trace.strokes.retain_mut(|stroke| {
            let photocraft_doc::Fill::Solid(color) = &mut stroke.stroke.paint else {
                return true;
            };
            let rgba = color.to_rgba8();
            if rgba[..3] != edit.from {
                return true;
            }
            if let Some(to) = edit.to {
                *color = photocraft_doc::Color::rgba(f32::from(to[0]) / 255., f32::from(to[1]) / 255., f32::from(to[2]) / 255., f32::from(rgba[3]) / 255.);
                true
            } else {
                false
            }
        });
    }
    merge_equal_paints(&mut trace, ctx)?;
    trace.nodes = trace.layers.iter().flat_map(|l| &l.path.subpaths).chain(trace.strokes.iter().flat_map(|l| &l.path.subpaths)).map(|s| s.knots.len()).sum();
    let result_rgba = if render_pixels {
        let preview_bounds = Rect::from_xywh(0, 0, tw, th);
        let mut preview_doc =
            Document::new("Trace preview", photocraft_doc::Size::new(tw, th), photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::F32);
        // Use the production rasterizer/compositor, with cancellation between paints.
        for colorpath in &trace.layers {
            ctx.check()?;
            let mut sh = shape(&colorpath.path, colorpath.color);
            sh.cache = Some(photocraft_vector::render_shape(&sh, photocraft_color::PixelFormat::RGBA32F, preview_bounds));
            preview_doc.layers.push(Layer::new("Trace preview", LayerContent::Shape(sh)));
        }
        preview_doc.layers.extend(stroke_layers(&preview_doc, &trace, Affine::IDENTITY, ctx)?);
        if params.layering == Layering::Cutout && trace.strokes.is_empty() {
            let mut group = Layer::group("Trace partition", std::mem::take(&mut preview_doc.layers));
            if !matches!(params.mode, Some(Mode::Centerline)) && group.children().is_some_and(|children| !children.is_empty()) {
                photocraft_doc::vector_design::mosaic::mark(&mut group).map_err(error)?;
            }
            preview_doc.layers.push(group);
        }
        photocraft_compose::render(&preview_doc, preview_bounds).to_rgba8().pixels
    } else {
        Vec::new()
    };
    ctx.check()?;
    let cutout = params.layering == Layering::Cutout && trace.strokes.is_empty();
    Ok(Preview {
        capture: request.clone(),
        trace,
        scale,
        scale_xy: [f64::from(tw) / f64::from(width), f64::from(th) / f64::from(height)],
        bounds: [bounds.x0, bounds.y0, width as i32, height as i32],
        source_rgba: source_rgba.unwrap_or_default(),
        result_rgba,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
        cutout,
        cleanup_stats,
    })
}
fn resize_rgba(rgba: Vec<u8>, width: u32, height: u32, tw: u32, th: u32) -> Result<Vec<u8>> {
    let img = image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| error("invalid pixel buffer"))?;
    Ok(image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle).into_raw())
}
/// Merge only when palette edits created repeated paints. Pathfinder computes visible
/// geometry in stack order before grouping, so moving a paint never exposes hidden pixels.
fn merge_equal_paints(trace: &mut Trace, ctx: &JobCtl) -> Result<()> {
    let mut colors = Vec::new();
    for layer in &trace.layers {
        if !colors.contains(&layer.color) {
            colors.push(layer.color);
        }
    }
    if colors.len() == trace.layers.len() {
        return Ok(());
    }
    ctx.check()?;
    let shapes: Vec<_> = trace.layers.iter().map(|l| photocraft_pathops::Shape::new(l.path.clone(), u64::from(u32::from_be_bytes(l.color)))).collect();
    let merged = photocraft_pathops::pathfinder(photocraft_pathops::PathfinderOp::Merge, &shapes).map_err(error)?;
    ctx.check()?;
    trace.layers = colors
        .into_iter()
        .filter_map(|color| {
            let key = u64::from(u32::from_be_bytes(color));
            let mut path = Path::default();
            for shape in merged.iter().filter(|s| s.key == key) {
                path.subpaths.extend(shape.path.subpaths.clone());
            }
            (!path.is_empty()).then_some(pc_trace::ColorPath { color, path })
        })
        .collect();
    Ok(())
}
fn shape(path: &Path, color: [u8; 4]) -> ShapeLayer {
    ShapeLayer {
        path: path.clone(),
        fill: Some(Fill::Solid(Color::rgba(f32::from(color[0]) / 255., f32::from(color[1]) / 255., f32::from(color[2]) / 255., f32::from(color[3]) / 255.))),
        ..Default::default()
    }
}
/// Native caches and connectivity are prepared on the worker, before Session::edit.
pub struct Prepared {
    children: Vec<Layer>,
    work_path: Option<Path>,
    colors: usize,
    nodes: usize,
    cutout: bool,
}
pub fn prepare(doc: &Document, preview: &Preview, output: &Output, ctx: &JobCtl) -> Result<Prepared> {
    if preview.scale_xy.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v) || *v == 0.)
        || preview.bounds[2..].iter().any(|v| *v <= 0)
        || preview.trace.width == 0
        || preview.trace.height == 0
    {
        return Err(error("invalid trace preview dimensions"));
    }
    if preview.trace.layers.len() > 64 || preview.trace.nodes > 8_000_000 {
        return Err(error("trace output exceeds geometry budget"));
    }
    let mut children = Vec::new();
    let mut work = Path::default();
    let [x, y, _, _] = preview.bounds;
    let affine = Affine::translate(f64::from(x), f64::from(y)).mul(&Affine { m: [1. / preview.scale_xy[0], 0., 0., 1. / preview.scale_xy[1], 0., 0.] });
    for colorpath in &preview.trace.layers {
        ctx.check()?;
        let path = colorpath.path.transform(&affine);
        crate::vector_api::validate_path(&path)?;
        if path.is_empty() {
            continue;
        }
        if path.control_bounds().is_some_and(|b| [b.0, b.1, b.2, b.3].iter().any(|v| v.abs() > 1_000_000.)) {
            return Err(error("trace output exceeds coordinate bounds"));
        }
        if output.work_path {
            work.subpaths.extend(path.subpaths);
            continue;
        }
        let paths = if output.expand {
            photocraft_pathops::regions(&[photocraft_pathops::Shape::new(path, 0)]).map_err(error)?.into_iter().map(|r| r.path).collect()
        } else {
            vec![path]
        };
        let [r, g, b, _] = colorpath.color;
        for (index, path) in paths.into_iter().enumerate() {
            ctx.check()?;
            let mut sh = shape(&path, colorpath.color);
            crate::vector_cmds::refresh_shape(doc, &mut sh);
            let name = if output.expand { format!("#{r:02X}{g:02X}{b:02X} – {}", index + 1) } else { format!("#{r:02X}{g:02X}{b:02X}") };
            children.push(Layer::new(name, LayerContent::Shape(sh)));
        }
    }
    if output.work_path && !preview.trace.strokes.is_empty() {
        return Err(error("Centreline strokes cannot be published as a fill Work Path"));
    }
    children.extend(stroke_layers(doc, &preview.trace, affine, ctx)?);
    ctx.check()?;
    Ok(Prepared {
        children,
        work_path: output.work_path.then_some(work),
        colors: preview.trace.layers.len(),
        nodes: preview.trace.nodes,
        cutout: preview.cutout,
    })
}
pub fn publish(session: &mut Session, request: &Capture, prepared: Prepared) -> Result<Value> {
    check(session, request)?;
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let name = s.doc.layer(request.layer).ok_or(EngineError::NoLayer(request.layer))?.name.clone();
    if prepared.children.is_empty() && prepared.work_path.as_ref().is_none_or(Path::is_empty) {
        return Err(error("trace is empty; adjust the controls"));
    }
    let (colors, nodes, count) = (prepared.colors, prepared.nodes, prepared.children.len());
    let source = request.layer;
    session.edit("Vectorize", move |doc, active| {
        if let Some(path) = prepared.work_path {
            let base = format!("Vectorized – {name}");
            let mut name = base.clone();
            let mut suffix = 2;
            while doc.paths.iter().any(|p| p.name == name) {
                name = format!("{base} {suffix}");
                suffix += 1;
            }
            doc.paths.push(NamedPath { name: name.clone(), path: path.clone(), psd_raw: None });
            doc.work_path = Some(path);
            return Ok(json!({"path":name,"colors":colors,"nodes":nodes}));
        }
        let mut group = Layer::group(format!("Vectorized – {name}"), prepared.children);
        if prepared.cutout {
            photocraft_doc::vector_design::mosaic::mark(&mut group).map_err(error)?;
        }
        let id = doc.insert_above(Some(source), group);
        if doc.max_group_depth() > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(error("trace exceeds document group depth"));
        }
        *active = Some(id);
        Ok(json!({"group":id.0,"colors":colors,"nodes":nodes,"shapes":count}))
    })
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    Vec::new()
}

#[cfg(test)]
#[path = "trace_output/tests.rs"]
mod tests;

/// The shared dialog renders and publishes editable centreline strokes with the width provider.
fn stroke_layers(doc: &Document, trace: &Trace, affine: Affine, ctx: &JobCtl) -> Result<Vec<Layer>> {
    use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, StrokeItem};
    if trace.strokes.len() > 8192 {
        return Err(error("Centreline stroke budget exceeded"));
    }
    let factor = (affine.m[0].hypot(affine.m[1]) + affine.m[2].hypot(affine.m[3])) / 2.;
    let mut out = Vec::new();
    let mut bytes = 0u128;
    for source in &trace.strokes {
        ctx.check()?;
        let path = source.path.transform(&affine);
        crate::vector_api::validate_path(&path)?;
        let mut base = source.stroke.clone();
        base.width *= factor as f32;
        let mut profile = source.profile.clone();
        if let Some(profile) = &mut profile {
            for p in &mut profile.points {
                p.left *= factor;
                p.right *= factor;
            }
        }
        let appearance = Appearance { items: vec![AppearanceItem::Stroke(StrokeItem { base: base.clone(), profile, ..Default::default() })] };
        let mut shape = photocraft_doc::ShapeLayer { path, fill: None, stroke: Some(base), ..Default::default() };
        let plan = photocraft_vector::appearance::CompiledAppearance::with_projection(
            &shape,
            Some(&appearance),
            &doc.patterns,
            doc.bounds(),
            &super::styles::Projection,
        )
        .map_err(error)?;
        bytes += plan.cache_bytes(doc.pixel_format());
        if bytes > 512 << 20 {
            return Err(error("Centreline cache budget exceeded"));
        }
        shape.cache = Some(plan.render(doc.pixel_format(), doc.bounds()).map_err(error)?);
        let mut layer = Layer::new("Centreline", LayerContent::Shape(shape));
        layer.vector.appearance = Some(appearance);
        out.push(layer);
    }
    Ok(out)
}
