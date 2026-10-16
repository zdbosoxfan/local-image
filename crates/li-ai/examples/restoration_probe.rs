//! Owner-approved pin downloads and real ComfyUI app-path quality/timing probe.
//! cargo run --offline -p li-ai --example restoration_probe -- download|run
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use image::{Rgba, RgbaImage};
use li_ai::{
    ComfyClient, JobControl,
    restoration::{self, Task},
};

fn main() -> Result<()> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "run".into());
    let dir = std::env::var_os("LOCAL_IMAGE_RESTORATION_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".cache/local-image-test-models/restoration"));
    let cfg = li_ai::browser::Config::default();
    if mode == "download" {
        let net = li_ai::browser::HttpNet::new(cfg.clone());
        for item in li_ai::browser::restoration_items() {
            let plan = li_ai::browser::plan(&item, &item.files, Some(restoration::policy()), &Default::default(), &cfg, &net)?;
            li_ai::browser::install(&plan, &dir, &cfg, &JobControl::new(), &|done, total, file| {
                if done == total {
                    eprintln!("Verified {file}: {done} bytes");
                }
            })?;
        }
        println!("Verified restoration test models: {}", dir.display());
        return Ok(());
    }
    if mode != "run" {
        bail!("expected download or run");
    }
    let host = std::env::var("LOCAL_IMAGE_RESTORATION_COMFY").unwrap_or_else(|_| "127.0.0.1:8188".into());
    let client = ComfyClient::new(&host);
    let stats = client.system_stats()?;
    let devices = stats["devices"].as_array().context("No ComfyUI GPU devices")?;
    if !devices.iter().any(|d| d["name"].as_str().is_some_and(|n| n.contains("5090")) && d["type"] == "cuda") {
        bail!("Real CUDA RTX 5090 required, got {devices:?}");
    }
    println!("GPU {devices:?}");
    let mut settings = li_ai::AiSettings::load();
    settings.model_directory = dir.to_string_lossy().into();
    settings.comfy_port = host.rsplit(':').next().context("port")?.parse()?;
    settings.comfy_host = "127.0.0.1".into();
    let clean = fixture();
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/restoration-real");
    std::fs::create_dir_all(&artifacts)?;
    clean.save(artifacts.join("clean.png"))?;
    for task in [Task::Jpeg, Task::Scan, Task::Denoise] {
        if !li_ai::refine::queue_idle(&client)? {
            bail!("ComfyUI queue is busy; no job submitted");
        }
        let degraded = if task == Task::Jpeg { jpeg(&clean)? } else { noise(&clean) };
        degraded.save(artifacts.join(format!("{task:?}-input.png")))?;
        let started = Instant::now();
        let result = restoration::restore(&client, &settings, task, &degraded, &JobControl::new())?;
        let cold_ms = started.elapsed().as_secs_f64() * 1000.0;
        let input_psnr = psnr(&clean, &degraded);
        let output_psnr = psnr(&clean, &result);
        result.save(artifacts.join(format!("{task:?}-output.png")))?;
        let mut samples = Vec::new();
        for sample in 0..3 {
            if !li_ai::refine::queue_idle(&client)? {
                bail!("ComfyUI queue became busy");
            }
            let started = Instant::now();
            let mut fresh = degraded.clone();
            fresh.get_pixel_mut(0, 0)[0] = fresh.get_pixel(0, 0)[0].saturating_add(sample + 1);
            let warm = restoration::restore(&client, &settings, task, &fresh, &JobControl::new())?;
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(warm.dimensions(), degraded.dimensions());
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "RESTORATION {:?} size={}x{} cold_ms={:.2} warm_median_ms={:.2} input_psnr={:.4} output_psnr={:.4} gain_db={:.4}",
            task,
            clean.width(),
            clean.height(),
            cold_ms,
            samples[1],
            input_psnr,
            output_psnr,
            output_psnr - input_psnr
        );
    }
    Ok(())
}

/// Analytic colour gradients, edges and texture; a synthetic regression probe, not a dataset score.
fn fixture() -> RgbaImage {
    RgbaImage::from_fn(256, 192, |x, y| {
        let q = |v: f64| (v.clamp(0.0, 255.0).round()) as u8;
        let edge = if (50..180).contains(&x) && (40..140).contains(&y) { 35.0 } else { 0.0 };
        let wave = 12.0 * (x as f64 / 12.0).sin() * (y as f64 / 14.0).cos();
        Rgba([q(40.0 + x as f64 * 0.6 + wave), q(30.0 + y as f64 * 0.8 + edge), q(80.0 + x as f64 * 0.3 + y as f64 * 0.2 - wave), 255])
    })
}
fn jpeg(clean: &RgbaImage) -> Result<RgbaImage> {
    let rgb = image::DynamicImage::ImageRgba8(clean.clone()).to_rgb8();
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 15).encode_image(&rgb)?;
    Ok(image::load_from_memory(&bytes)?.to_rgba8())
}
fn noise(clean: &RgbaImage) -> RgbaImage {
    let mut state = 0x325a1915_u32;
    RgbaImage::from_fn(clean.width(), clean.height(), |x, y| {
        let mut pixel = *clean.get_pixel(x, y);
        for k in 0..3 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = (state >> 24) as i32 - 128;
            pixel[k] = (pixel[k] as i32 + n / 5).clamp(0, 255) as u8;
        }
        pixel
    })
}
fn psnr(a: &RgbaImage, b: &RgbaImage) -> f64 {
    let squared: f64 = a.pixels().zip(b.pixels()).flat_map(|(a, b)| (0..3).map(move |k| (a[k] as f64 - b[k] as f64).powi(2))).sum();
    let mse = squared / (a.width() as f64 * a.height() as f64 * 3.0);
    10.0 * (255.0_f64.powi(2) / mse.max(f64::EPSILON)).log10()
}
