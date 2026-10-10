use image::{Rgb, RgbImage};
use photocraft_doc::{Path, Subpath};
use photocraft_testkit::vector::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;
#[test]
fn identical_and_invalid_rasters() -> TestResult {
    for (w, h) in [(1, 1), (11, 7), (64, 64)] {
        let a = RgbImage::from_pixel(w, h, Rgb([17, 130, 210]));
        assert!((ssim_ms(&a, &a)? - 1.).abs() < 1e-12);
        assert!((fidelity(&a, &a)?.fidelity - 1.).abs() < 1e-12);
        assert_eq!(delta_e_ok(&a, &a)?, (0., 0.));
    }
    assert!(ssim_ms(&RgbImage::new(0, 0), &RgbImage::new(0, 0)).is_err());
    assert!(fidelity(&RgbImage::new(3, 4), &RgbImage::new(4, 3)).is_err());
    assert!(iou(&[f32::NAN], &[0.], 0.5).is_err());
    Ok(())
}
#[test]
fn coherent_missing_patch_beats_scattered_noise() -> TestResult {
    let a = RgbImage::from_pixel(128, 128, Rgb([255; 3]));
    let mut patch = a.clone();
    let mut dust = a.clone();
    for y in 48..64 {
        for x in 48..64 {
            patch.put_pixel(x, y, Rgb([0; 3]));
        }
    }
    for y in 0..16 {
        for x in 0..16 {
            dust.put_pixel(x * 8, y * 8, Rgb([0; 3]));
        }
    }
    let patch = fidelity(&a, &patch)?;
    let dust = fidelity(&a, &dust)?;
    assert!(patch.patch_mass > 200.);
    assert_eq!(dust.patch_mass, 0.);
    assert!(patch.fidelity < dust.fidelity);
    Ok(())
}
#[test]
fn geometry_oracles_detect_crossing_and_distance() -> TestResult {
    let a = Path::new(vec![Subpath::polygon(&[(0., 0.), (10., 0.), (10., 10.), (0., 10.)])]);
    let b = a.transform(&photocraft_geom::Affine::translate(2., 0.));
    assert!((hausdorff(&a, &b, 64)? - 2.).abs() < 1e-6);
    assert!(self_intersections(&a).is_empty());
    let bow = Path::new(vec![Subpath::polygon(&[(0., 0.), (10., 10.), (0., 10.), (10., 0.)])]);
    assert!(!self_intersections(&bow).is_empty());
    assert_eq!(node_count(&a), 4);
    assert!(is_finite(&a));
    assert!((delta_e([255; 3], [0; 3]) - 100.).abs() < 1e-5);
    assert_eq!(iou(&[0., 1., 1.], &[1., 1., 0.], 0.5)?, 1. / 3.);
    Ok(())
}
