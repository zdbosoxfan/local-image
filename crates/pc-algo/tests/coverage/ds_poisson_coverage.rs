//! Integration tests for the public API of `photocraft_algo::poisson`.
//!
//! Covers edge cases, invariants, determinism, and the reference solver.
//! Synthetic data only; no I/O, no network.

use photocraft_algo::poisson;
use photocraft_raster::Interrupt;

/// Disc-shaped mask (`true` inside), matching the unit test helper.
fn disc_mask(w: usize, h: usize, cx: f32, cy: f32, r: f32) -> Vec<bool> {
    (0..w * h)
        .map(|i| {
            let x = (i % w) as f32;
            let y = (i / w) as f32;
            (x - cx).hypot(y - cy) < r
        })
        .collect()
}

/// Interleaved buffer filled with a linear function `0.2 + 0.005*x + 0.003*y`.
fn linear_buffer(w: usize, h: usize, ch: usize) -> Vec<f32> {
    let mut data = Vec::with_capacity(w * h * ch);
    for i in 0..w * h {
        let x = (i % w) as f32;
        let y = (i / w) as f32;
        let v = 0.2 + 0.005 * x + 0.003 * y;
        for _ in 0..ch {
            data.push(v);
        }
    }
    data
}

#[test]
fn solve_membrane_empty_grid_returns_without_panic() {
    let mut v: Vec<f32> = Vec::new();
    poisson::solve_membrane(0, 0, &[], &mut v);
    assert!(v.is_empty());
}

#[test]
fn solve_membrane_no_unknown_leaves_data_unchanged() {
    let (w, h) = (8, 6);
    let unknown = vec![false; w * h];
    let original: Vec<f32> = (0..w * h).map(|i| (i as f32 * 0.1).sin()).collect();
    let mut v = original.clone();
    poisson::solve_membrane(w, h, &unknown, &mut v);
    assert_eq!(v, original);
}

#[test]
fn solve_membrane_single_unknown_pixel_stays_zero() {
    let (w, h) = (1, 1);
    let unknown = [true];
    let mut v = [0.0f32];
    poisson::solve_membrane(w, h, &unknown, &mut v);
    assert_eq!(v[0], 0.0);
}

#[test]
fn solve_membrane_reproduces_linear_solution() {
    // A linear function is harmonic: the solve must reproduce it exactly inside the hole.
    let (w, h) = (81, 63); // odd dimensions
    let unknown = disc_mask(w, h, 40.0, 31.0, 20.0);
    let truth: Vec<f32> = (0..w * h).map(|i| 0.2 + 0.005 * (i % w) as f32 + 0.003 * (i / w) as f32).collect();
    let mut v: Vec<f32> = truth.iter().zip(&unknown).map(|(t, u)| if *u { 0.0 } else { *t }).collect();
    poisson::solve_membrane(w, h, &unknown, &mut v);
    let max_err = v.iter().zip(&truth).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(max_err < 2e-3, "max error vs truth: {max_err}");
}

#[test]
fn solve_membrane_preserves_known_pixels() {
    let (w, h) = (10, 7);
    let unknown = disc_mask(w, h, 5.0, 3.5, 3.0);
    let original: Vec<f32> = (0..w * h).map(|i| ((i % w) as f32 * 0.17).cos() * 0.4 + 0.5).collect();
    let mut v: Vec<f32> = original.iter().zip(&unknown).map(|(x, u)| if *u { 0.0 } else { *x }).collect();
    poisson::solve_membrane(w, h, &unknown, &mut v);
    for (i, (a, b)) in v.iter().zip(&original).enumerate() {
        if !unknown[i] {
            assert_eq!(a, b, "known pixel {i} changed");
        }
    }
}

#[test]
fn solve_membrane_outputs_are_finite_for_finite_boundary() {
    let (w, h) = (12, 9);
    let unknown = disc_mask(w, h, 6.0, 4.5, 4.0);
    let mut v: Vec<f32> = (0..w * h)
        .map(|i| {
            let x = (i % w) as f32;
            let y = (i / w) as f32;
            if unknown[i] { 0.0 } else { (x * 0.321).sin() + (y * 0.123).cos() }
        })
        .collect();
    poisson::solve_membrane(w, h, &unknown, &mut v);
    assert!(v.iter().all(|x| x.is_finite()), "solver produced non-finite value");
}

#[test]
fn solve_membrane_nan_boundary_propagates_without_panic() {
    let (w, h) = (6, 6);
    let mut unknown = vec![false; w * h];
    // Put a small hole in the middle.
    for y in 2..4 {
        for x in 2..4 {
            unknown[y * w + x] = true;
        }
    }
    let mut v = vec![0.0f32; w * h];
    // Set a known boundary pixel to NaN.
    v[0] = f32::NAN;
    poisson::solve_membrane(w, h, &unknown, &mut v);
    // Known NaN should remain NaN; no panic.
    assert!(v[0].is_nan(), "known NaN was modified");
    // The NaN propagates into at least one unknown pixel (real behaviour of this solver).
    let any_unknown_nan = (0..w * h).any(|i| unknown[i] && v[i].is_nan());
    assert!(any_unknown_nan, "NaN did not propagate to unknown pixels");
}

#[test]
fn solve_membrane_infinite_boundary_propagates_without_panic() {
    let (w, h) = (6, 6);
    let mut unknown = vec![false; w * h];
    for y in 2..4 {
        for x in 2..4 {
            unknown[y * w + x] = true;
        }
    }
    let mut v = vec![0.0f32; w * h];
    v[0] = f32::INFINITY;
    poisson::solve_membrane(w, h, &unknown, &mut v);
    assert_eq!(v[0], f32::INFINITY, "known infinity was modified");
    let any_unknown_nonfinite = (0..w * h).any(|i| unknown[i] && !v[i].is_finite());
    assert!(any_unknown_nonfinite, "infinity did not affect unknown pixels");
}

#[test]
fn seamless_clone_empty_mask_returns_destination() {
    let (w, h, ch) = (5, 4, 3);
    let mask = vec![false; w * h];
    let dst: Vec<f32> = (0..w * h * ch).map(|i| i as f32 * 0.01).collect();
    let src: Vec<f32> = vec![0.5; w * h * ch];
    let out = poisson::seamless_clone(w, h, ch, &src, &dst, &mask);
    assert_eq!(out, dst);
}

#[test]
fn seamless_clone_full_mask_returns_source() {
    let (w, h, ch) = (5, 5, 4);
    let mask = vec![true; w * h];
    let dst: Vec<f32> = vec![0.9; w * h * ch];
    let src: Vec<f32> = (0..w * h * ch).map(|i| (i % 7) as f32 * 0.1).collect();
    let out = poisson::seamless_clone(w, h, ch, &src, &dst, &mask);
    assert_eq!(out, src);
}

#[test]
fn seamless_clone_single_channel_odd_dims_outside_matches_dst() {
    let (w, h, ch) = (7, 5, 1);
    let mask = disc_mask(w, h, 3.0, 2.0, 2.0);
    let dst: Vec<f32> = (0..w * h).map(|i| (i as f32 * 0.05).sin()).collect();
    let src: Vec<f32> = vec![0.3; w * h];
    let out = poisson::seamless_clone(w, h, ch, &src, &dst, &mask);
    for i in 0..w * h {
        if !mask[i] {
            assert_eq!(out[i], dst[i], "pixel {i} outside mask changed");
        }
    }
}

#[test]
fn seamless_clone_output_length_and_finite_values() {
    let (w, h, ch) = (9, 8, 3);
    let mask = disc_mask(w, h, 4.5, 4.0, 3.5);
    let dst = vec![0.6; w * h * ch];
    let src = vec![0.2; w * h * ch];
    let out = poisson::seamless_clone(w, h, ch, &src, &dst, &mask);
    assert_eq!(out.len(), w * h * ch);
    assert!(out.iter().all(|v| v.is_finite()), "output contains non-finite value");
}

#[test]
fn seamless_clone_mean_inside_matches_destination() {
    let (w, h, ch) = (12, 10, 3);
    let mask = disc_mask(w, h, 6.0, 5.0, 4.0);
    let dst: Vec<f32> = (0..w * h).flat_map(|_| [0.7, 0.3, 0.2]).collect();
    let src: Vec<f32> = (0..w * h).flat_map(|_| [0.1, 0.2, 0.8]).collect();
    let out = poisson::seamless_clone(w, h, ch, &src, &dst, &mask);
    let inside: Vec<usize> = (0..w * h).filter(|&i| mask[i]).collect();
    let inside_count = inside.len() as f32;
    for c in 0..ch {
        let mean = inside.iter().map(|&i| out[i * ch + c]).sum::<f32>() / inside_count;
        assert!((mean - dst[c]).abs() < 0.05, "channel {c} mean {mean} too far from dst {}", dst[c]);
    }
}

#[test]
fn membrane_fill_empty_hole_returns_image() {
    let (w, h, ch) = (7, 6, 4);
    let img: Vec<f32> = (0..w * h * ch).map(|i| (i as f32 * 0.2).tanh()).collect();
    let hole = vec![false; w * h];
    let out = poisson::membrane_fill(w, h, ch, &img, &hole);
    assert_eq!(out, img);
}

#[test]
fn membrane_fill_full_hole_returns_zeros() {
    let (w, h, ch) = (5, 5, 2);
    let img = vec![0.8; w * h * ch];
    let hole = vec![true; w * h];
    let out = poisson::membrane_fill(w, h, ch, &img, &hole);
    assert_eq!(out, vec![0.0; w * h * ch]);
}

#[test]
fn membrane_fill_reproduces_linear_data() {
    let (w, h, ch) = (10, 8, 2);
    let img = linear_buffer(w, h, ch);
    let hole = disc_mask(w, h, 5.0, 4.0, 3.0);
    let out = poisson::membrane_fill(w, h, ch, &img, &hole);
    let max_err = out.iter().zip(&img).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(max_err < 2e-3, "max error vs linear truth: {max_err}");
}

#[test]
fn membrane_fill_with_none_interrupt_matches_fill() {
    let (w, h, ch) = (9, 7, 3);
    let img: Vec<f32> = (0..w * h * ch).map(|i| ((i % 13) as f32 * 0.1).sin()).collect();
    let hole = disc_mask(w, h, 4.5, 3.5, 2.5);
    let expected = poisson::membrane_fill(w, h, ch, &img, &hole);
    let result = poisson::membrane_fill_with(w, h, ch, &img, &hole, &Interrupt::NONE);
    let actual = result.expect("membrane_fill_with should return Ok");
    assert_eq!(actual, expected);
}

#[test]
fn solve_membrane_is_deterministic() {
    let (w, h) = (11, 9);
    let unknown = disc_mask(w, h, 5.5, 4.5, 3.5);
    let base: Vec<f32> = (0..w * h).map(|i| ((i % w) as f32 * 0.133).cos() * 0.3 + ((i / w) as f32 * 0.171).sin() * 0.2).collect();
    let make_v = || -> Vec<f32> { base.iter().zip(&unknown).map(|(x, u)| if *u { 0.0 } else { *x }).collect() };
    let mut v1 = make_v();
    let mut v2 = make_v();
    poisson::solve_membrane(w, h, &unknown, &mut v1);
    poisson::solve_membrane(w, h, &unknown, &mut v2);
    assert_eq!(v1, v2);
}

#[test]
fn seamless_clone_is_deterministic() {
    let (w, h, ch) = (10, 8, 3);
    let mask = disc_mask(w, h, 5.0, 4.0, 3.5);
    let dst: Vec<f32> = (0..w * h).flat_map(|_| [0.6, 0.3, 0.1]).collect();
    let src: Vec<f32> = (0..w * h).flat_map(|_| [0.2, 0.4, 0.7]).collect();
    let out1 = poisson::seamless_clone(w, h, ch, &src, &dst, &mask);
    let out2 = poisson::seamless_clone(w, h, ch, &src, &dst, &mask);
    assert_eq!(out1, out2);
}

#[test]
fn membrane_fill_is_deterministic() {
    let (w, h, ch) = (8, 8, 2);
    let img: Vec<f32> = (0..w * h * ch).map(|i| (i as f32 * 0.71).sin()).collect();
    let hole = disc_mask(w, h, 4.0, 4.0, 2.5);
    let out1 = poisson::membrane_fill(w, h, ch, &img, &hole);
    let out2 = poisson::membrane_fill(w, h, ch, &img, &hole);
    assert_eq!(out1, out2);
}
