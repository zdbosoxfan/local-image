//! Exact squared Euclidean distance to background pixel centres (exterior included).
//! Original Rust implementation of Felzenszwalb–Huttenlocher, §2, Algorithms 1–2:
//! https://cs.brown.edu/people/pfelzens/papers/dt-final.pdf
//! A separable lower envelope of parabolas, O(pixels), with O(max(width,height)) scratch.
use super::Control;
use crate::Error;

pub(super) struct Field {
    pub squared: Vec<u32>,
    pub sites: Vec<(i32, i32)>,
}
pub(super) fn squared(mask: &[bool], w: usize, h: usize, c: &mut Control<'_, impl Fn() -> bool>) -> Result<Field, Error> {
    let mut d = vec![0u32; mask.len()];
    let mut nearest = vec![(0, 0); mask.len()];
    // Vertical nearest background, including virtual rows -1 and h. These
    // finite costs avoid infinity-infinity in the horizontal envelope pass.
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if i.is_multiple_of(1024) {
                c.work(1024)?;
            }
            if mask[i] {
                let above = if y == 0 { 0 } else { d[i - w] };
                d[i] = above + 1;
                nearest[i] = if y == 0 { (x as i32, -1) } else { nearest[i - w] };
            } else {
                nearest[i] = (x as i32, y as i32);
            }
        }
    }
    for y in (0..h).rev() {
        for x in 0..w {
            let i = y * w + x;
            if i.is_multiple_of(1024) {
                c.work(1024)?;
            }
            if mask[i] {
                let below = if y == h - 1 { 0 } else { d[i + w] };
                if below + 1 < d[i] {
                    d[i] = below + 1;
                    nearest[i] = if y == h - 1 { (x as i32, h as i32) } else { nearest[i + w] };
                }
            }
        }
    }
    let mut f = vec![0f64; w + 2];
    let mut sites = vec![0usize; w + 2];
    let mut rows = vec![0i32; w + 2];
    let mut bounds = vec![0f64; w + 3];
    for y in 0..h {
        c.poll()?;
        rows[0] = y as i32;
        rows[w + 1] = y as i32;
        for x in 0..w {
            if x.is_multiple_of(1024) {
                c.work(1024)?;
            }
            let v = f64::from(d[y * w + x]);
            f[x + 1] = v * v;
            rows[x + 1] = nearest[y * w + x].1;
        }
        let mut k = 0;
        sites[0] = 0;
        bounds[0] = f64::NEG_INFINITY;
        bounds[1] = f64::INFINITY;
        for q in 1..w + 2 {
            if q.is_multiple_of(1024) {
                c.work(1024)?;
            }
            let intersection = |v: usize| (f[q] + (q * q) as f64 - f[v] - (v * v) as f64) / (2. * (q - v) as f64);
            let mut s = intersection(sites[k]);
            while s <= bounds[k] && k > 0 {
                k -= 1;
                s = intersection(sites[k]);
            }
            k += 1;
            sites[k] = q;
            bounds[k] = s;
            bounds[k + 1] = f64::INFINITY;
        }
        k = 0;
        for q in 1..w + 1 {
            if q.is_multiple_of(1024) {
                c.work(1024)?;
            }
            while bounds[k + 1] < q as f64 {
                k += 1;
            }
            let delta = q as f64 - sites[k] as f64;
            d[y * w + q - 1] = (delta * delta + f[sites[k]]) as u32;
            nearest[y * w + q - 1] = (sites[k] as i32 - 1, rows[sites[k]]);
        }
    }
    Ok(Field { squared: d, sites: nearest })
}
