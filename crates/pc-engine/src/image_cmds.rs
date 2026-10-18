//! Image menu: Image Size, Canvas Size, Crop, Trim, Mode (colour model and
//! bit depth) conversions, Duplicate.

use li_raster::resample::Resample;
use photocraft_algo::resample::{crop_surface, resize_surface_in_canvas, translate_surface};
use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{Document, Effect, Effects, FxPaint, Layer, LayerContent, Size};
use photocraft_geom::Rect;
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

/// Applies `f` to every pixel surface of a layer tree: content (raster or
/// cached text/shape/smart pixels), fill caches, and (if `masks`) masks.
pub(crate) fn for_each_surface(layers: &mut [Layer], masks: bool, f: &mut dyn FnMut(&mut Surface, bool)) {
    for l in layers {
        if masks && let Some(m) = &mut l.mask {
            f(&mut m.surface, true);
        }
        if let Some(fc) = &mut l.fill_cache {
            f(&mut fc.surface, false);
        }
        match &mut l.content {
            LayerContent::Raster(s) => f(s, false),
            LayerContent::Text(t) => {
                if let Some(c) = &mut t.cache {
                    f(c, false)
                }
            }
            LayerContent::Shape(sh) => {
                if let Some(c) = &mut sh.cache {
                    f(c, false)
                }
            }
            LayerContent::Smart(sm) => {
                if let Some(c) = &mut sm.cache {
                    f(c, false)
                }
            }
            LayerContent::Symbol(s) => {
                if let Some(cache) = &mut s.cache {
                    f(cache, false);
                }
            }
            LayerContent::Group(g) => for_each_surface(&mut g.children, masks, f),
            _ => {}
        }
    }
}

/// Converts every pixel surface of a layer tree (masks included) to `depth`.
pub(crate) fn convert_layers_depth(layers: &mut [Layer], depth: SampleType) {
    for_each_surface(layers, true, &mut |surf, _| {
        let f = surf.format().with_sample(depth);
        *surf = surf.convert(f);
    });
}

pub(crate) fn for_each_layer(layers: &mut [Layer], f: &mut dyn FnMut(&mut Layer)) {
    for l in layers {
        f(l);
        if let Some(ch) = l.children_mut() {
            for_each_layer(ch, f);
        }
    }
}

pub(crate) fn scale_effects(fx: &mut Effects, k: f32) {
    for e in &mut fx.items {
        match e {
            Effect::DropShadow(s) | Effect::InnerShadow(s) => {
                s.distance *= k;
                s.size *= k;
            }
            Effect::OuterGlow(g) | Effect::InnerGlow(g) => g.size *= k,
            Effect::Stroke(s) => s.size *= k,
            Effect::Satin(s) => {
                s.distance *= k;
                s.size *= k;
            }
            Effect::BevelEmboss(b) => {
                b.size *= k;
                b.soften *= k;
            }
            Effect::PatternOverlay { scale, .. } => *scale *= k,
            Effect::GradientOverlay { .. } | Effect::ColorOverlay { .. } => {}
        }
        if let Effect::Stroke(s) = e
            && let FxPaint::Pattern { scale, .. } = &mut s.paint
        {
            *scale *= k;
        }
    }
    // Effects changed: the preserved PSD block no longer applies.
    if k != 1.0 {
        fx.psd_raw = None;
    }
}

fn parse_resample(s: &str) -> Option<Resample> {
    Some(match s {
        "none" => return None,
        "nearest" | "nearestNeighbor" => Resample::Nearest,
        "bilinear" => Resample::Bilinear,
        "lanczos" => Resample::Lanczos,
        "preserveDetails" => Resample::PreserveDetails,
        // local-image: real Smoother/Sharper kernels (they used to fall through to plain bicubic).
        "bicubicSmoother" => Resample::BicubicSmoother,
        "bicubicSharper" => Resample::BicubicSharper,
        _ => Resample::Bicubic,
    })
}

/// "bicubicAutomatic": Smoother when enlarging, Sharper when reducing (Photoshop's rule).
fn auto_resample(r: Option<&str>, enlarging: bool) -> Option<Resample> {
    match r {
        Some("bicubicAutomatic") | Some("automatic") => Some(if enlarging { Resample::BicubicSmoother } else { Resample::BicubicSharper }),
        r => parse_resample(r.unwrap_or("bicubic")),
    }
}

/// The largest raster Image Size resamples to: the decoders' default allocation budget.
const MAX_RESAMPLE_BYTES: u64 = if cfg!(target_pointer_width = "64") { 8 << 30 } else { 2 << 30 };

/// Image → Image Size.
fn image_size(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let (ow, oh) = (d.doc.size.width, d.doc.size.height);
    let pw = p.get("width").and_then(Value::as_f64);
    let ph = p.get("height").and_then(Value::as_f64);
    // Missing dimension keeps the aspect ratio.
    let (nw, nh) = match (pw, ph) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w * oh as f64 / ow.max(1) as f64),
        (None, Some(h)) => (h * ow as f64 / oh.max(1) as f64, h),
        (None, None) => (ow as f64, oh as f64),
    };
    let (nw, nh) = (nw.round().clamp(1.0, 300_000.0) as u32, nh.round().clamp(1.0, 300_000.0) as u32);
    let resample = auto_resample(p.get("resample").and_then(Value::as_str), u64::from(nw) * u64::from(nh) > u64::from(ow) * u64::from(oh));
    let pixel_bytes = d.doc.pixel_format().bytes_per_pixel().max(1) as u64;
    let bytes = u64::from(nw).saturating_mul(u64::from(nh)).saturating_mul(pixel_bytes);
    if resample.is_some() && (nw, nh) != (ow, oh) && bytes > MAX_RESAMPLE_BYTES {
        return Err(bad(
            "image.imageSize",
            format!(
                "{nw} x {nh} px exceeds the {} megapixels resampling limit at this bit depth. Choose a smaller size, or turn Resample off to change only the resolution",
                MAX_RESAMPLE_BYTES / pixel_bytes / 1_000_000
            ),
        ));
    }
    let dpi = p.get("resolution").and_then(Value::as_f64).map(|v| v as f32);
    s.edit("Image Size", |doc, _| {
        if let Some(r) = dpi {
            doc.resolution_dpi = r.clamp(1.0, 10_000.0);
        }
        let Some(filter) = resample else { return Ok(()) };
        let (sx, sy) = (nw as f64 / ow.max(1) as f64, nh as f64 / oh.max(1) as f64);
        if (sx - 1.0).abs() < 1e-12 && (sy - 1.0).abs() < 1e-12 {
            return Ok(());
        }
        // Content that reaches the canvas edge keeps it: a Background stays opaque to the border.
        let canvas = Rect::new(0, 0, ow as i32, oh as i32);
        for_each_surface(&mut doc.layers, true, &mut |surf, is_mask| {
            *surf = resize_surface_in_canvas(surf, sx, sy, if is_mask { Resample::Bilinear } else { filter }, canvas);
        });
        let k = ((sx + sy) / 2.0) as f32;
        for_each_layer(&mut doc.layers, &mut |l| scale_effects(&mut l.effects, k));
        for ch in doc.channels.iter_mut().chain(doc.quick_mask.as_mut()) {
            ch.surface = resize_surface_in_canvas(&ch.surface, sx, sy, Resample::Bilinear, canvas);
        }
        if let Some(sel) = &doc.selection {
            doc.selection = Some(resize_surface_in_canvas(sel, sx, sy, Resample::Bilinear, canvas));
        }
        // Vector geometry, guides and marks scale with the pixels; vectors re-render sharp.
        crate::canvas_geom::transform_geometry(doc, &photocraft_geom::Affine { m: [sx, 0.0, 0.0, sy, 0.0, 0.0] })?;
        doc.size = Size::new(nw, nh);
        crate::canvas_geom::refresh(doc, crate::canvas_geom::Refresh::All)?;
        Ok(())
    })?;
    Ok(json!({ "width": nw, "height": nh }))
}

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

/// Moves every surface, channel, selection and guide by `(dx, dy)`.
///
/// Fails, before moving anything, when some pixels would land outside the i32 coordinate
/// range: the surface copy saturates its target rectangle there, which no longer matches
/// the pixel data and panics (upstream 0c2c268c, #959).
fn translate_doc(doc: &mut Document, cmd: &str, dx: i32, dy: i32) -> Result<()> {
    if dx == 0 && dy == 0 {
        return Ok(());
    }
    let fits = |r: Rect| {
        r.is_empty() || (r.x0.checked_add(dx).is_some() && r.x1.checked_add(dx).is_some() && r.y0.checked_add(dy).is_some() && r.y1.checked_add(dy).is_some())
    };
    let mut ok = true;
    for_each_surface(&mut doc.layers, true, &mut |surf, _| ok = ok && fits(surf.content_bounds()));
    ok = ok
        && doc.channels.iter().chain(doc.quick_mask.as_ref()).all(|ch| fits(ch.surface.content_bounds()))
        && doc.selection.as_ref().is_none_or(|sel| fits(sel.content_bounds()));
    if !ok {
        return Err(bad(cmd, format!("moving the document by ({dx}, {dy}) would push its pixels outside the 32-bit coordinate range")));
    }
    for_each_surface(&mut doc.layers, true, &mut |surf, _| *surf = translate_surface(surf, dx, dy));
    for ch in doc.channels.iter_mut().chain(doc.quick_mask.as_mut()) {
        ch.surface = translate_surface(&ch.surface, dx, dy);
    }
    if let Some(sel) = &doc.selection {
        doc.selection = Some(translate_surface(sel, dx, dy));
    }
    // Type, shapes, smart objects, vector masks, paths, guides, slices, notes… (caches above
    // are already translated exactly, so nothing needs re-rendering for the move itself).
    crate::canvas_geom::transform_geometry(doc, &photocraft_geom::Affine::translate(f64::from(dx), f64::from(dy)))
}

/// Crops the document to `r` (in current document coordinates).
/// Crops `doc` to `r`, deleting the pixels outside (Perspective Crop's last step).
pub(crate) fn crop_to(doc: &mut Document, r: Rect) -> Result<()> {
    crop_doc(doc, "image.perspectiveCrop", r, true)
}

fn crop_doc(doc: &mut Document, cmd: &str, r: Rect, delete_pixels: bool) -> Result<()> {
    // The origin moves to (0, 0); `-i32::MIN` has no i32 value (upstream 0c2c268c, #959).
    let (Some(dx), Some(dy)) = (r.x0.checked_neg(), r.y0.checked_neg()) else {
        return Err(bad(cmd, format!("the crop origin ({}, {}) can't be moved to (0, 0) within the 32-bit coordinate range", r.x0, r.y0)));
    };
    if delete_pixels {
        for_each_surface(&mut doc.layers, false, &mut |surf, _| *surf = crop_surface(surf, r));
    }
    translate_doc(doc, cmd, dx, dy)?;
    doc.size = Size::new(r.width(), r.height());
    crate::canvas_geom::refresh(doc, crate::canvas_geom::Refresh::Shapes)
}

fn anchor_factors(a: &str) -> (f64, f64) {
    let x = if a.contains("Left") || a == "left" {
        0.0
    } else if a.contains("Right") || a == "right" {
        1.0
    } else {
        0.5
    };
    let y = if a.starts_with("top") {
        0.0
    } else if a.starts_with("bottom") {
        1.0
    } else {
        0.5
    };
    (x, y)
}

fn extension_color(s: &Session, p: &Value) -> [f32; 4] {
    match p.get("extensionColor").and_then(Value::as_str).unwrap_or("background") {
        "foreground" => s.tools.foreground,
        "white" => [1.0; 4],
        "black" => [0.0, 0.0, 0.0, 1.0],
        "transparent" => [0.0; 4],
        hex if hex.starts_with('#') => {
            let h = hex.trim_start_matches('#');
            let c = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok()).map_or(0.0, |v| f32::from(v) / 255.0);
            [c(0), c(2), c(4), 1.0]
        }
        _ => s.tools.background,
    }
}

/// Image → Canvas Size.
fn canvas_size(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let (ow, oh) = (d.doc.size.width as f64, d.doc.size.height as f64);
    let relative = p.get("relative").and_then(Value::as_bool).unwrap_or(false);
    let w = p.get("width").and_then(Value::as_f64).unwrap_or(if relative { 0.0 } else { ow });
    let h = p.get("height").and_then(Value::as_f64).unwrap_or(if relative { 0.0 } else { oh });
    let (nw, nh) = if relative { (ow + w, oh + h) } else { (w, h) };
    let (nw, nh) = (nw.round().clamp(1.0, 300_000.0) as u32, nh.round().clamp(1.0, 300_000.0) as u32);
    let (ax, ay) = anchor_factors(p.get("anchor").and_then(Value::as_str).unwrap_or("center"));
    let dx = ((nw as f64 - ow) * ax).round() as i32;
    let dy = ((nh as f64 - oh) * ay).round() as i32;
    let ext = extension_color(s, p);
    s.edit("Canvas Size", |doc, _| {
        translate_doc(doc, "image.canvasSize", dx, dy)?;
        doc.size = Size::new(nw, nh);
        crate::canvas_geom::refresh(doc, crate::canvas_geom::Refresh::Shapes)?;
        let canvas = doc.bounds();
        let old = Rect::from_xywh(dx, dy, ow as u32, oh as u32);
        // The locked Background layer is extended with the extension colour.
        if ext[3] > 0.0
            && let Some(bg) = doc.layers.first_mut()
            && bg.name == "Background"
            && bg.locks.transparency
            && let LayerContent::Raster(surf) = &mut bg.content
        {
            let fmt = surf.format();
            let px = photocraft_raster::from_rgba(&fmt, ext);
            let keep = surf.content_bounds().intersect(&old);
            let saved = surf.to_interleaved(keep);
            surf.fill_rect(canvas, &px);
            if !keep.is_empty() {
                surf.write_interleaved(keep, &saved);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "width": nw, "height": nh, "offset": [dx, dy] }))
}

/// Image → Crop (to the selection bounds).
/// The largest side an explicit crop may give the canvas (the `file.new` and Canvas Size limit).
const MAX_CROP_SIDE: i32 = 300_000;

fn crop(s: &mut Session, p: &Value) -> Result<Value> {
    let delete = p.get("deleteCroppedPixels").and_then(Value::as_bool).unwrap_or(true);
    // Values beyond i32 would wrap through the narrowing casts into a rectangle unrelated to
    // the numbers passed; reject them (see `commands::int_i32`).
    let explicit = match (
        crate::commands::int_i32("image.crop", p, "x")?,
        crate::commands::int_i32("image.crop", p, "y")?,
        crate::commands::int_i32("image.crop", p, "width")?,
        crate::commands::int_i32("image.crop", p, "height")?,
    ) {
        (Some(x), Some(y), Some(w), Some(h)) if w > 0 && h > 0 => {
            // Upstream fd74419a (#960): the new canvas size: refuse what File › New, Image Size and
            // Canvas Size won't make, before anything changes. Unbounded, a following full-canvas
            // flatten (Trim, Duplicate Merged) aborted on allocation.
            if w > MAX_CROP_SIDE || h > MAX_CROP_SIDE {
                return Err(bad("image.crop", format!("{w} x {h} exceeds the {MAX_CROP_SIDE} px limit per side")));
            }
            // A far edge past i32::MAX used to saturate, silently cropping less than asked (#959).
            let end = |o: i32, len: i32, ko: &str, kl: &str| {
                o.checked_add(len)
                    .ok_or_else(|| bad("image.crop", format!("`{ko}` + `{kl}` = {} is outside the 32-bit coordinate range", i64::from(o) + i64::from(len))))
            };
            Some(Rect::new(x, y, end(x, w, "x", "width")?, end(y, h, "y", "height")?))
        }
        _ => None,
    };
    // An explicit rectangle (the Crop tool) may extend past the canvas; a selection crop is clamped.
    let r = explicit
        .or_else(|| s.active().and_then(|d| d.doc.selection.as_ref().map(|sel| sel.content_bounds().intersect(&d.doc.bounds()))))
        .unwrap_or(Rect::EMPTY);
    if r.is_empty() {
        return Err(EngineError::Other("nothing to crop: pass x/y/width/height or make a selection".into()));
    }
    // local-image: Straighten. `angle` (degrees, clockwise) tilts the frame on the image; the
    // image is rotated the other way about the frame so the frame comes out upright.
    let angle = p.get("angle").and_then(Value::as_f64).unwrap_or(0.0);
    if !angle.is_finite() || angle.abs() > 45.0 {
        return Err(EngineError::BadParams { cmd: "image.crop".into(), msg: "angle must be within ±45°".into() });
    }
    if angle.abs() > 1e-3 {
        let bg = s.tools.background;
        let interp = photocraft_algo::transform::Interp::parse(p.get("interpolation").and_then(Value::as_str).unwrap_or("bicubic"));
        let mut out = r;
        s.edit("Crop", |doc, _| {
            let (_, a) = crate::mode_cmds::rotate_doc(doc, -angle, interp, bg)?;
            let (cx, cy) = ((f64::from(r.x0) + f64::from(r.x1)) / 2.0, (f64::from(r.y0) + f64::from(r.y1)) / 2.0);
            let (nx, ny) = (a.m[0] * cx + a.m[2] * cy + a.m[4], a.m[1] * cx + a.m[3] * cy + a.m[5]);
            let (w, h) = (r.width(), r.height());
            let x0 = (nx - f64::from(w) / 2.0).round() as i32;
            let y0 = (ny - f64::from(h) / 2.0).round() as i32;
            out = Rect::new(x0, y0, x0.saturating_add(w as i32), y0.saturating_add(h as i32));
            crop_doc(doc, "image.crop", out, delete)?;
            doc.selection = None;
            Ok(())
        })?;
        return Ok(json!({ "x": out.x0, "y": out.y0, "width": out.width(), "height": out.height(), "angle": angle }));
    }
    s.edit("Crop", |doc, _| {
        crop_doc(doc, "image.crop", r, delete)?;
        doc.selection = None;
        Ok(())
    })?;
    Ok(json!({ "x": r.x0, "y": r.y0, "width": r.width(), "height": r.height() }))
}

/// Image → Trim.
fn trim(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let canvas = d.doc.bounds();
    let buf = photocraft_compose::flatten(&d.doc);
    let based = p.get("basedOn").and_then(Value::as_str).unwrap_or("transparent");
    let w = canvas.width() as usize;
    let reference = match based {
        "topLeft" => Some(buf.px[0]),
        "bottomRight" => buf.px.last().copied(),
        _ => None,
    };
    let differs = |q: [f32; 4]| match reference {
        None => q[3] > 0.0,
        Some(r) => (0..4).any(|c| (q[c] * q[3] - r[c] * r[3]).abs() > 0.5 / 255.0 || (c == 3 && (q[3] - r[3]).abs() > 0.5 / 255.0)),
    };
    let mut b = Rect::EMPTY;
    for (i, q) in buf.px.iter().enumerate() {
        if differs(*q) {
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            b = if b.is_empty() { Rect::new(x, y, x + 1, y + 1) } else { b.union(&Rect::new(x, y, x + 1, y + 1)) };
        }
    }
    if b.is_empty() {
        return Err(EngineError::Other("nothing to trim to".into()));
    }
    let side = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(true);
    let r = Rect::new(
        if side("left") { b.x0 } else { canvas.x0 },
        if side("top") { b.y0 } else { canvas.y0 },
        if side("right") { b.x1 } else { canvas.x1 },
        if side("bottom") { b.y1 } else { canvas.y1 },
    );
    s.edit("Trim", |doc, _| {
        crop_doc(doc, "image.trim", r, true)?;
        Ok(())
    })?;
    Ok(json!({ "x": r.x0, "y": r.y0, "width": r.width(), "height": r.height() }))
}

/// Image → Mode → colour model, through the CMS (see [`crate::color_cmds::convert_mode`]).
fn convert_mode(s: &mut Session, mode: ColorMode, p: &Value) -> Result<Value> {
    crate::color_cmds::convert_mode(s, mode, p)
}

/// Image → Mode → 8/16/32 Bits/Channel.
fn convert_depth(s: &mut Session, depth: SampleType) -> Result<Value> {
    let current = s.active().ok_or(EngineError::NoDocument)?.doc.depth;
    if current == depth {
        // History::record clears redo even when an edit closure leaves the document unchanged.
        return Ok(Value::Null);
    }
    s.edit("Bit Depth", |doc, _| {
        convert_layers_depth(&mut doc.layers, depth);
        for def in &mut doc.symbols {
            convert_layers_depth(&mut def.layers, depth);
        }
        for ch in doc.channels.iter_mut().chain(doc.quick_mask.as_mut()) {
            let f = ch.surface.format().with_sample(depth);
            ch.surface = ch.surface.convert(f);
        }
        doc.depth = depth;
        Ok(())
    })?;
    Ok(Value::Null)
}

/// Image → Duplicate.
fn duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let mut doc = (*d.doc).clone();
    doc.id = photocraft_doc::DocId::fresh();
    doc.name = p.get("name").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{} copy", d.doc.name));
    if p.get("mergedOnly").and_then(Value::as_bool).unwrap_or(false) {
        let buf = photocraft_compose::flatten(&doc);
        let fmt = doc.pixel_format();
        let mut surf = Surface::new(fmt);
        let vals: Vec<f32> = buf.px.iter().flat_map(|q| photocraft_raster::from_rgba(&fmt, *q)).collect();
        surf.write_region(doc.bounds(), &vals);
        surf.prune();
        doc.layers = vec![Layer::new("Background", LayerContent::Raster(surf))];
        // Variables drove template layers that no longer exist: drop the whole binding set.
        doc.variables = photocraft_doc::Variables::default();
    }
    let mut ids = std::collections::HashMap::new();
    for_each_layer(&mut doc.layers, &mut |l| {
        let fresh = photocraft_doc::LayerId::fresh();
        ids.insert(l.id, fresh);
        l.id = fresh;
    });
    // Variable definitions follow their layers. A target that was already deleted is left as is,
    // so it stays a missing-layer skip rather than driving an unrelated layer.
    for def in &mut doc.variables.defs {
        if let Some(&fresh) = ids.get(&def.layer) {
            def.layer = fresh;
        }
    }
    let name = doc.name.clone();
    let index = s.add_document(doc, None);
    Ok(json!({ "document": index, "name": name }))
}

macro_rules! spec {
    ($id:literal, $label:literal, [$($m:literal),*], $params:literal, $en:expr, $run:expr) => {
        CommandSpec { id: $id, label: $label, menu: &[$($m),*], shortcut: None, params: $params, enabled: $en, run: $run, journal: true }
    };
}

/// Image menu command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "image.imageSize",
            "Image Size…",
            ["Image"],
            r##"{"width":px,"height":px,"resolution":ppi,"resample":"bicubic|bilinear|nearest|lanczos|preserveDetails|none"="bicubic"}"##,
            has_doc,
            image_size
        ),
        spec!(
            "image.canvasSize",
            "Canvas Size…",
            ["Image"],
            r##"{"width":px,"height":px,"relative":bool=false,"anchor":"topLeft|top|topRight|left|center|right|bottomLeft|bottom|bottomRight"="center","extensionColor":"background|foreground|white|black|transparent|#rrggbb"="background"}"##,
            has_doc,
            canvas_size
        ),
        spec!(
            "image.crop",
            "Crop",
            ["Image"],
            r##"{"x":px,"y":px,"width":px,"height":px,"angle":-45..45=0 (straighten: the frame's clockwise tilt on the image),"deleteCroppedPixels":bool=true}"##,
            has_doc,
            crop
        ),
        spec!(
            "image.trim",
            "Trim…",
            ["Image"],
            r##"{"basedOn":"transparent|topLeft|bottomRight"="transparent","top":bool=true,"bottom":bool=true,"left":bool=true,"right":bool=true}"##,
            has_doc,
            trim
        ),
        spec!(
            "image.mode.rgb",
            "RGB Color",
            ["Image", "Mode"],
            r##"{"profile":"<builtin id>|working|/path/to/profile.icc"=working,"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true}"##,
            has_doc,
            |s, p| convert_mode(s, ColorMode::Rgb, p)
        ),
        spec!(
            "image.mode.grayscale",
            "Grayscale",
            ["Image", "Mode"],
            r##"{"profile":"<builtin id>|working|/path/to/profile.icc"=working,"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true}"##,
            has_doc,
            |s, p| convert_mode(s, ColorMode::Grayscale, p)
        ),
        spec!(
            "image.mode.cmyk",
            "CMYK Color",
            ["Image", "Mode"],
            r##"{"profile":"<builtin id>|working|/path/to/profile.icc"=working,"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true}"##,
            has_doc,
            |s, p| convert_mode(s, ColorMode::Cmyk, p)
        ),
        spec!(
            "image.mode.lab",
            "Lab Color",
            ["Image", "Mode"],
            r##"{"profile":"<builtin id>|working|/path/to/profile.icc"=working,"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true}"##,
            has_doc,
            |s, p| convert_mode(s, ColorMode::Lab, p)
        ),
        spec!("image.mode.bits8", "8 Bits/Channel", ["Image", "Mode"], "{}", has_doc, |s, _| convert_depth(s, SampleType::U8)),
        spec!("image.mode.bits16", "16 Bits/Channel", ["Image", "Mode"], "{}", has_doc, |s, _| convert_depth(s, SampleType::U16)),
        spec!("image.mode.bits32", "32 Bits/Channel", ["Image", "Mode"], "{}", has_doc, |s, _| convert_depth(s, SampleType::F32)),
        spec!("image.duplicate", "Duplicate…", ["Image"], r##"{"name":str,"mergedOnly":bool=false}"##, has_doc, duplicate),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::LayerId;

    /// A document with `photo`, `badge` and `title` layers and one variable bound to each.
    fn variables_scene(png: &str) -> (Session, [LayerId; 3]) {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
        let ids = s
            .edit("setup", |doc, _| {
                let fmt = doc.pixel_format();
                let mut photo = Layer::raster("photo", fmt);
                photo.surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 64, 48), &photocraft_raster::from_rgba(&fmt, [0.5, 0.5, 0.5, 1.0]));
                let badge = Layer::raster("badge", fmt);
                let title = Layer::new("title", LayerContent::Text(photocraft_doc::TextLayer { text: "Old".into(), ..Default::default() }));
                let ids = [photo.id, badge.id, title.id];
                doc.layers.extend([photo, badge, title]);
                Ok(ids)
            })
            .unwrap();
        let [photo, badge, title] = ids;
        s.execute(
            "image.variables.define",
            json!({"defs": [
                {"name": "showBadge", "layer": badge.0, "type": "visibility"},
                {"name": "headline", "layer": title.0, "type": "textReplacement"},
                {"name": "photo", "layer": photo.0, "type": "pixelReplacement", "method": "conform"},
            ]}),
        )
        .unwrap();
        s.execute(
            "image.variables.dataSets",
            json!({"dataSets": [{"name": "A", "values": [
                {"variable": "showBadge", "kind": "visibility", "value": false},
                {"variable": "headline", "kind": "text", "value": "Hello"},
                {"variable": "photo", "kind": "pixels", "value": png},
            ]}]}),
        )
        .unwrap();
        (s, ids)
    }

    fn layer_named(s: &Session, name: &str) -> LayerId {
        s.active().unwrap().doc.walk().into_iter().find(|(_, _, l)| l.name == name).unwrap().2.id
    }

    /// Upstream 57d48803 (#4969): a duplicated document's variables drive the copied layers, the
    /// source keeps its own bindings, and a dangling target is not rewritten.
    #[test]
    fn duplicated_documents_keep_variables_targeting_the_copied_layers() {
        let dir = std::env::temp_dir().join(format!("pc-dup-vars-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let red = photocraft_doc::Document::with_background(
            "red",
            photocraft_doc::Size::new(4, 4),
            photocraft_doc::ColorMode::Rgb,
            photocraft_doc::SampleType::U8,
            photocraft_doc::Color::rgb(1.0, 0.0, 0.0),
        );
        let png = dir.join("replacement.png").to_string_lossy().into_owned();
        std::fs::write(&png, photocraft_io::export(&red, "png", &photocraft_io::ExportOptions::default()).unwrap().bytes).unwrap();

        for command in ["image.duplicate", "history.newDocument"] {
            let (mut s, [photo, badge, title]) = variables_scene(&png);
            s.execute(command, json!({})).unwrap();
            let (new_photo, new_badge, new_title) = (layer_named(&s, "photo"), layer_named(&s, "badge"), layer_named(&s, "title"));
            assert_ne!(new_photo, photo, "{command}: layers get new identities");
            let targets: Vec<LayerId> = s.active().unwrap().doc.variables.defs.iter().map(|d| d.layer).collect();
            assert_eq!(targets, vec![new_badge, new_title, new_photo], "{command}: variable defs follow their layers");

            s.execute("image.applyDataSet", json!({"name": "A"})).unwrap();
            let doc = &s.active().unwrap().doc;
            assert!(!doc.layer(new_badge).unwrap().visible, "{command}: visibility drives the copy");
            match &doc.layer(new_title).unwrap().content {
                LayerContent::Text(t) => assert_eq!(t.text, "Hello", "{command}: text drives the copy"),
                _ => panic!("not a text layer"),
            }
            let px = doc.layer(new_photo).unwrap().surface().unwrap().pixel(0, 0);
            assert!(px[0] > 0.9 && px[1] < 0.1, "{command}: pixel replacement drives the copy: {px:?}");

            s.set_active(0);
            let src = &s.active().unwrap().doc;
            assert_eq!(src.variables.defs.iter().map(|d| d.layer).collect::<Vec<_>>(), vec![badge, title, photo], "{command}: source defs untouched");
            assert!(src.layer(badge).unwrap().visible);
        }

        let (mut s, _) = variables_scene(&png);
        let missing = LayerId(u64::MAX);
        s.edit("Break one def", |doc, _| {
            doc.variables.defs.iter_mut().find(|d| d.name == "showBadge").unwrap().layer = missing;
            Ok(())
        })
        .unwrap();
        s.execute("image.duplicate", json!({})).unwrap();
        let defs = &s.active().unwrap().doc.variables.defs;
        assert_eq!(defs.iter().find(|d| d.name == "showBadge").unwrap().layer, missing, "a deleted target is not rewritten");
        assert_eq!(defs.iter().find(|d| d.name == "photo").unwrap().layer, layer_named(&s, "photo"), "live targets still remap");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Upstream 57d48803: a merged-only copy has none of the template layers its variables drove.
    #[test]
    fn merged_only_duplicate_drops_variables_for_removed_layers() {
        let (mut s, _) = variables_scene("unused.png");
        s.execute("image.duplicate", json!({"mergedOnly": true})).unwrap();
        let d = &s.active().unwrap().doc;
        assert_eq!(d.layers.len(), 1);
        assert!(d.variables.defs.is_empty() && d.variables.data_sets.is_empty());
        assert_eq!(d.variables.active, None);
        s.set_active(0);
        assert_eq!(s.active().unwrap().doc.variables.defs.len(), 3, "the original keeps its variables");
    }

    #[test]
    fn straightened_crop_comes_out_upright_and_undoes() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 200, "height": 100})).unwrap();
        let r = s.execute("image.crop", json!({"x": 50, "y": 25, "width": 100, "height": 50, "angle": 10})).unwrap();
        let d = &s.active().unwrap().doc;
        assert_eq!((d.size.width, d.size.height), (100, 50));
        assert_eq!(r["angle"], 10.0);
        s.execute("edit.undo", json!({})).unwrap();
        let d = &s.active().unwrap().doc;
        assert_eq!((d.size.width, d.size.height), (200, 100));
        assert!(s.execute("image.crop", json!({"x": 0, "y": 0, "width": 10, "height": 10, "angle": 60})).is_err());
    }

    #[test]
    fn dsfix_rotated_crop_has_large_positive_center_without_overflow() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width":2,"height":2})).unwrap();
        let out = s.execute("image.crop", json!({"x":1_100_000_000,"y":1_100_000_000,"width":2,"height":2,"angle":1})).unwrap();
        assert!(out["x"].as_i64().unwrap() > 1_000_000_000);
        assert!(out["y"].as_i64().unwrap() > 1_000_000_000);
        assert_eq!(s.active().unwrap().doc.size.width, 2);
    }

    #[test]
    fn crop_rejects_rectangles_that_would_wrap() {
        let mut s = session();
        // 2^32 + 100 wrapped to `x = 100` through the narrowing casts: a crop rectangle
        // somewhere unrelated to the numbers passed.
        let err = s.execute("image.crop", json!({"x": 4_294_967_396_i64, "y": 0, "width": 10, "height": 10})).unwrap_err();
        assert!(err.to_string().contains("32-bit"), "{err}");
        // In-range rectangles past the canvas still work (clamped by the crop itself).
        s.execute("image.crop", json!({"x": -10, "y": -10, "width": 1000, "height": 1000})).unwrap();
    }

    /// Upstream 0c2c268c (#959): a crop whose translation to the origin can't be represented in
    /// i32 is rejected before anything changes: `-i32::MIN` overflowed the negation in
    /// `crop_doc`, `x + width` past i32::MAX silently shortened the crop, and an origin one past
    /// i32::MIN moved the layer pixels beyond i32::MAX (a length-mismatch panic).
    #[test]
    fn crop_rejects_origins_that_cannot_move_to_zero() {
        const MIN: i64 = i32::MIN as i64;
        const MAX: i64 = i32::MAX as i64;
        let cases = [
            json!({"x": MIN, "y": 0, "width": 1, "height": 1}),
            json!({"x": 0, "y": MIN, "width": 1, "height": 1}),
            json!({"x": MIN, "y": MIN, "width": 1, "height": 1}),
            json!({"x": MIN, "y": 0, "width": 1, "height": 1, "deleteCroppedPixels": false}),
            json!({"x": MAX - 5, "y": 0, "width": 10, "height": 1}),
            json!({"x": 0, "y": MAX - 5, "width": 1, "height": 10}),
            json!({"x": MIN + 1, "y": 0, "width": 1, "height": 1, "deleteCroppedPixels": false}),
            json!({"x": 0, "y": MIN + 1, "width": 1, "height": 1, "deleteCroppedPixels": false}),
        ];
        for p in cases {
            let mut s = session();
            let before = s.active().unwrap().doc.clone();
            let (rev, steps) = (s.active().unwrap().revision, s.active().unwrap().history.past_len());
            let err = s.execute("image.crop", p.clone()).unwrap_err();
            assert!(matches!(err, EngineError::BadParams { .. }), "{p}: {err}");
            assert!(err.to_string().contains("32-bit"), "{p}: {err}");
            let st = s.active().unwrap();
            assert!(std::sync::Arc::ptr_eq(&st.doc, &before), "{p}: document changed");
            assert_eq!(st.doc.size, Size::new(40, 20), "{p}");
            assert_eq!(st.doc.layers[1].surface().unwrap().content_bounds(), Rect::new(10, 5, 20, 15), "{p}");
            assert_eq!((st.revision, st.history.past_len()), (rev, steps), "{p}: history step recorded");
        }
        // Control: a legal negative origin past the canvas still crops (and extends) the canvas.
        let mut s = session();
        s.execute("image.crop", json!({"x": -10, "y": -5, "width": 60, "height": 30})).unwrap();
        assert_eq!(doc(&s).size, Size::new(60, 30));
        assert_eq!(doc(&s).layers[1].surface().unwrap().pixel(20, 10), vec![1.0, 0.0, 0.0, 1.0]);
        // A far-off crop that deletes the (non-overlapping) pixels has nothing left to move.
        let mut s = session();
        s.execute("image.crop", json!({"x": MIN + 1, "y": 0, "width": 1, "height": 1})).unwrap();
        assert_eq!(doc(&s).size, Size::new(1, 1));
    }

    #[test]
    fn crop_refuses_a_canvas_side_past_the_new_document_limit() {
        // Upstream fd74419a (#960): an unbounded explicit crop left a canvas that a later Trim or
        // Duplicate Merged could only abort on allocating.
        for (w, h) in [(100_000_000, 100_000_000), (300_001, 10), (10, 300_001)] {
            let mut s = session();
            let (size, past) = (doc(&s).size, s.active().unwrap().history.past_len());
            let err = s.execute("image.crop", json!({"x": 0, "y": 0, "width": w, "height": h})).unwrap_err();
            assert!(err.to_string().contains("300000 px limit"), "{w} x {h}: {err}");
            assert_eq!((doc(&s).size, s.active().unwrap().history.past_len()), (size, past), "{w} x {h}: nothing changed");
        }
        // At the limit it still extends the canvas (the tiles stay lazy).
        let mut s = session();
        s.execute("image.crop", json!({"x": 0, "y": 0, "width": 300_000, "height": 2})).unwrap();
        assert_eq!(doc(&s).size, Size::new(300_000, 2));
    }

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 40, "height": 20})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.edit("paint", |doc, active| {
            let l = doc.layer_mut(active.unwrap()).unwrap();
            l.surface_mut().unwrap().fill_rect(Rect::new(10, 5, 20, 15), &[1.0, 0.0, 0.0, 1.0]);
            l.mask = Some(photocraft_doc::LayerMask::reveal_all());
            l.effects.items.push(Effect::default_drop_shadow());
            Ok(())
        })
        .unwrap();
        s
    }

    fn doc(s: &Session) -> &Document {
        &s.active().unwrap().doc
    }

    #[test]
    fn image_size_scales_everything() {
        for resample in ["bicubic", "bilinear", "nearest", "lanczos", "preserveDetails"] {
            let mut s = session();
            s.execute("image.imageSize", json!({"width": 80, "resample": resample})).unwrap();
            let d = doc(&s);
            assert_eq!(d.size, Size::new(80, 40), "{resample}");
            let l = &d.layers[1];
            let b = l.surface().unwrap().content_bounds();
            assert!((b.x0 - 20).abs() <= 1 && (b.x1 - 40).abs() <= 1, "{resample} {b:?}");
            let Effect::DropShadow(sh) = &l.effects.items[0] else { panic!() };
            assert_eq!(sh.distance, 10.0);
            assert_eq!(l.mask.as_ref().unwrap().surface.default_pixel(), vec![1.0]);
        }
    }

    /// The canvas border stays opaque after Image Size (it used to fade into transparency, so a
    /// Background or a 200 % export got a translucent frame), and a full selection stays full.
    #[test]
    fn image_size_keeps_canvas_edges_opaque() {
        for depth in [8, 16, 32] {
            for (w, resample) in [(60, "bicubic"), (41, "lanczos"), (15, "bilinear"), (77, "preserveDetails")] {
                let mut s = Session::new();
                s.execute("file.new", json!({"width": 30, "height": 20, "depth": depth, "background": "#336699"})).unwrap();
                s.execute("select.all", json!({})).unwrap();
                s.execute("image.imageSize", json!({"width": w, "resample": resample})).unwrap();
                let d = doc(&s);
                let flat = photocraft_compose::flatten(d);
                let (w, h) = (flat.rect.width() as usize, flat.rect.height() as usize);
                let mid = flat.px[(h / 2) * w + w / 2];
                assert!((mid[3] - 1.0).abs() < 1e-3, "{depth} {resample}: {mid:?}");
                for p in &flat.px {
                    let off = p.iter().zip(mid).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
                    assert!(off < 0.01, "{depth} {resample} {w}: {p:?} vs {mid:?}");
                }
                let sel = d.selection.as_ref().unwrap();
                let (x1, y1) = (d.size.width as i32 - 1, d.size.height as i32 - 1);
                for (x, y) in [(0, 0), (x1, 0), (0, y1), (x1, y1)] {
                    assert!(sel.pixel(x, y)[0] > 0.99, "{depth} {resample}: selection at ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn resolution_only_change() {
        let mut s = session();
        s.execute("image.imageSize", json!({"resolution": 300, "resample": "none", "width": 999})).unwrap();
        assert_eq!(doc(&s).size, Size::new(40, 20));
        assert_eq!(doc(&s).resolution_dpi, 300.0);
    }

    #[test]
    fn canvas_size_anchors_and_extends_background() {
        let mut s = session();
        s.execute("image.canvasSize", json!({"width": 60, "height": 30, "anchor": "topLeft", "extensionColor": "#00ff00"})).unwrap();
        let d = doc(&s);
        assert_eq!(d.size, Size::new(60, 30));
        assert_eq!(d.layers[1].surface().unwrap().content_bounds(), Rect::new(10, 5, 20, 15));
        let bg = d.layers[0].surface().unwrap();
        assert_eq!(bg.pixel(50, 25), vec![0.0, 1.0, 0.0, 1.0]);
        assert_eq!(bg.pixel(5, 5), vec![1.0, 1.0, 1.0, 1.0]);
        let mut s = session();
        s.execute("image.canvasSize", json!({"width": 20, "height": 10, "relative": true, "anchor": "center"})).unwrap();
        assert_eq!(doc(&s).layers[1].surface().unwrap().content_bounds(), Rect::new(20, 10, 30, 20));
    }

    #[test]
    fn crop_to_selection_and_trim() {
        let mut s = session();
        assert!(s.execute("image.crop", json!({})).is_err(), "no selection, no rect");
        s.execute("select.rect", json!({"x": 5, "y": 2, "width": 20, "height": 10})).unwrap();
        s.execute("image.crop", json!({})).unwrap();
        let d = doc(&s);
        assert_eq!(d.size, Size::new(20, 10));
        assert!(d.selection.is_none());
        assert_eq!(d.layers[1].surface().unwrap().content_bounds(), Rect::new(5, 3, 15, 10));
        // Trim transparent: hide the background, trim to the red square.
        let mut s = session();
        s.execute("layer.setProps", json!({"layer": doc(&s).layers[0].id.0, "visible": false})).unwrap();
        s.execute("image.trim", json!({"basedOn": "transparent"})).unwrap();
        // The red square (10×10) plus its drop shadow (distance 5, size 5).
        let size = doc(&s).size;
        assert!(size.width > 10 && size.width < 30 && size.height > 10 && size.height < 25, "{size:?}");
    }

    #[test]
    fn crop_to_explicit_rect_can_extend_canvas() {
        let mut s = session();
        s.execute("image.crop", json!({"x": 10, "y": 5, "width": 10, "height": 10})).unwrap();
        assert_eq!(doc(&s).size, Size::new(10, 10));
        assert_eq!(doc(&s).layers[1].surface().unwrap().pixel(0, 0), vec![1.0, 0.0, 0.0, 1.0]);
        let mut s = session();
        s.execute("image.crop", json!({"x": -10, "y": 0, "width": 60, "height": 20})).unwrap();
        assert_eq!(doc(&s).size, Size::new(60, 20));
        assert_eq!(doc(&s).layers[1].surface().unwrap().pixel(20, 5), vec![1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn trim_based_on_top_left_colour() {
        let mut s = session();
        s.edit("no fx", |doc, _| {
            doc.layers[1].effects.items.clear();
            Ok(())
        })
        .unwrap();
        s.execute("image.trim", json!({"basedOn": "topLeft"})).unwrap();
        assert_eq!(doc(&s).size, Size::new(10, 10));
    }

    #[test]
    fn mode_and_depth_conversions() {
        let mut s = session();
        s.execute("image.mode.grayscale", json!({})).unwrap();
        assert_eq!(doc(&s).mode, ColorMode::Grayscale);
        assert_eq!(doc(&s).layers[1].surface().unwrap().format().mode, ColorMode::Grayscale);
        s.execute("image.mode.bits16", json!({})).unwrap();
        assert_eq!(doc(&s).depth, SampleType::U16);
        assert_eq!(doc(&s).layers[1].surface().unwrap().format().sample, SampleType::U16);
        assert_eq!(doc(&s).layers[1].mask.as_ref().unwrap().surface.format().sample, SampleType::U16);
        s.execute("image.mode.cmyk", json!({})).unwrap();
        s.execute("image.mode.bits32", json!({})).unwrap();
        s.execute("image.mode.lab", json!({})).unwrap();
        s.execute("image.mode.rgb", json!({})).unwrap();
        s.execute("image.mode.bits8", json!({})).unwrap();
        let d = doc(&s);
        assert_eq!((d.mode, d.depth), (ColorMode::Rgb, SampleType::U8));
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(doc(&s).depth, SampleType::F32);
        let revision = s.active().unwrap().revision;
        s.execute("image.mode.bits32", json!({})).unwrap();
        assert_eq!(s.active().unwrap().revision, revision, "an unchanged bit depth must preserve the history branch");
        assert!(s.redo(), "selecting the current depth must not discard the undone conversion");
        assert_eq!(doc(&s).depth, SampleType::U8);
    }

    #[test]
    fn duplicate_document() {
        let mut s = session();
        let r = s.execute("image.duplicate", json!({"mergedOnly": true})).unwrap();
        assert_eq!(r["document"], json!(1));
        assert_eq!(s.documents().len(), 2);
        assert_eq!(doc(&s).layers.len(), 1);
        assert!(doc(&s).name.ends_with("copy"));
    }
}
