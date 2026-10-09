//! Seamless cloning with mean value coordinates: Edit › Paste Special › Paste Seamless and the
//! Clone Stamp's Seamless option.
//!
//! Method: Z. Farbman, G. Hoffer, Y. Lipman, D. Cohen-Or, D. Lischinski, *Coordinates for
//! Instant Image Cloning*, ACM SIGGRAPH 2009. Poisson cloning (Pérez et al. 2003, see
//! [`crate::poisson`]) adds to the pasted patch a *membrane* that interpolates the colour mismatch
//! along the patch boundary harmonically. Farbman et al. replace the harmonic interpolant by a
//! mean-value interpolant, `r(x) = Σ λ_i(x) (target − source)(p_i)` over the boundary points `p_i`,
//! with the mean value coordinates `λ_i = w_i / Σ w_j`,
//! `w_i = (tan(α_{i−1}/2) + tan(α_i/2)) / ‖p_i − x‖` (Floater 2003), using signed angles so that
//! non-convex patches and patches with holes work (Hormann & Floater 2006). No linear system is
//! solved.
//!
//! As in the paper, far boundary is sampled coarsely and near boundary finely (an adaptive
//! hierarchical sampling per point; coarse samples carry the box average of the boundary values
//! they stand for), and the membrane is evaluated on a coarse lattice away from the boundary and
//! interpolated, while pixels near the boundary are evaluated exactly. Implemented from the paper
//! (and Floater's formulas), not from GEGL's `seamless-clone`, whose weights are known to be wrong.
//!
//! The patch boundary is the marching-squares contour of the patch mask on the pixel-centre
//! lattice; each contour vertex sits between an inside and an outside pixel and carries the
//! mismatch at the inside one. Each 4-connected part of the patch is cloned with its own outline
//! and holes.

use std::collections::HashMap;

use photocraft_geom::cage::half_tan;

/// Lattice stride (pixels) for the interior membrane.
const STRIDE: usize = 4;
/// Pixels closer than this to the boundary are evaluated exactly.
const BAND: f32 = 8.0;
/// A boundary run of `len` contour vertices seen from distance `d` is subdivided while
/// `len > SPLIT · d`.
const SPLIT: f64 = 0.5;
/// Coarsest boundary sampling: this many runs per loop.
const ROOT_RUNS: usize = 16;

/// One closed contour: vertex positions and per-vertex values (`ch` per vertex), with prefix
/// sums (twice around) for box averages.
struct Loop {
    pts: Vec<[f64; 2]>,
    prefix: Vec<f64>,
    ch: usize,
}

impl Loop {
    fn new(pts: Vec<[f64; 2]>, vals: &[f32], ch: usize) -> Loop {
        let m = pts.len();
        let mut prefix = vec![0.0; (2 * m + 1) * ch];
        for k in 0..2 * m {
            for c in 0..ch {
                prefix[(k + 1) * ch + c] = prefix[k * ch + c] + f64::from(vals[(k % m) * ch + c]);
            }
        }
        Loop { pts, prefix, ch }
    }

    /// Mean of the values of vertices `[a, a + len)` (indices modulo the loop length), `a < m`.
    fn mean(&self, a: usize, len: usize, out: &mut [f64]) {
        let ch = self.ch;
        for (c, o) in out.iter_mut().enumerate().take(ch) {
            *o = (self.prefix[(a + len) * ch + c] - self.prefix[a * ch + c]) / len as f64;
        }
    }

    /// Adaptive samples of this loop as seen from `x`: (position, value) in loop order.
    fn sample(&self, x: [f64; 2], out: &mut Vec<([f64; 2], [f64; 8])>) {
        let m = self.pts.len();
        let runs = ROOT_RUNS.min(m);
        let mut stack: Vec<(usize, usize)> = Vec::with_capacity(64);
        for r in (0..runs).rev() {
            let (a, b) = (r * m / runs, (r + 1) * m / runs);
            stack.push((a, b - a));
        }
        while let Some((a, len)) = stack.pop() {
            let mid = self.pts[(a + len / 2) % m];
            let d = (mid[0] - x[0]).hypot(mid[1] - x[1]);
            if len > 1 && len as f64 > SPLIT * d {
                let h = len / 2;
                // Depth first, in loop order: the second half is popped after the first.
                stack.push(((a + h) % m, len - h));
                stack.push((a, h));
                continue;
            }
            let mut v = [0.0f64; 8];
            if len == 1 {
                self.mean(a, 1, &mut v);
            } else {
                // The run's representative vertex carries the mean of the values it stands for.
                let start = (a + m - len / 2) % m;
                self.mean(start, len, &mut v);
            }
            out.push((self.pts[a], v));
        }
    }
}

/// Mean value interpolation at `x` of the sampled loops (each a closed polygon, already in loop
/// order). Returns `None` when `x` coincides with a sample (callers never evaluate there).
fn mvc_eval(x: [f64; 2], loops: &[Vec<([f64; 2], [f64; 8])>], ch: usize, out: &mut [f64]) {
    out[..ch].fill(0.0);
    let mut wsum = 0.0;
    for l in loops {
        let n = l.len();
        if n < 2 {
            continue;
        }
        let s: Vec<[f64; 2]> = l.iter().map(|(p, _)| [p[0] - x[0], p[1] - x[1]]).collect();
        let r: Vec<f64> = s.iter().map(|v| v[0].hypot(v[1]).max(1e-12)).collect();
        let t: Vec<f64> = (0..n).map(|i| half_tan(s[i], s[(i + 1) % n], r[i], r[(i + 1) % n])).collect();
        for i in 0..n {
            let w = (t[(i + n - 1) % n] + t[i]) / r[i];
            wsum += w;
            for c in 0..ch {
                out[c] += w * l[i].1[c];
            }
        }
    }
    if wsum.abs() > 1e-300 {
        for o in &mut out[..ch] {
            *o /= wsum;
        }
    }
}

/// 4-connected components of `mask`: labels (0 = outside) and the count.
fn components4(mask: &[bool], w: usize, h: usize) -> (Vec<u32>, u32) {
    let mut lab = vec![0u32; w * h];
    let mut next = 0u32;
    let mut stack = Vec::new();
    for s in 0..w * h {
        if !mask[s] || lab[s] != 0 {
            continue;
        }
        next += 1;
        lab[s] = next;
        stack.push(s);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            let nb = [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)];
            for j in nb.into_iter().flatten() {
                if mask[j] && lab[j] == 0 {
                    lab[j] = next;
                    stack.push(j);
                }
            }
        }
    }
    (lab, next)
}

/// Marching-squares contours of `inside` (pixel-centre lattice, 4-connected inside): loops of
/// (position, index of the inside pixel the vertex belongs to). Outer loops run
/// counter-clockwise (positive signed area), holes clockwise.
fn contours(inside: &dyn Fn(i64, i64) -> bool, x0: i64, y0: i64, x1: i64, y1: i64, w: usize) -> Vec<Vec<([f64; 2], usize)>> {
    // Edge keys in doubled coordinates: (2x+1, 2y) between (x,y)-(x+1,y); (2x, 2y+1) between
    // (x,y)-(x,y+1).
    type Key = (i64, i64);
    let mut segs: Vec<(Key, Key)> = Vec::new();
    for cy in y0 - 1..y1 {
        for cx in x0 - 1..x1 {
            let (a, b, c, d) = (inside(cx, cy), inside(cx + 1, cy), inside(cx + 1, cy + 1), inside(cx, cy + 1));
            let top = (2 * cx + 1, 2 * cy);
            let right = (2 * cx + 2, 2 * cy + 1);
            let bottom = (2 * cx + 1, 2 * cy + 2);
            let left = (2 * cx, 2 * cy + 1);
            let mut e: Vec<Key> = Vec::with_capacity(4);
            if a != b {
                e.push(top);
            }
            if b != c {
                e.push(right);
            }
            if c != d {
                e.push(bottom);
            }
            if d != a {
                e.push(left);
            }
            match e.len() {
                2 => segs.push((e[0], e[1])),
                4 => {
                    // Saddle: inside corners are not connected (4-connectivity): each inside
                    // corner is cut off on its own.
                    if a {
                        segs.push((top, left));
                        segs.push((right, bottom));
                    } else {
                        segs.push((top, right));
                        segs.push((bottom, left));
                    }
                }
                _ => {}
            }
        }
    }
    let mut by_key: HashMap<Key, Vec<usize>> = HashMap::with_capacity(segs.len() * 2);
    for (i, (a, b)) in segs.iter().enumerate() {
        by_key.entry(*a).or_default().push(i);
        by_key.entry(*b).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    for s0 in 0..segs.len() {
        if used[s0] {
            continue;
        }
        used[s0] = true;
        let start = segs[s0].0;
        let mut keys = vec![start];
        let mut cur = segs[s0].1;
        let mut guard = 0usize;
        while cur != start && guard <= segs.len() {
            guard += 1;
            keys.push(cur);
            let Some(next) = by_key.get(&cur).and_then(|v| v.iter().copied().find(|j| !used[*j])) else { break };
            used[next] = true;
            cur = if segs[next].0 == cur { segs[next].1 } else { segs[next].0 };
        }
        if keys.len() < 3 {
            continue;
        }
        let verts: Vec<([f64; 2], usize)> = keys
            .iter()
            .map(|&(kx, ky)| {
                // The two lattice pixels the edge joins; the vertex belongs to the inside one.
                let (p, q) = if kx % 2 != 0 { ((kx - 1) / 2, ky / 2) } else { (kx / 2, (ky - 1) / 2) };
                let other = if kx % 2 != 0 { (p + 1, q) } else { (p, q + 1) };
                let ins = if inside(p, q) { (p, q) } else { other };
                ([kx as f64 / 2.0, ky as f64 / 2.0], ins.1 as usize * w + ins.0 as usize)
            })
            .collect();
        loops.push(verts);
    }
    // Orientation: the loop with the largest area is the outline (counter-clockwise), the rest
    // are holes (clockwise).
    let area = |l: &Vec<([f64; 2], usize)>| -> f64 {
        let n = l.len();
        (0..n).map(|i| l[i].0[0] * l[(i + 1) % n].0[1] - l[(i + 1) % n].0[0] * l[i].0[1]).sum::<f64>()
    };
    let outer = (0..loops.len()).max_by(|a, b| area(&loops[*a]).abs().total_cmp(&area(&loops[*b]).abs()));
    for (i, l) in loops.iter_mut().enumerate() {
        let a = area(l);
        if (Some(i) == outer) != (a > 0.0) {
            l.reverse();
        }
    }
    loops
}

/// Chamfer distance (3-4) of every inside pixel to the nearest outside pixel, in pixels.
fn inside_distance(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    let big = f32::MAX / 4.0;
    let mut d: Vec<f32> = mask.iter().map(|m| if *m { big } else { 0.0 }).collect();
    let at = |d: &Vec<f32>, x: i64, y: i64| if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 { 0.0 } else { d[y as usize * w + x as usize] };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let v = d[i]
                .min(at(&d, x - 1, y) + 1.0)
                .min(at(&d, x, y - 1) + 1.0)
                .min(at(&d, x - 1, y - 1) + std::f32::consts::SQRT_2)
                .min(at(&d, x + 1, y - 1) + std::f32::consts::SQRT_2);
            d[i] = v;
        }
    }
    for y in (0..h as i64).rev() {
        for x in (0..w as i64).rev() {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let v = d[i]
                .min(at(&d, x + 1, y) + 1.0)
                .min(at(&d, x, y + 1) + 1.0)
                .min(at(&d, x + 1, y + 1) + std::f32::consts::SQRT_2)
                .min(at(&d, x - 1, y + 1) + std::f32::consts::SQRT_2);
            d[i] = v;
        }
    }
    d
}

/// The mean-value membrane of a patch: for every pixel of `mask` (row-major `w × h`), the
/// interpolation of `diff` (`ch` values per pixel, read on the patch's boundary pixels) from the
/// patch boundary. Pixels outside the mask get 0. `ch` is at most 8.
pub fn mvc_membrane(w: usize, h: usize, mask: &[bool], diff: &[f32], ch: usize) -> Vec<f32> {
    let ch = ch.min(8);
    let mut out = vec![0.0f32; w * h * ch];
    if w == 0 || h == 0 || mask.len() < w * h || diff.len() < w * h * ch {
        return out;
    }
    let (lab, count) = components4(mask, w, h);
    let dist = inside_distance(mask, w, h);
    for comp in 1..=count {
        // Bounding box of the component.
        let (mut bx0, mut by0, mut bx1, mut by1) = (usize::MAX, usize::MAX, 0, 0);
        for (i, l) in lab.iter().enumerate() {
            if *l == comp {
                let (x, y) = (i % w, i / w);
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x + 1);
                by1 = by1.max(y + 1);
            }
        }
        let inside = |x: i64, y: i64| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && lab[y as usize * w + x as usize] == comp;
        let raw = contours(&inside, bx0 as i64, by0 as i64, bx1 as i64, by1 as i64, w);
        let loops: Vec<Loop> = raw
            .into_iter()
            .map(|l| {
                let vals: Vec<f32> = l.iter().flat_map(|(_, i)| diff[i * ch..(i + 1) * ch].iter().copied()).collect();
                Loop::new(l.into_iter().map(|(p, _)| p).collect(), &vals, ch)
            })
            .collect();
        let eval = |x: f64, y: f64| -> [f64; 8] {
            let mut sampled: Vec<Vec<([f64; 2], [f64; 8])>> = Vec::with_capacity(loops.len());
            for l in &loops {
                let mut s = Vec::with_capacity(64);
                l.sample([x, y], &mut s);
                sampled.push(s);
            }
            let mut v = [0.0f64; 8];
            mvc_eval([x, y], &sampled, ch, &mut v);
            v
        };
        // Lattice over the component's box (nodes at multiples of STRIDE from its corner, plus
        // the far edge), evaluated where an interior pixel needs it.
        let (cw, chh) = (bx1 - bx0, by1 - by0);
        let (lw, lh) = (cw.div_ceil(STRIDE) + 1, chh.div_ceil(STRIDE) + 1);
        let mut need = vec![false; lw * lh];
        let mut far_any = false;
        for y in by0..by1 {
            for x in bx0..bx1 {
                let i = y * w + x;
                if lab[i] == comp && dist[i] >= BAND {
                    far_any = true;
                    let (gx, gy) = ((x - bx0) / STRIDE, (y - by0) / STRIDE);
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        need[(gy + dy).min(lh - 1) * lw + (gx + dx).min(lw - 1)] = true;
                    }
                }
            }
        }
        let node_pos = |k: usize| ((bx0 + (k % lw) * STRIDE) as f64, (by0 + (k / lw) * STRIDE) as f64);
        let nodes: Vec<usize> = if far_any { (0..lw * lh).filter(|k| need[*k]).collect() } else { Vec::new() };
        let node_vals: Vec<(usize, [f64; 8])> = {
            let f = |k: &usize| {
                let (x, y) = node_pos(*k);
                (*k, eval(x, y))
            };
            #[cfg(not(target_arch = "wasm32"))]
            {
                use rayon::prelude::*;
                nodes.par_iter().map(f).collect()
            }
            #[cfg(target_arch = "wasm32")]
            {
                nodes.iter().map(f).collect()
            }
        };
        let mut lattice = vec![[0.0f64; 8]; lw * lh];
        for (k, v) in node_vals {
            lattice[k] = v;
        }
        // Every pixel: exact near the boundary, interpolated from the lattice inside.
        let rows: Vec<(usize, Vec<f32>)> = {
            let row = |y: &usize| -> (usize, Vec<f32>) {
                let mut r = vec![0.0f32; cw * ch];
                for x in bx0..bx1 {
                    let i = *y * w + x;
                    if lab[i] != comp {
                        continue;
                    }
                    let v = if dist[i] < BAND {
                        eval(x as f64, *y as f64)
                    } else {
                        let (gx, gy) = ((x - bx0) / STRIDE, (*y - by0) / STRIDE);
                        let (fx, fy) = (((x - bx0) % STRIDE) as f64 / STRIDE as f64, ((*y - by0) % STRIDE) as f64 / STRIDE as f64);
                        let n = |dx: usize, dy: usize| lattice[(gy + dy).min(lh - 1) * lw + (gx + dx).min(lw - 1)];
                        let (a, b, c, d) = (n(0, 0), n(1, 0), n(0, 1), n(1, 1));
                        std::array::from_fn(|k| {
                            let top = a[k] + (b[k] - a[k]) * fx;
                            let bot = c[k] + (d[k] - c[k]) * fx;
                            top + (bot - top) * fy
                        })
                    };
                    for c in 0..ch {
                        r[(x - bx0) * ch + c] = v[c] as f32;
                    }
                }
                (*y, r)
            };
            let ys: Vec<usize> = (by0..by1).collect();
            #[cfg(not(target_arch = "wasm32"))]
            {
                use rayon::prelude::*;
                ys.par_iter().map(row).collect()
            }
            #[cfg(target_arch = "wasm32")]
            {
                ys.iter().map(row).collect()
            }
        };
        for (y, r) in rows {
            for x in bx0..bx1 {
                let i = y * w + x;
                if lab[i] == comp {
                    out[i * ch..(i + 1) * ch].copy_from_slice(&r[(x - bx0) * ch..(x - bx0 + 1) * ch]);
                }
            }
        }
    }
    out
}

/// Seamless clone: `src` with the membrane of `dst − src` added inside `mask`, on the first
/// `colour` of the `n` channels per pixel (alpha and any further channels are kept from `src`).
/// Outside the mask the result is `src`. Values are not clamped.
pub fn seamless_blend(w: usize, h: usize, n: usize, colour: usize, src: &[f32], dst: &[f32], mask: &[bool]) -> Vec<f32> {
    let pixels = w.saturating_mul(h);
    let required = pixels.saturating_mul(n);
    if src.len() < required || dst.len() < required || mask.len() < pixels {
        return Vec::new();
    }
    let colour = colour.min(n).min(8);
    let mut diff = vec![0.0f32; pixels * colour];
    for i in 0..pixels {
        for c in 0..colour {
            diff[i * colour + c] = dst[i * n + c] - src[i * n + c];
        }
    }
    let mem = mvc_membrane(w, h, mask, &diff, colour);
    let mut out = src.to_vec();
    for i in 0..pixels {
        if mask[i] {
            for c in 0..colour {
                out[i * n + c] += mem[i * colour + c];
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(w: usize, h: usize, cx: f64, cy: f64, r: f64) -> Vec<bool> {
        (0..w * h).map(|i| ((i % w) as f64 - cx).hypot((i / w) as f64 - cy) <= r).collect()
    }

    #[test]
    fn a_constant_offset_patch_blends_to_match_its_border() {
        let (w, h) = (120, 90);
        let mask = disc(w, h, 60.0, 45.0, 35.0);
        // Textured target; the patch is the same texture shifted by a constant colour offset.
        let dst: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                [0.3 + 0.1 * (x * 0.3).sin(), 0.5 + 0.05 * (y * 0.2).cos(), 0.2, 1.0]
            })
            .collect();
        let src: Vec<f32> = dst.as_chunks::<4>().0.iter().flat_map(|p| [p[0] + 0.25, p[1] - 0.2, p[2] + 0.1, 1.0]).collect();
        let out = seamless_blend(w, h, 4, 3, &src, &dst, &mask);
        let mut worst = 0.0f32;
        for i in (0..w * h).filter(|i| mask[*i]) {
            for c in 0..4 {
                worst = worst.max((out[i * 4 + c] - dst[i * 4 + c]).abs());
            }
        }
        assert!(worst < 1e-4, "max error {worst}");
    }

    #[test]
    fn linear_mismatch_is_reproduced_and_holes_count() {
        let (w, h) = (100, 100);
        // A ring: an outline and a hole.
        let mask: Vec<bool> = (0..w * h)
            .map(|i| {
                let d = ((i % w) as f64 - 50.0).hypot((i / w) as f64 - 50.0);
                (12.0..=40.0).contains(&d)
            })
            .collect();
        let diff: Vec<f32> = (0..w * h).map(|i| 0.01 * (i % w) as f32 - 0.005 * (i / w) as f32).collect();
        let m = mvc_membrane(w, h, &mask, &diff, 1);
        let mut worst = 0.0f32;
        for i in (0..w * h).filter(|i| mask[*i]) {
            worst = worst.max((m[i] - diff[i]).abs());
        }
        assert!(worst < 0.01, "linear precision: {worst}");
        assert!(m.iter().zip(&mask).all(|(v, k)| *k || *v == 0.0));
    }

    #[test]
    fn separate_parts_use_their_own_borders() {
        let (w, h) = (80, 40);
        let mask: Vec<bool> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                (5..30).contains(&x) && (5..35).contains(&y) || (45..75).contains(&x) && (5..35).contains(&y)
            })
            .collect();
        let diff: Vec<f32> = (0..w * h).map(|i| if i % w < 40 { 1.0 } else { -1.0 }).collect();
        let m = mvc_membrane(w, h, &mask, &diff, 1);
        assert!((m[20 * w + 17] - 1.0).abs() < 1e-5 && (m[20 * w + 60] + 1.0).abs() < 1e-5);
    }

    #[test]
    fn membrane_is_smooth_inside() {
        let (w, h) = (160, 160);
        let mask = disc(w, h, 80.0, 80.0, 70.0);
        // A boundary mismatch that varies quickly along the border.
        let diff: Vec<f32> = (0..w * h).map(|i| ((i % w) as f32 * 0.7).sin() * ((i / w) as f32 * 0.5).cos()).collect();
        let m = mvc_membrane(w, h, &mask, &diff, 1);
        // Far from the border neighbouring pixels differ little (no seams between the exact band
        // and the interpolated interior).
        let mut worst = 0.0f32;
        for y in 40..120 {
            for x in 40..119 {
                worst = worst.max((m[y * w + x + 1] - m[y * w + x]).abs());
            }
        }
        assert!(worst < 0.02, "{worst}");
    }
}
