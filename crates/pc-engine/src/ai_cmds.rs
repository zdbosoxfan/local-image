//! local-image: Local Image's AI commands, run on a local ComfyUI through `li-ai`.
//!
//! Every command snapshots the document, does the AI work on a background job (progress and
//! cancel go through [`JobCtx`]) and applies the result as one history step:
//!
//! | Command | Result |
//! |---|---|
//! | `ai.remove` | the stroke (`points`) or the selection repaired, as a new layer above the active one |
//! | `ai.removeBackground` | a layer mask on the layer (Qwen alpha matte), pixels untouched |
//! | `ai.selectSubject` | the subject as the selection |
//! | `ai.generativeFill` | the selection regenerated from a prompt, as a new layer |
//! | `ai.generateBackground` | an empty background plate as a new layer below the active one |
//! | `ai.enhance` | the flattened image enhanced and enlarged (SeedVR2) as a new document |
//!
//! This file is Local Image's addition to the vendored PhotoCraft engine (new file: never
//! conflicts on upstream merges). Hook: `lib.rs` (`pub mod ai_cmds`) and `commands.rs`
//! (`v.extend(crate::ai_cmds::specs())`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use image::{GrayImage, Luma, Rgb, RgbImage, RgbaImage};
use li_ai::{JobControl, RemoveEngine, Stage};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, LayerContent, LayerId, LayerMask};
use photocraft_geom::{Rect, Size};
use photocraft_paint::{BrushSettings, Stroke, StrokePoint};
use photocraft_raster::{Surface, from_rgba_into};
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
    match p.get("engine").and_then(Value::as_str).unwrap_or("klein") {
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
    let done = Arc::new(AtomicBool::new(false));
    let (c, d, jc) = (ctl.clone(), done.clone(), ctx.clone());
    let watcher = std::thread::spawn(move || {
        while !d.load(Ordering::SeqCst) {
            if jc.cancelled() {
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
            jc.progress(frac, &msg);
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    let r = f(&ctl);
    done.store(true, Ordering::SeqCst);
    let _ = watcher.join();
    r.map_err(|e| if e.is::<li_ai::Cancelled>() { EngineError::Cancelled } else { EngineError::Other(format!("{e:#}")) })
}

/// The visible composite of `rect` as 8-bit RGB (transparency over white) — what the user sees.
fn composite_rgb(doc: &Document, rect: Rect) -> RgbImage {
    let buf = photocraft_compose::render(doc, rect);
    let (w, h) = (rect.width(), rect.height());
    RgbImage::from_fn(w, h, |x, y| {
        let p = buf.px[(y * w + x) as usize];
        let a = p[3].clamp(0.0, 1.0);
        let c = |v: f32| ((v.clamp(0.0, 1.0) * a + (1.0 - a)) * 255.0).round() as u8;
        Rgb([c(p[0]), c(p[1]), c(p[2])])
    })
}

/// The flattened document as 8-bit RGBA (for AI references and edits).
pub fn flatten_rgba(doc: &Document) -> RgbaImage {
    let b = doc.bounds();
    let buf = photocraft_compose::render(doc, b);
    let (w, h) = (b.width(), b.height());
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    RgbaImage::from_fn(w, h, |x, y| {
        let p = buf.px[(y * w + x) as usize];
        image::Rgba([q(p[0]), q(p[1]), q(p[2]), q(p[3])])
    })
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
pub fn layer_from_rgba(doc: &Document, name: &str, img: &RgbaImage, x: i32, y: i32) -> Layer {
    let fmt = doc.pixel_format();
    let rect = Rect::from_xywh(x, y, img.width(), img.height());
    let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let surf = surface_from(fmt, rect, &rgb, |px, py| img.get_pixel(px, py)[3] as f32 / 255.0);
    let mut l = Layer::raster(name, surf.format());
    if let LayerContent::Raster(s) = &mut l.content {
        *s = surf;
    }
    l
}

/// One layer rendered alone over `rect` as RGBA8 (with its own transparency).
fn layer_rgba(layer: &Layer, rect: Rect) -> RgbaImage {
    let mut l = layer.clone();
    l.mask = None;
    l.visible = true;
    l.opacity = 1.0;
    let buf = photocraft_compose::render_layer(&l, rect);
    let (w, h) = (rect.width(), rect.height());
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    RgbaImage::from_fn(w, h, |x, y| {
        let p = buf.px[(y * w + x) as usize];
        image::Rgba([q(p[0]), q(p[1]), q(p[2]), q(p[3])])
    })
}

/// A doc-format surface holding `rgb` at `rect`, with `alpha` (0..1 per pixel) as transparency.
fn surface_from(fmt: PixelFormat, rect: Rect, rgb: &RgbImage, alpha: impl Fn(u32, u32) -> f32) -> Surface {
    let fmt = PixelFormat::new(fmt.mode, fmt.sample, true);
    let n = fmt.channels();
    let (w, h) = (rect.width(), rect.height());
    let mut data = vec![0f32; (w * h) as usize * n];
    let mut enc = [0f32; 8];
    for y in 0..h {
        for x in 0..w {
            let p = rgb.get_pixel(x, y);
            let px = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, alpha(x, y)];
            from_rgba_into(&fmt, px, &mut enc);
            let i = (y * w + x) as usize * n;
            data[i..i + n].copy_from_slice(&enc[..n]);
        }
    }
    let mut s = Surface::new(fmt);
    s.write_region(rect, &data);
    s.prune();
    s
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
fn stroke_mask(s: &Session, p: &Value, cmd: &str) -> Result<(Rect, Vec<f32>)> {
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
    if p.get("points").is_some() {
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
        RemoveEngine::Klein => "AI Remove",
        RemoveEngine::Qwen { .. } => "AI Remove (Qwen)",
    };
    let fmt = doc.pixel_format();
    crate::jobs::run(
        s,
        label,
        true,
        move |ctx| {
            let rgb = composite_rgb(&doc, region);
            let local = GrayImage::from_fn(region.width(), region.height(), |x, y| {
                Luma([mask[(region.y0 as usize + y as usize) * size.width as usize + region.x0 as usize + x as usize]])
            });
            let out = bridged(ctx, |ctl| li_ai::service().remove_objects(&rgb, &local, &engine, seed, ctl))?;
            // Only the cleaned selection changed; its coverage becomes the new layer's alpha.
            let cleaned = li_ai::imaging::clean_selection_mask(&local);
            ctx.check()?;
            Ok(surface_from(fmt, region, &out, |x, y| cleaned.get_pixel(x, y)[0] as f32 / 255.0))
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
        let rgb = composite_rgb(&d.doc, bounds);
        image::DynamicImage::ImageRgb8(rgb).to_rgba8()
    } else {
        layer_rgba(layer, bounds)
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
    let (id, bounds, img, variant, hint, seed) = matte_job(s, p)?;
    let refine = refine_edges(p);
    crate::jobs::run(
        s,
        "Remove Background (AI)",
        true,
        move |ctx| {
            let mut alpha = bridged(ctx, |ctl| li_ai::service().cutout(&img, &variant, &hint, seed, ctl))?;
            if refine {
                ctx.progress(0.97, "Refining edges");
                alpha = refine_matte(&img, &alpha);
            }
            // Keep the layer's own transparency out of the matte's job: the mask only hides.
            Ok(gray_surface(bounds, &alpha))
        },
        move |s, surface| {
            s.edit("Remove Background (AI)", move |doc, active| {
                crate::extra_cmds::background_to_layer_for_mask(doc, id);
                doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?.mask = Some(LayerMask { surface, ..LayerMask::reveal_all() });
                doc.selection = None;
                *active = Some(id);
                Ok(())
            })?;
            Ok(json!({ "layer": id.0 }))
        },
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
    let (variant, seed, fmt) = (qwen_variant(p), seed(p), doc.pixel_format());
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
            let rgb = composite_rgb(&doc, region);
            let local = GrayImage::from_fn(region.width(), region.height(), |x, y| {
                Luma([mask[(region.y0 as usize + y as usize) * size.width as usize + region.x0 as usize + x as usize]])
            });
            let out = bridged(ctx, |ctl| li_ai::service().fill(&rgb, &local, &prompt, &variant, seed, ctl))?;
            Ok(surface_from(fmt, region, &out, |x, y| local.get_pixel(x, y)[0] as f32 / 255.0))
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
    let (variant, seed, fmt, bounds) = (qwen_variant(p), seed(p), doc.pixel_format(), doc.bounds());
    let (gw, gh) = generation_size(doc.size);
    let generation = json!({"command": "ai.generateBackground", "prompt": prompt, "seed": seed, "engine": variant});
    crate::jobs::run(
        s,
        "Generate Background",
        true,
        move |ctx| {
            let img = bridged(ctx, |ctl| li_ai::service().generate_background(&prompt, gw, gh, &variant, seed, ctl))?;
            let filled = li_ai::imaging::fit_cover(&img, bounds.width(), bounds.height());
            let rgb = image::DynamicImage::ImageRgba8(filled).to_rgb8();
            Ok(surface_from(fmt, bounds, &rgb, |_, _| 1.0))
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
    let doc = s.active().ok_or(EngineError::NoDocument)?.doc.clone();
    let (sw, sh) = (doc.size.width, doc.size.height);
    let (mut w, mut h) = match (p.get("width").and_then(Value::as_u64), p.get("height").and_then(Value::as_u64)) {
        (Some(w), Some(h)) => (w as u32, h as u32),
        _ => {
            let factor = p.get("scale").and_then(Value::as_f64).unwrap_or(2.0).clamp(1.1, 4.0);
            ((sw as f64 * factor) as u32, (sh as f64 * factor) as u32)
        }
    };
    w -= w % 2;
    h -= h % 2;
    if w <= sw || h <= sh || w > 8192 || h > 8192 {
        return Err(bad(CMD, "choose a larger size, up to 8192 px per side"));
    }
    let name = format!("{} (enhanced)", doc.name.trim_end_matches(".png").trim_end_matches(".jpg"));
    let seed = seed(p);
    crate::jobs::run(
        s,
        "AI Enhance",
        false,
        move |ctx| {
            let bounds = doc.bounds();
            let buf = photocraft_compose::render(&doc, bounds);
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let img = RgbaImage::from_fn(sw, sh, |x, y| {
                let p = buf.px[(y * sw + x) as usize];
                image::Rgba([q(p[0]), q(p[1]), q(p[2]), q(p[3])])
            });
            let out = bridged(ctx, |ctl| li_ai::service().upscale(&img, w, h, seed, ctl))?;
            Ok(out)
        },
        move |s, out| {
            let mut d = Document::new(name, Size::new(w, h), ColorMode::Rgb, SampleType::U8);
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
    let bytes = std::fs::read(path).map_err(|e| EngineError::Other(format!("Could not read {path}: {e}")))?;
    let img = image::load_from_memory(&bytes).map_err(|e| EngineError::Other(format!("Could not decode {path}: {e}")))?.to_rgba8();
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
        let mut l = layer_from_rgba(doc, &name, &img, x, y);
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
            params: r#"{"points"?: [[x,y,pressure]], "size"?: px, "hardness"?: 0-100, "engine"?: "klein"|"qwen-int8"|"qwen-bf16", "seed"?: int} — without points, removes the selection"#,
            enabled: has_doc,
            run: ai_remove,
            journal: true,
        },
        CommandSpec {
            id: "ai.removeBackground",
            label: "Remove Background (AI)",
            menu: &[],
            shortcut: None,
            params: r#"{"layer"?: id, "engine"?: "int8"|"bf16", "hint"?: "what to keep", "sampleAllLayers"?: bool, "seed"?: int}"#,
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
            params: r#"{"scale"?: 1.1-4 | "width"+"height", "seed"?: int}"#,
            enabled: has_doc,
            run: enhance,
            journal: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

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
        li_ai::set_service_override(None);
    }
}
