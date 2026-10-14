//! File › Automate › Merge to HDR Pro: exposure estimation, alignment, camera response recovery,
//! radiance merge, ghost removal and the 16/8-bit tone mapping methods.
//!
//! * **Alignment**: G. Ward, *Fast, Robust Image Registration for Compositing High Dynamic Range
//!   Photographs from Hand-Held Exposures*, JGT 2003 — median threshold bitmaps (invariant to
//!   exposure) with an exclusion band around the median, XOR-counted over an image pyramid.
//! * **Response curve**: P. Debevec, J. Malik, *Recovering High Dynamic Range Radiance Maps from
//!   Photographs*, SIGGRAPH 1997 — `g(Z) = ln E + ln Δt` solved by weighted linear least squares
//!   with a smoothness term and `g(128) = 0`, per channel, on stratified sample pixels.
//! * **Merge**: the hat-weighted average of `g(Z) − ln Δt` over the exposures.
//! * **Ghosts**: where the per-exposure radiance estimates disagree (high weighted variance of
//!   their logarithms) the result comes from one base exposure only, as Photoshop's *Remove
//!   ghosts* does; the mask is feathered.
//! * **Tone mapping** for 16/8-bit output: Local Adaptation (the editor HDR Toning stage),
//!   Exposure and Gamma, Highlight Compression (E. Reinhard et al., *Photographic Tone
//!   Reproduction for Digital Images*, SIGGRAPH 2002, global operator) and Equalize Histogram
//!   (histogram equalisation of log luminance, after G. Ward Larson et al., *A Visibility
//!   Matching Tone Reproduction Operator*, TVCG 1997, without the ceiling).
//!
//! Pixels are straight RGBA in `0..=1` (display-encoded inputs); radiance is linear.

#![allow(clippy::needless_range_loop)] // index loops mirror the maths (pixels × channels)

use super::support::{Normal, par_rows, srgb_to_linear};

fn luma(c: [f32; 4]) -> f32 {
    li_color::luminance_709([c[0], c[1], c[2]])
}

/// Hat weight on a 0..=255 code value.
fn hat(z: f64) -> f64 {
    if z <= 127.5 { z } else { 255.0 - z }.max(0.0)
}

/// Relative exposures (linear factors, the darkest = 1) estimated from the images themselves:
/// consecutive images (sorted by brightness) are compared on pixels well exposed in both.
pub fn estimate_exposures(imgs: &[&[[f32; 4]]]) -> Vec<f64> {
    let n = imgs.len();
    let mean: Vec<f64> = imgs.iter().map(|im| im.iter().map(|p| luma(*p) as f64).sum::<f64>() / im.len().max(1) as f64).collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| mean[a].total_cmp(&mean[b]));
    let mut ex = vec![1.0f64; n];
    for k in 1..n {
        let (a, b) = (order[k - 1], order[k]);
        let mut ratios: Vec<f64> = imgs[a]
            .iter()
            .zip(imgs[b].iter())
            .step_by(7)
            .filter_map(|(pa, pb)| {
                let (la, lb) = (luma(*pa), luma(*pb));
                (pa[3] > 0.0 && pb[3] > 0.0 && (0.08..0.92).contains(&la) && (0.08..0.92).contains(&lb))
                    .then(|| srgb_to_linear(lb) as f64 / srgb_to_linear(la).max(1e-6) as f64)
            })
            .collect();
        ratios.sort_by(f64::total_cmp);
        let r = if ratios.is_empty() { 2.0 } else { ratios[ratios.len() / 2].max(1.0) };
        ex[b] = ex[a] * r;
    }
    ex
}

// ---------- alignment ----------

fn shrink(w: usize, h: usize, g: &[f32]) -> (usize, usize, Vec<f32>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0.0f32; nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            let p = |xx: usize, yy: usize| g[yy.min(h - 1) * w + xx.min(w - 1)];
            out[y * nw + x] = 0.25 * (p(2 * x, 2 * y) + p(2 * x + 1, 2 * y) + p(2 * x, 2 * y + 1) + p(2 * x + 1, 2 * y + 1));
        }
    }
    (nw, nh, out)
}

fn bitmaps(g: &[f32]) -> (Vec<bool>, Vec<bool>) {
    let mut s: Vec<f32> = g.to_vec();
    s.sort_by(f32::total_cmp);
    let med = s[s.len() / 2];
    (g.iter().map(|v| *v > med).collect(), g.iter().map(|v| (v - med).abs() > 4.0 / 255.0).collect())
}

/// Translation `(dx, dy)` that aligns grey image `img` to `reference` (Ward's MTB), searching
/// up to `2^levels` px.
pub fn mtb_offset(w: usize, h: usize, reference: &[f32], img: &[f32], levels: usize) -> (i32, i32) {
    if levels == 0 || w < 16 || h < 16 || w.checked_mul(h) != Some(reference.len()) || reference.len() != img.len() {
        return (0, 0);
    }
    let (cur_x, cur_y) = {
        let (sw, sh, sr) = shrink(w, h, reference);
        let (_, _, si) = shrink(w, h, img);
        let (x, y) = mtb_offset(sw, sh, &sr, &si, levels - 1);
        (x * 2, y * 2)
    };
    let (tr, er) = bitmaps(reference);
    let (ti, ei) = bitmaps(img);
    // Candidates are tried centre first, then by distance from the current estimate, and only a
    // strictly smaller error replaces the best: ties (flat or identical images, where the
    // exclusion band masks every pixel and all errors are 0) keep the inherited offset instead
    // of drifting to the first corner searched.
    const NEIGHBOURS: [(i32, i32); 9] = [(0, 0), (0, -1), (-1, 0), (1, 0), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)];
    // (error, compared pixels, ox, oy): errors are compared as rates so a smaller overlap at a
    // larger shift doesn't win merely by having fewer pixels to disagree on.
    let mut best: Option<(u64, u64, i32, i32)> = None;
    for (dx, dy) in NEIGHBOURS {
        let (ox, oy) = (cur_x + dx, cur_y + dy);
        let (mut err, mut n) = (0u64, 0u64);
        for y in 0..h as i32 {
            let sy = y - oy;
            if sy < 0 || sy >= h as i32 {
                continue;
            }
            for x in 0..w as i32 {
                let sx = x - ox;
                if sx < 0 || sx >= w as i32 {
                    continue;
                }
                let (a, b) = ((y as usize) * w + x as usize, (sy as usize) * w + sx as usize);
                n += 1;
                if (tr[a] != ti[b]) && er[a] && ei[b] {
                    err += 1;
                }
            }
        }
        // err/n < best_err/best_n, in integers.
        if best.is_none_or(|(be, bn, ..)| (err as u128) * (bn as u128) < (be as u128) * (n as u128)) {
            best = Some((err, n, ox, oy));
        }
    }
    best.map_or((0, 0), |(.., ox, oy)| (ox, oy))
}

// ---------- response curve ----------

/// Sample pixel indices for response recovery: a spatial grid filtered to pixels whose
/// neighbourhood is flat in the middle exposure (robust to small misalignment), stratified by
/// its intensity.
pub fn sample_pixels(w: usize, h: usize, middle: &[[f32; 4]], count: usize) -> Vec<usize> {
    if count == 0 || w.checked_mul(h) != Some(middle.len()) {
        return Vec::new();
    }
    let mut by_bin: Vec<Vec<usize>> = vec![Vec::new(); 32];
    let step = ((w * h / (count * 40).max(1)) as f64).sqrt().max(1.0) as usize;
    for y in (2..h.saturating_sub(2)).step_by(step) {
        for x in (2..w.saturating_sub(2)).step_by(step) {
            let i = y * w + x;
            let l = luma(middle[i]);
            let flat =
                middle[i][3] > 0.0 && [i - 2, i + 2, i - 2 * w, i + 2 * w].iter().all(|&j| middle[j][3] > 0.0 && (luma(middle[j]) - l).abs() < 0.04);
            if flat {
                by_bin[((l * 31.99) as usize).min(31)].push(i);
            }
        }
    }
    // Round-robin over the bins so every intensity is represented.
    let mut out = Vec::new();
    let mut k = 0;
    while out.len() < count && by_bin.iter().any(|b| k < b.len()) {
        for b in &by_bin {
            if let Some(&i) = b.get(k * 7 % b.len().max(1)).filter(|_| k < b.len()) {
                out.push(i);
                if out.len() >= count {
                    break;
                }
            }
        }
        k += 1;
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Debevec–Malik response curve: `z[j][i]` is the code value (0..=255) of sample `i` in exposure
/// `j`; returns `g(0..=255)` (log exposure per code value, `g(128) = 0`), smoothed by `lambda`.
pub fn response_curve(z: &[Vec<u8>], log_dt: &[f64], lambda: f64) -> Vec<f64> {
    let p = z.len();
    let n = z.first().map_or(0, Vec::len);
    let mut ne = Normal::new(256 + n);
    for j in 0..p {
        for i in 0..n {
            let zz = z[j][i] as usize;
            let w = hat(zz as f64) + 1.0;
            ne.add(&[(zz, 1.0), (256 + i, -1.0)], log_dt[j], w);
        }
    }
    ne.add(&[(128, 1.0)], 0.0, 100.0);
    for k in 1..255 {
        let w = lambda.sqrt() * (hat(k as f64) + 1.0);
        ne.add(&[(k - 1, 1.0), (k, -2.0), (k + 1, 1.0)], 0.0, w);
    }
    let Some(x) = ne.solve() else {
        // Fall back to the sRGB curve.
        return (0..256).map(|k| (srgb_to_linear(k as f32 / 255.0).max(1e-5) as f64).ln() - (srgb_to_linear(128.0 / 255.0) as f64).ln()).collect();
    };
    let mut g: Vec<f64> = x[..256].to_vec();
    // Enforce monotonicity (noise at the extremes can fold the curve).
    for k in (0..128).rev() {
        g[k] = g[k].min(g[k + 1] - 1e-4);
    }
    for k in 129..256 {
        g[k] = g[k].max(g[k - 1] + 1e-4);
    }
    g
}

fn g_at(g: &[f64], v: f32) -> f64 {
    let x = (v.clamp(0.0, 1.0) * 255.0) as f64;
    let i = (x.floor() as usize).min(254);
    let t = x - i as f64;
    g[i] * (1.0 - t) + g[i + 1] * t
}

/// Merge options.
#[derive(Clone, Debug)]
pub struct MergeOptions {
    /// Linear exposure factors (time × gain), one per image.
    pub exposures: Vec<f64>,
    pub remove_ghosts: bool,
    /// Exposure used where ghosts are removed (`None`: the one with most well-exposed pixels).
    pub ghost_base: Option<usize>,
    /// Response curves per channel (`None`: recovered from the images).
    pub response: Option<[Vec<f64>; 3]>,
}

/// Result of [`merge`].
#[derive(Clone, Debug)]
pub struct Merged {
    /// Linear radiance, straight RGBA.
    pub px: Vec<[f32; 4]>,
    pub response: [Vec<f64>; 3],
    pub ghost_base: usize,
    /// Fraction of pixels handled as ghosts.
    pub ghost_fraction: f64,
    /// Feathered deghost overlay, one value per output pixel.
    pub ghost_mask: Vec<f32>,
    /// Dynamic range of the result in stops (1st..99.9th percentile of luminance).
    pub stops: f64,
}

/// Merges aligned exposures (`w × h` each) into a radiance map, scaled so the base exposure's
/// well-exposed pixels keep their linear values.
///
/// # Errors
/// No images, an `exposures` list that doesn't match the images, or images whose length isn't
/// `w * h`.
pub fn merge(w: usize, h: usize, imgs: &[&[[f32; 4]]], opts: &MergeOptions) -> Result<Merged, String> {
    merge_with_progress(w, h, imgs, opts, &crate::no_progress)
}

/// Rendered-source merge with stage progress and cancellation.
pub fn merge_with_progress(w: usize, h: usize, imgs: &[&[[f32; 4]]], opts: &MergeOptions, progress: &crate::Progress) -> Result<Merged, String> {
    let stage = |fraction, label| if progress(fraction, label) { Ok(()) } else { Err("cancelled".to_owned()) };
    stage(0.0, "Recovering camera response")?;
    let p = imgs.len();
    if p == 0 {
        return Err("HDR merge needs at least one image".into());
    }
    if opts.exposures.len() != p {
        return Err(format!("HDR merge needs one exposure per image ({} exposures for {p} images)", opts.exposures.len()));
    }
    if w == 0 || h == 0 || w.checked_mul(h).is_none() || imgs.iter().any(|im| Some(im.len()) != w.checked_mul(h)) {
        return Err(format!("HDR merge images must all be {w}x{h} pixels"));
    }
    if opts.exposures.iter().any(|e| !e.is_finite() || *e <= 0.0) {
        return Err("HDR exposures must be positive and finite".into());
    }
    if opts.response.as_ref().is_some_and(|r| r.iter().any(|c| c.len() != 256 || c.iter().any(|v| !v.is_finite()))) {
        return Err("HDR response needs 256 finite values per channel".into());
    }
    if imgs.iter().flat_map(|im| im.iter()).flatten().any(|v| !v.is_finite()) {
        return Err("HDR pixels must be finite".into());
    }
    let log_dt: Vec<f64> = opts.exposures.iter().map(|e| e.max(1e-12).ln()).collect();
    let well = |im: &[[f32; 4]]| im.iter().step_by(3).filter(|q| q[3] > 0.0 && (0.1..0.9).contains(&luma(**q))).count();
    let base = opts.ghost_base.filter(|b| *b < p).unwrap_or_else(|| (0..p).max_by_key(|&j| well(imgs[j])).unwrap_or(0));
    let response = opts.response.clone().unwrap_or_else(|| {
        let mut order: Vec<usize> = (0..p).collect();
        order.sort_by(|&a, &b| opts.exposures[a].total_cmp(&opts.exposures[b]));
        let mid = order[p / 2];
        let samples: Vec<_> = sample_pixels(w, h, imgs[mid], 160).into_iter().filter(|&i| imgs.iter().all(|im| im[i][3] > 0.0)).collect();
        std::array::from_fn(|c| {
            let z: Vec<Vec<u8>> = imgs.iter().map(|im| samples.iter().map(|&i| (im[i][c].clamp(0.0, 1.0) * 255.0).round() as u8).collect()).collect();
            response_curve(&z, &log_dt, 40.0)
        })
    });
    stage(0.25, "Merging radiance")?;
    let mut px = vec![[0.0f32; 4]; w * h];
    let mut ghost = vec![0.0f32; w * h];
    let resp = &response;
    par_rows(&mut px, w, 1, |y, row| {
        for (x, out) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let mut alpha = 0.0f32;
            if imgs.iter().all(|im| im[i][3] <= 0.0) {
                continue;
            }
            for c in 0..3 {
                let (mut num, mut den) = (0.0f64, 0.0f64);
                for j in 0..p {
                    let v = imgs[j][i][c];
                    let wt = hat(v.clamp(0.0, 1.0) as f64 * 255.0) * imgs[j][i][3].clamp(0.0, 1.0) as f64;
                    num += wt * (g_at(&resp[c], v) - log_dt[j]);
                    den += wt;
                }
                let le = if den > 1e-6 {
                    num / den
                } else {
                    // Saturated everywhere: darkest exposure if bright, brightest if dark.
                    let covered = imgs.iter().filter(|im| im[i][3] > 0.0);
                    let count = covered.clone().count().max(1);
                    let bright = covered.map(|im| im[i][c]).sum::<f32>() / count as f32 > 0.5;
                    let j = (0..p)
                        .filter(|&j| imgs[j][i][3] > 0.0)
                        .min_by(|&a, &b| if bright { log_dt[a].total_cmp(&log_dt[b]) } else { log_dt[b].total_cmp(&log_dt[a]) })
                        .unwrap_or(0);
                    g_at(&resp[c], imgs[j][i][c]) - log_dt[j]
                };
                out[c] = le.exp() as f32;
            }
            for im in imgs {
                alpha = alpha.max(im[i][3].clamp(0.0, 1.0));
            }
            out[3] = alpha;
        }
    });
    stage(0.60, "Removing ghosts")?;
    let mut ghost_fraction = 0.0;
    if opts.remove_ghosts && p > 1 {
        // Weighted variance of per-exposure log radiance (luminance).
        let score: Vec<f32> = super::support::par_map(w * h, |i| {
            let (mut s, mut s2, mut sw) = (0.0f64, 0.0f64, 0.0f64);
            for j in 0..p {
                let q = imgs[j][i];
                let l = luma(q);
                let wt = hat(l as f64 * 255.0) * q[3].clamp(0.0, 1.0) as f64;
                if wt < 20.0 {
                    continue;
                }
                let lr = (0..3).map(|c| 0.33 * (g_at(&response[c], q[c]) - log_dt[j])).sum::<f64>();
                s += wt * lr;
                s2 += wt * lr * lr;
                sw += wt;
            }
            if sw <= 0.0 {
                return 0.0;
            }
            let m = s / sw;
            ((s2 / sw - m * m).max(0.0)).sqrt() as f32
        });
        for i in 0..w * h {
            ghost[i] = if score[i] > 0.35 { 1.0 } else { 0.0 };
        }
        // Grow and feather the mask.
        let r = (w.max(h) as f32 / 200.0).max(2.0);
        super::support::gauss_blur_n(&mut ghost, w, h, 1, r);
        for g in ghost.iter_mut() {
            *g = (*g * 3.0).min(1.0);
        }
        ghost_fraction = ghost.iter().filter(|g| **g > 0.5).count() as f64 / (w * h).max(1) as f64;
        for i in 0..w * h {
            if ghost[i] <= 0.0 {
                continue;
            }
            let source = if imgs[base][i][3] > 0.0 {
                base
            } else {
                (0..p)
                    .filter(|&j| imgs[j][i][3] > 0.0)
                    .max_by(|&a, &b| hat(luma(imgs[a][i]) as f64 * 255.0).total_cmp(&hat(luma(imgs[b][i]) as f64 * 255.0)))
                    .unwrap_or(base)
            };
            let q = imgs[source][i];
            if q[3] <= 0.0 {
                continue;
            }
            let ld = log_dt[source];
            for c in 0..3 {
                let e = (g_at(&response[c], q[c]) - ld).exp() as f32;
                px[i][c] = px[i][c] * (1.0 - ghost[i]) + e * ghost[i];
            }
        }
    }
    stage(0.85, "Normalising radiance")?;
    // Scale per channel so the base exposure's well-exposed pixels map to their linear values.
    for c in 0..3 {
        let mut r: Vec<f64> = (0..w * h)
            .step_by(37)
            .filter_map(|i| {
                let v = imgs[base][i][c];
                (imgs[base][i][3] > 0.0 && (0.15..0.85).contains(&v) && px[i][c] > 0.0)
                    .then(|| srgb_to_linear(v) as f64 / (px[i][c] as f64 * opts.exposures[base]))
            })
            .collect();
        r.sort_by(f64::total_cmp);
        let k = if r.is_empty() { 1.0 } else { r[r.len() / 2] } * opts.exposures[base];
        for q in px.iter_mut() {
            q[c] = (q[c] as f64 * k) as f32;
        }
    }
    let mut l: Vec<f32> = px.iter().step_by(13).filter(|q| q[3] > 0.0).map(|q| luma(*q).max(1e-9)).collect();
    l.sort_by(f32::total_cmp);
    let stops = if l.is_empty() { 0.0 } else { (l[(l.len() as f64 * 0.999) as usize % l.len()] as f64 / l[l.len() / 100] as f64).log2() };
    stage(1.0, "HDR merge complete")?;
    Ok(Merged { px, response, ghost_base: base, ghost_fraction, ghost_mask: ghost, stops })
}
