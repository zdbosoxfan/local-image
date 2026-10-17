//! Shared colour maths keeps the bits of the caller formulas it replaced (fingerprinted when the
//! migration was verified against frozen copies of them) and measurably improves on the three it
//! corrected deliberately.
#![allow(clippy::excessive_precision, clippy::float_cmp)]
use li_color::{perceptual as p, srgb_perceptual as s, transfer as t};

fn random(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state >> 8) as f32 / 16_777_215.0
}
fn bits(a: [f32; 3], b: [f32; 3]) {
    assert_eq!(a.map(f32::to_bits), b.map(f32::to_bits));
}
fn fnv1a(hash: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *hash = (*hash ^ u64::from(*b)).wrapping_mul(0x100000001b3);
    }
}
fn fnv1a_f32(hash: &mut u64, values: &[f32]) {
    for v in values {
        fnv1a(hash, &v.to_bits().to_le_bytes());
    }
}
#[test]
fn caller_migrations_preserve_calibrated_bits() {
    // FNV-1a of the output bits over 10 000 seeded random colours. Captured while each function
    // still matched its frozen pre-migration caller formula bit for bit (brush/picker HSV, compose
    // HSL, the u8 and f32 Oklab conversions and the sRGB transfer).
    const NAMES: [&str; 9] = [
        "rgb_to_hsv_turns",
        "hsv_turns_to_rgb",
        "rgb_to_hsv",
        "hsv_to_rgb",
        "rgb_to_hsl_turns",
        "linear_srgb_to_oklab",
        "oklab_to_linear_srgb",
        "srgb_to_linear",
        "linear_to_srgb",
    ];
    const GOLDEN: [u64; 9] = [
        0xa356bd69b4b5f665,
        0xeaa3ff73ad1702c5,
        0x161e04319c571d94,
        0x769e9b00d03c02fb,
        0x02667770d53251b2,
        0x34644def6ef94d81,
        0xdeada16e120985a6,
        0x48de27a98b31141f,
        0x9d7a4a2b371cd16c,
    ];
    let mut state = 0x1234_5678;
    let mut h = [0xcbf29ce484222325_u64; 9];
    let mut oklab_u8 = 0xcbf29ce484222325_u64;
    for _ in 0..10_000 {
        let c: [f32; 3] = std::array::from_fn(|_| random(&mut state));
        fnv1a_f32(&mut h[0], &p::rgb_to_hsv_turns(c));
        fnv1a_f32(&mut h[1], &p::hsv_turns_to_rgb(c[0], c[1], c[2]));
        fnv1a_f32(&mut h[2], &p::rgb_to_hsv(c));
        fnv1a_f32(&mut h[3], &p::hsv_to_rgb(c[0] * 360.0, c[1], c[2]));
        fnv1a_f32(&mut h[4], &p::rgb_to_hsl_turns(c));
        let rgb8 = c.map(|v| (v * 255.0) as u8);
        let decoded = rgb8.map(|v| t::srgb_to_linear_f64(f64::from(v) / 255.0));
        for v in s::linear_srgb_to_oklab_f64(decoded) {
            fnv1a(&mut oklab_u8, &v.to_bits().to_le_bytes());
        }
        // The old encoded-sRGB Oklab path: decode, convert (and back, clamped and re-encoded).
        fnv1a_f32(&mut h[5], &s::linear_srgb_to_oklab(c.map(t::srgb_to_linear)));
        fnv1a_f32(&mut h[6], &s::oklab_to_linear_srgb(c));
        for v in c.into_iter().chain([-1.0, 0.0, 0.003_130_8, 0.04045, 1.0, 2.0]) {
            fnv1a_f32(&mut h[7], &[t::srgb_to_linear(v)]);
            fnv1a_f32(&mut h[8], &[t::linear_to_srgb(v)]);
        }
    }
    for i in 0..9 {
        assert_eq!(h[i], GOLDEN[i], "{} bits changed", NAMES[i]);
    }
    assert_eq!(oklab_u8, 0xbda8e160b75b016f, "linear_srgb_to_oklab_f64 of decoded u8 bits changed");
    // A remainder may round tiny negative hue to the endpoint. The shared product kernel
    // preserves camera-profile red (the old brush formula produced a spurious magenta seam).
    bits(p::hsv_to_rgb_profile(-0.000_001, 1.0, 1.0), [1.0, 0.0, 0.0]);
    bits(p::hsv_turns_to_rgb(-0.000_000_001, 1.0, 1.0), [1.0, 0.0, 0.0]);
    for (c, hsl) in [
        ([0.0; 3], [0.0; 3]),
        ([1.0; 3], [0.0, 0.0, 1.0]),
        ([0.5; 3], [0.0, 0.0, 0.5]),
        ([0.5, 0.5, 0.500_000_06], [0.0, 0.0, 0.5]),
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.5]),
    ] {
        bits(p::rgb_to_hsl_turns(c), hsl);
    }
}
fn hsl_reference([h, sat, l]: [f32; 3]) -> [f64; 3] {
    let (h, sat, l) = (f64::from(h), f64::from(sat), f64::from(l));
    let c = (1.0 - (2.0 * l - 1.0).abs()) * sat;
    let hp = h.rem_euclid(1.0) * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let channels = match hp as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    channels.map(|v| v + l - c / 2.0)
}
#[test]
fn deliberate_precision_corrections_are_measured() {
    // Fixed independent nominal-D50 fixtures from exact-rational matrices and
    // 80-digit decimal sRGB transfer evaluation, before f32 output rounding.
    for (lab, expected) in [
        ([0.0, 0.0, 0.0], [0.0; 3]),
        ([100.0, 0.0, 0.0], [1.0; 3]),
        ([50.0, 0.0, 0.0], [0.466_326_609_283_537_3; 3]),
        ([8.0, 0.0, 0.0], [0.092_212_275_323_640_78; 3]),
        ([60.0, 60.0, 30.0], [0.954_980_836_986_337_6, 0.353_967_212_993_205_3, 0.377_153_754_604_558_63]),
        ([50.0, -40.0, 35.0], [0.192_002_510_511_133, 0.531_422_896_324_369_6, 0.211_617_348_331_053_45]),
    ] {
        bits(li_color::convert::lab_to_srgb_with_white(lab, li_color::cms_math::D50), expected.map(|v| v as f32));
    }
    let mut state = 0x1234_5678;
    let mut errors = [0.0f64; 3];
    let mut max_errors = [0.0f64; 3];
    let mut count = 0;
    for _ in 0..100_000 {
        let c: [f32; 3] = std::array::from_fn(|_| random(&mut state));
        let [r, g, b] = c.map(|v| t::srgb_to_linear_f64(f64::from(v)));
        let xyz = [
            0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b,
            0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b,
            0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b,
        ];
        let cie = |x: f64| if x > (6.0f64 / 29.0).powi(3) { x.cbrt() } else { x / (3.0 * (6.0f64 / 29.0).powi(2)) + 4.0 / 29.0 };
        let [fx, fy, fz] = [cie(xyz[0] / 0.95047), cie(xyz[1]), cie(xyz[2] / 1.08883)];
        let lab_ref = [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)];
        let lab = [c[0] * 100.0, c[1] * 256.0 - 128.0, c[2] * 256.0 - 128.0];
        let [l, a, b] = lab.map(f64::from);
        let fy = (l + 16.0) / 116.0;
        let fx = fy + a / 500.0;
        let fz = fy - b / 200.0;
        let inv = |v: f64| if v > 6.0 / 29.0 { v * v * v } else { 3.0 * (6.0f64 / 29.0).powi(2) * (v - 4.0 / 29.0) };
        // Independent exact-rational derivation from ICC nominal D50, the published
        // sRGB primaries/D65 chromaticity, and Bradford (see source report).
        let xyz = [inv(fx) * 0.9642, inv(fy), inv(fz) * 0.8249];
        let matrix = [
            [3.134_186_364_236_819, -1.617_208_958_998_275, -0.490_694_064_006_383_9],
            [-0.978_748_504_190_694_4, 1.916_130_096_773_587_2, 0.033_433_399_159_995_54],
            [0.071_963_927_802_246_74, -0.228_993_873_453_203_28, 1.405_753_732_896_444_4],
        ];
        let encode = |v: f64| if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        let rgb_ref = matrix.map(|r| encode((r[0] * xyz[0] + r[1] * xyz[1] + r[2] * xyz[2]).clamp(0.0, 1.0)));
        let pairs = [
            (s::srgb_to_lab_d65(c), lab_ref),
            (li_color::convert::lab_to_srgb_with_white(lab, li_color::cms_math::D50), rgb_ref),
            (p::hsl_turns_to_rgb(c[0], c[1], c[2]), hsl_reference(c)),
        ];
        for (j, (shared, reference)) in pairs.into_iter().enumerate() {
            for i in 0..3 {
                let e = (f64::from(shared[i]) - reference[i]).abs();
                errors[j] += e * e;
                max_errors[j] = max_errors[j].max(e);
            }
        }
        count += 3;
    }
    // RMS error of the replaced caller formulas (SLIC Lab, PSD Lab import, compose HSL) against the
    // same references on these same samples, measured from frozen copies of them.
    const OLD_RMS: [f64; 3] = [0.000_010_367_389_585_016_788, 0.000_093_290_663_817_776_95, 0.000_000_039_020_661_234_630_426];
    for j in 0..3 {
        let rms = (errors[j] / f64::from(count)).sqrt();
        eprintln!("correction {j}: old rms {}; shared max {} rms {rms}", OLD_RMS[j], max_errors[j]);
        assert!(rms < OLD_RMS[j], "correction {j} must reduce measured RMS error");
    }
}
