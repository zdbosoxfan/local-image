//! Parallel u8 image resize. Opaque Lanczos preserves the previous builder's image-rs
//! arithmetic; alpha-bearing input filters premultiplied colour to avoid fringes.
//! The intermediate is one source-width float row per worker. Reference: imageops/sample.rs
//! (MIT OR Apache-2.0, image-rs contributors). Only independent output pixels are parallel.
use crate::{Filter, par_rows};
use image::{ImageBuffer, Pixel};
fn sinc(t: f32) -> f32 {
    let a = t * std::f32::consts::PI;
    if t == 0.0 { 1.0 } else { a.sin() / a }
}
// These two arithmetic policies match image-rs's normalized Gaussian and B/C cubic
// exactly, preserving opaque bytes while the shared alpha policy corrects fringes.
fn image_weight(filter: Filter, x: f32) -> f32 {
    match filter {
        Filter::Gaussian => ((2.0 * std::f32::consts::PI).sqrt() * 0.5).recip() * (-x.powi(2) / (2.0 * 0.5f32.powi(2))).exp(),
        Filter::CatmullRom => {
            let a = x.abs();
            let k = if a < 1.0 {
                9.0 * a.powi(3) - 15.0 * a.powi(2) + 6.0
            } else if a < 2.0 {
                -3.0 * a.powi(3) + 15.0 * a.powi(2) - 24.0 * a + 12.0
            } else {
                0.0
            };
            k / 6.0
        }
        _ => filter.eval(x),
    }
}
fn taps(src: u32, dst: u32, filter: Filter) -> Vec<(usize, Vec<f32>)> {
    let ratio = src as f32 / dst as f32;
    let scale = ratio.max(1.0);
    let support = filter.support() * scale;
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
                    let w = if filter == Filter::Lanczos3 { if x.abs() < 3.0 { sinc(x) * sinc(x / 3.0) } else { 0.0 } } else { image_weight(filter, x) };
                    sum += w;
                    w
                })
                .collect();
            weights.iter_mut().for_each(|w| *w /= sum);
            (left, weights)
        })
        .collect()
}
pub fn resize_u8<P: Pixel<Subpixel = u8> + Send + Sync + 'static>(
    source: &ImageBuffer<P, Vec<u8>>,
    width: u32,
    height: u32,
    filter: Filter,
) -> ImageBuffer<P, Vec<u8>> {
    let channels = P::CHANNEL_COUNT as usize;
    if source.dimensions() == (width, height) {
        return source.clone();
    }
    if width == 0 || height == 0 || source.width() == 0 || source.height() == 0 {
        return ImageBuffer::new(width, height);
    }
    if filter == Filter::Nearest {
        let mut out = ImageBuffer::from_fn(width, height, |x, y| {
            *source.get_pixel(
                ((u64::from(x) * 2 + 1) * u64::from(source.width()) / (u64::from(width) * 2)) as u32,
                ((u64::from(y) * 2 + 1) * u64::from(source.height()) / (u64::from(height) * 2)) as u32,
            )
        });
        let _ = out.set_color_space(source.color_space());
        return out;
    }
    {
        let sw = source.width() as usize;
        let vertical = taps(source.height(), height, filter);
        let horizontal = taps(source.width(), width, filter);
        // One source-width float row per worker, rather than source-width × target-height.
        // Rows remain independent and retain image-rs's vertical-then-horizontal arithmetic.
        let alpha = P::HAS_ALPHA && source.as_raw().chunks_exact(channels).any(|p| p[channels - 1] != 255);
        let mut out: ImageBuffer<P, Vec<u8>> = ImageBuffer::new(width, height);
        par_rows(out.as_mut(), width as usize * channels, |y, row| {
            let (left, weights) = &vertical[y];
            let mut tmp = vec![[0.0f32; 4]; sw];
            for (x, p) in tmp.iter_mut().enumerate() {
                for (i, weight) in weights.iter().enumerate() {
                    let at = ((*left + i) * sw + x) * channels;
                    for c in 0..channels {
                        let v = f32::from(source.as_raw()[at + c]);
                        let v = if alpha && c + 1 < channels { v * f32::from(source.as_raw()[at + channels - 1]) / 255.0 } else { v };
                        p[c] += v * weight;
                    }
                }
            }
            for (p, (left, weights)) in row.chunks_exact_mut(channels).zip(&horizontal) {
                let mut value = [0.0f32; 4];
                for (i, weight) in weights.iter().enumerate() {
                    for c in 0..channels {
                        value[c] += tmp[left + i][c] * weight;
                    }
                }
                for c in 0..channels {
                    let v = if alpha && c + 1 < channels {
                        if value[channels - 1] > 1e-7 { value[c] * 255.0 / value[channels - 1] } else { 0.0 }
                    } else {
                        value[c]
                    };
                    p[c] = v.clamp(0.0, 255.0).round() as u8;
                }
            }
        });
        let _ = out.set_color_space(source.color_space());
        out
    }
}

pub fn resize_rgba8(source: &image::RgbaImage, width: u32, height: u32, filter: Filter) -> image::RgbaImage {
    resize_u8(source, width, height, filter)
}
pub fn resize_rgb8(source: &image::RgbImage, width: u32, height: u32, filter: Filter) -> image::RgbImage {
    resize_u8(source, width, height, filter)
}
pub fn resize_gray8(source: &image::GrayImage, width: u32, height: u32, filter: Filter) -> image::GrayImage {
    resize_u8(source, width, height, filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn u8_filters_opaque_byte_goldens() {
        // Independent established image-rs reference, all five current AI filters.
        let filters = [
            (Filter::Nearest, image::imageops::FilterType::Nearest),
            (Filter::Triangle, image::imageops::FilterType::Triangle),
            (Filter::CatmullRom, image::imageops::FilterType::CatmullRom),
            (Filter::Gaussian, image::imageops::FilterType::Gaussian),
            (Filter::Lanczos3, image::imageops::FilterType::Lanczos3),
        ];
        for (sw, sh) in [(1, 1), (1, 23), (37, 1), (19, 31)] {
            let source = image::RgbImage::from_fn(sw, sh, |x, y| image::Rgb([(x * 19) as u8, (y * 31) as u8, ((x + y) * 7) as u8]));
            for (w, h) in [(1, 1), (7, 3), (26, 37), (sw, sh)] {
                for (filter, reference) in filters {
                    let want = image::imageops::resize(&source, w, h, reference);
                    let got = resize_rgb8(&source, w, h, filter);
                    assert_eq!(got.as_raw(), want.as_raw(), "{sw}x{sh}->{w}x{h} {filter:?}");
                }
            }
        }
    }
    #[test]
    fn u8_empty_identity_and_colour_metadata() {
        let empty = image::RgbaImage::new(0, 3);
        assert_eq!(resize_rgba8(&empty, 7, 3, Filter::Lanczos3), image::RgbaImage::new(7, 3));
        let src = image::RgbaImage::from_fn(3, 2, |x, y| image::Rgba([x as u8, y as u8, 91, 0]));
        assert_eq!(resize_rgba8(&src, 3, 2, Filter::Lanczos3), src);
        assert_eq!(resize_rgba8(&src, 0, 3, Filter::Lanczos3).dimensions(), (0, 3));
        assert_eq!(resize_rgba8(&src, 7, 3, Filter::Lanczos3).color_space(), src.color_space());
        assert!(resize_rgba8(&src, 7, 3, Filter::Lanczos3).as_raw().iter().all(|&v| v == 0));
    }
}
