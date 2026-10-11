use photocraft_algo::cf_matting::{MattingParams, closed_form_matting, closed_form_matting_tiled, estimate_foreground, trimap_from_mask};
use photocraft_algo::segment::RgbImage;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn solid_image(w: usize, h: usize, color: [f32; 3]) -> RgbImage {
    RgbImage { w, h, px: vec![color; w * h] }
}

fn gradient_image(w: usize, h: usize) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let fx = x as f32 / (w - 1).max(1) as f32;
        let fy = y as f32 / (h - 1).max(1) as f32;
        [fx, fy, 0.5]
    })
}

fn randomish_image(w: usize, h: usize) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let v = ((x * 7 + y * 13) % 100) as f32 / 100.0;
        [v, 1.0 - v, 0.5]
    })
}

#[test]
fn matting_params_defaults_are_correct() {
    let p = MattingParams::default();
    assert_eq!(p.radius, 1);
    assert_eq!(p.epsilon, 1e-6);
    assert_eq!(p.lambda, 100.0);
    assert_eq!(p.iterations, 120);
}

#[test]
fn closed_form_matting_no_unknown_returns_binary_threshold() {
    let w = 8;
    let h = 8;
    let img = solid_image(w, h, [0.5, 0.5, 0.5]);
    // All values are known: only 0.0 and 1.0 (no unknown range)
    let trimap = vec![
        0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0,
        1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0,
        1.0, 0.0, 1.0, 0.0,
    ];
    let alpha = closed_form_matting(&img, &trimap, &MattingParams::default());
    assert_eq!(alpha.len(), w * h);
    for (a, t) in alpha.iter().zip(&trimap) {
        let expected = if *t >= 0.5 { 1.0 } else { 0.0 };
        assert!((a - expected).abs() < 1e-6, "trimap {:.2} -> alpha {:.2}", t, a);
    }
}

#[test]
fn closed_form_matting_all_background_is_zero() {
    let w = 6;
    let h = 6;
    let img = gradient_image(w, h);
    let trimap = vec![0.0; w * h];
    let alpha = closed_form_matting(&img, &trimap, &MattingParams::default());
    assert!(alpha.iter().all(|&a| a == 0.0));
}

#[test]
fn closed_form_matting_all_foreground_is_one() {
    let w = 6;
    let h = 6;
    let img = gradient_image(w, h);
    let trimap = vec![1.0; w * h];
    let alpha = closed_form_matting(&img, &trimap, &MattingParams::default());
    assert!(alpha.iter().all(|&a| a == 1.0));
}

#[test]
fn closed_form_matting_tiny_image_no_panic() {
    let sizes = [(1, 1), (2, 2), (3, 3), (1, 5), (5, 1)];
    for &(w, h) in &sizes {
        let img = solid_image(w, h, [0.5, 0.5, 0.5]);
        let trimap = vec![0.5; w * h]; // unknown, but early return for w<4 or h<4
        let result = catch_unwind(AssertUnwindSafe(|| closed_form_matting(&img, &trimap, &MattingParams::default())));
        assert!(result.is_ok(), "panic for size {}x{}", w, h);
        let alpha = result.unwrap();
        assert_eq!(alpha.len(), w * h);
        // For tiny images with unknown trimap values, early return maps to binary threshold
        for &a in &alpha {
            assert!(a == 0.0 || a == 1.0, "alpha should be binary for tiny image, got {}", a);
        }
    }
}

#[test]
fn closed_form_matting_odd_sizes_no_panic_and_finite() {
    let (w, h) = (7, 5);
    let img = randomish_image(w, h);
    let trimap = (0..w * h)
        .map(|i| {
            if i % 3 == 0 {
                0.0
            } else if i % 3 == 1 {
                1.0
            } else {
                0.5
            }
        })
        .collect::<Vec<_>>();
    let alpha = closed_form_matting(&img, &trimap, &MattingParams::default());
    assert_eq!(alpha.len(), w * h);
    for &a in &alpha {
        assert!(a.is_finite(), "alpha {} not finite", a);
        assert!((0.0..=1.0).contains(&a), "alpha {} out of range", a);
    }
}

#[test]
fn closed_form_matting_known_pixels_preserved() {
    let w = 16;
    let h = 16;
    let img = gradient_image(w, h);
    // Construct trimap: known foreground border, known background interior, unknown ring.
    let mut trimap = vec![0.5f32; w * h];
    // background: top-left quadrant
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            trimap[y * w + x] = 0.0;
        }
    }
    // foreground: bottom-right quadrant
    for y in h / 2..h {
        for x in w / 2..w {
            trimap[y * w + x] = 1.0;
        }
    }
    // the rest is 0.5 (unknown)
    let alpha = closed_form_matting(&img, &trimap, &MattingParams::default());
    for (a, t) in alpha.iter().zip(&trimap) {
        if *t == 0.0 {
            assert!((a - 0.0).abs() < 1e-6, "known background changed");
        } else if *t == 1.0 {
            assert!((a - 1.0).abs() < 1e-6, "known foreground changed");
        }
    }
}

#[test]
fn closed_form_matting_alpha_finite_and_in_range() {
    let w = 16;
    let h = 16;
    let img = randomish_image(w, h);
    let trimap = (0..w * h)
        .map(|i| {
            if i % 4 == 0 {
                0.0
            } else if i % 4 == 3 {
                1.0
            } else {
                0.5
            }
        })
        .collect::<Vec<_>>();
    let alpha = closed_form_matting(&img, &trimap, &MattingParams::default());
    for &a in &alpha {
        assert!(a.is_finite());
        assert!((0.0..=1.0).contains(&a));
    }
}

#[test]
fn closed_form_matting_deterministic() {
    let w = 12;
    let h = 12;
    let img = randomish_image(w, h);
    let trimap = (0..w * h)
        .map(|i| {
            if i % 5 == 0 {
                0.0
            } else if i % 5 == 4 {
                1.0
            } else {
                0.5
            }
        })
        .collect::<Vec<_>>();
    let p = MattingParams::default();
    let a1 = closed_form_matting(&img, &trimap, &p);
    let a2 = closed_form_matting(&img, &trimap, &p);
    assert_eq!(a1, a2);
}

#[test]
fn closed_form_matting_nan_in_trimap_no_panic() {
    let w = 8;
    let h = 8;
    let img = solid_image(w, h, [0.5, 0.5, 0.5]);
    let mut trimap = vec![0.5; w * h];
    trimap[0] = f32::NAN;
    let result = catch_unwind(AssertUnwindSafe(|| closed_form_matting(&img, &trimap, &MattingParams::default())));
    assert!(result.is_ok(), "function should not panic on NaN trimap");
}

#[test]
fn closed_form_matting_inf_in_trimap_no_panic() {
    let w = 8;
    let h = 8;
    let img = solid_image(w, h, [0.5, 0.5, 0.5]);
    let mut trimap = vec![0.5; w * h];
    trimap[0] = f32::INFINITY;
    let result = catch_unwind(AssertUnwindSafe(|| closed_form_matting(&img, &trimap, &MattingParams::default())));
    assert!(result.is_ok(), "function should not panic on infinite trimap");
}

#[test]
fn closed_form_matting_nan_in_image_no_panic() {
    let w = 8;
    let h = 8;
    let mut img = solid_image(w, h, [0.5, 0.5, 0.5]);
    img.px[0][0] = f32::NAN;
    let trimap = vec![0.5; w * h];
    let result = catch_unwind(AssertUnwindSafe(|| closed_form_matting(&img, &trimap, &MattingParams::default())));
    assert!(result.is_ok(), "function should not panic on NaN image");
}

#[test]
fn closed_form_matting_empty_no_panic() {
    let img = RgbImage { w: 0, h: 0, px: vec![] };
    let trimap: Vec<f32> = vec![];
    let result = catch_unwind(AssertUnwindSafe(|| closed_form_matting(&img, &trimap, &MattingParams::default())));
    assert!(result.is_ok());
    let alpha = result.unwrap();
    assert!(alpha.is_empty());
}

#[test]
fn trimap_from_mask_zero_band_is_binary() {
    let w = 10;
    let h = 10;
    let mask = (0..w * h).map(|i| if i % 2 == 0 { 0.6 } else { 0.4 }).collect::<Vec<_>>();
    let tri = trimap_from_mask(&mask, w, h, 0);
    for (t, m) in tri.iter().zip(&mask) {
        if *m >= 0.5 {
            assert_eq!(*t, 1.0);
        } else {
            assert_eq!(*t, 0.0);
        }
    }
}

#[test]
fn trimap_from_mask_all_foreground() {
    let w = 8;
    let h = 8;
    let mask = vec![1.0; w * h];
    let tri = trimap_from_mask(&mask, w, h, 2);
    assert!(tri.iter().all(|&t| t == 1.0));
}

#[test]
fn trimap_from_mask_all_background() {
    let w = 8;
    let h = 8;
    let mask = vec![0.0; w * h];
    let tri = trimap_from_mask(&mask, w, h, 2);
    assert!(tri.iter().all(|&t| t == 0.0));
}

#[test]
fn trimap_from_mask_output_values_are_valid() {
    let w = 16;
    let h = 16;
    let mask = (0..w * h).map(|i| (i % 11) as f32 / 10.0).collect::<Vec<_>>();
    let tri = trimap_from_mask(&mask, w, h, 3);
    for &t in &tri {
        assert!(t == 0.0 || t == 0.5 || t == 1.0, "unexpected value {}", t);
    }
}

#[test]
fn estimate_foreground_alpha_all_one_matches_image() {
    let w = 8;
    let h = 8;
    let img = gradient_image(w, h);
    let alpha = vec![1.0; w * h];
    let fg = estimate_foreground(&img, &alpha);
    assert_eq!(fg.w, w);
    assert_eq!(fg.h, h);
    for i in 0..w * h {
        for c in 0..3 {
            let diff = (fg.px[i][c] - img.px[i][c]).abs();
            assert!(diff < 0.2, "pixel {} channel {} diff too large: {}", i, c, diff);
        }
    }
}

#[test]
fn estimate_foreground_output_finite_and_in_range() {
    let w = 16;
    let h = 16;
    let img = randomish_image(w, h);
    let alpha = (0..w * h).map(|i| (i % 10) as f32 / 9.0).collect::<Vec<_>>();
    let fg = estimate_foreground(&img, &alpha);
    assert_eq!(fg.w, w);
    assert_eq!(fg.h, h);
    for px in &fg.px {
        for &v in px {
            assert!(v.is_finite());
            assert!((0.0..=1.0).contains(&v));
        }
    }
}

#[test]
fn estimate_foreground_small_input_no_panic() {
    let sizes = [(1, 1), (2, 3), (4, 2)];
    for &(w, h) in &sizes {
        let img = solid_image(w, h, [0.5, 0.5, 0.5]);
        let alpha = (0..w * h).map(|i| if i % 2 == 0 { 0.0 } else { 1.0 }).collect::<Vec<_>>();
        let result = catch_unwind(AssertUnwindSafe(|| estimate_foreground(&img, &alpha)));
        assert!(result.is_ok(), "panic for size {}x{}", w, h);
        let fg = result.unwrap();
        assert_eq!(fg.w, w);
        assert_eq!(fg.h, h);
    }
}

#[test]
fn closed_form_matting_tiled_matches_non_tiled() {
    let w = 24;
    let h = 24;
    let img = randomish_image(w, h);
    let trimap = (0..w * h)
        .map(|i| {
            if i % 6 == 0 {
                0.0
            } else if i % 6 == 3 {
                1.0
            } else {
                0.5
            }
        })
        .collect::<Vec<_>>();
    let p = MattingParams::default();
    let a_full = closed_form_matting(&img, &trimap, &p);
    let a_tiled = closed_form_matting_tiled(&img, &trimap, &p, 8, 4);
    assert_eq!(a_full.len(), a_tiled.len());
    let mut max_diff: f32 = 0.0;
    for (a, b) in a_full.iter().zip(&a_tiled) {
        let d = (a - b).abs();
        if d > max_diff {
            max_diff = d;
        }
    }
    assert!(max_diff < 0.05, "maximum difference between tiled and full is {}", max_diff);
}

#[test]
fn closed_form_matting_tiled_min_tile_and_margin_no_panic() {
    let w = 10;
    let h = 8;
    let img = gradient_image(w, h);
    let trimap = (0..w * h).map(|i| if i % 2 == 0 { 0.0 } else { 1.0 }).collect::<Vec<_>>();
    // tile=32 (enforced min), margin=0
    let result = catch_unwind(AssertUnwindSafe(|| closed_form_matting_tiled(&img, &trimap, &MattingParams::default(), 32, 0)));
    assert!(result.is_ok());
    let alpha = result.unwrap();
    assert_eq!(alpha.len(), w * h);
}
