//! Frozen old caller formulas are independent migration oracles, never production copies.
#![allow(clippy::excessive_precision, clippy::float_cmp)]
#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]

use li_color::{perceptual as p, srgb_perceptual as s, transfer as t};

fn slic_lab(c: [f32; 3]) -> [f32; 3] {
    let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
    let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / 0.950_47;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
    let z = (0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b) / 1.088_83;
    let f = |t: f32| if t > 0.008_856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn brush_hsv(c: [f32; 3]) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let d = mx - mn;
    let h = if d <= 0.0 {
        0.0
    } else if mx == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if mx == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    } / 6.0;
    [h, if mx > 0.0 { d / mx } else { 0.0 }, mx]
}

fn brush_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

fn picker_hsv(c: [f32; 3]) -> [f32; 3] {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let d = mx - mn;
    let h = if d <= 0.0 {
        0.0
    } else if mx == c[0] {
        60.0 * ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if mx == c[1] {
        60.0 * ((c[2] - c[0]) / d + 2.0)
    } else {
        60.0 * ((c[0] - c[1]) / d + 4.0)
    };
    [h, if mx <= 0.0 { 0.0 } else { d / mx }, mx]
}

fn picker_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = (h.rem_euclid(360.0)) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

fn compose_hsl(c: [f32; 3]) -> (f32, f32, f32) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-7 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn compose_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s <= 0.0 {
        return [l; 3];
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}

fn import_lab([l, a, b]: [f32; 3]) -> [f32; 3] {
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let inv = |t: f32| if t > 6.0 / 29.0 { t * t * t } else { 3.0 * (6.0f32 / 29.0).powi(2) * (t - 4.0 / 29.0) };
    let (x, y, z) = (0.9642 * inv(fx), inv(fy), 0.8249 * inv(fz));
    // XYZ (D50) → linear sRGB (D65), Bradford-adapted matrix.
    let r = 3.1339 * x - 1.6169 * y - 0.4906 * z;
    let g = -0.9788 * x + 1.9161 * y + 0.0335 * z;
    let bb = 0.0719 * x - 0.2290 * y + 1.4052 * z;
    let gamma = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    };
    [gamma(r), gamma(g), gamma(bb)]
}

fn old_lin(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
fn old_enc(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}
fn old_to_oklab(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(old_lin);
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}
fn old_from_oklab(p: [f32; 3]) -> [f32; 3] {
    let l = (p[0] + 0.396_337_78 * p[1] + 0.215_803_76 * p[2]).powi(3);
    let m = (p[0] - 0.105_561_346 * p[1] - 0.063_854_17 * p[2]).powi(3);
    let s = (p[0] - 0.089_484_18 * p[1] - 1.291_485_5 * p[2]).powi(3);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
    .map(old_enc)
}
fn old_srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}
fn old_linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}
fn old_oklab_u8(rgb: [u8; 3]) -> [f64; 3] {
    let lin = |x: u8| {
        let v = f64::from(x) / 255.;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let [r, g, b] = rgb.map(lin);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}
fn random(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state >> 8) as f32 / 16_777_215.0
}
fn bits(a: [f32; 3], b: [f32; 3]) {
    assert_eq!(a.map(f32::to_bits), b.map(f32::to_bits));
}
#[test]
fn caller_migrations_preserve_calibrated_bits() {
    let mut state = 0x1234_5678;
    for _ in 0..10_000 {
        let c = std::array::from_fn(|_| random(&mut state));
        bits(brush_hsv(c), p::rgb_to_hsv_turns(c));
        bits(brush_rgb(c[0], c[1], c[2]), p::hsv_turns_to_rgb(c[0], c[1], c[2]));
        bits(picker_hsv(c), p::rgb_to_hsv(c));
        bits(picker_rgb(c[0] * 360.0, c[1], c[2]), p::hsv_to_rgb(c[0] * 360.0, c[1], c[2]));
        let (h, sat, l) = compose_hsl(c);
        bits([h, sat, l], p::rgb_to_hsl_turns(c));
        let rgb8 = c.map(|v| (v * 255.0) as u8);
        let decoded = rgb8.map(|v| t::srgb_to_linear_f64(f64::from(v) / 255.0));
        assert_eq!(old_oklab_u8(rgb8).map(f64::to_bits), s::linear_srgb_to_oklab_f64(decoded).map(f64::to_bits));
        bits(old_to_oklab(c), s::linear_srgb_to_oklab(c.map(t::srgb_to_linear)));
        bits(old_from_oklab(c), s::oklab_to_linear_srgb(c).map(|v| t::linear_to_srgb(v.clamp(0.0, 1.0))));
        for v in c.into_iter().chain([-1.0, 0.0, 0.003_130_8, 0.04045, 1.0, 2.0]) {
            assert_eq!(old_srgb_to_linear(v).to_bits(), t::srgb_to_linear(v).to_bits());
            assert_eq!(old_linear_to_srgb(v).to_bits(), t::linear_to_srgb(v).to_bits());
        }
    }
    // A remainder may round tiny negative hue to the endpoint. The shared product kernel
    // preserves camera-profile red and fixes the old brush's spurious magenta seam.
    bits(p::hsv_to_rgb_profile(-0.000_001, 1.0, 1.0), [1.0, 0.0, 0.0]);
    bits(p::hsv_turns_to_rgb(-0.000_000_001, 1.0, 1.0), [1.0, 0.0, 0.0]);
    bits(brush_rgb(-0.000_000_001, 1.0, 1.0), [1.0, 0.0, 1.0]);
    for c in [[0.0; 3], [1.0; 3], [0.5; 3], [0.5, 0.5, 0.500_000_06], [1.0, 0.0, 0.0]] {
        let (h, sat, l) = compose_hsl(c);
        bits([h, sat, l], p::rgb_to_hsl_turns(c));
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
    let mut errors = [[0.0f64; 2]; 3];
    let mut max_errors = [[0.0f64; 2]; 3];
    let mut max_change = [0.0f64; 3];
    let mut sum_change = [0.0f64; 3];
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
            (slic_lab(c), s::srgb_to_lab_d65(c), lab_ref),
            (import_lab(lab), li_color::convert::lab_to_srgb_with_white(lab, li_color::cms_math::D50), rgb_ref),
            (compose_rgb(c[0], c[1], c[2]), p::hsl_turns_to_rgb(c[0], c[1], c[2]), hsl_reference(c)),
        ];
        for (j, (old, new, reference)) in pairs.into_iter().enumerate() {
            for i in 0..3 {
                for (k, value) in [old[i], new[i]].into_iter().enumerate() {
                    let e = (f64::from(value) - reference[i]).abs();
                    errors[j][k] += e * e;
                    max_errors[j][k] = max_errors[j][k].max(e);
                }
                let d = (f64::from(old[i]) - f64::from(new[i])).abs();
                max_change[j] = max_change[j].max(d);
                sum_change[j] += d * d;
            }
        }
        count += 3;
    }
    for j in 0..3 {
        eprintln!(
            "correction {j}: old max {} rms {}; shared max {} rms {}; old/new max {} rms {}",
            max_errors[j][0],
            (errors[j][0] / f64::from(count)).sqrt(),
            max_errors[j][1],
            (errors[j][1] / f64::from(count)).sqrt(),
            max_change[j],
            (sum_change[j] / f64::from(count)).sqrt()
        );
        assert!(errors[j][1] < errors[j][0], "correction {j} must reduce measured RMS error");
    }
}
