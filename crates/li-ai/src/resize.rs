//! Parallel RGB(A)8 Lanczos3 resize, preserving image 0.25.10's vertical-then-horizontal
//! arithmetic, tap order, clamping and rounding exactly. Reference: imageops/sample.rs
//! (MIT OR Apache-2.0, image-rs contributors). Only independent output pixels are parallel.
#[cfg(test)]
use image::RgbaImage;
use image::{ImageBuffer, Pixel};
use rayon::prelude::*;
fn sinc(t: f32) -> f32 {
    let a = t * std::f32::consts::PI;
    if t == 0.0 { 1.0 } else { a.sin() / a }
}
fn taps(src: u32, dst: u32) -> Vec<(usize, Vec<f32>)> {
    let ratio = src as f32 / dst as f32;
    let scale = ratio.max(1.0);
    let support = 3.0 * scale;
    (0..dst)
        .map(|o| {
            let center = (o as f32 + 0.5) * ratio;
            let left = ((center - support).floor() as i64).clamp(0, i64::from(src) - 1) as usize;
            let right = ((center + support).ceil() as i64).clamp(left as i64 + 1, i64::from(src)) as usize;
            let center = center - 0.5;
            let mut sum = 0.0;
            let mut weights: Vec<_> = (left..right)
                .map(|i| {
                    let x = (i as f32 - center) / scale;
                    let w = if x.abs() < 3.0 { sinc(x) * sinc(x / 3.0) } else { 0.0 };
                    sum += w;
                    w
                })
                .collect();
            weights.iter_mut().for_each(|w| *w /= sum);
            (left, weights)
        })
        .collect()
}
pub fn lanczos3<P: Pixel<Subpixel = u8> + Send + Sync + 'static>(source: &ImageBuffer<P, Vec<u8>>, width: u32, height: u32) -> ImageBuffer<P, Vec<u8>> {
    let channels = P::CHANNEL_COUNT as usize;
    if source.dimensions() == (width, height) {
        return source.clone();
    }
    if width == 0 || height == 0 || source.width() == 0 || source.height() == 0 {
        return ImageBuffer::new(width, height);
    }
    lightcraft_sysmem::power::compute(|| {
        let sw = source.width() as usize;
        let vertical = taps(source.height(), height);
        let horizontal = taps(source.width(), width);
        let mut tmp = vec![[0.0f32; 4]; sw * height as usize];
        tmp.par_chunks_mut(sw).zip(&vertical).for_each(|(row, (left, weights))| {
            for (x, p) in row.iter_mut().enumerate() {
                for (i, weight) in weights.iter().enumerate() {
                    let at = ((*left + i) * sw + x) * channels;
                    for c in 0..channels {
                        p[c] += f32::from(source.as_raw()[at + c]) * weight;
                    }
                }
            }
        });
        let mut out: ImageBuffer<P, Vec<u8>> = ImageBuffer::new(width, height);
        let raw: &mut [u8] = out.as_mut();
        raw.par_chunks_exact_mut(width as usize * channels).enumerate().for_each(|(y, row)| {
            for (p, (left, weights)) in row.chunks_exact_mut(channels).zip(&horizontal) {
                let mut value = [0.0f32; 4];
                for (i, weight) in weights.iter().enumerate() {
                    let sample = tmp[y * sw + left + i];
                    for c in 0..4 {
                        value[c] += sample[c] * weight;
                    }
                }
                for c in 0..channels {
                    p[c] = value[c].clamp(0.0, 255.0).round() as u8;
                }
            }
        });
        let _ = out.set_color_space(source.color_space());
        out
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enhance_parallel_lanczos_byte_identical_to_image_serial() {
        for (sw, sh) in [(1, 1), (1, 23), (37, 1), (65, 49), (19, 31)] {
            let source = RgbaImage::from_fn(sw, sh, |x, y| image::Rgba([(x * 19) as u8, (y * 31) as u8, ((x + y) * 7) as u8, (x * y) as u8]));
            for (w, h) in [(1, 1), (7, 3), (66, 97), (sw, sh)] {
                let reference = image::imageops::resize(&source, w, h, image::imageops::FilterType::Lanczos3);
                for threads in [1, 4] {
                    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
                    assert_eq!(pool.install(|| lanczos3(&source, w, h)).as_raw(), reference.as_raw(), "{sw}x{sh} -> {w}x{h}, {threads}");
                    let rgb = image::DynamicImage::ImageRgba8(source.clone()).to_rgb8();
                    let rgb_reference = image::imageops::resize(&rgb, w, h, image::imageops::FilterType::Lanczos3);
                    assert_eq!(pool.install(|| lanczos3(&rgb, w, h)).as_raw(), rgb_reference.as_raw());
                }
            }
        }
    }
}
