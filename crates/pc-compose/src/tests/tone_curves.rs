//! Tone curves against Photoshop: modern Brightness and whole-level Levels.

use super::*;

/// Over the whole slider range the curve stays monotone and inside 0..1, lowering it undoes
/// raising it by as much, and odd input never panics.
#[test]
fn modern_brightness_is_monotone_and_lowering_inverts_raising() {
    for b in (-150..=150).step_by(5).map(|b| b as f32) {
        let mut prev = 0.0f32;
        for k in 0..=1020 {
            let v = adjust::modern_brightness(k as f32 / 1020.0, b);
            assert!((0.0..=1.0).contains(&v) && v >= prev - 1e-6, "brightness {b} at {k}/1020: {v} after {prev}");
            prev = v;
        }
        assert!(
            (adjust::modern_brightness(1.0, b) - 1.0).abs() < 1e-5 && adjust::modern_brightness(0.0, b).abs() < 1e-5,
            "brightness {b} pins black and white"
        );
        for k in 0..=50 {
            let x = k as f32 / 50.0;
            let back = adjust::modern_brightness(adjust::modern_brightness(x, b), -b);
            assert!((back - x).abs() < 2e-3, "brightness {b}: {x} -> {back}");
        }
    }
    for v in [f32::NAN, f32::INFINITY, -3.0, 7.0] {
        let _ = adjust::modern_brightness(v, 40.0);
        let _ = adjust::modern_brightness(0.5, v);
    }
    assert_eq!(adjust::modern_lut(0.0, 0.0).len(), 4096);
    // The table agrees with the curves it samples, lowered (inverted in one pass) or raised.
    for (b, c) in [(-50.0, 30.0), (-150.0, -50.0), (40.0, 0.0), (0.0, 100.0)] {
        let t = adjust::modern_lut(b, c);
        for k in (0..4096).step_by(37) {
            let x = k as f32 / 4095.0;
            let want = adjust::modern_contrast(adjust::modern_brightness(x, b), c);
            assert!((t[k] - want).abs() < 1e-3, "({b}, {c}) at {k}: {} vs {want}", t[k]);
        }
    }
}
