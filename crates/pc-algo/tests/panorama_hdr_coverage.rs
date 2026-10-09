use photocraft_algo::hdr::{self, MergeOptions, ToneMethod};
use photocraft_algo::panorama::{self, AlignOptions, ImageFeatures, Layout, Motion, Placement, Projection, RoiImage};
use photocraft_algo::tone::HdrToning;
use photocraft_algo::transform::Homography;

#[test]
fn layout_parse_name_round_trip() {
    for layout in [Layout::Auto, Layout::Perspective, Layout::Cylindrical, Layout::Spherical, Layout::Collage, Layout::Reposition] {
        assert_eq!(Layout::parse(layout.name()), Some(layout));
    }
    assert_eq!(Layout::parse("bogus"), None);
}

#[test]
fn fit_motion_translation_exact() {
    let src = [[0.0, 0.0], [1.0, 2.0], [3.0, 4.0]];
    let dst: Vec<[f64; 2]> = src.iter().map(|p| [p[0] + 5.0, p[1] - 3.0]).collect();
    let h = panorama::fit_motion(Motion::Translation, &src, &dst).unwrap();
    assert!((h.0[0] - 1.0).abs() < 1e-9);
    assert!((h.0[2] - 5.0).abs() < 1e-9);
    assert!((h.0[4] - 1.0).abs() < 1e-9);
    assert!((h.0[5] + 3.0).abs() < 1e-9);
}

#[test]
fn fit_motion_similarity_exact() {
    let src = [[0.0, 0.0], [2.0, 1.0], [1.0, 3.0], [4.0, 4.0]];
    let (scale, angle) = (2.0f64, std::f64::consts::FRAC_PI_6);
    let (sin, cos) = angle.sin_cos();
    let a = scale * cos;
    let b = scale * sin;
    let dst: Vec<[f64; 2]> = src.iter().map(|p| [a * p[0] - b * p[1] + 10.0, b * p[0] + a * p[1] - 20.0]).collect();
    let h = panorama::fit_motion(Motion::Similarity, &src, &dst).unwrap();
    assert!((h.0[0] - a).abs() < 1e-6, "a {} {}", h.0[0], a);
    assert!((h.0[1] + b).abs() < 1e-6);
    assert!((h.0[2] - 10.0).abs() < 1e-6);
    assert!((h.0[3] - b).abs() < 1e-6);
    assert!((h.0[4] - a).abs() < 1e-6);
    assert!((h.0[5] + 20.0).abs() < 1e-6);
}

#[test]
fn fit_motion_euclidean_exact() {
    let src = [[0.0, 0.0], [2.0, 1.0], [1.0, 3.0]];
    let angle = std::f64::consts::FRAC_PI_4;
    let (sin, cos) = angle.sin_cos();
    let dst: Vec<[f64; 2]> = src.iter().map(|p| [cos * p[0] - sin * p[1] + 4.0, sin * p[0] + cos * p[1] - 6.0]).collect();
    let h = panorama::fit_motion(Motion::Euclidean, &src, &dst).unwrap();
    assert!((h.0[0] - cos).abs() < 1e-6);
    assert!((h.0[1] + sin).abs() < 1e-6);
    assert!((h.0[2] - 4.0).abs() < 1e-6);
    assert!((h.0[3] - sin).abs() < 1e-6);
    assert!((h.0[4] - cos).abs() < 1e-6);
    assert!((h.0[5] + 6.0).abs() < 1e-6);
}

#[test]
fn fit_motion_homography_exact() {
    let src = [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [10.0, 10.0]];
    let h_true = Homography([1.1, 0.2, 5.0, -0.1, 0.9, 7.0, 0.0005, -0.0003, 1.0]);
    let dst: Vec<[f64; 2]> = src
        .iter()
        .map(|p| {
            let (x, y) = h_true.apply(p[0], p[1]);
            [x, y]
        })
        .collect();
    let h = panorama::fit_motion(Motion::Homography, &src, &dst).unwrap();
    let k = h.0[8];
    for (i, v) in h.0.iter().enumerate() {
        let expected = h_true.0[i] / h_true.0[8];
        assert!((v / k - expected).abs() < 1e-6, "index {i}");
    }
}

#[test]
fn fit_motion_insufficient_points_returns_none() {
    assert!(panorama::fit_motion(Motion::Translation, &[], &[]).is_none());
    assert!(panorama::fit_motion(Motion::Euclidean, &[[0.0, 0.0]], &[[1.0, 1.0]]).is_none());
    assert!(panorama::fit_motion(Motion::Similarity, &[], &[]).is_none());
    assert!(panorama::fit_motion(Motion::Homography, &[], &[]).is_none());
}

#[test]
fn fit_motion_non_finite_input_does_not_panic() {
    let src = [[0.0, 0.0], [1.0, 1.0], [f64::NAN, 2.0], [3.0, 4.0]];
    let dst = [[0.0, 0.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0]];
    for motion in [Motion::Translation, Motion::Euclidean, Motion::Similarity, Motion::Homography] {
        let _ = panorama::fit_motion(motion, &src, &dst);
    }
}

#[test]
fn ransac_motion_exact_no_outliers() {
    let src: Vec<[f64; 2]> = (0..10).map(|i| [i as f64, (i % 3) as f64]).collect();
    let dst: Vec<[f64; 2]> = src.iter().map(|p| [p[0] + 7.0, p[1] - 2.0]).collect();
    let (h, inliers) = panorama::ransac_motion(Motion::Translation, &src, &dst, 50, 0.1, 42).unwrap();
    assert_eq!(inliers.len(), src.len());
    assert!((h.0[2] - 7.0).abs() < 1e-6 && (h.0[5] + 2.0).abs() < 1e-6);
}

#[test]
fn ransac_motion_insufficient_points_returns_none() {
    assert!(panorama::ransac_motion(Motion::Translation, &[], &[], 10, 1.0, 0).is_none());
    assert!(panorama::ransac_motion(Motion::Euclidean, &[[0.0, 0.0]], &[[1.0, 1.0]], 10, 1.0, 0).is_none());
}

#[test]
fn ransac_motion_deterministic_same_seed() {
    let src: Vec<[f64; 2]> = (0..20).map(|i| [i as f64, (i % 5) as f64]).collect();
    let dst: Vec<[f64; 2]> = src.iter().map(|p| [p[0] + 1.0, p[1] - 1.0]).collect();
    let a = panorama::ransac_motion(Motion::Translation, &src, &dst, 100, 0.01, 7).unwrap();
    let b = panorama::ransac_motion(Motion::Translation, &src, &dst, 100, 0.01, 7).unwrap();
    assert_eq!(a.0.0, b.0.0);
    assert_eq!(a.1, b.1);
}

#[test]
fn placement_round_trips_all_projections_with_distortion() {
    for projection in [Projection::Plane, Projection::Cylinder, Projection::Sphere] {
        let p = Placement { projection, focal: 400.0, center: [200.0, 150.0], k1: 0.05, h: Homography([0.98, -0.1, 30.0, 0.1, 0.98, -12.0, 0.0, 0.0, 1.0]) };
        for (x, y) in [(0.0, 0.0), (390.0, 20.0), (123.0, 280.0)] {
            let (u, v) = p.forward(x, y);
            let (bx, by) = p.inverse(u, v).unwrap();
            assert!((bx - x).abs() < 1e-6 && (by - y).abs() < 1e-6, "{projection:?}");
        }
    }
}

#[test]
fn placement_unproject_outside_hemisphere_returns_none() {
    for projection in [Projection::Cylinder, Projection::Sphere] {
        let p = Placement { projection, focal: 100.0, center: [50.0, 50.0], k1: 0.0, h: Homography([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]) };
        assert!(p.unproject(100.0 * 1.6, 0.0).is_none());
    }
    let plane =
        Placement { projection: Projection::Plane, focal: 100.0, center: [50.0, 50.0], k1: 0.0, h: Homography([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]) };
    assert!(plane.unproject(1000.0, -1000.0).is_some());
}

#[test]
fn placement_scaled_and_translated_are_consistent() {
    let p = Placement {
        projection: Projection::Plane,
        focal: 100.0,
        center: [50.0, 40.0],
        k1: 0.0,
        h: Homography([1.0, 0.0, 50.0, 0.0, 1.0, 40.0, 0.0, 0.0, 1.0]),
    };
    assert_eq!(p.bounds(60.0, 40.0), [0.0, 0.0, 60.0, 40.0]);

    let (u, v) = p.forward(100.0, 50.0);
    let s = p.scaled(2.0);
    let (u2, v2) = s.forward(200.0, 100.0);
    assert!((u2 - 2.0 * u).abs() < 1e-6 && (v2 - 2.0 * v).abs() < 1e-6);

    let t = p.translated(10.0, -5.0);
    let (tu, tv) = t.forward(0.0, 0.0);
    assert!((tu - 10.0).abs() < 1e-6 && (tv + 5.0).abs() < 1e-6);
}

#[test]
fn detect_small_image_is_safe() {
    let f = panorama::detect(1, 1, &[0.5], None, 10);
    assert_eq!(f.w, 1);
    assert_eq!(f.h, 1);
    assert!(f.points.is_empty());
    assert!(f.desc.is_empty());
}

#[test]
fn detect_many_empty_returns_empty() {
    let imgs: Vec<panorama::PreparedImage> = Vec::new();
    assert!(panorama::detect_many(&imgs, 100).is_empty());
}

#[test]
fn match_pairs_empty_and_too_few_points() {
    assert!(panorama::match_pairs(&[]).is_empty());

    let feats = vec![
        ImageFeatures { w: 8, h: 8, points: vec![[0.0, 0.0]; 5], desc: Vec::new() },
        ImageFeatures { w: 8, h: 8, points: vec![[1.0, 1.0]; 5], desc: Vec::new() },
    ];
    assert!(panorama::match_pairs(&feats).is_empty());
}

#[test]
fn align_returns_none_for_insufficient_input() {
    let opts = AlignOptions { layout: Layout::Auto, focal: None, reference: None, geometric: false };
    assert!(panorama::align(&[], &[], &opts).is_none());

    let one = vec![ImageFeatures { w: 8, h: 8, points: Vec::new(), desc: Vec::new() }];
    assert!(panorama::align(&one, &[], &opts).is_none());

    let two = vec![ImageFeatures { w: 8, h: 8, points: Vec::new(), desc: Vec::new() }, ImageFeatures { w: 8, h: 8, points: Vec::new(), desc: Vec::new() }];
    assert!(panorama::align(&two, &[], &opts).is_none());
}

#[test]
fn focal_from_homography_identity_returns_none() {
    let h = Homography([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    let centers = ([50.0, 50.0], [50.0, 50.0]);
    assert!(panorama::focal_from_homography(&h, centers.0, centers.1).is_none());
}

#[test]
fn photometric_identity_and_single_image() {
    let identity = panorama::photometric(&[], None, true, true);
    assert!(identity.gains.is_empty());
    assert_eq!(identity.vignette, [0.0, 0.0]);

    let img = RoiImage { x0: 0, y0: 0, w: 8, h: 8, ch: 1, px: vec![0.5; 64], alpha: vec![1.0; 64] };
    let ph = panorama::photometric(&[img], None, true, true);
    assert_eq!(ph.gains, vec![vec![1.0]]);
    assert_eq!(ph.vignette, [0.0, 0.0]);
}

#[test]
fn photometric_factor_uses_gain_and_vignette() {
    let ph = panorama::Photometric { gains: vec![vec![2.0]], vignette: [0.1, 0.0] };
    assert!((ph.factor(0, 0, 0.0) - 0.5).abs() < 1e-6);
    assert!((ph.factor(0, 0, 1.0) - 1.0 / (2.0 * (0.1f64).exp()) as f32).abs() < 1e-5);
}

#[test]
fn seam_labels_single_image_covers_all() {
    let w = 4;
    let h = 3;
    let luma = vec![vec![0.5; w * h]];
    let cover = vec![vec![true; w * h]];
    let labels = panorama::seam_labels(w, h, &luma, &cover, &[0]);
    assert_eq!(labels, vec![0i32; w * h]);
}

#[test]
fn seam_labels_empty_grid_no_panic() {
    let labels = panorama::seam_labels(0, 0, &[], &[], &[]);
    assert!(labels.is_empty());
}

#[test]
fn blend_levels_expected_for_small_and_large() {
    assert_eq!(panorama::blend_levels(1, 1), 1);
    assert_eq!(panorama::blend_levels(16, 16), 1);
    assert_eq!(panorama::blend_levels(17, 17), 2);
    assert_eq!(panorama::blend_levels(256, 256), 5);
    assert_eq!(panorama::blend_levels(512, 512), 6);
}

#[test]
fn multiband_empty_returns_zeros() {
    let (out, cov) = panorama::multiband(3, 2, &[], &[], 1);
    assert_eq!(out, vec![0.0; 6]);
    assert_eq!(cov, vec![0.0; 6]);
}

#[test]
fn multiband_single_image_levels1_identity() {
    let roi = RoiImage { x0: 0, y0: 0, w: 3, h: 2, ch: 1, px: vec![0.25; 6], alpha: vec![1.0; 6] };
    let weights = vec![vec![1.0; 6]];
    let (out, cov) = panorama::multiband(3, 2, &[roi], &weights, 1);
    assert_eq!(out.len(), 6);
    assert_eq!(cov.len(), 6);
    for (o, c) in out.iter().zip(cov.iter()) {
        assert!((o - 0.25).abs() < 1e-6);
        assert!((c - 1.0).abs() < 1e-6);
    }
}

#[test]
fn estimate_exposures_empty_and_single() {
    let empty: Vec<&[[f32; 4]]> = Vec::new();
    assert!(hdr::estimate_exposures(&empty).is_empty());

    let img = vec![[0.5, 0.5, 0.5, 1.0]; 64];
    let refs: Vec<&[[f32; 4]]> = vec![img.as_slice()];
    assert_eq!(hdr::estimate_exposures(&refs), vec![1.0]);
}

#[test]
fn mtb_offset_zero_levels_or_small_image() {
    assert_eq!(hdr::mtb_offset(32, 32, &[], &[], 0), (0, 0));
    assert_eq!(hdr::mtb_offset(15, 15, &[], &[], 4), (0, 0));
    assert_eq!(hdr::mtb_offset(32, 15, &[], &[], 4), (0, 0));
}

#[test]
#[ignore = "BUG: mtb_offset returns a nonzero offset for identical images when ties occur"]
fn mtb_offset_identical_images_is_zero() {
    let img = vec![0.5f32; 32 * 32];
    // Identical images must align to (0,0); but mtb_offset's tie-breaking
    // prefers the first searched offset, e.g. (-1,-1) when active pixels
    // disagree nowhere.
    assert_eq!(hdr::mtb_offset(32, 32, &img, &img, 4), (0, 0));
}

#[test]
fn sample_pixels_count_zero_returns_empty() {
    let middle = vec![[0.5, 0.5, 0.5, 1.0]; 16 * 16];
    let idx = hdr::sample_pixels(16, 16, &middle, 0);
    assert!(idx.is_empty());
}

#[test]
fn sample_pixels_indices_valid_unique_and_sorted() {
    let middle = vec![[0.5, 0.5, 0.5, 1.0]; 16 * 16];
    let idx = hdr::sample_pixels(16, 16, &middle, 20);
    assert!(!idx.is_empty());
    assert!(idx.len() <= 20);
    let mut prev = None;
    for &i in &idx {
        assert!(i < 16 * 16);
        if let Some(p) = prev {
            assert!(i > p);
        }
        prev = Some(i);
    }
}

#[test]
fn response_curve_empty_input_returns_fallback() {
    let g = hdr::response_curve(&[], &[], 40.0);
    assert_eq!(g.len(), 256);
    assert_eq!(g[128], 0.0);
    assert!(g.iter().all(|v| v.is_finite()));
}

#[test]
fn response_curve_single_sample_is_finite_and_monotonic() {
    let z = vec![vec![100u8]];
    let g = hdr::response_curve(&z, &[0.0], 1.0);
    assert_eq!(g.len(), 256);
    assert!(g.iter().all(|v| v.is_finite()));
    assert!(g.windows(2).all(|w| w[1] > w[0]));
}

#[test]
fn merge_single_image_returns_finite_basic_result() {
    let img = vec![[0.5, 0.5, 0.5, 1.0]; 64];
    let refs = [img.as_slice()];
    let opts = MergeOptions { exposures: vec![1.0], remove_ghosts: false, ghost_base: None, response: None };
    let m = hdr::merge(8, 8, &refs, &opts);
    assert_eq!(m.px.len(), 64);
    assert_eq!(m.response.len(), 3);
    for curve in &m.response {
        assert_eq!(curve.len(), 256);
        assert!(curve.iter().all(|v| v.is_finite()));
    }
    assert_eq!(m.ghost_base, 0);
    assert_eq!(m.ghost_fraction, 0.0);
    assert!(m.stops.is_finite());
    for q in &m.px {
        assert!(q.iter().all(|v| v.is_finite()));
        assert_eq!(q[3], 1.0);
    }
}

#[test]
#[ignore = "BUG: merge panics on empty input"]
fn merge_empty_input_does_not_panic() {
    let _ = hdr::merge(1, 1, &[], &MergeOptions { exposures: Vec::new(), remove_ghosts: false, ghost_base: None, response: None });
}

#[test]
fn tone_map_all_methods_produce_finite_display_range() {
    let w = 16;
    let h = 8;
    let base: Vec<[f32; 4]> = (0..w * h)
        .map(|i| {
            let x = (i % w) as f32 / w as f32;
            let v = 0.001 + 10.0 * x;
            [v, v * 0.8, v * 0.6, 1.0]
        })
        .collect();

    for method in [
        ToneMethod::LocalAdaptation(HdrToning { radius: 7.0, strength: 0.52, ..Default::default() }),
        ToneMethod::ExposureGamma { exposure: 0.0, gamma: 1.0 },
        ToneMethod::HighlightCompression,
        ToneMethod::EqualizeHistogram,
    ] {
        let mut px = base.clone();
        hdr::tone_map(&mut px, w, h, &method);
        for q in &px {
            for c in 0..3 {
                assert!(q[c].is_finite());
                assert!((0.0..=1.0).contains(&q[c]), "{}", q[c]);
            }
            assert_eq!(q[3], 1.0);
        }
    }
}

#[test]
fn tone_map_empty_exposure_gamma_no_panic() {
    let mut px: Vec<[f32; 4]> = Vec::new();
    hdr::tone_map(&mut px, 2, 2, &ToneMethod::ExposureGamma { exposure: 0.0, gamma: 1.0 });
    assert!(px.is_empty());
}
