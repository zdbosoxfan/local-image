use super::*;

// Measured ramps from upstream 2c163bf7, Photoshop 27.11.
#[test]
fn legacy_brightness_contrast_matches_photoshop() {
    const X: [u8; 17] = [0, 16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 255];
    let measured: [(f32, f32, [u8; 17]); 7] = [
        (40.0, 0.0, [40, 56, 72, 88, 104, 120, 136, 152, 168, 184, 200, 216, 232, 248, 255, 255, 255]),
        (-40.0, 0.0, [0, 0, 0, 8, 24, 40, 56, 72, 88, 104, 120, 136, 152, 168, 184, 200, 215]),
        (0.0, 30.0, [0, 0, 0, 14, 37, 60, 82, 105, 128, 151, 174, 196, 219, 242, 255, 255, 255]),
        (0.0, -30.0, [38, 49, 61, 72, 83, 94, 106, 117, 128, 139, 150, 162, 173, 184, 195, 207, 217]),
        (40.0, 30.0, [3, 25, 48, 71, 94, 117, 139, 162, 185, 208, 231, 253, 255, 255, 255, 255, 255]),
        (100.0, 0.0, [100, 116, 132, 148, 164, 180, 196, 212, 228, 244, 255, 255, 255, 255, 255, 255, 255]),
        (0.0, 80.0, [0, 0, 0, 0, 0, 0, 0, 48, 128, 208, 255, 255, 255, 255, 255, 255, 255]),
    ];
    for (brightness, contrast, want) in measured {
        let rect = Rect::new(0, 0, 17, 1);
        let px = X.iter().map(|x| [f32::from(*x) / 255.0; 4]).map(|p| [p[0], p[1], p[2], 1.0]).collect();
        let mut b = Buffer { rect, px };
        adjust::apply(&Adjustment::BrightnessContrast { brightness, contrast, legacy: true }, &mut b);
        for (i, (p, w)) in b.px.iter().zip(want).enumerate() {
            let got = (p[0] * 255.0).round() as i32;
            assert!((got - i32::from(w)).abs() <= 1, "b {brightness} c {contrast} at {}: {got} vs Photoshop {w}", X[i]);
        }
    }
}
