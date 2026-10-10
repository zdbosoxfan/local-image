//! Real-model source-stage inference and wavelet comparison. All writes stay under target or a
//! supplied test output folder. Run sequentially; model copies use the approved test cache.
use li_seg::denoise::Variant;
use lightcraft_engine::enhance::denoise::{infer, pack_bayer, prepare_raw};
use lightcraft_raster::{
    Rgb32f,
    resample::{Filter, resize},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|v| v == "model") {
        let dir = Path::new(args.get(2).ok_or("models directory required")?);
        let kind = match args.get(3).map(String::as_str) {
            Some("linear") => Variant::Linear,
            Some("bayer") => Variant::Bayer,
            Some("rgb") => Variant::Rgb,
            _ => return Err("model variant must be linear, bayer or rgb".into()),
        };
        let path = li_seg::companion_path(dir, kind.file());
        let input = vec![0.1; kind.channels() * kind.tile() * kind.tile()];
        let start = Instant::now();
        let values = if args.get(4).is_some_and(|v| v == "gpu") {
            let d = lightcraft_gpu::denoise::Denoiser::load(&path, kind)?;
            println!("resident GPU bytes {}", d.resident_bytes());
            d.run_tile(&input)?
        } else {
            li_seg::denoise::Denoiser::load_variant(&path, kind).map_err(|e| format!("{e:#}"))?.run_tile(&input).map_err(|e| format!("{e:#}"))?
        };
        println!(
            "{kind:?} load + inference {:.3}s, {} finite samples",
            start.elapsed().as_secs_f64(),
            values.iter().filter(|v| v.is_finite()).count()
        );
        return Ok(());
    }
    let source = PathBuf::from(args.get(1).ok_or("raw path required")?);
    let dir = PathBuf::from(args.get(2).ok_or("models directory required")?);
    let backend = args.get(3).map(String::as_str).unwrap_or("gpu");
    let gpu = backend == "gpu";
    let megapixels = args.get(4).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let out = PathBuf::from(args.get(5).map(String::as_str).unwrap_or("target/ai-denoise"));
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&source).map_err(|e| e.to_string())?;
    if args.get(6).is_some_and(|v| v == "wavelet-only") {
        let (linear, info) = lightcraft_engine::files::load_bytes(&bytes, usize::MAX)?;
        let mut settings = lightcraft_develop::DevelopSettings::default();
        settings.detail.nr_luminance = 50.;
        settings.detail.nr_color = 25.;
        let req = lightcraft_pipeline::RenderRequest::fit(linear.width, linear.height);
        let start = Instant::now();
        let result = lightcraft_engine::media::develop(&Arc::new(linear), &info, &settings, &req, None, gpu);
        println!("{backend} wavelet full Develop path {:.3}s {}x{}", start.elapsed().as_secs_f64(), result.image.width, result.image.height);
        return Ok(());
    }
    let raw = prepare_raw(&bytes)?;
    let norm = raw.normalized().map_err(|e| e.to_string())?;
    let (mut packed, mut w, mut h, _) = pack_bayer(&norm)?;
    if megapixels > 0 {
        let ratio = (megapixels as f64 * 1e6 / (w * h * 4) as f64).sqrt();
        let (nw, nh) = ((w as f64 * ratio).round() as usize, (h as f64 * ratio).round() as usize);
        let mut scaled = vec![0.; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                let sy = (y * h / nh).min(h - 1);
                let sx = (x * w / nw).min(w - 1);
                scaled[(y * nw + x) * 4..(y * nw + x) * 4 + 4].copy_from_slice(&packed[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4]);
            }
        }
        packed = scaled;
        w = nw;
        h = nh;
    }
    let start = Instant::now();
    let pixels = infer(&dir, Variant::Bayer, &packed, w, h, gpu, &|done, total| {
        if done == total || done % 20 == 0 {
            eprintln!("{backend} {done}/{total} {:.1}s", start.elapsed().as_secs_f64());
        }
        true
    })?;
    let secs = start.elapsed().as_secs_f64();
    println!("{backend} Bayer {}x{} {:.3}MP total {secs:.3}s", w * 2, h * 2, (w * h * 4) as f64 / 1e6);
    let img = Rgb32f { width: w * 2, height: h * 2, data: pixels };
    write_float(&out.join(format!("bayer-{backend}-{}mp.bin", (w * h * 4) / 1000000)), &img)?;
    if megapixels == 0 {
        let (linear, info) = lightcraft_engine::files::load_bytes(&bytes, usize::MAX)?;
        let linear = Arc::new(linear);
        let mut settings = lightcraft_develop::DevelopSettings::default();
        settings.detail.nr_luminance = 50.;
        settings.detail.nr_color = 25.;
        let req = lightcraft_pipeline::RenderRequest::fit(linear.width, linear.height);
        let start = Instant::now();
        let _ = lightcraft_engine::media::develop(&linear, &info, &settings, &req, None, gpu);
        println!("{backend} wavelet full Develop path {:.3}s", start.elapsed().as_secs_f64());
        if args.get(6).is_some_and(|v| v == "bayer-only") {
            return Ok(());
        }
        let start = Instant::now();
        let den = infer(&dir, Variant::Linear, linear.data.as_flattened(), linear.width, linear.height, gpu, &|_, _| true)?;
        println!("{backend} linear {}x{} inference {:.3}s", linear.width, linear.height, start.elapsed().as_secs_f64());
        write_float(&out.join(format!("linear-{backend}.bin")), &Rgb32f { width: linear.width, height: linear.height, data: den })?;
        let small = resize(&linear, 1024, ((linear.height as f64 / linear.width as f64) * 1024.).round() as usize, Filter::Box);
        let start = Instant::now();
        let _ = infer(&dir, Variant::Linear, small.data.as_flattened(), small.width, small.height, gpu, &|_, _| true)?;
        println!("{backend} 1024px preview {:.3}s", start.elapsed().as_secs_f64());
    }
    if gpu {
        println!("adapter {}", lightcraft_gpu::adapter_name().unwrap_or_default());
    }
    Ok(())
}
fn write_float(path: &Path, img: &Rgb32f) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    f.write_all(&(img.width as u32).to_le_bytes()).and_then(|_| f.write_all(&(img.height as u32).to_le_bytes())).map_err(|e| e.to_string())?;
    for p in &img.data {
        for v in p {
            f.write_all(&v.to_le_bytes()).map_err(|e| e.to_string())?;
        }
    }
    f.flush().map_err(|e| e.to_string())?;
    Ok(())
}
