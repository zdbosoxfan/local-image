//! Remove / Heal / Clone spots, rendered at output resolution in scene-linear light.
//!
//! - **Clone** copies the source patch with a feathered edge.
//! - **Heal** copies the source's *detail* and keeps the target's *tone*: result = src + blur(target − src)
//!   (frequency separation — a fast, stable approximation of gradient-domain healing).
//! - **Remove** is Heal with an automatically chosen source: candidate offsets on rings around the
//!   spot are scored by how well the source's surrounding annulus matches the target's.

use lightcraft_develop::{Spot, SpotMode};
use lightcraft_geom::Point;
use lightcraft_raster::{Rgb32f, par_join, par_rows};

use crate::geometry::Frame;

#[inline]
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Samples of `img` on an annulus (24 angles x inner/mid/outer radius).
fn ring_stats(img: &Rgb32f, c: (f32, f32), r0: f32, r1: f32) -> Option<Vec<[f32; 3]>> {
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
fn score(img: &Rgb32f, a: &[[f32; 3]], source: (f32, f32), r: f32) -> f32 {
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
pub fn ranked_sources(img: &Rgb32f, target: (f32, f32), r: f32) -> Vec<(f32, f32)> {
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
pub fn auto_source(img: &Rgb32f, target: (f32, f32), r: f32) -> (f32, f32) {
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
    src: &Rgb32f,
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
    let r = (spot.size * frame.px_per_long(w)).max(1.0) as f32;
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
    /// Tone solve: normalized convolution of target - source around the stroke.
    pub solve: std::time::Duration,
    /// Blend back into the image.
    pub render: std::time::Duration,
}

/// Apply all spots to `img` (output resolution). `ppl` = output pixels per long-edge unit.
pub fn apply(img: &mut Rgb32f, spots: &[Spot], frame: &Frame, ppl: f64) {
    apply_timed(img, spots, frame, ppl, None);
}

/// [`apply`], adding each stage's time to `stages`.
pub fn apply_timed(img: &mut Rgb32f, spots: &[Spot], frame: &Frame, ppl: f64, mut stages: Option<&mut Stages>) {
    use std::time::Instant;
    if spots.is_empty() {
        return;
    }
    let (w, h) = (img.width, img.height);
    let to_out = frame.norm_to_out(w, h);
    // (AI removals are composited from their stored patches: `crate::patches`)
    for spot in spots.iter().filter(|s| !s.is_ai()) {
        let r = (spot.size * ppl).max(1.0) as f32;
        let pts: Vec<(f32, f32)> = spot
            .points
            .iter()
            .map(|p| {
                let q = to_out.apply(*p);
                (q.x as f32, q.y as f32)
            })
            .collect();
        let Some(&first) = pts.first() else { continue };
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
        let heal = spot.mode != SpotMode::Clone;
        let t2 = Instant::now();
        let low = if heal {
            // Tone correction from the spot's *surroundings* only: normalized convolution of
            // (target - source) over pixels outside the stroke (a membrane-like interpolation).
            let mut diff = Rgb32f { width: bw, height: bh, data: vec![[0.0; 3]; bw * bh] };
            let mut wts = lightcraft_raster::Plane { width: bw, height: bh, data: vec![0.0; bw * bh] };
            {
                let img = &*img;
                par_rows(&mut diff.data, bw, |y, row| {
                    let trow = &img.data[(by0 + y) * w + bx0..(by0 + y) * w + bx1];
                    for (x, d) in row.iter_mut().enumerate() {
                        let s = asrc[y * bw + x];
                        if s[0] < 0.02 {
                            let t = trow[x];
                            *d = [t[0] - s[1], t[1] - s[2], t[2] - s[3]];
                        }
                    }
                });
                par_rows(&mut wts.data, bw, |y, row| {
                    for (x, m) in row.iter_mut().enumerate() {
                        if asrc[y * bw + x][0] < 0.02 {
                            *m = 1.0;
                        }
                    }
                });
            }
            let sigma = (r * 0.6).max(1.0);
            let (mut bd, bm) = par_join(|| lightcraft_raster::blur::gaussian(&diff, sigma), || lightcraft_raster::blur::gaussian(&wts, sigma));
            par_rows(&mut bd.data, bw, |y, row| {
                for (x, d) in row.iter_mut().enumerate() {
                    let m = bm.data[y * bw + x].max(1e-4);
                    *d = [d[0] / m, d[1] / m, d[2] / m];
                }
            });
            Some(bd)
        } else {
            None
        };
        if let Some(s) = stages.as_deref_mut() {
            s.solve += t2.elapsed();
        }
        let t3 = Instant::now();
        par_rows(&mut img.data[by0 * w..by1 * w], w, |y, row| {
            for x in 0..bw {
                let i = y * bw + x;
                let a = asrc[i][0];
                if a <= 0.0 {
                    continue;
                }
                let mut v = [asrc[i][1], asrc[i][2], asrc[i][3]];
                if let Some(l) = &low {
                    let d = l.data[i];
                    v = [v[0] + d[0], v[1] + d[1], v[2] + d[2]];
                }
                let t = row[bx0 + x];
                row[bx0 + x] = [t[0] + (v[0] - t[0]) * a, t[1] + (v[1] - t[1]) * a, t[2] + (v[2] - t[2]) * a].map(|c| c.max(0.0));
            }
        });
        if let Some(s) = stages.as_deref_mut() {
            s.render += t3.elapsed();
        }
    }
}

#[cfg(test)]
/// The straightforward pre-optimisation implementation, kept to prove the fast one is bit-identical.
mod reference {
    use super::*;

    /// Mean colour of `img` in a disc/annulus.
    fn ring_stats(img: &Rgb32f, c: (f32, f32), r0: f32, r1: f32) -> Option<Vec<[f32; 3]>> {
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

    fn score(img: &Rgb32f, target: (f32, f32), source: (f32, f32), r: f32) -> f32 {
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
    pub(super) fn ranked_sources(img: &Rgb32f, target: (f32, f32), r: f32) -> Vec<(f32, f32)> {
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

    pub(super) fn auto_source(img: &Rgb32f, target: (f32, f32), r: f32) -> (f32, f32) {
        ranked_sources(img, target, r).first().copied().unwrap_or((r * 2.5, 0.0))
    }

    pub(super) fn apply(img: &mut Rgb32f, spots: &[Spot], frame: &Frame, ppl: f64) {
        if spots.is_empty() {
            return;
        }
        let (w, h) = (img.width, img.height);
        let to_out = frame.norm_to_out(w, h);
        // (AI removals are composited from their stored patches: `crate::patches`)
        for spot in spots.iter().filter(|s| !s.is_ai()) {
            let r = (spot.size * ppl).max(1.0) as f32;
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
                let diff = Rgb32f {
                    width: bw,
                    height: bh,
                    data: tgt.iter().zip(&src).zip(&outside).map(|((t, s), m)| [(t[0] - s[0]) * m, (t[1] - s[1]) * m, (t[2] - s[2]) * m]).collect(),
                };
                let wts = lightcraft_raster::Plane { width: bw, height: bh, data: outside };
                let sigma = (r * 0.6).max(1.0);
                let (bd, bm) = (lightcraft_raster::blur::gaussian(&diff, sigma), lightcraft_raster::blur::gaussian(&wts, sigma));
                Some(Rgb32f {
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
    fn remove_erases_a_dot() {
        // flat grey with a gentle gradient and a black dot in the middle
        let mut img = Rgb32f::from_fn(120, 80, |x, _| [0.2 + x as f32 * 0.001; 3]);
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
        let mut img = Rgb32f::from_fn(100, 100, |x, _| if x < 50 { [0.1; 3] } else { [0.9; 3] });
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
    fn synthetic(w: usize, h: usize) -> Rgb32f {
        Rgb32f::from_fn(w, h, |x, y| {
            let n = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
            let n = (n ^ (n >> 15)).wrapping_mul(0x2C1B_3C6D);
            let grain = ((n >> 8) & 0xFFFF) as f32 / 65535.0 - 0.5;
            let base = 0.25 + 0.3 * (x as f32 / w as f32) + 0.15 * ((y as f32 * 0.01).sin());
            [base + grain * 0.04, base * 0.9 + grain * 0.03, base * 0.8 + grain * 0.05]
        })
    }

    fn hash(img: &Rgb32f) -> u64 {
        img.data.iter().flatten().fold(0xcbf2_9ce4_8422_2325u64, |h, v| (h ^ v.to_bits() as u64).wrapping_mul(0x100_0000_01b3))
    }

    fn spot(w: usize, cx: f64, cy: f64, r: f64, mode: SpotMode, src: Option<Point>) -> Spot {
        Spot { mode, points: vec![Point::new(cx, cy)], size: r / w as f64, feather: 40.0, source_offset: src, ..Default::default() }
    }

    /// The optimised path must reproduce the reference bit for bit (heal and clone, auto and
    /// manual sources, multi-point strokes, spots at the image edge).
    #[test]
    fn matches_reference_bit_exactly() {
        let (w, h) = (640usize, 420usize);
        let frame = Frame::new(w, h, &DevelopSettings::default(), true);
        let ppl = frame.px_per_long(w);
        let mut stroke = spot(w, 0.3, 0.4, 18.0, SpotMode::Heal, None);
        stroke.points.push(Point::new(0.34, 0.42));
        stroke.points.push(Point::new(0.38, 0.40));
        let spots = vec![
            spot(w, 0.5, 0.5, 20.0, SpotMode::Heal, None),
            spot(w, 0.7, 0.3, 40.0, SpotMode::Remove, None),
            spot(w, 0.1, 0.1, 25.0, SpotMode::Heal, Some(Point::new(0.15, 0.1))),
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

    /// Release-mode benchmark on a 24 MP image:
    /// `cargo +1.98.1 test --release -p lightcraft-pipeline --offline spots_bench -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark: run explicitly in release mode"]
    fn spots_bench() {
        use std::time::{Duration, Instant};
        let (w, h) = (6000usize, 4000usize);
        let base = synthetic(w, h);
        let frame = Frame::new(w, h, &DevelopSettings::default(), true);
        let ppl = frame.px_per_long(w);
        let med = |mut v: Vec<Duration>| {
            v.sort();
            v[v.len() / 2].as_secs_f64() * 1000.0
        };
        println!("\n24 MP heal spots (brush = diameter), median of 3 after warm-up, ms");
        println!(
            "{:>6} | {:>8} {:>8} {:>8} {:>8} | {:>9} | {:>9} {:>9} | speedup",
            "brush", "search", "feather", "solve", "render", "total", "ref total", "ref srch"
        );
        for diameter in [30.0f64, 150.0, 600.0] {
            let spots = [spot(w, 0.5, 0.5, diameter / 2.0, SpotMode::Heal, None)];
            let mut img = base.clone();
            let mut runs = Vec::new();
            let mut refs = Vec::new();
            let mut ref_search = Vec::new();
            for i in 0..4 {
                let mut st = Stages::default();
                let t = Instant::now();
                apply_timed(&mut img, &spots, &frame, ppl, Some(&mut st));
                let total = t.elapsed();
                let mut img2 = base.clone();
                let t = Instant::now();
                reference::apply(&mut img2, &spots, &frame, ppl);
                let rtotal = t.elapsed();
                let t = Instant::now();
                reference::auto_source(&base, (3000.0, 2000.0), (diameter / 2.0) as f32);
                let rs = t.elapsed();
                if i > 0 {
                    runs.push((st, total));
                    refs.push(rtotal);
                    ref_search.push(rs);
                }
                img = base.clone();
            }
            let m = |f: fn(&Stages) -> Duration| med(runs.iter().map(|(s, _)| f(s)).collect());
            let total = med(runs.iter().map(|r| r.1).collect());
            let rt = med(refs);
            println!(
                "{:>6} | {:>8.1} {:>8.1} {:>8.1} {:>8.1} | {:>9.1} | {:>9.1} {:>9.1} | {:.1}x",
                diameter,
                m(|s| s.search),
                m(|s| s.feather),
                m(|s| s.solve),
                m(|s| s.render),
                total,
                rt,
                med(ref_search),
                rt / total
            );
        }
    }
}
