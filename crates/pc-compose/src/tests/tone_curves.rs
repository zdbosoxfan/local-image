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

// Every Levels step lands on a whole level in 8 bits, and the gamma's shadow toe starts at
// slope 2^gamma (`adjust::levels_gamma`). Level → Photoshop's level, exact, from:
// - psd-tools levels_rgb.psd, Levels 2 (each channel, then the master): master 0..213, gamma
//   1.22, output 48..255; R gamma 0.47 to 0..194; B 61..255, gamma 0.47, to 0..213. Narrow
//   output ranges merge neighbouring levels (R 43..48 → 58, G 8, 9 → 63, B 89, 90 → 54).
// - the photoshop corpus's rgb8 levels.psd: master 15..230, gamma 1.3, output 10..245.
// - psd-tools levels_grayscale.psd, Levels 3: 44..214, gamma 1.78.
#[test]
fn levels_steps_match_photoshop() {
    let ch = |ib: f32, iw: f32, gamma: f32, ob: f32, ow: f32| LevelsChannel {
        in_black: ib / 255.0,
        in_white: iw / 255.0,
        gamma,
        out_black: ob / 255.0,
        out_white: ow / 255.0,
    };
    let id = LevelsChannel::default();
    let levels = |master: LevelsChannel, per_channel: [LevelsChannel; 3]| Adjustment::Levels {
        master,
        per_channel,
        space: Default::default(),
        black: LevelsChannel::default(),
    };
    let run = |adj: &Adjustment, xs: &[u8]| {
        let px = xs.iter().map(|&x| f32::from(x) / 255.0).map(|v| [v, v, v, 1.0]).collect();
        let mut b = Buffer { rect: Rect::new(0, 0, xs.len() as i32, 1), px };
        adjust::apply_depth(adj, &mut b, adjust::Transfer::Srgb, Some(SampleType::U8));
        b.px
    };
    let rgb = levels(ch(0.0, 213.0, 1.22, 48.0, 255.0), [ch(0.0, 255.0, 0.47, 0.0, 194.0), id.clone(), ch(61.0, 255.0, 0.47, 0.0, 213.0)]);
    let xs = [2, 8, 9, 15, 16, 43, 45, 48, 55, 61, 72, 79, 84, 89, 90, 101, 106, 120, 150, 180, 200];
    let want: [[u8; 21]; 3] = [
        [48, 48, 48, 50, 50, 58, 58, 58, 63, 63, 69, 72, 76, 79, 79, 86, 89, 100, 123, 153, 174],
        [51, 63, 63, 72, 72, 103, 106, 109, 116, 122, 133, 141, 145, 149, 150, 160, 165, 178, 204, 228, 244],
        [48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 50, 51, 54, 54, 54, 63, 63, 76, 102, 136, 164],
    ];
    for (k, p) in run(&rgb, &xs).iter().enumerate() {
        for c in 0..3 {
            assert_eq!((p[c] * 255.0).round(), f32::from(want[c][k]), "levels_rgb channel {c} at {}", xs[k]);
        }
    }
    let single = |c: LevelsChannel| levels(c, [id.clone(), id.clone(), id.clone()]);
    let oracles: [(Adjustment, &[u8], &[u8]); 2] = [
        (
            single(ch(15.0, 230.0, 1.3, 10.0, 245.0)),
            &[0, 21, 22, 49, 82, 118, 154, 176, 195, 215, 235, 251],
            &[10, 25, 27, 66, 106, 144, 178, 198, 215, 232, 245, 245],
        ),
        (
            single(ch(44.0, 214.0, 1.78, 0.0, 255.0)),
            &[44, 45, 46, 47, 48, 49, 50, 51, 53, 55, 57, 60, 65, 70, 80, 100, 150, 200, 214],
            &[0, 7, 10, 17, 20, 26, 29, 35, 43, 51, 58, 66, 79, 89, 107, 137, 196, 243, 255],
        ),
    ];
    for (adj, xs, want) in &oracles {
        for ((p, x), w) in run(adj, xs).iter().zip(*xs).zip(*want) {
            assert_eq!((p[0] * 255.0).round(), f32::from(*w), "levels at {x}");
        }
    }
}

// The shadow toe stays a well-behaved curve for any gamma: from 0 to 1, rising, and without a
// jump where it meets the power curve.
#[test]
fn levels_gamma_toe_is_monotone_for_any_gamma() {
    for g in [1.0001, 1.01, 1.0105422, 1.02, 1.05, 1.5, 3.0, 9.99, 1e6, f32::INFINITY] {
        let mut last = 0.0f32;
        for k in 0..=4096 {
            let u = k as f32 / 4096.0;
            let t = adjust::levels_gamma(u, g);
            assert!(t.is_finite() && (0.0..=1.0 + 1e-6).contains(&t), "gamma {g} at {u}: {t}");
            assert!(t >= last - 1e-6, "gamma {g} falls at {u}: {last} -> {t}");
            // Never steeper than the toe's initial slope 2^g.
            assert!(k == 0 || t - last <= 2f32.powf(g) / 4096.0 + 1e-4, "gamma {g} jumps at {u}: {last} -> {t}");
            last = t;
        }
        assert!((adjust::levels_gamma(1.0, g) - 1.0).abs() < 1e-6, "gamma {g}: white");
    }
}
