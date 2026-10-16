//! `Image` adapters over the shared `li_raster` resampler.
use li_raster::resample::{Filter, Pixel};

use crate::Image;
pub fn resize<T: Pixel>(img: &Image<T>, w: usize, h: usize, filter: Filter) -> Image<T> {
    Image { width: w.max(1), height: h.max(1), data: li_raster::resample::resize_pixels(&img.data, img.width, img.height, w, h, filter) }
}
/// Fit within `max_w × max_h` preserving aspect (never upscales).
pub fn fit<T: Pixel>(img: &Image<T>, max_w: usize, max_h: usize, filter: Filter) -> Image<T> {
    let s = (max_w as f64 / img.width as f64).min(max_h as f64 / img.height as f64).min(1.0);
    let w = ((img.width as f64 * s).round() as usize).max(1);
    let h = ((img.height as f64 * s).round() as usize).max(1);
    resize(img, w, h, filter)
}

/// 2× box downsample (pyramid level).
pub fn half<T: Pixel>(img: &Image<T>) -> Image<T> {
    let w = img.width.div_ceil(2).max(1);
    let h = img.height.div_ceil(2).max(1);
    Image { width: w, height: h, data: li_raster::resample::half_pixels(&img.data, img.width, img.height) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_image_stays_constant() {
        let img = Image::<f32>::filled(37, 21, 0.42);
        for f in [Filter::Box, Filter::Bilinear, Filter::Mitchell, Filter::Lanczos3] {
            for (w, h) in [(10, 7), (80, 40), (37, 21), (1, 1)] {
                let r = resize(&img, w, h, f);
                assert!(r.data.iter().all(|v| (v - 0.42).abs() < 1e-5), "{f:?} {w}x{h}");
            }
        }
    }

    #[test]
    fn downscale_preserves_mean() {
        let img = Image::<f32>::from_fn(64, 64, |x, y| ((x * 7 + y * 3) % 11) as f32 / 10.0);
        let mean = img.data.iter().sum::<f32>() / img.len() as f32;
        let r = resize(&img, 16, 16, Filter::Lanczos3);
        let m2 = r.data.iter().sum::<f32>() / r.len() as f32;
        assert!((mean - m2).abs() < 0.01);
        let hh = half(&img);
        assert_eq!((hh.width, hh.height), (32, 32));
        let m3 = hh.data.iter().sum::<f32>() / hh.len() as f32;
        assert!((mean - m3).abs() < 1e-5);
    }

    #[test]
    fn fit_keeps_aspect() {
        let img = Image::<f32>::new(4000, 3000);
        let r = fit(&img, 400, 400, Filter::Box);
        assert_eq!((r.width, r.height), (400, 300));
        let small = fit(&Image::<f32>::new(10, 5), 400, 400, Filter::Box);
        assert_eq!((small.width, small.height), (10, 5));
    }
}

#[cfg(test)]
mod j01_golden {
    use super::*;
    #[test]
    fn j01_resample_adapter_byte_goldens() {
        for (f, expected) in [Filter::Box, Filter::Bilinear, Filter::Mitchell, Filter::Lanczos3].into_iter().zip([
            16937065032499866148,
            13728202162992833757,
            1960567902461757162,
            495902503799777730,
        ]) {
            let s = Image::<[f32; 3]>::from_fn(9, 7, |x, y| {
                [((x + y * 9) * 17 % 255) as f32 / 255.0, ((x + y * 9) * 31 % 255) as f32 / 255.0, ((x + y * 9) * 7 % 255) as f32 / 255.0]
            });
            let o = resize(&s, 7, 10, f);
            let data: Vec<_> = o.data.into_iter().flatten().collect();
            let hash = data
                .iter()
                .flat_map(|v| v.to_bits().to_le_bytes())
                .fold(14695981039346656037u64, |h, b| (h ^ u64::from(b)).wrapping_mul(1099511628211));
            assert_eq!(hash, expected, "{f:?}");
        }
    }
}

#[cfg(test)]
#[test]
fn j01_final_dense_alpha_half_nearest_and_empty() {
    let img = Image::from_fn(3, 1, |x, _| if x == 0 { [1.0, 0.2, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 0.0] });
    for got in [resize(&img, 7, 1, Filter::Bilinear), half(&img)] {
        for p in got.data {
            if p[3] > 1e-7 {
                assert!((p[0] - 1.0).abs() < 1e-6 && (p[1] - 0.2).abs() < 1e-6 && p[2] == 0.0);
            }
        }
    }
    let ramp = Image::<f32>::from_fn(9, 3, |x, y| (x + y * 9) as f32);
    let got = resize(&ramp, 3, 1, Filter::Nearest);
    assert_eq!(li_raster::resample::weights(9, 3, Filter::Nearest), vec![(1, vec![1.0]), (4, vec![1.0]), (7, vec![1.0])]);
    assert_eq!(got.data, vec![10.0, 13.0, 16.0]);
    assert_eq!(half(&Image::<f32>::new(0, 3)), Image::<f32>::new(1, 2));
    assert_eq!(resize(&Image::<f32>::new(0, 3), 2, 1, Filter::Lanczos3), Image::<f32>::new(2, 1));
}
