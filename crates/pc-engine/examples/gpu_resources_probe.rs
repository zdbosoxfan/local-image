//! Opt-in end-to-end resource measurements with NASA's public-domain astronaut fixture.
//! Runs the real editor adjustment command plus the old/new finishing resizes. No window.
use anyhow::{Context, Result, ensure};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, LayerContent};
use photocraft_engine::Session;
use photocraft_geom::{Rect, Size};
use serde_json::json;
use std::time::Instant;
fn main() -> Result<()> {
    lightcraft_sysmem::init(Default::default());
    let fixture = std::env::args().nth(1).context("pass a test fixture path")?;
    let original = image::open(fixture)?.to_rgba8();
    let profile = lightcraft_sysmem::profile();
    let mut counts = vec![1, 2, profile.threads, profile.hardware.logical];
    counts.sort_unstable();
    counts.dedup();
    for (w, h) in [(257u32, 193u32), (6000, 4000), (10000, 6000)] {
        let source = li_ai::resize::lanczos3(&original, w, h);
        let dw = (w / 4).max(1);
        let dh = (h / 4).max(1);
        let start = Instant::now();
        let serial = image::imageops::resize(&source, dw, dh, image::imageops::FilterType::Lanczos3);
        let serial_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut expected: Option<Vec<u8>> = None;
        for &threads in &counts {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build()?;
            let start = Instant::now();
            let parallel = pool.install(|| li_ai::resize::lanczos3(&source, dw, dh));
            let resize_ms = start.elapsed().as_secs_f64() * 1000.0;
            let diff = serial.as_raw().iter().zip(parallel.as_raw()).filter(|(a, b)| a != b).count();
            ensure!(diff == 0, "finishing resize changed quality");
            let mut doc = Document::new("NASA public-domain reference", Size::new(w, h), ColorMode::Rgb, SampleType::U8);
            let area = Rect::from_xywh(0, 0, w, h);
            let mut layer = Layer::raster("Reference", PixelFormat::RGBA8);
            layer.content = LayerContent::Raster(photocraft_raster::Surface::from_interleaved(PixelFormat::RGBA8, area, source.as_raw()));
            doc.layers.push(layer);
            let mut session = Session::new();
            session.add_document(doc, None);
            let start = Instant::now();
            pool.install(|| session.execute("image.adjustments.invert", json!({})))?;
            let command_ms = start.elapsed().as_secs_f64() * 1000.0;
            let state = session.active().context("missing document")?;
            let surface = state.doc.layers[0].surface().context("missing surface")?;
            let output = surface.to_interleaved(area);
            if let Some(reference) = &expected {
                ensure!(*reference == output, "command differs between thread counts");
            } else {
                expected = Some(output);
            }
            println!(
                "{w}x{h},threads={threads},serial_resize_ms={serial_ms:.2},parallel_resize_ms={resize_ms:.2},command_ms={command_ms:.2},different_bytes={diff}"
            );
        }
    }
    Ok(())
}
