//! DCB by Jacek Gozdz (BSD-3-Clause), Rust port of RawTherapee
//! rtengine/demosaic_algos.cc at 5f486d3678b34c74ba0c63571c17babe20935019.
//! Three correction iterations; standard (unenhanced) final chroma interpolation.
//! Mirror padding and safe Rayon output bands replace border/OpenMP adapters.
use super::Mosaic;
use crate::Rgb32f;
use rayon::prelude::*;
const PAD: usize = 10;
const TILE: usize = 192;
const N: usize = TILE + 2 * PAD;
const P: usize = N * N;
fn color(rgb: &mut [[f32; 3]], fc: impl Fn(usize, usize) -> usize) {
    for y in 1..N - 1 {
        for x in 1..N - 1 {
            let c = 2 - fc(y, x);
            if c == 1 {
                continue;
            }
            let i = y * N + x;
            rgb[i][c] = rgb[i][1]
                + (rgb[i + N + 1][c] + rgb[i + N - 1][c] + rgb[i - N + 1][c] + rgb[i - N - 1][c]
                    - (rgb[i + N + 1][1] + rgb[i + N - 1][1] + rgb[i - N + 1][1] + rgb[i - N - 1][1]))
                    * 0.25;
        }
    }
    for y in 1..N - 1 {
        for x in 1..N - 1 {
            if fc(y, x) != 1 {
                continue;
            }
            let i = y * N + x;
            let c = fc(y, x + 1);
            let d = 2 - c;
            rgb[i][c] = rgb[i][1] + (rgb[i + 1][c] + rgb[i - 1][c] - (rgb[i + 1][1] + rgb[i - 1][1])) * 0.5;
            rgb[i][d] = rgb[i][1] + (rgb[i + N][d] + rgb[i - N][d] - (rgb[i + N][1] + rgb[i - N][1])) * 0.5;
        }
    }
}
fn map(rgb: &[[f32; 3]], out: &mut [u8]) {
    for y in 2..N - 2 {
        for x in 2..N - 2 {
            let i = y * N + x;
            let (l, r, u, d) = (rgb[i - 1][1], rgb[i + 1][1], rgb[i - N][1], rgb[i + N][1]);
            out[i] = u8::from(if 4.0 * rgb[i][1] > (l + r) + (u + d) {
                l.min(r) + (l + r) < u.min(d) + (u + d)
            } else {
                l.max(r) + (l + r) > u.max(d) + (u + d)
            });
        }
    }
}
fn weight(map: &[u8], i: usize) -> f32 {
    (4 * map[i] as u32
        + 2 * (map[i + N] as u32 + map[i - N] as u32 + map[i + 1] as u32 + map[i - 1] as u32)
        + map[i + 2 * N] as u32
        + map[i - 2 * N] as u32
        + map[i + 2] as u32
        + map[i - 2] as u32) as f32
}
fn correct(rgb: &mut [[f32; 3]], map: &[u8], fc: impl Fn(usize, usize) -> usize, second: bool) {
    let pad = if second { 4 } else { 2 };
    for y in pad..N - pad {
        for x in pad..N - pad {
            let c = fc(y, x);
            if c == 1 {
                continue;
            }
            let i = y * N + x;
            let k = weight(map, i);
            rgb[i][1] = if second {
                rgb[i][c]
                    + ((16.0 - k) * (rgb[i - 1][1] + rgb[i + 1][1] - (rgb[i + 2][c] + rgb[i - 2][c]))
                        + k * (rgb[i - N][1] + rgb[i + N][1] - (rgb[i + 2 * N][c] + rgb[i - 2 * N][c])))
                        * 0.03125
            } else {
                ((16.0 - k) * (rgb[i - 1][1] + rgb[i + 1][1]) + k * (rgb[i - N][1] + rgb[i + N][1])) * 0.03125
            };
        }
    }
}
fn run(m: &Mosaic, x0: usize, y0: usize) -> Vec<[f32; 3]> {
    let (gx, gy) = (x0 as isize - PAD as isize, y0 as isize - PAD as isize);
    let fc = |y: usize, x: usize| m.color(gx + x as isize, gy + y as isize) as usize;
    let mut rgb = vec![[0.0; 3]; P];
    let mut directions = vec![0u8; P];
    for y in 0..N {
        for x in 0..N {
            rgb[y * N + x][fc(y, x)] = m.get(gx + x as isize, gy + y as isize).max(0.0);
        }
    }
    let saved: Vec<[f32; 2]> = rgb.iter().map(|p| [p[0], p[2]]).collect();
    for y in 2..N - 2 {
        for x in 2..N - 2 {
            if fc(y, x) == 1 {
                continue;
            }
            let i = y * N + x;
            rgb[i][1] = 0.25 * (rgb[i - 1][1] + rgb[i + 1][1] + rgb[i - N][1] + rgb[i + N][1]);
        }
    }
    for _ in 0..3 {
        for _ in 0..3 {
            for y in 2..N - 2 {
                for x in 2..N - 2 {
                    let c = fc(y, x);
                    if c == 1 {
                        continue;
                    }
                    let i = y * N + x;
                    rgb[i][1] = rgb[i][c]
                        + (rgb[i + 2 * N][1] + rgb[i - 2 * N][1] + rgb[i - 2][1] + rgb[i + 2][1]
                            - (rgb[i + 2 * N][c] + rgb[i - 2 * N][c] + rgb[i - 2][c] + rgb[i + 2][c]))
                            * 0.25;
                }
            }
        }
        map(&rgb, &mut directions);
        correct(&mut rgb, &directions, fc, false);
    }
    color(&mut rgb, fc);
    for y in 2..N - 2 {
        for x in 2..N - 2 {
            let i = y * N + x;
            let mut sum = [0.0; 3];
            for dy in -1isize..=1 {
                for dx in -1..=1 {
                    if dy == 0 && dx == 0 {
                        continue;
                    }
                    let j = i.wrapping_add_signed(dy * N as isize + dx);
                    for c in 0..3 {
                        sum[c] += rgb[j][c];
                    }
                }
            }
            rgb[i][0] = sum[0] * 0.125 + (rgb[i][1] - sum[1] * 0.125);
            rgb[i][2] = sum[2] * 0.125 + (rgb[i][1] - sum[1] * 0.125);
        }
    }
    map(&rgb, &mut directions);
    correct(&mut rgb, &directions, fc, true);
    map(&rgb, &mut directions);
    correct(&mut rgb, &directions, fc, false);
    color(&mut rgb, fc);
    for _ in 0..2 {
        map(&rgb, &mut directions);
        correct(&mut rgb, &directions, fc, false);
    }
    for (p, s) in rgb.iter_mut().zip(saved) {
        p[0] = s[0];
        p[2] = s[1];
    }
    color(&mut rgb, fc);
    let (w, h) = ((m.w - x0).min(TILE), (m.h - y0).min(TILE));
    let mut out = vec![[0.0; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            out[y * w + x] = rgb[(y + PAD) * N + x + PAD].map(|v| v.max(0.0));
        }
    }
    out
}
pub(crate) fn dcb(m: &Mosaic) -> Rgb32f {
    let mut out = Rgb32f::new(m.w, m.h);
    out.data.par_chunks_mut(m.w * TILE).enumerate().for_each(|(band, rows)| {
        for x0 in (0..m.w).step_by(TILE) {
            let w = (m.w - x0).min(TILE);
            let data = run(m, x0, band * TILE);
            for (y, row) in data.chunks_exact(w).enumerate() {
                rows[y * m.w + x0..y * m.w + x0 + w].copy_from_slice(row);
            }
        }
    });
    out
}
