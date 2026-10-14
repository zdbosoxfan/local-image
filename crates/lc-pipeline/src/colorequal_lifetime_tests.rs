//! Original application formulas retained as independent lifetime-change oracles.
//! Production edits only release dead owners; filter math and call order stay fixed.
use super::*;

fn legacy_guided_uv(uv: &[f32], input: &[f32], w: usize, h: usize, sigma: f32, eps: f32, brightness: bool) -> Vec<f32> {
    let scaling = (sigma - 1.5).floor().clamp(1., 4.);
    let gs = (sigma / scaling).max(0.2);
    let (dw, dh) = (((w as f32 / scaling) as usize).max(1), ((h as f32 / scaling) as usize).max(1));
    let guide = eigf::interpolate(uv, w, h, dw, dh, 2);
    let target = eigf::interpolate(input, w, h, dw, dh, 2);
    let n = dw * dh;
    let mut cov = vec![0.; n * 4];
    let mut corr = cov.clone();
    for i in 0..n {
        let (u, v) = (guide[2 * i], guide[2 * i + 1]);
        cov[i * 4..i * 4 + 4].copy_from_slice(&[u * u, u * v, u * v, v * v]);
        corr[i * 4..i * 4 + 4].copy_from_slice(&[u * target[2 * i], v * target[2 * i], u * target[2 * i + 1], v * target[2 * i + 1]]);
    }
    let mean = blur(&guide, dw, dh, 2, gs);
    let mut mt = blur(&target, dw, dh, 2, gs);
    if brightness {
        let single: Vec<_> = target.as_chunks::<2>().0.iter().map(|c| c[1]).collect();
        let b = blur(&single, dw, dh, 1, 0.1 * gs);
        for i in 0..n {
            mt[2 * i + 1] = b[i];
        }
    }
    let cov = blur(&cov, dw, dh, 4, gs);
    let corr = blur(&corr, dw, dh, 4, gs);
    let mut a = vec![0.; n * 4];
    let mut b = vec![0.; n * 2];
    for i in 0..n {
        let (u, v) = (mean[2 * i], mean[2 * i + 1]);
        let s = [cov[4 * i] - u * u + eps, cov[4 * i + 1] - u * v, cov[4 * i + 2] - u * v, cov[4 * i + 3] - v * v + eps];
        let det = s[0] * s[3] - s[1] * s[2];
        if det.abs() > 4. * f32::EPSILON {
            for c in 0..2 {
                let cu = corr[4 * i + 2 * c] - u * mt[2 * i + c];
                let cv = corr[4 * i + 2 * c + 1] - v * mt[2 * i + c];
                a[4 * i + 2 * c] = (cu * s[3] - cv * s[1]) / det;
                a[4 * i + 2 * c + 1] = (-cu * s[2] + cv * s[0]) / det;
            }
        }
        for c in 0..2 {
            b[2 * i + c] = mt[2 * i + c] - a[4 * i + 2 * c] * u - a[4 * i + 2 * c + 1] * v;
        }
    }
    let a = eigf::interpolate(&blur(&a, dw, dh, 4, gs), dw, dh, w, h, 4);
    let b = eigf::interpolate(&blur(&b, dw, dh, 2, gs), dw, dh, w, h, 2);
    (0..w * h * 2)
        .map(|k| {
            let i = k / 2;
            let c = k % 2;
            a[4 * i + 2 * c] * uv[2 * i] + a[4 * i + 2 * c + 1] * uv[2 * i + 1] + b[k]
        })
        .collect()
}

fn legacy_apply(img: &mut Rgb32f, s: &DevelopSettings, ppl: f64) {
    if s.mixer.is_neutral() && !crate::is_bw(s) {
        return;
    }
    let (w, h, n) = (img.width, img.height, img.len());
    if n == 0 {
        return;
    }
    let lw = ucs::y_to_l_star(1.);
    let scale = (ppl as f32 / 6000.).max(1e-3);
    let mut uv = vec![0.; 2 * n];
    let mut l = vec![0.; n];
    let mut saturation = vec![0.; n];
    crate::for_rows(&mut uv, w * 2, |y, row| {
        for x in 0..w {
            let c = img.get(x, y);
            let xy = ucs::xyz_to_xyy(ucs::mul(&ucs::mats().rgb_to_xyz, c));
            row[2 * x..2 * x + 2].copy_from_slice(&ucs::xy_to_uv(xy[0], xy[1]));
        }
    });
    crate::for_rows(&mut l, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            *v = ucs::y_to_l_star(ucs::mul(&ucs::mats().rgb_to_xyz, img.get(x, y))[1]);
        }
    });
    crate::for_rows(&mut saturation, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            let c = img.get(x, y);
            let min = c[0].min(c[1]).min(c[2]);
            let max = c[0].max(c[1]).max(c[2]);
            let d = max - min;
            *v = if max > 1.525_878_9e-5 && d > 1.525_878_9e-5 { d / max } else { 0. };
        }
    });
    let saturation = blur(&saturation, w, h, 1, scale.max(0.5));
    let pre = legacy_guided_uv(&uv, &uv, w, h, (0.75 * scale).max(0.2), 1e-5, false);
    for i in 0..n {
        let wt = satweight(saturation[i] - 0.1);
        for c in 0..2 {
            uv[2 * i + c] += wt * (pre[2 * i + c] - uv[2 * i + c]);
        }
    }
    let bands = s.mixer.bands();
    let hue = rbf(bands.map(|b| b.hue as f32 / 100. * 0.5), *hues(), PI, false);
    let sat = rbf(bands.map(|b| 1. + b.sat as f32 / 200.), *hues(), PI, true);
    let bright = rbf(bands.map(|b| 1. + b.lum as f32 / 200.), *hues(), PI, true);
    let bw = rbf(s.bw_mix.bands().map(|b| b as f32 / 100.), *hues(), PI, false);
    let max_b = bands.iter().map(|b| (b.lum as f32 / 200.).abs()).fold(0., f32::max);
    let mut hsb = vec![[0.; 4]; n];
    crate::for_rows(&mut hsb, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let j = ucs::luv_to_jch(l[i], lw, [uv[2 * i], uv[2 * i + 1]]);
            let c = ucs::jch_to_hsb(j);
            *v = [c[0], c[1], c[2], j[1]];
        }
    });
    let mut corrections = vec![0.; 2 * n];
    let mut dh = vec![0.; n];
    let mut grad = vec![0.; n];
    for i in 0..n {
        let colour = hsb[i];
        if colour[3] > 1.525_878_9e-5 {
            dh[i] = ucs::lookup_gamut(&hue, colour[0]);
            corrections[2 * i] = ucs::lookup_gamut(&sat, colour[0]);
            corrections[2 * i + 1] = colour[1] * (ucs::lookup_gamut(&bright, colour[0]) - 1.);
        } else {
            corrections[2 * i] = 1.;
        }
        grad[i] = 4. * max_b.sqrt() * scale * scale * (gradient(&saturation, w, h, i % w, i / w) - 0.02).max(0.).powi(2);
    }
    let grad = blur(&grad, w, h, 1, scale.max(0.5));
    let corr = legacy_guided_uv(&uv, &corrections, w, h, (0.5 * scale).max(0.2), 1e-6, true);
    crate::for_rows(&mut img.data, w, |y, row| {
        for (x, pixel) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let c = hsb[i];
            let mut h = [c[0], c[1], c[2]];
            h[0] += dh[i];
            h[1] = (h[1] * (1. + 2. * (corr[2 * i] - 1.) * satweight(saturation[i] - 0.1))).max(0.);
            h[2] = (h[2] * (1. + 8. * corr[2 * i + 1] * (1. - grad[i].clamp(0., 1.)) * satweight(saturation[i] - 0.1 - 0.01 * max_b))).max(0.);
            if crate::is_bw(s) {
                let y = ucs::l_star_to_y((h[2] * lw).clamp(0., ucs::L_STAR_UPPER));
                let gain = (ucs::lookup_gamut(&bw, h[0]) * 1.3).exp2();
                *pixel = [y * gain; 3];
            } else {
                ucs::gamut_map_hsb(&mut h, lw);
                *pixel = ucs::hsb_to_rgb(h, lw);
            }
        }
    });
}

fn source(width: usize, height: usize, kind: usize) -> Rgb32f {
    Rgb32f::from_fn(width, height, |x, y| {
        let v = 0.001 + (x as f32 / (width.max(1) as f32)).powi(2) * 1.8;
        match kind {
            0 => [v * (0.7 + (y % 7) as f32 * 0.05), v * (0.5 + (y % 5) as f32 * 0.15), v * (0.6 + (x % 9) as f32 * 0.06)],
            1 => [v; 3],
            2 => [-0.0, 0.0, f32::from_bits(1)],
            _ => [0.08 + (x % 7) as f32 * 0.015, 0.05 + (y % 5) as f32 * 0.012, 0.04],
        }
    })
}
fn settings(bw: bool) -> DevelopSettings {
    let mut d = DevelopSettings::default();
    d.mixer.orange.hue = 17.0;
    d.mixer.orange.sat = -22.0;
    d.mixer.orange.lum = -5.0;
    d.mixer.green.sat = 13.0;
    d.mixer.green.lum = 4.0;
    d.mixer.blue.hue = -9.0;
    d.mixer.blue.sat = 8.0;
    if bw {
        d.treatment = lightcraft_develop::Treatment::Bw;
        d.bw_mix.orange = 12.0;
        d.bw_mix.blue = -8.0;
    }
    d
}
#[test]
fn guided_uv_lifetimes_preserve_original_bits_at_every_scale_and_brightness_mode() {
    for (w, h) in [(1, 1), (1, 17), (19, 1), (13, 7), (37, 29)] {
        let uv = (0..w * h).flat_map(|i| [0.03 * (i as f32 * 0.13).sin(), 0.04 * (i as f32 * 0.17).cos()]).collect::<Vec<_>>();
        let input = (0..w * h).flat_map(|i| [1.0 + (i % 9) as f32 * 0.01, 0.02 * ((i / w) as f32 * 0.1).sin()]).collect::<Vec<_>>();
        for sigma in [0.2, 1.08, 2.7, 3.7, 4.7, 5.7, 8.5] {
            for brightness in [false, true] {
                let (new, old) = lightcraft_raster::sequential(|| {
                    (guided_uv(&uv, &input, w, h, sigma, 1e-6, brightness), legacy_guided_uv(&uv, &input, w, h, sigma, 1e-6, brightness))
                });
                assert_eq!(
                    new.iter().copied().map(f32::to_bits).collect::<Vec<_>>(),
                    old.iter().copied().map(f32::to_bits).collect::<Vec<_>>(),
                    "{w}x{h} sigma={sigma} brightness={brightness}"
                );
            }
        }
    }
}
#[test]
fn palette_lifetimes_preserve_original_float_bits_for_hdr_neutral_bw_and_native_radii() {
    for (w, h) in [(0, 0), (1, 1), (1, 17), (19, 1), (13, 7), (37, 29)] {
        for kind in 0..4 {
            for bw in [false, true] {
                for ppl in [400.0, 6000.0, 8640.0, 12288.0, 48000.0] {
                    let d = settings(bw);
                    let mut new = source(w, h, kind);
                    let mut old = new.clone();
                    lightcraft_raster::sequential(|| {
                        apply(&mut new, &d, ppl);
                        legacy_apply(&mut old, &d, ppl);
                    });
                    assert_eq!(
                        new.data.iter().flatten().copied().map(f32::to_bits).collect::<Vec<_>>(),
                        old.data.iter().flatten().copied().map(f32::to_bits).collect::<Vec<_>>(),
                        "{w}x{h} kind={kind} bw={bw} ppl={ppl}"
                    );
                }
            }
        }
    }
    let mut new = source(37, 29, 0);
    let old = new.clone();
    apply(&mut new, &DevelopSettings::default(), 8640.0);
    assert_eq!(new, old, "neutral colour equalizer must retain its existing no-op");
}

#[test]
#[ignore = "child of bounded palette memory diagnostic; invoke parent diagnostic"]
fn palette_lifetime_memory_child() {
    let mode = std::env::var("LC_PALETTE_LIFETIME_CHILD").expect("invoke parent memory diagnostic");
    assert!(mode == "legacy" || mode == "scoped");
    let mut image = source(1600, 1200, 0);
    let d = settings(false);
    let start = std::time::Instant::now();
    lightcraft_raster::sequential(|| if mode == "legacy" { legacy_apply(&mut image, &d, 8640.0) } else { apply(&mut image, &d, 8640.0) });
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for value in image.data.iter().flatten() {
        assert!(value.is_finite());
        hash ^= u64::from(value.to_bits());
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    let status = std::fs::read_to_string("/proc/self/status").expect("Linux VmHWM required for observed memory diagnostic");
    let peak = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:").and_then(|tail| tail.split_whitespace().next()).and_then(|value| value.parse::<u64>().ok()))
        .expect("VmHWM unavailable")
        * 1024;
    println!(
        "PALETTE_LIFETIME_MEMORY {}",
        serde_json::json!({"mode":mode,"pixels":image.data.len(),"peakRssBytes":peak,"outputBitFingerprint":hash,"elapsedMs":start.elapsed().as_secs_f64()*1000.0,"timing":"shared-host ML/native contention uncontrolled; no quiet timing claim"})
    );
}
#[test]
#[ignore = "bounded1.92MP isolated-process old/new RSS diagnostic; run only in coordinated compute slot"]
fn palette_lifetime_bounded_memory_diagnostic() {
    let run = |mode: &str| {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "colorequal::lifetime_tests::palette_lifetime_memory_child", "--ignored", "--nocapture", "--test-threads=1"])
            .env("LC_PALETTE_LIFETIME_CHILD", mode)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode} child failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        stdout
            .lines()
            .find_map(|line| line.split_once("PALETTE_LIFETIME_MEMORY ").map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap()))
            .expect("child did not report observed VmHWM")
    };
    let old = run("legacy");
    let new = run("scoped");
    assert_eq!(old["outputBitFingerprint"], new["outputBitFingerprint"]);
    assert_eq!(old["pixels"], new["pixels"]);
    let old_peak = old["peakRssBytes"].as_u64().unwrap();
    let new_peak = new["peakRssBytes"].as_u64().unwrap();
    println!(
        "PALETTE_LIFETIME_OBSERVED {}",
        serde_json::json!({"original":old,"scoped":new,"reductionPercent":100.0*(1.0-new_peak as f64/old_peak as f64),"method":"isolated process VmHWM; bounded1.92MP fixture; allocation/RSS diagnostic, not universal cap or native-export measurement"})
    );
    assert!(new_peak * 100 < old_peak * 85, "require observed >=15% memory reduction, old={old_peak} new={new_peak}");
}
