//! Segmentation-based highlight reconstruction v2, ported from darktable
//! `src/iop/hlreconstruct/{segbased,segmentation,opposed}.c`, `common/distance_transform.c`,
//! `common/gaussian.c` and `develop/noise_generator.h` at
//! 733bd69f32cac7ff5e41025115942772add1f088. GPL-3.0-or-later.
//! Copyright (C) 2020-2026 darktable developers. Algorithm: Hanno Schwalm,
//! Iain and garagecoder (G'MIC); distance transform: Pedro Felzenszwalb (2006).
//!
//! The CFA is temporarily white balanced to retain upstream's clip and opponent math,
//! then converted back to camera samples. No late D65 correction is needed here.
//! GUI diagnostic masks and darktable's GUI chroma cache are host concerns and omitted.
use super::segmentation::{ID_MASK, Seg};
use crate::{Cfa, Normalized};
use rayon::prelude::*;

const BORDER: usize = 8;

/// All seven upstream rebuild modes (the segmentation candidate pass always runs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Recovery {
    #[default]
    Off,
    Small,
    Large,
    SmallFlat,
    LargeFlat,
    Adaptive,
    AdaptiveFlat,
}
impl Recovery {
    fn index(self) -> usize {
        self as usize
    }
    fn closing(self) -> usize {
        [0, 0, 0, 2, 2, 0, 2][self.index()]
    }
}

/// Upstream defaults, with optional recovery of areas clipped in all three channels.
#[derive(Clone, Copy, Debug)]
pub struct SegmentationOptions {
    /// Morphological combination radius, 0..8 plane pixels.
    pub combine: usize,
    /// Candidate tolerance, 0..1.
    pub candidating: f32,
    pub recovery: Recovery,
    pub strength: f32,
    pub noise: f32,
}
impl Default for SegmentationOptions {
    fn default() -> Self {
        Self { combine: 2, candidating: 0.4, recovery: Recovery::Off, strength: 0.0, noise: 0.0 }
    }
}
impl SegmentationOptions {
    /// Plane distances scale with a reduced CFA's sampling interval.
    pub fn at_scale(self, scale: usize) -> Self {
        Self { combine: ((self.combine as f32 / scale.max(1) as f32).round() as usize).min(8), ..self }
    }
}
#[inline]
fn cube(x: f32) -> f32 {
    x * x * x
}
#[inline]
fn raw_to_plane(w: usize, y: usize, x: usize) -> usize {
    (BORDER + y / 3) * w + BORDER + x / 3
}

// Retain upstream's truncated 3x3 window at the last row/column.
fn refavg(input: &[f32], w: usize, h: usize, cfa: &Cfa, y: usize, x: usize) -> f32 {
    let (mut sum, mut count) = ([0.0f32; 3], [0.0f32; 3]);
    for yy in y.saturating_sub(1)..(y + 2).min(h - 1) {
        let xmin = x.saturating_sub(1);
        for (dx, &v) in input[yy * w + xmin..yy * w + (x + 2).min(w - 1)].iter().enumerate() {
            let c = cfa.color_at(xmin + dx, yy) as usize;
            sum[c] += v.max(0.0);
            count[c] += 1.0;
        }
    }
    let mean: [f32; 3] = std::array::from_fn(|c| if count[c] > 0.0 { (sum[c] / count[c]).cbrt() } else { 0.0 });
    match cfa.color_at(x, y) {
        0 => 0.5 * (mean[1] + mean[2]),
        1 => 0.5 * (mean[0] + mean[2]),
        _ => 0.5 * (mean[0] + mean[1]),
    }
}

/// Upstream CFA opposed initialization, used as the fallback for segments without candidates.
fn opposed(input: &[f32], w: usize, h: usize, cfa: &Cfa, clips: [f32; 3]) -> Vec<f32> {
    let (mw, mh) = (w / 3, h / 3);
    let size = mw.max(1) * mh.max(1);
    let mut masks = vec![[false; 3]; size];
    masks.par_chunks_mut(mw).enumerate().for_each(|(my, row)| {
        for (mx, mask) in row.iter_mut().enumerate() {
            for y in 3 * my..3 * my + 3 {
                for x in 3 * mx..3 * mx + 3 {
                    let c = cfa.color_at(x, y) as usize;
                    mask[c] |= input[y * w + x] >= clips[c];
                }
            }
        }
    });
    let mut dilated = masks.clone();
    dilated.par_chunks_mut(mw).enumerate().for_each(|(y, row)| {
        if y < 3 || y + 3 >= mh {
            return;
        }
        for x in 3..mw.saturating_sub(3) {
            for dy in -3isize..=3 {
                for dx in -3isize..=3 {
                    if dy.abs() == 3 && dx.abs() > 2 {
                        continue;
                    }
                    let m = masks[(y as isize + dy) as usize * mw + (x as isize + dx) as usize];
                    for (o, v) in row[x].iter_mut().zip(m) {
                        *o |= v;
                    }
                }
            }
        }
    });
    // Evaluate expensive reference averages in parallel, then accumulate chroma
    // in the original pixel order. A floating-point parallel reduction would
    // change the chroma and every clipped pixel depending on it.
    let mut deltas = vec![0.0; w * h];
    deltas.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, delta) in row.iter_mut().enumerate() {
            let c = cfa.color_at(x, y) as usize;
            let i = (y / 3) * mw + x / 3;
            let v = input[y * w + x];
            if i < size && dilated[i][c] && v < clips[c] && v > 0.2 * clips[c] {
                *delta = v - cube(refavg(input, w, h, cfa, y, x));
            }
        }
    });
    let (mut sums, mut counts) = ([0.0f32; 3], [0usize; 3]);
    for y in 0..h {
        for x in 0..w {
            let c = cfa.color_at(x, y) as usize;
            let i = (y / 3) * mw + x / 3;
            let v = input[y * w + x];
            // Last partial blocks fall in upstream's zero-filled allocation padding.
            if i < size && dilated[i][c] && v < clips[c] && v > 0.2 * clips[c] {
                sums[c] += deltas[y * w + x];
                counts[c] += 1;
            }
        }
    }
    let chroma: [f32; 3] = std::array::from_fn(|c| if counts[c] > 100 { sums[c] / counts[c] as f32 } else { 0.0 });
    input
        .par_iter()
        .enumerate()
        .map(|(i, &v)| {
            let (x, y) = (i % w, i / w);
            let c = cfa.color_at(x, y) as usize;
            if v >= clips[c] { v.max(cube(refavg(input, w, h, cfa, y, x)) + chroma[c]) } else { v }
        })
        .collect()
}
fn extend(p: &mut [f32], w: usize, h: usize, b: usize) {
    for y in b..h - b {
        for x in 0..b {
            p[y * w + x] = p[y * w + b];
            p[y * w + w - x - 1] = p[y * w + w - b - 1];
        }
    }
    for x in 0..w {
        let xx = x.clamp(b, w - b - 1);
        let top = p[b * w + xx];
        let bot = p[(h - b - 1) * w + xx];
        for y in 0..b {
            p[y * w + x] = top;
            p[(h - y - 1) * w + x] = bot;
        }
    }
}
fn plane_rows<T: Send>(planes: &mut [Vec<T>; 3], w: usize) -> impl IndexedParallelIterator<Item = [&mut [T]; 3]> {
    let [a, b, c] = planes;
    a.par_chunks_mut(w).zip(b.par_chunks_mut(w)).zip(c.par_chunks_mut(w)).map(|((a, b), c)| [a, b, c])
}
fn weight(p: &[f32], i: usize, w: usize, clip: f32) -> f32 {
    let mut values = [0.0; 21];
    let mut k = 0;
    for y in -2isize..=2 {
        for x in -2isize..=2 {
            if y.abs() == 2 && x.abs() == 2 {
                continue;
            }
            values[k] = p[(i as isize + y * w as isize + x) as usize];
            k += 1;
        }
    }
    let mean = values.iter().sum::<f32>() / 21.0;
    let deviation = (values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / 21.0).sqrt();
    let smooth = (1.0 - 10.0 * deviation.sqrt()).max(0.0);
    let mut val = 0.0;
    for y in -1isize..=1 {
        for x in -1isize..=1 {
            val += p[(i as isize + y * w as isize + x) as usize] / 9.0;
        }
    }
    (val.min(clip) / clip).powf(2.0).max(1.0) * smooth
}
fn candidates(p: &[f32], reference: &[f32], s: &mut Seg, clip: f32, bad: f32) {
    let w = s.width as usize;
    let values: Vec<_> = (2..s.nr as usize)
        .into_par_iter()
        .map(|id| {
            if s.ymax[id] - s.ymin[id] <= 2 || s.xmax[id] - s.xmin[id] <= 2 {
                return (s.val1[id], s.val2[id]);
            }
            let ymin = (s.border + 2).max(s.ymin[id] - 2);
            let ymax = (s.height - s.border - 2).min(s.ymax[id] + 3);
            let rows: Vec<_> = (ymin..ymax)
                .into_par_iter()
                .map(|y| {
                    let (mut best, mut bestweight) = (0, 0.0);
                    for x in (s.border + 2).max(s.xmin[id] - 2)..(s.width - s.border - 2).min(s.xmax[id] + 3) {
                        let i = y as usize * w + x as usize;
                        if s.id(i) == id && p[i] < clip {
                            let k = weight(p, i, w, clip) * if s.data[i] & ID_MASK != 0 { 1.0 } else { 0.75 };
                            if k > bestweight {
                                bestweight = k;
                                best = i;
                            }
                        }
                    }
                    (best, bestweight)
                })
                .collect();
            let (best, bestweight) = rows.into_iter().fold((0, 0.0), |best, row| if row.1 > best.1 { row } else { best });
            if best == 0 || bestweight <= 1.0 - bad {
                return (s.val1[id], s.val2[id]);
            }
            let weights = [1.0, 4.0, 6.0, 4.0, 1.0];
            let (mut sum, mut cnt) = (0.0, 0.0);
            for y in -2isize..=2 {
                for x in -2isize..=2 {
                    let i = (best as isize + y * w as isize + x) as usize;
                    if p[i] < clip {
                        let k = weights[(y + 2) as usize] * weights[(x + 2) as usize];
                        sum += p[i] * k;
                        cnt += k;
                    }
                }
            }
            let av = sum / cnt.max(1.0);
            if av > 0.125 * clip {
                return (av.min(clip), reference[best]);
            }
            (s.val1[id], s.val2[id])
        })
        .collect();
    for (id, (v1, v2)) in (2..s.nr as usize).zip(values) {
        s.val1[id] = v1;
        s.val2[id] = v2;
    }
}

// Felzenszwalb/Huttenlocher lower envelope of parabolas, exactly the upstream two-pass EDT.
fn transform_1d(f: &[f32], d: &mut [f32], z: &mut [f32], v: &mut [usize]) {
    let n = f.len();
    let mut k = 0;
    z[0] = -1e20;
    z[1] = 1e20;
    for q in 1..n {
        let mut s = (f[q] + (q as f32).powi(2)) - (f[v[k]] + (v[k] as f32).powi(2));
        while s <= z[k] * (2 * q - 2 * v[k]) as f32 {
            k -= 1;
            s = (f[q] + (q as f32).powi(2)) - (f[v[k]] + (v[k] as f32).powi(2));
        }
        s /= (2 * q - 2 * v[k]) as f32;
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = 1e20;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < (q as f32) {
            k += 1;
        }
        *out = ((q as f32) - (v[k] as f32)).powi(2) + f[v[k]];
    }
}
fn distance_transform(p: &mut [f32], w: usize, h: usize) -> f32 {
    // Column-major staging makes each column independently writable in safe
    // Rust. Scratch is reused by Rayon jobs rather than allocated per line.
    let mut columns = vec![0.0; w * h];
    columns.par_chunks_mut(h).enumerate().for_each_init(
        || (vec![0.0; h], vec![0.0; h + 1], vec![0usize; h]),
        |(f, z, v), (x, column)| {
            for (y, value) in f.iter_mut().enumerate() {
                *value = p[y * w + x];
            }
            transform_1d(f, column, z, v);
        },
    );
    p.par_chunks_mut(w)
        .enumerate()
        .map_init(
            || (vec![0.0; w], vec![0.0; w + 1], vec![0usize; w]),
            |(f, z, v), (y, row)| {
                for (x, value) in f.iter_mut().enumerate() {
                    *value = columns[x * h + y];
                }
                transform_1d(f, row, z, v);
                let mut max = 0.0f32;
                for value in row {
                    *value = value.sqrt();
                    max = max.max(*value);
                }
                max
            },
        )
        .reduce(|| 0.0, f32::max)
}

fn scharr(p: &[f32], i: usize, w: usize) -> f32 {
    let at = |x: isize, y: isize| p[(i as isize + y * w as isize + x) as usize];
    let gx = 47.0 / 255.0 * (at(-1, -1) - at(1, -1) + at(-1, 1) - at(1, 1)) + 162.0 / 255.0 * (at(-1, 0) - at(1, 0));
    let gy = 47.0 / 255.0 * (at(-1, -1) - at(-1, 1) + at(1, -1) - at(1, 1)) + 162.0 / 255.0 * (at(0, -1) - at(0, 1));
    (gx * gx + gy * gy).sqrt()
}
fn blur(p: &[f32], w: usize, h: usize) -> Vec<f32> {
    crate::numerics::gaussian9(p, w, h, 1.2, 0.0, 20.0)
}
fn box_mean(p: &mut [f32], w: usize, h: usize, r: usize) {
    // Preserve each line's running-sum order and shrinking boundary window.
    // Column-major staging permits independent vertical sums without unsafe.
    let mut tmp = vec![0.0; w * h];
    let mut columns = vec![0.0; w * h];
    for _ in 0..2 {
        tmp.par_chunks_mut(w).zip(p.par_chunks(w)).for_each(|(out, row)| {
            let mut sum = 0.0;
            for &v in &row[..r.min(w)] {
                sum += v;
            }
            for (x, value) in out.iter_mut().enumerate() {
                if x > r {
                    sum -= row[x - r - 1];
                }
                if x + r < w {
                    sum += row[x + r];
                }
                *value = sum / ((x + r + 1).min(w) - x.saturating_sub(r)) as f32;
            }
        });
        columns.par_chunks_mut(h).enumerate().for_each(|(x, column)| {
            let mut sum = 0.0;
            for y in 0..r.min(h) {
                sum += tmp[y * w + x];
            }
            for (y, value) in column.iter_mut().enumerate() {
                if y > r {
                    sum -= tmp[(y - r - 1) * w + x];
                }
                if y + r < h {
                    sum += tmp[(y + r) * w + x];
                }
                *value = sum / ((y + r + 1).min(h) - y.saturating_sub(r)) as f32;
            }
        });
        p.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, value) in row.iter_mut().enumerate() {
                *value = columns[x * h + y];
            }
        });
    }
}

fn segment_gradients(distance: &[f32], gradient: &mut [f32], s: &Seg, id: usize, mode: Recovery) {
    let w = s.width as usize;
    let (xmin, xmax, ymin, ymax) = (
        (s.xmin[id] - 1).max(s.border) as usize,
        (s.xmax[id] + 2).min(s.width - s.border) as usize,
        (s.ymin[id] - 1).max(s.border) as usize,
        (s.ymax[id] + 2).min(s.height - s.border) as usize,
    );
    let attenuate = if mode.index() < 5 { [0.0, 1.7, 1.0, 1.7, 1.0][mode.index()] } else { (0.9 + 3.0 / s.val1[id].max(1.0)).min(1.7) };
    let strength = attenuate - 0.1 * mode.closing() as f32;
    let mut dist = 1.5;
    while dist < s.val1[id] {
        for y in ymin..ymax {
            for x in xmin..xmax {
                let i = y * w + x;
                if distance[i] >= dist && distance[i] < dist + 1.5 && s.data[i] == id as i32 {
                    let (mut grd, mut cnt) = (0.0, 0.0);
                    for dy in -2isize..=2 {
                        for dx in -2isize..=2 {
                            let j = (i as isize + dy * w as isize + dx) as usize;
                            if distance[j] >= dist - 1.5 && distance[j] < dist {
                                cnt += 1.0;
                                grd += gradient[j];
                            }
                        }
                    }
                    if cnt > 0.0 {
                        gradient[i] = ((grd / cnt) * (1.0 + 1.0 / distance[i].powf(attenuate))).min(1.5);
                    }
                }
            }
        }
        dist += 1.5;
    }
    if dist > 4.0 {
        let bw = xmax - xmin;
        let bh = ymax - ymin;
        let mut tmp: Vec<_> = (ymin..ymax).flat_map(|y| gradient[y * w + xmin..y * w + xmax].iter().copied()).collect();
        box_mean(&mut tmp, bw, bh, (dist as usize).min(15));
        for y in ymin..ymax {
            for x in xmin..xmax {
                let i = y * w + x;
                if s.data[i] == id as i32 {
                    gradient[i] = tmp[(y - ymin) * bw + x - xmin];
                }
            }
        }
    }
    for y in ymin..ymax {
        for x in xmin..xmax {
            let i = y * w + x;
            if s.data[i] == id as i32 {
                gradient[i] *= strength;
            }
        }
    }
}
fn splitmix(seed: u64) -> u32 {
    let mut r = (seed ^ (seed >> 33)).wrapping_mul(0x62a9d9ed799705f5);
    r = (r ^ (r >> 28)).wrapping_mul(0xcb24d0a5c88c35b3);
    (r >> 32) as u32
}
fn random(s: &mut [u32; 4]) -> f32 {
    let r = s[0].wrapping_add(s[3]);
    let t = s[1] << 9;
    s[2] ^= s[0];
    s[3] ^= s[1];
    s[1] ^= s[2];
    s[0] ^= s[3];
    s[2] ^= t;
    s[3] = s[3].rotate_left(11);
    (r >> 8) as f32 * (1.0 / 16777216.0)
}
fn add_noise(p: &mut [f32], s: &Seg, id: usize, noise: f32) {
    let (xmin, xmax, ymin, ymax) =
        (s.xmin[id].max(s.border), (s.xmax[id] + 1).min(s.width - s.border), s.ymin[id].max(s.border), (s.ymax[id] + 1).min(s.height - s.border));
    let mut state = [splitmix(ymin as u64), splitmix(xmin as u64), splitmix(1337), splitmix(666)];
    for _ in 0..4 {
        random(&mut state);
    }
    for y in ymin..ymax {
        for x in xmin..xmax {
            let i = (y * s.width + x) as usize;
            if s.data[i] == id as i32 {
                let u1 = random(&mut state).max(f32::MIN_POSITIVE);
                let u2 = random(&mut state);
                let angle = std::f32::consts::TAU * u2;
                let normal = (-2.0 * u1.ln()).sqrt() * if x & 1 != 0 { angle.cos() } else { angle.sin() };
                let r = normal * noise + 2.0 * (p[i] * noise + 3.0 / 8.0).max(0.0).sqrt();
                p[i] += (r * r - noise * noise) / 4.0 - 3.0 / 8.0;
            }
        }
    }
}

/// Reconstruct normalized CFA samples before demosaic. Returns the number of clipped samples.
/// Bayer and X-Trans are supported; linear RGB and monochrome inputs are left to the RGB stage.
pub fn segmentation(n: &mut Normalized, wb: [f32; 3], clip: f32, opts: &SegmentationOptions) -> usize {
    let Some(cfa) = &n.cfa else { return 0 };
    let (w, h) = (n.width, n.height);
    if n.cpp != 1 || w < 3 || h < 3 || n.data.len() != w * h {
        return 0;
    }
    let wb = wb.map(|v| if v.is_finite() && v > 0.0 { v } else { 1.0 });
    let clip = clip.max(0.1);
    let clips = wb.map(|v| clip * v);
    let cubes = clips.map(f32::cbrt);
    let mut input = vec![0.0; w * h];
    let count: usize = input
        .par_chunks_mut(w)
        .zip(n.data.par_chunks(w))
        .enumerate()
        .map(|(y, (out, row))| {
            let mut count = 0;
            for (x, (value, &v)) in out.iter_mut().zip(row).enumerate() {
                let c = cfa.color_at(x, y) as usize;
                *value = v * wb[c];
                count += usize::from(*value >= clips[c]);
            }
            count
        })
        .sum();
    if count == 0 {
        return 0;
    }
    let mut output = opposed(&input, w, h, cfa, clips);
    let (pw, ph) = ((w / 3).next_multiple_of(2) + 2 * BORDER, (h / 3).next_multiple_of(2) + 2 * BORDER);
    let size = pw * ph;
    let slots = w * h / 4000;
    let mut planes: [Vec<f32>; 3] = std::array::from_fn(|_| vec![0.0; size]);
    let mut refs: [Vec<f32>; 3] = std::array::from_fn(|_| vec![0.0; size]);
    let mut segments: [Seg; 4] = std::array::from_fn(|_| Seg::new(pw, ph, BORDER + 1, slots));
    let xshift = if cfa.is_bayer() && cfa.color_at(0, 0) == 1 { 1 } else { 2 };
    let [s0, s1, s2, s3] = &mut segments;
    let labels = s0
        .data
        .par_chunks_mut(pw)
        .zip(s1.data.par_chunks_mut(pw))
        .zip(s2.data.par_chunks_mut(pw))
        .zip(s3.data.par_chunks_mut(pw))
        .map(|(((a, b), c), d)| [a, b, c, d]);
    let (any, all) = plane_rows(&mut planes, pw)
        .zip(plane_rows(&mut refs, pw))
        .zip(labels)
        .enumerate()
        .map(|(py, ((planes, refs), labels))| {
            if py < BORDER {
                return (0, false);
            }
            let y = (py - BORDER) * 3 + 1;
            if y + 1 >= h {
                return (0, false);
            }
            let (mut any, mut all) = (0, false);
            for x in (xshift..w - 1).step_by(3) {
                let (mut mean, mut cnt) = ([0.0f32; 3], [0.0f32; 3]);
                for yy in y - 1..y + 2 {
                    for (dx, &v) in output[yy * w + x - 1..yy * w + x + 2].iter().enumerate() {
                        let c = cfa.color_at(x - 1 + dx, yy) as usize;
                        mean[c] += v;
                        cnt[c] += 1.0;
                    }
                }
                for c in 0..3 {
                    mean[c] = if cnt[c] > 0.0 { (mean[c] / cnt[c]).cbrt() } else { 0.0 };
                }
                let opp = [0.5 * (mean[1] + mean[2]), 0.5 * (mean[0] + mean[2]), 0.5 * (mean[0] + mean[1])];
                let i = BORDER + x / 3;
                let mut clipped = 0;
                for c in 0..3 {
                    planes[c][i] = mean[c];
                    refs[c][i] = opp[c];
                    if mean[c] > cubes[c] {
                        clipped += 1;
                        labels[c][i] = 1;
                    }
                }
                labels[3][i] = i32::from(clipped == 3);
                all |= clipped == 3;
                any += clipped;
            }
            (any, all)
        })
        .reduce(|| (0, false), |(ac, aa), (bc, ba)| (ac + bc, aa || ba));
    if any >= 20 {
        planes.par_iter_mut().zip(refs.par_iter()).zip(segments[..3].par_iter_mut()).enumerate().for_each(|(c, ((plane, reference), seg))| {
            extend(plane, pw, ph, BORDER);
            seg.combine(opts.combine.min(8));
            seg.segmentize();
            candidates(plane, reference, seg, cubes[c], opts.candidating.clamp(0.0, 1.0));
        });
        // A plane cell is written by multiple CFA pixels. Partition by three
        // source rows so the original last-write order stays local to one job.
        let [p0, p1, p2] = &mut planes;
        output
            .par_chunks_mut(3 * w)
            .zip(p0.par_chunks_mut(pw).skip(BORDER))
            .zip(p1.par_chunks_mut(pw).skip(BORDER))
            .zip(p2.par_chunks_mut(pw).skip(BORDER))
            .enumerate()
            .for_each(|(band, (((out, p0), p1), p2))| {
                let plane_rows = [p0, p1, p2];
                for (dy, row) in out.chunks_mut(w).enumerate() {
                    let y = band * 3 + dy;
                    if y == 0 || y + 1 >= h {
                        continue;
                    }
                    for x in 1..w - 1 {
                        let i = y * w + x;
                        let c = cfa.color_at(x, y) as usize;
                        let v = input[i].max(0.0);
                        if v > clips[c] {
                            let o = raw_to_plane(pw, y, x);
                            let id = segments[c].id(o);
                            if id > 1 && segments[c].val1[id] != 0.0 {
                                let oval = cube(refavg(&input, w, h, cfa, y, x) + segments[c].val1[id] - segments[c].val2[id]);
                                row[x] = v.max(oval);
                                plane_rows[c][BORDER + x / 3] = row[x];
                            }
                        }
                    }
                }
            });
        if opts.recovery != Recovery::Off && all && opts.strength > 0.0 {
            let seg = &mut segments[3];
            seg.combine(opts.recovery.closing());
            let b = seg.border as usize;
            let mut dist = vec![0.0; size];
            let mut tmp = vec![0.0; size];
            tmp.par_chunks_mut(pw).zip(dist.par_chunks_mut(pw)).enumerate().for_each(|(y, (tmp, dist))| {
                if y < b || y + b >= ph {
                    return;
                }
                for x in b..pw - b {
                    let i = y * pw + x;
                    tmp[x] = (planes[0][i] * wb[0] + planes[1][i] * wb[1] + planes[2][i] * wb[2]) / 3.0;
                    dist[x] = if seg.data[i] == 1 { 1e20 } else { 0.0 };
                }
            });
            extend(&mut tmp, pw, ph, b);
            let lum = blur(&tmp, pw, ph);
            let md = distance_transform(&mut dist, pw, ph);
            if md > 3.0 {
                seg.segmentize(); // Upstream recout aliases refavg[2]; its outer interior row survives initialization.
                let mut gradient = refs[2].clone();
                gradient.par_chunks_mut(pw).enumerate().for_each(|(y, row)| {
                    if y < BORDER + 2 || y + BORDER + 2 >= ph {
                        return;
                    }
                    for x in BORDER + 2..pw - BORDER - 2 {
                        let i = y * pw + x;
                        row[x] = if dist[i] > 0.0 && dist[i] < 2.0 { 4.0 * scharr(&lum, i, pw) } else { 0.0 };
                    }
                });
                extend(&mut gradient, pw, ph, b);
                for id in 2..seg.nr as usize {
                    let mut max = 0.0f32;
                    for y in (seg.ymin[id] - 2).max(seg.border)..(seg.ymax[id] + 3).min(seg.height - seg.border) {
                        for x in (seg.xmin[id] - 2).max(seg.border)..(seg.xmax[id] + 3).min(seg.width - seg.border) {
                            let i = (y * seg.width + x) as usize;
                            if seg.data[i] == id as i32 {
                                max = max.max(dist[i]);
                            }
                        }
                    }
                    seg.val1[id] = max;
                    if max > 2.0 {
                        segment_gradients(&dist, &mut gradient, seg, id, opts.recovery);
                    }
                }
                let mut gradient = blur(&gradient, pw, ph);
                if opts.noise > 0.0 {
                    for id in 2..seg.nr as usize {
                        if seg.val1[id] > 3.0 {
                            add_noise(&mut gradient, seg, id, opts.noise);
                        }
                    }
                }
                let shift = 2.0 + opts.recovery.closing() as f32;
                output.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                    if y == 0 || y + 1 >= h {
                        return;
                    }
                    for x in 1..w - 1 {
                        let i = y * w + x;
                        let c = cfa.color_at(x, y) as usize;
                        if input[i].max(0.0) > clips[c] {
                            let o = raw_to_plane(pw, y, x);
                            let effect = opts.strength / (1.0 + (-(dist[o] - shift)).exp());
                            row[x] += (gradient[o] * effect).max(0.0);
                        }
                    }
                });
            }
        }
    }
    n.data.par_iter_mut().enumerate().for_each(|(i, v)| {
        let c = cfa.color_at(i % w, i / w) as usize;
        // Preserve unclipped samples bit-for-bit across the temporary WB conversion.
        if input[i] >= clips[c] {
            *v = output[i] / wb[c];
        }
    });
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonneutral_wb_keeps_unclipped_samples_and_handles_all_clipped() {
        for cfa in [Cfa::bayer("GRBG").unwrap(), Cfa::xtrans()] {
            for all in [false, true] {
                let (w, h) = (66, 60);
                let data: Vec<_> = (0..w * h).map(|i| if all || (i / w > 20 && i % w > 20) { 1.0 } else { 0.4 + 0.01 * (i % 7) as f32 }).collect();
                let mut n = Normalized { width: w, height: h, cpp: 1, data: data.clone(), cfa: Some(cfa.clone()) };
                segmentation(
                    &mut n,
                    [1.7, 0.85, 2.3],
                    0.99,
                    &SegmentationOptions { recovery: Recovery::AdaptiveFlat, strength: 0.2, ..Default::default() },
                );
                for (&before, after) in data.iter().zip(&n.data) {
                    assert!(after.is_finite());
                    if before < 0.99 {
                        assert_eq!(before.to_bits(), after.to_bits());
                    }
                }
            }
        }
    }

    #[test]
    fn upstream_vectors() {
        let (w, h) = (192, 180);
        for pat in ["RGGB", "BGGR", "GRBG", "GBRG", "XTRANS"] {
            let cfa = if pat == "XTRANS" { Cfa::xtrans() } else { Cfa::bayer(pat).unwrap() };
            for (mode, recovery) in [
                Recovery::Off,
                Recovery::Small,
                Recovery::Large,
                Recovery::SmallFlat,
                Recovery::LargeFlat,
                Recovery::Adaptive,
                Recovery::AdaptiveFlat,
                Recovery::Off,
                Recovery::AdaptiveFlat,
            ]
            .into_iter()
            .enumerate()
            {
                let data = (0..w * h)
                    .map(|i| {
                        let (x, y) = ((i % w) as i32, (i / w) as i32);
                        let c = cfa.color_at(x as usize, y as usize);
                        let v = 0.22 + 0.005 * x as f32 + 0.001 * y as f32;
                        if (x - 90) * (x - 90) + (y - 80) * (y - 80) < 40 * 40 || (x * 13 + y * 7) % 331 == 0 {
                            1.06
                        } else {
                            (v * if c == 0 {
                                1.4
                            } else if c == 1 {
                                1.0
                            } else {
                                0.8
                            })
                            .min(1.0)
                        }
                    })
                    .collect();
                let mut n = Normalized { width: w, height: h, cpp: 1, data, cfa: Some(cfa.clone()) };
                segmentation(
                    &mut n,
                    if mode < 7 { [1.0; 3] } else { [1.7, 0.85, 2.3] },
                    0.99,
                    &SegmentationOptions {
                        recovery,
                        strength: 0.2,
                        noise: if mode == 6 || mode == 8 { 0.01 } else { 0.0 },
                        combine: if mode == 4 { 8 } else { 2 },
                        ..SegmentationOptions::default()
                    },
                );
                let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/segbased/{pat}-{mode}.f32")))
                    .unwrap();
                assert_eq!(bytes.len(), 64 * 60 * 4);
                let actual = (0..h).step_by(3).flat_map(|y| (0..w).step_by(3).map(move |x| y * w + x)).map(|i| n.data[i]);
                let mut max = 0.0f32;
                let mut sum = 0.0f64;
                let mut count = 0;
                for (a, b) in actual.zip(bytes.as_chunks::<4>().0.iter()) {
                    let b = f32::from_le_bytes(*b);
                    let d = (a - b).abs();
                    assert!(a.is_finite());
                    max = max.max(d);
                    sum += f64::from(d * d);
                    count += 1;
                }
                eprintln!("segbased {pat} mode {mode}: n {count} max {max:.9} rms {:.9}", (sum / count as f64).sqrt());
                assert!(max < 2e-6, "{pat} {mode} {max}");
            }
        }
    }
}
