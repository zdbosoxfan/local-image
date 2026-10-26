//! Opt-in measurements through the service used by the interactive app.
//! LOCAL_IMAGE_TEST_MODELS names the isolated cache, never the user's model folder.
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

use image::{Rgba, RgbaImage, imageops};
use li_ai::{
    Ai, JobControl, ModelId,
    enhance::{Backend, Method},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

mod enhance_eval;

fn pattern(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let v = ((x as f32 * 0.061).sin() * (y as f32 * 0.079).cos() * 34.0 + 128.0) as u8;
        Rgba([v, ((x * 137 / w + y * 53 / h) % 230) as u8, ((x / 21 + y / 17) % 2 * 65 + 80) as u8, 255])
    })
}
fn psnr(a: &RgbaImage, b: &RgbaImage) -> f64 {
    let mse = a.pixels().zip(b.pixels()).flat_map(|(a, b)| (0..3).map(move |c| (a[c] as f64 - b[c] as f64).powi(2))).sum::<f64>()
        / (a.width() as f64 * a.height() as f64 * 3.0);
    10.0 * (255.0f64.powi(2) / mse.max(1e-12)).log10()
}
fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(std::env::var("LOCAL_IMAGE_TEST_MODELS")?);
    let action = std::env::args().nth(1).unwrap_or_else(|| "download".into());
    let ctl = JobControl::new();
    if action == "download" {
        for method in [Method::Faithful, Method::Rescue] {
            for scale in [2, 4] {
                li_ai::enhance::download_cpu(method, scale, &root, &ctl, &|_| {})?;
                let preset = li_ai::catalog::preset(ModelId::intern(method.model(scale).unwrap().id), "default").unwrap();
                li_ai::download::download_preset(&root, preset, &ctl, &|_| {})?;
                println!("verified {} {scale}x CPU + GPU", method.key());
            }
        }
        return Ok(());
    }
    let host = std::env::var("LOCAL_IMAGE_COMFY").unwrap_or_else(|_| "127.0.0.1:8188".into());
    let ai = Ai::new(host);
    let backend = if action.starts_with("cpu") { Backend::Cpu } else { Backend::Gpu };
    if backend == Backend::Gpu {
        let queue = ai.client.queue_state()?;
        anyhow::ensure!(["queue_running", "queue_pending"].iter().all(|k| queue[k].as_array().is_some_and(Vec::is_empty)), "ComfyUI is busy");
    }
    if action.ends_with("eval") {
        return enhance_eval::run(&ai, &root, backend);
    }
    if action.ends_with("24mp") {
        let input = pattern(6000, 4000);
        let method = if std::env::var("ENHANCE_METHOD").as_deref() == Ok("rescue") { Method::Rescue } else { Method::Faithful };
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (watch_done, watch_ctl) = (done.clone(), ctl.clone());
        let watcher = std::thread::spawn(move || {
            let start = Instant::now();
            while !watch_done.load(std::sync::atomic::Ordering::Relaxed) {
                if backend == Backend::Cpu && start.elapsed() > Duration::from_secs(20) {
                    watch_ctl.cancel();
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let start = Instant::now();
        let result = ai.enhance(&input, 12000, 8000, method, backend, &root, 1, &ctl);
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        watcher.join().unwrap();
        println!(
            "{} {backend:?} 24 MP 2x: {:.3}s, {:?}, progress={:?}",
            method.key(),
            start.elapsed().as_secs_f64(),
            result.as_ref().map(|r| r.dimensions()).map_err(|e| format!("{e:#}")),
            ctl.progress()
        );
        return Ok(());
    }
    let truth = pattern(384, 256);
    let input = imageops::resize(&truth, 192, 128, imageops::FilterType::Lanczos3);
    let baseline = imageops::resize(&input, 384, 256, imageops::FilterType::Lanczos3);
    println!("synthetic clean 2x Lanczos PSNR {:.3} dB", psnr(&truth, &baseline));
    for method in [Method::Faithful, Method::Rescue] {
        for scale in [2, 4] {
            let start = Instant::now();
            let choice = li_ai::enhance::selection::Choice::Curated { id: method.model(scale).unwrap().id.into() };
            let output = ai.enhance_with_choice(&input, 192 * scale, 128 * scale, method, backend, &root, 1, &JobControl::new(), &choice)?;
            let seconds = start.elapsed().as_secs_f64();
            let quality = psnr(&truth, &imageops::resize(&output, 384, 256, imageops::FilterType::Lanczos3));
            println!("{} {scale}x {backend:?} {:.3}s, {}x{}, PSNR {:.3} dB", method.key(), seconds, output.width(), output.height(), quality);
            if let Ok(dir) = std::env::var("ENHANCE_OUTPUT") {
                std::fs::create_dir_all(&dir)?;
                output.save(PathBuf::from(dir).join(format!("{}-{scale}x.png", method.key())))?;
            }
        }
    }
    if backend == Backend::Gpu {
        let mut damaged = input.clone();
        for (i, p) in damaged.pixels_mut().enumerate() {
            let noise = (i.wrapping_mul(1664525).wrapping_add(1013904223) % 29) as i16 - 14;
            for c in 0..3 {
                p[c] = (p[c] as i16 + noise).clamp(0, 255) as u8;
            }
        }
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 50).encode_image(&image::DynamicImage::ImageRgba8(damaged).to_rgb8())?;
        let damaged = image::load_from_memory(&jpeg)?.to_rgba8();
        let baseline = imageops::resize(&damaged, 384, 256, imageops::FilterType::Lanczos3);
        println!("synthetic noise + JPEG50 Lanczos PSNR {:.3} dB", psnr(&truth, &baseline));
        for method in [Method::Faithful, Method::Rescue] {
            let start = Instant::now();
            let output = ai.enhance(&damaged, 384, 256, method, backend, &root, 1, &JobControl::new())?;
            println!("synthetic noise + JPEG50 {} {:.3}s, PSNR {:.3} dB", method.key(), start.elapsed().as_secs_f64(), psnr(&truth, &output));
        }
    }
    Ok(())
}
