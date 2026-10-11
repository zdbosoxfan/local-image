//! CC0 GB82 comparison. Metrics follow BSRGAN's published scoring code: luminance PSNR,
//! scale-pixel border crop and valid 11×11 Gaussian SSIM (sigma 1.5, K1 .01, K2 .03).
use anyhow::{Context, Result, ensure};
use image::{RgbaImage, imageops};
use li_ai::{
    Ai, JobControl,
    enhance::{
        Backend, Method,
        selection::{self, Choice},
    },
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

fn y(p: &image::Rgba<u8>) -> f64 {
    16.0 + (65.481 * p[0] as f64 + 128.553 * p[1] as f64 + 24.966 * p[2] as f64) / 255.0
}

fn metrics(reference: &RgbaImage, output: &RgbaImage, border: u32) -> Result<(f64, f64)> {
    let margin = border.checked_mul(2).context("metric border overflow")?;
    ensure!(
        reference.dimensions() == output.dimensions() && reference.width().saturating_sub(margin) >= 11 && reference.height().saturating_sub(margin) >= 11,
        "images too small or mismatched for metrics"
    );
    let (w, h) = (reference.width() - 2 * border, reference.height() - 2 * border);
    let count = (w as usize).checked_mul(h as usize).filter(|n| *n <= 400_000_000).context("metric dimensions too large")?;
    let mut fields = vec![[0.0; 5]; count];
    let mut mse = 0.0;
    for row in 0..h {
        for col in 0..w {
            let a = y(reference.get_pixel(col + border, row + border));
            let b = y(output.get_pixel(col + border, row + border));
            mse += (a - b).powi(2);
            fields[(row * w + col) as usize] = [a, b, a * a, b * b, a * b];
        }
    }
    let psnr = 10.0 * (255.0f64.powi(2) / (mse / (w * h) as f64).max(1e-12)).log10();
    let mut kernel: Vec<_> = (-5..=5).map(|i| (-(i * i) as f64 / (2.0 * 1.5f64.powi(2))).exp()).collect();
    let sum: f64 = kernel.iter().sum();
    kernel.iter_mut().for_each(|v| *v /= sum);
    let mut horizontal = fields.clone();
    for row in 0..h as usize {
        for col in 5..w as usize - 5 {
            let dst = &mut horizontal[row * w as usize + col];
            *dst = [0.0; 5];
            for (i, weight) in kernel.iter().enumerate() {
                for c in 0..5 {
                    dst[c] += weight * fields[row * w as usize + col + i - 5][c];
                }
            }
        }
    }
    let mut ssim = 0.0;
    for row in 5..h as usize - 5 {
        for col in 5..w as usize - 5 {
            let mut v = [0.0; 5];
            for (i, weight) in kernel.iter().enumerate() {
                for c in 0..5 {
                    v[c] += weight * horizontal[(row + i - 5) * w as usize + col][c];
                }
            }
            let [a, b, aa, bb, ab] = v;
            let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
            ssim += ((2.0 * a * b + c1) * (2.0 * (ab - a * b) + c2)) / ((a * a + b * b + c1) * (aa - a * a + bb - b * b + c2));
        }
    }
    Ok((psnr, ssim / ((w - 10) as f64 * (h - 10) as f64)))
}

fn damaged(input: &RgbaImage) -> Result<RgbaImage> {
    let mut out = imageops::blur(input, 0.6);
    // Reproducible Box–Muller normal noise, sigma 10 levels, then JPEG quality 50.
    let mut state = 104729u64;
    let mut uniform = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 32) as f64 + 1.0) / (u32::MAX as f64 + 2.0)
    };
    for pixel in out.pixels_mut() {
        for c in 0..3 {
            let noise = (-2.0 * uniform().ln()).sqrt() * (std::f64::consts::TAU * uniform()).cos() * 10.0;
            pixel[c] = (pixel[c] as f64 + noise).round().clamp(0.0, 255.0) as u8;
        }
    }
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 50).encode_image(&image::DynamicImage::ImageRgba8(out).to_rgb8())?;
    Ok(image::load_from_memory(&bytes)?.to_rgba8())
}

fn rss(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|s| s.lines().find(|s| s.starts_with("VmRSS:")).and_then(|s| s.split_whitespace().nth(1)).and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

struct Memory {
    done: Arc<AtomicBool>,
    rss: Arc<AtomicU64>,
    gpu: Arc<AtomicU64>,
    thread: std::thread::JoinHandle<()>,
}
impl Memory {
    fn start(backend: Backend) -> Self {
        let pid = if backend == Backend::Cpu { std::process::id() } else { std::env::var("ENHANCE_COMFY_PID").ok().and_then(|s| s.parse().ok()).unwrap_or(0) };
        let done = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(rss(pid)));
        let gpu = Arc::new(AtomicU64::new(0));
        let (stop, r, g) = (done.clone(), peak.clone(), gpu.clone());
        let thread = std::thread::spawn(move || {
            let mut last = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                r.fetch_max(rss(pid), Ordering::Relaxed);
                if backend == Backend::Gpu && last.elapsed().as_millis() >= 200 {
                    if let Ok(out) =
                        std::process::Command::new("nvidia-smi").args(["--query-compute-apps=pid,used_gpu_memory", "--format=csv,noheader,nounits"]).output()
                    {
                        for line in String::from_utf8_lossy(&out.stdout).lines() {
                            if let Some((p, v)) = line.split_once(',')
                                && p.trim().parse::<u32>().ok() == Some(pid)
                            {
                                g.fetch_max(v.trim().parse().unwrap_or(0), Ordering::Relaxed);
                            }
                        }
                    }
                    last = Instant::now();
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        Self { done, rss: peak, gpu, thread }
    }
    fn finish(self) -> (u64, u64) {
        self.done.store(true, Ordering::Relaxed);
        let _ = self.thread.join();
        (self.rss.load(Ordering::Relaxed), self.gpu.load(Ordering::Relaxed))
    }
}

pub fn run(ai: &Ai, models: &Path, backend: Backend) -> Result<()> {
    let source = std::path::PathBuf::from(std::env::var("ENHANCE_EVAL")?);
    let factor: u32 = std::env::var("ENHANCE_FACTOR").ok().and_then(|s| s.parse().ok()).unwrap_or(2);
    ensure!(matches!(factor, 2 | 4), "factor must be 2 or 4");
    let candidate = std::env::var("ENHANCE_CANDIDATE").unwrap_or_else(|_| "all".into());
    let clip = li_seg::spec("clip-b32-laion").filter(|s| li_seg::installed_bytes(models, s).is_some()).map(|s| li_seg::Clip::load(models, s)).transpose()?;
    let mut paths: Vec<_> =
        std::fs::read_dir(&source)?.filter_map(|p| p.ok().map(|p| p.path())).filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
    paths.sort();
    let limit: usize = std::env::var("ENHANCE_LIMIT").ok().and_then(|s| s.parse().ok()).unwrap_or(paths.len());
    paths.truncate(limit);
    ensure!(!paths.is_empty(), "empty evaluation set");
    println!("candidate,backend,factor,case,image,seconds,PSNR_Y,SSIM_Y,CLIP_cosine,peak_RSS_KiB,peak_GPU_MiB");
    for id in ["baseline", "realplksr-2x", "realplksr-4x", "bsrgan-2x", "bsrgan-4x", "seedvr2"] {
        if candidate != "all" && candidate != id {
            continue;
        }
        if id == "seedvr2" && backend == Backend::Cpu {
            continue;
        }
        let method = if id == "seedvr2" {
            Method::Generative
        } else if id.starts_with("bsrgan") {
            Method::Rescue
        } else {
            Method::Faithful
        };
        let choice = if id == "seedvr2" { selection::default_choice(method) } else { Choice::Curated { id: id.into() } };
        if backend == Backend::Gpu {
            ai.client.free_memory()?;
        }
        for path in &paths {
            let mut reference = image::open(path)?.to_rgba8();
            if backend == Backend::Cpu
                && let Ok(side) = std::env::var("ENHANCE_CPU_CROP")
            {
                let side: u32 = side.parse()?;
                ensure!(side <= reference.width() && side <= reference.height() && side > 16, "invalid CPU crop");
                reference = imageops::crop_imm(&reference, (reference.width() - side) / 2, (reference.height() - side) / 2, side, side).to_image();
            }
            let clean = imageops::resize(&reference, reference.width() / factor, reference.height() / factor, imageops::FilterType::CatmullRom);
            let reference_embedding = clip
                .as_ref()
                .map(|clip| clip.embed_image(reference.as_raw(), reference.width() as usize, reference.height() as usize, li_seg::Crops::Center))
                .transpose()?;
            let mut cases = vec![("clean", clean.clone()), ("blur_noise_jpeg", damaged(&clean)?)];
            if std::env::var("ENHANCE_CLEAN_ONLY").as_deref() == Ok("1") {
                cases.truncate(1);
            }
            for (case, input) in cases {
                let monitor = Memory::start(backend);
                let start = Instant::now();
                let ctl = JobControl::new();
                let complete = Arc::new(AtomicBool::new(false));
                let timeout: u64 = std::env::var("ENHANCE_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
                let watcher = if timeout > 0 {
                    let (c, done) = (ctl.clone(), complete.clone());
                    Some(std::thread::spawn(move || {
                        while !done.load(Ordering::Relaxed) {
                            if start.elapsed().as_secs() >= timeout {
                                c.cancel();
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                    }))
                } else {
                    None
                };
                let output = if id == "baseline" {
                    Ok(imageops::resize(&input, reference.width(), reference.height(), imageops::FilterType::CatmullRom))
                } else {
                    ai.enhance_with_choice(&input, reference.width(), reference.height(), method, backend, models, 1, &ctl, &choice)
                };
                let elapsed = start.elapsed().as_secs_f64();
                complete.store(true, Ordering::Relaxed);
                if let Some(watcher) = watcher {
                    let _ = watcher.join();
                }
                let (rss, gpu) = monitor.finish();
                if let Err(error) = &output
                    && error.is::<li_ai::Cancelled>()
                {
                    println!("{id},{backend:?},{factor},cancelled_{case},{},{elapsed:.6},NaN,NaN,NaN,{rss},{gpu}", path.file_name().unwrap().to_string_lossy());
                    continue;
                }
                let output = output.with_context(|| format!("{id} {case} {}", path.display()))?;
                let (psnr, ssim) = metrics(&reference, &output, factor)?;
                let cosine = if let (Some(clip), Some(expected)) = (&clip, &reference_embedding) {
                    let embedding = clip.embed_image(output.as_raw(), output.width() as usize, output.height() as usize, li_seg::Crops::Center)?;
                    expected.iter().zip(embedding).map(|(a, b)| *a as f64 * b as f64).sum::<f64>()
                } else {
                    f64::NAN
                };
                println!(
                    "{id},{backend:?},{factor},{case},{},{elapsed:.6},{psnr:.6},{ssim:.8},{cosine:.8},{rss},{gpu}",
                    path.file_name().unwrap().to_string_lossy()
                );
                if let Ok(folder) = std::env::var("ENHANCE_OUTPUT") {
                    let dir = std::path::PathBuf::from(folder).join(format!("{id}-{factor}x-{case}"));
                    std::fs::create_dir_all(&dir)?;
                    output.save(dir.join(path.file_name().unwrap()))?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn ds2_metrics_reject_small_mismatched_images_and_huge_border() {
        let image = RgbaImage::new(8, 8);
        assert!(metrics(&image, &image, 2).is_err());
        assert!(metrics(&image, &image, u32::MAX).is_err());
        assert!(metrics(&RgbaImage::new(32, 32), &image, 2).is_err());
    }

    use super::*;
    #[test]
    fn enhance_evaluation_metrics_match_identity_and_known_error() {
        let a = RgbaImage::from_pixel(32, 32, image::Rgba([128, 128, 128, 255]));
        let b = RgbaImage::from_pixel(32, 32, image::Rgba([138, 138, 138, 255]));
        let (psnr, ssim) = metrics(&a, &a, 2).unwrap();
        assert!(psnr > 100.0 && (ssim - 1.0).abs() < 1e-10);
        let (psnr, ssim) = metrics(&a, &b, 2).unwrap();
        let delta = y(b.get_pixel(0, 0)) - y(a.get_pixel(0, 0));
        assert!((psnr - 20.0 * (255.0 / delta).log10()).abs() < 1e-10);
        assert!(ssim < 1.0 && ssim > 0.9);
    }
}
