//! VNG4, including the four-colour linear pass and all 64 gradient terms.
//! Port of darktable `src/iop/demosaicing/vng.c` and `basics.c` at
//! 733bd69f32cac7ff5e41025115942772add1f088. Copyright (C) 2010-2026
//! darktable developers; dcraw VNG by Dave Coffin. GPL-3.0-or-later.
//! Bayer greens remain separate until the final averaging pass.
use super::Mosaic;
use crate::Rgb32f;
use lightcraft_raster::par_rows;

const TERMS: [i32; 384] = [
    -2, -2, 0, -1, 1, 0x01, -2, -2, 0, 0, 2, 0x01, -2, -1, -1, 0, 1, 0x01, -2, -1, 0, -1, 1, 0x02, -2, -1, 0, 0, 1, 0x03, -2, -1, 0, 1, 2, 0x01, -2,
    0, 0, -1, 1, 0x06, -2, 0, 0, 0, 2, 0x02, -2, 0, 0, 1, 1, 0x03, -2, 1, -1, 0, 1, 0x04, -2, 1, 0, -1, 2, 0x04, -2, 1, 0, 0, 1, 0x06, -2, 1, 0, 1,
    1, 0x02, -2, 2, 0, 0, 2, 0x04, -2, 2, 0, 1, 1, 0x04, -1, -2, -1, 0, 1, 0x80, -1, -2, 0, -1, 1, 0x01, -1, -2, 1, -1, 1, 0x01, -1, -2, 1, 0, 2,
    0x01, -1, -1, -1, 1, 1, 0x88, -1, -1, 1, -2, 1, 0x40, -1, -1, 1, -1, 1, 0x22, -1, -1, 1, 0, 1, 0x33, -1, -1, 1, 1, 2, 0x11, -1, 0, -1, 2, 1,
    0x08, -1, 0, 0, -1, 1, 0x44, -1, 0, 0, 1, 1, 0x11, -1, 0, 1, -2, 2, 0x40, -1, 0, 1, -1, 1, 0x66, -1, 0, 1, 0, 2, 0x22, -1, 0, 1, 1, 1, 0x33, -1,
    0, 1, 2, 2, 0x10, -1, 1, 1, -1, 2, 0x44, -1, 1, 1, 0, 1, 0x66, -1, 1, 1, 1, 1, 0x22, -1, 1, 1, 2, 1, 0x10, -1, 2, 0, 1, 1, 0x04, -1, 2, 1, 0, 2,
    0x04, -1, 2, 1, 1, 1, 0x04, 0, -2, 0, 0, 2, 0x80, 0, -1, 0, 1, 2, 0x88, 0, -1, 1, -2, 1, 0x40, 0, -1, 1, 0, 1, 0x11, 0, -1, 2, -2, 1, 0x40, 0,
    -1, 2, -1, 1, 0x20, 0, -1, 2, 0, 1, 0x30, 0, -1, 2, 1, 2, 0x10, 0, 0, 0, 2, 2, 0x08, 0, 0, 2, -2, 2, 0x40, 0, 0, 2, -1, 1, 0x60, 0, 0, 2, 0, 2,
    0x20, 0, 0, 2, 1, 1, 0x30, 0, 0, 2, 2, 2, 0x10, 0, 1, 1, 0, 1, 0x44, 0, 1, 1, 2, 1, 0x10, 0, 1, 2, -1, 2, 0x40, 0, 1, 2, 0, 1, 0x60, 0, 1, 2, 1,
    1, 0x20, 0, 1, 2, 2, 1, 0x10, 1, -2, 1, 0, 1, 0x80, 1, -1, 1, 1, 1, 0x88, 1, 0, 1, 2, 1, 0x08, 1, 0, 2, -1, 1, 0x40, 1, 0, 2, 1, 1, 0x10,
];
const CHOOD: [(isize, isize); 8] = [(-1, -1), (-1, 0), (-1, 1), (0, 1), (1, 1), (1, 0), (1, -1), (0, -1)];

fn color(m: &Mosaic, y: isize, x: isize) -> usize {
    let c = m.cfa.color_at(x.rem_euclid(2) as usize, y.rem_euclid(2) as usize) as usize;
    if c == 1 && y.rem_euclid(2) == 1 { 3 } else { c }
}

struct Term {
    a: isize,
    b: isize,
    c: usize,
    weight: f32,
    grads: i32,
}

/// `linear_only` is darktable's low-frequency dual demosaic, full VNG otherwise.
pub(crate) fn vng(m: &Mosaic, linear_only: bool) -> Rgb32f {
    let (w, h) = (m.w, m.h);
    let mut linear = vec![[0.0f32; 4]; w * h];
    par_rows(&mut linear, w, |y, row| {
        for (x, p) in row.iter_mut().enumerate() {
            let f = color(m, y as isize, x as isize);
            let border = x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            let mut sum = [0.0; 4];
            let mut count = [0.0; 4];
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    let (yy, xx) = (y as isize + dy, x as isize + dx);
                    if yy < 0 || xx < 0 || yy >= h as isize || xx >= w as isize {
                        continue;
                    }
                    let c = color(m, yy, xx);
                    if !border && c == f {
                        continue;
                    }
                    let weight = if border { 1.0 } else { (1 << (usize::from(dy == 0) + usize::from(dx == 0))) as f32 };
                    sum[c] += m.data[yy as usize * w + xx as usize].max(0.0) * weight;
                    count[c] += weight;
                }
            }
            for c in 0..4 {
                p[c] = if c != f && count[c] != 0.0 { sum[c] / count[c] } else { m.data[y * w + x].max(0.0) };
            }
        }
    });
    let mut result = linear.clone();
    if !linear_only && w >= 5 && h >= 5 {
        let codes: Vec<Vec<Term>> = (0..4)
            .map(|phase| {
                let (y, x) = ((phase / 2) as isize, (phase % 2) as isize);
                TERMS
                    .as_chunks::<6>()
                    .0
                    .iter()
                    .filter_map(|t| {
                        let (y1, x1, y2, x2) = (t[0] as isize, t[1] as isize, t[2] as isize, t[3] as isize);
                        let c = color(m, y + y1, x + x1);
                        if color(m, y + y2, x + x2) != c {
                            return None;
                        }
                        let diag = if color(m, y, x + 1) == c && color(m, y + 1, x) == c { 2 } else { 1 };
                        if (y1 - y2).abs() == diag && (x1 - x2).abs() == diag {
                            return None;
                        }
                        Some(Term { a: y1 * w as isize + x1, b: y2 * w as isize + x2, c, weight: t[4] as f32, grads: t[5] })
                    })
                    .collect()
            })
            .collect();
        par_rows(&mut result, w, |y, row| {
            if y < 2 || y + 2 >= h {
                return;
            }
            for x in 2..w - 2 {
                let i = y * w + x;
                let mut gval = [0.0f32; 8];
                for t in &codes[(y % 2) * 2 + x % 2] {
                    let diff = (linear[(i as isize + t.a) as usize][t.c] - linear[(i as isize + t.b) as usize][t.c]).abs() * t.weight;
                    for (g, v) in gval.iter_mut().enumerate() {
                        if t.grads & (1 << g) != 0 {
                            *v += diff;
                        }
                    }
                }
                let gmin = gval.iter().copied().fold(f32::INFINITY, f32::min);
                let gmax = gval.iter().copied().fold(0.0, f32::max);
                if gmax == 0.0 {
                    continue;
                }
                let threshold = gmin + 0.5 * gmax;
                let c0 = color(m, y as isize, x as isize);
                let mut sum = [0.0; 4];
                let mut num = 0.0;
                for (g, &(dy, dx)) in CHOOD.iter().enumerate() {
                    if gval[g] > threshold {
                        continue;
                    }
                    let offset = dy * w as isize + dx;
                    let special = color(m, y as isize + dy, x as isize + dx) != c0 && color(m, y as isize + 2 * dy, x as isize + 2 * dx) == c0;
                    for c in 0..4 {
                        sum[c] += if c == c0 && special {
                            (linear[i][c] + linear[(i as isize + 2 * offset) as usize][c]) * 0.5
                        } else {
                            linear[(i as isize + offset) as usize][c]
                        };
                    }
                    num += 1.0;
                }
                for c in 0..4 {
                    row[x][c] = linear[i][c0] + if c != c0 { (sum[c] - sum[c0]) / num } else { 0.0 };
                }
            }
        });
    }
    Rgb32f { width: w, height: h, data: result.into_iter().map(|p| [p[0].max(0.0), (0.5 * (p[1] + p[3])).max(0.0), p[2].max(0.0)]).collect() }
}

/// Two passes of darktable's 3×3 median of R−G and B−G, preserving green.
pub(crate) fn color_smoothing(img: &mut Rgb32f, passes: usize) {
    let (w, h) = (img.width, img.height);
    if w < 3 || h < 3 {
        return;
    }
    for _ in 0..passes {
        for c in [0, 2] {
            let diff: Vec<f32> = img.data.iter().map(|p| p[c] - p[1]).collect();
            par_rows(&mut img.data, w, |y, row| {
                if y == 0 || y + 1 == h {
                    return;
                }
                for x in 1..w - 1 {
                    let mut med = [0.0; 9];
                    for j in 0..3 {
                        for k in 0..3 {
                            med[j * 3 + k] = diff[(y + j - 1) * w + x + k - 1];
                        }
                    }
                    med.sort_unstable_by(f32::total_cmp);
                    row[x][c] = (med[4] + row[x][1]).max(0.0);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_vectors() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vng");
        for pat in ["RGGB", "BGGR", "GRBG", "GBRG"] {
            let cfa = crate::Cfa::bayer(pat).unwrap();
            let data: Vec<f32> = (0u32..4096).map(|i| ((i.wrapping_mul(1664525).wrapping_add(1013904223) >> 8) & 65535) as f32 / 65536.0).collect();
            let m = Mosaic { w: 64, h: 64, data: &data, cfa: &cfa };
            for mode in 0..3 {
                let mut out = vng(&m, mode != 0);
                if mode == 2 {
                    color_smoothing(&mut out, 2);
                }
                let bytes = std::fs::read(root.join(format!("{pat}-{mode}.f32"))).unwrap();
                assert_eq!(bytes.len(), 64 * 64 * 3 * 4);
                let mut max = 0.0f32;
                for (a, b) in out.data.iter().flatten().zip(bytes.as_chunks::<4>().0.iter()) {
                    max = max.max((a - f32::from_le_bytes(*b)).abs());
                }
                eprintln!("VNG {pat} mode {mode}: max abs {max:.9}");
                assert!(max <= 2e-7, "{pat} mode {mode}: {max}");
            }
        }
    }
}
