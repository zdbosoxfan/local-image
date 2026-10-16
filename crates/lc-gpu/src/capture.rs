//! Device capture sharpening, twin of `lightcraft_pipeline::capture` (unchanged reference).
//!
//! darktable `src/iop/demosaicing/capture.c` at 733bd69f32cac7ff5e41025115942772add1f088;
//! GPL-3.0-or-later, Copyright (C) 2025-2026 darktable developers; Ingo Weyrich's
//! RawTherapee algorithm. See `licenses/darktable-NOTICE.md`.
//!
//! The 256 tiny Gaussian kernels are calculated once on the host in the CPU's exact f32
//! order. Richardson–Lucy deconvolution and RGB gain run on the device. Metal uses
//! the CPU reference control planes: tiny mask-rounding differences can flip the
//! `blend > 0` participation test and accumulate visible errors over 50 iterations.
//! Other backends also prepare their indices and masks on the device. The
//! disc-truncated 5x5/9x9 PSFs are not separable. Small sigma uses sampled exponentials,
//! not the variance-adjusted taps used by Detail sharpening. Mask smoothing alone uses
//! the existing three-box kernels: whole-row horizontal sums and 32-row vertical bands
//! preserve the reference's running-sum order. WGSL sqrt replaces hypot for radial
//! distance; exp/log/division and driver multiply-add contraction may differ by ulps.

use std::sync::OnceLock;

use lightcraft_pipeline::capture::CaptureParams;

use crate::ctx::{Buf, groups2};
use crate::render::Cx;

/// Quarter kernels in sigma-index order, exactly as the CPU table generator.
fn kernels() -> &'static [[f32; 25]; 256] {
    static TABLE: OnceLock<[[f32; 25]; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|i| {
            let sigma = i as f32 * 0.01;
            let mut k = [0.0; 25];
            if sigma <= 0.0 {
                k[0] = 1.0;
                return k;
            }
            let range = if sigma < 0.66 { 2.5f32 * 2.5 } else { 4.5f32 * 4.5 };
            let temp = -2.0 * sigma * sigma;
            let mut full = [[0.0; 9]; 9];
            let mut sum = 0.0;
            for (yy, row) in full.iter_mut().enumerate() {
                for (xx, v) in row.iter_mut().enumerate() {
                    let (dy, dx) = (yy as f32 - 4.0, xx as f32 - 4.0);
                    let rad = dy * dy + dx * dx;
                    if rad <= range {
                        *v = (rad / temp).exp();
                        sum += *v;
                    }
                }
            }
            for dy in 0..5 {
                for dx in 0..5 {
                    k[5 * dy + dx] = full[dy + 4][dx + 4] / sum;
                }
            }
            k
        })
    })
}

fn params(w: usize, h: usize, p: &CaptureParams) -> [u32; 9] {
    [
        w as u32,
        h as u32,
        p.sigma.to_bits(),
        p.threshold.to_bits(),
        p.corner_boost.to_bits(),
        p.center.to_bits(),
        p.clip.is_some() as u32,
        p.clip.unwrap_or(0.0).to_bits(),
        0,
    ]
}

struct Prepared {
    lum: Buf,
    blend: Buf,
    indices: Buf,
}

#[cfg(test)]
fn prepare(cx: &mut Cx<'_>, src: &Buf, host: &lightcraft_raster::Rgb32f, p: &CaptureParams) -> Prepared {
    prepare_with_confidence(cx, src, host, p, None)
}

fn prepare_with_confidence(
    cx: &mut Cx<'_>,
    src: &Buf,
    host: &lightcraft_raster::Rgb32f,
    p: &CaptureParams,
    clip: Option<&lightcraft_pipeline::ClipConfidence>,
) -> Prepared {
    let (w, h) = (host.width, host.height);
    if cx.gpu.info.backend == wgpu::Backend::Metal {
        // The mask is both a continuous final blend and a discrete participation decision.
        // Even a positive residual of one ulp changes the ratio plane seen by neighbours.
        // Reuse the unchanged CPU preparation once per cached source, then do every expensive
        // deconvolution iteration on Metal. Exact indices also preserve corner-radius bins.
        let (blend, lum) = lightcraft_pipeline::capture::blend_mask_with_confidence(host, p, clip);
        let indices: Vec<u32> = lightcraft_pipeline::capture::sigma_index(w, h, p).into_iter().map(u32::from).collect();
        return Prepared { lum: cx.gpu.upload(&lum.data), blend: cx.gpu.upload(&blend.data), indices: cx.gpu.upload(&indices) };
    }
    let n = w * h;
    let clipped = clip.map(|_| {
        let pixels: Vec<f32> =
            (0..w * h).map(|i| if lightcraft_pipeline::capture::sensor_clipped(clip, i % w, i / w, w, h) { 1.0 } else { 0.0 }).collect();
        cx.gpu.upload(&pixels)
    });
    let mut k = params(w, h, p);
    k[8] = u32::from(clipped.is_some());
    let groups = groups2(w, h, [16, 16]);
    let lum = cx.gpu.buffer(n);
    let indices = cx.gpu.buffer(n);
    // Always bind the distinct writable indices buffer, even in entry points that
    // don't use it. Unused writable bindings must not alias the read-only dummy.
    cx.run("capture_lum", &k, &[Some(src), None, None, None, Some(&indices), None, Some(&lum)], groups);
    let modified = cx.gpu.buffer(n);
    cx.run("capture_mask", &k, &[Some(src), Some(&lum), clipped.as_ref(), None, Some(&indices), None, Some(&modified)], groups);
    let blurred = mask_blur(cx, &modified, w, h);
    let blend = cx.gpu.buffer(n);
    cx.run("capture_blend", &k, &[clipped.as_ref(), Some(&modified), Some(&blurred), None, Some(&indices), None, Some(&blend)], groups);
    drop((modified, blurred));
    cx.flush(); // release mask scratch before allocating RL scratch
    Prepared { lum, blend, indices }
}

/// Existing box kernels, with the CPU's sum restart locations instead of preview-sized chunks.
fn mask_blur(cx: &mut Cx<'_>, src: &Buf, w: usize, h: usize) -> Buf {
    let mut bufs = [cx.gpu.buffer(w * h), cx.gpu.buffer(w * h)];
    let radii = li_raster::blur::box_radii(2.0);
    for (i, r) in radii.into_iter().chain(radii).enumerate() {
        let (name, chunk, groups) = if i < 3 {
            ("box_h", w, groups2(1, h, [64, 4]))
        } else {
            let band = li_raster::blur::BAND;
            ("box_v", band, groups2(w, h.div_ceil(band), [64, 4]))
        };
        let input = if i == 0 { src } else { &bufs[(i + 1) % 2] };
        cx.run(name, &[w as u32, h as u32, 1, r as u32, chunk as u32], &[Some(input), Some(&bufs[i % 2])], groups);
    }
    bufs.swap(0, 1);
    let [out, _] = bufs;
    out
}

fn iteration(cx: &mut Cx<'_>, est: &Buf, next: &Buf, ratio: &Buf, prep: &Prepared, table: &Buf, w: usize, h: usize) {
    let k = [w as u32, h as u32];
    let groups = groups2(w, h, [16, 16]);
    cx.run("capture_div", &k, &[None, Some(est), Some(&prep.lum), Some(&prep.blend), Some(&prep.indices), Some(table), Some(ratio)], groups);
    cx.run("capture_mul", &k, &[None, Some(ratio), Some(est), Some(&prep.blend), Some(&prep.indices), Some(table), Some(next)], groups);
}

#[cfg(test)]
pub(crate) fn sharpen(cx: &mut Cx<'_>, src: &Buf, host: &lightcraft_raster::Rgb32f, p: &CaptureParams) -> Buf {
    sharpen_with_confidence(cx, src, host, p, None)
}

pub(crate) fn sharpen_with_confidence(
    cx: &mut Cx<'_>,
    src: &Buf,
    host: &lightcraft_raster::Rgb32f,
    p: &CaptureParams,
    clip: Option<&lightcraft_pipeline::ClipConfidence>,
) -> Buf {
    let (w, h) = (host.width, host.height);
    if p.sigma <= 0.2 || p.iterations == 0 || w < 9 || h < 9 {
        return cx.copy(src);
    }
    let prep = prepare_with_confidence(cx, src, host, p, clip);
    let table = cx.gpu.upload(kernels());
    let mut est = cx.copy(&prep.lum);
    let mut next = cx.gpu.buffer(w * h);
    let ratio = cx.gpu.buffer(w * h);
    for _ in 0..p.iterations {
        iteration(cx, &est, &next, &ratio, &prep, &table, w, h);
        std::mem::swap(&mut est, &mut next);
        // Bound each submission to one iteration even for very small previews. The
        // buffer pool cannot recycle any live input while these commands are queued.
        cx.flush();
    }
    let out = cx.gpu.buffer(w * h * 3);
    cx.run(
        "capture_apply",
        &params(w, h, p),
        &[Some(src), Some(&prep.lum), Some(&est), Some(&prep.blend), Some(&prep.indices), None, Some(&out)],
        groups2(w, h, [16, 16]),
    );
    out
}

#[cfg(test)]
mod tests;
