//! Numerical helpers from darktable `common/{math.h,gaussian.c}` at
//! 733bd69f32cac7ff5e41025115942772add1f088 (GPL-3.0-or-later).
use lightcraft_raster::par_rows;

/// darktable's bit approximation (including its positive-argument behavior).
pub(crate) fn fast_exp(x: f32) -> f32 {
    let bits = (0x3f800000u32 as f32 + x * (0x402df854u32 - 0x3f800000u32) as f32) as i32;
    f32::from_bits(bits.max(0) as u32)
}

/// The complete disc-truncated 9x9 Gaussian. Boundary samples outside the image are zero.
/// The summation follows upstream's symmetric groups, preserving its rounding behavior.
pub(crate) fn gaussian9(p: &[f32], w: usize, h: usize, sigma: f32, min: f32, max: f32) -> Vec<f32> {
    let mut kernel = [0.0f32; 81];
    let mut sum = 0.0;
    for y in -4i32..=4 {
        for x in -4i32..=4 {
            let r = (x * x + y * y) as f32;
            let v = if r <= 20.25 { (r / (-2.0 * sigma * sigma)).exp() } else { 0.0 };
            kernel[((y + 4) * 9 + x + 4) as usize] = v;
            sum += v;
        }
    }
    for v in &mut kernel {
        *v /= sum;
    }
    let groups: [&[(isize, isize)]; 13] = [
        &[(-2, -4), (2, -4), (-4, -2), (4, -2), (-4, 2), (4, 2), (-2, 4), (2, 4)],
        &[(-1, -4), (1, -4), (-4, -1), (4, -1), (-4, 1), (4, 1), (-1, 4), (1, 4)],
        &[(0, -4), (-4, 0), (4, 0), (0, 4)],
        &[(-3, -3), (3, -3), (-3, 3), (3, 3)],
        &[(-2, -3), (2, -3), (-3, -2), (3, -2), (-3, 2), (3, 2), (-2, 3), (2, 3)],
        &[(-1, -3), (1, -3), (-3, -1), (3, -1), (-3, 1), (3, 1), (-1, 3), (1, 3)],
        &[(0, -3), (-3, 0), (3, 0), (0, 3)],
        &[(-2, -2), (2, -2), (-2, 2), (2, 2)],
        &[(-1, -2), (1, -2), (-2, -1), (2, -1), (-2, 1), (2, 1), (-1, 2), (1, 2)],
        &[(0, -2), (-2, 0), (2, 0), (0, 2)],
        &[(-1, -1), (1, -1), (-1, 1), (1, 1)],
        &[(0, -1), (-1, 0), (1, 0), (0, 1)],
        &[(0, 0)],
    ];
    let mut out = vec![0.0; w * h];
    par_rows(&mut out, w, |y, row| {
        for (x, o) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let mut val = 0.0;
            if x >= 4 && y >= 4 && x + 4 < w && y + 4 < h {
                for g in groups {
                    let &(dx, dy) = &g[0];
                    let k = kernel[((dy + 4) * 9 + dx + 4) as usize];
                    let mut s = p[(i as isize + g[0].1 * w as isize + g[0].0) as usize];
                    for &(dx, dy) in &g[1..] {
                        s += p[(i as isize + dy * w as isize + dx) as usize];
                    }
                    val += k * s;
                }
            } else {
                for dy in -4isize..=4 {
                    for dx in -4isize..=4 {
                        let (xx, yy) = (x as isize + dx, y as isize + dy);
                        if xx >= 0 && yy >= 0 && xx < w as isize && yy < h as isize {
                            val += kernel[((dy + 4) * 9 + dx + 4) as usize] * p[yy as usize * w + xx as usize];
                        }
                    }
                }
            }
            *o = val.clamp(min, max);
        }
    });
    out
}
