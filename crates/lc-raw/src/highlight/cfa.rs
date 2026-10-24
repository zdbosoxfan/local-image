//! Highlight handling with the clip test on the mosaic, before demosaic and before `OpcodeList3`.
//! A demosaic spreads clipped samples into neighbouring pixels that then pass a threshold on
//! interpolated RGB as valid and keep a wrong colour; lens-shading gains in `OpcodeList3` lift
//! unclipped data over the clip level. darktable runs its highlight modes on the CFA for both
//! reasons.

use crate::{Cfa, Normalized, RgbFrame};
use rayon::prelude::*;

/// Each `k × k` block of the mosaic as one RGB pixel: a channel with a sample at or above `clip`
/// takes the block's maximum of that colour (so it still reads as clipped), else the mean.
/// `crop` is the region binned, in mosaic coordinates.
pub(crate) fn superpixels(n: &Normalized, cfa: &Cfa, crop: crate::Rect, k: usize, clip: f32) -> RgbFrame {
    let mut out = RgbFrame::new(crop.width / k, crop.height / k);
    let w = out.width;
    li_raster::par_rows(&mut out.data, w, |by, row| {
        for (bx, px) in row.iter_mut().enumerate() {
            let (mut sum, mut max, mut count) = ([0.0; 3], [f32::MIN; 3], [0usize; 3]);
            for dy in 0..k {
                for dx in 0..k {
                    let (x, y) = (crop.x + bx * k + dx, crop.y + by * k + dy);
                    let c = cfa.color_at(x, y).min(2) as usize;
                    let v = n.data[y * n.width + x];
                    sum[c] += v;
                    max[c] = max[c].max(v);
                    count[c] += 1;
                }
            }
            *px = std::array::from_fn(|c| if max[c] >= clip { max[c] } else { sum[c] / count[c].max(1) as f32 });
        }
    });
    out
}

/// For each pixel of a mosaic, the colours with a clipped sample within `radius` (Chebyshev) of it:
/// the samples a demosaic interpolates that pixel's channels from. Bit `c` is colour `c`.
pub struct ClipMask {
    pub width: usize,
    pub height: usize,
    bits: Vec<u8>,
}

impl ClipMask {
    /// `None` when `n` is not a mosaic or nothing in it is clipped.
    pub fn of(n: &Normalized, clip: f32, radius: usize) -> Option<ClipMask> {
        let cfa = n.cfa.as_ref().filter(|_| n.cpp == 1)?;
        let (w, h) = (n.width, n.height);
        let mut bits = vec![0u8; w * h];
        let any = bits
            .par_chunks_mut(w)
            .zip(n.data.par_chunks(w))
            .enumerate()
            .map(|(y, (out, row))| {
                let mut any = false;
                for (x, (b, &v)) in out.iter_mut().zip(row).enumerate() {
                    if v >= clip {
                        *b = 1 << cfa.color_at(x, y).min(2);
                        any = true;
                    }
                }
                any
            })
            .reduce(|| false, |a, b| a || b);
        if !any {
            return None;
        }
        let mut wide = vec![0u8; w * h];
        wide.par_chunks_mut(w).zip(bits.par_chunks(w)).for_each(|(out, row)| {
            for (x, o) in out.iter_mut().enumerate() {
                *o = row[x.saturating_sub(radius)..(x + radius + 1).min(w)].iter().fold(0, |a, b| a | b);
            }
        });
        bits.par_chunks_mut(w).enumerate().for_each(|(y, out)| {
            for (x, o) in out.iter_mut().enumerate() {
                *o = (y.saturating_sub(radius)..(y + radius + 1).min(h)).fold(0, |a, yy| a | wide[yy * w + x]);
            }
        });
        Some(ClipMask { width: w, height: h, bits })
    }

    #[inline]
    pub(crate) fn at(&self, i: usize) -> [bool; 3] {
        let b = self.bits[i];
        [b & 1 != 0, b & 2 != 0, b & 4 != 0]
    }
}

/// [`super::clip_neutral`] for a mosaic: every sample limited to the lowest channel's white-balanced
/// clip level, so clipped areas render neutral and the demosaic sees a flat plateau.
pub fn clip_cfa(n: &mut Normalized, wb: [f32; 3], clip: f32) {
    let Some(cfa) = n.cfa.clone().filter(|_| n.cpp == 1) else { return };
    let limit = wb.iter().copied().fold(f32::MAX, f32::min) * clip;
    let w = n.width;
    n.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            let m = wb[cfa.color_at(x, y).min(2) as usize];
            *v = (*v * m).min(limit) / m;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demosaic::mosaic_from_rgb;

    fn raw_of(n: &Normalized, opcodes: crate::OpcodeLists) -> crate::RawImage {
        crate::RawImage {
            format: crate::RawFormat::Dng,
            width: n.width,
            height: n.height,
            cpp: 1,
            data: crate::RawData::F32(n.data.clone()),
            cfa: n.cfa.clone(),
            bits: 32,
            black: crate::BlackLevel::uniform(0.0),
            white: vec![1.0],
            active_area: crate::Rect::new(0, 0, n.width, n.height),
            crop: crate::Rect::new(0, 0, n.width, n.height),
            orientation: crate::Orientation::Normal,
            color: Default::default(),
            wb_multipliers: None,
            linearized: false,
            opcodes,
            vendor_lens: None,
            metadata: Default::default(),
        }
    }

    fn highlights(mode: super::super::Mode, wb: [f32; 3]) -> super::super::Highlights {
        super::super::Highlights { mode, wb, clip: 0.99 }
    }

    /// DJI DNGs carry lens-shading gains in OpcodeList3 (up to 2.6× in the corners). Highlight
    /// handling runs on the sensor data before them: bright but unclipped corners keep their
    /// colour, and real clipping is still found.
    #[test]
    fn lens_shading_gains_do_not_make_unclipped_corners_clipped() {
        use super::super::Mode;
        let wb = [2.0, 1.0, 1.5];
        let cfa = Cfa::bayer("RGGB").unwrap();
        let camera = [0.4, 0.45, 0.35];
        let vignette = crate::Opcode::FixVignetteRadial { k: [2.0, 0.0, 0.0, 0.0, 0.0], center: [0.5, 0.5] };
        let lists = crate::OpcodeLists { list3: vec![vignette], ..Default::default() };
        let raw = raw_of(&mosaic_from_rgb(&RgbFrame::filled(64, 48, camera), &cfa), lists.clone());
        let plain = raw.develop_stages(crate::Method::Bilinear, &Default::default(), wb, |_| {}, |_| {}).unwrap();
        let corner = plain.get(0, 0);
        assert!(corner[0] > 1.0 && corner[1] > 1.0, "the gain lifts the corner over the sensor white: {corner:?}");
        for mode in [Mode::Reconstruct, Mode::Opposed, Mode::Clip, Mode::Segmentation] {
            let out = raw.develop_with_highlights(crate::Method::Bilinear, &Default::default(), &highlights(mode, wb)).unwrap();
            assert_eq!(out.data, plain.data, "{mode:?}: nothing on the sensor is clipped");
        }
        // the old order (highlights on the gained RGB) neutralised and lifted those corners
        let mut old = plain.clone();
        assert!(super::super::reconstruct(&mut old, wb, 0.99) > 0);
        let q = old.get(0, 0).map(|v| v);
        assert!((q[0] * wb[0] - q[1] * wb[1]).abs() < 1e-3, "neutralised: {q:?}");
        // a truly clipped patch is still reconstructed
        let mut clipped = mosaic_from_rgb(&RgbFrame::filled(64, 48, camera), &cfa);
        for y in 20..28 {
            for x in 28..36 {
                clipped.data[y * 64 + x] = 1.0;
            }
        }
        let raw = raw_of(&clipped, lists);
        let out = raw.develop_with_highlights(crate::Method::Bilinear, &Default::default(), &highlights(Mode::Reconstruct, wb)).unwrap();
        let plain = raw.develop_stages(crate::Method::Bilinear, &Default::default(), wb, |_| {}, |_| {}).unwrap();
        assert_eq!(out.get(0, 0), plain.get(0, 0));
        let (p, centre) = (out.get(32, 24), plain.get(32, 24));
        assert!(p.iter().zip(&centre).all(|(a, b)| a >= b) && p != centre, "{p:?} {centre:?}");
    }

    /// A neutral wall with a lens-blurred blown window (white-balanced truth 0.25 and 3.0).
    /// Reading the clipping from the mosaic leaves no coloured outline around the window, where
    /// thresholding demosaiced RGB keeps interpolated, wrongly coloured channels.
    #[test]
    fn blown_window_edges_stay_neutral() {
        use super::super::Mode;
        let wb = [2.2, 1.0, 1.6];
        let (w, h) = (96usize, 96usize);
        let sharp = |x: f32, y: f32| if (30.0..66.0).contains(&x) && (30.0..66.0).contains(&y) { 3.0 } else { 0.25 };
        let truth = RgbFrame::from_fn(w, h, |x, y| {
            // a 5×5 box at 0.6 px spacing: a soft lens edge
            let mut v = 0.0;
            for dy in -2..=2 {
                for dx in -2..=2 {
                    v += sharp(x as f32 + dx as f32 * 0.6, y as f32 + dy as f32 * 0.6);
                }
            }
            [v / 25.0; 3]
        });
        let cfa = Cfa::bayer("RGGB").unwrap();
        let camera = truth.map(|p| [(p[0] / wb[0]).min(1.0), (p[1] / wb[1]).min(1.0), (p[2] / wb[2]).min(1.0)]);
        let raw = raw_of(&mosaic_from_rgb(&camera, &cfa), Default::default());
        // over the transition from wall to blown core: mean white-balanced chroma error, and the
        // pixels whose luminance is off by more than 20 %
        let edge = |img: &RgbFrame| {
            let (mut sum, mut n, mut off) = (0.0, 0, 0);
            for (p, t) in img.data.iter().zip(&truth.data) {
                let t = t[0];
                if t > 0.3 && t < 2.9 {
                    let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
                    let s = q[0] + q[1] + q[2];
                    sum += q.iter().map(|v| (v / s - 1.0 / 3.0).abs()).fold(0.0, f32::max);
                    n += 1;
                    off += usize::from(s / 3.0 > 1.2 * t || s / 3.0 < t / 1.2);
                }
            }
            (sum / n as f32, off)
        };
        for method in [crate::Method::Rcd, crate::Method::Amaze, crate::Method::Ahd] {
            let develop = |mode| raw.develop_with_highlights(method, &Default::default(), &highlights(mode, wb)).unwrap();
            let plain = raw.develop_stages(method, &Default::default(), wb, |_| {}, |_| {}).unwrap();
            let mut thresholded = plain.clone();
            super::super::reconstruct(&mut thresholded, wb, 0.99);
            let ((chroma, off), (chroma_before, off_before)) = (edge(&develop(Mode::Reconstruct)), edge(&thresholded));
            assert!(chroma < 0.5 * chroma_before && chroma < 0.03, "{method:?}: chroma {chroma} (thresholding RGB: {chroma_before})");
            assert!(off <= off_before, "{method:?}: {off} pixels off in luminance (thresholding RGB: {off_before})");
            // far from any clipped sample nothing changes
            let out = develop(Mode::Reconstruct);
            for y in 0..h {
                for x in 0..w {
                    if !(22..74).contains(&x) || !(22..74).contains(&y) {
                        assert_eq!(out.get(x, y), plain.get(x, y), "{method:?} ({x}, {y})");
                    }
                }
            }
            // clipping the mosaic gives the demosaic a flat neutral plateau
            let mut clipped_rgb = plain.clone();
            super::super::clip_neutral(&mut clipped_rgb, wb, 0.99);
            let (clip_chroma, clip_rgb) = (edge(&develop(Mode::Clip)).0, edge(&clipped_rgb).0);
            assert!(clip_chroma < 0.005 && clip_chroma < clip_rgb, "{method:?}: clip {clip_chroma} (on RGB {clip_rgb})");
        }
    }

    /// darktable's opposed modes fill an area clipped in every channel from the white-balance
    /// multipliers (a peach window under daylight multipliers, a blue one under tungsten); the
    /// neutral pass makes it white, and leaves partly clipped areas to the ports.
    #[test]
    fn fully_clipped_areas_render_neutral_in_the_darktable_modes() {
        use super::super::Mode;
        let spread = |p: [f32; 3], wb: [f32; 3]| {
            let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
            let (lo, hi) = (q.iter().cloned().fold(f32::MAX, f32::min), q.iter().cloned().fold(0.0, f32::max));
            (hi - lo) / hi
        };
        for wb in [[2.2, 1.0, 1.6], [1.35, 1.0, 2.9]] {
            // a neutral wall (0.3 white balanced) with a 30×30 patch clipped in every channel
            let img = RgbFrame::from_fn(64, 64, |x, y| {
                if (17..47).contains(&x) && (17..47).contains(&y) { [1.0; 3] } else { [0.3 / wb[0], 0.3 / wb[1], 0.3 / wb[2]] }
            });
            let mut ported = img.clone();
            super::super::opposed(&mut ported, wb, 0.99);
            assert!(spread(ported.get(32, 32), wb) > 0.1, "the port alone tints the patch: {:?}", ported.get(32, 32));
            for mode in [Mode::Opposed, Mode::Segmentation] {
                let mut rgb = img.clone();
                highlights(mode, wb).rgb(&mut rgb);
                assert!(spread(rgb.get(32, 32), wb) < 0.02, "{mode:?} {wb:?}: {:?}", rgb.get(32, 32));
                assert_eq!(rgb.get(2, 2), img.get(2, 2), "the wall is untouched");
                let raw = raw_of(&mosaic_from_rgb(&img, &Cfa::bayer("RGGB").unwrap()), Default::default());
                let out = raw.develop_with_highlights(crate::Method::Rcd, &Default::default(), &highlights(mode, wb)).unwrap();
                assert!(spread(out.get(32, 32), wb) < 0.02, "{mode:?} {wb:?} on the mosaic: {:?}", out.get(32, 32));
            }
        }
        // only green clipped: nothing for the pass to do
        let wb = [2.2, 1.0, 1.6];
        let partly = RgbFrame::from_fn(64, 64, |x, _| [0.3, (0.6 + x as f32 * 0.02).min(1.0), 0.4]);
        let (mut a, mut b) = (partly.clone(), partly);
        super::super::opposed(&mut a, wb, 0.99);
        highlights(Mode::Opposed, wb).rgb(&mut b);
        assert_eq!(a.data, b.data);
    }

    #[test]
    fn clip_cfa_limits_to_the_lowest_balanced_channel() {
        let wb = [2.0, 1.0, 1.5];
        let cfa = Cfa::bayer("RGGB").unwrap();
        let mut n = mosaic_from_rgb(&RgbFrame::filled(4, 4, [1.0; 3]), &cfa);
        clip_cfa(&mut n, wb, 1.0);
        for y in 0..4 {
            for x in 0..4 {
                let c = cfa.color_at(x, y) as usize;
                assert!((n.data[y * 4 + x] * wb[c] - 1.0).abs() < 1e-6);
            }
        }
    }
}
