//! Resize directly from the borrowed input: a 24 MP source never needs an RGB8 copy merely to
//! produce the detector's small tensor. Both resize and alignment retain 16-bit/float precision.

use anyhow::{Result, ensure};

use super::{DETECT_LONG_EDGE, FaceImage};

pub type Affine = [[f32; 3]; 2];
pub const REFERENCE_LANDMARKS: [[f32; 2]; 5] = [[38.2946, 51.6963], [73.5318, 51.5014], [56.0252, 71.7366], [41.5493, 92.3655], [70.7299, 92.2041]];

/// Uniform scale, with zero padding only on the right/bottom (no centring offset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Letterbox {
    pub scale: f32,
    pub resized_w: usize,
    pub resized_h: usize,
    pub pad_w: usize,
    pub pad_h: usize,
}

impl Letterbox {
    pub fn new(width: usize, height: usize) -> Result<Self> {
        ensure!(width > 0 && height > 0, "Letterbox input must have nonzero dimensions");
        let scale = DETECT_LONG_EDGE as f64 / width.max(height) as f64;
        let resized_w = (width as f64 * scale).round().max(1.0) as usize;
        let resized_h = (height as f64 * scale).round().max(1.0) as usize;
        Ok(Self { scale: scale as f32, resized_w, resized_h, pad_w: resized_w.div_ceil(32) * 32, pad_h: resized_h.div_ceil(32) * 32 })
    }

    pub fn to_input(self, point: [f32; 2]) -> [f32; 2] {
        point.map(|v| v * self.scale)
    }

    pub fn to_original(self, point: [f32; 2]) -> [f32; 2] {
        point.map(|v| v / self.scale)
    }
}

/// BGR NCHW f32 in 0..255. Pixel-centre sampling avoids a half-pixel shift on resize.
pub fn letterbox(image: FaceImage<'_>) -> Result<(Vec<f32>, Letterbox)> {
    image.validate()?;
    let layout = Letterbox::new(image.width, image.height)?;
    let plane = layout.pad_w * layout.pad_h;
    let mut data = vec![0.0f32; plane * 3];
    for y in 0..layout.resized_h {
        let fy = ((y as f64 + 0.5) / f64::from(layout.scale) - 0.5).clamp(0.0, (image.height - 1) as f64);
        for x in 0..layout.resized_w {
            let fx = ((x as f64 + 0.5) / f64::from(layout.scale) - 0.5).clamp(0.0, (image.width - 1) as f64);
            for c in 0..3 {
                data[c * plane + y * layout.pad_w + x] = bilinear(image, fx, fy, 2 - c, false);
            }
        }
    }
    Ok((data, layout))
}

/// Proper (no reflection) Umeyama least-squares similarity in 2D. The closed-form covariance
/// solution is equivalent to the SVD formulation, without introducing a linear algebra crate.
/// `src` maps to the ArcFace reference in forward coordinates. Degenerate fits are errors.
pub fn similarity_transform(src: &[[f32; 2]; 5]) -> Result<Affine> {
    ensure!(src.iter().flatten().all(|v| v.is_finite()), "Nonfinite face landmarks");
    let mean = |p: &[[f32; 2]; 5]| std::array::from_fn::<_, 2, _>(|c| p.iter().map(|p| f64::from(p[c])).sum::<f64>() / 5.0);
    let from = mean(src);
    let to = mean(&REFERENCE_LANDMARKS);
    let (mut variance, mut dot, mut cross) = (0.0, 0.0, 0.0);
    for (p, q) in src.iter().zip(REFERENCE_LANDMARKS.iter()) {
        let [x, y] = [f64::from(p[0]) - from[0], f64::from(p[1]) - from[1]];
        let [u, v] = [f64::from(q[0]) - to[0], f64::from(q[1]) - to[1]];
        variance += x * x + y * y;
        dot += x * u + y * v;
        cross += x * v - y * u;
    }
    ensure!(variance > f64::EPSILON, "Degenerate face landmarks");
    let a = dot / variance;
    let b = cross / variance;
    ensure!(a.hypot(b) > f64::EPSILON, "Degenerate face alignment");
    let matrix = [[a as f32, -b as f32, (to[0] - a * from[0] + b * from[1]) as f32], [b as f32, a as f32, (to[1] - b * from[0] - a * from[1]) as f32]];
    ensure!(matrix.iter().flatten().all(|v| v.is_finite()), "Face alignment overflow");
    Ok(matrix)
}

/// Bilinear inverse warp to interleaved RGB 112×112 in 0..255. Samples outside the original
/// image contribute zero, matching OpenCV's constant black border. No full-resolution copy.
pub fn warp_affine_112(image: FaceImage<'_>, matrix: &Affine) -> Result<Vec<f32>> {
    image.validate()?;
    ensure!(matrix.iter().flatten().all(|v| v.is_finite()), "Nonfinite affine matrix");
    let [[a, b, tx], [c, d, ty]] = matrix.map(|row| row.map(f64::from));
    let det = a * d - b * c;
    ensure!(det.is_finite() && det.abs() > f64::EPSILON, "Singular affine matrix");
    let mut rgb = vec![0.0f32; 112 * 112 * 3];
    for y in 0..112 {
        for x in 0..112 {
            let px = x as f64 - tx;
            let py = y as f64 - ty;
            let fx = (d * px - b * py) / det;
            let fy = (a * py - c * px) / det;
            ensure!(fx.is_finite() && fy.is_finite(), "Affine coordinate overflow");
            for channel in 0..3 {
                rgb[(y * 112 + x) * 3 + channel] = bilinear(image, fx, fy, channel, true);
            }
        }
    }
    Ok(rgb)
}

fn bilinear(image: FaceImage<'_>, x: f64, y: f64, channel: usize, black_border: bool) -> f32 {
    if black_border && (x <= -1.0 || y <= -1.0 || x >= image.width as f64 || y >= image.height as f64) {
        return 0.0;
    }
    let x0 = x.floor();
    let y0 = y.floor();
    let tx = (x - x0) as f32;
    let ty = (y - y0) as f32;
    let sample = |sx: f64, sy: f64| {
        if sx < 0.0 || sy < 0.0 || sx >= image.width as f64 || sy >= image.height as f64 {
            if black_border {
                0.0
            } else {
                image.sample(sx.clamp(0.0, (image.width - 1) as f64) as usize, sy.clamp(0.0, (image.height - 1) as f64) as usize, channel)
            }
        } else {
            image.sample(sx as usize, sy as usize, channel)
        }
    };
    let top = sample(x0, y0) * (1.0 - tx) + sample(x0 + 1.0, y0) * tx;
    let bottom = sample(x0, y0 + 1.0) * (1.0 - tx) + sample(x0 + 1.0, y0 + 1.0) * tx;
    top * (1.0 - ty) + bottom * ty
}
