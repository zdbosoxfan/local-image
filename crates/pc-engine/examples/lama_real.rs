//! Actual command/undo timing, with CC0 input and model path supplied by the caller.
//! Usage: LOCAL_IMAGE_SEG_MODELS=target/lama/models cargo run ... --example lama_real -- IMAGE OUTPUT
use photocraft_engine::Session;
use serde_json::json;
use std::{path::Path, time::Instant};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!((3..=5).contains(&args.len()), "IMAGE OUTPUT [MASK] [PASSES=2]");
    let input = image::open(&args[1])?.to_rgba8();
    let (w, h) = input.dimensions();
    // The baseline must use the app's JPEG decoder; reference decoders can differ by a level.
    input.save(Path::new(&args[2]).with_extension("before.png"))?;
    let passes = args.get(4).map(|v| v.parse::<usize>()).transpose()?.unwrap_or(2);
    anyhow::ensure!(passes <= 10, "at most ten benchmark passes");
    if passes == 0 {
        return Ok(());
    }
    let format = photocraft_color::PixelFormat::RGBA8;
    let mut doc = photocraft_doc::Document::new(
        "CC0 LaMa evaluation",
        photocraft_geom::Size::new(w, h),
        photocraft_color::ColorMode::Rgb,
        photocraft_color::SampleType::U8,
    );
    let mut layer = photocraft_doc::Layer::raster("Reference", format);
    if let photocraft_doc::LayerContent::Raster(surface) = &mut layer.content {
        *surface = photocraft_raster::Surface::from_interleaved(format, photocraft_geom::Rect::from_xywh(0, 0, w, h), input.as_raw());
    }
    doc.layers.push(layer);
    let mut session = Session::new();
    session.add_document(doc, None);
    for pass in 0..passes {
        let before = session.active().unwrap().doc.clone();
        let start = Instant::now();
        let params = if let Some(path) = args.get(3) {
            let mask = image::open(path)?.to_luma8();
            anyhow::ensure!(mask.dimensions() == (w, h), "mask dimensions differ");
            json!({"engine":"lama", "mask":mask.into_raw()})
        } else {
            json!({"engine":"lama","points":[[w as f64/2.,h as f64/2.,1.]],"size":w.min(h) as f64/8.,"hardness":100})
        };
        let result = session.execute("ai.remove", params)?;
        let seconds = start.elapsed().as_secs_f64();
        let removed = session.active().unwrap().doc.clone();
        anyhow::ensure!(removed.layers.len() == before.layers.len() + 1, "no removal layer");
        session.execute("edit.undo", json!({}))?;
        anyhow::ensure!(session.active().unwrap().doc == before, "undo did not restore source");
        session.execute("edit.redo", json!({}))?;
        anyhow::ensure!(session.active().unwrap().doc == removed, "redo did not restore patch");
        let flat = photocraft_compose::flatten(&removed);
        let output = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba(flat.px[(y * w + x) as usize].map(|v| (v.clamp(0., 1.) * 255.).round() as u8)));
        output.save(Path::new(&args[2]).with_extension(format!("{pass}.png")))?;
        println!("actual ai.remove pass={pass} {w}x{h} seconds={seconds:.3} result={result} undo_redo=passed");
        session.execute("edit.undo", json!({}))?;
    }
    Ok(())
}
