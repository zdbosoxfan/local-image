//! Gaussian-like blur by three successive box filters (running sums, O(1) per pixel per pass).

use crate::resample::Pixel;
use crate::{Image, par_rows};

/// Box radii approximating a Gaussian of `sigma` with 3 passes (Kovesi / Wells).
pub fn box_radii(sigma: f32) -> [usize; 3] {
    let n = 3.0f32;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round() as i32;
    std::array::from_fn(|i| (((if (i as i32) < m { wl } else { wu }) - 1) / 2).max(0) as usize)
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

/// Rows per parallel band in the vertical pass (each band re-primes its running sums).
pub const BAND: usize = 32;

/// One vertical box pass `src → dst` (clamped edges): a running sum per column, vectorized across
/// the row, parallel over horizontal bands. Cache-friendly — no transposes.
fn box_v<T: Pixel>(src: &Image<T>, dst: &mut Image<T>, r: usize) {
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

/// Blur with Gaussian `sigma` (pixels). `sigma <= 0.3` returns a copy.
pub fn gaussian<T: Pixel>(img: &Image<T>, sigma: f32) -> Image<T> {
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
    let mut b = Image::<T>::new(img.width, img.height);
    for r in radii {
        if r == 0 {
            continue;
        }
        box_v(&a, &mut b, r);
        std::mem::swap(&mut a, &mut b);
    }
    a
}

/// Below this sigma [`gaussian_fine`] convolves with the true kernel; above it the three-box
/// approximation of [`gaussian`] is already accurate.
pub const FINE_SIGMA: f32 = 2.0;

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
pub fn gaussian_fine<T: Pixel>(img: &Image<T>, sigma: f32) -> Image<T> {
    if sigma >= FINE_SIGMA {
        return gaussian(img, sigma);
    }
    if sigma <= 0.05 || img.is_empty() {
        return img.clone();
    }
    let taps = gauss_taps(sigma);
    let (w, h) = (img.width, img.height);
    let mut a = Image::<T>::new(w, h);
    par_rows(&mut a.data, w, |y, row| conv_row(&img.data[y * w..(y + 1) * w], row, &taps));
    let mut b = Image::<T>::new(w, h);
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

/// Minimum over the `(2r+1)²` square around each pixel (clamped to the image), separable.
pub fn min_filter(img: &Image<f32>, r: usize) -> Image<f32> {
    let (w, h) = (img.width, img.height);
    if r == 0 || img.is_empty() {
        return img.clone();
    }
    let mut a = Image::<f32>::new(w, h);
    par_rows(&mut a.data, w, |y, row| {
        let src = &img.data[y * w..(y + 1) * w];
        for (x, o) in row.iter_mut().enumerate() {
            let (x0, x1) = (x.saturating_sub(r), (x + r).min(w - 1));
            *o = src[x0..=x1].iter().fold(f32::INFINITY, |m, v| m.min(*v));
        }
    });
    let mut b = Image::<f32>::new(w, h);
    par_rows(&mut b.data, w, |y, row| {
        let (y0, y1) = (y.saturating_sub(r), (y + r).min(h - 1));
        row.copy_from_slice(&a.data[y0 * w..(y0 + 1) * w]);
        for yy in y0 + 1..=y1 {
            for (o, v) in row.iter_mut().zip(&a.data[yy * w..(yy + 1) * w]) {
                *o = o.min(*v);
            }
        }
    });
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fine_gaussian_keeps_variance_and_mass_at_small_sigma() {
        for sigma in [0.3f32, 0.5, 0.8, 1.2, 1.9] {
            let mut imp = Image::<f32>::new(41, 1);
            imp.set(20, 0, 1.0);
            let b = gaussian_fine(&imp, sigma);
            let sum: f32 = b.data.iter().sum();
            let var: f32 = b.data.iter().enumerate().map(|(i, v)| v * (i as f32 - 20.0).powi(2)).sum();
            assert!((sum - 1.0).abs() < 1e-5, "{sigma}: {sum}");
            assert!((var.sqrt() - sigma).abs() / sigma < 0.05, "{sigma}: {}", var.sqrt());
        }
        // the box approximation can't blur at 0.5 px (it rounds to a copy); the true kernel does
        let mut imp = Image::<f32>::new(9, 9);
        imp.set(4, 4, 1.0);
        assert_eq!(gaussian(&imp, 0.5).get(4, 4), 1.0);
        assert!(gaussian_fine(&imp, 0.5).get(4, 4) < 0.6);
    }

    #[test]
    fn min_filter_takes_the_window_minimum() {
        let img = Image::<f32>::from_fn(20, 10, |x, y| ((x * 3 + y * 7) % 11) as f32);
        let m = min_filter(&img, 2);
        for y in 0..10 {
            for x in 0..20 {
                let mut want = f32::INFINITY;
                for yy in y.saturating_sub(2)..=(y + 2).min(9) {
                    for xx in x.saturating_sub(2)..=(x + 2).min(19) {
                        want = want.min(img.get(xx, yy));
                    }
                }
                assert_eq!(m.get(x, y), want);
            }
        }
    }

    #[test]
    fn preserves_constant_and_mass() {
        let img = Image::<f32>::filled(50, 30, 0.7);
        let b = gaussian(&img, 4.0);
        assert!(b.data.iter().all(|v| (v - 0.7).abs() < 1e-5));
        let mut imp = Image::<f32>::new(101, 101);
        imp.set(50, 50, 1.0);
        let b = gaussian(&imp, 5.0);
        let sum: f32 = b.data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "{sum}");
        // peak moved to the centre and symmetric
        assert!((b.get(45, 50) - b.get(55, 50)).abs() < 1e-6);
        assert!((b.get(50, 45) - b.get(50, 55)).abs() < 1e-6);
    }

    #[test]
    fn variance_close_to_sigma_squared() {
        let mut imp = Image::<f32>::new(201, 1);
        imp.set(100, 0, 1.0);
        let sigma = 8.0;
        let b = gaussian(&imp, sigma);
        let var: f32 = b.data.iter().enumerate().map(|(i, v)| v * ((i as f32 - 100.0).powi(2))).sum();
        assert!((var.sqrt() - sigma).abs() / sigma < 0.1, "{}", var.sqrt());
    }
}
