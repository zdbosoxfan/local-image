//! "Inpaint opposed" highlight reconstruction, the linear (demosaiced data) variant: a clipped
//! channel becomes the mean of the two *opposed* channels (averaged in cube-root space) plus a
//! global chrominance offset measured on the unclipped pixels just around the clipped areas.
//!
//! Ported from darktable's `src/iop/hlreconstruct/opposed.c` (`_process_linear_opposed`;
//! GPL-3.0-or-later; algorithm by Hanno Schwalm with @garagecoder and @Iain of the G'MIC team),
//! see `docs/PORTS.md`. Differences: it runs on camera RGB before white balance (the multipliers
//! are applied and removed here), the clipped-channel test reads each channel (darktable's loop
//! reads the first channel for all three), and the superpixel grid rounds up.

use crate::Rgb32f;
use rayon::prelude::*;

/// Mean of the two channels other than `c`, in cube-root space (darktable's `_calc_linear_refavg`).
#[inline]
fn refavg(p: [f32; 3], c: usize) -> f32 {
    let r = p.map(|v| v.max(0.0).cbrt());
    let opp = match c {
        0 => 0.5 * (r[1] + r[2]),
        1 => 0.5 * (r[0] + r[2]),
        _ => 0.5 * (r[0] + r[1]),
    };
    opp * opp * opp
}

/// Offsets (in 3×3 superpixels) of darktable's `_mask_dilated` neighbourhood.
fn dilation_offsets() -> Vec<(isize, isize)> {
    let mut v = Vec::new();
    for dy in -3isize..=3 {
        for dx in -3isize..=3 {
            let keep = match dy.abs() {
                0 | 1 => true,
                2 => true,
                _ => dx.abs() <= 2,
            };
            if keep {
                v.push((dx, dy));
            }
        }
    }
    v
}

/// Rebuild clipped channels of `img` (camera RGB, white = 1.0, not white balanced). `wb` are the
/// white-balance multipliers applied later, `clip` the sensor clip level (e.g. 0.99). Returns the
/// number of pixels with a clipped channel; pixels without one are not changed.
pub fn opposed(img: &mut Rgb32f, wb: [f32; 3], clip: f32) -> usize {
    let (w, h) = (img.width, img.height);
    let wb = wb.map(|v| if v.is_finite() && v > 0.0 { v } else { 1.0 });
    let clips = [clip * wb[0], clip * wb[1], clip * wb[2]];
    let balanced = |p: [f32; 3]| [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
    let count = img.data.par_iter().filter(|p| (0..3).any(|c| p[c] * wb[c] >= clips[c])).count();
    if count == 0 {
        return 0;
    }
    // clipped photosites per channel on a grid of 3×3 superpixels, then dilated
    let (mw, mh) = (w.div_ceil(3), h.div_ceil(3));
    let mut mask = vec![[false; 3]; mw * mh];
    for y in 0..h {
        for x in 0..w {
            let q = balanced(img.data[y * w + x]);
            let m = &mut mask[(y / 3) * mw + x / 3];
            for c in 0..3 {
                m[c] |= q[c] >= clips[c];
            }
        }
    }
    let offs = dilation_offsets();
    let dilated: Vec<[bool; 3]> = (0..mw * mh)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % mw) as isize, (i / mw) as isize);
            let mut d = [false; 3];
            for &(dx, dy) in &offs {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < mw && (ny as usize) < mh {
                    let m = mask[ny as usize * mw + nx as usize];
                    for c in 0..3 {
                        d[c] |= m[c];
                    }
                }
            }
            d
        })
        .collect();
    // chrominance: how far the unclipped values near clipped areas sit from their opposed mean
    let (sums, cnts) = (3..h.saturating_sub(3))
        .into_par_iter()
        .map(|y| {
            let (mut s, mut n) = ([0f64; 3], [0f64; 3]);
            for x in 3..w.saturating_sub(3) {
                let q = balanced(img.data[y * w + x]);
                let d = dilated[(y / 3) * mw + x / 3];
                for c in 0..3 {
                    if q[c] > 0.2 * clips[c] && q[c] < clips[c] && d[c] {
                        s[c] += (q[c] - refavg(q, c)) as f64;
                        n[c] += 1.0;
                    }
                }
            }
            (s, n)
        })
        .reduce(|| ([0f64; 3], [0f64; 3]), |a, b| (std::array::from_fn(|c| a.0[c] + b.0[c]), std::array::from_fn(|c| a.1[c] + b.1[c])));
    let chroma: [f32; 3] = std::array::from_fn(|c| if cnts[c] > 30.0 { (sums[c] / cnts[c]) as f32 } else { 0.0 });
    img.data.par_iter_mut().for_each(|px| {
        let q = balanced(*px);
        if !(0..3).any(|c| q[c] >= clips[c]) {
            return;
        }
        let mut o = q;
        for c in 0..3 {
            let v = q[c].max(0.0);
            if v >= clips[c] {
                o[c] = v.max(refavg(q, c) + chroma[c]);
            }
        }
        *px = [o[0] / wb[0], o[1] / wb[1], o[2] / wb[2]];
    });
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warm_gradient() -> Rgb32f {
        Rgb32f::from_fn(96, 48, |x, _| {
            let v = 0.3 + 1.2 * x as f32 / 95.0;
            [v, v * 0.6, v * 0.35]
        })
    }

    #[test]
    fn rebuilds_a_clipped_channel_from_the_opposed_ones() {
        let truth = warm_gradient();
        let mut img = truth.map(|p| p.map(|v| v.min(1.0)));
        let err = |img: &Rgb32f| img.data.iter().zip(&truth.data).map(|(a, b)| (a[0] - b[0]).abs()).sum::<f32>();
        let before = err(&img);
        let clipped = img.clone();
        assert!(opposed(&mut img, [1.0; 3], 0.999) > 0);
        let after = err(&img);
        assert!(after < before * 0.8, "before {before} after {after}");
        // unclipped pixels untouched, clipped values never go down, red keeps rising
        for (a, b) in img.data.iter().zip(&clipped.data) {
            if b[0] < 0.999 {
                assert_eq!(a, b);
            }
            assert!(a[0] >= b[0]);
        }
        let row: Vec<f32> = (0..96).map(|x| img.get(x, 24)[0]).collect();
        assert!(row.windows(2).all(|p| p[1] >= p[0] - 1e-4), "{row:?}");
    }

    #[test]
    fn white_balance_scales_the_clip_levels() {
        // only blue clips once white balanced: a small blue multiplier keeps it below its clip
        let mut img = Rgb32f::filled(30, 30, [0.5, 0.5, 1.0]);
        assert_eq!(opposed(&mut img, [2.0, 1.0, 1.5], 0.99), 900);
        let p = img.data[0];
        // blue (white balanced 1.5) is rebuilt from red/green (1.0, 0.5): at least its clip level
        assert!(p[2] >= 1.0 - 1e-6);
        assert_eq!((p[0], p[1]), (0.5, 0.5));
        let mut none = Rgb32f::filled(8, 8, [0.4; 3]);
        assert_eq!(opposed(&mut none, [2.0, 1.0, 1.5], 0.99), 0);
        assert_eq!(opposed(&mut Rgb32f::new(0, 0), [1.0; 3], 0.99), 0);
        // fully clipped stays bright and finite
        let mut white = Rgb32f::filled(12, 12, [1.0; 3]);
        opposed(&mut white, [2.0, 1.0, 1.5], 0.99);
        assert!(white.data.iter().all(|p| p.iter().all(|v| v.is_finite() && *v >= 0.99)));
    }
}

#[cfg(test)]
mod refvec_tests {
    use super::*;
    #[test]
    fn upstream_opposed_and_channel_test_difference() {
        let mut img = Rgb32f::from_fn(96, 96, |x, y| {
            let v = 0.4 + 0.002 * x as f32 + 0.001 * y as f32;
            let mut p = [v * 0.8, v, v * 1.2];
            let (x, y) = (x as i32, y as i32);
            if (x - 48) * (x - 48) + (y - 48) * (y - 48) < 21 * 21 {
                p[1] = 2.2;
                p[2] = 1.05;
            }
            p
        });
        opposed(&mut img, [1.0; 3], 0.99);
        let actual: Vec<_> = img.data.iter().step_by(3).flatten().copied().collect();
        crate::test_vectors::compare("opposed/corrected.f32", &actual, 2e-6);
        crate::test_vectors::compare("opposed/original.f32", &actual, 1.0);
    }
}
