use photocraft_algo::segment::{FREE, HARD_BG, HARD_FG, RgbImage, Rng, antialias_u8, clean_mask, components, contrast_beta, grid_cut, keep_seeded, subsample};

#[test]
fn rgb_image_new_is_zeroed() {
    let img = RgbImage::new(2, 3);
    assert_eq!(img.w, 2);
    assert_eq!(img.h, 3);
    assert_eq!(img.px.len(), 6);
    assert!(img.px.iter().all(|p| *p == [0.0; 3]));
}

#[test]
fn rgb_image_from_fn_matches_at() {
    let img = RgbImage::from_fn(3, 2, |x, y| [x as f32, y as f32, (x + y) as f32]);
    assert_eq!(img.at(0, 0), [0.0, 0.0, 0.0]);
    assert_eq!(img.at(2, 1), [2.0, 1.0, 3.0]);
}

#[test]
fn rgb_image_from_rgba_opaque_keeps_rgb() {
    let rgba = vec![[1.0, 0.0, 0.0, 1.0], [0.0, 0.5, 1.0, 1.0]];
    let img = RgbImage::from_rgba(&rgba, 2, 1);
    assert_eq!(img.at(0, 0), [1.0, 0.0, 0.0]);
    assert_eq!(img.at(1, 0), [0.0, 0.5, 1.0]);
}

#[test]
fn rgb_image_from_rgba_transparent_composites_over_mid_gray() {
    let rgba = vec![[0.0, 0.0, 0.0, 0.0], [0.2, 0.4, 0.6, 0.5]];
    let img = RgbImage::from_rgba(&rgba, 2, 1);
    let p0 = img.at(0, 0);
    assert!((p0[0] - 0.5).abs() < 1e-6);
    assert!((p0[1] - 0.5).abs() < 1e-6);
    assert!((p0[2] - 0.5).abs() < 1e-6);

    let p1 = img.at(1, 0);
    assert!((p1[0] - 0.35).abs() < 1e-6);
    assert!((p1[1] - 0.45).abs() < 1e-6);
    assert!((p1[2] - 0.55).abs() < 1e-6);
}

#[test]
fn rgb_image_downsample_step_one_returns_clone() {
    let img = RgbImage::from_fn(2, 2, |x, y| [x as f32, y as f32, 1.0]);
    let down = img.downsample(1);
    assert_eq!(down, img);
}

#[test]
fn rgb_image_downsample_step_two_averages_2x2() {
    let img = RgbImage::from_fn(2, 2, |x, y| [x as f32, y as f32, (x + y) as f32]);
    let down = img.downsample(2);
    assert_eq!(down.w, 1);
    assert_eq!(down.h, 1);
    let p = down.at(0, 0);
    assert!((p[0] - 0.5).abs() < 1e-6);
    assert!((p[1] - 0.5).abs() < 1e-6);
    assert!((p[2] - 1.0).abs() < 1e-6);
}

#[test]
fn rgb_image_downsample_step_larger_than_image_gives_single_pixel() {
    let img = RgbImage::from_fn(2, 3, |_x, _y| [1.0, 2.0, 3.0]);
    let down = img.downsample(10);
    assert_eq!(down.w, 1);
    assert_eq!(down.h, 1);
    assert_eq!(down.at(0, 0), [1.0, 2.0, 3.0]);
}

#[test]
fn rgb_image_downsample_odd_sizes_produces_ceil_dims() {
    let img = RgbImage::new(3, 3);
    let down = img.downsample(2);
    assert_eq!(down.w, 2);
    assert_eq!(down.h, 2);
    assert_eq!(down.px.len(), 4);
}

#[test]
fn rgb_image_downsample_step_zero_is_clone() {
    let img = RgbImage::from_fn(2, 2, |x, y| [x as f32, y as f32, 0.0]);
    let down = img.downsample(0);
    assert_eq!(down, img);
}

#[test]
fn contrast_beta_uniform_image_is_zero() {
    let img = RgbImage::new(3, 2);
    assert_eq!(contrast_beta(&img), 0.0);
}

#[test]
fn contrast_beta_empty_image_is_zero() {
    let img = RgbImage::new(0, 0);
    assert_eq!(contrast_beta(&img), 0.0);
}

#[test]
fn contrast_beta_nonuniform_image_is_positive_finite() {
    let img = RgbImage::from_fn(2, 1, |x, _| if x == 0 { [0.0, 0.0, 0.0] } else { [1.0, 1.0, 1.0] });
    let beta = contrast_beta(&img);
    assert!(beta > 0.0);
    assert!(beta.is_finite());
}

#[test]
fn grid_cut_all_hard_background_returns_false() {
    let img = RgbImage::new(1, 1);
    let fixed = vec![HARD_BG];
    let result = grid_cut(&img, &[1.0], &[0.0], &fixed, 1.0, 0.1);
    assert_eq!(result, vec![false]);
}

#[test]
fn grid_cut_all_hard_foreground_returns_true() {
    let img = RgbImage::new(1, 1);
    let fixed = vec![HARD_FG];
    let result = grid_cut(&img, &[0.0], &[1.0], &fixed, 1.0, 0.1);
    assert_eq!(result, vec![true]);
}

#[test]
fn grid_cut_single_free_cheap_foreground_is_true() {
    let img = RgbImage::new(1, 1);
    let fixed = vec![FREE];
    let result = grid_cut(&img, &[0.0], &[1.0], &fixed, 1.0, 0.1);
    assert_eq!(result, vec![true]);
}

#[test]
fn grid_cut_single_free_cheap_background_is_false() {
    let img = RgbImage::new(1, 1);
    let fixed = vec![FREE];
    let result = grid_cut(&img, &[1.0], &[0.0], &fixed, 1.0, 0.1);
    assert_eq!(result, vec![false]);
}

#[test]
fn grid_cut_empty_image_returns_empty() {
    let img = RgbImage::new(0, 0);
    let result = grid_cut(&img, &[], &[], &[], 1.0, 0.1);
    assert!(result.is_empty());
}

#[test]
fn components_empty_mask_returns_empty() {
    let (labels, sizes) = components(&[], 0, 0);
    assert!(labels.is_empty());
    assert_eq!(sizes, vec![0]);
}

#[test]
fn components_single_pixel_foreground() {
    let mask = vec![true];
    let (labels, sizes) = components(&mask, 1, 1);
    assert_eq!(labels, vec![1]);
    assert_eq!(sizes, vec![0, 1]);
}

#[test]
fn components_diagonal_are_8_connected() {
    let mask = vec![true, false, false, true];
    let (labels, sizes) = components(&mask, 2, 2);
    assert_eq!(labels[0], labels[3]);
    assert_eq!(sizes[labels[0] as usize], 2);
}

#[test]
fn keep_seeded_empty_returns_empty() {
    let result = keep_seeded(&[], &[], 0, 0);
    assert!(result.is_empty());
}

#[test]
fn keep_seeded_keeps_only_seeded_components() {
    let mask = vec![true, true, false, false, true, true];
    let seeds = vec![false, false, false, false, true, false];
    let result = keep_seeded(&mask, &seeds, 6, 1);
    assert_eq!(result, vec![false, false, false, false, true, true]);
}

#[test]
fn clean_mask_empty_returns_empty() {
    let result = clean_mask(&[], 0, 0, 0.5, 0.5);
    assert!(result.is_empty());
}

#[test]
fn clean_mask_removes_small_component() {
    let mask = vec![true, false, true, true, true];
    let result = clean_mask(&mask, 5, 1, 0.5, 0.5);
    assert_eq!(result, vec![false, false, true, true, true]);
}

#[test]
fn clean_mask_fills_small_internal_hole() {
    let mut mask = vec![true; 9];
    mask[4] = false; // center hole in 3x3 border
    let result = clean_mask(&mask, 3, 3, 0.0, 0.5);
    assert!(result.iter().all(|v| *v));
}

#[test]
fn antialias_u8_all_zero_unchanged() {
    let mut mask = vec![0u8; 9];
    antialias_u8(&mut mask, 3, 3);
    assert!(mask.iter().all(|v| *v == 0));
}

#[test]
fn antialias_u8_all_255_unchanged() {
    let mut mask = vec![255u8; 9];
    antialias_u8(&mut mask, 3, 3);
    assert!(mask.iter().all(|v| *v == 255));
}

#[test]
fn antialias_u8_edge_blends_and_is_deterministic() {
    let mut mask = vec![0u8; 9];
    mask[4] = 255;
    let mut copy = mask.clone();
    antialias_u8(&mut mask, 3, 3);
    antialias_u8(&mut copy, 3, 3);
    assert_eq!(mask, copy);
    assert!(mask[4] > 0 && mask[4] < 255);
    assert_ne!(mask, vec![0u8; 9]);
}

#[test]
fn antialias_u8_empty_with_positive_width_no_panic() {
    let mut mask: Vec<u8> = Vec::new();
    antialias_u8(&mut mask, 1, 0);
}

#[test]
fn rng_deterministic_same_seed() {
    let mut a = Rng::new(42);
    let mut b = Rng::new(42);
    assert_eq!(a.next_u64(), b.next_u64());
    assert_eq!(a.next_u64(), b.next_u64());
}

#[test]
fn rng_different_seeds_differ() {
    let mut a = Rng::new(1);
    let mut b = Rng::new(2);
    assert_ne!(a.next_u64(), b.next_u64());
}

#[test]
fn rng_f32_in_unit_interval() {
    let mut rng = Rng::new(7);
    for _ in 0..1000 {
        let v = rng.f32();
        assert!((0.0..1.0).contains(&v));
    }
}

#[test]
fn rng_normal_is_finite() {
    let mut rng = Rng::new(9);
    for _ in 0..1000 {
        let v = rng.normal();
        assert!(v.is_finite());
    }
}

#[test]
fn subsample_short_returns_clone() {
    let v = vec![10, 20, 30];
    let out = subsample(&v, 5);
    assert_eq!(out, v);
}

#[test]
fn subsample_long_returns_at_most_max() {
    let v: Vec<i32> = (0..20).collect();
    let out = subsample(&v, 3);
    assert!(out.len() <= 3);
    assert_eq!(out[0], 0);
    assert!(out.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn subsample_empty_returns_empty() {
    let v: Vec<i32> = Vec::new();
    assert!(subsample(&v, 3).is_empty());
}

#[test]
fn subsample_max_one_returns_first_element() {
    let v = vec![5, 6, 7, 8, 9];
    assert_eq!(subsample(&v, 1), vec![5]);
}

#[test]
fn subsample_max_zero_does_not_panic() {
    let v = vec![1, 2, 3];
    let _ = subsample(&v, 0);
}
