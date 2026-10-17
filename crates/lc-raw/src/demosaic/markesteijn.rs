//! Frank Markesteijn's three-pass X-Trans demosaic, ported from darktable
//! xtrans.c at 733bd69f32cac7ff5e41025115942772add1f088 (GPL-3.0-or-later).
//! Safe, disjoint output bands replace OpenMP/pointer writes. See docs/PORTS.md.
use super::Mosaic;
use crate::RgbFrame;
use rayon::prelude::*;
const PAD: usize = 17;
const N: usize = 122;
const TILE: usize = N - 2 * PAD;
const P: usize = N * N;

fn phase(m: &Mosaic, y: isize, x: isize) -> usize {
    m.cfa.color_at(x.rem_euclid(6) as usize, y.rem_euclid(6) as usize) as usize
}

/// Accept translations of the standard X-Trans layout.
pub(crate) fn supported(m: &Mosaic) -> bool {
    if m.cfa.width != 6 || m.cfa.height != 6 {
        return false;
    }
    let base = crate::Cfa::xtrans();
    (0..6).any(|dy| (0..6).any(|dx| (0..6).all(|y| (0..6).all(|x| m.cfa.color_at(x, y) == base.color_at(x + dx, y + dy)))))
}

/// darktable's three-colour VNG linear interpolation (X-Trans dual low branch).
pub(crate) fn linear(m: &Mosaic) -> RgbFrame {
    let mut out = RgbFrame::new(m.w, m.h);
    li_raster::par_rows(&mut out.data, m.w, |y, row| {
        for (x, p) in row.iter_mut().enumerate() {
            let own = m.cfa.color_at(x, y) as usize;
            let mut sum = [0.0; 3];
            let mut count = [0.0; 3];
            for yy in y.saturating_sub(1)..(y + 2).min(m.h) {
                for xx in x.saturating_sub(1)..(x + 2).min(m.w) {
                    let c = m.cfa.color_at(xx, yy) as usize;
                    let wt =
                        if x > 0 && y > 0 && x + 1 < m.w && y + 1 < m.h { (1 << (usize::from(xx == x) + usize::from(yy == y))) as f32 } else { 1.0 };
                    sum[c] += wt * m.data[yy * m.w + xx].max(0.0);
                    count[c] += wt;
                }
            }
            // A truncated 3x3 corner can omit red or blue at some CFA offsets.
            for c in 0..3 {
                if count[c] == 0.0 {
                    for yy in y.saturating_sub(2)..(y + 3).min(m.h) {
                        for xx in x.saturating_sub(2)..(x + 3).min(m.w) {
                            if m.cfa.color_at(xx, yy) as usize == c {
                                sum[c] += m.data[yy * m.w + xx].max(0.0);
                                count[c] += 1.0;
                            }
                        }
                    }
                }
            }
            for c in 0..3 {
                p[c] = if c == own || count[c] == 0.0 { m.data[y * m.w + x].max(0.0) } else { sum[c] / count[c] };
            }
        }
    });
    out
}

fn hexes(m: &Mosaic) -> ([[[isize; 8]; 3]; 3], isize, isize) {
    let orth = [1, 0, 0, 1, -1, 0, 0, -1, 1, 0, 0, 1];
    let patt = [[0, 1, 0, -1, 2, 0, -1, 0, 1, 1, 1, -1, 0, 0, 0, 0], [0, 1, 0, -2, 1, 0, -2, 0, 1, 1, -2, -2, 1, -1, -1, 1]];
    let mut hex = [[[0; 8]; 3]; 3];
    let (mut sy, mut sx) = (0, 0);
    for y in 0..3 {
        for x in 0..3 {
            let g = usize::from(phase(m, y as isize, x as isize) == 1);
            let mut ng = 0;
            for d in (0..10).step_by(2) {
                if phase(m, y as isize + orth[d], x as isize + orth[d + 2]) == 1 {
                    ng = 0;
                } else {
                    ng += 1;
                }
                if ng == 4 {
                    sy = y as isize;
                    sx = x as isize;
                }
                if ng == g + 1 {
                    for c in 0..8 {
                        let v = orth[d] * patt[g][2 * c] + orth[d + 1] * patt[g][2 * c + 1];
                        let h = orth[d + 2] * patt[g][2 * c] + orth[d + 3] * patt[g][2 * c + 1];
                        hex[y][x][c ^ ((g * 2) & d)] = h + v * N as isize;
                    }
                }
            }
        }
    }
    (hex, sy, sx)
}

fn tile(m: &Mosaic, x0: usize, y0: usize, hex: &[[[isize; 8]; 3]; 3], sy: isize, sx: isize) -> Vec<[f32; 3]> {
    let (top, left) = (y0 as isize - PAD as isize, x0 as isize - PAD as isize);
    let (hh, ww) = ((m.h - y0).min(TILE) + 2 * PAD, (m.w - x0).min(TILE) + 2 * PAD);
    let mut rgb = vec![[0.0; 3]; 8 * P];
    let mut bounds = vec![[0.0; 2]; P];
    let hx = |y: usize, x: usize| hex[(top + y as isize).rem_euclid(3) as usize][(left + x as isize).rem_euclid(3) as usize];
    let fc = |y: usize, x: usize| phase(m, top + y as isize, left + x as isize);
    for y in 0..hh {
        for x in 0..ww {
            let (gy, gx) = (top + y as isize, left + x as isize);
            let f = fc(y, x);
            let (ry, rx) = (super::reflect(gy, m.h), super::reflect(gx, m.w));
            let mut v = m.data[ry * m.w + rx].max(0.0);
            if m.cfa.color_at(rx, ry) as usize != f {
                let (mut sum, mut cnt) = (0.0, 0);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (yy, xx) = (super::reflect(gy + dy, m.h), super::reflect(gx + dx, m.w));
                        if m.cfa.color_at(xx, yy) as usize == f {
                            sum += m.data[yy * m.w + xx].max(0.0);
                            cnt += 1;
                        }
                    }
                }
                // Tiny inputs use the generic fallback before reaching this kernel.
                if cnt > 0 {
                    v = sum / cnt as f32;
                }
            }
            for d in 0..4 {
                rgb[d * P + y * N + x][f] = v;
            }
        }
    }
    // Every red/blue pair shares the same surrounding green hexagon.
    for y in 3..hh - 3 {
        for x in 3..ww - 3 {
            if fc(y, x) != 1 {
                let i = y * N + x;
                let h = hx(y, x);
                let mut lo = f32::MAX;
                let mut hi = 0.0f32;
                for off in &h[..6] {
                    let v = rgb[(i as isize + off) as usize][1];
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                bounds[i] = [lo, hi];
                let get = |o: isize, c: usize| rgb[(i as isize + o) as usize][c];
                let f = fc(y, x);
                let mut v = [0.0; 4];
                v[0] = 0.6796875 * (get(h[1], 1) + get(h[0], 1)) - 0.1796875 * (get(2 * h[1], 1) + get(2 * h[0], 1));
                v[1] = 0.87109375 * get(h[3], 1) + 0.13 * get(h[2], 1) + 0.359375 * (get(0, f) - get(-h[2], f));
                for c in 0..2 {
                    let off = h[4 + c];
                    v[2 + c] =
                        0.640625 * get(off, 1) + 0.359375 * get(-2 * off, 1) + 0.12890625 * (2.0 * get(0, f) - get(3 * off, f) - get(-3 * off, f));
                }
                let xor = usize::from((top + y as isize - sy) % 3 == 0);
                for c in 0..4 {
                    rgb[(c ^ xor) * P + i][1] = v[c].clamp(lo, hi);
                }
            }
        }
    }
    for pass in 0..3 {
        let base = if pass == 0 { 0 } else { 4 };
        if pass == 1 {
            rgb.copy_within(0..4 * P, 4 * P);
        }
        if pass > 0 {
            for y in 6..hh - 6 {
                for x in 6..ww - 6 {
                    let f = fc(y, x);
                    if f == 1 {
                        continue;
                    }
                    let i = y * N + x;
                    let h = hx(y, x);
                    let xor = usize::from((top + y as isize - sy) % 3 == 0);
                    for d in 3..6 {
                        let k = (base + ((d - 2) ^ xor)) * P + i;
                        let get = |o: isize, c: usize| rgb[(k as isize + o) as usize][c];
                        let val = get(-2 * h[d], 1) + 2.0 * get(h[d], 1) - get(-2 * h[d], f) - 2.0 * get(h[d], f) + 3.0 * get(0, f);
                        rgb[k][1] = (val / 3.0).clamp(bounds[i][0], bounds[i][1]);
                    }
                }
            }
        }
        for y in 5..hh - 5 {
            if (top + y as isize - sy) % 3 != 0 {
                continue;
            }
            for x in 5..ww - 5 {
                if (left + x as isize - sx) % 3 != 0 {
                    continue;
                }
                let i = y * N + x;
                let mut ch = phase(m, top + y as isize, left + x as isize + 1);
                let mut diff = [0.0; 6];
                let mut val = [[0.0; 6]; 2];
                let mut step = 1isize;
                let mut dir = base;
                for d in 0..6 {
                    let k = dir * P + i;
                    let get = |o: isize, c: usize| rgb[(k as isize + o) as usize][c];
                    for c in 0..2 {
                        let off = step << c;
                        let g = 2.0 * get(0, 1) - get(off, 1) - get(-off, 1);
                        val[ch / 2][d] = g + get(off, ch) + get(-off, ch);
                        if d > 1 {
                            diff[d] += (get(off, 1) - get(-off, 1) - get(off, ch) + get(-off, ch)).powi(2) + g * g;
                        }
                        ch ^= 2;
                    }
                    if d < 2 || d & 1 != 0 {
                        let j = if d > 1 && diff[d - 1] < diff[d] { d - 1 } else { d };
                        rgb[k][0] = val[0][j] * 0.5;
                        rgb[k][2] = val[1][j] * 0.5;
                        dir += 1;
                    }
                    step ^= N as isize ^ 1;
                    ch ^= 2;
                }
            }
        }
        for y in 5..hh - 5 {
            for x in 5..ww - 5 {
                let f = 2 - fc(y, x);
                if f == 1 {
                    continue;
                }
                let i = y * N + x;
                let c = if (top + y as isize - sy) % 3 != 0 { N as isize } else { 1 };
                let h = 3 * (c ^ N as isize ^ 1);
                for d in 0..4 {
                    let k = (base + d) * P + i;
                    let get = |o: isize, c: usize| rgb[(k as isize + o) as usize][c];
                    let off = if d > 1
                        || ((d as isize ^ c) & 1) != 0
                        || (get(0, 1) - get(c, 1)).abs() + (get(0, 1) - get(-c, 1)).abs()
                            < 2.0 * ((get(0, 1) - get(h, 1)).abs() + (get(0, 1) - get(-h, 1)).abs())
                    {
                        c
                    } else {
                        h
                    };
                    rgb[k][f] = (get(off, f) + get(-off, f) + 2.0 * get(0, 1) - get(off, 1) - get(-off, 1)) * 0.5;
                }
            }
        }
        for y in 4..hh - 4 {
            if (top + y as isize - sy) % 3 == 0 {
                continue;
            }
            for x in 4..ww - 4 {
                if (left + x as isize - sx) % 3 == 0 {
                    continue;
                }
                let i = y * N + x;
                let h = hx(y, x);
                for d in 0..4 {
                    let k = (base + d) * P + i;
                    let a = h[2 * d];
                    let b = h[2 * d + 1];
                    let get = |o: isize, c: usize| rgb[(k as isize + o) as usize][c];
                    let v = if a + b != 0 {
                        let g = 3.0 * get(0, 1) - 2.0 * get(a, 1) - get(b, 1);
                        [(g + 2.0 * get(a, 0) + get(b, 0)) / 3.0, (g + 2.0 * get(a, 2) + get(b, 2)) / 3.0]
                    } else {
                        let g = 2.0 * get(0, 1) - get(a, 1) - get(b, 1);
                        [(g + get(a, 0) + get(b, 0)) * 0.5, (g + get(a, 2) + get(b, 2)) * 0.5]
                    };
                    rgb[k][0] = v[0];
                    rgb[k][2] = v[1];
                }
            }
        }
    }
    let mut drv = vec![0.0f32; 8 * P];
    let mut yuv = vec![[0.0f32; 3]; P];
    for d in 0..8 {
        for y in 13..hh - 13 {
            for x in 13..ww - 13 {
                let i = y * N + x;
                let r = rgb[d * P + i];
                let v = 0.2627 * r[0] + 0.6780 * r[1] + 0.0593 * r[2];
                yuv[i] = [v, (r[2] - v) * 0.56433, (r[0] - v) * 0.67815];
            }
        }
        let off = [1, N, N + 1, N - 1][d & 3];
        for y in 14..hh - 14 {
            for x in 14..ww - 14 {
                let i = y * N + x;
                drv[d * P + i] = (0..3).map(|c| (2.0 * yuv[i][c] - yuv[i + off][c] - yuv[i - off][c]).powi(2)).sum();
            }
        }
    }
    let mut homo = vec![0u8; 8 * P];
    for y in 15..hh - 15 {
        for x in 15..ww - 15 {
            let i = y * N + x;
            let tr = (0..8).map(|d| drv[d * P + i]).fold(f32::MAX, f32::min) * 8.0;
            for d in 0..8 {
                for dy in -1isize..=1 {
                    for dx in -1..=1 {
                        if drv[(d * P + i).wrapping_add_signed(dy * N as isize + dx)] <= tr {
                            homo[d * P + i] += 1;
                        }
                    }
                }
            }
        }
    }
    let mut out = vec![[0.0; 3]; (hh - 2 * PAD) * (ww - 2 * PAD)];
    for y in PAD..hh - PAD {
        for x in PAD..ww - PAD {
            let i = y * N + x;
            let mut hm = [0u16; 8];
            for d in 0..8 {
                for dy in -2isize..=2 {
                    for dx in -2..=2 {
                        hm[d] += homo[(d * P + i).wrapping_add_signed(dy * N as isize + dx)] as u16;
                    }
                }
            }
            let max = hm.iter().copied().max().unwrap_or(0);
            let threshold = max - (max >> 3);
            for d in 0..4 {
                if hm[d] < hm[d + 4] {
                    hm[d] = 0;
                } else if hm[d] > hm[d + 4] {
                    hm[d + 4] = 0;
                }
            }
            let mut sum = [0.0f32; 3];
            let mut cnt = 0;
            for d in 0..8 {
                if hm[d] >= threshold {
                    for c in 0..3 {
                        sum[c] += rgb[d * P + i][c];
                    }
                    cnt += 1;
                }
            }
            out[(y - PAD) * (ww - 2 * PAD) + x - PAD] = sum.map(|v| (v / cnt as f32).max(0.0));
        }
    }
    out
}

pub(crate) fn markesteijn(m: &Mosaic) -> RgbFrame {
    if m.w < 36 || m.h < 36 {
        return super::xtrans::directional(m);
    }
    let (hex, sy, sx) = hexes(m);
    let mut out = linear(m);
    out.data.par_chunks_mut(m.w * TILE).enumerate().for_each(|(band, rows)| {
        let y0 = band * TILE;
        for x0 in (0..m.w).step_by(TILE) {
            let data = tile(m, x0, y0, &hex, sy, sx);
            let ww = (m.w - x0).min(TILE);
            for (y, row) in data.chunks_exact(ww).enumerate() {
                for (x, p) in row.iter().enumerate() {
                    let (gx, gy) = (x0 + x, y0 + y);
                    if gx >= PAD && gy >= PAD && gx + PAD < m.w && gy + PAD < m.h {
                        rows[y * m.w + gx] = *p;
                    }
                }
            }
        }
    });
    out
}
