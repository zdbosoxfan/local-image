//! Capture sharpening's automatic radius: an estimate of the sensor's blur (the Gaussian σ, in
//! sensor pixels, that the deconvolution undoes) from the sharpest transitions between
//! neighbouring same-colour photosites of the raw data.
//!
//! Ported from darktable's `src/iop/demosaicing/capture.c` (`_calcRadiusBayer`, `_calcRadiusMono`,
//! `_calcRadiusXtrans`, `_calc_auto_radius`; GPL-3.0-or-later; algorithm by Ingo Weyrich for
//! RawTherapee), see `docs/PORTS.md`. The data here is sensor data already (darktable undoes its
//! white balance first). The deconvolution itself runs in the develop pipeline
//! (`lightcraft_pipeline::capture`).

use crate::{Cfa, Normalized};
use rayon::prelude::*;

const RAWEPS: f32 = 0.005;
const LOWER: f32 = 0.01;
const UPPER: f32 = 0.9;

/// The radius to use when the data can't tell (darktable's default).
pub const DEFAULT_RADIUS: f32 = 0.5;

/// Read access to the analysed window of the mosaic.
struct Win<'a> {
    data: &'a [f32],
    stride: usize,
    cpp: usize,
    plane: usize,
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
}

impl Win<'_> {
    #[inline]
    fn at(&self, x: isize, y: isize) -> f32 {
        let (x, y) = ((self.x0 as isize + x) as usize, (self.y0 as isize + y) as usize);
        self.data[(y * self.stride + x) * self.cpp + self.plane]
    }
}

/// Largest ratio between diagonal neighbours (Bayer greens, or any two pixels of a monochrome
/// sensor) that touch no clipped photosite. `first_col(row)` is the first column to test.
fn max_ratio_diagonal(win: &Win, first_col: impl Fn(usize) -> usize + Sync) -> f32 {
    (4..win.h.saturating_sub(4))
        .into_par_iter()
        .map(|row| {
            let mut max_ratio = 1.0f32;
            let r = row as isize;
            let mut col = first_col(row);
            while col + 4 < win.w {
                let c = col as isize;
                let at = |dx: isize, dy: isize| win.at(c + dx, r + dy);
                let v00 = at(0, 0);
                if v00 > RAWEPS {
                    let v1m1 = at(-1, 1);
                    let v1p1 = at(1, 1);
                    let max0 = v00.max(v1m1);
                    if v1m1 > RAWEPS && max0 > LOWER {
                        let min = v00.min(v1m1);
                        if max0 > max_ratio * min {
                            let clipped = if max0 == v00 {
                                at(-1, -1).max(at(1, -1)).max(v1p1) >= UPPER
                            } else {
                                at(-2, 0).max(v00).max(at(-2, 2)).max(at(0, 2)) >= UPPER
                            };
                            if !clipped {
                                max_ratio = max0 / min;
                            }
                        }
                    }
                    let max1 = v00.max(v1p1);
                    if v1p1 > RAWEPS && max1 > LOWER {
                        let min = v00.min(v1p1);
                        if max1 > max_ratio * min {
                            let clipped = if max1 == v00 {
                                at(-1, -1).max(at(1, -1)).max(v1p1) >= UPPER
                            } else {
                                v00.max(at(2, 0)).max(at(0, 2)).max(at(2, 2)) >= UPPER
                            };
                            if !clipped {
                                max_ratio = max1 / min;
                            }
                        }
                    }
                }
                col += 2;
            }
            max_ratio
        })
        .reduce(|| 1.0, f32::max)
}

/// The X-Trans variant (darktable's `_calcRadiusXtrans`, without its +0.2).
fn max_ratio_xtrans(win: &Win, cfa: &Cfa) -> f32 {
    let fc = |x: usize, y: usize| cfa.color_at(win.x0 + x, win.y0 + y);
    // a solitary green whose left and right neighbours differ and whose upper and left are not green
    let mut start = None;
    'find: for sy in 6..12 {
        for sx in 6..12 {
            if fc(sx, sy) == 1 && fc(sx - 1, sy) != fc(sx + 1, sy) && fc(sx, sy - 1) != 1 && fc(sx - 1, sy) != 1 {
                start = Some((sx, sy));
                break 'find;
            }
        }
    }
    let Some((sx, sy)) = start else { return 1.0 };
    let ratio = |a: f32, b: f32, m: &mut f32| {
        let max = a.max(b);
        if max > LOWER {
            let min = a.min(b);
            if max > *m * min {
                *m = max / min;
            }
        }
    };
    let rows: Vec<usize> = (sy + 2..win.h.saturating_sub(4)).step_by(3).collect();
    rows.into_par_iter()
        .map(|row| {
            let mut m = 1.0f32;
            let r = row as isize;
            let mut col = sx + 2;
            while col + 4 < win.w {
                let c = col as isize;
                let at = |dx: isize, dy: isize| win.at(c + dx, r + dy);
                let vp1p1 = at(1, 1);
                let square_clipped = vp1p1.max(at(2, 1)).max(at(1, 2)).max(at(2, 2)) >= UPPER;
                let green = at(0, 0);
                if green > RAWEPS && at(-1, -1).max(at(1, -1)) < UPPER && green < UPPER {
                    let vp1m1 = at(-1, 1);
                    if vp1m1 > RAWEPS && at(-2, 1).max(vp1m1).max(at(-2, 2)).max(at(-1, 1)) < UPPER {
                        ratio(green, vp1m1, &mut m);
                    }
                    if vp1p1 > RAWEPS && !square_clipped {
                        ratio(green, vp1p1, &mut m);
                    }
                }
                if !square_clipped {
                    let vp2p2 = at(2, 2);
                    if vp2p2 > RAWEPS {
                        if vp1p1 > RAWEPS {
                            ratio(vp1p1, vp2p2, &mut m);
                        }
                        let right = at(3, 3);
                        if right.max(at(2, 4)).max(at(4, 4)) < UPPER && right > RAWEPS {
                            ratio(right, vp2p2, &mut m);
                        }
                    }
                    let (vp1p2, vp2p1) = (at(2, 1), at(1, 2));
                    if vp2p1 > RAWEPS {
                        if vp1p2 > RAWEPS {
                            ratio(vp1p2, vp2p1, &mut m);
                        }
                        let left = at(0, 3);
                        if left.max(at(-1, 4)).max(at(1, 4)) < UPPER && left > RAWEPS {
                            ratio(left, vp2p1, &mut m);
                        }
                    }
                }
                col += 3;
            }
            m
        })
        .reduce(|| 1.0, f32::max)
}

/// The capture-sharpening radius (Gaussian σ in sensor pixels, 0..1.5) of normalised raw data,
/// measured on the centre 60 % of the frame (the sharpest part of most lenses).
/// [`DEFAULT_RADIUS`] for data smaller than 1000 × 1000 or of a kind it can't analyse.
pub fn capture_radius(n: &Normalized) -> f32 {
    let (w, h) = (n.width, n.height);
    if w < 1000 || h < 1000 || n.data.len() < w * h * n.cpp {
        return DEFAULT_RADIUS;
    }
    // the centre 60 %, snapped to a multiple of 6 (keeps any CFA phase)
    let (x0, y0) = ((w / 5) / 6 * 6, (h / 5) / 6 * 6);
    let (ow, oh) = ((w * 4 / 5).saturating_sub(x0), (h * 4 / 5).saturating_sub(y0));
    let (plane, cpp) = match (&n.cfa, n.cpp) {
        (_, 1) => (0, 1),
        (None, 3) => (1, 3),
        _ => return DEFAULT_RADIUS,
    };
    let win = Win { data: &n.data, stride: w, cpp, plane, x0, y0, w: ow, h: oh };
    let (ratio, extra) = match &n.cfa {
        Some(cfa) if cpp == 1 && cfa.is_bayer() => {
            let fc0 = |row: usize| cfa.color_at(x0, y0 + row);
            (max_ratio_diagonal(&win, |row| 5 + (fc0(row) & 1) as usize), 0.0)
        }
        Some(cfa) if cpp == 1 => (max_ratio_xtrans(&win, cfa), 0.2),
        _ => (max_ratio_diagonal(&win, |_| 5), 0.0),
    };
    let r = extra + (1.0 / ratio.ln()).sqrt();
    if r.is_nan() { DEFAULT_RADIUS } else { r.clamp(0.0, 1.5) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demosaic::mosaic_from_rgb;
    use lightcraft_raster::Rgb32f;

    /// A scene of random-ish soft discs blurred by a Gaussian of `sigma` px (analytic edges).
    fn blurred_scene(w: usize, h: usize, sigma: f32) -> Rgb32f {
        // erf approximation (Abramowitz–Stegun 7.1.26)
        let erf = |x: f32| {
            let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
            let y = 1.0 - (((((1.061_405_4 * t - 1.453_152_1) * t) + 1.421_413_7) * t - 0.284_496_74) * t + 0.254_829_6) * t * (-x * x).exp();
            if x >= 0.0 { y } else { -y }
        };
        Rgb32f::from_fn(w, h, |x, y| {
            // vertical stripes 23 px wide: step edges (at the nearest multiple of 23 px, rising
            // and falling in turn) blurred by σ along x
            let fx = x as f32 + 0.5 + 0.0 * y as f32;
            let k = (fx / 23.0).round();
            let t = 0.5 * (1.0 + erf((fx - k * 23.0 - 0.3) / (sigma * std::f32::consts::SQRT_2)));
            let t = if (k as i64) % 2 == 0 { t } else { 1.0 - t };
            let v = 0.08 + 0.52 * t.clamp(0.0, 1.0);
            [v * 0.9, v, v * 0.8]
        })
    }

    #[test]
    fn sharper_data_gives_a_smaller_radius() {
        let cfa = Cfa::bayer("RGGB").unwrap();
        let r = |sigma| capture_radius(&mosaic_from_rgb(&blurred_scene(1200, 1020, sigma), &cfa));
        let (sharp, soft) = (r(0.5), r(1.2));
        eprintln!("radius: σ 0.5 → {sharp:.3}, σ 1.2 → {soft:.3}");
        assert!(sharp < soft, "{sharp} {soft}");
        assert!((0.0..=1.5).contains(&sharp) && (0.0..=1.5).contains(&soft));
        // X-Trans and monochrome data run too
        let x = capture_radius(&mosaic_from_rgb(&blurred_scene(1200, 1020, 0.5), &Cfa::xtrans()));
        assert!((0.2..=1.5).contains(&x), "{x}");
        let img = blurred_scene(1200, 1020, 0.5);
        let mono = Normalized { width: 1200, height: 1020, cpp: 1, data: img.data.iter().map(|p| p[1]).collect(), cfa: None };
        assert!((0.0..=1.5).contains(&capture_radius(&mono)));
    }

    #[test]
    fn small_or_flat_data_falls_back() {
        let cfa = Cfa::bayer("RGGB").unwrap();
        assert_eq!(capture_radius(&mosaic_from_rgb(&Rgb32f::filled(300, 200, [0.3; 3]), &cfa)), DEFAULT_RADIUS);
        // flat: no transition at all → the widest radius
        let flat = capture_radius(&mosaic_from_rgb(&Rgb32f::filled(1100, 1000, [0.3; 3]), &cfa));
        assert_eq!(flat, 1.5);
    }
}
