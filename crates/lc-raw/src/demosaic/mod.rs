//! Demosaicing: CFA mosaic → camera RGB ([`RgbFrame`]), values as normalised (white = 1.0).
//!
//! Methods (all from published descriptions, see the README):
//! - [`Method::Bilinear`] — average of same-colour neighbours (drafts / thumbnails); works for any pattern.
//! - [`Method::Ppg`] — Patterned Pixel Grouping (Chuan-kai Lin): gradient-selected green, colour-difference R/B.
//! - [`Method::Ahd`] — Adaptive Homogeneity-Directed (Hirakawa & Parks, IEEE TIP 2005): horizontal/vertical
//!   candidates chosen per pixel by CIELab homogeneity. Retained for explicit overrides.
//! - [`Method::Rcd`] — Ratio Corrected Demosaicing (Luis Sanz Rodríguez), ported from darktable (`rcd`):
//!   ratio-corrected directional green, diagonal-discriminated red/blue. Sharp, few artefacts, fast.
//! - [`Method::DualRcd`] — RCD where the image has detail, bilinear in flat areas, blended by a
//!   local-contrast mask ([`DemosaicOptions::dual_threshold`]; darktable's dual demosaic, `dual`).
//! - [`Method::Vng4`] — full four-colour VNG with all 64 gradient terms, ported from darktable.
//! - [`Method::Amaze`] — Emil Martinec's full AMaZE kernel, including Nyquist refinement.
//! - [`Method::DualRcdVng`] / [`Method::DualAmazeVng`] — upstream VNG-linear with two median
//!   colour-smoothing passes in flat areas, RCD/AMaZE in detail; legacy bilinear dual is preserved.
//! - X-Trans uses darktable Markesteijn three-pass (or its dual VNG-linear blend).
//! - LMMSE and DCB are additional Bayer methods; Auto uses measured noise curves.
//! - Other CFA layouts retain generic edge-weighted interpolation.
//!
//! Every method reproduces a constant-colour mosaic exactly.

mod ahd;
mod amaze;
mod bilinear;
mod dcb;
mod dual;
mod lmmse;
mod markesteijn;
mod ppg;
mod rcd;
mod vng;
mod xtrans;

use crate::{Cfa, Normalized, RgbFrame};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Method {
    Bilinear,
    Ppg,
    #[default]
    Ahd,
    /// Ratio Corrected Demosaicing (Bayer; other patterns use the X-Trans method).
    Rcd,
    /// RCD blended with bilinear in flat areas (Bayer only).
    DualRcd,
    /// Full four-colour variable-number-of-gradients demosaic.
    Vng4,
    /// Aliasing Minimization and Zipper Elimination (Bayer).
    Amaze,
    /// AMaZE with smoothed four-colour linear VNG in flat areas.
    DualAmazeVng,
    /// RCD with darktable four-colour linear VNG and colour smoothing in flat areas.
    DualRcdVng,
    /// Choose by measured sensor noise (Bayer) or Markesteijn three-pass (X-Trans).
    Auto,
    Lmmse,
    Dcb,
    Markesteijn3,
    DualMarkesteijnVng,
    /// Original edge-weighted X-Trans method, retained for comparison/override.
    XtransDirectional,
}

/// Parameters of the demosaic methods that have any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DemosaicOptions {
    /// All dual methods: contrast threshold 0..1 below which an area counts as flat
    /// (darktable's default 0.2; 0 = the high-detail method everywhere).
    pub dual_threshold: f32,
}

impl Default for DemosaicOptions {
    fn default() -> Self {
        DemosaicOptions { dual_threshold: 0.2 }
    }
}

/// Mirror-reflected access to a single-plane mosaic (reflection keeps Bayer parity: −k → k, w−1+k → w−1−k).
#[derive(Clone, Copy)]
pub(crate) struct Mosaic<'a> {
    pub w: usize,
    pub h: usize,
    pub data: &'a [f32],
    pub cfa: &'a Cfa,
}

#[inline]
pub(crate) fn reflect(i: isize, n: usize) -> usize {
    if i >= 0 && (i as usize) < n {
        return i as usize;
    }
    let n = n as isize;
    if n == 1 {
        return 0;
    }
    let period = 2 * (n - 1);
    let mut i = i.rem_euclid(period);
    if i >= n {
        i = period - i;
    }
    i as usize
}

/// `v` clamped between `a` and `b`: `v.clamp(a.min(b), a.max(b))`, except that it returns `v` as is when both
/// bounds are NaN (NaN samples in a float raw), where `f32::clamp` panics.
#[inline]
pub(crate) fn clamp_between(v: f32, a: f32, b: f32) -> f32 {
    let (lo, hi) = (a.min(b), a.max(b));
    if lo <= hi { v.clamp(lo, hi) } else { v }
}

impl Mosaic<'_> {
    #[inline]
    pub fn get(&self, x: isize, y: isize) -> f32 {
        self.data[reflect(y, self.h) * self.w + reflect(x, self.w)]
    }
    #[inline]
    pub fn color(&self, x: isize, y: isize) -> u8 {
        self.cfa.color_at(reflect(x, self.w), reflect(y, self.h))
    }
}

/// Demosaic normalised raw data. `cpp == 3` data is returned as is; monochrome data is replicated to grey.
pub fn demosaic(n: &Normalized, method: Method) -> RgbFrame {
    demosaic_with(n, method, &DemosaicOptions::default())
}

/// [`demosaic`] with the methods' parameters.
pub fn demosaic_with(n: &Normalized, method: Method, opts: &DemosaicOptions) -> RgbFrame {
    let (w, h) = (n.width, n.height);
    if w == 0 || h == 0 {
        return RgbFrame::new(w, h);
    }
    match (&n.cfa, n.cpp) {
        (_, 3) => RgbFrame { width: w, height: h, data: n.data.as_chunks::<3>().0.iter().map(|c| [c[0], c[1], c[2]]).collect() },
        // four colours develop from their first three; narrower data (refused by `RawImage::validate`)
        // repeats its last sample instead of reading past the pixel
        (_, cpp) if cpp != 1 => RgbFrame {
            width: w,
            height: h,
            data: n
                .data
                .chunks_exact(cpp.max(1))
                .map(|c| {
                    let s = |i: usize| c.get(i).or(c.last()).copied().unwrap_or(0.0);
                    [s(0), s(1), s(2)]
                })
                .collect(),
        },
        (None, _) => RgbFrame { width: w, height: h, data: n.data.iter().map(|&v| [v; 3]).collect() },
        (Some(cfa), _) => {
            let m = Mosaic { w, h, data: &n.data, cfa };
            if !cfa.is_bayer() {
                return match method {
                    Method::Bilinear => bilinear::bilinear(&m),
                    Method::XtransDirectional => xtrans::directional(&m),
                    _ if markesteijn::supported(&m) => {
                        let high = markesteijn::markesteijn(&m);
                        if matches!(method, Method::DualMarkesteijnVng | Method::DualRcd | Method::DualRcdVng | Method::DualAmazeVng) {
                            dual::dual_vng(&m, high, opts.dual_threshold)
                        } else {
                            high
                        }
                    }
                    _ => xtrans::directional(&m),
                };
            }
            if w < 4 || h < 4 {
                return bilinear::bilinear(&m);
            }
            let auto = method == Method::Auto;
            let method = if auto { automatic_method(n) } else { method };
            let auto_opts = DemosaicOptions::default();
            let opts = if auto { &auto_opts } else { opts };
            match method {
                Method::Lmmse => lmmse::lmmse(&m),
                Method::Dcb => dcb::dcb(&m),
                Method::Markesteijn3 | Method::XtransDirectional => rcd::rcd(&m),
                Method::DualMarkesteijnVng => dual::dual_vng(&m, rcd::rcd(&m), opts.dual_threshold),
                Method::Bilinear => bilinear::bilinear(&m),
                Method::Ppg => ppg::ppg(&m),
                Method::Ahd => ahd::ahd(&m),
                Method::Auto | Method::Rcd => rcd::rcd(&m),
                Method::DualRcd => dual::dual(&m, rcd::rcd(&m), opts.dual_threshold),
                Method::Vng4 => vng::vng(&m, false),
                Method::Amaze => amaze::amaze(&m),
                Method::DualAmazeVng => dual::dual_vng(&m, amaze::amaze(&m), opts.dual_threshold),
                Method::DualRcdVng => dual::dual_vng(&m, rcd::rcd(&m), opts.dual_threshold),
            }
        }
    }
}

/// Robust noise estimate on same-colour second differences. Lowest quartile rejects
/// edges/texture; scale by sqrt(6) and Gaussian MAD. Sensor-relative, so ISO and
/// camera gain differences are reflected in the actual normalized noise.
pub fn noise_sigma(n: &Normalized) -> f32 {
    let Some(cfa) = &n.cfa else {
        return 0.0;
    };
    if !cfa.is_bayer() || n.cpp != 1 || n.width < 8 || n.height < 8 {
        return 0.0;
    }
    use rayon::prelude::*;
    let positions: Vec<(usize, usize)> = (2..n.height - 3).step_by(32).flat_map(|y| (2..n.width - 3).step_by(32).map(move |x| (x, y))).collect();
    let mut blocks: Vec<f32> = positions
        .par_iter()
        .map(|&(bx, by)| {
            let mut v = Vec::with_capacity(2048);
            for y in by..(by + 32).min(n.height - 2) {
                for x in bx..(bx + 32).min(n.width - 2) {
                    let i = y * n.width + x;
                    v.push((n.data[i - 2] - 2.0 * n.data[i] + n.data[i + 2]).abs());
                    v.push((n.data[i - 2 * n.width] - 2.0 * n.data[i] + n.data[i + 2 * n.width]).abs());
                }
            }
            v.sort_unstable_by(f32::total_cmp);
            v[v.len() / 2] / 1.652
        })
        .collect();
    blocks.sort_unstable_by(f32::total_cmp);
    if blocks.is_empty() { 0.0 } else { blocks[blocks.len() / 4] }
}
/// Measured noisy re-mosaic crossover: RCD below sigma 0.0035, dual RCD/VNG
/// above (dual threshold 0.2). The bracket [0.002, 0.005] comes from eight CC0
/// natural crops, not an ISO assumption. See the quality harness/report.
pub fn automatic_method(n: &Normalized) -> Method {
    if n.cfa.as_ref().is_some_and(|c| !c.is_bayer()) {
        return Method::Markesteijn3;
    }
    if noise_sigma(n) >= 0.0035 { Method::DualRcdVng } else { Method::Rcd }
}

/// Build a mosaic from an RGB image by sampling each pixel's CFA colour (for tests and synthetic data).
pub fn mosaic_from_rgb(img: &RgbFrame, cfa: &Cfa) -> Normalized {
    let data = (0..img.height).flat_map(|y| (0..img.width).map(move |x| (x, y))).map(|(x, y)| img.get(x, y)[cfa.color_at(x, y) as usize]).collect();
    Normalized { width: img.width, height: img.height, cpp: 1, data, cfa: Some(cfa.clone()) }
}

/// Peak signal-to-noise ratio (dB, peak 1.0) between two images over an interior region (`border` pixels excluded).
pub fn psnr(a: &RgbFrame, b: &RgbFrame, border: usize) -> f64 {
    let mut se = 0.0f64;
    let mut n = 0usize;
    for y in border..a.height.saturating_sub(border) {
        for x in border..a.width.saturating_sub(border) {
            let (p, q) = (a.get(x, y), b.get(x, y));
            for c in 0..3 {
                se += ((p[c] - q[c]) as f64).powi(2);
                n += 1;
            }
        }
    }
    if n == 0 || se == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (1.0 / (se / n as f64)).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smooth_scene(w: usize, h: usize) -> RgbFrame {
        RgbFrame::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            [0.2 + 0.6 * fx * fy, 0.3 + 0.4 * (fx * 3.0).sin().abs() * 0.5 + 0.2 * fy, 0.1 + 0.5 * (1.0 - fx) * (0.5 + 0.5 * (fy * 5.0).cos())]
        })
    }

    /// A photo-like scene: sharp luminance structure (disc, stripes, diagonal edge, fine texture) modulating a
    /// smoothly varying colour — edges are mostly luminance edges, as in natural images.
    fn edgy_scene(w: usize, h: usize) -> RgbFrame {
        RgbFrame::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32, y as f32);
            let d = ((fx - w as f32 * 0.35).powi(2) + (fy - h as f32 * 0.4).powi(2)).sqrt();
            let lum = if d < w as f32 * 0.18 {
                0.85
            } else if x > w * 2 / 3 && (y / 3) % 2 == 0 {
                0.15
            } else if fx + fy * 0.5 > w as f32 * 1.1 {
                0.7
            } else {
                0.35 + 0.1 * ((fx * 0.7).sin() * (fy * 0.9).cos())
            };
            let tint = [0.9 + 0.1 * (fx / w as f32), 0.8, 0.55 + 0.3 * (fy / h as f32)];
            [lum * tint[0], lum * tint[1], lum * tint[2]]
        })
    }

    const ALL: [Method; 11] = [
        Method::Lmmse,
        Method::Dcb,
        Method::Bilinear,
        Method::Ppg,
        Method::Ahd,
        Method::Rcd,
        Method::DualRcd,
        Method::Vng4,
        Method::Amaze,
        Method::DualRcdVng,
        Method::DualAmazeVng,
    ];

    fn run(img: &RgbFrame, cfa: &Cfa, m: Method) -> RgbFrame {
        demosaic(&mosaic_from_rgb(img, cfa), m)
    }

    #[test]
    fn constant_colour_is_exact_for_all_methods_and_phases() {
        let img = RgbFrame::filled(17, 13, [0.3, 0.55, 0.8]);
        for pat in ["RGGB", "BGGR", "GRBG", "GBRG"] {
            let cfa = Cfa::bayer(pat).unwrap();
            for m in ALL {
                let out = run(&img, &cfa, m);
                for p in &out.data {
                    for c in 0..3 {
                        assert!((p[c] - img.data[0][c]).abs() < 1e-5, "{pat} {m:?} {p:?}");
                    }
                }
            }
        }
        let out = run(&RgbFrame::filled(19, 14, [0.3, 0.55, 0.8]), &Cfa::xtrans(), Method::Ahd);
        assert!(out.data.iter().all(|p| (p[0] - 0.3).abs() < 1e-5 && (p[1] - 0.55).abs() < 1e-5 && (p[2] - 0.8).abs() < 1e-5));
    }

    #[test]
    fn known_samples_are_preserved() {
        let img = edgy_scene(40, 32);
        let cfa = Cfa::bayer("GRBG").unwrap();
        for m in ALL {
            // VNG deliberately merges the two green phases; dual blending smooths known samples.
            if matches!(m, Method::Vng4 | Method::DualRcdVng | Method::DualAmazeVng) {
                continue;
            }
            let out = run(&img, &cfa, m);
            for y in 0..32 {
                for x in 0..40 {
                    let c = cfa.color_at(x, y) as usize;
                    assert!((out.get(x, y)[c] - img.get(x, y)[c]).abs() < 1e-6, "{m:?}");
                }
            }
        }
    }

    #[test]
    fn psnr_thresholds_and_ranking() {
        let smooth = smooth_scene(96, 80);
        let edgy = edgy_scene(96, 80);
        let cfa = Cfa::bayer("RGGB").unwrap();
        let p = |img: &RgbFrame, m| psnr(img, &run(img, &cfa, m), 4);
        let (sb, sp, sa) = (p(&smooth, Method::Bilinear), p(&smooth, Method::Ppg), p(&smooth, Method::Ahd));
        assert!(sb > 38.0 && sp > 38.0 && sa > 38.0, "smooth: bilinear {sb:.1} ppg {sp:.1} ahd {sa:.1}");
        let (eb, ep, ea) = (p(&edgy, Method::Bilinear), p(&edgy, Method::Ppg), p(&edgy, Method::Ahd));
        eprintln!("smooth: bilinear {sb:.1} ppg {sp:.1} ahd {sa:.1}; edgy: bilinear {eb:.1} ppg {ep:.1} ahd {ea:.1}");
        assert!(eb > 20.0, "edgy bilinear {eb:.1}");
        assert!(ep > eb + 6.0, "ppg {ep:.1} vs bilinear {eb:.1}");
        assert!(ea > eb + 6.0, "ahd {ea:.1} vs bilinear {eb:.1}");
        let xt = Cfa::xtrans();
        let xp = psnr(&edgy, &run(&edgy, &xt, Method::Ahd), 6);
        let xb = psnr(&edgy, &run(&edgy, &xt, Method::Bilinear), 6);
        assert!(xp > 20.0 && xp >= xb - 0.5, "xtrans directional {xp:.1} bilinear {xb:.1}");
    }

    #[test]
    fn rcd_and_dual_quality() {
        let edgy = edgy_scene(96, 80);
        let cfa = Cfa::bayer("GBRG").unwrap();
        let p = |m| psnr(&edgy, &run(&edgy, &cfa, m), 4);
        let (b, a, r, d) = (p(Method::Bilinear), p(Method::Ahd), p(Method::Rcd), p(Method::DualRcd));
        eprintln!("edgy: bilinear {b:.1} ahd {a:.1} rcd {r:.1} dual {d:.1}");
        assert!(r > b + 6.0, "rcd {r:.1} vs bilinear {b:.1}");
        assert!(r > a - 1.5, "rcd {r:.1} vs ahd {a:.1}");
        assert!(d >= b && d <= r + 0.5, "dual {d:.1} between bilinear {b:.1} and rcd {r:.1}");
        // a threshold of 0 is RCD itself
        let n = mosaic_from_rgb(&edgy, &cfa);
        let rcd = demosaic(&n, Method::Rcd);
        assert_eq!(demosaic_with(&n, Method::DualRcd, &DemosaicOptions { dual_threshold: 0.0 }).data, rcd.data);
        // flat noise takes the smooth method, edges keep RCD
        let flat = RgbFrame::from_fn(64, 64, |x, y| {
            let k = ((x * 7 + y * 13) % 5) as f32 * 0.0004;
            if x < 32 { [0.30 + k, 0.31 + k, 0.29 + k] } else { [0.8, 0.8, 0.8] }
        });
        let mask = dual::detail_mask(&demosaic(&mosaic_from_rgb(&flat, &cfa), Method::Rcd), 0.2);
        assert!(mask.get(31, 32) > 0.3 && mask.get(10, 32) < 0.05, "{} {}", mask.get(10, 32), mask.get(31, 32));
    }

    /// RCD works in tiles: no seams where they meet, at any phase.
    #[test]
    fn rcd_tiles_have_no_seams() {
        let img = smooth_scene(450, 401);
        for pat in ["RGGB", "BGGR", "GRBG", "GBRG"] {
            let cfa = Cfa::bayer(pat).unwrap();
            let out = run(&img, &cfa, Method::Rcd);
            assert!(psnr(&img, &out, 0) > 38.0, "{pat}: {:.1}", psnr(&img, &out, 0));
            // across the tile boundaries (x = 192, y = 192, 384) as good as elsewhere
            for (x, y) in [(191, 100), (192, 100), (100, 191), (100, 192), (300, 384), (384, 383)] {
                for c in 0..3 {
                    assert!((out.get(x, y)[c] - img.get(x, y)[c]).abs() < 0.01, "{pat} ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn tiny_and_degenerate_inputs() {
        let cfa = Cfa::bayer("RGGB").unwrap();
        for (w, h) in [(1, 1), (2, 2), (3, 5), (5, 3), (1, 7)] {
            let img = RgbFrame::filled(w, h, [0.5; 3]);
            for m in ALL {
                let out = run(&img, &cfa, m);
                assert_eq!((out.width, out.height), (w, h));
                assert!(out.data.iter().all(|p| p.iter().all(|v| v.is_finite())));
            }
        }
        let n = Normalized { width: 2, height: 1, cpp: 1, data: vec![0.2, 0.4], cfa: None };
        assert_eq!(demosaic(&n, Method::Ahd).data, vec![[0.2; 3], [0.4; 3]]);
        let n = Normalized { width: 1, height: 1, cpp: 3, data: vec![0.1, 0.2, 0.3], cfa: None };
        assert_eq!(demosaic(&n, Method::Ahd).data, vec![[0.1, 0.2, 0.3]]);
    }

    #[test]
    fn non_finite_samples_do_not_panic() {
        // regression: NaN samples (a float DNG) gave AHD and PPG NaN bounds for `f32::clamp`, which panics
        let cfa = Cfa::bayer("RGGB").unwrap();
        let mut patchy = mosaic_from_rgb(&edgy_scene(24, 20), &cfa);
        for (i, v) in patchy.data.iter_mut().enumerate() {
            let (x, y) = (i % 24, i / 24);
            if (8..12).contains(&x) && (6..10).contains(&y) {
                *v = f32::NAN;
            } else if i % 7 == 0 {
                *v = if i % 2 == 0 { f32::INFINITY } else { f32::NEG_INFINITY };
            }
        }
        let all_nan = Normalized { width: 9, height: 7, cpp: 1, data: vec![f32::NAN; 63], cfa: Some(cfa) };
        for n in [&patchy, &all_nan] {
            for m in [Method::Bilinear, Method::Ppg, Method::Ahd] {
                let out = demosaic(n, m);
                assert_eq!((out.width, out.height), (n.width, n.height), "{m:?}");
            }
        }
        // without NaN bounds it is the `f32::clamp` it replaced, bit for bit (finite results stay identical)
        let vals = [f32::NEG_INFINITY, -3e38, -2.5, -0.0, 0.0, 1e-40, 0.5, 1.0, 3e38, f32::INFINITY];
        for v in vals {
            for a in vals {
                for b in vals {
                    assert_eq!(clamp_between(v, a, b).to_bits(), v.clamp(a.min(b), a.max(b)).to_bits(), "{v} {a} {b}");
                }
            }
        }
    }

    #[test]
    fn reflect_indices() {
        assert_eq!(reflect(-1, 5), 1);
        assert_eq!(reflect(-2, 5), 2);
        assert_eq!(reflect(5, 5), 3);
        assert_eq!(reflect(6, 5), 2);
        assert_eq!(reflect(3, 1), 0);
        assert_eq!(reflect(-7, 2), 1);
    }

    #[test]
    #[ignore]
    fn throughput() {
        let (w, h) = (6000, 4000);
        let img = smooth_scene(w, h);
        let cfa = Cfa::bayer("RGGB").unwrap();
        let n = mosaic_from_rgb(&img, &cfa);
        for m in ALL {
            let t = std::time::Instant::now();
            let _ = demosaic(&n, m);
            let s = t.elapsed().as_secs_f64();
            println!("{m:?}: {:.1} MP/s ({:.0} ms for 24 MP)", (w * h) as f64 / 1e6 / s, s * 1e3);
        }
        let n = mosaic_from_rgb(&img, &Cfa::xtrans());
        let t = std::time::Instant::now();
        let _ = demosaic(&n, Method::Ahd);
        println!("X-Trans directional: {:.1} MP/s", (w * h) as f64 / 1e6 / t.elapsed().as_secs_f64());
    }
}

#[cfg(test)]
mod tests_new;
