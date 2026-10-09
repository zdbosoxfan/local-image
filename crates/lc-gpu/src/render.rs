//! The GPU renderer: the CPU pipeline's stages, evaluated on the device where a kernel exists and
//! on the CPU otherwise (per-stage hybrid). Stage results stay on the device and are cached per
//! view exactly like [`lightcraft_pipeline::StageCache`] (same keys, see [`lightcraft_pipeline::Plan`]).

use std::sync::{Arc, Mutex};

use lightcraft_develop::DevelopSettings;
use lightcraft_geom::Orientation;
use lightcraft_pipeline::capture::CaptureParams;
use lightcraft_pipeline::finish::{FinishParams, MASK_TERMS, mask_terms};
use lightcraft_pipeline::geometry::SampleMode;
use lightcraft_pipeline::{Plan, RenderRequest, Rendered, SourceInfo, local};
use lightcraft_raster::resample::Filter;
use lightcraft_raster::{Histogram, Plane, Rgb32f, Rgba8};

use crate::ctx::{Buf, FailKind, Gpu, fail, groups1, groups2};
use crate::params::{Present, finish_block};

/// Device-resident stages of one view, kept next to the CPU [`lightcraft_pipeline::StageCache`]
/// (as its extension). Holds the last two output sizes, like the CPU cache.
#[derive(Default)]
pub struct GpuStages {
    entries: Mutex<Vec<Entry>>,
    /// The uploaded source and last capture result (shared by all output sizes).
    source: Mutex<Option<Source>>,
    primary_dispatches: std::sync::atomic::AtomicUsize,
}

struct Source {
    src: Arc<Rgb32f>,
    uploaded: Arc<Buf>,
    capture: Option<(CaptureParams, Arc<Buf>)>,
}

const CAPACITY: usize = 2;

#[derive(Clone)]
struct Entry {
    src: Arc<Rgb32f>,
    capture: Option<CaptureParams>,
    geo: u64,
    sampled: Arc<Buf>,
    lin: Option<(u64, Arc<Buf>)>,
    planes: Planes,
    primary: Option<(u64, Arc<Buf>)>,
    native_primary: crate::primary::Cache,
}

/// Spatial planes of one linear image (`key` = its `lin_key`), each tagged with its radius.
#[derive(Clone, Default)]
struct Planes {
    key: u64,
    log_l: Option<Arc<Buf>>,
    base: Option<(u32, Arc<Buf>)>,
    clarity: Option<(u32, Arc<Buf>)>,
    texture: Option<(u32, Arc<Buf>)>,
    haze: Option<(Arc<Buf>, Arc<Buf>)>,
    sharp: Option<(u32, Arc<Buf>, Arc<Buf>)>,
    chroma: Option<(u32, Arc<Buf>)>,
    tone_eq: Option<(u64, Arc<Buf>)>,
}

impl GpuStages {
    fn get(&self, src: &Arc<Rgb32f>, geo: u64, capture: Option<CaptureParams>) -> Option<Entry> {
        self.lock().iter().find(|e| e.geo == geo && e.capture == capture && Arc::ptr_eq(&e.src, src)).cloned()
    }

    fn put(&self, e: Entry) {
        let mut v = self.lock();
        v.retain(|o| !(o.geo == e.geo && Arc::ptr_eq(&o.src, &e.src)));
        v.push(e);
        while v.len() > CAPACITY {
            v.remove(0);
        }
    }

    fn source(&self, cx: &mut Cx<'_>, src: &Arc<Rgb32f>, capture: Option<CaptureParams>) -> Arc<Buf> {
        let mut g = self.source.lock().unwrap_or_else(|e| e.into_inner());
        if !g.as_ref().is_some_and(|s| Arc::ptr_eq(&s.src, src)) {
            *g = None; // free the previous photo's buffers first
        }
        let source = g.get_or_insert_with(|| Source { src: src.clone(), uploaded: Arc::new(cx.gpu.upload(rgb_words(src))), capture: None });
        let Some(p) = capture else { return source.uploaded.clone() };
        if let Some((key, buf)) = &source.capture
            && *key == p
        {
            return buf.clone();
        }
        source.capture = None;
        let buf = Arc::new(crate::capture::sharpen(cx, &source.uploaded, src.width, src.height, &p));
        // Publish only after submission: another render thread may reuse this result immediately.
        cx.flush();
        source.capture = Some((p, buf.clone()));
        buf
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Entry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Compute dispatches in the most recent primary stage (zero for neutral controls).
    pub fn primary_dispatches(&self) -> usize {
        self.primary_dispatches.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Number of cached output sizes.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Device bytes held (each buffer counted once).
    pub fn bytes(&self) -> usize {
        let mut seen: Vec<*const Buf> = Vec::new();
        let mut total = 0;
        let mut add = |b: &Arc<Buf>| {
            let p = Arc::as_ptr(b);
            if !seen.contains(&p) {
                seen.push(p);
                total += b.len * 4;
            }
        };
        for e in self.lock().iter() {
            add(&e.sampled);
            for b in e.native_primary.buffers() {
                add(b);
            }
            if let Some((_, b)) = &e.primary {
                add(b);
            }
            if let Some((_, b)) = &e.lin {
                add(b);
            }
            let p = &e.planes;
            p.log_l.iter().for_each(&mut add);
            p.base.iter().for_each(|(_, b)| add(b));
            p.clarity.iter().for_each(|(_, b)| add(b));
            p.texture.iter().for_each(|(_, b)| add(b));
            p.haze.iter().for_each(|(b, air)| {
                add(b);
                add(air);
            });
            p.sharp.iter().for_each(|(_, a, b)| {
                add(a);
                add(b);
            });
            p.chroma.iter().for_each(|(_, b)| add(b));
            p.tone_eq.iter().for_each(|(_, b)| add(b));
        }
        if let Some(s) = &*self.source.lock().unwrap_or_else(|e| e.into_inner()) {
            add(&s.uploaded);
            s.capture.iter().for_each(|(_, b)| add(b));
        }
        total
    }

    /// Drop everything (e.g. when memory is needed).
    pub fn clear(&self) {
        self.lock().clear();
        *self.source.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A render in progress: the device and the commands recorded so far. Reading a result back
/// submits everything recorded before it.
pub(crate) struct Cx<'a> {
    pub gpu: &'a Gpu,
    enc: Option<wgpu::CommandEncoder>,
    /// Kernel invocations recorded since the last submit.
    pending: u64,
    primary_dispatches: usize,
}

/// Kernel invocations recorded before a render submits them (under one full-size pass over a
/// 24 MP export, several passes over a preview): no single submission runs long.
const FLUSH_INVOCATIONS: u64 = 16 << 20;

/// Pixels per dispatch of the per-pixel stage (the heaviest kernel).
const BAND_PIXELS: usize = 4 << 20;

impl<'a> Cx<'a> {
    pub fn new(gpu: &'a Gpu) -> Cx<'a> {
        Cx { gpu, enc: Some(gpu.encoder()), pending: 0, primary_dispatches: 0 }
    }

    /// Record kernel `name` (see [`Gpu::run`]).
    pub fn run(&mut self, name: &str, p: &[u32], bufs: &[Option<&Buf>], groups: [u32; 3]) {
        let enc = self.enc.get_or_insert_with(|| self.gpu.encoder());
        self.gpu.run(enc, name, p, bufs, groups);
        if name.starts_with("p_") {
            self.primary_dispatches += 1;
        }
        let invocations = if name == "p_deriche" { 64 } else { 256 };
        self.pending += groups.iter().map(|g| *g as u64).product::<u64>() * invocations;
        if self.pending >= FLUSH_INVOCATIONS {
            self.flush();
        }
    }

    #[cfg(test)]
    pub(crate) fn primary_dispatches(&self) -> usize {
        self.primary_dispatches
    }

    /// Submit what is recorded and read back `len` values of `b`.
    pub fn read<T: bytemuck::Pod>(&mut self, b: &Buf, len: usize) -> Vec<T> {
        self.pending = 0;
        let enc = self.enc.take().unwrap_or_else(|| self.gpu.encoder());
        self.gpu.finish_and_read(enc, b, len)
    }

    /// Submit what is recorded so far without waiting. Renders submit stage by stage and every
    /// [`FLUSH_INVOCATIONS`]: long submissions are what GPU watchdogs reset (Windows TDR, the
    /// i915 hang check; issue #78's iGPU spent seconds on an export recorded as one submission),
    /// and buffers dropped by earlier work become reusable by later work (lower peak memory).
    pub fn flush(&mut self) {
        self.pending = 0;
        if let Some(enc) = self.enc.take() {
            self.gpu.submit([enc.finish()]);
        }
    }

    /// Submit what is recorded and wait for it (profiling only: stage timings).
    pub fn sync(&mut self) {
        self.pending = 0;
        if let Some(enc) = self.enc.take() {
            self.gpu.submit([enc.finish()]);
        }
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// A zero-filled buffer (the clear is recorded).
    pub fn zeroed(&mut self, len: usize) -> Buf {
        let out = self.gpu.buffer(len);
        let enc = self.enc.get_or_insert_with(|| self.gpu.encoder());
        enc.clear_buffer(out.raw(), 0, None);
        out
    }

    /// A copy of `b` (recorded).
    pub fn copy(&mut self, b: &Buf) -> Buf {
        let out = self.gpu.buffer(b.len);
        let enc = self.enc.get_or_insert_with(|| self.gpu.encoder());
        enc.copy_buffer_to_buffer(b.raw(), 0, out.raw(), 0, (b.len * 4) as u64);
        out
    }

    /// `len` values of `b` from `offset` (both in 32-bit words), as a new buffer (recorded).
    pub fn slice(&mut self, b: &Buf, offset: usize, len: usize) -> Buf {
        let out = self.gpu.buffer(len);
        let enc = self.enc.get_or_insert_with(|| self.gpu.encoder());
        enc.copy_buffer_to_buffer(b.raw(), (offset * 4) as u64, out.raw(), 0, (len * 4) as u64);
        out
    }

    /// Copy all of `src` into `dst` at `offset` (32-bit words; recorded).
    pub fn copy_into(&mut self, src: &Buf, dst: &Buf, offset: usize) {
        let enc = self.enc.get_or_insert_with(|| self.gpu.encoder());
        enc.copy_buffer_to_buffer(src.raw(), 0, dst.raw(), (offset * 4) as u64, (src.len * 4) as u64);
    }

    pub fn read_rgb(&mut self, b: &Buf, w: usize, h: usize) -> Rgb32f {
        Rgb32f { width: w, height: h, data: self.read(b, w * h * 3) }
    }

    pub fn read_plane(&mut self, b: &Buf, w: usize, h: usize) -> Plane {
        Plane { width: w, height: h, data: self.read(b, w * h) }
    }
}

pub(crate) fn rgb_words(img: &Rgb32f) -> &[f32] {
    bytemuck::cast_slice(&img.data)
}

// ---------------------------------------------------------------------------------------------
// Filters (twins of `lightcraft_raster::blur` / `resample` and `lightcraft_pipeline::local`)

/// Pixels each blur thread slides its running sum over (at least; longer for large radii, so the
/// priming sum stays a small share of the work).
const CHUNK: usize = 16;

/// Gaussian blur of a `w × h` image of `nc` interleaved channels: three box passes each way
/// (`lightcraft_raster::blur::gaussian`).
pub(crate) fn gaussian(cx: &mut Cx<'_>, src: &Buf, w: usize, h: usize, nc: usize, sigma: f32) -> Buf {
    if sigma <= 0.3 || w * h == 0 {
        return cx.copy(src);
    }
    let radii = lightcraft_raster::blur::box_radii(sigma);
    let passes: Vec<(&str, usize)> = radii.iter().map(|r| ("box_h", *r)).chain(radii.iter().map(|r| ("box_v", *r))).filter(|p| p.1 > 0).collect();
    if passes.is_empty() {
        return cx.copy(src);
    }
    // ping-pong between two buffers
    let mut bufs = [cx.gpu.buffer(w * h * nc), cx.gpu.buffer(w * h * nc)];
    for (i, (k, r)) in passes.iter().enumerate() {
        let chunk = (2 * r + 1).clamp(CHUNK, 128);
        let groups = if *k == "box_h" { groups2(w.div_ceil(chunk), h, [64, 4]) } else { groups2(w, h.div_ceil(chunk), [64, 4]) };
        let p = [w as u32, h as u32, nc as u32, *r as u32, chunk as u32];
        let (out, prev) = (&bufs[i % 2], &bufs[(i + 1) % 2]);
        cx.run(k, &p, &[Some(if i == 0 { src } else { prev }), Some(out)], groups);
    }
    let last = (passes.len() - 1) % 2;
    let [a, b] = std::mem::replace(&mut bufs, [cx.gpu.buffer(0), cx.gpu.buffer(0)]);
    if last == 0 { a } else { b }
}

/// Resample taps of `lightcraft_raster::resample::resize`, packed for the `resize_*` kernels.
fn taps(src: usize, dst: usize, filter: Filter) -> Vec<u32> {
    let w = lightcraft_raster::resample::weights(src, dst, filter);
    let mut t = Vec::with_capacity(dst * 3);
    let mut weights = Vec::new();
    let base = dst * 3;
    for (lo, ws) in &w {
        t.extend_from_slice(&[*lo as u32, ws.len() as u32, (base + weights.len()) as u32]);
        weights.extend(ws.iter().map(|v| v.to_bits()));
    }
    t.extend(weights);
    t
}

/// `lightcraft_raster::resample::resize` of an `nc`-channel image.
pub(crate) fn resize(cx: &mut Cx<'_>, src: &Buf, (sw, sh): (usize, usize), (dw, dh): (usize, usize), nc: usize, filter: Filter) -> Buf {
    let (dw, dh) = (dw.max(1), dh.max(1));
    if (sw, sh) == (dw, dh) {
        return cx.copy(src);
    }
    let tx = cx.gpu.upload(&taps(sw, dw, filter));
    let tmp = cx.gpu.buffer(dw * sh * nc);
    cx.run("resize_h", &[sw as u32, sh as u32, dw as u32, nc as u32], &[Some(src), Some(&tx), Some(&tmp)], groups2(dw, sh, [16, 16]));
    let ty = cx.gpu.upload(&taps(sh, dh, filter));
    let out = cx.gpu.buffer(dw * dh * nc);
    cx.run("resize_v", &[dw as u32, sh as u32, dh as u32, nc as u32], &[Some(&tmp), Some(&ty), Some(&out)], groups2(dw, dh, [16, 16]));
    out
}

/// An element-wise kernel of the `map` module over `n` items.
pub(crate) fn map(cx: &mut Cx<'_>, k: &str, n: usize, extra: &[u32], ins: [Option<&Buf>; 3], out: &Buf) {
    let mut p = vec![n as u32];
    p.extend_from_slice(extra);
    cx.run(k, &p, &[ins[0], ins[1], ins[2], Some(out)], groups1(n));
}

/// Self-guided filter of a plane (`local::guided`).
fn guided(cx: &mut Cx<'_>, p: &Buf, w: usize, h: usize, sigma: f32, eps: f32) -> Buf {
    let ab = guided_coeffs(cx, p, w, h, sigma, eps);
    let q = cx.gpu.buffer(w * h);
    map(cx, "guided_apply", w * h, &[], [Some(p), Some(&ab), None], &q);
    q
}

/// Blurred (a, b) coefficients, interleaved.
fn guided_coeffs(cx: &mut Cx<'_>, p: &Buf, w: usize, h: usize, sigma: f32, eps: f32) -> Buf {
    let n = w * h;
    let pp = cx.gpu.buffer(2 * n);
    map(cx, "guided_pre", n, &[], [Some(p), None, None], &pp);
    let m = gaussian(cx, &pp, w, h, 2, sigma);
    let ab = cx.gpu.buffer(2 * n);
    map(cx, "guided_ab", n, &[eps.to_bits()], [Some(&m), None, None], &ab);
    gaussian(cx, &ab, w, h, 2, sigma)
}

/// Fast guided filter (`local::guided_fast`): coefficients on a subsampled grid.
fn guided_fast(cx: &mut Cx<'_>, p: &Buf, w: usize, h: usize, sigma: f32, eps: f32) -> Buf {
    let s = local::guided_fast_step(sigma);
    if s <= 1 {
        return guided(cx, p, w, h, sigma, eps);
    }
    let (lw, lh) = (w.div_ceil(s).max(1), h.div_ceil(s).max(1));
    let lo = resize(cx, p, (w, h), (lw, lh), 1, Filter::Box);
    let ab = guided_coeffs(cx, &lo, lw, lh, sigma / s as f32, eps);
    let up = resize(cx, &ab, (lw, lh), (w, h), 2, Filter::Bilinear);
    let q = cx.gpu.buffer(w * h);
    map(cx, "guided_apply", w * h, &[], [Some(p), Some(&up), None], &q);
    q
}

// ---------------------------------------------------------------------------------------------
// Geometry (`Frame::sample`)

/// Integer pixel map `(x, y) → (m0·x + m1·y + m2, m3·x + m4·y + m5)` from an oriented image back
/// to the source of `w × h` (`Image::oriented`: optional horizontal flip, then quarter turns).
pub(crate) fn orient_map(o: Orientation, w: usize, h: usize) -> [i32; 6] {
    let (flip, turns) = o.to_parts();
    let (w, h) = (w as i32, h as i32);
    let mut ops: Vec<fn(i32, i32) -> [i32; 6]> = Vec::new();
    if flip {
        ops.push(|w, _| [-1, 0, w - 1, 0, 1, 0]);
    }
    let cw: fn(i32, i32) -> [i32; 6] = |_, h| [0, 1, 0, -1, 0, h - 1];
    let half: fn(i32, i32) -> [i32; 6] = |w, h| [-1, 0, w - 1, 0, -1, h - 1];
    match turns {
        1 => ops.push(cw),
        2 => ops.push(half),
        3 => {
            ops.push(half);
            ops.push(cw);
        }
        _ => {}
    }
    // compose: T ← T ∘ op, tracking each op's input size
    let mut t = [1, 0, 0, 0, 1, 0];
    let (mut cw_, mut ch_) = (w, h);
    for op in ops {
        let o = op(cw_, ch_);
        t = [
            t[0] * o[0] + t[1] * o[3],
            t[0] * o[1] + t[1] * o[4],
            t[0] * o[2] + t[1] * o[5] + t[2],
            t[3] * o[0] + t[4] * o[3],
            t[3] * o[1] + t[4] * o[4],
            t[3] * o[2] + t[4] * o[5] + t[5],
        ];
        if o[0] == 0 {
            (cw_, ch_) = (ch_, cw_);
        }
    }
    t
}

fn affine_bits(a: &lightcraft_geom::Affine) -> [u32; 6] {
    a.0.map(|v| (v as f32).to_bits())
}

/// The `sample_warp` kernel's parameters (layout documented in `geom.wgsl`).
fn warp_params(
    wp: &lightcraft_pipeline::optics::Warp,
    base: (usize, usize),
    out: (usize, usize),
    o2t: &lightcraft_geom::Affine,
    sx: f64,
    sy: f64,
) -> Vec<u32> {
    let f = |v: f64| (v as f32).to_bits();
    let mut p = vec![base.0 as u32, base.1 as u32, out.0 as u32, out.1 as u32];
    p.extend(affine_bits(o2t));
    p.extend([f(sx), f(sy), f(wp.w), f(wp.h)]);
    let persp = wp.persp != lightcraft_geom::Homography::IDENTITY;
    p.push(persp as u32);
    p.extend(wp.persp_inv.0.map(f));
    p.push(f(wp.k1));
    p.extend(wp.ca.map(f));
    p.push(f(wp.lens_dist));
    let warp = wp.lens.and_then(|l| l.warp);
    p.push(warp.is_some() as u32);
    let w = warp.unwrap_or_default();
    p.extend(w.planes.iter().flatten().map(|v| f(*v)));
    p.extend([f(w.center.x), f(w.center.y), f(w.radius)]);
    p.extend([f(wp.vig_stops), f(wp.vig_power), f(wp.lens_vig)]);
    let vig = wp.lens.and_then(|l| l.vignette);
    p.push(vig.is_some() as u32);
    let v = vig.unwrap_or_default();
    p.extend(v.k.map(f));
    p.extend([f(v.center.x), f(v.center.y), f(v.radius)]);
    p.push(wp.per_channel() as u32);
    p.push(wp.has_gain() as u32);
    // `set_lensdb` has already moved the optical centre into the user-oriented
    // frame and removed the embedded profile. Zero strengths disable the model
    // branches entirely (including the normalize/denormalize round trip).
    use lightcraft_pipeline::lensdb::{Distortion, Tca};
    let m = wp.lensdb;
    let c = m.map(|m| m.c).unwrap_or_default();
    let (dist, k) = match c.distortion {
        _ if wp.lensdb_dist == 0.0 || c.distortion.is_identity() => (0, [0.0; 3]),
        Distortion::None => (0, [0.0; 3]),
        Distortion::Poly3 { k1 } => (1, [k1, 0.0, 0.0]),
        Distortion::Poly5 { k1, k2 } => (2, [k1, k2, 0.0]),
        Distortion::Ptlens { a, b, c } => (3, [a, b, c]),
    };
    p.push(dist);
    // The CPU multiplies each distortion coefficient by the strength in f64.
    p.extend(k.map(|k| f(k * wp.lensdb_dist)));
    let (tca, red, blue) = match c.tca {
        _ if wp.lensdb_tca == 0.0 || c.tca.is_identity() => (0, [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        Tca::None => (0, [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        Tca::Linear { kr, kb } => (1, [1.0 + (kr - 1.0) * wp.lensdb_tca, 0.0, 0.0], [1.0 + (kb - 1.0) * wp.lensdb_tca, 0.0, 0.0]),
        Tca::Poly3 { red, blue } => (2, red, blue),
    };
    p.push(tca);
    p.extend(red.map(f));
    p.extend(blue.map(f));
    p.push((wp.lensdb_vig != 0.0 && c.vignetting.is_some_and(|k| k != [0.0; 3])) as u32);
    p.extend(c.vignetting.unwrap_or_default().map(f));
    p.extend([f(m.map_or(1.0, |m| m.ns)), f(m.map_or(0.0, |m| m.cx)), f(m.map_or(0.0, |m| m.cy))]);
    p.push(f(wp.lensdb_tca));
    p.push(f(wp.lensdb_vig));
    debug_assert_eq!(p.len(), 85);
    p
}

/// Output rows per block of [`coverage_mask`]; a block is one mask word (32 px) wide. 16 rows measured
/// ~20 % faster than 8 at 6000 × 4000 (fewer interval evaluations, few more edge pixels).
const COVER_ROWS: usize = 16;

/// The reference framing decision ([`Warp::covers`](lightcraft_pipeline::optics::Warp::covers)) for every
/// output pixel, one bit per pixel, rows padded to 32-bit words. Blocks of 32 × [`COVER_ROWS`] pixels whose
/// interval bounds place them clearly inside or outside the image are filled at once
/// ([`Warp::block_coverage`](lightcraft_pipeline::optics::Warp::block_coverage), the same formulas evaluated
/// with outward-rounded intervals); only blocks near the image edge evaluate each pixel. Same bits as
/// evaluating every pixel.
fn coverage_mask(wp: &lightcraft_pipeline::optics::Warp, o2t: &lightcraft_geom::Affine, w: usize, h: usize) -> Vec<u32> {
    let words = w.div_ceil(32);
    let mut coverage = vec![0u32; words * h];
    if words == 0 {
        return coverage;
    }
    lightcraft_raster::par_rows(&mut coverage, words * COVER_ROWS, |band, rows| {
        let y0 = band * COVER_ROWS;
        let y1 = y0 + rows.len() / words;
        for word in 0..words {
            let (x0, x1) = (word * 32, (word * 32 + 32).min(w));
            let full = if x1 - x0 == 32 { u32::MAX } else { (1u32 << (x1 - x0)) - 1 };
            let block = wp.block_coverage(o2t, x0, x1, y0, y1);
            for (y, row) in (y0..y1).zip(rows.chunks_exact_mut(words)) {
                let Some(bits) = row.get_mut(word) else { continue };
                *bits = match block {
                    Some(true) => full,
                    Some(false) => 0,
                    None => (x0..x1).filter(|&x| wp.covers(o2t, x, y)).fold(0, |b, x| b | 1 << (x - x0)),
                };
            }
        }
    });
    coverage
}

/// Resample the source into the output frame (`Frame::sample`). May return the source buffer
/// itself when the frame is the identity.
fn sample(cx: &mut Cx<'_>, src: &Arc<Rgb32f>, src_buf: Arc<Buf>, plan: &Plan<'_>) -> Arc<Buf> {
    let (w, h) = (plan.w, plan.h);
    let fr = &plan.frame;
    let sp = fr.sample_plan(src.width, src.height, w, h);
    let oriented = if fr.orient == Orientation::Normal {
        src_buf
    } else {
        let out = cx.gpu.buffer(sp.ow * sp.oh * 3);
        let mut p = vec![src.width as u32, src.height as u32, sp.ow as u32, sp.oh as u32];
        p.extend(orient_map(fr.orient, src.width, src.height).map(|v| v as u32));
        cx.run("orient", &p, &[Some(&src_buf), Some(&out), None], groups2(sp.ow, sp.oh, [16, 16]));
        Arc::new(out)
    };
    let (base, (bw, bh)) = match sp.prefilter {
        Some(d) => (Arc::new(resize(cx, &oriented, (sp.ow, sp.oh), d, 3, Filter::Mitchell)), d),
        None => (oriented, (sp.ow, sp.oh)),
    };
    let mut affine = |xf: &lightcraft_geom::Affine| {
        let out = cx.gpu.buffer(w * h * 3);
        let mut p = vec![bw as u32, bh as u32, w as u32, h as u32];
        p.extend(affine_bits(xf));
        cx.run("sample_affine", &p, &[Some(&base), Some(&out), None], groups2(w, h, [16, 16]));
        Arc::new(out)
    };
    match (&sp.mode, fr.warp.as_ref()) {
        (SampleMode::Copy, _) => base,
        (SampleMode::Affine(xf), _) => affine(xf),
        // `sample_plan` plans a warp only when there is one; without it the same mapping is affine
        (SampleMode::Warp(o2t), None) => affine(&(lightcraft_geom::Affine::scale(sp.sx, sp.sy) * *o2t)),
        (SampleMode::Warp(o2t), Some(wp)) => {
            let out = cx.gpu.buffer(w * h * 3);
            let p = warp_params(wp, (bw, bh), (w, h), o2t, sp.sx, sp.sy);
            // f32 rounding can cross a source edge. Keep the reference f64
            // framing decision in a bit mask; color sampling stays on the GPU.
            let coverage = cx.gpu.upload(&coverage_mask(wp, o2t, w, h));
            cx.run("sample_warp", &p, &[Some(&base), Some(&out), Some(&coverage)], groups2(w, h, [16, 16]));
            Arc::new(out)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The render

/// CPU copies made along the way (for stages that still run on the CPU).
#[derive(Default)]
struct Host {
    sampled: Option<Rgb32f>,
    lin: Option<Arc<Rgb32f>>,
    log_l: Option<Arc<Plane>>,
}

pub(crate) fn profiling() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LIGHTCRAFT_PROFILE").is_some())
}

/// Render on `gpu`, reusing `stages` (if given). `None` (or a result the caller discards) when the
/// render failed: the reason is recorded with [`crate::ctx::fail`].
#[allow(clippy::too_many_arguments)]
pub fn render(
    gpu: &Gpu,
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    s: &DevelopSettings,
    req: &RenderRequest,
    stages: Option<&GpuStages>,
    fault: Option<crate::Fault>,
    method: lightcraft_pipeline::primary::HsMethod,
) -> Option<Rendered> {
    let mut t = profiling().then(std::time::Instant::now);
    // Each stage is submitted on its own; under `LIGHTCRAFT_PROFILE` also waited for, so the
    // timings are real.
    let lap = |what: &str, t: &mut Option<std::time::Instant>, cx: &mut Cx<'_>| match t {
        Some(t) => {
            cx.sync();
            eprintln!("  gpu {what}: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
            *t = std::time::Instant::now();
        }
        None => cx.flush(),
    };
    let _limit = match fault {
        Some(crate::Fault::Limit(bytes)) => Some(crate::ctx::LimitOverride::new(bytes)),
        _ => None,
    };
    let capture = lightcraft_pipeline::capture_params(info, s).filter(|_| src.width >= 9 && src.height >= 9);
    if capture.is_some() && !gpu.fits(src.data.len() * 3) {
        fail(FailKind::Limit, "capture sharpening needs a source buffer over the device's storage-buffer limit".into());
        return None;
    }
    let mut cx = Cx::new(gpu);
    // The CPU plans automatic Upright and detects pupils AFTER capture. These
    // uncommon source analyses need the same sharpened pixels; ordinary renders
    // plan from dimensions/settings alone and leave the source entirely on device.
    let needs_analysis = !s.red_eye.is_empty()
        || (s.section_enabled("geometry")
            && s.geometry.upright_transform.is_none()
            && !matches!(s.geometry.upright, lightcraft_develop::Upright::Off | lightcraft_develop::Upright::Guided));
    if needs_analysis {
        crate::record_cpu_stage("source analysis");
    }
    let analysis_buf = (capture.is_some() && needs_analysis).then(|| source_buffer(&mut cx, src, capture, stages));
    let mut analysis_src = analysis_buf.as_ref().map(|buf| cx.read_rgb(buf, src.width, src.height));
    if crate::ctx::failed() {
        return None;
    }
    let mut plan = lightcraft_pipeline::plan(analysis_src.as_ref().unwrap_or(src), info, s, req);
    let (w, h) = (plan.w, plan.h);
    // checked: an absurd request (e.g. a fit into `usize::MAX`) must not wrap into a size that
    // passes the check and then dispatches bands forever
    let Some(n) = w.checked_mul(h).filter(|n| n.checked_mul(12).is_some() && gpu.fits(n * 3)) else {
        let mib = (w as u128).saturating_mul(h as u128).saturating_mul(12).div_ceil(1 << 20);
        fail(FailKind::Limit, format!("{w}×{h} needs {mib} MiB buffers, over the device's {} MiB storage-buffer limit", gpu.limit() >> 20));
        return None;
    };
    let cached = stages.and_then(|c| c.get(src, plan.geo, capture));
    let mut host = Host::default();
    let mut uploaded_source = analysis_buf;
    lap("plan", &mut t, &mut cx);

    // 1. capture on the uploaded sensor-scale source, then geometry.
    let sampled = match &cached {
        Some(e) => e.sampled.clone(),
        None if gpu.fits(src.data.len() * 3) && plan.frame.gpu_samplable() => {
            let src_buf = uploaded_source.get_or_insert_with(|| source_buffer(&mut cx, src, capture, stages)).clone();
            lap("capture/upload", &mut t, &mut cx);
            sample(&mut cx, src, src_buf, &plan)
        }
        None => {
            crate::record_cpu_stage("geometry");
            // A source over the device limit is prefiltered/resampled on the CPU.
            if capture.is_some() && analysis_src.is_none() {
                let buf = uploaded_source.get_or_insert_with(|| source_buffer(&mut cx, src, capture, stages));
                analysis_src = Some(cx.read_rgb(buf, src.width, src.height));
            }
            let img = plan.frame.sample(analysis_src.as_ref().unwrap_or(src), w, h);
            let b = Arc::new(gpu.upload(rgb_words(&img)));
            host.sampled = Some(img);
            b
        }
    };
    lap("sample", &mut t, &mut cx);

    // 2. white balance, defringe, spots, noise reduction
    let lin = match cached.as_ref().and_then(|e| e.lin.clone()).filter(|(k, _)| *k == plan.lin_key) {
        Some((_, b)) => b,
        None => Arc::new(linear(&mut cx, &sampled, info, &plan, &mut host)),
    };
    lap("wb/nr", &mut t, &mut cx);

    // Native primary compute passes, with independent caches for fields and slider parameters.
    let mut native_primary = cached.as_ref().map(|e| e.native_primary.clone()).unwrap_or_default();
    let primary_key = lightcraft_pipeline::primary::key_for(&plan, info, method);
    let primary = if lightcraft_pipeline::primary::active(&plan.settings) {
        match cached.as_ref().and_then(|e| e.primary.clone()).filter(|p| p.0 == primary_key) {
            Some(p) => Some(p),
            None => {
                let proxy = if lightcraft_pipeline::primary::needs_proxy(&plan.settings) {
                    let pp = lightcraft_pipeline::primary::proxy_plan(analysis_src.as_ref().unwrap_or(src), &plan);
                    let pk = plan.lin_key;
                    let b = native_primary.proxy(pk, || {
                        let sampled = if gpu.fits(src.data.len() * 3) && pp.frame.gpu_samplable() {
                            // The proxy must see the same device capture result as the full image.
                            let source = uploaded_source.get_or_insert_with(|| source_buffer(&mut cx, src, capture, stages)).clone();
                            sample(&mut cx, src, source, &pp)
                        } else {
                            crate::record_cpu_stage("geometry");
                            if capture.is_some() && analysis_src.is_none() {
                                let source = uploaded_source.get_or_insert_with(|| source_buffer(&mut cx, src, capture, stages));
                                analysis_src = Some(cx.read_rgb(source, src.width, src.height));
                            }
                            Arc::new(gpu.upload(rgb_words(&pp.frame.sample(analysis_src.as_ref().unwrap_or(src), pp.w, pp.h))))
                        };
                        linear(&mut cx, &sampled, info, &pp, &mut Host::default())
                    });
                    Some((b, pp.w, pp.h))
                } else {
                    None
                };
                let clip = info
                    .clip_confidence
                    .as_ref()
                    .filter(|c| {
                        lightcraft_pipeline::primary::needs_clip(&plan.settings)
                            && c.width > 0
                            && c.height > 0
                            && c.data.len() == c.width * c.height
                            && c.data.iter().any(|v| *v > 0.66)
                    })
                    .map(|c| {
                        native_primary.clip(plan.geo, Arc::as_ptr(c) as usize, || {
                            let metadata = Arc::new(Rgb32f { width: c.width, height: c.height, data: c.data.iter().map(|v| [*v; 3]).collect() });
                            let source = Arc::new(gpu.upload(rgb_words(&metadata)));
                            let sampled = sample(&mut cx, &metadata, source, &plan);
                            crate::primary::clip_plane(&mut cx, &sampled, n)
                        })
                    });
                let alpha = if lightcraft_pipeline::primary::layers_active(&plan.settings) {
                    let l = gpu.buffer(n);
                    map(&mut cx, "log_lum_k", n, &[], [Some(&lin), None, None], &l);
                    let l = Arc::new(l);
                    let prep = Prep { log_l: l.clone(), base: l, clarity: None, texture: None, haze: None, sharp: None, chroma: None, tone_eq: None };
                    masks(&mut cx, &lin, &prep, &plan, &mut host, info).0
                } else {
                    None
                };
                let image =
                    crate::primary::process(&mut cx, lin.clone(), proxy, clip.as_deref(), alpha.as_ref(), info, &plan, method, &mut native_primary);
                host.lin = None;
                host.log_l = None;
                Some((primary_key, image))
            }
        }
    } else {
        None
    };
    if let Some(stages) = stages {
        stages.primary_dispatches.store(cx.primary_dispatches, std::sync::atomic::Ordering::Relaxed);
    }
    lap("primary", &mut t, &mut cx);
    if primary.is_some() {
        plan.settings = std::borrow::Cow::Owned(lightcraft_pipeline::primary::remaining(&plan.settings));
    }
    let s = &*plan.settings;
    let prepared_lin = primary.as_ref().map_or_else(|| lin.clone(), |p| p.1.clone());
    let planes_key = if primary.is_some() { primary_key } else { plan.lin_key };
    // 3. spatial planes
    let mut planes = match cached.map(|e| e.planes) {
        Some(p) if p.key == planes_key => p,
        _ => Planes { key: planes_key, ..Default::default() },
    };
    let prep = prepare(&mut cx, &prepared_lin, &plan, req, &mut planes, info.sensor_scale);
    lap("planes", &mut t, &mut cx);
    if let Some(c) = stages {
        c.put(Entry { src: src.clone(), capture, geo: plan.geo, sampled, lin: Some((plan.lin_key, lin.clone())), planes, primary, native_primary });
    }

    // 4. masks
    let (masks, terms) = masks(&mut cx, &prepared_lin, &prep, &plan, &mut host, info);
    let teq_plane = n * terms.len() + if prep.chroma.is_some() { 3 * n } else { 0 };
    let masks = if let Some(teq) = &prep.tone_eq {
        let joined = cx.zeroed(teq_plane + n);
        if let Some(m) = masks {
            cx.copy_into(&m, &joined, 0);
        }
        cx.copy_into(teq, &joined, teq_plane);
        Some(joined)
    } else {
        masks
    };
    lap("masks", &mut t, &mut cx);

    // 5. per-pixel stage
    let mut fp = FinishParams::new(s, &plan.frame, info, w, h, plan.px_per_long, req.space).with_planes(prep.sharp.as_ref().map(|v| v.2), None);
    // Reuse the existing texture binding for [texture | sharp B1 | sharp B2].
    let texture = if let Some((b1, b2, _)) = &prep.sharp {
        let buf = gpu.buffer(3 * n);
        if let Some(tex) = &prep.texture {
            cx.copy_into(tex, &buf, 0);
        }
        cx.copy_into(b1, &buf, n);
        cx.copy_into(b2, &buf, 2 * n);
        Some(buf)
    } else {
        None
    };
    let present = Present {
        clarity: prep.clarity.is_some(),
        texture: prep.texture.is_some(),
        dark: prep.haze.is_some(),
        sharp: prep.sharp.is_some(),
        chroma: masks.is_some() && prep.chroma.is_some(),
    };
    let resolved_layers = std::mem::take(&mut fp.layers);
    let (mut p, mut aux) = finish_block(&fp, &terms, &present);
    let air_plane = masks.as_ref().map_or(0, |m| m.len);
    let masks = if let Some((_, air)) = &prep.haze {
        let joined = cx.gpu.buffer(air_plane + 4);
        if let Some(masks) = masks {
            cx.copy_into(&masks, &joined, 0);
        }
        cx.copy_into(air, &joined, air_plane);
        Some(joined)
    } else {
        masks
    };
    if let Some(i) = crate::params::index("AIR_PLANE") {
        p[i] = air_plane as u32;
    }
    let masks = layer_buffers(&mut cx, &prepared_lin, &prep, masks, &plan, info, req, resolved_layers, &present, &mut p, &mut aux);
    if let Some(i) = crate::params::index("TEQ_PLANE") {
        p[i] = teq_plane as u32;
    }
    let aux = gpu.upload(&aux);
    // cleared, so that work which never ran cannot pass for an image (see the check below)
    let out = cx.zeroed(n);
    if fault == Some(crate::Fault::Validation) {
        // a dispatch the device rejects (too many workgroups)
        cx.run("box_h", &[0; 5], &[Some(&aux), Some(&out)], [gpu.device.limits().max_compute_workgroups_per_dimension + 1, 1, 1]);
    }
    // in bands of rows: each dispatch (and submission) stays short on a slow GPU
    let rows = (BAND_PIXELS / w.max(1) / 16 * 16).max(16); // whole workgroups: bands never overlap
    let y0_at = crate::params::index("Y0").unwrap_or(0);
    for y0 in (0..h).step_by(rows).filter(|_| fault != Some(crate::Fault::DropWork)) {
        p[y0_at] = y0 as u32;
        cx.run(
            "main",
            &p,
            &[
                Some(&prepared_lin),
                Some(&prep.log_l),
                Some(&prep.base),
                prep.clarity.as_deref(),
                texture.as_ref().or(prep.texture.as_deref()),
                prep.haze.as_ref().map(|v| &*v.0),
                masks.as_ref(),
                Some(&aux),
                Some(&out),
            ],
            groups2(w, rows.min(h - y0), [16, 16]),
        );
    }
    // the alpha a mask overlay shows: copied out before the readback below submits
    let overlay_mask = if req.overlay == lightcraft_pipeline::Overlay::ToneEqMask {
        prep.tone_eq.as_ref().map(|teq| {
            let out = gpu.buffer(n);
            map(&mut cx, "toneeq_preview", n, &[], [Some(teq), None, None], &out);
            Ok(out)
        })
    } else if req.overlay == lightcraft_pipeline::Overlay::SharpenMask {
        let (b2, sigma) = prep.sharp.as_ref().map_or((&prep.log_l, 0.0), |(_, b2, sg)| (b2, *sg));
        let k = lightcraft_pipeline::detail::SharpK::new(0.0, fp.sharp_masking, sigma);
        let buf = gpu.buffer(n);
        map(&mut cx, "sharp_preview", n, &[w as u32, h as u32, k.mask_t.to_bits(), k.edge_k.to_bits()], [Some(b2), None, None], &buf);
        Some(Ok(buf))
    } else {
        req.overlay.mask(s).map(|m| {
            let list = s.masks.iter().filter(|m| m.visible && !m.components.is_empty());
            match (list.map(|m| m.id).position(|id| id == m.id), masks.as_ref()) {
                (Some(mi), Some(all)) => Ok(cx.slice(all, mi * n, n)),
                _ => Err(m),
            }
        })
    };
    let data: Vec<[u8; 4]> = cx.read(&out, n);
    let image = Rgba8 { width: w, height: h, data };
    lap("finish + readback", &mut t, &mut cx);
    if crate::ctx::failed() {
        return None;
    }
    // The per-pixel kernel writes alpha 255 everywhere and `out` was cleared: a pixel with any
    // other alpha means work silently did not run (a driver that reset or dropped a submission
    // without reporting it — issue #78's all-black exports).
    let unwritten = unwritten(&image.data);
    if unwritten > 0 {
        fail(FailKind::Fatal, format!("the GPU returned an incomplete image ({unwritten} of {n} pixels unwritten)"));
        return None;
    }
    let histogram = Histogram::of_srgb8(&image);
    lap("histogram", &mut t, &mut cx);
    // An entirely black result (upstream work that did not run would give that too): redo it on
    // the CPU, which costs time only for the rare photo that really is black.
    if n >= 4096 && [&histogram.r, &histogram.g, &histogram.b].iter().all(|c| c.first() == Some(&histogram.total)) {
        fail(FailKind::Blank, "the GPU result is entirely black; rendering it on the CPU to be sure".into());
        return None;
    }
    let mut image = image;
    let overlay_mask = overlay_mask.map(|r| match r {
        Ok(b) => cx.read_plane(&b, w, h),
        // a hidden mask: evaluate it on the CPU
        Err(m) => {
            crate::record_cpu_stage("mask overlay");
            let img = host.lin.get_or_insert_with(|| Arc::new(cx.read_rgb(&lin, w, h))).clone();
            let l = host.log_l.get_or_insert_with(|| Arc::new(cx.read_plane(&prep.log_l, w, h))).clone();
            lightcraft_pipeline::masks::evaluate_one(m, &plan.frame, w, h, &img, &l, s.light.exposure as f32)
        }
    });
    lightcraft_pipeline::visualize::apply(&mut image, req.overlay, &plan, overlay_mask.as_ref());
    Some(Rendered { image, histogram, deep: None })
}

fn source_buffer(cx: &mut Cx<'_>, src: &Arc<Rgb32f>, capture: Option<CaptureParams>, stages: Option<&GpuStages>) -> Arc<Buf> {
    match stages {
        Some(c) => c.source(cx, src, capture),
        None => {
            let uploaded = cx.gpu.upload(rgb_words(src));
            Arc::new(match capture {
                Some(p) => crate::capture::sharpen(cx, &uploaded, src.width, src.height, &p),
                None => uploaded,
            })
        }
    }
}

/// Pixels whose alpha is not 255 (cheap: no early exit, so it vectorizes).
fn unwritten(data: &[[u8; 4]]) -> usize {
    data.chunks(4096).filter(|c| c.iter().fold(255u8, |a, p| a & p[3]) != 255).map(|c| c.iter().filter(|p| p[3] != 255).count()).sum()
}

/// The white-balanced, retouched, denoised image.
fn linear(cx: &mut Cx<'_>, sampled: &Buf, info: &SourceInfo, plan: &Plan<'_>, host: &mut Host) -> Buf {
    let (w, h) = (plan.w, plan.h);
    let n = w * h;
    let s = &*plan.settings;
    let img = if lightcraft_pipeline::lin_needs_cpu(s) || info.camera_profile.is_some() {
        crate::record_cpu_stage("linear stage");
        // film negative conversion / defringe / spot removal: CPU
        let mut img = match host.sampled.take() {
            Some(i) => i,
            None => cx.read_rgb(sampled, w, h),
        };
        lightcraft_pipeline::lin_cpu(&mut img, info, plan);
        cx.gpu.upload(rgb_words(&img))
    } else {
        let patched = patch_stage(cx, sampled, plan);
        let sampled = patched.as_ref().unwrap_or(sampled);
        let m = local::wb_matrix_for(info, s);
        let mut p = vec![m.is_some() as u32];
        p.extend(m.unwrap_or_default().iter().flatten().map(|v| v.to_bits()));
        let out = cx.gpu.buffer(n * 3);
        map(cx, "wb_k", n, &p, [Some(sampled), None, None], &out);
        let out = if let Some(cal) = lightcraft_pipeline::colorcal::of(info, s).filter(|c| !c.is_linear()) {
            let calibrated = cx.gpu.buffer(n * 3);
            map(cx, "colour_cal", n, &cal.gpu_words(), [Some(&out), None, None], &calibrated);
            calibrated
        } else {
            out
        };
        if plan.eyes.is_empty() {
            out
        } else {
            let mut p = vec![w as u32, plan.eyes.len() as u32];
            p.extend(plan.eyes.iter().flat_map(|e| e.words()).map(f32::to_bits));
            let eyed = cx.gpu.buffer(n * 3);
            map(cx, "redeye_k", n, &p, [Some(&out), None, None], &eyed);
            eyed
        }
    };
    denoise(cx, img, plan, info.sensor_scale)
}

/// Native wavelet NR, including robust noise estimation and recursive band statistics.
fn denoise(cx: &mut Cx<'_>, img: Buf, plan: &Plan<'_>, sensor_scale: f32) -> Buf {
    use lightcraft_pipeline::detail::nr::*;
    let (w, h) = (plan.w, plan.h);
    let scale = lightcraft_pipeline::detail::out_per_orig(plan.px_per_long, plan.src_long, sensor_scale);
    let Some(p) = nr_params(&plan.settings, scale) else { return img };
    denoise_native(cx, img, w, h, &p)
}
fn denoise_native(cx: &mut Cx<'_>, img: Buf, w: usize, h: usize, p: &lightcraft_pipeline::detail::nr::NrParams) -> Buf {
    use lightcraft_pipeline::detail::nr::*;
    let n = w * h;
    let samples = cx.gpu.buffer(16 * 16384);
    map(cx, "nr_samples", 16 * 16384, &[w as u32, h as u32], [Some(&img), None, None], &samples);
    let slopes = cx.gpu.buffer(16);
    cx.run("nr_median", &[], &[Some(&samples), None, None, Some(&slopes)], [16, 1, 1]);
    let vst = cx.gpu.buffer(24);
    let (to, from) = conversion_matrices([1.0; 3]);
    let mut metadata = vec![p.scale.to_bits(), p.lum.max(p.col).to_bits()];
    metadata.extend(to.into_iter().flatten().chain(from.into_iter().flatten()).map(f32::to_bits));
    map(cx, "nr_vst", 1, &metadata, [Some(&slopes), None, None], &vst);
    let mut coarse = cx.gpu.buffer(n * 3);
    map(cx, "nr_forward", n, &[], [Some(&img), Some(&vst), None], &coarse);
    let mut acc = cx.zeroed(n * 3);
    let levels = max_scale(w, h, p.scale);
    for level in 0..levels {
        let next = cx.gpu.buffer(n * 3);
        map(cx, "nr_eaw", n, &[w as u32, h as u32, level as u32, (1.0 / VARF.powi(2 * level as i32)).to_bits()], [Some(&coarse), None, None], &next);
        let det = cx.gpu.buffer(n * 3);
        map(cx, "nr_detail", n, &[], [Some(&coarse), Some(&next), None], &det);
        let groups = groups1(n);
        let count = groups[0] as usize * groups[1] as usize;
        let sums = cx.gpu.buffer(count * 3);
        map(cx, "nr_sum", n, &[], [Some(&det), None, None], &sums);
        let mut count = count;
        let mut sums = sums;
        while count > 1 {
            let groups = groups1(count);
            let next_count = (groups[0] * groups[1]) as usize;
            let reduced = cx.gpu.buffer(next_count * 3);
            map(cx, "nr_reduce", count, &[], [Some(&sums), None, None], &reduced);
            sums = reduced;
            count = next_count;
        }
        let thresholds = cx.gpu.buffer(3);
        let mut block = vec![n as u32, VARF.powi(level as i32).powi(2).to_bits()];
        block.extend(band_force(p, level, levels).map(f32::to_bits));
        map(cx, "nr_thresholds", 1, &block, [Some(&sums), None, None], &thresholds);
        let out = cx.gpu.buffer(n * 3);
        map(cx, "nr_synthesize", n, &[], [Some(&acc), Some(&det), Some(&thresholds)], &out);
        acc = out;
        coarse = next;
    }
    let joined = cx.gpu.buffer(n * 3);
    map(cx, "nr_residual", n, &[], [Some(&acc), Some(&coarse), None], &joined);
    let back = cx.gpu.buffer(n * 3);
    map(cx, "nr_backward", n, &[], [Some(&joined), Some(&vst), None], &back);
    let out = cx.gpu.buffer(n * 3);
    map(cx, "nr_join", n, &[(p.lum > 0.0) as u32, (p.col > 0.0) as u32], [Some(&img), Some(&back), None], &out);
    out
}

/// The planes the per-pixel stage reads.
struct Prep {
    log_l: Arc<Buf>,
    base: Arc<Buf>,
    clarity: Option<Arc<Buf>>,
    texture: Option<Arc<Buf>>,
    haze: Option<(Arc<Buf>, Arc<Buf>)>,
    sharp: Option<(Arc<Buf>, Arc<Buf>, f32)>,
    /// Blurred chromaticity (local Moiré / Noise).
    chroma: Option<Arc<Buf>>,
    tone_eq: Option<Arc<Buf>>,
}

fn plane_at(slot: &mut Option<(u32, Arc<Buf>)>, sigma: f32, f: impl FnOnce() -> Buf) -> Arc<Buf> {
    match slot {
        Some((k, p)) if *k == sigma.to_bits() => p.clone(),
        _ => {
            let p = Arc::new(f());
            *slot = Some((sigma.to_bits(), p.clone()));
            p
        }
    }
}

/// The spatial planes (`local::prepare`), reusing those in `planes`.
fn prepare(cx: &mut Cx<'_>, lin: &Buf, plan: &Plan<'_>, req: &RenderRequest, planes: &mut Planes, sensor_scale: f32) -> Prep {
    let (w, h) = (plan.w, plan.h);
    let n = w * h;
    let scale = lightcraft_pipeline::detail::out_per_orig(plan.px_per_long, plan.src_long, sensor_scale);
    let sig = local::plane_sigmas(&plan.settings, plan.px_per_long, scale, req.quality);
    let log_l = match &planes.log_l {
        Some(b) => b.clone(),
        None => {
            let l = cx.gpu.buffer(n);
            map(cx, "log_lum_k", n, &[], [Some(lin), None, None], &l);
            let b = Arc::new(l);
            planes.log_l = Some(b.clone());
            b
        }
    };
    let base = match sig.base {
        Some(sg) => plane_at(&mut planes.base, sg, || guided_fast(cx, &log_l, w, h, sg, local::BASE_EPS)),
        None => log_l.clone(),
    };
    let clarity = sig.clarity.map(|sg| plane_at(&mut planes.clarity, sg, || guided_fast(cx, &log_l, w, h, sg, local::CLARITY_EPS)));
    let texture = sig.texture.map(|sg| plane_at(&mut planes.texture, sg, || gaussian(cx, &log_l, w, h, 1, sg)));
    let sharp = sig.sharp.map(|sigma| match &planes.sharp {
        Some((key, b1, b2)) if *key == sigma.to_bits() => (b1.clone(), b2.clone(), sigma),
        _ => {
            let b1 = Arc::new(gaussian_fine(cx, &log_l, w, h, 1, sigma));
            let b2 = Arc::new(gaussian_fine(cx, &b1, w, h, 1, sigma));
            planes.sharp = Some((sigma.to_bits(), b1.clone(), b2.clone()));
            (b1, b2, sigma)
        }
    });
    let haze = sig.haze.then(|| planes.haze.get_or_insert_with(|| haze_planes(cx, lin, w, h, scale)).clone());
    let chroma = sig.chroma.map(|sg| {
        plane_at(&mut planes.chroma, sg, || {
            let ch = cx.gpu.buffer(n * 3);
            map(cx, "chroma_k", n, &[], [Some(lin), None, None], &ch);
            gaussian(cx, &ch, w, h, 3, sg)
        })
    });
    let tone_eq = lightcraft_pipeline::toneeq::mask_wanted(&plan.settings).then(|| {
        let s = &plan.settings;
        let (sigma, eps) = lightcraft_pipeline::toneeq::mask_sigma_eps(s, plan.px_per_long);
        let key = cache_key((
            sigma.to_bits(),
            eps.to_bits(),
            s.light.exposure.to_bits(),
            s.tone_eq.mask_exposure.to_bits(),
            s.tone_eq.mask_contrast.to_bits(),
        ));
        if let Some((k, b)) = &planes.tone_eq
            && *k == key
        {
            return b.clone();
        }
        let lum = cx.gpu.buffer(n);
        let gain = ((s.light.exposure + s.tone_eq.mask_exposure) as f32).exp2();
        let slope = (s.tone_eq.mask_contrast as f32).exp2();
        map(cx, "toneeq_lum", n, &[gain.to_bits(), slope.to_bits()], [Some(lin), None, None], &lum);
        let filtered = crate::primary::toneeq_eigf(cx, lin, &lum, w, h, sigma, eps, gain, slope);
        let log = cx.gpu.buffer(n);
        map(cx, "toneeq_log", n, &[], [Some(&filtered), None, None], &log);
        let log = Arc::new(log);
        planes.tone_eq = Some((key, log.clone()));
        log
    });
    Prep { log_l, base, clarity, texture, chroma, sharp, haze, tone_eq }
}

/// Most brush dabs the mask kernel evaluates per pixel (more: the CPU rasterizes the brush).
const MAX_DABS: usize = 4096;

/// The `shape` kernel's brush data for `dabs`: stroke records (12 words: dab offset, dab count,
/// r, hard, flow, density, erase, bbox x0 y0 x1 y1, auto) then the dab centres.
fn brush_aux(dabs: &[&lightcraft_pipeline::masks::BrushDabs], erase: bool) -> Vec<f32> {
    let mut aux = Vec::new();
    let mut off = 12 * dabs.len();
    for d in dabs {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for q in &d.dabs {
            (x0, y0, x1, y1) = (x0.min(q.x - d.r), y0.min(q.y - d.r), x1.max(q.x + d.r), y1.max(q.y + d.r));
        }
        aux.extend([off as f32, d.dabs.len() as f32, d.r as f32, d.hard as f32, d.flow, d.density, (erase && d.erase) as u8 as f32]);
        aux.extend([x0, y0, x1, y1].map(|v| v as f32));
        aux.push(d.auto as u8 as f32);
        off += 2 * d.dabs.len();
    }
    for d in dabs {
        aux.extend(d.dabs.iter().flat_map(|q| [q.x as f32, q.y as f32]));
    }
    aux
}

/// Cross guided filter (`masks::guided_cross`) of `p` steered by `guide`, max-combined with `p`.
fn guided_cross_max(cx: &mut Cx<'_>, guide: &Buf, p: &Buf, w: usize, h: usize, sigma: f32, eps: f32) -> Buf {
    let n = w * h;
    let (m0, m1) = (cx.gpu.buffer(2 * n), cx.gpu.buffer(2 * n));
    map(cx, "xguided_pre", n, &[0], [Some(guide), Some(p), None], &m0);
    map(cx, "xguided_pre", n, &[1], [Some(guide), Some(p), None], &m1);
    let (b0, b1) = (gaussian(cx, &m0, w, h, 2, sigma), gaussian(cx, &m1, w, h, 2, sigma));
    let ab = cx.gpu.buffer(2 * n);
    map(cx, "xguided_ab", n, &[eps.to_bits()], [Some(&b0), Some(&b1), None], &ab);
    let ab = gaussian(cx, &ab, w, h, 2, sigma);
    let q = cx.gpu.buffer(n);
    map(cx, "xguided_apply", n, &[], [Some(guide), Some(&ab), Some(p)], &q);
    q
}

/// Evaluate the masks (`lightcraft_pipeline::masks::evaluate`): their alpha planes (concatenated,
/// followed by the blurred chromaticity when local Moiré / Noise need it) and their adjustment
/// terms. Shapes without a kernel (Sky, Subject, …) run on the CPU.
fn masks(cx: &mut Cx<'_>, lin: &Buf, prep: &Prep, plan: &Plan<'_>, host: &mut Host, info: &SourceInfo) -> (Option<Buf>, Vec<[f32; MASK_TERMS]>) {
    use lightcraft_develop::{MaskOp, MaskShape};
    let s = &*plan.settings;
    let list: Vec<_> = s.masks.iter().filter(|m| m.visible && !m.components.is_empty()).collect();
    if list.is_empty() {
        return (None, Vec::new());
    }
    let (w, h) = (plan.w, plan.h);
    let n = w * h;
    let frame = &plan.frame;
    let ev = s.light.exposure as f32;
    let long = frame.ow.max(frame.oh);
    let [a, b, c, d, e, f] = frame.out_to_norm(w, h).0;
    let (kx, ky) = (frame.ow / long, frame.oh / long);
    let aff = [a * kx, b * ky, c * kx, d * ky, e * kx, f * ky].map(|v| (v as f32).to_bits());
    let alpha = cx.zeroed(n * list.len() + if prep.chroma.is_some() { 3 * n } else { 0 });
    if let Some(ch) = &prep.chroma {
        cx.copy_into(ch, &alpha, n * list.len());
    }
    let comp_plane = cx.gpu.buffer(n);
    let mut selection: Option<Buf> = None;
    for (mi, m) in list.iter().enumerate() {
        let mut first = true;
        for comp in &m.components {
            let op = match comp.op {
                _ if first && comp.op != MaskOp::Intersect => 0u32,
                MaskOp::Add => 1,
                MaskOp::Subtract => 2,
                MaskOp::Intersect => 3,
            };
            first = false;
            let mut p = vec![w as u32, h as u32, 0, comp.invert as u32];
            p.extend_from_slice(&aff);
            let f32s = |v: &[f64]| v.iter().map(|x| (*x as f32).to_bits()).collect::<Vec<u32>>();
            let mut aux: Vec<f32> = Vec::new();
            let mut own: Option<Buf> = None;
            let kind = match &comp.shape {
                MaskShape::Linear { start, end } => {
                    let (a, b) = (frame.norm_to_long(*start), frame.norm_to_long(*end));
                    let d = b - a;
                    p.extend(f32s(&[a.x, a.y, d.x, d.y, d.dot(d).max(1e-12)]));
                    Some(0)
                }
                MaskShape::Radial { center, rx, ry, angle, feather, invert } => {
                    let c = frame.norm_to_long(*center);
                    let (sn, co) = (-angle.to_radians()).sin_cos();
                    p.extend(f32s(&[c.x, c.y, co, sn, rx.max(1e-6), ry.max(1e-6), (*feather / 100.0).clamp(0.0, 1.0)]));
                    p.push(*invert as u32);
                    Some(1)
                }
                MaskShape::LuminanceRange { lo, hi, lo_feather, hi_feather } => {
                    p.extend(f32s(&[*lo, *hi, lo_feather.max(1e-3), hi_feather.max(1e-3)]));
                    p.push(ev.to_bits());
                    Some(2)
                }
                MaskShape::ColorRange { samples, refine } => {
                    if selection.is_none() {
                        let mut fp = FinishParams::new(s, frame, info, w, h, plan.px_per_long, lightcraft_pipeline::OutputSpace::Srgb);
                        fp.tone = lightcraft_pipeline::tone2::tone_map(s, info, s.light.contrast, 0., 0.);
                        let present = Present { clarity: false, texture: false, dark: false, sharp: false, chroma: false };
                        let (block, tab) = finish_block(&fp, &[], &present);
                        let tab = cx.gpu.upload(&tab);
                        let out = cx.gpu.buffer(n * 3);
                        cx.run(
                            "select_colour",
                            &block,
                            &[Some(lin), None, None, None, None, None, None, Some(&tab), Some(&out)],
                            groups2(w, h, [16, 16]),
                        );
                        selection = Some(out);
                    }
                    p.extend([(0.04 + 0.16 * (*refine as f32 / 100.)).to_bits(), 1f32.to_bits(), samples.len() as u32, 1]);
                    aux = samples.iter().flat_map(|s| s.map(|v| v as f32)).collect();
                    Some(3)
                }
                MaskShape::DepthRange { lo, hi, feather, seg } => {
                    let grid = seg.as_ref().and_then(|s| Some((s.side as usize, s.bounds()?, s.logits()?)));
                    // Depth uses normalized transformed coordinates, not long-edge units.
                    for (dst, v) in p[4..10].iter_mut().zip(frame.out_to_norm(w, h).0) {
                        *dst = (v as f32).to_bits();
                    }
                    if let Some((side, bounds, logits)) = grid {
                        p.push(side as u32);
                        p.extend(f32s(&bounds));
                        aux = logits;
                    } else {
                        p.extend([0; 5]);
                    }
                    p.extend([lo.min(*hi) as f32, lo.max(*hi) as f32, (*feather as f32).max(1e-3)].map(f32::to_bits));
                    Some(5)
                }
                MaskShape::Brush { strokes } => {
                    let dabs: Vec<_> = strokes.iter().map(|st| lightcraft_pipeline::masks::brush_dabs(st, frame, w, h)).collect();
                    if dabs.iter().map(|d| d.dabs.len()).sum::<usize>() > MAX_DABS {
                        None
                    } else if dabs.iter().any(|d| d.auto) {
                        // Auto Mask: stroke by stroke (each one refined before it's combined)
                        let brush = cx.zeroed(n);
                        let stroke = cx.gpu.buffer(n);
                        for d in &dabs {
                            let mut q = p.clone();
                            q[2] = 4;
                            q[3] = 0;
                            q.push(1);
                            let aux = cx.gpu.upload(&brush_aux(&[d], false));
                            cx.run("shape", &q, &[Some(lin), Some(&prep.log_l), Some(&aux), Some(&stroke), Some(&brush)], groups2(w, h, [16, 16]));
                            let refined = d.auto.then(|| {
                                let (sigma, eps) = lightcraft_pipeline::masks::auto_refine(d.r);
                                guided_cross_max(cx, &prep.log_l, &stroke, w, h, sigma, eps)
                            });
                            let op = if d.erase { 2 } else { 1 };
                            cx.run(
                                "combine",
                                &[n as u32, 0, 0, op],
                                &[None, None, None, Some(refined.as_ref().unwrap_or(&stroke)), Some(&brush)],
                                groups1(n),
                            );
                        }
                        if comp.invert {
                            cx.run("finalize", &[n as u32, 0, 0, 1, 1f32.to_bits()], &[None, None, None, Some(&stroke), Some(&brush)], groups1(n));
                        }
                        own = Some(brush);
                        None
                    } else {
                        p.push(dabs.len() as u32);
                        aux = brush_aux(&dabs.iter().collect::<Vec<_>>(), true);
                        Some(4)
                    }
                }
                _ => None,
            };
            let plane = match kind {
                _ if own.is_some() => own,
                Some(k) => {
                    p[2] = k;
                    let aux = (!aux.is_empty()).then(|| cx.gpu.upload(&aux));
                    cx.run(
                        "shape",
                        &p,
                        &[if k == 3 { selection.as_ref() } else { Some(lin) }, Some(&prep.log_l), aux.as_ref(), Some(&comp_plane), Some(&alpha)],
                        groups2(w, h, [16, 16]),
                    );
                    None
                }
                None => {
                    crate::record_cpu_stage("mask shape");
                    // no kernel: evaluate on the CPU
                    let img = host.lin.get_or_insert_with(|| Arc::new(cx.read_rgb(lin, w, h))).clone();
                    let l = host.log_l.get_or_insert_with(|| Arc::new(cx.read_plane(&prep.log_l, w, h))).clone();
                    let mut v = lightcraft_pipeline::masks::shape_alpha_tone(
                        &comp.shape,
                        frame,
                        w,
                        h,
                        &img,
                        &l,
                        ev,
                        Some(&FinishParams::new(s, frame, info, w, h, plan.px_per_long, lightcraft_pipeline::OutputSpace::Srgb).tone),
                    );
                    if comp.invert {
                        v.data.iter_mut().for_each(|x| *x = 1.0 - *x);
                    }
                    Some(cx.gpu.upload(&v.data))
                }
            };
            let cbuf = plane.as_ref().unwrap_or(&comp_plane);
            let k = [n as u32, 0, mi as u32, op];
            cx.run("combine", &k, &[None, None, None, Some(cbuf), Some(&alpha)], groups1(n));
        }
        if m.refine > 0. {
            let k = (m.refine as f32 / 100.).clamp(0., 1.);
            let input = cx.slice(&alpha, mi * n, n);
            let sigma = (0.04 * w.max(h) as f32 * k).max(1.);
            let (m0, m1) = (cx.gpu.buffer(n * 2), cx.gpu.buffer(n * 2));
            map(cx, "xguided_pre", n, &[0], [Some(&prep.log_l), Some(&input), None], &m0);
            map(cx, "xguided_pre", n, &[1], [Some(&prep.log_l), Some(&input), None], &m1);
            let b0 = gaussian(cx, &m0, w, h, 2, sigma);
            let b1 = gaussian(cx, &m1, w, h, 2, sigma);
            let ab = cx.gpu.buffer(n * 2);
            map(cx, "xguided_ab", n, &[0.02f32.to_bits()], [Some(&b0), Some(&b1), None], &ab);
            let ab = gaussian(cx, &ab, w, h, 2, sigma);
            let refined = cx.gpu.buffer(n);
            map(cx, "xguided_refine", n, &[k.sqrt().to_bits()], [Some(&prep.log_l), Some(&ab), Some(&input)], &refined);
            cx.copy_into(&refined, &alpha, mi * n);
        }
        let amt = lightcraft_pipeline::masks::mask_scale(m);
        cx.run(
            "finalize",
            &[n as u32, 0, mi as u32, m.invert as u32, amt.to_bits()],
            &[None, None, None, Some(&comp_plane), Some(&alpha)],
            groups1(n),
        );
    }
    let terms = list.iter().map(|m| mask_terms(&m.adjust)).collect();
    (Some(alpha), terms)
}

/// Exact small-radius Gaussian used by pixel-scale sharpening.
fn gaussian_fine(cx: &mut Cx<'_>, src: &Buf, w: usize, h: usize, nc: usize, sigma: f32) -> Buf {
    use lightcraft_raster::blur::{FINE_SIGMA, gauss_taps};
    if sigma >= FINE_SIGMA {
        return gaussian(cx, src, w, h, nc, sigma);
    }
    if sigma <= 0.05 {
        return cx.copy(src);
    }
    let taps = gauss_taps(sigma);
    let mut p = vec![w as u32, h as u32, nc as u32, (taps.len() - 1) as u32];
    p.extend(taps.iter().map(|v| v.to_bits()));
    let tmp = cx.gpu.buffer(w * h * nc);
    let out = cx.gpu.buffer(w * h * nc);
    cx.run("conv_h", &p, &[Some(src), Some(&tmp)], groups2(w, h, [16, 16]));
    cx.run("conv_v", &p, &[Some(&tmp), Some(&out)], groups2(w, h, [16, 16]));
    out
}
fn extrema(cx: &mut Cx<'_>, src: &Buf, w: usize, h: usize, r: usize, max: bool) -> Buf {
    let tmp = cx.gpu.buffer(w * h);
    let out = cx.gpu.buffer(w * h);
    cx.run("extrema", &[w as u32, h as u32, r as u32, max as u32, 0], &[Some(src), Some(&tmp)], groups2(w, h, [16, 16]));
    cx.run("extrema", &[w as u32, h as u32, r as u32, max as u32, 1], &[Some(&tmp), Some(&out)], groups2(w, h, [16, 16]));
    out
}
fn box_mean(cx: &mut Cx<'_>, src: &Buf, w: usize, h: usize, nc: usize, r: usize) -> Buf {
    let tmp = cx.gpu.buffer(w * h * nc);
    let out = cx.gpu.buffer(w * h * nc);
    cx.run("mean_box", &[w as u32, h as u32, nc as u32, r as u32, 0], &[Some(src), Some(&tmp)], groups1(h * nc));
    cx.run("mean_box", &[w as u32, h as u32, nc as u32, r as u32, 1], &[Some(&tmp), Some(&out)], groups1(w * nc));
    out
}
fn guided_rgb(cx: &mut Cx<'_>, guide: &Buf, guide_stats: &Buf, input: &Buf, w: usize, h: usize, r: usize) -> Buf {
    let n = w * h;
    let input_stats = cx.gpu.buffer(n * 4);
    map(cx, "haze_input_moments", n, &[], [Some(guide), Some(input), None], &input_stats);
    let input_stats = box_mean(cx, &input_stats, w, h, 4, r);
    let coeff = cx.gpu.buffer(n * 4);
    map(cx, "haze_solve_shared", n, &[lightcraft_pipeline::detail::haze::HAZE_EPS.to_bits()], [Some(guide_stats), Some(&input_stats), None], &coeff);
    let coeff = box_mean(cx, &coeff, w, h, 4, r);
    let out = cx.gpu.buffer(n);
    map(cx, "haze_apply", n, &[], [Some(guide), Some(&coeff), None], &out);
    out
}
/// Native airlight/depth reductions, morphology and RGB guidance. An undersized device
/// still uses the reference preparation; that hybrid stage is explicitly reported.
fn haze_planes(cx: &mut Cx<'_>, lin: &Buf, w: usize, h: usize, scale: f32) -> (Arc<Buf>, Arc<Buf>) {
    use lightcraft_pipeline::detail::haze;
    let n = w * h;
    if !cx.gpu.fits(n * 9) {
        crate::record_cpu_stage("haze preparation");
        let hz = haze::haze_plane(&cx.read_rgb(lin, w, h), scale);
        let buf = cx.gpu.buffer(n * 2);
        let positive = cx.gpu.upload(&hz.positive.data);
        let negative = cx.gpu.upload(&hz.negative.data);
        cx.copy_into(&positive, &buf, 0);
        cx.copy_into(&negative, &buf, n);
        let air = cx.gpu.upload(&[hz.air[0], hz.air[1], hz.air[2], hz.distance]);
        return (Arc::new(buf), Arc::new(air));
    }
    let (w1, w2) = haze::windows(scale);
    let raw_dark = cx.gpu.buffer(n);
    map(cx, "haze_dark", n, &[0], [Some(lin), None, None], &raw_dark);
    let dark = extrema(cx, &raw_dark, w, h, w1, false);
    let zero = cx.zeroed(1);
    let crit = haze_quantile(cx, &dark, lin, n, 0, &zero);
    let crit_b = haze_quantile(cx, &dark, lin, n, 1, &crit);
    let criteria = cx.gpu.buffer(2);
    cx.copy_into(&crit, &criteria, 0);
    cx.copy_into(&crit_b, &criteria, 1);
    let groups = groups1(n);
    let count = (groups[0] * groups[1]) as usize;
    let sums = cx.gpu.buffer(count * 4);
    map(cx, "haze_air_sum", n, &[], [Some(&dark), Some(lin), Some(&criteria)], &sums);
    let sums = reduce_haze(cx, sums, count, 4, "haze_air_reduce");
    let air = cx.gpu.buffer(4);
    map(cx, "haze_air_finish", 1, &[], [Some(&sums), Some(&crit), None], &air);
    let raw = cx.gpu.buffer(n);
    map(cx, "haze_dark", n, &[1], [Some(lin), Some(&air), None], &raw);
    let low = extrema(cx, &raw, w, h, w1, false);
    let positive = extrema(cx, &low, w, h, w1, true);
    let high = extrema(cx, &raw, w, h, w1, true);
    let negative = extrema(cx, &high, w, h, w1, false);
    let guide_stats = cx.gpu.buffer(n * 9);
    map(cx, "haze_guide_moments", n, &[], [Some(lin), None, None], &guide_stats);
    let guide_stats = box_mean(cx, &guide_stats, w, h, 9, w2);
    let positive = guided_rgb(cx, lin, &guide_stats, &positive, w, h, w2);
    let negative = guided_rgb(cx, lin, &guide_stats, &negative, w, h, w2);
    let buf = cx.gpu.buffer(n * 2);
    cx.copy_into(&positive, &buf, 0);
    cx.copy_into(&negative, &buf, n);
    (Arc::new(buf), Arc::new(air))
}
fn reduce_haze(cx: &mut Cx<'_>, mut sums: Buf, mut count: usize, nc: usize, name: &str) -> Buf {
    while count > 1 {
        let groups = groups1(count);
        let len = (groups[0] * groups[1]) as usize;
        let out = cx.gpu.buffer(len * nc);
        map(cx, name, count, &[], [Some(&sums), None, None], &out);
        sums = out;
        count = len;
    }
    sums
}
fn haze_quantile(cx: &mut Cx<'_>, dark: &Buf, img: &Buf, n: usize, phase: u32, criterion: &Buf) -> Buf {
    if phase == 0 {
        let scratch = cx.copy(dark);
        haze_select(cx, &scratch, n, phase, criterion)
    } else {
        let groups = groups1(n);
        let count = (groups[0] * groups[1]) as usize;
        let sums = cx.gpu.buffer(count * 2);
        // Brightness compaction uses read-only input bindings; selection owns its scratch.
        map(cx, "haze_count", n, &[], [Some(dark), None, Some(criterion)], &sums);
        let prefix = haze_scan(cx, &sums, count);
        let criteria = cx.gpu.buffer(1 + prefix.len);
        cx.copy_into(criterion, &criteria, 0);
        cx.copy_into(&prefix, &criteria, 1);
        let scratch = cx.gpu.buffer(n);
        map(cx, "haze_bright_fill", n, &[], [Some(dark), Some(img), Some(&criteria)], &scratch);
        let total = cx.slice(&prefix, count * 2, 1);
        haze_select(cx, &scratch, n, phase, &total)
    }
}

// Hierarchical exclusive vec2<u32> scan. Counts are bit-preserved in storage words.
// Every level stays on device; even a 24 MP selection has no scalar readback/wait.
fn haze_scan(cx: &mut Cx<'_>, counts: &Buf, n: usize) -> Buf {
    let groups = groups1(n);
    let ng = (groups[0] * groups[1]) as usize;
    let scanned = cx.gpu.buffer(2 * (n + ng));
    map(cx, "haze_scan_blocks", n, &[], [Some(counts), None, None], &scanned);
    if ng == 1 {
        return scanned;
    }
    let totals = cx.slice(&scanned, 2 * n, 2 * ng);
    let group_prefix = haze_scan(cx, &totals, ng);
    let out = cx.gpu.buffer(2 * (n + 1));
    map(cx, "haze_scan_add", n + 1, &[n as u32, ng as u32], [Some(&scanned), Some(&group_prefix), None], &out);
    out
}

fn haze_select(cx: &mut Cx<'_>, scratch: &Buf, n: usize, phase: u32, total: &Buf) -> Buf {
    let out = cx.gpu.buffer(1);
    if n <= 65536 {
        map(cx, "haze_quick_select", 1, &[n as u32, phase, 0], [None, Some(scratch), Some(total)], &out);
        return out;
    }
    let mut state = cx.gpu.buffer(16);
    map(cx, "haze_select_init", 1, &[n as u32, phase], [None, Some(scratch), Some(total)], &state);
    let groups = groups1(n);
    let ng = (groups[0] * groups[1]) as usize;
    let counts = cx.gpu.buffer(2 * ng);
    let lists = cx.gpu.buffer(2 * n);
    let unused = cx.gpu.buffer(1);
    // Eight exact parallel partitions reduce typical full-size intervals by orders
    // of magnitude. The remaining reference selector has no iteration cap: unusual
    // pivot distributions still produce the exact result, rather than an estimate.
    for _ in 0..8 {
        let prepared = cx.gpu.buffer(16);
        map(cx, "haze_select_prepare", 1, &[], [Some(&state), Some(scratch), None], &prepared);
        map(cx, "haze_partition_count", n, &[], [Some(scratch), Some(&prepared), None], &counts);
        let prefix = haze_scan(cx, &counts, ng);
        map(cx, "haze_partition_fill", n, &[], [Some(scratch), Some(&prepared), Some(&prefix)], &lists);
        let split = cx.gpu.buffer(16);
        map(cx, "haze_partition_split", 1, &[ng as u32, n as u32], [Some(&lists), Some(&prepared), Some(&prefix)], &split);
        map(cx, "haze_select_swap", n, &[n as u32], [Some(&split), Some(scratch), Some(&lists)], &unused);
        let next = cx.gpu.buffer(16);
        map(cx, "haze_select_commit", 1, &[], [Some(&split), Some(scratch), None], &next);
        state = next;
    }
    map(cx, "haze_quick_select", 1, &[0, 0, 1], [Some(&state), Some(scratch), None], &out);
    out
}

#[cfg(test)]
#[path = "lensdb_tests.rs"]
mod lensdb_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_source_cache_reuses_upload_and_invalidates_parameters() {
        let Some(gpu) = crate::test_device() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let _scope = crate::ctx::RenderScope::new(gpu);
        let errors = crate::ctx::ErrorScopes::push(gpu);
        let mut cx = Cx::new(gpu);
        let stages = GpuStages::default();
        let src = Arc::new(lightcraft_scenes::demo_library()[3].render(73, 49));
        let p = CaptureParams { sigma: 0.8, ..Default::default() };
        let original = stages.source(&mut cx, &src, None);
        let sharp = stages.source(&mut cx, &src, Some(p));
        assert!(!Arc::ptr_eq(&original, &sharp));
        assert!(Arc::ptr_eq(&sharp, &stages.source(&mut cx, &src, Some(p))));
        assert!(Arc::ptr_eq(&original, &stages.source(&mut cx, &src, None)));
        assert!(Arc::ptr_eq(&sharp, &stages.source(&mut cx, &src, Some(p))), "off/on reuses the same capture result");
        assert_eq!(stages.bytes(), 73 * 49 * 3 * 4 * 2, "upload and sharpened source counted once");
        let changed = stages.source(&mut cx, &src, Some(CaptureParams { threshold: 0.8, ..p }));
        assert!(!Arc::ptr_eq(&sharp, &changed), "parameter change reruns capture");
        assert!(Arc::ptr_eq(&original, &stages.source(&mut cx, &src, None)), "capture change preserves the original upload");
        let new_src = Arc::new((*src).clone());
        let new_upload = stages.source(&mut cx, &new_src, None);
        assert!(!Arc::ptr_eq(&original, &new_upload), "decoder source identity invalidates upload");
        assert_eq!(stages.bytes(), 73 * 49 * 3 * 4);
        stages.clear();
        assert_eq!(stages.bytes(), 0);
        cx.flush();
        assert!(errors.pop().is_none());
        assert!(!crate::ctx::failed());
    }

    #[test]
    fn coverage_mask_matches_per_pixel_decision() {
        use lightcraft_develop::{EmbeddedLens, EmbeddedWarp};
        let lens = EmbeddedLens {
            warp: Some(EmbeddedWarp {
                planes: [[1.0, -0.03, 0.01, 0.0, 0.001, -0.002]; 3],
                center: lightcraft_geom::Point::new(0.52, 0.48),
                radius: 0.6,
            }),
            vignette: None,
        };
        let mut s = DevelopSettings::default();
        s.optics.lens_profile = true;
        s.optics.distortion = -40.0;
        s.geometry.horizontal = -15.0;
        s.geometry.rotate = 3.0;
        for orientation in [Orientation::Normal, Orientation::Rotate270] {
            s.orientation = orientation;
            let f = lightcraft_pipeline::geometry::Frame::with_lens(900, 600, &s, true, Some(&lens));
            let wp = f.warp.as_ref().expect("warp");
            // widths that do and don't fill the last word; heights that do and don't fill the last band
            for (w, h) in [f.fit(700, 700), (333, 221), (64, 16), (1, 1)] {
                let o2t = f.out_to_oriented(w, h);
                let mask = coverage_mask(wp, &o2t, w, h);
                let words = w.div_ceil(32);
                assert_eq!(mask.len(), words * h);
                for y in 0..h {
                    for x in 0..w {
                        let bit = mask[y * words + x / 32] >> (x % 32) & 1 == 1;
                        assert_eq!(bit, wp.covers(&o2t, x, y), "{orientation:?} {w}x{h}: pixel {x},{y}");
                    }
                    let pad = mask[y * words + words - 1] >> (w % 32);
                    assert!(w.is_multiple_of(32) || pad == 0, "padding bits set");
                }
            }
        }
    }

    /// The perspective horizon in view: its denominator is exactly 0 on output row 75 (clamped to 1e-300) and
    /// changes sign there, so the blocks around it are undecided (clamp, and division by a range that may hold
    /// zero) and are decided per pixel, while blocks clear of it are filled at once.
    #[test]
    fn coverage_mask_with_the_horizon_in_view() {
        use lightcraft_geom::{Affine, Homography};
        let (w, h) = (900, 600);
        let mut wp = lightcraft_pipeline::optics::Warp::identity(w as f64, h as f64);
        // centred coordinates: v = (y − 300) / 450, denominator 2·v + m8, exactly 0 at y = 75.5
        let m8 = -((75.5 - 300.0) / 450.0 * 2.0);
        wp.persp_inv = Homography([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, m8]);
        wp.persp = wp.persp_inv.inverse().expect("invertible");
        let o2t = Affine::IDENTITY;
        assert_eq!(wp.block_coverage(&o2t, 0, 32, 64, 80), None, "the block holding the horizon");
        assert_eq!(wp.block_coverage(&o2t, 0, 32, 0, 16), Some(false), "beyond the horizon");
        assert_eq!(wp.block_coverage(&o2t, 448, 480, 288, 304), Some(true), "the centre");
        let mask = coverage_mask(&wp, &o2t, w, h);
        let words = w.div_ceil(32);
        let mut covered = 0;
        for y in 0..h {
            for x in 0..w {
                let bit = mask[y * words + x / 32] >> (x % 32) & 1 == 1;
                assert_eq!(bit, wp.covers(&o2t, x, y), "pixel {x},{y}");
                covered += bit as usize;
            }
        }
        assert!(covered > 0 && covered < w * h, "{covered} px covered");
    }

    /// Kernel timings on a 24 MP plane: `cargo test --release -p lightcraft-gpu -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bench_kernels() {
        let Some(gpu) = crate::device() else { return };
        let _scope = crate::ctx::RenderScope::new(gpu);
        let (w, h) = (6000, 4000);
        let src = gpu.upload(&vec![0.5f32; w * h * 3]);
        let time = |name: &str, f: &mut dyn FnMut(&mut Cx<'_>)| {
            let mut best = f64::MAX;
            for _ in 0..5 {
                let mut cx = Cx::new(gpu);
                cx.sync();
                let t = std::time::Instant::now();
                f(&mut cx);
                cx.sync();
                best = best.min(t.elapsed().as_secs_f64() * 1e3);
            }
            eprintln!("{name:<40} {best:.1} ms");
        };
        for nc in [1, 2, 3] {
            time(&format!("gaussian nc={nc} sigma=2"), &mut |cx| drop(gaussian(cx, &src, w, h, nc, 2.0)));
        }
        time("map log_lum", &mut |cx| {
            let o = cx.gpu.buffer(w * h);
            map(cx, "log_lum_k", w * h, &[], [Some(&src), None, None], &o);
        });
        time("guided sigma=1.75", &mut |cx| drop(guided(cx, &src, w, h, 1.75, 0.01)));
    }

    #[test]
    fn orient_map_matches_image_oriented() {
        use Orientation::*;
        let img = Rgb32f::from_fn(5, 3, |x, y| [x as f32, y as f32, 0.0]);
        for o in [Normal, Rotate90, Rotate180, Rotate270, FlipH, Transverse, FlipV, Transpose] {
            let r = img.oriented(o);
            let m = orient_map(o, 5, 3);
            for y in 0..r.height {
                for x in 0..r.width {
                    let (x, y) = (x as i32, y as i32);
                    let (sx, sy) = (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]);
                    assert_eq!(r.get(x as usize, y as usize), img.get(sx as usize, sy as usize), "{o:?} at {x},{y}");
                }
            }
        }
    }
}

fn cache_key(v: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

fn patch_stage(cx: &mut Cx<'_>, img: &Buf, plan: &Plan<'_>) -> Option<Buf> {
    use lightcraft_geom::Point;
    let (w, h) = (plan.w, plan.h);
    let mut image = None;
    for spot in plan.settings.spots.iter().filter(|s| s.is_ai()) {
        let opacity = (spot.opacity / 100.0).clamp(0.0, 1.0) as f32;
        let Some(placement) = spot.patch.as_ref().filter(|_| opacity > 0.0) else { continue };
        let Some(patch) = lightcraft_pipeline::patches::lookup(&placement.key) else { continue };
        let [x0, y0, x1, y1] = placement.rect;
        if patch.width == 0 || patch.height == 0 || x1 <= x0 || y1 <= y0 {
            continue;
        }
        let mut data = cx.gpu.upload(patch.data.as_flattened());
        let (mut pw, mut ph) = (patch.width, patch.height);
        let to_out = plan.frame.norm_to_out(w, h);
        let span = to_out.apply(Point::new(x0, y0)).dist(to_out.apply(Point::new(x1, y0))).max(1e-9);
        let shrink = (pw as f64 / span).floor() as usize;
        if shrink >= 2 {
            let (ow, oh) = (pw.div_ceil(shrink).max(1), ph.div_ceil(shrink).max(1));
            let shrunk = cx.gpu.buffer(ow * oh * 4);
            map(cx, "patch_shrink", ow * oh, &[pw as u32, ph as u32, shrink as u32, ow as u32], [None, Some(&data), None], &shrunk);
            data = shrunk;
            pw = ow;
            ph = oh;
        }
        let [a, b, c, d, e, f] = plan.frame.out_to_norm(w, h).0;
        let (sx, sy) = (pw as f64 / (x1 - x0), ph as f64 / (y1 - y0));
        let mut block = vec![w as u32, pw as u32, ph as u32, opacity.to_bits()];
        block.extend([a * sx, b * sy, c * sx, d * sy, (e - x0) * sx, (f - y0) * sy].map(|v| (v as f32).to_bits()));
        let out = cx.gpu.buffer(w * h * 3);
        map(cx, "patch_apply", w * h, &block, [Some(image.as_ref().unwrap_or(img)), Some(&data), None], &out);
        image = Some(out);
    }
    image
}

// Layer descriptors index parameter blocks in aux and image/plane data in the shared mask buffer.
// All tools blend at their reference stage; NR images always derive from the prepared input.
#[allow(clippy::too_many_arguments)]
fn layer_buffers(
    cx: &mut Cx<'_>,
    lin: &Buf,
    prep: &Prep,
    masks: Option<Buf>,
    plan: &Plan<'_>,
    info: &SourceInfo,
    req: &RenderRequest,
    layers: Vec<lightcraft_pipeline::layers::LayerK>,
    present: &Present,
    block: &mut [u32],
    aux: &mut Vec<f32>,
) -> Option<Buf> {
    let (w, h) = (plan.w, plan.h);
    let scale = lightcraft_pipeline::detail::out_per_orig(plan.px_per_long, plan.src_long, info.sensor_scale);
    let list: Vec<_> = plan.settings.masks.iter().filter(|m| m.visible && !m.components.is_empty()).collect();
    let mut layers = layers.into_iter().peekable();
    let mut planes = Vec::new();
    let mut offset = masks.as_ref().map_or(0, |m| m.len);
    let mut descriptors = Vec::new();
    for (mi, m) in list.iter().enumerate() {
        let resolved = if layers.peek().is_some_and(|l| l.mask == mi) { layers.next() } else { None };
        let view = m.tools.view(local::effective_wb(info, &plan.settings));
        let nr = lightcraft_pipeline::detail::nr::nr_params(&view, scale);
        if resolved.is_none() && nr.is_none() {
            continue;
        }
        let mut lf = FinishParams::new(&DevelopSettings::default(), &plan.frame, info, w, h, plan.px_per_long, req.space);
        lf.layers.clear();
        lf.curves = None;
        lf.vig = None;
        lf.grain = None;
        lf.tone_eq = None;
        let mut flags = 0u32;
        let mut wb = None;
        let mut gain = 1.0;
        if let Some(l) = resolved {
            wb = l.wb;
            gain = l.gain.unwrap_or(1.0);
            lf.dehaze = l.dehaze;
            if l.scene() {
                flags |= 1;
            }
            if let Some(local) = l.local {
                flags |= 2;
                lf.hl = local.hl;
                lf.sh = local.sh;
                lf.clar = local.clar;
                lf.tex = local.tex;
                lf.sharp_amount = local.sharp_amount;
                lf.sharp_detail = local.sharpen_detail;
                lf.sharp_masking = local.sharpen_mask;
            }
            if let Some(t) = l.tone {
                flags |= 4;
                lf.tone = t;
            }
            if let Some(ops) = l.ops {
                flags |= 8;
                lf.ops = ops;
            }
            if let Some(v) = l.vig {
                flags |= 16;
                lf.vig = Some(v);
            }
            if let Some((curves, refine)) = l.curves {
                flags |= 32;
                lf.curves = Some(curves);
                lf.refine_sat = refine;
            }
            if let Some(grain) = l.grain {
                flags |= 64;
                lf.grain = Some(grain);
            }
        }
        let mut nr_off = u32::MAX;
        let mut b1_off = 0;
        let mut b2_off = 0;
        if let Some(nr) = nr {
            nr_off = offset as u32;
            let input = cx.copy(lin);
            let image = denoise_native(cx, input, w, h, &nr);
            offset += image.len;
            planes.push(image);
        }
        if let Some(d) = m.tools.detail.filter(|d| d.sharpen_amount != 0.0) {
            let sigma = d.sharpen_radius.clamp(0.5, 3.0) as f32 * scale;
            if sigma >= lightcraft_pipeline::detail::SHARP_MIN_SIGMA {
                lf.sharp_sigma = sigma;
                let b1 = gaussian_fine(cx, &prep.log_l, w, h, 1, sigma);
                let b2 = gaussian_fine(cx, &b1, w, h, 1, sigma);
                b1_off = offset as u32;
                offset += b1.len;
                b2_off = offset as u32;
                offset += b2.len;
                planes.push(b1);
                planes.push(b2);
            }
        }
        let (mut lp, la) = finish_block(&lf, &[], present);
        let table_off = aux.len();
        aux.extend(la);
        for name in ["TONE_OFF", "SRGB_OFF", "CURVE_OFF", "TEQ_OFF"] {
            if let Some(i) = crate::params::index(name) {
                lp[i] += table_off as u32;
            }
        }
        if let Some(i) = crate::params::index("LAYER_GAIN") {
            lp[i] = gain.to_bits();
        }
        if let Some(mat) = wb {
            if let Some(i) = crate::params::index("LAYER_WB") {
                lp[i] = 1;
            }
            if let Some(i) = crate::params::index("LAYER_M") {
                for (p, v) in lp[i..].iter_mut().zip(mat.into_iter().flatten()) {
                    *p = v.to_bits();
                }
            }
        }
        let param_off = aux.len();
        aux.extend(lp.into_iter().map(f32::from_bits));
        descriptors.extend([mi as u32, param_off as u32, flags, nr_off, b1_off, b2_off].map(f32::from_bits));
    }
    if descriptors.is_empty() {
        return masks;
    }
    if let Some(i) = crate::params::index("NLAYER") {
        block[i] = (descriptors.len() / 6) as u32;
    }
    if let Some(i) = crate::params::index("LAYER_OFF") {
        block[i] = aux.len() as u32;
    }
    aux.extend(descriptors);
    if planes.is_empty() {
        return masks;
    }
    let joined = cx.gpu.buffer(offset);
    let mut offset = 0;
    if let Some(masks) = masks {
        cx.copy_into(&masks, &joined, 0);
        offset = masks.len;
    }
    for plane in planes {
        cx.copy_into(&plane, &joined, offset);
        offset += plane.len;
    }
    Some(joined)
}

#[cfg(test)]
#[path = "remaining_tests.rs"]
mod remaining_tests;
