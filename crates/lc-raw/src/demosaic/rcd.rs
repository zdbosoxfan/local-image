//! Ratio Corrected Demosaicing (RCD) for Bayer sensors, by Luis Sanz Rodríguez (v2.3, GPL-3.0).
//!
//! Ported from darktable's `src/iop/demosaicing/rcd.c` (GPL-3.0-or-later; tiling by Ingo
//! Weyrich for RawTherapee, optimisations by Hanno Schwalm), see `docs/PORTS.md`:
//!
//! 1. Vertical/horizontal discrimination from the squared colour-difference high-pass filters.
//! 2. A low-pass filter of the raw data at red and blue sites.
//! 3. Green at red/blue sites: directional estimates corrected by the low-pass ratio, blended by
//!    the (refined) vertical/horizontal discrimination.
//! 4. Red at blue and blue at red along the diagonals (P/Q discrimination), then red/blue at
//!    green sites from the colour differences of the cardinal neighbours.
//!
//! Differences from darktable: the mosaic is read through the mirror-reflected [`Mosaic`] so
//! every pixel is interpolated like the interior (no PPG border), the input is not rescaled (it
//! is already normalised), and the diagonal high-pass filters are read at the site itself
//! (darktable's half-width packing reads the neighbouring column on some rows; the original
//! RCD code reads the site).

use super::Mosaic;
use crate::Rgb32f;
use rayon::prelude::*;

/// Pixels around a tile that are computed but not kept (darktable's `RCD_BORDER`).
const BORDER: usize = 10;
/// Kept size of a tile.
const TILE: usize = 192;
/// Side of a tile's working buffers.
const N: usize = TILE + 2 * BORDER;
const EPS: f32 = 1e-5;
const EPSSQ: f32 = 1e-10;

#[inline]
fn sq(v: f32) -> f32 {
    v * v
}

/// `a·b + (1 − a)·c` (darktable's `interpolatef`), `a` clipped to 0..1.
#[inline]
fn mix(a: f32, b: f32, c: f32) -> f32 {
    let a = a.clamp(0.0, 1.0);
    a * (b - c) + c
}

struct Tile {
    cfa: Vec<f32>,
    rgb: [Vec<f32>; 3],
    vh: Vec<f32>,
    pq: Vec<f32>,
    lpf: Vec<f32>,
    vhpf: Vec<f32>,
    hhpf: Vec<f32>,
    phpf: Vec<f32>,
    qhpf: Vec<f32>,
}

impl Tile {
    fn new() -> Tile {
        let z = || vec![0f32; N * N];
        Tile { cfa: z(), rgb: [z(), z(), z()], vh: z(), pq: z(), lpf: z(), vhpf: z(), hhpf: z(), phpf: z(), qhpf: z() }
    }

    /// Interpolate the tile whose kept area starts at image pixel `(x0, y0)`.
    fn run(&mut self, m: &Mosaic, x0: usize, y0: usize) {
        let (gx, gy) = (x0 as isize - BORDER as isize, y0 as isize - BORDER as isize);
        let fc = |r: usize, c: usize| m.color(gx + c as isize, gy + r as isize);
        let w1 = N;
        let (w2, w3, w4) = (2 * N, 3 * N, 4 * N);
        // Step 0: the (non-negative) samples
        for r in 0..N {
            for c in 0..N {
                let v = m.get(gx + c as isize, gy + r as isize).max(0.0);
                let i = r * N + c;
                self.cfa[i] = v;
                for ch in &mut self.rgb {
                    ch[i] = v;
                }
            }
        }
        let cfa = &self.cfa;
        // Step 1: vertical / horizontal discrimination
        for r in 3..N - 3 {
            for c in 4..N - 4 {
                let i = r * N + c;
                self.vhpf[i] = sq((cfa[i - w3] - cfa[i - w1] - cfa[i + w1] + cfa[i + w3]) - 3.0 * (cfa[i - w2] + cfa[i + w2]) + 6.0 * cfa[i]);
            }
        }
        for r in 4..N - 4 {
            for c in 3..N - 3 {
                let i = r * N + c;
                self.hhpf[i] = sq((cfa[i - 3] - cfa[i - 1] - cfa[i + 1] + cfa[i + 3]) - 3.0 * (cfa[i - 2] + cfa[i + 2]) + 6.0 * cfa[i]);
            }
        }
        for r in 4..N - 4 {
            for c in 4..N - 4 {
                let i = r * N + c;
                let v = (self.vhpf[i - w1] + self.vhpf[i] + self.vhpf[i + w1]).max(EPSSQ);
                let h = (self.hhpf[i - 1] + self.hhpf[i] + self.hhpf[i + 1]).max(EPSSQ);
                self.vh[i] = v / (v + h);
            }
        }
        // Step 2: low-pass filter at red and blue sites
        for r in 2..N - 2 {
            let mut c = 2 + (fc(r, 0) & 1) as usize;
            while c < N - 2 {
                let i = r * N + c;
                self.lpf[i] = cfa[i]
                    + 0.5 * (cfa[i - w1] + cfa[i + w1] + cfa[i - 1] + cfa[i + 1])
                    + 0.25 * (cfa[i - w1 - 1] + cfa[i - w1 + 1] + cfa[i + w1 - 1] + cfa[i + w1 + 1]);
                c += 2;
            }
        }
        let vh_disc = |vh: &[f32], i: usize| {
            let central = vh[i];
            let around = 0.25 * (vh[i - w1 - 1] + vh[i - w1 + 1] + vh[i + w1 - 1] + vh[i + w1 + 1]);
            if (0.5 - central).abs() < (0.5 - around).abs() { around } else { central }
        };
        // Step 3: green at red and blue sites
        for r in 4..N - 4 {
            let mut c = 4 + (fc(r, 0) & 1) as usize;
            while c < N - 4 {
                let i = r * N + c;
                let ci = cfa[i];
                let n_grad = EPS + (cfa[i - w1] - cfa[i + w1]).abs() + (ci - cfa[i - w2]).abs() + (cfa[i - w1] - cfa[i - w3]).abs() + (cfa[i - w2] - cfa[i - w4]).abs();
                let s_grad = EPS + (cfa[i + w1] - cfa[i - w1]).abs() + (ci - cfa[i + w2]).abs() + (cfa[i + w1] - cfa[i + w3]).abs() + (cfa[i + w2] - cfa[i + w4]).abs();
                let w_grad = EPS + (cfa[i - 1] - cfa[i + 1]).abs() + (ci - cfa[i - 2]).abs() + (cfa[i - 1] - cfa[i - 3]).abs() + (cfa[i - 2] - cfa[i - 4]).abs();
                let e_grad = EPS + (cfa[i + 1] - cfa[i - 1]).abs() + (ci - cfa[i + 2]).abs() + (cfa[i + 1] - cfa[i + 3]).abs() + (cfa[i + 2] - cfa[i + 4]).abs();
                let lpf = &self.lpf;
                let l = lpf[i];
                let n_est = cfa[i - w1] * (l + l) / (EPS + l + lpf[i - w2]);
                let s_est = cfa[i + w1] * (l + l) / (EPS + l + lpf[i + w2]);
                let w_est = cfa[i - 1] * (l + l) / (EPS + l + lpf[i - 2]);
                let e_est = cfa[i + 1] * (l + l) / (EPS + l + lpf[i + 2]);
                let v_est = (s_grad * n_est + n_grad * s_est) / (n_grad + s_grad);
                let h_est = (w_grad * e_est + e_grad * w_est) / (e_grad + w_grad);
                self.rgb[1][i] = mix(vh_disc(&self.vh, i), h_est, v_est);
                c += 2;
            }
        }
        // Step 4.0: diagonal colour-difference high-pass filters
        for r in 3..N - 3 {
            for c in 3..N - 3 {
                let i = r * N + c;
                self.phpf[i] =
                    sq((cfa[i - w3 - 3] - cfa[i - w1 - 1] - cfa[i + w1 + 1] + cfa[i + w3 + 3]) - 3.0 * (cfa[i - w2 - 2] + cfa[i + w2 + 2]) + 6.0 * cfa[i]);
                self.qhpf[i] =
                    sq((cfa[i - w3 + 3] - cfa[i - w1 + 1] - cfa[i + w1 - 1] + cfa[i + w3 - 3]) - 3.0 * (cfa[i - w2 + 2] + cfa[i + w2 - 2]) + 6.0 * cfa[i]);
            }
        }
        // Step 4.1: P/Q diagonal discrimination at red and blue sites
        for r in 4..N - 4 {
            let mut c = 4 + (fc(r, 0) & 1) as usize;
            while c < N - 4 {
                let i = r * N + c;
                let p = (self.phpf[i - w1 - 1] + self.phpf[i] + self.phpf[i + w1 + 1]).max(EPSSQ);
                let q = (self.qhpf[i - w1 + 1] + self.qhpf[i] + self.qhpf[i + w1 - 1]).max(EPSSQ);
                self.pq[i] = p / (p + q);
                c += 2;
            }
        }
        // Step 4.2: red at blue sites and blue at red sites
        for r in 4..N - 4 {
            let mut c = 4 + (fc(r, 0) & 1) as usize;
            while c < N - 4 {
                let i = r * N + c;
                let ch = 2 - fc(r, c) as usize;
                let pq = &self.pq;
                let central = pq[i];
                let around = 0.25 * (pq[i - w1 - 1] + pq[i - w1 + 1] + pq[i + w1 - 1] + pq[i + w1 + 1]);
                let disc = if (0.5 - central).abs() < (0.5 - around).abs() { around } else { central };
                let (x, g) = (&self.rgb[ch], &self.rgb[1]);
                let nw_grad = EPS + (x[i - w1 - 1] - x[i + w1 + 1]).abs() + (x[i - w1 - 1] - x[i - w3 - 3]).abs() + (g[i] - g[i - w2 - 2]).abs();
                let ne_grad = EPS + (x[i - w1 + 1] - x[i + w1 - 1]).abs() + (x[i - w1 + 1] - x[i - w3 + 3]).abs() + (g[i] - g[i - w2 + 2]).abs();
                let sw_grad = EPS + (x[i - w1 + 1] - x[i + w1 - 1]).abs() + (x[i + w1 - 1] - x[i + w3 - 3]).abs() + (g[i] - g[i + w2 - 2]).abs();
                let se_grad = EPS + (x[i - w1 - 1] - x[i + w1 + 1]).abs() + (x[i + w1 + 1] - x[i + w3 + 3]).abs() + (g[i] - g[i + w2 + 2]).abs();
                let nw_est = x[i - w1 - 1] - g[i - w1 - 1];
                let ne_est = x[i - w1 + 1] - g[i - w1 + 1];
                let sw_est = x[i + w1 - 1] - g[i + w1 - 1];
                let se_est = x[i + w1 + 1] - g[i + w1 + 1];
                let p_est = (nw_grad * se_est + se_grad * nw_est) / (nw_grad + se_grad);
                let q_est = (ne_grad * sw_est + sw_grad * ne_est) / (ne_grad + sw_grad);
                let v = g[i] + mix(disc, q_est, p_est);
                self.rgb[ch][i] = v;
                c += 2;
            }
        }
        // Step 4.3: red and blue at green sites
        for r in 4..N - 4 {
            let mut c = 4 + (fc(r, 1) & 1) as usize;
            while c < N - 4 {
                let i = r * N + c;
                let disc = vh_disc(&self.vh, i);
                let g = &self.rgb[1];
                let g0 = g[i];
                let n1 = EPS + (g0 - g[i - w2]).abs();
                let s1 = EPS + (g0 - g[i + w2]).abs();
                let w1g = EPS + (g0 - g[i - 2]).abs();
                let e1 = EPS + (g0 - g[i + 2]).abs();
                let (gn, gs, gw, ge) = (g[i - w1], g[i + w1], g[i - 1], g[i + 1]);
                let mut vals = [0f32; 2];
                for (k, ch) in [0usize, 2].into_iter().enumerate() {
                    let x = &self.rgb[ch];
                    let sn = (x[i - w1] - x[i + w1]).abs();
                    let ew = (x[i - 1] - x[i + 1]).abs();
                    let n_grad = n1 + sn + (x[i - w1] - x[i - w3]).abs();
                    let s_grad = s1 + sn + (x[i + w1] - x[i + w3]).abs();
                    let w_grad = w1g + ew + (x[i - 1] - x[i - 3]).abs();
                    let e_grad = e1 + ew + (x[i + 1] - x[i + 3]).abs();
                    let (n_est, s_est, w_est, e_est) = (x[i - w1] - gn, x[i + w1] - gs, x[i - 1] - gw, x[i + 1] - ge);
                    let v_est = (n_grad * s_est + s_grad * n_est) / (n_grad + s_grad);
                    let h_est = (e_grad * w_est + w_grad * e_est) / (e_grad + w_grad);
                    vals[k] = g0 + mix(disc, h_est, v_est);
                }
                self.rgb[0][i] = vals[0];
                self.rgb[2][i] = vals[1];
                c += 2;
            }
        }
    }

    #[inline]
    fn at(&self, r: usize, c: usize) -> [f32; 3] {
        let i = r * N + c;
        [self.rgb[0][i].max(0.0), self.rgb[1][i].max(0.0), self.rgb[2][i].max(0.0)]
    }
}

/// RCD demosaic of a Bayer mosaic (any size ≥ 1×1; the caller sends tiny images to bilinear).
pub(crate) fn rcd(m: &Mosaic) -> Rgb32f {
    let (w, h) = (m.w, m.h);
    let mut out = Rgb32f::new(w, h);
    out.data.par_chunks_mut(TILE * w).enumerate().for_each(|(ty, band)| {
        let y0 = ty * TILE;
        let rows = band.len() / w;
        let mut t = Tile::new();
        let mut x0 = 0;
        while x0 < w {
            let cols = TILE.min(w - x0);
            t.run(m, x0, y0);
            for r in 0..rows {
                for c in 0..cols {
                    band[r * w + x0 + c] = t.at(r + BORDER, c + BORDER);
                }
            }
            x0 += TILE;
        }
    });
    out
}
