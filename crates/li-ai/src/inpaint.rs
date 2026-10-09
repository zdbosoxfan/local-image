//! Inpainting preparation and compositing, done by Local Image so only core ComfyUI nodes are
//! needed. The geometry and mask treatment follow Krita AI Diffusion (`model.py`,
//! `resolution.py`, `workflow.py`; GPL-3.0):
//!
//! - **Mask bounds**: the selection's box padded by its feather (10 % of the diagonal, at least
//!   32 px), 4 px and 6 % of the diagonal.
//! - **Context**: the mask bounds padded by max(longest canvas side / 16, mask average side / 2),
//!   made square and at least 512 px, clamped to the canvas — what the model sees.
//! - **Noise mask**: the selection grown by 4 + feather/2 and blurred by the feather.
//! - **Compositing mask**: the selection eroded by blend/2 and blurred by blend
//!   (blend = min(25, grow + feather/2)), so the result fades into the original.
//! - **Pre-fill** for regenerating from scratch: the masked area is filled with a blur of its
//!   surroundings (normalised convolution), so the model starts from plausible colours;
//!   instruction-edit models get it painted green with "fill the green area" (Krita's convention
//!   for FLUX.2 Klein and Qwen Image 2.1).

use image::{GrayImage, Luma, Rgb, RgbImage, imageops};

use crate::imaging::{Rect, bbox, dilate};

/// The plan for one inpaint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Where the result is composited (canvas pixels).
    pub mask_bounds: Rect,
    /// The crop the model sees (canvas pixels).
    pub context: Rect,
    pub grow: u32,
    pub feather: u32,
    pub blend: u32,
}

fn pad(r: Rect, pad: u32, min: u32, multiple: u32, square: bool, w: u32, h: u32) -> Rect {
    let (rw, rh) = (r.width(), r.height());
    let (mut pw, mut ph) = (pad, pad);
    if square {
        // Less padding on the long side so the result tends to a square.
        if rw > rh {
            pw = (pad / 2).max(pad.saturating_sub((rw - rh) / 2));
        } else {
            ph = (pad / 2).max(pad.saturating_sub((rh - rw) / 2));
        }
    }
    let mult = |v: u32| v.div_ceil(multiple.max(1)) * multiple.max(1);
    let nw = mult((rw + 2 * pw).max(min)).min(w.max(1));
    let nh = mult((rh + 2 * ph).max(min)).min(h.max(1));
    let cx = (r.x0 + r.x1) as i64 / 2;
    let cy = (r.y0 + r.y1) as i64 / 2;
    let x0 = (cx - nw as i64 / 2).clamp(0, (w - nw) as i64) as u32;
    let y0 = (cy - nh as i64 / 2).clamp(0, (h - nh) as i64) as u32;
    Rect { x0, y0, x1: x0 + nw, y1: y0 + nh }
}

/// Plans an inpaint of `mask` (white = regenerate) on a `w × h` canvas. `None` when empty.
pub fn plan(mask: &GrayImage) -> Option<Plan> {
    let (w, h) = mask.dimensions();
    let b = bbox(mask, 0)?;
    let diag = ((b.width() as f32).powi(2) + (b.height() as f32).powi(2)).sqrt();
    let feather = (0.10 * diag).max(32.0).round() as u32;
    let grow = 4 + feather / 2;
    let blend = (grow + feather / 2).min(25);
    let pad_px = feather + 4 + (0.06 * diag).round() as u32;
    let mask_bounds = pad(b, pad_px, 64.min(w).min(h), 16, false, w, h);
    let avg = (mask_bounds.width() + mask_bounds.height()) / 2;
    let ctx_pad = (w.max(h) / 16).max(avg / 2);
    let context = pad(mask_bounds, ctx_pad, 512.min(w).min(h), 8, true, w, h);
    Some(Plan { mask_bounds, context, grow, feather, blend })
}

fn crop_gray(m: &GrayImage, r: Rect) -> GrayImage {
    imageops::crop_imm(m, r.x0, r.y0, r.width(), r.height()).to_image()
}

/// Shrinks a mask by `r` (the complement grown).
pub fn erode(mask: &GrayImage, r: u32) -> GrayImage {
    if r == 0 {
        return mask.clone();
    }
    let inv = GrayImage::from_fn(mask.width(), mask.height(), |x, y| Luma([255 - mask.get_pixel(x, y)[0]]));
    let grown = dilate(&inv, r);
    GrayImage::from_fn(mask.width(), mask.height(), |x, y| Luma([255 - grown.get_pixel(x, y)[0]]))
}

fn blur_gray(m: &GrayImage, sigma: f32) -> GrayImage {
    if sigma < 0.5 { m.clone() } else { imageops::blur(m, sigma) }
}

/// The noise mask over the context crop: grown and feathered.
pub fn noise_mask(mask: &GrayImage, p: &Plan) -> GrayImage {
    let c = crop_gray(mask, p.context);
    let hard = GrayImage::from_fn(c.width(), c.height(), |x, y| Luma([if c.get_pixel(x, y)[0] > 0 { 255 } else { 0 }]));
    blur_gray(&dilate(&hard, p.grow), p.feather as f32 / 2.0)
}

/// The compositing mask over the whole canvas: eroded and blurred, zero outside the mask bounds.
pub fn compositing_mask(mask: &GrayImage, p: &Plan) -> GrayImage {
    let (w, h) = mask.dimensions();
    let hard = GrayImage::from_fn(w, h, |x, y| {
        let inside = x >= p.mask_bounds.x0 && x < p.mask_bounds.x1 && y >= p.mask_bounds.y0 && y < p.mask_bounds.y1;
        Luma([if inside && mask.get_pixel(x, y)[0] > 0 { 255 } else { 0 }])
    });
    // Grow first so the faded edge still covers the whole selection.
    let grown = dilate(&hard, p.grow);
    let m = blur_gray(&erode(&grown, p.blend / 2), p.blend as f32 / 2.0);
    GrayImage::from_fn(w, h, |x, y| {
        let inside = x >= p.mask_bounds.x0 && x < p.mask_bounds.x1 && y >= p.mask_bounds.y0 && y < p.mask_bounds.y1;
        Luma([if inside { m.get_pixel(x, y)[0] } else { 0 }])
    })
}

/// Fills the masked area of `img` with a blur of what surrounds it (normalised convolution).
pub fn blur_fill(img: &RgbImage, mask: &GrayImage, sigma: f32) -> RgbImage {
    let (w, h) = img.dimensions();
    let keep = GrayImage::from_fn(w, h, |x, y| Luma([255 - mask.get_pixel(x, y)[0]]));
    let weighted = RgbImage::from_fn(w, h, |x, y| {
        let k = keep.get_pixel(x, y)[0] as u32;
        let p = img.get_pixel(x, y);
        Rgb([0, 1, 2].map(|c| ((p[c] as u32 * k) / 255) as u8))
    });
    let num = imageops::blur(&weighted, sigma);
    let den = imageops::blur(&keep, sigma);
    RgbImage::from_fn(w, h, |x, y| {
        let m = mask.get_pixel(x, y)[0] as f32 / 255.0;
        let d = den.get_pixel(x, y)[0] as f32 / 255.0;
        let o = img.get_pixel(x, y);
        let n = num.get_pixel(x, y);
        Rgb([0, 1, 2].map(|c| {
            let fill = if d > 0.01 { (n[c] as f32 / d).min(255.0) } else { 128.0 };
            (o[c] as f32 * (1.0 - m) + fill * m).round() as u8
        }))
    })
}

/// Paints the masked area pure green (instruction-edit models are told to fill it).
pub fn green_fill(img: &RgbImage, mask: &GrayImage) -> RgbImage {
    RgbImage::from_fn(img.width(), img.height(), |x, y| if mask.get_pixel(x, y)[0] >= 128 { Rgb([0, 255, 0]) } else { *img.get_pixel(x, y) })
}

/// The instruction prefix for green-fill inpainting.
pub const GREEN_INSTRUCTION: &str = "Fill the green area so it fits the rest of the image. Remove all green.";

/// Composites `result` (the context crop, any size) back onto `canvas` through `comp`.
pub fn composite(canvas: &RgbImage, result: &RgbImage, p: &Plan, comp: &GrayImage) -> RgbImage {
    let r = if result.dimensions() != (p.context.width(), p.context.height()) {
        imageops::resize(result, p.context.width(), p.context.height(), imageops::FilterType::Lanczos3)
    } else {
        result.clone()
    };
    let mut out = canvas.clone();
    for y in p.context.y0..p.context.y1 {
        for x in p.context.x0..p.context.x1 {
            let a = comp.get_pixel(x, y)[0] as u32;
            if a == 0 {
                continue;
            }
            let (o, g) = (canvas.get_pixel(x, y), r.get_pixel(x - p.context.x0, y - p.context.y0));
            out.put_pixel(x, y, Rgb([0, 1, 2].map(|c| ((o[c] as u32 * (255 - a) + g[c] as u32 * a + 127) / 255) as u8)));
        }
    }
    out
}

/// The size to run a `w × h` crop at: about `native²` pixels, at most `max` per side, snapped
/// to `multiple`.
pub fn work_size(w: u32, h: u32, native: u32, max: u32, multiple: u32) -> (u32, u32) {
    let target = native as f64 * native as f64;
    let k = (target / (w as f64 * h as f64)).sqrt();
    let k = k.min(max as f64 / w.max(h) as f64);
    let m = multiple.max(8);
    let snap = |v: f64| (((v / m as f64).round() as u32).max(1)) * m;
    (snap(w as f64 * k), snap(h as f64 * k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(w: u32, h: u32, cx: u32, cy: u32, r: u32) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| {
            let d = ((x as i64 - cx as i64).pow(2) + (y as i64 - cy as i64).pow(2)) as f64;
            Luma([if d.sqrt() <= r as f64 { 255 } else { 0 }])
        })
    }

    #[test]
    fn plan_pads_mask_bounds_and_context() {
        let m = disc(2000, 1500, 1000, 700, 60);
        let p = plan(&m).unwrap();
        assert!(p.mask_bounds.x0 < 940 && p.mask_bounds.x1 > 1060);
        assert!(p.context.width() >= 512 && p.context.height() >= 512);
        assert!(p.context.x0 <= p.mask_bounds.x0 && p.context.x1 >= p.mask_bounds.x1);
        assert!(p.context.x1 <= 2000 && p.context.y1 <= 1500);
        assert_eq!(p.context.width() % 8, 0);
        assert!(plan(&GrayImage::new(10, 10)).is_none());
        // Near an edge the context stays inside the canvas.
        let p = plan(&disc(800, 600, 10, 10, 20)).unwrap();
        assert_eq!((p.context.x0, p.context.y0), (0, 0));
    }

    #[test]
    fn masks_fade_and_composite_stays_inside() {
        let m = disc(600, 600, 300, 300, 50);
        let p = plan(&m).unwrap();
        let nm = noise_mask(&m, &p);
        assert_eq!(nm.dimensions(), (p.context.width(), p.context.height()));
        assert_eq!(nm.get_pixel(300 - p.context.x0, 300 - p.context.y0)[0], 255);
        let cm = compositing_mask(&m, &p);
        assert_eq!(cm.get_pixel(300, 300)[0], 255);
        assert_eq!(cm.get_pixel(0, 0)[0], 0);
        let canvas = RgbImage::from_pixel(600, 600, Rgb([10, 10, 10]));
        let result = RgbImage::from_pixel(p.context.width(), p.context.height(), Rgb([250, 0, 0]));
        let out = composite(&canvas, &result, &p, &cm);
        assert_eq!(out.get_pixel(300, 300)[0], 250);
        assert_eq!(*out.get_pixel(5, 5), Rgb([10, 10, 10]));
    }

    #[test]
    fn blur_fill_uses_the_surroundings() {
        let img = RgbImage::from_fn(100, 100, |x, _| if x < 50 { Rgb([200, 0, 0]) } else { Rgb([0, 0, 200]) });
        let m = disc(100, 100, 50, 50, 10);
        let f = blur_fill(&img, &m, 12.0);
        let c = f.get_pixel(50, 50);
        assert!(c[0] > 40 && c[2] > 40, "{c:?}");
        assert_eq!(*f.get_pixel(5, 5), Rgb([200, 0, 0]));
        assert_eq!(work_size(3000, 2000, 1024, 2048, 16), (1248, 832));
    }
}
