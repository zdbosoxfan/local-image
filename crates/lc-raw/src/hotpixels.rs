//! Hot and dead photosites: a single sensor site stuck far above (or below) everything around it,
//! common in long exposures (an A1 bracket with Long Exposure NR off) and on ageing sensors. Left
//! in, the demosaic spreads one into a pure red, green or blue dot of up to nine pixels, highlight
//! handling takes it for a clipped highlight and binned previews make it a whole coloured block.
//! Lightroom and Capture One remove them automatically; darktable's `hotpixels` and RawTherapee's
//! `findHotDeadPixels` show the method: compare a site with its same-colour neighbours and replace
//! an outlier by their median.
//!
//! Optics never put light on one photosite alone: a lens and the anti-aliasing filter spread even
//! a star or a chrome glint over its neighbours, whatever their colour. So a site is replaced only
//! when its other-colour neighbours show nothing of it, which keeps point lights, window glints and
//! fine texture, and only by a margin well above the image's noise. The neighbours are compared
//! white balanced: in camera units a green-centred spot puts only a few percent of its light on
//! the less sensitive red and blue sites around it.

use crate::{RawData, RawImage};
use rayon::prelude::*;

/// A hot site exceeds every same-colour neighbour by this factor (a dead one falls below all of
/// them by it)...
const RATIO: f32 = 2.0;
/// ...and by at least this much of the white level, or this many noise sigmas.
const MIN_MARGIN: f32 = 0.04;
const SIGMAS: f32 = 8.0;
/// The four sites next to an outlier carry its light when, white balanced, together they stand
/// out from their own surroundings by this share of its excess: a lens spot of Gaussian σ 0.35 px
/// gives them about 0.36 of it, 0.4 px about 0.53. A hot site gives them noise.
const SPREAD: f32 = 0.2;
/// A dead site is only told from dark detail where its same-colour neighbours are this many
/// margins above black.
const DEAD_FLOOR: f32 = 4.0;

/// The eight same-colour neighbours of any Bayer site, two pixels away.
const RING: [(isize, isize); 8] = [(-2, -2), (0, -2), (2, -2), (-2, 0), (2, 0), (-2, 2), (0, 2), (2, 2)];
/// The four sites next to a site: green around red and blue, red and blue around green.
const EDGE: [(isize, isize); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

impl RawImage {
    /// Replace isolated hot and dead photosites of a Bayer mosaic with the median of their eight
    /// same-colour neighbours (see the module docs). `wb` are the white-balance multipliers
    /// (camera → balanced, any scale). Other layouts and linear data are left alone. Returns the
    /// number of sites replaced.
    pub fn suppress_hot_pixels(&mut self, wb: [f32; 3]) -> usize {
        let Some(cfa) = &self.cfa else { return 0 };
        let a = self.active_area;
        if self.cpp != 1 || !cfa.is_bayer() || a.width < 8 || a.height < 8 || self.validate().is_err() {
            return 0;
        }
        let range = self.white_at(0) - self.black.mean();
        if range.is_nan() || range <= 0.0 {
            return 0;
        }
        let scale = 1.0 / range;
        let width = self.width;
        let b = &self.black;
        let uniform = b.delta_h.is_empty() && b.delta_v.is_empty() && b.values.iter().all(|&v| v == b.values[0]);
        let black0 = b.at(0, 0, 0, 1);
        let data = &self.data;
        let at = |x: usize, y: usize| {
            let raw = data.get((a.y + y) * width + a.x + x);
            (raw - if uniform { black0 } else { b.at(x, y, 0, 1) }) * scale
        };
        let margin = (SIGMAS * noise_sigma(&at, a.width, a.height)).max(MIN_MARGIN);
        let wb = wb.map(|v| if v.is_finite() && v > 0.0 { v } else { 1.0 });
        let gain = |x: isize, y: isize| wb[cfa.color_at(a.x + x as usize, a.y + y as usize).min(2) as usize];
        let (w, h) = (a.width as isize, a.height as isize);
        let v = |x: isize, y: isize| at(x as usize, y as usize);
        let ring = |x: isize, y: isize| -> [f32; 8] { RING.map(|(dx, dy)| v(x + dx, y + dy)) };
        let median = |mut r: [f32; 8]| {
            r.sort_unstable_by(f32::total_cmp);
            (r[3] + r[4]) / 2.0
        };
        let fixes: Vec<(usize, usize, f32)> = (4..h - 4)
            .into_par_iter()
            .flat_map_iter(|y| {
                (4..w - 4).filter_map(move |x| {
                    let s = v(x, y);
                    // cheap rejection on the two same-colour neighbours of the row
                    let (l, r) = (v(x - 2, y), v(x + 2, y));
                    let (most, least) = (l.max(r), l.min(r));
                    if (s <= most * RATIO || s - most <= margin) && (s * RATIO >= least || least - s <= margin) {
                        return None;
                    }
                    let mut same = ring(x, y).to_vec();
                    if cfa.color_at(a.x + x as usize, a.y + y as usize) == 1 {
                        same.extend([(-1, -1), (1, -1), (-1, 1), (1, 1)].map(|(dx, dy)| v(x + dx, y + dy)));
                    }
                    let (lo, hi) = same.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &n| (lo.min(n), hi.max(n)));
                    let hot = s > hi * RATIO && s - hi > margin;
                    let dead = s * RATIO < lo && lo - s > margin && lo > DEAD_FLOOR * margin;
                    if !(hot || dead) {
                        return None;
                    }
                    // light spreads into the neighbouring sites; a hot site's does not
                    let fill = median(ring(x, y));
                    let excess = (s - fill).abs() * gain(x, y);
                    let (mut spread, mut gains) = (0.0, 0.0);
                    for (dx, dy) in EDGE {
                        let (nx, ny) = (x + dx, y + dy);
                        let (n, own) = (v(nx, ny), median(ring(nx, ny)));
                        let g = gain(nx, ny);
                        spread += (if hot { n - own } else { own - n }) * g;
                        gains += g;
                    }
                    // plus about one sigma of the sum's noise (a quarter margin), white balanced
                    let lit = spread > SPREAD * excess + gains * margin / 16.0;
                    (!lit).then_some((x as usize, y as usize, fill))
                })
            })
            .collect();
        for &(x, y, m) in &fixes {
            let i = (a.y + y) * width + a.x + x;
            let value = m / scale + self.black.at(x, y, 0, 1);
            match &mut self.data {
                RawData::U16(d) => d[i] = value.round().clamp(0.0, 65535.0) as u16,
                RawData::F32(d) => d[i] = value,
            }
        }
        fixes.len()
    }
}

/// Noise of the mosaic (normalised units): the median absolute same-colour second difference of
/// a sparse grid of sites, scaled to a Gaussian sigma (as [`crate::demosaic::noise_sigma`]).
fn noise_sigma(at: &(impl Fn(usize, usize) -> f32 + Sync), w: usize, h: usize) -> f32 {
    let step = ((w * h / 20_000).max(1) as f64).sqrt().ceil() as usize;
    let mut d: Vec<f32> = (2..h - 2)
        .step_by(step)
        .flat_map(|y| (2..w - 2).step_by(step).map(move |x| (x, y)))
        .map(|(x, y)| (at(x - 2, y) - 2.0 * at(x, y) + at(x + 2, y)).abs())
        .collect();
    if d.is_empty() {
        return 0.0;
    }
    let mid = d.len() / 2;
    d.select_nth_unstable_by(mid, f32::total_cmp);
    d[mid] / 1.652
}

#[cfg(test)]
mod tests {
    use crate::demosaic::mosaic_from_rgb;
    use crate::*;

    fn raw_of(n: &Normalized) -> RawImage {
        RawImage {
            format: RawFormat::Arw,
            width: n.width,
            height: n.height,
            cpp: 1,
            data: RawData::U16(n.data.iter().map(|&v| (512.0 + v.clamp(0.0, 1.0) * 15871.0).round() as u16).collect()),
            cfa: n.cfa.clone(),
            bits: 14,
            black: BlackLevel::uniform(512.0),
            white: vec![16383.0],
            active_area: Rect::new(0, 0, n.width, n.height),
            crop: Rect::new(0, 0, n.width, n.height),
            orientation: Orientation::Normal,
            color: ColorData::default(),
            wb_multipliers: None,
            linearized: false,
            opcodes: OpcodeLists::default(),
            vendor_lens: None,
            metadata: Metadata::default(),
        }
    }

    /// A grey wall (0.08 white balanced) with three hot sites and two real point lights.
    #[test]
    fn hot_sites_go_and_point_lights_stay() {
        let wb = [2.2, 1.0, 1.6];
        let cfa = Cfa::bayer("RGGB").unwrap();
        let (w, h) = (64usize, 48usize);
        let blob = |x: f32, y: f32, cx: f32, cy: f32| 3.0 * (-((x - cx).powi(2) + (y - cy).powi(2)) / (2.0 * 0.7 * 0.7)).exp();
        let scene = RgbFrame::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32, y as f32);
            let light = blob(fx, fy, 44.0, 12.0) + blob(fx, fy, 50.5, 34.5);
            std::array::from_fn(|c| (0.08 + light * [1.0, 0.9, 0.7][c]) / wb[c])
        });
        let mut n = mosaic_from_rgb(&scene, &cfa);
        // stuck sites on a red, a green and a blue photosite of the plain wall
        let hot = [(10usize, 10usize), (21, 10), (11, 31)];
        for &(x, y) in &hot {
            n.data[y * w + x] = 1.0;
        }
        let truth = raw_of(&mosaic_from_rgb(&scene, &cfa));
        let mut raw = raw_of(&n);
        assert_eq!(raw.suppress_hot_pixels(wb), 3);
        let fixed = raw.develop(Method::Rcd).unwrap();
        let clean = truth.develop(Method::Rcd).unwrap();
        for &(hx, hy) in &hot {
            for y in hy - 3..=hy + 3 {
                for x in hx - 3..=hx + 3 {
                    for c in 0..3 {
                        let (a, b) = (fixed.get(x, y)[c], clean.get(x, y)[c]);
                        assert!((a - b).abs() < 0.1 * b, "({x}, {y}) channel {c}: {a} vs {b}");
                    }
                }
            }
        }
        // the point lights keep their energy and colour: the mosaic there is untouched
        let (RawData::U16(a), RawData::U16(b)) = (&raw.data, &truth.data) else { panic!("integer samples") };
        for y in 0..h {
            for x in 0..w {
                if !hot.contains(&(x, y)) {
                    assert_eq!(a[y * w + x], b[y * w + x], "({x}, {y})");
                }
            }
        }
    }

    /// Sharp lens spots (Gaussian σ of 0.4 and 0.45 px, integrated over each photosite) on a dark
    /// wall, centred on a red, both greens and a blue site, dim and bright: a green-centred spot
    /// puts only a few percent of its light on its red and blue neighbours in camera units, but
    /// as much as any other spot once white balanced. Every spot stays, the hot sites go.
    #[test]
    fn sharp_point_lights_on_every_colour_stay() {
        let wb = [2.2, 1.0, 1.6];
        let cfa = Cfa::bayer("RGGB").unwrap();
        let (w, h) = (112usize, 56usize);
        let spot = |x: usize, y: usize, cx: f32, cy: f32, sigma: f32| {
            let mut v = 0.0;
            for j in 0..8 {
                for i in 0..8 {
                    let (px, py) = (x as f32 + (i as f32 + 0.5) / 8.0, y as f32 + (j as f32 + 0.5) / 8.0);
                    v += (-((px - cx).powi(2) + (py - cy).powi(2)) / (2.0 * sigma * sigma)).exp();
                }
            }
            v / 64.0
        };
        // (site, σ, white-balanced level the light adds to its centre site)
        let mut spots = Vec::new();
        for (j, peak) in [0.3f32, 1.0].into_iter().enumerate() {
            for (i, (sigma, (dx, dy))) in [0.4f32, 0.45].into_iter().flat_map(|s| [(0, 0), (1, 0), (0, 1), (1, 1)].map(|d| (s, d))).enumerate() {
                spots.push(((10 + 12 * i + dx, 12 + 14 * j + dy), sigma, peak));
            }
        }
        let scene = RgbFrame::from_fn(w, h, |x, y| {
            let light: f32 = spots
                .iter()
                .map(|&((sx, sy), sigma, peak)| {
                    let (cx, cy) = (sx as f32 + 0.5, sy as f32 + 0.5);
                    peak * spot(x, y, cx, cy, sigma) / spot(sx, sy, cx, cy, sigma)
                })
                .sum();
            std::array::from_fn(|c| (0.02 + light) / wb[c])
        });
        let mut n = mosaic_from_rgb(&scene, &cfa);
        let hot = [(20usize, 44usize), (33, 44), (47, 45)];
        for &(x, y) in &hot {
            n.data[y * w + x] = 0.8;
        }
        let truth = raw_of(&mosaic_from_rgb(&scene, &cfa));
        let mut raw = raw_of(&n);
        assert_eq!(raw.suppress_hot_pixels(wb), hot.len());
        let (RawData::U16(a), RawData::U16(b)) = (&raw.data, &truth.data) else { panic!("integer samples") };
        for &((x, y), sigma, peak) in &spots {
            assert_eq!(a[y * w + x], b[y * w + x], "a spot of σ {sigma} and level {peak} at ({x}, {y})");
        }
        for &(x, y) in &hot {
            assert!((a[y * w + x] as f32 - b[y * w + x] as f32).abs() < 0.01 * 15871.0, "hot site ({x}, {y})");
        }
    }

    /// Noise alone (σ = 0.01 around 0.2) is not mistaken for hot sites, and a dead site in a bright
    /// area is filled.
    #[test]
    fn noise_is_left_alone_and_dead_sites_are_filled() {
        let cfa = Cfa::bayer("GRBG").unwrap();
        let (w, h) = (256usize, 256usize);
        let mut n = mosaic_from_rgb(&RgbFrame::filled(w, h, [0.2; 3]), &cfa);
        for (i, v) in n.data.iter_mut().enumerate() {
            // a sum of uniforms: close to Gaussian with σ = 0.01
            let u: f32 = (0..12).map(|k| crate::test_vectors::noise(i * 12 + k)).sum::<f32>() - 6.0;
            *v += 0.01 * u;
        }
        let mut raw = raw_of(&n);
        assert!(raw.suppress_hot_pixels([2.2, 1.0, 1.6]) < w * h / 10_000, "noise taken for hot sites");
        let mut bright = mosaic_from_rgb(&RgbFrame::filled(64, 64, [0.5; 3]), &cfa);
        bright.data[30 * 64 + 30] = 0.0;
        let mut raw = raw_of(&bright);
        assert_eq!(raw.suppress_hot_pixels([2.2, 1.0, 1.6]), 1);
        let RawData::U16(d) = &raw.data else { panic!("integer samples") };
        assert!((d[30 * 64 + 30] as f32 - (512.0 + 0.5 * 15871.0)).abs() < 2.0);
    }
}
