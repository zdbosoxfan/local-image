use photocraft_algo::features::{Corner, Descriptor, Model, describe, fit, hamming, harris, match_descriptors, ransac, register};

fn apply_matrix(m: [f64; 9], x: f64, y: f64) -> [f64; 2] {
    let w = m[6] * x + m[7] * y + m[8];
    [(m[0] * x + m[1] * y + m[2]) / w, (m[3] * x + m[4] * y + m[5]) / w]
}

fn block_texture(w: usize, h: usize) -> Vec<f32> {
    let mut img = vec![0.0f32; w * h];
    for row in 0..4usize {
        for col in 0..4usize {
            let x0 = 16 + col * 20;
            let y0 = 16 + row * 20;
            let val = 0.25 + ((row * 4 + col) as f32) * 0.09;
            for y in y0..(y0 + 14).min(h) {
                for x in x0..(x0 + 14).min(w) {
                    img[y * w + x] = val.clamp(0.0, 1.0);
                }
            }
        }
    }
    img
}

fn translate_image(src: &[f32], w: usize, h: usize, tx: i32, ty: i32) -> Vec<f32> {
    let mut out = vec![0.2f32; w * h];
    for y in 0..h {
        let sy = y as i32 + ty;
        if sy < 0 || sy >= h as i32 {
            continue;
        }
        for x in 0..w {
            let sx = x as i32 + tx;
            if sx < 0 || sx >= w as i32 {
                continue;
            }
            out[y * w + x] = src[sy as usize * w + sx as usize];
        }
    }
    out
}

#[test]
fn harris_too_small_returns_empty() {
    assert!(harris(0, 0, &[], 10, 5.0, None).is_empty());
    assert!(harris(1, 50, &[0.5f32; 50], 10, 5.0, None).is_empty());
    assert!(harris(32, 32, &vec![0.5f32; 32 * 32], 10, 5.0, None).is_empty());
    assert!(harris(32, 33, &vec![0.5f32; 32 * 33], 10, 5.0, None).is_empty());
}

#[test]
fn harris_constant_image_returns_empty() {
    let (w, h) = (64, 64);
    let img = vec![0.6f32; w * h];
    assert!(harris(w, h, &img, 50, 5.0, None).is_empty());
}

#[test]
fn harris_square_corners_found() {
    let (w, h) = (80, 80);
    let mut img = vec![0.0f32; w * h];
    for y in 30..50 {
        for x in 30..50 {
            img[y * w + x] = 1.0;
        }
    }
    let corners = harris(w, h, &img, 20, 5.0, None);
    assert!(corners.len() >= 4, "{corners:?}");
    assert!(corners.iter().all(|c| c.score.is_finite() && c.x.is_finite() && c.y.is_finite()));
    for (x, y) in [(30.0, 30.0), (49.0, 49.0)] {
        assert!(corners.iter().any(|c| (c.x - x).abs() <= 2.0 && (c.y - y).abs() <= 2.0), "corner near ({x},{y}): {corners:?}");
    }
}

#[test]
fn harris_min_dist_and_max_respected() {
    let (w, h) = (80, 80);
    let mut img = vec![0.0f32; w * h];
    for y in 30..50 {
        for x in 30..50 {
            img[y * w + x] = 1.0;
        }
    }
    let c_max = harris(w, h, &img, 1, 1.0, None);
    assert_eq!(c_max.len(), 1);

    let c_spaced = harris(w, h, &img, 10, 80.0, None);
    assert!(c_spaced.len() <= 1);
}

#[test]
fn harris_valid_all_true_matches_none() {
    let (w, h) = (80, 80);
    let mut img = vec![0.0f32; w * h];
    for y in 30..50 {
        for x in 30..50 {
            img[y * w + x] = 1.0;
        }
    }
    let valid = vec![true; w * h];
    let c_none = harris(w, h, &img, 20, 5.0, None);
    let c_valid = harris(w, h, &img, 20, 5.0, Some(&valid));
    assert_eq!(c_none, c_valid);
}

#[test]
fn describe_empty_corners_returns_empty() {
    let (w, h) = (64, 64);
    let img = vec![0.5f32; w * h];
    assert!(describe(w, h, &img, &[], false).is_empty());
    assert!(describe(w, h, &img, &[], true).is_empty());
}

#[test]
fn describe_deterministic() {
    let (w, h) = (80, 80);
    let img = block_texture(w, h);
    let corners = harris(w, h, &img, 30, 6.0, None);
    assert!(!corners.is_empty());
    let d1 = describe(w, h, &img, &corners, false);
    let d2 = describe(w, h, &img, &corners, false);
    assert_eq!(d1, d2);
    assert_eq!(d1.len(), corners.len());
}

#[test]
fn describe_extreme_corners_do_not_panic() {
    let (w, h) = (64, 64);
    let img = block_texture(w, h);
    let corners = vec![Corner { x: 0.0, y: 0.0, score: 0.0 }, Corner { x: (w - 1) as f32, y: (h - 1) as f32, score: 0.0 }];
    let d1 = describe(w, h, &img, &corners, false);
    let d2 = describe(w, h, &img, &corners, false);
    assert_eq!(d1.len(), 2);
    assert_eq!(d1, d2);
}

#[test]
fn hamming_known_distances() {
    let zero: Descriptor = [0, 0, 0, 0];
    let full: Descriptor = [u64::MAX; 4];
    assert_eq!(hamming(&zero, &zero), 0);
    assert_eq!(hamming(&full, &full), 0);
    assert_eq!(hamming(&zero, &full), 256);
    assert_eq!(hamming(&full, &zero), 256);

    let a: Descriptor = [0b1010, 0, 0, 0];
    let b: Descriptor = [0b0110, 0, 0, 0];
    assert_eq!(hamming(&a, &b), 2);
    assert_eq!(hamming(&a, &b), hamming(&b, &a));
}

#[test]
fn match_descriptors_empty_sets() {
    assert!(match_descriptors(&[], &[], 0.8).is_empty());
    let a: Vec<Descriptor> = vec![[0, 0, 0, 0]];
    assert!(match_descriptors(&a, &[], 0.8).is_empty());
    assert!(match_descriptors(&[], &a, 0.8).is_empty());
}

#[test]
fn match_descriptors_unique_matches() {
    let a: Vec<Descriptor> = vec![[0, 0, 0, 0], [u64::MAX; 4]];
    let b: Vec<Descriptor> = vec![[0, 0, 0, 0], [u64::MAX; 4]];
    let mut matches = match_descriptors(&a, &b, 0.8);
    matches.sort();
    assert_eq!(matches, vec![(0, 0), (1, 1)]);
}

#[test]
fn match_descriptors_ambiguous_ratio_rejects() {
    let a: Vec<Descriptor> = vec![[0, 0, 0, 0]];
    let b: Vec<Descriptor> = vec![[1, 0, 0, 0], [2, 0, 0, 0]];
    assert!(match_descriptors(&a, &b, 0.85).is_empty());
}

#[test]
fn match_descriptors_clear_match_passes() {
    let a: Vec<Descriptor> = vec![[0, 0, 0, 0]];
    let b: Vec<Descriptor> = vec![[0, 0, 0, 0], [u64::MAX; 4]];
    assert_eq!(match_descriptors(&a, &b, 0.5), vec![(0, 0)]);
}

#[test]
fn fit_translation() {
    assert!(fit(Model::Translation, &[], &[]).is_none());

    let h = fit(Model::Translation, &[[1.0, 2.0]], &[[4.0, 6.0]]).unwrap();
    let (x, y) = h.apply(1.0, 2.0);
    assert!((x - 4.0).abs() < 1e-12 && (y - 6.0).abs() < 1e-12);
}

#[test]
fn fit_similarity_requires_two_points() {
    assert!(fit(Model::Similarity, &[], &[]).is_none());
    assert!(fit(Model::Similarity, &[[0.0, 0.0]], &[[1.0, 1.0]]).is_none());

    let h = fit(Model::Similarity, &[[0.0, 0.0], [1.0, 0.0]], &[[1.0, 3.0], [3.0, 3.0]]).unwrap();
    let (x, y) = h.apply(0.0, 0.0);
    assert!((x - 1.0).abs() < 1e-9 && (y - 3.0).abs() < 1e-9);
    let (x, y) = h.apply(1.0, 0.0);
    assert!((x - 3.0).abs() < 1e-9 && (y - 3.0).abs() < 1e-9);
}

#[test]
fn fit_homography_requires_four_points() {
    assert!(fit(Model::Homography, &[], &[]).is_none());
    assert!(fit(Model::Homography, &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],).is_none());
}

#[test]
fn fit_homography_clean_exact() {
    let truth = [1.1, 0.05, 3.0, -0.02, 0.95, -2.0, 0.001, 0.0005, 1.0];
    let src = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [5.0, 3.0]];
    let dst: Vec<[f64; 2]> = src.iter().map(|p| apply_matrix(truth, p[0], p[1])).collect();
    let h = fit(Model::Homography, &src, &dst).unwrap();

    for &p in &src {
        let (a, b) = h.apply(p[0], p[1]);
        let [c, d] = apply_matrix(truth, p[0], p[1]);
        assert!((a - c).abs() < 1e-6 && (b - d).abs() < 1e-6);
    }
}

#[test]
fn ransac_insufficient_points() {
    assert!(ransac(Model::Translation, &[], &[], 10, 1.0, 0).is_none());
    assert!(ransac(Model::Similarity, &[[0.0, 0.0]], &[[1.0, 1.0]], 10, 1.0, 0).is_none());
    assert!(ransac(Model::Homography, &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], 10, 1.0, 0,).is_none());
}

#[test]
fn ransac_translation_all_inliers() {
    let src: Vec<[f64; 2]> = (0..20).map(|i| [i as f64, (i % 5) as f64]).collect();
    let dst: Vec<[f64; 2]> = src.iter().map(|p| [p[0] + 5.0, p[1] - 3.0]).collect();

    let (h, inliers) = ransac(Model::Translation, &src, &dst, 30, 0.5, 7).unwrap();
    assert_eq!(inliers.len(), src.len());

    let (x, y) = h.apply(0.0, 0.0);
    assert!((x - 5.0).abs() < 1e-9 && (y + 3.0).abs() < 1e-9);
}

#[test]
fn ransac_deterministic_for_seed() {
    let src: Vec<[f64; 2]> = (0..15).map(|i| [i as f64, (i % 4) as f64]).collect();
    let dst: Vec<[f64; 2]> = src.iter().map(|p| [p[0] + 2.0, p[1] - 1.0]).collect();

    let (h1, inliers1) = ransac(Model::Translation, &src, &dst, 10, 1.0, 9).unwrap();
    let (h2, inliers2) = ransac(Model::Translation, &src, &dst, 10, 1.0, 9).unwrap();

    assert_eq!(inliers1, inliers2);
    let (x1, y1) = h1.apply(0.0, 0.0);
    let (x2, y2) = h2.apply(0.0, 0.0);
    assert!((x1 - x2).abs() < 1e-12 && (y1 - y2).abs() < 1e-12);
}

#[test]
fn register_too_small_returns_none() {
    let (w, h) = (10, 10);
    let img = vec![0.5f32; w * h];
    assert!(register(w, h, &img, &img, None, None, Model::Translation).is_none());
}

#[test]
fn register_constant_returns_none() {
    let (w, h) = (64, 64);
    let img = vec![0.5f32; w * h];
    assert!(register(w, h, &img, &img, None, None, Model::Translation).is_none());
}

#[test]
fn register_translation_alignment() {
    let (w, h) = (96, 96);
    let reference = block_texture(w, h);

    let tx = 4;
    let ty = -3;
    let moving = translate_image(&reference, w, h, tx, ty);

    let result = register(w, h, &reference, &moving, None, None, Model::Translation);
    assert!(result.is_some(), "registration failed");
    let (model, inliers) = result.unwrap();

    assert!(inliers >= 6);
    let (x, y) = model.apply(40.0, 40.0);
    assert!((x - (40.0 + tx as f64)).abs() < 2.0 && (y - (40.0 + ty as f64)).abs() < 2.0, "model {model:?} maps (40,40) to ({x},{y})");
}

// This currently panics because ransac does not validate that source and
// destination slices have the same length before indexing them.
#[test]
fn ransac_mismatched_lengths_does_not_panic() {
    let src = vec![[0.0, 0.0], [1.0, 1.0]];
    let dst = vec![[0.0, 0.0]];
    let _ = ransac(Model::Translation, &src, &dst, 1, 1.0, 0);
}
