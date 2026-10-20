//! Resolution-independent subject refinement and the Develop Edge adapter.

use crate::{RgbImage, cf_matting};
use li_raster::{Filter, blur::GaussianPolicy, resample::resize_pixels};

/// Maximum long edge of the iterative solve. Independent of CPU/GPU model or RAM size.
pub const MAX_SOLVE_EDGE: usize = 1024;

/// Convert Develop's signed Edge setting to a multiplier on model logits.
pub fn edge_gain(edge: f64) -> f32 {
    let e = (edge / 100.0).clamp(-1.0, 1.0) as f32;
    if e < 0.0 { 1.0 - 7.0 * e } else { 1.0 }
}

/// Positive Edge is Gaussian feathering up to 2% of the oriented photo's long edge.
/// Keep the existing Develop box policy and half-pixel minimum, including crop scaling.
pub fn feather_edge(mask: &[f32], w: usize, h: usize, edge: f64, pixels_per_long: f32) -> Vec<f32> {
    let e = (edge / 100.0).clamp(0.0, 1.0) as f32;
    if e == 0.0 {
        return mask.to_vec();
    }
    li_raster::blur::gaussian_with_policy(mask, w, h, (e * 0.02 * pixels_per_long).max(0.5), GaussianPolicy::Boxes)
}

/// Refine a subject boundary with the shared closed-form solver. Preserve input coverage
/// outside the solve-grid unknown band (plus one interpolation cell on upscaling)
/// and return seedless/constant masks unchanged.
///
/// The expensive solve is bounded to 1024 on the long edge. Upsample its *correction*,
/// rather than a replacement mask, so high-resolution model/detail coverage is retained.
/// Scene-referred RGB is affine-normalized per solve tile: this preserves the compositing
/// equation and chromatic edges, unlike per-pixel clipping or tone mapping.
pub fn subject_alpha(img: &RgbImage<'_>, mask: &[f32], band: f32) -> Vec<f32> {
    let (w, h) = (img.w, img.h);
    assert_eq!(mask.len(), w * h);
    if w < 4 || h < 4 || !band.is_finite() || band <= 0.0 {
        return mask.to_vec();
    }
    let scale = (MAX_SOLVE_EDGE as f32 / w.max(h) as f32).min(1.0);
    let (sw, sh) = (((w as f32 * scale).round() as usize).max(1), ((h as f32 * scale).round() as usize).max(1));
    if sw < 4 || sh < 4 {
        return mask.to_vec();
    }
    let small = resize_pixels(mask, w, h, sw, sh, Filter::Triangle);
    let band_px = (band * scale).round().clamp(1.0, 64.0) as usize;
    let tri = cf_matting::trimap_from_mask(&small, sw, sh, band_px);
    if !tri.contains(&0.0) || !tri.contains(&1.0) || !tri.contains(&0.5) {
        return mask.to_vec();
    }
    let rgb = resize_pixels(img.px.as_ref(), w, h, sw, sh, Filter::Triangle);
    if rgb.iter().flatten().any(|v| !v.is_finite()) {
        return mask.to_vec();
    }
    let guide = RgbImage::borrowed(sw, sh, &rgb);
    // Larger Laplacian windows propagate wide optical transitions without extending
    // the unknown band; narrow hair boundaries retain Levin's 3x3 window.
    let radius = (band_px / 4).clamp(1, 16);
    // Maintain a 1e-5 covariance ridge after the solver divides epsilon by window
    // area. A fixed epsilon would fall below f32 covariance precision at wide radii.
    let params = cf_matting::MattingParams { radius, epsilon: 1e-5 * ((2 * radius + 1).pow(2)) as f32, ..Default::default() };
    let solved = cf_matting::closed_form_matting_tiled_scene(&guide, &tri, &small, &params, 256, band_px + 32);
    let delta: Vec<f32> = solved.iter().zip(&small).zip(&tri).map(|((a, m), t)| if *t == 0.5 { a - m } else { 0.0 }).collect();
    let delta = resize_pixels(&delta, sw, sh, w, h, Filter::Triangle);
    mask.iter().zip(delta).map(|(a, d)| if d.is_finite() { (a + d).clamp(0.0, 1.0) } else { *a }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_wide_band_has_no_tile_seam_or_remote_hdr_dependency() {
        let (w, h) = (768, 16);
        let truth: Vec<f32> = (0..w * h).map(|i| ((281.0 - (i % w) as f32) / 50.0).clamp(0.0, 1.0)).collect();
        let img = RgbImage::from_fn(w, h, |x, y| {
            let a = truth[y * w + x];
            [0.1 + 0.8 * a, 0.6 - 0.4 * a, 0.3 + 0.3 * a]
        });
        let mask: Vec<f32> = truth.iter().map(|a| if *a >= 0.5 { 1.0 } else { 0.0 }).collect();
        let a = subject_alpha(&img, &mask, 41.0);
        let outlier = RgbImage::from_fn(w, h, |x, y| if x == 767 { [1e6; 3] } else { img.px[y * w + x] });
        let b = subject_alpha(&outlier, &mask, 41.0);
        assert_eq!(a, b, "a distant highlight must not change the matte");
        let row = &a[8 * w..9 * w];
        assert!((row[255] - row[256]).abs() < 0.08, "tile seam: {} vs {}", row[255], row[256]);
        let mae = row.iter().zip(&truth[..w]).map(|(a, b)| (a - b).abs()).sum::<f32>() / w as f32;
        let hard_mae = mask[..w].iter().zip(&truth[..w]).map(|(a, b)| (a - b).abs()).sum::<f32>() / w as f32;
        println!("wide optical edge: hard MAE={hard_mae:.8}, shared MAE={mae:.8}, tile seam={:.8}", (row[255] - row[256]).abs());
        assert!(mae < 0.015 && mae < hard_mae * 0.6, "wide-band matte MAE {mae} vs hard {hard_mae}");
    }

    #[test]
    fn subject_refine_preserves_seedless_and_invalid_guides() {
        let img = RgbImage::from_fn(32, 20, |x, _| [x as f32 / 32.0; 3]);
        for m in [vec![0.0; 640], vec![1.0; 640], vec![0.5; 640], vec![0.2; 640]] {
            assert_eq!(subject_alpha(&img, &m, 3.0), m);
        }
        let m: Vec<f32> = (0..640).map(|i| if i % 32 < 16 { 1.0 } else { 0.0 }).collect();
        assert_eq!(subject_alpha(&img, &m, 0.0), m);
        let invalid = RgbImage::from_fn(32, 20, |_, _| [f32::NAN; 3]);
        assert_eq!(subject_alpha(&invalid, &m, 3.0), m);
        for rgb in [[4.0; 3], [0.1, 0.6, 0.3]] {
            let flat = RgbImage::from_fn(32, 20, |_, _| rgb);
            assert_eq!(subject_alpha(&flat, &m, 3.0), m);
        }
        let tiny = RgbImage::from_fn(3, 17, |x, _| [x as f32; 3]);
        let m = vec![0.6; 51];
        assert_eq!(subject_alpha(&tiny, &m, 3.0), m);
    }

    #[test]
    fn subject_refine_bound_and_hdr_affine_invariance() {
        let (w, h) = (2051, 17); // odd, thin image, beyond the iterative solve limit
        let img = RgbImage::from_fn(w, h, |x, _| {
            let a = ((1030.0 - x as f32) / 10.0).clamp(0.0, 1.0);
            [0.1 + a * 0.8, 0.6 - a * 0.4, 0.3 + a * 0.3]
        });
        let m: Vec<f32> = (0..w * h).map(|i| if i % w < 1025 { 1.0 } else { 0.0 }).collect();
        let a = subject_alpha(&img, &m, 9.0);
        let hdr = RgbImage::from_fn(w, h, |x, y| img.px[y * w + x].map(|v| v * 8.0 - 2.0));
        let b = subject_alpha(&hdr, &m, 9.0);
        assert_eq!(a.len(), w * h);
        assert!(a.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
        assert!(a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 0.03));
        assert_eq!(a[0], 1.0);
        assert_eq!(a[w - 1], 0.0);
        assert!(a.iter().any(|v| (0.05..0.95).contains(v)), "actually solved the boundary");
    }

    #[test]
    fn edt_exact_small_and_invalid_parity() {
        for (w, h) in [(1, 17), (19, 1), (31, 23)] {
            let seeds: Vec<bool> = (0..w * h).map(|i| i % 11 == 0).collect();
            let actual = crate::distance::edt(&seeds, w, h);
            for (i, d) in actual.iter().enumerate() {
                let expected = seeds
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| **v)
                    .map(|(j, _)| {
                        let dx = (i % w) as f64 - (j % w) as f64;
                        let dy = (i / w) as f64 - (j / w) as f64;
                        (dx * dx + dy * dy).sqrt() as f32
                    })
                    .fold(f32::INFINITY, f32::min);
                assert_eq!(*d, expected);
            }
        }
        assert!(crate::distance::edt(&[], usize::MAX, 2).is_empty());
        assert_eq!(crate::distance::edt(&[false; 6], 3, 2), vec![1e10; 6]);
    }
}
