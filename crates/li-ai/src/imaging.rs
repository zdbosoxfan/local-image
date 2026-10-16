//! Pixel work around the AI models: selection cleanup and region planning, the red-outlined input
//! FLUX.2 Klein's removal LoRA expects, seamless (Poisson) blending of generated patches, Qwen
//! reference sizing and cutout alpha cleanup. Plain `image` buffers in, plain buffers out.

use image::{GrayImage, Luma, Rgb, RgbImage, Rgba, RgbaImage, imageops};
use li_raster::{RgbFrame, RgbaFrame};

/// An integer pixel rectangle `[x0, x1) × [y0, y1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Rect {
    pub fn width(&self) -> u32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> u32 {
        self.y1 - self.y0
    }
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
    pub fn expand(&self, by: u32, w: u32, h: u32) -> Rect {
        Rect { x0: self.x0.saturating_sub(by), y0: self.y0.saturating_sub(by), x1: (self.x1 + by).min(w), y1: (self.y1 + by).min(h) }
    }
}

/// Bounding box of pixels above `threshold`.
pub fn bbox(mask: &GrayImage, threshold: u8) -> Option<Rect> {
    let (w, h) = mask.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, p) in mask.enumerate_pixels() {
        if p[0] > threshold {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    (x1 > x0).then_some(Rect { x0, y0, x1, y1 })
}

/// Removes faint stray pixels (value 1) that are not near a real stroke (value ≥ 2): antialiasing
/// dust from brushes that would otherwise inflate crops.
pub fn clean_selection_mask(mask: &GrayImage) -> GrayImage {
    let (w, h) = mask.dimensions();
    if !mask.pixels().any(|p| p[0] >= 2) {
        return mask.clone();
    }
    let strong = GrayImage::from_fn(w, h, |x, y| Luma([if mask.get_pixel(x, y)[0] >= 2 { 255 } else { 0 }]));
    let near = dilate(&strong, 2);
    GrayImage::from_fn(w, h, |x, y| {
        let v = mask.get_pixel(x, y)[0];
        Luma([if v == 1 && near.get_pixel(x, y)[0] == 0 { 0 } else { v }])
    })
}

/// Square max filter of radius `r` (separable).
pub fn dilate(mask: &GrayImage, r: u32) -> GrayImage {
    let (w, h) = mask.dimensions();
    let r = r as i64;
    let mut tmp = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut m = 0;
            for dx in -r..=r {
                let xx = x as i64 + dx;
                if xx >= 0 && xx < w as i64 {
                    m = m.max(mask.get_pixel(xx as u32, y)[0]);
                }
            }
            tmp.put_pixel(x, y, Luma([m]));
        }
    }
    let mut out = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut m = 0;
            for dy in -r..=r {
                let yy = y as i64 + dy;
                if yy >= 0 && yy < h as i64 {
                    m = m.max(tmp.get_pixel(x, yy as u32)[0]);
                }
            }
            out.put_pixel(x, y, Luma([m]));
        }
    }
    out
}

/// Splits a selection into independent regions at wide empty gaps (≥ `min_gap` px) so distant
/// strokes are repaired one at a time with their own context. At most 8 regions, top-to-bottom,
/// then left-to-right.
pub fn plan_regions(mask: &GrayImage, min_gap: u32) -> Vec<Rect> {
    let Some(b) = bbox(mask, 0) else { return Vec::new() };
    let mut out = Vec::new();
    split(mask, b, min_gap, &mut out);
    out.sort_by_key(|r| (r.y0, r.x0));
    out.truncate(8);
    out
}

fn split(mask: &GrayImage, r: Rect, gap: u32, out: &mut Vec<Rect>) {
    if out.len() >= 8 {
        out.push(r);
        return;
    }
    let col_used: Vec<bool> = (r.x0..r.x1).map(|x| (r.y0..r.y1).any(|y| mask.get_pixel(x, y)[0] > 0)).collect();
    let row_used: Vec<bool> = (r.y0..r.y1).map(|y| (r.x0..r.x1).any(|x| mask.get_pixel(x, y)[0] > 0)).collect();
    let widest = |used: &[bool]| -> Option<(usize, usize)> {
        let (mut best, mut start) = (None::<(usize, usize)>, None);
        for (i, u) in used.iter().enumerate() {
            match (u, start) {
                (false, None) => start = Some(i),
                (true, Some(s)) => {
                    if best.is_none_or(|(a, b)| i - s > b - a) {
                        best = Some((s, i));
                    }
                    start = None;
                }
                _ => {}
            }
        }
        best
    };
    let cg = widest(&col_used).filter(|(a, b)| (b - a) as u32 >= gap);
    let rg = widest(&row_used).filter(|(a, b)| (b - a) as u32 >= gap);
    let pick = match (cg, rg) {
        (Some(c), Some(rw)) => Some(if c.1 - c.0 >= rw.1 - rw.0 { (true, c) } else { (false, rw) }),
        (Some(c), None) => Some((true, c)),
        (None, Some(rw)) => Some((false, rw)),
        _ => None,
    };
    let Some((vertical, (a, b))) = pick else {
        out.push(r);
        return;
    };
    let (ra, rb) = if vertical {
        (Rect { x1: r.x0 + a as u32, ..r }, Rect { x0: r.x0 + b as u32, ..r })
    } else {
        (Rect { y1: r.y0 + a as u32, ..r }, Rect { y0: r.y0 + b as u32, ..r })
    };
    for part in [ra, rb] {
        let sub = shrink_to_content(mask, part);
        if let Some(s) = sub {
            split(mask, s, gap, out);
        }
    }
}

fn shrink_to_content(mask: &GrayImage, r: Rect) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (r.x1, r.y1, r.x0, r.y0);
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            if mask.get_pixel(x, y)[0] > 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x1 > x0).then_some(Rect { x0, y0, x1, y1 })
}

/// The context crop around a removal region: at least 768 px, 2.5× the width and 2× the height
/// of the region, centred and clamped to the image.
pub fn klein_crop_box(w: u32, h: u32, region: Rect) -> Rect {
    let cw = w.min(768.max((region.width() as f32 * 2.5).round() as u32));
    let ch = h.min(768.max((region.height() as f32 * 2.0).round() as u32));
    let cx = (region.x0 + region.x1) as i64;
    let cy = (region.y0 + region.y1) as i64;
    let x = ((cx - cw as i64) / 2).clamp(0, (w - cw) as i64) as u32;
    let y = ((cy - ch as i64) / 2).clamp(0, (h - ch) as i64) as u32;
    Rect { x0: x, y0: y, x1: x + cw, y1: y + ch }
}

/// Scales a crop to the removal model's budget (≈1536² px, ≤2048 edge, multiples of 16) and
/// draws a 3 px red rectangle around the object, which stays visible (the LoRA's input format).
pub fn klein_highlight(crop: &RgbImage, region_in_crop: Rect) -> RgbImage {
    let (cw, ch) = crop.dimensions();
    let scale = 1f64.min((1536.0 * 1536.0 / (cw as f64 * ch as f64)).sqrt()).min(2048.0 / cw.max(ch) as f64);
    let tw = 16.max(((cw as f64 * scale / 16.0).round() * 16.0) as u32);
    let th = 16.max(((ch as f64 * scale / 16.0).round() * 16.0) as u32);
    let mut img = li_raster::resize_rgb8(crop, tw, th, li_raster::Filter::Lanczos3);
    let (sx, sy) = (tw as f64 / cw as f64, th as f64 / ch as f64);
    let l = ((region_in_crop.x0 as f64 * sx).round() as i64 - 3).max(0) as u32;
    let t = ((region_in_crop.y0 as f64 * sy).round() as i64 - 3).max(0) as u32;
    let r = ((region_in_crop.x1 as f64 * sx).round() as u32 + 3).min(tw - 1);
    let b = ((region_in_crop.y1 as f64 * sy).round() as u32 + 3).min(th - 1);
    let red = Rgb([255, 0, 0]);
    for k in 0..3u32 {
        for x in l..=r {
            for y in [t.saturating_add(k).min(b), b.saturating_sub(k).max(t)] {
                img.put_pixel(x, y, red);
            }
        }
        for y in t..=b {
            for x in [l.saturating_add(k).min(r), r.saturating_sub(k).max(l)] {
                img.put_pixel(x, y, red);
            }
        }
    }
    img
}

/// Seamless cloning: keeps the generated patch's gradients inside the selection while matching
/// the original at the selection's edge, so the repair has no visible seam or colour shift.
/// Solves Laplace's equation for the correction `c = blended − generated` (membrane
/// interpolation) with a coarse-to-fine Gauss–Seidel solver. Pixels outside the selection are
/// returned untouched.
pub fn poisson_blend(original: &RgbImage, generated: &RgbImage, mask: &GrayImage) -> RgbImage {
    poisson_blend_frame(&RgbFrame::from_rgb8(original), &RgbFrame::from_rgb8(generated), mask).to_rgb8()
}

/// [`poisson_blend`] on normalized frames: the boundary is matched against the unquantized
/// original (a 16-bit or float document), and the result is not rounded.
pub fn poisson_blend_frame(original: &RgbFrame, generated: &RgbFrame, mask: &GrayImage) -> RgbFrame {
    let (w, h) = (original.width as u32, original.height as u32);
    assert_eq!(generated.dimensions(), original.dimensions());
    assert_eq!(mask.dimensions(), (w, h));
    let inside = |x: u32, y: u32| mask.get_pixel(x, y)[0] > 0;
    let Some(b) = bbox(mask, 0) else { return original.clone() };
    if b.width() < 4 || b.height() < 4 {
        return RgbFrame::from_fn(original.width, original.height, |x, y| if inside(x as u32, y as u32) { generated.get(x, y) } else { original.get(x, y) });
    }
    // Work in the bbox grown by one pixel (the Dirichlet boundary ring).
    let r = b.expand(1, w, h);
    let (rw, rh) = (r.width() as usize, r.height() as usize);
    let n = rw * rh;
    let mut unknown = vec![false; n];
    // Boundary values: original − generated outside the selection.
    let mut boundary = vec![[0f32; 3]; n];
    for y in 0..rh {
        for x in 0..rw {
            let (gx, gy) = (r.x0 + x as u32, r.y0 + y as u32);
            let i = y * rw + x;
            let on_edge = gx == 0 || gy == 0 || gx == w - 1 || gy == h - 1;
            if inside(gx, gy) && !on_edge {
                unknown[i] = true;
            } else {
                let o = original.get(gx as usize, gy as usize);
                let g = generated.get(gx as usize, gy as usize);
                boundary[i] = [o[0] - g[0], o[1] - g[1], o[2] - g[2]];
            }
        }
    }
    let field = solve_membrane(rw, rh, &unknown, &boundary);
    let mut out = original.clone();
    for y in 0..rh {
        for x in 0..rw {
            let i = y * rw + x;
            let (gx, gy) = (r.x0 as usize + x, r.y0 as usize + y);
            if !inside(gx as u32, gy as u32) {
                continue;
            }
            let g = generated.get(gx, gy);
            let c = if unknown[i] { field[i] } else { boundary[i] };
            out.set(gx, gy, [0, 1, 2].map(|k| (g[k] + c[k]).clamp(0.0, 1.0)));
        }
    }
    out
}

/// Harmonic interpolation of `boundary` into the `unknown` cells (4-neighbour Laplacian = 0).
fn solve_membrane(w: usize, h: usize, unknown: &[bool], boundary: &[[f32; 3]]) -> Vec<[f32; 3]> {
    // Coarse initial guess: solve at half resolution first when the region is large.
    let mut x = boundary.to_vec();
    if w > 32 && h > 32 {
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let mut cu = vec![false; cw * ch];
        let mut cb = vec![[0f32; 3]; cw * ch];
        let mut cnt = vec![0f32; cw * ch];
        for y in 0..h {
            for xx in 0..w {
                let ci = (y / 2) * cw + xx / 2;
                let i = y * w + xx;
                if unknown[i] {
                    cu[ci] = true;
                } else {
                    for k in 0..3 {
                        cb[ci][k] += boundary[i][k];
                    }
                    cnt[ci] += 1.0;
                }
            }
        }
        for i in 0..cw * ch {
            if cnt[i] > 0.0 {
                for k in 0..3 {
                    cb[i][k] /= cnt[i];
                }
                // A coarse cell with any known fine cell stays a boundary only if fully known.
                if cu[i] && cnt[i] >= 4.0 {
                    cu[i] = false;
                }
            }
        }
        let coarse = solve_membrane(cw, ch, &cu, &cb);
        for y in 0..h {
            for xx in 0..w {
                let i = y * w + xx;
                if unknown[i] {
                    x[i] = coarse[(y / 2) * cw + xx / 2];
                }
            }
        }
    }
    let iterations = 40 + (w.max(h) as f32).log2() as usize * 10;
    let omega = 1.8f32;
    for _ in 0..iterations {
        for y in 0..h {
            for xx in 0..w {
                let i = y * w + xx;
                if !unknown[i] {
                    continue;
                }
                let mut sum = [0f32; 3];
                let mut c = 0f32;
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (xx as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    for k in 0..3 {
                        sum[k] += x[j][k];
                    }
                    c += 1.0;
                }
                if c > 0.0 {
                    for k in 0..3 {
                        let gs = sum[k] / c;
                        x[i][k] += omega * (gs - x[i][k]);
                    }
                }
            }
        }
    }
    x
}

/// Size for a secondary Qwen reference or repair input: at most 4 MP and 4096 px, aligned to 32.
pub fn qwen_canvas_size(w: u32, h: u32) -> (u32, u32) {
    let (wf, hf) = (w as f64, h as f64);
    let scale = 1f64.min((4194304.0 / (wf * hf)).sqrt()).min(4096.0 / wf.max(hf));
    let bound = |v: f64| 32u32.max(((v * scale) as u32) / 32 * 32);
    let (bw, bh) = (bound(wf), bound(hf));
    let fit = (bw as f64 / wf).min(bh as f64 / hf);
    let al = |v: f64, b: u32| b.min(32u32.max(((v * fit / 32.0).round() as u32) * 32));
    (al(wf, bw), al(hf, bh))
}

/// Snaps a generation canvas to the model's grid (16 px; 32 for Qwen with references), within
/// 256..=4096 per edge.
pub fn snap_size(w: u32, h: u32, grid: u32) -> (u32, u32) {
    let s = |v: u32| (((v.clamp(256, 4096) + grid / 2) / grid) * grid).clamp(256, 4096);
    (s(w), s(h))
}

/// Cutout alpha cleanup: alpha ≤1 → 0, ≥254 → 255. Fails when the model returned no usable
/// transparency.
pub fn finish_cutout(img: &RgbaImage) -> anyhow::Result<GrayImage> {
    let (lo, hi) = img.pixels().fold((255u8, 0u8), |(l, h), p| (l.min(p[3]), h.max(p[3])));
    if lo >= 250 || hi <= 5 {
        anyhow::bail!("The model did not return a usable transparent cutout. Try again (a new seed), or describe the subject in the prompt.");
    }
    Ok(GrayImage::from_fn(img.width(), img.height(), |x, y| {
        let a = img.get_pixel(x, y)[3];
        Luma([if a <= 1 {
            0
        } else if a >= 254 {
            255
        } else {
            a
        }])
    }))
}

/// Composite an RGBA result over white (opaque outputs whose invisible pixels may hold junk).
pub fn over_white(img: &RgbaImage) -> RgbaImage {
    RgbaImage::from_fn(img.width(), img.height(), |x, y| {
        let p = img.get_pixel(x, y);
        let a = if p[3] >= 254 { 255 } else { p[3] } as f32 / 255.0;
        let c = |k: usize| (p[k] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
        Rgba([c(0), c(1), c(2), 255])
    })
}

/// Validate the raw alpha of a dedicated empty background before white compositing.
pub fn finish_background(img: &RgbaImage) -> anyhow::Result<RgbaImage> {
    // Permit incidental edge transparency up to 1% missing coverage, before flattening.
    let missing: u64 = img.pixels().map(|p| u64::from(255 - p[3])).sum();
    let pixels = u64::from(img.width()) * u64::from(img.height());
    anyhow::ensure!(
        pixels > 0 && missing <= pixels * 255 / 100,
        "The model returned a transparent subject instead of an empty background. Try again with a new seed or describe the empty scene more clearly."
    );
    Ok(over_white(img))
}

/// Pads `img` into `w × h` keeping its aspect (letterboxed with transparency), as Qwen's first
/// generation reference.
pub fn pad_to(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let scale = (w as f64 / img.width() as f64).min(h as f64 / img.height() as f64);
    let (nw, nh) = (((img.width() as f64 * scale).round() as u32).max(1), ((img.height() as f64 * scale).round() as u32).max(1));
    let r = li_raster::resize_rgba8(img, nw, nh, li_raster::Filter::Lanczos3);
    let mut out = RgbaImage::new(w, h);
    imageops::overlay(&mut out, &r, ((w - nw) / 2) as i64, ((h - nh) / 2) as i64);
    out
}

/// Centre-crops to fill `w × h` (Z-Image starting images).
pub fn fit_cover(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let scale = (w as f64 / img.width() as f64).max(h as f64 / img.height() as f64);
    let (nw, nh) = (((img.width() as f64 * scale).ceil() as u32).max(w), ((img.height() as f64 * scale).ceil() as u32).max(h));
    let r = li_raster::resize_rgba8(img, nw, nh, li_raster::Filter::Lanczos3);
    imageops::crop_imm(&r, (nw - w) / 2, (nh - h) / 2, w, h).to_image()
}

pub fn encode_png<P: image::PixelWithColorType>(img: &image::ImageBuffer<P, Vec<P::Subpixel>>) -> anyhow::Result<Vec<u8>>
where
    [P::Subpixel]: image::EncodableLayout,
{
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

pub fn decode_rgba(bytes: &[u8]) -> anyhow::Result<RgbaImage> {
    Ok(image::load_from_memory(bytes)?.to_rgba8())
}

/// Decode a wire image at its own precision (8-bit, 16-bit or float) as straight RGBA.
pub fn decode_frame(bytes: &[u8]) -> anyhow::Result<RgbaFrame> {
    Ok(RgbaFrame::from_dynamic(&image::load_from_memory(bytes)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn j01_shared_resize_cc0_cutout_preserves_colour_at_transparent_edges() {
        // Original synthetic cutout authored for this regression, dedicated to CC0-1.0.
        // The disc's visible colour is constant, so its exact composite at every alpha
        // is known. Hidden black deliberately reproduces the old dark-fringe failure.
        const COLOUR: [u8; 3] = [224, 176, 96];
        let source = RgbaImage::from_fn(128, 96, |x, y| {
            let distance = ((x as f32 - 63.5).powi(2) + (y as f32 - 47.5).powi(2)).sqrt();
            let alpha = ((35.5 - distance).clamp(0.0, 1.0) * 255.0).round() as u8;
            if alpha == 0 { Rgba([0, 0, 0, 0]) } else { Rgba([COLOUR[0], COLOUR[1], COLOUR[2], alpha]) }
        });
        let fringe_error = |img: &RgbaImage| {
            let white = over_white(img);
            let mut total = 0u64;
            let mut samples = 0u64;
            for (x, y, p) in img.enumerate_pixels() {
                if !(2..254).contains(&p[3]) {
                    continue;
                }
                let alpha = f32::from(p[3]) / 255.0;
                for (c, colour) in COLOUR.iter().enumerate() {
                    let expected = (f32::from(*colour) * alpha + 255.0 * (1.0 - alpha)).round() as u8;
                    total += u64::from(white.get_pixel(x, y)[c].abs_diff(expected));
                    samples += 1;
                }
            }
            assert!(samples > 30, "must sample actual antialiased edges");
            total as f64 / samples as f64
        };
        for (w, h) in [(257, 193), (65, 49)] {
            // These matching aspect ratios round to exactly w × h in pad_to's fit.
            let scale = (f64::from(w) / 128.0).min(f64::from(h) / 96.0);
            let nw = (128.0 * scale).round() as u32;
            let nh = (96.0 * scale).round() as u32;
            let old_resize = imageops::resize(&source, nw, nh, imageops::FilterType::Lanczos3);
            let mut old = RgbaImage::new(w, h);
            imageops::overlay(&mut old, &old_resize, i64::from((w - nw) / 2), i64::from((h - nh) / 2));
            let new = pad_to(&source, w, h);
            assert_eq!(new.dimensions(), (w, h));
            let before = fringe_error(&old);
            let after = fringe_error(&new);
            eprintln!("CC0 cutout {w}x{h}: white-composite edge MAE {before:.6} -> {after:.6} u8 levels");
            assert!(before > 1.0, "fixture must expose the old fringe: {before}");
            assert!(after < 0.1 && after < before * 0.1, "premultiplied resize must remove the fringe: {before} -> {after}");
        }
    }

    #[test]
    fn regions_split_at_wide_gaps() {
        let mut m = GrayImage::new(400, 100);
        for y in 10..30 {
            for x in 10..40 {
                m.put_pixel(x, y, Luma([255]));
                m.put_pixel(x + 300, y + 50, Luma([255]));
            }
        }
        let r = plan_regions(&m, 64);
        assert_eq!(r.len(), 2, "{r:?}");
        assert_eq!(r[0], Rect { x0: 10, y0: 10, x1: 40, y1: 30 });
        assert_eq!(plan_regions(&m, 400).len(), 1);
    }

    #[test]
    fn crop_box_has_context_and_stays_inside() {
        let b = klein_crop_box(4000, 3000, Rect { x0: 3900, y0: 10, x1: 3990, y1: 50 });
        assert_eq!((b.width(), b.height()), (768, 768));
        assert_eq!(b.x1, 4000);
        assert_eq!(b.y0, 0);
        let small = klein_crop_box(500, 400, Rect { x0: 100, y0: 100, x1: 200, y1: 200 });
        assert_eq!((small.width(), small.height()), (500, 400));
    }

    #[test]
    fn highlight_is_model_sized_and_outlined() {
        let crop = RgbImage::from_pixel(1000, 600, Rgb([10, 200, 10]));
        let h = klein_highlight(&crop, Rect { x0: 400, y0: 200, x1: 600, y1: 400 });
        assert_eq!(h.width() % 16, 0);
        assert_eq!(h.height() % 16, 0);
        let reds = h.pixels().filter(|p| p.0 == [255, 0, 0]).count();
        assert!(reds > 100);
    }

    #[test]
    fn poisson_matches_the_edge_and_keeps_detail() {
        let (w, h) = (64, 64);
        let original = RgbImage::from_pixel(w, h, Rgb([100, 100, 100]));
        // Generated is 40 levels too bright with a feature in the middle.
        let generated =
            RgbImage::from_fn(w, h, |x, y| if (30..34).contains(&x) && (30..34).contains(&y) { Rgb([200, 140, 140]) } else { Rgb([140, 140, 140]) });
        let mut mask = GrayImage::new(w, h);
        for y in 16..48 {
            for x in 16..48 {
                mask.put_pixel(x, y, Luma([255]));
            }
        }
        let out = poisson_blend(&original, &generated, &mask);
        // Flat area inside is pulled to the surroundings' level.
        assert!((out.get_pixel(20, 20)[1] as i32 - 100).abs() <= 3, "{:?}", out.get_pixel(20, 20));
        // The feature's contrast survives.
        assert!(out.get_pixel(31, 31)[0] as i32 - out.get_pixel(20, 31)[0] as i32 > 50);
        // Outside is untouched.
        assert_eq!(out.get_pixel(5, 5), original.get_pixel(5, 5));
    }

    #[test]
    fn qwen_sizes_align() {
        assert_eq!(qwen_canvas_size(1024, 1024), (1024, 1024));
        let (w, h) = qwen_canvas_size(6000, 4000);
        assert!(w % 32 == 0 && h % 32 == 0 && w * h <= 4194304, "{w}x{h}");
        assert_eq!(snap_size(1000, 1500, 16), (1008, 1504));
        assert_eq!(snap_size(10, 9000, 32), (256, 4096));
    }

    #[test]
    fn cutout_alpha_validation() {
        let opaque = RgbaImage::from_pixel(8, 8, Rgba([1, 2, 3, 255]));
        assert!(finish_cutout(&opaque).is_err());
        let mut cut = opaque.clone();
        cut.put_pixel(0, 0, Rgba([0, 0, 0, 1]));
        cut.put_pixel(1, 0, Rgba([0, 0, 0, 128]));
        let a = finish_cutout(&cut).unwrap();
        assert_eq!((a.get_pixel(0, 0)[0], a.get_pixel(1, 0)[0], a.get_pixel(2, 0)[0]), (0, 128, 255));
    }
}

#[cfg(test)]
mod j28_tests {
    use super::*;

    /// The pre-J28 8-bit blend, verbatim: the golden reference for the adapter.
    fn pre_j28_poisson_blend(original: &RgbImage, generated: &RgbImage, mask: &GrayImage) -> RgbImage {
        let (w, h) = original.dimensions();
        let inside = |x: u32, y: u32| mask.get_pixel(x, y)[0] > 0;
        let Some(b) = bbox(mask, 0) else { return original.clone() };
        if b.width() < 4 || b.height() < 4 {
            return RgbImage::from_fn(w, h, |x, y| if inside(x, y) { *generated.get_pixel(x, y) } else { *original.get_pixel(x, y) });
        }
        let r = b.expand(1, w, h);
        let (rw, rh) = (r.width() as usize, r.height() as usize);
        let n = rw * rh;
        let mut unknown = vec![false; n];
        let mut boundary = vec![[0f32; 3]; n];
        for y in 0..rh {
            for x in 0..rw {
                let (gx, gy) = (r.x0 + x as u32, r.y0 + y as u32);
                let i = y * rw + x;
                let on_edge = gx == 0 || gy == 0 || gx == w - 1 || gy == h - 1;
                if inside(gx, gy) && !on_edge {
                    unknown[i] = true;
                } else {
                    let o = original.get_pixel(gx, gy);
                    let g = generated.get_pixel(gx, gy);
                    boundary[i] = [o[0] as f32 - g[0] as f32, o[1] as f32 - g[1] as f32, o[2] as f32 - g[2] as f32];
                }
            }
        }
        let field = solve_membrane(rw, rh, &unknown, &boundary);
        let mut out = original.clone();
        for y in 0..rh {
            for x in 0..rw {
                let i = y * rw + x;
                let (gx, gy) = (r.x0 + x as u32, r.y0 + y as u32);
                if !inside(gx, gy) {
                    continue;
                }
                let g = generated.get_pixel(gx, gy);
                let c = if unknown[i] { field[i] } else { [0.0; 3] };
                out.put_pixel(gx, gy, Rgb([0, 1, 2].map(|k| (g[k] as f32 + if unknown[i] { c[k] } else { boundary[i][k] }).round().clamp(0.0, 255.0) as u8)));
            }
        }
        out
    }

    fn case(w: u32, h: u32) -> (RgbFrame, RgbImage, GrayImage) {
        // A deep gradient (16-bit document) and a generated patch: the same scene, brighter, and
        // 8-bit as everything coming back over the ComfyUI wire.
        let truth = RgbFrame::from_fn(w as usize, h as usize, |x, y| [0.30 + 0.03 * x as f32 / w as f32, 0.50 + 0.02 * y as f32 / h as f32, 0.62]);
        let generated = truth.map(|p| p.map(|v| v + 0.08)).to_rgb8();
        let mask = GrayImage::from_fn(w, h, |x, y| Luma([if (w / 4..3 * w / 4).contains(&x) && (h / 4..3 * h / 4).contains(&y) { 255 } else { 0 }]));
        (truth, generated, mask)
    }

    #[test]
    fn j28_poisson_u8_adapter_matches_the_pre_j28_blend() {
        for (w, h) in [(48, 40), (160, 120), (6, 6)] {
            let (truth, generated, mask) = case(w, h);
            let original = truth.to_rgb8();
            let old = pre_j28_poisson_blend(&original, &generated, &mask);
            let new = poisson_blend(&original, &generated, &mask);
            let diff = old.as_raw().iter().zip(new.as_raw()).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            assert!(diff <= 1, "{w}x{h}: max difference {diff}");
            let differing = old.as_raw().iter().zip(new.as_raw()).filter(|(a, b)| a != b).count();
            eprintln!("j28 poisson {w}x{h}: adapter vs pre-J28 max {diff} level(s), {differing} of {} samples differ", old.as_raw().len());
        }
    }

    #[test]
    fn j28_poisson_float_meets_the_unquantized_document() {
        let (truth, generated, mask) = case(160, 120);
        let old = pre_j28_poisson_blend(&truth.to_rgb8(), &generated, &mask);
        let new = poisson_blend_frame(&truth, &RgbFrame::from_rgb8(&generated), &mask);
        // Seam: step between each selected edge pixel and its unselected neighbour, against the
        // document's own step there.
        let (mut seam_old, mut seam_new) = (0f32, 0f32);
        for y in 30..90usize {
            for (inner, outer) in [(40usize, 39usize), (119, 120)] {
                for k in 0..3 {
                    let want = truth.get(inner, y)[k] - truth.get(outer, y)[k];
                    seam_new = seam_new.max((new.get(inner, y)[k] - truth.get(outer, y)[k] - want).abs());
                    seam_old = seam_old.max((old.get_pixel(inner as u32, y as u32)[k] as f32 / 255.0 - truth.get(outer, y)[k] - want).abs());
                }
            }
        }
        eprintln!("j28 poisson seam vs 16-bit document: before {:.1}, after {:.1} 16-bit levels", seam_old * 65535.0, seam_new * 65535.0);
        assert!(seam_new < seam_old, "float blend must meet the document more closely ({seam_new} vs {seam_old})");
        for (i, m) in mask.as_raw().iter().enumerate() {
            if *m == 0 {
                assert_eq!(new.data[i], truth.data[i], "unselected samples keep every bit");
            }
        }
    }
}
