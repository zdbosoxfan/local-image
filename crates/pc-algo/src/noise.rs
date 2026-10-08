//! Add Noise, Median, Dust & Scratches.

use photocraft_geom::Rect;

use crate::image::Image;
use crate::{Ctx, Distribution};

/// Deterministic hash → `[0, 1)` from document coordinates, so results do not
/// depend on tiling.
#[inline]
pub(crate) fn hash01(x: i32, y: i32, c: u32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ c.wrapping_mul(0xcb1a_b31f) ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Amount 100 % spans ±50 % of the range (uniform); Gaussian uses the same
/// amount as ~2σ.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add(src: &Image, out: Rect, ctx: &Ctx, amount: f32, dist: Distribution, mono: bool, seed: u32) -> Vec<f32> {
    let n = src.ch;
    let cc = if ctx.alpha { n - 1 } else { n };
    let a = amount.max(0.0) / 100.0 * 0.5;
    let mut res = src.crop(out);
    let w = out.width() as usize;
    for (i, px) in res.chunks_exact_mut(n).enumerate() {
        if ctx.alpha && px[n - 1] <= 0.0 {
            continue;
        }
        let (x, y) = (out.x0 + (i % w) as i32, out.y0 + (i / w) as i32);
        for (c, pv) in px.iter_mut().enumerate().take(cc) {
            let ch = if mono { 0 } else { c as u32 };
            let v = match dist {
                Distribution::Uniform => (hash01(x, y, ch, seed) - 0.5) * 2.0 * a,
                Distribution::Gaussian => {
                    let u1 = hash01(x, y, ch * 2 + 101, seed).max(1e-7);
                    let u2 = hash01(x, y, ch * 2 + 102, seed);
                    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos() * a * 0.5
                }
            };
            // Integer storage clamps on write; float surfaces keep the value.
            *pv += v;
        }
    }
    res
}

/// Median over a disc of `radius`; with `threshold`, a pixel is replaced only
/// when it differs from the median by more than `threshold` levels.
/// Median (and Dust & Scratches with a `threshold`) over a circular window of `radius`.
///
/// Radii above 2 use a sliding histogram per row (Huang's algorithm, as Pinta does): moving one
/// pixel right removes the window's left edge and adds its right edge, one sample per window row,
/// and a running pointer tracks the median bin, so the cost is O(r) per pixel instead of the
/// O(r²) gather and selection (a 100 px Median on 24 MP went from minutes to seconds). Values
/// are binned at 1/4095, finer than an 8-bit step and invisible at 16 bits; images with values
/// outside 0–1 (32-bit HDR) keep the exact path.
pub(crate) fn median(src: &Image, out: Rect, radius: f32, threshold: Option<f32>) -> Vec<f32> {
    let r = radius.max(0.0).round() as i32;
    if r == 0 {
        return src.crop(out);
    }
    let in_range = src.data.iter().all(|v| (0.0..=1.0).contains(v));
    if r <= 2 || !in_range { median_exact(src, out, r, threshold) } else { median_histogram(src, out, r, threshold) }
}

/// The window's half-width on each row `dy` in `-r..=r` (the same disc as the exact path).
fn half_widths(r: i32) -> Vec<i32> {
    (-r..=r).map(|dy| (0..=r).rev().find(|dx| dx * dx + dy * dy <= r * r + r).unwrap_or(0)).collect()
}

fn median_exact(src: &Image, out: Rect, r: i32, threshold: Option<f32>) -> Vec<f32> {
    let n = src.ch;
    let offs: Vec<(i32, i32)> = (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).filter(|(dx, dy)| dx * dx + dy * dy <= r * r + r).collect();
    let t = threshold.map(|t| t / 255.0);
    let mut vals = Vec::with_capacity(offs.len());
    let mut res = Vec::with_capacity(out.width() as usize * out.height() as usize * n);
    for y in out.y0..out.y1 {
        for x in out.x0..out.x1 {
            for c in 0..n {
                vals.clear();
                vals.extend(offs.iter().map(|(dx, dy)| src.get(x + dx, y + dy, c)));
                let mid = vals.len() / 2;
                let (_, m, _) = vals.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
                let m = *m;
                let o = src.get(x, y, c);
                res.push(match t {
                    Some(t) if (o - m).abs() <= t => o,
                    _ => m,
                });
            }
        }
    }
    res
}

const BINS: usize = 4096;

#[inline]
fn bin(v: f32) -> usize {
    ((v.clamp(0.0, 1.0) * (BINS - 1) as f32).round() as usize).min(BINS - 1)
}

fn median_histogram(src: &Image, out: Rect, r: i32, threshold: Option<f32>) -> Vec<f32> {
    use rayon::prelude::*;
    let n = src.ch;
    let hw = half_widths(r);
    let total: usize = hw.iter().map(|w| (2 * w + 1) as usize).sum();
    // The median is the value with `total / 2` values below it (the exact path's middle index).
    let half = total / 2;
    let t = threshold.map(|t| t / 255.0);
    let w = out.width() as usize;
    let rows: Vec<Vec<f32>> = (out.y0..out.y1)
        .into_par_iter()
        .map(|y| {
            let mut row = vec![0f32; w * n];
            let mut hist = vec![0u32; BINS];
            for c in 0..n {
                hist.iter_mut().for_each(|h| *h = 0);
                // The first window of the row.
                for (i, dy) in (-r..=r).enumerate() {
                    for dx in -hw[i]..=hw[i] {
                        hist[bin(src.get(out.x0 + dx, y + dy, c))] += 1;
                    }
                }
                // `m`: the median bin; `below`: how many values sit in lower bins.
                let (mut m, mut below) = (0usize, 0usize);
                while below + hist[m] as usize <= half {
                    below += hist[m] as usize;
                    m += 1;
                }
                for (k, x) in (out.x0..out.x1).enumerate() {
                    if k > 0 {
                        for (i, dy) in (-r..=r).enumerate() {
                            let old = bin(src.get(x - 1 - hw[i], y + dy, c));
                            let new = bin(src.get(x + hw[i], y + dy, c));
                            hist[old] -= 1;
                            hist[new] += 1;
                            if old < m {
                                below -= 1;
                            }
                            if new < m {
                                below += 1;
                            }
                        }
                        while below > half {
                            m -= 1;
                            below -= hist[m] as usize;
                        }
                        while below + hist[m] as usize <= half {
                            below += hist[m] as usize;
                            m += 1;
                        }
                    }
                    let med = m as f32 / (BINS - 1) as f32;
                    let o = src.get(x, y, c);
                    row[k * n + c] = match t {
                        Some(t) if (o - med).abs() <= t => o,
                        _ => med,
                    };
                }
            }
            row
        })
        .collect();
    rows.concat()
}

#[cfg(test)]
mod median_tests {
    use super::*;

    fn noisy(w: i32, h: i32, ch: usize) -> Image {
        let rect = Rect::new(0, 0, w, h);
        let data = (0..(w * h) as usize * ch).map(|i| ((i as u32).wrapping_mul(2654435761) >> 20) as f32 / 4095.0).collect();
        Image { rect, ch, data }
    }

    /// The sliding histogram agrees with the exact median to within a bin.
    #[test]
    fn histogram_median_matches_the_exact_one() {
        let img = noisy(60, 40, 2);
        let out = Rect::new(5, 5, 55, 35);
        for r in [3, 6] {
            for th in [None, Some(20.0)] {
                let a = median_exact(&img, out, r, th);
                let b = median_histogram(&img, out, r, th);
                assert_eq!(a.len(), b.len());
                let worst = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0f32, f32::max);
                assert!(worst <= 1.0 / 4095.0 + 1e-6, "r {r} {th:?}: {worst}");
            }
        }
    }

    #[test]
    fn large_radii_are_fast() {
        let img = noisy(400, 300, 3);
        let t = std::time::Instant::now();
        let _ = median(&img, Rect::new(0, 0, 400, 300), 40.0, None);
        assert!(t.elapsed().as_secs_f32() < 10.0, "{:?}", t.elapsed());
    }
}
