//! CC0 reconstruction bake-off: app context crops versus whole-photo square resize.
//! Usage: lama_eval MODEL OUTPUT_DIR IMAGE... (all output stays under target/).
use image::{GrayImage, Luma, RgbImage, imageops};
use std::{path::Path, time::Instant};
fn metric(src: &RgbImage, out: &RgbImage, mask: &GrayImage) -> (f64, f64, usize) {
    let mut squared = 0.;
    let mut abs = 0.;
    let mut count = 0;
    let mut outside = 0;
    for ((a, b), m) in src.pixels().zip(out.pixels()).zip(mask.pixels()) {
        if m[0] > 0 {
            for c in 0..3 {
                let d = a[c] as f64 - b[c] as f64;
                squared += d * d;
                abs += d.abs();
                count += 1;
            }
        } else if a != b {
            outside += 1;
        }
    }
    (10. * (255. * 255. / (squared / count as f64).max(1e-12)).log10(), abs / count as f64, outside)
}
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(args.len() >= 4, "MODEL OUTPUT_DIR IMAGE...");
    let root = Path::new(&args[2]);
    std::fs::create_dir_all(root)?;
    let start = Instant::now();
    let model = li_seg::lama::Lama::from_path(Path::new(&args[1]))?;
    println!("load_s={:.3}", start.elapsed().as_secs_f64());
    let mut csv = String::from("image,method,seconds,masked_psnr_db,masked_mae_255,outside_changed\n");
    for file in &args[3..] {
        let name = Path::new(file).file_stem().unwrap().to_string_lossy();
        let original = image::open(file)?.to_rgb8();
        let k = (1600. / original.width().max(original.height()) as f64).min(1.);
        let src = imageops::resize(
            &original,
            (original.width() as f64 * k).round() as u32,
            (original.height() as f64 * k).round() as u32,
            imageops::FilterType::Lanczos3,
        );
        let (w, h) = src.dimensions();
        // Deterministic holdout over textured central content, with known clean ground truth.
        let cx = w / 2;
        let cy = h / 2;
        let mask = GrayImage::from_fn(w, h, |x, y| Luma([if x.abs_diff(cx) < w.min(h) / 16 && y.abs_diff(cy) < w.min(h) / 12 { 255 } else { 0 }]));
        src.save(root.join(format!("{name}-original.png")))?;
        mask.save(root.join(format!("{name}-mask.png")))?;
        let before = RgbImage::from_fn(w, h, |x, y| if mask.get_pixel(x, y)[0] > 0 { image::Rgb([0, 0, 0]) } else { *src.get_pixel(x, y) });
        before.save(root.join(format!("{name}-before.png")))?;
        let start = Instant::now();
        let cropped = model.run(&before, &mask, &|| Ok(()), &|_, _| {})?;
        let seconds = start.elapsed().as_secs_f64();
        let (psnr, mae, outside) = metric(&src, &cropped, &mask);
        cropped.save(root.join(format!("{name}-crop.png")))?;
        csv.push_str(&format!("{name},context-crop,{seconds:.3},{psnr:.3},{mae:.3},{outside}\n"));
        let rgb = imageops::resize(&before, 512, 512, imageops::FilterType::Lanczos3);
        let hard = imageops::resize(&mask, 512, 512, imageops::FilterType::Nearest);
        let start = Instant::now();
        let square = model.run(&rgb, &hard, &|| Ok(()), &|_, _| {})?;
        let resized = imageops::resize(&square, w, h, imageops::FilterType::Lanczos3);
        let baseline = RgbImage::from_fn(w, h, |x, y| if mask.get_pixel(x, y)[0] > 0 { *resized.get_pixel(x, y) } else { *src.get_pixel(x, y) });
        let seconds = start.elapsed().as_secs_f64();
        let (psnr, mae, outside) = metric(&src, &baseline, &mask);
        baseline.save(root.join(format!("{name}-square.png")))?;
        csv.push_str(&format!("{name},whole-square,{seconds:.3},{psnr:.3},{mae:.3},{outside}\n"));
        println!("{name} done");
    }
    std::fs::write(root.join("metrics.csv"), &csv)?;
    print!("{csv}");
    Ok(())
}
