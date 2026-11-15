//! Select Subject (classical, heuristic).
//!
//! Photoshop uses a learned model; we approximate it with salient-object detection feeding a
//! GrabCut:
//!
//! 1. SLIC superpixels on a working-resolution copy.
//! 2. Per-superpixel saliency from "Saliency Optimization from Robust Background Detection"
//!    (W. Zhu, S. Liang, Y. Wei and J. Sun, CVPR 2014): boundary connectivity
//!    `BndCon(p) = Len_bnd(p) / sqrt(Area(p))` over geodesic colour distances gives a background
//!    probability; background-weighted contrast gives foreground cues; a least-squares
//!    optimisation with smoothness between neighbours combines them.
//! 3. Blended with frequency-tuned saliency (R. Achanta, S. Hemami, F. Estrada and
//!    S. Süsstrunk, "Frequency-tuned Salient Region Detection", CVPR 2009): distance of the
//!    lightly blurred CIELAB image from its mean colour.
//! 4. Otsu's threshold on the saliency map seeds a GrabCut trimap (low-saliency border pixels
//!    are definite background); the result is cleaned (small islands dropped unless they lie
//!    next to the main part, such as a head cut off by a neck too thin for the working
//!    resolution; small holes filled) and re-cut at full resolution.
//!
//! Works well for a distinct object on a comparatively plain background; it is not a semantic
//! segmenter.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use photocraft_geom::Rect;

use super::grabcut::{self, BG, PR_BG, PR_FG};
use super::slic::{rgb_to_lab, slic_lab};
use super::{Region, Sampler};

/// Working-resolution budget (pixels).
pub const WORK_PX: usize = 110_000;
const SIGMA_CLR: f32 = 10.0;
const SIGMA_SPA: f32 = 0.25;

/// Selects the most salient object on the canvas.
pub fn select_subject(sampler: &dyn Sampler, canvas: Rect) -> Option<Region> {
    if canvas.width() < 4 || canvas.height() < 4 {
        return None;
    }
    let step = super::scale_for(canvas, WORK_PX);
    let img = sampler.rgb_scaled(canvas, step);
    let (w, h) = (img.w, img.h);
    let sal = saliency(&img);
    let t = otsu(&sal).clamp(0.15, 0.85);
    let mut trimap = vec![PR_BG; w * h];
    let mut any_fg = false;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let border = x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            trimap[i] = if sal[i] >= t {
                any_fg = true;
                PR_FG
            } else if border || sal[i] < 0.4 * t {
                BG
            } else {
                PR_BG
            };
        }
    }
    if !any_fg {
        return None;
    }
    let (fg, bg) = grabcut::grabcut(&img, &mut trimap, 6)?;
    let low: Vec<bool> = trimap.iter().map(|v| *v == PR_FG || *v == grabcut::FG).collect();
    let low = super::clean_mask(&keep_main_and_nearby(&low, w, h), w, h, 0.0, 0.05);
    if !low.iter().any(|v| *v) {
        return None;
    }
    super::finish_region(sampler, canvas, step, &low, w, h, Some((&fg, &bg)))
}

/// The connected parts of `mask` that make up the subject: those at least a fifth the size of
/// the largest, and smaller ones (not specks) with pixels close to it. A part of the subject the
/// cut separated from the rest (a head whose thin neck the working resolution lost, a tail,
/// something held) would otherwise go missing. Clutter further away is still dropped, and so is
/// a small narrow strip along an image edge.
fn keep_main_and_nearby(mask: &[bool], w: usize, h: usize) -> Vec<bool> {
    let (lab, sizes) = super::components(mask, w, h);
    let Some((main, largest)) = sizes.iter().copied().enumerate().skip(1).max_by_key(|(_, n)| *n) else { return vec![false; w * h] };
    // Bounding box per component: (x0, y0, x1, y1), inclusive.
    let mut bbox = vec![(usize::MAX, usize::MAX, 0usize, 0usize); sizes.len()];
    for (i, l) in lab.iter().enumerate() {
        let (x, y) = (i % w, i / w);
        let b = &mut bbox[*l as usize];
        *b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    }
    let (mx0, my0, mx1, my1) = bbox[main];
    let margin = ((mx1 - mx0 + 1).max(my1 - my0 + 1) as f32 * 0.15).ceil() as usize;
    // A part is near when some of its pixels lie within the margin, not merely its box: a thin
    // diagonal streak from a corner (a cable, a rail) can have a box that reaches the margin.
    let mut near = vec![false; sizes.len()];
    for y in my0.saturating_sub(margin)..=(my1 + margin).min(h - 1) {
        for x in mx0.saturating_sub(margin)..=(mx1 + margin).min(w - 1) {
            near[lab[y * w + x] as usize] = true;
        }
    }
    let (big, speck) = ((largest as f32 * 0.2).ceil() as usize, ((largest as f32 * 0.005).ceil() as usize).max(4));
    let keep: Vec<bool> = (0..sizes.len())
        .map(|l| {
            if l == 0 {
                return false;
            }
            if l == main || sizes[l] >= big {
                return true;
            }
            let (x0, y0, x1, y1) = bbox[l];
            let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
            // Edge clutter: a strip on an image edge, narrow against the canvas and at least
            // twice as long as it is wide.
            let strip = ((x0 == 0 || x1 + 1 == w) && bh < h * 12 / 100 && bw >= bh * 2) || ((y0 == 0 || y1 + 1 == h) && bw < w * 12 / 100 && bh >= bw * 2);
            sizes[l] >= speck && near[l] && !strip
        })
        .collect();
    lab.iter().map(|l| keep[*l as usize]).collect()
}

/// Per-pixel saliency in `0..=1` (see the module docs).
pub fn saliency(img: &super::RgbImage) -> Vec<f32> {
    let (w, h) = (img.w, img.h);
    let lab: Vec<[f32; 3]> = img.px.iter().map(|p| rgb_to_lab(*p)).collect();
    let target = ((w * h) / 150).clamp(16, 400);
    let sp = slic_lab(&lab, w, h, target, 10.0, 8);
    let n = sp.count;
    // Region statistics.
    let mut mean = vec![[0.0f64; 3]; n];
    let mut pos = vec![[0.0f64; 2]; n];
    let mut cnt = vec![0.0f64; n];
    let mut on_border = vec![false; n];
    for y in 0..h {
        for x in 0..w {
            let l = sp.labels[y * w + x] as usize;
            let c = lab[y * w + x];
            for k in 0..3 {
                mean[l][k] += c[k] as f64;
            }
            pos[l][0] += x as f64 / w as f64;
            pos[l][1] += y as f64 / h as f64;
            cnt[l] += 1.0;
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                on_border[l] = true;
            }
        }
    }
    let mean: Vec<[f32; 3]> = mean.iter().zip(&cnt).map(|(m, c)| m.map(|v| (v / c.max(1.0)) as f32)).collect();
    let pos: Vec<[f32; 2]> = pos.iter().zip(&cnt).map(|(p, c)| p.map(|v| (v / c.max(1.0)) as f32)).collect();
    let dapp = |a: usize, b: usize| super::d2(mean[a], mean[b]).sqrt();
    // Adjacency.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut link = |a: usize, b: usize| {
        if a != b && !adj[a].contains(&b) {
            adj[a].push(b);
            adj[b].push(a);
        }
    };
    for y in 0..h {
        for x in 0..w {
            let l = sp.labels[y * w + x] as usize;
            if x + 1 < w {
                link(l, sp.labels[y * w + x + 1] as usize);
            }
            if y + 1 < h {
                link(l, sp.labels[(y + 1) * w + x] as usize);
            }
        }
    }
    // Boundary connectivity from all-pairs geodesic distances (Dijkstra per region).
    let mut wbg = vec![0.0f32; n];
    let mut geo = vec![f32::INFINITY; n];
    for (p, wb) in wbg.iter_mut().enumerate() {
        dijkstra(p, &adj, &dapp, &mut geo);
        let (mut area, mut len) = (0.0f32, 0.0f32);
        for q in 0..n {
            let s = (-geo[q] * geo[q] / (2.0 * SIGMA_CLR * SIGMA_CLR)).exp();
            area += s;
            if on_border[q] {
                len += s;
            }
        }
        let bnd = len / area.max(1e-6).sqrt();
        *wb = 1.0 - (-bnd * bnd / 2.0).exp();
    }
    // Background-weighted contrast.
    let mut wctr = vec![0.0f32; n];
    for p in 0..n {
        let mut s = 0.0;
        for q in 0..n {
            let (dx, dy) = (pos[p][0] - pos[q][0], pos[p][1] - pos[q][1]);
            let wspa = (-(dx * dx + dy * dy) / (2.0 * SIGMA_SPA * SIGMA_SPA)).exp();
            s += dapp(p, q) * wspa * wbg[q];
        }
        wctr[p] = s;
    }
    let mx = wctr.iter().copied().fold(0.0f32, f32::max).max(1e-6);
    let wfg: Vec<f32> = wctr.iter().map(|v| v / mx).collect();
    // Saliency optimisation: min Σ wbg s² + Σ wfg (s−1)² + Σ_{i~j} w_ij (s_i − s_j)² (Gauss–Seidel).
    let wij: Vec<Vec<f32>> = (0..n).map(|p| adj[p].iter().map(|&q| (-dapp(p, q).powi(2) / (2.0 * SIGMA_CLR * SIGMA_CLR)).exp() + 0.1).collect()).collect();
    let mut s = wfg.clone();
    for _ in 0..200 {
        for p in 0..n {
            let (mut num, mut den) = (wfg[p], wbg[p] + wfg[p]);
            for (q, wq) in adj[p].iter().zip(&wij[p]) {
                num += wq * s[*q];
                den += wq;
            }
            s[p] = num / den.max(1e-6);
        }
    }
    // Frequency-tuned saliency.
    let mut mean_all = [0.0f64; 3];
    for c in &lab {
        for k in 0..3 {
            mean_all[k] += c[k] as f64;
        }
    }
    let mean_all = mean_all.map(|v| (v / lab.len() as f64) as f32);
    let blurred = super::RgbImage { w, h, px: lab.clone() };
    let blurred = blur_lab(&blurred);
    let ft: Vec<f32> = blurred.px.iter().map(|c| super::d2(*c, mean_all).sqrt()).collect();
    let ftmax = ft.iter().copied().fold(0.0f32, f32::max).max(1e-6);
    let smin = s.iter().copied().fold(f32::MAX, f32::min);
    let smax = s.iter().copied().fold(f32::MIN, f32::max);
    let srange = (smax - smin).max(1e-6);
    let mut out: Vec<f32> = sp.labels.iter().zip(&ft).map(|(l, f)| 0.75 * (s[*l as usize] - smin) / srange + 0.25 * f / ftmax).collect();
    let (lo, hi) = out.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let r = (hi - lo).max(1e-6);
    for v in &mut out {
        *v = (*v - lo) / r;
    }
    out
}

fn blur_lab(img: &super::RgbImage) -> super::RgbImage {
    let (w, h) = (img.w, img.h);
    let mut cur = img.clone();
    for _ in 0..2 {
        let mut tmp = cur.clone();
        for y in 0..h {
            for x in 0..w {
                let (l, c, r) = (cur.at(x.saturating_sub(1), y), cur.at(x, y), cur.at((x + 1).min(w - 1), y));
                tmp.px[y * w + x] = [0, 1, 2].map(|k| (l[k] + 2.0 * c[k] + r[k]) / 4.0);
            }
        }
        for y in 0..h {
            for x in 0..w {
                let (u, c, d) = (tmp.at(x, y.saturating_sub(1)), tmp.at(x, y), tmp.at(x, (y + 1).min(h - 1)));
                cur.px[y * w + x] = [0, 1, 2].map(|k| (u[k] + 2.0 * c[k] + d[k]) / 4.0);
            }
        }
    }
    cur
}

#[derive(PartialEq)]
struct Item(f32, usize);
impl Eq for Item {}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0)
    }
}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

fn dijkstra(src: usize, adj: &[Vec<usize>], wt: &dyn Fn(usize, usize) -> f32, dist: &mut [f32]) {
    dist.fill(f32::INFINITY);
    dist[src] = 0.0;
    let mut heap = BinaryHeap::new();
    heap.push(Item(0.0, src));
    while let Some(Item(d, u)) = heap.pop() {
        if d > dist[u] {
            continue;
        }
        for &v in &adj[u] {
            let nd = d + wt(u, v);
            if nd < dist[v] {
                dist[v] = nd;
                heap.push(Item(nd, v));
            }
        }
    }
}

/// Otsu's threshold of values in `0..=1` (256 bins).
pub fn otsu(v: &[f32]) -> f32 {
    let mut hist = [0u64; 256];
    for x in v {
        hist[(x.clamp(0.0, 1.0) * 255.0).round() as usize] += 1;
    }
    let total = v.len() as f64;
    let sum: f64 = hist.iter().enumerate().map(|(i, c)| i as f64 * *c as f64).sum();
    let (mut wb, mut sb, mut best, mut bt) = (0.0f64, 0.0f64, -1.0f64, 0usize);
    for (t, c) in hist.iter().enumerate() {
        wb += *c as f64;
        if wb == 0.0 {
            continue;
        }
        let wf = total - wb;
        if wf == 0.0 {
            break;
        }
        sb += t as f64 * *c as f64;
        let (mb, mf) = (sb / wb, (sum - sb) / wf);
        let between = wb * wf * (mb - mf) * (mb - mf);
        if between > best {
            best = between;
            bt = t;
        }
    }
    (bt as f32 + 0.5) / 255.0
}

#[cfg(test)]
mod tests {
    use super::keep_main_and_nearby;

    /// The main 40 x 40 part has a margin of ceil(40 x 0.15) = 6 pixels and a speck floor of
    /// ceil(1600 x 0.005) = 8 pixels: a 3 x 3 part inside the margin is kept (a fifth of the
    /// largest part used to be the floor), a 5 x 5 one past the margin and a 2 x 3 one inside it
    /// are dropped, and so is a narrow strip down from the top edge that reaches the margin.
    #[test]
    fn small_parts_next_to_the_main_part_are_kept_and_clutter_is_dropped() {
        let side = 240;
        let mut mask = vec![false; side * side];
        for (x0, y0, x1, y1) in [(100, 100, 140, 140), (120, 143, 123, 146), (147, 110, 152, 115), (110, 143, 112, 146), (112, 0, 115, 97)] {
            for y in y0..y1 {
                for x in x0..x1 {
                    mask[y * side + x] = true;
                }
            }
        }
        let kept = keep_main_and_nearby(&mask, side, side);
        assert!(kept[100 * side + 100], "the main part");
        assert!(kept[143 * side + 120], "a part inside the margin");
        assert!(!kept[110 * side + 147], "clutter past the margin");
        assert!(!kept[143 * side + 110], "a speck inside the margin");
        assert!(!kept[112], "a narrow strip on the top edge");
    }
}
