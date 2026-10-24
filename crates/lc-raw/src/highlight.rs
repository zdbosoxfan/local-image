//! Clip-aware highlight handling in the sensor domain: camera values before white balance, sensor
//! white = 1.0, before `OpcodeList3`. [`Highlights`] reads the clipping from the mosaic before
//! demosaic ([`Highlights::mosaic`], [`Highlights::demosaiced`]) or, for data without a mosaic and
//! for binned previews (whose blocks keep a clipped channel's maximum, no interpolation), from
//! camera RGB ([`Highlights::rgb`]).
//!
//! - [`segmentation`]: full segmentation/recovery on normalized CFA before demosaic.
//! - [`clip_neutral`] / [`clip_cfa`]: white-balance-aware clipping so that sensor-clipped areas render neutral
//!   instead of magenta/cyan (each channel limited to the smallest white-balanced clip level).
//! - [`reconstruct`] / [`reconstruct_masked`]: where only some channels are clipped, rebuild them from the unclipped
//!   channels using the local chromaticity of nearby unclipped pixels (diffused into the clipped region with a
//!   coarse-to-fine normalised-convolution fill). Fully clipped pixels become neutral at the brightest plausible level.
//! - [`opposed`] / [`opposed_cfa`]: darktable's "inpaint opposed" (see [`mod@opposed`]): a clipped channel becomes the
//!   cube-root mean of the two other channels plus a chrominance offset measured around clipped areas.

mod cfa;
mod opposed;
mod segbased;
mod segmentation;
pub(crate) use cfa::superpixels;
pub use cfa::{ClipMask, clip_cfa};
pub use opposed::opposed;
pub use segbased::{Recovery, SegmentationOptions, opposed_cfa, segmentation};

use crate::Normalized;
use li_raster::RgbFrame;
use rayon::prelude::*;

/// How development treats clipped highlights.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Clip every channel at the lowest white-balanced clip level ([`clip_neutral`]).
    Clip,
    /// Rebuild clipped channels from local chromaticity ([`reconstruct`]).
    #[default]
    Reconstruct,
    /// darktable's "inpaint opposed" ([`opposed`]).
    Opposed,
    /// darktable's segmentation-based reconstruction ([`segmentation`]).
    Segmentation,
}

/// A highlight mode with the white balance it renders for and the sensor clip level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Highlights {
    pub mode: Mode,
    /// White-balance multipliers the image will get (camera → balanced).
    pub wb: [f32; 3],
    /// Sensor clip level, normalised (slightly below 1.0, e.g. 0.99).
    pub clip: f32,
}

/// What [`Highlights::mosaic`] leaves for [`Highlights::demosaiced`].
#[derive(Default)]
pub enum Pending {
    /// Done on the mosaic (or nothing was clipped).
    #[default]
    Nothing,
    /// The data has no mosaic: the RGB variant of the mode ([`Highlights::rgb`]).
    Rgb,
    /// Reconstruct the clipped channels the mosaic's clip mask names.
    Masked(ClipMask),
    /// Neutralise where the mosaic's clip mask has all three colours clipped.
    Neutralize(ClipMask),
}

impl Highlights {
    /// The part of the mode that runs on a normalised mosaic before demosaic; `scale` is the
    /// sensor sampling interval of a reduced mosaic (1 at full size).
    pub fn mosaic(&self, n: &mut Normalized, scale: usize) -> Pending {
        if n.cpp != 1 || n.cfa.is_none() {
            return Pending::Rgb;
        }
        match self.mode {
            Mode::Clip => {
                clip_cfa(n, self.wb, self.clip);
                Pending::Nothing
            }
            // the samples a demosaic spreads into each pixel: its 3×3 neighbourhood
            Mode::Reconstruct => ClipMask::of(n, self.clip, 1).map_or(Pending::Nothing, Pending::Masked),
            Mode::Opposed | Mode::Segmentation => {
                let mask = ClipMask::of(n, self.clip, 1);
                self.inpaint(n, scale);
                mask.map_or(Pending::Nothing, Pending::Neutralize)
            }
        }
    }

    /// The darktable modes' fill of the clipped samples of a mosaic (nothing for the others).
    pub(crate) fn inpaint(&self, n: &mut Normalized, scale: usize) {
        match self.mode {
            Mode::Opposed => drop(opposed_cfa(n, self.wb, self.clip)),
            Mode::Segmentation => drop(segmentation(n, self.wb, self.clip, &SegmentationOptions::default().at_scale(scale))),
            Mode::Clip | Mode::Reconstruct => {}
        }
    }

    /// The rest of the mode, on the demosaic of the mosaic [`Self::mosaic`] saw.
    pub fn demosaiced(&self, img: &mut RgbFrame, pending: Pending) {
        match pending {
            Pending::Nothing => {}
            Pending::Rgb => self.rgb(img),
            Pending::Masked(mask) => drop(reconstruct_masked(img, &mask, self.wb, self.clip)),
            Pending::Neutralize(mask) => {
                if (mask.width, mask.height) == (img.width, img.height) {
                    neutralize(img, self.wb, self.clip, |i, _| mask.at(i) == [true; 3]);
                }
            }
        }
    }

    /// Run the mode on camera RGB that was not interpolated across clip boundaries: linear raw
    /// data, monochrome data, binned previews. Segmentation needs a mosaic and falls back to
    /// inpaint opposed here.
    pub fn rgb(&self, img: &mut RgbFrame) {
        match self.mode {
            Mode::Clip => clip_neutral(img, self.wb, self.clip),
            Mode::Reconstruct => drop(reconstruct(img, self.wb, self.clip)),
            Mode::Opposed | Mode::Segmentation => {
                opposed(img, self.wb, self.clip);
                neutralize_clipped(img, self.wb, self.clip);
            }
        }
    }

    /// What is left after the mode ran on a mosaic that was then binned (no interpolation, so a
    /// clipped channel kept its maximum): the neutral pass of the darktable modes.
    pub(crate) fn binned(&self, img: &mut RgbFrame) {
        if matches!(self.mode, Mode::Opposed | Mode::Segmentation) {
            neutralize_clipped(img, self.wb, self.clip);
        }
    }
}

/// darktable's inpaint opposed and segmentation fill a channel clipped together with both others
/// from those two clipped values plus a chroma offset, so where all three clip the result is set
/// by the white-balance multipliers, not the scene: peach or lilac windows, magenta suns. This
/// pass blends such pixels to a neutral as bright as [`reconstruct`] makes them, weighted by how
/// much of their 3×3 neighbourhood is clipped in every channel, so the partly clipped ring keeps
/// its colour without a hard edge. The ports themselves stay bit-exact.
pub fn neutralize_clipped(img: &mut RgbFrame, wb: [f32; 3], clip: f32) {
    neutralize(img, wb, clip, |_, p| p.iter().all(|&v| v >= clip));
}

fn neutralize(img: &mut RgbFrame, wb: [f32; 3], clip: f32, all_clipped: impl Fn(usize, &[f32; 3]) -> bool + Sync) {
    let (w, h) = (img.width, img.height);
    let full: Vec<u8> = img.data.par_iter().enumerate().map(|(i, p)| u8::from(all_clipped(i, p))).collect();
    if !full.contains(&1) {
        return;
    }
    let mut rows = vec![0u8; w * h];
    rows.par_chunks_mut(w).zip(full.par_chunks(w)).for_each(|(out, row)| {
        for (x, o) in out.iter_mut().enumerate() {
            *o = row[x.saturating_sub(1)..(x + 2).min(w)].iter().sum();
        }
    });
    let max_level = wb.iter().cloned().fold(0.0f32, f32::max) * clip;
    img.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let t = (y.saturating_sub(1)..(y + 2).min(h)).map(|yy| u32::from(rows[yy * w + x])).sum::<u32>() as f32 / 9.0;
            if t <= 0.0 {
                continue;
            }
            let q = [px[0] * wb[0], px[1] * wb[1], px[2] * wb[2]];
            let n = q.iter().cloned().fold(max_level, f32::max);
            *px = std::array::from_fn(|c| (q[c] + (n - q[c]) * t) / wb[c]);
        }
    });
}

/// Clip every channel of `wb ⊙ img` at `min_c(wb_c) · clip` — i.e. at the lowest channel's clip level after WB —
/// then divide the multipliers back out. `clip` is the sensor clip in normalised units (≈ 1.0).
pub fn clip_neutral(img: &mut RgbFrame, wb: [f32; 3], clip: f32) {
    let limit = wb.iter().cloned().fold(f32::MAX, f32::min) * clip;
    for p in &mut img.data {
        for c in 0..3 {
            p[c] = (p[c] * wb[c]).min(limit) / wb[c];
        }
    }
}

/// Fill `values` (per-pixel vectors) where `valid` is false from valid neighbours, coarse to fine.
fn fill_invalid(w: usize, h: usize, values: &mut [[f32; 3]], valid: &mut [bool]) {
    if w == 0 || h == 0 || valid.iter().all(|&v| v) || !valid.iter().any(|&v| v) {
        return;
    }
    // Build a pyramid of weighted means, fill each level from the next coarser one.
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut cv = vec![[0f32; 3]; cw * ch];
    let mut cval = vec![false; cw * ch];
    {
        let (values, valid) = (&*values, &*valid);
        downsample(w, h, &mut cv, &mut cval, |i| valid[i].then(|| values[i]));
    }
    if cw * ch < w * h {
        fill_invalid(cw, ch, &mut cv, &mut cval);
    } else {
        // cannot shrink further (1×1): nothing valid anywhere handled above
        return;
    }
    values.par_chunks_mut(w).zip(valid.par_chunks_mut(w)).enumerate().for_each(|(y, (vrow, okrow))| {
        for x in 0..w {
            if !okrow[x] {
                vrow[x] = upsample(&cv, cw, ch, x, y);
                okrow[x] = true;
            }
        }
    });
}

/// One pyramid step: each `2 × 2` block of a `w × h` level becomes the mean of its valid
/// entries (`at(i)`: the value at index `i` when valid) in `cv`, `cval` (`⌈w/2⌉ × ⌈h/2⌉`).
fn downsample(w: usize, h: usize, cv: &mut [[f32; 3]], cval: &mut [bool], at: impl Fn(usize) -> Option<[f32; 3]> + Sync) {
    let cw = w.div_ceil(2);
    cv.par_chunks_mut(cw).zip(cval.par_chunks_mut(cw)).enumerate().for_each(|(y, (vrow, okrow))| {
        for x in 0..cw {
            let (mut s, mut n) = ([0f32; 3], 0);
            for dy in 0..2 {
                for dx in 0..2 {
                    let (sx, sy) = (2 * x + dx, 2 * y + dy);
                    if sx < w
                        && sy < h
                        && let Some(v) = at(sy * w + sx)
                    {
                        for c in 0..3 {
                            s[c] += v[c];
                        }
                        n += 1;
                    }
                }
            }
            if n > 0 {
                vrow[x] = s.map(|v| v / n as f32);
                okrow[x] = true;
            }
        }
    });
}

/// Bilinear sample of the `cw × ch` coarse level at fine pixel `(x, y)` (twice the resolution).
fn upsample(cv: &[[f32; 3]], cw: usize, ch: usize, x: usize, y: usize) -> [f32; 3] {
    let fx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (cw - 1) as f32);
    let fy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (ch - 1) as f32);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(ch - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let g = |xx: usize, yy: usize| cv[yy * cw + xx];
    let mut v = [0f32; 3];
    for c in 0..3 {
        let top = g(x0, y0)[c] + (g(x1, y0)[c] - g(x0, y0)[c]) * tx;
        let bot = g(x0, y1)[c] + (g(x1, y1)[c] - g(x0, y1)[c]) * tx;
        v[c] = top + (bot - top) * ty;
    }
    v
}

/// White-balanced chromaticity of a pixel (`None`: too dark to tell).
#[inline]
fn chroma_of(p: &[f32; 3], wb: [f32; 3]) -> Option<[f32; 3]> {
    let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
    let s = q[0] + q[1] + q[2];
    (s > 1e-4).then(|| q.map(|v| v.max(0.0) / s))
}

/// Reconstruct partially clipped channels. `clip` is the sensor clip level in normalised units (use slightly
/// below 1.0, e.g. 0.99), `wb` the white-balance multipliers that will be applied afterwards.
/// Returns the number of pixels that had at least one clipped channel.
///
/// The chromaticity field is diffused coarse to fine from the unclipped pixels. A clipped pixel is
/// never valid at full resolution, so its chromaticity is always the bilinear sample of the
/// half-resolution level: we start the pyramid there, straight from the image, and never build
/// the full-resolution field (same result, a quarter of the memory and far less work).
pub fn reconstruct(img: &mut RgbFrame, wb: [f32; 3], clip: f32) -> usize {
    rebuild(img, wb, clip, |_, p| [p[0] >= clip, p[1] >= clip, p[2] >= clip])
}

/// [`reconstruct`] on demosaiced data, with the clipped channels of each pixel taken from `mask`
/// (built on the mosaic, see [`ClipMask`]) instead of thresholding interpolated values: a
/// demosaic spreads clipped samples into neighbours that would otherwise pass as valid and keep a
/// wrong colour.
pub fn reconstruct_masked(img: &mut RgbFrame, mask: &ClipMask, wb: [f32; 3], clip: f32) -> usize {
    if (mask.width, mask.height) != (img.width, img.height) {
        return reconstruct(img, wb, clip);
    }
    rebuild(img, wb, clip, |i, _| mask.at(i))
}

fn rebuild(img: &mut RgbFrame, wb: [f32; 3], clip: f32, clipped: impl Fn(usize, &[f32; 3]) -> [bool; 3] + Sync) -> usize {
    let (w, h) = (img.width, img.height);
    let count = img.data.par_iter().enumerate().filter(|(i, p)| clipped(*i, p).contains(&true)).count();
    if count == 0 {
        return 0;
    }
    // chromaticity (white-balanced ratios to the channel sum) of unclipped pixels, at half resolution
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut cv = vec![[1f32 / 3.0; 3]; cw * ch];
    let mut cval = vec![false; cw * ch];
    {
        let data = &img.data;
        downsample(w, h, &mut cv, &mut cval, |i| if clipped(i, &data[i]).contains(&true) { None } else { chroma_of(&data[i], wb) });
    }
    if cw * ch < w * h {
        fill_invalid(cw, ch, &mut cv, &mut cval);
    }
    let any_valid = cw * ch < w * h && cval.iter().any(|&v| v);
    let max_level = wb.iter().cloned().fold(0.0f32, f32::max) * clip;
    img.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let p = *px;
            let cl = clipped(y * w + x, &p);
            if !cl.contains(&true) {
                continue;
            }
            let r = if any_valid { upsample(&cv, cw, ch, x, y) } else { [1f32 / 3.0; 3] };
            let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
            // estimate the white-balanced channel sum from the unclipped channels
            let (mut sum, mut k) = (0f32, 0usize);
            for c in 0..3 {
                if !cl[c] && r[c] > 1e-3 {
                    sum += q[c] / r[c];
                    k += 1;
                }
            }
            if k == 0 {
                // clipped in every channel: filled from the rebuilt ring below
                continue;
            }
            let sum = sum / k as f32;
            let mut out = q;
            for c in 0..3 {
                if cl[c] {
                    // bounded: a nearly empty unclipped channel must not blow the estimate up
                    out[c] = q[c].max((sum * r[c]).min(MAX_REBUILT * max_level));
                }
            }
            *px = [out[0] / wb[0], out[1] / wb[1], out[2] / wb[2]];
        }
    });
    fill_cores(img, wb, clip, &clipped);
    count
}

/// A rebuilt channel is at most this many times the brightest white-balanced clip level (2 EV).
const MAX_REBUILT: f32 = 4.0;

/// Pixels clipped in every channel have nothing left to rebuild from. A constant neutral there
/// leaves a grey hole, darker and greyer than the rebuilt ring around it, in a blown sun or a sky
/// seen through a window. Instead the white-balanced values of the ring (the pixels touching the
/// core, partly clipped and rebuilt or not clipped at all) are diffused into the core, their colour
/// fading to neutral with depth over about 1 % of the image, never below the neutral at the
/// brightest plausible level (`max(wb) · clip`, what the core used to get).
fn fill_cores(img: &mut RgbFrame, wb: [f32; 3], clip: f32, clipped: &(impl Fn(usize, &[f32; 3]) -> [bool; 3] + Sync)) {
    let (w, h) = (img.width, img.height);
    let is_core = |i: usize, p: &[f32; 3]| clipped(i, p) == [true; 3];
    // the cores' bounding box, one pixel wider: depth (and core-ness) is only needed there
    let bbox = img
        .data
        .par_chunks(w.max(1))
        .enumerate()
        .filter_map(|(y, row)| {
            let core = |x: &usize| is_core(y * w + x, &row[*x]);
            let first = (0..row.len()).find(core)?;
            let last = (0..row.len()).rev().find(core).unwrap_or(first);
            Some((first, last, y, y))
        })
        .reduce_with(|a, b| (a.0.min(b.0), a.1.max(b.1), a.2.min(b.2), a.3.max(b.3)));
    let Some((x0, x1, y0, y1)) = bbox else { return };
    let (bx, by) = (x0.saturating_sub(1), y0.saturating_sub(1));
    let (bw, bh) = ((x1 + 2).min(w) - bx, (y1 + 2).min(h) - by);
    let depth = core_depth(bw, bh, |x, y| {
        let i = (by + y) * w + bx + x;
        is_core(i, &img.data[i])
    });
    let core_at = |x: usize, y: usize| x >= bx && y >= by && x < bx + bw && y < by + bh && depth[(y - by) * bw + x - bx] > 0;
    let balanced = |p: &[f32; 3]| [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut sv = vec![[0f32; 3]; cw * ch];
    let mut sval = vec![false; cw * ch];
    {
        let data = &img.data;
        downsample(w, h, &mut sv, &mut sval, |i| {
            let (x, y) = (i % w, i / w);
            if core_at(x, y) {
                return None;
            }
            let ring = (y.saturating_sub(1)..(y + 2).min(h)).any(|yy| (x.saturating_sub(1)..(x + 2).min(w)).any(|xx| core_at(xx, yy)));
            ring.then(|| balanced(&data[i]))
        });
    }
    let seeded = cw * ch < w * h && sval.contains(&true);
    if seeded {
        fill_invalid(cw, ch, &mut sv, &mut sval);
    }
    let max_level = wb.iter().cloned().fold(0.0f32, f32::max) * clip;
    let fade = (0.01 * w.max(h) as f32).max(4.0);
    img.data[by * w..(by + bh) * w].par_chunks_mut(w).enumerate().for_each(|(dy, row)| {
        for dx in 0..bw {
            let d = depth[dy * bw + dx];
            if d == 0 {
                continue;
            }
            let x = bx + dx;
            let px = &mut row[x];
            let q = balanced(px);
            let v = q.iter().cloned().fold(max_level, f32::max);
            let out = if seeded {
                let ring = upsample(&sv, cw, ch, x, by + dy);
                let mean = (ring[0] + ring[1] + ring[2]) / 3.0;
                let s = (d as f32 / 3.0 / fade).min(1.0);
                let t = s * s * (3.0 - 2.0 * s);
                std::array::from_fn(|c| (ring[c] + (mean - ring[c]) * t).max(v))
            } else {
                [v; 3]
            };
            *px = [out[0] / wb[0], out[1] / wb[1], out[2] / wb[2]];
        }
    });
}

/// Chamfer (3-4) distance of each `core` pixel of a `w × h` box to the nearest pixel that is not a
/// core one, in thirds of a pixel; 0 outside the cores. The box's border is never core-free
/// territory: a core reaching it is measured from its other sides.
fn core_depth(w: usize, h: usize, core: impl Fn(usize, usize) -> bool + Sync) -> Vec<u16> {
    let mut d: Vec<u16> = vec![0; w * h];
    d.par_chunks_mut(w.max(1)).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            *v = if core(x, y) { u16::MAX } else { 0 };
        }
    });
    let relax = |d: &mut [u16], i: usize, j: usize, step: u16| d[i] = d[i].min(d[j].saturating_add(step));
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            if x > 0 {
                relax(&mut d, i, i - 1, 3);
            }
            if y > 0 {
                relax(&mut d, i, i - w, 3);
                if x > 0 {
                    relax(&mut d, i, i - w - 1, 4);
                }
                if x + 1 < w {
                    relax(&mut d, i, i - w + 1, 4);
                }
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            if x + 1 < w {
                relax(&mut d, i, i + 1, 3);
            }
            if y + 1 < h {
                relax(&mut d, i, i + w, 3);
                if x + 1 < w {
                    relax(&mut d, i, i + w + 1, 4);
                }
                if x > 0 {
                    relax(&mut d, i, i + w - 1, 4);
                }
            }
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    // The previous full-resolution implementation, kept as the reference for `reconstruct`.
    /// Fill `values` (per-pixel vectors) where `valid` is false from valid neighbours, coarse to fine.
    fn fill_invalid_ref(w: usize, h: usize, values: &mut [[f32; 3]], valid: &mut [bool]) {
        if w == 0 || h == 0 || valid.iter().all(|&v| v) || !valid.iter().any(|&v| v) {
            return;
        }
        // Build a pyramid of weighted means, fill each level from the next coarser one.
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let mut cv = vec![[0f32; 3]; cw * ch];
        let mut cval = vec![false; cw * ch];
        {
            let (values, valid) = (&*values, &*valid);
            cv.par_chunks_mut(cw).zip(cval.par_chunks_mut(cw)).enumerate().for_each(|(y, (vrow, okrow))| {
                for x in 0..cw {
                    let (mut s, mut n) = ([0f32; 3], 0);
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let (sx, sy) = (2 * x + dx, 2 * y + dy);
                            if sx < w && sy < h && valid[sy * w + sx] {
                                for c in 0..3 {
                                    s[c] += values[sy * w + sx][c];
                                }
                                n += 1;
                            }
                        }
                    }
                    if n > 0 {
                        vrow[x] = s.map(|v| v / n as f32);
                        okrow[x] = true;
                    }
                }
            });
        }
        if cw * ch < w * h {
            fill_invalid_ref(cw, ch, &mut cv, &mut cval);
        } else {
            // cannot shrink further (1×1): nothing valid anywhere handled above
            return;
        }
        values.par_chunks_mut(w).zip(valid.par_chunks_mut(w)).enumerate().for_each(|(y, (vrow, okrow))| {
            for x in 0..w {
                if !okrow[x] {
                    // bilinear upsample of the coarse level
                    let fx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (cw - 1) as f32);
                    let fy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (ch - 1) as f32);
                    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
                    let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(ch - 1));
                    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                    let g = |xx: usize, yy: usize| cv[yy * cw + xx];
                    let mut v = [0f32; 3];
                    for c in 0..3 {
                        let top = g(x0, y0)[c] + (g(x1, y0)[c] - g(x0, y0)[c]) * tx;
                        let bot = g(x0, y1)[c] + (g(x1, y1)[c] - g(x0, y1)[c]) * tx;
                        v[c] = top + (bot - top) * ty;
                    }
                    vrow[x] = v;
                    okrow[x] = true;
                }
            }
        });
    }

    /// Reconstruct partially clipped channels. `clip` is the sensor clip level in normalised units (use slightly
    /// below 1.0, e.g. 0.99), `wb` the white-balance multipliers that will be applied afterwards.
    /// Returns the number of pixels that had at least one clipped channel.
    fn reconstruct_ref(img: &mut RgbFrame, wb: [f32; 3], clip: f32) -> usize {
        let (w, h) = (img.width, img.height);
        let n = w * h;
        let is_clipped = |p: &[f32; 3]| p[0] >= clip || p[1] >= clip || p[2] >= clip;
        let count = img.data.par_iter().filter(|p| is_clipped(p)).count();
        if count == 0 {
            return 0;
        }
        // chromaticity (white-balanced ratios to the channel mean) of unclipped pixels
        let mut chroma = vec![[1f32 / 3.0; 3]; n];
        let mut valid = vec![false; n];
        chroma.par_iter_mut().zip(valid.par_iter_mut()).zip(img.data.par_iter()).for_each(|((r, v), p)| {
            if is_clipped(p) {
                return;
            }
            let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
            let s = q[0] + q[1] + q[2];
            if s > 1e-4 {
                *r = q.map(|v| v.max(0.0) / s);
                *v = true;
            }
        });
        fill_invalid_ref(w, h, &mut chroma, &mut valid);
        let max_level = wb.iter().cloned().fold(0.0f32, f32::max) * clip;
        img.data.par_iter_mut().zip(chroma.par_iter()).for_each(|(px, r)| {
            let p = *px;
            let cl = [p[0] >= clip, p[1] >= clip, p[2] >= clip];
            if !cl.iter().any(|&b| b) {
                return;
            }
            let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
            // estimate the white-balanced channel sum from the unclipped channels
            let (mut sum, mut k) = (0f32, 0usize);
            for c in 0..3 {
                if !cl[c] && r[c] > 1e-3 {
                    sum += q[c] / r[c];
                    k += 1;
                }
            }
            if k == 0 {
                return;
            }
            let sum = sum / k as f32;
            let mut out = q;
            for c in 0..3 {
                if cl[c] {
                    out[c] = q[c].max((sum * r[c]).min(MAX_REBUILT * max_level));
                }
            }
            *px = [out[0] / wb[0], out[1] / wb[1], out[2] / wb[2]];
        });
        fill_cores(img, wb, clip, &|_, p: &[f32; 3]| [p[0] >= clip, p[1] >= clip, p[2] >= clip]);
        count
    }

    /// `cargo test --release -p lightcraft-raw --lib -- --ignored bench_reconstruct --nocapture`
    #[test]
    #[ignore]
    fn bench_reconstruct() {
        let img = RgbFrame::from_fn(6000, 4000, |x, y| {
            let v = 0.4 + 0.8 * ((x as f32 * 0.003).sin() * (y as f32 * 0.002).cos()).abs();
            [v.min(1.0), (v * 0.7).min(1.0), (v * 0.5).min(1.0)]
        });
        let wb = [2.0, 1.0, 1.5];
        let time = |f: &dyn Fn(&mut RgbFrame) -> usize| {
            (0..3)
                .map(|_| {
                    let mut i = img.clone();
                    let t = std::time::Instant::now();
                    f(&mut i);
                    t.elapsed().as_secs_f64() * 1e3
                })
                .fold(f64::MAX, f64::min)
        };
        let new = time(&|i| reconstruct(i, wb, 0.99));
        let old = time(&|i| reconstruct_ref(i, wb, 0.99));
        eprintln!("reconstruct 24 MP: {new:.0} ms (full-resolution reference: {old:.0} ms)");
    }

    /// The half-resolution start gives exactly the full-resolution algorithm's result.
    #[test]
    fn matches_the_full_resolution_reference() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f32 / (1u64 << 24) as f32
        };
        for (w, h) in [(1, 1), (1, 2), (3, 1), (7, 5), (64, 48), (129, 77)] {
            for density in [0.02f32, 0.3, 0.97] {
                // smooth colour with clipped blobs, some dark pixels, some fully clipped ones
                let noise: Vec<f32> = (0..w * h).map(|_| rnd()).collect();
                let img = RgbFrame::from_fn(w, h, |x, y| {
                    let t = ((x as f32 * 0.21).sin() + (y as f32 * 0.17).cos()) * 0.5;
                    let base = [0.5 + 0.4 * t, 0.45 - 0.2 * t, 0.3 + 0.1 * t];
                    let r = noise[y * w + x];
                    if r < density * 0.3 {
                        [1.0, 1.0, 1.0]
                    } else if r < density {
                        [1.0, base[1] * 1.6, base[2]]
                    } else if r > 0.995 {
                        [0.0, 0.0, 0.00001]
                    } else {
                        base
                    }
                });
                let wb = [2.1, 1.0, 1.6];
                let (mut a, mut b) = (img.clone(), img.clone());
                assert_eq!(reconstruct(&mut a, wb, 0.99), reconstruct_ref(&mut b, wb, 0.99));
                assert!(a.data == b.data, "{w}×{h} at {density}");
            }
        }
    }

    #[test]
    fn clip_neutral_makes_clipped_white_neutral() {
        let wb = [2.0, 1.0, 1.5];
        let mut img = RgbFrame::filled(2, 1, [1.0, 1.0, 1.0]);
        img.data[1] = [0.2, 0.3, 0.4];
        clip_neutral(&mut img, wb, 1.0);
        let p = img.data[0];
        let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
        assert!((q[0] - q[1]).abs() < 1e-6 && (q[1] - q[2]).abs() < 1e-6);
        assert_eq!(img.data[1], [0.2, 0.3, 0.4]);
    }

    #[test]
    fn reconstruct_restores_clipped_channel() {
        // a warm gradient whose red channel clips in the bright half
        let truth = RgbFrame::from_fn(64, 16, |x, _| {
            let v = 0.3 + 1.2 * x as f32 / 63.0;
            [v * 1.0, v * 0.6, v * 0.35]
        });
        let mut img = truth.map(|p| p.map(|v| v.min(1.0)));
        let before: f32 = img.data.iter().zip(&truth.data).map(|(a, b)| (a[0] - b[0]).abs()).sum();
        let n = reconstruct(&mut img, [1.0, 1.0, 1.0], 0.999);
        assert!(n > 0);
        let after: f32 = img.data.iter().zip(&truth.data).map(|(a, b)| (a[0] - b[0]).abs()).sum();
        assert!(after < before * 0.2, "before {before} after {after}");
        // unclipped pixels untouched
        assert_eq!(img.get(0, 0), truth.get(0, 0));
    }

    #[test]
    fn fully_clipped_and_no_clipping() {
        let mut img = RgbFrame::filled(4, 4, [1.0; 3]);
        assert_eq!(reconstruct(&mut img, [2.0, 1.0, 1.5], 0.99), 16);
        let p = img.data[0];
        assert!((p[0] * 2.0 - p[1]).abs() < 1e-5 && (p[2] * 1.5 - p[1]).abs() < 1e-5);
        let mut img = RgbFrame::filled(4, 4, [0.5; 3]);
        assert_eq!(reconstruct(&mut img, [2.0, 1.0, 1.5], 0.99), 0);
        let mut empty = RgbFrame::new(0, 0);
        assert_eq!(reconstruct(&mut empty, [1.0; 3], 0.99), 0);
    }

    /// A blue sky brightening into the sun (camera RGB under daylight multipliers): the core that
    /// clips in every channel continues the rebuilt ring around it, no darker and with no jump in
    /// hue, instead of a grey disc inside a brighter blue halo.
    #[test]
    fn a_fully_clipped_core_continues_its_ring() {
        let wb = [2.2, 1.0, 1.6];
        let sky = [0.35, 0.6, 1.0];
        let (w, h) = (128usize, 128usize);
        let scene = |x: usize, y: usize| {
            let r2 = (x as f32 - 64.0).powi(2) + (y as f32 - 64.0).powi(2);
            let b = 0.5 + 12.0 * (-r2 / (2.0 * 14.0 * 14.0)).exp();
            std::array::from_fn::<f32, 3, _>(|c| (b * sky[c] / wb[c]).min(1.0))
        };
        let mut img = RgbFrame::from_fn(w, h, scene);
        reconstruct(&mut img, wb, 0.99);
        let balanced = |p: [f32; 3]| [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
        let (mut cores, mut rings) = (0, 0);
        for x in 64..w - 1 {
            let (a, b) = (scene(x, 64), scene(x + 1, 64));
            if !a.iter().chain(&b).any(|&v| v >= 0.99) {
                continue;
            }
            cores += usize::from(a.iter().all(|&v| v >= 0.99));
            rings += usize::from(!a.iter().all(|&v| v >= 0.99));
            let (p, q) = (balanced(img.get(x, 64)), balanced(img.get(x + 1, 64)));
            let (lp, lq) = (p.iter().sum::<f32>() / 3.0, q.iter().sum::<f32>() / 3.0);
            assert!(lq <= lp * 1.02, "brighter outward at x = {x}: {p:?} then {q:?}");
            let chroma = |v: [f32; 3]| v.map(|c| c / (v[0] + v[1] + v[2]));
            let step = (0..3).map(|c| (chroma(p)[c] - chroma(q)[c]).abs()).fold(0.0, f32::max);
            assert!(step < 0.1, "hue jump {step} at x = {x}: {p:?} then {q:?}");
        }
        assert!(cores > 3 && rings > 3, "the scene has a core ({cores}) and a ring ({rings})");
        // deep inside, the core fades to neutral
        let c = balanced(img.get(64, 64));
        assert!((c[0] - c[2]).abs() < 0.01 * c[1] && (c[1] - c[2]).abs() < 0.01 * c[1], "{c:?}");
        // and a deep blue sky with almost no red never blows the rebuilt channels up
        let mut deep = RgbFrame::from_fn(32, 32, |x, _| if x < 16 { [0.002, 0.4, 0.6] } else { [0.004, 1.0, 1.0] });
        reconstruct(&mut deep, wb, 0.99);
        let max_level = 2.2 * 0.99;
        assert!(deep.data.iter().all(|p| balanced(*p).iter().all(|&v| v <= 4.0 * max_level + 1e-3)));
    }
}
