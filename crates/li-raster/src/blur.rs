//! Shared Gaussian and box filtering. Small Gaussian radii use the exact sampled kernel;
//! wide kernels use Kovesi's three running-sum boxes. Fine taps retain the variance-corrected
//! subpixel/GPU policy. Border rules, accumulation order and alpha are explicit at adapters.

use crate::par_rows;
use crate::resample::Pixel;

/// Gaussian transition to radius-independent running sums.
pub const EXACT_SIGMA: f32 = 4.0;
/// Fine-kernel transition retained for the Develop sharpening GPU twin.
pub const FINE_SIGMA: f32 = 2.0;
/// Rows in a vertical running-sum band.
pub const BAND: usize = 32;

#[derive(Clone)]
struct Buffer<T> {
    width: usize,
    height: usize,
    data: Vec<T>,
}
impl<T: Pixel> Buffer<T> {
    fn new(width: usize, height: usize) -> Self {
        Self { width, height, data: vec![T::zero(); width * height] }
    }
    fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Normalized Gaussian kernel with standard deviation `sigma` (radius 3σ).
pub fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    if sigma < 0.05 || !sigma.is_finite() {
        return vec![1.0];
    }
    let r = (sigma * 3.0).ceil() as i32;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let s: f32 = k.iter().sum();
    k.into_iter().map(|v| v / s).collect()
}

/// Odd box widths whose 3-fold convolution approximates a Gaussian of `sigma`
/// (Kovesi 2010), returned as radii.
pub fn box_radii(sigma: f32) -> [usize; 3] {
    let sigma = if sigma.is_finite() { sigma.abs().min(100_000.0) } else { 0.0 };
    let n = 3.0f32;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl as f32 * wl as f32) - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0)).round().clamp(0.0, n) as usize;
    let mut radii = std::array::from_fn(|i| if i < m { (wl as usize - 1) / 2 } else { (wu as usize - 1) / 2 });
    // Kovesi's rounded triple degenerates below sigma ~ 0.577: `wl` floors to 1 and every
    // radius becomes 0, so the passes would be no-ops and callers with documented small
    // sigmas (Camera Raw `sharpenRadius` 0.5, colour noise, HDR Toning `radius` 1) would
    // silently change nothing (issue #706). One radius-1 pass is the smallest real blur;
    // it only engages where the ideal widths are already at their floor, leaving larger
    // sigmas exactly as the paper prescribes.
    if radii.iter().all(|&r| r == 0) {
        radii[0] = 1;
    }
    radii
}

/// One horizontal box pass over `row` (clamped edges), using `tmp` as scratch.
#[inline]
fn box_row<T: Pixel>(row: &mut [T], tmp: &mut Vec<T>, r: usize) {
    let w = row.len();
    if r == 0 || w == 0 {
        return;
    }
    tmp.clear();
    tmp.extend_from_slice(row);
    let s = &tmp[..];
    let last = w - 1;
    let inv = 1.0 / (2 * r + 1) as f32;
    let mut acc = T::zero();
    for i in 0..=2 * r {
        acc = acc.madd(s[i.saturating_sub(r).min(last)], 1.0);
    }
    for x in 0..w {
        row[x] = T::zero().madd(acc, inv);
        acc = acc.madd(s[(x + r + 1).min(last)], 1.0).madd(s[x.saturating_sub(r)], -1.0);
    }
}

/// One vertical box pass `src → dst` (clamped edges): a running sum per column, vectorized across
/// the row, parallel over horizontal bands. Cache-friendly — no transposes.
fn box_v<T: Pixel>(src: &Buffer<T>, dst: &mut Buffer<T>, r: usize) {
    let (w, h) = (src.width, src.height);
    let inv = 1.0 / (2 * r + 1) as f32;
    let last = h - 1;
    let row = |y: usize| &src.data[y * w..(y + 1) * w];
    par_rows(&mut dst.data, w * BAND, |band, out| {
        let y0 = band * BAND;
        let mut acc = vec![T::zero(); w];
        for i in 0..=2 * r {
            let yy = (y0 + i).saturating_sub(r).min(last);
            for (a, v) in acc.iter_mut().zip(row(yy)) {
                *a = a.madd(*v, 1.0);
            }
        }
        for (k, o) in out.chunks_mut(w).enumerate() {
            let y = y0 + k;
            for (d, a) in o.iter_mut().zip(&acc) {
                *d = T::zero().madd(*a, inv);
            }
            let (add, sub) = (row((y + r + 1).min(last)), row(y.saturating_sub(r)));
            for ((a, p), m) in acc.iter_mut().zip(add).zip(sub) {
                *a = a.madd(*p, 1.0).madd(*m, -1.0);
            }
        }
    });
}

fn gaussian_boxes<T: Pixel>(img: &Buffer<T>, sigma: f32) -> Buffer<T> {
    if sigma <= 0.3 || img.is_empty() {
        return img.clone();
    }
    let radii = box_radii(sigma);
    let mut a = img.clone();
    par_rows(&mut a.data, img.width, |_, row| {
        let mut tmp = Vec::with_capacity(row.len());
        for r in radii {
            box_row(row, &mut tmp, r);
        }
    });
    let mut b = Buffer::<T>::new(img.width, img.height);
    for r in radii {
        if r == 0 {
            continue;
        }
        box_v(&a, &mut b, r);
        std::mem::swap(&mut a, &mut b);
    }
    a
}

/// One-sided taps of the true Gaussian of `sigma` (`w[0]` at the centre, `w[k]` at ±k), summing
/// to 1. Under 0.7 px a sampled Gaussian is too narrow (its variance falls below σ², so the
/// frequency response no longer matches σ): there a three-tap kernel `[a, 1 − 2a, a]` with
/// `a = σ²/2` keeps the variance exact.
pub fn gauss_taps(sigma: f32) -> Vec<f32> {
    let sigma = sigma.max(0.0);
    if sigma < 0.7 {
        let a = sigma * sigma * 0.5;
        return vec![1.0 - 2.0 * a, a];
    }
    let r = (3.0 * sigma).ceil().max(1.0) as usize;
    let k = -0.5 / (sigma * sigma);
    let mut w: Vec<f32> = (0..=r).map(|i| (k * (i * i) as f32).exp()).collect();
    let sum = w[0] + 2.0 * w[1..].iter().sum::<f32>();
    w.iter_mut().for_each(|v| *v /= sum);
    w
}

/// One horizontal pass of the symmetric kernel `taps` over the row `src` into `out` (clamped
/// edges). Summation order: centre, then each pair `−k, +k` outwards (the GPU twin's order).
#[inline]
fn conv_row<T: Pixel>(src: &[T], out: &mut [T], taps: &[f32]) {
    let last = src.len() as isize - 1;
    for (x, o) in out.iter_mut().enumerate() {
        let mut acc = T::zero().madd(src[x], taps[0]);
        for (k, t) in taps.iter().enumerate().skip(1) {
            let l = (x as isize - k as isize).clamp(0, last) as usize;
            let r = (x as isize + k as isize).clamp(0, last) as usize;
            acc = acc.madd(src[l], *t).madd(src[r], *t);
        }
        *o = acc;
    }
}

/// Blur with the true Gaussian kernel of `sigma` for `sigma` < [`FINE_SIGMA`] (separable,
/// clamped edges; see [`gauss_taps`]), else [`gaussian`]. For small radii the box approximation
/// rounds to whole-pixel boxes (σ = 0.5 is a copy); this keeps the frequency response.
/// `sigma <= 0.05` returns a copy.
fn gaussian_fine_buffer<T: Pixel>(img: &Buffer<T>, sigma: f32) -> Buffer<T> {
    if sigma >= FINE_SIGMA {
        return gaussian_boxes(img, sigma);
    }
    if sigma <= 0.05 || img.is_empty() {
        return img.clone();
    }
    let taps = gauss_taps(sigma);
    let (w, h) = (img.width, img.height);
    let mut a = Buffer::<T>::new(w, h);
    par_rows(&mut a.data, w, |y, row| conv_row(&img.data[y * w..(y + 1) * w], row, &taps));
    let mut b = Buffer::<T>::new(w, h);
    let last = h as isize - 1;
    par_rows(&mut b.data, w, |y, row| {
        let at = |dy: isize| {
            let yy = (y as isize + dy).clamp(0, last) as usize;
            &a.data[yy * w..(yy + 1) * w]
        };
        for (o, v) in row.iter_mut().zip(at(0)) {
            *o = T::zero().madd(*v, taps[0]);
        }
        for (k, t) in taps.iter().enumerate().skip(1) {
            let (up, down) = (at(-(k as isize)), at(k as isize));
            for ((o, u), d) in row.iter_mut().zip(up).zip(down) {
                *o = o.madd(*u, *t).madd(*d, *t);
            }
        }
    });
    b
}

/// Gaussian blur of dense pixels with replicated borders. No-op at sigma <= 0.05.
/// Unlike rounded box-only filtering, sigma 0.5 produces a real blur (#706).
pub fn gaussian<T: Pixel>(src: &[T], width: usize, height: usize, sigma: f32) -> Vec<T> {
    assert_eq!(src.len(), width * height);
    let mut img = Buffer { width, height, data: src.to_vec() };
    if img.is_empty() || sigma <= 0.05 || !sigma.is_finite() {
        return img.data;
    }
    img.data.iter_mut().for_each(|p| *p = p.premultiplied());
    let mut out = if sigma > EXACT_SIGMA { gaussian_boxes(&img, sigma).data } else { convolve(&img.data, width, height, &gaussian_kernel(sigma)) };
    out.iter_mut().for_each(|p| *p = p.straight());
    out
}

/// Variance-corrected fine Gaussian, preserving the GPU twin's centre/pair summation order.
pub fn gaussian_fine<T: Pixel>(src: &[T], width: usize, height: usize, sigma: f32) -> Vec<T> {
    assert_eq!(src.len(), width * height);
    if sigma <= 0.05 || !sigma.is_finite() || width == 0 || height == 0 {
        return src.to_vec();
    }
    let data = src.iter().map(|p| p.premultiplied()).collect();
    let mut out = gaussian_fine_buffer(&Buffer { width, height, data }, sigma).data;
    out.iter_mut().for_each(|p| *p = p.straight());
    out
}

/// Dense separable convolution, replicated borders, full odd symmetric kernel.
pub fn convolve<T: Pixel>(src: &[T], w: usize, h: usize, kernel: &[f32]) -> Vec<T> {
    assert_eq!(src.len(), w * h);
    if w == 0 || h == 0 {
        return src.to_vec();
    }
    let r = (kernel.len() / 2) as isize;
    let mut tmp = vec![T::zero(); src.len()];
    par_rows(&mut tmp, w, |y, row| {
        for (x, d) in row.iter_mut().enumerate() {
            for (i, k) in kernel.iter().enumerate() {
                let xx = (x as isize + i as isize - r).clamp(0, w as isize - 1) as usize;
                *d = d.madd(src[y * w + xx], *k);
            }
        }
    });
    let mut out = vec![T::zero(); src.len()];
    par_rows(&mut out, w, |y, row| {
        for (i, k) in kernel.iter().enumerate() {
            let yy = (y as isize + i as isize - r).clamp(0, h as isize - 1) as usize;
            for (d, s) in row.iter_mut().zip(&tmp[yy * w..(yy + 1) * w]) {
                *d = d.madd(*s, *k);
            }
        }
    });
    out
}

/// In-place running-sum box blur of every row of a `w × h × n` interleaved
/// buffer with radius `r` (edges clamped). `scratch` must hold `w × n`.
fn box_rows_n(buf: &mut [f32], w: usize, n: usize, r: usize, scratch: &mut Vec<f32>) {
    if r == 0 || w == 0 {
        return;
    }
    let norm = 1.0 / (2 * r + 1) as f64;
    for row in buf.chunks_exact_mut(w * n) {
        scratch.clear();
        scratch.extend_from_slice(row);
        // f64 sums: rounding must not depend on where a window starts (tile independence).
        let at = |x: isize, c: usize| scratch[(x.clamp(0, w as isize - 1) as usize) * n + c] as f64;
        for c in 0..n {
            let mut acc: f64 = (-(r as isize)..=r as isize).map(|x| at(x, c)).sum();
            for x in 0..w {
                row[x * n + c] = (acc * norm) as f32;
                acc += at(x as isize + r as isize + 1, c) - at(x as isize - r as isize, c);
            }
        }
    }
}

/// Same as [`box_rows_n`] over columns (no transpose; walks with a stride).
fn box_cols_n(buf: &mut [f32], w: usize, h: usize, n: usize, r: usize, scratch: &mut Vec<f32>) {
    if r == 0 || h == 0 {
        return;
    }
    let norm = 1.0 / (2 * r + 1) as f64;
    for x in 0..w {
        scratch.clear();
        for y in 0..h {
            scratch.extend_from_slice(&buf[(y * w + x) * n..(y * w + x + 1) * n]);
        }
        let at = |y: isize, c: usize| scratch[(y.clamp(0, h as isize - 1) as usize) * n + c] as f64;
        for c in 0..n {
            let mut acc: f64 = (-(r as isize)..=r as isize).map(|y| at(y, c)).sum();
            for y in 0..h {
                buf[(y * w + x) * n + c] = (acc * norm) as f32;
                acc += at(y as isize + r as isize + 1, c) - at(y as isize - r as isize, c);
            }
        }
    }
}

/// Box blur of an interleaved buffer (radius `r` in both directions).
pub fn box_blur_n(buf: &mut [f32], w: usize, h: usize, n: usize, r: usize) {
    if w == 0 || h == 0 || n == 0 {
        return;
    }
    assert_eq!(buf.len(), w * h * n);
    let mut scratch = Vec::with_capacity(w.max(h) * n);
    box_rows_n(buf, w, n, r, &mut scratch);
    box_cols_n(buf, w, h, n, r, &mut scratch);
}

/// Premultiplies colour channels by alpha (in place). No-op without alpha.
pub fn premultiply(data: &mut [f32], ch: usize, alpha: bool) {
    if !alpha {
        return;
    }
    for px in data.chunks_exact_mut(ch) {
        let a = px[ch - 1];
        for v in px.iter_mut().take(ch - 1) {
            *v *= a;
        }
    }
}

/// Inverse of [`premultiply`].
pub fn unpremultiply(data: &mut [f32], ch: usize, alpha: bool) {
    if !alpha {
        return;
    }
    for px in data.chunks_exact_mut(ch) {
        let a = px[ch - 1];
        for v in px.iter_mut().take(ch - 1) {
            *v = if a > 1e-7 { *v / a } else { 0.0 };
        }
    }
}
/// Box mean with radius `r` (window clipped to the image, divided by the pixel count).
pub fn box_mean(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0.0f32; w * h];
    let mut pre = vec![0.0f64; w.max(h) + 1];
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        for x in 0..w {
            pre[x + 1] = pre[x] + row[x] as f64;
        }
        for x in 0..w {
            let (lo, hi) = (x.saturating_sub(r), x.saturating_add(r).saturating_add(1).min(w));
            tmp[y * w + x] = ((pre[hi] - pre[lo]) / (hi - lo) as f64) as f32;
        }
    }
    let mut out = vec![0.0f32; w * h];
    let mut col = vec![0.0f64; (h + 1) * w];
    for y in 0..h {
        for x in 0..w {
            col[(y + 1) * w + x] = col[y * w + x] + tmp[y * w + x] as f64;
        }
    }
    for y in 0..h {
        let (lo, hi) = (y.saturating_sub(r), y.saturating_add(r).saturating_add(1).min(h));
        let n = (hi - lo) as f64;
        for x in 0..w {
            out[y * w + x] = ((col[hi * w + x] - col[lo * w + x]) / n) as f32;
        }
    }
    out
}

/// Scratch buffers of one worker for padded running-sum [`line_passes`].
#[derive(Default)]
pub struct Scratch {
    pub line: Vec<f32>,
    next: Vec<f32>,
    acc: Vec<f32>,
}

/// One running-sum box pass of radius `r` along a line of elements of `w` floats each (a pixel's
/// channels, or a strip of a row): element `i` of `dst` is the mean of elements `i ..= i + 2r` of
/// `src`, which is `2r` elements longer.
fn box_pass(src: &[f32], dst: &mut [f32], w: usize, r: usize, acc: &mut Vec<f32>) {
    // Pixels are short elements: fixed sizes let the compiler keep the sums in registers.
    match w {
        1 => box_pass_px::<1>(src, dst, r),
        2 => box_pass_px::<2>(src, dst, r),
        3 => box_pass_px::<3>(src, dst, r),
        4 => box_pass_px::<4>(src, dst, r),
        5 => box_pass_px::<5>(src, dst, r),
        0 => {}
        _ => {
            let norm = 1.0 / (2 * r + 1) as f32;
            acc.clear();
            acc.resize(w, 0.0);
            for e in src.chunks_exact(w).take(2 * r + 1) {
                for (a, s) in acc.iter_mut().zip(e) {
                    *a += s;
                }
            }
            for (i, d) in dst.chunks_exact_mut(w).enumerate() {
                for (d, a) in d.iter_mut().zip(acc.iter()) {
                    *d = a * norm;
                }
                let (add, sub) = ((i + 2 * r + 1) * w, i * w);
                if let (Some(add), Some(sub)) = (src.get(add..add + w), src.get(sub..sub + w)) {
                    for ((a, add), sub) in acc.iter_mut().zip(add).zip(sub) {
                        *a += add - sub;
                    }
                }
            }
        }
    }
}

/// [`box_pass`] for elements of `N` floats.
fn box_pass_px<const N: usize>(src: &[f32], dst: &mut [f32], r: usize) {
    let (src, dst) = (src.as_chunks::<N>().0, dst.as_chunks_mut::<N>().0);
    let norm = 1.0 / (2 * r + 1) as f32;
    let mut acc = [0.0f32; N];
    for e in src.iter().take(2 * r + 1) {
        for c in 0..N {
            acc[c] += e[c];
        }
    }
    for (i, d) in dst.iter_mut().enumerate() {
        for c in 0..N {
            d[c] = acc[c] * norm;
        }
        if let (Some(add), Some(sub)) = (src.get(i + 2 * r + 1), src.get(i)) {
            for c in 0..N {
                acc[c] += add[c] - sub[c];
            }
        }
    }
}

/// Box passes of the given radii in turn along the line in `s.line` (`w`-float elements, as many
/// as `dst` holds plus twice the radii's sum; consumed), ending in `dst`.
pub fn line_passes(dst: &mut [f32], w: usize, radii: &[usize], s: &mut Scratch) {
    let Scratch { line, next, acc } = s;
    let Some(len) = dst.len().checked_div(w) else { return };
    let mut tail: usize = radii.iter().sum();
    for (k, &r) in radii.iter().enumerate() {
        tail -= r;
        if k + 1 == radii.len() {
            box_pass(line, dst, w, r, acc);
        } else {
            next.resize((len + 2 * tail) * w, 0.0);
            box_pass(line, next, w, r, acc);
            std::mem::swap(line, next);
        }
    }
}

/// Convolve a premultiplied padded window into its interior, preserving editor summation.
/// Window dimensions are `(ow+kx.len()-1) x (oh+ky.len()-1)`.
pub fn convolve_padded(p: &[f32], ow: usize, oh: usize, n: usize, kx: &[f32], ky: &[f32]) -> Vec<f32> {
    let ww = ow + kx.len() - 1;
    let th = oh + ky.len() - 1;
    assert_eq!(p.len(), ww * th * n);
    let mut tmp = vec![0.0f32; ow * th * n];
    for ty in 0..th {
        let row = &p[ty * ww * n..(ty + 1) * ww * n];
        let dst = &mut tmp[ty * ow * n..(ty + 1) * ow * n];
        for ox in 0..ow {
            let d = &mut dst[ox * n..(ox + 1) * n];
            for (i, kv) in kx.iter().enumerate() {
                let s = &row[(ox + i) * n..(ox + i + 1) * n];
                for c in 0..n {
                    d[c] += s[c] * kv;
                }
            }
        }
    }
    let mut res = vec![0.0f32; ow * oh * n];
    for oy in 0..oh {
        let d = &mut res[oy * ow * n..(oy + 1) * ow * n];
        for (i, kv) in ky.iter().enumerate() {
            let s = &tmp[(oy + i) * ow * n..(oy + i + 1) * ow * n];
            for (dv, sv) in d.iter_mut().zip(s) {
                *dv += sv * kv;
            }
        }
    }
    res
}

/// Approximate Gaussian for tiled field effects whose halo derives from the three radii.
/// Preserves the editor's f64 running sums and #706 minimum nonzero box.
pub fn gaussian_boxes_n(buf: &mut [f32], w: usize, h: usize, n: usize, sigma: f32) {
    if sigma < 0.3 || !sigma.is_finite() || w == 0 || h == 0 || n == 0 {
        return;
    }
    assert_eq!(buf.len(), w * h * n);
    let mut scratch = Vec::with_capacity(w.max(h) * n);
    let radii = box_radii(sigma);
    for r in radii {
        box_rows_n(buf, w, n, r, &mut scratch);
    }
    for r in radii {
        box_cols_n(buf, w, h, n, r, &mut scratch);
    }
}

/// Gaussian of interleaved samples, alpha-weighted if requested. Replicated borders.
pub fn gaussian_n(buf: &mut [f32], w: usize, h: usize, n: usize, sigma: f32, alpha: bool) {
    if sigma <= 0.05 || !sigma.is_finite() || w == 0 || h == 0 || n == 0 {
        return;
    }
    assert_eq!(buf.len(), w * h * n);
    premultiply(buf, n, alpha);
    if sigma > EXACT_SIGMA {
        let mut scratch = Vec::with_capacity(w.max(h) * n);
        let radii = box_radii(sigma);
        for r in radii {
            box_rows_n(buf, w, n, r, &mut scratch);
        }
        for r in radii {
            box_cols_n(buf, w, h, n, r, &mut scratch);
        }
    } else {
        let k = gaussian_kernel(sigma);
        let r = k.len() / 2;
        let mut padded = Vec::with_capacity((w + 2 * r) * (h + 2 * r) * n);
        for y in 0..h + 2 * r {
            let yy = y.saturating_sub(r).min(h - 1);
            for x in 0..w + 2 * r {
                let xx = x.saturating_sub(r).min(w - 1);
                padded.extend_from_slice(&buf[(yy * w + xx) * n..(yy * w + xx + 1) * n]);
            }
        }
        buf.copy_from_slice(&convolve_padded(&padded, w, h, n, &k, &k));
    }
    unpremultiply(buf, n, alpha);
}

/// Scalar minimum (or maximum) over a square, borders replicated.
pub fn extrema_filter(src: &[f32], w: usize, h: usize, r: usize, maximum: bool) -> Vec<f32> {
    assert_eq!(src.len(), w * h);
    if r == 0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let pick = |a: f32, b: f32| if maximum { a.max(b) } else { a.min(b) };
    let mut tmp = vec![0.0; src.len()];
    par_rows(&mut tmp, w, |y, row| {
        for (x, d) in row.iter_mut().enumerate() {
            let lo = x.saturating_sub(r);
            let hi = x.saturating_add(r).min(w - 1);
            *d = src[y * w + lo..=y * w + hi].iter().copied().fold(if maximum { f32::NEG_INFINITY } else { f32::INFINITY }, pick);
        }
    });
    let mut out = vec![0.0; src.len()];
    par_rows(&mut out, w, |y, row| {
        let lo = y.saturating_sub(r);
        let hi = y.saturating_add(r).min(h - 1);
        row.copy_from_slice(&tmp[lo * w..(lo + 1) * w]);
        for yy in lo + 1..=hi {
            for (d, s) in row.iter_mut().zip(&tmp[yy * w..(yy + 1) * w]) {
                *d = pick(*d, *s);
            }
        }
    });
    out
}

pub mod selection;

#[cfg(test)]
mod d2_tests {
    use super::*;

    // Independent pre-merge matting/direct-Gaussian definition (left-to-right f32 sums).
    fn legacy_exact(src: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
        let r = (3.0 * sigma).ceil() as i64;
        let mut k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let sum: f32 = k.iter().sum();
        k.iter_mut().for_each(|v| *v /= sum);
        let pass = |src: &[f32], vertical: bool| {
            let mut dst = vec![0.0; w * h];
            for y in 0..h {
                for x in 0..w {
                    for (i, tap) in k.iter().enumerate() {
                        let xx = if vertical { x } else { (x as i64 + i as i64 - r).clamp(0, w as i64 - 1) as usize };
                        let yy = if vertical { (y as i64 + i as i64 - r).clamp(0, h as i64 - 1) as usize } else { y };
                        dst[y * w + x] += src[yy * w + xx] * tap;
                    }
                }
            }
            dst
        };
        pass(&pass(src, false), true)
    }

    #[test]
    fn j01_d2_exact_gaussian_byte_parity() {
        let src: Vec<f32> = (0..17 * 9).map(|i| ((i * 37 + 19) % 101) as f32 / 100.0).collect();
        for sigma in [0.3, 0.5, 1.0, 2.0, 4.0] {
            let expected = legacy_exact(&src, 17, 9, sigma);
            let mut actual = src.clone();
            gaussian_n(&mut actual, 17, 9, 1, sigma, false);
            assert_eq!(actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), "sigma {sigma}");
            assert_eq!(gaussian(&src, 17, 9, sigma), expected);
        }
    }

    #[test]
    fn j01_d2_small_sigma_706_and_variance() {
        let mut src = vec![0.0; 41];
        src[20] = 1.0;
        let sampled = gaussian(&src, 41, 1, 0.5);
        let fine = gaussian_fine(&src, 41, 1, 0.5);
        let mass = |v: &[f32]| v.iter().sum::<f32>();
        let variance = |v: &[f32]| v.iter().enumerate().map(|(i, v)| v * (i as f32 - 20.0).powi(2)).sum::<f32>();
        assert!(sampled[20] < 0.8, "0.5-pixel feather must blur");
        assert_eq!(box_radii(0.5), [1, 0, 0]);
        assert!((mass(&sampled) - 1.0).abs() < 1e-6);
        assert!((mass(&fine) - 1.0).abs() < 1e-6);
        assert!((variance(&fine) - 0.25).abs() < 1e-6);
        assert!((variance(&sampled) - 0.21501267).abs() < 1e-6);
        eprintln!(
            "D2 sigma 0.5: before box-only variance 0; sampled variance {}; fine variance {}; masses {}/{}",
            variance(&sampled),
            variance(&fine),
            mass(&sampled),
            mass(&fine)
        );
    }

    #[test]
    fn j01_d2_premultiplied_fringe_and_generic_parity() {
        let src = [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        let mut flat = src.into_iter().flatten().collect::<Vec<_>>();
        gaussian_n(&mut flat, 3, 1, 4, 0.5, true);
        let generic = gaussian(&src, 3, 1, 0.5);
        for (p, q) in generic.iter().zip(flat.as_chunks::<4>().0) {
            assert_eq!(p.as_slice(), q);
            if p[3] > 1e-7 {
                assert!((p[0] - 1.0).abs() < 1e-6);
                assert_eq!(p[2], 0.0);
            }
        }
        let mut straight = src.into_iter().flatten().collect::<Vec<_>>();
        gaussian_n(&mut straight, 3, 1, 4, 0.5, false);
        assert!(straight[6] > 0.8); // Hidden blue dominates the formerly straight-alpha result.
        eprintln!("D2 hidden-blue fringe at middle pixel: straight={} premultiplied={}", straight[6], flat[6]);
    }

    #[test]
    fn j01_d2_box_clipped_and_extrema_boundaries() {
        let src = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
        let clipped = box_mean(&src, 3, 2, 1);
        assert_eq!(clipped, vec![6.75, 10.5, 13.5, 6.75, 10.5, 13.5]);
        let mut repeated = src.to_vec();
        box_blur_n(&mut repeated, 3, 2, 1, 1);
        assert!((repeated[0] - 40.0 / 9.0).abs() < 1e-6);
        assert_eq!(extrema_filter(&src, 3, 2, usize::MAX, false), vec![1.0; 6]);
        assert_eq!(extrema_filter(&src, 3, 2, usize::MAX, true), vec![32.0; 6]);
        assert!(gaussian::<f32>(&[], 0, 0, 0.5).is_empty());
    }

    #[test]
    fn j01_d2_large_gaussian_policy_parity() {
        // Independent previous fxutil three-box definition, preserving f64 running sums.
        fn old_pass(v: &mut [f32], w: usize, h: usize, r: usize, vertical: bool) {
            if r == 0 {
                return;
            }
            let (lines, len) = if vertical { (w, h) } else { (h, w) };
            for line in 0..lines {
                let idx = |p| if vertical { p * w + line } else { line * w + p };
                let s: Vec<f32> = (0..len).map(|p| v[idx(p)]).collect();
                let at = |p: isize| s[p.clamp(0, len as isize - 1) as usize] as f64;
                let mut sum: f64 = (-(r as isize)..=r as isize).map(at).sum();
                let norm = 1.0 / (2 * r + 1) as f64;
                for p in 0..len {
                    v[idx(p)] = (sum * norm) as f32;
                    sum += at(p as isize + r as isize + 1) - at(p as isize - r as isize);
                }
            }
        }
        let src: Vec<f32> = (0..19 * 11).map(|i| ((i * 43 + 7) % 71) as f32 / 70.0).collect();
        for sigma in [0.5, 1.0, 4.0, 5.0, 12.0] {
            let mut old = src.clone();
            for r in box_radii(sigma) {
                old_pass(&mut old, 19, 11, r, false);
            }
            for r in box_radii(sigma) {
                old_pass(&mut old, 19, 11, r, true);
            }
            let mut new = src.clone();
            gaussian_boxes_n(&mut new, 19, 11, 1, sigma);
            assert_eq!(old, new, "sigma {sigma}");
            if sigma > EXACT_SIGMA {
                let mut default = src.clone();
                gaussian_n(&mut default, 19, 11, 1, sigma, false);
                assert_eq!(old, default);
            }
        }
    }
}

#[cfg(test)]
#[test]
fn j01_final_field_box_precision_against_direct_mean() {
    let (w, h, r) = (129, 7, 17usize);
    let src: Vec<f32> = (0..w * h).map(|i| ((i * 37 + 19) % 101) as f32 / 100.0).collect();
    let legacy = |input: &[f32], horizontal: bool| {
        let mut output = vec![0.0; input.len()];
        let (lines, len) = if horizontal { (h, w) } else { (w, h) };
        let norm = 1.0 / (2 * r + 1) as f32;
        for line in 0..lines {
            let at = |k: isize| {
                let k = k.clamp(0, len as isize - 1) as usize;
                input[if horizontal { line * w + k } else { k * w + line }]
            };
            let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
            for k in 0..len {
                output[if horizontal { line * w + k } else { k * w + line }] = sum * norm;
                sum += at(k as isize + r as isize + 1) - at(k as isize - r as isize);
            }
        }
        output
    };
    let old = legacy(&legacy(&src, true), false);
    let mut new = src.clone();
    box_blur_n(&mut new, w, h, 1, r);
    let mut before = 0.0f64;
    let mut after = 0.0f64;
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for dy in -(r as isize)..=r as isize {
                for dx in -(r as isize)..=r as isize {
                    let xx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                    let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                    sum += f64::from(src[yy * w + xx]);
                }
            }
            let mean = sum / ((2 * r + 1) * (2 * r + 1)) as f64;
            before = before.max((f64::from(old[y * w + x]) - mean).abs());
            after = after.max((f64::from(new[y * w + x]) - mean).abs());
        }
    }
    eprintln!("D2 field box max absolute error to direct f64 mean: {before:.9e} -> {after:.9e}");
    assert!(after < before && after < 1e-7);
}
