//! local-image: Local Image's AI commands, run on a local ComfyUI through `li-ai`.
//!
//! Every command snapshots the document, does the AI work on a background job (progress and
//! cancel go through [`JobCtx`]) and applies the result as one history step:
//!
//! | Command | Result |
//! |---|---|
//! | `ai.remove` | the stroke (`points`) or the selection repaired, as a new layer above the active one |
//! | `ai.removeBackground` | the Qwen matte as a mask (default), transparency, a background plate or a new cutout layer |
//! | `ai.selectSubject` | the subject as the selection |
//! | `ai.generativeFill` | the selection regenerated from a prompt, as a new layer |
//! | `ai.generateBackground` | an empty background plate as a new layer below the active one |
//! | `ai.enhance` | the flattened image enhanced and enlarged (Faithful / Rescue / Generative) as a new document |
//!
//! This file is Local Image's addition to the vendored PhotoCraft engine (new file: never
//! conflicts on upstream merges). Hook: `lib.rs` (`pub mod ai_cmds`) and `commands.rs`
//! (`v.extend(crate::ai_cmds::specs())`).

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use image::{GrayImage, Luma, RgbImage, RgbaImage};
use li_ai::{JobControl, RemoveEngine, Stage};
use li_ai::{RgbFrame, RgbaFrame};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, LayerContent, LayerId};
use photocraft_geom::{Rect, Size};
use photocraft_paint::{BrushSettings, Stroke, StrokePoint};
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::jobs::JobCtx;
use crate::{EngineError, Result, Session};

fn bad(cmd: &str, msg: &str) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

fn has_pixel_layer(s: &Session) -> std::result::Result<(), String> {
    let l = crate::active_layer_of(s)?;
    if matches!(l.content, LayerContent::Raster(_)) { Ok(()) } else { Err(format!("the layer is a {} layer, not a pixel layer", l.content.kind_name())) }
}

fn has_selection(s: &Session) -> std::result::Result<(), String> {
    let d = s.active().ok_or("no document open")?;
    if d.doc.selection.as_ref().is_some_and(|sel| !sel.content_bounds().is_empty()) { Ok(()) } else { Err("make a selection first".into()) }
}

/// `engine` param: `klein` (default), `qwen-int8`, `qwen-bf16`.
fn remove_engine(p: &Value) -> RemoveEngine {
    match p.get("engine").and_then(Value::as_str).unwrap_or("auto") {
        "auto" => RemoveEngine::Auto { models_dir: crate::seg::models_dir() },
        "lama" => RemoveEngine::Lama { models_dir: crate::seg::models_dir() },
        "qwen-bf16" => RemoveEngine::Qwen { variant: "bf16".into() },
        "qwen-int8" | "qwen" => RemoveEngine::Qwen { variant: "int8".into() },
        _ => RemoveEngine::Klein,
    }
}

/// The Qwen precision for cutouts and fills: `int8` (default) or `bf16`.
fn qwen_variant(p: &Value) -> String {
    match p.get("engine").or_else(|| p.get("variant")).and_then(Value::as_str).unwrap_or("int8") {
        "bf16" | "qwen-bf16" => "bf16".into(),
        _ => "int8".into(),
    }
}

fn seed(p: &Value) -> u64 {
    p.get("seed").and_then(Value::as_u64).unwrap_or_else(li_ai::ops::new_seed)
}

/// Runs `f` with a `li-ai` job control mirrored onto `ctx`: progress out, cancellation in.
pub fn bridged<T>(ctx: &JobCtx, f: impl FnOnce(&JobControl) -> anyhow::Result<T>) -> Result<T> {
    let ctl = JobControl::new();
    // Drop runs before the scope joins, including when the callback unwinds.
    struct StopWatcher(mpsc::Sender<()>);
    impl Drop for StopWatcher {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let r = std::thread::scope(|scope| {
        let (stop, stopped) = mpsc::channel();
        let c = &ctl;
        scope.spawn(move || {
            loop {
                if ctx.cancelled() {
                    c.cancel();
                }
                let p = c.progress();
                let frac = match p.stage {
                    Stage::Preparing => 0.02,
                    Stage::Uploading => 0.05,
                    Stage::Queued => 0.08,
                    Stage::Loading => 0.12,
                    Stage::Conditioning => 0.2,
                    Stage::Sampling => 0.22 + 0.68 * p.fraction().unwrap_or(0.0),
                    Stage::Decoding => 0.92,
                    Stage::Saving | Stage::Running => 0.94,
                    Stage::Finishing | Stage::Done => 0.97,
                    Stage::Failed | Stage::Cancelled => 0.0,
                };
                let mut msg = p.stage.label().to_owned();
                if let Some((v, m)) = p.step {
                    msg = format!("{msg} {v}/{m}");
                }
                if !p.message.is_empty() {
                    msg = format!("{} · {msg}", p.message);
                }
                ctx.progress(frac, &msg);
                if stopped.recv_timeout(Duration::from_millis(100)) != Err(mpsc::RecvTimeoutError::Timeout) {
                    break;
                }
            }
        });
        let _stop = StopWatcher(stop);
        f(&ctl)
    });
    r.map_err(|e| if e.is::<li_ai::Cancelled>() { EngineError::Cancelled } else { EngineError::Other(format!("{e:#}")) })
}

/// The visible composite of `rect` as 8-bit RGB (transparency over white) — what the user sees.
fn composite_rgb(doc: &Document, rect: Rect) -> Result<RgbImage> {
    Ok(composite_frame(doc, rect)?.to_rgb8())
}

/// [`composite_rgb`] at full precision (encoded sRGB over white, normalized): what AI engines
/// receive, so 16-bit and float documents are quantized only at a ComfyUI wire.
fn composite_frame(doc: &Document, rect: Rect) -> Result<RgbFrame> {
    let buf = crate::color_cmds::composite_to_srgb(doc, photocraft_compose::render(doc, rect))?;
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    Ok(RgbFrame::from_fn(w, h, |x, y| {
        let p = buf.px[y * w + x];
        let a = p[3].clamp(0.0, 1.0);
        [0, 1, 2].map(|k| p[k].clamp(0.0, 1.0) * a + (1.0 - a))
    }))
}

/// Read a file through the bounded, profile-preserving importer, then convert for an AI model.
pub fn file_rgba_srgb(path: &std::path::Path) -> Result<RgbaImage> {
    use std::io::Read;
    let limit = photocraft_format::read::MAX_FILE_BYTES;
    let file = std::fs::File::open(path).map_err(|e| EngineError::Other(format!("Could not read {}: {e}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(|e| EngineError::Other(format!("Could not read {}: {e}", path.display())))?;
    if bytes.len() as u64 > limit {
        return Err(EngineError::Other("Image exceeds the file memory limit.".into()));
    }
    let imported = photocraft_io::import(&path.to_string_lossy(), &bytes).map_err(|e| EngineError::Other(e.to_string()))?;
    flatten_rgba(&imported.document)
}

/// The flattened document as sRGB8 RGBA (for AI references and edits).
pub fn flatten_rgba(doc: &Document) -> Result<RgbaImage> {
    rendered_rgba(doc, photocraft_compose::render(doc, doc.bounds()))
}

/// Convert a document-space render before quantizing it for a model or reference.
pub fn rendered_rgba(doc: &Document, buf: photocraft_compose::Buffer) -> Result<RgbaImage> {
    let buf = crate::color_cmds::composite_to_srgb(doc, buf)?;
    let (w, h) = (buf.rect.width(), buf.rect.height());
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Ok(RgbaImage::from_fn(w, h, |x, y| {
        let p = buf.px[(y * w + x) as usize];
        image::Rgba([q(p[0]), q(p[1]), q(p[2]), q(p[3])])
    }))
}

/// local-image: the selection as an 8-bit mask the size of the document (white = selected), or
/// `None` without a selection.
pub fn selection_gray(doc: &Document) -> Option<GrayImage> {
    let sel = doc.selection.as_ref()?;
    let bounds = doc.bounds();
    let r = sel.content_bounds().intersect(&bounds);
    if r.is_empty() {
        return None;
    }
    let vals = sel.read_region(r);
    let mut m = GrayImage::new(bounds.width(), bounds.height());
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let v = vals[((y - r.y0) as u32 * r.width() + (x - r.x0) as u32) as usize];
            m.put_pixel(x as u32, y as u32, Luma([(v.clamp(0.0, 1.0) * 255.0).round() as u8]));
        }
    }
    Some(m)
}

/// A new raster layer holding `img`, in the document's format, at `(x, y)`.
pub fn layer_from_rgba(doc: &Document, name: &str, img: &RgbaImage, x: i32, y: i32) -> Result<Layer> {
    layer_from_frame(doc, name, &RgbaFrame::from_rgba8(img), x, y)
}

/// [`layer_from_rgba`] for a float sRGB frame (straight alpha), without an 8-bit step.
pub fn layer_from_frame(doc: &Document, name: &str, img: &RgbaFrame, x: i32, y: i32) -> Result<Layer> {
    let rect = Rect::from_xywh(x, y, img.width as u32, img.height as u32);
    let surf = surface_from(doc, rect, &img.rgb(), |px, py| img.get(px as usize, py as usize)[3])?;
    let mut l = Layer::raster(name, surf.format());
    if let LayerContent::Raster(s) = &mut l.content {
        *s = surf;
    }
    Ok(l)
}

/// One layer rendered alone over `rect` as RGBA8 (with its own transparency).
fn layer_rgba(doc: &Document, layer: &Layer, rect: Rect) -> Result<RgbaImage> {
    let mut l = layer.clone();
    l.mask = None;
    l.visible = true;
    l.opacity = 1.0;
    selected_layer_rgba(doc, &l, rect)
}

/// Render one layer while retaining its document's profiles, patterns and lighting.
/// Visibility, opacity and masks are respected; matte callers normalize them explicitly.
pub fn selected_layer_rgba(doc: &Document, layer: &Layer, rect: Rect) -> Result<RgbaImage> {
    let mut isolated = doc.clone();
    isolated.layers = vec![layer.clone()];
    rendered_rgba(&isolated, photocraft_compose::render(&isolated, rect))
}

/// Convert the model's sRGB pixels (normalized, unquantized) to document space. Coverage stays
/// unchanged.
fn surface_from(doc: &Document, rect: Rect, rgb: &RgbFrame, alpha: impl Fn(u32, u32) -> f32) -> Result<Surface> {
    use photocraft_cms::{Builtin, Intent, Transform};
    let fmt = PixelFormat::new(doc.mode, doc.depth, true);
    let dst = crate::color_cmds::document_profile(doc);
    let transform =
        Transform::new(Builtin::Srgb.profile(), &dst, Intent::RelativeColorimetric, true).map_err(|e| EngineError::Other(format!("colour management: {e}")))?;
    if transform.inputs() != 3 || transform.outputs() != fmt.mode.color_channels() {
        return Err(EngineError::Other("AI output cannot be converted to this document's colour mode.".into()));
    }
    let (w, h) = (rect.width(), rect.height());
    let mut data = vec![0f32; w as usize * h as usize * fmt.channels()];
    // Convert one row at a time to avoid another full float image at AI boundaries.
    let mut row = vec![0f32; w as usize * 4];
    for y in 0..h {
        for x in 0..w {
            let p = rgb.get(x as usize, y as usize);
            row[x as usize * 4..x as usize * 4 + 4].copy_from_slice(&[p[0], p[1], p[2], alpha(x, y)]);
        }
        let start = y as usize * w as usize * fmt.channels();
        transform.convert_f32(&row, 4, &mut data[start..start + w as usize * fmt.channels()], fmt.channels(), true);
    }
    let mut s = Surface::new(fmt);
    s.write_region(rect, &data);
    s.prune();
    Ok(s)
}

fn gray_surface(rect: Rect, mask: &GrayImage) -> Surface {
    let data: Vec<f32> = mask.pixels().map(|p| p[0] as f32 / 255.0).collect();
    let mut s = Surface::with_default(PixelFormat::GRAY8, &[0.0]);
    s.write_region(rect, &data);
    s.prune();
    s
}

/// Inserts `layer` above the active layer and makes it active.
fn add_layer_above(doc: &mut Document, active: &mut Option<LayerId>, layer: Layer) -> LayerId {
    let id = doc.insert_above(*active, layer);
    *active = Some(id);
    id
}

/// The stroke's coverage (brush footprint) from `points` and the brush params.
/// Brush coverage used by the removal preview and the final command.
pub fn stroke_mask(s: &Session, p: &Value, cmd: &str) -> Result<(Rect, Vec<f32>)> {
    let pts: Vec<StrokePoint> = p
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| bad(cmd, "missing `points`"))?
        .iter()
        .filter_map(|v| {
            let a = v.as_array()?;
            Some(StrokePoint::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2).and_then(Value::as_f64).unwrap_or(1.0) as f32))
        })
        .filter(|pt| pt.x.is_finite() && pt.y.is_finite())
        .collect();
    if pts.is_empty() {
        return Err(bad(cmd, "`points` is empty"));
    }
    let base = s.tools.brush.clone();
    let num = |k: &str, d: f32| p.get(k).and_then(Value::as_f64).map(|v| v as f32).filter(|v| v.is_finite()).unwrap_or(d);
    let brush = BrushSettings {
        size: num("size", base.size).clamp(1.0, 5000.0),
        hardness: (num("hardness", base.hardness * 100.0) / 100.0).clamp(0.0, 1.0),
        opacity: 1.0,
        flow: 1.0,
        spacing: 0.25,
        erase: false,
        ..base
    };
    Ok(photocraft_paint::retouch::stroke_coverage(&Stroke { brush, points: pts }))
}

fn mask_index(width: u32, x: u32, y: u32) -> usize {
    y as usize * width as usize + x as usize
}

#[test]
#[cfg(target_pointer_width = "64")]
fn ds_finding_mask_index_exceeds_u32_without_wrapping() {
    assert_eq!(mask_index(70_000, 69_999, 69_999), 4_899_999_999usize);
}

/// The removal area: stroke coverage, else the selection. Returns (bbox, full-doc mask).
fn removal_mask(s: &Session, p: &Value, cmd: &str) -> Result<(Rect, Size, Vec<u8>)> {
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let size = d.doc.size;
    let bounds = d.doc.bounds();
    let mut mask = vec![0u8; size.width as usize * size.height as usize];
    let mut bbox = Rect::EMPTY;
    let mut put = |x: i32, y: i32, v: f32| {
        if bounds.contains(x, y) && v > 0.0 {
            let i = y as usize * size.width as usize + x as usize;
            mask[i] = mask[i].max((v.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    };
    if let Some(bytes) = p.get("mask") {
        let bytes: Vec<u8> = serde_json::from_value(bytes.clone()).map_err(|_| bad(cmd, "invalid removal mask"))?;
        if bytes.len() != mask.len() {
            return Err(bad(cmd, "removal mask must match the document size"));
        }
        mask = bytes;
        for y in 0..size.height {
            for x in 0..size.width {
                if mask[mask_index(size.width, x, y)] > 0 {
                    bbox = bbox.union(&Rect::from_xywh(x as i32, y as i32, 1, 1));
                }
            }
        }
    } else if p.get("points").is_some() {
        let (r, cov) = stroke_mask(s, p, cmd)?;
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                put(x, y, cov[((y - r.y0) as u32 * r.width() + (x - r.x0) as u32) as usize]);
            }
        }
        bbox = r.intersect(&bounds);
    } else if let Some(sel) = d.doc.selection.as_ref() {
        let r = sel.content_bounds().intersect(&bounds);
        let vals = sel.read_region(r);
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                put(x, y, vals[((y - r.y0) as u32 * r.width() + (x - r.x0) as u32) as usize]);
            }
        }
        bbox = r;
    }
    if bbox.is_empty() || !mask.iter().any(|&v| v > 0) {
        return Err(EngineError::Other("Paint over what you want to remove, or make a selection first.".into()));
    }
    Ok((bbox, size, mask))
}

/// The region sent to the model: generous context around `bbox` (the AI crops further itself).
fn context_rect(bbox: Rect, bounds: Rect) -> Rect {
    let pad = (bbox.width().max(bbox.height()) as i32 * 2).max(640);
    bbox.inflate(pad).intersect(&bounds)
}

fn ai_remove(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ai.remove";
    let (bbox, size, mask) = removal_mask(s, p, CMD)?;
    let doc = s.active().ok_or(EngineError::NoDocument)?.doc.clone();
    let region = context_rect(bbox, doc.bounds());
    let engine = remove_engine(p);
    let seed = seed(p);
    let label = match engine {
        RemoveEngine::Auto { .. } | RemoveEngine::Klein => "AI Remove",
        RemoveEngine::Lama { .. } => "AI Remove (On-device)",
        RemoveEngine::Qwen { .. } => "AI Remove (Qwen)",
    };
    crate::jobs::run(
        s,
        label,
        true,
        move |ctx| {
            let rgb = composite_frame(&doc, region)?;
            let local = GrayImage::from_fn(region.width(), region.height(), |x, y| {
                Luma([mask[(region.y0 as usize + y as usize) * size.width as usize + region.x0 as usize + x as usize]])
            });
            let out = bridged(ctx, |ctl| li_ai::service().remove_patch_frame(&rgb, &local, &engine, seed, ctl))?;
            // Only the cleaned selection changed; its coverage becomes the new layer's alpha.
            let cleaned = li_ai::imaging::clean_selection_mask(&local);
            ctx.check()?;
            surface_from(&doc, region, &out, |x, y| cleaned.get_pixel(x, y)[0] as f32 / 255.0)
        },
        move |s, surf| {
            let id = s.edit(label, move |doc, active| {
                let mut l = Layer::raster(doc.next_layer_name("AI Remove"), surf.format());
                *crate::pixels_mut(&mut l)? = surf;
                Ok(add_layer_above(doc, active, l))
            })?;
            Ok(json!({ "layer": id.0, "bounds": [bbox.x0, bbox.y0, bbox.width(), bbox.height()] }))
        },
    )
}

/// Hair-grade edges: the model's matte becomes a trimap (confident inside/outside, an unknown
/// band along the edge and wherever the model was unsure) and the band is re-solved against the
/// actual image with closed-form matting (photocraft_algo::cf_matting).
fn refine_matte(img: &RgbaImage, alpha: &GrayImage) -> GrayImage {
    use photocraft_algo::cf_matting::{MattingParams, closed_form_matting_tiled, trimap_from_mask};
    let (w, h) = (img.width() as usize, img.height() as usize);
    let m: Vec<f32> = alpha.pixels().map(|p| p[0] as f32 / 255.0).collect();
    let band = ((w.min(h) as f32 / 160.0).round() as usize).clamp(3, 24);
    let mut tri = trimap_from_mask(&m, w, h, band);
    for (t, v) in tri.iter_mut().zip(&m) {
        if (0.1..0.9).contains(v) {
            *t = 0.5;
        }
    }
    let rgb = photocraft_algo::segment::RgbImage::from_fn(w, h, |x, y| {
        let p = img.get_pixel(x as u32, y as u32);
        [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
    });
    let a = closed_form_matting_tiled(&rgb, &tri, &MattingParams::default(), 384, 24);
    GrayImage::from_fn(w as u32, h as u32, |x, y| Luma([(a[y as usize * w + x as usize] * 255.0).round() as u8]))
}

/// The matte of the target layer from Qwen (document bounds).
fn matte_job(s: &Session, p: &Value) -> Result<(LayerId, Rect, RgbaImage, String, String, u64)> {
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let id = crate::commands::layer_param(s, p)?;
    let layer = d.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    let bounds = d.doc.bounds();
    let hint = p.get("hint").and_then(Value::as_str).unwrap_or("").chars().take(500).collect::<String>();
    // Sample all layers (default) or the layer alone.
    let img = if p.get("sampleAllLayers").and_then(Value::as_bool).unwrap_or(false) {
        let rgb = composite_rgb(&d.doc, bounds)?;
        image::DynamicImage::ImageRgb8(rgb).to_rgba8()
    } else {
        layer_rgba(&d.doc, layer, bounds)?
    };
    Ok((id, bounds, img, qwen_variant(p), hint, seed(p)))
}

fn refine_edges(p: &Value) -> bool {
    p.get("refineEdges").and_then(Value::as_bool).unwrap_or(true)
}

fn remove_background(s: &mut Session, p: &Value) -> Result<Value> {
    {
        let d = s.active().ok_or(EngineError::NoDocument)?;
        let id = crate::commands::layer_param(s, p)?;
        let l = d.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
        if !matches!(l.content, LayerContent::Raster(_)) {
            return Err(EngineError::Other(format!("Remove Background needs a pixel layer; “{}” is a {} layer.", l.name, l.content.kind_name())));
        }
        let locks = d.doc.effective_locks(l.id);
        if locks.pixels || locks.all {
            return Err(EngineError::Other(format!("Unlock “{}” first.", l.name)));
        }
    }
    let output = crate::cutout_cmds::BackgroundOutput::from_params(p, "ai.removeBackground")?;
    let (id, bounds, img, variant, hint, seed) = matte_job(s, p)?;
    let refine = refine_edges(p);
    crate::jobs::edit_job(
        s,
        "Remove Background (AI)",
        move |doc, active, ctx| {
            let mut alpha = bridged(ctx, |ctl| li_ai::service().cutout(&img, &variant, &hint, seed, ctl))?;
            if refine {
                ctx.progress(0.97, "Refining edges");
                alpha = refine_matte(&img, &alpha);
            }
            // Keep the layer's own transparency out of the matte's job: the mask only hides.
            let result = crate::cutout_cmds::apply_output(doc, id, gray_surface(bounds, &alpha), &output, ctx)?;
            *active = Some(result);
            Ok(result)
        },
        |result| json!({ "layer": result.0 }),
    )
}

fn select_subject(s: &mut Session, p: &Value) -> Result<Value> {
    let (_, bounds, img, variant, hint, seed) = matte_job(s, &{
        let mut q = p.clone();
        if q.get("sampleAllLayers").is_none() {
            q["sampleAllLayers"] = json!(true);
        }
        q
    })?;
    let refine = refine_edges(p);
    crate::jobs::run(
        s,
        "Select Subject (AI)",
        true,
        move |ctx| {
            let mut alpha = bridged(ctx, |ctl| li_ai::service().cutout(&img, &variant, &hint, seed, ctl))?;
            if refine {
                ctx.progress(0.97, "Refining edges");
                alpha = refine_matte(&img, &alpha);
            }
            Ok(gray_surface(bounds, &alpha))
        },
        move |s, surface| {
            s.edit("Select Subject (AI)", move |doc, _| {
                doc.selection = Some(surface);
                Ok(())
            })?;
            Ok(json!({}))
        },
    )
}

/// What Generative Fill asks for when the prompt is left empty: Photoshop's "fill from the
/// surroundings".
pub const SURROUNDINGS_PROMPT: &str =
    "Fill the masked area so it continues the surrounding image seamlessly: the same background, texture, lighting and perspective, with no new objects";

/// A result to replace when regenerating. Hide it only in the sampling snapshot until the
/// job succeeds; applying the new layer and hiding the old one is a single undo step.
fn replacement_layer(s: &Session, p: &Value, cmd: &str) -> Result<Option<LayerId>> {
    let Some(value) = p.get("replaceLayer") else { return Ok(None) };
    let id = LayerId(value.as_u64().ok_or_else(|| bad(cmd, "`replaceLayer` must be a layer id"))?);
    let l = s.active().ok_or(EngineError::NoDocument)?.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    if !matches!(l.content, LayerContent::Raster(_)) || l.generation.is_none() {
        return Err(bad(cmd, "`replaceLayer` must be an AI-generated pixel layer"));
    }
    Ok(Some(id))
}

fn generative_fill(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ai.generativeFill";
    let typed = p.get("prompt").and_then(Value::as_str).unwrap_or("").trim().chars().take(4000).collect::<String>();
    // As in Photoshop, an empty prompt fills the selection from its surroundings.
    let prompt = if typed.is_empty() { SURROUNDINGS_PROMPT.to_owned() } else { typed.clone() };
    let mut q = p.clone();
    if let Some(o) = q.as_object_mut() {
        o.remove("points");
    }
    let (bbox, size, mask) = removal_mask(s, &q, CMD)?;
    let replace = replacement_layer(s, p, CMD)?;
    let mut doc = s.active().ok_or(EngineError::NoDocument)?.doc.clone();
    if let Some(id) = replace {
        Arc::make_mut(&mut doc).layer_mut(id).ok_or(EngineError::NoLayer(id))?.visible = false;
    }
    let region = context_rect(bbox, doc.bounds());
    let (variant, seed) = (qwen_variant(p), seed(p));
    let name: String = if typed.is_empty() { "Generative Fill".to_owned() } else { format!("Fill: {}", typed.chars().take(32).collect::<String>()) };
    // Kept on the layer, so the Contextual Task Bar can regenerate it.
    let generation = json!({
        "command": CMD, "prompt": typed, "seed": seed, "engine": variant,
        "bounds": [bbox.x0, bbox.y0, bbox.width(), bbox.height()],
    });
    crate::jobs::run(
        s,
        "Generative Fill",
        true,
        move |ctx| {
            let rgb = composite_frame(&doc, region)?;
            let local = GrayImage::from_fn(region.width(), region.height(), |x, y| {
                Luma([mask[(region.y0 as usize + y as usize) * size.width as usize + region.x0 as usize + x as usize]])
            });
            let out = bridged(ctx, |ctl| li_ai::service().fill_frame(&rgb, &local, &prompt, &variant, seed, ctl))?;
            surface_from(&doc, region, &out, |x, y| local.get_pixel(x, y)[0] as f32 / 255.0)
        },
        move |s, surf| {
            let id = s.edit("Generative Fill", move |doc, active| {
                let mut l = Layer::raster(name, surf.format());
                *crate::pixels_mut(&mut l)? = surf;
                l.generation = Some(generation);
                if let Some(id) = replace {
                    doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?.visible = false;
                    *active = Some(id);
                }
                Ok(add_layer_above(doc, active, l))
            })?;
            Ok(json!({ "layer": id.0 }))
        },
    )
}

/// Generates at about 1.5 MP in the document's proportions; the result is scaled to cover it.
fn generation_size(size: Size) -> (u32, u32) {
    let (w, h) = (size.width.max(1) as f64, size.height.max(1) as f64);
    let s = (1_572_864.0 / (w * h)).sqrt();
    li_ai::imaging::snap_size((w * s) as u32, (h * s) as u32, 16)
}

fn generate_background(s: &mut Session, p: &Value) -> Result<Value> {
    let replace = replacement_layer(s, p, "ai.generateBackground")?;
    let prompt = p.get("prompt").and_then(Value::as_str).unwrap_or("").trim().chars().take(2000).collect::<String>();
    let doc = s.active().ok_or(EngineError::NoDocument)?.doc.clone();
    let (variant, seed, bounds) = (qwen_variant(p), seed(p), doc.bounds());
    let (gw, gh) = generation_size(doc.size);
    let generation = json!({"command": "ai.generateBackground", "prompt": prompt, "seed": seed, "engine": variant});
    crate::jobs::run(
        s,
        "Generate Background",
        true,
        move |ctx| {
            let img = bridged(ctx, |ctl| li_ai::service().generate_background(&prompt, gw, gh, &variant, seed, ctl))?;
            let filled = li_ai::imaging::fit_cover(&img, bounds.width(), bounds.height());
            surface_from(&doc, bounds, &RgbaFrame::from_rgba8(&filled).rgb(), |_, _| 1.0)
        },
        move |s, surf| {
            let id = s.edit("Generate Background", move |doc, active| {
                let mut l = Layer::raster(doc.next_layer_name("Background"), surf.format());
                *crate::pixels_mut(&mut l)? = surf;
                l.generation = Some(generation);
                if let Some(old) = replace {
                    doc.layer_mut(old).ok_or(EngineError::NoLayer(old))?.visible = false;
                    let id = doc.insert_above(Some(old), l);
                    *active = Some(id);
                    return Ok(id);
                }
                let target = *active;
                let id = doc.insert_above(target, l);
                // Below the active layer: the subject stays on top.
                doc.shift(id, -1);
                Ok(id)
            })?;
            Ok(json!({ "layer": id.0 }))
        },
    )
}

fn enhance(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ai.enhance";
    let method = p
        .get("method")
        .and_then(Value::as_str)
        .map(|v| li_ai::enhance::Method::parse(v).ok_or_else(|| bad(CMD, "unknown enhancement method")))
        .transpose()?
        .unwrap_or_default();
    let backend = p
        .get("backend")
        .cloned()
        .map(serde_json::from_value::<li_ai::enhance::Backend>)
        .transpose()
        .map_err(|_| bad(CMD, "unknown enhancement backend"))?
        .unwrap_or_default();
    let models_dir = crate::seg::models_dir();
    let choice = p.get("model").and_then(Value::as_str).map(|id| {
        if id == "seedvr2" {
            li_ai::enhance::selection::default_choice(li_ai::enhance::Method::Generative)
        } else {
            li_ai::enhance::selection::Choice::Curated { id: id.into() }
        }
    });
    if let Some(choice) = &choice {
        li_ai::enhance::selection::validate(method, choice).map_err(|e| bad(CMD, &e.to_string()))?;
    }
    let doc = s.active().ok_or(EngineError::NoDocument)?.doc.clone();
    let (sw, sh) = (doc.size.width, doc.size.height);
    let (mut w, mut h) = match (p.get("width").and_then(Value::as_u64), p.get("height").and_then(Value::as_u64)) {
        (Some(w), Some(h)) => (w as u32, h as u32),
        _ => {
            let factor = p.get("scale").and_then(Value::as_f64).unwrap_or(2.0).clamp(1.1, 4.0);
            ((sw as f64 * factor) as u32, (sh as f64 * factor) as u32)
        }
    };
    if method == li_ai::enhance::Method::Generative {
        w -= w % 2;
        h -= h % 2;
    }
    li_ai::enhance::native_scale((sw, sh), (w, h)).map_err(|e| bad(CMD, &e.to_string()))?;
    let name = format!("{} (enhanced)", doc.name.trim_end_matches(".png").trim_end_matches(".jpg"));
    let seed = seed(p);
    crate::jobs::run(
        s,
        "AI Enhance",
        false,
        move |ctx| {
            let img = flatten_rgba(&doc)?;
            let out = bridged(ctx, |ctl| match &choice {
                Some(choice) => li_ai::service().enhance_with_choice(&img, w, h, method, backend, &models_dir, seed, ctl, choice),
                None => li_ai::service().enhance(&img, w, h, method, backend, &models_dir, seed, ctl),
            })?;
            Ok(out)
        },
        move |s, out| {
            let mut d = Document::new(name, Size::new(w, h), ColorMode::Rgb, SampleType::U8);
            d.icc_profile = Some(photocraft_cms::Builtin::Srgb.profile().to_bytes());
            let fmt = PixelFormat::RGBA8;
            let surf = Surface::from_interleaved(fmt, Rect::from_xywh(0, 0, w, h), out.as_raw());
            let mut l = Layer::raster("Background", fmt);
            *crate::pixels_mut(&mut l)? = surf;
            d.layers.push(l);
            let idx = s.add_document(d, None);
            Ok(json!({ "document": idx }))
        },
    )
}

/// Places a PNG/JPEG file (a generated result) as a new layer: `fit` = `none` (at 0,0, for
/// edits of this document), `contain` (centred, scaled down to fit) or `cover`.
fn place_layer(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ai.placeLayer";
    let path = p.get("path").and_then(Value::as_str).ok_or_else(|| bad(CMD, "missing `path`"))?;
    let name = p.get("name").and_then(Value::as_str).unwrap_or("Generated").chars().take(120).collect::<String>();
    let fit = p.get("fit").and_then(Value::as_str).unwrap_or("contain").to_owned();
    let generation = p.get("generation").filter(|g| g.is_object()).cloned().map(|mut g| {
        g["command"] = json!("ai.generate");
        g
    });
    let img = file_rgba_srgb(std::path::Path::new(path))?;
    s.edit("Place Generated Image", move |doc, active| {
        let (dw, dh) = (doc.size.width, doc.size.height);
        let (iw, ih) = img.dimensions();
        let scale = match fit.as_str() {
            "cover" => (dw as f64 / iw as f64).max(dh as f64 / ih as f64),
            "contain" => (dw as f64 / iw as f64).min(dh as f64 / ih as f64).min(1.0),
            _ => 1.0,
        };
        let img = if (scale - 1.0).abs() > 1e-6 {
            image::imageops::resize(
                &img,
                ((iw as f64 * scale).round() as u32).max(1),
                ((ih as f64 * scale).round() as u32).max(1),
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            img
        };
        let (x, y) = if fit == "none" { (0, 0) } else { ((dw as i32 - img.width() as i32) / 2, (dh as i32 - img.height() as i32) / 2) };
        let mut l = layer_from_rgba(doc, &name, &img, x, y)?;
        l.generation = generation;
        Ok(add_layer_above(doc, active, l))
    })
    .map(|id| json!({ "layer": id.0 }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "ai.placeLayer",
            label: "Place Generated Image",
            menu: &[],
            shortcut: None,
            params: r#"{"path": file, "name"?: str, "fit"?: "none"|"contain"|"cover", "generation"?: object}"#,
            enabled: has_doc,
            run: place_layer,
            journal: true,
        },
        CommandSpec {
            id: "ai.remove",
            label: "AI Remove",
            menu: &[],
            shortcut: None,
            params: r#"{"points"?: [[x,y,pressure]], "size"?: px, "hardness"?: 0-100, "engine"?: "auto"|"lama"|"klein"|"qwen-int8"|"qwen-bf16", "seed"?: int} — without points, removes the selection"#,
            enabled: has_doc,
            run: ai_remove,
            journal: true,
        },
        CommandSpec {
            id: "ai.removeBackground",
            label: "Remove Background (AI)",
            menu: &[],
            shortcut: None,
            params: r##"{"layer"?: id, "engine"?: "int8"|"bf16"|"qwen-int8"|"qwen-bf16", "hint"?: "what to keep", "sampleAllLayers"?: bool, "seed"?: int, "refineEdges"?: bool=true, "output"?: "mask"|"transparent"|"white"|"color"|"blur"|"newLayer"="mask", "color"?: "#rrggbb[aa]"|[r,g,b,a?], "amount"?: 0..1000=12}"##,
            enabled: has_pixel_layer,
            run: remove_background,
            journal: true,
        },
        CommandSpec {
            id: "ai.selectSubject",
            label: "Select Subject (AI)",
            menu: &[],
            shortcut: None,
            params: r#"{"engine"?: "int8"|"bf16", "hint"?: str, "seed"?: int}"#,
            enabled: has_doc,
            run: select_subject,
            journal: true,
        },
        CommandSpec {
            id: "ai.generativeFill",
            label: "Generative Fill",
            menu: &[],
            shortcut: None,
            params: r#"{"prompt"?: str (empty: fill from the surroundings), "engine"?: "int8"|"bf16", "seed"?: int, "replaceLayer"?: id}"#,
            enabled: has_selection,
            run: generative_fill,
            journal: true,
        },
        CommandSpec {
            id: "ai.generateBackground",
            label: "Generate Background",
            menu: &[],
            shortcut: None,
            params: r#"{"prompt"?: "the empty scene", "engine"?: "int8"|"bf16", "seed"?: int, "replaceLayer"?: id}"#,
            enabled: has_doc,
            run: generate_background,
            journal: true,
        },
        CommandSpec {
            id: "ai.enhance",
            label: "AI Enhance",
            menu: &[],
            shortcut: None,
            params: r#"{"scale"?: 1.1-4 | "width"+"height", "method"?: "faithful"|"rescue"|"generative", "backend"?: "auto"|"cpu"|"gpu", "model"?: curated-upscaler-id, "seed"?: int}"#,
            enabled: has_doc,
            run: enhance,
            journal: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn colour_document(profile: photocraft_cms::Builtin, pixel: [f32; 4]) -> Result<Document> {
        let mut doc = Document::new("AI colour", Size::new(1, 1), ColorMode::Rgb, SampleType::F32);
        doc.icc_profile = Some(profile.profile().to_bytes());
        let mut layer = Layer::raster("Source", PixelFormat::RGBA32F);
        crate::pixels_mut(&mut layer)?.write_region(doc.bounds(), &pixel);
        doc.layers.push(layer);
        Ok(doc)
    }

    #[test]
    fn ai_colour_linear_references_encode_rgb_but_preserve_coverage() {
        let doc = colour_document(photocraft_cms::Builtin::LinearSrgb, [0.5, 0.25, 0.0, 0.5]).unwrap();
        // Independent IEC sRGB transfer-function values, not a round-trip oracle.
        let encoded = flatten_rgba(&doc).unwrap();
        assert_eq!(encoded.get_pixel(0, 0).0, [188, 137, 0, 128]);
        assert_eq!(layer_rgba(&doc, &doc.layers[0], doc.bounds()).unwrap(), encoded);
        let white = composite_rgb(&doc, doc.bounds()).unwrap();
        assert_eq!(white.get_pixel(0, 0).0, [221, 196, 128]);
        let preview = crate::color_cmds::composite_to_srgb(&doc, photocraft_compose::thumbnail_buffer(&doc, 1)).unwrap().to_rgba8();
        assert_eq!(preview.pixels, encoded.into_raw());
    }

    #[test]
    fn ai_colour_generated_srgb_decodes_into_linear_document_and_keeps_alpha() {
        let doc = colour_document(photocraft_cms::Builtin::LinearSrgb, [0.0; 4]).unwrap();
        let img = RgbaImage::from_pixel(1, 1, image::Rgba([128, 64, 0, 87]));
        let layer = layer_from_rgba(&doc, "Generated", &img, 3, 4).unwrap();
        let p = layer.surface().unwrap().read_region(Rect::from_xywh(3, 4, 1, 1));
        assert!((p[0] - 0.215_860_5).abs() < 0.0001, "{}", p[0]);
        assert!((p[1] - 0.051_269_46).abs() < 0.0001, "{}", p[1]);
        assert!(p[2].abs() < 1e-7, "matrix arithmetic preserves black within float precision: {}", p[2]);
        assert_eq!(p[3], 87.0 / 255.0);
        assert_eq!(layer_rgba(&doc, &layer, Rect::from_xywh(3, 4, 1, 1)).unwrap(), img);
    }

    #[test]
    fn ai_colour_wide_gamut_and_non_rgb_outputs_follow_document_profile() {
        use photocraft_cms::{Builtin, Intent, Transform};
        for (mode, profile) in [
            (ColorMode::Rgb, Builtin::AdobeRgbCompat),
            (ColorMode::Grayscale, Builtin::SGray),
            (ColorMode::Cmyk, Builtin::CoatedCmyk),
            (ColorMode::Lab, Builtin::LabD50),
        ] {
            let mut doc = Document::new("AI mode", Size::new(1, 1), mode, SampleType::F32);
            doc.icc_profile = Some(profile.profile().to_bytes());
            let img = RgbaImage::from_pixel(1, 1, image::Rgba([140, 90, 60, 73]));
            let layer = layer_from_rgba(&doc, "Generated", &img, 0, 0).unwrap();
            let pixels = layer.surface().unwrap().read_region(doc.bounds());
            let mut expected = [0f32; 4];
            let transform = Transform::new(Builtin::Srgb.profile(), profile.profile(), Intent::RelativeColorimetric, true).unwrap();
            transform.eval(&[140.0 / 255.0, 90.0 / 255.0, 60.0 / 255.0], &mut expected);
            for (actual, expected) in pixels[..mode.color_channels()].iter().zip(expected) {
                assert!((actual - expected).abs() < 0.002, "{mode:?}: {actual} vs {expected}");
            }
            assert_eq!(pixels[mode.color_channels()], 73.0 / 255.0, "{mode:?}");
        }
    }

    #[test]
    fn ai_colour_selected_layer_retains_document_patterns_and_cmyk_context() {
        use photocraft_cms::synth::{CmykParams, cmyk_profile};
        use photocraft_doc::{Fill, Pattern};
        let mut doc = colour_document(photocraft_cms::Builtin::LinearSrgb, [0.5, 0.25, 0.0, 1.0]).unwrap();
        let pattern = Pattern::new("Linear pattern", doc.layers[0].surface().unwrap().clone(), 1, 1);
        doc.layers[0].content =
            LayerContent::Fill(Fill::Pattern { id: pattern.id.clone(), name: pattern.name.clone(), scale: 1.0, phase: (0.0, 0.0), angle: 0.0, link: true });
        doc.patterns.push(pattern);
        let expected = flatten_rgba(&doc).unwrap();
        assert_eq!(expected.get_pixel(0, 0).0, [188, 137, 0, 255]);
        assert_eq!(selected_layer_rgba(&doc, &doc.layers[0], doc.bounds()).unwrap(), expected);

        let profile = cmyk_profile(&CmykParams { tvi: [0.45; 4], grid_a2b: 3, grid_b2a: 3, ..Default::default() });
        let mut doc = Document::new("Custom CMYK", Size::new(1, 1), ColorMode::Cmyk, SampleType::F32);
        doc.icc_profile = Some(profile.to_bytes());
        let mut layer = Layer::raster("Ink", PixelFormat::new(ColorMode::Cmyk, SampleType::F32, true));
        layer.surface_mut().unwrap().write_region(doc.bounds(), &[0.3, 0.6, 0.2, 0.1, 0.7]);
        doc.layers.push(layer);
        let expected = flatten_rgba(&doc).unwrap();
        assert_eq!(selected_layer_rgba(&doc, &doc.layers[0], doc.bounds()).unwrap(), expected);
        let mut untagged = doc.clone();
        untagged.icc_profile = None;
        assert_ne!(flatten_rgba(&untagged).unwrap(), expected, "fixture distinguishes the document's CMYK context");
    }

    #[test]
    fn ai_colour_tagged_file_references_match_open_document_and_generated_placement() {
        let doc = colour_document(photocraft_cms::Builtin::LinearSrgb, [0.5, 0.25, 0.0, 0.5]).unwrap();
        let bytes = photocraft_io::export(&doc, "png", &Default::default()).unwrap().bytes;
        let dir = std::env::temp_dir().join(format!(
            "li-ai-colour-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("linear.png");
        std::fs::write(&path, &bytes).unwrap();
        let img = file_rgba_srgb(&path).unwrap();
        let expected = flatten_rgba(&doc).unwrap();
        for (a, b) in img.as_raw().iter().zip(expected.as_raw()) {
            assert!(a.abs_diff(*b) <= 1, "{a} vs {b}");
        }
        let mut session = Session::new();
        session.add_document(colour_document(photocraft_cms::Builtin::LinearSrgb, [0.0; 4]).unwrap(), None);
        let before = session.active().unwrap().doc.clone();
        session.execute("ai.placeLayer", json!({"path":path, "fit":"none"})).unwrap();
        let d = session.active().unwrap();
        assert_eq!(flatten_rgba(&d.doc).unwrap(), img);
        session.execute("edit.undo", json!({})).unwrap();
        assert_eq!(session.active().unwrap().doc, before);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn final_review_bridge_stops_on_unwind_and_preserves_results() {
        let ctx = JobCtx::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<()> = bridged(&ctx, |_| {
                std::thread::sleep(Duration::from_millis(120));
                panic!("deliberate callback unwind");
            });
        }));
        assert!(result.is_err());
        ctx.progress(1.0, "sentinel");
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(ctx.message(), "sentinel", "no watcher may survive callback unwinding");
        assert_eq!(bridged(&JobCtx::new(), |_| Ok(vec![4, 9, 16])).unwrap(), [4, 9, 16]);
        let ctx = JobCtx::new();
        ctx.cancel();
        assert!(matches!(
            bridged(&ctx, |ctl| {
                let until = std::time::Instant::now() + Duration::from_secs(2);
                while !ctl.is_cancelled() && std::time::Instant::now() < until {
                    std::thread::yield_now();
                }
                ctl.check()
            }),
            Err(EngineError::Cancelled)
        ));
    }

    #[test]
    fn background_output_qwen_request_samples_the_target_and_keeps_hint_and_variant() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 5, "height": 3})).unwrap();
        s.execute("edit.fill", json!({"color": "#336699"})).unwrap();
        let id = LayerId(s.execute("layer.new.layer", json!({"name":"Subject"})).unwrap()["layer"].as_u64().unwrap());
        s.execute("edit.fill", json!({"color":"#cc4400"})).unwrap();
        for (engine, expected) in [("qwen-int8", "int8"), ("qwen-bf16", "bf16")] {
            let (target, bounds, img, variant, hint, seed) =
                matte_job(&s, &json!({"layer":id.0, "engine":engine, "hint":"the red car", "seed":42, "output":"white"})).unwrap();
            assert_eq!(target, id);
            assert_eq!(bounds, Rect::new(0, 0, 5, 3));
            assert_eq!(img.get_pixel(0, 0).0, [204, 68, 0, 255]);
            assert_eq!(variant, expected);
            assert_eq!(hint, "the red car");
            assert_eq!(seed, 42);
        }
    }

    #[test]
    fn explicit_removal_mask_matches_document_and_preserves_subtracted_holes() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 40, "height": 30})).unwrap();
        let mut bytes = vec![0u8; 40 * 30];
        bytes[10 * 40 + 10] = 255;
        bytes[10 * 40 + 12] = 128;
        let (bounds, size, mask) = removal_mask(&s, &json!({"mask":bytes}), "ai.remove").unwrap();
        assert_eq!(size, Size::new(40, 30));
        assert_eq!(bounds, Rect::from_xywh(10, 10, 3, 1));
        assert_eq!(mask[10 * 40 + 11], 0, "the subtraction hole stays empty");
        assert_eq!(mask[10 * 40 + 12], 128);
        assert!(removal_mask(&s, &json!({"mask":[255]}), "ai.remove").is_err());
        assert!(removal_mask(&s, &json!({"mask":vec![0u8;1200]}), "ai.remove").is_err());
    }

    #[test]
    fn generation_size_keeps_proportions() {
        let (w, h) = generation_size(Size::new(6000, 4000));
        assert!(w * h <= 1_700_000 && (w as f64 / h as f64 - 1.5).abs() < 0.05, "{w}x{h}");
    }

    #[test]
    fn ai_commands_fail_cleanly_without_a_document_or_engine() {
        let mut s = Session::new();
        for id in ["ai.remove", "ai.removeBackground", "ai.selectSubject", "ai.generativeFill", "ai.generateBackground", "ai.enhance"] {
            assert!(s.execute(id, json!({})).is_err(), "{id}");
        }
    }

    #[test]
    fn placed_generated_images_keep_their_generation_settings() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("pc-generated-{}-{stamp}.png", std::process::id()));
        RgbaImage::from_pixel(8, 8, image::Rgba([48, 112, 192, 255])).save(&path).unwrap();
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 20, "height": 20})).unwrap();
        let g = json!({"model": "qwen", "prompt": "marble", "seed": 42, "variant": "int8"});
        let r = s.execute("ai.placeLayer", json!({"path": path, "generation": g})).unwrap();
        std::fs::remove_file(path).unwrap();
        let id = LayerId(r["layer"].as_u64().unwrap());
        let generation = s.active().unwrap().doc.layer(id).unwrap().generation.as_ref().unwrap();
        assert_eq!(generation["command"], "ai.generate");
        assert_eq!(generation["prompt"], g["prompt"]);
        assert_eq!(generation["seed"], g["seed"]);
        assert_eq!(generation["model"], g["model"]);
    }

    /// The real flows against the mock ComfyUI: a stroke becomes an "AI Remove" layer, Remove
    /// Background puts a mask on the layer, Generative Fill adds a layer, Enhance opens a document.
    /// Like the UI's mock-server tests, skip when the sandbox denies local sockets.
    #[test]
    fn ai_commands_against_the_mock_server() {
        let server = match li_ai::mock::MockComfy::start() {
            Ok(server) => server,
            Err(e)
                if e.kind() == std::io::ErrorKind::PermissionDenied
                    || e.get_ref().and_then(|e| e.downcast_ref::<std::io::Error>()).is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied) =>
            {
                eprintln!("skipped: sandbox denies sockets for the mock HTTP server");
                return;
            }
            Err(e) => panic!("mock server: {e}"),
        };
        server.delay_ms.store(10, Ordering::SeqCst);
        li_ai::set_service_override(Some(server.host().to_owned()));
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 320, "height": 240})).expect("new");
        s.execute("edit.fill", json!({"color": "#3366cc"})).expect("fill");
        let r = s.execute("ai.remove", json!({"points": [[100.0, 100.0, 1.0], [140.0, 110.0, 1.0]], "size": 30})).expect("remove");
        assert!(r.get("layer").is_some(), "{r}");
        let names: Vec<String> = s.active().expect("doc").doc.walk().into_iter().map(|(_, _, l)| l.name.clone()).collect();
        assert!(names.iter().any(|n| n.starts_with("AI Remove")), "{names:?}");
        // Undo removes the layer in one step.
        s.execute("edit.undo", json!({})).expect("undo");
        let after: Vec<String> = s.active().expect("doc").doc.walk().into_iter().map(|(_, _, l)| l.name.clone()).collect();
        assert!(!after.iter().any(|n| n.starts_with("AI Remove")), "{after:?}");
        // A subject on the plain background (the mock keeps what differs from the border).
        s.execute("paint.stroke", json!({"points": [[150.0, 120.0], [170.0, 120.0]], "size": 80, "hardness": 100})).expect("subject");
        s.execute("ai.removeBackground", json!({})).expect("cutout");
        let d = s.active().expect("doc");
        let active = d.active_layer.expect("active");
        assert!(d.doc.layer(active).expect("layer").mask.is_some());
        s.execute("select.all", json!({})).expect("select all");
        let r = s.execute("ai.generativeFill", json!({"prompt": "a red balloon", "seed": 7})).expect("fill");
        // The layer keeps how it was made, for Regenerate.
        let id = LayerId(r["layer"].as_u64().expect("layer"));
        let g = s.active().expect("doc").doc.layer(id).expect("layer").generation.clone().expect("generation");
        assert_eq!((g["command"].as_str(), g["prompt"].as_str(), g["seed"].as_u64()), (Some("ai.generativeFill"), Some("a red balloon"), Some(7)));
        // An empty prompt fills from the surroundings, as in Photoshop.
        let r = s.execute("ai.generativeFill", json!({"prompt": ""})).expect("empty prompt fills");
        let id = LayerId(r["layer"].as_u64().expect("layer"));
        assert_eq!(s.active().expect("doc").doc.layer(id).expect("layer").name, "Generative Fill");
        let r = s.execute("ai.generativeFill", json!({"prompt": "", "replaceLayer": id.0, "seed": 43})).expect("regenerate fill");
        let new = LayerId(r["layer"].as_u64().expect("layer"));
        assert!(!s.active().unwrap().doc.layer(id).unwrap().visible);
        assert_eq!(s.active().unwrap().doc.layer(new).unwrap().generation.as_ref().unwrap()["seed"], 43);
        s.undo();
        assert!(s.active().unwrap().doc.layer(id).unwrap().visible, "hide and replace are one undo step");
        let before = s.active().unwrap().doc.clone();
        server.fail_next();
        assert!(s.execute("ai.generativeFill", json!({"prompt": "", "replaceLayer": id.0})).is_err());
        assert_eq!(s.active().unwrap().doc, before, "a failed generation changes no layers");
        let r = s.execute("ai.generateBackground", json!({"prompt": "marble"})).expect("bg");
        let bg = LayerId(r["layer"].as_u64().expect("layer"));
        let r = s.execute("ai.generateBackground", json!({"prompt": "marble", "replaceLayer": bg.0})).expect("regenerate background");
        assert_eq!(s.active().unwrap().active_layer.map(|id| id.0), r["layer"].as_u64());
        assert!(!s.active().unwrap().doc.layer(bg).unwrap().visible);
        s.undo();
        assert!(s.active().unwrap().doc.layer(bg).unwrap().visible);
        let before = s.documents().len();
        s.execute("ai.enhance", json!({"scale": 2.0})).expect("enhance");
        assert_eq!(s.documents().len(), before + 1);
        // Advanced Generate's explicit checkpoint must override the function's default.
        s.execute("file.new", json!({"width": 67, "height": 49})).unwrap();
        s.execute("ai.enhance", json!({"method": "faithful", "backend": "gpu", "model": "realplksr-4x", "scale": 2})).unwrap();
        assert_eq!(s.active().unwrap().doc.size, Size::new(134, 98));
        assert_eq!(server.prompts().last().unwrap()["1"]["inputs"]["model_name"], "4x_realplksr_mssim_pretrain.pth");
        let before = s.documents().len();
        assert!(s.execute("ai.enhance", json!({"method": "faithful", "model": "not-an-upscaler"})).is_err());
        assert_eq!(s.documents().len(), before);
        li_ai::set_service_override(None);
    }
}

/// Real local inference followed by the actual compositor and document undo/redo path.
#[cfg(test)]
#[test]
#[ignore = "requires pinned LaMa under LOCAL_IMAGE_SEG_MODELS and real RTX 5090 Vulkan"]
fn lama_real_rtx5090_patch_undo_redo_parity() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device("LaMa compositor");
    let mut gpu = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lama/photos/portrait.jpg");
    let input = image::imageops::resize(&image::open(source).unwrap().to_rgba8(), 320, 240, image::imageops::FilterType::Lanczos3);
    let format = photocraft_color::PixelFormat::RGBA8;
    let mut doc = Document::new("CC0 LaMa GPU test", Size::new(320, 240), photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8);
    let mut layer = Layer::raster("CC0 source", format);
    *layer.surface_mut().unwrap() = Surface::from_interleaved(format, doc.bounds(), input.as_raw());
    doc.layers.push(layer);
    let mut session = Session::new();
    session.add_document(doc, None);
    let before = session.active().unwrap().doc.clone();
    let start = std::time::Instant::now();
    let result = session.execute("ai.remove", json!({"engine":"lama","points":[[160.0,120.0,1.0]],"size":32.0,"hardness":75})).unwrap();
    let seconds = start.elapsed().as_secs_f64();
    assert!(result["layer"].is_u64());
    let repaired = session.active().unwrap().doc.clone();
    let mut check = |doc: &Document| {
        let cpu = photocraft_compose::flatten(doc);
        let pixels = photocraft_gpu::render_to_vec(&mut gpu, &device, &queue, doc, doc.bounds()).unwrap();
        let max = cpu
            .px
            .iter()
            .zip(&pixels)
            .flat_map(|(a, b)| {
                (0..4).map(move |c| {
                    let x = if c == 3 { a[c] } else { a[c] * a[3] };
                    (x - b[c]).abs()
                })
            })
            .fold(0f32, f32::max);
        assert!(max <= 0.5 / 255.0, "GPU max difference {max}");
        eprintln!("LaMa GPU max difference {max}");
    };
    check(&repaired);
    session.execute("edit.undo", json!({})).unwrap();
    assert_eq!(session.active().unwrap().doc, before);
    check(&before);
    session.execute("edit.redo", json!({})).unwrap();
    assert_eq!(session.active().unwrap().doc, repaired);
    check(&repaired);
    eprintln!("actual ai.remove (CPU LaMa + layer) {seconds:.3}s; GPU patch undo/redo passed");
}

#[cfg(test)]
mod j28_tests {
    use super::*;

    /// A 16-bit sRGB document holding a deep gradient: about six 8-bit levels across 512 px.
    fn deep_document(sample: SampleType, fmt: PixelFormat) -> Document {
        let mut doc = Document::new("Deep", Size::new(512, 2), ColorMode::Rgb, sample);
        doc.icc_profile = Some(photocraft_cms::Builtin::Srgb.profile().to_bytes());
        let mut layer = Layer::raster("Source", fmt);
        let px: Vec<f32> = (0..2 * 512).flat_map(|i| [0.40 + 0.02 * (i % 512) as f32 / 511.0, 0.55, 0.70, 1.0]).collect();
        crate::pixels_mut(&mut layer).unwrap().write_region(doc.bounds(), &px);
        doc.layers.push(layer);
        doc
    }

    #[test]
    fn j28_ai_boundary_keeps_16bit_and_float_documents_out_of_8bit() {
        for (sample, fmt) in [(SampleType::U16, PixelFormat::RGBA16), (SampleType::F32, PixelFormat::RGBA32F)] {
            let doc = deep_document(sample, fmt);
            let rect = doc.bounds();
            let source = doc.layers[0].surface().unwrap().read_region(rect);
            // An identity "model": what comes back is what the engine was given.
            let before = surface_from(&doc, rect, &RgbFrame::from_rgb8(&composite_rgb(&doc, rect).unwrap()), |_, _| 1.0).unwrap().read_region(rect);
            let after = surface_from(&doc, rect, &composite_frame(&doc, rect).unwrap(), |_, _| 1.0).unwrap().read_region(rect);
            let err = |v: &[f32]| v.iter().zip(&source).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max) * 65535.0;
            let levels = |v: &[f32]| v.as_chunks::<4>().0.iter().map(|p| (p[0] * 65535.0).round() as u32).collect::<std::collections::BTreeSet<_>>().len();
            eprintln!(
                "j28 editor AI boundary {sample:?}: max error before {:.1}, after {:.2} 16-bit levels; distinct red levels before {}, after {} (source {})",
                err(&before),
                err(&after),
                levels(&before),
                levels(&after),
                levels(&source)
            );
            assert!(err(&after) <= 1.0, "{sample:?}: no 8-bit step remains ({})", err(&after));
            assert!(err(&before) > 50.0);
            assert_eq!(composite_frame(&doc, rect).unwrap().to_rgb8(), composite_rgb(&doc, rect).unwrap(), "8-bit wire bytes unchanged");
        }
    }
}
