//! L. Zhang / X. Wu LMMSE, ported from darktable lmmse.c at
//! 733bd69f32cac7ff5e41025115942772add1f088 (GPL-3.0-or-later).
//! RawTherapee adaptations by Jacques Desmis and Ingo Weyrich; darktable
//! tiling by Hanno Schwalm. Three median passes, without EECI refinement.
//! Mirror padding replaces PPG borders. A 16-pixel halo covers the complete
//! filter + colour + three-median stencil; upstream uses 12. The upstream
//! 136-pixel working tile is retained. Safe, disjoint output bands use Rayon.
use super::Mosaic;
use crate::RgbFrame;
use rayon::prelude::*;
use std::sync::OnceLock;
const PAD: usize = 16;
const N: usize = 136;
const TILE: usize = N - 2 * PAD;
const P: usize = N * N;
static GAMMA: OnceLock<[Vec<f32>; 2]> = OnceLock::new();
fn tables() -> &'static [Vec<f32>; 2] {
    GAMMA.get_or_init(|| {
        std::array::from_fn(|dir| {
            (0..65536)
                .map(|j| {
                    let x = j as f64 / 65535.0;
                    if dir == 0 {
                        if x <= 0.001867 { x * 17.0 } else { 1.044445 * (x.ln() / 2.4).exp() - 0.044445 }
                    } else if x <= 0.031746 {
                        x / 17.0
                    } else {
                        (((x + 0.044445) / 1.044445).ln() * 2.4).exp()
                    }
                })
                .map(|x| x as f32)
                .collect()
        })
    })
}
fn gamma(v: f32, t: &[f32]) -> f32 {
    let index = v * 65535.0;
    if index < 0.0 {
        return 0.0;
    }
    if index > 65534.99 {
        return 1.0;
    }
    let i = index as usize;
    t[i] + (t[i + 1] - t[i]) * (index - i as f32)
}
fn median3(a: f32, b: f32, c: f32) -> f32 {
    a.min(b).max(c.min(a.max(b)))
}
fn run(m: &Mosaic, x0: usize, y0: usize, scale: f32) -> Vec<[f32; 3]> {
    let g = tables();
    let mut q: [Vec<f32>; 6] = std::array::from_fn(|_| vec![0.0; P]);
    let (gx, gy) = (x0 as isize - PAD as isize, y0 as isize - PAD as isize);
    let fc = |y: usize, x: usize| m.color(gx + x as isize, gy + y as isize) as usize;
    for y in 0..N {
        for x in 0..N {
            q[5][y * N + x] = gamma(m.get(gx + x as isize, gy + y as isize) / scale, &g[0]);
        }
    }
    {
        let (diff, raw) = q.split_at_mut(5);
        let cfa = &raw[0];
        for y in 2..N - 2 {
            for x in 2..N - 2 {
                let i = y * N + x;
                let v0 = 0.0625 * (cfa[i - N - 1] + cfa[i - N + 1] + cfa[i + N - 1] + cfa[i + N + 1]) + 0.25 * cfa[i];
                for (d, off) in [1, N].into_iter().enumerate() {
                    let mut val = if fc(y, x) != 1 {
                        -0.25 * (cfa[i - 2 * off] + cfa[i + 2 * off]) + 0.5 * (cfa[i - off] + cfa[i] + cfa[i + off])
                    } else {
                        0.25 * (cfa[i - 2 * off] + cfa[i + 2 * off]) - 0.5 * (cfa[i - off] + cfa[i] + cfa[i + off])
                    };
                    if fc(y, x) != 1 {
                        val = if cfa[i] > 1.75 * (v0 + 0.5 * val) { median3(val, cfa[i - off], cfa[i + off]) } else { val.clamp(0.0, 1.0) };
                        val -= cfa[i];
                    } else {
                        val = val.clamp(-1.0, 0.0) + cfa[i];
                    }
                    diff[d][i] = val;
                }
            }
        }
    }
    let mut h: [f32; 5] = std::array::from_fn(|i| (-((i * i) as f32) / 8.0).exp());
    let hs = h[0] + 2.0 * (h[1] + h[2] + h[3] + h[4]);
    h.iter_mut().for_each(|v| *v /= hs);
    for y in 4..N - 4 {
        for x in 4..N - 4 {
            let i = y * N + x;
            for (d, off) in [1, N].into_iter().enumerate() {
                q[d + 2][i] = h[0] * q[d][i]
                    + h[1] * (q[d][i - off] + q[d][i + off])
                    + h[2] * (q[d][i - 2 * off] + q[d][i + 2 * off])
                    + h[3] * (q[d][i - 3 * off] + q[d][i + 3 * off])
                    + h[4] * (q[d][i - 4 * off] + q[d][i + 4 * off]);
            }
        }
    }
    for y in 4..N - 4 {
        for x in 4..N - 4 {
            if fc(y, x) == 1 {
                continue;
            }
            let i = y * N + x;
            let mut est = [0.0; 2];
            let mut variance = [0.0; 2];
            for (d, off) in [1, N].into_iter().enumerate() {
                let v: [f32; 9] = std::array::from_fn(|k| q[d + 2][i.wrapping_add_signed((k as isize - 4) * off as isize)]);
                let mu = v.iter().sum::<f32>() / 9.0;
                let vx = 1e-7 + v.iter().map(|v| (v - mu).powi(2)).sum::<f32>();
                let vn = 1e-7 + (0..9).map(|k| (v[k] - q[d][i.wrapping_add_signed((k as isize - 4) * off as isize)]).powi(2)).sum::<f32>();
                est[d] = (q[d][i] * vx + q[d + 2][i] * vn) / (vx + vn);
                variance[d] = vx * vn / (vx + vn);
            }
            q[4][i] = (est[0] * variance[1] + est[1] * variance[0]) / (variance[0] + variance[1]);
        }
    }
    for y in 0..N {
        for x in 0..N {
            let i = y * N + x;
            let c = fc(y, x);
            q[c][i] = q[5][i];
            if c != 1 {
                q[1][i] = q[c][i] + q[4][i];
            }
        }
    }
    for y in 1..N - 1 {
        for x in 1..N - 1 {
            if fc(y, x) != 1 {
                continue;
            }
            let i = y * N + x;
            let c = fc(y, x + 1);
            for (c, off) in [(c, 1), (2 - c, N)] {
                q[c][i] = q[1][i] + 0.5 * (q[c][i - off] - q[1][i - off] + q[c][i + off] - q[1][i + off]);
            }
        }
    }
    for y in 1..N - 1 {
        for x in 1..N - 1 {
            let c = 2 - fc(y, x);
            if c == 1 {
                continue;
            }
            let i = y * N + x;
            q[c][i] =
                q[1][i] + 0.25 * (q[c][i - N] - q[1][i - N] + q[c][i - 1] - q[1][i - 1] + q[c][i + 1] - q[1][i + 1] + q[c][i + N] - q[1][i + N]);
        }
    }
    for _ in 0..3 {
        for y in 1..N - 1 {
            for x in 1..N - 1 {
                let i = y * N + x;
                for c in [0, 2] {
                    let mut v = [0.0; 9];
                    let mut k = 0;
                    for dy in -1isize..=1 {
                        for dx in -1..=1 {
                            let j = i.wrapping_add_signed(dy * N as isize + dx);
                            v[k] = q[c][j] - q[1][j];
                            k += 1;
                        }
                    }
                    v.sort_unstable_by(f32::total_cmp);
                    q[3 + c / 2][i] = v[4];
                }
            }
        }
        for y in 2..N - 2 {
            for x in 2..N - 2 {
                let i = y * N + x;
                let c = fc(y, x);
                if c == 1 {
                    q[0][i] = q[1][i] + q[3][i];
                    q[2][i] = q[1][i] + q[4][i];
                } else {
                    let other = 2 - c;
                    q[other][i] = q[1][i] + q[3 + other / 2][i];
                    q[1][i] = 0.5 * (q[0][i] - q[3][i] + q[2][i] - q[4][i]);
                }
            }
        }
    }
    let (w, h) = ((m.w - x0).min(TILE), (m.h - y0).min(TILE));
    let mut out = vec![[0.0; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = (y + PAD) * N + x + PAD;
            let c = fc(y + PAD, x + PAD);
            out[y * w + x] = std::array::from_fn(|ch| if ch == c { m.data[(y + y0) * m.w + x + x0] } else { scale * gamma(q[ch][i], &g[1]) });
        }
    }
    out
}

pub(crate) fn lmmse(m: &Mosaic) -> RgbFrame {
    // Retain unclipped scene-linear highlights; gamma tables operate in [0,1].
    let scale = m.data.iter().copied().fold(1.0f32, f32::max);
    let mut out = RgbFrame::new(m.w, m.h);
    out.data.par_chunks_mut(m.w * TILE).enumerate().for_each(|(band, rows)| {
        for x0 in (0..m.w).step_by(TILE) {
            let w = (m.w - x0).min(TILE);
            let data = run(m, x0, band * TILE, scale);
            for (y, row) in data.chunks_exact(w).enumerate() {
                rows[y * m.w + x0..y * m.w + x0 + w].copy_from_slice(row);
            }
        }
    });
    out
}
