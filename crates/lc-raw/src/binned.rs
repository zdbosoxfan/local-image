//! Reduced-size development straight from the mosaic ("superpixel" binning).
//!
//! When the caller needs an image at most half (or a third, a quarter…) of the sensor size — a
//! 2560 px loupe preview or a grid thumbnail of a 24 MP raw — demosaicing the full mosaic and then
//! downscaling throws most of the work away. Instead every `k × k` block of CFA samples becomes one
//! output pixel whose channels are the means of the block's samples of that colour. Every block must
//! contain all three colours (Bayer: `k` even; X-Trans: `k` a multiple of 3). There is no
//! interpolation, so there are no demosaicing artefacts; the colour planes are offset by less than
//! one output pixel, which is invisible at these scales.
//!
//! Clipping is preserved for [`crate::highlight::reconstruct`]: a channel whose block contains a
//! sample at or above `clip` takes that block's maximum sample of the colour (instead of a mean that
//! would fall just below the clip level and escape reconstruction).

use crate::opcodes::Binning;
use crate::{Normalized, RawData, RawImage, Result, RgbFrame};
use rayon::prelude::*;

impl RawImage {
    /// Can `k × k` blocks be binned (single-plane CFA data, every `k × k` window containing R, G
    /// and B; Bayer: `k` even, so that every block has the same layout)?
    pub fn can_bin(&self, k: usize) -> bool {
        let Some(cfa) = &self.cfa else { return false };
        if k < 2 || self.cpp != 1 || !cfa.valid() || (cfa.is_bayer() && !k.is_multiple_of(2)) {
            return false;
        }
        // every window position relative to the pattern
        (0..cfa.height).all(|j| (0..cfa.width).all(|i| block_layout(cfa, i, j, k).1.iter().all(|&n| n > 0)))
    }

    /// Like [`RawImage::develop`] at `1/k` of the size (camera RGB, white = 1.0, default crop applied,
    /// not oriented). `None` when the data cannot be binned ([`RawImage::can_bin`]) or carries an
    /// `OpcodeList3` (see [`Self::develop_binned_with_highlights`]); callers then demosaic.
    pub fn develop_binned(&self, k: usize, clip: f32) -> Result<Option<RgbFrame>> {
        self.validate()?;
        if !self.can_bin(k) || !self.opcodes.list3.is_empty() {
            return Ok(None);
        }
        let Some((region, _)) = self.bin_region(k) else { return Ok(None) };
        self.bin_rows(k, clip, region)
    }

    /// The part of the active area to bin by `k`, and the default crop to take of the binned image
    /// afterwards with where the binned pixels sit. An `OpcodeList3` is defined on the whole
    /// active area, so with one the area is binned whole (in whole blocks on the default crop's
    /// grid, so the crop takes whole blocks and frames the image as the full development does)
    /// and cropped once the list has run; without, only the default crop is binned.
    fn bin_region(&self, k: usize) -> Option<(crate::Rect, Option<(crate::Rect, Binning)>)> {
        let a = self.active_area;
        let c = self.crop.clipped(a.width, a.height);
        let c = if c.width < k || c.height < k { crate::Rect::new(0, 0, a.width, a.height) } else { c };
        if self.opcodes.list3.is_empty() {
            return (c.width >= k && c.height >= k).then_some((c, None));
        }
        let (ox, oy) = (c.x % k, c.y % k);
        let (bw, bh) = ((a.width - ox) / k, (a.height - oy) / k);
        if bw == 0 || bh == 0 {
            return None;
        }
        let (x, y) = ((c.x / k).min(bw - 1), (c.y / k).min(bh - 1));
        let after = crate::Rect::new(x, y, (c.width / k).clamp(1, bw - x), (c.height / k).clamp(1, bh - y));
        let binning = Binning { k, origin: [ox, oy], sensor: [a.width, a.height] };
        Some((crate::Rect::new(ox, oy, bw * k, bh * k), Some((after, binning))))
    }

    /// Bin `c` (relative to the active area) straight from the samples, row by row.
    fn bin_rows(&self, k: usize, clip: f32, c: crate::Rect) -> Result<Option<RgbFrame>> {
        let Some(cfa) = &self.cfa else { return Ok(None) };
        let a = self.active_area;
        let (bw, bh) = (c.width / k, c.height / k);
        if bw == 0 || bh == 0 {
            return Ok(None);
        }
        // Opcode lists 1/2 need the normalised copy; plain files are normalised row by row.
        let norm: Option<Normalized> = if self.opcodes.list1.is_empty() && self.opcodes.list2.is_empty() { None } else { Some(self.normalized()?) };
        let cfa = cfa.shifted(a.x, a.y);
        let range = self.white_at(0) - self.black.mean();
        let scale = if range > 0.0 { 1.0 / range } else { 1.0 };
        let uniform_black =
            self.black.delta_h.is_empty() && self.black.delta_v.is_empty() && self.black.values.iter().all(|&v| v == self.black.values[0]);
        let black0 = self.black.at(0, 0, 0, 1);
        // colour of every sample of a block, per pattern phase of the block's corner
        let (pw, ph) = (cfa.width, cfa.height);
        let layouts: Vec<(Vec<u8>, [f32; 3])> = (0..ph)
            .flat_map(|j| (0..pw).map(move |i| (i, j)))
            .map(|(i, j)| {
                let (l, n) = block_layout(&cfa, i, j, k);
                (l, n.map(|n| 1.0 / n.max(1) as f32))
            })
            .collect();
        let wlen = bw * k;
        let mut out = RgbFrame::new(bw, bh);
        out.data.par_chunks_mut(bw).enumerate().for_each(|(by, row)| {
            // the block row's k sample rows, normalised
            let mut rows = vec![0f32; k * wlen];
            for dy in 0..k {
                let y = c.y + by * k + dy;
                let dst = &mut rows[dy * wlen..(dy + 1) * wlen];
                match (&norm, &self.data) {
                    (Some(nm), _) => dst.copy_from_slice(&nm.data[y * nm.width + c.x..][..wlen]),
                    (None, RawData::U16(d)) if uniform_black && self.white.len() != 4 => {
                        let src = &d[(a.y + y) * self.width + a.x + c.x..][..wlen];
                        for (o, &v) in dst.iter_mut().zip(src) {
                            *o = (v as f32 - black0) * scale;
                        }
                    }
                    (None, data) => {
                        let base = (a.y + y) * self.width + a.x + c.x;
                        for (x, o) in dst.iter_mut().enumerate() {
                            *o = (data.get(base + x) - self.black.at(c.x + x, y, 0, 1)) * self.normalization_scale(c.x + x, y, 0).unwrap_or(scale);
                        }
                    }
                }
            }
            let py = (c.y + by * k) % ph;
            for (bx, px) in row.iter_mut().enumerate() {
                let (layout, inv) = &layouts[py * pw + (c.x + bx * k) % pw];
                let (mut sum, mut max) = ([0f32; 3], [f32::MIN; 3]);
                for dy in 0..k {
                    let src = &rows[dy * wlen + bx * k..][..k];
                    for (&v, &col) in src.iter().zip(&layout[dy * k..(dy + 1) * k]) {
                        let col = col as usize;
                        sum[col] += v;
                        max[col] = max[col].max(v);
                    }
                }
                for ch in 0..3 {
                    px[ch] = if max[ch] >= clip { max[ch] } else { sum[ch] * inv[ch] };
                }
            }
        });
        Ok(Some(out))
    }

    /// Binned development with highlight handling `h`, run on the sensor data before
    /// `OpcodeList3`: the darktable modes on the reduced mosaic ([`Self::develop_binned_with`]),
    /// as at full size, Clip and Reconstruct on the binned RGB, whose clipped channels keep their
    /// block maximum. Unlike [`Self::develop_binned`] this also bins raws whose `OpcodeList3`
    /// holds only operations defined in relative coordinates ([`crate::opcodes::list3_binnable`]:
    /// the warps, vignette and gain maps of DJI and Hasselblad drone DNGs), evaluating them at
    /// the binned size.
    pub fn develop_binned_with_highlights(&self, k: usize, h: &crate::highlight::Highlights) -> Result<Option<RgbFrame>> {
        self.validate()?;
        if !self.can_bin(k) || !crate::opcodes::list3_binnable(&self.opcodes.list3) {
            return Ok(None);
        }
        let Some((region, after)) = self.bin_region(k) else { return Ok(None) };
        let img = if matches!(h.mode, crate::highlight::Mode::Opposed | crate::highlight::Mode::Segmentation) {
            self.bin_mosaic(k, h.clip, region, |n, scale| h.inpaint(n, scale))?.map(|mut img| {
                h.binned(&mut img);
                img
            })
        } else {
            self.bin_rows(k, h.clip, region)?.map(|mut img| {
                h.rgb(&mut img);
                img
            })
        };
        match (img, after) {
            (Some(mut img), Some((c, binning))) => {
                crate::opcodes::apply_list3_binned(&self.opcodes.list3, &mut img, binning);
                let (cw, ch) = (c.width.min(img.width - c.x), c.height.min(img.height - c.y));
                Ok(Some(img.into_crop(c.x, c.y, cw, ch)))
            }
            (img, _) => Ok(img),
        }
    }

    /// Binned development with a pre-demosaic CFA operation. Bayer blocks are reduced to
    /// four separate CFA phases at twice the RGB output size, preserving a clipped phase's
    /// maximum. `hook` receives the reduced mosaic and its sensor sampling interval.
    /// X-Trans uses a full-size CFA hook because arbitrary reduction cannot preserve its phase.
    pub fn develop_binned_with(&self, k: usize, clip: f32, hook: impl FnOnce(&mut Normalized, usize)) -> Result<Option<RgbFrame>> {
        self.validate()?;
        if !self.can_bin(k) || !self.opcodes.list3.is_empty() {
            return Ok(None);
        }
        let Some((region, _)) = self.bin_region(k) else { return Ok(None) };
        self.bin_mosaic(k, clip, region, hook)
    }

    /// Bin `c` (relative to the active area) after `hook` ran on the mosaic, reduced as
    /// [`Self::develop_binned_with`] describes.
    fn bin_mosaic(&self, k: usize, clip: f32, c: crate::Rect, hook: impl FnOnce(&mut Normalized, usize)) -> Result<Option<RgbFrame>> {
        let n = self.normalized_samples()?;
        let Some(cfa) = n.cfa.clone() else {
            return Ok(None);
        };
        if c.width < k || c.height < k {
            return Ok(None);
        }
        if !cfa.is_bayer() {
            let mut n = self.normalized()?;
            hook(&mut n, 1);
            return Ok(Some(crate::highlight::superpixels(&n, &cfa, c, k, clip)));
        }
        let scale = k / 2;
        let (w, h) = (2 * (c.width / k), 2 * (c.height / k));
        let data: Vec<f32> = (0..w * h)
            .into_par_iter()
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let (x0, y0) = (c.x + (x / 2) * k + x % 2, c.y + (y / 2) * k + y % 2);
                let (mut sum, mut max) = (0.0, f32::MIN);
                for dy in 0..scale {
                    for dx in 0..scale {
                        let v = n.get(x0 + 2 * dx, y0 + 2 * dy, 0);
                        sum += v;
                        max = max.max(v);
                    }
                }
                if max >= clip { max } else { sum / (scale * scale) as f32 }
            })
            .collect();
        let reduced_cfa = cfa.shifted(c.x, c.y);
        let mut reduced = Normalized { width: w, height: h, cpp: 1, data, cfa: Some(reduced_cfa.clone()) };
        hook(&mut reduced, scale);
        Ok(Some(crate::highlight::superpixels(&reduced, &reduced_cfa, crate::Rect::new(0, 0, w, h), 2, clip)))
    }
}

/// Colours of the `k × k` samples of a block whose corner sits at pattern position `(i, j)`
/// (row-major) and the count of each colour.
fn block_layout(cfa: &crate::Cfa, i: usize, j: usize, k: usize) -> (Vec<u8>, [u32; 3]) {
    let mut n = [0u32; 3];
    let l: Vec<u8> = (0..k * k)
        .map(|t| {
            let c = cfa.color_at(i + t % k, j + t / k).min(2);
            n[c as usize] += 1;
            c
        })
        .collect();
    (l, n)
}

#[cfg(test)]
mod tests {
    use crate::demosaic::mosaic_from_rgb;
    use crate::*;

    fn raw_from(n: &Normalized, black: f32, white: f32) -> RawImage {
        let data = n.data.iter().map(|&v| (v * (white - black) + black).round() as u16).collect();
        RawImage {
            format: RawFormat::Dng,
            width: n.width,
            height: n.height,
            cpp: 1,
            data: RawData::U16(data),
            cfa: n.cfa.clone(),
            bits: 14,
            black: BlackLevel::uniform(black),
            white: vec![white],
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

    #[test]
    fn binning_matches_block_means_and_keeps_clipping() {
        let img = RgbFrame::from_fn(24, 18, |x, y| [0.1 + x as f32 * 0.01, 0.2 + y as f32 * 0.01, 0.3]);
        let mut raw = raw_from(&mosaic_from_rgb(&img, &Cfa::bayer("GRBG").unwrap()), 512.0, 16383.0);
        assert!(raw.can_bin(2) && raw.can_bin(4) && !raw.can_bin(3));
        let b = raw.develop_binned(2, 0.99).unwrap().unwrap();
        assert_eq!((b.width, b.height), (12, 9));
        let p = b.get(3, 2);
        // red sample of block (3,2) under GRBG is at (7,4); blue at (6,5); greens average (6,4)+(7,5)
        assert!((p[0] - img.get(7, 4)[0]).abs() < 1e-3, "{p:?}");
        assert!((p[2] - 0.3).abs() < 1e-3);
        assert!((p[1] - (img.get(6, 4)[1] + img.get(7, 5)[1]) / 2.0).abs() < 1e-3);
        // a single clipped green sample keeps the block's green clipped
        if let RawData::U16(d) = &mut raw.data {
            d[4 * 24 + 6] = 16383;
        }
        assert!(raw.develop_binned(2, 0.99).unwrap().unwrap().get(3, 2)[1] >= 0.99);
        // default crop is honoured at the binned scale
        raw.crop = Rect::new(2, 2, 20, 12);
        let c = raw.develop_binned(2, 0.99).unwrap().unwrap();
        assert_eq!((c.width, c.height), (10, 6));
        // X-Trans: 3×3 blocks hold all colours, 2×2 don't
        let x = raw_from(&mosaic_from_rgb(&img, &Cfa::xtrans()), 0.0, 1000.0);
        assert!(x.can_bin(3) && x.can_bin(6) && !x.can_bin(2));
        let xb = x.develop_binned(3, 0.99).unwrap().unwrap();
        assert_eq!((xb.width, xb.height), (8, 6));
        assert!((xb.get(4, 3)[2] - 0.3).abs() < 2e-3);
    }

    #[test]
    fn binning_matches_demosaic_downscaled_on_smooth_images() {
        let img = RgbFrame::from_fn(64, 48, |x, y| [0.2 + 0.3 * (x as f32 / 64.0), 0.4 - 0.2 * (y as f32 / 48.0), 0.25]);
        let raw = raw_from(&mosaic_from_rgb(&img, &Cfa::bayer("RGGB").unwrap()), 0.0, 4095.0);
        let b = raw.develop_binned(2, 0.99).unwrap().unwrap();
        let full = raw.develop(Method::Ahd).unwrap().half();
        for y in 2..b.height - 2 {
            for x in 2..b.width - 2 {
                for c in 0..3 {
                    assert!((b.get(x, y)[c] - full.get(x, y)[c]).abs() < 0.01, "({x},{y})");
                }
            }
        }
    }

    /// DJI and Hasselblad drone DNGs keep their warp, vignette and gain map in OpcodeList3. Those
    /// are defined in relative coordinates, so previews bin the sensor and run them at the binned
    /// size instead of demosaicing and warping 100 MP, and match the full development, framed
    /// exactly as it is when the default crop does not start on a block boundary.
    #[test]
    fn drone_lens_opcodes_run_at_the_binned_size() {
        let (w, h) = (192usize, 144usize);
        let wave = |v: f32, period: f32| 0.06 * (std::f32::consts::TAU * v / period).sin();
        let img = RgbFrame::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            [
                0.2 + 0.25 * fx + wave(x as f32, 48.0),
                0.3 + 0.2 * fy * fx + wave(y as f32, 40.0),
                0.25 + 0.15 * (1.0 - fy) + wave((x + y) as f32, 56.0),
            ]
        });
        let mut raw = raw_from(&mosaic_from_rgb(&img, &Cfa::bayer("RGGB").unwrap()), 256.0, 16383.0);
        raw.crop = Rect::new(6, 12, 176, 120);
        let area = crate::opcodes::Area { bottom: h as u32, right: w as u32 - 16, planes: 3, row_pitch: 1, col_pitch: 1, ..Default::default() };
        raw.opcodes.list3 = vec![
            Opcode::FixVignetteRadial { k: [0.6, 0.0, 0.0, 0.0, 0.0], center: [0.5, 0.5] },
            Opcode::WarpRectilinear {
                planes: vec![[0.97, 0.02, 0.0, 0.0, 0.0, 0.0], [0.97, 0.03, 0.0, 0.0, 0.0, 0.0], [0.975, 0.02, 0.0, 0.0, 0.0, 0.0]],
                center: [0.48, 0.52],
            },
            Opcode::GainMap {
                area,
                points_v: 2,
                points_h: 2,
                spacing: [1.0, 1.0],
                origin: [0.0, 0.0],
                map_planes: 1,
                gains: vec![1.0, 1.3, 1.1, 1.5],
            },
        ];
        let highlights = crate::highlight::Highlights { mode: crate::highlight::Mode::Reconstruct, wb: [1.0; 3], clip: 0.99 };
        assert!(raw.develop_binned(2, 0.99).unwrap().is_none(), "the plain binner still refuses OpcodeList3");
        let full = raw.develop_with_highlights(Method::Bilinear, &Default::default(), &highlights).unwrap();
        assert_eq!((full.width, full.height), (176, 120));
        for k in [2, 4, 8] {
            let binned = raw.develop_binned_with_highlights(k, &highlights).unwrap().unwrap();
            assert_eq!((binned.width, binned.height), (176 / k, 120 / k));
            let (mut err, mut n) = (0.0f64, 0);
            for by in 2..binned.height - 2 {
                for bx in 2..binned.width - 2 {
                    for c in 0..3 {
                        let mean = (0..k * k).map(|i| full.get(bx * k + i % k, by * k + i / k)[c]).sum::<f32>() / (k * k) as f32;
                        err += ((binned.get(bx, by)[c] - mean) / mean).abs() as f64;
                        n += 1;
                    }
                }
            }
            assert!(err / (n as f64) < 0.01, "k = {k}: mean relative error {}", err / n as f64);
        }
        // a pixel-indexed opcode still needs the full development
        raw.opcodes.list3.push(Opcode::DeltaPerRow { area, deltas: vec![0.01; h] });
        assert!(raw.develop_binned_with_highlights(2, &highlights).unwrap().is_none());
    }

    /// Previews run the same sensor-domain inpaint opposed as the full-size development: the CFA
    /// port on the reduced mosaic, then the neutral pass on the binned RGB.
    #[test]
    fn binned_inpaint_opposed_runs_on_the_mosaic() {
        use crate::highlight::{Highlights, Mode, neutralize_clipped, opposed_cfa};
        let wb = [2.2, 1.0, 1.6];
        let img = RgbFrame::from_fn(64, 48, |x, y| {
            let b = 0.4 + 1.6 * (-((x as f32 - 30.0).powi(2) + (y as f32 - 22.0).powi(2)) / 120.0).exp();
            [(b * 0.9 / wb[0]).min(1.0), (b / wb[1]).min(1.0), (b * 0.7 / wb[2]).min(1.0)]
        });
        let raw = raw_from(&mosaic_from_rgb(&img, &Cfa::bayer("RGGB").unwrap()), 0.0, 4095.0);
        let highlights = Highlights { mode: Mode::Opposed, wb, clip: 0.99 };
        for k in [2, 4] {
            let binned = raw.develop_binned_with_highlights(k, &highlights).unwrap().unwrap();
            let mut expected = raw
                .develop_binned_with(k, 0.99, |n, _| {
                    opposed_cfa(n, wb, 0.99);
                })
                .unwrap()
                .unwrap();
            neutralize_clipped(&mut expected, wb, 0.99);
            assert_eq!((binned.width, binned.height), (expected.width, expected.height));
            for (a, b) in binned.data.iter().zip(&expected.data) {
                assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() <= 1e-5 * b.abs().max(1.0)), "k = {k}: {a:?} {b:?}");
            }
        }
    }

    #[test]
    fn cfa_hook_preserves_phases_crop_black_and_clipping() {
        let img = RgbFrame::from_fn(50, 42, |x, y| [0.12 + x as f32 * 0.003, 0.22 + y as f32 * 0.004, 0.35]);
        for pattern in ["RGGB", "BGGR", "GRBG", "GBRG"] {
            let mut raw = raw_from(&mosaic_from_rgb(&img, &Cfa::bayer(pattern).unwrap()), 512.0, 16383.0);
            raw.active_area = Rect::new(1, 1, 48, 40);
            raw.crop = Rect::new(1, 3, 40, 32);
            raw.black.values = vec![511.0, 512.0, 513.0, 514.0];
            raw.black.repeat_cols = 2;
            raw.black.repeat_rows = 2;
            if let RawData::U16(d) = &mut raw.data {
                d[6 * raw.width + 5] = 16383;
            }
            for k in [2, 4, 6, 8] {
                let old = raw.develop_binned(k, 0.99).unwrap().unwrap();
                let new = raw
                    .develop_binned_with(k, 0.99, |n, scale| {
                        assert_eq!(scale, k / 2);
                        assert_eq!((n.width, n.height), (2 * (40 / k), 2 * (32 / k)));
                        let shifted = raw.cfa.as_ref().unwrap().shifted(2, 4);
                        assert_eq!(n.cfa.as_ref(), Some(&shifted));
                    })
                    .unwrap()
                    .unwrap();
                assert_eq!((new.width, new.height), (old.width, old.height));
                for (a, b) in new.data.iter().zip(&old.data) {
                    for c in 0..3 {
                        assert!((a[c] - b[c]).abs() < 2e-7, "{pattern} k={k}: {a:?} {b:?}");
                    }
                }
                let altered = raw
                    .develop_binned_with(k, 0.99, |n, _| {
                        // Distinguish the two green phases; they must both reach the hook.
                        for y in 0..n.height {
                            for x in 0..n.width {
                                n.data[y * n.width + x] = [0.1, 0.2, 0.4, 0.8][(y % 2) * 2 + x % 2];
                            }
                        }
                    })
                    .unwrap()
                    .unwrap();
                let shifted = raw.cfa.as_ref().unwrap().shifted(2, 4);
                let mut expected = [0.0; 3];
                let mut counts = [0.0; 3];
                for (i, v) in [0.1, 0.2, 0.4, 0.8].into_iter().enumerate() {
                    let c = shifted.color_at(i % 2, i / 2) as usize;
                    expected[c] += v;
                    counts[c] += 1.0;
                }
                for c in 0..3 {
                    expected[c] /= counts[c];
                }
                assert!(altered.data.iter().all(|p| *p == expected));
            }
        }
    }

    #[test]
    fn xtrans_hook_runs_before_crop_and_binning() {
        let img = RgbFrame::from_fn(48, 42, |_, _| [0.2, 0.3, 0.4]);
        let mut raw = raw_from(&mosaic_from_rgb(&img, &Cfa::xtrans()), 0.0, 4095.0);
        raw.crop = Rect::new(1, 2, 42, 36);
        let out = raw
            .develop_binned_with(6, 0.99, |n, scale| {
                assert_eq!(scale, 1);
                assert_eq!((n.width, n.height), (48, 42));
                n.data.fill(0.5);
            })
            .unwrap()
            .unwrap();
        assert_eq!((out.width, out.height), (7, 6));
        assert!(out.data.iter().all(|p| *p == [0.5; 3]));
    }

    #[test]
    fn cfa_hook_follows_mosaic_opcodes_and_defers_rgb_opcodes() {
        let img = RgbFrame::from_fn(48, 40, |_, _| [0.1, 0.2, 0.3]);
        let mut raw = raw_from(&mosaic_from_rgb(&img, &Cfa::bayer("RGGB").unwrap()), 256.0, 4095.0);
        raw.opcodes.list2.push(Opcode::MapPolynomial {
            area: crate::opcodes::Area { bottom: 40, right: 48, planes: 1, row_pitch: 1, col_pitch: 1, ..Default::default() },
            coefficients: vec![0.0, 2.0],
        });
        let expected = raw.normalized().unwrap();
        let full = raw
            .develop_with_cfa(Method::Bilinear, &Default::default(), |n| {
                assert_eq!(n.data, expected.data);
                n.data.fill(0.25);
            })
            .unwrap();
        assert!(full.data.iter().all(|p| *p == [0.25; 3]));
        let binned = raw
            .develop_binned_with(2, 0.99, |n, scale| {
                assert_eq!(scale, 1);
                assert_eq!(n.data, expected.data);
                n.data.fill(0.25);
            })
            .unwrap()
            .unwrap();
        assert!(binned.data.iter().all(|p| *p == [0.25; 3]));
        raw.opcodes.list3.push(Opcode::FixVignetteRadial { k: [0.0; 5], center: [0.5; 2] });
        let called = std::cell::Cell::new(false);
        assert!(raw.develop_binned_with(2, 0.99, |_, _| called.set(true)).unwrap().is_none());
        assert!(!called.get(), "RGB opcodes require full development");
    }
}
