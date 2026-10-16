//! VNG4, including the four-colour linear pass and all 64 gradient terms.
//! Port of darktable `src/iop/demosaicing/vng.c` and `basics.c` at
//! 733bd69f32cac7ff5e41025115942772add1f088. Copyright (C) 2010-2026
//! darktable developers; dcraw VNG by Dave Coffin. GPL-3.0-or-later.
//! Bayer greens remain separate until the final averaging pass.
use super::Mosaic;
use crate::Rgb32f;
use lightcraft_raster::par_rows;
use rayon::prelude::*;

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
    // Precompute the interior stencil for each CFA phase. Keep the original
    // dy/dx accumulation order and weights, including separate Bayer greens.
    let stencils: [Vec<(isize, usize, f32)>; 4] = std::array::from_fn(|phase| {
        let (y, x) = ((phase / 2) as isize, (phase % 2) as isize);
        let f = color(m, y, x);
        (-1isize..=1)
            .flat_map(|dy| (-1isize..=1).map(move |dx| (dy, dx)))
            .filter_map(|(dy, dx)| {
                let c = color(m, y + dy, x + dx);
                (c != f).then_some((dy * w as isize + dx, c, (1 << (usize::from(dy == 0) + usize::from(dx == 0))) as f32))
            })
            .collect()
    });
    let colors: [usize; 4] = std::array::from_fn(|phase| color(m, (phase / 2) as isize, (phase % 2) as isize));
    let counts: [[f32; 4]; 4] = std::array::from_fn(|phase| {
        let mut count = [0.0; 4];
        for &(_, c, weight) in &stencils[phase] {
            count[c] += weight;
        }
        count
    });
    par_rows(&mut linear, w, |y, row| {
        for (x, p) in row.iter_mut().enumerate() {
            let phase = (y % 2) * 2 + x % 2;
            let f = colors[phase];
            let i = y * w + x;
            let mut sum = [0.0; 4];
            let mut count = counts[phase];
            if x > 0 && y > 0 && x + 1 < w && y + 1 < h {
                for &(offset, c, weight) in &stencils[phase] {
                    sum[c] += m.data[(i as isize + offset) as usize].max(0.0) * weight;
                }
            } else {
                count = [0.0; 4];
                for yy in y.saturating_sub(1)..(y + 2).min(h) {
                    for xx in x.saturating_sub(1)..(x + 2).min(w) {
                        let c = colors[(yy % 2) * 2 + xx % 2];
                        sum[c] += m.data[yy * w + xx].max(0.0);
                        count[c] += 1.0;
                    }
                }
            }
            for c in 0..4 {
                p[c] = if c != f && count[c] != 0.0 { sum[c] / count[c] } else { m.data[i].max(0.0) };
            }
        }
    });
    // The linear dual branch does not need a second four-channel image.
    let result = if linear_only || w < 5 || h < 5 {
        linear
    } else {
        let mut result = linear.clone();
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
        let neighbors: [[(isize, bool); 8]; 4] = std::array::from_fn(|phase| {
            let (y, x) = ((phase / 2) as isize, (phase % 2) as isize);
            std::array::from_fn(|g| {
                let (dy, dx) = CHOOD[g];
                (dy * w as isize + dx, color(m, y + dy, x + dx) != colors[phase] && color(m, y + 2 * dy, x + 2 * dx) == colors[phase])
            })
        });
        par_rows(&mut result, w, |y, row| {
            if y < 2 || y + 2 >= h {
                return;
            }
            for x in 2..w - 2 {
                let i = y * w + x;
                let mut gval = [0.0f32; 8];
                for t in &codes[(y % 2) * 2 + x % 2] {
                    let diff = (linear[(i as isize + t.a) as usize][t.c] - linear[(i as isize + t.b) as usize][t.c]).abs() * t.weight;
                    let mut grads = t.grads as u32;
                    while grads != 0 {
                        let g = grads.trailing_zeros() as usize;
                        gval[g] += diff;
                        grads &= grads - 1;
                    }
                }
                let gmin = gval.iter().copied().fold(f32::INFINITY, f32::min);
                let gmax = gval.iter().copied().fold(0.0, f32::max);
                if gmax == 0.0 {
                    continue;
                }
                let threshold = gmin + 0.5 * gmax;
                let phase = (y % 2) * 2 + x % 2;
                let c0 = colors[phase];
                let mut sum = [0.0; 4];
                let mut num = 0.0;
                for (g, &(offset, special)) in neighbors[phase].iter().enumerate() {
                    if gval[g] > threshold {
                        continue;
                    }
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
        result
    };
    Rgb32f { width: w, height: h, data: result.into_par_iter().map(|p| [p[0].max(0.0), (0.5 * (p[1] + p[3])).max(0.0), p[2].max(0.0)]).collect() }
}

/// Two passes of darktable's 3×3 median of R−G and B−G, preserving green.
pub(crate) fn color_smoothing(img: &mut Rgb32f, passes: usize) {
    let (w, h) = (img.width, img.height);
    if w < 3 || h < 3 {
        return;
    }
    // Both colour differences depend only on green, which is never changed.
    // Snapshot them together and smooth both channels in the same row pass.
    let mut diff = vec![[0.0; 2]; w * h];
    for _ in 0..passes {
        diff.par_iter_mut().zip(&img.data).for_each(|(d, p)| *d = [p[0] - p[1], p[2] - p[1]]);
        par_rows(&mut img.data, w, |y, row| {
            if y == 0 || y + 1 == h {
                return;
            }
            let rows = [&diff[(y - 1) * w..y * w], &diff[y * w..(y + 1) * w], &diff[(y + 1) * w..(y + 2) * w]];
            for x in 1..w - 1 {
                let mut med = [[0.0; 9]; 2];
                for j in 0..3 {
                    for (k, d) in rows[j][x - 1..x + 2].iter().enumerate() {
                        med[0][j * 3 + k] = d[0];
                        med[1][j * 3 + k] = d[1];
                    }
                }
                row[x][0] = (median9(med[0]) + row[x][1]).max(0.0);
                row[x][2] = (median9(med[1]) + row[x][1]).max(0.0);
            }
        });
    }
}

// Fixed median-of-nine selection network. total_cmp preserves the old sort's
// treatment of signed zero and NaNs as well as ordinary finite samples.
#[inline]
fn median9(p: [f32; 9]) -> f32 {
    // Convert to total_cmp's signed integer keys once, then use branch-free
    // integer min/max. The transform is its own inverse, including NaN payloads.
    let mut keys = p.map(|v| {
        let bits = v.to_bits() as i32;
        bits ^ (((bits >> 31) as u32) >> 1) as i32
    });
    macro_rules! sort_pair {
        ($a:literal, $b:literal) => {{
            let (a, b) = (keys[$a], keys[$b]);
            keys[$a] = a.min(b);
            keys[$b] = a.max(b);
        }};
    }
    sort_pair!(1, 2);
    sort_pair!(4, 5);
    sort_pair!(7, 8);
    sort_pair!(0, 1);
    sort_pair!(3, 4);
    sort_pair!(6, 7);
    sort_pair!(1, 2);
    sort_pair!(4, 5);
    sort_pair!(7, 8);
    sort_pair!(0, 3);
    sort_pair!(5, 8);
    sort_pair!(4, 7);
    sort_pair!(3, 6);
    sort_pair!(1, 4);
    sort_pair!(2, 5);
    sort_pair!(4, 7);
    sort_pair!(4, 2);
    sort_pair!(6, 4);
    sort_pair!(4, 2);
    let bits = keys[4];
    f32::from_bits((bits ^ (((bits >> 31) as u32) >> 1) as i32) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn median_network_matches_total_order_sort() {
        let mut values = [-0.0, 0.0, f32::NEG_INFINITY, f32::INFINITY, -0.25, 0.75, f32::from_bits(0x7fc00001), f32::from_bits(0xffc00001), 0.75];
        // All permutations also exercise duplicates, signed zero and NaN signs.
        fn check(p: &mut [f32; 9], n: usize) {
            if n == p.len() {
                let mut sorted = *p;
                sorted.sort_unstable_by(f32::total_cmp);
                assert_eq!(median9(*p).to_bits(), sorted[4].to_bits());
                return;
            }
            for i in n..p.len() {
                p.swap(n, i);
                check(p, n + 1);
                p.swap(n, i);
            }
        }
        check(&mut values, 0);
    }
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
