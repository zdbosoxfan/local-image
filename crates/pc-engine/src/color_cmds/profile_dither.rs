//! Opt-in 8-bit profile quantization. The ordinary CMS buffer paths stay unchanged.

use photocraft_geom::{Rect, TILE_SIZE, TileCoord};

use super::{
    ColorMode, Document, EngineError, Intent, Layer, PixelFormat, Profile, Result, SampleType, Surface, Transform, cms_err, conversion_source,
    convert_document, convert_layer_colors, convert_surface, space_mode,
};

const TILE_PIXELS: u128 = (TILE_SIZE * TILE_SIZE) as u128;
// Two per-pixel arrays and the decoded source default; no tile or plane staging.
const SCRATCH_BYTES: u128 = (3 * 16 * std::mem::size_of::<f32>()) as u128;

fn bad(message: &str) -> EngineError {
    EngineError::BadParams { cmd: "edit.convertToProfile".into(), msg: message.into() }
}

/// Dither only layouts the CMS can transform directly, copying alpha without conversion.
fn eligible(s: &Surface, from: ColorMode, to: PixelFormat, t: &Transform) -> bool {
    let sf = s.format();
    sf.sample == SampleType::U8
        && sf.mode == from
        && t.inputs() == sf.mode.color_channels()
        && t.outputs() == to.mode.color_channels()
        && sf.alpha == to.alpha
        && sf.channels() <= 16
        && to.channels() <= 16
}

fn in_canvas_tiles(c: TileCoord, bounds: Rect) -> bool {
    if bounds.is_empty() {
        return false;
    }
    let a = TileCoord::containing(bounds.x0, bounds.y0);
    let b = TileCoord::containing(bounds.x1 - 1, bounds.y1 - 1);
    c.tx >= a.tx && c.tx <= b.tx && c.ty >= a.ty && c.ty <= b.ty
}

/// How many tiles overlap the canvas, counted without visiting them.
fn canvas_tile_count(bounds: Rect) -> u128 {
    if bounds.is_empty() {
        return 0;
    }
    let a = TileCoord::containing(bounds.x0, bounds.y0);
    let b = TileCoord::containing(bounds.x1 - 1, bounds.y1 - 1);
    ((i64::from(b.tx) - i64::from(a.tx) + 1) as u128) * ((i64::from(b.ty) - i64::from(a.ty) + 1) as u128)
}

/// Noise needs real pixel coordinates, not the saturating rectangle of a malformed tile.
fn tile_origin(c: TileCoord) -> Result<(i32, i32)> {
    let x = i64::from(c.tx) * i64::from(TILE_SIZE);
    let y = i64::from(c.ty) * i64::from(TILE_SIZE);
    if x < i64::from(i32::MIN)
        || y < i64::from(i32::MIN)
        || x + i64::from(TILE_SIZE - 1) > i64::from(i32::MAX)
        || y + i64::from(TILE_SIZE - 1) > i64::from(i32::MAX)
    {
        return Err(bad("a stored tile has unaddressable coordinates; disable dithering to use the ordinary conversion"));
    }
    Ok((x as i32, y as i32))
}

/// Price every additional output tile before allocating any; distant tiles add no bounding box.
fn output_bytes(s: &Surface, to: PixelFormat, bounds: Rect) -> Result<u128> {
    let mut count = canvas_tile_count(bounds);
    for (c, _) in s.tiles() {
        tile_origin(*c)?;
        if !in_canvas_tiles(*c, bounds) {
            count = count.checked_add(1).ok_or_else(|| bad("dither output size overflowed"))?;
        }
    }
    count
        .checked_mul(TILE_PIXELS)
        .and_then(|n| n.checked_mul(to.bytes_per_pixel() as u128))
        .and_then(|n| n.checked_add(to.bytes_per_pixel() as u128))
        .ok_or_else(|| bad("dither output size overflowed"))
}

/// Stochastic rounding before quantization preserves integer samples and neutral channels.
fn quantize(value: f32, threshold: f64) -> u8 {
    if !value.is_finite() {
        // Match the ordinary converter's saturating float cast, including NaN -> zero.
        return (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }
    let scaled = f64::from(value.clamp(0.0, 1.0) * 255.0);
    let lower = scaled.floor();
    (lower + f64::from(u8::from(scaled - lower > threshold))) as u8
}

/// Stream the finite canvas and stored deviations, retaining the undithered infinite default.
fn surface(s: &Surface, to: PixelFormat, t: &Transform, bounds: Rect) -> Result<Surface> {
    let (ss, ds) = (s.channels(), to.channels());
    let (ni, no) = (t.inputs(), t.outputs());
    let mut converted_default = [0.0; 16];
    t.convert_f32(&s.default_pixel(), ss, &mut converted_default[..ds], ds, true);
    let mut out = Surface::with_default(to, &converted_default[..ds]);
    let default = s.default_bytes();
    let outside = s.tiles().map(|(c, _)| *c).filter(|c| !in_canvas_tiles(*c, bounds));
    for c in bounds.tiles().chain(outside) {
        let (x0, y0) = tile_origin(c)?;
        let source = s.tile(c).map(|tile| tile.bytes());
        let dest = out.try_tile_mut(c).map_err(|e| EngineError::Other(format!("could not allocate dither output: {e}")))?;
        for (index, dp) in dest.bytes_mut().chunks_exact_mut(ds).enumerate() {
            let x = x0 + (index % TILE_SIZE as usize) as i32;
            let y = y0 + (index / TILE_SIZE as usize) as i32;
            let sp = match source {
                Some(bytes) => bytes.get(index * ss..index * ss + ss).ok_or_else(|| bad("a stored tile has invalid sample storage"))?,
                None => default,
            };
            // A redundant default tile outside the canvas must match the implicit plane.
            if !bounds.contains(x, y) && sp == default {
                continue;
            }
            let mut input = [0.0; 16];
            let mut output = [0.0; 16];
            for (d, s) in input.iter_mut().zip(sp).take(ni) {
                *d = f32::from(*s) / 255.0;
            }
            t.eval_fast(&input[..ni], &mut output[..no]);
            // The hash can return 1.0; recenter its 65536 bins strictly inside (0, 1).
            let threshold = (f64::from(photocraft_color::dither_noise(x, y)) * 65535.0 + 0.5) / 65536.0;
            for (d, value) in dp.iter_mut().zip(output).take(no) {
                *d = quantize(value, threshold);
            }
            for (d, s) in dp.iter_mut().skip(no).zip(sp.iter().skip(ni)) {
                *d = *s;
            }
        }
    }
    Ok(out)
}

/// The document's layers and every symbol master's, the trees `convert_document` converts.
fn layer_trees(doc: &mut Document) -> impl Iterator<Item = &mut Vec<Layer>> {
    std::iter::once(&mut doc.layers).chain(doc.symbols.iter_mut().map(|def| &mut def.layers))
}

/// Preserve ordinary conversion outside opt-in U8, admitting the whole dither edit first.
pub(super) fn convert(doc: &mut Document, dst: &Profile, intent: Intent, bpc: bool) -> Result<()> {
    if doc.depth != SampleType::U8 || doc.mode == ColorMode::Duotone {
        return convert_document(doc, dst, intent, bpc);
    }
    let src = conversion_source(doc)?;
    if src.to_bytes() == dst.to_bytes() {
        return convert_document(doc, dst, intent, bpc);
    }
    let to_mode = space_mode(dst.color_space).ok_or_else(|| EngineError::Other(format!("cannot convert to a {:?} profile", dst.color_space)))?;
    let from = doc.pixel_format().mode;
    let t = Transform::new(&src, dst, intent, bpc).map_err(cms_err)?;
    let format = PixelFormat::new(to_mode, doc.depth, true);
    let bounds = doc.bounds();
    if doc.size.width > i32::MAX as u32 || doc.size.height > i32::MAX as u32 {
        return Err(bad("the canvas has unaddressable coordinates; disable dithering to use the ordinary conversion"));
    }
    let mut bytes = SCRATCH_BYTES;
    let mut admission = Ok(());
    for layers in layer_trees(doc) {
        crate::image_cmds::for_each_surface(layers, false, &mut |s, _| {
            if admission.is_err() {
                return;
            }
            let to = PixelFormat { alpha: s.format().alpha, ..format };
            if eligible(s, from, to, &t) {
                admission = output_bytes(s, to, bounds).and_then(|additional| {
                    bytes = bytes.checked_add(additional).ok_or_else(|| bad("dither output size overflowed"))?;
                    Ok(())
                });
            }
        });
    }
    admission?;
    // This is an output-raster cap, not process peak: the source and history remain retained.
    if bytes > u128::from(crate::image_cmds::MAX_RESAMPLE_BYTES) {
        return Err(bad("dither output exceeds the raster allocation budget; use a smaller canvas or disable dithering"));
    }
    let mut result = Ok(());
    for layers in layer_trees(doc) {
        crate::image_cmds::for_each_surface(layers, false, &mut |s, _| {
            if result.is_err() {
                return;
            }
            let to = PixelFormat { alpha: s.format().alpha, ..format };
            if eligible(s, from, to, &t) {
                result = surface(s, to, &t, bounds).map(|converted| *s = converted);
            } else {
                *s = convert_surface(s, from, to, &t);
            }
        });
        for layer in layers.iter_mut() {
            convert_layer_colors(layer, from, to_mode, &t);
        }
    }
    result?;
    doc.mode = to_mode;
    doc.icc_profile = Some(dst.to_bytes());
    // Leaving Indexed: the palette no longer applies.
    doc.color_table = None;
    doc.duotone = None;
    Ok(())
}
