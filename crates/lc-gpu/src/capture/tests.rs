//! Scalar f32 transcription of capture.wgsl, compared with the unchanged CPU
//! reference and independent upstream RL vectors. Device tests also inspect the
//! intermediate planes; the scalar tests always run without an adapter.

use super::*;
use lightcraft_raster::{Plane, Rgb32f};

fn max_delta(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(a, b)| {
            assert!(a.is_finite() && b.is_finite());
            (a - b).abs()
        })
        .fold(0.0, f32::max)
}

fn chart(w: usize, h: usize) -> Rgb32f {
    Rgb32f::from_fn(w, h, |x, y| {
        let v = 0.06 + 0.6 * (0.5 + 0.5 * (x as f32 * 0.47 + y as f32 * 0.11).sin());
        if (x, y) == (4, 3) {
            [0.0; 3]
        } else if (x, y) == (w - 5, h - 4) {
            [1.6, 0.8, 0.4]
        } else {
            [v * 0.8, v, v * 1.2]
        }
    })
}

fn indices_f32(w: usize, h: usize, p: &CaptureParams, hypot: bool) -> Vec<u32> {
    let (rw, rh) = (w as f32 / 2.0, h as f32 / 2.0);
    (0..w * h)
        .map(|i| {
            let (y, x) = (i / w, i % w);
            let (fr, fc) = (y as f32 - rh, x as f32 - rw);
            let distance = if hypot { fr.hypot(fc) } else { (fr * fr + fc * fc).sqrt() };
            let sc = distance / rw.min(rh).max(1.0);
            let radial = (sc - 0.5 - p.center).max(0.0);
            let corr = (1.0 + 8.0 * p.center * p.center) * p.corner_boost * (radial * radial);
            let border = (h - y - 1).min(w - x - 1).min(x).min(y).min(8);
            let sigma = (p.sigma + corr) * 0.125 * border as f32;
            ((sigma / 0.01) as i32).clamp(0, 255) as u32
        })
        .collect()
}

/// WGSL's gather mask and variance addition order, with the shared box blur's
/// whole-row/32-row restart locations (identical to raster::blur::gaussian).
fn mask_f32(img: &Rgb32f, p: &CaptureParams) -> (Vec<f32>, Vec<f32>) {
    let (w, h) = (img.width, img.height);
    let lum: Vec<_> = img.data.iter().map(|c| lightcraft_color::luminance_2020(*c).max(0.0)).collect();
    let modified = Plane::from_fn(w, h, |x, y| {
        if x <= 1 || y <= 1 || x + 2 >= w || y + 2 >= h {
            return 0.0;
        }
        for dy in -2i32..=2 {
            let dxs = if dy.abs() == 2 { 1 } else { 2 };
            for dx in -dxs..=dxs {
                let (sy, sx) = (y as i32 + dy, x as i32 + dx);
                if sy <= 1 || sx <= 1 || sy + 2 >= h as i32 || sx + 2 >= w as i32 {
                    continue;
                }
                let i = sy as usize * w + sx as usize;
                if lum[i] < 0.001 || p.clip.is_some_and(|clip| img.data[i].iter().any(|v| *v >= clip)) {
                    return 0.0;
                }
            }
        }
        let (mut sum, mut sq) = (0.0f32, 0.0f32);
        for yy in y - 1..y + 2 {
            for xx in x - 2..x + 3 {
                let v = lum[yy * w + xx];
                sum += v;
                sq += v * v;
            }
        }
        for xx in x - 1..x + 2 {
            let up = lum[(y - 2) * w + xx];
            sum += up;
            sq += up * up;
            let down = lum[(y + 2) * w + xx];
            sum += down;
            sq += down * down;
        }
        let sd = ((sq - sum * sum / 21.0).max(0.0) / 21.0).sqrt();
        let mean = (sum / 21.0).max(1.52587890625e-05);
        let t = (1.0 + sd / mean.sqrt()).ln();
        let threshold = 0.6 * p.threshold * p.threshold;
        let offset = -2.5 + 200.0 * threshold / 2.0;
        let weight = 1.0 / (1.0 + (offset - 200.0 * t).exp());
        (1.01011 * (weight - 0.01)).clamp(0.0, 1.0)
    });
    let blurred = lightcraft_raster::blur::gaussian(&modified, 2.0);
    let blend = modified
        .data
        .iter()
        .zip(blurred.data)
        .map(|(u, b)| {
            let wt = 1.0 / (1.0 + (5.0 - 10.0 * (u - b)).exp());
            (wt * u + (1.0 - wt) * b).clamp(0.0, 1.0)
        })
        .collect();
    (lum, blend)
}

fn convolve_f32(input: &[f32], w: usize, h: usize, i: usize, index: u32) -> f32 {
    let (y, x) = ((i / w) as i32, (i % w) as i32);
    let bd = if index < 66 { 2 } else { 4 };
    let mut val = 0.0f32;
    for dy in -bd..=bd {
        let yy = y + dy;
        if yy < 0 || yy >= h as i32 {
            continue;
        }
        for dx in -bd..=bd {
            let xx = x + dx;
            if xx < 0 || xx >= w as i32 {
                continue;
            }
            val += kernels()[index as usize][(5 * dy.abs() + dx.abs()) as usize] * input[yy as usize * w + xx as usize];
        }
    }
    val
}

fn step_f32(est: &[f32], lum: &[f32], blend: &[f32], indices: &[u32], w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
    let ratio: Vec<_> = (0..w * h).map(|i| if blend[i] <= 0.0 { 1.0 } else { lum[i] / convolve_f32(est, w, h, i, indices[i]).max(0.001) }).collect();
    let next = (0..w * h).map(|i| if blend[i] <= 0.0 { est[i] } else { est[i] * convolve_f32(&ratio, w, h, i, indices[i]) }).collect();
    (ratio, next)
}

fn apply_f32(src: &Rgb32f, lum: &[f32], blend: &[f32], est: &[f32]) -> Rgb32f {
    let mut out = src.clone();
    for (i, px) in out.data.iter_mut().enumerate() {
        if blend[i] > 0.0 {
            let new = blend[i].clamp(0.0, 1.0) * (est[i] - lum[i]) + lum[i];
            *px = px.map(|v| v * (new / lum[i].max(0.001)));
        }
    }
    out
}

#[test]
fn wgsl_f32_mask_and_iterations_match_cpu_reference() {
    let (mut mask_max, mut estimate_max, mut rgb_max) = (0.0f32, 0.0f32, 0.0f32);
    for (w, h) in [(9, 9), (17, 41), (73, 49)] {
        let src = chart(w, h);
        for (sigma, threshold, boost, center) in [(0.26, 0.0, 0.0, 0.0), (0.65, 0.4, 0.7, 0.0), (0.66, 1.0, 1.5, 0.6), (1.4, 0.2, 0.0, 0.0)] {
            let p = CaptureParams { sigma, threshold, corner_boost: boost, center, clip: Some(1.0), ..Default::default() };
            let (lum, blend) = mask_f32(&src, &p);
            let (cpu_blend, cpu_lum) = lightcraft_pipeline::capture::blend_mask(&src, &p);
            assert_eq!(lum, cpu_lum.data);
            mask_max = mask_max.max(max_delta(&blend, &cpu_blend.data));
            assert!(mask_max <= 1e-7);
            let indices = indices_f32(w, h, &p, false);
            let reference_indices = indices_f32(w, h, &p, true);
            assert!(indices.iter().zip(&reference_indices).all(|(a, b)| a.abs_diff(*b) <= 1));
            let mut est = lum.clone();
            for it in 1..=50 {
                est = step_f32(&est, &lum, &blend, &indices, w, h).1;
                if [1, 2, 8, 50].contains(&it) {
                    let mut cpu = src.clone();
                    lightcraft_pipeline::capture::sharpen(&mut cpu, &CaptureParams { iterations: it, ..p });
                    let mirrored = apply_f32(&src, &lum, &blend, &est);
                    let d = max_delta(crate::render::rgb_words(&cpu), crate::render::rgb_words(&mirrored));
                    rgb_max = rgb_max.max(d);
                    assert!(d <= 3e-5, "{w}x{h} sigma {sigma} it {it}: {d}");
                    // Recover the reference's intermediate estimate from its RGB
                    // gain and known blend. Strong-mask pixels avoid magnifying
                    // rounding error in the inverse; dark pixels use the Y floor.
                    for i in 0..w * h {
                        if blend[i] >= 0.25 && lum[i] >= 0.001 {
                            let new_l = lightcraft_color::luminance_2020(cpu.data[i]);
                            let reference_est = (new_l - lum[i]) / blend[i] + lum[i];
                            let d = (est[i] - reference_est).abs();
                            estimate_max = estimate_max.max(d);
                            assert!(d <= 3e-5, "estimate {w}x{h} sigma {sigma} it {it} pixel {i}: {d}");
                        }
                    }
                }
            }
        }
    }
    eprintln!("WGSL f32 vs CPU: mask max {mask_max:.9}, intermediate estimate max {estimate_max:.9}, RGB max {rgb_max:.9}");
}

#[test]
fn wgsl_f32_rl_matches_independent_upstream_estimates() {
    let (w, h) = (64, 64);
    let input: Vec<_> = (0..w * h)
        .map(|i| {
            let noise = ((i as u32).wrapping_mul(1664525).wrapping_add(1013904223) >> 8 & 65535) as f32 / 65536.0;
            0.1 + 0.7 * noise
        })
        .collect();
    let blend: Vec<_> = (0..w * h).map(|i| if i % 29 != 0 { 1.0 } else { 0.0 }).collect();
    let indices: Vec<_> = (0..w * h).map(|i| [30, 70, 150][i % 3]).collect();
    let mut est = input.clone();
    for _ in 0..8 {
        est = step_f32(&est, &input, &blend, &indices, w, h).1;
    }
    let bytes = include_bytes!("../../../lc-pipeline/tests/fixtures/capture-rl/estimate.f32");
    let reference: Vec<_> = bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
    let d = max_delta(&est, &reference);
    eprintln!("WGSL f32 RL vs upstream: {} samples, max {d:.9}", est.len());
    assert!(d <= 3e-6);
    // Every sigma has unit mass; the small kernel is the true sampled Gaussian.
    for k in kernels() {
        let sum: f32 = (-4i32..=4).flat_map(|dy| (-4i32..=4).map(move |dx| k[(5 * dy.abs() + dx.abs()) as usize])).sum();
        assert!((sum - 1.0).abs() < 2e-6);
    }
    assert_eq!((0.66f32 / 0.01) as u32, 66);
    assert!(kernels()[30][0] > 0.98);
}

#[test]
fn gpu_capture_intermediate_planes_match_cpu() {
    let Some(gpu) = crate::test_device() else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let (w, h) = (73, 49); // partial workgroups, both kernel sizes, border and corner taper
    let src = chart(w, h);
    for clip in [None, Some(1.0)] {
        let p = CaptureParams { sigma: 0.7, threshold: 0.25, corner_boost: 0.6, clip, ..Default::default() };
        let mut cx = Cx::new(gpu);
        let uploaded = gpu.upload(crate::render::rgb_words(&src));
        let prep = prepare(&mut cx, &uploaded, w, h, &p);
        let (cpu_blend, cpu_lum) = lightcraft_pipeline::capture::blend_mask(&src, &p);
        assert!(max_delta(&cx.read::<f32>(&prep.lum, w * h), &cpu_lum.data) <= 3e-7);
        assert!(max_delta(&cx.read::<f32>(&prep.blend, w * h), &cpu_blend.data) <= 5e-5);
        let idx = cx.read::<u32>(&prep.indices, w * h);
        let cpu_idx = indices_f32(w, h, &p, true);
        assert!(idx.iter().zip(&cpu_idx).all(|(a, b)| a.abs_diff(*b) <= 1));
        let table = gpu.upload(kernels());
        let mut est = cx.copy(&prep.lum);
        let mut next = gpu.buffer(w * h);
        let ratio = gpu.buffer(w * h);
        let mut cpu_est = cpu_lum.data.clone();
        for it in 1..=8 {
            // Use the CPU mask and actual device indices to isolate iterative f32
            // arithmetic from hypot/index quantization. Final RGB parity uses the
            // entire CPU reference (including its own indices) in toolset.rs.
            let (cpu_ratio, e) = step_f32(&cpu_est, &cpu_lum.data, &cpu_blend.data, &idx, w, h);
            cpu_est = e;
            iteration(&mut cx, &est, &next, &ratio, &prep, &table, w, h);
            std::mem::swap(&mut est, &mut next);
            let dr = max_delta(&cx.read::<f32>(&ratio, w * h), &cpu_ratio);
            let de = max_delta(&cx.read::<f32>(&est, w * h), &cpu_est);
            eprintln!("capture intermediate clip {clip:?} iteration {it}: ratio {dr:.8}, estimate {de:.8}");
            assert!(dr <= 2e-4 && de <= 2e-4);
        }
    }
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}
