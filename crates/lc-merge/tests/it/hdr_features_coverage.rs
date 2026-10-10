use lightcraft_merge::{
    MergeError,
    features::{Features, detect, match_features},
    frame::{Frame, FrameColor},
    hdr::{Deghost, HdrOptions, merge_hdr},
    no_progress,
};
use lightcraft_raster::{Plane, Rgb32f};

fn make_frame(w: usize, h: usize, value: [f32; 3], clip: f32, exposure: Option<f64>) -> Frame {
    let mut image = Rgb32f::new(w, h);
    for p in image.data.iter_mut() {
        *p = value;
    }
    Frame {
        image,
        clip,
        exposure,
        color: FrameColor::Linear { to_xyz_d50: lightcraft_color::Mat3::IDENTITY },
        raw: false,
        orientation: Default::default(),
        metadata: Default::default(),
        baseline_exposure: 0.0,
    }
}

fn simple_opts() -> HdrOptions {
    HdrOptions { align: false, deghost: Deghost::None }
}

#[test]
fn merge_two_identical_frames_keeps_data() {
    let f1 = make_frame(8, 8, [0.4, 0.5, 0.6], 1.0, Some(0.0));
    let f2 = make_frame(8, 8, [0.4, 0.5, 0.6], 1.0, Some(0.0));
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert_eq!(res.radiance.width, 8);
    assert_eq!(res.radiance.height, 8);
    for p in res.radiance.data.iter() {
        assert!((p[0] - 0.4).abs() < 1e-6, "red {}", p[0]);
        assert!((p[1] - 0.5).abs() < 1e-6, "green {}", p[1]);
        assert!((p[2] - 0.6).abs() < 1e-6, "blue {}", p[2]);
    }
    assert_eq!(res.reference, 1);
    assert_eq!(res.ev.len(), 2);
    assert!(res.ev.iter().all(|v| v.abs() < 1e-5));
    assert_eq!(res.ghost.width, 0);
    assert_eq!(res.alignments.len(), 2);
}

#[test]
fn merge_requires_two_frames() {
    let f = make_frame(4, 4, [0.5; 3], 1.0, None);
    let err = merge_hdr(vec![f], &simple_opts(), &no_progress).unwrap_err();
    assert!(matches!(err, MergeError::TooFew(1, 2)));
}

#[test]
fn merge_rejects_size_mismatch() {
    let f1 = make_frame(4, 4, [0.5; 3], 1.0, None);
    let f2 = make_frame(4, 5, [0.5; 3], 1.0, None);
    let err = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap_err();
    assert!(matches!(err, MergeError::Mismatch(_)));
}

#[test]
fn merge_handles_1x1() {
    let f1 = make_frame(1, 1, [0.25; 3], 1.0, Some(0.0));
    let f2 = make_frame(1, 1, [0.75; 3], 1.0, Some(0.0));
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert_eq!(res.radiance.width, 1);
    assert_eq!(res.radiance.height, 1);
    assert!(res.radiance.data[0].iter().all(|v| v.is_finite()));
}

#[test]
fn merge_handles_odd_sizes() {
    let f1 = make_frame(3, 5, [0.4; 3], 1.0, None);
    let f2 = make_frame(3, 5, [0.6; 3], 1.0, None);
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert_eq!(res.radiance.width, 3);
    assert_eq!(res.radiance.height, 5);
}

#[test]
fn merge_handles_larger_image_fast() {
    let f1 = make_frame(64, 64, [0.5; 3], 1.0, None);
    let f2 = make_frame(64, 64, [0.5; 3], 1.0, None);
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert_eq!(res.radiance.width, 64);
    assert_eq!(res.radiance.height, 64);
}

#[test]
fn merge_averages_unsaturated_pixels() {
    let f1 = make_frame(8, 8, [0.1; 3], 1.0, Some(0.0));
    let f2 = make_frame(8, 8, [0.2; 3], 1.0, Some(0.0));
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    for p in res.radiance.data.iter() {
        for c in 0..3 {
            assert!((p[c] - 0.15).abs() < 1e-3, "{}", p[c]);
        }
    }
}

#[test]
fn merge_uses_darker_frame_when_bright_frame_clips() {
    let f1 = make_frame(8, 8, [0.05; 3], 0.5, Some(0.0));
    let f2 = make_frame(8, 8, [0.5; 3], 0.5, Some(0.0));
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    for p in res.radiance.data.iter() {
        for c in 0..3 {
            assert!((p[c] - 0.05).abs() < 1e-5, "{}", p[c]);
        }
    }
}

#[test]
fn merge_normalises_exposure_ratios() {
    let f1 = make_frame(16, 16, [0.25; 3], 1.0, Some(0.0));
    let f2 = make_frame(16, 16, [0.5; 3], 1.0, Some(1.0));
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    for p in res.radiance.data.iter() {
        for c in 0..3 {
            assert!((p[c] - 0.5).abs() < 1e-3, "{}", p[c]);
        }
    }
    assert_eq!(res.ev.len(), 2);
    assert!((res.ev[0] + 1.0).abs() < 1e-3, "ev0 {}", res.ev[0]);
    assert!(res.ev[1].abs() < 1e-3, "ev1 {}", res.ev[1]);
}

#[test]
fn align_false_keeps_identity_alignments() {
    let f1 = make_frame(8, 8, [0.5; 3], 1.0, None);
    let f2 = make_frame(8, 8, [0.5; 3], 1.0, None);
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert!(res.alignments.iter().all(|a| a.model == "identity"));
}

#[test]
fn align_true_on_identical_images_does_not_change_radiance() {
    let opts = HdrOptions { align: true, deghost: Deghost::None };
    let f1 = make_frame(32, 32, [0.35; 3], 1.0, None);
    let f2 = make_frame(32, 32, [0.35; 3], 1.0, None);
    let res = merge_hdr(vec![f1, f2], &opts, &no_progress).unwrap();
    for p in res.radiance.data.iter() {
        assert!((p[0] - 0.35).abs() < 1e-3);
    }
}

#[test]
fn deghost_high_produces_overlay_for_moving_patch() {
    let mut f1 = make_frame(32, 32, [0.5; 3], 1.0, Some(0.0));
    for y in 0..5 {
        for x in 0..5 {
            f1.image.data[y * 32 + x] = [1.0; 3];
        }
    }
    let f2 = make_frame(32, 32, [0.5; 3], 1.0, Some(0.0));
    let opts = HdrOptions { align: false, deghost: Deghost::High };
    let res = merge_hdr(vec![f1, f2], &opts, &no_progress).unwrap();
    assert_eq!(res.ghost.width, 32);
    assert!(res.ghost.data.iter().any(|&v| v > 0.0));
}

#[test]
fn deghost_none_ghost_empty() {
    let mut f1 = make_frame(32, 32, [0.5; 3], 1.0, Some(0.0));
    for y in 0..5 {
        for x in 0..5 {
            f1.image.data[y * 32 + x] = [1.0; 3];
        }
    }
    let f2 = make_frame(32, 32, [0.5; 3], 1.0, Some(0.0));
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert_eq!(res.ghost.width, 0);
}

#[test]
fn cancel_via_progress_returns_cancelled() {
    let f1 = make_frame(8, 8, [0.5; 3], 1.0, None);
    let f2 = make_frame(8, 8, [0.5; 3], 1.0, None);
    fn cancel(_: f32, _: &str) -> bool {
        false
    }
    let err = merge_hdr(vec![f1, f2], &simple_opts(), &cancel).unwrap_err();
    assert!(matches!(err, MergeError::Cancelled));
}

#[test]
fn merge_preserves_reference_baseline_exposure() {
    let mut f1 = make_frame(8, 8, [0.5; 3], 1.0, Some(0.0));
    f1.baseline_exposure = 1.0;
    let mut f2 = make_frame(8, 8, [0.5; 3], 1.0, Some(0.0));
    f2.baseline_exposure = 3.5;
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert_eq!(res.reference, 1);
    assert!((res.baseline_exposure - 3.5).abs() < 1e-9);
}

#[test]
fn merge_handles_nan_input_without_panicking() {
    let mut f1 = make_frame(8, 8, [0.5; 3], 1.0, Some(0.0));
    let f2 = make_frame(8, 8, [0.5; 3], 1.0, Some(0.0));
    f1.image.data[0] = [f32::NAN, f32::NAN, f32::NAN];
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress);
    assert!(res.is_ok());
}

#[test]
fn merge_handles_infinite_input_without_panicking() {
    let mut f1 = make_frame(8, 8, [0.5; 3], 1.0, Some(0.0));
    let f2 = make_frame(8, 8, [0.5; 3], 1.0, Some(0.0));
    f1.image.data[0] = [f32::INFINITY, f32::INFINITY, f32::INFINITY];
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress);
    assert!(res.is_ok());
}

#[test]
fn detect_finds_features_in_synthetic_texture() {
    let img = Plane::from_fn(200, 150, |x, y| {
        let u = x as f32 + 0.5;
        let v = y as f32 + 0.5;
        let a = ((u * 0.071).sin() * (v * 0.053).cos()
            + (u * 0.023 + v * 0.031).sin() * 0.7
            + ((u * 0.011).floor() as i32 + (v * 0.013).floor() as i32).rem_euclid(2) as f32 * 0.6)
            * 0.25
            + 0.5;
        a.clamp(0.0, 1.0)
    });
    let f = detect(&img, 300);
    assert!(!f.is_empty());
}

#[test]
fn detect_small_image_is_empty() {
    let img = Plane::from_fn(8, 8, |_, _| 0.5);
    let f = detect(&img, 100);
    assert!(f.is_empty());
}

#[test]
fn match_features_identical_images_produce_matches() {
    let img = Plane::from_fn(160, 120, |x, y| {
        let u = x as f32 * 0.07;
        let v = y as f32 * 0.05;
        ((u.sin() * v.cos() + (u + v).sin() * 0.7) * 0.25 + 0.5).clamp(0.0, 1.0)
    });
    let a = detect(&img, 200);
    let b = detect(&img, 200);
    assert!(!a.is_empty());
    let m = match_features(&a, &b, 0.8);
    assert!(!m.is_empty());
}

#[test]
fn match_features_empty_inputs_return_empty() {
    let a = Features::default();
    let b = Features::default();
    assert!(match_features(&a, &b, 0.8).is_empty());
}

#[test]
fn match_features_shifted_texture_matches_shift() {
    let pattern = |w: usize, h: usize, dx: f32, dy: f32| {
        Plane::from_fn(w, h, |x, y| {
            let u = x as f32 + 0.5 - dx;
            let v = y as f32 + 0.5 - dy;
            let a = ((u * 0.071).sin() * (v * 0.053).cos()
                + (u * 0.023 + v * 0.031).sin() * 0.7
                + ((u * 0.011).floor() as i32 + (v * 0.013).floor() as i32).rem_euclid(2) as f32 * 0.6)
                * 0.25
                + 0.5;
            a.clamp(0.0, 1.0)
        })
    };
    let a = detect(&pattern(200, 150, 0.0, 0.0), 300);
    let b = detect(&pattern(200, 150, 7.0, -4.0), 300);
    assert!(a.len() > 10, "{} features", a.len());
    let m = match_features(&a, &b, 0.8);
    assert!(m.len() > 5, "{} matches", m.len());
    let good = m
        .iter()
        .filter(|(i, j)| {
            let p = a.points[*i];
            let q = b.points[*j];
            ((q.x - p.x - 7.0).powi(2) + (q.y - p.y + 4.0).powi(2)).sqrt() < 1.5
        })
        .count();
    assert!(good as f32 > m.len() as f32 * 0.8, "{good}/{}", m.len());
}

#[test]
fn hdr_result_and_ev_lengths_match() {
    let f1 = make_frame(6, 6, [0.45; 3], 1.0, Some(0.0));
    let f2 = make_frame(6, 6, [0.45; 3], 1.0, Some(0.0));
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert_eq!(res.ev.len(), 2);
    assert_eq!(res.alignments.len(), 2);
}

#[test]
fn merge_is_deterministic() {
    let f1 = make_frame(12, 12, [0.3; 3], 1.0, Some(0.0));
    let f2 = make_frame(12, 12, [0.6; 3], 1.0, Some(1.0));
    let a = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    let f1 = make_frame(12, 12, [0.3; 3], 1.0, Some(0.0));
    let f2 = make_frame(12, 12, [0.6; 3], 1.0, Some(1.0));
    let b = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert_eq!(a.radiance.data, b.radiance.data);
}

#[test]
fn merge_handles_large_odd_dimensions_fast() {
    let f1 = make_frame(17, 23, [0.2; 3], 1.0, None);
    let f2 = make_frame(17, 23, [0.8; 3], 1.0, None);
    let res = merge_hdr(vec![f1, f2], &simple_opts(), &no_progress).unwrap();
    assert_eq!(res.radiance.width, 17);
    assert_eq!(res.radiance.height, 23);
    assert!(res.radiance.data.iter().all(|p| p.iter().all(|v| v.is_finite())));
}
