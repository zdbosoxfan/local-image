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

use crate::{Normalized, RawData, RawImage, Result, Rgb32f};
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
    /// `OpcodeList3` (whose operations are defined at full resolution); callers then demosaic.
    pub fn develop_binned(&self, k: usize, clip: f32) -> Result<Option<Rgb32f>> {
        self.validate()?;
        if !self.can_bin(k) || !self.opcodes.list3.is_empty() {
            return Ok(None);
        }
        let Some(cfa) = &self.cfa else { return Ok(None) };
        let a = self.active_area;
        // bin the default crop (relative to the active area) only
        let c = self.crop.clipped(a.width, a.height);
        let c = if c.width < k || c.height < k { crate::Rect::new(0, 0, a.width, a.height) } else { c };
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
        let mut out = Rgb32f::new(bw, bh);
        out.data.par_chunks_mut(bw).enumerate().for_each(|(by, row)| {
            // the block row's k sample rows, normalised
            let mut rows = vec![0f32; k * wlen];
            for dy in 0..k {
                let y = c.y + by * k + dy;
                let dst = &mut rows[dy * wlen..(dy + 1) * wlen];
                match (&norm, &self.data) {
                    (Some(nm), _) => dst.copy_from_slice(&nm.data[y * nm.width + c.x..][..wlen]),
                    (None, RawData::U16(d)) if uniform_black => {
                        let src = &d[(a.y + y) * self.width + a.x + c.x..][..wlen];
                        for (o, &v) in dst.iter_mut().zip(src) {
                            *o = (v as f32 - black0) * scale;
                        }
                    }
                    (None, data) => {
                        let base = (a.y + y) * self.width + a.x + c.x;
                        for (x, o) in dst.iter_mut().enumerate() {
                            *o = (data.get(base + x) - self.black.at(c.x + x, y, 0, 1)) * scale;
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

    /// Binned development with a pre-demosaic CFA operation. Bayer blocks are reduced to
    /// four separate CFA phases at twice the RGB output size, preserving a clipped phase's
    /// maximum. `hook` receives the reduced mosaic and its sensor sampling interval.
    /// X-Trans uses a full-size CFA hook because arbitrary reduction cannot preserve its phase.
    pub fn develop_binned_with(&self, k: usize, clip: f32, hook: impl FnOnce(&mut Normalized, usize)) -> Result<Option<Rgb32f>> {
        self.validate()?;
        if !self.can_bin(k) || !self.opcodes.list3.is_empty() {
            return Ok(None);
        }
        let mut n = self.normalized()?;
        let Some(cfa) = n.cfa.clone() else {
            return Ok(None);
        };
        let c = self.crop.clipped(n.width, n.height);
        let c = if c.width < k || c.height < k { crate::Rect::new(0, 0, n.width, n.height) } else { c };
        if c.width < k || c.height < k {
            return Ok(None);
        }
        if !cfa.is_bayer() {
            hook(&mut n, 1);
            return Ok(Some(bin_normalized(&n, &cfa, c, k, clip)));
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
                        let v = n.data[(y0 + 2 * dy) * n.width + x0 + 2 * dx];
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
        Ok(Some(bin_normalized(&reduced, &reduced_cfa, crate::Rect::new(0, 0, w, h), 2, clip)))
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

/// Same clipping-aware channel reduction as the historical row-normalized binner.
fn bin_normalized(n: &Normalized, cfa: &crate::Cfa, crop: crate::Rect, k: usize, clip: f32) -> Rgb32f {
    Rgb32f::from_fn(crop.width / k, crop.height / k, |bx, by| {
        let (mut sum, mut max, mut count) = ([0.0; 3], [f32::MIN; 3], [0usize; 3]);
        for dy in 0..k {
            for dx in 0..k {
                let (x, y) = (crop.x + bx * k + dx, crop.y + by * k + dy);
                let c = cfa.color_at(x, y) as usize;
                let v = n.data[y * n.width + x];
                sum[c] += v;
                max[c] = max[c].max(v);
                count[c] += 1;
            }
        }
        std::array::from_fn(|c| if max[c] >= clip { max[c] } else { sum[c] / count[c].max(1) as f32 })
    })
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
            metadata: Metadata::default(),
        }
    }

    #[test]
    fn binning_matches_block_means_and_keeps_clipping() {
        let img = Rgb32f::from_fn(24, 18, |x, y| [0.1 + x as f32 * 0.01, 0.2 + y as f32 * 0.01, 0.3]);
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
        let img = Rgb32f::from_fn(64, 48, |x, y| [0.2 + 0.3 * (x as f32 / 64.0), 0.4 - 0.2 * (y as f32 / 48.0), 0.25]);
        let raw = raw_from(&mosaic_from_rgb(&img, &Cfa::bayer("RGGB").unwrap()), 0.0, 4095.0);
        let b = raw.develop_binned(2, 0.99).unwrap().unwrap();
        let full = lightcraft_raster::resample::half(&raw.develop(Method::Ahd).unwrap());
        for y in 2..b.height - 2 {
            for x in 2..b.width - 2 {
                for c in 0..3 {
                    assert!((b.get(x, y)[c] - full.get(x, y)[c]).abs() < 0.01, "({x},{y})");
                }
            }
        }
    }

    #[test]
    fn cfa_hook_preserves_phases_crop_black_and_clipping() {
        let img = Rgb32f::from_fn(50, 42, |x, y| [0.12 + x as f32 * 0.003, 0.22 + y as f32 * 0.004, 0.35]);
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
        let img = Rgb32f::from_fn(48, 42, |_, _| [0.2, 0.3, 0.4]);
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
        let img = Rgb32f::from_fn(48, 40, |_, _| [0.1, 0.2, 0.3]);
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
