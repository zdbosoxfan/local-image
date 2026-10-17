use super::*;

/// Inter SemiBold (OFL, see assets/ATTRIBUTION-lightcraft.md).
static WATERMARK_FONT: &[u8] = include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf");

/// The watermark faces: Inter first, then the craft-fonts CJK faces (Mincho first, the watermark's
/// serif look; then any other CJK face — a watermark has no UI language, so every CJK script is
/// offered). Without craft-fonts that is Inter alone, and CJK characters draw as Inter's
/// missing-glyph box.
pub(super) fn watermark_fonts(craft: &'static [crate::fonts::CraftFont]) -> Vec<ab_glyph::FontRef<'static>> {
    let mut cjk: Vec<_> = crate::fonts::cjk(craft).collect();
    cjk.sort_by_key(|f| !f.is_mincho());
    std::iter::once(WATERMARK_FONT)
        .chain(cjk.into_iter().map(|f| f.bytes))
        .filter_map(|bytes| ab_glyph::FontRef::try_from_slice(bytes).ok())
        .collect()
}

/// Draw `wm` onto `img` (straight alpha blending of the encoded values).
pub fn draw_watermark(img: &mut Rgba8, wm: &Watermark) {
    let width = img.width;
    watermark_coverage(img.width, img.height, wm, crate::fonts::CRAFT_FONTS, |x, y, k, col| {
        let p = &mut img.data[y * width + x];
        for c in 0..3 {
            p[c] = (p[c] as f32 + (col[c] as f32 - p[c] as f32) * k).round() as u8;
        }
    });
}

/// Draw `wm` onto a high-bit-depth image (`wm.color` is in the image's space, 8-bit encoded; float
/// images are linear and blend in linear light).
pub fn draw_watermark_deep(img: &mut DeepImage, wm: &Watermark) {
    let (width, trc) = (img.width, img.space.trc());
    match &mut img.samples {
        DeepSamples::U16(v) => watermark_coverage(img.width, img.height, wm, crate::fonts::CRAFT_FONTS, |x, y, k, col| {
            let i = (y * width + x) * 3;
            for c in 0..3 {
                let p = v[i + c] as f32;
                v[i + c] = (p + (col[c] as f32 * 257.0 - p) * k).round().clamp(0.0, 65535.0) as u16;
            }
        }),
        DeepSamples::F32(v) => watermark_coverage(img.width, img.height, wm, crate::fonts::CRAFT_FONTS, |x, y, k, col| {
            let i = (y * width + x) * 3;
            for c in 0..3 {
                let target = trc.decode(col[c] as f32 / 255.0);
                v[i + c] += (target - v[i + c]) * k;
            }
        }),
    }
}

/// Lay out `wm` on a `width × height` image and call `blend(x, y, coverage × opacity, colour)` for
/// every covered pixel (shadow pass first). `craft` is [`crate::fonts::CRAFT_FONTS`] (a parameter
/// so tests can render without it).
pub(super) fn watermark_coverage(
    width: usize,
    height: usize,
    wm: &Watermark,
    craft: &'static [crate::fonts::CraftFont],
    mut blend_px: impl FnMut(usize, usize, f32, [u8; 3]),
) {
    use ab_glyph::{Font, PxScale, ScaleFont, point};
    if !wm.image.trim().is_empty() {
        logo_coverage(width, height, wm, blend_px);
        return;
    }
    let text = wm.text.trim();
    if text.is_empty() || width == 0 || height == 0 {
        return;
    }
    let fonts = watermark_fonts(craft);
    let Some(latin) = fonts.first() else { return };
    let short = width.min(height) as f32;
    let px = (wm.size.clamp(0.005, 0.5) * short).max(6.0);
    let mut glyphs = Vec::new();
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut tw = 0.0f32;
    let mut prev = None;
    let columns = text.split('\n').count();
    let mut column = 0usize;
    for original in text.chars() {
        if original == '\n' {
            if wm.vertical {
                column = column.saturating_add(1);
                tw = tw.max(y);
                y = 0.0;
            } else {
                tw = tw.max(x);
                x = 0.0;
                y += px;
            }
            prev = None;
            continue;
        }
        let vertical_form = match original {
            '、' => '︑',
            '。' => '︒',
            '（' => '︵',
            '）' => '︶',
            '「' => '﹁',
            '」' => '﹂',
            '『' => '﹃',
            '』' => '﹄',
            '【' => '︻',
            '】' => '︼',
            '…' => '︙',
            other => other,
        };
        let has = |font: &ab_glyph::FontRef<'_>, c: char| font.glyph_id(c).0 != 0;
        let ch = if wm.vertical && fonts.iter().skip(1).any(|f| has(f, vertical_form)) { vertical_form } else { original };
        // The first face with the glyph; Inter (its missing-glyph box) when none has it.
        let face = fonts.iter().position(|f| has(f, ch)).unwrap_or(0);
        let sf = fonts.get(face).unwrap_or(latin).as_scaled(PxScale::from(px));
        let id = sf.glyph_id(ch);
        if wm.vertical {
            let left = columns.saturating_sub(column.saturating_add(1)) as f32 * px;
            let pen = point(left + (px - sf.h_advance(id)) / 2.0, y + sf.ascent());
            let rotation = matches!(original, 'ー' | '—' | '–').then_some((left + px / 2.0, y + px / 2.0));
            glyphs.push((face, id.with_scale_and_position(px, pen), rotation));
            y += px;
        } else {
            if let Some((previous_face, previous)) = prev
                && previous_face == face
            {
                x += sf.kern(previous, id);
            }
            glyphs.push((face, id.with_scale_and_position(px, point(x, y + sf.ascent())), None));
            x += sf.h_advance(id);
            prev = Some((face, id));
        }
    }
    let (tw, th) = if wm.vertical { (columns as f32 * px, tw.max(y)) } else { (tw.max(x), y + px) };
    let inset = wm.inset.clamp(0.0, 0.4) * short;
    let (w, h) = (width as f32, height as f32);
    use Anchor::*;
    let ox = match wm.anchor {
        TopLeft | Left | BottomLeft => inset,
        Top | Center | Bottom => (w - tw) / 2.0,
        TopRight | Right | BottomRight => w - inset - tw,
    };
    let oy = match wm.anchor {
        TopLeft | Top | TopRight => inset,
        Left | Center | Right => (h - th) / 2.0,
        BottomLeft | Bottom | BottomRight => h - inset - th,
    };
    let alpha = wm.opacity.clamp(0.0, 1.0);
    let mut blend = |gx: i32, gy: i32, cov: f32, col: [u8; 3], a: f32| {
        if gx < 0 || gy < 0 || gx >= width as i32 || gy >= height as i32 {
            return;
        }
        blend_px(gx as usize, gy as usize, (cov * a).clamp(0.0, 1.0), col);
    };
    let passes: &[(f32, [u8; 3], f32)] =
        if wm.shadow { &[((px * 0.05).max(1.0), [0, 0, 0], 0.45), (0.0, wm.color, 1.0)] } else { &[(0.0, wm.color, 1.0)] };
    for &(off, col, a) in passes {
        for (face, g, rotation) in &glyphs {
            if let Some(o) = fonts.get(*face).and_then(|font| font.outline_glyph(g.clone())) {
                let b = o.px_bounds();
                o.draw(|gx, gy, cov| {
                    let (sx, sy) = (b.min.x + gx as f32, b.min.y + gy as f32);
                    let (sx, sy) = match rotation {
                        Some((cx, cy)) => (cx - (sy - cy), cy + (sx - cx)),
                        None => (sx, sy),
                    };
                    blend((ox + off + sx) as i32, (oy + off + sy) as i32, cov, col, a * alpha);
                });
            }
        }
    }
}

/// A watermark graphic, decoded once per file (and again when the file changes).
fn watermark_logo(path: &str) -> Option<std::sync::Arc<Rgba8>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    static CACHE: Mutex<Option<HashMap<String, (std::time::SystemTime, Arc<Rgba8>)>>> = Mutex::new(None);
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = c.get_or_insert_with(HashMap::new);
    if let Some((m, img)) = map.get(path)
        && *m == modified
    {
        return Some(img.clone());
    }
    let bytes = std::fs::read(path).ok()?;
    let img = Arc::new(lightcraft_codecs::decode(&bytes, Default::default()).ok()?.to_srgb8());
    map.insert(path.to_string(), (modified, img.clone()));
    Some(img)
}

/// The graphic watermark: scaled (premultiplied, so transparent edges stay clean) to
/// `image_width` of the photo, placed like the text, blended with its alpha × opacity.
fn logo_coverage(width: usize, height: usize, wm: &Watermark, mut blend_px: impl FnMut(usize, usize, f32, [u8; 3])) {
    let Some(logo) = watermark_logo(wm.image.trim()) else {
        log::warn!("watermark: can't read {}", wm.image);
        return;
    };
    if width == 0 || height == 0 || logo.width == 0 || logo.height == 0 {
        return;
    }
    let lw = ((wm.image_width.clamp(0.01, 1.0) * width as f32).round() as usize).clamp(1, width);
    let lh = ((lw as f32 * logo.height as f32 / logo.width as f32).round() as usize).clamp(1, height);
    let pre: li_raster::Frame<[f32; 4]> = li_raster::Frame::from_fn(logo.width, logo.height, |x, y| {
        let p = logo.get(x, y);
        let a = p[3] as f32 / 255.0;
        [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, a]
    });
    let scaled = pre.resize(lw, lh, li_raster::resample::Filter::Mitchell);
    let inset = wm.inset.clamp(0.0, 0.4) * width.min(height) as f32;
    let (w, h, lwf, lhf) = (width as f32, height as f32, lw as f32, lh as f32);
    use Anchor::*;
    let ox = match wm.anchor {
        TopLeft | Left | BottomLeft => inset,
        Top | Center | Bottom => (w - lwf) / 2.0,
        TopRight | Right | BottomRight => w - inset - lwf,
    }
    .max(0.0) as usize;
    let oy = match wm.anchor {
        TopLeft | Top | TopRight => inset,
        Left | Center | Right => (h - lhf) / 2.0,
        BottomLeft | Bottom | BottomRight => h - inset - lhf,
    }
    .max(0.0) as usize;
    let space = wm.target.unwrap_or(OutputSpace::Srgb);
    let op = wm.opacity.clamp(0.0, 1.0);
    for y in 0..lh {
        for x in 0..lw {
            let [r, g, b, a] = scaled.get(x, y);
            let a = a.clamp(0.0, 1.0);
            let (px, py) = (ox + x, oy + y);
            if a <= 1e-4 || px >= width || py >= height {
                continue;
            }
            let c = [r, g, b].map(|v| (v / a).round().clamp(0.0, 255.0) as u8);
            blend_px(px, py, a * op, srgb8_in(space, c));
        }
    }
}
