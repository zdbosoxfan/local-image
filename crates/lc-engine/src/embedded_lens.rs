//! Vendor metadata adapter for the shared Develop lens model.
//!
//! Port of darktable `src/iop/lens.cc::_init_coeffs_md_v2` at
//! 733bd69f32cac7ff5e41025115942772add1f088 (GPL-3.0-or-later).
//! The raw reader retains the original coefficients; this adapter alone builds
//! the inverse map and source attenuation splines used by CPU and GPU Develop.

use lightcraft_develop::EmbeddedRadial;
use lightcraft_geom::Point;
use lightcraft_pipeline::optics::interpolate_radial as interpolate;
use lightcraft_raw::embedded_lens::VendorLens;

pub(crate) fn radial(data: &VendorLens, w: f64, h: f64) -> Option<EmbeddedRadial> {
    let mut out = EmbeddedRadial {
        center: Point::new(0.5, 0.5),
        radius: 0.5 * w.hypot(h) / w.max(h),
        scales: [[1.0; 16]; 3],
        attenuation: [1.0; 16],
        ..Default::default()
    };
    match data {
        VendorLens::Sony { distortion, ca_red, ca_blue, vignetting } => {
            let n = distortion.len();
            if !(2..=16).contains(&n) || [ca_red.len(), ca_blue.len(), vignetting.len()].iter().any(|&l| l != n) {
                return None;
            }
            out.count = n;
            out.vig_count = n;
            for i in 0..n {
                out.knots[i] = (i as f64 + 0.5) / (n - 1) as f64;
                out.vig_knots[i] = out.knots[i];
                let d = 1.0 + distortion[i] / 16384.0;
                out.scales[0][i] = d * (1.0 + ca_red[i] / 2097152.0);
                out.scales[1][i] = d;
                out.scales[2][i] = d * (1.0 + ca_blue[i] / 2097152.0);
                out.attenuation[i] = (0.5 - (vignetting[i] / 8192.0 - 1.0).exp2()).exp2();
            }
        }
        VendorLens::Fuji { knots, distortion, ca_red, ca_blue, vignetting, crop_factor } => {
            let n = knots.len();
            if !(2..=15).contains(&n) || [distortion.len(), ca_red.len(), ca_blue.len(), vignetting.len()].iter().any(|&l| l != n) {
                return None;
            }
            let offset = usize::from(knots[0] > 0.0);
            let ni = n + offset;
            let mut input = [0.0; 16];
            let mut d = [1.0; 16];
            let mut r = [0.0; 16];
            let mut b = [0.0; 16];
            for i in 0..n {
                let j = i + offset;
                input[j] = crop_factor * knots[i];
                d[j] = 1.0 + distortion[i] / 100.0;
                r[j] = ca_red[i];
                b[j] = ca_blue[i];
                out.attenuation[j] = vignetting[i] / 100.0;
            }
            out.vig_count = ni;
            out.vig_knots = input;
            // Keep every calibration knot, then spend the remaining 16-knot
            // budget where inverse-map interpolation has the largest error.
            // The same Fuji source-radius equation/linear interpolation is
            // retained. Uniform-source v2 sampling misses knot discontinuities.
            let mut nodes = input[..ni].to_vec();
            let evaluate = |source: f64| {
                let m = interpolate(&input, &d, ni, source);
                [m * (1.0 + interpolate(&input, &r, ni, source)), m, m * (1.0 + interpolate(&input, &b, ni, source))]
            };
            loop {
                out.count = nodes.len();
                for (i, &source) in nodes.iter().enumerate() {
                    let scales = evaluate(source);
                    out.knots[i] = source / scales[1];
                    for ch in 0..3 {
                        out.scales[ch][i] = scales[ch];
                    }
                }
                if out.knots[..out.count].windows(2).any(|p| p[0] >= p[1]) || out.scales[1][..out.count].iter().any(|m| !m.is_finite() || *m <= 0.0) {
                    return None;
                }
                if nodes.len() == 16 {
                    break;
                }
                let mut worst = -1.0;
                let mut candidate = None;
                for pair in nodes.windows(2) {
                    for step in 1..16 {
                        let source = pair[0] + (pair[1] - pair[0]) * step as f64 / 16.0;
                        if source <= pair[0] || source >= pair[1] {
                            continue;
                        }
                        let scales = evaluate(source);
                        let dest = source / scales[1];
                        let error = (0..3)
                            .map(|ch| dest * (interpolate(&out.knots, &out.scales[ch], out.count, dest) - scales[ch]).abs())
                            .fold(0.0, f64::max);
                        if error > worst {
                            worst = error;
                            candidate = Some(source);
                        }
                    }
                }
                let Some(source) = candidate else { break };
                nodes.push(source);
                nodes.sort_by(f64::total_cmp);
            }
        }
        VendorLens::Olympus { distortion, ca } => {
            let [k2, k4, k6, rs] = distortion.unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let [r0, r2, r4, b0, b2, b4] = ca.unwrap_or([0.0; 6]);
            out.count = 16;
            for i in 0..16 {
                let r = i as f64 / 15.0;
                out.knots[i] = r;
                let rs2 = (r * rs).powi(2);
                let d = rs * (1.0 + rs2 * (k2 + rs2 * (k4 + rs2 * k6)));
                let rd2 = (r * d).powi(2);
                out.scales[1][i] = d;
                // At r=0 darktable leaves all three scales equal.
                out.scales[0][i] = d * (1.0 + if i == 0 { 0.0 } else { r0 + rd2 * (r2 + rd2 * r4) });
                out.scales[2][i] = d * (1.0 + if i == 0 { 0.0 } else { b0 + rd2 * (b2 + rd2 * b4) });
            }
        }
        VendorLens::Panasonic { coefficients: [a, b, c], scale } => {
            out.count = 16;
            for i in 0..16 {
                let r = i as f64 / 15.0;
                out.knots[i] = r;
                // The established forward equation is retained. Darktable's
                // two fixed-point steps leave 18.25 px of corner error on the
                // CC0 LX100 sample; converge with safeguarded Newton instead.
                let rd = invert_panasonic(r, [*a, *b, *c], *scale)?;
                let d = if r > 0.0 { rd / r } else { 1.0 };
                for channel in &mut out.scales {
                    channel[i] = d;
                }
            }
        }
    }
    // Reject invalid/non-monotone tables before either render backend sees them.
    let n = out.count;
    if out.knots[..n].iter().any(|x| !x.is_finite())
        || out.knots[..n].windows(2).any(|x| x[0] >= x[1])
        || out.scales.iter().any(|c| c[..n].iter().any(|x| !x.is_finite() || *x <= 0.0))
        || out.attenuation[..out.vig_count].iter().any(|x| !x.is_finite() || *x <= 0.0)
        || out.vig_knots[..out.vig_count].windows(2).any(|x| x[0] >= x[1])
    {
        return None;
    }
    // Darktable v2 autoscale: retain the largest usable image without blank
    // borders in any colour channel. Vignetting remains in source coordinates.
    let short_radius = w.min(h) / w.hypot(h);
    let mut scale = 0.0_f64;
    for i in 0..200 {
        let x = short_radius + (1.0 - short_radius) * i as f64 / 199.0;
        for channel in &out.scales {
            scale = scale.max(interpolate(&out.knots, channel, n, x));
        }
    }
    for i in 0..n {
        out.knots[i] *= scale;
        for channel in &mut out.scales {
            channel[i] /= scale;
        }
    }
    Some(out)
}

fn invert_panasonic(r: f64, [a, b, c]: [f64; 3], scale: f64) -> Option<f64> {
    let forward = |x: f64| {
        let q = x * x;
        x * (1.0 + scale * q * (a + q * (b + q * c)))
    };
    let mut lo = 0.0;
    let mut hi = r.max(1.0);
    while forward(hi) < r && hi < 8.0 {
        hi *= 2.0;
    }
    if !forward(hi).is_finite() || forward(hi) < r {
        return None;
    }
    let mut x = r;
    for _ in 0..32 {
        let error = forward(x) - r;
        if error.abs() < 1e-13 {
            return Some(x);
        }
        if error > 0.0 {
            hi = x;
        } else {
            lo = x;
        }
        let q = x * x;
        let derivative = 1.0 + scale * q * (3.0 * a + q * (5.0 * b + q * 7.0 * c));
        let step = x - error / derivative;
        x = if derivative > 0.0 && step > lo && step < hi { step } else { (lo + hi) * 0.5 };
    }
    ((forward(x) - r).abs() < 1e-10).then_some(x)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lightcraft_develop::DevelopSettings;
    use lightcraft_pipeline::optics::Warp;

    #[test]
    fn lenscorr_fuji_real_calibration_knots_reduce_inverse_error() {
        // CC0 raw.pixls.us 2783, X-T3 XF35/2. Independent piecewise
        // source-radius equation, including the calibration's change in slope.
        let knots: Vec<f64> = [502.0, 710.0, 870.0, 1004.0, 1123.0, 1230.0, 1328.0, 1420.0, 1506.0].map(|v| v / 1420.0).to_vec();
        let distortion: Vec<f64> =
            [-30736.0, -78105.0, -131263.0, -189473.0, -254105.0, -327052.0, -409052.0, -496000.0, -582947.0].map(|v| v / 65536.0).to_vec();
        let profile = VendorLens::Fuji {
            knots: knots.clone(),
            distortion: distortion.clone(),
            ca_red: vec![0.0; 9],
            ca_blue: vec![0.0; 9],
            vignetting: vec![100.0; 9],
            crop_factor: 1.0,
        };
        let table = radial(&profile, 6240.0, 4160.0).unwrap();
        let mut source_knots = vec![0.0];
        source_knots.extend(&knots);
        let mut multipliers = vec![1.0];
        multipliers.extend(distortion.iter().map(|v| 1.0 + v / 100.0));
        let reference = |x: f64| {
            let i = (1..source_knots.len()).find(|&i| x <= source_knots[i]);
            match i {
                Some(i) => {
                    multipliers[i - 1] + (x - source_knots[i - 1]) * (multipliers[i] - multipliers[i - 1]) / (source_knots[i] - source_knots[i - 1])
                }
                None => *multipliers.last().unwrap(),
            }
        };
        let mut uniform_knots = [0.0; 16];
        let mut uniform_scales = [1.0; 16];
        for i in 0..16 {
            let source = i as f64 / 15.0;
            uniform_scales[i] = reference(source);
            uniform_knots[i] = source / uniform_scales[i];
        }
        let last = table.count - 1;
        let autoscale = table.knots[last] / (knots[8] / reference(knots[8]));
        let radius = 6240.0_f64.hypot(4160.0) / 2.0;
        let (mut old, mut improved) = (0.0_f64, 0.0_f64);
        for i in 0..=4096 {
            let source = i as f64 / 4096.0;
            let dest = source / reference(source);
            let approximate = dest * interpolate(&uniform_knots, &uniform_scales, 16, dest);
            old = old.max((approximate - source).abs() * radius);
            let dest = dest * autoscale;
            let actual = dest * interpolate(&table.knots, &table.scales[1], table.count, dest);
            improved = improved.max((actual - source).abs() * radius);
        }
        println!("Fuji X-T3 inverse-map error: uniform v2 {old:.6} px, calibration-preserving 16 knots {improved:.6} px");
        assert!(improved < 0.1 && improved < old / 10.0);
    }

    #[test]
    fn lenscorr_dng_preserves_crop_offcenter_tangential_and_orientation() {
        let coeff = [1.0, -0.03, 0.01, 0.0, 0.001, -0.002];
        let raw = lightcraft_raw::RawInfo {
            format: lightcraft_raw::RawFormat::Dng,
            width: 130,
            height: 90,
            cpp: 3,
            cfa: None,
            bits: 16,
            active_area: lightcraft_raw::Rect::new(0, 0, 120, 80),
            crop: lightcraft_raw::Rect::new(10, 10, 100, 60),
            orientation: lightcraft_geom::Orientation::Rotate90,
            color: Default::default(),
            wb_multipliers: None,
            opcodes: lightcraft_raw::OpcodeLists {
                list3: vec![
                    lightcraft_raw::Opcode::WarpRectilinear { planes: vec![coeff], center: [0.4, 0.6] },
                    lightcraft_raw::Opcode::FixVignetteRadial { k: [0.2, 0.0, 0.0, 0.0, 0.0], center: [0.4, 0.6] },
                ],
                ..Default::default()
            },
            vendor_lens: Some(VendorLens::Panasonic { coefficients: [0.1, 0.0, 0.0], scale: 1.0 }),
            metadata: Default::default(),
        };
        let lens = crate::files::embedded_lens(&raw).unwrap();
        assert!(lens.radial.is_none(), "DNG opcode takes precedence over vendor round-trip data");
        let m = (71.4_f64).hypot(47.4);
        let p = Point::new(20.5, 10.5);
        let dx = (p.x + 10.0 - 48.1) / m;
        let dy = (p.y + 10.0 - 47.9) / m;
        let r2 = dx * dx + dy * dy;
        let radial = 1.0 - 0.03 * r2 + 0.01 * r2 * r2;
        let sx = 48.1 + m * (dx * radial + 0.001 * 2.0 * dx * dy - 0.002 * (r2 + 2.0 * dx * dx)) - 10.0;
        let sy = 47.9 + m * (dy * radial - 0.002 * 2.0 * dx * dy + 0.001 * (r2 + 2.0 * dy * dy)) - 10.0;
        let rotated = Point::new(60.0 - p.y, p.x);
        let actual = lightcraft_pipeline::optics::embedded_warp(&lens.warp.unwrap(), rotated, 0, 60.0, 100.0);
        assert!(actual.dist(Point::new(60.0 - sy, sx)) < 1e-9);
        let gain = lightcraft_pipeline::optics::embedded_vignette_gain(&lens.vignette.unwrap(), rotated, 60.0, 100.0);
        assert!((gain - (1.0 + 0.2 * r2)).abs() < 1e-12);
        let mut fisheye = raw.clone();
        fisheye.opcodes.list3 = vec![lightcraft_raw::Opcode::WarpFisheye { planes: vec![[1.0, 0.0, 0.0, 0.0]], center: [0.4, 0.6] }];
        let lens = crate::files::embedded_lens(&fisheye).unwrap();
        assert!(lens.warp.unwrap().fisheye);
        assert!(crate::files::is_lens_opcode(&fisheye.opcodes.list3[0]));
        let mut disabled = DevelopSettings::default();
        assert!(lightcraft_pipeline::geometry::Frame::with_lens(60, 100, &disabled, false, Some(&lens)).warp.is_none());
        disabled.optics.lens_profile = true;
        assert!(lightcraft_pipeline::geometry::Frame::with_lens(60, 100, &disabled, false, Some(&lens)).warp.is_some());
    }

    #[test]
    fn lenscorr_panasonic_converged_inverse_is_subpixel_on_real_coefficients() {
        // CC0 raw.pixls.us ID 1136, Panasonic LX100. Independent forward
        // equation verifies the inverse, including the post-autoscale table.
        let coefficients = [6174.0, 1806.0, -1703.0].map(|v| v / 32768.0);
        let w = 4112.0_f64;
        let h = 3088.0_f64;
        let lens = radial(&VendorLens::Panasonic { coefficients, scale: 1.0 }, w, h).unwrap();
        let autoscale = lens.knots[15];
        let radius = w.hypot(h) / 2.0;
        let [a, b, c] = coefficients;
        let mut worst = 0.0_f64;
        for i in 0..=4096 {
            let ru = i as f64 / 4096.0;
            let rd = invert_panasonic(ru, coefficients, 1.0).unwrap();
            let q = rd * rd;
            assert!((rd * (1.0 + q * (a + q * (b + q * c))) - ru).abs() < 1e-10);
            let dest = ru * autoscale;
            let actual = dest * interpolate(&lens.knots, &lens.scales[1], lens.count, dest);
            worst = worst.max((actual - rd).abs() * radius);
        }
        println!("LX100 inverse + 16-knot maximum displacement residual: {worst:.6} px");
        assert!(worst < 0.2, "{worst} px");
    }

    #[test]
    fn lenscorr_sony_flat_frame_and_channel_positions_follow_metadata() {
        let data = VendorLens::Sony {
            // CC0 raw.pixls.us 1971, Sony A6000 E18-55.
            distortion: vec![9.0, 7.0, 14.0, 34.0, 63.0, 102.0, 155.0, 221.0, 304.0, 406.0, 528.0],
            ca_red: vec![-384.0, -128.0, -128.0, 0.0, 0.0, -128.0, -256.0, -384.0, -640.0, -1024.0, -1408.0],
            ca_blue: vec![640.0, 640.0, 512.0, 512.0, 640.0, 640.0, 768.0, 1024.0, 1408.0, 1792.0, 2304.0],
            vignetting: vec![0.0, 0.0, 192.0, 640.0, 1280.0, 1920.0, 2560.0, 3072.0, 3968.0, 5632.0, 8448.0],
        };
        let r = radial(&data, 6000.0, 4000.0).unwrap();
        let mut s = DevelopSettings::default();
        s.optics.lens_profile = true;
        let lens = lightcraft_develop::EmbeddedLens { radial: Some(r), ..Default::default() };
        let warp = Warp::from_settings(6000.0, 4000.0, &s, Some(&lens));
        let mut worst = 0.0_f64;
        for i in 0..=100 {
            let x = i as f64 * 60.0;
            let p = Point::new(x, 0.5);
            let rn = p.dist(Point::new(3000.0, 2000.0)) / 6000.0_f64.hypot(4000.0) * 2.0;
            let attenuation = interpolate(&r.vig_knots, &r.attenuation, r.vig_count, rn);
            worst = worst.max((f64::from(warp.gain(p)) * attenuation - 1.0).abs());
            for ch in 0..3 {
                let scale = interpolate(&r.knots, &r.scales[ch], r.count, rn);
                let expected = Point::new(3000.0 + (x - 3000.0) * scale, 2000.0 + (0.5 - 2000.0) * scale);
                assert!(warp.to_source(p, ch).dist(expected) < 1e-8);
            }
        }
        println!("Synthetic flat-frame residual: {:.8}% (CA coordinate residual <1e-8 px)", worst * 100.0);
        assert!(worst < 0.000001);
    }

    #[test]
    fn lenscorr_fuji_source_knots_and_invalid_profiles() {
        let data = VendorLens::Fuji {
            knots: vec![0.1, 0.5, 1.0],
            distortion: vec![0.0, -1.0, -3.0],
            ca_red: vec![0.0, 0.001, 0.002],
            ca_blue: vec![0.0, -0.001, -0.002],
            vignetting: vec![100.0, 85.0, 60.0],
            crop_factor: 1.25,
        };
        let r = radial(&data, 6000.0, 4000.0).unwrap();
        assert_eq!(r.vig_count, 4);
        assert_eq!(r.vig_knots[..4], [0.0, 0.125, 0.625, 1.25]);
        assert_eq!(r.attenuation[..4], [1.0, 1.0, 0.85, 0.6]);
        assert!(r.knots.windows(2).all(|p| p[1] > p[0]));
        assert!(r.scales[0][15] > r.scales[1][15]);
        let bad = VendorLens::Olympus { distortion: Some([f64::NAN, 0.0, 0.0, 1.0]), ca: None };
        assert!(radial(&bad, 6000.0, 4000.0).is_none());
    }
}
