//! Local photographic evaluation through the app's decoder and real Develop pipeline.
//! cargo run -p lightcraft-engine --example matchlook_photographs -- REF OUTPUT TARGET...
//! Originals are read only. Output includes images, editable settings, timing and diagnostics;
//! distribution errors are diagnostics, never a photographic quality acceptance threshold.
use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{RenderRequest, match_look, render};
use lightcraft_raster::Rgba8;
use serde_json::json;
use std::{error::Error, path::Path, sync::Arc};
fn save(path: &Path, image: &Rgba8) -> Result<(), Box<dyn Error>> {
    std::fs::write(path, lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(image), &Default::default())?)?;
    Ok(())
}
fn clipping(image: &Rgba8) -> f64 {
    image.data.iter().filter(|p| p[..3].iter().any(|v| *v == 255)).count() as f64 / image.data.len().max(1) as f64
}
fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 3 {
        return Err("usage: matchlook_photographs REF OUTPUT TARGET...".into());
    }
    lightcraft_sysmem::init(Default::default());
    let out = Path::new(&args[1]);
    std::fs::create_dir_all(out)?;
    let base = DevelopSettings::default();
    let (source, info) = lightcraft_engine::files::load_vec(std::fs::read(&args[0])?, 1536)?;
    let source = Arc::new(source);
    let reference = match_look::analyze(&source, &info, &base);
    let req = RenderRequest::fit(1024, 1024);
    save(&out.join("reference.png"), &render(&source, &info, &base, &req).image)?;
    let mut cases = Vec::new();
    for (index, path) in args[2..].iter().enumerate() {
        let started = std::time::Instant::now();
        let loaded = lightcraft_engine::files::load_vec(std::fs::read(path)?, 1536);
        let (image, info) = match loaded {
            Ok(value) => value,
            Err(e) => {
                cases.push(json!({"file":path,"error":e}));
                println!("FAILED {path}: {e}");
                continue;
            }
        };
        let source = Arc::new(image);
        let load_ms = started.elapsed().as_millis();
        let before = render(&source, &info, &base, &req).image;
        save(&out.join(format!("{index:02}-before.png")), &before)?;
        let started = std::time::Instant::now();
        let opts = match_look::Options { appearance_aware: std::env::var_os("LOCAL_IMAGE_MATCHLOOK_APPEARANCE").is_some(), ..Default::default() };
        let solved = match_look::solve(&source, &info, &base, std::slice::from_ref(&reference), opts, &|| false).ok_or("solve failed")?;
        let solve_ms = started.elapsed().as_millis();
        let mut results = Vec::new();
        for strength in [65.0, 100.0] {
            let settings = match_look::amount_settings(&base, &solved.full_settings, match_look::Options { strength, ..opts });
            let after = render(&source, &info, &settings, &req).image;
            save(&out.join(format!("{index:02}-after-{}.png", strength as usize)), &after)?;
            std::fs::write(out.join(format!("{index:02}-settings-{}.json", strength as usize)), serde_json::to_vec_pretty(&settings)?)?;
            results.push(json!({"strength":strength,"clippedChannelFraction":clipping(&after)}));
        }
        let max_input = source.data.iter().flatten().copied().filter(|v| v.is_finite()).fold(0.0f32, f32::max);
        cases.push(json!({"file":path,"index":index,"sensorRaw":info.raw,"relativeWb":info.relative_wb,
            "linearSourceMax":max_input,"sourceWidth":source.width,"sourceHeight":source.height,"loadMs":load_ms,"solveMs":solve_ms,
            "renders":solved.renders,"similarity":solved.similarity,
            "referenceLightness":reference.features[..5],"goalLightness":solved.goal.features[..5],"beforeDistributionError":solved.before_error,
            "afterDistributionError":solved.after_error,"beforeClippedChannelFraction":clipping(&before),"results":results}));
        println!("{index:02} raw={} max={max_input:.3} load={load_ms}ms solve={solve_ms}ms renders={} {path}", info.raw, solved.renders);
        std::fs::write(out.join("evaluation.json"), serde_json::to_vec_pretty(&json!({"reference":args[0],"cases":cases}))?)?;
    }
    std::fs::write(out.join("evaluation.json"), serde_json::to_vec_pretty(&json!({"reference":args[0],"cases":cases}))?)?;
    Ok(())
}
