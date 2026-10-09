//! Integration coverage for the public `inpaint` and `seamless` APIs.
//!
//! The tests exercise edge cases, deterministic behaviour, invariants,
//! error paths and a couple of known limitations (marked `#[ignore]`).

use photocraft_algo::inpaint::{self, CompleteParams};
use photocraft_algo::seamless;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn disc_mask(w: usize, h: usize, cx: f64, cy: f64, r: f64) -> Vec<bool> {
    (0..w * h).map(|i| ((i % w) as f64 - cx).hypot((i / w) as f64 - cy) <= r).collect()
}

fn small_gradient(w: usize, h: usize, ch: usize) -> Vec<f32> {
    (0..w * h)
        .flat_map(|i| {
            let x = (i % w) as f32;
            let y = (i / w) as f32;
            (0..ch).map(|c| 0.1 * (c as f32 + 1.0) + 0.01 * x + 0.005 * y).collect::<Vec<_>>()
        })
        .collect()
}

fn assert_all_finite(data: &[f32]) {
    assert!(data.iter().all(|v| v.is_finite()), "non-finite value found");
}

#[test]
fn complete_empty_hole_returns_input() {
    let (w, h, ch) = (16, 16, 3);
    let img = small_gradient(w, h, ch);
    let hole = vec![false; w * h];
    let out = inpaint::complete(w, h, ch, &img, &hole, &CompleteParams::default()).unwrap();
    assert_eq!(out, img);
}

#[test]
fn complete_all_hole_returns_none() {
    let (w, h, ch) = (8, 8, 1);
    let img = vec![0.5; w * h * ch];
    let hole = vec![true; w * h];
    assert!(inpaint::complete(w, h, ch, &img, &hole, &CompleteParams::default()).is_none());
}

#[test]
fn complete_tiny_1x1_hole_returns_none() {
    let img = vec![0.5];
    let hole = vec![true];
    assert!(inpaint::complete(1, 1, 1, &img, &hole, &CompleteParams::default()).is_none());
}

#[test]
fn complete_outside_hole_unchanged() {
    let (w, h, ch) = (32, 32, 3);
    let img = small_gradient(w, h, ch);
    let mut hole = vec![false; w * h];
    // A small 5x5 hole in the centre.
    for y in 14..19 {
        for x in 14..19 {
            hole[y * w + x] = true;
        }
    }
    let out = inpaint::complete(w, h, ch, &img, &hole, &CompleteParams::default()).unwrap();
    for (i, is_hole) in hole.iter().enumerate() {
        if !is_hole {
            for c in 0..ch {
                assert_eq!(out[i * ch + c], img[i * ch + c], "outside pixel changed");
            }
        }
    }
}

#[test]
fn complete_is_deterministic() {
    let (w, h, ch) = (24, 24, 1);
    let img: Vec<f32> = (0..w * h).map(|i| ((i % w) as f32 * 0.3).sin() * 0.5 + 0.5).collect();
    let hole = disc_mask(w, h, 12.0, 12.0, 6.0);
    let p = CompleteParams::default();
    let out1 = inpaint::complete(w, h, ch, &img, &hole, &p).unwrap();
    let out2 = inpaint::complete(w, h, ch, &img, &hole, &p).unwrap();
    assert_eq!(out1, out2);
}

#[test]
fn complete_produces_finite_output() {
    let (w, h, ch) = (28, 28, 3);
    let img = small_gradient(w, h, ch);
    let hole = disc_mask(w, h, 14.0, 14.0, 5.0);
    let out = inpaint::complete(w, h, ch, &img, &hole, &CompleteParams::default()).unwrap();
    assert_eq!(out.len(), w * h * ch);
    assert_all_finite(&out);
}

#[test]
fn best_offset_no_fit_returns_none() {
    // 3x3 image, hole in the centre, max_radius=0 -> only (0,0) candidate which is disallowed.
    let img = vec![0.5; 9];
    let hole = {
        let mut m = vec![false; 9];
        m[4] = true;
        m
    };
    assert_eq!(inpaint::best_offset(3, 3, 1, &img, &hole, 1, 0), None);
}

#[test]
fn best_offset_finds_matching_periodic_offset() {
    let (w, h, ch) = (40, 40, 1);
    let img: Vec<f32> = (0..w * h).map(|i| ((i % w) as f32 * std::f32::consts::TAU / 10.0).sin() * 0.4 + 0.5).collect();
    let hole = disc_mask(w, h, 20.0, 20.0, 4.0);
    let (dx, dy) = inpaint::best_offset(w, h, ch, &img, &hole, 2, 10).unwrap();
    assert_eq!(dx.rem_euclid(10), 0, "dx should be a multiple of 10, got {dx}");
    assert_ne!((dx, dy), (0, 0), "should find a non-zero offset");
}

#[test]
fn synthesize_empty_hole_returns_input() {
    let (w, h, ch) = (20, 20, 3);
    let img = small_gradient(w, h, ch);
    let hole = vec![false; w * h];
    let out = inpaint::synthesize(w, h, ch, &img, &hole, 8, 1);
    assert_eq!(out, img);
}

#[test]
fn synthesize_all_hole_returns_finite_fallback() {
    let (w, h, ch) = (16, 16, 1);
    let img = vec![0.3; w * h * ch];
    let hole = vec![true; w * h];
    let out = inpaint::synthesize(w, h, ch, &img, &hole, 8, 5);
    assert_eq!(out.len(), w * h * ch);
    assert_all_finite(&out);
}

#[test]
fn synthesize_is_deterministic_for_seed() {
    let (w, h, ch) = (24, 24, 1);
    let img = small_gradient(w, h, ch);
    let hole = disc_mask(w, h, 12.0, 12.0, 5.0);
    let a = inpaint::synthesize(w, h, ch, &img, &hole, 6, 123);
    let b = inpaint::synthesize(w, h, ch, &img, &hole, 6, 123);
    assert_eq!(a, b);
}

#[test]
fn mvc_membrane_empty_mask_returns_zeros() {
    let (w, h, ch) = (12, 12, 2);
    let mask = vec![false; w * h];
    let diff = vec![0.0; w * h * ch];
    let m = seamless::mvc_membrane(w, h, &mask, &diff, ch);
    assert_eq!(m.len(), w * h * ch);
    assert!(m.iter().all(|v| *v == 0.0));
}

#[test]
fn mvc_membrane_zero_dimensions_returns_empty() {
    let m = seamless::mvc_membrane(0, 10, &[], &[], 3);
    assert!(m.is_empty());
    let m = seamless::mvc_membrane(10, 0, &[], &[], 3);
    assert!(m.is_empty());
}

#[test]
fn mvc_membrane_insufficient_diff_returns_zeros() {
    let (w, h, ch) = (4, 4, 3);
    let mask = vec![true; w * h];
    let diff = vec![1.0; 10]; // shorter than w*h*ch=48
    let m = seamless::mvc_membrane(w, h, &mask, &diff, ch);
    assert_eq!(m.len(), w * h * ch);
    assert!(m.iter().all(|v| *v == 0.0));
}

#[test]
fn mvc_membrane_reproduces_linear_diff() {
    let (w, h) = (32, 32);
    let mask = disc_mask(w, h, 16.0, 16.0, 12.0);
    let diff: Vec<f32> = (0..w * h).map(|i| 0.01 * (i % w) as f32 - 0.004 * (i / w) as f32).collect();
    let m = seamless::mvc_membrane(w, h, &mask, &diff, 1);
    let mut max_err = 0.0f32;
    for (i, inside) in mask.iter().enumerate() {
        if *inside {
            max_err = max_err.max((m[i] - diff[i]).abs());
        }
    }
    assert!(max_err < 0.01, "linear precision too low: {max_err}");
}

#[test]
#[ignore = "BUG: mvc_membrane clamps ch to 8 and returns a shorter buffer than requested"]
fn mvc_membrane_ch_greater_than_eight_respects_requested_length() {
    let (w, h, ch) = (8, 8, 10);
    let mask = vec![true; w * h];
    let diff = vec![0.5; w * h * ch];
    let m = seamless::mvc_membrane(w, h, &mask, &diff, ch);
    assert_eq!(m.len(), w * h * ch, "expected full requested length");
}

#[test]
fn seamless_blend_identity_when_src_equals_dst() {
    let (w, h, n) = (16, 16, 4);
    let src = small_gradient(w, h, n);
    let dst = src.clone();
    let mask = disc_mask(w, h, 8.0, 8.0, 6.0);
    let out = seamless::seamless_blend(w, h, n, n, &src, &dst, &mask);
    assert_eq!(out, src, "identity blend should not modify");
}

#[test]
fn seamless_blend_mask_all_false_returns_src() {
    let (w, h, n) = (10, 10, 3);
    let src = small_gradient(w, h, n);
    let dst = small_gradient(w, h, n);
    let mask = vec![false; w * h];
    let out = seamless::seamless_blend(w, h, n, n, &src, &dst, &mask);
    assert_eq!(out, src);
}

#[test]
fn seamless_blend_constant_offset_matches_border() {
    let (w, h) = (48, 32);
    let n = 4;
    let dst: Vec<f32> = (0..w * h)
        .flat_map(|i| {
            let x = (i % w) as f32;
            let y = (i / w) as f32;
            [0.4 + 0.1 * (x * 0.2).sin(), 0.6 + 0.05 * (y * 0.3).cos(), 0.2, 1.0]
        })
        .collect();
    let src: Vec<f32> = dst.chunks_exact(n).flat_map(|p| [p[0] + 0.3, p[1] - 0.2, p[2] + 0.1, p[3]]).collect();
    let mask = disc_mask(w, h, (w as f64) / 2.0, (h as f64) / 2.0, 12.0);
    let out = seamless::seamless_blend(w, h, n, 3, &src, &dst, &mask);
    let mut max_err = 0.0f32;
    for (i, inside) in mask.iter().enumerate() {
        if *inside {
            for c in 0..3 {
                max_err = max_err.max((out[i * n + c] - dst[i * n + c]).abs());
            }
        }
    }
    assert!(max_err < 1e-3, "blend should match target near border, max err {max_err}");
}

#[test]
#[ignore = "BUG: seamless_blend panics on truncated src/dst buffers"]
fn seamless_blend_truncated_input_does_not_panic() {
    let (w, h, n) = (8, 8, 3);
    let src = vec![0.5; 10]; // much shorter than w*h*n
    let dst = vec![0.5; 10];
    let mask = vec![true; w * h];
    let result = catch_unwind(AssertUnwindSafe(|| seamless::seamless_blend(w, h, n, 3, &src, &dst, &mask)));
    assert!(result.is_ok(), "expected no panic for malformed input, got: {result:?}");
}
