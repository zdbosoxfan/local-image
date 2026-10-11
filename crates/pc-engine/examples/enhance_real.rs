//! Opt-in timing of the actual editor command: compose, infer, restore alpha, open document.
//! Uses only in-memory documents and a separately configured test ComfyUI.
use photocraft_engine::Session;
use serde_json::json;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let host = std::env::var("LOCAL_IMAGE_COMFY")?;
    let stats = li_ai::ComfyClient::new(&host).system_stats()?;
    anyhow::ensure!(
        stats["devices"].as_array().is_some_and(|devices| devices.iter().any(|d| d["name"].as_str().is_some_and(|n| n.contains("RTX 5090")))),
        "real RTX 5090 required"
    );
    let queue = li_ai::ComfyClient::new(&host).queue_state()?;
    anyhow::ensure!(["queue_running", "queue_pending"].iter().all(|k| queue[k].as_array().is_some_and(Vec::is_empty)), "ComfyUI is busy");
    li_ai::set_service_override(Some(host));
    let method = std::env::var("ENHANCE_METHOD").unwrap_or_else(|_| "faithful".into());
    let (width, height) = if std::env::args().any(|s| s == "24mp") { (6000, 4000) } else { (192, 128) };
    let mut session = Session::new();
    if let Ok(source) = std::env::var("ENHANCE_INPUT") {
        let image = image::imageops::resize(&image::open(source)?.to_rgba8(), width, height, image::imageops::FilterType::Lanczos3);
        let format = photocraft_color::PixelFormat::RGBA8;
        let mut doc = photocraft_doc::Document::new(
            "CC0 evaluation",
            photocraft_geom::Size::new(width, height),
            photocraft_color::ColorMode::Rgb,
            photocraft_color::SampleType::U8,
        );
        let mut layer = photocraft_doc::Layer::raster("Reference", format);
        if let photocraft_doc::LayerContent::Raster(surface) = &mut layer.content {
            *surface = photocraft_raster::Surface::from_interleaved(format, photocraft_geom::Rect::from_xywh(0, 0, width, height), image.as_raw());
        }
        doc.layers.push(layer);
        session.add_document(doc, None);
    } else {
        session.execute("file.new", json!({"width": width, "height": height}))?;
        session.execute("edit.fill", json!({"color": "#436690"}))?;
    }
    let start = Instant::now();
    let output = session.execute("ai.enhance", json!({"method": method, "backend": "gpu", "scale": 2.0, "seed": 1}))?;
    let seconds = start.elapsed().as_secs_f64();
    let doc = &session.active().ok_or_else(|| anyhow::anyhow!("missing output document"))?.doc;
    anyhow::ensure!(doc.size.width == width * 2 && doc.size.height == height * 2, "incorrect dimensions");
    println!("actual ai.enhance {method}: {width}x{height} -> {}x{}, {seconds:.3}s, {output}", doc.size.width, doc.size.height);
    li_ai::set_service_override(None);
    Ok(())
}
