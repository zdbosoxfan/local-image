//! Frozen caller formulas verify adapter policies across platforms, without system-math snapshots.
use crate::{convert, perceptual};
fn affinity_hsl_before(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(1.0)) * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (r + m, g + m, b + m)
}

fn vectorscope_before([r, g, b]: [f32; 3]) -> (f32, f32) {
    let high = r.max(g).max(b);
    let low = r.min(g).min(b);
    let delta = high - low;
    if delta <= f32::EPSILON || high <= 0.0 {
        return (0.0, 0.0);
    }
    let h = if high == r {
        (g - b) / delta
    } else if high == g {
        2.0 + (b - r) / delta
    } else {
        4.0 + (r - g) / delta
    };
    ((h / 6.0).rem_euclid(1.0), delta / high)
}

fn codec_cmyk_before(c: f32, m: f32, y: f32, k: f32) -> [f32; 3] {
    [(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)]
}

fn codec_rgb_before(r: f32, g: f32, b: f32) -> [f32; 4] {
    let k = 1.0 - r.max(g).max(b);
    if k >= 1.0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let d = 1.0 - k;
    [(1.0 - r - k) / d, (1.0 - g - k) / d, (1.0 - b - k) / d, k]
}

#[test]
fn legacy_native_units_and_endpoint_policies_keep_bits() {
    for h in [-1.0, -0.25, -1e-12, -f64::MIN_POSITIVE, 0.0, 0.125, 0.5, 0.999_999_9, 1.0, 2.5] {
        for s in [0.0, 0.125, 0.5, 1.0, 1.5] {
            for l in [0.0, 0.1, 0.5, 0.9, 1.0, 1.2] {
                let (r, g, b) = affinity_hsl_before(h, s, l);
                assert_eq!(perceptual::hsl_turns_to_rgb_f64(h, s, l).map(f64::to_bits), [r, g, b].map(f64::to_bits));
            }
        }
    }
    for r in [-1.0, 0.0, 1e-8, 1e-7, 0.125, 0.5, 1.0, 2.0] {
        for g in [-1.0, 0.0, 1e-8, 1e-7, 0.125, 0.5, 1.0, 2.0] {
            for b in [-1.0, 0.0, 1e-8, 1e-7, 0.125, 0.5, 1.0, 2.0] {
                let (h, s) = vectorscope_before([r, g, b]);
                let [new_h, new_s, _] = perceptual::rgb_to_hsv_scope_turns([r, g, b]);
                assert_eq!([new_h, new_s].map(f32::to_bits), [h, s].map(f32::to_bits));
                assert_eq!(
                    convert::rgb_to_cmyk_naive_with_black_threshold([r, g, b], 1.0).map(f32::to_bits),
                    codec_rgb_before(r, g, b).map(f32::to_bits)
                );
                for k in [0.0, 0.5, 1.0, 1.5] {
                    assert_eq!(convert::cmyk_to_rgb_naive([r, g, b, k]).map(f32::to_bits), codec_cmyk_before(r, g, b, k).map(f32::to_bits));
                }
            }
        }
    }
}
