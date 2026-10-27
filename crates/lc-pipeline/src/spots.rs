//! Scene-linear Clone spots and the source-based renderer for Heal / Remove spots.
//!
//! New automatic heals are background jobs stored as patches. Source-based Heal / Remove spots
//! (with a source position, e.g. imported Camera Raw `crs:RetouchAreas` and Camera Raw Filter
//! spots) use the same patch/Poisson backend through a bounded region adapter.
//! Clone keeps its original feathered copy renderer and automatic-source APIs.

use li_raster::{RgbFrame, par_rows};
use lightcraft_develop::{Spot, SpotMode};
use lightcraft_geom::Point;

use crate::geometry::Frame;

#[inline]
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Most a spot's size counts for, in long edges: a size read from a sidecar isn't held to the
/// slider's range, and one far past the picture (or infinite) made the radius, the blur it heals
/// with and the patch it reads from unbounded. Two long edges cover the whole frame from anywhere
/// in it.
const MAX_SPOT_SIZE: f64 = 2.0;

/// A spot's radius in output pixels, `ppl` output pixels to the long edge: at least a pixel (a
/// size that is not a number too), at most [`MAX_SPOT_SIZE`] long edges.
fn radius_px(size: f64, ppl: f64) -> f64 {
    let size = if size.is_nan() { 0.0 } else { size.min(MAX_SPOT_SIZE) };
    (size * ppl).max(1.0)
}

/// Samples of `img` on an annulus (24 angles x inner/mid/outer radius).
fn ring_stats(img: &RgbFrame, c: (f32, f32), r0: f32, r1: f32) -> Option<Vec<[f32; 3]>> {
    let mut out = Vec::with_capacity(72);
    let n = 24;
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        for rr in [r0, (r0 + r1) / 2.0, r1] {
            let (x, y) = (c.0 + a.cos() * rr, c.1 + a.sin() * rr);
            if x < 0.0 || y < 0.0 || x >= img.width as f32 || y >= img.height as f32 {
                return None;
            }
            out.push(img.sample_bilinear(x, y));
        }
    }
    Some(out)
}

/// Mismatch of the source's surroundings against the target's ring samples `a`.
fn score(img: &RgbFrame, a: &[[f32; 3]], source: (f32, f32), r: f32) -> f32 {
    let Some(b) = ring_stats(img, source, r * 1.05, r * 1.6) else { return f32::MAX };
    // also compare the inside of the source against the target's surroundings (texture continuity)
    let inside = ring_stats(img, source, r * 0.2, r * 0.9);
    let mut s = 0.0;
    for (p, q) in a.iter().zip(&b) {
        for c in 0..3 {
            let (u, v) = (p[c].max(0.0).sqrt(), q[c].max(0.0).sqrt());
            s += (u - v) * (u - v);
        }
    }
    if let Some(ins) = inside {
        let mean_a: f32 = a.iter().map(|p| p[1].max(0.0).sqrt()).sum::<f32>() / a.len() as f32;
        let var: f32 = ins.iter().map(|p| (p[1].max(0.0).sqrt() - mean_a).powi(2)).sum::<f32>() / ins.len() as f32;
        s += var * a.len() as f32 * 0.5;
    }
    s
}

/// Candidate source offsets (output pixels) for a spot of radius `r` at `target`, best first
/// (candidates reaching outside the image are left out).
pub fn ranked_sources(img: &RgbFrame, target: (f32, f32), r: f32) -> Vec<(f32, f32)> {
    // the target's ring is the same for every candidate: sample it once
    let Some(a) = ring_stats(img, target, r * 1.05, r * 1.6) else { return Vec::new() };
    let mut cand: Vec<(f32, (f32, f32))> = Vec::with_capacity(48);
    for ring in [2.4f32, 3.2, 4.2] {
        for i in 0..16 {
            let ang = i as f32 / 16.0 * std::f32::consts::TAU;
            cand.push((0.0, (ang.cos() * r * ring, ang.sin() * r * ring)));
        }
    }
    // (48 candidates x ~150 samples: too little work to be worth the thread hand-off)
    for c in &mut cand {
        c.0 = score(img, &a, (target.0 + c.1.0, target.1 + c.1.1), r);
    }
    cand.retain(|c| c.0 < f32::MAX);
    cand.sort_by(|a, b| a.0.total_cmp(&b.0));
    cand.into_iter().map(|(_, d)| d).collect()
}

/// Pick a source offset (in output pixels) for a remove/heal spot.
pub fn auto_source(img: &RgbFrame, target: (f32, f32), r: f32) -> (f32, f32) {
    ranked_sources(img, target, r).first().copied().unwrap_or((r * 2.5, 0.0))
}

/// A source offset (normalized, as [`Spot::source_offset`]) for `spot` on `src` developed with
/// `s`: the best match, or with `avoid` (the current offset) the best one at least a spot radius
/// away from it ("refresh source"). `None` when no candidate fits in the image.
///
/// The candidates are scored on the photo with `s`'s AI removals and heals composited
/// ([`crate::patches::apply_spots`]), so a source is never taken from what one of them removed.
/// (This only picks the offset a new spot stores; renders are unaffected.)
pub fn pick_source(
    src: &RgbFrame,
    info: &crate::SourceInfo,
    s: &lightcraft_develop::DevelopSettings,
    spot: &Spot,
    avoid: Option<Point>,
) -> Option<Point> {
    let frame = crate::frame_for(src, info, s, true);
    let (w, h) = frame.fit(512, 512);
    if w == 0 || h == 0 {
        return None;
    }
    let mut img = frame.sample(src, w, h);
    crate::patches::apply_spots(&mut img, &s.spots, &frame);
    let (to_out, to_norm) = (frame.norm_to_out(w, h), frame.out_to_norm(w, h));
    let t = to_out.apply(*spot.points.first()?);
    let r = radius_px(spot.size, frame.px_per_long(w)) as f32;
    let avoid = avoid.map(|o| {
        let (a, b) = (to_out.apply(Point::new(0.0, 0.0)), to_out.apply(o));
        ((b.x - a.x) as f32, (b.y - a.y) as f32)
    });
    let far = |d: &(f32, f32)| avoid.is_none_or(|a| ((d.0 - a.0).powi(2) + (d.1 - a.1).powi(2)).sqrt() > r);
    let d = ranked_sources(&img, (t.x as f32, t.y as f32), r).into_iter().find(far)?;
    let (n0, n1) = (to_norm.apply(t), to_norm.apply(Point::new(t.x + d.0 as f64, t.y + d.1 as f64)));
    Some(Point::new(n1.x - n0.x, n1.y - n0.y))
}

/// Per-stage wall time of [`apply_timed`].
#[derive(Default, Clone, Copy, Debug)]
pub struct Stages {
    /// Automatic source search (candidate scoring).
    pub search: std::time::Duration,
    /// Stroke alpha (feathering) and source-patch sampling over the spot's box.
    pub feather: std::time::Duration,
    /// Patch completion and Poisson seamless blending (zero for Clone).
    pub solve: std::time::Duration,
    /// Blend back into the image.
    pub render: std::time::Duration,
}

/// Apply all spots to `img` (output resolution). `ppl` = output pixels per long-edge unit.
pub fn apply(img: &mut RgbFrame, spots: &[Spot], frame: &Frame, ppl: f64) {
    apply_timed(img, spots, frame, ppl, None);
}

/// [`apply`], adding each stage's time to `stages`.
pub fn apply_timed(img: &mut RgbFrame, spots: &[Spot], frame: &Frame, ppl: f64, mut stages: Option<&mut Stages>) {
    use std::time::Instant;
    if spots.is_empty() {
        return;
    }
    let (w, h) = (img.width, img.height);
    let to_out = frame.norm_to_out(w, h);
    // (AI removals are composited from their stored patches: `crate::patches`)
    for spot in spots.iter().filter(|s| !s.is_ai() && s.opacity > 0.0) {
        let r = radius_px(spot.size, ppl) as f32;
        let pts: Vec<(f32, f32)> = spot
            .points
            .iter()
            .map(|p| {
                let q = to_out.apply(*p);
                (q.x as f32, q.y as f32)
            })
            .collect();
        let Some(&first) = pts.first() else { continue };
        if spot.mode != SpotMode::Clone {
            apply_source_heal(img, spot, &pts, r, to_out, stages.as_deref_mut());
            continue;
        }
        let t0 = Instant::now();
        let offset = match spot.source_offset {
            Some(o) => {
                let a = to_out.apply(Point::new(0.0, 0.0));
                let b = to_out.apply(o);
                ((b.x - a.x) as f32, (b.y - a.y) as f32)
            }
            None => auto_source(img, first, r),
        };
        if let Some(s) = stages.as_deref_mut() {
            s.search += t0.elapsed();
        }
        // Bounding box of the stroke.
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &pts {
            x0 = x0.min(p.0 - r);
            y0 = y0.min(p.1 - r);
            x1 = x1.max(p.0 + r);
            y1 = y1.max(p.1 + r);
        }
        let pad = r;
        let (bx0, by0) = (((x0 - pad).floor().max(0.0)) as usize, ((y0 - pad).floor().max(0.0)) as usize);
        let (bx1, by1) = (((x1 + pad).ceil() as usize).min(w), ((y1 + pad).ceil() as usize).min(h));
        if bx1 <= bx0 || by1 <= by0 {
            continue;
        }
        let (bw, bh) = (bx1 - bx0, by1 - by0);
        let feather = (spot.feather / 100.0).clamp(0.0, 1.0) as f32;
        let opacity = (spot.opacity / 100.0).clamp(0.0, 1.0) as f32;
        let t1 = Instant::now();
        // per pixel of the box: [stroke alpha, source r, g, b]
        let mut asrc = vec![[0.0f32; 4]; bw * bh];
        {
            let img = &*img;
            par_rows(&mut asrc, bw, |y, row| {
                let py = (by0 + y) as f32 + 0.5;
                for (x, px4) in row.iter_mut().enumerate() {
                    let px = (bx0 + x) as f32 + 0.5;
                    let d = pts.iter().map(|p| ((px - p.0).powi(2) + (py - p.1).powi(2)).sqrt()).fold(f32::MAX, f32::min);
                    let inner = r * (1.0 - feather * 0.8);
                    let s = img.sample_bilinear(px + offset.0, py + offset.1);
                    *px4 = [(1.0 - smooth(inner, r, d)) * opacity, s[0], s[1], s[2]];
                }
            });
        }
        if let Some(s) = stages.as_deref_mut() {
            s.feather += t1.elapsed();
        }
        let t3 = Instant::now();
        par_rows(&mut img.data[by0 * w..by1 * w], w, |y, row| {
            for x in 0..bw {
                let i = y * bw + x;
                let a = asrc[i][0];
                if a <= 0.0 {
                    continue;
                }
                let v = [asrc[i][1], asrc[i][2], asrc[i][3]];
                let t = row[bx0 + x];
                row[bx0 + x] = [t[0] + (v[0] - t[0]) * a, t[1] + (v[1] - t[1]) * a, t[2] + (v[2] - t[2]) * a].map(|c| c.max(0.0));
            }
        });
        if let Some(s) = stages.as_deref_mut() {
            s.render += t3.elapsed();
        }
    }
}

/// An inclusive-exclusive pixel box.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PxBox {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl PxBox {
    fn width(&self) -> usize {
        self.x1 - self.x0
    }
    fn height(&self) -> usize {
        self.y1 - self.y0
    }
}

/// Dilate a `w × h` mask by `r` pixels (square structuring element, separable).
fn dilate(w: usize, h: usize, m: &[bool], r: usize) -> Vec<bool> {
    let mut tmp = vec![false; w * h];
    for y in 0..h {
        for x in (0..w).filter(|x| m[y * w + x]) {
            tmp[y * w + x.saturating_sub(r)..y * w + (x + r + 1).min(w)].fill(true);
        }
    }
    let mut out = vec![false; w * h];
    for y in 0..h {
        for x in (0..w).filter(|x| tmp[y * w + x]) {
            for yy in y.saturating_sub(r)..(y + r + 1).min(h) {
                out[yy * w + x] = true;
            }
        }
    }
    out
}

/// Content-aware healing of `cov` (the stroke's coverage) in `pixels` (`w × h`, linear, the
/// source's space), as Compositing's Spot Healing Brush does it: PatchMatch completion of the
/// stroke from the texture around it, then a Poisson seamless blend into the surroundings. Works
/// through a perceptual encoding (the area's bright end at 1, a 2.2 gamma) so the patch search
/// weighs shadows as the eye does. Returns the region healed (as it was away from the stroke).
pub fn heal_patch(
    pixels: &[[f32; 3]],
    w: usize,
    h: usize,
    cov: &[f32],
    seed: u64,
    interrupt: &lightcraft_sysmem::jobs::Interrupt<'_>,
) -> Result<(Vec<[f32; 3]>, Vec<f32>), lightcraft_sysmem::jobs::Cancelled> {
    heal_patch_from(pixels, w, h, cov, seed, None, interrupt)
}

/// As [`heal_patch`], with a user-selected source supplying the texture instead of PatchMatch.
/// Both paths use the same perceptual encoding, domain and Poisson solve.
fn heal_patch_from(
    pixels: &[[f32; 3]],
    w: usize,
    h: usize,
    cov: &[f32],
    seed: u64,
    source: Option<&[[f32; 3]]>,
    interrupt: &lightcraft_sysmem::jobs::Interrupt<'_>,
) -> Result<(Vec<[f32; 3]>, Vec<f32>), lightcraft_sysmem::jobs::Cancelled> {
    interrupt.check()?;
    use photocraft_algo::inpaint::{CompleteParams, complete_with};
    use photocraft_algo::poisson;
    const GAMMA: f32 = 2.2;
    let Some(hb) = bbox(cov, w, h) else { return Ok((pixels.to_vec(), vec![0.; pixels.len()])) };
    let hole: Vec<bool> = cov.iter().map(|c| *c > 0.004).collect();
    // the encoding: the area's 99th percentile at 1
    let mut peak: Vec<f32> = pixels.iter().map(|p| p[0].max(p[1]).max(p[2])).filter(|v| v.is_finite()).collect();
    let k = if peak.is_empty() {
        1.0
    } else {
        let i = ((peak.len() - 1) as f32 * 0.99) as usize;
        let (_, p99, _) = peak.select_nth_unstable_by(i, f32::total_cmp);
        1.0 / p99.max(1e-6)
    };
    let img: Vec<f32> = pixels.iter().flat_map(|p| p.map(|v| if v.is_finite() { (v.max(0.0) * k).powf(1.0 / GAMMA) } else { 0.0 })).collect();
    // fill a slightly larger domain than the blend's, so the blend's border carries filled values
    // that differ from the surroundings: that mismatch is what the membrane corrects
    let domain = dilate(w, h, &hole, 2);
    let blend = dilate(w, h, &hole, 1);
    let params = CompleteParams { seed, ..CompleteParams::default() };
    let filled = match source {
        Some(source) => source.iter().flat_map(|p| p.map(|v| if v.is_finite() { (v.max(0.0) * k).powf(1.0 / GAMMA) } else { 0.0 })).collect(),
        None => complete_with(w, h, 3, &img, &domain, &params, interrupt)?.unwrap_or_else(|| poisson::membrane_fill(w, h, 3, &img, &domain)),
    };
    interrupt.check()?;
    // the seamless clone over the stroke's box (with a border of known pixels around it)
    let r = inflate(hb, 2, w, h);
    let (cw, ch) = (r.width(), r.height());
    let crop = |src: &[f32]| -> Vec<f32> { (r.y0..r.y1).flat_map(|y| src[(y * w + r.x0) * 3..(y * w + r.x1) * 3].iter().copied()).collect() };
    let mask: Vec<bool> = (r.y0..r.y1).flat_map(|y| blend[y * w + r.x0..y * w + r.x1].iter().copied()).collect();
    let healed = poisson::seamless_clone(cw, ch, 3, &crop(&filled), &crop(&img), &mask);
    let mut out = pixels.to_vec();
    for y in 0..ch {
        for x in 0..cw {
            if mask[y * cw + x] {
                let j = (y * cw + x) * 3;
                out[(r.y0 + y) * w + r.x0 + x] = [0, 1, 2].map(|c| healed[j + c].max(0.0).powf(GAMMA) / k);
            }
        }
    }
    interrupt.check()?;
    // Hard masks retain the solved border of the established LOCAL path. A feathered brush
    // must composite with its original coverage, rather than silently turning its edge hard.
    let alpha = if cov.iter().any(|c| *c > 0.0 && *c < 1.0) {
        cov.iter().map(|c| c.clamp(0.0, 1.0)).collect()
    } else {
        blend.into_iter().map(|covered| if covered { 1.0 } else { 0.0 }).collect()
    };
    Ok((out, alpha))
}

/// The box of coverage above a trace, `None` when there is none.
fn bbox(cov: &[f32], w: usize, h: usize) -> Option<PxBox> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if cov[y * w + x] > 0.004 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    (x1 > x0 && y1 > y0).then_some(PxBox { x0, y0, x1, y1 })
}

/// `b` grown by `m` on every side, within the image.
fn inflate(b: PxBox, m: usize, w: usize, h: usize) -> PxBox {
    PxBox { x0: b.x0.saturating_sub(m), y0: b.y0.saturating_sub(m), x1: (b.x1 + m).min(w), y1: (b.y1 + m).min(h) }
}

/// Source-based spots keep their coordinates, source pin, feather and opacity. The work is
/// restricted to the stroke plus three diameters of context, just like the background worker.
fn apply_source_heal(img: &mut RgbFrame, spot: &Spot, pts: &[(f32, f32)], r: f32, to_out: lightcraft_geom::Affine, stages: Option<&mut Stages>) {
    use std::time::Instant;
    let start = Instant::now();
    let margin = (r * 6.0).ceil().clamp(48.0, 600.0);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in pts {
        (x0, y0, x1, y1) = (x0.min(x - r - margin), y0.min(y - r - margin), x1.max(x + r + margin), y1.max(y + r + margin));
    }
    let (bx, by) = (x0.floor().max(0.0) as usize, y0.floor().max(0.0) as usize);
    let (ex, ey) = ((x1.ceil().max(0.0) as usize).min(img.width), (y1.ceil().max(0.0) as usize).min(img.height));
    if ex <= bx || ey <= by {
        return;
    }
    let (w, h) = (ex - bx, ey - by);
    let pixels: Vec<_> = (0..w * h).map(|i| img.get(bx + i % w, by + i / w)).collect();
    let inner = r * (1.0 - (spot.feather / 100.0).clamp(0.0, 1.0) as f32 * 0.8);
    let cov: Vec<_> = (0..w * h)
        .map(|i| {
            let (x, y) = ((bx + i % w) as f32 + 0.5, (by + i / w) as f32 + 0.5);
            let d = pts.iter().map(|p| ((x - p.0).powi(2) + (y - p.1).powi(2)).sqrt()).fold(f32::MAX, f32::min);
            1.0 - smooth(inner, r, d)
        })
        .collect();
    let source = spot.source_offset.map(|offset| {
        let (a, b) = (to_out.apply(Point::new(0.0, 0.0)), to_out.apply(offset));
        (0..w * h)
            .map(|i| img.sample_bilinear((bx + i % w) as f32 + 0.5 + (b.x - a.x) as f32, (by + i / w) as f32 + 0.5 + (b.y - a.y) as f32))
            .collect::<Vec<_>>()
    });
    let gather = start.elapsed();
    let solve = Instant::now();
    let Ok((out, _)) = heal_patch_from(&pixels, w, h, &cov, 1, source.as_deref(), &lightcraft_sysmem::jobs::Interrupt::NONE) else { return };
    let solved = solve.elapsed();
    let render = Instant::now();
    let opacity = (spot.opacity / 100.0).clamp(0.0, 1.0) as f32;
    for (i, p) in out.iter().enumerate() {
        let a = cov[i] * opacity;
        if a > 0.0 {
            img.set(bx + i % w, by + i / w, std::array::from_fn(|c| (pixels[i][c] + (p[c] - pixels[i][c]) * a).max(0.0)));
        }
    }
    if let Some(stages) = stages {
        stages.feather += gather;
        stages.solve += solved;
        stages.render += render.elapsed();
    }
}

#[cfg(test)]
/// The straightforward pre-optimisation implementation, kept to prove the fast one is bit-identical.
mod reference {
    use super::*;
    pub(super) fn local_heal_reference(pixels: &[[f32; 3]], w: usize, h: usize, cov: &[f32], seed: u64) -> (Vec<[f32; 3]>, Vec<f32>) {
        use photocraft_algo::inpaint::{CompleteParams, complete_with};
        use photocraft_algo::poisson;
        const GAMMA: f32 = 2.2;
        let Some(hb) = bbox(cov, w, h) else { return (pixels.to_vec(), vec![0.; pixels.len()]) };
        let hole: Vec<bool> = cov.iter().map(|c| *c > 0.004).collect();
        // the encoding: the area's 99th percentile at 1
        let mut peak: Vec<f32> = pixels.iter().map(|p| p[0].max(p[1]).max(p[2])).filter(|v| v.is_finite()).collect();
        let k = if peak.is_empty() {
            1.0
        } else {
            let i = ((peak.len() - 1) as f32 * 0.99) as usize;
            let (_, p99, _) = peak.select_nth_unstable_by(i, f32::total_cmp);
            1.0 / p99.max(1e-6)
        };
        let img: Vec<f32> = pixels.iter().flat_map(|p| p.map(|v| if v.is_finite() { (v.max(0.0) * k).powf(1.0 / GAMMA) } else { 0.0 })).collect();
        // fill a slightly larger domain than the blend's, so the blend's border carries filled values
        // that differ from the surroundings: that mismatch is what the membrane corrects
        let domain = dilate(w, h, &hole, 2);
        let blend = dilate(w, h, &hole, 1);
        let cancel = || false;
        let progress = |_: f32| {};
        let interrupt = lightcraft_sysmem::jobs::Interrupt::new(&cancel, &progress);
        let params = CompleteParams { seed, ..CompleteParams::default() };
        let filled =
            complete_with(w, h, 3, &img, &domain, &params, &interrupt).unwrap().unwrap_or_else(|| poisson::membrane_fill(w, h, 3, &img, &domain));

        // the seamless clone over the stroke's box (with a border of known pixels around it)
        let r = inflate(hb, 2, w, h);
        let (cw, ch) = (r.width(), r.height());
        let crop = |src: &[f32]| -> Vec<f32> { (r.y0..r.y1).flat_map(|y| src[(y * w + r.x0) * 3..(y * w + r.x1) * 3].iter().copied()).collect() };
        let mask: Vec<bool> = (r.y0..r.y1).flat_map(|y| blend[y * w + r.x0..y * w + r.x1].iter().copied()).collect();
        let healed = poisson::seamless_clone(cw, ch, 3, &crop(&filled), &crop(&img), &mask);
        let mut out = pixels.to_vec();
        for y in 0..ch {
            for x in 0..cw {
                if mask[y * cw + x] {
                    let j = (y * cw + x) * 3;
                    out[(r.y0 + y) * w + r.x0 + x] = [0, 1, 2].map(|c| healed[j + c].max(0.0).powf(GAMMA) / k);
                }
            }
        }
        (out, blend.into_iter().map(|covered| if covered { 1. } else { 0. }).collect())
    }

    /// Mean colour of `img` in a disc/annulus.
    fn ring_stats(img: &RgbFrame, c: (f32, f32), r0: f32, r1: f32) -> Option<Vec<[f32; 3]>> {
        let mut out = Vec::new();
        let n = 24;
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            for rr in [r0, (r0 + r1) / 2.0, r1] {
                let (x, y) = (c.0 + a.cos() * rr, c.1 + a.sin() * rr);
                if x < 0.0 || y < 0.0 || x >= img.width as f32 || y >= img.height as f32 {
                    return None;
                }
                out.push(img.sample_bilinear(x, y));
            }
        }
        Some(out)
    }

    fn score(img: &RgbFrame, target: (f32, f32), source: (f32, f32), r: f32) -> f32 {
        let (Some(a), Some(b)) = (ring_stats(img, target, r * 1.05, r * 1.6), ring_stats(img, source, r * 1.05, r * 1.6)) else { return f32::MAX };
        // also compare the inside of the source against the target's surroundings (texture continuity)
        let inside = ring_stats(img, source, r * 0.2, r * 0.9);
        let mut s = 0.0;
        for (p, q) in a.iter().zip(&b) {
            for c in 0..3 {
                let (u, v) = (p[c].max(0.0).sqrt(), q[c].max(0.0).sqrt());
                s += (u - v) * (u - v);
            }
        }
        if let Some(ins) = inside {
            let mean_a: f32 = a.iter().map(|p| p[1].max(0.0).sqrt()).sum::<f32>() / a.len() as f32;
            let var: f32 = ins.iter().map(|p| (p[1].max(0.0).sqrt() - mean_a).powi(2)).sum::<f32>() / ins.len() as f32;
            s += var * a.len() as f32 * 0.5;
        }
        s
    }

    /// Candidate source offsets (output pixels) for a spot of radius `r` at `target`, best first
    /// (candidates reaching outside the image are left out).
    pub(super) fn ranked_sources(img: &RgbFrame, target: (f32, f32), r: f32) -> Vec<(f32, f32)> {
        let mut c: Vec<(f32, (f32, f32))> = Vec::new();
        for ring in [2.4f32, 3.2, 4.2] {
            for i in 0..16 {
                let a = i as f32 / 16.0 * std::f32::consts::TAU;
                let d = (a.cos() * r * ring, a.sin() * r * ring);
                let s = score(img, target, (target.0 + d.0, target.1 + d.1), r);
                if s < f32::MAX {
                    c.push((s, d));
                }
            }
        }
        c.sort_by(|a, b| a.0.total_cmp(&b.0));
        c.into_iter().map(|(_, d)| d).collect()
    }

    pub(super) fn auto_source(img: &RgbFrame, target: (f32, f32), r: f32) -> (f32, f32) {
        ranked_sources(img, target, r).first().copied().unwrap_or((r * 2.5, 0.0))
    }

    pub(super) fn apply(img: &mut RgbFrame, spots: &[Spot], frame: &Frame, ppl: f64) {
        if spots.is_empty() {
            return;
        }
        let (w, h) = (img.width, img.height);
        let to_out = frame.norm_to_out(w, h);
        // (AI removals are composited from their stored patches: `crate::patches`)
        for spot in spots.iter().filter(|s| !s.is_ai()) {
            let r = radius_px(spot.size, ppl) as f32;
            let pts: Vec<(f32, f32)> = spot
                .points
                .iter()
                .map(|p| {
                    let q = to_out.apply(*p);
                    (q.x as f32, q.y as f32)
                })
                .collect();
            let Some(&first) = pts.first() else { continue };
            let offset = match spot.source_offset {
                Some(o) => {
                    let a = to_out.apply(Point::new(0.0, 0.0));
                    let b = to_out.apply(o);
                    ((b.x - a.x) as f32, (b.y - a.y) as f32)
                }
                None => auto_source(img, first, r),
            };
            // Bounding box of the stroke.
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in &pts {
                x0 = x0.min(p.0 - r);
                y0 = y0.min(p.1 - r);
                x1 = x1.max(p.0 + r);
                y1 = y1.max(p.1 + r);
            }
            let pad = r;
            let (bx0, by0) = (((x0 - pad).floor().max(0.0)) as usize, ((y0 - pad).floor().max(0.0)) as usize);
            let (bx1, by1) = (((x1 + pad).ceil() as usize).min(w), ((y1 + pad).ceil() as usize).min(h));
            if bx1 <= bx0 || by1 <= by0 {
                continue;
            }
            let (bw, bh) = (bx1 - bx0, by1 - by0);
            let feather = (spot.feather / 100.0).clamp(0.0, 1.0) as f32;
            let opacity = (spot.opacity / 100.0).clamp(0.0, 1.0) as f32;
            // alpha of the stroke and source patch in the box
            let mut alpha = vec![0.0f32; bw * bh];
            let mut src = vec![[0.0f32; 3]; bw * bh];
            let mut tgt = vec![[0.0f32; 3]; bw * bh];
            for y in 0..bh {
                for x in 0..bw {
                    let (px, py) = ((bx0 + x) as f32 + 0.5, (by0 + y) as f32 + 0.5);
                    let d = pts.iter().map(|p| ((px - p.0).powi(2) + (py - p.1).powi(2)).sqrt()).fold(f32::MAX, f32::min);
                    let inner = r * (1.0 - feather * 0.8);
                    alpha[y * bw + x] = (1.0 - smooth(inner, r, d)) * opacity;
                    src[y * bw + x] = img.sample_bilinear(px + offset.0, py + offset.1);
                    tgt[y * bw + x] = img.get(bx0 + x, by0 + y);
                }
            }
            let heal = spot.mode != SpotMode::Clone;
            let low = if heal {
                // Tone correction from the spot's *surroundings* only: normalized convolution of
                // (target − source) over pixels outside the stroke (a membrane-like interpolation).
                let outside: Vec<f32> = alpha.iter().map(|a| if *a < 0.02 { 1.0 } else { 0.0 }).collect();
                let diff = RgbFrame {
                    width: bw,
                    height: bh,
                    data: tgt.iter().zip(&src).zip(&outside).map(|((t, s), m)| [(t[0] - s[0]) * m, (t[1] - s[1]) * m, (t[2] - s[2]) * m]).collect(),
                };
                let wts = li_raster::GrayFrame { width: bw, height: bh, data: outside };
                let sigma = (r * 0.6).max(1.0);
                let (bd, bm) = (diff.gaussian(sigma), wts.gaussian(sigma));
                Some(RgbFrame {
                    width: bw,
                    height: bh,
                    data: bd
                        .data
                        .iter()
                        .zip(&bm.data)
                        .map(|(d, m)| {
                            let m = m.max(1e-4);
                            [d[0] / m, d[1] / m, d[2] / m]
                        })
                        .collect(),
                })
            } else {
                None
            };
            for y in 0..bh {
                for x in 0..bw {
                    let i = y * bw + x;
                    let a = alpha[i];
                    if a <= 0.0 {
                        continue;
                    }
                    let mut v = src[i];
                    if let Some(l) = &low {
                        let d = l.data[i];
                        v = [v[0] + d[0], v[1] + d[1], v[2] + d[2]];
                    }
                    let t = tgt[i];
                    img.set(bx0 + x, by0 + y, [t[0] + (v[0] - t[0]) * a, t[1] + (v[1] - t[1]) * a, t[2] + (v[2] - t[2]) * a].map(|c| c.max(0.0)));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_develop::DevelopSettings;

    #[test]
    fn pixel_bounds_threshold_empty_and_image_edges() {
        assert_eq!(bbox(&[], 0, 0), None);
        assert_eq!(bbox(&[0.0; 12], 4, 3), None);
        assert_eq!(bbox(&[0.003, 0.004, 0.005, 0.0], 4, 1), Some(PxBox { x0: 2, y0: 0, x1: 3, y1: 1 }));

        let mut cov = [0.0; 12];
        cov[0] = 1.0;
        assert_eq!(bbox(&cov, 4, 3), Some(PxBox { x0: 0, y0: 0, x1: 1, y1: 1 }));
        cov[0] = 0.0;
        cov[11] = 1.0;
        assert_eq!(bbox(&cov, 4, 3), Some(PxBox { x0: 3, y0: 2, x1: 4, y1: 3 }));
        cov[0] = 1.0;
        let area = bbox(&cov, 4, 3).unwrap();
        assert_eq!(area, PxBox { x0: 0, y0: 0, x1: 4, y1: 3 });
        assert_eq!((area.width(), area.height()), (4, 3));

        let inner = PxBox { x0: 2, y0: 3, x1: 5, y1: 6 };
        assert_eq!(inflate(inner, 1, 8, 10), PxBox { x0: 1, y0: 2, x1: 6, y1: 7 });
        assert_eq!(inflate(inner, 5, 8, 10), PxBox { x0: 0, y0: 0, x1: 8, y1: 10 });
        assert_eq!(inflate(area, 2, 4, 3), PxBox { x0: 0, y0: 0, x1: 4, y1: 3 });
    }

    #[test]
    fn remove_erases_a_dot() {
        // flat grey with a gentle gradient and a black dot in the middle
        let mut img = RgbFrame::from_fn(120, 80, |x, _| [0.2 + x as f32 * 0.001; 3]);
        for y in 36..44 {
            for x in 56..64 {
                img.set(x, y, [0.0; 3]);
            }
        }
        let frame = Frame::new(120, 80, &DevelopSettings::default(), true);
        let spot = Spot { points: vec![Point::new(0.5, 0.5)], size: 8.0 / 120.0, feather: 30.0, ..Default::default() };
        apply(&mut img, &[spot], &frame, frame.px_per_long(120));
        let c = img.get(60, 40);
        assert!((c[0] - 0.26).abs() < 0.03, "{c:?}");
    }

    #[test]
    fn clone_copies_source() {
        let mut img = RgbFrame::from_fn(100, 100, |x, _| if x < 50 { [0.1; 3] } else { [0.9; 3] });
        let frame = Frame::new(100, 100, &DevelopSettings::default(), true);
        let spot = Spot {
            mode: SpotMode::Clone,
            points: vec![Point::new(0.25, 0.5)],
            size: 0.05,
            feather: 0.0,
            source_offset: Some(Point::new(0.5, 0.0)),
            ..Default::default()
        };
        apply(&mut img, &[spot], &frame, frame.px_per_long(100));
        assert!(img.get(25, 50)[0] > 0.8);
        assert!(img.get(5, 50)[0] < 0.2);
    }

    /// Deterministic textured test image: gradients, a hash noise grain and a few dark blemishes.
    fn synthetic(w: usize, h: usize) -> RgbFrame {
        RgbFrame::from_fn(w, h, |x, y| {
            let n = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
            let n = (n ^ (n >> 15)).wrapping_mul(0x2C1B_3C6D);
            let grain = ((n >> 8) & 0xFFFF) as f32 / 65535.0 - 0.5;
            let base = 0.25 + 0.3 * (x as f32 / w as f32) + 0.15 * ((y as f32 * 0.01).sin());
            [base + grain * 0.04, base * 0.9 + grain * 0.03, base * 0.8 + grain * 0.05]
        })
    }

    fn hash(img: &RgbFrame) -> u64 {
        img.data.iter().flatten().fold(0xcbf2_9ce4_8422_2325u64, |h, v| (h ^ v.to_bits() as u64).wrapping_mul(0x100_0000_01b3))
    }

    fn spot(w: usize, cx: f64, cy: f64, r: f64, mode: SpotMode, src: Option<Point>) -> Spot {
        Spot { mode, points: vec![Point::new(cx, cy)], size: r / w as f64, feather: 40.0, source_offset: src, ..Default::default() }
    }

    /// Clone keeps a bit-exact gate against the reference, including automatic/manual
    /// sources, brushed strokes and image edges. Heal/Remove have patch parity and a
    /// measured quality gate below instead of requiring frequency-separation pixels.
    #[test]
    fn clone_matches_reference_bit_exactly() {
        let (w, h) = (640usize, 420usize);
        let frame = Frame::new(w, h, &DevelopSettings::default(), true);
        let ppl = frame.px_per_long(w);
        let mut stroke = spot(w, 0.3, 0.4, 18.0, SpotMode::Clone, None);
        stroke.points.extend([Point::new(0.34, 0.42), Point::new(0.38, 0.40)]);
        let spots = vec![
            spot(w, 0.5, 0.5, 20.0, SpotMode::Clone, None),
            spot(w, 0.1, 0.1, 25.0, SpotMode::Clone, Some(Point::new(0.15, 0.1))),
            spot(w, 0.9, 0.9, 30.0, SpotMode::Clone, Some(Point::new(-0.1, -0.05))),
            stroke,
        ];
        let base = synthetic(w, h);
        let (mut a, mut b) = (base.clone(), base.clone());
        apply(&mut a, &spots, &frame, ppl);
        reference::apply(&mut b, &spots, &frame, ppl);
        assert_ne!(hash(&a), hash(&base), "spots changed nothing");
        assert_eq!(hash(&a), hash(&b));
        assert!(a.data == b.data);
        // source ranking is identical too
        let t = (320.0, 210.0);
        assert_eq!(ranked_sources(&base, t, 20.0), reference::ranked_sources(&base, t, 20.0));
    }

    #[test]
    fn patch_backend_is_bit_exact_with_previous_local_heal() {
        let (w, h) = (48, 40);
        let pixels = synthetic(w, h).data;
        let cov: Vec<_> = (0..w * h).map(|i| if (i % w).abs_diff(24) < 4 && (i / w).abs_diff(20) < 4 { 1.0 } else { 0.0 }).collect();
        let expected = reference::local_heal_reference(&pixels, w, h, &cov, 7);
        let actual = heal_patch(&pixels, w, h, &cov, 7, &lightcraft_sysmem::jobs::Interrupt::NONE).unwrap();
        assert_eq!(actual, expected, "preserves the established local PatchMatch+Poisson result exactly");
    }

    #[test]
    fn poisson_source_quality_beats_frequency_separation() {
        let (w, h) = (96, 80);
        let truth = RgbFrame::from_fn(w, h, |x, y| {
            let tone = 0.1 + 0.003 * x as f32 + 0.002 * y as f32;
            let texture = 0.015 * ((x as f32 * std::f32::consts::TAU / 16.0).sin());
            [tone + texture, tone * 0.8 + texture, tone * 0.6 + texture]
        });
        let mut damaged = truth.clone();
        for y in 32..48 {
            for x in 40..56 {
                damaged.set(x, y, [0.01; 3]);
            }
        }
        let frame = Frame::new(w, h, &DevelopSettings::default(), true);
        let sp = Spot {
            mode: SpotMode::Heal,
            points: vec![Point::new(0.5, 0.5)],
            size: 12.0 / w as f64,
            feather: 0.0,
            source_offset: Some(Point::new(32.0 / w as f64, 0.0)),
            ..Default::default()
        };
        let mut old = damaged.clone();
        let start = std::time::Instant::now();
        reference::apply(&mut old, std::slice::from_ref(&sp), &frame, frame.px_per_long(w));
        let old_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut new = damaged.clone();
        let start = std::time::Instant::now();
        apply(&mut new, std::slice::from_ref(&sp), &frame, frame.px_per_long(w));
        let new_ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("single small source-heal CPU sample: previous {old_ms:.3} ms, shared Poisson {new_ms:.3} ms");
        let rmse = |img: &RgbFrame| {
            let error: f32 = (32..48)
                .flat_map(|y| (40..56).map(move |x| (x, y)))
                .map(|(x, y)| (0..3).map(|c| (img.get(x, y)[c] - truth.get(x, y)[c]).powi(2)).sum::<f32>())
                .sum();
            (error / (16 * 16 * 3) as f32).sqrt()
        };
        let (before, after) = (rmse(&old), rmse(&new));
        eprintln!("source-heal linear RGB RMSE: frequency separation {before:.8}, Poisson {after:.8}");
        assert!(after < before, "measured quality must improve: {before} -> {after}");
        assert_eq!(new.get(2, 2), damaged.get(2, 2));
        let mut remove = sp.clone();
        remove.mode = SpotMode::Remove;
        let mut same = damaged.clone();
        apply(&mut same, &[remove], &frame, frame.px_per_long(w));
        assert_eq!(same.data, new.data, "legacy Remove and Heal share one result");
    }

    #[test]
    fn legacy_feather_opacity_and_zero_opacity_are_preserved() {
        let (w, h) = (48, 40);
        let base = synthetic(w, h);
        let frame = Frame::new(w, h, &DevelopSettings::default(), true);
        let sp = Spot {
            mode: SpotMode::Heal,
            points: vec![Point::new(0.5, 0.5), Point::new(0.55, 0.5)],
            size: 0.1,
            feather: 80.0,
            source_offset: Some(Point::new(-0.25, 0.0)),
            ..Default::default()
        };
        let render = |opacity| {
            let mut image = base.clone();
            let mut spot = sp.clone();
            spot.opacity = opacity;
            apply(&mut image, &[spot], &frame, frame.px_per_long(w));
            image
        };
        let full = render(100.0);
        let half = render(50.0);
        let zero = render(0.0);
        assert_eq!(zero.data, base.data);
        for i in 0..w * h {
            for c in 0..3 {
                assert!((half.data[i][c] - (base.data[i][c] + full.data[i][c]) * 0.5).abs() < 1e-6);
            }
        }
        assert_eq!(full.get(0, 0), base.get(0, 0));
        assert_ne!(full.data, base.data);
    }

    #[test]
    fn patch_backend_cancellation_empty_and_edge_masks() {
        use lightcraft_sysmem::jobs::Interrupt;
        let pixels = vec![[0.2; 3]; 24 * 24];
        let mut cov = vec![0.0; 24 * 24];
        assert_eq!(heal_patch(&pixels, 24, 24, &cov, 1, &Interrupt::NONE).unwrap(), (pixels.clone(), cov.clone()));
        cov[0] = 1.0;
        cov[1] = 0.5;
        let (out, alpha) = heal_patch(&pixels, 24, 24, &cov, 1, &Interrupt::NONE).unwrap();
        assert!(out.iter().flatten().all(|v| v.is_finite()));
        assert_eq!(out[23 * 24 + 23], pixels[23 * 24 + 23]);
        assert_eq!(alpha[23 * 24 + 23], 0.0);
        assert!(heal_patch(&pixels, 24, 24, &cov, 1, &Interrupt::cancel_only(&|| true)).is_err());
    }
    #[test]
    fn patch_feather_alpha_keeps_soft_coverage() {
        let pixels = synthetic(24, 24).data;
        let mut cov = vec![0.0; 24 * 24];
        for y in 10..14 {
            for x in 10..14 {
                cov[y * 24 + x] = 1.0;
            }
        }
        cov[10 * 24 + 9] = 0.25;
        cov[11 * 24 + 9] = 0.75;
        let (out, alpha) = heal_patch(&pixels, 24, 24, &cov, 1, &lightcraft_sysmem::jobs::Interrupt::NONE).unwrap();
        assert_eq!(alpha, cov, "feather is never promoted to a hard binary patch");
        assert_eq!(out[0], pixels[0]);
        assert!(out.iter().flatten().all(|v| v.is_finite()));
    }
}
