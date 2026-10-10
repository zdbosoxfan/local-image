//! Reproducible paired evaluation, PSNR and Wang et al. single-scale SSIM.
//! Reference images and masks remain in the supplied evaluation folder, never owner photos.
use li_seg::denoise::Variant;
use lightcraft_engine::enhance::denoise::{camera_rgb_to_working, infer, pack_bayer, prepare_raw, unpack_camera};
use lightcraft_raster::Rgb32f;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let model_dir = PathBuf::from(args.get(1).ok_or("model directory")?);
    let data = PathBuf::from(args.get(2).ok_or("evaluation directory")?);
    let gpu = args.get(3).is_none_or(|s| s == "gpu");
    let scene = args.get(4).map(String::as_str).unwrap_or("test-canon");
    let ext = if scene == "fuji" {
        "RAF"
    } else if scene == "test-sony" {
        "ARW"
    } else {
        "CR2"
    };
    let clean_bytes = std::fs::read(data.join(format!("{scene}-clean.{ext}"))).map_err(|e| e.to_string())?;
    let noisy_bytes = std::fs::read(data.join(format!("{scene}-noisy.{ext}"))).map_err(|e| e.to_string())?;
    let (mut clean, _) = lightcraft_engine::files::load_bytes(&clean_bytes, usize::MAX)?;
    let (noisy, info) = lightcraft_engine::files::load_bytes(&noisy_bytes, usize::MAX)?;
    if (clean.width, clean.height) != (noisy.width, noisy.height) {
        return Err("pair dimensions differ".into());
    }
    // A single exposure normalization, determined from the input pair, shared by every candidate.
    let mean = |img: &Rgb32f| img.data.iter().map(|p| p[1].clamp(0., 1.) as f64).sum::<f64>() / img.data.len() as f64;
    let gain = (mean(&noisy) / mean(&clean)) as f32;
    clean.map_in_place(|p| p.map(|v| v * gain));
    let offset = align(&noisy, &clean);
    println!("pair {scene} {}x{} reference gain {gain:.6} translation {offset:?}", noisy.width, noisy.height);
    let raw = prepare_raw(&noisy_bytes)?;
    let norm = raw.normalized().map_err(|e| e.to_string())?;
    let bayer = norm.cfa.as_ref().is_some_and(|c| c.is_bayer());
    // Native-resolution crops with context; 64 px are excluded from all metrics.
    for (index, (fx, fy)) in [(0.3, 0.35), (0.65, 0.65)].into_iter().enumerate() {
        let side = 768usize;
        let x = ((noisy.width - side) as f64 * fx).round() as usize / 2 * 2;
        let y = ((noisy.height - side) as f64 * fy).round() as usize / 2 * 2;
        let input = crop(&noisy, x, y, side);
        let reference = crop(&clean, (x as i32 + offset[0]) as usize, (y as i32 + offset[1]) as usize, side);
        evaluate(&data, scene, index, "input", &input, &reference, 0.)?;
        for kind in [Variant::Linear, Variant::Rgb] {
            let pixels = if kind == Variant::Rgb { display(&input) } else { input.data.clone() };
            let start = Instant::now();
            let mut out = infer(&model_dir, kind, pixels.as_flattened(), side, side, gpu, &|_, _| true)?;
            let seconds = start.elapsed().as_secs_f64();
            if kind == Variant::Rgb {
                from_display(&mut out);
            }
            evaluate(
                &data,
                scene,
                index,
                if kind == Variant::Rgb { "nind-rgb" } else { "rawnind-linear" },
                &Rgb32f { width: side, height: side, data: out },
                &reference,
                seconds,
            )?;
        }
        if bayer && raw.orientation == lightcraft_geom::Orientation::Normal {
            let rx = x + raw.crop.x;
            let ry = y + raw.crop.y;
            let cfa = norm.cfa.as_ref().unwrap().shifted(rx, ry);
            let n = lightcraft_raw::Normalized {
                width: side,
                height: side,
                cpp: 1,
                cfa: Some(cfa),
                data: (0..side).flat_map(|yy| norm.data[(ry + yy) * norm.width + rx..(ry + yy) * norm.width + rx + side].iter().copied()).collect(),
            };
            let (packed, w, h, phase) = pack_bayer(&n)?;
            let start = Instant::now();
            let out = infer(&model_dir, Variant::Bayer, &packed, w, h, gpu, &|_, _| true)?;
            let seconds = start.elapsed().as_secs_f64();
            let mut img = unpack_camera(out, w * 2, h * 2, side, side, phase);
            camera_rgb_to_working(&raw, &noisy_bytes, &mut img);
            evaluate(&data, scene, index, "rawnind-bayer", &img, &reference, seconds)?;
        }
        let mut settings = lightcraft_develop::DevelopSettings::default();
        settings.detail.nr_luminance = 50.;
        settings.detail.nr_color = 25.;
        let mut wavelet = input.clone();
        let params = lightcraft_pipeline::detail::nr::nr_params(&settings, 1.).unwrap();
        let start = Instant::now();
        lightcraft_pipeline::detail::nr::denoise(&mut wavelet, &params);
        evaluate(&data, scene, index, "wavelet-cpu", &wavelet, &reference, start.elapsed().as_secs_f64())?;
        let start = Instant::now();
        let result = lightcraft_engine::media::develop(
            &std::sync::Arc::new(input.clone()),
            &info,
            &settings,
            &lightcraft_pipeline::RenderRequest::fit(side, side),
            None,
            gpu,
        );
        println!(
            "wavelet complete Develop path {scene} crop {index} {:.6}s {}x{}",
            start.elapsed().as_secs_f64(),
            result.image.width,
            result.image.height
        );
    }
    println!("backend {} adapter {:?}", if gpu { "gpu" } else { "cpu" }, lightcraft_gpu::adapter_name());
    Ok(())
}
fn crop(img: &Rgb32f, x: usize, y: usize, n: usize) -> Rgb32f {
    Rgb32f {
        width: n,
        height: n,
        data: (0..n).flat_map(|yy| img.data[(yy + y) * img.width + x..(yy + y) * img.width + x + n].iter().copied()).collect(),
    }
}
fn display(img: &Rgb32f) -> Vec<[f32; 3]> {
    let m = lightcraft_color::SRGB.from_xyz().mul(&lightcraft_color::REC2020.to_xyz());
    img.data.iter().map(|p| m.apply_f32(*p).map(|v| lightcraft_color::transfer::linear_to_srgb(v).clamp(0., 1.))).collect()
}
fn from_display(data: &mut [[f32; 3]]) {
    let m = lightcraft_color::REC2020.from_xyz().mul(&lightcraft_color::SRGB.to_xyz());
    for p in data {
        *p = m.apply_f32(p.map(lightcraft_color::transfer::srgb_to_linear));
    }
}
fn align(input: &Rgb32f, reference: &Rgb32f) -> [i32; 2] {
    let score = |dx: i32, dy: i32| {
        let mut e = 0f64;
        for y in (64..input.height - 64).step_by(32) {
            for x in (64..input.width - 64).step_by(32) {
                let a = input.data[y * input.width + x][1].clamp(0., 1.);
                let b = reference.data[(y as i32 + dy) as usize * reference.width + (x as i32 + dx) as usize][1].clamp(0., 1.);
                e += (a - b).powi(2) as f64;
            }
        }
        e
    };
    let mut best = [0, 0];
    let mut e = f64::INFINITY;
    for dy in -24..=24 {
        for dx in -24..=24 {
            let d = score(dx, dy);
            if d < e {
                e = d;
                best = [dx, dy];
            }
        }
    }
    best
}
fn evaluate(dir: &Path, scene: &str, index: usize, name: &str, img: &Rgb32f, reference: &Rgb32f, seconds: f64) -> Result<(), String> {
    let a = display(img);
    let b = display(reference);
    let n = img.width;
    let margin = 64;
    let mut mse = 0.;
    let mut count = 0;
    for y in margin..n - margin {
        for x in margin..n - margin {
            for c in 0..3 {
                mse += (a[y * n + x][c] as f64 - b[y * n + x][c] as f64).powi(2);
                count += 1;
            }
        }
    }
    let psnr = -10. * (mse / count as f64).log10();
    let ssim = ssim(&a, &b, n, margin);
    println!("{scene},{index},{name},PSNR={psnr:.6},SSIM={ssim:.8},seconds={seconds:.6}");
    let samples: Vec<u8> = a.iter().flatten().map(|v| (v * 255. + 0.5) as u8).collect();
    let bytes = lightcraft_codecs::encode_png(
        &lightcraft_codecs::EncodeImage::new(n as u32, n as u32, 3, lightcraft_codecs::Samples::U8(&samples)),
        &Default::default(),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{scene}-{index}-{name}.png")), bytes).map_err(|e| e.to_string())?;
    if name == "input" {
        let samples: Vec<u8> = b.iter().flatten().map(|v| (v * 255. + 0.5) as u8).collect();
        let bytes = lightcraft_codecs::encode_png(
            &lightcraft_codecs::EncodeImage::new(n as u32, n as u32, 3, lightcraft_codecs::Samples::U8(&samples)),
            &Default::default(),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(dir.join(format!("{scene}-{index}-reference.png")), bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}
// Wang et al. (2004), 11x11 Gaussian sigma=1.5, K1=.01, K2=.03, range=1.
// Average the channel SSIMs at native resolution, valid windows; do not downsample.
fn ssim(a: &[[f32; 3]], b: &[[f32; 3]], n: usize, margin: usize) -> f64 {
    let mut kernel = [0.; 11];
    for (i, v) in kernel.iter_mut().enumerate() {
        *v = (-((i as f64 - 5.).powi(2)) / (2. * 1.5f64.powi(2))).exp();
    }
    let sum: f64 = kernel.iter().sum();
    kernel.iter_mut().for_each(|v| *v /= sum);
    let mut total = 0.;
    let mut count = 0;
    for y in margin + 5..n - margin - 5 {
        for x in margin + 5..n - margin - 5 {
            for c in 0..3 {
                let (mut u, mut v, mut aa, mut bb, mut ab) = (0., 0., 0., 0., 0.);
                for (dy, wy) in kernel.iter().enumerate() {
                    for (dx, wx) in kernel.iter().enumerate() {
                        let i = (y + dy - 5) * n + x + dx - 5;
                        let (a, b) = (a[i][c] as f64, b[i][c] as f64);
                        let weight = wy * wx;
                        u += weight * a;
                        v += weight * b;
                        aa += weight * a * a;
                        bb += weight * b * b;
                        ab += weight * a * b;
                    }
                }
                total += ((2. * u * v + 0.0001) * (2. * (ab - u * v) + 0.0009)) / ((u * u + v * v + 0.0001) * ((aa - u * u) + (bb - v * v) + 0.0009));
                count += 1;
            }
        }
    }
    total / count as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ai_denoise_ssim_matches_identity_and_constant_formula() {
        let a = vec![[0.2; 3]; 32 * 32];
        assert!((ssim(&a, &a, 32, 0) - 1.).abs() < 1e-12);
        let b = vec![[0.3; 3]; 32 * 32];
        let expected = (2. * 0.2 * 0.3 + 0.0001) / (0.2f64.powi(2) + 0.3f64.powi(2) + 0.0001);
        assert!((ssim(&a, &b, 32, 0) - expected).abs() < 1e-7);
    }
}
