//! Adapters to the shared raster blur backend.

use crate::Image;
use crate::resample::Pixel;

pub use li_raster::blur::{BAND, EXACT_SIGMA, FINE_SIGMA, box_radii, gauss_taps, gaussian_kernel};

/// Gaussian blur; exact at sigma <= 4, three running-sum boxes above it.
/// Small sigmas produce a real blur (#706).
pub fn gaussian<T: Pixel>(img: &Image<T>, sigma: f32) -> Image<T> {
    Image { width: img.width, height: img.height, data: li_raster::blur::gaussian(&img.data, img.width, img.height, sigma) }
}

/// Variance-corrected fine Gaussian, retaining the Develop GPU twin's tap order.
pub fn gaussian_fine<T: Pixel>(img: &Image<T>, sigma: f32) -> Image<T> {
    Image { width: img.width, height: img.height, data: li_raster::blur::gaussian_fine(&img.data, img.width, img.height, sigma) }
}

/// Square minimum with replicated borders.
pub fn min_filter(img: &Image<f32>, r: usize) -> Image<f32> {
    Image { width: img.width, height: img.height, data: li_raster::blur::extrema_filter(&img.data, img.width, img.height, r, false) }
}

/// Square maximum with replicated borders.
pub fn max_filter(img: &Image<f32>, r: usize) -> Image<f32> {
    Image { width: img.width, height: img.height, data: li_raster::blur::extrema_filter(&img.data, img.width, img.height, r, true) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fine_gaussian_keeps_variance_and_mass_at_small_sigma() {
        for sigma in [0.3f32, 0.5, 0.8, 1.2, 1.9] {
            let mut imp = Image::<f32>::new(41, 1);
            imp.set(20, 0, 1.0);
            let b = gaussian_fine(&imp, sigma);
            let sum: f32 = b.data.iter().sum();
            let var: f32 = b.data.iter().enumerate().map(|(i, v)| v * (i as f32 - 20.0).powi(2)).sum();
            assert!((sum - 1.0).abs() < 1e-5, "{sigma}: {sum}");
            assert!((var.sqrt() - sigma).abs() / sigma < 0.05, "{sigma}: {}", var.sqrt());
        }
        // Both shared policies now blur at 0.5 px (#706).
        let mut imp = Image::<f32>::new(9, 9);
        imp.set(4, 4, 1.0);
        assert!(gaussian(&imp, 0.5).get(4, 4) < 1.0);
        assert!(gaussian_fine(&imp, 0.5).get(4, 4) < 0.6);
    }

    #[test]
    fn min_filter_takes_the_window_minimum() {
        let img = Image::<f32>::from_fn(20, 10, |x, y| ((x * 3 + y * 7) % 11) as f32);
        let m = min_filter(&img, 2);
        for y in 0..10usize {
            for x in 0..20usize {
                let mut want = f32::INFINITY;
                for yy in y.saturating_sub(2)..=(y + 2).min(9) {
                    for xx in x.saturating_sub(2)..=(x + 2).min(19) {
                        want = want.min(img.get(xx, yy));
                    }
                }
                assert_eq!(m.get(x, y), want);
            }
        }
    }

    #[test]
    fn preserves_constant_and_mass() {
        let img = Image::<f32>::filled(50, 30, 0.7);
        let b = gaussian(&img, 4.0);
        assert!(b.data.iter().all(|v| (v - 0.7).abs() < 1e-5));
        let mut imp = Image::<f32>::new(101, 101);
        imp.set(50, 50, 1.0);
        let b = gaussian(&imp, 5.0);
        let sum: f32 = b.data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "{sum}");
        // peak moved to the centre and symmetric
        assert!((b.get(45, 50) - b.get(55, 50)).abs() < 1e-6);
        assert!((b.get(50, 45) - b.get(50, 55)).abs() < 1e-6);
    }

    #[test]
    fn variance_close_to_sigma_squared() {
        let mut imp = Image::<f32>::new(201, 1);
        imp.set(100, 0, 1.0);
        let sigma = 8.0;
        let b = gaussian(&imp, sigma);
        let var: f32 = b.data.iter().enumerate().map(|(i, v)| v * ((i as f32 - 100.0).powi(2))).sum();
        assert!((var.sqrt() - sigma).abs() / sigma < 0.1, "{}", var.sqrt());
    }
}

#[cfg(test)]
#[test]
fn ds_finding_min_filter_extreme_radius() {
    let image = Image::from_fn(3, 2, |x, y| (x + 3 * y) as f32);
    assert_eq!(min_filter(&image, usize::MAX), Image::filled(3, 2, 0.));
}

#[cfg(test)]
#[test]
fn j01_d2_library_adapter_small_sigma_and_alpha() {
    let img = Image::from_fn(3, 1, |x, _| if x == 0 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 0.0] });
    let actual = gaussian(&img, 0.5);
    assert!(actual.get(0, 0)[3] < 1.0);
    assert!(actual.get(1, 0)[3] > 0.0);
    assert_eq!(actual.get(1, 0)[0], 1.0);
    assert_eq!(actual.get(1, 0)[2], 0.0);
    assert_eq!(actual.data, li_raster::blur::gaussian(&img.data, 3, 1, 0.5));
}
