//! Compatibility entry point for the shared Lanczos3 resampler.
#[cfg(test)]
use image::RgbaImage;
use image::{ImageBuffer, Pixel};
pub fn lanczos3<P: Pixel<Subpixel = u8> + Send + Sync + 'static>(source: &ImageBuffer<P, Vec<u8>>, width: u32, height: u32) -> ImageBuffer<P, Vec<u8>> {
    lightcraft_sysmem::power::compute(|| li_raster::resize_u8(source, width, height, li_raster::Filter::Lanczos3))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_resize_parallel_lanczos_opaque_byte_identical_to_image_serial() {
        for (sw, sh) in [(1, 1), (1, 23), (37, 1), (65, 49), (19, 31)] {
            let source = RgbaImage::from_fn(sw, sh, |x, y| image::Rgba([(x * 19) as u8, (y * 31) as u8, ((x + y) * 7) as u8, 255]));
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
