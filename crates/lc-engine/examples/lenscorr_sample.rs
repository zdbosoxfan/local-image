//! Small embedded-lens measurement sample through the application's loader and
//! Develop bridge. Writes private/CC0 results only under target/, never fixtures.
use std::{path::Path, sync::Arc, time::Instant};

use lightcraft_develop::DevelopSettings;
use lightcraft_engine::{
    export::{ExportFormat, ExportOptions, encode_image},
    files, media,
};
use lightcraft_pipeline::{RenderRequest, StageCache};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("expected input raw and target/ output prefix")?;
    let output = args.next().ok_or("expected target/ output prefix")?;
    let target = std::fs::canonicalize("target")?;
    let parent = Path::new(&output).parent().ok_or("missing output directory")?;
    std::fs::create_dir_all(parent)?;
    if !std::fs::canonicalize(parent)?.starts_with(target) {
        return Err("outputs must be under target/".into());
    }
    let bytes = std::fs::read(input)?;
    let header = lightcraft_raw::probe_info(&bytes)?;
    let lens = files::embedded_lens(&header);
    let (w, h) = header.developed_size();
    std::fs::write(
        format!("{output}-profile.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"width": w, "height": h, "vendor": header.vendor_lens, "lens": lens}))?,
    )?;
    let source_edge = std::env::var("LENSCORR_SOURCE_EDGE").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let (source, info) = files::load_bytes(&bytes, source_edge)?;
    assert_eq!(info.lens, lens, "probe and load must agree");
    let source = Arc::new(source);
    let req = RenderRequest::fit(1920, 1920);
    let mut s = DevelopSettings::default();
    let options = ExportOptions { format: ExportFormat::Png, ..Default::default() };
    let mut enabled = None;
    for on in [false, true] {
        s.optics.lens_profile = on;
        let rendered = media::develop(&source, &info, &s, &req, None, false);
        let suffix = if on { "on" } else { "off" };
        std::fs::write(format!("{output}-{suffix}.png"), encode_image(&rendered.image, &options)?)?;
        if on {
            enabled = Some(rendered.image);
        }
    }
    // Two actual slider renders, one warm cache. These measurements are a small
    // sample, never a benchmark/acceptance sweep or a hardware tuning choice.
    if source_edge <= 1920 && lightcraft_engine::gpu::available() {
        let adapter = lightcraft_engine::gpu::adapter_name().unwrap_or_default();
        if adapter.to_lowercase().contains("llvmpipe") {
            return Err("hardware GPU required".into());
        }
        let cache = StageCache::default();
        for amount in [100.0, 99.0] {
            s.optics.profile_distortion = amount;
            let start = Instant::now();
            let rendered = media::develop(&source, &info, &s, &req, Some(&cache), true);
            if let Some(reason) = lightcraft_gpu::last_fallback() {
                return Err(format!("GPU fallback: {reason}").into());
            }
            println!(
                "{adapter}: app Develop profile {amount}% {}x{} {:.3} ms",
                rendered.image.width,
                rendered.image.height,
                start.elapsed().as_secs_f64() * 1000.0
            );
            if amount == 100.0 {
                let cpu = enabled.as_ref().ok_or("missing CPU render")?;
                let max = cpu.data.iter().zip(&rendered.image.data).flat_map(|(a, b)| (0..3).map(move |ch| a[ch].abs_diff(b[ch]))).max().unwrap_or(0);
                println!("app CPU/GPU maximum RGB difference {max} LSB");
                std::fs::write(format!("{output}-gpu.png"), encode_image(&rendered.image, &options)?)?;
            }
        }
    }
    println!("developed {w}x{h}; embedded {}", lens.is_some());
    Ok(())
}
